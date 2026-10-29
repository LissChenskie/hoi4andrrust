//! Scrollable division editor. Draft edits are pure; Save is an XP transaction
//! and assignment/reinforcement operate on the existing campaign inventories.
use crate::{
    hoi_ui as ui,
    military_model::{BattalionSlot, DivisionTemplate, SupportSlot},
    production::ProductionState,
    warfare::Warfare,
    world::World,
};
use macroquad::prelude::*;
use std::collections::BTreeSet;
#[derive(Debug, Default)]
pub struct DesignerView {
    pub template_index: usize,
    pub draft: Option<DivisionTemplate>,
    pub editing: Option<String>,
    pub name_active: bool,
    new_draft: bool,
    page: usize,
    scroll: f32,
    drag: Option<Vec2>,
    slot: (usize, usize),
    support: bool,
    regimental: bool,
    picker: bool,
    picker_scroll: usize,
}
fn unique_name(w: &Warfare, tag: &str, base: &str) -> String {
    let army = &w.countries[tag];
    if !army.templates.contains_key(base) {
        return base.into();
    }
    for i in 2..10000 {
        let name = format!("{base} {i}");
        if !army.templates.contains_key(&name) {
            return name;
        }
    }
    format!("{base} {}", army.templates.len() + 1)
}

fn open_slot(v: &mut DesignerView, column: usize, row: usize, support: bool,
             regimental: bool) {
    v.slot = (column,row);
    v.support = support;
    v.regimental = regimental;
    v.picker = true;
    v.picker_scroll = 0;
}
fn slot_button(
    w: &Warfare, draft: &DivisionTemplate, v: &mut DesignerView,
    position: (usize,usize), support: bool, regimental: bool,
    cell: Rect, enabled: bool,
) {
    let id = if support {
        draft.support.iter()
            .find(|s| s.regiment == regimental.then_some(position.0)
                && s.row == position.1).map(|s| s.unit.as_str())
    } else {
        draft.line.iter()
            .find(|s| s.column == position.0 && s.row == position.1)
            .map(|s| s.unit.as_str())
    };
    let selected = v.slot == position && v.support == support
        && v.regimental == regimental && v.picker;
    if !enabled {
        ui::disabled("🔒",cell,
            "Слот не открыт текущим количеством батальонов или правилами игры");
        return;
    }
    if ui::small_button(if id.is_none() { "+" } else { "" },cell,selected) {
        open_slot(v,position.0,position.1,support,regimental);
    }
    if let Some(id) = id {
        let group = w.catalogue.units.get(id).map_or("infantry",|u|u.group.as_str());
        let icon = Rect::new(cell.x+cell.w*.16,cell.y+2.,cell.w*.68,(cell.h-7.).max(1.));
        crate::division_icons::draw(id,group,icon);
        // The popup contains the full localized unit label; do not squeeze it
        // under 25 battalion icons, particularly on Android portrait layouts.
        ui::tooltip(cell,w.catalogue.label(id),
            &format!("{} · Нажмите для изменения",id));
    } else {
        ui::tooltip(cell,"Пустой слот","Нажмите для выбора изученного подразделения");
    }
}
fn draw_picker(
    w: &Warfare, draft: &DivisionTemplate,
    modifiers: &crate::military_model::MilitaryModifiers,
    v: &mut DesignerView, area: Rect,
) {
    ui::section("ВЫБОР ПОДРАЗДЕЛЕНИЯ",area.x,area.y,area.w);
    if ui::small_button("НАЗАД",Rect::new(area.right()-79.,area.y,77.,22.),false) {
        v.picker = false;
        v.picker_scroll = 0;
        return;
    }
    let mut candidates = w.catalogue.units.values()
        .filter(|u| if v.support {
            u.group == "support" && if v.regimental { u.regimental } else { u.divisional }
        } else { u.group != "support" })
        .collect::<Vec<_>>();
    candidates.sort_by(|a,b|
        a.group.cmp(&b.group).then_with(||a.id.cmp(&b.id)));
    let top=area.y+29.;
    let row_h=38.;
    let visible = ((area.h-31.)/row_h).floor().max(1.) as usize;
    // First entry removes the selected component and never costs XP until Save.
    let total = candidates.len()+1;
    let max_scroll=total.saturating_sub(visible);
    v.picker_scroll=v.picker_scroll.min(max_scroll);
    if ui::inside(Rect::new(area.x,top,area.w,area.h-30.)) {
        let wheel=mouse_wheel().1;
        if wheel>0. {v.picker_scroll=v.picker_scroll.saturating_sub(2);}
        if wheel<0. {v.picker_scroll=(v.picker_scroll+2).min(max_scroll);}
    }
    ui::scrollbar(Rect::new(area.right()-9.,top,8.,(area.h-32.).max(1.)),
        &mut v.picker_scroll,max_scroll,visible);
    for index in v.picker_scroll..(v.picker_scroll+visible).min(total) {
        let row=Rect::new(area.x+2.,
            top+(index-v.picker_scroll) as f32*row_h,area.w-14.,row_h-3.);
        if row.y+row.h>area.bottom()+1. {break;}
        let mut changed=draft.clone();
        if v.support {
            let regiment=v.regimental.then_some(v.slot.0);
            changed.support.retain(|s|!(s.regiment==regiment && s.row==v.slot.1));
            if index>0 {
                changed.support.push(SupportSlot{
                    unit:candidates[index-1].id.clone(),regiment,row:v.slot.1});
            }
        } else {
            changed.line.retain(|s| !(s.column==v.slot.0 && s.row==v.slot.1));
            if index>0 {
                changed.line.push(BattalionSlot{
                    unit:candidates[index-1].id.clone(),
                    column:v.slot.0,row:v.slot.1});
            }
        }
        let label = if index==0 {"Удалить из слота".to_owned()}
            else {w.catalogue.label(&candidates[index-1].id).to_owned()};
        match w.catalogue.validate_template(&changed,modifiers,true) {
            Ok(()) => {
                if ui::small_button("",row,false) {
                    v.draft=Some(changed);
                    v.picker=false;
                    v.picker_scroll=0;
                }
                if index>0 {
                    let u=candidates[index-1];
                    crate::division_icons::draw(&u.id,&u.group,
                        Rect::new(row.x+4.,row.y+2.,30.,row.h-4.));
                }
                ui::label(&label,Rect::new(row.x+40.,row.y+6.,row.w-47.,24.),
                    11.,ui::TEXT);
                ui::tooltip(row,&label,
                    "Доступно по исследованиям, типу полка и ограничениям поддержки");
            }
            Err(reason) => {
                ui::disabled(&label,row,&reason);
            }
        }
    }
}
fn draw_preview(
    preview: &Result<crate::military_model::DerivedTemplate,String>,
    area: Rect,
) {
    ui::section("ХАРАКТЕРИСТИКИ ДИВИЗИИ",area.x,area.y,area.w);
    match preview {
        Err(message) => {
            ui::paragraph(message,Rect::new(area.x+6.,area.y+34.,
                area.w-12.,120.),12.,ui::GOLD);
        }
        Ok(d) => {
            let stats=&d.stats;
            let fields=[
                ("Организация", stats.organization),
                ("Боевой фронт",stats.width),
                ("Скорость",stats.speed),
                ("HP",stats.hp),
                ("Защита",stats.defense),
                ("Прорыв",stats.breakthrough),
                ("Мягкая атака",stats.soft_attack),
                ("Твёрдая атака",stats.hard_attack),
                ("Броня",stats.armor),
                ("Пробитие",stats.piercing),
                ("Твёрдость %",stats.hardness*100.),
                ("Снабжение",stats.supply),
                ("Топливо",stats.fuel),
                ("Людские ресурсы",stats.manpower),
                ("Производство IC",d.equipment_cost),
            ];
            for (index,(name,value)) in fields.iter().enumerate() {
                let yy=area.y+28.+index as f32*22.;
                if yy+22.>area.bottom()-3. {break;}
                let cell=Rect::new(area.x+2.,yy,area.w-4.,21.);
                ui::row_bg(cell,false,index%2==1);
                ui::label(name,Rect::new(cell.x+4.,cell.y+3.,
                    (cell.w-64.).max(1.),17.),11.,ui::TEXT);
                ui::label(&format!("{value:.1}"),
                    Rect::new(cell.right()-60.,cell.y+3.,58.,17.),11.,ui::GOLD);
            }
        }
    }
}
fn draw_composition(
    w: &Warfare, draft: &DivisionTemplate,
    modifiers: &crate::military_model::MilitaryModifiers,
    preview: &Result<crate::military_model::DerivedTemplate,String>,
    v: &mut DesignerView, area: Rect, notice: &mut String,
) {
    let wide=area.w>=615.;
    let left_w=if wide {area.w*.61} else {area.w};
    let left=Rect::new(area.x,area.y,left_w-3.,area.h);
    let sidebar=if wide {
        Rect::new(area.x+left_w+5.,area.y,(area.w-left_w-5.).max(1.),area.h)
    } else {area};
    if v.picker && !wide {
        draw_picker(w,draft,modifiers,v,area);
        return;
    }
    let grid_top=area.y+28.-v.scroll;
    ui::section("БОЕВЫЕ БАТАЛЬОНЫ",left.x,area.y-v.scroll,left.w);
    let has_reg=w.catalogue.units.values().any(|u|u.regimental);
    let gap=5.;
    let side_width=if wide {79.} else {5.};
    let cell=((left.w-side_width-4.*gap-10.)/5.).clamp(25.,72.);
    let step=cell+gap;
    let columns=w.catalogue.rules.regiment_columns.min(5);
    let rows=w.catalogue.rules.regiment_rows.min(5);
    for col in 0..5 {
        for row in 0..5 {
            let rect=Rect::new(left.x+col as f32*step,grid_top+row as f32*step,
                cell,cell);
            if rect.bottom()<area.y || rect.y>area.bottom() {continue;}
            slot_button(w,draft,v,(col,row),false,false,
                rect,col<columns && row<rows);
        }
    }
    // Five divisional support companies live next to combat battalions in the
    // full-width editor, and beneath them on a phone-size screen.
    let sy=if wide {grid_top} else {grid_top+5.*step+33.};
    let sx=if wide {left.x+5.*step+3.} else {left.x};
    ui::section("ПОДДЕРЖКА",sx,sy-25.,
        if wide {(left.right()-sx).max(26.)}else{left.w});
    for row in 0..5 {
        let r=if wide {
            Rect::new(sx,sy+row as f32*step,(left.right()-sx-2.).max(24.),cell)
        } else {
            Rect::new(sx+row as f32*step,sy,cell,cell)
        };
        if r.bottom()<area.y || r.y>area.bottom() {continue;}
        slot_button(w,draft,v,(0,row),true,false,r,
            row<w.catalogue.rules.support_slots);
    }
    let reg_y=if wide {grid_top+5.*step+37.} else {sy+step+47.};
    if has_reg {
        ui::section("ПОЛКОВАЯ ПОДДЕРЖКА",left.x,reg_y-27.,left.w);
        for col in 0..5 {
            let count=draft.line.iter().filter(|s|s.column==col).count();
            let active=count>=3;
            let r=Rect::new(left.x+col as f32*step,reg_y,cell,cell);
            if r.bottom()>area.y && r.y<area.bottom() {
                slot_button(w,draft,v,(col,0),true,true,r,
                    col<columns && (active || draft.support.iter()
                        .any(|s|s.regiment==Some(col))));
                ui::tooltip(r,"Полковая поддержка",
                    &format!("Полк {}: {count}/3 батальона",col+1));
            }
        }
    }
    if wide {
        if v.picker {draw_picker(w,draft,modifiers,v,sidebar);}
        else {draw_preview(preview,sidebar);}
    } else if !has_reg && draft.support.is_empty() {
        ui::label("Роты поддержки открываются технологиями",
            Rect::new(left.x,reg_y+3.,left.w,22.),12.,ui::MUTED);
    }
    let _=notice;
}
pub fn draw(
    world: &World,
    tag: &str,
    w: &mut Warfare,
    production: &mut ProductionState,
    province: Option<u32>,
    selected: &BTreeSet<u64>,
    v: &mut DesignerView,
    notice: &mut String,
    r: Rect,
) {
    let Some(army) = w.countries.get(tag) else {
        return;
    };
    let names: Vec<_> = army.templates.keys().cloned().collect();
    let modifiers = army.modifiers.clone();
    let xp = army.army_xp;
    if v.draft.is_none() {
        if !names.is_empty() {
            v.template_index = v.template_index.min(names.len() - 1);
            v.editing = Some(names[v.template_index].clone());
            v.draft = Some(army.templates[&names[v.template_index]].clone());
            v.new_draft = false;
        } else {
            v.new_draft = true;
            v.editing = None;
            v.draft = Some(DivisionTemplate {
                name: "Новый шаблон".into(),
                icon: "infantry".into(),
                ..Default::default()
            });
        }
    }
    let x = r.x;
    let width = r.w;
    let mut y = r.y;
    let third = (width - 12.0) / 4.0;
    if ui::small_button("НОВЫЙ", Rect::new(x, y, third, 26.0), false) {
        v.draft = Some(DivisionTemplate {
            name: unique_name(w, tag, "Новый шаблон"),
            icon: "infantry".into(),
            ..Default::default()
        });
        v.new_draft = true;
        v.editing = None;
        v.scroll = 0.0;
    }
    if ui::small_button("КОПИЯ", Rect::new(x + third + 4.0, y, third, 26.0), false) {
        if let Some(old) = v.editing.clone() {
            let name = unique_name(w, tag, &format!("{old} (копия)"));
            match w.copy_template(tag, &old, &name) {
                Ok(()) => {
                    v.draft = Some(w.countries[tag].templates[&name].clone());
                    v.editing = Some(name);
                    v.new_draft = false;
                    *notice = "Копия создана без затрат XP".into();
                }
                Err(e) => *notice = e,
            }
        }
    }
    if ui::small_button("ОТКАТ",Rect::new(x + 2.0 * (third + 4.),y,third,26.),false) {
        if let Some(saved) = v.editing.as_ref().and_then(|id|w.countries[tag].templates.get(id)) {
            v.draft = Some(saved.clone());
            *notice = "Несохранённые изменения отменены".into();
        } else {
            v.draft = Some(DivisionTemplate {
                name: v.draft.as_ref().map_or("Новый шаблон".into(),|d|d.name.clone()),
                icon: "infantry".into(), ..Default::default()
            });
            *notice = "Черновик очищен".into();
        }
        v.picker = false;
    }
    if ui::small_button(
        "ЗНАЧОК",
        Rect::new(x + 3.0 * (third + 4.0), y, third, 26.0),
        false,
    ) {
        let t = v.draft.as_mut().unwrap();
        t.icon = match t.icon.as_str() {
            "infantry" => "armor",
            "armor" => "motorized",
            "motorized" => "artillery",
            _ => "infantry",
        }
        .into();
    }
    y += 32.0;
    if !v.new_draft && !names.is_empty() {
        if ui::small_button("◀", Rect::new(x, y, 25.0, 27.0), false) {
            v.template_index = (v.template_index + names.len() - 1) % names.len();
            v.draft = None;
            v.editing = None;
            v.scroll = 0.0;
            return;
        }
        if ui::small_button("▶", Rect::new(x + width - 25.0, y, 25.0, 27.0), false) {
            v.template_index = (v.template_index + 1) % names.len();
            v.draft = None;
            v.editing = None;
            v.scroll = 0.0;
            return;
        }
    }
    let name = v.draft.as_ref().unwrap().name.clone();
    if ui::small_button(
        &name,
        Rect::new(x + 30.0, y, width - 60.0, 27.0),
        v.name_active,
    ) {
        v.name_active = true;
        macroquad::miniquad::window::show_keyboard(true);
    }
    if v.name_active {
        let t = v.draft.as_mut().unwrap();
        while let Some(ch) = get_char_pressed() {
            if !ch.is_control() && t.name.chars().count() < 64 {
                t.name.push(ch);
            }
        }
        if is_key_pressed(KeyCode::Backspace) {
            t.name.pop();
        }
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Escape) {
            v.name_active = false;
            macroquad::miniquad::window::show_keyboard(false);
        }
    }
    y += 34.0;
    let titles = ["СОСТАВ", "ПАРАМЕТРЫ", "ТЕХНИКА", "МЕСТНОСТЬ"];
    let tab = width / 4.0;
    for (i, title) in titles.iter().enumerate() {
        if ui::small_button(
            title,
            Rect::new(x + i as f32 * tab, y, tab - 3.0, 26.0),
            v.page == i,
        ) {
            v.page = i;
            v.scroll = 0.0;
            v.picker = false;
        }
    }
    y += 32.0;
    let body = Rect::new(x, y, width, (r.bottom() - y - 70.0).max(1.0));
    let draft = v.draft.as_ref().unwrap().clone();
    let preview = w.catalogue.aggregate(&draft, &modifiers, None);
    let mut total = if v.page == 0 { 580.0 } else { 650.0 };
    if ui::inside(body) && (v.page != 0 || !v.picker) {
        let (_, wheel) = mouse_wheel();
        v.scroll = (v.scroll - wheel * 36.).clamp(0.,(total - body.h).max(0.));
    }
    if ui::inside(body) && is_mouse_button_pressed(MouseButton::Left)
        && !touches().is_empty() {
        v.drag = Some(vec2(mouse_position().0, mouse_position().1));
    }
    if let Some(last) = v.drag {
        if is_mouse_button_down(MouseButton::Left) {
            let current = vec2(mouse_position().0, mouse_position().1);
            v.scroll = (v.scroll + last.y - current.y).clamp(0.,(total-body.h).max(0.));
            v.drag = Some(current);
        } else { v.drag = None; }
    }
    let clip = ui::clip(body);
    let top = body.y - v.scroll;
    if v.page == 0 {
        draw_composition(w, &draft, &modifiers, &preview, v, body, notice);
    } else {
        match &preview {
            Err(error) => ui::paragraph(error, Rect::new(x, top, width, 100.0), 13.0, ui::GOLD),
            Ok(d) => {
                if v.page == 1 {
                    let s = &d.stats;
                    let values = [
                        ("Организация", s.organization),
                        ("HP", s.hp),
                        ("Восстановление / ч", s.recovery),
                        ("Soft Attack", s.soft_attack),
                        ("Hard Attack", s.hard_attack),
                        ("Защита", s.defense),
                        ("Прорыв", s.breakthrough),
                        ("Броня", s.armor),
                        ("Пробитие", s.piercing),
                        ("Hardness, %", s.hardness * 100.0),
                        ("Ширина боя", s.width),
                        ("Инициатива", s.initiative),
                        ("Разведка", s.recon),
                        ("Окапывание", s.entrenchment),
                        ("Скорость, км/ч", s.speed),
                        ("Снабжение", s.supply),
                        ("Топливо", s.fuel),
                        ("Надёжность, %", s.reliability * 100.0),
                        ("Людские ресурсы", s.manpower),
                        ("Стоимость IC", d.equipment_cost),
                        ("Обучение, дней", s.training),
                        ("Вес", s.weight),
                        ("ПВО", s.air_attack),
                    ];
                    for (i, (name, value)) in values.iter().enumerate() {
                        let row = Rect::new(x, top + i as f32 * 23.0, width, 22.0);
                        ui::row_bg(row, false, i % 2 == 1);
                        ui::label(
                            name,
                            Rect::new(row.x + 4.0, row.y + 2.0, row.w * 0.7, 19.0),
                            12.0,
                            ui::TEXT,
                        );
                        ui::label(
                            &format!("{value:.2}"),
                            Rect::new(row.x + row.w * 0.7, row.y + 2.0, row.w * 0.3 - 4.0, 19.0),
                            12.0,
                            ui::GOLD,
                        );
                    }
                } else if v.page == 2 {
                    let mut yy = top;
                    for (archetype, quantity) in &d.equipment {
                        let models: Vec<_> = w
                            .catalogue
                            .equipment
                            .values()
                            .filter(|e| {
                                e.archetype == *archetype
                                    && w.catalogue.equipment_available(&e.id, &modifiers)
                            })
                            .collect();
                        let preferred = draft
                            .equipment_preferences
                            .get(archetype)
                            .cloned()
                            .or_else(|| {
                                w.catalogue
                                    .best_equipment(archetype, &modifiers)
                                    .map(|e| e.id.clone())
                            });
                        let label = format!("{}: {:.0}", w.catalogue.label(archetype), quantity);
                        ui::label(&label, Rect::new(x, yy, width, 20.0), 12.0, ui::TEXT);
                        yy += 23.0;
                        let title = preferred
                            .as_ref()
                            .map_or("Нет доступной модели", |id| {
                                w.catalogue.label(id)
                            });
                        if ui::small_button(title, Rect::new(x, yy, width, 28.0), false)
                            && !models.is_empty()
                        {
                            let at = preferred
                                .as_ref()
                                .and_then(|id| models.iter().position(|e| e.id == *id))
                                .unwrap_or(0);
                            v.draft.as_mut().unwrap().equipment_preferences.insert(
                                archetype.clone(),
                                models[(at + 1) % models.len()].id.clone(),
                            );
                        }
                        yy += 36.0;
                    }
                } else {
                    for (i, (terrain, bonus)) in d.terrain.iter().enumerate() {
                        let yy = top + i as f32 * 49.0;
                        ui::label(terrain, Rect::new(x, yy, width, 20.0), 13.0, ui::GOLD);
                        ui::label(
                            &format!(
                                "А {:+.0}%  З {:+.0}%  скорость {:+.0}%  потери {:+.0}%",
                                bonus.attack * 100.0,
                                bonus.defense * 100.0,
                                bonus.movement * 100.0,
                                bonus.attrition * 100.0
                            ),
                            Rect::new(x, yy + 22.0, width, 20.0),
                            11.0,
                            ui::TEXT,
                        );
                    }
                }
            }
        }
    }
    drop(clip);
    let current = v.draft.as_ref().unwrap().clone();
    let cost = w.template_edit_cost(tag, v.editing.as_deref(), &current);
    let footer = r.bottom() - 64.0;
    let half = (width - 5.0) / 2.0;
    let title = format!(
        "Сохранить · {} XP",
        cost.as_ref().map_or("?".into(), |c| format!("{c:.0}"))
    );
    let reason = cost.as_ref().err().cloned().or_else(|| {
        cost.as_ref()
            .ok()
            .filter(|c| **c > xp + 1e-9)
            .map(|c| format!("Нужно {c:.0} XP, доступно {xp:.1}"))
    });
    if let Some(reason) = reason {
        ui::disabled(&title, Rect::new(x, footer, half, 28.0), &reason);
    } else if ui::small_button(&title, Rect::new(x, footer, half, 28.0), false) {
        match w.save_template(tag, v.editing.as_deref(), current.clone()) {
            Ok(()) => {
                let ids: Vec<_> = w
                    .divisions
                    .values()
                    .filter(|d| d.country == tag && d.template == current.name && !d.destroyed)
                    .map(|d| d.id)
                    .collect();
                for id in ids {
                    w.return_surplus(id, production);
                }
                v.editing = Some(current.name.clone());
                v.new_draft = false;
                v.template_index = w.countries[tag]
                    .templates
                    .keys()
                    .position(|n| *n == current.name)
                    .unwrap_or(0);
                *notice = "Шаблон сохранён; оснащение пополняется из запасов страны".into();
            }
            Err(e) => *notice = e,
        }
    }
    if ui::small_button(
        "Применить к выбранным",
        Rect::new(x + half + 5.0, footer, half, 28.0),
        false,
    ) {
        let ids: Vec<_> = selected.iter().copied().collect();
        *notice = w
            .apply_template(tag, &ids, &current.name, production)
            .map_or_else(
                |e| e,
                |_| "Шаблон назначен; недостающее оснащение запрошено".into(),
            );
    }
    if ui::small_button(
        "В очередь подготовки",
        Rect::new(x, footer + 34.0, width, 27.0),
        false,
    ) {
        *notice = province
            .ok_or_else(|| "Выберите свою провинцию".into())
            .and_then(|p| w.queue_recruitment(tag, &current.name, p, world, production))
            .map_or_else(|e| e, |_| "Набор добавлен в очередь подготовки".into());
    }
}
