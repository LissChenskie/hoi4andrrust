//! Pending events already exist in campaign scheduler; show their real options.
use crate::{campaign::{CampaignState,Scope},hoi_ui as ui};
use macroquad::prelude::*;
pub fn draw(tag:Option<&str>,state:&CampaignState,hud:&mut crate::campaign_ui::Hud){
    if hud.active!=Some(crate::campaign_ui::Panel::Decisions){return;}
    let Some(tag)=tag else {return;};
    let panel=crate::ui_layout::Layout::current().primary();
    let x=panel.x+12.;
    let y=panel.y+50.;
    let width=panel.w-24.;
    ui::section("ОЖИДАЮЩИЕ СОБЫТИЯ",x,y,width);
    let pending=state.pending.values().filter(|e|
        matches!(&e.context.scope,Scope::Country(t) if t==tag)).collect::<Vec<_>>();
    if pending.is_empty(){
        ui::paragraph("Нет ожидающих решений по событиям. Импортированные решения и миссии с длительными таймерами ещё требуют отдельного исполнителя.",
            Rect::new(x,y+35.,width,120.),12.,ui::MUTED);
    }
    let mut yy=y+31.;
    for event in pending.iter().take(5){
        if yy+90.>=panel.bottom()-28. {break;}
        ui::section(&format!("{} · {}",event.event,event.source),x,yy,width);
        yy+=29.;
        for (option,raw) in event.options.iter().enumerate(){
            if yy+31.>panel.bottom()-28. {break;}
            let name=crate::pdx::atom(raw,"name")
                .map_or_else(||format!("Вариант {}",option+1),str::to_owned);
            if ui::small_button(&name,Rect::new(x+4.,yy,width-8.,28.),false) {
                hud.event_request=Some((event.id,option));
            }
            yy+=34.;
        }
        yy+=12.;
    }
    if !hud.notice.is_empty(){
        ui::label(&hud.notice,Rect::new(x,panel.bottom()-27.,width,20.),
            11.,ui::GOLD);
    }
}
