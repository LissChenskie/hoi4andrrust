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
const STEPS: usize = 10000;
const QUEUE: usize = 4096;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
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
    Event { id: String, raw: Vec<Entry> },
    CivilWar(crate::civil_war::SplitPlan),
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
#[serde(default)]
pub struct CampaignState {
    #[serde(default)]
    pub focuses: crate::focus::FocusCampaign,
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
            focuses: Default::default(),
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
        let scope = |s: &Scope| match s {
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
        let t = self.tag(c)?;
        self.research(&t)
            .context
            .variables
            .get(a.strip_prefix("var:").unwrap_or(a))
            .copied()
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
        x.ok_or_else(|| format!("Unknown scope {k}"))
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
                "political_power" | "stability" | "war_support" => {
                    let t = self.tag(c)?;
                    let p = self
                        .state
                        .countries
                        .get(&t)
                        .ok_or("Unknown political history")?;
                    let n = match e.key.as_str() {
                        "political_power" => p.power,
                        "stability" => p.stability,
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
            "add_political_power" | "add_stability" | "add_war_support" => {
                let t = self.tag(c)?;
                let n = self.num(atom(e)?, c)?;
                let p = self
                    .state
                    .to_mut()
                    .countries
                    .get_mut(&t)
                    .ok_or("Political history unknown")?;
                let v = match e.key.as_str() {
                    "add_political_power" => &mut p.power,
                    "add_stability" => &mut p.stability,
                    _ => &mut p.war_support,
                };
                let next = v.ok_or("Political value unknown")? + n;
                if !next.is_finite() {
                    return Err("Political overflow".into());
                }
                *v = Some(if e.key == "add_political_power" {
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
        let mut j = Journal::new(self, cat, rules);
        j.execute(b, ctx, source, 0)?;
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
        self.campaign = state.into_owned();
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
        Ok(())
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
    pub fn tick_campaign(&mut self, cat: &ResearchCatalogue, rules: &ResearchRules) {
        let h = self.clock.elapsed_hours;
        if self.campaign.last_hour.is_some_and(|v| v >= h) {
            return;
        }
        self.campaign.last_hour = Some(h);
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
