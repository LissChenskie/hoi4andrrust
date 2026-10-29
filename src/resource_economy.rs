//! Daily HOI4-style strategic-resource allocation and civilian-factory trade.
//! Resources are a *flow*, not a stockpile. All quantities are integer units/day.
//! No imaginary default national laws, routes or starting trade deals are added.
use crate::{
    construction::{self, CountryConstructionPolicy},
    trade_routes::{self, TradeRoute},
    warfare::Warfare,
    world::World,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeLaw {
    #[default]
    Unset,
    FreeTrade,
    ExportFocus,
    LimitedExports,
    ClosedEconomy,
}
impl TradeLaw {
    pub fn from_hoi4_idea(id: &str) -> Option<Self> {
        match id {
            "free_trade" => Some(Self::FreeTrade),
            "export_focus" => Some(Self::ExportFocus),
            "limited_exports" => Some(Self::LimitedExports),
            "closed_economy" => Some(Self::ClosedEconomy),
            _ => None,
        }
    }
    pub fn export_percentage(self) -> u32 {
        match self {
            Self::FreeTrade => 80,
            Self::ExportFocus => 50,
            Self::LimitedExports => 25,
            Self::ClosedEconomy | Self::Unset => 0,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeDeal {
    pub buyer: String,
    pub seller: String,
    pub resource: String,
    /// Each committed civilian factory buys up to 8 units/day.
    pub factories: u32,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CountryResources {
    pub trade_convoys_used: u64,
    pub extracted: BTreeMap<String, u32>,
    pub export_reserved: BTreeMap<String, u32>,
    pub imported: BTreeMap<String, u32>,
    pub exported: BTreeMap<String, u32>,
    pub available: BTreeMap<String, u32>,
    pub factories_spent: u32,
    pub factories_received: u32,
}
/// Observed contract status from the actual route and convoy allocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeShipment {
    pub buyer: String,
    pub seller: String,
    pub resource: String,
    pub factories: u32,
    pub contracted: u32,
    pub delivered: u32,
    pub required_convoys: u64,
    pub assigned_convoys: u64,
    pub route: TradeRoute,
}

/// Provisional convoy capacity coefficients; route geometry is authoritative.
/// A future game-version verification must replace these estimates.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TradeConvoyRules {
    /// Resource units carried per convoy on a short water route.
    pub resource_units_per_convoy: u32,
    /// Sea-zone steps per further multiplier of the hull requirement.
    pub sea_provinces_per_distance_step: u32,
}
impl Default for TradeConvoyRules {
    fn default() -> Self {
        Self {
            resource_units_per_convoy: 8,
            sea_provinces_per_distance_step: 8,
        }
    }
}
impl TradeConvoyRules {
    pub fn required(self, resources: u32, sea_steps: usize) -> u64 {
        if resources == 0 || sea_steps == 0 {
            return 0;
        }
        let units = u64::from(self.resource_units_per_convoy.max(1));
        let distance = u64::from(self.sea_provinces_per_distance_step.max(1));
        let distance_factor = (sea_steps as u64).div_ceil(distance).max(1);
        u64::from(resources)
            .div_ceil(units)
            .saturating_mul(distance_factor)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ResourceEconomy {
    /// Last settlement includes individual routes and missed cargo.
    pub last_shipments: Vec<TradeShipment>,
    pub convoy_rules: TradeConvoyRules,
    pub laws: BTreeMap<String, TradeLaw>,
    pub deals: Vec<TradeDeal>,
    /// Opt-in per country; manually placed agreements are never altered by automation.
    pub automatic_buyers: std::collections::BTreeSet<String>,
    pub automatic_deals: Vec<TradeDeal>,
    #[serde(default)]
    pub last: BTreeMap<String, CountryResources>,
}
impl ResourceEconomy {
    /// Recompute the cheapest viable *approximate* factory-backed supply plan
    /// against real equipment-line material demands. We do not invent resources,
    /// routes, free factories or importers. Manual deals remain higher priority.
    pub fn plan_automatic_trade(
        &mut self,
        world: &World,
        warfare: &Warfare,
        production: &crate::production::ProductionState,
        policies: &BTreeMap<String, CountryConstructionPolicy>,
        research: &crate::research::ResearchWorld,
        rules: &crate::production::ProductionRules,
    ) {
        self.automatic_deals.clear();
        if self.automatic_buyers.is_empty() {
            return;
        }
        let mut deposits = BTreeMap::<String, BTreeMap<String, u32>>::new();
        for (&state_id, resources) in &world.resources_by_state {
            let Some(tag) = world.state_controller(state_id) else {
                continue;
            };
            let infra = world
                .states
                .get(&state_id)
                .map_or(0, |s| s.economy.infrastructure);
            let gain = research
                .country(tag)
                .map_or(0., |c| c.modifier("resource_gain_efficiency"));
            for (resource, &amount) in resources {
                let quantity =
                    (f64::from(amount) * (1. + f64::from(infra) * 0.2) * (1. + gain).max(0.))
                        .floor()
                        .clamp(0., u32::MAX as f64) as u32;
                let entry = deposits
                    .entry(tag.into())
                    .or_default()
                    .entry(resource.clone())
                    .or_default();
                *entry = entry.saturating_add(quantity);
            }
        }
        let mut exports = BTreeMap::<(String, String), u32>::new();
        for (tag, resources) in &deposits {
            let percent = self
                .laws
                .get(tag)
                .copied()
                .unwrap_or_default()
                .export_percentage();
            for (resource, &amount) in resources {
                exports.insert(
                    (tag.clone(), resource.clone()),
                    amount.saturating_mul(percent) / 100,
                );
            }
        }
        for deal in &self.deals {
            if let Some(q) = exports.get_mut(&(deal.seller.clone(), deal.resource.clone())) {
                *q = q.saturating_sub(deal.factories.saturating_mul(8));
            }
        }
        let mut route_cache = BTreeMap::<(String, String), bool>::new();
        for buyer in &self.automatic_buyers {
            if !world.countries.contains_key(buyer) {
                continue;
            }
            let Some(country) = production.country(buyer) else {
                continue;
            };
            let Some(economy) = world.country_economy(buyer) else {
                continue;
            };
            let mut requirements = BTreeMap::<String, u32>::new();
            let allocations = country.allocations(economy.military_factories, rules);
            for (line, allocation) in country.lines.iter().zip(allocations) {
                if allocation.assigned == 0 || line.blocked_reason.is_some() {
                    continue;
                }
                if let Some(need) = line
                    .model
                    .as_ref()
                    .and_then(|model| warfare.catalogue.equipment_resources.get(model))
                {
                    for (resource, &quantity) in need {
                        let entry = requirements.entry(resource.clone()).or_default();
                        *entry = entry.saturating_add(quantity.saturating_mul(allocation.assigned));
                    }
                }
            }
            // Oil fuels the operational fleet and divisions even when it is
            // not a direct factory ingredient. Do not create synthetic fuel.
            if warfare
                .logistics
                .countries
                .get(buyer)
                .is_some_and(|c| c.fuel_capacity > 0. && c.fuel < c.fuel_capacity * 0.25)
            {
                requirements
                    .entry("oil".into())
                    .and_modify(|n| *n = (*n).max(8))
                    .or_insert(8);
            }
            for (resource, need) in requirements {
                let own = deposits
                    .get(buyer)
                    .and_then(|r| r.get(&resource))
                    .copied()
                    .unwrap_or(0);
                let local_export = self
                    .laws
                    .get(buyer)
                    .copied()
                    .unwrap_or_default()
                    .export_percentage();
                let own_available = own.saturating_sub(own.saturating_mul(local_export) / 100);
                let manual = self
                    .deals
                    .iter()
                    .filter(|d| d.buyer == *buyer && d.resource == resource)
                    .map(|d| d.factories.saturating_mul(8))
                    .sum::<u32>();
                let mut short = need.saturating_sub(own_available).saturating_sub(manual);
                if short == 0 {
                    continue;
                }
                let spent_manual = self
                    .deals
                    .iter()
                    .filter(|d| d.buyer == *buyer)
                    .map(|d| d.factories)
                    .sum::<u32>();
                let planned = self
                    .automatic_deals
                    .iter()
                    .filter(|d| d.buyer == *buyer)
                    .map(|d| d.factories)
                    .sum::<u32>();
                let free = crate::construction::country_capacity(
                    world,
                    buyer,
                    policies.get(buyer).copied().unwrap_or_default(),
                )
                .map_or(0, |c| c.available)
                .saturating_sub(spent_manual)
                .saturating_sub(planned);
                if free == 0 {
                    continue;
                }
                let mut offers = exports
                    .iter()
                    .filter(|((tag, r), stock)| {
                        tag != buyer && r == &resource && **stock > 0 && !warfare.at_war(buyer, tag)
                    })
                    .map(|((tag, _), stock)| (tag.clone(), *stock))
                    .collect::<Vec<_>>();
                offers.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                let mut remaining_factories = free;
                // At most four source routes per material: predictable mobile cost.
                for (seller, _) in offers.into_iter().take(12) {
                    if short == 0 || remaining_factories == 0 {
                        break;
                    }
                    let route_ok = *route_cache
                        .entry((buyer.clone(), seller.clone()))
                        .or_insert_with(|| {
                            !matches!(
                                trade_routes::find_route(
                                    world,
                                    warfare,
                                    buyer,
                                    &seller,
                                    &warfare.logistics.blocked_sea
                                ),
                                TradeRoute::Unavailable { .. }
                            )
                        });
                    if !route_ok {
                        continue;
                    }
                    let available = *exports
                        .get(&(seller.clone(), resource.clone()))
                        .unwrap_or(&0);
                    let units = short
                        .min(available)
                        .min(remaining_factories.saturating_mul(8));
                    if units == 0 {
                        continue;
                    }
                    let factories = units.div_ceil(8);
                    self.automatic_deals.push(TradeDeal {
                        buyer: buyer.clone(),
                        seller: seller.clone(),
                        resource: resource.clone(),
                        factories,
                    });
                    if let Some(n) = exports.get_mut(&(seller, resource.clone())) {
                        *n = n.saturating_sub(units);
                    }
                    short = short.saturating_sub(units);
                    remaining_factories = remaining_factories.saturating_sub(factories);
                    if self
                        .automatic_deals
                        .iter()
                        .filter(|d| d.buyer == *buyer && d.resource == resource)
                        .count()
                        >= 4
                    {
                        break;
                    }
                }
            }
        }
    }
    pub fn set_deal(&mut self, world: &World, deal: TradeDeal) -> Result<(), String> {
        if deal.buyer == deal.seller
            || deal.factories == 0
            || deal.resource.is_empty()
            || !world.countries.contains_key(&deal.buyer)
            || !world.countries.contains_key(&deal.seller)
        {
            return Err("Некорректный торговый договор".into());
        }
        // Replace a buyer/seller/resource pair, never silently duplicate it.
        self.deals.retain(|existing| {
            existing.buyer != deal.buyer
                || existing.seller != deal.seller
                || existing.resource != deal.resource
        });
        self.deals.push(deal);
        Ok(())
    }
    pub fn cancel_deal(&mut self, buyer: &str, seller: &str, resource: &str) -> bool {
        let before = self.deals.len();
        self.deals
            .retain(|d| d.buyer != buyer || d.seller != seller || d.resource != resource);
        before != self.deals.len()
    }
    /// Snapshot daily deposits, reserve national exports, fulfill available
    /// deals in stable order and move factories to exporters.
    ///
    /// Legacy resource-only API for isolated tests. In the campaign use
    /// settle_with_transport, which enforces geographical routes and convoys.
    pub fn settle(
        &mut self,
        world: &mut World,
        policies: &BTreeMap<String, CountryConstructionPolicy>,
    ) {
        self.settle_with_wars(world, policies, &std::collections::BTreeSet::new());
    }

    /// Active war automatically suspends direct resource deals between rivals.
    pub fn settle_with_wars(
        &mut self,
        world: &mut World,
        policies: &BTreeMap<String, CountryConstructionPolicy>,
        wars: &std::collections::BTreeSet<(String, String)>,
    ) {
        self.settle_internal(world, policies, wars, None, None);
    }

    /// Real campaign settlement: require an actual land or port-to-port sea
    /// connection, then distribute the buyer's finite shared convoy inventory.
    pub fn settle_with_transport(
        &mut self,
        world: &mut World,
        policies: &BTreeMap<String, CountryConstructionPolicy>,
        warfare: &Warfare,
    ) {
        self.settle_internal(world, policies, &warfare.wars, Some(warfare), None);
    }

    pub fn settle_campaign(
        &mut self,
        world: &mut World,
        policies: &BTreeMap<String, CountryConstructionPolicy>,
        warfare: &Warfare,
        research: &crate::research::ResearchWorld,
    ) {
        self.settle_internal(
            world,
            policies,
            &warfare.wars,
            Some(warfare),
            Some(research),
        );
    }

    fn settle_internal(
        &mut self,
        world: &mut World,
        policies: &BTreeMap<String, CountryConstructionPolicy>,
        wars: &std::collections::BTreeSet<(String, String)>,
        warfare: Option<&Warfare>,
        research: Option<&crate::research::ResearchWorld>,
    ) {
        self.last_shipments.clear();
        world.trade_factory_income.clear();
        let mut routes = BTreeMap::<(String, String), TradeRoute>::new();
        let mut unassigned_convoys = BTreeMap::<String, u64>::new();
        if let Some(w) = warfare {
            for (tag, country) in &w.logistics.countries {
                unassigned_convoys.insert(
                    tag.clone(),
                    country
                        .convoys
                        .saturating_sub(country.transport_convoys_committed),
                );
            }
        }
        let mut reports: BTreeMap<String, CountryResources> = world
            .countries
            .keys()
            .cloned()
            .map(|tag| (tag, CountryResources::default()))
            .collect();
        for (&state_id, deposits) in &world.resources_by_state {
            let Some(tag) = world.state_controller(state_id) else {
                continue;
            };
            let Some(report) = reports.get_mut(tag) else {
                continue;
            };
            let infrastructure = world
                .states
                .get(&state_id)
                .map_or(0, |state| state.economy.infrastructure);
            for (resource, &base) in deposits {
                // +20% resource extraction per infrastructure level; fractional
                // output is floored per state, not accumulated across states.
                let gain = research
                    .and_then(|r| r.country(tag))
                    .map_or(0., |c| c.modifier("resource_gain_efficiency"));
                let extracted = (f64::from(base)
                    * (1. + f64::from(infrastructure) * 0.2)
                    * (1. + gain).max(0.))
                .floor()
                .clamp(0., f64::from(u32::MAX)) as u32;
                let total = report.extracted.entry(resource.clone()).or_default();
                *total = total.saturating_add(extracted);
            }
        }
        for (tag, report) in &mut reports {
            let export = self
                .laws
                .get(tag)
                .copied()
                .unwrap_or_default()
                .export_percentage();
            for (resource, &total) in &report.extracted {
                let reserved = u64::from(total) * u64::from(export) / 100;
                let reserved = reserved.min(u64::from(u32::MAX)) as u32;
                report.export_reserved.insert(resource.clone(), reserved);
                report
                    .available
                    .insert(resource.clone(), total.saturating_sub(reserved));
            }
        }
        // The buyer cannot rent more factories than are free after consumer
        // goods. Over-subscribed offers and exhausted exports are only partially
        // fulfilled; no resources or factories are created out of thin air.
        let mut unspent = BTreeMap::<String, u32>::new();
        for tag in world.countries.keys() {
            let policy = policies.get(tag).copied().unwrap_or_default();
            unspent.insert(
                tag.clone(),
                construction::country_capacity(world, tag, policy).map_or(0, |c| c.available),
            );
        }
        for deal in self.deals.iter().chain(self.automatic_deals.iter()) {
            let rivals = if deal.buyer < deal.seller {
                (deal.buyer.clone(), deal.seller.clone())
            } else {
                (deal.seller.clone(), deal.buyer.clone())
            };
            if wars.contains(&rivals)
                || deal.buyer == deal.seller
                || deal.factories == 0
                || !reports.contains_key(&deal.buyer)
                || !reports.contains_key(&deal.seller)
            {
                continue;
            }
            let offer = reports[&deal.seller]
                .export_reserved
                .get(&deal.resource)
                .copied()
                .unwrap_or(0);
            let sold = reports[&deal.seller]
                .exported
                .get(&deal.resource)
                .copied()
                .unwrap_or(0);
            let remaining = offer.saturating_sub(sold);
            if remaining == 0 {
                continue;
            }
            let available_factories = unspent.get(&deal.buyer).copied().unwrap_or(0);
            let requested = deal.factories.min(available_factories);
            let used_factories = requested.min(remaining.saturating_add(7) / 8);
            if used_factories == 0 {
                continue;
            }
            let contracted = remaining.min(used_factories.saturating_mul(8));
            let route = if let Some(w) = warfare {
                routes
                    .entry((deal.buyer.clone(), deal.seller.clone()))
                    .or_insert_with(|| {
                        trade_routes::find_route(
                            world,
                            w,
                            &deal.buyer,
                            &deal.seller,
                            &w.logistics.blocked_sea,
                        )
                    })
                    .clone()
            } else {
                // Compatibility for isolated existing resource-only tests.
                // The real campaign always calls settle_with_transport.
                TradeRoute::Land {
                    provinces: Vec::new(),
                }
            };
            let (delivered, required_convoys, assigned_convoys) = match &route {
                TradeRoute::Land { .. } => (contracted, 0, 0),
                TradeRoute::Unavailable { .. } => (0, 0, 0),
                TradeRoute::Sea { sea_provinces, .. } => {
                    let required = self.convoy_rules.required(contracted, sea_provinces.len());
                    let available = unassigned_convoys.entry(deal.buyer.clone()).or_default();
                    let assigned = (*available).min(required);
                    *available -= assigned;
                    let fulfilled = if required > 0 {
                        (u64::from(contracted) * assigned / required) as u32
                    } else {
                        0
                    };
                    (fulfilled, required, assigned)
                }
            };
            // A non-existent route prevents a contract; a connected route with
            // a convoy shortage still ties up the paid factories until cancelled.
            let charged_factories = if matches!(route, TradeRoute::Unavailable { .. }) {
                0
            } else {
                used_factories
            };
            *unspent.get_mut(&deal.buyer).unwrap() -= charged_factories;
            if charged_factories > 0 {
                let buyer = reports.get_mut(&deal.buyer).unwrap();
                *buyer.imported.entry(deal.resource.clone()).or_default() += delivered;
                *buyer.available.entry(deal.resource.clone()).or_default() += delivered;
                buyer.factories_spent += charged_factories;
                buyer.trade_convoys_used += assigned_convoys;
                let seller = reports.get_mut(&deal.seller).unwrap();
                *seller.exported.entry(deal.resource.clone()).or_default() += contracted;
                seller.factories_received += charged_factories;
            }
            self.last_shipments.push(TradeShipment {
                buyer: deal.buyer.clone(),
                seller: deal.seller.clone(),
                resource: deal.resource.clone(),
                factories: charged_factories,
                contracted: if charged_factories > 0 { contracted } else { 0 },
                delivered,
                required_convoys,
                assigned_convoys,
                route,
            });
        }
        for (tag, report) in &reports {
            if report.factories_received > 0 {
                world
                    .trade_factory_income
                    .insert(tag.clone(), report.factories_received);
            }
        }
        self.last = reports;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::StateEconomy;
    use std::collections::HashMap;
    fn world() -> World {
        let mut world = World::from_map_data(
            &HashMap::from([(1, 1), (2, 2)]),
            &HashMap::from([(1, "A".into()), (2, "B".into())]),
            &HashMap::new(),
            &HashMap::from([
                (
                    1,
                    StateEconomy {
                        civilian_factories: 1,
                        ..Default::default()
                    },
                ),
                (
                    2,
                    StateEconomy {
                        civilian_factories: 4,
                        ..Default::default()
                    },
                ),
            ]),
            &HashMap::new(),
        );
        world
            .resources_by_state
            .insert(1, BTreeMap::from([("steel".into(), 10)]));
        world
            .resources_by_state
            .insert(2, BTreeMap::from([("oil".into(), 5)]));
        world
    }
    #[test]
    fn trade_laws_reserve_resources_even_without_buyers() {
        let mut world = world();
        let mut market = ResourceEconomy::default();
        market.laws.insert("A".into(), TradeLaw::FreeTrade);
        market.settle(&mut world, &BTreeMap::new());
        assert_eq!(market.last["A"].extracted["steel"], 10);
        assert_eq!(market.last["A"].available["steel"], 2);
        assert_eq!(market.last["A"].export_reserved["steel"], 8);
    }
    #[test]
    fn war_suspends_imports_and_exporter_income() {
        let mut world = world();
        let mut market = ResourceEconomy::default();
        market.laws.insert("A".into(), TradeLaw::FreeTrade);
        market
            .set_deal(
                &world,
                TradeDeal {
                    buyer: "B".into(),
                    seller: "A".into(),
                    resource: "steel".into(),
                    factories: 1,
                },
            )
            .unwrap();
        market.settle_with_wars(
            &mut world,
            &BTreeMap::new(),
            &std::collections::BTreeSet::from([("A".into(), "B".into())]),
        );
        assert_eq!(market.last["B"].factories_spent, 0);
        assert!(world.trade_factory_income.is_empty());
        market.settle(&mut world, &BTreeMap::new());
        assert_eq!(market.last["B"].factories_spent, 1);
    }

    #[test]
    fn trade_cannot_duplicate_factories_or_resources_and_capture_changes_output() {
        let mut world = world();
        let mut market = ResourceEconomy::default();
        market.laws.insert("A".into(), TradeLaw::FreeTrade);
        market
            .set_deal(
                &world,
                TradeDeal {
                    buyer: "B".into(),
                    seller: "A".into(),
                    resource: "steel".into(),
                    factories: 8,
                },
            )
            .unwrap();
        market.settle(&mut world, &BTreeMap::new());
        assert_eq!(market.last["B"].imported["steel"], 8);
        assert_eq!(market.last["B"].factories_spent, 1);
        assert_eq!(market.last["A"].factories_received, 1);
        assert_eq!(world.trade_factory_income["A"], 1);
        world.set_province_controller(1, Some("B"));
        market.settle(&mut world, &BTreeMap::new());
        assert_eq!(market.last["A"].extracted.get("steel"), None);
        assert_eq!(market.last["B"].extracted["steel"], 10);
        assert_eq!(market.last["B"].factories_spent, 0);
        assert!(world.trade_factory_income.is_empty());
    }
}

#[cfg(test)]
mod maritime_trade_tests {
    use super::*;
    use crate::logistics::CountryLogistics;
    use crate::map_graph::MapGraph;
    use crate::state::StateEconomy;
    use crate::warfare::{ProvinceConditions, Warfare};
    use std::collections::{BTreeSet, HashMap};

    fn scenario(convoys: u64, overland: bool) -> (World, Warfare, ResourceEconomy) {
        let definition = (1..=5)
            .map(|i| {
                format!(
                    "{i};{i};0;0;{};true;plains;1",
                    if overland || i <= 2 || i == 5 {
                        "land"
                    } else {
                        "sea"
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let rgba = (1u8..=5).flat_map(|i| [i, 0, 0, 255]).collect::<Vec<_>>();
        let map = MapGraph::from_rgba(5, 1, &rgba, &definition, false).unwrap();
        let mut memberships = HashMap::from([(1, 1), (2, 1), (5, 5)]);
        let mut owners = HashMap::from([(1, "A".into()), (5, "B".into())]);
        if overland {
            memberships.insert(3, 3);
            memberships.insert(4, 4);
            owners.insert(3, "N".into());
            owners.insert(4, "N".into());
        }
        let mut world = World::from_map_data(
            &memberships,
            &owners,
            &HashMap::new(),
            &HashMap::from([
                (
                    1,
                    StateEconomy {
                        civilian_factories: 4,
                        ..Default::default()
                    },
                ),
                (
                    5,
                    StateEconomy {
                        civilian_factories: 2,
                        ..Default::default()
                    },
                ),
            ]),
            &HashMap::new(),
        );
        world.geography = map;
        world.resources_by_state.insert(
            5,
            BTreeMap::from([("steel".into(), 40), ("oil".into(), 20)]),
        );
        let mut war = Warfare::default();
        war.logistics.countries.insert(
            "A".into(),
            CountryLogistics {
                capital: Some(1),
                convoys,
                ..Default::default()
            },
        );
        war.logistics.countries.insert(
            "B".into(),
            CountryLogistics {
                capital: Some(5),
                ..Default::default()
            },
        );
        war.conditions.insert(
            2,
            ProvinceConditions {
                naval_base: 1,
                ..Default::default()
            },
        );
        war.conditions.insert(
            5,
            ProvinceConditions {
                naval_base: 1,
                ..Default::default()
            },
        );
        let mut market = ResourceEconomy::default();
        market.laws.insert("B".into(), TradeLaw::FreeTrade);
        market
            .set_deal(
                &world,
                TradeDeal {
                    buyer: "A".into(),
                    seller: "B".into(),
                    resource: "steel".into(),
                    factories: 2,
                },
            )
            .unwrap();
        (world, war, market)
    }

    #[test]
    fn shipping_uses_convoys_and_prevents_duplicate_allocation() {
        let (mut world, war, mut market) = scenario(1, false);
        market
            .set_deal(
                &world,
                TradeDeal {
                    buyer: "A".into(),
                    seller: "B".into(),
                    resource: "oil".into(),
                    factories: 1,
                },
            )
            .unwrap();
        market.settle_with_transport(&mut world, &BTreeMap::new(), &war);
        assert_eq!(market.last_shipments.len(), 2);
        assert_eq!(market.last_shipments[0].required_convoys, 2);
        assert_eq!(market.last_shipments[0].assigned_convoys, 1);
        assert_eq!(market.last_shipments[0].delivered, 8);
        assert_eq!(market.last_shipments[1].assigned_convoys, 0);
        assert_eq!(market.last_shipments[1].delivered, 0);
        assert_eq!(market.last["A"].trade_convoys_used, 1);
        assert_eq!(market.last["A"].imported["steel"], 8);
        assert_eq!(market.last["A"].factories_spent, 3);
        assert_eq!(market.last["B"].factories_received, 3);
        assert_eq!(world.trade_factory_income["B"], 3);
        // Release convoys in a fresh settlement after cancelling contracts.
        market.deals.clear();
        market.settle_with_transport(&mut world, &BTreeMap::new(), &war);
        assert_eq!(market.last["A"].trade_convoys_used, 0);
        assert_eq!(market.last["A"].factories_spent, 0);
        assert!(world.trade_factory_income.is_empty());
    }

    #[test]
    fn a_land_trade_route_consumes_no_convoys() {
        let (mut world, war, mut market) = scenario(0, true);
        market.settle_with_transport(&mut world, &BTreeMap::new(), &war);
        assert_eq!(market.last["A"].imported["steel"], 16);
        assert_eq!(market.last["A"].trade_convoys_used, 0);
        assert!(matches!(
            market.last_shipments[0].route,
            TradeRoute::Land { .. }
        ));
    }

    #[test]
    fn closing_a_sea_province_blocks_payment_and_deliveries() {
        let (mut world, mut war, mut market) = scenario(10, false);
        war.logistics.blocked_sea = BTreeSet::from([3]);
        market.settle_with_transport(&mut world, &BTreeMap::new(), &war);
        assert_eq!(market.last["A"].factories_spent, 0);
        assert!(market.last["A"].imported.is_empty());
        assert_eq!(market.last["B"].factories_received, 0);
        assert!(matches!(
            market.last_shipments[0].route,
            TradeRoute::Unavailable { .. }
        ));
    }

    #[test]
    fn naval_trade_restores_delivery_when_new_convoys_arrive() {
        let (mut world, mut war, mut market) = scenario(0, false);
        market.settle_with_transport(&mut world, &BTreeMap::new(), &war);
        assert_eq!(market.last_shipments[0].delivered, 0);
        assert_eq!(market.last["A"].factories_spent, 2);
        war.logistics.countries.get_mut("A").unwrap().convoys = 2;
        market.settle_with_transport(&mut world, &BTreeMap::new(), &war);
        assert_eq!(market.last_shipments[0].delivered, 16);
        assert_eq!(market.last["A"].trade_convoys_used, 2);
    }
}
