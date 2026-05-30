use alloc::{collections::BTreeMap, vec::Vec};
use core::cell::UnsafeCell;
use spin::Mutex;

/// MemoryPool - пул предварительно выделенной памяти для быстрого выделения/освобождения
///
/// Пул памяти позволяет избежать фрагментации и ускорить выделение маленьких
/// блоков памяти, часто используемых в системе.
pub struct MemoryPool {
    /// Буфер пула
    buffer: UnsafeCell<Vec<u8>>,
    /// Список свободных блоков
    free_blocks: Vec<FreeBlock>,
    /// Использованные блоки
    allocated_blocks: BTreeMap<usize, AllocatedBlock>,
    /// Размер пула
    size: usize,
    /// Указатель на следующую свободную позицию
    next_free: usize,
    /// Выравнивание
    alignment: usize,
}

/// Описание свободного блока
#[derive(Debug)]
struct FreeBlock {
    /// Смещение в буфере
    offset: usize,
    /// Размер
    size: usize,
}

/// Описание выделенного блока
#[derive(Debug)]
struct AllocatedBlock {
    /// Смещение в буфере
    offset: usize,
    /// Размер
    size: usize,
    /// Флаг выделения
    allocated: bool,
}

unsafe impl Sync for MemoryPool {}
unsafe impl Send for MemoryPool {}

impl MemoryPool {
    /// Создает новый пул памяти
    pub fn new(size: usize, alignment: usize) -> Self {
        let mut buffer = Vec::with_capacity(size);
        buffer.resize(size, 0);

        MemoryPool {
            buffer: UnsafeCell::new(buffer),
            free_blocks: vec![FreeBlock { offset: 0, size }],
            allocated_blocks: BTreeMap::new(),
            size,
            next_free: 0,
            alignment,
        }
    }

    /// Выделяет блок памяти из пула
    pub fn alloc(&mut self, size: usize) -> Option<*mut u8> {
        let aligned_size = self.align_to_alignment(size);

        // Сначала пытаемся найти подходящий свободный блок
        if let Some(index) = self.find_free_block(aligned_size) {
            let free_block = self.free_blocks.remove(index);

            // Если блок больше нужного размера, разбиваем его
            if free_block.size > aligned_size {
                let new_free_block = FreeBlock {
                    offset: free_block.offset + aligned_size,
                    size: free_block.size - aligned_size,
                };
                self.free_blocks.push(new_free_block);
            }

            // Регистрируем выделение
            let block = AllocatedBlock {
                offset: free_block.offset,
                size: aligned_size,
                allocated: true,
            };
            self.allocated_blocks.insert(free_block.offset, block);

            // Возвращаем указатель на выделенную память
            let buffer_ptr = self.buffer.get() as *mut u8;
            Some(unsafe { buffer_ptr.add(free_block.offset) })
        } else {
            // Пытаемся использовать остаток буфера
            if self.next_free + aligned_size <= self.size {
                let offset = self.next_free;
                self.next_free += aligned_size;

                let block = AllocatedBlock {
                    offset,
                    size: aligned_size,
                    allocated: true,
                };
                self.allocated_blocks.insert(offset, block);

                let buffer_ptr = self.buffer.get() as *mut u8;
                Some(unsafe { buffer_ptr.add(offset) })
            } else {
                None // Нет места
            }
        }
    }

    /// Освобождает блок памяти
    pub fn dealloc(&mut self, ptr: *mut u8) -> Result<(), &'static str> {
        // Получаем смещение из указателя
        let buffer_start = self.buffer.get() as *const u8;
        let offset = unsafe { ptr.offset_from(buffer_start) } as usize;

        // Ищем выделенный блок по смещению
        if let Some(block) = self.allocated_blocks.remove(&offset) {
            // Добавляем блок обратно в свободные
            let free_block = FreeBlock {
                offset: block.offset,
                size: block.size,
            };

            // Пытаемся объединить с соседними свободными блоками
            self.merge_free_blocks(free_block);

            Ok(())
        } else {
            Err("Pointer not found in allocated blocks")
        }
    }

    /// Ищет подходящий свободный блок
    fn find_free_block(&self, size: usize) -> Option<usize> {
        self.free_blocks.iter().position(|block| block.size >= size)
    }

    /// Выравнивает размер до границы
    fn align_to_alignment(&self, size: usize) -> usize {
        let alignment_mask = self.alignment - 1;
        (size + alignment_mask) & !alignment_mask
    }

    /// Объединяет свободные блоки
    fn merge_free_blocks(&mut self, new_block: FreeBlock) {
        let mut merged = false;

        for i in 0..self.free_blocks.len() {
            let block = &self.free_blocks[i];

            // Проверяем, можно ли объединить с существующим блоком
            if block.offset + block.size == new_block.offset {
                // Новый блок идет сразу после существующего
                self.free_blocks[i].size += new_block.size;
                merged = true;
                break;
            } else if new_block.offset + new_block.size == block.offset {
                // Новый блок идет сразу перед существующим
                self.free_blocks[i].offset = new_block.offset;
                self.free_blocks[i].size += new_block.size;
                merged = true;
                break;
            }
        }

        if !merged {
            // Добавляем как отдельный блок
            self.free_blocks.push(new_block);
        }
    }

    /// Получает статистику пула
    pub fn get_stats(&self) -> PoolStats {
        let allocated_count = self.allocated_blocks.len();
        let free_count = self.free_blocks.len();
        let used_size: usize = self.allocated_blocks.values().map(|b| b.size).sum();
        let free_size: usize = self.free_blocks.iter().map(|b| b.size).sum();

        PoolStats {
            total_size: self.size,
            used_size,
            free_size,
            allocated_blocks: allocated_count,
            free_blocks: free_count,
        }
    }

    /// Проверяет, указатель принадлежит пулу
    pub fn is_pool_pointer(&self, ptr: *const u8) -> bool {
        let buffer_start = self.buffer.get() as *const u8;
        let buffer_end = unsafe { buffer_start.add(self.size) };
        let ptr_val = ptr as usize;
        let start_val = buffer_start as usize;
        let end_val = buffer_end as usize;

        ptr_val >= start_val && ptr_val < end_val
    }

    /// Очищает пул (освобождает все блоки)
    pub fn clear(&mut self) {
        // Очищаем все списки
        self.free_blocks.clear();
        self.allocated_blocks.clear();

        // Добавляем один большой свободный блок
        self.free_blocks.push(FreeBlock {
            offset: 0,
            size: self.size,
        });

        self.next_free = 0;
    }
}

/// Статистика пула памяти
pub struct PoolStats {
    pub total_size: usize,
    pub used_size: usize,
    pub free_size: usize,
    pub allocated_blocks: usize,
    pub free_blocks: usize,
}

// Типичные размеры пулов
pub const SMALL_POOL_SIZE: usize = 64 * 1024; // 64KB для маленьких объектов
pub const MEDIUM_POOL_SIZE: usize = 256 * 1024; // 256KB для средних объектов
pub const LARGE_POOL_SIZE: usize = 1024 * 1024; // 1MB для больших объектов

// Выравнивание
pub const DEFAULT_ALIGNMENT: usize = 16; // 16-байтовое выравнивание

/// Группа пулов памяти для разных размеров
pub struct MemoryPoolManager {
    /// Маленькие объекты (до 256 байт)
    small_pool: Mutex<MemoryPool>,
    /// Средние объекты (до 1KB)
    medium_pool: Mutex<MemoryPool>,
    /// Большие объекты (до 10KB)
    large_pool: Mutex<MemoryPool>,
}

impl MemoryPoolManager {
    /// Создает новый менеджер пулов
    pub fn new() -> Self {
        MemoryPoolManager {
            small_pool: Mutex::new(MemoryPool::new(SMALL_POOL_SIZE, DEFAULT_ALIGNMENT)),
            medium_pool: Mutex::new(MemoryPool::new(MEDIUM_POOL_SIZE, DEFAULT_ALIGNMENT)),
            large_pool: Mutex::new(MemoryPool::new(LARGE_POOL_SIZE, DEFAULT_ALIGNMENT)),
        }
    }

    /// Выделяет память из соответствующего пула
    pub fn alloc(&self, size: usize) -> Option<*mut u8> {
        if size <= 256 {
            self.small_pool.lock().alloc(size)
        } else if size <= 1024 {
            self.medium_pool.lock().alloc(size)
        } else if size <= 10240 {
            self.large_pool.lock().alloc(size)
        } else {
            None // Слишком большой для пулов
        }
    }

    /// Освобождает память в соответствующем пуле
    pub fn dealloc(&self, ptr: *mut u8, size: usize) -> Result<(), &'static str> {
        if size <= 256 {
            self.small_pool.lock().dealloc(ptr)
        } else if size <= 1024 {
            self.medium_pool.lock().dealloc(ptr)
        } else if size <= 10240 {
            self.large_pool.lock().dealloc(ptr)
        } else {
            Err("Size too large for pools")
        }
    }

    /// Проверяет, указатель принадлежит какому-либо пулу
    pub fn is_pool_pointer(&self, ptr: *const u8) -> Option<PoolType> {
        if self.small_pool.lock().is_pool_pointer(ptr) {
            Some(PoolType::Small)
        } else if self.medium_pool.lock().is_pool_pointer(ptr) {
            Some(PoolType::Medium)
        } else if self.large_pool.lock().is_pool_pointer(ptr) {
            Some(PoolType::Large)
        } else {
            None
        }
    }

    /// Получает статистику всех пулов
    pub fn get_all_stats(&self) -> AllPoolStats {
        AllPoolStats {
            small: self.small_pool.lock().get_stats(),
            medium: self.medium_pool.lock().get_stats(),
            large: self.large_pool.lock().get_stats(),
        }
    }

    /// Очищает все пулы
    pub fn clear_all(&self) {
        self.small_pool.lock().clear();
        self.medium_pool.lock().clear();
        self.large_pool.lock().clear();
    }
}

/// Тип пула
pub enum PoolType {
    Small,
    Medium,
    Large,
}

/// Статистика всех пулов
pub struct AllPoolStats {
    pub small: PoolStats,
    pub medium: PoolStats,
    pub large: PoolStats,
}

// Глобальный менеджер пулов
use spin::Once;

static MEMORY_POOL_MANAGER: Once<Mutex<MemoryPoolManager>> = Once::new();

pub fn get_memory_pool_manager() -> &'static Mutex<MemoryPoolManager> {
    MEMORY_POOL_MANAGER.call_once(|| Mutex::new(MemoryPoolManager::new()))
}

/// Обертка для глобального использования
pub fn global_pool_alloc(size: usize) -> Option<*mut u8> {
    get_memory_pool_manager().lock().alloc(size)
}

pub fn global_pool_dealloc(ptr: *mut u8, size: usize) -> Result<(), &'static str> {
    get_memory_pool_manager().lock().dealloc(ptr, size)
}

/// Проверяет, принадлежит ли указатель пулу
pub fn is_pool_pointer_global(ptr: *const u8) -> bool {
    get_memory_pool_manager()
        .lock()
        .is_pool_pointer(ptr)
        .is_some()
}

/// Реализация GlobalAlloc для использования пулов вместо системного аллокатора
use core::alloc::GlobalAlloc;

pub struct PoolAllocator;

unsafe impl GlobalAlloc for PoolAllocator {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        if let Some(ptr) = global_pool_alloc(layout.size()) {
            ptr
        } else {
            // Если пул не может выделить, используем стандартный аллокатор
            // В реальной системе нужно будет здесь выделить физическую память
            core::ptr::null_mut()
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        // Пытаемся освободить через пул
        let _ = global_pool_dealloc(ptr, layout.size());
        // Если не принадлежит пулу, игнорируем
    }
}

// Глобальный аллокатор на основе пулов
pub static POOL_ALLOCATOR: PoolAllocator = PoolAllocator;

/// Интерфейс для ядра
pub struct KernelPoolInterface;

impl KernelPoolInterface {
    /// Выделяет память для ядра
    pub fn kernel_alloc(size: usize) -> Option<*mut u8> {
        global_pool_alloc(size)
    }

    /// Освобождает память ядра
    pub fn kernel_dealloc(ptr: *mut u8, size: usize) -> Result<(), &'static str> {
        global_pool_dealloc(ptr, size)
    }

    /// Выделяет память для процесса
    pub fn process_alloc(size: usize) -> Option<*mut u8> {
        global_pool_alloc(size)
    }

    /// Освобождает память процесса
    pub fn process_dealloc(ptr: *mut u8, size: usize) -> Result<(), &'static str> {
        global_pool_dealloc(ptr, size)
    }
}

/// Макросы для удобства
#[macro_export]
macro_rules! pool_alloc {
    ($size:expr) => {
        $crate::memory_pool::global_pool_alloc($size)
    };
}

#[macro_export]
macro_rules! pool_dealloc {
    ($ptr:expr, $size:expr) => {
        let _ = $crate::memory_pool::global_pool_dealloc($ptr, $size);
    };
}

#[macro_export]
macro_rules! with_pool_alloc {
    ($size:expr, |$ptr:ident| $body:block) => {
        if let Some($ptr) = $crate::memory_pool::global_pool_alloc($size) {
            $body
            $crate::memory_pool::global_pool_dealloc($ptr, $size).ok();
        }
    };
}

// Инициализация
pub fn init_memory_pools() {
    let _ = get_memory_pool_manager(); // Инициализируем однажды
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_basic_operations() {
        let mut pool = MemoryPool::new(1024, 8);

        // Выделяем первый блок
        let ptr1 = pool.alloc(32).expect("Should allocate");
        assert!(!ptr1.is_null());

        // Выделяем второй блок
        let ptr2 = pool.alloc(64).expect("Should allocate");
        assert!(!ptr2.is_null());

        // Проверяем, что блоки не пересекаются
        assert_ne!(ptr1, ptr2);

        // Освобождаем первый блок
        pool.dealloc(ptr1).expect("Should dealloc");

        // Освобождаем второй блок
        pool.dealloc(ptr2).expect("Should dealloc");
    }
}
