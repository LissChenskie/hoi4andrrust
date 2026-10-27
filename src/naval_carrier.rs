//! Carrier-based squadrons consume real aircraft stock and deck slots.
//! Aircraft losses/sortie timing are provisional where HOI4 data defines are
//! unavailable; there is no free on-deck aircraft creation.
use crate::{
    air_warfare::is_aircraft, military_model::EquipmentDefinition, naval_warfare::NavalState,
    production::ProductionState, warfare::Warfare,
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
    pub fn replenish_carriers(&mut self, w: &Warfare, production: &mut ProductionState) {
        for (&(id, index), groups) in &mut self.carrier_wings {
            let Some(force) = self.task_forces.get(&id) else {
                continue;
            };
            if force.mission != crate::naval_warfare::NavalMission::Hold
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
        let enemy_tag = enemy.country.clone();
        let mut attacks = Vec::new();
        let define = &w.catalogue.literal_defines;
        let delay = define
            .get("NDefines.NNavy.CARRIER_HOURS_DELAY")
            .copied()
            .filter(|n| n.is_finite() && *n > 0.)
            .unwrap_or(8.)
            .round() as u64;
        let air_damage = define
            .get("NDefines.NNavy.NAVAL_STRIKE_DAMAGE_TO_STR")
            .copied()
            .filter(|v| v.is_finite() && *v > 0.)
            .unwrap_or(0.015);
        let efficiency = (1.
            + w.countries.get(&source_tag).map_or(0., |c| {
                c.modifiers
                    .country
                    .get("sortie_efficiency")
                    .copied()
                    .unwrap_or(0.)
            }))
        .max(0.);
        let enemy_fighters = self
            .carrier_wings
            .iter()
            .filter(|((id, _), _)| *id == dst)
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
        let enemy_hp: Vec<f64> = enemy.ships.iter()
            .map(|ship|self.ship_stats(ship,w)
                .and_then(|s|s.get("max_strength")).copied().unwrap_or(100.).max(1.))
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
        // Avoid accidental interleaving of two simulation systems at once.
        let _ = enemy_tag;
        attacks
    }
}
