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
    Offensive,
    Spearhead,
    Fallback,
    AreaDefense,
    EditPlan,
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
    let plans: Vec<_> = w
        .plans
        .plans
        .values()
        .filter(|p| p.country == tag && p.army == army)
        .map(|p| p.id)
        .collect();
    if !view.selected_plan.is_some_and(|id| plans.contains(&id)) {
        view.selected_plan = plans.first().copied();
    }
    let actions = [
        "ФРОНТ",
        "НАСТУП.",
        "ПРОРЫВ",
        "ОБОРОНА",
        "ОБЛАСТЬ",
        "▶ ПЛАН",
        "❚❚ ПЛАН",
        "РЕЖИМ",
        "К ПЛАНУ",
        "ИСКЛЮЧИТЬ",
        "ПЛАН ▸",
        "УДАЛ.ПЛАН",
        "НАЧАЛО",
        "КОНЕЦ",
        "РАЗДЕЛИТЬ",
        "СЛИТЬ",
        "РЕЗЕРВ +",
        "РЕЗЕРВ −",
        "УДАЛ.ФРОНТ",
        "ИЗМ.ПЛАН",
    ];
    let compact = screen_width() < 760.0;
    let page_size = if compact { 8 } else { actions.len() };
    let cols = if compact { 3 } else { actions.len() };
    let cell = (r.w - 8.0) / cols as f32;
    let first = if compact {
        view.plan_page % actions.len().div_ceil(page_size) * page_size
    } else {
        0
    };
    for (slot, i) in (first..(first + page_size).min(actions.len())).enumerate() {
        let b = Rect::new(
            r.x + 4.0 + (slot % cols) as f32 * cell,
            r.y + 3.0 + (slot / cols) as f32 * 34.0,
            cell - 3.0,
            28.0,
        );
        let tool = match i {
            0 => Some(Tool::Front),
            1 => Some(Tool::Offensive),
            2 => Some(Tool::Spearhead),
            3 => Some(Tool::Fallback),
            4 => Some(Tool::AreaDefense),
            12 => Some(Tool::EditStart),
            13 => Some(Tool::EditEnd),
            19 => Some(Tool::EditPlan),
            _ => None,
        };
        if !ui::small_button(actions[i], b, tool.is_some() && view.plan_tool == tool) {
            continue;
        }
        if let Some(tool) = tool {
            view.plan_tool = Some(tool);
            view.front_start = None;
            view.front_preview.clear();
            view.line_preview.clear();
            view.line_preview_end = None;
            view.open = false;
            view.notice = match tool {
                Tool::Front | Tool::EditStart | Tool::EditEnd => "Протяните линию по границе",
                Tool::AreaDefense => "Проведите по областям для обороны",
                _ => "Нарисуйте линию цели в провинциях",
            }
            .into();
            continue;
        }
        let result = match i {
            5 | 6 => view
                .selected_plan
                .ok_or_else(|| "Сначала нарисуйте план".into())
                .and_then(|id| w.plans.activate(tag, id, i == 5)),
            7 => view
                .selected_plan
                .ok_or_else(|| "Выберите план".into())
                .map(|id| {
                    let p = w.plans.plans.get_mut(&id).unwrap();
                    p.mode = p.mode.next();
                    p.last_review = 0;
                }),
            8 => {
                let ids: Vec<_> = view.selected.iter().copied().collect();
                w.resume_plans(tag, &ids)
            }
            9 => {
                let ids: Vec<_> = view.selected.iter().copied().collect();
                w.detach_plans(tag, &ids)
            }
            10 => {
                if !plans.is_empty() {
                    let at = view
                        .selected_plan
                        .and_then(|id| plans.iter().position(|p| *p == id))
                        .unwrap_or(0);
                    view.selected_plan = Some(plans[(at + 1) % plans.len()]);
                }
                Ok(())
            }
            11 => view
                .selected_plan
                .ok_or_else(|| "Выберите план".into())
                .and_then(|id| w.plans.delete(tag, id)),
            14 => view
                .selected_front
                .ok_or_else(|| "Выберите фронт".into())
                .and_then(|id| {
                    w.fronts
                        .split(tag, id, w.fronts.fronts[&id].edges.len() / 2)
                        .map(|_| ())
                }),
            15 => view
                .selected_front
                .zip(
                    fronts
                        .iter()
                        .copied()
                        .find(|id| Some(*id) != view.selected_front),
                )
                .ok_or_else(|| "Нужны два фронта".into())
                .and_then(|(a, b)| w.fronts.merge(world, tag, a, b)),
            16 | 17 => view
                .selected_front
                .ok_or_else(|| "Выберите фронт".into())
                .map(|id| {
                    let f = w.fronts.fronts.get_mut(&id).unwrap();
                    f.reserve_count = if i == 16 {
                        (f.reserve_count + 1).min(f.assigned_divisions.len())
                    } else {
                        f.reserve_count.saturating_sub(1)
                    };
                    f.command_revision = u64::MAX;
                }),
            18 => view
                .selected_front
                .ok_or_else(|| "Выберите фронт".into())
                .and_then(|id| w.fronts.delete(tag, id)),
            _ => Ok(()),
        };
        view.notice = result.map_or_else(|e| e, |_| "Приказ обновлён".into());
    }
    if compact
        && ui::small_button(
            "ЕЩЁ ▸",
            Rect::new(r.x + 4.0 + 2.0 * cell, r.y + 71.0, cell - 3.0, 28.0),
            false,
        )
    {
        view.plan_page = (view.plan_page + 1) % actions.len().div_ceil(page_size);
    }
    let status = if let Some(p) = view.selected_plan.and_then(|id| w.plans.plans.get(&id)) {
        format!(
            "{} · {:?} · цель {:.0}% · подготовка {:.0}% · {}",
            p.mode.label(),
            p.activation,
            p.progress * 100.0,
            p.preparation * 100.0,
            p.status
        )
    } else {
        view.notice.clone()
    };
    draw_rectangle(r.x, r.y - 22.0, r.w, 22.0, ui::BASE);
    ui::label(
        &status,
        Rect::new(r.x + 6.0, r.y - 20.0, r.w - 12.0, 19.0),
        11.0,
        ui::GOLD,
    );
    ui::tooltip(Rect::new(r.x, r.y - 22.0, r.w, 22.0), "План армии", &status);
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
    if matches!(
        tool,
        Tool::Offensive | Tool::Spearhead | Tool::Fallback | Tool::AreaDefense | Tool::EditPlan
    ) {
        inland_pointer(view, w, world, tag, tool, province, inside);
        return;
    }
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
                    _ => Err("Выберите инструмент фронта".into()),
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
fn inland_pointer(
    view: &mut ArmyView,
    w: &mut Warfare,
    world: &World,
    tag: &str,
    tool: Tool,
    province: Option<u32>,
    inside: bool,
) {
    view.front_hover = None;
    if is_mouse_button_pressed(MouseButton::Left) && inside {
        view.line_preview = province.into_iter().collect();
        view.line_preview_end = province;
    }
    if is_mouse_button_down(MouseButton::Left) && inside {
        if let Some(end) = province.filter(|p| Some(*p) != view.line_preview_end) {
            view.line_preview_end = Some(end);
            if tool == Tool::AreaDefense {
                if !view.line_preview.contains(&end) {
                    view.line_preview.push(end);
                }
            } else if let Some(start) = view.line_preview.last().copied() {
                let foreign = if matches!(tool, Tool::Fallback) {
                    None
                } else {
                    view.line_preview
                        .first()
                        .and_then(|p| world.controller_of_province(*p))
                };
                if let Some(at) = view.line_preview.iter().position(|p| *p == end) {
                    view.line_preview.truncate(at + 1);
                } else if let Ok(path) =
                    crate::operations::province_line(world, start, end, tag, foreign)
                {
                    view.line_preview.extend(path.into_iter().skip(1));
                }
            }
        }
    }
    if is_mouse_button_released(MouseButton::Left) && !view.line_preview.is_empty() {
        let kind = match tool {
            Tool::Spearhead => crate::operations::OrderType::Spearhead,
            Tool::Fallback => crate::operations::OrderType::Fallback,
            Tool::AreaDefense => crate::operations::OrderType::AreaDefense,
            _ => crate::operations::OrderType::Offensive,
        };
        let line = if kind == crate::operations::OrderType::Spearhead {
            view.line_preview.last().copied().into_iter().collect()
        } else {
            view.line_preview.clone()
        };
        let mut plans = std::mem::take(&mut w.plans);
        let result = if tool == Tool::EditPlan {
            view.selected_plan
                .ok_or_else(|| "Выберите план".into())
                .and_then(|id| plans.edit_line(tag, id, line, world))
        } else {
            view.selected_army
                .ok_or_else(|| "Выберите армию".into())
                .and_then(|army| {
                    plans
                        .create(w, world, tag, army, view.selected_front, kind, line)
                        .map(|id| {
                            view.selected_plan = Some(id);
                        })
                })
        };
        w.plans = plans;
        view.notice = result.map_or_else(
            |e| e,
            |_| "План сохранён; нажмите ▶ ПЛАН для исполнения".into(),
        );
        view.plan_tool = None;
        view.line_preview.clear();
        view.line_preview_end = None;
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
    let point = |id: u32| crate::army_controls::project(world, id, source, area);
    let arrow = |a: Vec2, b: Vec2, color: Color| {
        draw_line(a.x, a.y, b.x, b.y, 5.0, BLACK);
        draw_line(a.x, a.y, b.x, b.y, 2.5, color);
        let direction = (b - a).normalize_or_zero();
        let side = vec2(-direction.y, direction.x);
        let left = b - direction * 12.0 + side * 6.0;
        let right = b - direction * 12.0 - side * 6.0;
        draw_triangle(b, left, right, color);
    };
    for p in w.plans.plans.values() {
        let color = if matches!(
            p.order_type,
            crate::operations::OrderType::Fallback | crate::operations::OrderType::AreaDefense
        ) {
            ui::GREEN
        } else {
            ui::GOLD
        };
        for pair in p.destination_line.windows(2) {
            if let Some((a, b)) = point(pair[0]).zip(point(pair[1])) {
                draw_line(a.x, a.y, b.x, b.y, 5.0, BLACK);
                draw_line(a.x, a.y, b.x, b.y, 2.5, color);
            }
        }
        if matches!(
            p.order_type,
            crate::operations::OrderType::Offensive | crate::operations::OrderType::Spearhead
        ) {
            if let Some(front) = p.source_front.and_then(|id| w.fronts.fronts.get(&id)) {
                if let Some((a, b)) = front
                    .edges
                    .get(front.edges.len() / 2)
                    .and_then(|e| point(e.friendly))
                    .zip(
                        p.destination_line
                            .get(p.destination_line.len() / 2)
                            .and_then(|id| point(*id)),
                    )
                {
                    arrow(a, b, color);
                }
            }
        } else if p.order_type == crate::operations::OrderType::AreaDefense {
            for state in p.area_states.iter().filter_map(|id| world.states.get(id)) {
                for &province in &state.province_ids {
                    for edge in world.geography.land_neighbors(province) {
                        if !state.province_ids.contains(&edge.to) {
                            draw_edge(
                                FrontEdge {
                                    friendly: province,
                                    enemy: edge.to,
                                },
                                color,
                                2.0,
                            );
                        }
                    }
                }
            }
        }
    }
    for pair in view.line_preview.windows(2) {
        if let Some((a, b)) = point(pair[0]).zip(point(pair[1])) {
            arrow(a, b, WHITE);
        }
    }
}
