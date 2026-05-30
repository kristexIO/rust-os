use alloc::{format, vec::Vec};

use spin::Mutex;
use x86_64::instructions::port::Port;

const CONFIG_ADDRESS: u16 = 0xCF8;
const CONFIG_DATA: u16 = 0xCFC;

// ─── PCI Class/Subclass constants
// ─────────────────────────────────────────────
pub const CLASS_MASS_STORAGE: u8 = 0x01;
pub const SUBCLASS_IDE: u8 = 0x01;
pub const SUBCLASS_AHCI: u8 = 0x06;
pub const SUBCLASS_NVME: u8 = 0x08;
pub const CLASS_NETWORK: u8 = 0x02;
pub const CLASS_DISPLAY: u8 = 0x03;

// ─── PCI Config Space offsets
// ─────────────────────────────────────────────────
pub const PCI_VENDOR_ID: u8 = 0x00;
pub const PCI_DEVICE_ID: u8 = 0x02;
pub const PCI_COMMAND: u8 = 0x04;
pub const PCI_CLASS_REVISION: u8 = 0x08;
pub const PCI_HEADER_TYPE: u8 = 0x0C;
pub const PCI_BAR0: u8 = 0x10;
pub const PCI_BAR1: u8 = 0x14;
pub const PCI_BAR2: u8 = 0x18;
pub const PCI_BAR3: u8 = 0x1C;
pub const PCI_BAR4: u8 = 0x20;
pub const PCI_BAR5: u8 = 0x24;
pub const PCI_INTERRUPT_LINE: u8 = 0x3C;

/// Записывает dword в PCI config space
pub fn write_config_dword(bus: u8, slot: u8, func: u8, offset: u8, value: u32) {
    let address = ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | (offset as u32 & 0xFC)
        | 0x80000000;

    let mut addr_port = Port::<u32>::new(CONFIG_ADDRESS);
    let mut data_port = Port::<u32>::new(CONFIG_DATA);

    unsafe {
        addr_port.write(address);
        data_port.write(value);
    }
}

pub fn read_config_dword(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
    let address = ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | (offset as u32 & 0xFC)
        | 0x80000000;

    let mut addr_port = Port::<u32>::new(CONFIG_ADDRESS);
    let mut data_port = Port::<u32>::new(CONFIG_DATA);

    unsafe {
        addr_port.write(address);
        data_port.read()
    }
}

/// Читает BAR (Base Address Register) и возвращает адрес MMIO
pub fn read_bar(bus: u8, slot: u8, func: u8, bar_num: u8) -> u64 {
    let offset = PCI_BAR0 + bar_num * 4;
    let bar = read_config_dword(bus, slot, func, offset);

    if bar & 1 != 0 {
        // I/O BAR — возвращаем 0 (не MMIO)
        return 0;
    }

    let bar_type = (bar >> 1) & 0x3;
    match bar_type {
        0 => {
            // 32-bit MMIO
            (bar & !0xF) as u64
        },
        2 => {
            // 64-bit MMIO — читаем также следующий BAR
            let bar_high = read_config_dword(bus, slot, func, offset + 4);
            ((bar_high as u64) << 32) | ((bar & !0xF) as u64)
        },
        _ => 0,
    }
}

/// Включает Bus Master и Memory Space в PCI Command Register
pub fn enable_device(bus: u8, slot: u8, func: u8) {
    let cmd = read_config_dword(bus, slot, func, PCI_COMMAND);
    // Bit 1 = Memory Space Enable, Bit 2 = Bus Master Enable
    write_config_dword(bus, slot, func, PCI_COMMAND, cmd | 0x06);
}

pub fn get_vendor_id(bus: u8, slot: u8, func: u8) -> u16 {
    (read_config_dword(bus, slot, func, 0) & 0xFFFF) as u16
}

pub fn get_device_id(bus: u8, slot: u8, func: u8) -> u16 {
    ((read_config_dword(bus, slot, func, 0) >> 16) & 0xFFFF) as u16
}

pub fn get_class_id(bus: u8, slot: u8, func: u8) -> u8 {
    ((read_config_dword(bus, slot, func, 0x08) >> 24) & 0xFF) as u8
}

pub fn get_subclass(bus: u8, slot: u8, func: u8) -> u8 {
    ((read_config_dword(bus, slot, func, 0x08) >> 16) & 0xFF) as u8
}

pub fn check_device(bus: u8, slot: u8) {
    let func = 0;
    let vendor = get_vendor_id(bus, slot, func);
    if vendor == 0xFFFF {
        return; // Device doesn't exist
    }

    let device = get_device_id(bus, slot, func);
    let class = get_class_id(bus, slot, func);
    let subclass = get_subclass(bus, slot, func);

    let mut disp_lock = crate::graphics::GLOBAL_DISPLAY.lock();
    if let Some(disp) = disp_lock.as_mut() {
        let mut term_lock = crate::fast_terminal::TERMINAL.lock();
        if let Some(term) = term_lock.as_mut() {
            term.print_str(
                disp,
                &format!(
                    "[pci] B{}:S{}:F{} | Vendor: 0x{:04X}, Dev: 0x{:04X} | Class: 0x{:02X}, Sub: \
                     0x{:02X}\n",
                    bus, slot, func, vendor, device, class, subclass
                ),
            );
        }
    }
}

fn log_terminal(s: &str) {
    let mut disp_lock = crate::graphics::GLOBAL_DISPLAY.lock();
    if let Some(disp) = disp_lock.as_mut() {
        let mut term_lock = crate::fast_terminal::TERMINAL.lock();
        if let Some(term) = term_lock.as_mut() {
            term.print_str(disp, s);
        }
    }
}

/// Описатель найденного PCI устройства
#[derive(Debug, Clone, Copy)]
pub struct PciDevice {
    pub bus: u8,
    pub slot: u8,
    pub func: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class: u8,
    pub subclass: u8,
    pub bar5: u64, // AHCI использует BAR5
}

lazy_static::lazy_static! {
    pub static ref PCI_DEVICES: Mutex<Vec<PciDevice>> = Mutex::new(Vec::new());
}

pub fn init() {
    {
        let mut disp_lock = crate::graphics::GLOBAL_DISPLAY.lock();
        if let Some(disp) = disp_lock.as_mut() {
            let mut term_lock = crate::fast_terminal::TERMINAL.lock();
            if let Some(term) = term_lock.as_mut() {
                term.print_str(disp, "[kernel] Scanning PCI Bus...\n");
            }
        }
    }

    // Сканируем все 256 шин, 32 слота, 8 функций
    // (для QEMU обычно достаточно Bus 0, но поддерживаем полное сканирование)
    for bus in 0u8..=4 {
        for slot in 0u8..32 {
            let vendor = get_vendor_id(bus, slot, 0);
            if vendor == 0xFFFF {
                continue;
            }

            let func_count = {
                let hdr = (read_config_dword(bus, slot, 0, PCI_HEADER_TYPE) >> 16) & 0xFF;
                if hdr & 0x80 != 0 {
                    8u8
                } else {
                    1u8
                }
            };

            for func in 0..func_count {
                let vendor = get_vendor_id(bus, slot, func);
                if vendor == 0xFFFF {
                    continue;
                }

                let device = get_device_id(bus, slot, func);
                let class = get_class_id(bus, slot, func);
                let subclass = get_subclass(bus, slot, func);

                let pci_dev = PciDevice {
                    bus,
                    slot,
                    func,
                    vendor_id: vendor,
                    device_id: device,
                    class,
                    subclass,
                    bar5: if class == CLASS_MASS_STORAGE && subclass == SUBCLASS_AHCI {
                        read_bar(bus, slot, func, 5)
                    } else {
                        0
                    },
                };
                PCI_DEVICES.lock().push(pci_dev);

                check_device(bus, slot);

                // Инициализируем AHCI если нашли SATA-контроллер
                if class == CLASS_MASS_STORAGE && subclass == SUBCLASS_AHCI {
                    let abar = pci_dev.bar5;
                    if abar != 0 {
                        enable_device(bus, slot, func);
                        log_terminal("[pci] AHCI controller found\n");
                        crate::ahci::init(abar);
                    }
                }

                // VirtIO-GPU
                if vendor == 0x1AF4 && device == 0x1050 {
                    let bar4 = read_bar(bus, slot, func, 4);
                    if bar4 != 0 {
                        enable_device(bus, slot, func);
                        log_terminal("[pci] VirtIO-GPU device found\n");
                        let drv = crate::driver::virtio_gpu::VirtioGpuDriver::new(bar4);
                        let _ = crate::driver::registry::REGISTRY
                            .register(alloc::sync::Arc::new(spin::Mutex::new(drv)));
                    }
                }

                // Intel E1000
                if vendor == 0x8086 && device == 0x100E {
                    let bar0 = read_bar(bus, slot, func, 0);
                    if bar0 != 0 {
                        enable_device(bus, slot, func);
                        log_terminal("[pci] Intel E1000 NIC found\n");
                        let drv = crate::driver::e1000::E1000Driver::new(bar0);
                        let _ = crate::driver::registry::REGISTRY
                            .register(alloc::sync::Arc::new(spin::Mutex::new(drv)));
                    }
                }

                if func == 0 {
                    break;
                } // Не multi-function — уходим
            }
        }
    }
}
