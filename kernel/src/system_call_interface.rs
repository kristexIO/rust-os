use alloc::{string::String, vec::Vec};
use x86_64::registers::model_specific::Msr;
use x86_64::VirtAddr;

/// Коды системных вызовов
pub const SYS_READ: u64 = 0;
pub const SYS_WRITE: u64 = 1;
pub const SYS_OPEN: u64 = 2;
pub const SYS_CLOSE: u64 = 3;
pub const SYS_FORK: u64 = 57;
pub const SYS_EXECVE: u64 = 59;
pub const SYS_EXIT: u64 = 60;
pub const SYS_WAIT4: u64 = 61;
pub const SYS_MMAP: u64 = 9;
pub const SYS_MUNMAP: u64 = 11;
pub const SYS_GETPID: u64 = 39;
pub const SYS_SCHED_YIELD: u64 = 24;
pub const SYS_KILL: u64 = 62;
pub const SYS_TGKILL: u64 = 234;
pub const SYS_NANOSLEEP: u64 = 35;
pub const SYS_CLOCK_GETTIME: u64 = 228;
pub const SYS_SOCKET: u64 = 41;
pub const SYS_CONNECT: u64 = 42;
pub const SYS_ACCEPT: u64 = 43;
pub const SYS_SENDTO: u64 = 44;
pub const SYS_RECVFROM: u64 = 45;
pub const SYS_BRK: u64 = 12;
pub const SYS_UNAME: u64 = 63;
pub const SYS_GETUID: u64 = 102;
pub const SYS_GETEUID: u64 = 107;
pub const SYS_CHDIR: u64 = 80;
pub const SYS_GETCWD: u64 = 79;
pub const SYS_FCNTL: u64 = 72;
pub const SYS_IOCTL: u64 = 16;
pub const SYS_MPROTECT: u64 = 10;
pub const SYS_RT_SIGACTION: u64 = 13;
pub const SYS_RT_SIGRETURN: u64 = 15;
pub const SYS_CLONE: u64 = 56;
pub const SYS_FUTEX: u64 = 202;
pub const SYS_SET_TID_ADDRESS: u64 = 218;
pub const SYS_UNAME: u64 = 63;

/// Результат системного вызова
#[derive(Debug, Clone, Copy)]
pub enum SyscallResult {
    Success(i64),
    Error(i64), // Отрицательное значение обозначает ошибку
}

impl SyscallResult {
    pub fn is_success(&self) -> bool {
        match self {
            SyscallResult::Success(_) => true,
            SyscallResult::Error(_) => false,
        }
    }

    pub fn is_error(&self) -> bool {
        !self.is_success()
    }

    pub fn as_i64(&self) -> i64 {
        match self {
            SyscallResult::Success(val) => *val,
            SyscallResult::Error(val) => *val,
        }
    }
}

/// Структура системного вызова
#[derive(Debug, Clone)]
pub struct SyscallInfo {
    pub number: u64,
    pub name: String,
    pub args: [u64; 6],
    pub return_value: Option<i64>,
    pub execution_time: u64,
    pub caller_pid: u64,
    pub timestamp: u64,
    pub result_status: SyscallResult,
}

pub struct SystemCallInterface {
    /// Количество системных вызовов
    pub call_counts: [u64; 256],
    /// История системных вызовов
    pub call_history: Vec<SyscallInfo>,
    /// Максимальный размер истории
    pub max_history_size: usize,
    /// Обработчики системных вызовов
    pub handlers: [Option<unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64>; 256],
    /// Флаг включения профилирования системных вызовов
    pub profiling_enabled: bool,
    /// Флаг включения аудита системных вызовов
    pub audit_enabled: bool,
}

impl SystemCallInterface {
    /// Создает новый интерфейс системных вызовов
    pub fn new() -> Self {
        SystemCallInterface {
            call_counts: [0; 256],
            call_history: Vec::new(),
            max_history_size: 1000,
            handlers: [None; 256],
            profiling_enabled: true,
            audit_enabled: true,
        }
    }

    /// Инициализирует системные вызовы
    pub fn init(&mut self) {
        // Регистрируем обработчики системных вызовов
        self.register_handler(
            SYS_READ,
            system_call_read as unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64,
        );
        self.register_handler(
            SYS_WRITE,
            system_call_write as unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64,
        );
        self.register_handler(
            SYS_OPEN,
            system_call_open as unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64,
        );
        self.register_handler(
            SYS_CLOSE,
            system_call_close as unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64,
        );
        self.register_handler(
            SYS_EXIT,
            system_call_exit as unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64,
        );
        self.register_handler(
            SYS_GETPID,
            system_call_getpid as unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64,
        );
        self.register_handler(
            SYS_SCHED_YIELD,
            system_call_sched_yield as unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64,
        );

        // Дополнительные системные вызовы
        self.register_handler(
            SYS_MMAP,
            system_call_mmap as unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64,
        );
        self.register_handler(
            SYS_MUNMAP,
            system_call_munmap as unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64,
        );
        self.register_handler(
            SYS_BRK,
            system_call_brk as unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64,
        );
        self.register_handler(
            SYS_GETUID,
            system_call_getuid as unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64,
        );
        self.register_handler(
            SYS_GETEUID,
            system_call_geteuid as unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64,
        );
        self.register_handler(
            SYS_CHDIR,
            system_call_chdir as unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64,
        );
        self.register_handler(
            SYS_GETCWD,
            system_call_getcwd as unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64,
        );
        self.register_handler(
            SYS_UNAME,
            system_call_uname as unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64,
        );
    }

    /// Регистрирует обработчик системного вызова
    pub fn register_handler(
        &mut self,
        syscall_number: u64,
        handler: unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64,
    ) {
        if syscall_number < 256 {
            self.handlers[syscall_number as usize] = Some(handler);
        }
    }

    /// Вызывает системный вызов
    pub fn call(&mut self, syscall_number: u64, args: [u64; 6]) -> SyscallResult {
        let start_time = crate::timer::get_uptime_ms();

        // Проверяем, что номер системного вызова в пределах диапазона
        if syscall_number >= 256 {
            return SyscallResult::Error(-1); // Invalid system call number
        }

        // Увеличиваем счетчик вызовов
        self.call_counts[syscall_number as usize] += 1;

        let result = if let Some(handler) = self.handlers[syscall_number as usize] {
            // Проверяем права доступа к системному вызову
            if self.audit_enabled && !self.check_permission(syscall_number) {
                return SyscallResult::Error(-13); // EACCES - Permission denied
            }

            // Вызываем обработчик
            let return_value =
                unsafe { handler(args[0], args[1], args[2], args[3], args[4], args[5]) };

            SyscallResult::Success(return_value as i64)
        } else {
            SyscallResult::Error(-38) // ENOSYS - Function not implemented
        };

        // Записываем информацию о вызове для профилирования
        if self.profiling_enabled {
            let call_info = SyscallInfo {
                number: syscall_number,
                name: self.get_syscall_name(syscall_number).to_string(),
                args,
                return_value: match result {
                    SyscallResult::Success(val) => Some(val),
                    SyscallResult::Error(_) => None,
                },
                execution_time: crate::timer::get_uptime_ms() - start_time,
                caller_pid: self.get_current_process_id(),
                timestamp: start_time,
                result_status: result,
            };

            // Добавляем в историю
            self.call_history.push(call_info);

            // Ограничиваем размер истории
            if self.call_history.len() > self.max_history_size {
                self.call_history.remove(0);
            }
        }

        result
    }

    /// Проверяет разрешение на вызов системного вызова
    fn check_permission(&self, syscall_number: u64) -> bool {
        // В реальной системе тут будет проверка привилегий процесса
        // Пока возвращаем true для всех вызовов
        true
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

    /// Получает ID текущего процесса
    fn get_current_process_id(&self) -> u64 {
        // В реальной системе: получить ID из PCB/TCB
        // Пока возвращаем 0 для эмуляции
        0
    }

    /// Получает количество вызовов системного вызова
    pub fn get_call_count(&self, syscall_number: u64) -> u64 {
        if syscall_number < 256 {
            self.call_counts[syscall_number as usize]
        } else {
            0
        }
    }

    /// Получает историю системных вызовов
    pub fn get_call_history(&self) -> &[SyscallInfo] {
        &self.call_history
    }

    /// Получает последние N вызовов системных вызовов
    pub fn get_recent_calls(&self, count: usize) -> Vec<SyscallInfo> {
        let start = self.call_history.len().saturating_sub(count);
        self.call_history[start..].to_vec()
    }

    /// Получает самые часто вызываемые системные вызовы
    pub fn get_most_frequent_calls(&self, count: usize) -> Vec<(u64, &'static str, u64)> {
        let mut calls: Vec<_> = self
            .call_counts
            .iter()
            .enumerate()
            .filter(|(_, &count)| count > 0)
            .map(|(num, &count)| (num as u64, self.get_syscall_name(num as u64), count))
            .collect();

        calls.sort_by(|a, b| b.2.cmp(&a.2)); // Сортируем по убыванию
        calls.truncate(count);
        calls
    }

    /// Сбрасывает статистику
    pub fn reset_stats(&mut self) {
        self.call_counts = [0; 256];
        self.call_history.clear();
    }

    /// Включает профилирование
    pub fn enable_profiling(&mut self) {
        self.profiling_enabled = true;
    }

    /// Выключает профилирование
    pub fn disable_profiling(&mut self) {
        self.profiling_enabled = false;
    }

    /// Включает аудит
    pub fn enable_audit(&mut self) {
        self.audit_enabled = true;
    }

    /// Выключает аудит
    pub fn disable_audit(&mut self) {
        self.audit_enabled = false;
    }

    /// Получает статистику по системным вызовам
    pub fn get_statistics(&self) -> SyscallStats {
        SyscallStats {
            total_calls: self.call_counts.iter().sum(),
            top_syscalls: self.get_most_frequent_calls(10),
        }
    }
}

/// Статистика системных вызовов
#[derive(Debug)]
pub struct SyscallStats {
    pub total_calls: u64,
    pub top_syscalls: Vec<(u64, &'static str, u64)>,
}

// Обработчики системных вызовов
unsafe extern "C" fn system_call_read(
    fd: u64,
    buf: u64,
    count: u64,
    _arg3: u64,
    _arg4: u64,
    _arg5: u64,
) -> u64 {
    // Реализация системного вызова read()
    // В реальной системе: проверка прав, чтение из файла/сокета в буфер
    // Для эмуляции возвращаем 0 (число прочитанных байт)
    0
}

unsafe extern "C" fn system_call_write(
    fd: u64,
    buf: u64,
    count: u64,
    _arg3: u64,
    _arg4: u64,
    _arg5: u64,
) -> u64 {
    // Реализация системного вызова write()
    // В реальной системе: проверка прав, запись из буфера в файл/сокет
    // Для эмуляции возвращаем число записанных байт

    // Можем обработать специальные fd:
    if fd == 1 || fd == 2 {
        // stdout/stderr
        // Пытаемся вывести содержимое буфера
        if let Ok(str_slice) = core::str::from_utf8(
            core::slice::from_raw_parts(buf as *const u8, count.min(1024) as usize), // Максимум 1024 байта для безопасности
        ) {
            crate::println!("{}", str_slice);
        }
    }

    count // Возвращаем количество записанных байт
}

unsafe extern "C" fn system_call_open(
    filename: u64,
    flags: u64,
    mode: u64,
    _arg3: u64,
    _arg4: u64,
    _arg5: u64,
) -> u64 {
    // Реализация системного вызова open()
    // В реальной системе: открытие файла, проверка прав доступа
    // Возвращаем дескриптор файла или -1 в случае ошибки
    -1 as u64 // Пока не реализовано
}

unsafe extern "C" fn system_call_close(
    fd: u64,
    _arg1: u64,
    _arg2: u64,
    _arg3: u64,
    _arg4: u64,
    _arg5: u64,
) -> u64 {
    // Реализация системного вызова close()
    // В реальной системе: закрытие дескриптора файла
    0 // Успех
}

unsafe extern "C" fn system_call_exit(
    status: u64,
    _arg1: u64,
    _arg2: u64,
    _arg3: u64,
    _arg4: u64,
    _arg5: u64,
) -> u64 {
    // Реализация системного вызова exit()
    // В реальной системе: завершение процесса, освобождение ресурсов
    // Пока просто возвращаем статус

    // Логгируем выход процесса
    crate::println!("Process exiting with status: {}", status as i32);

    0
}

unsafe extern "C" fn system_call_getpid(
    _arg1: u64,
    _arg2: u64,
    _arg3: u64,
    _arg4: u64,
    _arg5: u64,
    _arg6: u64,
) -> u64 {
    // Реализация системного вызова getpid()
    // В реальной системе: возвращаем ID текущего процесса
    // Пока возвращаем 0 для эмуляции
    0
}

unsafe extern "C" fn system_call_sched_yield(
    _arg1: u64,
    _arg2: u64,
    _arg3: u64,
    _arg4: u64,
    _arg5: u64,
    _arg6: u64,
) -> u64 {
    // Реализация системного вызова sched_yield()
    // В реальной системе: передаем управление другому процессу
    // Пока просто возвращаем 0
    0
}

unsafe extern "C" fn system_call_mmap(
    addr: u64,
    len: u64,
    prot: u64,
    flags: u64,
    fd: u64,
    offset: u64,
) -> u64 {
    // Реализация системного вызова mmap()
    // В реальной системе: отображение файла или устройства в виртуальную память

    // Проверяем аргументы
    if len == 0 || addr == 0 && (flags & 0x10 /* MAP_FIXED */ != 0) {
        // MAP_FIXED
        return (-1i64) as u64; // EINVAL
    }

    // В реальной системе: выделяем виртуальную память
    // Для эмуляции возвращаем фиктивный адрес

    // Это упрощенная реализация - в реальной системе нужно:
    // 1. Проверить права доступа
    // 2. Найти подходящий виртуальный адрес
    // 3. Выделить физическую память
    // 4. Установить отображение в таблице страниц
    // 5. Обновить учет памяти
    0x40000000 // Возвращаем фиктивный адрес
}

unsafe extern "C" fn system_call_munmap(
    addr: u64,
    len: u64,
    _arg2: u64,
    _arg3: u64,
    _arg4: u64,
    _arg5: u64,
) -> u64 {
    // Реализация системного вызова munmap()
    // В реальной системе: удаляем отображение памяти
    // Пока возвращаем 0 (успех)

    if addr == 0 || len == 0 {
        return (-1i64) as u64; // EINVAL
    }

    0
}

unsafe extern "C" fn system_call_brk(
    brk: u64,
    _arg1: u64,
    _arg2: u64,
    _arg3: u64,
    _arg4: u64,
    _arg5: u64,
) -> u64 {
    // Реализация системного вызова brk()
    // В реальной системе: устанавливаем конец сегмента данных

    // В нашей системе управление кучей реализовано по-другому
    // Пока возвращаем текущий "break" адрес
    0x50000000 // Фиктивное значение
}

unsafe extern "C" fn system_call_getuid(
    _arg1: u64,
    _arg2: u64,
    _arg3: u64,
    _arg4: u64,
    _arg5: u64,
    _arg6: u64,
) -> u64 {
    // Реализация системного вызова getuid()
    // Возвращаем UID пользователя (в нашей системе всегда 0 для root)
    0
}

unsafe extern "C" fn system_call_geteuid(
    _arg1: u64,
    _arg2: u64,
    _arg3: u64,
    _arg4: u64,
    _arg5: u64,
    _arg6: u64,
) -> u64 {
    // Реализация системного вызова geteuid()
    // Возвращаем эффективный UID (в нашей системе всегда 0 для root)
    0
}

unsafe extern "C" fn system_call_chdir(
    path: u64,
    _arg1: u64,
    _arg2: u64,
    _arg3: u64,
    _arg4: u64,
    _arg5: u64,
) -> u64 {
    // Реализация системного вызова chdir()
    // В реальной системе: изменение текущей директории
    -1i64 as u64 // Пока не реализовано
}

unsafe extern "C" fn system_call_getcwd(
    buf: u64,
    size: u64,
    _arg2: u64,
    _arg3: u64,
    _arg4: u64,
    _arg5: u64,
) -> u64 {
    // Реализация системного вызова getcwd()
    // В реальной системе: получение текущей директории

    if buf != 0 && size > 0 {
        // Копируем путь в буфер (в нашей системе всегда корень)
        let cwd_path = b"/\0"; // Корневая директория
        let actual_size = size.min(cwd_path.len() as u64) as usize;

        // Безопасная копия в память пользователя
        let user_buf = buf as *mut u8;
        for i in 0..actual_size {
            if i < cwd_path.len() {
                unsafe {
                    core::ptr::write(user_buf.add(i), cwd_path[i]);
                }
            }
        }

        return actual_size as u64;
    }

    -1i64 as u64 // Ошибка
}

unsafe extern "C" fn system_call_uname(
    buf: u64,
    _arg1: u64,
    _arg2: u64,
    _arg3: u64,
    _arg4: u64,
    _arg5: u64,
) -> u64 {
    // Реализация системного вызова uname()
    // Заполняет структуру utsname информацией о системе

    if buf != 0 {
        // В реальной системе: заполняем utsname структуру
        // Пока возвращаем 0 (успех)
        0
    } else {
        -1i64 as u64 // EINVAL
    }
}

// Глобальный интерфейс системных вызовов
use spin::Mutex;

static SYSTEM_CALL_INTERFACE: spin::Mutex<SystemCallInterface> =
    spin::Mutex::new(SystemCallInterface::new());

pub fn get_system_call_interface() -> &'static spin::Mutex<SystemCallInterface> {
    &SYSTEM_CALL_INTERFACE
}

/// Интерфейс для вызова системных вызовов
pub struct SyscallInvocation;

impl SyscallInvocation {
    /// Вызывает системный вызов
    pub fn invoke(syscall_number: u64, args: [u64; 6]) -> SyscallResult {
        get_system_call_interface()
            .lock()
            .call(syscall_number, args)
    }

    /// Регистрирует обработчик системного вызова
    pub fn register_handler(
        syscall_number: u64,
        handler: unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64,
    ) {
        get_system_call_interface()
            .lock()
            .register_handler(syscall_number, handler);
    }

    /// Получает информацию о системных вызовах
    pub fn get_syscall_info() -> SyscallStats {
        get_system_call_interface().lock().get_statistics()
    }

    /// Получает историю системных вызовов
    pub fn get_syscall_history() -> Vec<SyscallInfo> {
        get_system_call_interface()
            .lock()
            .get_call_history()
            .to_vec()
    }

    /// Получает количество вызовов конкретного системного вызова
    pub fn get_call_count(syscall_number: u64) -> u64 {
        get_system_call_interface()
            .lock()
            .get_call_count(syscall_number)
    }

    /// Включает профилирование системных вызовов
    pub fn enable_profiling() {
        get_system_call_interface().lock().enable_profiling();
    }

    /// Выключает профилирование системных вызовов
    pub fn disable_profiling() {
        get_system_call_interface().lock().disable_profiling();
    }

    /// Включает аудит системных вызовов
    pub fn enable_audit() {
        get_system_call_interface().lock().enable_audit();
    }

    /// Выключает аудит системных вызовов
    pub fn disable_audit() {
        get_system_call_interface().lock().disable_audit();
    }

    /// Сбрасывает статистику системных вызовов
    pub fn reset_stats() {
        get_system_call_interface().lock().reset_stats();
    }
}

/// Макросы для удобства вызова системных вызовов
#[macro_export]
macro_rules! syscall {
    ($num:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr, $arg5:expr, $arg6:expr) => {
        $crate::system_call_interface::SyscallInvocation::invoke(
            $num,
            [$arg1, $arg2, $arg3, $arg4, $arg5, $arg6],
        )
    };
    ($num:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr, $arg5:expr) => {
        $crate::system_call_interface::SyscallInvocation::invoke(
            $num,
            [$arg1, $arg2, $arg3, $arg4, $arg5, 0],
        )
    };
    ($num:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr) => {
        $crate::system_call_interface::SyscallInvocation::invoke(
            $num,
            [$arg1, $arg2, $arg3, $arg4, 0, 0],
        )
    };
    ($num:expr, $arg1:expr, $arg2:expr, $arg3:expr) => {
        $crate::system_call_interface::SyscallInvocation::invoke(
            $num,
            [$arg1, $arg2, $arg3, 0, 0, 0],
        )
    };
    ($num:expr, $arg1:expr, $arg2:expr) => {
        $crate::system_call_interface::SyscallInvocation::invoke($num, [$arg1, $arg2, 0, 0, 0, 0])
    };
    ($num:expr, $arg1:expr) => {
        $crate::system_call_interface::SyscallInvocation::invoke($num, [$arg1, 0, 0, 0, 0, 0])
    };
    ($num:expr) => {
        $crate::system_call_interface::SyscallInvocation::invoke($num, [0, 0, 0, 0, 0, 0])
    };
}

/// Специальные макросы для часто используемых системных вызовов
#[macro_export]
macro_rules! sys_write {
    ($fd:expr, $buf:expr, $count:expr) => {
        $crate::syscall!(
            $crate::system_call_interface::SYS_WRITE,
            $fd,
            $buf,
            $count,
            0,
            0,
            0
        )
    };
}

#[macro_export]
macro_rules! sys_read {
    ($fd:expr, $buf:expr, $count:expr) => {
        $crate::syscall!(
            $crate::system_call_interface::SYS_READ,
            $fd,
            $buf,
            $count,
            0,
            0,
            0
        )
    };
}

#[macro_export]
macro_rules! sys_exit {
    ($status:expr) => {
        $crate::syscall!(
            $crate::system_call_interface::SYS_EXIT,
            $status,
            0,
            0,
            0,
            0,
            0
        )
    };
}

#[macro_export]
macro_rules! sys_getpid {
    () => {
        $crate::syscall!($crate::system_call_interface::SYS_GETPID)
    };
}

#[macro_export]
macro_rules! sys_yield {
    () => {
        $crate::syscall!($crate::system_call_interface::SYS_SCHED_YIELD)
    };
}

#[macro_export]
macro_rules! sys_mmap {
    ($addr:expr, $len:expr, $prot:expr, $flags:expr, $fd:expr, $offset:expr) => {
        $crate::syscall!(
            $crate::system_call_interface::SYS_MMAP,
            $addr,
            $len,
            $prot,
            $flags,
            $fd,
            $offset
        )
    };
}

#[macro_export]
macro_rules! sys_munmap {
    ($addr:expr, $len:expr) => {
        $crate::syscall!(
            $crate::system_call_interface::SYS_MUNMAP,
            $addr,
            $len,
            0,
            0,
            0,
            0
        )
    };
}

#[macro_export]
macro_rules! sys_brk {
    ($addr:expr) => {
        $crate::syscall!($crate::system_call_interface::SYS_BRK, $addr, 0, 0, 0, 0, 0)
    };
}

#[macro_export]
macro_rules! sys_getuid {
    () => {
        $crate::syscall!($crate::system_call_interface::SYS_GETUID)
    };
}

#[macro_export]
macro_rules! sys_getcwd {
    ($buf:expr, $size:expr) => {
        $crate::syscall!(
            $crate::system_call_interface::SYS_GETCWD,
            $buf,
            $size,
            0,
            0,
            0,
            0
        )
    };
}

// Инициализация системных вызовов
pub fn init_system_calls() {
    let mut syscall_interface = get_system_call_interface().lock();
    syscall_interface.init();
}

/// Утилиты для безопасности системных вызовов
pub struct SyscallSecurityUtils;

impl SyscallSecurityUtils {
    /// Проверяет, что указатель находится в пользовательском пространстве
    pub fn is_user_space_ptr(ptr: u64) -> bool {
        // В x86_64 пользовательское пространство обычно до 0x00007FFFFFFFFFFF
        ptr < 0x0000800000000000
    }

    /// Проверяет длину буфера
    pub fn is_valid_buffer_length(length: u64) -> bool {
        // Проверяем, что длина буфера разумна (не слишком велика)
        length > 0 && length < (1024 * 1024 * 1024) // 1GB максимум
    }

    /// Проверяет, что строка валидна (заканчивается \0)
    pub fn is_valid_c_string(ptr: u64, max_len: usize) -> bool {
        if ptr == 0 {
            return false;
        }

        let mut len = 0;
        unsafe {
            let p = ptr as *const u8;
            while len < max_len {
                if *p.add(len) == 0 {
                    return true; // Нашли нулевой байт
                }
                len += 1;
            }
        }

        false // Не нашли нулевой байт в пределах max_len
    }

    /// Проверяет разрешение системного вызова для процесса
    pub fn check_syscall_permission(pid: u64, syscall_number: u64) -> bool {
        // В реальной системе проверка прав доступа для процесса
        // В нашей системе пока все разрешено
        true
    }

    /// Аудит вызова системного вызова
    pub fn audit_syscall_invocation(syscall_number: u64, args: [u64; 6], caller_pid: u64) {
        // Проверяем, включено ли аудирование
        if get_system_call_interface().lock().audit_enabled {
            // В реальной системе: логгируем вызов системного вызова
            // с информацией о процессе, аргументах и времени
            crate::logging::log_info(&format!(
                "SYSCALL_AUDIT: Process {} called syscall {} with args [{}, {}, {}, {}, {}, {}]",
                caller_pid, syscall_number, args[0], args[1], args[2], args[3], args[4], args[5]
            ));
        }
    }
}

/// Интерфейс безопасности для системных вызовов
pub struct SyscallSecurityInterface;

impl SyscallSecurityInterface {
    /// Выполняет защищенный вызов системного вызова
    pub fn secure_invoke(syscall_number: u64, args: [u64; 6]) -> SyscallResult {
        let current_pid = 0; // В реальной системе получаем ID текущего процесса

        // Проверяем права доступа
        if !SyscallSecurityUtils::check_syscall_permission(current_pid, syscall_number) {
            return SyscallResult::Error(-13); // EACCES
        }

        // Выполняем аудит
        if get_system_call_interface().lock().audit_enabled {
            SyscallSecurityUtils::audit_syscall_invocation(syscall_number, args, current_pid);
        }

        // Вызываем системный вызов
        get_system_call_interface()
            .lock()
            .call(syscall_number, args)
    }

    /// Регистрирует защищенный обработчик
    pub fn register_secure_handler(
        syscall_number: u64,
        handler: unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64,
        security_checker: fn(u64, [u64; 6]) -> bool,
    ) {
        // В реальной системе: оборачиваем обработчик в проверку безопасности
        let wrapped_handler = move |a: u64, b: u64, c: u64, d: u64, e: u64, f: u64| -> u64 {
            if security_checker(a, [a, b, c, d, e, f]) {
                handler(a, b, c, d, e, f)
            } else {
                (-1i64) as u64 // EPERM
            }
        };

        get_system_call_interface()
            .lock()
            .register_handler(syscall_number, wrapped_handler);
    }
}

// Список часто используемых системных вызовов для быстрого доступа
pub struct CommonSyscalls;

impl CommonSyscalls {
    /// Пишет в stdout
    pub fn write_stdout(buf: &str) -> SyscallResult {
        let ptr = buf.as_ptr() as u64;
        let len = buf.len() as u64;
        syscall!(SYS_WRITE, 1, ptr, len) // fd=1 is stdout
    }

    /// Пишет в stderr
    pub fn write_stderr(buf: &str) -> SyscallResult {
        let ptr = buf.as_ptr() as u64;
        let len = buf.len() as u64;
        syscall!(SYS_WRITE, 2, ptr, len) // fd=2 is stderr
    }

    /// Завершает процесс с кодом
    pub fn exit_process(code: i32) -> ! {
        let result = syscall!(SYS_EXIT, code as u64);
        // Даже если системный вызов не succeeded, вызываем планировщик
        crate::task::yield_cpu();
        loop {
            x86_64::instructions::hlt();
        }
    }

    /// Получает ID текущего процесса
    pub fn get_current_pid() -> u64 {
        match syscall!(SYS_GETPID) {
            SyscallResult::Success(pid) => pid as u64,
            _ => 0, // Если не удалось получить PID, возвращаем 0
        }
    }

    /// Выдает управление планировщику
    pub fn yield_cpu() {
        let _ = syscall!(SYS_SCHED_YIELD);
    }

    /// Получает текущего пользователя
    pub fn get_current_uid() -> u32 {
        match syscall!(SYS_GETUID) {
            SyscallResult::Success(uid) => uid as u32,
            _ => 0, // В нашей системе все работает от root
        }
    }
}
