// net/utils.rs — Вспомогательные функции для сети

pub fn htons(val: u16) -> u16 {
    val.to_be()
}

pub fn ntohs(val: u16) -> u16 {
    u16::from_be(val)
}

pub fn process_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i < data.len() {
        let word = if i + 1 < data.len() {
            ((data[i] as u32) << 8) | (data[i + 1] as u32)
        } else {
            (data[i] as u32) << 8
        };
        sum = sum.wrapping_add(word);
        i += 2;
    }

    while (sum >> 16) > 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    !(sum as u16)
}
