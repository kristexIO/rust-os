use alloc::{collections::BTreeMap, sync::Arc, vec::Vec};

use lazy_static::lazy_static;
use spin::{Mutex, RwLock};

use super::*;

/// Глобальный реестр драйверов. Доступен из любого места через lazy_static.
pub struct DriverRegistry {
    /// Все загруженные драйверы (по имени)
    drivers: RwLock<BTreeMap<&'static str, Arc<Mutex<dyn Driver>>>>,
    /// Специализированные списки для быстрого доступа
    block_devices: RwLock<Vec<Arc<Mutex<dyn BlockDevice>>>>,
    net_interfaces: RwLock<Vec<Arc<Mutex<dyn NetworkInterface>>>>,
    input_devices: RwLock<Vec<Arc<Mutex<dyn InputDevice>>>>,
    graphics_devices: RwLock<Vec<Arc<Mutex<dyn GraphicsDevice>>>>,
}

impl DriverRegistry {
    pub const fn new() -> Self {
        DriverRegistry {
            drivers: RwLock::new(BTreeMap::new()),
            block_devices: RwLock::new(Vec::new()),
            net_interfaces: RwLock::new(Vec::new()),
            input_devices: RwLock::new(Vec::new()),
            graphics_devices: RwLock::new(Vec::new()),
        }
    }

    /// Регистрация драйвера. Если драйвер с таким именем уже есть,
    /// перезаписывает (hot-swap).
    pub fn register(&self, driver: Arc<Mutex<dyn Driver>>) -> Result<(), DriverError> {
        let name = driver.lock().name();
        // Удаляем старый драйвер с таким именем, если был
        if self.drivers.read().contains_key(name) {
            let _ = self.unregister(name);
        }
        self.drivers.write().insert(name, driver.clone());

        // Добавляем в специализированный список по типу
        // MVP: Упрощаем регистрацию без downcast, так как он требует Any/'static
        // и сложен в no_std. В будущем можно добавить типизированные контейнеры.
        // let name_str = driver.lock().name();

        // Инициализируем драйвер
        driver.lock().init()?;
        log::info!("Driver registered: {}", name);
        Ok(())
    }

    /// Удаление драйвера (hot-unload)
    pub fn unregister(&self, name: &str) -> Result<(), DriverError> {
        if let Some(driver) = self.drivers.write().remove(name) {
            driver.lock().shutdown();
            // Удаляем из специализированных списков (пересобираем вектора)
            self.block_devices
                .write()
                .retain(|d| d.lock().name() != name);
            self.net_interfaces
                .write()
                .retain(|d| d.lock().name() != name);
            self.input_devices
                .write()
                .retain(|d| d.lock().name() != name);
            self.graphics_devices
                .write()
                .retain(|d| d.lock().name() != name);
            log::info!("Driver unregistered: {}", name);
            Ok(())
        } else {
            Err(DriverError::DeviceNotFound)
        }
    }

    /// Получить драйвер по имени
    pub fn get(&self, name: &str) -> Option<Arc<Mutex<dyn Driver>>> {
        self.drivers.read().get(name).cloned()
    }

    /// Перечислить все зарегистрированные имена
    pub fn list_drivers(&self) -> Vec<&'static str> {
        self.drivers.read().keys().copied().collect()
    }

    /// Найти драйвер блочного устройства по индексу (0-based)
    pub fn block_device(&self, idx: usize) -> Option<Arc<Mutex<dyn BlockDevice>>> {
        self.block_devices.read().get(idx).cloned()
    }

    /// Найти сетевой интерфейс по индексу
    pub fn network_interface(&self, idx: usize) -> Option<Arc<Mutex<dyn NetworkInterface>>> {
        self.net_interfaces.read().get(idx).cloned()
    }

    /// Найти устройство ввода по индексу
    pub fn input_device(&self, idx: usize) -> Option<Arc<Mutex<dyn InputDevice>>> {
        self.input_devices.read().get(idx).cloned()
    }

    /// Найти графическое устройство по индексу
    pub fn graphics_device(&self, idx: usize) -> Option<Arc<Mutex<dyn GraphicsDevice>>> {
        self.graphics_devices.read().get(idx).cloned()
    }

    /// Количество устройств каждого типа
    pub fn num_block(&self) -> usize {
        self.block_devices.read().len()
    }
    pub fn num_network(&self) -> usize {
        self.net_interfaces.read().len()
    }
    pub fn num_input(&self) -> usize {
        self.input_devices.read().len()
    }
    pub fn num_graphics(&self) -> usize {
        self.graphics_devices.read().len()
    }
}

lazy_static! {
    pub static ref REGISTRY: DriverRegistry = DriverRegistry::new();
}
