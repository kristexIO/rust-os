use crate::memory::{EmptyFrameAllocator, FrameAllocator};
use alloc::{collections::BTreeMap, vec::Vec};
use spin::Mutex;
use x86_64::{
    structures::paging::{Mapper, OffsetPageTable, Page, PageTableFlags, PhysFrame, Size4KiB},
    PhysAddr, VirtAddr,
};

/// MemoryMapper - система для отображения физической памяти в виртуальную
///
/// Позволяет маппировать области физической памяти (например, MMIO) в виртуальную адресацию
pub struct MemoryMapper {
    /// Карта отображений физической памяти к виртуальной
    mappings: BTreeMap<PhysAddr, MemoryMapping>,
    /// Карта виртуальных адресов к физическим
    reverse_mappings: BTreeMap<VirtAddr, PhysAddr>,
    /// Дополнительные флаги для отображений
    mapping_flags: BTreeMap<VirtAddr, PageTableFlags>,
}

/// Информация о конкретном отображении
#[derive(Debug, Clone)]
pub struct MemoryMapping {
    /// Виртуальный адрес начала отображения
    pub virt_addr: VirtAddr,
    /// Физический адрес начала отображения
    pub phys_addr: PhysAddr,
    /// Размер отображения
    pub size: usize,
    /// Флаги отображения
    pub flags: PageTableFlags,
    /// Тип отображения
    pub mapping_type: MappingType,
    /// Владелец отображения (если применимо)
    pub owner_pid: Option<u64>,
    /// Время создания
    pub creation_time: u64,
}

/// Типы отображения
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MappingType {
    /// Обычное отображение памяти
    Regular,
    /// MMIO (Memory-Mapped I/O)
    Mmio,
    /// Framebuffer
    Framebuffer,
    /// Кэшируемая память
    Cached,
    /// Некэшируемая память
    Uncached,
    /// Write-combining (для видеокарт)
    WriteCombining,
    /// Device memory
    Device,
}

impl MemoryMapper {
    /// Создает новый маппер памяти
    pub fn new() -> Self {
        MemoryMapper {
            mappings: BTreeMap::new(),
            reverse_mappings: BTreeMap::new(),
            mapping_flags: BTreeMap::new(),
        }
    }

    /// Отображает физическую память в виртуальную
    pub fn map_memory_region(
        &mut self,
        virt_addr: VirtAddr,
        phys_addr: PhysAddr,
        size: usize,
        flags: PageTableFlags,
        mapping_type: MappingType,
        owner_pid: Option<u64>,
    ) -> Result<(), &'static str> {
        // Проверяем, что виртуальный адрес выровнен по странице
        if !virt_addr.is_aligned(4096) {
            return Err("Virtual address not page-aligned");
        }

        // Проверяем, что физический адрес выровнен по странице
        if !phys_addr.is_aligned(4096) {
            return Err("Physical address not page-aligned");
        }

        // Проверяем, что размер кратен размеру страницы
        if size % 4096 != 0 {
            return Err("Size not page-aligned");
        }

        // Проверяем, не пересекается ли новое отображение с существующими
        if self.overlaps_existing_mapping(virt_addr, size) {
            return Err("Mapping overlaps with existing mapping");
        }

        // Создаем отображения
        for offset in (0..size).step_by(4096) {
            let v_page = Page::<Size4KiB>::containing_address(virt_addr + offset as u64);
            let p_frame = PhysFrame::containing_address(phys_addr + offset as u64);

            // Добавляем информацию об отображении
            let mapping = MemoryMapping {
                virt_addr: v_page.start_address(),
                phys_addr: p_frame.start_address(),
                size: 4096,
                flags,
                mapping_type,
                owner_pid,
                creation_time: crate::timer::get_uptime_ms(),
            };

            self.mappings.insert(p_frame.start_address(), mapping);
            self.reverse_mappings
                .insert(v_page.start_address(), p_frame.start_address());
            self.mapping_flags.insert(v_page.start_address(), flags);
        }

        Ok(())
    }

    /// Отображает MMIO область
    pub fn map_mmio_region(
        &mut self,
        virt_addr: VirtAddr,
        phys_addr: PhysAddr,
        size: usize,
        permissions: MmioPermissions,
    ) -> Result<(), &'static str> {
        let mut flags = PageTableFlags::PRESENT;

        if permissions.readable {
            flags |= PageTableFlags::PRESENT;
        }

        if permissions.writable {
            flags |= PageTableFlags::WRITABLE;
        }

        // MMIO обычно не кэшируется
        flags |= PageTableFlags::NO_CACHE;

        self.map_memory_region(
            virt_addr,
            phys_addr,
            size,
            flags,
            MappingType::Mmio,
            None, // MMIO не имеет владельца процесса
        )
    }

    /// Отображает framebuffer
    pub fn map_framebuffer(
        &mut self,
        framebuffer_addr: PhysAddr,
        size: usize,
        width: u32,
        height: u32,
    ) -> Result<VirtAddr, &'static str> {
        // Выбираем виртуальный адрес для framebuffer
        let virt_addr = self
            .find_free_virtual_range(size)
            .ok_or("No free virtual range for framebuffer")?;

        // Framebuffer не кэшируемый и доступен для чтения/записи
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE;

        self.map_memory_region(
            virt_addr,
            framebuffer_addr,
            size,
            flags,
            MappingType::Framebuffer,
            None,
        )?;

        Ok(virt_addr)
    }

    /// Отображает устройство
    pub fn map_device(
        &mut self,
        virt_addr: VirtAddr,
        phys_addr: PhysAddr,
        size: usize,
        permissions: DevicePermissions,
    ) -> Result<(), &'static str> {
        let mut flags = PageTableFlags::PRESENT;

        if permissions.readable {
            flags |= PageTableFlags::PRESENT;
        }

        if permissions.writable {
            flags |= PageTableFlags::WRITABLE;
        }

        // Устройства обычно не кэшируются
        flags |= PageTableFlags::NO_CACHE;

        self.map_memory_region(virt_addr, phys_addr, size, flags, MappingType::Device, None)
    }

    /// Отменяет отображение памяти
    pub fn unmap_memory_region(
        &mut self,
        virt_addr: VirtAddr,
        size: usize,
    ) -> Result<(), &'static str> {
        // Проверяем, что отображение существует
        for offset in (0..size).step_by(4096) {
            let v_addr = virt_addr + offset as u64;
            if let Some(phys_addr) = self.reverse_mappings.remove(&v_addr) {
                self.mappings.remove(&phys_addr);
                self.mapping_flags.remove(&v_addr);
            }
        }

        Ok(())
    }

    /// Находит свободный диапазон виртуальной памяти
    fn find_free_virtual_range(&self, size: usize) -> Option<VirtAddr> {
        // Начинаем поиск с адреса 0x100000000 (4GB)
        let mut current_addr = VirtAddr::new(0x100000000);

        while current_addr.as_u64() < 0xFFFF800000000000 {
            // До начала адресного пространства ядра
            // Проверяем, занято ли место
            let end_addr = current_addr + size as u64;
            let mut overlaps = false;

            for (mapped_virt, _) in &self.reverse_mappings {
                if (current_addr < *mapped_virt + size as u64) && (end_addr > *mapped_virt) {
                    overlaps = true;
                    break;
                }
            }

            if !overlaps {
                return Some(current_addr);
            }

            current_addr += 0x100000; // Проверяем каждые 1MB
        }

        None
    }

    /// Проверяет, пересекается ли новое отображение с существующими
    fn overlaps_existing_mapping(&self, virt_addr: VirtAddr, size: usize) -> bool {
        let end_addr = virt_addr + size as u64;

        for (mapped_virt, phys_addr) in &self.reverse_mappings {
            let mapped_end = *mapped_virt + 4096; // Каждое отображение минимум одна страница

            if (virt_addr < *mapped_virt + 4096) && (end_addr > *mapped_virt) {
                return true;
            }
        }

        false
    }

    /// Получает информацию о отображении по виртуальному адресу
    pub fn get_mapping_info(&self, virt_addr: VirtAddr) -> Option<&MemoryMapping> {
        // Найдем страницу, содержащую адрес
        let page_addr = VirtAddr::new(virt_addr.as_u64() & !0xFFF); // Выравнивание на страницу

        if let Some(phys_addr) = self.reverse_mappings.get(&page_addr) {
            self.mappings.get(phys_addr)
        } else {
            None
        }
    }

    /// Получает физический адрес по виртуальному
    pub fn virt_to_phys(&self, virt_addr: VirtAddr) -> Option<PhysAddr> {
        let page_addr = VirtAddr::new(virt_addr.as_u64() & !0xFFF);
        self.reverse_mappings.get(&page_addr).copied()
    }

    /// Получает виртуальный адрес по физическому
    pub fn phys_to_virt(&self, phys_addr: PhysAddr) -> Option<VirtAddr> {
        let frame_addr = PhysAddr::new(phys_addr.as_u64() & !0xFFF);
        if let Some(mapping) = self.mappings.get(&frame_addr) {
            Some(mapping.virt_addr)
        } else {
            None
        }
    }

    /// Проверяет, может ли процесс получить доступ к памяти
    pub fn can_process_access(&self, pid: u64, virt_addr: VirtAddr) -> bool {
        if let Some(mapping) = self.get_mapping_info(virt_addr) {
            // Владелец всегда может получить доступ
            if let Some(owner) = mapping.owner_pid {
                return owner == pid;
            }

            // Для MMIO, Framebuffer и Device mappings доступ ограничен
            match mapping.mapping_type {
                MappingType::Mmio | MappingType::Framebuffer | MappingType::Device => false,
                _ => true,
            }
        } else {
            // Если нет отображения, доступ запрещен
            false
        }
    }

    /// Получает список всех отображений
    pub fn get_all_mappings(&self) -> Vec<&MemoryMapping> {
        self.mappings.values().collect()
    }

    /// Получает отображения для конкретного процесса
    pub fn get_process_mappings(&self, pid: u64) -> Vec<&MemoryMapping> {
        self.mappings
            .values()
            .filter(|mapping| mapping.owner_pid == Some(pid))
            .collect()
    }

    /// Проверяет, содержит ли отображение определенный тип
    pub fn has_mapping_type(&self, mapping_type: MappingType) -> bool {
        self.mappings
            .values()
            .any(|mapping| mapping.mapping_type == mapping_type)
    }

    /// Получает количество отображений определенного типа
    pub fn count_mappings_by_type(&self, mapping_type: MappingType) -> usize {
        self.mappings
            .values()
            .filter(|mapping| mapping.mapping_type == mapping_type)
            .count()
    }

    /// Обновляет флаги отображения
    pub fn update_mapping_flags(
        &mut self,
        virt_addr: VirtAddr,
        new_flags: PageTableFlags,
    ) -> Result<(), &'static str> {
        let page_addr = VirtAddr::new(virt_addr.as_u64() & !0xFFF);

        if let Some(phys_addr) = self.reverse_mappings.get(&page_addr).copied() {
            if let Some(mapping) = self.mappings.get_mut(&phys_addr) {
                mapping.flags = new_flags;
                *self.mapping_flags.get_mut(&page_addr).unwrap() = new_flags;
                Ok(())
            } else {
                Err("Mapping not found")
            }
        } else {
            Err("Virtual address not mapped")
        }
    }

    /// Возвращает список MMIO отображений
    pub fn get_mmio_mappings(&self) -> Vec<&MemoryMapping> {
        self.mappings
            .values()
            .filter(|mapping| mapping.mapping_type == MappingType::Mmio)
            .collect()
    }

    /// Возвращает список framebuffer отображений
    pub fn get_framebuffer_mappings(&self) -> Vec<&MemoryMapping> {
        self.mappings
            .values()
            .filter(|mapping| mapping.mapping_type == MappingType::Framebuffer)
            .collect()
    }

    /// Возвращает список device отображений
    pub fn get_device_mappings(&self) -> Vec<&MemoryMapping> {
        self.mappings
            .values()
            .filter(|mapping| mapping.mapping_type == MappingType::Device)
            .collect()
    }
}

/// Права доступа для MMIO
#[derive(Debug, Clone, Copy)]
pub struct MmioPermissions {
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
}

impl MmioPermissions {
    pub fn read_only() -> Self {
        MmioPermissions {
            readable: true,
            writable: false,
            executable: false,
        }
    }

    pub fn read_write() -> Self {
        MmioPermissions {
            readable: true,
            writable: true,
            executable: false,
        }
    }

    pub fn full_access() -> Self {
        MmioPermissions {
            readable: true,
            writable: true,
            executable: true,
        }
    }
}

/// Права доступа для устройств
#[derive(Debug, Clone, Copy)]
pub struct DevicePermissions {
    pub readable: bool,
    pub writable: bool,
    pub privileged: bool, // Только для привилегированного кода
}

impl DevicePermissions {
    pub fn user_read_only() -> Self {
        DevicePermissions {
            readable: true,
            writable: false,
            privileged: false,
        }
    }

    pub fn user_read_write() -> Self {
        DevicePermissions {
            readable: true,
            writable: true,
            privileged: false,
        }
    }

    pub fn privileged_read_write() -> Self {
        DevicePermissions {
            readable: true,
            writable: true,
            privileged: true,
        }
    }
}

/// Виртуальный MMIO маппер для эмуляции
pub struct VirtualMmioMapper {
    /// Фейковые MMIO отображения для тестирования
    virtual_mappings: BTreeMap<PhysAddr, Vec<u8>>,
}

impl VirtualMmioMapper {
    pub fn new() -> Self {
        VirtualMmioMapper {
            virtual_mappings: BTreeMap::new(),
        }
    }

    /// Создает виртуальное MMIO отображение
    pub fn create_virtual_mmio_mapping(&mut self, phys_addr: PhysAddr, size: usize) {
        let mut buffer = Vec::with_capacity(size);
        buffer.resize(size, 0);
        self.virtual_mappings.insert(phys_addr, buffer);
    }

    /// Читает из виртуального MMIO
    pub fn read_mmio(
        &self,
        phys_addr: PhysAddr,
        offset: usize,
        length: usize,
    ) -> Result<Vec<u8>, &'static str> {
        if let Some(buffer) = self.virtual_mappings.get(&phys_addr) {
            if offset + length <= buffer.len() {
                Ok(buffer[offset..offset + length].to_vec())
            } else {
                Err("Read beyond buffer boundary")
            }
        } else {
            Err("MMIO region not found")
        }
    }

    /// Пишет в виртуальное MMIO
    pub fn write_mmio(
        &mut self,
        phys_addr: PhysAddr,
        offset: usize,
        data: &[u8],
    ) -> Result<(), &'static str> {
        if let Some(buffer) = self.virtual_mappings.get_mut(&phys_addr) {
            if offset + data.len() <= buffer.len() {
                buffer[offset..offset + data.len()].copy_from_slice(data);
                Ok(())
            } else {
                Err("Write beyond buffer boundary")
            }
        } else {
            Err("MMIO region not found")
        }
    }

    /// Проверяет, существует ли виртуальное MMIO отображение
    pub fn has_virtual_mapping(&self, phys_addr: PhysAddr) -> bool {
        self.virtual_mappings.contains_key(&phys_addr)
    }
}

/// Интерфейс для системных вызовов
pub struct MemoryMappingInterface;

impl MemoryMappingInterface {
    /// Отображает память для процесса
    pub fn map_memory_for_process(
        virt_addr: VirtAddr,
        phys_addr: PhysAddr,
        size: usize,
        pid: u64,
    ) -> Result<(), &'static str> {
        let mut mapper = get_memory_mapper().lock();
        mapper.map_memory_region(
            virt_addr,
            phys_addr,
            size,
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
            MappingType::Regular,
            Some(pid),
        )
    }

    /// Отображает MMIO для драйвера
    pub fn map_mmio_for_driver(
        virt_addr: VirtAddr,
        phys_addr: PhysAddr,
        size: usize,
        permissions: MmioPermissions,
    ) -> Result<(), &'static str> {
        let mut mapper = get_memory_mapper().lock();
        mapper.map_mmio_region(virt_addr, phys_addr, size, permissions)
    }

    /// Отображает framebuffer
    pub fn map_framebuffer_region(
        framebuffer_addr: PhysAddr,
        size: usize,
        width: u32,
        height: u32,
    ) -> Result<VirtAddr, &'static str> {
        let mut mapper = get_memory_mapper().lock();
        mapper.map_framebuffer(framebuffer_addr, size, width, height)
    }

    /// Отменяет отображение памяти
    pub fn unmap_memory_region(virt_addr: VirtAddr, size: usize) -> Result<(), &'static str> {
        let mut mapper = get_memory_mapper().lock();
        mapper.unmap_memory_region(virt_addr, size)
    }

    /// Получает информацию об отображении
    pub fn get_mapping_info(virt_addr: VirtAddr) -> Option<MemoryMapping> {
        let mapper = get_memory_mapper().lock();
        mapper.get_mapping_info(virt_addr).cloned()
    }

    /// Проверяет права доступа процесса
    pub fn check_process_access(pid: u64, virt_addr: VirtAddr) -> bool {
        let mapper = get_memory_mapper().lock();
        mapper.can_process_access(pid, virt_addr)
    }

    /// Получает физический адрес по виртуальному
    pub fn virtual_to_physical(virt_addr: VirtAddr) -> Option<PhysAddr> {
        let mapper = get_memory_mapper().lock();
        mapper.virt_to_phys(virt_addr)
    }

    /// Получает виртуальный адрес по физическому
    pub fn physical_to_virtual(phys_addr: PhysAddr) -> Option<VirtAddr> {
        let mapper = get_memory_mapper().lock();
        mapper.phys_to_virt(phys_addr)
    }
}

// Глобальный маппер
use spin::Once;

static MEMORY_MAPPER: Once<Mutex<MemoryMapper>> = Once::new();

pub fn get_memory_mapper() -> &'static Mutex<MemoryMapper> {
    MEMORY_MAPPER.call_once(|| Mutex::new(MemoryMapper::new()))
}

/// Макросы для удобства работы с отображениями
#[macro_export]
macro_rules! map_memory_region {
    ($virt_addr:expr, $phys_addr:expr, $size:expr, $flags:expr) => {
        $crate::memory_mapping::MemoryMappingInterface::map_memory_for_process(
            $virt_addr,
            $phys_addr,
            $size,
            $crate::scheduler::get_current_process_id().unwrap_or(0),
        )
    };
    ($virt_addr:expr, $phys_addr:expr, $size:expr, $flags:expr, $pid:expr) => {
        $crate::memory_mapping::MemoryMappingInterface::map_memory_for_process(
            $virt_addr, $phys_addr, $size, $pid,
        )
    };
}

#[macro_export]
macro_rules! unmap_memory_region {
    ($virt_addr:expr, $size:expr) => {
        $crate::memory_mapping::MemoryMappingInterface::unmap_memory_region($virt_addr, $size)
    };
}

#[macro_export]
macro_rules! map_mmio_region {
    ($virt_addr:expr, $phys_addr:expr, $size:expr, $permissions:expr) => {
        $crate::memory_mapping::MemoryMappingInterface::map_mmio_for_driver(
            $virt_addr,
            $phys_addr,
            $size,
            $permissions,
        )
    };
}

#[macro_export]
macro_rules! map_framebuffer {
    ($phys_addr:expr, $size:expr, $width:expr, $height:expr) => {
        $crate::memory_mapping::MemoryMappingInterface::map_framebuffer_region(
            $phys_addr, $size, $width, $height,
        )
    };
}

#[macro_export]
macro_rules! virt_to_phys {
    ($virt_addr:expr) => {
        $crate::memory_mapping::MemoryMappingInterface::virtual_to_physical($virt_addr)
    };
}

#[macro_export]
macro_rules! phys_to_virt {
    ($phys_addr:expr) => {
        $crate::memory_mapping::MemoryMappingInterface::physical_to_virtual($phys_addr)
    };
}

#[macro_export]
macro_rules! check_access {
    ($pid:expr, $virt_addr:expr) => {
        $crate::memory_mapping::MemoryMappingInterface::check_process_access($pid, $virt_addr)
    };
}

/// Функции для работы с пейджингом
pub struct PagingHelpers;

impl PagingHelpers {
    /// Настраивает отображение для пейджинга
    pub fn setup_page_table_mapping(
        mapper: &mut OffsetPageTable,
        frame_allocator: &mut impl FrameAllocator<Size4KiB>,
        virt_addr: VirtAddr,
        phys_addr: PhysAddr,
        size: usize,
        flags: PageTableFlags,
    ) -> Result<(), &'static str> {
        for offset in (0..size).step_by(4096) {
            let page = Page::containing_address(virt_addr + offset as u64);
            let frame = PhysFrame::containing_address(phys_addr + offset as u64);

            unsafe {
                mapper
                    .map_to(page, frame, flags, frame_allocator)
                    .map_err(|_| "Failed to map page")?
                    .flush();
            }
        }

        Ok(())
    }

    /// Создает отображение для нового процесса
    pub fn create_user_process_mapping(
        pid: u64,
        virt_addr: VirtAddr,
        size: usize,
    ) -> Result<VirtAddr, &'static str> {
        // Найдем физический кадр для процесса
        let phys_addr =
            crate::memory::allocate_frames(size / 4096 + if size % 4096 != 0 { 1 } else { 0 })
                .ok_or("Cannot allocate physical frames")?;

        // Создаем отображение в адресном пространстве процесса
        MemoryMappingInterface::map_memory_for_process(virt_addr, phys_addr, size, pid)?;

        Ok(virt_addr)
    }

    /// Удаляет отображение для процесса
    pub fn remove_user_process_mapping(
        pid: u64,
        virt_addr: VirtAddr,
        size: usize,
    ) -> Result<(), &'static str> {
        // Проверяем, что процесс действительно владеет этим отображением
        if !check_access!(pid, virt_addr) {
            return Err("Process does not own this mapping");
        }

        MemoryMappingInterface::unmap_memory_region(virt_addr, size)
    }
}

/// Статистика отображений
pub struct MappingStats {
    pub total_mappings: usize,
    pub mmio_mappings: usize,
    pub framebuffer_mappings: usize,
    pub device_mappings: usize,
    pub regular_mappings: usize,
}

impl MemoryMapper {
    /// Получает статистику отображений
    pub fn get_stats(&self) -> MappingStats {
        MappingStats {
            total_mappings: self.mappings.len(),
            mmio_mappings: self.count_mappings_by_type(MappingType::Mmio),
            framebuffer_mappings: self.count_mappings_by_type(MappingType::Framebuffer),
            device_mappings: self.count_mappings_by_type(MappingType::Device),
            regular_mappings: self.count_mappings_by_type(MappingType::Regular),
        }
    }
}

// Инициализация системы отображения
pub fn init_memory_mapping_system() {
    let _ = get_memory_mapper(); // Инициализируем однажды
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_mapper_creation() {
        let mapper = MemoryMapper::new();
        assert_eq!(mapper.get_all_mappings().len(), 0);
    }

    #[test]
    fn test_simple_mapping() {
        let mut mapper = MemoryMapper::new();

        let virt_addr = VirtAddr::new(0x200000);
        let phys_addr = PhysAddr::new(0x300000);
        let size = 4096;

        let result = mapper.map_memory_region(
            virt_addr,
            phys_addr,
            size,
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
            MappingType::Regular,
            Some(123),
        );

        assert!(result.is_ok());

        // Проверяем, что отображение существует
        let mapping_info = mapper.get_mapping_info(virt_addr);
        assert!(mapping_info.is_some());
        assert_eq!(mapping_info.unwrap().owner_pid, Some(123));
    }

    #[test]
    fn test_virt_to_phys_conversion() {
        let mut mapper = MemoryMapper::new();

        let virt_addr = VirtAddr::new(0x400000);
        let phys_addr = PhysAddr::new(0x500000);
        let size = 8192; // 2 страницы

        mapper
            .map_memory_region(
                virt_addr,
                phys_addr,
                size,
                PageTableFlags::PRESENT,
                MappingType::Regular,
                None,
            )
            .unwrap();

        // Проверяем преобразование
        let converted_phys = mapper.virt_to_phys(virt_addr);
        assert_eq!(converted_phys, Some(phys_addr));

        // Проверяем для второй страницы
        let virt_addr2 = virt_addr + 4096;
        let converted_phys2 = mapper.virt_to_phys(virt_addr2);
        assert_eq!(converted_phys2, Some(phys_addr + 4096));
    }

    #[test]
    fn test_process_access_control() {
        let mut mapper = MemoryMapper::new();

        let virt_addr = VirtAddr::new(0x600000);
        let phys_addr = PhysAddr::new(0x700000);
        let size = 4096;

        mapper
            .map_memory_region(
                virt_addr,
                phys_addr,
                size,
                PageTableFlags::PRESENT,
                MappingType::Regular,
                Some(456),
            )
            .unwrap();

        // Проверяем права доступа
        assert_eq!(mapper.can_process_access(456, virt_addr), true);
        assert_eq!(mapper.can_process_access(789, virt_addr), false);
    }

    #[test]
    fn test_mmio_mapping() {
        let mut mapper = MemoryMapper::new();

        let virt_addr = VirtAddr::new(0x800000);
        let phys_addr = PhysAddr::new(0x900000);
        let size = 4096;

        let permissions = MmioPermissions::read_write();

        let result = mapper.map_mmio_region(virt_addr, phys_addr, size, permissions);
        assert!(result.is_ok());

        // Проверяем тип отображения
        let mapping_info = mapper.get_mapping_info(virt_addr).unwrap();
        assert_eq!(mapping_info.mapping_type, MappingType::Mmio);
    }
}
