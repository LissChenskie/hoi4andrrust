//! Operational integration: orders, movement, battles, retreat and inventories.
use crate::combat_rules::SeededRandom;
use crate::game::GameDate;
use crate::land_combat::{AirSupport, BattleEnvironment, BattleOutcome, LandBattle};
use crate::map_graph::{CrossingKind, ProvinceKind, RiverCrossing};
use crate::military_model::*;
use crate::production::{Equipment, ProductionRules, ProductionState};
use crate::research::ResearchWorld;
use crate::world::World;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque};
use std::sync::Arc;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ProvinceConditions {
    pub weather: Option<String>,
    pub night_override: Option<bool>,
    pub fort_level: u16,
    pub naval_base: u16,
    pub victory_points: f64,
    pub supply_node: bool,
    pub attacker_air: AirSupport,
    pub defender_air: AirSupport,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Warfare {
    #[serde(default)]
    pub naval: crate::naval_warfare::NavalState,
    #[serde(default)]
    pub logistics: crate::logistics::LogisticsState,
    #[serde(default)]
    pub air: crate::air_warfare::AirState,
    pub command: crate::command::CommandState,
    pub abilities: crate::command_abilities::AbilityState,
    pub fronts: crate::frontline::FrontState,
    pub plans: crate::operations::OperationalState,
    #[serde(skip)]
    pub automatic_order: bool,
    #[serde(skip)]
    pub catalogue: Arc<MilitaryCatalogue>,
    pub countries: BTreeMap<String, CountryArmy>,
    pub divisions: BTreeMap<DivisionId, Division>,
    #[serde(default)]
    pub recruitment: BTreeMap<u64, crate::recruitment::RecruitmentOrder>,
    #[serde(default = "first_recruitment_id")]
    pub next_recruitment_id: u64,
    pub battles: BTreeMap<u64, LandBattle>,
    pub wars: BTreeSet<(String, String)>,
    pub military_access: BTreeSet<(String, String)>,
    pub strait_access: BTreeSet<(String, u32, u32)>,
    pub conditions: BTreeMap<u32, ProvinceConditions>,
    pub rng: SeededRandom,
    pub notices: VecDeque<String>,
    pub diagnostics: Vec<String>,
    pub strict_geography: bool,
    pub next_division_id: u64,
    pub next_battle_id: u64,
    pub simulated_hours: u64,
    pub ai_enabled: bool,
}
fn first_recruitment_id() -> u64 {
    1
}
impl Default for Warfare {
    fn default() -> Self {
        Self::new(MilitaryCatalogue::default())
    }
}
impl Warfare {
    pub fn new(catalogue: MilitaryCatalogue) -> Self {
        let diagnostics = catalogue.diagnostics.clone();
        Self {
            naval: Default::default(),
            logistics: Default::default(),
            air: Default::default(),
            command: Default::default(),
            abilities: Default::default(),
            fronts: Default::default(),
            plans: Default::default(),
            automatic_order: false,
            catalogue: Arc::new(catalogue),
            countries: BTreeMap::new(),
            divisions: BTreeMap::new(),
            recruitment: BTreeMap::new(),
            next_recruitment_id: 1,
            battles: BTreeMap::new(),
            wars: BTreeSet::new(),
            military_access: BTreeSet::new(),
            strait_access: BTreeSet::new(),
            conditions: BTreeMap::new(),
            rng: SeededRandom::new(19360101),
            notices: VecDeque::new(),
            diagnostics,
            strict_geography: false,
            next_division_id: 1,
            next_battle_id: 1,
            simulated_hours: 0,
            ai_enabled: true,
        }
    }
    pub fn clean_references(&mut self) {
        let abandoned: Vec<_> = self
            .fronts
            .fronts
            .values()
            .filter(|f| {
                f.auto_include
                    && f.assigned_divisions.is_empty()
                    && self.fronts.fronts.values().any(|other| {
                        other.id != f.id
                            && other.army == f.army
                            && !other.assigned_divisions.is_empty()
                    })
            })
            .map(|f| f.id)
            .collect();
        if !abandoned.is_empty() {
            for id in abandoned {
                self.fronts.fronts.remove(&id);
            }
            self.fronts.revision += 1;
        }
        for f in self.fronts.fronts.values_mut() {
            f.assignments
                .retain(|id, _| f.assigned_divisions.contains(id));
            f.reserves.retain(|id| f.assigned_divisions.contains(id));
        }
        self.fronts
            .fronts
            .retain(|_, f| self.command.armies.contains_key(&f.army));
        self.plans.normalize(&self.command, &self.fronts);
        self.abilities
            .headquarters
            .retain(|_, h| self.command.armies.contains_key(&h.army));
        for h in self.abilities.headquarters.values_mut() {
            if h.plan.is_some_and(|p| !self.plans.plans.contains_key(&p)) {
                h.plan = None;
            }
        }
    }
    pub fn validate_references(&self, world: &World) -> Result<(), String> {
        if !self.command.identifiers_valid()
            || !self.fronts.identifiers_valid()
            || !self.plans.identifiers_valid()
            || self.divisions.keys().any(|id| *id >= self.next_division_id)
            || self.battles.keys().any(|id| *id >= self.next_battle_id)
        {
            return Err("Повреждены устойчивые идентификаторы армии".into());
        }
        for (&id, job) in &self.recruitment {
            if id != job.id
                || id >= self.next_recruitment_id
                || !self.countries.contains_key(&job.country)
                || !world.provinces.contains_key(&job.province)
                || !job.training_hours.is_finite()
                || !job.required_hours.is_finite()
                || job.required_hours <= 0.0
                || job.training_hours < 0.0
                || job.training_hours > job.required_hours
            {
                return Err("Повреждена очередь подготовки".into());
            }
        }
        for (&id, d) in &self.divisions {
            if id != d.id
                || !world.provinces.contains_key(&d.province)
                || !self
                    .countries
                    .get(&d.country)
                    .is_some_and(|c| c.templates.contains_key(&d.template))
                || d.battle.is_some_and(|b| !self.battles.contains_key(&b))
            {
                return Err(format!("Повреждена ссылка дивизии {id}"));
            }
            let path = match &d.order {
                DivisionOrder::Move { path, .. }
                | DivisionOrder::StrategicRedeployment { path, .. } => path.clone(),
                DivisionOrder::Retreat { destination, .. } => vec![*destination],
                _ => Vec::new(),
            };
            if path.iter().any(|p| !world.provinces.contains_key(p)) {
                return Err("Маршрут ссылается на отсутствующую провинцию".into());
            }
        }
        if self.command.membership.iter().any(|(d, a)| {
            !self.divisions.contains_key(d)
                || !self
                    .command
                    .armies
                    .get(a)
                    .is_some_and(|a| a.country == self.divisions[d].country)
        }) {
            return Err("Повреждён состав армии".into());
        }
        for a in self.command.armies.values() {
            if !self.command.theaters.contains_key(&a.theater)
                || a.group
                    .is_some_and(|g| !self.command.groups.contains_key(&g))
                || a.general.as_ref().is_some_and(|c| {
                    !self
                        .command
                        .commanders
                        .get(c)
                        .is_some_and(|c| c.country == a.country)
                })
            {
                return Err("Повреждена структура командования".into());
            }
        }
        for f in self.fronts.fronts.values() {
            if !self.command.armies.contains_key(&f.army)
                || f.assigned_divisions
                    .iter()
                    .any(|id| !self.divisions.contains_key(id))
                || f.edges.iter().any(|e| {
                    !world.provinces.contains_key(&e.friendly)
                        || !world.provinces.contains_key(&e.enemy)
                })
            {
                return Err("Повреждён фронт".into());
            }
        }
        for p in self.plans.plans.values() {
            if !self.command.armies.contains_key(&p.army)
                || p.assigned_divisions
                    .iter()
                    .any(|id| !self.divisions.contains_key(id))
                || p.destination_line
                    .iter()
                    .any(|id| !world.provinces.contains_key(id))
                || p.source_front
                    .is_some_and(|f| !self.fronts.fronts.contains_key(&f))
            {
                return Err("Повреждён план".into());
            }
        }
        for b in self.battles.values() {
            if !world.provinces.contains_key(&b.province)
                || b.attackers
                    .active
                    .iter()
                    .chain(&b.defenders.active)
                    .chain(&b.attackers.reserves)
                    .chain(&b.defenders.reserves)
                    .any(|id| !self.divisions.contains_key(id))
            {
                return Err("Повреждён состав сражения".into());
            }
        }
        Ok(())
    }
    pub fn export_combat_log(&self, path: &std::path::Path) -> Result<(), String> {
        let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
        serde_json::to_writer_pretty(std::io::BufWriter::new(file), &serde_json::json!({
            "schema":1,"target":self.catalogue.rules.target_version,
            "source_version":self.catalogue.source_version,"source_checksum":self.catalogue.source_checksum,
            "simulated_hours":self.simulated_hours,"rng_state":self.rng.state,
            "rules":self.catalogue.rules,"battles":self.battles
        })).map_err(|e|e.to_string())
    }
    pub fn notice(&mut self, text: impl Into<String>) {
        self.notices.push_back(text.into());
        while self.notices.len() > 100 {
            self.notices.pop_front();
        }
    }
    fn pair(a: &str, b: &str) -> (String, String) {
        if a < b {
            (a.into(), b.into())
        } else {
            (b.into(), a.into())
        }
    }
    pub fn at_war(&self, a: &str, b: &str) -> bool {
        a != b && self.wars.contains(&Self::pair(a, b))
    }
    pub fn declare_war(&mut self, a: &str, b: &str, world: &World) -> Result<(), String> {
        if a == b || !world.countries.contains_key(a) || !world.countries.contains_key(b) {
            return Err("Invalid war participants".into());
        }
        self.wars.insert(Self::pair(a, b));
        for p in self.plans.plans.values_mut() {
            p.control_revision = u64::MAX;
            p.last_review = 0;
        }
        self.notice(format!("{a} is at war with {b}"));
        Ok(())
    }
    pub(crate) fn friendly(&self, country: &str, controller: &str) -> bool {
        country == controller
            || (!self.at_war(country, controller)
                && self
                    .military_access
                    .contains(&(country.into(), controller.into())))
    }
    pub fn use_research_registry(
        &mut self,
        registry: &crate::research::ResearchCatalogue,
        research: &ResearchWorld,
    ) {
        let db = std::sync::Arc::make_mut(&mut self.catalogue);
        db.technologies.clear();
        for t in registry.technologies.values() {
            let mut effect = TechnologyEffects::default();
            effect.country = t.modifiers.clone();
            for e in &t.effects {
                match e {
                    crate::research::TechnologyEffect::ScopedModifier { scope, values } => {
                        if db.equipment.contains_key(scope) {
                            effect.equipment.insert(scope.clone(), values.clone());
                        } else if db.units.contains_key(scope) || scope.starts_with("category_") {
                            effect.units.insert(scope.clone(), values.clone());
                        }
                    }
                    crate::research::TechnologyEffect::Unlock { kind, id, limit }
                        if limit.is_empty() =>
                    {
                        match kind.as_str() {
                            "enable_subunits" => {
                                effect.unlock_units.insert(id.clone());
                            }
                            "enable_equipments" => {
                                effect.unlock_equipment.insert(id.clone());
                            }
                            "enable_equipment_modules" => {
                                effect.unlock_modules.insert(id.clone());
                            }
                            "enable_tactic" => {
                                effect.unlock_tactics.insert(id.clone());
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            db.technologies.insert(t.id.clone(), effect);
        }
        self.refresh_modifiers(research);
    }
    pub fn refresh_modifiers(&mut self, research: &ResearchWorld) {
        for (tag, country) in &mut self.countries {
            let known = research
                .country(tag)
                .map(|c| c.researched.clone())
                .unwrap_or_default();
            let mut m = MilitaryModifiers::from_research(&self.catalogue, &known);
            for (k, v) in &country.base_modifiers {
                *m.country.entry(k.clone()).or_default() += v;
            }
            m.flags = country.modifiers.flags.clone();
            if let Some(c) = research.country(tag) {
                m.flags.extend(c.flags.iter().cloned());
                for unlock in &c.unlocks {
                    if let Some((kind, id)) = unlock.split_once(':') {
                        match kind {
                            "enable_subunits" => {
                                m.unlocked_units.insert(id.into());
                            }
                            "enable_equipments" => {
                                m.unlocked_equipment.insert(id.into());
                            }
                            "enable_equipment_modules" => {
                                m.unlocked_modules.insert(id.into());
                            }
                            "enable_tactic" => {
                                m.unlocked_tactics.insert(id.into());
                            }
                            _ => {}
                        }
                    }
                }
            }
            m.dlcs = country.modifiers.dlcs.clone();
            for tech in &known {
                if let Some(effect) = self.catalogue.technologies.get(tech) {
                    for unlock in &effect.conditional_unlocks {
                        let ctx = crate::land_combat::TacticContext {
                            country: tag,
                            attacker: false,
                            phase: None,
                            modifiers: &m,
                            numbers: BTreeMap::new(),
                        };
                        if crate::land_combat::evaluate_conditions(&unlock.conditions, &ctx)
                            == Some(true)
                        {
                            match unlock.kind {
                                UnlockKind::Unit => {
                                    m.unlocked_units.extend(unlock.ids.iter().cloned())
                                }
                                UnlockKind::Equipment => {
                                    m.unlocked_equipment.extend(unlock.ids.iter().cloned())
                                }
                            }
                        }
                    }
                }
            }
            country.modifiers = m;
        }
        for division in self.divisions.values_mut().filter(|d| !d.destroyed) {
            if let Some(country) = self.countries.get(&division.country) {
                division.coordination = country.modifiers.value("coordination");
                division.reinforce_bonus = country.modifiers.value("reinforce_rate");
                if let Some(template) = country.templates.get(&division.template) {
                    if let Ok(new) = self.catalogue.aggregate(
                        template,
                        &country.modifiers,
                        Some(&division.equipment),
                    ) {
                        let strength = division.strength();
                        division.hp = new.stats.hp * strength;
                        division.organization = division.organization.min(new.stats.organization);
                        division.derived = new;
                    }
                }
            }
        }
    }
    pub fn spawn_history(
        &mut self,
        country: &str,
        template: &str,
        province: u32,
        name: &str,
        equipment_factor: f64,
        manpower_factor: f64,
        experience: f64,
        world: &World,
    ) -> Result<u64, String> {
        if !world
            .geography
            .provinces
            .get(&province)
            .is_some_and(|p| p.definition.kind == ProvinceKind::Land)
        {
            return Err(format!("Invalid land deployment province {province}"));
        }
        let army = self
            .countries
            .get(country)
            .ok_or("Country army is missing")?;
        let t = army
            .templates
            .get(template)
            .ok_or_else(|| format!("Unknown template {template}"))?;
        // Missing equipment is an under-equipped historical formation, not a
        // reason to erase the entire division from its order of battle.
        let mut derived = self
            .catalogue
            .aggregate(t, &army.modifiers, Some(&BTreeMap::new()))?;
        if derived.stats.hp <= 0.0
            || derived.stats.organization <= 0.0
            || derived.stats.width <= 0.0
        {
            return Err("Incomplete combat stats in the imported template".into());
        }
        let mut equipment = BTreeMap::new();
        let mut missing = Vec::new();
        for (archetype, quantity) in &derived.equipment {
            let Some(model) = self.catalogue.best_equipment(archetype, &army.modifiers) else {
                missing.push(archetype.clone());
                continue;
            };
            equipment.insert(
                model.id.clone(),
                (quantity * equipment_factor.clamp(0.0, 1.0)).floor() as u32,
            );
        }
        derived = self
            .catalogue
            .aggregate(t, &army.modifiers, Some(&equipment))?;
        let manpower = (derived.stats.manpower * manpower_factor.clamp(0.0, 1.0)).floor() as u32;
        let id = self.next_division_id;
        self.next_division_id += 1;
        let experience = if experience >= 0.9 {
            ExperienceLevel::Veteran
        } else if experience >= 0.75 {
            ExperienceLevel::Seasoned
        } else if experience >= 0.3 {
            ExperienceLevel::Regular
        } else if experience >= 0.1 {
            ExperienceLevel::Trained
        } else {
            ExperienceLevel::Green
        };
        let fraction = manpower_factor
            .min(if derived.equipment.is_empty() {
                1.0
            } else {
                equipment_factor
            })
            .clamp(0.0, 1.0);
        self.divisions.insert(
            id,
            Division {
                id,
                name: name.into(),
                country: country.into(),
                template: template.into(),
                province,
                hp: derived.stats.hp * fraction,
                organization: derived.stats.organization,
                manpower,
                derived,
                equipment,
                equipment_loss_carry: BTreeMap::new(),
                manpower_loss_carry: 0.0,
                experience,
                entrenchment: 0.0,
                planning: 0.0,
                supply_fraction: 1.0,
                fuel_fraction: 1.0,
                command: Default::default(),
                ability_modifiers: Default::default(),
                commander_attack: 0.0,
                commander_defense: 0.0,
                reinforce_bonus: 0.0,
                coordination: 0.0,
                order: DivisionOrder::Hold,
                destination: None,
                waypoints: VecDeque::new(),
                attack_delay: 0,
                battle: None,
                destroyed: false,
                recent_losses: 0,
            },
        );
        if !missing.is_empty() {
            self.diagnostics.push(format!(
                "{country} / {name}: retained without unresolved equipment: {}",
                missing.join(", ")
            ));
        }
        Ok(id)
    }
    pub fn draw_front(
        &mut self,
        world: &World,
        country: &str,
        army: u64,
        existing: Option<u64>,
        path: &[crate::frontline::FrontEdge],
        selection: &BTreeSet<u64>,
        group: bool,
    ) -> Result<u64, String> {
        if group && self.command.own_army(country, army)?.group.is_none() {
            return Err("Сначала назначьте армию в группу".into());
        }
        let mut fronts = std::mem::take(&mut self.fronts);
        let result = if let Some(id) = existing.filter(|id| {
            fronts
                .fronts
                .get(id)
                .is_some_and(|f| f.army == army && f.country == country)
        }) {
            fronts.redraw(self, world, country, id, path).map(|_| id)
        } else {
            fronts.create_drawn(self, world, country, army, path, selection)
        }
        .and_then(|id| {
            if group {
                fronts.assign_group(self, country, id, selection)?;
            }
            Ok(id)
        });
        self.fronts = fronts;
        if let Ok(id) = result {
            let members = self.fronts.fronts[&id].assigned_divisions.clone();
            self.refresh_automatic_orders(&members);
            for p in self
                .plans
                .plans
                .values_mut()
                .filter(|p| p.source_front == Some(id))
            {
                p.last_review = 0;
                p.source_revision = u64::MAX;
            }
            self.clean_references();
        }
        result
    }
    pub fn edit_front_sector(
        &mut self,
        world: &World,
        country: &str,
        id: u64,
        sector: usize,
        path: &[crate::frontline::FrontEdge],
    ) -> Result<(), String> {
        let f = self
            .fronts
            .fronts
            .get(&id)
            .filter(|f| f.country == country)
            .ok_or("Выберите свой фронт")?;
        let mut sectors = if f.sectors.is_empty() {
            vec![f.edges.clone()]
        } else {
            f.sectors.clone()
        };
        if sector >= sectors.len() {
            return Err("Участок фронта уже изменился".into());
        }
        let army = f.army;
        let mut fronts = std::mem::take(&mut self.fronts);
        let result = fronts.redraw(self, world, country, id, path);
        if result.is_ok() {
            sectors[sector] = path.to_vec();
            let edges: Vec<_> = sectors.into_iter().flatten().collect();
            let enemy = fronts.fronts[&id].enemy_country.clone();
            let sectors = fronts.boundary(world, country, &enemy).sectors(&edges);
            let f = fronts.fronts.get_mut(&id).unwrap();
            f.edges = edges;
            f.anchor_edges = f.edges.clone();
            f.sectors = sectors;
        }
        self.fronts = fronts;
        if result.is_ok() {
            let members = self.fronts.fronts[&id].assigned_divisions.clone();
            self.refresh_automatic_orders(&members);
            for p in self.plans.plans.values_mut().filter(|p| p.army == army) {
                p.source_revision = u64::MAX;
                p.last_review = 0;
            }
        }
        result
    }
    pub fn set_army_execution(
        &mut self,
        country: &str,
        army: u64,
        active: bool,
    ) -> Result<(), String> {
        self.command.own_army(country, army)?;
        let ids: Vec<_> = self
            .plans
            .plans
            .values()
            .filter(|p| {
                p.army == army
                    && p.country == country
                    && p.activation != crate::operations::Activation::Completed
            })
            .map(|p| p.id)
            .collect();
        if ids.is_empty() {
            return Err("Сначала нарисуйте фронт и линию наступления".into());
        }
        for id in ids {
            self.plans.activate(country, id, active)?;
        }
        Ok(())
    }
    pub fn draw_plan(
        &mut self,
        world: &World,
        country: &str,
        army: u64,
        source: Option<u64>,
        kind: crate::operations::OrderType,
        line: Vec<u32>,
        edit: Option<u64>,
    ) -> Result<u64, String> {
        let mut plans = std::mem::take(&mut self.plans);
        let result = if let Some(id) = edit {
            plans.edit_line(country, id, line, world).map(|_| id)
        } else {
            plans.draw_order(self, world, country, army, source, kind, line)
        };
        self.plans = plans;
        if let Ok(id) = result {
            let members = self.plans.plans[&id].assigned_divisions.clone();
            self.refresh_automatic_orders(&members);
            self.clean_references();
        }
        result
    }
    pub fn assign_front(
        &mut self,
        country: &str,
        id: u64,
        selection: &BTreeSet<u64>,
    ) -> Result<(), String> {
        self.validate_selection(country, &selection.iter().copied().collect::<Vec<_>>())?;
        let f = self
            .fronts
            .fronts
            .get(&id)
            .filter(|f| f.country == country)
            .ok_or("Выберите свой фронт")?;
        let army = f.army;
        let group = f.group;
        if selection.iter().any(|d| {
            !self.command.membership.get(d).is_some_and(|a| {
                *a == army
                    || group.is_some_and(|g| {
                        self.command
                            .armies
                            .get(a)
                            .is_some_and(|a| a.group == Some(g))
                    })
            })
        }) {
            return Err("Выбранные дивизии должны принадлежать этой армии или группе".into());
        }
        for f in self
            .fronts
            .fronts
            .values_mut()
            .filter(|f| f.country == country)
        {
            f.assigned_divisions.retain(|d| !selection.contains(d));
            f.assignments.retain(|d, _| !selection.contains(d));
            f.reserves.retain(|d| !selection.contains(d));
            f.command_revision = u64::MAX;
            f.revision += 1;
        }
        let f = self.fronts.fronts.get_mut(&id).unwrap();
        f.assigned_divisions.extend(selection);
        f.auto_include = false;
        self.fronts.revision += 1;
        self.resume_plans(country, &selection.iter().copied().collect::<Vec<_>>())?;
        self.clean_references();
        Ok(())
    }
    pub fn refresh_automatic_orders(&mut self, ids: &BTreeSet<u64>) {
        for id in ids {
            if self.fronts.manual.contains(id) || self.fronts.detached.contains(id) {
                continue;
            }
            if let Some(d) = self.divisions.get_mut(id).filter(|d| {
                d.battle.is_none()
                    && !matches!(
                        d.order,
                        DivisionOrder::Retreat { .. } | DivisionOrder::Train { .. }
                    )
            }) {
                d.order = DivisionOrder::Hold;
                d.destination = None;
                d.waypoints.clear();
            }
        }
    }
    pub fn merge_fronts(
        &mut self,
        world: &World,
        country: &str,
        a: u64,
        b: u64,
    ) -> Result<(), String> {
        self.fronts.merge(world, country, a, b)?;
        let primary: BTreeSet<_> = self
            .plans
            .plans
            .values()
            .filter(|p| p.source_front == Some(a))
            .map(|p| match p.order_type {
                crate::operations::OrderType::Offensive
                | crate::operations::OrderType::Spearhead => 0,
                crate::operations::OrderType::Fallback => 1,
                crate::operations::OrderType::AreaDefense => 2,
            })
            .collect();
        self.plans.plans.retain(|_, p| {
            p.source_front != Some(b)
                || !primary.contains(&match p.order_type {
                    crate::operations::OrderType::Offensive
                    | crate::operations::OrderType::Spearhead => 0,
                    crate::operations::OrderType::Fallback => 1,
                    crate::operations::OrderType::AreaDefense => 2,
                })
        });
        for p in self
            .plans
            .plans
            .values_mut()
            .filter(|p| p.source_front == Some(b))
        {
            p.source_front = Some(a);
            p.source_revision = u64::MAX;
            p.last_review = 0;
        }
        self.plans.revision += 1;
        self.clean_references();
        Ok(())
    }
    pub fn delete_front(&mut self, country: &str, id: u64) -> Result<(), String> {
        let members = self
            .fronts
            .fronts
            .get(&id)
            .filter(|f| f.country == country)
            .ok_or("Выберите свой фронт")?
            .assigned_divisions
            .clone();
        self.fronts.delete(country, id)?;
        self.refresh_automatic_orders(&members);
        self.clean_references();
        Ok(())
    }
    pub fn delete_plan(&mut self, country: &str, id: u64) -> Result<(), String> {
        let members = self
            .plans
            .plans
            .get(&id)
            .filter(|p| p.country == country)
            .ok_or("Выберите свой план")?
            .assigned_divisions
            .clone();
        self.plans.delete(country, id)?;
        self.refresh_automatic_orders(&members);
        self.clean_references();
        Ok(())
    }
    pub fn resume_plans(&mut self, country: &str, ids: &[u64]) -> Result<(), String> {
        self.validate_selection(country, ids)?;
        for id in ids {
            self.fronts.manual.remove(id);
            self.fronts.manual_return.remove(id);
            self.fronts.detached.remove(id);
            if let Some(d) = self.divisions.get_mut(id) {
                if d.battle.is_none() && !matches!(d.order, DivisionOrder::Retreat { .. }) {
                    d.order = DivisionOrder::Hold;
                    d.destination = None;
                    d.waypoints.clear();
                }
            }
        }
        for f in self.fronts.fronts.values_mut() {
            f.command_revision = u64::MAX;
        }
        for p in self.plans.plans.values_mut() {
            p.last_review = 0;
        }
        Ok(())
    }
    pub fn detach_plans(&mut self, country: &str, ids: &[u64]) -> Result<(), String> {
        self.validate_selection(country, ids)?;
        self.fronts.detached.extend(ids.iter().copied());
        self.fronts.manual.extend(ids.iter().copied());
        self.fronts.manual_return.retain(|id| !ids.contains(id));
        for f in self.fronts.fronts.values_mut() {
            f.command_revision = u64::MAX;
        }
        Ok(())
    }
    pub fn create_army(&mut self, country: &str, ids: &[u64], name: &str) -> Result<u64, String> {
        self.validate_selection(country, ids)?;
        Ok(self.command.create_army(country, name, ids))
    }
    pub fn assign_army(&mut self, country: &str, army: u64, ids: &[u64]) -> Result<(), String> {
        self.validate_selection(country, ids)?;
        self.command.assign(country, army, ids)
    }
    fn validate_selection(&self, country: &str, ids: &[u64]) -> Result<(), String> {
        if ids.is_empty() {
            return Err("Сначала выделите дивизии".into());
        }
        if ids.iter().any(|id| {
            !self
                .divisions
                .get(id)
                .is_some_and(|d| d.country == country && !d.destroyed)
        }) {
            return Err("В выделении есть чужая или отсутствующая дивизия".into());
        }
        Ok(())
    }
    pub fn save_template(
        &mut self,
        country: &str,
        old_name: Option<&str>,
        template: DivisionTemplate,
    ) -> Result<(), String> {
        let army = self.countries.get_mut(country).ok_or("No country army")?;
        self.catalogue
            .validate_template(&template, &army.modifiers, true)?;
        if template.name.trim().is_empty() {
            return Err("Template needs a name".into());
        }
        if old_name.is_some_and(|name| !army.templates.contains_key(name)) {
            return Err("Original template is missing".into());
        }
        if old_name.is_some_and(|name| name != template.name)
            && army.templates.contains_key(&template.name)
        {
            return Err("Another template already has this name".into());
        }
        let empty = DivisionTemplate::default();
        let old = old_name
            .and_then(|id| army.templates.get(id))
            .unwrap_or(&empty);
        if old_name.is_none() && army.templates.contains_key(&template.name) {
            return Err("Template name already exists".into());
        }
        let cost = self
            .catalogue
            .edit_cost_with_modifiers(old, &template, &army.modifiers)?;
        if army.army_xp + 1e-9 < cost {
            return Err(format!(
                "Need {cost:.0} Army XP; available {:.1}",
                army.army_xp
            ));
        }
        if let Some(name) = old_name {
            if name != template.name {
                army.templates.remove(name);
            }
        }
        army.army_xp -= cost;
        army.templates
            .insert(template.name.clone(), template.clone());
        if let Some(old) = old_name {
            if old != template.name {
                for d in self
                    .divisions
                    .values_mut()
                    .filter(|d| d.country == country && d.template == old)
                {
                    d.template = template.name.clone();
                }
            }
        }

        for d in self
            .divisions
            .values_mut()
            .filter(|d| d.country == country && d.template == template.name && !d.destroyed)
        {
            if let Ok(derived) =
                self.catalogue
                    .aggregate(&template, &army.modifiers, Some(&d.equipment))
            {
                // Added battalions are initially unfilled; editing a template
                // must not manufacture strength before actual reinforcements.
                d.hp = d.hp.min(derived.stats.hp);
                d.organization = d.organization.min(derived.stats.organization);
                d.derived = derived;
            }
        }
        Ok(())
    }
    pub fn copy_template(&mut self, country: &str, source: &str, name: &str) -> Result<(), String> {
        let army = self.countries.get_mut(country).ok_or("No country army")?;
        if name.trim().is_empty() || army.templates.contains_key(name) {
            return Err("Укажите свободное имя шаблона".into());
        }
        let mut t = army
            .templates
            .get(source)
            .ok_or("Нет исходного шаблона")?
            .clone();
        t.name = name.into();
        army.templates.insert(name.into(), t);
        Ok(())
    }
    pub fn template_edit_cost(
        &self,
        country: &str,
        old: Option<&str>,
        new: &DivisionTemplate,
    ) -> Result<f64, String> {
        let army = self.countries.get(country).ok_or("No country army")?;
        let empty = DivisionTemplate::default();
        self.catalogue.edit_cost_with_modifiers(
            old.and_then(|id| army.templates.get(id)).unwrap_or(&empty),
            new,
            &army.modifiers,
        )
    }
    pub fn apply_template(
        &mut self,
        country: &str,
        ids: &[u64],
        name: &str,
        production: &mut ProductionState,
    ) -> Result<(), String> {
        self.validate_selection(country, ids)?;
        let army = self.countries.get(country).ok_or("No country army")?;
        let t = army.templates.get(name).ok_or("Нет выбранного шаблона")?;
        self.catalogue.validate_template(t, &army.modifiers, true)?;
        let mut staged = Vec::new();
        for id in ids {
            let d = &self.divisions[id];
            if d.battle.is_some() {
                return Err("Сначала выведите дивизии из боя".into());
            }
            let stats = self
                .catalogue
                .aggregate(t, &army.modifiers, Some(&d.equipment))?;
            staged.push((*id, stats));
        }
        for (id, stats) in staged {
            let d = self.divisions.get_mut(&id).unwrap();
            d.template = name.into();
            d.hp = d.hp.min(stats.stats.hp);
            d.organization = d.organization.min(stats.stats.organization);
            d.derived = stats;
            self.return_surplus(id, production);
        }
        Ok(())
    }
    pub fn return_surplus(&mut self, id: u64, production: &mut ProductionState) {
        let Some(d) = self.divisions.get_mut(&id).filter(|d| !d.destroyed) else {
            return;
        };
        let mut remaining: BTreeMap<_, _> = d
            .derived
            .equipment
            .iter()
            .map(|(k, v)| (k.clone(), v.ceil().max(0.0) as u64))
            .collect();
        let mut models: Vec<_> = d
            .equipment
            .keys()
            .filter_map(|id| self.catalogue.equipment.get(id))
            .collect();
        models.sort_by(|a, b| b.year.cmp(&a.year).then(a.id.cmp(&b.id)));
        for def in models {
            let quantity = d.equipment.get_mut(&def.id).unwrap();
            let needed = remaining.entry(def.archetype.clone()).or_default();
            let keep = (*needed).min(u64::from(*quantity)) as u32;
            let returned = *quantity - keep;
            *needed -= u64::from(keep);
            *quantity = keep;
            if returned > 0 {
                let stock = production
                    .country_mut(&d.country)
                    .model_stockpile
                    .entry(def.id.clone())
                    .or_default();
                *stock = stock.saturating_add(u64::from(returned));
            }
        }
        d.equipment.retain(|_, n| *n > 0);
        let surplus = d
            .manpower
            .saturating_sub(d.derived.stats.manpower.ceil().max(0.0) as u32);
        d.manpower -= surplus;
        if surplus > 0 {
            let army = self.countries.entry(d.country.clone()).or_default();
            army.manpower_pool = army.manpower_pool.saturating_add(u64::from(surplus));
        }
        if let Some(army) = self.countries.get(&d.country) {
            if let Some(t) = army.templates.get(&d.template) {
                if let Ok(derived) =
                    self.catalogue
                        .aggregate(t, &army.modifiers, Some(&d.equipment))
                {
                    d.derived = derived;
                }
            }
        }
    }
    fn refit_preferred_equipment(&mut self, id: u64, production: &mut ProductionState) {
        let Some(d) = self
            .divisions
            .get_mut(&id)
            .filter(|d| !d.destroyed && d.battle.is_none() && d.supply_fraction > 0.0)
        else {
            return;
        };
        let Some(army) = self.countries.get(&d.country) else {
            return;
        };
        let Some(t) = army.templates.get(&d.template) else {
            return;
        };
        let stock = &mut production.country_mut(&d.country).model_stockpile;
        for (archetype, preferred) in &t.equipment_preferences {
            if !self
                .catalogue
                .equipment_available(preferred, &army.modifiers)
            {
                continue;
            }
            if !self
                .catalogue
                .equipment
                .get(preferred)
                .is_some_and(|e| e.archetype == *archetype)
            {
                continue;
            }
            let old: Vec<_> = d
                .equipment
                .keys()
                .filter(|id| {
                    *id != preferred
                        && self
                            .catalogue
                            .equipment
                            .get(*id)
                            .is_some_and(|e| e.archetype == *archetype)
                })
                .cloned()
                .collect();
            for model in old {
                let count = d.equipment[&model];
                let replacement = stock
                    .get(preferred)
                    .copied()
                    .unwrap_or(0)
                    .min(u64::from(count)) as u32;
                if replacement == 0 {
                    continue;
                }
                *stock.get_mut(preferred).unwrap() -= u64::from(replacement);
                *stock.entry(model.clone()).or_default() += u64::from(replacement);
                *d.equipment.get_mut(&model).unwrap() -= replacement;
                *d.equipment.entry(preferred.clone()).or_default() += replacement;
            }
        }
        d.equipment.retain(|_, n| *n > 0);
    }
    pub fn recruit(
        &mut self,
        country: &str,
        template: &str,
        province: u32,
        world: &World,
        production: &mut ProductionState,
    ) -> Result<u64, String> {
        if world.controller_of_province(province) != Some(country) {
            return Err("Recruit in a province controlled by your country".into());
        }
        let army = self.countries.get(country).ok_or("No country army")?;
        let t = army.templates.get(template).ok_or("Unknown template")?;
        self.catalogue.validate_template(t, &army.modifiers, true)?;
        let derived = self.catalogue.aggregate(t, &army.modifiers, None)?;
        if army.manpower_pool < (derived.stats.manpower.ceil() as u64) {
            return Err("Insufficient recruitable manpower; import country laws/history".into());
        }
        let id = self.spawn_history(
            country,
            template,
            province,
            "New division",
            0.0,
            1.0,
            0.0,
            world,
        )?;
        let need = self.divisions[&id].manpower;
        self.countries.get_mut(country).unwrap().manpower_pool -= u64::from(need);
        let d = self.divisions.get_mut(&id).unwrap();
        d.order = DivisionOrder::Train { hours: 0.0 };
        d.organization = 0.0;
        self.replenish(production);
        Ok(id)
    }
    pub fn disband(
        &mut self,
        country: &str,
        id: u64,
        production: &mut ProductionState,
    ) -> Result<(), String> {
        let d = self
            .divisions
            .get(&id)
            .filter(|d| d.country == country && !d.destroyed)
            .ok_or("Unknown own division")?;
        if d.battle.is_some() {
            return Err("Withdraw from combat before disbanding".into());
        }
        let d = self.divisions.remove(&id).unwrap();
        let army = self.countries.get_mut(country).unwrap();
        army.manpower_pool += u64::from(d.manpower);
        let stock = &mut production.country_mut(country).model_stockpile;
        for (model, count) in d.equipment {
            *stock.entry(model).or_default() += u64::from(count);
        }
        Ok(())
    }
    pub fn add_production(
        &mut self,
        country: &str,
        model: &str,
        production: &mut ProductionState,
        rules: &ProductionRules,
    ) -> Result<u64, String> {
        let army = self.countries.get(country).ok_or("No country army")?;
        if !self.catalogue.equipment_available(model, &army.modifiers) {
            return Err("Equipment is locked or its modular design is unresolved".into());
        }
        let equipment = &self.catalogue.equipment[model];
        if equipment.cost <= 0.0 {
            return Err("Equipment has no valid imported production cost".into());
        }
        let category = if equipment.archetype.contains("support") {
            Equipment::SupportKit
        } else if equipment.archetype.contains("motorized") {
            Equipment::Truck
        } else if equipment.archetype.contains("infantry") {
            Equipment::InfantryKit
        } else {
            Equipment::FieldEquipment
        };
        Ok(production
            .country_mut(country)
            .create_model(model, category, 1, rules))
    }
    pub(crate) fn equipment_fraction(&self, d: &Division) -> f64 {
        if d.derived.equipment.is_empty() {
            return 1.0;
        }
        d.derived
            .equipment
            .iter()
            .map(|(archetype, needed)| {
                let actual: u64 = d
                    .equipment
                    .iter()
                    .filter_map(|(id, n)| {
                        self.catalogue
                            .equipment
                            .get(id)
                            .filter(|e| &e.archetype == archetype)
                            .map(|_| u64::from(*n))
                    })
                    .sum();
                (actual as f64 / needed.max(1.0)).clamp(0.0, 1.0)
            })
            .fold(1.0, f64::min)
    }
    pub fn replenish(&mut self, production: &mut ProductionState) {
        let ids: Vec<_> = self.divisions.keys().copied().collect();
        for id in ids {
            self.return_surplus(id, production);
            self.refit_preferred_equipment(id, production);
            let Some(d) = self
                .divisions
                .get(&id)
                .filter(|d| !d.destroyed && d.battle.is_none() && d.supply_fraction > 0.0)
            else {
                continue;
            };
            let tag = d.country.clone();
            let before = self
                .equipment_fraction(d)
                .min(f64::from(d.manpower) / d.derived.stats.manpower.max(1.0));
            let needs = d.derived.equipment.clone();
            let old_equipment = d.equipment.clone();
            let max_manpower = d.derived.stats.manpower.ceil() as u32;
            let stock = &mut production.country_mut(&tag).model_stockpile;
            let d = self.divisions.get_mut(&id).unwrap();
            for (archetype, needed) in needs {
                let actual: u64 = old_equipment
                    .iter()
                    .filter_map(|(model, n)| {
                        self.catalogue
                            .equipment
                            .get(model)
                            .filter(|e| e.archetype == archetype)
                            .map(|_| u64::from(*n))
                    })
                    .sum();
                let mut deficit = (needed.ceil() as u64).saturating_sub(actual);
                let mut models: Vec<_> = stock
                    .keys()
                    .filter_map(|model| {
                        self.catalogue
                            .equipment
                            .get(model)
                            .filter(|e| e.archetype == archetype)
                    })
                    .collect();
                models.sort_by(|a, b| b.year.cmp(&a.year).then_with(|| a.id.cmp(&b.id)));
                for model in models {
                    if deficit == 0 {
                        break;
                    }
                    let available = stock.get_mut(&model.id).unwrap();
                    let taken = deficit.min(*available).min(u64::from(u32::MAX));
                    *available -= taken;
                    *d.equipment.entry(model.id.clone()).or_default() += taken as u32;
                    deficit -= taken;
                }
            }
            let army = self.countries.entry(tag).or_default();
            let men = u64::from(max_manpower.saturating_sub(d.manpower)).min(army.manpower_pool);
            army.manpower_pool -= men;
            d.manpower = d.manpower.saturating_add(men as u32);
            let after = self.equipment_fraction(&self.divisions[&id]).min(
                f64::from(self.divisions[&id].manpower)
                    / self.divisions[&id].derived.stats.manpower.max(1.0),
            );
            let d = self.divisions.get_mut(&id).unwrap();
            d.hp = (d.hp + (after - before).max(0.0) * d.derived.stats.hp)
                .min(d.derived.stats.hp * after);
            if let Some(army) = self.countries.get(&d.country) {
                if let Some(t) = army.templates.get(&d.template) {
                    if let Ok(stats) =
                        self.catalogue
                            .aggregate(t, &army.modifiers, Some(&d.equipment))
                    {
                        d.derived = stats;
                    }
                }
            }
        }
        self.supply_recruitment(production);
    }

    pub(crate) fn edge_allowed(&self, country: &str, from: u32, to: u32, world: &World) -> bool {
        let Some(edge) = world.geography.connection(from, to) else {
            return false;
        };
        if edge.kind == CrossingKind::Impassable {
            return false;
        }
        if edge.kind == CrossingKind::Strait
            && !self
                .strait_access
                .contains(&(country.into(), from.min(to), from.max(to)))
        {
            return false;
        }
        if self.strict_geography && edge.river == RiverCrossing::Unknown {
            return false;
        }
        world
            .geography
            .provinces
            .get(&to)
            .is_some_and(|p| p.definition.kind == ProvinceKind::Land)
    }
    pub fn movement_distances(
        &self,
        country: &str,
        from: u32,
        world: &World,
    ) -> BTreeMap<u32, f64> {
        let Some(scale) = self.catalogue.rules.pixels_to_km else {
            return BTreeMap::new();
        };
        let mut dist = BTreeMap::from([(from, 0.0f64)]);
        let mut heap = BinaryHeap::from([Reverse((0u64, from))]);
        while let Some(Reverse((bits, p))) = heap.pop() {
            let cost = f64::from_bits(bits);
            if cost > dist[&p] {
                continue;
            }
            for edge in world.geography.land_neighbors(p) {
                if !self.edge_allowed(country, p, edge.to, world)
                    || !world
                        .controller_of_province(edge.to)
                        .is_some_and(|c| self.friendly(country, c))
                {
                    continue;
                }
                let Some(d) = world.geography.distance_in_pixels(p, edge.to) else {
                    continue;
                };
                let terrain = &world.geography.provinces[&edge.to].definition.terrain;
                let next = cost
                    + (d * scale * self.catalogue.default_terrain(terrain).movement_cost).max(0.01);
                if next < dist.get(&edge.to).copied().unwrap_or(f64::INFINITY) {
                    dist.insert(edge.to, next);
                    heap.push(Reverse((next.to_bits(), edge.to)));
                }
            }
        }
        dist
    }
    pub fn route(
        &self,
        country: &str,
        from: u32,
        to: u32,
        world: &World,
    ) -> Result<Vec<u32>, String> {
        self.route_for_order(country, from, to, world, false)
    }
    fn route_for_order(
        &self,
        country: &str,
        from: u32,
        to: u32,
        world: &World,
        hostile: bool,
    ) -> Result<Vec<u32>, String> {
        if from == to {
            return Ok(Vec::new());
        }
        let scale = self
            .catalogue
            .rules
            .pixels_to_km
            .ok_or("Movement requires MAP_SCALE_PIXEL_TO_KM from common/defines")?;
        let mut distances = BTreeMap::from([(from, 0.0f64)]);
        let mut parents = BTreeMap::new();
        let mut heap = BinaryHeap::from([Reverse((0u64, from))]);
        while let Some(Reverse((bits, province))) = heap.pop() {
            let cost = f64::from_bits(bits);
            if cost > distances.get(&province).copied().unwrap_or(f64::INFINITY) {
                continue;
            }
            if province == to {
                break;
            }
            for edge in world.geography.land_neighbors(province) {
                if !self.edge_allowed(country, province, edge.to, world) {
                    continue;
                }
                if !world.controller_of_province(edge.to).is_some_and(|c| {
                    self.friendly(country, c) || (hostile && self.at_war(country, c))
                }) {
                    continue;
                }
                let terrain = &world.geography.provinces[&edge.to].definition.terrain;
                let Some(distance) = world.geography.distance_in_pixels(province, edge.to) else {
                    continue;
                };
                let candidate = cost
                    + (distance * scale * self.catalogue.default_terrain(terrain).movement_cost)
                        .max(0.01);
                if candidate < distances.get(&edge.to).copied().unwrap_or(f64::INFINITY) {
                    distances.insert(edge.to, candidate);
                    parents.insert(edge.to, province);
                    heap.push(Reverse((candidate.to_bits(), edge.to)));
                }
            }
        }
        if !parents.contains_key(&to) {
            return Err(
                "No legal land route (check control, straits and impassable borders)".into(),
            );
        }
        let mut path = vec![to];
        let mut p = to;
        while let Some(&parent) = parents.get(&p) {
            if parent == from {
                break;
            }
            path.push(parent);
            p = parent;
        }
        path.reverse();
        Ok(path)
    }
    fn may_withdraw(&self, d: &Division, b: &LandBattle) -> Result<(), String> {
        let key = if b.origins.contains_key(&d.id) {
            "cannot_retreat_while_attacking"
        } else {
            "cannot_retreat_while_defending"
        };
        if d.ability_modifiers.get(key).is_some_and(|v| *v > 0.0) {
            return Err("Действующая способность запрещает отход".into());
        }
        Ok(())
    }
    pub fn stop(&mut self, country: &str, id: u64, world: &World) -> Result<(), String> {
        let d = self
            .divisions
            .get(&id)
            .filter(|d| d.country == country && !d.destroyed)
            .ok_or("Unknown own division")?;
        if matches!(d.order, DivisionOrder::Retreat { .. }) {
            return Err("A forced retreat cannot be cancelled".into());
        }
        let battle = d.battle.and_then(|id| self.battles.get(&id)).cloned();
        if let Some(b) = &battle {
            self.may_withdraw(d, b)?;
            self.battles
                .get_mut(&b.id)
                .unwrap()
                .withdraw(id, &self.catalogue.rules)?;
            if !b.origins.contains_key(&id) {
                self.retreat(id, b, world);
                if !self.automatic_order {
                    self.fronts.manual.insert(id);
                    self.fronts.manual_return.remove(&id);
                }
                return Ok(());
            }
        }
        let d = self.divisions.get_mut(&id).unwrap();
        d.battle = None;
        d.order = DivisionOrder::Hold;
        d.destination = None;
        d.waypoints.clear();
        if !self.automatic_order {
            self.fronts.manual.insert(id);
            self.fronts.manual_return.remove(&id);
        }
        if battle.is_some() {
            d.attack_delay = self.catalogue.rules.attack_delay;
        }
        Ok(())
    }
    pub fn move_to(
        &mut self,
        country: &str,
        ids: &[u64],
        target: u32,
        world: &World,
        strategic: bool,
    ) -> Result<(), String> {
        self.issue_move(country, ids, target, world, strategic, false)
    }
    pub fn issue_move(
        &mut self,
        country: &str,
        ids: &[u64],
        target: u32,
        world: &World,
        strategic: bool,
        append: bool,
    ) -> Result<(), String> {
        if ids.is_empty() {
            return Err("Select a division first".into());
        }
        if strategic && self.catalogue.rules.strategic_speed.is_none() {
            return Err("Strategic redeployment requires STRATEGIC_SPEED_BASE".into());
        }
        let controller = world
            .controller_of_province(target)
            .ok_or("Choose a controlled land province")?;
        if !self.friendly(country, controller) && !self.at_war(country, controller) {
            return Err(format!(
                "No military access to {controller}; declare war before attacking"
            ));
        }
        let objective = MoveObjective {
            province: target,
            strategic,
        };
        let mut staged = Vec::new();
        let mut routes = BTreeMap::<u32, Vec<u32>>::new();
        for &id in ids {
            let d = self
                .divisions
                .get(&id)
                .filter(|d| d.country == country && !d.destroyed)
                .ok_or("Unknown own division")?;
            if matches!(d.order, DivisionOrder::Retreat { .. }) {
                return Err("This division is retreating".into());
            }
            if d.derived.stats.speed <= 0.0 {
                return Err(format!("{} has no imported movement speed", d.name));
            }
            if append && d.waypoints.len() >= 128 {
                return Err("Maximum 128 queued waypoints".into());
            }
            if d.battle.is_some() && !append {
                let b = &self.battles[&d.battle.unwrap()];
                self.may_withdraw(d, b)?;
                if b.elapsed_hours < self.catalogue.rules.minimum_hours {
                    return Err(format!(
                        "Cannot withdraw before {} combat hours",
                        self.catalogue.rules.minimum_hours
                    ));
                }
            }
            let from = if append {
                d.waypoints
                    .back()
                    .copied()
                    .or(d.destination)
                    .map_or(d.province, |o| o.province)
            } else {
                d.province
            };
            let path = if let Some(path) = routes.get(&from) {
                path.clone()
            } else {
                let path = self.route_for_order(country, from, target, world, !strategic)?;
                routes.insert(from, path.clone());
                path
            };
            if !append {
                if let Some(b) = d
                    .battle
                    .and_then(|id| self.battles.get(&id))
                    .filter(|b| !b.origins.contains_key(&id))
                {
                    let _ = b;
                    if !path.first().is_some_and(|p| {
                        world
                            .controller_of_province(*p)
                            .is_some_and(|c| self.friendly(country, c))
                    }) {
                        return Err(
                            "Choose a friendly retreat destination outside this province".into(),
                        );
                    }
                }
            }
            staged.push((id, path));
        }
        for (id, path) in staged {
            if !self.automatic_order {
                self.fronts.manual.insert(id);
                self.fronts.manual_return.insert(id);
            }
            if append && path.is_empty() {
                continue;
            }
            if append
                && (self.divisions[&id].destination.is_some()
                    || self.divisions[&id].battle.is_some())
            {
                self.divisions
                    .get_mut(&id)
                    .unwrap()
                    .waypoints
                    .push_back(objective);
                continue;
            }
            let defending = self.divisions[&id]
                .battle
                .and_then(|b| self.battles.get(&b))
                .is_some_and(|b| !b.origins.contains_key(&id));
            if self.divisions[&id].battle.is_some() {
                self.stop(country, id, world)?;
            }
            if !self.automatic_order {
                self.fronts.manual_return.insert(id);
            }
            let d = self.divisions.get_mut(&id).unwrap();
            if !append {
                d.waypoints.clear();
            }
            d.destination = if path.is_empty() {
                None
            } else {
                Some(objective)
            };
            d.entrenchment = 0.0;
            d.order = if defending {
                DivisionOrder::Retreat {
                    destination: path[0],
                    progress_km: 0.0,
                }
            } else if path.is_empty() {
                DivisionOrder::Hold
            } else if strategic {
                DivisionOrder::StrategicRedeployment {
                    path,
                    progress_km: 0.0,
                }
            } else {
                DivisionOrder::Move {
                    path,
                    progress_km: 0.0,
                }
            };
        }
        Ok(())
    }
    fn resume_movement(&mut self, id: u64, world: &World) {
        let Some(d) = self.divisions.get_mut(&id).filter(|d| {
            !d.destroyed && d.battle.is_none() && matches!(d.order, DivisionOrder::Hold)
        }) else {
            return;
        };
        if d.destination.is_some_and(|o| o.province == d.province) {
            d.destination = None;
        }
        if d.destination.is_none() {
            d.destination = d.waypoints.pop_front();
        }
        let Some(objective) = d.destination else {
            return;
        };
        let from = d.province;
        let tag = d.country.clone();
        match self.route_for_order(&tag, from, objective.province, world, !objective.strategic) {
            Ok(path) => {
                let d = self.divisions.get_mut(&id).unwrap();
                d.order = if objective.strategic {
                    DivisionOrder::StrategicRedeployment {
                        path,
                        progress_km: 0.0,
                    }
                } else {
                    DivisionOrder::Move {
                        path,
                        progress_km: 0.0,
                    }
                };
            }
            Err(e) => {
                let d = self.divisions.get_mut(&id).unwrap();
                d.destination = None;
                d.waypoints.clear();
                self.notice(format!("Division {id}: {e}"));
            }
        }
    }
    pub(crate) fn environment(
        &self,
        battle: &LandBattle,
        world: &World,
        hour: u8,
    ) -> BattleEnvironment {
        let conditions = self
            .conditions
            .get(&battle.province)
            .cloned()
            .unwrap_or_default();
        let terrain_id = world
            .geography
            .provinces
            .get(&battle.province)
            .map(|p| p.definition.terrain.clone())
            .unwrap_or_else(|| "plains".into());
        let local_hour = world
            .geography
            .provinces
            .get(&battle.province)
            .and_then(|p| p.center)
            .map(|p| {
                (f64::from(hour) + p.x / f64::from(world.geography.width.max(1)) * 24.0 - 12.0)
                    .rem_euclid(24.0)
            })
            .unwrap_or(f64::from(hour));
        let mut env = BattleEnvironment {
            terrain: self.catalogue.default_terrain(&terrain_id),
            terrain_id,
            night: conditions
                .night_override
                .unwrap_or(local_hour < 6.0 || local_hour >= 18.0),
            fort_level: conditions.fort_level,
            attacker_air: conditions.attacker_air,
            defender_air: conditions.defender_air,
            ..Default::default()
        };
        if self.air.enabled && self.air.region_at(battle.province).is_some() {
            let attacker = battle
                .attackers
                .active
                .iter()
                .chain(&battle.attackers.reserves)
                .find_map(|id| self.divisions.get(id))
                .map(|d| d.country.as_str())
                .unwrap_or("");
            let defender = battle
                .defenders
                .active
                .iter()
                .chain(&battle.defenders.reserves)
                .find_map(|id| self.divisions.get(id))
                .map(|d| d.country.as_str())
                .unwrap_or("");
            env.attacker_air =
                self.air
                    .support(attacker, defender, battle.province, self.simulated_hours);
            env.defender_air =
                self.air
                    .support(defender, attacker, battle.province, self.simulated_hours);
            let region = self.air.region_at(battle.province);
            let simultaneous = 1 + self
                .battles
                .values()
                .filter(|b| {
                    b.id != battle.id
                        && b.outcome.is_none()
                        && self.air.region_at(b.province) == region
                })
                .count();
            // A sortie cannot hit every province with the wing's full strength.
            for air in [&mut env.attacker_air, &mut env.defender_air] {
                air.plane_attack_rolls /= simultaneous as f64;
                air.cas_planes /= simultaneous as u32;
            }
            let aa = |ids: &[u64]| {
                ids.iter()
                    .filter_map(|id| self.divisions.get(id))
                    .map(|d| d.derived.stats.air_attack)
                    .sum::<f64>()
                    .max(0.)
            };
            let attacker_aa = aa(&battle.attackers.active);
            let defender_aa = aa(&battle.defenders.active);
            env.attacker_air.aa_damage_reduction = self.catalogue.rules.aa_max_reduction
                * defender_aa
                / (defender_aa + self.air.rules.aa_effect_scale.max(1e-9));
            env.defender_air.aa_damage_reduction = self.catalogue.rules.aa_max_reduction
                * attacker_aa
                / (attacker_aa + self.air.rules.aa_effect_scale.max(1e-9));
        }
        if self.air.enabled && self.air.region_at(battle.province).is_some() {
            let attacker_width = battle
                .attackers
                .active
                .iter()
                .filter_map(|id| self.divisions.get(id))
                .map(|d| d.derived.stats.width)
                .sum::<f64>();
            let defender_width = battle
                .defenders
                .active
                .iter()
                .filter_map(|id| self.divisions.get(id))
                .map(|d| d.derived.stats.width)
                .sum::<f64>();
            env.attacker_air.ground_support_bonus *= (f64::from(env.attacker_air.cas_planes)
                / (defender_width * self.catalogue.rules.cas_planes_per_width).max(1.))
            .min(1.);
            env.defender_air.ground_support_bonus *= (f64::from(env.defender_air.cas_planes)
                / (attacker_width * self.catalogue.rules.cas_planes_per_width).max(1.))
            .min(1.);
        }
        if let Some(weather) = conditions
            .weather
            .and_then(|name| self.catalogue.weather.get(&name))
        {
            env.weather = weather.clone();
        }
        for (id, from) in &battle.origins {
            if let Some(edge) = world.geography.connection(*from, battle.province) {
                let penalty = match edge.river {
                    RiverCrossing::PaletteIndex(i)
                        if i <= self.catalogue.rules.small_river_index =>
                    {
                        self.catalogue.rules.small_river_attack
                    }
                    RiverCrossing::PaletteIndex(i)
                        if i <= self.catalogue.rules.large_river_index =>
                    {
                        self.catalogue.rules.large_river_attack
                    }
                    _ => 0.0,
                };
                env.crossing_penalties.insert(*id, penalty);
            }
        }
        env
    }
    pub fn support_attack(
        &mut self,
        country: &str,
        ids: &[u64],
        province: u32,
        world: &World,
    ) -> Result<u64, String> {
        let occupied = self.divisions.values().any(|d| {
            d.province == province
                && self.at_war(country, &d.country)
                && d.fighting()
                && !matches!(d.order, DivisionOrder::Retreat { .. })
        });
        if !occupied {
            return Err(
                "Поддержка атаки требует действительного противника в целевой провинции".into(),
            );
        }
        let battle = self.attack(country, ids, province, world)?;
        for id in ids {
            let d = self.divisions.get_mut(id).unwrap();
            d.order = DivisionOrder::SupportAttack { province };
            d.destination = None;
            d.waypoints.clear();
        }
        Ok(battle)
    }

    pub fn attack(
        &mut self,
        country: &str,
        ids: &[u64],
        province: u32,
        world: &World,
    ) -> Result<u64, String> {
        let controller = world
            .controller_of_province(province)
            .ok_or("Target province has no controller")?;
        if self.catalogue.rules.pixels_to_km.is_none() {
            return Err(
                "Attack movement requires MAP_SCALE_PIXEL_TO_KM from common/defines".into(),
            );
        }
        if !self.at_war(country, controller) {
            return Err("Declare war before attacking this country".into());
        }
        let mut attackers = Vec::new();
        for id in ids {
            let d = self
                .divisions
                .get(id)
                .filter(|d| d.country == country && d.fighting())
                .ok_or("Unknown/unready own division")?;
            if d.battle.is_some()
                || d.attack_delay > 0
                || matches!(
                    d.order,
                    DivisionOrder::Retreat { .. } | DivisionOrder::Train { .. }
                )
            {
                return Err("Division is busy or still has attack delay".into());
            }
            if !self.edge_allowed(country, d.province, province, world) {
                return Err("Attack requires a passable adjacent land connection".into());
            }
            attackers.push(*id);
        }
        if attackers.is_empty() {
            return Err("Select at least one division".into());
        }
        if let Some(id) = self
            .battles
            .iter()
            .find(|(_, b)| b.province == province && b.outcome.is_none())
            .map(|(id, _)| *id)
        {
            if self.battles[&id].attackers.active.is_empty() {
                return Err("This battle is ending; wait for its final hour".into());
            }
            let first = self.battles[&id]
                .attackers
                .active
                .first()
                .or(self.battles[&id].attackers.reserves.first())
                .and_then(|i| self.divisions.get(i));
            if first.is_some_and(|d| d.country != country) {
                return Err("Another attacking country already commands this battle".into());
            }
            for unit in attackers {
                if !self.automatic_order {
                    self.fronts.manual.insert(unit);
                    self.fronts.manual_return.insert(unit);
                }
                let origin = self.divisions[&unit].province;
                self.battles
                    .get_mut(&id)
                    .unwrap()
                    .add_attacker(unit, origin);
                let d = self.divisions.get_mut(&unit).unwrap();
                d.battle = Some(id);
                d.order = DivisionOrder::Attack { province };
                d.entrenchment = 0.0;
            }
            return Ok(id);
        }
        let defenders: Vec<_> = self
            .divisions
            .iter()
            .filter(|(_, d)| {
                d.province == province
                    && self.at_war(country, &d.country)
                    && d.fighting()
                    && !matches!(d.order, DivisionOrder::Retreat { .. })
            })
            .map(|(id, _)| *id)
            .collect();
        if defenders.is_empty() {
            for id in attackers {
                if !self.automatic_order {
                    self.fronts.manual.insert(id);
                    self.fronts.manual_return.insert(id);
                }
                let d = self.divisions.get_mut(&id).unwrap();
                d.entrenchment = 0.0;
                d.order = DivisionOrder::Move {
                    path: vec![province],
                    progress_km: 0.0,
                };
            }
            return Ok(0); // Empty hostile provinces require travel, not an artificial battle.
        }
        // A division cannot disappear from defense because it is attacking
        // elsewhere. The current operational model interrupts its outgoing order.
        for id in &defenders {
            if let Some(previous) = self.divisions[id].battle {
                if let Some(b) = self.battles.get_mut(&previous) {
                    for side in [&mut b.attackers, &mut b.defenders] {
                        side.active.retain(|v| v != id);
                        side.reserves.retain(|v| v != id);
                    }
                    b.origins.remove(id);
                }
                let d = self.divisions.get_mut(id).unwrap();
                d.battle = None;
                d.order = DivisionOrder::Hold;
            }
        }
        let id = self.next_battle_id;
        self.next_battle_id += 1;
        let preliminary = LandBattle::new(
            id,
            province,
            attackers.clone(),
            defenders.clone(),
            &self.divisions,
            &BattleEnvironment::default(),
            &self.catalogue.rules,
        );
        let env = self.environment(&preliminary, world, 12);
        let battle = LandBattle::new(
            id,
            province,
            attackers.clone(),
            defenders.clone(),
            &self.divisions,
            &env,
            &self.catalogue.rules,
        );
        for unit in attackers.iter().chain(&defenders) {
            if attackers.contains(unit) && !self.automatic_order {
                self.fronts.manual.insert(*unit);
                self.fronts.manual_return.insert(*unit);
            }
            let d = self.divisions.get_mut(unit).unwrap();
            d.battle = Some(id);
            if attackers.contains(unit) {
                d.order = DivisionOrder::Attack { province };
                d.entrenchment = 0.0;
            } else {
                d.order = DivisionOrder::Hold;
            }
        }
        self.battles.insert(id, battle);
        self.notice(format!("Battle {id} started for province {province}"));
        Ok(id)
    }

    fn retreat(&mut self, id: u64, battle: &LandBattle, world: &World) {
        let Some(d) = self.divisions.get(&id) else {
            return;
        };
        if d.destroyed {
            return;
        }
        let origin = d.province;
        let tag = d.country.clone();
        let destination = world
            .geography
            .land_neighbors(origin)
            .filter(|e| {
                self.edge_allowed(&tag, origin, e.to, world)
                    && world
                        .controller_of_province(e.to)
                        .is_some_and(|c| self.friendly(&tag, c))
                    && !battle.origins.values().any(|p| *p == e.to)
            })
            .min_by(|a, b| {
                world
                    .geography
                    .distance_in_pixels(origin, a.to)
                    .unwrap_or(f64::INFINITY)
                    .total_cmp(
                        &world
                            .geography
                            .distance_in_pixels(origin, b.to)
                            .unwrap_or(f64::INFINITY),
                    )
                    .then_with(|| a.to.cmp(&b.to))
            })
            .map(|e| e.to);
        let d = self.divisions.get_mut(&id).unwrap();
        d.battle = None;
        d.entrenchment = 0.0;
        d.attack_delay = self.catalogue.rules.attack_delay;
        d.destination = None;
        d.waypoints.clear();
        if let Some(destination) = destination {
            d.order = DivisionOrder::Retreat {
                destination,
                progress_km: 0.0,
            };
        } else {
            d.destroyed = true;
            d.manpower = 0;
            d.equipment.clear();
            self.notice(format!("Division {id} destroyed without a retreat route"));
        }
    }
    fn finish_battle(&mut self, battle: &LandBattle, world: &World) {
        let Some(outcome) = battle.outcome else {
            return;
        };
        let all: Vec<_> = self
            .divisions
            .iter()
            .filter(|(_, d)| d.battle == Some(battle.id))
            .map(|(id, _)| *id)
            .collect();
        for id in all {
            let was_attacker = battle.origins.contains_key(&id);
            let loser = match outcome {
                BattleOutcome::AttackerWon => !was_attacker,
                BattleOutcome::DefenderWon => was_attacker,
                BattleOutcome::Draw => true,
            };
            if loser && !was_attacker {
                self.retreat(id, battle, world);
                continue;
            }
            let d = self.divisions.get_mut(&id).unwrap();
            d.battle = None;
            d.attack_delay = self.catalogue.rules.attack_delay;
            if was_attacker
                && outcome == BattleOutcome::AttackerWon
                && !d.destroyed
                && !matches!(d.order, DivisionOrder::SupportAttack { .. })
            {
                d.order = DivisionOrder::Move {
                    path: vec![battle.province],
                    progress_km: 0.0,
                };
            } else {
                d.order = DivisionOrder::Hold;
                d.destination = None;
                d.waypoints.clear();
            }
        }
        self.notice(format!("Battle {} ended: {:?}", battle.id, outcome));
    }

    fn movement_step(&mut self, world: &mut World) {
        let Some(scale) = self.catalogue.rules.pixels_to_km else {
            return;
        };
        let ids: Vec<_> = self.divisions.keys().copied().collect();
        for id in ids {
            let mut time_left = 1.0;
            // Preserve the unused fraction of the hour across short provinces.
            // A route is finite; this bound also protects against zero-length edges.
            for _ in 0..256 {
                self.resume_movement(id, world);
                let Some(d) = self
                    .divisions
                    .get(&id)
                    .filter(|d| !d.destroyed && d.battle.is_none())
                else {
                    break;
                };
                let (path, progress, retreat, strategic) = match &d.order {
                    DivisionOrder::Move { path, progress_km } => {
                        (path.clone(), *progress_km, false, false)
                    }
                    DivisionOrder::StrategicRedeployment { path, progress_km } => {
                        (path.clone(), *progress_km, false, true)
                    }
                    DivisionOrder::Retreat {
                        destination,
                        progress_km,
                    } => (vec![*destination], *progress_km, true, false),
                    _ => break,
                };
                let Some(&next) = path.first() else {
                    self.divisions.get_mut(&id).unwrap().order = DivisionOrder::Hold;
                    break;
                };
                if !self.edge_allowed(&d.country, d.province, next, world) {
                    self.divisions.get_mut(&id).unwrap().order = DivisionOrder::Hold;
                    self.notice(format!("Division {id}: route became impassable"));
                    break;
                }
                let hostile = world
                    .controller_of_province(next)
                    .is_some_and(|c| self.at_war(&d.country, c));
                let blocked = self.divisions.values().any(|enemy| {
                    enemy.province == next
                        && self.at_war(&d.country, &enemy.country)
                        && enemy.fighting()
                        && !matches!(enemy.order, DivisionOrder::Retreat { .. })
                });
                if hostile && blocked {
                    if d.attack_delay > 0 {
                        break;
                    }
                    if !retreat {
                        let tag = d.country.clone();
                        self.divisions.get_mut(&id).unwrap().order = DivisionOrder::Hold;
                        if let Err(e) = self.attack(&tag, &[id], next, world) {
                            self.notice(e);
                        }
                    }
                    break;
                }
                if !hostile
                    && !world
                        .controller_of_province(next)
                        .is_some_and(|c| self.friendly(&d.country, c))
                {
                    self.divisions.get_mut(&id).unwrap().order = DivisionOrder::Hold;
                    break;
                }
                let terrain_id = &world.geography.provinces[&next].definition.terrain;
                let terrain = self.catalogue.default_terrain(terrain_id);
                let adjust = d
                    .derived
                    .terrain
                    .get(terrain_id)
                    .map_or(0.0, |a| a.movement);
                let mut speed =
                    d.derived.stats.speed * (1.0 + adjust).max(0.0) / terrain.movement_cost;
                if d.derived.stats.fuel > 0.0 {
                    let fuel = d.fuel_fraction.clamp(0.0, 1.0);
                    speed *= self.catalogue.rules.fuel_speed_penalty.map_or_else(
                        || {
                            if self.logistics.enabled {
                                self.logistics.rules.minimum_fuel_speed
                                    + (1. - self.logistics.rules.minimum_fuel_speed) * fuel
                            } else {
                                fuel
                            }
                        },
                        |minimum| minimum + (1.0 - minimum) * fuel,
                    );
                }
                if let (Some(maximum), Some(impact)) = (
                    self.catalogue.infrastructure_max,
                    self.catalogue
                        .literal_defines
                        .get("NDefines.NMilitary.INFRASTRUCTURE_MOVEMENT_SPEED_IMPACT"),
                ) {
                    let infrastructure = world
                        .provinces
                        .get(&next)
                        .and_then(|p| p.state_id)
                        .and_then(|id| world.states.get(&id))
                        .map_or(0, |s| s.economy.infrastructure);
                    speed *=
                        (1.0 + impact * f64::from(maximum.saturating_sub(infrastructure))).max(0.0);
                }
                speed *= (1.0
                    + d.command.movement
                    + d.ability_modifiers
                        .get("army_speed_factor")
                        .copied()
                        .unwrap_or(0.0))
                .max(0.0);
                let supply = d.supply_fraction.clamp(0.0, 1.0);
                speed *= if self.logistics.enabled {
                    self.logistics.rules.minimum_supply_speed
                        + (1. - self.logistics.rules.minimum_supply_speed) * supply
                } else {
                    supply
                };
                let org_fraction =
                    (d.organization / d.derived.stats.organization.max(1e-9)).clamp(0.0, 1.0);
                speed *= 1.0 + self.catalogue.rules.zero_org_speed * (1.0 - org_fraction);
                if retreat {
                    speed *= 1.0 + self.catalogue.rules.retreat_speed;
                }
                if strategic {
                    speed = self.catalogue.rules.strategic_speed.unwrap_or(0.0);
                }
                if let Some(weather) = self
                    .conditions
                    .get(&next)
                    .and_then(|c| c.weather.as_ref())
                    .and_then(|id| self.catalogue.weather.get(id))
                {
                    speed *=
                        (1.0 + weather.get("army_speed_factor").copied().unwrap_or(0.0)).max(0.0);
                }
                if let Some(edge) = world.geography.connection(d.province, next) {
                    let penalty = match edge.river {
                        RiverCrossing::PaletteIndex(i)
                            if i <= self.catalogue.rules.small_river_index =>
                        {
                            self.catalogue.rules.small_river_movement
                        }
                        RiverCrossing::PaletteIndex(i)
                            if i <= self.catalogue.rules.large_river_index =>
                        {
                            self.catalogue.rules.large_river_movement
                        }
                        _ => 0.0,
                    };
                    speed *= (1.0 + penalty).max(0.0);
                }
                if speed <= 0.0 || !speed.is_finite() {
                    break;
                }
                let distance = world
                    .geography
                    .distance_in_pixels(d.province, next)
                    .unwrap_or(f64::INFINITY)
                    * scale;
                let needed = (distance - progress).max(0.0) / speed;
                let spent = needed.min(time_left);

                let tag = d.country.clone();
                let previous = d.province;
                let d = self.divisions.get_mut(&id).unwrap();

                if strategic {
                    d.organization = d
                        .organization
                        .min(d.derived.stats.organization * self.catalogue.rules.strategic_org);
                }
                if needed > time_left + 1e-9 {
                    let progress_km = progress + speed * time_left;
                    d.order = if retreat {
                        DivisionOrder::Retreat {
                            destination: next,
                            progress_km,
                        }
                    } else if strategic {
                        DivisionOrder::StrategicRedeployment { path, progress_km }
                    } else {
                        DivisionOrder::Move { path, progress_km }
                    };
                    break;
                }
                time_left = (time_left - spent).max(0.0);
                let remaining = path[1..].to_vec();
                d.province = next;
                d.entrenchment = 0.0;
                d.order = if remaining.is_empty() {
                    DivisionOrder::Hold
                } else if strategic {
                    DivisionOrder::StrategicRedeployment {
                        path: remaining,
                        progress_km: 0.0,
                    }
                } else {
                    DivisionOrder::Move {
                        path: remaining,
                        progress_km: 0.0,
                    }
                };
                if hostile {
                    world.set_province_controller(next, Some(&tag));
                    self.notice(format!("{tag} controls province {next}"));
                    let overrun: Vec<_> = self
                        .divisions
                        .values()
                        .filter(|d| {
                            d.id != id
                                && d.province == next
                                && self.at_war(&tag, &d.country)
                                && (!d.fighting()
                                    || matches!(d.order, DivisionOrder::Retreat { .. }))
                        })
                        .map(|d| d.id)
                        .collect();
                    for enemy in overrun {
                        let d = self.divisions.get_mut(&enemy).unwrap();
                        d.destroyed = true;
                        d.manpower = 0;
                        d.equipment.clear();
                    }
                }
                let incoming = self
                    .battles
                    .iter()
                    .find(|(_, b)| b.province == next && b.outcome.is_none())
                    .map(|(id, _)| *id);
                if let Some(battle) = incoming {
                    if world.controller_of_province(next) == Some(tag.as_str()) {
                        self.battles.get_mut(&battle).unwrap().add_defender(id);
                        self.divisions.get_mut(&id).unwrap().battle = Some(battle);
                        break;
                    }
                }
                if retreat {
                    self.notice(format!("Division {id} retreated from {previous} to {next}"));
                }
                if time_left <= 1e-9 {
                    break;
                }
            }
        }
    }

    fn complete_manual_orders(&mut self) {
        // Saves made before temporary manual control was introduced still carry
        // manual moving units. Persistent Hold/Exercise/detach stays untouched.
        self.fronts.manual_return.extend(
            self.fronts
                .manual
                .iter()
                .filter(|id| {
                    !self.fronts.detached.contains(id)
                        && self.divisions.get(id).is_some_and(|d| {
                            matches!(
                                d.order,
                                DivisionOrder::Move { .. }
                                    | DivisionOrder::StrategicRedeployment { .. }
                                    | DivisionOrder::Attack { .. }
                                    | DivisionOrder::SupportAttack { .. }
                            )
                        })
                })
                .copied(),
        );
        let completed: Vec<_> = self
            .fronts
            .manual_return
            .iter()
            .copied()
            .filter(|id| {
                !self.fronts.detached.contains(id)
                    && self.divisions.get(id).is_none_or(|d| {
                        d.destroyed
                            || (d.battle.is_none()
                                && matches!(d.order, DivisionOrder::Hold)
                                && d.destination.is_none()
                                && d.waypoints.is_empty())
                    })
            })
            .collect();
        if completed.is_empty() {
            return;
        }
        for id in &completed {
            self.fronts.manual.remove(id);
            self.fronts.manual_return.remove(id);
        }
        for f in self.fronts.fronts.values_mut() {
            if completed.iter().any(|id| f.assigned_divisions.contains(id)) {
                f.command_revision = u64::MAX;
            }
        }
        for p in self.plans.plans.values_mut() {
            if completed.iter().any(|id| p.assigned_divisions.contains(id)) {
                p.last_review = 0;
            }
        }
    }

    pub fn tick_hour(
        &mut self,
        world: &mut World,
        _date: GameDate,
        hour: u8,
        elapsed: u64,
        player: Option<&str>,
    ) {
        self.simulated_hours = elapsed;
        let mut logistics = std::mem::take(&mut self.logistics);
        logistics.tick(self, world, elapsed);
        self.logistics = logistics;
        let mut air = std::mem::take(&mut self.air);
        air.tick(self, world, elapsed);
        self.air = air;
        self.complete_manual_orders();
        self.tick_recruitment(world);
        let initial_control_revision = world.control_revision;
        self.fronts.active_orders = self.plans.active_divisions();
        let mut fronts = std::mem::take(&mut self.fronts);
        self.automatic_order = true;
        fronts.tick(self, world);
        self.automatic_order = false;
        self.fronts = fronts;
        let mut plans = std::mem::take(&mut self.plans);
        self.automatic_order = true;
        plans.tick(self, world, elapsed);
        self.automatic_order = false;
        self.plans = plans;
        self.command.garrison_armies = self
            .plans
            .plans
            .values()
            .filter(|p| p.order_type == crate::operations::OrderType::AreaDefense)
            .map(|p| p.army)
            .collect();
        self.abilities.tick(elapsed);
        self.tick_command_power();
        self.automatic_order = true;
        self.tick_headquarters(world);
        self.automatic_order = false;
        self.command
            .membership
            .retain(|id, _| self.divisions.get(id).is_some_and(|d| !d.destroyed));
        let bonuses: BTreeMap<_, _> = self
            .divisions
            .keys()
            .map(|&id| {
                let mut b = self.command.bonuses(id);
                b.add(self.hq_bonus(id), 1.0);
                (id, (b, self.unit_ability_modifiers(id)))
            })
            .collect();
        for (id, (command, ability)) in bonuses {
            let d = self.divisions.get_mut(&id).unwrap();
            d.command = command;
            d.ability_modifiers = ability;
        }
        if self.divisions.is_empty() {
            return;
        }
        for d in self.divisions.values_mut().filter(|d| !d.destroyed) {
            d.attack_delay = d.attack_delay.saturating_sub(1);
            if d.battle.is_none() {
                let supply = 1.0
                    + (1.0 - d.supply_fraction.clamp(0.0, 1.0))
                        * self.catalogue.rules.out_of_supply_recovery;
                let moving = matches!(
                    d.order,
                    DivisionOrder::Move { .. }
                        | DivisionOrder::Retreat { .. }
                        | DivisionOrder::StrategicRedeployment { .. }
                );
                let movement = if moving {
                    (1.0 + self.catalogue.rules.movement_recovery).max(0.0)
                } else {
                    1.0
                };
                d.organization = (d.organization + d.derived.stats.recovery * supply * movement)
                    .min(d.derived.stats.organization);
                if matches!(d.order, DivisionOrder::Hold) {
                    d.entrenchment = (d.entrenchment
                        + self.catalogue.rules.dig_in_daily
                            * (1.0
                                + d.ability_modifiers
                                    .get("dig_in_speed")
                                    .copied()
                                    .unwrap_or(0.0))
                            .max(0.0)
                            / 24.0)
                        .min(self.catalogue.rules.dig_in_cap + d.derived.stats.entrenchment);
                }
                let strength = d.strength();
                if let DivisionOrder::Train { hours } = &mut d.order {
                    *hours += strength;
                    if *hours >= d.derived.stats.training * 24.0 {
                        d.order = DivisionOrder::Hold;
                        d.experience = ExperienceLevel::Trained;
                    }
                }
            }
            let manual = self.fronts.manual.contains(&d.id) || self.fronts.detached.contains(&d.id);
            let at_front = self.fronts.fronts.values().any(|f| {
                f.assigned_divisions.contains(&d.id)
                    && f.edges.iter().any(|e| e.friendly == d.province)
            });
            let modifiers = self.countries.get(&d.country).map(|a| &a.modifiers);
            let get = |key: &str| modifiers.map_or(0.0, |m| m.value(key));
            let cap = (self.catalogue.rules.planning_cap
                + get("max_planning")
                + d.command.planning_max
                + d.ability_modifiers
                    .get("max_planning")
                    .copied()
                    .unwrap_or(0.0))
            .max(0.0);
            if at_front && !manual && d.battle.is_none() && matches!(d.order, DivisionOrder::Hold) {
                let speed = (1.0
                    + get("planning_speed")
                    + d.command.planning_speed
                    + d.ability_modifiers
                        .get("planning_speed")
                        .copied()
                        .unwrap_or(0.0))
                .max(0.0)
                    * (1.0 + d.derived.stats.initiative).max(0.0);
                d.planning =
                    (d.planning + self.catalogue.rules.planning_daily * speed / 24.0).min(cap);
            } else if manual || d.battle.is_some() || !matches!(d.order, DivisionOrder::Hold) {
                let decay = if manual {
                    self.catalogue.rules.manual_planning_decay
                } else {
                    self.catalogue.rules.planning_decay
                };
                d.planning = (d.planning - decay / 24.0).max(0.0);
            }
        }
        self.automatic_order = true;
        self.movement_step(world);
        self.automatic_order = false;
        let battle_ids: Vec<_> = self
            .battles
            .iter()
            .filter(|(_, b)| b.outcome.is_none())
            .map(|(id, _)| *id)
            .collect();
        let modifiers = self
            .countries
            .iter()
            .map(|(tag, a)| (tag.clone(), a.modifiers.clone()))
            .collect();
        for id in battle_ids {
            let mut battle = self.battles.remove(&id).unwrap();
            let active: Vec<_> = battle
                .attackers
                .active
                .iter()
                .chain(&battle.defenders.active)
                .copied()
                .collect();
            self.command.gain_combat_xp(&active);
            self.command.advance_levels(&mut self.rng);
            let env = self.environment(&battle, world, hour);
            if let Some(trace) = battle.step(
                &mut self.divisions,
                &self.catalogue,
                &modifiers,
                &env,
                &mut self.rng,
            ) {
                for unit in trace.exited {
                    if !battle.origins.contains_key(&unit) {
                        self.retreat(unit, &battle, world);
                    } else if let Some(d) = self.divisions.get_mut(&unit) {
                        d.battle = None;
                        d.order = DivisionOrder::Hold;
                        d.destination = None;
                        d.waypoints.clear();
                        d.attack_delay = self.catalogue.rules.attack_delay;
                    }
                }
            }
            if battle.outcome.is_some() {
                self.finish_battle(&battle, world);
            }
            self.battles.insert(id, battle);
        }
        // Bounded completed battle archive. Active battle state is never discarded.
        let completed: Vec<_> = self
            .battles
            .iter()
            .filter(|(_, b)| b.outcome.is_some())
            .map(|(id, _)| *id)
            .collect();
        for id in completed.iter().take(completed.len().saturating_sub(32)) {
            self.battles.remove(id);
        }
        if self.ai_enabled && elapsed % 6 == 0 {
            let candidates: Vec<_> = self
                .divisions
                .iter()
                .filter(|(_, d)| {
                    Some(d.country.as_str()) != player
                        && !self.command.membership.contains_key(&d.id)
                        && d.fighting()
                        && d.battle.is_none()
                        && d.attack_delay == 0
                        && matches!(d.order, DivisionOrder::Hold)
                })
                .filter_map(|(id, d)| {
                    world
                        .geography
                        .land_neighbors(d.province)
                        .find(|e| {
                            self.edge_allowed(&d.country, d.province, e.to, world)
                                && world
                                    .controller_of_province(e.to)
                                    .is_some_and(|c| self.at_war(&d.country, c))
                        })
                        .map(|e| (d.country.clone(), *id, e.to))
                })
                .take(8)
                .collect();
            for (tag, id, target) in candidates {
                let _ = self.attack(&tag, &[id], target, world);
            }
        }
        if world.control_revision != initial_control_revision {
            self.fronts.active_orders = self.plans.active_divisions();
            let mut fronts = std::mem::take(&mut self.fronts);
            self.automatic_order = true;
            fronts.tick(self, world);
            self.automatic_order = false;
            self.fronts = fronts;
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::map_graph::MapGraph;
    use crate::military_model::tests::{catalogue, division, template};
    fn army() -> Warfare {
        let mut c = catalogue();
        c.rules.pixels_to_km = Some(1.0);
        let mut w = Warfare::new(c);
        w.ai_enabled = false;
        w
    }
    #[test]
    fn support_attack_never_occupies_and_movement_modifies_regain_instead_of_draining_org() {
        let mut world = world();
        let mut w = army();
        let a = division(1, "A", 1);
        let mut support = division(2, "A", 1);
        let b = division(3, "B", 2);
        support.organization = 20.0;
        support.derived.stats.recovery = 1.0;
        support.derived.stats.speed = 0.01;
        w.divisions = BTreeMap::from([(1, a), (2, support), (3, b)]);
        w.declare_war("A", "B", &world).unwrap();
        let id = w.attack("A", &[1], 2, &world).unwrap();
        assert_eq!(w.support_attack("A", &[2], 2, &world).unwrap(), id);
        let mut battle = w.battles[&id].clone();
        battle.outcome = Some(BattleOutcome::AttackerWon);
        w.finish_battle(&battle, &world);
        assert!(matches!(w.divisions[&2].order, DivisionOrder::Hold));
        assert!(matches!(w.divisions[&1].order, DivisionOrder::Move { .. }));
        assert_eq!(w.divisions[&2].province, 1);
        w.battles.clear();
        w.divisions.remove(&1);
        w.divisions.remove(&3);
        world.set_province_controller(2, Some("A"));
        w.issue_move("A", &[2], 2, &world, false, false).unwrap();
        w.tick_hour(&mut world, GameDate::START, 12, 1, Some("A"));
        assert!((w.divisions[&2].organization - 20.6).abs() < 1e-9);
        assert_eq!(w.divisions[&2].province, 1);
        assert!(w.support_attack("A", &[2], 3, &world).is_err());
    }
    #[test]
    fn replacing_a_move_cancels_the_previous_waypoints_and_stop_clears_intent() {
        let mut world = world();
        world.set_province_controller(2, Some("A"));
        world.set_province_controller(3, Some("A"));
        let mut w = army();
        w.divisions.insert(1, division(1, "A", 1));
        w.issue_move("A", &[1], 2, &world, false, false).unwrap();
        w.issue_move("A", &[1], 3, &world, false, true).unwrap();
        assert_eq!(w.divisions[&1].waypoints.len(), 1);
        w.issue_move("A", &[1], 3, &world, false, false).unwrap();
        assert!(w.divisions[&1].waypoints.is_empty());
        w.stop("A", 1, &world).unwrap();
        assert!(w.divisions[&1].destination.is_none());
        assert!(matches!(w.divisions[&1].order, DivisionOrder::Hold));
    }
    #[test]
    fn map_orders_accept_a_distant_enemy_goal_and_shift_queue_continues_after_capture() {
        let mut world = world();
        let mut w = army();
        w.divisions.insert(1, division(1, "A", 1));
        w.declare_war("A", "B", &world).unwrap();
        w.issue_move("A", &[1], 3, &world, false, false).unwrap();
        assert!(matches!(&w.divisions[&1].order,DivisionOrder::Move{path,..} if path==&vec![2,3]));
        w.issue_move("A", &[1], 1, &world, false, true).unwrap();
        assert_eq!(w.divisions[&1].waypoints.len(), 1);
        for _ in 0..3 {
            w.movement_step(&mut world);
        }
        assert_eq!(world.controller_of_province(2), Some("A"));
        assert_eq!(world.controller_of_province(3), Some("A"));
        assert_eq!(w.divisions[&1].province, 1);
        assert!(w.divisions[&1].waypoints.is_empty());
    }
    #[test]
    fn queued_orders_survive_a_battle_without_requiring_another_click() {
        let mut world = world();
        let mut w = army();
        w.catalogue = std::sync::Arc::new({
            let mut c = (*w.catalogue).clone();
            c.rules.minimum_hours = 1;
            c.rules.attack_delay = 0;
            c
        });
        w.declare_war("A", "B", &world).unwrap();
        let mut a = division(1, "A", 1);
        a.derived.stats.soft_attack = 100000.0;
        w.divisions.insert(1, a);
        w.divisions.insert(2, division(2, "B", 2));
        w.issue_move("A", &[1], 3, &world, false, false).unwrap();
        for hour in 1..10 {
            w.tick_hour(&mut world, GameDate::START, 12, hour, Some("A"));
        }
        assert_eq!(w.divisions[&1].province, 3);
        assert_eq!(world.controller_of_province(3), Some("A"));
    }
    #[test]
    fn a_missing_model_does_not_erase_a_historical_division() {
        let world = world();
        let mut c = catalogue();
        c.units
            .get_mut("infantry")
            .unwrap()
            .need
            .insert("unknown_equipment".into(), 10.0);
        let mut w = Warfare::new(c);
        let mut a = CountryArmy::default();
        a.templates.insert("test".into(), template());
        w.countries.insert("A".into(), a);
        let id = w
            .spawn_history("A", "test", 1, "Existing formation", 1.0, 1.0, 0.3, &world)
            .unwrap();
        assert!(w.divisions.contains_key(&id));
        assert_eq!(w.divisions[&id].manpower, 1000);
        assert!(w.divisions[&id].equipment.is_empty());
        assert!(
            w.diagnostics
                .iter()
                .any(|s| s.contains("unknown_equipment"))
        );
    }
    #[test]
    fn adding_a_battalion_spends_xp_without_creating_people_equipment_or_hp() {
        let mut w = army();
        let mut a = CountryArmy {
            army_xp: 10.0,
            ..Default::default()
        };
        a.templates.insert("test".into(), template());
        w.countries.insert("A".into(), a);
        let d = division(1, "A", 1);
        let hp = d.hp;
        let men = d.manpower;
        w.divisions.insert(1, d);
        let mut t = template();
        t.line.push(BattalionSlot {
            unit: "infantry".into(),
            column: 0,
            row: 1,
        });
        w.save_template("A", Some("test"), t).unwrap();
        assert_eq!(w.countries["A"].army_xp, 5.0);
        assert_eq!(w.divisions[&1].hp, hp);
        assert_eq!(w.divisions[&1].manpower, men);
        assert_eq!(w.divisions[&1].derived.stats.hp, hp * 2.0);
        assert_eq!(w.divisions[&1].strength(), 0.5);
    }
    #[test]
    fn empty_enemy_territory_uses_travel_and_short_edges_preserve_the_remaining_hour() {
        let mut world = world();
        let mut w = army();
        w.divisions.insert(1, division(1, "A", 1));
        w.declare_war("A", "B", &world).unwrap();
        assert_eq!(w.attack("A", &[1], 2, &world).unwrap(), 0);
        assert!(w.battles.is_empty());
        w.movement_step(&mut world);
        assert_eq!(w.divisions[&1].province, 2);
        world.set_province_controller(3, Some("A"));
        world.set_province_controller(1, Some("A"));
        w.divisions.get_mut(&1).unwrap().province = 1;
        w.move_to("A", &[1], 3, &world, false).unwrap();
        w.movement_step(&mut world);
        assert_eq!(w.divisions[&1].province, 3);
        assert_eq!(w.divisions[&1].organization, 60.0);
    }
    #[test]
    fn defending_withdrawal_obeys_minimum_time_and_uses_a_friendly_retreat_route() {
        let world = world();
        let mut w = army();
        w.declare_war("A", "B", &world).unwrap();
        w.divisions.insert(1, division(1, "A", 1));
        w.divisions.insert(2, division(2, "B", 2));
        let battle = w.attack("A", &[1], 2, &world).unwrap();
        assert!(w.stop("B", 2, &world).is_err());
        assert_eq!(w.divisions[&2].battle, Some(battle));
        w.battles.get_mut(&battle).unwrap().elapsed_hours = 4;
        w.stop("B", 2, &world).unwrap();
        assert!(matches!(
            w.divisions[&2].order,
            DivisionOrder::Retreat { destination: 3, .. }
        ));
    }
    #[test]
    fn an_outgoing_attack_never_makes_the_origin_province_defenseless() {
        let world = world();
        let mut w = army();
        w.declare_war("A", "B", &world).unwrap();
        for (id, tag, p) in [(1, "A", 1), (2, "B", 2), (3, "B", 2)] {
            w.divisions.insert(id, division(id, tag, p));
        }
        let first = w.attack("A", &[1], 2, &world).unwrap();
        // A separate enemy division can pin the outgoing attacker.
        w.divisions.get_mut(&3).unwrap().battle = None;
        {
            let b = w.battles.get_mut(&first).unwrap();
            b.defenders.active.retain(|id| *id != 3);
        }
        let second = w.attack("B", &[3], 1, &world).unwrap();
        assert!(w.battles[&second].defenders.active.contains(&1));
        assert!(!w.battles[&first].attackers.active.contains(&1));
        assert_eq!(w.divisions[&1].battle, Some(second));
    }
    #[test]
    fn replenishment_consumes_models_and_manpower_once_and_refreshes_combat_stats() {
        let world = world();
        let mut c = catalogue();
        c.units
            .get_mut("infantry")
            .unwrap()
            .need
            .insert("rifles".into(), 100.0);
        c.equipment.insert(
            "rifle_1".into(),
            EquipmentDefinition {
                id: "rifle_1".into(),
                archetype: "rifles".into(),
                year: 1936,
                active: true,
                buildable: true,
                stats: StatBlock::from([("soft_attack".into(), 10.0)]),
                cost: 1.0,
                modular: false,
                source: "fixture".into(),
            },
        );
        let mut w = Warfare::new(c);
        w.countries.insert(
            "A".into(),
            CountryArmy {
                manpower_pool: 1000,
                ..Default::default()
            },
        );
        w.countries
            .get_mut("A")
            .unwrap()
            .templates
            .insert("test".into(), template());
        let id = w
            .spawn_history("A", "test", 1, "D", 0.0, 0.5, 0.1, &world)
            .unwrap();
        let mut p = ProductionState::default();
        p.country_mut("A")
            .model_stockpile
            .insert("rifle_1".into(), 80);
        w.replenish(&mut p);
        let hp = w.divisions[&id].hp;
        assert_eq!(w.divisions[&id].equipment["rifle_1"], 80);
        assert_eq!(w.divisions[&id].manpower, 1000);
        assert_eq!(w.countries["A"].manpower_pool, 500);
        assert!(w.divisions[&id].derived.stats.soft_attack > 0.0);
        w.replenish(&mut p);
        assert_eq!(w.divisions[&id].hp, hp);
        assert_eq!(p.country("A").unwrap().model_stockpile["rifle_1"], 0);
    }
    #[test]
    fn dlc_technology_unlocks_and_history_flags_survive_refresh() {
        let mut c = catalogue();
        crate::military_data::import_technologies(
            "technologies={ t={ enable_subunits={ limit={has_dlc=D} new_unit } } }",
            &mut c,
        )
        .unwrap();
        let mut w = Warfare::new(c);
        let mut army = CountryArmy::default();
        army.modifiers.flags.insert("historical".into());
        army.modifiers.dlcs.insert("D".into());
        w.countries.insert("A".into(), army);
        let mut research = ResearchWorld::default();
        research
            .country_mut("A", 3, &Default::default())
            .researched
            .insert("t".into());
        w.refresh_modifiers(&research);
        assert!(w.countries["A"].modifiers.flags.contains("historical"));
        assert!(
            w.countries["A"]
                .modifiers
                .unlocked_units
                .contains("new_unit")
        );
        w.countries.get_mut("A").unwrap().modifiers.dlcs.clear();
        w.refresh_modifiers(&research);
        assert!(
            !w.countries["A"]
                .modifiers
                .unlocked_units
                .contains("new_unit")
        );
    }
    pub fn world() -> World {
        let rgba: Vec<u8> = [1u8, 2, 3]
            .into_iter()
            .flat_map(|v| [v, 0, 0, 255])
            .collect();
        let graph = MapGraph::from_rgba(
            3,
            1,
            &rgba,
            "1;1;0;0;land;false;plains;1\n2;2;0;0;land;false;plains;1\n3;3;0;0;land;false;plains;1",
            false,
        )
        .unwrap();
        let mut world = World::from_map_data(
            &std::collections::HashMap::from([(1, 1), (2, 2), (3, 2)]),
            &std::collections::HashMap::from([(1, "A".into()), (2, "B".into())]),
            &Default::default(),
            &Default::default(),
            &Default::default(),
        );
        world.geography = graph;
        world
    }
    #[test]
    fn wars_and_adjacency_guard_orders_and_capture_keeps_legal_owner() {
        let mut world = world();
        let mut c = catalogue();
        c.rules.pixels_to_km = Some(1.0);
        c.rules.minimum_hours = 1;
        let mut w = Warfare::new(c);
        w.ai_enabled = false;
        let mut d = division(1, "A", 1);
        d.derived.stats.speed = 4.0;
        w.divisions.insert(1, d);
        assert!(w.attack("A", &[1], 2, &world).is_err());
        w.declare_war("A", "B", &world).unwrap();
        assert!(w.attack("A", &[1], 3, &world).is_err());
        w.attack("A", &[1], 2, &world).unwrap();
        w.tick_hour(&mut world, GameDate::START, 12, 1, Some("A"));
        w.tick_hour(&mut world, GameDate::START, 13, 2, Some("A"));
        assert_eq!(world.controller_of_province(2), Some("A"));
        assert_eq!(world.owner_of_province(2), Some("B"));
    }
    #[test]
    fn stockpile_replenishment_uses_actual_models_and_disband_cannot_duplicate() {
        let mut w = Warfare::new(catalogue());
        w.countries.insert("A".into(), CountryArmy::default());
        w.countries
            .get_mut("A")
            .unwrap()
            .templates
            .insert("test".into(), template());
        let mut d = division(1, "A", 1);
        d.equipment.insert("rifle".into(), 7);
        d.manpower = 500;
        w.divisions.insert(1, d);
        let mut p = ProductionState::default();
        w.disband("A", 1, &mut p).unwrap();
        assert_eq!(p.country("A").unwrap().model_stockpile["rifle"], 7);
        assert_eq!(w.countries["A"].manpower_pool, 500);
        assert!(w.disband("A", 1, &mut p).is_err());
    }
}

#[cfg(test)]
mod research_registry_integration {
    use super::*;
    #[test]
    fn registry_effects_change_real_template_stats_without_double_application() {
        let db = crate::military_model::tests::catalogue();
        let template = crate::military_model::tests::template();
        let before = db
            .aggregate(&template, &MilitaryModifiers::default(), None)
            .unwrap();
        let mut registry = crate::research::ResearchCatalogue::default();
        crate::research_data::parse_technology_file("technologies={training={research_cost=1 modifier={army_attack_factor=0.2} infantry={soft_attack=0.3}}}",&mut registry).unwrap();
        let rules = crate::research::ResearchRules::default();
        let mut research = ResearchWorld::default();
        research
            .country_mut("AAA", 1, &rules)
            .grant("training", &registry);
        let mut warfare = Warfare::new(db);
        warfare.use_research_registry(&registry, &research);
        let known = &research.country("AAA").unwrap().researched;
        let m = MilitaryModifiers::from_research(&warfare.catalogue, known);
        let once = warfare.catalogue.aggregate(&template, &m, None).unwrap();
        assert!(once.stats.soft_attack > before.stats.soft_attack);
        warfare.use_research_registry(&registry, &research);
        let twice = warfare
            .catalogue
            .aggregate(
                &template,
                &MilitaryModifiers::from_research(&warfare.catalogue, known),
                None,
            )
            .unwrap();
        assert_eq!(once.stats.soft_attack, twice.stats.soft_attack);
    }
}

#[cfg(test)]
mod designer_transactions {
    use super::*;
    use crate::military_model::tests::{catalogue, template};
    #[test]
    fn editing_and_refilling_conserve_real_equipment_and_manpower() {
        let mut db = catalogue();
        db.units
            .get_mut("infantry")
            .unwrap()
            .need
            .insert("rifles".into(), 100.0);
        let mut artillery = db.units["infantry"].clone();
        artillery.id = "artillery".into();
        artillery.need = BTreeMap::from([("guns".into(), 12.0)]);
        artillery.stats.insert("combat_width".into(), 3.0);
        artillery.stats.insert("manpower".into(), 500.0);
        db.units.insert("artillery".into(), artillery);
        for (id, archetype, attack) in [("rifle_1", "rifles", 10.0), ("gun_1", "guns", 30.0)] {
            db.equipment.insert(
                id.into(),
                EquipmentDefinition {
                    id: id.into(),
                    archetype: archetype.into(),
                    year: 1936,
                    active: true,
                    buildable: true,
                    stats: BTreeMap::from([("soft_attack".into(), attack)]),
                    cost: 1.0,
                    modular: false,
                    source: "authored fixture".into(),
                },
            );
        }
        let mut w = Warfare::new(db);
        w.countries.insert(
            "A".into(),
            CountryArmy {
                manpower_pool: 1000,
                ..Default::default()
            },
        );
        let t = template();
        w.countries
            .get_mut("A")
            .unwrap()
            .templates
            .insert(t.name.clone(), t.clone());
        let world = super::tests::world();
        let id = w
            .spawn_history("A", &t.name, 1, "Existing", 1.0, 1.0, 0.3, &world)
            .unwrap();
        let mut changed = t.clone();
        changed.name = "Combined".into();
        changed.line.push(BattalionSlot {
            unit: "artillery".into(),
            column: 0,
            row: 1,
        });
        changed.support.push(SupportSlot {
            unit: "engineer".into(),
            regiment: None,
            row: 0,
        });
        assert!(
            w.save_template("A", Some(&t.name), changed.clone())
                .is_err()
        );
        assert_eq!(w.divisions[&id].template, t.name);
        w.countries.get_mut("A").unwrap().army_xp = 15.0;
        w.copy_template("A", &t.name, "Copy").unwrap();
        assert_eq!(w.countries["A"].army_xp, 15.0);
        w.save_template("A", Some(&t.name), changed.clone())
            .unwrap();
        assert_eq!(w.divisions[&id].template, "Combined");
        assert_eq!(w.divisions[&id].equipment.get("gun_1"), None);
        assert_eq!(w.divisions[&id].derived.stats.width, 5.0);
        let before = w.divisions[&id].derived.stats.soft_attack;
        let men = w.divisions[&id].derived.stats.manpower.ceil() as u32;
        let missing = men - w.divisions[&id].manpower;
        w.countries.get_mut("A").unwrap().manpower_pool = u64::from(missing);
        let mut production = ProductionState::default();
        production
            .country_mut("A")
            .model_stockpile
            .insert("gun_1".into(), 12);
        w.replenish(&mut production);
        assert_eq!(w.divisions[&id].equipment["gun_1"], 12);
        assert_eq!(production.country("A").unwrap().model_stockpile["gun_1"], 0);
        assert_eq!(w.countries["A"].manpower_pool, 0);
        assert!(w.divisions[&id].derived.stats.soft_attack > before);
        let mut smaller = changed.clone();
        smaller.line.retain(|s| s.unit == "infantry");
        smaller.support.clear();
        w.countries.get_mut("A").unwrap().army_xp = 15.0;
        w.save_template("A", Some("Combined"), smaller).unwrap();
        w.return_surplus(id, &mut production);
        assert_eq!(
            production.country("A").unwrap().model_stockpile["gun_1"],
            12
        );
        assert_eq!(w.countries["A"].manpower_pool, u64::from(missing));
        w.return_surplus(id, &mut production);
        assert_eq!(
            production.country("A").unwrap().model_stockpile["gun_1"],
            12
        );
    }
    #[test]
    fn moving_battalions_is_free_and_regimental_support_requires_a_verified_cost() {
        let mut db = catalogue();
        let t = template();
        let mut moved = t.clone();
        moved.line[0].column = 1;
        assert_eq!(db.edit_cost(&t, &moved).unwrap(), 0.0);
        db.units.get_mut("engineer").unwrap().regimental = true;
        let mut reg = t.clone();
        reg.support.push(SupportSlot {
            unit: "engineer".into(),
            regiment: Some(0),
            row: 0,
        });
        assert!(db.edit_cost(&t, &reg).is_err());
        db.rules.regimental_support_edit_xp = Some(7.0);
        assert_eq!(db.edit_cost(&t, &reg).unwrap(), 7.0);
        let mut m = MilitaryModifiers::default();
        m.country
            .insert("unit_infantry_design_cost_factor".into(), -1.0);
        let mut added = t.clone();
        added.line.push(BattalionSlot {
            unit: "infantry".into(),
            column: 0,
            row: 1,
        });
        assert_eq!(db.edit_cost_with_modifiers(&t, &added, &m).unwrap(), 0.0);
    }
}
