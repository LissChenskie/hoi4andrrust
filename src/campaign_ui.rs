//! Campaign HUD, navigation and visible shells for systems not simulated yet.
//! The HUD reads the campaign; placeholder controls never manufacture game state.
use crate::{
    army_ui::ArmyView, construction_ui::ConstructionView, production_ui::ProductionView,
    research_ui::ResearchView,
};
use crate::{
    hoi_ui as ui,
    ui_icons::{self, Icon},
    ui_layout::Layout,
};
use crate::{
    naval_warfare::{NavalMission, NavalState},
    production::ProductionState,
    research::ResearchWorld,
    resource_economy::{ResourceEconomy, TradeDeal, TradeLaw},
    warfare::Warfare,
    world::World,
};
use macroquad::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Politics,
    Decisions,
    Research,
    Diplomacy,
    Trade,
    Construction,
    Production,
    Recruitment,
    Logistics,
    Agency,
    Officer,
    Focus,
    Army,
    Navy,
    Air,
}
impl Panel {
    pub const NAV: [Self; 12] = [
        Self::Politics,
        Self::Decisions,
        Self::Research,
        Self::Diplomacy,
        Self::Trade,
        Self::Construction,
        Self::Production,
        Self::Recruitment,
        Self::Logistics,
        Self::Agency,
        Self::Officer,
        Self::Focus,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Politics => "Политика",
            Self::Decisions => "События и решения",
            Self::Research => "Исследования",
            Self::Diplomacy => "Дипломатия",
            Self::Trade => "Торговля",
            Self::Construction => "Строительство",
            Self::Production => "Производство",
            Self::Recruitment => "Набор и развёртывание",
            Self::Logistics => "Логистика",
            Self::Agency => "Разведывательное агентство",
            Self::Officer => "Офицерский корпус",
            Self::Focus => "Национальные фокусы",
            Self::Army => "Сухопутные войска",
            Self::Navy => "Военно-морской флот",
            Self::Air => "Военно-воздушные силы",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            Self::Politics => "ПОЛИТ.",
            Self::Decisions => "РЕШЕНИЯ",
            Self::Research => "НАУКА",
            Self::Diplomacy => "ДИПЛ.",
            Self::Trade => "ТОРГ.",
            Self::Construction => "СТРОЙКА",
            Self::Production => "ВЫПУСК",
            Self::Recruitment => "НАБОР",
            Self::Logistics => "СКЛАД",
            Self::Agency => "РАЗВЕДКА",
            Self::Officer => "КОРПУС",
            Self::Focus => "ФОКУСЫ",
            _ => self.title(),
        }
    }
    pub fn binding(self) -> Option<(KeyCode, bool)> {
        Some(match self {
            Self::Politics => (KeyCode::Q, false),
            Self::Decisions => (KeyCode::Q, true),
            Self::Research => (KeyCode::W, false),
            Self::Diplomacy => (KeyCode::E, false),
            Self::Trade => (KeyCode::R, false),
            Self::Construction => (KeyCode::T, false),
            Self::Production => (KeyCode::Y, false),
            Self::Recruitment => (KeyCode::U, false),
            Self::Logistics => (KeyCode::I, false),
            Self::Agency => (KeyCode::W, true),
            Self::Officer => (KeyCode::E, true),
            Self::Army => (KeyCode::O, false),
            Self::Navy => (KeyCode::P, false),
            Self::Air => (KeyCode::L, false),
            Self::Focus => return None,
        })
    }
    pub fn key(self) -> String {
        self.binding().map_or(String::new(), |(key, shift)| {
            format!("{}{key:?}", if shift { "Shift + " } else { "" })
        })
    }
    pub fn icon(self) -> Icon {
        match self {
            Self::Politics => Icon::Politics,
            Self::Decisions => Icon::Decisions,
            Self::Research => Icon::Research,
            Self::Diplomacy => Icon::Diplomacy,
            Self::Trade => Icon::Trade,
            Self::Construction => Icon::Construction,
            Self::Production => Icon::Production,
            Self::Recruitment => Icon::Recruit,
            Self::Logistics => Icon::Logistics,
            Self::Agency => Icon::Agency,
            Self::Officer => Icon::Officer,
            Self::Focus => Icon::Focus,
            Self::Army => Icon::Army,
            Self::Navy => Icon::Navy,
            Self::Air => Icon::Air,
        }
    }
    pub fn placeholder(self) -> bool {
        matches!(self, Self::Politics | Self::Agency)
    }
}

pub struct Hud {
    pub active: Option<Panel>,
    pub diplomacy_country: Option<String>,
    pub outliner: bool,
    pub minimap: bool,
    pub orders_layout: bool,
    pub map_palette: bool,
    pub war_panel_open: bool,
    pub menu: bool,
    pub menu_was_paused: bool,
    pub reload_requested: bool,
    pub save_requested: bool,
    pub load_requested: bool,
    pub detail: Option<String>,
    pub scroll: usize,
    pub outliner_scroll: usize,
    pub army_dock_scroll: usize,
    pub notice: String,
    pub trade_partner_index: usize,
    pub trade_resource_index: usize,
    pub trade_factories: u32,
    pub naval_region_index: usize,
    pub naval_hull_index: usize,
    pub naval_page: u8,
    pub naval_slot_index: usize,
    pub naval_module_index: usize,
    pub naval_design_index: usize,
    pub naval_refit_index: usize,
    pub naval_aircraft_index: usize,
    pub naval_carrier_index: usize,
    pub naval_target_planes: u32,
    pub naval_draft: std::collections::BTreeMap<String, String>,
    pub focus_view: crate::focus_ui::View,
    pub focus_request: Option<crate::focus_ui::Request>,
    pub event_request: Option<(u64, usize)>,
}
impl Default for Hud {
    fn default() -> Self {
        Self {
            active: None,
            diplomacy_country: None,
            outliner: true,
            minimap: true,
            orders_layout: false,
            map_palette: false,
            war_panel_open: false,
            menu: false,
            menu_was_paused: true,
            reload_requested: false,
            save_requested: false,
            load_requested: false,
            detail: None,
            scroll: 0,
            outliner_scroll: 0,
            army_dock_scroll: 0,
            notice: String::new(),
            trade_partner_index: 0,
            trade_resource_index: 0,
            trade_factories: 1,
            naval_region_index: 0,
            naval_hull_index: 0,
            naval_page: 0,
            naval_slot_index: 0,
            naval_module_index: 0,
            naval_design_index: 0,
            naval_refit_index: 0,
            naval_aircraft_index: 0,
            naval_carrier_index: 0,
            naval_target_planes: 10,
            naval_draft: Default::default(),
            focus_view: Default::default(),
            focus_request: None,
            event_request: None,
        }
    }
}
impl Hud {
    pub fn open(
        &mut self,
        panel: Option<Panel>,
        c: &mut ConstructionView,
        p: &mut ProductionView,
        r: &mut ResearchView,
        a: &mut ArmyView,
    ) {
        self.map_palette = false;
        self.war_panel_open = false;
        c.close();
        p.close();
        r.open = false;
        r.browsing = false;
        a.close();
        a.deployment_pick = false;
        self.active = panel;
        self.scroll = 0;
        self.detail = None;
        match panel {
            Some(Panel::Construction) => c.open = true,
            Some(Panel::Production) => p.open = true,
            Some(Panel::Research) => {
                r.open = true;
                r.browsing = false;
            }
            Some(Panel::Army) => a.open = true,
            Some(Panel::Recruitment) => a.show_recruitment(),
            Some(Panel::Officer) => a.show_command(false),
            Some(Panel::Air) => {
                a.show_command(false);
                a.mobile.page = crate::mobile_army_ui::MobilePage::Logistics;
            }
            _ => {}
        }
    }
    pub fn toggle(
        &mut self,
        panel: Panel,
        c: &mut ConstructionView,
        p: &mut ProductionView,
        r: &mut ResearchView,
        a: &mut ArmyView,
    ) {
        self.open(
            if self.active == Some(panel) {
                None
            } else {
                Some(panel)
            },
            c,
            p,
            r,
            a,
        );
    }
    pub fn reconcile(
        &mut self,
        c: &ConstructionView,
        p: &ProductionView,
        r: &ResearchView,
        a: &ArmyView,
    ) {
        let closed = match self.active {
            Some(Panel::Construction) => !c.open,
            Some(Panel::Production) => !p.open,
            Some(Panel::Research) => !r.open,
            Some(Panel::Army | Panel::Recruitment | Panel::Officer | Panel::Air) => !a.open,
            _ => false,
        };
        if closed {
            self.active = None;
        }
    }
    pub fn minimap_bounds(&self) -> Rect {
        let mut r = Layout::current().minimap();
        if self.orders_layout {
            r.y = crate::battle_plan_ui::toolbar_bounds().y - 42.0 - 8.0 - r.h;
        }
        r
    }
    pub fn regions(&self) -> Vec<Rect> {
        let l = Layout::current();
        let mut out = vec![
            Rect::new(0.0, 0.0, l.width, l.top),
            l.army_dock(),
            l.map_modes(),
        ];
        if self.active == Some(Panel::Focus)
            || self.war_panel_open
            || self.menu
            || self.detail.is_some()
            || self.map_palette
        {
            out.push(Rect::new(0.0, 0.0, l.width, l.height));
        } else if self.active.is_some() {
            out.push(l.primary());
        }
        if !l.compact {
            let x = if self.active.is_some() {
                l.primary().right() + 8.0
            } else {
                12.0
            };
            if x + 150.0 < l.outliner().x {
                out.push(Rect::new(x, l.top + 8.0, 83.0, 31.0));
            }
            out.push(Rect::new(l.width - 93.0, l.top + 4.0, 53.0, 29.0));
        }
        if self.outliner && !l.compact {
            out.push(l.outliner());
        }
        out.push(Rect::new(l.width - 33.0, l.top + 5.0, 28.0, 28.0));
        if self.minimap && !l.compact {
            out.push(self.minimap_bounds());
        }
        out
    }
    pub fn blocked(&self, p: Vec2) -> bool {
        self.regions().iter().any(|r| r.contains(p))
    }
}

#[derive(Debug)]
pub enum Action {
    Open(Panel),
    Select(u64),
    SelectAll,
    SelectArmy(u64, bool),
    AssignArmy(u64),
    ToggleArmy(u64),
    CreateArmy,
    Commander,
    Designer(usize),
    Stop,
    Redeploy,
    MapMode(usize),
    Zoom(f32),
    Fit,
    Menu,
    Resume,
    MainMenu,
    Save,
    Load,
    Reload,
    DeclareWar(String),
    NavalOrder(u64, NavalMission, u32),
    NavalQueueShip(String, u32),
    NavalCancelShip(u64),
    NavalAssignAdmiral(u64, Option<String>),
    NavalSaveDesign(String, String, std::collections::BTreeMap<String, String>),
    NavalQueueDesign(String, u32),
    NavalQueueRefit(u64, usize, String),
    NavalAssignCarrierWing(u64, usize, String, u32),
    NavalSetCarrierStance(u64, crate::naval_carrier::CarrierStance),
}

fn icon_button(icon: Icon, r: Rect, active: bool, title: &str, body: &str) -> bool {
    let hot = ui::inside(r);
    draw_rectangle(
        r.x,
        r.y,
        r.w,
        r.h,
        if active {
            Color::from_rgba(95, 88, 62, 255)
        } else if hot {
            Color::from_rgba(78, 84, 82, 255)
        } else {
            Color::from_rgba(45, 51, 53, 255)
        },
    );
    draw_rectangle_lines(
        r.x,
        r.y,
        r.w,
        r.h,
        1.0,
        if active { ui::GOLD } else { ui::EDGE },
    );
    draw_line(
        r.x + 1.0,
        r.y + 1.0,
        r.right() - 1.0,
        r.y + 1.0,
        1.0,
        Color::from_rgba(110, 115, 106, 255),
    );
    ui_icons::draw(
        icon,
        Rect::new(r.x + 5.0, r.y + 4.0, r.w - 10.0, r.h - 8.0),
        if active { ui::GOLD } else { ui::TEXT },
    );
    ui::tooltip(r, title, body);
    hot && is_mouse_button_pressed(MouseButton::Left)
}
fn stub_button(h: &mut Hud, title: &str, r: Rect) {
    ui::disabled(title, r, "ЗАГЛУШКА · Механика ещё не реализована.");
    if ui::inside(r) && is_mouse_button_pressed(MouseButton::Left) {
        h.detail = Some(title.into());
    }
}
fn value(icon: Icon, r: Rect, value: &str, title: &str, body: &str, live: bool) {
    ui_icons::draw(
        icon,
        Rect::new(r.x + 4.0, r.y + 5.0, 18.0, 19.0),
        if live { ui::GOLD } else { ui::MUTED },
    );
    let mut size = 12u16;
    while size > 9 && crate::measure_ui_text(value, size).width > r.w - 28.0 {
        size -= 1;
    }
    ui::label(
        value,
        Rect::new(r.x + 26.0, r.y + 7.0, r.w - 28.0, 20.0),
        size as f32,
        if live { ui::TEXT } else { ui::MUTED },
    );
    draw_line(
        r.right(),
        r.y + 4.0,
        r.right(),
        r.bottom() - 3.0,
        1.0,
        Color::from_rgba(67, 70, 66, 255),
    );
    ui::tooltip(r, title, body);
}
fn number(v: u64) -> String {
    if v >= 1_000_000 {
        format!("{:.2}M", v as f64 / 1_000_000.0)
    } else if v >= 10_000 {
        format!("{:.1}K", v as f64 / 1000.0)
    } else {
        v.to_string()
    }
}

pub fn draw_chrome(
    h: &mut Hud,
    world: &World,
    tag: Option<&str>,
    w: &Warfare,
    prod: &ProductionState,
    research: &ResearchWorld,
    selected: &ArmyView,
    mode: usize,
) -> Vec<Action> {
    let l = Layout::current();
    let mut actions = Vec::new();
    draw_rectangle(0.0, 0.0, l.width, l.top, Color::from_rgba(35, 41, 43, 255));
    draw_rectangle(
        0.0,
        0.0,
        l.width,
        if l.compact { 37.0 } else { 34.0 * l.scale },
        ui::BASE,
    );
    draw_line(0.0, l.top - 1.0, l.width, l.top - 1.0, 2.0, ui::EDGE);
    if let Some(tag) = tag {
        let flag = Rect::new(7.0, 5.0, 45.0, 27.0);
        ui::flag(tag, flag);
        ui::tooltip(
            flag,
            crate::country_display_name(tag),
            "Открыть политику страны · Q",
        );
        if ui::inside(flag) && is_mouse_button_pressed(MouseButton::Left) {
            actions.push(Action::Open(Panel::Politics));
        }
    }
    let economy = tag
        .and_then(|t| world.country_economy(t))
        .unwrap_or_default();
    let army = tag.and_then(|t| w.countries.get(t));
    let factories = economy.civilian_factories + economy.military_factories + economy.dockyards;
    let manpower = number(army.map_or(0, |a| a.manpower_pool));
    let manpower_body = army.map_or("Военные данные страны не загружены".into(), |a| {
        let serving: u64 = w.divisions.values().filter(|d| !d.destroyed && Some(d.country.as_str()) == tag)
            .map(|d| u64::from(d.manpower)).sum();
        format!("Свободно: {}; в дивизиях: {}. Население своих основных территорий: {}; призывной фонд: {}. Источник: {}.{}",
            a.manpower_pool, serving, a.core_population, a.recruitable_total,
            a.history_source.as_deref().unwrap_or("история не импортирована"),
            if a.core_population == 0 { " Не импортировано население / основные территории." }
            else if a.recruitable_total == 0 { " Проверьте импорт призывного закона в common/ideas." } else { "" })
    });
    let xp = army.map_or("—".into(), |a| format!("{:.0}", a.army_xp));
    let support = tag.and_then(|t| w.logistics.countries.get(t));
    let fuel_body=support.map_or("Нет исходных топливных данных".into(),|c|format!("Запас {:.0} / {:.0}; расход за час {:.1}. Дивизии и авиация расходуют один национальный запас. Нажмите для управления.",c.fuel,c.fuel_capacity,c.fuel_used_last_hour));
    let mut demand = 0.;
    let mut received = 0.;
    for d in w
        .divisions
        .values()
        .filter(|d| !d.destroyed && Some(d.country.as_str()) == tag)
    {
        let n = crate::logistics::supply_demand(d);
        demand += n;
        received += n * d.supply_fraction.clamp(0., 1.);
    }
    let supply_label = if demand > 0. && w.logistics.enabled {
        format!("{:.0}%", received / demand * 100.)
    } else {
        "—".into()
    };
    let metrics = [
        (
            Icon::Power,
            "—".into(),
            "Политическая власть",
            "ЗАГЛУШКА · Политическая власть и ежедневный прирост ещё не рассчитываются.",
            false,
        ),
        (
            Icon::Stability,
            "—".into(),
            "Стабильность",
            "ЗАГЛУШКА · Стабильность страны ещё не рассчитывается.",
            false,
        ),
        (
            Icon::WarSupport,
            "—".into(),
            "Поддержка войны",
            "ЗАГЛУШКА · Поддержка войны ещё не рассчитывается.",
            false,
        ),
        (
            Icon::Manpower,
            manpower,
            "Людские ресурсы",
            manpower_body.as_str(),
            true,
        ),
        (
            Icon::Factory,
            factories.to_string(),
            "Промышленность",
            "Фабрики, военные заводы и верфи из состояния страны.",
            true,
        ),
        (
            Icon::Fuel,
            support.map_or("—".into(), |c| number(c.fuel.max(0.) as u64)),
            "Топливо",
            fuel_body.as_str(),
            support.is_some(),
        ),
        (
            Icon::Convoy,
            support.map_or("—".into(), |c| number(c.convoys)),
            "Конвои",
            "Парк конвоев ограничивает морское снабжение через порты. Морское производство и бои флотов пока не моделируются. Нажмите для управления.",
            support.is_some(),
        ),
        (
            Icon::Command,
            tag.and_then(|t| w.countries.get(t))
                .and_then(|c| c.command_power)
                .map_or_else(|| "—".into(), |p| format!("{p:.1}")),
            "Командный ресурс",
            "Command Power из состояния страны. Расходуется на доступные способности командующих.",
            tag.and_then(|t| w.countries.get(t))
                .and_then(|c| c.command_power)
                .is_some(),
        ),
        (
            Icon::Army,
            xp,
            "Опыт армии",
            "Доступный опыт армии. Используется при изменении шаблонов.",
            true,
        ),
        (
            Icon::Navy,
            "—".into(),
            "Опыт флота",
            "ЗАГЛУШКА · Морская система ещё не реализована.",
            false,
        ),
        (
            Icon::Air,
            "—".into(),
            "Опыт авиации",
            "Опыт авиации пока не начисляется. Региональные воздушные миссии доступны через кнопку авиации.",
            false,
        ),
        (
            Icon::Supply,
            supply_label,
            "Снабжение",
            "Обеспеченность действующих дивизий с учётом доставки и переносимых запасов. Нажмите для просмотра сети, моторизации и топлива.",
            w.logistics.enabled,
        ),
        (
            Icon::Nuclear,
            "—".into(),
            "Ядерное оружие",
            "ЗАГЛУШКА · Разработка и запасы ядерного оружия ещё не рассчитываются.",
            false,
        ),
    ];
    let (start, y, space, count) = if l.compact {
        (
            6.0,
            38.0,
            l.width - 12.0,
            ((l.width - 12.0) / 66.0).floor().clamp(5.0, 8.0) as usize,
        )
    } else {
        (60.0, 0.0, l.time().x - 64.0, metrics.len())
    };
    let mw = space / count as f32;
    for (i, (icon, v, title, body, live)) in metrics.iter().take(count).enumerate() {
        value(
            *icon,
            Rect::new(start + i as f32 * mw, y, mw, 30.0),
            v,
            title,
            body,
            *live,
        );
    }
    if is_mouse_button_pressed(MouseButton::Left) {
        for (i, (icon, _, _, _, _)) in metrics.iter().take(count).enumerate() {
            if matches!(icon, Icon::Fuel | Icon::Convoy | Icon::Supply)
                && ui::inside(Rect::new(start + i as f32 * mw, y, mw, 30.))
            {
                actions.push(Action::Open(Panel::Air));
            }
        }
    }
    let nav_y = if l.compact { 70.0 } else { 35.0 * l.scale };
    let nav_h = l.top - nav_y - 4.0;
    let nav_start = if l.compact { 4.0 } else { 7.0 };
    let nav_w = if l.compact {
        (l.width - 8.0) / 12.0
    } else {
        55.0 * l.scale
    };
    for (i, panel) in Panel::NAV.iter().enumerate() {
        let r = Rect::new(nav_start + i as f32 * nav_w, nav_y, nav_w - 2.0, nav_h);
        let active = h.active == Some(*panel);
        draw_rectangle(
            r.x,
            r.y,
            r.w,
            r.h,
            if active {
                Color::from_rgba(92, 82, 57, 255)
            } else {
                Color::from_rgba(44, 49, 50, 255)
            },
        );
        draw_rectangle_lines(
            r.x,
            r.y,
            r.w,
            r.h,
            1.0,
            if active { ui::GOLD } else { ui::EDGE },
        );
        let size = (r.h - 12.0).min(23.0);
        ui_icons::draw(
            panel.icon(),
            Rect::new(r.x + (r.w - size) * 0.5, r.y + 3.0, size, size),
            if active { ui::GOLD } else { ui::TEXT },
        );
        if !l.compact {
            ui::label(
                panel.short(),
                Rect::new(r.x + 2.0, r.bottom() - 11.0, r.w - 4.0, 10.0),
                8.0,
                ui::MUTED,
            );
        }
        let key = panel.key();
        let title = if key.is_empty() {
            panel.title().to_owned()
        } else {
            format!("{} · {key}", panel.title())
        };
        ui::tooltip(
            r,
            &title,
            if panel.placeholder() {
                "Открыть окно. Не реализованные действия отмечены как заглушки."
            } else {
                "Открыть управление этим разделом."
            },
        );
        if ui::inside(r) && is_mouse_button_pressed(MouseButton::Left) {
            actions.push(Action::Open(*panel));
        }
    }
    if l.compact {
        if icon_button(
            Icon::Menu,
            Rect::new(64.0, 4.0, 31.0, 29.0),
            h.menu,
            "Меню игры",
            "",
        ) {
            actions.push(Action::Menu);
        }
    }
    if !l.compact {
        let y = nav_y;
        let x = l.width - 185.0 * l.scale;
        let size = nav_h;
        for (i, (icon, title, panel)) in [
            (Icon::Army, "Сухопутные войска · O", Panel::Army),
            (Icon::Navy, "Флот", Panel::Navy),
            (Icon::Air, "Авиация", Panel::Air),
        ]
        .iter()
        .enumerate()
        {
            if icon_button(
                *icon,
                Rect::new(x + i as f32 * (size + 3.0), y, size, size),
                h.active == Some(*panel),
                title,
                "",
            ) {
                actions.push(Action::Open(*panel));
            }
        }
        let r = Rect::new(l.width - 47.0, y, 39.0, nav_h);
        if icon_button(Icon::Menu, r, h.menu, "Главное меню · Esc", "") {
            actions.push(Action::Menu);
        }
    }
    // Real alerts. Unimplemented political/focus warnings are not invented.
    if !l.compact {
        let free = tag
            .and_then(|t| research.country(t))
            .map_or(0, |c| c.slots.iter().filter(|s| s.active.is_none()).count());
        let x = if h.active.is_some() {
            l.primary().right() + 8.0
        } else {
            12.0
        };
        if free > 0 && x + 150.0 < l.outliner().x {
            let r = Rect::new(x, l.top + 8.0, 39.0, 31.0);
            if icon_button(
                Icon::Research,
                r,
                false,
                "Свободные исследовательские ячейки",
                &format!("Свободно: {free}. Нажмите, чтобы выбрать исследование."),
            ) {
                actions.push(Action::Open(Panel::Research));
            }
            crate::draw_ui_text(
                &free.to_string(),
                r.right() - 12.0,
                r.bottom() - 4.0,
                11.0,
                ui::GOLD,
            );
        }
        if let Some(c) = tag.and_then(|t| research.country(t)) {
            if let Some(id) = c.completed.last() {
                let r = Rect::new(x + 88.0, l.top + 8.0, 39.0, 31.0);
                if x + 150.0 < l.outliner().x
                    && icon_button(
                        Icon::Research,
                        r,
                        true,
                        "Исследование завершено",
                        &format!("{id}. Нажмите, чтобы открыть исследования."),
                    )
                {
                    actions.push(Action::Open(Panel::Research));
                }
            }
        }
        let used = tag.and_then(|t| prod.country(t)).map_or(0, |c| {
            c.lines.iter().map(|l| l.requested_factories).sum::<u32>()
        });
        if used < economy.military_factories && x + 150.0 < l.outliner().x {
            let r = Rect::new(x + 44.0, l.top + 8.0, 39.0, 31.0);
            if icon_button(
                Icon::Factory,
                r,
                false,
                "Свободные военные заводы",
                "Нажмите, чтобы назначить заводы производственным линиям.",
            ) {
                actions.push(Action::Open(Panel::Production));
            }
        }
        let globe = Rect::new(l.width - 93.0, l.top + 4.0, 53.0, 29.0);
        ui_icons::draw(
            Icon::Globe,
            Rect::new(globe.x, globe.y + 3.0, 23.0, 23.0),
            ui::MUTED,
        );
        crate::draw_ui_text("—", globe.x + 28.0, globe.y + 21.0, 13.0, ui::MUTED);
        ui::tooltip(
            globe,
            "Мировая напряжённость",
            "ЗАГЛУШКА · Мировая напряжённость ещё не рассчитывается.",
        );
    }
    let toggle = Rect::new(l.width - 33.0, l.top + 5.0, 28.0, 28.0);
    if icon_button(
        Icon::Army,
        toggle,
        h.outliner,
        "Список войск",
        "Показать или скрыть список дивизий.",
    ) {
        h.outliner = !h.outliner;
    }
    if h.outliner && !l.compact {
        draw_outliner(h, tag, w, &mut actions);
    }
    draw_dock(h, tag, w, selected, &mut actions);
    draw_modes(h, mode, &mut actions);
    actions
}

fn draw_outliner(h: &mut Hud, tag: Option<&str>, w: &Warfare, actions: &mut Vec<Action>) {
    let r = Layout::current().outliner();
    ui::panel(r.x, r.y, r.w, r.h);
    let units: Vec<_> = w
        .divisions
        .values()
        .filter(|d| Some(d.country.as_str()) == tag && !d.destroyed)
        .collect();
    ui::section(
        &format!("ДИВИЗИИ · {}", units.len()),
        r.x + 3.0,
        r.y + 3.0,
        r.w - 6.0,
    );
    let list = Rect::new(r.x + 4.0, r.y + 31.0, r.w - 12.0, r.h - 64.0);
    let rows = (list.h / 34.0).max(0.0) as usize;
    let max = units.len().saturating_sub(rows.max(1));
    h.outliner_scroll = h.outliner_scroll.min(max);
    if ui::inside(list) {
        let (_, dy) = mouse_wheel();
        if dy > 0.0 {
            h.outliner_scroll = h.outliner_scroll.saturating_sub(2);
        } else if dy < 0.0 {
            h.outliner_scroll = (h.outliner_scroll + 2).min(max);
        }
    }
    ui::scrollbar(list, &mut h.outliner_scroll, max, rows);
    for (i, d) in units.iter().skip(h.outliner_scroll).take(rows).enumerate() {
        let row = Rect::new(list.x, list.y + i as f32 * 34.0, list.w, 32.0);
        ui::row_bg(row, false, i % 2 == 1);
        ui_icons::draw(
            Icon::Army,
            Rect::new(row.x + 4.0, row.y + 7.0, 17.0, 17.0),
            ui::GOLD,
        );
        ui::label(
            &d.name,
            Rect::new(row.x + 26.0, row.y + 3.0, row.w - 31.0, 16.0),
            10.0,
            ui::TEXT,
        );
        ui::progress_bar(
            Rect::new(row.x + 26.0, row.y + 23.0, row.w - 34.0, 4.0),
            (d.organization / d.derived.stats.organization.max(1.0)) as f32,
        );
        ui::tooltip(
            row,
            &d.name,
            &format!("Провинция {} · Нажмите для выбора дивизии.", d.province),
        );
        if ui::inside(row) && is_mouse_button_pressed(MouseButton::Left) {
            actions.push(Action::Select(d.id));
        }
    }
    if units.is_empty() {
        ui::label(
            "Дивизии не загружены",
            Rect::new(list.x + 7.0, list.y + 13.0, list.w - 14.0, 20.0),
            11.0,
            ui::MUTED,
        );
    }
    if ui::small_button(
        "Все войска",
        Rect::new(r.x + 6.0, r.bottom() - 28.0, r.w - 12.0, 22.0),
        false,
    ) {
        actions.push(Action::Open(Panel::Army));
    }
}
fn draw_dock(
    h: &mut Hud,
    tag: Option<&str>,
    w: &Warfare,
    selection: &ArmyView,
    actions: &mut Vec<Action>,
) {
    let r = Layout::current().army_dock();
    ui::panel(r.x, r.y, r.w, r.h);
    let compact = Layout::current().compact;
    let all = Rect::new(
        r.x + 3.0,
        r.y + 4.0,
        if compact { 34.0 } else { 80.0 },
        r.h - 8.0,
    );
    let n = w
        .divisions
        .values()
        .filter(|d| Some(d.country.as_str()) == tag && !d.destroyed)
        .count();
    if ui::small_button(
        &if compact {
            n.to_string()
        } else {
            format!("Все: {n}")
        },
        all,
        false,
    ) {
        actions.push(Action::SelectAll);
    }
    ui::tooltip(
        all,
        "Все сухопутные дивизии",
        "Выделить все свои соединения. ПКМ по карте — ручной приказ.",
    );
    let add = Rect::new(
        all.right() + 3.0,
        r.y + 4.0,
        if compact { 28.0 } else { 38.0 },
        r.h - 8.0,
    );
    if ui::small_button("+", add, false) {
        actions.push(Action::CreateArmy);
    }
    ui::tooltip(
        add,
        "Создать армию",
        "Сначала выделите дивизии. Они будут объединены в новую армию.",
    );
    let armies: Vec<_> = w
        .command
        .armies
        .values()
        .filter(|a| Some(a.country.as_str()) == tag)
        .collect();
    let available = (r.right() - add.right() - 10.0).max(0.0);
    let cw = if compact { 60.0 } else { 135.0 };
    let capacity = (available / cw).floor() as usize;
    h.army_dock_scroll = h
        .army_dock_scroll
        .min(armies.len().saturating_sub(capacity.max(1)));
    if ui::inside(r) {
        let dy = mouse_wheel().1;
        if dy > 0.0 {
            h.army_dock_scroll = h.army_dock_scroll.saturating_sub(1);
        } else if dy < 0.0 {
            h.army_dock_scroll =
                (h.army_dock_scroll + 1).min(armies.len().saturating_sub(capacity.max(1)));
        }
    }
    for (slot, a) in armies
        .iter()
        .skip(h.army_dock_scroll)
        .take(capacity)
        .enumerate()
    {
        let b = Rect::new(
            add.right() + 5.0 + slot as f32 * cw,
            r.y + 4.0,
            cw - 4.0,
            r.h - 8.0,
        );
        let selected = selection.selected_army == Some(a.id);
        ui::row_bg(b, selected, false);
        let color = Color::new(
            a.color[0] as f32 / 255.0,
            a.color[1] as f32 / 255.0,
            a.color[2] as f32 / 255.0,
            1.0,
        );
        draw_rectangle(b.x, b.bottom() - 3.0, b.w, 3.0, color);
        let portrait = Rect::new(b.x + 3.0, b.y + 2.0, 26.0, b.h - 5.0);
        if !a
            .general
            .as_ref()
            .is_some_and(|id| crate::hoi_assets::portrait(id, portrait))
        {
            ui_icons::draw(
                Icon::Officer,
                portrait,
                if a.general.is_some() {
                    ui::GOLD
                } else {
                    ui::MUTED
                },
            );
        }
        let general = a
            .general
            .as_ref()
            .and_then(|id| w.command.commanders.get(id))
            .map(|c| c.name.as_str())
            .unwrap_or("Назначить генерала");
        ui::tooltip(
            portrait,
            general,
            "ЛКМ: выбрать армию. Генерал назначается через портрет в левой панели. ПКМ по карточке: перевести выделенные дивизии.",
        );
        ui::label(
            &if compact {
                w.command.members(a.id).count().to_string()
            } else {
                format!("{} · {}", a.name, w.command.members(a.id).count())
            },
            Rect::new(
                b.x + 32.0,
                b.y + if compact { 29.0 } else { 5.0 },
                b.w - if compact { 35.0 } else { 62.0 },
                20.0,
            ),
            11.0,
            ui::TEXT,
        );
        ui::tooltip(
            b,
            &a.name,
            &format!("{} дивизий · {general}", w.command.members(a.id).count()),
        );
        let active =
            w.plans.plans.values().any(|p| {
                p.army == a.id && p.activation == crate::operations::Activation::Executing
            });
        let play = Rect::new(b.right() - 24.0, b.y + 3.0, 21.0, 22.0);
        if ui::small_button(if active { "■" } else { "▶" }, play, active) {
            actions.push(Action::ToggleArmy(a.id));
        }
        ui::tooltip(
            play,
            if active {
                "Остановить планы армии"
            } else {
                "Выполнить планы армии"
            },
            "Отдельное управление исполнением этой армии. Время идёт после снятия паузы.",
        );
        if !compact {
            ui::label(
                if active {
                    "ПЛАНЫ ВКЛЮЧЕНЫ"
                } else {
                    "ПЛАНЫ ОСТАНОВЛЕНЫ"
                },
                Rect::new(b.x + 32.0, b.y + 29.0, b.w - 35.0, 18.0),
                9.0,
                if active { ui::GREEN } else { ui::MUTED },
            );
        }
        let mouse = vec2(mouse_position().0, mouse_position().1);
        if ui::inside(b) && !play.contains(mouse) && is_mouse_button_pressed(MouseButton::Left) {
            actions.push(Action::SelectArmy(
                a.id,
                is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift),
            ));
        }
        if ui::inside(b) && is_mouse_button_pressed(MouseButton::Right) {
            actions.push(Action::AssignArmy(a.id));
        }
    }
    if capacity == 0
        && !armies.is_empty()
        && ui::small_button(
            "Армии",
            Rect::new(
                add.right() + 3.0,
                r.y + 4.0,
                (r.right() - add.right() - 6.0).max(1.0),
                r.h - 8.0,
            ),
            false,
        )
    {
        actions.push(Action::Open(Panel::Army));
    }
}
fn draw_modes(h: &mut Hud, mode: usize, actions: &mut Vec<Action>) {
    let r = Layout::current().map_modes();
    if Layout::current().compact {
        if ui::small_button("Карта", r, h.map_palette) {
            h.map_palette = !h.map_palette;
        }
        ui::tooltip(r, "Режим карты", "Политика, местность, ресурсы и масштаб");
        return;
    }
    ui::panel(r.x, r.y, r.w, r.h);
    let bw = (r.w - 10.0) / 9.0;
    for (i, (icon, title)) in [
        (Icon::Politics, "Политическая карта · F1"),
        (Icon::Resources, "Ресурсы · F8"),
        (Icon::Terrain, "Местность · F5"),
        (Icon::Supply, "Снабжение · F4"),
        (Icon::Navy, "Морские регионы · F2"),
        (Icon::Air, "Воздушные регионы · F3"),
    ]
    .iter()
    .enumerate()
    {
        let b = Rect::new(r.x + 4.0 + i as f32 * bw, r.y + 5.0, bw - 2.0, 29.0);
        if i < 3 {
            if icon_button(*icon, b, mode == i, title, "") {
                actions.push(Action::MapMode(i));
            }
        } else {
            ui_icons::draw(
                *icon,
                Rect::new(b.x + 5.0, b.y + 4.0, b.w - 10.0, b.h - 8.0),
                ui::MUTED,
            );
            ui::tooltip(b, title, "ЗАГЛУШКА · Этот режим карты ещё не реализован.");
        }
    }
    for (i, (label, action)) in [
        ("−", Action::Zoom(0.8)),
        ("+", Action::Zoom(1.25)),
        ("⋮", Action::Fit),
    ]
    .into_iter()
    .enumerate()
    {
        if ui::small_button(
            label,
            Rect::new(r.x + 4.0 + (i + 6) as f32 * bw, r.y + 5.0, bw - 2.0, 29.0),
            false,
        ) {
            if i == 2 {
                h.map_palette = !h.map_palette;
            } else {
                actions.push(action);
            }
        }
    }
}

pub fn draw_panel(
    h: &mut Hud,
    world: &World,
    tag: Option<&str>,
    w: &Warfare,
    prod: &ProductionState,
    resources: &mut ResourceEconomy,
    selected_province: Option<u32>,
) -> Vec<Action> {
    let Some(panel) = h.active else {
        return Vec::new();
    };
    if matches!(
        panel,
        Panel::Construction
            | Panel::Production
            | Panel::Research
            | Panel::Army
            | Panel::Recruitment
            | Panel::Officer
            | Panel::Air
    ) {
        return Vec::new();
    }
    let l = Layout::current();
    let r = if panel == Panel::Focus || panel == Panel::Navy {
        l.wide()
    } else {
        l.primary()
    };
    ui::panel(r.x, r.y, r.w, r.h);
    ui::header(r.x + 3.0, r.y + 3.0, r.w - 6.0, panel.title());
    if ui::small_button(
        "×",
        Rect::new(r.right() - 35.0, r.y + 7.0, 27.0, 25.0),
        false,
    ) {
        h.active = None;
        return Vec::new();
    }
    let mut actions = Vec::new();
    let Some(tag) = tag else {
        return actions;
    };
    let x = r.x + 11.0;
    let width = r.w - 22.0;
    let y = r.y + 44.0;
    match panel {
        Panel::Politics => politics(h, tag, x, y, width, r.bottom() - 8.0, &mut actions),
        Panel::Diplomacy => diplomacy(
            h,
            world,
            tag,
            w,
            selected_province,
            x,
            y,
            width,
            r.bottom() - 8.0,
            &mut actions,
        ),
        Panel::Trade => trade(h, world, tag, w, resources, x, y, width, r.bottom() - 8.0),
        Panel::Logistics => logistics(h, tag, w, prod, x, y, width, r.bottom() - 8.0),
        Panel::Navy => navy(
            h,
            world,
            tag,
            w,
            x,
            y,
            width,
            r.bottom() - 8.0,
            &mut actions,
        ),
        Panel::Focus => {} // The imported source-aware tree is drawn by focus_ui.
        Panel::Decisions => {} // Real pending events are drawn by campaign_events_ui.
        _ => system_shell(h, panel, tag, x, y, width, r.bottom() - 8.0),
    }
    actions
}

pub fn draw_map_palette(h: &mut Hud, mode: usize) -> Vec<Action> {
    let mut actions = Vec::new();
    if !h.map_palette {
        return actions;
    }
    let l = Layout::current();
    let r = Rect::new(l.width - 320.0, l.height - 341.0, 315.0, 288.0);
    ui::panel(r.x, r.y, r.w, r.h);
    ui::header(r.x + 3.0, r.y + 3.0, r.w - 6.0, "РЕЖИМЫ КАРТЫ");
    if ui::small_button(
        "×",
        Rect::new(r.right() - 33.0, r.y + 7.0, 25.0, 24.0),
        false,
    ) {
        h.map_palette = false;
    }
    let items = [
        ("Политическая · F1", Some(0)),
        ("Морская · F2", None),
        ("Воздушная · F3", None),
        ("Снабжение · F4", None),
        ("Местность · F5", Some(2)),
        ("Сопротивление · F6", None),
        ("Подчинение · F7", None),
        ("Ресурсы · F8", Some(1)),
        ("Инфраструктура · F9", None),
        ("Альянсы · F10", None),
    ];
    for (i, (title, index)) in items.iter().enumerate() {
        let b = Rect::new(
            r.x + 8.0 + (i % 2) as f32 * 151.0,
            r.y + 45.0 + (i / 2) as f32 * 37.0,
            146.0,
            31.0,
        );
        if let Some(index) = index {
            if ui::small_button(title, b, mode == *index) {
                actions.push(Action::MapMode(*index));
                h.map_palette = false;
            }
        } else {
            ui::disabled(title, b, "ЗАГЛУШКА · Этот режим карты ещё не реализован.");
            if ui::inside(b) && is_mouse_button_pressed(MouseButton::Left) {
                h.detail = Some((*title).into());
                h.map_palette = false;
            }
        }
    }
    if ui::small_button(
        "Показать всю карту · Home",
        Rect::new(r.x + 8.0, r.bottom() - 42.0, r.w - 16.0, 30.0),
        false,
    ) {
        actions.push(Action::Fit);
        h.map_palette = false;
    }
    actions
}
fn status(text: &str, x: f32, y: f32, width: f32) {
    ui::label(text, Rect::new(x, y, width, 17.0), 11.0, ui::GOLD);
}
fn politics(
    h: &mut Hud,
    tag: &str,
    x: f32,
    y: f32,
    width: f32,
    bottom: f32,
    actions: &mut Vec<Action>,
) {
    ui::flag(tag, Rect::new(x + 5.0, y + 6.0, 74.0, 46.0));
    ui::label(
        crate::country_display_name(tag),
        Rect::new(x + 95.0, y + 5.0, width - 100.0, 25.0),
        21.0,
        ui::TEXT,
    );
    status(
        "Политическая система · ЗАГЛУШКА",
        x + 95.0,
        y + 34.0,
        width - 100.0,
    );
    let portrait = Rect::new(x + 5.0, y + 67.0, 105.0, 127.0);
    ui::row_bg(portrait, false, false);
    if !crate::hoi_assets::country_portrait(
        tag,
        Rect::new(
            portrait.x + 6.,
            portrait.y + 5.,
            portrait.w - 12.,
            portrait.h - 27.,
        ),
    ) {
        ui_icons::draw(
            Icon::Officer,
            Rect::new(portrait.x + 18., portrait.y + 16., 70., 82.),
            ui::MUTED,
        );
    }
    ui::label(
        "Лидер страны",
        Rect::new(
            portrait.x + 5.0,
            portrait.bottom() - 23.0,
            portrait.w - 10.0,
            18.0,
        ),
        11.0,
        ui::MUTED,
    );
    ui::tooltip(
        portrait,
        "Исторический портрет",
        "Берётся из местной HOI4: первый гражданский персонаж, завербованный историей. Действующий лидер меняется политикой; динамическое назначение пока не реализовано.",
    );
    let tx = x + 124.0;
    let tw = width - 128.0;
    stub_button(h, "Правящая партия —", Rect::new(tx, y + 67.0, tw, 30.0));
    stub_button(h, "Идеология —", Rect::new(tx, y + 101.0, tw, 27.0));
    stub_button(h, "Выборы —", Rect::new(tx, y + 132.0, tw, 27.0));
    let cy = y + 173.0;
    for (i, label) in ["Демократия", "Фашизм", "Коммунизм", "Нейтралитет"]
        .iter()
        .enumerate()
    {
        let yy = cy + i as f32 * 16.0;
        if yy < bottom {
            ui::label(
                &format!("{label}  —"),
                Rect::new(tx, yy, tw, 17.0),
                11.0,
                ui::MUTED,
            );
        }
    }
    let focus = Rect::new(x, y + 247.0, width, 48.0);
    if focus.bottom() < bottom {
        if ui::small_button("Выбрать национальный фокус", focus, false) {
            actions.push(Action::Open(Panel::Focus));
        }
        ui::tooltip(
            focus,
            "Национальный фокус",
            "ЗАГЛУШКА · Открывает каркас дерева фокусов. Прогресс не начисляется.",
        );
    }
    let mut yy = y + 303.0;
    let sections = [
        (
            "НАЦИОНАЛЬНЫЕ ДУХИ",
            ["Национальный дух", "Национальный дух", "Национальный дух"],
        ),
        (
            "ЗАКОНЫ И ПРАВИТЕЛЬСТВО",
            ["Призыв", "Торговля", "Экономика"],
        ),
        (
            "ПОЛИТИЧЕСКИЕ СОВЕТНИКИ",
            ["Советник", "Советник", "Советник"],
        ),
        (
            "ИССЛЕДОВАНИЯ И ПРОИЗВОДСТВО",
            ["Конструктор танков", "Конструктор флота", "Промышленность"],
        ),
        ("ВОЕННОЕ РУКОВОДСТВО", ["Армия", "Флот", "Авиация"]),
    ];
    let visible = ((bottom - yy - 19.0) / 66.0).max(1.0) as usize;
    let max = sections.len().saturating_sub(visible);
    h.scroll = h.scroll.min(max);
    scroll(h, Rect::new(x, yy, width, (bottom - yy).max(1.0)), max);
    for (heading, slots) in sections.iter().skip(h.scroll).take(visible) {
        if yy + 64.0 > bottom {
            break;
        }
        ui::section(heading, x, yy, width);
        yy += 27.0;
        let bw = (width - 8.0) / 3.0;
        for (i, s) in slots.iter().enumerate() {
            stub_button(h, s, Rect::new(x + i as f32 * (bw + 4.0), yy, bw, 31.0));
        }
        yy += 39.0;
    }
    if max > 0 {
        if ui::small_button(
            "▲",
            Rect::new(x + width - 56.0, bottom - 21.0, 25.0, 20.0),
            false,
        ) {
            h.scroll = h.scroll.saturating_sub(1);
        }
        if ui::small_button(
            "▼",
            Rect::new(x + width - 26.0, bottom - 21.0, 25.0, 20.0),
            false,
        ) {
            h.scroll = (h.scroll + 1).min(max);
        }
    }
}
fn diplomacy(
    h: &mut Hud,
    world: &World,
    tag: &str,
    w: &Warfare,
    province: Option<u32>,
    x: f32,
    y: f32,
    width: f32,
    bottom: f32,
    actions: &mut Vec<Action>,
) {
    if h.diplomacy_country.is_none() {
        h.diplomacy_country = province
            .and_then(|p| world.controller_of_province(p))
            .filter(|t| *t != tag)
            .map(str::to_owned)
            .or_else(|| world.countries.keys().find(|t| t.as_str() != tag).cloned());
    }
    let countries: Vec<_> = world.countries.keys().collect();
    let target = h.diplomacy_country.clone().unwrap_or_else(|| tag.into());
    if ui::small_button("◀", Rect::new(x, y, 27.0, 29.0), false) {
        let n = countries
            .iter()
            .position(|t| t.as_str() == target)
            .unwrap_or(0);
        if !countries.is_empty() {
            h.diplomacy_country =
                Some(countries[(n + countries.len() - 1) % countries.len()].to_string());
        }
    }
    if ui::small_button("▶", Rect::new(x + width - 27.0, y, 27.0, 29.0), false) {
        let n = countries
            .iter()
            .position(|t| t.as_str() == target)
            .unwrap_or(0);
        if !countries.is_empty() {
            h.diplomacy_country = Some(countries[(n + 1) % countries.len()].to_string());
        }
    }
    ui::flag(&target, Rect::new(x + 37.0, y + 3.0, 56.0, 35.0));
    ui::label(
        crate::country_display_name(&target),
        Rect::new(x + 106.0, y + 5.0, width - 142.0, 26.0),
        18.0,
        ui::TEXT,
    );
    ui::section("ОТНОШЕНИЯ", x, y + 49.0, width);
    ui::label(
        if w.at_war(tag, &target) {
            "В состоянии войны"
        } else if tag == target {
            "Ваша страна"
        } else {
            "В состоянии мира"
        },
        Rect::new(x + 9.0, y + 80.0, width - 18.0, 23.0),
        15.0,
        ui::GOLD,
    );
    status(
        "Отношение: —     Альянс: —     Разведданные: —",
        x + 9.0,
        y + 109.0,
        width - 18.0,
    );
    ui::section("ДИПЛОМАТИЧЕСКИЕ ДЕЙСТВИЯ", x, y + 140.0, width);
    let mut yy = y + 169.0;
    if tag != target && !w.at_war(tag, &target) {
        if ui::small_button("Объявить войну", Rect::new(x, yy, width, 29.0), false) {
            h.detail = Some(format!("Объявить войну: {target}"));
        }
        ui::tooltip(
            Rect::new(x, yy, width, 29.0),
            "Объявить войну",
            "Работает в текущем прототипе. Система обоснований и дипломатических ограничений ещё отсутствует.",
        );
        yy += 34.0;
    }
    let items = [
        "Обосновать цель войны",
        "Предложить мир",
        "Гарантировать независимость",
        "Улучшить отношения",
        "Предложить военный доступ",
        "Запросить военный доступ",
        "Пакт о ненападении",
        "Пригласить в альянс",
        "Отправить добровольцев",
        "Ленд-лиз",
    ];
    let count = ((bottom - yy - 23.0) / 34.0).max(1.0) as usize;
    let max = items.len().saturating_sub(count);
    h.scroll = h.scroll.min(max);
    scroll(h, Rect::new(x, yy, width, (bottom - yy).max(1.0)), max);
    for title in items.iter().skip(h.scroll).take(count) {
        if yy + 29.0 > bottom - 23.0 {
            break;
        }
        stub_button(h, title, Rect::new(x, yy, width, 29.0));
        yy += 34.0;
    }
    if max > 0 {
        if ui::small_button(
            "▲",
            Rect::new(x + width - 56.0, bottom - 21.0, 25.0, 20.0),
            false,
        ) {
            h.scroll = h.scroll.saturating_sub(1);
        }
        if ui::small_button(
            "▼",
            Rect::new(x + width - 26.0, bottom - 21.0, 25.0, 20.0),
            false,
        ) {
            h.scroll = (h.scroll + 1).min(max);
        }
    }
    let _ = actions;
}
fn trade(
    h: &mut Hud,
    world: &World,
    tag: &str,
    warfare: &Warfare,
    market: &mut ResourceEconomy,
    x: f32,
    y: f32,
    width: f32,
    bottom: f32,
) {
    const MATERIALS: [(&str, &str); 7] = [
        ("oil", "Нефть"),
        ("steel", "Сталь"),
        ("aluminium", "Алюминий"),
        ("rubber", "Резина"),
        ("tungsten", "Вольфрам"),
        ("chromium", "Хром"),
        ("coal", "Уголь"),
    ];
    let law = market.laws.get(tag).copied().unwrap_or_default();
    let law_text = match law {
        TradeLaw::Unset => "Не импортирован (экспорт 0%)",
        TradeLaw::FreeTrade => "Свободная торговля (80%)",
        TradeLaw::ExportFocus => "Приоритет экспорта (50%)",
        TradeLaw::LimitedExports => "Ограниченный экспорт (25%)",
        TradeLaw::ClosedEconomy => "Закрытая экономика (0%)",
    };
    ui::label(
        "Ресурсы: добыча / внутренний остаток / импорт",
        Rect::new(x, y, (width - 176.0).max(85.0), 23.0),
        12.0,
        ui::TEXT,
    );
    let automatic = market.automatic_buyers.contains(tag);
    if ui::small_button(
        if automatic {
            "АВТОИМПОРТ: ВКЛ"
        } else {
            "АВТОИМПОРТ: ВЫКЛ"
        },
        Rect::new(x + width - 174.0, y, 174.0, 25.0),
        automatic,
    ) {
        if automatic {
            market.automatic_buyers.remove(tag);
            market.automatic_deals.retain(|d| d.buyer != tag);
            h.notice = "Автоимпорт выключен; ручные договоры сохранены".into();
        } else {
            market.automatic_buyers.insert(tag.into());
            h.notice = "Автоимпорт включён: дефицит производственных линий будет покрываться при наличии фабрик, ресурсов и маршрутов".into();
        }
    }
    let report = market.last.get(tag);
    let mut yy = y + 29.0;
    for (key, label) in MATERIALS {
        if yy + 25.0 > bottom {
            return;
        }
        ui::row_bg(Rect::new(x, yy, width, 25.0), false, false);
        let mined = report
            .and_then(|r| r.extracted.get(key))
            .copied()
            .unwrap_or(0);
        let retained = report
            .and_then(|r| r.available.get(key))
            .copied()
            .unwrap_or(0);
        let imported = report
            .and_then(|r| r.imported.get(key))
            .copied()
            .unwrap_or(0);
        ui::label(
            &format!("{label}  {mined} / {retained} / +{imported}"),
            Rect::new(x + 8.0, yy + 3.0, width - 16.0, 20.0),
            12.0,
            ui::TEXT,
        );
        yy += 27.0;
    }
    let factories_spent = report.map_or(0, |r| r.factories_spent);
    let factories_received = report.map_or(0, |r| r.factories_received);
    if yy + 50.0 > bottom {
        return;
    }
    ui::label(
        &format!(
            "Фабрики: импорт −{factories_spent} / экспорт +{factories_received} · авто: {}",
            market
                .automatic_deals
                .iter()
                .filter(|d| d.buyer == tag)
                .map(|d| d.factories)
                .sum::<u32>()
        ),
        Rect::new(x, yy + 4.0, width, 21.0),
        12.0,
        ui::GOLD,
    );
    yy += 27.0;
    if yy + 29.0 > bottom {
        return;
    }
    let total_convoys = warfare
        .logistics
        .countries
        .get(tag)
        .map_or(0, |c| c.convoys);
    let used_convoys = report.map_or(0, |r| r.trade_convoys_used);
    let disrupted = market
        .last_shipments
        .iter()
        .filter(|s| s.buyer == tag && s.delivered < s.contracted)
        .count();
    ui::label(
        &format!(
            "Торговые конвои: {used_convoys}/{total_convoys}   Неполные поставки: {disrupted}"
        ),
        Rect::new(x, yy + 2.0, width, 23.0),
        12.0,
        if disrupted > 0 { ui::GOLD } else { ui::TEXT },
    );
    yy += 29.0;
    if ui::small_button(
        &format!("Закон: {law_text}   [сменить]"),
        Rect::new(x, yy, width, 29.0),
        false,
    ) {
        let next = match law {
            TradeLaw::Unset | TradeLaw::ClosedEconomy => TradeLaw::FreeTrade,
            TradeLaw::FreeTrade => TradeLaw::ExportFocus,
            TradeLaw::ExportFocus => TradeLaw::LimitedExports,
            TradeLaw::LimitedExports => TradeLaw::ClosedEconomy,
        };
        market.laws.insert(tag.into(), next);
        h.notice = "Новый торговый закон применяется со следующих игровых суток".into();
    }
    yy += 37.0;
    if yy + 86.0 > bottom {
        return;
    }
    ui::section("ИМПОРТ — ЦЕНА 1 ФАБРИКА ЗА ДО 8 ЕДИНИЦ", x, yy, width);
    yy += 28.0;
    let sellers: Vec<_> = world
        .countries
        .keys()
        .filter(|country| country.as_str() != tag)
        .collect();
    if sellers.is_empty() {
        return;
    }
    h.trade_partner_index %= sellers.len();
    h.trade_resource_index %= MATERIALS.len();
    let seller = sellers[h.trade_partner_index];
    let (resource, label) = MATERIALS[h.trade_resource_index];
    let button = (width * 0.16).max(32.0);
    if ui::small_button("◀", Rect::new(x, yy, button, 27.0), false) {
        h.trade_partner_index = (h.trade_partner_index + sellers.len() - 1) % sellers.len();
    }
    ui::label(
        seller,
        Rect::new(x + button + 4.0, yy + 3.0, width - 2.0 * button - 8.0, 20.0),
        13.0,
        ui::TEXT,
    );
    if ui::small_button("▶", Rect::new(x + width - button, yy, button, 27.0), false) {
        h.trade_partner_index = (h.trade_partner_index + 1) % sellers.len();
    }
    yy += 31.0;
    if ui::small_button("◀", Rect::new(x, yy, button, 27.0), false) {
        h.trade_resource_index = (h.trade_resource_index + MATERIALS.len() - 1) % MATERIALS.len();
    }
    ui::label(
        label,
        Rect::new(x + button + 4.0, yy + 3.0, width - 2.0 * button - 8.0, 20.0),
        13.0,
        ui::TEXT,
    );
    if ui::small_button("▶", Rect::new(x + width - button, yy, button, 27.0), false) {
        h.trade_resource_index = (h.trade_resource_index + 1) % MATERIALS.len();
    }
    yy += 33.0;
    if yy + 65.0 > bottom {
        return;
    }
    if ui::small_button("−", Rect::new(x, yy, button, 28.0), false) {
        h.trade_factories = h.trade_factories.saturating_sub(1).max(1);
    }
    ui::label(
        &format!("Фабрики на импорт: {}", h.trade_factories),
        Rect::new(x + button + 4.0, yy + 3.0, width - 2.0 * button - 8.0, 20.0),
        12.0,
        ui::TEXT,
    );
    if ui::small_button("+", Rect::new(x + width - button, yy, button, 28.0), false) {
        h.trade_factories = (h.trade_factories + 1).min(99);
    }
    yy += 33.0;
    let deal = TradeDeal {
        buyer: tag.into(),
        seller: seller.clone(),
        resource: resource.into(),
        factories: h.trade_factories,
    };
    if ui::small_button(
        "Купить / изменить",
        Rect::new(x, yy, width * 0.62 - 3.0, 30.0),
        false,
    ) {
        h.notice = market.set_deal(world, deal).map_or_else(
            |e| e,
            |_| "Торговый договор вступит в силу со следующих игровых суток".into(),
        );
    }
    if ui::small_button(
        "Отменить",
        Rect::new(x + width * 0.62, yy, width * 0.38, 30.0),
        false,
    ) {
        h.notice = if market.cancel_deal(tag, seller, resource) {
            "Договор отменён".into()
        } else {
            "Такого договора нет".into()
        };
    }
    // Convoys, embargoes, subject price bonuses and political-power costs not yet supported.
}

fn naval_designer_view(
    h: &mut Hud,
    world: &World,
    tag: &str,
    w: &Warfare,
    x: f32,
    mut y: f32,
    width: f32,
    bottom: f32,
    actions: &mut Vec<Action>,
) {
    let Some(country) = w.countries.get(tag) else {
        return;
    };
    let schemas = &w.catalogue.naval_designer;
    let hulls: Vec<_> = w
        .catalogue
        .equipment
        .values()
        .filter(|e| {
            schemas.hulls.contains_key(&e.id)
                && e.buildable
                && (e.active || country.modifiers.unlocked_equipment.contains(&e.id))
        })
        .collect();
    ui::section("КОНСТРУКТОР КОРАБЛЕЙ — КОРПУС, СЛОТЫ, МОДУЛИ", x, y, width);
    y += 30.0;
    if hulls.is_empty() {
        ui::label(
            "Исследованных корпусов с данными слотов нет",
            Rect::new(x, y, width, 25.0),
            12.0,
            ui::GOLD,
        );
        return;
    }
    h.naval_hull_index %= hulls.len();
    let hull = hulls[h.naval_hull_index];
    let arrow = 38.0_f32.min(width / 5.0);
    if ui::small_button("◀", Rect::new(x, y, arrow, 28.0), false) {
        h.naval_hull_index = (h.naval_hull_index + hulls.len() - 1) % hulls.len();
        h.naval_slot_index = 0;
        h.naval_module_index = 0;
        h.naval_draft.clear();
    }
    ui::label(
        &format!("Корпус: {}", w.catalogue.label(&hull.id)),
        Rect::new(x + arrow + 5.0, y + 3.0, width - 2.0 * arrow - 10.0, 22.0),
        12.0,
        ui::TEXT,
    );
    if ui::small_button("▶", Rect::new(x + width - arrow, y, arrow, 28.0), false) {
        h.naval_hull_index = (h.naval_hull_index + 1) % hulls.len();
        h.naval_slot_index = 0;
        h.naval_module_index = 0;
        h.naval_draft.clear();
    }
    y += 40.0;
    let Ok(schema) = schemas.hull(&hull.id) else {
        return;
    };
    let slots: Vec<_> = schema.slots.iter().collect();
    if slots.is_empty() {
        ui::label(
            "У корпуса нет импортированных слотов",
            Rect::new(x, y, width, 24.0),
            12.0,
            ui::GOLD,
        );
        return;
    }
    h.naval_slot_index %= slots.len();
    let (slot, rule) = slots[h.naval_slot_index];
    ui::section("СЛОТЫ КОРПУСА", x, y, width);
    y += 27.0;
    if ui::small_button("◀", Rect::new(x, y, arrow, 27.0), false) {
        h.naval_slot_index = (h.naval_slot_index + slots.len() - 1) % slots.len();
        h.naval_module_index = 0;
    }
    ui::label(
        &format!(
            "{} {} ({}/{})",
            slot,
            if rule.required {
                "обязателен"
            } else {
                "свободный"
            },
            h.naval_slot_index + 1,
            slots.len()
        ),
        Rect::new(x + arrow + 5.0, y + 2.0, width - 2.0 * arrow - 10.0, 24.0),
        11.0,
        ui::TEXT,
    );
    if ui::small_button("▶", Rect::new(x + width - arrow, y, arrow, 27.0), false) {
        h.naval_slot_index = (h.naval_slot_index + 1) % slots.len();
        h.naval_module_index = 0;
    }
    y += 36.0;
    let mut modules = schemas.allowed_modules(&hull.id, slot);
    modules.retain(|id| {
        schemas.modules.get(id).is_some_and(|m| {
            m.active
                || schema.default_modules.get(slot) == Some(id)
                || country.modifiers.unlocked_modules.contains(id)
                || country.modifiers.unlocked_modules.contains(&m.category)
        })
    });
    if !rule.required {
        modules.insert(0, "empty".into());
    }
    if modules.is_empty() {
        ui::label(
            "Нет доступных модулей для этого слота",
            Rect::new(x, y, width, 23.0),
            11.0,
            ui::GOLD,
        );
    } else {
        let current = h
            .naval_draft
            .get(slot)
            .or_else(|| schema.default_modules.get(slot))
            .map(String::as_str)
            .unwrap_or("empty")
            .to_owned();
        let index = modules.iter().position(|m| m == &current).unwrap_or(0);
        if ui::small_button("◀", Rect::new(x, y, arrow, 27.0), false) {
            let new = (index + modules.len() - 1) % modules.len();
            h.naval_draft.insert(slot.clone(), modules[new].clone());
        }
        ui::label(
            &format!("Модуль: {current}"),
            Rect::new(x + arrow + 5.0, y + 3.0, width - 2.0 * arrow - 10.0, 22.0),
            11.0,
            ui::TEXT,
        );
        if ui::small_button("▶", Rect::new(x + width - arrow, y, arrow, 27.0), false) {
            let new = (index + 1) % modules.len();
            h.naval_draft.insert(slot.clone(), modules[new].clone());
        }
    }
    y += 36.0;
    // Display real computed stats / validation errors, not hardcoded prices.
    let preview = schemas.construct(
        tag,
        "Мой проект",
        &hull.id,
        &h.naval_draft,
        &w.catalogue,
        &country.modifiers,
        None,
    );
    match &preview {
        Ok(p) => ui::label(
            &format!(
                "Проект: {:.0} IC · {} слотов · палуба {:.0}",
                p.cost,
                p.modules.len(),
                p.stats.get("deck_size").copied().unwrap_or(0.)
            ),
            Rect::new(x, y, width, 22.0),
            12.0,
            ui::TEXT,
        ),
        Err(error) => ui::label(error, Rect::new(x, y, width, 22.0), 11.0, ui::GOLD),
    }
    y += 29.0;
    if ui::small_button(
        "Сохранить проект корабля",
        Rect::new(x, y, width, 30.0),
        false,
    ) {
        actions.push(Action::NavalSaveDesign(
            hull.id.clone(),
            format!("{} — проект", w.catalogue.label(&hull.id)),
            h.naval_draft.clone(),
        ));
    }
    y += 44.0;
    if y + 100.0 > bottom {
        return;
    }
    ui::section("СОХРАНЁННЫЕ КОНСТРУКЦИИ", x, y, width);
    y += 27.0;
    let designs: Vec<_> = w
        .naval
        .designs
        .values()
        .filter(|d| d.country == tag)
        .collect();
    if designs.is_empty() {
        ui::label(
            "Сохранённых проектов ещё нет",
            Rect::new(x, y, width, 24.0),
            11.0,
            ui::MUTED,
        );
        return;
    }
    h.naval_design_index %= designs.len();
    let design = designs[h.naval_design_index];
    if ui::small_button("◀", Rect::new(x, y, arrow, 28.0), false) {
        h.naval_design_index = (h.naval_design_index + designs.len() - 1) % designs.len();
    }
    ui::label(
        &format!("{} · {:.0} IC", design.name, design.cost),
        Rect::new(x + arrow + 5.0, y + 3.0, width - 2.0 * arrow - 10.0, 22.0),
        12.0,
        ui::TEXT,
    );
    if ui::small_button("▶", Rect::new(x + width - arrow, y, arrow, 28.0), false) {
        h.naval_design_index = (h.naval_design_index + 1) % designs.len();
    }
    y += 37.0;
    if ui::small_button(
        "Поставить проект на верфи",
        Rect::new(x, y, width, 28.0),
        false,
    ) {
        let base = w
            .conditions
            .iter()
            .find(|(id, c)| c.naval_base > 0 && world.controller_of_province(**id) == Some(tag))
            .map(|(&id, _)| id);
        if let Some(base) = base {
            actions.push(Action::NavalQueueDesign(design.id.clone(), base))
        } else {
            h.notice = "Нет доступного морского порта".into()
        }
    }
    y += 35.0;
    if y + 54.0 > bottom {
        return;
    }
    let candidates: Vec<_> = w
        .naval
        .task_forces
        .values()
        .filter(|f| {
            f.country == tag
                && f.mission == NavalMission::Hold
                && world.controller_of_province(f.base) == Some(tag)
                && w.conditions.get(&f.base).is_some_and(|b| b.naval_base > 0)
        })
        .flat_map(|f| {
            f.ships
                .iter()
                .enumerate()
                .filter(move |(_, s)| {
                    s.model == design.hull
                        && s.variant.as_deref() != Some(design.id.as_str())
                        && s.strength > 0.
                })
                .map(move |(idx, s)| (f.id, idx, s))
        })
        .filter(|(id, idx, _)| {
            !w.naval
                .refits
                .values()
                .any(|job| job.task_force == *id && job.ship_index == *idx)
        })
        .collect();
    if candidates.is_empty() {
        ui::label(
            "В порту нет подходящих кораблей для рефита",
            Rect::new(x, y, width, 22.0),
            11.0,
            ui::MUTED,
        );
    } else {
        h.naval_refit_index %= candidates.len();
        let (id, index, ship) = candidates[h.naval_refit_index];
        let b = 32.0_f32.min(width / 5.0);
        if ui::small_button("◀", Rect::new(x, y, b, 24.0), false) {
            h.naval_refit_index = (h.naval_refit_index + candidates.len() - 1) % candidates.len();
        }
        ui::label(
            &format!("Рефит: {}", ship.name),
            Rect::new(x + b + 4.0, y + 2.0, width - 2.0 * b - 8.0, 20.0),
            11.0,
            ui::TEXT,
        );
        if ui::small_button("▶", Rect::new(x + width - b, y, b, 24.0), false) {
            h.naval_refit_index = (h.naval_refit_index + 1) % candidates.len();
        }
        y += 27.0;
        if ui::small_button(
            "Переоборудовать выбранный корабль",
            Rect::new(x, y, width, 27.0),
            false,
        ) {
            actions.push(Action::NavalQueueRefit(id, index, design.id.clone()));
        }
    }
}

fn naval_carrier_view(
    h: &mut Hud,
    tag: &str,
    w: &Warfare,
    x: f32,
    mut y: f32,
    width: f32,
    bottom: f32,
    actions: &mut Vec<Action>,
) {
    ui::section("ПАЛУБНАЯ АВИАЦИЯ — ЗАПАСЫ И АВИАГРУППЫ", x, y, width);
    y += 28.0;
    let mut carriers = Vec::new();
    for force in w.naval.task_forces.values().filter(|f| f.country == tag) {
        for (idx, ship) in force.ships.iter().enumerate() {
            let cap = w.naval.carrier_capacity(force.id, idx, w);
            if cap > 0 {
                carriers.push((force.id, idx, cap));
            }
        }
    }
    if carriers.is_empty() {
        ui::label(
            "Действующих авианосцев с палубой нет",
            Rect::new(x, y, width, 24.0),
            12.0,
            ui::GOLD,
        );
        return;
    }
    h.naval_carrier_index %= carriers.len();
    let (id, index, capacity) = carriers[h.naval_carrier_index];
    let ship = &w.naval.task_forces[&id].ships[index];
    let arrow = 38.0_f32.min(width / 5.0);
    if ui::small_button("◀", Rect::new(x, y, arrow, 27.0), false) {
        h.naval_carrier_index = (h.naval_carrier_index + carriers.len() - 1) % carriers.len();
    }
    ui::label(
        &format!("{} (палуба {capacity})", ship.name),
        Rect::new(x + arrow + 5.0, y + 3.0, width - 2.0 * arrow - 10.0, 22.0),
        12.0,
        ui::TEXT,
    );
    if ui::small_button("▶", Rect::new(x + width - arrow, y, arrow, 27.0), false) {
        h.naval_carrier_index = (h.naval_carrier_index + 1) % carriers.len();
    }
    y += 37.0;
    let stance = w.naval.carrier_stance(id);
    let next = stance.next();
    if ui::small_button(
        &format!(
            "Позиция флота: {} (атака {:.0}% / защита {:.0}%) →",
            stance.label(),
            stance.offensive_fraction() * 100.0,
            stance.defensive_fraction() * 100.0,
        ),
        Rect::new(x, y, width, 29.0),
        false,
    ) {
        actions.push(Action::NavalSetCarrierStance(id, next));
    }
    y += 35.0;
    let groups = w.naval.carrier_wings.get(&(id, index));
    let deployed = groups.map_or(0, |gs| gs.iter().map(|g| g.planes).sum::<u32>());
    let reserved = groups.map_or(0, |gs| gs.iter().map(|g| g.target).sum::<u32>());
    ui::label(
        &format!("На палубе {deployed}/{capacity} · заказано {reserved}"),
        Rect::new(x, y, width, 23.0),
        12.0,
        ui::TEXT,
    );
    y += 30.0;
    for group in groups.into_iter().flatten().take(5) {
        if y + 22.0 > bottom - 115.0 {
            break;
        }
        ui::label(
            &format!(
                "{}: {}/{} · вылетов {} · потерь {}",
                group.model, group.planes, group.target, group.sorties, group.losses
            ),
            Rect::new(x, y, width, 20.0),
            11.0,
            ui::TEXT,
        );
        y += 24.0;
    }
    y = y.max(bottom - 145.0);
    if y + 130.0 > bottom {
        return;
    }
    let aircraft: Vec<_> = w
        .countries
        .get(tag)
        .map(|country| {
            w.catalogue
                .equipment
                .values()
                .filter(|def| {
                    crate::naval_carrier::role(def).is_some()
                        && w.catalogue.equipment_available(&def.id, &country.modifiers)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if aircraft.is_empty() {
        ui::label(
            "Нет исследованной палубной авиации",
            Rect::new(x, y, width, 25.0),
            11.0,
            ui::GOLD,
        );
        return;
    }
    h.naval_aircraft_index %= aircraft.len();
    let model = aircraft[h.naval_aircraft_index];
    if ui::small_button("◀", Rect::new(x, y, arrow, 26.0), false) {
        h.naval_aircraft_index = (h.naval_aircraft_index + aircraft.len() - 1) % aircraft.len();
    }
    ui::label(
        w.catalogue.label(&model.id),
        Rect::new(x + arrow + 5.0, y + 3.0, width - 2.0 * arrow - 10.0, 22.0),
        12.0,
        ui::TEXT,
    );
    if ui::small_button("▶", Rect::new(x + width - arrow, y, arrow, 26.0), false) {
        h.naval_aircraft_index = (h.naval_aircraft_index + 1) % aircraft.len();
    }
    y += 33.0;
    if ui::small_button("−10", Rect::new(x, y, arrow + 15.0, 27.0), false) {
        h.naval_target_planes = h.naval_target_planes.saturating_sub(10).max(1);
    }
    ui::label(
        &format!("Цель: {} самолётов", h.naval_target_planes),
        Rect::new(x + arrow + 20.0, y + 3.0, width - 2. * (arrow + 20.0), 22.0),
        11.0,
        ui::TEXT,
    );
    if ui::small_button(
        "+10",
        Rect::new(x + width - arrow - 15.0, y, arrow + 15.0, 27.0),
        false,
    ) {
        h.naval_target_planes = h.naval_target_planes.saturating_add(10).min(capacity);
    }
    y += 33.0;
    if ui::small_button("Назначить авиагруппу", Rect::new(x, y, width, 29.0), false)
    {
        actions.push(Action::NavalAssignCarrierWing(
            id,
            index,
            model.id.clone(),
            h.naval_target_planes.min(capacity),
        ));
    }
}
fn navy(
    h: &mut Hud,
    world: &World,
    tag: &str,
    w: &Warfare,
    x: f32,
    y: f32,
    width: f32,
    bottom: f32,
    actions: &mut Vec<Action>,
) {
    let page_width = (width - 8.0) / 3.0;
    for (i, title) in ["Соединения", "Конструктор", "Авианосцы"]
        .into_iter()
        .enumerate()
    {
        if ui::small_button(
            title,
            Rect::new(x + i as f32 * (page_width + 4.0), y, page_width, 28.0),
            h.naval_page as usize == i,
        ) {
            h.naval_page = i as u8;
            h.scroll = 0;
        }
    }
    let y = y + 35.0;
    if h.naval_page == 1 {
        naval_designer_view(h, world, tag, w, x, y, width, bottom, actions);
        return;
    }
    if h.naval_page == 2 {
        naval_carrier_view(h, tag, w, x, y, width, bottom, actions);
        return;
    }
    let sea_regions: Vec<u32> = w
        .air
        .regions
        .iter()
        .filter(|(_, provinces)| {
            provinces.iter().any(|p| {
                world
                    .geography
                    .provinces
                    .get(p)
                    .is_some_and(|g| g.definition.kind == crate::map_graph::ProvinceKind::Sea)
            })
        })
        .map(|(&id, _)| id)
        .collect();
    let port = w
        .conditions
        .iter()
        .find(|(id, c)| c.naval_base > 0 && world.controller_of_province(**id) == Some(tag))
        .map(|(&id, _)| id);
    let dockyards = world.country_economy(tag).map_or(0, |e| e.dockyards);
    let forces: Vec<_> = w
        .naval
        .task_forces
        .values()
        .filter(|f| f.country == tag)
        .collect();
    ui::label(
        &format!(
            "Флот: {} соединений · Верфей: {} · Кораблей: {}",
            forces.len(),
            dockyards,
            forces.iter().map(|f| f.ships.len()).sum::<usize>()
        ),
        Rect::new(x, y, width, 22.0),
        12.0,
        ui::TEXT,
    );
    let battle = w
        .naval
        .battle_reports
        .iter()
        .rev()
        .find(|battle| battle.attacker == tag || battle.defender == tag);
    if let Some(battle) = battle {
        ui::label(
            &format!(
                "Последний бой: регион #{} · попаданий {} · потоплено {}",
                battle.region,
                battle.hits,
                battle.sunk.len()
            ),
            Rect::new(x, y + 22.0, width, 19.0),
            11.0,
            ui::GOLD,
        );
    }
    let mut yy = y + if battle.is_some() { 47.0 } else { 27.0 };
    if sea_regions.is_empty() {
        ui::label(
            "Морские регионы не импортированы",
            Rect::new(x, yy, width, 22.0),
            12.0,
            ui::GOLD,
        );
    } else {
        h.naval_region_index %= sea_regions.len();
        let region = sea_regions[h.naval_region_index];
        let bw = width.min(42.0);
        if ui::small_button("◀", Rect::new(x, yy, bw, 27.0), false) {
            h.naval_region_index =
                (h.naval_region_index + sea_regions.len() - 1) % sea_regions.len();
        }
        ui::label(
            &format!("Морской регион #{region}"),
            Rect::new(x + bw + 5.0, yy + 3.0, width - 2.0 * bw - 10.0, 21.0),
            12.0,
            ui::TEXT,
        );
        if ui::small_button("▶", Rect::new(x + width - bw, yy, bw, 27.0), false) {
            h.naval_region_index = (h.naval_region_index + 1) % sea_regions.len();
        }
    }
    yy += 33.0;
    ui::section("СОЕДИНЕНИЯ И ПРИКАЗЫ", x, yy, width);
    yy += 28.0;
    let max_visible = ((bottom - yy - 149.0) / 69.0).max(0.0) as usize;
    let max_visible = max_visible.min(6);
    if forces.is_empty() {
        ui::label(
            "Стартовый флот не найден — проверьте OOB",
            Rect::new(x, yy, width, 21.0),
            11.0,
            ui::MUTED,
        );
    } else {
        h.scroll = h
            .scroll
            .min(forces.len().saturating_sub(max_visible.max(1)));
        for force in forces.iter().skip(h.scroll).take(max_visible.max(1)) {
            if yy + 65.0 > bottom - 124.0 {
                break;
            }
            ui::row_bg(Rect::new(x, yy, width, 65.0), false, false);
            let alive = force.ships.iter().filter(|ship| ship.strength > 0.).count();
            let strength = if alive == 0 {
                0.0
            } else {
                force.ships.iter().map(|ship| ship.strength).sum::<f64>() / alive as f64
            };
            ui::label(
                &format!(
                    "{} · {alive}/{} · прочность {:.0}%",
                    force.name,
                    force.ships.len(),
                    100.0 * strength
                ),
                Rect::new(x + 5.0, yy + 2.0, width - 10.0, 19.0),
                11.0,
                ui::TEXT,
            );
            if ui::small_button(
                &format!("{} →", force.mission.label()),
                Rect::new(x + 4.0, yy + 21.0, width - 8.0, 21.0),
                false,
            ) {
                let next = force.mission.next();
                actions.push(Action::NavalOrder(
                    force.id,
                    next,
                    sea_regions.get(h.naval_region_index).copied().unwrap_or(0),
                ));
            }
            let commander = w
                .naval
                .admiral_for(force.id)
                .map_or("Без командира", |a| a.name.as_str());
            if ui::small_button(
                &format!("Адмирал: {commander} →"),
                Rect::new(x + 4.0, yy + 43.0, width - 8.0, 20.0),
                false,
            ) {
                let eligible: Vec<_> = w
                    .naval
                    .admirals
                    .values()
                    .filter(|a| a.country == tag)
                    .map(|a| a.name.as_str())
                    .collect();
                let next = if eligible.is_empty() {
                    None
                } else {
                    let index = eligible
                        .iter()
                        .position(|name| *name == commander)
                        .map_or(0, |pos| pos + 1);
                    eligible.get(index).map(|s| (*s).to_owned())
                };
                actions.push(Action::NavalAssignAdmiral(force.id, next));
            }
            yy += 69.0;
        }
        if forces.len() > max_visible.max(1) && yy + 29.0 < bottom - 119.0 {
            let bw = (width - 8.0) / 2.0;
            if ui::small_button("Предыдущие", Rect::new(x, yy, bw, 25.0), false) {
                h.scroll = h.scroll.saturating_sub(1);
            }
            if ui::small_button("Следующие", Rect::new(x + bw + 8.0, yy, bw, 25.0), false)
            {
                h.scroll = (h.scroll + 1).min(forces.len() - max_visible.max(1));
            }
        }
    }
    let mut yy = (bottom - 116.0).max(yy + 6.0);
    if yy + 107.0 > bottom {
        return;
    }
    ui::section("СТРОИТЕЛЬСТВО КОРАБЛЕЙ", x, yy, width);
    yy += 26.0;
    let models: Vec<_> = w
        .countries
        .get(tag)
        .map(|army| {
            w.catalogue
                .equipment
                .values()
                .filter(|e| NavalState::can_build_ship(&w.catalogue, &e.id, &army.modifiers))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if models.is_empty() {
        ui::label(
            "Нет исследованных конструкций кораблей",
            Rect::new(x, yy, width, 22.0),
            11.0,
            ui::MUTED,
        );
        return;
    }
    h.naval_hull_index %= models.len();
    let model = models[h.naval_hull_index];
    let bw = 34.0_f32.min(width / 4.0);
    if ui::small_button("◀", Rect::new(x, yy, bw, 24.0), false) {
        h.naval_hull_index = (h.naval_hull_index + models.len() - 1) % models.len();
    }
    ui::label(
        &format!("{} · {:.0} IC", w.catalogue.label(&model.id), model.cost),
        Rect::new(x + bw + 4.0, yy + 2.0, width - 2.0 * bw - 8.0, 20.0),
        11.0,
        ui::TEXT,
    );
    if ui::small_button("▶", Rect::new(x + width - bw, yy, bw, 24.0), false) {
        h.naval_hull_index = (h.naval_hull_index + 1) % models.len();
    }
    yy += 29.0;
    if ui::small_button(
        "Поставить на верфи (до 5)",
        Rect::new(x, yy, width, 27.0),
        false,
    ) {
        if let Some(base) = port {
            actions.push(Action::NavalQueueShip(model.id.clone(), base));
        } else {
            h.notice = "Нет собственного морского порта".into();
        }
    }
    let in_queue = w.naval.shipbuilding.get(tag).map_or(0, |lines| lines.len());
    ui::label(
        &format!("Очередь кораблей: {in_queue} · Порт: {}", port.unwrap_or(0)),
        Rect::new(x, yy + 30.0, width, 18.0),
        11.0,
        ui::MUTED,
    );
}

fn scroll(h: &mut Hud, rect: Rect, max: usize) {
    ui::scrollbar(
        rect,
        &mut h.scroll,
        max,
        (rect.h / 40.0).floor().max(1.0) as usize,
    );
    if ui::inside(rect) {
        let (_, v) = mouse_wheel();
        if v > 0.0 {
            h.scroll = h.scroll.saturating_sub(2);
        } else if v < 0.0 {
            h.scroll = (h.scroll + 2).min(max);
        }
    }
}
fn logistics(
    h: &mut Hud,
    tag: &str,
    w: &Warfare,
    prod: &ProductionState,
    x: f32,
    y: f32,
    width: f32,
    bottom: f32,
) {
    let stock_status = prod.country(tag).map_or("Нет производственных данных страны".into(), |c| {
        if !c.starting_import_errors.is_empty() {
            format!("Ошибка стартового импорта: {}", c.starting_import_errors[0])
        } else if c.starting_data_applied && c.starting_stockpile_effects == 0 {
            "В исходных данных нет прямых записей национального запаса. Оснащение частей показано отдельно.".into()
        } else { "Склад, техника в частях и нехватка · единое оснащение кампании".into() }
    });
    status(&stock_status, x, y, width);
    ui::section(
        "СНАРЯЖЕНИЕ           СКЛАД       В ЧАСТЯХ      НЕХВАТКА",
        x,
        y + 29.0,
        width,
    );
    let mut rows = std::collections::BTreeMap::<String, (u64, u64, u64)>::new();
    if let Some(c) = prod.country(tag) {
        for (model, n) in &c.model_stockpile {
            let key = w
                .catalogue
                .equipment
                .get(model)
                .map_or(model.as_str(), |e| e.archetype.as_str());
            rows.entry(key.into()).or_default().0 += *n;
        }
    }
    let mut need = std::collections::BTreeMap::<String, u64>::new();
    for d in w
        .divisions
        .values()
        .filter(|d| d.country == tag && !d.destroyed)
    {
        for (arch, quantity) in &d.derived.equipment {
            let actual: u64 = d
                .equipment
                .iter()
                .filter(|(id, _)| {
                    w.catalogue
                        .equipment
                        .get(*id)
                        .is_some_and(|e| e.archetype == *arch)
                })
                .map(|(_, n)| u64::from(*n))
                .sum();
            rows.entry(arch.clone()).or_default().2 += actual;
            *need.entry(arch.clone()).or_default() +=
                (quantity.ceil() as u64).saturating_sub(actual);
        }
    }
    for (arch, n) in need {
        if n > 0 {
            rows.entry(arch).or_default().1 = n;
        }
    }
    let count = ((bottom - y - 110.0) / 42.0).max(0.0) as usize;
    let max = rows.len().saturating_sub(count.max(1));
    h.scroll = h.scroll.min(max);
    scroll(h, Rect::new(x, y + 59.0, width, bottom - y - 59.0), max);
    for (i, (name, (stock, deficit, deployed))) in
        rows.iter().skip(h.scroll).take(count).enumerate()
    {
        let yy = y + 59.0 + i as f32 * 42.0;
        ui::row_bg(Rect::new(x, yy, width, 38.0), false, i % 2 == 1);
        ui::label(
            name,
            Rect::new(x + 7.0, yy + 10.0, width * 0.42, 20.0),
            12.0,
            ui::TEXT,
        );
        ui::label(
            &format!("{stock}     {deployed}     {deficit}"),
            Rect::new(x + width * 0.45, yy + 10.0, width * 0.53, 20.0),
            12.0,
            if *deficit > 0 {
                Color::from_rgba(209, 122, 103, 255)
            } else {
                ui::GREEN
            },
        );
    }
    if rows.is_empty() {
        ui::label(
            "Склад пуст. Создайте производственные линии.",
            Rect::new(x + 7.0, y + 75.0, width - 14.0, 20.0),
            12.0,
            ui::MUTED,
        );
    }
    ui::label(
        if w.logistics.enabled {
            "Доставка: действующая сеть хабов, ж/д и портов; изолированные части не получают складское пополнение."
        } else {
            "Сетевая доставка отключена для этого набора данных."
        },
        Rect::new(x, bottom - 35.0, width, 28.0),
        11.0,
        ui::MUTED,
    );
}
fn system_shell(h: &mut Hud, panel: Panel, tag: &str, x: f32, y: f32, width: f32, bottom: f32) {
    ui::flag(tag, Rect::new(x + 4.0, y + 4.0, 55.0, 34.0));
    status(
        "ЗАГЛУШКА · Механика ещё не реализована",
        x + 74.0,
        y + 12.0,
        width - 76.0,
    );
    let sections: Vec<(&str, Vec<&str>)> = match panel {
        Panel::Decisions => vec![
            ("СОБЫТИЯ", vec!["Журнал событий"]),
            (
                "РЕШЕНИЯ",
                vec![
                    "Политические решения",
                    "Экономические решения",
                    "Военные решения",
                ],
            ),
            ("МИССИИ", vec!["Срочные задания и сроки выполнения"]),
        ],
        Panel::Agency => vec![
            (
                "АГЕНТСТВО",
                vec!["Создать агентство", "Улучшения агентства"],
            ),
            (
                "ОПЕРАТИВНИКИ",
                vec!["Нанять оперативника", "Разведывательная сеть"],
            ),
            ("ОПЕРАЦИИ", vec!["Подготовить операцию", "Контрразведка"]),
        ],
        Panel::Officer => vec![
            (
                "ВОЕННОЕ РУКОВОДСТВО",
                vec![
                    "Главнокомандующий армией",
                    "Главнокомандующий флотом",
                    "Главнокомандующий авиацией",
                ],
            ),
            (
                "ДУХИ И ДОКТРИНЫ",
                vec!["Армейские традиции", "Дух армии", "Военные доктрины"],
            ),
            (
                "КОМАНДОВАНИЕ",
                vec!["Генералы и фельдмаршалы", "Военные академии"],
            ),
        ],
        Panel::Navy => vec![
            (
                "МОРСКИЕ СИЛЫ",
                vec!["Флоты и оперативные соединения", "Конструктор кораблей"],
            ),
            (
                "ЗАДАЧИ",
                vec![
                    "Патрулирование",
                    "Поиск и уничтожение",
                    "Рейд на конвои",
                    "Морское вторжение",
                ],
            ),
        ],
        Panel::Air => vec![
            (
                "ВОЗДУШНЫЕ СИЛЫ",
                vec!["Авиакрылья и авиабазы", "Конструктор самолётов"],
            ),
            (
                "ЗАДАЧИ",
                vec![
                    "Превосходство в воздухе",
                    "Непосредственная поддержка",
                    "Стратегические бомбардировки",
                    "Перехват",
                ],
            ),
        ],
        _ => vec![],
    };
    let mut yy = y + 60.0;
    for (heading, items) in sections {
        if yy + 28.0 > bottom {
            break;
        }
        ui::section(heading, x, yy, width);
        yy += 29.0;
        for item in items {
            if yy + 32.0 > bottom {
                break;
            }
            stub_button(h, item, Rect::new(x, yy, width, 30.0));
            yy += 35.0;
        }
        yy += 12.0;
    }
}

pub fn draw_modal(h: &mut Hud) -> Option<Action> {
    if !h.menu && h.detail.is_none() {
        return None;
    }
    let l = Layout::current();
    draw_rectangle(0.0, 0.0, l.width, l.height, Color::new(0.0, 0.0, 0.0, 0.57));
    let width = (l.width - 24.0).min(450.0);
    let height = if h.menu { 355.0 } else { 220.0 };
    let r = Rect::new(
        (l.width - width) * 0.5,
        (l.height - height) * 0.5,
        width,
        height,
    );
    ui::panel(r.x, r.y, r.w, r.h);
    if h.menu {
        ui::header(r.x + 4.0, r.y + 4.0, r.w - 8.0, "МЕНЮ ИГРЫ");
        let mut yy = r.y + 53.0;
        if ui::small_button(
            "Продолжить",
            Rect::new(r.x + 24.0, yy, r.w - 48.0, 36.0),
            false,
        ) {
            return Some(Action::Resume);
        }
        yy += 45.0;
        if ui::small_button(
            "Сохранить игру",
            Rect::new(r.x + 24.0, yy, r.w - 48.0, 34.0),
            false,
        ) {
            return Some(Action::Save);
        }
        yy += 43.0;
        if ui::small_button(
            "Загрузить игру",
            Rect::new(r.x + 24.0, yy, r.w - 48.0, 34.0),
            false,
        ) {
            return Some(Action::Load);
        }
        yy += 43.0;
        if ui::small_button(
            "Перезагрузить отображение карты",
            Rect::new(r.x + 24.0, yy, r.w - 48.0, 34.0),
            false,
        ) {
            return Some(Action::Reload);
        }
        yy += 43.0;
        if ui::small_button(
            "В главное меню",
            Rect::new(r.x + 24.0, yy, r.w - 48.0, 34.0),
            false,
        ) {
            return Some(Action::MainMenu);
        }
    } else {
        let title = h.detail.clone().unwrap_or_default();
        ui::header(
            r.x + 4.0,
            r.y + 4.0,
            r.w - 8.0,
            if title.starts_with("Объявить войну:") {
                "ОБЪЯВЛЕНИЕ ВОЙНЫ"
            } else {
                "ЗАГЛУШКА"
            },
        );
        ui::label(
            &title,
            Rect::new(r.x + 18.0, r.y + 58.0, r.w - 36.0, 24.0),
            16.0,
            ui::GOLD,
        );
        if let Some(target) = title.strip_prefix("Объявить войну: ") {
            ui::label(
                "Подтвердить объявление войны?",
                Rect::new(r.x + 18.0, r.y + 94.0, r.w - 36.0, 22.0),
                13.0,
                ui::TEXT,
            );
            if ui::small_button(
                "Объявить войну",
                Rect::new(r.x + 18.0, r.bottom() - 51.0, (r.w - 44.0) * 0.6, 32.0),
                false,
            ) {
                h.detail = None;
                return Some(Action::DeclareWar(target.into()));
            }
        } else {
            ui::label(
                "Эта механика ещё не подключена к симуляции.",
                Rect::new(r.x + 18.0, r.y + 98.0, r.w - 36.0, 22.0),
                13.0,
                ui::TEXT,
            );
            ui::label(
                "Действие не изменяет состояние страны.",
                Rect::new(r.x + 18.0, r.y + 123.0, r.w - 36.0, 22.0),
                12.0,
                ui::MUTED,
            );
        }
        if ui::small_button(
            "Закрыть",
            Rect::new(r.right() - 128.0, r.bottom() - 51.0, 110.0, 32.0),
            false,
        ) {
            h.detail = None;
        }
    }
    None
}

pub fn state_bounds() -> Rect {
    let l = Layout::current();
    let height = 353.0f32.min(l.height - l.top - 55.0);
    Rect::new(
        0.0,
        l.height - l.bottom - height,
        l.primary().w.min(420.0),
        height,
    )
}
pub fn draw_state(
    h: &mut Hud,
    world: &World,
    w: &Warfare,
    peace: &crate::peace::PeaceSystem,
    province: u32,
) -> Vec<Action> {
    let r = state_bounds();
    let mut actions = Vec::new();
    let state = world.state_for_province(province);
    ui::panel(r.x, r.y, r.w, r.h);
    let name = state.and_then(|s| s.name.as_deref()).unwrap_or("Провинция");
    ui::header(r.x + 3.0, r.y + 3.0, r.w - 6.0, name);
    let owner = world.owner_of_province(province);
    let controller = world.controller_of_province(province);
    let x = r.x + 12.0;
    let width = r.w - 24.0;
    if let Some(tag) = controller {
        ui::flag(tag, Rect::new(x, r.y + 49.0, 52.0, 32.0));
        let b = Rect::new(x + 66.0, r.y + 47.0, width - 66.0, 36.0);
        if ui::small_button(crate::country_display_name(tag), b, false) {
            h.diplomacy_country = Some(tag.into());
            actions.push(Action::Open(Panel::Diplomacy));
        }
        ui::tooltip(
            b,
            "Страна, контролирующая провинцию",
            "Открыть дипломатию с этой страной.",
        );
    }
    let terrain = world
        .geography
        .provinces
        .get(&province)
        .map_or("—", |p| p.definition.terrain.as_str());
    ui::label(
        &format!(
            "Владелец: {} · Контроль: {}",
            owner.unwrap_or("—"),
            controller.unwrap_or("—")
        ),
        Rect::new(x, r.y + 92.0, width, 20.0),
        12.0,
        ui::GOLD,
    );
    let vp = world
        .geography
        .provinces
        .get(&province)
        .map_or(0., |p| p.victory_points);
    let capital = w
        .logistics
        .countries
        .iter()
        .find(|(_, c)| c.capital == Some(province))
        .map(|(tag, _)| tag.as_str());
    ui::label(
        &format!(
            "{terrain} · P{province} · VP {vp:.0}{}",
            capital.map_or(String::new(), |tag| format!(" · Столица {tag}"))
        ),
        Rect::new(x, r.y + 114.0, width, 20.0),
        12.0,
        ui::MUTED,
    );
    ui::section("ПРОМЫШЛЕННОСТЬ И ИНФРАСТРУКТУРА", x, r.y + 143.0, width);
    let e = state.map(|s| s.economy);
    let data = [
        ("Фабрики", e.map(|e| e.civilian_factories)),
        ("Военные заводы", e.map(|e| e.military_factories)),
        ("Верфи", e.map(|e| e.dockyards)),
        ("Инфраструктура", e.map(|e| e.infrastructure)),
    ];
    for (i, (title, value)) in data.iter().enumerate() {
        let cw = (width - 9.0) / 4.0;
        let xx = x + i as f32 * (cw + 3.0);
        ui::label(title, Rect::new(xx, r.y + 177.0, cw, 18.0), 10.0, ui::MUTED);
        ui::label(
            &value.map_or("—".into(), |n| n.to_string()),
            Rect::new(xx + 4.0, r.y + 195.0, cw - 4.0, 24.0),
            18.0,
            ui::TEXT,
        );
    }
    let slots = state.map_or(0, |s| s.shared_building_slots);
    let used = e.map_or(0, |e| e.occupied_shared_slots());
    ui::label(
        &format!("Слоты строительства: {used} / {slots}"),
        Rect::new(x, r.y + 230.0, width, 18.0),
        12.0,
        ui::GOLD,
    );
    if let Some(state) = state {
        let rights = peace.rights.get(&state.id);
        let cores = rights
            .map(|r| r.cores.iter().cloned().collect::<Vec<_>>().join(", "))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "нет импортированных прав".into());
        let claims = rights
            .map(|r| r.claims.iter().cloned().collect::<Vec<_>>().join(", "))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "—".into());
        ui::label(
            &format!("Национальные (core): {cores}"),
            Rect::new(x, r.y + 254.0, width, 19.0),
            11.0,
            ui::GOLD,
        );
        ui::label(
            &format!("Претензии (claims): {claims}"),
            Rect::new(x, r.y + 274.0, width, 19.0),
            11.0,
            ui::MUTED,
        );
    }
    let b = Rect::new(x, r.bottom() - 40.0, (width - 5.0) * 0.6, 28.0);
    if ui::small_button("Строительство", b, false) {
        actions.push(Action::Open(Panel::Construction));
    }
    if ui::small_button(
        "Ресурсы",
        Rect::new(b.right() + 5.0, b.y, width - b.w - 5.0, b.h),
        false,
    ) {
        actions.push(Action::MapMode(1));
    }
    let _ = w;
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn existing_air_button_opens_real_mission_inspector_and_closes_with_it() {
        let mut hud = Hud::default();
        let mut c = ConstructionView::default();
        let mut p = ProductionView::default();
        let mut r = ResearchView::default();
        let mut a = ArmyView::default();
        hud.open(Some(Panel::Air), &mut c, &mut p, &mut r, &mut a);
        assert!(a.open);
        assert!(!Panel::Air.placeholder());
        assert_eq!(a.mobile.page, crate::mobile_army_ui::MobilePage::Logistics);
        assert_eq!(a.navigation_panel(), Panel::Air);
        a.close();
        hud.reconcile(&c, &p, &r, &a);
        assert!(hud.active.is_none());
    }
    #[test]
    fn hoi_shortcuts_are_unique_and_shift_does_not_open_two_panels() {
        let mut bindings = std::collections::BTreeSet::new();
        for panel in Panel::NAV
            .into_iter()
            .chain([Panel::Army, Panel::Navy, Panel::Air])
        {
            if let Some((key, shift)) = panel.binding() {
                assert!(
                    bindings.insert((key as u32, shift)),
                    "Conflicting shortcut for {panel:?}"
                );
            }
        }
        assert_eq!(Panel::Research.binding(), Some((KeyCode::W, false)));
        assert_eq!(Panel::Construction.binding(), Some((KeyCode::T, false)));
        assert_eq!(Panel::Production.binding(), Some((KeyCode::Y, false)));
        assert_eq!(Panel::Agency.binding(), Some((KeyCode::W, true)));
    }
    #[test]
    fn navigation_has_single_owner_and_placeholders_leave_simulation_untouched() {
        let mut hud = Hud::default();
        let mut c = ConstructionView::default();
        let mut p = ProductionView::default();
        let mut r = ResearchView::default();
        let mut a = ArmyView::default();
        for panel in Panel::NAV {
            hud.open(Some(panel), &mut c, &mut p, &mut r, &mut a);
            assert_eq!(hud.active, Some(panel));
            assert_eq!(c.open, panel == Panel::Construction);
            assert_eq!(p.open, panel == Panel::Production);
            assert_eq!(r.open, panel == Panel::Research);
            assert_eq!(a.open, matches!(panel, Panel::Recruitment | Panel::Officer));
            hud.toggle(panel, &mut c, &mut p, &mut r, &mut a);
            assert!(hud.active.is_none());
            assert!(!c.open && !p.open && !r.open && !a.open);
        }
    }
}
