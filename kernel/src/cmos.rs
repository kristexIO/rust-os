// cmos.rs — Real Time Clock (RTC) Driver

use spin::Mutex;
use x86_64::instructions::port::Port;

lazy_static::lazy_static! {
    static ref CMOS_ADDR: Mutex<Port<u8>> = Mutex::new(Port::new(0x70));
    static ref CMOS_DATA: Mutex<Port<u8>> = Mutex::new(Port::new(0x71));
}

fn get_update_in_progress_flag() -> u8 {
    unsafe {
        CMOS_ADDR.lock().write(0x0A);
        CMOS_DATA.lock().read() & 0x80
    }
}

fn get_rtc_register(reg: u8) -> u8 {
    unsafe {
        CMOS_ADDR.lock().write(reg);
        CMOS_DATA.lock().read()
    }
}

/// Returns (year, month, day, hour, minute, second)
pub fn read_rtc() -> (u8, u8, u8, u8, u8, u8) {
    while get_update_in_progress_flag() != 0 {}

    let mut sec = get_rtc_register(0x00);
    let mut min = get_rtc_register(0x02);
    let mut hour = get_rtc_register(0x04);
    let mut day = get_rtc_register(0x07);
    let mut month = get_rtc_register(0x08);
    let mut year = get_rtc_register(0x09);

    let register_b = get_rtc_register(0x0B);

    // Convert BCD to binary if necessary
    if (register_b & 0x04) == 0 {
        sec = (sec & 0x0F) + ((sec / 16) * 10);
        min = (min & 0x0F) + ((min / 16) * 10);
        hour = ((hour & 0x0F) + (((hour & 0x70) / 16) * 10)) | (hour & 0x80);
        day = (day & 0x0F) + ((day / 16) * 10);
        month = (month & 0x0F) + ((month / 16) * 10);
        year = (year & 0x0F) + ((year / 16) * 10);
    }

    // Convert 12-hour clock to 24-hour clock if necessary
    if (register_b & 0x02) == 0 && (hour & 0x80) != 0 {
        hour = ((hour & 0x7F) + 12) % 24;
    }

    (year, month, day, hour, min, sec)
}
