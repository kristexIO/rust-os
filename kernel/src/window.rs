// window.rs — Window with XFCE-style title bar, drag, close, minimize.

use alloc::{string::String, vec::Vec};
use core::sync::atomic::{AtomicUsize, Ordering};

use font8x8::UnicodeFonts;

use crate::graphics::{Color, DrawTarget};

static NEXT_WINDOW_ID: AtomicUsize = AtomicUsize::new(1);

/// Height of the title bar in pixels
pub const TITLE_BAR_H: usize = 22;

const CLOSE_BTN_BG: Color = Color::from_u32(0xCC3333);
const CLOSE_BTN_HOVER: Color = Color::from_u32(0xFF5555);
const MIN_BTN_BG: Color = Color::from_u32(0x3399CC);
const MAX_BTN_BG: Color = Color::from_u32(0x33CC33);

#[derive(Clone, Copy)]
pub enum AnimationType {
    Open,
    Close,
    Minimize,
    Restore,
}

const ANIMATION_DURATION: u64 = 4; // ~220ms (at 18.2 ticks/sec)

#[derive(Clone, Copy, PartialEq)]
pub enum ResizeDir {
    Right,
    Bottom,
    BottomRight,
}

pub struct Window {
    pub id: usize,
    pub x: isize,
    pub y: isize,
    width: usize,
    height: usize,
    pub title: String,
    pub buffer: Vec<Color>,
    pub focused: bool,
    pub dragging: bool,
    pub resizing: bool,
    pub resize_dir: Option<ResizeDir>,
    pub resize_start_w: usize,
    pub resize_start_h: usize,
    pub drag_offset_x: isize,
    pub drag_offset_y: isize,
    pub closed: bool,
    pub minimized: bool,
    pub maximized: bool,
    pub saved_x: isize,
    pub saved_y: isize,
    pub saved_width: usize,
    pub saved_height: usize,
    pub desktop_id: usize,
    pub animation_type: Option<AnimationType>,
    pub animation_progress: f32,
    pub animation_start_time: u64,
}

impl Window {
    pub fn update_animation(&mut self, current_time: u64) {
        if let Some(anim) = self.animation_type {
            let elapsed = current_time.saturating_sub(self.animation_start_time);
            let progress = (elapsed as f32 / ANIMATION_DURATION as f32).min(1.0);
            self.animation_progress = match anim {
                AnimationType::Open => progress,
                AnimationType::Close => 1.0 - progress,
                AnimationType::Minimize => 1.0 - progress,
                AnimationType::Restore => progress,
            };
            if progress >= 1.0 {
                self.animation_type = None;
                match anim {
                    AnimationType::Close => self.closed = true,
                    AnimationType::Minimize => self.minimized = true,
                    _ => {},
                }
            }
        }
    }

    pub fn is_animating(&self) -> bool {
        self.animation_type.is_some()
    }

    pub fn start_animation(&mut self, anim: AnimationType, current_time: u64) {
        self.animation_type = Some(anim);
        self.animation_start_time = current_time;
    }

    pub fn new(x: isize, y: isize, width: usize, height: usize, title: String) -> Self {
        let mut win = Window {
            id: NEXT_WINDOW_ID.fetch_add(1, Ordering::Relaxed),
            x,
            y,
            width,
            height,
            title,
            buffer: alloc::vec![crate::theme::get_theme_colors().win_bg; width * height],
            focused: true,
            dragging: false,
            resizing: false,
            resize_dir: None,
            resize_start_w: 0,
            resize_start_h: 0,
            drag_offset_x: 0,
            drag_offset_y: 0,
            closed: false,
            minimized: false,
            maximized: false,
            saved_x: x,
            saved_y: y,
            saved_width: width,
            saved_height: height,
            desktop_id: 0,
            animation_type: Some(AnimationType::Open),
            animation_progress: 0.0,
            animation_start_time: crate::gui::CLOCK_TICKS
                .load(core::sync::atomic::Ordering::Relaxed),
        };

        win.redraw_title_bar();
        win
    }

    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
    pub fn buffer(&self) -> &[Color] {
        &self.buffer
    }

    /// Redraws the title bar. Call after focus changes.
    pub fn redraw_title_bar(&mut self) {
        let w = self.width;
        let h = TITLE_BAR_H;
        let theme_colors = crate::theme::get_theme_colors();

        // Gradient fill: top color to bottom color
        let (top_col, bot_col) = if self.focused {
            (theme_colors.title_bg_active, theme_colors.title_grad_bot)
        } else {
            (theme_colors.title_bg_inactive, Color::from_u32(0x2E3440))
        };

        for row in 0..h {
            // Linear interpolation per row
            let t = row * 255 / h.max(1);
            let blend = |a: u8, b: u8| -> u8 {
                let a = a as u32;
                let b = b as u32;
                ((a * (255 - t as u32) + b * t as u32) / 255) as u8
            };
            let c = Color {
                r: blend(top_col.r, bot_col.r),
                g: blend(top_col.g, bot_col.g),
                b: blend(top_col.b, bot_col.b),
                a: 255,
            };
            for col in 0..w {
                self.buffer[row * w + col] = c;
            }
        }

        // Highlight: 1-pixel bright line at top
        let highlight = if self.focused {
            Color::from_u32(0x5C89C1)
        } else {
            Color::from_u32(0x555566)
        };
        for col in 0..w {
            self.buffer[col] = highlight;
        }

        // Bottom border of title bar (separator)
        let sep = if self.focused {
            Color::from_u32(0x1A4D78)
        } else {
            Color::from_u32(0x333345)
        };
        for col in 0..w {
            self.buffer[(h - 1) * w + col] = sep;
        }

        // Close button: X
        self.draw_title_button(w, 1, CLOSE_BTN_BG);
        self.draw_button_icon(w, 1, 'x');

        // Maximize button: ◻ or #
        self.draw_title_button(w, 2, MAX_BTN_BG);
        // Simple rectangle icon
        self.draw_button_icon(w, 2, '#');

        // Minimize button: _
        self.draw_title_button(w, 3, MIN_BTN_BG);
        self.draw_button_icon(w, 3, '_');

        // Window title text (font8x8)
        self.draw_title_text(theme_colors.title_fg);

        // Outer window border (first pixel row/col of entire window)
        let border = if self.focused {
            theme_colors.win_border_active
        } else {
            theme_colors.win_border_inactive
        };
        // Top row
        for col in 0..w {
            self.buffer[col] = border;
        }
        // Left and right edges of the title bar
        for row in 0..h {
            self.buffer[row * w] = border;
            if w > 0 {
                self.buffer[row * w + w - 1] = border;
            }
        }
    }

    fn draw_title_button(&mut self, w: usize, pos: usize, base_color: Color) {
        let btn_size = TITLE_BAR_H - 6;
        let btn_spacing = 4usize;
        let btn_x = w.saturating_sub((btn_size + btn_spacing) * pos);
        let btn_y = 3usize;

        let top_c = Color::from_rgb(
            base_color.r.saturating_add(40),
            base_color.g.saturating_add(40),
            base_color.b.saturating_add(40),
        );

        for row in btn_y..btn_y + btn_size {
            let c = if row < btn_y + btn_size / 2 {
                top_c
            } else {
                base_color
            };
            for col in btn_x..btn_x + btn_size {
                if col < w {
                    self.buffer[row * w + col] = c;
                }
            }
        }
    }

    fn draw_button_icon(&mut self, w: usize, pos: usize, icon: char) {
        let btn_size = TITLE_BAR_H - 6;
        let btn_spacing = 4usize;
        let btn_x = w.saturating_sub((btn_size + btn_spacing) * pos);
        let btn_y = 3usize;

        if icon == 'x' {
            let xs = btn_size.saturating_sub(4);
            for i in 0..xs {
                let r = btn_y + 2 + i;
                let c1 = btn_x + 2 + i;
                let c2 = btn_x + btn_size - 3 - i;
                if r < self.height && c1 < w {
                    self.buffer[r * w + c1] = crate::theme::get_theme_colors().title_fg;
                }
                if r < self.height && c2 < w {
                    self.buffer[r * w + c2] = crate::theme::get_theme_colors().title_fg;
                }
            }
        } else if icon == '#' {
            for row in btn_y + 3..btn_y + btn_size - 3 {
                for col in btn_x + 3..btn_x + btn_size - 3 {
                    if row == btn_y + 3
                        || row == btn_y + btn_size - 4
                        || col == btn_x + 3
                        || col == btn_x + btn_size - 4
                    {
                        if col < w {
                            self.buffer[row * w + col] = crate::theme::get_theme_colors().title_fg;
                        }
                    }
                }
            }
        } else if icon == '_' {
            let ry = btn_y + btn_size - 4;
            for col in btn_x + 3..btn_x + btn_size - 3 {
                if col < w {
                    self.buffer[ry * w + col] = crate::theme::get_theme_colors().title_fg;
                }
            }
        }
    }

    fn draw_title_text(&mut self, text_color: Color) {
        let w = self.width;
        let btn_area = TITLE_BAR_H + 4; // skip btn
        let max_chars = (w.saturating_sub(btn_area + 6)) / 8;
        let text: alloc::vec::Vec<char> = self.title.chars().take(max_chars).collect();
        let text_y = (TITLE_BAR_H.saturating_sub(8)) / 2;
        let text_x = 6usize;

        for (i, &ch) in text.iter().enumerate() {
            let px = text_x + i * 8;
            let py = text_y;
            self.draw_char_to_buf(w, px, py, ch, text_color);
        }
    }

    fn draw_char_to_buf(&mut self, buf_w: usize, px: usize, py: usize, ch: char, fg: Color) {
        let glyph = font8x8::BASIC_FONTS
            .get(ch)
            .or_else(|| font8x8::LATIN_FONTS.get(ch));
        if let Some(glyph) = glyph {
            for (row, &byte) in glyph.iter().enumerate() {
                let mut b = byte;
                for col in 0..8usize {
                    let x = px + col;
                    let y = py + row;
                    if y < buf_w && x < buf_w {
                        let idx = y * buf_w + x;
                        if idx < self.buffer.len() && (b & 1) != 0 {
                            self.buffer[idx] = fg;
                        }
                    }
                    b >>= 1;
                }
            }
        }
    }

    /// Resizes the window buffer. Useful for maximization.
    pub fn resize(&mut self, new_w: usize, new_h: usize) {
        if new_w == self.width && new_h == self.height {
            return;
        }
        let old_w = self.width;
        let old_h = self.height;
        self.width = new_w;
        self.height = new_h;
        // WIN_BG color inline
        let win_bg = crate::graphics::Color::from_u32(0x1e2233);
        let mut new_buf = alloc::vec![win_bg; new_w * new_h];
        let copy_w = old_w.min(new_w);
        let copy_h = old_h.min(new_h);
        for y in 0..copy_h {
            let src_start = y * old_w;
            let dst_start = y * new_w;
            new_buf[dst_start..dst_start + copy_w]
                .copy_from_slice(&self.buffer[src_start..src_start + copy_w]);
        }
        self.buffer = new_buf;
        self.redraw_title_bar();
    }

    pub fn toggle_maximize(&mut self) {
        if self.maximized {
            // Restore
            self.maximized = false;
            self.x = self.saved_x;
            self.y = self.saved_y;
            let (sw, sh) = (self.saved_width, self.saved_height);
            self.resize(sw, sh);
        } else {
            // Maximize to 800x600 (default resolution)
            self.maximized = true;
            self.saved_x = self.x;
            self.saved_y = self.y;
            self.saved_width = self.width;
            self.saved_height = self.height;
            self.x = 0;
            self.y = 0;
            self.resize(800, 600);
        }
    }

    // ── Hit-testing ─────────────────────────────────────────────────────────

    pub fn hit_test_resize(&self, mx: isize, my: isize) -> Option<ResizeDir> {
        let border = 8;
        let right_edge =
            mx >= self.x + self.width as isize - border && mx < self.x + self.width as isize;
        let bottom_edge =
            my >= self.y + self.height as isize - border && my < self.y + self.height as isize;

        if right_edge && bottom_edge {
            Some(ResizeDir::BottomRight)
        } else if right_edge {
            Some(ResizeDir::Right)
        } else if bottom_edge {
            Some(ResizeDir::Bottom)
        } else {
            None
        }
    }

    pub fn hit_test_titlebar(&self, mx: isize, my: isize) -> bool {
        mx >= self.x
            && mx < self.x + self.width as isize
            && my >= self.y
            && my < self.y + TITLE_BAR_H as isize
    }

    pub fn hit_test_close(&self, mx: isize, my: isize) -> bool {
        self.hit_test_title_button(mx, my, 1)
    }

    pub fn hit_test_maximize(&self, mx: isize, my: isize) -> bool {
        self.hit_test_title_button(mx, my, 2)
    }

    pub fn hit_test_minimize(&self, mx: isize, my: isize) -> bool {
        self.hit_test_title_button(mx, my, 3)
    }

    fn hit_test_title_button(&self, mx: isize, my: isize, pos: usize) -> bool {
        let btn_size = TITLE_BAR_H - 6;
        let btn_spacing = 4usize;
        let btn_x = self.x + self.width as isize - ((btn_size + btn_spacing) * pos) as isize;
        mx >= btn_x
            && mx < btn_x + btn_size as isize
            && my >= self.y + 3
            && my < self.y + 3 + btn_size as isize
    }

    pub fn hit_test(&self, mx: isize, my: isize) -> bool {
        mx >= self.x
            && mx < self.x + self.width as isize
            && my >= self.y
            && my < self.y + self.height as isize
    }

    pub fn client_height(&self) -> usize {
        self.height.saturating_sub(TITLE_BAR_H)
    }

    /// Scroll client area up by `lines` pixels.
    pub fn scroll_up(&mut self, lines: usize) {
        let w = self.width;
        let client_start = TITLE_BAR_H;
        let client_end = self.height;

        if lines == 0 || client_start >= client_end {
            return;
        }

        let scroll_pixels = lines.min(client_end - client_start);
        let src_start = (client_start + scroll_pixels) * w;
        let dst_start = client_start * w;
        let copy_len = (client_end - client_start - scroll_pixels) * w;

        if src_start < self.buffer.len() && dst_start + copy_len <= self.buffer.len() {
            self.buffer
                .copy_within(src_start..src_start + copy_len, dst_start);
            let clear_start = (client_end - scroll_pixels) * w;
            let clear_end = client_end * w;
            let theme_bg = crate::theme::get_theme_colors().win_bg;
            if clear_start < self.buffer.len() && clear_end <= self.buffer.len() {
                for i in clear_start..clear_end {
                    self.buffer[i] = theme_bg;
                }
            }
        }
    }
}

impl DrawTarget for Window {
    fn width(&self) -> usize {
        self.width
    }
    fn height(&self) -> usize {
        self.height
    }

    fn draw_pixel(&mut self, x: usize, y: usize, color: Color) {
        let real_y = y + TITLE_BAR_H;
        if x < self.width && real_y < self.height {
            self.buffer[real_y * self.width + x] = color;
        }
    }

    fn scroll_up(&mut self, lines: usize) {
        Window::scroll_up(self, lines);
    }
}
