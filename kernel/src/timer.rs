use core::sync::atomic::Ordering;

use crate::scheduler::TIMER_TICKS;

/// Возвращает время монотонного аптайма в миллисекундах.
/// Основано на тиках таймера (обычно 100 Гц = 10 мс/тик).
pub fn get_uptime_ms() -> u64 {
    TIMER_TICKS.load(Ordering::Relaxed) * 10
}
