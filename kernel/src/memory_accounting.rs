use crate::timer::get_uptime_ms;
use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
};
use spin::Mutex;
use x86_64::{PhysAddr, VirtAddr};

/// MemoryAccounting - система учета использования памяти
///
/// Отслеживает, сколько памяти используется каждым процессом, подсистемой ядра,
/// драйвером и т.д. для эффективного управления ресурсами.
pub struct MemoryAccountant {
    /// Учет памяти процессов
    process_accounts: BTreeMap<u64, ProcessMemoryUsage>,
    /// Учет памяти подсистем ядра
    kernel_accounts: BTreeMap<String, KernelMemoryUsage>,
    /// Учет памяти драйверов
    driver_accounts: BTreeMap<String, DriverMemoryUsage>,
    /// Глобальная статистика
    global_stats: GlobalMemoryStats,
    /// Ограничения на память
    memory_limits: BTreeMap<u64, MemoryLimit>,
}

/// Информация об использовании памяти процессом
#[derive(Debug, Clone)]
pub struct ProcessMemoryUsage {
    /// PID процесса
    pub pid: u64,
    /// Виртуальная память
    pub virtual_memory: usize,
    /// Физическая память (резидентный набор)
    pub resident_set: usize,
    /// Общее выделение памяти
    pub total_allocated: usize,
    /// Общее освобождение памяти
    pub total_deallocated: usize,
    /// Пиковое использование
    pub peak_usage: usize,
    /// Время последнего обновления
    pub last_update: u64,
    /// Имя процесса (для отладки)
    pub name: String,
}

/// Информация об использовании памяти подсистемой ядра
#[derive(Debug, Clone)]
pub struct KernelMemoryUsage {
    /// Название подсистемы
    pub name: String,
    /// Выделено памяти
    pub allocated: usize,
    /// Освобождено памяти
    pub deallocated: usize,
    /// Текущее использование
    pub current_usage: usize,
    /// Пиковое использование
    pub peak_usage: usize,
    /// Время последнего обновления
    pub last_update: u64,
}

/// Информация об использовании памяти драйвером
#[derive(Debug, Clone)]
pub struct DriverMemoryUsage {
    /// Название драйвера
    pub name: String,
    /// Выделено памяти
    pub allocated: usize,
    /// Освобождено памяти
    pub deallocated: usize,
    /// Текущее использование
    pub current_usage: usize,
    /// Пиковое использование
    pub peak_usage: usize,
    /// Время последнего обновления
    pub last_update: u64,
}

/// Глобальная статистика памяти
#[derive(Debug, Clone)]
pub struct GlobalMemoryStats {
    /// Всего физической памяти
    pub total_memory: usize,
    /// Свободной памяти
    pub free_memory: usize,
    /// Используемой памяти
    pub used_memory: usize,
    /// Общее выделение
    pub allocations: usize,
    /// Общее освобождение
    pub deallocations: usize,
    /// Количество процессов
    pub active_processes: usize,
    /// Количество подсистем
    pub active_subsystems: usize,
    /// Количество драйверов
    pub active_drivers: usize,
    /// Время последнего обновления
    pub last_update: u64,
}

/// Ограничение памяти для процесса
#[derive(Debug, Clone)]
pub struct MemoryLimit {
    /// Максимальное количество виртуальной памяти
    pub max_virtual_memory: usize,
    /// Максимальное количество физической памяти
    pub max_physical_memory: usize,
    /// Лимит на выделение за одну операцию
    pub max_single_allocation: usize,
    /// Время сброса квоты (в миллисекундах)
    pub quota_reset_interval: u64,
}

impl MemoryAccountant {
    /// Создает новый счетчик памяти
    pub fn new(total_memory: usize) -> Self {
        MemoryAccountant {
            process_accounts: BTreeMap::new(),
            kernel_accounts: BTreeMap::new(),
            driver_accounts: BTreeMap::new(),
            global_stats: GlobalMemoryStats {
                total_memory,
                free_memory: total_memory,
                used_memory: 0,
                allocations: 0,
                deallocations: 0,
                active_processes: 0,
                active_subsystems: 0,
                active_drivers: 0,
                last_update: get_uptime_ms(),
            },
            memory_limits: BTreeMap::new(),
        }
    }

    /// Регистрирует выделение памяти процессом
    pub fn register_process_allocation(&mut self, pid: u64, size: usize) {
        let current_time = get_uptime_ms();

        // Обновляем учет процесса
        let process_usage =
            self.process_accounts
                .entry(pid)
                .or_insert_with(|| ProcessMemoryUsage {
                    pid,
                    virtual_memory: 0,
                    resident_set: 0,
                    total_allocated: 0,
                    total_deallocated: 0,
                    peak_usage: 0,
                    last_update: current_time,
                    name: format!("Process-{}", pid),
                });

        process_usage.virtual_memory += size;
        process_usage.resident_set += size;
        process_usage.total_allocated += size;

        // Обновляем пиковое значение
        if process_usage.virtual_memory > process_usage.peak_usage {
            process_usage.peak_usage = process_usage.virtual_memory;
        }

        process_usage.last_update = current_time;

        // Обновляем глобальные статистики
        self.global_stats.used_memory += size;
        self.global_stats.allocations += 1;
        self.global_stats.free_memory = self
            .global_stats
            .total_memory
            .saturating_sub(self.global_stats.used_memory);
        self.global_stats.last_update = current_time;
    }

    /// Регистрирует освобождение памяти процессом
    pub fn register_process_deallocation(&mut self, pid: u64, size: usize) {
        if let Some(process_usage) = self.process_accounts.get_mut(&pid) {
            let current_time = get_uptime_ms();

            process_usage.virtual_memory = process_usage.virtual_memory.saturating_sub(size);
            process_usage.resident_set = process_usage.resident_set.saturating_sub(size);
            process_usage.total_deallocated += size;
            process_usage.last_update = current_time;

            // Обновляем глобальные статистики
            self.global_stats.used_memory = self.global_stats.used_memory.saturating_sub(size);
            self.global_stats.deallocations += 1;
            self.global_stats.free_memory = self
                .global_stats
                .total_memory
                .saturating_sub(self.global_stats.used_memory);
            self.global_stats.last_update = current_time;
        }
    }

    /// Регистрирует выделение памяти подсистемой ядра
    pub fn register_kernel_allocation(&mut self, subsystem: &str, size: usize) {
        let current_time = get_uptime_ms();

        let subsystem_usage = self
            .kernel_accounts
            .entry(subsystem.to_string())
            .or_insert_with(|| KernelMemoryUsage {
                name: subsystem.to_string(),
                allocated: 0,
                deallocated: 0,
                current_usage: 0,
                peak_usage: 0,
                last_update: current_time,
            });

        subsystem_usage.allocated += size;
        subsystem_usage.current_usage += size;

        // Обновляем пиковое значение
        if subsystem_usage.current_usage > subsystem_usage.peak_usage {
            subsystem_usage.peak_usage = subsystem_usage.current_usage;
        }

        subsystem_usage.last_update = current_time;

        // Обновляем глобальные статистики
        self.global_stats.used_memory += size;
        self.global_stats.allocations += 1;
        self.global_stats.free_memory = self
            .global_stats
            .total_memory
            .saturating_sub(self.global_stats.used_memory);
        self.global_stats.last_update = current_time;
    }

    /// Регистрирует освобождение памяти подсистемой ядра
    pub fn register_kernel_deallocation(&mut self, subsystem: &str, size: usize) {
        if let Some(subsystem_usage) = self.kernel_accounts.get_mut(subsystem) {
            let current_time = get_uptime_ms();

            subsystem_usage.deallocated += size;
            subsystem_usage.current_usage = subsystem_usage.current_usage.saturating_sub(size);
            subsystem_usage.last_update = current_time;

            // Обновляем глобальные статистики
            self.global_stats.used_memory = self.global_stats.used_memory.saturating_sub(size);
            self.global_stats.deallocations += 1;
            self.global_stats.free_memory = self
                .global_stats
                .total_memory
                .saturating_sub(self.global_stats.used_memory);
            self.global_stats.last_update = current_time;
        }
    }

    /// Регистрирует выделение памяти драйвером
    pub fn register_driver_allocation(&mut self, driver: &str, size: usize) {
        let current_time = get_uptime_ms();

        let driver_usage = self
            .driver_accounts
            .entry(driver.to_string())
            .or_insert_with(|| DriverMemoryUsage {
                name: driver.to_string(),
                allocated: 0,
                deallocated: 0,
                current_usage: 0,
                peak_usage: 0,
                last_update: current_time,
            });

        driver_usage.allocated += size;
        driver_usage.current_usage += size;

        // Обновляем пиковое значение
        if driver_usage.current_usage > driver_usage.peak_usage {
            driver_usage.peak_usage = driver_usage.current_usage;
        }

        driver_usage.last_update = current_time;

        // Обновляем глобальные статистики
        self.global_stats.used_memory += size;
        self.global_stats.allocations += 1;
        self.global_stats.free_memory = self
            .global_stats
            .total_memory
            .saturating_sub(self.global_stats.used_memory);
        self.global_stats.last_update = current_time;
    }

    /// Регистрирует освобождение памяти драйвером
    pub fn register_driver_deallocation(&mut self, driver: &str, size: usize) {
        if let Some(driver_usage) = self.driver_accounts.get_mut(driver) {
            let current_time = get_uptime_ms();

            driver_usage.deallocated += size;
            driver_usage.current_usage = driver_usage.current_usage.saturating_sub(size);
            driver_usage.last_update = current_time;

            // Обновляем глобальные статистики
            self.global_stats.used_memory = self.global_stats.used_memory.saturating_sub(size);
            self.global_stats.deallocations += 1;
            self.global_stats.free_memory = self
                .global_stats
                .total_memory
                .saturating_sub(self.global_stats.used_memory);
            self.global_stats.last_update = current_time;
        }
    }

    /// Устанавливает лимит памяти для процесса
    pub fn set_memory_limit(&mut self, pid: u64, limit: MemoryLimit) {
        self.memory_limits.insert(pid, limit);
    }

    /// Проверяет, не превышает ли процесс лимит памяти
    pub fn check_memory_limit(&self, pid: u64, requested_size: usize) -> bool {
        if let Some(limit) = self.memory_limits.get(&pid) {
            if let Some(process) = self.process_accounts.get(&pid) {
                // Проверяем лимит на разовое выделение
                if requested_size > limit.max_single_allocation {
                    return false;
                }

                // Проверяем общий лимит виртуальной памяти
                if process.virtual_memory + requested_size > limit.max_virtual_memory {
                    return false;
                }

                // Проверяем лимит на физическую память
                if process.resident_set + requested_size > limit.max_physical_memory {
                    return false;
                }
            }
        }

        true // Лимиты не установлены или не превышены
    }

    /// Получает информацию о памяти процесса
    pub fn get_process_memory_usage(&self, pid: u64) -> Option<&ProcessMemoryUsage> {
        self.process_accounts.get(&pid)
    }

    /// Получает информацию о памяти подсистемы
    pub fn get_kernel_memory_usage(&self, subsystem: &str) -> Option<&KernelMemoryUsage> {
        self.kernel_accounts.get(subsystem)
    }

    /// Получает информацию о памяти драйвера
    pub fn get_driver_memory_usage(&self, driver: &str) -> Option<&DriverMemoryUsage> {
        self.driver_accounts.get(driver)
    }

    /// Получает глобальную статистику
    pub fn get_global_stats(&self) -> &GlobalMemoryStats {
        &self.global_stats
    }

    /// Получает список всех процессов с информацией об использовании памяти
    pub fn get_all_process_usage(&self) -> Vec<&ProcessMemoryUsage> {
        self.process_accounts.values().collect()
    }

    /// Получает список всех подсистем с информацией об использовании памяти
    pub fn get_all_kernel_usage(&self) -> Vec<&KernelMemoryUsage> {
        self.kernel_accounts.values().collect()
    }

    /// Получает список всех драйверов с информацией об использовании памяти
    pub fn get_all_driver_usage(&self) -> Vec<&DriverMemoryUsage> {
        self.driver_accounts.values().collect()
    }

    /// Сбрасывает статистику для процесса
    pub fn reset_process_stats(&mut self, pid: u64) {
        if let Some(process) = self.process_accounts.get_mut(&pid) {
            process.virtual_memory = 0;
            process.resident_set = 0;
            process.total_allocated = 0;
            process.total_deallocated = 0;
            process.peak_usage = 0;
        }
    }

    /// Сбрасывает статистику для подсистемы
    pub fn reset_kernel_stats(&mut self, subsystem: &str) {
        if let Some(kernel) = self.kernel_accounts.get_mut(subsystem) {
            kernel.allocated = 0;
            kernel.deallocated = 0;
            kernel.current_usage = 0;
            kernel.peak_usage = 0;
        }
    }

    /// Сбрасывает статистику для драйвера
    pub fn reset_driver_stats(&mut self, driver: &str) {
        if let Some(driver) = self.driver_accounts.get_mut(driver) {
            driver.allocated = 0;
            driver.deallocated = 0;
            driver.current_usage = 0;
            driver.peak_usage = 0;
        }
    }

    /// Очищает учет для завершенного процесса
    pub fn cleanup_process_account(&mut self, pid: u64) {
        if let Some(process) = self.process_accounts.remove(&pid) {
            // Обновляем глобальные статистики
            self.global_stats.used_memory = self
                .global_stats
                .used_memory
                .saturating_sub(process.virtual_memory);
            self.global_stats.free_memory = self
                .global_stats
                .total_memory
                .saturating_sub(self.global_stats.used_memory);
            self.global_stats.active_processes =
                self.global_stats.active_processes.saturating_sub(1);
        }

        // Удаляем лимиты
        self.memory_limits.remove(&pid);
    }

    /// Обновляет статистику активных компонентов
    pub fn update_component_counts(&mut self) {
        self.global_stats.active_processes = self.process_accounts.len();
        self.global_stats.active_subsystems = self.kernel_accounts.len();
        self.global_stats.active_drivers = self.driver_accounts.len();
        self.global_stats.last_update = get_uptime_ms();
    }
}

/// Интерфейс для процессов
pub struct ProcessMemoryAccountant;

impl ProcessMemoryAccountant {
    /// Регистрирует выделение памяти для процесса
    pub fn account_allocation(pid: u64, size: usize) {
        get_memory_accountant()
            .lock()
            .register_process_allocation(pid, size);
    }

    /// Регистрирует освобождение памяти для процесса
    pub fn account_deallocation(pid: u64, size: usize) {
        get_memory_accountant()
            .lock()
            .register_process_deallocation(pid, size);
    }

    /// Проверяет лимит памяти для процесса
    pub fn check_limit(pid: u64, requested_size: usize) -> bool {
        get_memory_accountant()
            .lock()
            .check_memory_limit(pid, requested_size)
    }

    /// Устанавливает лимит памяти для процесса
    pub fn set_limit(pid: u64, limit: MemoryLimit) {
        get_memory_accountant().lock().set_memory_limit(pid, limit);
    }

    /// Получает информацию об использовании памяти процессом
    pub fn get_usage(pid: u64) -> Option<ProcessMemoryUsage> {
        get_memory_accountant()
            .lock()
            .get_process_memory_usage(pid)
            .cloned()
    }
}

/// Интерфейс для подсистем ядра
pub struct KernelMemoryAccountant;

impl KernelMemoryAccountant {
    /// Регистрирует выделение памяти для подсистемы
    pub fn account_allocation(subsystem: &str, size: usize) {
        get_memory_accountant()
            .lock()
            .register_kernel_allocation(subsystem, size);
    }

    /// Регистрирует освобождение памяти для подсистемы
    pub fn account_deallocation(subsystem: &str, size: usize) {
        get_memory_accountant()
            .lock()
            .register_kernel_deallocation(subsystem, size);
    }

    /// Получает информацию об использовании памяти подсистемой
    pub fn get_usage(subsystem: &str) -> Option<KernelMemoryUsage> {
        get_memory_accountant()
            .lock()
            .get_kernel_memory_usage(subsystem)
            .cloned()
    }
}

/// Интерфейс для драйверов
pub struct DriverMemoryAccountant;

impl DriverMemoryAccountant {
    /// Регистрирует выделение памяти для драйвера
    pub fn account_allocation(driver: &str, size: usize) {
        get_memory_accountant()
            .lock()
            .register_driver_allocation(driver, size);
    }

    /// Регистрирует освобождение памяти для драйвера
    pub fn account_deallocation(driver: &str, size: usize) {
        get_memory_accountant()
            .lock()
            .register_driver_deallocation(driver, size);
    }

    /// Получает информацию об использовании памяти драйвером
    pub fn get_usage(driver: &str) -> Option<DriverMemoryUsage> {
        get_memory_accountant()
            .lock()
            .get_driver_memory_usage(driver)
            .cloned()
    }
}

// Глобальный счетчик памяти
use spin::Once;

static MEMORY_ACCOUNTANT: Once<Mutex<MemoryAccountant>> = Once::new();

pub fn get_memory_accountant() -> &'static Mutex<MemoryAccountant> {
    MEMORY_ACCOUNTANT.call_once(|| Mutex::new(MemoryAccountant::new(1024 * 1024 * 1024)))
    // 1GB по умолчанию
}

/// Макросы для удобства использования
#[macro_export]
macro_rules! account_process_alloc {
    ($pid:expr, $size:expr) => {
        $crate::memory_accounting::ProcessMemoryAccountant::account_allocation($pid, $size);
    };
}

#[macro_export]
macro_rules! account_process_dealloc {
    ($pid:expr, $size:expr) => {
        $crate::memory_accounting::ProcessMemoryAccountant::account_deallocation($pid, $size);
    };
}

#[macro_export]
macro_rules! check_process_memory_limit {
    ($pid:expr, $size:expr) => {
        $crate::memory_accounting::ProcessMemoryAccountant::check_limit($pid, $size)
    };
}

#[macro_export]
macro_rules! account_kernel_alloc {
    ($subsystem:expr, $size:expr) => {
        $crate::memory_accounting::KernelMemoryAccountant::account_allocation($subsystem, $size);
    };
}

#[macro_export]
macro_rules! account_kernel_dealloc {
    ($subsystem:expr, $size:expr) => {
        $crate::memory_accounting::KernelMemoryAccountant::account_deallocation($subsystem, $size);
    };
}

#[macro_export]
macro_rules! account_driver_alloc {
    ($driver:expr, $size:expr) => {
        $crate::memory_accounting::DriverMemoryAccountant::account_allocation($driver, $size);
    };
}

#[macro_export]
macro_rules! account_driver_dealloc {
    ($driver:expr, $size:expr) => {
        $crate::memory_accounting::DriverMemoryAccountant::account_deallocation($driver, $size);
    };
}

/// Функции для отладки и мониторинга
pub struct MemoryMonitor;

impl MemoryMonitor {
    /// Выводит информацию об использовании памяти процессом
    pub fn print_process_usage(pid: u64) {
        if let Some(usage) = get_memory_accountant().lock().get_process_memory_usage(pid) {
            crate::println!("Process {} memory usage:", usage.pid);
            crate::println!("  Virtual Memory: {} bytes", usage.virtual_memory);
            crate::println!("  Resident Set: {} bytes", usage.resident_set);
            crate::println!("  Peak Usage: {} bytes", usage.peak_usage);
            crate::println!("  Total Allocated: {} bytes", usage.total_allocated);
            crate::println!("  Total Deallocated: {} bytes", usage.total_deallocated);
        } else {
            crate::println!("Process {} not found in memory accounting", pid);
        }
    }

    /// Выводит информацию об использовании памяти всеми процессами
    pub fn print_all_process_usage() {
        let accountant = get_memory_accountant().lock();
        let processes = accountant.get_all_process_usage();

        crate::println!("Memory usage by processes:");
        for process in processes {
            crate::println!(
                "  PID {}: {} bytes (Peak: {})",
                process.pid,
                process.virtual_memory,
                process.peak_usage
            );
        }
    }

    /// Выводит глобальную статистику
    pub fn print_global_stats() {
        let stats = get_memory_accountant().lock().get_global_stats();
        crate::println!("Global Memory Statistics:");
        crate::println!("  Total Memory: {} bytes", stats.total_memory);
        crate::println!("  Used Memory: {} bytes", stats.used_memory);
        crate::println!("  Free Memory: {} bytes", stats.free_memory);
        crate::println!("  Allocations: {}", stats.allocations);
        crate::println!("  Deallocations: {}", stats.deallocations);
        crate::println!("  Active Processes: {}", stats.active_processes);
        crate::println!("  Active Subsystems: {}", stats.active_subsystems);
        crate::println!("  Active Drivers: {}", stats.active_drivers);
    }

    /// Выводит процессы с наибольшим потреблением памяти
    pub fn print_top_memory_consumers(limit: usize) {
        let accountant = get_memory_accountant().lock();
        let mut processes: Vec<_> = accountant.get_all_process_usage().collect();

        // Сортируем по количеству используемой памяти
        processes.sort_by(|a, b| b.virtual_memory.cmp(&a.virtual_memory));

        crate::println!(
            "Top {} memory-consuming processes:",
            limit.min(processes.len())
        );
        for (i, process) in processes.iter().take(limit).enumerate() {
            crate::println!(
                "  {}.) PID {}: {} bytes",
                i + 1,
                process.pid,
                process.virtual_memory
            );
        }
    }
}

/// Интерфейс для управления ресурсами
pub struct ResourceManager;

impl ResourceManager {
    /// Получает текущее использование памяти в процентах
    pub fn get_memory_usage_percentage() -> f64 {
        let stats = get_memory_accountant().lock().get_global_stats();
        if stats.total_memory > 0 {
            (stats.used_memory as f64 / stats.total_memory as f64) * 100.0
        } else {
            0.0
        }
    }

    /// Проверяет, нужно ли запускать GC или освобождать память
    pub fn should_reclaim_memory(threshold: f64) -> bool {
        ResourceManager::get_memory_usage_percentage() > threshold
    }

    /// Получает список процессов, потребляющих больше лимита
    pub fn get_excessive_memory_processes(limit: usize) -> Vec<u64> {
        let accountant = get_memory_accountant().lock();
        let mut excessive_processes = Vec::new();

        for process in accountant.get_all_process_usage() {
            if process.virtual_memory > limit {
                excessive_processes.push(process.pid);
            }
        }

        excessive_processes
    }
}
