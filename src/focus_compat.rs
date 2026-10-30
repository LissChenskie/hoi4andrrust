//! Conservative, machine-readable focus compatibility evidence. Parsed/resolved,
//! structural coverage, runtime tests and original-engine parity stay distinct.
use crate::{
    focus_data,
    pdx::{self, Entry},
    research::ResearchCatalogue,
    research_import::ResearchDataset,
    script_validation,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(crate) fn predicates(
    b: &[Entry],
    cat: &ResearchCatalogue,
    out: &mut BTreeSet<String>,
    depth: usize,
) {
    walk(b, cat, out, depth, &mut 10000);
}
fn walk(
    b: &[Entry],
    cat: &ResearchCatalogue,
    out: &mut BTreeSet<String>,
    depth: usize,
    budget: &mut usize,
) {
    if depth > 64 {
        out.insert("Trigger expansion depth exceeded".into());
        return;
    }
    for e in b {
        if *budget == 0 {
            out.insert("Trigger expansion budget exceeded".into());
            return;
        }
        *budget -= 1;
        if let Some(raw) = cat.scripts.triggers.get(&e.key) {
            match script_validation::instantiate(raw, e) {
                Ok(b) => walk(&b, cat, out, depth + 1, budget),
                Err(err) if e.atom() == Some("no") => {
                    let _ = err;
                    walk(raw, cat, out, depth + 1, budget);
                }
                Err(err) => {
                    out.insert(format!("{}: {err}", e.key));
                }
            }
            continue;
        }
        match e.key.as_str() {
            "AND" | "OR" | "NOT" | "NOR" | "NAND" | "and" | "or" | "not" | "nor" | "nand"
            | "ROOT" | "THIS" | "FROM" | "PREV" | "owner" | "controller" | "if" | "else_if"
            | "else" | "limit" | "hidden_trigger" => {
                walk(e.block().unwrap_or(&[]), cat, out, depth + 1, budget)
            }
            "custom_trigger_tooltip" => walk(
                &e.block()
                    .unwrap_or(&[])
                    .iter()
                    .filter(|e| e.key != "tooltip")
                    .cloned()
                    .collect::<Vec<_>>(),
                cat,
                out,
                depth + 1,
                budget,
            ),
            "political_power"
            | "stability"
            | "war_support"
            | "has_state_flag"
            | "has_capitulated"
            | "has_political_power"
            | "has_stability"
            | "has_war_support"
            | "has_focus_tree"
            | "focus_progress"
            | "has_dynamic_modifier" => {}
            "has_decision" | "has_active_mission" => {}
            k if e.block().is_some()
                && (k.parse::<u32>().is_ok()
                    || k.len() == 3
                        && k.bytes()
                            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())) =>
            {
                walk(e.block().unwrap(), cat, out, depth + 1, budget)
            }
            _ => {
                for k in crate::research_script::unsupported_predicates(std::slice::from_ref(e)) {
                    out.insert(format!("{}:{} {k}", e.span.line, e.span.column));
                }
            }
        }
    }
}
pub(crate) fn effect_conditions(
    b: &[Entry],
    cat: &ResearchCatalogue,
    out: &mut BTreeSet<String>,
    seen: &mut BTreeSet<String>,
    depth: usize,
) {
    if depth > 64 || seen.len() > 10000 {
        out.insert("Effect dependency depth/budget exceeded".into());
        return;
    }
    for e in b {
        if let Some(raw) = cat.scripts.effects.get(&e.key) {
            if seen.insert(format!("effect:{}:{:?}", e.key, e.value)) {
                match script_validation::instantiate(raw, e) {
                    Ok(b) => effect_conditions(&b, cat, out, seen, depth + 1),
                    Err(err) => {
                        out.insert(err);
                    }
                }
            }
        } else if e.key == "add_dynamic_modifier" {
            if let Some(id) = e.block().and_then(|b| pdx::atom(b, "modifier")) {
                if seen.insert(format!("dynamic:{id}")) {
                    if let Some(raw) = cat.scripts.dynamic_modifiers.get(id) {
                        for key in ["enable", "remove_trigger"] {
                            predicates(pdx::block(raw, key).unwrap_or(&[]), cat, out, 0);
                        }
                    }
                }
            }
        } else if e.key == "activate_mission" {
            if let Some(id) = e.atom() {
                if seen.insert(format!("decision:{id}")) {
                    if let Some(record) = cat.scripts.decisions.get(id) {
                        for key in crate::decision_data::CONDITIONS {
                            predicates(pdx::block(&record.raw, key).unwrap_or(&[]), cat, out, 0);
                        }
                        for key in crate::decision_data::EFFECTS {
                            effect_conditions(
                                pdx::block(&record.raw, key).unwrap_or(&[]),
                                cat,
                                out,
                                seen,
                                depth + 1,
                            );
                        }
                    }
                }
            }
        } else if e.key == "complete_national_focus" {
            if let Some(id) = e
                .atom()
                .or_else(|| e.block().and_then(|b| pdx::atom(b, "focus")))
            {
                if seen.insert(format!("focus:{id}")) {
                    if let Some(src) = cat.scripts.sources.get(&format!("focus:{id}")) {
                        effect_conditions(&src.effects, cat, out, seen, depth + 1);
                    }
                }
            }
        } else if e.key == "country_event" {
            if let Some(id) = e.block().and_then(|b| pdx::atom(b, "id")) {
                if seen.insert(format!("event:{id}")) {
                    if let Ok((_, raw)) = script_validation::event(cat, id) {
                        predicates(pdx::block(&raw, "trigger").unwrap_or(&[]), cat, out, 0);
                        effect_conditions(
                            pdx::block(&raw, "immediate").unwrap_or(&[]),
                            cat,
                            out,
                            seen,
                            depth + 1,
                        );
                        for option in raw
                            .iter()
                            .filter(|e| e.key == "option")
                            .filter_map(Entry::block)
                        {
                            for k in ["trigger", "available"] {
                                predicates(pdx::block(option, k).unwrap_or(&[]), cat, out, 0);
                            }
                            effect_conditions(
                                &option
                                    .iter()
                                    .filter(|e| {
                                        !["name", "trigger", "available", "ai_chance"]
                                            .contains(&e.key.as_str())
                                    })
                                    .cloned()
                                    .collect::<Vec<_>>(),
                                cat,
                                out,
                                seen,
                                depth + 1,
                            );
                        }
                    }
                }
            }
        } else if let Some(b) = e.block() {
            // Trigger bodies are not effects; effect parameter records do not
            // introduce scope. Only known control-flow containers are descended.
            if [
                "if",
                "else_if",
                "every_country",
                "every_owned_state",
                "random_owned_state",
            ]
            .contains(&e.key.as_str())
            {
                predicates(pdx::block(b, "limit").unwrap_or(&[]), cat, out, 0);
                effect_conditions(
                    &b.iter()
                        .filter(|e| e.key != "limit")
                        .cloned()
                        .collect::<Vec<_>>(),
                    cat,
                    out,
                    seen,
                    depth + 1,
                );
            } else if [
                "else",
                "hidden_effect",
                "ROOT",
                "THIS",
                "FROM",
                "PREV",
                "owner",
                "controller",
            ]
            .contains(&e.key.as_str())
                || e.key.parse::<u32>().is_ok()
                || e.key.len() == 3
            {
                effect_conditions(b, cat, out, seen, depth + 1);
            } else if e.key == "random_list" {
                for child in b.iter().filter_map(Entry::block) {
                    effect_conditions(child, cat, out, seen, depth + 1);
                }
            }
        }
    }
}
pub(crate) fn dependencies(
    b: &[Entry],
    cat: &ResearchCatalogue,
    missing: &mut BTreeSet<String>,
    seen: &mut BTreeSet<String>,
    depth: usize,
) {
    if depth > 64 || seen.len() > 10000 {
        missing.insert("Dependency traversal depth/budget exceeded".into());
        return;
    }
    for e in b {
        if matches!(
            e.key.as_str(),
            "add_dynamic_modifier" | "has_dynamic_modifier"
        ) {
            match e
                .atom()
                .or_else(|| e.block().and_then(|b| pdx::atom(b, "modifier")))
            {
                Some(id) if cat.scripts.dynamic_modifiers.contains_key(id) => {}
                id => {
                    missing.insert(format!("dynamic_modifier:{}", id.unwrap_or("<dynamic>")));
                }
            }
        } else if [
            "activate_mission",
            "remove_mission",
            "remove_decision",
            "has_decision",
            "has_active_mission",
        ]
        .contains(&e.key.as_str())
        {
            if let Some(id) = e.atom() {
                if seen.insert(format!("decision:{id}")) {
                    match cat.scripts.decisions.get(id) {
                        Some(record) => dependencies(&record.raw, cat, missing, seen, depth + 1),
                        None => {
                            missing.insert(format!("decision:{id}"));
                        }
                    }
                }
            } else {
                missing.insert(format!("{}: unresolved decision ID", e.key));
            }
        } else if e.key == "complete_national_focus" {
            if let Some(id) = e
                .atom()
                .or_else(|| e.block().and_then(|b| pdx::atom(b, "focus")))
            {
                if seen.insert(format!("focus:{id}")) {
                    match cat.scripts.sources.get(&format!("focus:{id}")) {
                        Some(src) => dependencies(&src.raw, cat, missing, seen, depth + 1),
                        None => {
                            missing.insert(format!("focus:{id}"));
                        }
                    }
                }
            } else {
                missing.insert("complete_national_focus: unresolved ID".into());
            }
        } else if e.key == "country_event" {
            let id = e
                .atom()
                .or_else(|| e.block().and_then(|b| pdx::atom(b, "id")));
            if let Some(id) = id {
                if seen.insert(format!("event:{id}")) {
                    match script_validation::event(cat, id) {
                        Ok((_, raw)) => dependencies(&raw, cat, missing, seen, depth + 1),
                        Err(_) => {
                            missing.insert(format!("event:{id}"));
                        }
                    }
                }
            } else {
                missing.insert("country_event: unresolved ID".into());
            }
        } else if e.key == "load_focus_tree" {
            match e
                .atom()
                .or_else(|| e.block().and_then(|b| pdx::atom(b, "tree")))
            {
                Some(id) if cat.scripts.focus_trees.contains_key(id) => {}
                id => {
                    missing.insert(format!("tree:{}", id.unwrap_or("<dynamic>")));
                }
            }
        } else if matches!(e.key.as_str(), "has_tech" | "has_technology") {
            if let Some(id) = e.atom() {
                if !cat.technologies.contains_key(id) {
                    missing.insert(format!("technology:{id}"));
                }
            }
        }
        if let Some(raw) = cat.scripts.effects.get(&e.key) {
            // Arguments are part of the visited identity; distinct parameter
            // values must not conceal different transitive event dependencies.
            let key = format!("{}:{:?}", e.key, e.value);
            if seen.insert(key) {
                match script_validation::instantiate(raw, e) {
                    Ok(b) => dependencies(&b, cat, missing, seen, depth + 1),
                    Err(err) => {
                        missing.insert(format!("{}: {err}", e.key));
                    }
                }
            }
        }
        if let Some(b) = e.block() {
            dependencies(b, cat, missing, seen, depth + 1);
        }
    }
}
pub fn report(data: &ResearchDataset) -> Value {
    let cat = &data.registry;
    let mut nodes = Vec::new();
    for src in cat.scripts.sources.values().filter(|s| s.kind == "focus") {
        let trees: Vec<_> = cat
            .scripts
            .focus_trees
            .values()
            .filter(|t| t.focuses.contains(&src.id))
            .collect();
        let mut missing = BTreeSet::new();
        for dep in focus_data::references(&src.raw, "prerequisite")
            .chain(focus_data::references(&src.raw, "mutually_exclusive"))
            .chain(pdx::atom(&src.raw, "relative_position_id"))
        {
            if !cat.scripts.sources.contains_key(&format!("focus:{dep}")) {
                missing.insert(dep.to_owned());
            }
        }
        dependencies(&src.raw, cat, &mut missing, &mut BTreeSet::new(), 0);
        if trees.is_empty() {
            missing
                .insert("Focus not attached to a tree (shared library node may be unused)".into());
        }
        for t in &trees {
            missing.extend(t.problems.iter().cloned());
        }
        let mut unknown = BTreeSet::new();
        for key in ["available", "visible", "bypass", "allow_branch", "cancel"] {
            predicates(
                pdx::block(&src.raw, key).unwrap_or(&[]),
                cat,
                &mut unknown,
                0,
            );
        }
        for t in &trees {
            for e in t
                .country
                .iter()
                .filter(|e| e.key == "modifier")
                .filter_map(Entry::block)
            {
                predicates(
                    &e.iter()
                        .filter(|e| !["add", "factor"].contains(&e.key.as_str()))
                        .cloned()
                        .collect::<Vec<_>>(),
                    cat,
                    &mut unknown,
                    0,
                );
            }
        }
        let mut effects = BTreeSet::new();
        for key in [
            "completion_reward",
            "select_effect",
            "bypass_effect",
            "cancel_effect",
        ] {
            let b = pdx::block(&src.raw, key).unwrap_or(&[]);
            // Probe each sibling too, so the first unsupported command does not
            // conceal independent missing dependencies in the same reward.
            if let Err(e) = script_validation::preflight(b, cat) {
                effects.insert(format!("{key}: {e}"));
            }
            for e in b {
                if let Err(err) = script_validation::preflight(std::slice::from_ref(e), cat) {
                    effects.insert(format!("{key}: {err}"));
                }
            }
            effect_conditions(b, cat, &mut unknown, &mut BTreeSet::new(), 0);
        }
        let fields = focus_data::unsupported_focus_fields(&src.raw);
        let resolved = missing.is_empty();
        let structural = resolved && effects.is_empty() && unknown.is_empty() && fields.is_empty();
        let text = pdx::atom(&src.raw, "text").unwrap_or(&src.id);
        let locale_missing: Vec<_> = [text.to_owned(), format!("{text}_desc")]
            .into_iter()
            .filter(|k| !cat.scripts.localisation.contains_key(k))
            .collect();
        nodes.push(json!({"id":src.id,"source":src.source,"trees":trees.iter().map(|t|&t.id).collect::<Vec<_>>(),
            "parsed":true,"resolved":resolved,"structural_coverage":structural,
            "supported":null,"tested":false,"parity_verified":false,
            "status":if !resolved{"parsed"}else{"resolved"},
            "missing_dependencies":missing,"unknown_or_dynamic_predicates":unknown,"effect_blockers":effects,
            "unsupported_fields":fields,"missing_localisation":locale_missing,
            "runtime_validation":"Required: current scopes, DLC facts, political state, modifiers and effect consumers"}));
    }
    let resolved = nodes.iter().filter(|v| v["resolved"] == true).count();
    let structural = nodes
        .iter()
        .filter(|v| v["structural_coverage"] == true)
        .count();
    json!({"schema":1,"installation":data.installation,"fingerprint":cat.fingerprint,
        "counts":{"trees":cat.scripts.focus_trees.len(),"focuses":nodes.len(),"shared_focuses":cat.scripts.shared_focuses.len(),"resolved":resolved,"structural_candidates":structural,"tested_original_focuses":0,"parity_verified":0},
        "focuses":nodes,"duplicate_ids":cat.scripts.focus_problems,"failed_source_files":data.failed_source_files,
        "eof_terminated_sources":data.eof_terminated_sources,
        "continuous_focuses":cat.scripts.sources.values().filter(|s|s.kind=="continuous_focus").map(|s|json!({"id":s.id,"source":s.source,"parsed":true,"supported":false,"tested":false,"parity_verified":false,"blocker":"Continuous focus lifecycle and modifier removal not implemented"})).collect::<Vec<_>>(),
        "tree_diagnostics":cat.scripts.focus_trees.iter().map(|(id,t)|(id,&t.problems)).collect::<std::collections::BTreeMap<_,_>>(),
        "limitations":["supported=null means runtime evidence unavailable, not full support", "Authored unit tests are not tests of imported proprietary focuses", "Continuous/joint focuses, targeted decisions and DLC-specific mechanics need additional runtime implementations", "See campaign-script-compatibility-report.json for country decision/missions and bound/unbound on-actions", "Dynamic localisation and source speed modifiers remain unverified"]})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recursive_trigger_expansion_is_bounded() {
        let mut cat = ResearchCatalogue::default();
        cat.scripts.triggers.insert(
            "loop_trigger".into(),
            pdx::parse_pdx("loop_trigger=yes loop_trigger=yes").unwrap(),
        );
        let mut errors = BTreeSet::new();
        predicates(
            &pdx::parse_pdx("loop_trigger=yes").unwrap(),
            &cat,
            &mut errors,
            0,
        );
        assert!(errors.iter().any(|e| e.contains("budget")));
    }
}
