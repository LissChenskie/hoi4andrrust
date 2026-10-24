//! Data-driven military-factory production prototype.
//!
//! Implements factory allocation by line priority, equipment output (IC),
//! efficiency growth and national stockpiles. Exact equipment definitions,
//! historical 1936 lines, technologies, resource consumption and energy must
//! be imported later. Defaults below are explicit prototypes, not hidden
//! guesses about a country's historical production.
use std::collections::BTreeMap;
use crate::world::World;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Equipment {
    InfantryKit,
    SupportKit,
    Truck,
    FieldEquipment,
    Aircraft,
}

impl Equipment {
    pub const ALL: [Self; 5] = [
        Self::InfantryKit, Self::SupportKit, Self::Truck,
        Self::FieldEquipment, Self::Aircraft,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::InfantryKit => "Infantry equipment",
            Self::SupportKit => "Support equipment",
            Self::Truck => "Trucks",
            Self::FieldEquipment => "Field equipment",
            Self::Aircraft => "Aircraft",
        }
    }
    pub fn short_label(self) -> &'static str {
        match self {
            Self::InfantryKit => "INF",
            Self::SupportKit => "SUP",
            Self::Truck => "TRK",
            Self::FieldEquipment => "FLD",
            Self::Aircraft => "AIR",
        }
    }
    /// Deliberately provisional IC costs for an original prototype.
    pub fn placeholder_cost(self) -> f64 {
        match self {
            Self::InfantryKit => 0.6,
            Self::SupportKit => 4.0,
            Self::Truck => 2.5,
            Self::FieldEquipment => 3.5,
            Self::Aircraft => 25.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProductionRules {
    pub base_factory_output: f64,
    pub powered_factory_output: f64,
    pub powered_fraction: f64,
    pub factory_output_bonus: f64,
    pub resource_factor: f64,
    pub starting_efficiency: f64,
    pub efficiency_cap: f64,
    pub efficiency_growth: f64,
    pub switch_retention: f64,
    pub max_factories_per_line: u32,
}

impl Default for ProductionRules {
    fn default() -> Self {
        Self {
            // Published industry mechanics distinguish energy-starved
            // factories (3.5 IC) from fully powered factories (4.5 IC).
            base_factory_output: 3.5,
            powered_factory_output: 4.5,
            powered_fraction: 1.0,
            factory_output_bonus: 0.0,
            resource_factor: 1.0,
            starting_efficiency: 0.10,
            efficiency_cap: 0.50,
            efficiency_growth: 0.001,
            switch_retention: 0.50,
            max_factories_per_line: 150,
        }
    }
}

impl ProductionRules {
    pub fn ic_per_factory(&self) -> f64 {
        let power = self.powered_fraction.clamp(0.0, 1.0);
        let output = self.base_factory_output
            + (self.powered_factory_output - self.base_factory_output) * power;
        (output * (1.0 + self.factory_output_bonus).max(0.0)
            * self.resource_factor.clamp(0.0, 1.0)).max(0.0)
    }
    pub fn efficiency_gain(&self, efficiency: f64) -> f64 {
        let cap = self.efficiency_cap.max(0.0);
        if efficiency >= cap || cap == 0.0 { return 0.0; }
        // HOI4-like curve: growth slows as the line nears its cap.
        (self.efficiency_growth * cap * cap / efficiency.max(0.01)).max(0.0)
    }
}

#[derive(Debug, Clone)]
pub struct ProductionLine {
    pub id: u64,
    pub equipment: Equipment,
    pub requested_factories: u32,
    pub efficiency: f64,
    /// Unspent industrial capacity towards the next whole equipment item.
    pub partial_ic: f64,
    pub total_produced: u64,
    pub last_day_units: u64,
    pub last_day_ic: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Allocation {
    pub assigned: u32,
    pub requested: u32,
}

#[derive(Debug, Clone, Default)]
pub struct CountryProduction {
    pub lines: Vec<ProductionLine>,
    pub stockpile: BTreeMap<Equipment, u64>,
    next_id: u64,
}

impl CountryProduction {
    pub fn create(&mut self, equipment: Equipment, requested: u32, rules: &ProductionRules) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.lines.push(ProductionLine {
            id, equipment,
            requested_factories: requested.min(rules.max_factories_per_line),
            efficiency: rules.starting_efficiency,
            partial_ic: 0.0, total_produced: 0,
            last_day_units: 0, last_day_ic: 0.0,
        });
        id
    }
    pub fn remove(&mut self, id: u64) -> bool {
        let old = self.lines.len();
        self.lines.retain(|line| line.id != id);
        self.lines.len() != old
    }
    pub fn change_requested(&mut self, id: u64, delta: i32, rules: &ProductionRules) -> bool {
        let Some(line) = self.lines.iter_mut().find(|line| line.id == id) else { return false; };
        let old = line.requested_factories;
        line.requested_factories = (i64::from(old) + i64::from(delta))
            .clamp(0, i64::from(rules.max_factories_per_line)) as u32;
        old != line.requested_factories
    }
    pub fn move_up(&mut self, id: u64) -> bool {
        let Some(index) = self.lines.iter().position(|line| line.id == id) else { return false; };
        if index == 0 { return false; }
        self.lines.swap(index - 1, index);
        true
    }
    pub fn move_down(&mut self, id: u64) -> bool {
        let Some(index) = self.lines.iter().position(|line| line.id == id) else { return false; };
        if index + 1 >= self.lines.len() { return false; }
        self.lines.swap(index, index + 1);
        true
    }
    pub fn switch_equipment(&mut self, id: u64, equipment: Equipment, rules: &ProductionRules) -> bool {
        let Some(line) = self.lines.iter_mut().find(|line| line.id == id) else { return false; };
        if line.equipment == equipment { return false; }
        line.equipment = equipment;
        line.efficiency = (line.efficiency * rules.switch_retention)
            .clamp(rules.starting_efficiency, rules.efficiency_cap);
        line.partial_ic = 0.0;
        line.last_day_units = 0;
        line.last_day_ic = 0.0;
        true
    }
    pub fn allocations(&self, available_factories: u32, rules: &ProductionRules) -> Vec<Allocation> {
        let mut free = available_factories;
        self.lines.iter().map(|line| {
            let requested = line.requested_factories.min(rules.max_factories_per_line);
            let assigned = free.min(requested);
            free -= assigned;
            Allocation { assigned, requested }
        }).collect()
    }
    pub fn allocated_total(&self, available: u32, rules: &ProductionRules) -> u32 {
        self.allocations(available, rules).iter().map(|a| a.assigned).sum()
    }
    pub fn tick(&mut self, available_factories: u32, rules: &ProductionRules) {
        let allocations = self.allocations(available_factories, rules);
        let ic_per_factory = rules.ic_per_factory();
        for (line, allocation) in self.lines.iter_mut().zip(allocations) {
            line.last_day_units = 0;
            line.last_day_ic = 0.0;
            if allocation.assigned == 0 { continue; }
            let ic = allocation.assigned as f64 * ic_per_factory * line.efficiency.max(0.0);
            if !ic.is_finite() || ic <= 0.0 { continue; }
            line.last_day_ic = ic;
            let item_cost = line.equipment.placeholder_cost();
            let budget = line.partial_ic + ic;
            let made = (budget / item_cost).floor().clamp(0.0, u64::MAX as f64) as u64;
            line.partial_ic = (budget - made as f64 * item_cost).max(0.0);
            if line.partial_ic >= item_cost {
                // Floating point roundoff correction only; never lose whole items.
                line.partial_ic %= item_cost;
            }
            line.total_produced = line.total_produced.saturating_add(made);
            line.last_day_units = made;
            *self.stockpile.entry(line.equipment).or_default() =
                self.stockpile.get(&line.equipment).copied().unwrap_or(0).saturating_add(made);
            line.efficiency = (line.efficiency + rules.efficiency_gain(line.efficiency))
                .min(rules.efficiency_cap);
        }
    }
}

#[derive(Debug, Default)]
pub struct ProductionState {
    pub by_country: BTreeMap<String, CountryProduction>,
}

impl ProductionState {
    pub fn country(&self, tag: &str) -> Option<&CountryProduction> { self.by_country.get(tag) }
    pub fn country_mut(&mut self, tag: &str) -> &mut CountryProduction {
        self.by_country.entry(tag.to_owned()).or_default()
    }
    pub fn tick_country(&mut self, world: &World, tag: &str, rules: &ProductionRules) {
        let Some(economy) = world.country_economy(tag) else { return; };
        self.country_mut(tag).tick(economy.military_factories, rules);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::StateEconomy;
    use std::collections::HashMap;

    fn world(factories: u16) -> World {
        World::from_map_data(
            &HashMap::from([(12, 1)]),
            &HashMap::from([(1, "ABC".to_owned())]),
            &HashMap::new(),
            &HashMap::from([(1, StateEconomy {
                military_factories: factories, ..Default::default()
            })]),
            &HashMap::new(),
        )
    }

    #[test]
    fn production_uses_fractional_ic_and_adds_to_national_stockpile() {
        let rules = ProductionRules::default();
        let mut game = ProductionState::default();
        let id = game.country_mut("ABC").create(Equipment::InfantryKit, 2, &rules);
        game.tick_country(&world(2), "ABC", &rules);
        let country = game.country("ABC").unwrap();
        assert_eq!(country.lines[0].id, id);
        assert_eq!(country.lines[0].last_day_ic, 0.9);
        assert_eq!(country.stockpile[&Equipment::InfantryKit], 1);
        assert!((country.lines[0].partial_ic - 0.3).abs() < 0.000001);
        game.tick_country(&world(2), "ABC", &rules);
        assert!(game.country("ABC").unwrap().stockpile[&Equipment::InfantryKit] > 1);
    }

    #[test]
    fn line_priority_and_factory_capacity_apply_without_overallocation() {
        let rules = ProductionRules::default();
        let mut c = CountryProduction::default();
        let a = c.create(Equipment::SupportKit, 6, &rules);
        let b = c.create(Equipment::Truck, 4, &rules);
        assert_eq!(c.allocations(7, &rules).iter().map(|a| a.assigned).collect::<Vec<_>>(), vec![6, 1]);
        assert!(c.move_up(b));
        assert_eq!(c.allocations(7, &rules).iter().map(|a| a.assigned).collect::<Vec<_>>(), vec![4, 3]);
        assert!(c.change_requested(b, 2000, &rules));
        assert_eq!(c.lines[0].requested_factories, 150);
        assert!(!c.move_up(b));
        assert!(c.remove(a));
        assert_eq!(c.lines.len(), 1);
    }

    #[test]
    fn newly_built_factories_become_available_next_production_tick() {
        let rules = ProductionRules::default();
        let mut game = ProductionState::default();
        game.country_mut("ABC").create(Equipment::SupportKit, 7, &rules);
        game.tick_country(&world(2), "ABC", &rules);
        let first = game.country("ABC").unwrap().lines[0].last_day_ic;
        game.tick_country(&world(7), "ABC", &rules);
        let second = game.country("ABC").unwrap().lines[0].last_day_ic;
        assert!(second > first);
        assert!(second > 3.0 * first);
    }

    #[test]
    fn production_efficiency_grows_and_pauses_without_factories() {
        let rules = ProductionRules::default();
        let mut c = CountryProduction::default();
        let id = c.create(Equipment::Aircraft, 1, &rules);
        let start = c.lines[0].efficiency;
        c.tick(0, &rules);
        assert_eq!(c.lines[0].efficiency, start);
        for _ in 0..1000 { c.tick(1, &rules); }
        assert!(c.lines[0].efficiency <= rules.efficiency_cap);
        assert!(c.lines[0].efficiency > 0.45);
        assert!(c.switch_equipment(id, Equipment::Truck, &rules));
        assert!(c.lines[0].efficiency < rules.efficiency_cap);
        assert_eq!(c.lines[0].partial_ic, 0.0);
    }

    #[test]
    fn country_stockpiles_are_isolated_and_no_hidden_starting_lines() {
        let rules = ProductionRules::default();
        let mut game = ProductionState::default();
        game.country_mut("ABC").create(Equipment::InfantryKit, 2, &rules);
        game.tick_country(&world(2), "ABC", &rules);
        assert!(game.country("DEF").is_none());
        assert!(game.country("ABC").unwrap().stockpile[&Equipment::InfantryKit] > 0);
    }

    #[test]
    fn unavailable_resources_pause_production_but_do_not_forge_stockpile() {
        let mut rules = ProductionRules::default();
        rules.resource_factor = 0.0;
        let mut c = CountryProduction::default();
        c.create(Equipment::InfantryKit, 10, &rules);
        c.tick(10, &rules);
        assert_eq!(c.stockpile.get(&Equipment::InfantryKit), None);
    }
}
