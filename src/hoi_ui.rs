//! Original dark-steel grand-strategy UI skin, rendered procedurally.
//! No copyrighted HOI4 interface textures or sprites are bundled.
use crate::{draw_ui_text, measure_ui_text};
use macroquad::prelude::*;

pub const GOLD: Color = Color::new(0.76, 0.69, 0.48, 1.0);
pub const TEXT: Color = Color::new(0.87, 0.86, 0.82, 1.0);
pub const MUTED: Color = Color::new(0.64, 0.67, 0.66, 1.0);
pub const GREEN: Color = Color::new(0.46, 0.67, 0.43, 1.0);
pub const BASE: Color = Color::new(0.075, 0.088, 0.092, 1.0);
pub const FACE: Color = Color::new(0.19, 0.21, 0.22, 1.0);
pub const EDGE: Color = Color::new(0.36, 0.39, 0.38, 1.0);

pub fn panel(x: f32, y: f32, w: f32, h: f32) {
    if w <= 1.0 || h <= 1.0 {
        return;
    }
    draw_rectangle(x + 3.0, y + 4.0, w, h, Color::new(0.01, 0.02, 0.02, 0.52));
    draw_rectangle(x, y, w, h, BASE);
    draw_rectangle(x + 3.0, y + 3.0, w - 6.0, h - 6.0, FACE);
    // Subtle original brushed-steel shading; no raster game assets required.
    for stripe in 0..(h / 5.0).min(240.0) as usize {
        let sy = y + 4.0 + stripe as f32 * 5.0;
        if sy < y + h - 3.0 {
            draw_line(
                x + 4.0,
                sy,
                x + w - 4.0,
                sy,
                1.0,
                Color::new(0.7, 0.75, 0.8, 0.018),
            );
        }
    }
    draw_rectangle_lines(x, y, w, h, 1.5, EDGE);
    draw_line(
        x + 2.0,
        y + 2.0,
        x + w - 2.0,
        y + 2.0,
        1.0,
        Color::new(0.52, 0.51, 0.44, 1.0),
    );
    draw_line(
        x + 2.0,
        y + h - 3.0,
        x + w - 2.0,
        y + h - 3.0,
        1.0,
        Color::new(0.06, 0.07, 0.07, 1.0),
    );
}

pub fn header(x: f32, y: f32, w: f32, text: &str) {
    draw_rectangle(x, y, w, 34.0, Color::new(0.12, 0.14, 0.14, 1.0));
    draw_rectangle(x + 1.0, y + 32.0, w - 2.0, 2.0, GOLD);
    let mut size = 19u16;
    while size > 13 && measure_ui_text(text, size).width > w - 58.0 {
        size -= 1;
    }
    label(
        text,
        Rect::new(x + 12.0, y + 5.0, w - 55.0, 24.0),
        size as f32,
        TEXT,
    );
}

pub fn inside(rect: Rect) -> bool {
    if INPUT_BLOCKED.with(|v| v.get()) {
        return false;
    }
    if SCROLL_DRAG.with(|s| s.borrow().is_some()) {
        return false;
    }
    let (mx, my) = mouse_position();
    mx >= rect.x && mx <= rect.x + rect.w && my >= rect.y && my <= rect.y + rect.h
}

pub fn small_button(text: &str, rect: Rect, active: bool) -> bool {
    if rect.w <= 1.0 || rect.h <= 1.0 {
        return false;
    }
    let hot = inside(rect);
    let bg = if active {
        Color::new(0.37, 0.30, 0.19, 1.0)
    } else if hot {
        Color::new(0.29, 0.30, 0.28, 1.0)
    } else {
        Color::new(0.20, 0.22, 0.22, 1.0)
    };
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, bg);
    draw_rectangle_lines(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        1.0,
        if active { GOLD } else { EDGE },
    );
    draw_line(
        rect.x + 1.0,
        rect.y + 1.0,
        rect.x + rect.w - 1.0,
        rect.y + 1.0,
        1.0,
        Color::new(0.42, 0.43, 0.40, 1.0),
    );
    if text.chars().count() > 2 {
        tooltip(rect, text, "");
    }
    let mut size: u16 = if rect.h >= 33.0 { 15 } else { 12 };
    while size > 9 && measure_ui_text(text, size).width > rect.w - 10.0 {
        size -= 1;
    }
    // Very long strings are truncated rather than drawn over neighboring controls.
    let mut shown = text.to_owned();
    while !shown.is_empty() && measure_ui_text(&shown, size).width > rect.w - 8.0 {
        shown.pop();
    }
    let tw = measure_ui_text(&shown, size).width;
    draw_ui_text(
        &shown,
        rect.x + (rect.w - tw) * 0.5,
        rect.y + (rect.h + size as f32 * 0.56) * 0.5,
        size as f32,
        if active { GOLD } else { TEXT },
    );
    hot && is_mouse_button_pressed(MouseButton::Left)
}

pub fn progress_bar(rect: Rect, portion: f32) {
    if rect.w <= 0.0 {
        return;
    }
    draw_rectangle(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        Color::new(0.10, 0.12, 0.12, 1.0),
    );
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, EDGE);
    let p = if portion.is_finite() {
        portion.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let fill = (rect.w - 2.0).max(0.0) * p;
    if fill > 0.0 {
        draw_rectangle(
            rect.x + 1.0,
            rect.y + 1.0,
            fill,
            (rect.h - 2.0).max(1.0),
            GREEN,
        );
        draw_rectangle(
            rect.x + 1.0,
            rect.y + 1.0,
            fill,
            2.0,
            Color::new(0.69, 0.81, 0.57, 1.0),
        );
    }
}

pub fn stat(x: f32, y: f32, w: f32, label: &str, value: &str) {
    draw_rectangle(x, y, w, 35.0, Color::new(0.14, 0.16, 0.16, 1.0));
    draw_rectangle_lines(x, y, w, 35.0, 1.0, EDGE);
    draw_ui_text(label, x + 8.0, y + 13.0, 10.0, MUTED);
    draw_ui_text(value, x + 8.0, y + 29.0, 15.0, GOLD);
}

pub fn row_bg(rect: Rect, selected: bool, alternate: bool) {
    let shade = if selected {
        Color::new(0.31, 0.29, 0.24, 1.0)
    } else if alternate {
        Color::new(0.21, 0.23, 0.23, 1.0)
    } else {
        Color::new(0.17, 0.19, 0.19, 1.0)
    };
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, shade);
    draw_rectangle_lines(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        1.0,
        if selected { GOLD } else { EDGE },
    );
}

pub fn label(text: &str, r: Rect, size: f32, color: Color) {
    let mut shown = text.to_owned();
    while !shown.is_empty() && measure_ui_text(&shown, size as u16).width > r.w {
        shown.pop();
    }
    draw_ui_text(&shown, r.x, r.y + size, size, color);
}

pub fn section(title: &str, x: f32, y: f32, width: f32) {
    draw_rectangle(x, y, width, 23.0, BASE);
    draw_line(x, y + 22.0, x + width, y + 22.0, 1.0, EDGE);
    label(
        title,
        Rect::new(x + 7.0, y + 3.0, width - 14.0, 18.0),
        13.0,
        GOLD,
    );
}

pub fn disabled(text: &str, rect: Rect, reason: &str) {
    draw_rectangle(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        Color::from_rgba(39, 43, 45, 255),
    );
    draw_rectangle_lines(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        1.0,
        Color::from_rgba(67, 72, 72, 255),
    );
    label(
        text,
        Rect::new(
            rect.x + 8.0,
            rect.y + (rect.h - 13.0) * 0.5 - 1.0,
            rect.w - 16.0,
            17.0,
        ),
        12.0,
        MUTED,
    );
    tooltip(rect, text, reason);
}

pub fn placeholder(rect: Rect, text: &str) {
    disabled(text, rect, "ЗАГЛУШКА · Эта механика ещё не реализована.");
}

#[derive(Default)]
struct TooltipState {
    candidate: Option<(String, String)>,
    previous: String,
    since: f64,
    anchor: Vec2,
}
thread_local! {
    static SCROLL_DRAG: std::cell::RefCell<Option<(Rect,f32)>> = const { std::cell::RefCell::new(None) };
    static INPUT_BLOCKED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static TOOLTIPS: std::cell::RefCell<TooltipState> = std::cell::RefCell::new(TooltipState::default());
    static FLAGS: std::cell::RefCell<std::collections::HashMap<String,Option<Texture2D>>> = std::cell::RefCell::new(std::collections::HashMap::new());
}
pub fn block_input(block: bool) {
    INPUT_BLOCKED.with(|v| v.set(block));
}
pub fn begin_frame() {
    TOOLTIPS.with(|s| s.borrow_mut().candidate = None);
    if !is_mouse_button_down(MouseButton::Left) {
        SCROLL_DRAG.with(|s| *s.borrow_mut() = None);
    }
}
pub fn tooltip(rect: Rect, title: &str, body: &str) {
    if inside(rect) {
        TOOLTIPS.with(|s| s.borrow_mut().candidate = Some((title.into(), body.into())));
    }
}
pub fn draw_tooltips() {
    if cfg!(target_os = "android") && !is_mouse_button_down(MouseButton::Left) {
        return;
    }
    TOOLTIPS.with(|cell| {
        let mut s = cell.borrow_mut();
        let Some((title, body)) = s.candidate.clone() else {
            s.previous.clear();
            return;
        };
        let key = format!("{title}\n{body}");
        let m = vec2(mouse_position().0, mouse_position().1);
        if s.previous != key || m.distance(s.anchor) > 10.0 {
            s.previous = key;
            s.since = get_time();
            s.anchor = m;
        }
        if get_time() - s.since < 0.38 {
            return;
        }
        let width = (measure_ui_text(&title, 13).width + 28.0)
            .max(if body.is_empty() { 40.0 } else { 250.0 })
            .min(360.0)
            .min(screen_width() - 12.0);
        let mut lines = Vec::<String>::new();
        let mut line = String::new();
        for word in body.split_whitespace() {
            let next = if line.is_empty() {
                word.into()
            } else {
                format!("{line} {word}")
            };
            if measure_ui_text(&next, 12).width > width - 24.0 && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            if line.is_empty() {
                line = word.into();
            } else {
                line.push(' ');
                line.push_str(word);
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
        let h = 39.0 + lines.len() as f32 * 16.0;
        let x = (m.x + 16.0).min(screen_width() - width - 6.0).max(6.0);
        let y = (m.y + 23.0).min(screen_height() - h - 6.0).max(6.0);
        panel(x, y, width, h);
        label(
            &title,
            Rect::new(x + 12.0, y + 8.0, width - 24.0, 18.0),
            13.0,
            GOLD,
        );
        for (i, line) in lines.iter().enumerate() {
            draw_ui_text(line, x + 12.0, y + 40.0 + i as f32 * 16.0, 12.0, TEXT);
        }
    });
}

/// The bar occupies the content's right margin. Capture prevents pressing a
/// list row while dragging its thumb, including emulated mouse input on touch.
pub fn scrollbar(content: Rect, first: &mut usize, max: usize, visible: usize) {
    if max == 0 || content.h < 24.0 {
        return;
    }
    let track = Rect::new(content.right() + 1.0, content.y, 6.0, content.h);
    let thumb_h =
        (track.h * visible.max(1) as f32 / (visible.max(1) + max) as f32).clamp(18.0, track.h);
    let thumb = Rect::new(
        track.x,
        track.y + (track.h - thumb_h) * (*first).min(max) as f32 / max as f32,
        track.w,
        thumb_h,
    );
    let mouse = vec2(mouse_position().0, mouse_position().1);
    if !INPUT_BLOCKED.with(|v| v.get()) {
        SCROLL_DRAG.with(|cell| {
            let mut drag = cell.borrow_mut();
            let hit = Rect::new(track.x - 4.0, track.y, track.w + 8.0, track.h);
            if hit.contains(mouse) && is_mouse_button_pressed(MouseButton::Left) {
                *drag = Some((
                    track,
                    if thumb.contains(mouse) {
                        mouse.y - thumb.y
                    } else {
                        thumb_h * 0.5
                    },
                ));
            }
            if let Some((captured, grab)) = *drag {
                if captured == track && is_mouse_button_down(MouseButton::Left) {
                    let fraction =
                        ((mouse.y - track.y - grab) / (track.h - thumb_h).max(1.0)).clamp(0.0, 1.0);
                    *first = (fraction * max as f32).round() as usize;
                }
            }
        });
    }
    draw_rectangle(track.x, track.y, track.w, track.h, BASE);
    draw_rectangle(
        track.x,
        track.y + (track.h - thumb_h) * (*first).min(max) as f32 / max as f32,
        track.w,
        thumb_h,
        EDGE,
    );
}

/// Read only requested local flags; absent textures remain explicit placeholders.
pub fn flag(tag: &str, rect: Rect) {
    FLAGS.with(|cell| {
        let mut flags = cell.borrow_mut();
        let texture = flags.entry(tag.into()).or_insert_with(|| {
            let image = image::open(format!("data/hoi4/gfx/flags/{tag}.tga"))
                .ok()?
                .to_rgba8();
            if image.width() > 512 || image.height() > 512 {
                return None;
            }
            let t =
                Texture2D::from_rgba8(image.width() as u16, image.height() as u16, image.as_raw());
            t.set_filter(FilterMode::Linear);
            Some(t)
        });
        draw_rectangle(rect.x - 2.0, rect.y - 2.0, rect.w + 4.0, rect.h + 4.0, BASE);
        if let Some(t) = texture {
            draw_texture_ex(
                t,
                rect.x,
                rect.y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(rect.w, rect.h)),
                    ..Default::default()
                },
            );
        } else {
            let c = crate::tag_color(tag);
            draw_rectangle(
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                Color::from_rgba(c[0], c[1], c[2], 255),
            );
            label(
                tag,
                Rect::new(
                    rect.x + 4.0,
                    rect.y + (rect.h - 13.0) * 0.5,
                    rect.w - 8.0,
                    18.0,
                ),
                13.0,
                TEXT,
            );
            tooltip(
                rect,
                "Флаг страны",
                "Флаг не загружен. Используется обозначение страны.",
            );
        }
        draw_rectangle_lines(
            rect.x - 1.0,
            rect.y - 1.0,
            rect.w + 2.0,
            rect.h + 2.0,
            1.0,
            EDGE,
        );
    });
}
