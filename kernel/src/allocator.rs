use core::sync::atomic::{AtomicUsize, Ordering};

/// Глобальный аллокатор памяти ядра на основе связного списка.
///
/// linked_list_allocator::LockedHeap — thread-safe аллокатор, который уже
/// оборачивает внутреннее состояние в spin::Mutex. Корректно работает с
/// alloc::boxed::Box, alloc::vec::Vec и alloc::string::String.
use linked_list_allocator::LockedHeap;

/// Статистика аллокатора для отладки и мониторинга
#[derive(Debug, Default)]
pub struct AllocatorStats {
    /// Общее количество выделенной памяти в байтах
    pub allocated_bytes: AtomicUsize,
    /// Общее количество освобожденной памяти в байтах
    pub deallocated_bytes: AtomicUsize,
    /// Текущее использование памяти в байтах
    pub current_usage: AtomicUsize,
    /// Пиковое использование памяти в байтах
    pub peak_usage: AtomicUsize,
    /// Количество операций выделения
    pub allocation_count: AtomicUsize,
    /// Количество операций освобождения
    pub deallocation_count: AtomicUsize,
    /// Количество ошибок OOM
    pub oom_count: AtomicUsize,
}

impl AllocatorStats {
    /// Создать новую статистику с нулевыми значениями
    pub const fn new() -> Self {
        Self {
            allocated_bytes: AtomicUsize::new(0),
            deallocated_bytes: AtomicUsize::new(0),
            current_usage: AtomicUsize::new(0),
            peak_usage: AtomicUsize::new(0),
            allocation_count: AtomicUsize::new(0),
            deallocation_count: AtomicUsize::new(0),
            oom_count: AtomicUsize::new(0),
        }
    }

    /// Вывести статистику через терминал (заглушка для будущей реализации)
    pub fn print(&self) {
        // В будущей реализации здесь будет вывод через logging macros
        // Пока оставляем пустой, так как мы не можем легко вывести в консоль
        // из alloc_error_handler контекста
    }
}

/// Глобальная статистика аллокатора
static ALLOCATOR_STATS: AllocatorStats = AllocatorStats::new();

/// Объявляем этот аллокатор как глобальный (#[global_allocator]).
/// Rust будет вызывать его для всех аллокаций через alloc::alloc::alloc().
#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Размер кучи ядра: 16 MiB.
/// Увеличено с 15 MiB для лучшей производительности при больших нагрузках.
/// Хранится в BSS-сегменте (нули) — не занимает место в образе диска.
const HEAP_SIZE: usize = 16 * 1024 * 1024;

/// Статический буфер кучи. `repr(align(16))` гарантирует выравнивание,
/// необходимое для большинства структур. `uninit` — не инициализируем,
/// linked_list_allocator сам управляет содержимым.
#[repr(align(16))]
struct HeapBuf([u8; HEAP_SIZE]);

static mut HEAP_BUF: HeapBuf = HeapBuf([0; HEAP_SIZE]);

/// Инициализирует кучу ядра. Должна быть вызвана ОДИН РАЗ из kernel_main
/// до любого использования Box/Vec/String.
pub fn init() {
    // SAFETY: HEAP_BUF — статический буфер с правильным выравниванием.
    // Функция вызывается ровно один раз до любых многопоточных операций.
    // linked_list_allocator::init() помечает всю область как свободную
    // и с этого момента безопасно управляет ею через AtomicMutex.
    unsafe {
        ALLOCATOR
            .lock()
            .init(core::ptr::addr_of_mut!(HEAP_BUF) as *mut u8, HEAP_SIZE);
    }
}

/// Обработчик ошибки аллокации: вызывается при OOM.
/// В ядре аллокация не должна падать тихо — паникуем.
///
/// В Rust >=1.82 этот обработчик встроен в alloc и паникует по умолчанию.
/// Оставляем явно для наглядности.
#[alloc_error_handler]
fn alloc_error(layout: core::alloc::Layout) -> ! {
    // Увеличиваем счетчик OOM для статистики
    ALLOCATOR_STATS.oom_count.fetch_add(1, Ordering::Relaxed);

    // В реальной системе здесь можно было бы попытаться освободить память
    // или предпринять другие меры перед паникой
    panic!("OOM: не удалось выделить {} байт", layout.size());
}
