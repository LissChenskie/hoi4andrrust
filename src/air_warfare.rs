//! Regional air missions feed the existing land resolver, never province input mocks.
//! Sortie and air-combat defaults are explicit mobile approximations.
use crate::{combat_rules::CombatRandom, land_combat::AirSupport, warfare::Warfare, world::World};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
/// AA guns also have air_attack. An air-only stat or an aircraft archetype
/// is required; attack alone must never turn ground equipment into planes.
pub fn is_aircraft(e: &crate::military_model::EquipmentDefinition) -> bool {
    ["air_range", "air_agility", "air_defence", "air_defense"]
        .iter()
        .any(|k| e.stats.contains_key(*k))
        || [
            "plane",
            "fighter",
            "bomber",
            "CAS_equipment",
            "cas_equipment",
            "transport_plane",
        ]
        .iter()
        .any(|kind| e.archetype.contains(kind))
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AirMission {
    #[default]
    Standby,
    Superiority,
    Interception,
    CloseAirSupport,
}
impl AirMission {
    pub fn label(self) -> &'static str {
        match self {
            Self::Standby => "Ожидание",
            Self::Superiority => "Превосходство",
            Self::Interception => "Перехват",
            Self::CloseAirSupport => "Поддержка войск",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AirRules {
    pub profile: String,
    pub mission_interval: u64,
    pub planes_per_base_level: u32,
    pub air_damage_scale: f64,
    pub accident_factor: f64,
    pub aa_loss_factor: f64,
    pub ground_support_bonus: f64,
    pub aa_effect_scale: f64,
}
impl Default for AirRules {
    fn default() -> Self {
        Self {
            profile: "mobile-air-v1-approximation".into(),
            mission_interval: 8,
            planes_per_base_level: 200,
            air_damage_scale: 0.05,
            accident_factor: 0.001,
            aa_loss_factor: 0.001,
            ground_support_bonus: 0.25,
            aa_effect_scale: 75.,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirWing {
    pub id: u64,
    pub name: String,
    pub country: String,
    pub model: String,
    pub base: u32,
    pub planes: u32,
    #[serde(default)]
    pub target_planes: u32,
    pub mission: AirMission,
    pub region: Option<u32>,
    #[serde(default)]
    pub sorties: u64,
    #[serde(default)]
    pub losses: u64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegionResult {
    pub hour: u64,
    pub sorties: u32,
    pub fighters: u32,
    pub cas: u32,
    pub ground_attack: f64,
    pub losses: u32,
    pub fuel_used: f64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AirState {
    pub enabled: bool,
    pub rules: AirRules,
    pub regions: BTreeMap<u32, BTreeSet<u32>>,
    pub bases: BTreeMap<u32, u16>,
    pub wings: BTreeMap<u64, AirWing>,
    pub next_id: u64,
    pub results: BTreeMap<u32, BTreeMap<String, RegionResult>>,
    pub last_hour: Option<u64>,
    #[serde(skip)]
    pub province_region: BTreeMap<u32, u32>,
}
impl AirState {
    pub fn create_wing(
        &mut self,
        tag: &str,
        model: &str,
        base: u32,
        world: &World,
        catalogue: &crate::military_model::MilitaryCatalogue,
    ) -> Result<u64, String> {
        if world.controller_of_province(base) != Some(tag)
            || self.bases.get(&base).copied().unwrap_or(0) == 0
        {
            return Err("Нужна контролируемая авиабаза".into());
        }
        let def = catalogue
            .equipment
            .get(model)
            .ok_or("Неизвестная модель самолёта")?;
        if !def.buildable || def.modular || !is_aircraft(def) {
            return Err("Модель самолёта не поддерживается".into());
        }
        self.next_id = self
            .next_id
            .max(self.wings.keys().copied().max().unwrap_or(0))
            .checked_add(1)
            .ok_or("Исчерпаны ID авиагрупп")?;
        let id = self.next_id;
        self.wings.insert(
            id,
            AirWing {
                id,
                name: format!("Авиагруппа {id}"),
                country: tag.into(),
                model: model.into(),
                base,
                planes: 0,
                target_planes: 100,
                mission: AirMission::Standby,
                region: None,
                sorties: 0,
                losses: 0,
            },
        );
        Ok(id)
    }
    pub fn replenish(
        &mut self,
        production: &mut crate::production::ProductionState,
        world: &World,
    ) {
        for a in self
            .wings
            .values_mut()
            .filter(|a| world.controller_of_province(a.base) == Some(a.country.as_str()))
        {
            let stock = production
                .country_mut(&a.country)
                .model_stockpile
                .entry(a.model.clone())
                .or_default();
            let n = u64::from(a.target_planes.saturating_sub(a.planes)).min(*stock);
            *stock -= n;
            a.planes += n as u32;
        }
    }
    pub fn validate(&self, world: &World, w: &Warfare) -> Result<(), String> {
        if self.rules.mission_interval == 0
            || self.rules.planes_per_base_level == 0
            || [
                self.rules.air_damage_scale,
                self.rules.accident_factor,
                self.rules.aa_loss_factor,
                self.rules.ground_support_bonus,
                self.rules.aa_effect_scale,
            ]
            .iter()
            .any(|v| !v.is_finite() || *v < 0.)
        {
            return Err("Invalid air rules".into());
        }
        for (&id, a) in &self.wings {
            if id != a.id
                || a.id == u64::MAX
                || a.planes > 1_000_000
                || a.target_planes > 1_000_000
                || !world.countries.contains_key(&a.country)
                || !world.provinces.contains_key(&a.base)
                || !w
                    .catalogue
                    .equipment
                    .get(&a.model)
                    .is_some_and(|e| is_aircraft(e) && !e.modular)
                || a.region.is_some_and(|r| !self.regions.contains_key(&r))
            {
                return Err(format!("Invalid air wing {id}"));
            }
        }
        Ok(())
    }
    pub fn reindex(&mut self) {
        self.province_region.clear();
        for (&r, ps) in &self.regions {
            for &p in ps {
                self.province_region.entry(p).or_insert(r);
            }
        }
    }
    pub fn region_at(&self, province: u32) -> Option<u32> {
        self.province_region.get(&province).copied()
    }
    pub fn assign(
        &mut self,
        tag: &str,
        id: u64,
        mission: AirMission,
        region: Option<u32>,
    ) -> Result<(), String> {
        if mission != AirMission::Standby && !region.is_some_and(|r| self.regions.contains_key(&r))
        {
            return Err("Выберите известный воздушный регион".into());
        }
        let a = self
            .wings
            .get_mut(&id)
            .filter(|a| a.country == tag)
            .ok_or("Чужая авиагруппа")?;
        a.mission = mission;
        a.region = region;
        Ok(())
    }
    pub fn support(&self, tag: &str, enemy: &str, province: u32, combat_hour: u64) -> AirSupport {
        let Some(r) = self.region_at(province).and_then(|r| self.results.get(&r)) else {
            return AirSupport::default();
        };
        let own = r
            .get(tag)
            .filter(|x| combat_hour.saturating_sub(x.hour) < self.rules.mission_interval);
        let enemy = r
            .get(enemy)
            .filter(|x| combat_hour.saturating_sub(x.hour) < self.rules.mission_interval);
        let ours = own.map_or(0., |x| f64::from(x.fighters));
        let theirs = enemy.map_or(0., |x| f64::from(x.fighters));
        let superiority = if ours + theirs > 0. {
            ((theirs - ours) / (ours + theirs)).max(0.)
        } else {
            0.
        };
        AirSupport {
            enemy_superiority: superiority,
            cas_planes: own.map_or(0, |x| x.cas),
            plane_attack_rolls: own.map_or(0., |x| x.ground_attack)
                / self.rules.mission_interval.max(1) as f64,
            ground_support_bonus: own
                .filter(|x| x.cas > 0)
                .map_or(0., |_| self.rules.ground_support_bonus),
            aa_damage_reduction: 0.,
        }
    }
    pub fn tick(&mut self, w: &mut Warfare, world: &World, hour: u64) {
        if !self.enabled || self.last_hour == Some(hour) {
            return;
        }
        self.last_hour = Some(hour);
        if !hour.is_multiple_of(self.rules.mission_interval.max(1)) {
            return;
        }
        self.results.clear();
        if self.province_region.is_empty() {
            self.reindex();
        }
        let mut stationed = BTreeMap::<u32, u32>::new();
        for a in self.wings.values() {
            let n = stationed.entry(a.base).or_default();
            *n = n.saturating_add(a.planes);
        }
        // Snapshot simultaneous air damage; a destroyed wing still fires this sortie.
        let mut launched = BTreeMap::<u64, u32>::new();
        for (&id, a) in &mut self.wings {
            let Some(region) = a.region.filter(|r| self.regions.contains_key(r)) else {
                continue;
            };
            if a.mission == AirMission::Standby
                || a.planes == 0
                || world.controller_of_province(a.base) != Some(a.country.as_str())
            {
                continue;
            }
            if w.logistics.enabled && !w.logistics.serviced.contains(&(a.country.clone(), a.base)) {
                continue;
            }
            if a.mission == AirMission::CloseAirSupport
                && !w.divisions.values().any(|d| {
                    !d.destroyed
                        && d.battle.is_some()
                        && w.at_war(&a.country, &d.country)
                        && self.province_region.get(&d.province) == Some(&region)
                })
            {
                continue;
            }
            let Some(model) = w.catalogue.equipment.get(&a.model) else {
                continue;
            };
            let ps = &self.regions[&region];
            let range = model.stats.get("air_range").copied().unwrap_or(0.);
            let in_range = ps.contains(&a.base)
                || w.catalogue.rules.pixels_to_km.is_some_and(|scale| {
                    ps.iter()
                        .filter_map(|&p| world.geography.distance_in_pixels(a.base, p))
                        .any(|d| d * scale <= range)
                });
            if !in_range {
                continue;
            }
            let capacity = u32::from(self.bases.get(&a.base).copied().unwrap_or(0))
                * self.rules.planes_per_base_level;
            let base_eff = (f64::from(capacity) / f64::from(stationed[&a.base].max(1))).min(1.);
            let weather = w
                .conditions
                .get(&a.base)
                .and_then(|c| c.weather.as_ref())
                .map_or(1., |id| {
                    w.catalogue
                        .weather
                        .get(id)
                        .and_then(|b| b.get("air_mission_efficiency"))
                        .map_or(1., |v| (1. + v).clamp(0., 1.))
                });
            let proposed = (f64::from(a.planes) * base_eff * weather).floor() as u32;
            let per_plane = model
                .stats
                .get("fuel_consumption")
                .copied()
                .unwrap_or(0.)
                .max(0.)
                * self.rules.mission_interval as f64
                / 24.;
            let country = w.logistics.countries.entry(a.country.clone()).or_default();
            let count = if per_plane > 0. {
                proposed.min((country.fuel / per_plane).floor() as u32)
            } else {
                proposed
            };
            let fuel = per_plane * f64::from(count);
            country.fuel = (country.fuel - fuel).max(0.);
            country.fuel_used_last_hour += fuel;
            if count == 0 {
                continue;
            }
            a.sorties += u64::from(count);
            launched.insert(id, count);
            let r = self
                .results
                .entry(region)
                .or_default()
                .entry(a.country.clone())
                .or_default();
            r.hour = hour;
            r.sorties += count;
            r.fuel_used += fuel;
            if a.mission == AirMission::Superiority || a.mission == AirMission::Interception {
                r.fighters += count;
            }
        }
        let mut damage = BTreeMap::<u64, f64>::new();
        for (&id, &count) in &launched {
            let a = &self.wings[&id];
            let model = &w.catalogue.equipment[&a.model];
            let reliability = model
                .stats
                .get("reliability")
                .copied()
                .unwrap_or(1.)
                .clamp(0., 1.);
            *damage.entry(id).or_default() +=
                f64::from(count) * (1. - reliability) * self.rules.accident_factor;
            if !matches!(
                a.mission,
                AirMission::Superiority | AirMission::Interception
            ) {
                continue;
            }
            let enemies: Vec<_> = launched
                .iter()
                .filter(|(other, _)| {
                    let b = &self.wings[other];
                    b.region == a.region
                        && w.at_war(&a.country, &b.country)
                        && (a.mission != AirMission::Interception
                            || b.mission == AirMission::CloseAirSupport)
                })
                .collect();
            let total: f64 = enemies.iter().map(|(_, n)| f64::from(**n)).sum();
            for (&other, &n) in enemies {
                let b = &self.wings[&other];
                let target = &w.catalogue.equipment[&b.model];
                let attack = model.stats.get("air_attack").copied().unwrap_or(0.).max(0.);
                let defense = target
                    .stats
                    .get("air_defence")
                    .or_else(|| target.stats.get("air_defense"))
                    .copied()
                    .unwrap_or(1.)
                    .max(1.);
                let agility = model
                    .stats
                    .get("air_agility")
                    .copied()
                    .unwrap_or(1.)
                    .max(1.);
                let enemy_agility = target
                    .stats
                    .get("air_agility")
                    .copied()
                    .unwrap_or(1.)
                    .max(1.);
                let avoidance =
                    (1. - 0.45 * (enemy_agility / agility - 1.).max(0.)).clamp(0.55, 1.);
                *damage.entry(other).or_default() += f64::from(count) * attack / defense
                    * self.rules.air_damage_scale
                    * avoidance
                    * f64::from(n)
                    / total.max(1.);
            }
        }
        for (&id, &count) in &launched {
            let a = &self.wings[&id];
            if a.mission != AirMission::CloseAirSupport {
                continue;
            }
            let aa = w
                .divisions
                .values()
                .filter(|d| {
                    !d.destroyed
                        && d.battle.is_some()
                        && w.at_war(&a.country, &d.country)
                        && self.region_at(d.province) == a.region
                })
                .map(|d| d.derived.stats.air_attack)
                .sum::<f64>()
                .max(0.);
            *damage.entry(id).or_default() += f64::from(count) * aa * self.rules.aa_loss_factor;
        }
        for (id, expected) in damage {
            let a = self.wings.get_mut(&id).unwrap();
            let lost = w.rng.stochastic_round(expected).min(a.planes);
            a.planes -= lost;
            a.losses += u64::from(lost);
            if let Some(r) = self
                .results
                .get_mut(&a.region.unwrap())
                .and_then(|r| r.get_mut(&a.country))
            {
                r.losses += lost;
            }
        }
        for rs in self.results.values_mut() {
            for r in rs.values_mut() {
                r.fighters = 0;
            }
        }
        for (&id, &count) in &launched {
            let a = &self.wings[&id];
            if matches!(
                a.mission,
                AirMission::Superiority | AirMission::Interception
            ) {
                if let Some(r) = self
                    .results
                    .get_mut(&a.region.unwrap())
                    .and_then(|r| r.get_mut(&a.country))
                {
                    r.fighters += count.min(a.planes);
                }
            }
        }
        for (id, count) in launched {
            let a = &self.wings[&id];
            if a.mission != AirMission::CloseAirSupport {
                continue;
            }
            let region = a.region.unwrap();
            let surviving = count.min(a.planes);
            let r = &mut self
                .results
                .get_mut(&region)
                .unwrap()
                .get_mut(&a.country)
                .unwrap();
            r.cas += surviving;
            r.ground_attack += f64::from(surviving)
                * w.catalogue.equipment[&a.model]
                    .stats
                    .get("air_ground_attack")
                    .copied()
                    .unwrap_or(0.)
                    .max(0.);
        }
    }
}
