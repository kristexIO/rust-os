use crate::timer::get_uptime_ms;
use alloc::{collections::BTreeMap, string::String, vec::Vec};
use spin::Mutex;

/// Структура для трассировки системных вызовов
#[derive(Debug, Clone)]
pub struct SyscallTraceEntry {
    /// Номер системного вызова
    pub syscall_number: u64,
    /// Имя системного вызова
    pub syscall_name: String,
    /// Аргументы
    pub args: [u64; 6],
    /// Время начала выполнения (в миллисекундах с загрузки)
    pub start_time: u64,
    /// Время завершения выполнения
    pub end_time: u64,
    /// Результат выполнения
    pub result: i64,
    /// PID процесса
    pub pid: u64,
    /// Тип результата (успех/ошибка)
    pub result_type: SyscallResultType,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SyscallResultType {
    Success,
    Error,
}

/// Менеджер трассировки системных вызовов
pub struct SyscallTracer {
    /// Трассировка включена
    pub enabled: bool,
    /// История вызовов
    pub trace_history: Vec<SyscallTraceEntry>,
    /// Максимальное количество записей в истории
    pub max_trace_entries: usize,
    /// Фильтр по процессам
    pub process_filter: Option<Vec<u64>>,
    /// Фильтр по системным вызовам
    pub syscall_filter: Option<Vec<u64>>,
    /// Статистика вызовов
    pub statistics: SyscallStatistics,
}

#[derive(Debug, Clone)]
pub struct SyscallStatistics {
    /// Количество вызовов каждого системного вызова
    pub call_counts: BTreeMap<u64, u64>,
    /// Среднее время выполнения для каждого системного вызова
    pub avg_execution_times: BTreeMap<u64, f64>,
    /// Общее время выполнения системных вызовов
    pub total_execution_time: u64,
    /// Количество ошибок
    pub error_count: u64,
}

impl SyscallTracer {
    /// Создает новый трассировщик системных вызовов
    pub fn new() -> Self {
        SyscallTracer {
            enabled: false,
            trace_history: Vec::new(),
            max_trace_entries: 10000, // 10k entries
            process_filter: None,
            syscall_filter: None,
            statistics: SyscallStatistics::new(),
        }
    }

    /// Включает трассировку
    pub fn enable_tracing(&mut self) {
        self.enabled = true;
    }

    /// Выключает трассировку
    pub fn disable_tracing(&mut self) {
        self.enabled = false;
    }

    /// Устанавливает фильтр по процессам
    pub fn set_process_filter(&mut self, pids: Vec<u64>) {
        self.process_filter = Some(pids);
    }

    /// Устанавливает фильтр по системным вызовам
    pub fn set_syscall_filter(&mut self, syscall_numbers: Vec<u64>) {
        self.syscall_filter = Some(syscall_numbers);
    }

    /// Очищает фильтры
    pub fn clear_filters(&mut self) {
        self.process_filter = None;
        self.syscall_filter = None;
    }

    /// Проверяет, нужно ли трассировать вызов
    pub fn should_trace(&self, syscall_number: u64, pid: u64) -> bool {
        if !self.enabled {
            return false;
        }

        // Проверяем фильтр по процессам
        if let Some(ref pids) = self.process_filter {
            if !pids.contains(&pid) {
                return false;
            }
        }

        // Проверяем фильтр по системным вызовам
        if let Some(ref syscalls) = self.syscall_filter {
            if !syscalls.contains(&syscall_number) {
                return false;
            }
        }

        true
    }

    /// Начинает трассировку системного вызова
    pub fn start_trace(&mut self, syscall_number: u64, args: [u64; 6], pid: u64) -> Option<u64> {
        if !self.should_trace(syscall_number, pid) {
            return None;
        }

        Some(get_uptime_ms())
    }

    /// Завершает трассировку системного вызова
    pub fn end_trace(
        &mut self,
        syscall_number: u64,
        args: [u64; 6],
        pid: u64,
        result: i64,
        start_time: u64,
    ) {
        if !self.enabled {
            return;
        }

        let end_time = get_uptime_ms();
        let execution_time = end_time - start_time;

        let result_type = if result < 0 {
            SyscallResultType::Error
        } else {
            SyscallResultType::Success
        };

        let entry = SyscallTraceEntry {
            syscall_number,
            syscall_name: self.get_syscall_name(syscall_number).to_string(),
            args,
            start_time,
            end_time,
            result,
            pid,
            result_type,
        };

        // Добавляем в историю
        self.trace_history.push(entry);

        // Ограничиваем размер истории
        if self.trace_history.len() > self.max_trace_entries {
            self.trace_history.remove(0);
        }

        // Обновляем статистику
        self.statistics
            .update(syscall_number, execution_time, result_type);
    }

    /// Получает имя системного вызова по номеру
    fn get_syscall_name(&self, syscall_number: u64) -> &'static str {
        match syscall_number {
            0 => "read",
            1 => "write",
            2 => "open",
            3 => "close",
            57 => "fork",
            59 => "execve",
            60 => "exit",
            61 => "wait4",
            9 => "mmap",
            11 => "munmap",
            39 => "getpid",
            24 => "sched_yield",
            62 => "kill",
            234 => "tgkill",
            35 => "nanosleep",
            228 => "clock_gettime",
            41 => "socket",
            42 => "connect",
            43 => "accept",
            44 => "sendto",
            45 => "recvfrom",
            12 => "brk",
            63 => "uname",
            102 => "getuid",
            107 => "geteuid",
            80 => "chdir",
            79 => "getcwd",
            72 => "fcntl",
            16 => "ioctl",
            10 => "mprotect",
            13 => "rt_sigaction",
            15 => "rt_sigreturn",
            56 => "clone",
            202 => "futex",
            218 => "set_tid_address",
            _ => "unknown",
        }
    }

    /// Получает историю трассировки
    pub fn get_trace_history(&self) -> &[SyscallTraceEntry] {
        &self.trace_history
    }

    /// Получает статистику трассировки
    pub fn get_statistics(&self) -> &SyscallStatistics {
        &self.statistics
    }

    /// Фильтрует трассировку по процессу
    pub fn get_trace_by_process(&self, pid: u64) -> Vec<&SyscallTraceEntry> {
        self.trace_history
            .iter()
            .filter(|entry| entry.pid == pid)
            .collect()
    }

    /// Фильтрует трассировку по системному вызову
    pub fn get_trace_by_syscall(&self, syscall_number: u64) -> Vec<&SyscallTraceEntry> {
        self.trace_history
            .iter()
            .filter(|entry| entry.syscall_number == syscall_number)
            .collect()
    }

    /// Сбрасывает историю трассировки
    pub fn clear_trace_history(&mut self) {
        self.trace_history.clear();
    }

    /// Сбрасывает статистику
    pub fn reset_statistics(&mut self) {
        self.statistics = SyscallStatistics::new();
    }

    /// Получает трассировку за определенный период времени
    pub fn get_trace_in_time_range(&self, start: u64, end: u64) -> Vec<&SyscallTraceEntry> {
        self.trace_history
            .iter()
            .filter(|entry| entry.start_time >= start && entry.end_time <= end)
            .collect()
    }

    /// Получает вызовы с ошибками
    pub fn get_error_calls(&self) -> Vec<&SyscallTraceEntry> {
        self.trace_history
            .iter()
            .filter(|entry| matches!(entry.result_type, SyscallResultType::Error))
            .collect()
    }

    /// Получает вызовы с медленным выполнением (время > threshold_ms)
    pub fn get_slow_calls(&self, threshold_ms: u64) -> Vec<&SyscallTraceEntry> {
        self.trace_history
            .iter()
            .filter(|entry| (entry.end_time - entry.start_time) > threshold_ms)
            .collect()
    }

    /// Получает топ N самых медленных вызовов
    pub fn get_slowest_calls(&self, n: usize) -> Vec<&SyscallTraceEntry> {
        let mut calls: Vec<&SyscallTraceEntry> = self.trace_history.iter().collect();
        calls.sort_by(|a, b| (b.end_time - b.start_time).cmp(&(a.end_time - a.start_time)));
        calls.truncate(n);
        calls
    }
}

impl SyscallStatistics {
    /// Создает новую статистику
    pub fn new() -> Self {
        SyscallStatistics {
            call_counts: BTreeMap::new(),
            avg_execution_times: BTreeMap::new(),
            total_execution_time: 0,
            error_count: 0,
        }
    }

    /// Обновляет статистику
    pub fn update(
        &mut self,
        syscall_number: u64,
        execution_time: u64,
        result_type: SyscallResultType,
    ) {
        // Увеличиваем счетчик вызовов
        *self.call_counts.entry(syscall_number).or_insert(0) += 1;

        // Обновляем среднее время выполнения
        let current_avg = self
            .avg_execution_times
            .entry(syscall_number)
            .or_insert(0.0);
        let count = *self.call_counts.get(&syscall_number).unwrap();
        *current_avg = (*current_avg * (count - 1) as f64 + execution_time as f64) / count as f64;

        // Обновляем общее время
        self.total_execution_time += execution_time;

        // Если это ошибка, увеличиваем счетчик ошибок
        if let SyscallResultType::Error = result_type {
            self.error_count += 1;
        }
    }

    /// Получает количество вызовов системного вызова
    pub fn get_call_count(&self, syscall_number: u64) -> u64 {
        *self.call_counts.get(&syscall_number).unwrap_or(&0)
    }

    /// Получает среднее время выполнения системного вызова
    pub fn get_average_time(&self, syscall_number: u64) -> f64 {
        *self
            .avg_execution_times
            .get(&syscall_number)
            .unwrap_or(&0.0)
    }

    /// Получает общее время выполнения всех вызовов
    pub fn get_total_time(&self) -> u64 {
        self.total_execution_time
    }

    /// Получает количество ошибок
    pub fn get_error_count(&self) -> u64 {
        self.error_count
    }

    /// Получает список самых часто вызываемых системных вызовов
    pub fn get_most_frequent_calls(&self, n: usize) -> Vec<(u64, u64)> {
        let mut calls: Vec<_> = self
            .call_counts
            .iter()
            .map(|(&num, &count)| (num, count))
            .collect();
        calls.sort_by(|a, b| b.1.cmp(&a.1));
        calls.truncate(n);
        calls
    }

    /// Получает список самых медленных системных вызовов
    pub fn get_slowest_calls(&self, n: usize) -> Vec<(u64, f64)> {
        let mut calls: Vec<_> = self
            .avg_execution_times
            .iter()
            .map(|(&num, &time)| (num, time))
            .collect();
        calls.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(core::cmp::Ordering::Equal));
        calls.truncate(n);
        calls
    }
}

/// Глобальный трассировщик системных вызовов
use spin::Once;

static SYSCALL_TRACER: Once<Mutex<SyscallTracer>> = Once::new();

pub fn get_syscall_tracer() -> &'static Mutex<SyscallTracer> {
    SYSCALL_TRACER.call_once(|| Mutex::new(SyscallTracer::new()))
}

/// Интерфейс для трассировки
pub struct SyscallTraceInterface;

impl SyscallTraceInterface {
    /// Включает трассировку
    pub fn enable_tracing() {
        get_syscall_tracer().lock().enable_tracing();
    }

    /// Выключает трассировку
    pub fn disable_tracing() {
        get_syscall_tracer().lock().disable_tracing();
    }

    /// Устанавливает фильтр по процессам
    pub fn set_process_filter(pids: Vec<u64>) {
        get_syscall_tracer().lock().set_process_filter(pids);
    }

    /// Устанавливает фильтр по системным вызовам
    pub fn set_syscall_filter(syscall_numbers: Vec<u64>) {
        get_syscall_tracer()
            .lock()
            .set_syscall_filter(syscall_numbers);
    }

    /// Очищает фильтры
    pub fn clear_filters() {
        get_syscall_tracer().lock().clear_filters();
    }

    /// Начинает трассировку системного вызова
    pub fn start_trace(syscall_number: u64, args: [u64; 6], pid: u64) -> Option<u64> {
        get_syscall_tracer()
            .lock()
            .start_trace(syscall_number, args, pid)
    }

    /// Завершает трассировку системного вызова
    pub fn end_trace(syscall_number: u64, args: [u64; 6], pid: u64, result: i64, start_time: u64) {
        get_syscall_tracer()
            .lock()
            .end_trace(syscall_number, args, pid, result, start_time);
    }

    /// Получает историю трассировки
    pub fn get_trace_history() -> Vec<SyscallTraceEntry> {
        get_syscall_tracer().lock().get_trace_history().to_vec()
    }

    /// Получает статистику трассировки
    pub fn get_trace_statistics() -> SyscallStatistics {
        get_syscall_tracer().lock().get_statistics().clone()
    }

    /// Получает трассировку по процессу
    pub fn get_trace_by_process(pid: u64) -> Vec<SyscallTraceEntry> {
        get_syscall_tracer()
            .lock()
            .get_trace_by_process(pid)
            .iter()
            .map(|x| (*x).clone())
            .collect()
    }

    /// Получает трассировку по системному вызову
    pub fn get_trace_by_syscall(syscall_number: u64) -> Vec<SyscallTraceEntry> {
        get_syscall_tracer()
            .lock()
            .get_trace_by_syscall(syscall_number)
            .iter()
            .map(|x| (*x).clone())
            .collect()
    }

    /// Сбрасывает историю трассировки
    pub fn clear_trace_history() {
        get_syscall_tracer().lock().clear_trace_history();
    }

    /// Сбрасывает статистику
    pub fn reset_statistics() {
        get_syscall_tracer().lock().reset_statistics();
    }

    /// Получает вызовы с ошибками
    pub fn get_error_calls() -> Vec<SyscallTraceEntry> {
        get_syscall_tracer()
            .lock()
            .get_error_calls()
            .iter()
            .map(|x| (*x).clone())
            .collect()
    }

    /// Получает медленные вызовы
    pub fn get_slow_calls(threshold_ms: u64) -> Vec<SyscallTraceEntry> {
        get_syscall_tracer()
            .lock()
            .get_slow_calls(threshold_ms)
            .iter()
            .map(|x| (*x).clone())
            .collect()
    }

    /// Получает топ N самых медленных вызовов
    pub fn get_slowest_calls(n: usize) -> Vec<SyscallTraceEntry> {
        get_syscall_tracer()
            .lock()
            .get_slowest_calls(n)
            .iter()
            .map(|x| (*x).clone())
            .collect()
    }

    /// Получает вызовы в диапазоне времени
    pub fn get_trace_in_time_range(start: u64, end: u64) -> Vec<SyscallTraceEntry> {
        get_syscall_tracer()
            .lock()
            .get_trace_in_time_range(start, end)
            .iter()
            .map(|x| (*x).clone())
            .collect()
    }
}

/// Макросы для удобства трассировки
#[macro_export]
macro_rules! trace_syscall_start {
    ($syscall_num:expr, [$($arg:expr),*], $pid:expr) => {
        if $crate::system_call_tracing::SyscallTraceInterface::is_tracing_enabled() {
            let args = [$($arg),*];
            $crate::system_call_tracing::SyscallTraceInterface::start_trace($syscall_num, args, $pid)
        } else {
            None
        }
    };
}

#[macro_export]
macro_rules! trace_syscall_end {
    ($syscall_num:expr, [$($arg:expr),*], $pid:expr, $result:expr, $start_time:expr) => {
        if let Some(start_time) = $start_time {
            let args = [$($arg),*];
            $crate::system_call_tracing::SyscallTraceInterface::end_trace($syscall_num, args, $pid, $result, start_time);
        }
    };
}

#[macro_export]
macro_rules! with_syscall_tracing {
    ($syscall:expr, [$($args:expr),*], $pid:expr, $body:block) => {{
        let start_time = if $crate::system_call_tracing::SyscallTraceInterface::is_tracing_enabled() {
            let args = [$($args),*];
            $crate::system_call_tracing::SyscallTraceInterface::start_trace($syscall, args, $pid)
        } else {
            None
        };

        let result = $body;

        if let Some(time) = start_time {
            let args = [$($args),*];
            $crate::system_call_tracing::SyscallTraceInterface::end_trace($syscall, args, $pid, result, time);
        }

        result
    }};
}

/// Вспомогательные функции для интеграции с системными вызовами
pub struct SyscallTracingIntegration;

impl SyscallTracingIntegration {
    /// Модифицирует вызов системного вызова для добавления трассировки
    pub fn traced_syscall_call(syscall_number: u64, args: [u64; 6], pid: u64) -> i64 {
        let tracer = get_syscall_tracer();

        // Начинаем трассировку если она включена
        if tracer.lock().should_trace(syscall_number, pid) {
            let start_time = get_uptime_ms();

            // Выполняем системный вызов
            let result =
                crate::system_call_interface::SyscallInvocation::invoke(syscall_number, args);

            // Завершаем трассировку
            let end_time = get_uptime_ms();
            tracer
                .lock()
                .end_trace(syscall_number, args, pid, result.as_i64(), start_time);

            result.as_i64()
        } else {
            // Простой вызов без трассировки
            let result =
                crate::system_call_interface::SyscallInvocation::invoke(syscall_number, args);
            result.as_i64()
        }
    }

    /// Получает ID текущего процесса (для трассировки)
    pub fn get_current_pid() -> u64 {
        // В реальной системе получаем PID из PCB
        // Пока возвращаем 0 для эмуляции
        0
    }

    /// Проверяет, включена ли трассировка
    pub fn is_tracing_enabled() -> bool {
        get_syscall_tracer().lock().enabled
    }
}

/// Интерфейс для анализа трассировки
pub struct TraceAnalyzer;

impl TraceAnalyzer {
    /// Анализирует историю трассировки и возвращает отчет
    pub fn analyze_trace_history(history: &[SyscallTraceEntry]) -> TraceAnalysisReport {
        let mut report = TraceAnalysisReport::new();

        for entry in history {
            report.total_calls += 1;

            match entry.result_type {
                SyscallResultType::Error => report.error_count += 1,
                SyscallResultType::Success => report.success_count += 1,
            }

            // Группируем вызовы по системному вызову
            *report
                .syscall_counts
                .entry(entry.syscall_number)
                .or_insert(0) += 1;

            // Группируем по процессу
            *report.process_counts.entry(entry.pid).or_insert(0) += 1;

            // Анализируем время выполнения
            let exec_time = entry.end_time - entry.start_time;
            report.total_execution_time += exec_time;

            // Обновляем статистику времени выполнения
            let stats = report
                .execution_times
                .entry(entry.syscall_number)
                .or_insert_with(|| ExecutionTimeStats::new());
            stats.update(exec_time);
        }

        // Вычисляем среднее время
        if !history.is_empty() {
            report.average_execution_time =
                report.total_execution_time as f64 / history.len() as f64;
        }

        report
    }

    /// Выводит отчет о трассировке
    pub fn print_trace_report(history: &[SyscallTraceEntry]) {
        let report = Self::analyze_trace_history(history);

        crate::println!("=== System Call Trace Analysis Report ===");
        crate::println!("Total Calls: {}", report.total_calls);
        crate::println!("Successful: {}", report.success_count);
        crate::println!("Errors: {}", report.error_count);
        crate::println!(
            "Average Execution Time: {:.2} ms",
            report.average_execution_time
        );
        crate::println!("Total Execution Time: {} ms", report.total_execution_time);

        crate::println!("\nTop 5 Most Frequent System Calls:");
        let mut top_calls: Vec<_> = report.syscall_counts.iter().collect();
        top_calls.sort_by(|a, b| b.1.cmp(a.1));
        for (syscall_num, &count) in top_calls.iter().take(5) {
            crate::println!(
                "  {}: {} calls ({})",
                crate::system_call_interface::get_syscall_name(*syscall_num),
                count,
                syscall_num
            );
        }

        crate::println!("\nTop 5 Slowest System Calls:");
        let mut slow_calls: Vec<_> = report.execution_times.iter().collect();
        slow_calls.sort_by(|a, b| {
            b.1.max_time
                .partial_cmp(&a.1.max_time)
                .unwrap_or(core::cmp::Ordering::Equal)
        });
        for (syscall_num, stats) in slow_calls.iter().take(5) {
            crate::println!(
                "  {}: Avg {:.2}ms, Max {:.2}ms, Min {:.2}ms",
                crate::system_call_interface::get_syscall_name(**syscall_num),
                stats.avg_time,
                stats.max_time,
                stats.min_time
            );
        }
    }
}

#[derive(Debug, Clone)]
pub struct TraceAnalysisReport {
    pub total_calls: usize,
    pub success_count: usize,
    pub error_count: usize,
    pub total_execution_time: u64,
    pub average_execution_time: f64,
    pub syscall_counts: BTreeMap<u64, usize>,
    pub process_counts: BTreeMap<u64, usize>,
    pub execution_times: BTreeMap<u64, ExecutionTimeStats>,
}

impl TraceAnalysisReport {
    fn new() -> Self {
        TraceAnalysisReport {
            total_calls: 0,
            success_count: 0,
            error_count: 0,
            total_execution_time: 0,
            average_execution_time: 0.0,
            syscall_counts: BTreeMap::new(),
            process_counts: BTreeMap::new(),
            execution_times: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionTimeStats {
    pub avg_time: f64,
    pub min_time: f64,
    pub max_time: f64,
    pub count: usize,
}

impl ExecutionTimeStats {
    fn new() -> Self {
        ExecutionTimeStats {
            avg_time: 0.0,
            min_time: f64::MAX,
            max_time: 0.0,
            count: 0,
        }
    }

    fn update(&mut self, time: u64) {
        let time_f64 = time as f64;

        // Обновляем минимальное время
        if time_f64 < self.min_time {
            self.min_time = time_f64;
        }

        // Обновляем максимальное время
        if time_f64 > self.max_time {
            self.max_time = time_f64;
        }

        // Обновляем среднее время
        self.avg_time = (self.avg_time * self.count as f64 + time_f64) / (self.count + 1) as f64;

        self.count += 1;
    }
}

// Интеграция с системными вызовами
pub fn init_syscall_tracing() {
    let _ = get_syscall_tracer(); // Инициализируем трассировщик
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_syscall_tracer_creation() {
        let tracer = SyscallTracer::new();
        assert_eq!(tracer.enabled, false);
        assert_eq!(tracer.trace_history.len(), 0);
        assert_eq!(tracer.max_trace_entries, 10000);
    }

    #[test]
    fn test_tracer_enable_disable() {
        let mut tracer = SyscallTracer::new();
        assert_eq!(tracer.enabled, false);

        tracer.enable_tracing();
        assert_eq!(tracer.enabled, true);

        tracer.disable_tracing();
        assert_eq!(tracer.enabled, false);
    }

    #[test]
    fn test_should_trace() {
        let mut tracer = SyscallTracer::new();
        tracer.enable_tracing();

        // Без фильтров - все вызовы должны трассироваться
        assert_eq!(tracer.should_trace(1, 1), true);

        // С фильтром по процессам
        tracer.set_process_filter(vec![1, 2, 3]);
        assert_eq!(tracer.should_trace(1, 1), true);
        assert_eq!(tracer.should_trace(1, 4), false);

        // С фильтром по системным вызовам
        tracer.clear_filters();
        tracer.set_syscall_filter(vec![1, 2]);
        assert_eq!(tracer.should_trace(1, 1), true);
        assert_eq!(tracer.should_trace(3, 1), false);
    }

    #[test]
    fn test_trace_entry() {
        let mut tracer = SyscallTracer::new();
        tracer.enable_tracing();

        let args = [1, 2, 3, 4, 5, 6];
        let start_time = tracer.start_trace(1, args, 123);
        assert!(start_time.is_some());

        tracer.end_trace(1, args, 123, 0, start_time.unwrap());
        assert_eq!(tracer.trace_history.len(), 1);

        let entry = &tracer.trace_history[0];
        assert_eq!(entry.syscall_number, 1);
        assert_eq!(entry.args, args);
        assert_eq!(entry.pid, 123);
        assert_eq!(entry.result_type, SyscallResultType::Success);
    }

    #[test]
    fn test_statistics() {
        let mut tracer = SyscallTracer::new();
        tracer.enable_tracing();

        let args = [0; 6];

        // Выполняем несколько вызовов
        let start = get_uptime_ms();
        tracer.end_trace(1, args, 1, 0, start);
        tracer.end_trace(1, args, 1, 0, start);
        tracer.end_trace(1, args, 1, -1, start); // Ошибка

        let stats = tracer.get_statistics();
        assert_eq!(stats.get_call_count(1), 3);
        assert_eq!(stats.get_error_count(), 1);
        assert_eq!(stats.get_average_time(1), 0.0); // Все вызовы заняли 0 времени
    }

    #[test]
    fn test_analysis() {
        let mut tracer = SyscallTracer::new();
        tracer.enable_tracing();

        let args = [0; 6];

        // Добавляем несколько записей трассировки
        let start = get_uptime_ms();
        tracer.end_trace(1, args, 1, 0, start);
        tracer.end_trace(2, args, 2, 0, start);
        tracer.end_trace(1, args, 1, -1, start);

        let analysis = TraceAnalyzer::analyze_trace_history(&tracer.trace_history);
        assert_eq!(analysis.total_calls, 3);
        assert_eq!(analysis.success_count, 2);
        assert_eq!(analysis.error_count, 1);
        assert_eq!(analysis.syscall_counts.get(&1), Some(&2));
        assert_eq!(analysis.syscall_counts.get(&2), Some(&1));
        assert_eq!(analysis.process_counts.get(&1), Some(&2));
        assert_eq!(analysis.process_counts.get(&2), Some(&1));
    }
}
