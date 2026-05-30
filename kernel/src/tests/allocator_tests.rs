use alloc::{boxed::Box, string::String, vec::Vec};

/// Тесты для аллокатора памяти
use crate::test_framework::{Test, TestResult};
use crate::{test_assert, test_assert_eq};

/// Тест базовой аллокации
fn test_basic_allocation() -> TestResult {
    // Проверяем, что можем выделить память
    let boxed = Box::new(42);
    test_assert_eq!(*boxed, 42);

    TestResult::Ok
}

pub const TEST_BASIC_ALLOCATION: Test = Test {
    name: "allocator::basic_allocation",
    test_fn: test_basic_allocation,
};

/// Тест множественных аллокаций
fn test_multiple_allocations() -> TestResult {
    let mut vec = Vec::new();

    // Выделяем память несколько раз
    for i in 0..100 {
        vec.push(i);
    }

    // Проверяем, что все значения на месте
    for i in 0..100 {
        test_assert_eq!(vec[i], i);
    }

    TestResult::Ok
}

pub const TEST_MULTIPLE_ALLOCATIONS: Test = Test {
    name: "allocator::multiple_allocations",
    test_fn: test_multiple_allocations,
};

/// Тест деаллокации
fn test_deallocation() -> TestResult {
    // Создаем и уничтожаем несколько объектов
    for _ in 0..10 {
        let _boxed = Box::new([0u8; 1024]);
        // boxed автоматически освобождается здесь
    }

    // Если мы дошли сюда без паники, деаллокация работает
    TestResult::Ok
}

pub const TEST_DEALLOCATION: Test = Test {
    name: "allocator::deallocation",
    test_fn: test_deallocation,
};

/// Тест реаллокации (через Vec)
fn test_reallocation() -> TestResult {
    let mut vec = Vec::with_capacity(10);

    // Добавляем элементы, чтобы вызвать реаллокацию
    for i in 0..100 {
        vec.push(i);
    }

    test_assert_eq!(vec.len(), 100);
    test_assert!(vec.capacity() >= 100);

    TestResult::Ok
}

pub const TEST_REALLOCATION: Test = Test {
    name: "allocator::reallocation",
    test_fn: test_reallocation,
};

/// Тест выравнивания
fn test_alignment() -> TestResult {
    // Проверяем, что аллокатор соблюдает выравнивание
    #[repr(align(64))]
    struct Aligned {
        data: [u8; 64],
    }

    let aligned = Box::new(Aligned {
        data: [0; 64],
    });
    let ptr = &*aligned as *const Aligned as usize;

    // Проверяем, что адрес выровнен на 64 байта
    test_assert_eq!(ptr % 64, 0);

    TestResult::Ok
}

pub const TEST_ALIGNMENT: Test = Test {
    name: "allocator::alignment",
    test_fn: test_alignment,
};

/// Тест большой аллокации
fn test_large_allocation() -> TestResult {
    // Пытаемся выделить большой блок памяти (1 MB)
    let large = Box::new([0u8; 1024 * 1024]);

    // Проверяем, что можем записать и прочитать
    test_assert_eq!(large[0], 0);
    test_assert_eq!(large[1024 * 1024 - 1], 0);

    TestResult::Ok
}

pub const TEST_LARGE_ALLOCATION: Test = Test {
    name: "allocator::large_allocation",
    test_fn: test_large_allocation,
};

/// Тест аллокации нулевого размера
fn test_zero_size_allocation() -> TestResult {
    // Vec с нулевой емкостью не должен вызывать аллокацию
    let vec: Vec<u8> = Vec::new();
    test_assert_eq!(vec.len(), 0);
    test_assert_eq!(vec.capacity(), 0);

    // String с нулевой длиной
    let s = String::new();
    test_assert_eq!(s.len(), 0);

    TestResult::Ok
}

pub const TEST_ZERO_SIZE_ALLOCATION: Test = Test {
    name: "allocator::zero_size_allocation",
    test_fn: test_zero_size_allocation,
};
