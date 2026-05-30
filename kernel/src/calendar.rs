/// Calendar — календарь с датами из RTC
use alloc::string::String;

use font8x8::UnicodeFonts;

use crate::{
    graphics::Color,
    window::{Window, TITLE_BAR_H},
};

#[derive(Clone)]
pub struct CalendarState {
    pub current_month: u8,
    pub current_year: u16,
}

impl CalendarState {
    pub fn new() -> Self {
        let (_, month, _, _, _, _) = crate::cmos::read_rtc();
        let (_, _, _, year_low, _, _) = crate::cmos::read_rtc();
        Self {
            current_month: month,
            current_year: 2000 + year_low as u16,
        }
    }

    pub fn next_month(&mut self) {
        if self.current_month == 12 {
            self.current_month = 1;
            self.current_year += 1;
        } else {
            self.current_month += 1;
        }
    }

    pub fn prev_month(&mut self) {
        if self.current_month == 1 {
            self.current_month = 12;
            self.current_year -= 1;
        } else {
            self.current_month -= 1;
        }
    }
}

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const DAY_NAMES: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

fn days_in_month(month: u8, year: u16) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0) {
                29
            } else {
                28
            }
        },
        _ => 30,
    }
}

fn first_day_of_month(month: u8, year: u16) -> u8 {
    // Zeller's congruence (Monday = 0)
    let m = if month <= 2 {
        month as u16 + 12
    } else {
        month as u16
    };
    let y = if month <= 2 { year - 1 } else { year };
    let q = 1u16;
    let k = y % 100;
    let j = y / 100;
    let h = (q + 13 * (m + 1) / 5 + k + k / 4 + j / 4 - 2 * j) % 7;
    ((h + 5) % 7) as u8
}

pub fn render_calendar(win: &mut Window, state: &CalendarState) {
    let bw = win.width();
    let bh = win.height();
    let buf = &mut win.buffer;

    fill_buf(
        buf,
        bw,
        0,
        TITLE_BAR_H,
        bw,
        bh - TITLE_BAR_H,
        Color::from_u32(0x1A1B26),
    );

    // Header
    let month_name = MONTH_NAMES[(state.current_month - 1) as usize];
    let mut header = String::from(month_name);
    header.push(' ');
    push_u64(&mut header, state.current_year as u64);

    draw_str_buf(
        buf,
        bw,
        bh,
        (bw - header.len() * 8) / 2,
        TITLE_BAR_H + 10,
        &header,
        Color::from_u32(0x7AA2F7),
        1,
    );

    // Navigation
    draw_str_buf(
        buf,
        bw,
        bh,
        20,
        TITLE_BAR_H + 10,
        "<",
        Color::from_u32(0x9ECE6A),
        1,
    );
    draw_str_buf(
        buf,
        bw,
        bh,
        bw - 28,
        TITLE_BAR_H + 10,
        ">",
        Color::from_u32(0x9ECE6A),
        1,
    );

    // Day names
    let day_y = TITLE_BAR_H + 36;
    let cell_w = 32;
    let start_x = (bw - 7 * cell_w) / 2;

    for (i, day) in DAY_NAMES.iter().enumerate() {
        let color = if i >= 5 {
            Color::from_u32(0xF7768E)
        } else {
            Color::from_u32(0x565F89)
        };
        draw_str_buf(buf, bw, bh, start_x + i * cell_w + 8, day_y, day, color, 1);
    }

    // Calendar grid
    let days = days_in_month(state.current_month, state.current_year);
    let first_day = first_day_of_month(state.current_month, state.current_year);

    let (_, _, day, _, _, _) = crate::cmos::read_rtc();
    let (_, month, _, year_low, _, _) = crate::cmos::read_rtc();
    let is_current_month =
        month == state.current_month && (2000 + year_low as u16) == state.current_year;

    let grid_y = day_y + 20;
    let mut day_num = 1;

    for row in 0..6 {
        for col in 0..7 {
            let cell_idx = row * 7 + col;
            if cell_idx < first_day as usize || day_num > days {
                continue;
            }

            let x = start_x + col * cell_w;
            let y = grid_y + row * 20;

            let is_today = is_current_month && day_num == day;
            let is_weekend = col >= 5;

            let color = if is_today {
                Color::from_u32(0x7AA2F7)
            } else if is_weekend {
                Color::from_u32(0xF7768E)
            } else {
                Color::from_u32(0xC0CAF5)
            };

            // Background for today
            if is_today {
                fill_buf(buf, bw, x, y, cell_w, 16, Color::from_u32(0x3D59A1));
            }

            let mut num = String::new();
            push_u64(&mut num, day_num as u64);
            draw_str_buf(buf, bw, bh, x + 8, y, &num, color, 1);

            day_num += 1;
        }
    }

    // Today info
    let info_y = grid_y + 6 * 20 + 16;
    let mut today_str = String::from("Today: ");
    push_u64(&mut today_str, day as u64);
    today_str.push('.');
    push_u64(&mut today_str, month as u64);
    today_str.push('.');
    push_u64(&mut today_str, (2000 + year_low as u16) as u64);
    draw_str_buf(
        buf,
        bw,
        bh,
        20,
        info_y,
        &today_str,
        Color::from_u32(0x9ECE6A),
        1,
    );
}

fn fill_buf(buf: &mut [Color], bw: usize, x: usize, y: usize, w: usize, h: usize, color: Color) {
    let x_end = (x + w).min(bw);
    let bh = buf.len() / bw.max(1);
    let y_end = (y + h).min(bh);
    for row in y..y_end {
        for col in x..x_end {
            let idx = row * bw + col;
            if idx < buf.len() {
                buf[idx] = color;
            }
        }
    }
}

fn draw_str_buf(
    buf: &mut [Color],
    bw: usize,
    bh: usize,
    x: usize,
    y: usize,
    s: &str,
    fg: Color,
    scale: usize,
) {
    let mut cx = x;
    for ch in s.chars() {
        if cx + 8 * scale >= bw {
            break;
        }
        let glyph = font8x8::BASIC_FONTS
            .get(ch)
            .or_else(|| font8x8::LATIN_FONTS.get(ch));
        if let Some(glyph) = glyph {
            for (row, &byte) in glyph.iter().enumerate() {
                let mut b = byte;
                for col in 0..8usize {
                    if b & 1 != 0 {
                        for sy in 0..scale {
                            for sx in 0..scale {
                                let px = cx + col * scale + sx;
                                let py = y + row * scale + sy;
                                if px < bw && py < bh {
                                    let idx = py * bw + px;
                                    if idx < buf.len() {
                                        buf[idx] = fg;
                                    }
                                }
                            }
                        }
                    }
                    b >>= 1;
                }
            }
        }
        cx += 8 * scale;
    }
}

fn push_u64(s: &mut String, mut v: u64) {
    if v == 0 {
        s.push('0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut len = 0usize;
    while v > 0 {
        buf[len] = (v % 10) as u8;
        v /= 10;
        len += 1;
    }
    for i in (0..len).rev() {
        s.push((b'0' + buf[i]) as char);
    }
}
