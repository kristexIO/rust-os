use x86_64::instructions::port::Port;

/// Драйвер системного динамика (PC Speaker)
/// Использует программируемый таймер (PIT) канал 2.
pub struct PcSpeaker;

impl PcSpeaker {
    /// Издает звук заданной частоты
    pub unsafe fn play_sound(freq_hz: u32) {
        if freq_hz == 0 {
            Self::stop_sound();
            return;
        }

        let divisor = 1193180 / freq_hz;

        let mut command_port = Port::<u8>::new(0x43);
        let mut data_port = Port::<u8>::new(0x42);
        let mut speaker_port = Port::<u8>::new(0x61);

        // Команда PIT: канал 2, lobyte/hibyte, квадратная волна, бинарный режим
        command_port.write(0xB6);
        data_port.write(divisor as u8);
        data_port.write((divisor >> 8) as u8);

        // Включаем динамик (биты 0 и 1 порта 0x61)
        let state = speaker_port.read();
        if state & 3 != 3 {
            speaker_port.write(state | 3);
        }
    }

    /// Останавливает звук
    pub unsafe fn stop_sound() {
        let mut speaker_port = Port::<u8>::new(0x61);
        let state = speaker_port.read();
        speaker_port.write(state & 0xFC);
    }

    /// Короткий "бип"
    pub fn beep() {
        unsafe {
            Self::play_sound(750);
            for _ in 0..10_000_000 {
                core::arch::asm!("nop");
            }
            Self::stop_sound();
        }
    }

    /// Асинхронное проигрывание звука на определенное количество тиков часов
    /// (18.2 тиков/с)
    pub async fn async_play_sound(freq_hz: u32, duration_ticks: u64) {
        if freq_hz > 0 {
            unsafe { Self::play_sound(freq_hz) };
        } else {
            unsafe { Self::stop_sound() };
        }
        let start = crate::gui::CLOCK_TICKS.load(core::sync::atomic::Ordering::Relaxed);
        while crate::gui::CLOCK_TICKS.load(core::sync::atomic::Ordering::Relaxed) - start
            < duration_ticks
        {
            crate::task::yield_now().await;
        }
        unsafe { Self::stop_sound() };
    }

    /// Асинхронное проигрывание последовательности нот `(частота, тики)`
    pub async fn play_sequence(notes: &[(u32, u64)]) {
        for &(freq, ticks) in notes {
            Self::async_play_sound(freq, ticks).await;

            // Крошечная пауза между нотами
            let start = crate::gui::CLOCK_TICKS.load(core::sync::atomic::Ordering::Relaxed);
            while crate::gui::CLOCK_TICKS.load(core::sync::atomic::Ordering::Relaxed) - start < 1 {
                crate::task::yield_now().await;
            }
        }
    }
}
