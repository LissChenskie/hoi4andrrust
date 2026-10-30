//! Additive native on-action blocks, persisted dispatch and bounded scheduling.
use super::*;
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OnActionRuntime {
    pub startup: bool,
    pub dispatched: BTreeMap<String, u64>,
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
        let blocks = cat
            .scripts
            .on_actions
            .get(hook)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
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
            },
            cat,
        )?;
        self.campaign.on_actions.startup = true;
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
}
