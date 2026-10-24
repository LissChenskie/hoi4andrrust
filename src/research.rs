//! Data-driven HOI4-inspired research simulation (target baseline 1.19.x).
//! Version-dependent constants live in ResearchRules, not in research nodes.
//! Exact engine parity requires matching a specific installed HOI4 build.
use crate::game::GameDate;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub enum Requirement {
    Always(bool),
    HasCountryFlag(String),
    HasTechnology(String),
    And(Vec<Requirement>),
    Or(Vec<Requirement>),
    Not(Box<Requirement>),
    /// An unknown Clausewitz trigger must not silently become "true".
    Unsupported(String),
}
impl Requirement {
    pub fn allows(&self, country: &CountryResearch) -> bool {
        self.evaluate(country) == Some(true)
    }
    fn evaluate(&self, country: &CountryResearch) -> Option<bool> {
        match self {
            Self::Always(b) => Some(*b),
            Self::HasCountryFlag(v) => Some(country.flags.contains(v)),
            Self::HasTechnology(v) => Some(country.researched.contains(v)),
            Self::And(v) => {
                let results: Vec<_> = v.iter().map(|x| x.evaluate(country)).collect();
                if results.contains(&Some(false)) {Some(false)} else if results.contains(&None) {None} else {Some(true)}
            }
            Self::Or(v) => {
                let results: Vec<_> = v.iter().map(|x| x.evaluate(country)).collect();
                if results.contains(&Some(true)) {Some(true)} else if results.contains(&None) {None} else {Some(false)}
            }
            Self::Not(x) => x.evaluate(country).map(|v| !v),
            Self::Unsupported(_) => None,
        }
    }
}

#[cfg(test)]
mod condition_regressions {
    use super::*;
    #[test]
    fn negating_unknown_conditions_never_unlocks_a_technology() {
        let country=CountryResearch::new(3,&ResearchRules::default());
        let unknown=Requirement::Unsupported("unimplemented predicate".into());
        assert!(!Requirement::Not(Box::new(unknown.clone())).allows(&country));
        assert!(!Requirement::Not(Box::new(Requirement::And(vec![unknown.clone(),Requirement::Always(true)]))).allows(&country));
        assert!(Requirement::Or(vec![unknown,Requirement::Always(true)]).allows(&country));
    }
}

#[derive(Debug, Clone)]
pub struct Technology {
    pub id: String,
    pub name: String,
    pub description: String,
    pub folder: String,
    pub x: f32,
    pub y: f32,
    pub start_year: u32,
    pub research_cost: f64,
    pub categories: BTreeSet<String>,
    /// Every parent is required. Includes incoming path edges and dependencies.
    pub prerequisites: BTreeSet<String>,
    pub xor: BTreeSet<String>,
    pub allow: Requirement,
    pub allow_branch: Requirement,
    pub ai_weight: f64,
    /// Script payloads that can be interpreted by later game systems.
    pub unlocks: BTreeSet<String>,
    pub modifiers: BTreeMap<String, f64>,
}

impl Technology {
    pub fn sample(id: &str, year: u32, cost: f64) -> Self {
        Self {
            id: id.to_owned(), name: id.replace('_', " "), description: String::new(),
            folder: "industry".into(),
            x: 0.0, y: 0.0, start_year: year, research_cost: cost,
            categories: BTreeSet::new(), prerequisites: BTreeSet::new(),
            xor: BTreeSet::new(), allow: Requirement::Always(true),
            allow_branch: Requirement::Always(true),
            ai_weight: 1.0, unlocks: BTreeSet::new(), modifiers: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResearchCatalogue {
    pub technologies: BTreeMap<String, Technology>,
    pub folders: BTreeSet<String>,
}

impl ResearchCatalogue {
    pub fn insert(&mut self, tech: Technology) -> Result<(), String> {
        if tech.id.is_empty() || !tech.research_cost.is_finite() || tech.research_cost <= 0.0 {
            return Err(format!("Invalid research cost or ID for {}", tech.id));
        }
        if !tech.ai_weight.is_finite() || tech.ai_weight < 0.0 {
            return Err(format!("Invalid AI weight for {}", tech.id));
        }
        if self.technologies.contains_key(&tech.id) {
            return Err(format!("Duplicate technology {}", tech.id));
        }
        self.folders.insert(tech.folder.clone());
        self.technologies.insert(tech.id.clone(), tech);
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        for tech in self.technologies.values() {
            for linked in tech.prerequisites.iter().chain(&tech.xor) {
                if !self.technologies.contains_key(linked) {
                    return Err(format!("{} references missing technology {}", tech.id, linked));
                }
                if linked == &tech.id {
                    return Err(format!("{} references itself", tech.id));
                }
            }
        }
        fn visit(id: &str, tree: &ResearchCatalogue, grey: &mut HashSet<String>,
            black: &mut HashSet<String>) -> Result<(), String> {
            if black.contains(id) { return Ok(()); }
            if !grey.insert(id.to_owned()) {
                return Err(format!("Technology dependency cycle at {id}"));
            }
            for parent in &tree.technologies[id].prerequisites {
                visit(parent, tree, grey, black)?;
            }
            grey.remove(id);
            black.insert(id.to_owned());
            Ok(())
        }
        let mut grey = HashSet::new();
        let mut black = HashSet::new();
        for id in self.technologies.keys() { visit(id, self, &mut grey, &mut black)?; }
        Ok(())
    }
    /// Branch restrictions of ancestors apply to all descendants.
    pub fn visible(&self, id: &str, country: &CountryResearch) -> bool {
        let Some(tech) = self.technologies.get(id) else { return false; };
        if !tech.allow.allows(country) || !tech.allow_branch.allows(country) { return false; }
        let mut todo: Vec<&str> = tech.prerequisites.iter().map(String::as_str).collect();
        let mut seen = HashSet::new();
        while let Some(parent) = todo.pop() {
            if !seen.insert(parent) { continue; }
            let Some(parent_tech) = self.technologies.get(parent) else { return false; };
            if !parent_tech.allow_branch.allows(country) { return false; }
            todo.extend(parent_tech.prerequisites.iter().map(String::as_str));
        }
        true
    }
    pub fn can_research(&self, id: &str, country: &CountryResearch) -> Result<(), &'static str> {
        let Some(tech) = self.technologies.get(id) else { return Err("Unknown technology"); };
        if country.researched.contains(id) { return Err("Already researched"); }
        if country.slots.iter().any(|slot| slot.active.as_ref().is_some_and(|a| a.tech_id == id)) {
            return Err("Already being researched");
        }
        if !self.visible(id, country) { return Err("Technology is unavailable"); }
        if !tech.prerequisites.iter().all(|id| country.researched.contains(id)) {
            return Err("Prerequisites not researched");
        }
        if tech.xor.iter().any(|id| country.researched.contains(id)
            || country.slots.iter().any(|slot| slot.active.as_ref().is_some_and(|x| &x.tech_id == id))) {
            return Err("Mutually exclusive technology selected");
        }
        // XOR may be declared in only one direction in source data.
        if self.technologies.values().any(|other| other.xor.contains(id)
            && (country.researched.contains(&other.id)
                || country.slots.iter().any(|slot| slot.active.as_ref().is_some_and(|x| x.tech_id == other.id)))) {
            return Err("Mutually exclusive technology selected");
        }
        Ok(())
    }
}

/// Baseline matches the user's 1.19.x research brief, but the exact target
/// values should be loaded from that game's own defines before claiming parity.
#[derive(Debug, Clone, Copy)]
pub struct ResearchRules {
    pub base_tech_cost: f64,
    pub year_ahead_penalty: f64,
    pub max_saved_days: f64,
    pub minimum_speed: f64,
    pub max_slots: usize,
}
impl Default for ResearchRules {
    fn default() -> Self {
        Self { base_tech_cost: 110.0, year_ahead_penalty: 2.0,
            max_saved_days: 30.0, minimum_speed: 0.10, max_slots: 12 }
    }
}

#[derive(Debug, Clone)]
pub struct ResearchBonus {
    pub name: String,
    pub category: Option<String>,
    pub technology: Option<String>,
    pub speed: f64,
    pub ahead_reduction_years: f64,
    pub uses: u32,
}
impl ResearchBonus {
    pub fn matches(&self, tech: &Technology) -> bool {
        self.uses > 0 && self.technology.as_ref().is_none_or(|id| id == &tech.id)
            && self.category.as_ref().is_none_or(|cat| tech.categories.contains(cat))
    }
}

#[derive(Debug, Clone)]
pub struct ActiveResearch {
    pub tech_id: String,
    pub progress: f64,
    pub locked_bonus: f64,
    pub ahead_reduction: f64,
}
#[derive(Debug, Clone, Default)]
pub struct ResearchSlot {
    pub active: Option<ActiveResearch>,
    pub saved_days: f64,
}

#[derive(Debug, Clone)]
pub struct CountryResearch {
    pub slots: Vec<ResearchSlot>,
    pub researched: BTreeSet<String>,
    pub flags: BTreeSet<String>,
    pub speed_bonus: f64,
    pub category_bonuses: BTreeMap<String, f64>,
    pub sharing_bonuses: BTreeMap<String, f64>,
    pub available_bonuses: Vec<ResearchBonus>,
    pub completed: Vec<String>,
    pub unlocks: BTreeSet<String>,
    pub applied_modifiers: BTreeMap<String, f64>,
}
impl CountryResearch {
    pub fn new(slot_count: usize, rules: &ResearchRules) -> Self {
        Self {
            slots: vec![ResearchSlot::default(); slot_count.min(rules.max_slots)],
            researched: BTreeSet::new(), flags: BTreeSet::new(),
            speed_bonus: 0.0, category_bonuses: BTreeMap::new(),
            sharing_bonuses: BTreeMap::new(), available_bonuses: Vec::new(),
            completed: Vec::new(), unlocks: BTreeSet::new(),
            applied_modifiers: BTreeMap::new(),
        }
    }
    pub fn resize_slots(&mut self, count: usize, rules: &ResearchRules) -> Result<(), &'static str> {
        if count > rules.max_slots { return Err("Research slot cap exceeded"); }
        if count < self.slots.len() && self.slots[count..].iter().any(|s| s.active.is_some()) {
            return Err("Cannot remove a busy research slot");
        }
        self.slots.resize_with(count, ResearchSlot::default);
        Ok(())
    }
    pub fn start(&mut self, slot: usize, id: &str,
        catalogue: &ResearchCatalogue, rules: &ResearchRules, date: GameDate
    ) -> Result<(), &'static str> {
        if slot >= self.slots.len() { return Err("Research slot is unavailable"); }
        if self.slots[slot].active.is_some() { return Err("Research slot is occupied"); }
        catalogue.can_research(id, self)?;
        let tech = &catalogue.technologies[id];
        let mut speed = 0.0;
        let mut ahead = 0.0;
        if let Some(bonus) = self.available_bonuses.iter_mut().find(|b| b.matches(tech)) {
            speed = bonus.speed;
            ahead = bonus.ahead_reduction_years;
            bonus.uses -= 1;
        }
        let idle_days = std::mem::take(&mut self.slots[slot].saved_days)
            .min(rules.max_saved_days);
        let mut active = ActiveResearch {
            tech_id: id.to_owned(), progress: 0.0,
            locked_bonus: speed, ahead_reduction: ahead,
        };
        // Saved days inherit the chosen technology's effective speed and AOT.
        active.progress = idle_days * daily_speed(tech, self, &active, rules, date);
        self.slots[slot].active = Some(active);
        Ok(())
    }
    /// Cancel an ongoing research without refunding consumed focus bonuses.
    pub fn cancel(&mut self, slot: usize) -> bool {
        self.slots.get_mut(slot).is_some_and(|s| s.active.take().is_some())
    }
    /// Direct award by scripted focus/event (works even for focus-only tech).
    pub fn grant(&mut self, id: &str, catalogue: &ResearchCatalogue) -> bool {
        if !catalogue.technologies.contains_key(id) || !self.researched.insert(id.to_owned()) {
            return false;
        }
        for slot in &mut self.slots {
            if slot.active.as_ref().is_some_and(|a| a.tech_id == id) {
                slot.active = None;
            }
        }
        self.complete_effects(id, catalogue);
        true
    }
    fn complete_effects(&mut self, id: &str, catalogue: &ResearchCatalogue) {
        self.completed.push(id.to_owned());
        let tech = &catalogue.technologies[id];
        self.unlocks.extend(tech.unlocks.iter().cloned());
        for (key, value) in &tech.modifiers {
            *self.applied_modifiers.entry(key.clone()).or_default() += *value;
            // Existing national modifiers and research bonuses are additive;
            // a newly completed technology must not overwrite them.
            if key == "research_speed_factor" {
                self.speed_bonus += *value;
            }
        }
    }
    pub fn tick(&mut self, catalogue: &ResearchCatalogue, rules: &ResearchRules, date: GameDate) {
        let day_work: Vec<Option<f64>> = self.slots.iter().map(|slot| {
            slot.active.as_ref().and_then(|active| catalogue.technologies.get(&active.tech_id)
                .map(|tech| daily_speed(tech, self, active, rules, date)))
        }).collect();
        let mut completed = Vec::new();
        for (slot, speed) in self.slots.iter_mut().zip(day_work) {
            if let (Some(active), Some(speed)) = (slot.active.as_mut(), speed) {
                active.progress += speed;
                if let Some(tech) = catalogue.technologies.get(&active.tech_id) {
                    if active.progress + 1.0e-8 >= tech.research_cost * rules.base_tech_cost {
                        completed.push(active.tech_id.clone());
                        slot.active = None;
                    }
                }
            } else if slot.active.is_none() {
                slot.saved_days = (slot.saved_days + 1.0).min(rules.max_saved_days);
            }
        }
        for tech in completed {
            self.grant(&tech, catalogue);
        }
    }
    /// Estimate by replaying the exact day-step formula and advancing calendar.
    pub fn estimated_days(&self, slot: usize, catalogue: &ResearchCatalogue,
        rules: &ResearchRules, date: GameDate) -> Option<u32> {
        let active = self.slots.get(slot)?.active.as_ref()?;
        let tech = catalogue.technologies.get(&active.tech_id)?;
        let mut progress = active.progress;
        let target = tech.research_cost * rules.base_tech_cost;
        let mut day = date;
        for days in 1..=15_000u32 {
            progress += daily_speed(tech, self, active, rules, day);
            if progress + 1.0e-8 >= target { return Some(days); }
            day.advance_one_day();
        }
        None
    }
}

pub fn years_ahead(tech: &Technology, date: GameDate) -> f64 {
    if date.year >= tech.start_year { return 0.0; }
    let current_year_days = if date.year % 400 == 0
        || (date.year % 4 == 0 && date.year % 100 != 0) { 366.0 } else { 365.0 };
    let days_elapsed = match date.month {
        1 => 0, 2 => 31, 3 => 59, 4 => 90, 5 => 120, 6 => 151, 7 => 181,
        8 => 212, 9 => 243, 10 => 273, 11 => 304, _ => 334,
    } + u32::from(date.day.saturating_sub(1))
        + if current_year_days == 366.0 && date.month > 2 { 1 } else { 0 };
    tech.start_year.saturating_sub(date.year) as f64
        - days_elapsed as f64 / current_year_days
}

pub fn daily_speed(tech: &Technology, country: &CountryResearch,
    active: &ActiveResearch, rules: &ResearchRules, date: GameDate) -> f64 {
    let categories = tech.categories.iter()
        .map(|cat| country.category_bonuses.get(cat).copied().unwrap_or(0.0))
        .sum::<f64>();
    let sharing = country.sharing_bonuses.get(&tech.id).copied().unwrap_or(0.0);
    let speed = (1.0 + country.speed_bonus + categories + sharing + active.locked_bonus)
        .max(rules.minimum_speed);
    let ahead = (years_ahead(tech, date) - active.ahead_reduction).max(0.0);
    speed / (1.0 + rules.year_ahead_penalty * ahead)
}

#[derive(Debug, Default)]
pub struct ResearchWorld {
    pub countries: BTreeMap<String, CountryResearch>,
    pub day_index: u64,
}
impl ResearchWorld {
    pub fn country(&self, tag: &str) -> Option<&CountryResearch> { self.countries.get(tag) }
    pub fn country_mut(&mut self, tag: &str, slots: usize,
        rules: &ResearchRules) -> &mut CountryResearch {
        self.countries.entry(tag.to_owned())
            .or_insert_with(|| CountryResearch::new(slots, rules))
    }
    pub fn tick_country(&mut self, tag: &str, catalogue: &ResearchCatalogue,
        rules: &ResearchRules, date: GameDate) {
        if let Some(c) = self.countries.get_mut(tag) { c.tick(catalogue, rules, date); }
    }
    /// Deterministic hands-off AI, checked weekly. More complex strategic AI
    /// conditions remain out of scope until imported script conditions exist.
    pub fn ai_choose(&mut self, tag: &str, catalogue: &ResearchCatalogue,
        rules: &ResearchRules, date: GameDate) {
        let Some(country) = self.countries.get_mut(tag) else { return; };
        for index in 0..country.slots.len() {
            if country.slots[index].active.is_some() { continue; }
            let choice = catalogue.technologies.values()
                .filter(|tech| tech.ai_weight > 0.0 && catalogue.can_research(&tech.id, country).is_ok())
                .map(|tech| {
                    let ahead = (years_ahead(tech, date) - 0.0).max(0.0);
                    (tech.ai_weight / (1.0 + ahead * 4.0), tech.id.clone())
                })
                .max_by(|a, b| a.0.total_cmp(&b.0).then_with(|| b.1.cmp(&a.1)))
                .map(|(_, id)| id);
            if let Some(id) = choice {
                let _ = country.start(index, &id, catalogue, rules, date);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tree() -> ResearchCatalogue {
        let mut tree = ResearchCatalogue::default();
        let mut a = Technology::sample("a", 1936, 1.0);
        a.categories.insert("industry".into());
        let mut b = Technology::sample("b", 1937, 1.0);
        b.prerequisites.insert("a".into());
        b.xor.insert("c".into());
        let mut c = Technology::sample("c", 1937, 1.0);
        c.prerequisites.insert("a".into());
        let mut focus = Technology::sample("focus_only", 1936, 1.0);
        focus.allow = Requirement::Always(false);
        for t in [a,b,c,focus] { tree.insert(t).unwrap(); }
        tree.validate().unwrap();
        tree
    }
    #[test]
    fn timing_and_parallel_research() {
        let tree = tree(); let rules=ResearchRules::default();
        let mut c = CountryResearch::new(2,&rules);
        let mut day=GameDate::START;
        c.start(0,"a",&tree,&rules,day).unwrap();
        assert_eq!(c.estimated_days(0,&tree,&rules,day),Some(110));
        for _ in 0..110 { c.tick(&tree,&rules,day); day.advance_one_day(); }
        assert!(c.researched.contains("a"));
        assert!(c.slots[1].saved_days == 30.0);
        assert_eq!(c.slots[0].active.is_none(),true);
    }
    #[test]
    fn bonus_saved_days_and_limited_use_apply_at_start() {
        let tree=tree();let rules=ResearchRules::default();
        let mut c=CountryResearch::new(1,&rules);
        c.slots[0].saved_days=30.0;
        c.available_bonuses.push(ResearchBonus {
            name:"focus".into(),category:Some("industry".into()),technology:None,
            speed:0.5,ahead_reduction_years:0.0,uses:1,
        });
        c.start(0,"a",&tree,&rules,GameDate::START).unwrap();
        assert_eq!(c.slots[0].active.as_ref().unwrap().progress,45.0);
        assert_eq!(c.available_bonuses[0].uses,0);
        assert!(c.slots[0].saved_days==0.0);
    }
    #[test]
    fn technology_dependencies_xor_and_focus_gates() {
        let tree=tree();let rules=ResearchRules::default();
        let mut c=CountryResearch::new(2,&rules);
        assert!(c.start(0,"b",&tree,&rules,GameDate::START).is_err());
        assert!(c.start(0,"focus_only",&tree,&rules,GameDate::START).is_err());
        assert!(c.grant("focus_only",&tree));
        assert!(c.grant("a",&tree));
        c.start(0,"b",&tree,&rules,GameDate::START).unwrap();
        assert!(c.start(1,"c",&tree,&rules,GameDate::START).is_err());
    }
    #[test]
    fn ahead_penalty_drops_over_calendar_and_ahead_reduction_works() {
        let tech=Technology::sample("t",1938,1.0);
        let rules=ResearchRules::default();
        let c=CountryResearch::new(1,&rules);
        let mut active=ActiveResearch {tech_id:"t".into(),progress:0.0,locked_bonus:0.0,ahead_reduction:0.0};
        let jan=GameDate::START;
        let dec=GameDate {year:1937,month:12,day:31};
        assert!(daily_speed(&tech,&c,&active,&rules,dec)>daily_speed(&tech,&c,&active,&rules,jan));
        active.ahead_reduction=1.0;
        assert!(daily_speed(&tech,&c,&active,&rules,jan)>0.3);
        assert_eq!(daily_speed(&tech,&c,&active,&rules,GameDate {year:1938,month:1,day:1}),1.0);
    }
    #[test]
    fn branch_ancestors_unsupported_conditions_and_cycle_validation() {
        let mut tree=tree();tree.technologies.get_mut("a").unwrap().allow_branch=
            Requirement::HasCountryFlag("program".into());
        let mut c=CountryResearch::new(1,&ResearchRules::default());
        assert!(!tree.visible("b",&c));c.flags.insert("program".into());
        assert!(tree.visible("b",&c));
        tree.technologies.get_mut("a").unwrap().allow = Requirement::Unsupported("x".into());
        assert!(!tree.visible("a",&c));
        tree.technologies.get_mut("a").unwrap().prerequisites.insert("b".into());
        assert!(tree.validate().is_err());
    }
    #[test]
    fn slot_resize_does_not_delete_active_research() {
        let tree=tree();let rules=ResearchRules::default();let mut c=CountryResearch::new(2,&rules);
        c.start(1,"a",&tree,&rules,GameDate::START).unwrap();
        assert!(c.resize_slots(1,&rules).is_err());
        assert!(c.cancel(1));assert!(c.resize_slots(1,&rules).is_ok());
    }
    #[test]
    fn speed_bonuses_are_additive_and_respect_ten_percent_floor() {
        let tree=tree();let rules=ResearchRules::default();
        let mut country=CountryResearch::new(1,&rules);
        let tech=&tree.technologies["a"];
        let active=ActiveResearch {tech_id:"a".into(),progress:0.0,locked_bonus:0.0,ahead_reduction:0.0};
        country.speed_bonus=0.5;
        country.category_bonuses.insert("industry".into(),0.1);
        assert!((daily_speed(tech,&country,&active,&rules,GameDate::START)-1.6).abs()<1e-10);
        country.speed_bonus=-2.0;
        assert!((daily_speed(tech,&country,&active,&rules,GameDate::START)-0.1).abs()<1e-10);
    }
    #[test]
    fn speed_changes_affect_ongoing_research_but_used_focus_bonus_does_not() {
        let tree=tree();let rules=ResearchRules::default();
        let mut country=CountryResearch::new(1,&rules);
        country.available_bonuses.push(ResearchBonus{
            name:"focus".into(),category:Some("industry".into()),technology:None,
            speed:0.5,ahead_reduction_years:1.0,uses:1,
        });
        country.start(0,"a",&tree,&rules,GameDate::START).unwrap();
        let active=country.slots[0].active.as_ref().unwrap().clone();
        let tech=&tree.technologies["a"];
        assert!((daily_speed(tech,&country,&active,&rules,GameDate::START)-1.5).abs()<1e-10);
        country.speed_bonus=0.2;
        assert!((daily_speed(tech,&country,&active,&rules,GameDate::START)-1.7).abs()<1e-10);
        assert_eq!(country.available_bonuses[0].uses,0);
    }
    #[test]
    fn era_ahead_dynamic_eta_is_recomputed_from_the_day_step() {
        let mut cat=ResearchCatalogue::default();
        cat.insert(Technology::sample("future",1938,1.0)).unwrap();
        let rules=ResearchRules::default();
        let mut c=CountryResearch::new(1,&rules);
        c.start(0,"future",&cat,&rules,GameDate::START).unwrap();
        let eta_1936=c.estimated_days(0,&cat,&rules,GameDate::START).unwrap();
        let eta_later=c.estimated_days(0,&cat,&rules,GameDate{
            year:1937,month:12,day:31
        }).unwrap();
        assert!(eta_1936>eta_later);
        assert!(eta_1936>110);
    }
    #[test]
    fn tech_speed_modifier_does_not_replace_manual_country_bonus() {
        let mut cat=tree();
        cat.technologies.get_mut("a").unwrap().modifiers.insert("research_speed_factor".into(),0.1);
        let rules=ResearchRules::default();
        let mut c=CountryResearch::new(1,&rules);
        c.speed_bonus=0.25;
        c.grant("a",&cat);
        assert!((c.speed_bonus-0.35).abs()<1e-10);
    }

    #[test]
    fn research_grants_unlocks_and_country_state_isolated() {
        let mut tree=tree();
        tree.technologies.get_mut("a").unwrap().unlocks.insert("equipment:test".into());
        tree.technologies.get_mut("a").unwrap().modifiers.insert("research_speed_factor".into(),0.1);
        let mut world=ResearchWorld::default();let rules=ResearchRules::default();
        let a=world.country_mut("AAA",2,&rules);a.grant("a",&tree);
        assert!(a.unlocks.contains("equipment:test"));
        assert_eq!(a.speed_bonus,0.1);
        assert!(world.country("BBB").is_none());
    }
}
