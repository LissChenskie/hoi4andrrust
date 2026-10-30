//! Additive native on-action blocks, persisted dispatch and bounded scheduling.
use super::*;
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OnActionRuntime {
    pub startup: bool,
    pub dispatched: BTreeMap<String, u64>,
    pub transition_error: Option<String>,
}
impl OnActionRuntime {
    pub fn validate(&self, w: &crate::world::World, hour: u64) -> Result<(), String> {
        if self.dispatched.iter().any(|(key, h)| {
            *h > hour
                || key
                    .rsplit_once(':')
                    .is_none_or(|(_, t)| !w.countries.contains_key(t))
        }) {
            return Err("Invalid on-action dispatch history".into());
        }
        Ok(())
    }
}
impl Journal<'_> {
    fn on_action(&mut self, ctx: &Context, raw: &[Entry], source: &str) -> Result<(), String> {
        for e in raw {
            self.step(0)?;
            if e.operator != "=" {
                return Err("Invalid on-action operator".into());
            }
            match e.key.as_str() {
                "effect" => {
                    let b = body(e)?;
                    preflight(b, self.cat)?;
                    self.execute(b, ctx, source, 0)?;
                }
                "random_events" => {
                    let mut entries = Vec::new();
                    let mut total = 0u64;
                    for entry in body(e)? {
                        self.step(0)?;
                        let weight = entry
                            .key
                            .parse::<u64>()
                            .map_err(|_| "Invalid random event weight")?;
                        let id = atom(entry)?;
                        if entry.operator != "=" {
                            return Err("Invalid random event operator".into());
                        }
                        if weight == 0 {
                            continue;
                        }
                        if id != "0" {
                            let (_, event) = event(self.cat, id)?;
                            check_event(&event)?;
                            let t = self.tag(ctx)?;
                            if pdx::atom(&event, "fire_only_once") == Some("yes")
                                && self.state.once.get(&t).is_some_and(|s| s.contains(id))
                            {
                                continue;
                            }
                            if !self.conditions(
                                pdx::block(&event, "trigger").unwrap_or(&[]),
                                ctx,
                                0,
                            )? {
                                continue;
                            }
                        }
                        total = total
                            .checked_add(weight)
                            .ok_or("Random event weights overflow")?;
                        entries.push((total, id));
                    }
                    if total == 0 {
                        continue;
                    }
                    // Exact integer rejection sampling; the campaign RNG is part of the journal.
                    let threshold = total.wrapping_neg() % total;
                    let n = loop {
                        let n = self.state.to_mut().rng.next_u64();
                        if n >= threshold {
                            break n % total;
                        }
                    };
                    if let Some((_, id)) = entries.into_iter().find(|(sum, _)| n < *sum) {
                        if id != "0" {
                            let (source, raw) = event(self.cat, id)?;
                            self.state.to_mut().schedule(
                                self.s.clock.elapsed_hours,
                                ctx.clone(),
                                source,
                                Payload::Event { id: id.into(), raw },
                            )?;
                        }
                    }
                }
                key => return Err(format!("Unsupported on-action field {key}")),
            }
        }
        Ok(())
    }
}
impl GameSession {
    pub(super) fn run_on_action(
        &mut self,
        ctx: &Context,
        raw: &[Entry],
        source: &str,
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
    ) -> Result<(), String> {
        self.campaign_transaction(cat, rules, |j| j.on_action(ctx, raw, source))
    }
    fn enqueue_on_action(
        &mut self,
        hook: &str,
        ctx: Context,
        cat: &ResearchCatalogue,
    ) -> Result<(), String> {
        let blocks: Vec<_> = cat
            .scripts
            .on_actions
            .get(hook)
            .map(Vec::as_slice)
            .unwrap_or(&[])
            .iter()
            .filter(|block| !self.campaign.queue.values().any(|a| a.blocked.is_some()
                && a.context.scope == ctx.scope && a.source == block.source
                && matches!(&a.payload, Payload::OnAction { hook: queued_hook, raw } if queued_hook == hook && raw == &block.raw)))
            .collect();
        if self.campaign.queue.len() + self.campaign.pending.len() + blocks.len() > QUEUE {
            return Err("On-action queue capacity exceeded".into());
        }
        if self
            .campaign
            .next_id
            .checked_add(blocks.len() as u64)
            .is_none()
        {
            return Err("Campaign ID exhausted".into());
        }
        for block in blocks {
            self.campaign.schedule(
                self.clock.elapsed_hours,
                ctx.clone(),
                block.source.clone(),
                Payload::OnAction {
                    hook: hook.into(),
                    raw: block.raw.clone(),
                },
            )?;
        }
        Ok(())
    }
    /// Only called after a NEW campaign's country selection, never while restoring a save.
    pub fn start_campaign_scripts(
        &mut self,
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
    ) -> Result<(), String> {
        if self.campaign.on_actions.startup {
            return Ok(());
        }
        self.enqueue_on_action(
            "on_startup",
            Context {
                scope: Scope::Global,
                root: Scope::Global,
                from: None,
                previous: vec![],
                tokens: BTreeMap::new(),
            },
            cat,
        )?;
        self.campaign.on_actions.startup = true;
        // Historical wars at scenario load are a baseline, not new declarations.
        self.military.script_transitions.clear();
        self.initialize_decisions(cat, rules);
        // Startup precedes the first daily pulse; does not advance the game clock.
        self.campaign.last_hour = None;
        self.tick_campaign(cat, rules);
        Ok(())
    }
    pub fn tick_on_actions(&mut self, cat: &ResearchCatalogue) {
        let h = self.clock.elapsed_hours;
        if h % 24 != 0 {
            return;
        }
        let tags: Vec<_> = self
            .world
            .countries
            .keys()
            .filter(|t| !self.campaign.inactive.contains(*t))
            .cloned()
            .collect();
        let hooks: Vec<_> = cat
            .scripts
            .on_actions
            .keys()
            .filter(|key| {
                key.as_str() == "on_daily"
                    || key.starts_with("on_daily_")
                    || ((key.as_str() == "on_weekly" || key.starts_with("on_weekly_"))
                        && h / 24 % 7 == 0)
                    || ((key.as_str() == "on_monthly" || key.starts_with("on_monthly_"))
                        && self.clock.date.day == 1)
            })
            .cloned()
            .collect();
        for hook in hooks {
            for t in &tags {
                let base = if hook.starts_with("on_daily") {
                    "on_daily"
                } else if hook.starts_with("on_weekly") {
                    "on_weekly"
                } else {
                    "on_monthly"
                };
                if hook != base && hook.strip_prefix(&format!("{base}_")) != Some(t.as_str()) {
                    continue;
                }
                let key = format!("{hook}:{t}");
                if self
                    .campaign
                    .on_actions
                    .dispatched
                    .get(&key)
                    .is_some_and(|last| *last >= h)
                {
                    continue;
                }
                match self.enqueue_on_action(&hook, Context::country(t), cat) {
                    Ok(()) => {
                        self.campaign.on_actions.dispatched.insert(key, h);
                    }
                    Err(e) => {
                        self.campaign
                            .decisions
                            .failures
                            .entry(t.clone())
                            .or_default()
                            .insert(key, e);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdx::parse_pdx;
    fn fixture(raw: &str) -> (GameSession, ResearchCatalogue) {
        let mut s = GameSession::new(crate::warfare::tests::world());
        s.campaign.countries.insert(
            "A".into(),
            CountryPolitics {
                power: Some(100.),
                ..Default::default()
            },
        );
        let mut cat = ResearchCatalogue::default();
        for e in parse_pdx(raw).unwrap() {
            cat.scripts
                .on_actions
                .entry(e.key.clone())
                .or_default()
                .push(crate::research_script::ScriptSource {
                    kind: "on_action".into(),
                    id: e.key.clone(),
                    source: "authored".into(),
                    conditions: vec![],
                    effects: vec![],
                    raw: e.block().unwrap().to_vec(),
                });
        }
        (s, cat)
    }
    fn tick(s: &mut GameSession, cat: &ResearchCatalogue, h: u64, day: u8) {
        s.clock.elapsed_hours = h;
        s.clock.date.day = day;
        s.tick_campaign(cat, &Default::default());
    }
    #[test]
    fn startup_has_no_country_scope_is_additive_and_never_replays_after_restore() {
        let (mut s, cat) = fixture(
            "on_startup={effect={set_global_flag=started} effect={every_country={limit={tag=A} add_political_power=3}}} on_startup={effect={every_country={limit={tag=A} add_political_power=4}}}",
        );
        s.start_campaign_scripts(&cat, &Default::default()).unwrap();
        assert_eq!(s.campaign.countries["A"].power, Some(107.));
        assert!(s.research.global_flags.contains("started"));
        let bytes = serde_json::to_vec(&s).unwrap();
        s = serde_json::from_slice(&bytes).unwrap();
        s.start_campaign_scripts(&cat, &Default::default()).unwrap();
        tick(&mut s, &cat, 1, 1);
        assert_eq!(s.campaign.countries["A"].power, Some(107.));
        s.campaign.validate(&s.world, 1).unwrap();
        let (mut s, cat) =
            fixture("on_startup={effect={set_global_flag=partial add_political_power=9}}");
        s.start_campaign_scripts(&cat, &Default::default()).unwrap();
        assert!(!s.research.global_flags.contains("partial"));
        assert!(
            s.campaign
                .queue
                .values()
                .next()
                .unwrap()
                .blocked
                .as_ref()
                .unwrap()
                .contains("country scope")
        );
    }
    #[test]
    fn daily_country_weekly_and_monthly_hooks_have_saved_once_markers() {
        let (mut s, cat) = fixture(
            "on_daily_A={effect={add_political_power=2}} on_daily_A={effect={add_political_power=3}} on_weekly_A={effect={add_political_power=10}} on_monthly_A={effect={add_political_power=20}}",
        );
        tick(&mut s, &cat, 24, 2);
        assert_eq!(s.campaign.countries["A"].power, Some(105.));
        let bytes = serde_json::to_vec(&s).unwrap();
        s = serde_json::from_slice(&bytes).unwrap();
        s.campaign.last_hour = None;
        tick(&mut s, &cat, 24, 2);
        assert_eq!(s.campaign.countries["A"].power, Some(105.));
        tick(&mut s, &cat, 168, 8);
        assert_eq!(s.campaign.countries["A"].power, Some(120.));
        s.clock.date.month = 2;
        tick(&mut s, &cat, 744, 1);
        assert_eq!(s.campaign.countries["A"].power, Some(145.));
        s.campaign.validate(&s.world, 744).unwrap();
    }
    #[test]
    fn random_events_use_actual_eligibility_and_schedule_a_real_event() {
        let (mut s, mut cat) = fixture(
            "on_daily_A={random_events={100=blocked 1=reward}} on_weekly_A={random_events={1=0}}",
        );
        for (id, raw) in [
            (
                "blocked",
                "id=blocked hidden=yes trigger={always=no} immediate={add_political_power=500}",
            ),
            (
                "reward",
                "id=reward hidden=yes fire_only_once=yes trigger={always=yes} immediate={add_political_power=7}",
            ),
        ] {
            cat.scripts.sources.insert(
                format!("event:{id}"),
                crate::research_script::ScriptSource {
                    kind: "event".into(),
                    id: id.into(),
                    source: "authored".into(),
                    conditions: vec![],
                    effects: vec![],
                    raw: parse_pdx(raw).unwrap(),
                },
            );
        }
        tick(&mut s, &cat, 24, 2);
        assert_eq!(s.campaign.countries["A"].power, Some(107.));
        assert!(s.campaign.once["A"].contains("reward"));
        tick(&mut s, &cat, 48, 3);
        assert_eq!(s.campaign.countries["A"].power, Some(107.));
        assert!(s.campaign.queue.is_empty());
    }
    #[test]
    fn unsupported_hook_rolls_back_entire_block_and_stays_blocked_without_daily_duplicate() {
        let (mut s, cat) =
            fixture("on_daily_A={effect={add_political_power=3} events={not_supported}} ");
        tick(&mut s, &cat, 24, 2);
        assert_eq!(s.campaign.countries["A"].power, Some(100.));
        assert_eq!(s.campaign.queue.len(), 1);
        assert!(
            s.campaign
                .queue
                .values()
                .next()
                .unwrap()
                .blocked
                .as_ref()
                .unwrap()
                .contains("Unsupported on-action")
        );
        s.campaign.last_hour = None;
        tick(&mut s, &cat, 24, 2);
        assert_eq!(s.campaign.queue.len(), 1);
    }
    #[test]
    fn blocked_daily_handler_does_not_fill_the_queue_again_each_day() {
        let (mut s, cat) = fixture("on_daily_A={effect={add_political_power=3} unknown_field=yes}");
        for day in 1..20 {
            tick(&mut s, &cat, day * 24, (day + 1) as u8);
        }
        assert_eq!(s.campaign.countries["A"].power, Some(100.));
        assert_eq!(s.campaign.queue.len(), 1);
        assert_eq!(s.campaign.on_actions.dispatched["on_daily_A:A"], 19 * 24);
    }
    #[test]
    fn many_additive_startup_blocks_obey_budget_and_resume_without_duplication() {
        let source = (0..70)
            .map(|_| "on_startup={effect={every_country={limit={tag=A} add_political_power=1}}}")
            .collect::<String>();
        let (mut s, cat) = fixture(&source);
        s.start_campaign_scripts(&cat, &Default::default()).unwrap();
        assert_eq!(s.campaign.countries["A"].power, Some(164.));
        assert_eq!(s.campaign.queue.len(), 6);
        let bytes = serde_json::to_vec(&s).unwrap();
        s = serde_json::from_slice(&bytes).unwrap();
        tick(&mut s, &cat, 1, 1);
        assert_eq!(s.campaign.countries["A"].power, Some(170.));
        assert!(s.campaign.queue.is_empty());
        s.start_campaign_scripts(&cat, &Default::default()).unwrap();
        assert_eq!(s.campaign.countries["A"].power, Some(170.));
    }
}

/// A saved transition owns its context, including temporary token values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookCall {
    pub hook: String,
    pub context: Context,
}
impl HookCall {
    pub fn country(hook: &str, tag: &str, from: Option<&str>) -> Self {
        let mut context = Context::country(tag);
        context.from = from.map(|t| Scope::Country(t.into()));
        Self {
            hook: hook.into(),
            context,
        }
    }
}
impl Journal<'_> {
    pub(super) fn dispatch_hook(&mut self, call: &HookCall) -> Result<(), String> {
        self.hook_depth += 1;
        let result = (|| {
            self.step(0)?;
            let blocks = self
                .cat
                .scripts
                .on_actions
                .get(&call.hook)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            for block in blocks {
                self.on_action(&call.context, &block.raw, &block.source)?;
            }
            Ok(())
        })();
        self.hook_depth -= 1;
        result
    }
    pub(super) fn flush_deferred_hooks(&mut self) -> Result<(), String> {
        while let Some(call) = self.deferred_hooks.pop_front() {
            self.dispatch_hook(&call)?;
        }
        Ok(())
    }
    pub(super) fn government_changed(
        &mut self,
        tag: &str,
        old: Option<&str>,
    ) -> Result<(), String> {
        let mut context = Context::country(tag);
        if let Some(old) = old {
            context
                .tokens
                .insert("old_ideology_token".into(), old.into());
        }
        if old.is_none()
            && ["on_ruling_party_change", "on_ruling_party_change_immediate"]
                .iter()
                .any(|hook| {
                    self.cat
                        .scripts
                        .on_actions
                        .get(*hook)
                        .is_some_and(|blocks| !blocks.is_empty())
                })
        {
            return Err("Old ruling ideology unknown for government-change hooks".into());
        }
        self.dispatch_hook(&HookCall {
            hook: "on_ruling_party_change_immediate".into(),
            context: context.clone(),
        })?;
        for hook in ["on_government_change", "on_ruling_party_change"] {
            if self.cat.scripts.on_actions.contains_key(hook) {
                if self.deferred_hooks.len() >= QUEUE {
                    return Err("Deferred hook capacity exceeded".into());
                }
                self.deferred_hooks.push_back(HookCall {
                    hook: hook.into(),
                    context: context.clone(),
                });
            }
        }
        Ok(())
    }
    /// Typed tokens are kept separate from numeric variables; unknown token
    /// expressions must never compare as a made-up ideology string or zero.
    pub(super) fn token_value(&mut self, raw: &str, c: &Context) -> Result<Option<String>, String> {
        if raw.parse::<f64>().is_ok() {
            return Ok(None);
        }
        if let Some(value) = raw.strip_prefix("token:") {
            if value.is_empty() || value.len() > 4096 {
                return Err("Invalid token literal".into());
            }
            return Ok(Some(value.into()));
        }
        let key = raw.strip_prefix("var:").unwrap_or(raw);
        if let Some(value) = c.tokens.get(key) {
            return Ok(Some(value.clone()));
        }
        if key == "old_ideology_token" {
            return Err("Temporary old_ideology_token is unavailable".into());
        }
        let (tag, key) = if let Some((scope, key)) = key.split_once('.') {
            let Scope::Country(t) = self.scope(scope, c)? else {
                return Err("Token variable requires country scope".into());
            };
            (t, key)
        } else {
            match &c.scope {
                Scope::Country(t) => (t.clone(), key),
                _ => return Ok(None),
            }
        };
        Ok(self
            .research(&tag)
            .context
            .token_variables
            .get(key)
            .cloned())
    }
    pub(super) fn token_name(&mut self, raw: &str, c: &Context) -> Result<String, String> {
        if let Some(value) = self.token_value(raw, c)? {
            return Ok(value);
        }
        if raw.starts_with("var:") {
            return Err(format!("Unknown token expression {raw}"));
        }
        Ok(raw.into())
    }
    pub(super) fn check_variable(&mut self, e: &Entry, c: &Context) -> Result<bool, String> {
        let b = body(e)?;
        let (var, value, op) = if pdx::atom(b, "var").is_some() {
            fields(e, &["var", "value", "compare", "tooltip"])?;
            (
                field(b, "var")?,
                field(b, "value")?,
                pdx::atom(b, "compare").unwrap_or("greater_than_or_equals"),
            )
        } else {
            let values: Vec<_> = b.iter().filter(|e| e.key != "tooltip").collect();
            if values.len() != 1 {
                return Err("Expected one variable comparison".into());
            }
            let v = values[0];
            (v.key.as_str(), atom(v)?, v.operator.as_str())
        };
        let left = self.token_value(var, c)?;
        let right = self.token_value(value, c)?;
        if left.is_some() || right.is_some() {
            let (Some(a), Some(b)) = (left, right) else {
                return Err("Cannot compare token and numeric/unknown variable".into());
            };
            return match op {
                "equals" | "=" | "==" => Ok(a == b),
                "not_equals" | "!=" => Ok(a != b),
                _ => Err("Tokens only support equality comparisons".into()),
            };
        }
        let a = self.num(var, c)?;
        let b = self.num(value, c)?;
        match op {
            "greater_than" | ">" => Ok(a > b),
            "greater_than_or_equals" | ">=" => Ok(a >= b),
            "less_than" | "<" => Ok(a < b),
            "less_than_or_equals" | "<=" => Ok(a <= b),
            "equals" | "=" | "==" => Ok(a == b),
            "not_equals" | "!=" => Ok(a != b),
            _ => Err("Unknown variable comparison".into()),
        }
    }
}
impl GameSession {
    pub(super) fn run_hook_batch(
        &mut self,
        calls: &[HookCall],
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
    ) -> Result<(), String> {
        self.campaign_transaction(cat, rules, |j| {
            for call in calls {
                j.dispatch_hook(call)?;
            }
            Ok(())
        })
    }
    pub(crate) fn reserve_hook_batch(
        &mut self,
        calls: Vec<HookCall>,
        cat: &ResearchCatalogue,
    ) -> Result<Option<u64>, String> {
        let calls: Vec<_> = calls
            .into_iter()
            .filter(|c| {
                cat.scripts
                    .on_actions
                    .get(&c.hook)
                    .is_some_and(|b| !b.is_empty())
            })
            .collect();
        if calls.is_empty() {
            return Ok(None);
        }
        let ctx = calls[0].context.clone();
        self.campaign
            .schedule(
                self.clock.elapsed_hours,
                ctx,
                "simulation transition hooks".into(),
                Payload::HookBatch { calls },
            )
            .map(Some)
    }
    /// The physical external transition has committed. Failed callback effects
    /// roll back and remain visible in one saved blocked job; never replay it silently.
    pub(crate) fn finish_reserved_hook_batch(
        &mut self,
        id: Option<u64>,
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
    ) {
        let Some(id) = id else {
            return;
        };
        let Some(a) = self.campaign.queue.get(&id).cloned() else {
            return;
        };
        if let Payload::HookBatch { calls } = &a.payload {
            match self.run_hook_batch(calls, cat, rules) {
                Ok(()) => {
                    self.campaign.queue.remove(&id);
                }
                Err(e) => {
                    self.campaign.queue.get_mut(&id).unwrap().blocked = Some(e);
                }
            }
        }
    }
    pub(crate) fn before_civil_annex(
        &mut self,
        winner: &str,
        loser: &str,
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
    ) -> Result<Option<u64>, String> {
        self.campaign_transaction(cat, rules, |j| {
            let calls: Vec<_> = ["on_annex", "on_civil_war_end"]
                .iter()
                .filter(|key| {
                    cat.scripts
                        .on_actions
                        .get(**key)
                        .is_some_and(|b| !b.is_empty())
                })
                .map(|hook| HookCall::country(hook, winner, Some(loser)))
                .collect();
            let post = if calls.is_empty() {
                None
            } else {
                Some(j.state.to_mut().schedule(
                    j.s.clock.elapsed_hours,
                    Context::country(winner),
                    "civil-war post-annexation hooks".into(),
                    Payload::HookBatch { calls },
                )?)
            };
            j.dispatch_hook(&HookCall::country(
                "on_civil_war_end_before_annexation",
                winner,
                Some(loser),
            ))?;
            j.flush_deferred_hooks()?;
            // The callback may award men/equipment. Revalidate the *staged*
            // totals before committing it, so overflow cannot reward a failed annexation.
            let men = |t: &str| {
                j.men
                    .get(t)
                    .copied()
                    .or_else(|| j.s.military.countries.get(t).map(|a| a.manpower_pool))
                    .ok_or("Civil-war army missing")
            };
            men(winner)?
                .checked_add(men(loser)?)
                .ok_or("Post-hook manpower overflow")?;
            let stock = |t: &str| {
                j.stock
                    .get(t)
                    .or_else(|| j.s.production.country(t))
                    .cloned()
                    .unwrap_or_default()
            };
            if !stock(winner).can_absorb(&stock(loser)) {
                return Err("Post-hook inventory/production overflow".into());
            }
            Ok(post)
        })
    }
}

impl GameSession {
    /// Consume the authoritative warfare outbox, preserving attacker direction.
    /// A full script queue leaves the transition saved for the next attempt.
    pub fn sync_war_hooks(&mut self, cat: &ResearchCatalogue) -> Result<(), String> {
        use crate::warfare::WarTransition;
        while let Some(change) = self.military.script_transitions.front().cloned() {
            let mut calls = Vec::new();
            match change {
                WarTransition::Added {
                    attacker,
                    defender,
                    declared,
                    entered_attacker,
                    entered_defender,
                } => {
                    if declared {
                        calls.push(HookCall::country(
                            "on_declare_war",
                            &attacker,
                            Some(&defender),
                        ));
                    }
                    calls.push(HookCall::country(
                        "on_war_relation_added",
                        &attacker,
                        Some(&defender),
                    ));
                    if entered_attacker {
                        calls.push(HookCall::country("on_war", &attacker, Some(&defender)));
                    }
                    if entered_defender {
                        calls.push(HookCall::country("on_war", &defender, Some(&attacker)));
                    }
                }
                WarTransition::Ended {
                    a,
                    b,
                    peace_a,
                    peace_b,
                } => {
                    if peace_a && !self.campaign.inactive.contains(&a) {
                        calls.push(HookCall::country("on_peace", &a, Some(&b)));
                    }
                    if peace_b && !self.campaign.inactive.contains(&b) {
                        calls.push(HookCall::country("on_peace", &b, Some(&a)));
                    }
                }
            }
            self.reserve_hook_batch(calls, cat)?;
            self.military.script_transitions.pop_front();
        }
        Ok(())
    }
}

impl GameSession {
    pub fn flush_simulation_hooks(&mut self, cat: &ResearchCatalogue, rules: &ResearchRules) {
        self.campaign.on_actions.transition_error = self.sync_war_hooks(cat).err();
        self.process_campaign_queue(cat, rules);
    }
}

#[cfg(test)]
mod transition_tests {
    use super::*;
    use crate::pdx::parse_pdx;
    fn fixture(raw: &str) -> (GameSession, ResearchCatalogue) {
        let mut s = GameSession::new(crate::warfare::tests::world());
        s.world.countries.insert(
            "C".into(),
            crate::world::Country {
                tag: "C".into(),
                state_ids: Default::default(),
            },
        );
        for tag in ["A", "B", "C"] {
            let c = s.research.country_mut(tag, 0, &Default::default());
            c.context.tag = tag.into();
            c.context.original_tag = tag.into();
            c.context.ideology = Some("neutrality".into());
            s.campaign.countries.insert(
                tag.into(),
                CountryPolitics {
                    power: Some(100.),
                    stability: Some(0.5),
                    war_support: Some(0.5),
                },
            );
        }
        let mut cat = ResearchCatalogue::default();
        for e in parse_pdx(raw).unwrap() {
            cat.scripts
                .on_actions
                .entry(e.key.clone())
                .or_default()
                .push(crate::research_script::ScriptSource {
                    kind: "on_action".into(),
                    id: e.key.clone(),
                    source: format!("authored:{}", e.span.line),
                    conditions: vec![],
                    effects: vec![],
                    raw: e.block().unwrap().to_vec(),
                });
        }
        (s, cat)
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
    fn var(s: &GameSession, t: &str, k: &str) -> f64 {
        s.research
            .country(t)
            .unwrap()
            .context
            .variables
            .get(k)
            .copied()
            .unwrap_or(0.)
    }
    fn flag(s: &GameSession, t: &str, k: &str) -> bool {
        s.research.country(t).unwrap().flags.contains(k)
    }
    #[test]
    fn government_callbacks_observe_immediate_tail_and_deferred_order_with_owned_old_token() {
        let (mut s, cat) = fixture(
            "on_ruling_party_change_immediate={effect={if={limit={check_variable={var=old_ideology_token value=token:neutrality compare=equals} has_government=democratic} set_variable={var=sequence value=1} set_variable={var=saved_old value=old_ideology_token}}}} on_government_change={effect={add_to_variable={var=sequence value=10}}} on_ruling_party_change={effect={if={limit={check_variable={sequence=12} check_variable={var=old_ideology_token value=token:neutrality compare=equals}} set_country_flag=ordinary_after_tail}}}",
        );
        run(&mut s,&cat,"set_variable={var=sequence value=0} set_politics={ruling_party=democratic} add_to_variable={var=sequence value=1}").unwrap();
        assert_eq!(var(&s, "A", "sequence"), 12.);
        assert!(flag(&s, "A", "ordinary_after_tail"));
        assert_eq!(
            s.research.country("A").unwrap().context.token_variables["saved_old"],
            "neutrality"
        );
        assert!(
            !s.research
                .country("A")
                .unwrap()
                .context
                .token_variables
                .contains_key("old_ideology_token")
        );
        assert!(
            s.campaign_conditions(
                &Context::country("A"),
                &parse_pdx("check_variable={old_ideology_token=token:neutrality}").unwrap(),
                &cat,
                &Default::default()
            )
            .is_err()
        );
        run(&mut s, &cat, "set_politics={ruling_party=democratic}").unwrap();
        assert_eq!(var(&s, "A", "sequence"), 12.);
        let bytes = serde_json::to_vec(&s).unwrap();
        s = serde_json::from_slice(&bytes).unwrap();
        assert!(
            s.campaign_conditions(
                &Context::country("A"),
                &parse_pdx("check_variable={saved_old=token:neutrality}").unwrap(),
                &cat,
                &Default::default()
            )
            .unwrap()
        );
    }
    #[test]
    fn callback_scope_uses_affected_country_and_failure_rolls_back_entire_parent_block() {
        let (mut s, cat) = fixture(
            "on_ruling_party_change_immediate={effect={if={limit={tag=ROOT} set_country_flag=affected}}} on_ruling_party_change={effect={add_political_power=4 divide_variable={var=zero value=0}}}",
        );
        let before = serde_json::to_value(&s).unwrap();
        let ctx = Context::country("A").enter(Scope::Country("B".into()));
        assert!(
            s.campaign_effects(
                &ctx,
                &parse_pdx("add_political_power=3 set_politics={ruling_party=democratic}").unwrap(),
                "authored",
                &cat,
                &Default::default()
            )
            .is_err()
        );
        assert_eq!(before, serde_json::to_value(&s).unwrap());
        let mut cat = cat;
        cat.scripts.on_actions.remove("on_ruling_party_change");
        s.campaign_effects(
            &ctx,
            &parse_pdx("set_politics={ruling_party=democratic}").unwrap(),
            "authored",
            &cat,
            &Default::default(),
        )
        .unwrap();
        assert!(flag(&s, "B", "affected"));
        assert!(!flag(&s, "A", "affected"));
    }
    #[test]
    fn recursive_immediate_party_changes_are_bounded_and_atomic() {
        let (mut s, cat) = fixture(
            "on_ruling_party_change_immediate={effect={if={limit={has_government=democratic} set_politics={ruling_party=fascism}} else={set_politics={ruling_party=democratic}}}}",
        );
        let before = serde_json::to_value(&s).unwrap();
        let err = run(
            &mut s,
            &cat,
            "add_political_power=5 set_politics={ruling_party=democratic}",
        )
        .unwrap_err();
        assert!(err.contains("recursion") || err.contains("nesting"));
        assert_eq!(before, serde_json::to_value(&s).unwrap());
    }
    #[test]
    fn real_war_edges_preserve_direction_and_first_war_last_peace_semantics_across_save() {
        let (mut s, cat) = fixture(
            "on_declare_war={effect={set_country_flag=declared FROM={set_country_flag=targeted}}} on_war_relation_added={effect={add_to_variable={var=relations value=1}}} on_war={effect={add_to_variable={var=entered value=1}}} on_peace={effect={add_to_variable={var=peace value=1}}}",
        );
        s.military.declare_war("B", "A", &s.world).unwrap();
        let bytes = serde_json::to_vec(&s).unwrap();
        s = serde_json::from_slice(&bytes).unwrap();
        s.flush_simulation_hooks(&cat, &Default::default());
        assert!(flag(&s, "B", "declared"));
        assert!(flag(&s, "A", "targeted"));
        assert!(!flag(&s, "A", "declared"));
        assert_eq!(var(&s, "A", "entered"), 1.);
        assert_eq!(var(&s, "B", "entered"), 1.);
        s.military.declare_war("B", "A", &s.world).unwrap();
        s.military.declare_war("B", "C", &s.world).unwrap();
        s.flush_simulation_hooks(&cat, &Default::default());
        assert_eq!(var(&s, "B", "entered"), 1.);
        assert_eq!(var(&s, "C", "entered"), 1.);
        assert_eq!(var(&s, "B", "relations"), 2.);
        s.military.remove_war_relation("B", "A");
        s.flush_simulation_hooks(&cat, &Default::default());
        assert_eq!(var(&s, "A", "peace"), 1.);
        assert_eq!(var(&s, "B", "peace"), 0.);
        s.military.remove_war_relation("B", "C");
        s.flush_simulation_hooks(&cat, &Default::default());
        assert_eq!(var(&s, "B", "peace"), 1.);
        assert_eq!(var(&s, "C", "peace"), 1.);
        s.flush_simulation_hooks(&cat, &Default::default());
        assert_eq!(var(&s, "B", "relations"), 2.);
    }
    #[test]
    fn ally_relation_emits_no_declaration_and_full_queue_preserves_outbox() {
        let (mut s, cat) = fixture(
            "on_declare_war={effect={set_country_flag=declared}} on_war_relation_added={effect={set_country_flag=relation}}",
        );
        s.military
            .add_war_relation("A", "B", &s.world, false)
            .unwrap();
        for _ in 0..QUEUE {
            s.campaign
                .schedule(
                    0,
                    Context::country("A"),
                    "authored filler".into(),
                    Payload::Effects(vec![]),
                )
                .unwrap();
        }
        assert!(s.sync_war_hooks(&cat).is_err());
        assert_eq!(s.military.script_transitions.len(), 1);
        s.campaign.queue.clear();
        s.flush_simulation_hooks(&cat, &Default::default());
        assert!(s.military.script_transitions.is_empty());
        assert!(flag(&s, "A", "relation"));
        assert!(!flag(&s, "A", "declared"));
    }
}
