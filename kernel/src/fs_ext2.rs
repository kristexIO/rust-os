use alloc::{
    string::{String, ToString},
    vec,
    vec::Vec,
};

// Простой каркас файловой системы ext2
// В будущем будет расширен для полной поддержки

pub struct Ext2FileSystem {
    // Пока заглушка - в будущем будет содержать информацию о смонтированной ФС
    is_mounted: bool,
}

impl Ext2FileSystem {
    pub fn new() -> Self {
        Ext2FileSystem { is_mounted: false }
    }

    pub fn mount(&mut self) -> Result<(), &'static str> {
        // Заглушка для демонстрации
        // В реальной реализации здесь будут:
        // - Чтение суперблока
        // - Проверка сигнатуры ext2
        // - Анализ структуры файловой системы
        self.is_mounted = true;
        Ok(())
    }

    pub fn is_mounted(&self) -> bool {
        self.is_mounted
    }

    pub fn create_file(&mut self, _path: &str) -> Result<(), &'static str> {
        if !self.is_mounted {
            return Err("Filesystem not mounted");
        }
        // Заглушка
        Ok(())
    }

    pub fn read_file(&self, _path: &str) -> Result<Vec<u8>, &'static str> {
        if !self.is_mounted {
            return Err("Filesystem not mounted");
        }
        // Заглушка - возвращаем пример содержимого
        Ok(b"This is sample content from ext2 filesystem\n".to_vec())
    }

    pub fn write_file(&mut self, _path: &str, _data: &[u8]) -> Result<(), &'static str> {
        if !self.is_mounted {
            return Err("Filesystem not mounted");
        }
        // Заглушка
        Ok(())
    }

    pub fn list_directory(&self, _path: &str) -> Result<Vec<String>, &'static str> {
        if !self.is_mounted {
            return Err("Filesystem not mounted");
        }
        // Заглушка
        Ok(vec![
            "file1.txt".to_string(),
            "file2.txt".to_string(),
            "subdir".to_string(),
        ])
    }
}

// Вспомогательные функции для проверки возможностей ext2
pub fn is_ext2_compatible() -> bool {
    // Возвращает true если система поддерживает ext2
    true
}

pub fn detect_filesystem(sector_data: &[u8]) -> Option<&'static str> {
    // Проверяет сигнатуру файловой системы в первом секторе
    // Сигнатура ext2 находится в суперблоке (смещение 56-57)
    if sector_data.len() >= 58 {
        let magic_low = sector_data[56];
        let magic_high = sector_data[57];
        let magic = (magic_high as u16) << 8 | (magic_low as u16);

        if magic == 0xEF53 {
            // EXT2_SIGNATURE
            return Some("ext2");
        }
    }
    None
}
