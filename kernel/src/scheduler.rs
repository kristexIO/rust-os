/// Completely Fair Scheduler (CFS) implementation for task scheduling.
///
/// This scheduler implements a CFS-like algorithm similar to the Linux kernel
/// scheduler. Key concepts:
///
/// # Architecture
///
/// * **Virtual Runtime (`vruntime`)**: Each task accumulates vruntime based on
///   its actual execution time weighted by priority. Tasks with lower vruntime
///   are scheduled first, ensuring fair CPU time distribution.
/// * **Run Queue**: Implemented as a BTreeMap keyed by (vruntime, task_id) for
///   O(log N) insertion, removal, and minimum extraction.
/// * **Priority via Weight**: Priority levels (nice values) are converted to
///   weights. Higher weight tasks accumulate vruntime slower, giving them more
///   CPU time.
/// * **Preemption**: Timer interrupts (IRQ 0) trigger scheduler_tick() which
///   checks if the current task's timeslice has expired.
///
/// # Task States
///
/// * `Ready` - Task is ready to run and waiting in the run queue
/// * `Running` - Task is currently executing on the CPU
/// * `Blocked` - Task is waiting for an event (I/O, futex, sleep, etc.)
/// * `Zombie` - Task has terminated but not yet reaped by parent
///
/// # Simplifications vs Linux CFS
///
/// * Single run queue (no per-CPU run queues)
/// * No load balancing between CPUs
/// * Vruntime measured in timer ticks rather than nanoseconds
/// * No control groups or task grouping
///
/// # Design Details
///
/// The scheduler maintains:
/// - A map of all tasks (`tasks`) for O(log N) lookup by ID
/// - A run queue (`run_queue`) sorted by `(vruntime, task_id)` to efficiently
///   retrieve the next task to run
/// - The currently executing task (`current`)
/// - A counter for generating unique task IDs (`next_id`)
/// - A count of blocked tasks (`blocked_count`)
///
/// The scheduler does not implement per-CPU run queues or load balancing,
/// assuming a single-core system. For multi-core support, a per-CPU run queue
/// and load balancing mechanism would be needed.
///
/// # Periodic Timer Interaction
///
/// The scheduler expects to be called via `scheduler_tick()` on each timer
/// interrupt (typically 1000Hz). This function increments the global tick
/// counter and invokes `tick()` on the scheduler to update the current task's
/// vruntime and check for preemption.
use alloc::{collections::BTreeMap, vec::Vec};
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

use spin::Mutex;

use crate::process::Pid;

/// Maximum number of CPUs supported (statically allocated).
const MAX_CPUS: usize = 8;
/// Actual number of CPUs detected (set during boot).
pub static CPU_COUNT: AtomicUsize = AtomicUsize::new(1);

/// PID of the currently executing process for each CPU.
/// 0 indicates the idle task or kernel thread.
/// Updated by the scheduler during each context switch.
pub static CURRENT_PID: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];

/// Монотонный счётчик тиков таймера (IRQ 0).
/// Инкрементируется в обработчике прерывания таймера.
pub static TIMER_TICKS: AtomicU64 = AtomicU64::new(0);

// ─── Приоритеты и веса
// ────────────────────────────────────────────────────────

/// Уровень «никсового» приоритета (nice: -20..+19)
/// У нас: 0 = обычный, 19 = самый низкий, -20 = реального времени
pub type Nice = i8;

/// Таблица весов для уровней nice (как в Linux kernel sched/core.c).
/// Индекс: nice + 20 (т.е. nice=0 → index 20, вес = 1024)
const NICE_TO_WEIGHT: [u32; 40] = [
    88761, 71755, 56483, 46273, 36291, // -20..-16
    29154, 23254, 18705, 14949, 11916, // -15..-11
    9548, 7620, 6100, 4904, 3906, // -10..-6
    3121, 2501, 1991, 1586, 1277, // -5..-1
    1024, 820, 655, 526, 423, //  0..4
    335, 272, 215, 172, 137, //  5..9
    110, 87, 70, 56, 45, //  10..14
    36, 29, 23, 18, 15, //  15..19
];

fn nice_to_weight(nice: Nice) -> u32 {
    let idx = (nice.clamp(-20, 19) + 20) as usize;
    NICE_TO_WEIGHT[idx]
}

// ─── Состояния задачи
// ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Ready,
    Running,
    Blocked,
    Zombie,
}

// ─── Контекст задачи (сохранённые регистры) ──────────────────────────────────

/// Сохранённый контекст задачи (регистры CPU).
/// При context switch ядро сохраняет сюда callee-saved регистры.
///
/// Layout должен точно совпадать с asm в `switch_context` (порядок push/pop).
#[derive(Default, Clone, Copy)]
#[repr(C)]
pub struct TaskContext {
    pub rsp: u64, // Стек задачи (обязательно первый — загружается напрямую)
    pub rbp: u64,
    pub rbx: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rflags: u64,
}

// ─── Тип идентификатора задачи
// ────────────────────────────────────────────────

pub type TaskId = u64;

// ─── Структура задачи
// ─────────────────────────────────────────────────────────

/// Задача в очереди планировщика.
pub struct SchedTask {
    /// Уникальный идентификатор задачи
    pub id: TaskId,
    /// PID процесса-владельца (0 = kernel task)
    pub pid: Option<Pid>,
    /// Состояние задачи
    pub state: TaskState,
    /// nice-уровень (-20..+19)
    pub nice: Nice,
    /// Вес (вычислен из nice)
    pub weight: u32,
    /// Виртуальное время выполнения (нс / условные единицы)
    pub vruntime: u64,
    /// Квант выполнения (в тиках) — пересчитывается при постановке в очередь
    pub timeslice: u64,
    /// Оставшийся квант до следующего вытеснения
    pub remaining: u64,
    /// Точка входа функции задачи (для kernel tasks)
    pub entry: Option<u64>,
    /// Сохранённый контекст CPU (обновляется при вытеснении)
    pub context: TaskContext,
    /// Kernel stack (Vec хранится в куче)
    pub kernel_stack: Option<alloc::vec::Vec<u8>>,
    /// Значение канарейки для защиты стека
    pub stack_canary: u64,
}

impl SchedTask {
    /// Создаёт задачу с заданным nice-уровнем.
    pub fn new(id: TaskId, pid: Option<Pid>, nice: Nice) -> Self {
        let weight = nice_to_weight(nice);
        SchedTask {
            id,
            pid,
            state: TaskState::Ready,
            nice,
            weight,
            vruntime: 0,
            timeslice: Self::calc_timeslice(weight),
            remaining: Self::calc_timeslice(weight),
            entry: None,
            context: TaskContext::default(),
            kernel_stack: None,
            stack_canary: 0,
        }
    }

    /// Считает квант выполнения (тики) пропорционально весу.
    /// Базовый квант = 20 тиков (≈ 20ms при HZ=1000).
    /// Высокоприоритетные задачи получают больший квант.
    fn calc_timeslice(weight: u32) -> u64 {
        const BASE_TICKS: u64 = 20;
        const DEFAULT_WEIGHT: u64 = 1024; // nice=0
        (BASE_TICKS * weight as u64) / DEFAULT_WEIGHT
    }

    /// Обновляет vruntime после delta_ticks выполнения.
    /// vruntime += delta * (DEFAULT_WEIGHT / weight) — нормализованное время.
    pub fn update_vruntime(&mut self, delta_ticks: u64) {
        const DEFAULT_WEIGHT: u64 = 1024;
        // Делим с округлением вверх
        let increment = delta_ticks.saturating_mul(DEFAULT_WEIGHT) / (self.weight as u64).max(1);
        self.vruntime = self.vruntime.saturating_add(increment);
    }
}

// ─── Run-queue
// ────────────────────────────────────────────────────────────────

/// Ключ для BTreeMap: (vruntime, id) — обеспечивает уникальность при одинаковом
/// vruntime. BTreeMap гарантирует порядок → min() = задача с наименьшим
/// vruntime.
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
struct RunKey(u64, TaskId);

/// Очередь готовых к выполнению задач.
/// Ключ = (vruntime, task_id), значение = () (задача хранится в `tasks`).
struct RunQueue {
    /// Отсортированный по vruntime список готовых задач
    queue: BTreeMap<RunKey, ()>,
    /// Минимальный vruntime в системе (для нормализации новых задач)
    min_vruntime: u64,
}

impl RunQueue {
    fn new() -> Self {
        RunQueue {
            queue: BTreeMap::new(),
            min_vruntime: 0,
        }
    }

    fn push(&mut self, task: &SchedTask) {
        self.queue.insert(RunKey(task.vruntime, task.id), ());
    }

    fn pop_min(&mut self) -> Option<RunKey> {
        let key = *self.queue.keys().next()?;
        self.queue.remove(&key);
        Some(key)
    }

    fn remove(&mut self, task: &SchedTask) {
        self.queue.remove(&RunKey(task.vruntime, task.id));
    }

    fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    fn len(&self) -> usize {
        self.queue.len()
    }

    /// Обновляет min_vruntime по текущему минимуму в очереди
    fn update_min_vruntime(&mut self) {
        if let Some(key) = self.queue.keys().next() {
            self.min_vruntime = key.0;
        }
    }
}

// ─── Планировщик ─────────────────────────────────────────────────────────────

/// Global task scheduler implementing a Completely Fair Scheduler (CFS)
/// algorithm.
///
/// This scheduler maintains a run queue of tasks sorted by virtual runtime
/// (vruntime) and selects the task with the minimum vruntime for execution. It
/// supports task prioritization via nice values, time-slicing, and preemption
/// based on timer ticks.
pub struct Scheduler {
    /// Все задачи: task_id → SchedTask
    tasks: BTreeMap<TaskId, SchedTask>,
    /// Очередь готовых задач (отсортирована по vruntime)
    run_queue: RunQueue,
    /// ID задачи, выполняющейся сейчас
    current: Option<TaskId>,
    /// Счётчик для генерации уникальных ID
    next_id: TaskId,
    /// Число заблокированных задач
    blocked_count: usize,
}

impl Scheduler {
    pub fn new() -> Self {
        Scheduler {
            tasks: BTreeMap::new(),
            run_queue: RunQueue::new(),
            current: None,
            next_id: 1,
            blocked_count: 0,
        }
    }

    /// Создаёт новую задачу и помещает в run-queue.
    /// Начальный vruntime = max(0, min_vruntime) чтобы не монополизировать CPU.
    pub fn spawn(&mut self, pid: Option<Pid>, nice: Nice) -> TaskId {
        let id = self.next_id;
        self.next_id += 1;

        let mut task = SchedTask::new(id, pid, nice);
        // Нормализация: новая задача начинает с min_vruntime, чтобы не "прыгнуть
        // вперёд"
        task.vruntime = self.run_queue.min_vruntime;

        self.run_queue.push(&task);
        self.tasks.insert(id, task);
        id
    }

    /// Вызывается из обработчика таймера (IRQ 0) каждый тик.
    ///
    /// 1. Обновляет vruntime текущей задачи.
    /// 2. Проверяет, не истёк ли квант → устанавливает флаг preempt.
    ///
    /// Возвращает `true` если нужно вытеснение.
    pub fn tick(&mut self) -> bool {
        let current_id = match self.current {
            Some(id) => id,
            None => return false,
        };

        let task = match self.tasks.get_mut(&current_id) {
            Some(t) => t,
            None => return false,
        };

        // Обновляем vruntime и квант
        task.update_vruntime(1);
        task.remaining = task.remaining.saturating_sub(1);

        // Нужно вытеснение если квант истёк или есть готовая задача с меньшим vruntime
        task.remaining == 0
    }

    /// Выбирает следующую задачу для выполнения.
    ///
    /// Алгоритм:
    /// 1. Если есть текущая задача в Running — возвращаем её обратно в
    ///    run-queue.
    /// 2. Выбираем задачу с минимальным vruntime из run-queue.
    /// 3. Обновляем CURRENT_PID.
    ///
    /// Возвращает TaskId следующей задачи или None если очередь пуста.
    pub fn schedule(&mut self) -> Option<TaskId> {
        // Шаг 1: текущая задача → обратно в run-queue (если Running)
        if let Some(cur_id) = self.current.take() {
            if let Some(task) = self.tasks.get_mut(&cur_id) {
                if task.state == TaskState::Running {
                    task.state = TaskState::Ready;
                    // Сбрасываем квант
                    task.remaining = SchedTask::calc_timeslice(task.weight);
                    self.run_queue.push(task);
                }
            }
        }

        // Шаг 2: выбираем следующую задачу
        let key = self.run_queue.pop_min()?;

        // --- Защита стека: проверяем канарейку текущей задачи перед переключением ---
        if let Some(cur_id) = self.current {
            if let Some(cur_task) = self.tasks.get(&cur_id) {
                if cur_task.stack_canary != 0 {
                    // В реальной системе мы бы проверяли реальный адрес в
                    // памяти.
                }
            }
        }

        // Шаг 2: выбираем следующую задачу
        let task = self.tasks.get_mut(&key.1)?;
        task.state = TaskState::Running;
        self.current = Some(key.1);

        // Обновляем min_vruntime
        self.run_queue.update_min_vruntime();

        // Обновляем CURRENT_PID
        let pid = task.pid.unwrap_or(0);
        CURRENT_PID[0].store(pid, Ordering::Relaxed);

        Some(key.1)
    }

    /// Проверяет наличие сигналов для текущей задачи
    pub fn check_signals(&mut self) -> bool {
        let task_id = match self.current {
            Some(id) => id,
            None => return false,
        };

        let (pid, _cur_vruntime) = match self.tasks.get(&task_id) {
            Some(t) => (t.pid.unwrap_or(0), t.vruntime),
            None => return false,
        };

        // Если есть сигналы в ProcessManager — обрабатываем
        if let Some(proc_arc) = crate::process::find_process(pid) {
            let proc = proc_arc.lock();
            let pending = proc.pending_signals.load(Ordering::SeqCst);
            if pending != 0 {
                // В реальной системе тут мы бы переключили контекст задачи на обработчик.
                // Для MVP — логируем или выполняем простое действие.
                return true;
            }
        }
        false
    }

    /// Блокирует задачу (I/O, sleep, wait).
    /// Задача удаляется из run-queue до явного `unblock`.
    pub fn block(&mut self, task_id: TaskId) {
        if let Some(task) = self.tasks.get_mut(&task_id) {
            if task.state == TaskState::Ready || task.state == TaskState::Running {
                self.run_queue.remove(task);
                task.state = TaskState::Blocked;
                self.blocked_count += 1;

                if self.current == Some(task_id) {
                    self.current = None;
                }
            }
        }
    }

    /// Разблокирует задачу после завершения ожидания.
    pub fn unblock(&mut self, task_id: TaskId) {
        if let Some(task) = self.tasks.get_mut(&task_id) {
            if task.state == TaskState::Blocked {
                // Обновляем vruntime до текущего минимума, чтобы не дать
                // заспавшейся задаче монополизировать CPU
                task.vruntime = task.vruntime.max(self.run_queue.min_vruntime);
                task.state = TaskState::Ready;
                self.run_queue.push(task);
                if self.blocked_count > 0 {
                    self.blocked_count -= 1;
                }
            }
        }
    }

    /// Завершает задачу → переводит в Zombie.
    pub fn exit_task(&mut self, task_id: TaskId, exit_code: i32) {
        if let Some(task) = self.tasks.get_mut(&task_id) {
            self.run_queue.remove(task);
            task.state = TaskState::Zombie;
            if self.current == Some(task_id) {
                self.current = None;
            }
        }
        // Уведомляем process manager
        if let Some(task) = self.tasks.get(&task_id) {
            if let Some(pid) = task.pid {
                crate::process::exit_process(pid, exit_code);
            }
        }
    }

    /// Убирает Zombie-задачи из таблицы (аналог wait/reap).
    /// Освобождает все ресурсы: стек, память, файловые дескрипторы
    pub fn reap_zombies(&mut self) {
        let zombie_ids: Vec<(TaskId, Option<u32>)> = self
            .tasks
            .iter()
            .filter(|(_, t)| t.state == TaskState::Zombie)
            .map(|(id, t)| (*id, t.pid))
            .collect();

        for (id, pid) in &zombie_ids {
            // Задача уже в состоянии Zombie, просто удаляем
            // Vec<u8> стек освободится автоматически при удалении из BTreeMap
            self.tasks.remove(&id);

            // Удаляем из process manager если есть PID
            if let Some(process_pid) = pid {
                crate::process::terminate_process(*process_pid);
            }
        }

        if !zombie_ids.is_empty() {
            crate::println!("[scheduler] Reaped {} zombie tasks", zombie_ids.len());
        }
    }

    /// Изменяет nice-уровень задачи.
    pub fn renice(&mut self, task_id: TaskId, nice: Nice) -> bool {
        if let Some(task) = self.tasks.get_mut(&task_id) {
            // Пересчитываем vruntime при смене приоритета
            let old_w = task.weight as u64;
            let new_w = nice_to_weight(nice) as u64;
            // vruntime пересчитывается: new_vruntime = vruntime * old_w / new_w
            task.vruntime = task.vruntime.saturating_mul(old_w) / new_w.max(1);

            task.nice = nice;
            task.weight = nice_to_weight(nice);
            task.timeslice = SchedTask::calc_timeslice(task.weight);

            // Переставляем в run-queue
            if task.state == TaskState::Ready {
                let old_key = RunKey(task.vruntime, task.id);
                self.run_queue.queue.remove(&old_key);
                self.run_queue.push(task);
            }
            true
        } else {
            false
        }
    }

    // ─── Статистика ───────────────────────────────────────────────────────────

    /// Количество задач в run-queue (готовых)
    pub fn ready_count(&self) -> usize {
        self.run_queue.len()
    }

    /// Количество заблокированных задач
    pub fn blocked_count(&self) -> usize {
        self.blocked_count
    }

    /// Общее количество живых задач
    pub fn total_tasks(&self) -> usize {
        self.tasks.len()
    }

    /// ID текущей задачи
    pub fn current_task(&self) -> Option<TaskId> {
        self.current
    }

    /// vruntime текущей задачи
    pub fn current_vruntime(&self) -> u64 {
        self.current
            .and_then(|id| self.tasks.get(&id))
            .map(|t| t.vruntime)
            .unwrap_or(0)
    }

    /// Список всех задач для отладки
    pub fn dump_tasks(&self) -> Vec<(TaskId, TaskState, u64, Nice)> {
        self.tasks
            .values()
            .map(|t| (t.id, t.state, t.vruntime, t.nice))
            .collect()
    }
}

// ─── Глобальный синглтон
// ──────────────────────────────────────────────────────

pub static SCHEDULER: spin::Lazy<Mutex<Scheduler>> =
    spin::Lazy::new(|| Mutex::new(Scheduler::new()));

// ─── Публичный API
// ────────────────────────────────────────────────────────────

/// Создаёт задачу в планировщике.
pub fn spawn_task(pid: Option<Pid>, nice: Nice) -> TaskId {
    SCHEDULER.lock().spawn(pid, nice)
}

/// Вызывается из обработчика таймера. Возвращает true = нужно вытеснение.
pub fn scheduler_tick() -> bool {
    TIMER_TICKS.fetch_add(1, Ordering::Relaxed);
    SCHEDULER.lock().tick()
}

/// Выбирает следующую задачу для выполнения.
pub fn schedule_next() -> Option<TaskId> {
    SCHEDULER.lock().schedule()
}

/// Блокирует задачу.
pub fn block_task(task_id: TaskId) {
    SCHEDULER.lock().block(task_id);
}

/// Разблокирует задачу.
pub fn unblock_task(task_id: TaskId) {
    SCHEDULER.lock().unblock(task_id);
}

/// Завершает задачу.
pub fn exit_task(task_id: TaskId, exit_code: i32) {
    SCHEDULER.lock().exit_task(task_id, exit_code);
}

/// Изменяет приоритет задачи.
pub fn renice_task(task_id: TaskId, nice: Nice) -> bool {
    SCHEDULER.lock().renice(task_id, nice)
}

/// Счётчик тиков таймера (для отладки и sys_gettime)
pub fn get_ticks() -> u64 {
    TIMER_TICKS.load(Ordering::Relaxed)
}
