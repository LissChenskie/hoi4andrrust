//! Dockyard-backed naval refits. Same-hull validation and saved production
//! progress are authoritative. The relative refit IC price is a clearly
//! documented approximation until every module conversion factor is imported.
use crate::{
    naval_warfare::{NavalMission, NavalState},
    warfare::Warfare,
    world::World,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavalRefit {
    pub id: u64,
    pub country: String,
    pub task_force: u64,
    pub ship_index: usize,
    pub original_variant: Option<String>,
    pub design: String,
    pub base: u32,
    pub required_ic: f64,
    pub progress_ic: f64,
    pub requested_dockyards: u32,
    pub last_day_ic: f64,
}

impl NavalState {
    /// Start a refit, but never transform a ship into a different hull or
    /// relocate a ship to a fictional repair facility.
    pub fn queue_refit(
        &mut self,
        tag: &str,
        task_force: u64,
        ship_index: usize,
        design_id: &str,
        dockyards: u32,
        world: &World,
        w: &Warfare,
    ) -> Result<u64, String> {
        let force = self
            .task_forces
            .get(&task_force)
            .ok_or("Соединение не найдено")?;
        let ship = force.ships.get(ship_index).ok_or("Корабль не найден")?;
        if force.country != tag || ship.strength <= 0.0 {
            return Err("Переоборудовать можно только собственный действующий корабль".into());
        }
        if force.mission != NavalMission::Hold
            || world.controller_of_province(force.base) != Some(tag)
            || w.conditions
                .get(&force.base)
                .is_none_or(|c| c.naval_base == 0)
        {
            return Err(
                "Для переоборудования корабль должен находиться в собственном порту".into(),
            );
        }
        let design = self.designs.get(design_id).ok_or("Проект не найден")?;
        if design.country != tag || design.hull != ship.model {
            return Err("Переоборудование возможно только на тот же корпус страны".into());
        }
        if ship.variant.as_deref() == Some(design_id) {
            return Err("Корабль уже соответствует выбранному проекту".into());
        }
        if dockyards == 0 || !design.cost.is_finite() || design.cost <= 0.0 {
            return Err("Некорректная стоимость или количество верфей".into());
        }
        if self
            .refits
            .values()
            .any(|job| job.task_force == task_force && job.ship_index == ship_index)
        {
            return Err("Этот корабль уже проходит переоборудование".into());
        }
        // The target may not invalidate the existing embarked aircraft.
        if let Some(groups) = self.carrier_wings.get(&(task_force, ship_index)) {
            let reserved: u32 = groups.iter().map(|g| g.target.max(g.planes)).sum();
            let capacity = design
                .stats
                .get("deck_size")
                .copied()
                .unwrap_or(0.0)
                .max(0.0) as u32;
            if reserved > capacity {
                return Err("Новый проект не вмещает размещённые палубные самолёты".into());
            }
        }
        let original = ship
            .variant
            .as_ref()
            .and_then(|id| self.designs.get(id))
            .map_or_else(
                || {
                    w.catalogue
                        .equipment
                        .get(&ship.model)
                        .map_or(0.0, |d| d.cost)
                },
                |d| d.cost,
            );
        let hull_cost = w
            .catalogue
            .equipment
            .get(&ship.model)
            .map_or(0.0, |d| d.cost);
        // HOI4 has module-dependent conversion rules. Until those are
        // imported, use the define-derived minimum hull floor and positive
        // price difference, explicitly NOT claimed as engine equivalence.
        let fraction = w
            .catalogue
            .literal_defines
            .get("NDefines.NProduction.BASE_NAVAL_EQUIPMENT_CONVERSION_IC_COST_FACTOR")
            .copied()
            .filter(|n| n.is_finite() && *n >= 0.0)
            .unwrap_or(0.2);
        let min_fraction = w
            .catalogue
            .literal_defines
            .get("NDefines.NProduction.MIN_NAVAL_EQUIPMENT_CONVERSION_IC_COST_FACTOR")
            .copied()
            .filter(|n| n.is_finite() && *n >= 0.0)
            .unwrap_or(0.2);
        let required = (hull_cost * fraction.max(min_fraction))
            .max((design.cost - original).max(0.0))
            .max(0.1);
        if !required.is_finite() {
            return Err("Некорректная стоимость рефита".into());
        }
        self.next_refit_id = self.next_refit_id.saturating_add(1);
        let id = self.next_refit_id;
        self.refits.insert(
            id,
            NavalRefit {
                id,
                country: tag.into(),
                task_force,
                ship_index,
                original_variant: ship.variant.clone(),
                design: design_id.into(),
                base: force.base,
                required_ic: required,
                progress_ic: 0.0,
                requested_dockyards: dockyards.min(self.rules.dockyards_per_line.max(1)),
                last_day_ic: 0.0,
            },
        );
        Ok(id)
    }

    pub fn cancel_refit(&mut self, tag: &str, id: u64) -> bool {
        if self.refits.get(&id).is_some_and(|job| job.country == tag) {
            self.refits.remove(&id);
            true
        } else {
            false
        }
    }

    /// Run refits before new hull construction; both draw from the same finite
    /// dockyard and strategic-resource pools. Neither completion nor cancel
    /// creates a new ship or restores lost ship strength for free.
    pub fn advance_refits(
        &mut self,
        tag: &str,
        dockyards: u32,
        world: &World,
        w: &Warfare,
        pool: &mut BTreeMap<String, u32>,
        shortage_penalty: f64,
    ) -> u32 {
        let mut free = dockyards;
        let mut completed = Vec::new();
        for (&id, job) in self.refits.iter_mut().filter(|(_, job)| job.country == tag) {
            job.last_day_ic = 0.0;
            let Some(force) = self.task_forces.get_mut(&job.task_force) else {
                continue;
            };
            let Some(ship) = force.ships.get_mut(job.ship_index) else {
                continue;
            };
            if ship.strength <= 0.0
                || ship.variant != job.original_variant
                || force.mission != NavalMission::Hold
                || force.base != job.base
                || world.controller_of_province(job.base) != Some(tag)
                || w.conditions
                    .get(&job.base)
                    .is_none_or(|c| c.naval_base == 0)
            {
                continue;
            }
            let Some(design) = self.designs.get(&job.design) else {
                continue;
            };
            if design.hull != ship.model || design.country != tag {
                continue;
            }
            let assigned = free.min(job.requested_dockyards);
            free -= assigned;
            for _ in 0..assigned {
                let mut missing = 0_u32;
                for (resource, &needed) in &design.resources {
                    let available = pool.entry(resource.clone()).or_default();
                    missing = missing.max(needed.saturating_sub(*available));
                    *available = available.saturating_sub(needed);
                }
                job.last_day_ic += self.rules.dockyard_output.max(0.0)
                    * (1.0 - shortage_penalty * f64::from(missing)).clamp(0.0, 1.0);
            }
            job.progress_ic += job.last_day_ic;
            if job.progress_ic >= job.required_ic {
                ship.variant = Some(job.design.clone());
                completed.push(id);
            }
        }
        for id in completed {
            self.refits.remove(&id);
        }
        free
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        military_model::{CountryArmy, EquipmentDefinition, MilitaryCatalogue},
        naval_designer::NavalDesign,
        naval_warfare::{NavalShip, NavalTaskForce},
        warfare::ProvinceConditions,
    };
    use std::collections::{BTreeMap, BTreeSet, HashMap};
    fn fixture() -> (World, Warfare, NavalState) {
        let world = World::from_map_data(
            &HashMap::from([(1, 1)]),
            &HashMap::from([(1, "A".into())]),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );
        let mut catalogue = MilitaryCatalogue::default();
        catalogue.equipment.insert(
            "ship_hull_light_1".into(),
            EquipmentDefinition {
                id: "ship_hull_light_1".into(),
                archetype: "ship_hull_light".into(),
                year: 1936,
                active: true,
                buildable: true,
                stats: BTreeMap::new(),
                cost: 100.,
                modular: false,
                source: "test".into(),
            },
        );
        let mut w = Warfare::new(catalogue);
        w.conditions.insert(
            1,
            ProvinceConditions {
                naval_base: 3,
                ..Default::default()
            },
        );
        w.countries.insert("A".into(), CountryArmy::default());
        let mut navy = NavalState::default();
        navy.task_forces.insert(
            1,
            NavalTaskForce {
                id: 1,
                country: "A".into(),
                name: "Port squadron".into(),
                base: 1,
                mission: NavalMission::Hold,
                regions: BTreeSet::new(),
                ships: vec![NavalShip {
                    name: "One".into(),
                    model: "ship_hull_light_1".into(),
                    definition: "light".into(),
                    variant: None,
                    strength: 0.6,
                }],
            },
        );
        navy.designs.insert(
            "A_new".into(),
            NavalDesign {
                id: "A_new".into(),
                country: "A".into(),
                name: "New".into(),
                hull: "ship_hull_light_1".into(),
                modules: BTreeMap::new(),
                stats: BTreeMap::from([("build_cost_ic".into(), 120.)]),
                resources: BTreeMap::from([("steel".into(), 1)]),
                cost: 120.,
            },
        );
        (world, w, navy)
    }
    #[test]
    fn refit_requires_same_hull_idle_owned_port_and_single_job() {
        let (mut world, w, mut navy) = fixture();
        assert!(navy.queue_refit("B", 1, 0, "A_new", 1, &world, &w).is_err());
        navy.task_forces.get_mut(&1).unwrap().mission = NavalMission::Patrol;
        assert!(navy.queue_refit("A", 1, 0, "A_new", 1, &world, &w).is_err());
        navy.task_forces.get_mut(&1).unwrap().mission = NavalMission::Hold;
        world.set_province_controller(1, None);
        assert!(navy.queue_refit("A", 1, 0, "A_new", 1, &world, &w).is_err());
        world.set_province_controller(1, Some("A"));
        let id = navy.queue_refit("A", 1, 0, "A_new", 1, &world, &w).unwrap();
        assert!(navy.queue_refit("A", 1, 0, "A_new", 1, &world, &w).is_err());
        assert!(!navy.cancel_refit("B", id));
        assert!(navy.cancel_refit("A", id));
        assert_eq!(navy.task_forces[&1].ships[0].strength, 0.6);
    }
    #[test]
    fn refit_claims_real_dockyards_material_and_never_duplicates_a_ship() {
        let (world, w, mut navy) = fixture();
        let id = navy.queue_refit("A", 1, 0, "A_new", 2, &world, &w).unwrap();
        let cost = navy.refits[&id].required_ic;
        assert!(cost > 0. && cost < navy.designs["A_new"].cost);
        let mut pool = BTreeMap::from([("steel".into(), 1)]);
        let remaining = navy.advance_refits("A", 2, &world, &w, &mut pool, 0.1);
        assert_eq!(remaining, 0);
        assert_eq!(pool["steel"], 0);
        assert_eq!(navy.task_forces[&1].ships.len(), 1);
        assert_eq!(navy.task_forces[&1].ships[0].variant, None);
        let progress = navy.refits[&id].progress_ic;
        assert!(progress > 0.);
        let snapshot = serde_json::to_string(&navy).unwrap();
        let mut restored: NavalState = serde_json::from_str(&snapshot).unwrap();
        assert_eq!(restored.refits[&id].progress_ic, progress);
        let mut pool = BTreeMap::from([("steel".into(), 100)]);
        for _ in 0..50 {
            restored.advance_refits("A", 2, &world, &w, &mut pool, 0.0);
            if restored.refits.is_empty() {
                break;
            }
        }
        assert!(restored.refits.is_empty());
        assert_eq!(
            restored.task_forces[&1].ships[0].variant.as_deref(),
            Some("A_new")
        );
        assert_eq!(restored.task_forces[&1].ships[0].strength, 0.6);
        assert_eq!(restored.task_forces[&1].ships.len(), 1);
    }
}
