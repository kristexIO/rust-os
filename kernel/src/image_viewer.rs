/// Image Viewer — просмотр BMP изображений
use alloc::{format, string::String, vec::Vec};

use font8x8::UnicodeFonts;

use crate::{
    graphics::Color,
    window::{Window, TITLE_BAR_H},
};

#[derive(Clone)]
pub struct ImageViewerState {
    pub image_data: Vec<Color>,
    pub image_w: usize,
    pub image_h: usize,
    pub file_name: String,
    pub zoom: f32,
    pub error: Option<String>,
}

impl ImageViewerState {
    pub fn new() -> Self {
        Self {
            image_data: Vec::new(),
            image_w: 0,
            image_h: 0,
            file_name: String::from("No image"),
            zoom: 1.0,
            error: None,
        }
    }

    pub fn load_bmp(&mut self, data: &[u8]) {
        if data.len() < 54 {
            self.error = Some(String::from("File too small"));
            return;
        }
        if data[0] != b'B' || data[1] != b'M' {
            self.error = Some(String::from("Not a BMP file"));
            return;
        }

        let width = u32::from_le_bytes([data[18], data[19], data[20], data[21]]) as usize;
        let height = u32::from_le_bytes([data[22], data[23], data[24], data[25]]) as usize;
        let bpp = u16::from_le_bytes([data[28], data[29]]) as u16;
        let data_offset = u32::from_le_bytes([data[10], data[11], data[12], data[13]]) as usize;

        if bpp != 24 && bpp != 32 {
            self.error = Some(String::from("Unsupported BPP"));
            return;
        }

        self.image_w = width;
        self.image_h = height;
        self.image_data.clear();
        self.image_data.reserve(width * height);

        let bytes_per_pixel = (bpp / 8) as usize;
        let row_size = ((width * bytes_per_pixel + 3) / 4) * 4;

        for y in (0..height).rev() {
            let row_start = data_offset + y * row_size;
            for x in 0..width {
                let idx = row_start + x * bytes_per_pixel;
                if idx + 2 < data.len() {
                    let b = data[idx];
                    let g = data[idx + 1];
                    let r = data[idx + 2];
                    self.image_data.push(Color {
                        r,
                        g,
                        b,
                        a: 255,
                    });
                } else {
                    self.image_data.push(Color::BLACK);
                }
            }
        }

        self.error = None;
    }

    pub fn load_test_pattern(&mut self) {
        self.image_w = 256;
        self.image_h = 256;
        self.image_data.clear();
        for y in 0..256 {
            for x in 0..256 {
                self.image_data.push(Color {
                    r: x as u8,
                    g: y as u8,
                    b: 128,
                    a: 255,
                });
            }
        }
        self.file_name = String::from("test_pattern.bmp");
        self.error = None;
    }
}

pub fn render_image_viewer(win: &mut Window, state: &ImageViewerState) {
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

    if let Some(ref err) = state.error {
        draw_str_buf(
            buf,
            bw,
            bh,
            20,
            TITLE_BAR_H + 40,
            "Error:",
            Color::from_u32(0xF7768E),
            1,
        );
        draw_str_buf(
            buf,
            bw,
            bh,
            20,
            TITLE_BAR_H + 60,
            err,
            Color::from_u32(0xABB2BF),
            1,
        );
        return;
    }

    if state.image_data.is_empty() {
        draw_str_buf(
            buf,
            bw,
            bh,
            (bw - 160) / 2,
            TITLE_BAR_H + 60,
            "No image loaded",
            Color::from_u32(0x565F89),
            1,
        );
        draw_str_buf(
            buf,
            bw,
            bh,
            (bw - 200) / 2,
            TITLE_BAR_H + 80,
            "Open a .bmp file or load test pattern",
            Color::from_u32(0x565F89),
            1,
        );
        return;
    }

    let img_area_y = TITLE_BAR_H + 30;
    let img_area_h = bh.saturating_sub(img_area_y + 20);
    let img_area_w = bw.saturating_sub(20);

    let zoom = state.zoom;
    let draw_w = (state.image_w as f32 * zoom) as usize;
    let draw_h = (state.image_h as f32 * zoom) as usize;

    let offset_x = 10 + (img_area_w.saturating_sub(draw_w)) / 2;
    let offset_y = img_area_y + (img_area_h.saturating_sub(draw_h)) / 2;

    for dy in 0..draw_h.min(img_area_h) {
        for dx in 0..draw_w.min(img_area_w) {
            let src_x = (dx as f32 / zoom) as usize;
            let src_y = (dy as f32 / zoom) as usize;
            if src_x < state.image_w && src_y < state.image_h {
                let src_idx = src_y * state.image_w + src_x;
                let dst_x = offset_x + dx;
                let dst_y = offset_y + dy;
                if dst_x < bw && dst_y < bh {
                    let dst_idx = dst_y * bw + dst_x;
                    if src_idx < state.image_data.len() && dst_idx < buf.len() {
                        buf[dst_idx] = state.image_data[src_idx];
                    }
                }
            }
        }
    }

    let mut info = String::from("File: ");
    info.push_str(&state.file_name);
    info.push_str(&format!(" | {}x{}", state.image_w, state.image_h));
    draw_str_buf(
        buf,
        bw,
        bh,
        10,
        bh - 18,
        &info,
        Color::from_u32(0x565F89),
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
