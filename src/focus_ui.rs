//! Real imported focus list. No fake country branches and no reward on render.
use crate::{campaign::CampaignState,hoi_ui as ui,research::{ResearchCatalogue,ResearchWorld}};
use macroquad::prelude::*;
#[derive(Debug,Clone)]
pub enum Request { Begin(String), Cancel }
pub fn draw(
    tag: Option<&str>, research: &ResearchWorld, state: &CampaignState,
    cat: &ResearchCatalogue, hud: &mut crate::campaign_ui::Hud,
) {
    if hud.active!=Some(crate::campaign_ui::Panel::Focus) {return;}
    let Some(tag)=tag else {return;};
    let layout=crate::ui_layout::Layout::current().wide();
    let x=layout.x+14.;
    let y=layout.y+53.;
    let width=layout.w-28.;
    let height=(layout.bottom()-y-16.).max(1.);
    let original=research.country(tag).map(|c|c.context.original_tag.as_str())
        .filter(|t|!t.is_empty()).unwrap_or(tag);
    let tree=match crate::focus::selected_tree(original,cat) {
        Ok(tree)=>tree,
        Err(error)=>{
            ui::paragraph(&format!("Нет достоверного дерева фокусов: {error}"),
                Rect::new(x,y,width,height),14.,ui::MUTED);
            return;
        }
    };
    ui::label(&format!("{} · {} · {}",tree.id,tag,tree.source),
        Rect::new(x,y,width,23.),13.,ui::GOLD);
    if let Some(active)=state.focuses.active.get(tag) {
        let r=Rect::new(x,y+30.,width.min(565.),50.);
        ui::row_bg(r,true,false);
        ui::label(&format!("Активный фокус: {} · {:.0}/{:.0} дн.",
            active.id,active.progress,active.required),
            Rect::new(r.x+7.,r.y+3.,r.w-112.,20.),12.,ui::TEXT);
        ui::progress_bar(Rect::new(r.x+7.,r.y+29.,r.w-114.,9.),
            (active.progress/active.required) as f32);
        if ui::small_button("ОТМЕНА",
            Rect::new(r.right()-102.,r.y+10.,97.,28.),false) {
            hud.focus_request=Some(Request::Cancel);
        }
        if let Some(reason)=&active.blocked {
            ui::tooltip(r,"Прогресс приостановлен",reason);
        }
    }
    let list_y=y+94.;
    let row_h=72.;
    let view=Rect::new(x,list_y,width,(layout.bottom()-list_y-12.).max(1.));
    let mut ids=tree.focuses.iter().cloned().collect::<Vec<_>>();
    // Preserve original visible imported IDs; no generated placeholder names.
    ids.sort();
    let visible=((view.h/row_h).floor() as usize).max(1);
    let max=ids.len().saturating_sub(visible);
    hud.focus_scroll=hud.focus_scroll.min(max);
    if ui::inside(view) {
        let delta=mouse_wheel().1;
        if delta>0. {hud.focus_scroll=hud.focus_scroll.saturating_sub(2);}
        if delta<0. {hud.focus_scroll=(hud.focus_scroll+2).min(max);}
    }
    ui::scrollbar(Rect::new(view.right()-11.,view.y,9.,view.h),
        &mut hud.focus_scroll,max,visible);
    let clip=ui::clip(view);
    for (offset,id) in ids.iter().skip(hud.focus_scroll).take(visible).enumerate(){
        let r=Rect::new(view.x+1.,view.y+offset as f32*row_h,
            view.w-19.,row_h-4.);
        if r.bottom()>view.bottom()+1. {break;}
        let Some(src)=cat.scripts.sources.get(&format!("focus:{id}")) else {continue;};
        let done=research.country(tag).is_some_and(|c|
            c.context.completed_focuses.contains(id));
        let running=state.focuses.active.get(tag).is_some_and(|f|&f.id==id);
        ui::row_bg(r,running,offset%2==1);
        let icon=Rect::new(r.x+8.,r.y+10.,42.,42.);
        if !crate::hoi_assets::draw("research",id,icon) {
            crate::ui_icons::draw(crate::ui_icons::Icon::Focus,icon,ui::MUTED);
        }
        let name=if src.id.is_empty(){id.as_str()}else{src.id.as_str()};
        ui::label(name,Rect::new(r.x+59.,r.y+5.,(r.w-195.).max(20.),23.),
            13.,if done{ui::GREEN}else{ui::TEXT});
        ui::label(&format!("{} · {}",
            if done{"Завершено"}else if running{"В процессе"}else{"Не начато"},
            src.source),
            Rect::new(r.x+59.,r.y+35.,(r.w-195.).max(20.),17.),
            10.,ui::MUTED);
        let button=Rect::new(r.right()-126.,r.y+16.,119.,30.);
        if done {ui::disabled("ЗАВЕРШЁН",button,"Фокус уже выполнен");}
        else if running {ui::disabled("ВЫБРАН",button,"Фокус выполняется");}
        else if state.focuses.active.contains_key(tag) {
            ui::disabled("НЕДОСТУПНО",button,"Сначала завершите или отмените текущий фокус");
        } else if ui::small_button("НАЧАТЬ",button,false) {
            hud.focus_request=Some(Request::Begin(id.clone()));
        }
        ui::tooltip(r,id,&format!("Оригинальный источник: {}\nСтоимость и условия проверяются при выборе; неизвестная награда блокируется.\nНеподтверждённые DLC-ветки не считаются реализованными.",src.source));
    }
    drop(clip);
    if !hud.notice.is_empty() {
        ui::label(&hud.notice,Rect::new(x,layout.bottom()-16.,width,16.),10.,ui::GOLD);
    }
}
