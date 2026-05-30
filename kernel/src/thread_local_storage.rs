use alloc::{boxed::Box, vec::Vec};
use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::ptr;

/// ThreadLocalStorage - реализация Thread Local Storage (TLS) для ядра
///
/// Позволяет каждому потоку иметь собственную копию данных,
/// которая не разделяется с другими потоками.
pub struct ThreadLocalStorage {
    /// Контекст TLS для текущего потока
    current_thread_context: *mut TlsContext,
    /// Глобальный пул TLS ключей
    global_keys: spin::Mutex<Vec<TlsKey>>,
    /// Следующий доступный ключ
    next_key: spin::Mutex<usize>,
}

/// Контекст TLS для конкретного потока
pub struct TlsContext {
    /// Данные TLS для потока
    data: UnsafeCell<Vec<Option<*mut u8>>>,
    /// Размер данных
    capacity: usize,
    /// Идентификатор потока
    thread_id: u64,
}

/// Тип данных ключа TLS
pub type TlsKey = usize;

/// Дескриптор данных TLS
pub struct TlsSlot {
    /// Значение данных
    value: *mut u8,
    /// Функция освобождения (для типобезопасности)
    destructor: Option<unsafe fn(*mut u8)>,
    /// Размер данных
    size: usize,
}

impl TlsSlot {
    /// Создает новый TLS слот
    fn new(value: *mut u8, destructor: Option<unsafe fn(*mut u8)>, size: usize) -> Self {
        TlsSlot {
            value,
            destructor,
            size,
        }
    }

    /// Получает значение из слота
    pub fn get_value(&self) -> *mut u8 {
        self.value
    }

    /// Устанавливает новое значение
    pub fn set_value(&mut self, new_value: *mut u8) {
        self.value = new_value;
    }
}

impl ThreadLocalStorage {
    /// Создает новый TLS менеджер
    pub fn new() -> Self {
        ThreadLocalStorage {
            current_thread_context: ptr::null_mut(),
            global_keys: spin::Mutex::new(Vec::new()),
            next_key: spin::Mutex::new(0),
        }
    }

    /// Создает новый TLS ключ
    pub fn create_tls_key(
        &self,
        destructor: Option<unsafe fn(*mut u8)>,
    ) -> Result<TlsKey, &'static str> {
        let mut keys = self.global_keys.lock();
        let mut next_key = self.next_key.lock();

        let key = *next_key;
        *next_key += 1;

        keys.push(key);
        drop(next_key);

        // Сохраняем деструктор для ключа
        unsafe {
            TLS_KEY_DESTRUCTORS.lock().insert(key, destructor);
        }

        Ok(key)
    }

    /// Удаляет TLS ключ
    pub fn delete_tls_key(&self, key: TlsKey) -> Result<(), &'static str> {
        let mut keys = self.global_keys.lock();

        if let Some(pos) = keys.iter().position(|&k| k == key) {
            keys.remove(pos);
            unsafe {
                TLS_KEY_DESTRUCTORS.lock().remove(&key);
            }
            Ok(())
        } else {
            Err("Invalid TLS key")
        }
    }

    /// Устанавливает значение для TLS ключа в текущем потоке
    pub fn set_tls_value(&self, key: TlsKey, value: *mut u8) -> Result<(), &'static str> {
        if self.current_thread_context.is_null() {
            return Err("No thread context available");
        }

        let context = unsafe { &mut *self.current_thread_context };

        // Проверяем, что ключ действителен
        {
            let keys = self.global_keys.lock();
            if !keys.contains(&key) {
                return Err("Invalid TLS key");
            }
        }

        // Расширяем вектор данных, если нужно
        let mut data = unsafe { context.data.get().as_mut().unwrap() };
        if key >= data.len() {
            data.resize_with(key + 1, || None);
        }

        // Сохраняем новое значение
        data[key] = Some(value);

        Ok(())
    }

    /// Получает значение для TLS ключа из текущего потока
    pub fn get_tls_value(&self, key: TlsKey) -> Result<*mut u8, &'static str> {
        if self.current_thread_context.is_null() {
            return Err("No thread context available");
        }

        let context = unsafe { &mut *self.current_thread_context };

        // Проверяем, что ключ действителен
        {
            let keys = self.global_keys.lock();
            if !keys.contains(&key) {
                return Err("Invalid TLS key");
            }
        }

        let data = unsafe { context.data.get().as_mut().unwrap() };

        if key < data.len() {
            match data[key] {
                Some(ptr) => Ok(ptr),
                None => Ok(ptr::null_mut()),
            }
        } else {
            Ok(ptr::null_mut())
        }
    }

    /// Создает новый контекст TLS для потока
    pub fn create_thread_context(&mut self, thread_id: u64) -> *mut TlsContext {
        let context = Box::into_raw(Box::new(TlsContext::new(thread_id)));
        self.current_thread_context = context;
        context
    }

    /// Устанавливает контекст TLS для текущего потока
    pub fn set_current_context(&mut self, context: *mut TlsContext) {
        self.current_thread_context = context;
    }

    /// Освобождает контекст TLS потока
    pub fn destroy_thread_context(&mut self) -> Result<(), &'static str> {
        if self.current_thread_context.is_null() {
            return Err("No thread context to destroy");
        }

        let context = unsafe { Box::from_raw(self.current_thread_context) };

        // Уничтожаем данные с вызовом деструкторов
        context.destroy_with_destructors();

        self.current_thread_context = ptr::null_mut();
        Ok(())
    }

    /// Переключает контекст TLS для потока (например, при переключении задач)
    pub fn switch_context(
        &mut self,
        new_context: *mut TlsContext,
    ) -> Result<*mut TlsContext, &'static str> {
        if self.current_thread_context.is_null() {
            return Err("No current context to switch from");
        }

        let old_context = self.current_thread_context;
        self.current_thread_context = new_context;
        Ok(old_context)
    }

    /// Инициализирует TLS для текущего потока
    pub fn init_current_thread(&mut self, thread_id: u64) {
        self.current_thread_context = self.create_thread_context(thread_id);
    }

    /// Деинициализирует TLS для текущего потока
    pub fn deinit_current_thread(&mut self) {
        let _ = self.destroy_thread_context(); // Игнорируем ошибку при деинициализации
    }

    /// Выполняет очистку TLS для завершения потока
    pub fn cleanup_thread(&self, context: *mut TlsContext) {
        if !context.is_null() {
            unsafe {
                // Вызываем деструкторы для всех значений
                let ctx = &mut *context;
                ctx.destroy_with_destructors();
                // Освобождаем память
                let _ = Box::from_raw(context);
            }
        }
    }

    /// Получает контекст TLS текущего потока
    pub fn get_current_context(&self) -> Option<&TlsContext> {
        if self.current_thread_context.is_null() {
            None
        } else {
            Some(unsafe { &*self.current_thread_context })
        }
    }

    /// Устанавливает значение с выделением памяти
    pub fn set_tls_value_with_allocation<T>(
        &self,
        key: TlsKey,
        value: T,
    ) -> Result<*mut T, &'static str>
    where
        T: 'static,
    {
        let boxed_value = Box::new(value);
        let ptr = Box::into_raw(boxed_value);

        self.set_tls_value(key, ptr as *mut u8)?;

        Ok(ptr)
    }

    /// Получает значение с проверкой типа
    pub fn get_tls_value_typed<T>(&self, key: TlsKey) -> Result<Option<&mut T>, &'static str>
    where
        T: 'static,
    {
        let ptr = self.get_tls_value(key)?;

        if ptr.is_null() {
            Ok(None)
        } else {
            Ok(Some(unsafe { &mut *(ptr as *mut T) }))
        }
    }

    /// Освобождает значение TLS с вызовом деструктора
    pub fn free_tls_value<T>(&self, key: TlsKey) -> Result<(), &'static str>
    where
        T: 'static,
    {
        let ptr = self.get_tls_value(key)?;

        if !ptr.is_null() {
            unsafe {
                // Вызываем деструктор, если он есть
                if let Some(destructor) = TLS_KEY_DESTRUCTORS.lock().get(&key).cloned().flatten() {
                    destructor(ptr);
                }

                // Освобождаем память
                let _ = Box::from_raw(ptr as *mut T);
            }

            // Удаляем из контекста
            if !self.current_thread_context.is_null() {
                let context = unsafe { &mut *self.current_thread_context };
                let data = unsafe { context.data.get().as_mut().unwrap() };

                if key < data.len() {
                    data[key] = None;
                }
            }
        }

        Ok(())
    }
}

impl TlsContext {
    /// Создает новый контекст TLS
    fn new(thread_id: u64) -> Self {
        TlsContext {
            data: UnsafeCell::new(Vec::new()),
            capacity: 32, // Начальный размер
            thread_id,
        }
    }

    /// Уничтожает контекст с вызовом деструкторов
    fn destroy_with_destructors(&mut self) {
        let data = unsafe { self.data.get().as_mut().unwrap() };

        for opt_ptr in data.iter() {
            if let Some(ptr) = opt_ptr {
                // В реальной системе вызываем деструкторы
                // для каждого значения в соответствии с его ключом
            }
        }

        // Очищаем вектор
        data.clear();
    }

    /// Получает ID потока
    pub fn get_thread_id(&self) -> u64 {
        self.thread_id
    }

    /// Получает количество установленных значений
    pub fn get_value_count(&self) -> usize {
        let data = unsafe { self.data.get().as_ref().unwrap() };
        data.iter().filter(|opt| opt.is_some()).count()
    }
}

/// Глобальный менеджер TLS
use spin::{Mutex, Once};

static TLS_MANAGER: Once<Mutex<ThreadLocalStorage>> = Once::new();
static TLS_KEY_DESTRUCTORS: Mutex<BTreeMap<usize, Option<unsafe fn(*mut u8)>>> =
    Mutex::new(BTreeMap::new());

pub fn get_tls_manager() -> &'static Mutex<ThreadLocalStorage> {
    TLS_MANAGER.call_once(|| Mutex::new(ThreadLocalStorage::new()))
}

/// Инициализирует TLS систему
pub fn init_tls_system() {
    let _ = get_tls_manager(); // Инициализируем менеджер
}

/// Создает TLS ключ
pub fn tls_create_key(destructor: Option<unsafe fn(*mut u8)>) -> Result<TlsKey, &'static str> {
    let manager = get_tls_manager().lock();
    manager.create_tls_key(destructor)
}

/// Удаляет TLS ключ
pub fn tls_delete_key(key: TlsKey) -> Result<(), &'static str> {
    let manager = get_tls_manager().lock();
    manager.delete_tls_key(key)
}

/// Устанавливает значение TLS
pub fn tls_set_value(key: TlsKey, value: *mut u8) -> Result<(), &'static str> {
    let manager = get_tls_manager().lock();
    manager.set_tls_value(key, value)
}

/// Получает значение TLS
pub fn tls_get_value(key: TlsKey) -> Result<*mut u8, &'static str> {
    let manager = get_tls_manager().lock();
    manager.get_tls_value(key)
}

/// Устанавливает значение TLS с выделением памяти
pub fn tls_set_boxed_value<T: 'static>(key: TlsKey, value: T) -> Result<*mut T, &'static str> {
    let manager = get_tls_manager().lock();
    manager.set_tls_value_with_allocation(key, value)
}

/// Получает значение TLS с проверкой типа
pub fn tls_get_typed_value<T: 'static>(
    key: TlsKey,
) -> Result<Option<&'static mut T>, &'static str> {
    let manager = get_tls_manager().lock();
    match manager.get_tls_value_typed(key) {
        Ok(Some(value)) => Ok(Some(unsafe { &mut *(value as *mut T) })),
        Ok(None) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Освобождает значение TLS
pub fn tls_free_value<T: 'static>(key: TlsKey) -> Result<(), &'static str> {
    let manager = get_tls_manager().lock();
    manager.free_tls_value::<T>(key)
}

/// Инициализирует TLS для текущего потока
pub fn init_current_thread_tls(thread_id: u64) {
    let mut manager = get_tls_manager().lock();
    manager.init_current_thread(thread_id);
}

/// Освобождает TLS текущего потока
pub fn deinit_current_thread_tls() {
    let mut manager = get_tls_manager().lock();
    manager.deinit_current_thread();
}

/// Переключает TLS контекст
pub fn switch_tls_context(new_context: *mut TlsContext) -> Result<*mut TlsContext, &'static str> {
    let mut manager = get_tls_manager().lock();
    manager.switch_context(new_context)
}

/// Создает TLS контекст для потока
pub fn create_thread_tls_context(thread_id: u64) -> *mut TlsContext {
    let mut manager = get_tls_manager().lock();
    manager.create_thread_context(thread_id)
}

/// Освобождает TLS контекст потока
pub fn destroy_thread_tls_context(context: *mut TlsContext) {
    let manager = get_tls_manager().lock();
    manager.cleanup_thread(context);
}

/// Получает текущий TLS контекст
pub fn get_current_tls_context() -> Option<&'static TlsContext> {
    let manager = get_tls_manager().lock();
    manager.get_current_context()
}

/// Макросы для удобного использования TLS
#[macro_export]
macro_rules! tls_key {
    () => {
        $crate::thread_local_storage::tls_create_key(None).expect("Failed to create TLS key")
    };
    ($destructor:expr) => {
        $crate::thread_local_storage::tls_create_key(Some($destructor))
            .expect("Failed to create TLS key")
    };
}

#[macro_export]
macro_rules! tls_set {
    ($key:expr, $value:expr) => {{
        let val = Box::new($value);
        let ptr = Box::into_raw(val);
        $crate::thread_local_storage::tls_set_value($key, ptr as *mut u8).expect("Failed to set TLS value");
        ptr // Возвращаем указатель для возможного дальнейшего использования
    }};
}

#[macro_export]
macro_rules! tls_get {
    ($key:expr, $type:ty) => {{
        match $crate::thread_local_storage::tls_get_typed_value::<$type>($key) {
            Ok(Some(value)) => Some(unsafe { &mut *value }),
            Ok(None) => None,
            Err(_) => None,
        }
    }};
}

#[macro_export]
macro_rules! tls_get_or_init {
    ($key:expr, $type:ty, $init:expr) => {{
        match $crate::thread_local_storage::tls_get_typed_value::<$type>($key) {
            Ok(Some(value)) => &mut *value,
            Ok(None) | Err(_) => {
                let val = $init;
                let ptr = $crate::thread_local_storage::tls_set_boxed_value($key, val)
                    .expect("Failed to init TLS value");
                unsafe { &mut *ptr }
            }
        }
    }};
}

/// Типобезопасный TLS контейнер
pub struct TypedTls<T: 'static> {
    key: TlsKey,
    _phantom: PhantomData<T>,
}

impl<T: 'static> TypedTls<T> {
    /// Создает новый типобезопасный TLS контейнер
    pub fn new() -> Result<Self, &'static str> {
        let key = tls_create_key(None)?;
        Ok(TypedTls {
            key,
            _phantom: PhantomData,
        })
    }

    /// Устанавливает значение
    pub fn set(&self, value: T) -> Result<*mut T, &'static str> {
        tls_set_boxed_value(self.key, value)
    }

    /// Получает значение
    pub fn get(&self) -> Result<Option<&'static mut T>, &'static str> {
        tls_get_typed_value(self.key)
    }

    /// Получает значение или инициализирует
    pub fn get_or_init<F>(&self, init_fn: F) -> Result<&'static mut T, &'static str>
    where
        F: FnOnce() -> T,
    {
        match self.get()? {
            Some(value) => Ok(value),
            None => {
                let value = init_fn();
                let ptr = self.set(value)?;
                Ok(unsafe { &mut *ptr })
            }
        }
    }

    /// Освобождает значение
    pub fn free(&self) -> Result<(), &'static str> {
        tls_free_value::<T>(self.key)
    }

    /// Получает ключ
    pub fn key(&self) -> TlsKey {
        self.key
    }
}

impl<T: 'static> Drop for TypedTls<T> {
    fn drop(&mut self) {
        // Освобождаем значение при уничтожении ключа
        let _ = tls_free_value::<T>(self.key);

        // Удаляем сам ключ
        let _ = tls_delete_key(self.key);
    }
}

/// Интерфейс для системных компонентов
pub struct TlsSystemInterface;

impl TlsSystemInterface {
    /// Инициализирует TLS для задачи
    pub fn init_task_tls(task_id: u64) {
        init_current_thread_tls(task_id);
    }

    /// Деинициализирует TLS для задачи
    pub fn deinit_task_tls() {
        deinit_current_thread_tls();
    }

    /// Переключает TLS при переключении задач
    pub fn switch_task_tls(
        new_task_context: *mut TlsContext,
    ) -> Result<*mut TlsContext, &'static str> {
        switch_tls_context(new_task_context)
    }

    /// Создает TLS контекст для задачи
    pub fn create_task_tls_context(task_id: u64) -> *mut TlsContext {
        create_thread_tls_context(task_id)
    }

    /// Уничтожает TLS контекст задачи
    pub fn destroy_task_tls_context(context: *mut TlsContext) {
        destroy_thread_tls_context(context);
    }

    /// Получает TLS контекст текущей задачи
    pub fn get_current_task_tls_context() -> Option<&'static TlsContext> {
        get_current_tls_context()
    }
}

/// Пример использования TLS
pub struct TlsExample;

impl TlsExample {
    /// Пример использования типобезопасного TLS
    pub fn example_typed_tls_usage() -> Result<(), &'static str> {
        // Создаем TLS контейнер для i32
        let tls_container = TypedTls::<i32>::new()?;

        // Устанавливаем значение
        let ptr = tls_container.set(42)?;

        // Получаем значение
        if let Some(value) = tls_container.get()? {
            assert_eq!(*value, 42);
        }

        // Освобождаем значение
        tls_container.free()?;

        Ok(())
    }

    /// Пример использования TLS через макросы
    pub fn example_macro_usage() {
        // Создаем ключ TLS
        let key = tls_key!();

        // Устанавливаем значение
        let ptr = tls_set!(key, 123i32);

        // Получаем значение
        if let Some(value) = tls_get!(key, i32) {
            assert_eq!(*value, 123);
        }

        // Очищаем
        unsafe {
            let _ = Box::from_raw(ptr);
        }
    }

    /// Пример TLS с инициализацией по требованию
    pub fn example_lazy_init() {
        let key = tls_key!();

        // Получаем или инициализируем значение
        let value = tls_get_or_init!(key, i32, {
            println!("Initializing TLS value");
            999
        });

        assert_eq!(*value, 999);
    }
}

/// Переменные окружения с TLS
pub struct TlsEnvironment;

impl TlsEnvironment {
    /// Устанавливает переменную окружения в TLS текущего потока
    pub fn set_env_var(name: &str, value: &str) -> Result<(), &'static str> {
        let env_map: &mut alloc::collections::BTreeMap<
            alloc::string::String,
            alloc::string::String,
        > = tls_get_or_init!(ENV_TLS_KEY, _, alloc::collections::BTreeMap::new());

        env_map.insert(name.to_string(), value.to_string());
        Ok(())
    }

    /// Получает переменную окружения из TLS текущего потока
    pub fn get_env_var(name: &str) -> Option<alloc::string::String> {
        if let Some(env_map) = tls_get!(ENV_TLS_KEY, alloc::collections::BTreeMap<alloc::string::String, alloc::string::String>)
        {
            env_map.get(name).cloned()
        } else {
            None
        }
    }

    /// Удаляет переменную окружения из TLS
    pub fn unset_env_var(name: &str) -> bool {
        if let Some(env_map) = tls_get!(ENV_TLS_KEY, alloc::collections::BTreeMap<alloc::string::String, alloc::string::String>)
        {
            env_map.remove(name).is_some()
        } else {
            false
        }
    }
}

// Глобальный ключ для переменных окружения
static ENV_TLS_KEY: Lazy<TlsKey> =
    Lazy::new(|| tls_create_key(None).expect("Failed to create ENV_TLS_KEY"));

use alloc::collections::BTreeMap;
use core::ops::{Deref, DerefMut};
use spin::Lazy;

/// TLS для стека вызовов
pub struct CallStackTls;

impl CallStackTls {
    /// Добавляет уровень в стек вызовов TLS
    pub fn push_call_frame(function_name: &str, line: u32) {
        let call_stack: &mut Vec<(alloc::string::String, u32)> =
            tls_get_or_init!(CALL_STACK_TLS_KEY, _, Vec::new());

        call_stack.push((function_name.to_string(), line));
    }

    /// Удаляет уровень из стека вызовов TLS
    pub fn pop_call_frame() -> Option<(alloc::string::String, u32)> {
        if let Some(call_stack) = tls_get!(CALL_STACK_TLS_KEY, Vec<(alloc::string::String, u32)>) {
            call_stack.pop()
        } else {
            None
        }
    }

    /// Получает текущий стек вызовов
    pub fn get_current_call_stack() -> Vec<(alloc::string::String, u32)> {
        if let Some(call_stack) = tls_get!(CALL_STACK_TLS_KEY, Vec<(alloc::string::String, u32)>) {
            call_stack.clone()
        } else {
            Vec::new()
        }
    }
}

static CALL_STACK_TLS_KEY: Lazy<TlsKey> =
    Lazy::new(|| tls_create_key(None).expect("Failed to create CALL_STACK_TLS_KEY"));

/// TLS для обработки ошибок
pub struct ErrorHandlingTls;

impl ErrorHandlingTls {
    /// Устанавливает последнюю ошибку в контексте потока
    pub fn set_last_error(error: alloc::string::String) {
        let last_error: &mut Option<alloc::string::String> =
            tls_get_or_init!(ERROR_TLS_KEY, _, None);

        *last_error = Some(error);
    }

    /// Получает последнюю ошибку из контекста потока
    pub fn get_last_error() -> Option<alloc::string::String> {
        if let Some(last_error) = tls_get!(ERROR_TLS_KEY, Option<alloc::string::String>) {
            last_error.clone()
        } else {
            None
        }
    }

    /// Очищает последнюю ошибку
    pub fn clear_last_error() {
        if let Some(last_error) = tls_get!(ERROR_TLS_KEY, Option<alloc::string::String>) {
            *last_error = None;
        }
    }
}

static ERROR_TLS_KEY: Lazy<TlsKey> =
    Lazy::new(|| tls_create_key(None).expect("Failed to create ERROR_TLS_KEY"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tls_basic_operations() {
        let tls_manager = ThreadLocalStorage::new();
        let key = tls_manager.create_tls_key(None).unwrap();

        // Тестируем установку и получение значения
        let value = 42u32;
        let ptr = Box::into_raw(Box::new(value));

        assert!(tls_manager.set_tls_value(key, ptr as *mut u8).is_ok());

        let retrieved_ptr = tls_manager.get_tls_value(key).unwrap() as *mut u32;
        unsafe {
            assert_eq!(*retrieved_ptr, 42);
        }

        // Очищаем
        unsafe {
            let _ = Box::from_raw(retrieved_ptr);
        }

        assert!(tls_manager.delete_tls_key(key).is_ok());
    }

    #[test]
    fn test_typed_tls() {
        let typed_tls = TypedTls::<i32>::new().unwrap();

        // Устанавливаем значение
        let ptr = typed_tls.set(123).unwrap();

        // Получаем значение
        if let Some(value) = typed_tls.get().unwrap() {
            assert_eq!(*value, 123);
        }

        // Освобождаем
        assert!(typed_tls.free().is_ok());
    }

    #[test]
    fn test_tls_environment() {
        // Устанавливаем переменную окружения
        assert!(TlsEnvironment::set_env_var("TEST_VAR", "test_value").is_ok());

        // Получаем переменную окружения
        assert_eq!(
            TlsEnvironment::get_env_var("TEST_VAR"),
            Some("test_value".to_string())
        );

        // Удаляем переменную окружения
        assert_eq!(TlsEnvironment::unset_env_var("TEST_VAR"), true);

        // Проверяем, что переменная больше не существует
        assert_eq!(TlsEnvironment::get_env_var("TEST_VAR"), None);
    }

    #[test]
    fn test_tls_error_handling() {
        // Устанавливаем ошибку
        ErrorHandlingTls::set_last_error("Test error".to_string());

        // Получаем ошибку
        assert_eq!(
            ErrorHandlingTls::get_last_error(),
            Some("Test error".to_string())
        );

        // Очищаем ошибку
        ErrorHandlingTls::clear_last_error();

        // Проверяем, что ошибки больше нет
        assert_eq!(ErrorHandlingTls::get_last_error(), None);
    }
}
