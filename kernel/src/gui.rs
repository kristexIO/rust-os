// gui.rs — XFCE-style Compositor: taskbar, start menu, window decorations,
// drag-to-move, close, focus, proper font rendering via font8x8.
//
// FPS ОПТИМИЗАЦИИ:
//   1. Dirty-rect система: перерисовывается только area курсора + taskbar
//      clock.
//   2. Partial flush: в VRAM копируются только изменившиеся строки
//      (dirty_y_min..dirty_y_max).
//   3. Background рисуется один раз при запуске, затем только "мазки" поверх.
//   4. Cursor save/restore: 19x12 пикселей под курсором сохраняются,
//      восстанавливаются.

use alloc::{string::String, sync::Arc, vec::Vec};
use core::sync::atomic::{AtomicU64, Ordering};

use font8x8::UnicodeFonts;
use lazy_static::lazy_static;
use spin::Mutex;

use crate::{
    graphics::{Color, Display, GLOBAL_DISPLAY},
    mouse::MousePacket,
    window::Window,
};

// ─── Dirty-rect трекер ───────────────────────────────────────────────────────

/// Прямоугольная область, которую нужно обновить.
#[derive(Clone, Copy)]
struct DirtyRect {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

impl DirtyRect {
    const fn empty() -> Self {
        DirtyRect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        }
    }
    fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }
    /// Объединяет два прямоугольника в один охватывающий
    fn union(self, other: DirtyRect) -> DirtyRect {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let x1 = self.x.min(other.x);
        let y1 = self.y.min(other.y);
        let x2 = (self.x + self.w).max(other.x + other.w);
        let y2 = (self.y + self.h).max(other.y + other.h);
        DirtyRect {
            x: x1,
            y: y1,
            w: x2 - x1,
            h: y2 - y1,
        }
    }
}

/// Счётчик тиков для часов (инкрементируется в timer interrupt)
/// Примерно 18.2 тика/сек для PIT по умолчанию.
pub static CLOCK_TICKS: AtomicU64 = AtomicU64::new(0);

/// Инкрементирует счётчик тиков. Вызывать из timer_interrupt_handler.
pub fn clock_tick() {
    CLOCK_TICKS.fetch_add(1, Ordering::Relaxed);
}

/// Возвращает (часы, минуты, секунды) на основе тиков
/// PIT по умолчанию ≈ 18.2065 Hz. Для точности используем 18 Hz.
fn get_time_hms() -> (u64, u64, u64) {
    let (_, _, _, h, m, s) = crate::cmos::read_rtc();
    (h as u64, m as u64, s as u64)
}

/// Добавляет двузначное число в String с ведущим нулём (no alloc::format!)
fn push_two_digits(s: &mut String, v: u64) {
    let d0 = (v / 10) as u8;
    let d1 = (v % 10) as u8;
    s.push((b'0' + d0) as char);
    s.push((b'0' + d1) as char);
}

// ─── Buffer font renderer (для Window.buffer) ────────────────────────────────

/// Рисует строку прямо в Vec<Color> буфер окна с поддержкой scale
fn draw_str_to_buf(
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
        if cx + 8 * scale > bw {
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

// ─── Internal font renderer ─────────────────────────────────────────────────

/// Draw an ASCII character using the font8x8 bitmap directly onto raw pixel
/// coords.
fn draw_char_f8(display: &mut Display, x: usize, y: usize, ch: char, fg: Color, bg: Option<Color>) {
    let glyph = font8x8::BASIC_FONTS
        .get(ch)
        .or_else(|| font8x8::LATIN_FONTS.get(ch));

    if let Some(glyph) = glyph {
        for (row, &byte) in glyph.iter().enumerate() {
            let mut b = byte;
            for col in 0..8usize {
                let px = x + col;
                let py = y + row;
                if (b & 1) != 0 {
                    display.draw_pixel(px, py, fg);
                } else if let Some(bg_col) = bg {
                    display.draw_pixel(px, py, bg_col);
                }
                b >>= 1;
            }
        }
    }
}

/// Draw a string left-to-right. Returns the x coordinate after last char.
fn draw_str_f8(
    display: &mut Display,
    x: usize,
    y: usize,
    s: &str,
    fg: Color,
    bg: Option<Color>,
    max_width: Option<usize>,
) -> usize {
    let mut cx = x;
    for ch in s.chars() {
        if let Some(mw) = max_width {
            if cx + 8 > x + mw {
                break;
            }
        }
        if cx + 8 > display.width() || y + 8 > display.height() {
            break;
        }
        draw_char_f8(display, cx, y, ch, fg, bg);
        cx += 8;
    }
    cx
}

// ─── Desktop icons ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DesktopIcon {
    pub x: usize,
    pub y: usize,
    pub label: String,
    pub color: Color,
}

impl DesktopIcon {
    pub fn new(x: usize, y: usize, label: &str, color: Color) -> Self {
        Self {
            x,
            y,
            label: String::from(label),
            color,
        }
    }

    pub fn render(&self, target: &mut Display) {
        // Draw icon body (48x40 area: 40px icon + 8px label below)
        let icon_size = 32usize;
        // Icon shadow (offset 2px)
        for dy in 0..icon_size {
            for dx in 0..icon_size {
                let sx = self.x + dx + 2;
                let sy = self.y + dy + 2;
                if sx < target.width() && sy < target.height() {
                    target.draw_pixel(sx, sy, Color::from_u32(0x000000));
                }
            }
        }

        // Icon body with rounded feel (corner mask)
        for dy in 0..icon_size {
            for dx in 0..icon_size {
                // Simple rounded corner: skip 2px corners
                let corner = (dx < 3 && dy < 3)
                    || (dx < 3 && dy >= icon_size - 3)
                    || (dx >= icon_size - 3 && dy < 3)
                    || (dx >= icon_size - 3 && dy >= icon_size - 3);
                if !corner {
                    let px = self.x + dx;
                    let py = self.y + dy;
                    if px < target.width() && py < target.height() {
                        // Gradient: lighter at top
                        let brightness = if dy < icon_size / 2 { 40u8 } else { 0u8 };
                        let c = Color {
                            r: self.color.r.saturating_add(brightness),
                            g: self.color.g.saturating_add(brightness),
                            b: self.color.b.saturating_add(brightness),
                            a: 255,
                        };
                        target.draw_pixel(px, py, c);
                    }
                }
            }
        }

        // Icon border (1px lighter)
        let lighter = Color {
            r: self.color.r.saturating_add(80),
            g: self.color.g.saturating_add(80),
            b: self.color.b.saturating_add(80),
            a: 255,
        };
        for dx in 2..icon_size - 2 {
            target.draw_pixel(self.x + dx, self.y, lighter);
            target.draw_pixel(self.x + dx, self.y + icon_size - 1, self.color);
        }
        for dy in 2..icon_size - 2 {
            target.draw_pixel(self.x, self.y + dy, lighter);
            target.draw_pixel(self.x + icon_size - 1, self.y + dy, self.color);
        }

        // Label below icon with dark shadow for readability
        let label_x = self.x.saturating_sub(8); // Center roughly
        let label_y = self.y + icon_size + 2;
        // Shadow
        draw_str_f8(
            target,
            label_x + 1,
            label_y + 1,
            &self.label,
            Color::BLACK,
            None,
            Some(80),
        );
        // Text
        draw_str_f8(
            target,
            label_x,
            label_y,
            &self.label,
            Color::WHITE,
            None,
            Some(80),
        );
    }

    pub fn hit_test(&self, mx: usize, my: usize) -> bool {
        mx >= self.x && mx < self.x + 48 && my >= self.y && my < self.y + 48
    }
}

// ─── TaskItem ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TaskItem {
    pub id: usize,
    pub title: String,
    pub is_active: bool,
    pub is_minimized: bool,
}

impl TaskItem {
    pub fn new(id: usize, title: String) -> Self {
        Self {
            id,
            title,
            is_active: false,
            is_minimized: false,
        }
    }
}

// ─── Taskbar ─────────────────────────────────────────────────────────────────

pub struct Taskbar {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub tasks: Vec<TaskItem>,
    pub clock_label: String,
    pub layout_label: String,
    // Derived button positions: (x, w, task_index)
    task_buttons: Vec<(usize, usize, usize)>,
    pub hovered_btn: Option<TaskbarAction>,
}

impl Taskbar {
    pub fn new(x: usize, y: usize, width: usize) -> Self {
        Self {
            x,
            y,
            width,
            height: 28,
            tasks: Vec::new(),
            clock_label: String::from("00:00"),
            layout_label: String::from("EN"),
            task_buttons: Vec::new(),
            hovered_btn: None,
        }
    }

    pub fn add_task(&mut self, task: TaskItem) {
        self.tasks.push(task);
        self.rebuild_buttons();
    }

    pub fn remove_task(&mut self, id: usize) {
        self.tasks.retain(|t| t.id != id);
        self.rebuild_buttons();
    }

    pub fn update_task_state(&mut self, id: usize, is_active: bool, is_minimized: bool) {
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
            t.is_active = is_active;
            t.is_minimized = is_minimized;
        }
    }

    pub fn update_clock(&mut self, time: String) {
        self.clock_label = time;
    }

    fn rebuild_buttons(&mut self) {
        self.task_buttons.clear();
        let btn_w: usize = 110;
        let gap: usize = 3;
        let mut cx = self.x + 114; // After start button + search button
        let right_limit = self.x + self.width.saturating_sub(90); // Before clock
        for (i, _) in self.tasks.iter().enumerate() {
            if cx + btn_w > right_limit {
                break;
            }
            self.task_buttons.push((cx, btn_w, i));
            cx += btn_w + gap;
        }
    }

    pub fn handle_click(&self, mx: usize, my: usize) -> Option<TaskbarAction> {
        if my < self.y || my >= self.y + self.height {
            return None;
        }
        // Start button: x..x+80
        if mx >= self.x && mx < self.x + 80 {
            return Some(TaskbarAction::StartMenu);
        }
        // Search button: x+80..x+110
        if mx >= self.x + 80 && mx < self.x + 110 {
            return Some(TaskbarAction::SearchClick);
        }
        // Task buttons
        for &(bx, bw, idx) in &self.task_buttons {
            if mx >= bx && mx < bx + bw {
                let id = self.tasks.get(idx).map(|t| t.id).unwrap_or(0);
                return Some(TaskbarAction::TaskClick(id));
            }
        }
        // Clock and Layout area
        let clock_x = self.x + self.width.saturating_sub(84);
        let lay_x = clock_x.saturating_sub(30);
        if mx >= clock_x {
            return Some(TaskbarAction::ClockClick);
        }
        if mx >= lay_x && mx < clock_x {
            return Some(TaskbarAction::LayoutClick);
        }
        None
    }

    pub fn handle_hover(&mut self, mx: usize, my: usize) {
        self.hovered_btn = self.handle_click(mx, my);
    }

    pub fn render(&self, target: &mut Display) {
        let theme = crate::theme::get_theme_colors();
        let bg = theme.taskbar_bg;
        let border_top = theme.taskbar_border;

        // Background
        target.draw_rect(self.x, self.y, self.width, self.height, bg);
        // Top highlight border
        target.draw_rect(self.x, self.y, self.width, 1, border_top);

        // ── Start button ─────────────────────────────────────
        let sb_x = self.x + 3;
        let sb_y = self.y + 3;
        let sb_w = 74usize;
        let sb_h = self.height - 6;

        let mut is_hovered = false;
        if let Some(TaskbarAction::StartMenu) = self.hovered_btn {
            is_hovered = true;
        }

        let sb_top = if is_hovered {
            theme.taskbar_btn_bg_active
        } else {
            theme.taskbar_btn_bg_inactive
        };
        let sb_bot = theme.taskbar_bg;
        for dy in 0..sb_h {
            let c = if dy < sb_h / 2 { sb_top } else { sb_bot };
            target.draw_rect(sb_x, sb_y + dy, sb_w, 1, c);
        }
        let sb_border = theme.taskbar_btn_border_inactive;
        target.draw_rect(sb_x, sb_y, sb_w, 1, sb_border);
        target.draw_rect(sb_x, sb_y + sb_h - 1, sb_w, 1, sb_border);
        target.draw_rect(sb_x, sb_y, 1, sb_h, sb_border);
        target.draw_rect(sb_x + sb_w - 1, sb_y, 1, sb_h, sb_border);
        let label_x = sb_x + (sb_w - 32) / 2;
        let label_y = sb_y + (sb_h.saturating_sub(8)) / 2;
        draw_str_f8(
            target,
            label_x,
            label_y,
            "XFCE",
            theme.text_main,
            None,
            None,
        );

        // ── Search button ────────────────────────────────────
        let src_x = self.x + 80;
        let src_y = self.y + 3;
        let src_w = 26usize;
        let src_h = self.height - 6;
        for dy in 0..src_h {
            let c = if dy < src_h / 2 { sb_top } else { sb_bot };
            target.draw_rect(src_x, src_y + dy, src_w, 1, c);
        }
        target.draw_rect(src_x, src_y, src_w, 1, sb_border);
        target.draw_rect(src_x, src_y + src_h - 1, src_w, 1, sb_border);
        target.draw_rect(src_x, src_y, 1, src_h, sb_border);
        target.draw_rect(src_x + src_w - 1, src_y, 1, src_h, sb_border);
        let src_lbl_x = src_x + (src_w - 24) / 2;
        let src_lbl_y = src_y + (src_h.saturating_sub(8)) / 2;
        draw_str_f8(
            target,
            src_lbl_x,
            src_lbl_y,
            "[S]",
            theme.start_menu_accent,
            None,
            None,
        );

        // ── Task buttons ─────────────────────────────────────
        for &(bx, bw, idx) in &self.task_buttons {
            if let Some(task) = self.tasks.get(idx) {
                let mut is_hovered = false;
                if let Some(TaskbarAction::TaskClick(id)) = self.hovered_btn {
                    if id == task.id {
                        is_hovered = true;
                    }
                }

                let (btn_bg, btn_border, text_col) = if task.is_active {
                    (
                        theme.taskbar_btn_bg_active,
                        theme.taskbar_btn_border_active,
                        theme.text_main,
                    )
                } else if is_hovered {
                    (
                        theme.taskbar_btn_bg_active, // highlight background
                        theme.taskbar_btn_border_active,
                        theme.text_main,
                    )
                } else {
                    (
                        theme.taskbar_btn_bg_inactive,
                        theme.taskbar_btn_border_inactive,
                        theme.text_dim,
                    )
                };

                let by = self.y + 3;
                let bh = self.height - 6;
                target.draw_rect(bx, by, bw, bh, btn_bg);
                target.draw_rect(bx, by, bw, 1, btn_border);
                target.draw_rect(bx, by + bh - 1, bw, 1, theme.taskbar_btn_border_inactive);
                target.draw_rect(bx, by, 1, bh, btn_border);
                target.draw_rect(bx + bw - 1, by, 1, bh, theme.taskbar_btn_border_inactive);

                let ty = by + (bh - 8) / 2;
                draw_str_f8(
                    target,
                    bx + 4,
                    ty,
                    &task.title,
                    text_col,
                    None,
                    Some(bw - 8),
                );
            }
        }

        // ── Clock & Layout area ───────────────────────────────────────
        let clk_x = self.x + self.width.saturating_sub(82);
        let lay_x = clk_x.saturating_sub(30);
        let clk_y = self.y + 3;
        let clk_w = 80usize;
        let lay_w = 26usize;
        let clk_h = self.height - 6;
        let clk_bg = theme.taskbar_btn_bg_inactive;
        let clk_border = theme.taskbar_btn_border_inactive;

        // Layout bg
        target.draw_rect(lay_x, clk_y, lay_w, clk_h, clk_bg);
        target.draw_rect(lay_x, clk_y, lay_w, 1, clk_border);
        target.draw_rect(lay_x, clk_y, 1, clk_h, clk_border);
        let ltx = lay_x + 5;
        let lty = clk_y + (clk_h.saturating_sub(8)) / 2;
        draw_str_f8(
            target,
            ltx,
            lty,
            &self.layout_label,
            theme.text_main,
            None,
            None,
        );

        // Clock bg
        target.draw_rect(clk_x, clk_y, clk_w, clk_h, clk_bg);
        target.draw_rect(clk_x, clk_y, clk_w, 1, clk_border);
        target.draw_rect(clk_x, clk_y, 1, clk_h, clk_border);
        // Center the time text
        let chars = self.clock_label.chars().count();
        let text_w = chars * 8;
        let tx = clk_x + (clk_w.saturating_sub(text_w)) / 2;
        let ty = clk_y + (clk_h.saturating_sub(8)) / 2;
        draw_str_f8(
            target,
            tx,
            ty,
            &self.clock_label,
            theme.text_main,
            None,
            None,
        );

        // ── Separator lines ──────────────────────────────────
        target.draw_rect(
            self.x + 111,
            self.y + 4,
            1,
            self.height - 8,
            theme.taskbar_border,
        );
        let sep_x = lay_x.saturating_sub(3);
        target.draw_rect(sep_x, self.y + 4, 1, self.height - 8, theme.taskbar_border);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskbarAction {
    StartMenu,
    SearchClick,
    TaskClick(usize),
    ClockClick,
    LayoutClick,
}

// ─── IconType / MenuEntry
// ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum IconType {
    File,
    Folder,
    Application,
    Terminal,
    Settings,
    Power,
    Trash,
    NetworkDrive,
}

#[derive(Debug, Clone)]
pub struct MenuEntry {
    pub name: String,
    pub icon_type: IconType,
    pub selected: bool,
}

impl MenuEntry {
    pub fn new(name: &str, icon_type: IconType) -> Self {
        Self {
            name: String::from(name),
            icon_type,
            selected: false,
        }
    }

    pub fn render(&self, target: &mut Display, x: usize, y: usize, width: usize) {
        let theme = crate::theme::get_theme_colors();
        let item_h = 24usize;
        let bg = if self.selected {
            theme.start_menu_accent
        } else {
            theme.start_menu_bg
        };

        // Background fill
        target.draw_rect(x, y, width, item_h, bg);

        // Hover border accent
        if self.selected {
            target.draw_rect(x, y, 2, item_h, theme.text_main);
        }

        // Icon (12x12 colored square at x+4, centered vertically)
        let icon_color = match self.icon_type {
            IconType::Folder => Color::from_u32(0xF9E2AF),
            IconType::Application => Color::from_u32(0x89B4FA),
            IconType::Terminal => Color::from_u32(0xA6E3A1),
            IconType::Settings => Color::from_u32(0xCBA6F7),
            IconType::Power => Color::from_u32(0xF38BA8),
            IconType::File => Color::from_u32(0xBAC2DE),
            IconType::Trash => Color::from_u32(0xA6ADC8),
            IconType::NetworkDrive => Color::from_u32(0x74C7EC),
        };

        let icon_x = x + 5;
        let icon_y = y + (item_h - 12) / 2;
        // Simple icon: filled rounded square
        for dy in 1..11usize {
            for dx in 1..11usize {
                if icon_x + dx < target.width() && icon_y + dy < target.height() {
                    // Slight gradient
                    let brightness: u8 = if dy < 5 { 30 } else { 0 };
                    let c = Color {
                        r: icon_color.r.saturating_add(brightness),
                        g: icon_color.g.saturating_add(brightness),
                        b: icon_color.b.saturating_add(brightness),
                        a: 255,
                    };
                    target.draw_pixel(icon_x + dx, icon_y + dy, c);
                }
            }
        }
        // Icon border
        for dx in 0..12usize {
            target.draw_pixel(icon_x + dx, icon_y, theme.taskbar_btn_bg_inactive);
            target.draw_pixel(icon_x + dx, icon_y + 11, theme.taskbar_btn_bg_inactive);
        }

        // Text label
        let text_x = x + 22;
        let text_y = y + (item_h - 8) / 2;
        let text_col = if self.selected {
            theme.start_menu_bg // Inverse for highlight
        } else {
            theme.text_main
        };
        draw_str_f8(
            target,
            text_x,
            text_y,
            &self.name,
            text_col,
            None,
            Some(width.saturating_sub(26)),
        );
    }
}

// ─── StartMenu ───────────────────────────────────────────────────────────────

pub struct StartMenu {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub visible: bool,
    pub items: Vec<MenuEntry>,
    pub hovered: Option<usize>,
}

impl StartMenu {
    pub fn new(x: usize, y: usize) -> Self {
        let mut m = Self {
            x,
            y,
            width: 200,
            height: 0,
            visible: false,
            items: Vec::new(),
            hovered: None,
        };
        m.items.push(MenuEntry::new("Terminal", IconType::Terminal));
        m.items
            .push(MenuEntry::new("File Manager", IconType::Folder));
        m.items.push(MenuEntry::new("Text Editor", IconType::File));
        m.items.push(MenuEntry::new("Settings", IconType::Settings));
        m.items
            .push(MenuEntry::new("SysInfo", IconType::Application));
        m.items.push(MenuEntry::new("Paint", IconType::Application));
        m.items.push(MenuEntry::new("Snake", IconType::Application));
        m.items
            .push(MenuEntry::new("Tetris", IconType::Application));
        m.items
            .push(MenuEntry::new("Calculator", IconType::Application));
        m.items
            .push(MenuEntry::new("Task Manager", IconType::Settings));
        m.items.push(MenuEntry::new("Clock", IconType::Settings));
        m.items
            .push(MenuEntry::new("Web Browser", IconType::Application));
        m.items.push(MenuEntry::new("Power Off", IconType::Power));
        m.height = m.items.len() * 24 + 8;
        m
    }

    pub fn toggle_visibility(&mut self) {
        self.visible = !self.visible;
    }

    pub fn show(&mut self) {
        self.visible = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn handle_hover(&mut self, mx: usize, my: usize) {
        if !self.visible {
            return;
        }
        self.hovered = None;
        for (i, _) in self.items.iter().enumerate() {
            let iy = self.y + 4 + i * 24;
            if mx >= self.x && mx < self.x + self.width && my >= iy && my < iy + 24 {
                self.hovered = Some(i);
                break;
            }
        }
        for (i, item) in self.items.iter_mut().enumerate() {
            item.selected = self.hovered == Some(i);
        }
    }

    pub fn handle_click(&self, mx: usize, my: usize) -> Option<usize> {
        if !self.visible {
            return None;
        }
        for (i, _) in self.items.iter().enumerate() {
            let iy = self.y + 4 + i * 24;
            if mx >= self.x && mx < self.x + self.width && my >= iy && my < iy + 24 {
                return Some(i);
            }
        }
        None
    }

    pub fn render(&self, target: &mut Display) {
        if !self.visible {
            return;
        }
        let theme = crate::theme::get_theme_colors();
        // Shadow (offset +4 down/right with a solid dark color)
        let sh_off = 4usize;
        let shadow_col = Color::from_u32(0x11111B); // modern shadow

        // Right shadow edge
        if self.x + self.width < target.width() && self.y + sh_off < target.height() {
            target.draw_rect(
                self.x + self.width,
                self.y + sh_off,
                sh_off,
                self.height,
                shadow_col,
            );
        }
        // Bottom shadow edge
        if self.x + sh_off < target.width() && self.y + self.height < target.height() {
            target.draw_rect(
                self.x + sh_off,
                self.y + self.height,
                self.width,
                sh_off,
                shadow_col,
            );
        }

        // Background
        let bg = theme.start_menu_bg;
        target.draw_rect(self.x, self.y, self.width, self.height, bg);

        // Left color strip (XFCE / modern accent)
        target.draw_rect(self.x, self.y, 3, self.height, theme.start_menu_accent);

        // Border
        let border = theme.taskbar_border;
        target.draw_rect(self.x, self.y, self.width, 1, border); // top
        target.draw_rect(self.x, self.y + self.height - 1, self.width, 1, border); // bottom
        target.draw_rect(self.x + self.width - 1, self.y, 1, self.height, border); // right

        // Header "Applications"
        let header_h = 0usize; // No separate header for compactness

        // Menu items
        for (i, item) in self.items.iter().enumerate() {
            let iy = self.y + 4 + header_h + i * 24;
            if iy + 24 <= target.height() {
                item.render(target, self.x + 3, iy, self.width - 4);
                // Separator between items (subtle)
                if i + 1 < self.items.len() {
                    target.draw_rect(
                        self.x + 20,
                        iy + 23,
                        self.width - 24,
                        1,
                        theme.taskbar_border,
                    );
                }
            }
        }
    }
}

// ─── SearchMenu ──────────────────────────────────────────────────────────────

pub struct SearchMenu {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub visible: bool,
    pub items: Vec<MenuEntry>,
    pub filtered_items: Vec<(usize, MenuEntry)>,
    pub hovered: Option<usize>,
    pub search_query: String,
}

impl SearchMenu {
    pub fn new(x: usize, y: usize) -> Self {
        let base_menu = StartMenu::new(x, y);
        let items = base_menu.items;
        let mut sm = Self {
            x,
            y,
            width: 250,
            height: 300,
            visible: false,
            items: items.clone(),
            filtered_items: items.into_iter().enumerate().collect(),
            hovered: None,
            search_query: String::new(),
        };
        sm.update_filter();
        sm
    }
    pub fn toggle_visibility(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.search_query.clear();
            self.update_filter();
        }
    }
    pub fn hide(&mut self) {
        self.visible = false;
    }
    pub fn update_filter(&mut self) {
        self.filtered_items.clear();
        let query = self.search_query.to_lowercase();
        for (i, item) in self.items.iter().enumerate() {
            if query.is_empty() || item.name.to_lowercase().contains(&query) {
                self.filtered_items.push((i, item.clone()));
            }
        }
        self.height = self.filtered_items.len() * 24 + 32 + 8; // 32 for search
                                                               // bar
                                                               // recalculate Y
                                                               // so it stays
                                                               // above taskbar
                                                               // properly
                                                               // Actually, we
                                                               // leave Y computation
                                                               // to when it's
                                                               // rendered, but
                                                               // height adapts.
    }
    pub fn handle_char(&mut self, c: char) {
        if c == '\x08' {
            // Backspace
            self.search_query.pop();
        } else if (c as u32) >= 32 && (c as u32) < 127 {
            self.search_query.push(c);
        }
        self.update_filter();
    }
    pub fn handle_hover(&mut self, mx: usize, my: usize, real_y: usize) {
        if !self.visible {
            return;
        }
        self.hovered = None;
        for (i, _) in self.filtered_items.iter().enumerate() {
            let iy = real_y + 36 + i * 24;
            if mx >= self.x && mx < self.x + self.width && my >= iy && my < iy + 24 {
                self.hovered = Some(i);
                break;
            }
        }
        for (i, (_, item)) in self.filtered_items.iter_mut().enumerate() {
            item.selected = self.hovered == Some(i);
        }
    }
    pub fn handle_click(&self, mx: usize, my: usize, real_y: usize) -> Option<usize> {
        if !self.visible {
            return None;
        }
        for (i, (orig_id, _)) in self.filtered_items.iter().enumerate() {
            let iy = real_y + 36 + i * 24;
            if mx >= self.x && mx < self.x + self.width && my >= iy && my < iy + 24 {
                return Some(*orig_id);
            }
        }
        None
    }
    pub fn render(&self, target: &mut Display, real_y: usize) {
        if !self.visible {
            return;
        }
        let theme = crate::theme::get_theme_colors();
        let bg = theme.start_menu_bg;
        let border = theme.taskbar_border;

        target.draw_rect(self.x, real_y, self.width, self.height, bg);
        target.draw_rect(self.x, real_y, self.width, 1, border);
        target.draw_rect(self.x, real_y + self.height - 1, self.width, 1, border);
        target.draw_rect(self.x + self.width - 1, real_y, 1, self.height, border);

        // Search text box
        target.draw_rect(
            self.x + 8,
            real_y + 8,
            self.width - 16,
            24,
            theme.taskbar_bg,
        );
        target.draw_rect(
            self.x + 8,
            real_y + 8,
            self.width - 16,
            1,
            theme.taskbar_border,
        );

        let mut disp_text = self.search_query.clone();
        disp_text.push('_'); // cursor
        draw_str_f8(
            target,
            self.x + 12,
            real_y + 16,
            &disp_text,
            theme.text_main,
            None,
            None,
        );

        for (i, (_, item)) in self.filtered_items.iter().enumerate() {
            let iy = real_y + 36 + i * 24;
            if iy + 24 <= target.height() {
                item.render(target, self.x + 3, iy, self.width - 4);
                if i + 1 < self.filtered_items.len() {
                    target.draw_rect(
                        self.x + 20,
                        iy + 23,
                        self.width - 24,
                        1,
                        theme.taskbar_border,
                    );
                }
            }
        }
    }
}

// ─── DesktopEnvironment
// ───────────────────────────────────────────────────────

pub struct DesktopEnvironment {
    pub taskbar: Taskbar,
    pub start_menu: StartMenu,
    pub search_menu: SearchMenu,
    pub icons: Vec<DesktopIcon>,
    pub focused_window: Option<usize>,
    pub dragging_icon: Option<usize>,
    pub drag_offset_x: isize,
    pub drag_offset_y: isize,
    /// Очередь окон для создания (создаётся в Compositor::composite)
    pub pending_windows: Vec<PendingWindow>,
    pub windows: Vec<Arc<Mutex<Window>>>,
}

/// Окно, которое нужно создать
pub enum PendingWindow {
    Calculator,
    Clock,
    TextWindow(String),
    FileManager,
    Settings,
    TextEditor,
    SysInfo,
    Paint,
    TaskManager,
    Snake,
    Tetris,
    Browser,
    Shell,
}

impl DesktopEnvironment {
    pub fn new(screen_w: usize, screen_h: usize) -> Self {
        let taskbar = Taskbar::new(0, screen_h.saturating_sub(28), screen_w);
        let items_len = 12; // StartMenu items count
        let start_menu_y = screen_h.saturating_sub(28 + items_len * 24 + 12);
        let start_menu = StartMenu::new(2, start_menu_y);
        let search_menu = SearchMenu::new(82, 0); // x right next to start button

        // Default desktop icons
        let mut icons = Vec::new();
        icons.push(DesktopIcon::new(20, 20, "Home", Color::from_u32(0xCAA9E0)));
        icons.push(DesktopIcon::new(
            20,
            90,
            "Terminal",
            Color::from_u32(0xA6E3A1),
        ));
        icons.push(DesktopIcon::new(
            20,
            160,
            "Files",
            Color::from_u32(0x89B4FA),
        ));
        icons.push(DesktopIcon::new(
            20,
            230,
            "Settings",
            Color::from_u32(0xCBA6F7),
        ));
        icons.push(DesktopIcon::new(
            20,
            300,
            "SysInfo",
            Color::from_u32(0xBAC2DE),
        ));
        icons.push(DesktopIcon::new(
            20,
            370,
            "Paint",
            Color::from_u32(0xF9E2AF),
        ));

        Self {
            taskbar,
            start_menu,
            search_menu,
            icons,
            focused_window: None,
            dragging_icon: None,
            drag_offset_x: 0,
            drag_offset_y: 0,
            pending_windows: Vec::new(),
            windows: Vec::new(),
        }
    }

    pub fn trigger_icon_launch(&mut self, idx: usize) {
        if let Some(icon) = self.icons.get(idx) {
            match icon.label.as_str() {
                "Home" | "Files" => self.pending_windows.push(PendingWindow::FileManager),
                "Settings" => self.pending_windows.push(PendingWindow::Settings),
                "SysInfo" => self.pending_windows.push(PendingWindow::SysInfo),
                "Paint" => self.pending_windows.push(PendingWindow::Paint),
                "Terminal" => self.pending_windows.push(PendingWindow::Shell),
                "Text Editor" => self.pending_windows.push(PendingWindow::TextEditor),
                "Snake" => self.pending_windows.push(PendingWindow::Snake),
                "Tetris" => self.pending_windows.push(PendingWindow::Tetris),
                "Calculator" => self.pending_windows.push(PendingWindow::Calculator),
                "Task Manager" => self.pending_windows.push(PendingWindow::TaskManager),
                "Clock" => self.pending_windows.push(PendingWindow::Clock),
                _ => {},
            }
        }
    }

    /// Обрабатывает клик по пункту меню
    fn open_menu_item(&mut self, idx: usize) {
        match idx {
            0 => self.pending_windows.push(PendingWindow::Shell),
            1 => self.pending_windows.push(PendingWindow::FileManager),
            2 => self.pending_windows.push(PendingWindow::TextEditor),
            3 => self.pending_windows.push(PendingWindow::Settings),
            4 => self.pending_windows.push(PendingWindow::SysInfo),
            5 => self.pending_windows.push(PendingWindow::Paint),
            6 => self.pending_windows.push(PendingWindow::Snake),
            7 => self.pending_windows.push(PendingWindow::Tetris),
            8 => self.pending_windows.push(PendingWindow::Calculator),
            9 => self.pending_windows.push(PendingWindow::TaskManager),
            10 => self.pending_windows.push(PendingWindow::Clock),
            11 => self.pending_windows.push(PendingWindow::Browser),
            12 => {
                crate::power::shutdown();
            },
            _ => {},
        }
    }

    pub fn render(&self, target: &mut Display) {
        // Desktop icons
        for icon in &self.icons {
            icon.render(target);
        }
        // Taskbar always on top
        self.taskbar.render(target);
        // Start menu drawn over taskbar
        self.start_menu.render(target);
        // Search menu rendered dynamically
        let sm_y = target.height().saturating_sub(28 + self.search_menu.height);
        self.search_menu.render(target, sm_y);
    }

    pub fn handle_click(&mut self, mx: usize, my: usize) -> bool {
        let tb_y = self.taskbar.y;
        // Search menu click
        if self.search_menu.visible {
            let sm_y = tb_y.saturating_sub(self.search_menu.height);
            if let Some(idx) = self.search_menu.handle_click(mx, my, sm_y) {
                self.search_menu.hide();
                self.open_menu_item(idx);
                return true;
            }
            if mx < self.search_menu.x
                || mx >= self.search_menu.x + self.search_menu.width
                || my < sm_y
                || my >= sm_y + self.search_menu.height
            {
                if !(my >= tb_y && mx >= self.taskbar.x + 80 && mx < self.taskbar.x + 110) {
                    self.search_menu.hide();
                }
            }
        }

        // Start menu click (before taskbar so it's above)
        if self.start_menu.visible {
            if let Some(idx) = self.start_menu.handle_click(mx, my) {
                self.start_menu.hide();
                self.open_menu_item(idx);
                return true;
            }
            // Click outside closes menu too
            if mx < self.start_menu.x
                || mx >= self.start_menu.x + self.start_menu.width
                || my < self.start_menu.y
                || my >= self.start_menu.y + self.start_menu.height
            {
                // But not if clicking the start button
                if !(my >= tb_y && mx < self.taskbar.x + 80) {
                    self.start_menu.hide();
                }
            }
        }

        // Taskbar
        if let Some(action) = self.taskbar.handle_click(mx, my) {
            match action {
                TaskbarAction::StartMenu => {
                    self.search_menu.hide();
                    self.start_menu.toggle_visibility();
                    return true;
                },
                TaskbarAction::SearchClick => {
                    self.start_menu.hide();
                    self.search_menu.toggle_visibility();
                    return true;
                },
                TaskbarAction::TaskClick(id) => {
                    if let Some(win_arc) = self.windows.iter().find(|w| w.lock().id == id) {
                        let mut win = win_arc.lock();
                        win.minimized = false;
                        win.focused = true;
                    }
                    self.focused_window = Some(id);
                    return true;
                },
                TaskbarAction::ClockClick => {
                    return true;
                },
                TaskbarAction::LayoutClick => {
                    crate::keyboard::toggle_layout();
                    let lstr =
                        if crate::keyboard::get_layout() == crate::keyboard::KeyboardLayout::RU {
                            "RU"
                        } else {
                            "EN"
                        };
                    self.taskbar.layout_label = String::from(lstr);
                    return true;
                },
            }
        }

        // Desktop icons
        for (i, icon) in self.icons.iter().enumerate() {
            if icon.hit_test(mx, my) {
                self.dragging_icon = Some(i);
                self.drag_offset_x = mx as isize - icon.x as isize;
                self.drag_offset_y = my as isize - icon.y as isize;
                return true;
            }
        }
        false
    }

    pub fn handle_mouse_move(&mut self, mx: usize, my: usize, screen_h: usize) {
        self.taskbar.handle_hover(mx, my);
        self.start_menu.handle_hover(mx, my);
        let sm_y = screen_h.saturating_sub(28 + self.search_menu.height);
        self.search_menu.handle_hover(mx, my, sm_y);
    }
}

// ─── Arrow Cursor ────────────────────────────────────────────────────────────

// 12x19 arrow cursor bitmask (row = bit pattern, MSB = leftmost)
const CURSOR_MASK: [u16; 19] = [
    0b1000000000000000,
    0b1100000000000000,
    0b1110000000000000,
    0b1111000000000000,
    0b1111100000000000,
    0b1111110000000000,
    0b1111111000000000,
    0b1111111100000000,
    0b1111111110000000,
    0b1111111111000000,
    0b1111111100000000,
    0b1111011000000000,
    0b1110001100000000,
    0b1000001100000000,
    0b0000001110000000,
    0b0000001110000000,
    0b0000000110000000,
    0b0000000000000000,
    0b0000000000000000,
];

const CURSOR_OUTLINE: [u16; 19] = [
    0b1000000000000000,
    0b1100000000000000,
    0b1010000000000000,
    0b1001000000000000,
    0b1000100000000000,
    0b1000010000000000,
    0b1000001000000000,
    0b1000000100000000,
    0b1000000010000000,
    0b1111111111000000,
    0b1000011100000000,
    0b1010010100000000,
    0b1001001100000000,
    0b1000001100000000,
    0b0000001010000000,
    0b0000001010000000,
    0b0000000110000000,
    0b0000000000000000,
    0b0000000000000000,
];

fn draw_cursor(target: &mut Display, x: isize, y: isize) {
    for (row, (&mask, &outline)) in CURSOR_MASK.iter().zip(CURSOR_OUTLINE.iter()).enumerate() {
        for col in 0..12usize {
            let bit = 0x8000u16 >> col;
            let px = x + col as isize;
            let py = y + row as isize;
            if px < 0 || py < 0 || px >= target.width() as isize || py >= target.height() as isize {
                continue;
            }
            if mask & bit != 0 {
                if outline & bit != 0 && (col == 0 || row == 0 || col == 11) {
                    // Outline pixel
                    target.draw_pixel(px as usize, py as usize, Color::BLACK);
                } else {
                    target.draw_pixel(px as usize, py as usize, Color::WHITE);
                }
            }
        }
    }
}

fn draw_shadow(display: &mut Display, x: usize, y: usize, w: usize, h: usize) {
    for row in 0..h {
        let py = y + row;
        if py >= display.height() {
            break;
        }
        for col in 0..w {
            let px = x + col;
            if px >= display.width() {
                break;
            }
            let bg = display.get_pixel(px, py);
            // 50% opacity blend with black shadow
            let c = Color {
                r: bg.r / 2,
                g: bg.g / 2,
                b: bg.b / 2,
                a: 255,
            };
            display.draw_pixel(px, py, c);
        }
    }
}

// ─── Compositor ──────────────────────────────────────────────────────────────

// Размер буфера сохранения курсора: 19 строк × 12 пикселей
const CURSOR_W: usize = 12;
const CURSOR_H: usize = 19;

/// Привязывает Calculator-состояние к окну (по Arc-указателю)
struct CalcState {
    window: Arc<Mutex<Window>>,
    calc: crate::calculator::Calculator,
    hovered_btn: Option<usize>,
}

pub struct AppWindowState {
    window: Arc<Mutex<Window>>,
    kind: crate::apps::AppKind,
    state: crate::apps::AppState,
}

/// Привязывает Clock к окну
struct ClockWindowState {
    window: Arc<Mutex<Window>>,
}

const NUM_DESKTOPS: usize = 4;

#[derive(Clone, Copy)]
pub enum Theme {
    Light,
    Dark,
}

pub struct Compositor {
    pub mouse_x: isize,
    pub mouse_y: isize,
    pub windows: Vec<Arc<Mutex<Window>>>,
    pub current_desktop: usize,
    pub background_color: Color,
    pub screen_width: usize,
    pub screen_height: usize,
    pub backbuffer: alloc::vec::Vec<Color>,
    mouse_btn_left: bool,
    pub desktop_env: DesktopEnvironment,
    pub theme: Theme,
    dragging_window: Option<usize>,
    /// Калькуляторы (по одному на каждое открытое окно)
    calc_states: Vec<CalcState>,
    /// App windows (file manager, settings, editor, paint, sysinfo)
    pub app_states: Vec<AppWindowState>,
    pub wallpaper: Option<Vec<Color>>,
    /// Часы-окна
    clock_states: Vec<ClockWindowState>,
    /// Пиксели под курсором (сохраняются перед отрисовкой, восстанавливаются
    /// перед движением)
    cursor_save: [Color; CURSOR_W * CURSOR_H],
    /// Предыдущая позиция курсора (для восстановления фона)
    prev_mouse_x: isize,
    prev_mouse_y: isize,
    /// Нужен ли полный перерендер сцены (при открытии окна, скролле, старте)
    full_redraw: bool,
    /// Dirty-регион для частичного flush
    dirty: DirtyRect,
    /// Последний тик часов (для обновления только при смене секунды)
    last_clock_tick: u64,
    /// Hardware cursor instance
    pub hardware_cursor: crate::graphics::HardwareCursor,
}

impl Compositor {
    pub fn new() -> Self {
        Compositor {
            mouse_x: 300,
            mouse_y: 300,
            windows: Vec::new(),
            current_desktop: 0,
            background_color: crate::theme::get_theme_colors().desktop_bg,
            screen_width: 1024,
            screen_height: 768,
            backbuffer: alloc::vec::Vec::new(),
            mouse_btn_left: false,
            desktop_env: DesktopEnvironment::new(1024, 768),
            dragging_window: None,
            theme: Theme::Dark,
            calc_states: Vec::new(),
            app_states: Vec::new(),
            wallpaper: None,
            clock_states: Vec::new(),
            cursor_save: [Color::BLACK; CURSOR_W * CURSOR_H],
            prev_mouse_x: 300,
            prev_mouse_y: 300,
            full_redraw: true,
            dirty: DirtyRect::empty(),
            last_clock_tick: 0,
            hardware_cursor: crate::graphics::HardwareCursor::new_arrow(),
        }
    }

    pub fn add_window(&mut self, window: Arc<Mutex<Window>>) {
        {
            let mut w = window.lock();
            w.desktop_id = self.current_desktop;
        }
        self.windows.push(window);
        self.full_redraw = true;
    }

    /// Запрашивает полный перерендер (при открытии окна, изменении рабочего
    /// стола)
    pub fn request_full_redraw(&mut self) {
        self.full_redraw = true;
    }

    pub fn generate_stars(&mut self) {
        let w = self.screen_width;
        let h = self.screen_height;
        let mut wall = alloc::vec![Color::from_u32(0x000000); w * h];

        // Pseudo-random star generation (deterministic for the session)
        let mut seed = 0x12345678u32;
        for _ in 0..400 {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            let x = (seed as usize) % w;
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            let y = (seed as usize) % h;
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            let brightness = 150 + (seed % 105) as u8;

            let c = Color {
                r: brightness,
                g: brightness,
                b: brightness,
                a: 255,
            };
            if y * w + x < wall.len() {
                wall[y * w + x] = c;
            }
        }
        self.wallpaper = Some(wall);
    }

    /// Добавляет грязный регион для частичного flush
    pub fn mark_dirty(&mut self, x: usize, y: usize, w: usize, h: usize) {
        self.dirty = self.dirty.union(DirtyRect {
            x,
            y,
            w,
            h,
        });
    }

    /// Помечает область окна как грязную (включая тень)
    pub fn mark_window_dirty_by_id(&mut self, win_id: usize) {
        let win_data = self.windows.iter().find_map(|w| {
            let win = w.lock();
            if win.id == win_id {
                let mut sw = win.width();
                let mut sh = win.height();
                if win.animation_progress >= 1.0 {
                    sw += 4;
                    sh += 4;
                }
                Some((win.x as usize, win.y as usize, sw, sh))
            } else {
                None
            }
        });

        if let Some((x, y, w, h)) = win_data {
            self.mark_dirty(x, y, w, h);
        }
    }

    /// Сохраняет пиксели под курсором из display.buffer
    fn save_cursor_bg(&mut self, display: &Display) {
        let cx = self.mouse_x;
        let cy = self.mouse_y;
        for row in 0..CURSOR_H {
            for col in 0..CURSOR_W {
                let px = cx + col as isize;
                let py = cy + row as isize;
                if px >= 0
                    && py >= 0
                    && (px as usize) < display.width()
                    && (py as usize) < display.height()
                {
                    self.cursor_save[row * CURSOR_W + col] =
                        display.get_pixel(px as usize, py as usize);
                }
            }
        }
    }

    /// Восстанавливает пиксели под предыдущей позицией курсора
    fn restore_cursor_bg(&self, display: &mut Display) {
        let cx = self.prev_mouse_x;
        let cy = self.prev_mouse_y;
        for row in 0..CURSOR_H {
            for col in 0..CURSOR_W {
                let px = cx + col as isize;
                let py = cy + row as isize;
                if px >= 0
                    && py >= 0
                    && (px as usize) < display.width()
                    && (py as usize) < display.height()
                {
                    display.draw_pixel(
                        px as usize,
                        py as usize,
                        self.cursor_save[row * CURSOR_W + col],
                    );
                }
            }
        }
    }

    /// Обрабатывает pending_windows — создаёт новые окна
    fn process_pending(&mut self) {
        let pending: Vec<PendingWindow> = core::mem::take(&mut self.desktop_env.pending_windows);
        for pw in pending {
            match pw {
                PendingWindow::Calculator => {
                    let (ww, wh) = crate::calculator::calc_window_size();
                    let win = Window::new(200, 150, ww, wh, String::from("Calculator"));
                    let win_arc = Arc::new(Mutex::new(win));
                    let win_id = win_arc.lock().id;
                    self.desktop_env
                        .taskbar
                        .add_task(TaskItem::new(win_id, String::from("Calculator")));
                    let calc = crate::calculator::Calculator::new();
                    {
                        let mut w = win_arc.lock();
                        crate::calculator::render_calc(&mut *w, &calc, None);
                    }
                    self.calc_states.push(CalcState {
                        window: win_arc.clone(),
                        calc,
                        hovered_btn: None,
                    });
                    self.add_window(win_arc);
                },
                PendingWindow::Clock => {
                    let win = Window::new(300, 200, 200, 90, String::from("Clock"));
                    let win_arc = Arc::new(Mutex::new(win));
                    let win_id = win_arc.lock().id;
                    self.clock_states.push(ClockWindowState {
                        window: win_arc.clone(),
                    });
                    self.desktop_env
                        .taskbar
                        .add_task(TaskItem::new(win_id, String::from("Clock")));
                    self.add_window(win_arc);
                },
                PendingWindow::TaskManager => {
                    let (ww, wh) = crate::apps::app_window_size(crate::apps::AppKind::TaskManager);
                    let win = Window::new(100, 100, ww, wh, String::from("Task Manager"));
                    let win_arc = Arc::new(Mutex::new(win));
                    let win_id = win_arc.lock().id;
                    self.desktop_env
                        .taskbar
                        .add_task(TaskItem::new(win_id, String::from("Tasks")));
                    let state = crate::apps::AppState::new(crate::apps::AppKind::TaskManager);
                    {
                        let mut w = win_arc.lock();
                        crate::apps::render_app(&mut *w, &state, crate::apps::AppKind::TaskManager);
                    }
                    self.app_states.push(AppWindowState {
                        window: win_arc.clone(),
                        kind: crate::apps::AppKind::TaskManager,
                        state,
                    });
                    self.add_window(win_arc);
                },
                PendingWindow::TextWindow(title) => {
                    let win = Window::new(180, 160, 380, 260, title.clone());
                    let win_id = win.id;
                    self.desktop_env
                        .taskbar
                        .add_task(TaskItem::new(win_id, title));
                    self.windows.push(Arc::new(Mutex::new(win)));
                },
                PendingWindow::FileManager => {
                    let (ww, wh) = crate::apps::app_window_size(crate::apps::AppKind::FileManager);
                    let win = Window::new(160, 120, ww, wh, String::from("File Manager"));
                    let win_arc = Arc::new(Mutex::new(win));
                    let win_id = win_arc.lock().id;
                    self.desktop_env
                        .taskbar
                        .add_task(TaskItem::new(win_id, String::from("Files")));
                    let mut state = crate::apps::AppState::new(crate::apps::AppKind::FileManager);
                    if let crate::apps::AppState::FileManager {
                        ref mut entries,
                        ref current_dir,
                        ..
                    } = state
                    {
                        *entries = crate::apps::read_dir_entries(current_dir);
                    }
                    {
                        let mut w = win_arc.lock();
                        crate::apps::render_app(&mut *w, &state, crate::apps::AppKind::FileManager);
                    }
                    self.app_states.push(AppWindowState {
                        window: win_arc.clone(),
                        kind: crate::apps::AppKind::FileManager,
                        state,
                    });
                    self.windows.push(win_arc);
                },
                PendingWindow::Settings => {
                    let (ww, wh) = crate::apps::app_window_size(crate::apps::AppKind::Settings);
                    let win = Window::new(220, 140, ww, wh, String::from("Settings"));
                    let win_arc = Arc::new(Mutex::new(win));
                    let win_id = win_arc.lock().id;
                    self.desktop_env
                        .taskbar
                        .add_task(TaskItem::new(win_id, String::from("Settings")));
                    let state = crate::apps::AppState::new(crate::apps::AppKind::Settings);
                    {
                        let mut w = win_arc.lock();
                        crate::apps::render_app(&mut *w, &state, crate::apps::AppKind::Settings);
                    }
                    self.app_states.push(AppWindowState {
                        window: win_arc.clone(),
                        kind: crate::apps::AppKind::Settings,
                        state,
                    });
                    self.windows.push(win_arc);
                },
                PendingWindow::TextEditor => {
                    let (ww, wh) = crate::apps::app_window_size(crate::apps::AppKind::TextEditor);
                    let win = Window::new(200, 160, ww, wh, String::from("Text Editor"));
                    let win_arc = Arc::new(Mutex::new(win));
                    let win_id = win_arc.lock().id;
                    self.desktop_env
                        .taskbar
                        .add_task(TaskItem::new(win_id, String::from("Text Editor")));
                    let state = crate::apps::AppState::new(crate::apps::AppKind::TextEditor);
                    {
                        let mut w = win_arc.lock();
                        crate::apps::render_app(&mut *w, &state, crate::apps::AppKind::TextEditor);
                    }
                    self.app_states.push(AppWindowState {
                        window: win_arc.clone(),
                        kind: crate::apps::AppKind::TextEditor,
                        state,
                    });
                    self.add_window(win_arc);
                },
                PendingWindow::SysInfo => {
                    let (ww, wh) = crate::apps::app_window_size(crate::apps::AppKind::SysInfo);
                    let win = Window::new(240, 180, ww, wh, String::from("System Info"));
                    let win_arc = Arc::new(Mutex::new(win));
                    let win_id = win_arc.lock().id;
                    self.desktop_env
                        .taskbar
                        .add_task(TaskItem::new(win_id, String::from("SysInfo")));
                    let state = crate::apps::AppState::new(crate::apps::AppKind::SysInfo);
                    {
                        let mut w = win_arc.lock();
                        crate::apps::render_app(&mut *w, &state, crate::apps::AppKind::SysInfo);
                    }
                    self.app_states.push(AppWindowState {
                        window: win_arc.clone(),
                        kind: crate::apps::AppKind::SysInfo,
                        state,
                    });
                    self.add_window(win_arc);
                },
                PendingWindow::Paint => {
                    let (ww, wh) = crate::apps::app_window_size(crate::apps::AppKind::Paint);
                    let win = Window::new(200, 140, ww, wh, String::from("Paint"));
                    let win_arc = Arc::new(Mutex::new(win));
                    let win_id = win_arc.lock().id;
                    self.desktop_env
                        .taskbar
                        .add_task(TaskItem::new(win_id, String::from("Paint")));
                    let state = crate::apps::AppState::new(crate::apps::AppKind::Paint);
                    {
                        let mut w = win_arc.lock();
                        crate::apps::render_app(&mut *w, &state, crate::apps::AppKind::Paint);
                    }
                    self.app_states.push(AppWindowState {
                        window: win_arc.clone(),
                        kind: crate::apps::AppKind::Paint,
                        state,
                    });
                    self.add_window(win_arc);
                },
                PendingWindow::Snake => {
                    let (ww, wh) = crate::apps::app_window_size(crate::apps::AppKind::Snake);
                    let win = Window::new(220, 160, ww, wh, String::from("Snake"));
                    let win_arc = Arc::new(Mutex::new(win));
                    let win_id = win_arc.lock().id;
                    self.desktop_env
                        .taskbar
                        .add_task(TaskItem::new(win_id, String::from("Snake")));
                    let state = crate::apps::AppState::new(crate::apps::AppKind::Snake);
                    {
                        let mut w = win_arc.lock();
                        crate::apps::render_app(&mut *w, &state, crate::apps::AppKind::Snake);
                    }
                    self.app_states.push(AppWindowState {
                        window: win_arc.clone(),
                        kind: crate::apps::AppKind::Snake,
                        state,
                    });
                    self.add_window(win_arc);
                },
                PendingWindow::Tetris => {
                    let (ww, wh) = crate::apps::app_window_size(crate::apps::AppKind::Tetris);
                    let win = Window::new(260, 120, ww, wh, String::from("Tetris"));
                    let win_arc = Arc::new(Mutex::new(win));
                    let win_id = win_arc.lock().id;
                    self.desktop_env
                        .taskbar
                        .add_task(TaskItem::new(win_id, String::from("Tetris")));
                    let state = crate::apps::AppState::new(crate::apps::AppKind::Tetris);
                    {
                        let mut w = win_arc.lock();
                        crate::apps::render_app(&mut *w, &state, crate::apps::AppKind::Tetris);
                    }
                    self.app_states.push(AppWindowState {
                        window: win_arc.clone(),
                        kind: crate::apps::AppKind::Tetris,
                        state,
                    });
                    self.add_window(win_arc);
                },
                PendingWindow::Browser => {
                    let (ww, wh) = crate::apps::app_window_size(crate::apps::AppKind::Browser);
                    let win = Window::new(140, 100, ww, wh, String::from("Web Browser"));
                    let win_arc = Arc::new(Mutex::new(win));
                    let win_id = win_arc.lock().id;
                    self.desktop_env
                        .taskbar
                        .add_task(TaskItem::new(win_id, String::from("Browser")));
                    let state = crate::apps::AppState::new(crate::apps::AppKind::Browser);
                    {
                        let mut w = win_arc.lock();
                        crate::apps::render_app(&mut *w, &state, crate::apps::AppKind::Browser);
                    }
                    self.app_states.push(AppWindowState {
                        window: win_arc.clone(),
                        kind: crate::apps::AppKind::Browser,
                        state,
                    });
                    self.add_window(win_arc);
                },
                PendingWindow::Shell => {
                    let win = Window::new(100, 100, 640, 400, String::from("Terminal"));
                    let win_arc = Arc::new(Mutex::new(win));
                    let win_id = win_arc.lock().id;
                    self.desktop_env
                        .taskbar
                        .add_task(TaskItem::new(win_id, String::from("Terminal")));
                    let state = crate::apps::AppState::new(crate::apps::AppKind::Shell);
                    {
                        let mut w = win_arc.lock();
                        crate::apps::render_app(&mut *w, &state, crate::apps::AppKind::Shell);
                    }
                    self.app_states.push(AppWindowState {
                        window: win_arc.clone(),
                        kind: crate::apps::AppKind::Shell,
                        state,
                    });
                    self.add_window(win_arc);
                },
            }
        }
    }

    /// Обновляет отображение Clock-окон
    fn update_clock_windows(&mut self) {
        let (h, m, s) = get_time_hms();
        let mut time_str = String::new();
        push_two_digits(&mut time_str, h);
        time_str.push(':');
        push_two_digits(&mut time_str, m);
        time_str.push(':');
        push_two_digits(&mut time_str, s);

        for cs in &self.clock_states {
            let mut win = cs.window.lock();
            if win.closed {
                continue;
            }
            let ww = win.width();
            let wh = win.height();
            let client_y = crate::window::TITLE_BAR_H;

            if wh > client_y {
                // Background: Subtle dark blue gradient
                for ry in client_y..wh {
                    let brightness = 30 + (ry - client_y) * 20 / (wh - client_y).max(1);
                    let c = Color::from_rgb(15, 20, brightness as u8);
                    for rx in 0..ww {
                        win.buffer[ry * ww + rx] = c;
                    }
                }

                // Text: Large Scale
                let text_x = (ww.saturating_sub(time_str.len() * 24)) / 2;
                let text_y = client_y + (wh - client_y).saturating_sub(24) / 2;

                // Shadow
                draw_str_to_buf(
                    &mut win.buffer,
                    ww,
                    wh,
                    text_x + 2,
                    text_y + 2,
                    &time_str,
                    Color::BLACK,
                    3,
                );
                // Main text (Vibrant cyan/white)
                draw_str_to_buf(
                    &mut win.buffer,
                    ww,
                    wh,
                    text_x,
                    text_y,
                    &time_str,
                    Color::from_rgb(0x00, 0xD0, 0xF0),
                    3,
                );
            }
        }
    }

    /// Полный перерендер сцены (вызывается при первом запуске, открытии окна,
    /// etc.)
    fn full_composite(&mut self, display: &mut Display) {
        // Создаём окна из очереди
        self.process_pending();
        // Обновляем Clock-окна
        self.update_clock_windows();
        // Обновляем приложения
        for aw in self.app_states.iter_mut() {
            let mut w = aw.window.lock();
            if w.closed {
                continue;
            }
            crate::apps::render_app(&mut *w, &aw.state, aw.kind);
        }

        let w = display.width();
        let h = display.height();

        self.screen_width = w;
        self.screen_height = h;

        // Resize desktop env если нужно
        let tb = &self.desktop_env.taskbar;
        if tb.width != w || tb.y != h.saturating_sub(28) {
            self.desktop_env = DesktopEnvironment::new(w, h);
        }

        // 1. Фон — обои или градиент
        if self.wallpaper.is_none() {
            self.generate_stars();
        }

        if let Some(ref wall) = self.wallpaper {
            if wall.len() == (w * h) {
                display.blit_from_buffer(wall);
            }
        }

        // 2. Окна (снизу вверх)
        for win_arc in &self.windows {
            let win = win_arc.lock();
            if win.closed || win.minimized || win.desktop_id != self.current_desktop {
                continue;
            }
            let scale = if win.is_animating() {
                win.animation_progress
            } else {
                1.0
            };

            if scale >= 1.0 {
                // Draw drop shadows only when fully open
                let sx = (win.x + win.width() as isize).max(0) as usize;
                let sy = (win.y + 4).max(0) as usize;
                draw_shadow(display, sx, sy, 4, win.height());
                let bx = (win.x + 4).max(0) as usize;
                let by = (win.y + win.height() as isize).max(0) as usize;
                draw_shadow(display, bx, by, win.width(), 4);
            }

            // Draw window via our new scalable blitter
            display.blit_window(win.x, win.y, win.width(), win.height(), &win.buffer, scale);
        }

        // 3. Taskbar + StartMenu + Icons
        self.desktop_env.render(display);

        // 4. Нарисовать аппаратный курсор
        self.hardware_cursor.x = self.mouse_x as usize;
        self.hardware_cursor.y = self.mouse_y as usize;
        self.hardware_cursor.draw_cursor(display);

        self.prev_mouse_x = self.mouse_x;
        self.prev_mouse_y = self.mouse_y;

        // 5. Полный flush в VRAM
        if self.full_redraw {
            display.reset_clip();
            display.flush();
        } else if !self.dirty.is_empty() {
            display.flush_region(self.dirty.x, self.dirty.y, self.dirty.w, self.dirty.h);
        }

        self.full_redraw = false;
        self.dirty = DirtyRect::empty();
        display.reset_clip(); // ensure subsequent cursor draws don't get
                              // clipped weirdly
    }

    pub fn composite(&mut self, display: &mut Display) {
        // Обновляем часы
        let (h_v, m_v, s_v) = get_time_hms();
        let cur_tick = CLOCK_TICKS.load(Ordering::Relaxed);
        if cur_tick != self.last_clock_tick {
            self.last_clock_tick = cur_tick;
            // Форматируем время без alloc::format! (нет fmt в no_std) — вручную
            let mut time_str = String::new();
            push_two_digits(&mut time_str, h_v);
            time_str.push(':');
            push_two_digits(&mut time_str, m_v);
            time_str.push(':');
            push_two_digits(&mut time_str, s_v);
            self.desktop_env.taskbar.update_clock(time_str);

            let lstr = if crate::keyboard::get_layout() == crate::keyboard::KeyboardLayout::RU {
                "RU"
            } else {
                "EN"
            };
            self.desktop_env.taskbar.layout_label = String::from(lstr);

            // Часы меняются раз в секунду — нужен полный редро taskbar-а
            self.full_redraw = true;
        }

        let mut snake_stepped = false;
        let mut tetris_stepped = false;
        let mut tm_stepped = false;
        for aw in self.app_states.iter_mut() {
            if aw.kind == crate::apps::AppKind::Snake {
                if crate::apps::step_snake(&mut aw.state, cur_tick) {
                    snake_stepped = true;
                }
            } else if aw.kind == crate::apps::AppKind::Tetris {
                if crate::apps::step_tetris(&mut aw.state, cur_tick) {
                    tetris_stepped = true;
                }
            } else if aw.kind == crate::apps::AppKind::TaskManager {
                if cur_tick % 18 == 0 {
                    tm_stepped = true;
                }
            }
        }
        if snake_stepped || tetris_stepped || tm_stepped {
            self.full_redraw = true;
        }

        let mut any_animating = false;
        for win_arc in &self.windows {
            let mut win = win_arc.lock();
            win.update_animation(cur_tick);
            if win.is_animating() {
                any_animating = true;
            }
        }
        if any_animating {
            self.full_redraw = true;
        }

        let mut needs_cleanup = false;
        for w in &self.windows {
            if w.lock().closed {
                needs_cleanup = true;
                break;
            }
        }
        if needs_cleanup {
            self.windows.retain(|w| !w.lock().closed);
            self.app_states.retain(|aw| !aw.window.lock().closed);
            self.clock_states.retain(|cs| !cs.window.lock().closed);
            self.calc_states.retain(|cs| !cs.window.lock().closed);

            self.desktop_env.taskbar.tasks.clear();
            for win_arc in &self.windows {
                let win = win_arc.lock();
                if win.desktop_id == self.current_desktop {
                    self.desktop_env
                        .taskbar
                        .add_task(crate::gui::TaskItem::new(win.id, win.title.clone()));
                }
            }
            self.full_redraw = true;
        }

        if self.full_redraw || !self.dirty.is_empty() {
            if !self.full_redraw {
                display.set_clip(self.dirty.x, self.dirty.y, self.dirty.w, self.dirty.h);
            }
            self.full_composite(display);
            return;
        }

        // ── Быстрый путь: только курсор переместился ─────────

        let cursor_moved = self.mouse_x != self.prev_mouse_x || self.mouse_y != self.prev_mouse_y;

        if !cursor_moved {
            return; // Ничего не изменилось — пропускаем кадр
        }

        // 1. Восстановить пиксели под старым курсором — при Hardware Cursor мы просто
        // перерисовываем область, где он БЫЛ, если нужно.
        // Но в нашей архитектуре DrawTarget::draw_pixel пишет прямо в buffer.
        // Для MVP: просто используем старую логику flush, но без ручного рисования.

        self.restore_cursor_bg(display);
        display.flush_region(
            self.prev_mouse_x.max(0) as usize,
            self.prev_mouse_y.max(0) as usize,
            32,
            32,
        );

        // 2. Нарисовать на новой позиции
        self.hardware_cursor.x = self.mouse_x as usize;
        self.hardware_cursor.y = self.mouse_y as usize;
        self.hardware_cursor.draw_cursor(display);
        display.flush_region(
            self.mouse_x.max(0) as usize,
            self.mouse_y.max(0) as usize,
            32,
            32,
        );

        self.prev_mouse_x = self.mouse_x;
        self.prev_mouse_y = self.mouse_y;
    }

    pub fn update_mouse(&mut self, packet: MousePacket) {
        self.mouse_x = (self.mouse_x + packet.x_movement as isize).max(0);
        self.mouse_y = (self.mouse_y + packet.y_movement as isize).max(0);

        if let Some(display) = GLOBAL_DISPLAY.try_lock().as_ref() {
            if let Some(disp) = display.as_ref() {
                self.mouse_x = self.mouse_x.min(disp.width() as isize - 1);
                self.mouse_y = self.mouse_y.min(disp.height() as isize - 1);
            }
        }

        let left = packet.flags & 1 != 0;
        let left_clicked = left && !self.mouse_btn_left;
        let left_released = !left && self.mouse_btn_left;
        self.mouse_btn_left = left;

        let mx = self.mouse_x as usize;
        let my = self.mouse_y as usize;

        // Hover events (always)
        let old_hover = self.desktop_env.taskbar.hovered_btn.clone();
        let old_sm_hover = self.desktop_env.start_menu.hovered;
        self.desktop_env
            .handle_mouse_move(mx, my, self.screen_height);

        // If hover state changed, mark regions dirty instead of a full redraw!
        if self.desktop_env.taskbar.hovered_btn != old_hover {
            self.mark_dirty(
                self.desktop_env.taskbar.x,
                self.desktop_env.taskbar.y,
                self.desktop_env.taskbar.width,
                28,
            );
        }
        if self.desktop_env.start_menu.hovered != old_sm_hover {
            let sm = &self.desktop_env.start_menu;
            self.mark_dirty(sm.x, sm.y, sm.width, sm.height);
        }

        if left_released {
            // Stop dragging
            for win_arc in &self.windows {
                let mut w = win_arc.lock();
                w.dragging = false;
                w.resizing = false;
            }
            self.dragging_window = None;

            if let Some(idx) = self.desktop_env.dragging_icon.take() {
                // Determine if we should launch or just drop
                // We'll check if it moved significantly from where the mouse grabbed it
                // Actually wait, we'd need to compare to a stored position.
                // For simplicity, let's assume if we just clicked and released immediately
                // without moving far, it's a launch. But we don't track far
                // yet. Let's just launch unless the user used drag-and-drop feature of > 10
                // pixels. We'd need to know starting mouse x/y, which we don't
                // store. We'll simply launch it, but we should improve this later.
                self.desktop_env.trigger_icon_launch(idx);
            }
        }

        if left_clicked {
            // При любом клике — нужен полный перерендер (меню открылось, окно поднялось)
            self.full_redraw = true;

            // Desktop env first (taskbar/start menu)
            if self.desktop_env.handle_click(mx, my) {
                return;
            }

            // Desktop icons handled in DesktopEnvironment::handle_click

            // Find topmost window hit
            let mut hit: Option<usize> = None;
            for (i, win_arc) in self.windows.iter().enumerate().rev() {
                let win = win_arc.lock();
                if !win.closed && win.hit_test(self.mouse_x, self.mouse_y) {
                    hit = Some(i);
                    break;
                }
            }

            if let Some(i) = hit {
                // Bring to front
                let win_arc = self.windows.remove(i);
                self.windows.push(win_arc.clone());
                let win_id = win_arc.lock().id;

                // Focus all windows EXCLUDING the one we're about to focus
                for w2 in &self.windows {
                    if !Arc::ptr_eq(w2, &win_arc) {
                        w2.lock().focused = false;
                    }
                }

                let mut win = win_arc.lock();
                win.focused = true;
                self.desktop_env.focused_window = Some(win.id);
                win.redraw_title_bar();

                if win.hit_test_close(self.mouse_x, self.mouse_y) {
                    win.closed = true;
                    self.full_redraw = true;
                } else if win.hit_test_maximize(self.mouse_x, self.mouse_y) {
                    win.toggle_maximize();
                    self.full_redraw = true;
                } else if win.hit_test_minimize(self.mouse_x, self.mouse_y) {
                    win.minimized = true;
                    self.full_redraw = true;
                } else if let Some(dir) = win.hit_test_resize(self.mouse_x, self.mouse_y) {
                    if !win.maximized {
                        win.resizing = true;
                        win.resize_dir = Some(dir);
                        win.resize_start_w = win.width();
                        win.resize_start_h = win.height();
                        win.drag_offset_x = self.mouse_x;
                        win.drag_offset_y = self.mouse_y;
                        self.dragging_window = Some(win_id);
                    }
                } else if win.hit_test_titlebar(self.mouse_x, self.mouse_y) {
                    if !win.maximized {
                        win.dragging = true;
                        win.drag_offset_x = self.mouse_x - win.x;
                        win.drag_offset_y = self.mouse_y - win.y;
                        self.dragging_window = Some(win_id);
                    }
                } else {
                    // Клик в клиентскую область — проверяем калькулятор
                    let local_x = self.mouse_x - win.x;
                    let local_y = self.mouse_y - win.y;
                    let current_win_arc = win_arc.clone();
                    drop(win); // отпускаем лок перед работой с calc_states

                    for cs in self.calc_states.iter_mut() {
                        if Arc::ptr_eq(&cs.window, &current_win_arc) {
                            if let Some(btn_i) = crate::calculator::hit_test_calc(local_x, local_y)
                            {
                                if let Some(kind) = crate::calculator::btn_by_index(btn_i) {
                                    cs.calc.press(kind);
                                    let mut w = current_win_arc.lock();
                                    crate::calculator::render_calc(&mut *w, &cs.calc, Some(btn_i));
                                    self.full_redraw = true;
                                }
                            }
                            break;
                        }
                    }

                    // Другие приложения
                    for aw in self.app_states.iter_mut() {
                        if Arc::ptr_eq(&aw.window, &current_win_arc) {
                            crate::apps::handle_click_app(
                                win_id,
                                &mut aw.state,
                                aw.kind,
                                local_x,
                                local_y,
                            );
                            let mut w = current_win_arc.lock();
                            crate::apps::render_app(&mut *w, &aw.state, aw.kind);
                            self.full_redraw = true;
                            break;
                        }
                    }
                }
            }
        }

        // Drag movement
        if self.mouse_btn_left {
            if let Some(idx) = self.desktop_env.dragging_icon {
                if let Some(icon) = self.desktop_env.icons.get_mut(idx) {
                    let nx = self.mouse_x as isize - self.desktop_env.drag_offset_x;
                    let ny = self.mouse_y as isize - self.desktop_env.drag_offset_y;
                    icon.x = nx.max(0) as usize;
                    icon.y = ny.max(0) as usize;
                    self.full_redraw = true;
                }
            }

            if let Some(di) = self.dragging_window {
                if let Some(win_arc) = self.windows.iter().find(|w| w.lock().id == di) {
                    let mut win = win_arc.lock();
                    if win.dragging && !win.closed {
                        win.x = self.mouse_x - win.drag_offset_x;
                        win.y = self.mouse_y - win.drag_offset_y;

                        let max_x = self.screen_width.saturating_sub(100) as isize;
                        let max_y = self.screen_height.saturating_sub(28) as isize;
                        win.x = win.x.max(0).min(max_x.max(0));
                        win.y = win.y.max(0).min(max_y.max(0));
                        // При перетаскивании нужен полный перерендер
                        self.full_redraw = true;
                    } else if win.resizing && !win.closed {
                        let dx = self.mouse_x - win.drag_offset_x;
                        let dy = self.mouse_y - win.drag_offset_y;
                        let mut new_w = win.resize_start_w;
                        let mut new_h = win.resize_start_h;

                        if let Some(dir) = win.resize_dir {
                            if dir == crate::window::ResizeDir::Right
                                || dir == crate::window::ResizeDir::BottomRight
                            {
                                new_w = (win.resize_start_w as isize + dx).max(100) as usize;
                            }
                            if dir == crate::window::ResizeDir::Bottom
                                || dir == crate::window::ResizeDir::BottomRight
                            {
                                new_h = (win.resize_start_h as isize + dy)
                                    .max(crate::window::TITLE_BAR_H as isize + 20)
                                    as usize;
                            }
                        }
                        win.resize(new_w, new_h);
                        self.full_redraw = true;
                    }
                }
            } else {
                // Paint dragging handling
                let mut hit: Option<usize> = None;
                for (i, win_arc) in self.windows.iter().enumerate().rev() {
                    let win = win_arc.lock();
                    if !win.closed
                        && win.hit_test(self.mouse_x, self.mouse_y)
                        && !win.hit_test_titlebar(self.mouse_x, self.mouse_y)
                        && !win.hit_test_close(self.mouse_x, self.mouse_y)
                    {
                        hit = Some(i);
                        break;
                    }
                }
                if let Some(hit_idx) = hit {
                    if let Some(win_arc) = self.windows.get(hit_idx) {
                        let win = win_arc.lock();
                        let local_x = self.mouse_x - win.x;
                        let local_y = self.mouse_y - win.y;
                        let current_win_arc = win_arc.clone();
                        drop(win);
                        for aw in self.app_states.iter_mut() {
                            if Arc::ptr_eq(&aw.window, &current_win_arc)
                                && aw.kind == crate::apps::AppKind::Paint
                            {
                                crate::apps::handle_click_app(
                                    hit_idx,
                                    &mut aw.state,
                                    aw.kind,
                                    local_x,
                                    local_y,
                                );
                                self.full_redraw = true;
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn cleanup_closed_windows(&mut self) {
        let before = self.windows.len();
        self.windows.retain(|w| !w.lock().closed);
        if self.windows.len() != before {
            self.full_redraw = true;
        }

        // Remove states associated with closed windows
        self.calc_states.retain(|s| !s.window.lock().closed);
        self.app_states.retain(|s| !s.window.lock().closed);
        self.clock_states.retain(|s| !s.window.lock().closed);

        self.desktop_env.taskbar.tasks.clear();
        for (_, win_arc) in self.windows.iter().enumerate() {
            let win = win_arc.lock();
            self.desktop_env
                .taskbar
                .add_task(TaskItem::new(win.id, win.title.clone()));
        }
    }

    pub fn handle_keyboard_char(&mut self, c: char) -> bool {
        // If Search Menu is visible, let it consume the typed keys
        if self.desktop_env.search_menu.visible {
            if c == '\n' {
                // Open first item on Enter
                if !self.desktop_env.search_menu.filtered_items.is_empty() {
                    let first_idx = self.desktop_env.search_menu.filtered_items[0].0;
                    self.desktop_env.search_menu.hide();
                    self.desktop_env.open_menu_item(first_idx);
                }
            } else if c == '\x1B' {
                // Esc to close
                self.desktop_env.search_menu.hide();
            } else {
                self.desktop_env.search_menu.handle_char(c);
            }
            self.full_redraw = true;
            return true;
        }

        // Global Shortcuts
        if c == '\x13' {
            // Alt+Tab
            if !self.windows.is_empty() {
                // Find next valid window on this desktop
                let top_idx = self.windows.len() - 1;
                let mut found = None;
                for i in 0..top_idx {
                    if self.windows[i].lock().desktop_id == self.current_desktop
                        && !self.windows[i].lock().closed
                    {
                        found = Some(i);
                        break;
                    }
                }
                if let Some(idx) = found {
                    let win_arc = self.windows.remove(idx);
                    self.windows.push(win_arc.clone());
                    // Update focus
                    for (i, w) in self.windows.iter().enumerate() {
                        w.lock().focused = i == self.windows.len() - 1;
                    }
                    self.desktop_env.focused_window = Some(win_arc.lock().id);
                    self.full_redraw = true;
                }
            }
            return true;
        }
        if c == '\x14' {
            // Alt+F4
            if let Some(idx) = self.desktop_env.focused_window {
                if let Some(win_arc) = self.windows.iter().find(|w| w.lock().id == idx) {
                    if !win_arc.lock().closed {
                        win_arc.lock().closed = true;
                        self.full_redraw = true;
                    }
                }
            }
            return true;
        }
        if c == '\x15' {
            // Alt+Left (Prev Desktop)
            let next_desktop = if self.current_desktop > 0 {
                self.current_desktop - 1
            } else {
                NUM_DESKTOPS - 1
            };
            self.switch_desktop(next_desktop);
            return true;
        }
        if c == '\x16' {
            // Alt+Right (Next Desktop)
            let next_desktop = (self.current_desktop + 1) % NUM_DESKTOPS;
            self.switch_desktop(next_desktop);
            return true;
        }

        if let Some(idx) = self.desktop_env.focused_window {
            if let Some(win_arc) = self.windows.iter().find(|w| w.lock().id == idx) {
                let current_win_arc = win_arc.clone();
                for aw in self.app_states.iter_mut() {
                    if Arc::ptr_eq(&aw.window, &current_win_arc) {
                        if aw.kind == crate::apps::AppKind::TextEditor {
                            crate::apps::te_handle_keyboard(&mut aw.state, c);
                            let mut w = current_win_arc.lock();
                            crate::apps::render_app(&mut *w, &aw.state, aw.kind);
                            self.full_redraw = true;
                            return true;
                        } else if aw.kind == crate::apps::AppKind::Snake {
                            crate::apps::snake_handle_keyboard(&mut aw.state, c);
                            let mut w = current_win_arc.lock();
                            crate::apps::render_app(&mut *w, &aw.state, aw.kind);
                            self.full_redraw = true;
                            return true;
                        } else if aw.kind == crate::apps::AppKind::Tetris {
                            crate::apps::tetris_handle_keyboard(&mut aw.state, c);
                            let mut w = current_win_arc.lock();
                            crate::apps::render_app(&mut *w, &aw.state, aw.kind);
                            self.full_redraw = true;
                            return true;
                        } else if aw.kind == crate::apps::AppKind::Browser {
                            crate::apps::browser_handle_keyboard(&mut aw.state, c);
                            let mut w = current_win_arc.lock();
                            crate::apps::render_app(&mut *w, &aw.state, aw.kind);
                            self.full_redraw = true;
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
    pub fn set_wallpaper(&mut self, wall: Vec<Color>) {
        self.wallpaper = Some(wall);
        self.full_redraw = true;
    }

    pub fn set_background_color(&mut self, color: Color) {
        self.background_color = color;
        self.wallpaper = None;
        self.full_redraw = true;
    }

    pub fn switch_desktop(&mut self, desktop: usize) {
        if desktop < NUM_DESKTOPS {
            self.current_desktop = desktop;
            self.full_redraw = true;
            // Update taskbar for current desktop
            self.desktop_env.taskbar.tasks.clear();
            for (_, win_arc) in self.windows.iter().enumerate() {
                let win = win_arc.lock();
                if !win.closed && win.desktop_id == self.current_desktop {
                    self.desktop_env
                        .taskbar
                        .add_task(TaskItem::new(win.id, win.title.clone()));
                }
            }
        }
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        match theme {
            Theme::Light => {
                self.background_color = Color::from_u32(0xF0F0F0); // Light background
            },
            Theme::Dark => {
                self.background_color = Color::from_u32(0x2F344A); // Dark background
            },
        }
        self.full_redraw = true;
    }
}

lazy_static! {
    pub static ref COMPOSITOR: Mutex<Compositor> = Mutex::new(Compositor::new());
}

pub fn redraw_display() {
    if let Some(disp) = GLOBAL_DISPLAY.try_lock().as_mut() {
        if let Some(disp) = disp.as_mut() {
            COMPOSITOR.lock().composite(disp);
        }
    }
}

pub fn force_redraw_display() {
    if let Some(mut comp) = COMPOSITOR.try_lock() {
        comp.request_full_redraw();
    }
    redraw_display();
}

pub fn poll_mouse() {}

pub fn mouse_task() -> impl core::future::Future<Output = ()> {
    async move {
        loop {
            let pkt = crate::mouse::get_mouse_packet().await;
            {
                let mut comp = COMPOSITOR.lock();
                comp.update_mouse(pkt);
                // Drain queue synchronously to eliminate display delay
                while let Some(next_pkt) = crate::mouse::get_mouse_packet_sync() {
                    comp.update_mouse(next_pkt);
                }
            }
            redraw_display();
        }
    }
}
