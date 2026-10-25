//! Original small line/silhouette icons, shared by navigation and management.
use macroquad::prelude::*;

#[derive(Clone, Copy, Debug)]
pub enum Icon {
    Politics,
    Decisions,
    Research,
    Diplomacy,
    Trade,
    Construction,
    Production,
    Recruit,
    Logistics,
    Agency,
    Officer,
    Focus,
    Army,
    Navy,
    Air,
    Supply,
    Resources,
    Terrain,
    Power,
    Stability,
    WarSupport,
    Manpower,
    Factory,
    Convoy,
    Fuel,
    Command,
    Globe,
    Menu,
    Flag,
    Nuclear,
}

pub fn draw(icon: Icon, r: Rect, color: Color) {
    let p = |x: f32, y: f32| vec2(r.x + x * r.w, r.y + y * r.h);
    let line = |x: f32, y: f32, a: f32, b: f32| {
        let u = p(x, y);
        let v = p(a, b);
        draw_line(u.x, u.y, v.x, v.y, (r.w / 14.0).max(1.0), color);
    };
    let box_ = |x: f32, y: f32, w: f32, h: f32| {
        draw_rectangle(r.x + x * r.w, r.y + y * r.h, w * r.w, h * r.h, color)
    };
    let ring = |x: f32, y: f32, rad: f32| {
        let q = p(x, y);
        draw_circle_lines(q.x, q.y, rad * r.w, (r.w / 15.0).max(1.0), color);
    };
    match icon {
        Icon::Nuclear => {
            let center = p(0.5, 0.5);
            draw_circle(center.x, center.y, r.w * 0.09, color);
            for i in 0..3 {
                let a = i as f32 * std::f32::consts::TAU / 3.0;
                let b = a + 0.85;
                draw_triangle(
                    p(0.5 + 0.16 * (a + 0.42).cos(), 0.5 + 0.16 * (a + 0.42).sin()),
                    p(0.5 + 0.45 * a.cos(), 0.5 + 0.45 * a.sin()),
                    p(0.5 + 0.45 * b.cos(), 0.5 + 0.45 * b.sin()),
                    color,
                );
            }
        }
        Icon::Politics | Icon::Manpower | Icon::Recruit | Icon::Officer => {
            let q = p(0.5, 0.27);
            draw_circle(q.x, q.y, r.w * 0.16, color);
            draw_triangle(p(0.5, 0.42), p(0.16, 0.9), p(0.84, 0.9), color);
            if matches!(icon, Icon::Officer) {
                box_(0.27, 0.08, 0.46, 0.08);
                box_(0.19, 0.2, 0.62, 0.06);
            }
            if matches!(icon, Icon::Recruit) {
                line(0.8, 0.05, 0.8, 0.35);
                line(0.65, 0.2, 0.95, 0.2);
            }
        }
        Icon::Construction => {
            line(0.2, 0.88, 0.73, 0.23);
            line(0.35, 0.09, 0.87, 0.49);
            line(0.43, 0.07, 0.92, 0.41);
        }
        Icon::Production => {
            ring(0.5, 0.5, 0.28);
            ring(0.5, 0.5, 0.1);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let u = p(0.5 + 0.3 * a.cos(), 0.5 + 0.3 * a.sin());
                let v = p(0.5 + 0.46 * a.cos(), 0.5 + 0.46 * a.sin());
                draw_line(u.x, u.y, v.x, v.y, r.w * 0.13, color);
            }
        }
        Icon::Factory => {
            box_(0.15, 0.46, 0.72, 0.4);
            box_(0.19, 0.14, 0.12, 0.42);
            for i in 0..3 {
                let q = p(0.3 + i as f32 * 0.19, 0.64);
                draw_rectangle(q.x, q.y, r.w * 0.08, r.h * 0.08, crate::hoi_ui::BASE);
            }
        }
        Icon::Research => {
            line(0.35, 0.13, 0.65, 0.13);
            line(0.42, 0.13, 0.42, 0.45);
            line(0.59, 0.13, 0.59, 0.45);
            line(0.42, 0.45, 0.16, 0.87);
            line(0.59, 0.45, 0.85, 0.87);
            line(0.16, 0.87, 0.85, 0.87);
            line(0.29, 0.67, 0.72, 0.67);
        }
        Icon::Army => {
            line(0.12, 0.12, 0.86, 0.88);
            line(0.86, 0.12, 0.12, 0.88);
            line(0.17, 0.66, 0.35, 0.85);
            line(0.65, 0.85, 0.85, 0.65);
        }
        Icon::Navy | Icon::Convoy => {
            line(0.5, 0.1, 0.5, 0.85);
            ring(0.5, 0.17, 0.1);
            line(0.25, 0.4, 0.75, 0.4);
            line(0.1, 0.6, 0.25, 0.86);
            line(0.9, 0.6, 0.75, 0.86);
            line(0.25, 0.86, 0.5, 0.94);
            line(0.75, 0.86, 0.5, 0.94);
        }
        Icon::Air => {
            line(0.5, 0.08, 0.5, 0.9);
            line(0.13, 0.6, 0.5, 0.36);
            line(0.87, 0.6, 0.5, 0.36);
            line(0.32, 0.88, 0.5, 0.76);
            line(0.68, 0.88, 0.5, 0.76);
        }
        Icon::Logistics | Icon::Supply => {
            box_(0.14, 0.25, 0.72, 0.58);
            line(0.1, 0.23, 0.5, 0.08);
            line(0.5, 0.08, 0.9, 0.23);
            let q = p(0.46, 0.25);
            draw_rectangle(q.x, q.y, r.w * 0.1, r.h * 0.58, crate::hoi_ui::BASE);
        }
        Icon::Diplomacy | Icon::Trade => {
            line(0.1, 0.35, 0.9, 0.35);
            line(0.9, 0.35, 0.71, 0.15);
            line(0.1, 0.65, 0.9, 0.65);
            line(0.1, 0.65, 0.29, 0.85);
            if matches!(icon, Icon::Diplomacy) {
                ring(0.5, 0.5, 0.2);
            }
        }
        Icon::Decisions => {
            box_(0.2, 0.12, 0.6, 0.8);
            for y in [0.35, 0.52, 0.69] {
                let q = p(0.31, y);
                draw_rectangle(q.x, q.y, r.w * 0.38, r.h * 0.045, crate::hoi_ui::BASE);
            }
        }
        Icon::Agency => {
            ring(0.5, 0.4, 0.23);
            line(0.1, 0.8, 0.9, 0.8);
            box_(0.22, 0.7, 0.56, 0.16);
            line(0.28, 0.4, 0.72, 0.4);
        }
        Icon::Focus | Icon::Command | Icon::Power => {
            for i in 0..5 {
                let a = i as f32 * std::f32::consts::TAU / 5.0 - std::f32::consts::FRAC_PI_2;
                let b = a + std::f32::consts::TAU * 2.0 / 5.0;
                let u = p(0.5 + 0.43 * a.cos(), 0.5 + 0.43 * a.sin());
                let v = p(0.5 + 0.43 * b.cos(), 0.5 + 0.43 * b.sin());
                draw_line(u.x, u.y, v.x, v.y, (r.w * 0.09).max(1.0), color);
            }
        }
        Icon::Stability => {
            line(0.5, 0.1, 0.5, 0.87);
            line(0.15, 0.29, 0.85, 0.29);
            line(0.17, 0.3, 0.07, 0.65);
            line(0.17, 0.3, 0.33, 0.65);
            line(0.83, 0.3, 0.68, 0.65);
            line(0.83, 0.3, 0.95, 0.65);
            line(0.25, 0.9, 0.75, 0.9);
        }
        Icon::WarSupport | Icon::Flag => {
            line(0.2, 0.12, 0.2, 0.93);
            box_(0.2, 0.15, 0.6, 0.36);
        }
        Icon::Fuel => {
            draw_triangle(p(0.5, 0.06), p(0.18, 0.68), p(0.82, 0.68), color);
            let q = p(0.5, 0.66);
            draw_circle(q.x, q.y, r.w * 0.32, color);
        }
        Icon::Globe => {
            ring(0.5, 0.5, 0.4);
            line(0.12, 0.5, 0.88, 0.5);
            line(0.5, 0.1, 0.5, 0.9);
            ring(0.5, 0.5, 0.24);
        }
        Icon::Terrain => {
            draw_triangle(p(0.08, 0.86), p(0.45, 0.16), p(0.83, 0.86), color);
            draw_triangle(p(0.45, 0.86), p(0.75, 0.39), p(0.96, 0.86), color);
        }
        Icon::Resources => {
            ring(0.5, 0.5, 0.32);
            line(0.18, 0.23, 0.8, 0.86);
            line(0.22, 0.83, 0.8, 0.2);
        }
        Icon::Menu => {
            for y in [0.24, 0.5, 0.76] {
                line(0.12, y, 0.88, y);
            }
        }
    }
}
