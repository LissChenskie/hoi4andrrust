//! Original procedural research panel. Displays the imported tech DAG,
//! conditions, multiple parallel slots and day-step ETA without bundling art.
use crate::game::GameDate;
use crate::hoi_ui as ui;
use crate::research::{CountryResearch, ResearchCatalogue, ResearchRules, ResearchWorld};
use crate::{draw_ui_text, measure_ui_text};
use macroquad::prelude::*;

#[derive(Debug, Default)]
pub struct ResearchView {
    pub open: bool,
    pub browsing: bool,
    pub slot_scroll: usize,
    pub tree_pan: Vec2,
    pub drag_last: Option<Vec2>,
    pub folder_index: usize,
    pub selected: Option<String>,
    pub scroll_rows: usize,
    pub scroll_columns: usize,
    pub preferred_slot: Option<usize>,
    pub notice: String,
}

pub fn bounds(view: &ResearchView) -> Rect {
    let layout = crate::ui_layout::Layout::current();
    if view.browsing {
        layout.wide()
    } else {
        layout.primary()
    }
}

pub fn is_over_panel(point: Vec2, view: &ResearchView) -> bool {
    view.open && bounds(view).contains(point)
}
fn fit(text: &str, width: f32, size: u16) -> String {
    let mut s = text.to_owned();
    while !s.is_empty() && measure_ui_text(&s, size).width > width {
        s.pop();
    }
    s
}
fn folder_label(folder: &str) -> (&str, crate::ui_icons::Icon) {
    use crate::ui_icons::Icon;
    if folder.contains("doctrine") {
        ("Доктрины", Icon::Officer)
    } else if folder.contains("infantry") {
        ("Пехота", Icon::Army)
    } else if folder.contains("support") {
        ("Поддержка", Icon::Supply)
    } else if folder.contains("artillery") {
        ("Артиллерия", Icon::Army)
    } else if folder.contains("armour") || folder.contains("armor") {
        ("Бронетехника", Icon::Army)
    } else if folder.contains("naval") || folder.contains("ship") {
        ("Флот", Icon::Navy)
    } else if folder.contains("air") {
        ("Авиация", Icon::Air)
    } else if folder.contains("industry") {
        ("Промышленность", Icon::Factory)
    } else if folder.contains("engineer") || folder.contains("electronic") {
        ("Электроника", Icon::Research)
    } else {
        (folder, Icon::Research)
    }
}
fn label_color(tech_id: &str, country: &CountryResearch, catalogue: &ResearchCatalogue) -> Color {
    if country.researched.contains(tech_id) {
        ui::GREEN
    } else if country
        .slots
        .iter()
        .any(|s| s.active.as_ref().is_some_and(|a| a.tech_id == tech_id))
    {
        ui::GOLD
    } else if catalogue.can_research(tech_id, country).is_ok() {
        ui::TEXT
    } else {
        ui::MUTED
    }
}

fn draw_slots(
    tag: Option<&str>,
    research: &mut ResearchWorld,
    catalogue: &ResearchCatalogue,
    rules: &ResearchRules,
    date: GameDate,
    view: &mut ResearchView,
) {
    let r = bounds(view);
    ui::panel(r.x, r.y, r.w, r.h);
    ui::header(r.x + 3.0, r.y + 3.0, r.w - 6.0, "ИССЛЕДОВАНИЯ");
    if ui::small_button(
        "×",
        Rect::new(r.right() - 35.0, r.y + 7.0, 27.0, 25.0),
        false,
    ) {
        view.open = false;
        return;
    }
    let Some(tag) = tag else {
        return;
    };
    let Some(country) = research.country(tag).cloned() else {
        return;
    };
    let active = country.slots.iter().filter(|s| s.active.is_some()).count();
    ui::label(
        &format!("Занято ячеек: {active} / {}", country.slots.len()),
        Rect::new(r.x + 12.0, r.y + 47.0, r.w - 24.0, 22.0),
        14.0,
        ui::GOLD,
    );
    let list = Rect::new(r.x + 10.0, r.y + 82.0, r.w - 20.0, r.h - 142.0);
    let rows = (list.h / 86.0).max(1.0) as usize;
    let max = country.slots.len().saturating_sub(rows);
    view.slot_scroll = view.slot_scroll.min(max);
    if ui::inside(list) {
        let (_, v) = mouse_wheel();
        if v > 0.0 {
            view.slot_scroll = view.slot_scroll.saturating_sub(1);
        } else if v < 0.0 {
            view.slot_scroll = (view.slot_scroll + 1).min(max);
        }
    }
    ui::scrollbar(list, &mut view.slot_scroll, max, rows);
    let mut cancel = None;
    for (i, slot) in country
        .slots
        .iter()
        .enumerate()
        .skip(view.slot_scroll)
        .take(rows)
    {
        let row = Rect::new(
            list.x,
            list.y + (i - view.slot_scroll) as f32 * 86.0,
            list.w,
            79.0,
        );
        ui::row_bg(row, slot.active.is_some(), i % 2 == 1);
        crate::ui_icons::draw(
            crate::ui_icons::Icon::Research,
            Rect::new(row.x + 12.0, row.y + 17.0, 37.0, 39.0),
            if slot.active.is_some() {
                ui::GOLD
            } else {
                ui::MUTED
            },
        );
        let tx = row.x + 64.0;
        let tw = row.w - 105.0;
        if let Some(a) = &slot.active {
            let tech = catalogue.technologies.get(&a.tech_id);
            let name = tech.map_or(a.tech_id.as_str(), |t| t.name.as_str());
            let total = tech.map_or(1.0, |t| t.research_cost * rules.base_tech_cost);
            ui::label(name, Rect::new(tx, row.y + 9.0, tw, 21.0), 14.0, ui::TEXT);
            let eta = country
                .estimated_days(i, catalogue, rules, date)
                .map_or("—".into(), |d| format!("{d} дн."));
            ui::label(
                &format!("До завершения: {eta}"),
                Rect::new(tx, row.y + 34.0, tw, 18.0),
                11.0,
                ui::MUTED,
            );
            ui::progress_bar(
                Rect::new(tx, row.bottom() - 15.0, tw, 7.0),
                (a.progress / total) as f32,
            );
            let stop = ui::small_button(
                "×",
                Rect::new(row.right() - 30.0, row.y + 8.0, 23.0, 23.0),
                false,
            );
            if stop {
                cancel = Some(i);
            } else if ui::inside(row) && is_mouse_button_pressed(MouseButton::Left) {
                view.preferred_slot = Some(i);
                view.selected = Some(a.tech_id.clone());
                view.browsing = true;
                view.scroll_rows = 0;
            }
        } else {
            ui::label(
                "Выбрать исследование",
                Rect::new(tx, row.y + 15.0, tw, 23.0),
                15.0,
                ui::GOLD,
            );
            ui::label(
                &format!(
                    "Сохранено: {:.0} / {:.0} дн.",
                    slot.saved_days, rules.max_saved_days
                ),
                Rect::new(tx, row.y + 44.0, tw, 20.0),
                11.0,
                ui::MUTED,
            );
            if ui::inside(row) && is_mouse_button_pressed(MouseButton::Left) {
                view.preferred_slot = Some(i);
                view.browsing = true;
                view.scroll_rows = 0;
            }
        }
        ui::tooltip(
            row,
            "Исследовательская ячейка",
            "Нажмите, чтобы открыть дерево технологий и выбрать исследование.",
        );
    }
    if let Some(i) = cancel {
        if let Some(country) = research.countries.get_mut(tag) {
            country.cancel(i);
        }
    }
    if ui::small_button(
        "Открыть дерево технологий",
        Rect::new(r.x + 12.0, r.bottom() - 46.0, r.w - 24.0, 31.0),
        false,
    ) {
        view.browsing = true;
        view.scroll_rows = 0;
    }
}

pub fn draw(
    tag: Option<&str>,
    research: &mut ResearchWorld,
    catalogue: &ResearchCatalogue,
    rules: &ResearchRules,
    date: GameDate,
    view: &mut ResearchView,
) {
    if !view.open {
        return;
    }
    if !view.browsing {
        draw_slots(tag, research, catalogue, rules, date, view);
        return;
    }
    let r = bounds(view);
    ui::panel(r.x, r.y, r.w, r.h);
    ui::header(r.x + 3.0, r.y + 3.0, r.w - 6.0, "ТЕХНОЛОГИИ");
    if ui::small_button(
        "X",
        Rect::new(r.x + r.w - 36.0, r.y + 8.0, 26.0, 25.0),
        false,
    ) {
        view.browsing = false;
        return;
    }
    let Some(tag) = tag else {
        draw_ui_text("Выберите страну", r.x + 14.0, r.y + 76.0, 13.0, ui::MUTED);
        return;
    };
    let Some(country) = research.country(tag) else {
        draw_ui_text(
            "Исследования этой страны не загружены",
            r.x + 14.0,
            r.y + 76.0,
            12.0,
            ui::MUTED,
        );
        return;
    };
    let compact = screen_width() < 760.0;
    let mut command: Option<(usize, String)> = None;
    let mut selected = view.selected.clone();
    let mut available_slot = country.slots.iter().position(|s| s.active.is_none());
    if let Some(preferred) = view.preferred_slot {
        if country
            .slots
            .get(preferred)
            .is_some_and(|s| s.active.is_none())
        {
            available_slot = Some(preferred);
        } else {
            view.preferred_slot = None;
        }
    }
    let tabs_y = r.y + 45.0;
    let tabs = Rect::new(r.x + 12.0, tabs_y, r.w - 24.0, 39.0);
    let folders: Vec<String> = catalogue.folders.iter().cloned().collect();
    if folders.is_empty() {
        draw_ui_text(
            "Технологии не загружены",
            r.x + 13.0,
            tabs_y + 25.0,
            12.0,
            ui::MUTED,
        );
        return;
    }
    view.folder_index = view.folder_index.min(folders.len() - 1);
    let shown = ((tabs.w - 50.0) / 76.0).floor().max(1.0) as usize;
    let first = view.folder_index / shown * shown;
    let tw = ((tabs.w - 50.0) / shown as f32).min(130.0);
    if ui::small_button("◀", Rect::new(tabs.x, tabs.y, 22.0, tabs.h), false) {
        view.folder_index = view.folder_index.saturating_sub(shown);
        view.tree_pan = Vec2::ZERO;
        view.scroll_rows = 0;
        view.scroll_columns = 0;
    }
    if ui::small_button(
        "▶",
        Rect::new(tabs.right() - 22.0, tabs.y, 22.0, tabs.h),
        false,
    ) {
        view.folder_index = (view.folder_index + shown).min(folders.len() - 1);
        view.tree_pan = Vec2::ZERO;
        view.scroll_rows = 0;
        view.scroll_columns = 0;
    }
    for (i, name) in folders.iter().enumerate().skip(first).take(shown) {
        let b = Rect::new(
            tabs.x + 25.0 + (i - first) as f32 * tw,
            tabs.y,
            tw - 3.0,
            tabs.h,
        );
        let (label, icon) = folder_label(name);
        let selected = view.folder_index == i;
        ui::row_bg(b, selected, false);
        crate::ui_icons::draw(
            icon,
            Rect::new(b.x + 6.0, b.y + 7.0, 23.0, 23.0),
            if selected { ui::GOLD } else { ui::MUTED },
        );
        ui::label(
            label,
            Rect::new(b.x + 35.0, b.y + 12.0, b.w - 40.0, 20.0),
            10.0,
            ui::TEXT,
        );
        ui::tooltip(b, label, name);
        if ui::inside(b) && is_mouse_button_pressed(MouseButton::Left) {
            view.folder_index = i;
            view.scroll_rows = 0;
            view.scroll_columns = 0;
            view.tree_pan = Vec2::ZERO;
        }
    }
    let folder = &folders[view.folder_index];
    let tree_top = tabs.y + 46.0;
    let details_h = if compact { 102.0 } else { 115.0 };
    let tree_h = (r.y + r.h - details_h - tree_top - 8.0).max(25.0);
    let tree_rect = Rect::new(r.x + 12.0, tree_top, r.w - 24.0, tree_h);
    draw_rectangle(tree_rect.x, tree_rect.y, tree_rect.w, tree_rect.h, ui::BASE);
    draw_rectangle_lines(
        tree_rect.x,
        tree_rect.y,
        tree_rect.w,
        tree_rect.h,
        1.0,
        ui::EDGE,
    );
    let node_w: f32 = if compact { 117.0 } else { 152.0 };
    let node_h: f32 = if compact { 48.0 } else { 53.0 };
    let row_step = node_h + 17.0;
    let nodes: Vec<_> = catalogue
        .technologies
        .values()
        .filter(|tech| tech.folder == *folder && catalogue.visible(&tech.id, country))
        .collect();
    let max_row = nodes
        .iter()
        .map(|tech| tech.y.max(0.0) as usize)
        .max()
        .unwrap_or(0);
    let max_col = nodes
        .iter()
        .map(|tech| tech.x.max(0.0) as usize)
        .max()
        .unwrap_or(0);
    let rows_visible = (tree_h / row_step).ceil().max(1.0) as usize;
    let col_step = node_w + 12.0;
    let cols_visible = (tree_rect.w / col_step).floor().max(1.0) as usize;
    view.scroll_rows = view
        .scroll_rows
        .min(max_row.saturating_sub(rows_visible.saturating_sub(1)));
    view.scroll_columns = view
        .scroll_columns
        .min(max_col.saturating_sub(cols_visible.saturating_sub(1)));
    let pan = view.tree_pan;
    let coords = |x: f32, y: f32| -> Vec2 {
        vec2(
            tree_rect.x + 12.0 + pan.x + (x.max(0.0) - view.scroll_columns as f32) * col_step,
            tree_rect.y + 9.0 + pan.y + (y.max(0.0) - view.scroll_rows as f32) * row_step,
        )
    };
    // Parent links are rendered before nodes. Arrow color follows completion.
    for tech in &nodes {
        let dest = coords(tech.x, tech.y);
        for parent in &tech.prerequisites {
            if let Some(p) = catalogue
                .technologies
                .get(parent)
                .filter(|p| p.folder == *folder)
            {
                let start = coords(p.x, p.y);
                if start.y + node_h >= tree_rect.y
                    && dest.y <= tree_rect.y + tree_h
                    && start.x >= tree_rect.x
                    && start.x + node_w <= tree_rect.x + tree_rect.w
                    && dest.x >= tree_rect.x
                    && dest.x + node_w <= tree_rect.x + tree_rect.w
                {
                    draw_line(
                        start.x + node_w * 0.5,
                        start.y + node_h,
                        dest.x + node_w * 0.5,
                        dest.y,
                        2.0,
                        if country.researched.contains(parent) {
                            ui::GREEN
                        } else {
                            ui::EDGE
                        },
                    );
                }
            }
        }
    }
    let mut node_hit = false;
    for tech in &nodes {
        let pos = coords(tech.x, tech.y);
        if pos.y < tree_rect.y
            || pos.y + node_h > tree_rect.y + tree_rect.h
            || pos.x < tree_rect.x
            || pos.x + node_w > tree_rect.x + tree_rect.w
        {
            continue;
        }
        let area = Rect::new(pos.x, pos.y, node_w.min(tree_rect.w - 20.0), node_h);
        let done = country.researched.contains(&tech.id);
        let active = country
            .slots
            .iter()
            .any(|slot| slot.active.as_ref().is_some_and(|a| a.tech_id == tech.id));
        ui::row_bg(area, selected.as_deref() == Some(tech.id.as_str()), false);
        draw_rectangle_lines(
            area.x,
            area.y,
            area.w,
            area.h,
            2.0,
            label_color(&tech.id, country, catalogue),
        );
        draw_ui_text(
            &fit(&tech.name, area.w - 8.0, if compact { 10 } else { 11 }),
            area.x + 5.0,
            area.y + 17.0,
            if compact { 10.0 } else { 11.0 },
            ui::TEXT,
        );
        draw_ui_text(
            &format!(
                "{}  {}  {}",
                tech.start_year,
                if done {
                    "ГОТОВО"
                } else if active {
                    "ИЗУЧАЕТСЯ"
                } else {
                    "ТЕХНОЛОГИИ"
                },
                tech.research_cost
            ),
            area.x + 5.0,
            area.y + 33.0,
            9.0,
            label_color(&tech.id, country, catalogue),
        );
        if ui::inside(area) {
            node_hit = true;
        }
        if ui::inside(area) && is_mouse_button_pressed(MouseButton::Left) {
            selected = Some(tech.id.clone());
        }
    }
    let mouse = vec2(mouse_position().0, mouse_position().1);
    let inside_tree = ui::inside(tree_rect);
    if inside_tree && !node_hit && is_mouse_button_pressed(MouseButton::Left) {
        view.drag_last = Some(mouse);
    }
    if let Some(previous) = view.drag_last {
        if is_mouse_button_down(MouseButton::Left) {
            view.tree_pan += mouse - previous;
            view.tree_pan.x = view.tree_pan.x.clamp(-(max_col as f32 * col_step), 0.0);
            view.tree_pan.y = view.tree_pan.y.clamp(-(max_row as f32 * row_step), 0.0);
            view.drag_last = Some(mouse);
        } else {
            view.drag_last = None;
        }
    }

    if inside_tree {
        let (_, dy) = mouse_wheel();
        if dy > 0.0 {
            view.scroll_rows = view.scroll_rows.saturating_sub(1);
        } else if dy < 0.0 {
            view.scroll_rows = (view.scroll_rows + 1).min(max_row);
        }
    }
    if ui::small_button(
        "<",
        Rect::new(tree_rect.x + 3.0, tree_rect.y + 3.0, 20.0, 22.0),
        false,
    ) {
        view.scroll_columns = view.scroll_columns.saturating_sub(1);
    }
    if ui::small_button(
        ">",
        Rect::new(tree_rect.x + 27.0, tree_rect.y + 3.0, 20.0, 22.0),
        false,
    ) {
        view.scroll_columns = (view.scroll_columns + 1).min(max_col);
    }
    if ui::small_button(
        "^",
        Rect::new(
            tree_rect.x + tree_rect.w - 24.0,
            tree_rect.y + 3.0,
            20.0,
            22.0,
        ),
        false,
    ) {
        view.scroll_rows = view.scroll_rows.saturating_sub(1);
    }
    if ui::small_button(
        "v",
        Rect::new(
            tree_rect.x + tree_rect.w - 24.0,
            tree_rect.y + 28.0,
            20.0,
            22.0,
        ),
        false,
    ) {
        view.scroll_rows = (view.scroll_rows + 1).min(max_row);
    }

    view.selected = selected;
    let details_y = tree_rect.y + tree_rect.h + 8.0;
    if let Some(selected) = view.selected.as_ref() {
        if let Some(tech) = catalogue.technologies.get(selected) {
            let prereq = if tech.prerequisites.is_empty() {
                "нет".into()
            } else {
                tech.prerequisites
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let preview_eta = available_slot.and_then(|slot| {
                let mut simulation = country.clone();
                simulation
                    .start(slot, &tech.id, catalogue, rules, date)
                    .ok()?;
                simulation.estimated_days(slot, catalogue, rules, date)
            });
            let preview = preview_eta.map_or("НЕДОСТУПНО".to_owned(), |days| {
                let date_label = date
                    .after_days(days)
                    .map_or("?".to_owned(), GameDate::display);
                format!("{} дн. / {}", days, date_label)
            });
            draw_ui_text(
                &fit(
                    &format!(
                        "{} · Год {} · Базовая стоимость {:.0} дн. · Срок {}",
                        tech.name,
                        tech.start_year,
                        tech.research_cost * rules.base_tech_cost,
                        preview
                    ),
                    r.w - 32.0,
                    if compact { 10 } else { 13 },
                ),
                r.x + 14.0,
                details_y + 15.0,
                if compact { 10.0 } else { 13.0 },
                ui::GOLD,
            );
            draw_ui_text(
                &fit(
                    &format!(
                        "Требуется: {prereq} · {}",
                        catalogue
                            .can_research(&tech.id, country)
                            .err()
                            .unwrap_or("ДОСТУПНО")
                    ),
                    r.w - 32.0,
                    10,
                ),
                r.x + 14.0,
                details_y + 32.0,
                10.0,
                ui::MUTED,
            );
            let x = r.x + 14.0;
            let max_button = (r.w - 32.0).min(580.0);
            if let Some(slot) = available_slot {
                if ui::small_button(
                    &format!("ИССЛЕДОВАТЬ · ЯЧЕЙКА {}", slot + 1),
                    Rect::new(x, details_y + 42.0, max_button * 0.54, 31.0),
                    false,
                ) {
                    command = Some((slot, tech.id.clone()));
                }
            }
            let summary = if !tech.description.is_empty() {
                tech.description.as_str().to_owned()
            } else {
                format!(
                    "Открывает: {}",
                    tech.unlocks.iter().cloned().collect::<Vec<_>>().join(", ")
                )
            };
            draw_ui_text(
                &fit(&summary, r.w - 33.0, 10),
                x,
                details_y + 88.0,
                10.0,
                ui::MUTED,
            );
        }
    } else {
        draw_ui_text(
            "Выберите технологию в дереве",
            r.x + 14.0,
            details_y + 20.0,
            12.0,
            ui::MUTED,
        );
    }
    if !view.notice.is_empty() {
        draw_ui_text(
            &fit(&view.notice, r.w - 36.0, 11),
            r.x + 14.0,
            (r.y + r.h - 10.0).max(details_y + 75.0),
            11.0,
            ui::GOLD,
        );
    }

    if let Some((slot, id)) = command {
        let Some(c) = research.countries.get_mut(tag) else {
            return;
        };
        if id.is_empty() {
            if c.cancel(slot) {
                view.notice = format!("Исследование в ячейке {} остановлено", slot + 1);
            }
        } else {
            match c.start(slot, &id, catalogue, rules, date) {
                Ok(()) => {
                    view.notice = format!("Начато: {id}");
                    view.browsing = false;
                }
                Err(error) => view.notice = error.into(),
            }
        }
    }
}
