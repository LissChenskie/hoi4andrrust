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
    pub tree_zoom: f32,
    pub pinch_last: Option<(f32, Vec2)>,
    pub estimate_key: String,
    pub estimates: Vec<Option<u32>>,
    pub available: std::collections::BTreeSet<String>,
    pub preview_key: String,
    pub preview_days: Option<u32>,
    pub preview_active: Option<crate::research::ActiveResearch>,
    pub last_completion_count: usize,
    pub completion_notice: String,
    pub completion_until: f64,

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
    } else if folder == "unplaced" {
        ("Без координат", Icon::Research)
    } else {
        (folder, Icon::Research)
    }
}
fn label_color(
    tech_id: &str,
    country: &CountryResearch,
    available: &std::collections::BTreeSet<String>,
) -> Color {
    if country.researched.contains(tech_id) {
        ui::GREEN
    } else if country
        .slots
        .iter()
        .any(|s| s.active.as_ref().is_some_and(|a| a.tech_id == tech_id))
    {
        ui::GOLD
    } else if available.contains(tech_id) {
        ui::TEXT
    } else {
        ui::MUTED
    }
}

fn refresh_estimates(
    view: &mut ResearchView,
    country: &CountryResearch,
    cat: &ResearchCatalogue,
    rules: &ResearchRules,
    date: GameDate,
    tag: &str,
) {
    let key = format!("{tag}:{}:{}", date.display(), country.revision);
    if view.estimate_key != key {
        view.available = cat
            .technologies
            .keys()
            .filter(|id| cat.can_research(id, country).is_ok())
            .cloned()
            .collect();
        view.estimates = (0..country.slots.len())
            .map(|i| country.estimated_days(i, cat, rules, date))
            .collect();
        view.estimate_key = key;
    }
    if country.completed.len() < view.last_completion_count {
        view.last_completion_count = 0;
    }
    if country.completed.len() > view.last_completion_count {
        if let Some(id) = country.completed.last() {
            view.completion_notice = format!(
                "Завершено: {}",
                cat.technologies
                    .get(id)
                    .map_or(id.as_str(), |t| t.name.as_str())
            );
            view.completion_until = get_time() + 8.0;
        }
        view.last_completion_count = country.completed.len();
    }
}
fn effect_summary(t: &crate::research::Technology) -> String {
    use crate::research::TechnologyEffect;
    let mut parts = Vec::new();
    for e in &t.effects {
        parts.push(match e {
            TechnologyEffect::Building { id, level } => {
                format!("Здание {id}: максимальный уровень {level}")
            }
            TechnologyEffect::Unlock { kind, id, limit } => format!(
                "{kind}: {id}{}{}",
                if limit.is_empty() {
                    ""
                } else {
                    " (при выполнении limit)"
                },
                if kind == "enable_equipment_modules" {
                    " · конструктор пока не реализован"
                } else {
                    ""
                }
            ),
            TechnologyEffect::CountryModifier { key, value } => format!(
                "{key}: {value:+.3} · {}",
                crate::research::modifier_consumer(key)
                    .unwrap_or("хранится; потребитель пока отсутствует")
            ),
            TechnologyEffect::ScopedModifier { scope, values } => format!(
                "{scope}: {}",
                values
                    .iter()
                    .map(|(k, v)| format!("{k} {v:+.3}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            TechnologyEffect::Deferred { field, .. } => {
                format!("{field}: сохранён, выполнение пока не поддерживается")
            }
        });
    }
    if !t.on_complete.is_empty() {
        parts.push(format!(
            "Скрипт завершения: {}",
            t.on_complete
                .iter()
                .map(|e| e.key.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if parts.is_empty() {
        "Определение не содержит распознанных эффектов; исходные поля доступны в отчёте".into()
    } else {
        parts.join("\n")
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
    let Some(country) = research.country(tag) else {
        return;
    };
    refresh_estimates(view, country, catalogue, rules, date, tag);
    let active = country.slots.iter().filter(|s| s.active.is_some()).count();
    ui::label(
        &format!("Занято ячеек: {active} / {}", if country.research_slots_known { country.slots.len().to_string() } else { "?".into() }),
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
            let total = tech.map_or(1.0, |t| {
                t.research_cost * rules.base_tech_cost * a.cost_coefficient
            });
            ui::label(name, Rect::new(tx, row.y + 9.0, tw, 21.0), 14.0, ui::TEXT);
            let eta = view
                .estimates
                .get(i)
                .copied()
                .flatten()
                .map_or("—".into(), |d| {
                    format!(
                        "{d} дн. · {}",
                        date.after_days(d).map_or("—".into(), GameDate::display)
                    )
                });
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
    if get_time() < view.completion_until {
        ui::label(
            &view.completion_notice,
            Rect::new(r.x + 12.0, r.y + 68.0, r.w - 24.0, 14.0),
            10.0,
            ui::GREEN,
        );
    }
    if let Some(i) = cancel {
        if let Some(country) = research.countries.get_mut(tag) {
            country.cancel(i);
        }
    }
    ui::label(
        &view.notice,
        Rect::new(r.x + 12.0, r.bottom() - 70.0, r.w - 24.0, 18.0),
        10.0,
        ui::MUTED,
    );
    ui::tooltip(
        Rect::new(r.x + 12.0, r.bottom() - 70.0, r.w - 24.0, 18.0),
        "Источник исследований",
        &view.notice,
    );
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
    refresh_estimates(view, country, catalogue, rules, date, tag);
    if view.tree_zoom == 0.0 {
        view.tree_zoom = 1.0;
    }
    let compact = screen_width() < 760.0;
    let mut command: Option<(usize, String)> = None;
    let mut selected = view.selected.clone();
    let mut available_slot = country.slots.iter().position(|s| s.active.is_none());
    if let Some(preferred) = view.preferred_slot {
        if country.slots.get(preferred).is_some() {
            available_slot = Some(preferred);
        } else {
            view.preferred_slot = None;
        }
    }
    let tabs_y = r.y + 45.0;
    let tabs = Rect::new(r.x + 12.0, tabs_y, r.w - 24.0, 39.0);
    let mut folders: Vec<String> = catalogue
        .folder_order
        .iter()
        .filter(|f| catalogue.folders.contains(*f))
        .cloned()
        .collect();
    for f in &catalogue.folders {
        if !folders.contains(f) {
            folders.push(f.clone());
        }
    }
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
        let (fallback, icon) = folder_label(name);
        let label = catalogue
            .folder_names
            .get(name)
            .map_or(fallback, String::as_str);
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
    let details_h = if compact { 191.0 } else { 157.0 };
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
    let zoom = view.tree_zoom.clamp(0.5, 1.8);
    let node_w: f32 = if compact { 117.0 } else { 152.0 } * zoom;
    let node_h: f32 = if compact { 48.0 } else { 53.0 } * zoom;
    let row_step = node_h + 17.0 * zoom;
    let nodes: Vec<_> = catalogue
        .folder_nodes
        .get(folder)
        .into_iter()
        .flatten()
        .filter_map(|id| catalogue.technologies.get(id))
        .filter(|t| catalogue.displayed(&t.id, country))
        .collect();
    let position = |t: &crate::research::Technology| {
        if folder == "unplaced" && t.positions.is_empty() {
            return (
                0.0,
                nodes.iter().position(|node| node.id == t.id).unwrap_or(0) as f32,
            );
        }
        t.positions
            .iter()
            .find(|p| p.folder == *folder)
            .map_or((t.x, t.y), |p| (p.x, p.y))
    };
    let min_x = nodes.iter().map(|t| position(t).0).fold(0.0_f32, f32::min);
    let min_y = nodes.iter().map(|t| position(t).1).fold(0.0_f32, f32::min);
    let max_row = nodes
        .iter()
        .map(|t| (position(t).1 - min_y).ceil().max(0.0) as usize)
        .max()
        .unwrap_or(0);
    let max_col = nodes
        .iter()
        .map(|t| (position(t).0 - min_x).ceil().max(0.0) as usize)
        .max()
        .unwrap_or(0);
    let rows_visible = (tree_h / row_step).ceil().max(1.0) as usize;
    let col_step = node_w + 12.0 * zoom;
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
            tree_rect.x + 12.0 + pan.x + (x - min_x - view.scroll_columns as f32) * col_step,
            tree_rect.y + 34.0 + pan.y + (y - min_y - view.scroll_rows as f32) * row_step,
        )
    };
    // Parent links are rendered before nodes. Arrow color follows completion.
    for tech in &nodes {
        let dest = coords(position(tech).0, position(tech).1);
        for parent in tech.prerequisites.iter().chain(&tech.incoming_paths) {
            if let Some(p) = catalogue.technologies.get(parent).filter(|p| {
                p.positions.iter().any(|pos| pos.folder == *folder) || p.folder == *folder
            }) {
                let start = coords(position(p).0, position(p).1);
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
        let pos = coords(position(tech).0, position(tech).1);
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
            label_color(&tech.id, country, &view.available),
        );
        draw_ui_text(
            &fit(
                &tech.name,
                area.w - 10.0,
                (((if compact { 10.0 } else { 11.0 }) * zoom).max(7.0)).ceil() as u16,
            ),
            area.x + 5.0,
            area.y + 17.0 * zoom,
            ((if compact { 10.0 } else { 11.0 }) * zoom).max(7.0),
            ui::TEXT,
        );
        draw_ui_text(
            &fit(
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
                    format!("{:.0}д.", tech.research_cost * rules.base_tech_cost)
                ),
                area.w - 10.0,
                (9.0 * zoom).max(7.0).ceil() as u16,
            ),
            area.x + 5.0,
            area.y + 33.0 * zoom,
            (9.0 * zoom).max(7.0),
            label_color(&tech.id, country, &view.available),
        );
        ui::tooltip(
            area,
            &tech.name,
            &format!(
                "{}\nID: {} · {}\n{}\nИконка: визуальная заглушка",
                tech.description,
                tech.id,
                tech.source,
                effect_summary(tech)
            ),
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
    if inside_tree
        && ((!node_hit && is_mouse_button_pressed(MouseButton::Left))
            || is_mouse_button_pressed(MouseButton::Middle))
    {
        view.drag_last = Some(mouse);
    }
    if let Some(previous) = view.drag_last {
        if is_mouse_button_down(MouseButton::Left) || is_mouse_button_down(MouseButton::Middle) {
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
        if is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl) {
            if dy != 0.0 {
                view.tree_zoom = (zoom * 1.12_f32.powf(dy)).clamp(0.5, 1.8);
            }
        } else if dy > 0.0 {
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

    let zrect = Rect::new(
        tree_rect.right() - 110.0,
        tree_rect.bottom() - 26.0,
        106.0,
        23.0,
    );
    if ui::small_button("−", Rect::new(zrect.x, zrect.y, 25.0, 23.0), false) {
        view.tree_zoom = (zoom / 1.15).max(0.5);
    }
    if ui::small_button(
        "+",
        Rect::new(zrect.right() - 25.0, zrect.y, 25.0, 23.0),
        false,
    ) {
        view.tree_zoom = (zoom * 1.15).min(1.8);
    }
    ui::label(
        &format!("{:.0}%", zoom * 100.0),
        Rect::new(zrect.x + 28.0, zrect.y + 3.0, 49.0, 18.0),
        10.0,
        ui::TEXT,
    );
    let touch = touches();
    if touch.len() == 2 && touch.iter().all(|t| tree_rect.contains(t.position)) {
        let distance = touch[0].position.distance(touch[1].position);
        let center = (touch[0].position + touch[1].position) * 0.5;
        if let Some((last, old)) = view.pinch_last {
            if last > 4.0 {
                view.tree_zoom = (zoom * distance / last).clamp(0.5, 1.8);
                view.tree_pan += center - old;
            }
        }
        view.pinch_last = Some((distance, center));
        view.drag_last = None;
    } else {
        view.pinch_last = None;
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
            let key = format!(
                "{tag}:{}:{}:{available_slot:?}:{}",
                country.revision,
                date.display(),
                tech.id
            );
            if view.preview_key != key {
                view.preview_active = None;
                view.preview_days = available_slot.and_then(|slot| {
                    let mut simulation = country.clone();
                    simulation
                        .switch(slot, &tech.id, catalogue, rules, date)
                        .ok()?;
                    view.preview_active = simulation.slots[slot].active.clone();
                    simulation.estimated_days(slot, catalogue, rules, date)
                });
                view.preview_key = key;
            }
            let running = country
                .slots
                .iter()
                .position(|s| s.active.as_ref().is_some_and(|a| a.tech_id == tech.id));
            let preview_eta = running
                .and_then(|i| view.estimates.get(i).copied().flatten())
                .or(view.preview_days);
            let preview = preview_eta.map_or(
                if country.researched.contains(&tech.id) {
                    "ИЗУЧЕНО"
                } else {
                    "НЕДОСТУПНО"
                }
                .to_owned(),
                |days| {
                    let date_label = date
                        .after_days(days)
                        .map_or("?".to_owned(), GameDate::display);
                    format!("{} дн. / {}", days, date_label)
                },
            );
            if compact {
                ui::label(
                    &tech.name,
                    Rect::new(r.x + 14.0, details_y + 2.0, r.w - 32.0, 20.0),
                    12.0,
                    ui::GOLD,
                );
                ui::label(
                    &format!(
                        "Год {} · База {:.0} дн.",
                        tech.start_year,
                        tech.research_cost * rules.base_tech_cost
                    ),
                    Rect::new(r.x + 14.0, details_y + 24.0, r.w - 32.0, 18.0),
                    10.0,
                    ui::TEXT,
                );
                ui::label(
                    &format!("Срок: {preview}"),
                    Rect::new(r.x + 14.0, details_y + 42.0, r.w - 32.0, 18.0),
                    10.0,
                    ui::TEXT,
                );
            } else {
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
            }
            draw_ui_text(
                &fit(
                    &format!(
                        "Требуется: {prereq} · {}",
                        catalogue
                            .blocking_reason(&tech.id, country)
                            .unwrap_or("ДОСТУПНО".into())
                    ),
                    r.w - 32.0,
                    10,
                ),
                r.x + 14.0,
                details_y + if compact { 77.0 } else { 32.0 },
                10.0,
                ui::MUTED,
            );
            let x = r.x + 14.0;
            let max_button = (r.w - 32.0).min(580.0);
            if let Some(slot) = available_slot {
                let ready = catalogue.can_research(&tech.id, country).is_ok();
                if ready
                    && ui::small_button(
                        &format!(
                            "{} · ЯЧЕЙКА {}",
                            if country.slots[slot].active.is_some() {
                                "ЗАМЕНИТЬ"
                            } else {
                                "ИССЛЕДОВАТЬ"
                            },
                            slot + 1
                        ),
                        Rect::new(
                            x,
                            details_y + if compact { 88.0 } else { 42.0 },
                            if compact {
                                max_button
                            } else {
                                max_button * 0.54
                            },
                            31.0,
                        ),
                        false,
                    )
                {
                    command = Some((slot, tech.id.clone()));
                }
                if !ready {
                    ui::disabled(
                        "НЕДОСТУПНО",
                        Rect::new(
                            x,
                            details_y + if compact { 88.0 } else { 42.0 },
                            if compact {
                                max_button
                            } else {
                                max_button * 0.54
                            },
                            31.0,
                        ),
                        &catalogue
                            .blocking_reason(&tech.id, country)
                            .unwrap_or_default(),
                    );
                }
            }
            let ahead = crate::research::years_ahead(tech, date);
            let bonuses = country
                .available_bonuses
                .iter()
                .filter(|b| b.matches(tech))
                .map(|b| {
                    format!(
                        "{}: +{:.0}%, −{:.1} г., {} исп.",
                        b.name,
                        b.speed * 100.0,
                        b.ahead_reduction_years,
                        b.uses
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            let details = format!(
                "{}\n{}\nКатегории: {}\nИсточник: {}\nДоступные бонусы: {}",
                tech.description,
                effect_summary(tech),
                tech.categories
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", "),
                tech.source,
                if bonuses.is_empty() {
                    "нет"
                } else {
                    &bonuses
                }
            );
            ui::tooltip(
                Rect::new(x, details_y, r.w - 30.0, 139.0),
                &tech.name,
                &details,
            );
            ui::label(
                &if ahead > 0.0 {
                    format!("Опережение: {ahead:.2} г. · штраф пересчитывается каждый день")
                } else {
                    let active = running
                        .and_then(|i| country.slots[i].active.as_ref())
                        .or(view.preview_active.as_ref());
                    active.map_or(
                        "Без штрафа за опережение".into(),
                        |a| {
                            format!(
                                "Эффективно: {:.2} дн./сутки",
                                crate::research::daily_speed(tech, country, a, rules, date)
                            )
                        },
                    )
                },
                Rect::new(
                    x,
                    details_y + if compact { 146.0 } else { 109.0 },
                    r.w - 32.0,
                    18.0,
                ),
                10.0,
                if ahead > 0.0 { ui::GOLD } else { ui::MUTED },
            );
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
                details_y + if compact { 134.0 } else { 88.0 },
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
            match c.switch(slot, &id, catalogue, rules, date) {
                Ok(()) => {
                    view.notice = format!("Начато: {id}");
                    view.browsing = false;
                }
                Err(error) => view.notice = error.into(),
            }
        }
    }
}
