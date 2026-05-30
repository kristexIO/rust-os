use alloc::vec::Vec;

use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::instructions::port::Port;

use crate::driver::{Driver, DriverError, NetworkInterface};

// Смещения регистров RTL8139
const REG_MAC: u16 = 0x00;
const REG_TXST0: u16 = 0x10;
const REG_TXADDR0: u16 = 0x20;
const REG_RXSTART: u16 = 0x30;
const REG_COMMAND: u16 = 0x37;
const REG_BPC: u16 = 0x38;
const REG_IMR: u16 = 0x3C;
const REG_ISR: u16 = 0x3E;
const REG_TCR: u16 = 0x40;
const REG_RCR: u16 = 0x44;
const REG_CONFIG1: u16 = 0x52;

const RX_BUF_SIZE: usize = 8192 + 16 + 1500;
static mut RX_BUFFER: [u8; RX_BUF_SIZE] = [0; RX_BUF_SIZE];
static mut TX_BUFFERS: [[u8; 2048]; 4] = [[0; 2048]; 4];

/// Внутреннее состояние драйвера
struct Rtl8139Inner {
    io_base: u16,
    mac: [u8; 6],
    tx_index: usize,
    rx_current: usize,
    initialized: bool,
    /// Последний принятый пакет (offset, len). Используется для poll().
    last_packet: Option<(usize, usize)>,
}

/// Драйвер RTL8139, реализующий трейты
pub struct Rtl8139Driver {
    inner: Mutex<Rtl8139Inner>,
}

impl Rtl8139Driver {
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(Rtl8139Inner {
                io_base: 0,
                mac: [0; 6],
                tx_index: 0,
                rx_current: 0,
                initialized: false,
                last_packet: None,
            }),
        }
    }
}

impl Driver for Rtl8139Driver {
    fn name(&self) -> &'static str {
        "net:rtl8139"
    }

    fn init(&mut self) -> Result<(), DriverError> {
        // Поиск I/O базы через PCI
        let mut io_base = 0;
        for slot in 0..32 {
            let vendor = crate::pci::get_vendor_id(0, slot, 0);
            let device = crate::pci::get_device_id(0, slot, 0);
            if vendor == 0x10EC && device == 0x8139 {
                let bar0 = crate::pci::read_config_dword(0, slot, 0, 0x10);
                if bar0 & 1 == 1 {
                    io_base = (bar0 & !3) as u16;
                }
                break;
            }
        }

        if io_base == 0 {
            return Err(DriverError::DeviceNotFound);
        }

        let mut inner = self.inner.lock();
        inner.io_base = io_base;

        unsafe {
            // Сброс
            Port::<u8>::new(io_base + REG_CONFIG1).write(0x00);
            Port::<u8>::new(io_base + REG_COMMAND).write(0x10);
            while Port::<u8>::new(io_base + REG_COMMAND).read() & 0x10 != 0 {}
            // RX buffer
            let rx_phys = core::ptr::addr_of!(RX_BUFFER) as u32;
            Port::<u32>::new(io_base + REG_RXSTART).write(rx_phys);
            // IMR
            Port::<u16>::new(io_base + REG_IMR).write(0x0005);
            // RCR
            Port::<u32>::new(io_base + REG_RCR).write(0x0F | (1 << 7));
            // Command
            Port::<u8>::new(io_base + REG_COMMAND).write(0x0C);
            // Читаем MAC
            for i in 0..6 {
                inner.mac[i] = Port::<u8>::new(io_base + REG_MAC + i as u16).read();
            }
        }

        inner.initialized = true;
        log::info!("RTL8139 initialized at I/O 0x{:X}", io_base);
        Ok(())
    }

    fn shutdown(&mut self) {
        let mut inner = self.inner.lock();
        if inner.initialized {
            unsafe {
                Port::<u8>::new(inner.io_base + REG_COMMAND).write(0x00);
            }
        }
        inner.initialized = false;
        log::info!("RTL8139 shut down");
    }

    fn priority(&self) -> u8 {
        100
    }
}

impl NetworkInterface for Rtl8139Driver {
    fn send(&mut self, packet: &[u8]) -> Result<(), DriverError> {
        let mut inner = self.inner.lock();
        if !inner.initialized {
            return Err(DriverError::NotSupported);
        }
        if packet.len() > 2048 {
            return Err(DriverError::InvalidParameter);
        }

        let idx = inner.tx_index;
        unsafe {
            let ptr = TX_BUFFERS[idx].as_mut_ptr();
            core::ptr::copy_nonoverlapping(packet.as_ptr(), ptr, packet.len());
            let phys_addr = ptr as u32;
            let io_base = inner.io_base;
            Port::<u32>::new(io_base + REG_TXADDR0 + (idx * 4) as u16).write(phys_addr);
            Port::<u32>::new(io_base + REG_TXST0 + (idx * 4) as u16).write(packet.len() as u32);
        }
        inner.tx_index = (inner.tx_index + 1) % 4;
        Ok(())
    }

    fn poll(&mut self, _timeout_ms: Option<u32>) -> Result<Option<&'static [u8]>, DriverError> {
        let mut inner = self.inner.lock();
        if let Some((offset, len)) = inner.last_packet.take() {
            // Возвращаем пакет. Внимание: lifetime 'static, но буфер статический.
            unsafe {
                let ptr = (core::ptr::addr_of!(RX_BUFFER) as *const u8).add(offset);
                let slice = core::slice::from_raw_parts(ptr, len);
                return Ok(Some(slice));
            }
        }
        Ok(None)
    }

    fn mac_address(&self) -> [u8; 6] {
        self.inner.lock().mac
    }

    fn mtu(&self) -> usize {
        1500
    }

    fn set_interface_up(&mut self, up: bool) -> Result<(), DriverError> {
        let inner = self.inner.lock();
        if !inner.initialized {
            return Err(DriverError::NotSupported);
        }
        unsafe {
            let cmd = Port::<u8>::new(inner.io_base + REG_COMMAND).read();
            if up {
                Port::<u8>::new(inner.io_base + REG_COMMAND).write(cmd | 0x0C);
            } else {
                Port::<u8>::new(inner.io_base + REG_COMMAND).write(cmd & !0x0C);
            }
        }
        Ok(())
    }

    fn is_interface_up(&self) -> bool {
        self.inner.lock().initialized
    }
}

/// Обработчик прерывания от RTL8139
pub fn handle_interrupt() {
    let packets = {
        let mut dev = DRIVER.lock();
        if !dev.inner.lock().initialized {
            return;
        }
        let io_base = dev.inner.lock().io_base;
        unsafe {
            let status = Port::<u16>::new(io_base + REG_ISR).read();
            let packets = if status & 0x01 != 0 {
                receive_packets(&mut dev)
            } else {
                Vec::new()
            };
            Port::<u16>::new(io_base + REG_ISR).write(status);
            packets
        }
    };

    // Packet handlers may transmit replies, so do not keep the driver lock here.
    for packet in packets {
        crate::net::ethernet::handle_packet(&packet);
    }
}

unsafe fn receive_packets(driver: &mut Rtl8139Driver) -> Vec<Vec<u8>> {
    let mut inner = driver.inner.lock();
    let io_base = inner.io_base;
    let mut packets = Vec::new();

    while Port::<u8>::new(io_base + REG_COMMAND).read() & 0x01 == 0 {
        let rx_offset = inner.rx_current;
        let buf_ptr = (core::ptr::addr_of!(RX_BUFFER) as *const u16).add(rx_offset / 2);
        let header = buf_ptr.read_unaligned();
        let length = buf_ptr.add(1).read_unaligned() as usize;

        if header & 0x01 == 0 {
            break;
        }
        if length == 0 {
            inner.rx_current = 0;
            Port::<u16>::new(io_base + REG_BPC).write(0);
            break;
        }

        let packet_len = length.saturating_sub(4);
        let data_ptr = (core::ptr::addr_of!(RX_BUFFER) as *const u8).add(rx_offset + 4);

        if rx_offset + 4 + packet_len <= RX_BUF_SIZE {
            packets.push(core::slice::from_raw_parts(data_ptr, packet_len).to_vec());
            inner.last_packet = Some((rx_offset + 4, packet_len));
        }

        inner.rx_current = (inner.rx_current + length + 4 + 3) & !3;
        if inner.rx_current >= 8192 {
            inner.rx_current -= 8192;
        }
        let cpr = (inner.rx_current as u16).wrapping_sub(16);
        Port::<u16>::new(io_base + REG_BPC).write(cpr);
    }

    packets
}

// Глобальный экземпляр драйвера
lazy_static! {
    pub static ref DRIVER: Mutex<Rtl8139Driver> = Mutex::new(Rtl8139Driver::new());
}

// Обработчики для совместимости
pub fn init() {
    DRIVER.lock().init().ok();
}
pub fn send_packet(data: &[u8]) {
    DRIVER.lock().send(data).ok();
}

pub fn get_mac() -> [u8; 6] {
    DRIVER.lock().mac_address()
}
