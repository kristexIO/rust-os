//! Скелет декодера MP3.
//! Для полноценной реализации требуется сложный математический аппарат (IMDCT,
//! Huffman, etc).

pub struct Mp3Frame {
    pub header: u32,
    pub data: [u8; 1024], // Упрощенно
}

pub struct Mp3Decoder {
    pub bit_rate: u32,
    pub sampling_freq: u32,
    pub is_stereo: bool,
}

impl Mp3Decoder {
    pub fn new() -> Self {
        Mp3Decoder {
            bit_rate: 0,
            sampling_freq: 0,
            is_stereo: false,
        }
    }

    /// Декодирует один фрейм MP3 в PCM
    pub fn decode_frame(&mut self, _mp3_data: &[u8]) -> Option<alloc::vec::Vec<i16>> {
        // 1. Поиск синхронизации (0xFFE)
        // 2. Чтение заголовка (Layer III, CRC)
        // 3. Huffman декомпрессия
        // 4. Inverse Quantization
        // 5. IMDCT (Inverse Modified Discrete Cosine Transform)
        // 6. Polyphase Synthesis Filterbank

        None // Заглушка
    }
}

pub fn play_mp3(_path: &str) {
    // В будущем: чтение из VFS -> декодирование -> AudioMixer
}
