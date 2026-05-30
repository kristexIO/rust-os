use core::sync::atomic::{AtomicUsize, AtomicIsize, AtomicU32, AtomicI32, AtomicU64, AtomicI64, AtomicBool};
use core::sync::atomic::Ordering;

/// Atomic Operations - безопасные атомарные операции для многопроцессорных систем
/// 
/// Атомарные операции позволяют безопасно манипулировать данными из нескольких
/// потоков без использования блокировок.
pub struct AtomicOperations;

impl AtomicOperations {
    /// Атомарное увеличение usize значения
    pub fn atomic_usize_increment(location: &AtomicUsize) -> usize {
        location.fetch_add(1, Ordering::SeqCst)
    }
    
    /// Атомарное уменьшение usize значения
    pub fn atomic_usize_decrement(location: &AtomicUsize) -> usize {
        location.fetch_sub(1, Ordering::SeqCst)
    }
    
    /// Атомарное сложение usize значения
    pub fn atomic_usize_add(location: &AtomicUsize, value: usize) -> usize {
        location.fetch_add(value, Ordering::SeqCst)
    }
    
    /// Атомарное вычитание usize значения
    pub fn atomic_usize_sub(location: &AtomicUsize, value: usize) -> usize {
        location.fetch_sub(value, Ordering::SeqCst)
    }
    
    /// Атомарное сравнение и обмен (CAS) для usize
    pub fn atomic_usize_compare_exchange(location: &AtomicUsize, current: usize, new: usize) -> Result<usize, usize> {
        location.compare_exchange(current, new, Ordering::SeqCst, Ordering::SeqCst)
    }
    
    /// Атомарное сложение и возврат результата
    pub fn atomic_usize_add_and_fetch(location: &AtomicUsize, value: usize) -> usize {
        location.fetch_add(value, Ordering::SeqCst).wrapping_add(value)
    }
    
    /// Атомарное получение значения
    pub fn atomic_usize_load(location: &AtomicUsize) -> usize {
        location.load(Ordering::SeqCst)
    }
    
    /// Атомарное сохранение значения
    pub fn atomic_usize_store(location: &AtomicUsize, value: usize) {
        location.store(value, Ordering::SeqCst);
    }
}

/// Атомарные операции для разных типов
pub mod atomics {
    use super::*;
    
    /// Атомарная ячейка для usize
    #[derive(Debug)]
    pub struct AtomicUsizeCell {
        value: AtomicUsize,
    }
    
    impl AtomicUsizeCell {
        pub const fn new(value: usize) -> Self {
            Self { value: AtomicUsize::new(value) }
        }
        
        pub fn load(&self) -> usize {
            self.value.load(Ordering::SeqCst)
        }
        
        pub fn store(&self, value: usize) {
            self.value.store(value, Ordering::SeqCst);
        }
        
        pub fn swap(&self, new: usize) -> usize {
            self.value.swap(new, Ordering::SeqCst)
        }
        
        pub fn compare_exchange(&self, current: usize, new: usize) -> Result<usize, usize> {
            self.value.compare_exchange(current, new, Ordering::SeqCst, Ordering::SeqCst)
        }
        
        pub fn compare_exchange_weak(&self, current: usize, new: usize) -> Result<usize, usize> {
            self.value.compare_exchange_weak(current, new, Ordering::SeqCst, Ordering::SeqCst)
        }
        
        pub fn fetch_add(&self, val: usize) -> usize {
            self.value.fetch_add(val, Ordering::SeqCst)
        }
        
        pub fn fetch_sub(&self, val: usize) -> usize {
            self.value.fetch_sub(val, Ordering::SeqCst)
        }
        
        pub fn fetch_and(&self, val: usize) -> usize {
            self.value.fetch_and(val, Ordering::SeqCst)
        }
        
        pub fn fetch_or(&self, val: usize) -> usize {
            self.value.fetch_or(val, Ordering::SeqCst)
        }
        
        pub fn fetch_xor(&self, val: usize) -> usize {
            self.value.fetch_xor(val, Ordering::SeqCst)
        }
    }
    
    /// Атомарная ячейка для i32
    #[derive(Debug)]
    pub struct AtomicI32Cell {
        value: AtomicI32,
    }
    
    impl AtomicI32Cell {
        pub const fn new(value: i32) -> Self {
            Self { value: AtomicI32::new(value) }
        }
        
        pub fn load(&self) -> i32 {
            self.value.load(Ordering::SeqCst)
        }
        
        pub fn store(&self, value: i32) {
            self.value.store(value, Ordering::SeqCst);
        }
        
        pub fn swap(&self, new: i32) -> i32 {
            self.value.swap(new, Ordering::SeqCst)
        }
        
        pub fn compare_exchange(&self, current: i32, new: i32) -> Result<i32, i32> {
            self.value.compare_exchange(current, new, Ordering::SeqCst, Ordering::SeqCst)
        }
        
        pub fn fetch_add(&self, val: i32) -> i32 {
            self.value.fetch_add(val, Ordering::SeqCst)
        }
        
        pub fn fetch_sub(&self, val: i32) -> i32 {
            self.value.fetch_sub(val, Ordering::SeqCst)
        }
    }
    
    /// Атомарная ячейка для bool
    #[derive(Debug)]
    pub struct AtomicBoolCell {
        value: AtomicBool,
    }
    
    impl AtomicBoolCell {
        pub const fn new(value: bool) -> Self {
            Self { value: AtomicBool::new(value) }
        }
        
        pub fn load(&self) -> bool {
            self.value.load(Ordering::SeqCst)
        }
        
        pub fn store(&self, value: bool) {
            self.value.store(value, Ordering::SeqCst);
        }
        
        pub fn swap(&self, new: bool) -> bool {
            self.value.swap(new, Ordering::SeqCst)
        }
        
        pub fn compare_exchange(&self, current: bool, new: bool) -> Result<bool, bool> {
            self.value.compare_exchange(current, new, Ordering::SeqCst, Ordering::SeqCst)
        }
        
        pub fn fetch_and(&self, val: bool) -> bool {
            self.value.fetch_and(val, Ordering::SeqCst)
        }
        
        pub fn fetch_or(&self, val: bool) -> bool {
            self.value.fetch_or(val, Ordering::SeqCst)
        }
        
        pub fn fetch_xor(&self, val: bool) -> bool {
            self.value.fetch_xor(val, Ordering::SeqCst)
        }
    }
    
    /// Атомарная ячейка для u64
    #[derive(Debug)]
    pub struct AtomicU64Cell {
        value: AtomicU64,
    }
    
    impl AtomicU64Cell {
        pub const fn new(value: u64) -> Self {
            Self { value: AtomicU64::new(value) }
        }
        
        pub fn load(&self) -> u64 {
            self.value.load(Ordering::SeqCst)
        }
        
        pub fn store(&self, value: u64) {
            self.value.store(value, Ordering::SeqCst);
        }
        
        pub fn swap(&self, new: u64) -> u64 {
            self.value.swap(new, Ordering::SeqCst)
        }
        
        pub fn compare_exchange(&self, current: u64, new: u64) -> Result<u64, u64> {
            self.value.compare_exchange(current, new, Ordering::SeqCst, Ordering::SeqCst)
        }
        
        pub fn fetch_add(&self, val: u64) -> u64 {
            self.value.fetch_add(val, Ordering::SeqCst)
        }
        
        pub fn fetch_sub(&self, val: u64) -> u64 {
            self.value.fetch_sub(val, Ordering::SeqCst)
        }
    }
}

/// Атомарные счетчики и флаги
pub struct AtomicPrimitives;

impl AtomicPrimitives {
    /// Атомарный счетчик
    pub struct AtomicCounter {
        count: AtomicUsize,
    }
    
    impl AtomicCounter {
        pub const fn new(initial: usize) -> Self {
            AtomicCounter { count: AtomicUsize::new(initial) }
        }
        
        pub fn increment(&self) -> usize {
            self.count.fetch_add(1, Ordering::SeqCst) + 1
        }
        
        pub fn decrement(&self) -> usize {
            let current = self.count.fetch_sub(1, Ordering::SeqCst);
            current - 1
        }
        
        pub fn add(&self, value: usize) -> usize {
            self.count.fetch_add(value, Ordering::SeqCst) + value
        }
        
        pub fn get(&self) -> usize {
            self.count.load(Ordering::SeqCst)
        }
        
        pub fn set(&self, value: usize) {
            self.count.store(value, Ordering::SeqCst);
        }
        
        pub fn compare_and_swap(&self, current: usize, new: usize) -> usize {
            self.count.compare_exchange_weak(current, new, Ordering::SeqCst, Ordering::SeqCst)
                .unwrap_or_else(|x| x)
        }
    }
    
    /// Атомарный флаг
    pub struct AtomicFlag {
        flag: AtomicBool,
    }
    
    impl AtomicFlag {
        pub const fn new(initial: bool) -> Self {
            AtomicFlag { flag: AtomicBool::new(initial) }
        }
        
        pub fn is_set(&self) -> bool {
            self.flag.load(Ordering::SeqCst)
        }
        
        pub fn set(&self) -> bool {
            self.flag.swap(true, Ordering::SeqCst)
        }
        
        pub fn clear(&self) -> bool {
            self.flag.swap(false, Ordering::SeqCst)
        }
        
        pub fn toggle(&self) -> bool {
            self.flag.fetch_xor(true, Ordering::SeqCst)
        }
    }
}

/// Атомарные операции для счетчиков и статистики
pub struct AtomicStatistics {
    /// Счетчик операций
    pub operations: atomics::AtomicUsizeCell,
    /// Счетчик ошибок
    pub errors: atomics::AtomicUsizeCell,
    /// Счетчик успешных операций
    pub successes: atomics::AtomicUsizeCell,
    /// Атомарный флаг готовности
    pub ready: atomics::AtomicBoolCell,
}

impl AtomicStatistics {
    pub const fn new() -> Self {
        AtomicStatistics {
            operations: atomics::AtomicUsizeCell::new(0),
            errors: atomics::AtomicUsizeCell::new(0),
            successes: atomics::AtomicUsizeCell::new(0),
            ready: atomics::AtomicBoolCell::new(false),
        }
    }
    
    /// Фиксирует успешную операцию
    pub fn record_success(&self) {
        self.operations.fetch_add(1, Ordering::SeqCst);
        self.successes.fetch_add(1, Ordering::SeqCst);
    }
    
    /// Фиксирует ошибку
    pub fn record_error(&self) {
        self.operations.fetch_add(1, Ordering::SeqCst);
        self.errors.fetch_add(1, Ordering::SeqCst);
    }
    
    /// Получает статистику
    pub fn get_stats(&self) -> (usize, usize, usize) {
        (
            self.operations.load(),
            self.errors.load(),
            self.successes.load()
        )
    }
    
    /// Проверяет, все ли операции успешны
    pub fn all_successful(&self) -> bool {
        self.errors.load() == 0 && self.ready.load()
    }
    
    /// Помечает систему как готовую
    pub fn mark_ready(&self) {
        self.ready.store(true);
    }
}

/// Атомарные операции с барьерами памяти
pub struct AtomicMemoryBarriers;

impl AtomicMemoryBarriers {
    /// Полный барьер памяти
    pub fn full_fence() {
        core::sync::atomic::fence(Ordering::SeqCst);
    }
    
    /// Барьер для чтения
    pub fn read_fence() {
        core::sync::atomic::fence(Ordering::Acquire);
    }
    
    /// Барьер для записи
    pub fn write_fence() {
        core::sync::atomic::fence(Ordering::Release);
    }
    
    /// Барьер для чтения-записи
    pub fn acquire_release_fence() {
        core::sync::atomic::fence(Ordering::AcqRel);
    }
}

/// Интерфейс для использования атомарных операций
pub struct AtomicInterface;

impl AtomicInterface {
    /// Создает новый атомарный счетчик
    pub fn create_counter(initial: usize) -> atomics::AtomicUsizeCell {
        atomics::AtomicUsizeCell::new(initial)
    }
    
    /// Создает атомарный флаг
    pub fn create_flag(initial: bool) -> atomics::AtomicBoolCell {
        atomics::AtomicBoolCell::new(initial)
    }
    
    /// Создает атомарную ячейку для i32
    pub fn create_i32_cell(initial: i32) -> atomics::AtomicI32Cell {
        atomics::AtomicI32Cell::new(initial)
    }
    
    /// Создает атомарную ячейку для u64
    pub fn create_u64_cell(initial: u64) -> atomics::AtomicU64Cell {
        atomics::AtomicU64Cell::new(initial)
    }
}

/// Макросы для удобства работы с атомарными операциями
#[macro_export]
macro_rules! atomic_increment {
    ($var:expr) => {
        $var.fetch_add(1, core::sync::atomic::Ordering::SeqCst)
    };
    ($var:expr, $amount:expr) => {
        $var.fetch_add($amount, core::sync::atomic::Ordering::SeqCst)
    };
}

#[macro_export]
macro_rules! atomic_decrement {
    ($var:expr) => {
        $var.fetch_sub(1, core::sync::atomic::Ordering::SeqCst)
    };
    ($var:expr, $amount:expr) => {
        $var.fetch_sub($amount, core::sync::atomic::Ordering::SeqCst)
    };
}

#[macro_export]
macro_rules! atomic_compare_exchange {
    ($var:expr, $current:expr, $new:expr) => {
        $var.compare_exchange($current, $new, core::sync::atomic::Ordering::SeqCst, core::sync::atomic::Ordering::SeqCst)
    };
}

#[macro_export]
macro_rules! atomic_load {
    ($var:expr) => {
        $var.load(core::sync::atomic::Ordering::SeqCst)
    };
}

#[macro_export]
macro_rules! atomic_store {
    ($var:expr, $value:expr) => {
        $var.store($value, core::sync::atomic::Ordering::SeqCst)
    };
}

#[macro_export]
macro_rules! atomic_fetch_add {
    ($var:expr, $value:expr) => {
        $var.fetch_add($value, core::sync::atomic::Ordering::SeqCst)
    };
}

/// Кольцевой буфер с атомарными операциями
pub struct AtomicRingBuffer<T> {
    buffer: Vec<Option<T>>,
    capacity: usize,
    read_index: AtomicUsize,
    write_index: AtomicUsize,
    size: AtomicUsize,
}

impl<T: Clone> AtomicRingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        let mut buffer = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            buffer.push(None);
        }
        
        AtomicRingBuffer {
            buffer,
            capacity,
            read_index: AtomicUsize::new(0),
            write_index: AtomicUsize::new(0),
            size: AtomicUsize::new(0),
        }
    }
    
    /// Пытается вставить элемент в буфер
    pub fn try_push(&self, item: T) -> Result<(), T> {
        let current_size = self.size.load(Ordering::SeqCst);
        if current_size >= self.capacity {
            return Err(item); // Буфер полон
        }
        
        let write_idx = self.write_index.load(Ordering::SeqCst);
        let next_write_idx = (write_idx + 1) % self.capacity;
        
        // Пытаемся обновить индекс записи
        if self.write_index.compare_exchange(
            write_idx, 
            next_write_idx, 
            Ordering::SeqCst, 
            Ordering::SeqCst
        ).is_ok() {
            // Успешно захватили слот, теперь можем записать
            unsafe {
                let ptr = self.buffer.as_ptr();
                core::ptr::write(&(*ptr.add(write_idx)).unwrap(), item);
            }
            
            // Увеличиваем размер
            self.size.fetch_add(1, Ordering::SeqCst);
            Ok(())
        } else {
            // Потоки пересеклись, возвращаем элемент
            Err(item)
        }
    }
    
    /// Пытается извлечь элемент из буфера
    pub fn try_pop(&self) -> Option<T> {
        let current_size = self.size.load(Ordering::SeqCst);
        if current_size == 0 {
            return None; // Буфер пуст
        }
        
        let read_idx = self.read_index.load(Ordering::SeqCst);
        let next_read_idx = (read_idx + 1) % self.capacity;
        
        // Пытаемся обновить индекс чтения
        if self.read_index.compare_exchange(
            read_idx, 
            next_read_idx, 
            Ordering::SeqCst, 
            Ordering::SeqCst
        ).is_ok() {
            // Уменьшаем размер
            self.size.fetch_sub(1, Ordering::SeqCst);
            
            // Получаем элемент
            unsafe {
                let ptr = self.buffer.as_ptr();
                (&(*ptr.add(read_idx))).clone()
            }
        } else {
            None // Потоки пересеклись
        }
    }
    
    /// Возвращает текущий размер буфера
    pub fn len(&self) -> usize {
        self.size.load(Ordering::SeqCst)
    }
    
    /// Проверяет, пуст ли буфер
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    
    /// Проверяет, полон ли буфер
    pub fn is_full(&self) -> bool {
        self.len() >= self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atomic_counter() {
        let counter = AtomicPrimitives::AtomicCounter::new(0);
        assert_eq!(counter.increment(), 1);
        assert_eq!(counter.add(5), 6);
        assert_eq!(counter.get(), 6);
    }

    #[test]
    fn test_atomic_flag() {
        let flag = AtomicPrimitives::AtomicFlag::new(false);
        assert_eq!(flag.is_set(), false);
        
        flag.set();
        assert_eq!(flag.is_set(), true);
        
        flag.clear();
        assert_eq!(flag.is_set(), false);
    }
    
    #[test]
    fn test_atomic_ring_buffer() {
        let buffer = AtomicRingBuffer::new(2);
        
        assert!(buffer.try_push(1).is_ok());
        assert!(buffer.try_push(2).is_ok());
        assert!(buffer.try_push(3).is_err()); // Буфер полон
        
        assert_eq!(buffer.try_pop(), Some(1));
        assert_eq!(buffer.try_pop(), Some(2));
        assert_eq!(buffer.try_pop(), None);   // Буфер пуст
    }
}