//! Bounded campaign interpreter. Mutations use a journal; unknown effects and
//! failed conditions cannot commit half a reward. No vanilla country data lives here.
use crate::{
    combat_rules::{CombatRandom, SeededRandom},
    pdx::{self, Entry},
    research::{CountryResearch, ResearchCatalogue, ResearchRules},
    session::GameSession,
};
pub use hoi4_mobile::script_validation::preflight;
use hoi4_mobile::script_validation::{atom, body, check_event, event, field, fields, instantiate};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};
#[path = "decisions.rs"]
pub mod decisions;
#[path = "on_actions.rs"]
pub mod on_actions;
const STEPS: usize = 10000;
const QUEUE: usize = 4096;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    Global,
    Country(String),
    State(u32),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    pub scope: Scope,
    pub root: Scope,
    pub from: Option<Scope>,
    pub previous: Vec<Scope>,
}
impl Context {
    pub fn country(tag: &str) -> Self {
        let s = Scope::Country(tag.into());
        Self {
            scope: s.clone(),
            root: s,
            from: None,
            previous: vec![],
        }
    }
    fn enter(&self, s: Scope) -> Self {
        let mut n = self.clone();
        n.previous.push(self.scope.clone());
        n.scope = s;
        n
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CountryPolitics {
    pub power: Option<f64>,
    pub stability: Option<f64>,
    pub war_support: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Payload {
    Effects(Vec<Entry>),
    Event {
        id: String,
        raw: Vec<Entry>,
    },
    CivilWar(crate::civil_war::SplitPlan),
    OnAction {
        #[serde(default)]
        hook: String,
        raw: Vec<Entry>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scheduled {
    pub id: u64,
    pub due: u64,
    pub context: Context,
    pub source: String,
    pub payload: Payload,
    pub blocked: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingEvent {
    pub id: u64,
    pub event: String,
    pub context: Context,
    pub source: String,
    pub options: Vec<Vec<Entry>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicModifier {
    pub begun: u64,
    pub expires: Option<u64>,
    pub source: String,
    #[serde(default)]
    pub blocked: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CampaignState {
    pub decisions: decisions::DecisionRuntime,
    pub on_actions: on_actions::OnActionRuntime,
    #[serde(default)]
    pub focuses: crate::focus::FocusCampaign,
    #[serde(default)]
    pub politics: crate::politics::PoliticalRuntime,
    #[serde(default)]
    pub dynamic_modifiers: BTreeMap<String, BTreeMap<String, DynamicModifier>>,
    pub countries: BTreeMap<String, CountryPolitics>,
    pub state_flags: BTreeMap<u32, BTreeSet<String>>,
    pub queue: BTreeMap<u64, Scheduled>,
    pub pending: BTreeMap<u64, PendingEvent>,
    pub once: BTreeMap<String, BTreeSet<String>>,
    pub inactive: BTreeSet<String>,
    pub civil_wars: BTreeMap<String, (String, Option<String>)>,
    pub next_id: u64,
    pub last_hour: Option<u64>,
    pub rng: SeededRandom,
}
impl Default for CampaignState {
    fn default() -> Self {
        Self {
            decisions: Default::default(),
            on_actions: Default::default(),
            focuses: Default::default(),
            politics: Default::default(),
            dynamic_modifiers: Default::default(),
            countries: BTreeMap::new(),
            state_flags: BTreeMap::new(),
            queue: BTreeMap::new(),
            pending: BTreeMap::new(),
            once: BTreeMap::new(),
            inactive: BTreeSet::new(),
            civil_wars: BTreeMap::new(),
            next_id: 1,
            last_hour: None,
            rng: SeededRandom::new(19360101),
        }
    }
}
impl CampaignState {
    pub fn schedule(
        &mut self,
        due: u64,
        context: Context,
        source: String,
        payload: Payload,
    ) -> Result<u64, String> {
        if self.queue.len() + self.pending.len() >= QUEUE {
            return Err("Campaign queue capacity exceeded".into());
        }
        let id = self.next_id;
        self.next_id = id.checked_add(1).ok_or("Campaign ID exhausted")?;
        self.queue.insert(
            id,
            Scheduled {
                id,
                due,
                context,
                source,
                payload,
                blocked: None,
            },
        );
        Ok(id)
    }
    pub fn validate(&self, w: &crate::world::World, hour: u64) -> Result<(), String> {
        self.decisions.validate(w, hour)?;
        self.on_actions.validate(w, hour)?;
        let scope = |s: &Scope| match s {
            Scope::Global => true,
            Scope::Country(t) => w.countries.contains_key(t),
            Scope::State(i) => w.states.contains_key(i),
        };
        let ctx = |c: &Context| {
            scope(&c.scope)
                && scope(&c.root)
                && c.from.as_ref().is_none_or(scope)
                && c.previous.len() <= 64
                && c.previous.iter().all(scope)
        };
        if self.next_id == 0
            || self.queue.len() + self.pending.len() > QUEUE
            || self.last_hour.is_some_and(|h| h > hour)
        {
            return Err("Invalid campaign counters".into());
        }
        for (id, a) in &self.queue {
            if *id == 0 || *id != a.id || *id >= self.next_id || !ctx(&a.context) {
                return Err("Invalid scheduled script".into());
            }
        }
        for (id, a) in &self.pending {
            if *id == 0
                || *id != a.id
                || *id >= self.next_id
                || self.queue.contains_key(id)
                || !ctx(&a.context)
            {
                return Err("Invalid pending event".into());
            }
        }
        if self
            .politics
            .last_day
            .iter()
            .any(|(tag, day)| !w.countries.contains_key(tag) || *day > hour / 24)
            || self.politics.income.iter().any(|(tag, i)| {
                !w.countries.contains_key(tag)
                    || [
                        i.base,
                        i.flat,
                        i.factor,
                        i.cost,
                        i.net,
                        i.stability_factor,
                        i.focus_gain,
                        i.focus_factor,
                        i.focus_cost,
                    ]
                    .iter()
                    .any(|n| !n.is_finite())
            })
        {
            return Err("Invalid political income ledger".into());
        }
        if self
            .dynamic_modifiers
            .values()
            .map(|m| m.len())
            .sum::<usize>()
            > 4096
            || self.dynamic_modifiers.iter().any(|(tag, items)| {
                !w.countries.contains_key(tag)
                    || items.iter().any(|(id, m)| {
                        id.is_empty() || m.begun > hour || m.expires.is_some_and(|e| e < m.begun)
                    })
            })
        {
            return Err("Invalid dynamic modifier state".into());
        }
        for (tag, c) in &self.countries {
            if !w.countries.contains_key(tag)
                || [c.power, c.stability, c.war_support]
                    .into_iter()
                    .flatten()
                    .any(|v| !v.is_finite())
                || [c.stability, c.war_support]
                    .into_iter()
                    .flatten()
                    .any(|v| !(0.0..=1.).contains(&v))
            {
                return Err("Invalid political state".into());
            }
        }
        for (tag, f) in &self.focuses.active {
            if !w.countries.contains_key(tag)
                || f.id.is_empty()
                || f.tree.is_empty()
                || !f.progress.is_finite()
                || f.progress < 0.
                || !f.required.is_finite()
                || f.required <= 0.
                || f.progress >= f.required
                || f.begun > hour
            {
                return Err("Invalid active national focus".into());
            }
        }
        if self
            .focuses
            .selected
            .iter()
            .any(|(tag, id)| !w.countries.contains_key(tag) || id.is_empty())
            || self
                .focuses
                .layouts
                .keys()
                .any(|t| !w.countries.contains_key(t))
            || self
                .focuses
                .bypassed
                .keys()
                .any(|t| !w.countries.contains_key(t))
        {
            return Err("Invalid focus country/tree references".into());
        }
        if self.focuses.positions.iter().any(|(tag, nodes)| {
            !w.countries.contains_key(tag)
                || nodes
                    .values()
                    .any(|(x, y)| !x.is_finite() || !y.is_finite())
        }) {
            return Err("Invalid focus layout coordinates".into());
        }
        if self.state_flags.keys().any(|id| !w.states.contains_key(id)) {
            return Err("Invalid state flag".into());
        }
        Ok(())
    }
}
fn integral(old: u64, n: f64) -> Result<u64, String> {
    if !n.is_finite() || n.fract() != 0. || n.abs() >= u64::MAX as f64 {
        return Err("Invalid integral resource amount".into());
    }
    if n < 0. {
        old.checked_sub((-n) as u64)
    } else {
        old.checked_add(n as u64)
    }
    .ok_or_else(|| "Resource overflow/underflow".into())
}
struct Journal<'a> {
    s: &'a GameSession,
    cat: &'a ResearchCatalogue,
    rules: &'a ResearchRules,
    state: Cow<'a, CampaignState>,
    research: BTreeMap<String, CountryResearch>,
    stock: BTreeMap<String, crate::production::CountryProduction>,
    men: BTreeMap<String, u64>,
    economy: BTreeMap<u32, crate::state::StateEconomy>,
    retired: BTreeSet<String>,
    globals: BTreeSet<String>,
    budget: usize,
    completing: BTreeSet<(String, String)>,
    decision_timers: BTreeMap<(String, String), (bool, Option<u64>)>,
}
impl<'a> Journal<'a> {
    fn new(s: &'a GameSession, cat: &'a ResearchCatalogue, rules: &'a ResearchRules) -> Self {
        Self {
            s,
            cat,
            rules,
            state: Cow::Borrowed(&s.campaign),
            research: BTreeMap::new(),
            stock: BTreeMap::new(),
            men: BTreeMap::new(),
            economy: BTreeMap::new(),
            retired: BTreeSet::new(),
            globals: s.research.global_flags.clone(),
            budget: STEPS,
            completing: BTreeSet::new(),
            decision_timers: BTreeMap::new(),
        }
    }
    fn step(&mut self, d: usize) -> Result<(), String> {
        if d > 64 {
            return Err("Script recursion limit".into());
        }
        self.budget = self
            .budget
            .checked_sub(1)
            .ok_or("Script execution budget")?;
        Ok(())
    }
    fn tag(&self, c: &Context) -> Result<String, String> {
        match &c.scope {
            Scope::Country(t)
                if self.s.world.countries.contains_key(t) && !self.state.inactive.contains(t) =>
            {
                Ok(t.clone())
            }
            _ => Err("Existing active country scope required".into()),
        }
    }
    fn research(&mut self, t: &str) -> &mut CountryResearch {
        let c = self.research.entry(t.into()).or_insert_with(|| {
            self.s.research.country(t).cloned().unwrap_or_else(|| {
                let mut c = CountryResearch::new(0, self.rules);
                c.context.tag = t.into();
                c.context.original_tag = t.into();
                c
            })
        });
        c.context.date = Some(self.s.clock.date);
        c.context.global_flags = self.globals.clone();
        c.context.at_war = Some(self.s.military.wars.iter().any(|(a, b)| a == t || b == t));
        c
    }
    fn num(&mut self, a: &str, c: &Context) -> Result<f64, String> {
        if let Ok(n) = a.parse::<f64>() {
            return if n.is_finite() {
                Ok(n)
            } else {
                Err("Non-finite value".into())
            };
        }
        if let Some((kind, id)) = a.split_once('@') {
            if ["days_mission_timeout", "days_decision_timeout"].contains(&kind) {
                let t = self.tag(c)?;
                let timer = self
                    .decision_timers
                    .get(&(t.clone(), id.into()))
                    .copied()
                    .or_else(|| {
                        self.state
                            .decisions
                            .active
                            .get(&t)
                            .and_then(|m| m.get(id))
                            .map(|j| (j.mission, j.deadline))
                    })
                    .ok_or("Decision timer not active")?;
                if timer.0 != (kind == "days_mission_timeout") {
                    return Err("Decision timer type mismatch".into());
                }
                return Ok(timer.1.map_or(-1., |h| {
                    h.saturating_sub(self.s.clock.elapsed_hours) as f64 / 24.
                }));
            }
        }
        let raw = a.strip_prefix("var:").unwrap_or(a);
        let (name, fallback) = match raw.split_once('?') {
            Some((key, n)) => (
                key,
                Some(
                    n.parse::<f64>()
                        .ok()
                        .filter(|n| n.is_finite())
                        .ok_or("Invalid variable fallback")?,
                ),
            ),
            None => (raw, None),
        };
        let (t, key) = if let Some((scope, key)) = name.split_once('.') {
            let Scope::Country(t) = self.scope(scope, c)? else {
                return Err("Numeric variable requires country".into());
            };
            (t, key)
        } else {
            (self.tag(c)?, name)
        };
        self.research(&t)
            .context
            .variables
            .get(key)
            .copied()
            .or(fallback)
            .filter(|n| n.is_finite())
            .ok_or_else(|| format!("Unknown expression {a}"))
    }
    fn scope(&self, k: &str, c: &Context) -> Result<Scope, String> {
        let x = match k {
            "THIS" => Some(c.scope.clone()),
            "ROOT" => Some(c.root.clone()),
            "FROM" => c.from.clone(),
            "PREV" => c.previous.last().cloned(),
            "owner" | "controller" => {
                if let Scope::State(id) = c.scope {
                    if k == "owner" {
                        self.s.world.states.get(&id).and_then(|s| s.owner.clone())
                    } else {
                        self.s.world.state_controller(id).map(str::to_owned)
                    }
                    .map(Scope::Country)
                } else {
                    None
                }
            }
            k if self.s.world.countries.contains_key(k) => Some(Scope::Country(k.into())),
            k => k
                .parse()
                .ok()
                .filter(|i| self.s.world.states.contains_key(i))
                .map(Scope::State),
        };
        x.filter(|s| !matches!(s, Scope::Global))
            .ok_or_else(|| format!("Unknown scope {k}"))
    }
    fn refresh_dynamic(&mut self, t: &str) -> Result<(), String> {
        let active = self
            .state
            .dynamic_modifiers
            .get(t)
            .cloned()
            .unwrap_or_default();
        let ctx = Context::country(t);
        self.research(t)
            .source_modifiers
            .retain(|k, _| !k.starts_with("dynamic:"));
        for (id, job) in active {
            self.step(0)?;
            let cat = self.cat;
            if job.expires.is_some_and(|h| h <= self.s.clock.elapsed_hours) {
                self.state
                    .to_mut()
                    .dynamic_modifiers
                    .get_mut(t)
                    .unwrap()
                    .remove(&id);
                continue;
            }
            let raw = hoi4_mobile::script_validation::dynamic_definition(cat, &id)?;
            if let Some(b) = pdx::block(raw, "remove_trigger") {
                if self.conditions(b, &ctx, 0)? {
                    self.state
                        .to_mut()
                        .dynamic_modifiers
                        .get_mut(t)
                        .unwrap()
                        .remove(&id);
                    continue;
                }
            }
            let enabled = match pdx::block(raw, "enable") {
                Some(b) => self.conditions(b, &ctx, 0)?,
                None => true,
            };
            if enabled {
                let mut values = BTreeMap::new();
                for e in raw
                    .iter()
                    .filter(|e| crate::research::modifier_consumer(&e.key).is_some())
                {
                    let n = self.num(atom(e)?, &ctx)?;
                    let value = values.entry(e.key.clone()).or_insert(0.);
                    *value += n;
                    if !value.is_finite() {
                        return Err("Dynamic modifier overflow".into());
                    }
                }
                self.research(t)
                    .source_modifiers
                    .insert(format!("dynamic:{id}"), values);
            }
            if let Some(job) = self
                .state
                .to_mut()
                .dynamic_modifiers
                .get_mut(t)
                .and_then(|m| m.get_mut(&id))
            {
                job.blocked = None;
            }
        }
        Ok(())
    }
    fn conditions(&mut self, b: &[Entry], c: &Context, d: usize) -> Result<bool, String> {
        let mut ok = true;
        let mut i = 0;
        while let Some(e) = b.get(i) {
            i += 1;
            if e.key.eq_ignore_ascii_case("if") {
                let mut branches = vec![(false, body(e)?)];
                while let Some(e) = b.get(i).filter(|e| {
                    e.key.eq_ignore_ascii_case("else_if") || e.key.eq_ignore_ascii_case("else")
                }) {
                    branches.push((e.key.eq_ignore_ascii_case("else"), body(e)?));
                    i += 1;
                }
                for (otherwise, branch) in branches {
                    if otherwise
                        || self.conditions(
                            pdx::keyword_block(branch, "limit")
                                .ok_or("Trigger if limit missing")?,
                            c,
                            d + 1,
                        )?
                    {
                        let selected: Vec<_> = branch
                            .iter()
                            .filter(|e| !e.key.eq_ignore_ascii_case("limit"))
                            .cloned()
                            .collect();
                        ok &= self.conditions(&selected, c, d + 1)?;
                        break;
                    }
                }
                continue;
            }
            self.step(d)?;
            let v = match e.key.as_str() {
                "AND" | "and" => self.conditions(body(e)?, c, d + 1)?,
                "OR" | "NOT" | "NOR" | "NAND" => {
                    let mut v = Vec::new();
                    for e in body(e)? {
                        v.push(self.conditions(std::slice::from_ref(e), c, d + 1)?)
                    }
                    match e.key.as_str() {
                        "OR" => v.iter().any(|v| *v),
                        "NAND" => !v.iter().all(|v| *v),
                        _ => !v.iter().any(|v| *v),
                    }
                }
                "always" => match atom(e)? {
                    "yes" => true,
                    "no" => false,
                    _ => return Err("Invalid always predicate".into()),
                },
                "hidden_trigger" | "custom_trigger_tooltip" => {
                    let b: Vec<_> = body(e)?
                        .iter()
                        .filter(|e| e.key != "tooltip")
                        .cloned()
                        .collect();
                    self.conditions(&b, c, d + 1)?
                }
                "has_decision" | "has_active_mission" => {
                    let t = self.tag(c)?;
                    let id = atom(e)?;
                    self.state
                        .decisions
                        .active
                        .get(&t)
                        .and_then(|m| m.get(id))
                        .is_some_and(|job| job.mission == (e.key == "has_active_mission"))
                }
                "has_dynamic_modifier" => {
                    let t = self.tag(c)?;
                    let id = if let Some(id) = e.atom() {
                        id
                    } else {
                        fields(e, &["modifier"])?;
                        field(body(e)?, "modifier")?
                    };
                    self.state
                        .dynamic_modifiers
                        .get(&t)
                        .and_then(|m| m.get(id))
                        .is_some_and(|m| m.expires.is_none_or(|h| h > self.s.clock.elapsed_hours))
                }
                "has_focus_tree" => {
                    let t = self.tag(c)?;
                    self.state
                        .focuses
                        .selected
                        .get(&t)
                        .ok_or("Focus tree not initialized")?
                        .as_str()
                        == atom(e)?
                }
                "focus_progress" => {
                    let mut seen = BTreeSet::new();
                    if body(e)?.iter().any(|e| {
                        !["focus", "progress"].contains(&e.key.as_str())
                            || !seen.insert(&e.key)
                            || (e.key == "focus" && e.operator != "=")
                    }) {
                        return Err("Invalid/duplicate focus_progress field".into());
                    }
                    let t = self.tag(c)?;
                    let raw = body(e)?;
                    let id = field(raw, "focus")?;
                    let test = raw
                        .iter()
                        .find(|e| e.key == "progress")
                        .ok_or("Progress comparator missing")?;
                    let progress = if self.research(&t).context.completed_focuses.contains(id) {
                        1.
                    } else {
                        self.state
                            .focuses
                            .active
                            .get(&t)
                            .filter(|a| a.id == id)
                            .map_or(0., |a| a.progress / a.required)
                    };
                    let target = self.num(atom(test)?, c)?;
                    match test.operator.as_str() {
                        ">" => progress > target,
                        "<" => progress < target,
                        ">=" => progress >= target,
                        "<=" => progress <= target,
                        "=" | "==" => progress == target,
                        "!=" => progress != target,
                        _ => return Err("Invalid focus progress comparison".into()),
                    }
                }
                "has_completed_focus" => {
                    let t = self.tag(c)?;
                    self.research(&t)
                        .context
                        .completed_focuses
                        .contains(atom(e)?)
                        || self
                            .state
                            .focuses
                            .bypassed
                            .get(&t)
                            .is_some_and(|b| b.contains(atom(e).unwrap()))
                }
                "has_capitulated" => {
                    let t = self.tag(c)?;
                    let yes = self.s.peace.capitulated.contains(&t);
                    match atom(e)? {
                        "yes" => yes,
                        "no" => !yes,
                        _ => return Err("Invalid has_capitulated".into()),
                    }
                }
                "has_global_flag" => self.globals.contains(atom(e)?),
                "tag" | "original_tag" => {
                    let tag = self.tag(c)?;
                    let raw = atom(e)?;
                    let target = if ["ROOT", "THIS", "FROM", "PREV"].contains(&raw) {
                        match self.scope(raw, c)? {
                            Scope::Country(t) => t,
                            _ => return Err("Country comparison target required".into()),
                        }
                    } else {
                        raw.to_owned()
                    };
                    if e.key == "tag" {
                        tag == target
                    } else {
                        self.research(&tag).context.original_tag == target
                    }
                }
                "has_state_flag" => {
                    let flag = atom(e)?;
                    let Scope::State(id) = c.scope else {
                        return Err("State required".into());
                    };
                    self.state
                        .state_flags
                        .get(&id)
                        .is_some_and(|f| f.contains(flag))
                }
                "political_power"
                | "has_political_power"
                | "stability"
                | "has_stability"
                | "war_support"
                | "has_war_support" => {
                    let t = self.tag(c)?;
                    let p = self
                        .state
                        .countries
                        .get(&t)
                        .ok_or("Unknown political history")?;
                    let n = match e.key.as_str() {
                        "political_power" | "has_political_power" => p.power,
                        "stability" | "has_stability" => p.stability,
                        _ => p.war_support,
                    }
                    .ok_or("Unknown political value")?;
                    let target = self.num(atom(e)?, c)?;
                    match e.operator.as_str() {
                        ">" => n > target,
                        ">=" | "=" => n >= target,
                        "<" => n < target,
                        "<=" => n <= target,
                        "==" => n == target,
                        _ => return Err("Invalid comparison".into()),
                    }
                }
                k if self.cat.scripts.triggers.contains_key(k) => {
                    let raw = self.cat.scripts.triggers[k].clone();
                    if e.atom() == Some("no") {
                        !self.conditions(&raw, c, d + 1)?
                    } else {
                        self.conditions(&instantiate(&raw, e)?, c, d + 1)?
                    }
                }
                k if e.block().is_some() && self.scope(k, c).is_ok() => {
                    self.conditions(body(e)?, &c.enter(self.scope(k, c)?), d + 1)?
                }
                _ => {
                    let t = self.tag(c)?;
                    crate::research_script::evaluate(std::slice::from_ref(e), self.research(&t))
                        .ok_or_else(|| format!("Unknown predicate {}", e.key))?
                }
            };
            ok &= v;
        }
        Ok(ok)
    }
    fn execute(&mut self, b: &[Entry], c: &Context, source: &str, d: usize) -> Result<(), String> {
        let mut i = 0;
        while let Some(e) = b.get(i) {
            self.step(d)?;
            i += 1;
            if e.key == "if" {
                let mut branches = vec![(false, body(e)?)];
                while let Some(e) = b.get(i).filter(|e| e.key == "else" || e.key == "else_if") {
                    branches.push((e.key == "else", body(e)?));
                    i += 1;
                    if e.key == "else" {
                        break;
                    }
                }
                for (other, branch) in branches {
                    if other
                        || self.conditions(
                            pdx::block(branch, "limit").ok_or("if limit missing")?,
                            c,
                            d + 1,
                        )?
                    {
                        let effects: Vec<_> = branch
                            .iter()
                            .filter(|e| e.key != "limit")
                            .cloned()
                            .collect();
                        self.execute(&effects, c, source, d + 1)?;
                        break;
                    }
                }
                continue;
            }
            self.effect(e, c, source, d).map_err(|r| {
                format!("{source}:{}:{} {}: {r}", e.span.line, e.span.column, e.key)
            })?;
        }
        Ok(())
    }
    fn effect(&mut self, e: &Entry, c: &Context, source: &str, d: usize) -> Result<(), String> {
        match e.key.as_str() {
            "activate_mission" => self.activate_mission(c, atom(e)?, d + 1),
            "remove_mission" | "remove_decision" => {
                self.remove_decision(c, atom(e)?, e.key == "remove_mission")
            }
            "add_days_remove" | "add_days_mission_timeout" => self.adjust_decision_timer(c, e),
            "hidden_effect" => self.execute(body(e)?, c, source, d + 1),
            "else" | "else_if" => Err("Orphan branch".into()),
            "random_list" => {
                let b = body(e)?;
                let total = b
                    .iter()
                    .try_fold(0., |v, e| e.key.parse::<f64>().map(|n| v + n))
                    .map_err(|_| "Invalid random weight")?;
                if !total.is_finite() || total <= 0. {
                    return Err("Invalid random total".into());
                }
                let mut n = self.state.to_mut().rng.unit() * total;
                for e in b {
                    n -= e.key.parse::<f64>().unwrap();
                    if n < 0. {
                        return self.execute(body(e)?, c, source, d + 1);
                    }
                }
                Ok(())
            }
            "every_country" | "every_owned_state" | "random_owned_state" => {
                let scopes: Vec<_> = if e.key == "every_country" {
                    self.s
                        .world
                        .countries
                        .keys()
                        .filter(|t| !self.state.inactive.contains(*t))
                        .cloned()
                        .map(Scope::Country)
                        .collect()
                } else {
                    let t = self.tag(c)?;
                    self.s
                        .world
                        .states
                        .values()
                        .filter(|s| s.owner.as_deref() == Some(&t))
                        .map(|s| Scope::State(s.id))
                        .collect()
                };
                let b = body(e)?;
                let mut selected = vec![];
                for s in scopes {
                    self.step(d)?;
                    let ctx = c.enter(s);
                    if self.conditions(pdx::block(b, "limit").unwrap_or(&[]), &ctx, d + 1)? {
                        selected.push(ctx)
                    }
                }
                if e.key == "random_owned_state" && !selected.is_empty() {
                    let n =
                        u32::try_from(selected.len()).map_err(|_| "Too many random candidates")?;
                    let chosen = self.state.to_mut().rng.die(n) as usize - 1;
                    selected = vec![selected.swap_remove(chosen)];
                }
                let effects: Vec<_> = b.iter().filter(|e| e.key != "limit").cloned().collect();
                for ctx in selected {
                    self.execute(&effects, &ctx, source, d + 1)?
                }
                Ok(())
            }
            "mark_focus_tree_layout_dirty" => {
                let t = self.tag(c)?;
                if atom(e)? != "yes" {
                    return Err("Layout invalidation requires yes".into());
                }
                self.state.to_mut().focuses.positions.remove(&t);
                self.state.to_mut().focuses.layouts.remove(&t);
                self.state.to_mut().focuses.layout_trees.remove(&t);
                Ok(())
            }
            "add_dynamic_modifier" => {
                let t = self.tag(c)?;
                let raw = body(e)?;
                let id = field(raw, "modifier")?.to_owned();
                let expires = match pdx::atom(raw, "days") {
                    Some(n) => {
                        let n = self.num(n, c)?;
                        if n <= 0. || n.fract() != 0. || n >= u64::MAX as f64 {
                            return Err("Invalid dynamic modifier duration".into());
                        }
                        Some(
                            self.s
                                .clock
                                .elapsed_hours
                                .checked_add(
                                    (n as u64)
                                        .checked_mul(24)
                                        .ok_or("Dynamic duration overflow")?,
                                )
                                .ok_or("Dynamic expiry overflow")?,
                        )
                    }
                    None => None,
                };
                if self
                    .state
                    .dynamic_modifiers
                    .values()
                    .map(|m| m.len())
                    .sum::<usize>()
                    >= 4096
                    && !self
                        .state
                        .dynamic_modifiers
                        .get(&t)
                        .is_some_and(|m| m.contains_key(&id))
                {
                    return Err("Too many dynamic modifiers".into());
                }
                self.state
                    .to_mut()
                    .dynamic_modifiers
                    .entry(t.clone())
                    .or_default()
                    .insert(
                        id,
                        DynamicModifier {
                            begun: self.s.clock.elapsed_hours,
                            expires,
                            source: source.into(),
                            blocked: None,
                        },
                    );
                self.refresh_dynamic(&t)
            }
            "remove_dynamic_modifier" => {
                let t = self.tag(c)?;
                let id = field(body(e)?, "modifier")?;
                if let Some(m) = self.state.to_mut().dynamic_modifiers.get_mut(&t) {
                    m.remove(id);
                }
                self.research(&t)
                    .source_modifiers
                    .remove(&format!("dynamic:{id}"));
                Ok(())
            }
            "complete_national_focus" => {
                let t = self.tag(c)?;
                let id = e
                    .atom()
                    .or_else(|| e.block().and_then(|b| pdx::atom(b, "focus")))
                    .ok_or("Completed focus ID required")?
                    .to_owned();
                if self.research(&t).context.completed_focuses.contains(&id) {
                    return Ok(());
                }
                if !self.completing.insert((t.clone(), id.clone())) {
                    return Err(format!("Recursive completion of {id}"));
                }
                let src = self
                    .cat
                    .scripts
                    .sources
                    .get(&format!("focus:{id}"))
                    .ok_or("Focus source missing")?;
                self.execute(&src.effects, c, &src.source, d + 1)?;
                self.research(&t)
                    .context
                    .completed_focuses
                    .insert(id.clone());
                if self
                    .state
                    .focuses
                    .active
                    .get(&t)
                    .is_some_and(|f| f.id == id)
                {
                    self.state.to_mut().focuses.active.remove(&t);
                }
                self.state.to_mut().focuses.failures.remove(&t);
                self.completing.remove(&(t, id));
                Ok(())
            }
            "set_politics" => {
                let t = self.tag(c)?;
                let b = body(e)?;
                let party = pdx::atom(b, "ruling_party").map(str::to_owned);
                if party
                    .as_ref()
                    .is_some_and(|p| self.research(&t).context.ideology.as_ref() != Some(p))
                    && ["on_ruling_party_change", "on_ruling_party_change_immediate"]
                        .iter()
                        .any(|key| self.cat.scripts.on_actions.contains_key(*key))
                {
                    return Err("Ruling-party change on-actions require native old_ideology_token/immediate dispatch support".into());
                }
                let elections = match pdx::atom(b, "elections_allowed") {
                    None => None,
                    Some("yes") => Some(true),
                    Some("no") => Some(false),
                    _ => return Err("elections_allowed requires bool".into()),
                };
                let r = self.research(&t);
                if let Some(party) = party {
                    r.context.ideology = Some(party);
                }
                if let Some(elections) = elections {
                    r.context.elections_allowed = Some(elections);
                }
                if pdx::atom(b, "name").is_some() || pdx::atom(b, "long_name").is_some() {
                    let party = r
                        .context
                        .ideology
                        .clone()
                        .ok_or("Ruling ideology unknown")?;
                    let names = r.context.party_names.entry(party).or_default();
                    if let Some(name) = pdx::atom(b, "name") {
                        names.0 = Some(name.into());
                    }
                    if let Some(name) = pdx::atom(b, "long_name") {
                        names.1 = Some(name.into());
                    }
                }
                Ok(())
            }
            "set_popularities" => {
                let t = self.tag(c)?;
                let mut values = body(e)?.to_vec();
                for v in &mut values {
                    v.value = Some(crate::pdx::Value::Atom(self.num(atom(v)?, c)?.to_string()));
                }
                self.research(&t).context.party_popularities =
                    Some(crate::research_script::popularities(&values)?);
                Ok(())
            }
            "load_focus_tree" => {
                let t = self.tag(c)?;
                let (id, keep) = if let Some(id) = e.atom() {
                    (id, false)
                } else {
                    let b = body(e)?;
                    let keep = match pdx::atom(b, "keep_completed") {
                        None | Some("no") => false,
                        Some("yes") => true,
                        _ => return Err("keep_completed requires bool".into()),
                    };
                    (field(b, "tree")?, keep)
                };
                let tree = self
                    .cat
                    .scripts
                    .focus_trees
                    .get(id)
                    .ok_or("Focus tree missing")?;
                if !tree.problems.is_empty() {
                    return Err(tree.problems.join("; "));
                }
                let allowed = tree.focuses.clone();
                let copy = if let Some(raw) =
                    e.block().and_then(|b| pdx::atom(b, "copy_completed_from"))
                {
                    let Scope::Country(from) = self.scope(raw, c)? else {
                        return Err("Copy progress requires country scope".into());
                    };
                    let completed = self
                        .research(&from)
                        .context
                        .completed_focuses
                        .iter()
                        .filter(|id| allowed.contains(*id))
                        .cloned()
                        .collect::<BTreeSet<_>>();
                    let bypassed = self
                        .state
                        .focuses
                        .bypassed
                        .get(&from)
                        .into_iter()
                        .flatten()
                        .filter(|id| allowed.contains(*id))
                        .cloned()
                        .collect::<BTreeSet<_>>();
                    Some((completed, bypassed))
                } else {
                    None
                };
                if !keep {
                    let shared = self.cat.scripts.shared_focuses.clone();
                    self.research(&t)
                        .context
                        .completed_focuses
                        .retain(|f| shared.contains(f));
                    if let Some(b) = self.state.to_mut().focuses.bypassed.get_mut(&t) {
                        b.retain(|f| shared.contains(f));
                    }
                }
                if let Some((completed, bypassed)) = copy {
                    self.research(&t)
                        .context
                        .completed_focuses
                        .extend(completed);
                    self.state
                        .to_mut()
                        .focuses
                        .bypassed
                        .entry(t.clone())
                        .or_default()
                        .extend(bypassed);
                }
                self.state.to_mut().focuses.active.remove(&t);
                self.state
                    .to_mut()
                    .focuses
                    .selected
                    .insert(t.clone(), id.into());
                self.state.to_mut().focuses.positions.remove(&t);
                self.state.to_mut().focuses.layouts.remove(&t);
                self.state.to_mut().focuses.layout_trees.remove(&t);
                Ok(())
            }
            "set_global_flag" | "clr_global_flag" => {
                if e.key == "set_global_flag" {
                    self.globals.insert(atom(e)?.into());
                } else {
                    self.globals.remove(atom(e)?);
                }
                Ok(())
            }
            "set_state_flag" | "clr_state_flag" => {
                let Scope::State(id) = c.scope else {
                    return Err("State required".into());
                };
                let flags = self.state.to_mut().state_flags.entry(id).or_default();
                if e.key == "set_state_flag" {
                    flags.insert(atom(e)?.into());
                } else {
                    flags.remove(atom(e)?);
                }
                Ok(())
            }
            "set_country_flag" | "clr_country_flag" => {
                let t = self.tag(c)?;
                let legacy = self
                    .s
                    .military
                    .countries
                    .get(&t)
                    .map(|a| a.modifiers.flags.clone())
                    .unwrap_or_default();
                let r = self.research(&t);
                if !r.flags_authoritative {
                    r.flags.extend(legacy);
                    r.flags_authoritative = true;
                }
                if e.key == "set_country_flag" {
                    r.flags.insert(atom(e)?.into());
                } else {
                    r.flags.remove(atom(e)?);
                }
                Ok(())
            }
            "set_variable"
            | "add_to_variable"
            | "subtract_from_variable"
            | "multiply_variable"
            | "divide_variable" => {
                let b = body(e)?;
                let v = self.num(field(b, "value")?, c)?;
                let t = self.tag(c)?;
                let id = field(b, "var")?;
                let vars = &mut self.research(&t).context.variables;
                let old = vars.get(id).copied().unwrap_or(0.);
                let n = match e.key.as_str() {
                    "set_variable" => v,
                    "add_to_variable" => old + v,
                    "subtract_from_variable" => old - v,
                    "multiply_variable" => old * v,
                    _ if v != 0. => old / v,
                    _ => return Err("Division by zero".into()),
                };
                if !n.is_finite() {
                    return Err("Variable overflow".into());
                }
                vars.insert(id.into(), n);
                Ok(())
            }
            "add_manpower" => {
                let t = self.tag(c)?;
                let n = self.num(atom(e)?, c)?;
                let old = self
                    .men
                    .get(&t)
                    .copied()
                    .or_else(|| self.s.military.countries.get(&t).map(|a| a.manpower_pool))
                    .ok_or("Missing army")?;
                self.men.insert(t, integral(old, n)?);
                Ok(())
            }
            "add_equipment_to_stockpile" => {
                let t = self.tag(c)?;
                let b = body(e)?;
                let model = field(b, "type")?;
                if !self
                    .s
                    .military
                    .catalogue
                    .equipment
                    .get(model)
                    .is_some_and(|e| e.buildable && !e.modular)
                {
                    return Err("A concrete non-modular model is required".into());
                }
                let n = self.num(field(b, "amount")?, c)?;
                let p = self
                    .stock
                    .entry(t.clone())
                    .or_insert_with(|| self.s.production.country(&t).cloned().unwrap_or_default());
                let v = p.model_stockpile.entry(model.into()).or_default();
                *v = integral(*v, n)?;
                Ok(())
            }
            "add_political_power"
            | "add_stability"
            | "add_war_support"
            | "set_political_power"
            | "set_stability"
            | "set_war_support" => {
                let t = self.tag(c)?;
                let n = self.num(atom(e)?, c)?;
                let p = self
                    .state
                    .to_mut()
                    .countries
                    .get_mut(&t)
                    .ok_or("Political history unknown")?;
                let v = match e.key.as_str() {
                    "add_political_power" | "set_political_power" => &mut p.power,
                    "add_stability" | "set_stability" => &mut p.stability,
                    _ => &mut p.war_support,
                };
                let next = if e.key.starts_with("set_") {
                    n
                } else {
                    v.ok_or("Political value unknown")? + n
                };
                if !next.is_finite() {
                    return Err("Political overflow".into());
                }
                *v = Some(if e.key.ends_with("political_power") {
                    next
                } else {
                    next.clamp(0., 1.)
                });
                Ok(())
            }
            "add_building_construction" => {
                let Scope::State(id) = c.scope else {
                    return Err("State required".into());
                };
                let b = body(e)?;
                let n = self.num(field(b, "level")?, c)?;
                let old = self.s.world.states.get(&id).ok_or("State missing")?.economy;
                let a = self.economy.entry(id).or_insert(old);
                let v = match field(b, "type")? {
                    "arms_factory" => &mut a.military_factories,
                    "industrial_complex" => &mut a.civilian_factories,
                    "dockyard" => &mut a.dockyards,
                    "infrastructure" => &mut a.infrastructure,
                    "fuel_silo" => &mut a.fuel_silos,
                    "synthetic_refinery" => &mut a.synthetic_refineries,
                    _ => return Err("Unsupported building".into()),
                };
                *v = u16::try_from(integral(u64::from(*v), n)?).map_err(|_| "Building overflow")?;
                Ok(())
            }
            "retire_character" => {
                let t = self.tag(c)?;
                let id = atom(e)?;
                if !self
                    .s
                    .military
                    .command
                    .commanders
                    .get(id)
                    .is_some_and(|g| g.country == t)
                {
                    return Err("Military character missing; political roles unsupported".into());
                }
                self.retired.insert(id.into());
                Ok(())
            }
            "country_event" => {
                let b = body(e)?;
                let id = field(b, "id")?;
                let (origin, raw) = event(self.cat, id)?;
                let h = self.num(pdx::atom(b, "hours").unwrap_or("0"), c)?
                    + 24. * self.num(pdx::atom(b, "days").unwrap_or("0"), c)?;
                if h < 0. || h.fract() != 0. || h >= u64::MAX as f64 {
                    return Err("Invalid delay".into());
                }
                let due = self
                    .s
                    .clock
                    .elapsed_hours
                    .checked_add(h as u64)
                    .ok_or("Event time overflow")?;
                let mut ctx = Context::country(&self.tag(c)?);
                ctx.from = Some(c.root.clone());
                self.state.to_mut().schedule(
                    due,
                    ctx,
                    origin,
                    Payload::Event { id: id.into(), raw },
                )?;
                Ok(())
            }
            "add_ideas" | "remove_ideas" | "swap_ideas" | "set_technology" | "add_tech_bonus"
            | "set_research_slots" | "add_research_slot" => {
                if matches!(e.key.as_str(), "add_ideas" | "remove_ideas" | "swap_ideas") {
                    let ids: Vec<_> = if e.key == "swap_ideas" {
                        fields(e, &["add_idea", "remove_idea"])?;
                        body(e)?.iter().map(|e| atom(e)).collect::<Result<_, _>>()?
                    } else if let Some(a) = e.atom() {
                        vec![a]
                    } else {
                        body(e)?.iter().map(|e| e.key.as_str()).collect()
                    };
                    for id in ids {
                        let raw = self.cat.scripts.ideas.get(id).ok_or("Unknown idea")?;
                        for f in raw {
                            if f.key == "modifier" {
                                for v in body(f)? {
                                    if crate::research::modifier_consumer(&v.key).is_none()
                                        || v.atom()
                                            .and_then(|v| v.parse::<f64>().ok())
                                            .is_none_or(|v| !v.is_finite())
                                    {
                                        return Err(format!("Missing modifier consumer {}", v.key));
                                    }
                                }
                            } else if ![
                                "research_bonus",
                                "picture",
                                "name",
                                "desc",
                                "allowed",
                                "visible",
                                "cost",
                                "removal_cost",
                                "allowed_civil_war",
                            ]
                            .contains(&f.key.as_str())
                            {
                                return Err(format!("Unsupported idea semantics {}", f.key));
                            }
                        }
                    }
                }
                let t = self.tag(c)?;
                let (cat, rules, date) = (self.cat, self.rules, self.s.clock.date);
                let r = self.research(&t);
                let pending = std::mem::take(&mut r.pending_effects);
                crate::research_script::apply_effects(
                    r,
                    std::slice::from_ref(e),
                    cat,
                    rules,
                    date,
                    source,
                );
                if !r.pending_effects.is_empty() {
                    return Err(r.pending_effects[0].reason.clone());
                }
                r.pending_effects = pending;
                if matches!(e.key.as_str(), "add_ideas" | "remove_ideas" | "swap_ideas") {
                    r.ideas_authoritative = true;
                }
                Ok(())
            }
            k if self.cat.scripts.effects.contains_key(k) => {
                let b = instantiate(&self.cat.scripts.effects[k], e)?;
                preflight(&b, self.cat)?;
                self.execute(&b, c, source, d + 1)
            }
            k => self.execute(body(e)?, &c.enter(self.scope(k, c)?), source, d + 1),
        }
    }
}
impl GameSession {
    pub fn campaign_effects(
        &mut self,
        ctx: &Context,
        b: &[Entry],
        source: &str,
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
    ) -> Result<(), String> {
        preflight(b, cat)?;
        self.campaign_transaction(cat, rules, |j| j.execute(b, ctx, source, 0))
    }
    fn campaign_transaction<T>(
        &mut self,
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
        f: impl FnOnce(&mut Journal<'_>) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut j = Journal::new(self, cat, rules);
        let result = f(&mut j)?;
        let Journal {
            state,
            research,
            stock,
            men,
            economy,
            retired,
            globals,
            ..
        } = j;
        if let Cow::Owned(state) = state {
            self.campaign = state;
        }
        self.research.global_flags = globals;
        for (t, mut r) in research {
            r.revision = r.revision.saturating_add(1);
            self.research.countries.insert(t, r);
        }
        for r in self.research.countries.values_mut() {
            r.context.global_flags = self.research.global_flags.clone();
        }
        for (t, p) in stock {
            self.production.by_country.insert(t, p);
        }
        for (t, n) in men {
            self.military.countries.get_mut(&t).unwrap().manpower_pool = n;
        }
        for (id, a) in economy {
            self.world.states.get_mut(&id).unwrap().economy = a;
        }
        for id in retired {
            self.military.command.commanders.remove(&id);
            for a in self.military.command.armies.values_mut() {
                if a.general.as_deref() == Some(&id) {
                    a.general = None;
                }
            }
            for g in self.military.command.groups.values_mut() {
                if g.field_marshal.as_deref() == Some(&id) {
                    g.field_marshal = None;
                }
            }
            self.military.abilities.active.retain(|a| a.leader != id);
            self.military.abilities.cooldowns.remove(&id);
            self.military.command.revision = self.military.command.revision.saturating_add(1);
        }
        self.military.refresh_modifiers(&self.research);
        let bonuses: Vec<_> = self
            .military
            .divisions
            .keys()
            .map(|id| {
                let mut b = self.military.command.bonuses(*id);
                b.add(self.military.hq_bonus(*id), 1.);
                (*id, b, self.military.unit_ability_modifiers(*id))
            })
            .collect();
        for (id, b, a) in bonuses {
            let d = self.military.divisions.get_mut(&id).unwrap();
            d.command = b;
            d.ability_modifiers = a;
        }
        self.refresh_research_building_slots();
        self.campaign.decisions.revision = self.campaign.decisions.revision.saturating_add(1);
        Ok(result)
    }
    pub fn campaign_conditions(
        &self,
        c: &Context,
        b: &[Entry],
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
    ) -> Result<bool, String> {
        Journal::new(self, cat, rules).conditions(b, c, 0)
    }
    /// Replay only independent dynamic history effects that the research-only
    /// importer deferred. Conditional/unknown wrappers are NOT flattened.
    pub fn initialize_campaign_dynamic_history(
        &mut self,
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
    ) {
        let jobs: Vec<_> = self
            .research
            .countries
            .iter()
            .flat_map(|(tag, c)| {
                c.pending_effects
                    .iter()
                    .filter(|e| {
                        e.payload.len() == 1
                            && matches!(
                                e.payload[0].key.as_str(),
                                "add_dynamic_modifier" | "remove_dynamic_modifier"
                            )
                    })
                    .map(|e| (tag.clone(), e.clone()))
            })
            .collect();
        for (tag, job) in jobs {
            match self.campaign_effects(
                &Context::country(&tag),
                &job.payload,
                &job.source,
                cat,
                rules,
            ) {
                Ok(()) => {
                    if let Some(c) = self.research.countries.get_mut(&tag) {
                        c.pending_effects
                            .retain(|e| e.source != job.source || e.payload != job.payload);
                    }
                }
                Err(e) => {
                    self.campaign.politics.failures.insert(tag, e);
                }
            }
        }
    }
    fn expire_dynamic_modifiers(&mut self) {
        let expired: Vec<_> = self
            .campaign
            .dynamic_modifiers
            .iter()
            .flat_map(|(tag, jobs)| {
                jobs.iter()
                    .filter(|(_, m)| m.expires.is_some_and(|h| h <= self.clock.elapsed_hours))
                    .map(|(id, _)| (tag.clone(), id.clone()))
            })
            .collect();
        if expired.is_empty() {
            return;
        }
        for (tag, id) in expired {
            self.campaign
                .dynamic_modifiers
                .get_mut(&tag)
                .unwrap()
                .remove(&id);
            if let Some(c) = self.research.countries.get_mut(&tag) {
                c.source_modifiers.remove(&format!("dynamic:{id}"));
                c.revision = c.revision.saturating_add(1);
            }
        }
        self.military.refresh_modifiers(&self.research);
        self.refresh_research_building_slots();
    }
    pub fn tick_dynamic_modifiers(&mut self, cat: &ResearchCatalogue, rules: &ResearchRules) {
        let tags: Vec<_> = self
            .campaign
            .dynamic_modifiers
            .keys()
            .filter(|t| !self.campaign.inactive.contains(*t))
            .cloned()
            .collect();
        for t in tags {
            let mut j = Journal::new(self, cat, rules);
            match j.refresh_dynamic(&t) {
                Ok(()) => {
                    let Journal {
                        state, research, ..
                    } = j;
                    self.campaign = state.into_owned();
                    for (tag, mut c) in research {
                        c.revision = c.revision.saturating_add(1);
                        self.research.countries.insert(tag, c);
                    }
                }
                Err(e) => {
                    if let Some(c) = self.research.countries.get_mut(&t) {
                        c.source_modifiers.retain(|k, _| !k.starts_with("dynamic:"));
                        c.revision = c.revision.saturating_add(1);
                    }
                    if let Some(items) = self.campaign.dynamic_modifiers.get_mut(&t) {
                        for job in items.values_mut() {
                            job.blocked = Some(e.clone());
                        }
                    }
                }
            }
        }
        self.military.refresh_modifiers(&self.research);
        self.refresh_research_building_slots();
    }
    pub fn tick_campaign(&mut self, cat: &ResearchCatalogue, rules: &ResearchRules) {
        let h = self.clock.elapsed_hours;
        if self.campaign.last_hour.is_some_and(|v| v >= h) {
            return;
        }
        self.campaign.last_hour = Some(h);
        self.expire_dynamic_modifiers();
        self.tick_on_actions(cat);
        for _ in 0..64 {
            let next = self
                .campaign
                .queue
                .values()
                .filter(|a| a.due <= h && a.blocked.is_none())
                .min_by_key(|a| (a.due, a.id))
                .cloned();
            let Some(a) = next else { break };
            let r = match &a.payload {
                Payload::Effects(b) => self.campaign_effects(&a.context, b, &a.source, cat, rules),
                Payload::Event { id, raw } => self.fire_campaign_event(&a, id, raw, cat, rules),
                Payload::CivilWar(p) => self.start_civil_war(p, rules),
                Payload::OnAction { raw, .. } => {
                    self.run_on_action(&a.context, raw, &a.source, cat, rules)
                }
            };
            match r {
                Ok(()) => {
                    self.campaign.queue.remove(&a.id);
                }
                Err(e) => {
                    if let Some(a) = self.campaign.queue.get_mut(&a.id) {
                        a.blocked = Some(e);
                    }
                }
            }
        }
        if !self.campaign.queue.values().any(|a| {
            a.blocked.is_none()
                && matches!(&a.payload, Payload::OnAction {hook, ..} if hook == "on_startup")
        }) {
            self.tick_decisions(cat, rules);
        }
    }
    fn fire_campaign_event(
        &mut self,
        a: &Scheduled,
        id: &str,
        raw: &[Entry],
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
    ) -> Result<(), String> {
        check_event(raw)?;
        let Scope::Country(tag) = &a.context.scope else {
            return Err("Country event scope required".into());
        };
        if pdx::atom(raw, "fire_only_once") == Some("yes")
            && self.campaign.once.get(tag).is_some_and(|s| s.contains(id))
        {
            return Ok(());
        }
        if !self.campaign_conditions(
            &a.context,
            pdx::block(raw, "trigger").unwrap_or(&[]),
            cat,
            rules,
        )? {
            return Ok(());
        }
        let opts: Vec<_> = raw
            .iter()
            .filter(|e| e.key == "option")
            .map(|e| body(e).map(<[Entry]>::to_vec))
            .collect::<Result<_, _>>()?;
        for o in &opts {
            let b: Vec<_> = o
                .iter()
                .filter(|e| {
                    !["name", "trigger", "available", "ai_chance"].contains(&e.key.as_str())
                })
                .cloned()
                .collect();
            preflight(&b, cat)?;
        }
        if opts.is_empty() && pdx::atom(raw, "hidden") != Some("yes") {
            return Err("Visible event has no options".into());
        }
        self.campaign_effects(
            &a.context,
            pdx::block(raw, "immediate").unwrap_or(&[]),
            &a.source,
            cat,
            rules,
        )?;
        self.campaign
            .once
            .entry(tag.clone())
            .or_default()
            .insert(id.into());
        if !opts.is_empty() {
            self.campaign.pending.insert(
                a.id,
                PendingEvent {
                    id: a.id,
                    event: id.into(),
                    context: a.context.clone(),
                    source: a.source.clone(),
                    options: opts,
                },
            );
        }
        Ok(())
    }
    pub fn choose_campaign_event(
        &mut self,
        id: u64,
        option: usize,
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
    ) -> Result<(), String> {
        let a = self
            .campaign
            .pending
            .get(&id)
            .cloned()
            .ok_or("Pending event missing")?;
        let b = a.options.get(option).ok_or("Option missing")?;
        for key in ["trigger", "available"] {
            if !self.campaign_conditions(
                &a.context,
                pdx::block(b, key).unwrap_or(&[]),
                cat,
                rules,
            )? {
                return Err("Unavailable event option".into());
            }
        }
        let effects: Vec<_> = b
            .iter()
            .filter(|e| !["name", "trigger", "available", "ai_chance"].contains(&e.key.as_str()))
            .cloned()
            .collect();
        self.campaign_effects(&a.context, &effects, &a.source, cat, rules)?;
        self.campaign.pending.remove(&id);
        Ok(())
    }
}
