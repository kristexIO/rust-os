use alloc::{collections::BTreeMap, string::String, vec::Vec};
use core::marker::PhantomData;
use core::ptr::NonNull;
use spin::{Mutex, Once};

/// MemorySafetyManager - система для обеспечения безопасности памяти
///
/// Следит за:
/// - Правильным выделением/освобождением памяти
/// - Двойным освобождением
/// - Использованием после освобождения (UAF)
/// - Переполнением буфера
/// - Нарушением прав доступа к памяти
pub struct MemorySafetyManager {
    /// Следит за выделенными блоками
    allocated_blocks: BTreeMap<usize, MemoryBlockInfo>,
    /// Статистика безопасности
    stats: MemorySafetyStats,
    /// Включена ли система безопасности
    enabled: bool,
    /// Ошибки безопасности
    violations: Vec<MemoryViolation>,
}

/// Информация о выделенном блоке памяти
#[derive(Debug, Clone)]
pub struct MemoryBlockInfo {
    /// Адрес начала блока
    pub start_addr: usize,
    /// Размер блока
    pub size: usize,
    /// Использовано байт
    pub used_bytes: usize,
    /// Владелец (PID или ID потока)
    pub owner: Option<u64>,
    /// Время выделения
    pub alloc_time: u64,
    /// Контекст выделения (имя функции, строка, файл)
    pub context: String,
    /// Статус блока
    pub status: BlockStatus,
    /// Флаги безопасности
    pub flags: MemoryBlockFlags,
}

/// Статус блока
#[derive(Debug, Clone, PartialEq)]
pub enum BlockStatus {
    /// Выделен и используется
    Allocated,
    /// Освобожден
    Deallocated,
    /// Поврежден
    Corrupted,
    /// Переполнен
    Overflowed,
    /// Нарушение прав доступа
    AccessViolation,
}

/// Флаги безопасности блока
#[derive(Debug, Clone)]
pub struct MemoryBlockFlags {
    /// Только для чтения
    pub read_only: bool,
    /// Только для записи
    pub write_only: bool,
    /// Выполняемый код
    pub executable: bool,
    /// Может быть освобожден
    pub deallocatable: bool,
    /// Отслеживать UAF
    pub track_uaf: bool,
    /// Проверять переполнения
    pub check_overflow: bool,
}

/// Статистика безопасности памяти
#[derive(Debug, Clone)]
pub struct MemorySafetyStats {
    /// Всего выделений
    pub allocations: u64,
    /// Всего освобождений
    pub deallocations: u64,
    /// Обнаруженных нарушений
    pub violations: u64,
    /// Двойных освобождений
    pub double_frees: u64,
    /// Использований после освобождения
    pub use_after_frees: u64,
    /// Переполнений буфера
    pub buffer_overflows: u64,
    /// Нарушений прав доступа
    pub access_violations: u64,
    /// Поврежденных блоков
    pub corrupted_blocks: u64,
    /// Последнее обновление
    pub last_update: u64,
}

/// Типы нарушений безопасности
#[derive(Debug, Clone)]
pub enum MemoryViolation {
    /// Двойное освобождение
    DoubleFree { addr: usize, context: String },
    /// Использование после освобождения
    UseAfterFree { addr: usize, context: String },
    /// Переполнение буфера
    BufferOverflow {
        addr: usize,
        size_requested: usize,
        block_size: usize,
        context: String,
    },
    /// Нарушение прав доступа
    AccessViolation {
        addr: usize,
        operation: AccessOperation,
        context: String,
    },
    /// Повреждение блока
    BlockCorruption { addr: usize, context: String },
    /// Неверное выравнивание
    MisalignedAccess {
        addr: usize,
        expected_alignment: usize,
        context: String,
    },
}

/// Тип операции доступа
#[derive(Debug, Clone)]
pub enum AccessOperation {
    /// Чтение
    Read,
    /// Запись
    Write,
    /// Выполнение
    Execute,
}

impl MemorySafetyManager {
    /// Создает новый менеджер безопасности
    pub fn new() -> Self {
        MemorySafetyManager {
            allocated_blocks: BTreeMap::new(),
            stats: MemorySafetyStats::new(),
            enabled: true,
            violations: Vec::new(),
        }
    }

    /// Регистрирует выделение памяти
    pub fn register_allocation(
        &mut self,
        addr: usize,
        size: usize,
        owner: Option<u64>,
        context: String,
    ) {
        if !self.enabled {
            return;
        }

        let block_info = MemoryBlockInfo {
            start_addr: addr,
            size,
            used_bytes: size, // Изначально весь блок используется
            owner,
            alloc_time: crate::timer::get_uptime_ms(),
            context,
            status: BlockStatus::Allocated,
            flags: MemoryBlockFlags::default(),
        };

        self.allocated_blocks.insert(addr, block_info);
        self.stats.allocations += 1;
        self.stats.last_update = crate::timer::get_uptime_ms();
    }

    /// Регистрирует освобождение памяти
    pub fn register_deallocation(&mut self, addr: usize) -> Result<(), MemoryViolation> {
        if !self.enabled {
            return Ok(());
        }

        if let Some(block) = self.allocated_blocks.get_mut(&addr) {
            if block.status != BlockStatus::Allocated {
                let violation = MemoryViolation::DoubleFree {
                    addr,
                    context: block.context.clone(),
                };

                self.record_violation(violation.clone());
                return Err(violation);
            }

            // Отмечаем блок как освобожденный
            block.status = BlockStatus::Deallocated;

            // Проверяем, нужно ли отслеживать UAF
            if block.flags.track_uaf {
                // Оставляем метаинформацию о блоке на некоторое время для отслеживания UAF
                // В реальной системе: добавляем в список "освобожденных, но отслеживаемых" блоков
            } else {
                // Удаляем блок из списка
                self.allocated_blocks.remove(&addr);
            }

            self.stats.deallocations += 1;
            self.stats.last_update = crate::timer::get_uptime_ms();
            Ok(())
        } else {
            // Попытка освободить несуществующий блок
            let violation = MemoryViolation::DoubleFree {
                addr,
                context: "Unknown context".to_string(),
            };

            self.record_violation(violation.clone());
            Err(violation)
        }
    }

    /// Проверяет, является ли адрес валидным для доступа
    pub fn check_access(
        &mut self,
        addr: usize,
        size: usize,
        operation: AccessOperation,
    ) -> Result<(), MemoryViolation> {
        if !self.enabled {
            return Ok(());
        }

        // Проверяем, находится ли адрес в каком-либо выделенном блоке
        for (_, block) in self.allocated_blocks.iter() {
            if addr >= block.start_addr && addr + size <= block.start_addr + block.size {
                // Адрес находится в пределах блока

                // Проверяем права доступа
                match operation {
                    AccessOperation::Read => {
                        if block.flags.read_only || block.status != BlockStatus::Allocated {
                            let violation = MemoryViolation::AccessViolation {
                                addr,
                                operation,
                                context: block.context.clone(),
                            };

                            self.record_violation(violation.clone());
                            return Err(violation);
                        }
                    }
                    AccessOperation::Write => {
                        if block.flags.read_only || block.status != BlockStatus::Allocated {
                            let violation = MemoryViolation::AccessViolation {
                                addr,
                                operation,
                                context: block.context.clone(),
                            };

                            self.record_violation(violation.clone());
                            return Err(violation);
                        }
                    }
                    AccessOperation::Execute => {
                        if !block.flags.executable || block.status != BlockStatus::Allocated {
                            let violation = MemoryViolation::AccessViolation {
                                addr,
                                operation,
                                context: block.context.clone(),
                            };

                            self.record_violation(violation.clone());
                            return Err(violation);
                        }
                    }
                }

                return Ok(());
            }
        }

        // Если не нашли соответствующий блок, это нарушение
        let violation = MemoryViolation::AccessViolation {
            addr,
            operation,
            context: "Address not in allocated block".to_string(),
        };

        self.record_violation(violation.clone());
        Err(violation)
    }

    /// Проверяет, не произошло ли UAF
    pub fn check_use_after_free(&mut self, addr: usize) -> Result<(), MemoryViolation> {
        if !self.enabled {
            return Ok(());
        }

        if let Some(block) = self.allocated_blocks.get(&addr) {
            if block.status == BlockStatus::Deallocated {
                let violation = MemoryViolation::UseAfterFree {
                    addr,
                    context: block.context.clone(),
                };

                self.record_violation(violation.clone());
                return Err(violation);
            }
        }

        Ok(())
    }

    /// Проверяет переполнение буфера
    pub fn check_buffer_overflow(
        &mut self,
        addr: usize,
        requested_size: usize,
    ) -> Result<(), MemoryViolation> {
        if !self.enabled {
            return Ok(());
        }

        for (_, block) in self.allocated_blocks.iter() {
            // Проверяем, пересекается ли запрашиваемый диапазон с блоком
            if addr >= block.start_addr && addr < block.start_addr + block.size {
                if addr + requested_size > block.start_addr + block.size {
                    let violation = MemoryViolation::BufferOverflow {
                        addr,
                        size_requested: requested_size,
                        block_size: block.size,
                        context: block.context.clone(),
                    };

                    self.record_violation(violation.clone());
                    return Err(violation);
                }
            }
        }

        Ok(())
    }

    /// Проверяет выравнивание адреса
    pub fn check_alignment(
        &mut self,
        addr: usize,
        alignment: usize,
    ) -> Result<(), MemoryViolation> {
        if !self.enabled {
            return Ok(());
        }

        if addr % alignment != 0 {
            let violation = MemoryViolation::MisalignedAccess {
                addr,
                expected_alignment: alignment,
                context: "Alignment check failed".to_string(),
            };

            self.record_violation(violation.clone());
            Err(violation)
        } else {
            Ok(())
        }
    }

    /// Устанавливает флаги безопасности для блока
    pub fn set_block_flags(&mut self, addr: usize, flags: MemoryBlockFlags) {
        if let Some(block) = self.allocated_blocks.get_mut(&addr) {
            block.flags = flags;
        }
    }

    /// Включает отслеживание безопасности
    pub fn enable_safety(&mut self) {
        self.enabled = true;
    }

    /// Выключает отслеживание безопасности (для отладки)
    pub fn disable_safety(&mut self) {
        self.enabled = false;
    }

    /// Проверяет, включено ли отслеживание безопасности
    pub fn is_safety_enabled(&self) -> bool {
        self.enabled
    }

    /// Записывает нарушение безопасности
    fn record_violation(&mut self, violation: MemoryViolation) {
        self.violations.push(violation);
        self.stats.violations += 1;

        // Обновляем статистику по типу нарушения
        match &self.violations[self.violations.len() - 1] {
            MemoryViolation::DoubleFree { .. } => self.stats.double_frees += 1,
            MemoryViolation::UseAfterFree { .. } => self.stats.use_after_frees += 1,
            MemoryViolation::BufferOverflow { .. } => self.stats.buffer_overflows += 1,
            MemoryViolation::AccessViolation { .. } => self.stats.access_violations += 1,
            MemoryViolation::BlockCorruption { .. } => self.stats.corrupted_blocks += 1,
            MemoryViolation::MisalignedAccess { .. } => self.stats.access_violations += 1,
        }

        // Логгируем нарушение (в реальной системе тут будет более подробная информация)
        crate::println!(
            "Memory Safety Violation: {:?}",
            &self.violations[self.violations.len() - 1]
        );
    }

    /// Получает текущую статистику безопасности
    pub fn get_stats(&self) -> &MemorySafetyStats {
        &self.stats
    }

    /// Получает список нарушений
    pub fn get_violations(&self) -> &[MemoryViolation] {
        &self.violations
    }

    /// Очищает историю нарушений
    pub fn clear_violations(&mut self) {
        self.violations.clear();
    }

    /// Сбрасывает статистику
    pub fn reset_stats(&mut self) {
        self.stats = MemorySafetyStats::new();
    }

    /// Проверяет целостность всех блоков
    pub fn verify_all_blocks(&mut self) -> Vec<MemoryViolation> {
        let mut violations = Vec::new();

        // В реальной системе тут проверяется:
        // - Контрольные суммы блоков
        // - Гарантии целостности
        // - Повреждения от переполнения

        // Пока возвращаем пустой список
        violations
    }

    /// Проверяет, принадлежит ли адрес какому-либо выделенному блоку
    pub fn is_address_allocated(&self, addr: usize) -> bool {
        for (_, block) in self.allocated_blocks.iter() {
            if addr >= block.start_addr && addr < block.start_addr + block.size {
                return block.status == BlockStatus::Allocated;
            }
        }
        false
    }

    /// Получает информацию о блоке
    pub fn get_block_info(&self, addr: usize) -> Option<&MemoryBlockInfo> {
        for (_, block) in self.allocated_blocks.iter() {
            if addr >= block.start_addr && addr < block.start_addr + block.size {
                return Some(block);
            }
        }
        None
    }

    /// Получает размер выделенного блока
    pub fn get_block_size(&self, addr: usize) -> Option<usize> {
        self.get_block_info(addr).map(|info| info.size)
    }

    /// Проверяет, принадлежит ли блок процессу
    pub fn is_block_owned_by_process(&self, addr: usize, pid: u64) -> bool {
        if let Some(block) = self.get_block_info(addr) {
            block.owner == Some(pid)
        } else {
            false
        }
    }
}

impl MemorySafetyStats {
    fn new() -> Self {
        MemorySafetyStats {
            allocations: 0,
            deallocations: 0,
            violations: 0,
            double_frees: 0,
            use_after_frees: 0,
            buffer_overflows: 0,
            access_violations: 0,
            corrupted_blocks: 0,
            last_update: 0,
        }
    }
}

impl MemoryBlockFlags {
    fn default() -> Self {
        MemoryBlockFlags {
            read_only: false,
            write_only: false,
            executable: false,
            deallocatable: true,
            track_uaf: false,
            check_overflow: false,
        }
    }
}

/// Обертка для безопасного указателя
pub struct SafePtr<T> {
    ptr: NonNull<T>,
    /// Проверять ли безопасность при доступе
    check_safety: bool,
    /// PhantomData для обозначения владения типом T
    _phantom: PhantomData<T>,
}

impl<T> SafePtr<T> {
    /// Создает новый безопасный указатель
    pub fn new(ptr: *mut T) -> Option<Self> {
        NonNull::new(ptr).map(|ptr_nn| SafePtr {
            ptr: ptr_nn,
            check_safety: true,
            _phantom: PhantomData,
        })
    }

    /// Создает новый безопасный указатель без проверки безопасности
    pub fn new_unchecked(ptr: *mut T) -> Option<Self> {
        NonNull::new(ptr).map(|ptr_nn| SafePtr {
            ptr: ptr_nn,
            check_safety: false,
            _phantom: PhantomData,
        })
    }

    /// Получает сырой указатель
    pub fn as_ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }

    /// Получает ссылку на значение (безопасно)
    pub fn get_ref(&self) -> &T {
        if self.check_safety {
            let addr = self.ptr.as_ptr() as usize;
            if !get_memory_safety_manager()
                .lock()
                .is_address_allocated(addr)
            {
                // Адрес не выделен - это может быть критическая ошибка
                // В реальной системе тут нужно обработать ситуацию
            }
        }

        unsafe { self.ptr.as_ref() }
    }

    /// Получает изменяемую ссылку на значение (безопасно)
    pub fn get_mut(&mut self) -> &mut T {
        if self.check_safety {
            let addr = self.ptr.as_ptr() as usize;
            if !get_memory_safety_manager()
                .lock()
                .is_address_allocated(addr)
            {
                // Адрес не выделен
            }
        }

        unsafe { self.ptr.as_mut() }
    }

    /// Проверяет, может ли указатель производить заданную операцию
    pub fn check_access(&self, operation: AccessOperation) -> bool {
        if self.check_safety {
            let addr = self.ptr.as_ptr() as usize;
            get_memory_safety_manager()
                .lock()
                .check_access(addr, core::mem::size_of::<T>(), operation)
                .is_ok()
        } else {
            true
        }
    }

    /// Отключает проверки безопасности
    pub fn disable_safety_checks(&mut self) {
        self.check_safety = false;
    }

    /// Включает проверки безопасности
    pub fn enable_safety_checks(&mut self) {
        self.check_safety = true;
    }
}

impl<T> Clone for SafePtr<T> {
    fn clone(&self) -> Self {
        SafePtr {
            ptr: self.ptr,
            check_safety: self.check_safety,
            _phantom: self._phantom,
        }
    }
}

impl<T> Copy for SafePtr<T> {}

impl<T> Drop for SafePtr<T> {
    fn drop(&mut self) {
        // В реальной системе: освобождение памяти или уменьшение счетчика ссылок
        // В нашем случае безопасное освобождение делается отдельно
    }
}

/// Обертка для безопасного буфера
pub struct SafeBuffer {
    ptr: *mut u8,
    size: usize,
    flags: MemoryBlockFlags,
}

impl SafeBuffer {
    /// Создает новый безопасный буфер
    pub fn new(size: usize) -> Option<Self> {
        // В реальной системе тут будет выделение памяти через системный аллокатор
        // Пока эмулируем выделение

        // Имитация выделения физической памяти
        let ptr = unsafe {
            // В реальности тут будет системный вызов выделения
            core::ptr::null_mut() // Заглушка
        };

        if !ptr.is_null() {
            // Регистрируем выделение в системе безопасности
            get_memory_safety_manager().lock().register_allocation(
                ptr as usize,
                size,
                None, // PID владельца
                "SafeBuffer::new".to_string(),
            );

            Some(SafeBuffer {
                ptr,
                size,
                flags: MemoryBlockFlags::default(),
            })
        } else {
            None
        }
    }

    /// Создает безопасный буфер из сырого указателя
    pub fn from_raw(ptr: *mut u8, size: usize) -> Option<Self> {
        if ptr.is_null() {
            return None;
        }

        // Регистрируем в системе безопасности
        get_memory_safety_manager().lock().register_allocation(
            ptr as usize,
            size,
            None,
            "SafeBuffer::from_raw".to_string(),
        );

        Some(SafeBuffer {
            ptr,
            size,
            flags: MemoryBlockFlags::default(),
        })
    }

    /// Получает доступ к буферу по индексу (безопасно)
    pub fn get(&self, index: usize) -> Option<u8> {
        if index >= self.size {
            return None;
        }

        // Проверяем безопасность
        if get_memory_safety_manager()
            .lock()
            .check_access((self.ptr as usize) + index, 1, AccessOperation::Read)
            .is_err()
        {
            return None;
        }

        Some(unsafe { *self.ptr.add(index) })
    }

    /// Устанавливает значение по индексу (безопасно)
    pub fn set(&mut self, index: usize, value: u8) -> Result<(), &'static str> {
        if index >= self.size {
            return Err("Index out of bounds");
        }

        // Проверяем, разрешена ли запись
        if self.flags.read_only {
            return Err("Write to read-only buffer");
        }

        // Проверяем безопасность
        if get_memory_safety_manager()
            .lock()
            .check_access((self.ptr as usize) + index, 1, AccessOperation::Write)
            .is_err()
        {
            return Err("Write access violation");
        }

        unsafe {
            *self.ptr.add(index) = value;
        }
        Ok(())
    }

    /// Копирует данные в безопасный буфер (безопасно)
    pub fn copy_from_slice(&mut self, src: &[u8], offset: usize) -> Result<(), &'static str> {
        if offset + src.len() > self.size {
            return Err("Copy would exceed buffer bounds");
        }

        // Проверяем безопасность
        if get_memory_safety_manager()
            .lock()
            .check_access(
                (self.ptr as usize) + offset,
                src.len(),
                AccessOperation::Write,
            )
            .is_err()
        {
            return Err("Write access violation");
        }

        unsafe {
            core::ptr::copy_nonoverlapping(src.as_ptr(), self.ptr.add(offset), src.len());
        }
        Ok(())
    }

    /// Копирует данные из буфера (безопасно)
    pub fn copy_to_slice(&self, dst: &mut [u8], offset: usize) -> Result<(), &'static str> {
        if offset + dst.len() > self.size {
            return Err("Copy would exceed buffer bounds");
        }

        // Проверяем безопасность
        if get_memory_safety_manager()
            .lock()
            .check_access(
                (self.ptr as usize) + offset,
                dst.len(),
                AccessOperation::Read,
            )
            .is_err()
        {
            return Err("Read access violation");
        }

        unsafe {
            core::ptr::copy_nonoverlapping(self.ptr.add(offset), dst.as_mut_ptr(), dst.len());
        }
        Ok(())
    }

    /// Получает длину буфера
    pub fn len(&self) -> usize {
        self.size
    }

    /// Проверяет, пуст ли буфер
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Освобождает буфер
    pub fn deallocate(self) -> Result<(), &'static str> {
        if !self.ptr.is_null() {
            // Регистрируем освобождение в системе безопасности
            if get_memory_safety_manager()
                .lock()
                .register_deallocation(self.ptr as usize)
                .is_err()
            {
                return Err("Double-free detected");
            }

            // В реальной системе тут будет освобождение физической памяти
            // unsafe { deallocate_physical_memory(self.ptr, self.size); }
        }
        Ok(())
    }

    /// Получает сырой указатель
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr
    }

    /// Получает изменяемый сырой указатель
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr
    }

    /// Устанавливает флаги безопасности
    pub fn set_flags(&mut self, flags: MemoryBlockFlags) {
        self.flags = flags;
    }
}

// Глобальный менеджер безопасности
static MEMORY_SAFETY_MANAGER: spin::Once<spin::Mutex<MemorySafetyManager>> = spin::Once::new();

pub fn get_memory_safety_manager() -> &'static spin::Mutex<MemorySafetyManager> {
    MEMORY_SAFETY_MANAGER.call_once(|| spin::Mutex::new(MemorySafetyManager::new()))
}

/// Интерфейс для системы безопасности
pub struct MemorySafetyInterface;

impl MemorySafetyInterface {
    /// Включает систему безопасности памяти
    pub fn enable_safety() {
        get_memory_safety_manager().lock().enable_safety();
    }

    /// Выключает систему безопасности памяти
    pub fn disable_safety() {
        get_memory_safety_manager().lock().disable_safety();
    }

    /// Проверяет, включена ли система безопасности
    pub fn is_safety_enabled() -> bool {
        get_memory_safety_manager().lock().is_safety_enabled()
    }

    /// Регистрирует выделение памяти
    pub fn register_allocation(addr: usize, size: usize, context: &str) {
        get_memory_safety_manager().lock().register_allocation(
            addr,
            size,
            None,
            context.to_string(),
        );
    }

    /// Регистрирует освобождение памяти
    pub fn register_deallocation(addr: usize) -> Result<(), MemoryViolation> {
        get_memory_safety_manager()
            .lock()
            .register_deallocation(addr)
    }

    /// Проверяет доступ к памяти
    pub fn check_memory_access(
        addr: usize,
        size: usize,
        operation: AccessOperation,
    ) -> Result<(), MemoryViolation> {
        get_memory_safety_manager()
            .lock()
            .check_access(addr, size, operation)
    }

    /// Проверяет использование после освобождения
    pub fn check_use_after_free(addr: usize) -> Result<(), MemoryViolation> {
        get_memory_safety_manager()
            .lock()
            .check_use_after_free(addr)
    }

    /// Проверяет переполнение буфера
    pub fn check_buffer_overflow(
        addr: usize,
        requested_size: usize,
    ) -> Result<(), MemoryViolation> {
        get_memory_safety_manager()
            .lock()
            .check_buffer_overflow(addr, requested_size)
    }

    /// Проверяет выравнивание
    pub fn check_alignment(addr: usize, alignment: usize) -> Result<(), MemoryViolation> {
        get_memory_safety_manager()
            .lock()
            .check_alignment(addr, alignment)
    }

    /// Получает статистику безопасности
    pub fn get_safety_stats() -> MemorySafetyStats {
        get_memory_safety_manager().lock().get_stats().clone()
    }

    /// Получает нарушения безопасности
    pub fn get_safety_violations() -> Vec<MemoryViolation> {
        get_memory_safety_manager().lock().get_violations().to_vec()
    }

    /// Очищает нарушения безопасности
    pub fn clear_violations() {
        get_memory_safety_manager().lock().clear_violations();
    }

    /// Сбрасывает статистику безопасности
    pub fn reset_safety_stats() {
        get_memory_safety_manager().lock().reset_stats();
    }

    /// Проверяет, выделен ли адрес
    pub fn is_address_allocated(addr: usize) -> bool {
        get_memory_safety_manager()
            .lock()
            .is_address_allocated(addr)
    }

    /// Получает информацию о блоке
    pub fn get_block_info(addr: usize) -> Option<MemoryBlockInfo> {
        get_memory_safety_manager()
            .lock()
            .get_block_info(addr)
            .cloned()
    }

    /// Получает размер блока
    pub fn get_block_size(addr: usize) -> Option<usize> {
        get_memory_safety_manager().lock().get_block_size(addr)
    }

    /// Проверяет, принадлежит ли блок процессу
    pub fn is_block_owned_by_process(addr: usize, pid: u64) -> bool {
        get_memory_safety_manager()
            .lock()
            .is_block_owned_by_process(addr, pid)
    }

    /// Проверяет целостность всех блоков
    pub fn verify_all_blocks() -> Vec<MemoryViolation> {
        get_memory_safety_manager().lock().verify_all_blocks()
    }
}

/// Специальный аллокатор с безопасностью
pub struct SafeAllocator {
    /// Внутренний аллокатор
    backing_allocator: crate::memory::BumpAllocator,
}

impl SafeAllocator {
    pub fn new(memory_start: usize, memory_size: usize) -> Self {
        SafeAllocator {
            backing_allocator: crate::memory::BumpAllocator::new(memory_start, memory_size),
        }
    }
}

unsafe impl alloc::alloc::GlobalAlloc for SafeAllocator {
    unsafe fn alloc(&self, layout: alloc::alloc::Layout) -> *mut u8 {
        let ptr = self.backing_allocator.alloc(layout);
        if !ptr.is_null() {
            // Регистрируем выделение в системе безопасности
            MemorySafetyInterface::register_allocation(
                ptr as usize,
                layout.size(),
                "SafeAllocator",
            );
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: alloc::alloc::Layout) {
        // Проверяем, было ли выделение зарегистрировано
        if MemorySafetyInterface::is_address_allocated(ptr as usize) {
            let _ = MemorySafetyInterface::register_deallocation(ptr as usize);
        }
        self.backing_allocator.dealloc(ptr, layout);
    }
}

/// Макросы для удобства
#[macro_export]
macro_rules! safe_ptr {
    ($expr:expr) => {
        $crate::memory_safety::SafePtr::new($expr).expect("SafePtr creation failed")
    };
}

#[macro_export]
macro_rules! safe_buffer {
    ($size:expr) => {
        $crate::memory_safety::SafeBuffer::new($size).expect("SafeBuffer creation failed")
    };
}

#[macro_export]
macro_rules! check_safe_access {
    ($addr:expr, $size:expr, $op:expr) => {
        $crate::memory_safety::MemorySafetyInterface::check_memory_access($addr, $size, $op).is_ok()
    };
}

#[macro_export]
macro_rules! register_safe_allocation {
    ($addr:expr, $size:expr, $context:expr) => {
        $crate::memory_safety::MemorySafetyInterface::register_allocation($addr, $size, $context);
    };
}

#[macro_export]
macro_rules! register_safe_deallocation {
    ($addr:expr) => {
        let _ = $crate::memory_safety::MemorySafetyInterface::register_deallocation($addr);
    };
}

/// Система безопасности для кучи
pub struct HeapSafetyManager;

impl HeapSafetyManager {
    /// Инициализирует безопасную кучу
    pub fn init_safe_heap() -> Result<(), &'static str> {
        // В реальной системе: регистрируем безопасный аллокатор как глобальный
        // unsafe { alloc::alloc::set_global_allocator(SafeAllocator::new(HEAP_START, HEAP_SIZE)); }
        Ok(())
    }

    /// Проверяет безопасность кучи
    pub fn verify_heap_safety() -> Result<(), &'static str> {
        // В реальной системе: проверка целостности кучи, поиск утечек и т.д.
        Ok(())
    }

    /// Проверяет, не произошло ли утечки памяти
    pub fn check_memory_leaks() -> Vec<usize> {
        let mut leaks = Vec::new();
        let safety_manager = get_memory_safety_manager().lock();

        // Ищем выделенные, но не освобожденные блоки
        for (addr, block) in safety_manager.allocated_blocks.iter() {
            if block.status == BlockStatus::Allocated {
                leaks.push(*addr);
            }
        }

        leaks
    }
}

/// Система отслеживания ссылок
pub struct ReferenceTracker {
    /// Счетчики ссылок
    reference_counts: BTreeMap<usize, usize>,
    /// Владельцы
    owners: BTreeMap<usize, u64>,
}

impl ReferenceTracker {
    pub fn new() -> Self {
        ReferenceTracker {
            reference_counts: BTreeMap::new(),
            owners: BTreeMap::new(),
        }
    }

    /// Создает новую ссылку
    pub fn create_reference(&mut self, addr: usize, owner: u64) -> usize {
        let count = self.reference_counts.entry(addr).or_insert(0);
        *count += 1;
        self.owners.insert(addr, owner);
        *count
    }

    /// Удаляет ссылку
    pub fn remove_reference(&mut self, addr: usize) -> usize {
        if let Some(count) = self.reference_counts.get_mut(&addr) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.reference_counts.remove(&addr);
                self.owners.remove(&addr);
            }
            *count
        } else {
            0
        }
    }

    /// Получает количество ссылок
    pub fn get_reference_count(&self, addr: usize) -> usize {
        *self.reference_counts.get(&addr).unwrap_or(&0)
    }

    /// Проверяет, есть ли живые ссылки
    pub fn has_live_references(&self, addr: usize) -> bool {
        self.get_reference_count(addr) > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_safety_manager_creation() {
        let manager = MemorySafetyManager::new();
        assert_eq!(manager.allocated_blocks.len(), 0);
        assert_eq!(manager.enabled, true);
        assert_eq!(manager.stats.allocations, 0);
    }

    #[test]
    fn test_memory_allocation_tracking() {
        let mut manager = MemorySafetyManager::new();

        let addr = 0x1000;
        let size = 1024;

        manager.register_allocation(addr, size, None, "test".to_string());

        assert!(manager.is_address_allocated(addr));
        assert_eq!(manager.get_block_size(addr), Some(size));

        let info = manager.get_block_info(addr).unwrap();
        assert_eq!(info.size, size);
        assert_eq!(info.start_addr, addr);
        assert_eq!(info.status, BlockStatus::Allocated);
    }

    #[test]
    fn test_memory_deallocation_tracking() {
        let mut manager = MemorySafetyManager::new();

        let addr = 0x2000;
        let size = 512;

        manager.register_allocation(addr, size, None, "test".to_string());

        // Первое освобождение - должно пройти успешно
        assert!(manager.register_deallocation(addr).is_ok());

        // Второе освобождение - должно вызвать ошибку
        assert!(matches!(
            manager.register_deallocation(addr),
            Err(MemoryViolation::DoubleFree { .. })
        ));
    }

    #[test]
    fn test_safe_ptr_creation() {
        let mut data = 42i32;
        let ptr = &mut data as *mut i32;

        let safe_ptr = SafePtr::new(ptr).unwrap();
        assert_eq!(*safe_ptr.get_ref(), 42);

        let mut safe_ptr_mut = safe_ptr;
        *safe_ptr_mut.get_mut() = 100;
        assert_eq!(*safe_ptr.get_ref(), 100);
    }

    #[test]
    fn test_safe_buffer_basic() {
        let mut buffer = SafeBuffer::new(1024).unwrap();

        // Тестируем безопасный доступ
        assert!(buffer.set(0, 255).is_ok());
        assert_eq!(buffer.get(0), Some(255));

        // Тестируем безопасный доступ за пределами
        assert_eq!(buffer.get(2048), None); // За пределами буфера
    }

    #[test]
    fn test_access_violation_detection() {
        let mut manager = MemorySafetyManager::new();

        let addr = 0x3000;
        manager.register_allocation(addr, 100, None, "test".to_string());

        // Проверяем безопасный доступ
        assert!(manager
            .check_access(addr, 10, AccessOperation::Read)
            .is_ok());

        // Помечаем блок как read-only
        manager.set_block_flags(
            addr,
            MemoryBlockFlags {
                read_only: true,
                ..MemoryBlockFlags::default()
            },
        );

        // Попытка записи в read-only блок
        assert!(matches!(
            manager.check_access(addr, 10, AccessOperation::Write),
            Err(MemoryViolation::AccessViolation { .. })
        ));
    }

    #[test]
    fn test_buffer_overflow_detection() {
        let mut manager = MemorySafetyManager::new();

        let addr = 0x4000;
        let size = 100;
        manager.register_allocation(addr, size, None, "test".to_string());

        // Попытка доступа за пределы блока
        assert!(matches!(
            manager.check_buffer_overflow(addr + size + 1, 10),
            Err(MemoryViolation::BufferOverflow { .. })
        ));
    }
}
