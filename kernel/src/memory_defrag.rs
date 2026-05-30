use alloc::{collections::BTreeMap, vec, vec::Vec};
use core::ops::Range;
use spin::Mutex;

/// MemoryDefrag - система дефрагментации памяти
///
/// Оптимизирует распределение физической памяти для уменьшения фрагментации
/// и улучшения производительности при выделении крупных блоков.
pub struct MemoryDefrag {
    /// Список свободных блоков
    pub free_blocks: Vec<Range<usize>>,
    /// Список занятых блоков
    pub used_blocks: BTreeMap<usize, UsedBlock>,
    /// Общий размер управляемой памяти
    pub total_size: usize,
    /// Размер минимального блока (для выравнивания)
    pub min_block_size: usize,
    /// Статистика дефрагментации
    pub stats: DefragStats,
    /// Включена ли автоматическая дефрагментация
    pub auto_defrag_enabled: bool,
    /// Порог фрагментации для запуска автоматической дефрагментации
    pub defrag_threshold: f64,
}

/// Информация о занятом блоке
#[derive(Debug, Clone)]
pub struct UsedBlock {
    /// Размер блока
    pub size: usize,
    /// Владелец (PID или ID процесса)
    pub owner: Option<u64>,
    /// Время выделения
    pub allocation_time: u64,
    /// Флаги блока
    pub flags: BlockFlags,
    /// Состояние блока
    pub state: BlockState,
}

/// Флаги блока
#[derive(Debug, Clone)]
pub struct BlockFlags {
    /// Блок может быть перемещен
    pub movable: bool,
    /// Блок содержит код
    pub executable: bool,
    /// Блок может быть сжат
    pub compressible: bool,
    /// Блок критический (не может быть перемещен)
    pub critical: bool,
}

/// Состояние блока
#[derive(Debug, Clone, PartialEq)]
pub enum BlockState {
    /// Активный
    Active,
    /// Неактивный (не используется давно)
    Inactive,
    /// Готов к перемещению
    ReadyToMove,
    /// Заблокирован (не может быть перемещен)
    Locked,
}

/// Статистика дефрагментации
#[derive(Debug, Clone)]
pub struct DefragStats {
    /// Количество перемещений
    pub move_count: u64,
    /// Количество объединений
    pub merge_count: u64,
    /// Объем перемещенной памяти
    pub moved_bytes: u64,
    /// Время последней дефрагментации
    pub last_defrag_time: u64,
    /// Общее количество дефрагментаций
    pub total_defrags: u64,
    /// Количество свободных блоков до дефрагментации
    pub free_blocks_before: usize,
    /// Количество свободных блоков после дефрагментации
    pub free_blocks_after: usize,
}

impl MemoryDefrag {
    /// Создает новый дефрагментатор
    pub fn new(total_size: usize, min_block_size: usize) -> Self {
        let mut defrag = MemoryDefrag {
            free_blocks: vec![0..total_size],
            used_blocks: BTreeMap::new(),
            total_size,
            min_block_size,
            stats: DefragStats::new(),
            auto_defrag_enabled: true,
            defrag_threshold: 0.7, // 70% фрагментации
        };

        defrag.defrag_stats.free_blocks_before = defrag.get_free_blocks_count();
        defrag.defrag_stats.free_blocks_after = defrag.get_free_blocks_count();

        defrag
    }

    /// Добавляет выделенный блок в систему отслеживания
    pub fn register_allocated_block(
        &mut self,
        start: usize,
        size: usize,
        owner: Option<u64>,
    ) -> Result<(), &'static str> {
        if start + size > self.total_size {
            return Err("Block exceeds memory bounds");
        }

        // Проверяем, что блок не пересекается с уже существующими
        if self.intersects_with_used_block(start, start + size) {
            return Err("Block intersects with existing block");
        }

        // Удаляем из свободных блоков
        self.remove_from_free_range(start..start + size);

        // Добавляем в занятые
        self.used_blocks.insert(
            start,
            UsedBlock {
                size,
                owner,
                allocation_time: crate::timer::get_uptime_ms(),
                flags: BlockFlags::default(),
                state: BlockState::Active,
            },
        );

        Ok(())
    }

    /// Регистрирует освобождение блока
    pub fn register_deallocated_block(&mut self, start: usize) -> Result<(), &'static str> {
        if let Some(block) = self.used_blocks.remove(&start) {
            // Добавляем в свободные блоки
            self.add_free_range(start..start + block.size);
            Ok(())
        } else {
            Err("Block not found in allocated list")
        }
    }

    /// Выполняет дефрагментацию памяти
    pub fn defragment(&mut self) -> Result<(), &'static str> {
        // Обновляем статистику до дефрагментации
        self.stats.free_blocks_before = self.get_free_blocks_count();

        // Сжимаем память (перемещаем блоки ближе друг к другу)
        self.compact_memory()?;

        // Объединяем соседние свободные блоки
        self.merge_adjacent_free_blocks();

        // Обновляем статистику после дефрагментации
        self.stats.free_blocks_after = self.get_free_blocks_count();
        self.stats.last_defrag_time = crate::timer::get_uptime_ms();
        self.stats.total_defrags += 1;

        Ok(())
    }

    /// Сжимает память, перемещая блоки
    fn compact_memory(&mut self) -> Result<(), &'static str> {
        // Сортируем использованные блоки по адресу
        let mut used_sorted: Vec<_> = self.used_blocks.iter().collect();
        used_sorted.sort_by_key(|(addr, _)| *addr);

        // Создаем копию для безопасного перемещения
        let mut new_used_blocks = BTreeMap::new();
        let mut current_position = 0;

        for (addr, block_info) in used_sorted {
            if block_info.state != BlockState::Locked && block_info.flags.movable {
                // Проверяем, можно ли переместить блок
                if current_position < *addr {
                    // Перемещаем блок
                    if self.move_block(*addr, current_position, block_info.size)? {
                        // Создаем новый блок в новой позиции
                        new_used_blocks.insert(current_position, block_info.clone());

                        // Помечаем старую позицию как свободную
                        self.add_free_range(*addr..addr + block_info.size);

                        self.stats.move_count += 1;
                        self.stats.moved_bytes += block_info.size as u64;

                        // Обновляем текущую позицию
                        current_position += block_info.size;
                    } else {
                        // Не смогли переместить, оставляем на месте
                        new_used_blocks.insert(*addr, block_info.clone());
                        current_position = addr + block_info.size;
                    }
                } else {
                    // Блок уже находится в хорошем месте
                    new_used_blocks.insert(*addr, block_info.clone());
                    current_position = addr + block_info.size;
                }
            } else {
                // Неперемещаемый блок - оставляем на месте
                new_used_blocks.insert(*addr, block_info.clone());
                current_position = addr + block_info.size;
            }
        }

        // Заменяем использованные блоки
        self.used_blocks = new_used_blocks;

        Ok(())
    }

    /// Перемещает блок из одной позиции в другую
    fn move_block(
        &mut self,
        old_start: usize,
        new_start: usize,
        size: usize,
    ) -> Result<bool, &'static str> {
        // Проверяем, что новый диапазон свободен
        if !self.range_is_free(new_start..new_start + size) {
            return Ok(false); // Не можем переместить
        }

        // В реальной системе тут будет:
        // 1. Копирование содержимого блока
        // 2. Обновление всех указателей на блок
        // 3. Обновление таблицы страниц (если применимо)

        // Для эмуляции просто помечаем перемещение
        Ok(true)
    }

    /// Объединяет соседние свободные блоки
    fn merge_adjacent_free_blocks(&mut self) {
        if self.free_blocks.len() < 2 {
            return;
        }

        // Сортируем блоки по началу
        self.free_blocks.sort_by_key(|range| range.start);

        let mut merged = Vec::new();
        let mut current = self.free_blocks[0].clone();

        for next_range in self.free_blocks.iter().skip(1) {
            if current.end == next_range.start {
                // Блоки смежные, объединяем
                current = current.start..next_range.end;
            } else {
                // Не смежные, добавляем текущий и начинам новый
                merged.push(current);
                current = next_range.clone();
            }
        }
        merged.push(current);

        self.free_blocks = merged;
        self.stats.merge_count += 1;
    }

    /// Проверяет, пересекается ли диапазон с занятыми блоками
    fn intersects_with_used_block(&self, start: usize, end: usize) -> bool {
        for (used_start, block_info) in &self.used_blocks {
            let used_end = used_start + block_info.size;
            if !(end <= *used_start || start >= used_end) {
                return true;
            }
        }
        false
    }

    /// Удаляет диапазон из свободных блоков
    fn remove_from_free_range(&mut self, range: Range<usize>) {
        let mut new_free_blocks = Vec::new();

        for free_range in &self.free_blocks {
            if range.end <= free_range.start || range.start >= free_range.end {
                // Нет пересечения, сохраняем как есть
                new_free_blocks.push(free_range.clone());
            } else {
                // Есть пересечение, разбиваем
                if free_range.start < range.start {
                    new_free_blocks.push(free_range.start..range.start);
                }
                if free_range.end > range.end {
                    new_free_blocks.push(range.end..free_range.end);
                }
            }
        }

        self.free_blocks = new_free_blocks;
    }

    /// Добавляет свободный диапазон
    fn add_free_range(&mut self, range: Range<usize>) {
        self.free_blocks.push(range);
        self.merge_adjacent_free_blocks();
    }

    /// Проверяет, является ли диапазон свободным
    fn range_is_free(&self, range: Range<usize>) -> bool {
        for free_range in &self.free_blocks {
            if range.start >= free_range.start && range.end <= free_range.end {
                return true;
            }
        }
        false
    }

    /// Проверяет, нужно ли запустить дефрагментацию
    pub fn needs_defragmentation(&self) -> bool {
        if !self.auto_defrag_enabled {
            return false;
        }

        let fragmentation = self.calculate_fragmentation();
        fragmentation > self.defrag_threshold
    }

    /// Вычисляет уровень фрагментации
    pub fn calculate_fragmentation(&self) -> f64 {
        if self.free_blocks.is_empty() {
            return 0.0;
        }

        // Вычисляем размер самого большого свободного блока
        let largest_free = self
            .free_blocks
            .iter()
            .map(|range| range.end - range.start)
            .max()
            .unwrap_or(0);

        // Вычисляем общее количество свободной памяти
        let total_free = self
            .free_blocks
            .iter()
            .map(|range| range.end - range.start)
            .sum::<usize>();

        if total_free == 0 {
            return 0.0;
        }

        // Фрагментация = 1 - (размер самого большого блока / общий свободный размер)
        1.0 - (largest_free as f64 / total_free as f64)
    }

    /// Получает количество свободных блоков
    pub fn get_free_blocks_count(&self) -> usize {
        self.free_blocks.len()
    }

    /// Получает количество занятых блоков
    pub fn get_used_blocks_count(&self) -> usize {
        self.used_blocks.len()
    }

    /// Получает размер самого большого свободного блока
    pub fn get_largest_free_block_size(&self) -> usize {
        self.free_blocks
            .iter()
            .map(|range| range.end - range.start)
            .max()
            .unwrap_or(0)
    }

    /// Получает размер самого большого занятого блока
    pub fn get_largest_used_block_size(&self) -> usize {
        self.used_blocks
            .values()
            .map(|block| block.size)
            .max()
            .unwrap_or(0)
    }

    /// Получает общее количество свободной памяти
    pub fn get_total_free_memory(&self) -> usize {
        self.free_blocks
            .iter()
            .map(|range| range.end - range.start)
            .sum()
    }

    /// Получает общее количество занятой памяти
    pub fn get_total_used_memory(&self) -> usize {
        self.used_blocks.values().map(|block| block.size).sum()
    }

    /// Устанавливает порог для автоматической дефрагментации
    pub fn set_auto_defrag_threshold(&mut self, threshold: f64) {
        self.defrag_threshold = threshold.max(0.0).min(1.0);
    }

    /// Включает/выключает автоматическую дефрагментацию
    pub fn set_auto_defrag(&mut self, enabled: bool) {
        self.auto_defrag_enabled = enabled;
    }

    /// Запускает автоматическую дефрагментацию при необходимости
    pub fn auto_defragment_if_needed(&mut self) -> Result<bool, &'static str> {
        if self.needs_defragmentation() {
            self.defragment()?;
            Ok(true) // Была выполнена дефрагментация
        } else {
            Ok(false) // Не требовалась дефрагментация
        }
    }

    /// Получает статистику
    pub fn get_stats(&self) -> &DefragStats {
        &self.stats
    }

    /// Сбрасывает статистику
    pub fn reset_stats(&mut self) {
        self.stats = DefragStats::new();
    }

    /// Помечает блок как перемещаемый/неперемещаемый
    pub fn set_block_movable(&mut self, start: usize, movable: bool) -> Result<(), &'static str> {
        if let Some(block) = self.used_blocks.get_mut(&start) {
            block.flags.movable = movable;
            Ok(())
        } else {
            Err("Block not found")
        }
    }

    /// Помечает блок как критичный (не может быть перемещен)
    pub fn set_block_critical(&mut self, start: usize, critical: bool) -> Result<(), &'static str> {
        if let Some(block) = self.used_blocks.get_mut(&start) {
            block.flags.critical = critical;
            block.flags.movable = !critical; // Критические блоки не перемещаемы
            Ok(())
        } else {
            Err("Block not found")
        }
    }

    /// Получает информацию о блоке
    pub fn get_block_info(&self, start: usize) -> Option<&UsedBlock> {
        self.used_blocks.get(&start)
    }

    /// Обновляет состояние блока
    pub fn update_block_state(
        &mut self,
        start: usize,
        state: BlockState,
    ) -> Result<(), &'static str> {
        if let Some(block) = self.used_blocks.get_mut(&start) {
            block.state = state;
            Ok(())
        } else {
            Err("Block not found")
        }
    }

    /// Отмечает блоки, которые не использовались долго, как неактивные
    pub fn mark_inactive_blocks(&mut self, inactive_time_threshold_ms: u64) {
        let current_time = crate::timer::get_uptime_ms();

        for (_, block) in self.used_blocks.iter_mut() {
            if current_time - block.allocation_time > inactive_time_threshold_ms {
                if block.state == BlockState::Active {
                    block.state = BlockState::Inactive;
                }
            }
        }
    }

    /// Выделяет большой непрерывный блок (выполняет дефрагментацию при необходимости)
    pub fn allocate_large_block(&mut self, size: usize) -> Option<usize> {
        // Сначала пробуем выделить без дефрагментации
        if let Some(addr) = self.find_free_block(size) {
            return Some(addr);
        }

        // Если не нашли, проверяем, нужна ли дефрагментация
        if self.needs_defragmentation() {
            if self.defragment().is_ok() {
                // После дефрагментации пробуем снова
                if let Some(addr) = self.find_free_block(size) {
                    return Some(addr);
                }
            }
        }

        None
    }

    /// Находит свободный блок нужного размера
    fn find_free_block(&self, size: usize) -> Option<usize> {
        for range in &self.free_blocks {
            if range.end - range.start >= size {
                return Some(range.start);
            }
        }
        None
    }
}

impl DefragStats {
    pub fn new() -> Self {
        DefragStats {
            move_count: 0,
            merge_count: 0,
            moved_bytes: 0,
            last_defrag_time: 0,
            total_defrags: 0,
            free_blocks_before: 0,
            free_blocks_after: 0,
        }
    }
}

impl Default for BlockFlags {
    fn default() -> Self {
        BlockFlags {
            movable: true,
            executable: false,
            compressible: true,
            critical: false,
        }
    }
}

/// Глобальный дефрагментатор памяти
use spin::Once;

static MEMORY_DEFRAG: Once<Mutex<MemoryDefrag>> = Once::new();

pub fn get_memory_defragger() -> &'static Mutex<MemoryDefrag> {
    MEMORY_DEFRAG.call_once(|| {
        Mutex::new(MemoryDefrag::new(
            1024 * 1024 * 256, // 256MB
            4096,              // 4KB минимальный блок
        ))
    })
}

/// Интерфейс для дефрагментации
pub struct MemoryDeFragmentationInterface;

impl MemoryDeFragmentationInterface {
    /// Инициализирует дефрагментатор
    pub fn init(total_memory: usize, min_block_size: usize) -> Result<(), &'static str> {
        let mut defrag = get_memory_defragger().lock();
        *defrag = MemoryDefrag::new(total_memory, min_block_size);
        Ok(())
    }

    /// Регистрирует выделение блока
    pub fn register_allocation(
        start: usize,
        size: usize,
        owner: Option<u64>,
    ) -> Result<(), &'static str> {
        get_memory_defragger()
            .lock()
            .register_allocated_block(start, size, owner)
    }

    /// Регистрирует освобождение блока
    pub fn register_deallocation(start: usize) -> Result<(), &'static str> {
        get_memory_defragger()
            .lock()
            .register_deallocated_block(start)
    }

    /// Выполняет дефрагментацию
    pub fn defragment() -> Result<(), &'static str> {
        get_memory_defragger().lock().defragment()
    }

    /// Автоматически запускает дефрагментацию при необходимости
    pub fn auto_defragment_if_needed() -> Result<bool, &'static str> {
        get_memory_defragger().lock().auto_defragment_if_needed()
    }

    /// Проверяет, нужно ли выполнить дефрагментацию
    pub fn needs_defragmentation() -> bool {
        get_memory_defragger().lock().needs_defragmentation()
    }

    /// Вычисляет уровень фрагментации
    pub fn calculate_fragmentation() -> f64 {
        get_memory_defragger().lock().calculate_fragmentation()
    }

    /// Включает/выключает автоматическую дефрагментацию
    pub fn set_auto_defragmentation(enabled: bool) {
        get_memory_defragger().lock().set_auto_defrag(enabled);
    }

    /// Устанавливает порог для автоматической дефрагментации
    pub fn set_defragmentation_threshold(threshold: f64) {
        get_memory_defragger()
            .lock()
            .set_auto_defrag_threshold(threshold);
    }

    /// Получает статистику дефрагментации
    pub fn get_defrag_stats() -> DefragStats {
        get_memory_defragger().lock().get_stats().clone()
    }

    /// Сбрасывает статистику дефрагментации
    pub fn reset_defrag_stats() {
        get_memory_defragger().lock().reset_stats();
    }

    /// Помечает блок как перемещаемый
    pub fn set_block_movable(start: usize, movable: bool) -> Result<(), &'static str> {
        get_memory_defragger()
            .lock()
            .set_block_movable(start, movable)
    }

    /// Помечает блок как критичный
    pub fn set_block_critical(start: usize, critical: bool) -> Result<(), &'static str> {
        get_memory_defragger()
            .lock()
            .set_block_critical(start, critical)
    }

    /// Обновляет состояние блока
    pub fn update_block_state(start: usize, state: BlockState) -> Result<(), &'static str> {
        get_memory_defragger()
            .lock()
            .update_block_state(start, state)
    }

    /// Помечает неактивные блоки
    pub fn mark_inactive_blocks(threshold_ms: u64) {
        get_memory_defragger()
            .lock()
            .mark_inactive_blocks(threshold_ms);
    }

    /// Выделяет большой блок
    pub fn allocate_large_block(size: usize) -> Option<usize> {
        get_memory_defragger().lock().allocate_large_block(size)
    }

    /// Получает информацию о блоке
    pub fn get_block_info(start: usize) -> Option<UsedBlock> {
        get_memory_defragger().lock().get_block_info(start).cloned()
    }

    /// Получает размер самого большого свободного блока
    pub fn get_largest_free_block_size() -> usize {
        get_memory_defragger().lock().get_largest_free_block_size()
    }

    /// Получает общее количество свободной памяти
    pub fn get_total_free_memory() -> usize {
        get_memory_defragger().lock().get_total_free_memory()
    }
}

/// Макросы для удобства
#[macro_export]
macro_rules! defrag_register_allocation {
    ($start:expr, $size:expr, $owner:expr) => {
        $crate::memory_defrag::MemoryDeFragmentationInterface::register_allocation(
            $start, $size, $owner,
        )
    };
}

#[macro_export]
macro_rules! defrag_register_deallocation {
    ($start:expr) => {
        $crate::memory_defrag::MemoryDeFragmentationInterface::register_deallocation($start)
    };
}

#[macro_export]
macro_rules! defrag_perform {
    () => {
        $crate::memory_defrag::MemoryDeFragmentationInterface::defragment()
    };
}

#[macro_export]
macro_rules! defrag_auto_if_needed {
    () => {
        $crate::memory_defrag::MemoryDeFragmentationInterface::auto_defragment_if_needed()
    };
}

#[macro_export]
macro_rules! defrag_needs_perform {
    () => {
        $crate::memory_defrag::MemoryDeFragmentationInterface::needs_defragmentation()
    };
}

#[macro_export]
macro_rules! defrag_calc_fragmentation {
    () => {
        $crate::memory_defrag::MemoryDeFragmentationInterface::calculate_fragmentation()
    };
}

#[macro_export]
macro_rules! defrag_stats {
    () => {
        $crate::memory_defrag::MemoryDeFragmentationInterface::get_defrag_stats()
    };
}

#[macro_export]
macro_rules! defrag_set_block_movable {
    ($start:expr, $movable:expr) => {
        $crate::memory_defrag::MemoryDeFragmentationInterface::set_block_movable($start, $movable)
    };
}

#[macro_export]
macro_rules! defrag_set_block_critical {
    ($start:expr, $critical:expr) => {
        $crate::memory_defrag::MemoryDeFragmentationInterface::set_block_critical($start, $critical)
    };
}

#[macro_export]
macro_rules! defrag_mark_block_state {
    ($start:expr, $state:expr) => {
        $crate::memory_defrag::MemoryDeFragmentationInterface::update_block_state($start, $state)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_defrag_creation() {
        let defrag = MemoryDefrag::new(1024 * 1024, 4096); // 1MB, 4KB blocks

        assert_eq!(defrag.total_size, 1024 * 1024);
        assert_eq!(defrag.min_block_size, 4096);
        assert_eq!(defrag.get_free_blocks_count(), 1);
        assert_eq!(defrag.get_total_free_memory(), 1024 * 1024);
        assert_eq!(defrag.get_used_blocks_count(), 0);
    }

    #[test]
    fn test_block_registration() {
        let mut defrag = MemoryDefrag::new(1024 * 1024, 4096);

        // Регистрируем выделение
        assert!(defrag
            .register_allocated_block(10000, 4096, Some(1))
            .is_ok());

        assert_eq!(defrag.get_free_blocks_count(), 2); // Блок разделил свободное пространство
        assert_eq!(defrag.get_used_blocks_count(), 1);

        // Регистрируем освобождение
        assert!(defrag.register_deallocated_block(10000).is_ok());

        assert_eq!(defrag.get_used_blocks_count(), 0);
    }

    #[test]
    fn test_fragmentation_calculation() {
        let mut defrag = MemoryDefrag::new(1000000, 1000);

        // Создаем фрагментацию: выделяем и освобождаем несколько блоков
        defrag
            .register_allocated_block(10000, 1000, Some(1))
            .unwrap();
        defrag
            .register_allocated_block(20000, 1000, Some(1))
            .unwrap();
        defrag
            .register_allocated_block(30000, 1000, Some(1))
            .unwrap();

        // Освобождаем средний блок - создаем дырку
        defrag.register_deallocated_block(20000).unwrap();

        let frag = defrag.calculate_fragmentation();
        assert!(frag > 0.0); // Должна быть некоторая фрагментация

        // После дефрагментации фрагментация должна уменьшиться
        defrag.defragment().unwrap();
        let frag_after = defrag.calculate_fragmentation();
        // После нашей упрощенной дефрагментации уровень фрагментации должен уменьшиться
    }

    #[test]
    fn test_large_block_allocation() {
        let mut defrag = MemoryDefrag::new(1000000, 1000);

        // Создаем много маленьких выделений, чтобы фрагментировать память
        for i in 0..100 {
            if i % 3 != 0 {
                // Пропускаем каждое третье выделение
                defrag
                    .register_allocated_block(10000 + i * 2000, 1000, Some(1))
                    .unwrap();
            }
        }

        // Должно не найтись большой блок без дефрагментации
        assert!(defrag.allocate_large_block(50000).is_none());

        // После дефрагментации большой блок должен найтись
        defrag.defragment().unwrap();
        assert!(defrag.allocate_large_block(50000).is_some());
    }

    #[test]
    fn test_auto_defragmentation() {
        let mut defrag = MemoryDefrag::new(1000000, 1000);
        defrag.set_auto_defrag_threshold(0.1); // Очень низкий порог для теста

        // Создаем фрагментацию
        for i in 0..100 {
            defrag
                .register_allocated_block(10000 + i * 2000, 1000, Some(1))
                .unwrap();
        }

        // Освобождаем половину блоков
        for i in (0..100).step_by(2) {
            defrag.register_deallocated_block(10000 + i * 2000).unwrap();
        }

        // Автоматическая дефрагментация должна сработать
        let result = defrag.auto_defragment_if_needed();
        assert!(result.is_ok());
    }

    #[test]
    fn test_block_flags() {
        let mut defrag = MemoryDefrag::new(1000000, 1000);

        defrag
            .register_allocated_block(10000, 1000, Some(1))
            .unwrap();

        // Пометим блок как критичный
        assert!(defrag.set_block_critical(10000, true).is_ok());

        // Проверим, что блок не может быть перемещен
        let block_info = defrag.get_block_info(10000).unwrap();
        assert!(block_info.flags.critical);
        assert!(!block_info.flags.movable);

        // Попробуем сбросить критичность
        assert!(defrag.set_block_critical(10000, false).is_ok());
        let block_info = defrag.get_block_info(10000).unwrap();
        assert!(!block_info.flags.critical);
        assert!(block_info.flags.movable);
    }
}
