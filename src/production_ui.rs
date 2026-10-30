//! HOI4-inspired production management window. All values and controls are
//! backed by real daily simulation in production.rs. Artwork is procedural.
use crate::hoi_ui as ui;
use crate::production::{Equipment, ProductionRules, ProductionState};
use crate::world::World;
use crate::{draw_ui_text, measure_ui_text};
use macroquad::prelude::*;

#[derive(Debug, Default)]
pub struct ProductionView {
    pub open: bool,
    pub selected_line: Option<u64>,
    pub selected_equipment: Option<Equipment>,
    pub catalogue_open: bool,
    pub catalogue_scroll: usize,
    pub switch_target: Option<u64>,
    pub scroll: usize,
    pub notice: String,
    pub drag_source: Option<u64>,
}

impl ProductionView {
    pub fn close(&mut self) {
        self.open = false;
        self.catalogue_open = false;
        self.drag_source = None;
    }
}

pub fn bounds() -> Rect {
    let layout = crate::ui_layout::Layout::current();
    let mut rect = layout.primary();
    if !layout.compact {
        rect.w = (layout.width * 0.67)
            .clamp(540.0, 920.0)
            .min(layout.width - 10.0);
    }
    rect
}

pub fn is_over_panel(point: Vec2, view: &ProductionView) -> bool {
    view.open && bounds().contains(point)
}

fn belongs(e: &crate::military_model::EquipmentDefinition, category: Equipment) -> bool {
    match category {
        Equipment::InfantryKit => e.archetype.contains("infantry"),
        Equipment::SupportKit => e.archetype.contains("support"),
        Equipment::Truck => e.archetype.contains("motorized") || e.archetype.contains("mechanized"),
        Equipment::Aircraft => crate::air_warfare::is_aircraft(e),
        Equipment::FieldEquipment => {
            e.archetype.contains("artillery")
                || e.archetype.contains("anti_tank")
                || e.archetype.contains("anti_air")
                || e.archetype.contains("tank")
        }
    }
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
    CreateModel(Equipment, String),
    SwitchModel(u64, Equipment, String),
    Reorder(u64, usize),
}

pub fn draw(
    world: Option<&World>,
    tag: Option<&str>,
    state: &mut ProductionState,
    view: &mut ProductionView,
    rules: &ProductionRules,
    military: &crate::warfare::Warfare,
    resources: &crate::resource_economy::ResourceEconomy,
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
        Rect::new(area.right() - 38.0, area.y + 7.0, 27.0, 25.0),
        false,
    ) {
        view.close();
        return;
    }
    let (Some(world), Some(tag)) = (world, tag) else {
        ui::paragraph(
            "Загрузите карту и выберите страну.",
            Rect::new(area.x + 15., area.y + 65., area.w - 30., 70.),
            13.,
            ui::MUTED,
        );
        return;
    };
    let Some(economy) = world.country_economy(tag) else {
        ui::paragraph(
            "Экономика выбранной страны не загружена.",
            Rect::new(area.x + 15., area.y + 65., area.w - 30., 70.),
            13.,
            ui::MUTED,
        );
        return;
    };
    let snapshot = state.country(tag).cloned().unwrap_or_default();
    if view
        .selected_line
        .is_some_and(|id| !snapshot.lines.iter().any(|line| line.id == id))
    {
        view.selected_line = None;
    }
    let allocations = snapshot.allocations(economy.military_factories, rules);
    let assigned: u32 = allocations.iter().map(|a| a.assigned).sum();
    let free = economy.military_factories.saturating_sub(assigned);
    let mut action: Option<Action> = None;

    // HOI4-style resource strip: use *settled* national resource flows, never
    // invented stockpiles. Missing settlement is shown as unknown, not zero.
    let resource_y = area.y + 43.0;
    let resource_keys = [
        ("НЕФТЬ", "oil"),
        ("АЛЮМ.", "aluminium"),
        ("РЕЗИНА", "rubber"),
        ("СТАЛЬ", "steel"),
        ("ВОЛЬФ.", "tungsten"),
        ("ХРОМ", "chromium"),
    ];
    let strip_x = area.x + 9.0;
    let strip_w = area.w - 18.0;
    let cell_w = strip_w / resource_keys.len() as f32;
    let settlement = resources.last.get(tag);
    for (i, &(label, id)) in resource_keys.iter().enumerate() {
        let x = strip_x + i as f32 * cell_w;
        let r = Rect::new(x, resource_y, cell_w - 2.0, 27.0);
        draw_rectangle(r.x, r.y, r.w, r.h, ui::BASE);
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, ui::EDGE);
        let amount = settlement.and_then(|s| {
            s.available.get(id).or_else(|| {
                if id == "aluminium" {
                    s.available.get("aluminum")
                } else {
                    None
                }
            })
        });
        let amount_text = amount.map(|v| v.to_string()).unwrap_or_else(|| "—".into());
        draw_ui_text(
            label,
            r.x + 4.0,
            r.y + 10.0,
            if compact { 8.0 } else { 10.0 },
            ui::MUTED,
        );
        let width = measure_ui_text(&amount_text, 11).width;
        draw_ui_text(
            &amount_text,
            (r.right() - width - 4.0).max(r.x + 3.0),
            r.y + 22.0,
            11.0,
            ui::GOLD,
        );
        let detail = settlement
            .map(|s| {
                format!(
                    "Добыча: {} · импорт: {} · экспортный резерв: {} · доступно: {}",
                    s.extracted.get(id).copied().unwrap_or(0),
                    s.imported.get(id).copied().unwrap_or(0),
                    s.export_reserved.get(id).copied().unwrap_or(0),
                    amount.copied().unwrap_or(0),
                )
            })
            .unwrap_or_else(|| "Данные о ресурсах ещё не рассчитаны".to_owned());
        ui::tooltip(r, label, &detail);
    }

    let industry_y = resource_y + 31.0;
    let half_w = (strip_w - 4.0) * 0.5;
    ui::stat(
        strip_x,
        industry_y,
        half_w,
        "ВОЕННЫЕ ЗАВОДЫ",
        &format!(
            "{assigned}/{}  (+{free} свободно)",
            economy.military_factories
        ),
    );
    ui::stat(
        strip_x + half_w + 4.0,
        industry_y,
        half_w,
        "ВЕРФИ",
        &format!("{} · морские линии ещё не подключены", economy.dockyards),
    );
    ui::tooltip(
        Rect::new(strip_x + half_w + 4.0, industry_y, half_w, 35.0),
        "Военно-морское производство",
        "Число верфей действительно, но симуляция корабельных заказов не готова. Военные заводы не могут заменять верфи.",
    );

    let tools_y = industry_y + 39.0;
    let button_w = if compact {
        (strip_w - 8.0) / 2.0
    } else {
        138.0
    };
    if ui::small_button(
        "+ НОВАЯ ЛИНИЯ",
        Rect::new(strip_x, tools_y, button_w, 29.0),
        false,
    ) {
        view.catalogue_open = true;
        view.switch_target = None;
        view.catalogue_scroll = 0;
    }
    if ui::small_button(
        "СМЕНИТЬ МОДЕЛЬ",
        Rect::new(strip_x + button_w + 7.0, tools_y, button_w, 29.0),
        false,
    ) {
        if let Some(id) = view.selected_line {
            view.catalogue_open = true;
            view.switch_target = Some(id);
            view.catalogue_scroll = 0;
        } else {
            view.notice = "Выберите производственную линию для замены".into();
        }
    }
    if !compact {
        draw_ui_text(
            "Приоритет: сверху вниз · перетаскивайте номер линии",
            strip_x + button_w * 2.0 + 22.0,
            tools_y + 19.0,
            11.0,
            ui::MUTED,
        );
    }

    let content_y = tools_y + 34.0;
    let content_h = (area.bottom() - content_y - 31.0).max(36.0);
    let sidebar_w = if compact {
        0.0
    } else {
        (area.w * 0.38).clamp(220.0, 340.0)
    };
    let list_rect = Rect::new(
        area.x + 8.0,
        content_y,
        (area.w - sidebar_w - if compact { 16.0 } else { 23.0 }).max(180.0),
        content_h,
    );
    let catalogue_rect = if compact {
        Rect::new(area.x + 8.0, content_y, area.w - 16.0, content_h)
    } else {
        Rect::new(list_rect.right() + 7.0, content_y, sidebar_w, content_h)
    };
    draw_rectangle(list_rect.x, list_rect.y, list_rect.w, list_rect.h, ui::BASE);
    draw_rectangle_lines(
        list_rect.x,
        list_rect.y,
        list_rect.w,
        list_rect.h,
        1.0,
        ui::EDGE,
    );
    ui::section(
        &format!("ЛИНИИ ПРОИЗВОДСТВА ({})", snapshot.lines.len()),
        list_rect.x,
        list_rect.y,
        list_rect.w,
    );
    let inner = Rect::new(
        list_rect.x + 4.0,
        list_rect.y + 26.0,
        (list_rect.w - 14.0).max(30.0),
        (list_rect.h - 31.0).max(8.0),
    );
    let row_h = 89.0;
    let visible = ((inner.h / row_h).floor() as usize).max(1);
    let max_first = snapshot.lines.len().saturating_sub(visible);
    view.scroll = view.scroll.min(max_first);
    let mouse = vec2(mouse_position().0, mouse_position().1);
    if inner.contains(mouse) && !(compact && view.catalogue_open) {
        let wheel = mouse_wheel().1;
        if wheel > 0.0 {
            view.scroll = view.scroll.saturating_sub(2);
        } else if wheel < 0.0 {
            view.scroll = (view.scroll + 2).min(max_first);
        }
    }
    if !(compact && view.catalogue_open) {
        ui::scrollbar(inner, &mut view.scroll, max_first, visible);
        {
            let _clip = ui::clip(inner);
            for (index, line) in snapshot
                .lines
                .iter()
                .enumerate()
                .skip(view.scroll)
                .take(visible)
            {
                let y = inner.y + (index - view.scroll) as f32 * row_h;
                let row = Rect::new(inner.x + 1.0, y + 1.0, inner.w - 3.0, row_h - 3.0);
                let selected = view.selected_line == Some(line.id);
                ui::row_bg(row, selected, index % 2 != 0);
                if ui::inside(row) && is_mouse_button_pressed(MouseButton::Left) {
                    action = Some(Action::Select(line.id));
                }
                let rank = Rect::new(row.x + 3.0, row.y + 3.0, 24.0, 23.0);
                if ui::small_button(&format!("{}", index + 1), rank, false) {
                    view.drag_source = Some(line.id);
                    action = Some(Action::Select(line.id));
                }
                ui::tooltip(
                    rank,
                    "Порядок линий",
                    "Удерживайте номер и перетаскивайте линию вверх или вниз",
                );
                let picture = Rect::new(row.x + 29.0, row.y + 5.0, 46.0, 46.0);
                let has_art = line
                    .model
                    .as_deref()
                    .is_some_and(|id| crate::hoi_assets::draw_equipment(id, picture));
                if !has_art {
                    icon(line.equipment, picture.x, picture.y, picture.w, selected);
                }
                let right_x = row.right() - 136.0;
                let text_x = picture.right() + 6.0;
                let text_w = (right_x - text_x - 5.0).max(20.0);
                let model_name = line
                    .starting
                    .as_ref()
                    .and_then(|s| s.variant.as_deref())
                    .or(line.model.as_deref())
                    .or_else(|| line.starting.as_ref().map(|s| s.equipment.as_str()))
                    .unwrap_or(line.equipment.label());
                draw_ui_text(
                    &compact_label(model_name, text_w, 12),
                    text_x,
                    row.y + 17.0,
                    12.0,
                    ui::TEXT,
                );
                let assigned_here = allocations[index].assigned;
                let amount = line
                    .model
                    .as_deref()
                    .and_then(|id| snapshot.model_stockpile.get(id).copied())
                    .unwrap_or_else(|| {
                        snapshot
                            .stockpile
                            .get(&line.equipment)
                            .copied()
                            .unwrap_or(0)
                    });
                draw_ui_text(
                    &compact_label(
                        &format!("Склад: {amount}  ·  +{}/сут", line.last_day_units),
                        text_w,
                        10,
                    ),
                    text_x,
                    row.y + 34.0,
                    10.0,
                    ui::GREEN,
                );
                if line.blocked_reason.is_some() {
                    draw_ui_text(
                        "ПРОИЗВОДСТВО ЗАБЛОКИРОВАНО",
                        row.x + 6.0,
                        row.y + 66.0,
                        9.0,
                        ui::GOLD,
                    );
                } else {
                    let factor = snapshot
                        .last_day_resource_factors
                        .get(&line.id)
                        .copied()
                        .unwrap_or(1.0);
                    draw_ui_text(
                        &compact_label(
                            &format!(
                                "Заводы {assigned_here}/{} · ресурсы {:.0}%",
                                line.requested_factories,
                                factor * 100.0
                            ),
                            row.w - 12.0,
                            10,
                        ),
                        row.x + 6.0,
                        row.y + 80.0,
                        10.0,
                        ui::MUTED,
                    );
                }
                let eff_x = text_x;
                let eff_w = (row.right() - eff_x - 6.0).max(10.0);
                ui::progress_bar(
                    Rect::new(eff_x, row.y + 50.0, eff_w, 8.0),
                    (line.efficiency / rules.efficiency_cap.max(0.01)) as f32,
                );
                draw_ui_text(
                    &format!("Эффективность {:.1}%", line.efficiency * 100.0),
                    eff_x,
                    row.y + 68.0,
                    10.0,
                    ui::GOLD,
                );
                // Exact allocation uses the country's prioritized factory pool.
                // The small squares visualize its first fifteen factories; the
                // numeric count stays authoritative for larger allocations.
                let pip_y = row.y + 39.0;
                let pips_x = right_x + 2.0;
                let pip_count = 15u32;
                for n in 0..pip_count {
                    let x = pips_x + n as f32 * 8.2;
                    let colour = if n < assigned_here {
                        ui::GREEN
                    } else if n < line.requested_factories {
                        ui::GOLD
                    } else {
                        ui::BASE
                    };
                    draw_rectangle(x, pip_y, 7.2, 7.0, colour);
                    draw_rectangle_lines(x, pip_y, 7.2, 7.0, 0.7, ui::EDGE);
                }
                let controls = [
                    ("↑", 0.0),
                    ("↓", 27.0),
                    ("-", 54.0),
                    ("+", 81.0),
                    ("X", 108.0),
                ];
                for (control, &(label, offset)) in controls.iter().enumerate() {
                    let r = Rect::new(right_x + offset, row.y + 6.0, 25.0, 27.0);
                    if ui::small_button(label, r, false) {
                        let shift =
                            is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
                        action = Some(match control {
                            0 => Action::Up(line.id),
                            1 => Action::Down(line.id),
                            2 => Action::AddFactories(line.id, if shift { -5 } else { -1 }),
                            3 => Action::AddFactories(line.id, if shift { 5 } else { 1 }),
                            _ => Action::Remove(line.id),
                        });
                    }
                }
                if let Some(reason) = &line.blocked_reason {
                    ui::tooltip(row, "Линия не работает", reason);
                } else {
                    let requirements = line
                        .model
                        .as_deref()
                        .and_then(|id| military.catalogue.equipment_resources.get(id));
                    if let Some(requirements) = requirements {
                        let parts = requirements
                            .iter()
                            .filter(|(_, n)| **n > 0)
                            .map(|(id, n)| format!("{id}: {n}"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        ui::tooltip(
                            row,
                            "Материалы на завод за сутки",
                            &format!("{parts} · Shift +/- меняет сразу 5 заводов"),
                        );
                    }
                }
            }
        }
    }
    if snapshot.lines.is_empty() {
        ui::paragraph(
            "Нет производственных линий. Выберите изученную модель справа и создайте линию.",
            Rect::new(inner.x + 8., inner.y + 14., inner.w - 22., 70.),
            12.,
            ui::MUTED,
        );
    }
    // Drag handle only; ordinary taps on cards and +/- remain independent.
    if !(compact && view.catalogue_open) {
        if let Some(id) = view.drag_source {
            if is_mouse_button_released(MouseButton::Left) {
                if inner.contains(mouse) && !snapshot.lines.is_empty() {
                    let local = ((mouse.y - inner.y) / row_h).floor().max(0.0) as usize;
                    let target = (view.scroll + local).min(snapshot.lines.len() - 1);
                    action = Some(Action::Reorder(id, target));
                }
                view.drag_source = None;
            } else if !is_mouse_button_down(MouseButton::Left) {
                view.drag_source = None;
            }
        }
    }

    // The equipment catalogue is the second column on desktop and a separate
    // page in a narrow mobile viewport. Both use the same live catalogue.
    if !compact || view.catalogue_open {
        ui::panel(
            catalogue_rect.x,
            catalogue_rect.y,
            catalogue_rect.w,
            catalogue_rect.h,
        );
        ui::section(
            if view.switch_target.is_some() {
                "ЗАМЕНА МОДЕЛИ"
            } else {
                "НОВАЯ ЛИНИЯ"
            },
            catalogue_rect.x,
            catalogue_rect.y,
            catalogue_rect.w,
        );
        if compact
            && ui::small_button(
                "НАЗАД",
                Rect::new(
                    catalogue_rect.right() - 80.,
                    catalogue_rect.y + 1.,
                    76.,
                    21.,
                ),
                false,
            )
        {
            view.catalogue_open = false;
        }
        if view.switch_target.is_some()
            && ui::small_button(
                "+ НОВАЯ",
                Rect::new(
                    catalogue_rect.x + 5.,
                    catalogue_rect.y + 25.,
                    catalogue_rect.w - 10.,
                    22.,
                ),
                false,
            )
        {
            view.switch_target = None;
        }
        let chips_y = catalogue_rect.y
            + if view.switch_target.is_some() {
                50.0
            } else {
                26.0
            };
        let chip_w = (catalogue_rect.w - 12.0) / Equipment::ALL.len() as f32;
        for (i, equipment) in Equipment::ALL.iter().copied().enumerate() {
            let selected = view.selected_equipment.unwrap_or(Equipment::InfantryKit) == equipment;
            if ui::small_button(
                equipment.short_label(),
                Rect::new(
                    catalogue_rect.x + 4.0 + i as f32 * chip_w,
                    chips_y,
                    chip_w - 2.0,
                    28.0,
                ),
                selected,
            ) {
                view.selected_equipment = Some(equipment);
                view.catalogue_scroll = 0;
            }
        }
        let choice = view.selected_equipment.unwrap_or(Equipment::InfantryKit);
        let catalogue_label_y = chips_y + 32.0;
        draw_ui_text(
            &compact_label(choice.label(), catalogue_rect.w - 12.0, 12),
            catalogue_rect.x + 7.0,
            catalogue_label_y + 12.0,
            12.0,
            ui::GOLD,
        );
        let cat_inner = Rect::new(
            catalogue_rect.x + 5.0,
            catalogue_label_y + 18.0,
            catalogue_rect.w - 18.0,
            (catalogue_rect.bottom() - catalogue_label_y - 24.0).max(8.0),
        );
        let army = military.countries.get(tag);
        let mut models = military
            .catalogue
            .equipment
            .values()
            .filter(|e| belongs(e, choice))
            .filter(|e| e.cost.is_finite() && e.cost > 0.0)
            .collect::<Vec<_>>();
        models.sort_by(|a, b| a.year.cmp(&b.year).then_with(|| a.id.cmp(&b.id)));
        let cat_h = 54.0;
        let visible_models = ((cat_inner.h / cat_h).floor() as usize).max(1);
        let max_cat = models.len().saturating_sub(visible_models);
        view.catalogue_scroll = view.catalogue_scroll.min(max_cat);
        if cat_inner.contains(mouse) {
            let wheel = mouse_wheel().1;
            if wheel > 0.0 {
                view.catalogue_scroll = view.catalogue_scroll.saturating_sub(1);
            } else if wheel < 0.0 {
                view.catalogue_scroll = (view.catalogue_scroll + 2).min(max_cat);
            }
        }
        ui::scrollbar(
            cat_inner,
            &mut view.catalogue_scroll,
            max_cat,
            visible_models,
        );
        {
            let _clip = ui::clip(cat_inner);
            for (index, model) in models
                .iter()
                .enumerate()
                .skip(view.catalogue_scroll)
                .take(visible_models)
            {
                let y = cat_inner.y + (index - view.catalogue_scroll) as f32 * cat_h;
                let row = Rect::new(cat_inner.x + 1.0, y + 1.0, cat_inner.w - 3.0, cat_h - 3.0);
                let available = army.is_some_and(|army| {
                    military
                        .catalogue
                        .equipment_available(&model.id, &army.modifiers)
                });
                ui::row_bg(row, false, index % 2 != 0);
                if available && ui::inside(row) && is_mouse_button_pressed(MouseButton::Left) {
                    action = Some(match view.switch_target {
                        Some(id) => Action::SwitchModel(id, choice, model.id.clone()),
                        None => Action::CreateModel(choice, model.id.clone()),
                    });
                    view.catalogue_open = false;
                    view.switch_target = None;
                }
                let pic = Rect::new(row.x + 4., row.y + 5., 42., 40.);
                if !crate::hoi_assets::draw_equipment(&model.id, pic) {
                    crate::division_icons::draw(&model.archetype, &model.archetype, pic);
                }
                let colour = if available { ui::TEXT } else { ui::MUTED };
                draw_ui_text(
                    &compact_label(&model.id, row.w - 56., 11),
                    row.x + 52.,
                    row.y + 18.,
                    11.,
                    colour,
                );
                draw_ui_text(
                    &format!(
                        "{} · {:.2} ПМ {}",
                        model.year,
                        model.cost,
                        if available { "+" } else { "ЗАКРЫТО" }
                    ),
                    row.x + 52.,
                    row.y + 38.,
                    10.,
                    if available { ui::GREEN } else { ui::GOLD },
                );
                let detail = military
                    .catalogue
                    .equipment_resources
                    .get(&model.id)
                    .map(|r| {
                        r.iter()
                            .map(|(k, n)| format!("{k} {n}"))
                            .collect::<Vec<_>>()
                            .join(" · ")
                    })
                    .unwrap_or_default();
                ui::tooltip(
                    row,
                    if available {
                        "Начать производство"
                    } else {
                        "Недоступная модель"
                    },
                    if available {
                        &detail
                    } else {
                        "Требуется исследование или поддержка модульной модели"
                    },
                );
            }
        }
        if models.is_empty() {
            ui::paragraph(
                "Нет импортированных моделей этого типа. Проверьте экспорт данных HOI4.",
                Rect::new(cat_inner.x + 5., cat_inner.y + 12., cat_inner.w - 15., 72.),
                11.,
                ui::MUTED,
            );
        }
        // Old category-only saves can still be edited without inventing a
        // concrete model when no catalogue has been loaded at all.
        if military.catalogue.equipment.is_empty() {
            if ui::small_button(
                "+ ПРОТОТИП",
                Rect::new(
                    catalogue_rect.x + 6.,
                    catalogue_rect.bottom() - 34.,
                    catalogue_rect.w - 12.,
                    28.,
                ),
                false,
            ) {
                action = Some(match view.switch_target {
                    Some(id) => Action::Switch(id, choice),
                    None => Action::Create(choice),
                });
            }
        }
    }
    let footer = area.bottom() - 24.0;
    draw_rectangle(area.x + 7., footer - 3., area.w - 14., 23., ui::BASE);
    let selected_stock = view
        .selected_line
        .and_then(|id| snapshot.lines.iter().find(|l| l.id == id))
        .map(|line| {
            let count = line
                .model
                .as_deref()
                .and_then(|m| snapshot.model_stockpile.get(m).copied())
                .unwrap_or_else(|| {
                    snapshot
                        .stockpile
                        .get(&line.equipment)
                        .copied()
                        .unwrap_or(0)
                });
            format!(
                "Выбрано: {} · склад: {count}",
                line.model.as_deref().unwrap_or(line.equipment.label())
            )
        })
        .unwrap_or_else(|| {
            format!(
                "Военных заводов: {} · свободно: {free}",
                economy.military_factories
            )
        });
    let message = if view.notice.is_empty() {
        &selected_stock
    } else {
        &view.notice
    };
    draw_ui_text(
        &compact_label(message, area.w - 26., 11),
        area.x + 12.,
        footer + 13.,
        11.,
        ui::GOLD,
    );

    if let Some(command) = action {
        let country = state.country_mut(tag);
        match command {
            Action::Reorder(id, target) => {
                if let Some(mut pos) = country.lines.iter().position(|line| line.id == id) {
                    let last = country.lines.len().saturating_sub(1);
                    let target = target.min(last);
                    while pos > target {
                        if !country.move_up(id) {
                            break;
                        }
                        pos -= 1;
                    }
                    while pos < target {
                        if !country.move_down(id) {
                            break;
                        }
                        pos += 1;
                    }
                    view.notice = "Приоритет производственной линии изменён".into();
                }
            }
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
            Action::CreateModel(equipment, model) => {
                let permitted = military.countries.get(tag).is_some_and(|army| {
                    military
                        .catalogue
                        .equipment_available(&model, &army.modifiers)
                });
                if permitted && country.lines.len() < 128 {
                    view.selected_line = Some(country.create_model(&model, equipment, 1, rules));
                    view.notice = format!("Начато производство {model}. Заводы: + / Shift +");
                } else {
                    view.notice = "Модель не изучена или достигнут лимит линий".into();
                }
            }
            Action::SwitchModel(id, equipment, model) => {
                let permitted = military.countries.get(tag).is_some_and(|army| {
                    military
                        .catalogue
                        .equipment_available(&model, &army.modifiers)
                });
                if permitted && country.switch_model(id, &model, equipment, rules) {
                    view.notice = format!("Линия переведена на {model}");
                } else {
                    view.notice = "Недоступная модель или линия".into();
                }
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
        .filter(|e| belongs(e, category))
        .max_by_key(|e| (e.year, &e.id))
        .map(|e| e.id.clone())
}
