//! Data-driven starter model for HOI4-style state construction.
//! Costs and speed constants are isolated here so they can later be loaded from
//! common/buildings and common/defines instead of being scattered through UI code.
use std::collections::{HashMap, VecDeque};
use crate::world::World;

pub const BASE_CONSTRUCTION_POINTS_PER_CIVILIAN_FACTORY: f32 = 5.0;
pub const MAX_CIVILIAN_FACTORIES_PER_LINE: u32 = 15;
pub const MAX_SHARED_BUILDING_SLOTS: u16 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuildingType {
    CivilianFactory,
    MilitaryFactory,
    Dockyard,
    SyntheticRefinery,
    FuelSilo,
    RocketSite,
    NuclearReactor,
}

impl BuildingType {
    pub const ALL: [Self; 7] = [
        Self::CivilianFactory, Self::MilitaryFactory, Self::Dockyard,
        Self::SyntheticRefinery, Self::FuelSilo, Self::RocketSite, Self::NuclearReactor,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::CivilianFactory => "Civilian factory",
            Self::MilitaryFactory => "Military factory",
            Self::Dockyard => "Dockyard",
            Self::SyntheticRefinery => "Synthetic refinery",
            Self::FuelSilo => "Fuel silo",
            Self::RocketSite => "Rocket site",
            Self::NuclearReactor => "Nuclear reactor",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::CivilianFactory => "Civ",
            Self::MilitaryFactory => "Mil",
            Self::Dockyard => "Dock",
            Self::SyntheticRefinery => "Synth",
            Self::FuelSilo => "Fuel",
            Self::RocketSite => "Rocket",
            Self::NuclearReactor => "Nuke",
        }
    }

    pub fn glyph(self) -> &'static str {
        match self {
            Self::CivilianFactory => "c",
            Self::MilitaryFactory => "m",
            Self::Dockyard => "n",
            Self::SyntheticRefinery => "s",
            Self::FuelSilo => "f",
            Self::RocketSite => "r",
            Self::NuclearReactor => "a",
        }
    }

    pub fn base_cost(self) -> f32 {
        match self {
            Self::CivilianFactory | Self::MilitaryFactory => 10_800.0,
            Self::Dockyard => 6_400.0,
            Self::SyntheticRefinery => 14_000.0,
            Self::FuelSilo => 3_000.0,
            Self::RocketSite => 12_000.0,
            Self::NuclearReactor => 30_000.0,
        }
    }

    /// Base cost for the next level in the same state. Values are placeholders
    /// until the game's common/buildings definitions are parsed.
    pub fn per_level_extra_cost(self) -> f32 {
        0.0
    }

    pub fn built_count(self, economy: crate::state::StateEconomy) -> u16 {
        match self {
            Self::CivilianFactory => economy.civilian_factories,
            Self::MilitaryFactory => economy.military_factories,
            Self::Dockyard => economy.dockyards,
            Self::SyntheticRefinery => economy.synthetic_refineries,
            Self::FuelSilo => economy.fuel_silos,
            Self::RocketSite => economy.rocket_sites,
            Self::NuclearReactor => economy.nuclear_reactors,
        }
    }

    pub fn add_one(self, economy: &mut crate::state::StateEconomy) {
        match self {
            Self::CivilianFactory => economy.civilian_factories = economy.civilian_factories.saturating_add(1),
            Self::MilitaryFactory => economy.military_factories = economy.military_factories.saturating_add(1),
            Self::Dockyard => economy.dockyards = economy.dockyards.saturating_add(1),
            Self::SyntheticRefinery => economy.synthetic_refineries = economy.synthetic_refineries.saturating_add(1),
            Self::FuelSilo => economy.fuel_silos = economy.fuel_silos.saturating_add(1),
            Self::RocketSite => economy.rocket_sites = economy.rocket_sites.saturating_add(1),
            Self::NuclearReactor => economy.nuclear_reactors = economy.nuclear_reactors.saturating_add(1),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstructionProject {
    pub building: BuildingType,
    pub progress: f32,
    pub cost: f32,
}

impl ConstructionProject {
    pub fn new(building: BuildingType, state_level: u16) -> Self {
        Self {
            building,
            progress: 0.0,
            cost: building.base_cost()
                + f32::from(state_level) * building.per_level_extra_cost(),
        }
    }

    pub fn progress_fraction(&self) -> f32 {
        if self.cost <= 0.0 { 1.0 } else { (self.progress / self.cost).clamp(0.0, 1.0) }
    }
}

pub type ConstructionQueues = HashMap<u32, VecDeque<ConstructionProject>>;

/// Advance one in-game day. All owned-state civilian factories form a shared
/// pool; up to 15 factories are assigned to each active project in state-ID order.
/// Consumer-goods laws, national modifiers and scripted effects are not yet modeled.
pub fn advance_day(world: &mut World, country_tag: &str, queues: &mut ConstructionQueues) {
    let mut state_ids: Vec<u32> = match world.countries.get(country_tag) {
        Some(country) => country.state_ids.iter().copied().collect(),
        None => return,
    };
    state_ids.sort_unstable();

    let mut available_factories: u32 = state_ids.iter()
        .filter_map(|id| world.states.get(id))
        .map(|state| u32::from(state.economy.civilian_factories))
        .sum();

    if available_factories == 0 { return; }

    for state_id in state_ids {
        if available_factories == 0 { break; }
        let project = match queues.get_mut(&state_id).and_then(|q| q.front_mut()) {
            Some(project) => project,
            None => continue,
        };
        let state = match world.states.get(&state_id) {
            Some(state) => state,
            None => continue,
        };
        let assigned = available_factories.min(MAX_CIVILIAN_FACTORIES_PER_LINE);
        available_factories -= assigned;
        let infrastructure_bonus = (1.0 + f32::from(state.economy.infrastructure) * 0.1).min(1.5);
        project.progress += assigned as f32 * BASE_CONSTRUCTION_POINTS_PER_CIVILIAN_FACTORY * infrastructure_bonus;

        if project.progress + f32::EPSILON >= project.cost {
            let building = project.building;
            if let Some(state) = world.states.get_mut(&state_id) {
                building.add_one(&mut state.economy);
            }
            if let Some(queue) = queues.get_mut(&state_id) {
                queue.pop_front();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::StateEconomy;
    use crate::world::{Country, Province, State, World};
    use std::collections::{BTreeSet, BTreeMap};

    fn test_world() -> World {
        let mut countries = BTreeMap::new();
        countries.insert("TST".to_owned(), Country { tag: "TST".to_owned(), state_ids: BTreeSet::from([1]) });
        let mut states = BTreeMap::new();
        states.insert(1, State { id: 1, name: None, owner: Some("TST".to_owned()), province_ids: vec![],
            shared_building_slots: 5, economy: StateEconomy { civilian_factories: 2, infrastructure: 5, ..StateEconomy::default() } });
        World { countries, states, provinces: BTreeMap::<u32, Province>::new() }
    }

    #[test]
    fn project_cost_and_progress_are_explicit() {
        let project = ConstructionProject::new(BuildingType::MilitaryFactory, 2);
        assert_eq!(project.cost, 10_800.0);
        assert_eq!(project.progress_fraction(), 0.0);
    }

    #[test]
    fn daily_tick_uses_civilian_factories_and_infrastructure() {
        let mut world = test_world();
        let mut queues = ConstructionQueues::new();
        queues.insert(1, VecDeque::from([ConstructionProject { building: BuildingType::CivilianFactory, progress: 10_790.0, cost: 10_800.0 }]));
        advance_day(&mut world, "TST", &mut queues);
        assert_eq!(world.states[&1].economy.civilian_factories, 3);
        assert!(queues.get(&1).is_none_or(VecDeque::is_empty));
    }

    #[test]
    fn country_without_civilian_factories_does_not_progress() {
        let mut world = test_world();
        world.states.get_mut(&1).unwrap().economy.civilian_factories = 0;
        let mut queues = ConstructionQueues::new();
        queues.insert(1, VecDeque::from([ConstructionProject::new(BuildingType::Dockyard, 0)]));
        advance_day(&mut world, "TST", &mut queues);
        assert_eq!(queues[&1][0].progress, 0.0);
    }
}
