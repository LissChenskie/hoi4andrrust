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
    DeleteOrder,
}
pub fn toolbar_bounds() -> Rect {
    if screen_width() < 760.0 {
        Rect::new(5.0, screen_height() - 178.0, screen_width() - 10.0, 106.0)
    } else {
        Rect::new(5.0, screen_height() - 110.0, screen_width() - 10.0, 34.0)
    }
}
pub fn blocked(view: &ArmyView, p: Vec2) -> bool {
    view.orders_visible()
        && (toolbar_bounds().contains(p) || (view.plan_advanced && advanced_bounds().contains(p)))
}
fn advanced_bounds() -> Rect {
    let r = toolbar_bounds();
    Rect::new(r.x, r.y - 244.0, r.w.min(600.0), 198.0)
}
pub fn begin_tool(view: &mut ArmyView, w: &mut Warfare, tag: &str, tool: Tool) {
    if view.plan_tool == Some(tool) {
        view.cancel_tool();
        return;
    }
    if let Err(e) = view.ensure_army(tag, w) {
        view.notice = e;
        return;
    }
    view.cancel_tool();
    view.plan_advanced = false;
    view.plan_tool = Some(tool);
    view.draw_new_front = tool == Tool::Front
        && (is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift));
    view.open = false;
    view.notice = match tool {
        Tool::Front | Tool::EditStart | Tool::EditEnd => "Протяните по границе. ПКМ / Esc — отмена",
        Tool::AreaDefense => "Проведите по своим областям. ПКМ / Esc — отмена",
        Tool::DeleteOrder => "Нажмите приказ на карте для удаления. ПКМ / Esc — отмена",
        _ => "Нарисуйте конечную линию. Повторный рисунок меняет цель. ПКМ / Esc — отмена",
    }
    .into();
}
pub fn draw_toolbar(world: &World, tag: &str, w: &mut Warfare, view: &mut ArmyView, paused: bool) {
    if !view.orders_visible() {
        return;
    }
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
    if !view.selected_plan.is_some_and(|id| {
        plans.contains(&id)
            && w.plans.plans[&id]
                .source_front
                .is_none_or(|f| Some(f) == view.selected_front)
    }) {
        view.selected_plan = plans
            .iter()
            .copied()
            .find(|id| w.plans.plans[id].source_front == view.selected_front);
    }
    let actions = [
        "Фронт",
        "Наступление",
        "Прорыв",
        "Оборона",
        "Область",
        "▶ Начать",
        "■ Остановить",
        "Режим",
        "К плану",
        "Исключить",
        "Следующий план",
        "Удалить",
        "Начало фронта",
        "Конец фронта",
        "Разделить фронт",
        "Слить фронты",
        "Резерв +",
        "Резерв −",
        "Удалить фронт",
        "Изменить цель",
        "Группа армий",
        "Новый участок",
    ];
    let compact = screen_width() < 760.0;
    let primary = [0usize, 1, 2, 3, 5, 6, 7, 11];
    let cols = if compact { 3 } else { 9 };
    let cell = (r.w - 8.0) / cols as f32;
    let extra = advanced_bounds();
    if view.plan_advanced {
        ui::panel(extra.x, extra.y, extra.w, extra.h);
    }
    let visible: Vec<_> = primary
        .into_iter()
        .enumerate()
        .map(|(slot, i)| {
            (
                i,
                Rect::new(
                    r.x + 4.0 + (slot % cols) as f32 * cell,
                    r.y + 3.0 + (slot / cols) as f32 * 34.0,
                    cell - 3.0,
                    28.0,
                ),
            )
        })
        .chain(
            (0..actions.len())
                .filter(|i| view.plan_advanced && !primary.contains(i))
                .enumerate()
                .map(|(slot, i)| {
                    let cw = (extra.w - 12.0) / 3.0;
                    (
                        i,
                        Rect::new(
                            extra.x + 5.0 + (slot % 3) as f32 * cw,
                            extra.y + 5.0 + (slot / 3) as f32 * 36.0,
                            cw - 4.0,
                            30.0,
                        ),
                    )
                }),
        )
        .collect();
    for (i, b) in visible {
        let tool = match i {
            0 => Some(Tool::Front),
            1 => Some(Tool::Offensive),
            2 => Some(Tool::Spearhead),
            3 => Some(Tool::Fallback),
            4 => Some(Tool::AreaDefense),
            12 => Some(Tool::EditStart),
            13 => Some(Tool::EditEnd),
            19 => Some(Tool::EditPlan),
            11 => Some(Tool::DeleteOrder),
            _ => None,
        };
        let mode = view
            .selected_plan
            .and_then(|id| w.plans.plans.get(&id))
            .map(|p| p.mode.label())
            .unwrap_or("Режим");
        let title = if i == 7 { mode } else { actions[i] };
        let active = (tool.is_some() && view.plan_tool == tool)
            || (i == 5
                && view
                    .selected_plan
                    .and_then(|id| w.plans.plans.get(&id))
                    .is_some_and(|p| p.activation == crate::operations::Activation::Executing));
        if !ui::small_button(title, b, active) {
            continue;
        }
        if let Some(tool) = tool {
            begin_tool(view, w, tag, tool);
            continue;
        }
        let result = match i {
            21 => {
                view.cancel_tool();
                begin_tool(view, w, tag, Tool::Front);
                view.draw_new_front = true;
                Ok(())
            }
            20 => {
                if w.command.armies[&army].group.is_some() {
                    view.group_orders = !view.group_orders;
                    Ok(())
                } else {
                    Err("Сначала включите армию в группу".into())
                }
            }
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
                .and_then(|id| w.delete_plan(tag, id)),
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
                .and_then(|(a, b)| w.merge_fronts(world, tag, a, b)),
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
                .and_then(|id| w.delete_front(tag, id)),
            _ => Ok(()),
        };
        view.notice = result.map_or_else(|e| e, |_| "Приказ обновлён".into());
    }
    if ui::small_button(
        "Ещё ▴",
        Rect::new(
            r.x + 4.0 + (8 % cols) as f32 * cell,
            r.y + 3.0 + (8 / cols) as f32 * 34.0,
            cell - 3.0,
            28.0,
        ),
        view.plan_advanced,
    ) {
        view.plan_advanced = !view.plan_advanced;
    }
    let mut status = if let Some(p) = view.selected_plan.and_then(|id| w.plans.plans.get(&id)) {
        format!(
            "План #{} · {} · {} · цель {:.0}% · подготовка {:.0}% · {}",
            p.id,
            p.order_type.label(),
            p.activation.label(),
            p.progress * 100.0,
            p.preparation * 100.0,
            p.status
        )
    } else {
        view.notice.clone()
    };
    if paused {
        status = format!("ПАУЗА · Space для хода времени · {status}");
    }
    draw_rectangle(r.x, r.y - 42.0, r.w, 42.0, ui::BASE);
    ui::label(
        &view.notice,
        Rect::new(r.x + 6.0, r.y - 40.0, r.w - 12.0, 18.0),
        11.0,
        ui::TEXT,
    );
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
    if is_mouse_button_pressed(MouseButton::Right) || is_key_pressed(KeyCode::Escape) {
        view.cancel_tool();
        return;
    }
    if view.order_handle.is_some() {
        handle_pointer(view, w, world, tag, province, point, max_distance, inside);
        return;
    }
    let Some(tool) = view.plan_tool else {
        view.front_hover = None;
        return;
    };
    if tool == Tool::DeleteOrder {
        if inside && is_mouse_button_pressed(MouseButton::Left) {
            if let Some(order) = view.order_hover {
                view.selected_order = Some(order);
                view.notice =
                    delete_selected(view, w, tag).map_or_else(|e| e, |_| "Приказ удалён".into());
            } else {
                view.notice = "Нажмите линию фронта или наступления".into();
            }
            view.cancel_tool();
        }
        return;
    }
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
        if let Some((_start, end)) = view.front_start.take().zip(view.front_hover) {
            if let Some(army) = view.selected_army {
                let result = match tool {
                    Tool::Front => w
                        .draw_front(
                            world,
                            tag,
                            army,
                            if view.draw_new_front {
                                None
                            } else {
                                view.selected_front
                            },
                            &view.front_preview,
                            &view.selected,
                            view.group_orders,
                        )
                        .map(|id| {
                            view.selected_front = Some(id);
                        }),
                    Tool::EditStart | Tool::EditEnd => view
                        .selected_front
                        .ok_or_else(|| "Выберите фронт".into())
                        .and_then(|id| {
                            let mut fronts = std::mem::take(&mut w.fronts);
                            let r =
                                fronts.edit_end(w, world, tag, id, end, tool == Tool::EditStart);
                            w.fronts = fronts;
                            r
                        }),
                    _ => Err("Выберите инструмент фронта".into()),
                };
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
        let result = view
            .selected_army
            .ok_or_else(|| "Выберите армию".into())
            .and_then(|army| {
                w.draw_plan(
                    world,
                    tag,
                    army,
                    view.selected_front,
                    kind,
                    line,
                    if tool == Tool::EditPlan {
                        view.selected_plan
                    } else {
                        None
                    },
                )
                .map(|id| {
                    view.selected_plan = Some(id);
                })
            });
        view.notice = result.map_or_else(
            |e| e,
            |_| "План сохранён; нажмите ▶ Начать и снимите паузу".into(),
        );
        view.plan_tool = None;
        view.line_preview.clear();
        view.line_preview_end = None;
    }
}
pub fn draw_map(
    world: &World,
    w: &Warfare,
    view: &mut ArmyView,
    tag: Option<&str>,
    source: Rect,
    area: Rect,
    paused: bool,
) {
    if let Some(tag) = tag {
        crate::battle_plan_render::draw(world, w, view, tag, source, area, get_time(), paused);
    }
    let project = |x: u32, y: u32| {
        vec2(
            area.x + (x as f32 - source.x) / source.w * area.w,
            area.y + (y as f32 - source.y) / source.h * area.h,
        )
    };
    let _clip = ui::clip(area);
    for edge in view.front_preview.iter().copied().chain(view.front_hover) {
        for s in world
            .geography
            .border_segments
            .get(&edge.key())
            .into_iter()
            .flatten()
        {
            let a = project(s[0], s[1]);
            let b = project(s[2], s[3]);
            draw_line(a.x, a.y, b.x, b.y, 6.0, BLACK);
            draw_line(a.x, a.y, b.x, b.y, 3.0, WHITE);
        }
    }
    for p in view.line_preview.windows(2) {
        if let Some((a, b)) = crate::army_controls::project(world, p[0], source, area)
            .zip(crate::army_controls::project(world, p[1], source, area))
        {
            draw_line(a.x, a.y, b.x, b.y, 8.0, BLACK);
            draw_line(a.x, a.y, b.x, b.y, 4.0, WHITE);
        }
    }
}

pub fn choose_order(
    view: &mut ArmyView,
    w: &mut Warfare,
    world: &World,
    tag: &str,
    source: Rect,
    area: Rect,
    point: Vec2,
    assign: bool,
    add: bool,
) -> bool {
    let Some(order) = view
        .order_map
        .hit(world, w, source, area, point, view.selected_plan)
    else {
        return false;
    };
    if assign && !view.selected.is_empty() {
        let front = match order {
            crate::battle_plan_render::OrderRef::Front(id) => Some(id),
            crate::battle_plan_render::OrderRef::Plan(id) => {
                w.plans.plans.get(&id).and_then(|p| p.source_front)
            }
        };
        view.notice = front
            .ok_or_else(|| "Для назначения выберите фронт или связанное наступление".into())
            .and_then(|id| w.assign_front(tag, id, &view.selected))
            .map_or_else(|e| e, |_| "Дивизии назначены на выбранный приказ".into());
    } else {
        let (army, front, plan, members) = match order {
            crate::battle_plan_render::OrderRef::Front(id) => {
                let f = &w.fronts.fronts[&id];
                (
                    f.army,
                    Some(id),
                    w.plans
                        .plans
                        .values()
                        .find(|p| p.source_front == Some(id))
                        .map(|p| p.id),
                    f.assigned_divisions.clone(),
                )
            }
            crate::battle_plan_render::OrderRef::Plan(id) => {
                let p = &w.plans.plans[&id];
                (
                    p.army,
                    p.source_front,
                    Some(id),
                    p.assigned_divisions.clone(),
                )
            }
        };
        view.cancel_tool();
        view.selected_army = Some(army);
        view.selected_front = front;
        view.selected_plan = plan;
        if !add {
            view.selected.clear();
        }
        view.selected.extend(members);
        view.open = false;
        view.notice="Приказ выбран · перетащите белые концы · Del: удалить · Ctrl+клик: назначить выделенные".into();
    }
    view.selected_order = Some(order);
    true
}
pub fn delete_selected(view: &mut ArmyView, w: &mut Warfare, tag: &str) -> Result<(), String> {
    use crate::battle_plan_render::OrderRef;
    let order = view
        .selected_order
        .or(view.selected_plan.map(OrderRef::Plan))
        .or(view.selected_front.map(OrderRef::Front))
        .ok_or("Нажмите на приказ на карте")?;
    match order {
        OrderRef::Front(id) => w.delete_front(tag, id)?,
        OrderRef::Plan(id) => w.delete_plan(tag, id)?,
    }
    view.selected_order = None;
    view.selected_plan = None;
    if matches!(order, OrderRef::Front(_)) {
        view.selected_front = None;
    }
    view.order_map = Default::default();
    Ok(())
}
pub fn begin_handle(view: &mut ArmyView, h: crate::battle_plan_render::Handle, w: &Warfare) {
    use crate::battle_plan_render::OrderRef;
    view.cancel_tool();
    view.order_handle = Some(h);
    view.selected_order = Some(h.order);
    match h.order {
        OrderRef::Front(id) => {
            view.selected_front = Some(id);
            view.plan_tool = Some(if h.start {
                Tool::EditStart
            } else {
                Tool::EditEnd
            });
        }
        OrderRef::Plan(id) => {
            view.selected_plan = Some(id);
            view.plan_tool = Some(Tool::EditPlan);
            view.line_preview = w.plans.plans[&id].destination_line.clone();
        }
    }
}
fn handle_pointer(
    view: &mut ArmyView,
    w: &mut Warfare,
    world: &World,
    tag: &str,
    province: Option<u32>,
    point: MapPosition,
    max_distance: f64,
    inside: bool,
) {
    use crate::battle_plan_render::OrderRef;
    let Some(h) = view.order_handle else {
        return;
    };
    if inside
        && is_mouse_button_down(MouseButton::Left)
        && !is_mouse_button_pressed(MouseButton::Left)
    {
        match h.order {
            OrderRef::Front(id) => {
                if let Some((edge, fixed)) = province
                    .and_then(|p| nearest_edge(world, tag, p, point, max_distance))
                    .zip(h.fixed)
                {
                    let enemy = w.fronts.fronts[&id].enemy_country.clone();
                    if let Ok(path) = w.fronts.boundary(world, tag, &enemy).path(
                        if h.start { edge } else { fixed },
                        if h.start { fixed } else { edge },
                    ) {
                        view.front_preview = path;
                    }
                }
            }
            OrderRef::Plan(id) => {
                let p = &w.plans.plans[&id];
                if let Some(moving) = province {
                    if p.order_type == crate::operations::OrderType::Spearhead {
                        view.line_preview = vec![moving];
                    } else {
                        let fixed = if h.start {
                            *p.destination_line.last().unwrap()
                        } else {
                            p.destination_line[0]
                        };
                        let foreign = if matches!(
                            p.order_type,
                            crate::operations::OrderType::Fallback
                                | crate::operations::OrderType::AreaDefense
                        ) {
                            None
                        } else {
                            p.source_front
                                .and_then(|id| w.fronts.fronts.get(&id))
                                .map(|f| f.enemy_country.as_str())
                                .or_else(|| world.controller_of_province(moving))
                        };
                        if let Ok(path) = crate::operations::province_line(
                            world,
                            if h.start { moving } else { fixed },
                            if h.start { fixed } else { moving },
                            tag,
                            foreign,
                        ) {
                            view.line_preview = path;
                        }
                    }
                }
            }
        }
    }
    if is_mouse_button_released(MouseButton::Left) {
        let result = match h.order {
            OrderRef::Front(id) => {
                if view.front_preview.is_empty() {
                    Ok(())
                } else {
                    w.edit_front_sector(world, tag, id, h.sector, &view.front_preview)
                }
            }
            OrderRef::Plan(id) => {
                let p = &w.plans.plans[&id];
                w.draw_plan(
                    world,
                    tag,
                    p.army,
                    p.source_front,
                    p.order_type,
                    view.line_preview.clone(),
                    Some(id),
                )
                .map(|_| ())
            }
        };
        view.notice = result.map_or_else(|e| e, |_| "Геометрия приказа изменена".into());
        view.cancel_tool();
    }
}
