use lazy_static::lazy_static;
use pic8259::ChainedPics;
use spin::Mutex;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

use crate::{gdt, keyboard, mouse};

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: Mutex<ChainedPics> =
    Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Keyboard,
    Network = PIC_1_OFFSET + 11, // IRQ 11
    Mouse = PIC_2_OFFSET + 4,    // IRQ 12
}

impl InterruptIndex {
    fn as_u8(self) -> u8 {
        self as u8
    }
    fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}

lazy_static! {
    pub static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        // 1. Double fault
        unsafe {
            idt.double_fault.set_handler_fn(double_fault_handler)
                .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }

        // 1.5. Page Fault & General Protection Fault
        idt.page_fault.set_handler_fn(page_fault_handler);
        idt.general_protection_fault.set_handler_fn(general_protection_fault_handler);

        // 2. Hardware Timer
        idt[InterruptIndex::Timer.as_usize()].set_handler_fn(timer_interrupt_handler);

        // 3. Hardware Keyboard
        idt[InterruptIndex::Keyboard.as_usize()].set_handler_fn(keyboard_interrupt_handler);

        // 4. Hardware Mouse
        idt[InterruptIndex::Mouse.as_usize()].set_handler_fn(mouse_interrupt_handler);

        // 5. Hardware Network (RTL8139)
        idt[InterruptIndex::Network.as_usize()].set_handler_fn(network_interrupt_handler);

        idt
    };
}

pub fn init_idt() {
    IDT.load();
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    let faulting_addr = Cr2::read();

    // Пробуем обработать demand page fault или CoW fault
    if crate::paging::handle_page_fault(faulting_addr, error_code) {
        // Успешно обработано — возвращаемся из прерывания
        return;
    }

    // Не удалось обработать → kernel panic
    panic!(
        "EXCEPTION: PAGE FAULT (unhandled)\nAccessed Address: {:?}\nError Code: {:?}\n{:#?}",
        faulting_addr, error_code, stack_frame
    );
}

extern "x86-interrupt" fn general_protection_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    panic!(
        "EXCEPTION: GENERAL PROTECTION FAULT\nError Code: {}\n{:#?}",
        error_code, stack_frame
    );
}

extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // Тик часов GUI
    crate::gui::clock_tick();
    // CFS accounting exists, but live desktop futures are still owned by
    // task::Executor.
    let _ = crate::scheduler::scheduler_tick();
    crate::net::tcp::retransmit_tick();

    // Периодически чистим zombie процессы (каждые 100 тиков)
    static REAP_COUNTER: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
    let count = REAP_COUNTER.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    if count % 100 == 99 {
        crate::scheduler::SCHEDULER.lock().reap_zombies();
    }

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // Отправляем сканкод в модуль клавиатуры
    keyboard::handle_keyboard_interrupt();

    // Сигнализируем контроллеру PIC об успешной обработке
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}

extern "x86-interrupt" fn mouse_interrupt_handler(_stack_frame: InterruptStackFrame) {
    mouse::handle_mouse_interrupt();
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Mouse.as_u8());
    }
}
extern "x86-interrupt" fn network_interrupt_handler(_stack_frame: InterruptStackFrame) {
    crate::net::rtl8139::handle_interrupt();
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Network.as_u8());
    }
}
