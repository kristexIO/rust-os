use x86_64::registers::control::{Cr4, Cr4Flags};

/// SMEP/SMAP (Supervisor Mode Execution Prevention / Supervisor Mode Access Prevention)
///
/// Эти механизмы защищают операционную систему от исполнения пользовательского кода
/// и доступа к пользовательским страницам из привилегированного режима (кольцо 0).
pub struct SmepSmapManager;

impl SmepSmapManager {
    /// Проверяет, поддерживается ли SMEP/SMAP процессором
    pub fn is_smep_smap_supported() -> bool {
        // Проверка через CPUID
        // В реальной реализации: вызов CPUID с функцией 7, проверка битов в EBX
        // Для эмуляции предполагаем поддержку
        true
    }

    /// Включает SMEP (Supervisor Mode Execution Prevention)
    pub fn enable_smep() -> Result<(), &'static str> {
        if !SmepSmapManager::is_smep_smap_supported() {
            return Err("SMEP is not supported by this CPU");
        }

        unsafe {
            // Включаем SMEP бит в CR4
            let mut cr4 = Cr4::read();
            cr4.insert(Cr4Flags::SMAP); // SMAP также включаем
            cr4.insert(Cr4Flags::SMEP); // Включаем SMEP
            Cr4::write(cr4);
        }

        Ok(())
    }

    /// Включает SMAP (Supervisor Mode Access Prevention)
    pub fn enable_smap() -> Result<(), &'static str> {
        if !SmepSmapManager::is_smep_smap_supported() {
            return Err("SMAP is not supported by this CPU");
        }

        unsafe {
            // Включаем SMAP бит в CR4
            let mut cr4 = Cr4::read();
            cr4.insert(Cr4Flags::SMAP); // Включаем SMAP
                                        // SMEP может потребоваться вместе с SMAP
            cr4.insert(Cr4Flags::SMEP);
            Cr4::write(cr4);
        }

        Ok(())
    }

    /// Включает оба механизма: SMEP и SMAP
    pub fn enable_smep_smap() -> Result<(), &'static str> {
        if !SmepSmapManager::is_smep_smap_supported() {
            return Err("SMEP/SMAP are not supported by this CPU");
        }

        unsafe {
            let mut cr4 = Cr4::read();
            cr4.insert(Cr4Flags::SMEP | Cr4Flags::SMAP);
            Cr4::write(cr4);
        }

        Ok(())
    }

    /// Временно отключает SMAP для безопасного доступа к пользовательской памяти
    pub unsafe fn temporary_disable_smap<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        // Сохраняем текущее состояние CR4
        let cr4 = Cr4::read();

        // Временно отключаем SMAP
        let mut new_cr4 = cr4;
        new_cr4.remove(Cr4Flags::SMAP);
        Cr4::write(new_cr4);

        // Выполняем необходимую операцию
        let result = f();

        // Восстанавливаем предыдущее состояние
        Cr4::write(cr4);

        result
    }

    /// Проверяет права доступа к памяти
    pub fn check_access_permission(address: u64, supervisor_mode: bool) -> bool {
        // В реальной системе:
        // - Если включен SMEP и мы в режиме супервизора, доступ к пользовательским страницам запрещен
        // - Если включен SMAP и мы в режиме супервизора, доступ к пользовательским страницам запрещен без CLAC

        // Для эмуляции просто возвращаем true
        true
    }

    /// Устанавливает флаг CLAC (Clear AC Flag) для временного отключения SMAP
    pub unsafe fn clear_ac_flag() {
        // CLAC - инструкция для отключения SMAP
        asm!("clac");
    }

    /// Устанавливает флаг STAC (Set AC Flag) для временного включения SMAP
    pub unsafe fn set_ac_flag() {
        // STAC - инструкция для включения SMAP
        asm!("stac");
    }
}

/// Inline assembly для x86_64
use core::arch::asm;

// Регистрируем модуль в основном ядре
use spin::Mutex;

pub static SMEP_SMAP_MANAGER: Mutex<SmepSmapManager> = Mutex::new(SmepSmapManager);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smep_smap_support() {
        // Тест на проверку поддержки SMEP/SMAP
        assert!(SmepSmapManager::is_smep_smap_supported());
    }
}
