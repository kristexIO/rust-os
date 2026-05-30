use alloc::{boxed::Box, vec, vec::Vec};
use spin::Mutex;

// Структуры для работы с аудио
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioFormat {
    U8,
    S16,
    F32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChannelLayout {
    Mono,
    Stereo,
    Surround5Point1,
    Surround7Point1,
}

#[derive(Clone)]
pub struct AudioSpec {
    pub freq: u32,               // Частота дискретизации
    pub format: AudioFormat,     // Формат данных
    pub channels: ChannelLayout, // Каналы
    pub samples: u16,            // Размер буфера в сэмплах
}

#[derive(Clone)]
pub struct AudioBuffer {
    pub data: Vec<i16>,  // Аудио данные (вообще могут быть разного формата)
    pub spec: AudioSpec, // Спецификация буфера
}

pub trait AudioDriver: Send {
    fn init(&mut self, spec: &AudioSpec) -> Result<(), &'static str>;
    fn play(&mut self, buffer: &AudioBuffer) -> Result<(), &'static str>;
    fn pause(&mut self);
    fn resume(&mut self);
    fn stop(&mut self);
    fn set_volume(&mut self, volume: f32); // 0.0 - 1.0
    fn get_spec(&self) -> &AudioSpec;
}

// Реализация простого программного микшера
pub struct AudioMixer {
    pub channels: Vec<AudioChannel>,
    pub master_volume: f32,
    pub spec: AudioSpec,
}

pub struct AudioChannel {
    pub buffer: Option<AudioBuffer>,
    pub position: usize,
    pub playing: bool,
    pub loop_enabled: bool,
    pub volume: f32,
    pub pan: f32, // -1.0 (left) to 1.0 (right)
}

impl AudioMixer {
    pub fn new(spec: AudioSpec) -> Self {
        AudioMixer {
            channels: Vec::new(),
            master_volume: 1.0,
            spec,
        }
    }

    pub fn add_channel(&mut self) -> usize {
        let channel = AudioChannel {
            buffer: None,
            position: 0,
            playing: false,
            loop_enabled: false,
            volume: 1.0,
            pan: 0.0,
        };
        self.channels.push(channel);
        self.channels.len() - 1
    }

    pub fn load_sample(&mut self, channel_id: usize, buffer: AudioBuffer) {
        if channel_id < self.channels.len() {
            self.channels[channel_id].buffer = Some(buffer);
            self.channels[channel_id].position = 0;
        }
    }

    pub fn play_channel(&mut self, channel_id: usize) {
        if channel_id < self.channels.len() {
            self.channels[channel_id].playing = true;
        }
    }

    pub fn pause_channel(&mut self, channel_id: usize) {
        if channel_id < self.channels.len() {
            self.channels[channel_id].playing = false;
        }
    }

    pub fn set_channel_volume(&mut self, channel_id: usize, volume: f32) {
        if channel_id < self.channels.len() {
            self.channels[channel_id].volume = volume.clamp(0.0, 1.0);
        }
    }

    pub fn set_channel_pan(&mut self, channel_id: usize, pan: f32) {
        if channel_id < self.channels.len() {
            self.channels[channel_id].pan = pan.clamp(-1.0, 1.0);
        }
    }

    pub fn mix_audio(&mut self) -> Vec<i16> {
        let buffer_size = self.spec.samples as usize;
        let mut mixed_buffer = vec![0i16; buffer_size * self.get_channel_count()];

        for channel in &mut self.channels {
            if channel.playing {
                if let Some(ref buffer) = channel.buffer {
                    if !buffer.data.is_empty() {
                        // Применяем объем и панораму к данным канала
                        for (i, &sample) in buffer.data.iter().enumerate() {
                            if i < buffer_size && channel.position + i < buffer.data.len() {
                                let original_sample = buffer.data[channel.position + i];

                                // Применяем громкость канала
                                let vol_sample = (original_sample as f32 * channel.volume) as i16;

                                // Применяем панораму (для стерео)
                                match self.spec.channels {
                                    ChannelLayout::Stereo => {
                                        // Для простоты используем моно в стерео канале
                                        let left_gain =
                                            ((1.0 - channel.pan).max(0.0) / 2.0).min(1.0);
                                        let right_gain =
                                            ((1.0 + channel.pan).max(0.0) / 2.0).min(1.0);

                                        let left_sample = (vol_sample as f32 * left_gain) as i16;
                                        let right_sample = (vol_sample as f32 * right_gain) as i16;

                                        if i * 2 < mixed_buffer.len() {
                                            mixed_buffer[i * 2] =
                                                mixed_buffer[i * 2].saturating_add(left_sample);
                                        }
                                        if i * 2 + 1 < mixed_buffer.len() {
                                            mixed_buffer[i * 2 + 1] = mixed_buffer[i * 2 + 1]
                                                .saturating_add(right_sample);
                                        }
                                    }
                                    _ => {
                                        // Для моно просто добавляем
                                        if i < mixed_buffer.len() {
                                            mixed_buffer[i] =
                                                mixed_buffer[i].saturating_add(vol_sample);
                                        }
                                    }
                                }
                            }
                        }

                        // Обновляем позицию
                        channel.position += buffer_size;
                        if channel.loop_enabled && channel.position >= buffer.data.len() {
                            channel.position = 0;
                        } else if channel.position >= buffer.data.len() {
                            channel.playing = false;
                        }
                    }
                }
            }
        }

        // Применяем мастер-громкость
        for sample in &mut mixed_buffer {
            *sample = (*sample as f32 * self.master_volume) as i16;
        }

        mixed_buffer
    }

    pub fn get_channel_count(&self) -> usize {
        match self.spec.channels {
            ChannelLayout::Mono => 1,
            ChannelLayout::Stereo => 2,
            ChannelLayout::Surround5Point1 => 6,
            ChannelLayout::Surround7Point1 => 8,
        }
    }
}

// Виртуальный драйвер аудио (для эмуляции, т.к. реальный аудио-аппарат не поддерживается в текущем виде)
pub struct VirtualAudioDriver {
    pub mixer: AudioMixer,
    pub is_paused: bool,
    pub spec: AudioSpec,
}

impl VirtualAudioDriver {
    pub fn new() -> Self {
        let default_spec = AudioSpec {
            freq: 44100,
            format: AudioFormat::S16,
            channels: ChannelLayout::Stereo,
            samples: 1024,
        };

        VirtualAudioDriver {
            mixer: AudioMixer::new(default_spec.clone()),
            is_paused: true,
            spec: default_spec,
        }
    }
}

impl AudioDriver for VirtualAudioDriver {
    fn init(&mut self, spec: &AudioSpec) -> Result<(), &'static str> {
        self.spec = spec.clone();
        self.mixer = AudioMixer::new(spec.clone());
        Ok(())
    }

    fn play(&mut self, buffer: &AudioBuffer) -> Result<(), &'static str> {
        let channel_id = self.mixer.add_channel();
        self.mixer.load_sample(channel_id, buffer.clone());
        self.mixer.play_channel(channel_id);
        Ok(())
    }

    fn pause(&mut self) {
        self.is_paused = true;
    }

    fn resume(&mut self) {
        self.is_paused = false;
    }

    fn stop(&mut self) {
        // Останавливаем все каналы
        for channel in &mut self.mixer.channels {
            channel.playing = false;
        }
    }

    fn set_volume(&mut self, volume: f32) {
        self.mixer.master_volume = volume.clamp(0.0, 1.0);
    }

    fn get_spec(&self) -> &AudioSpec {
        &self.spec
    }
}

// Глобальные функции аудио
use spin::Once;

static AUDIO_DRIVER_ONCE: Once<Mutex<VirtualAudioDriver>> = Once::new();

pub fn get_audio_driver() -> &'static Mutex<VirtualAudioDriver> {
    AUDIO_DRIVER_ONCE.call_once(|| Mutex::new(VirtualAudioDriver::new()))
}

pub fn init_audio() -> Result<(), &'static str> {
    let mut driver = get_audio_driver().lock();
    let spec = AudioSpec {
        freq: 44100,
        format: AudioFormat::S16,
        channels: ChannelLayout::Stereo,
        samples: 1024,
    };
    driver.init(&spec)
}

pub fn play_sound(samples: &[i16]) -> Result<(), &'static str> {
    let mut driver = get_audio_driver().lock();
    let spec = driver.get_spec().clone();
    let buffer = AudioBuffer {
        data: samples.to_vec(),
        spec,
    };
    driver.play(&buffer)
}

pub fn set_master_volume(volume: f32) {
    let mut driver = get_audio_driver().lock();
    driver.set_volume(volume);
}

pub fn pause_audio() {
    let mut driver = get_audio_driver().lock();
    driver.pause();
}

pub fn resume_audio() {
    let mut driver = get_audio_driver().lock();
    driver.resume();
}

pub fn stop_audio() {
    let mut driver = get_audio_driver().lock();
    driver.stop();
}

// Пример генерации простого звукового сигнала (например, квадратной волны)
pub fn generate_tone(frequency: f32, duration_ms: u32, sample_rate: u32) -> Vec<i16> {
    let samples_count = (sample_rate as f32 * duration_ms as f32 / 1000.0) as usize;
    let mut samples = Vec::with_capacity(samples_count);

    for i in 0..samples_count {
        let t = i as f32 / sample_rate as f32;
        // Используем квадратную волну вместо sin, так как в no_std нет математических функций
        let period = 1.0 / frequency;
        let phase = (t % period) / period;
        let sample = if phase < 0.5 { i16::MAX } else { i16::MIN };
        samples.push(sample);
    }

    samples
}

pub fn play_tone(frequency: f32, duration_ms: u32) -> Result<(), &'static str> {
    let samples = generate_tone(frequency, duration_ms, 44100);
    play_sound(&samples)
}
