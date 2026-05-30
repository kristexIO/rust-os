use alloc::{boxed::Box, collections::BTreeMap, sync::Arc, task::Wake};
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use crossbeam_queue::ArrayQueue;
use spin::Mutex;
use x86_64::instructions::interrupts;

pub static TASKS_INFO: Mutex<alloc::vec::Vec<(usize, alloc::string::String)>> =
    Mutex::new(alloc::vec::Vec::new());

/// Обёртка вокруг Future для выполнения в Executor
pub struct Task {
    pub name: alloc::string::String,
    future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
}

impl Task {
    pub fn new(name: &str, future: impl Future<Output = ()> + Send + 'static) -> Self {
        Task {
            name: alloc::string::String::from(name),
            future: Box::pin(future),
        }
    }

    pub fn poll(&mut self, context: &mut Context) -> Poll<()> {
        self.future.as_mut().poll(context)
    }
}

pub struct YieldNow {
    yielded: bool,
}

impl Future for YieldNow {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<()> {
        if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

pub async fn yield_now() {
    YieldNow {
        yielded: false,
    }
    .await
}

/// Наш пользовательский Waker. Кладет task_id обратно в очередь при
/// пробуждении.
struct TaskWaker {
    task_id: usize,
    task_queue: Arc<ArrayQueue<usize>>,
}

impl TaskWaker {
    fn new(task_id: usize, task_queue: Arc<ArrayQueue<usize>>) -> Waker {
        // Rust 1.51+ позволяет создавать Waker из Arc<impl Wake>
        Waker::from(Arc::new(TaskWaker {
            task_id,
            task_queue,
        }))
    }
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        // Игнорируем ошибку, если очередь полна - задача будет пробуждена позже
        let _ = self.task_queue.push(self.task_id);
    }
}

/// Асинхронный Executor (Кооперативная многозадачность)
pub struct Executor {
    tasks: BTreeMap<usize, Task>,
    task_queue: Arc<ArrayQueue<usize>>,
    waker_cache: BTreeMap<usize, Waker>,
    next_id: usize,
}

impl Executor {
    pub fn new() -> Self {
        Executor {
            tasks: BTreeMap::new(),
            task_queue: Arc::new(ArrayQueue::new(256)),
            waker_cache: BTreeMap::new(),
            next_id: 1,
        }
    }

    /// Добавляет новую таску на выполнение
    pub fn spawn(&mut self, task: Task) {
        let task_id = self.next_id;
        self.next_id += 1;
        TASKS_INFO.lock().push((task_id, task.name.clone()));
        self.tasks.insert(task_id, task);
        // Игнорируем ошибку, если очередь полна - таска будет выполнена позже
        let _ = self.task_queue.push(task_id);
    }

    /// Запускает бесконечный цикл обработки тасок
    pub fn run(&mut self) -> ! {
        loop {
            self.run_ready_tasks();
            self.sleep_if_idle();
        }
    }

    fn run_ready_tasks(&mut self) {
        while let Some(task_id) = self.task_queue.pop() {
            let task = match self.tasks.get_mut(&task_id) {
                Some(task) => task,
                None => continue, // Таска была удалена
            };

            let queue = self.task_queue.clone();
            let waker = self
                .waker_cache
                .entry(task_id)
                .or_insert_with(|| TaskWaker::new(task_id, queue));

            let mut context = Context::from_waker(waker);

            match task.poll(&mut context) {
                Poll::Ready(()) => {
                    self.tasks.remove(&task_id);
                    self.waker_cache.remove(&task_id);
                    TASKS_INFO.lock().retain(|(id, _)| *id != task_id);
                },
                Poll::Pending => {}, // Таска уснула, Waker её разбудит
            }
        }
    }

    fn sleep_if_idle(&self) {
        // Мы выключаем прерывания перед проверкой, чтобы избежать race condition,
        // когда прерывание срабатывает между is_empty и hlt, и мы засыпаем навечно.
        interrupts::disable();
        if self.task_queue.is_empty() {
            interrupts::enable_and_hlt(); // Атомарное включение прерываний и
                                          // засыпание (sti; hlt)
        } else {
            interrupts::enable();
        }
    }
}
