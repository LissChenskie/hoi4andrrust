//! Minimal truthful, resumable national-focus lifecycle using imported 1.19.x AST.
//! Missing defines, effects, tree selectors and politics are blockers, never guessed.
use crate::{
    campaign::Context,
    pdx::{self, Entry},
    research::{ResearchCatalogue, ResearchRules},
    session::GameSession,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveFocus {
    pub id: String,
    pub tree: String,
    pub progress: f64,
    pub required: f64,
    pub begun: u64,
    #[serde(default)]
    pub blocked: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FocusCampaign {
    pub active: BTreeMap<String, ActiveFocus>,
    #[serde(default)]
    pub bypassed: BTreeMap<String, BTreeSet<String>>,
    #[serde(default)]
    pub failures: BTreeMap<String, String>,
}
fn finite(v: Option<&f64>, name: &str) -> Result<f64, String> {
    let x = v
        .copied()
        .ok_or_else(|| format!("HOI4 define {name} missing"))?;
    if !x.is_finite() || x <= 0. {
        return Err(format!("Invalid HOI4 define {name}"));
    }
    Ok(x)
}
fn cost(raw: &[Entry], s: &GameSession) -> Result<f64, String> {
    let n = pdx::atom(raw, "cost")
        .ok_or("Focus cost not present in original data")?
        .parse::<f64>()
        .map_err(|_| "Non-literal focus cost")?;
    if !n.is_finite() || n <= 0. {
        return Err("Invalid focus cost".into());
    }
    Ok(n * finite(
        s.military
            .catalogue
            .literal_defines
            .get("NDefines.NFocus.FOCUS_POINT_DAYS"),
        "NFocus.FOCUS_POINT_DAYS",
    )?)
}
fn selection(
    tree: &crate::research_script::FocusTreeRecord,
    tag: &str,
) -> Result<Option<f64>, String> {
    let b = &tree.country;
    if b.is_empty() {
        return Ok(tree.default.then_some(0.));
    }
    let mut score = match pdx::atom(b, "factor") {
        Some(n) => n
            .parse::<f64>()
            .map_err(|_| format!("{}: variable tree factor unsupported", tree.id))?,
        None => 1.,
    };
    if !score.is_finite() {
        return Err("Invalid focus tree score".into());
    }
    for e in b.iter().filter(|e| e.key == "modifier") {
        let items = e.block().ok_or("Focus tree modifier must be a block")?;
        // Only the simple, verifiable tag/original_tag selector is admitted
        // to a first playable tree. Complex predicates are not interpreted as true.
        if items
            .iter()
            .any(|f| !["add", "tag", "original_tag"].contains(&f.key.as_str()))
        {
            return Err(format!("{}: unsupported focus tree selector", tree.id));
        }
        let match_tag = items
            .iter()
            .filter(|f| matches!(f.key.as_str(), "tag" | "original_tag"))
            .all(|f| f.atom() == Some(tag));
        if match_tag {
            let v = pdx::atom(items, "add")
                .ok_or("Focus tree modifier lacks add")?
                .parse::<f64>()
                .map_err(|_| "Nonliteral tree score")?;
            score += v;
        }
    }
    if b.iter()
        .any(|f| !["factor", "modifier"].contains(&f.key.as_str()))
    {
        return Err(format!("{}: unsupported country selector", tree.id));
    }
    if score > 0. {
        Ok(Some(score))
    } else {
        Ok(None)
    }
}
/// Read-only tree selection for both simulation and the existing focus panel.
pub fn selected_tree<'a>(original: &str, cat: &'a ResearchCatalogue)
    -> Result<&'a crate::research_script::FocusTreeRecord, String> {
    let mut choices = Vec::new();
    let mut unsupported = Vec::new();
    for tree in cat.scripts.focus_trees.values() {
        match selection(tree, original) {
            Ok(Some(score)) => choices.push((score, tree)),
            Ok(None) => {}
            Err(e) => unsupported.push(e),
        }
    }
    choices.sort_by(|(a, _), (b, _)| b.total_cmp(a));
    let Some((score, tree)) = choices.first() else {
        return Err(format!("No verified focus tree for {original}; {}",
            unsupported.join("; ")));
    };
    if choices.get(1).is_some_and(|(other, _)| other == score) {
        return Err(format!("Ambiguous focus trees for {original} at score {score}"));
    }
    Ok(tree)
}
impl GameSession {
    /// Country-tree selection never silently falls through unknown DLC gates.
    pub fn focus_tree<'a>(
        &self,
        tag: &str,
        cat: &'a ResearchCatalogue,
    ) -> Result<&'a crate::research_script::FocusTreeRecord, String> {
        let c = self
            .research
            .country(tag)
            .ok_or("No country research history")?;
        let original = if c.context.original_tag.is_empty() {
            tag
        } else {
            &c.context.original_tag
        };
        selected_tree(original, cat)
    }
    fn focus_rules<'a>(
        &self,
        tag: &str,
        id: &str,
        cat: &'a ResearchCatalogue,
        rules: &ResearchRules,
    ) -> Result<(&'a crate::research_script::ScriptSource, f64), String> {
        let tree = self.focus_tree(tag, cat)?;
        if !tree.focuses.contains(id) {
            return Err(format!("{id} not in selected tree {}", tree.id));
        }
        let src = cat
            .scripts
            .sources
            .get(&format!("focus:{id}"))
            .ok_or("Focus source unavailable")?;
        let c = self
            .research
            .country(tag)
            .ok_or("Missing country research state")?;
        if c.context.completed_focuses.contains(id) {
            return Err("Focus completed".into());
        }
        for group in src.raw.iter().filter(|f| f.key == "prerequisite") {
            let req = group.block().ok_or("Bad prerequisite syntax")?;
            let ids = req
                .iter()
                .filter(|f| f.key == "focus")
                .map(|f| f.atom().ok_or("Nonliteral prerequisite"))
                .collect::<Result<Vec<_>, _>>()?;
            if ids.is_empty() {
                return Err("Empty prerequisite group".into());
            }
            if !ids.iter().any(|id| {
                c.context.completed_focuses.contains(*id)
                    || self
                        .campaign
                        .focuses
                        .bypassed
                        .get(tag)
                        .is_some_and(|b| b.contains(*id))
            }) {
                return Err(format!("Unmet prerequisite: {}", ids.join(" OR ")));
            }
        }
        for xor in src.raw.iter().filter(|f| f.key == "mutually_exclusive") {
            for sibling in xor
                .block()
                .ok_or("Bad mutually exclusive definition")?
                .iter()
                .filter(|e| e.key == "focus")
            {
                let other = sibling.atom().ok_or("Bad exclusive reference")?;
                if c.context.completed_focuses.contains(other) {
                    return Err(format!("Mutually exclusive focus completed: {other}"));
                }
            }
        }
        let ctx = Context::country(tag);
        for name in ["visible", "available", "allow_branch"] {
            if let Some(conditions) = pdx::block(&src.raw, name) {
                if !self.campaign_conditions(&ctx, conditions, cat, rules)? {
                    return Err(format!("Focus {id} blocked: {name}"));
                }
            }
        }
        if let Some(bypass) = pdx::block(&src.raw, "bypass") {
            if self.campaign_conditions(&ctx, bypass, cat, &ResearchRules::default())? {
                return Err("Focus bypass condition already met; bypass handling required".into());
            }
        }
        for key in [
            "select_effect",
            "cancel_if_invalid",
            "continue_if_invalid",
            "completion_reward",
        ] {
            if let Some(effects) = pdx::block(&src.raw, key) {
                if key == "completion_reward" {
                    crate::campaign::preflight(effects, cat)
                        .map_err(|e| format!("{} {} reward: {e}", src.source, src.id))?;
                } else {
                    return Err(format!(
                        "{}: unsupported focus lifecycle field {key}",
                        src.source
                    ));
                }
            }
        }
        // HOI4 cost is point-days, not always 70.
        Ok((src, cost(&src.raw, self)?))
    }
    pub fn begin_focus(
        &mut self,
        tag: &str,
        id: &str,
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
    ) -> Result<(), String> {
        if !self.world.countries.contains_key(tag) || self.campaign.inactive.contains(tag) {
            return Err("Country absent or inactive".into());
        }
        if self.campaign.focuses.active.contains_key(tag) {
            return Err("Focus already active".into());
        }
        let (src, required) = self.focus_rules(tag, id, cat, rules)?;
        let tree = self.focus_tree(tag, cat)?.id.clone();
        if self
            .campaign
            .countries
            .get(tag)
            .and_then(|p| p.power)
            .is_none()
        {
            return Err("Starting political power not imported; focus costs cannot be paid".into());
        }
        self.campaign.focuses.active.insert(
            tag.into(),
            ActiveFocus {
                id: src.id.clone(),
                tree,
                required,
                progress: 0.,
                begun: self.clock.elapsed_hours,
                blocked: None,
            },
        );
        Ok(())
    }
    pub fn cancel_focus(&mut self, tag: &str) -> Result<(), String> {
        self.campaign
            .focuses
            .active
            .remove(tag)
            .ok_or_else(|| "No focus active".to_owned())?;
        Ok(())
    }
    pub fn tick_focuses(&mut self, cat: &ResearchCatalogue, rules: &ResearchRules) {
        let tags = self
            .campaign
            .focuses
            .active
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for tag in tags {
            let Some(active) = self.campaign.focuses.active.get(&tag).cloned() else {
                continue;
            };
            // Validate current selection, cost and prerequisites each midnight.
            let check = self.focus_rules(&tag, &active.id, cat, rules);
            let mut blocked = check.err();
            let war = self
                .military
                .wars
                .iter()
                .any(|(a, b)| a == &tag || b == &tag);
            let key = if war {
                "NDefines.NFocus.FOCUS_PROGRESS_WAR"
            } else {
                "NDefines.NFocus.FOCUS_PROGRESS_PEACE"
            };
            let speed = finite(self.military.catalogue.literal_defines.get(key), key);
            if blocked.is_none() {
                blocked = speed.as_ref().err().cloned();
            }
            let power = self.campaign.countries.get(&tag).and_then(|p| p.power);
            if blocked.is_none() && power.is_none() {
                blocked = Some("Political power unknown".into());
            }
            if let Some(reason) = blocked {
                if let Some(item) = self.campaign.focuses.active.get_mut(&tag) {
                    item.blocked = Some(reason);
                }
                continue;
            }
            let step = speed.unwrap();
            let next = active.progress + step;
            // Focus spends political power in game-time. A deficit is recorded,
            // never silently replaced by a free progress tick.
            if let Some(p) = self.campaign.countries.get_mut(&tag) {
                p.power = Some(p.power.unwrap() - step);
            }
            if next + 1e-9 < active.required {
                if let Some(item) = self.campaign.focuses.active.get_mut(&tag) {
                    item.progress = next;
                    item.blocked = None;
                }
                continue;
            }
            let source = cat.scripts.sources[&format!("focus:{}", active.id)].clone();
            let result = self.campaign_effects(
                &Context::country(&tag),
                &source.effects,
                &source.source,
                cat,
                rules,
            );
            match result {
                Ok(()) => {
                    // Runtime rewards complete before marking history done;
                    // never repeat the same reward after save/load.
                    if let Some(c) = self.research.countries.get_mut(&tag) {
                        c.context.completed_focuses.insert(active.id.clone());
                        c.revision = c.revision.saturating_add(1);
                    }
                    self.campaign.focuses.active.remove(&tag);
                    self.campaign.focuses.failures.remove(&tag);
                }
                Err(reason) => {
                    // Roll back focus payment on failed reward; retain the
                    // blocked attempt for diagnosis and allow compatible restart.
                    if let Some(p) = self.campaign.countries.get_mut(&tag) {
                        p.power = Some(p.power.unwrap() + step);
                    }
                    if let Some(item) = self.campaign.focuses.active.get_mut(&tag) {
                        item.blocked = Some(format!("{}: {reason}", source.source));
                    }
                    self.campaign.focuses.failures.insert(tag.clone(), reason);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        campaign::CountryPolitics,
        military_model::tests::catalogue,
        research::{CountryResearch,ResearchCatalogue},
        research_script::{FocusTreeRecord,ScriptSource},
        warfare::Warfare,
    };
    fn prepared() -> (GameSession,ResearchCatalogue,ResearchRules) {
        let mut s=GameSession::new(crate::warfare::tests::world());
        let mut db=catalogue();
        for (k,n) in [
            ("FOCUS_POINT_DAYS",7.),("FOCUS_PROGRESS_PEACE",1.),
            ("FOCUS_PROGRESS_WAR",1.)
        ] {
            db.literal_defines.insert(format!("NDefines.NFocus.{k}"),n);
        }
        s.military=Warfare::new(db);
        s.campaign.countries.insert("A".into(),CountryPolitics{
            power:Some(100.),stability:Some(0.5),war_support:Some(0.5)
        });
        let rules=ResearchRules::default();
        let mut c=CountryResearch::new(1,&rules);
        c.context.tag="A".into();
        c.context.original_tag="A".into();
        s.research.countries.insert("A".into(),c);
        let mut cat=ResearchCatalogue::default();
        cat.scripts.focus_trees.insert("test".into(),FocusTreeRecord{
            id:"test".into(),source:"authored/test.txt:1".into(),
            country:crate::pdx::parse_pdx("factor=0 modifier={add=10 tag=A}").unwrap(),
            default:false,
            focuses:BTreeSet::from(["first".into(),"second".into(),"bad".into()]),
        });
        for (id,txt) in [
            ("first","id=first cost=1 completion_reward={add_political_power=5}"),
            ("second","id=second cost=2 prerequisite={focus=first} completion_reward={add_political_power=3}"),
            ("bad","id=bad cost=1 completion_reward={undocumented_effect=yes}"),
        ] {
            let raw=crate::pdx::parse_pdx(txt).unwrap();
            cat.scripts.sources.insert(format!("focus:{id}"),ScriptSource{
                kind:"focus".into(),id:id.into(),source:format!("authored/test.txt:{id}"),
                conditions:Vec::new(),
                effects:crate::pdx::block(&raw,"completion_reward").unwrap().to_vec(),
                raw,
            });
        }
        (s,cat,rules)
    }
    #[test]
    fn imported_cost_prerequisites_and_one_time_reward() {
        let (mut s,cat,rules)=prepared();
        assert!(s.begin_focus("A","second",&cat,&rules).is_err());
        s.begin_focus("A","first",&cat,&rules).unwrap();
        assert_eq!(s.campaign.focuses.active["A"].required,7.);
        for _ in 0..6 { s.tick_focuses(&cat,&rules); }
        assert!(!s.research.countries["A"].context.completed_focuses.contains("first"));
        let snapshot=serde_json::to_string(&s.campaign).unwrap();
        let restored:crate::campaign::CampaignState=serde_json::from_str(&snapshot).unwrap();
        assert_eq!(restored.focuses.active["A"].progress,6.);
        s.tick_focuses(&cat,&rules);
        assert!(s.research.countries["A"].context.completed_focuses.contains("first"));
        assert_eq!(s.campaign.countries["A"].power,Some(98.));
        assert!(s.campaign.focuses.active.is_empty());
        s.begin_focus("A","second",&cat,&rules).unwrap();
        assert_eq!(s.campaign.focuses.active["A"].required,14.);
        assert!(s.begin_focus("A","first",&cat,&rules).is_err());
    }
    #[test]
    fn unsupported_reward_cannot_start_or_spend_currency() {
        let (mut s,cat,rules)=prepared();
        let before=serde_json::to_value(&s.campaign).unwrap();
        let r=s.begin_focus("A","bad",&cat,&rules);
        assert!(r.unwrap_err().contains("Unsupported"));
        assert_eq!(serde_json::to_value(&s.campaign).unwrap(),before);
    }
    #[test]
    fn missing_versioned_focus_timing_is_not_guessed() {
        let (mut s,cat,rules)=prepared();
        let mut db=(*s.military.catalogue).clone();
        db.literal_defines.remove("NDefines.NFocus.FOCUS_POINT_DAYS");
        s.military=Warfare::new(db);
        assert!(s.begin_focus("A","first",&cat,&rules).unwrap_err().contains("define"));
    }
    #[test]
    fn nonmatching_tree_rejected_without_stealing_foreign_focuses() {
        let (mut s,cat,rules)=prepared();
        s.research.countries.get_mut("A").unwrap().context.original_tag="B".into();
        assert!(s.begin_focus("A","first",&cat,&rules).is_err());
    }
}
