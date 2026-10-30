//! Single HOI-style army command tree shared by desktop and touch inspectors.
//! Uses the authoritative CommandState; there are no duplicate UI-owned units.
use crate::{
    army_ui::ArmyView,
    command::{ArmyId, CommandState},
    hoi_ui as ui,
    warfare::Warfare,
};
use macroquad::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HierarchyRow {
    Theater(u64),
    Group(u64),
    Army(ArmyId),
}

/// Stable hierarchy for the selected country. Groups belong to theaters,
/// armies belong to groups, and ungrouped armies remain in their theater.
pub fn hierarchy_rows(state: &CommandState, country: &str) -> Vec<HierarchyRow> {
    let mut rows = Vec::new();
    for theater in state.theaters.values().filter(|t| t.country == country) {
        rows.push(HierarchyRow::Theater(theater.id));
        for group in state
            .groups
            .values()
            .filter(|g| g.country == country && g.theater == theater.id)
        {
            rows.push(HierarchyRow::Group(group.id));
            rows.extend(
                state
                    .armies
                    .values()
                    .filter(|a| {
                        a.country == country && a.theater == theater.id && a.group == Some(group.id)
                    })
                    .map(|a| HierarchyRow::Army(a.id)),
            );
        }
        rows.extend(
            state
                .armies
                .values()
                .filter(|a| a.country == country && a.theater == theater.id && a.group.is_none())
                .map(|a| HierarchyRow::Army(a.id)),
        );
    }
    // Older saves or imported malformed hierarchies should not hide armies.
    for army in state.armies.values().filter(|a| a.country == country) {
        if !rows.contains(&HierarchyRow::Army(army.id)) {
            rows.push(HierarchyRow::Army(army.id));
        }
    }
    rows
}

pub fn draw(tag: &str, w: &Warfare, view: &mut ArmyView, rect: Rect) -> f32 {
    const ROW_HEIGHT: f32 = 44.0;
    let heading = Rect::new(rect.x, rect.y, rect.w, 32.);
    ui::section(
        "ТЕАТРЫ  /  ГРУППЫ  /  АРМИИ",
        heading.x,
        heading.y,
        heading.w,
    );
    if ui::small_button(
        if view.hierarchy_collapsed { "+" } else { "−" },
        Rect::new(rect.right() - 28., rect.y + 1., 27., 26.),
        false,
    ) {
        view.hierarchy_collapsed = !view.hierarchy_collapsed;
    }
    if view.hierarchy_collapsed || rect.h < 115. {
        return 36.;
    }
    let height = (rect.h * 0.46).clamp(115., 218.);
    let list = Rect::new(rect.x, rect.y + 34., rect.w, height - 38.);
    let entries = hierarchy_rows(&w.command, tag);
    let visible = (list.h / ROW_HEIGHT).floor().max(1.) as usize;
    let max_scroll = entries.len().saturating_sub(visible);
    view.hierarchy_scroll = view.hierarchy_scroll.min(max_scroll);
    if ui::inside(list) {
        let wheel = mouse_wheel().1;
        if wheel > 0. {
            view.hierarchy_scroll = view.hierarchy_scroll.saturating_sub(wheel.ceil() as usize);
        } else if wheel < 0. {
            view.hierarchy_scroll =
                (view.hierarchy_scroll + (-wheel).ceil() as usize).min(max_scroll);
        }
    }
    let members_by_army = w.command.membership.values().fold(
        std::collections::BTreeMap::<ArmyId, usize>::new(),
        |mut counts, id| {
            *counts.entry(*id).or_default() += 1;
            counts
        },
    );
    {
        let _clip = ui::clip(list);
        for (index, entry) in entries
            .iter()
            .skip(view.hierarchy_scroll)
            .take(visible + 1)
            .enumerate()
        {
            let r = Rect::new(
                list.x,
                list.y + index as f32 * ROW_HEIGHT,
                list.w,
                ROW_HEIGHT - 3.,
            );
            let (selected, indent, title, detail, portrait, army_color, destination) = match *entry
            {
                HierarchyRow::Theater(id) => {
                    let t = &w.command.theaters[&id];
                    let count = w
                        .command
                        .armies
                        .values()
                        .filter(|a| a.country == tag && a.theater == id)
                        .count();
                    let target = w
                        .command
                        .armies
                        .values()
                        .find(|a| a.country == tag && a.theater == id)
                        .map(|a| a.id);
                    (
                        false,
                        3.,
                        t.name.clone(),
                        format!("{count} арм."),
                        None,
                        None,
                        target,
                    )
                }
                HierarchyRow::Group(id) => {
                    let g = &w.command.groups[&id];
                    let armies: Vec<_> = w
                        .command
                        .armies
                        .values()
                        .filter(|a| a.country == tag && a.group == Some(id))
                        .collect();
                    let target = armies.first().map(|a| a.id);
                    let chosen = view
                        .selected_army
                        .is_some_and(|a| armies.iter().any(|other| other.id == a));
                    let commander = g
                        .field_marshal
                        .as_deref()
                        .and_then(|id| w.command.commanders.get(id));
                    (
                        chosen,
                        12.,
                        g.name.clone(),
                        format!(
                            "{} · {} арм.",
                            commander
                                .map_or("Без фельдмаршала", |c| c.name.as_str()),
                            armies.len()
                        ),
                        g.field_marshal.clone(),
                        None,
                        target,
                    )
                }
                HierarchyRow::Army(id) => {
                    let a = &w.command.armies[&id];
                    let commander = a
                        .general
                        .as_deref()
                        .and_then(|id| w.command.commanders.get(id));
                    let count = members_by_army.get(&id).copied().unwrap_or(0);
                    let cap = commander
                        .and_then(|c| w.command.army_cap(c, id))
                        .map_or("?", |c| if c == 0 { "∞" } else { "" });
                    let detail = if cap == "" {
                        format!(
                            "{count}/{} див. · {}",
                            commander
                                .and_then(|c| w.command.army_cap(c, id))
                                .unwrap_or(0),
                            commander.map_or("Без генерала", |c| c.name.as_str())
                        )
                    } else {
                        format!(
                            "{count}/{cap} див. · {}",
                            commander.map_or("Без генерала", |c| c.name.as_str())
                        )
                    };
                    (
                        view.selected_army == Some(id),
                        23.,
                        a.name.clone(),
                        detail,
                        a.general.clone(),
                        Some(a.color),
                        Some(id),
                    )
                }
            };
            if ui::small_button("", r, selected) {
                if let Some(id) = destination {
                    view.selected_army = Some(id);
                    view.selected = w.command.members(id).collect();
                    view.selected_front = None;
                    view.selected_plan = None;
                    view.selected_order = None;
                    view.army_scroll = 0.;
                    view.edit_army_name = false;
                } else {
                    view.notice = "В этой группе или театре ещё нет армий".into();
                }
            }
            let px = r.x + indent + 4.;
            if let Some(rgb) = army_color {
                draw_rectangle(
                    px,
                    r.y + 6.,
                    3.,
                    r.h - 12.,
                    Color::from_rgba(rgb[0], rgb[1], rgb[2], 255),
                );
            }
            let image = Rect::new(px + 5., r.y + 4., 30., 33.);
            if let Some(id) = portrait.as_deref() {
                if !crate::hoi_assets::portrait(id, image) {
                    crate::ui_icons::draw(crate::ui_icons::Icon::Officer, image, ui::MUTED);
                }
            } else {
                crate::ui_icons::draw(crate::ui_icons::Icon::Army, image, ui::GOLD);
            }
            let text_x = image.right() + 4.;
            ui::label(
                &title,
                Rect::new(text_x, r.y + 3., (r.right() - text_x - 4.).max(4.), 16.),
                12.,
                if selected { ui::GOLD } else { ui::TEXT },
            );
            ui::label(
                &detail,
                Rect::new(text_x, r.y + 22., (r.right() - text_x - 4.).max(4.), 15.),
                10.,
                ui::MUTED,
            );
            ui::tooltip(r, &title, &detail);
        }
    }
    if entries.len() > visible {
        let controls_y = rect.y + height - 3.;
        let bw = (rect.w - 5.) * 0.5;
        if ui::small_button("▲", Rect::new(rect.x, controls_y, bw, 24.), false) {
            view.hierarchy_scroll = view.hierarchy_scroll.saturating_sub(visible);
        }
        if ui::small_button("▼", Rect::new(rect.x + bw + 5., controls_y, bw, 24.), false) {
            view.hierarchy_scroll = (view.hierarchy_scroll + visible).min(max_scroll);
        }
        height + 29.
    } else {
        height + 4.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_country_scoped_theater_group_army_tree() {
        let mut state = CommandState::default();
        let first = state.create_army("GER", "First", &[1, 2]);
        let group = state.create_group("GER", first, "North").unwrap();
        let second = state.create_army("GER", "Second", &[3]);
        state.set_group("GER", second, Some(group)).unwrap();
        let third = state.create_army("GER", "Third", &[4]);
        let foreign = state.create_army("FRA", "French", &[5]);
        let theater = state.armies[&first].theater;
        assert_eq!(
            hierarchy_rows(&state, "GER"),
            vec![
                HierarchyRow::Theater(theater),
                HierarchyRow::Group(group),
                HierarchyRow::Army(first),
                HierarchyRow::Army(second),
                HierarchyRow::Army(third),
            ]
        );
        assert!(!hierarchy_rows(&state, "GER").contains(&HierarchyRow::Army(foreign)));
    }
    #[test]
    fn moving_army_to_group_keeps_theater_and_membership_consistent() {
        let mut state = CommandState::default();
        let lead = state.create_army("GER", "Lead", &[1]);
        let group = state.create_group("GER", lead, "North").unwrap();
        let other = state.create_army("GER", "Other", &[2, 3]);
        let east = state.create_theater("GER", "East");
        state.set_theater("GER", other, east).unwrap();
        state.set_group("GER", other, Some(group)).unwrap();
        assert_eq!(state.armies[&other].theater, state.groups[&group].theater);
        assert_eq!(state.members(other).collect::<Vec<_>>(), vec![2, 3]);
        assert_eq!(
            hierarchy_rows(&state, "GER")
                .iter()
                .filter(|row| **row == HierarchyRow::Army(other))
                .count(),
            1
        );
    }

}
