//! Stateful logistics, deliberately separated from land combat. Numeric defaults
//! are a versioned mobile approximation, not certified HOI4 1.19.3 constants.
use crate::{
    combat_rules::CombatRandom,
    map_graph::{CrossingKind, ProvinceKind},
    military_model::DivisionOrder,
    warfare::Warfare,
    world::World,
};
use serde::{Deserialize, Serialize};
pub fn supply_demand(d: &crate::military_model::Division) -> f64 {
    d.derived.stats.supply.max(0.) * (1. + d.command.supply).max(0.)
}
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LogisticsRules {
    pub profile: String,
    pub capital_capacity: f64,
    pub rail_capacity_per_level: f64,
    pub hub_capacity: f64,
    pub port_capacity_per_level: f64,
    pub supply_per_convoy: f64,
    pub local_supply_per_infrastructure: f64,
    pub last_mile_hops: u32,
    pub motorized_extra_hops: u32,
    pub trucks_per_hub: u64,
    pub supply_reserve_hours: f64,
    pub fuel_reserve_hours: f64,
    pub idle_fuel_factor: f64,
    pub minimum_supply_speed: f64,
    pub minimum_fuel_speed: f64,
    pub attrition_loss_factor: f64,
    pub supply_attrition: f64,
    pub exercise_attrition: f64,
    pub refinery_fuel_per_day: f64,
    pub oil_fuel_per_day: f64,
    pub base_fuel_capacity: f64,
    pub silo_fuel_capacity: f64,
    /// Fuel units occupy this many supply-equivalent transport units. This is
    /// an explicit mobile profile coefficient, not a recovered engine define.
    pub fuel_transport_weight: f64,
    /// Cargo uses the same network flow as ordinary supply and fuel.
    /// These weights are explicitly calibrated mobile approximations.
    pub equipment_transport_weight: f64,
    pub manpower_transport_weight: f64,
    pub max_reinforcement_daily: f64,
    pub network_interval: u64,
}
impl Default for LogisticsRules {
    fn default() -> Self {
        Self {
            profile: "mobile-logistics-v2-approximation".into(),
            capital_capacity: 30.,
            rail_capacity_per_level: 5.,
            hub_capacity: 20.,
            port_capacity_per_level: 3.,
            supply_per_convoy: 0.2,
            local_supply_per_infrastructure: 0.05,
            last_mile_hops: 3,
            motorized_extra_hops: 2,
            trucks_per_hub: 40,
            supply_reserve_hours: 48.,
            fuel_reserve_hours: 24.,
            idle_fuel_factor: 0.1,
            minimum_supply_speed: 0.5,
            minimum_fuel_speed: 0.1,
            attrition_loss_factor: 0.1,
            supply_attrition: 0.3,
            exercise_attrition: 0.06,
            refinery_fuel_per_day: 48.,
            oil_fuel_per_day: 48.,
            base_fuel_capacity: 100_000.,
            silo_fuel_capacity: 100_000.,
            fuel_transport_weight: 1.,
            equipment_transport_weight: 0.01,
            manpower_transport_weight: 0.001,
            max_reinforcement_daily: 2.,
            network_interval: 6,
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CountryLogistics {
    /// Province, resolved from the selected country history's capital state.
    pub capital: Option<u32>,
    pub fuel: f64,
    pub fuel_capacity: f64,
    /// Fixed national capacity, excluding dynamically controlled fuel silos.
    /// None migrates existing profiles/saves on the first logistics tick.
    pub fuel_capacity_base: Option<f64>,
    /// Explicit extraction/import input, in fuel units/day. No free oil is invented.
    pub fuel_income_daily: f64,
    /// Domestic plus delivered imported oil in resources/day; computed by
    /// the daily trade market. None retains legacy support-import profiles.
    pub oil_supply_daily: Option<f64>,
    pub convoys: u64,
    /// Reserved by international trade. These hulls cannot simultaneously
    /// serve overseas military supply.
    pub trade_convoys_committed: u64,
    pub trucks: u64,
    pub truck_models: BTreeMap<String, u64>,
    pub fuel_used_last_hour: f64,
}
impl CountryLogistics {
    pub fn truck_count(&self) -> u64 {
        self.trucks
            .saturating_add(self.truck_models.values().sum::<u64>())
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DivisionLogistics {
    /// Stocks are in supply/fuel consumption units, not fractions.
    pub supply: f64,
    pub fuel: f64,
    pub received: f64,
    pub demand: f64,
    pub reason: String,
    pub equipment_lost: u64,
    #[serde(default)]
    pub fuel_received: f64,
    #[serde(default)]
    pub transport_received: f64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LogisticsState {
    /// Version of persisted dispatch allocations (v1 carried only supply).
    pub dispatch_schema: u32,
    pub enabled: bool,
    pub rules: LogisticsRules,
    pub countries: BTreeMap<String, CountryLogistics>,
    /// Undirected, validated adjacent rail segments. Ordered endpoint pair.
    pub rails: Vec<RailSegment>,
    pub oil_by_state: BTreeMap<u32, f64>,
    pub motorized_hubs: BTreeSet<u32>,
    pub blocked_sea: BTreeSet<u32>,
    pub divisions: BTreeMap<u64, DivisionLogistics>,
    pub last_hour: Option<u64>,
    /// Current dispatch area, retained with allocations until the next network pulse.
    pub serviced: BTreeSet<(String, u32)>,
    #[serde(skip)]
    pub local_counts: BTreeMap<u32, u32>,
    pub delivery: BTreeMap<u64, f64>,
    /// Network requests are refreshed from actual national stockpiles every hour.
    pub reinforcement_requests: BTreeMap<u64, f64>,
    /// Carry-over of network-allocated reinforcement cargo, in supply-equivalent units.
    pub reinforcement_credit: BTreeMap<u64, f64>,
    pub network_key: Option<(u64, u64)>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RailSegment {
    pub from: u32,
    pub to: u32,
    pub level: u32,
}

// Residual flow graph: rail and port bottlenecks are shared by all sinks.
#[derive(Clone)]
struct Flow {
    edges: Vec<Vec<(usize, usize, f64)>>,
}
impl Flow {
    fn new(n: usize) -> Self {
        Self {
            edges: vec![Vec::new(); n],
        }
    }
    fn node(&mut self) -> usize {
        let n = self.edges.len();
        self.edges.push(Vec::new());
        n
    }
    fn add(&mut self, a: usize, b: usize, cap: f64) -> usize {
        let ai = self.edges[a].len();
        let bi = self.edges[b].len();
        self.edges[a].push((b, bi, cap.max(0.)));
        self.edges[b].push((a, ai, 0.));
        ai
    }
    fn send(&mut self, v: usize, sink: usize, limit: f64, levels: &[i32], at: &mut [usize]) -> f64 {
        if v == sink {
            return limit;
        }
        while at[v] < self.edges[v].len() {
            let i = at[v];
            let (to, rev, cap) = self.edges[v][i];
            if cap > 1e-8 && levels[to] == levels[v] + 1 {
                let sent = self.send(to, sink, limit.min(cap), levels, at);
                if sent > 1e-8 {
                    self.edges[v][i].2 -= sent;
                    self.edges[to][rev].2 += sent;
                    return sent;
                }
            }
            at[v] += 1;
        }
        0.
    }
    fn solve(&mut self, source: usize, sink: usize) {
        loop {
            let mut level = vec![-1; self.edges.len()];
            level[source] = 0;
            let mut q = VecDeque::from([source]);
            while let Some(v) = q.pop_front() {
                for &(to, _, cap) in &self.edges[v] {
                    if cap > 1e-8 && level[to] < 0 {
                        level[to] = level[v] + 1;
                        q.push_back(to);
                    }
                }
            }
            if level[sink] < 0 {
                break;
            }
            let mut at = vec![0; level.len()];
            while self.send(source, sink, f64::MAX, &level, &mut at) > 1e-8 {}
        }
    }
}
fn controlled(w: &Warfare, world: &World, tag: &str, id: u32) -> bool {
    world.controller_of_province(id).is_some_and(|owner| {
        owner == tag
            || (!w.at_war(tag, owner) && w.military_access.contains(&(tag.into(), owner.into())))
    })
}
fn reachable(w: &Warfare, world: &World, tag: &str, start: u32, hops: u32) -> BTreeMap<u32, u32> {
    let mut seen = BTreeMap::from([(start, 0)]);
    let mut q = VecDeque::from([start]);
    while let Some(v) = q.pop_front() {
        let d = seen[&v];
        if d >= hops {
            continue;
        }
        for e in world.geography.land_neighbors(v) {
            if e.kind == CrossingKind::Border
                && controlled(w, world, tag, e.to)
                && !seen.contains_key(&e.to)
            {
                seen.insert(e.to, d + 1);
                q.push_back(e.to);
            }
        }
    }
    seen
}
impl LogisticsState {
    /// Shared physical convoy pool; free hulls remain for supply shipping.
    pub fn set_trade_convoys(&mut self, tag: &str, committed: u64) {
        let c = self.countries.entry(tag.to_owned()).or_default();
        let actual = committed.min(c.convoys);
        if c.trade_convoys_committed != actual {
            c.trade_convoys_committed = actual;
            self.network_key = None;
        }
    }
    /// A motorized hub owns reserved trucks; production stocks cannot be reused
    /// for divisions while those vehicles are assigned to logistics.
    pub fn reserve_trucks(
        &mut self,
        production: &mut crate::production::ProductionState,
        world: &World,
        catalogue: &crate::military_model::MilitaryCatalogue,
    ) {
        let mut reservation_changed = false;
        for (tag, c) in &mut self.countries {
            let before = c.truck_count();
            let hubs = self
                .motorized_hubs
                .iter()
                .filter(|&&p| world.controller_of_province(p) == Some(tag.as_str()))
                .count() as u64;
            let required = hubs.saturating_mul(self.rules.trucks_per_hub);
            let mut need = required.saturating_sub(c.truck_count());
            let stock = &mut production.country_mut(tag).model_stockpile;
            for (model, n) in stock.iter_mut() {
                if catalogue.equipment.get(model).is_some_and(|e| {
                    e.archetype == "motorized_equipment" || e.id == "motorized_equipment"
                }) {
                    let take = need.min(*n);
                    *n -= take;
                    *c.truck_models.entry(model.clone()).or_default() += take;
                    need -= take;
                }
            }
            let mut excess = c
                .truck_count()
                .saturating_sub(required)
                .min(c.truck_models.values().sum());
            for (model, n) in &mut c.truck_models {
                let returned = excess.min(*n);
                *n -= returned;
                excess -= returned;
                *stock.entry(model.clone()).or_default() += returned;
            }
            reservation_changed |= c.truck_count() != before;
        }
        if reservation_changed {
            // Newly allocated trucks change the effective motorization range.
            self.network_key = None;
        }
    }
    pub fn validate(&self, world: &World) -> Result<(), String> {
        if self.dispatch_schema > 2 {
            return Err("Unsupported logistics dispatch version".into());
        }
        let r = &self.rules;
        let values = [
            r.capital_capacity,
            r.rail_capacity_per_level,
            r.hub_capacity,
            r.port_capacity_per_level,
            r.supply_per_convoy,
            r.local_supply_per_infrastructure,
            r.supply_reserve_hours,
            r.fuel_reserve_hours,
            r.idle_fuel_factor,
            r.minimum_supply_speed,
            r.minimum_fuel_speed,
            r.attrition_loss_factor,
            r.supply_attrition,
            r.exercise_attrition,
            r.refinery_fuel_per_day,
            r.oil_fuel_per_day,
            r.base_fuel_capacity,
            r.silo_fuel_capacity,
            r.fuel_transport_weight,
        ];
        if values.iter().any(|v| !v.is_finite() || *v < 0.)
            || r.fuel_transport_weight == 0.
            || r.supply_reserve_hours == 0.
            || r.fuel_reserve_hours == 0.
            || r.minimum_supply_speed > 1.
            || r.minimum_fuel_speed > 1.
            || r.network_interval == 0
            || r.last_mile_hops > 32
            || r.motorized_extra_hops > 32
        {
            return Err("Invalid logistics rules".into());
        }
        for (tag, c) in &self.countries {
            if !world.countries.contains_key(tag)
                || c.capital
                    .is_some_and(|id| !world.provinces.contains_key(&id))
                || [c.fuel, c.fuel_capacity, c.fuel_income_daily]
                    .iter()
                    .any(|n| !n.is_finite() || *n < 0.)
                || c.fuel_capacity_base
                    .is_some_and(|n| !n.is_finite() || n < 0.)
                || c.fuel > c.fuel_capacity
            {
                return Err(format!("Invalid logistics country {tag}"));
            }
        }
        for e in &self.rails {
            if e.level == 0
                || e.level > 5
                || e.from == e.to
                || !world
                    .geography
                    .connection(e.from, e.to)
                    .is_some_and(|x| x.kind == CrossingKind::Border)
            {
                return Err(format!("Invalid rail {}–{}", e.from, e.to));
            }
        }
        if self
            .oil_by_state
            .iter()
            .any(|(id, n)| !world.states.contains_key(id) || !n.is_finite() || *n < 0.)
            || self
                .motorized_hubs
                .iter()
                .any(|p| !world.provinces.contains_key(p))
            || self
                .blocked_sea
                .iter()
                .any(|p| !world.geography.provinces.contains_key(p))
        {
            return Err("Invalid logistics geography".into());
        }
        if self.delivery.values().any(|n| !n.is_finite() || *n < 0.)
            || self.serviced.iter().any(|(tag, p)| {
                !world.countries.contains_key(tag) || !world.provinces.contains_key(p)
            })
        {
            return Err("Invalid dispatch state".into());
        }
        for s in self.divisions.values() {
            if [s.supply, s.fuel, s.received, s.demand]
                .iter()
                .any(|x| !x.is_finite() || *x < 0.)
            {
                return Err("Invalid division stocks".into());
            }
        }
        Ok(())
    }
    pub fn can_deliver(&self, country: &str, province: u32) -> bool {
        !self.enabled || self.serviced.contains(&(country.to_owned(), province))
    }
    /// Refresh reachability without consuming resources or advancing time.
    pub fn refresh_network(&mut self, w: &Warfare, world: &World) {
        self.rebuild(w, world);
        self.dispatch_schema = 2;
        self.network_key = None;
    }
    fn dispatch_parts(&self, d: &crate::military_model::Division) -> (f64, f64) {
        let demand = supply_demand(d);
        let supply_cap = demand / 24. * self.rules.supply_reserve_hours;
        let fuel_cap = d.derived.stats.fuel.max(0.) / 24. * self.rules.fuel_reserve_hours;
        let state = self.divisions.get(&d.id);
        let supply = state.map_or(supply_cap * d.supply_fraction.clamp(0., 1.), |s| s.supply);
        let fuel = state.map_or(fuel_cap * d.fuel_fraction.clamp(0., 1.), |s| s.fuel);
        let active = d.battle.is_some()
            || d.destination.is_some()
            || matches!(d.order, DivisionOrder::Train { .. });
        let burn = d.derived.stats.fuel.max(0.)
            * if active {
                1.
            } else {
                self.rules.idle_fuel_factor
            };
        (
            demand + (supply_cap - supply).max(0.) * 24. / self.rules.supply_reserve_hours.max(1.),
            if self
                .countries
                .get(&d.country)
                .is_some_and(|c| c.fuel > 1e-9)
            {
                burn + (fuel_cap - fuel).max(0.) * 24. / self.rules.fuel_reserve_hours.max(1.)
            } else {
                0.
            },
        )
    }
    fn dispatch_demand(&self, d: &crate::military_model::Division) -> f64 {
        let (supply, fuel) = self.dispatch_parts(d);
        supply + fuel * self.rules.fuel_transport_weight
            + self.reinforcement_requests.get(&d.id).copied().unwrap_or(0.)
    }
    /// Reserve throughput only for equipment or personnel that can really be dispatched.
    /// This is separate from the daily stockpile transaction: no goods are created here.
    pub fn prepare_reinforcements(
        &mut self,
        w: &Warfare,
        production: &crate::production::ProductionState,
    ) {
        if !self.enabled {
            return;
        }
        let mut requests = BTreeMap::new();
        for d in w.divisions.values().filter(|d| !d.destroyed && d.battle.is_none()) {
            let mut stock_by_kind = BTreeMap::<String, u64>::new();
            if let Some(stock) = production.country(&d.country) {
                for (id, &quantity) in &stock.model_stockpile {
                    if let Some(model) = w.catalogue.equipment.get(id) {
                        *stock_by_kind.entry(model.archetype.clone()).or_default() += quantity;
                    }
                }
            }
            let mut equipment = 0_u64;
            for (kind, &needed) in &d.derived.equipment {
                let carried: u64 = d.equipment.iter().filter_map(|(id, &n)| {
                    w.catalogue.equipment.get(id)
                        .filter(|model| model.archetype == *kind)
                        .map(|_| u64::from(n))
                }).sum();
                let deficit = (needed.ceil().max(0.) as u64).saturating_sub(carried);
                equipment = equipment.saturating_add(
                    deficit.min(stock_by_kind.get(kind).copied().unwrap_or(0))
                );
            }
            let people_needed = (d.derived.stats.manpower.ceil().max(0.) as u64)
                .saturating_sub(u64::from(d.manpower));
            let available_people = w.countries.get(&d.country)
                .map_or(0, |country| country.manpower_pool);
            let cost = equipment as f64 * self.rules.equipment_transport_weight.max(0.)
                + people_needed.min(available_people) as f64
                    * self.rules.manpower_transport_weight.max(0.);
            let demand = cost.min(self.rules.max_reinforcement_daily.max(0.));
            if demand.is_finite() && demand > 0. {
                requests.insert(d.id, demand);
            }
        }
        if self.reinforcement_requests != requests {
            self.reinforcement_requests = requests;
            self.network_key = None;
        }
    }

    fn rebuild(&mut self, w: &Warfare, world: &World) {
        self.delivery.clear();
        self.serviced.clear();
        for (tag, country) in &self.countries {
            let Some(capital) = country.capital.filter(|&p| controlled(w, world, tag, p)) else {
                continue;
            };
            let units: Vec<_> = w
                .divisions
                .values()
                .filter(|d| !d.destroyed && d.country == *tag)
                .collect();
            let mut flow = Flow::new(2);
            let (source, sink) = (0, 1);
            let mut nodes = BTreeMap::new();
            // Only actual rail endpoints and hubs need transport nodes.
            nodes.insert(capital, flow.node());
            for (&p, c) in &w.conditions {
                if (c.supply_node || c.naval_base > 0) && controlled(w, world, tag, p) {
                    nodes.entry(p).or_insert_with(|| flow.node());
                }
            }
            for e in &self.rails {
                if controlled(w, world, tag, e.from) && controlled(w, world, tag, e.to) {
                    for p in [e.from, e.to] {
                        nodes.entry(p).or_insert_with(|| flow.node());
                    }
                    let cap = self.rules.rail_capacity_per_level * f64::from(e.level);
                    flow.add(nodes[&e.from], nodes[&e.to], cap);
                    flow.add(nodes[&e.to], nodes[&e.from], cap);
                }
            }
            flow.add(source, nodes[&capital], self.rules.capital_capacity);
            // Ports connect only through physically connected, unblocked sea provinces.
            let ports: Vec<_> = nodes
                .keys()
                .copied()
                .filter(|p| w.conditions.get(p).is_some_and(|c| c.naval_base > 0))
                .collect();
            let mut sea_component = BTreeMap::new();
            let mut component = 0usize;
            for &port in &ports {
                for e in world.geography.connections_from(port) {
                    if e.kind == CrossingKind::Impassable
                        || sea_component.contains_key(&e.to)
                        || self.blocked_sea.contains(&e.to)
                        || !world
                            .geography
                            .provinces
                            .get(&e.to)
                            .is_some_and(|p| p.definition.kind == ProvinceKind::Sea)
                    {
                        continue;
                    }
                    component += 1;
                    let mut q = VecDeque::from([e.to]);
                    sea_component.insert(e.to, component);
                    while let Some(p) = q.pop_front() {
                        for edge in world.geography.connections_from(p) {
                            if edge.kind != CrossingKind::Impassable
                                && !self.blocked_sea.contains(&edge.to)
                                && !sea_component.contains_key(&edge.to)
                                && world
                                    .geography
                                    .provinces
                                    .get(&edge.to)
                                    .is_some_and(|p| p.definition.kind == ProvinceKind::Sea)
                            {
                                sea_component.insert(edge.to, component);
                                q.push_back(edge.to);
                            }
                        }
                    }
                }
            }
            // One shared convoy pool, partitioned across sea basins (no cross-basin teleportation).
            let mut basins = BTreeMap::<usize, Vec<u32>>::new();
            for p in ports {
                let cs: BTreeSet<_> = world
                    .geography
                    .connections_from(p)
                    .iter()
                    .filter_map(|e| sea_component.get(&e.to).copied())
                    .collect();
                for c in cs {
                    basins.entry(c).or_default().push(p);
                }
            }
            let count = basins.len().max(1) as f64;
            for ps in basins.values() {
                let a = flow.node();
                let b = flow.node();
                flow.add(
                    a,
                    b,
                    country
                        .convoys
                        .saturating_sub(country.trade_convoys_committed) as f64
                        * self.rules.supply_per_convoy
                        / count,
                );
                for &p in ps {
                    let cap =
                        f64::from(w.conditions[&p].naval_base) * self.rules.port_capacity_per_level;
                    flow.add(nodes[&p], a, cap);
                    flow.add(b, nodes[&p], cap);
                }
            }
            let mut sinks = Vec::new();
            for d in &units {
                let n = flow.node();
                let demand = self.dispatch_demand(d);
                let edge = flow.add(n, sink, demand);
                sinks.push((d.id, n, edge, demand));
            }
            let mut truck_budget = country.truck_count();
            let mut hub_ranges = BTreeMap::new();
            for (&p, &n) in &nodes {
                let c = w.conditions.get(&p);
                if p != capital && !c.is_some_and(|c| c.supply_node || c.naval_base > 0) {
                    continue;
                }
                let motor =
                    self.motorized_hubs.contains(&p) && truck_budget >= self.rules.trucks_per_hub;
                if motor {
                    truck_budget -= self.rules.trucks_per_hub;
                }
                let range = self.rules.last_mile_hops
                    + if motor {
                        self.rules.motorized_extra_hops
                    } else {
                        0
                    };
                hub_ranges.insert(p, range);
                let reach = reachable(w, world, tag, p, range);
                let hub = flow.node();
                flow.add(n, hub, self.rules.hub_capacity);
                for (i, d) in units.iter().enumerate() {
                    if let Some(&distance) = reach.get(&d.province) {
                        let infra = world
                            .provinces
                            .get(&d.province)
                            .and_then(|p| p.state_id)
                            .and_then(|s| world.states.get(&s))
                            .map_or(0, |s| s.economy.infrastructure);
                        let efficiency =
                            (1. + f64::from(infra) * 0.1) / (1. + f64::from(distance) * 0.25);
                        flow.add(
                            hub,
                            sinks[i].1,
                            self.dispatch_demand(d) * efficiency.min(1.),
                        );
                    }
                }
            }
            let mut seen = BTreeSet::from([source]);
            let mut q = VecDeque::from([source]);
            while let Some(n) = q.pop_front() {
                for &(to, _, cap) in &flow.edges[n] {
                    if cap > 1e-8 && seen.insert(to) {
                        q.push_back(to);
                    }
                }
            }
            for (&p, &n) in &nodes {
                if seen.contains(&n)
                    && (p == capital
                        || w.conditions
                            .get(&p)
                            .is_some_and(|c| c.supply_node || c.naval_base > 0))
                {
                    for province in reachable(
                        w,
                        world,
                        tag,
                        p,
                        hub_ranges
                            .get(&p)
                            .copied()
                            .unwrap_or(self.rules.last_mile_hops),
                    )
                    .keys()
                    {
                        self.serviced.insert((tag.clone(), *province));
                    }
                }
            }
            if sinks.is_empty() {
                continue;
            }
            // Max-min proportional floor: find the largest shared satisfaction
            // fraction feasible for every reachable unit, then use leftover capacity.
            // This prevents low IDs from consuming all of a narrow rail bottleneck.
            let baseline = flow.clone();
            flow.solve(source, sink);
            let maximum = sinks
                .iter()
                .map(|&(_, n, e, d)| d - flow.edges[n][e].2)
                .sum::<f64>();
            let reachable_demand = sinks
                .iter()
                .filter(|(_, n, _, _)| seen.contains(n))
                .map(|s| s.3)
                .sum::<f64>();
            let mut high = (maximum / reachable_demand.max(1e-9)).min(1.);
            let mut low = 0.;
            let mut best = baseline.clone();
            for &(_, n, e, _) in &sinks {
                best.edges[n][e].2 = 0.;
            }
            for _ in 0..12 {
                let ratio = (low + high) * 0.5;
                let mut trial = baseline.clone();
                let mut required = 0.;
                for &(_, n, e, d) in &sinks {
                    let cap = if seen.contains(&n) { d * ratio } else { 0. };
                    trial.edges[n][e].2 = cap;
                    required += cap;
                }
                trial.solve(source, sink);
                let remainder = sinks
                    .iter()
                    .map(|&(_, n, e, _)| trial.edges[n][e].2)
                    .sum::<f64>();
                if remainder <= required.max(1.) * 1e-8 {
                    low = ratio;
                    best = trial;
                } else {
                    high = ratio;
                }
            }
            flow = best;
            for &(_, n, e, d) in &sinks {
                flow.edges[n][e].2 += d - if seen.contains(&n) { d * low } else { 0. };
            }
            flow.solve(source, sink);
            for (id, n, e, demand) in sinks {
                self.delivery
                    .insert(id, (demand - flow.edges[n][e].2).clamp(0., demand));
            }
        }
    }
    pub fn tick(&mut self, w: &mut Warfare, world: &World, hour: u64) {
        if !self.enabled || self.last_hour == Some(hour) {
            return;
        }
        self.last_hour = Some(hour);
        if self.dispatch_schema < 2 {
            self.network_key = None;
            self.dispatch_schema = 2;
            if self.rules.profile == "mobile-logistics-v1-approximation" {
                self.rules.profile = "mobile-logistics-v2-approximation".into();
            }
        }
        let key = (
            world.control_revision,
            hour / self.rules.network_interval.max(1),
        );
        if self.network_key != Some(key) {
            self.rebuild(w, world);
            self.network_key = Some(key);
        }
        self.divisions
            .retain(|id, _| w.divisions.get(id).is_some_and(|d| !d.destroyed));
        self.reinforcement_credit
            .retain(|id, _| w.divisions.get(id).is_some_and(|d| !d.destroyed));
        for (tag, c) in &mut self.countries {
            let refineries = world
                .country_economy(tag)
                .map_or(0, |e| e.synthetic_refineries);
            let oil = c.oil_supply_daily.unwrap_or_else(|| {
                self.oil_by_state
                    .iter()
                    .filter(|(id, _)| world.state_controller(**id) == Some(tag.as_str()))
                    .map(|(_, n)| *n)
                    .sum::<f64>()
            });
            let silos = world.country_economy(tag).map_or(0, |e| e.fuel_silos);
            // Remember the fixed capacity once, rather than treating yesterday's
            // computed silo capacity as another permanent national bonus.
            let base = *c
                .fuel_capacity_base
                .get_or_insert_with(|| c.fuel_capacity.max(self.rules.base_fuel_capacity));
            c.fuel_capacity = base + f64::from(silos) * self.rules.silo_fuel_capacity;
            // Capturing or losing a silo cannot leave fuel above the new limit.
            c.fuel = c.fuel.min(c.fuel_capacity);
            let income = (c.fuel_income_daily
                + oil * self.rules.oil_fuel_per_day
                + f64::from(refineries) * self.rules.refinery_fuel_per_day)
                / 24.;
            c.fuel = (c.fuel + income).min(c.fuel_capacity);
            c.fuel_used_last_hour = 0.;
        }
        // Fuel rationing is proportional across all divisions, independent of IDs.
        let mut fuel_requests = BTreeMap::<String, f64>::new();
        for d in w.divisions.values().filter(|d| !d.destroyed) {
            let active = d.battle.is_some()
                || d.destination.is_some()
                || matches!(d.order, DivisionOrder::Train { .. });
            let consumption = d.derived.stats.fuel.max(0.) / 24.
                * if active {
                    1.
                } else {
                    self.rules.idle_fuel_factor
                };
            let state = self
                .divisions
                .entry(d.id)
                .or_insert_with(|| DivisionLogistics {
                    supply: supply_demand(d) / 24.
                        * self.rules.supply_reserve_hours
                        * d.supply_fraction.clamp(0., 1.),
                    fuel: d.derived.stats.fuel.max(0.) / 24.
                        * self.rules.fuel_reserve_hours
                        * d.fuel_fraction.clamp(0., 1.),
                    ..Default::default()
                });
            let carried_fuel = state.fuel;
            let cap = d.derived.stats.fuel.max(0.) / 24. * self.rules.fuel_reserve_hours;
            if self.serviced.contains(&(d.country.clone(), d.province)) {
                let (supply, fuel) = self.dispatch_parts(d);
                let fraction = (self.delivery.get(&d.id).copied().unwrap_or(0.)
                    / (supply + fuel * self.rules.fuel_transport_weight).max(1e-9))
                .clamp(0., 1.);
                *fuel_requests.entry(d.country.clone()).or_default() += (cap - carried_fuel
                    + consumption)
                    .max(0.)
                    .min(fuel / 24. * fraction);
            }
        }
        let shares: BTreeMap<_, _> = fuel_requests
            .iter()
            .map(|(tag, &demand)| {
                (
                    tag.clone(),
                    self.countries
                        .get(tag)
                        .map_or(0., |c| (c.fuel / demand.max(1e-9)).min(1.)),
                )
            })
            .collect();
        self.local_counts.clear();
        for d in w.divisions.values().filter(|d| !d.destroyed) {
            *self.local_counts.entry(d.province).or_default() += 1;
        }
        let mut dirty = Vec::new();
        for d in w.divisions.values_mut().filter(|d| !d.destroyed) {
            let (supply_request, fuel_request) = self.dispatch_parts(d);
            let cargo_request = self.reinforcement_requests.get(&d.id).copied().unwrap_or(0.);
            let total_request = supply_request + fuel_request * self.rules.fuel_transport_weight
                + cargo_request;
            let delivered = if self.serviced.contains(&(d.country.clone(), d.province)) {
                self.delivery
                    .get(&d.id)
                    .copied()
                    .unwrap_or(0.)
                    .min(total_request)
            } else {
                0.
            };
            let transport_fraction = (delivered / total_request.max(1e-9)).clamp(0., 1.);
            // Allocate each hour of the solved shared flow among supplies, fuel and cargo.
            // The credit is a physical delivery budget, not free national equipment.
            let cargo_per_hour = cargo_request * transport_fraction / 24.;
            let limit = self.rules.max_reinforcement_daily.max(0.) * 2.;
            let credit = self.reinforcement_credit.entry(d.id).or_default();
            *credit = (*credit + cargo_per_hour).min(limit);
            let s = self.divisions.get_mut(&d.id).unwrap();
            s.transport_received = delivered;
            s.fuel_received = 0.;
            let demand = supply_demand(d);
            s.demand = demand;
            let local = world
                .provinces
                .get(&d.province)
                .and_then(|p| p.state_id)
                .and_then(|id| world.states.get(&id))
                .map_or(0., |state| {
                    let n = state.province_ids.len().max(1) as f64;
                    if world.controller_of_province(d.province) == Some(d.country.as_str()) {
                        f64::from(state.economy.infrastructure)
                            * self.rules.local_supply_per_infrastructure
                            / n
                    } else {
                        0.
                    }
                });
            // Local output shared by co-located divisions, not replicated per unit.
            let here = self.local_counts.get(&d.province).copied().unwrap_or(1) as f64;
            let received = supply_request * transport_fraction + local / here;
            // Include delivery into reserves, not just this hour's consumption.
            s.received = received.min(demand * 2.);
            let required = demand / 24.;
            let cap = required * self.rules.supply_reserve_hours;
            s.supply = (s.supply + s.received / 24. - required).clamp(0., cap);
            d.supply_fraction = if demand > 0. {
                (s.received / demand + s.supply / cap.max(1e-9)).min(1.)
            } else {
                1.
            };
            s.reason = if s.received + 1e-8 >= demand {
                "доставка достаточна"
            } else if self.serviced.contains(&(d.country.clone(), d.province)) {
                "пропускная способность / дальность хаба"
            } else {
                "нет пути к столице / порту"
            }
            .into();
            let active = d.battle.is_some()
                || d.destination.is_some()
                || matches!(d.order, DivisionOrder::Train { .. });
            let use_fuel = d.derived.stats.fuel.max(0.) / 24.
                * if active {
                    1.
                } else {
                    self.rules.idle_fuel_factor
                };
            let fuel_cap = d.derived.stats.fuel.max(0.) / 24. * self.rules.fuel_reserve_hours;
            if self.serviced.contains(&(d.country.clone(), d.province)) {
                let take = (fuel_cap - s.fuel + use_fuel)
                    .max(0.)
                    .min(fuel_request / 24. * transport_fraction)
                    * shares.get(&d.country).copied().unwrap_or(0.);
                if let Some(c) = self.countries.get_mut(&d.country) {
                    let take = take.min(c.fuel);
                    c.fuel -= take;
                    c.fuel_used_last_hour += take;
                    s.fuel += take;
                    s.fuel_received = take;
                }
            }
            let consumed = use_fuel.min(s.fuel);
            s.fuel = (s.fuel - consumed).min(fuel_cap);
            d.fuel_fraction = if use_fuel > 0. {
                consumed / use_fuel
            } else {
                1.
            };
            let terrain = world
                .geography
                .provinces
                .get(&d.province)
                .map(|p| w.catalogue.default_terrain(&p.definition.terrain).attrition)
                .unwrap_or(0.);
            let weather = w
                .conditions
                .get(&d.province)
                .and_then(|c| c.weather.as_ref())
                .and_then(|id| w.catalogue.weather.get(id))
                .and_then(|b| b.get("attrition"))
                .copied()
                .unwrap_or(0.);
            let attrition = (terrain
                + weather
                + (1. - d.supply_fraction) * self.rules.supply_attrition
                + if matches!(d.order, DivisionOrder::Train { .. }) {
                    self.rules.exercise_attrition
                } else {
                    0.
                })
            .max(0.);
            let mut lost = 0u64;
            for (model, n) in &mut d.equipment {
                let reliability = w
                    .countries
                    .get(&d.country)
                    .map(|a| {
                        w.catalogue
                            .effective_equipment_stat(model, "reliability", &a.modifiers)
                    })
                    .unwrap_or_else(|| {
                        w.catalogue
                            .equipment
                            .get(model)
                            .and_then(|e| e.stats.get("reliability"))
                            .copied()
                            .unwrap_or(d.derived.stats.reliability)
                    })
                    .clamp(0., 1.);
                let expected = f64::from(*n)
                    * attrition
                    * (1. - reliability)
                    * self.rules.attrition_loss_factor
                    / 24.;
                let count = w.rng.stochastic_round(expected).min(*n);
                *n -= count;
                lost += u64::from(count);
            }
            s.equipment_lost += lost;
            if lost > 0 {
                dirty.push(d.id);
            }
        }
        // Attrition never directly kills manpower. Actual surviving models recalculate attack etc.
        for id in dirty {
            let d = &mut w.divisions.get_mut(&id).unwrap();
            if let Some(a) = w.countries.get(&d.country) {
                if let Some(t) = a.templates.get(&d.template) {
                    if let Ok(stats) = w.catalogue.aggregate(t, &a.modifiers, Some(&d.equipment)) {
                        d.derived = stats;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod integration_regressions {
    use super::*;
    use crate::military_model::EquipmentDefinition;
    use crate::production::ProductionState;
    use crate::state::StateEconomy;
    use std::collections::HashMap;

    #[test]
    fn motorization_reserves_only_real_trucks_and_returns_them_when_disabled() {
        let world = crate::warfare::tests::world();
        let mut logistics = LogisticsState::default();
        logistics
            .countries
            .insert("A".into(), CountryLogistics::default());
        logistics.motorized_hubs.insert(1);
        logistics.network_key = Some((1, 0));
        let mut production = ProductionState::default();
        production
            .country_mut("A")
            .model_stockpile
            .insert("truck_test".into(), 55);
        let mut db = crate::military_model::MilitaryCatalogue::default();
        db.equipment.insert(
            "truck_test".into(),
            EquipmentDefinition {
                id: "truck_test".into(),
                archetype: "motorized_equipment".into(),
                year: 1936,
                active: true,
                buildable: true,
                stats: Default::default(),
                cost: 2.,
                modular: false,
                source: "test".into(),
            },
        );
        logistics.reserve_trucks(&mut production, &world, &db);
        assert_eq!(logistics.network_key, None);
        assert_eq!(
            production.country("A").unwrap().model_stockpile["truck_test"],
            15
        );
        assert_eq!(logistics.countries["A"].truck_models["truck_test"], 40);
        // Repeated calls must never consume another forty trucks for the same hub.
        logistics.reserve_trucks(&mut production, &world, &db);
        assert_eq!(
            production.country("A").unwrap().model_stockpile["truck_test"],
            15
        );
        logistics.motorized_hubs.clear();
        logistics.reserve_trucks(&mut production, &world, &db);
        assert_eq!(
            production.country("A").unwrap().model_stockpile["truck_test"],
            55
        );
        assert_eq!(logistics.countries["A"].truck_count(), 0);
    }

    #[test]
    fn fuel_silo_capacity_shrinks_after_capture_without_becoming_permanent() {
        let mut world = crate::warfare::tests::world();
        world.states.get_mut(&1).unwrap().economy = StateEconomy {
            fuel_silos: 1,
            ..Default::default()
        };
        let mut logistics = LogisticsState::default();
        logistics.enabled = true;
        logistics.countries.insert(
            "A".into(),
            CountryLogistics {
                capital: Some(1),
                fuel: 75.,
                fuel_capacity: 100.,
                ..Default::default()
            },
        );
        logistics.rules.base_fuel_capacity = 100.;
        logistics.rules.silo_fuel_capacity = 100.;
        let mut warfare = Warfare::default();
        // Avoid unrelated network/import dependencies: no divisions are required.
        logistics.tick(&mut warfare, &world, 1);
        assert_eq!(logistics.countries["A"].fuel_capacity, 200.);
        assert_eq!(logistics.countries["A"].fuel_capacity_base, Some(100.));
        world.set_province_controller(1, Some("B"));
        logistics.tick(&mut warfare, &world, 2);
        assert_eq!(logistics.countries["A"].fuel_capacity, 100.);
        logistics.countries.get_mut("A").unwrap().fuel = 150.;
        logistics.tick(&mut warfare, &world, 3);
        assert_eq!(logistics.countries["A"].fuel, 100.);
        world.set_province_controller(1, Some("A"));
        logistics.tick(&mut warfare, &world, 4);
        assert_eq!(logistics.countries["A"].fuel_capacity, 200.);
    }
}
