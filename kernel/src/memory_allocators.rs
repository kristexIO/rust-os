use alloc::{boxed::Box, vec, vec::Vec};
use core::{
    alloc::Layout,
    ptr::NonNull,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use spin::Mutex;

/// Memory allocation subsystem.
///
/// This module provides multiple memory allocator implementations suitable for
/// different use cases within the kernel. Each allocator implements the
/// `MemoryAllocator` trait, allowing them to be used interchangeably where
/// appropriate.
///
/// # Allocator Types
///
/// * **Buddy Allocator**: Good for general-purpose allocation with low external
///   fragmentation. Uses power-of-two block sizes and can split/coalesce
///   blocks.
/// * **Slab Allocator**: Optimized for frequent allocation/deallocation of
///   small, same-sized objects (e.g., kernel objects, file descriptors).
/// * **Bump Allocator**: Very fast for allocations that are freed all at once
///   (e.g., temporary buffers during a system call). Does not support
///   individual free.
/// * **Bitmap Allocator**: Manages memory via a bitmap, suitable for fixed-size
///   chunks and simple memory regions.
/// * **Universal Allocator**: Combines multiple allocators to handle different
///   size ranges efficiently.
///
/// # Usage
///
/// Kernel subsystems should allocate memory via the global allocator (set in
/// `allocator.rs`) or by creating instances of these allocators for specific
/// purposes (e.g., a slab allocator for a frequently allocated structure).
///
/// # Custom Allocators
///
/// To create a custom allocator, implement the `MemoryAllocator` trait for your
/// type. Ensure that your implementation is thread-safe (Send + Sync) if it
/// will be used from multiple cores.
///
/// # Safety
///
/// All allocators in this module are designed to be safe for use in concurrent
/// environments, using appropriate synchronization primitives (Mutex, Atomic*).
/// However, callers must still ensure that memory is used correctly (e.g., no
/// use-after-free, no buffer overflows).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AllocatorType {
    /// Buddy system allocator
    Buddy,
    /// Slab allocator
    Slab,
    /// Stack allocator
    Stack,
    /// Pool allocator
    Pool,
    /// Bump pointer allocator
    Bump,
    /// Bitmap allocator
    Bitmap,
}

/// MemoryAllocator trait - интерфейс для всех аллокаторов
#[allow(dead_code)]
pub trait MemoryAllocator: Send + Sync {
    /// Выделить память
    fn allocate(&self, layout: Layout) -> Result<NonNull<u8>, &'static str>;

    /// Освободить память  
    fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) -> Result<(), &'static str>;

    /// Дать статистику
    fn get_stats(&self) -> AllocatorStats;

    /// Поддерживает ли аллокатор realloc
    fn supports_realloc(&self) -> bool;

    /// Изменить размер выделенного блока (если поддерживается)
    fn realloc(
        &self,
        ptr: NonNull<u8>,
        layout: Layout,
        new_size: usize,
    ) -> Result<NonNull<u8>, &'static str>;
}

/// Статистика аллокатора
#[derive(Debug, Clone)]
pub struct AllocatorStats {
    /// Всего выделено байт
    pub allocated_bytes: usize,
    /// Всего освобождено байт
    pub deallocated_bytes: usize,
    /// Текущий объем используемой памяти
    pub current_usage: usize,
    /// Пиковое использование
    pub peak_usage: usize,
    /// Всего операций выделения
    pub allocation_count: usize,
    /// Всего операций освобождения
    pub deallocation_count: usize,
    /// Количество ошибок
    pub error_count: usize,
    /// Размер области управления
    pub overhead: usize,
}

impl AllocatorStats {
    pub fn new() -> Self {
        AllocatorStats {
            allocated_bytes: 0,
            deallocated_bytes: 0,
            current_usage: 0,
            peak_usage: 0,
            allocation_count: 0,
            deallocation_count: 0,
            error_count: 0,
            overhead: 0,
        }
    }
}

/// Buddy System Allocator - аллокатор с эффективной дефрагментацией
#[allow(dead_code)]
pub struct BuddyAllocator {
    /// Базовый адрес области памяти
    base_addr: usize,
    /// Размер области памяти
    size: usize,
    /// Массив флагов занятости фреймов (битовая карта)
    frame_bitmap: Mutex<Vec<u64>>,
    /// Свободные списки для каждого уровня (размера)
    free_lists: Vec<Mutex<Vec<BuddyFrame>>>,
    /// Статистика
    stats: Mutex<AllocatorStats>,
    /// Включена ли защита
    protection_enabled: AtomicBool,
    /// Максимальный уровень (log2(max_size))
    max_order: usize,
}

/// Информация о фрейме в buddy системе
#[derive(Debug, Clone)]
struct BuddyFrame {
    /// Адрес начала фрейма
    addr: usize,
    /// Уровень (order) фрейма - 2^order байт
    order: usize,
}

impl BuddyAllocator {
    /// Создает новый buddy allocator
    pub fn new(base_addr: usize, size: usize) -> Result<Self, &'static str> {
        if size == 0 || !size.is_power_of_two() {
            return Err("Size must be a power of two");
        }

        let max_order = size.trailing_zeros() as usize;
        let frame_count = size / 0x1000; // 4KB как минимальный размер фрейма

        // Определяем размер битовой карты
        let bitmap_size = (frame_count + 63) / 64;

        // Создаем free lists для каждого уровня (order от 0 до max_order)
        let mut free_lists = Vec::with_capacity(max_order + 1);
        for _ in 0..=max_order {
            free_lists.push(Mutex::new(Vec::new()));
        }

        let allocator = BuddyAllocator {
            base_addr,
            size,
            frame_bitmap: Mutex::new(vec![0; bitmap_size]),
            free_lists,
            stats: Mutex::new(AllocatorStats::new()),
            protection_enabled: AtomicBool::new(true),
            max_order,
        };

        // Инициализация: добавляем один большой блок в самый верхний уровень
        allocator.free_lists[max_order].lock().push(BuddyFrame {
            addr: base_addr,
            order: max_order,
        });

        Ok(allocator)
    }

    /// Найти минимальный порядок для размера
    fn get_min_order(size: usize) -> usize {
        let min_frame_size = 0x1000; // 4KB минимальный фрейм
        let needed_size = size.max(min_frame_size);
        let order = needed_size.next_power_of_two().trailing_zeros() as usize;
        order.saturating_sub(12) // 2^12 = 4096
    }

    /// Получить размер фрейма по order
    fn frame_size(order: usize) -> usize {
        1 << (order + 12) // 2^(order + 12) - минимальный размер 4096
    }

    /// Найти buddy фрейм
    fn get_buddy_addr(addr: usize, order: usize, base: usize, total_size: usize) -> Option<usize> {
        let frame_size = Self::frame_size(order);
        let frame_start = (addr - base) / frame_size;

        // Получаем индекс buddy фрейма (XOR с 1)
        let buddy_start = frame_start ^ 1;
        let buddy_addr = base + (buddy_start * frame_size);

        // Проверяем, находится ли buddy внутри диапазона
        if buddy_addr >= base && (buddy_addr - base) < total_size {
            Some(buddy_addr)
        } else {
            None
        }
    }

    /// Отметить фреймы как занятые в битовой карте
    fn set_frames_allocated(&self, start_addr: usize, order: usize) {
        let frame_size = Self::frame_size(0); // Размер минимального фрейма
        let start_frame = (start_addr - self.base_addr) / frame_size;
        let frame_count = 1 << order; // 2^order фреймов

        let mut bitmap = self.frame_bitmap.lock();
        for i in start_frame..start_frame + frame_count {
            let word_idx = i / 64;
            let bit_idx = i % 64;

            if word_idx < bitmap.len() {
                bitmap[word_idx] |= 1 << bit_idx;
            }
        }
    }

    /// Отметить фреймы как свободные в битовой карте
    fn set_frames_free(&self, start_addr: usize, order: usize) {
        let frame_size = Self::frame_size(0);
        let start_frame = (start_addr - self.base_addr) / frame_size;
        let frame_count = 1 << order;

        let mut bitmap = self.frame_bitmap.lock();
        for i in start_frame..start_frame + frame_count {
            let word_idx = i / 64;
            let bit_idx = i % 64;

            if word_idx < bitmap.len() {
                bitmap[word_idx] &= !(1 << bit_idx);
            }
        }
    }

    /// Проверить, все ли фреймы в диапазоне свободны
    fn are_frames_free(&self, start_addr: usize, order: usize) -> bool {
        let frame_size = Self::frame_size(0);
        let start_frame = (start_addr - self.base_addr) / frame_size;
        let frame_count = 1 << order;

        let bitmap = self.frame_bitmap.lock();
        for i in start_frame..start_frame + frame_count {
            let word_idx = i / 64;
            let bit_idx = i % 64;

            if word_idx < bitmap.len() {
                if bitmap[word_idx] & (1 << bit_idx) != 0 {
                    return false; // Найден занятый фрейм
                }
            }
        }
        true
    }
}

impl MemoryAllocator for BuddyAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<u8>, &'static str> {
        let min_order = BuddyAllocator::get_min_order(layout.size());
        if min_order > self.max_order {
            return Err("Requested allocation too large");
        }

        let mut stats = self.stats.lock();

        // Поиск подходящего фрейма, начиная с min_order
        for order in min_order..=self.max_order {
            let mut free_list = self.free_lists[order].lock();

            if let Some(frame) = free_list.pop() {
                // Нашли подходящий фрейм, возможно, нужно разбить
                drop(free_list);

                let current_frame = frame;
                let mut current_order = order;

                // Разбиваем фреймы до нужного размера
                while current_order > min_order {
                    // Разбиваем фрейм на две части
                    let buddy_size = BuddyAllocator::frame_size(current_order - 1);
                    let second_half_addr = current_frame.addr + buddy_size;

                    // Добавляем вторую половину в свободный список
                    self.free_lists[current_order - 1].lock().push(BuddyFrame {
                        addr: second_half_addr,
                        order: current_order - 1,
                    });

                    current_order -= 1;
                }

                // Отмечаем фрейм как занятый
                self.set_frames_allocated(current_frame.addr, current_order);

                // Обновляем статистику
                let size = BuddyAllocator::frame_size(current_order);
                stats.allocated_bytes += size;
                stats.current_usage += size;
                stats.allocation_count += 1;

                if stats.current_usage > stats.peak_usage {
                    stats.peak_usage = stats.current_usage;
                }

                // Возвращаем указатель на выделенную память
                return NonNull::new(current_frame.addr as *mut u8)
                    .ok_or("Failed to create NonNull pointer");
            }
            // Если в текущем уровне нет свободных блоков, пробуем следующий
        }

        // Если не нашли подходящего фрейма
        stats.error_count += 1;
        Err("No suitable memory block found")
    }

    fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) -> Result<(), &'static str> {
        let addr = ptr.as_ptr() as usize;
        let mut stats = self.stats.lock();

        // Проверяем, находится ли указатель в нашей области
        if addr < self.base_addr || addr >= self.base_addr + self.size {
            return Err("Pointer out of allocator range");
        }

        // Вычисляем размер, который был выделен
        let order = BuddyAllocator::get_min_order(layout.size());

        // Находим подходящий фрейм для освобождения
        let frame_addr =
            ((addr - self.base_addr) & !(Self::frame_size(order) - 1)) + self.base_addr;

        // Отмечаем фреймы как свободные
        self.set_frames_free(frame_addr, order);

        // Обновляем статистику
        let size = BuddyAllocator::frame_size(order);
        stats.deallocated_bytes += size;
        stats.current_usage = stats.current_usage.saturating_sub(size);
        stats.deallocation_count += 1;

        // Пытаемся объединить соседние фреймы (дефрагментация)
        let mut current_addr = frame_addr;
        let mut current_order = order;

        while current_order < self.max_order {
            let buddy_addr = BuddyAllocator::get_buddy_addr(
                current_addr,
                current_order,
                self.base_addr,
                self.size,
            );

            if let Some(buddy) = buddy_addr {
                // Проверяем, свободен ли buddy фрейм
                if self.are_frames_free(buddy, current_order) {
                    // Удаляем оба фрейма из списка и объединяем
                    let mut free_list = self.free_lists[current_order].lock();

                    // Убираем buddy из списка, если он там есть
                    if let Some(pos) = free_list.iter().position(|f| f.addr == buddy) {
                        free_list.remove(pos);

                        // Убираем текущий фрейм (если он был добавлен ранее)
                        if let Some(pos) = free_list.iter().position(|f| f.addr == current_addr) {
                            free_list.remove(pos);
                        }

                        // Добавляем объединенный фрейм в следующий уровень
                        let new_addr = current_addr.min(buddy);
                        drop(free_list);

                        self.free_lists[current_order + 1].lock().push(BuddyFrame {
                            addr: new_addr,
                            order: current_order + 1,
                        });

                        current_addr = new_addr;
                        current_order += 1;
                    } else {
                        break; // Buddy не свободен, нельзя объединить
                    }
                } else {
                    break; // Buddy занят, нельзя объединить
                }
            } else {
                break; // Buddy вне диапазона, нельзя объединить
            }
        }

        // Добавляем фрейм в соответствующий свободный список
        self.free_lists[current_order].lock().push(BuddyFrame {
            addr: current_addr,
            order: current_order,
        });

        Ok(())
    }

    fn get_stats(&self) -> AllocatorStats {
        self.stats.lock().clone()
    }

    fn supports_realloc(&self) -> bool {
        true
    }

    fn realloc(
        &self,
        ptr: NonNull<u8>,
        layout: Layout,
        new_size: usize,
    ) -> Result<NonNull<u8>, &'static str> {
        if new_size <= layout.size() {
            // Если новый размер меньше или равен старому, просто возвращаем старый
            // указатель
            return Ok(ptr);
        }

        // Выделяем новую память
        let new_layout = Layout::from_size_align(new_size, layout.align())
            .map_err(|_| "Invalid layout for reallocation")?;
        let new_ptr = self.allocate(new_layout)?;

        // Копируем данные
        let old_size = layout.size().min(new_size);
        unsafe {
            core::ptr::copy_nonoverlapping(ptr.as_ptr(), new_ptr.as_ptr(), old_size);
        }

        // Освобождаем старую память
        let _ = self.deallocate(ptr, layout);

        Ok(new_ptr)
    }
}

/// Slab Allocator для маленьких объектов
pub struct SlabAllocator {
    /// Размер слаба
    slab_size: usize,
    /// Размер объекта
    obj_size: usize,
    /// Количество объектов в слабе
    objs_per_slab: usize,
    /// Список свободных объектов
    free_list: Mutex<Vec<*mut u8>>,
    /// Список выделенных слабов
    allocated_slabs: Mutex<Vec<*mut u8>>,
    /// Статистика
    stats: Mutex<AllocatorStats>,
}

impl SlabAllocator {
    pub fn new(obj_size: usize) -> Result<Self, &'static str> {
        let aligned_size = obj_size
            .max(core::mem::size_of::<usize>())
            .max(8) // Минимум 8 байт для указателей на следующий
            .next_power_of_two(); // Для эффективности выравнивания

        let slab_size = 0x2000; // 8KB слаб
        let objs_per_slab = (slab_size - core::mem::size_of::<usize>()) / aligned_size;

        if objs_per_slab == 0 {
            return Err("Object size too large for slab allocator");
        }

        Ok(SlabAllocator {
            slab_size,
            obj_size: aligned_size,
            objs_per_slab,
            free_list: Mutex::new(Vec::new()),
            allocated_slabs: Mutex::new(Vec::new()),
            stats: Mutex::new(AllocatorStats::new()),
        })
    }
}

// SAFETY: SlabAllocator's raw pointers are only accessed while holding the
// Mutex lock.
unsafe impl Send for SlabAllocator {}
unsafe impl Sync for SlabAllocator {}

impl MemoryAllocator for SlabAllocator {
    fn allocate(&self, _layout: Layout) -> Result<NonNull<u8>, &'static str> {
        let mut free_list = self.free_list.lock();
        let mut stats = self.stats.lock();

        // Проверяем, свободен ли объект в пуле
        if let Some(ptr) = free_list.pop() {
            stats.allocated_bytes += self.obj_size;
            stats.current_usage += self.obj_size;
            stats.allocation_count += 1;

            if stats.current_usage > stats.peak_usage {
                stats.peak_usage = stats.current_usage;
            }

            return NonNull::new(ptr).ok_or("Failed to create NonNull pointer");
        }

        // Так как в тестовой системе мы не можем выделять физическую память,
        // возвращаем ошибку
        stats.error_count += 1;
        Err(
            "Slab allocator needs system allocator to allocate slabs - not implemented in this \
             test version",
        )
    }

    fn deallocate(&self, ptr: NonNull<u8>, _layout: Layout) -> Result<(), &'static str> {
        // Проверяем, принадлежит ли указатель нашему аллокатору
        let ptr_addr = ptr.as_ptr() as usize;

        // Проверяем, находится ли в одном из выделенных слабов
        let allocated_slabs = self.allocated_slabs.lock();
        let mut is_valid = false;

        for &slab_ptr in allocated_slabs.iter() {
            let slab_addr = slab_ptr as usize;
            if ptr_addr >= slab_addr && ptr_addr < slab_addr + self.slab_size {
                if (ptr_addr - slab_addr) % self.obj_size == 0 {
                    is_valid = true;
                    break;
                }
            }
        }

        if !is_valid {
            return Err("Invalid pointer for slab allocator");
        }

        let mut free_list = self.free_list.lock();
        let mut stats = self.stats.lock();

        // Добавляем объект обратно в свободный список
        free_list.push(ptr.as_ptr());

        stats.deallocated_bytes += self.obj_size;
        stats.current_usage = stats.current_usage.saturating_sub(self.obj_size);
        stats.deallocation_count += 1;

        Ok(())
    }

    fn get_stats(&self) -> AllocatorStats {
        self.stats.lock().clone()
    }

    fn supports_realloc(&self) -> bool {
        false // В этом упрощенном слаб-аллокаторе realloc не поддерживается
    }

    fn realloc(
        &self,
        _ptr: NonNull<u8>,
        _layout: Layout,
        _new_size: usize,
    ) -> Result<NonNull<u8>, &'static str> {
        Err("Realloc not supported in slab allocator")
    }
}

/// Bump Pointer Allocator - простой аллокатор для быстрых выделений
pub struct BumpAllocator {
    /// Начальный адрес
    start: usize,
    /// Текущая позиция
    current: AtomicUsize,
    /// Размер области
    size: usize,
    /// Статистика
    stats: Mutex<AllocatorStats>,
}

impl BumpAllocator {
    pub fn new(start: usize, size: usize) -> Self {
        BumpAllocator {
            start,
            current: AtomicUsize::new(start),
            size,
            stats: Mutex::new(AllocatorStats::new()),
        }
    }
}

impl MemoryAllocator for BumpAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<u8>, &'static str> {
        let mut stats = self.stats.lock();

        let current = self.current.load(Ordering::Relaxed);

        // Вычисляем выровненный адрес
        let offset = current.saturating_sub(self.start);
        let aligned_offset = (offset + layout.align() - 1) & !(layout.align() - 1);
        let aligned_addr = self.start + aligned_offset;

        // Проверяем, достаточно ли места
        let end_addr = aligned_addr + layout.size();
        if end_addr > self.start + self.size {
            stats.error_count += 1;
            return Err("Not enough memory in bump allocator");
        }

        // Обновляем указатель
        self.current.store(end_addr, Ordering::Relaxed);

        // Обновляем статистику
        stats.allocated_bytes += layout.size();
        stats.current_usage += layout.size();
        stats.allocation_count += 1;

        if stats.current_usage > stats.peak_usage {
            stats.peak_usage = stats.current_usage;
        }

        NonNull::new(aligned_addr as *mut u8).ok_or("Failed to create NonNull pointer")
    }

    fn deallocate(&self, _ptr: NonNull<u8>, _layout: Layout) -> Result<(), &'static str> {
        // Bump allocator не поддерживает освобождение отдельных блоков
        Err("Bump allocator does not support deallocation")
    }

    fn get_stats(&self) -> AllocatorStats {
        self.stats.lock().clone()
    }

    fn supports_realloc(&self) -> bool {
        false
    }

    fn realloc(
        &self,
        _ptr: NonNull<u8>,
        _layout: Layout,
        _new_size: usize,
    ) -> Result<NonNull<u8>, &'static str> {
        Err("Realloc not supported in bump allocator")
    }
}

/// Bitmap Allocator - аллокатор с битовой картой
pub struct BitmapAllocator {
    /// Базовый адрес
    base: usize,
    /// Размер области
    size: usize,
    /// Размер чанка
    chunk_size: usize,
    /// Число чанков
    num_chunks: usize,
    /// Битмапа (хранит состояние чанков)
    bitmap: Vec<AtomicUsize>,
    /// Последний выделенный чанк (для кругового поиска)
    last_allocated: AtomicUsize,
    /// Статистика
    stats: Mutex<AllocatorStats>,
}

impl BitmapAllocator {
    pub fn new(base: usize, size: usize, chunk_size: usize) -> Result<Self, &'static str> {
        if chunk_size == 0 || !chunk_size.is_power_of_two() {
            return Err("Chunk size must be a power of two");
        }

        let num_chunks = size / chunk_size;
        if num_chunks == 0 {
            return Err("Size too small for given chunk size");
        }

        let bits_per_word = core::mem::size_of::<usize>() * 8;
        let bitmap_size = (num_chunks + bits_per_word - 1) / bits_per_word;

        let mut bitmap = Vec::with_capacity(bitmap_size);
        for _ in 0..bitmap_size {
            bitmap.push(AtomicUsize::new(0));
        }

        Ok(BitmapAllocator {
            base,
            size,
            chunk_size,
            num_chunks,
            bitmap,
            last_allocated: AtomicUsize::new(0),
            stats: Mutex::new(AllocatorStats::new()),
        })
    }

    /// Проверить, свободен ли чанк
    fn is_chunk_free(&self, chunk_idx: usize) -> bool {
        if chunk_idx >= self.num_chunks {
            return false;
        }

        let word_idx = chunk_idx / (core::mem::size_of::<usize>() * 8);
        let bit_idx = chunk_idx % (core::mem::size_of::<usize>() * 8);

        if word_idx < self.bitmap.len() {
            let word = self.bitmap[word_idx].load(Ordering::Relaxed);
            (word & (1 << bit_idx)) == 0
        } else {
            false
        }
    }

    /// Отметить чанк как занятый
    fn set_chunk_allocated(&self, chunk_idx: usize) {
        if chunk_idx >= self.num_chunks {
            return;
        }

        let word_idx = chunk_idx / (core::mem::size_of::<usize>() * 8);
        let bit_idx = chunk_idx % (core::mem::size_of::<usize>() * 8);

        if word_idx < self.bitmap.len() {
            self.bitmap[word_idx].fetch_or(1 << bit_idx, Ordering::Relaxed);
        }
    }

    /// Отметить чанк как свободный
    fn set_chunk_free(&self, chunk_idx: usize) {
        if chunk_idx >= self.num_chunks {
            return;
        }

        let word_idx = chunk_idx / (core::mem::size_of::<usize>() * 8);
        let bit_idx = chunk_idx % (core::mem::size_of::<usize>() * 8);

        if word_idx < self.bitmap.len() {
            self.bitmap[word_idx].fetch_and(!(1 << bit_idx), Ordering::Relaxed);
        }
    }

    /// Найти первый свободный чанк
    fn find_free_chunk(&self) -> Option<usize> {
        let start_idx = self.last_allocated.load(Ordering::Relaxed);

        // Сначала ищем с того места, где последний раз выделяли
        for i in start_idx..self.num_chunks {
            if self.is_chunk_free(i) {
                self.last_allocated.store(i, Ordering::Relaxed);
                return Some(i);
            }
        }

        // Если не нашли с начала до конца, ищем с начала
        for i in 0..start_idx {
            if self.is_chunk_free(i) {
                self.last_allocated.store(i, Ordering::Relaxed);
                return Some(i);
            }
        }

        None // Нет свободных чанков
    }
}

impl MemoryAllocator for BitmapAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<u8>, &'static str> {
        let chunks_needed = layout
            .size()
            .checked_add(self.chunk_size - 1)
            .ok_or("Allocation size overflow")?
            / self.chunk_size;

        if chunks_needed == 0 {
            return Err("Cannot allocate 0 bytes");
        }

        // Ищем последовательность свободных чанков
        for start_chunk in 0..=(self.num_chunks - chunks_needed) {
            let mut all_free = true;

            // Проверяем, все ли чанки в диапазоне свободны
            for offset in 0..chunks_needed {
                if !self.is_chunk_free(start_chunk + offset) {
                    all_free = false;
                    break;
                }
            }

            if all_free {
                // Отмечаем чанки как занятые
                for offset in 0..chunks_needed {
                    self.set_chunk_allocated(start_chunk + offset);
                }

                // Обновляем статистику
                let mut stats = self.stats.lock();
                let size = chunks_needed * self.chunk_size;
                stats.allocated_bytes += size;
                stats.current_usage += size;
                stats.allocation_count += 1;

                if stats.current_usage > stats.peak_usage {
                    stats.peak_usage = stats.current_usage;
                }

                // Возвращаем указатель на выделенную память
                let addr = self.base + (start_chunk * self.chunk_size);
                return NonNull::new(addr as *mut u8).ok_or("Failed to create NonNull pointer");
            }
        }

        // Не найдено подходящего места
        let mut stats = self.stats.lock();
        stats.error_count += 1;
        Err("No suitable memory region found")
    }

    fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) -> Result<(), &'static str> {
        let addr = ptr.as_ptr() as usize;

        // Проверяем, что указатель в наших границах
        if addr < self.base || addr >= self.base + self.size {
            return Err("Pointer out of allocator range");
        }

        // Вычисляем индекс первого чанка
        let chunk_idx = (addr - self.base) / self.chunk_size;
        let chunks_needed = (layout.size() + self.chunk_size - 1) / self.chunk_size;

        // Проверяем, что все чанки принадлежат выделению
        if chunk_idx + chunks_needed > self.num_chunks {
            return Err("Invalid deallocation range");
        }

        // Отмечаем чанки как свободные
        for offset in 0..chunks_needed {
            self.set_chunk_free(chunk_idx + offset);
        }

        // Обновляем статистику
        let mut stats = self.stats.lock();
        let size = chunks_needed * self.chunk_size;
        stats.deallocated_bytes += size;
        stats.current_usage = stats.current_usage.saturating_sub(size);
        stats.deallocation_count += 1;

        Ok(())
    }

    fn get_stats(&self) -> AllocatorStats {
        self.stats.lock().clone()
    }

    fn supports_realloc(&self) -> bool {
        true
    }

    fn realloc(
        &self,
        ptr: NonNull<u8>,
        layout: Layout,
        new_size: usize,
    ) -> Result<NonNull<u8>, &'static str> {
        if new_size <= layout.size() {
            // Если новый размер меньше или равен старому, просто возвращаем старый
            // указатель
            return Ok(ptr);
        }

        // Для Bitmap аллокатора realloc требует выделения нового участка
        let new_layout = Layout::from_size_align(new_size, layout.align())
            .map_err(|_| "Invalid layout for reallocation")?;
        let new_ptr = self.allocate(new_layout)?;

        // Копируем данные
        let old_size = layout.size().min(new_size);
        unsafe {
            core::ptr::copy_nonoverlapping(ptr.as_ptr(), new_ptr.as_ptr(), old_size);
        }

        // Освобождаем старую память
        let _ = self.deallocate(ptr, layout);

        Ok(new_ptr)
    }
}

/// Универсальный аллокатор, который выбирает подходящий аллокатор
pub struct UniversalAllocator {
    /// Маленькие объекты (до 256 байт)
    small_allocator: Mutex<SlabAllocator>,
    /// Средние объекты (257-4096 байт)
    medium_allocator: Mutex<BuddyAllocator>,
    /// Большие объекты (больше 4096 байт)
    large_allocator: Mutex<BuddyAllocator>,
    /// Фоллбэк для экзотических случаев
    fallback_allocator: Mutex<BumpAllocator>,
}

impl UniversalAllocator {
    pub fn new() -> Self {
        UniversalAllocator {
            small_allocator: Mutex::new(SlabAllocator::new(64).unwrap()),
            medium_allocator: Mutex::new(BuddyAllocator::new(0x100000, 0x100000).unwrap()),
            large_allocator: Mutex::new(BuddyAllocator::new(0x200000, 0x400000).unwrap()),
            fallback_allocator: Mutex::new(BumpAllocator::new(0x800000, 0x80000)),
        }
    }
}

impl MemoryAllocator for UniversalAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<u8>, &'static str> {
        if layout.size() <= 256 {
            self.small_allocator.lock().allocate(layout)
        } else if layout.size() <= 4096 {
            self.medium_allocator.lock().allocate(layout)
        } else {
            self.large_allocator.lock().allocate(layout)
        }
    }

    fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) -> Result<(), &'static str> {
        // Для простоты, в универсальном аллокаторе будем использовать тот же способ
        // определения, какой использовался при выделении, но на практике это сложно
        if layout.size() <= 256 {
            self.small_allocator.lock().deallocate(ptr, layout)
        } else if layout.size() <= 4096 {
            self.medium_allocator.lock().deallocate(ptr, layout)
        } else {
            self.large_allocator.lock().deallocate(ptr, layout)
        }
    }

    fn get_stats(&self) -> AllocatorStats {
        // Объединенная статистика всех аллокаторов
        let mut stats = AllocatorStats::new();
        let small_stats = self.small_allocator.lock().get_stats();
        let medium_stats = self.medium_allocator.lock().get_stats();
        let large_stats = self.large_allocator.lock().get_stats();

        stats.allocated_bytes = small_stats.allocated_bytes
            + medium_stats.allocated_bytes
            + large_stats.allocated_bytes;
        stats.deallocated_bytes = small_stats.deallocated_bytes
            + medium_stats.deallocated_bytes
            + large_stats.deallocated_bytes;
        stats.current_usage =
            small_stats.current_usage + medium_stats.current_usage + large_stats.current_usage;
        stats.peak_usage = small_stats
            .peak_usage
            .max(medium_stats.peak_usage)
            .max(large_stats.peak_usage);
        stats.allocation_count = small_stats.allocation_count
            + medium_stats.allocation_count
            + large_stats.allocation_count;
        stats.deallocation_count = small_stats.deallocation_count
            + medium_stats.deallocation_count
            + large_stats.deallocation_count;
        stats.error_count =
            small_stats.error_count + medium_stats.error_count + large_stats.error_count;

        stats
    }

    fn supports_realloc(&self) -> bool {
        true
    }

    fn realloc(
        &self,
        ptr: NonNull<u8>,
        layout: Layout,
        new_size: usize,
    ) -> Result<NonNull<u8>, &'static str> {
        // Для простоты перераспределяем через правильный аллокатор
        let new_layout = Layout::from_size_align(new_size, layout.align())
            .map_err(|_| "Invalid layout for reallocation")?;
        let new_ptr = self.allocate(new_layout)?;

        // Копируем данные
        let old_size = layout.size().min(new_size);
        unsafe {
            core::ptr::copy_nonoverlapping(ptr.as_ptr(), new_ptr.as_ptr(), old_size);
        }

        // Освобождаем старую память
        self.deallocate(ptr, layout)?;

        Ok(new_ptr)
    }
}

impl UniversalAllocator {
    pub fn create_allocator(
        alloc_type: AllocatorType,
        base: usize,
        size: usize,
        obj_size: usize,
    ) -> Result<Box<dyn MemoryAllocator>, &'static str> {
        match alloc_type {
            AllocatorType::Buddy => Ok(Box::new(BuddyAllocator::new(base, size)?)),
            AllocatorType::Slab => Ok(Box::new(SlabAllocator::new(obj_size)?)),
            AllocatorType::Bump => Ok(Box::new(BumpAllocator::new(base, size))),
            AllocatorType::Bitmap => Ok(Box::new(BitmapAllocator::new(base, size, obj_size)?)),
            _ => Err("This allocator type is not implemented"),
        }
    }

    pub fn create_bump_allocator(start: usize, size: usize) -> BumpAllocator {
        BumpAllocator::new(start, size)
    }
}
