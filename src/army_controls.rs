//! Shared geometry and selection for map counters. No frame/input globals:
//! rendering and hit testing use exactly the same rectangles.
use crate::military_model::DivisionId;
use crate::warfare::Warfare;
use crate::world::World;
use macroquad::prelude::{Rect, Vec2, vec2};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default)]
pub struct OrderReport {
    pub accepted: usize,
    pub errors: Vec<String>,
}
impl OrderReport {
    pub fn message(&self) -> String {
        if self.errors.is_empty() {
            format!("Orders accepted: {}", self.accepted)
        } else {
            format!(
                "{} ordered; {} blocked: {}",
                self.accepted,
                self.errors.len(),
                self.errors[0]
            )
        }
    }
}
pub fn order_selection(
    w: &mut Warfare,
    world: &World,
    tag: &str,
    selected: &BTreeSet<u64>,
    target: u32,
    strategic: bool,
    append: bool,
) -> OrderReport {
    let mut groups = BTreeMap::<u32, Vec<u64>>::new();
    let mut report = OrderReport::default();
    for &id in selected {
        if let Some(d) = w
            .divisions
            .get(&id)
            .filter(|d| d.country == tag && !d.destroyed)
        {
            let from = if append {
                d.waypoints
                    .back()
                    .copied()
                    .or(d.destination)
                    .map_or(d.province, |o| o.province)
            } else {
                d.province
            };
            groups.entry(from).or_default().push(id);
        } else {
            report.errors.push("Unknown own division".into());
        }
    }
    for ids in groups.into_values() {
        match w.issue_move(tag, &ids, target, world, strategic, append) {
            Ok(()) => report.accepted += ids.len(),
            Err(error) if ids.len() == 1 => report.errors.push(error),
            Err(_) => {
                for id in ids {
                    match w.issue_move(tag, &[id], target, world, strategic, append) {
                        Ok(()) => report.accepted += 1,
                        Err(e) => report.errors.push(e),
                    }
                }
            }
        }
    }
    report
}
pub fn stop_selection(
    w: &mut Warfare,
    world: &World,
    tag: &str,
    selected: &BTreeSet<u64>,
) -> OrderReport {
    let mut report = OrderReport::default();
    for &id in selected {
        match w.stop(tag, id, world) {
            Ok(()) => report.accepted += 1,
            Err(e) => report.errors.push(e),
        }
    }
    report
}

#[derive(Debug, Clone)]
pub struct Counter {
    pub province: u32,
    pub country: String,
    pub ids: Vec<DivisionId>,
    pub rect: Rect,
}
pub fn project(world: &World, province: u32, source: Rect, area: Rect) -> Option<Vec2> {
    let p = world.geography.provinces.get(&province)?.center?;
    Some(vec2(
        area.x + (p.x as f32 - source.x) / source.w * area.w,
        area.y + (p.y as f32 - source.y) / source.h * area.h,
    ))
}
pub fn counters(world: &World, w: &Warfare, source: Rect, area: Rect) -> Vec<Counter> {
    let mut stacks = BTreeMap::<(u32, &str), Vec<u64>>::new();
    for d in w.divisions.values().filter(|d| !d.destroyed) {
        stacks
            .entry((d.province, &d.country))
            .or_default()
            .push(d.id);
    }
    let mut offsets = BTreeMap::<u32, usize>::new();
    stacks
        .into_iter()
        .filter_map(|((province, country), ids)| {
            let p = project(world, province, source, area)?;
            if !area.contains(p) {
                return None;
            }
            let index = offsets.entry(province).or_default();
            let y = p.y + *index as f32 * 30.0;
            *index += 1;
            let x = (p.x - 22.0).clamp(area.x, (area.right() - 44.0).max(area.x));
            let y = (y - 13.0).clamp(area.y, (area.bottom() - 26.0).max(area.y));
            Some(Counter {
                province,
                country: country.into(),
                ids,
                rect: Rect::new(x, y, 44.0, 26.0),
            })
        })
        .collect()
}
pub fn hit<'a>(counters: &'a [Counter], tag: &str, point: Vec2) -> Option<&'a Counter> {
    counters
        .iter()
        .filter(|c| c.country == tag && c.rect.contains(point))
        .min_by(|a, b| {
            a.rect
                .center()
                .distance_squared(point)
                .total_cmp(&b.rect.center().distance_squared(point))
        })
}
pub fn target(counters: &[Counter], point: Vec2) -> Option<u32> {
    counters
        .iter()
        .filter(|c| c.rect.contains(point))
        .min_by(|a, b| {
            a.rect
                .center()
                .distance_squared(point)
                .total_cmp(&b.rect.center().distance_squared(point))
        })
        .map(|c| c.province)
}
pub fn select_counter(selected: &mut BTreeSet<u64>, counter: &Counter, add: bool, all: bool) {
    if counter.ids.is_empty() {
        return;
    }
    if all {
        if !add {
            selected.clear();
        }
        selected.extend(counter.ids.iter().copied());
        return;
    }
    let id = if add {
        counter
            .ids
            .iter()
            .find(|id| !selected.contains(id))
            .copied()
            .unwrap_or(counter.ids[0])
    } else if selected.len() == 1 {
        counter
            .ids
            .iter()
            .position(|id| selected.contains(id))
            .map(|i| counter.ids[(i + 1) % counter.ids.len()])
            .unwrap_or(counter.ids[0])
    } else {
        counter.ids[0]
    };
    if add {
        if !selected.insert(id) {
            selected.remove(&id);
        }
    } else {
        selected.clear();
        selected.insert(id);
    }
}
pub fn selection_rect(start: Vec2, end: Vec2) -> Rect {
    Rect::new(
        start.x.min(end.x),
        start.y.min(end.y),
        (start.x - end.x).abs(),
        (start.y - end.y).abs(),
    )
}
pub fn select_box(
    selected: &mut BTreeSet<u64>,
    counters: &[Counter],
    tag: &str,
    rect: Rect,
    add: bool,
    blocked: &[Rect],
) {
    if !add {
        selected.clear();
    }
    for c in counters.iter().filter(|c| {
        c.country == tag
            && rect.contains(c.rect.center())
            && !blocked.iter().any(|r| r.contains(c.rect.center()))
    }) {
        selected.extend(c.ids.iter().copied());
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Drag {
    pub start: Vec2,
    pub current: Vec2,
    pub last: Vec2,
    pub touch: bool,
    pub add: bool,
    pub box_select: bool,
}
impl Drag {
    pub fn new(start: Vec2, touch: bool, add: bool, box_select: bool) -> Self {
        Self {
            start,
            current: start,
            last: start,
            touch,
            add,
            box_select,
        }
    }
    pub fn moved(&self) -> bool {
        self.start.distance_squared(self.current) >= 16.0
    }
    pub fn update(&mut self, position: Vec2) -> Vec2 {
        let delta = position - self.last;
        self.current = position;
        self.last = position;
        delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_busy_unit_does_not_block_the_rest_of_a_map_selection() {
        let mut world = crate::warfare::tests::world();
        world.set_province_controller(2, Some("A"));
        let mut c = crate::military_model::tests::catalogue();
        c.rules.pixels_to_km = Some(10.0);
        let mut w = Warfare::new(c);
        w.divisions
            .insert(1, crate::military_model::tests::division(1, "A", 1));
        let mut busy = crate::military_model::tests::division(2, "A", 1);
        busy.order = crate::military_model::DivisionOrder::Retreat {
            destination: 2,
            progress_km: 0.0,
        };
        w.divisions.insert(2, busy);
        let report = order_selection(
            &mut w,
            &world,
            "A",
            &BTreeSet::from([1, 2]),
            2,
            false,
            false,
        );
        assert_eq!(report.accepted, 1);
        assert_eq!(report.errors.len(), 1);
        assert!(matches!(
            w.divisions[&1].order,
            crate::military_model::DivisionOrder::Move { .. }
        ));
        assert!(matches!(
            w.divisions[&2].order,
            crate::military_model::DivisionOrder::Retreat { .. }
        ));
    }
    fn counter(ids: &[u64], country: &str, x: f32) -> Counter {
        Counter {
            ids: ids.to_vec(),
            country: country.into(),
            province: 1,
            rect: Rect::new(x, 20.0, 44.0, 26.0),
        }
    }
    #[test]
    fn click_selects_one_cycles_a_stack_and_shift_adds_without_losing_previous_selection() {
        let c = counter(&[1, 2, 3], "A", 0.0);
        let mut selected = BTreeSet::new();
        select_counter(&mut selected, &c, false, false);
        assert_eq!(selected, BTreeSet::from([1]));
        select_counter(&mut selected, &c, false, false);
        assert_eq!(selected, BTreeSet::from([2]));
        select_counter(&mut selected, &counter(&[4], "A", 0.0), true, false);
        assert_eq!(selected, BTreeSet::from([2, 4]));
        select_counter(&mut selected, &c, false, true);
        assert_eq!(selected, BTreeSet::from([1, 2, 3]));
    }
    #[test]
    fn rectangle_is_direction_independent_never_selects_foreign_or_obscured_counters() {
        let counters = vec![
            counter(&[1, 2], "A", 0.0),
            counter(&[3], "B", 50.0),
            counter(&[4], "A", 100.0),
        ];
        let mut selected = BTreeSet::from([8]);
        let r = selection_rect(vec2(160.0, 70.0), vec2(0.0, 0.0));
        select_box(
            &mut selected,
            &counters,
            "A",
            r,
            false,
            &[Rect::new(100.0, 0.0, 60.0, 70.0)],
        );
        assert_eq!(selected, BTreeSet::from([1, 2]));
        assert!(hit(&counters, "A", vec2(60.0, 30.0)).is_none());
        assert_eq!(
            hit(&counters, "A", vec2(1.0, 21.0)).unwrap().ids,
            vec![1, 2]
        );
    }
    #[test]
    fn drag_threshold_uses_total_displacement_instead_of_each_frame_delta() {
        let mut d = Drag::new(vec2(0.0, 0.0), false, false, true);
        for x in 1..6 {
            d.update(vec2(x as f32, 0.0));
        }
        assert!(d.moved());
    }
}
