//! Army controls and map counters. All orders go through the campaign model.
use crate::military_model::{BattalionSlot, DivisionOrder, DivisionTemplate, SupportSlot};
use crate::production::{ProductionRules, ProductionState};
use crate::warfare::Warfare;
use crate::world::World;
use crate::{draw_ui_text, hoi_ui as ui};
use macroquad::prelude::*;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Divisions,
    Templates,
    Equipment,
    Battles,
    Data,
}
#[derive(Debug, Default)]
pub struct ArmyView {
    pub open: bool,
    pub selected: BTreeSet<u64>,
    pub map_drag: Option<crate::army_controls::Drag>,
    pub pan_last: Option<Vec2>,
    pub right_down: Option<Vec2>,
    pub strategic: bool,
    page: Page,
    scroll: usize,
    selection_scroll: usize,
    template: usize,
    unit: usize,
    slot: (usize, usize),
    support: bool,
    regimental: bool,
    draft: Option<DivisionTemplate>,
    editing: Option<String>,
    pub notice: String,
    model: usize,
    battle: usize,
    trace: usize,
}
impl ArmyView {
    pub fn show_template(&mut self, index: usize) {
        self.open = true;
        self.page = Page::Templates;
        self.template = index;
        self.draft = None;
    }
    pub fn close(&mut self) {
        self.open = false;
        self.map_drag = None;
        self.pan_last = None;
        self.right_down = None;
    }
}
pub fn bounds(_view: &ArmyView) -> Rect {
    crate::ui_layout::Layout::current().primary()
}
pub fn is_over_panel(point: Vec2, view: &ArmyView) -> bool {
    interactive_bounds(view).is_some_and(|r| r.contains(point))
}
fn label(text: &str, x: f32, y: f32, width: f32, size: f32, color: Color) {
    let mut s = text.to_owned();
    while !s.is_empty() && crate::measure_ui_text(&s, size as u16).width > width {
        s.pop();
    }
    draw_ui_text(&s, x, y, size, color);
}
fn order(d: &DivisionOrder) -> &'static str {
    match d {
        DivisionOrder::Hold => "ОБОРОНА",
        DivisionOrder::Move { .. } => "ДВИЖЕНИЕ",
        DivisionOrder::Attack { .. } => "АТАКА",
        DivisionOrder::Retreat { .. } => "ОТСТУПЛЕНИЕ",
        DivisionOrder::StrategicRedeployment { .. } => "ПЕРЕБРОСКА",
        DivisionOrder::Train { .. } => "УЧЕНИЯ",
    }
}
fn result<T>(view: &mut ArmyView, value: Result<T, String>, ok: &str) {
    view.notice = match value {
        Ok(_) => ok.into(),
        Err(e) => e,
    };
}

pub fn interactive_bounds(view: &ArmyView) -> Option<Rect> {
    if view.open {
        Some(bounds(view))
    } else if !view.selected.is_empty() {
        Some(Rect::new(
            0.0,
            crate::ui_layout::Layout::current().top,
            (screen_width() - 10.0).min(310.0).max(1.0),
            (screen_height() - crate::ui_layout::Layout::current().top - 52.0).max(120.0),
        ))
    } else {
        None
    }
}
pub fn issue_order(
    view: &mut ArmyView,
    w: &mut Warfare,
    world: &World,
    tag: &str,
    province: u32,
    append: bool,
) {
    let report = crate::army_controls::order_selection(
        w,
        world,
        tag,
        &view.selected,
        province,
        view.strategic,
        append,
    );
    view.notice = report.message();
}
pub fn draw_selection(world: &World, tag: Option<&str>, w: &mut Warfare, view: &mut ArmyView) {
    if view.open || view.selected.is_empty() {
        return;
    }
    let Some(tag) = tag else {
        return;
    };
    view.selected.retain(|id| {
        w.divisions
            .get(id)
            .is_some_and(|d| d.country == tag && !d.destroyed)
    });
    let Some(a) = interactive_bounds(view) else {
        return;
    };
    ui::panel(a.x, a.y, a.w, a.h);
    ui::header(a.x + 3.0, a.y + 3.0, a.w - 6.0, "ВЫБОР ВОЙСК");
    if ui::small_button(
        "×",
        Rect::new(a.right() - 33.0, a.y + 7.0, 25.0, 24.0),
        false,
    ) {
        view.selected.clear();
        return;
    }
    let x = a.x + 10.0;
    let width = a.w - 20.0;
    let total = w
        .divisions
        .values()
        .filter(|d| d.country == tag && !d.destroyed)
        .count();
    label(
        &format!("Выбрано: {} / {total}", view.selected.len()),
        x,
        a.y + 54.0,
        width,
        13.0,
        ui::GOLD,
    );
    let portrait = Rect::new(x, a.y + 66.0, 61.0, 72.0);
    ui::row_bg(portrait, false, false);
    crate::ui_icons::draw(
        crate::ui_icons::Icon::Officer,
        Rect::new(x + 8.0, a.y + 73.0, 45.0, 54.0),
        ui::MUTED,
    );
    ui::placeholder(
        Rect::new(x + 72.0, a.y + 69.0, width - 72.0, 29.0),
        "Назначить командира",
    );
    label(
        "ПКМ: движение / атака",
        x + 72.0,
        a.y + 115.0,
        width - 72.0,
        11.0,
        ui::TEXT,
    );
    label(
        "Shift + ПКМ: маршрут",
        x + 72.0,
        a.y + 133.0,
        width - 72.0,
        11.0,
        ui::MUTED,
    );
    let list = Rect::new(x, a.y + 151.0, width, (a.h - 252.0).max(45.0));
    let ids: Vec<_> = view.selected.iter().copied().collect();
    let rows = (list.h / 48.0).max(1.0) as usize;
    let max = ids.len().saturating_sub(rows);
    view.selection_scroll = view.selection_scroll.min(max);
    if ui::inside(list) {
        let (_, dy) = mouse_wheel();
        if dy > 0.0 {
            view.selection_scroll = view.selection_scroll.saturating_sub(2);
        } else if dy < 0.0 {
            view.selection_scroll = (view.selection_scroll + 2).min(max);
        }
    }
    ui::scrollbar(list, &mut view.selection_scroll, max, rows);
    for (i, id) in ids
        .iter()
        .skip(view.selection_scroll)
        .take(rows)
        .enumerate()
    {
        let d = &w.divisions[id];
        let r = Rect::new(x, list.y + i as f32 * 48.0, width, 44.0);
        ui::row_bg(r, false, i % 2 == 1);
        crate::ui_icons::draw(
            crate::ui_icons::Icon::Army,
            Rect::new(r.x + 5.0, r.y + 10.0, 22.0, 22.0),
            ui::GOLD,
        );
        label(&d.name, r.x + 36.0, r.y + 16.0, r.w - 44.0, 12.0, ui::TEXT);
        label(
            order(&d.order),
            r.x + 36.0,
            r.y + 30.0,
            r.w - 44.0,
            9.0,
            ui::MUTED,
        );
        ui::progress_bar(
            Rect::new(r.x + 36.0, r.bottom() - 7.0, (r.w - 48.0) * 0.48, 4.0),
            d.strength() as f32,
        );
        ui::progress_bar(
            Rect::new(
                r.x + 36.0 + (r.w - 48.0) * 0.53,
                r.bottom() - 7.0,
                (r.w - 48.0) * 0.47,
                4.0,
            ),
            (d.organization / d.derived.stats.organization.max(1.0)) as f32,
        );
        ui::tooltip(
            r,
            &d.name,
            &format!(
                "Провинция {} · Организация {:.1} · Сила {:.0}%",
                d.province,
                d.organization,
                d.strength() * 100.0
            ),
        );
        if ui::inside(r) && is_mouse_button_pressed(MouseButton::Left) {
            view.selected.clear();
            view.selected.insert(*id);
        }
    }
    label(&view.notice, x, a.bottom() - 89.0, width, 10.0, ui::GOLD);
    if ui::small_button(
        "Стоп · H",
        Rect::new(x, a.bottom() - 76.0, 95.0, 29.0),
        false,
    ) {
        view.notice = crate::army_controls::stop_selection(w, world, tag, &view.selected).message();
    }
    if ui::small_button(
        "Переброска",
        Rect::new(x + 101.0, a.bottom() - 76.0, width - 101.0, 29.0),
        view.strategic,
    ) {
        view.strategic = !view.strategic;
    }
    if ui::small_button(
        "Управление дивизиями",
        Rect::new(x, a.bottom() - 39.0, width, 29.0),
        false,
    ) {
        view.open = true;
        view.page = Page::Divisions;
    }
}

pub fn draw(
    world: &World,
    tag: Option<&str>,
    w: &mut Warfare,
    production: &mut ProductionState,
    production_rules: &ProductionRules,
    selected_province: Option<u32>,
    view: &mut ArmyView,
) {
    if !view.open {
        return;
    }
    let a = bounds(view);
    ui::panel(a.x, a.y, a.w, a.h);
    ui::header(a.x + 4.0, a.y + 4.0, a.w - 8.0, "СУХОПУТНЫЕ ВОЙСКА");
    if ui::small_button(
        "X",
        Rect::new(a.x + a.w - 36.0, a.y + 7.0, 26.0, 24.0),
        false,
    ) {
        view.open = false;
        view.map_drag = None;
        view.right_down = None;
        return;
    }
    let Some(tag) = tag else {
        return;
    };
    view.selected.retain(|id| {
        w.divisions
            .get(id)
            .is_some_and(|d| d.country == tag && !d.destroyed)
    });
    let x = a.x + 10.0;
    let width = a.w - 20.0;
    let mut y = a.y + 51.0;
    let tabw = width / 5.0;
    for (i, (p, title)) in [
        (Page::Divisions, "ДИВИЗИИ"),
        (Page::Templates, "ШАБЛОНЫ"),
        (Page::Equipment, "ТЕХНИКА"),
        (Page::Battles, "БОИ"),
        (Page::Data, "ДАННЫЕ"),
    ]
    .into_iter()
    .enumerate()
    {
        if ui::small_button(
            title,
            Rect::new(x + i as f32 * tabw, y - 15.0, tabw - 3.0, 29.0),
            view.page == p,
        ) {
            view.page = p;
            view.scroll = 0;
        }
    }
    y += 37.0;
    let (men, xp) = w
        .countries
        .get(tag)
        .map(|c| (c.manpower_pool, c.army_xp))
        .unwrap_or_default();
    label(
        &format!(
            "Резерв {men} · Опыт {xp:.1} · Выбрано {}",
            view.selected.len()
        ),
        x,
        y,
        width,
        12.0,
        ui::GOLD,
    );
    y += 25.0;
    let bottom = a.y + a.h - 47.0;
    match view.page {
        Page::Divisions => {
            let count = w
                .divisions
                .values()
                .filter(|d| d.country == tag && !d.destroyed)
                .count();
            let expected = w.countries.get(tag).map_or(0, |a| a.history_expected);
            label(
                &if count == 0
                    && w.countries
                        .get(tag)
                        .is_some_and(|a| !a.history_errors.is_empty())
                {
                    "Ошибка загрузки армии · см. вкладку «Данные»".into()
                } else {
                    format!("Дивизии {count} / стартовый состав {expected} · ПКМ: приказ")
                },
                x,
                y + 12.0,
                width,
                11.0,
                ui::TEXT,
            );
            y += 22.0;
            let bw = (width - 9.0) / 4.0;
            if ui::small_button("СНЯТЬ", Rect::new(x, y, bw, 28.0), false) {
                view.selected.clear();
            }
            if ui::small_button("СТОП", Rect::new(x + bw + 3.0, y, bw, 28.0), false) {
                let ids: Vec<_> = view.selected.iter().copied().collect();
                for id in ids {
                    let r = w.stop(tag, id, world);
                    result(view, r, "Приказы остановлены");
                }
            }
            if ui::small_button(
                "ПЕРЕБРОСКА",
                Rect::new(x + (bw + 3.0) * 2.0, y, bw, 28.0),
                view.strategic,
            ) {
                view.strategic = !view.strategic;
            }
            if ui::small_button(
                "РАСФОРМ.",
                Rect::new(x + (bw + 3.0) * 3.0, y, bw, 28.0),
                false,
            ) {
                let ids: Vec<_> = view.selected.iter().copied().collect();
                for id in ids {
                    let r = w.disband(tag, id, production);
                    result(view, r, "Дивизия расформирована");
                }
            }
            y += 36.0;
            if let Some(province) = selected_province {
                let controller = world.controller_of_province(province).unwrap_or("---");
                label(
                    &format!("Провинция {province} · Контроль {controller}"),
                    x,
                    y + 16.0,
                    (width - 119.0).max(1.0),
                    12.0,
                    ui::TEXT,
                );
                if controller != tag
                    && !w.at_war(tag, controller)
                    && ui::small_button(
                        "ВОЙНА",
                        Rect::new(x + width - 116.0, y, 116.0, 25.0),
                        false,
                    )
                {
                    let r = w.declare_war(tag, controller, world);
                    result(view, r, "Война объявлена");
                }
                y += 33.0;
            }
            let ids: Vec<_> = w
                .divisions
                .values()
                .filter(|d| d.country == tag && !d.destroyed)
                .map(|d| d.id)
                .collect();
            let rows = ((bottom - y - 29.0) / 55.0).max(0.0) as usize;
            view.scroll = view.scroll.min(ids.len().saturating_sub(rows.max(1)));
            if ids.is_empty() {
                label(
                    "Дивизии не загружены. Подробности во вкладке «ДАННЫЕ».",
                    x,
                    y + 18.0,
                    width,
                    12.0,
                    ui::MUTED,
                );
            }
            for (id, row) in ids.iter().skip(view.scroll).take(rows).zip(0..) {
                let d = &w.divisions[id];
                let r = Rect::new(x, y + row as f32 * 55.0, width, 51.0);
                let active = view.selected.contains(id);
                ui::row_bg(r, active, row % 2 == 0);
                label(
                    &format!("{}  #{}  {}  P{}", d.name, id, order(&d.order), d.province),
                    r.x + 5.0,
                    r.y + 15.0,
                    r.w - 10.0,
                    12.0,
                    ui::TEXT,
                );
                ui::progress_bar(
                    Rect::new(r.x + 5.0, r.y + 21.0, (r.w - 15.0) / 2.0, 7.0),
                    d.strength() as f32,
                );
                ui::progress_bar(
                    Rect::new(
                        r.x + 10.0 + (r.w - 15.0) / 2.0,
                        r.y + 21.0,
                        (r.w - 15.0) / 2.0,
                        7.0,
                    ),
                    (d.organization / d.derived.stats.organization.max(1.0)) as f32,
                );
                label(
                    &format!(
                        "HP {:.1}/{:.1}  ORG {:.1}  supply {:.0}%  delay {}h",
                        d.hp,
                        d.derived.stats.hp,
                        d.organization,
                        d.supply_fraction * 100.0,
                        d.attack_delay
                    ),
                    r.x + 5.0,
                    r.y + 44.0,
                    r.w - 10.0,
                    10.0,
                    ui::MUTED,
                );
                if ui::inside(r) && is_mouse_button_pressed(MouseButton::Left) {
                    if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) {
                        if !view.selected.insert(*id) {
                            view.selected.remove(id);
                        }
                    } else {
                        view.selected.clear();
                        view.selected.insert(*id);
                    }
                }
            }
            let py = bottom - 22.0;
            if ui::small_button("НАЗАД", Rect::new(x, py, 65.0, 24.0), false) {
                view.scroll = view.scroll.saturating_sub(rows.max(1));
            }
            if ui::small_button("ДАЛЕЕ", Rect::new(x + 70.0, py, 65.0, 24.0), false) {
                view.scroll =
                    (view.scroll + rows.max(1)).min(ids.len().saturating_sub(rows.max(1)));
            }
        }
        Page::Equipment => {
            let models: Vec<_> = w
                .catalogue
                .equipment
                .keys()
                .filter(|id| {
                    w.countries
                        .get(tag)
                        .is_some_and(|c| w.catalogue.equipment_available(id, &c.modifiers))
                })
                .cloned()
                .collect();
            if models.is_empty() {
                label(
                    "Доступные модели снаряжения не загружены.",
                    x,
                    y,
                    width,
                    13.0,
                    ui::MUTED,
                );
            } else {
                view.model %= models.len();
                let id = models[view.model].clone();
                if ui::small_button("<", Rect::new(x, y, 30.0, 28.0), false) {
                    view.model = (view.model + models.len() - 1) % models.len();
                }
                label(&id, x + 37.0, y + 19.0, width - 76.0, 13.0, ui::TEXT);
                if ui::small_button(">", Rect::new(x + width - 30.0, y, 30.0, 28.0), false) {
                    view.model = (view.model + 1) % models.len();
                }
                y += 45.0;
                let e = &w.catalogue.equipment[&id];
                let count = production
                    .country(tag)
                    .and_then(|p| p.model_stockpile.get(&id))
                    .copied()
                    .unwrap_or(0);
                label(
                    &format!("Склад {count} · Стоимость {:.2} ПМ", e.cost),
                    x,
                    y,
                    width,
                    13.0,
                    ui::GOLD,
                );
                y += 14.0;
                if ui::small_button(
                    "СОЗДАТЬ ЛИНИЮ ВЫПУСКА",
                    Rect::new(x, y, 190.0f32.min(width), 30.0),
                    false,
                ) {
                    let r = w.add_production(tag, &id, production, production_rules);
                    result(
                        view,
                        r,
                        "Линия создана. Назначьте заводы в разделе «Производство».",
                    );
                }
                y += 57.0;
            }
            label(
                "Пополнения получают снаряжение с этого склада ежедневно.",
                x,
                y,
                width,
                12.0,
                ui::MUTED,
            );
        }
        Page::Templates => draw_templates(
            world,
            tag,
            w,
            production,
            selected_province,
            view,
            x,
            y,
            width,
            bottom,
        ),
        Page::Battles => {
            let battles: Vec<_> = w.battles.keys().copied().collect();
            if battles.is_empty() {
                label(
                    "Боёв ещё нет. Объявите войну и прикажите атаковать.",
                    x,
                    y,
                    width,
                    12.0,
                    ui::MUTED,
                );
            } else {
                view.battle %= battles.len();
                if ui::small_button("<", Rect::new(x, y, 30.0, 26.0), false) {
                    view.battle = (view.battle + battles.len() - 1) % battles.len();
                    view.trace = 0;
                }
                if ui::small_button(">", Rect::new(x + width - 30.0, y, 30.0, 26.0), false) {
                    view.battle = (view.battle + 1) % battles.len();
                    view.trace = 0;
                }
                let b = w.battles.get_mut(&battles[view.battle]).unwrap();
                b.trace_enabled = true;
                label(
                    &format!(
                        "Бой {} · Провинция {} · {} ч",
                        b.id, b.province, b.elapsed_hours
                    ),
                    x + 36.0,
                    y + 18.0,
                    width - 75.0,
                    13.0,
                    ui::GOLD,
                );
                y += 46.0;
                label(
                    &format!(
                        "Атака: {} в бою + {} в резерве",
                        b.attackers.active.len(),
                        b.attackers.reserves.len()
                    ),
                    x,
                    y,
                    width,
                    12.0,
                    ui::TEXT,
                );
                y += 21.0;
                label(
                    &format!(
                        "Оборона: {} в бою + {} в резерве",
                        b.defenders.active.len(),
                        b.defenders.reserves.len()
                    ),
                    x,
                    y,
                    width,
                    12.0,
                    ui::TEXT,
                );
                y += 21.0;
                label(
                    &format!("Result {:?} / phase {:?}", b.outcome, b.phase),
                    x,
                    y,
                    width,
                    12.0,
                    ui::TEXT,
                );
                y += 21.0;
                if let Some(t) = b.history.back() {
                    label(
                        &format!(
                            "Width {:.0}: A {:.0}, D {:.0}",
                            t.width, t.attacker_width, t.defender_width
                        ),
                        x,
                        y,
                        width,
                        12.0,
                        ui::TEXT,
                    );
                    y += 21.0;
                    label(
                        &format!("Тактики: {} / {}", t.attacker_tactic, t.defender_tactic),
                        x,
                        y,
                        width,
                        11.0,
                        ui::TEXT,
                    );
                    y += 21.0;
                    label(
                        &format!(
                            "Потери за час: {} чел., {} ед. техники",
                            t.manpower_losses, t.equipment_losses
                        ),
                        x,
                        y,
                        width,
                        12.0,
                        ui::GOLD,
                    );
                    y += 30.0;
                    if !t.attacks.is_empty() {
                        view.trace %= t.attacks.len();
                        if ui::small_button("НАЗАД", Rect::new(x, y, 92.0, 23.0), false) {
                            view.trace = (view.trace + t.attacks.len() - 1) % t.attacks.len();
                        }
                        if ui::small_button("ДАЛЕЕ", Rect::new(x + 98.0, y, 92.0, 23.0), false)
                        {
                            view.trace = (view.trace + 1) % t.attacks.len();
                        }
                        y += 40.0;
                        let hit = &t.attacks[view.trace];
                        label(
                            &format!(
                                "{} -> {}: A {:.2}, {} rolls, {} hits",
                                hit.from,
                                hit.to,
                                hit.effective_attack,
                                hit.rolls,
                                hit.protected_hits + hit.overflow_hits
                            ),
                            x,
                            y,
                            width,
                            11.0,
                            ui::TEXT,
                        );
                        y += 18.0;
                        label(
                            &format!(
                                "HP -{:.3}, ORG -{:.3}, armor x{:.2}",
                                hit.hp_damage, hit.org_damage, hit.armor_factor
                            ),
                            x,
                            y,
                            width,
                            11.0,
                            ui::GOLD,
                        );
                        y += 18.0;
                        label(
                            &format!("Shared defense: {} protected rolls", hit.protected),
                            x,
                            y,
                            width,
                            11.0,
                            ui::MUTED,
                        );
                        y += 22.0;
                        for m in hit
                            .modifiers
                            .iter()
                            .take(((bottom - y) / 16.0).max(0.0) as usize)
                        {
                            label(
                                &format!(
                                    "{}: attack x{:.3}, defense x{:.3}",
                                    m.source, m.attack, m.defense
                                ),
                                x,
                                y,
                                width,
                                10.0,
                                ui::MUTED,
                            );
                            y += 16.0;
                        }
                    } else {
                        label(
                            "Detailed rolls are recorded while this battle is inspected.",
                            x,
                            y,
                            width,
                            11.0,
                            ui::MUTED,
                        );
                    }
                }
            }
        }
        Page::Data => {
            label(
                &w.catalogue.rules.target_version,
                x,
                y,
                width,
                12.0,
                ui::GOLD,
            );
            y += 24.0;
            label(
                &format!(
                    "{} battalions, {} models, {} tactics",
                    w.catalogue.units.len(),
                    w.catalogue.equipment.len(),
                    w.catalogue.tactics.len()
                ),
                x,
                y,
                width,
                12.0,
                ui::TEXT,
            );
            y += 24.0;
            let history = w.countries.get(tag);
            label(
                &format!(
                    "{tag}: imported {} / {} starting divisions",
                    history.map_or(0, |a| a.history_imported),
                    history.map_or(0, |a| a.history_expected)
                ),
                x,
                y,
                width,
                12.0,
                ui::GOLD,
            );
            y += 24.0;
            let country_errors = history.map(|a| a.history_errors.as_slice()).unwrap_or(&[]);
            let diagnostics: Vec<_> =
                country_errors
                    .iter()
                    .chain(
                        w.diagnostics
                            .iter()
                            .filter(|s| s.starts_with(&format!("{tag} /"))),
                    )
                    .chain(w.diagnostics.iter().filter(|s| {
                        !s.contains(": no starting land templates") && !s.contains(" / ")
                    }))
                    .collect();
            let rows = ((bottom - y - 30.0) / 35.0).max(0.0) as usize;
            view.scroll = view
                .scroll
                .min(diagnostics.len().saturating_sub(rows.max(1)));
            for diagnostic in diagnostics.iter().skip(view.scroll).take(rows) {
                let chars: Vec<char> = diagnostic.chars().collect();
                let cut = (width / 6.0).max(1.0) as usize;
                label(
                    &chars.iter().take(cut).collect::<String>(),
                    x,
                    y,
                    width,
                    11.0,
                    ui::MUTED,
                );
                label(
                    &chars.iter().skip(cut).collect::<String>(),
                    x,
                    y + 14.0,
                    width,
                    11.0,
                    ui::MUTED,
                );
                y += 35.0;
            }
            if ui::small_button("НАЗАД", Rect::new(x, bottom - 22.0, 65.0, 24.0), false) {
                view.scroll = view.scroll.saturating_sub(rows.max(1));
            }
            if ui::small_button(
                "ДАЛЕЕ",
                Rect::new(x + 70.0, bottom - 22.0, 65.0, 24.0),
                false,
            ) {
                view.scroll =
                    (view.scroll + rows.max(1)).min(diagnostics.len().saturating_sub(rows.max(1)));
            }
        }
    }
    label(&view.notice, x, a.y + a.h - 14.0, width, 11.0, ui::GOLD);
}

fn draw_templates(
    world: &World,
    tag: &str,
    w: &mut Warfare,
    production: &mut ProductionState,
    province: Option<u32>,
    view: &mut ArmyView,
    x: f32,
    mut y: f32,
    width: f32,
    bottom: f32,
) {
    let Some(army) = w.countries.get(tag) else {
        return;
    };
    let names: Vec<_> = army.templates.keys().cloned().collect();
    if names.is_empty() {
        label(
            "Шаблоны дивизий не загружены.",
            x,
            y,
            width,
            12.0,
            ui::MUTED,
        );
        return;
    }
    view.template %= names.len();
    let name = names[view.template].clone();
    if ui::small_button("<", Rect::new(x, y, 28.0, 26.0), false) {
        view.template = (view.template + names.len() - 1) % names.len();
        view.draft = None;
    }
    label(&name, x + 35.0, y + 18.0, width - 74.0, 13.0, ui::TEXT);
    if ui::small_button(">", Rect::new(x + width - 28.0, y, 28.0, 26.0), false) {
        view.template = (view.template + 1) % names.len();
        view.draft = None;
    }
    y += 34.0;
    if view.draft.is_none() || view.editing.as_deref() != Some(name.as_str()) {
        view.draft = Some(army.templates[&name].clone());
        view.editing = Some(name.clone());
    }
    let t = view.draft.as_ref().unwrap().clone();
    let cell = ((width - 12.0) / 6.0).min(43.0);
    let grid_height = cell * 5.0;
    if y + grid_height + 145.0 > bottom {
        label(
            "Увеличьте высоту окна для редактора шаблонов.",
            x,
            y + 20.0,
            width,
            12.0,
            ui::MUTED,
        );
        return;
    }
    for c in 0..5 {
        for r in 0..5 {
            let unit = t
                .line
                .iter()
                .find(|s| s.column == c && s.row == r)
                .map(|s| s.unit.as_str())
                .unwrap_or("+");
            if ui::small_button(
                unit,
                Rect::new(
                    x + c as f32 * cell,
                    y + r as f32 * cell,
                    cell - 2.0,
                    cell - 2.0,
                ),
                !view.support && view.slot == (c, r),
            ) {
                view.slot = (c, r);
                view.support = false;
            }
        }
    }
    for r in 0..5 {
        let unit = t
            .support
            .iter()
            .find(|s| {
                s.regiment
                    == if view.regimental {
                        Some(view.slot.0)
                    } else {
                        None
                    }
                    && s.row == r
            })
            .map(|s| s.unit.as_str())
            .unwrap_or("+");
        if ui::small_button(
            unit,
            Rect::new(
                x + 5.0 * cell + 8.0,
                y + r as f32 * cell,
                cell - 2.0,
                cell - 2.0,
            ),
            view.support && view.slot.1 == r,
        ) {
            view.slot.1 = r;
            view.support = true;
        }
    }
    y += grid_height + 8.0;
    if ui::small_button(
        "ПОЛКОВАЯ ПОДДЕРЖКА",
        Rect::new(x, y, width, 25.0),
        view.regimental,
    ) {
        view.regimental = !view.regimental;
    }
    y += 32.0;
    let options: Vec<_> = w
        .catalogue
        .units
        .values()
        .filter(|u| {
            (u.active || army.modifiers.unlocked_units.contains(&u.id))
                && if view.support {
                    u.group == "support"
                        && if view.regimental {
                            u.regimental
                        } else {
                            u.divisional
                        }
                } else {
                    u.group != "support"
                }
        })
        .map(|u| u.id.clone())
        .collect();
    if !options.is_empty() {
        view.unit %= options.len();
        if ui::small_button("<", Rect::new(x, y, 27.0, 25.0), false) {
            view.unit = (view.unit + options.len() - 1) % options.len();
        }
        label(
            &options[view.unit],
            x + 33.0,
            y + 17.0,
            width - 178.0,
            12.0,
            ui::TEXT,
        );
        if ui::small_button(">", Rect::new(x + width - 140.0, y, 27.0, 25.0), false) {
            view.unit = (view.unit + 1) % options.len();
        }
        let mut changed = t.clone();
        let apply = ui::small_button(
            "ВЫБРАТЬ",
            Rect::new(x + width - 106.0, y, 48.0, 25.0),
            false,
        );
        let remove = ui::small_button("УБРАТЬ", Rect::new(x + width - 53.0, y, 48.0, 25.0), false);
        if apply || remove {
            if view.support {
                let regiment = view.regimental.then_some(view.slot.0);
                changed
                    .support
                    .retain(|s| !(s.regiment == regiment && s.row == view.slot.1));
                if apply {
                    changed.support.push(SupportSlot {
                        unit: options[view.unit].clone(),
                        regiment,
                        row: view.slot.1,
                    });
                }
            } else {
                changed
                    .line
                    .retain(|s| !(s.column == view.slot.0 && s.row == view.slot.1));
                if apply {
                    changed.line.push(BattalionSlot {
                        unit: options[view.unit].clone(),
                        column: view.slot.0,
                        row: view.slot.1,
                    });
                }
            }
            view.draft = Some(changed);
        }
    }
    y += 34.0;
    if let Ok(d) = w
        .catalogue
        .aggregate(view.draft.as_ref().unwrap(), &army.modifiers, None)
    {
        label(
            &format!(
                "SA {:.1}  HA {:.1}  DEF {:.1}  BRK {:.1}  W {:.0}",
                d.stats.soft_attack,
                d.stats.hard_attack,
                d.stats.defense,
                d.stats.breakthrough,
                d.stats.width
            ),
            x,
            y + 13.0,
            width,
            11.0,
            ui::TEXT,
        );
    }
    y += 22.0;
    let cost = w
        .catalogue
        .edit_cost(&army.templates[&name], view.draft.as_ref().unwrap());
    if ui::small_button(
        &format!(
            "СОХРАНИТЬ · {:.0} ОПЫТА",
            cost.as_ref().copied().unwrap_or(0.0)
        ),
        Rect::new(x, y, width / 2.0 - 4.0, 28.0),
        false,
    ) {
        let r = w.save_template(
            tag,
            view.editing.as_deref(),
            view.draft.as_ref().unwrap().clone(),
        );
        result(view, r, "Шаблон сохранён");
    }
    if ui::small_button(
        "РАЗВЕРНУТЬ",
        Rect::new(x + width / 2.0 + 4.0, y, width / 2.0 - 4.0, 28.0),
        false,
    ) {
        if let Some(province) = province {
            let r = w.recruit(tag, &name, province, world, production);
            result(view, r, "Дивизия развёрнута");
        } else {
            view.notice = "Сначала выберите свою провинцию на карте".into();
        }
    }
}

pub fn draw_counters(
    world: &World,
    w: &Warfare,
    tag: Option<&str>,
    view: &ArmyView,
    source: Rect,
    area: Rect,
    pointer_blocked: bool,
) {
    let project = |p| crate::army_controls::project(world, p, source, area);
    for d in w
        .divisions
        .values()
        .filter(|d| !d.destroyed && view.selected.contains(&d.id))
    {
        let mut path = match &d.order {
            DivisionOrder::Move { path, .. }
            | DivisionOrder::StrategicRedeployment { path, .. } => path.clone(),
            DivisionOrder::Attack { province } => vec![*province],
            DivisionOrder::Retreat { destination, .. } => vec![*destination],
            _ => Vec::new(),
        };
        if let Some(o) = d.destination {
            if path.last() != Some(&o.province) {
                path.push(o.province);
            }
        }
        path.extend(d.waypoints.iter().map(|o| o.province));
        let mut from = project(d.province);
        for province in path {
            let to = project(province);
            if let Some((a, b)) = from.zip(to) {
                if area.contains(a) && area.contains(b) {
                    draw_line(a.x, a.y, b.x, b.y, 2.0, ui::GOLD);
                    draw_circle(b.x, b.y, 3.0, ui::GOLD);
                }
            }
            from = to;
        }
    }
    let mouse = vec2(mouse_position().0, mouse_position().1);
    for c in crate::army_controls::counters(world, w, source, area) {
        let selected = c.ids.iter().any(|id| view.selected.contains(id));
        let battle = c.ids.iter().any(|id| w.divisions[id].battle.is_some());
        let color = if selected {
            ui::GOLD
        } else if Some(c.country.as_str()) == tag {
            Color::from_rgba(89, 155, 75, 255)
        } else if tag.is_some_and(|t| w.at_war(t, &c.country)) {
            Color::from_rgba(190, 66, 58, 255)
        } else {
            Color::from_rgba(110, 130, 165, 255)
        };
        let r = c.rect;
        draw_rectangle(r.x, r.y, r.w, r.h, ui::BASE);
        draw_rectangle_lines(r.x, r.y, r.w, r.h, if selected { 2.0 } else { 1.0 }, color);
        draw_line(r.x + 5.0, r.y + 6.0, r.x + 15.0, r.y + 15.0, 1.3, color);
        draw_line(r.x + 15.0, r.y + 6.0, r.x + 5.0, r.y + 15.0, 1.3, color);
        draw_ui_text(
            &c.ids.len().to_string(),
            r.x + 23.0,
            r.y + 16.0,
            13.0,
            color,
        );
        let hp = c
            .ids
            .iter()
            .map(|id| w.divisions[id].strength())
            .sum::<f64>()
            / c.ids.len() as f64;
        let org = c
            .ids
            .iter()
            .map(|id| {
                let d = &w.divisions[id];
                d.organization / d.derived.stats.organization.max(1.0)
            })
            .sum::<f64>()
            / c.ids.len() as f64;
        draw_rectangle(r.x + 2.0, r.y + 20.0, (r.w - 4.0) * hp as f32, 2.0, GREEN);
        draw_rectangle(
            r.x + 2.0,
            r.y + 23.0,
            (r.w - 4.0) * org as f32,
            2.0,
            SKYBLUE,
        );
        if battle {
            draw_circle(r.right(), r.y, 4.0, RED);
        }
        if r.contains(mouse) && !pointer_blocked {
            let selected_id = c
                .ids
                .iter()
                .find(|id| view.selected.contains(id))
                .unwrap_or(&c.ids[0]);
            let title = format!(
                "{} [{}] · Дивизий: {}",
                w.divisions[selected_id].name,
                c.country,
                c.ids.len()
            );
            ui::tooltip(
                r,
                &title,
                "ЛКМ: выбрать дивизию. Ctrl + ЛКМ: выбрать весь стек. ПКМ: движение или атака.",
            );
        }
    }
}
