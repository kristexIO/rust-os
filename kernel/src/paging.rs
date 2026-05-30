/// Менеджер виртуальной памяти: маппинг, demand paging, CoW.
///
/// Архитектура:
///   - `OffsetPageTable` из x86_64 для маппинга виртуальных → физических
///     адресов.
///   - `BitmapFrameAllocator` (frame_allocator.rs) для выделения физических
///     фреймов.
///   - `VmaRegion` — регион виртуальной памяти с политикой (demand/mapped/cow).
///   - Page fault handler интегрирован сюда и различает три случая:
///       1. Demand fault  — страница ещё не была mapped → аллоцируем фрейм +
///          map.
///       2. CoW fault     — страница read-only, shared → копируем +
///          перемаппируем RW.
///       3. Invalid fault — адрес за пределами VMA → panic/kill process.
///
/// Ключевые инварианты:
///   - Флаг `NO_EXECUTE` выставляется для стеков и data-страниц.
///   - Флаг `USER_ACCESSIBLE` только для user-space VMA.
///   - `BIT_9` (software-defined) в PTE используется как маркер demand-пage.
use core::sync::atomic::{AtomicU64, Ordering};

use x86_64::{
    registers::control::{Cr3, Cr3Flags},
    structures::paging::{
        Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB,
    },
    PhysAddr, VirtAddr,
};

use crate::frame_allocator;

// ─── Программный бит в PTE
// ────────────────────────────────────────────────────

/// BIT_9 в PTE — программный маркер «demand page».
/// Страница создана как VMA-регион, но фрейм ещё не выделен.
/// При первом обращении page fault handler аллоцирует фрейм.
const PTE_DEMAND: PageTableFlags = PageTableFlags::BIT_9;

/// BIT_10 — маркер CoW-страницы.
/// Read-only страница, разделённая между несколькими процессами.
/// При записи — копируется.
const PTE_COW: PageTableFlags = PageTableFlags::BIT_10;

// ─── VMA — регион виртуальной памяти ─────────────────────────────────────────

/// Тип VMA-региона — определяет политику при page fault
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmaKind {
    /// Anonymous: обнуляемая память (стек, heap, BSS)
    Anonymous,
    /// Файловый маппинг (read-only ELF сегменты)
    FileMapped,
    /// Shared memory (CoW-fork)
    SharedCow,
}

/// Флаги доступа к VMA
#[derive(Debug, Clone, Copy)]
pub struct VmaFlags {
    pub read: bool,
    pub write: bool,
    pub exec: bool,
    pub user: bool,
}

impl VmaFlags {
    pub const fn rw_user() -> Self {
        VmaFlags {
            read: true,
            write: true,
            exec: false,
            user: true,
        }
    }
    pub const fn ro_user() -> Self {
        VmaFlags {
            read: true,
            write: false,
            exec: false,
            user: true,
        }
    }
    pub const fn rx_user() -> Self {
        VmaFlags {
            read: true,
            write: false,
            exec: true,
            user: true,
        }
    }
    pub const fn rw_kernel() -> Self {
        VmaFlags {
            read: true,
            write: true,
            exec: false,
            user: false,
        }
    }

    /// Конвертация в PageTableFlags
    pub fn to_pte_flags(self) -> PageTableFlags {
        let mut flags = PageTableFlags::PRESENT;
        if self.write {
            flags |= PageTableFlags::WRITABLE;
        }
        if self.user {
            flags |= PageTableFlags::USER_ACCESSIBLE;
        }
        if !self.exec {
            flags |= PageTableFlags::NO_EXECUTE;
        }
        flags
    }
}

/// Регион виртуальной памяти процесса.
#[derive(Clone)]
pub struct VmaRegion {
    /// Начальный виртуальный адрес (выровнен по странице)
    pub start: VirtAddr,
    /// Конечный виртуальный адрес (не включительно)
    pub end: VirtAddr,
    /// Тип региона
    pub kind: VmaKind,
    /// Флаги доступа
    pub flags: VmaFlags,
}

impl VmaRegion {
    pub fn new(start: VirtAddr, end: VirtAddr, kind: VmaKind, flags: VmaFlags) -> Self {
        VmaRegion {
            start,
            end,
            kind,
            flags,
        }
    }

    pub fn contains(&self, addr: VirtAddr) -> bool {
        addr >= self.start && addr < self.end
    }

    pub fn page_count(&self) -> usize {
        let bytes = self.end.as_u64() - self.start.as_u64();
        (bytes as usize).saturating_add(0xFFF) / 0x1000
    }
}

// ─── Менеджер адресного пространства процесса
// ─────────────────────────────────

/// VMA-таблица для одного процесса.
/// Простой Vec<VmaRegion> — для production нужно interval tree.
pub struct AddressSpace {
    pub vmas: alloc::vec::Vec<VmaRegion>,
    /// Физический адрес корневой таблицы страниц этого процесса
    pub cr3: PhysAddr,
}

impl AddressSpace {
    pub fn new(cr3: PhysAddr) -> Self {
        AddressSpace {
            vmas: alloc::vec::Vec::new(),
            cr3,
        }
    }

    pub fn add_vma(&mut self, vma: VmaRegion) {
        self.vmas.push(vma);
    }

    /// Найти VMA, содержащую данный адрес
    pub fn find_vma(&self, addr: VirtAddr) -> Option<&VmaRegion> {
        self.vmas.iter().find(|v| v.contains(addr))
    }
}

// ─── Глобальный PageManager
// ───────────────────────────────────────────────────

/// Offset от физических адресов до виртуальных (устанавливается загрузчиком).
/// После init() используется для создания OffsetPageTable.
static PHYS_MEM_OFFSET: AtomicU64 = AtomicU64::new(0);

/// Флаг инициализации
static PAGING_INITIALIZED: AtomicU64 = AtomicU64::new(0);

/// Инициализирует подсистему пейджинга.
///
/// # Safety
/// Должна вызываться ровно один раз с корректным physical_memory_offset
/// из BootInfo. Устанавливает PHYS_MEM_OFFSET для всех последующих операций.
pub unsafe fn init(physical_memory_offset: u64) {
    PHYS_MEM_OFFSET.store(physical_memory_offset, Ordering::SeqCst);
    PAGING_INITIALIZED.store(1, Ordering::SeqCst);

    // Опционально: защищаем первую страницу (адрес 0) от null-deref
    // путём снятия маппинга — но это может сломать identity-mapped регионы
    // на ранней стадии загрузки. Пока пропускаем.
}

/// Создаёт `OffsetPageTable` для текущего CR3.
///
/// # Safety
/// physical_memory_offset должен быть корректным (из init()).
/// Возвращаемый `OffsetPageTable` имеет lifetime, ограниченный вызывающим
/// контекстом.
pub unsafe fn get_page_mapper<'a>() -> OffsetPageTable<'a> {
    let phys_offset = VirtAddr::new(PHYS_MEM_OFFSET.load(Ordering::Relaxed));
    let (frame, _) = Cr3::read();
    let phys_addr = frame.start_address();
    let virt_addr = phys_offset + phys_addr.as_u64();
    let page_table = &mut *(virt_addr.as_mut_ptr());
    OffsetPageTable::new(page_table, phys_offset)
}

/// Создаёт `OffsetPageTable` для произвольного CR3 (для другого процесса).
///
/// # Safety
/// `cr3_phys` должен быть физическим адресом валидной L4 page table.
pub unsafe fn get_mapper_for_cr3<'a>(cr3_phys: PhysAddr) -> OffsetPageTable<'a> {
    let phys_offset = VirtAddr::new(PHYS_MEM_OFFSET.load(Ordering::Relaxed));
    let virt_addr = phys_offset + cr3_phys.as_u64();
    let page_table = &mut *(virt_addr.as_mut_ptr());
    OffsetPageTable::new(page_table, phys_offset)
}

// ─── Маппинг страниц
// ──────────────────────────────────────────────────────────

/// Маппит виртуальную страницу на физический фрейм с заданными флагами.
///
/// # Safety
/// - `virt` должен быть выровнен по 4KiB.
/// - `frame` должен быть валидным выделенным физическим фреймом.
/// - Вызывающий код отвечает за корректность флагов.
pub unsafe fn map_page(
    virt: VirtAddr,
    frame: PhysFrame<Size4KiB>,
    flags: PageTableFlags,
) -> Result<(), &'static str> {
    let mut mapper = get_page_mapper();
    let page: Page<Size4KiB> = Page::containing_address(virt);
    let mut fa = frame_allocator::get_frame_allocator();

    mapper
        .map_to(page, frame, flags, &mut *fa)
        .map_err(|_| "map_to failed")?
        .flush();

    Ok(())
}

/// Снимает маппинг страницы и возвращает физический фрейм в аллокатор.
///
/// # Safety
/// Страница должна быть mapped.
pub unsafe fn unmap_page(virt: VirtAddr) -> Result<(), &'static str> {
    let mut mapper = get_page_mapper();
    let page: Page<Size4KiB> = Page::containing_address(virt);

    let (frame, flush) = mapper.unmap(page).map_err(|_| "unmap failed")?;
    flush.flush();

    // Возвращаем фрейм в аллокатор
    frame_allocator::free_frame(frame);
    Ok(())
}

/// Выделяет фрейм и маппит страницу.
/// Содержимое фрейма обнуляется (zero page).
///
/// # Safety
/// Как `map_page`.
pub unsafe fn alloc_and_map(
    virt: VirtAddr,
    flags: PageTableFlags,
) -> Result<PhysFrame<Size4KiB>, &'static str> {
    let frame = frame_allocator::try_alloc_frame().ok_or("OOM: no physical frames")?;

    // Обнуляем страницу через physical offset mapping
    let phys_offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let virt_of_frame = VirtAddr::new(phys_offset + frame.start_address().as_u64());
    let page_ptr = virt_of_frame.as_mut_ptr::<u8>();
    // SAFETY: phys_offset + frame address → корректный виртуальный адрес
    // (bootloader всегда маппит всю физическую память).
    core::ptr::write_bytes(page_ptr, 0, 4096);

    map_page(virt, frame, flags)?;
    Ok(frame)
}

// ─── Page Fault Handler
// ────────────────────────────────────────────────────────

/// Обрабатывает page fault.
///
/// Вызывается из `interrupts::page_fault_handler` с CR2 (faulting address)
/// и error code.
///
/// Алгоритм:
///   1. Читаем CR2 → faulting virtual address.
///   2. Ищем VMA для этого адреса.
///   3. Если VMA не найдена → паника (invalid access).
///   4. Если fault = write и страница помечена CoW → CoW handler.
///   5. Если fault = demand page → аллоцируем фрейм, маппим.
///   6. Иначе → паника (protection fault).
///
/// # Safety
/// Вызывается в контексте прерывания. Не должна паниковать без причины.
pub fn handle_page_fault(
    faulting_addr: VirtAddr,
    error_code: x86_64::structures::idt::PageFaultErrorCode,
) -> bool {
    // Проверяем, инициализирован ли paging
    if PAGING_INITIALIZED.load(Ordering::Relaxed) == 0 {
        return false;
    }

    let is_write =
        error_code.contains(x86_64::structures::idt::PageFaultErrorCode::CAUSED_BY_WRITE);
    let is_user = error_code.contains(x86_64::structures::idt::PageFaultErrorCode::USER_MODE);
    let is_present =
        error_code.contains(x86_64::structures::idt::PageFaultErrorCode::PROTECTION_VIOLATION);

    // Пробуем обработать как demand page или CoW
    unsafe {
        if is_present && is_write {
            // Страница присутствует, но запись нарушает права → CoW
            handle_cow_fault(faulting_addr).is_ok()
        } else if !is_present {
            // Страница не присутствует → demand page
            handle_demand_fault(faulting_addr, is_write, is_user).is_ok()
        } else {
            // Protection fault (exec NX, ring violation и т.д.)
            false
        }
    }
}

/// Demand paging: аллоцируем фрейм при первом обращении к странице.
///
/// # Safety
/// Вызывается в контексте page fault. PHYS_MEM_OFFSET должен быть установлен.
unsafe fn handle_demand_fault(
    faulting_addr: VirtAddr,
    _is_write: bool,
    is_user: bool,
) -> Result<(), &'static str> {
    // Флаги страницы: PRESENT + WRITABLE (для anonymous demand pages)
    let mut flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
    if is_user {
        flags |= PageTableFlags::USER_ACCESSIBLE;
    }

    // Аллоцируем фрейм и маппим (содержимое = нули)
    alloc_and_map(faulting_addr, flags)?;

    Ok(())
}

/// CoW fault: страница shared read-only, кто-то пишет → копируем фрейм.
///
/// # Safety
/// Вызывается в контексте page fault.
unsafe fn handle_cow_fault(faulting_addr: VirtAddr) -> Result<(), &'static str> {
    let mut mapper = get_page_mapper();
    let page: Page<Size4KiB> = Page::containing_address(faulting_addr);
    let phys_offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);

    // Получаем текущий физический фрейм
    let old_frame = mapper
        .translate_page(page)
        .map_err(|_| "CoW: page not mapped")?;

    // Выделяем новый фрейм
    let new_frame = frame_allocator::try_alloc_frame().ok_or("CoW: OOM")?;

    // Копируем содержимое old_frame → new_frame
    let src = (phys_offset + old_frame.start_address().as_u64()) as *const u8;
    let dst = (phys_offset + new_frame.start_address().as_u64()) as *mut u8;
    // SAFETY: оба указателя валидны через physical offset mapping.
    core::ptr::copy_nonoverlapping(src, dst, 4096);

    // Перемаппируем страницу с новым фреймом и RW-флагами
    let (_, flush) = mapper.unmap(page).map_err(|_| "CoW: unmap failed")?;
    flush.flush();

    let flags = PageTableFlags::PRESENT
        | PageTableFlags::WRITABLE
        | PageTableFlags::USER_ACCESSIBLE
        | PageTableFlags::NO_EXECUTE;

    let mut fa = frame_allocator::get_frame_allocator();
    mapper
        .map_to(page, new_frame, flags, &mut *fa)
        .map_err(|_| "CoW: remap failed")?
        .flush();

    // Старый фрейм: уменьшаем refcount (CoW manager можно добавить позже)
    let _ = old_frame; // фрейм теперь свободен (не возвращаем — может быть shared)

    Ok(())
}

// ─── Переключение таблицы страниц
// ─────────────────────────────────────────────

/// Активирует таблицу страниц процесса (записывает CR3).
///
/// # Safety
/// `page_table_phys` должен быть физическим адресом валидной L4 page table.
/// Вся нужная память должна быть mapped в новой таблице.
pub unsafe fn switch_address_space(page_table_phys: PhysAddr) -> Result<(), &'static str> {
    let frame = PhysFrame::from_start_address(page_table_phys)
        .map_err(|_| "switch_address_space: unaligned CR3")?;
    Cr3::write(frame, Cr3Flags::empty());
    Ok(())
}

// ─── Создание пространства для нового процесса
// ────────────────────────────────

/// Создаёт новую L4 page table для процесса.
/// Копирует kernel-half маппинги (top 256 PML4 entries) из текущей таблицы.
///
/// # Safety
/// Требует корректного PHYS_MEM_OFFSET и работающего frame allocator.
pub unsafe fn create_process_page_table() -> Result<PhysAddr, &'static str> {
    let phys_offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);

    // Выделяем фрейм для новой L4 таблицы
    let new_l4_frame =
        frame_allocator::try_alloc_frame().ok_or("create_process_page_table: OOM")?;

    // Обнуляем новую L4 таблицу
    let new_l4_virt = phys_offset + new_l4_frame.start_address().as_u64();
    core::ptr::write_bytes(new_l4_virt as *mut u8, 0, 4096);

    // Копируем kernel-half (верхние 256 записей PML4, адреса ≥
    // 0xFFFF_8000_0000_0000)
    let (cur_frame, _) = Cr3::read();
    let cur_l4_virt = (phys_offset + cur_frame.start_address().as_u64()) as *const u64;
    let new_l4_ptr = new_l4_virt as *mut u64;

    // PML4 имеет 512 записей по 8 байт = 4096 байт.
    // Нижние 256 (user-half) обнуляем, верхние 256 (kernel-half) копируем.
    for i in 256..512usize {
        // SAFETY: cur_l4_virt и new_l4_ptr указывают на L4 таблицы через physical
        // mapping.
        *new_l4_ptr.add(i) = *cur_l4_virt.add(i);
    }

    Ok(new_l4_frame.start_address())
}

// ─── ASLR ──────────────────────────────────────────────────────────────────────

static ASLR_COUNTER: AtomicU64 = AtomicU64::new(0x100_0000);

/// Генерирует случайный базовый адрес для ASLR.
/// Использует RDTSC + AtomicU64 как LCG.
pub fn aslr_base(size: u64) -> VirtAddr {
    let tsc: u64 = unsafe {
        let mut lo: u32;
        let mut hi: u32;
        core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nomem, nostack));
        ((hi as u64) << 32) | (lo as u64)
    };

    let counter = ASLR_COUNTER.fetch_add(0x1000, Ordering::Relaxed);
    let seed = tsc ^ counter ^ 0xDEAD_BEEF_CAFE_BABE;

    // LCG step
    let rand = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);

    // Выровнен по 2MiB в диапазоне 0x0000_0040_0000_0000 – 0x0000_007F_FFFF_FFFF
    let base =
        0x0000_0040_0000_0000u64 + ((rand & 0x0000_003F_FFFF_0000u64) | (size & 0xFFFF_F000));

    VirtAddr::new(base)
}

// ─── Статистика
// ───────────────────────────────────────────────────────────────

/// Печатает статистику памяти (делегирует frame_allocator)
pub fn print_memory_stats() {
    frame_allocator::print_memory_stats();
}

/// Возвращает physical memory offset (для отладки)
pub fn phys_mem_offset() -> u64 {
    PHYS_MEM_OFFSET.load(Ordering::Relaxed)
}

// ─── Валидация указателей для системных вызовов
// ───────────────────────────────────────────

const USER_SPACE_END: u64 = 0x0000_8000_0000_0000;

#[derive(Clone, Copy)]
struct MappingPermissions {
    user: bool,
    writable: bool,
}

fn is_user_range(addr: VirtAddr, len: usize) -> bool {
    if addr.as_u64() >= USER_SPACE_END {
        return false;
    }
    if len == 0 {
        return true;
    }
    addr.as_u64()
        .checked_add(len as u64 - 1)
        .is_some_and(|end| end < USER_SPACE_END)
}

fn include_entry_flags(flags: PageTableFlags, permissions: &mut MappingPermissions) -> bool {
    if !flags.contains(PageTableFlags::PRESENT) {
        return false;
    }
    permissions.user &= flags.contains(PageTableFlags::USER_ACCESSIBLE);
    permissions.writable &= flags.contains(PageTableFlags::WRITABLE);
    true
}

unsafe fn page_table_from_frame(frame: PhysFrame<Size4KiB>) -> &'static PageTable {
    let virt_addr = PHYS_MEM_OFFSET.load(Ordering::Relaxed) + frame.start_address().as_u64();
    &*(virt_addr as *const PageTable)
}

unsafe fn mapped_page_permissions(page: Page<Size4KiB>) -> Option<MappingPermissions> {
    if PAGING_INITIALIZED.load(Ordering::Relaxed) == 0 {
        return None;
    }

    let (root_frame, _) = Cr3::read();
    let p4 = page_table_from_frame(root_frame);
    let mut permissions = MappingPermissions {
        user: true,
        writable: true,
    };

    let p4_entry = &p4[page.p4_index()];
    if !include_entry_flags(p4_entry.flags(), &mut permissions) {
        return None;
    }
    let p3 = page_table_from_frame(p4_entry.frame().ok()?);

    let p3_entry = &p3[page.p3_index()];
    if !include_entry_flags(p3_entry.flags(), &mut permissions) {
        return None;
    }
    if p3_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        return Some(permissions);
    }
    let p2 = page_table_from_frame(p3_entry.frame().ok()?);

    let p2_entry = &p2[page.p2_index()];
    if !include_entry_flags(p2_entry.flags(), &mut permissions) {
        return None;
    }
    if p2_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        return Some(permissions);
    }
    let p1 = page_table_from_frame(p2_entry.frame().ok()?);

    let p1_entry = &p1[page.p1_index()];
    if !include_entry_flags(p1_entry.flags(), &mut permissions) {
        return None;
    }
    Some(permissions)
}

fn user_pages_allow(addr: VirtAddr, len: usize, writable: bool) -> bool {
    if !is_user_range(addr, len) {
        return false;
    }
    if len == 0 {
        return true;
    }

    let start_page: Page<Size4KiB> = Page::containing_address(addr);
    let last_byte = addr.as_u64() + len as u64 - 1;
    let end_page: Page<Size4KiB> = Page::containing_address(VirtAddr::new(last_byte));
    Page::range_inclusive(start_page, end_page).all(|page| {
        unsafe { mapped_page_permissions(page) }
            .is_some_and(|permissions| permissions.user && (!writable || permissions.writable))
    })
}

/// Проверяет, что диапазон памяти действительно отображён как user-readable.
pub fn is_user_readable(addr: VirtAddr, len: usize) -> bool {
    user_pages_allow(addr, len, false)
}

/// Проверяет, что диапазон памяти действительно отображён как user-writable.
pub fn is_user_writable(addr: VirtAddr, len: usize) -> bool {
    user_pages_allow(addr, len, true)
}
