use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct Partition {
    pub start_sector: u64,
    pub sector_count: u64,
    pub part_type: u8,
}

/// Парсит MBR (Master Boot Record) в первом секторе диска
pub fn parse_mbr(data: &[u8]) -> Vec<Partition> {
    let mut parts = Vec::new();
    if data.len() < 512 {
        return parts;
    }

    // Проверка сигнатуры 0x55AA
    if data[510] != 0x55 || data[511] != 0xAA {
        return parts;
    }

    // Таблица разделов начинается с 446 смещения
    for i in 0..4 {
        let offset = 446 + i * 16;
        let part_type = data[offset + 4];
        if part_type == 0 {
            continue;
        }

        let start_sector =
            u32::from_le_bytes(data[offset + 8..offset + 12].try_into().unwrap()) as u64;
        let sector_count =
            u32::from_le_bytes(data[offset + 12..offset + 16].try_into().unwrap()) as u64;

        parts.push(Partition {
            start_sector,
            sector_count,
            part_type,
        });
    }

    parts
}

/// Простейший парсер GPT (GUID Partition Table)
/// Для полноценной работы нужно читать и MBR (защитный) и заголовок GPT.
pub fn parse_gpt(header_data: &[u8]) -> Vec<Partition> {
    let parts = Vec::new();
    if header_data.len() < 512 {
        return parts;
    }

    // Проверка сигнатуры "EFI PART" (8 байт)
    if &header_data[0..8] != b"EFI PART" {
        return parts;
    }

    // В реальности нужно прочитать еще и сами записи разделов (обычно LBA 2+)
    // Тут пока заглушка.
    parts
}
