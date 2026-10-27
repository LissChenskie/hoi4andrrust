//! HOI4-inspired production management window. All values and controls are
//! backed by real daily simulation in production.rs. Artwork is procedural.
use crate::hoi_ui as ui;
use crate::production::{CountryProduction, Equipment, ProductionRules, ProductionState};
use crate::world::World;
use crate::{draw_ui_text, measure_ui_text};
use macroquad::prelude::*;

#[derive(Debug, Default)]
pub struct ProductionView {
    pub open: bool,
    pub selected_line: Option<u64>,
    pub selected_equipment: Option<Equipment>,
    pub scroll: usize,
    pub notice: String,
}

impl ProductionView {
    pub fn close(&mut self) {
        self.open = false;
    }
}

pub fn bounds() -> Rect {
    crate::ui_layout::Layout::current().primary()
}

pub fn is_over_panel(point: Vec2, view: &ProductionView) -> bool {
    view.open && bounds().contains(point)
}

fn compact_label(text: &str, width: f32, size: u16) -> String {
    let mut label = text.to_owned();
    while !label.is_empty() && measure_ui_text(&label, size).width > width {
        label.pop();
    }
    label
}

fn icon(equipment: Equipment, x: f32, y: f32, size: f32, active: bool) {
    let color = match equipment {
        Equipment::InfantryKit => Color::from_rgba(185, 169, 115, 255),
        Equipment::SupportKit => Color::from_rgba(133, 187, 149, 255),
        Equipment::Truck => Color::from_rgba(147, 177, 190, 255),
        Equipment::FieldEquipment => Color::from_rgba(182, 148, 116, 255),
        Equipment::Aircraft => Color::from_rgba(160, 161, 207, 255),
    };
    draw_rectangle(x, y, size, size, ui::BASE);
    draw_rectangle_lines(
        x,
        y,
        size,
        size,
        if active { 2.0 } else { 1.0 },
        if active { ui::GOLD } else { ui::EDGE },
    );
    // Simple original abstract symbols, not imported game art.
    match equipment {
        Equipment::InfantryKit => {
            draw_rectangle(x + 8.0, y + 7.0, size - 16.0, size - 14.0, color);
            draw_rectangle_lines(x + 12.0, y + 12.0, size - 24.0, size - 24.0, 1.5, ui::BASE);
        }
        Equipment::SupportKit => {
            draw_rectangle(x + size * 0.37, y + 6.0, size * 0.26, size - 12.0, color);
            draw_rectangle(x + 6.0, y + size * 0.37, size - 12.0, size * 0.26, color);
        }
        Equipment::Truck => {
            draw_rectangle(x + 5.0, y + 11.0, size - 10.0, 15.0, color);
            draw_circle(x + 12.0, y + size - 7.0, 4.0, color);
            draw_circle(x + size - 12.0, y + size - 7.0, 4.0, color);
        }
        Equipment::FieldEquipment => {
            draw_circle_lines(x + size * 0.5, y + size * 0.5, size * 0.33, 3.0, color);
            draw_circle(x + size * 0.5, y + size * 0.5, 4.0, color);
        }
        Equipment::Aircraft => {
            draw_triangle(
                vec2(x + size * 0.5, y + 5.0),
                vec2(x + 8.0, y + size - 9.0),
                vec2(x + size - 8.0, y + size - 9.0),
                color,
            );
        }
    }
}

enum Action {
    Up(u64),
    Down(u64),
    AddFactories(u64, i32),
    Remove(u64),
    Select(u64),
    Create(Equipment),
    Switch(u64, Equipment),
}

pub fn draw(
    world: Option<&World>,
    tag: Option<&str>,
    state: &mut ProductionState,
    view: &mut ProductionView,
    rules: &ProductionRules,
    military: &crate::warfare::Warfare,
) {
    if !view.open {
        return;
    }
    let area = bounds();
    let compact = screen_width() < 760.0;
    ui::panel(area.x, area.y, area.w, area.h);
    ui::header(area.x + 4.0, area.y + 4.0, area.w - 8.0, "ПРОИЗВОДСТВО");
    if ui::small_button(
        "X",
        Rect::new(area.x + area.w - 38.0, area.y + 7.0, 27.0, 25.0),
        false,
    ) {
        view.close();
        return;
    }

    let (Some(world), Some(tag)) = (world, tag) else {
        draw_ui_text(
            "Загрузите карту и выберите страну.",
            area.x + 12.0,
            area.y + 72.0,
            13.0,
            ui::MUTED,
        );
        return;
    };
    let Some(economy) = world.country_economy(tag) else {
        draw_ui_text(
            "Регионы страны не загружены.",
            area.x + 12.0,
            area.y + 72.0,
            13.0,
            ui::MUTED,
        );
        return;
    };
    let snapshot = state
        .country(tag)
        .cloned()
        .unwrap_or_else(CountryProduction::default);
    if view
        .selected_line
        .is_some_and(|id| !snapshot.lines.iter().any(|line| line.id == id))
    {
        view.selected_line = None;
    }
    let allocations = snapshot.allocations(economy.military_factories, rules);
    let assigned: u32 = allocations.iter().map(|line| line.assigned).sum();
    draw_ui_text(
        &format!(
            "Заводы {} · Занято {} · Свободно {} · Линии {}",
            economy.military_factories,
            assigned,
            economy.military_factories.saturating_sub(assigned),
            snapshot.lines.len()
        ),
        area.x + 12.0,
        area.y + 61.0,
        if compact { 11.0 } else { 14.0 },
        ui::GOLD,
    );
    draw_ui_text(
        "Приоритет сверху вниз: заводы сначала выделяются верхним линиям",
        area.x + 12.0,
        area.y + 81.0,
        if compact { 10.0 } else { 12.0 },
        ui::MUTED,
    );

    let list_y = area.y + 94.0;
    let footer_y = area.y + area.h - 136.0;
    let list_h = (footer_y - list_y).max(36.0);
    let list_x = area.x + 9.0;
    let list_w = area.w - 18.0;
    draw_rectangle(list_x, list_y, list_w, list_h, ui::BASE);
    draw_rectangle_lines(list_x, list_y, list_w, list_h, 1.0, ui::EDGE);
    let row_h = 73.0;
    let visible = (list_h / row_h).floor() as usize;
    let max_first = snapshot.lines.len().saturating_sub(visible);
    view.scroll = view.scroll.min(max_first);
    let mouse = vec2(mouse_position().0, mouse_position().1);
    if Rect::new(list_x, list_y, list_w, list_h).contains(mouse) {
        let (_, wheel_y) = mouse_wheel();
        if wheel_y > 0.0 {
            view.scroll = view.scroll.saturating_sub(2);
        }
        if wheel_y < 0.0 {
            view.scroll = (view.scroll + 2).min(max_first);
        }
    }

    ui::scrollbar(
        Rect::new(list_x, list_y, list_w, list_h),
        &mut view.scroll,
        max_first,
        visible,
    );
    let mut action: Option<Action> = None;
    for (index, line) in snapshot
        .lines
        .iter()
        .enumerate()
        .skip(view.scroll)
        .take(visible)
    {
        let y = list_y + (index - view.scroll) as f32 * row_h + 2.0;
        let row = Rect::new(list_x + 3.0, y, list_w - 6.0, row_h - 4.0);
        ui::row_bg(row, view.selected_line == Some(line.id), index % 2 != 0);
        let allocated = allocations[index].assigned;
        let icon_x = row.x + 6.0;
        icon(
            line.equipment,
            icon_x,
            row.y + 9.0,
            42.0,
            view.selected_line == Some(line.id),
        );
        if let Some(reason) = &line.blocked_reason {
            ui::tooltip(row, "Производство приостановлено", reason);
        }
        let tx = row.x + 56.0;
        let bx = row.x + row.w - 159.0;
        let tw = (bx - tx - 6.0).max(35.0);
        let title = compact_label(
            line.starting
                .as_ref()
                .and_then(|s| s.variant.as_deref())
                .or(line.model.as_deref())
                .or_else(|| line.starting.as_ref().map(|s| s.equipment.as_str()))
                .unwrap_or(line.equipment.label()),
            tw,
            if compact { 11 } else { 13 },
        );
        if ui::inside(Rect::new(icon_x, row.y + 3.0, (bx - icon_x).max(1.0), 42.0))
            && is_mouse_button_pressed(MouseButton::Left)
        {
            action = Some(Action::Select(line.id));
        }
        draw_ui_text(
            &title,
            tx,
            row.y + 18.0,
            if compact { 11.0 } else { 13.0 },
            ui::TEXT,
        );
        draw_ui_text(
            &format!(
                "{} {allocated}/{} · {:.1} ПМ/сут",
                if line.blocked_reason.is_some() {
                    "Ожидание:"
                } else {
                    "Заводы:"
                },
                line.requested_factories,
                line.last_day_ic
            ),
            tx,
            row.y + 35.0,
            10.0,
            ui::MUTED,
        );
        ui::progress_bar(
            Rect::new(tx, row.y + 42.0, tw, 8.0),
            (line.efficiency / rules.efficiency_cap) as f32,
        );
        draw_ui_text(
            &format!(
                "Эфф. {:.1}% · Сегодня +{}",
                line.efficiency * 100.0,
                line.last_day_units
            ),
            tx,
            row.y + 61.0,
            10.0,
            ui::GOLD,
        );

        let controls = [
            ("↑", 0.0),
            ("↓", 32.0),
            ("-", 64.0),
            ("+", 96.0),
            ("X", 128.0),
        ];
        for (i, (label, offset)) in controls.iter().enumerate() {
            if ui::small_button(
                label,
                Rect::new(
                    bx + offset,
                    row.y + 20.0,
                    if i == 4 { 27.0 } else { 29.0 },
                    27.0,
                ),
                false,
            ) {
                action = Some(match i {
                    0 => Action::Up(line.id),
                    1 => Action::Down(line.id),
                    2 => Action::AddFactories(line.id, -1),
                    3 => Action::AddFactories(line.id, 1),
                    _ => Action::Remove(line.id),
                });
            }
        }
    }
    if snapshot.lines.is_empty() {
        draw_ui_text(
            "Нет линий: выберите снаряжение и добавьте производство.",
            list_x + 9.0,
            list_y + 34.0,
            if compact { 10.0 } else { 12.0 },
            ui::MUTED,
        );
    }
    if visible < snapshot.lines.len() {
        if ui::small_button(
            "^",
            Rect::new(list_x + list_w - 57.0, list_y + list_h - 26.0, 24.0, 22.0),
            false,
        ) {
            view.scroll = view.scroll.saturating_sub(1);
        }
        if ui::small_button(
            "v",
            Rect::new(list_x + list_w - 28.0, list_y + list_h - 26.0, 24.0, 22.0),
            false,
        ) {
            view.scroll = (view.scroll + 1).min(max_first);
        }
    }

    draw_ui_text(
        "ВЫБРАТЬ СНАРЯЖЕНИЕ",
        list_x + 4.0,
        footer_y + 13.0,
        if compact { 10.0 } else { 12.0 },
        ui::GOLD,
    );
    let chip_y = footer_y + 18.0;
    let chip_w = (list_w - 16.0) / Equipment::ALL.len() as f32;
    for (index, equip) in Equipment::ALL.into_iter().enumerate() {
        let x = list_x + index as f32 * (chip_w + 4.0);
        let chosen = view.selected_equipment.unwrap_or(Equipment::InfantryKit) == equip;
        if ui::small_button(
            equip.short_label(),
            Rect::new(x, chip_y, chip_w, 28.0),
            chosen,
        ) {
            view.selected_equipment = Some(equip);
        }
    }
    let choice = view.selected_equipment.unwrap_or(Equipment::InfantryKit);
    let half = (list_w - 5.0) * 0.5;
    if ui::small_button(
        "ДОБАВИТЬ ЛИНИЮ",
        Rect::new(list_x, footer_y + 52.0, half, 29.0),
        false,
    ) {
        action = Some(Action::Create(choice));
    }
    if ui::small_button(
        "ЗАМЕНИТЬ СНАРЯЖЕНИЕ",
        Rect::new(list_x + half + 5.0, footer_y + 52.0, half, 29.0),
        false,
    ) {
        if let Some(id) = view.selected_line {
            action = Some(Action::Switch(id, choice));
        } else {
            view.notice = "Сначала выберите производственную линию".to_owned();
        }
    }
    // National equipment stockpiles are stored in the simulation, not inferred
    // from completed buildings or hardcoded starting-country values.
    let stock = if snapshot.model_stockpile.is_empty() {
        Equipment::ALL
            .iter()
            .map(|equipment| {
                let count = snapshot.stockpile.get(equipment).copied().unwrap_or(0);
                format!("{}:{count}", equipment.short_label())
            })
            .collect::<Vec<_>>()
            .join("  ")
    } else {
        snapshot
            .model_stockpile
            .iter()
            .map(|(id, n)| format!("{id}:{n}"))
            .collect::<Vec<_>>()
            .join("  ")
    };
    draw_ui_text(
        &compact_label(&stock, list_w - 5.0, 10),
        list_x,
        footer_y + 99.0,
        10.0,
        ui::TEXT,
    );
    ui::tooltip(
        Rect::new(list_x, footer_y + 84.0, list_w, 19.0),
        "Склад техники",
        &stock,
    );
    if !view.notice.is_empty() {
        draw_ui_text(
            &compact_label(&view.notice, list_w - 5.0, 10),
            list_x,
            footer_y + 117.0,
            10.0,
            ui::GOLD,
        );
    }

    if let Some(command) = action {
        let country = state.country_mut(tag);
        match command {
            Action::Up(id) => {
                if country.move_up(id) {
                    view.notice = "Приоритет линии повышен".to_owned();
                }
            }
            Action::Down(id) => {
                if country.move_down(id) {
                    view.notice = "Приоритет линии понижен".to_owned();
                }
            }
            Action::AddFactories(id, amount) => {
                if country.change_requested(id, amount, rules) {
                    view.notice = "Распределение заводов изменено".to_owned();
                }
            }
            Action::Remove(id) => {
                if country.remove(id) {
                    if view.selected_line == Some(id) {
                        view.selected_line = None;
                    }
                    view.notice = "Производственная линия удалена".to_owned();
                }
            }
            Action::Select(id) => {
                view.selected_line = Some(id);
            }
            Action::Create(equipment) => {
                if country.lines.len() < 128 {
                    if let Some(model) = default_model(military, tag, equipment) {
                        let id = country.create_model(&model, equipment, 1, rules);
                        view.selected_line = Some(id);
                        view.notice = format!(
                            "Линия {model} создана. Точные модели доступны в разделе «Войска → Техника»."
                        );
                    } else if military.catalogue.equipment.is_empty() {
                        let id = country.create(equipment, 1, rules);
                        view.selected_line = Some(id);
                        view.notice =
                            "Производственная линия прототипа: определения моделей не загружены"
                                .into();
                    } else {
                        view.notice = "В этой категории нет доступных моделей".into();
                    }
                } else {
                    view.notice = "Достигнут лимит: 128 производственных линий".to_owned();
                }
            }
            Action::Switch(id, equipment) => {
                let changed = if let Some(model) = default_model(military, tag, equipment) {
                    country.switch_model(id, &model, equipment, rules)
                } else if military.catalogue.equipment.is_empty() {
                    country.switch_equipment(id, equipment, rules)
                } else {
                    false
                };
                if changed {
                    view.notice =
                        "Снаряжение заменено с сохранением части эффективности".to_owned();
                }
            }
        }
    }
}

fn default_model(
    military: &crate::warfare::Warfare,
    tag: &str,
    category: Equipment,
) -> Option<String> {
    let army = military.countries.get(tag)?;
    military
        .catalogue
        .equipment
        .values()
        .filter(|e| {
            military
                .catalogue
                .equipment_available(&e.id, &army.modifiers)
                && e.cost > 0.0
        })
        .filter(|e| match category {
            Equipment::InfantryKit => e.archetype.contains("infantry"),
            Equipment::SupportKit => e.archetype.contains("support"),
            Equipment::Truck => e.archetype.contains("motorized"),
            Equipment::Aircraft => crate::air_warfare::is_aircraft(e),
            Equipment::FieldEquipment => {
                e.archetype.contains("artillery")
                    || e.archetype.contains("anti_tank")
                    || e.archetype.contains("tank")
            }
        })
        .max_by_key(|e| (e.year, &e.id))
        .map(|e| e.id.clone())
}
