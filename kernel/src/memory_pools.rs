use alloc::{boxed::Box, collections::BTreeMap, vec::Vec};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::{Mutex, MutexGuard};

/// MemoryPool - пул предварительно выделенной памяти для быстрого выделения/освобождения
///
/// Позволяет избежать фрагментации и ускорить выделение маленьких блоков памяти,
/// часто используемых в системе.
pub struct MemoryPool {
    /// Буфер пула
    buffer: UnsafeCell<Vec<u8>>,
    /// Список свободных блоков
    free_blocks: Vec<FreeBlock>,
    /// Использованные блоки (для отслеживания)
    used_blocks: BTreeMap<usize, UsedBlock>,
    /// Общий размер пула
    total_size: usize,
    /// Выравнивание
    alignment: usize,
    /// Счетчик выделений
    allocation_count: AtomicUsize,
    /// Счетчик освобождений
    deallocation_count: AtomicUsize,
    /// Флаги пула
    flags: MemoryPoolFlags,
}

/// Описание свободного блока
#[derive(Debug)]
struct FreeBlock {
    /// Смещение в буфере
    offset: usize,
    /// Размер блока
    size: usize,
}

/// Описание используемого блока
#[derive(Debug)]
struct UsedBlock {
    /// Смещение в буфере
    offset: usize,
    /// Размер блока
    size: usize,
    /// Флаги блока
    flags: BlockFlags,
    /// Время выделения (в миллисекундах с загрузки)
    alloc_time: u64,
    /// Владелец (PID или другой идентификатор)
    owner: Option<u64>,
}

/// Флаги пула памяти
#[derive(Debug, Clone)]
pub struct MemoryPoolFlags {
    /// Пул может расти (увеличивать размер)
    pub growable: bool,
    /// Пул может быть сжат (defragmentation)
    pub compressible: bool,
    /// Пул проверяет границы
    pub bounds_checking: bool,
    /// Пул отслеживает утечки
    pub leak_checking: bool,
    /// Пул заполняет освобождаемую память специальным значением
    pub poison_on_dealloc: bool,
}

impl MemoryPoolFlags {
    pub fn default() -> Self {
        MemoryPoolFlags {
            growable: true,
            compressible: true,
            bounds_checking: true,
            leak_checking: true,
            poison_on_dealloc: true,
        }
    }
}

/// Флаги блока
#[derive(Debug, Clone)]
pub struct BlockFlags {
    /// Блок используется только для чтения
    pub read_only: bool,
    /// Блок используется только для записи
    pub write_only: bool,
    /// Блок может содержать исполняемый код
    pub executable: bool,
    /// Блок может быть освобожден
    pub deallocatable: bool,
}

impl MemoryPool {
    /// Создает новый пул памяти с заданным размером
    pub fn new(size: usize, alignment: usize) -> Self {
        let mut buffer = Vec::with_capacity(size);
        buffer.resize(size, 0);

        MemoryPool {
            buffer: UnsafeCell::new(buffer),
            free_blocks: vec![FreeBlock { offset: 0, size }],
            used_blocks: BTreeMap::new(),
            total_size: size,
            alignment,
            allocation_count: AtomicUsize::new(0),
            deallocation_count: AtomicUsize::new(0),
            flags: MemoryPoolFlags::default(),
        }
    }

    /// Выделяет блок памяти из пула
    pub fn alloc(&mut self, size: usize) -> Option<*mut u8> {
        // Вычисляем размер с учетом выравнивания
        let aligned_size = self.align_to_alignment(size);

        // Ищем подходящий свободный блок
        if let Some(index) = self.find_free_block(aligned_size) {
            let free_block = self.free_blocks.remove(index);

            // Разбиваем блок, если он больше нужного размера
            if free_block.size > aligned_size {
                let remaining_block = FreeBlock {
                    offset: free_block.offset + aligned_size,
                    size: free_block.size - aligned_size,
                };
                self.free_blocks.push(remaining_block);
            }

            // Регистрируем использованный блок
            let used_block = UsedBlock {
                offset: free_block.offset,
                size: aligned_size,
                flags: BlockFlags {
                    read_only: false,
                    write_only: false,
                    executable: false,
                    deallocatable: true,
                },
                alloc_time: crate::timer::get_uptime_ms(),
                owner: None,
            };

            self.used_blocks.insert(free_block.offset, used_block);

            // Увеличиваем счетчик
            self.allocation_count.fetch_add(1, Ordering::SeqCst);

            // Возвращаем указатель на выделенную память
            let buffer_ptr = self.buffer.get();
            Some(unsafe { buffer_ptr.add(free_block.offset) as *mut u8 })
        } else if self.flags.growable {
            // Пытаемся расширить пул (в реальной системе это требует дополнительной памяти)
            None // На данном этапе расширение не реализовано
        } else {
            None // Нет места
        }
    }

    /// Освобождает блок памяти
    pub fn dealloc(&mut self, ptr: *mut u8) -> Result<(), &'static str> {
        // Вычисляем смещение из указателя
        let buffer_start = self.buffer.get() as *const u8;
        let offset = unsafe { ptr.offset_from(buffer_start as *const u8) } as usize;

        if offset >= self.total_size {
            return Err("Pointer out of pool bounds");
        }

        // Проверяем, что блок действительно выделен
        if let Some(used_block) = self.used_blocks.remove(&offset) {
            // Добавляем в список свободных блоков
            let free_block = FreeBlock {
                offset: used_block.offset,
                size: used_block.size,
            };

            // Очищаем память при освобождении (если включена опция)
            if self.flags.poison_on_dealloc {
                self.poison_memory(free_block.offset, free_block.size);
            }

            self.free_blocks.push(free_block);

            // Объединяем смежные свободные блоки (дефрагментация)
            self.coalesce_free_blocks();

            // Увеличиваем счетчик
            self.deallocation_count.fetch_add(1, Ordering::SeqCst);

            Ok(())
        } else {
            Err("Pointer not allocated from this pool")
        }
    }

    /// Выравнивает размер до границы выравнивания
    fn align_to_alignment(&self, size: usize) -> usize {
        let mask = self.alignment - 1;
        (size + mask) & !mask
    }

    /// Находит подходящий свободный блок
    fn find_free_block(&mut self, size: usize) -> Option<usize> {
        // Ищем первый подходящий блок (first-fit алгоритм)
        self.free_blocks.iter().position(|block| block.size >= size)
    }

    /// Объединяет смежные свободные блоки
    fn coalesce_free_blocks(&mut self) {
        if self.free_blocks.len() < 2 {
            return;
        }

        // Сортируем блоки по смещению
        self.free_blocks.sort_by_key(|block| block.offset);

        let mut i = 0;
        while i < self.free_blocks.len() - 1 {
            let current = &self.free_blocks[i];
            let next = &self.free_blocks[i + 1];

            // Проверяем, смежны ли блоки
            if current.offset + current.size == next.offset {
                // Объединяем блоки
                let new_block = FreeBlock {
                    offset: current.offset,
                    size: current.size + next.size,
                };

                // Удаляем старые блоки и добавляем объединенный
                self.free_blocks.remove(i + 1);
                self.free_blocks[i] = new_block;
                // Не увеличиваем i, чтобы проверить снова
            } else {
                i += 1;
            }
        }
    }

    /// Заполняет память специальным значением (для обнаружения ошибок использования)
    fn poison_memory(&mut self, offset: usize, size: usize) {
        if size == 0 {
            return;
        }

        let buffer_ptr = self.buffer.get() as *mut u8;
        unsafe {
            for i in 0..size {
                core::ptr::write(buffer_ptr.add(offset + i), 0xDE); // Магическое значение для обнаружения
            }
        }
    }

    /// Проверяет, принадлежит ли указатель пулу
    pub fn is_pool_pointer(&self, ptr: *const u8) -> bool {
        let buffer_start = self.buffer.get() as *const u8;
        let buffer_end = unsafe { buffer_start.add(self.total_size) };
        let ptr_val = ptr as usize;
        let start_val = buffer_start as usize;
        let end_val = buffer_end as usize;

        ptr_val >= start_val && ptr_val < end_val
    }

    /// Получает размер выделенного блока
    pub fn get_allocated_size(&self, ptr: *const u8) -> Option<usize> {
        let buffer_start = self.buffer.get() as *const u8;
        let offset = unsafe { (ptr as *const u8).offset_from(buffer_start) } as usize;

        if offset < self.total_size {
            self.used_blocks.get(&offset).map(|block| block.size)
        } else {
            None
        }
    }

    /// Получает количество выделений
    pub fn get_allocation_count(&self) -> usize {
        self.allocation_count.load(Ordering::SeqCst)
    }

    /// Получает количество освобождений
    pub fn get_deallocation_count(&self) -> usize {
        self.deallocation_count.load(Ordering::SeqCst)
    }

    /// Получает количество активных выделений
    pub fn get_active_allocation_count(&self) -> usize {
        self.used_blocks.len()
    }

    /// Получает текущую загрузку пула (0.0 - 0%, 1.0 - 100%)
    pub fn get_utilization(&self) -> f64 {
        let total_used: usize = self.used_blocks.values().map(|block| block.size).sum();
        if self.total_size > 0 {
            total_used as f64 / self.total_size as f64
        } else {
            0.0
        }
    }

    /// Устанавливает владельца блока
    pub fn set_block_owner(&mut self, ptr: *const u8, owner: u64) -> Result<(), &'static str> {
        let buffer_start = self.buffer.get() as *const u8;
        let offset = unsafe { (ptr as *const u8).offset_from(buffer_start) } as usize;

        if offset >= self.total_size {
            return Err("Pointer out of pool bounds");
        }

        if let Some(block) = self.used_blocks.get_mut(&offset) {
            block.owner = Some(owner);
            Ok(())
        } else {
            Err("Pointer not allocated from this pool")
        }
    }

    /// Получает владельца блока
    pub fn get_block_owner(&self, ptr: *const u8) -> Option<u64> {
        let buffer_start = self.buffer.get() as *const u8;
        let offset = unsafe { (ptr as *const u8).offset_from(buffer_start) } as usize;

        if offset < self.total_size {
            self.used_blocks.get(&offset).and_then(|block| block.owner)
        } else {
            None
        }
    }

    /// Устанавливает флаги для блока
    pub fn set_block_flags(
        &mut self,
        ptr: *const u8,
        flags: BlockFlags,
    ) -> Result<(), &'static str> {
        let buffer_start = self.buffer.get() as *const u8;
        let offset = unsafe { (ptr as *const u8).offset_from(buffer_start) } as usize;

        if offset >= self.total_size {
            return Err("Pointer out of pool bounds");
        }

        if let Some(block) = self.used_blocks.get_mut(&offset) {
            block.flags = flags;
            Ok(())
        } else {
            Err("Pointer not allocated from this pool")
        }
    }

    /// Получает флаги блока
    pub fn get_block_flags(&self, ptr: *const u8) -> Option<BlockFlags> {
        let buffer_start = self.buffer.get() as *const u8;
        let offset = unsafe { (ptr as *const u8).offset_from(buffer_start) } as usize;

        if offset < self.total_size {
            self.used_blocks
                .get(&offset)
                .map(|block| block.flags.clone())
        } else {
            None
        }
    }

    /// Собирает статистику пула
    pub fn get_stats(&self) -> MemoryPoolStats {
        let total_used: usize = self.used_blocks.values().map(|block| block.size).sum();
        let total_free: usize = self.free_blocks.iter().map(|block| block.size).sum();

        MemoryPoolStats {
            total_size: self.total_size,
            used_size: total_used,
            free_size: total_free,
            allocation_count: self.allocation_count.load(Ordering::SeqCst),
            deallocation_count: self.deallocation_count.load(Ordering::SeqCst),
            active_allocations: self.used_blocks.len(),
            utilization: self.get_utilization(),
        }
    }

    /// Сжимает пул (дефрагментирует)
    pub fn compress(&mut self) -> Result<(), &'static str> {
        if !self.flags.compressible {
            return Err("Pool is not compressible");
        }

        // Объединяем соседние свободные блоки
        self.coalesce_free_blocks();
        Ok(())
    }

    /// Очищает пул от всех выделений
    ///
    /// ВНИМАНИЕ: это приведет к потере всех выделенных блоков!
    pub fn clear(&mut self) -> Result<(), &'static str> {
        // Добавляем один большой свободный блок
        self.free_blocks.clear();
        self.used_blocks.clear();

        self.free_blocks.push(FreeBlock {
            offset: 0,
            size: self.total_size,
        });

        // Сброс счетчиков
        self.allocation_count.store(0, Ordering::SeqCst);
        self.deallocation_count.store(0, Ordering::SeqCst);

        // Очищаем буфер
        let buffer = unsafe { &mut *self.buffer.get() };
        buffer.fill(0);

        Ok(())
    }
}

/// Статистика пула памяти
#[derive(Debug, Clone)]
pub struct MemoryPoolStats {
    /// Общий размер пула
    pub total_size: usize,
    /// Используемый размер
    pub used_size: usize,
    /// Свободный размер
    pub free_size: usize,
    /// Количество выделений
    pub allocation_count: usize,
    /// Количество освобождений
    pub deallocation_count: usize,
    /// Активные выделения
    pub active_allocations: usize,
    /// Загрузка пула (0.0 - 1.0)
    pub utilization: f64,
}

/// Пул фиксированного размера (оптимизированный для блоков одного размера)
pub struct FixedSizePool {
    /// Буфер пула
    buffer: UnsafeCell<Vec<u8>>,
    /// Битовая карта использования
    bitmap: Vec<u64>,
    /// Размер каждого блока
    block_size: usize,
    /// Общее количество блоков
    total_blocks: usize,
    /// Количество свободных блоков
    free_count: AtomicUsize,
    /// Индекс первого свободного блока (оптимизация)
    first_free_index: AtomicUsize,
    /// Флаги пула
    flags: FixedPoolFlags,
}

/// Флаги для фиксированного пула
#[derive(Debug, Clone)]
pub struct FixedPoolFlags {
    /// Проверка границ
    pub bounds_checking: bool,
    /// Заполнение при освобождении
    pub poison_on_dealloc: bool,
}

impl FixedPoolFlags {
    pub fn default() -> Self {
        FixedPoolFlags {
            bounds_checking: true,
            poison_on_dealloc: true,
        }
    }
}

impl FixedSizePool {
    /// Создает пул фиксированного размера
    pub fn new(block_size: usize, block_count: usize) -> Self {
        let total_size = block_size * block_count;
        let mut buffer = Vec::with_capacity(total_size);
        buffer.resize(total_size, 0);

        // Вычисляем количество u64 в битовой карте
        let bitmap_size = (block_count + 63) / 64; // Округление вверх
        let bitmap = vec![0; bitmap_size];

        FixedSizePool {
            buffer: UnsafeCell::new(buffer),
            bitmap,
            block_size,
            total_blocks: block_count,
            free_count: AtomicUsize::new(block_count),
            first_free_index: AtomicUsize::new(0),
            flags: FixedPoolFlags::default(),
        }
    }

    /// Выделяет блок из пула фиксированного размера
    pub fn alloc(&self) -> Option<*mut u8> {
        // Ищем свободный блок
        let first_free = self.first_free_index.load(Ordering::Relaxed);
        let mut start_index = first_free;

        // Попробуем найти свободный блок начиная с last_free
        for i in start_index..self.total_blocks {
            if self.is_block_free(i) {
                if self.mark_block_used(i) {
                    self.first_free_index.store(i, Ordering::Relaxed);

                    let buffer_ptr = self.buffer.get() as *mut u8;
                    let offset = i * self.block_size;

                    self.free_count.fetch_sub(1, Ordering::SeqCst);
                    return Some(unsafe { buffer_ptr.add(offset) });
                }
            }
        }

        // Если не нашли с last_free, ищем с начала
        for i in 0..start_index {
            if self.is_block_free(i) {
                if self.mark_block_used(i) {
                    self.first_free_index.store(i, Ordering::Relaxed);

                    let buffer_ptr = self.buffer.get() as *mut u8;
                    let offset = i * self.block_size;

                    self.free_count.fetch_sub(1, Ordering::SeqCst);
                    return Some(unsafe { buffer_ptr.add(offset) });
                }
            }
        }

        None // Нет свободных блоков
    }

    /// Освобождает блок в пуле фиксированного размера
    pub fn dealloc(&self, ptr: *mut u8) -> Result<(), &'static str> {
        let buffer_start = self.buffer.get() as *const u8;
        let offset = unsafe { (ptr as *const u8).offset_from(buffer_start) } as usize;

        if offset % self.block_size != 0 {
            return Err("Pointer not aligned to block boundary");
        }

        let block_index = offset / self.block_size;
        if block_index >= self.total_blocks {
            return Err("Pointer out of pool bounds");
        }

        if !self.is_block_used(block_index) {
            return Err("Block not allocated");
        }

        // Помечаем как свободный
        self.mark_block_free(block_index);

        // Обновляем first_free_index
        let current = self.first_free_index.load(Ordering::Relaxed);
        if block_index < current {
            self.first_free_index.store(block_index, Ordering::Relaxed);
        }

        // Заполняем память при освобождении
        if self.flags.poison_on_dealloc {
            self.poison_block(block_index);
        }

        self.free_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    /// Проверяет, свободен ли блок
    fn is_block_free(&self, index: usize) -> bool {
        let word_index = index / 64;
        let bit_index = index % 64;

        let bitmap = unsafe { &*self.bitmap.get() };
        (bitmap[word_index] & (1u64 << bit_index)) == 0
    }

    /// Проверяет, используется ли блок
    fn is_block_used(&self, index: usize) -> bool {
        !self.is_block_free(index)
    }

    /// Помечает блок как используемый
    fn mark_block_used(&self, index: usize) -> bool {
        let word_index = index / 64;
        let bit_index = index % 64;

        let bitmap = self.bitmap.get() as *mut [u64];
        let bit_mask = 1u64 << bit_index;

        unsafe {
            let old_word = core::ptr::read(bitmap.as_ptr().add(word_index));
            if old_word & bit_mask != 0 {
                // Блок уже помечен как используемый
                return false;
            }

            core::ptr::write(bitmap.as_ptr().add(word_index), old_word | bit_mask);
        }

        true
    }

    /// Помечает блок как свободный
    fn mark_block_free(&self, index: usize) {
        let word_index = index / 64;
        let bit_index = index % 64;

        let bitmap = self.bitmap.get() as *mut [u64];
        let bit_mask = !(1u64 << bit_index);

        unsafe {
            let old_word = core::ptr::read(bitmap.as_ptr().add(word_index));
            core::ptr::write(bitmap.as_ptr().add(word_index), old_word & bit_mask);
        }
    }

    /// Заполняет блок специальным значением
    fn poison_block(&self, index: usize) {
        let buffer_ptr = self.buffer.get() as *mut u8;
        let offset = index * self.block_size;

        unsafe {
            for i in 0..self.block_size {
                core::ptr::write(buffer_ptr.add(offset + i), 0xFD); // Другой магический байт
            }
        }
    }

    /// Проверяет, принадлежит ли указатель пулу
    pub fn is_pool_pointer(&self, ptr: *const u8) -> bool {
        let buffer_start = self.buffer.get() as *const u8;
        let buffer_end = unsafe { buffer_start.add(self.total_blocks * self.block_size) };
        let ptr_val = ptr as usize;
        let start_val = buffer_start as usize;
        let end_val = buffer_end as usize;

        ptr_val >= start_val && ptr_val < end_val && (ptr_val - start_val) % self.block_size == 0
    }

    /// Получает статистику фиксированного пула
    pub fn get_stats(&self) -> FixedPoolStats {
        FixedPoolStats {
            block_size: self.block_size,
            total_blocks: self.total_blocks,
            free_blocks: self.free_count.load(Ordering::SeqCst),
            used_blocks: self.total_blocks - self.free_count.load(Ordering::SeqCst),
            utilization: self.get_utilization(),
        }
    }

    /// Получает загрузку пула
    pub fn get_utilization(&self) -> f64 {
        let used = self.total_blocks - self.free_count.load(Ordering::SeqCst);
        if self.total_blocks > 0 {
            used as f64 / self.total_blocks as f64
        } else {
            0.0
        }
    }

    /// Получает количество свободных блоков
    pub fn get_free_count(&self) -> usize {
        self.free_count.load(Ordering::SeqCst)
    }

    /// Получает количество использованных блоков
    pub fn get_used_count(&self) -> usize {
        self.total_blocks - self.free_count.load(Ordering::SeqCst)
    }
}

/// Статистика фиксированного пула
#[derive(Debug, Clone)]
pub struct FixedPoolStats {
    /// Размер блока
    pub block_size: usize,
    /// Общее количество блоков
    pub total_blocks: usize,
    /// Количество свободных блоков
    pub free_blocks: usize,
    /// Количество использованных блоков
    pub used_blocks: usize,
    /// Загрузка (0.0 - 1.0)
    pub utilization: f64,
}

/// Менеджер пулов памяти разного размера
pub struct MemoryPoolManager {
    /// Пулы маленьких блоков (32, 64, 128 байт)
    small_pools: BTreeMap<usize, Mutex<FixedSizePool>>,
    /// Пулы средних блоков (256, 512, 1024 байт)
    medium_pools: BTreeMap<usize, Mutex<FixedSizePool>>,
    /// Пулы больших блоков (2048, 4096 байт)
    large_pools: BTreeMap<usize, Mutex<FixedSizePool>>,
    /// Общий пул для больших выделений
    general_pool: Mutex<MemoryPool>,
    /// Пулы по размеру блока
    pub all_pools: BTreeMap<usize, Mutex<MemoryPool>>,
}

impl MemoryPoolManager {
    /// Создает новый менеджер пулов
    pub fn new() -> Self {
        MemoryPoolManager {
            small_pools: BTreeMap::new(),
            medium_pools: BTreeMap::new(),
            large_pools: BTreeMap::new(),
            general_pool: Mutex::new(MemoryPool::new(1024 * 1024, 16)), // 1MB общий пул
            all_pools: BTreeMap::new(),
        }
    }

    /// Инициализирует стандартные пулы
    pub fn init_standard_pools(&mut self) {
        // Пулы маленьких блоков
        for &size in &[32, 64, 128] {
            self.small_pools.insert(
                size,
                Mutex::new(FixedSizePool::new(size, 1000)), // 1000 блоков каждого размера
            );
        }

        // Пулы средних блоков
        for &size in &[256, 512, 1024] {
            self.medium_pools.insert(
                size,
                Mutex::new(FixedSizePool::new(size, 500)), // 500 блоков каждого размера
            );
        }

        // Пулы больших блоков
        for &size in &[2048, 4096] {
            self.large_pools.insert(
                size,
                Mutex::new(FixedSizePool::new(size, 250)), // 250 блоков каждого размера
            );
        }
    }

    /// Выделяет память из подходящего пула
    pub fn alloc(&self, size: usize) -> Option<*mut u8> {
        // Определяем, какой пул использовать
        if size <= 128 {
            // Пытаемся выделить из маленького пула
            if let Some(pool) = self.select_small_pool(size) {
                return pool.lock().alloc();
            }
        } else if size <= 1024 {
            // Пытаемся выделить из среднего пула
            if let Some(pool) = self.select_medium_pool(size) {
                return pool.lock().alloc();
            }
        } else if size <= 4096 {
            // Пытаемся выделить из большого пула
            if let Some(pool) = self.select_large_pool(size) {
                return pool.lock().alloc();
            }
        }

        // Если размер больше 4KB или не нашли подходящий пул, используем общий
        self.general_pool.lock().alloc(size)
    }

    /// Освобождает память в соответствующем пуле
    pub fn dealloc(&self, ptr: *mut u8, size: usize) -> Result<(), &'static str> {
        // Определяем, из какого пула был выделен указатель
        if size <= 128 {
            if let Some(pool) = self.select_small_pool(size) {
                return pool.lock().dealloc(ptr);
            }
        } else if size <= 1024 {
            if let Some(pool) = self.select_medium_pool(size) {
                return pool.lock().dealloc(ptr);
            }
        } else if size <= 4096 {
            if let Some(pool) = self.select_large_pool(size) {
                return pool.lock().dealloc(ptr);
            }
        }

        // Используем общий пул
        self.general_pool.lock().dealloc(ptr)
    }

    /// Выбирает маленький пул для размера
    fn select_small_pool(&self, size: usize) -> Option<&Mutex<FixedSizePool>> {
        // Выбираем ближайший больший размер
        let sizes = [32, 64, 128];
        for &pool_size in &sizes {
            if size <= pool_size {
                return self.small_pools.get(&pool_size);
            }
        }
        None
    }

    /// Выбирает средний пул для размера
    fn select_medium_pool(&self, size: usize) -> Option<&Mutex<FixedSizePool>> {
        let sizes = [256, 512, 1024];
        for &pool_size in &sizes {
            if size <= pool_size {
                return self.medium_pools.get(&pool_size);
            }
        }
        None
    }

    /// Выбирает большой пул для размера
    fn select_large_pool(&self, size: usize) -> Option<&Mutex<FixedSizePool>> {
        let sizes = [2048, 4096];
        for &pool_size in &sizes {
            if size <= pool_size {
                return self.large_pools.get(&pool_size);
            }
        }
        None
    }

    /// Получает статистику всех пулов
    pub fn get_all_stats(&self) -> AllPoolStats {
        let mut small_stats = Vec::new();
        for (size, pool) in &self.small_pools {
            small_stats.push((*size, pool.lock().get_stats()));
        }

        let mut medium_stats = Vec::new();
        for (size, pool) in &self.medium_pools {
            medium_stats.push((*size, pool.lock().get_stats()));
        }

        let mut large_stats = Vec::new();
        for (size, pool) in &self.large_pools {
            large_stats.push((*size, pool.lock().get_stats()));
        }

        let general_stats = self.general_pool.lock().get_stats();

        AllPoolStats {
            small_pools: small_stats,
            medium_pools: medium_stats,
            large_pools: large_stats,
            general_pool: general_stats,
        }
    }
}

/// Статистика всех пулов
#[derive(Debug, Clone)]
pub struct AllPoolStats {
    pub small_pools: Vec<(usize, FixedPoolStats)>,
    pub medium_pools: Vec<(usize, FixedPoolStats)>,
    pub large_pools: Vec<(usize, FixedPoolStats)>,
    pub general_pool: MemoryPoolStats,
}

// Глобальный менеджер пулов
use spin::Once;

static POOL_MANAGER: Once<Mutex<MemoryPoolManager>> = Once::new();

pub fn get_pool_manager() -> &'static Mutex<MemoryPoolManager> {
    POOL_MANAGER.call_once(|| {
        let mut manager = MemoryPoolManager::new();
        manager.init_standard_pools();
        Mutex::new(manager)
    })
}

/// Интерфейс для работы с пулами
pub struct PoolInterface;

impl PoolInterface {
    /// Выделяет память из подходящего пула
    pub fn alloc(size: usize) -> Option<*mut u8> {
        get_pool_manager().lock().alloc(size)
    }

    /// Освобождает память в соответствующем пуле
    pub fn dealloc(ptr: *mut u8, size: usize) -> Result<(), &'static str> {
        get_pool_manager().lock().dealloc(ptr, size)
    }

    /// Проверяет, принадлежит ли указатель какому-либо пулу
    pub fn is_pool_pointer(ptr: *const u8) -> bool {
        let manager = get_pool_manager().lock();

        // Проверяем во всех пулах
        for (_, pool) in &manager.small_pools {
            if pool.lock().is_pool_pointer(ptr) {
                return true;
            }
        }

        for (_, pool) in &manager.medium_pools {
            if pool.lock().is_pool_pointer(ptr) {
                return true;
            }
        }

        for (_, pool) in &manager.large_pools {
            if pool.lock().is_pool_pointer(ptr) {
                return true;
            }
        }

        manager.general_pool.lock().is_pool_pointer(ptr)
    }

    /// Получает статистику всех пулов
    pub fn get_all_pool_stats() -> AllPoolStats {
        get_pool_manager().lock().get_all_stats()
    }

    /// Получает размер выделенного блока (если возможно)
    pub fn get_allocated_size(ptr: *const u8) -> Option<usize> {
        let manager = get_pool_manager().lock();

        // В фиксированных пулах размер известен
        for (_, pool) in &manager.small_pools {
            if pool.lock().is_pool_pointer(ptr) {
                // В фиксированном пуле размер блока фиксирован
                return Some(pool.lock().block_size);
            }
        }

        for (_, pool) in &manager.medium_pools {
            if pool.lock().is_pool_pointer(ptr) {
                return Some(pool.lock().block_size);
            }
        }

        for (_, pool) in &manager.large_pools {
            if pool.lock().is_pool_pointer(ptr) {
                return Some(pool.lock().block_size);
            }
        }

        // В общем пуле получаем из информации о блоке
        manager.general_pool.lock().get_allocated_size(ptr)
    }
}

/// Макрос для выделения из пула
#[macro_export]
macro_rules! pool_alloc {
    ($size:expr) => {
        $crate::memory_pools::PoolInterface::alloc($size)
    };
}

/// Макрос для освобождения в пул
#[macro_export]
macro_rules! pool_dealloc {
    ($ptr:expr, $size:expr) => {
        let _ = $crate::memory_pools::PoolInterface::dealloc($ptr, $size);
    };
}

/// Макрос для безопасного выделения с автоматическим освобождением
#[macro_export]
macro_rules! with_pool_alloc {
    ($size:expr, |$ptr:ident| $body:block) => {
        if let Some($ptr) = $crate::memory_pools::PoolInterface::alloc($size) {
            $body
            $crate::memory_pools::pool_dealloc!($ptr, $size);
        } else {
            // Память не выделена, обработка ошибки
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_pool_creation() {
        let pool = MemoryPool::new(1024, 16);
        assert_eq!(pool.total_size, 1024);
        assert_eq!(pool.alignment, 16);
        assert_eq!(pool.get_utilization(), 0.0);
    }

    #[test]
    fn test_fixed_size_pool_creation() {
        let pool = FixedSizePool::new(64, 10); // Блоки по 64 байта, 10 блоков
        assert_eq!(pool.block_size, 64);
        assert_eq!(pool.total_blocks, 10);
        assert_eq!(pool.get_used_count(), 0);
        assert_eq!(pool.get_free_count(), 10);
    }

    #[test]
    fn test_pool_alloc_and_dealloc() {
        let mut pool = MemoryPool::new(1024, 16);

        let ptr = pool.alloc(32);
        assert!(ptr.is_some());

        let size = pool.get_allocated_size(ptr.unwrap());
        assert!(size.is_some());
        assert!(size.unwrap() >= 32); // Выравнивание может увеличить размер

        assert!(pool.dealloc(ptr.unwrap()).is_ok());
    }

    #[test]
    fn test_fixed_pool_alloc_and_dealloc() {
        let pool = FixedSizePool::new(64, 10);

        let ptr1 = pool.alloc();
        assert!(ptr1.is_some());

        let ptr2 = pool.alloc();
        assert!(ptr2.is_some());

        let used_before = pool.get_used_count();
        assert!(used_before >= 2);

        assert!(pool.dealloc(ptr1.unwrap()).is_ok());
        assert!(pool.dealloc(ptr2.unwrap()).is_ok());

        let used_after = pool.get_used_count();
        assert_eq!(used_after, used_before - 2);
    }

    #[test]
    fn test_memory_pool_manager() {
        let manager = get_pool_manager();

        let ptr = PoolInterface::alloc(64);
        assert!(ptr.is_some());

        let is_member = PoolInterface::is_pool_pointer(ptr.unwrap());
        assert!(is_member);

        let _ = PoolInterface::dealloc(ptr.unwrap(), 64);
    }

    #[test]
    fn test_pool_utilization() {
        let mut pool = MemoryPool::new(1024, 16);

        let utilization_before = pool.get_utilization();
        assert_eq!(utilization_before, 0.0);

        let ptr = pool.alloc(512);
        assert!(ptr.is_some());

        let utilization_after = pool.get_utilization();
        assert!(utilization_after > 0.0);

        let _ = pool.dealloc(ptr.unwrap());

        let utilization_final = pool.get_utilization();
        assert_eq!(utilization_final, 0.0);
    }

    #[test]
    fn test_pool_clear() {
        let mut pool = MemoryPool::new(1024, 16);

        // Выделяем несколько блоков
        let ptr1 = pool.alloc(32);
        let ptr2 = pool.alloc(64);
        assert!(ptr1.is_some() && ptr2.is_some());

        assert!(pool.get_active_allocation_count() > 0);

        // Очищаем пул
        assert!(pool.clear().is_ok());

        assert_eq!(pool.get_active_allocation_count(), 0);
        assert_eq!(pool.get_utilization(), 0.0);
    }
}
