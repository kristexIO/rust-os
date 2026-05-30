use alloc::{collections::BTreeMap, vec::Vec, string::String};
use spin::Mutex;
use x86_64::registers::model_specific::Msr;

/// Profiler - система профилирования для анализа производительности
/// 
/// Позволяет измерять:
/// - Время выполнения функций
/// - Частоту вызовов
/// - Использование ЦП
/// - Использование памяти
pub struct Profiler {
    /// Карта профилирования функций
    function_profiles: BTreeMap<String, FunctionProfile>,
    /// Профилирование системных вызовов
    syscall_profiles: BTreeMap<u64, SyscallProfile>,
    /// Профилирование задач
    task_profiles: BTreeMap<u64, TaskProfile>,
    /// Профилирование прерываний
    interrupt_profiles: BTreeMap<u8, InterruptProfile>,
    /// Статистика по памяти
    memory_stats: MemoryStats,
    /// Общая статистика профилирования
    global_stats: GlobalProfilingStats,
    /// Флаг включения профилирования
    enabled: bool,
}

/// Профиль функции
#[derive(Debug, Clone)]
pub struct FunctionProfile {
    /// Имя функции
    pub name: String,
    /// Общее время выполнения (в циклах)
    pub total_cycles: u64,
    /// Время в наносекундах
    pub total_ns: u64,
    /// Количество вызовов
    pub call_count: u64,
    /// Максимальное время выполнения
    pub max_cycles: u64,
    /// Минимальное время выполнения
    pub min_cycles: u64,
    /// Среднее время выполнения
    pub avg_cycles: u64,
    /// Время последнего вызова
    pub last_call_time: u64,
    /// Профиль вызовов (для анализа дерева вызовов)
    pub call_tree: CallTreeNode,
}

/// Узел дерева вызовов
#[derive(Debug, Clone)]
pub struct CallTreeNode {
    /// Имя функции
    pub name: String,
    /// Дочерние вызовы
    pub children: Vec<CallTreeNode>,
    /// Время выполнения
    pub self_cycles: u64,
    /// Время выполнения с детьми
    pub inclusive_cycles: u64,
}

/// Профиль системного вызова
#[derive(Debug, Clone)]
pub struct SyscallProfile {
    /// Номер системного вызова
    pub number: u64,
    /// Общее время выполнения
    pub total_cycles: u64,
    /// Количество вызовов
    pub call_count: u64,
    /// Среднее время
    pub avg_time: u64,
    /// Максимальное время
    pub max_time: u64,
    /// Минимальное время
    pub min_time: u64,
}

/// Профиль задачи
#[derive(Debug, Clone)]
pub struct TaskProfile {
    /// ID задачи
    pub task_id: u64,
    /// Имя задачи
    pub name: String,
    /// Время ЦП (в циклах)
    pub cpu_cycles: u64,
    /// Время ЦП в наносекундах
    pub cpu_time_ns: u64,
    /// Количество переключений контекста
    pub context_switches: u64,
    /// Количество прерываний
    pub interrupt_count: u64,
    /// Использование памяти
    pub memory_usage: MemoryUsage,
}

/// Профиль прерывания
#[derive(Debug, Clone)]
pub struct InterruptProfile {
    /// Номер прерывания
    pub vector: u8,
    /// Время выполнения обработчика
    pub total_cycles: u64,
    /// Количество вызовов
    pub call_count: u64,
    /// Среднее время
    pub avg_time: u64,
    /// Максимальное время
    pub max_time: u64,
}

/// Статистика памяти
#[derive(Debug, Clone)]
pub struct MemoryStats {
    /// Выделено памяти
    pub allocated_bytes: u64,
    /// Освобождено памяти
    pub deallocated_bytes: u64,
    /// Текущее использование
    pub current_usage: u64,
    /// Пиковое использование
    pub peak_usage: u64,
    /// Количество выделений
    pub allocation_count: u64,
    /// Количество освобождений
    pub deallocation_count: u64,
}

/// Статистика задачи
#[derive(Debug, Clone)]
pub struct MemoryUsage {
    /// Виртуальная память
    pub virtual_memory: u64,
    /// Резидентная память
    pub resident_memory: u64,
    /// Пиковое значение
    pub peak_memory: u64,
}

/// Глобальная статистика профилирования
#[derive(Debug, Clone)]
pub struct GlobalProfilingStats {
    /// Общее время профилирования (в миллисекундах)
    pub profiling_duration: u64,
    /// Общее количество профилированных функций
    pub profiled_functions: u64,
    /// Общее количество профилированных системных вызовов
    pub profiled_syscalls: u64,
    /// Общее количество профилированных задач
    pub profiled_tasks: u64,
    /// Общее количество профилированных прерываний
    pub profiled_interrupts: u64,
    /// Начало профилирования
    pub start_time: u64,
    /// Последнее обновление
    pub last_update: u64,
}

impl Profiler {
    /// Создает новый профайлер
    pub fn new() -> Self {
        Profiler {
            function_profiles: BTreeMap::new(),
            syscall_profiles: BTreeMap::new(),
            task_profiles: BTreeMap::new(),
            interrupt_profiles: BTreeMap::new(),
            memory_stats: MemoryStats {
                allocated_bytes: 0,
                deallocated_bytes: 0,
                current_usage: 0,
                peak_usage: 0,
                allocation_count: 0,
                deallocation_count: 0,
            },
            global_stats: GlobalProfilingStats {
                profiling_duration: 0,
                profiled_functions: 0,
                profiled_syscalls: 0,
                profiled_tasks: 0,
                profiled_interrupts: 0,
                start_time: crate::timer::get_uptime_ms(),
                last_update: 0,
            },
            enabled: false,
        }
    }

    /// Включает профилирование
    pub fn enable(&mut self) {
        self.enabled = true;
        self.global_stats.start_time = crate::timer::get_uptime_ms();
    }

    /// Выключает профилирование
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Проверяет, включено ли профилирование
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Начинает профилирование функции
    pub fn start_function_profile(&mut self, function_name: &str) -> u64 {
        if !self.enabled {
            return 0;
        }

        let start_time = self.get_cycle_count();

        self.function_profiles.entry(function_name.to_string())
            .or_insert_with(|| FunctionProfile::new(function_name.to_string()));

        start_time
    }

    /// Завершает профилирование функции
    pub fn end_function_profile(&mut self, function_name: &str, start_time: u64) {
        if !self.enabled {
            return;
        }

        let end_time = self.get_cycle_count();
        let duration = end_time.saturating_sub(start_time);

        if let Some(profile) = self.function_profiles.get_mut(function_name) {
            profile.update(duration, start_time);
            self.global_stats.last_update = crate::timer::get_uptime_ms();
        }
    }

    /// Начинает профилирование системного вызова
    pub fn start_syscall_profile(&mut self, syscall_number: u64) -> u64 {
        if !self.enabled {
            return 0;
        }

        let start_time = self.get_cycle_count();
        self.syscall_profiles.entry(syscall_number)
            .or_insert_with(|| SyscallProfile::new(syscall_number));

        start_time
    }

    /// Завершает профилирование системного вызова
    pub fn end_syscall_profile(&mut self, syscall_number: u64, start_time: u64) {
        if !self.enabled {
            return;
        }

        let end_time = self.get_cycle_count();
        let duration = end_time.saturating_sub(start_time);

        if let Some(profile) = self.syscall_profiles.get_mut(&syscall_number) {
            profile.update(duration);
            self.global_stats.last_update = crate::timer::get_uptime_ms();
        }
    }

    /// Обновляет статистику задачи
    pub fn update_task_profile(&mut self, task_id: u64, cycles: u64, name: &str) {
        if !self.enabled {
            return;
        }

        self.task_profiles.entry(task_id)
            .or_insert_with(|| TaskProfile::new(task_id, name.to_string()))
            .cpu_cycles += cycles;

        self.global_stats.last_update = crate::timer::get_uptime_ms();
    }

    /// Обновляет статистику прерывания
    pub fn update_interrupt_profile(&mut self, vector: u8, cycles: u64) {
        if !self.enabled {
            return;
        }

        self.interrupt_profiles.entry(vector)
            .or_insert_with(|| InterruptProfile::new(vector))
            .total_cycles += cycles;

        self.global_stats.last_update = crate::timer::get_uptime_ms();
    }

    /// Обновляет статистику памяти
    pub fn update_memory_stats(&mut self, allocated: bool, size: usize) {
        if !self.enabled {
            return;
        }

        if allocated {
            self.memory_stats.allocated_bytes += size as u64;
            self.memory_stats.current_usage += size as u64;
            self.memory_stats.allocation_count += 1;

            if self.memory_stats.current_usage > self.memory_stats.peak_usage {
                self.memory_stats.peak_usage = self.memory_stats.current_usage;
            }
        } else {
            self.memory_stats.deallocated_bytes += size as u64;
            self.memory_stats.current_usage = self.memory_stats.current_usage.saturating_sub(size as u64);
            self.memory_stats.deallocation_count += 1;
        }

        self.global_stats.last_update = crate::timer::get_uptime_ms();
    }

    /// Получает профиль функции
    pub fn get_function_profile(&self, function_name: &str) -> Option<&FunctionProfile> {
        self.function_profiles.get(function_name)
    }

    /// Получает профиль системного вызова
    pub fn get_syscall_profile(&self, syscall_number: u64) -> Option<&SyscallProfile> {
        self.syscall_profiles.get(&syscall_number)
    }

    /// Получает профиль задачи
    pub fn get_task_profile(&self, task_id: u64) -> Option<&TaskProfile> {
        self.task_profiles.get(&task_id)
    }

    /// Получает профиль прерывания
    pub fn get_interrupt_profile(&self, vector: u8) -> Option<&InterruptProfile> {
        self.interrupt_profiles.get(&vector)
    }

    /// Получает статистику памяти
    pub fn get_memory_stats(&self) -> &MemoryStats {
        &self.memory_stats
    }

    /// Получает глобальную статистику
    pub fn get_global_stats(&self) -> &GlobalProfilingStats {
        &self.global_stats
    }

    /// Получает все профили функций
    pub fn get_all_function_profiles(&self) -> Vec<(&String, &FunctionProfile)> {
        self.function_profiles.iter().collect()
    }

    /// Получает все профили системных вызовов
    pub fn get_all_syscall_profiles(&self) -> Vec<(&u64, &SyscallProfile)> {
        self.syscall_profiles.iter().collect()
    }

    /// Получает все профили задач
    pub fn get_all_task_profiles(&self) -> Vec<(&u64, &TaskProfile)> {
        self.task_profiles.iter().collect()
    }

    /// Получает все профили прерываний
    pub fn get_all_interrupt_profiles(&self) -> Vec<(&u8, &InterruptProfile)> {
        self.interrupt_profiles.iter().collect()
    }

    /// Сбрасывает статистику
    pub fn reset_stats(&mut self) {
        self.function_profiles.clear();
        self.syscall_profiles.clear();
        self.task_profiles.clear();
        self.interrupt_profiles.clear();
        
        self.memory_stats = MemoryStats {
            allocated_bytes: 0,
            deallocated_bytes: 0,
            current_usage: 0,
            peak_usage: 0,
            allocation_count: 0,
            deallocation_count: 0,
        };
        
        self.global_stats = GlobalProfilingStats {
            profiling_duration: 0,
            profiled_functions: 0,
            profiled_syscalls: 0,
            profiled_tasks: 0,
            profiled_interrupts: 0,
            start_time: crate::timer::get_uptime_ms(),
            last_update: 0,
        };
    }

    /// Получает топ N самых медленных функций
    pub fn get_slowest_functions(&self, n: usize) -> Vec<(&String, &FunctionProfile)> {
        let mut profiles: Vec<_> = self.function_profiles.iter().collect();
        profiles.sort_by(|a, b| b.1.avg_cycles.cmp(&a.1.avg_cycles));
        profiles.into_iter().take(n).collect()
    }

    /// Получает топ N самых часто вызываемых функций
    pub fn get_most_called_functions(&self, n: usize) -> Vec<(&String, &FunctionProfile)> {
        let mut profiles: Vec<_> = self.function_profiles.iter().collect();
        profiles.sort_by(|a, b| b.1.call_count.cmp(&a.1.call_count));
        profiles.into_iter().take(n).collect()
    }

    /// Получает топ N самых медленных системных вызовов
    pub fn get_slowest_syscalls(&self, n: usize) -> Vec<(&u64, &SyscallProfile)> {
        let mut profiles: Vec<_> = self.syscall_profiles.iter().collect();
        profiles.sort_by(|a, b| b.1.avg_time.cmp(&a.1.avg_time));
        profiles.into_iter().take(n).collect()
    }

    /// Получает топ N самых медленных прерываний
    pub fn get_slowest_interrupts(&self, n: usize) -> Vec<(&u8, &InterruptProfile)> {
        let mut profiles: Vec<_> = self.interrupt_profiles.iter().collect();
        profiles.sort_by(|a, b| b.1.avg_time.cmp(&a.1.avg_time));
        profiles.into_iter().take(n).collect()
    }

    /// Получает время в циклах процессора
    fn get_cycle_count(&self) -> u64 {
        // Используем RDTSC (Read Time-Stamp Counter) для получения циклов
        unsafe {
            core::arch::x86_64::_rdtsc()
        }
    }
}

impl FunctionProfile {
    pub fn new(name: String) -> Self {
        FunctionProfile {
            name,
            total_cycles: 0,
            total_ns: 0,
            call_count: 0,
            max_cycles: 0,
            min_cycles: u64::MAX,
            avg_cycles: 0,
            last_call_time: 0,
            call_tree: CallTreeNode::new("<root>".to_string()),
        }
    }

    pub fn update(&mut self, duration: u64, call_time: u64) {
        self.total_cycles += duration;
        self.call_count += 1;
        
        if duration > self.max_cycles {
            self.max_cycles = duration;
        }
        
        if duration < self.min_cycles {
            self.min_cycles = duration;
        }
        
        self.avg_cycles = self.total_cycles / self.call_count.max(1);
        self.last_call_time = call_time;
    }
}

impl SyscallProfile {
    pub fn new(number: u64) -> Self {
        SyscallProfile {
            number,
            total_cycles: 0,
            call_count: 0,
            avg_time: 0,
            max_time: 0,
            min_time: u64::MAX,
        }
    }

    pub fn update(&mut self, duration: u64) {
        self.total_cycles += duration;
        self.call_count += 1;
        
        if duration > self.max_time {
            self.max_time = duration;
        }
        
        if duration < self.min_time {
            self.min_time = duration;
        }
        
        self.avg_time = self.total_cycles / self.call_count.max(1);
    }
}

impl TaskProfile {
    pub fn new(task_id: u64, name: String) -> Self {
        TaskProfile {
            task_id,
            name,
            cpu_cycles: 0,
            cpu_time_ns: 0,
            context_switches: 0,
            interrupt_count: 0,
            memory_usage: MemoryUsage {
                virtual_memory: 0,
                resident_memory: 0,
                peak_memory: 0,
            },
        }
    }
}

impl InterruptProfile {
    pub fn new(vector: u8) -> Self {
        InterruptProfile {
            vector,
            total_cycles: 0,
            call_count: 0,
            avg_time: 0,
            max_time: 0,
        }
    }
}

impl CallTreeNode {
    pub fn new(name: String) -> Self {
        CallTreeNode {
            name,
            children: Vec::new(),
            self_cycles: 0,
            inclusive_cycles: 0,
        }
    }
}

/// Глобальный профайлер
use spin::Once;

static PROFILER: Once<Mutex<Profiler>> = Once::new();

pub fn get_profiler() -> &'static Mutex<Profiler> {
    PROFILER.call_once(|| Mutex::new(Profiler::new()))
}

/// Интерфейс для профилирования
pub struct ProfilingInterface;

impl ProfilingInterface {
    /// Инициализирует профилирование
    pub fn init_profiling() {
        get_profiler().lock().enable();
    }

    /// Включает профилирование
    pub fn enable_profiling() {
        get_profiler().lock().enable();
    }

    /// Выключает профилирование
    pub fn disable_profiling() {
        get_profiler().lock().disable();
    }

    /// Начинает профилирование функции
    pub fn start_function_profile(function_name: &str) -> u64 {
        get_profiler().lock().start_function_profile(function_name)
    }

    /// Завершает профилирование функции
    pub fn end_function_profile(function_name: &str, start_time: u64) {
        get_profiler().lock().end_function_profile(function_name, start_time);
    }

    /// Начинает профилирование системного вызова
    pub fn start_syscall_profile(syscall_number: u64) -> u64 {
        get_profiler().lock().start_syscall_profile(syscall_number)
    }

    /// Завершает профилирование системного вызова
    pub fn end_syscall_profile(syscall_number: u64, start_time: u64) {
        get_profiler().lock().end_syscall_profile(syscall_number, start_time);
    }

    /// Обновляет статистику задачи
    pub fn update_task_profile(task_id: u64, cycles: u64, name: &str) {
        get_profiler().lock().update_task_profile(task_id, cycles, name);
    }

    /// Обновляет статистику прерывания
    pub fn update_interrupt_profile(vector: u8, cycles: u64) {
        get_profiler().lock().update_interrupt_profile(vector, cycles);
    }

    /// Обновляет статистику памяти
    pub fn update_memory_stats(allocated: bool, size: usize) {
        get_profiler().lock().update_memory_stats(allocated, size);
    }

    /// Получает профиль функции
    pub fn get_function_profile(function_name: &str) -> Option<FunctionProfile> {
        get_profiler().lock().get_function_profile(function_name).cloned()
    }

    /// Получает профиль системного вызова
    pub fn get_syscall_profile(syscall_number: u64) -> Option<SyscallProfile> {
        get_profiler().lock().get_syscall_profile(syscall_number).cloned()
    }

    /// Получает профиль задачи
    pub fn get_task_profile(task_id: u64) -> Option<TaskProfile> {
        get_profiler().lock().get_task_profile(task_id).cloned()
    }

    /// Получает профиль прерывания
    pub fn get_interrupt_profile(vector: u8) -> Option<InterruptProfile> {
        get_profiler().lock().get_interrupt_profile(vector).cloned()
    }

    /// Получает статистику памяти
    pub fn get_memory_stats() -> MemoryStats {
        get_profiler().lock().get_memory_stats().clone()
    }

    /// Получает глобальную статистику
    pub fn get_global_stats() -> GlobalProfilingStats {
        get_profiler().lock().get_global_stats().clone()
    }

    /// Получает топ медленных функций
    pub fn get_slowest_functions(n: usize) -> Vec<(String, FunctionProfile)> {
        let profiler = get_profiler().lock();
        profiler.get_slowest_functions(n)
            .iter()
            .map(|(name, profile)| (name.clone(), profile.clone()))
            .collect()
    }

    /// Получает топ самых часто вызываемых функций
    pub fn get_most_called_functions(n: usize) -> Vec<(String, FunctionProfile)> {
        let profiler = get_profiler().lock();
        profiler.get_most_called_functions(n)
            .iter()
            .map(|(name, profile)| (name.clone(), profile.clone()))
            .collect()
    }

    /// Сбрасывает статистику
    pub fn reset_stats() {
        get_profiler().lock().reset_stats();
    }

    /// Показывает отчет о производительности
    pub fn print_performance_report() {
        let profiler = get_profiler().lock();
        
        crate::println!("=== Performance Profile Report ===");
        crate::println!("Total Functions Profiled: {}", profiler.function_profiles.len());
        crate::println!("Total Syscalls Profiled: {}", profiler.syscall_profiles.len());
        crate::println!("Total Tasks Profiled: {}", profiler.task_profiles.len());
        crate::println!("Total Interrupts Profiled: {}", profiler.interrupt_profiles.len());
        
        crate::println!("\nTop 5 Slowest Functions:");
        for (i, (name, profile)) in profiler.get_slowest_functions(5).iter().enumerate() {
            crate::println!("  {}: {} - {} cycles (avg: {})", 
                           i + 1, name, profile.total_cycles, profile.avg_cycles);
        }
        
        crate::println!("\nTop 5 Most Called Functions:");
        for (i, (name, profile)) in profiler.get_most_called_functions(5).iter().enumerate() {
            crate::println!("  {}: {} - {} calls", i + 1, name, profile.call_count);
        }
        
        crate::println!("\nMemory Stats:");
        crate::println!("  Allocated: {} bytes", profiler.memory_stats.allocated_bytes);
        crate::println!("  Deallocated: {} bytes", profiler.memory_stats.deallocated_bytes);
        crate::println!("  Current Usage: {} bytes", profiler.memory_stats.current_usage);
        crate::println!("  Peak Usage: {} bytes", profiler.memory_stats.peak_usage);
        crate::println!("  Allocation Count: {}", profiler.memory_stats.allocation_count);
        crate::println!("  Deallocation Count: {}", profiler.memory_stats.deallocation_count);
    }
}

/// Макросы для профилирования
#[macro_export]
macro_rules! profile_function {
    ($name:expr) => {
        let start_time = $crate::profiling::ProfilingInterface::start_function_profile($name);
        defer!({
            $crate::profiling::ProfilingInterface::end_function_profile($name, start_time);
        });
    };
}

#[macro_export]
macro_rules! profile_syscall {
    ($num:expr) => {
        let start_time = $crate::profiling::ProfilingInterface::start_syscall_profile($num);
        defer!({
            $crate::profiling::ProfilingInterface::end_syscall_profile($num, start_time);
        });
    };
}

#[macro_export]
macro_rules! profile_block {
    ($name:expr, $block:block) => {{
        let start_time = $crate::profiling::ProfilingInterface::start_function_profile($name);
        let result = $block;
        $crate::profiling::ProfilingInterface::end_function_profile($name, start_time);
        result
    }};
}

#[macro_export]
macro_rules! update_task_profile {
    ($task_id:expr, $cycles:expr, $name:expr) => {
        $crate::profiling::ProfilingInterface::update_task_profile($task_id, $cycles, $name);
    };
}

#[macro_export]
macro_rules! update_memory_stats {
    ($allocated:expr, $size:expr) => {
        $crate::profiling::ProfilingInterface::update_memory_stats($allocated, $size);
    };
}

// Утилита для измерения времени выполнения
pub struct Timer;

impl Timer {
    /// Измеряет время выполнения функции и возвращает результат с временем
    pub fn measure<T, F>(func: F) -> (T, u64)
    where
        F: FnOnce() -> T,
    {
        let start = unsafe { core::arch::x86_64::_rdtsc() };
        let result = func();
        let end = unsafe { core::arch::x86_64::_rdtsc() };
        (result, end.saturating_sub(start))
    }
    
    /// Измеряет время выполнения асинхронной функции
    pub async fn measure_async<T, F, Fut>(func: F) -> (T, u64)
    where
        F: FnOnce() -> Fut,
        Fut: core::future::Future<Output = T>,
    {
        let start = unsafe { core::arch::x86_64::_rdtsc() };
        let result = func().await;
        let end = unsafe { core::arch::x86_64::_rdtsc() };
        (result, end.saturating_sub(start))
    }
}

/// Статистика производительности
pub struct PerformanceStats {
    /// Процент использования ЦП
    pub cpu_usage_percent: f64,
    /// Количество задач в секунду
    pub tasks_per_second: u64,
    /// Количество системных вызовов в секунду
    pub syscalls_per_second: u64,
    /// Количество прерываний в секунду
    pub interrupts_per_second: u64,
    /// Изменение использования памяти
    pub memory_change_per_second: i64,
    /// Среднее время выполнения задачи
    pub avg_task_execution_time: u64,
}

impl PerformanceStats {
    /// Получает текущую статистику производительности
    pub fn get_current_stats() -> Self {
        let profiler = get_profiler().lock();
        let global_stats = profiler.get_global_stats();
        
        // В реальной системе эти значения вычисляются на основе данных за последнюю секунду
        // Пока возвращаем фиктивные значения
        
        PerformanceStats {
            cpu_usage_percent: 0.0,
            tasks_per_second: 0,
            syscalls_per_second: 0,
            interrupts_per_second: 0,
            memory_change_per_second: 0,
            avg_task_execution_time: 0,
        }
    }
}

/// Анализ производительности
pub struct PerformanceAnalyzer;

impl PerformanceAnalyzer {
    /// Выполняет анализ производительности и возвращает отчет
    pub fn analyze_performance() -> PerformanceReport {
        let profiler = get_profiler().lock();
        
        PerformanceReport {
            slowest_functions: profiler.get_slowest_functions(10),
            most_called_functions: profiler.get_most_called_functions(10),
            slowest_syscalls: profiler.get_slowest_syscalls(5),
            slowest_interrupts: profiler.get_slowest_interrupts(5),
            memory_stats: profiler.get_memory_stats().clone(),
            overall_stats: profiler.get_global_stats().clone(),
        }
    }

    /// Получает рекомендации по оптимизации
    pub fn get_optimization_recommendations() -> Vec<OptimizationRecommendation> {
        let mut recommendations = Vec::new();
        let profiler = get_profiler().lock();
        
        // Находим самые медленные функции
        for (name, profile) in profiler.get_slowest_functions(5) {
            if profile.avg_cycles > 1000000 { // Если больше 1M циклов в среднем
                recommendations.push(OptimizationRecommendation::FunctionOptimization {
                    function_name: name.clone(),
                    average_cycles: profile.avg_cycles,
                    recommendation: "Consider optimizing this function - takes too long to execute".to_string(),
                });
            }
        }
        
        recommendations
    }
}

/// Отчет о производительности
pub struct PerformanceReport<'a> {
    pub slowest_functions: Vec<(&'a String, &'a FunctionProfile)>,
    pub most_called_functions: Vec<(&'a String, &'a FunctionProfile)>,
    pub slowest_syscalls: Vec<(&'a u64, &'a SyscallProfile)>,
    pub slowest_interrupts: Vec<(&'a u8, &'a InterruptProfile)>,
    pub memory_stats: MemoryStats,
    pub overall_stats: GlobalProfilingStats,
}

/// Рекомендация по оптимизации
pub enum OptimizationRecommendation {
    FunctionOptimization {
        function_name: String,
        average_cycles: u64,
        recommendation: String,
    },
    SyscallOptimization {
        syscall_number: u64,
        average_time: u64,
        recommendation: String,
    },
    MemoryOptimization {
        current_usage: u64,
        peak_usage: u64,
        recommendation: String,
    },
}

/// Профилирование памяти
pub struct MemoryProfiler;

impl MemoryProfiler {
    /// Измеряет использование памяти для конкретного действия
    pub fn measure_memory_usage<T, F>(func: F) -> (T, MemoryUsageDelta)
    where
        F: FnOnce() -> T,
    {
        let initial_usage = get_profiler().lock().get_memory_stats().current_usage;
        let result = func();
        let final_usage = get_profiler().lock().get_memory_stats().current_usage;
        
        let delta = MemoryUsageDelta {
            initial: initial_usage,
            final_usage,
            difference: final_usage as i64 - initial_usage as i64,
        };
        
        (result, delta)
    }
}

/// Изменение использования памяти
#[derive(Debug, Clone)]
pub struct MemoryUsageDelta {
    pub initial: u64,
    pub final_usage: u64,
    pub difference: i64,
}

// Инициализация при запуске системы
pub fn init_profiling_system() {
    let _ = get_profiler(); // Инициализируем однажды
}