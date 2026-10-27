//! Carrier-based squadrons consume real aircraft stock and deck slots.
//! Aircraft losses/sortie timing are provisional where HOI4 data defines are
//! unavailable; there is no free on-deck aircraft creation.
use crate::{
    air_warfare::is_aircraft, military_model::EquipmentDefinition, naval_warfare::NavalState,
    production::ProductionState, warfare::Warfare, world::World,
};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarrierAirGroup {
    pub model: String,
    pub planes: u32,
    pub target: u32,
    pub sorties: u64,
    pub losses: u64,
    pub last_launch: Option<u64>,
    pub loss_debt: f64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AircraftRole {
    Fighter,
    NavalBomber,
    Support,
}
pub fn role(def: &EquipmentDefinition) -> Option<AircraftRole> {
    if !is_aircraft(def) {
        return None;
    }
    let key = format!("{} {}", def.id, def.archetype).to_lowercase();
    if !key.contains("cv_")
        && !key.contains("carrier")
        && def.stats.get("carrier_capable").copied().unwrap_or(0.) <= 0.
    {
        return None;
    }
    if key.contains("fighter") {
        Some(AircraftRole::Fighter)
    } else if key.contains("nav_bomber")
        || key.contains("naval_bomber")
        || def.stats.get("naval_strike_attack").copied().unwrap_or(0.) > 0.
    {
        Some(AircraftRole::NavalBomber)
    } else {
        Some(AircraftRole::Support)
    }
}
#[derive(Debug, Clone)]
pub struct CarrierAirSortie {
    pub target_force: u64,
    pub target_index: usize,
    pub relative_damage: f64,
    pub aircraft_losses: u32,
    pub aircraft_launched: u32,
}
/// Read the actual script keys used by HOI4 rather than silently ignoring
/// NAIR defines. Defaults remain explicit approximations if game data is absent.
fn carrier_air_parameters(w: &Warfare) -> (u64, f64, f64) {
    let defs = &w.catalogue.literal_defines;
    let get = |keys: &[&str], fallback: f64, allow_zero: bool| {
        keys.iter()
            .find_map(|k| {
                defs.get(*k)
                    .copied()
                    .filter(|v| v.is_finite() && (*v > 0.0 || (allow_zero && *v == 0.0)))
            })
            .unwrap_or(fallback)
    };
    let delay = get(
        &[
            "NDefines.NAir.CARRIER_HOURS_DELAY_AFTER_EACH_COMBAT",
            "NDefines.NNavy.CARRIER_HOURS_DELAY_AFTER_EACH_COMBAT",
            "NDefines.NNavy.CARRIER_HOURS_DELAY",
        ],
        4.0,
        false,
    )
    .ceil()
    .max(1.0) as u64;
    let sortie_fraction = get(
        &["NDefines.NNavy.BASE_CARRIER_SORTIE_EFFICIENCY"],
        0.2,
        true,
    )
    .clamp(0.0, 1.0);
    let damage_scale = get(
        &[
            "NDefines.NAir.NAVAL_STRIKE_DAMAGE_TO_STR",
            "NDefines.NNavy.NAVAL_STRIKE_DAMAGE_TO_STR",
        ],
        0.015,
        true,
    );
    (delay, sortie_fraction, damage_scale)
}
impl NavalState {
    pub fn carrier_capacity(&self, tf: u64, index: usize, w: &Warfare) -> u32 {
        let Some(ship) = self.task_forces.get(&tf).and_then(|f| f.ships.get(index)) else {
            return 0;
        };
        let Some(def) = w.catalogue.equipment.get(&ship.model) else {
            return 0;
        };
        if !format!("{} {}", def.id, def.archetype)
            .to_lowercase()
            .contains("carrier")
            || ship.strength <= 0.
        {
            return 0;
        }
        self.ship_stats(ship, w)
            .and_then(|s| s.get("deck_size"))
            .copied()
            .unwrap_or(0.)
            .max(0.)
            .floor()
            .min(u32::MAX as f64) as u32
    }
    pub fn assign_carrier_aircraft(
        &mut self,
        tag: &str,
        tf: u64,
        index: usize,
        model: &str,
        target: u32,
        w: &Warfare,
    ) -> Result<(), String> {
        let force = self.task_forces.get(&tf).ok_or("Неизвестный флот")?;
        if force.country != tag {
            return Err("Нельзя назначить чужие самолёты".into());
        }
        let capacity = self.carrier_capacity(tf, index, w);
        if capacity == 0 {
            return Err("Корабль не имеет действующей палубы".into());
        }
        let aircraft = w
            .catalogue
            .equipment
            .get(model)
            .ok_or("Модель самолёта отсутствует")?;
        if role(aircraft).is_none() {
            return Err("Самолёт не может базироваться на авианосце".into());
        }
        let country = &w.countries.get(tag).ok_or("Страна не найдена")?.modifiers;
        if !w.catalogue.equipment_available(model, country) {
            return Err("Модель палубного самолёта ещё не исследована".into());
        }
        if target == 0 {
            return Err("Размер авиагруппы не может быть нулём".into());
        }
        let key = (tf, index);
        let others = self.carrier_wings.get(&key).map_or(0_u32, |g| {
            g.iter()
                .filter(|g| g.model != model)
                .map(|g| g.target)
                .sum()
        });
        if target > capacity.saturating_sub(others) {
            return Err("На палубе недостаточно мест".into());
        }
        let groups = self.carrier_wings.entry(key).or_default();
        if let Some(group) = groups.iter_mut().find(|g| g.model == model) {
            group.target = target;
        } else {
            groups.push(CarrierAirGroup {
                model: model.into(),
                planes: 0,
                target,
                sorties: 0,
                losses: 0,
                last_launch: None,
                loss_debt: 0.,
            });
        }
        Ok(())
    }
    /// Replenishment only happens when the carrier is in a friendly naval
    /// base with an idle order. Prevents spawning aircraft at sea.
    pub fn replenish_carriers(
        &mut self,
        world: &World,
        w: &Warfare,
        production: &mut ProductionState,
    ) {
        self.reconcile_sunk_carriers();
        for (&(id, index), groups) in &mut self.carrier_wings {
            let Some(force) = self.task_forces.get(&id) else {
                continue;
            };
            if force.mission != crate::naval_warfare::NavalMission::Hold
                || world.controller_of_province(force.base) != Some(force.country.as_str())
                || w.conditions
                    .get(&force.base)
                    .is_none_or(|c| c.naval_base == 0)
                || force.ships.get(index).is_none_or(|s| s.strength <= 0.)
            {
                continue;
            }
            let stock = production.country_mut(&force.country);
            for group in groups {
                let available = stock
                    .model_stockpile
                    .entry(group.model.clone())
                    .or_default();
                let requested = group.target.saturating_sub(group.planes) as u64;
                let delivered = requested.min(*available);
                *available -= delivered;
                group.planes = group.planes.saturating_add(delivered as u32);
            }
        }
    }
    /// Air groups on a destroyed carrier cannot keep defending other ships or
    /// return to production inventory. Preserve cumulative losses for save data.
    pub fn reconcile_sunk_carriers(&mut self) {
        for (&(force_id, index), groups) in &mut self.carrier_wings {
            let sunk = self
                .task_forces
                .get(&force_id)
                .and_then(|f| f.ships.get(index))
                .is_none_or(|ship| ship.strength <= 0.0);
            if sunk {
                for group in groups {
                    group.losses = group.losses.saturating_add(u64::from(group.planes));
                    group.planes = 0;
                    group.target = 0;
                }
            }
        }
    }
    /// Combat integrates this as part of the same hour-start damage snapshot.
    /// No target reference or weapon type information escapes the naval model.
    pub fn carrier_air_strike(
        &mut self,
        src: u64,
        dst: u64,
        w: &Warfare,
        hour: u64,
    ) -> Vec<CarrierAirSortie> {
        let Some(enemy) = self.task_forces.get(&dst) else {
            return Vec::new();
        };
        let enemy_ships = enemy
            .ships
            .iter()
            .enumerate()
            .filter(|(_, s)| s.strength > 0.)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        if enemy_ships.is_empty() {
            return Vec::new();
        }
        let Some(source) = self.task_forces.get(&src) else {
            return Vec::new();
        };
        let source_tag = source.country.clone();
        let mut attacks = Vec::new();
        let (delay, base_sortie_fraction, air_damage) = carrier_air_parameters(w);
        let sortie_modifier = w.countries.get(&source_tag).map_or(0., |c| {
            c.modifiers
                .country
                .get("sortie_efficiency")
                .copied()
                .unwrap_or(0.)
        });
        let efficiency = (base_sortie_fraction * (1.0 + sortie_modifier).max(0.0)).clamp(0.0, 1.0);
        let enemy_fighters = self
            .carrier_wings
            .iter()
            .filter(|((id, index), _)| {
                *id == dst
                    && enemy
                        .ships
                        .get(*index)
                        .is_some_and(|ship| ship.strength > 0.0)
            })
            .flat_map(|(_, gs)| gs.iter())
            .filter_map(|g| {
                w.catalogue.equipment.get(&g.model).and_then(|m| {
                    (role(m) == Some(AircraftRole::Fighter)).then_some(g.planes as f64)
                })
            })
            .sum::<f64>();
        let enemy_aa = enemy
            .ships
            .iter()
            .filter_map(|s| self.ship_stats(s, w))
            .map(|v| {
                v.get("anti_air_attack")
                    .or_else(|| v.get("air_attack"))
                    .copied()
                    .unwrap_or(0.)
            })
            .sum::<f64>();
        let enemy_hp: Vec<f64> = enemy
            .ships
            .iter()
            .map(|ship| {
                self.ship_stats(ship, w)
                    .and_then(|s| s.get("max_strength"))
                    .copied()
                    .unwrap_or(100.)
                    .max(1.)
            })
            .collect();
        for (&(force_id, ship_idx), groups) in &mut self.carrier_wings {
            if force_id != src {
                continue;
            }
            let has_deck = source.ships.get(ship_idx).is_some_and(|s| s.strength > 0.);
            if !has_deck {
                continue;
            }
            for group in groups {
                if group.planes == 0
                    || group
                        .last_launch
                        .is_some_and(|h| hour.saturating_sub(h) < delay)
                {
                    continue;
                }
                let Some(model) = w.catalogue.equipment.get(&group.model) else {
                    continue;
                };
                let Some(r) = role(model) else { continue };
                group.last_launch = Some(hour);
                if r != AircraftRole::NavalBomber {
                    continue;
                }
                let launch =
                    (((group.planes as f64) * efficiency).round() as u32).min(group.planes);
                if launch == 0 {
                    continue;
                }
                group.sorties += u64::from(launch);
                let target_idx = enemy_ships[(hour as usize + ship_idx) % enemy_ships.len()];
                let attack = model
                    .stats
                    .get("naval_strike_attack")
                    .copied()
                    .unwrap_or(0.)
                    .max(0.);
                let damage = attack * launch as f64 * air_damage / enemy_hp[target_idx];
                // Carrier fighters and AA both reduce attacking aircraft.
                let loss_ratio = (enemy_fighters * 0.0002 + enemy_aa * 0.0005).clamp(0., 0.5);
                group.loss_debt += launch as f64 * loss_ratio;
                let losses = (group.loss_debt.floor() as u32).min(group.planes);
                group.loss_debt -= losses as f64;
                group.planes -= losses;
                group.losses += u64::from(losses);
                attacks.push(CarrierAirSortie {
                    target_force: dst,
                    target_index: target_idx,
                    relative_damage: damage,
                    aircraft_losses: losses,
                    aircraft_launched: launch,
                });
            }
        }
        attacks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::military_model::{CountryArmy, EquipmentDefinition, MilitaryCatalogue};
    use crate::naval_warfare::{NavalMission, NavalShip, NavalTaskForce};
    use crate::warfare::ProvinceConditions;
    use std::collections::{BTreeMap, BTreeSet, HashMap};
    fn test_world() -> World {
        World::from_map_data(
            &HashMap::from([(1, 1)]),
            &HashMap::from([(1, "A".to_owned())]),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        )
    }
    fn fixture() -> (Warfare, NavalState) {
        let mut catalogue = MilitaryCatalogue::default();
        for (id, arch, stats) in [
            (
                "ship_hull_carrier_1",
                "ship_hull_carrier",
                BTreeMap::from([("deck_size".into(), 10.), ("max_strength".into(), 200.)]),
            ),
            (
                "ship_hull_light_1",
                "ship_hull_light",
                BTreeMap::from([("max_strength".into(), 100.)]),
            ),
            (
                "cv_nav_bomber_1",
                "cv_nav_bomber_equipment",
                BTreeMap::from([
                    ("air_range".into(), 150.),
                    ("naval_strike_attack".into(), 10.),
                ]),
            ),
            (
                "fighter_1",
                "fighter_equipment",
                BTreeMap::from([("air_range".into(), 100.), ("air_attack".into(), 5.)]),
            ),
        ] {
            catalogue.equipment.insert(
                id.into(),
                EquipmentDefinition {
                    id: id.into(),
                    archetype: arch.into(),
                    year: 1936,
                    active: true,
                    buildable: true,
                    stats,
                    cost: 20.,
                    modular: false,
                    source: "fixture".into(),
                },
            );
        }
        // Explicit full-sortie fixture: separate tests check lower rates.
        catalogue
            .literal_defines
            .insert("NDefines.NNavy.BASE_CARRIER_SORTIE_EFFICIENCY".into(), 1.0);
        let mut w = Warfare::new(catalogue);
        w.countries.insert("A".into(), CountryArmy::default());
        w.countries.insert("B".into(), CountryArmy::default());
        w.conditions.insert(
            1,
            ProvinceConditions {
                naval_base: 2,
                ..Default::default()
            },
        );
        let mut navy = NavalState::default();
        for (id, tag, model) in [
            (1, "A", "ship_hull_carrier_1"),
            (2, "B", "ship_hull_light_1"),
        ] {
            navy.task_forces.insert(
                id,
                NavalTaskForce {
                    id,
                    country: tag.into(),
                    name: "Fleet".into(),
                    base: 1,
                    mission: NavalMission::Hold,
                    regions: BTreeSet::new(),
                    ships: vec![NavalShip {
                        name: model.into(),
                        model: model.into(),
                        definition: model.into(),
                        variant: None,
                        strength: 1.0,
                    }],
                },
            );
        }
        (w, navy)
    }
    #[test]
    fn deck_size_and_research_gate_carrier_aircraft() {
        let (mut w, mut navy) = fixture();
        assert_eq!(navy.carrier_capacity(1, 0, &w), 10);
        assert!(
            navy.assign_carrier_aircraft("A", 1, 0, "fighter_1", 5, &w)
                .is_err()
        );
        assert!(
            navy.assign_carrier_aircraft("B", 1, 0, "cv_nav_bomber_1", 5, &w)
                .is_err()
        );
        assert!(
            navy.assign_carrier_aircraft("A", 1, 0, "cv_nav_bomber_1", 11, &w)
                .is_err()
        );
        w.catalogue = std::sync::Arc::new({
            let mut cat = (*w.catalogue).clone();
            cat.equipment.get_mut("cv_nav_bomber_1").unwrap().active = false;
            cat
        });
        assert!(
            navy.assign_carrier_aircraft("A", 1, 0, "cv_nav_bomber_1", 5, &w)
                .is_err()
        );
        w.countries
            .get_mut("A")
            .unwrap()
            .modifiers
            .unlocked_equipment
            .insert("cv_nav_bomber_1".into());
        navy.assign_carrier_aircraft("A", 1, 0, "cv_nav_bomber_1", 5, &w)
            .unwrap();
        assert_eq!(navy.carrier_wings[&(1, 0)][0].planes, 0);
    }
    #[test]
    fn imported_air_defines_override_the_old_nonexistent_names() {
        let (mut w, mut navy) = fixture();
        {
            let cat = std::sync::Arc::make_mut(&mut w.catalogue);
            cat.literal_defines.insert(
                "NDefines.NAir.CARRIER_HOURS_DELAY_AFTER_EACH_COMBAT".into(),
                6.0,
            );
            cat.literal_defines
                .insert("NDefines.NAir.NAVAL_STRIKE_DAMAGE_TO_STR".into(), 0.05);
            cat.literal_defines
                .insert("NDefines.NNavy.BASE_CARRIER_SORTIE_EFFICIENCY".into(), 0.4);
        }
        let (delay, portion, damage) = carrier_air_parameters(&w);
        assert_eq!(delay, 6);
        assert_eq!(portion, 0.4);
        assert_eq!(damage, 0.05);
        // Zeroed values in a mod must disable sorties or damage rather than
        // accidentally restoring fallback values.
        {
            let cat = std::sync::Arc::make_mut(&mut w.catalogue);
            cat.literal_defines
                .insert("NDefines.NNavy.BASE_CARRIER_SORTIE_EFFICIENCY".into(), 0.0);
            cat.literal_defines
                .insert("NDefines.NAir.NAVAL_STRIKE_DAMAGE_TO_STR".into(), 0.0);
        }
        assert_eq!(carrier_air_parameters(&w).1, 0.0);
        assert_eq!(carrier_air_parameters(&w).2, 0.0);
        {
            let cat = std::sync::Arc::make_mut(&mut w.catalogue);
            cat.literal_defines
                .insert("NDefines.NNavy.BASE_CARRIER_SORTIE_EFFICIENCY".into(), 0.4);
            cat.literal_defines
                .insert("NDefines.NAir.NAVAL_STRIKE_DAMAGE_TO_STR".into(), 0.05);
        }
        navy.assign_carrier_aircraft("A", 1, 0, "cv_nav_bomber_1", 10, &w)
            .unwrap();
        navy.carrier_wings.get_mut(&(1, 0)).unwrap()[0].planes = 10;
        let strikes = navy.carrier_air_strike(1, 2, &w, 6);
        assert_eq!(strikes[0].aircraft_launched, 4);
        assert_eq!(navy.carrier_air_strike(1, 2, &w, 11).len(), 0);
        assert_eq!(navy.carrier_air_strike(1, 2, &w, 12).len(), 1);
    }

    #[test]
    fn occupied_port_cannot_refill_and_sunk_carrier_cannot_keep_aircraft() {
        let (w, mut navy) = fixture();
        let mut world = test_world();
        navy.assign_carrier_aircraft("A", 1, 0, "cv_nav_bomber_1", 6, &w)
            .unwrap();
        let mut prod = ProductionState::default();
        prod.country_mut("A")
            .model_stockpile
            .insert("cv_nav_bomber_1".into(), 6);
        assert!(world.set_province_controller(1, Some("B")));
        navy.replenish_carriers(&world, &w, &mut prod);
        assert_eq!(navy.carrier_wings[&(1, 0)][0].planes, 0);
        assert_eq!(
            prod.country("A").unwrap().model_stockpile["cv_nav_bomber_1"],
            6
        );
        assert!(world.set_province_controller(1, Some("A")));
        navy.replenish_carriers(&world, &w, &mut prod);
        assert_eq!(navy.carrier_wings[&(1, 0)][0].planes, 6);
        navy.task_forces.get_mut(&1).unwrap().ships[0].strength = 0.0;
        navy.reconcile_sunk_carriers();
        let group = &navy.carrier_wings[&(1, 0)][0];
        assert_eq!(group.planes, 0);
        assert_eq!(group.target, 0);
        assert_eq!(group.losses, 6);
        assert!(navy.carrier_air_strike(1, 2, &w, 12).is_empty());
        let saved = serde_json::to_string(&navy).unwrap();
        let loaded: NavalState = serde_json::from_str(&saved).unwrap();
        assert_eq!(loaded.carrier_wings[&(1, 0)][0].losses, 6);
    }

    #[test]
    fn inventory_funded_airgroups_launch_only_when_present_and_respect_cooldowns() {
        let (w, mut navy) = fixture();
        let world = test_world();
        navy.assign_carrier_aircraft("A", 1, 0, "cv_nav_bomber_1", 5, &w)
            .unwrap();
        let mut prod = ProductionState::default();
        navy.replenish_carriers(&world, &w, &mut prod);
        assert_eq!(navy.carrier_wings[&(1, 0)][0].planes, 0);
        prod.country_mut("A")
            .model_stockpile
            .insert("cv_nav_bomber_1".into(), 3);
        navy.replenish_carriers(&world, &w, &mut prod);
        assert_eq!(navy.carrier_wings[&(1, 0)][0].planes, 3);
        assert_eq!(
            prod.country("A").unwrap().model_stockpile["cv_nav_bomber_1"],
            0
        );
        let strike = navy.carrier_air_strike(1, 2, &w, 8);
        assert_eq!(strike.len(), 1);
        assert!(strike[0].relative_damage > 0.);
        assert_eq!(strike[0].aircraft_launched, 3);
        assert!(navy.carrier_air_strike(1, 2, &w, 9).is_empty());
        assert_eq!(navy.carrier_air_strike(1, 2, &w, 16).len(), 1);
        navy.task_forces.get_mut(&1).unwrap().mission = NavalMission::Patrol;
        prod.country_mut("A")
            .model_stockpile
            .insert("cv_nav_bomber_1".into(), 10);
        navy.replenish_carriers(&world, &w, &mut prod);
        assert_eq!(navy.carrier_wings[&(1, 0)][0].planes, 3);
        assert_eq!(
            prod.country("A").unwrap().model_stockpile["cv_nav_bomber_1"],
            10
        );
        let json = serde_json::to_string(&navy).unwrap();
        let loaded: NavalState = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.carrier_wings[&(1, 0)][0].sorties, 6);
    }
}
