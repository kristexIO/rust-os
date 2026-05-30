/// System Monitor — графики CPU/RAM в реальном времени
use alloc::{string::String, vec::Vec};

use font8x8::UnicodeFonts;

use crate::{
    graphics::Color,
    window::{Window, TITLE_BAR_H},
};

#[derive(Clone)]
pub struct SystemMonitorState {
    pub cpu_history: Vec<u32>,
    pub ram_history: Vec<u32>,
    pub max_history: usize,
    pub last_update_tick: u64,
}

impl SystemMonitorState {
    pub fn new() -> Self {
        Self {
            cpu_history: Vec::new(),
            ram_history: Vec::new(),
            max_history: 100,
            last_update_tick: 0,
        }
    }

    pub fn update(&mut self, current_tick: u64) {
        if current_tick.saturating_sub(self.last_update_tick) < 18 {
            return;
        }
        self.last_update_tick = current_tick;

        // Фейковые данные CPU (в реальности нужно читать из scheduler)
        let cpu_usage = ((current_tick * 13) % 100) as u32;
        self.cpu_history.push(cpu_usage);
        if self.cpu_history.len() > self.max_history {
            self.cpu_history.remove(0);
        }

        // RAM usage
        let alloc = crate::frame_allocator::get_frame_allocator();
        let total = alloc.total_frames() * crate::frame_allocator::FRAME_SIZE as usize;
        let free = alloc.free_memory_bytes() as usize;
        let ram_usage = if total > 0 {
            ((total - free) * 100) / total
        } else {
            0
        };
        self.ram_history.push(ram_usage as u32);
        if self.ram_history.len() > self.max_history {
            self.ram_history.remove(0);
        }
    }
}

pub fn render_system_monitor(win: &mut Window, state: &SystemMonitorState) {
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

    // Заголовок
    draw_str_buf(
        buf,
        bw,
        bh,
        10,
        TITLE_BAR_H + 8,
        "SYSTEM MONITOR",
        Color::from_u32(0x7AA2F7),
        1,
    );

    // CPU Graph
    draw_str_buf(
        buf,
        bw,
        bh,
        10,
        TITLE_BAR_H + 28,
        "CPU Usage",
        Color::from_u32(0x9ECE6A),
        1,
    );
    draw_graph(
        buf,
        bw,
        bh,
        10,
        TITLE_BAR_H + 44,
        bw - 20,
        80,
        &state.cpu_history,
        Color::from_u32(0x9ECE6A),
    );

    // RAM Graph
    let ram_y = TITLE_BAR_H + 140;
    draw_str_buf(
        buf,
        bw,
        bh,
        10,
        ram_y,
        "Memory Usage",
        Color::from_u32(0x7AA2F7),
        1,
    );
    draw_graph(
        buf,
        bw,
        bh,
        10,
        ram_y + 16,
        bw - 20,
        80,
        &state.ram_history,
        Color::from_u32(0x7AA2F7),
    );

    // Статистика
    let stats_y = ram_y + 112;
    if let Some(cpu) = state.cpu_history.last() {
        let mut cpu_str = String::from("CPU: ");
        push_u64(&mut cpu_str, *cpu as u64);
        cpu_str.push_str("%");
        draw_str_buf(
            buf,
            bw,
            bh,
            10,
            stats_y,
            &cpu_str,
            Color::from_u32(0x9ECE6A),
            1,
        );
    }
    if let Some(ram) = state.ram_history.last() {
        let mut ram_str = String::from("RAM: ");
        push_u64(&mut ram_str, *ram as u64);
        ram_str.push_str("%");
        draw_str_buf(
            buf,
            bw,
            bh,
            10,
            stats_y + 16,
            &ram_str,
            Color::from_u32(0x7AA2F7),
            1,
        );
    }

    let (total, free) = memory_stats_mib();
    let mut mem_str = String::from("Memory: ");
    push_u64(&mut mem_str, (total - free) as u64);
    mem_str.push_str(" / ");
    push_u64(&mut mem_str, total as u64);
    mem_str.push_str(" MiB");
    draw_str_buf(
        buf,
        bw,
        bh,
        10,
        stats_y + 32,
        &mem_str,
        Color::from_u32(0xABB2BF),
        1,
    );

    let uptime = crate::gui::CLOCK_TICKS.load(core::sync::atomic::Ordering::Relaxed) / 18;
    let mut up_str = String::from("Uptime: ");
    push_u64(&mut up_str, uptime);
    up_str.push_str("s");
    draw_str_buf(
        buf,
        bw,
        bh,
        10,
        stats_y + 48,
        &up_str,
        Color::from_u32(0xBB9AF7),
        1,
    );
}

fn draw_graph(
    buf: &mut [Color],
    bw: usize,
    bh: usize,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    data: &[u32],
    color: Color,
) {
    // Фон графика
    fill_buf(buf, bw, x, y, w, h, Color::from_u32(0x16161E));
    // Рамка
    draw_rect_outline_buf(buf, bw, x, y, w, h, Color::from_u32(0x292E42));

    if data.len() < 2 {
        return;
    }

    let bar_w = w / data.len().max(1);

    for (i, &val) in data.iter().enumerate() {
        let bar_h = (val as usize * h) / 100;
        let bar_x = x + i * bar_w;
        let bar_y = y + h - bar_h;

        // Градиент
        for row in 0..bar_h {
            let intensity = (row * 255) / bar_h.max(1);
            let intensity = intensity as u32;
            let c = Color {
                r: (color.r as u32 * intensity / 255) as u8,
                g: (color.g as u32 * intensity / 255) as u8,
                b: (color.b as u32 * intensity / 255) as u8,
                a: 255,
            };
            let py = bar_y + row;
            if py < bh {
                for col in 0..bar_w.max(1) {
                    let px = bar_x + col;
                    if px < bw {
                        let idx = py * bw + px;
                        if idx < buf.len() {
                            buf[idx] = c;
                        }
                    }
                }
            }
        }
    }
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
    for col in x..(x + w).min(bw) {
        if y * bw + col < buf.len() {
            buf[y * bw + col] = color;
        }
        if y + h > 0 && (y + h - 1) * bw + col < buf.len() {
            buf[(y + h - 1) * bw + col] = color;
        }
    }
    for row in y..(y + h).min(bh) {
        if row * bw + x < buf.len() {
            buf[row * bw + x] = color;
        }
        if x + w > 0 && row * bw + x + w - 1 < buf.len() {
            buf[row * bw + x + w - 1] = color;
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

pub fn memory_stats_mib() -> (usize, usize) {
    let alloc = crate::frame_allocator::get_frame_allocator();
    let total_mib =
        alloc.total_frames() * (crate::frame_allocator::FRAME_SIZE as usize) / (1024 * 1024);
    let free_mib = (alloc.free_memory_bytes() / (1024 * 1024)) as usize;
    (total_mib, free_mib)
}
