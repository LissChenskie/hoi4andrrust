//! Operational policies issue ordinary movement/attack orders; combat remains
//! exclusively in the hourly combat core. Risk thresholds are tunable policy,
//! not a claim to reproduce the executable's unpublished battle-plan AI.
use crate::{
    command::ArmyId,
    frontline::{FrontId, minimum_cost_assignment},
    military_model::{Division, DivisionOrder},
    warfare::Warfare,
    world::World,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
pub type PlanId = u64;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    Offensive,
    Spearhead,
    Fallback,
    AreaDefense,
}
impl OrderType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Offensive => "Наступление",
            Self::Spearhead => "Прорыв",
            Self::Fallback => "Линия обороны",
            Self::AreaDefense => "Охрана области",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionMode {
    Cautious,
    Balanced,
    Aggressive,
}
impl ExecutionMode {
    pub fn next(self) -> Self {
        match self {
            Self::Cautious => Self::Balanced,
            Self::Balanced => Self::Aggressive,
            Self::Aggressive => Self::Cautious,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Cautious => "Осторожно",
            Self::Balanced => "Сбалансированно",
            Self::Aggressive => "Агрессивно",
        }
    }
    fn policy(self) -> (f64, f64, f64) {
        match self {
            Self::Cautious => (0.65, 0.75, 1.4),
            Self::Balanced => (0.4, 0.45, 1.05),
            Self::Aggressive => (0.15, 0.2, 0.65),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Activation {
    Preparing,
    Executing,
    Paused,
    Completed,
    Blocked,
}
impl Activation {
    pub fn label(self) -> &'static str {
        match self {
            Self::Preparing => "Подготовка",
            Self::Executing => "Выполняется",
            Self::Paused => "Остановлен",
            Self::Completed => "Завершён",
            Self::Blocked => "Недоступен",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AreaGuard {
    pub ports: bool,
    pub victory_points: bool,
    pub forts: bool,
    pub supply_nodes: bool,
    pub coast: bool,
}
impl Default for AreaGuard {
    fn default() -> Self {
        Self {
            ports: true,
            victory_points: true,
            forts: true,
            supply_nodes: true,
            coast: false,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BattlePlan {
    pub id: PlanId,
    pub country: String,
    pub army: ArmyId,
    pub source_front: Option<FrontId>,
    #[serde(default)]
    pub source_revision: u64,
    pub assigned_divisions: BTreeSet<u64>,
    pub destination_line: Vec<u32>,
    pub area_states: BTreeSet<u32>,
    pub order_type: OrderType,
    pub mode: ExecutionMode,
    pub activation: Activation,
    pub progress: f64,
    pub preparation: f64,
    pub assignments: BTreeMap<u64, u32>,
    pub guard: AreaGuard,
    pub status: String,
    pub last_review: u64,
    pub control_revision: u64,
    pub cached_route: BTreeSet<u32>,
    #[serde(skip)]
    distance: BTreeMap<u32, u32>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OperationalState {
    pub plans: BTreeMap<PlanId, BattlePlan>,
    pub revision: u64,
    next_id: u64,
}
impl OperationalState {
    pub fn rebuild_after_restore(&mut self, w: &Warfare, world: &World) {
        for p in self.plans.values_mut() {
            p.distance = goal_distances(w, world, &p.country, &p.destination_line);
        }
    }
    pub fn identifiers_valid(&self) -> bool {
        self.plans.keys().all(|id| *id <= self.next_id)
    }
    pub fn create(
        &mut self,
        w: &Warfare,
        world: &World,
        country: &str,
        army: ArmyId,
        source: Option<FrontId>,
        kind: OrderType,
        mut line: Vec<u32>,
    ) -> Result<PlanId, String> {
        w.command.own_army(country, army)?;
        if line.is_empty() {
            return Err("Нарисуйте конечную линию или выберите область".into());
        }
        if line.iter().any(|p| {
            !world
                .geography
                .provinces
                .get(p)
                .is_some_and(|p| p.definition.kind == crate::map_graph::ProvinceKind::Land)
        }) {
            return Err("Приказ должен лежать на суше".into());
        }
        if kind == OrderType::Spearhead {
            line = vec![*line.last().unwrap()];
        }
        let units = if matches!(kind, OrderType::Offensive | OrderType::Spearhead) {
            let f = source
                .and_then(|id| w.fronts.fronts.get(&id))
                .filter(|f| f.country == country && f.army == army)
                .ok_or("Сначала создайте фронт выбранной армии")?;
            f.assigned_divisions.clone()
        } else {
            if line
                .iter()
                .any(|p| world.controller_of_province(*p) != Some(country))
            {
                return Err("Оборонительный приказ рисуется на контролируемой территории".into());
            }
            w.command.members(army).collect()
        };
        if units.is_empty() {
            return Err("У армии нет назначенных дивизий".into());
        }
        self.next_id += 1;
        let id = self.next_id;
        let area_states = if kind == OrderType::AreaDefense {
            line.iter()
                .filter_map(|p| world.provinces.get(p)?.state_id)
                .collect()
        } else {
            BTreeSet::new()
        };
        self.plans.insert(
            id,
            BattlePlan {
                id,
                country: country.into(),
                army,
                source_front: source,
                source_revision: 0,
                assigned_divisions: units,
                destination_line: line,
                area_states,
                order_type: kind,
                mode: ExecutionMode::Balanced,
                activation: Activation::Preparing,
                progress: 0.0,
                preparation: 0.0,
                assignments: BTreeMap::new(),
                guard: AreaGuard::default(),
                status: "Подготовка".into(),
                last_review: 0,
                control_revision: u64::MAX,
                cached_route: BTreeSet::new(),
                distance: BTreeMap::new(),
            },
        );
        self.revision += 1;
        Ok(id)
    }
    /// Drawing over the same order changes its objective rather than silently
    /// creating competing orders for the same army/front. Other fronts survive.
    pub fn draw_order(
        &mut self,
        w: &Warfare,
        world: &World,
        country: &str,
        army: ArmyId,
        source: Option<FrontId>,
        kind: OrderType,
        line: Vec<u32>,
    ) -> Result<PlanId, String> {
        let previous = self
            .plans
            .values()
            .find(|p| {
                p.country == country
                    && p.army == army
                    && p.source_front == source
                    && (p.order_type == kind
                        || (matches!(p.order_type, OrderType::Offensive | OrderType::Spearhead)
                            && matches!(kind, OrderType::Offensive | OrderType::Spearhead)))
            })
            .map(|p| p.id);
        // Validate the replacement before touching a working order.
        let new = self.create(w, world, country, army, source, kind, line)?;
        if let Some(id) = previous {
            let mut replacement = self.plans.remove(&new).unwrap();
            replacement.id = id;
            replacement.mode = self.plans[&id].mode;
            if self.plans[&id].activation == Activation::Executing {
                replacement.activation = Activation::Executing;
                replacement.status = "Цель изменена; выполнение продолжается".into();
            } else {
                replacement.status = "Цель изменена; план готов к запуску".into();
            }
            self.plans.retain(|other, p| {
                *other == id
                    || !(p.country == country
                        && p.army == army
                        && p.source_front == source
                        && (p.order_type == kind
                            || (matches!(
                                p.order_type,
                                OrderType::Offensive | OrderType::Spearhead
                            ) && matches!(kind, OrderType::Offensive | OrderType::Spearhead))))
            });
            self.plans.insert(id, replacement);
            Ok(id)
        } else {
            Ok(new)
        }
    }

    pub fn edit_line(
        &mut self,
        country: &str,
        id: PlanId,
        line: Vec<u32>,
        world: &World,
    ) -> Result<(), String> {
        let p = self
            .plans
            .get_mut(&id)
            .filter(|p| p.country == country)
            .ok_or("Выберите свой план")?;
        if line.is_empty()
            || line
                .iter()
                .any(|id| !world.geography.provinces.contains_key(id))
        {
            return Err("Нужна линия из существующих провинций".into());
        }
        if line.iter().any(|id| {
            !world
                .geography
                .provinces
                .get(id)
                .is_some_and(|p| p.definition.kind == crate::map_graph::ProvinceKind::Land)
        }) {
            return Err("Линия должна лежать на суше".into());
        }
        if matches!(p.order_type, OrderType::Fallback | OrderType::AreaDefense)
            && line
                .iter()
                .any(|id| world.controller_of_province(*id) != Some(country))
        {
            return Err("Линия обороны должна находиться на своей территории".into());
        }
        if p.order_type != OrderType::AreaDefense
            && !line
                .windows(2)
                .all(|p| p[0] == p[1] || world.geography.land_neighbors(p[0]).any(|e| e.to == p[1]))
        {
            return Err("Конечная линия должна быть непрерывной".into());
        }
        p.destination_line = if p.order_type == OrderType::Spearhead {
            vec![*line.last().unwrap()]
        } else {
            line
        };
        p.assignments.clear();
        if p.activation == Activation::Completed {
            p.activation = Activation::Preparing;
            p.status = "Цель изменена; план готов к запуску".into();
        }
        p.distance.clear();
        p.cached_route.clear();
        p.control_revision = u64::MAX;
        p.last_review = 0;
        p.progress = 0.0;
        if p.order_type == OrderType::AreaDefense {
            p.area_states = p
                .destination_line
                .iter()
                .filter_map(|id| world.provinces.get(id)?.state_id)
                .collect();
        }
        self.revision += 1;
        Ok(())
    }
    pub fn activate(&mut self, country: &str, id: PlanId, active: bool) -> Result<(), String> {
        let p = self
            .plans
            .get_mut(&id)
            .filter(|p| p.country == country)
            .ok_or("Выберите свой план")?;
        // One active movement owner per division; other armies are unaffected.
        let members = p.assigned_divisions.clone();
        p.activation = if active {
            Activation::Executing
        } else {
            Activation::Paused
        };
        p.last_review = 0;
        p.status = if active {
            "Выполнение"
        } else {
            "Приостановлен"
        }
        .into();
        if active {
            for other in self.plans.values_mut().filter(|p| {
                !p.assigned_divisions.is_disjoint(&members)
                    && p.id != id
                    && p.activation == Activation::Executing
            }) {
                other.activation = Activation::Paused;
            }
        }
        self.revision += 1;
        Ok(())
    }
    pub fn delete(&mut self, country: &str, id: PlanId) -> Result<(), String> {
        if !self.plans.get(&id).is_some_and(|p| p.country == country) {
            return Err("Выберите свой план".into());
        }
        self.plans.remove(&id);
        self.revision += 1;
        Ok(())
    }
    pub fn normalize(
        &mut self,
        w: &crate::command::CommandState,
        fronts: &crate::frontline::FrontState,
    ) {
        let before = self.plans.len();
        self.plans.retain(|_, p| {
            w.armies.contains_key(&p.army)
                && p.source_front
                    .is_none_or(|id| fronts.fronts.contains_key(&id))
        });
        let mut latest = BTreeMap::new();
        for (&id, p) in &self.plans {
            let family = match p.order_type {
                OrderType::Offensive | OrderType::Spearhead => 0,
                OrderType::Fallback => 1,
                OrderType::AreaDefense => 2,
            };
            latest.insert((p.country.clone(), p.army, p.source_front, family), id);
        }
        let keep: BTreeSet<_> = latest.into_values().collect();
        self.plans.retain(|id, _| keep.contains(id));
        if before != self.plans.len() {
            self.revision += 1;
        }
    }
    pub fn active_divisions(&self) -> BTreeSet<u64> {
        self.plans
            .values()
            .filter(|p| p.activation == Activation::Executing)
            .flat_map(|p| p.assigned_divisions.iter().copied())
            .collect()
    }
    pub fn tick(&mut self, w: &mut Warfare, world: &World, elapsed: u64) {
        self.normalize(&w.command, &w.fronts);
        let ids: Vec<_> = self.plans.keys().copied().collect();
        for id in ids {
            let Some(mut p) = self.plans.remove(&id) else {
                continue;
            };
            if let Some(f) = p.source_front.and_then(|id| w.fronts.fronts.get(&id)) {
                p.assigned_divisions = f.assigned_divisions.clone();
            }
            p.assigned_divisions.retain(|id| {
                w.divisions.get(id).is_some_and(|d| {
                    !d.destroyed
                        && !w.abilities.headquarters.contains_key(id)
                        && (w.command.membership.get(id) == Some(&p.army)
                            || p.source_front
                                .and_then(|f| w.fronts.fronts.get(&f))
                                .is_some_and(|f| {
                                    f.group.is_some() && f.assigned_divisions.contains(id)
                                }))
                })
            });
            let n = p.assigned_divisions.len().max(1) as f64;
            p.preparation = p
                .assigned_divisions
                .iter()
                .filter_map(|id| w.divisions.get(id))
                .map(|d| d.planning)
                .sum::<f64>()
                / n;
            p.progress = p
                .destination_line
                .iter()
                .filter(|id| world.controller_of_province(**id) == Some(p.country.as_str()))
                .count() as f64
                / p.destination_line.len().max(1) as f64;
            if matches!(p.order_type, OrderType::Offensive | OrderType::Spearhead)
                && p.progress >= 1.0
            {
                p.activation = Activation::Completed;
                p.status = "Цель достигнута".into();
            }
            if p.activation != Activation::Executing {
                self.plans.insert(id, p);
                continue;
            }
            let source_revision = p
                .source_front
                .and_then(|id| w.fronts.fronts.get(&id))
                .map_or(0, |f| f.revision);
            let topology = p.control_revision != world.control_revision
                || p.distance.is_empty()
                || source_revision != p.source_revision;
            if topology {
                p.distance = goal_distances(w, world, &p.country, &p.destination_line);
                if p.order_type == OrderType::Spearhead {
                    p.cached_route = spearhead_corridor(w, world, &p);
                }
                p.control_revision = world.control_revision;
                p.source_revision = source_revision;
            }
            // Event invalidation plus a bounded operational interval. Rendering
            // never invokes this solver.
            let review_interval = if p.order_type == OrderType::Offensive {
                3
            } else {
                6
            };
            if !topology
                && p.last_review != 0
                && elapsed.saturating_sub(p.last_review) < review_interval
            {
                self.plans.insert(id, p);
                continue;
            }
            p.last_review = elapsed;
            if matches!(p.order_type, OrderType::Fallback | OrderType::AreaDefense) {
                defend(&mut p, w, world);
            } else {
                advance(&mut p, w, world);
            }
            self.plans.insert(id, p);
        }
    }
}
fn goal_distances(w: &Warfare, world: &World, country: &str, goals: &[u32]) -> BTreeMap<u32, u32> {
    let mut dist = BTreeMap::new();
    let mut q = VecDeque::new();
    for &p in goals {
        dist.insert(p, 0);
        q.push_back(p);
    }
    while let Some(p) = q.pop_front() {
        let d = dist[&p];
        for e in world.geography.land_neighbors(p) {
            if !w.edge_allowed(country, e.to, p, world)
                || !world
                    .controller_of_province(e.to)
                    .is_some_and(|c| w.friendly(country, c) || w.at_war(country, c))
            {
                continue;
            }
            if let std::collections::btree_map::Entry::Vacant(v) = dist.entry(e.to) {
                v.insert(d + 1);
                q.push_back(e.to);
            }
        }
    }
    dist
}
fn spearhead_corridor(w: &Warfare, world: &World, p: &BattlePlan) -> BTreeSet<u32> {
    let mut corridor = BTreeSet::new();
    let starts: Vec<_> = p
        .source_front
        .and_then(|id| w.fronts.fronts.get(&id))
        .map(|f| f.edges.iter().map(|e| e.friendly).collect())
        .unwrap_or_default();
    if let Some(&start) = starts
        .iter()
        .min_by_key(|id| p.distance.get(id).copied().unwrap_or(u32::MAX))
    {
        let mut at = start;
        corridor.insert(at);
        while let Some(&d) = p.distance.get(&at) {
            if d == 0 {
                break;
            }
            let next = world
                .geography
                .land_neighbors(at)
                .filter(|e| w.edge_allowed(&p.country, at, e.to, world))
                .filter(|e| p.distance.get(&e.to).is_some_and(|n| *n < d))
                .min_by_key(|e| (p.distance[&e.to], e.to))
                .map(|e| e.to);
            let Some(next) = next else { break };
            at = next;
            corridor.insert(at);
        }
    }
    corridor
}
fn ready(d: &Division, mode: ExecutionMode) -> bool {
    let (org, supply, _) = mode.policy();
    d.fighting()
        && d.battle.is_none()
        && d.attack_delay == 0
        && matches!(d.order, DivisionOrder::Hold)
        && d.organization / d.derived.stats.organization.max(1.0) >= org
        && d.supply_fraction >= supply
}
/// An unopposed province needs movement, not a combat-strength comparison.
/// Do not confuse an ongoing battle with an empty province.
fn unopposed_target(w: &Warfare, world: &World, country: &str, target: u32) -> bool {
    world
        .controller_of_province(target)
        .is_some_and(|c| w.at_war(country, c))
        && !w.divisions.values().any(|d| {
            d.province == target
                && !d.destroyed
                && d.fighting()
                && w.at_war(country, &d.country)
                && !matches!(d.order, DivisionOrder::Retreat { .. })
        })
        && !w
            .battles
            .values()
            .any(|b| b.province == target && b.outcome.is_none())
}

/// Two controlled approaches allow taking the mouth/side of a salient, even
/// when the province is not strictly closer to the drawn final line.
/// This does not require the rest of the pocket to be fully surrounded.
fn exposed_salient(world: &World, country: &str, target: u32) -> bool {
    world
        .geography
        .land_neighbors(target)
        .filter(|e| world.controller_of_province(e.to) == Some(country))
        .take(2)
        .count()
        >= 2
}

fn advance(p: &mut BattlePlan, w: &mut Warfare, world: &World) {
    if let Some(neutral) = p
        .destination_line
        .iter()
        .filter_map(|id| world.controller_of_province(*id))
        .find(|c| !w.friendly(&p.country, c) && !w.at_war(&p.country, c))
    {
        p.status = format!("Нет войны с {neutral}. Объявите войну в дипломатии");
        return;
    }
    let Some(front) = p.source_front.and_then(|id| w.fronts.fronts.get(&id)) else {
        p.status = "У приказа больше нет исходного фронта".into();
        return;
    };
    if front.edges.is_empty() {
        p.status = "На исходном фронте больше нет границы".into();
        return;
    }
    let enemy_regions = crate::frontline::controlled_regions(world, &front.enemy_country);
    let goal_regions: BTreeSet<_> = p
        .destination_line
        .iter()
        .filter_map(|id| enemy_regions.get(id).copied())
        .collect();
    let enclosed: BTreeSet<_> = front
        .edges
        .iter()
        .filter(|e| !goal_regions.contains(enemy_regions.get(&e.enemy).unwrap_or(&e.enemy)))
        .filter(|e| {
            let region = enemy_regions.get(&e.enemy);
            world.geography.land_neighbors(e.enemy).all(|n| {
                enemy_regions.get(&n.to) == region
                    || world.controller_of_province(n.to) == Some(p.country.as_str())
            })
        })
        .map(|e| e.enemy)
        .collect();
    let (_, _, risk) = p.mode.policy();
    let mut attacks = BTreeMap::<u32, Vec<u64>>::new();
    let mut origins = BTreeMap::<u32, BTreeSet<u32>>::new();
    let mut planned_targets = BTreeMap::<u32, usize>::new();
    let mut moves = Vec::new();
    let units: Vec<_> = p
        .assigned_divisions
        .iter()
        .copied()
        .filter(|id| !w.fronts.manual.contains(id) && !w.fronts.detached.contains(id))
        .collect();
    for id in units {
        let Some(d) = w.divisions.get(&id) else {
            continue;
        };
        // A lightly supplied/organised division may still march into an
        // undefended enemy province. Defended attacks keep the normal risk and
        // readiness policies; never bypass battle, retreat or attack delay.
        let available = d.fighting()
            && d.battle.is_none()
            && d.attack_delay == 0
            && d.derived.stats.speed > 0.0
            && matches!(d.order, DivisionOrder::Hold);
        if !available {
            continue;
        }
        let battle_ready = ready(d, p.mode);
        let assigned = p
            .source_front
            .and_then(|front| w.fronts.fronts.get(&front))
            .and_then(|f| f.assignments.get(&id))
            .copied()
            .filter(|t| *t != d.province);
        let here = p.distance.get(&d.province).copied().unwrap_or(u32::MAX);
        let targets: Vec<_> = world
            .geography
            .land_neighbors(d.province)
            .filter(|e| w.edge_allowed(&p.country, d.province, e.to, world))
            .filter(|e| {
                world
                    .controller_of_province(e.to)
                    .is_some_and(|c| w.at_war(&p.country, c))
            })
            .filter(|e| {
                let forward = p.distance.get(&e.to).is_some_and(|n| *n < here);
                let close_salient = p.order_type == OrderType::Offensive
                    && exposed_salient(world, &p.country, e.to)
                    && p.distance
                        .get(&e.to)
                        .is_some_and(|n| *n <= here.saturating_add(1));
                forward
                    || close_salient
                    || (p.order_type == OrderType::Offensive && enclosed.contains(&e.to))
            })
            .filter(|e| p.order_type != OrderType::Spearhead || p.cached_route.contains(&e.to))
            .filter(|e| battle_ready || unopposed_target(w, world, &p.country, e.to))
            .collect();
        let target = targets
            .into_iter()
            .min_by(|a, b| {
                let score = |target: u32| -> f64 {
                    let terrain = &world.geography.provinces[&target].definition.terrain;
                    let base = w.catalogue.default_terrain(terrain);
                    let crossing = world
                        .geography
                        .connection(d.province, target)
                        .map(|e| e.river);
                    let river = match crossing {
                        Some(crate::map_graph::RiverCrossing::PaletteIndex(i))
                            if i > w.catalogue.rules.small_river_index =>
                        {
                            w.catalogue.rules.large_river_attack
                        }
                        Some(crate::map_graph::RiverCrossing::PaletteIndex(_)) => {
                            w.catalogue.rules.small_river_attack
                        }
                        _ => 0.0,
                    };
                    let forts = w.conditions.get(&target).map_or(0, |c| c.fort_level) as f64;
                    // Local easy occupations should not wait for every guarded
                    // frontal approach to meet a risk threshold first.
                    let empty_bonus = if unopposed_target(w, world, &p.country, target) {
                        -3.0
                    } else {
                        0.0
                    };
                    let pocket_bonus = if exposed_salient(world, &p.country, target) {
                        -1.0
                    } else {
                        0.0
                    };
                    p.distance.get(&target).copied().unwrap_or(u32::MAX / 4) as f64
                        + base.movement_cost
                        + forts
                        - river
                        + empty_bonus
                        + pocket_bonus
                        + planned_targets.get(&target).copied().unwrap_or(0) as f64
                };
                score(a.to).total_cmp(&score(b.to)).then(a.to.cmp(&b.to))
            })
            .map(|e| e.to);
        // Take a nearby undefended opening before walking away to rebalance the
        // frontline. Guarded assaults still wait until assigned units arrive.
        if let Some(target) =
            target.filter(|t| assigned.is_none() || unopposed_target(w, world, &p.country, *t))
        {
            attacks.entry(target).or_default().push(id);
            origins.entry(target).or_default().insert(d.province);
            *planned_targets.entry(target).or_default() += 1;
        } else if battle_ready {
            if let Some(target) = assigned {
                moves.push((id, target));
            }
        }
    }
    let relocating = moves.len();
    for (id, target) in moves {
        let _ = w.issue_move(&p.country, &[id], target, world, false, false);
    }
    let mut launched = 0;
    for (target, mut units) in attacks {
        let terrain = &world.geography.provinces[&target].definition.terrain;
        let t = w.catalogue.default_terrain(terrain);
        let width = t.width + t.extra_width * origins[&target].len().saturating_sub(1) as f64;
        units.sort_by(|a, b| {
            let score = |id: &u64| {
                let d = &w.divisions[id];
                d.derived.stats.breakthrough * d.combat_strength() * (1.0 + d.planning)
            };
            score(b).total_cmp(&score(a)).then(a.cmp(b))
        });
        let mut chosen = Vec::new();
        let mut used = 0.0;
        for id in units {
            let width_d = w.divisions[&id].derived.stats.width;
            if w.catalogue.rules.can_fit(used, width_d, width) {
                used += width_d;
                chosen.push(id);
            }
        }
        // Only enemies in this adjacent province are observed. No distant enemy
        // templates, future orders or stockpiles enter the prediction.
        let enemies: Vec<_> = w
            .divisions
            .values()
            .filter(|d| !d.destroyed && d.province == target && w.at_war(&p.country, &d.country))
            .collect();
        let own: f64 = chosen
            .iter()
            .map(|id| {
                let d = &w.divisions[id];
                let adjusted = d.derived.terrain.get(terrain).map_or(0.0, |v| v.attack);
                (d.derived.stats.soft_attack + d.derived.stats.hard_attack)
                    * d.combat_strength()
                    * (1.0 + d.planning)
                    * (1.0 + t.attack + adjusted).max(0.0)
            })
            .sum();
        let enemy: f64 = enemies
            .iter()
            .map(|d| {
                (d.derived.stats.soft_attack + d.derived.stats.hard_attack) * d.combat_strength()
            })
            .sum();
        let forts = w.conditions.get(&target).map_or(0, |c| c.fort_level) as f64;
        if !enemies.is_empty()
            && own * (1.0 + w.catalogue.rules.fort_penalty * forts).max(0.0) < enemy * risk
        {
            continue;
        }
        if !chosen.is_empty() {
            match w.attack(&p.country, &chosen, target, world) {
                Ok(_) => {
                    launched += chosen.len();
                }
                Err(e) => p.status = e,
            }
        }
    }
    let engaged = p
        .assigned_divisions
        .iter()
        .filter(|id| w.divisions.get(id).is_some_and(|d| d.battle.is_some()))
        .count();
    if launched > 0 {
        p.status = format!("Начали атаку: {launched}");
    } else if engaged > 0 {
        p.status = format!("В сражениях: {engaged} дивизий");
    } else if relocating > 0 {
        p.status = format!("Выдвигаются на позиции: {relocating} дивизий");
    } else if p
        .assigned_divisions
        .iter()
        .all(|id| w.fronts.manual.contains(id) || w.fronts.detached.contains(id))
    {
        p.status = "Назначенные дивизии сейчас под ручным управлением".into();
    } else {
        p.status = "Ожидание организации, снабжения или подходящего направления".into();
    }
}
fn defend(p: &mut BattlePlan, w: &mut Warfare, world: &World) {
    let targets: BTreeSet<u32> = if p.order_type == OrderType::Fallback {
        p.destination_line
            .iter()
            .copied()
            .filter(|id| world.controller_of_province(*id) == Some(p.country.as_str()))
            .collect()
    } else {
        p.area_states
            .iter()
            .filter_map(|id| world.states.get(id))
            .flat_map(|s| s.province_ids.iter().copied())
            .filter(|id| world.controller_of_province(*id) == Some(p.country.as_str()))
            .filter(|id| {
                let c = w.conditions.get(id);
                (p.guard.ports && c.is_some_and(|c| c.naval_base > 0))
                    || (p.guard.victory_points && c.is_some_and(|c| c.victory_points > 0.0))
                    || (p.guard.forts && c.is_some_and(|c| c.fort_level > 0))
                    || (p.guard.supply_nodes && c.is_some_and(|c| c.supply_node))
                    || p.guard.coast
                        && world.geography.connections_from(*id).iter().any(|e| {
                            world.geography.provinces.get(&e.to).is_some_and(|p| {
                                p.definition.kind == crate::map_graph::ProvinceKind::Sea
                            })
                        })
            })
            .collect()
    };
    if targets.is_empty() {
        p.status = "В области нет выбранных объектов охраны".into();
        return;
    }
    let units: Vec<_> = p
        .assigned_divisions
        .iter()
        .filter_map(|id| w.divisions.get(id))
        .filter(|d| {
            !w.fronts.manual.contains(&d.id)
                && !w.fronts.detached.contains(&d.id)
                && d.battle.is_none()
        })
        .collect();
    let mut slots = Vec::new();
    let mut target_iter = targets.iter().cycle();
    for _ in &units {
        slots.push(*target_iter.next().unwrap());
    }
    let mut cache = BTreeMap::new();
    for d in &units {
        cache
            .entry(d.province)
            .or_insert_with(|| w.movement_distances(&p.country, d.province, world));
    }
    let costs: Vec<Vec<f64>> = units
        .iter()
        .map(|d| {
            slots
                .iter()
                .map(|t| {
                    cache[&d.province].get(t).copied().unwrap_or(1e12)
                        / d.derived.stats.speed.max(0.01)
                        + if p.assignments.get(&d.id) == Some(t) {
                            0.0
                        } else {
                            6.0
                        }
                })
                .collect()
        })
        .collect();
    let matched = minimum_cost_assignment(&costs);
    p.assignments.clear();
    let mut orders = Vec::new();
    for (i, j) in matched.into_iter().enumerate() {
        if costs[i][j] < 1e11 {
            p.assignments.insert(units[i].id, slots[j]);
            if units[i].province != slots[j]
                && !units[i].destination.is_some_and(|o| o.province == slots[j])
            {
                orders.push((units[i].id, slots[j]));
            }
        }
    }
    for (id, target) in orders {
        let _ = w.issue_move(&p.country, &[id], target, world, false, false);
    }
    p.status = format!("Охраняемых позиций: {}", targets.len());
}

/// A drawn inland line is an ordered chain of real adjacent provinces.
pub fn province_line(
    world: &World,
    start: u32,
    end: u32,
    country: &str,
    foreign: Option<&str>,
) -> Result<Vec<u32>, String> {
    let mut prev = BTreeMap::from([(start, start)]);
    let mut q = VecDeque::from([start]);
    while let Some(p) = q.pop_front() {
        if p == end {
            break;
        }
        for e in world.geography.land_neighbors(p) {
            if !world
                .controller_of_province(e.to)
                .is_some_and(|c| c == country || Some(c) == foreign)
            {
                continue;
            }
            if let std::collections::btree_map::Entry::Vacant(v) = prev.entry(e.to) {
                v.insert(p);
                q.push_back(e.to);
            }
        }
    }
    if !prev.contains_key(&end) {
        return Err("Линия пересекает непроходимый или чужой участок".into());
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        frontline::{FrontEdge, FrontState},
        game::GameDate,
    };
    pub fn configured(kind: OrderType) -> (World, Warfare, u64, u64) {
        let (world, mut w) = crate::frontline::tests::scenario();
        let ids: Vec<_> = (1..=12).collect();
        let a = w.create_army("A", &ids, "").unwrap();
        let mut f = FrontState::default();
        let front = f
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
        w.fronts = f;
        w.declare_war("A", "B", &world).unwrap();
        let mut ops = OperationalState::default();
        let plan = ops
            .create(&w, &world, "A", a, Some(front), kind, (17..=24).collect())
            .unwrap();
        w.plans = ops;
        (world, w, front, plan)
    }
    fn tick(w: &mut Warfare, world: &mut World, h: u64) {
        w.tick_hour(world, GameDate::START, (h % 24) as u8, h, Some("A"));
    }
    #[test]
    fn autoplan_takes_an_empty_neighbor_before_rebalancing_its_front() {
        let (world, mut w, front, plan) = configured(OrderType::Offensive);
        let mut p = w.plans.plans.remove(&plan).unwrap();
        p.distance = goal_distances(&w, &world, "A", &p.destination_line);
        p.assigned_divisions = BTreeSet::from([1]);
        // Previous policy chose the (friendly) assignment before even
        // looking at the unguarded adjacent enemy province 9.
        w.fronts
            .fronts
            .get_mut(&front)
            .unwrap()
            .assignments
            .insert(1, 2);
        advance(&mut p, &mut w, &world);
        assert!(matches!(&w.divisions[&1].order,
            DivisionOrder::Move { path, .. } if path == &vec![9]));
        assert_eq!(world.controller_of_province(9), Some("B"));
    }
    #[test]
    fn empty_provinces_do_not_wait_for_battle_readiness_but_defended_ones_do() {
        let (world, mut w, _, plan) = configured(OrderType::Offensive);
        let mut p = w.plans.plans.remove(&plan).unwrap();
        p.distance = goal_distances(&w, &world, "A", &p.destination_line);
        p.assigned_divisions = BTreeSet::from([1]);
        w.divisions.get_mut(&1).unwrap().organization = 1.;
        w.divisions.get_mut(&1).unwrap().supply_fraction = 0.1;
        advance(&mut p, &mut w, &world);
        assert!(matches!(&w.divisions[&1].order,
            DivisionOrder::Move { path, .. } if path == &vec![9]));
        // A live defending formation restores the normal cautious policy.
        w.divisions.get_mut(&1).unwrap().order = DivisionOrder::Hold;
        w.divisions
            .insert(99, crate::military_model::tests::division(99, "B", 9));
        advance(&mut p, &mut w, &world);
        assert!(matches!(w.divisions[&1].order, DivisionOrder::Hold));
        assert!(w.battles.is_empty());
    }
    #[test]
    fn autoplan_closes_a_lateral_half_pocket_without_full_encirclement() {
        let (mut world, mut w, front, plan) = configured(OrderType::Offensive);
        // In this 8x3 grid, the enemy holds 12, 13, 19 and 20.
        // Province 12 connects to the wider enemy region, and is at the
        // SAME goal distance as the friendly 11: strict downhill and the old
        // fully-enclosed special case would both refuse the closing move.
        world.set_province_controller(11, Some("A"));
        w.divisions.get_mut(&1).unwrap().province = 11;
        w.divisions
            .insert(99, crate::military_model::tests::division(99, "B", 19));
        w.fronts
            .fronts
            .get_mut(&front)
            .unwrap()
            .assignments
            .insert(1, 11);
        let mut p = w.plans.plans.remove(&plan).unwrap();
        p.distance = goal_distances(&w, &world, "A", &p.destination_line);
        p.assigned_divisions = BTreeSet::from([1]);
        assert_eq!(p.distance[&11], p.distance[&12]);
        assert!(exposed_salient(&world, "A", 12));
        assert!(!unopposed_target(&w, &world, "A", 19));
        advance(&mut p, &mut w, &world);
        assert!(
            matches!(&w.divisions[&1].order,
            DivisionOrder::Move { path, .. } if path == &vec![12]),
            "The plan must close the exposed salient without waiting for a complete ring"
        );
    }
    #[test]
    fn offensive_orders_use_real_movement_and_reach_the_target_line() {
        let (mut world, mut w, front, plan) = configured(OrderType::Offensive);
        w.plans.activate("A", plan, true).unwrap();
        assert_eq!(world.controller_of_province(9), Some("B"));
        for h in 1..=96 {
            tick(&mut w, &mut world, h);
            if w.plans.plans[&plan].activation == Activation::Completed {
                break;
            }
        }
        assert!(
            (17..=24).all(|p| world.controller_of_province(p) == Some("A")),
            "{}",
            w.plans.plans[&plan].status
        );
        assert_eq!(w.plans.plans[&plan].activation, Activation::Completed);
        assert_eq!(w.command.members(w.fronts.fronts[&front].army).count(), 12);
    }
    #[test]
    fn a_manual_division_keeps_its_order_while_the_other_units_advance() {
        let (mut world, mut w, _, plan) = configured(OrderType::Offensive);
        for h in 1..=4 {
            tick(&mut w, &mut world, h);
        }
        w.issue_move("A", &[1], 1, &world, false, false).unwrap();
        w.stop("A", 1, &world).unwrap();
        w.plans.activate("A", plan, true).unwrap();
        for h in 5..=12 {
            tick(&mut w, &mut world, h);
        }
        assert!(w.fronts.manual.contains(&1));
        assert_eq!(w.divisions[&1].province, 1);
        assert!(w.divisions.values().any(|d| d.id != 1 && d.province > 8));
        w.resume_plans("A", &[1]).unwrap();
        assert!(!w.fronts.manual.contains(&1));
    }
    #[test]
    fn execution_modes_change_readiness_and_unassigned_hold_does_not_plan() {
        let (mut world, mut w, _, plan) = configured(OrderType::Offensive);
        for d in w.divisions.values_mut() {
            d.organization = d.derived.stats.organization * 0.3;
            d.supply_fraction = 0.3;
        }
        w.plans.plans.get_mut(&plan).unwrap().mode = ExecutionMode::Cautious;
        w.plans.activate("A", plan, true).unwrap();
        tick(&mut w, &mut world, 1);
        assert_eq!(world.controller_of_province(9), Some("B"));
        w.plans.plans.get_mut(&plan).unwrap().mode = ExecutionMode::Aggressive;
        w.plans.plans.get_mut(&plan).unwrap().last_review = 0;
        tick(&mut w, &mut world, 2);
        assert!(
            w.divisions
                .values()
                .any(|d| matches!(d.order, DivisionOrder::Move { .. })),
            "aggressive mode must issue an order, not teleport a division"
        );
        for h in 3..=8 {
            tick(&mut w, &mut world, h);
        }
        assert_eq!(world.controller_of_province(9), Some("A"));
        let (mut world, mut alone) = crate::frontline::tests::scenario();
        tick(&mut alone, &mut world, 1);
        assert_eq!(alone.divisions[&1].planning, 0.0);
    }
    #[test]
    fn fallback_and_area_defense_use_existing_provinces_and_imported_guard_sites() {
        let (mut world, mut w) = crate::frontline::tests::scenario();
        let ids: Vec<_> = (1..=12).collect();
        let a = w.create_army("A", &ids, "").unwrap();
        let mut ops = OperationalState::default();
        let p = ops
            .create(
                &w,
                &world,
                "A",
                a,
                None,
                OrderType::Fallback,
                vec![5, 6, 7, 8],
            )
            .unwrap();
        ops.activate("A", p, true).unwrap();
        w.plans = ops;
        for h in 1..=4 {
            tick(&mut w, &mut world, h);
        }
        assert!(
            w.divisions
                .values()
                .all(|d| [5, 6, 7, 8].contains(&d.province))
        );
        let mut ops = std::mem::take(&mut w.plans);
        let area = ops
            .create(&w, &world, "A", a, None, OrderType::AreaDefense, vec![1])
            .unwrap();
        ops.activate("A", area, true).unwrap();
        w.plans = ops;
        w.conditions.entry(2).or_default().naval_base = 1;
        w.conditions.entry(4).or_default().victory_points = 10.0;
        for h in 5..=12 {
            tick(&mut w, &mut world, h);
        }
        assert!(w.divisions.values().all(|d| [2, 4].contains(&d.province)));
    }
}

#[cfg(test)]
mod interaction_regressions {
    use super::*;
    #[test]
    fn redrawing_an_offensive_or_spearhead_reuses_the_front_order() {
        let (world, mut w, front, id) = tests::configured(OrderType::Offensive);
        let mut plans = std::mem::take(&mut w.plans);
        for _ in 0..50 {
            assert_eq!(
                plans
                    .draw_order(
                        &w,
                        &world,
                        "A",
                        plans.plans[&id].army,
                        Some(front),
                        OrderType::Spearhead,
                        vec![24]
                    )
                    .unwrap(),
                id
            );
        }
        assert_eq!(plans.plans.len(), 1);
        assert_eq!(plans.plans[&id].destination_line, vec![24]);
        assert!(
            plans
                .draw_order(
                    &w,
                    &world,
                    "A",
                    plans.plans[&id].army,
                    Some(front),
                    OrderType::Offensive,
                    vec![99999]
                )
                .is_err()
        );
        assert_eq!(plans.plans.len(), 1);
        assert_eq!(plans.plans[&id].destination_line, vec![24]);
    }
    #[test]
    fn manual_waypoints_finish_then_rejoin_without_stopping_the_army() {
        let (mut world, mut w, _, plan) = tests::configured(OrderType::Offensive);
        w.divisions.get_mut(&1).unwrap().province = 1;
        w.divisions.get_mut(&1).unwrap().derived.stats.speed = 0.25;
        w.issue_move("A", &[1], 2, &world, false, false).unwrap();
        w.issue_move("A", &[1], 3, &world, false, true).unwrap();
        w.plans.activate("A", plan, true).unwrap();
        w.tick_hour(&mut world, crate::game::GameDate::START, 1, 1, Some("A"));
        assert!(w.fronts.manual.contains(&1));
        assert_eq!(w.divisions[&1].destination.unwrap().province, 2);
        assert_eq!(w.divisions[&1].waypoints.back().unwrap().province, 3);
        assert!(
            w.divisions
                .values()
                .any(|d| d.id != 1 && !matches!(d.order, DivisionOrder::Hold))
        );
        w.divisions.get_mut(&1).unwrap().derived.stats.speed = 100.0;
        for h in 2..=20 {
            w.tick_hour(
                &mut world,
                crate::game::GameDate::START,
                (h % 24) as u8,
                h,
                Some("A"),
            );
            if !w.fronts.manual.contains(&1) {
                break;
            }
        }
        assert!(!w.fronts.manual.contains(&1));
        assert!(!w.fronts.manual_return.contains(&1));
        assert!(w.command.membership.contains_key(&1));
        w.stop("A", 1, &world).unwrap();
        w.tick_hour(&mut world, crate::game::GameDate::START, 21, 21, Some("A"));
        assert!(w.fronts.manual.contains(&1), "Explicit Hold remains manual");
        w.resume_plans("A", &[1]).unwrap();
        assert!(!w.fronts.manual.contains(&1));
    }
    #[test]
    fn a_peacetime_plan_explains_why_it_is_waiting() {
        let (mut world, mut w, _, plan) = tests::configured(OrderType::Offensive);
        w.wars.clear();
        w.plans.activate("A", plan, true).unwrap();
        w.tick_hour(&mut world, crate::game::GameDate::START, 1, 1, Some("A"));
        assert!(w.plans.plans[&plan].status.contains("Нет войны"));
        w.declare_war("A", "B", &world).unwrap();
        for h in 2..=100 {
            w.tick_hour(
                &mut world,
                crate::game::GameDate::START,
                (h % 24) as u8,
                h,
                Some("A"),
            );
        }
        assert_eq!(w.plans.plans[&plan].activation, Activation::Completed);
    }
}

#[cfg(test)]
mod order_lifecycle {
    use super::*;
    #[test]
    fn edit_and_delete_clear_automatic_routes_but_preserve_manual_control() {
        let (world, mut w, front, plan) = super::tests::configured(OrderType::Offensive);
        w.issue_move("A", &[1], 2, &world, false, false).unwrap();
        w.automatic_order = true;
        w.issue_move("A", &[2], 3, &world, false, false).unwrap();
        w.automatic_order = false;
        let manual = w.divisions[&1].destination;
        let army = w.plans.plans[&plan].army;
        w.draw_plan(
            &world,
            "A",
            army,
            Some(front),
            OrderType::Offensive,
            vec![18, 19],
            None,
        )
        .unwrap();
        assert_eq!(w.divisions[&1].destination, manual);
        assert!(w.fronts.manual.contains(&1));
        assert!(w.divisions[&2].destination.is_none());
        assert_eq!(w.plans.plans.len(), 1);
        let edges = w.fronts.fronts[&front].edges.clone();
        assert_eq!(
            w.draw_front(
                &world,
                "A",
                army,
                Some(front),
                &edges,
                &BTreeSet::new(),
                false
            )
            .unwrap(),
            front
        );
        assert_eq!(w.fronts.fronts.len(), 1);
        assert_eq!(w.plans.plans[&plan].source_front, Some(front));
        w.delete_front("A", front).unwrap();
        assert!(w.fronts.fronts.is_empty());
        assert!(w.plans.plans.is_empty());
        assert_eq!(w.divisions[&1].destination, manual);
    }
    #[test]
    fn legacy_duplicates_are_removed_and_editing_a_completed_goal_makes_it_preparable() {
        let (world, mut w, front, plan) = super::tests::configured(OrderType::Offensive);
        let army = w.plans.plans[&plan].army;
        let mut ops = std::mem::take(&mut w.plans);
        let newer = ops
            .create(
                &w,
                &world,
                "A",
                army,
                Some(front),
                OrderType::Spearhead,
                vec![24],
            )
            .unwrap();
        w.plans = ops;
        w.clean_references();
        assert_eq!(w.plans.plans.len(), 1);
        assert!(w.plans.plans.contains_key(&newer));
        w.plans.plans.get_mut(&newer).unwrap().activation = Activation::Completed;
        w.plans.edit_line("A", newer, vec![23], &world).unwrap();
        assert_eq!(w.plans.plans[&newer].activation, Activation::Preparing);
        let mut view = crate::army_ui::ArmyView::default();
        view.selected_army = Some(army);
        view.selected_front = Some(front);
        view.selected_plan = Some(newer);
        view.selected_order = Some(crate::battle_plan_render::OrderRef::Front(front));
        crate::battle_plan_ui::delete_selected(&mut view, &mut w, "A").unwrap();
        assert!(w.plans.plans.is_empty());
        view.order_map.sync(&world, &w, "A");
        assert!(view.order_map.shapes.is_empty());
    }
    #[test]
    fn army_play_controls_all_its_disjoint_orders_and_does_not_touch_another_army() {
        let (world, mut w, front, plan) = super::tests::configured(OrderType::Offensive);
        let army = w.fronts.fronts[&front].army;
        w.set_army_execution("A", army, true).unwrap();
        assert_eq!(w.plans.plans[&plan].activation, Activation::Executing);
        w.set_army_execution("A", army, false).unwrap();
        assert_eq!(w.plans.plans[&plan].activation, Activation::Paused);
        assert!(w.set_army_execution("B", army, true).is_err());
        let _ = world;
    }
}
