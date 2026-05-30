//! Расширенный стек USB для Rust OS.
//! Поддерживает обнаружение контроллеров UHCI/EHCI и базовый HID (мышь/клава).

use alloc::vec::Vec;

use spin::Mutex;

use crate::{driver::*, pci};

/// Типы контроллеров
#[derive(Debug, Clone, Copy)]
pub enum UsbControllerType {
    Uhci,
    Ohci,
    Ehci,
    Xhci,
}

pub struct UsbController {
    pub bus: u8,
    pub slot: u8,
    pub func: u8,
    pub mmio_base: u64,
    pub kind: UsbControllerType,
}

pub struct UsbManager {
    controllers: Vec<UsbController>,
}

pub static USB_MANAGER: Mutex<UsbManager> = Mutex::new(UsbManager::new());

impl UsbManager {
    pub const fn new() -> Self {
        UsbManager {
            controllers: Vec::new(),
        }
    }

    pub fn init(&mut self) {
        log::info!("USB: Scanning for controllers...");
        let devices = pci::PCI_DEVICES.lock();
        for dev in devices.iter() {
            if dev.class == 0x0C && dev.subclass == 0x03 {
                let kind =
                    match pci::read_config_dword(dev.bus, dev.slot, dev.func, 0x08) >> 8 & 0xFF {
                        0x00 => Some(UsbControllerType::Uhci),
                        0x10 => Some(UsbControllerType::Ohci),
                        0x20 => Some(UsbControllerType::Ehci),
                        0x30 => Some(UsbControllerType::Xhci),
                        _ => None,
                    };

                if let Some(k) = kind {
                    let bar = pci::read_bar(dev.bus, dev.slot, dev.func, 0);
                    if bar != 0 {
                        log::info!(
                            "USB: Found {:?} at B{}:S{}:F{}",
                            k,
                            dev.bus,
                            dev.slot,
                            dev.func
                        );
                        pci::enable_device(dev.bus, dev.slot, dev.func);
                        self.controllers.push(UsbController {
                            bus: dev.bus,
                            slot: dev.slot,
                            func: dev.func,
                            mmio_base: bar,
                            kind: k,
                        });
                    }
                }
            }
        }

        if self.controllers.is_empty() {
            log::warn!("USB: No controllers found.");
        }
    }
}

pub fn init_usb() {
    USB_MANAGER.lock().init();
}

impl Driver for UsbManager {
    fn name(&self) -> &'static str {
        "bus:usb"
    }

    fn init(&mut self) -> Result<(), DriverError> {
        self.init();
        Ok(())
    }

    fn shutdown(&mut self) {}
    fn priority(&self) -> u8 {
        1
    }
}
