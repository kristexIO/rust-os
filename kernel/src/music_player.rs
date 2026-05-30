/// Music Player — воспроизведение через PC Speaker
use alloc::{string::String, vec, vec::Vec};

use font8x8::UnicodeFonts;

use crate::{
    graphics::Color,
    window::{Window, TITLE_BAR_H},
};

#[derive(Clone)]
pub struct MusicPlayerState {
    pub playlist: Vec<Song>,
    pub current_song: usize,
    pub is_playing: bool,
    pub volume: u8,
    pub progress: u8,
    pub last_tick: u64,
}

#[derive(Clone)]
pub struct Song {
    pub title: String,
    pub notes: Vec<(u32, u32)>,
}

impl MusicPlayerState {
    pub fn new() -> Self {
        let mut playlist = Vec::new();

        // Twinkle Twinkle Little Star
        playlist.push(Song {
            title: String::from("Twinkle Twinkle"),
            notes: vec![
                (523, 4),
                (523, 4),
                (784, 4),
                (784, 4),
                (880, 4),
                (880, 4),
                (784, 8),
                (698, 4),
                (698, 4),
                (659, 4),
                (659, 4),
                (587, 4),
                (587, 4),
                (523, 8),
            ],
        });

        // Ode to Joy
        playlist.push(Song {
            title: String::from("Ode to Joy"),
            notes: vec![
                (659, 4),
                (659, 4),
                (698, 4),
                (784, 4),
                (784, 4),
                (698, 4),
                (659, 4),
                (587, 4),
                (523, 4),
                (523, 4),
                (587, 4),
                (659, 4),
                (659, 6),
                (587, 4),
                (587, 8),
            ],
        });

        // Mary Had a Little Lamb
        playlist.push(Song {
            title: String::from("Mary's Lamb"),
            notes: vec![
                (659, 4),
                (587, 4),
                (523, 4),
                (587, 4),
                (659, 4),
                (659, 4),
                (659, 8),
                (587, 4),
                (587, 4),
                (587, 8),
                (659, 4),
                (784, 4),
                (784, 8),
            ],
        });

        Self {
            playlist,
            current_song: 0,
            is_playing: false,
            volume: 80,
            progress: 0,
            last_tick: 0,
        }
    }

    pub fn toggle_play(&mut self) {
        self.is_playing = !self.is_playing;
    }

    pub fn next_song(&mut self) {
        if self.playlist.len() > 0 {
            self.current_song = (self.current_song + 1) % self.playlist.len();
            self.progress = 0;
        }
    }

    pub fn prev_song(&mut self) {
        if self.playlist.len() > 0 {
            self.current_song = if self.current_song == 0 {
                self.playlist.len() - 1
            } else {
                self.current_song - 1
            };
            self.progress = 0;
        }
    }

    pub fn update(&mut self, current_tick: u64) {
        if !self.is_playing || self.playlist.is_empty() {
            return;
        }
        if current_tick.saturating_sub(self.last_tick) < 4 {
            return;
        }
        self.last_tick = current_tick;

        let song = &self.playlist[self.current_song];
        let total_notes = song.notes.len();
        self.progress = ((self.last_tick * 100 / 60) % 100) as u8;

        // Воспроизводим текущую ноту
        let note_idx = (self.last_tick as usize / 4) % total_notes;
        let (freq, _duration) = song.notes[note_idx];
        unsafe {
            crate::pcspeaker::PcSpeaker::play_sound(freq);
        }
    }
}

pub fn render_music_player(win: &mut Window, state: &MusicPlayerState) {
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

    // Album art area (визуализатор)
    let viz_x = 20;
    let viz_y = TITLE_BAR_H + 20;
    let viz_w = bw - 40;
    let viz_h = 100;

    fill_buf(
        buf,
        bw,
        viz_x,
        viz_y,
        viz_w,
        viz_h,
        Color::from_u32(0x16161E),
    );
    draw_rect_outline_buf(
        buf,
        bw,
        viz_x,
        viz_y,
        viz_w,
        viz_h,
        Color::from_u32(0x3D59A1),
    );

    // Визуализатор
    if state.is_playing {
        for i in 0..viz_w {
            let bar_h = ((i as u64
                * crate::gui::CLOCK_TICKS.load(core::sync::atomic::Ordering::Relaxed)
                * 7)
                % 60) as usize;
            let c = if bar_h > 40 {
                Color::from_u32(0xF7768E)
            } else if bar_h > 20 {
                Color::from_u32(0xE0AF68)
            } else {
                Color::from_u32(0x9ECE6A)
            };
            for row in 0..bar_h {
                let py = viz_y + viz_h - 1 - row;
                if py < bh && viz_x + i < bw {
                    buf[py * bw + viz_x + i] = c;
                }
            }
        }
    } else {
        draw_str_buf(
            buf,
            bw,
            bh,
            viz_x + 40,
            viz_y + 40,
            "Press Play to start",
            Color::from_u32(0x565F89),
            1,
        );
    }

    // Song info
    let info_y = viz_y + viz_h + 16;
    if !state.playlist.is_empty() {
        let song = &state.playlist[state.current_song];
        draw_str_buf(
            buf,
            bw,
            bh,
            20,
            info_y,
            &song.title,
            Color::from_u32(0x7AA2F7),
            1,
        );

        let mut idx = String::from("Track ");
        push_u64(&mut idx, (state.current_song + 1) as u64);
        idx.push_str("/");
        push_u64(&mut idx, state.playlist.len() as u64);
        draw_str_buf(
            buf,
            bw,
            bh,
            20,
            info_y + 16,
            &idx,
            Color::from_u32(0x565F89),
            1,
        );
    }

    // Progress bar
    let prog_y = info_y + 36;
    fill_buf(buf, bw, 20, prog_y, bw - 40, 8, Color::from_u32(0x292E42));
    let prog_w = ((bw - 40) * state.progress as usize) / 100;
    fill_buf(buf, bw, 20, prog_y, prog_w, 8, Color::from_u32(0x7AA2F7));

    // Controls
    let ctrl_y = prog_y + 24;
    draw_str_buf(
        buf,
        bw,
        bh,
        (bw - 120) / 2,
        ctrl_y,
        "[<<] [Play] [>>]",
        Color::from_u32(0x9ECE6A),
        1,
    );

    // Volume
    let vol_y = ctrl_y + 20;
    let mut vol = String::from("Volume: ");
    push_u64(&mut vol, state.volume as u64);
    vol.push('%');
    draw_str_buf(buf, bw, bh, 20, vol_y, &vol, Color::from_u32(0xABB2BF), 1);

    // Playlist
    let pl_y = vol_y + 20;
    draw_str_buf(
        buf,
        bw,
        bh,
        20,
        pl_y,
        "Playlist:",
        Color::from_u32(0xBB9AF7),
        1,
    );
    for (i, song) in state.playlist.iter().enumerate() {
        let color = if i == state.current_song {
            Color::from_u32(0x7AA2F7)
        } else {
            Color::from_u32(0x565F89)
        };
        let mut line = String::from("  ");
        if i == state.current_song {
            line.push_str(">> ");
        }
        line.push_str(&song.title);
        draw_str_buf(
            buf,
            bw,
            bh,
            20,
            pl_y + 16 + i as usize * 16,
            &line,
            color,
            1,
        );
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
