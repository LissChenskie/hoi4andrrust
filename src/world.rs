//! Authoritative world model, independent from Macroquad rendering.
use std::collections::{BTreeMap, BTreeSet, HashMap};
use crate::state::StateEconomy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Country {
    pub tag: String,
    pub state_ids: BTreeSet<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub id: u32,
    pub name: Option<String>,
    pub owner: Option<String>,
    pub province_ids: Vec<u32>,
    pub shared_building_slots: u16,
    pub economy: StateEconomy,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CountryEconomy {
    pub states: u32,
    pub infrastructure_total: u32,
    pub civilian_factories: u32,
    pub military_factories: u32,
    pub dockyards: u32,
    pub synthetic_refineries: u32,
    pub fuel_silos: u32,
    pub rocket_sites: u32,
    pub nuclear_reactors: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Province {
    pub id: u32,
    pub state_id: Option<u32>,
    /// Actual control is per province; its legal owner is inherited from State.
    pub controller: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct World {
    pub countries: BTreeMap<String, Country>,
    pub states: BTreeMap<u32, State>,
    pub provinces: BTreeMap<u32, Province>,
    pub geography: crate::map_graph::MapGraph,
}

impl World {
    /// Builds the simulation model from the currently loaded map metadata.
    pub fn from_map_data(
        state_by_province: &HashMap<u32, u32>,
        owner_by_state: &HashMap<u32, String>,
        state_names: &HashMap<u32, String>,
        economy_by_state: &HashMap<u32, StateEconomy>,
        shared_slots_by_state: &HashMap<u32, u16>,
    ) -> Self {
        let mut world = Self::default();

        for (&province_id, &state_id) in state_by_province {
            world.provinces.insert(province_id, Province {
                id: province_id,
                state_id: Some(state_id),
                controller: owner_by_state.get(&state_id).cloned(),
            });
            world.states.entry(state_id).or_insert_with(|| State {
                id: state_id,
                name: state_names.get(&state_id).cloned(),
                owner: owner_by_state.get(&state_id).cloned(),
                province_ids: Vec::new(),
                shared_building_slots: shared_slots_by_state.get(&state_id).copied().unwrap_or(0),
                economy: economy_by_state.get(&state_id).copied().unwrap_or_default(),
            }).province_ids.push(province_id);
        }

        // Keep states that have an owner/name even if province membership is incomplete.
        for (&state_id, owner) in owner_by_state {
            let state = world.states.entry(state_id).or_insert_with(|| State {
                id: state_id,
                name: state_names.get(&state_id).cloned(),
                owner: None,
                province_ids: Vec::new(),
                shared_building_slots: shared_slots_by_state.get(&state_id).copied().unwrap_or(0),
                economy: economy_by_state.get(&state_id).copied().unwrap_or_default(),
            });
            state.owner = Some(owner.clone());
        }
        for (&state_id, name) in state_names {
            world.states.entry(state_id).or_insert_with(|| State {
                id: state_id,
                name: Some(name.clone()),
                owner: owner_by_state.get(&state_id).cloned(),
                province_ids: Vec::new(),
                shared_building_slots: shared_slots_by_state.get(&state_id).copied().unwrap_or(0),
                economy: economy_by_state.get(&state_id).copied().unwrap_or_default(),
            }).name = Some(name.clone());
        }

        for state in world.states.values_mut() {
            state.province_ids.sort_unstable();
            if let Some(tag) = state.owner.as_ref() {
                world.countries.entry(tag.clone()).or_insert_with(|| Country {
                    tag: tag.clone(),
                    state_ids: BTreeSet::new(),
                }).state_ids.insert(state.id);
            }
        }
        world
    }

    /// Aggregate state-level factory counts for a country. Production rules are separate.
    pub fn country_economy(&self, country_tag: &str) -> Option<CountryEconomy> {
        let country = self.countries.get(country_tag)?;
        let mut totals = CountryEconomy::default();
        for state_id in &country.state_ids {
            let Some(state) = self.states.get(state_id) else { continue };
            totals.states += 1;
            totals.infrastructure_total += u32::from(state.economy.infrastructure);
            totals.civilian_factories += u32::from(state.economy.civilian_factories);
            totals.military_factories += u32::from(state.economy.military_factories);
            totals.dockyards += u32::from(state.economy.dockyards);
            totals.synthetic_refineries += u32::from(state.economy.synthetic_refineries);
            totals.fuel_silos += u32::from(state.economy.fuel_silos);
            totals.rocket_sites += u32::from(state.economy.rocket_sites);
            totals.nuclear_reactors += u32::from(state.economy.nuclear_reactors);
        }
        Some(totals)
    }

    pub fn state_for_province(&self, province_id: u32) -> Option<&State> {
        let state_id = self.provinces.get(&province_id)?.state_id?;
        self.states.get(&state_id)
    }

    pub fn owner_of_province(&self, province_id: u32) -> Option<&str> {
        self.state_for_province(province_id)?.owner.as_deref()
    }

    pub fn controller_of_province(&self, province_id: u32) -> Option<&str> {
        self.provinces.get(&province_id)?.controller.as_deref()
    }

    /// Occupying a province does not transfer its state, legal ownership,
    /// factories, or the controllers of neighbouring provinces.
    pub fn set_province_controller(&mut self, province_id: u32, controller: Option<&str>) -> bool {
        if controller.is_some_and(|tag| tag.is_empty()
            || !tag.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')) {
            return false;
        }
        let Some(province) = self.provinces.get_mut(&province_id) else { return false; };
        province.controller = controller.map(str::to_owned);
        if let Some(tag) = controller {
            self.countries.entry(tag.to_owned()).or_insert_with(|| Country {
                tag: tag.to_owned(), state_ids: BTreeSet::new(),
            });
        }
        true
    }

    /// Changes the state owner and keeps each country's state index synchronized.
    pub fn set_state_owner(&mut self, state_id: u32, new_owner: Option<&str>) -> bool {
        let Some(state) = self.states.get_mut(&state_id) else {
            return false;
        };
        let old_owner = state.owner.take();
        // Peaceful ownership transfers also move unoccupied control. A province
        // occupied by a third country retains that independent controller.
        for id in &state.province_ids {
            if let Some(province) = self.provinces.get_mut(id) {
                if province.controller == old_owner {
                    province.controller = new_owner.filter(|s| !s.is_empty()).map(str::to_owned);
                }
            }
        }
        if let Some(old_tag) = old_owner {
            if let Some(country) = self.countries.get_mut(&old_tag) {
                country.state_ids.remove(&state_id);
                if country.state_ids.is_empty() && !self.provinces.values()
                    .any(|province| province.controller.as_deref() == Some(old_tag.as_str())) {
                    self.countries.remove(&old_tag);
                }
            }
        }

        if let Some(tag) = new_owner.filter(|tag| !tag.is_empty()) {
            let tag = tag.to_owned();
            state.owner = Some(tag.clone());
            self.countries.entry(tag.clone()).or_insert_with(|| Country {
                tag: tag.clone(),
                state_ids: BTreeSet::new(),
            }).state_ids.insert(state_id);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::World;
    use std::collections::HashMap;

    fn fixture() -> World {
        let membership = HashMap::from([(101, 1), (102, 1), (201, 2)]);
        let owners = HashMap::from([(1, "GER".to_owned()), (2, "POL".to_owned())]);
        let names = HashMap::from([(1, "State One".to_owned()), (2, "State Two".to_owned())]);
        World::from_map_data(&membership, &owners, &names, &HashMap::new(), &HashMap::new())
    }

    #[test]
    fn builds_country_state_province_relationships() {
        let world = fixture();
        assert_eq!(world.provinces.len(), 3);
        assert_eq!(world.states[&1].province_ids, vec![101, 102]);
        assert_eq!(world.states[&1].name.as_deref(), Some("State One"));
        assert_eq!(world.countries["GER"].state_ids.iter().copied().collect::<Vec<_>>(), vec![1]);
        assert_eq!(world.owner_of_province(102), Some("GER"));
        assert_eq!(world.country_economy("GER").unwrap().states, 1);
        assert_eq!(world.owner_of_province(999), None);
    }


    #[test]
    fn aggregates_factories_for_the_current_state_owner() {
        use crate::state::StateEconomy;
        let membership = HashMap::from([(101, 1), (201, 2)]);
        let owners = HashMap::from([(1, "GER".to_owned()), (2, "GER".to_owned())]);
        let names = HashMap::new();
        let economies = HashMap::from([
            (1, StateEconomy { infrastructure: 5, civilian_factories: 3, military_factories: 2, dockyards: 1, ..StateEconomy::default() }),
            (2, StateEconomy { infrastructure: 7, civilian_factories: 4, military_factories: 1, dockyards: 2, ..StateEconomy::default() }),
        ]);
        let world = World::from_map_data(&membership, &owners, &names, &economies, &HashMap::new());
        let totals = world.country_economy("GER").unwrap();
        assert_eq!(totals.states, 2);
        assert_eq!(totals.civilian_factories, 7);
        assert_eq!(totals.military_factories, 3);
        assert_eq!(totals.dockyards, 3);
        assert_eq!(totals.infrastructure_total, 12);
    }

    #[test]
    fn transferring_state_updates_both_country_indexes() {
        let mut world = fixture();
        assert!(world.set_state_owner(1, Some("POL")));
        assert!(!world.countries.contains_key("GER"));
        assert_eq!(world.countries["POL"].state_ids.iter().copied().collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(world.owner_of_province(101), Some("POL"));
    }

    #[test]
    fn releasing_state_removes_empty_country() {
        let mut world = fixture();
        assert!(world.set_state_owner(1, None));
        assert!(!world.countries.contains_key("GER"));
        assert_eq!(world.owner_of_province(101), None);
        assert!(!world.set_state_owner(999, Some("GER")));
    }

    #[test]
    fn capturing_one_province_preserves_state_owner_economy_and_other_provinces() {
        let mut world = fixture();
        let before = world.country_economy("GER");
        assert!(world.set_province_controller(101, Some("POL")));
        assert_eq!(world.owner_of_province(101), Some("GER"));
        assert_eq!(world.controller_of_province(101), Some("POL"));
        assert_eq!(world.controller_of_province(102), Some("GER"));
        assert_eq!(world.country_economy("GER"), before);
        assert!(world.countries["GER"].state_ids.contains(&1));
        assert!(!world.set_province_controller(999, Some("POL")));
        assert!(!world.set_province_controller(101, Some("invalid tag")));
    }

    #[test]
    fn ownership_transfer_preserves_foreign_occupation() {
        let mut world = fixture();
        world.set_province_controller(101, Some("FRA"));
        world.set_state_owner(1, Some("POL"));
        assert_eq!(world.owner_of_province(101), Some("POL"));
        assert_eq!(world.controller_of_province(101), Some("FRA"));
        assert_eq!(world.controller_of_province(102), Some("POL"));
    }
}
