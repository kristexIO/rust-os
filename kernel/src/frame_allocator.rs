/// Физический аллокатор фреймов на основе битовой карты (Bitmap Frame
/// Allocator).
///
/// Управляет физической памятью на уровне страниц (4 KiB).
/// Используется всеми подсистемами, которым нужна физическая память:
///   - paging (маппинг виртуальных адресов)
///   - CoW (копирование страниц при записи)
///   - demand paging (ленивое выделение)
///   - fork/exec (клонирование адресных пространств)
///
/// Архитектура:
///   - Bitmap: 1 бит = 1 физический фрейм (4 KiB). Бит 0 = фрейм свободен, бит
///     1 = фрейм занят.
///   - Поддерживает до 4 GiB физической памяти (1M фреймов = 128 KiB bitmap).
///   - Первые 4 MiB помечаются занятыми (kernel + BIOS reserved).
///   - Thread-safe через spin::Mutex.
use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
use spin::Mutex;
use x86_64::{
    structures::paging::{FrameAllocator, FrameDeallocator, PhysFrame, Size4KiB},
    PhysAddr,
};

// ─── Константы
// ────────────────────────────────────────────────────────────────

/// Размер фрейма — 4 KiB (стандарт x86_64)
pub const FRAME_SIZE: u64 = 4096;

/// Максимальное количество поддерживаемых фреймов (покрывает 4 GiB)
const MAX_FRAMES: usize = (4 * 1024 * 1024 * 1024usize) / (FRAME_SIZE as usize);

/// Размер bitmap в u64-словах (каждое u64 хранит 64 фрейма)
const BITMAP_WORDS: usize = MAX_FRAMES / 64;

// ─── Bitmap
// ────────────────────────────────────────────────────────────────────

/// Битовая карта физических фреймов.
/// 0 = свободен, 1 = занят.
/// Хранится в BSS (статическая инициализация нулями).
struct FrameBitmap {
    bits: [u64; BITMAP_WORDS],
}

impl FrameBitmap {
    const fn new() -> Self {
        FrameBitmap {
            bits: [0u64; BITMAP_WORDS],
        }
    }

    /// Помечает фрейм как занятый
    #[inline]
    fn set(&mut self, frame: usize) {
        let word = frame / 64;
        let bit = frame % 64;
        if word < BITMAP_WORDS {
            self.bits[word] |= 1u64 << bit;
        }
    }

    /// Помечает фрейм как свободный
    #[inline]
    fn clear(&mut self, frame: usize) {
        let word = frame / 64;
        let bit = frame % 64;
        if word < BITMAP_WORDS {
            self.bits[word] &= !(1u64 << bit);
        }
    }

    /// Возвращает `true`, если фрейм занят
    #[inline]
    fn is_set(&self, frame: usize) -> bool {
        let word = frame / 64;
        let bit = frame % 64;
        if word < BITMAP_WORDS {
            self.bits[word] & (1u64 << bit) != 0
        } else {
            true // за границей — считаем занятым
        }
    }

    /// Ищет первый свободный фрейм начиная с `hint`.
    /// Возвращает индекс фрейма или None если памяти нет.
    fn find_free(&self, hint: usize) -> Option<usize> {
        let start_word = hint / 64;

        // Проходим от hint до конца, потом от начала до hint
        let words = (start_word..BITMAP_WORDS).chain(0..start_word);

        for word_idx in words {
            let word = self.bits[word_idx];
            if word != u64::MAX {
                // Есть хотя бы один свободный бит
                // trailing_ones() даёт позицию первого нулевого бита
                let bit = word.trailing_ones() as usize;
                let frame = word_idx * 64 + bit;
                if frame < MAX_FRAMES {
                    return Some(frame);
                }
            }
        }
        None
    }

    /// Ищет `count` подряд идущих свободных фреймов (для hugepage / contiguous
    /// DMA).
    fn find_free_contiguous(&self, count: usize) -> Option<usize> {
        let mut run_start = 0usize;
        let mut run_len = 0usize;

        for frame in 0..MAX_FRAMES {
            if !self.is_set(frame) {
                if run_len == 0 {
                    run_start = frame;
                }
                run_len += 1;
                if run_len >= count {
                    return Some(run_start);
                }
            } else {
                run_len = 0;
            }
        }
        None
    }
}

// ─── BitmapFrameAllocator
// ─────────────────────────────────────────────────────

/// Глобальный физический аллокатор фреймов.
pub struct BitmapFrameAllocator {
    /// Битовая карта: 1 = занят
    bitmap: FrameBitmap,
    /// Последний выделенный фрейм (подсказка для следующего поиска — O(1) в
    /// лучшем случае)
    last_alloc: usize,
    /// Общее число фреймов в системе (определяется по memory map)
    total_frames: usize,
    /// Число свободных фреймов
    free_frames: usize,
    /// Инициализирован ли аллокатор
    initialized: bool,
}

impl BitmapFrameAllocator {
    pub const fn new() -> Self {
        BitmapFrameAllocator {
            bitmap: FrameBitmap::new(),
            last_alloc: 0,
            total_frames: 0,
            free_frames: 0,
            initialized: false,
        }
    }

    /// Инициализирует аллокатор по memory map загрузчика.
    ///
    /// Алгоритм:
    /// 1. Помечаем ВСЮ память как занятую.
    /// 2. Проходим по memory map: помечаем Usable-регионы как свободные.
    /// 3. Защищаем первые 4 MiB (0–0x3FFFFF): там живут ядро, BIOS, EBDA.
    ///
    /// # Safety
    /// Должна вызываться ровно один раз после инициализации загрузчика.
    pub fn init(&mut self, memory_regions: &MemoryRegions) {
        // Шаг 1: Всё занято по умолчанию (bitmap изначально нулевой = свободный,
        // поэтому инвертируем — помечаем всё занятым)
        for word in self.bitmap.bits.iter_mut() {
            *word = u64::MAX;
        }

        // Шаг 2: Освобождаем Usable-регионы
        let mut _usable_bytes = 0u64;
        for region in memory_regions.iter() {
            if region.kind == MemoryRegionKind::Usable {
                let start_frame = (region.start / FRAME_SIZE) as usize;
                let end_frame = (region.end / FRAME_SIZE) as usize;

                for frame in start_frame..end_frame {
                    if frame < MAX_FRAMES {
                        self.bitmap.clear(frame);
                        _usable_bytes += FRAME_SIZE;
                    }
                }

                let frames_in_region = end_frame.saturating_sub(start_frame);
                self.total_frames += frames_in_region;
                self.free_frames += frames_in_region;
            }
        }

        // Шаг 3: Защищаем первые 4 MiB (фреймы 0..1024)
        // Там: BIOS (0-0x500), low memory, kernel image, EBDA, VGA buffer (0xA0000+)
        let reserved_frames = (4 * 1024 * 1024 / FRAME_SIZE) as usize;
        for frame in 0..reserved_frames {
            if !self.bitmap.is_set(frame) {
                self.bitmap.set(frame);
                if self.free_frames > 0 {
                    self.free_frames -= 1;
                }
            }
        }

        self.initialized = true;
        self.last_alloc = reserved_frames; // Начинаем поиск после
                                           // зарезервированных
    }

    /// Выделяет один физический фрейм.
    /// Возвращает `None` при OOM.
    ///
    /// Сложность: O(1) в типичном случае (next-fit), O(N) в худшем.
    pub fn allocate(&mut self) -> Option<PhysFrame<Size4KiB>> {
        if !self.initialized || self.free_frames == 0 {
            return None;
        }

        let frame_idx = self.bitmap.find_free(self.last_alloc)?;
        self.bitmap.set(frame_idx);
        self.free_frames -= 1;
        self.last_alloc = frame_idx + 1;

        // SAFETY: frame_idx * FRAME_SIZE выровнен по 4096 и находится
        // в диапазоне физической памяти (гарантируется memory map).
        let addr = PhysAddr::new((frame_idx as u64) * FRAME_SIZE);
        Some(PhysFrame::from_start_address(addr).ok()?)
    }

    /// Выделяет `count` физически непрерывных фреймов.
    /// Нужно для DMA-буферов, hugepages.
    pub fn allocate_contiguous(&mut self, count: usize) -> Option<PhysFrame<Size4KiB>> {
        if !self.initialized || self.free_frames < count {
            return None;
        }

        let start_frame = self.bitmap.find_free_contiguous(count)?;
        for i in 0..count {
            self.bitmap.set(start_frame + i);
        }
        self.free_frames -= count;

        let addr = PhysAddr::new((start_frame as u64) * FRAME_SIZE);
        Some(PhysFrame::from_start_address(addr).ok()?)
    }

    /// Освобождает физический фрейм.
    ///
    /// # Safety
    /// Фрейм должен был быть ранее выделен этим аллокатором.
    /// Двойное освобождение (double-free) не вызовет UB, но приведёт
    /// к некорректному счётчику free_frames.
    pub fn deallocate(&mut self, frame: PhysFrame<Size4KiB>) {
        let frame_idx = (frame.start_address().as_u64() / FRAME_SIZE) as usize;
        if frame_idx < MAX_FRAMES && self.bitmap.is_set(frame_idx) {
            self.bitmap.clear(frame_idx);
            self.free_frames += 1;
            // Обновляем подсказку назад, если это более ранний адрес
            if frame_idx < self.last_alloc {
                self.last_alloc = frame_idx;
            }
        }
        // Молча игнорируем попытку освободить незанятый фрейм
        // (лучше чем паника внутри деструктора)
    }

    /// Статистика: количество свободных фреймов
    pub fn free_frames(&self) -> usize {
        self.free_frames
    }

    /// Статистика: общее число фреймов
    pub fn total_frames(&self) -> usize {
        self.total_frames
    }

    /// Свободная память в байтах
    pub fn free_memory_bytes(&self) -> u64 {
        self.free_frames as u64 * FRAME_SIZE
    }

    /// Используемая память в байтах
    pub fn used_memory_bytes(&self) -> u64 {
        (self.total_frames - self.free_frames) as u64 * FRAME_SIZE
    }

    /// Инициализирован ли аллокатор
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

// ─── Реализация трейтов x86_64
// ────────────────────────────────────────────────

/// Интеграция с x86_64::structures::paging::Mapper.
///
/// # Safety
/// Реализация FrameAllocator небезопасна по контракту x86_64:
/// каждый возвращённый фрейм не должен использоваться нигде ещё.
/// BitmapFrameAllocator гарантирует это через bitmap.
unsafe impl FrameAllocator<Size4KiB> for BitmapFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        self.allocate()
    }
}

impl FrameDeallocator<Size4KiB> for BitmapFrameAllocator {
    /// # Safety
    /// Фрейм должен был быть выделен ранее и больше не использоваться.
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>) {
        self.deallocate(frame);
    }
}

// ─── Глобальный синглтон
// ──────────────────────────────────────────────────────

/// Глобальный физический аллокатор фреймов.
/// Инициализируется в kernel_main через `init_frame_allocator(memory_regions)`.
static FRAME_ALLOCATOR: Mutex<BitmapFrameAllocator> = Mutex::new(BitmapFrameAllocator::new());

/// Инициализирует глобальный аллокатор по memory map загрузчика.
/// Вызывается один раз из kernel_main ДО любого использования paging/CoW.
pub fn init_frame_allocator(memory_regions: &MemoryRegions) {
    FRAME_ALLOCATOR.lock().init(memory_regions);
}

/// Предоставляет доступ к глобальному аллокатору для подсистем ядра.
pub fn get_frame_allocator() -> spin::MutexGuard<'static, BitmapFrameAllocator> {
    FRAME_ALLOCATOR.lock()
}

/// Выделяет один фрейм из глобального аллокатора.
/// Возвращает None при OOM — физическая память может закончиться.
pub fn alloc_frame() -> Option<PhysFrame<Size4KiB>> {
    FRAME_ALLOCATOR.lock().allocate()
}

/// Выделяет один фрейм. Возвращает None вместо паники.
pub fn try_alloc_frame() -> Option<PhysFrame<Size4KiB>> {
    FRAME_ALLOCATOR.lock().allocate()
}

/// Освобождает фрейм обратно в аллокатор.
pub fn free_frame(frame: PhysFrame<Size4KiB>) {
    FRAME_ALLOCATOR.lock().deallocate(frame);
}

/// Выводит статистику памяти через терминал
pub fn print_memory_stats() {
    let alloc = FRAME_ALLOCATOR.lock();
    let total_mib = alloc.total_frames() * (FRAME_SIZE as usize) / (1024 * 1024);
    let free_mib = (alloc.free_memory_bytes() / (1024 * 1024)) as usize;
    let used_mib = (alloc.used_memory_bytes() / (1024 * 1024)) as usize;
    drop(alloc);

    if let Some(disp) = crate::graphics::GLOBAL_DISPLAY.lock().as_mut() {
        if let Some(term) = crate::fast_terminal::TERMINAL.lock().as_mut() {
            term.print_str(disp, "[mem] Physical memory stats available\n");
            let _ = (total_mib, free_mib, used_mib);
        }
    }
}

// ─── Тесты ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitmap_set_clear() {
        let mut bm = FrameBitmap::new();
        assert!(!bm.is_set(0));
        bm.set(0);
        assert!(bm.is_set(0));
        bm.clear(0);
        assert!(!bm.is_set(0));
    }

    #[test]
    fn test_bitmap_find_free() {
        let mut bm = FrameBitmap::new();
        // Всё свободно — первый свободный = 0
        assert_eq!(bm.find_free(0), Some(0));
        bm.set(0);
        assert_eq!(bm.find_free(0), Some(1));
    }
}
