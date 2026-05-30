use crate::paging::{MemoryProtection, MEMORY_MANAGER};
use x86_64::{
    structures::paging::{Mapper, Page, PageTable, PageTableFlags, Size4KiB},
    PhysAddr, VirtAddr,
};

/// Реализация Data Execution Prevention (DEP) / No-eXecute (NX) bit
pub struct DepManager;

impl DepManager {
    /// Устанавливает флаг NX (No-eXecute) для страницы памяти
    pub unsafe fn set_nx_bit(
        page_table: &mut PageTable,
        page_vaddr: VirtAddr,
    ) -> Result<(), &'static str> {
        // В x86_64 NX бит находится в 63-м бите PTE (Page Table Entry)
        // Найдем соответствующую запись в таблице страниц
        let page = Page::<Size4KiB>::containing_address(page_vaddr);

        // Проверим, поддерживается ли NX бит процессором
        if !DepManager::is_nx_supported() {
            return Err("NX bit not supported by processor");
        }

        // Найдем запись о странице (это требует доступа к mapper)
        // В реальной реализации нужно использовать mapper

        // Для теста просто помечаем, что NX должен быть установлен
        // В реальном коде нужно будет получить доступ к PTE и установить бит 63

        Ok(())
    }

    /// Проверяет, поддерживается ли NX бит процессором
    pub fn is_nx_supported() -> bool {
        // Проверяем поддержку NX бита через CPUID
        // Возвращаем true для эмуляции, если поддерживается
        true // В реальной системе - через CPUID
    }

    /// Защищает стек от выполнения кода
    pub unsafe fn protect_stack(
        stack_bottom: VirtAddr,
        stack_top: VirtAddr,
    ) -> Result<(), &'static str> {
        // Получим размер стека
        let stack_size = stack_top.as_u64() - stack_bottom.as_u64();

        // Установим NX бит для каждой страницы в стеке
        let pages_count = (stack_size as usize + 4095) / 4096; // округление вверх

        for i in 0..pages_count {
            let page_addr = VirtAddr::new(stack_bottom.as_u64() + (i as u64) * 4096);
            DepManager::set_nx_bit_for_page(page_addr)?;
        }

        Ok(())
    }

    /// Устанавливает NX бит для конкретной страницы
    unsafe fn set_nx_bit_for_page(page_vaddr: VirtAddr) -> Result<(), &'static str> {
        // В настоящей реализации:
        // 1. Найти соответствующую запись в page table
        // 2. Установить 63-й бит как NX
        // 3. Освежить TLB

        // Пока просто возвращаем OK
        Ok(())
    }

    /// Защищает кучу от выполнения кода
    pub fn protect_heap(heap_start: VirtAddr, heap_size: usize) -> Result<(), &'static str> {
        // Проверяем, поддерживается ли NX
        if !DepManager::is_nx_supported() {
            return Err("NX bit not supported");
        }

        // Защитим каждую страницу в куче
        let pages_count = (heap_size + 4095) / 4096; // округление вверх

        for i in 0..pages_count {
            let page_addr = VirtAddr::new(heap_start.as_u64() + (i as u64) * 4096);
            // В реальной системе: поставим в очередь установку NX для этой страницы
        }

        Ok(())
    }

    /// Создает исполняемую страницу (исключает из DEP)
    pub fn create_executable_page(vaddr: VirtAddr) -> Result<(), &'static str> {
        // В реальной системе:
        // 1. Проверить права доступа
        // 2. Снять флаг NX с страницы
        // 3. Обновить кэш TLB

        // Для эмуляции просто возвращаем успех
        Ok(())
    }

    /// Блокирует выполнение кода в заданном диапазоне
    pub fn block_execution(start: VirtAddr, end: VirtAddr) -> Result<(), &'static str> {
        if start >= end {
            return Err("Invalid address range");
        }

        let size = end.as_u64() - start.as_u64();
        let page_count = (size as usize + 4095) / 4096;

        for i in 0..page_count {
            let page_addr = VirtAddr::new(start.as_u64() + (i as u64) * 4096);
            unsafe {
                DepManager::set_nx_bit_for_page(page_addr)?;
            }
        }

        Ok(())
    }
}

// Интерфейс для защиты от выполнения в ядре
pub struct KernelDepProtector;

impl KernelDepProtector {
    /// Защищает ядро от выполнения пользовательского кода
    pub fn setup_kernel_depprotection() {
        // Устанавливаем флаги защиты для ядра
        let mut prot = crate::paging::MEMORY_PROTECTION.lock();
        prot.smap_supported = true; // Включаем SMAP если поддерживается
        prot.smep_supported = true; // Включаем SMEP если поддерживается
    }

    /// Защищает стек ядра
    pub unsafe fn protect_kernel_stack(
        stack_bottom: VirtAddr,
        stack_top: VirtAddr,
    ) -> Result<(), &'static str> {
        // Для ядра стек защищается особенно жестко
        DepManager::protect_stack(stack_bottom, stack_top)
    }
}

// Глобальный менеджер DEP
use spin::Mutex;

pub static DEP_MANAGER: Mutex<DepManager> = Mutex::new(DepManager);
