//! Универсальные трейты для драйверов оборудования

pub mod e1000;
pub mod registry;
pub mod virtio_gpu;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverError {
    NotSupported,
    IoError,
    Timeout,
    InvalidParameter,
    OutOfMemory,
    DeviceNotFound,
}

pub trait Driver: Send + Sync {
    /// Возвращает имя драйвера (для логов и реестра)
    fn name(&self) -> &'static str;
    /// Инициализация оборудования. Вызывается один раз при загрузке драйвера.
    fn init(&mut self) -> Result<(), DriverError>;
    /// Остановка и освобождение ресурсов (для горячей замены)
    fn shutdown(&mut self);
    /// Уровень приоритета драйвера (для порядка инициализации)
    fn priority(&self) -> u8;
}

/// Трейт для блочных устройств (диски, флешки)
pub trait BlockDevice: Driver {
    /// Чтение секторов (размер сектора обычно 512 байт)
    fn read(&self, sector: u64, buf: &mut [u8]) -> Result<(), DriverError>;
    /// Запись секторов
    fn write(&self, sector: u64, buf: &[u8]) -> Result<(), DriverError>;
    /// Управляющие операции (ioctl)
    fn ioctl(&self, cmd: u32, arg: usize) -> Result<(), DriverError>;
    /// Размер устройства в секторах
    fn size_sectors(&self) -> u64;
}

/// Трейт для сетевых интерфейсов
pub trait NetworkInterface: Driver {
    /// Отправка пакета (Ethernet frame)
    fn send(&mut self, packet: &[u8]) -> Result<(), DriverError>;
    /// Получение пакета (блокирующий или non-blocking)
    fn poll(&mut self, timeout_ms: Option<u32>) -> Result<Option<&'static [u8]>, DriverError>;
    /// MAC-адрес интерфейса
    fn mac_address(&self) -> [u8; 6];
    /// Максимальный размер передачи (MTU)
    fn mtu(&self) -> usize;
    /// Включить/выключить интерфейс (например, для ifconfig up/down)
    fn set_interface_up(&mut self, up: bool) -> Result<(), DriverError>;
    /// Проверить, активен ли интерфейс
    fn is_interface_up(&self) -> bool;
}

/// Трейт для входных устройств (клавиатура, мышь, touchpad)
pub trait InputDevice: Driver {
    /// Опросить событие (например, KeyPress, MouseMove)
    /// Возвращает None, если событий нет (non-blocking)
    fn poll_event(&mut self) -> Result<Option<InputEvent>, DriverError>;
}

/// Событие от InputDevice
#[derive(Debug, Clone)]
pub enum InputEvent {
    Key {
        scancode: u32,
        pressed: bool,
    },
    Mouse {
        x: i32,
        y: i32,
        buttons: MouseButtons,
        wheel: i8,
    },
    // Можно расширить: Touch, Gamepad, etc.
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct MouseButtons: u8 {
        const LEFT = 0b001;
        const RIGHT = 0b010;
        const MIDDLE = 0b100;
    }
}

/// Трейт для графических устройств (GPU, простой framebuffer)
pub trait GraphicsDevice: Driver {
    /// Получить информацию о режиме (разрешение, bpp)
    fn mode(&self) -> GraphicsMode;
    /// Установить режим (изменение разрешения/глубины цвета)
    fn set_mode(&mut self, mode: GraphicsMode) -> Result<(), DriverError>;
    /// Получить прямой доступ к framebuffer (уже в нужном формате)
    fn framebuffer(&self) -> Option<&[u8]>;
    /// Выполнить двойную буферизацию (переключение буферов, если
    /// поддерживается)
    fn swap_buffers(&mut self) -> Result<(), DriverError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphicsMode {
    pub width: u32,
    pub height: u32,
    pub bits_per_pixel: u8,
    pub bytes_per_pixel: u8,
}
