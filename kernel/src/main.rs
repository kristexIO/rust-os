// ============================================================
// Ядро ОС — RamFS, Terminal, Shell
// ============================================================
#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
#![feature(abi_x86_interrupt)]

extern crate alloc;

use alloc::string::String;
use core::panic::PanicInfo;

use bootloader_api::{entry_point, BootInfo};

mod ahci;
mod allocator;
#[macro_use]
mod logging;
mod apps;
mod ata;
mod calculator;
mod calendar;
mod cmos;
mod cow;
mod crypto;
mod disk;
mod driver;
mod elf;
mod fast_terminal;
mod frame_allocator;
mod gdt;
mod graphics;
mod gui;
mod html_parser;
mod image_viewer;
mod interrupts;
mod kaslr;
mod keyboard;
mod math;
mod memory_allocators;
pub mod memory_isolation;
pub mod mouse;
mod mp3_decoder;
mod music_player;
mod net;
mod paging;
pub mod partition;
pub mod pci;
pub mod pcie;
pub mod pcspeaker;
pub mod power;
pub mod process;
mod ramfs;
mod scheduler;
mod security;
mod shell;
pub mod shm;
mod stack_canary;
mod syscall;
mod system_monitor;
mod task;
mod test_framework;
mod tests;
pub mod theme;
mod timer;
pub mod usb;
mod userspace;
mod vfs;
mod widgets;
mod window;

use core::fmt::Write;

use graphics::Display;

struct TerminalWriter<'a> {
    term: &'a mut fast_terminal::Terminal,
    disp: &'a mut graphics::Display,
}

impl<'a> core::fmt::Write for TerminalWriter<'a> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.term.print_str(self.disp, s);
        Ok(())
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    unsafe {
        // Забираем локи принудительно, чтобы избежать дедлока при панике!
        graphics::GLOBAL_DISPLAY.force_unlock();
        fast_terminal::TERMINAL.force_unlock();

        if let Some(disp) = graphics::GLOBAL_DISPLAY.lock().as_mut() {
            // Синий экран смерти (BSOD)
            disp.clear_screen(graphics::Color::from_u32(0x0000AA)); // Синий фон

            if let Some(term) = fast_terminal::TERMINAL.lock().as_mut() {
                // Сбрасываем курсор на (0, 0)
                term.reset_cursor();
                term.print_str(disp, "\n\n  *** FATAL ERROR: KERNEL PANIC ***\n\n");

                // Используем Writer без аллокаций
                let mut writer = TerminalWriter {
                    term,
                    disp,
                };
                let _ = write!(&mut writer, "  {}\n", info);

                term.print_str(disp, "\n\n  SYSTEM HALTED. PLEASE RESTART.");
            }
            // Вывод BSOD на экран
            disp.flush();
        }
    }
    loop {
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    allocator::init();
    logging::init();

    // Инициализируем физический аллокатор фреймов по memory map загрузчика.
    // Должно быть ДО любого использования paging / CoW / demand paging.
    frame_allocator::init_frame_allocator(&boot_info.memory_regions);

    let fb = match boot_info.framebuffer.as_mut() {
        Some(fb) => fb,
        None => {
            panic!(
                "WARNING: No framebuffer provided by the bootloader. GUI OS requires a \
                 framebuffer to run."
            );
        },
    };

    let mut display = Display::new(fb);
    display.clear_screen(graphics::Color::BLACK);

    // Передаем Display в глобальный Mutex ДО инициализации графики
    *graphics::GLOBAL_DISPLAY.lock() = Some(display);

    // Создаем Окно Терминала
    let term_win = alloc::sync::Arc::new(spin::Mutex::new(window::Window::new(
        10,
        10,
        780,
        580,
        alloc::string::String::from("Terminal"),
    )));
    gui::COMPOSITOR.lock().add_window(term_win.clone());
    // Инициализация KASLR и защиты стека
    kaslr::init_kaslr();
    stack_canary::StackCanaryProtection::init();

    println!("Security: ACL and syscall checks active; KASLR and stack canaries are prototypes.");

    // Инициализируем виртуальный терминал под размер окна
    fast_terminal::init(780, 580);

    // Инициализация базовых подсистем
    {
        let mut term_lock = fast_terminal::TERMINAL.lock();
        if let Some(term) = term_lock.as_mut() {
            let mut win_lock = term_win.lock();

            term.print_str(&mut *win_lock, "[kernel] Initializing GDT...\n");
            gdt::init();
            term.print_str(
                &mut *win_lock,
                "[kernel] Initializing Syscalls (Ring 3 MSRs)...\n",
            );
            syscall::init();
            term.print_str(&mut *win_lock, "[kernel] Initializing Paging...\n");
            // Инициализация пейджинга (включаем управление виртуальной памятью)
            match boot_info.physical_memory_offset {
                bootloader_api::info::Optional::Some(phys_mem_offset) => {
                    unsafe {
                        paging::init(phys_mem_offset);
                    }
                    term.print_str(&mut *win_lock, "[kernel] Paging initialized.\n");
                },
                bootloader_api::info::Optional::None => {
                    term.print_str(
                        &mut *win_lock,
                        "[kernel] WARNING: Physical memory offset not provided, skipping paging.\n",
                    );
                },
            }
            term.print_str(&mut *win_lock, "[kernel] Initializing Process Manager...\n");
            // Инициализация процессного менеджера
            term.print_str(
                &mut *win_lock,
                "[kernel] Process structures initialized; desktop uses async executor.\n",
            );
            term.print_str(&mut *win_lock, "[kernel] Initializing IDT...\n");
            interrupts::init_idt();
            term.print_str(&mut *win_lock, "[kernel] Initializing PIC8259 & Mouse...\n");
            mouse::init();
            unsafe { interrupts::PICS.lock().initialize() };
            term.print_str(&mut *win_lock, "[kernel] Enabling Hardware Interrupts...\n");
            x86_64::instructions::interrupts::enable();
        }
    } // Drop locks

    pci::init();

    // Инициализируем VFS: монтируем RamFS как корень ("/")
    {
        if let Some(term) = fast_terminal::TERMINAL.lock().as_mut() {
            term.print_str(
                &mut *term_win.lock(),
                "[kernel] Initializing VFS + RamFS...\n",
            );
        }
    }
    let ramfs = alloc::sync::Arc::new(ramfs::create_default_ramfs());
    vfs::GLOBAL_VFS.lock().mount(String::from("/"), ramfs);

    #[cfg(feature = "test-mode")]
    {
        let stats = test_framework::run_headless_tests();
        test_framework::exit_qemu(stats.failed == 0);
    }

    {
        if let Some(term) = fast_terminal::TERMINAL.lock().as_mut() {
            term.print_str(
                &mut *term_win.lock(),
                "[kernel] Initializing Network (RTL8139)...\n",
            );
        }
    }
    net::init();

    {
        if let Some(term) = fast_terminal::TERMINAL.lock().as_mut() {
            term.print_str(
                &mut *term_win.lock(),
                "[kernel] VFS ready. Use 'format_disk' then 'mount /mnt' for FAT32.\n",
            );
        }
    }

    // Обновляем экран впервые
    if let Some(disp) = graphics::GLOBAL_DISPLAY.lock().as_mut() {
        gui::COMPOSITOR.lock().composite(disp);
    }

    // Запуск Executor-а и спавн тасок
    let mut executor = task::Executor::new();

    // 0. Таска Приветственного звука ОС (Startup Chime)
    executor.spawn(task::Task::new("PC Speaker Chime", async {
        // C-Major Arpeggio -> High C
        let startup_tune = [
            (523, 2),  // C5
            (659, 2),  // E5
            (784, 2),  // G5
            (1046, 6), // C6
        ];
        crate::pcspeaker::PcSpeaker::play_sequence(&startup_tune).await;
    }));

    // 1. Таска GUI-мыши (Асинхронно ждет пакетов)
    executor.spawn(task::Task::new("PS/2 Mouse Driver", gui::mouse_task()));

    // 2. Таска Шелла
    // Используем Proper shell state management without unsafe lifetime extension
    let shell_task = {
        let mut term_lock = fast_terminal::TERMINAL.lock();
        if let Some(_term) = term_lock.as_mut() {
            // Создаем Shell с собственным состоянием, без unsafe transmute
            let mut shell = shell::Shell::new();
            let term_win_clone = term_win.clone();

            Some(task::Task::new("Terminal Shell", async move {
                shell.interactive_task(term_win_clone).await;
            }))
        } else {
            None
        }
    };

    if let Some(shell_task) = shell_task {
        executor.spawn(shell_task);

        // Начинаем бесконечный кооперативный Event Loop
        executor.run();
    } else {
        panic!("Terminal not initialized!");
    }
}
