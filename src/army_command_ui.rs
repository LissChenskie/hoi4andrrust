//! Command hierarchy controls operate on the same Warfare membership index
//! used by the map and the hourly planner.
use crate::{
    army_ui::ArmyView,
    command::{ArmyId, Rank},
    hoi_ui as ui,
    warfare::Warfare,
};
use macroquad::prelude::*;

pub fn draw(
    tag: &str,
    w: &mut Warfare,
    view: &mut ArmyView,
    r: Rect,
    world: &crate::world::World,
    production: &mut crate::production::ProductionState,
    province: Option<u32>,
) {
    let _clip = ui::clip(r);
    if view.commander_picker.is_none() && ui::inside(r) {
        let (_, wheel) = mouse_wheel();
        view.army_scroll = (view.army_scroll - wheel * 30.0).clamp(
            0.0,
            (1050.0 + w.abilities.definitions.len() as f32 * 33.0 - r.h).max(0.0),
        );
    }
    let touch = touches()
        .into_iter()
        .find(|t| !matches!(t.phase, TouchPhase::Ended | TouchPhase::Cancelled));
    if let Some(t) = touch.filter(|t| r.contains(t.position)) {
        if let Some(last) = view.command_touch_last {
            view.army_scroll = (view.army_scroll + last.y - t.position.y).max(0.0);
        }
        view.command_touch_last = Some(t.position);
    } else {
        view.command_touch_last = None;
    }
    let x = r.x;
    let width = r.w;
    let mut y = r.y
        - if view.commander_picker.is_none() {
            view.army_scroll
        } else {
            0.0
        };
    let armies: Vec<_> = w
        .command
        .armies
        .values()
        .filter(|a| a.country == tag)
        .map(|a| a.id)
        .collect();
    if !view.selected_army.is_some_and(|id| armies.contains(&id)) {
        view.selected_army = armies.first().copied();
    }
    if let Some(marshal) = view.commander_picker {
        ui::section(
            if marshal {
                "ВЫБРАТЬ ФЕЛЬДМАРШАЛА"
            } else {
                "ВЫБРАТЬ ГЕНЕРАЛА"
            },
            x,
            y,
            width,
        );
        y += 30.0;
        if ui::small_button("Без командующего", Rect::new(x, y, width, 28.0), false)
        {
            assign(tag, w, view, None, marshal);
        }
        y += 34.0;
        let choices: Vec<_> = w
            .command
            .commanders
            .values()
            .filter(|c| c.country == tag && (!marshal || c.rank == Rank::FieldMarshal))
            .cloned()
            .collect();
        let rows = ((r.bottom() - y - 36.0) / 38.0).max(0.0) as usize;
        let max = choices.len().saturating_sub(rows.max(1));
        if ui::inside(r) {
            let (_, wheel) = mouse_wheel();
            if wheel > 0.0 {
                view.command_scroll = view.command_scroll.saturating_sub(1);
            } else if wheel < 0.0 {
                view.command_scroll = (view.command_scroll + 1).min(max);
            }
        }
        view.command_scroll = view.command_scroll.min(max);
        for c in choices.iter().skip(view.command_scroll).take(rows) {
            let row = Rect::new(x, y, width, 32.0);
            if ui::small_button(
                &format!(
                    "{} · ур. {} · А{} З{} П{} Л{}",
                    c.name, c.skill, c.attack, c.defense, c.planning, c.logistics
                ),
                row,
                false,
            ) {
                assign(tag, w, view, Some(&c.id), marshal);
            }
            ui::tooltip(
                row,
                &c.name,
                &format!(
                    "Опыт {:.1}\nЧерты: {}",
                    c.experience,
                    c.traits.iter().cloned().collect::<Vec<_>>().join(", ")
                ),
            );
            y += 38.0;
        }
        if ui::small_button("Назад", Rect::new(x, r.bottom() - 28.0, width, 27.0), false) {
            view.commander_picker = None;
        }
        return;
    }
    let half = (width - 6.0) / 2.0;
    if ui::small_button("Создать армию", Rect::new(x, y, half, 29.0), false) {
        let ids: Vec<_> = view.selected.iter().copied().collect();
        match w.create_army(tag, &ids, "") {
            Ok(id) => {
                view.selected_army = Some(id);
                view.notice = "Армия создана".into();
            }
            Err(e) => view.notice = e,
        }
    }
    if ui::small_button(
        "Добавить выделенные",
        Rect::new(x + half + 6.0, y, half, 29.0),
        false,
    ) {
        if let Some(id) = view.selected_army {
            let ids: Vec<_> = view.selected.iter().copied().collect();
            notice(view, w.assign_army(tag, id, &ids), "Дивизии переведены");
        }
    }
    y += 36.0;
    let Some(id) = view.selected_army else {
        ui::paragraph(
            "Выделите дивизии на карте и создайте армию.",
            Rect::new(x, y, width, 70.0),
            14.0,
            ui::TEXT,
        );
        return;
    };
    let Some(army) = w.command.armies.get(&id).cloned() else {
        return;
    };
    let index = armies.iter().position(|a| *a == id).unwrap_or(0);
    if ui::small_button("◀", Rect::new(x, y, 26.0, 30.0), false) {
        view.selected_army = Some(armies[(index + armies.len() - 1) % armies.len()]);
        view.edit_army_name = false;
    }
    if ui::small_button("▶", Rect::new(x + width - 26.0, y, 26.0, 30.0), false) {
        view.selected_army = Some(armies[(index + 1) % armies.len()]);
        view.edit_army_name = false;
    }
    if ui::small_button(
        &army.name,
        Rect::new(x + 31.0, y, width - 62.0, 30.0),
        view.edit_army_name,
    ) {
        view.army_name = army.name.clone();
        view.name_target = 0;
        view.edit_army_name = true;
        macroquad::miniquad::window::show_keyboard(true);
    }
    if view.edit_army_name {
        while let Some(ch) = get_char_pressed() {
            if !ch.is_control() && view.army_name.chars().count() < 64 {
                view.army_name.push(ch);
            }
        }
        if is_key_pressed(KeyCode::Backspace) {
            view.army_name.pop();
        }
        if is_key_pressed(KeyCode::Enter) && !view.army_name.trim().is_empty() {
            let name = view.army_name.trim().to_owned();
            match view.name_target {
                1 => {
                    if let Some(g) = army.group.and_then(|id| w.command.groups.get_mut(&id)) {
                        g.name = name;
                    }
                }
                2 => {
                    if let Some(t) = w.command.theaters.get_mut(&army.theater) {
                        t.name = name;
                    }
                }
                _ => w.command.armies.get_mut(&id).unwrap().name = name,
            }
            w.command.revision += 1;
            view.edit_army_name = false;
            macroquad::miniquad::window::show_keyboard(false);
        }
        ui::label(
            &view.army_name,
            Rect::new(x + 35.0, y + 4.0, width - 70.0, 23.0),
            13.0,
            ui::GOLD,
        );
    }
    y += 38.0;
    let members: Vec<_> = w
        .command
        .members(id)
        .filter_map(|d| w.divisions.get(&d))
        .filter(|d| !d.destroyed)
        .collect();
    let count = members.len();
    let avg_org = members
        .iter()
        .map(|d| d.organization / d.derived.stats.organization.max(1.0))
        .sum::<f64>()
        / count.max(1) as f64;
    let avg_strength = members.iter().map(|d| d.strength()).sum::<f64>() / count.max(1) as f64;
    let cap = army
        .general
        .as_ref()
        .and_then(|c| w.command.commanders.get(c))
        .and_then(|c| w.command.cap(c, false))
        .map_or("?".into(), |v| {
            if v == 0 { "∞".into() } else { v.to_string() }
        });
    ui::label(
        &format!(
            "Дивизии: {count}/{cap} · Организация {:.0}% · Состав {:.0}%",
            avg_org * 100.0,
            avg_strength * 100.0
        ),
        Rect::new(x, y, width, 22.0),
        12.0,
        ui::GOLD,
    );
    y += 29.0;
    if ui::small_button("Выбрать всю армию", Rect::new(x, y, half, 28.0), false) {
        view.selected = w.command.members(id).collect();
    }
    if ui::small_button(
        "Выбрать по шаблону",
        Rect::new(x + half + 6.0, y, half, 28.0),
        false,
    ) {
        if let Some(template) = view
            .selected
            .iter()
            .find_map(|d| w.divisions.get(d))
            .map(|d| d.template.clone())
        {
            view.selected = w
                .divisions
                .values()
                .filter(|d| d.country == tag && !d.destroyed && d.template == template)
                .map(|d| d.id)
                .collect();
        }
    }
    y += 36.0;
    let general = army
        .general
        .as_ref()
        .and_then(|c| w.command.commanders.get(c))
        .map_or("Назначить генерала", |c| c.name.as_str());
    if ui::small_button(general, Rect::new(x, y, width, 31.0), false) {
        view.commander_picker = Some(false);
        view.command_scroll = 0;
    }
    y += 39.0;
    let group = army.group.and_then(|id| w.command.groups.get(&id)).cloned();
    if let Some(group) = group {
        if ui::small_button(
            &group.name,
            Rect::new(x, y, width, 24.0),
            view.edit_army_name && view.name_target == 1,
        ) {
            view.army_name = group.name.clone();
            view.name_target = 1;
            view.edit_army_name = true;
            macroquad::miniquad::window::show_keyboard(true);
        }
        y += 26.0;
        let marshal = group
            .field_marshal
            .as_ref()
            .and_then(|c| w.command.commanders.get(c))
            .map_or("Назначить фельдмаршала", |c| {
                c.name.as_str()
            });
        if ui::small_button(marshal, Rect::new(x, y, width, 29.0), false) {
            view.commander_picker = Some(true);
            view.command_scroll = 0;
        }
        y += 36.0;
        if ui::small_button("Выйти из группы", Rect::new(x, y, half, 27.0), false) {
            notice(
                view,
                w.command.set_group(tag, id, None),
                "Армия выведена из группы",
            );
        }
        if ui::small_button(
            "Удалить группу",
            Rect::new(x + half + 6.0, y, half, 27.0),
            false,
        ) {
            notice(
                view,
                w.command.delete_group(tag, group.id),
                "Группа удалена",
            );
        }
    } else {
        if ui::small_button("Создать группу армий", Rect::new(x, y, width, 29.0), false)
        {
            notice(
                view,
                w.command.create_group(tag, id, "").map(|_| ()),
                "Группа армий создана",
            );
        }
        y += 36.0;
        let groups: Vec<_> = w
            .command
            .groups
            .values()
            .filter(|g| g.country == tag)
            .map(|g| (g.id, g.name.clone()))
            .collect();
        if !groups.is_empty() {
            view.group_choice %= groups.len();
            let (group, name) = &groups[view.group_choice];
            if ui::small_button("Следующая группа", Rect::new(x, y, width, 26.0), false)
            {
                view.group_choice = (view.group_choice + 1) % groups.len();
            }
            y += 32.0;
            if ui::small_button(
                &format!("В группу: {name}"),
                Rect::new(x, y, width, 29.0),
                false,
            ) {
                notice(
                    view,
                    w.command.set_group(tag, id, Some(*group)),
                    "Армия включена в группу",
                );
            }
        }
    }
    y += 36.0;
    {
        if ui::small_button("Цвет армии", Rect::new(x, y, half, 28.0), false) {
            let c = &mut w.command.armies.get_mut(&id).unwrap().color;
            *c = [c[1], c[2], c[0]];
            w.command.revision += 1;
        }
        if ui::small_button(
            "Удалить армию",
            Rect::new(x + half + 6.0, y, half, 28.0),
            false,
        ) {
            notice(
                view,
                w.command.delete_army(tag, id),
                "Армия удалена; дивизии сохранены",
            );
            view.selected_army = None;
        }
    }
    y += 36.0;
    let theater_name = w
        .command
        .theaters
        .get(&army.theater)
        .map(|t| t.name.clone())
        .unwrap_or_default();
    if ui::small_button(
        &format!("Театр: {theater_name}"),
        Rect::new(x, y, width, 28.0),
        false,
    ) {
        view.army_name = theater_name;
        view.name_target = 2;
        view.edit_army_name = true;
        macroquad::miniquad::window::show_keyboard(true);
    }
    y += 34.0;
    if ui::small_button("Новый театр", Rect::new(x, y, half, 28.0), false) {
        let t = w.command.create_theater(tag, "Новый театр");
        notice(view, w.command.set_theater(tag, id, t), "Театр создан");
    }
    if ui::small_button(
        "Сменить театр",
        Rect::new(x + half + 6.0, y, half, 28.0),
        false,
    ) {
        let ts: Vec<_> = w
            .command
            .theaters
            .values()
            .filter(|t| t.country == tag)
            .map(|t| t.id)
            .collect();
        if let Some(at) = ts.iter().position(|t| *t == army.theater) {
            notice(
                view,
                w.command.set_theater(tag, id, ts[(at + 1) % ts.len()]),
                "Театр изменён",
            );
        }
    }
    y += 37.0;
    let power = w.countries.get(tag).and_then(|c| c.command_power);
    ui::label(
        &format!(
            "Command Power: {} / {}",
            power.map_or_else(|| "нет данных".into(), |p| format!("{p:.1}")),
            w.command_power_max(tag)
                .map_or_else(|| "?".into(), |p| format!("{p:.1}"))
        ),
        Rect::new(x, y, width, 22.0),
        12.0,
        ui::GOLD,
    );
    y += 28.0;
    if ui::small_button(
        if view.ability_marshal {
            "Способности фельдмаршала"
        } else {
            "Способности генерала"
        },
        Rect::new(x, y, width, 28.0),
        view.ability_marshal,
    ) {
        view.ability_marshal = !view.ability_marshal;
    }
    y += 34.0;
    let definitions: Vec<_> = w
        .abilities
        .definitions
        .values()
        .map(|d| (d.id.clone(), d.name.clone()))
        .collect();
    if definitions.is_empty() {
        ui::paragraph(
            "Определения способностей не импортированы.",
            Rect::new(x, y, width, 35.0),
            12.0,
            ui::MUTED,
        );
        y += 40.0;
    }
    for (ability, name) in definitions {
        let cost = w.ability_cost(tag, id, view.ability_marshal, &ability);
        let title = format!(
            "{} · {}",
            w.catalogue.label(&name),
            cost.as_ref()
                .map_or_else(|_| "недоступно".into(), |c| format!("{c:.1} CP"))
        );
        let row = Rect::new(x, y, width, 28.0);
        if ui::small_button(&title, row, false) {
            notice(
                view,
                w.activate_ability(tag, id, view.ability_marshal, &ability)
                    .map(|_| ()),
                "Способность активирована",
            );
        }
        ui::tooltip(row,&name,&cost.map_or_else(|e|e,|c|format!("Цена {c:.2} CP, только назначенным дивизиям; длительность и cooldown из локальных данных")));
        y += 33.0;
    }
    y += 7.0;
    ui::section("ARMY HQ", x, y, width);
    y += 29.0;
    if w.abilities.hq_rules.is_none() {
        ui::paragraph(
            "Подтверждённые правила HQ 1.19.3 отсутствуют в экспорте.",
            Rect::new(x, y, width, 40.0),
            12.0,
            ui::MUTED,
        );
    } else {
        if ui::small_button(
            "Выделенную дивизию назначить штабом",
            Rect::new(x, y, width, 28.0),
            false,
        ) {
            if let Some(&d) = view.selected.first() {
                notice(
                    view,
                    w.assign_hq(tag, id, d, view.selected_plan),
                    "Штаб назначен приказу",
                );
            }
        }
        y += 34.0;
        let h = w
            .abilities
            .headquarters
            .values()
            .find(|h| h.army == id)
            .cloned();
        if ui::small_button(
            "Развернуть штаб из своего шаблона",
            Rect::new(x, y, width, 28.0),
            false,
        ) {
            let template = w
                .countries
                .get(tag)
                .and_then(|c| {
                    c.templates.values().find(|t| {
                        !t.line.is_empty()
                            && t.line.iter().all(|b| {
                                w.abilities
                                    .hq_rules
                                    .as_ref()
                                    .unwrap()
                                    .unit_ids
                                    .contains(&b.unit)
                            })
                    })
                })
                .map(|t| t.name.clone());
            let result = template
                .ok_or_else(|| "Нет импортированного штабного шаблона".to_owned())
                .and_then(|t| {
                    province
                        .ok_or_else(|| "Выберите свою провинцию".into())
                        .and_then(|p| w.deploy_hq(tag, id, &t, p, world, production))
                });
            notice(
                view,
                result.map(|_| ()),
                "Штаб развёрнут; пополнение и подготовка используют общие правила",
            );
        }
        y += 34.0;
        if let Some(h) = h {
            if ui::small_button(
                if h.automatic {
                    "HQ: авто · переключить"
                } else {
                    "HQ: ручной · переключить"
                },
                Rect::new(x, y, width, 28.0),
                h.automatic,
            ) {
                w.abilities
                    .headquarters
                    .get_mut(&h.division)
                    .unwrap()
                    .automatic = !h.automatic;
                if !h.automatic {
                    w.fronts.manual.remove(&h.division);
                }
            }
            y += 34.0;
            ui::label(
                &format!(
                    "Расстояние до фронта: {} км",
                    h.distance_km
                        .map_or_else(|| "?".into(), |d| format!("{d:.1}"))
                ),
                Rect::new(x, y, width, 22.0),
                12.0,
                ui::TEXT,
            );
        }
    }
}
fn assign(tag: &str, w: &mut Warfare, view: &mut ArmyView, id: Option<&str>, marshal: bool) {
    if let Some(army) = view.selected_army {
        let r = if marshal {
            w.command.armies[&army]
                .group
                .ok_or_else(|| "Сначала создайте группу армий".into())
                .and_then(|g| w.command.set_marshal(tag, g, id))
        } else {
            w.command.set_general(tag, army, id)
        };
        if r.is_ok() {
            view.commander_picker = None;
        }
        notice(view, r, "Командующий назначен");
    }
}
fn notice(view: &mut ArmyView, r: Result<(), String>, ok: &str) {
    view.notice = r.map_or_else(|e| e, |_| ok.into());
}

pub fn selected_army(view: &ArmyView, w: &Warfare) -> Option<ArmyId> {
    view.selected_army.or_else(|| {
        view.selected
            .iter()
            .find_map(|id| w.command.membership.get(id).copied())
    })
}
