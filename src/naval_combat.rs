//! Naval surface-action combat in HOI4-style hourly phases.
//! Source naval defines and imported ship stats take precedence over defaults.
//! Detection/contact progress, target choice and some damage curves are still
//! game-prototype approximations, not proven engine 1:1 equivalence.
use crate::{
    military_model::EquipmentDefinition,
    naval_warfare::{NavalMission, NavalState},
    warfare::Warfare,
    world::World,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Line {
    Screen,
    Capital,
    Carrier,
    Submarine,
}
fn line(ship: &EquipmentDefinition) -> Line {
    let ty = format!("{} {}", ship.archetype, ship.id).to_lowercase();
    if ty.contains("submarine") {
        Line::Submarine
    } else if ty.contains("carrier") {
        Line::Carrier
    } else if ty.contains("battleship")
        || ty.contains("battlecruiser")
        || ty.contains("heavy_cruiser")
        || ty.contains("heavy_ship")
        || ty.contains("ship_hull_heavy")
        || ty.contains("ship_hull_cruiser")
            && ship.stats.get("heavy_attack").copied().unwrap_or(0.) > 1.
    {
        Line::Capital
    } else {
        Line::Screen
    }
}
fn stat(def: &EquipmentDefinition, key: &str) -> f64 {
    def.stats
        .get(key)
        .copied()
        .filter(|v| v.is_finite())
        .unwrap_or(0.)
        .max(0.)
}
fn ship_strength(nav: &NavalState, id: u64, idx: usize) -> f64 {
    nav.task_forces
        .get(&id)
        .and_then(|tf| tf.ships.get(idx))
        .map_or(0., |ship| ship.strength.max(0.))
}
fn dice(seed: u64) -> f64 {
    // Stable per event; does not depend on UI FPS or hash-map random seeds.
    let mut z = seed.wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    let z = z ^ (z >> 31);
    (z >> 11) as f64 / ((1_u64 << 53) as f64)
}
fn battle_key(a: u64, b: u64, region: u32) -> (u32, u64, u64) {
    (region, a.min(b), a.max(b))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipCondition {
    /// Remaining organisation, 0-1.
    pub organisation: f64,
    /// A critical-hit impairment that disappears through naval-base repair.
    pub critical_damage: f64,
}
impl Default for ShipCondition {
    fn default() -> Self {
        Self {
            organisation: 1.0,
            critical_damage: 0.0,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavalBattleReport {
    pub hour: u64,
    pub region: u32,
    pub attacker: String,
    pub defender: String,
    pub attacker_force: u64,
    pub defender_force: u64,
    pub screening_attacker: f64,
    pub screening_defender: f64,
    pub hits: u32,
    pub sunk: Vec<String>,
    pub damage_dealt: f64,
}
#[derive(Debug, Clone, Copy)]
pub struct NavalCombatRules {
    pub base_hit: f64,
    pub min_hit: f64,
    pub screen_ratio: f64,
    pub carrier_ratio: f64,
    pub torpedo_interval: u64,
    pub spotting_per_hour: f64,
    pub repair_per_dockyard_hour: f64,
    pub critical_hit_chance: f64,
}
impl NavalCombatRules {
    pub fn from_defines(def: &BTreeMap<String, f64>) -> Self {
        let get = |key: &str, fallback: f64| {
            def.get(&format!("NDefines.NNavy.{key}"))
                .copied()
                .filter(|v| v.is_finite() && *v >= 0.)
                .unwrap_or(fallback)
        };
        Self {
            base_hit: get("COMBAT_BASE_HIT_CHANCE", 0.1).clamp(0., 1.),
            min_hit: get("COMBAT_MIN_HIT_CHANCE", 0.05).clamp(0., 1.),
            screen_ratio: get("SCREEN_RATIO_FOR_FULL_SCREENING_FOR_CAPITALS", 3.).max(0.1),
            carrier_ratio: get("CAPITAL_RATIO_FOR_FULL_SCREENING_FOR_CARRIERS", 1.).max(0.1),
            torpedo_interval: get("TORPEDO_ATTACK_INTERVAL", 4.).max(1.) as u64,
            spotting_per_hour: get("DETECTION_CHANCE_MULT_BASE", 0.1),
            repair_per_dockyard_hour: get("NAVAL_REPAIR_SPEED_FACTOR", 0.004),
            critical_hit_chance: get("BASE_CRITICAL_CHANCE", 0.05).clamp(0., 1.),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Target {
    task_force: u64,
    index: usize,
    group: Line,
    visibility: f64,
    speed: f64,
    armor: f64,
    max_hp: f64,
}
fn screen_ratio(nav: &NavalState, tf: u64, warfare: &Warfare, rules: NavalCombatRules) -> f64 {
    let Some(force) = nav.task_forces.get(&tf) else {
        return 0.;
    };
    let mut screens = 0.;
    let mut capitals = 0.;
    let mut carriers = 0.;
    for ship in &force.ships {
        if ship.strength <= 0. {
            continue;
        }
        let Some(def) = warfare.catalogue.equipment.get(&ship.model) else {
            continue;
        };
        match line(def) {
            Line::Screen => screens += ship.strength,
            Line::Capital => capitals += ship.strength,
            Line::Carrier => carriers += ship.strength,
            Line::Submarine => {}
        }
    }
    if capitals + carriers == 0. {
        return 1.;
    }
    let outer = (screens / (rules.screen_ratio * (capitals + carriers))).min(1.);
    let inner = if carriers > 0. {
        (capitals / (rules.carrier_ratio * carriers)).min(1.)
    } else {
        1.
    };
    (outer * inner).clamp(0., 1.)
}
fn targets(nav: &NavalState, tf: u64, w: &Warfare) -> Vec<Target> {
    let mut result = Vec::new();
    let Some(force) = nav.task_forces.get(&tf) else {
        return result;
    };
    for (idx, ship) in force.ships.iter().enumerate() {
        if ship.strength <= 0. {
            continue;
        }
        let Some(def) = w.catalogue.equipment.get(&ship.model) else {
            continue;
        };
        let org = nav
            .conditions
            .get(&(tf, idx))
            .map_or(1., |s| s.organisation);
        if org <= 0.05 {
            continue;
        }
        result.push(Target {
            task_force: tf,
            index: idx,
            group: line(def),
            visibility: stat(def, "surface_visibility")
                .max(stat(def, "sub_visibility"))
                .max(1.),
            speed: stat(def, "maximum_speed"),
            armor: stat(def, "armor_value"),
            max_hp: stat(def, "max_strength").max(1.),
        });
    }
    result
}
fn pick_target<'a>(list: &'a [Target], class: u8, screened: f64, roll: f64) -> Option<&'a Target> {
    let allowed: Vec<&Target> = list
        .iter()
        .filter(|t| match class {
            0 => t.group == Line::Screen,
            1 => t.group == Line::Capital || t.group == Line::Carrier,
            2 => t.group == Line::Submarine,
            // Torpedo bypass chance improves when targets are unscreened.
            _ => t.group != Line::Submarine && (t.group == Line::Screen || roll >= screened),
        })
        .collect();
    if allowed.is_empty() {
        if class == 0 || class == 3 {
            return list
                .iter()
                .find(|t| t.group == Line::Capital || t.group == Line::Carrier);
        }
        return None;
    }
    allowed
        .get(((roll * allowed.len() as f64) as usize).min(allowed.len() - 1))
        .copied()
}
fn buff(w: &Warfare, tag: &str, key: &str) -> f64 {
    w.countries
        .get(tag)
        .and_then(|c| c.modifiers.country.get(key))
        .copied()
        .filter(|v| v.is_finite())
        .unwrap_or(0.)
}
#[derive(Clone, Copy)]
struct Hit {
    target: (u64, usize),
    damage: f64,
    org: f64,
    critical: bool,
}
impl NavalState {
    /// Hourly patrol contact, combat and repair. Stable force/ship ordering
    /// and snapshot-based simultaneous damage prevent side-order exploits.
    pub fn tick_naval_combat(&mut self, world: &World, warfare: &mut Warfare, hour: u64) {
        let rules = NavalCombatRules::from_defines(&warfare.catalogue.literal_defines);
        self.hourly_repairs(world, warfare, rules);
        // Cached contacts are invalidated when an order changes, a force
        // disappears or neither opponent still operates in the same region.
        self.spotting.retain(|&(region, a, b), _| {
            self.task_forces.get(&a).is_some_and(|f| f.regions.contains(&region) && f.mission != NavalMission::Hold)
            && self.task_forces.get(&b).is_some_and(|f| f.regions.contains(&region) && f.mission != NavalMission::Hold)
        });
        let mut active = Vec::new();
        let mut fuel_efficiency = BTreeMap::new();
        // Fuel comes from the same national stockpile as air and ground units.
        for tf in self.task_forces.values() {
            if tf.mission == NavalMission::Hold
                || world.controller_of_province(tf.base) != Some(tf.country.as_str())
            {
                continue;
            }
            let demand: f64 = tf
                .ships
                .iter()
                .filter(|s| s.strength > 0.)
                .filter_map(|s| {
                    warfare
                        .catalogue
                        .equipment
                        .get(&s.model)
                        .map(|def| stat(def, "fuel_consumption") * s.strength)
                })
                .sum::<f64>()
                .max(0.)
                / 24.;
            let budget = warfare
                .logistics
                .countries
                .entry(tf.country.clone())
                .or_default();
            let used = demand.min(budget.fuel.max(0.));
            budget.fuel = (budget.fuel - used).max(0.);
            budget.fuel_used_last_hour += used;
            let fuel = if demand > 0. { used / demand } else { 1. };
            fuel_efficiency.insert(tf.id, fuel);
            if fuel > 0. || demand == 0. {
                active.push(tf.id);
            }
        }
        let mut combat_events = Vec::new();
        // All hostile task-force pairs share a persistent spotting progress.
        for (ai, &a) in active.iter().enumerate() {
            for &b in active.iter().skip(ai + 1) {
                let (Some(ours), Some(theirs)) =
                    (self.task_forces.get(&a), self.task_forces.get(&b))
                else {
                    continue;
                };
                if !warfare.at_war(&ours.country, &theirs.country) {
                    continue;
                }
                let Some(region) = ours.regions.intersection(&theirs.regions).copied().next()
                else {
                    continue;
                };
                let a_eligible = matches!(
                    ours.mission,
                    NavalMission::Patrol
                        | NavalMission::ConvoyRaiding
                        | NavalMission::ConvoyEscort
                        | NavalMission::StrikeForce
                );
                let b_eligible = matches!(
                    theirs.mission,
                    NavalMission::Patrol
                        | NavalMission::ConvoyRaiding
                        | NavalMission::ConvoyEscort
                        | NavalMission::StrikeForce
                );
                if !a_eligible || !b_eligible {
                    continue;
                }
                if targets(self, a, warfare).is_empty() || targets(self, b, warfare).is_empty() {
                    continue;
                }
                let key = battle_key(a, b, region);
                let strength = |tf: u64| {
                    self.task_forces[&tf]
                        .ships
                        .iter()
                        .filter_map(|s| {
                            warfare.catalogue.equipment.get(&s.model).map(|d| {
                                (stat(d, "surface_detection") + stat(d, "sub_detection"))
                                    * s.strength
                            })
                        })
                        .sum::<f64>()
                };
                let admiral_spotting = self.admiral_for(a).map_or(0., |v| f64::from(v.maneuvering) * 0.02)
                    + self.admiral_for(b).map_or(0., |v| f64::from(v.maneuvering) * 0.02);
                let doctrine_spotting = buff(warfare, &ours.country, "naval_detection")
                    + buff(warfare, &theirs.country, "naval_detection");
                let speed = (strength(a) + strength(b)).max(1.) * rules.spotting_per_hour
                    * (1. + admiral_spotting + doctrine_spotting).max(0.) / 100.;
                let progress = self.spotting.entry(key).or_default();
                *progress =
                    (*progress + speed * fuel_efficiency[&a].min(fuel_efficiency[&b])).min(1.);
                if *progress < 1. {
                    continue;
                }
                // A given pair engages at most every 4 hours; contact remains
                // until ships retreat or orders change.
                if self
                    .last_contact_battle
                    .get(&key)
                    .is_some_and(|last| hour.saturating_sub(*last) < 4)
                {
                    continue;
                }
                combat_events.push((a, b, region));
                self.last_contact_battle.insert(key, hour);
            }
        }
        for (a, b, region) in combat_events {
            let report = self.resolve_naval_hour(
                warfare,
                a,
                b,
                region,
                hour,
                fuel_efficiency.get(&a).copied().unwrap_or(1.),
                fuel_efficiency.get(&b).copied().unwrap_or(1.),
                rules,
            );
            self.battle_reports.push(report);
            if self.battle_reports.len() > 64 {
                self.battle_reports.remove(0);
            }
        }
    }
    fn resolve_naval_hour(
        &mut self,
        w: &Warfare,
        a: u64,
        b: u64,
        region: u32,
        hour: u64,
        fuel_a: f64,
        fuel_b: f64,
        rules: NavalCombatRules,
    ) -> NavalBattleReport {
        let (ca, cb) = (
            self.task_forces[&a].country.clone(),
            self.task_forces[&b].country.clone(),
        );
        let (screen_a, screen_b) = (
            screen_ratio(self, a, w, rules),
            screen_ratio(self, b, w, rules),
        );
        let (ta, tb) = (targets(self, a, w), targets(self, b, w));
        let mut impacts = Vec::new();
        // Attackers are taken from the same hour-start snapshot; sinking a
        // ship mid-hour cannot prevent it from returning fire that hour.
        for &(src, dst, own, enemy, fuel, screen) in &[
            (a, b, &ta, &tb, fuel_a, screen_b),
            (b, a, &tb, &ta, fuel_b, screen_a),
        ] {
            let country = &self.task_forces[&src].country;
            for ship in own {
                let model = &self.task_forces[&src].ships[ship.index].model;
                let Some(def) = w.catalogue.equipment.get(model) else {
                    continue;
                };
                let impairment = self
                    .conditions
                    .get(&(src, ship.index))
                    .map_or(0., |c| c.critical_damage);
                let org = self
                    .conditions
                    .get(&(src, ship.index))
                    .map_or(1., |c| c.organisation);
                let base =
                    (ship_strength(self, src, ship.index) * org * (1. - impairment) * fuel).max(0.);
                for (weapon, category, power, profile, interval) in [
                    (0_u8, 0_u8, stat(def, "light_attack"), 45., 1_u64),
                    (1, 1, stat(def, "heavy_attack"), 70., 1),
                    (
                        2,
                        3,
                        stat(def, "torpedo_attack"),
                        100.,
                        rules.torpedo_interval,
                    ),
                    (3, 2, stat(def, "sub_attack"), 100., 1),
                ] {
                    if power <= 0. || base <= 0. || hour % interval != 0 {
                        continue;
                    }
                    let seed = hour.rotate_left(23)
                        ^ src.rotate_left(13)
                        ^ dst.rotate_left(31)
                        ^ (ship.index as u64).wrapping_mul(713)
                        ^ ((weapon as u64) << 48)
                        ^ (region as u64);
                    let bypass = dice(seed ^ 0xCAFEBABE);
                    let Some(target) = pick_target(enemy, category, screen, bypass) else {
                        continue;
                    };
                    // No depth-charges against surface hulls, no light guns against hidden subs.
                    if category == 2 && target.group != Line::Submarine {
                        continue;
                    }
                    let hit_profile = 100. * target.visibility / (target.speed * 0.5 + 20.);
                    let base_chance = rules.base_hit * (hit_profile / profile).min(1.);
                    // Navy doctrines and scenario admirals are already imported,
                // so their modifiers apply to every combat round.
                let admiral = self.admiral_for(src);
                let commander_hit = admiral.map_or(0., |v| f64::from(v.attack) * 0.025);
                let doctrine_hit = buff(w, country, "naval_hit_chance")
                    + buff(w, country, "naval_coordination");
                let bonus = (1. + commander_hit + doctrine_hit).max(0.);
                    let chance = (base_chance * bonus * base).clamp(rules.min_hit, 1.);
                    if dice(seed ^ 0xABCDEF) >= chance {
                        continue;
                    }
                    let pierce = if weapon == 2 {
                        stat(def, "torpedo_piercing")
                    } else {
                        stat(def, "ap_attack") + stat(def, "hg_armor_piercing")
                    };
                    let armor_effect = if target.armor > pierce && weapon != 2 {
                        0.5
                    } else {
                        1.
                    };
                    let critical = dice(seed ^ 0xBADC0DE) < rules.critical_hit_chance;
                    let damage = power * base * armor_effect * if critical { 1.5 } else { 1. };
                    impacts.push(Hit {
                        target: (target.task_force, target.index),
                        damage: damage / target.max_hp,
                        org: damage / target.max_hp * 1.5,
                        critical,
                    });
                }
            }
        }
        let mut summed = BTreeMap::<(u64, usize), (f64, f64, bool)>::new();
        for hit in &impacts {
            let v = summed.entry(hit.target).or_insert((0., 0., false));
            v.0 += hit.damage;
            v.1 += hit.org;
            v.2 |= hit.critical;
        }
        let mut sunk = Vec::new();
        let mut dealt = 0.;
        for ((tf, idx), (hp, org, critical)) in summed {
            let Some(ship) = self
                .task_forces
                .get_mut(&tf)
                .and_then(|f| f.ships.get_mut(idx))
            else {
                continue;
            };
            if ship.strength <= 0. {
                continue;
            }
            let actual = hp.min(ship.strength);
            ship.strength = (ship.strength - actual).max(0.);
            dealt += actual;
            let condition = self.conditions.entry((tf, idx)).or_default();
            condition.organisation = (condition.organisation - org).max(0.);
            if critical {
                condition.critical_damage = (condition.critical_damage + 0.1).min(0.75);
            }
            if ship.strength <= 0. {
                sunk.push(ship.name.clone());
            }
        }
        // A damaged task force is recalled instead of being magically
        // repaired while its mission is still active at sea.
        for id in [a,b] {
            let retreat = self.task_forces.get(&id).is_some_and(|force|
                force.ships.iter().enumerate().any(|(idx,ship)|
                    ship.strength > 0. && (ship.strength < 0.25
                        || self.conditions.get(&(id,idx)).is_some_and(|c| c.organisation < 0.10))));
            if retreat {
                if let Some(force) = self.task_forces.get_mut(&id) {
                    force.mission = NavalMission::Hold;
                    force.regions.clear();
                }
            }
        }
        NavalBattleReport {
            hour,
            region,
            attacker: ca,
            defender: cb,
            attacker_force: a,
            defender_force: b,
            screening_attacker: screen_a,
            screening_defender: screen_b,
            hits: impacts.len() as u32,
            sunk,
            damage_dealt: dealt,
        }
    }
    fn hourly_repairs(&mut self, world: &World, warfare: &Warfare, rules: NavalCombatRules) {
        for tf in self.task_forces.values_mut() {
            if world.controller_of_province(tf.base) != Some(tf.country.as_str()) {
                continue;
            }
            let level = warfare.conditions.get(&tf.base).map_or(0, |c| c.naval_base);
            if level == 0 {
                continue;
            }
            for (idx, ship) in tf.ships.iter_mut().enumerate() {
                if ship.strength <= 0. {
                    continue;
                } // Sunk ships never respawn.
                if tf.mission != NavalMission::Hold {
                    continue;
                }
                let repair = rules.repair_per_dockyard_hour * f64::from(level);
                ship.strength = (ship.strength + repair).min(1.);
                let condition = self.conditions.entry((tf.id, idx)).or_default();
                condition.critical_damage = (condition.critical_damage - repair).max(0.);
                condition.organisation = (condition.organisation + repair * 3.).min(1.);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        military_model::{CountryArmy, EquipmentDefinition},
        naval_warfare::{NavalShip, NavalTaskForce},
        warfare::ProvinceConditions,
    };
    use std::collections::HashMap;
    fn fixture() -> (World, Warfare, NavalState) {
        let world = World::from_map_data(
            &HashMap::from([(1, 1), (2, 2)]),
            &HashMap::from([(1, "A".into()), (2, "B".into())]),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );
        let mut w = Warfare::default();
        for tag in ["A", "B"] {
            w.countries.insert(tag.into(), CountryArmy::default());
        }
        w.wars.insert(("A".into(), "B".into()));
        w.conditions.insert(
            1,
            ProvinceConditions {
                naval_base: 3,
                ..Default::default()
            },
        );
        w.conditions.insert(
            2,
            ProvinceConditions {
                naval_base: 3,
                ..Default::default()
            },
        );
        w.air.regions.insert(99, BTreeSet::from([55]));
        let mut db = (*w.catalogue).clone();
        db.equipment.insert(
            "ship_hull_light_1".into(),
            EquipmentDefinition {
                id: "ship_hull_light_1".into(),
                archetype: "destroyer".into(),
                year: 1936,
                active: true,
                buildable: true,
                stats: BTreeMap::from([
                    ("surface_detection".into(), 50.),
                    ("maximum_speed".into(), 35.),
                    ("surface_visibility".into(), 10.),
                    ("max_strength".into(), 100.),
                    ("light_attack".into(), 25.),
                    ("fuel_consumption".into(), 24.),
                ]),
                cost: 100.,
                modular: false,
                source: "test".into(),
            },
        );
        w.catalogue = std::sync::Arc::new(db);
        let mut navy = NavalState::default();
        for (id, tag) in [(1, "A"), (2, "B")] {
            let f = NavalTaskForce {
                id,
                country: tag.into(),
                name: tag.into(),
                base: id as u32,
                mission: NavalMission::Patrol,
                regions: BTreeSet::from([99]),
                ships: vec![NavalShip {
                    name: tag.into(),
                    model: "ship_hull_light_1".into(),
                    definition: String::new(),
                    variant: None,
                    strength: 1.,
                }],
            };
            navy.task_forces.insert(id, f);
            w.logistics.countries.entry(tag.into()).or_default().fuel = 1000.;
        }
        (world, w, navy)
    }
    #[test]
    fn war_is_required_for_a_naval_encounter() {
        let (world, mut w, mut navy) = fixture();
        w.wars.clear();
        for h in 1..=15 {
            navy.tick_naval_combat(&world, &mut w, h);
        }
        assert!(navy.battle_reports.is_empty());
    }
    #[test]
    fn combat_uses_spotting_persists_damage_and_consumes_fuel() {
        let (world, mut w, mut navy) = fixture();
        for h in 1..=120 {
            navy.tick_naval_combat(&world, &mut w, h);
        }
        assert!(!navy.battle_reports.is_empty());
        assert!(w.logistics.countries["A"].fuel < 1000.);
        assert!(
            navy.task_forces[&1].ships[0].strength < 1.
                || navy.task_forces[&2].ships[0].strength < 1.
        );
    }
    #[test]
    fn repair_cannot_resurrect_sunk_ships_and_requires_friendly_port() {
        let (world, mut w, mut navy) = fixture();
        navy.task_forces.get_mut(&1).unwrap().mission = NavalMission::Hold;
        navy.task_forces.get_mut(&1).unwrap().ships[0].strength = 0.6;
        navy.task_forces.get_mut(&2).unwrap().mission = NavalMission::Hold;
        navy.task_forces.get_mut(&2).unwrap().ships[0].strength = 0.;
        for h in 1..=100 {
            navy.tick_naval_combat(&world, &mut w, h);
        }
        assert!(navy.task_forces[&1].ships[0].strength > 0.6);
        assert_eq!(navy.task_forces[&2].ships[0].strength, 0.);
    }
}
