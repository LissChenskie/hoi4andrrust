//! The independent construction-screen view. Mimics the UX hierarchy of
//! HOI4 (queue on left, building palette on right) without copying game art.
use macroquad::prelude::*;
use crate::{draw_ui_text, country_display_name, construction::{
    self, BuildingRules, BuildingType, ConstructionModifiers, ConstructionQueues,
    CountryConstructionPolicy, EconomicLawPreset,
}, world::World};
use crate::hoi_ui as ui;

pub struct ConstructionView {
    pub open: bool,
    pub selected_building: BuildingType,
    pub focused_state: Option<u32>,
    pub first_visible_row: usize,
}

impl Default for ConstructionView {
    fn default() -> Self {
        Self {
            open: false,
            selected_building: BuildingType::CivilianFactory,
            focused_state: None,
            first_visible_row: 0,
        }
    }
}

pub fn bounds() -> Rect {
    let compact = screen_width() < 760.0;
    let x = if compact { 7.0 } else { 12.0 };
    let y = if compact { 107.0 } else { 110.0 };
    let width = if compact {
        (screen_width() - 14.0).max(1.0)
    } else {
        (screen_width() * 0.58).clamp(470.0, 700.0).min(screen_width() - 24.0)
    };
    let bottom = if compact { 72.0 } else { 14.0 };
    Rect::new(x, y, width, (screen_height() - y - bottom).max(200.0))
}

pub fn is_over_panel(point: Vec2) -> bool {
    bounds().contains(point)
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, w.max(1.0), h.max(1.0))
}

fn progress_description(
    world: &World, state_id: u32, project: &construction::ConstructionProject,
    assigned: u32, policy: CountryConstructionPolicy, rules: &BuildingRules,
) -> String {
    let infra = world.states.get(&state_id).map_or(0, |state| state.economy.infrastructure);
    let daily = construction::daily_construction_points(
        rules, policy, &ConstructionModifiers::default(),
        state_id, project.building, infra, assigned,
    );
    let eta = construction::estimated_days_remaining(project, daily)
        .map_or("Paused".to_owned(), |days| format!("{days} days"));
    format!("{}  |  {assigned}/{} CIV  |  {eta}",
        project.building.short_label(), rules.max_civilian_factories_per_line)
}

fn build_in_state(
    world: &World, country: &str, state_id: u32,
    queues: &mut ConstructionQueues, priority: &mut Vec<u32>,
    rules: &BuildingRules, view: &ConstructionView, notice: &mut String,
) {
    match construction::enqueue_project(world, country, state_id, queues, rules, view.selected_building) {
        Ok(()) => {
            if !priority.contains(&state_id) { priority.push(state_id); }
            *notice = format!("{}: {} added to queue", state_id, view.selected_building.label());
        }
        Err(reason) => *notice = reason.to_owned(),
    }
}

/// Returns the mouse wheel delta only while over the queue, otherwise leaves
/// the map and its zoom gestures alone.
pub fn draw(
    world: Option<&World>,
    tag: Option<&str>,
    queues: &mut ConstructionQueues,
    priority: &mut Vec<u32>,
    rules: &BuildingRules,
    policy: &mut CountryConstructionPolicy,
    law: &mut EconomicLawPreset,
    notice: &mut String,
    view: &mut ConstructionView,
) {
    if !view.open { return; }
    let area = bounds();
    let narrow = screen_width() < 760.0;
    ui::panel(area.x, area.y, area.w, area.h);
    ui::header(area.x + 4.0, area.y + 4.0, area.w - 8.0, "CONSTRUCTION");
    if ui::small_button("X", rect(area.x + area.w - 41.0, area.y + 7.0, 28.0, 25.0), false) {
        view.open = false;
        return;
    }

    let (Some(world), Some(tag)) = (world, tag) else {
        draw_ui_text("Load map and select a country first.", area.x + 16.0, area.y + 84.0, 15.0, ui::MUTED);
        return;
    };
    let capacity = construction::country_capacity(world, tag, *policy);
    let free = capacity.map_or(0, |stats| stats.available);
    let total = capacity.map_or(0, |stats| stats.total_civilian);
    let consumer = capacity.map_or(0, |stats| stats.consumer_goods);
    let trade = capacity.map_or(0, |stats| stats.trade);
    let active_lines: usize = queues.iter()
        .filter(|(id, _)| world.states.get(id).is_some_and(|s| s.owner.as_deref() == Some(tag)))
        .map(|(_, q)| q.len()).sum();

    let stat_y = area.y + 43.0;
    let gap = 5.0;
    let available_w = (area.w - 23.0) / 4.0;
    let labels = [("CIVILIAN", format!("{free}/{total}")), ("GOODS", consumer.to_string()),
        ("TRADE", trade.to_string()), ("QUEUED", active_lines.to_string())];
    for (index, (label, value)) in labels.into_iter().enumerate() {
        ui::stat(area.x + 10.0 + index as f32 * (available_w + gap), stat_y, available_w,
            label, &value);
    }

    // Palette is to the right on desktop, but wraps above the list on phones.
    let list_x = area.x + 10.0;
    let palette_w = if narrow { area.w - 20.0 } else { 168.0 };
    let list_w = if narrow { area.w - 20.0 } else { (area.w - palette_w - 29.0).max(210.0) };
    let palette_x = if narrow { list_x } else { list_x + list_w + 9.0 };
    let palette_y = if narrow { area.y + 107.0 } else { area.y + 105.0 };

    draw_ui_text("SELECT BUILDING", palette_x + 3.0, palette_y - 7.0, 12.0, ui::GOLD);
    let cols = if narrow { 4 } else { 2 };
    let cell_gap = 4.0;
    let cell_w = (palette_w - (cols as f32 - 1.0) * cell_gap) / cols as f32;
    let cell_h = if narrow { 40.0 } else { 51.0 };
    for (index, building) in BuildingType::ALL.into_iter().enumerate() {
        let col = index % cols;
        let row = index / cols;
        let x = palette_x + col as f32 * (cell_w + cell_gap);
        let y = palette_y + row as f32 * (cell_h + 5.0);
        if ui::small_button(building.short_label(), rect(x, y, cell_w, cell_h),
            building == view.selected_building) {
            view.selected_building = building;
        }
    }

    let list_y = if narrow { area.y + 211.0 } else { area.y + 107.0 };
    let list_bottom = area.y + area.h - if narrow { 31.0 } else { 50.0 };
    let list_h = (list_bottom - list_y).max(10.0);
    draw_rectangle(list_x, list_y, list_w, list_h, Color::new(0.10, 0.11, 0.11, 1.0));
    draw_rectangle_lines(list_x, list_y, list_w, list_h, 1.0, ui::EDGE);
    draw_rectangle(list_x + 1.0, list_y + 1.0, list_w - 2.0, 24.0,
        Color::new(0.13, 0.14, 0.14, 1.0));
    draw_ui_text("LOCATION / BUILD QUEUE", list_x + 8.0, list_y + 17.0,
        if narrow { 11.0 } else { 13.0 }, ui::GOLD);

    let mut states: Vec<u32> = world.countries.get(tag)
        .map(|country| country.state_ids.iter().copied().collect()).unwrap_or_default();
    // The player has explicitly reordered these states; unqueued ones are shown
    // after active ones in ascending ID order.
    states.sort_unstable_by_key(|id| (
        priority.iter().position(|candidate| candidate == id).unwrap_or(usize::MAX),
        *id,
    ));

    let content_y = list_y + 26.0;
    let row_height = if narrow { 69.0 } else { 64.0 };
    let visible_count = ((list_h - 29.0) / row_height).floor().max(0.0) as usize;
    let max_first = states.len().saturating_sub(visible_count);
    view.first_visible_row = view.first_visible_row.min(max_first);
    let list_region = rect(list_x, content_y, list_w, list_h - 26.0);
    if ui::inside(list_region) {
        let (_, wheel) = mouse_wheel();
        if wheel > 0.0 { view.first_visible_row = view.first_visible_row.saturating_sub(2); }
        if wheel < 0.0 { view.first_visible_row = (view.first_visible_row + 2).min(max_first); }
    }

    let allocations = construction::allocate_factories(world, tag, queues, priority, rules, *policy);
    for (position, state_id) in states.iter().copied()
        .skip(view.first_visible_row).take(visible_count).enumerate()
    {
        let Some(state) = world.states.get(&state_id) else { continue; };
        let row_y = content_y + position as f32 * row_height + 1.0;
        let r = rect(list_x + 3.0, row_y, list_w - 6.0, row_height - 3.0);
        ui::row_bg(r, view.focused_state == Some(state_id), position % 2 == 1);
        let queued = queues.get(&state_id).cloned().unwrap_or_default();
        let slots = construction::shared_slot_limit(world, state_id, rules).unwrap_or(0);
        let occupied = usize::from(state.economy.occupied_shared_slots());
        let reserved = construction::queued_shared_slots_with_rules(&queued, rules);
        let free_slots = slots.saturating_sub(occupied + reserved);
        let label = state.name.as_deref().unwrap_or("Unnamed state");
        let label_area = rect(r.x + 4.0, r.y + 4.0, (r.w - 119.0).max(85.0), 24.0);
        // State label selection is independent of map hit-testing.
        if ui::inside(label_area) && is_mouse_button_pressed(MouseButton::Left) {
            view.focused_state = Some(state_id);
        }
        let display: String = label.chars().take(if narrow { 17 } else { 23 }).collect();
        draw_ui_text(&display, r.x + 8.0, r.y + 18.0, if narrow { 12.0 } else { 13.0 }, ui::TEXT);
        let status = format!("Slots {free_slots}/{slots}  |  Infra {}", state.economy.infrastructure);
        draw_ui_text(&status, r.x + 8.0, r.y + 33.0, 10.0, ui::MUTED);
        let mut fraction = 0.0;
        let mut description = "No construction orders".to_owned();
        if let Some(project) = queued.front() {
            fraction = project.progress_fraction();
            let assigned = allocations.iter().find(|a| a.state_id == state_id && a.project_index == 0)
                .map_or(0, |a| a.civilian_factories);
            description = progress_description(world, state_id, project, assigned, *policy, rules);
        }
        ui::progress_bar(rect(r.x + 8.0, r.y + row_height - 23.0, (r.w - 137.0).max(55.0), 7.0), fraction);
        draw_ui_text(&description.chars().take(40).collect::<String>(), r.x + 8.0,
            r.y + row_height - 9.0, 10.0, ui::MUTED);

        let controls_x = r.x + r.w - 113.0;
        if ui::small_button("+", rect(controls_x, r.y + 5.0, 27.0, 27.0), false) {
            view.focused_state = Some(state_id);
            build_in_state(world, tag, state_id, queues, priority, rules, view, notice);
        }
        if ui::small_button("^", rect(controls_x + 30.0, r.y + 5.0, 24.0, 27.0), false) {
            if let Some(index) = priority.iter().position(|id| *id == state_id) {
                if index > 0 { priority.swap(index - 1, index); }
            } else { priority.insert(0, state_id); }
        }
        if ui::small_button("v", rect(controls_x + 57.0, r.y + 5.0, 24.0, 27.0), false) {
            if let Some(index) = priority.iter().position(|id| *id == state_id) {
                if index + 1 < priority.len() { priority.swap(index, index + 1); }
            } else { priority.push(state_id); }
        }
        if ui::small_button("X", rect(controls_x + 84.0, r.y + 5.0, 24.0, 27.0), false) {
            if construction::cancel_project(queues, state_id, 0) {
                if !queues.contains_key(&state_id) { priority.retain(|id| *id != state_id); }
                *notice = format!("Cancelled first project in state {state_id}");
            }
        }
        draw_ui_text(&format!("{} queued", queued.len()), controls_x, r.y + row_height - 16.0,
            10.0, ui::MUTED);
    }
    if states.is_empty() {
        draw_ui_text("No owned states. Select a country with valid map history.",
            list_x + 8.0, content_y + 28.0, 12.0, ui::MUTED);
    }
    if visible_count < states.len() {
        if ui::small_button("^", rect(list_x + list_w - 60.0, list_y + 2.0, 23.0, 21.0), false) {
            view.first_visible_row = view.first_visible_row.saturating_sub(1);
        }
        if ui::small_button("v", rect(list_x + list_w - 32.0, list_y + 2.0, 23.0, 21.0), false) {
            view.first_visible_row = (view.first_visible_row + 1).min(max_first);
        }
    }

    if !narrow {
        let footer_y = area.y + area.h - 47.0;
        let chosen_name = view.focused_state.and_then(|id| world.states.get(&id))
            .and_then(|state| state.name.as_deref()).unwrap_or("Choose state from list");
        draw_ui_text(&format!("Selected: {}", chosen_name.chars().take(30).collect::<String>()),
            list_x + 5.0, footer_y + 15.0, 12.0, ui::GOLD);
        if ui::small_button("QUEUE BUILDING", rect(palette_x, footer_y, palette_w, 28.0), false) {
            if let Some(state_id) = view.focused_state {
                build_in_state(world, tag, state_id, queues, priority, rules, view, notice);
            } else { *notice = "Select a state in the list first".to_owned(); }
        }
        if ui::small_button(law.label(), rect(palette_x, palette_y + 244.0, palette_w, 26.0), false) {
            *law = law.next();
            policy.consumer_goods_fraction = law.consumer_goods_fraction();
        }
    } else if ui::small_button(law.label(),
        rect(list_x + list_w - 153.0, area.y + 88.0, 150.0, 20.0), false) {
        *law = law.next();
        policy.consumer_goods_fraction = law.consumer_goods_fraction();
    }
    let tip = if notice.is_empty() {
        format!("{}  |  choose building, then press + for a state", country_display_name(tag))
    } else {
        notice.clone()
    };
    draw_ui_text(&tip.chars().take(if narrow { 39 } else { 85 }).collect::<String>(),
        area.x + 13.0, area.y + area.h - 11.0, 11.0, ui::GOLD);
}
