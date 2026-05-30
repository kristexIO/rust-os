/// Модуль тестов для ядра ОС
///
/// Содержит unit-тесты и интеграционные тесты для различных компонентов
pub mod allocator_tests;
// pub mod frame_allocator_tests;
pub mod fs_tests;
pub mod memory_tests;
pub mod network_tests;

use crate::test_framework::{Test, TestRunner};

/// Получить все тесты
pub fn get_all_tests() -> &'static [Test] {
    &[
        // Тесты аллокатора
        allocator_tests::TEST_BASIC_ALLOCATION,
        allocator_tests::TEST_MULTIPLE_ALLOCATIONS,
        allocator_tests::TEST_DEALLOCATION,
        allocator_tests::TEST_REALLOCATION,
        allocator_tests::TEST_ALIGNMENT,
        allocator_tests::TEST_LARGE_ALLOCATION,
        allocator_tests::TEST_ZERO_SIZE_ALLOCATION,
        // Тесты фреймового аллокатора
        // frame_allocator_tests::TEST_FRAME_ALLOC_BASIC,
        // frame_allocator_tests::TEST_FRAME_ALLOC_CONTIGUOUS,
        // frame_allocator_tests::TEST_FRAME_ALLOC_OOM,
        // Тесты памяти
        memory_tests::TEST_BUDDY_ALLOCATOR,
        memory_tests::TEST_SLAB_ALLOCATOR,
        memory_tests::TEST_MEMORY_STATS,
        memory_tests::TEST_MEMORY_LEAK_DETECTION,
        memory_tests::TEST_COW_PAGES,
        // Тесты файловых систем
        fs_tests::TEST_RAMFS_CREATE_FILE,
        fs_tests::TEST_RAMFS_READ_WRITE,
        fs_tests::TEST_RAMFS_DIRECTORY,
        fs_tests::TEST_VFS_MOUNT,
        fs_tests::TEST_FAT32_OPERATIONS,
        fs_tests::TEST_VFS_ACL_BOOTSTRAP,
        // Тесты сети
        network_tests::TEST_ETHERNET_PARSE,
        network_tests::TEST_IPV4_PARSE,
        network_tests::TEST_TCP_CHECKSUM,
        network_tests::TEST_ARP_PACKET,
        network_tests::TEST_TCP_HEADER,
        network_tests::TEST_UDP_HEADER,
        network_tests::TEST_HTTP_URL_SECURITY,
    ]
}

/// Создать test runner со всеми тестами
pub fn create_test_runner() -> TestRunner {
    TestRunner::new(get_all_tests())
}
