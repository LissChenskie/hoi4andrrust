//! Battle summary uses the same participant IDs and hourly state as combat.
use crate::{army_ui::ArmyView, hoi_ui as ui, land_combat::BattleOutcome, warfare::Warfare};
use macroquad::prelude::*;
pub fn draw(tag: &str, w: &mut Warfare, view: &mut ArmyView, r: Rect) {
    let _clip = ui::clip(r);
    let ids: Vec<_> = w
        .battles
        .iter()
        .filter(|(_, b)| {
            b.origins
                .keys()
                .chain(&b.defenders.active)
                .chain(&b.defenders.reserves)
                .any(|id| w.divisions.get(id).is_some_and(|d| d.country == tag))
        })
        .map(|(id, _)| *id)
        .collect();
    if ids.is_empty() {
        ui::paragraph(
            "Сражений пока нет. Во время атаки нажмите круглый значок боя рядом с дивизиями на карте.",
            r,
            14.0,
            ui::MUTED,
        );
        return;
    }
    let id = view
        .battle_id
        .filter(|id| ids.contains(id))
        .unwrap_or(ids[0]);
    let at = ids.iter().position(|b| *b == id).unwrap_or(0);
    let mut y = r.y;
    if ui::small_button("◀", Rect::new(r.x, y, 29.0, 27.0), false) {
        view.battle_id = Some(ids[(at + ids.len() - 1) % ids.len()]);
        view.battle_scroll = 0;
    }
    if ui::small_button("▶", Rect::new(r.right() - 29.0, y, 29.0, 27.0), false) {
        view.battle_id = Some(ids[(at + 1) % ids.len()]);
        view.battle_scroll = 0;
    }
    let b = w.battles.get_mut(&id).unwrap();
    b.trace_enabled = true;
    let b = &w.battles[&id];
    ui::label(
        &format!("Провинция {} · {} ч", b.province, b.elapsed_hours),
        Rect::new(r.x + 35.0, y + 4.0, r.w - 70.0, 23.0),
        14.0,
        ui::GOLD,
    );
    y += 36.0;
    let outcome = match b.outcome {
        None => "Сражение продолжается",
        Some(BattleOutcome::AttackerWon) => "Победа атакующих",
        Some(BattleOutcome::DefenderWon) => "Победа обороняющихся",
        Some(BattleOutcome::Draw) => "Бой окончен",
    };
    ui::label(outcome, Rect::new(r.x, y, r.w, 22.0), 14.0, ui::TEXT);
    y += 28.0;
    let half = (r.w - 8.0) / 2.0;
    for (i, (title, side)) in [("АТАКУЮЩИЕ", &b.attackers), ("ОБОРОНЯЮЩИЕСЯ", &b.defenders)]
        .into_iter()
        .enumerate()
    {
        let x = r.x + i as f32 * (half + 8.0);
        ui::section(title, x, y, half);
        let units: Vec<_> = side
            .active
            .iter()
            .chain(&side.reserves)
            .filter_map(|id| w.divisions.get(id))
            .collect();
        let org = units
            .iter()
            .map(|d| d.organization / d.derived.stats.organization.max(1.0))
            .sum::<f64>()
            / units.len().max(1) as f64;
        ui::label(
            &format!(
                "{} в бою · {} резерв",
                side.active.len(),
                side.reserves.len()
            ),
            Rect::new(x, y + 27.0, half, 19.0),
            12.0,
            ui::TEXT,
        );
        ui::progress_bar(Rect::new(x, y + 51.0, half, 8.0), org as f32);
        ui::tooltip(
            Rect::new(x, y + 49.0, half, 14.0),
            "Организация стороны",
            &format!("{:.0}% от максимальной", org * 100.0),
        );
    }
    y += 74.0;
    if let Some(h) = b.history.back() {
        ui::label(
            &format!(
                "Ширина {:.0} · занято {:.0} / {:.0}",
                h.width, h.attacker_width, h.defender_width
            ),
            Rect::new(r.x, y, r.w, 22.0),
            12.0,
            ui::GOLD,
        );
        y += 24.0;
        ui::tooltip(
            Rect::new(r.x, y - 24., r.w, 23.),
            "Авиация за этот час",
            &format!(
                "CAS: {} / {}\nВражеское превосходство: {:.0}% / {:.0}%\nУменьшение CAS через ПВО: {:.0}% / {:.0}%",
                h.attacker_air.cas_planes,
                h.defender_air.cas_planes,
                h.attacker_air.enemy_superiority * 100.,
                h.defender_air.enemy_superiority * 100.,
                h.attacker_air.aa_damage_reduction * 100.,
                h.defender_air.aa_damage_reduction * 100.
            ),
        );
        let tactics = |id: &str| {
            if id.is_empty() || id == "neutral/unavailable" {
                "—".to_owned()
            } else {
                w.catalogue
                    .localisation
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| id.into())
            }
        };
        ui::label(
            &format!(
                "Тактики: {} / {}",
                tactics(&h.attacker_tactic),
                tactics(&h.defender_tactic)
            ),
            Rect::new(r.x, y, r.w, 21.0),
            11.0,
            ui::TEXT,
        );
        y += 24.0;
        ui::label(
            &format!(
                "За час: {} чел. · {} ед. техники",
                h.manpower_losses, h.equipment_losses
            ),
            Rect::new(r.x, y, r.w, 21.0),
            12.0,
            ui::MUTED,
        );
        y += 27.0;
    }
    if ui::small_button(
        "Подробный почасовой журнал",
        Rect::new(r.x, y, r.w, 28.0),
        false,
    ) {
        view.show_battle_log(id, w);
    }
    y += 36.0;
    let troops: Vec<_> = [("Атака", &b.attackers), ("Оборона", &b.defenders)]
        .into_iter()
        .flat_map(|(side, s)| {
            s.active
                .iter()
                .map(move |id| (side, *id, false))
                .chain(s.reserves.iter().map(move |id| (side, *id, true)))
        })
        .collect();
    let list = Rect::new(r.x, y, r.w, (r.bottom() - y).max(0.0));
    let rows = (list.h / 48.0).floor() as usize;
    let max = troops.len().saturating_sub(rows.max(1));
    view.battle_scroll = view.battle_scroll.min(max);
    if ui::inside(list) {
        let dy = mouse_wheel().1;
        if dy > 0.0 {
            view.battle_scroll = view.battle_scroll.saturating_sub(1);
        } else if dy < 0.0 {
            view.battle_scroll = (view.battle_scroll + 1).min(max);
        }
    }
    ui::scrollbar(list, &mut view.battle_scroll, max, rows);
    for (slot, (side, id, reserve)) in troops
        .iter()
        .skip(view.battle_scroll)
        .take(rows)
        .enumerate()
    {
        let Some(d) = w.divisions.get(id) else {
            continue;
        };
        let row = Rect::new(r.x, y + slot as f32 * 48.0, r.w - 12.0, 45.0);
        ui::row_bg(row, false, slot % 2 == 0);
        ui::label(
            &format!(
                "{} · {}{}",
                side,
                d.name,
                if *reserve { " · резерв" } else { "" }
            ),
            Rect::new(row.x + 4.0, row.y + 3.0, row.w - 8.0, 20.0),
            12.0,
            ui::TEXT,
        );
        let hw = (row.w - 15.0) / 2.0;
        ui::progress_bar(
            Rect::new(row.x + 5.0, row.y + 29.0, hw, 6.0),
            d.strength() as f32,
        );
        ui::progress_bar(
            Rect::new(row.x + 10.0 + hw, row.y + 29.0, hw, 6.0),
            (d.organization / d.derived.stats.organization.max(1.0)) as f32,
        );
        ui::tooltip(
            row,
            &d.name,
            &format!(
                "{} · HP {:.1}/{:.1} · Организация {:.1}/{:.1} · снабжение {:.0}% · топливо {:.0}%\n{}",
                d.country,
                d.hp,
                d.derived.stats.hp,
                d.organization,
                d.derived.stats.organization,
                d.supply_fraction * 100.0,
                d.fuel_fraction * 100.0,
                w.logistics
                    .divisions
                    .get(id)
                    .map_or(String::new(), |s| format!(
                        "{} · запас {:.2} · износ {} ед.",
                        s.reason, s.supply, s.equipment_lost
                    ))
            ),
        );
    }
}
