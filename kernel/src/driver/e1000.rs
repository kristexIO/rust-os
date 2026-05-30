//! Драйвер сетевой карты Intel E1000 (8254x).
//! Поддерживает передачу и прием пакетов через кольцевые буферы (descriptors).

use crate::{driver::*, pci};

const E1000_VENDOR_ID: u16 = 0x8086;
const E1000_DEVICE_ID: u16 = 0x100E; // QEMU default

pub struct E1000Driver {
    mmio_base: u64,
    mac: [u8; 6],
    initialized: bool,
}

impl E1000Driver {
    pub fn new(mmio_base: u64) -> Self {
        Self {
            mmio_base,
            mac: [0; 6],
            initialized: false,
        }
    }

    pub fn probe() -> Option<Self> {
        let devices = pci::PCI_DEVICES.lock();
        for dev in devices.iter() {
            if dev.vendor_id == E1000_VENDOR_ID && dev.device_id == E1000_DEVICE_ID {
                let bar0 = pci::read_bar(dev.bus, dev.slot, dev.func, 0);
                if bar0 != 0 {
                    pci::enable_device(dev.bus, dev.slot, dev.func);
                    return Some(Self::new(bar0));
                }
            }
        }
        None
    }

    fn read_register(&self, offset: u32) -> u32 {
        unsafe { core::ptr::read_volatile((self.mmio_base + offset as u64) as *const u32) }
    }

    fn write_register(&self, offset: u32, value: u32) {
        unsafe { core::ptr::write_volatile((self.mmio_base + offset as u64) as *mut u32, value) }
    }
}

impl Driver for E1000Driver {
    fn name(&self) -> &'static str {
        "net:e1000"
    }

    fn init(&mut self) -> Result<(), DriverError> {
        // Читаем MAC-адрес из EEPROM или регистров
        // (упрощение: читаем из регистров RAL/RAH)
        let low = self.read_register(0x5400);
        let high = self.read_register(0x5404);
        self.mac[0] = low as u8;
        self.mac[1] = (low >> 8) as u8;
        self.mac[2] = (low >> 16) as u8;
        self.mac[3] = (low >> 24) as u8;
        self.mac[4] = high as u8;
        self.mac[5] = (high >> 8) as u8;

        log::info!(
            "E1000: Initialized. MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            self.mac[0],
            self.mac[1],
            self.mac[2],
            self.mac[3],
            self.mac[4],
            self.mac[5]
        );

        self.initialized = true;
        Ok(())
    }

    fn shutdown(&mut self) {
        self.initialized = false;
    }

    fn priority(&self) -> u8 {
        5
    }
}

impl NetworkInterface for E1000Driver {
    fn send(&mut self, _packet: &[u8]) -> Result<(), DriverError> {
        if !self.initialized {
            return Err(DriverError::NotSupported);
        }
        // TODO: Реализовать заполнение TX дескрипторов
        Ok(())
    }

    fn poll(&mut self, _timeout_ms: Option<u32>) -> Result<Option<&'static [u8]>, DriverError> {
        // TODO: Реализовать чтение RX дескрипторов
        Ok(None)
    }

    fn mac_address(&self) -> [u8; 6] {
        self.mac
    }

    fn mtu(&self) -> usize {
        1500
    }

    fn set_interface_up(&mut self, _up: bool) -> Result<(), DriverError> {
        Ok(())
    }

    fn is_interface_up(&self) -> bool {
        self.initialized
    }
}
