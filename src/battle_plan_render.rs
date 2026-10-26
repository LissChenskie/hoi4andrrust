//! Cached world-space order geometry. Animation only samples existing paths;
//! it never changes orders, performs pathfinding or advances the combat clock.
use crate::{
    army_ui::ArmyView,
    frontline::FrontEdge,
    hoi_ui as ui,
    operations::{Activation, OrderType},
    warfare::Warfare,
    world::World,
};
use macroquad::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderRef {
    Front(u64),
    Plan(u64),
}
#[derive(Debug, Clone, Copy)]
pub struct Handle {
    pub order: OrderRef,
    pub start: bool,
    pub sector: usize,
    pub point: Vec2,
    pub fixed: Option<FrontEdge>,
}
#[derive(Debug, Clone)]
pub struct Geometry {
    pub order: OrderRef,
    pub army: u64,
    pub paths: Vec<Vec<Vec2>>,
    pub arrows: Vec<Vec<Vec2>>,
    pub signs: Vec<f32>,
    pub handles: Vec<Handle>,
    pub label: Vec2,
}
#[derive(Debug, Default)]
pub struct OrderMap {
    key: Option<(u64, u64, u64, u64, String)>,
    pub shapes: Vec<Geometry>,
    pub rebuilds: u64,
}
fn center(world: &World, id: u32) -> Option<Vec2> {
    world
        .geography
        .provinces
        .get(&id)?
        .center
        .map(|p| vec2(p.x as f32, p.y as f32))
}
fn edge(a: (u32, u32), b: (u32, u32)) -> ((u32, u32), (u32, u32)) {
    if a < b { (a, b) } else { (b, a) }
}
/// Walk actual raster segments, including closed rings. A missing sector never
/// creates a decorative straight connector across enemy/neutral provinces.
pub fn contours(world: &World, edges: &[FrontEdge]) -> Vec<Vec<Vec2>> {
    let wrap = world.geography.wrap_x;
    let width = world.geography.width.max(1);
    let point = |x, y| (if wrap { x % width } else { x }, y);
    let mut remaining = BTreeSet::new();
    let mut neighbours = BTreeMap::<_, BTreeSet<_>>::new();
    for e in edges {
        for s in world
            .geography
            .border_segments
            .get(&e.key())
            .into_iter()
            .flatten()
        {
            let a = point(s[0], s[1]);
            let b = point(s[2], s[3]);
            if a == b {
                continue;
            }
            remaining.insert(edge(a, b));
            neighbours.entry(a).or_default().insert(b);
            neighbours.entry(b).or_default().insert(a);
        }
    }
    let mut paths = Vec::new();
    while let Some(&(a, b)) = remaining.first() {
        let start = neighbours
            .iter()
            .find(|(p, n)| n.len() != 2 && n.iter().any(|q| remaining.contains(&edge(**p, *q))))
            .map_or(a, |(&p, _)| p);
        let mut path = vec![vec2(start.0 as f32, start.1 as f32)];
        let mut at = start;
        while let Some(next) = neighbours[&at]
            .iter()
            .copied()
            .find(|&q| remaining.contains(&edge(at, q)))
        {
            remaining.remove(&edge(at, next));
            let mut x = next.0 as f32;
            if wrap {
                x = next.0 as f32
                    + ((path.last().unwrap().x - next.0 as f32) / width as f32).round()
                        * width as f32;
            }
            path.push(vec2(x, next.1 as f32));
            at = next;
            if at == start || neighbours[&at].len() != 2 {
                break;
            }
        }
        if path.len() > 1 {
            paths.push(path);
        } else {
            remaining.remove(&edge(a, b));
        }
    }
    paths
}
pub fn length(path: &[Vec2]) -> f32 {
    path.windows(2).map(|p| p[0].distance(p[1])).sum()
}
pub fn sample(path: &[Vec2], mut distance: f32) -> (Vec2, Vec2) {
    for p in path.windows(2) {
        let n = p[0].distance(p[1]);
        if distance <= n && n > 0.0 {
            return (
                p[0].lerp(p[1], (distance / n).clamp(0.0, 1.0)),
                (p[1] - p[0]) / n,
            );
        }
        distance -= n;
    }
    let end = path.last().copied().unwrap_or(Vec2::ZERO);
    let tangent = path
        .iter()
        .rev()
        .nth(1)
        .map_or(Vec2::X, |p| (end - *p).normalize_or_zero());
    (end, tangent)
}
pub fn segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let d = b - a;
    let t = if d.length_squared() > 0.0 {
        ((p - a).dot(d) / d.length_squared()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(a + t * d)
}
fn curve(a: Vec2, b: Vec2) -> Vec<Vec2> {
    let d = b - a;
    let n = vec2(-d.y, d.x);
    let control = (a + b) * 0.5 + n * 0.12;
    (0..=24)
        .map(|i| {
            let t = i as f32 / 24.0;
            (1.0 - t).powi(2) * a + 2.0 * (1.0 - t) * t * control + t * t * b
        })
        .collect()
}
fn nearest_edge_at(world: &World, p: Vec2, edges: &[FrontEdge]) -> Option<FrontEdge> {
    edges
        .iter()
        .min_by(|a, b| {
            let distance = |e: &&FrontEdge| {
                world
                    .geography
                    .border_segments
                    .get(&e.key())
                    .into_iter()
                    .flatten()
                    .map(|s| {
                        segment_distance(
                            p,
                            vec2(s[0] as f32, s[1] as f32),
                            vec2(s[2] as f32, s[3] as f32),
                        )
                    })
                    .fold(f32::INFINITY, f32::min)
            };
            distance(a).total_cmp(&distance(b))
        })
        .copied()
}
pub fn handle_screen(world: &World, h: &Handle, source: Rect, area: Rect) -> Vec2 {
    let mut p = screen_path(world, &[h.point], source, area)[0];
    if matches!(h.order, OrderRef::Plan(_)) {
        p.y -= 20.0;
    }
    let top = area.y + crate::ui_layout::Layout::new(area.w, area.h).top + 12.0;
    let bottom = area.bottom() - if area.w < 760.0 { 230.0 } else { 162.0 };
    p.y = p.y.clamp(top, bottom.max(top));
    p.x =
        p.x.clamp(area.x + 8.0, (area.right() - 8.0).max(area.x + 8.0));
    p
}

fn sign(world: &World, path: &[Vec2], edges: &[FrontEdge]) -> f32 {
    let Some(pair) = path.windows(2).next() else {
        return 1.0;
    };
    let mid = (pair[0] + pair[1]) * 0.5;
    let nearest = edges.iter().min_by(|a, b| {
        let distance = |e: &&FrontEdge| {
            world
                .geography
                .border_segments
                .get(&e.key())
                .into_iter()
                .flatten()
                .map(|s| {
                    segment_distance(
                        mid,
                        vec2(s[0] as f32, s[1] as f32),
                        vec2(s[2] as f32, s[3] as f32),
                    )
                })
                .fold(f32::INFINITY, f32::min)
        };
        distance(a).total_cmp(&distance(b))
    });
    let dir = (pair[1] - pair[0]).normalize_or_zero();
    let normal = vec2(-dir.y, dir.x);
    nearest
        .and_then(|e| center(world, e.enemy).zip(center(world, e.friendly)))
        .map_or(1.0, |(enemy, own)| {
            if normal.dot(enemy - own) >= 0.0 {
                1.0
            } else {
                -1.0
            }
        })
}
impl OrderMap {
    pub fn sync(&mut self, world: &World, w: &Warfare, tag: &str) {
        if self.key.as_ref().is_some_and(|(f, p, c, r, t)| {
            *f == w.fronts.revision
                && *p == w.plans.revision
                && *c == w.command.revision
                && *r == world.control_revision
                && t == tag
        }) {
            return;
        }
        let key = (
            w.fronts.revision,
            w.plans.revision,
            w.command.revision,
            world.control_revision,
            tag.to_owned(),
        );
        if self.key.as_ref() == Some(&key) {
            return;
        }
        self.key = Some(key);
        self.rebuilds += 1;
        self.shapes.clear();
        for f in w.fronts.fronts.values().filter(|f| f.country == tag) {
            let sectors = if f.sectors.is_empty() {
                vec![f.edges.clone()]
            } else {
                f.sectors.clone()
            };
            let mut paths = Vec::new();
            let mut signs = Vec::new();
            let mut handles = Vec::new();
            for (sector, edges) in sectors.iter().enumerate() {
                for path in contours(world, edges) {
                    if path.first() != path.last() {
                        for start in [true, false] {
                            handles.push(Handle {
                                order: OrderRef::Front(f.id),
                                start,
                                sector,
                                point: if start {
                                    path[0]
                                } else {
                                    *path.last().unwrap()
                                },
                                fixed: nearest_edge_at(
                                    world,
                                    if start {
                                        *path.last().unwrap()
                                    } else {
                                        path[0]
                                    },
                                    edges,
                                ),
                            });
                        }
                    }
                    signs.push(sign(world, &path, edges));
                    paths.push(path);
                }
            }
            let label = paths
                .iter()
                .max_by(|a, b| length(a).total_cmp(&length(b)))
                .map_or(Vec2::ZERO, |p| sample(p, length(p) * 0.5).0);
            self.shapes.push(Geometry {
                order: OrderRef::Front(f.id),
                army: f.army,
                paths,
                arrows: Vec::new(),
                signs,
                handles,
                label,
            });
        }
        for p in w.plans.plans.values().filter(|p| p.country == tag) {
            let source = p.source_front.and_then(|id| w.fronts.fronts.get(&id));
            if matches!(p.order_type, OrderType::Offensive | OrderType::Spearhead)
                && source.is_none_or(|f| f.edges.is_empty())
            {
                continue;
            }
            let mut goal: Vec<_> = p
                .destination_line
                .iter()
                .filter_map(|id| center(world, *id))
                .collect();
            if goal.is_empty() {
                continue;
            }
            if world.geography.wrap_x {
                let width = world.geography.width.max(1) as f32;
                for i in 1..goal.len() {
                    goal[i].x += ((goal[i - 1].x - goal[i].x) / width).round() * width;
                }
            }
            let mut arrows = Vec::new();
            if let Some(front) = source
                .filter(|_| matches!(p.order_type, OrderType::Offensive | OrderType::Spearhead))
            {
                let regions = crate::frontline::controlled_regions(world, &front.enemy_country);
                let goals: BTreeSet<_> = p
                    .destination_line
                    .iter()
                    .filter_map(|id| regions.get(id))
                    .copied()
                    .collect();
                let sectors = if front.sectors.is_empty() {
                    vec![front.edges.clone()]
                } else {
                    front.sectors.clone()
                };
                for sector in sectors {
                    let Some(e) = sector.get(sector.len() / 2) else {
                        continue;
                    };
                    let Some(a) = center(world, e.friendly) else {
                        continue;
                    };
                    let pocket = !goals.is_empty()
                        && regions.get(&e.enemy).is_some_and(|r| !goals.contains(r));
                    let b = if pocket {
                        center(world, e.enemy).unwrap_or(goal[0])
                    } else {
                        *goal
                            .iter()
                            .min_by(|b, c| {
                                a.distance_squared(**b).total_cmp(&a.distance_squared(**c))
                            })
                            .unwrap()
                    };
                    let mut b = b;
                    if world.geography.wrap_x {
                        let width = world.geography.width.max(1) as f32;
                        b.x += ((a.x - b.x) / width).round() * width;
                    }
                    arrows.push(curve(a, b));
                }
            }
            let mut handles = vec![Handle {
                order: OrderRef::Plan(p.id),
                start: false,
                sector: 0,
                point: *goal.last().unwrap(),
                fixed: None,
            }];
            if goal.len() > 1 {
                handles.push(Handle {
                    order: OrderRef::Plan(p.id),
                    start: true,
                    sector: 0,
                    point: goal[0],
                    fixed: None,
                });
            }
            let label = arrows.first().map_or(goal[goal.len() / 2], |path| {
                sample(path, length(path) * 0.5).0
            });
            let mut paths = vec![goal];
            if p.order_type == OrderType::AreaDefense {
                let edges: Vec<_> = p
                    .area_states
                    .iter()
                    .filter_map(|id| world.states.get(id))
                    .flat_map(|s| s.province_ids.iter())
                    .flat_map(|id| {
                        world
                            .geography
                            .land_neighbors(*id)
                            .filter(|e| {
                                world
                                    .provinces
                                    .get(&e.to)
                                    .and_then(|p| p.state_id)
                                    .is_none_or(|s| !p.area_states.contains(&s))
                            })
                            .map(|e| FrontEdge {
                                friendly: *id,
                                enemy: e.to,
                            })
                    })
                    .collect();
                let contour = contours(world, &edges);
                if !contour.is_empty() {
                    paths = contour;
                }
            }
            self.shapes.push(Geometry {
                order: OrderRef::Plan(p.id),
                army: p.army,
                label,
                paths,
                arrows,
                signs: vec![1.0],
                handles,
            });
        }
    }
    pub fn hit(
        &self,
        world: &World,
        w: &Warfare,
        source: Rect,
        area: Rect,
        point: Vec2,
        selected_plan: Option<u64>,
    ) -> Option<OrderRef> {
        self.shapes
            .iter()
            .filter(|s| match s.order {
                OrderRef::Front(id) => w.fronts.fronts.contains_key(&id),
                OrderRef::Plan(id) => w.plans.plans.get(&id).is_some_and(|p| {
                    p.activation != Activation::Completed || selected_plan == Some(id)
                }),
            })
            .filter_map(|s| {
                let distance = s
                    .paths
                    .iter()
                    .chain(&s.arrows)
                    .map(|path| screen_path(world, path, source, area))
                    .map(|p| {
                        if p.len() == 1 {
                            p[0].distance(point)
                        } else {
                            p.windows(2)
                                .map(|p| segment_distance(point, p[0], p[1]))
                                .fold(f32::INFINITY, f32::min)
                        }
                    })
                    .fold(f32::INFINITY, f32::min);
                (distance
                    <= if matches!(s.order, OrderRef::Front(_)) {
                        14.0
                    } else {
                        24.0
                    })
                .then_some((s.order, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|p| p.0)
    }
    pub fn handle(
        &self,
        world: &World,
        source: Rect,
        area: Rect,
        point: Vec2,
        front: Option<u64>,
        plan: Option<u64>,
    ) -> Option<Handle> {
        self.shapes
            .iter()
            .filter(|s| match s.order {
                OrderRef::Front(id) => Some(id) == front,
                OrderRef::Plan(id) => Some(id) == plan,
            })
            .flat_map(|s| &s.handles)
            .copied()
            .filter(|h| handle_screen(world, h, source, area).distance(point) <= 22.0)
            .next()
    }
}
fn screen_path(world: &World, path: &[Vec2], source: Rect, area: Rect) -> Vec<Vec2> {
    let shift = if world.geography.wrap_x {
        let width = world.geography.width.max(1) as f32;
        ((source.center().x - path.first().map_or(0.0, |p| p.x)) / width).round() * width
    } else {
        0.0
    };
    path.iter()
        .map(|p| {
            vec2(
                area.x + (p.x + shift - source.x) / source.w * area.w,
                area.y + (p.y - source.y) / source.h * area.h,
            )
        })
        .collect()
}
fn stroke(path: &[Vec2], width: f32, color: Color) {
    if path.len() < 2 {
        return;
    }
    let closed = path.first() == path.last();
    let mut previous = None;
    for (i, &point) in path.iter().enumerate() {
        let before = if i > 0 {
            path[i - 1]
        } else if closed {
            path[path.len() - 2]
        } else {
            point - (path[1] - point)
        };
        let after = if i + 1 < path.len() {
            path[i + 1]
        } else if closed {
            path[1]
        } else {
            point + (point - before)
        };
        let incoming = (point - before).normalize_or_zero();
        let outgoing = (after - point).normalize_or_zero();
        let normal = vec2(-incoming.y, incoming.x);
        let next = vec2(-outgoing.y, outgoing.x);
        let miter = (normal + next).normalize_or_zero();
        let offset = miter * (width * 0.5 / miter.dot(normal).abs().max(0.65));
        let left = point + offset;
        let right = point - offset;
        if let Some((a, b)) = previous {
            draw_triangle(a, b, left, color);
            draw_triangle(b, right, left, color);
        }
        previous = Some((left, right));
    }
}
fn prefix(path: &[Vec2], distance: f32) -> Vec<Vec2> {
    let mut result = Vec::new();
    let mut length = 0.0;
    if let Some(&p) = path.first() {
        result.push(p);
    }
    for p in path.windows(2) {
        let n = p[0].distance(p[1]);
        if length + n >= distance {
            result.push(p[0].lerp(p[1], ((distance - length) / n.max(0.001)).clamp(0.0, 1.0)));
            break;
        }
        result.push(p[1]);
        length += n;
    }
    result
}
fn marker(p: Vec2, t: Vec2, color: Color, size: f32) {
    let n = vec2(-t.y, t.x);
    draw_line(
        (p - t * size + n * size * 0.65).x,
        (p - t * size + n * size * 0.65).y,
        p.x,
        p.y,
        2.2,
        color,
    );
    draw_line(
        (p - t * size - n * size * 0.65).x,
        (p - t * size - n * size * 0.65).y,
        p.x,
        p.y,
        2.2,
        color,
    );
}
pub fn animation_phase(state: Activation, paused: bool, time: f64) -> f32 {
    if state == Activation::Executing && !paused {
        (time as f32 * 22.0) % 28.0
    } else {
        0.0
    }
}
pub fn draw(
    world: &World,
    w: &Warfare,
    view: &mut ArmyView,
    tag: &str,
    source: Rect,
    area: Rect,
    time: f64,
    paused: bool,
) {
    view.order_map.sync(world, w, tag);
    let _clip = ui::clip(area);
    for shape in &view.order_map.shapes {
        let Some(army) = w.command.armies.get(&shape.army) else {
            continue;
        };
        let base = Color::from_rgba(army.color[0], army.color[1], army.color[2], 255);
        let editing = view.order_handle.is_some_and(|h| h.order == shape.order)
            || match shape.order {
                OrderRef::Front(id) => {
                    view.selected_front == Some(id)
                        && view.plan_tool == Some(crate::battle_plan_ui::Tool::Front)
                        && !view.draw_new_front
                        && !view.front_preview.is_empty()
                }
                OrderRef::Plan(id) => {
                    view.selected_plan == Some(id)
                        && matches!(
                            view.plan_tool,
                            Some(
                                crate::battle_plan_ui::Tool::Offensive
                                    | crate::battle_plan_ui::Tool::Spearhead
                                    | crate::battle_plan_ui::Tool::EditPlan
                            )
                        )
                        && !view.line_preview.is_empty()
                }
            };
        if editing {
            continue;
        }
        let selected = match shape.order {
            OrderRef::Front(id) => view.selected_front == Some(id),
            OrderRef::Plan(id) => view.selected_plan == Some(id),
        };
        let hovered = view.order_hover == Some(shape.order);
        if let OrderRef::Plan(id) = shape.order {
            let Some(p) = w.plans.plans.get(&id) else {
                continue;
            };
            if p.activation == Activation::Completed && !selected {
                continue;
            }
            let active = p.activation == Activation::Executing;
            let color = if active {
                Color::new(
                    (base.r + 0.18).min(1.0),
                    (base.g + 0.18).min(1.0),
                    (base.b + 0.12).min(1.0),
                    1.0,
                )
            } else if p.activation == Activation::Paused {
                ui::MUTED
            } else {
                base
            };
            let phase = animation_phase(p.activation, paused, time);
            for path in &shape.paths {
                let p = screen_path(world, path, source, area);
                if p.len() == 1 {
                    draw_circle_lines(p[0].x, p[0].y, 10.0, 3.0, color);
                } else {
                    stroke(
                        &p,
                        if selected || hovered { 17.0 } else { 13.0 },
                        Color::new(0.02, 0.04, 0.04, 0.85),
                    );
                    stroke(
                        &p,
                        if selected || hovered { 12.0 } else { 8.0 },
                        Color::new(color.r, color.g, color.b, 0.55),
                    );
                    let mut d = phase;
                    while d < length(&p) {
                        let (q, t) = sample(&p, d);
                        let n = vec2(-t.y, t.x);
                        draw_line(
                            (q - n * 5.0 - t * 3.0).x,
                            (q - n * 5.0 - t * 3.0).y,
                            (q + n * 5.0 + t * 3.0).x,
                            (q + n * 5.0 + t * 3.0).y,
                            2.0,
                            color,
                        );
                        d += 28.0;
                    }
                }
            }
            for path in &shape.arrows {
                let points = screen_path(world, path, source, area);
                let total = length(&points);
                if total < 8.0 {
                    continue;
                }
                let width = (total * 0.18).clamp(
                    12.0,
                    if p.order_type == OrderType::Spearhead {
                        18.0
                    } else {
                        34.0
                    },
                );
                let (tip, t) = sample(&points, total);
                let n = vec2(-t.y, t.x);
                let head = (width * 1.3).min(total * 0.4);
                let body = prefix(&points, total - head);
                stroke(&body, width + 4.0, Color::new(0.01, 0.025, 0.025, 0.8));
                stroke(&body, width, Color::new(color.r, color.g, color.b, 0.65));
                draw_triangle(
                    tip + t * 2.0,
                    tip - t * (head + 2.0) + n * (width + 2.0),
                    tip - t * (head + 2.0) - n * (width + 2.0),
                    Color::new(0.01, 0.025, 0.025, 0.8),
                );
                draw_triangle(
                    tip,
                    tip - t * head + n * width,
                    tip - t * head - n * width,
                    Color::new(color.r, color.g, color.b, 0.65),
                );
                let mut d = phase + 10.0;
                while d < total - head * 0.6 {
                    let (q, t) = sample(&points, d);
                    marker(q, t, if active { WHITE } else { color }, width * 0.24);
                    d += 28.0;
                }
            }
            if selected || active || hovered {
                let mut a = screen_path(world, &[shape.label], source, area)[0] + vec2(10.0, -65.0);
                let title = format!(
                    "{} · {} #{}{}",
                    p.activation.label(),
                    p.order_type.label(),
                    id,
                    if active && paused {
                        " · ПАУЗА"
                    } else {
                        ""
                    }
                );
                let width = (crate::measure_ui_text(&title, 12).width + 14.0).min(area.w - 10.0);
                a.x = a.x.clamp(
                    area.x + 5.0,
                    (area.right() - width - if area.w >= 760.0 { 220.0 } else { 5.0 })
                        .max(area.x + 5.0),
                );
                draw_rectangle(a.x, a.y, width, 23.0, Color::new(0.02, 0.04, 0.04, 0.9));
                ui::label(
                    &title,
                    Rect::new(a.x + 5.0, a.y + 3.0, width - 10.0, 18.0),
                    12.0,
                    color,
                );
            }
        } else {
            for (index, path) in shape.paths.iter().enumerate() {
                let p = screen_path(world, path, source, area);
                stroke(&p, if selected || hovered { 9.0 } else { 7.0 }, BLACK);
                stroke(&p, if selected || hovered { 5.0 } else { 3.0 }, base);
                let mut d = 10.0;
                while d < length(&p) {
                    let (q, t) = sample(&p, d);
                    let n = vec2(-t.y, t.x) * shape.signs[index];
                    draw_triangle(q - t * 3.5, q + t * 3.5, q + n * 8.0, base);
                    d += 22.0;
                }
            }
        }
        if (selected || hovered) && matches!(shape.order, OrderRef::Front(_)) {
            let a = screen_path(world, &[shape.label], source, area)[0] + vec2(12.0, 8.0);
            let count = w.command.members(shape.army).count();
            let title = format!("{} · {} див. · ФРОНТ", army.name, count);
            let width = crate::measure_ui_text(&title, 11).width + 12.0;
            draw_rectangle(a.x, a.y, width, 21.0, Color::new(0.02, 0.04, 0.04, 0.9));
            ui::label(
                &title,
                Rect::new(a.x + 4.0, a.y + 2.0, width - 8.0, 17.0),
                11.0,
                base,
            );
        }
        if selected {
            for h in &shape.handles {
                let p = handle_screen(world, h, source, area);
                draw_circle(p.x, p.y, 7.0, BLACK);
                draw_circle_lines(p.x, p.y, 5.0, 2.0, WHITE);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contours_are_closed_without_fake_cross_sector_connectors() {
        let (mut world, _, _, _) = crate::operations::tests::configured(OrderType::Offensive);
        world.set_province_controller(10, Some("A"));
        let b = crate::frontline::Boundary::build(&world, "A", "B");
        let paths = contours(&world, &b.edges.iter().copied().collect::<Vec<_>>());
        assert!(!paths.is_empty());
        for p in paths {
            assert!(p.windows(2).all(|p| p[0].x == p[1].x || p[0].y == p[1].y));
        }
    }
    #[test]
    fn sampled_animation_moves_along_existing_geometry_and_cache_does_not_rebuild_each_frame() {
        let (world, w, _, _) = crate::operations::tests::configured(OrderType::Offensive);
        let mut cache = OrderMap::default();
        cache.sync(&world, &w, "A");
        let n = cache.rebuilds;
        for _ in 0..120 {
            cache.sync(&world, &w, "A");
        }
        assert_eq!(cache.rebuilds, n);
        let path = curve(vec2(0.0, 0.0), vec2(100.0, 0.0));
        let (a, _) = sample(&path, 10.0);
        let (b, _) = sample(&path, 25.0);
        assert!(a.distance(b) > 10.0);
    }
    #[test]
    fn replaced_deleted_and_hidden_orders_leave_no_old_geometry_or_click_targets() {
        let (world, mut w, front, plan) =
            crate::operations::tests::configured(OrderType::Offensive);
        let army = w.fronts.fronts[&front].army;
        let mut cache = OrderMap::default();
        cache.sync(&world, &w, "A");
        let source = Rect::new(0.0, 0.0, 8.0, 3.0);
        let area = Rect::new(0.0, 0.0, 800.0, 300.0);
        let old_end = center(&world, 24).unwrap() * 100.0;
        assert_eq!(
            cache.hit(&world, &w, source, area, old_end, None),
            Some(OrderRef::Plan(plan))
        );
        let id = w
            .draw_plan(
                &world,
                "A",
                army,
                Some(front),
                OrderType::Offensive,
                vec![17, 18],
                None,
            )
            .unwrap();
        assert_eq!(id, plan);
        cache.sync(&world, &w, "A");
        assert_eq!(cache.shapes.len(), 2);
        assert_eq!(cache.hit(&world, &w, source, area, old_end, None), None);
        let new_end = center(&world, 18).unwrap() * 100.0;
        w.plans.plans.get_mut(&plan).unwrap().activation = Activation::Completed;
        assert_eq!(cache.hit(&world, &w, source, area, new_end, None), None);
        assert_eq!(
            cache.hit(&world, &w, source, area, new_end, Some(plan)),
            Some(OrderRef::Plan(plan))
        );
        w.delete_front("A", front).unwrap();
        cache.sync(&world, &w, "A");
        assert!(cache.shapes.is_empty());
        assert!(w.plans.plans.is_empty());
    }
}

#[cfg(test)]
mod animation_regressions {
    use super::*;
    #[test]
    fn motion_only_marks_execution_and_paused_geometry_stays_still() {
        assert_ne!(
            animation_phase(Activation::Executing, false, 1.0),
            animation_phase(Activation::Executing, false, 1.5)
        );
        for state in [
            Activation::Preparing,
            Activation::Paused,
            Activation::Completed,
            Activation::Blocked,
        ] {
            assert_eq!(animation_phase(state, false, 1.5), 0.0);
        }
        assert_eq!(
            animation_phase(Activation::Executing, true, 1.0),
            animation_phase(Activation::Executing, true, 1.5)
        );
    }
}
