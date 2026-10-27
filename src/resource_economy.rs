//! Daily HOI4-style strategic-resource allocation and civilian-factory trade.
//! Resources are a *flow*, not a stockpile. All quantities are integer units/day.
//! No imaginary default national laws, routes or starting trade deals are added.
use crate::{
    construction::{self, CountryConstructionPolicy},
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
pub struct CountryResources {
    pub extracted: BTreeMap<String, u32>,
    pub export_reserved: BTreeMap<String, u32>,
    pub imported: BTreeMap<String, u32>,
    pub exported: BTreeMap<String, u32>,
    pub available: BTreeMap<String, u32>,
    pub factories_spent: u32,
    pub factories_received: u32,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceEconomy {
    pub laws: BTreeMap<String, TradeLaw>,
    pub deals: Vec<TradeDeal>,
    #[serde(default)]
    pub last: BTreeMap<String, CountryResources>,
}
impl ResourceEconomy {
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
    /// Naval routes, convoy losses, trade influence, embargoes and subjects'
    /// special exchange rates must be added before claiming full parity.
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
        world.trade_factory_income.clear();
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
                let extracted = (u64::from(base) * (5 + u64::from(infrastructure)) / 5)
                    .min(u64::from(u32::MAX)) as u32;
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
        for deal in &self.deals {
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
            let delivered = remaining.min(used_factories.saturating_mul(8));
            *unspent.get_mut(&deal.buyer).unwrap() -= used_factories;
            let buyer = reports.get_mut(&deal.buyer).unwrap();
            *buyer.imported.entry(deal.resource.clone()).or_default() += delivered;
            *buyer.available.entry(deal.resource.clone()).or_default() += delivered;
            buyer.factories_spent += used_factories;
            let seller = reports.get_mut(&deal.seller).unwrap();
            *seller.exported.entry(deal.resource.clone()).or_default() += delivered;
            seller.factories_received += used_factories;
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
