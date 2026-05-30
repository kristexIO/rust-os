use alloc::{collections::BTreeMap, vec::Vec};
use core::sync::atomic::{AtomicU32, Ordering};
use spin::{Mutex, MutexGuard};

/// PriorityInheritanceManager - менеджер наследования приоритетов
///
/// Решает проблему инверсии приоритетов, когда высокоприоритетная задача
/// блокируется низкоприоритетной задачей, удерживающей ресурс.
pub struct PriorityInheritanceManager {
    /// Карта ресурсов и их владельцев
    resource_owners: BTreeMap<ResourceId, TaskId>,
    /// Карта ресурсов и ожидающих их задач
    resource_waiters: BTreeMap<ResourceId, Vec<TaskId>>,
    /// Приоритеты задач
    task_priorities: BTreeMap<TaskId, Priority>,
    /// Флаги, показывающие, были ли приоритеты изменены
    inherited_priorities: BTreeMap<TaskId, Priority>,
    /// Счетчики заблокированных ресурсов
    blocked_resources: BTreeMap<ResourceId, usize>,
}

/// Идентификатор задачи
pub type TaskId = u64;

/// Идентификатор ресурса
pub type ResourceId = u64;

/// Приоритет задачи
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Priority(pub u32);

impl Priority {
    pub fn new(level: u32) -> Self {
        Priority(level)
    }

    pub fn numeric(&self) -> u32 {
        self.0
    }
}

impl PriorityInheritanceManager {
    /// Создает новый менеджер наследования приоритетов
    pub fn new() -> Self {
        PriorityInheritanceManager {
            resource_owners: BTreeMap::new(),
            resource_waiters: BTreeMap::new(),
            task_priorities: BTreeMap::new(),
            inherited_priorities: BTreeMap::new(),
            blocked_resources: BTreeMap::new(),
        }
    }

    /// Устанавливает исходный приоритет задачи
    pub fn set_task_priority(&mut self, task_id: TaskId, priority: Priority) {
        self.task_priorities.insert(task_id, priority);
    }

    /// Задача пытается захватить ресурс, но он уже занят
    pub fn task_attempting_resource_access(
        &mut self,
        task_id: TaskId,
        resource_id: ResourceId,
    ) -> Result<(), &'static str> {
        // Проверяем, занят ли ресурс
        if let Some(owner_id) = self.resource_owners.get(&resource_id) {
            // Ресурс занят другой задачей
            if *owner_id != task_id {
                // Добавляем задачу в список ожидающих
                self.resource_waiters
                    .entry(resource_id)
                    .or_insert_with(Vec::new)
                    .push(task_id);

                // Проверяем приоритеты: если задача имеет более высокий приоритет
                let task_priority = self
                    .task_priorities
                    .get(&task_id)
                    .unwrap_or(&Priority::new(0));
                let owner_priority = self
                    .task_priorities
                    .get(owner_id)
                    .unwrap_or(&Priority::new(0));

                if task_priority > owner_priority {
                    // Нужно повысить приоритет владельца
                    self.boost_priority(*owner_id, *task_priority)?;
                }

                // Увеличиваем счетчик заблокированных ресурсов
                *self.blocked_resources.entry(resource_id).or_insert(0) += 1;

                return Ok(());
            }
        }

        // Ресурс не занят, задача может захватить его
        self.resource_owners.insert(resource_id, task_id);
        Ok(())
    }

    /// Задача освобождает ресурс
    pub fn task_releasing_resource(
        &mut self,
        task_id: TaskId,
        resource_id: ResourceId,
    ) -> Result<(), &'static str> {
        // Проверяем, что задача действительно владеет ресурсом
        if let Some(owner_id) = self.resource_owners.get(&resource_id) {
            if *owner_id == task_id {
                // Удаляем владельца
                self.resource_owners.remove(&resource_id);

                // Проверяем, есть ли кто-то, кто ждет ресурс
                if let Some(waiters) = self.resource_waiters.get(&resource_id) {
                    if !waiters.is_empty() {
                        // Передаем ресурс одному из ожидающих
                        let next_owner = waiters[0];

                        // Удаляем нового владельца из списка ожидания
                        let mut waiters = self.resource_waiters.get_mut(&resource_id).unwrap();
                        waiters.retain(|&id| id != next_owner);

                        // Назначаем нового владельца
                        self.resource_owners.insert(resource_id, next_owner);

                        // Уменьшаем счетчик заблокированных ресурсов
                        if let Some(count) = self.blocked_resources.get_mut(&resource_id) {
                            *count = count.saturating_sub(1);
                        }

                        // Возможно, нужно понизить приоритет бывшего владельца
                        self.restore_original_priority(task_id)?;
                    }

                    if self.resource_waiters[&resource_id].is_empty() {
                        self.resource_waiters.remove(&resource_id);
                    }
                }
            }
        }

        Ok(())
    }

    /// Повышает приоритет задачи до указанного уровня (для наследования приоритетов)
    fn boost_priority(
        &mut self,
        task_id: TaskId,
        new_priority: Priority,
    ) -> Result<(), &'static str> {
        // Сохраняем текущий приоритет (если не сохранен ранее)
        if !self.inherited_priorities.contains_key(&task_id) {
            if let Some(&original_priority) = self.task_priorities.get(&task_id) {
                self.inherited_priorities.insert(task_id, original_priority);
            }
        }

        // Устанавливаем новый приоритет
        self.task_priorities.insert(task_id, new_priority);

        Ok(())
    }

    /// Восстанавливает оригинальный приоритет задачи
    fn restore_original_priority(&mut self, task_id: TaskId) -> Result<(), &'static str> {
        // Проверяем, был ли приоритет повышен из-за наследования
        if let Some(original_priority) = self.inherited_priorities.remove(&task_id) {
            // Восстанавливаем оригинальный приоритет
            self.task_priorities.insert(task_id, original_priority);
        }

        Ok(())
    }

    /// Получает текущий приоритет задачи
    pub fn get_task_priority(&self, task_id: TaskId) -> Priority {
        *self
            .task_priorities
            .get(&task_id)
            .unwrap_or(&Priority::new(0))
    }

    /// Проверяет, есть ли ресурс в системе
    pub fn is_resource_available(&self, resource_id: ResourceId) -> bool {
        !self.resource_owners.contains_key(&resource_id)
    }

    /// Получает владельца ресурса
    pub fn get_resource_owner(&self, resource_id: ResourceId) -> Option<TaskId> {
        self.resource_owners.get(&resource_id).copied()
    }

    /// Получает список ожидающих задач для ресурса
    pub fn get_resource_waiters(&self, resource_id: ResourceId) -> Option<&Vec<TaskId>> {
        self.resource_waiters.get(&resource_id)
    }

    /// Сбрасывает наследование приоритетов для задачи
    pub fn reset_task_priority_inheritance(&mut self, task_id: TaskId) -> Result<(), &'static str> {
        if let Some(original_priority) = self.inherited_priorities.remove(&task_id) {
            self.task_priorities.insert(task_id, original_priority);
        }

        // Очищаем задачу из всех списков ожидания
        let resources_to_clean: Vec<_> = self
            .resource_waiters
            .iter()
            .filter(|(_, waiters)| waiters.contains(&task_id))
            .map(|(&resource_id, _)| resource_id)
            .collect();

        for resource_id in resources_to_clean {
            if let Some(waiters) = self.resource_waiters.get_mut(&resource_id) {
                waiters.retain(|&id| id != task_id);
                if waiters.is_empty() {
                    self.resource_waiters.remove(&resource_id);
                }
            }
        }

        Ok(())
    }
}

/// Обертка для мьютекса с поддержкой наследования приоритетов
pub struct PriorityInheritedMutex<T: ?Sized> {
    /// Внутренняя защита данных
    data: spin::Mutex<T>,
    /// Идентификатор ресурса (для наследования приоритетов)
    resource_id: ResourceId,
    /// Ссылка на менеджер наследования приоритетов
    priority_manager: &'static Mutex<PriorityInheritanceManager>,
}

/// Охранник мьютекса с наследованием приоритетов
pub struct PriorityInheritedMutexGuard<'a, T: ?Sized + 'a> {
    guard: MutexGuard<'a, T>,
    resource_id: ResourceId,
    task_id: TaskId,
    priority_manager: &'a Mutex<PriorityInheritanceManager>,
}

impl<T> PriorityInheritedMutex<T> {
    /// Создает новый мьютекс с поддержкой наследования приоритетов
    pub fn new(
        value: T,
        resource_id: ResourceId,
        priority_manager: &'static Mutex<PriorityInheritanceManager>,
    ) -> Self {
        PriorityInheritedMutex {
            data: spin::Mutex::new(value),
            resource_id,
            priority_manager,
        }
    }

    /// Блокирует мьютекс
    pub fn lock(&self, task_id: TaskId) -> Result<PriorityInheritedMutexGuard<T>, &'static str> {
        // Проверяем, можно ли захватить ресурс
        self.priority_manager
            .lock()
            .task_attempting_resource_access(task_id, self.resource_id)?;

        // Захватываем внутреннюю блокировку
        let guard = self.data.lock();

        Ok(PriorityInheritedMutexGuard {
            guard,
            resource_id: self.resource_id,
            task_id,
            priority_manager: self.priority_manager,
        })
    }

    /// Пытается заблокировать мьютекс
    pub fn try_lock(
        &self,
        task_id: TaskId,
    ) -> Result<PriorityInheritedMutexGuard<T>, &'static str> {
        // Проверяем, можно ли захватить ресурс
        self.priority_manager
            .lock()
            .task_attempting_resource_access(task_id, self.resource_id)?;

        // Пытаемся захватить внутреннюю блокировку
        if let Some(guard) = self.data.try_lock() {
            Ok(PriorityInheritedMutexGuard {
                guard,
                resource_id: self.resource_id,
                task_id,
                priority_manager: self.priority_manager,
            })
        } else {
            // Не удалось захватить, восстанавливаем состояние
            let mut pm = self.priority_manager.lock();
            if let Some(waiters) = pm.resource_waiters.get_mut(&self.resource_id) {
                waiters.retain(|&id| id != task_id);
                if waiters.is_empty() {
                    pm.resource_waiters.remove(&self.resource_id);
                }
            }
            Err("Resource already locked")
        }
    }
}

impl<T: ?Sized> Deref for PriorityInheritedMutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<T: ?Sized> DerefMut for PriorityInheritedMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

impl<T: ?Sized> Drop for PriorityInheritedMutexGuard<'_, T> {
    fn drop(&mut self) {
        // Освобождаем ресурс
        let _ = self
            .priority_manager
            .lock()
            .task_releasing_resource(self.task_id, self.resource_id);
    }
}

/// Интерфейс для работы с наследованием приоритетов
pub struct PriorityInheritanceInterface;

impl PriorityInheritanceInterface {
    /// Инициализирует систему наследования приоритетов
    pub fn init() -> &'static Mutex<PriorityInheritanceManager> {
        use spin::Once;
        static PRIORITY_INHERITANCE_MANAGER: Once<Mutex<PriorityInheritanceManager>> = Once::new();

        PRIORITY_INHERITANCE_MANAGER.call_once(|| Mutex::new(PriorityInheritanceManager::new()))
    }

    /// Устанавливает приоритет задачи
    pub fn set_task_priority(
        manager: &Mutex<PriorityInheritanceManager>,
        task_id: TaskId,
        priority: Priority,
    ) {
        manager.lock().set_task_priority(task_id, priority);
    }

    /// Проверяет, есть ли инверсия приоритетов
    pub fn detect_priority_inversion(manager: &Mutex<PriorityInheritanceManager>) -> bool {
        // Простая проверка: есть ли задачи с пониженным приоритетом из-за ожидания
        !manager.lock().blocked_resources.is_empty()
    }

    /// Получает статистику по наследованию приоритетов
    pub fn get_inheritance_statistics(
        manager: &Mutex<PriorityInheritanceManager>,
    ) -> PriorityInheritanceStats {
        let pm = manager.lock();
        PriorityInheritanceStats {
            total_resources: pm.resource_owners.len(),
            total_waiters: pm.resource_waiters.values().map(Vec::len).sum(),
            total_inherited_priorities: pm.inherited_priorities.len(),
            total_blocked_resources: pm.blocked_resources.values().sum(),
        }
    }
}

/// Статистика по наследованию приоритетов
pub struct PriorityInheritanceStats {
    /// Общее количество ресурсов
    pub total_resources: usize,
    /// Общее количество ожидающих задач
    pub total_waiters: usize,
    /// Общее количество задач с наследованными приоритетами
    pub total_inherited_priorities: usize,
    /// Общее количество заблокированных ресурсов
    pub total_blocked_resources: usize,
}

/// Утилита для анализа зависимостей задач
pub struct TaskDependencyAnalyzer {
    /// Карта зависимостей: задача -> список ресурсов, которые она удерживает
    holding_resources: BTreeMap<TaskId, Vec<ResourceId>>,
    /// Карта зависимостей: задача -> список задач, которые она блокирует
    blocking_tasks: BTreeMap<TaskId, Vec<TaskId>>,
}

impl TaskDependencyAnalyzer {
    pub fn new() -> Self {
        TaskDependencyAnalyzer {
            holding_resources: BTreeMap::new(),
            blocking_tasks: BTreeMap::new(),
        }
    }

    /// Регистрирует, что задача захватила ресурс
    pub fn register_resource_acquisition(&mut self, task_id: TaskId, resource_id: ResourceId) {
        self.holding_resources
            .entry(task_id)
            .or_insert_with(Vec::new)
            .push(resource_id);
    }

    /// Регистрирует, что задача освободила ресурс
    pub fn register_resource_release(&mut self, task_id: TaskId, resource_id: ResourceId) {
        if let Some(resources) = self.holding_resources.get_mut(&task_id) {
            resources.retain(|&id| id != resource_id);
            if resources.is_empty() {
                self.holding_resources.remove(&task_id);
            }
        }
    }

    /// Регистрирует, что задача блокирует другую задачу
    pub fn register_blockage(&mut self, blocker: TaskId, blocked: TaskId) {
        self.blocking_tasks
            .entry(blocker)
            .or_insert_with(Vec::new)
            .push(blocked);
    }

    /// Получает список задач, которых блокирует указанная задача
    pub fn get_blocking_tasks(&self, task_id: TaskId) -> Option<&Vec<TaskId>> {
        self.blocking_tasks.get(&task_id)
    }

    /// Получает список ресурсов, которые удерживает задача
    pub fn get_held_resources(&self, task_id: TaskId) -> Option<&Vec<ResourceId>> {
        self.holding_resources.get(&task_id)
    }

    /// Обнаруживает циклы зависимостей (потенциальные взаимоблокировки)
    pub fn detect_deadlock_cycles(&self) -> Vec<Vec<TaskId>> {
        let mut cycles = Vec::new();

        // Простой алгоритм для обнаружения циклов
        for &task in self.blocking_tasks.keys() {
            let mut visited = BTreeMap::new();
            let mut path = Vec::new();

            if self.has_cycle(task, &mut visited, &mut path, &mut Vec::new()) {
                cycles.push(path);
            }
        }

        cycles
    }

    /// Вспомогательная функция для обнаружения циклов
    fn has_cycle(
        &self,
        current: TaskId,
        visited: &mut BTreeMap<TaskId, bool>,
        path: &mut Vec<TaskId>,
        recursion_stack: &mut Vec<TaskId>,
    ) -> bool {
        if let Some(&done) = visited.get(&current) {
            if done {
                return false;
            }
        }

        visited.insert(current, false);
        path.push(current);
        recursion_stack.push(current);

        if let Some(blocked_tasks) = self.blocking_tasks.get(&current) {
            for &task in blocked_tasks {
                if !visited.contains_key(&task) {
                    if self.has_cycle(task, visited, path, recursion_stack) {
                        return true;
                    }
                } else if let Some(pos) = recursion_stack.iter().position(|&x| x == task) {
                    // Нашли цикл
                    path.extend_from_slice(&recursion_stack[pos..]);
                    return true;
                }
            }
        }

        recursion_stack.pop();
        visited.insert(current, true);
        true
    }
}

/// Макросы для удобства
#[macro_export]
macro_rules! priority_mutex {
    ($val:expr, $res_id:expr) => {
        $crate::priority_inheritance::PriorityInheritedMutex::new(
            $val,
            $res_id,
            $crate::priority_inheritance::PriorityInheritanceInterface::init(),
        )
    };
}

#[macro_export]
macro_rules! with_priority_mutex {
    ($mutex:expr, $task_id:expr, |$guard:ident| $body:block) => {
        if let Ok($guard) = $mutex.lock($task_id) {
            $body
        }
    };
}

#[macro_export]
macro_rules! priority_inheritance_init {
    () => {
        $crate::priority_inheritance::PriorityInheritanceInterface::init()
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_inheritance() {
        let mut manager = PriorityInheritanceManager::new();

        // Устанавливаем приоритеты задач
        manager.set_task_priority(1, Priority::new(1)); // Низкий приоритет
        manager.set_task_priority(2, Priority::new(5)); // Высокий приоритет

        // Задача 1 захватывает ресурс
        manager.task_attempting_resource_access(1, 100).unwrap();

        // Задача 2 пытается захватить тот же ресурс
        manager.task_attempting_resource_access(2, 100).unwrap();

        // Приоритет задачи 1 должен быть повышен до уровня задачи 2
        assert_eq!(manager.get_task_priority(1).numeric(), 5);

        // Задача 1 освобождает ресурс
        manager.task_releasing_resource(1, 100).unwrap();

        // Приоритет задачи 1 должен быть восстановлен
        assert_eq!(manager.get_task_priority(1).numeric(), 1);
    }
}
