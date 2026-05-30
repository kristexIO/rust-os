use alloc::{boxed::Box, vec::Vec};
use core::alloc::Layout;
use spin::Mutex;

/// SLAB Allocator (Slab Allocation)
///
/// Эффективный аллокатор для часто используемых структур фиксированного размера
/// Используется для уменьшения фрагментации и увеличения производительности
pub struct SlabAllocator {
    /// Кэши слабов для разных размеров
    caches: Vec<SlabCache>,
    /// Общий пул памяти
    memory_pool: Vec<u8>,
    /// Свободный список
    free_list: Vec<usize>,
}

impl SlabAllocator {
    /// Создает новый SLAB аллокатор
    pub fn new(pool_size: usize) -> Self {
        SlabAllocator {
            caches: Vec::new(),
            memory_pool: vec![0; pool_size],
            free_list: Vec::new(),
        }
    }

    /// Регистрирует новый кэш слабов для определенного размера
    pub fn register_cache(&mut self, size: usize, count: usize) -> usize {
        let cache_id = self.caches.len();
        let slab_cache = SlabCache::new(size, count);
        self.caches.push(slab_cache);
        cache_id
    }

    /// Выделяет память из соответствующего слаба
    pub fn alloc(&mut self, cache_id: usize) -> Option<*mut u8> {
        if cache_id < self.caches.len() {
            self.caches[cache_id].alloc()
        } else {
            None
        }
    }

    /// Освобождает память обратно в слаб
    pub fn dealloc(&mut self, ptr: *mut u8, cache_id: usize) {
        if cache_id < self.caches.len() {
            self.caches[cache_id].dealloc(ptr);
        }
    }

    /// Получает размер объекта для кэша
    pub fn get_cache_object_size(&self, cache_id: usize) -> Option<usize> {
        if cache_id < self.caches.len() {
            Some(self.caches[cache_id].object_size)
        } else {
            None
        }
    }
}

/// Индивидуальный кэш слабов
struct SlabCache {
    /// Размер объекта
    object_size: usize,
    /// Количество объектов в слабе
    objects_per_slab: usize,
    /// Слабы
    slabs: Vec<Slab>,
    /// Список свободных объектов
    free_objects: Vec<*mut u8>,
    /// Общее количество выделенных объектов
    allocated_count: usize,
}

impl SlabCache {
    /// Создает новый кэш слабов
    fn new(object_size: usize, objects_per_slab: usize) -> Self {
        SlabCache {
            object_size: object_size.max(core::mem::align_of::<usize>()),
            objects_per_slab,
            slabs: Vec::new(),
            free_objects: Vec::new(),
            allocated_count: 0,
        }
    }

    /// Выделяет объект из слаба
    fn alloc(&mut self) -> Option<*mut u8> {
        // Сначала пытаемся получить из свободного списка
        if let Some(ptr) = self.free_objects.pop() {
            self.allocated_count += 1;
            return Some(ptr);
        }

        // Если свободных объектов нет, создаем новый слаб
        let mut new_slab = Slab::new(self.object_size, self.objects_per_slab);

        // Получаем указатель на первый объект
        if let Some(ptr) = new_slab.alloc() {
            self.slabs.push(new_slab);
            self.allocated_count += 1;
            Some(ptr)
        } else {
            None
        }
    }

    /// Освобождает объект обратно в слаб
    fn dealloc(&mut self, ptr: *mut u8) {
        // Проверяем, принадлежит ли указатель одному из наших слабов
        for slab in &mut self.slabs {
            if slab.contains(ptr) {
                slab.dealloc(ptr);
                self.allocated_count -= 1;
                return;
            }
        }
        // Если указатель не принадлежит ни одному слабу, игнорируем
    }
}

/// Один слаб памяти
struct Slab {
    /// Буфер памяти
    buffer: Vec<u8>,
    /// Список занятых/свободных участков
    occupied: Vec<bool>,
    /// Следующий индекс для выделения
    next_free_index: usize,
    /// Размер каждого объекта
    object_size: usize,
}

impl Slab {
    /// Создает новый слаб
    fn new(object_size: usize, count: usize) -> Self {
        let total_size = object_size * count;
        let mut buffer = vec![0; total_size];
        let mut occupied = Vec::with_capacity(count);
        for _ in 0..count {
            occupied.push(false);
        }

        Slab {
            buffer,
            occupied,
            next_free_index: 0,
            object_size,
        }
    }

    /// Выделяет объект из слаба
    fn alloc(&mut self) -> Option<*mut u8> {
        // Ищем свободное место
        for i in 0..self.occupied.len() {
            let idx = (self.next_free_index + i) % self.occupied.len();
            if !self.occupied[idx] {
                self.occupied[idx] = true;
                self.next_free_index = (idx + 1) % self.occupied.len();

                let offset = idx * self.object_size;
                return Some(self.buffer[offset..].as_mut_ptr());
            }
        }
        None
    }

    /// Освобождает объект в слабе
    fn dealloc(&mut self, ptr: *mut u8) {
        // Найдем индекс по указателю
        let start_ptr = self.buffer.as_ptr() as usize;
        let obj_ptr = ptr as usize;

        if obj_ptr >= start_ptr && obj_ptr < start_ptr + self.buffer.len() {
            let offset = obj_ptr - start_ptr;
            let idx = offset / self.object_size;

            if idx < self.occupied.len() {
                self.occupied[idx] = false;
            }
        }
    }

    /// Проверяет, содержит ли слаб указатель
    fn contains(&self, ptr: *mut u8) -> bool {
        let start_ptr = self.buffer.as_ptr() as usize;
        let end_ptr = start_ptr + self.buffer.len();
        let ptr_val = ptr as usize;
        ptr_val >= start_ptr && ptr_val < end_ptr
    }
}

// Реализация GlobalAlloc для интеграции с системой выделения памяти
use core::alloc::GlobalAlloc;

pub struct LockedSlabAllocator(Mutex<SlabAllocator>);

impl LockedSlabAllocator {
    pub const fn new(pool_size: usize) -> Self {
        LockedSlabAllocator(Mutex::new(SlabAllocator {
            caches: Vec::new(),
            memory_pool: vec![0; pool_size],
            free_list: Vec::new(),
        }))
    }
}

unsafe impl GlobalAlloc for LockedSlabAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut allocator = self.0.lock();

        // Ищем кэш подходящего размера
        for (i, cache) in allocator.caches.iter().enumerate() {
            if cache.object_size == layout.size() {
                if let Some(ptr) = allocator.alloc(i) {
                    return ptr;
                }
            }
        }

        // Если нет подходящего кэша, регистрируем новый
        let cache_id = allocator.register_cache(layout.size(), 16); // 16 объектов по умолчанию
        allocator.alloc(cache_id).unwrap_or(core::ptr::null_mut())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut allocator = self.0.lock();

        // Найти кэш, которому принадлежит этот указатель
        for (i, cache) in allocator.caches.iter().enumerate() {
            if cache.object_size == layout.size() {
                allocator.dealloc(ptr, i);
                return;
            }
        }

        // Если не нашли кэш, просто игнорируем
    }
}

// Глобальный SLAB аллокатор
pub static SLAB_ALLOCATOR: LockedSlabAllocator = LockedSlabAllocator::new(1024 * 1024); // 1MB пула

/// Менеджер SLUB (SLAB Unqueued) - улучшенная версия SLAB с лучшей масштабируемостью
pub struct SlubAllocator {
    /// Кэши для разных размеров
    size_caches: Vec<SlubCache>,
    /// Общий пул памяти
    memory_pool: Vec<u8>,
}

/// SLUB кэш для конкретного размера
pub struct SlubCache {
    /// Размер объекта
    object_size: usize,
    /// Количество объектов в блоке
    objects_per_block: usize,
    /// Блоки памяти
    blocks: Vec<SlubBlock>,
    /// Свободные объекты
    free_objects: Vec<*mut u8>,
}

impl SlubCache {
    pub fn new(object_size: usize, objects_per_block: usize) -> Self {
        SlubCache {
            object_size,
            objects_per_block,
            blocks: Vec::new(),
            free_objects: Vec::new(),
        }
    }

    /// Выделяет объект
    pub fn alloc(&mut self) -> Option<*mut u8> {
        // Сначала проверяем свободные объекты
        if let Some(ptr) = self.free_objects.pop() {
            Some(ptr)
        } else {
            // Создаем новый блок
            let mut new_block = SlubBlock::new(self.object_size, self.objects_per_block);

            // Выделяем первый объект из нового блока
            if let Some(ptr) = new_block.alloc() {
                self.blocks.push(new_block);
                Some(ptr)
            } else {
                None
            }
        }
    }

    /// Освобождает объект
    pub fn dealloc(&mut self, ptr: *mut u8) {
        self.free_objects.push(ptr);
    }
}

/// Блок в SLUB
pub struct SlubBlock {
    buffer: Vec<u8>,
    occupied: Vec<bool>,
    object_size: usize,
}

impl SlubBlock {
    pub fn new(object_size: usize, count: usize) -> Self {
        let total_size = object_size * count;
        let mut buffer = vec![0; total_size];
        let mut occupied = vec![false; count];

        SlubBlock {
            buffer,
            occupied,
            object_size,
        }
    }

    pub fn alloc(&mut self) -> Option<*mut u8> {
        for (i, occupied) in self.occupied.iter_mut().enumerate() {
            if !*occupied {
                *occupied = true;
                let offset = i * self.object_size;
                return Some(self.buffer[offset..].as_mut_ptr());
            }
        }
        None
    }

    pub fn dealloc(&mut self, ptr: *mut u8) {
        let start_ptr = self.buffer.as_ptr() as usize;
        let obj_ptr = ptr as usize;

        if obj_ptr >= start_ptr && obj_ptr < start_ptr + self.buffer.len() {
            let offset = obj_ptr - start_ptr;
            let idx = offset / self.object_size;

            if idx < self.occupied.len() {
                self.occupied[idx] = false;
            }
        }
    }
}

// Интерфейс для пользователя
pub struct SlabManager {
    slab_alloc: Mutex<SlabAllocator>,
}

impl SlabManager {
    pub fn new() -> Self {
        SlabManager {
            slab_alloc: Mutex::new(SlabAllocator::new(1024 * 1024)), // 1MB пула
        }
    }

    pub fn alloc_from_cache(&self, cache_id: usize) -> Option<*mut u8> {
        let mut allocator = self.slab_alloc.lock();
        allocator.alloc(cache_id)
    }

    pub fn dealloc_to_cache(&self, ptr: *mut u8, cache_id: usize) {
        let mut allocator = self.slab_alloc.lock();
        allocator.dealloc(ptr, cache_id);
    }

    pub fn create_cache(&self, size: usize, count: usize) -> usize {
        let mut allocator = self.slab_alloc.lock();
        allocator.register_cache(size, count)
    }
}

// Глобальный менеджер SLAB
pub static SLAB_MANAGER: SlabManager = SlabManager::new();

/// Макросы для удобства использования
#[macro_export]
macro_rules! slab_alloc {
    ($size:expr, $count:expr) => {
        $crate::slab_allocator::SLAB_MANAGER.create_cache($size, $count)
    };
}

#[macro_export]
macro_rules! slab_dealloc {
    ($ptr:expr, $cache_id:expr) => {
        $crate::slab_allocator::SLAB_MANAGER.dealloc_to_cache($ptr, $cache_id)
    };
}
