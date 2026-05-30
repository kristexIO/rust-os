use alloc::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    vec::Vec,
};
use core::sync::atomic::{AtomicU32, Ordering};

use spin::Mutex;
use x86_64::VirtAddr;

/// Состояния процесса
#[derive(Debug, Clone, PartialEq)]
pub enum ProcessState {
    Running,
    Ready,
    Blocked,
    Terminated,
    Waiting,
}

/// Сообщение для IPC
#[derive(Debug, Clone)]
pub struct Message {
    pub from_pid: Pid,
    pub data: Vec<u8>,
}

/// Тип для идентификатора процесса
pub type Pid = u32;

/// Информация о процессе
pub struct Process {
    pub pid: Pid,
    pub state: ProcessState,
    pub name: alloc::string::String,
    pub priority: u8,
    pub memory_space: Option<VirtAddr>, // Адресное пространство процесса
    pub kernel_stack: Option<VirtAddr>, // Стек ядра для системных вызовов
    pub user_stack: Option<VirtAddr>,   // Стек пользовательского режима
    pub entry_point: Option<VirtAddr>,  // Точка входа процесса
    pub exit_code: Option<i32>,         // Код возврата при завершении
    pub parent_pid: Option<Pid>,        // PID родительского процесса
    pub children: Vec<Pid>,             // Дочерние процессы
    pub mailbox: Mutex<VecDeque<Message>>, // Очередь сообщений (IPC)
    pub pending_signals: AtomicU32,     // Бит-маска ожидающих сигналов
    pub signal_handlers: [Option<VirtAddr>; 32], // Обработчики сигналов
    pub syscall_whitelist: u64,         // Бит-маска разрешенных системных вызовов
}

impl Process {
    pub fn new(pid: Pid, name: alloc::string::String, parent_pid: Option<Pid>) -> Self {
        Process {
            pid,
            state: ProcessState::Ready,
            name,
            priority: 1, // по умолчанию обычный приоритет
            memory_space: None,
            kernel_stack: None,
            user_stack: None,
            entry_point: None,
            exit_code: None,
            parent_pid,
            children: Vec::new(),
            mailbox: Mutex::new(VecDeque::new()),
            pending_signals: AtomicU32::new(0),
            signal_handlers: [None; 32],
            syscall_whitelist: !0u64, // По умолчанию разрешены все (для совместимости)
        }
    }

    /// Завершает процесс с указанным кодом
    pub fn terminate(&mut self, exit_code: i32) {
        self.state = ProcessState::Terminated;
        self.exit_code = Some(exit_code);
    }

    /// Проверяет, завершен ли процесс
    pub fn is_terminated(&self) -> bool {
        self.state == ProcessState::Terminated
    }

    /// Отправляет сообщение в почтовый ящик процесса
    pub fn send_message(&self, msg: Message) {
        self.mailbox.lock().push_back(msg);
    }

    /// Получает сообщение из почтового ящика (FIFO)
    pub fn receive_message(&self) -> Option<Message> {
        self.mailbox.lock().pop_front()
    }

    /// Посылает сигнал процессу
    pub fn send_signal(&self, signal: u8) {
        if signal < 32 {
            self.pending_signals.fetch_or(1 << signal, Ordering::SeqCst);
        }
    }

    /// Устанавливает обработчик сигнала
    pub fn set_signal_handler(&mut self, signal: u8, handler: VirtAddr) {
        if signal < 32 {
            self.signal_handlers[signal as usize] = Some(handler);
        }
    }
}

/// Процессный менеджер
pub struct ProcessManager {
    processes: BTreeMap<Pid, Arc<Mutex<Process>>>,
    next_pid: Pid,
}

impl ProcessManager {
    pub const fn new() -> Self {
        ProcessManager {
            processes: BTreeMap::new(),
            next_pid: 1,
        }
    }

    /// Создает новый процесс
    pub fn create_process(
        &mut self,
        name: alloc::string::String,
        parent_pid: Option<Pid>,
    ) -> Arc<Mutex<Process>> {
        let pid = self.next_pid;
        self.next_pid += 1;

        let process = Arc::new(Mutex::new(Process::new(pid, name, parent_pid)));

        // Если у процесса есть родитель, добавляем его в список детей
        if let Some(parent_id) = parent_pid {
            if let Some(parent_proc) = self.processes.get(&parent_id) {
                let mut parent_guard = parent_proc.lock();
                parent_guard.children.push(pid);
            }
        }

        self.processes.insert(pid, process.clone());
        process
    }

    /// Находит процесс по PID
    pub fn find_process_by_pid(&self, pid: Pid) -> Option<Arc<Mutex<Process>>> {
        self.processes.get(&pid).cloned()
    }

    /// Удаляет процесс и всех его потомков
    pub fn terminate_process(&mut self, pid: Pid) -> Option<i32> {
        // Сначала завершаем все дочерние процессы
        if let Some(process) = self.processes.get(&pid) {
            let children: Vec<Pid> = process.lock().children.clone();
            for child_pid in children {
                self.terminate_process(child_pid);
            }
        }

        // Завершаем сам процесс
        if let Some(process) = self.processes.get(&pid) {
            let exit_code = {
                let mut proc = process.lock();
                proc.terminate(0); // по умолчанию успешное завершение
                proc.exit_code.unwrap_or(0)
            };

            // Удаляем процесс из менеджера
            self.processes.remove(&pid);
            Some(exit_code)
        } else {
            None
        }
    }

    /// Получает список всех процессов
    pub fn get_all_processes(&self) -> Vec<Arc<Mutex<Process>>> {
        self.processes.values().cloned().collect()
    }

    /// Получает количество активных процессов
    pub fn active_process_count(&self) -> usize {
        self.processes.len()
    }

    /// Завершает процесс с указанным кодом выхода
    pub fn exit_process(&mut self, pid: Pid, exit_code: i32) -> Option<()> {
        if let Some(process) = self.processes.get(&pid) {
            let mut proc = process.lock();
            proc.terminate(exit_code);

            // Уведомляем родительский процесс о завершении
            if let Some(_parent_pid) = proc.parent_pid {
                // В реальной системе тут может быть сигнал родителю
                // или добавление в очередь завершенных процессов
            }

            Some(())
        } else {
            None
        }
    }

    /// Получает список дочерних процессов
    pub fn get_children_of_process(&self, pid: Pid) -> Vec<Arc<Mutex<Process>>> {
        let mut children = Vec::new();

        if let Some(parent_process) = self.processes.get(&pid) {
            let parent_children = parent_process.lock().children.clone();

            for child_pid in parent_children {
                if let Some(child_process) = self.processes.get(&child_pid) {
                    children.push(child_process.clone());
                }
            }
        }

        children
    }
}

/// Глобальный процессный менеджер
pub static PROCESS_MANAGER: spin::Mutex<ProcessManager> = spin::Mutex::new(ProcessManager::new());

// Функции для работы с процессами из других частей системы

/// Создает новый процесс
pub fn create_process(name: alloc::string::String) -> Arc<Mutex<Process>> {
    PROCESS_MANAGER.lock().create_process(name, None) // Процесс без родителя
                                                      // (root)
}

/// Создает дочерний процесс
pub fn create_child_process(name: alloc::string::String, parent_pid: Pid) -> Arc<Mutex<Process>> {
    PROCESS_MANAGER
        .lock()
        .create_process(name, Some(parent_pid))
}

/// Находит процесс по PID
pub fn find_process(pid: Pid) -> Option<Arc<Mutex<Process>>> {
    PROCESS_MANAGER.lock().find_process_by_pid(pid)
}

/// Уничтожает процесс
pub fn terminate_process(pid: Pid) -> Option<i32> {
    PROCESS_MANAGER.lock().terminate_process(pid)
}

/// Завершает процесс с кодом
pub fn exit_process(pid: Pid, exit_code: i32) -> Option<()> {
    PROCESS_MANAGER.lock().exit_process(pid, exit_code)
}

/// Получает список всех процессов
pub fn get_all_processes() -> Vec<Arc<Mutex<Process>>> {
    PROCESS_MANAGER.lock().get_all_processes()
}

/// Получает количество активных процессов
pub fn active_process_count() -> usize {
    PROCESS_MANAGER.lock().active_process_count()
}

/// Получает список дочерних процессов
pub fn get_children_of_process(pid: Pid) -> Vec<Arc<Mutex<Process>>> {
    PROCESS_MANAGER.lock().get_children_of_process(pid)
}
