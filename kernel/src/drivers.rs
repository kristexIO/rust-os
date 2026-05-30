use alloc::{
    boxed::Box,
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use spin::Mutex;

// Стандартные устройства
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeviceClass {
    Keyboard,
    Mouse,
    Storage,
    Network,
    Display,
    Audio,
    Usb,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeviceStatus {
    NotInitialized,
    Initialized,
    Working,
    Error,
    Disabled,
}

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub name: String,
    pub class: DeviceClass,
    pub vendor_id: u16,
    pub device_id: u16,
    pub status: DeviceStatus,
    pub description: String,
}

// Интерфейс для драйвера устройства
pub trait DeviceDriver: Send {
    fn initialize(&mut self) -> Result<(), &'static str>;
    fn start(&mut self) -> Result<(), &'static str>;
    fn stop(&mut self) -> Result<(), &'static str>;
    fn get_info(&self) -> DeviceInfo;
    fn handle_interrupt(&mut self) -> Result<(), &'static str>;
    fn read(&mut self, offset: usize, buffer: &mut [u8]) -> Result<usize, &'static str>;
    fn write(&mut self, offset: usize, buffer: &[u8]) -> Result<usize, &'static str>;
    fn ioctl(&mut self, command: u32, arg: usize) -> Result<usize, &'static str>;
}

// Менеджер устройств
pub struct DriverManager {
    pub devices: BTreeMap<String, Box<dyn DeviceDriver>>,
    pub registered_drivers: BTreeMap<String, fn() -> Box<dyn DeviceDriver>>,
    pub device_info: BTreeMap<String, DeviceInfo>,
}

impl DriverManager {
    pub fn new() -> Self {
        DriverManager {
            devices: BTreeMap::new(),
            registered_drivers: BTreeMap::new(),
            device_info: BTreeMap::new(),
        }
    }

    // Регистрация драйвера устройства
    pub fn register_driver(&mut self, name: &str, constructor: fn() -> Box<dyn DeviceDriver>) {
        self.registered_drivers
            .insert(name.to_string(), constructor);
    }

    // Поиск подходящего драйвера для устройства
    pub fn probe_device(&mut self, vendor_id: u16, device_id: u16) -> Option<String> {
        // В реальной системе тут будет сложная логика поиска подходящего драйвера
        // Здесь используем простую логику
        for (driver_name, _) in &self.registered_drivers {
            if driver_name.contains(&format!("{:X}", vendor_id))
                || driver_name.contains(&format!("{:X}", device_id))
            {
                return Some(driver_name.clone());
            }
        }
        None
    }

    // Загрузка и инициализация драйвера
    pub fn load_driver(&mut self, name: &str) -> Result<(), &'static str> {
        if let Some(constructor) = self.registered_drivers.get(name) {
            let mut driver = constructor();
            driver.initialize()?;

            let info = driver.get_info();
            let device_name = info.name.clone();

            self.devices.insert(device_name.clone(), driver);
            self.device_info.insert(device_name, info);

            Ok(())
        } else {
            Err("Driver not found")
        }
    }

    // Получение драйвера по имени
    pub fn get_driver(&mut self, name: &str) -> Option<&mut Box<dyn DeviceDriver>> {
        self.devices.get_mut(name)
    }

    // Запуск всех драйверов
    pub fn start_all(&mut self) -> Result<(), &'static str> {
        for (_, driver) in self.devices.iter_mut() {
            if let Err(e) = driver.start() {
                // Не прерываем работу, просто логируем ошибку
                // В реальной системе логировали бы ошибку
            }
        }
        Ok(())
    }

    // Остановка всех драйверов
    pub fn stop_all(&mut self) -> Result<(), &'static str> {
        for (_, driver) in self.devices.iter_mut() {
            if let Err(e) = driver.stop() {
                // Не прерываем работу, просто логируем ошибку
            }
        }
        Ok(())
    }

    // Получение списка устройств
    pub fn list_devices(&self) -> Vec<DeviceInfo> {
        self.device_info.values().cloned().collect()
    }

    // Обработка прерываний от устройств
    pub fn handle_device_interrupts(&mut self) -> Result<(), &'static str> {
        for (name, driver) in self.devices.iter_mut() {
            if let Err(e) = driver.handle_interrupt() {
                // В реальной системе логировали бы ошибку
            }
        }
        Ok(())
    }

    // Получение информации об устройстве
    pub fn get_device_info(&self, name: &str) -> Option<&DeviceInfo> {
        self.device_info.get(name)
    }
}

// Пример конкретного драйвера - драйвер мыши
pub struct MouseDriver {
    pub info: DeviceInfo,
    pub x: i32,
    pub y: i32,
    pub buttons: u8,
}

impl MouseDriver {
    pub fn new() -> Self {
        MouseDriver {
            info: DeviceInfo {
                name: "PS2_Mouse".to_string(),
                class: DeviceClass::Mouse,
                vendor_id: 0x0000, // Generic PS/2
                device_id: 0x0001,
                status: DeviceStatus::NotInitialized,
                description: "PS/2 Mouse Driver".to_string(),
            },
            x: 0,
            y: 0,
            buttons: 0,
        }
    }

    pub fn handle_mouse_packet(&mut self, data: [u8; 3]) {
        // Обработка пакета мыши
        let x_movement = data[1] as i8 as i32;
        let y_movement = data[2] as i8 as i32;
        let buttons = data[0] & 0x07; // Только правые 3 бита

        self.x += x_movement * 2; // Умножение на чувствительность
        self.y -= y_movement * 2; // Инвертируем Y для правильного направления

        self.buttons = buttons;
    }
}

impl DeviceDriver for MouseDriver {
    fn initialize(&mut self) -> Result<(), &'static str> {
        // Инициализация PS/2 мыши
        // В реальной системе: настройка контроллера PS/2, включение устройства

        // Эмуляция инициализации
        self.info.status = DeviceStatus::Initialized;
        Ok(())
    }

    fn start(&mut self) -> Result<(), &'static str> {
        // Запуск мыши
        self.info.status = DeviceStatus::Working;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), &'static str> {
        // Остановка мыши
        self.info.status = DeviceStatus::Disabled;
        Ok(())
    }

    fn get_info(&self) -> DeviceInfo {
        self.info.clone()
    }

    fn handle_interrupt(&mut self) -> Result<(), &'static str> {
        // Обработка прерывания мыши
        // В реальной системе: чтение пакета мыши из порта 0x60

        // Для эмуляции просто возвращаем OK
        Ok(())
    }

    fn read(&mut self, _offset: usize, buffer: &mut [u8]) -> Result<usize, &'static str> {
        if buffer.len() < 6 {
            return Err("Buffer too small");
        }

        // Возвращаем позицию мыши и состояние кнопок
        let x_bytes = self.x.to_le_bytes();
        let y_bytes = self.y.to_le_bytes();

        buffer[0] = x_bytes[0]; // LSB X
        buffer[1] = x_bytes[1]; // MSB X
        buffer[2] = y_bytes[0]; // LSB Y
        buffer[3] = y_bytes[1]; // MSB Y
        buffer[4] = self.buttons;
        buffer[5] = 0; // Reserved

        Ok(6)
    }

    fn write(&mut self, _offset: usize, _buffer: &[u8]) -> Result<usize, &'static str> {
        // Мы не поддерживаем запись в драйвер мыши
        Err("Write not supported")
    }

    fn ioctl(&mut self, command: u32, arg: usize) -> Result<usize, &'static str> {
        match command {
            0x1001 => {
                // SET_SENSITIVITY
                let sensitivity = arg as u8;
                Ok(sensitivity as usize)
            }
            0x1002 => {
                // GET_POSITION
                Ok((self.x as usize) << 16 | self.y as usize)
            }
            _ => Err("Unknown command"),
        }
    }
}

// Пример драйвера клавиатуры
pub struct KeyboardDriver {
    pub info: DeviceInfo,
    pub led_state: u8,
}

impl KeyboardDriver {
    pub fn new() -> Self {
        KeyboardDriver {
            info: DeviceInfo {
                name: "PS2_Keyboard".to_string(),
                class: DeviceClass::Keyboard,
                vendor_id: 0x0000, // Generic PS/2
                device_id: 0x0002,
                status: DeviceStatus::NotInitialized,
                description: "PS/2 Keyboard Driver".to_string(),
            },
            led_state: 0,
        }
    }

    pub fn set_leds(&mut self, scroll_lock: bool, num_lock: bool, caps_lock: bool) {
        let mut led_byte = 0;
        if scroll_lock {
            led_byte |= 1;
        }
        if num_lock {
            led_byte |= 2;
        }
        if caps_lock {
            led_byte |= 4;
        }

        self.led_state = led_byte;
        // В реальной системе: отправка команды 0xED с led_byte в порт 0x60
    }
}

impl DeviceDriver for KeyboardDriver {
    fn initialize(&mut self) -> Result<(), &'static str> {
        // Инициализация PS/2 клавиатуры
        // В реальной системе: настройка контроллера PS/2, сброс, самотестирование

        self.info.status = DeviceStatus::Initialized;
        Ok(())
    }

    fn start(&mut self) -> Result<(), &'static str> {
        // Запуск клавиатуры
        self.info.status = DeviceStatus::Working;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), &'static str> {
        // Остановка клавиатуры
        self.info.status = DeviceStatus::Disabled;
        Ok(())
    }

    fn get_info(&self) -> DeviceInfo {
        self.info.clone()
    }

    fn handle_interrupt(&mut self) -> Result<(), &'static str> {
        // Обработка прерывания клавиатуры
        // В реальной системе: чтение скэнкода из порта 0x60
        Ok(())
    }

    fn read(&mut self, _offset: usize, _buffer: &mut [u8]) -> Result<usize, &'static str> {
        // Клавиатура обычно не поддерживает чтение, только прерывания
        Err("Read not supported, use interrupt-based input")
    }

    fn write(&mut self, _offset: usize, buffer: &[u8]) -> Result<usize, &'static str> {
        // Запись команд в клавиатуру
        if buffer.is_empty() {
            return Err("Empty command");
        }

        match buffer[0] {
            0xED => {
                // Set LEDs command
                if buffer.len() < 2 {
                    return Err("LED command requires argument");
                }

                self.led_state = buffer[1];
                // В реальной системе: отправка команды в порт 0x60
                Ok(1)
            }
            0xF4 => {
                // Enable scanning
                Ok(0)
            }
            0xF5 => {
                // Disable scanning
                Ok(0)
            }
            _ => Err("Unsupported command"),
        }
    }

    fn ioctl(&mut self, command: u32, arg: usize) -> Result<usize, &'static str> {
        match command {
            0x2001 => {
                // SET_LEDS
                self.led_state = arg as u8;
                Ok(0)
            }
            0x2002 => {
                // GET_LEDS
                Ok(self.led_state as usize)
            }
            _ => Err("Unknown command"),
        }
    }
}

// Глобальный менеджер устройств
use spin::Once;

static DRIVER_MANAGER: Once<Mutex<DriverManager>> = Once::new();

pub fn get_driver_manager() -> &'static Mutex<DriverManager> {
    DRIVER_MANAGER.call_once(|| Mutex::new(DriverManager::new()))
}

// Функции для удобства
pub fn init_drivers() -> Result<(), &'static str> {
    let mut dm = get_driver_manager().lock();

    // Регистрируем системные драйверы
    dm.register_driver("PS2_Mouse", || Box::new(MouseDriver::new()));
    dm.register_driver("PS2_Keyboard", || Box::new(KeyboardDriver::new()));

    // Загружаем драйверы
    dm.load_driver("PS2_Mouse")?;
    dm.load_driver("PS2_Keyboard")?;

    dm.start_all()
}

pub fn list_devices() -> Vec<DeviceInfo> {
    let dm = get_driver_manager().lock();
    dm.list_devices()
}

pub fn get_device_info(device_name: &str) -> Option<DeviceInfo> {
    let dm = get_driver_manager().lock();
    dm.get_device_info(device_name).cloned()
}

pub fn handle_device_interrupts() -> Result<(), &'static str> {
    let mut dm = get_driver_manager().lock();
    dm.handle_device_interrupts()
}
