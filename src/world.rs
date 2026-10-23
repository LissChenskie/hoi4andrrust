//! Authoritative world model, independent from Macroquad rendering.
use std::collections::{BTreeMap, BTreeSet, HashMap};

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Province {
    pub id: u32,
    pub state_id: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct World {
    pub countries: BTreeMap<String, Country>,
    pub states: BTreeMap<u32, State>,
    pub provinces: BTreeMap<u32, Province>,
}

impl World {
    /// Builds the simulation model from the currently loaded map metadata.
    pub fn from_map_data(
        state_by_province: &HashMap<u32, u32>,
        owner_by_state: &HashMap<u32, String>,
        state_names: &HashMap<u32, String>,
    ) -> Self {
        let mut world = Self::default();

        for (&province_id, &state_id) in state_by_province {
            world.provinces.insert(province_id, Province {
                id: province_id,
                state_id: Some(state_id),
            });
            world.states.entry(state_id).or_insert_with(|| State {
                id: state_id,
                name: state_names.get(&state_id).cloned(),
                owner: owner_by_state.get(&state_id).cloned(),
                province_ids: Vec::new(),
            }).province_ids.push(province_id);
        }

        // Keep states that have an owner/name even if province membership is incomplete.
        for (&state_id, owner) in owner_by_state {
            let state = world.states.entry(state_id).or_insert_with(|| State {
                id: state_id,
                name: state_names.get(&state_id).cloned(),
                owner: None,
                province_ids: Vec::new(),
            });
            state.owner = Some(owner.clone());
        }
        for (&state_id, name) in state_names {
            world.states.entry(state_id).or_insert_with(|| State {
                id: state_id,
                name: Some(name.clone()),
                owner: owner_by_state.get(&state_id).cloned(),
                province_ids: Vec::new(),
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

    pub fn state_for_province(&self, province_id: u32) -> Option<&State> {
        let state_id = self.provinces.get(&province_id)?.state_id?;
        self.states.get(&state_id)
    }

    pub fn owner_of_province(&self, province_id: u32) -> Option<&str> {
        self.state_for_province(province_id)?.owner.as_deref()
    }

    /// Changes the state owner and keeps each country's state index synchronized.
    pub fn set_state_owner(&mut self, state_id: u32, new_owner: Option<&str>) -> bool {
        let Some(state) = self.states.get_mut(&state_id) else {
            return false;
        };
        let old_owner = state.owner.take();
        if let Some(old_tag) = old_owner {
            if let Some(country) = self.countries.get_mut(&old_tag) {
                country.state_ids.remove(&state_id);
                if country.state_ids.is_empty() {
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
        World::from_map_data(&membership, &owners, &names)
    }

    #[test]
    fn builds_country_state_province_relationships() {
        let world = fixture();
        assert_eq!(world.provinces.len(), 3);
        assert_eq!(world.states[&1].province_ids, vec![101, 102]);
        assert_eq!(world.states[&1].name.as_deref(), Some("State One"));
        assert_eq!(world.countries["GER"].state_ids.iter().copied().collect::<Vec<_>>(), vec![1]);
        assert_eq!(world.owner_of_province(102), Some("GER"));
        assert_eq!(world.owner_of_province(999), None);
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
}
