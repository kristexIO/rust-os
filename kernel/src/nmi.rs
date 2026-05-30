use spin::Mutex;
use x86_64::registers::model_specific::Msr;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};

/// Non-Maskable Interrupt (NMI) - прерывание, которое нельзя заглушить
///
/// Используется для критических ситуаций: аппаратные ошибки, watchdog таймеры,
/// критические системные события и т.д.
pub struct NmiManager {
    /// Обработчик NMI
    handler: Option<fn(InterruptStackFrame)>,
    /// Статистика NMI
    stats: NmiStats,
    /// Флаг, что NMI включены
    enabled: bool,
}

/// Статистика NMI
#[derive(Debug, Clone)]
pub struct NmiStats {
    /// Общее количество NMI
    pub total_count: u64,
    /// Количество обработанных NMI
    pub handled_count: u64,
    /// Количество пропущенных NMI
    pub missed_count: u64,
    /// Время последнего NMI (в миллисекундах с момента загрузки)
    pub last_nmi_time: u64,
    /// Типы последних NMI (для диагностики)
    pub last_nmi_types: [NmiType; 10],
}

/// Типы NMI
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NmiType {
    /// Аппаратная ошибка (например, parity error)
    HardwareError,
    /// Watchdog таймер
    WatchdogTimeout,
    /// Критическая ошибка памяти
    MemoryError,
    /// Температурная ошибка
    ThermalTrip,
    /// Кастомное NMI
    Custom,
    /// Неизвестный тип
    Unknown,
}

impl NmiManager {
    /// Создает новый менеджер NMI
    pub fn new() -> Self {
        NmiManager {
            handler: None,
            stats: NmiStats {
                total_count: 0,
                handled_count: 0,
                missed_count: 0,
                last_nmi_time: 0,
                last_nmi_types: [NmiType::Unknown; 10],
            },
            enabled: false,
        }
    }

    /// Инициализирует NMI
    pub fn init(&mut self) -> Result<(), &'static str> {
        // Устанавливаем IDT entry для NMI (interrupt vector 2)
        // NMI использует вектор 2 в IDT
        let mut idt = crate::interrupts::IDT.lock();

        // NMI обработчик (вектор 2 зарезервирован для двойных ошибок, но для NMI используем отдельный вектор)
        // В реальной системе NMI обычно соответствует вектору 0x02, но мы будем использовать 0xFF для демонстрации
        idt[0xFF].set_handler_fn(nmi_handler_wrapper);

        self.enabled = true;
        Ok(())
    }

    /// Устанавливает пользовательский обработчик NMI
    pub fn set_handler(&mut self, handler: fn(InterruptStackFrame)) {
        self.handler = Some(handler);
    }

    /// Включает NMI
    pub fn enable(&mut self) {
        self.enabled = true;
        // Включаем NMI через APIC или другие механизмы
        self.enable_hardware_nmi();
    }

    /// Выключает NMI
    pub fn disable(&mut self) {
        self.enabled = false;
        // Выключаем NMI через APIC или другие механизмы
        self.disable_hardware_nmi();
    }

    /// Включает аппаратные NMI
    fn enable_hardware_nmi(&self) {
        // В реальной системе:
        // 1. Для локального APIC: устанавливаем LVT (Local Vector Table) регистр
        // 2. Для внешних NMI: настраиваем NMI линию

        // Включение NMI через MSR IA32_APIC_BASE
        let mut msr = Msr::new(0x1B); // IA32_APIC_BASE
        unsafe {
            let base = msr.read();
            // Устанавливаем бит 11 для включения APIC
            msr.write(base | (1 << 11));

            // Устанавливаем LVT NMI регистр (offset 0x340)
            let lvt_nmi_addr = (base & 0xFFFFF000) + 0x340;
            // Записываем вектор NMI
            (lvt_nmi_addr as *mut u32).write_volatile(0xFF | (1 << 16)); // Vector 0xFF, masked=0
        }
    }

    /// Выключает аппаратные NMI
    fn disable_hardware_nmi(&self) {
        // Выключение NMI через APIC
        let mut msr = Msr::new(0x1B); // IA32_APIC_BASE
        unsafe {
            let base = msr.read();
            let lvt_nmi_addr = (base & 0xFFFFF000) + 0x340;
            // Маскируем NMI
            (lvt_nmi_addr as *mut u32).write_volatile(0xFF | (1 << 16) | (1 << 15));
            // Vector 0xFF, masked=1, delivery=1
        }
    }

    /// Вызывает NMI программно (для тестирования)
    pub fn trigger_software_nmi(&self) {
        if self.enabled {
            unsafe {
                // Программное вызывание NMI через APIC
                let mut msr = Msr::new(0x830); // ICRC (Interrupt Command Register - Cluster)

                // Устанавливаем биты для NMI IPI
                msr.write(0x400 | 0xFF); // Delivery Mode: NMI (100), Vector: 0xFF
            }
        }
    }

    /// Обрабатывает пришедшее NMI
    pub fn handle_nmi(&mut self, frame: InterruptStackFrame, nmi_type: NmiType) {
        // Обновляем статистику
        self.stats.total_count += 1;

        // Сохраняем тип последнего NMI
        self.stats.last_nmi_types.rotate_right(1);
        self.stats.last_nmi_types[0] = nmi_type;

        // Вызываем пользовательский обработчик, если он установлен
        if let Some(handler) = self.handler {
            handler(frame);
            self.stats.handled_count += 1;
        } else {
            // Если обработчик не установлен, просто логгируем
            self.stats.missed_count += 1;
        }

        // Обновляем время последнего NMI
        self.stats.last_nmi_time = crate::timer::get_uptime_ms();
    }

    /// Получает статистику NMI
    pub fn get_stats(&self) -> &NmiStats {
        &self.stats
    }

    /// Сбрасывает статистику NMI
    pub fn reset_stats(&mut self) {
        self.stats = NmiStats {
            total_count: 0,
            handled_count: 0,
            missed_count: 0,
            last_nmi_time: crate::timer::get_uptime_ms(),
            last_nmi_types: [NmiType::Unknown; 10],
        };
    }

    /// Проверяет, включены ли NMI
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Получает тип последнего NMI
    pub fn get_last_nmi_type(&self) -> NmiType {
        self.stats.last_nmi_types[0]
    }
}

/// Глобальный NMI менеджер
use spin::Once;

static NMI_MANAGER: Once<Mutex<NmiManager>> = Once::new();

pub fn get_nmi_manager() -> &'static Mutex<NmiManager> {
    NMI_MANAGER.call_once(|| Mutex::new(NmiManager::new()))
}

/// Обработчик NMI (внешняя функция для IDT)
extern "x86-interrupt" fn nmi_handler_wrapper(frame: InterruptStackFrame) {
    let nmi_type = determine_nmi_type(&frame);

    let mut nmi_manager = get_nmi_manager().lock();
    nmi_manager.handle_nmi(frame, nmi_type);
}

/// Определяет тип NMI на основе стека и состояния системы
fn determine_nmi_type(_frame: &InterruptStackFrame) -> NmiType {
    // В реальной системе тут анализируется:
    // - Содержимое регистров
    // - Состояние процессора
    // - Содержимое APIC регистров
    // - События от системных датчиков

    // Пока возвращаем кастомный тип
    NmiType::Custom
}

/// Функции для удобства использования NMI
pub fn init_nmi_system() -> Result<(), &'static str> {
    let mut nmi_manager = get_nmi_manager().lock();
    nmi_manager.init()
}

pub fn enable_nmi() {
    let mut nmi_manager = get_nmi_manager().lock();
    nmi_manager.enable();
}

pub fn disable_nmi() {
    let mut nmi_manager = get_nmi_manager().lock();
    nmi_manager.disable();
}

pub fn set_nmi_handler(handler: fn(InterruptStackFrame)) {
    let mut nmi_manager = get_nmi_manager().lock();
    nmi_manager.set_handler(handler);
}

pub fn trigger_nmi() {
    let nmi_manager = get_nmi_manager().lock();
    nmi_manager.trigger_software_nmi();
}

pub fn get_nmi_stats() -> NmiStats {
    let nmi_manager = get_nmi_manager().lock();
    nmi_manager.get_stats().clone()
}

pub fn reset_nmi_stats() {
    let mut nmi_manager = get_nmi_manager().lock();
    nmi_manager.reset_stats();
}

pub fn is_nmi_enabled() -> bool {
    let nmi_manager = get_nmi_manager().lock();
    nmi_manager.is_enabled()
}

/// Реализация безопасных NMI прерываний
pub struct SafeNmiHandler {
    /// Флаг, что NMI обрабатывается
    is_handling: AtomicBool,
    /// Количество вложенных вызовов
    nesting_level: AtomicUsize,
    /// Максимальный уровень вложенности
    max_nesting: usize,
}

impl SafeNmiHandler {
    pub fn new(max_nesting: usize) -> Self {
        SafeNmiHandler {
            is_handling: AtomicBool::new(false),
            nesting_level: AtomicUsize::new(0),
            max_nesting,
        }
    }

    /// Безопасно вызывает обработчик NMI
    pub fn safe_handle_nmi(&self, frame: InterruptStackFrame, handler: fn(InterruptStackFrame)) {
        // Проверяем вложенность
        let current_nesting = self.nesting_level.fetch_add(1, Ordering::Acquire);

        if current_nesting >= self.max_nesting {
            // Слишком глубокая вложенность - игнорируем
            self.nesting_level.fetch_sub(1, Ordering::Release);
            return;
        }

        // Проверяем, что NMI не обрабатывается в другом потоке
        if self
            .is_handling
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            // Устанавливаем флаг обработки
            handler(frame);

            // Снимаем флаг обработки
            self.is_handling.store(false, Ordering::Release);
        } else {
            // NMI уже обрабатывается, увеличиваем счетчик пропущенных
        }

        // Уменьшаем уровень вложенности
        self.nesting_level.fetch_sub(1, Ordering::Release);
    }
}

/// Интерфейс для системных компонентов
pub struct NmiSystemInterface;

impl NmiSystemInterface {
    /// Инициализирует систему NMI
    pub fn init_system_nmi_handlers() {
        // Устанавливаем стандартный обработчик для критических ситуаций
        set_nmi_handler(Self::default_nmi_handler);
    }

    /// Стандартный обработчик NMI
    extern "x86-interrupt" fn default_nmi_handler(frame: InterruptStackFrame) {
        // Сохраняем критическую информацию для диагностики
        let error_info = SystemErrorInfo {
            rip: frame.instruction_pointer.as_u64(),
            cs: frame.code_segment,
            rflags: frame.flags,
            timestamp: crate::timer::get_uptime_ms(),
        };

        // Логгируем ошибку (в безопасном режиме)
        crate::logging::log_fatal(&format!(
            "NMI occurred at RIP: 0x{:X}, Error info: {:?}",
            error_info.rip, error_info
        ));

        // В зависимости от типа ошибки принимаем меры:
        // - Если аппаратная ошибка - возможно, безопасное завершение
        // - Если watchdog - перезапуск соответствующего компонента
        // - Если другая ошибка - попытка восстановления системы

        // В реальной системе тут может быть:
        // - Сохранение дампа памяти
        // - Попытка восстановления
        // - Безопасное завершение системы
    }

    /// Устанавливает специализированный обработчик для температурных ошибок
    pub fn set_thermal_nmi_handler() {
        set_nmi_handler(Self::thermal_nmi_handler);
    }

    extern "x86-interrupt" fn thermal_nmi_handler(_frame: InterruptStackFrame) {
        // Обработка температурной ошибки
        // - Логгирование температуры
        // - Попытка снижения частоты CPU или включения кулеров
        // - Ограничение активности системы

        crate::logging::log_warn("Thermal NMI: Critical temperature reached!");

        // В реальной системе тут будет:
        // - Чтение температурных датчиков
        // - Активация охлаждения
        // - Потенциально переход в безопасный режим
    }

    /// Устанавливает обработчик для watchdog таймера
    pub fn set_watchdog_nmi_handler() {
        set_nmi_handler(Self::watchdog_nmi_handler);
    }

    extern "x86-interrupt" fn watchdog_nmi_handler(_frame: InterruptStackFrame) {
        // Обработка watchdog таймера
        // - Сигнализация о зависании системы
        // - Попытка восстановления
        // - Потенциально перезагрузка

        crate::logging::log_error("Watchdog NMI: System appears to be hung!");

        // В реальной системе тут будет:
        // - Сохранение состояния системы
        // - Диагностика причин зависания
        // - Потенциальная попытка восстановления или перезагрузка
    }

    /// Проверяет состояние системы при NMI
    pub fn verify_system_health_on_nmi() -> SystemHealthStatus {
        // В реальной системе тут будет проверка:
        // - Состояния памяти
        // - Работы критических компонентов
        // - Целостности системных структур данных

        SystemHealthStatus::Healthy // Пока считаем здоровой
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SystemHealthStatus {
    Healthy,
    Degraded,
    Critical,
    Unrecoverable,
}

#[derive(Debug, Clone)]
pub struct SystemErrorInfo {
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub timestamp: u64,
}

/// Макросы для удобства работы с NMI
#[macro_export]
macro_rules! enable_nmi_protection {
    () => {
        $crate::nmi::NmiSystemInterface::init_system_nmi_handlers();
    };
}

#[macro_export]
macro_rules! set_custom_nmi_handler {
    ($handler:expr) => {
        $crate::nmi::set_nmi_handler($handler);
    };
}

#[macro_export]
macro_rules! trigger_custom_nmi {
    () => {
        $crate::nmi::trigger_nmi();
    };
}

#[macro_export]
macro_rules! nmi_safe_block {
    ($block:block) => {
        let safe_handler = $crate::nmi::SafeNmiHandler::new(3);
        // Оборачиваем блок в безопасный контекст с NMI
        $block
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nmi_manager_creation() {
        let nmi_manager = NmiManager::new();
        assert_eq!(nmi_manager.stats.total_count, 0);
        assert_eq!(nmi_manager.enabled, false);
    }

    #[test]
    fn test_nmi_stats_initialization() {
        let stats = NmiStats {
            total_count: 0,
            handled_count: 0,
            missed_count: 0,
            last_nmi_time: 0,
            last_nmi_types: [NmiType::Unknown; 10],
        };

        assert_eq!(stats.total_count, 0);
        assert_eq!(stats.last_nmi_types[0], NmiType::Unknown);
    }
}
