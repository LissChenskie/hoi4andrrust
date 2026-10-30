//! Native country decision definitions shared by runtime and compatibility checks.
use crate::{pdx::Entry, research::ResearchCatalogue, research_script::DecisionRecord};
use std::collections::BTreeSet;

pub const CONDITIONS: &[&str] = &[
    "allowed",
    "visible",
    "available",
    "activation",
    "cancel_trigger",
    "remove_trigger",
    "custom_cost_trigger",
];
pub const EFFECTS: &[&str] = &[
    "complete_effect",
    "remove_effect",
    "cancel_effect",
    "timeout_effect",
];

pub fn definition<'a>(cat: &'a ResearchCatalogue, id: &str) -> Result<&'a DecisionRecord, String> {
    let record = cat
        .scripts
        .decisions
        .get(id)
        .ok_or_else(|| format!("Missing decision {id}"))?;
    if !record.problems.is_empty() {
        return Err(record.problems.join("; "));
    }
    check(&record.raw, false)?;
    let category = category(cat, &record.category)?;
    check(&category.raw, true)?;
    Ok(record)
}
pub fn category<'a>(cat: &'a ResearchCatalogue, id: &str) -> Result<&'a DecisionRecord, String> {
    let record = cat
        .scripts
        .decision_categories
        .get(id)
        .ok_or_else(|| format!("Missing decision category {id}"))?;
    if !record.problems.is_empty() {
        return Err(record.problems.join("; "));
    }
    check(&record.raw, true)?;
    Ok(record)
}
fn check(raw: &[Entry], category: bool) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for e in raw {
        if e.operator != "=" || (!seen.insert(e.key.as_str()) && e.key != "icon") {
            return Err(format!("Duplicate/invalid decision field {}", e.key));
        }
        let k = e.key.as_str();
        if CONDITIONS.contains(&k) || EFFECTS.contains(&k) || k == "modifier" {
            if category && !["allowed", "visible", "available", "cancel_trigger"].contains(&k) {
                return Err(format!("Unsupported category field {k}"));
            }
            let b = e.block().ok_or_else(|| format!("{k} needs a block"))?;
            if k == "modifier" {
                for v in b {
                    if v.operator != "="
                        || v.atom().is_none()
                        || crate::research::modifier_consumer(&v.key).is_none()
                    {
                        return Err(format!("Unsupported decision modifier consumer {}", v.key));
                    }
                }
            }
        } else if [
            "icon",
            "name",
            "desc",
            "priority",
            "picture",
            "visible_when_empty",
            "ai_will_do",
            "ai_hint_pp_cost",
            "custom_cost_text",
            "highlight_states",
            "highlight_color_before_active",
            "highlight_color_while_active",
            "on_map_mode",
            "cosmetic_tag",
            "cosmetic_ideology",
            "is_good",
        ]
        .contains(&k)
        {
            // Presentation/AI metadata is retained. Automatic AI selection is not enabled.
        } else if !category
            && [
                "cost",
                "days_remove",
                "days_mission_timeout",
                "days_re_enable",
            ]
            .contains(&k)
        {
            if e.atom().is_none() {
                return Err(format!("{k} needs a scalar"));
            }
        } else if !category
            && [
                "fire_only_once",
                "cancel_if_not_visible",
                "fixed_random_seed",
                "selectable_mission",
            ]
            .contains(&k)
        {
            if !matches!(e.atom(), Some("yes" | "no")) {
                return Err(format!("Invalid decision flag {k}"));
            }
        } else if !category && k == "state_target" && e.atom() == Some("no") {
        } else {
            return Err(format!(
                "Unsupported decision {}field {k}",
                if category { "category " } else { "" }
            ));
        }
    }
    Ok(())
}
