use crate::timer::get_uptime_ms;
use alloc::{collections::BTreeMap, string::String, vec::Vec};
use spin::Mutex;
use x86_64::{PhysAddr, VirtAddr};

/// MemoryLeakDetector - детектор утечек памяти
///
/// Система отслеживает все выделения и освобождения памяти для выявления утечек.
/// Используется в отладочных целях.
pub struct MemoryLeakDetector {
    /// Карты выделенной памяти
    allocated_blocks: BTreeMap<*mut u8, AllocationInfo>,
    /// Статистика
    stats: Stats,
    /// Флаг включенного детектора
    enabled: bool,
}

/// Информация о выделенном блоке
#[derive(Debug, Clone)]
pub struct AllocationInfo {
    /// Размер блока
    pub size: usize,
    /// Адрес возврата (где была вызвана функция выделения)
    pub caller_address: usize,
    /// Имя функции (если доступно)
    pub function_name: String,
    /// Номер строки
    pub line_number: u32,
    /// Имя файла
    pub file_name: String,
    /// Время выделения (в миллисекундах с момента загрузки)
    pub alloc_time: u64,
    /// PID процесса (если применимо)
    pub process_id: Option<u64>,
}

/// Статистика детектора
#[derive(Debug)]
pub struct Stats {
    /// Общее число выделений
    pub total_allocations: usize,
    /// Общее число освобождений
    pub total_deallocations: usize,
    /// Текущее число выделенных блоков
    pub current_allocated_blocks: usize,
    /// Общий объем выделенной памяти
    pub total_allocated_bytes: usize,
    /// Общий объем освобожденной памяти
    pub total_deallocated_bytes: usize,
    /// Текущий объем выделенной памяти (не освобожденной)
    pub current_allocated_bytes: usize,
}

impl MemoryLeakDetector {
    /// Создает новый детектор утечек
    pub fn new() -> Self {
        MemoryLeakDetector {
            allocated_blocks: BTreeMap::new(),
            stats: Stats::new(),
            enabled: false,
        }
    }

    /// Включает детектор
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Выключает детектор
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Регистрирует выделение памяти
    pub fn register_allocation(
        &mut self,
        ptr: *mut u8,
        size: usize,
        function: &str,
        line: u32,
        file: &str,
        process_id: Option<u64>,
    ) {
        if !self.enabled {
            return;
        }

        let info = AllocationInfo {
            size,
            caller_address: 0, // В реальном коде нужно будет получать адрес возврата
            function_name: function.to_string(),
            line_number: line,
            file_name: file.to_string(),
            alloc_time: get_uptime_ms(),
            process_id,
        };

        self.allocated_blocks.insert(ptr, info);

        // Обновляем статистику
        self.stats.total_allocations += 1;
        self.stats.total_allocated_bytes += size;
        self.stats.current_allocated_blocks += 1;
        self.stats.current_allocated_bytes += size;
    }

    /// Регистрирует освобождение памяти
    pub fn register_deallocation(&mut self, ptr: *mut u8) -> Result<AllocationInfo, &'static str> {
        if !self.enabled {
            return Err("Detector disabled");
        }

        if let Some(info) = self.allocated_blocks.remove(&ptr) {
            // Обновляем статистику
            self.stats.total_deallocations += 1;
            self.stats.total_deallocated_bytes += info.size;
            self.stats.current_allocated_blocks -= 1;
            self.stats.current_allocated_bytes -= info.size;

            Ok(info)
        } else {
            Err("Pointer not found in allocations")
        }
    }

    /// Проверяет наличие утечек памяти
    pub fn check_for_leaks(&self) -> Vec<&AllocationInfo> {
        let mut leaks = Vec::new();

        for (ptr, info) in &self.allocated_blocks {
            if !self.is_potential_false_positive(info) {
                leaks.push(info);
            }
        }

        leaks
    }

    /// Проверяет, является ли выделение потенциальной утечкой
    fn is_potential_false_positive(&self, _info: &AllocationInfo) -> bool {
        // Проверяем, не является ли это системным или ядерным блоком,
        // который не должен освобождаться
        // В реальной системе тут будет более сложная логика
        false
    }

    /// Получает информацию о выделении
    pub fn get_allocation_info(&self, ptr: *const u8) -> Option<&AllocationInfo> {
        self.allocated_blocks.get(&(ptr as *mut u8))
    }

    /// Получает статистику
    pub fn get_stats(&self) -> &Stats {
        &self.stats
    }

    /// Сбрасывает статистику
    pub fn reset_stats(&mut self) {
        self.stats = Stats::new();
    }

    /// Удаляет устаревшие блоки (те, что выделены более определенного времени назад)
    pub fn cleanup_old_blocks(&mut self, max_age_ms: u64) -> Vec<AllocationInfo> {
        let current_time = get_uptime_ms();
        let mut old_blocks = Vec::new();
        let mut to_remove = Vec::new();

        for (ptr, info) in &self.allocated_blocks {
            if current_time - info.alloc_time > max_age_ms {
                old_blocks.push(info.clone());
                to_remove.push(*ptr);
            }
        }

        for ptr in to_remove {
            if let Some(info) = self.allocated_blocks.remove(&ptr) {
                // Обновляем статистику
                self.stats.current_allocated_blocks -= 1;
                self.stats.current_allocated_bytes -= info.size;
            }
        }

        old_blocks
    }
}

impl Stats {
    fn new() -> Self {
        Stats {
            total_allocations: 0,
            total_deallocations: 0,
            current_allocated_blocks: 0,
            total_allocated_bytes: 0,
            total_deallocated_bytes: 0,
            current_allocated_bytes: 0,
        }
    }
}

// Глобальный детектор утечек
use spin::Once;

static LEAK_DETECTOR: Once<Mutex<MemoryLeakDetector>> = Once::new();

pub fn get_leak_detector() -> &'static Mutex<MemoryLeakDetector> {
    LEAK_DETECTOR.call_once(|| Mutex::new(MemoryLeakDetector::new()))
}

/// Включает детектор утечек
pub fn enable_leak_detection() {
    get_leak_detector().lock().enable();
}

/// Выключает детектор утечек
pub fn disable_leak_detection() {
    get_leak_detector().lock().disable();
}

/// Регистрирует выделение памяти
pub fn register_allocation(ptr: *mut u8, size: usize, function: &str, line: u32, file: &str) {
    if get_leak_detector().lock().enabled {
        get_leak_detector()
            .lock()
            .register_allocation(ptr, size, function, line, file, None);
    }
}

/// Регистрирует освобождение памяти
pub fn register_deallocation(ptr: *mut u8) -> bool {
    get_leak_detector()
        .lock()
        .register_deallocation(ptr)
        .is_ok()
}

/// Проверяет наличие утечек
pub fn check_memory_leaks() -> Vec<AllocationInfo> {
    let detector = get_leak_detector().lock();
    detector.check_for_leaks().into_iter().cloned().collect()
}

/// Получает информацию о выделении
pub fn get_allocation_info(ptr: *const u8) -> Option<AllocationInfo> {
    get_leak_detector().lock().get_allocation_info(ptr).cloned()
}

/// Получает статистику
pub fn get_leak_stats() -> Stats {
    get_leak_detector().lock().get_stats().clone()
}

/// Сбрасывает статистику
pub fn reset_leak_stats() {
    get_leak_detector().lock().reset_stats();
}

// Макросы для автоматической регистрации выделений
#[macro_export]
macro_rules! track_alloc {
    ($ptr:expr, $size:expr) => {
        if $crate::memory_leak_detection::get_leak_detector()
            .lock()
            .enabled
        {
            $crate::memory_leak_detection::register_allocation(
                $ptr,
                $size,
                core::stringify!($ptr),
                line!(),
                file!(),
            );
        }
    };
    ($ptr:expr, $size:expr, $func:expr) => {
        if $crate::memory_leak_detection::get_leak_detector()
            .lock()
            .enabled
        {
            $crate::memory_leak_detection::register_allocation(
                $ptr,
                $size,
                $func,
                line!(),
                file!(),
            );
        }
    };
}

#[macro_export]
macro_rules! track_dealloc {
    ($ptr:expr) => {
        if $crate::memory_leak_detection::get_leak_detector()
            .lock()
            .enabled
        {
            $crate::memory_leak_detection::register_deallocation($ptr);
        }
    };
}

/// Обертка для выделения с отслеживанием утечек
pub struct LeakedAlloc<T> {
    ptr: *mut T,
    size: usize,
}

impl<T> LeakedAlloc<T> {
    pub fn new(value: T) -> Option<Self> {
        // В реальной системе тут будет выделение физической памяти
        // Используем небезопасный код для создания корректной обертки
        let _size = core::mem::size_of::<T>();

        // Это упрощенная реализация - в продакшене нужно использовать правильный аллокатор
        None // Заглушка
    }

    pub fn as_ptr(&self) -> *const T {
        self.ptr
    }

    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr
    }

    pub fn into_inner(self) -> *mut T {
        let ptr = self.ptr;
        core::mem::forget(self);
        ptr
    }
}

impl<T> Drop for LeakedAlloc<T> {
    fn drop(&mut self) {
        // При удалении автоматически регистрируем освобождение
        if get_leak_detector().lock().enabled {
            register_deallocation(self.ptr as *mut u8);
        }
    }
}

// Интерфейс для интеграции с системными вызовами
pub struct SystemCallTracker;

impl SystemCallTracker {
    /// Отслеживает выделение памяти в системных вызовах
    pub fn track_syscall_allocation(ptr: *mut u8, size: usize, syscall_id: u64) {
        if get_leak_detector().lock().enabled {
            get_leak_detector().lock().register_allocation(
                ptr,
                size,
                &format!("syscall_{}", syscall_id),
                0,
                "syscall",
                None,
            );
        }
    }

    /// Отслеживает освобождение в системных вызовах
    pub fn track_syscall_deallocation(ptr: *mut u8) -> Result<(), &'static str> {
        get_leak_detector().lock().register_deallocation(ptr)
    }
}

// Интерфейс для процессного менеджера
pub struct ProcessMemoryTracker;

impl ProcessMemoryTracker {
    /// Отслеживает память процесса
    pub fn track_process_allocation(process_id: u64, ptr: *mut u8, size: usize) {
        if get_leak_detector().lock().enabled {
            get_leak_detector().lock().register_allocation(
                ptr,
                size,
                &format!("process_{}", process_id),
                0,
                "process",
                Some(process_id),
            );
        }
    }

    /// Отслеживает освобождение памяти процесса
    pub fn track_process_deallocation(
        process_id: u64,
        ptr: *mut u8,
    ) -> Result<AllocationInfo, &'static str> {
        get_leak_detector().lock().register_deallocation(ptr)
    }

    /// Получает список утечек для конкретного процесса
    pub fn get_leaks_for_process(process_id: u64) -> Vec<AllocationInfo> {
        let detector = get_leak_detector().lock();
        detector
            .allocated_blocks
            .values()
            .filter(|info| info.process_id == Some(process_id))
            .cloned()
            .collect()
    }

    /// Удаляет устаревшие блоки для процесса
    pub fn cleanup_process_blocks(process_id: u64, max_age_ms: u64) -> Vec<AllocationInfo> {
        let mut detector = get_leak_detector().lock();
        let mut blocks_to_remove = Vec::new();

        for (ptr, info) in &detector.allocated_blocks {
            if info.process_id == Some(process_id)
                && get_timer::get_uptime_ms() - info.alloc_time > max_age_ms
            {
                blocks_to_remove.push(*ptr);
            }
        }

        let mut removed_blocks = Vec::new();
        for ptr in blocks_to_remove {
            if let Some(info) = detector.allocated_blocks.remove(&ptr) {
                detector.stats.current_allocated_blocks -= 1;
                detector.stats.current_allocated_bytes -= info.size;
                removed_blocks.push(info);
            }
        }

        removed_blocks
    }
}

// Интерфейс для отладки
pub struct DebugLeakChecker;

impl DebugLeakChecker {
    /// Выводит список всех утечек на экран
    pub fn print_leaks() {
        let leaks = check_memory_leaks();
        if leaks.is_empty() {
            crate::println!("No memory leaks detected.");
        } else {
            crate::println!("Detected {} memory leaks:", leaks.len());
            for (i, leak) in leaks.iter().enumerate() {
                crate::println!(
                    "  {}: {} bytes allocated at {}:{} in {}",
                    i + 1,
                    leak.size,
                    leak.file_name,
                    leak.line_number,
                    leak.function_name
                );
            }
        }
    }

    /// Выводит статистику
    pub fn print_stats() {
        let stats = get_leak_stats();
        crate::println!("Memory allocation statistics:");
        crate::println!("  Total allocations: {}", stats.total_allocations);
        crate::println!("  Total deallocations: {}", stats.total_deallocations);
        crate::println!(
            "  Currently allocated blocks: {}",
            stats.current_allocated_blocks
        );
        crate::println!("  Total allocated bytes: {}", stats.total_allocated_bytes);
        crate::println!(
            "  Total deallocated bytes: {}",
            stats.total_deallocated_bytes
        );
        crate::println!(
            "  Currently allocated bytes: {}",
            stats.current_allocated_bytes
        );
    }

    /// Выполняет проверку и выводит результат
    pub fn run_check() {
        DebugLeakChecker::print_stats();
        DebugLeakChecker::print_leaks();
    }
}

// Вспомогательная функция для обновления времени
fn get_timer() -> u64 {
    get_uptime_ms()
}
