//! Explicit country partition policy; native DLC allocation rules must be
//! compiled into a plan, never guessed from an army-strength percentage.
use crate::{
    campaign::{Context, Payload},
    military_model::DivisionOrder,
    session::GameSession,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Share {
    pub numerator: u32,
    pub denominator: u32,
}
impl Share {
    fn valid(self) -> bool {
        self.denominator > 0 && self.numerator <= self.denominator
    }
    fn take(self, n: &mut u64) -> u64 {
        let v = (u128::from(*n) * u128::from(self.numerator) / u128::from(self.denominator)) as u64;
        *n -= v;
        v
    }
    fn float(self, n: &mut f64) -> f64 {
        let v = *n * (f64::from(self.numerator) / f64::from(self.denominator));
        *n -= v;
        v
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitPlan {
    pub parent: String,
    pub rebel: String,
    pub original_tag: String,
    pub ideology: String,
    pub states: BTreeSet<u32>,
    pub divisions: BTreeSet<u64>,
    pub commanders: BTreeSet<String>,
    pub wings: BTreeSet<u64>,
    pub fleets: BTreeSet<u64>,
    pub admirals: BTreeSet<String>,
    pub production_lines: BTreeSet<u64>,
    pub loyal_capital: u32,
    pub rebel_capital: u32,
    pub resources: Share,
    pub population: Share,
    pub inherit_flags: bool,
    pub inherit_variables: bool,
    pub inherit_focuses: bool,
}
impl SplitPlan {
    pub fn validate(&self, s: &GameSession) -> Result<(), String> {
        if !self.resources.valid()
            || !self.population.valid()
            || self.parent == self.rebel
            || self.rebel.len() != 3
            || !self
                .rebel
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            || self.original_tag.is_empty()
            || self.ideology.is_empty()
        {
            return Err("Invalid split identity/share".into());
        }
        if !s.world.countries.contains_key(&self.parent)
            || s.world.countries.contains_key(&self.rebel)
            || s.research.countries.contains_key(&self.rebel)
            || s.military.countries.contains_key(&self.rebel)
            || s.production.by_country.contains_key(&self.rebel)
            || s.construction.contains_key(&self.rebel)
            || s.military.logistics.countries.contains_key(&self.rebel)
            || s.campaign.countries.contains_key(&self.rebel)
            || s.resources.laws.contains_key(&self.rebel)
        {
            return Err("Country missing or rebel tag already registered".into());
        }
        if s.campaign
            .decisions
            .active
            .get(&self.parent)
            .is_some_and(|m| !m.is_empty())
        {
            return Err("Active decisions require an explicit civil-war inheritance policy".into());
        }
        if s.campaign
            .dynamic_modifiers
            .get(&self.parent)
            .is_some_and(|m| !m.is_empty())
        {
            return Err(
                "Dynamic modifiers require an explicit civil-war inheritance policy".into(),
            );
        }
        if s.military
            .wars
            .iter()
            .any(|(a, b)| a == &self.parent || b == &self.parent)
            || s.military
                .divisions
                .values()
                .any(|d| d.country == self.parent && d.battle.is_some())
            || s.military
                .naval
                .voyages
                .values()
                .any(|v| v.country == self.parent)
        {
            return Err(
                "External wars, active battles and voyages need a separate transition policy"
                    .into(),
            );
        }
        let owned = &s.world.countries[&self.parent].state_ids;
        if self.states.is_empty()
            || !self.states.is_subset(owned)
            || self.states.len() == owned.len()
        {
            return Err("Each side must retain owned territory".into());
        }
        for id in &self.states {
            let a = s.world.states.get(id).ok_or("State missing")?;
            if a.owner.as_deref() != Some(&self.parent)
                || a.province_ids
                    .iter()
                    .any(|p| s.world.controller_of_province(*p) != Some(&self.parent))
            {
                return Err("Only fully controlled owned states can transfer".into());
            }
        }
        let rebel = |p: u32| {
            s.world
                .provinces
                .get(&p)
                .and_then(|p| p.state_id)
                .is_some_and(|i| self.states.contains(&i))
        };
        for (p, is_rebel) in [(self.loyal_capital, false), (self.rebel_capital, true)] {
            if s.world.controller_of_province(p) != Some(&self.parent)
                || rebel(p) != is_rebel
                || !s
                    .world
                    .geography
                    .provinces
                    .get(&p)
                    .is_some_and(|p| p.definition.kind == crate::map_graph::ProvinceKind::Land)
            {
                return Err("Invalid capital allocation".into());
            }
        }
        for id in &self.divisions {
            if !s
                .military
                .divisions
                .get(id)
                .is_some_and(|d| d.country == self.parent && !d.destroyed)
            {
                return Err("Invalid division selection".into());
            }
        }
        for d in s
            .military
            .divisions
            .values()
            .filter(|d| d.country == self.parent && !d.destroyed)
        {
            if rebel(d.province) != self.divisions.contains(&d.id) {
                return Err("Relocate selected divisions explicitly before splitting".into());
            }
        }
        for id in &self.commanders {
            if !s
                .military
                .command
                .commanders
                .get(id)
                .is_some_and(|c| c.country == self.parent)
            {
                return Err("Invalid commander selection".into());
            }
        }
        for id in &self.wings {
            if !s
                .military
                .air
                .wings
                .get(id)
                .is_some_and(|w| w.country == self.parent)
            {
                return Err("Invalid wing selection".into());
            }
        }
        for w in s
            .military
            .air
            .wings
            .values()
            .filter(|w| w.country == self.parent)
        {
            if rebel(w.base) != self.wings.contains(&w.id) {
                return Err("Rebase wings before split".into());
            }
        }
        for id in &self.fleets {
            if !s
                .military
                .naval
                .task_forces
                .get(id)
                .is_some_and(|f| f.country == self.parent)
            {
                return Err("Invalid fleet selection".into());
            }
        }
        for f in s
            .military
            .naval
            .task_forces
            .values()
            .filter(|f| f.country == self.parent)
        {
            if rebel(f.base) != self.fleets.contains(&f.id) {
                return Err("Rebase fleets before split".into());
            }
        }
        for id in &self.admirals {
            if !s
                .military
                .naval
                .admirals
                .contains_key(&(self.parent.clone(), id.clone()))
            {
                return Err("Invalid admiral selection".into());
            }
        }
        let ids: BTreeSet<_> = s
            .production
            .country(&self.parent)
            .into_iter()
            .flat_map(|p| p.lines.iter().map(|l| l.id))
            .collect();
        if !self.production_lines.is_subset(&ids) {
            return Err("Invalid production line selection".into());
        }
        let a = s
            .military
            .countries
            .get(&self.parent)
            .ok_or("Army missing")?;
        if ![
            a.army_xp,
            a.mobilization_carry,
            a.command_power.unwrap_or(0.),
        ]
        .iter()
        .all(|v| v.is_finite() && *v >= 0.)
        {
            return Err("Invalid military currency".into());
        }
        if !s.military.command.can_allocate(
            (self.divisions.len() as u64)
                .saturating_mul(2)
                .saturating_add(1),
        ) {
            return Err("Command ID exhaustion".into());
        }
        if s.military
            .naval
            .designs
            .values()
            .filter(|d| d.country == self.parent)
            .any(|d| {
                s.military
                    .naval
                    .designs
                    .contains_key(&format!("{}::{}", self.rebel, d.id))
            })
        {
            return Err("Blueprint alias collision".into());
        }
        s.military.validate_references(&s.world)?;
        s.military.logistics.validate(&s.world)?;
        s.military.air.validate(&s.world, &s.military)?;
        s.campaign.validate(&s.world, s.clock.elapsed_hours)?;
        Ok(())
    }
}
impl GameSession {
    pub fn schedule_civil_war(&mut self, p: SplitPlan, due: u64) -> Result<u64, String> {
        p.validate(self)?;
        self.campaign.schedule(
            due,
            Context::country(&p.parent),
            "civil-war transition".into(),
            Payload::CivilWar(p),
        )
    }
    pub fn start_civil_war(
        &mut self,
        p: &SplitPlan,
        cat: &crate::research::ResearchCatalogue,
        rules: &crate::research::ResearchRules,
    ) -> Result<(), String> {
        p.validate(self)?;
        if self.military.script_transitions.len() >= 4096 {
            return Err("Pending war transition capacity exceeded".into());
        }
        let old = self
            .research
            .country(&p.parent)
            .and_then(|c| c.context.ideology.clone());
        let mut calls = Vec::new();
        for hook in [
            "on_ruling_party_change_immediate",
            "on_government_change",
            "on_ruling_party_change",
        ] {
            for tag in [&p.parent, &p.rebel] {
                let mut call = crate::campaign::on_actions::HookCall::country(hook, tag, None);
                if let Some(old) = &old {
                    call.context
                        .tokens
                        .insert("old_ideology_token".into(), old.clone());
                }
                calls.push(call);
            }
        }
        let hook_job = self.reserve_hook_batch(calls, cat)?;
        let a = &p.parent;
        let b = &p.rebel;
        crate::construction::initialize_builders(&self.world, &mut self.construction_queues);
        self.world.countries.insert(
            b.clone(),
            crate::world::Country {
                tag: b.clone(),
                state_ids: p.states.clone(),
            },
        );
        for id in &p.states {
            self.world
                .countries
                .get_mut(a)
                .unwrap()
                .state_ids
                .remove(id);
            let state = self.world.states.get_mut(id).unwrap();
            state.owner = Some(b.clone());
            let provinces = state.province_ids.clone();
            for id in provinces {
                self.world.set_province_controller(id, Some(b));
            }
            if let Some(q) = self.construction_queues.get_mut(id) {
                for j in q.iter_mut().filter(|j| j.builder.as_deref() == Some(a)) {
                    j.builder = Some(b.clone());
                }
            }
        }
        let loyal = self.military.countries.get_mut(a).unwrap();
        if loyal.mobilization_target.is_none() {
            loyal.mobilization_target = Some(loyal.recruitable_total);
        }
        let mut army = loyal.clone();
        army.manpower_pool = p.resources.take(&mut loyal.manpower_pool);
        army.core_population = p.population.take(&mut loyal.core_population);
        army.recruitable_total = p.population.take(&mut loyal.recruitable_total);
        army.mobilization_target = loyal.mobilization_target.map(|mut n| {
            let v = p.population.take(&mut n);
            loyal.mobilization_target = Some(n);
            v
        });
        army.mobilization_carry = p.population.float(&mut loyal.mobilization_carry);
        army.army_xp = p.resources.float(&mut loyal.army_xp);
        army.command_power = loyal.command_power.map(|mut n| {
            let v = p.resources.float(&mut n);
            loyal.command_power = Some(n);
            v
        });
        army.history_source = None;
        army.history_oobs.clear();
        army.history_errors.clear();
        army.history_expected = 0;
        army.history_imported = 0;
        if !p.inherit_flags {
            army.modifiers.flags.clear();
        }
        self.military.countries.insert(b.clone(), army);
        let loyal = self.production.country_mut(a);
        let mut production = loyal.clone();
        for (k, n) in &mut loyal.model_stockpile {
            production
                .model_stockpile
                .insert(k.clone(), p.resources.take(n));
        }
        for (k, n) in &mut loyal.stockpile {
            production.stockpile.insert(*k, p.resources.take(n));
        }
        production
            .lines
            .retain(|l| p.production_lines.contains(&l.id));
        loyal.lines.retain(|l| !p.production_lines.contains(&l.id));
        production
            .last_day_resource_factors
            .retain(|id, _| p.production_lines.contains(id));
        loyal
            .last_day_resource_factors
            .retain(|id, _| !p.production_lines.contains(id));
        production.starting_data_applied = true;
        self.production.by_country.insert(b.clone(), production);
        let mut r = self
            .research
            .country(a)
            .cloned()
            .unwrap_or_else(|| crate::research::CountryResearch::new(0, rules));
        r.context.tag = b.clone();
        r.context.original_tag = p.original_tag.clone();
        r.context.ideology = Some(p.ideology.clone());
        r.context.faction = None;
        r.context.overlord = None;
        r.context.is_subject = Some(false);
        r.context.at_war = Some(true);
        r.completed.clear();
        r.parked.clear();
        r.available_bonuses.clear();
        r.sharing_groups.clear();
        r.sharing_adjustments.clear();
        r.sharing_bonuses.clear();
        r.pending_effects.clear();
        for s in &mut r.slots {
            *s = Default::default();
        }
        if !p.inherit_flags {
            r.flags.clear();
            r.flags_authoritative = true;
        }
        if !p.inherit_variables {
            r.context.variables.clear();
        }
        if !p.inherit_focuses {
            r.context.completed_focuses.clear();
        }
        self.research.countries.insert(b.clone(), r);
        let mut politics = self.campaign.countries.get(a).cloned().unwrap_or_default();
        if let Some(c) = self.campaign.countries.get_mut(a) {
            politics.power = c.power.map(|mut v| {
                let n = p.resources.float(&mut v);
                c.power = Some(v);
                n
            });
        }
        self.campaign.countries.insert(b.clone(), politics);
        if let Some(day) = self.campaign.politics.last_day.get(a).copied() {
            self.campaign.politics.last_day.insert(b.clone(), day);
        }
        let inherited = self.construction.get(a).map(|c| (c.policy, c.law));
        let construction = self.construction.entry(b.clone()).or_default();
        if let Some((policy, law)) = inherited {
            construction.policy = policy;
            construction.law = law;
        }
        if let Some(law) = self.resources.laws.get(a).copied() {
            self.resources.laws.insert(b.clone(), law);
        }
        self.resources.last.remove(a);
        self.world.trade_factory_income.remove(a);
        let loyal = self
            .military
            .logistics
            .countries
            .entry(a.clone())
            .or_default();
        let mut c = loyal.clone();
        loyal.capital = Some(p.loyal_capital);
        c.capital = Some(p.rebel_capital);
        c.fuel = p.resources.float(&mut loyal.fuel);
        c.fuel_capacity = p.resources.float(&mut loyal.fuel_capacity);
        c.fuel_capacity_base = loyal.fuel_capacity_base.map(|mut v| {
            let n = p.resources.float(&mut v);
            loyal.fuel_capacity_base = Some(v);
            n
        });
        c.fuel_income_daily = p.resources.float(&mut loyal.fuel_income_daily);
        c.oil_supply_daily = None;
        loyal.oil_supply_daily = None;
        c.convoys = p.resources.take(&mut loyal.convoys);
        c.trucks = p.resources.take(&mut loyal.trucks);
        for (k, n) in &mut loyal.truck_models {
            c.truck_models.insert(k.clone(), p.resources.take(n));
        }
        c.trade_convoys_committed = 0;
        c.transport_convoys_committed = 0;
        c.fuel_used_last_hour = 0.;
        self.military.logistics.countries.insert(b.clone(), c);
        let mut groups = BTreeMap::<u64, Vec<u64>>::new();
        for id in &p.divisions {
            if let Some(old) = self.military.command.membership.remove(id) {
                groups.entry(old).or_default().push(*id);
            }
            self.military.divisions.get_mut(id).unwrap().country = b.clone();
        }
        for (old, ids) in groups {
            let army = self.military.command.armies[&old].clone();
            let new = self.military.command.create_army(b, &army.name, &ids);
            self.military.command.armies.get_mut(&new).unwrap().general =
                army.general.filter(|g| p.commanders.contains(g));
        }
        for id in &p.commanders {
            self.military
                .command
                .commanders
                .get_mut(id)
                .unwrap()
                .country = b.clone();
        }
        for army in self
            .military
            .command
            .armies
            .values_mut()
            .filter(|x| x.country == *a)
        {
            if army
                .general
                .as_ref()
                .is_some_and(|g| p.commanders.contains(g))
            {
                army.general = None;
            }
        }
        for group in self
            .military
            .command
            .groups
            .values_mut()
            .filter(|x| x.country == *a)
        {
            if group
                .field_marshal
                .as_ref()
                .is_some_and(|g| p.commanders.contains(g))
            {
                group.field_marshal = None;
            }
        }
        for j in self
            .military
            .recruitment
            .values_mut()
            .filter(|j| j.country == *a)
        {
            if self
                .world
                .provinces
                .get(&j.province)
                .and_then(|p| p.state_id)
                .is_some_and(|i| p.states.contains(&i))
            {
                j.country = b.clone();
            }
        }
        for id in &p.wings {
            let w = self.military.air.wings.get_mut(id).unwrap();
            w.country = b.clone();
            w.mission = crate::air_warfare::AirMission::Standby;
            w.region = None;
        }
        self.military.air.results.clear();
        let aliases: BTreeMap<_, _> = self
            .military
            .naval
            .designs
            .values()
            .filter(|d| d.country == *a)
            .map(|d| (d.id.clone(), format!("{b}::{}", d.id)))
            .collect();
        for (old, new) in &aliases {
            let mut d = self.military.naval.designs[old].clone();
            d.country = b.clone();
            d.id = new.clone();
            self.military.naval.designs.insert(new.clone(), d);
        }
        for id in &p.fleets {
            let f = self.military.naval.task_forces.get_mut(id).unwrap();
            f.country = b.clone();
            f.mission = crate::naval_warfare::NavalMission::Hold;
            f.regions.clear();
            for s in &mut f.ships {
                if let Some(id) = s.variant.as_ref().and_then(|id| aliases.get(id)) {
                    s.variant = Some(id.clone());
                }
            }
            if !self
                .military
                .naval
                .assigned_admirals
                .get(id)
                .is_some_and(|id| p.admirals.contains(id))
            {
                self.military.naval.assigned_admirals.remove(id);
            }
        }
        for id in &p.admirals {
            let mut adm = self
                .military
                .naval
                .admirals
                .remove(&(a.clone(), id.clone()))
                .unwrap();
            adm.country = b.clone();
            self.military
                .naval
                .admirals
                .insert((b.clone(), id.clone()), adm);
        }
        self.military.naval.assigned_admirals.retain(|fleet, id| {
            p.fleets.contains(fleet)
                || !p.admirals.contains(id)
                || self
                    .military
                    .naval
                    .task_forces
                    .get(fleet)
                    .is_none_or(|f| f.country != *a)
        });
        let mut builds = vec![];
        if let Some(lines) = self.military.naval.shipbuilding.get_mut(a) {
            let mut kept = vec![];
            for mut l in std::mem::take(lines) {
                if self.world.controller_of_province(l.base) == Some(b) {
                    l.country = b.clone();
                    if let Some(id) = aliases.get(&l.model) {
                        l.model = id.clone();
                    }
                    builds.push(l);
                } else {
                    kept.push(l);
                }
            }
            *lines = kept;
        }
        self.military.naval.shipbuilding.insert(b.clone(), builds);
        for j in self
            .military
            .naval
            .refits
            .values_mut()
            .filter(|j| p.fleets.contains(&j.task_force))
        {
            j.country = b.clone();
            if let Some(id) = aliases.get(&j.design) {
                j.design = id.clone();
            }
            if let Some(id) = j.original_variant.as_ref().and_then(|id| aliases.get(id)) {
                j.original_variant = Some(id.clone());
            }
        }
        // Both tags are validated/created before this infallible relation insertion.
        self.military.record_civil_war_relation(a, b);
        self.campaign
            .civil_wars
            .insert(b.clone(), (a.clone(), None));
        self.invalidate_civil_war(a, b);
        self.finish_reserved_hook_batch(hook_job, cat, rules);
        Ok(())
    }
    fn invalidate_civil_war(&mut self, a: &str, b: &str) {
        let affected: BTreeSet<_> = self
            .military
            .divisions
            .values_mut()
            .filter(|d| d.country == a || d.country == b)
            .map(|d| {
                d.order = DivisionOrder::Hold;
                d.destination = None;
                d.waypoints.clear();
                d.planning = 0.;
                d.command = Default::default();
                d.ability_modifiers.clear();
                d.id
            })
            .collect();
        self.military.fronts.fronts.retain(|_, f| {
            f.country != a && f.country != b && f.enemy_country != a && f.enemy_country != b
        });
        self.military.fronts.boundaries.clear();
        self.military
            .fronts
            .manual
            .retain(|d| !affected.contains(d));
        self.military
            .fronts
            .manual_return
            .retain(|d| !affected.contains(d));
        self.military
            .fronts
            .detached
            .retain(|d| !affected.contains(d));
        self.military
            .fronts
            .active_orders
            .retain(|d| !affected.contains(d));
        self.military.fronts.revision = self.military.fronts.revision.saturating_add(1);
        self.military
            .plans
            .plans
            .retain(|_, p| p.country != a && p.country != b);
        self.military.plans.revision = self.military.plans.revision.saturating_add(1);
        self.military
            .abilities
            .headquarters
            .retain(|d, _| !affected.contains(d));
        self.military.command.revision = self.military.command.revision.saturating_add(1);
        self.military.naval.spotting.clear();
        self.military.naval.last_contact_battle.clear();
        self.military.naval.operational_fuel.clear();
        self.military.naval.operational_fuel_sampled = false;
        for id in &affected {
            self.military.logistics.reinforcement_credit.remove(id);
            self.military.logistics.delivery.remove(id);
        }
        self.refresh_research_contexts();
        self.military.refresh_modifiers(&self.research);
        self.refresh_research_building_slots();
        self.military.clean_references();
        self.military.logistics.network_key = None;
        let mut l = std::mem::take(&mut self.military.logistics);
        l.prepare_reinforcements(&self.military, &self.production);
        if l.enabled {
            l.refresh_network(&self.military, &self.world);
        }
        self.military.logistics = l;
    }
    /// Conservative outcome policy, not a vanilla surrender threshold: require
    /// full occupation and an isolated civil war. Defunct tags retain historical identity.
    pub fn settle_civil_war(
        &mut self,
        rebel: &str,
        winner: &str,
        cat: &crate::research::ResearchCatalogue,
        rules: &crate::research::ResearchRules,
    ) -> Result<(), String> {
        let (parent, done) = self
            .campaign
            .civil_wars
            .get(rebel)
            .cloned()
            .ok_or("Civil war missing")?;
        if done.is_some() || (winner != parent && winner != rebel) {
            return Err("Invalid/repeated outcome".into());
        }
        let loser = if winner == parent { rebel } else { &parent };
        if !self.military.at_war(winner, loser)
            || self.military.wars.iter().any(|(a, b)| {
                (a == winner || a == loser || b == winner || b == loser)
                    && !((a == winner && b == loser) || (a == loser && b == winner))
            })
            || self
                .military
                .divisions
                .values()
                .any(|d| (d.country == loser || d.country == winner) && d.battle.is_some())
            || self
                .military
                .naval
                .voyages
                .values()
                .any(|v| v.country == loser || v.country == winner)
        {
            return Err("Resolve other wars/battles/voyages before settlement".into());
        }
        let states = self
            .world
            .countries
            .get(loser)
            .ok_or("Losing country missing")?
            .state_ids
            .clone();
        if states.is_empty()
            || self
                .world
                .provinces
                .values()
                .any(|p| p.controller.as_deref() == Some(loser))
            || states.iter().any(|id| {
                self.world.states[id]
                    .province_ids
                    .iter()
                    .any(|p| self.world.controller_of_province(*p) != Some(winner))
            })
        {
            return Err("Full occupation required by this policy".into());
        }
        self.military.validate_references(&self.world)?;
        let a = self
            .military
            .countries
            .get(winner)
            .ok_or("Winning army missing")?;
        let b = self
            .military
            .countries
            .get(loser)
            .ok_or("Losing army missing")?;
        for (x, y) in [
            (a.manpower_pool, b.manpower_pool),
            (a.core_population, b.core_population),
            (a.recruitable_total, b.recruitable_total),
        ] {
            x.checked_add(y).ok_or("Population overflow")?;
        }
        if ![
            a.army_xp + b.army_xp,
            a.mobilization_carry + b.mobilization_carry,
            a.command_power.unwrap_or(0.) + b.command_power.unwrap_or(0.),
        ]
        .iter()
        .all(|v| v.is_finite())
        {
            return Err("Military currency overflow".into());
        }
        let incoming = self.production.country(loser).cloned().unwrap_or_default();
        if !self
            .production
            .country(winner)
            .cloned()
            .unwrap_or_default()
            .can_absorb(&incoming)
        {
            return Err("Inventory/production ID overflow".into());
        }
        if let (Some(a), Some(b)) = (
            self.military.logistics.countries.get(winner),
            self.military.logistics.countries.get(loser),
        ) {
            for (x, y) in [(a.convoys, b.convoys), (a.trucks, b.trucks)] {
                x.checked_add(y).ok_or("Transport overflow")?;
            }
            for (k, n) in &b.truck_models {
                a.truck_models
                    .get(k)
                    .copied()
                    .unwrap_or(0)
                    .checked_add(*n)
                    .ok_or("Truck overflow")?;
            }
            if ![
                a.fuel + b.fuel,
                a.fuel_capacity + b.fuel_capacity,
                a.fuel_income_daily + b.fuel_income_daily,
                a.fuel_capacity_base.unwrap_or(0.) + b.fuel_capacity_base.unwrap_or(0.),
            ]
            .iter()
            .all(|v| v.is_finite())
            {
                return Err("Fuel overflow".into());
            }
        }
        let mut templates = a.templates.clone();
        let mut aliases = BTreeMap::new();
        for (name, t) in &b.templates {
            let mut id = name.clone();
            if templates.get(name).is_some_and(|x| x != t) {
                id = format!("{loser}::{name}");
                while templates.contains_key(&id) {
                    id.push('_');
                }
            }
            let mut t = t.clone();
            t.name = id.clone();
            templates.insert(id.clone(), t);
            aliases.insert(name.clone(), id);
        }
        let post_hooks = self.before_civil_annex(winner, loser, cat, rules)?;
        // Include equipment granted by the pre-annex callback in the merger.
        let incoming = self.production.country(loser).cloned().unwrap_or_default();
        crate::construction::initialize_builders(&self.world, &mut self.construction_queues);
        for id in states {
            self.world.states.get_mut(&id).unwrap().owner = Some(winner.into());
            self.world
                .countries
                .get_mut(winner)
                .unwrap()
                .state_ids
                .insert(id);
        }
        self.world
            .countries
            .get_mut(loser)
            .unwrap()
            .state_ids
            .clear();
        self.world.control_revision = self.world.control_revision.saturating_add(1);
        for q in self.construction_queues.values_mut() {
            for j in q.iter_mut().filter(|j| j.builder.as_deref() == Some(loser)) {
                j.builder = Some(winner.into());
            }
        }
        let b = self.military.countries.remove(loser).unwrap();
        let a = self.military.countries.get_mut(winner).unwrap();
        a.templates = templates;
        a.manpower_pool += b.manpower_pool;
        a.core_population += b.core_population;
        a.recruitable_total += b.recruitable_total;
        a.mobilization_target = Some(a.recruitable_total);
        a.mobilization_carry += b.mobilization_carry;
        a.army_xp += b.army_xp;
        a.command_power = match (a.command_power, b.command_power) {
            (Some(x), Some(y)) => Some(x + y),
            _ => None,
        };
        for d in self
            .military
            .divisions
            .values_mut()
            .filter(|d| d.country == loser)
        {
            d.country = winner.into();
            d.template = aliases[&d.template].clone();
        }
        for j in self
            .military
            .recruitment
            .values_mut()
            .filter(|j| j.country == loser)
        {
            j.country = winner.into();
            if let Some(id) = aliases.get(&j.template.name) {
                j.template.name = id.clone();
            }
        }
        for c in self
            .military
            .command
            .commanders
            .values_mut()
            .filter(|c| c.country == loser)
        {
            c.country = winner.into();
        }
        for a in self
            .military
            .command
            .armies
            .values_mut()
            .filter(|a| a.country == loser)
        {
            a.country = winner.into();
        }
        for g in self
            .military
            .command
            .groups
            .values_mut()
            .filter(|g| g.country == loser)
        {
            g.country = winner.into();
        }
        for t in self
            .military
            .command
            .theaters
            .values_mut()
            .filter(|t| t.country == loser)
        {
            t.country = winner.into();
        }
        self.production.by_country.remove(loser);
        self.production.country_mut(winner).absorb(incoming);
        if let Some(b) = self.military.logistics.countries.remove(loser) {
            let a = self
                .military
                .logistics
                .countries
                .entry(winner.into())
                .or_default();
            a.fuel += b.fuel;
            a.fuel_capacity += b.fuel_capacity;
            a.fuel_capacity_base = match (a.fuel_capacity_base, b.fuel_capacity_base) {
                (Some(x), Some(y)) => Some(x + y),
                _ => None,
            };
            a.fuel_income_daily += b.fuel_income_daily;
            a.oil_supply_daily = None;
            a.convoys += b.convoys;
            a.trucks += b.trucks;
            for (k, n) in b.truck_models {
                *a.truck_models.entry(k).or_default() += n;
            }
        }
        for w in self
            .military
            .air
            .wings
            .values_mut()
            .filter(|w| w.country == loser)
        {
            w.country = winner.into();
            w.mission = crate::air_warfare::AirMission::Standby;
            w.region = None;
        }
        self.military.air.results.clear();
        let transferred: BTreeSet<_> = self
            .military
            .naval
            .task_forces
            .values()
            .filter(|f| f.country == loser)
            .map(|f| f.id)
            .collect();
        for f in self
            .military
            .naval
            .task_forces
            .values_mut()
            .filter(|f| f.country == loser)
        {
            f.country = winner.into();
            f.mission = crate::naval_warfare::NavalMission::Hold;
            f.regions.clear();
        }
        for d in self
            .military
            .naval
            .designs
            .values_mut()
            .filter(|d| d.country == loser)
        {
            d.country = winner.into();
        }
        let keys: Vec<_> = self
            .military
            .naval
            .admirals
            .keys()
            .filter(|(t, _)| t == loser)
            .cloned()
            .collect();
        for key in keys {
            let mut a = self.military.naval.admirals.remove(&key).unwrap();
            a.country = winner.into();
            let mut id = format!("{loser}::{}", key.1);
            while self
                .military
                .naval
                .admirals
                .contains_key(&(winner.into(), id.clone()))
            {
                id.push('_');
            }
            for (f, g) in &mut self.military.naval.assigned_admirals {
                if transferred.contains(f) && *g == key.1 {
                    *g = id.clone();
                }
            }
            self.military.naval.admirals.insert((winner.into(), id), a);
        }
        if let Some(lines) = self.military.naval.shipbuilding.remove(loser) {
            self.military
                .naval
                .shipbuilding
                .entry(winner.into())
                .or_default()
                .extend(lines.into_iter().map(|mut l| {
                    l.country = winner.into();
                    l
                }));
        }
        for j in self
            .military
            .naval
            .refits
            .values_mut()
            .filter(|j| j.country == loser)
        {
            j.country = winner.into();
        }
        self.research.countries.remove(loser);
        if let Some(b) = self.campaign.countries.remove(loser) {
            let a = self.campaign.countries.entry(winner.into()).or_default();
            a.power = match (a.power, b.power) {
                (Some(x), Some(y)) if (x + y).is_finite() => Some(x + y),
                _ => None,
            };
        }
        self.construction.remove(loser);
        self.construction
            .entry(winner.into())
            .or_default()
            .order
            .sync(&self.world, winner, &self.construction_queues);
        self.resources.laws.remove(loser);
        self.resources
            .deals
            .retain(|d| d.buyer != loser && d.seller != loser);
        self.resources.last.remove(loser);
        self.resources.last.remove(winner);
        self.resources
            .last_shipments
            .retain(|d| d.buyer != loser && d.seller != loser);
        self.world.trade_factory_income.remove(loser);
        self.world.trade_factory_income.remove(winner);
        self.campaign
            .queue
            .retain(|_, a| a.context.scope != crate::campaign::Scope::Country(loser.into()));
        self.campaign
            .pending
            .retain(|_, a| a.context.scope != crate::campaign::Scope::Country(loser.into()));
        self.campaign.dynamic_modifiers.remove(loser);
        self.campaign.decisions.clear_country(loser);
        self.campaign.politics.last_day.remove(loser);
        self.campaign.politics.income.remove(loser);
        self.campaign.politics.failures.remove(loser);
        self.campaign.focuses.active.remove(loser);
        self.campaign.inactive.insert(loser.into());
        self.campaign.civil_wars.get_mut(rebel).unwrap().1 = Some(winner.into());
        self.military.remove_war_relation(winner, loser);
        self.military
            .military_access
            .retain(|(a, b)| a != loser && b != loser);
        self.military.strait_access.retain(|(t, _, _)| t != loser);
        if self.player_country.as_deref() == Some(loser) {
            self.player_country = Some(winner.into());
        }
        self.invalidate_civil_war(winner, loser);
        self.finish_reserved_hook_batch(post_hooks, cat, rules);
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn integer_partitions_conserve_extreme_resources() {
        for d in 1..=64 {
            for n in 0..=d {
                for total in [0, 1, 17, u32::MAX as u64, u64::MAX] {
                    let mut left = total;
                    let taken = Share {
                        numerator: n,
                        denominator: d,
                    }
                    .take(&mut left);
                    assert_eq!(u128::from(left) + u128::from(taken), u128::from(total));
                    assert_eq!(
                        u128::from(taken),
                        u128::from(total) * u128::from(n) / u128::from(d)
                    );
                }
            }
        }
    }
}
