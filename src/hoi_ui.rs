//! Original dark-steel grand-strategy UI skin, rendered procedurally.
//! No copyrighted HOI4 interface textures or sprites are bundled.
use macroquad::prelude::*;
use crate::{draw_ui_text, measure_ui_text};

pub const GOLD: Color = Color::new(0.79, 0.66, 0.43, 1.0);
pub const TEXT: Color = Color::new(0.87, 0.86, 0.82, 1.0);
pub const MUTED: Color = Color::new(0.64, 0.67, 0.66, 1.0);
pub const GREEN: Color = Color::new(0.46, 0.67, 0.43, 1.0);
pub const BASE: Color = Color::new(0.075, 0.088, 0.092, 1.0);
pub const FACE: Color = Color::new(0.17, 0.18, 0.18, 1.0);
pub const EDGE: Color = Color::new(0.39, 0.39, 0.34, 1.0);

pub fn panel(x: f32, y: f32, w: f32, h: f32) {
    if w <= 1.0 || h <= 1.0 { return; }
    draw_rectangle(x + 3.0, y + 4.0, w, h, Color::new(0.01, 0.02, 0.02, 0.52));
    draw_rectangle(x, y, w, h, BASE);
    draw_rectangle(x + 3.0, y + 3.0, w - 6.0, h - 6.0, FACE);
    draw_rectangle_lines(x, y, w, h, 1.5, EDGE);
    draw_line(x + 2.0, y + 2.0, x + w - 2.0, y + 2.0, 1.0, Color::new(0.52, 0.51, 0.44, 1.0));
    draw_line(x + 2.0, y + h - 3.0, x + w - 2.0, y + h - 3.0, 1.0, Color::new(0.06, 0.07, 0.07, 1.0));
}

pub fn header(x: f32, y: f32, w: f32, text: &str) {
    draw_rectangle(x, y, w, 34.0, Color::new(0.12, 0.14, 0.14, 1.0));
    draw_rectangle(x + 1.0, y + 32.0, w - 2.0, 2.0, GOLD);
    draw_ui_text(text, x + 13.0, y + 24.0, 19.0, TEXT);
}

pub fn inside(rect: Rect) -> bool {
    let (mx, my) = mouse_position();
    mx >= rect.x && mx <= rect.x + rect.w && my >= rect.y && my <= rect.y + rect.h
}

pub fn small_button(text: &str, rect: Rect, active: bool) -> bool {
    if rect.w <= 1.0 || rect.h <= 1.0 { return false; }
    let hot = inside(rect);
    let bg = if active {
        Color::new(0.37, 0.30, 0.19, 1.0)
    } else if hot {
        Color::new(0.29, 0.30, 0.28, 1.0)
    } else {
        Color::new(0.20, 0.22, 0.22, 1.0)
    };
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, bg);
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, if active { GOLD } else { EDGE });
    draw_line(rect.x + 1.0, rect.y + 1.0, rect.x + rect.w - 1.0, rect.y + 1.0,
        1.0, Color::new(0.42, 0.43, 0.40, 1.0));
    let mut size: u16 = if rect.h >= 33.0 { 15 } else { 12 };
    while size > 9 && measure_ui_text(text, size).width > rect.w - 10.0 { size -= 1; }
    // Very long strings are truncated rather than drawn over neighboring controls.
    let mut shown = text.to_owned();
    while !shown.is_empty() && measure_ui_text(&shown, size).width > rect.w - 8.0 { shown.pop(); }
    let tw = measure_ui_text(&shown, size).width;
    draw_ui_text(&shown, rect.x + (rect.w - tw) * 0.5, rect.y + (rect.h + size as f32 * 0.56) * 0.5,
        size as f32, if active { GOLD } else { TEXT });
    hot && is_mouse_button_pressed(MouseButton::Left)
}

pub fn progress_bar(rect: Rect, portion: f32) {
    if rect.w <= 0.0 { return; }
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::new(0.10, 0.12, 0.12, 1.0));
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, EDGE);
    let p = if portion.is_finite() { portion.clamp(0.0, 1.0) } else { 0.0 };
    let fill = (rect.w - 2.0).max(0.0) * p;
    if fill > 0.0 {
        draw_rectangle(rect.x + 1.0, rect.y + 1.0, fill, (rect.h - 2.0).max(1.0), GREEN);
        draw_rectangle(rect.x + 1.0, rect.y + 1.0, fill, 2.0, Color::new(0.69, 0.81, 0.57, 1.0));
    }
}

pub fn stat(x: f32, y: f32, w: f32, label: &str, value: &str) {
    draw_rectangle(x, y, w, 35.0, Color::new(0.14, 0.16, 0.16, 1.0));
    draw_rectangle_lines(x, y, w, 35.0, 1.0, EDGE);
    draw_ui_text(label, x + 8.0, y + 13.0, 10.0, MUTED);
    draw_ui_text(value, x + 8.0, y + 29.0, 15.0, GOLD);
}

pub fn row_bg(rect: Rect, selected: bool, alternate: bool) {
    let shade = if selected { Color::new(0.31, 0.29, 0.24, 1.0) }
        else if alternate { Color::new(0.21, 0.23, 0.23, 1.0) }
        else { Color::new(0.17, 0.19, 0.19, 1.0) };
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, shade);
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, if selected { GOLD } else { EDGE });
}
