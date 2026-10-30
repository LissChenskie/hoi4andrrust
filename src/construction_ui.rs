//! The independent construction-screen view. Mimics the UX hierarchy of
//! HOI4 (queue on left, building palette on right) without copying game art.
use crate::game::GameDate;
use crate::hoi_ui as ui;
use crate::{
    construction::{
        self, BuildingRules, BuildingType, ConstructionModifiers, ConstructionOrder,
        ConstructionQueues, CountryConstructionPolicy, EconomicLawPreset, ProjectRef,
    },
    country_display_name, draw_ui_text,
    world::World,
};
use macroquad::prelude::*;

pub struct ConstructionView {
    pub open: bool,
    pub selected_building: BuildingType,
    pub focused_state: Option<u32>,
    pub first_visible_row: usize,
    /// Clicking a building switches from the full queue to map placement.
    pub map_placement: bool,
    /// Active queue-icon drag (also works with a touch pointer).
    pub dragging_project: Option<ProjectRef>,
}

impl Default for ConstructionView {
    fn default() -> Self {
        Self {
            open: false,
            selected_building: BuildingType::CivilianFactory,
            focused_state: None,
            first_visible_row: 0,
            map_placement: false,
            dragging_project: None,
        }
    }
}

impl ConstructionView {
    pub fn close(&mut self) {
        self.open = false;
        self.map_placement = false;
        self.dragging_project = None;
    }
}

pub fn bounds(_view: &ConstructionView) -> Rect {
    crate::ui_layout::Layout::current().primary()
}

pub fn is_over_panel(point: Vec2, view: &ConstructionView) -> bool {
    view.open && bounds(view).contains(point)
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, w.max(1.0), h.max(1.0))
}

fn building_tile(building: BuildingType, cell: Rect, selected: bool, narrow: bool) -> bool {
    let hovered = ui::inside(cell);
    let bg = if selected {
        Color::new(0.38, 0.31, 0.20, 1.0)
    } else if hovered {
        Color::new(0.27, 0.29, 0.27, 1.0)
    } else {
        Color::new(0.19, 0.21, 0.20, 1.0)
    };
    draw_rectangle(cell.x, cell.y, cell.w, cell.h, bg);
    draw_rectangle_lines(
        cell.x,
        cell.y,
        cell.w,
        cell.h,
        if selected { 2.0 } else { 1.0 },
        if selected { ui::GOLD } else { ui::EDGE },
    );
    let accent = match building {
        BuildingType::CivilianFactory => Color::new(0.86, 0.56, 0.27, 1.0),
        BuildingType::MilitaryFactory => Color::new(0.57, 0.70, 0.43, 1.0),
        BuildingType::Dockyard => Color::new(0.53, 0.70, 0.77, 1.0),
        BuildingType::Infrastructure => Color::new(0.73, 0.65, 0.43, 1.0),
        BuildingType::SyntheticRefinery => Color::new(0.63, 0.52, 0.73, 1.0),
        BuildingType::FuelSilo => Color::new(0.71, 0.63, 0.41, 1.0),
        BuildingType::RocketSite => Color::new(0.70, 0.69, 0.66, 1.0),
        BuildingType::NuclearReactor => Color::new(0.78, 0.70, 0.47, 1.0),
    };
    let icon_size = if narrow { 22.0 } else { 25.0 };
    let ix = cell.x
        + if narrow {
            7.0
        } else {
            (cell.w - icon_size) * 0.5
        };
    let iy = cell.y + if narrow { 7.0 } else { 5.0 };
    let cx = ix + icon_size * 0.5;
    // Distinct original silhouettes that remain readable on 360px screens.
    match building {
        BuildingType::Infrastructure => {
            for i in 0..3 {
                let dx = i as f32 * 5.0;
                draw_line(
                    ix + dx + 4.0,
                    iy + 2.0,
                    ix + dx + 4.0,
                    iy + icon_size - 2.0,
                    2.0,
                    accent,
                );
            }
            draw_line(
                ix + 1.0,
                iy + icon_size * 0.5,
                ix + icon_size,
                iy + icon_size * 0.5,
                2.0,
                accent,
            );
        }
        BuildingType::Dockyard => {
            draw_line(
                ix + 3.0,
                iy + icon_size * 0.7,
                ix + icon_size - 2.0,
                iy + icon_size * 0.7,
                3.0,
                accent,
            );
            draw_line(cx, iy + 1.0, cx, iy + icon_size - 3.0, 3.0, accent);
            draw_line(cx, iy + 3.0, ix + 5.0, iy + 9.0, 2.0, accent);
            draw_line(
                ix + 4.0,
                iy + icon_size - 2.0,
                ix + icon_size - 2.0,
                iy + icon_size - 2.0,
                2.0,
                accent,
            );
        }
        BuildingType::FuelSilo | BuildingType::NuclearReactor => {
            draw_circle_lines(cx, iy + icon_size * 0.48, icon_size * 0.39, 3.0, accent);
            draw_circle(cx, iy + icon_size * 0.48, 3.3, accent);
            draw_line(
                ix + 3.0,
                iy + icon_size - 1.0,
                ix + icon_size - 2.0,
                iy + icon_size - 1.0,
                2.0,
                accent,
            );
        }
        BuildingType::RocketSite => {
            draw_triangle(
                vec2(cx, iy + 1.0),
                vec2(cx - 6.0, iy + icon_size - 4.0),
                vec2(cx + 6.0, iy + icon_size - 4.0),
                accent,
            );
            draw_rectangle(ix + 3.0, iy + icon_size - 6.0, icon_size - 6.0, 4.0, accent);
        }
        _ => {
            draw_rectangle(
                ix + 2.0,
                iy + 10.0,
                icon_size - 3.0,
                icon_size - 11.0,
                accent,
            );
            draw_rectangle(ix + 5.0, iy + 2.0, 4.0, 12.0, accent);
            draw_triangle(
                vec2(ix + 8.0, iy + 11.0),
                vec2(ix + 15.0, iy + 5.0),
                vec2(ix + 19.0, iy + 11.0),
                accent,
            );
            draw_rectangle(ix + 7.0, iy + 17.0, 3.0, 4.0, bg);
            draw_rectangle(ix + 14.0, iy + 17.0, 3.0, 4.0, bg);
        }
    }
    let size = if narrow { 10.0 } else { 11.0 };
    let text_x = if narrow { cell.x + 30.0 } else { cell.x + 7.0 };
    draw_ui_text(
        building.short_label(),
        text_x,
        if narrow {
            cell.y + 25.0
        } else {
            cell.y + cell.h - 6.0
        },
        size,
        if selected { ui::GOLD } else { ui::TEXT },
    );
    hovered && is_mouse_button_pressed(MouseButton::Left)
}

/// Small original factory pictogram for each national construction line.
fn draw_queue_icon(building: BuildingType, x: f32, y: f32, size: f32) {
    let color = match building {
        BuildingType::CivilianFactory => Color::new(0.87, 0.58, 0.31, 1.0),
        BuildingType::MilitaryFactory => Color::new(0.56, 0.73, 0.43, 1.0),
        BuildingType::Dockyard => Color::new(0.47, 0.67, 0.81, 1.0),
        BuildingType::Infrastructure => Color::new(0.79, 0.67, 0.42, 1.0),
        BuildingType::SyntheticRefinery => Color::new(0.73, 0.59, 0.80, 1.0),
        BuildingType::FuelSilo => Color::new(0.75, 0.67, 0.41, 1.0),
        BuildingType::RocketSite => Color::new(0.83, 0.78, 0.70, 1.0),
        BuildingType::NuclearReactor => Color::new(0.78, 0.68, 0.51, 1.0),
    };
    draw_rectangle(x, y, size, size, Color::new(0.11, 0.13, 0.13, 1.0));
    draw_rectangle_lines(x, y, size, size, 1.0, ui::EDGE);
    match building {
        BuildingType::Infrastructure => {
            for n in 0..3 {
                let xx = x + 8.0 + n as f32 * 9.0;
                draw_line(xx, y + 4.0, xx, y + size - 5.0, 2.5, color);
            }
            draw_line(
                x + 4.0,
                y + size * 0.52,
                x + size - 4.0,
                y + size * 0.52,
                3.0,
                color,
            );
        }
        BuildingType::Dockyard => {
            draw_line(
                x + 5.0,
                y + size - 8.0,
                x + size - 5.0,
                y + size - 8.0,
                4.0,
                color,
            );
            draw_line(
                x + size * 0.50,
                y + 5.0,
                x + size * 0.50,
                y + size - 8.0,
                3.0,
                color,
            );
            draw_line(x + size * 0.50, y + 7.0, x + 7.0, y + 12.0, 2.0, color);
        }
        BuildingType::RocketSite => {
            draw_triangle(
                vec2(x + size * 0.5, y + 4.0),
                vec2(x + 11.0, y + size - 6.0),
                vec2(x + size - 11.0, y + size - 6.0),
                color,
            );
        }
        _ => {
            draw_rectangle(x + 6.0, y + size * 0.38, size - 12.0, size * 0.45, color);
            draw_rectangle(x + 9.0, y + 6.0, 5.0, size * 0.45, color);
            draw_rectangle(x + 20.0, y + 11.0, 5.0, size * 0.30, color);
            draw_rectangle(x + 11.0, y + size - 12.0, 4.0, 4.0, ui::BASE);
            draw_rectangle(x + 23.0, y + size - 12.0, 4.0, 4.0, ui::BASE);
        }
    }
}

pub fn draw(
    world: Option<&World>,
    tag: Option<&str>,
    queues: &mut ConstructionQueues,
    priority: &mut Vec<u32>,
    order: &mut ConstructionOrder,
    rules: &BuildingRules,
    policy: &mut CountryConstructionPolicy,
    law: &mut EconomicLawPreset,
    notice: &mut String,
    view: &mut ConstructionView,
    current_date: GameDate,
    modifiers: &ConstructionModifiers,
) {
    if !view.open {
        return;
    }
    let area = bounds(view);
    let narrow = screen_width() < 760.0;
    ui::panel(area.x, area.y, area.w, area.h);
    ui::header(area.x + 4.0, area.y + 4.0, area.w - 8.0, "СТРОИТЕЛЬСТВО");
    if ui::small_button(
        "X",
        rect(area.x + area.w - 41.0, area.y + 7.0, 28.0, 25.0),
        false,
    ) {
        view.close();
        return;
    }

    let (Some(world), Some(tag)) = (world, tag) else {
        draw_ui_text(
            "Выберите страну и загрузите карту.",
            area.x + 16.0,
            area.y + 84.0,
            15.0,
            ui::MUTED,
        );
        return;
    };
    let capacity = construction::country_capacity(world, tag, *policy);
    let free = capacity.map_or(0, |stats| stats.available);
    let total = capacity.map_or(0, |stats| stats.total_civilian);
    let consumer = capacity.map_or(0, |stats| stats.consumer_goods);
    let trade = capacity.map_or(0, |stats| stats.trade);
    let active_lines: usize = queues
        .iter()
        .map(|(&id, q)| {
            q.iter()
                .filter(|p| construction::project_belongs(world, id, p, tag))
                .count()
        })
        .sum();

    let stat_y = area.y + 43.0;
    let gap = 5.0;
    let available_w = (area.w - 35.0) / 4.0;
    let labels = [
        ("ФАБРИКИ", format!("{free}/{total}")),
        ("ТОВАРЫ", consumer.to_string()),
        ("ТОРГОВЛЯ", trade.to_string()),
        ("В ОЧЕРЕДИ", active_lines.to_string()),
    ];
    for (index, (label, value)) in labels.into_iter().enumerate() {
        ui::stat(
            area.x + 10.0 + index as f32 * (available_w + gap),
            stat_y,
            available_w,
            label,
            &value,
        );
    }

    // Palette is to the right on desktop, but wraps above the list on phones.
    let list_x = area.x + 10.0;
    let palette_w = area.w - 20.0;
    let list_w = area.w - 20.0;
    let palette_x = list_x;
    let palette_y = if narrow {
        area.y + 107.0
    } else {
        area.y + 105.0
    };

    draw_ui_text(
        "ВЫБРАТЬ ПОСТРОЙКУ",
        palette_x + 3.0,
        palette_y - 7.0,
        12.0,
        ui::GOLD,
    );
    let cols = 4;
    let cell_gap = 4.0;
    let cell_w = (palette_w - (cols as f32 - 1.0) * cell_gap) / cols as f32;
    let cell_h = 40.0;
    for (index, building) in BuildingType::ALL.into_iter().enumerate() {
        let col = index % cols;
        let row = index / cols;
        let x = palette_x + col as f32 * (cell_w + cell_gap);
        let y = palette_y + row as f32 * (cell_h + 5.0);
        if building_tile(
            building,
            rect(x, y, cell_w, cell_h),
            building == view.selected_building,
            narrow,
        ) {
            view.selected_building = building;
            view.map_placement = true;
        }
    }

    let list_y = area.y + 211.0;
    let detail_height = if view.focused_state.is_some() {
        if narrow { 61.0 } else { 84.0 }
    } else {
        0.0
    };
    let list_bottom = area.y + area.h
        - if narrow { 31.0 } else { 50.0 }
        - detail_height;
    let list_h = (list_bottom - list_y).max(10.0);
    draw_rectangle(
        list_x,
        list_y,
        list_w,
        list_h,
        Color::new(0.10, 0.11, 0.11, 1.0),
    );
    draw_rectangle_lines(list_x, list_y, list_w, list_h, 1.0, ui::EDGE);
    draw_rectangle(
        list_x + 1.0,
        list_y + 1.0,
        list_w - 2.0,
        24.0,
        Color::new(0.13, 0.14, 0.14, 1.0),
    );
    draw_ui_text(
        "ОЧЕРЕДЬ СТРОИТЕЛЬСТВА",
        list_x + 8.0,
        list_y + 17.0,
        if narrow { 11.0 } else { 13.0 },
        ui::GOLD,
    );

    // A single national construction queue, not one entry per state.
    // Each row is a separate project with independently assigned factories.
    order.sync(world, tag, queues);
    let content_y = list_y + 26.0;
    let row_height = if narrow { 83.0 } else { 79.0 };
    let visible_count = ((list_h - 29.0) / row_height).floor().max(0.0) as usize;
    let project_count = order.lines.len();
    let max_first = project_count.saturating_sub(visible_count);
    view.first_visible_row = view.first_visible_row.min(max_first);
    let list_region = rect(list_x, content_y, list_w, list_h - 26.0);
    if ui::inside(list_region) {
        let (_, wheel) = mouse_wheel();
        if wheel > 0.0 {
            view.first_visible_row = view.first_visible_row.saturating_sub(2);
        }
        if wheel < 0.0 {
            view.first_visible_row = (view.first_visible_row + 2).min(max_first);
        }
    }
    let allocations =
        construction::allocate_factories_by_order(world, tag, queues, order, rules, *policy);
    // Snapshot read-only row data. Apply UI commands after drawing so a
    // cancellation cannot invalidate indices halfway through a render pass.
    let entries: Vec<_> = order
        .lines
        .iter()
        .copied()
        .skip(view.first_visible_row)
        .take(visible_count)
        .filter_map(|key| {
            let state = world.states.get(&key.state_id)?;
            let project = queues.get(&key.state_id)?.get(key.project_index)?.clone();
            Some((
                key,
                project,
                state
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("State {}", key.state_id)),
                state.economy.infrastructure,
            ))
        })
        .collect();
    let mut request: Option<(char, ProjectRef)> = None;
    for (position, (key, project, name, infrastructure)) in entries.iter().enumerate() {
        let y = content_y + position as f32 * row_height + 1.0;
        let row = rect(list_x + 3.0, y, list_w - 6.0, row_height - 3.0);
        ui::row_bg(
            row,
            view.focused_state == Some(key.state_id),
            position % 2 == 1,
        );
        let assigned = allocations
            .iter()
            .find(|line| line.state_id == key.state_id && line.project_index == key.project_index)
            .map_or(0, |line| line.civilian_factories);
        let daily = construction::daily_construction_points(
            rules,
            *policy,
            modifiers,
            key.state_id,
            project.building,
            *infrastructure,
            assigned,
        );
        let days_left = construction::estimated_days_remaining(project, daily);
        let completion = days_left
            .and_then(|days| current_date.after_days(days))
            .map_or_else(
                || "Ожидание фабрик".to_owned(),
                |date| format!("{}  ({} дн.)", date.display(), days_left.unwrap_or(0)),
            );
        let icon_x = row.x + 7.0;
        draw_queue_icon(project.building, icon_x, row.y + 9.0, 39.0);
        // Drag from the icon; do not invalidate queue indices during rendering.
        let handle = rect(icon_x, row.y + 7.0, 42.0, 44.0);
        if ui::inside(handle) && is_mouse_button_pressed(MouseButton::Left) {
            view.dragging_project = Some(*key);
        }
        if view.dragging_project == Some(*key) {
            draw_rectangle_lines(handle.x, handle.y, handle.w, handle.h, 2.0, ui::GOLD);
        }
        let text_x = row.x + 53.0;
        let text_space = (row.w - 162.0).max(65.0);
        let label_size: u16 = if narrow { 11 } else { 13 };
        let title = format!("{}  -  {}", project.building.label(), name);
        let mut visible_title = title;
        while !visible_title.is_empty()
            && crate::measure_ui_text(&visible_title, label_size).width > text_space
        {
            visible_title.pop();
        }
        if ui::inside(rect(text_x, row.y + 5.0, text_space, 26.0))
            && is_mouse_button_pressed(MouseButton::Left)
        {
            view.focused_state = Some(key.state_id);
        }
        draw_ui_text(
            &visible_title,
            text_x,
            row.y + 19.0,
            label_size as f32,
            ui::TEXT,
        );
        draw_ui_text(
            &format!(
                "{} / {} ПМ · Фабрики: {}",
                project.progress.round() as u32,
                project.cost.round() as u32,
                assigned
            ),
            text_x,
            row.y + 35.0,
            11.0,
            ui::MUTED,
        );
        ui::progress_bar(
            rect(text_x, row.y + 43.0, text_space, 9.0),
            project.progress_fraction(),
        );
        let eta_label = if assigned == 0 {
            "ОЖИДАНИЕ ФАБРИК".to_owned()
        } else if narrow {
            days_left
                .and_then(|days| current_date.after_days(days))
                .map_or_else(
                    || "СРОК НЕИЗВЕСТЕН".to_owned(),
                    |date| format!("ETA {}", date.display()),
                )
        } else {
            format!("Готово: {}", completion)
        };
        // Prevent a long calendar string from overlapping UP/DN/X on phones.
        let mut eta_visible = eta_label;
        while !eta_visible.is_empty() && crate::measure_ui_text(&eta_visible, 10).width > text_space
        {
            eta_visible.pop();
        }
        draw_ui_text(&eta_visible, text_x, row.y + 66.0, 10.0, ui::GOLD);
        // Moving projects across state boundaries affects the real national CIV
        // scheduler, not just the way rows are displayed.
        let control_x = row.x + row.w - 103.0;
        let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        if ui::small_button("↑", rect(control_x, row.y + 7.0, 31.0, 27.0), false) {
            request = Some((if shift { 't' } else { 'u' }, *key));
        }
        if ui::small_button("↓", rect(control_x + 35.0, row.y + 7.0, 31.0, 27.0), false) {
            request = Some((if shift { 'b' } else { 'd' }, *key));
        }
        if ui::small_button("X", rect(control_x + 70.0, row.y + 7.0, 25.0, 27.0), false) {
            request = Some(('x', *key));
        }
        draw_ui_text(
            &format!(
                "#{}  {:.0}%",
                view.first_visible_row + position + 1,
                project.progress_fraction() * 100.0
            ),
            control_x,
            row.y + 56.0,
            10.0,
            ui::MUTED,
        );
    }

    // Outline the hovered destination row; reorder only once on release.
    if view.dragging_project.is_some() && is_mouse_button_down(MouseButton::Left) {
        let pointer = vec2(mouse_position().0, mouse_position().1);
        if list_region.contains(pointer) && visible_count > 0 {
            let target = ((pointer.y - content_y) / row_height).floor()
                .clamp(0.0, (visible_count - 1) as f32) as usize;
            let marker_y = content_y + target as f32 * row_height;
            draw_rectangle_lines(list_x + 2.0, marker_y, list_w - 4.0, row_height - 2.0, 2.0, ui::GOLD);
        }
    }
    if is_mouse_button_released(MouseButton::Left) {
        if let Some(key) = view.dragging_project.take() {
            let pointer = vec2(mouse_position().0, mouse_position().1);
            if list_region.contains(pointer) && visible_count > 0 {
                let target = ((pointer.y - content_y) / row_height).floor()
                    .clamp(0.0, (visible_count - 1) as f32) as usize;
                if order.move_to(key, view.first_visible_row + target) {
                    *notice = "Приоритет строительства изменён перетаскиванием".to_owned();
                }
            }
        }
    }

    if let Some((command, key)) = request {
        match command {
            'u' if order.move_up(key) => *notice = "Приоритет строительства повышен".to_owned(),
            'd' if order.move_down(key) => *notice = "Приоритет строительства понижен".to_owned(),
            't' if order.move_to(key, 0) => *notice = "Проект перемещён в начало очереди".to_owned(),
            'b' if order.move_to(key, order.lines.len().saturating_sub(1)) => {
                *notice = "Проект перемещён в конец очереди".to_owned();
            }
            'x' if order.cancel(queues, key) => {
                if !queues.contains_key(&key.state_id) {
                    priority.retain(|id| *id != key.state_id);
                }
                *notice = "Строительство отменено".to_owned();
            }
            _ => {}
        }
    }
    if project_count == 0 {
        draw_ui_text(
            "Выберите постройку и нажмите на свой регион на карте.",
            list_x + 9.0,
            content_y + 27.0,
            11.0,
            ui::MUTED,
        );
    }
    if visible_count < project_count {
        if ui::small_button(
            "↑",
            rect(list_x + list_w - 66.0, list_y + 2.0, 27.0, 21.0),
            false,
        ) {
            view.first_visible_row = view.first_visible_row.saturating_sub(1);
        }
        if ui::small_button(
            "↓",
            rect(list_x + list_w - 35.0, list_y + 2.0, 27.0, 21.0),
            false,
        ) {
            view.first_visible_row = (view.first_visible_row + 1).min(max_first);
        }
    }

    if let Some(state_id) = view.focused_state {
        if let Some(state) = world.states.get(&state_id) {
            let info_y = list_bottom + 5.0;
            let info_h = (detail_height - 8.0).max(0.0);
            if info_h > 0.0 {
                draw_rectangle(list_x, info_y, list_w, info_h, ui::BASE);
                draw_rectangle_lines(list_x, info_y, list_w, info_h, 1.0, ui::EDGE);
                let name = state.name.as_deref().unwrap_or("Неизвестный регион");
                let name = name.chars().take(if narrow { 24 } else { 42 }).collect::<String>();
                let total_slots = construction::shared_slot_limit(world, state_id, rules).unwrap_or(0);
                let occupied_slots = usize::from(state.economy.occupied_shared_slots());
                let state_queue = queues.get(&state_id);
                let reserved_slots = state_queue.map_or(0, |q| {
                    q.iter().filter(|p| rules.shares_slots(p.building)
                        && construction::project_belongs(world, state_id, p, tag)).count()
                });
                let state_queue_count = state_queue.map_or(0, |q| {
                    q.iter().filter(|p|
                        construction::project_belongs(world, state_id, p, tag)).count()
                });
                let built = view.selected_building.built_count(state.economy);
                let pending = state_queue.map_or(0, |q| {
                    q.iter().filter(|p| p.building == view.selected_building
                        && construction::project_belongs(world, state_id, p, tag)).count()
                });
                let eligible = world.state_controller(state_id) == Some(tag);
                draw_ui_text(
                    &format!("РЕГИОН: {}", name),
                    list_x + 8.0, info_y + 16.0, 12.0, ui::GOLD,
                );
                draw_ui_text(
                    &format!("Слоты: {}+{}/{}  |  Инфра: {}/5",
                        occupied_slots, reserved_slots, total_slots, state.economy.infrastructure),
                    list_x + 8.0, info_y + 32.0, 11.0, ui::TEXT,
                );
                draw_ui_text(
                    &format!("{}: {}+{}  |  Строек: {}{}",
                        view.selected_building.short_label(), built, pending, state_queue_count,
                        if eligible { "" } else { "  |  ЧУЖОЙ РЕГИОН" }),
                    list_x + 8.0, info_y + if narrow { 46.0 } else { 49.0 }, 11.0, ui::MUTED,
                );
                if !narrow {
                    draw_ui_text(
                        "Значок: перетащить. Shift+стрелка: начало/конец.",
                        list_x + 8.0, info_y + 67.0, 10.0, ui::MUTED,
                    );
                }
            }
        }
    }

    if ui::small_button(
        law.label(),
        rect(list_x + list_w - 153.0, area.y + 83.0, 150.0, 20.0),
        false,
    ) {
        *law = law.next();
        policy.consumer_goods_fraction = law.consumer_goods_fraction();
    }
    let tip = if notice.is_empty() {
        format!(
            "{} · Выберите постройку и нажмите на регион",
            country_display_name(tag)
        )
    } else {
        notice.clone()
    };
    draw_ui_text(
        &tip.chars()
            .take(if narrow { 39 } else { 85 })
            .collect::<String>(),
        area.x + 13.0,
        area.y + area.h - 11.0,
        11.0,
        ui::GOLD,
    );
}
