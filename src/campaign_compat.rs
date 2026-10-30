//! Conservative static inventory. Structural candidates are not execution/parity proof.
use crate::{decision_data, focus_compat, pdx, research::ResearchCatalogue, script_validation};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub fn periodic_hook(key: &str) -> bool {
    ["on_daily", "on_weekly", "on_monthly"].iter().any(|base| {
        key == *base
            || key
                .strip_prefix(base)
                .is_some_and(|suffix| suffix.starts_with('_') && suffix.len() > 1)
    })
}
pub fn bound_hook(key: &str) -> bool {
    key == "on_startup" || periodic_hook(key)
}
fn effects(raw: &[pdx::Entry], cat: &ResearchCatalogue, blockers: &mut BTreeSet<String>) {
    if let Err(e) = script_validation::preflight(raw, cat) {
        blockers.insert(e);
    }
    focus_compat::effect_conditions(raw, cat, blockers, &mut BTreeSet::new(), 0);
    focus_compat::dependencies(raw, cat, blockers, &mut BTreeSet::new(), 0);
}
pub fn report(cat: &ResearchCatalogue) -> Value {
    let mut decisions = Vec::new();
    for (id, record) in &cat.scripts.decisions {
        let mut blockers = BTreeSet::new();
        if let Err(e) = decision_data::definition(cat, id) {
            blockers.insert(e);
        }
        for key in decision_data::CONDITIONS {
            focus_compat::predicates(
                pdx::block(&record.raw, key).unwrap_or(&[]),
                cat,
                &mut blockers,
                0,
            );
        }
        if let Some(category) = cat.scripts.decision_categories.get(&record.category) {
            for key in ["allowed", "visible", "available", "cancel_trigger"] {
                focus_compat::predicates(
                    pdx::block(&category.raw, key).unwrap_or(&[]),
                    cat,
                    &mut blockers,
                    0,
                );
            }
        }
        for key in decision_data::EFFECTS {
            effects(
                pdx::block(&record.raw, key).unwrap_or(&[]),
                cat,
                &mut blockers,
            );
        }
        decisions.push(json!({"id":id,"category":record.category,"source":record.source,"mission":pdx::atom(&record.raw,"days_mission_timeout").is_some(),"structural_candidate":blockers.is_empty(),"blockers":blockers,"supported":null,"tested":false,"parity_verified":false}));
    }
    let mut hooks = Vec::new();
    for (id, blocks) in &cat.scripts.on_actions {
        for (index, block) in blocks.iter().enumerate() {
            let mut blockers = BTreeSet::new();
            if !bound_hook(id) {
                blockers.insert("Hook is imported but not bound to a simulation transition".into());
            }
            for e in &block.raw {
                match e.key.as_str() {
                    "effect" => {
                        if let Some(b) = e.block() {
                            effects(b, cat, &mut blockers);
                        } else {
                            blockers.insert("effect needs a block".into());
                        }
                    }
                    "random_events" => {
                        if let Some(b) = e.block() {
                            for entry in b {
                                if entry.operator != "=" || entry.key.parse::<u64>().is_err() {
                                    blockers.insert("Invalid random event weight/operator".into());
                                }
                                match entry.atom() {
                                    Some("0") => {}
                                    Some(event) => match script_validation::event(cat, event) {
                                        Ok((_, raw)) => {
                                            if let Err(e) = script_validation::check_event(&raw) {
                                                blockers.insert(e);
                                            }
                                            focus_compat::predicates(
                                                pdx::block(&raw, "trigger").unwrap_or(&[]),
                                                cat,
                                                &mut blockers,
                                                0,
                                            );
                                            // Reuse event traversal, including options/transitive dependencies.
                                            let call = pdx::Entry {
                                                key: "country_event".into(),
                                                operator: "=".into(),
                                                span: entry.span.clone(),
                                                value: Some(pdx::Value::Block(vec![pdx::Entry {
                                                    key: "id".into(),
                                                    operator: "=".into(),
                                                    span: entry.span.clone(),
                                                    value: Some(pdx::Value::Atom(event.into())),
                                                }])),
                                            };
                                            effects(
                                                std::slice::from_ref(&call),
                                                cat,
                                                &mut blockers,
                                            );
                                            if id == "on_startup" {
                                                blockers.insert("Startup random events have no country recipient scope".into());
                                            }
                                        }
                                        Err(e) => {
                                            blockers.insert(e);
                                        }
                                    },
                                    None => {
                                        blockers.insert(
                                            "Random event requires a literal event ID".into(),
                                        );
                                    }
                                }
                            }
                        } else {
                            blockers.insert("random_events needs a block".into());
                        }
                    }
                    field => {
                        blockers.insert(format!("Unsupported on-action field {field}"));
                    }
                }
            }
            hooks.push(json!({"id":id,"block":index,"source":block.source,"bound":bound_hook(id),"structural_candidate":blockers.is_empty(),"blockers":blockers,"supported":null,"tested":false,"parity_verified":false}));
        }
    }
    json!({"schema":1,"fingerprint":cat.fingerprint,"decisions":decisions,"on_actions":hooks,"original_game_comparisons":0,"parity_verified":false,"limitations":["Structural candidates still require runtime facts, source-backed numerical inputs and original-engine comparisons", "Only country decisions/missions; targeted/state/character decisions and automatic AI choice are unimplemented", "Only startup/daily/weekly/monthly hooks are bound; other simulation hooks are explicit blockers in this inventory", "Authored Rust fixtures do not establish native timing, RNG or original-DLC coverage"]})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research_script::{DecisionRecord, ScriptSource};
    #[test]
    fn audit_does_not_promote_imported_content_or_unbound_hooks_to_supported() {
        let mut cat = ResearchCatalogue::default();
        cat.scripts
            .decision_categories
            .insert("c".into(), DecisionRecord::default());
        for (id, raw) in [
            ("ok", "cost=3 complete_effect={add_political_power=1}"),
            (
                "unknown",
                "state_target=yes complete_effect={not_implemented=yes}",
            ),
        ] {
            cat.scripts.decisions.insert(
                id.into(),
                DecisionRecord {
                    category: "c".into(),
                    raw: pdx::parse_pdx(raw).unwrap(),
                    ..Default::default()
                },
            );
        }
        for id in ["on_daily", "on_declare_war"] {
            cat.scripts.on_actions.insert(
                id.into(),
                vec![ScriptSource {
                    kind: "on_action".into(),
                    id: id.into(),
                    source: "authored".into(),
                    raw: pdx::parse_pdx("effect={add_political_power=1}").unwrap(),
                    conditions: vec![],
                    effects: vec![],
                }],
            );
        }
        let r = report(&cat);
        assert_eq!(r["decisions"][0]["structural_candidate"], true);
        assert!(r["decisions"][0]["supported"].is_null());
        assert_eq!(r["decisions"][1]["structural_candidate"], false);
        assert!(
            r["on_actions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|h| h["id"] == "on_declare_war"
                    && h["bound"] == false
                    && h["structural_candidate"] == false)
        );
        assert_eq!(r["original_game_comparisons"], 0);
    }
}
