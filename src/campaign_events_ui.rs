//! Cached, paginated decisions/missions and the existing pending-event options.
use crate::{
    campaign::{CampaignState, Scope, decisions::DecisionRow},
    hoi_ui as ui,
    research::{ResearchCatalogue, ResearchRules},
    session::GameSession,
};
use macroquad::prelude::*;
#[derive(Default)]
pub struct View {
    key: Option<(String, u64, u64)>,
    pub rows: Vec<DecisionRow>,
    pub events: bool,
}
impl View {
    pub fn refresh(&mut self, s: &GameSession, cat: &ResearchCatalogue, rules: &ResearchRules) {
        let Some(t) = s.player_country.as_deref() else {
            self.rows.clear();
            self.key = None;
            return;
        };
        let key = (
            t.to_owned(),
            s.clock.elapsed_hours,
            s.campaign.decisions.revision,
        );
        if self.key.as_ref() == Some(&key) {
            return;
        }
        self.rows = s.decision_rows(t, cat, rules);
        self.key = Some(key);
    }
}
pub fn draw(tag: Option<&str>, state: &CampaignState, hud: &mut crate::campaign_ui::Hud) {
    if hud.active != Some(crate::campaign_ui::Panel::Decisions) {
        return;
    }
    let Some(tag) = tag else {
        return;
    };
    let panel = crate::ui_layout::Layout::current().primary();
    let x = panel.x + 12.;
    let width = panel.w - 24.;
    let mut y = panel.y + 48.;
    let pending: Vec<_> = state
        .pending
        .values()
        .filter(|e| matches!(&e.context.scope,Scope::Country(t) if t==tag))
        .collect();
    let half = (width - 6.) / 2.;
    if ui::small_button(
        &format!("Решения ({})", hud.decision_view.rows.len()),
        Rect::new(x, y, half, 28.),
        !hud.decision_view.events,
    ) {
        hud.decision_view.events = false;
        hud.scroll = 0;
    }
    if ui::small_button(
        &format!("События ({})", pending.len()),
        Rect::new(x + half + 6., y, half, 28.),
        hud.decision_view.events,
    ) {
        hud.decision_view.events = true;
        hud.scroll = 0;
    }
    y += 38.;
    let bottom = panel.bottom() - 70.;
    let pages;
    if !hud.decision_view.events {
        let page_size = (((bottom - y) / 142.).floor() as usize).max(1);
        let count = hud.decision_view.rows.len();
        pages = count.div_ceil(page_size).max(1);
        hud.scroll = hud.scroll.min(pages - 1);
        if count == 0 {
            ui::paragraph(
                "Нет видимых решений и активных миссий.",
                Rect::new(x, y, width, 50.),
                12.,
                ui::MUTED,
            );
        }
        for row in hud
            .decision_view
            .rows
            .iter()
            .skip(hud.scroll * page_size)
            .take(page_size)
        {
            ui::section(&format!("{} · {}", row.category, row.title), x, y, width);
            ui::paragraph(
                &row.description,
                Rect::new(x, y + 26., width, 40.),
                11.,
                ui::TEXT,
            );
            ui::paragraph(
                &row.status,
                Rect::new(x, y + 68., width, 32.),
                11.,
                if row.status.starts_with("Заблокировано") {
                    Color::from_rgba(230, 130, 110, 255)
                } else {
                    ui::MUTED
                },
            );
            if !row.button.is_empty()
                && ui::small_button(
                    &row.button,
                    Rect::new(x, y + 104., width, 28.),
                    row.selectable,
                )
                && row.selectable
            {
                hud.decision_request = Some(row.id.clone());
            }
            y += 142.;
        }
    } else {
        let blocked: Vec<_> = state
            .queue
            .values()
            .filter(|a| {
                a.blocked.is_some() && matches!(&a.context.scope, Scope::Global)
                    || (a.blocked.is_some()
                        && matches!(&a.context.scope,Scope::Country(t) if t==tag))
            })
            .collect();
        let count = pending.len() + blocked.len();
        pages = count.max(1);
        hud.scroll = hud.scroll.min(pages - 1);
        if count == 0 {
            ui::paragraph(
                "Нет ожидающих событий.",
                Rect::new(x, y, width, 50.),
                12.,
                ui::MUTED,
            );
        }
        if let Some(event) = pending.get(hud.scroll) {
            ui::section(&event.event, x, y, width);
            y += 30.;
            for (option, raw) in event.options.iter().enumerate() {
                if y + 31. > bottom {
                    break;
                }
                let name = crate::pdx::atom(raw, "name")
                    .map_or_else(|| format!("Вариант {}", option + 1), str::to_owned);
                if ui::small_button(&name, Rect::new(x, y, width, 28.), false) {
                    hud.event_request = Some((event.id, option));
                }
                y += 34.;
            }
        } else if let Some(a) = blocked.get(hud.scroll.saturating_sub(pending.len())) {
            ui::section("СЦЕНАРИЙ ЗАБЛОКИРОВАН", x, y, width);
            ui::paragraph(
                &format!("{}\n{}", a.source, a.blocked.as_deref().unwrap_or("")),
                Rect::new(x, y + 30., width, (bottom - y - 30.).max(30.)),
                11.,
                ui::MUTED,
            );
        }
    }
    let y = panel.bottom() - 65.;
    if ui::small_button("←", Rect::new(x, y, 40., 27.), false) {
        hud.scroll = hud.scroll.saturating_sub(1);
    }
    ui::label(
        &format!("{} / {}", hud.scroll + 1, pages),
        Rect::new(x + 45., y, width - 90., 27.),
        12.,
        ui::MUTED,
    );
    if ui::small_button("→", Rect::new(x + width - 40., y, 40., 27.), false) {
        hud.scroll = (hud.scroll + 1).min(pages - 1);
    }
    if !hud.notice.is_empty() {
        ui::paragraph(
            &hud.notice,
            Rect::new(x, panel.bottom() - 32., width, 27.),
            10.,
            ui::GOLD,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        campaign::{Context, CountryPolitics},
        pdx::parse_pdx,
        research_script::DecisionRecord,
    };
    #[test]
    fn cached_rows_refresh_for_same_hour_effects_without_drawing_mutating_campaign() {
        let mut s = GameSession::new(crate::warfare::tests::world());
        s.campaign.countries.insert(
            "A".into(),
            CountryPolitics {
                power: Some(10.),
                ..Default::default()
            },
        );
        s.player_country = Some("A".into());
        let mut cat = ResearchCatalogue::default();
        cat.scripts
            .decision_categories
            .insert("c".into(), DecisionRecord::default());
        cat.scripts.decisions.insert(
            "d".into(),
            DecisionRecord {
                category: "c".into(),
                raw: parse_pdx("cost=3 available={has_country_flag=ready}").unwrap(),
                ..Default::default()
            },
        );
        s.initialize_decisions(&cat, &Default::default());
        let before = serde_json::to_value(&s).unwrap();
        let mut view = View::default();
        view.refresh(&s, &cat, &Default::default());
        assert!(!view.rows[0].selectable);
        view.refresh(&s, &cat, &Default::default());
        assert_eq!(before, serde_json::to_value(&s).unwrap());
        s.campaign_effects(
            &Context::country("A"),
            &parse_pdx("set_country_flag=ready").unwrap(),
            "authored",
            &cat,
            &Default::default(),
        )
        .unwrap();
        view.refresh(&s, &cat, &Default::default());
        assert!(view.rows[0].selectable);
        assert!(view.rows[0].button.contains("3 PP"));
        assert_eq!(s.campaign.countries["A"].power, Some(10.));
    }
}
