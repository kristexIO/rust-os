// power.rs — System power management (Shutdown and Reboot)

use x86_64::instructions::{hlt, port::Port};

/// Reboots the system using the 8042 PS/2 controller
pub fn reboot() -> ! {
    crate::println!("[power] Rebooting system...");
    let mut port = Port::<u8>::new(0x64);
    unsafe {
        // Wait for the input buffer to be empty
        while port.read() & 0x02 != 0 {}
        // Pulse the CPU reset line
        port.write(0xFE);
    }
    loop {
        hlt();
    }
}

/// Attempts to shut down the system.
/// This uses QEMU/Bochs specific ACPI ports for a quick shutdown without
/// parsing complex ACPI tables.
pub fn shutdown() -> ! {
    crate::println!("[power] Shutting down system...");

    unsafe {
        // QEMU ACPI shutdown port
        let mut qemu_port = Port::<u16>::new(0x604);
        qemu_port.write(0x2000);

        // Bochs & older QEMU ACPI shutdown port
        let mut bochs_port = Port::<u16>::new(0xB004);
        bochs_port.write(0x2000);

        // VirtualBox poweroff
        let mut vbox_port = Port::<u16>::new(0x4004);
        vbox_port.write(0x3400);
    }

    // If shutdown failed (e.g., real hardware without these ports)
    crate::println!("[power] Shutdown failed (unsupported hardware). System halted.");
    loop {
        // Disable interrupts and halt
        x86_64::instructions::interrupts::disable();
        hlt();
    }
}
