use core::cell::UnsafeCell;
use core::hint::spin_loop;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

/// Spinlock - примитив синхронизации для многопроцессорных систем
///
/// Spinlock блокирует поток выполнения циклическим проверением флага,
/// что делает его эффективным для коротких критических секций.
pub struct Spinlock<T: ?Sized> {
    /// Флаг занятости спинлока
    locked: AtomicBool,
    /// Защищенные данные
    data: UnsafeCell<T>,
}

/// Объект-охранник спинлока
pub struct SpinlockGuard<'a, T: ?Sized + 'a> {
    lock: &'a Spinlock<T>,
}

unsafe impl<T: ?Sized + Send> Sync for Spinlock<T> {}
unsafe impl<T: ?Sized + Send> Send for Spinlock<T> {}

impl<T> Spinlock<T> {
    /// Создает новый спинлок
    pub const fn new(value: T) -> Self {
        Spinlock {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(value),
        }
    }

    /// Создает разблокированный спинлок
    pub fn into_inner(self) -> T {
        // Мы можем безопасно вернуть внутренние данные,
        // так как владелец структуры уничтожается
        self.data.into_inner()
    }

    /// Обменивает значение спинлока на новое
    pub fn swap(&self, value: T) -> T {
        let _guard = self.lock();
        unsafe { core::ptr::replace(self.data.get(), value) }
    }
}

impl<T: ?Sized> Spinlock<T> {
    /// Блокирует спинлок, возвращая охранника
    pub fn lock(&self) -> SpinlockGuard<T> {
        // Пытаемся захватить спинлок в цикле
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            // Если спинлок занят, выполняем спин-вейт
            while self.locked.load(Ordering::Relaxed) {
                spin_loop();
            }
        }

        SpinlockGuard { lock: self }
    }

    /// Пытается захватить спинлок без блокировки
    pub fn try_lock(&self) -> Option<SpinlockGuard<T>> {
        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(SpinlockGuard { lock: self })
        } else {
            None
        }
    }

    /// Возвращает ссылку на данные без блокировки
    ///
    /// # Safety
    ///
    /// Вызывающий должен быть уверен, что доступ к данным синхронизирован
    pub unsafe fn get_ref(&self) -> &T {
        &*self.data.get()
    }

    /// Возвращает изменяемую ссылку на данные без блокировки
    ///
    /// # Safety
    ///
    /// Вызывающий должен быть уверен, что доступ к данным синхронизирован
    pub unsafe fn get_mut(&mut self) -> &mut T {
        &mut *self.data.get()
    }
}

impl<T: ?Sized + Default> Spinlock<T> {
    /// Создает новый спинлок с значением по умолчанию
    pub fn default() -> Self {
        Self::new(Default::default())
    }
}

impl<T: ?Sized> Deref for SpinlockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> DerefMut for SpinlockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for SpinlockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

impl<T: ?Sized + PartialEq> PartialEq for Spinlock<T> {
    fn eq(&self, other: &Self) -> bool {
        *self.lock() == *other.lock()
    }
}

impl<T: ?Sized + Eq> Eq for Spinlock<T> {}

/// Adaptive Spinlock - улучшенная версия с адаптивным поведением
pub struct AdaptiveSpinlock<T: ?Sized> {
    /// Флаг занятости
    locked: AtomicBool,
    /// Данные
    data: UnsafeCell<T>,
    /// Счетчик спинов
    spin_count: core::sync::atomic::AtomicU32,
}

impl<T> AdaptiveSpinlock<T> {
    /// Создает новый адаптивный спинлок
    pub const fn new(value: T) -> Self {
        AdaptiveSpinlock {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(value),
            spin_count: core::sync::atomic::AtomicU32::new(0),
        }
    }
}

impl<T: ?Sized> AdaptiveSpinlock<T> {
    /// Блокирует с адаптивным поведением
    pub fn lock(&self) -> AdaptiveSpinlockGuard<T> {
        let mut spin_count = 0;

        loop {
            // Сначала пробуем захватить немедленно
            if self
                .locked
                .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                // Успешно захвачен
                return AdaptiveSpinlockGuard { lock: self };
            }

            // Увеличиваем счетчик спинов
            spin_count += 1;

            // После нескольких неудачных попыток - немного отдыхаем
            if spin_count > 100 {
                // Выполняем yielding или другие оптимизации
                core::hint::spin_loop();

                // Если слишком долго ждем - делаем более продолжительный yield
                if spin_count > 1000 {
                    // В реальной системе тут может быть вызов scheduler_yield()
                    // или другие оптимизации
                    spin_count = 0; // Сбрасываем счетчик
                }
            } else {
                // Короткая задержка
                while self.locked.load(Ordering::Relaxed) {
                    spin_loop();
                }
            }
        }
    }

    /// Пытается захватить без блокировки
    pub fn try_lock(&self) -> Option<AdaptiveSpinlockGuard<T>> {
        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(AdaptiveSpinlockGuard { lock: self })
        } else {
            None
        }
    }
}

/// Охранник адаптивного спинлока
pub struct AdaptiveSpinlockGuard<'a, T: ?Sized + 'a> {
    lock: &'a AdaptiveSpinlock<T>,
}

impl<T: ?Sized> Deref for AdaptiveSpinlockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> DerefMut for AdaptiveSpinlockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for AdaptiveSpinlockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
        // Обновляем счетчик спинов при освобождении
        self.lock.spin_count.fetch_add(1, Ordering::Relaxed);
    }
}

/// Ticket Spinlock - улучшенная версия с гарантией отсутствия голодания
pub struct TicketSpinlock<T: ?Sized> {
    /// Счетчик выданных билетов
    ticket: core::sync::atomic::AtomicU32,
    /// Счетчик обслуженных билетов
    served: core::sync::atomic::AtomicU32,
    /// Защищенные данные
    data: UnsafeCell<T>,
}

/// Охранник тикетного спинлока
pub struct TicketSpinlockGuard<'a, T: ?Sized + 'a> {
    lock: &'a TicketSpinlock<T>,
    my_ticket: u32,
}

impl<T> TicketSpinlock<T> {
    /// Создает новый тикетный спинлок
    pub fn new(value: T) -> Self {
        TicketSpinlock {
            ticket: core::sync::atomic::AtomicU32::new(0),
            served: core::sync::atomic::AtomicU32::new(0),
            data: UnsafeCell::new(value),
        }
    }
}

impl<T: ?Sized> TicketSpinlock<T> {
    /// Блокирует с тикетной системой
    pub fn lock(&self) -> TicketSpinlockGuard<T> {
        // Выдаем билет
        let my_ticket = self.ticket.fetch_add(1, Ordering::Relaxed);

        // Ждем, пока наш билет не будет обслужен
        while self.served.load(Ordering::Acquire) != my_ticket {
            spin_loop();
        }

        TicketSpinlockGuard {
            lock: self,
            my_ticket,
        }
    }

    /// Пытается захватить без блокировки
    pub fn try_lock(&self) -> Option<TicketSpinlockGuard<T>> {
        let current_served = self.served.load(Ordering::Relaxed);
        let next_ticket = self.ticket.load(Ordering::Relaxed);

        // Проверяем, есть ли мы следующими в очереди
        if current_served == next_ticket {
            // Попробуем захватить
            if self
                .ticket
                .compare_exchange(
                    next_ticket,
                    next_ticket + 1,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                Some(TicketSpinlockGuard {
                    lock: self,
                    my_ticket: next_ticket,
                })
            } else {
                None
            }
        } else {
            None
        }
    }
}

impl<T: ?Sized> Deref for TicketSpinlockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> DerefMut for TicketSpinlockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for TicketSpinlockGuard<'_, T> {
    fn drop(&mut self) {
        // Обслуживаем следующий билет
        self.lock.served.fetch_add(1, Ordering::Release);
    }
}

/// Интерфейс для работы со спинлоками
pub struct SpinlockInterface;

impl SpinlockInterface {
    /// Создает новый	spinlock
    pub fn create_spinlock<T>(value: T) -> Spinlock<T> {
        Spinlock::new(value)
    }

    /// Создает адаптивный спинлок
    pub fn create_adaptive_spinlock<T>(value: T) -> AdaptiveSpinlock<T> {
        AdaptiveSpinlock::new(value)
    }

    /// Создает тикетный спинлок
    pub fn create_ticket_spinlock<T>(value: T) -> TicketSpinlock<T> {
        TicketSpinlock::new(value)
    }
}

/// Макросы для удобства
#[macro_export]
macro_rules! spinlock {
    ($val:expr) => {
        $crate::spinlocks::Spinlock::new($val)
    };
}

#[macro_export]
macro_rules! with_spinlock {
    ($lock:expr, |$guard:ident| $body:block) => {
        let $guard = $lock.lock();
        $body
    };
}

#[macro_export]
macro_rules! try_with_spinlock {
    ($lock:expr, |$guard:ident| $body:block) => {
        if let Some($guard) = $lock.try_lock() {
            $body
        }
    };
}

/// Статистика спинлоков
#[derive(Debug)]
pub struct SpinlockStats {
    /// Количество захватов
    pub locks: u64,
    /// Количество неудачных попыток
    pub failed_attempts: u64,
    /// Среднее время ожидания (в циклах)
    pub avg_wait_cycles: u64,
}

impl SpinlockStats {
    pub fn new() -> Self {
        SpinlockStats {
            locks: 0,
            failed_attempts: 0,
            avg_wait_cycles: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinlock_basic() {
        let lock = Spinlock::new(0);
        {
            let mut guard = lock.lock();
            *guard = 5;
        }
        assert_eq!(*lock.lock(), 5);
    }

    #[test]
    fn test_spinlock_try_lock() {
        let lock = Spinlock::new(0);

        {
            let guard = lock.lock();
            assert!(lock.try_lock().is_none());
            // guard выходит из области видимости
        }

        assert!(lock.try_lock().is_some());
    }

    #[test]
    fn test_ticket_spinlock_ordering() {
        let lock = TicketSpinlock::new(Vec::new());

        // Тест не может полностью проверить порядок в однопоточном контексте,
        // но проверяет базовую функциональность
        {
            let mut guard = lock.lock();
            guard.push(1);
        }

        {
            let mut guard = lock.lock();
            guard.push(2);
        }

        let guard = lock.lock();
        assert_eq!(guard.as_slice(), &[1, 2]);
    }
}
