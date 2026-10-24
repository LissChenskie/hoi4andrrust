//! Construction placement choropleth: pure simulation-to-color conversion.
//! Only rendered in placement mode, never baked into the political map.
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use crate::construction::{self, BuildingRules, BuildingType, ConstructionQueues};
use crate::world::World;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BuildAvailability {
    pub used: usize,
    pub capacity: usize,
    pub free: usize,
    pub eligible: bool,
}

impl BuildAvailability {
    /// 0 = plenty of space (green), 1 = completely full (blue).
    pub fn fullness(self) -> f32 {
        if !self.eligible || self.capacity == 0 { return 1.0; }
        (1.0 - self.free as f32 / self.capacity as f32).clamp(0.0, 1.0)
    }

    pub fn color(self) -> [u8; 4] {
        if !self.eligible { return [89, 91, 95, 155]; }
        let amount = self.fullness();
        let low = [55.0, 185.0, 86.0]; // available land = green
        let high = [46.0, 105.0, 211.0]; // exhausted slots = blue
        [
            (low[0] + (high[0] - low[0]) * amount).round() as u8,
            (low[1] + (high[1] - low[1]) * amount).round() as u8,
            (low[2] + (high[2] - low[2]) * amount).round() as u8,
            225,
        ]
    }
}

/// Mirrors enqueue_project validation inputs: total shared slots (including all
/// queued shared buildings) and the selected building's own state level cap.
/// Infrastructure uses a dedicated cap, never the shared slot pool.
pub fn availability(
    world: &World, tag: &str, state_id: u32,
    building: BuildingType, queues: &ConstructionQueues, rules: &BuildingRules,
) -> Option<BuildAvailability> {
    let state = world.states.get(&state_id)?;
    let owned = state.owner.as_deref() == Some(tag);
    let queued = queues.get(&state_id);
    let of_type = queued.map_or(0usize, |q| q.iter().filter(|p| p.building == building).count());
    let existing_type = usize::from(building.built_count(state.economy));
    let type_capacity = usize::from(rules.max_state_level(building));
    let type_free = type_capacity.saturating_sub(existing_type + of_type);
    if !rules.shares_slots(building) {
        return Some(BuildAvailability {
            used: existing_type + of_type,
            capacity: type_capacity,
            free: type_free,
            eligible: owned,
        });
    }
    let capacity = construction::shared_slot_limit(world, state_id, rules).unwrap_or(0);
    let occupied = usize::from(state.economy.occupied_shared_slots());
    let reserved = queued.map_or(0usize, |q| construction::queued_shared_slots_with_rules(q, rules));
    let used = occupied + reserved;
    let free = capacity.saturating_sub(used).min(type_free);
    Some(BuildAvailability { used, capacity, free, eligible: owned })
}

/// Hashing just state data avoids regenerating a 5632x2048 RGBA texture every
/// rendered frame. Invalidate if country, building, ownership, slots or queues change.
pub fn signature(
    world: &World, tag: &str, building: BuildingType,
    queues: &ConstructionQueues, rules: &BuildingRules,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    tag.hash(&mut hasher);
    building.hash(&mut hasher);
    for (&id, state) in &world.states {
        id.hash(&mut hasher);
        state.owner.hash(&mut hasher);
        availability(world, tag, id, building, queues, rules).hash(&mut hasher);
    }
    hasher.finish()
}

pub fn state_colors(
    world: &World, tag: &str, building: BuildingType,
    queues: &ConstructionQueues, rules: &BuildingRules,
) -> HashMap<u32, [u8; 4]> {
    world.states.keys().filter_map(|&id| {
        availability(world, tag, id, building, queues, rules)
            .map(|capacity| (id, capacity.color()))
    }).collect()
}

/// This function is independent from Macroquad, allowing deterministic tests
/// without a graphics context. State ID 0 and missing states remain transparent.
pub fn rgba_pixels(state_ids_by_pixel: &[u32], colors: &HashMap<u32, [u8; 4]>) -> Vec<u8> {
    let mut pixels = vec![0u8; state_ids_by_pixel.len() * 4];
    for (idx, state_id) in state_ids_by_pixel.iter().enumerate() {
        if let Some(color) = colors.get(state_id) {
            pixels[idx * 4..idx * 4 + 4].copy_from_slice(color);
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::StateEconomy;
    use crate::world::{Country, State};
    use std::collections::BTreeSet;

    fn sample() -> World {
        let mut world = World::default();
        world.countries.insert("AAA".to_owned(), Country {
            tag: "AAA".to_owned(), state_ids: BTreeSet::from([1, 2]),
        });
        for (id, owner, civ) in [(1, "AAA", 0), (2, "AAA", 4), (3, "BBB", 0)] {
            world.states.insert(id, State {
                id, name: None, owner: Some(owner.to_owned()),
                province_ids: Vec::new(), shared_building_slots: 5,
                economy: StateEconomy { civilian_factories: civ, ..Default::default() },
            });
        }
        world
    }

    #[test]
    fn low_occupancy_is_green_and_full_states_are_blue() {
        let world = sample();
        let queues = ConstructionQueues::new();
        let rules = BuildingRules::default();
        let free = availability(&world, "AAA", 1, BuildingType::CivilianFactory, &queues, &rules).unwrap();
        let almost_full = availability(&world, "AAA", 2, BuildingType::CivilianFactory, &queues, &rules).unwrap();
        assert_eq!((free.used, free.free, free.capacity), (0, 5, 5));
        assert_eq!((almost_full.used, almost_full.free), (4, 1));
        let green = free.color();
        let blueish = almost_full.color();
        assert!(green[1] > green[2]);
        assert!(blueish[2] > blueish[1]);
    }

    #[test]
    fn reservations_shift_color_toward_blue_immediately() {
        let world = sample();
        let rules = BuildingRules::default();
        let mut queues = ConstructionQueues::new();
        let before = signature(&world, "AAA", BuildingType::CivilianFactory, &queues, &rules);
        assert!(construction::enqueue_project(
            &world, "AAA", 1, &mut queues, &rules, BuildingType::CivilianFactory,
        ).is_ok());
        let after = signature(&world, "AAA", BuildingType::CivilianFactory, &queues, &rules);
        assert_ne!(before, after);
        let availability = availability(&world, "AAA", 1, BuildingType::CivilianFactory, &queues, &rules).unwrap();
        assert_eq!(availability.used, 1);
        assert_eq!(availability.free, 4);
    }

    #[test]
    fn full_state_and_foreign_state_are_not_buildable() {
        let world = sample();
        let rules = BuildingRules::default();
        let queues = ConstructionQueues::new();
        let capacity = availability(&world, "AAA", 2, BuildingType::CivilianFactory, &queues, &rules).unwrap();
        assert!(capacity.eligible);
        assert_eq!(capacity.free, 1);
        let foreign = availability(&world, "AAA", 3, BuildingType::CivilianFactory, &queues, &rules).unwrap();
        assert!(!foreign.eligible);
        assert_ne!(foreign.color(), capacity.color());
        let mut world = world;
        world.states.get_mut(&2).unwrap().economy.civilian_factories = 5;
        let full = availability(&world, "AAA", 2, BuildingType::CivilianFactory, &queues, &rules).unwrap();
        assert_eq!(full.free, 0);
        assert_eq!(full.color()[2], 211);
    }

    #[test]
    fn infrastructure_ignores_shared_slots_and_uses_its_own_levels() {
        let mut world = sample();
        world.states.get_mut(&2).unwrap().economy.infrastructure = 1;
        let q = ConstructionQueues::new();
        let rules = BuildingRules::default();
        let a = availability(&world, "AAA", 2, BuildingType::Infrastructure, &q, &rules).unwrap();
        assert_eq!((a.used, a.capacity, a.free), (1, 5, 4));
    }

    #[test]
    fn overlay_keeps_unknown_states_and_sea_transparent() {
        let colors = HashMap::from([(1u32, [1, 2, 3, 4])]);
        assert_eq!(rgba_pixels(&[0, 1, 2], &colors),
            vec![0, 0, 0, 0, 1, 2, 3, 4, 0, 0, 0, 0]);
    }
}
