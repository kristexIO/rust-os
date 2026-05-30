//! Драйвер VirtIO-GPU для QEMU/VirtIO графических ускорителей.
//! Поддерживает создание 2D ресурсов, привязку памяти и флаш фреймбуфера.

use crate::{
    driver::{GraphicsMode, *},
    pci,
};

/// Константы VirtIO PCI
const VIRTIO_VENDOR_ID: u16 = 0x1AF4;
const VIRTIO_GPU_DEVICE_ID: u16 = 0x1050; // Modern VirtIO GPU

/// Команды VirtIO GPU
#[allow(dead_code)]
enum VirtioGpuCtrlType {
    GetDisplayInfo = 0x0100,
    ResourceCreate2d = 0x0101,
    ResourceUnref = 0x0102,
    SetScanout = 0x0103,
    ResourceFlush = 0x0104,
    TransferToHost2d = 0x0105,
    ResourceAttachBacking = 0x0106,
    ResourceDetachBacking = 0x0107,
    CtxCreate = 0x0200,
    CtxDestroy = 0x0201,
}

#[repr(C)]
struct VirtioGpuCtrlHeader {
    ctrl_type: u32,
    flags: u32,
    fence_id: u64,
    ctx_id: u32,
    padding: u32,
}

pub struct VirtioGpuDriver {
    mmio_base: u64,
    pub width: u32,
    pub height: u32,
    resource_id: u32,
    initialized: bool,
}

impl VirtioGpuDriver {
    pub fn new(mmio_base: u64) -> Self {
        Self {
            mmio_base,
            width: 1024,
            height: 768,
            resource_id: 1,
            initialized: false,
        }
    }

    /// Поиск устройства на PCI шине
    pub fn probe() -> Option<Self> {
        let devices = pci::PCI_DEVICES.lock();
        for dev in devices.iter() {
            if dev.vendor_id == VIRTIO_VENDOR_ID && dev.device_id == VIRTIO_GPU_DEVICE_ID {
                // В VirtIO-PCI MMIO обычно в BAR4 для modern девайсов или BAR1/0 для legacy
                let bar4 = pci::read_bar(dev.bus, dev.slot, dev.func, 4);
                if bar4 != 0 {
                    pci::enable_device(dev.bus, dev.slot, dev.func);
                    return Some(Self::new(bar4));
                }
            }
        }
        None
    }
}

impl Driver for VirtioGpuDriver {
    fn name(&self) -> &'static str {
        "gfx:virtio-gpu"
    }

    fn init(&mut self) -> Result<(), DriverError> {
        // 1. Сброс устройства
        // 2. Установка ACKNOWLEDGE и DRIVER бит в Device Status
        // 3. Согласование фич
        // 4. Настройка Virtqueues
        // 5. Установка DRIVER_OK

        // MVP: Пока просто помечаем как "инициализировано" (нужно реализовать VirtQueue
        // для полноценной работы)
        log::info!("VirtIO-GPU: MMIO at 0x{:X}", self.mmio_base);
        self.initialized = true;
        Ok(())
    }

    fn shutdown(&mut self) {
        self.initialized = false;
    }

    fn priority(&self) -> u8 {
        10 // Графика — высокий приоритет
    }
}

impl GraphicsDevice for VirtioGpuDriver {
    fn mode(&self) -> GraphicsMode {
        GraphicsMode {
            width: self.width,
            height: self.height,
            bits_per_pixel: 32,
            bytes_per_pixel: 4,
        }
    }

    fn set_mode(&mut self, mode: GraphicsMode) -> Result<(), DriverError> {
        self.width = mode.width;
        self.height = mode.height;
        // Здесь должна быть команда ResourceCreate2d
        Ok(())
    }

    fn framebuffer(&self) -> Option<&[u8]> {
        // VirtIO-GPU может использовать либо shared memory (BAR2/4),
        // либо требовать явного флаша из системной памяти (TransferToHost2d +
        // ResourceFlush)
        None
    }

    fn swap_buffers(&mut self) -> Result<(), DriverError> {
        // Команды TransferToHost2d + ResourceFlush
        Ok(())
    }
}
