//! Existing Recruitment navigation renders the real preparation queue.
use crate::{
    army_ui::ArmyView, hoi_ui as ui, production::ProductionState, warfare::Warfare, world::World,
};
use macroquad::prelude::*;

pub fn draw(
    world: &World,
    tag: &str,
    w: &mut Warfare,
    production: &mut ProductionState,
    province: Option<u32>,
    view: &mut ArmyView,
    r: Rect,
) {
    let _clip = ui::clip(r);
    let x = r.x;
    let width = r.w;
    let selected = province.filter(|p| world.controller_of_province(*p) == Some(tag));
    if let Some(p) = selected {
        view.deployment_province = Some(p);
    }
    let location = view
        .deployment_province
        .filter(|p| world.controller_of_province(*p) == Some(tag));
    let mut y = r.y;
    ui::label(
        &location.map_or_else(
            || "Нажмите свою провинцию на карте".into(),
            |p| format!("Развёртывание: провинция {p}"),
        ),
        Rect::new(x, y, width, 24.0),
        13.0,
        ui::GOLD,
    );
    if ui::inside(Rect::new(x, y, width, 24.0)) && is_mouse_button_pressed(MouseButton::Left) {
        view.deployment_pick = true;
        view.open = false;
        view.selected.clear();
        view.notice = "Выберите свою провинцию на карте для развёртывания".into();
        return;
    }
    ui::tooltip(
        Rect::new(x, y, width, 24.0),
        "Место развёртывания",
        "Нажмите строку, затем свою провинцию на карте",
    );
    y += 28.0;
    ui::section("ШАБЛОНЫ · ПОДГОТОВИТЬ НОВУЮ ДИВИЗИЮ", x, y, width);
    y += 28.0;
    let names: Vec<_> = w
        .countries
        .get(tag)
        .map(|a| a.templates.keys().cloned().collect())
        .unwrap_or_default();
    if names.is_empty() {
        ui::label(
            "Нет импортированных шаблонов",
            Rect::new(x, y, width, 26.0),
            12.0,
            ui::MUTED,
        );
        return;
    }
    view.recruit_template = view.recruit_template.min(names.len() - 1);
    if ui::small_button("◀", Rect::new(x, y, 30.0, 28.0), false) {
        view.recruit_template = (view.recruit_template + names.len() - 1) % names.len();
    }
    if ui::small_button("▶", Rect::new(x + width - 30.0, y, 30.0, 28.0), false) {
        view.recruit_template = (view.recruit_template + 1) % names.len();
    }
    let name = &names[view.recruit_template];
    ui::label(
        name,
        Rect::new(x + 35.0, y + 3.0, width - 70.0, 24.0),
        14.0,
        ui::TEXT,
    );
    y += 34.0;
    let a = &w.countries[tag];
    if let Ok(t) = w
        .catalogue
        .aggregate(&a.templates[name], &a.modifiers, None)
    {
        ui::label(
            &format!(
                "{:.0} чел. · {:.0} дней · ширина {:.0}",
                t.stats.manpower, t.stats.training, t.stats.width
            ),
            Rect::new(x, y, width, 22.0),
            12.0,
            ui::TEXT,
        );
    }
    y += 28.0;
    let half = (width - 6.0) / 2.0;
    if ui::small_button("Подготовить +1", Rect::new(x, y, half, 30.0), false) {
        view.notice = location
            .ok_or_else(|| "Сначала выберите свою провинцию на карте".into())
            .and_then(|p| w.queue_recruitment(tag, name, p, world, production))
            .map_or_else(
                |e| e,
                |_| "Набор добавлен: люди зарезервированы, техника поступает со склада".into(),
            );
    }
    if ui::small_button(
        "Изменить шаблон",
        Rect::new(x + half + 6.0, y, half, 30.0),
        false,
    ) {
        view.show_template(view.recruit_template);
        return;
    }
    y += 40.0;
    ui::section("ОЧЕРЕДЬ ПОДГОТОВКИ", x, y, width);
    y += 28.0;
    let jobs: Vec<_> = w
        .recruitment
        .values()
        .filter(|j| j.country == tag)
        .map(|j| j.id)
        .collect();
    let list = Rect::new(x, y, width, (r.bottom() - y).max(0.0));
    let rows = (list.h / 88.0).floor() as usize;
    let max = jobs.len().saturating_sub(rows.max(1));
    view.recruit_scroll = view.recruit_scroll.min(max);
    if ui::inside(list) {
        let wheel = mouse_wheel().1;
        if wheel > 0.0 {
            view.recruit_scroll = view.recruit_scroll.saturating_sub(1);
        } else if wheel < 0.0 {
            view.recruit_scroll = (view.recruit_scroll + 1).min(max);
        }
    }
    ui::scrollbar(list, &mut view.recruit_scroll, max, rows);
    if jobs.is_empty() {
        ui::paragraph(
            "Выберите шаблон и нажмите «Подготовить +1». Подготовка идёт по игровым часам после получения оснащения. Готовая дивизия появится в выбранной провинции.",
            list,
            13.0,
            ui::MUTED,
        );
    }
    for (slot, id) in jobs.iter().skip(view.recruit_scroll).take(rows).enumerate() {
        let job = w.recruitment[id].clone();
        let supply = w.recruitment_equipment_fraction(&job);
        let row = Rect::new(x, y + slot as f32 * 88.0, width - 12.0, 83.0);
        ui::row_bg(row, false, slot % 2 == 0);
        ui::label(
            &format!("{} · P{}", job.template.name, job.province),
            Rect::new(x + 5.0, row.y + 4.0, row.w - 40.0, 21.0),
            13.0,
            ui::TEXT,
        );
        if ui::small_button(
            "×",
            Rect::new(row.right() - 29.0, row.y + 4.0, 25.0, 23.0),
            false,
        ) {
            view.notice = w
                .cancel_recruitment(tag, *id, production)
                .map_or_else(|e| e, |_| "Набор отменён, люди и техника возвращены".into());
        }
        ui::progress_bar(
            Rect::new(x + 5.0, row.y + 32.0, row.w - 10.0, 7.0),
            (job.training_hours / job.required_hours) as f32,
        );
        ui::label(
            &format!(
                "Подготовка {:.0}% · Техника {:.0}%",
                job.training_hours / job.required_hours * 100.0,
                supply * 100.0
            ),
            Rect::new(x + 5.0, row.y + 43.0, row.w - 10.0, 19.0),
            11.0,
            ui::GOLD,
        );
        let state = if world.controller_of_province(job.province) != Some(tag) {
            "Провинция потеряна · выберите другую"
        } else if supply < 1.0 {
            "Ожидает оснащение · производство → склад"
        } else {
            "Обучение · развёртывание автоматически"
        };
        let early_ready = job.training_hours / job.required_hours >= 0.2 && supply >= 0.2;
        ui::label(
            state,
            Rect::new(
                x + 5.0,
                row.y + 63.0,
                row.w - if early_ready { 70.0 } else { 10.0 },
                17.0,
            ),
            10.0,
            ui::MUTED,
        );
        if early_ready
            && ui::small_button(
                "Рано",
                Rect::new(row.right() - 58.0, row.y + 60.0, 53.0, 20.0),
                false,
            )
        {
            view.notice = w
                .deploy_recruitment_early(tag, *id, world)
                .map_or_else(|err| err, |_| "Дивизия досрочно развёрнута".into());
        }
        if let Some(p) = location.filter(|p| *p != job.province) {
            ui::tooltip(
                row,
                "Изменить место развёртывания",
                &format!("ПКМ по строке: провинция {p}"),
            );
            if ui::inside(row) && is_mouse_button_pressed(MouseButton::Right) {
                view.notice = w
                    .change_deployment(tag, *id, p, world)
                    .map_or_else(|e| e, |_| "Место развёртывания изменено".into());
            }
        }
    }
}
