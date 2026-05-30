/// Калькулятор — виджет для встраивания в Window.
///
/// Рендерит кнопочную раскладку и дисплей прямо в Window.buffer.
/// Обрабатывает клики через hit-test.
///
/// Поддерживаемые операции: +, -, *, /, %, +/-, decimal, backspace, clear, =
use alloc::string::String;

use font8x8::UnicodeFonts;

use crate::{
    graphics::Color,
    window::{Window, TITLE_BAR_H},
};

// ─── Цвета ────────────────────────────────────────────────────────────────────
const DISP_BG: Color = Color::from_u32(0x1a1f2e);
const DISP_FG: Color = Color::from_u32(0xffffff);
const DISP_SEC: Color = Color::from_u32(0x8899bb);
const BTN_NUM: Color = Color::from_u32(0x2e3448);
const BTN_OP: Color = Color::from_u32(0x3b5fa8);
const BTN_EQ: Color = Color::from_u32(0xe67e22);
const BTN_CLR: Color = Color::from_u32(0x8b2222);
const BTN_HOV: Color = Color::from_u32(0x4a5570);
const BTN_BRD: Color = Color::from_u32(0x1a1d2a);
const WIN_BG: Color = Color::from_u32(0x1e2233);

// ─── Лэйаут
// ───────────────────────────────────────────────────────────────────
const BTN_W: usize = 56;
const BTN_H: usize = 40;
const BTN_PAD: usize = 4;
const DISP_H: usize = 64;

/// Тип кнопки
#[derive(Clone, Copy, PartialEq)]
pub enum BtnKind {
    Digit(u8), // 0-9
    Dot,       // .
    Op(char),  // + - * /
    Eq,        // =
    Clear,     // C
    AllClear,  // AC
    Backspace, // ←
    Percent,   // %
    PlusMinus, // +/-
    Sin,
    Cos,
    Tan,
    Sqrt,
    Pow2,
}

impl BtnKind {
    fn label(self) -> &'static str {
        match self {
            BtnKind::Digit(0) => "0",
            BtnKind::Digit(1) => "1",
            BtnKind::Digit(2) => "2",
            BtnKind::Digit(3) => "3",
            BtnKind::Digit(4) => "4",
            BtnKind::Digit(5) => "5",
            BtnKind::Digit(6) => "6",
            BtnKind::Digit(7) => "7",
            BtnKind::Digit(8) => "8",
            BtnKind::Digit(9) => "9",
            BtnKind::Digit(_) => "?",
            BtnKind::Dot => ".",
            BtnKind::Op('+') => "+",
            BtnKind::Op('-') => "-",
            BtnKind::Op('*') => "x",
            BtnKind::Op('/') => "/",
            BtnKind::Op(_) => "?",
            BtnKind::Eq => "=",
            BtnKind::Clear => "C",
            BtnKind::AllClear => "AC",
            BtnKind::Backspace => "<",
            BtnKind::Percent => "%",
            BtnKind::PlusMinus => "+/-",
            BtnKind::Sin => "sin",
            BtnKind::Cos => "cos",
            BtnKind::Tan => "tan",
            BtnKind::Sqrt => "sqrt",
            BtnKind::Pow2 => "x^2",
        }
    }

    fn color(self) -> Color {
        match self {
            BtnKind::Eq => BTN_EQ,
            BtnKind::Clear | BtnKind::AllClear => BTN_CLR,
            BtnKind::Op(_) => BTN_OP,
            _ => BTN_NUM,
        }
    }
}

/// Позиция кнопки (столбец, строка) в сетке
#[derive(Clone, Copy)]
struct BtnPos {
    col: usize,
    row: usize,
    span: usize,
}

/// Полная кнопка
#[derive(Clone, Copy)]
pub struct Btn {
    pub kind: BtnKind,
    pos: BtnPos,
}

// Раскладка 4x5 + нижняя строка с широкой "0":
// [AC] [+/-] [%] [/]
// [ 7] [  8] [9] [*]
// [ 4] [  5] [6] [-]
// [ 1] [  2] [3] [+]
// [ 0 (span2)] [.] [=]
const LAYOUT: &[Btn] = &[
    Btn {
        kind: BtnKind::AllClear,
        pos: BtnPos {
            col: 0,
            row: 0,
            span: 1,
        },
    },
    Btn {
        kind: BtnKind::PlusMinus,
        pos: BtnPos {
            col: 1,
            row: 0,
            span: 1,
        },
    },
    Btn {
        kind: BtnKind::Percent,
        pos: BtnPos {
            col: 2,
            row: 0,
            span: 1,
        },
    },
    Btn {
        kind: BtnKind::Op('/'),
        pos: BtnPos {
            col: 3,
            row: 0,
            span: 1,
        },
    },
    Btn {
        kind: BtnKind::Digit(7),
        pos: BtnPos {
            col: 0,
            row: 1,
            span: 1,
        },
    },
    Btn {
        kind: BtnKind::Digit(8),
        pos: BtnPos {
            col: 1,
            row: 1,
            span: 1,
        },
    },
    Btn {
        kind: BtnKind::Digit(9),
        pos: BtnPos {
            col: 2,
            row: 1,
            span: 1,
        },
    },
    Btn {
        kind: BtnKind::Op('*'),
        pos: BtnPos {
            col: 3,
            row: 1,
            span: 1,
        },
    },
    Btn {
        kind: BtnKind::Digit(4),
        pos: BtnPos {
            col: 0,
            row: 2,
            span: 1,
        },
    },
    Btn {
        kind: BtnKind::Digit(5),
        pos: BtnPos {
            col: 1,
            row: 2,
            span: 1,
        },
    },
    Btn {
        kind: BtnKind::Digit(6),
        pos: BtnPos {
            col: 2,
            row: 2,
            span: 1,
        },
    },
    Btn {
        kind: BtnKind::Op('-'),
        pos: BtnPos {
            col: 3,
            row: 2,
            span: 1,
        },
    },
    Btn {
        kind: BtnKind::Digit(1),
        pos: BtnPos {
            col: 0,
            row: 3,
            span: 1,
        },
    },
    Btn {
        kind: BtnKind::Digit(2),
        pos: BtnPos {
            col: 1,
            row: 3,
            span: 1,
        },
    },
    Btn {
        kind: BtnKind::Digit(3),
        pos: BtnPos {
            col: 2,
            row: 3,
            span: 1,
        },
    },
    Btn {
        kind: BtnKind::Op('+'),
        pos: BtnPos {
            col: 3,
            row: 3,
            span: 1,
        },
    },
    Btn {
        kind: BtnKind::Digit(0),
        pos: BtnPos {
            col: 0,
            row: 4,
            span: 2,
        },
    },
    Btn {
        kind: BtnKind::Dot,
        pos: BtnPos {
            col: 2,
            row: 4,
            span: 1,
        },
    },
    Btn {
        kind: BtnKind::Eq,
        pos: BtnPos {
            col: 3,
            row: 4,
            span: 1,
        },
    },
    // Col 4: Scientific Functions
    Btn {
        kind: BtnKind::Sin,
        pos: BtnPos {
            col: 4,
            row: 0,
            span: 1,
        },
    },
    Btn {
        kind: BtnKind::Cos,
        pos: BtnPos {
            col: 4,
            row: 1,
            span: 1,
        },
    },
    Btn {
        kind: BtnKind::Tan,
        pos: BtnPos {
            col: 4,
            row: 2,
            span: 1,
        },
    },
    Btn {
        kind: BtnKind::Sqrt,
        pos: BtnPos {
            col: 4,
            row: 3,
            span: 1,
        },
    },
    Btn {
        kind: BtnKind::Pow2,
        pos: BtnPos {
            col: 4,
            row: 4,
            span: 1,
        },
    },
];

// ─── Состояние калькулятора
// ───────────────────────────────────────────────────

#[derive(Clone)]
pub struct Calculator {
    /// Текущий ввод на дисплее
    pub display: String,
    /// Левый операнд
    lhs: f64,
    /// Текущая операция
    op: Option<char>,
    /// Ожидаем ли новый ввод (после нажатия оператора/=)
    awaiting_rhs: bool,
    /// Была ли нажата точка
    has_dot: bool,
    /// Последний результат (для повторного =)
    last_rhs: f64,
    last_op: Option<char>,
    /// История операций
    pub history: alloc::vec::Vec<String>,
}

impl Calculator {
    pub fn new() -> Self {
        Calculator {
            display: String::from("0"),
            lhs: 0.0,
            op: None,
            awaiting_rhs: false,
            has_dot: false,
            last_rhs: 0.0,
            last_op: None,
            history: alloc::vec::Vec::new(),
        }
    }

    pub fn press(&mut self, btn: BtnKind) {
        match btn {
            BtnKind::Digit(d) => {
                if self.awaiting_rhs {
                    self.display.clear();
                    self.has_dot = false;
                    self.awaiting_rhs = false;
                }
                if self.display == "0" {
                    self.display.clear();
                }
                if self.display.len() < 12 {
                    self.display.push((b'0' + d) as char);
                }
            },
            BtnKind::Dot => {
                if self.awaiting_rhs {
                    self.display = String::from("0");
                    self.awaiting_rhs = false;
                    self.has_dot = false;
                }
                if !self.has_dot {
                    self.has_dot = true;
                    if self.display.is_empty() {
                        self.display.push('0');
                    }
                    self.display.push('.');
                }
            },
            BtnKind::Op(c) => {
                let cur = self.parse_display();
                if let Some(prev_op) = self.op {
                    if !self.awaiting_rhs {
                        self.lhs = Self::apply(self.lhs, cur, prev_op);
                        self.display = format_f64(self.lhs);
                    }
                } else {
                    self.lhs = cur;
                }
                self.op = Some(c);
                self.awaiting_rhs = true;
                self.has_dot = false;
            },
            BtnKind::Eq => {
                let rhs = if self.awaiting_rhs {
                    self.last_rhs
                } else {
                    let r = self.parse_display();
                    self.last_rhs = r;
                    r
                };
                let op = self.op.or(self.last_op).unwrap_or('+');
                self.last_op = Some(op);

                let result = Self::apply(self.lhs, rhs, op);

                let mut eq_str = format_f64(self.lhs);
                eq_str.push(' ');
                eq_str.push(op);
                eq_str.push(' ');
                eq_str.push_str(&format_f64(rhs));
                eq_str.push_str(" = ");
                eq_str.push_str(&format_f64(result));
                self.history.push(eq_str);

                self.lhs = result;
                self.display = format_f64(result);
                self.op = None;
                self.awaiting_rhs = true;
                self.has_dot = self.display.contains('.');
            },
            BtnKind::Clear => {
                self.display = String::from("0");
                self.has_dot = false;
                self.awaiting_rhs = false;
            },
            BtnKind::AllClear => {
                *self = Calculator::new();
            },
            BtnKind::Backspace => {
                if self.display.len() > 1 {
                    self.display.pop();
                    if self.display.ends_with('.') {
                        self.display.pop();
                        self.has_dot = false;
                    }
                } else {
                    self.display = String::from("0");
                }
            },
            BtnKind::Percent => {
                let v = self.parse_display() / 100.0;
                self.display = format_f64(v);
            },
            BtnKind::PlusMinus => {
                if self.display.starts_with('-') {
                    self.display.remove(0);
                } else if self.display != "0" {
                    self.display.insert(0, '-');
                }
            },
            BtnKind::Sin | BtnKind::Cos | BtnKind::Tan | BtnKind::Sqrt | BtnKind::Pow2 => {
                let cur = self.parse_display();
                use crate::math::FloatExt;
                let (res, name) = match btn {
                    BtnKind::Sin => ((cur as f32).to_radians().sin() as f64, "sin("),
                    BtnKind::Cos => ((cur as f32).to_radians().cos() as f64, "cos("),
                    BtnKind::Tan => ((cur as f32).to_radians().tan() as f64, "tan("),
                    BtnKind::Sqrt => ((cur as f32).sqrt() as f64, "sqrt("),
                    BtnKind::Pow2 => (cur * cur, "sqr("),
                    _ => (cur, ""),
                };

                let mut eq_str = String::from(name);
                eq_str.push_str(&format_f64(cur));
                eq_str.push_str(") = ");
                eq_str.push_str(&format_f64(res));
                self.history.push(eq_str);

                self.display = format_f64(res);
                self.lhs = res;
                self.op = None;
                self.awaiting_rhs = true;
                self.has_dot = self.display.contains('.');
            },
        }
    }

    fn parse_display(&self) -> f64 {
        parse_f64(&self.display)
    }

    fn apply(a: f64, b: f64, op: char) -> f64 {
        match op {
            '+' => a + b,
            '-' => a - b,
            '*' => a * b,
            '/' => {
                if b != 0.0 {
                    a / b
                } else {
                    f64::INFINITY
                }
            },
            _ => a,
        }
    }
}

// ─── Рендер
// ───────────────────────────────────────────────────────────────────

/// Вычисляет пиксельные координаты кнопки внутри окна (с учётом TITLE_BAR_H)
fn btn_rect(b: &Btn) -> (usize, usize, usize, usize) {
    let x = BTN_PAD + b.pos.col * (BTN_W + BTN_PAD);
    let y = TITLE_BAR_H + BTN_PAD + DISP_H + b.pos.row * (BTN_H + BTN_PAD);
    let w = b.pos.span * BTN_W + (b.pos.span - 1) * BTN_PAD;
    (x, y, w, BTN_H)
}

/// Рисует весь калькулятор в буфер окна
pub fn render_calc(win: &mut Window, calc: &Calculator, hovered: Option<usize>) {
    let ww = win.width();
    let wh = win.height();
    let buf = &mut win.buffer;

    // Фон окна
    fill_buf(buf, ww, 0, TITLE_BAR_H, ww, wh - TITLE_BAR_H, WIN_BG);

    // Дисплей
    let disp_x = BTN_PAD;
    let disp_y = TITLE_BAR_H + BTN_PAD;
    let disp_w = 5 * (BTN_W + BTN_PAD) - BTN_PAD;
    fill_buf(buf, ww, disp_x, disp_y, disp_w, DISP_H, DISP_BG);
    // Рамка дисплея
    draw_rect_outline_buf(
        buf,
        ww,
        disp_x,
        disp_y,
        disp_w,
        DISP_H,
        Color::from_u32(0x3a4060),
    );

    // Текст на дисплее — выровнен по правому краю
    let display_str = &calc.display;
    let char_count = display_str.chars().count().min(12);
    let text_w = char_count * 10; // шрифт 8x8 × scale 1 (10px с пробелом)
    let text_x = disp_x + disp_w.saturating_sub(text_w + 8);
    let text_y = disp_y + (DISP_H - 16) / 2;
    draw_str_buf(buf, ww, wh, text_x, text_y, display_str, DISP_FG, 2);

    // Подпись операции
    if let Some(op) = calc.op {
        let op_str = match op {
            '+' => "+",
            '-' => "-",
            '*' => "x",
            '/' => "/",
            _ => "",
        };
        draw_str_buf(buf, ww, wh, disp_x + 4, disp_y + 4, op_str, DISP_SEC, 1);
    }

    // Кнопки
    for (i, btn) in LAYOUT.iter().enumerate() {
        let (bx, by, bw, bh) = btn_rect(btn);
        let bg = if hovered == Some(i) {
            BTN_HOV
        } else {
            btn.kind.color()
        };
        fill_buf(buf, ww, bx, by, bw, bh, bg);

        // Highlight top
        let lighter = Color {
            r: bg.r.saturating_add(40),
            g: bg.g.saturating_add(40),
            b: bg.b.saturating_add(40),
            a: 255,
        };
        fill_buf(buf, ww, bx, by, bw, 1, lighter);

        // Border
        draw_rect_outline_buf(buf, ww, bx, by, bw, bh, BTN_BRD);

        // Label — центрируем
        let label = btn.kind.label();
        let lw = label.len() * 8;
        let lx = bx + (bw.saturating_sub(lw)) / 2;
        let ly = by + (bh - 8) / 2;
        draw_str_buf(buf, ww, wh, lx, ly, label, Color::WHITE, 1);
    }

    // History Panel (right side)
    let hist_x = 5 * (BTN_W + BTN_PAD) + BTN_PAD;
    let hist_y = TITLE_BAR_H + BTN_PAD;
    let hist_w = 160 - BTN_PAD;
    let hist_h = wh - hist_y - BTN_PAD;

    fill_buf(
        buf,
        ww,
        hist_x,
        hist_y,
        hist_w,
        hist_h,
        Color::from_u32(0x272D40),
    );
    draw_rect_outline_buf(
        buf,
        ww,
        hist_x,
        hist_y,
        hist_w,
        hist_h,
        Color::from_u32(0x3A4060),
    );
    draw_str_buf(buf, ww, wh, hist_x + 8, hist_y + 8, "History", DISP_SEC, 1);
    fill_buf(
        buf,
        ww,
        hist_x + 8,
        hist_y + 20,
        50,
        1,
        Color::from_u32(0x4A5570),
    );

    let mut hy = hist_y + 28;
    for eq in calc.history.iter().rev() {
        if hy + 16 > hist_y + hist_h {
            break;
        }
        // We only render the first 18 characters so it doesn't overflow
        let short_eq = if eq.len() > 18 { &eq[0..18] } else { eq };
        draw_str_buf(buf, ww, wh, hist_x + 8, hy, short_eq, Color::WHITE, 1);
        hy += 16;
    }
}

/// Hit-test: возвращает индекс кнопки под координатами (wx, wy) в системе окна
pub fn hit_test_calc(wx: isize, wy: isize) -> Option<usize> {
    for (i, btn) in LAYOUT.iter().enumerate() {
        let (bx, by, bw, bh) = btn_rect(btn);
        if wx >= bx as isize
            && wx < (bx + bw) as isize
            && wy >= by as isize
            && wy < (by + bh) as isize
        {
            return Some(i);
        }
    }
    None
}

/// Возвращает BtnKind по индексу
pub fn btn_by_index(i: usize) -> Option<BtnKind> {
    LAYOUT.get(i).map(|b| b.kind)
}

/// Возвращает нужный размер окна для калькулятора
pub fn calc_window_size() -> (usize, usize) {
    let w = 5 * (BTN_W + BTN_PAD) + BTN_PAD + 160; // 5 columns + 160px for history
    let h = TITLE_BAR_H + BTN_PAD + DISP_H + 5 * (BTN_H + BTN_PAD) + BTN_PAD;
    (w, h)
}

// ─── Вспомогательные функции рендера ─────────────────────────────────────────

fn fill_buf(buf: &mut [Color], bw: usize, x: usize, y: usize, w: usize, h: usize, color: Color) {
    let x_end = (x + w).min(bw);
    let y_end = (y + h).min(buf.len() / bw.max(1));
    for row in y..y_end {
        for col in x..x_end {
            let idx = row * bw + col;
            if idx < buf.len() {
                buf[idx] = color;
            }
        }
    }
}

fn draw_rect_outline_buf(
    buf: &mut [Color],
    bw: usize,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    color: Color,
) {
    let bh = buf.len() / bw.max(1);
    // Top
    for col in x..(x + w).min(bw) {
        let i = y * bw + col;
        if i < buf.len() {
            buf[i] = color;
        }
    }
    // Bottom
    if y + h > 0 {
        let row = y + h - 1;
        for col in x..(x + w).min(bw) {
            let i = row * bw + col;
            if i < buf.len() {
                buf[i] = color;
            }
        }
    }
    // Left
    for row in y..(y + h).min(bh) {
        let i = row * bw + x;
        if i < buf.len() {
            buf[i] = color;
        }
    }
    // Right
    if x + w > 0 {
        let col = x + w - 1;
        for row in y..(y + h).min(bh) {
            let i = row * bw + col;
            if i < buf.len() {
                buf[i] = color;
            }
        }
    }
}

/// Рисует строку с масштабом scale (1 или 2 пикселя на глиф-пиксель)
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

// ─── Мини-парсер f64 (no_std)
// ─────────────────────────────────────────────────

fn parse_f64(s: &str) -> f64 {
    let s = s.trim();
    let (neg, s) = if s.starts_with('-') {
        (true, &s[1..])
    } else {
        (false, s)
    };
    let mut int_part: f64 = 0.0;
    let mut frac_part: f64 = 0.0;
    let mut frac_div: f64 = 1.0;
    let mut in_frac = false;

    for ch in s.chars() {
        match ch {
            '0'..='9' => {
                let d = (ch as u8 - b'0') as f64;
                if in_frac {
                    frac_div *= 10.0;
                    frac_part += d / frac_div;
                } else {
                    int_part = int_part * 10.0 + d;
                }
            },
            '.' => {
                in_frac = true;
            },
            _ => {},
        }
    }
    let v = int_part + frac_part;
    if neg {
        -v
    } else {
        v
    }
}

/// Форматирует f64 в строку без std::format (no_std)
fn format_f64(v: f64) -> String {
    if v.is_infinite() {
        return if v > 0.0 {
            String::from("Inf")
        } else {
            String::from("-Inf")
        };
    }
    if v.is_nan() {
        return String::from("Err");
    }

    let neg = v < 0.0;
    let v = if neg { -v } else { v };

    // Целая часть
    let int = v as u64;
    let frac_raw = v - int as f64;

    let mut result = String::new();
    if neg {
        result.push('-');
    }

    // Форматируем целую часть
    if int == 0 {
        result.push('0');
    } else {
        let mut digits = [0u8; 20];
        let mut n = int;
        let mut len = 0;
        while n > 0 {
            digits[len] = (n % 10) as u8;
            n /= 10;
            len += 1;
        }
        for i in (0..len).rev() {
            result.push((b'0' + digits[i]) as char);
        }
    }

    // Дробная часть (до 6 знаков, отрезаем trailing zeros)
    if frac_raw > 1e-9 {
        result.push('.');
        let mut f = frac_raw;
        let mut count = 0;
        let mut frac_str = [b'0'; 6];
        while count < 6 && f > 1e-9 {
            f *= 10.0;
            let d = f as u8;
            frac_str[count] = b'0' + d.min(9);
            f -= d as f64;
            count += 1;
        }
        // Удаляем trailing zeros
        while count > 1 && frac_str[count - 1] == b'0' {
            count -= 1;
        }
        for i in 0..count {
            result.push(frac_str[i] as char);
        }
    }

    result
}
