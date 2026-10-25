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
use crate::{production::ProductionState, research::ResearchWorld, warfare::Warfare, world::World};
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
        matches!(
            self,
            Self::Politics
                | Self::Decisions
                | Self::Trade
                | Self::Agency
                | Self::Officer
                | Self::Focus
                | Self::Navy
                | Self::Air
        )
    }
}

pub struct Hud {
    pub active: Option<Panel>,
    pub diplomacy_country: Option<String>,
    pub outliner: bool,
    pub minimap: bool,
    pub map_palette: bool,
    pub menu: bool,
    pub menu_was_paused: bool,
    pub reload_requested: bool,
    pub detail: Option<String>,
    pub scroll: usize,
    pub outliner_scroll: usize,
    pub notice: String,
}
impl Default for Hud {
    fn default() -> Self {
        Self {
            active: None,
            diplomacy_country: None,
            outliner: true,
            minimap: true,
            map_palette: false,
            menu: false,
            menu_was_paused: true,
            reload_requested: false,
            detail: None,
            scroll: 0,
            outliner_scroll: 0,
            notice: String::new(),
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
        c.close();
        p.close();
        r.open = false;
        r.browsing = false;
        a.close();
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
            Some(Panel::Army) => !a.open,
            _ => false,
        };
        if closed {
            self.active = None;
        }
    }
    pub fn regions(&self) -> Vec<Rect> {
        let l = Layout::current();
        let mut out = vec![
            Rect::new(0.0, 0.0, l.width, l.top),
            l.army_dock(),
            l.map_modes(),
        ];
        if self.active == Some(Panel::Focus)
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
            out.push(l.minimap());
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
    Designer(usize),
    Stop,
    Redeploy,
    MapMode(usize),
    Zoom(f32),
    Fit,
    Menu,
    Resume,
    MainMenu,
    Reload,
    DeclareWar(String),
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
    let xp = army.map_or("—".into(), |a| format!("{:.0}", a.army_xp));
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
            "Доступный резерв из состояния армии. Люди расходуются при пополнении частей.",
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
            "—".into(),
            "Топливо",
            "ЗАГЛУШКА · Национальный топливный баланс ещё не рассчитывается.",
            false,
        ),
        (
            Icon::Convoy,
            "—".into(),
            "Конвои",
            "ЗАГЛУШКА · Производство и маршруты конвоев ещё не реализованы.",
            false,
        ),
        (
            Icon::Command,
            "—".into(),
            "Командный ресурс",
            "ЗАГЛУШКА · Командный ресурс ещё не реализован.",
            false,
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
            "ЗАГЛУШКА · Воздушная система ещё не реализована.",
            false,
        ),
        (
            Icon::Supply,
            "—".into(),
            "Снабжение",
            "ЗАГЛУШКА · Национальная логистическая сеть ещё не рассчитывается.",
            false,
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
    _h: &mut Hud,
    tag: Option<&str>,
    w: &Warfare,
    selection: &ArmyView,
    actions: &mut Vec<Action>,
) {
    let r = Layout::current().army_dock();
    ui::panel(r.x, r.y, r.w, r.h);
    let n = w
        .divisions
        .values()
        .filter(|d| Some(d.country.as_str()) == tag && !d.destroyed)
        .count();
    let card = Rect::new(
        r.x + 4.0,
        r.y + 4.0,
        if r.w < 300.0 { r.w - 8.0 } else { 135.0 },
        r.h - 8.0,
    );
    if ui::small_button(
        &format!("Дивизии: {n}"),
        card,
        !selection.selected.is_empty(),
    ) {
        actions.push(Action::SelectAll);
    }
    ui::tooltip(
        card,
        "Сухопутная армия",
        "Выбрать все свои дивизии. Приказы: ПКМ по провинции; Shift + ПКМ добавляет маршрут.",
    );
    let mut x = card.right() + 5.0;
    if x + 292.0 < r.right() {
        for (label, action) in [("■ H", Action::Stop), ("⇥", Action::Redeploy)] {
            let b = Rect::new(x, r.y + 5.0, 43.0, r.h - 10.0);
            if ui::small_button(
                label,
                b,
                matches!(action, Action::Redeploy) && selection.strategic,
            ) {
                actions.push(action);
            }
            x += 47.0;
        }
        for (icon, title) in [
            (Icon::Flag, "Линия фронта"),
            (Icon::Army, "План наступления"),
            (Icon::Terrain, "Рубеж отступления"),
            (Icon::Recruit, "Назначение генерала"),
        ] {
            let b = Rect::new(x, r.y + 5.0, 33.0, r.h - 10.0);
            ui_icons::draw(
                icon,
                Rect::new(b.x + 5.0, b.y + 4.0, b.w - 10.0, b.h - 8.0),
                ui::MUTED,
            );
            draw_rectangle_lines(b.x, b.y, b.w, b.h, 1.0, ui::EDGE);
            ui::tooltip(
                b,
                title,
                "ЗАГЛУШКА · Эта система приказов ещё не реализована.",
            );
            x += 37.0;
        }
    }
}
fn draw_modes(h: &mut Hud, mode: usize, actions: &mut Vec<Action>) {
    let r = Layout::current().map_modes();
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
    selected_province: Option<u32>,
) -> Vec<Action> {
    let Some(panel) = h.active else {
        return Vec::new();
    };
    if matches!(
        panel,
        Panel::Construction | Panel::Production | Panel::Research | Panel::Army
    ) {
        return Vec::new();
    }
    let l = Layout::current();
    let r = if panel == Panel::Focus {
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
        Panel::Trade => trade(h, world, tag, x, y, width, r.bottom() - 8.0),
        Panel::Recruitment => recruitment(h, tag, w, x, y, width, r.bottom() - 8.0, &mut actions),
        Panel::Logistics => logistics(h, tag, w, prod, x, y, width, r.bottom() - 8.0),
        Panel::Focus => focus(h, x, y, width, r.bottom() - 8.0),
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
    ui_icons::draw(
        Icon::Officer,
        Rect::new(portrait.x + 18.0, portrait.y + 16.0, 70.0, 82.0),
        ui::MUTED,
    );
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
        "Лидер страны",
        "ЗАГЛУШКА · Политическая модель и портрет лидера ещё не подключены.",
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
fn trade(h: &mut Hud, world: &World, tag: &str, x: f32, y: f32, width: f32, bottom: f32) {
    status(
        "ЗАГЛУШКА · Международная торговля не рассчитывается",
        x,
        y,
        width,
    );
    let ew = world
        .country_economy(tag)
        .map_or(0, |e| e.civilian_factories);
    ui::label(
        &format!("Гражданские фабрики: {ew}   Торговые договоры: —"),
        Rect::new(x, y + 27.0, width, 25.0),
        13.0,
        ui::TEXT,
    );
    ui::section(
        "РЕСУРС              ДОСТУПНО       НУЖНО       ИМПОРТ",
        x,
        y + 62.0,
        width,
    );
    let mut yy = y + 90.0;
    for resource in ["Нефть", "Алюминий", "Резина", "Вольфрам", "Сталь", "Хром"]
    {
        if yy + 35.0 > bottom {
            break;
        }
        ui::row_bg(Rect::new(x, yy, width, 33.0), false, false);
        ui_icons::draw(
            Icon::Resources,
            Rect::new(x + 6.0, yy + 7.0, 18.0, 18.0),
            ui::GOLD,
        );
        ui::label(
            resource,
            Rect::new(x + 32.0, yy + 7.0, width * 0.44, 18.0),
            13.0,
            ui::TEXT,
        );
        ui::label(
            "—                 —              —",
            Rect::new(x + width * 0.47, yy + 7.0, width * 0.5, 18.0),
            12.0,
            ui::MUTED,
        );
        yy += 37.0;
    }
    if yy + 75.0 < bottom {
        ui::section("ТОРГОВЫЕ ПАРТНЁРЫ", x, yy + 12.0, width);
        stub_button(
            h,
            "Выбрать страну для импорта",
            Rect::new(x, yy + 42.0, width, 30.0),
        );
    }
}
fn recruitment(
    h: &mut Hud,
    tag: &str,
    w: &Warfare,
    x: f32,
    y: f32,
    width: f32,
    bottom: f32,
    actions: &mut Vec<Action>,
) {
    let army = w.countries.get(tag);
    status(
        "Пополнение действующих частей работает. Обучение — заглушка.",
        x,
        y,
        width,
    );
    ui::section("ПРИОРИТЕТ СНАРЯЖЕНИЯ", x, y + 29.0, width);
    for (i, label) in ["Пополнения", "Улучшения", "Гарнизоны"].iter().enumerate()
    {
        let bw = (width - 8.0) / 3.0;
        stub_button(
            h,
            label,
            Rect::new(x + i as f32 * (bw + 4.0), y + 58.0, bw, 28.0),
        );
    }
    ui::section("ОЧЕРЕДЬ ПОДГОТОВКИ", x, y + 98.0, width);
    stub_button(
        h,
        "Набор новых дивизий · ЗАГЛУШКА",
        Rect::new(x, y + 128.0, width, 42.0),
    );
    ui::section("ШАБЛОНЫ ДИВИЗИЙ", x, y + 185.0, width);
    let templates: Vec<_> = army
        .map(|a| a.templates.values().collect())
        .unwrap_or_default();
    let count = ((bottom - y - 220.0) / 66.0).max(0.0) as usize;
    let max = templates.len().saturating_sub(count.max(1));
    h.scroll = h.scroll.min(max);
    scroll(h, Rect::new(x, y + 213.0, width, bottom - y - 213.0), max);
    for (i, t) in templates.iter().enumerate().skip(h.scroll).take(count) {
        let yy = y + 214.0 + (i - h.scroll) as f32 * 66.0;
        let row = Rect::new(x, yy, width, 61.0);
        ui::row_bg(row, false, i % 2 == 1);
        ui_icons::draw(
            Icon::Army,
            Rect::new(x + 8.0, yy + 14.0, 28.0, 28.0),
            ui::GOLD,
        );
        ui::label(
            &t.name,
            Rect::new(x + 47.0, yy + 8.0, width - 145.0, 20.0),
            13.0,
            ui::TEXT,
        );
        ui::label(
            &format!(
                "Батальоны: {} · Поддержка: {}",
                t.line.len(),
                t.support.len()
            ),
            Rect::new(x + 47.0, yy + 33.0, width - 145.0, 18.0),
            11.0,
            ui::MUTED,
        );
        if ui::small_button(
            "Изменить",
            Rect::new(x + width - 90.0, yy + 15.0, 82.0, 29.0),
            false,
        ) {
            actions.push(Action::Designer(i));
        }
    }
    if templates.is_empty() {
        ui::label(
            "Стартовые шаблоны не загружены",
            Rect::new(x + 7.0, y + 226.0, width - 14.0, 20.0),
            13.0,
            ui::MUTED,
        );
    }
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
    status(
        "Запасы и потребность частей · реальные данные кампании",
        x,
        y,
        width,
    );
    ui::section(
        "СНАРЯЖЕНИЕ                     СКЛАД          ПОТРЕБНО",
        x,
        y + 29.0,
        width,
    );
    let mut rows = std::collections::BTreeMap::<String, (u64, u64)>::new();
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
            *need.entry(arch.clone()).or_default() += (*quantity as u64).saturating_sub(actual);
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
    for (i, (name, (stock, deficit))) in rows.iter().skip(h.scroll).take(count).enumerate() {
        let yy = y + 59.0 + i as f32 * 42.0;
        ui::row_bg(Rect::new(x, yy, width, 38.0), false, i % 2 == 1);
        ui::label(
            name,
            Rect::new(x + 7.0, yy + 10.0, width * 0.6, 20.0),
            12.0,
            ui::TEXT,
        );
        ui::label(
            &format!("{stock}          {deficit}"),
            Rect::new(x + width * 0.64, yy + 10.0, width * 0.34, 20.0),
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
    ui::placeholder(
        Rect::new(x, bottom - 35.0, width, 28.0),
        "Сеть снабжения, поезда и конвои · ЗАГЛУШКА",
    );
}
fn focus(h: &mut Hud, x: f32, y: f32, width: f32, bottom: f32) {
    status(
        "ЗАГЛУШКА · Дерево страны и выполнение фокусов ещё не подключены",
        x,
        y,
        width,
    );
    let names = [
        "Политические усилия",
        "Промышленность",
        "Развитие армии",
        "Дипломатия",
        "Научные проекты",
        "Военная промышленность",
        "Доктрины",
        "Внешняя политика",
    ];
    let cols = if width < 700.0 { 2 } else { 4 };
    let step = (width / cols as f32).min(245.0);
    let node_w = (step - 20.0).min(205.0);
    for i in 0..names.len() {
        let col = i % cols;
        let row = i / cols;
        let xx = x + col as f32 * step + 9.0;
        let yy = y + 52.0 + row as f32 * 145.0;
        if yy + 105.0 > bottom {
            continue;
        }
        if row > 0 {
            draw_line(
                xx + node_w * 0.5,
                yy - 40.0,
                xx + node_w * 0.5,
                yy,
                2.0,
                ui::EDGE,
            );
        }
        let r = Rect::new(xx, yy, node_w, 103.0);
        ui::row_bg(r, false, false);
        ui_icons::draw(
            Icon::Focus,
            Rect::new(xx + node_w * 0.5 - 20.0, yy + 10.0, 40.0, 40.0),
            ui::MUTED,
        );
        ui::label(
            names[i],
            Rect::new(xx + 7.0, yy + 57.0, node_w - 14.0, 18.0),
            12.0,
            ui::TEXT,
        );
        ui::label(
            "ЗАГЛУШКА",
            Rect::new(xx + 7.0, yy + 79.0, node_w - 14.0, 18.0),
            10.0,
            ui::GOLD,
        );
        ui::tooltip(
            r,
            names[i],
            "Образец места в интерфейсе. Это не исторический фокус страны; стоимость, требования и эффекты пока отсутствуют.",
        );
        if ui::inside(r) && is_mouse_button_pressed(MouseButton::Left) {
            h.detail = Some(names[i].into());
        }
    }
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
        ui::placeholder(
            Rect::new(r.x + 24.0, yy, r.w - 48.0, 34.0),
            "Сохранить игру",
        );
        yy += 43.0;
        ui::placeholder(
            Rect::new(r.x + 24.0, yy, r.w - 48.0, 34.0),
            "Загрузить игру",
        );
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
    let height = 310.0f32.min(l.height - l.top - 55.0);
    Rect::new(
        0.0,
        l.height - l.bottom - height,
        l.primary().w.min(420.0),
        height,
    )
}
pub fn draw_state(h: &mut Hud, world: &World, w: &Warfare, province: u32) -> Vec<Action> {
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
    ui::label(
        &format!("Местность: {terrain} · Провинция {province}"),
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
            assert!(!a.open);
            hud.toggle(panel, &mut c, &mut p, &mut r, &mut a);
            assert!(hud.active.is_none());
            assert!(!c.open && !p.open && !r.open && !a.open);
        }
    }
}
