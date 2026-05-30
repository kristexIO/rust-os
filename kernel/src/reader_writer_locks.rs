use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::Ordering;
use core::sync::atomic::{AtomicBool, AtomicUsize};

/// Reader-Writer Lock (RWLock) - примитив синхронизации, позволяющий
/// множественное чтение, но эксклюзивную запись.
///
/// Полезно для ситуаций, где чтение происходит чаще чем запись.
pub struct RwLock<T: ?Sized> {
    /// Счетчик активных читателей
    readers: AtomicUsize,
    /// Флаг эксклюзивной записи
    writer: AtomicBool,
    /// Флаг ожидания записи (для справедливости)
    write_waiting: AtomicBool,
    /// Защищенные данные
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send + Sync> Sync for RwLock<T> {}
unsafe impl<T: ?Sized + Send> Send for RwLock<T> {}

/// Охранник читателя
pub struct RwLockReadGuard<'a, T: ?Sized + 'a> {
    lock: &'a RwLock<T>,
}

/// Охранник писателя
pub struct RwLockWriteGuard<'a, T: ?Sized + 'a> {
    lock: &'a RwLock<T>,
}

impl<T> RwLock<T> {
    /// Создает новый RWLock
    pub const fn new(value: T) -> Self {
        RwLock {
            readers: AtomicUsize::new(0),
            writer: AtomicBool::new(false),
            write_waiting: AtomicBool::new(false),
            data: UnsafeCell::new(value),
        }
    }
}

impl<T: ?Sized> RwLock<T> {
    /// Блокирует RWLock для чтения, возвращая охранника
    pub fn read(&self) -> RwLockReadGuard<T> {
        loop {
            // Проверяем, есть ли ожидающие писатели
            // Если есть, не позволяем новым читателям начать
            if self.write_waiting.load(Ordering::Acquire) {
                core::hint::spin_loop();
                continue;
            }

            // Проверяем, есть ли активный писатель
            if self.writer.load(Ordering::Acquire) {
                core::hint::spin_loop();
                continue;
            }

            // Увеличиваем счетчик читателей
            let current_readers = self.readers.fetch_add(1, Ordering::Acquire);

            // Проверяем снова, не начался ли писатель между проверкой и увеличением
            if self.writer.load(Ordering::Acquire) {
                // Писатель начался, уменьшаем счетчик и пробуем снова
                self.readers.fetch_sub(1, Ordering::Release);
                core::hint::spin_loop();
                continue;
            }

            // Успешно захвачена блокировка чтения
            break;
        }

        RwLockReadGuard { lock: self }
    }

    /// Блокирует RWLock для записи, возвращая охранника
    pub fn write(&self) -> RwLockWriteGuard<T> {
        // Помечаем, что писатель ожидает
        self.write_waiting.store(true, Ordering::Release);

        loop {
            // Ждем, пока никто не читает
            if self.readers.load(Ordering::Acquire) != 0 {
                core::hint::spin_loop();
                continue;
            }

            // Пытаемся захватить для записи
            if self
                .writer
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                // Успешно захватили для записи
                break;
            }

            core::hint::spin_loop();
        }

        // Писатель захвачен, снимаем флаг ожидания
        self.write_waiting.store(false, Ordering::Release);

        RwLockWriteGuard { lock: self }
    }

    /// Пытается захватить блокировку для чтения без блокировки
    pub fn try_read(&self) -> Option<RwLockReadGuard<T>> {
        // Если нет ожидающих писателей и нет активного писателя
        if !self.write_waiting.load(Ordering::Relaxed) && !self.writer.load(Ordering::Relaxed) {
            // Атомарно увеличиваем счетчик читателей
            self.readers.fetch_add(1, Ordering::Acquire);

            // Проверяем снова, не появился ли писатель
            if !self.writer.load(Ordering::Relaxed) {
                return Some(RwLockReadGuard { lock: self });
            } else {
                // Писатель начался, откатываем
                self.readers.fetch_sub(1, Ordering::Release);
            }
        }

        None
    }

    /// Пытается захватить блокировку для записи без блокировки
    pub fn try_write(&self) -> Option<RwLockWriteGuard<T>> {
        // Должно быть 0 читателей и 0 писателей
        if self.readers.load(Ordering::Relaxed) == 0
            && self
                .writer
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
        {
            Some(RwLockWriteGuard { lock: self })
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

impl<T: ?Sized + Default> RwLock<T> {
    /// Создает новый RWLock с значением по умолчанию
    pub fn default() -> Self {
        Self::new(Default::default())
    }
}

impl<T: ?Sized> Deref for RwLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> Deref for RwLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        // Уменьшаем счетчик читателей
        self.lock.readers.fetch_sub(1, Ordering::Release);
    }
}

impl<T: ?Sized> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        // Снимаем флаг писателя
        self.lock.writer.store(false, Ordering::Release);
    }
}

/// Улучшенный RWLock с поддержкой upgradeable locks
pub struct UpgradeableRwLock<T: ?Sized> {
    /// Обычный RWLock
    inner: RwLock<T>,
    /// Счетчик upgradeable читателей
    upgrade_readers: AtomicUsize,
    /// Флаг, что происходит апгрейд
    upgrading: AtomicBool,
}

/// Охранник для upgradeable чтения
pub struct UpgradeReadGuard<'a, T: ?Sized + 'a> {
    lock: &'a UpgradeableRwLock<T>,
}

impl<T> UpgradeableRwLock<T> {
    /// Создает новый upgradeable RWLock
    pub fn new(value: T) -> Self {
        UpgradeableRwLock {
            inner: RwLock::new(value),
            upgrade_readers: AtomicUsize::new(0),
            upgrading: AtomicBool::new(false),
        }
    }
}

impl<T: ?Sized> UpgradeableRwLock<T> {
    /// Создает reader lock с возможностью апгрейда
    pub fn upgradeable_read(&self) -> UpgradeReadGuard<T> {
        // Сначала получаем обычный reader lock
        loop {
            // Проверяем, не идет ли апгрейд
            if self.upgrading.load(Ordering::Acquire) {
                core::hint::spin_loop();
                continue;
            }

            // Получаем reader lock
            let reader_guard = self.inner.read();

            // Увеличиваем счетчик upgrade readers
            self.upgrade_readers.fetch_add(1, Ordering::Acquire);

            // Проверяем снова, не начался ли апгрейд
            if self.upgrading.load(Ordering::Acquire) {
                // Откатываем, если апгрейд начался
                self.upgrade_readers.fetch_sub(1, Ordering::Release);
                drop(reader_guard);
                core::hint::spin_loop();
                continue;
            }

            // Успешно захвачен upgradeable reader
            break;
        }

        UpgradeReadGuard { lock: self }
    }

    /// Повышает reader lock до writer lock
    pub fn upgrade(&self, _guard: UpgradeReadGuard<T>) -> RwLockWriteGuard<T> {
        // Снимаем upgrade reader
        self.upgrade_readers.fetch_sub(1, Ordering::Release);

        // Помечаем, что идет апгрейд
        self.upgrading.store(true, Ordering::Release);

        // Получаем writer lock
        let write_guard = self.inner.write();

        // Снимаем флаг апгрейда
        self.upgrading.store(false, Ordering::Release);

        write_guard
    }
}

impl<T: ?Sized> Deref for UpgradeReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.lock.inner
    }
}

impl<T: ?Sized> Drop for UpgradeReadGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.upgrade_readers.fetch_sub(1, Ordering::Release);
    }
}

/// Интерфейс для работы с RWLocks
pub struct RwLockInterface;

impl RwLockInterface {
    /// Создает новый RWLock
    pub fn create_rwlock<T>(value: T) -> RwLock<T> {
        RwLock::new(value)
    }

    /// Создает новый Upgradeable RWLock
    pub fn create_upgradeable_rwlock<T>(value: T) -> UpgradeableRwLock<T> {
        UpgradeableRwLock::new(value)
    }
}

/// Макросы для удобства
#[macro_export]
macro_rules! rwlock {
    ($val:expr) => {
        $crate::reader_writer_locks::RwLock::new($val)
    };
}

#[macro_export]
macro_rules! with_read_lock {
    ($rwlock:expr, |$guard:ident| $body:block) => {
        let $guard = $rwlock.read();
        $body
    };
}

#[macro_export]
macro_rules! with_write_lock {
    ($rwlock:expr, |$guard:ident| $body:block) => {
        let $guard = $rwlock.write();
        $body
    };
}

#[macro_export]
macro_rules! try_with_read_lock {
    ($rwlock:expr, |$guard:ident| $body:block) => {
        if let Some($guard) = $rwlock.try_read() {
            $body
        }
    };
}

#[macro_export]
macro_rules! try_with_write_lock {
    ($rwlock:expr, |$guard:ident| $body:block) => {
        if let Some($guard) = $rwlock.try_write() {
            $body
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rwlock_basic() {
        let lock = RwLock::new(5);

        {
            // Проверяем чтение
            let guard = lock.read();
            assert_eq!(*guard, 5);
        }

        {
            // Проверяем запись
            let mut guard = lock.write();
            *guard = 10;
        }

        {
            // Проверяем, что значение изменилось
            let guard = lock.read();
            assert_eq!(*guard, 10);
        }
    }

    #[test]
    fn test_rwlock_multiple_readers() {
        let lock = RwLock::new(0);

        // Можем получить несколько reader охранников
        let reader1 = lock.read();
        let reader2 = lock.read();
        let reader3 = lock.read();

        assert_eq!(*reader1, 0);
        assert_eq!(*reader2, 0);
        assert_eq!(*reader3, 0);

        // Но writer должен ждать
        assert!(lock.try_write().is_none());
    }

    #[test]
    fn test_rwlock_writer_exclusion() {
        let lock = RwLock::new(0);

        // Блокируем writer
        let writer = lock.write();

        // Не можем получить reader
        assert!(lock.try_read().is_none());
        // Не можем получить другой writer
        assert!(lock.try_write().is_none());

        // После освобождения writer можем получить reader
        drop(writer);
        assert!(lock.try_read().is_some());
    }
}
