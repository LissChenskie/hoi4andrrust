//! Original procedural research panel. Displays the imported tech DAG,
//! conditions, multiple parallel slots and day-step ETA without bundling art.
use crate::{draw_ui_text,measure_ui_text};
use crate::hoi_ui as ui;
use crate::game::GameDate;
use crate::research::{CountryResearch,ResearchCatalogue,ResearchRules,ResearchWorld};
use macroquad::prelude::*;

#[derive(Debug,Default)]
pub struct ResearchView {
    pub open:bool,
    pub folder_index:usize,
    pub selected:Option<String>,
    pub scroll_rows:usize,
    pub scroll_columns:usize,
    pub preferred_slot:Option<usize>,
    pub notice:String,
}

pub fn bounds()->Rect {
    let compact=screen_width()<760.0;
    let x=if compact {7.0}else{12.0};
    let w=if compact {(screen_width()-14.0).max(1.0)} else {
        (screen_width()*0.73).clamp(630.0,1010.0).min(screen_width()-24.0)
    };
    let y=107.0;
    Rect::new(x,y,w,(screen_height()-y-if compact{71.0}else{13.0}).max(210.0))
}
pub fn is_over_panel(point:Vec2,view:&ResearchView)->bool {
    view.open&&bounds().contains(point)
}
fn fit(text:&str,width:f32,size:u16)->String {
    let mut s=text.to_owned();
    while !s.is_empty() && measure_ui_text(&s,size).width>width {s.pop();}
    s
}
fn label_color(tech_id:&str, country:&CountryResearch,
    catalogue:&ResearchCatalogue)->Color {
    if country.researched.contains(tech_id){ui::GREEN}
    else if country.slots.iter().any(|s|s.active.as_ref().is_some_and(|a|a.tech_id==tech_id)){ui::GOLD}
    else if catalogue.can_research(tech_id,country).is_ok(){ui::TEXT}
    else{ui::MUTED}
}

pub fn draw(
    tag:Option<&str>, research:&mut ResearchWorld, catalogue:&ResearchCatalogue,
    rules:&ResearchRules, date:GameDate, view:&mut ResearchView,
){
    if !view.open{return;}
    let r=bounds();
    ui::panel(r.x,r.y,r.w,r.h);
    ui::header(r.x+3.0,r.y+3.0,r.w-6.0,"RESEARCH");
    if ui::small_button("X",Rect::new(r.x+r.w-36.0,r.y+8.0,26.0,25.0),false) {
        view.open=false;return;
    }
    let Some(tag)=tag else {
        draw_ui_text("Select a country first",r.x+14.0,r.y+76.0,13.0,ui::MUTED);
        return;
    };
    let Some(country)=research.country(tag) else {
        draw_ui_text("Research has not been initialized for this country",r.x+14.0,r.y+76.0,12.0,ui::MUTED);
        return;
    };
    let compact=screen_width()<760.0;
    let mut command:Option<(usize,String)>=None;
    let mut selected=view.selected.clone();
    let mut available_slot=None;
    let slots_y=r.y+47.0;
    let slot_height=if compact{37.0}else{40.0};
    let shown_slots=country.slots.len().min(if compact{4}else{6});
    for (i,slot) in country.slots.iter().take(shown_slots).enumerate(){
        let y=slots_y+i as f32*slot_height;
        let rect=Rect::new(r.x+10.0,y,r.w-20.0,slot_height-3.0);
        ui::row_bg(rect,slot.active.is_some(),i%2==1);
        let (description,progress) = if let Some(active)=&slot.active {
            let tech=catalogue.technologies.get(&active.tech_id);
            let total=tech.map_or(1.0,|t|t.research_cost*rules.base_tech_cost);
            let eta=country.estimated_days(i,catalogue,rules,date)
                .and_then(|days|date.after_days(days))
                .map_or("?".into(),GameDate::display);
            (format!("{}  {}  ETA {}",i+1,
                tech.map_or(active.tech_id.as_str(),|t|t.name.as_str()),eta),
                (active.progress/total).clamp(0.0,1.0) as f32)
        }else{
            available_slot=available_slot.or(Some(i));
            (format!("SLOT {}  EMPTY  +{:.0}/{} SAVED DAYS",
                i+1,slot.saved_days,rules.max_saved_days as u32),0.0)
        };
        let font=if compact{10}else{12};
        draw_ui_text(&fit(&description,rect.w-70.0,font),rect.x+8.0,rect.y+15.0,font as f32,ui::TEXT);
        ui::progress_bar(Rect::new(rect.x+8.0,rect.y+slot_height-15.0,rect.w-70.0,7.0),progress);
        if let Some(active)=&slot.active {
            if ui::small_button("STOP",Rect::new(rect.x+rect.w-55.0,rect.y+4.0,49.0,27.0),false){
                command=Some((i,String::new()));
            }
            if ui::inside(rect) && is_mouse_button_pressed(MouseButton::Left){
                selected=Some(active.tech_id.clone());
            }
        }else if ui::small_button("FREE",Rect::new(rect.x+rect.w-55.0,rect.y+4.0,49.0,27.0),
            view.preferred_slot==Some(i)){
            view.preferred_slot=Some(i);
            available_slot=Some(i);
        }
    }
    if let Some(preferred)=view.preferred_slot {
        if preferred<shown_slots && country.slots[preferred].active.is_none(){
            available_slot=Some(preferred);
        }else{
            view.preferred_slot=None;
        }
    }
    if country.slots.len()>shown_slots {
        draw_ui_text(&format!("+{} slots not shown",country.slots.len()-shown_slots),
            r.x+12.0,slots_y+shown_slots as f32*slot_height+13.0,10.0,ui::MUTED);
    }

    let tabs_y=slots_y+shown_slots as f32*slot_height+if country.slots.len()>shown_slots{19.0}else{5.0};
    let tabs=Rect::new(r.x+12.0,tabs_y,r.w-24.0,29.0);
    let folders:Vec<String>=catalogue.folders.iter().cloned().collect();
    if folders.is_empty(){
        draw_ui_text("No technologies loaded",r.x+13.0,tabs_y+25.0,12.0,ui::MUTED);
        return;
    }
    view.folder_index=view.folder_index.min(folders.len()-1);
    let btn=if compact{31.0}else{37.0};
    if ui::small_button("<",Rect::new(tabs.x,tabs.y,btn,tabs.h),false) {
        view.folder_index=view.folder_index.saturating_sub(1);view.scroll_rows=0;view.scroll_columns=0;
    }
    if ui::small_button(">",Rect::new(tabs.x+tabs.w-btn,tabs.y,btn,tabs.h),false) {
        view.folder_index=(view.folder_index+1).min(folders.len()-1);view.scroll_rows=0;view.scroll_columns=0;
    }
    let folder=&folders[view.folder_index];
    draw_ui_text(&fit(&format!("{}   {}/{}",folder.to_uppercase(),
        view.folder_index+1,folders.len()),tabs.w-2.0*btn-12.0,13),
        tabs.x+btn+9.0,tabs.y+19.0,13.0,ui::GOLD);

    let tree_top=tabs.y+36.0;
    let details_h=if compact{102.0}else{115.0};
    let tree_h=(r.y+r.h-details_h-tree_top-8.0).max(25.0);
    let tree_rect=Rect::new(r.x+12.0,tree_top,r.w-24.0,tree_h);
    draw_rectangle(tree_rect.x,tree_rect.y,tree_rect.w,tree_rect.h,ui::BASE);
    draw_rectangle_lines(tree_rect.x,tree_rect.y,tree_rect.w,tree_rect.h,1.0,ui::EDGE);
    let node_w: f32=if compact{117.0}else{152.0};
    let node_h: f32=if compact{48.0}else{53.0};
    let row_step=node_h+17.0;
    let nodes:Vec<_>=catalogue.technologies.values().filter(|tech|tech.folder==*folder
        && catalogue.visible(&tech.id,country)).collect();
    let max_row=nodes.iter().map(|tech|tech.y.max(0.0) as usize).max().unwrap_or(0);
    let max_col=nodes.iter().map(|tech|tech.x.max(0.0) as usize).max().unwrap_or(0);
    let rows_visible=(tree_h/row_step).ceil().max(1.0) as usize;
    let col_step=node_w+12.0;
    let cols_visible=(tree_rect.w/col_step).floor().max(1.0) as usize;
    view.scroll_rows=view.scroll_rows.min(max_row.saturating_sub(rows_visible.saturating_sub(1)));
    view.scroll_columns=view.scroll_columns.min(max_col.saturating_sub(cols_visible.saturating_sub(1)));
    let coords=|x:f32,y:f32|->Vec2{
        vec2(tree_rect.x+12.0+(x.max(0.0)-view.scroll_columns as f32)*col_step,
            tree_rect.y+9.0+(y.max(0.0)-view.scroll_rows as f32)*row_step)
    };
    // Parent links are rendered before nodes. Arrow color follows completion.
    for tech in &nodes{
        let dest=coords(tech.x,tech.y);
        for parent in &tech.prerequisites{
            if let Some(p)=catalogue.technologies.get(parent).filter(|p|p.folder==*folder){
                let start=coords(p.x,p.y);
                if start.y+node_h>=tree_rect.y && dest.y<=tree_rect.y+tree_h
                    && start.x>=tree_rect.x && start.x+node_w<=tree_rect.x+tree_rect.w
                    && dest.x>=tree_rect.x && dest.x+node_w<=tree_rect.x+tree_rect.w {
                    draw_line(start.x+node_w*0.5,start.y+node_h,
                        dest.x+node_w*0.5,dest.y,2.0,
                        if country.researched.contains(parent){ui::GREEN}else{ui::EDGE});
                }
            }
        }
    }
    for tech in &nodes{
        let pos=coords(tech.x,tech.y);
        if pos.y<tree_rect.y || pos.y+node_h>tree_rect.y+tree_rect.h
            || pos.x<tree_rect.x || pos.x+node_w>tree_rect.x+tree_rect.w {continue;}
        let area=Rect::new(pos.x,pos.y,node_w.min(tree_rect.w-20.0),node_h);
        let done=country.researched.contains(&tech.id);
        let active=country.slots.iter().any(|slot|slot.active.as_ref()
            .is_some_and(|a|a.tech_id==tech.id));
        ui::row_bg(area,selected.as_deref()==Some(tech.id.as_str()),false);
        draw_rectangle_lines(area.x,area.y,area.w,area.h,2.0,
            label_color(&tech.id,country,catalogue));
        draw_ui_text(&fit(&tech.name,area.w-8.0,if compact{10}else{11}),
            area.x+5.0,area.y+17.0,if compact{10.0}else{11.0},ui::TEXT);
        draw_ui_text(&format!("{}  {}  {}",tech.start_year,
            if done{"DONE"}else if active{"WORKING"}else{"RESEARCH"},
            tech.research_cost),area.x+5.0,area.y+33.0,9.0,
            label_color(&tech.id,country,catalogue));
        if ui::inside(area)&&is_mouse_button_pressed(MouseButton::Left) {
            selected=Some(tech.id.clone());
        }
    }
    let inside_tree=ui::inside(tree_rect);
    if inside_tree{
        let (_,dy)=mouse_wheel();
        if dy>0.0{view.scroll_rows=view.scroll_rows.saturating_sub(1);}
        else if dy<0.0{view.scroll_rows=(view.scroll_rows+1).min(max_row);}
    }
    if ui::small_button("<",Rect::new(tree_rect.x+3.0,tree_rect.y+3.0,20.0,22.0),false) {
        view.scroll_columns=view.scroll_columns.saturating_sub(1);
    }
    if ui::small_button(">",Rect::new(tree_rect.x+27.0,tree_rect.y+3.0,20.0,22.0),false) {
        view.scroll_columns=(view.scroll_columns+1).min(max_col);
    }
    if ui::small_button("^",Rect::new(tree_rect.x+tree_rect.w-24.0,tree_rect.y+3.0,20.0,22.0),false){
        view.scroll_rows=view.scroll_rows.saturating_sub(1);
    }
    if ui::small_button("v",Rect::new(tree_rect.x+tree_rect.w-24.0,tree_rect.y+28.0,20.0,22.0),false){
        view.scroll_rows=(view.scroll_rows+1).min(max_row);
    }

    view.selected=selected;
    let details_y=tree_rect.y+tree_rect.h+8.0;
    if let Some(selected)=view.selected.as_ref(){
        if let Some(tech)=catalogue.technologies.get(selected){
            let prereq=if tech.prerequisites.is_empty(){"none".into()}else{
                tech.prerequisites.iter().cloned().collect::<Vec<_>>().join(", ")
            };
            let preview_eta=available_slot.and_then(|slot|{
                let mut simulation=country.clone();
                simulation.start(slot,&tech.id,catalogue,rules,date).ok()?;
                simulation.estimated_days(slot,catalogue,rules,date)
            });
            let preview=preview_eta.map_or("LOCKED".to_owned(),|days|{
                let date_label=date.after_days(days).map_or("?".to_owned(),GameDate::display);
                format!("{}d / {}",days,date_label)
            });
            draw_ui_text(&fit(&format!("{}  |  YEAR {}  |  BASE {:.0}d  |  ETA {}",
                tech.name,tech.start_year,tech.research_cost*rules.base_tech_cost,preview),
                r.w-32.0,if compact{10}else{13}),r.x+14.0,details_y+15.0,
                if compact{10.0}else{13.0},ui::GOLD);
            draw_ui_text(&fit(&format!("Required: {prereq}  |  {}", 
                catalogue.can_research(&tech.id,country).err().unwrap_or("AVAILABLE")),
                r.w-32.0,10),r.x+14.0,details_y+32.0,10.0,ui::MUTED);
            let x=r.x+14.0;
            let max_button=(r.w-32.0).min(580.0);
            if let Some(slot)=available_slot {
                if ui::small_button(&format!("RESEARCH IN SLOT {}",slot+1),
                    Rect::new(x,details_y+42.0,max_button*0.54,31.0),false){
                    command=Some((slot,tech.id.clone()));
                }
            }
            let summary=if !tech.description.is_empty(){
                tech.description.as_str().to_owned()
            }else{
                format!("Unlocks: {}",tech.unlocks.iter().cloned().collect::<Vec<_>>().join(", "))
            };
            draw_ui_text(&fit(&summary,r.w-33.0,10),
                x,details_y+88.0,10.0,ui::MUTED);
        }
    }else {
        draw_ui_text("Select a technology from the tree",r.x+14.0,details_y+20.0,
            12.0,ui::MUTED);
    }
    if !view.notice.is_empty(){
        draw_ui_text(&fit(&view.notice,r.w-36.0,11),r.x+14.0,
            (r.y+r.h-10.0).max(details_y+75.0),11.0,ui::GOLD);
    }

    if let Some((slot,id))=command {
        let Some(c)=research.countries.get_mut(tag)else{return;};
        if id.is_empty(){
            if c.cancel(slot){view.notice=format!("Slot {} stopped",slot+1);}
        }else{
            match c.start(slot,&id,catalogue,rules,date){
                Ok(())=>view.notice=format!("Started: {id}"),
                Err(error)=>view.notice=error.into(),
            }
        }
    }
}
