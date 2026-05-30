use font8x8::UnicodeFonts;
use spin::Mutex;

use crate::graphics::{Color, DrawTarget};

/// Эмулятор терминала поверх фреймбуфера с поддержкой ANSI.
pub struct Terminal {
    x: usize, // Текущая колонка (в пикселях: x * 8)
    y: usize, // Текущая строка (в пикселях: y * 8)
    cols: usize,
    rows: usize,
    history: alloc::vec::Vec<alloc::vec::Vec<(char, Color, Color)>>,
    max_history: usize,

    fg_color: Color,
    bg_color: Color,
    escape_state: u8,
    escape_buf: alloc::string::String,
}

pub static TERMINAL: Mutex<Option<Terminal>> = Mutex::new(None);

pub fn init(width: usize, height: usize) {
    *TERMINAL.lock() = Some(Terminal::new(width, height));
}

impl Terminal {
    pub fn new(width: usize, height: usize) -> Self {
        Terminal {
            x: 0,
            y: 0,
            cols: width / 8,
            rows: height / 8,
            history: alloc::vec![alloc::vec::Vec::new()],
            max_history: 1000,
            fg_color: Color::WHITE,
            bg_color: Color::from_u32(0x222222),
            escape_state: 0,
            escape_buf: alloc::string::String::new(),
        }
    }

    fn new_line(&mut self, target: &mut dyn DrawTarget) {
        self.y += 1;
        self.x = 0;
        self.history.push(alloc::vec::Vec::new());
        if self.history.len() > self.max_history {
            self.history.remove(0);
        }

        if self.y >= self.rows {
            self.y = self.rows - 1;
            target.scroll_up(8);
        }
    }

    pub fn print_char(&mut self, target: &mut dyn DrawTarget, c: char) {
        if self.escape_state == 1 {
            if c == '[' {
                self.escape_state = 2;
                self.escape_buf.clear();
            } else {
                self.escape_state = 0;
            }
            return;
        } else if self.escape_state == 2 {
            if (c >= '0' && c <= '9') || c == ';' {
                self.escape_buf.push(c);
            } else if c == 'm' {
                let parts = self.escape_buf.split(';');
                for p in parts {
                    match p {
                        "0" => {
                            self.fg_color = Color::WHITE;
                            self.bg_color = Color::from_u32(0x222222);
                        },
                        "30" => self.fg_color = Color::BLACK,
                        "31" => self.fg_color = Color::RED,
                        "32" => self.fg_color = Color::GREEN,
                        "33" => self.fg_color = Color::YELLOW,
                        "34" => self.fg_color = Color::BLUE,
                        "35" => self.fg_color = Color::MAGENTA,
                        "36" => self.fg_color = Color::CYAN,
                        "37" => self.fg_color = Color::WHITE,
                        "90" => self.fg_color = Color::from_u32(0x555555),
                        "91" => self.fg_color = Color::from_u32(0xFF5555),
                        "92" => self.fg_color = Color::from_u32(0x55FF55),
                        "93" => self.fg_color = Color::from_u32(0xFFFF55),
                        "94" => self.fg_color = Color::from_u32(0x5555FF),
                        "95" => self.fg_color = Color::from_u32(0xFF55FF),
                        "96" => self.fg_color = Color::from_u32(0x55FFFF),
                        "97" => self.fg_color = Color::WHITE,
                        "40" => self.bg_color = Color::BLACK,
                        "41" => self.bg_color = Color::RED,
                        "42" => self.bg_color = Color::GREEN,
                        "43" => self.bg_color = Color::YELLOW,
                        "44" => self.bg_color = Color::BLUE,
                        "45" => self.bg_color = Color::MAGENTA,
                        "46" => self.bg_color = Color::CYAN,
                        "47" => self.bg_color = Color::WHITE,
                        _ => {},
                    }
                }
                self.escape_state = 0;
            } else {
                self.escape_state = 0;
            }
            return;
        } else if c == '\x1b' {
            self.escape_state = 1;
            return;
        }

        if c == '\n' {
            self.new_line(target);
            return;
        }

        if self.x >= self.cols {
            self.new_line(target);
        }

        if let Some(glyph) = font8x8::BASIC_FONTS
            .get(c)
            .or_else(|| font8x8::LATIN_FONTS.get(c))
        {
            let fg = self.fg_color;
            let bg = self.bg_color;

            for (r, &row_byte) in glyph.iter().enumerate() {
                let mut row = row_byte;
                for c_idx in 0..8 {
                    let px_x = self.x * 8 + c_idx;
                    let px_y = self.y * 8 + r;

                    if (row & 1) != 0 {
                        target.draw_pixel(px_x, px_y, fg);
                    } else {
                        target.draw_pixel(px_x, px_y, bg);
                    }
                    row >>= 1;
                }
            }
            if let Some(line) = self.history.last_mut() {
                line.push((c, fg, bg));
            }
        }
        self.x += 1;
    }

    pub fn print_str(&mut self, target: &mut dyn DrawTarget, s: &str) {
        for c in s.chars() {
            self.print_char(target, c);
        }
    }

    pub fn backspace(&mut self, target: &mut dyn DrawTarget) {
        if self.x > 0 {
            self.x -= 1;
        } else if self.y > 0 {
            self.y -= 1;
            self.x = self.cols - 1;
        } else {
            return;
        }

        let bg = self.bg_color;
        for r in 0..8 {
            for c_idx in 0..8 {
                let px_x = self.x * 8 + c_idx;
                let px_y = self.y * 8 + r;
                target.draw_pixel(px_x, px_y, bg);
            }
        }
        if let Some(line) = self.history.last_mut() {
            line.pop();
        }
    }

    pub fn redraw(&self, target: &mut dyn DrawTarget, scroll_offset: usize) {
        target.fill(Color::from_u32(0x222222));
        let start_line = self.history.len().saturating_sub(self.rows + scroll_offset);
        let end_line = (start_line + self.rows).min(self.history.len());

        for (row, line_idx) in (start_line..end_line).enumerate() {
            let line = &self.history[line_idx];
            for (col, &(c, fg, bg)) in line.iter().enumerate() {
                if col >= self.cols {
                    break;
                }
                self.print_char_at(target, c, fg, bg, col, row);
            }
        }
    }

    fn print_char_at(
        &self,
        target: &mut dyn DrawTarget,
        c: char,
        fg: Color,
        bg: Color,
        x: usize,
        y: usize,
    ) {
        if let Some(glyph) = font8x8::BASIC_FONTS
            .get(c)
            .or_else(|| font8x8::LATIN_FONTS.get(c))
        {
            for (r, &row_byte) in glyph.iter().enumerate() {
                let mut row = row_byte;
                for c_idx in 0..8 {
                    let px_x = x * 8 + c_idx;
                    let px_y = y * 8 + r;
                    if (row & 1) != 0 {
                        target.draw_pixel(px_x, px_y, fg);
                    } else {
                        target.draw_pixel(px_x, px_y, bg);
                    }
                    row >>= 1;
                }
            }
        }
    }

    pub fn reset_cursor(&mut self) {
        self.x = 0;
        self.y = 0;
    }
}
