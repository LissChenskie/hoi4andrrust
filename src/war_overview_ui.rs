//! Battle/war status sheet using the actual campaign's belligerents.
//! Cumulative land/air/naval losses are not yet persisted by the simulation.
//! Display the available recent land losses without mislabeling them as totals.
use crate::campaign_ui::Hud;
use crate::hoi_ui as ui;
use crate::peace::PeaceSystem;
use crate::research::ResearchWorld;
use crate::warfare::Warfare;
use crate::world::World;
use macroquad::prelude::*;

pub fn badge_bounds() -> Rect {
    let width = (screen_width() * 0.25).clamp(135.0, 210.0);
    Rect::new((screen_width() - width) * 0.5, 35.0, width, 31.0)
}
pub fn modal_bounds() -> Rect {
    let width = (screen_width() - 24.0).min(640.0).max(280.0);
    let height = (screen_height() - 40.0).min(520.0).max(300.0);
    Rect::new((screen_width() - width) * 0.5, (screen_height() - height) * 0.5, width, height)
}
pub fn draw(
    hud: &mut Hud,
    world: &World,
    warfare: &Warfare,
    peace: &PeaceSystem,
    research: &ResearchWorld,
    player: Option<&str>,
) {
    let Some(player) = player else { hud.war_panel_open = false; return; };
    let mut enemies = warfare.wars.iter().filter_map(|(a,b)| {
        if a == player { Some(b.clone()) } else if b == player { Some(a.clone()) } else { None }
    }).collect::<Vec<_>>();
    enemies.sort();
    enemies.dedup();
    if enemies.is_empty() { hud.war_panel_open = false; return; }
    if ui::small_button(
        if hud.war_panel_open { "В ВОЙНЕ · ЗАКРЫТЬ" } else { "В ВОЙНЕ · СВОДКА" },
        badge_bounds(), hud.war_panel_open
    ) { hud.war_panel_open = !hud.war_panel_open; }
    if !hud.war_panel_open { return; }
    let rect = modal_bounds();
    draw_rectangle(0.,0.,screen_width(),screen_height(),
        Color::new(0.,0.,0.,0.57));
    ui::panel(rect.x,rect.y,rect.w,rect.h);
    ui::header(rect.x + 3., rect.y + 3.,rect.w - 6.,"ВОЕННАЯ СВОДКА");
    if ui::small_button("X",Rect::new(rect.right() - 34., rect.y + 8., 25., 23.), false) {
        hud.war_panel_open = false;
        return;
    }
    let own = peace.surrender_status(world,warfare,research,player);
    let own_info = own.map_or_else(
        || "Капитуляция: нет импортированных национальных VP".into(),
        |s|format!("Капитуляция {:.1}% / порог {:.1}% · контролируется {:.1}/{:.1} VP",
            s.progress*100.,s.limit*100.,s.controlled_vp,s.total_vp)
    );
    ui::label(&format!("{player} · противников: {}",enemies.len()),
        Rect::new(rect.x+14.,rect.y+48.,rect.w-28.,24.),15.,ui::GOLD);
    ui::label(&own_info,Rect::new(rect.x+14.,rect.y+77.,rect.w-28.,24.),
        12.,ui::TEXT);
    if let Some(s) = own { ui::progress_bar(Rect::new(rect.x+14.,rect.y+103.,rect.w-28.,8.),s.progress as f32); }
    ui::section("ПРОТИВНИКИ И ИХ ОЧКИ ПОБЕДЫ",rect.x+14.,rect.y+125.,rect.w-28.);
    let available = ((rect.h-208.) / 67.).max(0.) as usize;
    for (i,opponent) in enemies.iter().take(available).enumerate() {
        let y = rect.y+160.+i as f32*67.;
        let r = Rect::new(rect.x+12.,y,rect.w-24.,61.);
        ui::row_bg(r,false,i%2==1);
        let status = peace.surrender_status(world,warfare,research,opponent);
        let text = status.map_or_else(
            || "Нет статуса: не импортированы cores/VP".into(),
            |s|format!("Капитуляция {:.1}% / {:.1}% · VP {:.1}/{:.1}",
                s.progress*100.,s.limit*100.,s.controlled_vp,s.total_vp));
        ui::label(opponent,Rect::new(r.x+8.,r.y+4.,r.w-16.,21.),14.,ui::GOLD);
        ui::label(&text,Rect::new(r.x+8.,r.y+24.,r.w-16.,20.),11.,ui::TEXT);
        if let Some(s)=status {ui::progress_bar(Rect::new(r.x+8.,r.y+48.,r.w-16.,6.),s.progress as f32);}
    }
    ui::label("Общие потери пока не агрегируются по войнам; сейчас нет достоверного счётчика.",
        Rect::new(rect.x+13.,rect.bottom()-39.,rect.w-26.,32.),11.,ui::MUTED);
}
