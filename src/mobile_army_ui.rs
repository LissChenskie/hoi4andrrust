//! Adaptive army inspector reached through the existing Army/Officer controls.
//! Release-based taps, fixed navigation and virtualized lists; no duplicate units.
use crate::{army_projection, army_ui::ArmyView, hoi_ui as ui, warfare::Warfare, world::World};
use macroquad::prelude::*;
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum MobilePage {
    #[default]
    Army,
    Armies,
    Logistics,
    Roster,
    Picker(bool),
    Commander(String),
    Trait(String, String),
}
#[derive(Debug, Default)]
pub struct MobileView {
    pub page: MobilePage,
    pub scroll: f32,
    pub advanced: bool,
    pub air_region: Option<u32>,
    pub air_base: Option<u32>,
    gesture: crate::army_touch::Gesture,
    scroll_drag: bool,
    cached_army: Option<(u64, u64, u64, u64, f64, army_projection::ArmyViewModel)>,
}
fn army_model(
    w: &Warfare,
    tag: &str,
    id: u64,
    m: &mut MobileView,
) -> Option<army_projection::ArmyViewModel> {
    let now = get_time();
    let valid = m.cached_army.as_ref().is_some_and(|(a, h, c, p, t, _)| {
        *a == id
            && *h == w.simulated_hours
            && *c == w.command.revision
            && *p == w.plans.revision
            && now - *t < 0.25
    });
    if !valid {
        m.cached_army = army_projection::army(w, tag, id, "russian").ok().map(|vm| {
            (
                id,
                w.simulated_hours,
                w.command.revision,
                w.plans.revision,
                now,
                vm,
            )
        });
    }
    m.cached_army.as_ref().map(|(_, _, _, _, _, vm)| vm.clone())
}
fn tap_frame(state: &mut MobileView, panel: Rect, body: Rect) -> Option<Vec2> {
    let (x, y) = mouse_position();
    let p = vec2(x, y);
    let pressed = is_mouse_button_pressed(MouseButton::Left) && ui::inside(panel);
    if pressed {
        state.scroll_drag = body.contains(p);
    }
    let contacts = touches()
        .iter()
        .filter(|t| !matches!(t.phase, TouchPhase::Ended | TouchPhase::Cancelled))
        .count();
    let event = state.gesture.update(
        p,
        pressed,
        is_mouse_button_down(MouseButton::Left),
        is_mouse_button_released(MouseButton::Left),
        contacts,
        panel,
    );
    if state.scroll_drag {
        state.scroll = (state.scroll + event.scroll).max(0.);
    }
    if body.contains(p) {
        state.scroll = (state.scroll - mouse_wheel().1 * 42.).max(0.);
    }
    event.tap
}
fn button(text: &str, rect: Rect, active: bool, tap: Option<Vec2>) -> bool {
    ui::row_bg(rect, active, false);
    ui::paragraph(
        text,
        Rect::new(rect.x + 9., rect.y + 9., rect.w - 18., rect.h - 14.),
        if rect.w < 95.0 { 13.0 } else { 15.0 },
        if active { ui::GOLD } else { ui::TEXT },
    );
    tap.is_some_and(|p| rect.contains(p)) && ui::inside(rect)
}
struct Rows {
    body: Rect,
    y: f32,
    tap: Option<Vec2>,
}
impl Rows {
    fn rect(&mut self, h: f32) -> Rect {
        let r = Rect::new(self.body.x, self.y, self.body.w, h);
        self.y += h + 6.;
        r
    }
    fn button(&mut self, text: &str, active: bool) -> bool {
        let r = self.rect(56.);
        if r.bottom() < self.body.y || r.y > self.body.bottom() {
            return false;
        }
        button(text, r, active, self.tap.filter(|p| self.body.contains(*p)))
    }
    fn text(&mut self, text: &str, height: f32, color: Color) {
        let r = self.rect(height);
        if r.bottom() >= self.body.y && r.y <= self.body.bottom() {
            ui::paragraph(
                text,
                Rect::new(r.x + 6., r.y + 4., r.w - 12., r.h - 8.),
                15.,
                color,
            );
        }
    }
    fn ratio(&mut self, title: &str, ratio: Option<f64>) {
        let r = self.rect(48.);
        if r.bottom() < self.body.y || r.y > self.body.bottom() {
            return;
        }
        ui::label(
            &format!(
                "{title}: {}",
                ratio.map_or("—".into(), |v| format!("{:.0}%", v * 100.))
            ),
            Rect::new(r.x + 6., r.y + 3., r.w - 12., 22.),
            14.,
            ui::TEXT,
        );
        ui::progress_bar(
            Rect::new(r.x + 6., r.y + 31., r.w - 12., 8.),
            ratio.unwrap_or(0.).clamp(0., 1.) as f32,
        );
    }
}
fn navigate(m: &mut MobileView, page: MobilePage) {
    m.page = page;
    m.scroll = 0.;
}
fn result(view: &mut ArmyView, r: Result<(), String>, ok: &str) {
    view.notice = r.map_or_else(|e| e, |_| ok.into());
}

pub fn draw(world: &World, tag: &str, w: &mut Warfare, view: &mut ArmyView, panel: Rect) {
    let mut m = std::mem::take(&mut view.mobile);
    if let Some(marshal) = view.commander_picker.take() {
        navigate(&mut m, MobilePage::Picker(marshal));
    }
    let body = Rect::new(
        panel.x + 8.,
        panel.y + 100.,
        panel.w - 16.,
        (panel.h - 162.).max(1.),
    );
    let tap = tap_frame(&mut m, panel, body);
    ui::panel(panel.x, panel.y, panel.w, panel.h);
    ui::label(
        if m.page == MobilePage::Logistics {
            "ТЫЛ И АВИАЦИЯ"
        } else {
            "АРМИЯ И КОМАНДОВАНИЕ"
        },
        Rect::new(panel.x + 12., panel.y + 16., panel.w - 74., 30.),
        17.,
        ui::GOLD,
    );
    if button(
        "×",
        Rect::new(panel.right() - 54., panel.y + 5., 48., 48.),
        false,
        tap,
    ) {
        view.open = false;
    }
    ui::paragraph(
        &view.notice,
        Rect::new(panel.x + 8., panel.y + 55., panel.w - 16., 38.),
        12.,
        ui::GOLD,
    );
    let army = view
        .selected_army
        .filter(|id| w.command.armies.get(id).is_some_and(|a| a.country == tag));
    let page = m.page.clone();
    let mut rows = Rows {
        body,
        y: body.y - m.scroll,
        tap,
    };
    {
        let _clip = ui::clip(body);
        match page.clone() {
            MobilePage::Logistics => {
                use crate::air_warfare::AirMission;
                let c = w.logistics.countries.get(tag).cloned().unwrap_or_default();
                rows.text(
                    &format!(
                        "Топливо {:.0} / {:.0}\nРасход за час {:.1} · конвои {} · грузовики {}",
                        c.fuel,
                        c.fuel_capacity,
                        c.fuel_used_last_hour,
                        c.convoys,
                        c.truck_count()
                    ),
                    78.,
                    ui::TEXT,
                );
                rows.text("Снабжение: столица → железная дорога / порт → хаб → дивизия. При разрыве пути расходуются переносимые запасы.",84.,ui::MUTED);
                if let Some(id) = army {
                    if let Ok(vm) = army_projection::army(w, tag, id, "russian") {
                        rows.text(
                            &format!(
                                "Запас армии: {}\n{}",
                                vm.supply
                                    .reserve_days
                                    .map_or("—".into(), |d| format!("{d:.1} суток")),
                                vm.supply.bottleneck_reason.unwrap_or_default()
                            ),
                            62.,
                            ui::GOLD,
                        );
                    }
                }
                let hubs: Vec<_> = w
                    .conditions
                    .iter()
                    .filter(|(p, c)| {
                        c.supply_node && world.controller_of_province(**p) == Some(tag)
                    })
                    .map(|(&p, _)| p)
                    .collect();
                for hub in hubs {
                    let motor = w.logistics.motorized_hubs.contains(&hub);
                    if rows.button(
                        &format!(
                            "Хаб {hub}: {}",
                            if motor {
                                "грузовики"
                            } else {
                                "пешая доставка"
                            }
                        ),
                        motor,
                    ) {
                        if motor {
                            w.logistics.motorized_hubs.remove(&hub);
                        } else {
                            w.logistics.motorized_hubs.insert(hub);
                        }
                        w.logistics.network_key = None;
                        view.notice =
                            "Грузовики резервируются из запаса страны при ходе времени".into();
                    }
                }
                let regions: Vec<_> = w.air.regions.keys().copied().collect();
                let target = m
                    .air_region
                    .filter(|r| regions.contains(r))
                    .or_else(|| {
                        view.selected.iter().find_map(|id| {
                            w.divisions
                                .get(id)
                                .and_then(|d| w.air.region_at(d.province))
                        })
                    })
                    .or_else(|| regions.first().copied());
                m.air_region = target;
                if rows.button(
                    &format!(
                        "Регион миссии: {} · следующий",
                        target.map_or("нет данных".into(), |r| r.to_string())
                    ),
                    false,
                ) {
                    if !regions.is_empty() {
                        let at = target
                            .and_then(|r| regions.iter().position(|x| *x == r))
                            .unwrap_or(0);
                        m.air_region = Some(regions[(at + 1) % regions.len()]);
                    }
                }
                let bases: Vec<_> = w
                    .air
                    .bases
                    .keys()
                    .copied()
                    .filter(|&p| world.controller_of_province(p) == Some(tag))
                    .collect();
                let base = m
                    .air_base
                    .filter(|p| bases.contains(p))
                    .or_else(|| bases.first().copied());
                m.air_base = base;
                if rows.button(
                    &format!(
                        "Авиабаза: {} · следующая",
                        base.map_or("нет данных".into(), |p| p.to_string())
                    ),
                    false,
                ) {
                    if !bases.is_empty() {
                        let at = base
                            .and_then(|p| bases.iter().position(|x| *x == p))
                            .unwrap_or(0);
                        m.air_base = Some(bases[(at + 1) % bases.len()]);
                    }
                }
                if let Some(p) = base {
                    let models: Vec<_> = w
                        .catalogue
                        .equipment
                        .values()
                        .filter(|e| {
                            e.buildable
                                && !e.modular
                                && crate::air_warfare::is_aircraft(e)
                                && w.countries.get(tag).is_some_and(|a| {
                                    w.catalogue.equipment_available(&e.id, &a.modifiers)
                                })
                        })
                        .map(|e| e.id.clone())
                        .collect();
                    rows.text("Новые авиагруппы получают самолёты из национального запаса при ходе времени. Самолёты не создаются кнопкой.",78.,ui::MUTED);
                    for model in models {
                        if rows.button(&format!("Создать авиагруппу: {model}"), false)
                        {
                            let res = w
                                .air
                                .create_wing(tag, &model, p, world, &w.catalogue)
                                .map(|_| ());
                            result(view, res, "Авиагруппа создана: ожидает самолёты из запаса");
                        }
                    }
                }
                let wings: Vec<_> = w
                    .air
                    .wings
                    .values()
                    .filter(|a| a.country == tag)
                    .cloned()
                    .collect();
                if wings.is_empty() {
                    rows.text("Авиагрупп нет. Нужны известные модели самолётов, авиабаза и техника в запасе страны.",90.,ui::MUTED);
                }
                for a in wings {
                    rows.text(
                        &format!(
                            "{} · {} самолётов\nБаза {} · регион {}\nВылетов {} · потери {}",
                            a.name,
                            a.planes,
                            a.base,
                            a.region.map_or("—".into(), |r| r.to_string()),
                            a.sorties,
                            a.losses
                        ),
                        82.,
                        ui::TEXT,
                    );
                    if rows.button(
                        &format!("{} · сменить миссию", a.mission.label()),
                        a.mission != AirMission::Standby,
                    ) {
                        let next = match a.mission {
                            AirMission::Standby => AirMission::Superiority,
                            AirMission::Superiority => AirMission::Interception,
                            AirMission::Interception => AirMission::CloseAirSupport,
                            AirMission::CloseAirSupport => AirMission::Standby,
                        };
                        let res = w.air.rebase_and_assign(
                            tag,
                            a.id,
                            a.base,
                            next,
                            target,
                            world,
                            &w.catalogue,
                        );
                        result(view, res, "Миссия назначена");
                    }
                    if rows.button("Назначить выбранные регион и базу", false)
                    {
                        let res = base
                            .ok_or_else(|| "Выберите доступную авиабазу".to_owned())
                            .and_then(|base| {
                                w.air.rebase_and_assign(
                                    tag,
                                    a.id,
                                    base,
                                    a.mission,
                                    target,
                                    world,
                                    &w.catalogue,
                                )
                            });
                        result(view, res, "Регион и база назначены");
                    }
                }
                if let Some(r) = target
                    .and_then(|id| w.air.results.get(&id))
                    .and_then(|r| r.get(tag))
                {
                    rows.text(&format!("Последний вылет: час {}\nСамолётов {} · истребители {} · CAS {}\nПотери {} · топливо {:.1}",r.hour,r.sorties,r.fighters,r.cas,r.losses,r.fuel_used),90.,ui::GOLD);
                }
            }
            MobilePage::Armies => {
                rows.text(
                    "Выберите армию или создайте её из выделенных дивизий.",
                    62.,
                    ui::MUTED,
                );
                if rows.button("+ Создать из выделенных", false) {
                    let ids = view.selected.iter().copied().collect::<Vec<_>>();
                    match w.create_army(tag, &ids, "") {
                        Ok(id) => {
                            view.selected_army = Some(id);
                            navigate(&mut m, MobilePage::Army);
                        }
                        Err(e) => view.notice = e,
                    }
                }
                let ids = w
                    .command
                    .armies
                    .values()
                    .filter(|a| a.country == tag)
                    .map(|a| a.id)
                    .collect::<Vec<_>>();
                for id in ids {
                    let r = rows.rect(64.);
                    if r.bottom() < body.y || r.y > body.bottom() {
                        continue;
                    }
                    let a = &w.command.armies[&id];
                    let title = format!("{} · {} див.", a.name, w.command.members(id).count());
                    if button(
                        &title,
                        r,
                        Some(id) == army,
                        tap.filter(|p| body.contains(*p)),
                    ) {
                        view.selected_army = Some(id);
                        view.selected = w.command.members(id).collect();
                        view.selected_front = None;
                        view.selected_plan = None;
                        navigate(&mut m, MobilePage::Army);
                    }
                }
            }
            MobilePage::Army | MobilePage::Roster => {
                if rows.button("Снабжение, топливо и авиация", false) {
                    navigate(&mut m, MobilePage::Logistics);
                }

                if let Some(vm) = army.and_then(|id| army_model(w, tag, id, &mut m)) {
                    rows.text(&vm.name, 48., ui::GOLD);
                    if page == MobilePage::Roster {
                        rows.text(if view.touch_add {"Мультивыбор: касания добавляют и убирают дивизии."}else{"Касание выбирает одну дивизию. Для нескольких включите мультивыбор."},66.,ui::MUTED);
                        // Stable IDs and visible rows only; no per-row stat projection for offscreen units.
                        for id in &vm.division_ids {
                            let r = rows.rect(64.);
                            if r.bottom() < body.y || r.y > body.bottom() {
                                continue;
                            }
                            let d = &w.divisions[id];
                            if button(
                                &format!(
                                    "{} · {}\nОрг {:.0}% · Состав {:.0}%",
                                    if view.selected.contains(id) {
                                        "✓"
                                    } else {
                                        "□"
                                    },
                                    d.name,
                                    d.organization / d.derived.stats.organization.max(1.) * 100.,
                                    d.strength() * 100.
                                ),
                                r,
                                view.selected.contains(id),
                                tap.filter(|p| body.contains(*p)),
                            ) {
                                if view.touch_add {
                                    if !view.selected.insert(*id) {
                                        view.selected.remove(id);
                                    }
                                } else {
                                    view.selected.clear();
                                    view.selected.insert(*id);
                                }
                                view.touch_move = false;
                            }
                        }
                    } else {
                        let title = vm
                            .commander
                            .as_ref()
                            .map_or("Назначить генерала".into(), |c| {
                                format!("Генерал: {} · уровень {}", c.name, c.skill)
                            });
                        if rows.button(&title, false) {
                            navigate(
                                &mut m,
                                vm.commander
                                    .as_ref()
                                    .map_or(MobilePage::Picker(false), |c| {
                                        MobilePage::Commander(c.id.clone())
                                    }),
                            );
                        }
                        if let Some(group) = w.command.armies[&vm.id]
                            .group
                            .and_then(|id| w.command.groups.get(&id))
                            .cloned()
                        {
                            let marshal = group
                                .field_marshal
                                .as_deref()
                                .and_then(|id| army_projection::commander(w, id, "russian"));
                            let title = marshal.as_ref().map_or(
                                format!("{} · назначить фельдмаршала", group.name),
                                |c| format!("{} · {}", group.name, c.name),
                            );
                            if rows.button(&title, false) {
                                navigate(
                                    &mut m,
                                    marshal.map_or(MobilePage::Picker(true), |c| {
                                        MobilePage::Commander(c.id)
                                    }),
                                );
                            }
                        } else if rows.button("Создать группу армий", false) {
                            result(
                                view,
                                w.command.create_group(tag, vm.id, "").map(|_| ()),
                                "Группа армий создана",
                            );
                        }
                        if rows.button(
                            &format!("Дивизии: {} · открыть состав", vm.division_count),
                            false,
                        ) {
                            navigate(&mut m, MobilePage::Roster);
                        }
                        rows.ratio("Организация", vm.organization_ratio);
                        rows.ratio("Прочность", vm.strength_ratio);
                        rows.ratio("Снабжение", vm.supply.ratio);
                        if let Some(reason) = &vm.supply.bottleneck_reason {
                            rows.text(reason, 54., ui::MUTED);
                        }
                        rows.text(
                            &format!(
                                "Снабжение: {:.2} / {:.2}\nПланов: {} · выполняется: {}",
                                vm.supply.received,
                                vm.supply.requested,
                                vm.plan_ids.len(),
                                vm.executing_plan_ids.len()
                            ),
                            58.,
                            ui::MUTED,
                        );
                        rows.ratio("Планирование", vm.planning);
                        if rows.button("Выбрать всю армию", false) {
                            view.selected = vm.division_ids.iter().copied().collect();
                            view.touch_move = false;
                        }
                        if rows.button(
                            "Двигать выделенные → выбрать цель на карте",
                            view.touch_move,
                        ) {
                            view.touch_move = true;
                            view.touch_box = false;
                            view.open = false;
                            view.notice =
                                "ПРИКАЗ: коснитесь провинции. Перетаскивание перемещает карту."
                                    .into();
                        }
                        if rows.button("Стоп: удерживать выделенные", false)
                        {
                            result(
                                view,
                                crate::army_controls::stop_selection(w, world, tag, &view.selected)
                                    .errors
                                    .first()
                                    .map_or(Ok(()), |e| Err(e.clone())),
                                "Дивизии удерживают позиции",
                            );
                        }
                        if rows.button("Вернуть выделенные к плану", false) {
                            result(
                                view,
                                w.resume_plans(
                                    tag,
                                    &view.selected.iter().copied().collect::<Vec<_>>(),
                                ),
                                "Дивизии возвращены к плану",
                            );
                        }
                        for (name, tool) in [
                            (
                                "Создать / изменить фронт",
                                crate::battle_plan_ui::Tool::Front,
                            ),
                            ("Линия наступления", crate::battle_plan_ui::Tool::Offensive),
                            ("Прорыв", crate::battle_plan_ui::Tool::Spearhead),
                            ("Линия обороны", crate::battle_plan_ui::Tool::Fallback),
                        ] {
                            if rows.button(name, false) {
                                crate::battle_plan_ui::begin_tool(view, w, tag, tool);
                                view.confirm_drawing = true;
                            }
                        }
                        if rows.button("Группа армий, способности и настройки", false)
                        {
                            m.advanced = true;
                            view.army_scroll = 0.;
                        }
                    }
                } else {
                    rows.text(
                        "Выделите дивизии на карте и создайте армию. Затем назначьте командующего.",
                        90.,
                        ui::MUTED,
                    );
                    if rows.button("Армии / создать армию", false) {
                        navigate(&mut m, MobilePage::Armies);
                    }
                    if rows.button("Посмотреть командующих страны", false)
                    {
                        navigate(&mut m, MobilePage::Picker(false));
                    }
                }
            }
            MobilePage::Picker(marshal) => {
                rows.text(
                    if marshal {
                        "Фельдмаршалы"
                    } else {
                        "Генералы · нажмите для подробностей"
                    },
                    48.,
                    ui::GOLD,
                );
                if army.is_some() && rows.button("Снять командующего", false) {
                    assign(view, w, tag, None, marshal);
                }
                let ids = w
                    .command
                    .commanders
                    .values()
                    .filter(|c| {
                        c.country == tag
                            && (!marshal || c.rank == crate::command::Rank::FieldMarshal)
                    })
                    .map(|c| c.id.clone())
                    .collect::<Vec<_>>();
                if ids.is_empty() {
                    rows.text(
                        "В доступных данных нет командующих этой роли.",
                        70.,
                        ui::MUTED,
                    );
                }
                for id in ids {
                    let r = rows.rect(64.);
                    if r.bottom() < body.y || r.y > body.bottom() {
                        continue;
                    }
                    if let Some(c) = army_projection::commander(w, &id, "russian") {
                        if button(
                            &format!(
                                "{} · ур. {}\nА{}  З{}  П{}  Л{}",
                                c.name, c.skill, c.attack, c.defense, c.planning, c.logistics
                            ),
                            r,
                            false,
                            tap.filter(|p| body.contains(*p)),
                        ) {
                            navigate(&mut m, MobilePage::Commander(id));
                        }
                    }
                }
            }
            MobilePage::Commander(ref id) => {
                if let Some(c) = army_projection::commander(w, id, "russian").filter(|_| {
                    w.command
                        .commanders
                        .get(id)
                        .is_some_and(|c| c.country == tag)
                }) {
                    let portrait = rows.rect(72.);
                    crate::ui_icons::draw(
                        crate::ui_icons::Icon::Officer,
                        Rect::new(portrait.x + 6., portrait.y + 6., 48., 54.),
                        ui::GOLD,
                    );
                    ui::paragraph(
                        &c.name,
                        Rect::new(portrait.x + 68., portrait.y + 8., portrait.w - 74., 62.),
                        17.,
                        ui::GOLD,
                    );
                    rows.text(
                        &format!(
                            "{} · уровень {}\nОпыт {:.1} · следующий уровень: {}",
                            if c.rank == crate::command::Rank::FieldMarshal {
                                "Фельдмаршал"
                            } else {
                                "Генерал"
                            },
                            c.skill,
                            c.experience,
                            c.next_level_xp
                                .map_or("нет данных".into(), |v| format!("{v:.1}"))
                        ),
                        72.,
                        ui::TEXT,
                    );
                    rows.text(
                        &format!(
                            "Атака: {}    Защита: {}\nПланирование: {}    Логистика: {}",
                            c.attack, c.defense, c.planning, c.logistics
                        ),
                        60.,
                        ui::TEXT,
                    );
                    let assignment = w
                        .command
                        .armies
                        .values()
                        .find(|a| a.general.as_deref() == Some(id))
                        .map(|a| a.name.as_str())
                        .or_else(|| {
                            w.command
                                .groups
                                .values()
                                .find(|g| g.field_marshal.as_deref() == Some(id))
                                .map(|g| g.name.as_str())
                        })
                        .unwrap_or("Не назначен");
                    rows.text(&format!("Назначение: {assignment}"), 48., ui::MUTED);
                    if army.is_some() && rows.button("Назначить на выбранную армию", false)
                    {
                        assign(view, w, tag, Some(id), false);
                    }
                    if c.rank == crate::command::Rank::FieldMarshal
                        && army.is_some()
                        && rows.button("Назначить на группу армий", false)
                    {
                        assign(view, w, tag, Some(id), true);
                    }
                    if rows.button("Выбрать другого командующего", false)
                    {
                        navigate(&mut m, MobilePage::Picker(false));
                    }
                    if c.traits.is_empty() {
                        rows.text("Черты отсутствуют.", 48., ui::MUTED);
                    }
                    for t in c.traits {
                        if rows.button(
                            &format!(
                                "{}{}",
                                t.name,
                                if t.definition_known {
                                    ""
                                } else {
                                    " · нет определения"
                                }
                            ),
                            false,
                        ) {
                            navigate(&mut m, MobilePage::Trait(id.clone(), t.id));
                        }
                    }
                    rows.text(
                        if w.abilities.hq_rules.is_some() {
                            "HQ доступен через настройки армии."
                        } else {
                            "Штаб: подтверждённые правила HQ не импортированы."
                        },
                        70.,
                        ui::MUTED,
                    );
                } else {
                    rows.text("Командующий недоступен этой стране.", 64., ui::MUTED);
                }
            }
            MobilePage::Trait(ref commander, ref trait_id) => {
                if let Some(c) = army_projection::commander(w, commander, "russian").filter(|_| {
                    w.command
                        .commanders
                        .get(commander)
                        .is_some_and(|c| c.country == tag)
                }) {
                    if let Some(t) = c.traits.iter().find(|t| &t.id == trait_id) {
                        rows.text(&t.name, 62., ui::GOLD);
                        let description = if !t.definition_known {
                            "Определение черты отсутствует. Неизвестный ID сохранён, бонусы не выдумываются."
                        } else if t.description.is_empty()
                            || t.description == format!("{}_desc", t.id)
                        {
                            "Описание черты не переведено."
                        } else {
                            &t.description
                        };
                        let height = ui::paragraph_height(description, body.w - 24.0, 15.0) + 16.0;
                        rows.text(description, height.max(60.0), ui::TEXT);
                    }
                }
            }
        }
    }
    let content_height = rows.y - (body.y - m.scroll);
    m.scroll = m.scroll.clamp(0., (content_height - body.h).max(0.));
    let footer = Rect::new(panel.x + 6., panel.bottom() - 56., panel.w - 12., 50.);
    let cw = (footer.w - 12.) / 3.;
    if button(
        if page == MobilePage::Army {
            "Армии"
        } else {
            "Назад"
        },
        Rect::new(footer.x, footer.y, cw, 48.),
        false,
        tap,
    ) {
        let back = match page {
            MobilePage::Army => MobilePage::Armies,
            MobilePage::Trait(id, _) => MobilePage::Commander(id),
            MobilePage::Commander(_) => MobilePage::Picker(false),
            _ => MobilePage::Army,
        };
        navigate(&mut m, back);
    }
    if button(
        "Несколько",
        Rect::new(footer.x + cw + 6., footer.y, cw, 48.),
        view.touch_add,
        tap,
    ) {
        view.touch_add = !view.touch_add;
        view.touch_move = false;
    }
    if button(
        "К карте",
        Rect::new(footer.x + 2. * (cw + 6.), footer.y, cw, 48.),
        false,
        tap,
    ) {
        view.open = false;
    }
    view.mobile = m;
}
fn assign(view: &mut ArmyView, w: &mut Warfare, tag: &str, id: Option<&str>, marshal: bool) {
    let r = view
        .selected_army
        .ok_or_else(|| "Сначала выберите армию".into())
        .and_then(|army| {
            if marshal {
                w.command
                    .own_army(tag, army)?
                    .group
                    .ok_or_else(|| "Сначала создайте группу армий".into())
                    .and_then(|g| w.command.set_marshal(tag, g, id))
            } else {
                w.command.set_general(tag, army, id)
            }
        });
    result(view, r, "Командующий назначен");
}
pub fn selection(w: &Warfare, view: &mut ArmyView, panel: Rect) {
    let mut m = std::mem::take(&mut view.mobile);
    let tap = tap_frame(&mut m, panel, panel);
    ui::panel(panel.x, panel.y, panel.w, panel.h);
    ui::label(
        &format!(
            "Выбрано: {} · {}",
            view.selected.len(),
            if view.touch_move {
                "ПРИКАЗ"
            } else if view.touch_box {
                "РАМКА"
            } else if view.touch_add {
                "МУЛЬТИВЫБОР"
            } else {
                "ВЫБОР"
            }
        ),
        Rect::new(panel.x + 8., panel.y + 8., panel.w - 74., 28.),
        14.,
        ui::GOLD,
    );
    if button(
        "×",
        Rect::new(panel.right() - 54., panel.y + 5., 48., 48.),
        false,
        tap,
    ) {
        view.selected.clear();
        view.selected_army = None;
        view.selected_front = None;
        view.selected_plan = None;
        view.selected_order = None;
        view.touch_move = false;
        view.touch_box = false;
        view.cancel_tool();
        view.mobile = m;
        return;
    }
    ui::paragraph(
        if view.touch_move {
            "Коснитесь провинции. Жест перетаскивания двигает карту."
        } else if view.touch_box {
            "Протяните рамку по карте. Для перемещения карты выключите рамку."
        } else {
            "Касание выбирает дивизию. Перетаскивание двигает карту."
        },
        Rect::new(panel.x + 8., panel.y + 57., panel.w - 16., 44.),
        13.,
        ui::MUTED,
    );
    let width = (panel.w - 25.) / 4.;
    for (i, title) in ["Выбор", "Рамка", "Двигать", "Армия"]
        .into_iter()
        .enumerate()
    {
        if button(
            title,
            Rect::new(
                panel.x + 5. + i as f32 * (width + 5.),
                panel.bottom() - 57.,
                width,
                48.,
            ),
            (i == 0 && view.touch_add) || (i == 1 && view.touch_box) || (i == 2 && view.touch_move),
            tap,
        ) {
            match i {
                0 => {
                    view.touch_add = !view.touch_add;
                    view.touch_move = false;
                    view.touch_box = false;
                }
                1 => {
                    view.touch_box = !view.touch_box;
                    view.touch_move = false;
                }
                2 => {
                    view.touch_move = !view.touch_move;
                    view.touch_box = false;
                }
                _ => {
                    view.selected_army = crate::army_command_ui::selected_army(view, w);
                    view.show_command(false);
                    navigate(&mut m, MobilePage::Army);
                    m.advanced = false;
                }
            }
        }
    }
    view.mobile = m;
}
