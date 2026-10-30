//! National-focus lifecycle; all script consequences use campaign transactions.
//! Unsupported source commands and unknown runtime facts remain explicit errors.
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
    #[serde(default)]
    pub manual_cancel_forbidden: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FocusCampaign {
    pub active: BTreeMap<String, ActiveFocus>,
    #[serde(default)]
    pub bypassed: BTreeMap<String, BTreeSet<String>>,
    #[serde(default)]
    pub failures: BTreeMap<String, String>,
    #[serde(default)]
    pub selected: BTreeMap<String, String>,
    #[serde(default)]
    pub layouts: BTreeMap<String, BTreeMap<String, bool>>,
    #[serde(default)]
    pub layout_trees: BTreeMap<String, String>,
    #[serde(default)]
    pub positions: BTreeMap<String, BTreeMap<String, (f32, f32)>>,
    #[serde(default)]
    pub layout_revision: u64,
}
fn finite(v: Option<&f64>, name: &str) -> Result<f64, String> {
    v.copied()
        .filter(|v| v.is_finite() && *v > 0.)
        .ok_or_else(|| format!("Missing/invalid HOI4 define {name}"))
}
fn flag(raw: &[Entry], key: &str, default: bool) -> Result<bool, String> {
    match raw.iter().find(|e| e.key == key) {
        None => Ok(default),
        Some(e) => match e.atom() {
            Some("yes") => Ok(true),
            Some("no") => Ok(false),
            _ => Err(format!("{key} requires yes/no")),
        },
    }
}
fn cost(raw: &[Entry], s: &GameSession) -> Result<f64, String> {
    let n = pdx::atom(raw, "cost")
        .and_then(|v| v.parse::<f64>().ok())
        .ok_or("Focus requires literal cost")?;
    let required = (n * finite(
        s.military
            .catalogue
            .literal_defines
            .get("NDefines.NFocus.FOCUS_POINT_DAYS"),
        "NFocus.FOCUS_POINT_DAYS",
    )?)
    .floor();
    if !required.is_finite() || required < 1. {
        return Err("Invalid/zero-day focus cost".into());
    }
    Ok(required)
}
/// Contextual score calculation used by simulation and UI. Unknown selectors in
/// ANY candidate prevent a misleading fallback to an unrelated generic tree.
fn select_tree<'a>(
    cat: &'a ResearchCatalogue,
    mut condition: impl FnMut(&[Entry]) -> Result<bool, String>,
) -> Result<&'a crate::research_script::FocusTreeRecord, String> {
    let mut choices = Vec::new();
    let mut defaults = Vec::new();
    for tree in cat.scripts.focus_trees.values() {
        let number = |v: &str| {
            v.parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .ok_or_else(|| format!("{}: nonliteral tree score", tree.id))
        };
        let mut score = pdx::atom(&tree.country, "factor")
            .or_else(|| pdx::atom(&tree.country, "base"))
            .map(number)
            .transpose()?
            .unwrap_or(1.);
        for e in &tree.country {
            match e.key.as_str() {
                "base" | "factor" => {}
                "modifier" => {
                    let raw = e.block().ok_or("Tree modifier requires block")?;
                    let conditions: Vec<_> = raw
                        .iter()
                        .filter(|e| !["add", "factor"].contains(&e.key.as_str()))
                        .cloned()
                        .collect();
                    if condition(&conditions).map_err(|e| format!("{} selector: {e}", tree.id))? {
                        for e in raw
                            .iter()
                            .filter(|e| ["add", "factor"].contains(&e.key.as_str()))
                        {
                            let n = number(e.atom().ok_or("Score requires atom")?)?;
                            if e.key == "add" {
                                score += n;
                            } else {
                                score *= n;
                            }
                        }
                    }
                }
                k => return Err(format!("{}: unsupported selector {k}", tree.id)),
            }
        }
        if !score.is_finite() {
            return Err("Non-finite focus tree score".into());
        }
        if tree.default {
            defaults.push(tree);
        }
        if !tree.country.is_empty() && score > 0. {
            choices.push((score, tree));
        }
    }
    choices.sort_by(|(a, _), (b, _)| b.total_cmp(a));
    if let Some((score, tree)) = choices.first() {
        if choices.get(1).is_some_and(|(s, _)| s == score) {
            return Err(format!("Ambiguous focus tree score {score}"));
        }
        return Ok(tree);
    }
    if defaults.len() == 1 {
        return Ok(defaults[0]);
    }
    Err("No unambiguous focus tree".into())
}
impl GameSession {
    pub fn focus_tree<'a>(
        &self,
        tag: &str,
        cat: &'a ResearchCatalogue,
    ) -> Result<&'a crate::research_script::FocusTreeRecord, String> {
        if let Some(id) = self.campaign.focuses.selected.get(tag) {
            return cat
                .scripts
                .focus_trees
                .get(id)
                .ok_or_else(|| format!("Missing selected focus tree {id}"));
        }
        self.research
            .country(tag)
            .ok_or("Missing country history")?;
        select_tree(cat, |b| {
            self.campaign_conditions(&Context::country(tag), b, cat, &ResearchRules::default())
        })
    }
    /// Cache allow_branch only on loading/explicit layout invalidation, like the
    /// source command mark_focus_tree_layout_dirty; don't change branches each tick.
    pub fn refresh_focus_layout(
        &mut self,
        tag: &str,
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
    ) -> Result<(), String> {
        let tree = self.focus_tree(tag, cat)?;
        if self.campaign.focuses.layout_trees.get(tag) == Some(&tree.id)
            && self.campaign.focuses.layouts.contains_key(tag)
            && self.campaign.focuses.positions.contains_key(tag)
        {
            return Ok(());
        }
        if !tree.problems.is_empty() {
            return Err(tree.problems.join("; "));
        }
        let mut allowed = BTreeMap::new();
        fn visit(
            s: &GameSession,
            tag: &str,
            id: &str,
            cat: &ResearchCatalogue,
            rules: &ResearchRules,
            out: &mut BTreeMap<String, bool>,
            stack: &mut BTreeSet<String>,
        ) -> Result<bool, String> {
            if let Some(v) = out.get(id) {
                return Ok(*v);
            }
            if stack.len() >= 64 || !stack.insert(id.into()) {
                return Err(format!("Focus branch cycle at {id}"));
            }
            let src = cat
                .scripts
                .sources
                .get(&format!("focus:{id}"))
                .ok_or("Focus source missing")?;
            let mut v = true;
            if let Some(b) = pdx::block(&src.raw, "allow_branch") {
                v = s.campaign_conditions(&Context::country(tag), b, cat, rules)?;
            } else {
                for parent in hoi4_mobile::focus_data::references(&src.raw, "prerequisite") {
                    // A shared branch may reference a parent outside this tree.
                    if cat.scripts.sources.contains_key(&format!("focus:{parent}")) {
                        v &= visit(s, tag, parent, cat, rules, out, stack)?;
                    }
                }
            }
            stack.remove(id);
            out.insert(id.into(), v);
            Ok(v)
        }
        for id in &tree.focuses {
            visit(
                self,
                tag,
                id,
                cat,
                rules,
                &mut allowed,
                &mut BTreeSet::new(),
            )?;
        }
        let positions = hoi4_mobile::focus_data::positions(tree, &cat.scripts, |b| {
            self.campaign_conditions(&Context::country(tag), b, cat, rules)
        })?;
        let id = tree.id.clone();
        self.campaign
            .focuses
            .positions
            .insert(tag.into(), positions);
        self.campaign.focuses.layout_revision =
            self.campaign.focuses.layout_revision.saturating_add(1);
        self.campaign.focuses.layouts.insert(tag.into(), allowed);
        self.campaign
            .focuses
            .layout_trees
            .insert(tag.into(), id.clone());
        self.campaign.focuses.selected.insert(tag.into(), id);
        Ok(())
    }
    fn source<'a>(
        &self,
        tag: &str,
        id: &str,
        cat: &'a ResearchCatalogue,
    ) -> Result<&'a crate::research_script::ScriptSource, String> {
        let tree = self.focus_tree(tag, cat)?;
        if !tree.problems.is_empty() {
            return Err(tree.problems.join("; "));
        }
        if !tree.focuses.contains(id) {
            return Err(format!("{id} not in tree {}", tree.id));
        }
        let src = cat
            .scripts
            .sources
            .get(&format!("focus:{id}"))
            .ok_or("Focus source missing")?;
        let unknown = hoi4_mobile::focus_data::unsupported_focus_fields(&src.raw);
        if !unknown.is_empty() {
            return Err(format!("Unsupported focus fields: {unknown:?}"));
        }
        for key in [
            "joint_trigger",
            "completion_reward_joint_originator",
            "completion_reward_joint_member",
        ] {
            if src.raw.iter().any(|e| e.key == key) {
                return Err(format!("Unsupported focus field {key}"));
            }
        }
        for k in [
            "cancel_if_invalid",
            "continue_if_invalid",
            "cancelable",
            "bypass_if_unavailable",
            "enable_automatic_bypass",
            "available_if_capitulated",
        ] {
            flag(&src.raw, k, false)?;
        }
        Ok(src)
    }
    fn dependencies(
        &self,
        tag: &str,
        src: &crate::research_script::ScriptSource,
    ) -> Result<(), String> {
        let c = self
            .research
            .country(tag)
            .ok_or("Missing country history")?;
        if c.context.completed_focuses.contains(&src.id) {
            return Err("Focus completed".into());
        }
        for group in src.raw.iter().filter(|e| e.key == "prerequisite") {
            let b = group.block().ok_or("Prerequisite requires block")?;
            let mut met = b.is_empty();
            for e in b {
                if e.key != "focus" {
                    return Err(format!("Unknown prerequisite {}", e.key));
                }
                met |= c
                    .context
                    .completed_focuses
                    .contains(e.atom().ok_or("Focus ID required")?);
            }
            if !met {
                return Err("Unmet prerequisite group".into());
            }
        }
        for other in hoi4_mobile::focus_data::references(&src.raw, "mutually_exclusive") {
            if c.context.completed_focuses.contains(other)
                || self
                    .campaign
                    .focuses
                    .active
                    .get(tag)
                    .is_some_and(|a| a.id == other)
            {
                return Err(format!("Mutually exclusive: {other}"));
            }
        }
        Ok(())
    }
    fn condition(
        &self,
        tag: &str,
        src: &crate::research_script::ScriptSource,
        key: &str,
        default: bool,
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
    ) -> Result<bool, String> {
        match src.raw.iter().find(|e| e.key == key) {
            None => Ok(default),
            Some(e) => self.campaign_conditions(
                &Context::country(tag),
                e.block().ok_or_else(|| format!("{key} requires block"))?,
                cat,
                rules,
            ),
        }
    }
    fn available(
        &self,
        tag: &str,
        src: &crate::research_script::ScriptSource,
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
    ) -> Result<bool, String> {
        Ok((!self.peace.capitulated.contains(tag)
            || flag(&src.raw, "available_if_capitulated", false)?)
            && self.condition(tag, src, "available", true, cat, rules)?)
    }
    fn bypass_condition(
        &self,
        tag: &str,
        src: &crate::research_script::ScriptSource,
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
    ) -> Result<bool, String> {
        if self.condition(tag, src, "bypass", false, cat, rules)? {
            return Ok(true);
        }
        Ok(flag(&src.raw, "bypass_if_unavailable", false)?
            && !self.available(tag, src, cat, rules)?)
    }
    pub fn focus_blocked_reason(
        &self,
        tag: &str,
        id: &str,
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
    ) -> Result<(), String> {
        if let Some(active) = self.campaign.focuses.active.get(tag) {
            return Err(if active.id == id {
                "Focus active".into()
            } else {
                "Another focus already active".into()
            });
        }
        let src = self.source(tag, id, cat)?;
        self.dependencies(tag, src)?;
        if self
            .campaign
            .focuses
            .layouts
            .get(tag)
            .and_then(|b| b.get(id))
            == Some(&false)
        {
            return Err("Hidden focus branch".into());
        }
        if !self.condition(tag, src, "visible", true, cat, rules)? {
            return Err("Focus not visible".into());
        }
        if self.bypass_condition(tag, src, cat, rules)? {
            return Ok(());
        }
        if !self.available(tag, src, cat, rules)? {
            return Err("Focus unavailable".into());
        }
        for key in ["completion_reward", "select_effect"] {
            if let Some(b) = pdx::block(&src.raw, key) {
                crate::campaign::preflight(b, cat)?;
            }
        }
        cost(&src.raw, self)?;
        self.political_income(tag, cat)?;
        if self
            .campaign
            .countries
            .get(tag)
            .and_then(|p| p.power)
            .is_none()
        {
            return Err("Political power unknown".into());
        }
        Ok(())
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
        let before = self.campaign.focuses.clone();
        let check = self
            .refresh_focus_layout(tag, cat, rules)
            .and_then(|_| self.focus_blocked_reason(tag, id, cat, rules));
        if let Err(e) = check {
            self.campaign.focuses = before;
            return Err(e);
        }
        let src = self.source(tag, id, cat)?;
        if self.bypass_condition(tag, src, cat, rules)? {
            return self.bypass_focus(tag, id, cat, rules);
        }
        let required = cost(&src.raw, self)?;
        let tree = self.focus_tree(tag, cat)?.id.clone();
        let forbidden =
            !flag(&src.raw, "cancelable", true)? || pdx::block(&src.raw, "select_effect").is_some();
        let effects = pdx::block(&src.raw, "select_effect").unwrap_or(&[]);
        if let Err(e) =
            self.campaign_effects(&Context::country(tag), effects, &src.source, cat, rules)
        {
            self.campaign.focuses = before;
            return Err(e);
        }
        self.campaign.focuses.active.insert(
            tag.into(),
            ActiveFocus {
                id: id.into(),
                tree,
                progress: 0.,
                required,
                begun: self.clock.elapsed_hours,
                blocked: None,
                manual_cancel_forbidden: forbidden,
            },
        );
        self.campaign.focuses.failures.remove(tag);
        Ok(())
    }
    pub fn cancel_focus(
        &mut self,
        tag: &str,
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
    ) -> Result<(), String> {
        let f = self
            .campaign
            .focuses
            .active
            .get(tag)
            .ok_or("No focus active")?
            .clone();
        let src = self.source(tag, &f.id, cat)?;
        if f.manual_cancel_forbidden
            || !flag(&src.raw, "cancelable", true)?
            || pdx::block(&src.raw, "select_effect").is_some()
        {
            return Err("Source focus cannot be cancelled manually".into());
        }
        self.campaign_effects(
            &Context::country(tag),
            pdx::block(&src.raw, "cancel_effect").unwrap_or(&[]),
            &src.source,
            cat,
            rules,
        )?;
        self.campaign.focuses.active.remove(tag);
        Ok(())
    }
    pub fn bypass_focus(
        &mut self,
        tag: &str,
        id: &str,
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
    ) -> Result<(), String> {
        let src = self.source(tag, id, cat)?;
        self.dependencies(tag, src)?;
        if !self.bypass_condition(tag, src, cat, rules)? {
            return Err("Bypass conditions false".into());
        }
        let b = pdx::block(&src.raw, "bypass_effect").unwrap_or(&[]);
        self.campaign_effects(&Context::country(tag), b, &src.source, cat, rules)?;
        self.mark_focus_done(tag, id, true);
        Ok(())
    }
    fn mark_focus_done(&mut self, tag: &str, id: &str, bypassed: bool) {
        if let Some(c) = self.research.countries.get_mut(tag) {
            c.context.completed_focuses.insert(id.into());
            c.revision = c.revision.saturating_add(1);
        }
        if bypassed {
            self.campaign
                .focuses
                .bypassed
                .entry(tag.into())
                .or_default()
                .insert(id.into());
        }
        if self
            .campaign
            .focuses
            .active
            .get(tag)
            .is_some_and(|a| a.id == id)
        {
            self.campaign.focuses.active.remove(tag);
        }
        self.campaign.focuses.failures.remove(tag);
    }
    fn tick_active_focus(
        &mut self,
        tag: &str,
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
    ) -> Result<(), String> {
        let Some(a) = self.campaign.focuses.active.get(tag).cloned() else {
            return Ok(());
        };
        let src = self.source(tag, &a.id, cat)?;
        if self.focus_tree(tag, cat)?.id != a.tree {
            return Err("Active focus belongs to a different tree".into());
        }
        if self.bypass_condition(tag, src, cat, rules)? {
            return self.bypass_focus(tag, &a.id, cat, rules);
        }
        let available = self.available(tag, src, cat, rules)?;
        if self.condition(tag, src, "cancel", false, cat, rules)?
            || (!available && flag(&src.raw, "cancel_if_invalid", true)?)
        {
            let b = pdx::block(&src.raw, "cancel_effect").unwrap_or(&[]);
            self.campaign_effects(&Context::country(tag), b, &src.source, cat, rules)?;
            self.campaign.focuses.active.remove(tag);
            return Ok(());
        }
        if let Some(reason) = self.campaign.politics.failures.get(tag) {
            return Err(format!("Political income blocked: {reason}"));
        }
        if self
            .campaign
            .countries
            .get(tag)
            .and_then(|p| p.power)
            .is_none()
        {
            return Err("Political power unknown".into());
        }
        if !available && !flag(&src.raw, "continue_if_invalid", false)? {
            self.campaign.focuses.active.get_mut(tag).unwrap().blocked =
                Some("Paused: available conditions false".into());
            return Ok(());
        }
        let war = self.military.wars.iter().any(|(a, b)| a == tag || b == tag);
        let key = if war {
            "NDefines.NFocus.FOCUS_PROGRESS_WAR"
        } else {
            "NDefines.NFocus.FOCUS_PROGRESS_PEACE"
        };
        let step = finite(self.military.catalogue.literal_defines.get(key), key)?;
        let next = a.progress + step;
        if !next.is_finite() {
            return Err("Focus progress overflow".into());
        }
        if next + 1e-9 < a.required {
            let f = self.campaign.focuses.active.get_mut(tag).unwrap();
            f.progress = next;
            f.blocked = None;
            return Ok(());
        }
        match self.campaign_effects(
            &Context::country(tag),
            &src.effects,
            &src.source,
            cat,
            rules,
        ) {
            Ok(()) => {
                self.mark_focus_done(tag, &a.id, false);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
    pub fn tick_focuses(&mut self, cat: &ResearchCatalogue, rules: &ResearchRules) {
        // Deterministic country/ID order; repeat until stable so a bypass chain
        // is independent of lexical ID order. Bound passes by tree size.
        let tags: Vec<_> = self
            .research
            .countries
            .keys()
            .filter(|t| !self.campaign.inactive.contains(*t))
            .cloned()
            .collect();
        for tag in tags {
            let result = (|| -> Result<(), String> {
                let mut visited = BTreeSet::new();
                let limit = cat.scripts.sources.len().min(10000) + 1;
                for pass in 0..limit {
                    self.refresh_focus_layout(&tag, cat, rules)?;
                    let tree = self.focus_tree(&tag, cat)?.id.clone();
                    let ids: Vec<_> = self
                        .focus_tree(&tag, cat)?
                        .focuses
                        .iter()
                        .cloned()
                        .collect();
                    let mut changed = false;
                    for id in &ids {
                        let src = match self.source(&tag, id, cat) {
                            Ok(src) => src,
                            Err(e) => {
                                self.campaign
                                    .focuses
                                    .failures
                                    .insert(format!("{tag}:{id}"), e);
                                continue;
                            }
                        };
                        if self.dependencies(&tag, src).is_err()
                            || self.campaign.focuses.layouts[&tag].get(id) == Some(&false)
                        {
                            continue;
                        }
                        let attempt = (|| -> Result<bool, String> {
                            if flag(&src.raw, "enable_automatic_bypass", true)?
                                && self.bypass_condition(&tag, src, cat, rules)?
                            {
                                if !visited.insert((tree.clone(), id.clone())) {
                                    return Err(
                                        "Repeated automatic bypass/tree-switch cycle".into()
                                    );
                                }
                                self.bypass_focus(&tag, id, cat, rules)?;
                                return Ok(true);
                            }
                            Ok(false)
                        })();
                        match attempt {
                            Ok(true) => {
                                changed = true;
                                break;
                            }
                            Ok(false) => {}
                            Err(e) => {
                                self.campaign
                                    .focuses
                                    .failures
                                    .insert(format!("{tag}:{id}"), e);
                            }
                        }
                    }
                    if !changed {
                        break;
                    }
                    if pass + 1 == limit {
                        return Err("Automatic bypass pass budget exceeded".into());
                    }
                }
                self.tick_active_focus(&tag, cat, rules)
            })();
            if let Err(reason) = result {
                if let Some(f) = self.campaign.focuses.active.get_mut(&tag) {
                    f.blocked = Some(reason.clone());
                }
                self.campaign.focuses.failures.insert(tag, reason);
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        campaign::CountryPolitics,
        military_model::tests::catalogue,
        research::{CountryResearch, ResearchCatalogue},
        research_script::{FocusTreeRecord, ScriptSource},
        warfare::Warfare,
    };
    pub(crate) fn prepared() -> (GameSession, ResearchCatalogue, ResearchRules) {
        let mut s = GameSession::new(crate::warfare::tests::world());
        let mut db = catalogue();
        for (k, n) in [
            ("FOCUS_POINT_DAYS", 7.),
            ("FOCUS_PROGRESS_PEACE", 1.),
            ("FOCUS_PROGRESS_WAR", 1.),
        ] {
            db.literal_defines.insert(format!("NDefines.NFocus.{k}"), n);
        }
        s.military = Warfare::new(db);
        s.campaign.countries.insert(
            "A".into(),
            CountryPolitics {
                power: Some(100.),
                stability: Some(0.5),
                war_support: Some(0.5),
            },
        );
        let rules = ResearchRules::default();
        let mut c = CountryResearch::new(1, &rules);
        c.context.tag = "A".into();
        c.context.original_tag = "A".into();
        s.research.countries.insert("A".into(), c);
        let mut cat = ResearchCatalogue::default();
        cat.scripts.defines.extend([
            (
                "NDefines.NPolitics.BASE_POLITICAL_POWER_INCREASE".into(),
                0.,
            ),
            ("NDefines.NCountry.POLITICAL_POWER_LOWER_CAP".into(), -500.),
            ("NDefines.NCountry.POLITICAL_POWER_UPPER_CAP".into(), 2000.),
        ]);
        cat.scripts.static_modifiers.insert(
            "gain_focus".into(),
            pdx::parse_pdx("political_power_gain=-1").unwrap(),
        );
        cat.scripts.focus_trees.insert(
            "test".into(),
            FocusTreeRecord {
                id: "test".into(),
                source: "authored/test.txt:1".into(),
                country: crate::pdx::parse_pdx("factor=0 modifier={add=10 original_tag=A}")
                    .unwrap(),
                default: false,
                focuses: BTreeSet::from(["first".into(), "second".into(), "bad".into()]),
                ..Default::default()
            },
        );
        for (id, txt) in [
            (
                "first",
                "id=first cost=1 completion_reward={add_political_power=5}",
            ),
            (
                "second",
                "id=second cost=2 prerequisite={focus=first} completion_reward={add_political_power=3}",
            ),
            (
                "bad",
                "id=bad cost=1 completion_reward={undocumented_effect=yes}",
            ),
        ] {
            let raw = crate::pdx::parse_pdx(txt).unwrap();
            cat.scripts.sources.insert(
                format!("focus:{id}"),
                ScriptSource {
                    kind: "focus".into(),
                    id: id.into(),
                    source: format!("authored/test.txt:{id}"),
                    conditions: Vec::new(),
                    effects: crate::pdx::block(&raw, "completion_reward")
                        .unwrap()
                        .to_vec(),
                    raw,
                },
            );
        }
        (s, cat, rules)
    }
    fn day(s: &mut GameSession, cat: &ResearchCatalogue, rules: &ResearchRules) {
        s.clock.elapsed_hours += 24;
        s.tick_politics(cat);
        s.tick_focuses(cat, rules);
    }
    #[test]
    fn imported_cost_prerequisites_and_one_time_reward() {
        let (mut s, cat, rules) = prepared();
        assert!(s.begin_focus("A", "second", &cat, &rules).is_err());
        s.begin_focus("A", "first", &cat, &rules).unwrap();
        assert_eq!(s.campaign.focuses.active["A"].required, 7.);
        for _ in 0..6 {
            day(&mut s, &cat, &rules);
        }
        assert!(
            !s.research.countries["A"]
                .context
                .completed_focuses
                .contains("first")
        );
        let snapshot = serde_json::to_string(&s.campaign).unwrap();
        let restored: crate::campaign::CampaignState = serde_json::from_str(&snapshot).unwrap();
        assert_eq!(restored.focuses.active["A"].progress, 6.);
        day(&mut s, &cat, &rules);
        assert!(
            s.research.countries["A"]
                .context
                .completed_focuses
                .contains("first")
        );
        assert_eq!(s.campaign.countries["A"].power, Some(98.));
        assert!(s.campaign.focuses.active.is_empty());
        s.begin_focus("A", "second", &cat, &rules).unwrap();
        assert_eq!(s.campaign.focuses.active["A"].required, 14.);
        assert!(s.begin_focus("A", "first", &cat, &rules).is_err());
    }
    #[test]
    fn unsupported_reward_cannot_start_or_spend_currency() {
        let (mut s, cat, rules) = prepared();
        let before = serde_json::to_value(&s.campaign).unwrap();
        let r = s.begin_focus("A", "bad", &cat, &rules);
        assert!(r.unwrap_err().contains("Unsupported"));
        assert_eq!(serde_json::to_value(&s.campaign).unwrap(), before);
    }
    #[test]
    fn missing_versioned_focus_timing_is_not_guessed() {
        let (mut s, cat, rules) = prepared();
        let mut db = (*s.military.catalogue).clone();
        db.literal_defines
            .remove("NDefines.NFocus.FOCUS_POINT_DAYS");
        s.military = Warfare::new(db);
        assert!(
            s.begin_focus("A", "first", &cat, &rules)
                .unwrap_err()
                .contains("define")
        );
    }
    #[test]
    fn nonmatching_tree_rejected_without_stealing_foreign_focuses() {
        let (mut s, cat, rules) = prepared();
        s.research
            .countries
            .get_mut("A")
            .unwrap()
            .context
            .original_tag = "B".into();
        assert!(s.begin_focus("A", "first", &cat, &rules).is_err());
    }
    fn source(cat: &mut ResearchCatalogue, id: &str, txt: &str) {
        let raw = pdx::parse_pdx(txt).unwrap();
        cat.scripts
            .focus_trees
            .get_mut("test")
            .unwrap()
            .focuses
            .insert(id.into());
        cat.scripts.sources.insert(
            format!("focus:{id}"),
            ScriptSource {
                kind: "focus".into(),
                id: id.into(),
                source: "authored/lifecycle:1".into(),
                conditions: vec![],
                effects: pdx::block(&raw, "completion_reward")
                    .unwrap_or(&[])
                    .to_vec(),
                raw,
            },
        );
    }
    #[test]
    fn selector_uses_dlc_scripted_triggers_politics_and_original_tag() {
        let (mut s, mut cat, rules) = prepared();
        cat.scripts.triggers.insert(
            "eligible".into(),
            pdx::parse_pdx("original_tag=A has_dlc=Test political_power>20").unwrap(),
        );
        cat.scripts.focus_trees.get_mut("test").unwrap().country =
            pdx::parse_pdx("factor=0 modifier={add=10 eligible=yes}").unwrap();
        s.research
            .countries
            .get_mut("A")
            .unwrap()
            .context
            .dlcs
            .insert("Test".into());
        assert!(s.begin_focus("A", "first", &cat, &rules).is_ok());
        let (s, mut cat, _) = prepared();
        let mut other = FocusTreeRecord {
            id: "unknown".into(),
            country: pdx::parse_pdx("factor=0 modifier={add=100 NOT={missing_selector=yes}}")
                .unwrap(),
            ..Default::default()
        };
        other.default = false;
        cat.scripts.focus_trees.insert("unknown".into(), other);
        assert!(s.focus_tree("A", &cat).unwrap_err().contains("Unknown"));
    }
    #[test]
    fn select_effect_executes_once_and_forbids_manual_cancel() {
        let (mut s, mut cat, rules) = prepared();
        source(
            &mut cat,
            "first",
            "cost=1 select_effect={add_political_power=10}",
        );
        s.begin_focus("A", "first", &cat, &rules).unwrap();
        assert_eq!(s.campaign.countries["A"].power, Some(110.));
        assert!(s.cancel_focus("A", &cat, &rules).is_err());
        day(&mut s, &cat, &rules);
        day(&mut s, &cat, &rules);
        assert_eq!(s.campaign.countries["A"].power, Some(108.));
    }
    #[test]
    fn bypass_marks_history_runs_only_bypass_reward_and_unlocks_prerequisites() {
        let (mut s, mut cat, rules) = prepared();
        source(
            &mut cat,
            "first",
            "cost=1 bypass={has_country_flag=bypass_me} bypass_effect={add_political_power=2} completion_reward={add_political_power=999}",
        );
        s.research
            .countries
            .get_mut("A")
            .unwrap()
            .flags
            .insert("bypass_me".into());
        day(&mut s, &cat, &rules);
        assert!(s.campaign.focuses.bypassed["A"].contains("first"));
        assert!(
            s.research.countries["A"]
                .context
                .completed_focuses
                .contains("first")
        );
        assert_eq!(s.campaign.countries["A"].power, Some(102.));
        day(&mut s, &cat, &rules);
        assert_eq!(s.campaign.countries["A"].power, Some(102.));
        s.begin_focus("A", "second", &cat, &rules).unwrap();
    }
    #[test]
    fn manual_bypass_optout_does_not_prevent_explicit_bypass() {
        let (mut s, mut cat, rules) = prepared();
        source(
            &mut cat,
            "first",
            "cost=1 enable_automatic_bypass=no bypass={always=yes} bypass_effect={add_political_power=2}",
        );
        day(&mut s, &cat, &rules);
        assert!(
            !s.research.countries["A"]
                .context
                .completed_focuses
                .contains("first")
        );
        s.begin_focus("A", "first", &cat, &rules).unwrap();
        assert_eq!(s.campaign.countries["A"].power, Some(102.));
    }
    #[test]
    fn invalidity_cancel_pause_continue_and_cancel_effects() {
        for (flags, expected_progress, expected_active) in [
            ("", 0., false),
            ("cancel_if_invalid=no", 0., true),
            ("cancel_if_invalid=no continue_if_invalid=yes", 1., true),
        ] {
            let (mut s, mut cat, rules) = prepared();
            source(
                &mut cat,
                "first",
                &format!(
                    "cost=1 available={{has_country_flag=ready}} cancel_effect={{add_political_power=4}} {flags}"
                ),
            );
            s.research
                .countries
                .get_mut("A")
                .unwrap()
                .flags
                .insert("ready".into());
            s.begin_focus("A", "first", &cat, &rules).unwrap();
            s.research
                .countries
                .get_mut("A")
                .unwrap()
                .flags
                .remove("ready");
            day(&mut s, &cat, &rules);
            assert_eq!(s.campaign.focuses.active.contains_key("A"), expected_active);
            if expected_active {
                assert_eq!(s.campaign.focuses.active["A"].progress, expected_progress);
                assert_eq!(s.campaign.countries["A"].power, Some(99.));
            } else {
                assert_eq!(s.campaign.countries["A"].power, Some(103.));
            }
        }
    }
    #[test]
    fn payment_is_independent_of_speed_and_fractional_duration_rounds_down() {
        let (mut s, mut cat, rules) = prepared();
        let mut db = (*s.military.catalogue).clone();
        db.literal_defines
            .insert("NDefines.NFocus.FOCUS_PROGRESS_PEACE".into(), 2.);
        s.military = Warfare::new(db);
        source(&mut cat, "first", "cost=1.5");
        s.begin_focus("A", "first", &cat, &rules).unwrap();
        assert_eq!(s.campaign.focuses.active["A"].required, 10.);
        day(&mut s, &cat, &rules);
        assert_eq!(s.campaign.focuses.active["A"].progress, 2.);
        assert_eq!(s.campaign.countries["A"].power, Some(99.));
    }
    #[test]
    fn bypass_reward_and_failed_selection_are_transactional() {
        let (mut s, mut cat, rules) = prepared();
        source(
            &mut cat,
            "first",
            "cost=1 bypass={always=yes} bypass_effect={add_political_power=2 ROOT={unsupported=yes}}",
        );
        let before = serde_json::to_value(&s.campaign).unwrap();
        assert!(s.bypass_focus("A", "first", &cat, &rules).is_err());
        assert_eq!(serde_json::to_value(&s.campaign).unwrap(), before);
        source(
            &mut cat,
            "first",
            "cost=1 select_effect={add_political_power=2 divide_variable={var=zero value=0}}",
        );
        assert!(s.begin_focus("A", "first", &cat, &rules).is_err());
        assert_eq!(serde_json::to_value(&s.campaign).unwrap(), before);
    }
    #[test]
    fn saved_session_resumes_reward_once_and_reward_observes_uncompleted_focus() {
        let (mut s, mut cat, rules) = prepared();
        source(
            &mut cat,
            "first",
            "cost=1 completion_reward={if={limit={NOT={has_completed_focus=first}} add_political_power=5}}",
        );
        s.begin_focus("A", "first", &cat, &rules).unwrap();
        for _ in 0..3 {
            day(&mut s, &cat, &rules);
        }
        // Runtime catalogue is intentionally not serialized; production save/load
        // reconnects it to the installation, as this fixture does here.
        let db = s.military.catalogue.clone();
        s = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        s.military.catalogue = db;
        for _ in 0..10 {
            day(&mut s, &cat, &rules);
        }
        assert_eq!(s.campaign.countries["A"].power, Some(98.));
        assert!(
            s.research.countries["A"]
                .context
                .completed_focuses
                .contains("first")
        );
    }
    #[test]
    fn industrial_focus_changes_real_building_capacity() {
        let (mut s, mut cat, rules) = prepared();
        source(
            &mut cat,
            "first",
            "cost=10 completion_reward={1={add_building_construction={type=industrial_complex level=1 instant_build=yes}}}",
        );
        let before = s.world.states[&1].economy.civilian_factories;
        s.begin_focus("A", "first", &cat, &rules).unwrap();
        for _ in 0..70 {
            day(&mut s, &cat, &rules);
        }
        assert_eq!(s.world.states[&1].economy.civilian_factories, before + 1);
        assert!(
            s.research.countries["A"]
                .context
                .completed_focuses
                .contains("first")
        );
    }
    #[test]
    fn tree_load_and_layout_refresh_use_campaign_transactions() {
        let (mut s, mut cat, rules) = prepared();
        source(
            &mut cat,
            "first",
            "cost=1 allow_branch={has_country_flag=branch}",
        );
        s.refresh_focus_layout("A", &cat, &rules).unwrap();
        assert!(!s.campaign.focuses.layouts["A"]["first"]);
        s.research
            .countries
            .get_mut("A")
            .unwrap()
            .flags
            .insert("branch".into());
        s.refresh_focus_layout("A", &cat, &rules).unwrap();
        assert!(!s.campaign.focuses.layouts["A"]["first"]);
        s.campaign_effects(
            &Context::country("A"),
            &pdx::parse_pdx("mark_focus_tree_layout_dirty=yes").unwrap(),
            "test",
            &cat,
            &rules,
        )
        .unwrap();
        s.refresh_focus_layout("A", &cat, &rules).unwrap();
        assert!(s.campaign.focuses.layouts["A"]["first"]);
        cat.scripts.focus_trees.insert(
            "new".into(),
            FocusTreeRecord {
                id: "new".into(),
                ..Default::default()
            },
        );
        s.research
            .countries
            .get_mut("A")
            .unwrap()
            .context
            .completed_focuses
            .insert("first".into());
        s.campaign_effects(
            &Context::country("A"),
            &pdx::parse_pdx("load_focus_tree={tree=new keep_completed=yes}").unwrap(),
            "test",
            &cat,
            &rules,
        )
        .unwrap();
        assert_eq!(s.focus_tree("A", &cat).unwrap().id, "new");
        assert!(
            s.research.countries["A"]
                .context
                .completed_focuses
                .contains("first")
        );
    }
    #[test]
    fn focus_event_option_unlocks_next_focus_in_real_campaign_queue() {
        let (mut s, mut cat, rules) = prepared();
        let raw=pdx::parse_pdx("id=authored.1 is_triggered_only=yes immediate={set_country_flag=event_fired} option={name=choose add_political_power=3 set_country_flag=unlock}").unwrap();
        cat.scripts.sources.insert(
            "event:authored.1:immediate".into(),
            ScriptSource {
                kind: "event".into(),
                id: "authored.1".into(),
                source: "authored/event:1".into(),
                conditions: vec![],
                effects: vec![],
                raw,
            },
        );
        source(
            &mut cat,
            "first",
            "cost=1 completion_reward={country_event={id=authored.1}} ",
        );
        source(
            &mut cat,
            "second",
            "cost=1 prerequisite={focus=first} available={has_country_flag=unlock}",
        );
        s.begin_focus("A", "first", &cat, &rules).unwrap();
        for _ in 0..7 {
            day(&mut s, &cat, &rules);
        }
        assert!(s.begin_focus("A", "second", &cat, &rules).is_err());
        s.clock.elapsed_hours += 1;
        s.tick_campaign(&cat, &rules);
        assert!(s.research.countries["A"].flags.contains("event_fired"));
        let pending = *s.campaign.pending.keys().next().unwrap();
        s.choose_campaign_event(pending, 0, &cat, &rules).unwrap();
        assert_eq!(s.campaign.countries["A"].power, Some(96.));
        s.begin_focus("A", "second", &cat, &rules).unwrap();
        assert!(s.choose_campaign_event(pending, 0, &cat, &rules).is_err());
    }
    #[test]
    fn bypass_effect_may_invalidate_or_switch_tree_mid_chain_without_stale_layout() {
        for effect in [
            "mark_focus_tree_layout_dirty=yes",
            "load_focus_tree={tree=new keep_completed=yes}",
        ] {
            let (mut s, mut cat, rules) = prepared();
            source(
                &mut cat,
                "first",
                &format!("cost=1 bypass={{always=yes}} bypass_effect={{{effect}}}"),
            );
            source(
                &mut cat,
                "second",
                "cost=1 prerequisite={focus=first} bypass={always=yes}",
            );
            cat.scripts.focus_trees.insert(
                "new".into(),
                FocusTreeRecord {
                    id: "new".into(),
                    focuses: BTreeSet::from(["second".into()]),
                    ..Default::default()
                },
            );
            day(&mut s, &cat, &rules);
            assert!(
                s.research.countries["A"]
                    .context
                    .completed_focuses
                    .contains("first")
            );
            assert!(
                s.research.countries["A"]
                    .context
                    .completed_focuses
                    .contains("second"),
                "{effect}: {:?}",
                s.campaign.focuses.failures
            );
        }
    }
    #[test]
    fn forced_completion_runs_reward_before_history_once_and_stops_matching_job() {
        let (mut s, mut cat, rules) = prepared();
        source(
            &mut cat,
            "first",
            "cost=1 select_effect={add_political_power=2} completion_reward={if={limit={NOT={has_completed_focus=first}} add_political_power=5}}",
        );
        s.begin_focus("A", "first", &cat, &rules).unwrap();
        day(&mut s, &cat, &rules);
        let script =
            pdx::parse_pdx("complete_national_focus={focus=first use_side_message=yes}").unwrap();
        s.campaign_effects(&Context::country("A"), &script, "authored", &cat, &rules)
            .unwrap();
        assert_eq!(s.campaign.countries["A"].power, Some(106.));
        assert!(!s.campaign.focuses.active.contains_key("A"));
        s.campaign_effects(&Context::country("A"), &script, "authored", &cat, &rules)
            .unwrap();
        assert_eq!(s.campaign.countries["A"].power, Some(106.));
    }
    #[test]
    fn nested_completion_failure_and_recursion_rollback_whole_transaction() {
        let (mut s, mut cat, rules) = prepared();
        source(
            &mut cat,
            "first",
            "cost=1 completion_reward={add_political_power=4 add_equipment_to_stockpile={type=missing amount=1}}",
        );
        let before = serde_json::to_value(&s.campaign).unwrap();
        let b = pdx::parse_pdx("add_political_power=1 complete_national_focus=first").unwrap();
        assert!(
            s.campaign_effects(&Context::country("A"), &b, "authored", &cat, &rules)
                .is_err()
        );
        assert_eq!(serde_json::to_value(&s.campaign).unwrap(), before);
        assert!(
            !s.research.countries["A"]
                .context
                .completed_focuses
                .contains("first")
        );
        source(
            &mut cat,
            "first",
            "cost=1 completion_reward={complete_national_focus=second}",
        );
        source(
            &mut cat,
            "second",
            "cost=1 completion_reward={complete_national_focus=first}",
        );
        assert!(
            s.campaign_effects(&Context::country("A"), &b, "authored", &cat, &rules)
                .unwrap_err()
                .contains("Recursive")
        );
        assert_eq!(serde_json::to_value(&s.campaign).unwrap(), before);
    }
    #[test]
    fn native_focus_triggers_and_copy_progress_preserve_source_country() {
        let (mut s, cat, rules) = prepared();
        s.begin_focus("A", "first", &cat, &rules).unwrap();
        day(&mut s, &cat, &rules);
        let ctx = Context::country("A");
        assert!(
            s.campaign_conditions(
                &ctx,
                &pdx::parse_pdx("has_focus_tree=test focus_progress={focus=first progress>0.1}")
                    .unwrap(),
                &cat,
                &rules
            )
            .unwrap()
        );
        let b = s.research.country_mut("B", 1, &rules);
        b.context
            .completed_focuses
            .extend(["second".into(), "foreign".into()]);
        s.campaign
            .focuses
            .bypassed
            .insert("B".into(), BTreeSet::from(["second".into()]));
        let mut ctx = ctx;
        ctx.from = Some(crate::campaign::Scope::Country("B".into()));
        s.campaign_effects(
            &ctx,
            &pdx::parse_pdx("load_focus_tree={tree=test copy_completed_from=FROM}").unwrap(),
            "authored",
            &cat,
            &rules,
        )
        .unwrap();
        assert!(
            s.research.countries["A"]
                .context
                .completed_focuses
                .contains("second")
        );
        assert!(
            !s.research.countries["A"]
                .context
                .completed_focuses
                .contains("foreign")
        );
        assert!(s.campaign.focuses.bypassed["A"].contains("second"));
        assert!(
            s.research.countries["B"]
                .context
                .completed_focuses
                .contains("foreign")
        );
    }
}
