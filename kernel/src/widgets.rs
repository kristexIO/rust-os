use alloc::{boxed::Box, string::String, vec::Vec};

use font8x8::UnicodeFonts;

use crate::graphics::{Color, DrawTarget};

pub trait Widget {
    fn render(&mut self, target: &mut dyn DrawTarget);
    fn handle_input(&mut self, input: &str) -> bool;
    fn position(&self) -> (isize, isize);
    fn size(&self) -> (usize, usize);
}

pub struct Button {
    x: isize,
    y: isize,
    width: usize,
    height: usize,
    text: String,
    is_pressed: bool,
}

impl Button {
    pub fn new(x: isize, y: isize, width: usize, height: usize, text: String) -> Self {
        Button {
            x,
            y,
            width,
            height,
            text,
            is_pressed: false,
        }
    }

    pub fn is_clicked(&self, mouse_x: isize, mouse_y: isize) -> bool {
        mouse_x >= self.x
            && mouse_x < self.x + self.width as isize
            && mouse_y >= self.y
            && mouse_y < self.y + self.height as isize
    }

    pub fn press(&mut self) {
        self.is_pressed = true;
    }

    pub fn release(&mut self) {
        self.is_pressed = false;
    }
}

impl Widget for Button {
    fn render(&mut self, target: &mut dyn DrawTarget) {
        let top_color = if self.is_pressed {
            Color::from_u32(0x3B5FA8)
        } else {
            Color::from_u32(0x4C566A) // Nord-style gray-blue
        };
        let bottom_color = if self.is_pressed {
            Color::from_u32(0x2E4A82)
        } else {
            Color::from_u32(0x3B4252)
        };

        // Draw gradient-like button background (simple 2-color vertical)
        let mid = self.height / 2;
        target.fill_rect(self.x as usize, self.y as usize, self.width, mid, top_color);
        target.fill_rect(
            self.x as usize,
            self.y as usize + mid,
            self.width,
            self.height - mid,
            bottom_color,
        );

        // Draw rounded-ish border (skip corners)
        let border = Color::from_u32(0x2E3440);
        target.draw_hline(self.x as usize + 1, self.y as usize, self.width - 2, border);
        target.draw_hline(
            self.x as usize + 1,
            (self.y + self.height as isize - 1) as usize,
            self.width - 2,
            border,
        );
        target.draw_vline(
            self.x as usize,
            self.y as usize + 1,
            self.height - 2,
            border,
        );
        target.draw_vline(
            (self.x + self.width as isize - 1) as usize,
            self.y as usize + 1,
            self.height - 2,
            border,
        );

        // Draw border
        for i in 0..self.width {
            target.draw_pixel(i + self.x as usize, self.y as usize, Color::BLACK);
            target.draw_pixel(
                i + self.x as usize,
                (self.y + self.height as isize - 1) as usize,
                Color::BLACK,
            );
        }

        for i in 0..self.height {
            target.draw_pixel(self.x as usize, i + self.y as usize, Color::BLACK);
            target.draw_pixel(
                (self.x + self.width as isize - 1) as usize,
                i + self.y as usize,
                Color::BLACK,
            );
        }

        // Simple text rendering using font8x8
        let text_w = self.text.chars().count() * 8;
        let text_x = self.x as usize + (self.width.saturating_sub(text_w)) / 2;
        let text_y = self.y as usize + (self.height.saturating_sub(8)) / 2;

        for (i, ch) in self.text.chars().enumerate() {
            let px = text_x + i * 8;
            let glyph = font8x8::BASIC_FONTS
                .get(ch)
                .or_else(|| font8x8::LATIN_FONTS.get(ch));

            if let Some(glyph) = glyph {
                for (row, &byte) in glyph.iter().enumerate() {
                    let mut b = byte;
                    for col in 0..8usize {
                        if (b & 1) != 0 {
                            let cx = px + col;
                            let cy = text_y + row;
                            if cx < self.x as usize + self.width
                                && cy < self.y as usize + self.height
                            {
                                target.draw_pixel(cx, cy, Color::BLACK);
                            }
                        }
                        b >>= 1;
                    }
                }
            }
        }
    }

    fn handle_input(&mut self, _input: &str) -> bool {
        // Button doesn't really handle text input, just mouse clicks
        false
    }

    fn position(&self) -> (isize, isize) {
        (self.x, self.y)
    }

    fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }
}

pub struct TextBox {
    x: isize,
    y: isize,
    width: usize,
    height: usize,
    text: String,
    cursor_pos: usize,
    is_focused: bool,
}

impl TextBox {
    pub fn new(x: isize, y: isize, width: usize, height: usize) -> Self {
        TextBox {
            x,
            y,
            width,
            height,
            text: String::new(),
            cursor_pos: 0,
            is_focused: false,
        }
    }

    pub fn set_focus(&mut self, focused: bool) {
        self.is_focused = focused;
    }

    pub fn insert_text(&mut self, text: &str) {
        if self.cursor_pos <= self.text.len() {
            self.text.insert_str(self.cursor_pos, text);
            self.cursor_pos += text.len();
        }
    }

    pub fn delete_char(&mut self) {
        if self.cursor_pos > 0 && self.cursor_pos <= self.text.len() {
            self.cursor_pos -= 1;
            self.text.remove(self.cursor_pos);
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor_pos < self.text.len() {
            self.cursor_pos += 1;
        }
    }
}

impl Widget for TextBox {
    fn render(&mut self, target: &mut dyn DrawTarget) {
        let bg_color = if self.is_focused {
            Color::from_u32(0xECEFF4) // Nord snow storm
        } else {
            Color::from_u32(0xD8DEE9)
        };
        let border_color = if self.is_focused {
            Color::from_u32(0x88C0D0) // Nord frost blue
        } else {
            Color::from_u32(0x4C566A)
        };

        target.fill_rect(
            self.x as usize,
            self.y as usize,
            self.width,
            self.height,
            bg_color,
        );

        // Draw border
        target.draw_rect_outline(
            self.x as usize,
            self.y as usize,
            self.width,
            self.height,
            border_color,
        );

        // Draw border
        for i in 0..self.width {
            target.draw_pixel(i + self.x as usize, self.y as usize, Color::BLACK);
            target.draw_pixel(
                i + self.x as usize,
                (self.y + self.height as isize - 1) as usize,
                Color::BLACK,
            );
        }

        for i in 0..self.height {
            target.draw_pixel(self.x as usize, i + self.y as usize, Color::BLACK);
            target.draw_pixel(
                (self.x + self.width as isize - 1) as usize,
                i + self.y as usize,
                Color::BLACK,
            );
        }

        // Render text content
        let mut text_x = self.x as usize + 2; // 2px padding
        let text_y = self.y as usize + 2;

        for ch in self.text.chars() {
            if text_x < self.x as usize + self.width - 8 {
                let glyph = font8x8::BASIC_FONTS
                    .get(ch)
                    .or_else(|| font8x8::LATIN_FONTS.get(ch));

                if let Some(glyph) = glyph {
                    for (row, &byte) in glyph.iter().enumerate() {
                        let mut b = byte;
                        for col in 0..8usize {
                            if (b & 1) != 0 {
                                let cx = text_x + col;
                                let cy = text_y + row;
                                if cx < self.x as usize + self.width
                                    && cy < self.y as usize + self.height
                                {
                                    target.draw_pixel(cx, cy, Color::BLACK);
                                }
                            }
                            b >>= 1;
                        }
                    }
                }
                text_x += 8; // Character width
            }
        }

        // Draw cursor if focused
        if self.is_focused {
            let cursor_x = self.x as usize + 2 + (self.cursor_pos * 8);
            if cursor_x < self.x as usize + self.width - 1 {
                for i in 0..8 {
                    if (text_y + i) < (self.y as usize + self.height - 1) {
                        target.draw_pixel(cursor_x, text_y + i, Color::BLACK);
                    }
                }
            }
        }
    }

    fn handle_input(&mut self, input: &str) -> bool {
        self.insert_text(input);
        true
    }

    fn position(&self) -> (isize, isize) {
        (self.x, self.y)
    }

    fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }
}

pub struct Panel {
    x: isize,
    y: isize,
    width: usize,
    height: usize,
    children: Vec<Box<dyn Widget>>,
    visible: bool,
}

impl Panel {
    pub fn new(x: isize, y: isize, width: usize, height: usize) -> Self {
        Panel {
            x,
            y,
            width,
            height,
            children: Vec::new(),
            visible: true,
        }
    }

    pub fn add_widget(&mut self, widget: Box<dyn Widget>) {
        self.children.push(widget);
    }

    pub fn show(&mut self) {
        self.visible = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }
}

impl Widget for Panel {
    fn render(&mut self, target: &mut dyn DrawTarget) {
        if !self.visible {
            return;
        }

        // Optionally draw panel background
        // target.fill_rect(self.x as usize, self.y as usize, self.width, self.height,
        // Color::from_u32(0x));

        // Render all child widgets
        for child in &mut self.children {
            child.render(target);
        }
    }

    fn handle_input(&mut self, input: &str) -> bool {
        for child in &mut self.children {
            if child.handle_input(input) {
                return true; // Input handled by child
            }
        }
        false // Input not handled
    }

    fn position(&self) -> (isize, isize) {
        (self.x, self.y)
    }

    fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }
}

pub struct ProgressBar {
    pub x: isize,
    pub y: isize,
    pub width: usize,
    pub height: usize,
    pub progress: f32, // 0.0 to 1.0
}

impl Widget for ProgressBar {
    fn render(&mut self, target: &mut dyn DrawTarget) {
        let bg = Color::from_u32(0x3B4252);
        let fg = Color::from_u32(0xA3BE8C); // Nord green

        target.fill_rect(
            self.x as usize,
            self.y as usize,
            self.width,
            self.height,
            bg,
        );
        let prog_w = (self.width as f32 * self.progress) as usize;
        if prog_w > 0 {
            target.fill_rect(self.x as usize, self.y as usize, prog_w, self.height, fg);
        }
        target.draw_rect_outline(
            self.x as usize,
            self.y as usize,
            self.width,
            self.height,
            Color::from_u32(0x2E3440),
        );
    }
    fn handle_input(&mut self, _input: &str) -> bool {
        false
    }
    fn position(&self) -> (isize, isize) {
        (self.x, self.y)
    }
    fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }
}

pub struct Label {
    pub x: isize,
    pub y: isize,
    pub text: String,
    pub color: Color,
}

impl Widget for Label {
    fn render(&mut self, target: &mut dyn DrawTarget) {
        for (i, ch) in self.text.chars().enumerate() {
            let px = self.x as usize + i * 8;
            let glyph = font8x8::BASIC_FONTS
                .get(ch)
                .or_else(|| font8x8::LATIN_FONTS.get(ch));
            if let Some(glyph) = glyph {
                for (row, &byte) in glyph.iter().enumerate() {
                    let mut b = byte;
                    for col in 0..8usize {
                        if (b & 1) != 0 {
                            target.draw_pixel(px + col, self.y as usize + row, self.color);
                        }
                        b >>= 1;
                    }
                }
            }
        }
    }
    fn handle_input(&mut self, _input: &str) -> bool {
        false
    }
    fn position(&self) -> (isize, isize) {
        (self.x, self.y)
    }
    fn size(&self) -> (usize, usize) {
        (self.text.len() * 8, 8)
    }
}
pub struct WidgetManager {
    widgets: Vec<Box<dyn Widget>>,
}

impl WidgetManager {
    pub fn new() -> Self {
        WidgetManager {
            widgets: Vec::new(),
        }
    }

    pub fn add_widget(&mut self, widget: Box<dyn Widget>) {
        self.widgets.push(widget);
    }

    pub fn render_all(&mut self, target: &mut dyn DrawTarget) {
        for widget in &mut self.widgets {
            widget.render(target);
        }
    }

    pub fn handle_input(&mut self, input: &str) -> bool {
        for widget in &mut self.widgets {
            if widget.handle_input(input) {
                return true;
            }
        }
        false
    }
}
