//! Scrollable division editor. Draft edits are pure; Save is an XP transaction
//! and assignment/reinforcement operate on the existing campaign inventories.
use crate::{
    hoi_ui as ui,
    military_model::{BattalionSlot, DivisionTemplate, SupportSlot},
    production::ProductionState,
    warfare::Warfare,
    world::World,
};
use macroquad::prelude::*;
use std::collections::BTreeSet;
#[derive(Debug, Default)]
pub struct DesignerView {
    pub template_index: usize,
    pub draft: Option<DivisionTemplate>,
    pub editing: Option<String>,
    pub name_active: bool,
    new_draft: bool,
    page: usize,
    scroll: f32,
    drag: Option<Vec2>,
    slot: (usize, usize),
    support: bool,
    regimental: bool,
    picker: bool,
}
fn unique_name(w: &Warfare, tag: &str, base: &str) -> String {
    let army = &w.countries[tag];
    if !army.templates.contains_key(base) {
        return base.into();
    }
    for i in 2..10000 {
        let name = format!("{base} {i}");
        if !army.templates.contains_key(&name) {
            return name;
        }
    }
    format!("{base} {}", army.templates.len() + 1)
}
pub fn draw(
    world: &World,
    tag: &str,
    w: &mut Warfare,
    production: &mut ProductionState,
    province: Option<u32>,
    selected: &BTreeSet<u64>,
    v: &mut DesignerView,
    notice: &mut String,
    r: Rect,
) {
    let Some(army) = w.countries.get(tag) else {
        return;
    };
    let names: Vec<_> = army.templates.keys().cloned().collect();
    let modifiers = army.modifiers.clone();
    let xp = army.army_xp;
    if v.draft.is_none() {
        if !names.is_empty() {
            v.template_index = v.template_index.min(names.len() - 1);
            v.editing = Some(names[v.template_index].clone());
            v.draft = Some(army.templates[&names[v.template_index]].clone());
            v.new_draft = false;
        } else {
            v.new_draft = true;
            v.editing = None;
            v.draft = Some(DivisionTemplate {
                name: "Новый шаблон".into(),
                icon: "infantry".into(),
                ..Default::default()
            });
        }
    }
    let x = r.x;
    let width = r.w;
    let mut y = r.y;
    let third = (width - 8.0) / 3.0;
    if ui::small_button("НОВЫЙ", Rect::new(x, y, third, 26.0), false) {
        v.draft = Some(DivisionTemplate {
            name: unique_name(w, tag, "Новый шаблон"),
            icon: "infantry".into(),
            ..Default::default()
        });
        v.new_draft = true;
        v.editing = None;
        v.scroll = 0.0;
    }
    if ui::small_button("КОПИЯ", Rect::new(x + third + 4.0, y, third, 26.0), false) {
        if let Some(old) = v.editing.clone() {
            let name = unique_name(w, tag, &format!("{old} (копия)"));
            match w.copy_template(tag, &old, &name) {
                Ok(()) => {
                    v.draft = Some(w.countries[tag].templates[&name].clone());
                    v.editing = Some(name);
                    v.new_draft = false;
                    *notice = "Копия создана без затрат XP".into();
                }
                Err(e) => *notice = e,
            }
        }
    }
    if ui::small_button(
        "ЗНАЧОК",
        Rect::new(x + 2.0 * (third + 4.0), y, third, 26.0),
        false,
    ) {
        let t = v.draft.as_mut().unwrap();
        t.icon = match t.icon.as_str() {
            "infantry" => "armor",
            "armor" => "motorized",
            "motorized" => "artillery",
            _ => "infantry",
        }
        .into();
    }
    y += 32.0;
    if !v.new_draft && !names.is_empty() {
        if ui::small_button("◀", Rect::new(x, y, 25.0, 27.0), false) {
            v.template_index = (v.template_index + names.len() - 1) % names.len();
            v.draft = None;
            v.editing = None;
            v.scroll = 0.0;
            return;
        }
        if ui::small_button("▶", Rect::new(x + width - 25.0, y, 25.0, 27.0), false) {
            v.template_index = (v.template_index + 1) % names.len();
            v.draft = None;
            v.editing = None;
            v.scroll = 0.0;
            return;
        }
    }
    let name = v.draft.as_ref().unwrap().name.clone();
    if ui::small_button(
        &name,
        Rect::new(x + 30.0, y, width - 60.0, 27.0),
        v.name_active,
    ) {
        v.name_active = true;
        macroquad::miniquad::window::show_keyboard(true);
    }
    if v.name_active {
        let t = v.draft.as_mut().unwrap();
        while let Some(ch) = get_char_pressed() {
            if !ch.is_control() && t.name.chars().count() < 64 {
                t.name.push(ch);
            }
        }
        if is_key_pressed(KeyCode::Backspace) {
            t.name.pop();
        }
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Escape) {
            v.name_active = false;
            macroquad::miniquad::window::show_keyboard(false);
        }
    }
    y += 34.0;
    let titles = ["СОСТАВ", "ПАРАМЕТРЫ", "ТЕХНИКА", "МЕСТНОСТЬ"];
    let tab = width / 4.0;
    for (i, title) in titles.iter().enumerate() {
        if ui::small_button(
            title,
            Rect::new(x + i as f32 * tab, y, tab - 3.0, 26.0),
            v.page == i,
        ) {
            v.page = i;
            v.scroll = 0.0;
            v.picker = false;
        }
    }
    y += 32.0;
    let body = Rect::new(x, y, width, (r.bottom() - y - 70.0).max(1.0));
    let draft = v.draft.as_ref().unwrap().clone();
    let preview = w.catalogue.aggregate(&draft, &modifiers, None);
    let mut total = if v.page == 0 { 440.0 } else { 560.0 };
    if v.picker {
        total = w.catalogue.units.len() as f32 * 35.0;
    }
    if ui::inside(body) {
        let (_, wheel) = mouse_wheel();
        v.scroll = (v.scroll - wheel * 32.0).clamp(0.0, (total - body.h).max(0.0));
        if is_mouse_button_pressed(MouseButton::Left) && !touches().is_empty() {
            v.drag = Some(vec2(mouse_position().0, mouse_position().1));
        }
    }
    if let Some(last) = v.drag {
        if is_mouse_button_down(MouseButton::Left) {
            let current = vec2(mouse_position().0, mouse_position().1);
            v.scroll = (v.scroll + last.y - current.y).clamp(0.0, (total - body.h).max(0.0));
            v.drag = Some(current);
        } else {
            v.drag = None;
        }
    }
    let clip = ui::clip(body);
    let top = body.y - v.scroll;
    if v.picker {
        let options: Vec<_> = w
            .catalogue
            .units
            .values()
            .filter(|u| {
                if v.support {
                    u.group == "support"
                        && if v.regimental {
                            u.regimental
                        } else {
                            u.divisional
                        }
                } else {
                    u.group != "support"
                }
            })
            .collect();
        if ui::small_button("← Назад к сетке", Rect::new(x, top, width, 28.0), false) {
            v.picker = false;
            v.scroll = 0.0;
        }
        for (i, u) in options.iter().enumerate() {
            let row = Rect::new(x, top + 34.0 + i as f32 * 35.0, width, 30.0);
            if !row.overlaps(&body) {
                continue;
            }
            let mut changed = draft.clone();
            if v.support {
                let regiment = v.regimental.then_some(v.slot.0);
                changed
                    .support
                    .retain(|s| !(s.regiment == regiment && s.row == v.slot.1));
                changed.support.push(SupportSlot {
                    unit: u.id.clone(),
                    regiment,
                    row: v.slot.1,
                });
            } else {
                changed
                    .line
                    .retain(|s| !(s.column == v.slot.0 && s.row == v.slot.1));
                changed.line.push(BattalionSlot {
                    unit: u.id.clone(),
                    column: v.slot.0,
                    row: v.slot.1,
                });
            }
            match w.catalogue.validate_template(&changed, &modifiers, true) {
                Ok(()) => {
                    if ui::small_button(w.catalogue.label(&u.id), row, false) {
                        v.draft = Some(changed);
                        v.picker = false;
                        v.scroll = 0.0;
                    }
                }
                Err(reason) => ui::disabled(w.catalogue.label(&u.id), row, &reason),
            }
        }
    } else if v.page == 0 {
        let columns = w.catalogue.rules.regiment_columns.min(12);
        let rows = w.catalogue.rules.regiment_rows.min(12);
        let supports = w.catalogue.rules.support_slots.min(12);
        let cell = ((width - 8.0) / (columns + 1) as f32).min(42.0);
        let start = top + 21.0;
        ui::label(
            "Боевые полки",
            Rect::new(x, top, width, 20.0),
            12.0,
            ui::GOLD,
        );
        for c in 0..columns {
            for row in 0..rows {
                let id = draft
                    .line
                    .iter()
                    .find(|s| s.column == c && s.row == row)
                    .map_or("+", |s| s.unit.as_str());
                if ui::small_button(
                    w.catalogue.label(id),
                    Rect::new(
                        x + c as f32 * cell,
                        start + row as f32 * cell,
                        cell - 2.0,
                        cell - 2.0,
                    ),
                    !v.support && v.slot == (c, row),
                ) {
                    v.slot = (c, row);
                    v.support = false;
                    v.regimental = false;
                    v.picker = true;
                    v.scroll = 0.0;
                }
            }
        }
        for row in 0..supports {
            let id = draft
                .support
                .iter()
                .find(|s| s.regiment.is_none() && s.row == row)
                .map_or("+", |s| s.unit.as_str());
            if ui::small_button(
                w.catalogue.label(id),
                Rect::new(
                    x + columns as f32 * cell + 7.0,
                    start + row as f32 * cell,
                    cell - 2.0,
                    cell - 2.0,
                ),
                v.support && !v.regimental && v.slot.1 == row,
            ) {
                v.slot.1 = row;
                v.support = true;
                v.regimental = false;
                v.picker = true;
                v.scroll = 0.0;
            }
        }
        let reg_y = start + rows.max(supports) as f32 * cell + 9.0;
        ui::label(
            "Полковая поддержка",
            Rect::new(x, reg_y, width, 20.0),
            12.0,
            ui::GOLD,
        );
        if w.catalogue.units.values().any(|u| u.regimental) {
            for c in 0..columns {
                let id = draft
                    .support
                    .iter()
                    .find(|s| s.regiment == Some(c) && s.row == 0)
                    .map_or("+", |s| s.unit.as_str());
                if ui::small_button(
                    w.catalogue.label(id),
                    Rect::new(x + c as f32 * cell, reg_y + 25.0, cell - 2.0, cell - 2.0),
                    v.regimental && v.slot == (c, 0),
                ) {
                    v.slot = (c, 0);
                    v.support = true;
                    v.regimental = true;
                    v.picker = true;
                    v.scroll = 0.0;
                }
            }
        } else {
            ui::disabled(
                "Нет импортированных полковых рот",
                Rect::new(x, reg_y + 24.0, width, 29.0),
                "Нужны определения Regimental Support целевой версии",
            );
        }
        let controls = reg_y + cell + 34.0;
        if ui::small_button(
            "Убрать выбранный компонент",
            Rect::new(x, controls, width, 28.0),
            false,
        ) {
            let t = v.draft.as_mut().unwrap();
            if v.support {
                let regiment = v.regimental.then_some(v.slot.0);
                t.support
                    .retain(|s| !(s.regiment == regiment && s.row == v.slot.1));
            } else {
                t.line
                    .retain(|s| !(s.column == v.slot.0 && s.row == v.slot.1));
            }
        }
        let role = draft.division_type.as_deref().unwrap_or("по составу");
        if ui::small_button(
            &format!("Тип: {}", w.catalogue.label(role)),
            Rect::new(x, controls + 35.0, width, 28.0),
            false,
        ) {
            v.draft.as_mut().unwrap().division_type = draft
                .line
                .iter()
                .find(|s| s.column == v.slot.0 && s.row == v.slot.1)
                .map(|s| s.unit.clone());
        }
    } else {
        match &preview {
            Err(error) => ui::paragraph(error, Rect::new(x, top, width, 100.0), 13.0, ui::GOLD),
            Ok(d) => {
                if v.page == 1 {
                    let s = &d.stats;
                    let values = [
                        ("Организация", s.organization),
                        ("HP", s.hp),
                        ("Восстановление / ч", s.recovery),
                        ("Soft Attack", s.soft_attack),
                        ("Hard Attack", s.hard_attack),
                        ("Защита", s.defense),
                        ("Прорыв", s.breakthrough),
                        ("Броня", s.armor),
                        ("Пробитие", s.piercing),
                        ("Hardness, %", s.hardness * 100.0),
                        ("Ширина боя", s.width),
                        ("Инициатива", s.initiative),
                        ("Разведка", s.recon),
                        ("Окапывание", s.entrenchment),
                        ("Скорость, км/ч", s.speed),
                        ("Снабжение", s.supply),
                        ("Топливо", s.fuel),
                        ("Надёжность, %", s.reliability * 100.0),
                        ("Людские ресурсы", s.manpower),
                        ("Стоимость IC", d.equipment_cost),
                        ("Обучение, дней", s.training),
                        ("Вес", s.weight),
                        ("ПВО", s.air_attack),
                    ];
                    for (i, (name, value)) in values.iter().enumerate() {
                        let row = Rect::new(x, top + i as f32 * 23.0, width, 22.0);
                        ui::row_bg(row, false, i % 2 == 1);
                        ui::label(
                            name,
                            Rect::new(row.x + 4.0, row.y + 2.0, row.w * 0.7, 19.0),
                            12.0,
                            ui::TEXT,
                        );
                        ui::label(
                            &format!("{value:.2}"),
                            Rect::new(row.x + row.w * 0.7, row.y + 2.0, row.w * 0.3 - 4.0, 19.0),
                            12.0,
                            ui::GOLD,
                        );
                    }
                } else if v.page == 2 {
                    let mut yy = top;
                    for (archetype, quantity) in &d.equipment {
                        let models: Vec<_> = w
                            .catalogue
                            .equipment
                            .values()
                            .filter(|e| {
                                e.archetype == *archetype
                                    && w.catalogue.equipment_available(&e.id, &modifiers)
                            })
                            .collect();
                        let preferred = draft
                            .equipment_preferences
                            .get(archetype)
                            .cloned()
                            .or_else(|| {
                                w.catalogue
                                    .best_equipment(archetype, &modifiers)
                                    .map(|e| e.id.clone())
                            });
                        let label = format!("{}: {:.0}", w.catalogue.label(archetype), quantity);
                        ui::label(&label, Rect::new(x, yy, width, 20.0), 12.0, ui::TEXT);
                        yy += 23.0;
                        let title = preferred
                            .as_ref()
                            .map_or("Нет доступной модели", |id| {
                                w.catalogue.label(id)
                            });
                        if ui::small_button(title, Rect::new(x, yy, width, 28.0), false)
                            && !models.is_empty()
                        {
                            let at = preferred
                                .as_ref()
                                .and_then(|id| models.iter().position(|e| e.id == *id))
                                .unwrap_or(0);
                            v.draft.as_mut().unwrap().equipment_preferences.insert(
                                archetype.clone(),
                                models[(at + 1) % models.len()].id.clone(),
                            );
                        }
                        yy += 36.0;
                    }
                } else {
                    for (i, (terrain, bonus)) in d.terrain.iter().enumerate() {
                        let yy = top + i as f32 * 49.0;
                        ui::label(terrain, Rect::new(x, yy, width, 20.0), 13.0, ui::GOLD);
                        ui::label(
                            &format!(
                                "А {:+.0}%  З {:+.0}%  скорость {:+.0}%  потери {:+.0}%",
                                bonus.attack * 100.0,
                                bonus.defense * 100.0,
                                bonus.movement * 100.0,
                                bonus.attrition * 100.0
                            ),
                            Rect::new(x, yy + 22.0, width, 20.0),
                            11.0,
                            ui::TEXT,
                        );
                    }
                }
            }
        }
    }
    drop(clip);
    let current = v.draft.as_ref().unwrap().clone();
    let cost = w.template_edit_cost(tag, v.editing.as_deref(), &current);
    let footer = r.bottom() - 64.0;
    let half = (width - 5.0) / 2.0;
    let title = format!(
        "Сохранить · {} XP",
        cost.as_ref().map_or("?".into(), |c| format!("{c:.0}"))
    );
    let reason = cost.as_ref().err().cloned().or_else(|| {
        cost.as_ref()
            .ok()
            .filter(|c| **c > xp + 1e-9)
            .map(|c| format!("Нужно {c:.0} XP, доступно {xp:.1}"))
    });
    if let Some(reason) = reason {
        ui::disabled(&title, Rect::new(x, footer, half, 28.0), &reason);
    } else if ui::small_button(&title, Rect::new(x, footer, half, 28.0), false) {
        match w.save_template(tag, v.editing.as_deref(), current.clone()) {
            Ok(()) => {
                let ids: Vec<_> = w
                    .divisions
                    .values()
                    .filter(|d| d.country == tag && d.template == current.name && !d.destroyed)
                    .map(|d| d.id)
                    .collect();
                for id in ids {
                    w.return_surplus(id, production);
                }
                v.editing = Some(current.name.clone());
                v.new_draft = false;
                v.template_index = w.countries[tag]
                    .templates
                    .keys()
                    .position(|n| *n == current.name)
                    .unwrap_or(0);
                *notice = "Шаблон сохранён; оснащение пополняется из запасов страны".into();
            }
            Err(e) => *notice = e,
        }
    }
    if ui::small_button(
        "Применить к выбранным",
        Rect::new(x + half + 5.0, footer, half, 28.0),
        false,
    ) {
        let ids: Vec<_> = selected.iter().copied().collect();
        *notice = w
            .apply_template(tag, &ids, &current.name, production)
            .map_or_else(
                |e| e,
                |_| "Шаблон назначен; недостающее оснащение запрошено".into(),
            );
    }
    if ui::small_button(
        "Развернуть из запасов",
        Rect::new(x, footer + 34.0, width, 27.0),
        false,
    ) {
        *notice = province
            .ok_or_else(|| "Выберите свою провинцию".into())
            .and_then(|p| w.recruit(tag, &current.name, p, world, production))
            .map_or_else(|e| e, |_| "Дивизия развёрнута".into());
    }
}
