use core::arch::asm;

use x86_64::registers::rflags::RFlags;

use crate::gdt;

/// Искусственная пользовательская программа.
/// Вызывает sys_print (syscall #0) и sys_exit (syscall #1).
#[no_mangle]
#[inline(never)]
pub extern "C" fn dummy_user_program() {
    let msg = b"Hello from userspace!\n";
    let ptr = msg.as_ptr() as u64;
    let len = msg.len() as u64;

    unsafe {
        // Вызов syscall #0 (sys_print)
        // C ABI: args in RDI, RSI, RDX, R10, R8, R9. Syscall num in RAX.
        asm!(
            "syscall",
            in("rax") 0, // sys_print
            in("rdi") ptr,
            in("rsi") len,
            out("rcx") _, // syscall clobbers RCX
            out("r11") _, // syscall clobbers R11
        );

        // Вызов syscall #1 (sys_exit)
        asm!(
            "syscall",
            in("rax") 1, // sys_exit
            in("rdi") 42, // exit code
            out("rcx") _,
            out("r11") _,
        );
    }

    // Сюда не должны дойти
    loop {}
}

/// Выделяет стек и переходит в пользовательскую программу по указанному адресу.
pub fn jump_to_userspace(entry_point: u64) {
    // 1. Создаем "пользовательский" стек (например, 8KB)
    let mut stack = alloc::vec![0u8; 8192];
    let stack_top = stack.as_mut_ptr() as u64 + 8192;

    // В rust-os мы обычно предотвращаем удаление стека, утекая его:
    core::mem::forget(stack);

    // 2. Подготавливаем стек для IRETQ
    // Формат стека для iretq:
    // SS
    // RSP
    // RFLAGS
    // CS
    // RIP

    let data_sel = gdt::GDT.1.user_data.0 as u64;
    let code_sel = gdt::GDT.1.user_code.0 as u64;

    // RFLAGS: важно установить IF (прерывания включены), иначе мы зависнем
    let rflags = (RFlags::INTERRUPT_FLAG | RFlags::IOPL_LOW | RFlags::IOPL_HIGH).bits();

    // Сбрасываем курсор терминала перед переходом в userspace
    if let Some(term) = crate::fast_terminal::TERMINAL.lock().as_mut() {
        term.reset_cursor();
    }

    unsafe {
        asm!(
            "cli",       // Отключаем прерывания на момент переключения
            "push {ss}", // User SS
            "push {rsp}",// User RSP
            "push {rflags}", // User RFLAGS
            "push {cs}", // User CS
            "push {rip}",// User RIP
            "iretq",     // Переход в Ring 3
            ss = in(reg) data_sel,
            rsp = in(reg) stack_top,
            rflags = in(reg) rflags,
            cs = in(reg) code_sel,
            rip = in(reg) entry_point,
        );
    }
}
