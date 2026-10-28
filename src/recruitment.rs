//! Preparation queue owns reserved people and equipment until deployment.
//! It shares the country's templates, production stock and hourly clock.
use crate::{military_model::*, production::ProductionState, warfare::Warfare, world::World};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecruitmentOrder {
    pub id: u64,
    pub country: String,
    pub template: DivisionTemplate,
    pub province: u32,
    pub manpower: u32,
    pub equipment: BTreeMap<String, u32>,
    pub training_hours: f64,
    pub required_hours: f64,
}
impl Warfare {
    pub fn queue_recruitment(
        &mut self,
        country: &str,
        template: &str,
        province: u32,
        world: &World,
        production: &mut ProductionState,
    ) -> Result<u64, String> {
        self.validate_deployment(country, province, world)?;
        let a = self
            .countries
            .get(country)
            .ok_or("Не загружена армия страны")?;
        let t = a.templates.get(template).ok_or("Нет такого шаблона")?;
        self.catalogue.validate_template(t, &a.modifiers, true)?;
        let derived = self.catalogue.aggregate(t, &a.modifiers, None)?;
        let men = derived.stats.manpower.ceil() as u32;
        if a.manpower_pool < u64::from(men) {
            return Err(format!(
                "Недостаточно людей: нужно {men}, доступно {}",
                a.manpower_pool
            ));
        }
        if derived.stats.training <= 0.0 {
            return Err("Не импортирована длительность подготовки этого шаблона".into());
        }
        let id = self.next_recruitment_id;
        self.next_recruitment_id += 1;
        let job = RecruitmentOrder {
            id,
            country: country.into(),
            template: t.clone(),
            province,
            manpower: men,
            equipment: BTreeMap::new(),
            training_hours: 0.0,
            required_hours: derived.stats.training * 24.0,
        };
        self.countries.get_mut(country).unwrap().manpower_pool -= u64::from(men);
        self.recruitment.insert(id, job);
        self.supply_recruitment(production);
        Ok(id)
    }
    fn validate_deployment(
        &self,
        country: &str,
        province: u32,
        world: &World,
    ) -> Result<(), String> {
        if world.controller_of_province(province) != Some(country)
            || !world
                .geography
                .provinces
                .get(&province)
                .is_some_and(|p| p.definition.kind == crate::map_graph::ProvinceKind::Land)
        {
            return Err("Выберите на карте свою сухопутную провинцию развёртывания".into());
        }
        if self
            .battles
            .values()
            .any(|b| b.province == province && b.outcome.is_none())
        {
            return Err("В провинции развёртывания идёт бой".into());
        }
        Ok(())
    }
    pub fn change_deployment(
        &mut self,
        country: &str,
        id: u64,
        province: u32,
        world: &World,
    ) -> Result<(), String> {
        self.validate_deployment(country, province, world)?;
        let job = self
            .recruitment
            .get_mut(&id)
            .filter(|j| j.country == country)
            .ok_or("Нет своего набора")?;
        job.province = province;
        Ok(())
    }
    pub fn cancel_recruitment(
        &mut self,
        country: &str,
        id: u64,
        production: &mut ProductionState,
    ) -> Result<(), String> {
        if !self
            .recruitment
            .get(&id)
            .is_some_and(|j| j.country == country)
        {
            return Err("Нет своего набора".into());
        }
        let job = self.recruitment.remove(&id).unwrap();
        self.countries.get_mut(country).unwrap().manpower_pool += u64::from(job.manpower);
        let stock = &mut production.country_mut(country).model_stockpile;
        for (model, n) in job.equipment {
            *stock.entry(model).or_default() += u64::from(n);
        }
        Ok(())
    }
    pub fn recruitment_equipment_fraction(&self, job: &RecruitmentOrder) -> f64 {
        let Some(a) = self.countries.get(&job.country) else {
            return 0.0;
        };
        let Ok(d) = self.catalogue.aggregate(&job.template, &a.modifiers, None) else {
            return 0.0;
        };
        d.equipment
            .iter()
            .map(|(kind, n)| {
                let actual: u64 = job
                    .equipment
                    .iter()
                    .filter(|(id, _)| {
                        self.catalogue
                            .equipment
                            .get(*id)
                            .is_some_and(|m| &m.archetype == kind)
                    })
                    .map(|(_, n)| u64::from(*n))
                    .sum();
                (actual as f64 / n.ceil().max(1.0)).min(1.0)
            })
            .fold(1.0, f64::min)
    }
    pub(crate) fn supply_recruitment(&mut self, production: &mut ProductionState) {
        for job in self.recruitment.values_mut() {
            if !self.logistics.can_deliver(&job.country, job.province) {
                continue;
            }
            let Some(a) = self.countries.get(&job.country) else {
                continue;
            };
            let Ok(d) = self.catalogue.aggregate(&job.template, &a.modifiers, None) else {
                continue;
            };
            let stock = &mut production.country_mut(&job.country).model_stockpile;
            for (kind, need) in d.equipment {
                let actual: u64 = job
                    .equipment
                    .iter()
                    .filter(|(id, _)| {
                        self.catalogue
                            .equipment
                            .get(*id)
                            .is_some_and(|m| m.archetype == kind)
                    })
                    .map(|(_, n)| u64::from(*n))
                    .sum();
                let mut missing = (need.ceil() as u64).saturating_sub(actual);
                let mut models: Vec<_> = stock
                    .keys()
                    .filter_map(|id| self.catalogue.equipment.get(id))
                    .filter(|m| m.archetype == kind)
                    .collect();
                let preference = job.template.equipment_preferences.get(&kind);
                models.sort_by(|a, b| {
                    (Some(&b.id) == preference)
                        .cmp(&(Some(&a.id) == preference))
                        .then(b.year.cmp(&a.year))
                        .then(a.id.cmp(&b.id))
                });
                for m in models {
                    let n = missing.min(stock[&m.id]).min(u64::from(u32::MAX));
                    *stock.get_mut(&m.id).unwrap() -= n;
                    *job.equipment.entry(m.id.clone()).or_default() += n as u32;
                    missing -= n;
                    if missing == 0 {
                        break;
                    }
                }
            }
        }
    }
    pub(crate) fn tick_recruitment(&mut self, world: &World) {
        let ids: Vec<_> = self.recruitment.keys().copied().collect();
        for id in ids {
            let equipped = self.recruitment_equipment_fraction(&self.recruitment[&id]);
            // Partial equipment permits slower training, but empty training
            // depots cannot accumulate progress out of nothing.
            if equipped < 0.2 {
                continue;
            }
            let job = self.recruitment.get_mut(&id).unwrap();
            job.training_hours = (job.training_hours + equipped).min(job.required_hours);
            // Auto-deployment needs both full training and full equipment.
            // Early deployment is only initiated by an explicit player order.
            if job.training_hours >= job.required_hours && equipped >= 1. - 1e-9 {
                let _ = self.finish_recruitment(id, world, false);
            }
        }
    }

    /// HOI4-style early order: the player may deploy a green formation after
    /// at least 20% training and 20% equipment, conserving the queued resources.
    pub fn deploy_recruitment_early(
        &mut self,
        country: &str,
        id: u64,
        world: &World,
    ) -> Result<u64, String> {
        let job = self
            .recruitment
            .get(&id)
            .filter(|job| job.country == country)
            .ok_or("Нет своего набора")?;
        if job.training_hours / job.required_hours < 0.2
            || self.recruitment_equipment_fraction(job) < 0.2
        {
            return Err("Для раннего развёртывания необходимо 20% обучения и 20% техники".into());
        }
        self.finish_recruitment(id, world, true)
    }

    fn finish_recruitment(&mut self, id: u64, world: &World, early: bool) -> Result<u64, String> {
        let job = self.recruitment.get(&id).ok_or("Набор не найден")?.clone();
        self.validate_deployment(&job.country, job.province, world)?;
        // Snapshot the requested template before deployment. Edits to the
        // currently saved template must not steal reserved equipment or people.
        let existing = self.countries[&job.country]
            .templates
            .get(&job.template.name)
            .cloned();
        self.countries
            .get_mut(&job.country)
            .unwrap()
            .templates
            .insert(job.template.name.clone(), job.template.clone());
        let result = self.spawn_history(
            &job.country,
            &job.template.name,
            job.province,
            &format!("{}. {}", self.next_division_id, job.template.name),
            0.0,
            1.0,
            if early { 0.0 } else { 0.1 },
            world,
        );
        match existing {
            Some(t) => {
                self.countries
                    .get_mut(&job.country)
                    .unwrap()
                    .templates
                    .insert(t.name.clone(), t);
            }
            None => {
                self.countries
                    .get_mut(&job.country)
                    .unwrap()
                    .templates
                    .remove(&job.template.name);
            }
        }
        let unit = result?;
        let d = self.divisions.get_mut(&unit).unwrap();
        d.fuel_fraction = 0.;
        d.equipment = job.equipment;
        d.manpower = job.manpower;
        if early {
            d.experience = ExperienceLevel::Green;
        }
        // The saved template may have been changed while the recruit trained.
        // Reaggregate using only its real carried equipment after deployment.
        let army = self.countries.get_mut(&job.country).unwrap();
        let template = army
            .templates
            .entry(job.template.name.clone())
            .or_insert(job.template);
        if let Ok(stats) = self
            .catalogue
            .aggregate(template, &army.modifiers, Some(&d.equipment))
        {
            d.derived = stats;
            d.organization = d.derived.stats.organization;
        }
        let fraction = self.equipment_fraction(&self.divisions[&unit]).min(
            f64::from(self.divisions[&unit].manpower)
                / self.divisions[&unit].derived.stats.manpower.max(1.0),
        );
        self.divisions.get_mut(&unit).unwrap().hp =
            self.divisions[&unit].derived.stats.hp * fraction;
        self.recruitment.remove(&id);
        let name = &self.divisions[&unit].name;
        self.notices.push_back(format!(
            "{}: {} дивизия {}",
            job.country,
            if early {
                "досрочно развёрнута"
            } else {
                "подготовлена"
            },
            name
        ));
        Ok(unit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (World, Warfare, ProductionState, String) {
        let world = crate::warfare::tests::world();
        let mut db = crate::military_model::tests::catalogue();
        db.units
            .get_mut("infantry")
            .unwrap()
            .need
            .insert("rifles".into(), 100.0);
        db.units
            .get_mut("infantry")
            .unwrap()
            .stats
            .insert("training_time".into(), 1.0);
        db.equipment.insert(
            "rifle".into(),
            EquipmentDefinition {
                id: "rifle".into(),
                archetype: "rifles".into(),
                active: true,
                buildable: true,
                year: 1936,
                cost: 1.0,
                modular: false,
                source: "authored".into(),
                stats: BTreeMap::new(),
            },
        );
        let t = crate::military_model::tests::template();
        let name = t.name.clone();
        let mut w = Warfare::new(db);
        w.countries.entry("A".into()).or_default().manpower_pool = 10000;
        w.countries
            .get_mut("A")
            .unwrap()
            .templates
            .insert(name.clone(), t);
        (world, w, ProductionState::default(), name)
    }
    #[test]
    fn partial_equipment_advances_training_proportionally_until_fully_equipped() {
        let (world, mut w, mut production, name) = setup();
        let id = w
            .queue_recruitment("A", &name, 1, &world, &mut production)
            .unwrap();
        production
            .country_mut("A")
            .model_stockpile
            .insert("rifle".into(), 50);
        w.supply_recruitment(&mut production);
        for _ in 0..24 {
            w.tick_recruitment(&world);
        }
        assert_eq!(w.recruitment[&id].training_hours, 12.);
        assert!(w.divisions.is_empty());
        production
            .country_mut("A")
            .model_stockpile
            .insert("rifle".into(), 50);
        w.supply_recruitment(&mut production);
        for _ in 0..12 {
            w.tick_recruitment(&world);
        }
        assert!(!w.recruitment.contains_key(&id));
        assert_eq!(w.divisions.values().next().unwrap().equipment["rifle"], 100);
    }
    #[test]
    fn early_deployment_requires_progress_and_preserves_reservations() {
        let (world, mut w, mut production, name) = setup();
        let initial = w.countries["A"].manpower_pool;
        let job = w
            .queue_recruitment("A", &name, 1, &world, &mut production)
            .unwrap();
        production
            .country_mut("A")
            .model_stockpile
            .insert("rifle".into(), 25);
        w.supply_recruitment(&mut production);
        assert!(w.deploy_recruitment_early("A", job, &world).is_err());
        for _ in 0..20 {
            w.tick_recruitment(&world);
        }
        let unit = w.deploy_recruitment_early("A", job, &world).unwrap();
        let d = &w.divisions[&unit];
        assert_eq!(d.experience, ExperienceLevel::Green);
        assert_eq!(d.equipment["rifle"], 25);
        assert!(d.hp < d.derived.stats.hp);
        assert!(w.recruitment.is_empty());
        assert_eq!(
            w.countries["A"].manpower_pool + u64::from(d.manpower),
            initial
        );
        assert_eq!(production.country("A").unwrap().model_stockpile["rifle"], 0);
        assert!(w.deploy_recruitment_early("A", job, &world).is_err());
    }
    #[test]
    fn equipment_gates_training_and_deployment_does_not_create_resources() {
        let (world, mut w, mut production, name) = setup();
        let men = w.countries["A"].manpower_pool;
        let job = w
            .queue_recruitment("A", &name, 1, &world, &mut production)
            .unwrap();
        let reserved = w.recruitment[&job].manpower;
        assert!(w.divisions.is_empty());
        for _ in 0..48 {
            w.tick_recruitment(&world);
        }
        assert_eq!(w.recruitment[&job].training_hours, 0.0);
        production
            .country_mut("A")
            .model_stockpile
            .insert("rifle".into(), 100);
        w.replenish(&mut production);
        assert_eq!(production.country("A").unwrap().model_stockpile["rifle"], 0);
        for _ in 0..24 {
            w.tick_recruitment(&world);
        }
        assert!(w.recruitment.is_empty());
        let d = w.divisions.values().next().unwrap();
        assert_eq!(d.province, 1);
        assert_eq!(d.manpower, reserved);
        assert_eq!(d.equipment["rifle"], 100);
        assert_eq!(w.countries["A"].manpower_pool + u64::from(reserved), men);
    }
    #[test]
    fn cancelled_training_returns_people_and_equipment_and_lost_location_does_not_deploy() {
        let (mut world, mut w, mut production, name) = setup();
        production
            .country_mut("A")
            .model_stockpile
            .insert("rifle".into(), 200);
        let men = w.countries["A"].manpower_pool;
        let job = w
            .queue_recruitment("A", &name, 1, &world, &mut production)
            .unwrap();
        world.set_province_controller(1, Some("B"));
        for _ in 0..48 {
            w.tick_recruitment(&world);
        }
        assert!(w.divisions.is_empty());
        assert!(w.recruitment.contains_key(&job));
        let roundtrip: Warfare = serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
        assert_eq!(roundtrip.recruitment[&job].equipment["rifle"], 100);
        w.cancel_recruitment("A", job, &mut production).unwrap();
        assert_eq!(w.countries["A"].manpower_pool, men);
        assert_eq!(
            production.country("A").unwrap().model_stockpile["rifle"],
            200
        );
        assert!(w.cancel_recruitment("A", job, &mut production).is_err());
        assert_eq!(
            production.country("A").unwrap().model_stockpile["rifle"],
            200
        );
    }
}
