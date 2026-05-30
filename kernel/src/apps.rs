/// apps.rs — simple GUI apps rendered into Window buffers.
///
/// Apps:
/// - FileManager (Explorer-like)
/// - Settings
/// - TextEditor
/// - SysInfo
/// - Paint
///
/// No std, uses alloc only.
use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use font8x8::UnicodeFonts;

use crate::{
    graphics::Color,
    window::{Window, TITLE_BAR_H},
};

// ─── App Types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppKind {
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
    ImageViewer,
    SystemMonitor,
    MusicPlayer,
    Calendar,
    Notes,
}

/// Shared app state for all apps
#[derive(Clone)]
pub enum AppState {
    FileManager {
        current_dir: String,
        selected_item: Option<usize>,
        entries: Vec<String>,
    },
    Settings {
        selected_tab: usize,
        bg_color: Color,
    },
    TextEditor {
        text_lines: Vec<String>,
        cursor_row: usize,
        cursor_col: usize,
        scroll_v: usize,
        file_path: Option<String>,
        is_dirty: bool,
    },
    SysInfo,
    Paint {
        paint_color: Color,
        canvas: Vec<Color>,
        canvas_w: usize,
        canvas_h: usize,
    },
    TaskManager,
    Snake {
        snake_body: Vec<(i32, i32)>,
        snake_dir: (i32, i32),
        snake_fruit: (i32, i32),
        snake_score: u32,
        snake_game_over: bool,
        snake_last_tick: u64,
    },
    Tetris {
        tetris_board: [[u8; 10]; 20],
        tetris_piece: u8,
        tetris_piece_x: i32,
        tetris_piece_y: i32,
        tetris_piece_rot: u8,
        tetris_score: u32,
        tetris_game_over: bool,
        tetris_last_tick: u64,
    },
    Browser {
        tabs: Vec<WebTab>,
        active_tab: usize,
        url_input: String,
        history: Vec<String>,
        page_content: String,
        page_lines: Vec<crate::html_parser::RenderedLine>,
        scroll_offset: usize,
        is_loading: bool,
        error_message: Option<String>,
    },
    ImageViewer(crate::image_viewer::ImageViewerState),
    SystemMonitor(crate::system_monitor::SystemMonitorState),
    MusicPlayer(crate::music_player::MusicPlayerState),
    Calendar(crate::calendar::CalendarState),
    Notes {
        text_lines: Vec<String>,
        cursor_row: usize,
        cursor_col: usize,
        scroll_v: usize,
        file_path: Option<String>,
        is_dirty: bool,
        find_mode: bool,
        find_query: String,
    },
}

#[derive(Clone)]
pub struct WebTab {
    pub title: String,
    pub url: String,
    pub history: Vec<String>,
    pub history_idx: usize,
}

impl AppState {
    pub fn new(kind: AppKind) -> Self {
        match kind {
            AppKind::FileManager => AppState::FileManager {
                current_dir: String::from("/"),
                selected_item: None,
                entries: Vec::new(),
            },
            AppKind::Settings => AppState::Settings {
                selected_tab: 0,
                bg_color: Color::from_u32(0x2F344A),
            },
            AppKind::TextEditor => AppState::TextEditor {
                text_lines: vec![String::new()],
                cursor_row: 0,
                cursor_col: 0,
                scroll_v: 0,
                file_path: None,
                is_dirty: false,
            },
            AppKind::SysInfo => AppState::SysInfo,
            AppKind::Paint => AppState::Paint {
                paint_color: Color::from_u32(0x000000),
                canvas: alloc::vec![Color::WHITE; 320 * 200],
                canvas_w: 320,
                canvas_h: 200,
            },
            AppKind::TaskManager => AppState::TaskManager,
            AppKind::Snake => AppState::Snake {
                snake_body: alloc::vec![(10, 10), (10, 11), (10, 12)],
                snake_dir: (0, -1),
                snake_fruit: (15, 10),
                snake_score: 0,
                snake_game_over: false,
                snake_last_tick: 0,
            },
            AppKind::Tetris => AppState::Tetris {
                tetris_board: [[0; 10]; 20],
                tetris_piece: 0,
                tetris_piece_x: 3,
                tetris_piece_y: 0,
                tetris_piece_rot: 0,
                tetris_score: 0,
                tetris_game_over: false,
                tetris_last_tick: 0,
            },
            AppKind::Shell => AppState::SysInfo, // TODO: Full shell state if needed
            AppKind::Browser => AppState::Browser {
                tabs: alloc::vec![WebTab {
                    title: String::from("Home"),
                    url: String::from("about:home"),
                    history: alloc::vec![String::from("about:home")],
                    history_idx: 0,
                }],
                active_tab: 0,
                url_input: String::from("about:home"),
                history: alloc::vec![],
                page_content: String::new(),
                page_lines: alloc::vec![
                    crate::html_parser::RenderedLine {
                        text: String::from("Welcome to RustOS Browser"),
                        color: Color::from_u32(0x7AA2F7),
                        is_bold: true,
                        is_italic: false,
                        is_heading: true,
                        heading_level: 1,
                        is_link: false,
                        link_url: None,
                        indent: 0,
                        is_horizontal_rule: false,
                        is_image: false,
                        image_alt: String::new(),
                    },
                    crate::html_parser::RenderedLine {
                        text: String::from("Enter a URL and press Enter to browse."),
                        color: Color::from_u32(0xC0CAF5),
                        is_bold: false,
                        is_italic: false,
                        is_heading: false,
                        heading_level: 0,
                        is_link: false,
                        link_url: None,
                        indent: 0,
                        is_horizontal_rule: false,
                        is_image: false,
                        image_alt: String::new(),
                    }
                ],
                scroll_offset: 0,
                is_loading: false,
                error_message: None,
            },
            AppKind::ImageViewer => {
                AppState::ImageViewer(crate::image_viewer::ImageViewerState::new())
            },
            AppKind::SystemMonitor => {
                AppState::SystemMonitor(crate::system_monitor::SystemMonitorState::new())
            },
            AppKind::MusicPlayer => {
                AppState::MusicPlayer(crate::music_player::MusicPlayerState::new())
            },
            AppKind::Calendar => AppState::Calendar(crate::calendar::CalendarState::new()),
            AppKind::Notes => AppState::Notes {
                text_lines: vec![String::new()],
                cursor_row: 0,
                cursor_col: 0,
                scroll_v: 0,
                file_path: None,
                is_dirty: false,
                find_mode: false,
                find_query: String::new(),
            },
        }
    }
}

pub fn app_window_size(kind: AppKind) -> (usize, usize) {
    match kind {
        AppKind::FileManager => (500, 400),
        AppKind::Settings => (400, 300),
        AppKind::TextEditor => (500, 380),
        AppKind::SysInfo => (350, 260),
        AppKind::Paint => (420, 320),
        AppKind::TaskManager => (400, 300),
        AppKind::Snake => (440, 340),
        AppKind::Tetris => (440, 480),
        AppKind::Browser => (600, 450),
        AppKind::Shell => (400, 300),
        AppKind::ImageViewer => (520, 400),
        AppKind::SystemMonitor => (450, 380),
        AppKind::MusicPlayer => (400, 380),
        AppKind::Calendar => (320, 340),
        AppKind::Notes => (500, 380),
    }
}

// ─── Rendering ───────────────────────────────────────────────────────────────

pub fn render_app(win: &mut Window, state: &AppState, kind: AppKind) {
    match kind {
        AppKind::FileManager => render_file_manager(win, state),
        AppKind::Settings => render_settings(win, state),
        AppKind::TextEditor => render_text_editor(win, state),
        AppKind::SysInfo => render_sysinfo(win, state),
        AppKind::Paint => render_paint(win, state),
        AppKind::TaskManager => render_task_manager(win, state),
        AppKind::Snake => render_snake(win, state),
        AppKind::Tetris => render_tetris(win, state),
        AppKind::Browser => render_browser(win, state),
        AppKind::Shell => render_sysinfo(win, state),
        AppKind::ImageViewer => {
            if let AppState::ImageViewer(iv) = state {
                crate::image_viewer::render_image_viewer(win, iv);
            }
        },
        AppKind::SystemMonitor => {
            if let AppState::SystemMonitor(sm) = state {
                crate::system_monitor::render_system_monitor(win, sm);
            }
        },
        AppKind::MusicPlayer => {
            if let AppState::MusicPlayer(mp) = state {
                crate::music_player::render_music_player(win, mp);
            }
        },
        AppKind::Calendar => {
            if let AppState::Calendar(cal) = state {
                crate::calendar::render_calendar(win, cal);
            }
        },
        AppKind::Notes => render_notes(win, state), // Mock shell view
    }
}

// ─── Input ───────────────────────────────────────────────────────────────────

pub fn handle_click_app(
    win_id: usize,
    state: &mut AppState,
    kind: AppKind,
    local_x: isize,
    local_y: isize,
) {
    match kind {
        AppKind::FileManager => fm_handle_click(state, local_x, local_y),
        AppKind::Settings => settings_handle_click(state, local_x, local_y),
        AppKind::TextEditor => te_handle_click(state, local_x, local_y),
        AppKind::SysInfo => {},
        AppKind::Paint => paint_handle_click(state, local_x, local_y),
        AppKind::TaskManager => {},
        AppKind::Snake => {},
        AppKind::Tetris => {},
        AppKind::Browser => browser_handle_click(win_id, state, local_x, local_y),
        AppKind::Shell => {},
        AppKind::ImageViewer => {},
        AppKind::SystemMonitor => {},
        AppKind::MusicPlayer => {},
        AppKind::Calendar => {},
        AppKind::Notes => notes_handle_click(state, local_x, local_y),
    }
}

// ─── File Manager ────────────────────────────────────────────────────────────

fn render_file_manager(win: &mut Window, state: &AppState) {
    let (current_dir, selected_item, entries) = match state {
        AppState::FileManager {
            current_dir,
            selected_item,
            entries,
        } => (current_dir, selected_item, entries),
        _ => return,
    };
    let bw = win.width();
    let bh = win.height();
    let buf = &mut win.buffer;

    let theme = crate::theme::get_theme_colors();

    fill_buf(buf, bw, 0, TITLE_BAR_H, bw, bh - TITLE_BAR_H, theme.win_bg);

    // Toolbar
    fill_buf(buf, bw, 0, TITLE_BAR_H, bw, 28, theme.taskbar_border);
    draw_str_buf(
        buf,
        bw,
        bh,
        8,
        TITLE_BAR_H + 6,
        current_dir,
        theme.text_main,
        1,
    );

    // File list area
    let list_y = TITLE_BAR_H + 32;
    let mut y = list_y;

    for (i, name) in entries.iter().enumerate() {
        let bg = if *selected_item == Some(i) {
            theme.start_menu_accent // modern blue highlight
        } else {
            theme.win_bg
        };
        fill_buf(buf, bw, 0, y, bw, 24, bg);

        // Icon square
        if name.ends_with('/') {
            // Folder shape
            let col = Color::from_u32(0xD4AF37);
            fill_buf(buf, bw, 8, y + 6, 14, 10, col); // body
            fill_buf(buf, bw, 8, y + 4, 6, 2, col); // tab
            draw_rect_outline_buf(buf, bw, 8, y + 6, 14, 10, Color::from_u32(0xB89600));
        } else {
            // File shape (page with folded corner)
            let col = Color::from_u32(0xEEEEEE);
            fill_buf(buf, bw, 10, y + 3, 10, 14, col); // paper
                                                       // Cut corner
            fill_buf(buf, bw, 15, y + 3, 5, 5, bg);
            // Draw fold
            fill_buf(buf, bw, 15, y + 8, 5, 1, Color::from_u32(0xAAAAAA));
            fill_buf(buf, bw, 14, y + 4, 1, 4, Color::from_u32(0xAAAAAA));
            draw_rect_outline_buf(buf, bw, 10, y + 3, 10, 14, Color::from_u32(0x999999));
        }

        draw_str_buf(buf, bw, bh, 28, y + 6, name, theme.text_main, 1);

        y += 24;
        if y + 24 >= bh {
            break;
        }
    }
}

fn fm_handle_click(state: &mut AppState, local_x: isize, local_y: isize) {
    let (current_dir, selected_item, entries) = match state {
        AppState::FileManager {
            current_dir,
            selected_item,
            entries,
        } => (current_dir, selected_item, entries),
        _ => return,
    };
    let _lx = local_x as i32;
    let ly = local_y as i32;

    let list_y = TITLE_BAR_H as i32 + 32;
    if ly < list_y {
        return;
    }

    let index = ((ly - list_y) / 24) as usize;
    if index >= entries.len() {
        return;
    }

    *selected_item = Some(index);
    let name = entries[index].clone();
    if name == "../" {
        if *current_dir != "/" {
            let mut parts: Vec<&str> = current_dir.split('/').filter(|s| !s.is_empty()).collect();
            if !parts.is_empty() {
                parts.pop();
            }
            let mut new_dir = String::from("/");
            for p in parts {
                new_dir.push_str(p);
                new_dir.push('/');
            }
            *current_dir = new_dir;
        }
        *entries = read_dir_entries(current_dir);
        *selected_item = None;
        return;
    }
    if name.ends_with('/') {
        // Navigate into folder
        if current_dir.ends_with('/') {
            current_dir.push_str(&name.trim_end_matches('/'));
        } else {
            current_dir.push('/');
            current_dir.push_str(&name.trim_end_matches('/'));
        }
        current_dir.push('/');
        *entries = read_dir_entries(current_dir);
        *selected_item = None;
    }
}

pub fn read_dir_entries(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(list) = crate::vfs::GLOBAL_VFS.lock().read_dir(path) {
        for entry in list {
            let mut name = entry.name.clone();
            if entry.file_type == crate::vfs::FileType::Directory {
                name.push('/');
            }
            out.push(name);
        }
    }
    if path != "/" {
        out.insert(0, String::from("../"));
    }
    out
}

// ─── Settings ────────────────────────────────────────────────────────────────

fn render_settings(win: &mut Window, state: &AppState) {
    let (selected_tab, _bg_color) = match state {
        AppState::Settings {
            selected_tab,
            bg_color,
        } => (selected_tab, bg_color),
        _ => return,
    };
    let bw = win.width();
    let bh = win.height();
    let buf = &mut win.buffer;
    let theme = crate::theme::get_theme_colors();

    fill_buf(buf, bw, 0, TITLE_BAR_H, bw, bh - TITLE_BAR_H, theme.win_bg);

    // Tabs
    let tabs = ["Display", "System", "About"];
    let tab_w = 100;
    for (i, t) in tabs.iter().enumerate() {
        let x = i * tab_w;
        let bg = if *selected_tab == i {
            theme.taskbar_btn_bg_active
        } else {
            theme.taskbar_btn_bg_inactive
        };
        fill_buf(buf, bw, x, TITLE_BAR_H, tab_w, 24, bg);
        draw_str_buf(buf, bw, bh, x + 8, TITLE_BAR_H + 6, t, theme.text_main, 1);
    }

    let body_y = TITLE_BAR_H + 28;
    match *selected_tab {
        0 => {
            draw_str_buf(
                buf,
                bw,
                bh,
                20,
                body_y + 10,
                "Select Theme:",
                theme.text_main,
                1,
            );
            let themes = [
                ("1. Light", 0),
                ("2. Dark", 1),
                ("3. Ocean", 2),
                ("4. Matrix", 3),
            ];
            let mut ty = body_y + 40;
            for (name, _id) in &themes {
                fill_buf(buf, bw, 20, ty, 120, 30, theme.taskbar_bg);
                draw_str_buf(buf, bw, bh, 30, ty + 10, name, theme.text_main, 1);
                ty += 40;
            }
        },
        1 => {
            draw_str_buf(buf, bw, bh, 20, body_y + 10, "System", theme.text_main, 1);
            let (total, free) = crate::system_monitor::memory_stats_mib();
            let mut line = String::from("Memory: ");
            push_u64(&mut line, total as u64);
            line.push_str(" MiB total, ");
            push_u64(&mut line, free as u64);
            line.push_str(" MiB free");
            draw_str_buf(buf, bw, bh, 20, body_y + 32, &line, theme.text_dim, 1);
        },
        _ => {
            draw_str_buf(
                buf,
                bw,
                bh,
                20,
                body_y + 20,
                "RustOS v0.1 - Release Candidate",
                theme.text_main,
                1,
            );
        },
    }
}

fn settings_handle_click(state: &mut AppState, local_x: isize, local_y: isize) {
    let (selected_tab, _bg_color) = match state {
        AppState::Settings {
            selected_tab,
            bg_color,
        } => (selected_tab, bg_color),
        _ => return,
    };
    let lx = local_x as i32;
    let ly = local_y as i32;

    // Tabs
    if ly >= TITLE_BAR_H as i32 && ly < (TITLE_BAR_H + 24) as i32 {
        let tab = (lx / 100) as usize;
        if tab < 3 {
            *selected_tab = tab;
        }
        return;
    }

    // Display swatches disabled
    if *selected_tab == 0 {
        let body_y = TITLE_BAR_H as i32 + 28;
        let mut ty = body_y + 40;
        for i in 0..4 {
            if lx >= 20 && lx <= 140 && ly >= ty && ly <= ty + 30 {
                let current_theme = match i {
                    0 => crate::theme::Theme::Light,
                    1 => crate::theme::Theme::Dark,
                    2 => crate::theme::Theme::Ocean,
                    3 => crate::theme::Theme::Matrix,
                    _ => crate::theme::Theme::Dark,
                };
                unsafe {
                    crate::theme::CURRENT_THEME = current_theme;
                }
                break;
            }
            ty += 40;
        }
    }
}

// ─── Text Editor ─────────────────────────────────────────────────────────────

fn render_text_editor(win: &mut Window, state: &AppState) {
    let (text_lines, cursor_row, cursor_col, scroll_v, file_path, is_dirty) = match state {
        AppState::TextEditor {
            text_lines,
            cursor_row,
            cursor_col,
            scroll_v,
            file_path,
            is_dirty,
        } => (
            text_lines, cursor_row, cursor_col, scroll_v, file_path, is_dirty,
        ),
        _ => return,
    };
    let bw = win.width();
    let bh = win.height();
    let buf = &mut win.buffer;

    // Notepad++ Style: Dark toolbar, Line numbers, Status bar
    let toolbar_h = 28;
    let status_h = 20;
    let line_num_w = 40;

    // Toolbar
    fill_buf(
        buf,
        bw,
        0,
        TITLE_BAR_H,
        bw,
        toolbar_h,
        Color::from_u32(0x3B4252),
    );
    let title = if let Some(path) = file_path {
        let mut s = path.clone();
        if *is_dirty {
            s.push('*');
        }
        s
    } else {
        String::from("New File*")
    };
    draw_str_buf(buf, bw, bh, 10, TITLE_BAR_H + 6, &title, Color::WHITE, 1);
    draw_str_buf(
        buf,
        bw,
        bh,
        bw - 120,
        TITLE_BAR_H + 6,
        "[Save] [Open]",
        Color::from_u32(0x88C0D0),
        1,
    );

    // Editor Area
    let edit_y = TITLE_BAR_H + toolbar_h;
    let edit_h = bh.saturating_sub(edit_y + status_h);
    fill_buf(buf, bw, 0, edit_y, bw, edit_h, Color::from_u32(0x2E3440));

    // Line Numbers gutter
    fill_buf(
        buf,
        bw,
        0,
        edit_y,
        line_num_w,
        edit_h,
        Color::from_u32(0x3B4252),
    );

    let mut y = edit_y + 4;
    let max_rows = edit_h / 16;

    for i in 0..max_rows {
        let line_idx = i + *scroll_v;
        if line_idx >= text_lines.len() {
            break;
        }

        // Line number
        let mut num_str = String::new();
        push_u64(&mut num_str, (line_idx + 1) as u64);
        draw_str_buf(buf, bw, bh, 5, y, &num_str, Color::from_u32(0x4C566A), 1);

        // Text
        draw_str_buf(
            buf,
            bw,
            bh,
            line_num_w + 5,
            y,
            &text_lines[line_idx],
            Color::from_u32(0xD8DEE9),
            1,
        );

        if line_idx == *cursor_row {
            // Cursor
            if crate::gui::CLOCK_TICKS.load(core::sync::atomic::Ordering::Relaxed) % 30 < 15 {
                let cx = line_num_w + 5 + *cursor_col * 8;
                if cx < bw - 8 {
                    fill_buf(buf, bw, cx, y, 2, 14, Color::from_u32(0x88C0D0));
                }
            }
        }
        y += 16;
    }

    // Status Bar
    let sb_y = bh - status_h;
    fill_buf(buf, bw, 0, sb_y, bw, status_h, Color::from_u32(0x3B4252));
    let mut status = String::from("Ln ");
    push_u64(&mut status, (*cursor_row + 1) as u64);
    status.push_str(", Col ");
    push_u64(&mut status, (*cursor_col + 1) as u64);
    draw_str_buf(
        buf,
        bw,
        bh,
        10,
        sb_y + 4,
        &status,
        Color::from_u32(0xD8DEE9),
        1,
    );
}

pub fn te_handle_keyboard(state: &mut AppState, c: char) {
    let (text_lines, cursor_row, cursor_col, scroll_v, _file_path, is_dirty) = match state {
        AppState::TextEditor {
            text_lines,
            cursor_row,
            cursor_col,
            scroll_v,
            file_path,
            is_dirty,
        } => (
            text_lines, cursor_row, cursor_col, scroll_v, file_path, is_dirty,
        ),
        _ => return,
    };
    if *cursor_row >= text_lines.len() {
        return;
    }

    *is_dirty = true;

    if c == '\x08' {
        // Backspace
        if *cursor_col > 0 {
            *cursor_col -= 1;
            let col = *cursor_col;
            text_lines[*cursor_row].remove(col);
        } else if *cursor_row > 0 {
            let current = text_lines.remove(*cursor_row);
            *cursor_row -= 1;
            *cursor_col = text_lines[*cursor_row].len();
            text_lines[*cursor_row].push_str(&current);
            // Adjust scroll if needed
            if *cursor_row < *scroll_v {
                *scroll_v = cursor_row.saturating_sub(5);
            }
        }
    } else if c == '\n' {
        let current_line = &mut text_lines[*cursor_row];
        let split_idx = (*cursor_col).min(current_line.len());
        let new_line = current_line.split_off(split_idx);
        *cursor_row += 1;
        *cursor_col = 0;
        text_lines.insert(*cursor_row, new_line);
        // Auto-scroll
        let visible_rows = 20; // estimate
        if *cursor_row >= *scroll_v + visible_rows {
            *scroll_v += 1;
        }
    } else if (c as u32) >= 32 && (c as u32) < 127 {
        if *cursor_col <= text_lines[*cursor_row].len() {
            text_lines[*cursor_row].insert(*cursor_col, c);
            *cursor_col += 1;
        }
    }
}

fn te_handle_click(state: &mut AppState, local_x: isize, local_y: isize) {
    let (text_lines, cursor_row, cursor_col, scroll_v, file_path, is_dirty) = match state {
        AppState::TextEditor {
            text_lines,
            cursor_row,
            cursor_col,
            scroll_v,
            file_path,
            is_dirty,
        } => (
            text_lines, cursor_row, cursor_col, scroll_v, file_path, is_dirty,
        ),
        _ => return,
    };
    let lx = local_x as i32;
    let ly = local_y as i32;

    // Toolbar clicks
    if ly >= TITLE_BAR_H as i32 && ly < (TITLE_BAR_H + 28) as i32 {
        let bw = 500; // default app width
        if lx > bw - 120 && lx < bw - 70 {
            // [Save]
            te_save_file(text_lines, file_path, is_dirty);
        } else if lx > bw - 60 && lx < bw - 10 {
            // [Open]
            te_load_file(text_lines, file_path, is_dirty, cursor_row, cursor_col);
        }
        return;
    }

    let line_num_w = 40;
    let edit_y = (TITLE_BAR_H + 28) as i32;
    let row_diff = ly - edit_y - 4;
    if row_diff < 0 {
        return;
    }

    let row_idx = (row_diff / 16) as usize + *scroll_v;
    if row_idx < text_lines.len() {
        *cursor_row = row_idx;
        let cx = lx - line_num_w - 5;
        let col = if cx < 0 { 0 } else { (cx / 8) as usize };
        *cursor_col = col.min(text_lines[*cursor_row].len());
    }
}

fn render_notes(win: &mut Window, state: &AppState) {
    let (text_lines, cursor_row, _cursor_col, scroll_v, _file_path, _is_dirty) = match state {
        AppState::Notes {
            text_lines,
            cursor_row,
            cursor_col,
            scroll_v,
            file_path,
            is_dirty,
            ..
        } => (
            text_lines, cursor_row, cursor_col, scroll_v, file_path, is_dirty,
        ),
        _ => return,
    };
    let bw = win.width();
    let bh = win.height();
    let buf = &mut win.buffer;

    // Background
    fill_buf(
        buf,
        bw,
        0,
        TITLE_BAR_H,
        bw,
        bh - TITLE_BAR_H,
        crate::graphics::Color::from_u32(0x1E2227),
    );

    // Text content
    let mut y = TITLE_BAR_H + 8;
    let max_rows = (bh - TITLE_BAR_H - 16) / 16;
    let start_row = *scroll_v;

    for i in 0..max_rows {
        let line_idx = i + start_row;
        if line_idx >= text_lines.len() {
            break;
        }
        let line = &text_lines[line_idx];
        let color = if i == *cursor_row - start_row {
            crate::graphics::Color::from_u32(0x7AA2F7)
        } else {
            crate::graphics::Color::from_u32(0xC0CAF5)
        };
        draw_str_buf(buf, bw, bh, 8, y, line, color, 1);
        y += 16;
    }
}

fn notes_handle_click(_state: &mut AppState, _local_x: isize, local_y: isize) {
    let (text_lines, cursor_row, cursor_col, scroll_v, _file_path, _is_dirty) = match _state {
        AppState::Notes {
            text_lines,
            cursor_row,
            cursor_col,
            scroll_v,
            file_path,
            is_dirty,
            ..
        } => (
            text_lines, cursor_row, cursor_col, scroll_v, file_path, is_dirty,
        ),
        _ => return,
    };
    let ly = local_y as i32;
    let edit_y = TITLE_BAR_H as i32;
    let row_diff = ly - edit_y - 4;
    if row_diff < 0 {
        return;
    }

    let row_idx = (row_diff / 16) as usize + *scroll_v;
    if row_idx < text_lines.len() {
        *cursor_row = row_idx;
        *cursor_col = 0;
    }
}

fn te_save_file(text_lines: &Vec<String>, file_path: &mut Option<String>, is_dirty: &mut bool) {
    let path = file_path.as_deref().unwrap_or("/home/note.txt");
    let mut content = String::new();
    for line in text_lines {
        content.push_str(line);
        content.push('\n');
    }
    let _ = crate::vfs::GLOBAL_VFS
        .lock()
        .write_file(path, content.as_bytes());
    if file_path.is_none() {
        *file_path = Some(String::from(path));
    }
    *is_dirty = false;
}

fn te_load_file(
    text_lines: &mut Vec<String>,
    file_path: &mut Option<String>,
    is_dirty: &mut bool,
    row: &mut usize,
    col: &mut usize,
) {
    let path = "/home/note.txt"; // Hardcoded for now in MVP
    if let Ok(data) = crate::vfs::GLOBAL_VFS.lock().read_file(path) {
        if let Ok(content) = String::from_utf8(data) {
            text_lines.clear();
            for line in content.lines() {
                text_lines.push(String::from(line));
            }
            if text_lines.is_empty() {
                text_lines.push(String::new());
            }
            *file_path = Some(String::from(path));
            *is_dirty = false;
            *row = 0;
            *col = 0;
        }
    }
}

fn render_sysinfo(win: &mut Window, _state: &AppState) {
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
        Color::from_u32(0x1E1E2E),
    );

    let (total, free) = memory_stats_mib();
    let uptime = crate::gui::CLOCK_TICKS.load(core::sync::atomic::Ordering::Relaxed) / 18;
    let pci_count = crate::pci::PCI_DEVICES.lock().len();

    // Security info
    let kaslr_status = if crate::kaslr::is_kaslr_enforced() {
        "Enforced"
    } else {
        "Prototype"
    };
    let canary_status = if crate::stack_canary::is_enforced() {
        "Enforced"
    } else {
        "Prototype"
    };
    let security_status_color = Color::from_u32(0xF9E2AF);

    // Draw stylized OS Logo on the left (scale=2)
    let logo_color = Color::from_u32(0xE57324);
    let lx = 20;
    let ly = TITLE_BAR_H + 30;
    draw_str_buf(buf, bw, bh, lx, ly, "  /-\\  ", logo_color, 2);
    draw_str_buf(buf, bw, bh, lx, ly + 16, " / R \\ ", logo_color, 2);
    draw_str_buf(buf, bw, bh, lx, ly + 32, " \\_- / ", logo_color, 2);
    draw_str_buf(buf, bw, bh, lx, ly + 48, "   v   ", logo_color, 2);

    // Draw Dashboard Info on the right
    let tx = 140;
    let mut ty = TITLE_BAR_H + 20;

    draw_str_buf(
        buf,
        bw,
        bh,
        tx,
        ty,
        "guest\x1b@\x1brustos",
        Color::from_u32(0x8899BB),
        1,
    ); // Mock color parsing fallback if not supported, but let's just use raw strings
    draw_str_buf(buf, bw, bh, tx, ty, "guest", Color::from_u32(0x55FF55), 1);
    draw_str_buf(buf, bw, bh, tx + 40, ty, "@", Color::WHITE, 1);
    draw_str_buf(
        buf,
        bw,
        bh,
        tx + 48,
        ty,
        "rust-os",
        Color::from_u32(0x55FFFF),
        1,
    );

    ty += 16;
    fill_buf(buf, bw, tx, ty, 160, 1, Color::from_u32(0x4A5570));
    ty += 8;

    draw_str_buf(buf, bw, bh, tx, ty, "OS:", Color::from_u32(0x8899BB), 1);
    draw_str_buf(buf, bw, bh, tx + 80, ty, "RustOS v0.1.1", Color::WHITE, 1);
    ty += 16;

    draw_str_buf(buf, bw, bh, tx, ty, "Host:", Color::from_u32(0x8899BB), 1);
    draw_str_buf(buf, bw, bh, tx + 80, ty, "x86_64 QEMU/PC", Color::WHITE, 1);
    ty += 16;

    let mut up = String::from("");
    push_u64(&mut up, uptime);
    up.push_str(" secs");
    draw_str_buf(buf, bw, bh, tx, ty, "Uptime:", Color::from_u32(0x8899BB), 1);
    draw_str_buf(buf, bw, bh, tx + 80, ty, &up, Color::WHITE, 1);
    ty += 16;

    let mut mem = String::from("");
    let used = total.saturating_sub(free);
    push_u64(&mut mem, used as u64);
    mem.push_str("MiB / ");
    push_u64(&mut mem, total as u64);
    mem.push_str("MiB");

    draw_str_buf(buf, bw, bh, tx, ty, "Memory:", Color::from_u32(0x8899BB), 1);
    draw_str_buf(buf, bw, bh, tx + 80, ty, &mem, Color::WHITE, 1);
    ty += 12;

    // Memory Progress Bar
    let bar_w = 160;
    let filled_w = if total > 0 {
        (used as usize * bar_w) / total as usize
    } else {
        0
    };
    fill_buf(buf, bw, tx, ty, bar_w, 8, Color::from_u32(0x313244));
    fill_buf(buf, bw, tx, ty, filled_w, 8, Color::from_u32(0x89B4FA));
    draw_rect_outline_buf(buf, bw, tx, ty, bar_w, 8, Color::from_u32(0x45475A));
    ty += 16;

    // Security Section
    draw_str_buf(buf, bw, bh, tx, ty, "KASLR:", Color::from_u32(0x8899BB), 1);
    draw_str_buf(
        buf,
        bw,
        bh,
        tx + 80,
        ty,
        kaslr_status,
        security_status_color,
        1,
    );
    ty += 16;

    draw_str_buf(buf, bw, bh, tx, ty, "Canary:", Color::from_u32(0x8899BB), 1);
    draw_str_buf(
        buf,
        bw,
        bh,
        tx + 80,
        ty,
        canary_status,
        security_status_color,
        1,
    );
    ty += 16;

    let mut pci = String::new();
    push_u64(&mut pci, pci_count as u64);
    draw_str_buf(
        buf,
        bw,
        bh,
        tx,
        ty,
        "PCI Devs:",
        Color::from_u32(0x8899BB),
        1,
    );
    draw_str_buf(buf, bw, bh, tx + 80, ty, &pci, Color::WHITE, 1);

    // Bottom color blocks (Classic Neofetch style)
    let b_y = bh.saturating_sub(30);
    fill_buf(buf, bw, tx, b_y, 20, 10, Color::from_u32(0x222222));
    fill_buf(buf, bw, tx + 20, b_y, 20, 10, Color::RED);
    fill_buf(buf, bw, tx + 40, b_y, 20, 10, Color::GREEN);
    fill_buf(buf, bw, tx + 60, b_y, 20, 10, Color::YELLOW);
    fill_buf(buf, bw, tx + 80, b_y, 20, 10, Color::BLUE);
    fill_buf(buf, bw, tx + 100, b_y, 20, 10, Color::MAGENTA);
    fill_buf(buf, bw, tx + 120, b_y, 20, 10, Color::CYAN);
    fill_buf(buf, bw, tx + 140, b_y, 20, 10, Color::WHITE);
}

// ─── Task Manager ────────────────────────────────────────────────────────────

fn render_task_manager(win: &mut Window, _state: &AppState) {
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
        Color::from_u32(0x1E2233),
    );

    // --- GUI Windows Section ---
    fill_buf(buf, bw, 0, TITLE_BAR_H, bw, 24, Color::from_u32(0x2E3440));
    draw_str_buf(
        buf,
        bw,
        bh,
        10,
        TITLE_BAR_H + 6,
        "GUI Windows             X      Y      W      H",
        Color::WHITE,
        1,
    );

    let mut y = TITLE_BAR_H + 24;
    let windows = &crate::gui::COMPOSITOR.lock().windows;

    for win_arc in windows {
        let w = win_arc.lock();
        if w.closed {
            continue;
        }
        let bg = if w.focused {
            Color::from_u32(0x3B5FA8)
        } else {
            Color::from_u32(0x1E2233)
        };
        fill_buf(buf, bw, 0, y, bw, 18, bg);
        draw_str_buf(buf, bw, bh, 10, y + 2, &w.title, Color::WHITE, 1);

        // Coordinates and size
        let mut stats = String::new();
        push_i32(&mut stats, w.x as i32);
        stats.push_str("    ");
        push_i32(&mut stats, w.y as i32);
        stats.push_str("    ");
        push_u64(&mut stats, w.width() as u64);
        stats.push_str("    ");
        push_u64(&mut stats, w.height() as u64);
        draw_str_buf(
            buf,
            bw,
            bh,
            180,
            y + 2,
            &stats,
            Color::from_u32(0x8899BB),
            1,
        );
        y += 18;
        if y + 18 > bh {
            break;
        }
    }

    // --- Kernel Tasks Section ---
    if y + 40 < bh {
        fill_buf(buf, bw, 0, y, bw, 24, Color::from_u32(0x2E3440));
        draw_str_buf(
            buf,
            bw,
            bh,
            10,
            y + 6,
            "Kernel Tasks (CFS prototype) ID   State    VRuntime",
            Color::WHITE,
            1,
        );
        y += 24;

        let sched_tasks = crate::scheduler::SCHEDULER.lock().dump_tasks();
        for (tid, state, vrun, _nice) in sched_tasks {
            let bg = Color::from_u32(0x1E2233);
            fill_buf(buf, bw, 0, y, bw, 18, bg);

            let mut line = String::from("Task ");
            push_u64(&mut line, tid);
            draw_str_buf(
                buf,
                bw,
                bh,
                10,
                y + 2,
                &line,
                Color::from_rgb(0x00, 0xFF, 0x00),
                1,
            );

            let state_str = match state {
                crate::scheduler::TaskState::Running => "Running",
                crate::scheduler::TaskState::Ready => "Ready",
                crate::scheduler::TaskState::Blocked => "Blocked",
                crate::scheduler::TaskState::Zombie => "Zombie",
            };
            draw_str_buf(buf, bw, bh, 150, y + 2, state_str, Color::WHITE, 1);

            let mut vrun_str = String::new();
            push_u64(&mut vrun_str, vrun);
            draw_str_buf(
                buf,
                bw,
                bh,
                240,
                y + 2,
                &vrun_str,
                Color::from_u32(0x8899BB),
                1,
            );

            y += 18;
            if y + 18 > bh {
                break;
            }
        }
    }

    // --- Async Tasks (Executor) ---
    if y + 40 < bh {
        fill_buf(buf, bw, 0, y, bw, 24, Color::from_u32(0x2E3440));
        draw_str_buf(
            buf,
            bw,
            bh,
            10,
            y + 6,
            "Async Tasks           TID      Status",
            Color::WHITE,
            1,
        );
        y += 24;

        let tasks = crate::task::TASKS_INFO.lock();
        for (id, name) in tasks.iter() {
            let bg = Color::from_u32(0x1E2233);
            fill_buf(buf, bw, 0, y, bw, 18, bg);
            let mut id_str = String::new();
            push_u64(&mut id_str, *id as u64);

            draw_str_buf(
                buf,
                bw,
                bh,
                10,
                y + 2,
                name,
                Color::from_rgb(0x00, 0xFF, 0x00),
                1,
            );
            draw_str_buf(buf, bw, bh, 180, y + 2, &id_str, Color::WHITE, 1);
            draw_str_buf(
                buf,
                bw,
                bh,
                240,
                y + 2,
                "Running",
                Color::from_u32(0x55FF55),
                1,
            );

            y += 18;
            if y + 18 > bh {
                break;
            }
        }
    }
}

// ─── Paint ───────────────────────────────────────────────────────────────────

fn render_paint(win: &mut Window, state: &AppState) {
    let (paint_color, canvas, canvas_w, canvas_h) = match state {
        AppState::Paint {
            paint_color,
            canvas,
            canvas_w,
            canvas_h,
        } => (paint_color, canvas, canvas_w, canvas_h),
        _ => return,
    };
    let bw = win.width();
    let bh = win.height();
    let buf = &mut win.buffer;

    // Toolbar
    fill_buf(buf, bw, 0, TITLE_BAR_H, bw, 28, Color::from_u32(0x2E3440));
    // Colors: red/green/blue
    let colors = [Color::RED, Color::GREEN, Color::BLUE];
    for (i, c) in colors.iter().enumerate() {
        let x = 10 + i * 28;
        fill_buf(buf, bw, x, TITLE_BAR_H + 6, 18, 18, *c);
        if *c == *paint_color {
            draw_rect_outline_buf(buf, bw, x, TITLE_BAR_H + 6, 18, 18, Color::WHITE);
        }
    }
    // Clear button
    fill_buf(
        buf,
        bw,
        120,
        TITLE_BAR_H + 6,
        40,
        18,
        Color::from_u32(0x4A5570),
    );
    draw_str_buf(buf, bw, bh, 126, TITLE_BAR_H + 9, "CLR", Color::WHITE, 1);

    // Canvas
    let canvas_x = 10;
    let canvas_y = TITLE_BAR_H + 36;
    let canvas_width = (*canvas_w).min(bw.saturating_sub(20));
    let canvas_height = (*canvas_h).min(bh.saturating_sub(canvas_y + 10));
    fill_buf(
        buf,
        bw,
        canvas_x,
        canvas_y,
        canvas_width,
        canvas_height,
        Color::WHITE,
    );

    for y in 0..canvas_height {
        for x in 0..canvas_width {
            let src = y * *canvas_w + x;
            let dst = (canvas_y + y) * bw + (canvas_x + x);
            if src < canvas.len() && dst < buf.len() {
                buf[dst] = canvas[src];
            }
        }
    }

    draw_rect_outline_buf(
        buf,
        bw,
        canvas_x,
        canvas_y,
        canvas_width,
        canvas_height,
        Color::from_u32(0x4A5570),
    );
}

fn paint_handle_click(state: &mut AppState, local_x: isize, local_y: isize) {
    let (paint_color, canvas, canvas_w, canvas_h) = match state {
        AppState::Paint {
            paint_color,
            canvas,
            canvas_w,
            canvas_h,
        } => (paint_color, canvas, canvas_w, canvas_h),
        _ => return,
    };
    let lx = local_x as i32;
    let ly = local_y as i32;

    // Color buttons
    if ly >= (TITLE_BAR_H as i32 + 6) && ly < (TITLE_BAR_H as i32 + 24) {
        if lx >= 10 && lx < 28 {
            *paint_color = Color::RED;
        }
        if lx >= 38 && lx < 56 {
            *paint_color = Color::GREEN;
        }
        if lx >= 66 && lx < 84 {
            *paint_color = Color::BLUE;
        }
        if lx >= 120 && lx < 160 {
            // Clear
            for p in canvas.iter_mut() {
                *p = Color::WHITE;
            }
        }
        return;
    }

    let canvas_x = 10;
    let canvas_y = TITLE_BAR_H as i32 + 36;
    let cx = lx - canvas_x as i32;
    let cy = ly - canvas_y;
    if cx >= 0 && cy >= 0 {
        let cx = cx as usize;
        let cy = cy as usize;
        if cx < *canvas_w && cy < *canvas_h {
            let idx = cy * *canvas_w + cx;
            if idx < canvas.len() {
                canvas[idx] = *paint_color;
            }
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn memory_stats_mib() -> (usize, usize) {
    let alloc = crate::frame_allocator::get_frame_allocator();
    let total_mib =
        alloc.total_frames() * (crate::frame_allocator::FRAME_SIZE as usize) / (1024 * 1024);
    let free_mib = (alloc.free_memory_bytes() / (1024 * 1024)) as usize;
    (total_mib, free_mib)
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

fn push_i32(s: &mut String, mut v: i32) {
    if v < 0 {
        s.push('-');
        v = -v;
    }
    push_u64(s, v as u64);
}

pub(crate) fn fill_buf(
    buf: &mut [Color],
    bw: usize,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    color: Color,
) {
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
        let i = y * bw + col;
        if i < buf.len() {
            buf[i] = color;
        }
    }
    if y + h > 0 {
        let row = y + h - 1;
        for col in x..(x + w).min(bw) {
            let i = row * bw + col;
            if i < buf.len() {
                buf[i] = color;
            }
        }
    }
    for row in y..(y + h).min(bh) {
        let i = row * bw + x;
        if i < buf.len() {
            buf[i] = color;
        }
    }
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

pub fn snake_handle_keyboard(state: &mut AppState, c: char) {
    let (snake_body, snake_dir, snake_score, snake_game_over) = match state {
        AppState::Snake {
            snake_body,
            snake_dir,
            snake_score,
            snake_game_over,
            ..
        } => (snake_body, snake_dir, snake_score, snake_game_over),
        _ => return,
    };
    if *snake_game_over {
        if c == 'r' || c == 'R' {
            *snake_body = alloc::vec![(10, 10), (10, 11), (10, 12)];
            *snake_dir = (0, -1);
            *snake_score = 0;
            *snake_game_over = false;
        }
        return;
    }
    match c {
        'w' | 'W' => {
            if snake_dir.1 != 1 {
                *snake_dir = (0, -1)
            }
        },
        's' | 'S' => {
            if snake_dir.1 != -1 {
                *snake_dir = (0, 1)
            }
        },
        'a' | 'A' => {
            if snake_dir.0 != 1 {
                *snake_dir = (-1, 0)
            }
        },
        'd' | 'D' => {
            if snake_dir.0 != -1 {
                *snake_dir = (1, 0)
            }
        },
        _ => {},
    }
}

pub fn step_snake(state: &mut AppState, current_tick: u64) -> bool {
    let (snake_body, snake_dir, snake_fruit, snake_score, snake_game_over, snake_last_tick) =
        match state {
            AppState::Snake {
                snake_body,
                snake_dir,
                snake_fruit,
                snake_score,
                snake_game_over,
                snake_last_tick,
            } => (
                snake_body,
                snake_dir,
                snake_fruit,
                snake_score,
                snake_game_over,
                snake_last_tick,
            ),
            _ => return false,
        };
    if *snake_game_over {
        return false;
    }
    if current_tick.saturating_sub(*snake_last_tick) < 3 {
        return false;
    }
    *snake_last_tick = current_tick;
    let head = snake_body[0];
    let next = (head.0 + snake_dir.0, head.1 + snake_dir.1);

    // Grid: 28x20 cells of 15x15 px (420x300 px play field)
    if next.0 < 0 || next.1 < 0 || next.0 >= 28 || next.1 >= 20 {
        *snake_game_over = true;
        return true;
    }
    if snake_body.contains(&next) {
        *snake_game_over = true;
        return true;
    }

    snake_body.insert(0, next);
    if next == *snake_fruit {
        *snake_score += 10;
        let fx = ((current_tick * 13) % 28) as i32;
        let fy = ((current_tick * 17) % 20) as i32;
        *snake_fruit = (fx, fy);
    } else {
        snake_body.pop();
    }
    true
}

fn render_snake(win: &mut Window, state: &AppState) {
    let (snake_body, snake_fruit, snake_score, snake_game_over) = match state {
        AppState::Snake {
            snake_body,
            snake_fruit,
            snake_score,
            snake_game_over,
            ..
        } => (snake_body, snake_fruit, snake_score, snake_game_over),
        _ => return,
    };
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
        Color::from_u32(0x1E2233),
    );

    let play_x = 10;
    let play_y = TITLE_BAR_H + 10;
    let cell_s = 15;

    // Draw boundary
    draw_rect_outline_buf(
        buf,
        bw,
        play_x - 1,
        play_y - 1,
        28 * cell_s + 2,
        20 * cell_s + 2,
        Color::WHITE,
    );

    // Fruit
    fill_buf(
        buf,
        bw,
        play_x + snake_fruit.0 as usize * cell_s,
        play_y + snake_fruit.1 as usize * cell_s,
        cell_s,
        cell_s,
        Color::from_u32(0xFF3333),
    );

    // Snake
    for (i, p) in snake_body.iter().enumerate() {
        let scol = if i == 0 {
            Color::from_u32(0x33FF33)
        } else {
            Color::from_u32(0x22AA22)
        };
        fill_buf(
            buf,
            bw,
            play_x + p.0 as usize * cell_s,
            play_y + p.1 as usize * cell_s,
            cell_s,
            cell_s,
            scol,
        );
    }

    // Score
    let mut score_str = String::from("Score: ");
    push_u64(&mut score_str, *snake_score as u64);
    draw_str_buf(
        buf,
        bw,
        bh,
        play_x + 10,
        play_y + 20 * cell_s + 5,
        &score_str,
        Color::WHITE,
        1,
    );

    if *snake_game_over {
        draw_str_buf(
            buf,
            bw,
            bh,
            play_x + 80,
            play_y + 120,
            "GAME OVER! Press 'R'",
            Color::WHITE,
            2,
        );
    }
}

// ─── Tetris ──────────────────────────────────────────────────────────────────

const TETROMINOES: [[[u8; 16]; 4]; 7] = [
    // I: Cyan (1)
    [
        [0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0],
        [0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0],
        [0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0],
        [0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0],
    ],
    // J: Blue (2)
    [
        [1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        [0, 1, 1, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0],
        [0, 1, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0],
    ],
    // L: Orange (3)
    [
        [0, 0, 1, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        [0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 1, 1, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0],
        [1, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0],
    ],
    // O: Yellow (4)
    [
        [0, 1, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        [0, 1, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        [0, 1, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        [0, 1, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    ],
    // S: Green (5)
    [
        [0, 1, 1, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        [0, 1, 0, 0, 0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0, 1, 1, 0, 1, 1, 0, 0, 0, 0, 0, 0],
        [1, 0, 0, 0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0],
    ],
    // T: Purple (6)
    [
        [0, 1, 0, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        [0, 1, 0, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 1, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0],
        [0, 1, 0, 0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0],
    ],
    // Z: Red (7)
    [
        [1, 1, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        [0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 1, 1, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0],
        [0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0],
    ],
];

fn tetris_collides(board: &[[u8; 10]; 20], p: u8, r: u8, tx: i32, ty: i32) -> bool {
    let shape = &TETROMINOES[p as usize][(r % 4) as usize];
    for y in 0..4 {
        for x in 0..4 {
            if shape[y * 4 + x] == 1 {
                let bx = tx + x as i32;
                let by = ty + y as i32;
                if bx < 0 || bx >= 10 || by >= 20 {
                    return true;
                }
                if by >= 0 && board[by as usize][bx as usize] != 0 {
                    return true;
                }
            }
        }
    }
    false
}

fn tetris_spawn(state: &mut AppState) {
    let (
        tetris_board,
        tetris_piece,
        tetris_piece_x,
        tetris_piece_y,
        tetris_piece_rot,
        tetris_game_over,
    ) = match state {
        AppState::Tetris {
            tetris_board,
            tetris_piece,
            tetris_piece_x,
            tetris_piece_y,
            tetris_piece_rot,
            tetris_game_over,
            ..
        } => (
            tetris_board,
            tetris_piece,
            tetris_piece_x,
            tetris_piece_y,
            tetris_piece_rot,
            tetris_game_over,
        ),
        _ => return,
    };
    let seed = crate::gui::CLOCK_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u8;
    *tetris_piece = seed % 7;
    *tetris_piece_rot = 0;
    *tetris_piece_x = 3;
    *tetris_piece_y = -3;
    if tetris_collides(
        tetris_board,
        *tetris_piece,
        *tetris_piece_rot,
        *tetris_piece_x,
        0,
    ) {
        *tetris_game_over = true;
    } else {
        *tetris_piece_y = 0;
    }
}

fn tetris_merge(state: &mut AppState) {
    let (tetris_board, tetris_piece, tetris_piece_x, tetris_piece_y, tetris_piece_rot) = match state
    {
        AppState::Tetris {
            tetris_board,
            tetris_piece,
            tetris_piece_x,
            tetris_piece_y,
            tetris_piece_rot,
            ..
        } => (
            tetris_board,
            tetris_piece,
            tetris_piece_x,
            tetris_piece_y,
            tetris_piece_rot,
        ),
        _ => return,
    };
    let shape = &TETROMINOES[*tetris_piece as usize][(*tetris_piece_rot % 4) as usize];
    for y in 0..4 {
        for x in 0..4 {
            if shape[y * 4 + x] == 1 {
                let bx = *tetris_piece_x + x as i32;
                let by = *tetris_piece_y + y as i32;
                if bx >= 0 && bx < 10 && by >= 0 && by < 20 {
                    tetris_board[by as usize][bx as usize] = *tetris_piece + 1;
                }
            }
        }
    }
}

fn tetris_clear_lines(state: &mut AppState) {
    let (tetris_board, tetris_score) = match state {
        AppState::Tetris {
            tetris_board,
            tetris_score,
            ..
        } => (tetris_board, tetris_score),
        _ => return,
    };
    let mut y = 19_i32;
    let mut lines_cleared = 0;
    while y >= 0 {
        let mut full = true;
        for x in 0..10 {
            if tetris_board[y as usize][x] == 0 {
                full = false;
                break;
            }
        }
        if full {
            lines_cleared += 1;
            for yy in (1..=y as usize).rev() {
                for x in 0..10 {
                    tetris_board[yy][x] = tetris_board[yy - 1][x];
                }
            }
            for x in 0..10 {
                tetris_board[0][x] = 0;
            }
        } else {
            y -= 1;
        }
    }
    match lines_cleared {
        1 => *tetris_score += 100,
        2 => *tetris_score += 300,
        3 => *tetris_score += 500,
        4 => *tetris_score += 800,
        _ => {},
    }
}

pub fn step_tetris(state: &mut AppState, current_tick: u64) -> bool {
    let mut do_merge = false;
    let mut do_dropdown = false;
    if let AppState::Tetris {
        tetris_game_over,
        tetris_score,
        tetris_last_tick,
        tetris_board,
        tetris_piece,
        tetris_piece_rot,
        tetris_piece_x,
        tetris_piece_y,
        ..
    } = state
    {
        if *tetris_game_over {
            return false;
        }
        let speed = 9u64.saturating_sub((*tetris_score / 1000) as u64).max(2);
        if current_tick.saturating_sub(*tetris_last_tick) < speed {
            return false;
        }
        *tetris_last_tick = current_tick;

        if tetris_collides(
            tetris_board,
            *tetris_piece,
            *tetris_piece_rot,
            *tetris_piece_x,
            *tetris_piece_y + 1,
        ) {
            do_merge = true;
        } else {
            do_dropdown = true;
        }
    } else {
        return false;
    }

    if do_merge {
        tetris_merge(state);
        tetris_clear_lines(state);
        tetris_spawn(state);
    } else if do_dropdown {
        if let AppState::Tetris {
            tetris_piece_y, ..
        } = state
        {
            *tetris_piece_y += 1;
        }
    }
    true
}

pub fn tetris_handle_keyboard(state: &mut AppState, c: char) {
    let mut do_hard_drop = false;
    let mut do_respawn = false;
    if let AppState::Tetris {
        tetris_game_over,
        tetris_board,
        tetris_score,
        tetris_piece,
        tetris_piece_rot,
        tetris_piece_x,
        tetris_piece_y,
        ..
    } = state
    {
        if *tetris_game_over {
            if c == 'r' || c == 'R' {
                *tetris_board = [[0; 10]; 20];
                *tetris_score = 0;
                *tetris_game_over = false;
                do_respawn = true;
            }
        } else {
            match c {
                'w' | 'W' => {
                    let next_r = *tetris_piece_rot + 1;
                    if !tetris_collides(
                        tetris_board,
                        *tetris_piece,
                        next_r,
                        *tetris_piece_x,
                        *tetris_piece_y,
                    ) {
                        *tetris_piece_rot = next_r;
                    }
                },
                'a' | 'A' => {
                    if !tetris_collides(
                        tetris_board,
                        *tetris_piece,
                        *tetris_piece_rot,
                        *tetris_piece_x - 1,
                        *tetris_piece_y,
                    ) {
                        *tetris_piece_x -= 1;
                    }
                },
                'd' | 'D' => {
                    if !tetris_collides(
                        tetris_board,
                        *tetris_piece,
                        *tetris_piece_rot,
                        *tetris_piece_x + 1,
                        *tetris_piece_y,
                    ) {
                        *tetris_piece_x += 1;
                    }
                },
                's' | 'S' => {
                    if !tetris_collides(
                        tetris_board,
                        *tetris_piece,
                        *tetris_piece_rot,
                        *tetris_piece_x,
                        *tetris_piece_y + 1,
                    ) {
                        *tetris_piece_y += 1;
                    }
                },
                ' ' => {
                    while !tetris_collides(
                        tetris_board,
                        *tetris_piece,
                        *tetris_piece_rot,
                        *tetris_piece_x,
                        *tetris_piece_y + 1,
                    ) {
                        *tetris_piece_y += 1;
                    }
                    do_hard_drop = true;
                },
                _ => {},
            }
        }
    }

    if do_respawn {
        tetris_spawn(state);
    } else if do_hard_drop {
        tetris_merge(state);
        tetris_clear_lines(state);
        tetris_spawn(state);
    }
}

fn get_tetris_color(idx: u8) -> Color {
    match idx {
        1 => Color::from_u32(0x00FFFF),
        2 => Color::from_u32(0x0000FF),
        3 => Color::from_u32(0xFFA500),
        4 => Color::from_u32(0xFFFF00),
        5 => Color::from_u32(0x00FF00),
        6 => Color::from_u32(0x800080),
        7 => Color::from_u32(0xFF0000),
        _ => Color::from_u32(0x222222),
    }
}

fn render_tetris(win: &mut Window, state: &AppState) {
    let (
        tetris_board,
        tetris_piece,
        tetris_piece_x,
        tetris_piece_y,
        tetris_piece_rot,
        tetris_score,
        tetris_game_over,
    ) = match state {
        AppState::Tetris {
            tetris_board,
            tetris_piece,
            tetris_piece_x,
            tetris_piece_y,
            tetris_piece_rot,
            tetris_score,
            tetris_game_over,
            ..
        } => (
            tetris_board,
            tetris_piece,
            tetris_piece_x,
            tetris_piece_y,
            tetris_piece_rot,
            tetris_score,
            tetris_game_over,
        ),
        _ => return,
    };
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
        Color::from_u32(0x1E2233),
    );

    let play_x = 20;
    let play_y = TITLE_BAR_H + 10;
    let cell_s = 20;

    draw_rect_outline_buf(
        buf,
        bw,
        play_x - 1,
        play_y - 1,
        10 * cell_s + 2,
        20 * cell_s + 2,
        Color::WHITE,
    );

    for y in 0..20 {
        for x in 0..10 {
            let col = tetris_board[y][x];
            if col != 0 {
                fill_buf(
                    buf,
                    bw,
                    play_x + x * cell_s + 1,
                    play_y + y * cell_s + 1,
                    cell_s - 2,
                    cell_s - 2,
                    get_tetris_color(col),
                );
            } else {
                draw_rect_outline_buf(
                    buf,
                    bw,
                    play_x + x * cell_s,
                    play_y + y * cell_s,
                    cell_s,
                    cell_s,
                    Color::from_u32(0x2E3440),
                );
            }
        }
    }

    if !*tetris_game_over {
        let shape = &TETROMINOES[*tetris_piece as usize][(*tetris_piece_rot % 4) as usize];
        let color = get_tetris_color(*tetris_piece + 1);
        for y in 0..4 {
            for x in 0..4 {
                if shape[y * 4 + x] == 1 {
                    let bx = *tetris_piece_x + x as i32;
                    let by = *tetris_piece_y + y as i32;
                    if bx >= 0 && bx < 10 && by >= 0 && by < 20 {
                        fill_buf(
                            buf,
                            bw,
                            play_x + bx as usize * cell_s + 1,
                            play_y + by as usize * cell_s + 1,
                            cell_s - 2,
                            cell_s - 2,
                            color,
                        );
                    }
                }
            }
        }
    }

    let ui_x = play_x + 10 * cell_s + 20;
    draw_str_buf(buf, bw, bh, ui_x, play_y, "TETRIS", Color::WHITE, 2);

    let mut score_str = alloc::string::String::from("Score: ");
    push_u64(&mut score_str, *tetris_score as u64);
    draw_str_buf(buf, bw, bh, ui_x, play_y + 40, &score_str, Color::WHITE, 1);

    draw_str_buf(
        buf,
        bw,
        bh,
        ui_x,
        play_y + 80,
        "W to rotate",
        Color::from_u32(0x8899BB),
        1,
    );
    draw_str_buf(
        buf,
        bw,
        bh,
        ui_x,
        play_y + 100,
        "A/D to move",
        Color::from_u32(0x8899BB),
        1,
    );
    draw_str_buf(
        buf,
        bw,
        bh,
        ui_x,
        play_y + 120,
        "S soft drop",
        Color::from_u32(0x8899BB),
        1,
    );
    draw_str_buf(
        buf,
        bw,
        bh,
        ui_x,
        play_y + 140,
        "SPACE drop",
        Color::from_u32(0x8899BB),
        1,
    );

    if *tetris_game_over {
        draw_str_buf(
            buf,
            bw,
            bh,
            play_x + 20,
            play_y + 180,
            "GAME OVER! Press 'R'",
            Color::WHITE,
            2,
        );
    }
}

pub fn browser_handle_keyboard(state: &mut AppState, c: char) {
    let (url_input, _page_lines, _page_content, _is_loading, _error_message) = match state {
        AppState::Browser {
            url_input,
            page_lines,
            page_content,
            is_loading,
            error_message,
            ..
        } => (
            url_input,
            page_lines,
            page_content,
            is_loading,
            error_message,
        ),
        _ => return,
    };
    if c == '\x08' {
        url_input.pop();
    } else if c == '\n' {
        let target = url_input.clone();
        navigate_browser(state, &target, true);
    } else if (c as u32) >= 32 && (c as u32) < 127 {
        url_input.push(c);
    }
}

fn navigate_browser(state: &mut AppState, url: &str, record_history: bool) {
    if let AppState::Browser {
        tabs,
        active_tab,
        url_input,
        page_lines,
        page_content,
        is_loading,
        error_message,
        ..
    } = state
    {
        let target = url.to_string();

        // Update tab
        if let Some(tab) = tabs.get_mut(*active_tab) {
            tab.url = target.clone();
            if record_history {
                tab.history.truncate(tab.history_idx + 1);
                if tab.history.last() != Some(&target) {
                    tab.history.push(target.clone());
                }
                tab.history_idx = tab.history.len() - 1;
            }
        }
        *url_input = target.clone();
        *is_loading = true;
        *error_message = None;

        if target == "about:home" {
            let html = r#"
            <h1>RustOS Browser v0.3</h1>
            <hr>
            <p><strong>Features:</strong></p>
            <ul>
              <li>HTTP/1.0 GET requests</li>
              <li>HTML parsing (h1-h6, p, a, div, span, etc)</li>
              <li>Rich formatting (links, bold, headings)</li>
            </ul>
            <br>
            <p>Try <a href="http://example.com">example.com</a> or rust-lang.org</p>
            <p>HTTPS/TLS is not implemented yet; secure URLs are rejected explicitly.</p>
            "#;
            let nodes = crate::html_parser::parse_html(html);
            *page_lines = crate::html_parser::render_dom(&nodes);
            *page_content = String::new();
            *is_loading = false;
            if let Some(tab) = tabs.get_mut(*active_tab) {
                tab.title = String::from("Home");
            }
        } else {
            // Attempt HTTP GET
            let full_url = if target.starts_with("http://") || target.starts_with("https://") {
                target.clone()
            } else {
                format!("http://{}", target)
            };

            match crate::net::http::http_get(&full_url) {
                Ok(response) => {
                    if response.status_code == 200 {
                        let body = String::from_utf8_lossy(&response.body).to_string();
                        *page_content = body.clone();

                        // Try to extract title
                        let title = extract_html_title(&body);
                        if let Some(tab) = tabs.get_mut(*active_tab) {
                            tab.title = if title.is_empty() {
                                target.clone()
                            } else {
                                title
                            };
                        }

                        // Parse HTML via DOM
                        let nodes = crate::html_parser::parse_html(&body);
                        *page_lines = crate::html_parser::render_dom(&nodes);
                        if page_lines.is_empty() {
                            *page_lines = alloc::vec![crate::html_parser::RenderedLine {
                                text: String::from("(Empty page)"),
                                color: Color::from_u32(0xABB2BF),
                                is_bold: false,
                                is_italic: false,
                                is_heading: false,
                                heading_level: 0,
                                is_link: false,
                                link_url: None,
                                indent: 0,
                                is_horizontal_rule: false,
                                is_image: false,
                                image_alt: String::new()
                            }];
                        }
                    } else {
                        let mut status_str = String::from("HTTP ");
                        push_u64(&mut status_str, response.status_code as u64);
                        status_str.push_str(" ");
                        status_str.push_str(&response.status_text);
                        *page_lines = crate::html_parser::render_dom(
                            &crate::html_parser::parse_html(&format!(
                                "<h1>{}</h1><p>The server returned an error.</p>",
                                status_str
                            )),
                        );
                        *error_message = Some(response.status_text.clone());
                    }
                    *is_loading = false;
                },
                Err(error) => {
                    *page_lines =
                        crate::html_parser::render_dom(&crate::html_parser::parse_html(&format!(
                            "<h1>Request Failed</h1><p>{}</p><p>Network support is experimental \
                             in RustOS.</p>",
                            error.message()
                        )));
                    *page_content = String::new();
                    *is_loading = false;
                    *error_message = Some(String::from(error.message()));
                    if let Some(tab) = tabs.get_mut(*active_tab) {
                        tab.title = String::from("Error");
                    }
                },
            }
        }
    }
}

fn extract_html_title(html: &str) -> String {
    if let Some(start) = html.find("<title>") {
        if let Some(end) = html.find("</title>") {
            if end > start + 7 {
                let title = &html[start + 7..end];
                // Remove nested tags
                let mut clean = String::new();
                let mut in_tag = false;
                for ch in title.chars() {
                    if ch == '<' {
                        in_tag = true;
                        continue;
                    }
                    if ch == '>' {
                        in_tag = false;
                        continue;
                    }
                    if !in_tag {
                        clean.push(ch);
                    }
                }
                return clean.trim().to_string();
            }
        }
    }
    String::new()
}

fn resolve_browser_link(current_url: &str, link: &str) -> String {
    if link.starts_with('/') {
        if let Ok(base) = crate::net::http::parse_url(current_url) {
            let port = if base.port == 80 {
                String::new()
            } else {
                format!(":{}", base.port)
            };
            return format!("http://{}{}{}", base.host, port, link);
        }
    }
    link.to_string()
}

fn render_browser(win: &mut Window, state: &AppState) {
    let (tabs, active_tab, url_input, page_lines, is_loading, scroll_offset) = match state {
        AppState::Browser {
            tabs,
            active_tab,
            url_input,
            page_lines,
            is_loading,
            scroll_offset,
            ..
        } => (
            tabs,
            active_tab,
            url_input,
            page_lines,
            is_loading,
            scroll_offset,
        ),
        _ => return,
    };
    let bw = win.width();
    let bh = win.height();
    let buf = &mut win.buffer;

    // Chrome/Firefox Style with KDE Neon accents
    let tab_bar_h = 32;
    let nav_bar_h = 38;

    // 1. Tab Bar
    fill_buf(
        buf,
        bw,
        0,
        TITLE_BAR_H,
        bw,
        tab_bar_h,
        Color::from_u32(0x1A1B26),
    );
    let mut tx = 5;
    for (i, tab) in tabs.iter().enumerate() {
        let is_active = i == *active_tab;
        let tab_w = 120;
        let bg = if is_active {
            Color::from_u32(0x24283B)
        } else {
            Color::from_u32(0x16161E)
        };
        fill_buf(buf, bw, tx, TITLE_BAR_H + 4, tab_w, tab_bar_h - 4, bg);
        // Neon accent on active tab
        if is_active {
            fill_buf(
                buf,
                bw,
                tx,
                TITLE_BAR_H + 4,
                2,
                tab_bar_h - 4,
                Color::from_u32(0x7AA2F7),
            );
        }
        draw_str_buf(
            buf,
            bw,
            bh,
            tx + 8,
            TITLE_BAR_H + 10,
            &tab.title,
            Color::WHITE,
            1,
        );
        draw_str_buf(
            buf,
            bw,
            bh,
            tx + tab_w - 15,
            TITLE_BAR_H + 10,
            "x",
            Color::from_u32(0x565F89),
            1,
        );
        tx += tab_w + 2;
    }
    // New tab [+]
    fill_buf(
        buf,
        bw,
        tx,
        TITLE_BAR_H + 8,
        20,
        20,
        Color::from_u32(0x292E42),
    );
    draw_str_buf(
        buf,
        bw,
        bh,
        tx + 6,
        TITLE_BAR_H + 12,
        "+",
        Color::from_u32(0x7AA2F7),
        1,
    );

    // 2. Navigation Bar
    let nav_y = TITLE_BAR_H + tab_bar_h;
    fill_buf(buf, bw, 0, nav_y, bw, nav_bar_h, Color::from_u32(0x1F2335));

    // Buttons: [<] [>] [R] with neon accents
    draw_str_buf(
        buf,
        bw,
        bh,
        8,
        nav_y + 10,
        "<",
        Color::from_u32(0x7AA2F7),
        1,
    );
    draw_str_buf(
        buf,
        bw,
        bh,
        28,
        nav_y + 10,
        ">",
        Color::from_u32(0x7AA2F7),
        1,
    );
    draw_str_buf(
        buf,
        bw,
        bh,
        48,
        nav_y + 10,
        "R",
        Color::from_u32(0x9ECE6A),
        1,
    );

    // URL Bar with neon glow effect
    let url_x = 70;
    let url_w = bw.saturating_sub(url_x + 10);
    fill_buf(
        buf,
        bw,
        url_x,
        nav_y + 6,
        url_w,
        24,
        Color::from_u32(0x16161E),
    );
    // Neon border
    fill_buf(
        buf,
        bw,
        url_x,
        nav_y + 6,
        url_w,
        1,
        Color::from_u32(0x3D59A1),
    );
    fill_buf(
        buf,
        bw,
        url_x,
        nav_y + 29,
        url_w,
        1,
        Color::from_u32(0x3D59A1),
    );
    fill_buf(buf, bw, url_x, nav_y + 6, 1, 24, Color::from_u32(0x3D59A1));
    fill_buf(
        buf,
        bw,
        url_x + url_w - 1,
        nav_y + 6,
        1,
        24,
        Color::from_u32(0x3D59A1),
    );

    let display_url = if *is_loading {
        let mut s = String::from("Loading: ");
        s.push_str(url_input);
        s
    } else {
        url_input.clone()
    };
    draw_str_buf(
        buf,
        bw,
        bh,
        url_x + 8,
        nav_y + 10,
        &display_url,
        Color::from_u32(0xC0CAF5),
        1,
    );

    // 3. Content Area
    let content_y = nav_y + nav_bar_h;
    let content_h = bh.saturating_sub(content_y);
    fill_buf(
        buf,
        bw,
        0,
        content_y,
        bw,
        content_h,
        Color::from_u32(0x1A1B26),
    );

    // Render page content
    let mut y = content_y + 4;
    let max_rows = content_h.saturating_sub(8) / 16;
    let start_row = *scroll_offset;

    for i in 0..max_rows {
        let line_idx = i + start_row;
        if line_idx >= page_lines.len() {
            break;
        }

        let rl = &page_lines[line_idx];

        if rl.is_horizontal_rule {
            fill_buf(buf, bw, 8, y + 8, bw.saturating_sub(16), 2, rl.color);
            y += 16;
            continue;
        }

        if rl.is_image {
            if rl.text.is_empty() {
                draw_str_buf(buf, bw, bh, 8 + rl.indent * 8, y, "[Image]", rl.color, 1);
            } else {
                let s = alloc::format!("[Image: {}]", rl.text);
                draw_str_buf(buf, bw, bh, 8 + rl.indent * 8, y, &s, rl.color, 1);
            }
            y += 16;
            continue;
        }

        let scale = if rl.is_heading {
            (5 - rl.heading_level).max(1).min(3) as usize
        } else {
            1
        };

        draw_str_buf(buf, bw, bh, 8 + rl.indent * 8, y, &rl.text, rl.color, scale);
        y += 16 * scale;
    }

    // Loading indicator
    if *is_loading {
        let dots = match crate::gui::CLOCK_TICKS.load(core::sync::atomic::Ordering::Relaxed) % 4 {
            0 => ".",
            1 => "..",
            2 => "...",
            _ => "....",
        };
        let mut load_msg = String::from("Loading");
        load_msg.push_str(dots);
        draw_str_buf(
            buf,
            bw,
            bh,
            (bw - 80) / 2,
            content_y + content_h / 2,
            &load_msg,
            Color::from_u32(0x7AA2F7),
            1,
        );
    }

    // Scrollbar
    if page_lines.len() > max_rows {
        let sb_x = bw - 8;
        let sb_h = content_h;
        let thumb_h = (max_rows * sb_h) / page_lines.len();
        let thumb_y = content_y + (start_row * sb_h) / page_lines.len();
        fill_buf(buf, bw, sb_x, content_y, 4, sb_h, Color::from_u32(0x292E42));
        fill_buf(
            buf,
            bw,
            sb_x,
            thumb_y,
            4,
            thumb_h.max(10),
            Color::from_u32(0x565F89),
        );
    }
}

fn render_mock_web_content(buf: &mut [Color], bw: usize, bh: usize, y: usize, url: &str) {
    if url == "about:home" {
        draw_str_buf(
            buf,
            bw,
            bh,
            (bw - 120) / 2,
            y + 40,
            "RustOS Search",
            Color::from_u32(0x4285F4),
            2,
        );
        fill_buf(
            buf,
            bw,
            (bw - 300) / 2,
            y + 80,
            300,
            30,
            Color::from_u32(0xF1F3F4),
        );
        draw_str_buf(
            buf,
            bw,
            bh,
            (bw - 300) / 2 + 10,
            y + 88,
            "Search the web...",
            Color::from_u32(0x70757A),
            1,
        );
    } else if url.contains("google") {
        draw_str_buf(
            buf,
            bw,
            bh,
            20,
            y + 20,
            "Search results for:",
            Color::BLACK,
            1,
        );
        draw_str_buf(
            buf,
            bw,
            bh,
            20,
            y + 40,
            "1. Rust Programming Language",
            Color::from_u32(0x1A0DAB),
            1,
        );
        draw_str_buf(
            buf,
            bw,
            bh,
            20,
            y + 60,
            "2. x86_64 OSDev Wiki",
            Color::from_u32(0x1A0DAB),
            1,
        );
    } else {
        draw_str_buf(buf, bw, bh, 20, y + 20, "404 Not Found", Color::RED, 2);
        draw_str_buf(
            buf,
            bw,
            bh,
            20,
            y + 60,
            "Could not connect to server.",
            Color::BLACK,
            1,
        );
    }
}

fn browser_handle_click(_win_id: usize, state: &mut AppState, lx: isize, ly: isize) {
    let x = lx as i32;
    let y = ly as i32;

    // Tab Bar click
    if y >= TITLE_BAR_H as i32 && y < (TITLE_BAR_H + 32) as i32 {
        let mut target_to_load = None;
        if let AppState::Browser {
            tabs,
            active_tab,
            url_input,
            ..
        } = state
        {
            let mut tx = 5;
            for i in 0..tabs.len() {
                if x >= tx && x < tx + 120 {
                    if x >= tx + 105 {
                        if tabs.len() > 1 {
                            tabs.remove(i);
                            if *active_tab >= tabs.len() {
                                *active_tab = tabs.len() - 1;
                            }
                            target_to_load = Some(tabs[*active_tab].url.clone());
                        }
                    } else {
                        *active_tab = i;
                        *url_input = tabs[i].url.clone();
                        target_to_load = Some(tabs[i].url.clone());
                    }
                    break;
                }
                tx += 122;
            }
            if target_to_load.is_none() && x >= tx && x < tx + 20 {
                tabs.push(WebTab {
                    title: String::from("New Tab"),
                    url: String::from("about:home"),
                    history: vec![String::from("about:home")],
                    history_idx: 0,
                });
                *active_tab = tabs.len() - 1;
                *url_input = String::from("about:home");
                target_to_load = Some(String::from("about:home"));
            }
        }
        if let Some(target) = target_to_load {
            navigate_browser(state, &target, false);
        }
        return;
    }

    // Navigation buttons and URL bar
    let nav_y = (TITLE_BAR_H + 32) as i32;
    if y >= nav_y && y < nav_y + 38 {
        if x >= 8 && x < 22 {
            // [Back]
            if let AppState::Browser {
                tabs,
                active_tab,
                url_input,
                ..
            } = state
            {
                if let Some(tab) = tabs.get_mut(*active_tab) {
                    if tab.history_idx > 0 {
                        tab.history_idx -= 1;
                        let url = tab.history[tab.history_idx].clone();
                        *url_input = url.clone();
                        navigate_browser(state, &url, false);
                    }
                }
            }
        } else if x >= 28 && x < 42 {
            // [Forward]
            if let AppState::Browser {
                tabs,
                active_tab,
                url_input,
                ..
            } = state
            {
                if let Some(tab) = tabs.get_mut(*active_tab) {
                    if tab.history_idx + 1 < tab.history.len() {
                        tab.history_idx += 1;
                        let url = tab.history[tab.history_idx].clone();
                        *url_input = url.clone();
                        navigate_browser(state, &url, false);
                    }
                }
            }
        } else if x >= 48 && x < 62 {
            // [Reload]
            if let AppState::Browser {
                tabs,
                active_tab,
                ..
            } = state
            {
                if let Some(tab) = tabs.get(*active_tab) {
                    let url = tab.url.clone();
                    navigate_browser(state, &url, false);
                }
            }
        }
        return;
    }

    // Follow rendered links in page content.
    let content_y = (TITLE_BAR_H + 32 + 38) as i32;
    let mut clicked_link = None;
    if y >= content_y && x < 592 {
        if let AppState::Browser {
            tabs,
            active_tab,
            page_lines,
            scroll_offset,
            ..
        } = state
        {
            let current_url = &tabs[*active_tab].url;
            let mut row_y = content_y + 4;
            for line in page_lines.iter().skip(*scroll_offset) {
                let scale = if line.is_heading {
                    (5 - line.heading_level).clamp(1, 3) as i32
                } else {
                    1
                };
                let height = if line.is_image || line.is_horizontal_rule {
                    16
                } else {
                    16 * scale
                };
                let left = 8 + line.indent as i32 * 8;
                let right = left + line.text.len() as i32 * 8 * scale;
                if line.is_link && y >= row_y && y < row_y + height && x >= left && x <= right {
                    if let Some(url) = &line.link_url {
                        clicked_link = Some(resolve_browser_link(current_url, url));
                    }
                    break;
                }
                row_y += height;
                if row_y > 450 {
                    break;
                }
            }
        }
    }
    if let Some(target) = clicked_link {
        navigate_browser(state, &target, true);
        return;
    }

    // Scrollbar click
    let bw = 600;
    let nav_y_full = (TITLE_BAR_H + 32 + 38) as i32;
    if y >= nav_y_full && x >= (bw - 8) as i32 {
        if let AppState::Browser {
            page_lines,
            scroll_offset,
            ..
        } = state
        {
            let content_h = 450i32 - nav_y_full;
            let sb_h = content_h;
            let max_rows = content_h.saturating_sub(8) / 16;
            let total = page_lines.len() as i32;
            if total > max_rows {
                let click_ratio = (y - nav_y_full) as f32 / sb_h as f32;
                *scroll_offset = (click_ratio * (total - max_rows) as f32).max(0.0) as usize;
            }
        }
    }
}
