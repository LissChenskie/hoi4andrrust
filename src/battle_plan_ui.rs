//! Map tools use the physical border geometry already stored in MapGraph.
use crate::{
    army_ui::ArmyView, frontline::FrontEdge, hoi_ui as ui, map_graph::MapPosition,
    warfare::Warfare, world::World,
};
use macroquad::prelude::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Front,
    EditStart,
    EditEnd,
}
pub fn toolbar_bounds() -> Rect {
    if screen_width() < 760.0 {
        Rect::new(5.0, screen_height() - 154.0, screen_width() - 10.0, 106.0)
    } else {
        Rect::new(5.0, screen_height() - 86.0, screen_width() - 10.0, 34.0)
    }
}
pub fn blocked(view: &ArmyView, p: Vec2) -> bool {
    view.selected_army.is_some() && toolbar_bounds().contains(p)
}
pub fn draw_toolbar(world: &World, tag: &str, w: &mut Warfare, view: &mut ArmyView) {
    let Some(army) = view
        .selected_army
        .filter(|id| w.command.armies.get(id).is_some_and(|a| a.country == tag))
    else {
        return;
    };
    let r = toolbar_bounds();
    ui::panel(r.x, r.y, r.w, r.h);
    let fronts: Vec<_> = w
        .fronts
        .fronts
        .values()
        .filter(|f| f.army == army && f.country == tag)
        .map(|f| f.id)
        .collect();
    if !view.selected_front.is_some_and(|id| fronts.contains(&id)) {
        view.selected_front = fronts.first().copied();
    }
    let actions = [
        "ФРОНТ",
        "НАЧАЛО",
        "КОНЕЦ",
        "УДАЛИТЬ",
        "ДРУГОЙ",
        "РАЗДЕЛИТЬ",
        "СЛИТЬ",
        "РЕЗЕРВ +",
        "РЕЗЕРВ −",
    ];
    let cols = if screen_width() < 760.0 {
        3
    } else {
        actions.len()
    };
    let cell = (r.w - 8.0) / cols as f32;
    for (i, label) in actions.iter().enumerate() {
        let b = Rect::new(
            r.x + 4.0 + (i % cols) as f32 * cell,
            r.y + 3.0 + (i / cols) as f32 * 34.0,
            cell - 3.0,
            28.0,
        );
        if !ui::small_button(
            label,
            b,
            matches!(
                (i, view.plan_tool),
                (0, Some(Tool::Front)) | (1, Some(Tool::EditStart)) | (2, Some(Tool::EditEnd))
            ),
        ) {
            continue;
        }
        match i {
            0 => {
                view.plan_tool = Some(Tool::Front);
                view.notice = "Протяните линию по границе с другой страной".into();
                view.open = false;
            }
            1 | 2 => {
                if view.selected_front.is_some() {
                    view.plan_tool = Some(if i == 1 {
                        Tool::EditStart
                    } else {
                        Tool::EditEnd
                    });
                    view.open = false;
                }
            }
            3 => {
                if let Some(id) = view.selected_front {
                    view.notice = w
                        .fronts
                        .delete(tag, id)
                        .map_or_else(|e| e, |_| "Фронт удалён".into());
                }
            }
            4 => {
                if !fronts.is_empty() {
                    let at = view
                        .selected_front
                        .and_then(|id| fronts.iter().position(|f| *f == id))
                        .unwrap_or(0);
                    view.selected_front = Some(fronts[(at + 1) % fronts.len()]);
                }
            }
            5 => {
                if let Some(id) = view.selected_front {
                    let at = w.fronts.fronts[&id].edges.len() / 2;
                    view.notice = w
                        .fronts
                        .split(tag, id, at)
                        .map_or_else(|e| e, |_| "Фронт разделён".into());
                }
            }
            6 => {
                if let Some(id) = view.selected_front {
                    if let Some(other) = fronts.iter().copied().find(|other| *other != id) {
                        view.notice = w
                            .fronts
                            .merge(world, tag, id, other)
                            .map_or_else(|e| e, |_| "Фронты объединены".into());
                    }
                }
            }
            7 | 8 => {
                if let Some(id) = view.selected_front {
                    let f = w.fronts.fronts.get_mut(&id).unwrap();
                    f.reserve_count = if i == 7 {
                        (f.reserve_count + 1).min(f.assigned_divisions.len())
                    } else {
                        f.reserve_count.saturating_sub(1)
                    };
                    f.command_revision = u64::MAX;
                }
            }
            _ => {}
        }
    }
}
fn point_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let d = b - a;
    let t = if d.length_squared() > 0.0 {
        ((p - a).dot(d) / d.length_squared()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(a + t * d)
}
pub fn nearest_edge(
    world: &World,
    country: &str,
    province: u32,
    point: MapPosition,
    max_distance: f64,
) -> Option<FrontEdge> {
    let mut best = None;
    let mut distance = max_distance as f32;
    for c in world.geography.land_neighbors(province) {
        let own = world.controller_of_province(province);
        let other = world.controller_of_province(c.to);
        let e = if own == Some(country) && other.is_some_and(|t| t != country) {
            FrontEdge {
                friendly: province,
                enemy: c.to,
            }
        } else if other == Some(country) && own.is_some_and(|t| t != country) {
            FrontEdge {
                friendly: c.to,
                enemy: province,
            }
        } else {
            continue;
        };
        for s in world
            .geography
            .border_segments
            .get(&e.key())
            .into_iter()
            .flatten()
        {
            let d = point_segment(
                vec2(point.x as f32, point.y as f32),
                vec2(s[0] as f32, s[1] as f32),
                vec2(s[2] as f32, s[3] as f32),
            );
            if d < distance {
                distance = d;
                best = Some(e);
            }
        }
    }
    best
}
pub fn pointer(
    view: &mut ArmyView,
    w: &mut Warfare,
    world: &World,
    tag: &str,
    province: Option<u32>,
    point: MapPosition,
    max_distance: f64,
    inside: bool,
) {
    let Some(tool) = view.plan_tool else {
        view.front_hover = None;
        return;
    };
    view.front_hover = if inside {
        province.and_then(|p| nearest_edge(world, tag, p, point, max_distance))
    } else {
        None
    };
    if is_mouse_button_pressed(MouseButton::Left) && inside {
        view.front_start = view.front_hover;
        view.front_preview.clear();
        view.front_preview_end = None;
    }
    if let Some((start, end)) = view.front_start.zip(view.front_hover) {
        if view.front_preview_end != Some(end) {
            view.front_preview_end = Some(end);
            if let Some(enemy) = world.controller_of_province(start.enemy) {
                if view.front_preview.is_empty() {
                    view.front_preview.push(start);
                }
                if let Some(at) = view.front_preview.iter().position(|e| *e == end) {
                    view.front_preview.truncate(at + 1);
                } else if let Some(last) = view.front_preview.last().copied() {
                    if let Ok(path) = w.fronts.boundary(world, tag, enemy).path(last, end) {
                        view.front_preview.extend(path.into_iter().skip(1));
                    }
                }
            }
        }
    }
    if is_mouse_button_released(MouseButton::Left) {
        if let Some((start, end)) = view.front_start.take().zip(view.front_hover) {
            if let Some(army) = view.selected_army {
                let mut fronts = std::mem::take(&mut w.fronts);
                let result = match tool {
                    Tool::Front => fronts
                        .create_drawn(w, world, tag, army, &view.front_preview, &view.selected)
                        .map(|id| {
                            view.selected_front = Some(id);
                        }),
                    Tool::EditStart | Tool::EditEnd => view
                        .selected_front
                        .ok_or_else(|| "Выберите фронт".into())
                        .and_then(|id| {
                            fronts.edit_end(w, world, tag, id, end, tool == Tool::EditStart)
                        }),
                };
                w.fronts = fronts;
                view.notice = result.map_or_else(|e| e, |_| "Приказ сохранён".into());
                view.plan_tool = None;
            }
        }
        view.front_preview.clear();
        view.front_preview_end = None;
    }
    if is_key_pressed(KeyCode::Escape) {
        view.plan_tool = None;
        view.front_start = None;
        view.front_preview.clear();
    }
}
pub fn draw_map(world: &World, w: &Warfare, view: &ArmyView, source: Rect, area: Rect) {
    let _clip = ui::clip(area);
    let project = |x: u32, y: u32| {
        vec2(
            area.x + (x as f32 - source.x) / source.w * area.w,
            area.y + (y as f32 - source.y) / source.h * area.h,
        )
    };
    let draw_edge = |edge: FrontEdge, color: Color, thickness: f32| {
        for s in world
            .geography
            .border_segments
            .get(&edge.key())
            .into_iter()
            .flatten()
        {
            let a = project(s[0], s[1]);
            let b = project(s[2], s[3]);
            if a.x.max(b.x) < area.x
                || a.x.min(b.x) > area.right()
                || a.y.max(b.y) < area.y
                || a.y.min(b.y) > area.bottom()
            {
                continue;
            }
            draw_line(a.x, a.y, b.x, b.y, thickness + 2.0, BLACK);
            draw_line(a.x, a.y, b.x, b.y, thickness, color);
        }
    };
    for f in w.fronts.fronts.values() {
        if let Some(army) = w.command.armies.get(&f.army) {
            let color = Color::from_rgba(army.color[0], army.color[1], army.color[2], 255);
            for &edge in &f.edges {
                draw_edge(
                    edge,
                    color,
                    if view.selected_front == Some(f.id) {
                        4.0
                    } else {
                        2.0
                    },
                );
            }
        }
    }
    for &edge in &view.front_preview {
        draw_edge(edge, ui::GOLD, 4.0);
    }
    if let Some(edge) = view.front_hover {
        draw_edge(edge, WHITE, 4.0);
    }
}
