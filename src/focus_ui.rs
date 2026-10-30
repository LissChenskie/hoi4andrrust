//! Cached imported national-focus graph. Rendering never grants rewards.
use crate::{
    campaign_ui::{Hud, Panel},
    hoi_ui as ui, pdx,
    research::{ResearchCatalogue, ResearchRules},
    session::GameSession,
};
use macroquad::prelude::*;
use std::collections::BTreeMap;
#[derive(Debug, Clone)]
pub enum Request {
    Begin(String),
    Cancel,
}
#[derive(Debug, Clone)]
struct Node {
    id: String,
    name: String,
    description: String,
    icon: String,
    position: Vec2,
    reason: Option<String>,
    done: bool,
    active: bool,
    bypassed: bool,
    duration: String,
    reward: String,
}
#[derive(Debug, Clone)]
struct Link {
    from: usize,
    to: usize,
    alternative: bool,
    exclusive: bool,
}
#[derive(Debug, Default)]
pub struct View {
    stamp: String,
    tree: String,
    error: Option<String>,
    nodes: Vec<Node>,
    links: Vec<Link>,
    pub selected: Option<String>,
    pan: Vec2,
    zoom: f32,
    centered: bool,
    drag: Option<(Vec2, Vec2)>,
    dragged: bool,
    pinch: Option<(u64, u64, Vec2, f32)>,
    active_label: String,
    progress: f32,
    cancelable: bool,
    reward_tab: bool,
    detail_scroll: f32,
    detail_drag: Option<(Vec2, f32)>,
}
fn plain(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '§' {
            chars.next();
        } else {
            out.push(ch);
        }
    }
    out
}
fn summary(b: &[pdx::Entry], out: &mut String, depth: usize) {
    if depth > 5 {
        out.push_str("…\n");
        return;
    }
    for e in b {
        if out.len() > 2000 {
            out.push('…');
            break;
        }
        let name = match e.key.as_str() {
            "add_political_power" => "Политическая власть",
            "add_stability" => "Стабильность",
            "add_war_support" => "Поддержка войны",
            "add_manpower" => "Людские ресурсы",
            "add_research_slot" => "Ячейки исследований",
            k => k,
        };
        for _ in 0..depth {
            out.push_str("  ");
        }
        out.push_str(name);
        if let Some(v) = e.atom() {
            out.push_str(": ");
            out.push_str(v);
            out.push('\n');
        } else if let Some(b) = e.block() {
            out.push_str(":\n");
            summary(b, out, depth + 1);
        }
    }
}
/// Rebuild condition/status presentation only when simulation/country/content
/// changes. Coordinates/links and text are retained between rendered frames.
pub fn prepare(
    session: &GameSession,
    cat: &ResearchCatalogue,
    rules: &ResearchRules,
    view: &mut View,
) {
    let Some(tag) = session.player_country.as_deref() else {
        view.error = Some("Выберите страну".into());
        view.stamp.clear();
        return;
    };
    let revision = session.research.country(tag).map_or(0, |c| c.revision);
    let stamp = format!(
        "{tag}:{}:{}:{revision}:{:?}:{:?}:{:?}:{}",
        cat.fingerprint,
        session.clock.elapsed_hours,
        session.campaign.focuses.selected.get(tag),
        session.campaign.focuses.layout_trees.get(tag),
        session.campaign.focuses.active.get(tag),
        session.campaign.focuses.layout_revision
    );
    if view.stamp == stamp {
        return;
    }
    view.stamp = stamp;
    let result = (|| -> Result<(), String> {
        let tree = session.focus_tree(tag, cat)?;
        if !tree.problems.is_empty() {
            return Err(tree.problems.join("; "));
        }
        if view.tree != tree.id {
            view.centered = false;
            view.selected = None;
        }
        view.tree = tree.id.clone();
        let positions = session
            .campaign
            .focuses
            .positions
            .get(tag)
            .ok_or("Разметка дерева не загружена")?;
        view.nodes.clear();
        view.links.clear();
        let c = session
            .research
            .country(tag)
            .ok_or("Country history missing")?;
        let mut indexes = BTreeMap::new();
        for id in &tree.focuses {
            let src = cat
                .scripts
                .sources
                .get(&format!("focus:{id}"))
                .ok_or("Focus source missing")?;
            if session
                .campaign
                .focuses
                .layouts
                .get(tag)
                .and_then(|v| v.get(id))
                == Some(&false)
            {
                continue;
            }
            if let Some(b) = pdx::block(&src.raw, "visible") {
                if !session.campaign_conditions(
                    &crate::campaign::Context::country(tag),
                    b,
                    cat,
                    rules,
                )? {
                    continue;
                }
            }
            let key = pdx::atom(&src.raw, "text").unwrap_or(id);
            let name = plain(
                cat.scripts
                    .localisation
                    .get(key)
                    .map(String::as_str)
                    .unwrap_or(key),
            );
            let description = plain(
                cat.scripts
                    .localisation
                    .get(&format!("{key}_desc"))
                    .map(String::as_str)
                    .unwrap_or("Описание отсутствует в импортированных данных"),
            );
            let mut reward = String::new();
            summary(
                pdx::block(&src.raw, "completion_reward").unwrap_or(&[]),
                &mut reward,
                0,
            );
            let required = pdx::atom(&src.raw, "cost")
                .and_then(|n| n.parse::<f64>().ok())
                .zip(
                    session
                        .military
                        .catalogue
                        .literal_defines
                        .get("NDefines.NFocus.FOCUS_POINT_DAYS"),
                )
                .map(|(a, b)| (a * b).floor());
            let (x, y) = positions[id];
            let mut icon = pdx::atom(&src.raw, "icon")
                .map(str::to_owned)
                .unwrap_or_default();
            if let Some(b) = pdx::block(&src.raw, "icon") {
                for e in b {
                    if e.atom() == Some("yes")
                        || e.block().is_some_and(|b| {
                            session.campaign_conditions(
                                &crate::campaign::Context::country(tag),
                                b,
                                cat,
                                rules,
                            ) == Ok(true)
                        })
                    {
                        icon = e.key.clone();
                        break;
                    }
                }
            }
            indexes.insert(id.clone(), view.nodes.len());
            view.nodes.push(Node {
                id: id.clone(),
                name,
                description,
                icon,
                position: vec2(x * 90., y * 110.),
                reason: if let Some(active) = session
                    .campaign
                    .focuses
                    .active
                    .get(tag)
                    .filter(|a| a.id == *id)
                {
                    active.blocked.clone()
                } else {
                    session.focus_blocked_reason(tag, id, cat, rules).err()
                },
                done: c.context.completed_focuses.contains(id),
                active: session
                    .campaign
                    .focuses
                    .active
                    .get(tag)
                    .is_some_and(|a| a.id == *id),
                bypassed: session
                    .campaign
                    .focuses
                    .bypassed
                    .get(tag)
                    .is_some_and(|b| b.contains(id)),
                duration: required
                    .map(|n| format!("{n:.0} дн."))
                    .unwrap_or_else(|| "Длительность неизвестна".into()),
                reward,
            });
        }
        for (id, &to) in &indexes {
            let raw = &cat.scripts.sources[&format!("focus:{id}")].raw;
            for group in raw
                .iter()
                .filter(|e| e.key == "prerequisite")
                .filter_map(pdx::Entry::block)
            {
                let parents: Vec<_> = group
                    .iter()
                    .filter(|e| e.key == "focus")
                    .filter_map(pdx::Entry::atom)
                    .collect();
                for parent in &parents {
                    if let Some(&from) = indexes.get(*parent) {
                        view.links.push(Link {
                            from,
                            to,
                            alternative: parents.len() > 1,
                            exclusive: false,
                        });
                    }
                }
            }
            for other in hoi4_mobile::focus_data::references(raw, "mutually_exclusive") {
                if let Some(&from) = indexes.get(other) {
                    if from < to {
                        view.links.push(Link {
                            from,
                            to,
                            alternative: false,
                            exclusive: true,
                        });
                    }
                }
            }
        }
        let active = session.campaign.focuses.active.get(tag);
        view.progress = active.map_or(0., |a| (a.progress / a.required) as f32);
        view.active_label = active
            .map(|a| {
                format!(
                    "{} · {:.0}/{:.0} дн.",
                    view.nodes
                        .iter()
                        .find(|n| n.id == a.id)
                        .map(|n| n.name.as_str())
                        .unwrap_or(&a.id),
                    a.progress,
                    a.required
                )
            })
            .unwrap_or_else(|| "Национальный фокус не выбран".into());
        view.cancelable = active.is_some_and(|a| !a.manual_cancel_forbidden);
        if !view.centered {
            let initial = pdx::block(&tree.raw, "initial_show_position");
            let pos = initial
                .and_then(|b| pdx::atom(b, "focus"))
                .and_then(|id| positions.get(id))
                .copied()
                .or_else(|| {
                    initial.map(|b| {
                        (
                            pdx::number(b, "x").unwrap_or(0.) as f32,
                            pdx::number(b, "y").unwrap_or(0.) as f32,
                        )
                    })
                })
                .map(|(x, y)| vec2(x * 90., y * 110.))
                .or_else(|| view.nodes.first().map(|n| n.position))
                .unwrap_or(Vec2::ZERO);
            view.zoom = 0.8;
            view.pan = -pos * view.zoom;
            view.centered = true;
        }
        Ok(())
    })();
    view.error = result.err();
}
fn node_rect(node: &Node, area: Rect, view: &View) -> Rect {
    let center = vec2(area.x + area.w * 0.5, area.y + 72.) + view.pan + node.position * view.zoom;
    Rect::new(
        center.x - 67. * view.zoom,
        center.y - 36. * view.zoom,
        134. * view.zoom,
        92. * view.zoom,
    )
}
fn zoom_at(view: &mut View, area: Rect, point: Vec2, factor: f32) {
    let old = view.zoom;
    let next = (old * factor).clamp(0.35, 1.75);
    let origin = vec2(area.x + area.w * 0.5, area.y + 72.);
    view.pan = point - origin - (point - origin - view.pan) * (next / old);
    view.zoom = next;
}
fn gestures(view: &mut View, area: Rect) -> Option<Vec2> {
    let touches: Vec<_> = touches()
        .into_iter()
        .filter(|t| !matches!(t.phase, TouchPhase::Ended | TouchPhase::Cancelled))
        .collect();
    if touches.len() >= 2 {
        let (a, b) = (&touches[0], &touches[1]);
        let center = (a.position + b.position) * 0.5;
        let distance = a.position.distance(b.position).max(1.);
        if area.contains(center) {
            if let Some((id1, id2, last, dist)) = view.pinch {
                if id1 == a.id && id2 == b.id {
                    view.pan += center - last;
                    zoom_at(view, area, center, distance / dist);
                }
            }
            view.pinch = Some((a.id, b.id, center, distance));
        }
        view.drag = None;
        view.dragged = true;
        return None;
    }
    if view.pinch.take().is_some() {
        view.drag = None;
        view.dragged = true;
        return None;
    }
    let point = Vec2::from(mouse_position());
    if ui::inside(area) {
        let wheel = mouse_wheel().1;
        if wheel != 0. {
            zoom_at(view, area, point, (wheel * 0.12).exp());
        }
        if is_mouse_button_pressed(MouseButton::Left) {
            view.drag = Some((point, view.pan));
            view.dragged = false;
        }
    }
    if let Some((origin, pan)) = view.drag {
        if is_mouse_button_down(MouseButton::Left) {
            if point.distance(origin) > 6. {
                view.dragged = true;
            }
            if view.dragged {
                view.pan = pan + point - origin;
            }
        }
        if is_mouse_button_released(MouseButton::Left) {
            view.drag = None;
            if !view.dragged && area.contains(point) {
                return Some(point);
            }
        }
    }
    None
}
pub fn draw(hud: &mut Hud) {
    if hud.active != Some(Panel::Focus) {
        return;
    }
    let layout = crate::ui_layout::Layout::current().wide();
    let area = Rect::new(
        layout.x + 12.,
        layout.y + 86.,
        layout.w - 24.,
        (layout.h - 100.).max(1.),
    );
    let view = &mut hud.focus_view;
    if let Some(e) = &view.error {
        ui::paragraph(e, area, 14., ui::MUTED);
        return;
    }
    ui::label(
        &view.active_label,
        Rect::new(area.x, area.y - 32., area.w - 112., 22.),
        13.,
        ui::GOLD,
    );
    if ui::small_button(
        "ОТМЕНА",
        Rect::new(area.right() - 102., area.y - 32., 98., 25.),
        false,
    ) && view.cancelable
    {
        hud.focus_request = Some(Request::Cancel);
    }
    ui::progress_bar(Rect::new(area.x, area.y - 7., area.w, 4.), view.progress);
    let has_detail = view
        .selected
        .as_ref()
        .is_some_and(|id| view.nodes.iter().any(|n| &n.id == id));
    let side = has_detail && area.w > 760.;
    let detail_h = if area.h < 400. { area.h * 0.48 } else { 210. };
    let graph = if side {
        Rect::new(area.x, area.y, area.w - 290., area.h)
    } else if has_detail {
        Rect::new(area.x, area.y, area.w, (area.h - detail_h - 5.).max(50.))
    } else {
        area
    };
    let clicked = gestures(view, graph);
    draw_rectangle(
        graph.x,
        graph.y,
        graph.w,
        graph.h,
        Color::new(0.075, 0.09, 0.095, 1.),
    );
    {
        let _clip = ui::clip(graph);
        for link in &view.links {
            let from = node_rect(&view.nodes[link.from], graph, view);
            let to = node_rect(&view.nodes[link.to], graph, view);
            let color = if link.exclusive {
                Color::new(0.72, 0.29, 0.25, 1.)
            } else if view.nodes[link.from].done {
                ui::GREEN
            } else {
                ui::MUTED
            };
            let a = vec2(from.x + from.w * 0.5, from.bottom());
            let b = vec2(to.x + to.w * 0.5, to.y);
            if link.exclusive {
                draw_line(
                    a.x,
                    a.y - 45. * view.zoom,
                    b.x,
                    b.y + 30. * view.zoom,
                    1.5,
                    color,
                );
            } else {
                let mid = (a.y + b.y) * 0.5;
                draw_line(a.x, a.y, a.x, mid, 1.4, color);
                draw_line(a.x, mid, b.x, mid, 1.4, color);
                draw_line(b.x, mid, b.x, b.y, 1.4, color);
                if link.alternative {
                    ui::label(
                        "ИЛИ",
                        Rect::new(b.x - 16., mid - 15., 32., 14.),
                        9. * view.zoom,
                        ui::GOLD,
                    );
                }
            }
        }
        for n in &view.nodes {
            let r = node_rect(n, graph, view);
            if !r.overlaps(&graph) {
                continue;
            }
            let selected = view.selected.as_deref() == Some(&n.id);
            let color = if n.done {
                ui::GREEN
            } else if n.active || n.reason.is_none() {
                ui::GOLD
            } else {
                ui::MUTED
            };
            let icon = Rect::new(
                r.x + r.w * 0.5 - 27. * view.zoom,
                r.y,
                54. * view.zoom,
                54. * view.zoom,
            );
            draw_rectangle(
                icon.x - 3.,
                icon.y - 3.,
                icon.w + 6.,
                icon.h + 6.,
                Color::new(0.16, 0.18, 0.18, 1.),
            );
            draw_rectangle_lines(
                icon.x - 3.,
                icon.y - 3.,
                icon.w + 6.,
                icon.h + 6.,
                if selected { 3. } else { 1. },
                color,
            );
            if !crate::hoi_assets::draw(
                "focus",
                n.icon.strip_prefix("GFX_").unwrap_or(&n.icon),
                icon,
            ) {
                crate::ui_icons::draw(crate::ui_icons::Icon::Focus, icon, color);
            }
            ui::paragraph(
                &n.name,
                Rect::new(r.x, icon.bottom() + 5., r.w, 33. * view.zoom),
                11. * view.zoom,
                color,
            );
            if clicked.is_some_and(|p| r.contains(p)) {
                if view.selected.as_deref() != Some(&n.id) {
                    view.detail_scroll = 0.;
                    view.reward_tab = false;
                }
                view.selected = Some(n.id.clone());
            }
            ui::tooltip(r, &n.name, &n.description);
        }
    }
    if let Some(id) = view.selected.as_deref() {
        if let Some(n) = view.nodes.iter().find(|n| n.id == id) {
            let detail = if side {
                Rect::new(graph.right() + 8., area.y, 282., area.h)
            } else {
                Rect::new(
                    area.x,
                    graph.bottom() + 5.,
                    area.w,
                    area.bottom() - graph.bottom() - 5.,
                )
            };
            draw_rectangle(
                detail.x,
                detail.y,
                detail.w,
                detail.h,
                Color::new(0.14, 0.16, 0.16, 1.),
            );
            ui::label(
                &n.name,
                Rect::new(detail.x + 8., detail.y + 6., detail.w - 40., 23.),
                14.,
                ui::GOLD,
            );
            if ui::small_button(
                "×",
                Rect::new(detail.right() - 30., detail.y + 3., 26., 24.),
                false,
            ) {
                view.selected = None;
            }
            let status = if n.bypassed {
                "Обойдён"
            } else if n.done {
                "Завершён"
            } else if n.active {
                if n.reason.is_some() {
                    "Приостановлен"
                } else {
                    "Выполняется"
                }
            } else {
                &n.duration
            };
            ui::label(
                status,
                Rect::new(detail.x + 8., detail.y + 33., detail.w - 16., 18.),
                12.,
                ui::TEXT,
            );
            let half = (detail.w - 22.) * 0.5;
            if ui::small_button(
                "ОПИСАНИЕ",
                Rect::new(detail.x + 8., detail.y + 55., half, 24.),
                !view.reward_tab,
            ) {
                view.reward_tab = false;
                view.detail_scroll = 0.;
            }
            if ui::small_button(
                "РЕЗУЛЬТАТ",
                Rect::new(detail.x + 14. + half, detail.y + 55., half, 24.),
                view.reward_tab,
            ) {
                view.reward_tab = true;
                view.detail_scroll = 0.;
            }
            let text = if view.reward_tab {
                n.reward.clone()
            } else {
                match &n.reason {
                    Some(r) if !n.done => format!(
                        "{}\n\n{}: {r}",
                        n.description,
                        if n.active {
                            "Приостановлен"
                        } else {
                            "Недоступно"
                        }
                    ),
                    _ => n.description.clone(),
                }
            };
            let body = Rect::new(
                detail.x + 8.,
                detail.y + 84.,
                detail.w - 16.,
                (detail.h - 128.).max(1.),
            );
            let text_h = ui::paragraph_height(&text, body.w, 12.).max(body.h);
            let max = (text_h - body.h).max(0.);
            let point = Vec2::from(mouse_position());
            if ui::inside(body) {
                view.detail_scroll = (view.detail_scroll - mouse_wheel().1 * 30.).clamp(0., max);
                if is_mouse_button_pressed(MouseButton::Left) {
                    view.detail_drag = Some((point, view.detail_scroll));
                }
            }
            if let Some((start, scroll)) = view.detail_drag {
                if is_mouse_button_down(MouseButton::Left) {
                    view.detail_scroll = (scroll + start.y - point.y).clamp(0., max);
                } else {
                    view.detail_drag = None;
                }
            }
            view.detail_scroll = view.detail_scroll.clamp(0., max);
            {
                let _clip = ui::clip(body);
                ui::paragraph(
                    &text,
                    Rect::new(body.x, body.y - view.detail_scroll, body.w, text_h),
                    12.,
                    ui::TEXT,
                );
            }
            let button = Rect::new(detail.x + 8., detail.bottom() - 37., detail.w - 16., 29.);
            if n.reason.is_none() && !n.active && !n.done {
                if ui::small_button("НАЧАТЬ ФОКУС", button, false) {
                    hud.focus_request = Some(Request::Begin(n.id.clone()));
                }
            } else {
                ui::disabled(
                    if n.done {
                        "ЗАВЕРШЁН"
                    } else if n.active {
                        "ВЫПОЛНЯЕТСЯ"
                    } else {
                        "НЕДОСТУПНО"
                    },
                    button,
                    n.reason.as_deref().unwrap_or(status),
                );
            }
        }
    }
    ui::label(
        "Перемещение: потяните дерево · Масштаб: колесо / два пальца",
        Rect::new(graph.x + 6., graph.bottom() - 17., graph.w - 12., 16.),
        10.,
        ui::MUTED,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zoom_preserves_world_point_under_finger_and_clamps() {
        let mut v = View {
            zoom: 1.,
            pan: vec2(13., 29.),
            ..Default::default()
        };
        let area = Rect::new(20., 50., 800., 400.);
        let p = vec2(200., 170.);
        let origin = vec2(420., 122.);
        let before = (p - origin - v.pan) / v.zoom;
        zoom_at(&mut v, area, p, 1.5);
        assert!(((p - origin - v.pan) / v.zoom - before).length() < 0.001);
        zoom_at(&mut v, area, p, 100.);
        assert_eq!(v.zoom, 1.75);
        zoom_at(&mut v, area, p, 0.001);
        assert_eq!(v.zoom, 0.35);
        assert_eq!(plain("§GЗелёный§!"), "Зелёный");
    }
    #[test]
    fn prepared_view_uses_persisted_offsets_and_explicit_layout_invalidation() {
        let (mut s, mut cat, rules) = crate::focus::tests::prepared();
        s.player_country = Some("A".into());
        cat.scripts
            .sources
            .get_mut("focus:first")
            .unwrap()
            .raw
            .extend(pdx::parse_pdx("x=2 offset={x=3 trigger={has_country_flag=shift}}").unwrap());
        s.refresh_focus_layout("A", &cat, &rules).unwrap();
        let mut view = View::default();
        prepare(&s, &cat, &rules, &mut view);
        assert!(view.error.is_none(), "{:?}", view.error);
        assert_eq!(
            view.nodes
                .iter()
                .find(|n| n.id == "first")
                .unwrap()
                .position
                .x,
            180.
        );
        let stamp = view.stamp.clone();
        view.pan = vec2(12., 34.);
        prepare(&s, &cat, &rules, &mut view);
        assert_eq!(view.stamp, stamp);
        assert_eq!(view.pan, vec2(12., 34.));
        s.campaign_effects(
            &crate::campaign::Context::country("A"),
            &pdx::parse_pdx("set_country_flag=shift mark_focus_tree_layout_dirty=yes").unwrap(),
            "authored",
            &cat,
            &rules,
        )
        .unwrap();
        s.refresh_focus_layout("A", &cat, &rules).unwrap();
        prepare(&s, &cat, &rules, &mut view);
        assert_eq!(
            view.nodes
                .iter()
                .find(|n| n.id == "first")
                .unwrap()
                .position
                .x,
            450.
        );
        let before = view.stamp.clone();
        s.campaign_effects(
            &crate::campaign::Context::country("A"),
            &pdx::parse_pdx("mark_focus_tree_layout_dirty=yes").unwrap(),
            "authored",
            &cat,
            &rules,
        )
        .unwrap();
        s.refresh_focus_layout("A", &cat, &rules).unwrap();
        prepare(&s, &cat, &rules, &mut view);
        assert_ne!(view.stamp, before);
        s.begin_focus("A", "first", &cat, &rules).unwrap();
        prepare(&s, &cat, &rules, &mut view);
        assert!(view.nodes.iter().find(|n| n.id == "first").unwrap().active);
        assert!(
            view.nodes
                .iter()
                .find(|n| n.id == "second")
                .unwrap()
                .reason
                .is_some()
        );
    }
}
