use crate::graphics::Color;
use crate::window::{Window, TITLE_BAR_H};
use super::{AppState, fill_buf, draw_str_buf, push_u64};

pub fn render_settings(win: &mut Window, state: &AppState) {
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

    fill_buf(
        buf,
        bw,
        0,
        TITLE_BAR_H,
        bw,
        bh - TITLE_BAR_H,
        theme.win_bg, 
    );

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
            draw_str_buf(buf, bw, bh, 20, body_y + 10, "Select Theme:", theme.text_main, 1);
            let themes = [("1. Light", 0), ("2. Dark", 1), ("3. Ocean", 2), ("4. Matrix", 3)];
            let mut ty = body_y + 40;
            for (name, _id) in &themes {
                fill_buf(buf, bw, 20, ty, 120, 30, theme.taskbar_bg);
                draw_str_buf(buf, bw, bh, 30, ty + 10, name, theme.text_main, 1);
                ty += 40;
            }
        },
        1 => {
            draw_str_buf(buf, bw, bh, 20, body_y + 10, "System", theme.text_main, 1);
            let (total, free) = crate::memory_allocators::memory_stats_mib();
            let mut line = alloc::string::String::from("Memory: ");
            push_u64(&mut line, total as u64);
            line.push_str(" MiB total, ");
            push_u64(&mut line, free as u64);
            line.push_str(" MiB free");
            draw_str_buf(
                buf,
                bw,
                bh,
                20,
                body_y + 32,
                &line,
                theme.text_dim,
                1,
            );
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

pub fn settings_handle_click(state: &mut AppState, local_x: isize, local_y: isize) {
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
                unsafe { crate::theme::CURRENT_THEME = current_theme; }
                break;
            }
            ty += 40;
        }
    }
}
