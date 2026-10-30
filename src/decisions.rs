//! Country decisions/missions run inside the campaign journal, including costs
//! and lifecycle transitions. Authored behavior still needs native differential tests.
use super::*;
use hoi4_mobile::{decision_data, research_script::DecisionRecord};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveDecision {
    pub serial: u64,
    pub begun: u64,
    pub deadline: Option<u64>,
    pub mission: bool,
    #[serde(default)]
    pub blocked: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DecisionRuntime {
    pub active: BTreeMap<String, BTreeMap<String, ActiveDecision>>,
    pub used: BTreeMap<String, BTreeSet<String>>,
    pub cooldowns: BTreeMap<String, BTreeMap<String, u64>>,
    /// `allowed` is a campaign-start snapshot, not a dynamic availability test.
    pub allowed: BTreeMap<String, BTreeMap<String, bool>>,
    pub seeds: BTreeMap<String, BTreeMap<String, u64>>,
    pub failures: BTreeMap<String, BTreeMap<String, String>>,
    pub last_day: Option<u64>,
    pub next_serial: u64,
    pub revision: u64,
}
impl DecisionRuntime {
    pub fn validate(&self, w: &crate::world::World, hour: u64) -> Result<(), String> {
        if self.last_day.is_some_and(|h| h > hour / 24)
            || self.active.values().map(|m| m.len()).sum::<usize>() > QUEUE
            || self.active.iter().any(|(t, jobs)| {
                !w.countries.contains_key(t)
                    || jobs.iter().any(|(id, j)| {
                        id.is_empty()
                            || j.serial == 0
                            || j.serial > self.next_serial
                            || j.begun > hour
                            || j.deadline.is_some_and(|h| h < j.begun)
                    })
            })
            || self
                .allowed
                .keys()
                .chain(self.cooldowns.keys())
                .chain(self.seeds.keys())
                .chain(self.used.keys())
                .chain(self.failures.keys())
                .any(|t| !w.countries.contains_key(t))
        {
            return Err("Invalid decision runtime".into());
        }
        Ok(())
    }
    pub fn clear_country(&mut self, t: &str) {
        self.active.remove(t);
        self.used.remove(t);
        self.cooldowns.remove(t);
        self.allowed.remove(t);
        self.seeds.remove(t);
        self.failures.remove(t);
        self.revision = self.revision.saturating_add(1);
    }
}
fn flag(raw: &[Entry], k: &str, default: bool) -> bool {
    pdx::atom(raw, k).map_or(default, |v| v == "yes")
}
impl Journal<'_> {
    fn decision_allowed(&mut self, tag: &str, id: &str) -> Result<bool, String> {
        if let Some(value) = self
            .state
            .decisions
            .allowed
            .get(tag)
            .and_then(|m| m.get(id))
        {
            return Ok(*value);
        }
        let record = self
            .cat
            .scripts
            .decisions
            .get(id)
            .ok_or("Missing decision")?;
        let category = self
            .cat
            .scripts
            .decision_categories
            .get(&record.category)
            .ok_or("Missing decision category")?;
        let ctx = Context::country(tag);
        let value =
            self.conditions(pdx::block(&category.raw, "allowed").unwrap_or(&[]), &ctx, 0)?
                && self.conditions(pdx::block(&record.raw, "allowed").unwrap_or(&[]), &ctx, 0)?;
        self.state
            .to_mut()
            .decisions
            .allowed
            .entry(tag.into())
            .or_default()
            .insert(id.into(), value);
        Ok(value)
    }
    fn decision_condition(
        &mut self,
        raw: &[Entry],
        key: &str,
        ctx: &Context,
        default: bool,
    ) -> Result<bool, String> {
        match pdx::block(raw, key) {
            Some(b) => self.conditions(b, ctx, 0),
            None => Ok(default),
        }
    }
    fn decision_visible(&mut self, record: &DecisionRecord, ctx: &Context) -> Result<bool, String> {
        let category = decision_data::category(self.cat, &record.category)?;
        Ok(
            self.decision_condition(&category.raw, "visible", ctx, true)?
                && self.decision_condition(&record.raw, "visible", ctx, true)?,
        )
    }
    fn decision_available(
        &mut self,
        record: &DecisionRecord,
        ctx: &Context,
    ) -> Result<bool, String> {
        let category = decision_data::category(self.cat, &record.category)?;
        Ok(
            self.decision_condition(&category.raw, "available", ctx, true)?
                && self.decision_condition(&record.raw, "available", ctx, true)?
                && self.decision_condition(&record.raw, "custom_cost_trigger", ctx, true)?,
        )
    }
    fn decision_cost(&mut self, raw: &[Entry], ctx: &Context) -> Result<f64, String> {
        let n = self.num(pdx::atom(raw, "cost").unwrap_or("0"), ctx)?;
        if n == -1. && pdx::block(raw, "custom_cost_trigger").is_some() {
            return Ok(0.);
        }
        if n < 0. || n.fract() != 0. {
            return Err("Decision cost requires nonnegative integral PP or custom cost=-1".into());
        }
        Ok(n)
    }
    fn pay_decision(&mut self, raw: &[Entry], ctx: &Context) -> Result<(), String> {
        let n = self.decision_cost(raw, ctx)?;
        if n == 0. {
            return Ok(());
        }
        let t = self.tag(ctx)?;
        let p = self
            .state
            .to_mut()
            .countries
            .get_mut(&t)
            .and_then(|p| p.power.as_mut())
            .ok_or("Unknown political power")?;
        if *p < n {
            return Err(format!("Not enough political power: {p:.0}/{n:.0}"));
        }
        *p -= n;
        Ok(())
    }
    fn decision_deadline(
        &mut self,
        raw: &[Entry],
        key: &str,
        ctx: &Context,
    ) -> Result<Option<u64>, String> {
        let Some(value) = pdx::atom(raw, key) else {
            return Ok(None);
        };
        let days = self.num(value, ctx)?;
        if days == -1. {
            return Ok(None);
        }
        if days <= 0. || days.fract() != 0. || days > (u64::MAX / 24) as f64 {
            return Err(format!("Unsupported {key} duration {days}"));
        }
        self.s
            .clock
            .elapsed_hours
            .checked_add(
                (days as u64)
                    .checked_mul(24)
                    .ok_or("Decision duration overflow")?,
            )
            .map(Some)
            .ok_or("Decision deadline overflow".into())
    }
    fn preflight_decision(&self, record: &DecisionRecord) -> Result<(), String> {
        for key in decision_data::EFFECTS {
            preflight(pdx::block(&record.raw, key).unwrap_or(&[]), self.cat)?;
        }
        Ok(())
    }
    fn insert_decision(
        &mut self,
        t: &str,
        id: &str,
        mission: bool,
        deadline: Option<u64>,
    ) -> Result<(), String> {
        if self
            .state
            .decisions
            .active
            .values()
            .map(|m| m.len())
            .sum::<usize>()
            >= QUEUE
        {
            return Err("Decision capacity exceeded".into());
        }
        let serial = self
            .state
            .decisions
            .next_serial
            .checked_add(1)
            .ok_or("Decision ID exhausted")?;
        let state = &mut self.state.to_mut().decisions;
        state.next_serial = serial;
        state.active.entry(t.into()).or_default().insert(
            id.into(),
            ActiveDecision {
                serial,
                begun: self.s.clock.elapsed_hours,
                deadline,
                mission,
                blocked: None,
            },
        );
        Ok(())
    }
    pub(super) fn activate_mission(
        &mut self,
        ctx: &Context,
        id: &str,
        d: usize,
    ) -> Result<(), String> {
        self.step(d)?;
        let t = self.tag(ctx)?;
        let record = decision_data::definition(self.cat, id)?;
        if pdx::atom(&record.raw, "days_mission_timeout").is_none() {
            return Err(format!("{id} is not a mission"));
        }
        if self
            .state
            .decisions
            .active
            .get(&t)
            .is_some_and(|m| m.contains_key(id))
        {
            return Err(format!("Mission {id} already active"));
        }
        self.preflight_decision(record)?;
        let deadline = self.decision_deadline(&record.raw, "days_mission_timeout", ctx)?;
        self.insert_decision(&t, id, true, deadline)?;
        self.state
            .to_mut()
            .decisions
            .used
            .entry(t)
            .or_default()
            .insert(id.into());
        self.refresh_decision_modifiers(ctx)
    }
    pub(super) fn remove_decision(
        &mut self,
        ctx: &Context,
        id: &str,
        mission: bool,
    ) -> Result<(), String> {
        let t = self.tag(ctx)?;
        if let Some(job) = self.state.decisions.active.get(&t).and_then(|m| m.get(id)) {
            if job.mission != mission {
                return Err("Decision/mission type mismatch".into());
            }
            self.state
                .to_mut()
                .decisions
                .active
                .get_mut(&t)
                .unwrap()
                .remove(id);
        }
        // Native explicit removal ignores fire_only_once and starts no cooldown.
        if let Some(used) = self.state.to_mut().decisions.used.get_mut(&t) {
            used.remove(id);
        }
        if let Some(c) = self.state.to_mut().decisions.cooldowns.get_mut(&t) {
            c.remove(id);
        }
        self.refresh_decision_modifiers(ctx)
    }
    pub(super) fn adjust_decision_timer(&mut self, ctx: &Context, e: &Entry) -> Result<(), String> {
        let mission = e.key == "add_days_mission_timeout";
        let b = body(e)?;
        let id = field(b, if mission { "mission" } else { "decision" })?;
        let delta = self.num(field(b, "days")?, ctx)? * 24.;
        if delta.fract() != 0. || delta.abs() >= i64::MAX as f64 {
            return Err("Invalid decision timer delta".into());
        }
        let t = self.tag(ctx)?;
        let job = self
            .state
            .to_mut()
            .decisions
            .active
            .get_mut(&t)
            .and_then(|m| m.get_mut(id))
            .ok_or("Decision not active")?;
        if job.mission != mission {
            return Err("Decision timer type mismatch".into());
        }
        let old = job.deadline.ok_or("No finite decision timer")?;
        job.deadline = Some(
            old.checked_add_signed(delta as i64)
                .ok_or("Decision timer overflow")?
                .max(self.s.clock.elapsed_hours),
        );
        Ok(())
    }
    fn decision_callback(
        &mut self,
        ctx: &Context,
        id: &str,
        record: &DecisionRecord,
        key: &str,
    ) -> Result<(), String> {
        let b = pdx::block(&record.raw, key).unwrap_or(&[]);
        if b.is_empty() {
            return Ok(());
        }
        preflight(b, self.cat)?;
        if !flag(&record.raw, "fixed_random_seed", true) {
            return self.execute(b, ctx, &record.source, 0);
        }
        let t = self.tag(ctx)?;
        let seed_key = format!("{id}:{key}");
        let seed = if let Some(n) = self
            .state
            .decisions
            .seeds
            .get(&t)
            .and_then(|m| m.get(&seed_key))
        {
            *n
        } else {
            let n = self.state.to_mut().rng.next_u64();
            self.state
                .to_mut()
                .decisions
                .seeds
                .entry(t)
                .or_default()
                .insert(seed_key, n);
            n
        };
        let old = std::mem::replace(&mut self.state.to_mut().rng, SeededRandom::new(seed));
        let result = self.execute(b, ctx, &record.source, 0);
        self.state.to_mut().rng = old;
        result
    }
    fn cooldown(&mut self, ctx: &Context, id: &str, record: &DecisionRecord) -> Result<(), String> {
        let t = self.tag(ctx)?;
        // An instant decision is selectable again at the next daily check.
        let n = self.num(pdx::atom(&record.raw, "days_re_enable").unwrap_or("1"), ctx)?;
        if n < 0. || n.fract() != 0. || n >= (u64::MAX / 24) as f64 {
            return Err("Invalid decision cooldown".into());
        }
        let due = self
            .s
            .clock
            .elapsed_hours
            .checked_add(
                (n as u64)
                    .checked_mul(24)
                    .ok_or("Decision cooldown overflow")?,
            )
            .ok_or("Decision cooldown overflow")?;
        self.state
            .to_mut()
            .decisions
            .cooldowns
            .entry(t)
            .or_default()
            .insert(id.into(), due);
        Ok(())
    }
    fn finish_decision(&mut self, ctx: &Context, id: &str, key: &str) -> Result<(), String> {
        let t = self.tag(ctx)?;
        let record = decision_data::definition(self.cat, id)?;
        let job = self
            .state
            .to_mut()
            .decisions
            .active
            .get_mut(&t)
            .and_then(|m| m.remove(id))
            .ok_or("Decision not active")?;
        self.cooldown(ctx, id, record)?;
        self.decision_timers
            .insert((t.clone(), id.into()), (job.mission, job.deadline));
        self.decision_callback(ctx, id, record, key)?;
        self.decision_timers.remove(&(t, id.into()));
        self.refresh_decision_modifiers(ctx)
    }
    fn select_decision(&mut self, ctx: &Context, id: &str, manual: bool) -> Result<(), String> {
        let t = self.tag(ctx)?;
        let record = decision_data::definition(self.cat, id)?;
        self.preflight_decision(record)?;
        let active = self
            .state
            .decisions
            .active
            .get(&t)
            .and_then(|m| m.get(id))
            .cloned();
        let mission = pdx::atom(&record.raw, "days_mission_timeout").is_some();
        if mission {
            if !active.as_ref().is_some_and(|j| j.mission)
                || (manual && !flag(&record.raw, "selectable_mission", false))
            {
                return Err("Mission is not selectable".into());
            }
        } else {
            if active.is_some() {
                return Err("Decision already active".into());
            }
            if !self.decision_allowed(&t, id)? || !self.decision_visible(record, ctx)? {
                return Err("Decision not visible/allowed".into());
            }
            if !self.decision_ready(&t, id, record) {
                return Err("Decision already used or on cooldown".into());
            }
        }
        if active.as_ref().is_some_and(|j| j.blocked.is_some()) {
            return Err("Decision lifecycle is blocked".into());
        }
        if !self.decision_available(record, ctx)? {
            return Err("Decision conditions are not met".into());
        }
        self.pay_decision(&record.raw, ctx)?;
        let deadline = self.decision_deadline(&record.raw, "days_remove", ctx)?;
        if mission {
            let old = self
                .state
                .to_mut()
                .decisions
                .active
                .get_mut(&t)
                .unwrap()
                .remove(id)
                .unwrap();
            self.decision_timers
                .insert((t.clone(), id.into()), (true, old.deadline));
        }
        self.state
            .to_mut()
            .decisions
            .used
            .entry(t.clone())
            .or_default()
            .insert(id.into());
        // Insert before complete_effect so source predicates see a selected decision.
        self.insert_decision(&t, id, false, deadline)?;
        let serial = self.state.decisions.next_serial;
        self.decision_callback(ctx, id, record, "complete_effect")?;
        self.decision_timers.remove(&(t.clone(), id.into()));
        if pdx::atom(&record.raw, "days_remove").is_none() {
            if self
                .state
                .decisions
                .active
                .get(&t)
                .and_then(|m| m.get(id))
                .is_some_and(|j| j.serial == serial)
            {
                self.state
                    .to_mut()
                    .decisions
                    .active
                    .get_mut(&t)
                    .unwrap()
                    .remove(id);
                self.cooldown(ctx, id, record)?;
            }
        }
        self.refresh_decision_modifiers(ctx)
    }
    fn decision_ready(&self, t: &str, id: &str, record: &DecisionRecord) -> bool {
        !self
            .state
            .decisions
            .active
            .get(t)
            .is_some_and(|m| m.contains_key(id))
            && !(flag(&record.raw, "fire_only_once", false)
                && self
                    .state
                    .decisions
                    .used
                    .get(t)
                    .is_some_and(|m| m.contains(id)))
            && self
                .state
                .decisions
                .cooldowns
                .get(t)
                .and_then(|m| m.get(id))
                .is_none_or(|h| *h <= self.s.clock.elapsed_hours)
    }
    fn refresh_decision_modifiers(&mut self, ctx: &Context) -> Result<(), String> {
        let t = self.tag(ctx)?;
        let ids: Vec<_> = self
            .state
            .decisions
            .active
            .get(&t)
            .into_iter()
            .flat_map(|m| m.keys().cloned())
            .collect();
        if ids.is_empty()
            && !self
                .research
                .get(&t)
                .or_else(|| self.s.research.country(&t))
                .is_some_and(|c| {
                    c.source_modifiers
                        .keys()
                        .any(|k| k.starts_with("decision:"))
                })
        {
            return Ok(());
        }
        self.research(&t)
            .source_modifiers
            .retain(|k, _| !k.starts_with("decision:"));
        for id in ids {
            if self
                .state
                .decisions
                .active
                .get(&t)
                .and_then(|m| m.get(&id))
                .is_some_and(|j| j.blocked.is_some())
            {
                continue;
            }
            self.step(0)?;
            let record = decision_data::definition(self.cat, &id)?;
            let mut values = BTreeMap::<String, f64>::new();
            for e in pdx::block(&record.raw, "modifier").unwrap_or(&[]) {
                let n = self.num(atom(e)?, ctx)?;
                let total = values.entry(e.key.clone()).or_default();
                *total += n;
                if !total.is_finite() {
                    return Err("Decision modifier overflow".into());
                }
            }
            if !values.is_empty() {
                self.research(&t)
                    .source_modifiers
                    .insert(format!("decision:{id}"), values);
            }
        }
        Ok(())
    }
    fn update_decision(&mut self, ctx: &Context, id: &str, daily: bool) -> Result<(), String> {
        let t = self.tag(ctx)?;
        let record = decision_data::definition(self.cat, id)?;
        let active = self
            .state
            .decisions
            .active
            .get(&t)
            .and_then(|m| m.get(id))
            .cloned();
        let Some(job) = active else {
            if !daily
                || !self.decision_allowed(&t, id)?
                || !self.decision_ready(&t, id, record)
                || pdx::atom(&record.raw, "days_mission_timeout").is_none()
            {
                return Ok(());
            }
            if self.decision_condition(&record.raw, "activation", ctx, false)? {
                self.activate_mission(ctx, id, 0)?;
                return self.update_decision(ctx, id, daily);
            }
            return Ok(());
        };
        if job.blocked.is_some() {
            return Ok(());
        }
        if daily {
            let cat = decision_data::category(self.cat, &record.category)?;
            if self.decision_condition(&cat.raw, "cancel_trigger", ctx, false)?
                || self.decision_condition(&record.raw, "cancel_trigger", ctx, false)?
                || (!job.mission
                    && flag(&record.raw, "cancel_if_not_visible", false)
                    && !self.decision_visible(record, ctx)?)
            {
                return self.finish_decision(ctx, id, "cancel_effect");
            }
            if job.mission
                && !flag(&record.raw, "selectable_mission", false)
                && self.decision_available(record, ctx)?
            {
                return self.select_decision(ctx, id, false);
            }
            if !job.mission && self.decision_condition(&record.raw, "remove_trigger", ctx, false)? {
                return self.finish_decision(ctx, id, "remove_effect");
            }
        }
        if job
            .deadline
            .is_some_and(|due| due <= self.s.clock.elapsed_hours)
        {
            return self.finish_decision(
                ctx,
                id,
                if job.mission {
                    "timeout_effect"
                } else {
                    "remove_effect"
                },
            );
        }
        Ok(())
    }
}
impl GameSession {
    pub fn select_decision(
        &mut self,
        t: &str,
        id: &str,
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
    ) -> Result<(), String> {
        self.campaign_transaction(cat, rules, |j| {
            j.select_decision(&Context::country(t), id, true)
        })
    }
    pub fn initialize_decisions(&mut self, cat: &ResearchCatalogue, rules: &ResearchRules) {
        let tags: Vec<_> = self
            .world
            .countries
            .keys()
            .filter(|t| !self.campaign.inactive.contains(*t))
            .cloned()
            .collect();
        for t in tags {
            for id in cat.scripts.decisions.keys() {
                let r = self.campaign_transaction(cat, rules, |j| j.decision_allowed(&t, id));
                if let Err(e) = r {
                    self.campaign
                        .decisions
                        .failures
                        .entry(t.clone())
                        .or_default()
                        .insert(id.clone(), e);
                }
            }
        }
    }
    pub fn tick_decisions(&mut self, cat: &ResearchCatalogue, rules: &ResearchRules) {
        if cat.scripts.decisions.is_empty()
            && self
                .campaign
                .decisions
                .active
                .values()
                .all(|m| m.is_empty())
        {
            return;
        }
        let day = self.clock.elapsed_hours / 24;
        let daily = self.campaign.decisions.last_day.is_none_or(|d| d < day);
        if daily {
            self.campaign.decisions.last_day = Some(day);
        }
        let tags: Vec<_> = self
            .world
            .countries
            .keys()
            .filter(|t| !self.campaign.inactive.contains(*t))
            .cloned()
            .collect();
        for t in tags {
            let ids: Vec<_> = if daily {
                cat.scripts
                    .decisions
                    .keys()
                    .chain(
                        self.campaign
                            .decisions
                            .active
                            .get(&t)
                            .into_iter()
                            .flat_map(|m| m.keys()),
                    )
                    .cloned()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect()
            } else {
                self.campaign
                    .decisions
                    .active
                    .get(&t)
                    .into_iter()
                    .flat_map(|m| m.keys().cloned())
                    .collect()
            };
            for id in ids {
                // Evaluate start-only eligibility first: foreign definitions are not executed.
                if !self
                    .campaign
                    .decisions
                    .active
                    .get(&t)
                    .is_some_and(|m| m.contains_key(&id))
                    && self
                        .campaign
                        .decisions
                        .allowed
                        .get(&t)
                        .and_then(|m| m.get(&id))
                        == Some(&false)
                {
                    continue;
                }
                let result = self.campaign_transaction(cat, rules, |j| {
                    j.update_decision(&Context::country(&t), &id, daily)
                });
                if let Err(e) = result {
                    self.campaign
                        .decisions
                        .failures
                        .entry(t.clone())
                        .or_default()
                        .insert(id.clone(), e.clone());
                    if let Some(j) = self
                        .campaign
                        .decisions
                        .active
                        .get_mut(&t)
                        .and_then(|m| m.get_mut(&id))
                    {
                        j.blocked = Some(e);
                    }
                    // A blocked modifier cannot retain a stale optimistic bonus.
                    if let Some(c) = self.research.countries.get_mut(&t) {
                        c.source_modifiers.remove(&format!("decision:{id}"));
                    }
                }
            }
            if daily {
                if let Err(e) = self.campaign_transaction(cat, rules, |j| {
                    j.refresh_decision_modifiers(&Context::country(&t))
                }) {
                    self.campaign
                        .decisions
                        .failures
                        .entry(t.clone())
                        .or_default()
                        .insert("modifier_refresh".into(), e);
                    if let Some(c) = self.research.countries.get_mut(&t) {
                        c.source_modifiers
                            .retain(|k, _| !k.starts_with("decision:"));
                    }
                }
            }
        }
        self.military.refresh_modifiers(&self.research);
        self.refresh_research_building_slots();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdx::parse_pdx;
    fn fixture(raw: &str) -> (GameSession, ResearchCatalogue) {
        let mut s = GameSession::new(crate::warfare::tests::world());
        s.research
            .country_mut("A", 0, &Default::default())
            .context
            .tag = "A".into();
        s.campaign.countries.insert(
            "A".into(),
            CountryPolitics {
                power: Some(100.),
                stability: Some(0.5),
                war_support: Some(0.5),
            },
        );
        let mut cat = ResearchCatalogue::default();
        cat.scripts
            .decision_categories
            .insert("test".into(), DecisionRecord::default());
        for e in parse_pdx(raw).unwrap() {
            cat.scripts.decisions.insert(
                e.key.clone(),
                DecisionRecord {
                    category: "test".into(),
                    source: "authored".into(),
                    raw: e.block().unwrap().to_vec(),
                    problems: vec![],
                },
            );
        }
        (s, cat)
    }
    fn power(s: &GameSession) -> f64 {
        s.campaign.countries["A"].power.unwrap()
    }
    fn select(s: &mut GameSession, cat: &ResearchCatalogue, id: &str) -> Result<(), String> {
        s.select_decision("A", id, cat, &Default::default())
    }
    fn tick(s: &mut GameSession, cat: &ResearchCatalogue, h: u64) {
        s.clock.elapsed_hours = h;
        s.tick_decisions(cat, &Default::default());
    }
    fn run(s: &mut GameSession, cat: &ResearchCatalogue, raw: &str) -> Result<(), String> {
        s.campaign_effects(
            &Context::country("A"),
            &parse_pdx(raw).unwrap(),
            "authored",
            cat,
            &Default::default(),
        )
    }
    #[test]
    fn timed_decision_pays_once_modifies_real_consumers_and_removes_once_after_restore() {
        let (mut s, cat) = fixture(
            "d={cost=30 days_remove=2 modifier={political_power_gain=4 research_speed_factor=0.2} complete_effect={add_political_power=5} remove_effect={add_political_power=9} days_re_enable=3}",
        );
        select(&mut s, &cat, "d").unwrap();
        assert_eq!(power(&s), 75.);
        assert_eq!(
            s.research
                .country("A")
                .unwrap()
                .modifier("research_speed_factor"),
            0.2
        );
        assert!(select(&mut s, &cat, "d").is_err());
        let bytes = serde_json::to_vec(&s).unwrap();
        let mut s: GameSession = serde_json::from_slice(&bytes).unwrap();
        s.campaign.validate(&s.world, 0).unwrap();
        tick(&mut s, &cat, 47);
        assert_eq!(power(&s), 75.);
        tick(&mut s, &cat, 48);
        assert_eq!(power(&s), 84.);
        assert_eq!(
            s.research
                .country("A")
                .unwrap()
                .modifier("research_speed_factor"),
            0.
        );
        tick(&mut s, &cat, 48);
        tick(&mut s, &cat, 72);
        assert_eq!(power(&s), 84.);
        assert!(select(&mut s, &cat, "d").is_err());
        tick(&mut s, &cat, 120);
        select(&mut s, &cat, "d").unwrap();
        assert_eq!(power(&s), 59.);
    }
    #[test]
    fn failed_nested_reward_rolls_back_payment_rng_history_and_jobs() {
        let (mut s, cat) = fixture(
            "d={cost=30 complete_effect={add_political_power=5 divide_variable={var=missing value=0}}}",
        );
        let before = serde_json::to_value(&s).unwrap();
        assert!(select(&mut s, &cat, "d").is_err());
        assert_eq!(before, serde_json::to_value(&s).unwrap());
        let (mut s, cat) = fixture("d={cost=300 complete_effect={add_political_power=999}}");
        assert!(select(&mut s, &cat, "d").is_err());
        assert_eq!(power(&s), 100.);
    }
    #[test]
    fn mission_activation_completion_timeout_and_forced_activation() {
        let (mut s, cat) = fixture(
            "m={allowed={always=no} activation={always=no} days_mission_timeout=2 available={has_country_flag=done} complete_effect={add_political_power=8} timeout_effect={add_political_power=-17}}",
        );
        tick(&mut s, &cat, 0);
        assert!(
            s.campaign
                .decisions
                .active
                .get("A")
                .is_none_or(|m| m.is_empty())
        );
        run(&mut s, &cat, "activate_mission=m").unwrap();
        assert!(
            s.campaign_conditions(
                &Context::country("A"),
                &parse_pdx("has_active_mission=m").unwrap(),
                &cat,
                &Default::default()
            )
            .unwrap()
        );
        run(&mut s, &cat, "set_country_flag=done").unwrap();
        tick(&mut s, &cat, 24);
        assert_eq!(power(&s), 108.);
        run(&mut s, &cat, "clr_country_flag=done activate_mission=m").unwrap();
        let bytes = serde_json::to_vec(&s).unwrap();
        s = serde_json::from_slice(&bytes).unwrap();
        tick(&mut s, &cat, 72);
        assert_eq!(power(&s), 91.);
        tick(&mut s, &cat, 96);
        assert_eq!(power(&s), 91.);
    }
    #[test]
    fn selectable_mission_preserves_remaining_timer_and_can_transition_to_timed_decision() {
        let (mut s, cat) = fixture(
            "m={selectable_mission=yes cost=12 days_mission_timeout=5 days_remove=2 complete_effect={set_variable={var=remaining value=days_mission_timeout@m} add_political_power=3} remove_effect={add_political_power=10}}",
        );
        run(&mut s, &cat, "activate_mission=m").unwrap();
        tick(&mut s, &cat, 24);
        assert_eq!(power(&s), 100.);
        select(&mut s, &cat, "m").unwrap();
        assert_eq!(power(&s), 91.);
        assert_eq!(
            s.research.country("A").unwrap().context.variables["remaining"],
            4.
        );
        assert!(!s.campaign.decisions.active["A"]["m"].mission);
        tick(&mut s, &cat, 72);
        assert_eq!(power(&s), 101.);
    }
    #[test]
    fn cancellation_wins_over_expiry_and_explicit_removal_grants_no_callbacks() {
        let (mut s, cat) = fixture(
            "d={days_remove=1 cancel_trigger={has_country_flag=stop} cancel_effect={add_political_power=2} remove_effect={add_political_power=100}} m={days_mission_timeout=1 available={always=no} timeout_effect={add_political_power=100}}",
        );
        select(&mut s, &cat, "d").unwrap();
        run(
            &mut s,
            &cat,
            "set_country_flag=stop activate_mission=m remove_mission=m",
        )
        .unwrap();
        tick(&mut s, &cat, 24);
        assert_eq!(power(&s), 102.);
        run(&mut s, &cat, "clr_country_flag=stop").unwrap();
        tick(&mut s, &cat, 48);
        select(&mut s, &cat, "d").unwrap();
        run(&mut s, &cat, "remove_decision=d").unwrap();
        tick(&mut s, &cat, 96);
        assert_eq!(power(&s), 102.);
    }
    #[test]
    fn timeout_can_reactivate_same_mission_without_being_erased() {
        let (mut s, cat) = fixture(
            "m={days_mission_timeout=1 available={always=no} timeout_effect={add_political_power=2 activate_mission=m}}",
        );
        run(&mut s, &cat, "activate_mission=m").unwrap();
        tick(&mut s, &cat, 24);
        assert_eq!(power(&s), 102.);
        assert_eq!(s.campaign.decisions.active["A"]["m"].deadline, Some(48));
        tick(&mut s, &cat, 24);
        assert_eq!(power(&s), 102.);
        tick(&mut s, &cat, 48);
        assert_eq!(power(&s), 104.);
    }
    #[test]
    fn timer_adjustment_and_atomic_failed_timeout() {
        let (mut s, cat) = fixture(
            "m={days_mission_timeout=2 available={always=no} timeout_effect={add_political_power=50 divide_variable={var=missing value=0}}}",
        );
        run(
            &mut s,
            &cat,
            "activate_mission=m add_days_mission_timeout={mission=m days=-1}",
        )
        .unwrap();
        tick(&mut s, &cat, 24);
        assert_eq!(power(&s), 100.);
        assert!(s.campaign.decisions.active["A"]["m"].blocked.is_some());
        tick(&mut s, &cat, 48);
        assert_eq!(power(&s), 100.);
    }
    #[test]
    fn start_only_allowed_is_persisted_and_availability_is_rechecked() {
        let (mut s, cat) = fixture(
            "d={allowed={has_country_flag=initial} visible={always=yes} available={has_political_power>50} cost=40}",
        );
        run(&mut s, &cat, "set_country_flag=initial").unwrap();
        s.initialize_decisions(&cat, &Default::default());
        run(&mut s, &cat, "clr_country_flag=initial").unwrap();
        select(&mut s, &cat, "d").unwrap();
        assert_eq!(power(&s), 60.);
        tick(&mut s, &cat, 24);
        run(&mut s, &cat, "set_political_power=50").unwrap();
        assert!(select(&mut s, &cat, "d").is_err());
        assert_eq!(power(&s), 50.);
    }
    #[test]
    fn fixed_seed_is_stable_per_decision_and_no_unknown_field_is_ignored() {
        let (mut s, mut cat) = fixture(
            "d={days_re_enable=0 complete_effect={random_list={1={set_variable={var=roll value=1}} 1={set_variable={var=roll value=2}}}}}",
        );
        select(&mut s, &cat, "d").unwrap();
        let n = s.research.country("A").unwrap().context.variables["roll"];
        for _ in 0..5 {
            select(&mut s, &cat, "d").unwrap();
            assert_eq!(
                s.research.country("A").unwrap().context.variables["roll"],
                n
            );
        }
        cat.scripts
            .decisions
            .get_mut("d")
            .unwrap()
            .raw
            .extend(parse_pdx("state_target=yes").unwrap());
        let before = serde_json::to_value(&s).unwrap();
        assert!(
            select(&mut s, &cat, "d")
                .unwrap_err()
                .contains("state_target")
        );
        assert_eq!(before, serde_json::to_value(&s).unwrap());
    }
}

#[derive(Debug, Clone)]
pub struct DecisionRow {
    pub id: String,
    pub title: String,
    pub category: String,
    pub description: String,
    pub status: String,
    pub button: String,
    pub selectable: bool,
}
impl GameSession {
    pub fn decision_rows(
        &self,
        t: &str,
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
    ) -> Vec<DecisionRow> {
        let mut j = Journal::new(self, cat, rules);
        let ctx = Context::country(t);
        let text = |key: &str| {
            cat.scripts
                .localisation
                .get(key)
                .cloned()
                .unwrap_or_else(|| key.into())
        };
        let mut rows = Vec::new();
        for (id, record) in &cat.scripts.decisions {
            let active = self
                .campaign
                .decisions
                .active
                .get(t)
                .and_then(|m| m.get(id));
            if active.is_none() && j.decision_allowed(t, id).is_ok_and(|a| !a) {
                continue;
            }
            let mut row = DecisionRow {
                id: id.clone(),
                title: text(pdx::atom(&record.raw, "name").unwrap_or(id)),
                category: text(&record.category),
                description: text(pdx::atom(&record.raw, "desc").unwrap_or(&format!("{id}_desc"))),
                status: String::new(),
                button: String::new(),
                selectable: false,
            };
            if row.description == format!("{id}_desc") {
                row.description.clear();
            }
            let result = (|| -> Result<bool, String> {
                decision_data::definition(cat, id)?;
                j.preflight_decision(record)?;
                if active.is_none() && pdx::atom(&record.raw, "days_mission_timeout").is_some() {
                    return Ok(false);
                }
                if active.is_none() && !j.decision_visible(record, &ctx)? {
                    return Ok(false);
                }
                if let Some(job) = active {
                    let remaining = job.deadline.map_or("без срока".into(), |due| {
                        format!("{} ч", due.saturating_sub(self.clock.elapsed_hours))
                    });
                    row.status = format!(
                        "{} · {remaining}",
                        if job.mission {
                            "Миссия"
                        } else {
                            "Действует"
                        }
                    );
                    if let Some(e) = &job.blocked {
                        return Err(e.clone());
                    }
                    if !job.mission || !flag(&record.raw, "selectable_mission", false) {
                        return Ok(true);
                    }
                } else if !j.decision_ready(t, id, record) {
                    row.status = if flag(&record.raw, "fire_only_once", false)
                        && self
                            .campaign
                            .decisions
                            .used
                            .get(t)
                            .is_some_and(|m| m.contains(id))
                    {
                        "Уже принято".into()
                    } else {
                        format!(
                            "Повтор через {} ч",
                            self.campaign
                                .decisions
                                .cooldowns
                                .get(t)
                                .and_then(|m| m.get(id))
                                .copied()
                                .unwrap_or(self.clock.elapsed_hours)
                                .saturating_sub(self.clock.elapsed_hours)
                        )
                    };
                    return Ok(true);
                }
                let cost = j.decision_cost(&record.raw, &ctx)?;
                row.button = format!(
                    "{} · {cost:.0} PP",
                    if active.is_some() {
                        "Завершить"
                    } else {
                        "Принять"
                    }
                );
                if !j.decision_available(record, &ctx)? {
                    row.status = "Условия не выполнены".into();
                    return Ok(true);
                }
                if cost > 0.
                    && self
                        .campaign
                        .countries
                        .get(t)
                        .and_then(|p| p.power)
                        .is_none_or(|p| p < cost)
                {
                    row.status = "Недостаточно политической власти".into();
                    return Ok(true);
                }
                row.selectable = true;
                if active.is_none() {
                    row.status = "Доступно".into();
                }
                Ok(true)
            })();
            match result {
                Ok(false) => continue,
                Err(e) => {
                    row.status = format!("Заблокировано: {e}");
                    row.selectable = false;
                }
                _ => {}
            }
            rows.push(row);
        }
        rows.sort_by(|a, b| a.category.cmp(&b.category).then(a.id.cmp(&b.id)));
        rows
    }
}
