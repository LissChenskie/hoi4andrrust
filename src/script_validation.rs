//! Shared structural effect checks for runtime and the local compatibility audit.
//! Structural acceptance is not runtime or native-game behavioral verification.
use crate::{
    pdx::{self, Entry, Value},
    research::ResearchCatalogue,
};
use std::collections::{BTreeMap, BTreeSet};
const STEPS: usize = 10000;
pub fn body(e: &Entry) -> Result<&[Entry], String> {
    e.block().ok_or_else(|| format!("{} needs a block", e.key))
}
pub fn atom(e: &Entry) -> Result<&str, String> {
    e.atom().ok_or_else(|| format!("{} needs an atom", e.key))
}
pub fn field<'a>(b: &'a [Entry], k: &str) -> Result<&'a str, String> {
    pdx::atom(b, k).ok_or_else(|| format!("Missing {k}"))
}
pub fn fields(e: &Entry, allowed: &[&str]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for f in body(e)? {
        if !allowed.contains(&f.key.as_str()) || !seen.insert(&f.key) || f.operator != "=" {
            return Err(format!("Unsupported/duplicate field {}", f.key));
        }
    }
    Ok(())
}
pub fn instantiate(raw: &[Entry], call: &Entry) -> Result<Vec<Entry>, String> {
    let mut args = BTreeMap::new();
    if let Some(b) = call.block() {
        for e in b {
            if e.operator != "=" || args.insert(e.key.clone(), atom(e)?.to_string()).is_some() {
                return Err("Invalid script parameters".into());
            }
        }
    } else if call.atom() != Some("yes") {
        return Err("Expected scripted effect=yes or parameter block".into());
    }
    fn sub(s: &str, a: &BTreeMap<String, String>) -> Result<String, String> {
        let mut r = s;
        let mut o = String::new();
        while let Some(i) = r.find('$') {
            o.push_str(&r[..i]);
            r = &r[i + 1..];
            let j = r.find('$').ok_or("Unclosed parameter")?;
            o.push_str(a.get(&r[..j]).ok_or("Missing script parameter")?);
            r = &r[j + 1..];
            if o.len() > 65536 {
                return Err("Parameter expansion limit".into());
            }
        }
        o.push_str(r);
        Ok(o)
    }
    fn walk(b: &[Entry], a: &BTreeMap<String, String>, d: usize) -> Result<Vec<Entry>, String> {
        if d > 64 {
            return Err("Parameter nesting limit".into());
        }
        b.iter()
            .map(|e| {
                let mut x = e.clone();
                x.key = sub(&e.key, a)?;
                x.value = match &e.value {
                    Some(Value::Atom(s)) => Some(Value::Atom(sub(s, a)?)),
                    Some(Value::Block(b)) => Some(Value::Block(walk(b, a, d + 1)?)),
                    None => None,
                };
                Ok(x)
            })
            .collect()
    }
    walk(raw, &args, 0)
}
pub fn event(cat: &ResearchCatalogue, id: &str) -> Result<(String, Vec<Entry>), String> {
    cat.scripts
        .sources
        .values()
        .find(|s| s.kind == "event" && pdx::atom(&s.raw, "id") == Some(id))
        .map(|s| (s.source.clone(), s.raw.clone()))
        .ok_or_else(|| format!("Unknown event {id}"))
}
pub fn check_event(raw: &[Entry]) -> Result<(), String> {
    for e in raw {
        if ![
            "id",
            "title",
            "desc",
            "picture",
            "is_triggered_only",
            "fire_only_once",
            "hidden",
            "trigger",
            "immediate",
            "option",
        ]
        .contains(&e.key.as_str())
        {
            return Err(format!("Unsupported event semantics {}", e.key));
        }
    }
    if pdx::atom(raw, "hidden") == Some("yes") && raw.iter().any(|e| e.key == "option") {
        return Err("Hidden event automatic choices unsupported".into());
    }
    Ok(())
}
/// Checks every effect branch, including events reached indirectly. Successful
/// structural coverage still requires runtime conditions/data to be validated.
pub fn preflight(b: &[Entry], cat: &ResearchCatalogue) -> Result<(), String> {
    fn walk(
        b: &[Entry],
        cat: &ResearchCatalogue,
        seen: &mut BTreeSet<String>,
        budget: &mut usize,
        d: usize,
    ) -> Result<(), String> {
        if d > 64 {
            return Err("Script nesting limit".into());
        }
        for e in b {
            *budget = budget.checked_sub(1).ok_or("Script preflight budget")?;
            if e.operator != "=" {
                return Err("Effect operator must be =".into());
            }
            let r = (|| -> Result<(), String> {
                match e.key.as_str() {
                    "if" | "else_if" | "every_country" | "every_owned_state"
                    | "random_owned_state" => {
                        let x: Vec<_> = body(e)?
                            .iter()
                            .filter(|f| f.key != "limit")
                            .cloned()
                            .collect();
                        walk(&x, cat, seen, budget, d + 1)
                    }
                    "else" | "hidden_effect" | "ROOT" | "THIS" | "FROM" | "PREV" | "owner"
                    | "controller" => walk(body(e)?, cat, seen, budget, d + 1),
                    "random_list" => {
                        for branch in body(e)? {
                            let n = branch.key.parse::<f64>().map_err(|_| "Invalid weight")?;
                            if !n.is_finite() || n <= 0. {
                                return Err("Invalid weight".into());
                            }
                            walk(body(branch)?, cat, seen, budget, d + 1)?
                        }
                        Ok(())
                    }
                    "country_event" => {
                        fields(e, &["id", "days", "hours"])?;
                        let id = field(body(e)?, "id")?;
                        if seen.insert(format!("event:{id}")) {
                            let (_, raw) = event(cat, id)?;
                            check_event(&raw)?;
                            walk(
                                pdx::block(&raw, "immediate").unwrap_or(&[]),
                                cat,
                                seen,
                                budget,
                                d + 1,
                            )?;
                            for o in raw.iter().filter(|e| e.key == "option") {
                                let x: Vec<_> = body(o)?
                                    .iter()
                                    .filter(|e| {
                                        !["name", "trigger", "available", "ai_chance"]
                                            .contains(&e.key.as_str())
                                    })
                                    .cloned()
                                    .collect();
                                walk(&x, cat, seen, budget, d + 1)?
                            }
                        }
                        Ok(())
                    }
                    "mark_focus_tree_layout_dirty" => {
                        if atom(e)? != "yes" {
                            return Err("Layout invalidation requires yes".into());
                        }
                        Ok(())
                    }
                    "complete_national_focus" => {
                        let id = if let Some(id) = e.atom() {
                            id
                        } else {
                            fields(e, &["focus", "use_side_message", "originator_name"])?;
                            field(body(e)?, "focus")?
                        };
                        if let Some(value) =
                            e.block().and_then(|b| pdx::atom(b, "use_side_message"))
                        {
                            if !["yes", "no"].contains(&value) {
                                return Err("use_side_message requires bool".into());
                            }
                        }
                        let src = cat
                            .scripts
                            .sources
                            .get(&format!("focus:{id}"))
                            .ok_or_else(|| format!("Missing focus {id}"))?;
                        if src
                            .raw
                            .iter()
                            .any(|e| e.key.starts_with("completion_reward_joint_"))
                        {
                            return Err("Joint focus completion requires joint mechanics".into());
                        }
                        if seen.insert(format!("focus:{id}")) {
                            walk(&src.effects, cat, seen, budget, d + 1)?;
                        }
                        Ok(())
                    }
                    "set_politics" => fields(
                        e,
                        &["ruling_party", "elections_allowed", "name", "long_name"],
                    ),
                    "set_popularities" => {
                        body(e)?;
                        Ok(())
                    }
                    "load_focus_tree" => {
                        let id = if let Some(id) = e.atom() {
                            id
                        } else {
                            fields(e, &["tree", "keep_completed", "copy_completed_from"])?;
                            field(body(e)?, "tree")?
                        };
                        if !cat.scripts.focus_trees.contains_key(id) {
                            return Err(format!("Missing focus tree {id}"));
                        }
                        Ok(())
                    }
                    "add_manpower"
                    | "add_political_power"
                    | "add_stability"
                    | "add_war_support"
                    | "set_political_power"
                    | "set_stability"
                    | "set_war_support"
                    | "set_state_flag"
                    | "clr_state_flag"
                    | "retire_character"
                    | "set_country_flag"
                    | "clr_country_flag"
                    | "set_global_flag"
                    | "clr_global_flag" => {
                        atom(e)?;
                        Ok(())
                    }
                    "set_variable"
                    | "add_to_variable"
                    | "subtract_from_variable"
                    | "multiply_variable"
                    | "divide_variable" => fields(e, &["var", "value"]),
                    "add_equipment_to_stockpile" => fields(e, &["type", "amount"]),
                    "add_building_construction" => {
                        fields(e, &["type", "level", "instant_build"])?;
                        if pdx::atom(body(e)?, "instant_build") != Some("yes") {
                            return Err("Only instant_build=yes supported".into());
                        }
                        Ok(())
                    }
                    "add_ideas" | "remove_ideas" | "swap_ideas" | "set_technology"
                    | "add_tech_bonus" | "add_research_slot" | "set_research_slots" => Ok(()),
                    k if cat.scripts.effects.contains_key(k) => {
                        if seen.insert(format!("effect:{k}:{:?}", e.value)) {
                            walk(
                                &instantiate(&cat.scripts.effects[k], e)?,
                                cat,
                                seen,
                                budget,
                                d + 1,
                            )?
                        }
                        Ok(())
                    }
                    k if k.parse::<u32>().is_ok()
                        || (k.len() == 3
                            && k.bytes()
                                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())) =>
                    {
                        walk(body(e)?, cat, seen, budget, d + 1)
                    }
                    _ => Err(format!("Unsupported effect {}", e.key)),
                }
            })();
            r.map_err(|r| format!("{}:{} {}: {r}", e.span.line, e.span.column, e.key))?;
        }
        Ok(())
    }
    let mut budget = STEPS;
    walk(b, cat, &mut BTreeSet::new(), &mut budget, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn different_script_arguments_cannot_hide_unsupported_nested_effects() {
        let mut cat = ResearchCatalogue::default();
        cat.scripts.effects.insert(
            "dispatch".into(),
            pdx::parse_pdx("$COMMAND$=$VALUE$").unwrap(),
        );
        let b=pdx::parse_pdx("if={limit={always=yes} dispatch={COMMAND=set_country_flag VALUE=ok} dispatch={COMMAND=not_supported VALUE=yes}}").unwrap();
        assert!(
            preflight(&b, &cat)
                .unwrap_err()
                .contains("Unsupported effect not_supported")
        );
    }
}
