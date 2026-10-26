//! Province-border topology and stable minimum-cost front assignments.
//! Geometry is cached by control revisions; no frame-time pathfinding.
use crate::{
    command::ArmyId,
    map_graph::{CrossingKind, MapPosition},
    warfare::Warfare,
    world::World,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub type FrontId = u64;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FrontEdge {
    pub friendly: u32,
    pub enemy: u32,
}
impl FrontEdge {
    pub fn key(self) -> (u32, u32) {
        (self.friendly.min(self.enemy), self.friendly.max(self.enemy))
    }
}
#[derive(Debug, Clone, Default)]
pub struct Boundary {
    pub edges: BTreeSet<FrontEdge>,
    pub adjacent: BTreeMap<FrontEdge, BTreeSet<FrontEdge>>,
    pub friendly_regions: BTreeMap<u32, u32>,
    pub enemy_regions: BTreeMap<u32, u32>,
}
impl Boundary {
    pub fn build(world: &World, country: &str, enemy: &str) -> Self {
        let mut b = Self {
            friendly_regions: controlled_regions(world, country),
            enemy_regions: controlled_regions(world, enemy),
            ..Default::default()
        };
        let mut vertices = BTreeMap::<(u32, u32), BTreeSet<FrontEdge>>::new();
        for p in world
            .provinces
            .values()
            .filter(|p| p.controller.as_deref() == Some(country))
        {
            for c in world.geography.land_neighbors(p.id).filter(|e| {
                e.kind == CrossingKind::Border && world.controller_of_province(e.to) == Some(enemy)
            }) {
                let edge = FrontEdge {
                    friendly: p.id,
                    enemy: c.to,
                };
                let Some(segments) = world.geography.border_segments.get(&edge.key()) else {
                    continue;
                };
                b.edges.insert(edge);
                for s in segments {
                    for (x, y) in [(s[0], s[1]), (s[2], s[3])] {
                        let x = if world.geography.wrap_x {
                            x % world.geography.width.max(1)
                        } else {
                            x
                        };
                        vertices.entry((x, y)).or_default().insert(edge);
                    }
                }
            }
        }
        for list in vertices.values() {
            for &a in list {
                for &other in list {
                    if a != other
                        && b.friendly_regions.get(&a.friendly)
                            == b.friendly_regions.get(&other.friendly)
                        && b.enemy_regions.get(&a.enemy) == b.enemy_regions.get(&other.enemy)
                    {
                        b.adjacent.entry(a).or_default().insert(other);
                    }
                }
            }
        }
        b
    }
    pub fn path(&self, start: FrontEdge, end: FrontEdge) -> Result<Vec<FrontEdge>, String> {
        if !self.edges.contains(&start) || !self.edges.contains(&end) {
            return Err("Концы должны лежать на границе одной страны".into());
        }
        let mut queue = VecDeque::from([start]);
        let mut prev = BTreeMap::from([(start, start)]);
        while let Some(e) = queue.pop_front() {
            if e == end {
                break;
            }
            for &next in self.adjacent.get(&e).into_iter().flatten() {
                if let std::collections::btree_map::Entry::Vacant(v) = prev.entry(next) {
                    v.insert(e);
                    queue.push_back(next);
                }
            }
        }
        if !prev.contains_key(&end) {
            return Err("Между выбранными участками нет непрерывного фронта".into());
        }
        let mut path = vec![end];
        let mut at = end;
        while at != start {
            at = prev[&at];
            path.push(at);
        }
        path.reverse();
        Ok(path)
    }
    /// Keep disconnected coast-to-coast sectors and closed pocket contours.
    pub fn sectors(&self, edges: &[FrontEdge]) -> Vec<Vec<FrontEdge>> {
        let mut remaining: BTreeSet<_> = edges
            .iter()
            .copied()
            .filter(|e| self.edges.contains(e))
            .collect();
        let mut result = Vec::new();
        while let Some(&first) = remaining.first() {
            remaining.remove(&first);
            let mut sector = Vec::new();
            let mut queue = VecDeque::from([first]);
            while let Some(e) = queue.pop_front() {
                sector.push(e);
                for &next in self.adjacent.get(&e).into_iter().flatten() {
                    if remaining.remove(&next) {
                        queue.push_back(next);
                    }
                }
            }
            result.push(sector);
        }
        result
    }
    /// Move the *owned edges* through provinces changed since the last tick.
    /// No nearest-endpoint fallback and no shortest-path jump to another front.
    fn migrate(&self, world: &World, old: &[FrontEdge], revision: u64) -> Vec<FrontEdge> {
        let mut kept: BTreeSet<_> = old
            .iter()
            .copied()
            .filter(|e| self.edges.contains(e))
            .collect();
        let mut patch = BTreeSet::new();
        let mut queue = VecDeque::new();
        for p in old.iter().flat_map(|e| [e.friendly, e.enemy]) {
            if world
                .provinces
                .get(&p)
                .is_some_and(|p| p.control_revision > revision)
                && patch.insert(p)
            {
                queue.push_back(p);
            }
        }
        while let Some(p) = queue.pop_front() {
            for e in world.geography.land_neighbors(p) {
                if world
                    .provinces
                    .get(&e.to)
                    .is_some_and(|p| p.control_revision > revision)
                    && patch.insert(e.to)
                {
                    queue.push_back(e.to);
                }
            }
        }
        kept.extend(
            self.edges
                .iter()
                .filter(|e| patch.contains(&e.friendly) || patch.contains(&e.enemy))
                .copied(),
        );
        // Retain the player's traversal where it survives, then append new edges.
        let mut ordered: Vec<_> = old.iter().copied().filter(|e| kept.remove(e)).collect();
        ordered.extend(kept);
        ordered
    }
    pub fn nearest(&self, world: &World, p: MapPosition) -> Option<FrontEdge> {
        self.edges
            .iter()
            .min_by(|a, b| distance(world, **a, p).total_cmp(&distance(world, **b, p)))
            .copied()
    }
}
pub fn controlled_regions(world: &World, country: &str) -> BTreeMap<u32, u32> {
    let mut regions = BTreeMap::new();
    for p in world
        .provinces
        .values()
        .filter(|p| p.controller.as_deref() == Some(country))
    {
        if regions.contains_key(&p.id) {
            continue;
        }
        regions.insert(p.id, p.id);
        let mut queue = VecDeque::from([p.id]);
        while let Some(id) = queue.pop_front() {
            for e in world
                .geography
                .land_neighbors(id)
                .filter(|e| e.kind == CrossingKind::Border)
            {
                if world.controller_of_province(e.to) == Some(country)
                    && !regions.contains_key(&e.to)
                {
                    regions.insert(e.to, p.id);
                    queue.push_back(e.to);
                }
            }
        }
    }
    regions
}
fn center(world: &World, e: FrontEdge) -> MapPosition {
    let a = world.geography.provinces[&e.friendly]
        .center
        .unwrap_or(MapPosition { x: 0.0, y: 0.0 });
    let b = world.geography.provinces[&e.enemy].center.unwrap_or(a);
    MapPosition {
        x: (a.x + b.x) / 2.0,
        y: (a.y + b.y) / 2.0,
    }
}
fn distance(world: &World, e: FrontEdge, p: MapPosition) -> f64 {
    let c = center(world, e);
    let mut dx = (c.x - p.x).abs();
    if world.geography.wrap_x {
        dx = dx.min((world.geography.width as f64 - dx).abs());
    }
    dx * dx + (c.y - p.y).powi(2)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontLine {
    pub id: FrontId,
    pub country: String,
    pub army: ArmyId,
    #[serde(default)]
    pub group: Option<crate::command::GroupId>,
    pub enemy_country: String,
    pub edges: Vec<FrontEdge>,
    #[serde(default)]
    pub sectors: Vec<Vec<FrontEdge>>,
    #[serde(default)]
    pub anchor_edges: Vec<FrontEdge>,
    pub assigned_divisions: BTreeSet<u64>,
    pub auto_include: bool,
    pub assignments: BTreeMap<u64, u32>,
    pub reserve_count: usize,
    pub reserves: BTreeSet<u64>,
    pub priorities: BTreeMap<u32, f64>,
    pub control_revision: u64,
    pub command_revision: u64,
    pub revision: u64,
    pub status: String,
    #[serde(skip)]
    watched: BTreeSet<u32>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FrontState {
    pub fronts: BTreeMap<FrontId, FrontLine>,
    pub manual: BTreeSet<u64>,
    /// Manual routes return to their assignment after the final waypoint.
    /// Explicit Hold/detach remains persistent until the player resumes it.
    #[serde(default)]
    pub manual_return: BTreeSet<u64>,
    pub detached: BTreeSet<u64>,
    #[serde(skip)]
    pub active_orders: BTreeSet<u64>,
    pub revision: u64,
    next_id: u64,
    #[serde(skip)]
    pub boundaries: BTreeMap<(String, String), (u64, Boundary)>,
}
impl FrontState {
    pub fn rebuild_after_restore(&mut self, world: &World) {
        self.boundaries.clear();
        self.active_orders.clear();
        for f in self.fronts.values_mut() {
            if f.anchor_edges.is_empty() {
                f.anchor_edges = f.edges.clone();
            }
            let boundary = Boundary::build(world, &f.country, &f.enemy_country);
            let migrated = boundary.migrate(
                world,
                if f.edges.is_empty() {
                    &f.anchor_edges
                } else {
                    &f.edges
                },
                f.control_revision,
            );
            if migrated != f.edges {
                f.revision += 1;
                self.revision += 1;
                f.command_revision = u64::MAX;
            }
            f.edges = migrated;
            if !f.edges.is_empty() {
                f.anchor_edges = f.edges.clone();
            }
            f.sectors = boundary.sectors(&f.edges);
            f.control_revision = world.control_revision;
            f.watched = if f.edges.is_empty() {
                &f.anchor_edges
            } else {
                &f.edges
            }
            .iter()
            .flat_map(|e| [e.friendly, e.enemy])
            .collect();
            let points: Vec<_> = f.watched.iter().copied().collect();
            for p in points {
                f.watched
                    .extend(world.geography.land_neighbors(p).map(|e| e.to));
            }
        }
    }
    pub fn identifiers_valid(&self) -> bool {
        self.fronts.keys().all(|id| *id <= self.next_id)
    }
    pub fn boundary(&mut self, world: &World, country: &str, enemy: &str) -> &Boundary {
        let key = (country.to_owned(), enemy.to_owned());
        if !self
            .boundaries
            .get(&key)
            .is_some_and(|(rev, _)| *rev == world.control_revision)
        {
            self.boundaries.insert(
                key.clone(),
                (
                    world.control_revision,
                    Boundary::build(world, country, enemy),
                ),
            );
        }
        &self.boundaries[&key].1
    }
    pub fn create(
        &mut self,
        w: &Warfare,
        world: &World,
        country: &str,
        army: ArmyId,
        start: FrontEdge,
        end: FrontEdge,
        selection: &BTreeSet<u64>,
    ) -> Result<FrontId, String> {
        w.command.own_army(country, army)?;
        let enemy = world
            .controller_of_province(start.enemy)
            .ok_or("Нет страны по другую сторону границы")?
            .to_owned();
        let edges = self.boundary(world, country, &enemy).path(start, end)?;
        let members: BTreeSet<_> = w.command.members(army).collect();
        let assigned: BTreeSet<_> = if selection.is_empty() {
            members.clone()
        } else {
            members.intersection(selection).copied().collect()
        };
        if assigned.is_empty() {
            return Err("В этой армии нет выбранных дивизий".into());
        }
        for front in self.fronts.values_mut().filter(|f| f.army == army) {
            front.assigned_divisions.retain(|id| !assigned.contains(id));
            front.assignments.retain(|id, _| !assigned.contains(id));
            front.reserves.retain(|id| !assigned.contains(id));
            front.command_revision = u64::MAX;
        }
        self.fronts
            .retain(|_, f| f.army != army || !f.assigned_divisions.is_empty());
        self.next_id += 1;
        let id = self.next_id;
        self.fronts.insert(
            id,
            FrontLine {
                id,
                country: country.into(),
                army,
                group: None,
                enemy_country: enemy,
                anchor_edges: edges.clone(),
                sectors: vec![edges.clone()],
                edges,
                auto_include: assigned == members,
                assigned_divisions: assigned,
                assignments: BTreeMap::new(),
                reserve_count: 0,
                reserves: BTreeSet::new(),
                priorities: BTreeMap::new(),
                control_revision: world.control_revision,
                command_revision: u64::MAX,
                revision: 1,
                status: "Фронт создан".into(),
                watched: BTreeSet::new(),
            },
        );
        self.revision += 1;
        Ok(id)
    }
    pub fn assign_group(
        &mut self,
        w: &Warfare,
        country: &str,
        id: FrontId,
        selection: &BTreeSet<u64>,
    ) -> Result<(), String> {
        let f = self
            .fronts
            .get(&id)
            .filter(|f| f.country == country)
            .ok_or("Выберите свой фронт")?;
        let group = w
            .command
            .armies
            .get(&f.army)
            .and_then(|a| a.group)
            .ok_or("Армия должна входить в группу")?;
        let members: BTreeSet<_> = w
            .command
            .membership
            .iter()
            .filter(|(_, army)| {
                w.command
                    .armies
                    .get(army)
                    .is_some_and(|a| a.group == Some(group))
            })
            .map(|(&id, _)| id)
            .collect();
        let assigned = if selection.is_empty() {
            members.clone()
        } else {
            members.intersection(selection).copied().collect()
        };
        if assigned.is_empty() {
            return Err("Нет выбранных дивизий этой группы".into());
        }
        for other in self
            .fronts
            .values_mut()
            .filter(|f| f.id != id && f.country == country)
        {
            other.assigned_divisions.retain(|d| !assigned.contains(d));
            other.assignments.retain(|d, _| !assigned.contains(d));
            other.reserves.retain(|d| !assigned.contains(d));
            other.command_revision = u64::MAX;
        }
        let f = self.fronts.get_mut(&id).unwrap();
        f.group = Some(group);
        f.auto_include = assigned == members;
        f.assigned_divisions = assigned;
        f.command_revision = u64::MAX;
        f.revision += 1;
        self.revision += 1;
        Ok(())
    }
    pub fn create_drawn(
        &mut self,
        w: &Warfare,
        world: &World,
        country: &str,
        army: ArmyId,
        path: &[FrontEdge],
        selection: &BTreeSet<u64>,
    ) -> Result<FrontId, String> {
        let start = *path.first().ok_or("Нарисуйте непрерывную линию")?;
        let end = *path.last().unwrap();
        let enemy = world
            .controller_of_province(start.enemy)
            .ok_or("Неизвестная граница")?;
        let b = self.boundary(world, country, enemy);
        if !path.iter().all(|e| b.edges.contains(e))
            || !path
                .windows(2)
                .all(|p| b.adjacent.get(&p[0]).is_some_and(|n| n.contains(&p[1])))
        {
            return Err("Линия должна идти по непрерывным рёбрам границы".into());
        }
        let id = self.create(w, world, country, army, start, end, selection)?;
        let f = self.fronts.get_mut(&id).unwrap();
        f.edges = path.to_vec();
        f.anchor_edges = f.edges.clone();
        f.sectors = vec![path.to_vec()];
        Ok(id)
    }
    pub fn redraw(
        &mut self,
        w: &Warfare,
        world: &World,
        country: &str,
        id: FrontId,
        path: &[FrontEdge],
    ) -> Result<(), String> {
        let f = self
            .fronts
            .get(&id)
            .filter(|f| f.country == country)
            .ok_or("Выберите свой фронт")?;
        w.command.own_army(country, f.army)?;
        let first = *path.first().ok_or("Нарисуйте непрерывный фронт")?;
        let enemy = world
            .controller_of_province(first.enemy)
            .ok_or("Нет страны за фронтом")?
            .to_owned();
        let b = self.boundary(world, country, &enemy);
        if !path.iter().all(|e| b.edges.contains(e))
            || !path
                .windows(2)
                .all(|p| b.adjacent.get(&p[0]).is_some_and(|v| v.contains(&p[1])))
        {
            return Err("Фронт должен идти по непрерывной границе".into());
        }
        let sectors = b.sectors(path);
        let f = self.fronts.get_mut(&id).unwrap();
        f.edges = path.to_vec();
        f.anchor_edges = f.edges.clone();
        f.sectors = sectors;
        f.enemy_country = enemy;
        f.assignments.clear();
        f.reserves.clear();
        f.watched.clear();
        f.command_revision = u64::MAX;
        f.revision += 1;
        f.control_revision = world.control_revision;
        f.status = "Фронт изменён".into();
        self.revision += 1;
        Ok(())
    }
    pub fn edit_end(
        &mut self,
        w: &Warfare,
        world: &World,
        country: &str,
        id: FrontId,
        edge: FrontEdge,
        start: bool,
    ) -> Result<(), String> {
        let f = self
            .fronts
            .get(&id)
            .filter(|f| f.country == country)
            .ok_or("Выберите свой фронт")?
            .clone();
        w.command.own_army(country, f.army)?;
        let first = if start {
            edge
        } else {
            *f.edges.first().ok_or("Фронт исчез")?
        };
        let last = if start {
            *f.edges.last().ok_or("Фронт исчез")?
        } else {
            edge
        };
        let path = self
            .boundary(world, country, &f.enemy_country)
            .path(first, last)?;
        let f = self.fronts.get_mut(&id).unwrap();
        f.sectors = vec![path.clone()];
        f.edges = path;
        f.anchor_edges = f.edges.clone();
        f.revision += 1;
        f.command_revision = u64::MAX;
        f.watched.clear();
        self.revision += 1;
        Ok(())
    }
    pub fn delete(&mut self, country: &str, id: FrontId) -> Result<(), String> {
        if !self.fronts.get(&id).is_some_and(|f| f.country == country) {
            return Err("Выберите свой фронт".into());
        }
        self.fronts.remove(&id);
        self.revision += 1;
        Ok(())
    }
    pub fn split(&mut self, country: &str, id: FrontId, at: usize) -> Result<FrontId, String> {
        let f = self
            .fronts
            .get_mut(&id)
            .filter(|f| f.country == country)
            .ok_or("Выберите свой фронт")?;
        if at == 0 || at >= f.edges.len() {
            return Err("Нужно выбрать внутреннюю точку фронта".into());
        }
        let mut other = f.clone();
        other.edges = f.edges.split_off(at);
        f.sectors = vec![f.edges.clone()];
        f.anchor_edges = f.edges.clone();
        other.sectors = vec![other.edges.clone()];
        other.anchor_edges = other.edges.clone();
        f.revision += 1;
        other.revision += 1;
        f.watched.clear();
        other.watched.clear();
        let ids: Vec<_> = f.assigned_divisions.iter().copied().collect();
        for d in ids.iter().skip(ids.len() / 2) {
            f.assigned_divisions.remove(d);
        }
        other.assigned_divisions = ids
            .into_iter()
            .filter(|d| !f.assigned_divisions.contains(d))
            .collect();
        f.command_revision = u64::MAX;
        f.auto_include = false;
        other.auto_include = false;
        other.assignments.clear();
        other.command_revision = u64::MAX;
        self.next_id += 1;
        other.id = self.next_id;
        self.fronts.insert(other.id, other);
        self.revision += 1;
        Ok(self.next_id)
    }
    pub fn merge(
        &mut self,
        world: &World,
        country: &str,
        a: FrontId,
        b: FrontId,
    ) -> Result<(), String> {
        if a == b {
            return Err("Нужны два разных фронта".into());
        }
        let left = self
            .fronts
            .get(&a)
            .filter(|f| f.country == country)
            .ok_or("Нет первого фронта")?
            .clone();
        let right = self
            .fronts
            .get(&b)
            .filter(|f| {
                f.country == country && f.army == left.army && f.enemy_country == left.enemy_country
            })
            .ok_or("Нужны фронты одной армии и границы")?
            .clone();
        let mut edges = self.boundary(world, country, &left.enemy_country).path(
            *left.edges.first().ok_or("Пустой фронт")?,
            *right.edges.last().ok_or("Пустой фронт")?,
        )?;
        edges.dedup();
        let f = self.fronts.get_mut(&a).unwrap();
        f.sectors = vec![edges.clone()];
        f.edges = edges;
        f.anchor_edges = f.edges.clone();
        f.revision += 1;
        f.assigned_divisions.extend(right.assigned_divisions);
        f.command_revision = u64::MAX;
        f.watched.clear();
        self.fronts.remove(&b);
        self.revision += 1;
        Ok(())
    }
    pub fn tick(&mut self, w: &mut Warfare, world: &World) {
        self.fronts.retain(|_, f| {
            w.command
                .armies
                .get(&f.army)
                .is_some_and(|a| a.country == f.country)
        });
        let ids: Vec<_> = self.fronts.keys().copied().collect();
        for id in ids {
            let Some(mut f) = self.fronts.remove(&id) else {
                continue;
            };
            let changed = f.watched.is_empty()
                || f.watched.iter().any(|p| {
                    world
                        .provinces
                        .get(p)
                        .is_some_and(|p| p.control_revision > f.control_revision)
                });
            if changed {
                let b = self.boundary(world, &f.country, &f.enemy_country);
                f.edges = b.migrate(
                    world,
                    if f.edges.is_empty() {
                        &f.anchor_edges
                    } else {
                        &f.edges
                    },
                    f.control_revision,
                );
                if !f.edges.is_empty() {
                    f.anchor_edges = f.edges.clone();
                }
                f.sectors = b.sectors(&f.edges);
                f.command_revision = u64::MAX;
                f.revision += 1;
                self.revision += 1;
            }
            f.control_revision = world.control_revision;
            if changed || f.command_revision != w.command.revision {
                if f.auto_include {
                    let assigned_elsewhere: BTreeSet<_> = self
                        .fronts
                        .values()
                        .filter(|other| other.country == f.country)
                        .flat_map(|other| other.assigned_divisions.iter().copied())
                        .collect();
                    f.assigned_divisions.extend(
                        w.command
                            .membership
                            .iter()
                            .filter(|(_, army)| {
                                if let Some(group) = f.group {
                                    w.command
                                        .armies
                                        .get(army)
                                        .is_some_and(|a| a.group == Some(group))
                                } else {
                                    **army == f.army
                                }
                            })
                            .map(|(&d, _)| d)
                            .filter(|d| {
                                !assigned_elsewhere.contains(d)
                                    && !w.abilities.headquarters.contains_key(d)
                            }),
                    );
                }
                f.assigned_divisions.retain(|d| {
                    w.command.membership.get(d).is_some_and(|army| {
                        if let Some(group) = f.group {
                            w.command
                                .armies
                                .get(army)
                                .is_some_and(|a| a.group == Some(group))
                        } else {
                            *army == f.army
                        }
                    }) && w.divisions.get(d).is_some_and(|d| !d.destroyed)
                        && !w.abilities.headquarters.contains_key(d)
                });
                self.rebalance(&mut f, w, world);
                f.command_revision = w.command.revision;
                f.watched = if f.edges.is_empty() {
                    &f.anchor_edges
                } else {
                    &f.edges
                }
                .iter()
                .flat_map(|e| [e.friendly, e.enemy])
                .collect();
                let p: Vec<_> = f.watched.iter().copied().collect();
                for p in p {
                    f.watched
                        .extend(world.geography.land_neighbors(p).map(|e| e.to));
                }
            }
            for (&d, &target) in &f.assignments {
                if self.manual.contains(&d)
                    || self.detached.contains(&d)
                    || self.active_orders.contains(&d)
                {
                    continue;
                }
                let Some(unit) = w.divisions.get(&d) else {
                    continue;
                };
                if unit.province == target
                    || unit.battle.is_some()
                    || matches!(
                        unit.order,
                        crate::military_model::DivisionOrder::Retreat { .. }
                    )
                {
                    continue;
                }
                if unit.destination.is_some_and(|o| o.province == target) {
                    continue;
                }
                if let Err(e) = w.issue_move(&f.country, &[d], target, world, false, false) {
                    f.status = e;
                }
            }
            self.fronts.insert(id, f);
        }
    }
    fn rebalance(&self, f: &mut FrontLine, w: &Warfare, world: &World) {
        let positions: BTreeSet<_> = f.edges.iter().map(|e| e.friendly).collect();
        let units: Vec<_> = f
            .assigned_divisions
            .iter()
            .filter_map(|id| w.divisions.get(id))
            .filter(|d| !self.manual.contains(&d.id) && !self.detached.contains(&d.id))
            .collect();
        if positions.is_empty() {
            f.assignments.clear();
            f.status = "На этом участке больше нет границы".into();
            return;
        }
        let mut distances = BTreeMap::new();
        for d in &units {
            distances
                .entry(d.province)
                .or_insert_with(|| w.movement_distances(&f.country, d.province, world));
        }
        // Partition by actual friendly reachability: a division inside a pocket
        // must never consume a slot on a disconnected outer front (or vice versa).
        let mut groups: BTreeMap<Vec<u32>, Vec<_>> = BTreeMap::new();
        for d in &units {
            let reachable = positions
                .iter()
                .filter(|p| distances[&d.province].contains_key(p))
                .copied()
                .collect();
            groups.entry(reachable).or_default().push(*d);
        }
        let previous = std::mem::take(&mut f.assignments);
        f.reserves.clear();
        let mut reserve_left = f.reserve_count.min(units.len());
        for (positions, units) in groups {
            if positions.is_empty() {
                f.reserves.extend(units.iter().map(|d| d.id));
                continue;
            }
            let reserved = reserve_left.min(units.len());
            reserve_left -= reserved;
            let count = units.len() - reserved;
            let mut slots = Vec::new();
            let mut density = BTreeMap::<u32, f64>::new();
            for &p in &positions {
                let allied = w
                    .divisions
                    .values()
                    .filter(|d| {
                        !d.destroyed
                            && d.province == p
                            && !f.assigned_divisions.contains(&d.id)
                            && w.friendly(&f.country, &d.country)
                    })
                    .map(|d| d.derived.stats.width * d.strength())
                    .sum();
                density.insert(p, allied);
            }
            let average_width = units.iter().map(|d| d.derived.stats.width).sum::<f64>()
                / units.len().max(1) as f64;
            for _ in 0..count {
                let p = *positions
                    .iter()
                    .min_by(|a, b| {
                        let score = |p: &u32| {
                            density[p] / f.priorities.get(p).copied().unwrap_or(1.0).max(0.01)
                        };
                        score(a).total_cmp(&score(b)).then(a.cmp(b))
                    })
                    .unwrap();
                slots.push(p);
                *density.get_mut(&p).unwrap() += average_width;
            }
            let mut costs = vec![vec![0.0; units.len()]; units.len()];
            for (i, d) in units.iter().enumerate() {
                for j in 0..slots.len() {
                    let p = slots[j];
                    let hours = distances[&d.province][&p] / d.derived.stats.speed.max(0.01);
                    costs[i][j] = hours
                        + (if previous.get(&d.id) == Some(&p) {
                            0.0
                        } else {
                            6.0
                        })
                        + (1.0 - d.supply_fraction) * hours;
                }
            }
            for (i, j) in minimum_cost_assignment(&costs).into_iter().enumerate() {
                if j < slots.len() {
                    f.assignments.insert(units[i].id, slots[j]);
                } else {
                    f.reserves.insert(units[i].id);
                }
            }
        }
        f.status = format!(
            "Распределено {} · резерв {}",
            f.assignments.len(),
            f.reserves.len()
        );
    }
}
/// Hungarian assignment, deterministic tie breaking. Costs may encode travel,
/// supply, unit suitability and a penalty for changing a previous position.
pub fn minimum_cost_assignment(cost: &[Vec<f64>]) -> Vec<usize> {
    let n = cost.len();
    if n == 0 {
        return Vec::new();
    };
    let m = cost[0].len();
    assert!(m >= n && cost.iter().all(|r| r.len() == m));
    let mut u = vec![0.0; n + 1];
    let mut v = vec![0.0; m + 1];
    let mut p = vec![0; m + 1];
    let mut way = vec![0; m + 1];
    for i in 1..=n {
        p[0] = i;
        let mut j0 = 0;
        let mut minv = vec![f64::INFINITY; m + 1];
        let mut used = vec![false; m + 1];
        loop {
            used[j0] = true;
            let i0 = p[j0];
            let mut delta = f64::INFINITY;
            let mut j1 = 0;
            for j in 1..=m {
                if !used[j] {
                    let cur = cost[i0 - 1][j - 1] - u[i0] - v[j];
                    if cur < minv[j] {
                        minv[j] = cur;
                        way[j] = j0;
                    }
                    if minv[j] < delta {
                        delta = minv[j];
                        j1 = j;
                    }
                }
            }
            for j in 0..=m {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }
        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }
    let mut out = vec![0; n];
    for j in 1..=m {
        if p[j] != 0 {
            out[p[j] - 1] = j - 1;
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        game::GameDate,
        map_graph::MapGraph,
        military_model::tests::{catalogue, division},
    };
    pub fn scenario() -> (World, Warfare) {
        let mut rgba = Vec::new();
        let mut definition = String::new();
        for p in 1..=24 {
            rgba.extend([p as u8, 1, 1, 255]);
            definition.push_str(&format!("{p};{p};1;1;land;false;plains;1\n"));
        }
        let geography = MapGraph::from_rgba(8, 3, &rgba, &definition, false).unwrap();
        let membership = (1..=24).map(|p| (p, if p <= 8 { 1 } else { 2 })).collect();
        let owner = std::collections::HashMap::from([(1, "A".into()), (2, "B".into())]);
        let mut world = World::from_map_data(
            &membership,
            &owner,
            &Default::default(),
            &Default::default(),
            &Default::default(),
        );
        world.geography = geography;
        let mut db = catalogue();
        db.rules.pixels_to_km = Some(1.0);
        let mut w = Warfare::new(db);
        w.ai_enabled = false;
        for id in 1..=12 {
            w.divisions.insert(id, division(id, "A", 1));
        }
        (world, w)
    }
    #[test]
    fn twelve_divisions_cover_eight_physical_border_provinces_and_keep_assignments() {
        let (mut world, mut w) = scenario();
        let ids: Vec<_> = (1..=12).collect();
        let a = w.create_army("A", &ids, "").unwrap();
        let b = Boundary::build(&world, "A", "B");
        assert_eq!(b.edges.len(), 8);
        let mut fronts = FrontState::default();
        let f = fronts
            .create(
                &w,
                &world,
                "A",
                a,
                FrontEdge {
                    friendly: 1,
                    enemy: 9,
                },
                FrontEdge {
                    friendly: 8,
                    enemy: 16,
                },
                &ids.iter().copied().collect(),
            )
            .unwrap();
        w.fronts = fronts;
        let before = w.divisions[&1].province;
        for h in 1..=4 {
            w.tick_hour(&mut world, GameDate::START, h, h as u64, Some("A"));
        }
        assert_eq!(before, 1);
        assert_eq!(w.fronts.fronts[&f].assignments.len(), 12);
        let occupied: BTreeSet<_> = w.divisions.values().map(|d| d.province).collect();
        assert_eq!(occupied, (1..=8).collect());
        let assigned = w.fronts.fronts[&f].assignments.clone();
        w.tick_hour(&mut world, GameDate::START, 5, 5, Some("A"));
        assert_eq!(assigned, w.fronts.fronts[&f].assignments);
        let n = w.fronts.fronts[&f].revision;
        world.set_province_controller(9, Some("A"));
        w.tick_hour(&mut world, GameDate::START, 6, 6, Some("A"));
        assert!(w.fronts.fronts[&f].revision > n);
        assert_eq!(w.command.members(a).count(), 12);
        assert!(
            w.fronts.fronts[&f]
                .edges
                .iter()
                .all(|e| world.controller_of_province(e.friendly) == Some("A")
                    && world.controller_of_province(e.enemy) == Some("B"))
        );
    }
    #[test]
    fn assignment_is_minimal_and_front_split_merge_preserve_members() {
        assert_eq!(
            minimum_cost_assignment(&[vec![9.0, 1.0], vec![1.0, 9.0]]),
            vec![1, 0]
        );
        let (world, mut w) = scenario();
        let ids: Vec<_> = (1..=12).collect();
        let a = w.create_army("A", &ids, "").unwrap();
        let mut state = FrontState::default();
        let f = state
            .create(
                &w,
                &world,
                "A",
                a,
                FrontEdge {
                    friendly: 1,
                    enemy: 9,
                },
                FrontEdge {
                    friendly: 8,
                    enemy: 16,
                },
                &ids.iter().copied().collect(),
            )
            .unwrap();
        let other = state.split("A", f, 4).unwrap();
        assert!(
            state.fronts[&f]
                .assigned_divisions
                .is_disjoint(&state.fronts[&other].assigned_divisions)
        );
        state.merge(&world, "A", f, other).unwrap();
        assert_eq!(state.fronts[&f].assigned_divisions.len(), 12);
        assert_eq!(state.fronts.len(), 1);
        let encoded = serde_json::to_string(&state).unwrap();
        let decoded: FrontState = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.fronts[&f].edges.len(), 8);
    }
}

#[cfg(test)]
mod pocket_regressions {
    use super::*;
    use crate::military_model::tests::{catalogue, division};
    fn grid() -> (World, Warfare, u64) {
        let mut rgba = Vec::new();
        let mut defs = String::new();
        let mut membership = std::collections::HashMap::new();
        for y in 0..7 {
            for x in 0..7 {
                let p = y * 7 + x + 1;
                rgba.extend([p as u8, 1, 1, 255]);
                defs += &format!("{p};{p};1;1;land;false;plains;1\n");
                membership.insert(p, if x < 2 { 1 } else { 2 });
            }
        }
        let mut world = World::from_map_data(
            &membership,
            &std::collections::HashMap::from([(1, "A".into()), (2, "B".into())]),
            &Default::default(),
            &Default::default(),
            &Default::default(),
        );
        world.geography = crate::map_graph::MapGraph::from_rgba(7, 7, &rgba, &defs, false).unwrap();
        let mut db = catalogue();
        db.rules.pixels_to_km = Some(1.0);
        let mut w = Warfare::new(db);
        w.ai_enabled = false;
        for id in 1..=12 {
            w.divisions.insert(id, division(id, "A", 1));
        }
        let army = w
            .create_army("A", &(1..=12).collect::<Vec<_>>(), "")
            .unwrap();
        let mut f = FrontState::default();
        let id = f
            .create(
                &w,
                &world,
                "A",
                army,
                FrontEdge {
                    friendly: 2,
                    enemy: 3,
                },
                FrontEdge {
                    friendly: 44,
                    enemy: 45,
                },
                &BTreeSet::new(),
            )
            .unwrap();
        w.fronts = f;
        (world, w, id)
    }
    fn tick(w: &mut Warfare, world: &mut World, h: u64) {
        w.tick_hour(
            world,
            crate::game::GameDate::START,
            (h % 24) as u8,
            h,
            Some("A"),
        );
    }
    #[test]
    fn closing_and_clearing_a_pocket_preserves_outer_front_and_complete_inner_ring() {
        let (mut world, mut w, id) = grid();
        tick(&mut w, &mut world, 1);
        for y in [1, 5] {
            for x in 2..=4 {
                world.set_province_controller(y * 7 + x + 1, Some("A"));
            }
        }
        for y in 1..=5 {
            world.set_province_controller(y * 7 + 5, Some("A"));
        }
        tick(&mut w, &mut world, 2);
        let f = &w.fronts.fronts[&id];
        let expected = Boundary::build(&world, "A", "B");
        assert_eq!(
            f.edges.iter().copied().collect::<BTreeSet<_>>(),
            expected.edges
        );
        assert_eq!(
            f.sectors.len(),
            2,
            "outer front and closed pocket must both survive"
        );
        assert_eq!(f.assigned_divisions.len(), 12);
        for y in 2..5 {
            for x in 2..4 {
                world.set_province_controller(y * 7 + x + 1, Some("A"));
            }
        }
        tick(&mut w, &mut world, 3);
        let f = &w.fronts.fronts[&id];
        assert_eq!(f.sectors.len(), 1);
        assert_eq!(
            f.edges.iter().copied().collect::<BTreeSet<_>>(),
            Boundary::build(&world, "A", "B").edges
        );
        assert_eq!(w.fronts.fronts.len(), 1);
        assert_eq!(w.command.members(f.army).count(), 12);
    }
    #[test]
    fn friendly_pockets_never_get_assignments_across_an_enemy_cut() {
        let (mut world, mut w, id) = grid();
        tick(&mut w, &mut world, 1);
        w.divisions.get_mut(&1).unwrap().province = 16;
        for y in 1..=4 {
            world.set_province_controller(y * 7 + 1, Some("B"));
        }
        for y in [1, 4] {
            world.set_province_controller(y * 7 + 2, Some("B"));
        }
        tick(&mut w, &mut world, 2);
        let f = &w.fronts.fronts[&id];
        assert!(f.sectors.len() >= 2);
        assert!(f.assignments.contains_key(&1));
        for (&unit, &target) in &f.assignments {
            assert!(
                w.movement_distances("A", w.divisions[&unit].province, &world)
                    .contains_key(&target),
                "unreachable assignment {unit}->{target}"
            );
        }
    }
    #[test]
    fn restoring_before_the_next_front_tick_keeps_new_pocket_sectors() {
        let (mut world, mut w, id) = grid();
        tick(&mut w, &mut world, 1);
        for y in [1, 5] {
            for x in 2..=4 {
                world.set_province_controller(y * 7 + x + 1, Some("A"));
            }
        }
        for y in 1..=5 {
            world.set_province_controller(y * 7 + 5, Some("A"));
        }
        // A save can occur after a capture but before the hourly front review.
        let saved = serde_json::to_string(&w.fronts).unwrap();
        let mut restored: FrontState = serde_json::from_str(&saved).unwrap();
        restored.rebuild_after_restore(&world);
        assert_eq!(restored.fronts[&id].sectors.len(), 2);
        assert_eq!(
            restored.fronts[&id]
                .edges
                .iter()
                .copied()
                .collect::<BTreeSet<_>>(),
            Boundary::build(&world, "A", "B").edges
        );
        assert_eq!(
            restored.fronts[&id].control_revision,
            world.control_revision
        );
        let after = serde_json::to_string(&restored).unwrap();
        restored.rebuild_after_restore(&world);
        assert_eq!(serde_json::to_string(&restored).unwrap(), after);
        restored.tick(&mut w, &world);
        assert_eq!(restored.fronts[&id].assignments.len(), 12);
    }
    #[test]
    fn dormant_front_can_reacquire_contact_after_save_without_a_ghost_line() {
        let (mut world, mut w, id) = grid();
        tick(&mut w, &mut world, 1);
        for p in 1..=49 {
            world.set_province_controller(p, Some("A"));
        }
        tick(&mut w, &mut world, 2);
        assert!(w.fronts.fronts[&id].edges.is_empty());
        assert!(w.fronts.fronts[&id].assignments.is_empty());
        assert!(!w.fronts.fronts[&id].anchor_edges.is_empty());
        world.set_province_controller(24, Some("B"));
        let saved = serde_json::to_string(&w.fronts).unwrap();
        let mut restored: FrontState = serde_json::from_str(&saved).unwrap();
        restored.rebuild_after_restore(&world);
        assert_eq!(restored.fronts[&id].sectors.len(), 1);
        assert_eq!(
            restored.fronts[&id]
                .edges
                .iter()
                .copied()
                .collect::<BTreeSet<_>>(),
            Boundary::build(&world, "A", "B").edges
        );
        assert_eq!(restored.fronts[&id].edges.len(), 4);
        restored.tick(&mut w, &world);
        assert_eq!(restored.fronts[&id].assignments.len(), 12);
    }
    #[test]
    fn redrawing_reuses_id_and_partial_front_does_not_expand_to_whole_border() {
        let (world, mut w, id) = grid();
        let army = w.fronts.fronts[&id].army;
        let path = vec![
            FrontEdge {
                friendly: 16,
                enemy: 17,
            },
            FrontEdge {
                friendly: 23,
                enemy: 24,
            },
            FrontEdge {
                friendly: 30,
                enemy: 31,
            },
        ];
        let mut fronts = std::mem::take(&mut w.fronts);
        for _ in 0..30 {
            fronts.redraw(&w, &world, "A", id, &path).unwrap();
        }
        assert_eq!(fronts.fronts.len(), 1);
        assert_eq!(fronts.fronts[&id].army, army);
        fronts.tick(&mut w, &world);
        assert_eq!(fronts.fronts[&id].edges, path);
        let before = fronts.fronts[&id].edges.clone();
        assert!(
            fronts
                .redraw(
                    &w,
                    &world,
                    "A",
                    id,
                    &[FrontEdge {
                        friendly: 1,
                        enemy: 49
                    }]
                )
                .is_err()
        );
        assert_eq!(fronts.fronts[&id].edges, before);
    }
}
