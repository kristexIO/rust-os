use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use crate::graphics::Color;
use crate::window::{Window, TITLE_BAR_H};
use super::{AppState, fill_buf, draw_str_buf, draw_rect_outline_buf};

pub fn render_file_manager(win: &mut Window, state: &AppState) {
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

    fill_buf(
        buf,
        bw,
        0,
        TITLE_BAR_H,
        bw,
        bh - TITLE_BAR_H,
        theme.win_bg,
    );

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

pub fn fm_handle_click(state: &mut AppState, local_x: isize, local_y: isize) {
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
