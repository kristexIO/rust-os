use alloc::{collections::BTreeMap, vec::Vec};

use spin::Mutex;
use x86_64::{
    structures::paging::{Mapper, Page, PageTableFlags, Size4KiB},
    PhysAddr, VirtAddr,
};

/// Copy-on-Write (COW) - механизм при котором страницы разделяются между
/// процессами до тех пор, пока кто-то не попытается записать в них,
/// тогда страница становится уникальной для процесса.
pub struct CowManager {
    /// Карта разделяемых страниц
    shared_pages: BTreeMap<PhysAddr, SharedPage>,
    /// Счетчики ссылок на страницы
    reference_counts: BTreeMap<PhysAddr, usize>,
}

/// Информация о разделяемой странице
struct SharedPage {
    /// Виртуальные адреса, ссылающиеся на эту физическую страницу
    referenced_by: Vec<VirtAddr>,
    /// Является ли страница разделяемой
    is_shared: bool,
    /// Была ли страница модифицирована
    is_dirty: bool,
}

impl CowManager {
    /// Создает новый менеджер COW
    pub fn new() -> Self {
        CowManager {
            shared_pages: BTreeMap::new(),
            reference_counts: BTreeMap::new(),
        }
    }

    /// Помечает страницу как разделяемую между процессами
    pub fn mark_page_shared(&mut self, phys_addr: PhysAddr) {
        if !self.shared_pages.contains_key(&phys_addr) {
            self.shared_pages.insert(phys_addr, SharedPage {
                referenced_by: Vec::new(),
                is_shared: true,
                is_dirty: false,
            });
            self.reference_counts.insert(phys_addr, 1);
        } else {
            *self.reference_counts.get_mut(&phys_addr).unwrap() += 1;
        }
    }

    /// Удаляет ссылку на разделяемую страницу
    pub fn unmark_page_shared(&mut self, phys_addr: PhysAddr) {
        if let Some(count) = self.reference_counts.get_mut(&phys_addr) {
            *count -= 1;
            if *count == 0 {
                self.shared_pages.remove(&phys_addr);
                self.reference_counts.remove(&phys_addr);
            }
        }
    }

    /// Проверяет, нужно ли выполнить COW при попытке записи
    pub fn needs_cow(&self, phys_addr: PhysAddr) -> bool {
        if let Some(_page) = self.shared_pages.get(&phys_addr) {
            // Если страница разделяется между несколькими процессами
            // и счетчик ссылок больше 1, то нужен COW при изменении
            self.get_reference_count(phys_addr) > 1
        } else {
            false
        }
    }

    /// Выполняет COW: копирует страницу, если она разделяется
    pub fn handle_cow(&mut self, phys_addr: PhysAddr) -> Result<PhysAddr, &'static str> {
        if self.needs_cow(phys_addr) {
            // Нужно скопировать страницу
            let new_phys_addr = self.allocate_private_page()?;

            // Копируем содержимое старой страницы в новую
            unsafe {
                CowManager::copy_page_content(phys_addr, new_phys_addr);
            }

            // Обновляем информацию о разделяемых страницах
            *self.reference_counts.get_mut(&phys_addr).unwrap() -= 1;

            // Создаем новую запись для скопированной страницы
            self.shared_pages.insert(new_phys_addr, SharedPage {
                referenced_by: Vec::new(),
                is_shared: false,
                is_dirty: true,
            });
            self.reference_counts.insert(new_phys_addr, 1);

            Ok(new_phys_addr)
        } else {
            // Страница не разделяется, просто возвращаем оригинал
            Ok(phys_addr)
        }
    }

    /// Выделяет новую физическую страницу для процесса
    fn allocate_private_page(&self) -> Result<PhysAddr, &'static str> {
        // В реальной системе тут будет аллокация из пула физических страниц
        // Для эмуляции просто возвращаем фиктивный адрес
        // В продакшене нужно использовать FrameAllocator
        Err("Physical frame allocator not implemented")
    }

    /// Копирует содержимое страницы из одной в другую
    unsafe fn copy_page_content(from: PhysAddr, to: PhysAddr) {
        let from_ptr = from.as_u64() as *const u8;
        let to_ptr = to.as_u64() as *mut u8;

        // Копируем по 4096 байт (размер страницы)
        for i in 0..4096 {
            *to_ptr.add(i) = *from_ptr.add(i);
        }
    }

    /// Получает количество ссылок на страницу
    pub fn get_reference_count(&self, phys_addr: PhysAddr) -> usize {
        *self.reference_counts.get(&phys_addr).unwrap_or(&0)
    }

    /// Добавляет виртуальный адрес в список, ссылающихся на физическую страницу
    pub fn add_virt_addr_ref(&mut self, phys_addr: PhysAddr, virt_addr: VirtAddr) {
        if let Some(page) = self.shared_pages.get_mut(&phys_addr) {
            if !page.referenced_by.contains(&virt_addr) {
                page.referenced_by.push(virt_addr);
            }
        }
    }

    /// Удаляет виртуальный адрес из списка, ссылающихся на физическую страницу
    pub fn remove_virt_addr_ref(&mut self, phys_addr: PhysAddr, virt_addr: VirtAddr) {
        if let Some(page) = self.shared_pages.get_mut(&phys_addr) {
            page.referenced_by.retain(|&addr| addr != virt_addr);
        }
    }
}

/// Хелперы для интеграции с пейджингом
pub struct PagingCowHelper;

impl PagingCowHelper {
    /// Проверяет, нужен ли COW при обращении к странице
    pub fn check_cow_needed(page_flags: PageTableFlags) -> bool {
        // Если флаг Read-Only выставлен и страница помечена как разделяемая
        // значит, при попытке записи нужно выполнить COW
        !page_flags.contains(PageTableFlags::WRITABLE)
            && page_flags.contains(PageTableFlags::PRESENT)
    }

    /// Устанавливает флаги для разделяемой страницы
    pub fn set_shared_page_flags(flags: &mut PageTableFlags) {
        // Убираем WRITE флаг, чтобы вызвать исключение при записи
        flags.remove(PageTableFlags::WRITABLE);
        // Добавляем USER_ACCESSIBLE если еще не установлен
        flags.insert(PageTableFlags::USER_ACCESSIBLE);
    }

    /// Устанавливает флаги для уникальной (после COW) страницы
    pub fn set_unique_page_flags(flags: &mut PageTableFlags) {
        // Возвращаем WRITE флаг
        flags.insert(PageTableFlags::WRITABLE | PageTableFlags::PRESENT);
    }
}

// Глобальный COW менеджер
use spin::Once;

static COW_MANAGER_ONCE: Once<Mutex<CowManager>> = Once::new();

pub fn get_cow_manager() -> &'static Mutex<CowManager> {
    COW_MANAGER_ONCE.call_once(|| Mutex::new(CowManager::new()))
}

/// Функция для интеграции с обработчиком исключений
pub fn handle_page_fault_cow(
    _virt_addr: VirtAddr,
    phys_addr: PhysAddr,
) -> Result<(), &'static str> {
    let mut cow_manager = get_cow_manager().lock();

    if cow_manager.needs_cow(phys_addr) {
        // Выполняем COW
        let _new_phys_addr = cow_manager.handle_cow(phys_addr)?;

        // Обновляем таблицу страниц, чтобы использовать новую физическую страницу
        // Это требует изменения соответствующей записи в PageTable
        // В реальной реализации тут будет Mapper::map_to с новым фреймом

        Ok(())
    } else {
        // Это не COW-страница, обработка нормального page fault
        Err("Not a COW page fault")
    }
}

/// Макросы для удобства работы с COW
#[macro_export]
macro_rules! cow_clone_page {
    ($phys_addr:expr) => {
        match $crate::cow::get_cow_manager().lock().handle_cow($phys_addr) {
            Ok(new_addr) => new_addr,
            Err(e) => {
                $crate::println!("COW error: {}", e);
                $phys_addr // Возвращаем оригинал в случае ошибки
            },
        }
    };
}

#[macro_export]
macro_rules! cow_mark_shared {
    ($phys_addr:expr) => {
        $crate::cow::get_cow_manager()
            .lock()
            .mark_page_shared($phys_addr);
    };
}

#[macro_export]
macro_rules! cow_unmark_shared {
    ($phys_addr:expr) => {
        $crate::cow::get_cow_manager()
            .lock()
            .unmark_page_shared($phys_addr);
    };
}

/// Клонирование виртуального адресного пространства с COW
pub struct VirtualMemoryCloner;

impl VirtualMemoryCloner {
    /// Клонирует адресное пространство с COW для нового процесса
    pub fn clone_vm_with_cow(
        source_page_table: &mut x86_64::structures::paging::PageTable,
        _dest_page_table: &mut x86_64::structures::paging::PageTable,
    ) -> Result<(), &'static str> {
        let mut cow_manager = get_cow_manager().lock();

        // Проходим по всем записям в исходной таблице страниц
        for (i, entry) in source_page_table.iter().enumerate() {
            if !entry.is_unused() {
                use x86_64::structures::paging::page_table::PageTableIndex;
                let page = Page::<Size4KiB>::from_page_table_indices(
                    PageTableIndex::new(0),
                    PageTableIndex::new(0),
                    PageTableIndex::new(0),
                    PageTableIndex::new(i as u16),
                );

                // Получаем физический адрес и флаги
                if let Ok(translation) =
                    unsafe { crate::paging::get_page_mapper() }.translate_page(page)
                {
                    let phys_addr = translation.start_address();

                    // Помечаем страницу как разделяемую
                    cow_manager.mark_page_shared(phys_addr);

                    // Копируем запись в таблице страниц
                    // В реальной системе нужно будет использовать map_to с
                    // флагами COW dest_page_table.
                    // entries[i] = source_page_table.entries[i];
                }
            }
        }

        Ok(())
    }

    /// Разделяет виртуальную память для процесса
    pub fn separate_vm_for_process(_virt_addr: VirtAddr) -> Result<(), &'static str> {
        // В реальной системе тут будет создание копии для процесса
        // при первом обращении к разделяемой странице
        Ok(())
    }
}

// Интерфейс для процессного менеджера
pub struct ProcessCowInterface;

impl ProcessCowInterface {
    /// Подготовка COW для fork()
    pub fn prepare_cow_for_fork() -> Result<(), &'static str> {
        // В момент fork() пометить все страницы текущего процесса как разделяемые
        // Это будет реализовано в процессном менеджере
        Ok(())
    }

    /// Управление разделяемыми страницами
    pub fn manage_shared_pages(_child_process_id: u64) -> Result<(), &'static str> {
        // Когда создается дочерний процесс, управлять разделяемыми страницами
        Ok(())
    }
}
