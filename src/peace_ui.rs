//! Minimal usable peace-conference interface. No Paradox art is bundled.
//! One state can be selected and demanded; conflict bidding, multi-round
//! scoring, country AI and source-verified prices remain future stages.
use crate::hoi_ui as ui;
use crate::peace::{ConferencePhase, PeaceAction, PeaceSystem};
use crate::warfare::Warfare;
use crate::world::World;
use macroquad::prelude::*;

#[derive(Debug, Default)]
pub struct PeaceView {
    pub selected_state: Option<u32>,
    pub selected_recipient: usize,
    pub scroll: usize,
    pub notice: String,
    pub last_conference: Option<u64>,
}
pub fn active(system: &PeaceSystem, player: Option<&str>) -> bool {
    let Some(tag) = player else {
        return false;
    };
    system.conferences.values().any(|c| {
        c.phase == ConferencePhase::Demands && (c.winners.contains(tag) || c.losers.contains(tag))
    })
}
pub fn draw(
    system: &mut PeaceSystem,
    world: &mut World,
    military: &mut Warfare,
    player: Option<&str>,
    view: &mut PeaceView,
) {
    let Some(player) = player else {
        return;
    };
    let Some(c) = system
        .conferences
        .values()
        .find(|c| {
            c.phase == ConferencePhase::Demands
                && (c.winners.contains(player) || c.losers.contains(player))
        })
        .cloned()
    else {
        return;
    };
    if view.last_conference != Some(c.id) {
        *view = PeaceView {
            last_conference: Some(c.id),
            ..PeaceView::default()
        };
    }
    // Modal covers the map and cannot leak clicks into province and army orders.
    draw_rectangle(
        0.,
        0.,
        screen_width(),
        screen_height(),
        Color::new(0.0, 0.015, 0.025, 0.84),
    );
    let wide = screen_width().min(850.).max(290.);
    let x = (screen_width() - wide) * 0.5;
    let y = 20.;
    let height = (screen_height() - 40.).max(300.);
    ui::panel(x, y, wide, height);
    ui::header(x + 4., y + 4., wide - 8., "МИРНАЯ КОНФЕРЕНЦИЯ");
    ui::label(
        &format!(
            "Победители: {}     Проигравшие: {}",
            c.winners.iter().cloned().collect::<Vec<_>>().join(", "),
            c.losers.iter().cloned().collect::<Vec<_>>().join(", ")
        ),
        Rect::new(x + 12., y + 45., wide - 24., 22.),
        13.,
        ui::TEXT,
    );
    let points = c.points.get(player).copied().unwrap_or(0);
    let used = c.spent.get(player).copied().unwrap_or(0);
    ui::label(
        &format!(
            "Очки: {} / {}    |    Требований: {}",
            points.saturating_sub(used),
            points,
            c.demands.len()
        ),
        Rect::new(x + 12., y + 70., wide - 24., 25.),
        15.,
        ui::GOLD,
    );
    if !c.winners.contains(player) {
        ui::label(
            "Страна капитулировала. Ожидается раздел территории победителями.",
            Rect::new(x + 16., y + 108., wide - 32., 45.),
            14.,
            ui::MUTED,
        );
        return;
    }
    let states: Vec<u32> = world
        .states
        .iter()
        .filter(|(id, s)| {
            s.owner.as_ref().is_some_and(|o| c.losers.contains(o))
                && !c.demands.iter().any(|d| d.state == **id)
        })
        .map(|(id, _)| *id)
        .collect();
    let available_rows = (((height - 345.) / 35.).floor() as usize).max(1);
    let visible = available_rows.min(11);
    let max_scroll = states.len().saturating_sub(visible);
    view.scroll = view.scroll.min(max_scroll);
    let list_width = (wide * 0.52).max(175.);
    ui::section("ШТАТЫ ДЛЯ ТРЕБОВАНИЙ", x + 12., y + 104., list_width - 18.);
    if ui::small_button(
        "▲",
        Rect::new(x + list_width - 56., y + 104., 24., 22.),
        false,
    ) {
        view.scroll = view.scroll.saturating_sub(1);
    }
    if ui::small_button(
        "▼",
        Rect::new(x + list_width - 28., y + 104., 24., 22.),
        false,
    ) {
        view.scroll = (view.scroll + 1).min(max_scroll);
    }
    for (index, &id) in states.iter().enumerate().skip(view.scroll).take(visible) {
        let yy = y + 137. + (index - view.scroll) as f32 * 35.;
        let state = &world.states[&id];
        let txt = format!(
            "{} · {} [{}]",
            id,
            state.name.as_deref().unwrap_or("Штат"),
            state.owner.as_deref().unwrap_or("—")
        );
        if ui::small_button(
            &txt,
            Rect::new(x + 12., yy, list_width - 24., 31.),
            view.selected_state == Some(id),
        ) {
            view.selected_state = Some(id);
            view.selected_recipient = 0;
        }
    }
    let rx = x + list_width + 2.;
    let rw = wide - list_width - 15.;
    ui::section("ТРЕБОВАНИЕ", rx, y + 104., rw);
    if let Some(state_id) = view.selected_state.filter(|id| states.contains(id)) {
        let rights = system.rights.get(&state_id).cloned().unwrap_or_default();
        let mut recipients = rights
            .cores
            .into_iter()
            .filter(|tag| tag != player && tag.len() == 3)
            .collect::<Vec<_>>();
        recipients.sort();
        if !recipients.is_empty() {
            view.selected_recipient %= recipients.len();
        }
        let recipient = recipients.get(view.selected_recipient).cloned();
        ui::label(
            &format!("Штат №{}", state_id),
            Rect::new(rx, y + 143., rw, 26.),
            15.,
            ui::TEXT,
        );
        let note = match recipient.as_deref() {
            Some(t) => format!("Получатель: {t}"),
            None => "Нет core-получателя".into(),
        };
        ui::label(&note, Rect::new(rx, y + 175., rw, 25.), 12., ui::MUTED);
        if ui::small_button("◀", Rect::new(rx, y + 203., 36., 30.), false) && !recipients.is_empty()
        {
            view.selected_recipient =
                (view.selected_recipient + recipients.len() - 1) % recipients.len();
        }
        if ui::small_button("▶", Rect::new(rx + 42., y + 203., 36., 30.), false)
            && !recipients.is_empty()
        {
            view.selected_recipient = (view.selected_recipient + 1) % recipients.len();
        }
        let recipient = recipients.get(view.selected_recipient).cloned();
        let choices = [
            ("Аннексировать", Some(PeaceAction::Annex)),
            (
                "Передать владельцу core",
                recipient
                    .clone()
                    .map(|country| PeaceAction::ReturnCore { country }),
            ),
            (
                "Освободить страну",
                recipient
                    .clone()
                    .map(|country| PeaceAction::Liberate { country }),
            ),
            (
                "Создать марионетку",
                recipient.map(|country| PeaceAction::Puppet { country }),
            ),
        ];
        for (i, (label, action)) in choices.into_iter().enumerate() {
            let yy = y + 248. + i as f32 * 39.;
            if yy + 32. > y + height - 84. {
                break;
            }
            if let Some(action) = action {
                let cost = system.demand_price(world, state_id, player, &action).ok();
                let title = format!("{} ({})", label, cost.unwrap_or(0));
                if ui::small_button(&title, Rect::new(rx, yy, rw, 32.), false) {
                    view.notice = system
                        .submit_demand(world, c.id, player, state_id, action)
                        .map(|n| format!("Требование добавлено: -{n} очков"))
                        .unwrap_or_else(|e| e);
                }
            }
        }
    } else {
        ui::label(
            "Выберите штат слева",
            Rect::new(rx, y + 145., rw, 30.),
            14.,
            ui::MUTED,
        );
    }
    let bottom = y + height - 76.;
    if !view.notice.is_empty() {
        ui::label(
            &view.notice,
            Rect::new(x + 12., bottom - 32., wide - 24., 26.),
            12.,
            ui::GOLD,
        );
    }
    ui::label(
        "Цена и распределение очков — предварительная модель; ставки и ИИ ещё не перенесены.",
        Rect::new(x + 12., bottom - 3., wide - 24., 19.),
        11.,
        ui::MUTED,
    );
    let button_width = (wide - 30.) / 2.;
    if ui::small_button(
        "Отменить последнее",
        Rect::new(x + 12., bottom + 24., button_width, 38.),
        false,
    ) {
        view.notice = system
            .withdraw_latest_demand(c.id, player)
            .map(|d| {
                format!(
                    "Требование по штату {} отменено, {} очков возвращено",
                    d.state, d.price
                )
            })
            .unwrap_or_else(|err| err);
    }
    if ui::small_button(
        "Подписать мир",
        Rect::new(x + 18. + button_width, bottom + 24., button_width, 38.),
        false,
    ) {
        view.notice = system
            .finalize(world, military, c.id)
            .map(|_| "Мир подписан, границы изменены".into())
            .unwrap_or_else(|e| e);
        view.selected_state = None;
    }
}
