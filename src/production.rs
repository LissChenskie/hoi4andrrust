//! Data-driven military-factory production prototype.
//!
//! Implements factory allocation by line priority, equipment output (IC),
//! efficiency growth and national stockpiles. Exact equipment definitions,
//! historical 1936 lines, technologies, resource consumption and energy must
//! be imported later. Defaults below are explicit prototypes, not hidden
//! guesses about a country's historical production.
use crate::world::World;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Equipment {
    InfantryKit,
    SupportKit,
    Truck,
    FieldEquipment,
    Aircraft,
}

impl Equipment {
    pub const ALL: [Self; 5] = [
        Self::InfantryKit,
        Self::SupportKit,
        Self::Truck,
        Self::FieldEquipment,
        Self::Aircraft,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::InfantryKit => "Пехотное снаряжение",
            Self::SupportKit => "Доп. оснащение",
            Self::Truck => "Грузовики",
            Self::FieldEquipment => "Артиллерия",
            Self::Aircraft => "Авиация",
        }
    }
    pub fn short_label(self) -> &'static str {
        match self {
            Self::InfantryKit => "ПЕХ",
            Self::SupportKit => "ДОП",
            Self::Truck => "АВТО",
            Self::FieldEquipment => "АРТ",
            Self::Aircraft => "АВИА",
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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
    pub fn for_research(&self, c: Option<&crate::research::CountryResearch>) -> Self {
        let mut r = *self;
        if let Some(c) = c {
            r.factory_output_bonus += c.modifier("industrial_capacity_factory");
            r.efficiency_cap = (r.efficiency_cap
                + c.modifier("production_factory_max_efficiency_factor"))
            .max(0.0);
            r.efficiency_growth *=
                (1.0 + c.modifier("production_factory_efficiency_gain_factor")).max(0.0);
            r.starting_efficiency = (r.starting_efficiency
                + c.modifier("production_factory_start_efficiency_factor"))
            .clamp(0.0, r.efficiency_cap.max(0.0));
        }
        r
    }

    pub fn ic_per_factory(&self) -> f64 {
        let power = self.powered_fraction.clamp(0.0, 1.0);
        let output = self.base_factory_output
            + (self.powered_factory_output - self.base_factory_output) * power;
        (output * (1.0 + self.factory_output_bonus).max(0.0) * self.resource_factor.clamp(0.0, 1.0))
            .max(0.0)
    }
    pub fn efficiency_gain(&self, efficiency: f64) -> f64 {
        let cap = self.efficiency_cap.max(0.0);
        if efficiency >= cap || cap == 0.0 {
            return 0.0;
        }
        // HOI4-like curve: growth slows as the line nears its cap.
        (self.efficiency_growth * cap * cap / efficiency.max(0.01)).max(0.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductionLine {
    pub id: u64,
    pub equipment: Equipment,
    /// Concrete imported equipment model; None retains the original prototype category.
    pub model: Option<String>,
    pub requested_factories: u32,
    pub efficiency: f64,
    /// Unspent industrial capacity towards the next whole equipment item.
    pub partial_ic: f64,
    pub total_produced: u64,
    pub last_day_units: u64,
    pub last_day_ic: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Allocation {
    pub assigned: u32,
    pub requested: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CountryProduction {
    pub lines: Vec<ProductionLine>,
    pub stockpile: BTreeMap<Equipment, u64>,
    pub model_stockpile: BTreeMap<String, u64>,
    next_id: u64,
}

impl CountryProduction {
    pub fn create(&mut self, equipment: Equipment, requested: u32, rules: &ProductionRules) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.lines.push(ProductionLine {
            id,
            equipment,
            model: None,
            requested_factories: requested.min(rules.max_factories_per_line),
            efficiency: rules.starting_efficiency,
            partial_ic: 0.0,
            total_produced: 0,
            last_day_units: 0,
            last_day_ic: 0.0,
        });
        id
    }
    pub fn create_model(
        &mut self,
        model: &str,
        equipment: Equipment,
        requested: u32,
        rules: &ProductionRules,
    ) -> u64 {
        let id = self.create(equipment, requested, rules);
        self.lines.last_mut().unwrap().model = Some(model.to_owned());
        id
    }
    pub fn remove(&mut self, id: u64) -> bool {
        let old = self.lines.len();
        self.lines.retain(|line| line.id != id);
        self.lines.len() != old
    }
    pub fn change_requested(&mut self, id: u64, delta: i32, rules: &ProductionRules) -> bool {
        let Some(line) = self.lines.iter_mut().find(|line| line.id == id) else {
            return false;
        };
        let old = line.requested_factories;
        line.requested_factories = (i64::from(old) + i64::from(delta))
            .clamp(0, i64::from(rules.max_factories_per_line))
            as u32;
        old != line.requested_factories
    }
    pub fn move_up(&mut self, id: u64) -> bool {
        let Some(index) = self.lines.iter().position(|line| line.id == id) else {
            return false;
        };
        if index == 0 {
            return false;
        }
        self.lines.swap(index - 1, index);
        true
    }
    pub fn move_down(&mut self, id: u64) -> bool {
        let Some(index) = self.lines.iter().position(|line| line.id == id) else {
            return false;
        };
        if index + 1 >= self.lines.len() {
            return false;
        }
        self.lines.swap(index, index + 1);
        true
    }
    pub fn switch_equipment(
        &mut self,
        id: u64,
        equipment: Equipment,
        rules: &ProductionRules,
    ) -> bool {
        let Some(line) = self.lines.iter_mut().find(|line| line.id == id) else {
            return false;
        };
        if line.equipment == equipment {
            return false;
        }
        line.equipment = equipment;
        line.model = None;
        line.efficiency = (line.efficiency * rules.switch_retention)
            .clamp(rules.starting_efficiency, rules.efficiency_cap);
        line.partial_ic = 0.0;
        line.last_day_units = 0;
        line.last_day_ic = 0.0;
        true
    }
    pub fn switch_model(
        &mut self,
        id: u64,
        model: &str,
        equipment: Equipment,
        rules: &ProductionRules,
    ) -> bool {
        let Some(line) = self.lines.iter_mut().find(|line| line.id == id) else {
            return false;
        };
        if line.model.as_deref() == Some(model) {
            return false;
        }
        line.equipment = equipment;
        line.model = Some(model.into());
        line.efficiency = (line.efficiency * rules.switch_retention)
            .clamp(rules.starting_efficiency, rules.efficiency_cap);
        line.partial_ic = 0.0;
        line.last_day_units = 0;
        line.last_day_ic = 0.0;
        true
    }
    pub fn allocations(
        &self,
        available_factories: u32,
        rules: &ProductionRules,
    ) -> Vec<Allocation> {
        let mut free = available_factories;
        self.lines
            .iter()
            .map(|line| {
                let requested = line.requested_factories.min(rules.max_factories_per_line);
                let assigned = free.min(requested);
                free -= assigned;
                Allocation {
                    assigned,
                    requested,
                }
            })
            .collect()
    }
    pub fn allocated_total(&self, available: u32, rules: &ProductionRules) -> u32 {
        self.allocations(available, rules)
            .iter()
            .map(|a| a.assigned)
            .sum()
    }
    pub fn tick(&mut self, available_factories: u32, rules: &ProductionRules) {
        self.tick_with_models(available_factories, rules, &BTreeMap::new());
    }
    pub fn tick_with_models(
        &mut self,
        available_factories: u32,
        rules: &ProductionRules,
        model_costs: &BTreeMap<String, f64>,
    ) {
        let allocations = self.allocations(available_factories, rules);
        let ic_per_factory = rules.ic_per_factory();
        for (line, allocation) in self.lines.iter_mut().zip(allocations) {
            line.last_day_units = 0;
            line.last_day_ic = 0.0;
            if allocation.assigned == 0 {
                continue;
            }
            let ic = allocation.assigned as f64 * ic_per_factory * line.efficiency.max(0.0);
            if !ic.is_finite() || ic <= 0.0 {
                continue;
            }
            line.last_day_ic = ic;
            let item_cost = if let Some(model) = line.model.as_ref() {
                let Some(cost) = model_costs
                    .get(model)
                    .copied()
                    .filter(|v| v.is_finite() && *v > 0.0)
                else {
                    continue;
                };
                cost
            } else {
                line.equipment.placeholder_cost()
            };
            let budget = line.partial_ic + ic;
            let made = (budget / item_cost).floor().clamp(0.0, u64::MAX as f64) as u64;
            line.partial_ic = (budget - made as f64 * item_cost).max(0.0);
            if line.partial_ic >= item_cost {
                // Floating point roundoff correction only; never lose whole items.
                line.partial_ic %= item_cost;
            }
            line.total_produced = line.total_produced.saturating_add(made);
            line.last_day_units = made;
            let stock = if let Some(model) = &line.model {
                self.model_stockpile.entry(model.clone()).or_default()
            } else {
                self.stockpile.entry(line.equipment).or_default()
            };
            *stock = stock.saturating_add(made);
            line.efficiency = (line.efficiency + rules.efficiency_gain(line.efficiency))
                .min(rules.efficiency_cap);
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProductionState {
    pub by_country: BTreeMap<String, CountryProduction>,
}

impl ProductionState {
    pub fn country(&self, tag: &str) -> Option<&CountryProduction> {
        self.by_country.get(tag)
    }
    pub fn country_mut(&mut self, tag: &str) -> &mut CountryProduction {
        self.by_country.entry(tag.to_owned()).or_default()
    }
    pub fn tick_country(&mut self, world: &World, tag: &str, rules: &ProductionRules) {
        let Some(economy) = world.country_economy(tag) else {
            return;
        };
        self.country_mut(tag)
            .tick(economy.military_factories, rules);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concrete_models_use_imported_cost_and_only_the_model_stockpile() {
        let rules = ProductionRules {
            starting_efficiency: 1.0,
            efficiency_cap: 1.0,
            ..Default::default()
        };
        let mut country = CountryProduction::default();
        country.create_model("rifle_1936", Equipment::InfantryKit, 1, &rules);
        country.tick_with_models(1, &rules, &BTreeMap::from([("rifle_1936".into(), 1.0)]));
        let expected = rules.ic_per_factory().floor() as u64;
        assert_eq!(country.model_stockpile["rifle_1936"], expected);
        assert!(country.stockpile.is_empty());
        let before = country.model_stockpile.clone();
        country.tick_with_models(1, &rules, &BTreeMap::new());
        assert_eq!(before, country.model_stockpile);
    }
    use crate::state::StateEconomy;
    use std::collections::HashMap;

    fn world(factories: u16) -> World {
        World::from_map_data(
            &HashMap::from([(12, 1)]),
            &HashMap::from([(1, "ABC".to_owned())]),
            &HashMap::new(),
            &HashMap::from([(
                1,
                StateEconomy {
                    military_factories: factories,
                    ..Default::default()
                },
            )]),
            &HashMap::new(),
        )
    }

    #[test]
    fn production_uses_fractional_ic_and_adds_to_national_stockpile() {
        let rules = ProductionRules::default();
        let mut game = ProductionState::default();
        let id = game
            .country_mut("ABC")
            .create(Equipment::InfantryKit, 2, &rules);
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
        assert_eq!(
            c.allocations(7, &rules)
                .iter()
                .map(|a| a.assigned)
                .collect::<Vec<_>>(),
            vec![6, 1]
        );
        assert!(c.move_up(b));
        assert_eq!(
            c.allocations(7, &rules)
                .iter()
                .map(|a| a.assigned)
                .collect::<Vec<_>>(),
            vec![4, 3]
        );
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
        game.country_mut("ABC")
            .create(Equipment::SupportKit, 7, &rules);
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
        for _ in 0..1000 {
            c.tick(1, &rules);
        }
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
        game.country_mut("ABC")
            .create(Equipment::InfantryKit, 2, &rules);
        game.tick_country(&world(2), "ABC", &rules);
        assert!(game.country("DEF").is_none());
        assert!(game.country("ABC").unwrap().stockpile[&Equipment::InfantryKit] > 0);
    }

    #[test]
    fn completed_military_factory_increases_real_production_next_day() {
        use crate::construction::{
            self, BuildingRules, BuildingType, ConstructionModifiers, ConstructionOrder,
            ConstructionQueues, CountryConstructionPolicy,
        };
        let rules = ProductionRules::default();
        let mut world = world(1);
        let state = world.states.get_mut(&1).unwrap();
        state.economy.civilian_factories = 15;
        state.shared_building_slots = 25;
        let mut game = ProductionState::default();
        game.country_mut("ABC")
            .create(Equipment::InfantryKit, 2, &rules);
        let mut queue = ConstructionQueues::new();
        let building_rules = BuildingRules::default();
        construction::enqueue_project(
            &world,
            "ABC",
            1,
            &mut queue,
            &building_rules,
            BuildingType::MilitaryFactory,
        )
        .unwrap();
        // Fast-forward one building for integration testing without simulating
        // thousands of days; completion is still performed by the real engine.
        queue.get_mut(&1).unwrap()[0].cost = 1.0;
        let mut order = ConstructionOrder::default();

        // Day 1: equipment production only sees the old one MIL factory.
        game.tick_country(&world, "ABC", &rules);
        let day_one = game.country("ABC").unwrap().lines[0].last_day_ic;
        construction::advance_day_by_order(
            &mut world,
            "ABC",
            &mut queue,
            &mut order,
            &building_rules,
            CountryConstructionPolicy::default(),
            &ConstructionModifiers::default(),
        );
        assert_eq!(world.country_economy("ABC").unwrap().military_factories, 2);
        assert!(queue.is_empty());

        // Day 2: the second factory contributes output.
        game.tick_country(&world, "ABC", &rules);
        let day_two = game.country("ABC").unwrap().lines[0].last_day_ic;
        assert!(day_two > day_one * 2.0 - 0.0001);
    }

    #[test]
    fn equipment_switch_retains_some_efficiency_but_not_old_fractional_work() {
        let mut c = CountryProduction::default();
        let rules = ProductionRules::default();
        let id = c.create(Equipment::SupportKit, 4, &rules);
        for _ in 0..40 {
            c.tick(4, &rules);
        }
        let previous = c.lines[0].efficiency;
        let old_stock = c
            .stockpile
            .get(&Equipment::SupportKit)
            .copied()
            .unwrap_or(0);
        assert!(c.switch_equipment(id, Equipment::Aircraft, &rules));
        assert!(c.lines[0].efficiency < previous);
        assert_eq!(c.lines[0].partial_ic, 0.0);
        c.tick(4, &rules);
        assert_eq!(
            c.stockpile
                .get(&Equipment::SupportKit)
                .copied()
                .unwrap_or(0),
            old_stock
        );
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
