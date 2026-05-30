/// Syscall-слой ядра: SYSCALL/SYSRET (AMD64 ABI).
///
/// Архитектура:
///   - Настройка MSR STAR/LSTAR/SFMASK при старте.
///   - `syscall_handler` — низкоуровневый asm-обработчик, сохраняет
///     callee-saved регистры, вызывает `rust_syscall_dispatch`.
///   - `rust_syscall_dispatch` — таблица системных вызовов на Rust.
///
/// Соглашение о вызовах (Linux-совместимое):
///   rax — номер syscall
///   rdi, rsi, rdx, r10, r8, r9 — аргументы (1-6)
///   rax — возвращаемое значение (или -errno)
///
/// Реализованные syscall-ы:
///   0  sys_write   — write(fd, buf, len)
///   1  sys_exit    — exit(code)
///   2  sys_getpid  — getpid()
///   3  sys_sleep   — sleep(ms)  [busy-wait]
///   4  sys_yield   — yield()
///   5  sys_open    — open(path, flags) → fd
///   6  sys_read    — read(fd, buf, len)
///   7  sys_close   — close(fd)
///   8  sys_mkdir   — mkdir(path)
///   9  sys_unlink  — unlink(path)
///  10  sys_gettime — gettime() → ticks
use core::arch::asm;

use x86_64::{
    registers::{
        model_specific::{Efer, EferFlags, KernelGsBase, LStar, SFMask, Star},
        rflags::RFlags,
    },
    VirtAddr,
};

use crate::gdt;

// ─── Коды ошибок (Linux-совместимые errno)
// ────────────────────────────────────

pub const ENOSYS: i64 = -38; // Function not implemented
pub const EINVAL: i64 = -22; // Invalid argument
pub const EBADF: i64 = -9; // Bad file descriptor
pub const EFAULT: i64 = -14; // Bad address
pub const ENOENT: i64 = -2; // No such file or directory
pub const EEXIST: i64 = -17; // File exists
pub const ENOMEM: i64 = -12; // Out of memory
pub const EPERM: i64 = -1; // Operation not permitted

// ─── Таблица файловых дескрипторов (упрощённая)
// ───────────────────────────────

/// Максимум открытых fd на всю систему (временно — один процесс)
const MAX_FDS: usize = 64;

/// Вид файлового дескриптора
#[derive(Clone)]
enum FdKind {
    Stdin,
    Stdout,
    Stderr,
    File(alloc::string::String), // путь в VFS
}

/// Таблица файловых дескрипторов ядра
struct FdTable {
    slots: [Option<FdKind>; MAX_FDS],
    next_fd: usize,
}

impl FdTable {
    const fn new() -> Self {
        FdTable {
            slots: [const { None }; MAX_FDS],
            next_fd: 3, // 0/1/2 зарезервированы
        }
    }

    fn alloc(&mut self, kind: FdKind) -> Option<usize> {
        // Ищем свободный слот начиная с next_fd
        for fd in self.next_fd..MAX_FDS {
            if self.slots[fd].is_none() {
                self.slots[fd] = Some(kind);
                self.next_fd = fd + 1;
                return Some(fd);
            }
        }
        // Пробуем с начала (после 2)
        for fd in 3..self.next_fd {
            if self.slots[fd].is_none() {
                self.slots[fd] = Some(kind);
                return Some(fd);
            }
        }
        None
    }

    fn free(&mut self, fd: usize) -> bool {
        if fd < MAX_FDS && self.slots[fd].is_some() {
            self.slots[fd] = None;
            if fd < self.next_fd {
                self.next_fd = fd;
            }
            true
        } else {
            false
        }
    }

    fn get(&self, fd: usize) -> Option<&FdKind> {
        if fd < MAX_FDS {
            self.slots[fd].as_ref()
        } else {
            None
        }
    }
}

static FD_TABLE: spin::Mutex<FdTable> = spin::Mutex::new(FdTable::new());

#[repr(C)]
struct SyscallCpuLocal {
    user_rsp: u64,
    kernel_rsp: u64,
}

static mut SYSCALL_CPU_LOCAL: SyscallCpuLocal = SyscallCpuLocal {
    user_rsp: 0,
    kernel_rsp: 0,
};

// ─── Инициализация MSR
// ────────────────────────────────────────────────────────

/// Инициализирует MSR для SYSCALL/SYSRET.
///
/// STAR  — сегментные селекторы для kernel/user режима.
/// LSTAR — линейный адрес точки входа `syscall_handler` (64-bit mode).
/// SFMASK — биты RFLAGS, которые обнуляются при входе в SYSCALL.
///
/// # Safety
/// Запись в MSR — привилегированная операция (Ring 0).
pub fn init() {
    unsafe {
        core::ptr::write(
            core::ptr::addr_of_mut!(SYSCALL_CPU_LOCAL),
            SyscallCpuLocal {
                user_rsp: 0,
                kernel_rsp: gdt::syscall_stack_top().as_u64(),
            },
        );
        KernelGsBase::write(VirtAddr::from_ptr(core::ptr::addr_of!(SYSCALL_CPU_LOCAL)));

        // Включаем SYSCALL/SYSRET в EFER
        let mut efer = Efer::read();
        efer |= EferFlags::SYSTEM_CALL_EXTENSIONS;
        Efer::write(efer);

        // STAR: [63:48] = user CS - 8 (для SYSRET), [47:32] = kernel CS
        // GDT layout: 0=null, 1=kernel_code, 2=kernel_data, 3=user_data, 4=user_code
        // Для STAR: kernel_cs = selector 0x08 (index 1), user_cs = 0x20 (index 4)
        // SYSRET ставит CS = STAR[63:48] + 16 (user_code) и SS = STAR[63:48] + 8
        // (user_data) Поэтому STAR[63:48] должен = user_data - 8
        Star::write(
            gdt::GDT.1.user_code,
            gdt::GDT.1.user_data,
            gdt::GDT.1.kernel_code,
            gdt::GDT.1.kernel_data,
        )
        .expect("Невалидные GDT-селекторы для STAR");

        // LSTAR — адрес точки входа syscall_handler
        LStar::write(VirtAddr::from_ptr(syscall_entry as *const ()));

        // SFMASK — при SYSCALL обнуляем IF (прерывания выкл.) и DF
        // IF обнуляется чтобы обработчик мог работать без прерываний,
        // затем мы сами их включим через sti
        SFMask::write(RFlags::INTERRUPT_FLAG | RFlags::DIRECTION_FLAG);
    }

    // Инициализируем стандартные дескрипторы
    {
        let mut fdt = FD_TABLE.lock();
        fdt.slots[0] = Some(FdKind::Stdin);
        fdt.slots[1] = Some(FdKind::Stdout);
        fdt.slots[2] = Some(FdKind::Stderr);
    }
}

// ─── Низкоуровневая точка входа (asm)
// ─────────────────────────────────────────

/// Точка входа SYSCALL (записывается в LSTAR).
///
/// Соглашение при входе:
///   - RCX = RIP пользователя (куда вернуться через SYSRET)
///   - R11 = RFLAGS пользователя
///   - RSP всё ещё указывает на пользовательский стек!
///
/// Мы должны:
///   1. Сохранить пользовательский RSP.
///   2. Переключиться на kernel stack (из TSS.RSP0).
///   3. Сохранить все callee-saved регистры.
///   4. Вызвать rust_syscall_dispatch.
///   5. Восстановить регистры и вернуться через SYSRET.
///
/// # Safety
/// naked-функция — вся логика написана на asm. Rust не генерирует
/// пролог/эпилог.
#[unsafe(naked)]
unsafe extern "C" fn syscall_entry() {
    // SAFETY: Это naked-функция — у неё нет пролога/эпилога от компилятора.
    // Мы полностью контролируем стек и регистры.
    core::arch::naked_asm!(
        // --- Сохраняем пользовательский RSP в ядерном per-CPU хранилище ---
        // Временно используем стек ядра из TSS.RSP0.
        // swapgs + GS-base содержат указатель на kernel stack (в реальной системе).
        // У нас упрощённая модель: переключаемся на статический kernel stack.
        "swapgs",                    // Обмен GS.base и KernelGSBase MSR
        "mov gs:[0], rsp",           // Сохраняем user RSP через GS-base
        "mov rsp, gs:[8]",           // Загружаем kernel RSP из GS-base+8

        // --- Строим kernel stack frame ---
        "push rcx",                  // Сохраняем RIP возврата (от SYSCALL)
        "push r11",                  // Сохраняем RFLAGS пользователя
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        // --- Включаем прерывания в ядре ---
        "sti",

        // --- Вызываем Rust-диспетчер ---
        // Перекладываем Linux syscall ABI в SysV ABI Rust-функции:
        // dispatch(nr, a1, a2, a3, a4, a5, a6).
        "sub rsp, 16",
        "mov [rsp], r9",
        "mov r9, r8",
        "mov r8, r10",
        "mov rcx, rdx",
        "mov rdx, rsi",
        "mov rsi, rdi",
        "mov rdi, rax",
        "call {dispatch}",           // rust_syscall_dispatch(nr, a1, a2, a3, a4, a5, a6) -> i64
        "add rsp, 16",
        // Результат в rax

        // --- Выключаем прерывания перед SYSRET ---
        "cli",

        // --- Восстанавливаем callee-saved регистры ---
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "pop r11",                   // Восстанавливаем RFLAGS пользователя
        "pop rcx",                   // Восстанавливаем RIP пользователя

        // --- Восстанавливаем user RSP ---
        "mov rsp, gs:[0]",
        "swapgs",

        // --- Возврат в Ring 3 ---
        "sysretq",
        dispatch = sym rust_syscall_dispatch,
    );
}

// ─── Диспетчер syscall (Rust)
// ─────────────────────────────────────────────────

/// Диспетчер системных вызовов.
///
/// Вызывается из `syscall_entry` с аргументами в регистрах (Linux x86_64 ABI).
/// Возвращает i64: неотрицательное = успех, отрицательное = -errno.
///
/// # Safety
/// Указатели (`buf`, `path`) приходят из userspace — мы обязаны валидировать
/// их. Текущая реализация делает базовую проверку (non-null, в user-range).
#[no_mangle]
unsafe extern "C" fn rust_syscall_dispatch(
    nr: u64,  // rax — номер syscall
    a1: u64,  // rdi — аргумент 1
    a2: u64,  // rsi — аргумент 2
    a3: u64,  // rdx — аргумент 3
    a4: u64,  // r10 — аргумент 4
    _a5: u64, // r8  — аргумент 5
    _a6: u64, // r9  — аргумент 6
) -> i64 {
    // 1. Проверяем белый список syscall-ов для текущего процесса
    let pid = crate::scheduler::CURRENT_PID[0].load(core::sync::atomic::Ordering::Relaxed);
    if let Some(proc_arc) = crate::process::find_process(pid) {
        let whitelist = proc_arc.lock().syscall_whitelist;
        if nr < 64 && (whitelist & (1 << nr)) == 0 {
            // Syscall запрещен политикой безопасности
            return EPERM;
        }
    }

    match nr {
        0 => sys_write(a1 as i32, a2 as *const u8, a3 as usize),
        1 => sys_exit(a1 as i32),
        2 => sys_getpid(),
        3 => sys_sleep(a1),
        4 => sys_yield(),
        5 => sys_open(a1 as *const u8, a2 as usize, a3 as u32),
        6 => sys_read(a1 as i32, a2 as *mut u8, a3 as usize),
        7 => sys_close(a1 as i32),
        8 => sys_mkdir(a1 as *const u8, a2 as usize),
        9 => sys_unlink(a1 as *const u8, a2 as usize),
        10 => sys_gettime(),
        11 => sys_shm_get(a1 as u32, a2 as usize),
        12 => sys_shm_at(a1 as u32, a2),
        13 => sys_send_msg(a1 as u32, a2 as *const u8, a3 as usize),
        14 => sys_recv_msg(a1 as *mut u8, a2 as usize),
        15 => sys_signal(a1 as u8, a2),
        16 => sys_kill(a1 as u32, a2 as u8),
        17 => sys_fork(),
        18 => sys_exec(a1 as *const u8, a2 as usize, a3 as *const u8, a4 as usize),
        _ => ENOSYS,
    }
}

// ─── Вспомогательные функции ─────────────────────────────────────────────────

/// Максимальный допустимый адрес userspace (ниже kernel half)
const USER_ADDR_MAX: u64 = 0x0000_7FFF_FFFF_FFFF;

fn checked_user_addr(raw: u64) -> Option<VirtAddr> {
    (raw != 0 && raw <= USER_ADDR_MAX).then(|| VirtAddr::new(raw))
}

fn current_pid() -> crate::process::Pid {
    crate::scheduler::CURRENT_PID[0].load(core::sync::atomic::Ordering::Relaxed)
}

/// Проверяет, что ptr..ptr+len находится в user-space и не null.
///
/// # Safety
/// Не проверяет реальный mapping — только диапазон адресов.
unsafe fn validate_user_ptr(ptr: u64, len: usize) -> bool {
    if ptr == 0 || len == 0 {
        return false;
    }
    checked_user_addr(ptr).is_some_and(|addr| crate::paging::is_user_readable(addr, len))
}

/// Читает C-строку из userspace (ptr + явная длина).
/// Возвращает &str если данные валидный UTF-8.
///
/// # Safety
/// ptr должен быть валидным указателем в userspace с len байтами.
unsafe fn user_str<'a>(ptr: *const u8, len: usize) -> Option<&'a str> {
    if !validate_user_ptr(ptr as u64, len) {
        return None;
    }
    let slice = core::slice::from_raw_parts(ptr, len);
    core::str::from_utf8(slice).ok()
}

// ─── Безопасное копирование данных ──────────────────────────────────────────

/// Копирует данные из адресного пространства пользователя в ядро.
/// Выполняет проверку прав доступа к страницам.
pub unsafe fn copy_from_user(dst: *mut u8, src: *const u8, len: usize) -> Result<(), i64> {
    let Some(src_addr) = checked_user_addr(src as u64) else {
        return Err(EFAULT);
    };
    if !crate::paging::is_user_readable(src_addr, len) {
        return Err(EFAULT);
    }
    core::ptr::copy_nonoverlapping(src, dst, len);
    Ok(())
}

/// Копирует данные из ядра в адресное пространство пользователя.
/// Выполняет проверку прав доступа к страницам.
pub unsafe fn copy_to_user(dst: *mut u8, src: *const u8, len: usize) -> Result<(), i64> {
    let Some(dst_addr) = checked_user_addr(dst as u64) else {
        return Err(EFAULT);
    };
    if !crate::paging::is_user_writable(dst_addr, len) {
        return Err(EFAULT);
    }
    core::ptr::copy_nonoverlapping(src, dst, len);
    Ok(())
}

// ─── Реализации системных вызовов
// ─────────────────────────────────────────────

/// sys_write(fd, buf, len) → bytes_written
///
/// fd 0 = stdin (игнорируем), 1/2 = stdout/stderr (→ терминал).
/// Другие fd → VFS.
unsafe fn sys_write(fd: i32, buf: *const u8, len: usize) -> i64 {
    if len == 0 {
        return 0;
    }
    let Some(buf_addr) = checked_user_addr(buf as u64) else {
        return EFAULT;
    };
    if !crate::paging::is_user_readable(buf_addr, len) {
        return EFAULT;
    }

    let data = core::slice::from_raw_parts(buf, len);

    match fd {
        1 | 2 => {
            // stdout / stderr → терминал (через fast_terminal напрямую)
            if let Ok(s) = core::str::from_utf8(data) {
                if let Some(disp) = crate::graphics::GLOBAL_DISPLAY.lock().as_mut() {
                    if let Some(term) = crate::fast_terminal::TERMINAL.lock().as_mut() {
                        term.print_str(disp, s);
                    }
                }
            }
            len as i64
        },
        fd if fd >= 3 => {
            // Файловый дескриптор — пишем в VFS
            let fdt = FD_TABLE.lock();
            match fdt.get(fd as usize) {
                Some(FdKind::File(path)) => {
                    let path = path.clone();
                    drop(fdt);
                    let data_vec = alloc::vec::Vec::from(data);
                    match crate::vfs::GLOBAL_VFS.lock().write_file_as(
                        &path,
                        &data_vec,
                        current_pid(),
                    ) {
                        Ok(_) => len as i64,
                        Err(_) => EBADF,
                    }
                },
                _ => EBADF,
            }
        },
        _ => EBADF,
    }
}

/// sys_exit(code) → !
///
/// Завершает текущий процесс. В нашей модели — паника/halt.
unsafe fn sys_exit(code: i32) -> i64 {
    // Получаем PID текущего процесса (заглушка — в реальной системе из per-CPU)
    let pid = crate::scheduler::CURRENT_PID[0].load(core::sync::atomic::Ordering::Relaxed);
    if pid != 0 {
        crate::process::exit_process(pid, code);
    }
    // Переходим в состояние ожидания — реальный планировщик подберёт следующую
    // задачу Сейчас просто halt
    // sys_exit вызван
    let _ = code;
    let _ = pid;
    loop {
        x86_64::instructions::hlt();
    }
}

/// sys_getpid() → pid
unsafe fn sys_getpid() -> i64 {
    crate::scheduler::CURRENT_PID[0].load(core::sync::atomic::Ordering::Relaxed) as i64
}

/// sys_sleep(ms) — busy-wait приблизительно ms миллисекунд
unsafe fn sys_sleep(ms: u64) -> i64 {
    // Простая эмуляция через NOP-циклы. В реальной системе — timer + block.
    // ~100_000 итераций ≈ 1ms на QEMU при ~2GHz
    let iterations = ms.saturating_mul(100_000);
    for _ in 0..iterations {
        core::arch::asm!("nop", options(nomem, nostack));
    }
    0
}

/// sys_yield() — отдаёт квант планировщику
unsafe fn sys_yield() -> i64 {
    // Пока кооперативная многозадачность — просто hint
    core::arch::asm!("nop", options(nomem, nostack));
    0
}

/// sys_open(path_ptr, path_len, flags) → fd
unsafe fn sys_open(path_ptr: *const u8, path_len: usize, _flags: u32) -> i64 {
    let path = match user_str(path_ptr, path_len) {
        Some(p) => alloc::string::String::from(p),
        None => return EFAULT,
    };

    // Проверяем, существует ли файл в VFS
    let exists = crate::vfs::GLOBAL_VFS
        .lock()
        .read_file_as(&path, current_pid())
        .is_ok();
    if !exists {
        return ENOENT;
    }

    // Выделяем fd
    let mut fdt = FD_TABLE.lock();
    match fdt.alloc(FdKind::File(path)) {
        Some(fd) => fd as i64,
        None => ENOMEM,
    }
}

/// sys_read(fd, buf, len) → bytes_read
unsafe fn sys_read(fd: i32, buf: *mut u8, len: usize) -> i64 {
    if len == 0 {
        return 0;
    }
    let Some(buf_addr) = checked_user_addr(buf as u64) else {
        return EFAULT;
    };
    if !crate::paging::is_user_writable(buf_addr, len) {
        return EFAULT;
    }

    match fd {
        0 => {
            // stdin — блокирующее чтение с клавиатуры (не реализовано пока)
            ENOSYS
        },
        fd if fd >= 3 => {
            let fdt = FD_TABLE.lock();
            match fdt.get(fd as usize) {
                Some(FdKind::File(path)) => {
                    let path = path.clone();
                    drop(fdt);
                    match crate::vfs::GLOBAL_VFS
                        .lock()
                        .read_file_as(&path, current_pid())
                    {
                        Ok(data) => {
                            let bytes_to_copy = len.min(data.len());
                            let dst = core::slice::from_raw_parts_mut(buf, bytes_to_copy);
                            dst.copy_from_slice(&data[..bytes_to_copy]);
                            bytes_to_copy as i64
                        },
                        Err(_) => EBADF,
                    }
                },
                _ => EBADF,
            }
        },
        _ => EBADF,
    }
}

/// sys_close(fd) → 0 | -errno
unsafe fn sys_close(fd: i32) -> i64 {
    if fd < 3 {
        return EPERM; // Нельзя закрывать stdin/stdout/stderr
    }
    let mut fdt = FD_TABLE.lock();
    if fdt.free(fd as usize) {
        0
    } else {
        EBADF
    }
}

/// sys_mkdir(path_ptr, path_len) → 0 | -errno
unsafe fn sys_mkdir(path_ptr: *const u8, path_len: usize) -> i64 {
    let path = match user_str(path_ptr, path_len) {
        Some(p) => alloc::string::String::from(p),
        None => return EFAULT,
    };

    match crate::vfs::GLOBAL_VFS.lock().mkdir_as(&path, current_pid()) {
        Ok(_) => 0,
        Err(crate::vfs::FsError::AlreadyExists) => EEXIST,
        Err(crate::vfs::FsError::PermissionDenied) => EPERM,
        Err(_) => EINVAL,
    }
}

/// sys_unlink(path_ptr, path_len) → 0 | -errno
unsafe fn sys_unlink(path_ptr: *const u8, path_len: usize) -> i64 {
    let path = match user_str(path_ptr, path_len) {
        Some(p) => alloc::string::String::from(p),
        None => return EFAULT,
    };

    match crate::vfs::GLOBAL_VFS
        .lock()
        .remove_as(&path, current_pid())
    {
        Ok(_) => 0,
        Err(crate::vfs::FsError::NotFound) => ENOENT,
        Err(crate::vfs::FsError::PermissionDenied) => EPERM,
        Err(_) => EINVAL,
    }
}

/// sys_gettime() → ticks (TSC)
unsafe fn sys_gettime() -> i64 {
    let tsc: u64;
    asm!(
        "rdtsc",
        "shl rdx, 32",
        "or rax, rdx",
        out("rax") tsc,
        out("rdx") _,
        options(nomem, nostack),
    );
    tsc as i64
}

/// sys_shm_get(id, size_pages)
unsafe fn sys_shm_get(id: u32, size_pages: usize) -> i64 {
    match crate::shm::shm_get(id, size_pages) {
        Some(id) => id as i64,
        None => ENOMEM,
    }
}

/// sys_shm_at(id, virt_addr)
unsafe fn sys_shm_at(id: u32, virt_addr: u64) -> i64 {
    if !validate_user_ptr(virt_addr, 4096) {
        return EFAULT;
    }
    let Some(addr) = checked_user_addr(virt_addr) else {
        return EFAULT;
    };
    match crate::shm::map_shm_to_current(id, addr) {
        Ok(_) => 0,
        Err(_) => EINVAL,
    }
}

/// sys_send_msg(pid, buf, len)
unsafe fn sys_send_msg(pid: u32, buf: *const u8, len: usize) -> i64 {
    if len == 0 {
        return 0;
    }
    let Some(buf_addr) = checked_user_addr(buf as u64) else {
        return EFAULT;
    };
    if !crate::paging::is_user_readable(buf_addr, len) {
        return EFAULT;
    }
    let data = core::slice::from_raw_parts(buf, len).to_vec();
    let from_pid = crate::scheduler::CURRENT_PID[0].load(core::sync::atomic::Ordering::Relaxed);

    if let Some(proc_arc) = crate::process::find_process(pid) {
        proc_arc.lock().send_message(crate::process::Message {
            from_pid,
            data,
        });
        0
    } else {
        EBADF
    }
}

/// sys_recv_msg(buf, max_len)
unsafe fn sys_recv_msg(buf: *mut u8, max_len: usize) -> i64 {
    if max_len == 0 {
        return 0;
    }
    let Some(buf_addr) = checked_user_addr(buf as u64) else {
        return EFAULT;
    };
    if !crate::paging::is_user_writable(buf_addr, max_len) {
        return EFAULT;
    }
    let pid = crate::scheduler::CURRENT_PID[0].load(core::sync::atomic::Ordering::Relaxed);
    if let Some(proc_arc) = crate::process::find_process(pid) {
        if let Some(msg) = proc_arc.lock().receive_message() {
            let len = max_len.min(msg.data.len());
            let dst = core::slice::from_raw_parts_mut(buf, len);
            dst.copy_from_slice(&msg.data[..len]);
            return len as i64;
        }
    }
    0 // No message
}

/// sys_signal(sig, handler_addr)
unsafe fn sys_signal(sig: u8, handler: u64) -> i64 {
    let Some(handler_addr) = checked_user_addr(handler) else {
        return EFAULT;
    };
    let pid = crate::scheduler::CURRENT_PID[0].load(core::sync::atomic::Ordering::Relaxed);
    if let Some(proc_arc) = crate::process::find_process(pid) {
        proc_arc.lock().set_signal_handler(sig, handler_addr);
        0
    } else {
        EPERM
    }
}

/// sys_kill(pid, sig)
unsafe fn sys_kill(pid: u32, sig: u8) -> i64 {
    if let Some(proc_arc) = crate::process::find_process(pid) {
        proc_arc.lock().send_signal(sig);
        0
    } else {
        EBADF
    }
}

/// sys_fork() --- creates a new process as a copy of the current one
unsafe fn sys_fork() -> i64 {
    // Address-space duplication and scheduler context installation are not
    // implemented.
    ENOSYS
}

/// sys_exec(path, path_len, args, args_len) --- execute a new program
unsafe fn sys_exec(
    _path_ptr: *const u8,
    _path_len: usize,
    _args_ptr: *const u8,
    _args_len: usize,
) -> i64 {
    // Loading an ELF into a scheduled Ring 3 process is not implemented yet.
    ENOSYS
}
