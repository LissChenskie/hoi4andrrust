//! Resolve dated/conditional history once, including the nested `else` syntax
//! used by country histories. Unsupported conditions never enable both branches.
use crate::game::GameDate;
use crate::land_combat::{TacticContext, evaluate_conditions};
use crate::military_model::MilitaryModifiers;
use crate::research_data::{Entry, parse_pdx};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn date(raw: &str) -> Option<GameDate> {
    let mut p = raw.split('.');
    let year = p.next()?.parse().ok()?;
    let month = p.next()?.parse().ok()?;
    let day = p.next()?.parse().ok()?;
    if p.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(GameDate { year, month, day })
}
pub fn profile_dlcs(root: &Path) -> BTreeSet<String> {
    if root.join("research-import.json").is_file() {
        return hoi4_mobile::research_import::profile_context(root).dlcs;
    }
    std::fs::read_to_string(root.join("military_profile.txt"))
        .ok()
        .and_then(|s| parse_pdx(&s).ok())
        .and_then(|e| e.into_iter().find(|e| e.key == "dlcs"))
        .and_then(|e| {
            e.block().map(|b| {
                b.iter()
                    .filter(|e| e.value.is_none())
                    .map(|e| e.key.clone())
                    .collect()
            })
        })
        .unwrap_or_default()
}

pub fn resolve(
    entries: &[Entry],
    tag: &str,
    initial: &MilitaryModifiers,
    today: GameDate,
) -> (Vec<Entry>, Vec<String>) {
    resolve_expanding(entries, tag, initial, today, &|_| Ok(None))
}

pub fn resolve_expanding(
    entries: &[Entry],
    tag: &str,
    initial: &MilitaryModifiers,
    today: GameDate,
    expand: &dyn Fn(&Entry) -> Result<Option<Vec<Entry>>, String>,
) -> (Vec<Entry>, Vec<String>) {
    fn alternatives<'a>(b: &'a [Entry], out: &mut Vec<(&'a str, &'a [Entry])>) {
        for e in b
            .iter()
            .filter(|e| e.key.eq_ignore_ascii_case("else") || e.key.eq_ignore_ascii_case("else_if"))
        {
            if let Some(b) = e.block() {
                out.push((&e.key, b));
                alternatives(b, out);
            }
        }
    }
    fn walk(
        entries: &[Entry],
        tag: &str,
        m: &mut MilitaryModifiers,
        today: GameDate,
        out: &mut Vec<Entry>,
        diagnostics: &mut Vec<String>,
        expand: &dyn Fn(&Entry) -> Result<Option<Vec<Entry>>, String>,
        depth: usize,
        budget: &mut usize,
    ) {
        if depth > 64 {
            diagnostics.push("History expansion: recursion limit".into());
            return;
        }
        let mut dated = Vec::new();
        let mut i = 0;
        while i < entries.len() {
            let e = &entries[i];
            i += 1;
            if *budget == 0 {
                diagnostics.push("History expansion: node limit".into());
                return;
            }
            *budget -= 1;
            match expand(e) {
                Ok(Some(body)) => {
                    walk(
                        &body,
                        tag,
                        m,
                        today,
                        out,
                        diagnostics,
                        expand,
                        depth + 1,
                        budget,
                    );
                    continue;
                }
                Err(error) => {
                    diagnostics.push(format!("History expansion: {error}"));
                    continue;
                }
                Ok(None) => {}
            }
            if let Some(at) = date(&e.key) {
                if at <= today {
                    dated.push((at, e));
                }
                continue;
            }
            if e.key.eq_ignore_ascii_case("if") {
                let Some(b) = e.block() else {
                    continue;
                };
                let mut chain = vec![("if", b)];
                alternatives(b, &mut chain);
                while i < entries.len()
                    && (entries[i].key.eq_ignore_ascii_case("else")
                        || entries[i].key.eq_ignore_ascii_case("else_if"))
                {
                    let e = &entries[i];
                    i += 1;
                    if let Some(b) = e.block() {
                        chain.push((&e.key, b));
                        alternatives(b, &mut chain);
                    }
                }
                for (kind, b) in chain {
                    let limit = b
                        .iter()
                        .find(|e| e.key.eq_ignore_ascii_case("limit"))
                        .and_then(Entry::block)
                        .unwrap_or(&[]);
                    let ctx = TacticContext {
                        country: tag,
                        attacker: false,
                        phase: None,
                        modifiers: m,
                        numbers: BTreeMap::new(),
                    };
                    let allowed = if kind.eq_ignore_ascii_case("else") {
                        Some(true)
                    } else {
                        evaluate_conditions(limit, &ctx)
                    };
                    match allowed {
                        Some(true) => {
                            let body: Vec<_> = b
                                .iter()
                                .filter(|e| {
                                    !(e.key.eq_ignore_ascii_case("limit")
                                        || e.key.eq_ignore_ascii_case("else")
                                        || e.key.eq_ignore_ascii_case("else_if"))
                                })
                                .cloned()
                                .collect();
                            walk(
                                &body,
                                tag,
                                m,
                                today,
                                out,
                                diagnostics,
                                expand,
                                depth + 1,
                                budget,
                            );
                            break;
                        }
                        Some(false) => {}
                        None => {
                            diagnostics.push(format!(
                                "{tag}: unsupported conditional history ({})",
                                limit
                                    .iter()
                                    .map(|e| e.key.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ));
                            break;
                        }
                    }
                }
                continue;
            }
            if e.key.eq_ignore_ascii_case("else") || e.key.eq_ignore_ascii_case("else_if") {
                continue;
            }
            match e.key.as_str() {
                "instant_effect" | "hidden_effect" => {
                    walk(
                        e.block().unwrap_or(&[]),
                        tag,
                        m,
                        today,
                        out,
                        diagnostics,
                        expand,
                        depth + 1,
                        budget,
                    );
                    continue;
                }
                "units" => {
                    // Country/OOB conditions also occur inside the units
                    // container. Resolve them in the same ordered context.
                    let mut body = Vec::new();
                    walk(
                        e.block().unwrap_or(&[]),
                        tag,
                        m,
                        today,
                        &mut body,
                        diagnostics,
                        expand,
                        depth + 1,
                        budget,
                    );
                    let mut container = e.clone();
                    container.value = Some(crate::pdx::Value::Block(body));
                    out.push(container);
                    continue;
                }
                "add_ideas" | "remove_ideas" => {
                    let ids: Vec<_> = e.atom().map(|s| vec![s.to_owned()]).unwrap_or_else(|| {
                        e.block()
                            .unwrap_or(&[])
                            .iter()
                            .map(|e| e.key.clone())
                            .collect()
                    });
                    for id in ids {
                        if e.key == "add_ideas" {
                            m.ideas.insert(id);
                        } else {
                            m.ideas.remove(&id);
                        }
                    }
                }
                "swap_ideas" => {
                    if let Some(b) = e.block() {
                        if let Some(id) = crate::military_data::atom(b, "remove_idea") {
                            m.ideas.remove(id);
                        }
                        if let Some(id) = crate::military_data::atom(b, "add_idea") {
                            m.ideas.insert(id.into());
                        }
                    }
                }
                "set_country_flag" => {
                    if let Some(id) = e.atom() {
                        m.flags.insert(id.into());
                    }
                }
                "clr_country_flag" => {
                    if let Some(id) = e.atom() {
                        m.flags.remove(id);
                    }
                }
                "set_technology" => {
                    for t in e.block().unwrap_or(&[]) {
                        if let Some(v) = t.atom() {
                            if v == "1" || v == "yes" {
                                m.researched.insert(t.key.clone());
                            } else if v == "0" || v == "no" {
                                m.researched.remove(&t.key);
                            }
                        }
                    }
                }
                _ => {}
            }
            out.push(e.clone());
        }
        dated.sort_by_key(|(d, _)| *d);
        for (_, e) in dated {
            walk(
                e.block().unwrap_or(&[]),
                tag,
                m,
                today,
                out,
                diagnostics,
                expand,
                depth + 1,
                budget,
            );
        }
    }
    let mut out = Vec::new();
    let mut diagnostics = Vec::new();
    let mut m = initial.clone();
    walk(
        entries,
        tag,
        &mut m,
        today,
        &mut out,
        &mut diagnostics,
        expand,
        0,
        &mut 100_000,
    );
    (out, diagnostics)
}

#[cfg(test)]
mod tests {
    #[test]
    fn uppercase_country_history_selects_the_correct_oob_and_keeps_ids() {
        let raw=crate::pdx::parse_pdx("IF={LIMIT={has_dlc=D} set_oob=ModernCase set_technology={CaSeD=1}} ELSE_IF={LIMIT={always=no} set_oob=Wrong} ELSE={set_oob=LegacyCase}").unwrap();
        let mut modifiers = crate::military_model::MilitaryModifiers::default();
        modifiers.dlcs.insert("D".into());
        let (selected, diagnostics) =
            super::resolve(&raw, "AAA", &modifiers, crate::game::GameDate::START);
        assert!(diagnostics.is_empty());
        assert!(
            selected
                .iter()
                .any(|e| e.key == "set_oob" && e.atom() == Some("ModernCase"))
        );
        assert!(
            !selected
                .iter()
                .any(|e| e.atom() == Some("LegacyCase") || e.atom() == Some("Wrong"))
        );
        let (selected, _) = super::resolve(
            &raw,
            "AAA",
            &Default::default(),
            crate::game::GameDate::START,
        );
        assert!(
            selected
                .iter()
                .any(|e| e.key == "set_oob" && e.atom() == Some("LegacyCase"))
        );
    }
    use super::*;
    #[test]
    fn nested_and_sibling_else_choose_exactly_one_branch_and_ignore_future_history() {
        let e=parse_pdx("if={limit={has_dlc=D} set_oob=modern else={set_oob=legacy set_technology={old=1}}} if={limit={always=no} load_oob=bad} else_if={limit={always=yes} load_oob=extra} else={load_oob=bad2} 1939.1.1={set_oob=future}").unwrap();
        let (active, d) = resolve(&e, "GER", &Default::default(), GameDate::START);
        assert!(d.is_empty());
        assert_eq!(
            active.iter().filter_map(Entry::atom).collect::<Vec<_>>(),
            vec!["legacy", "extra"]
        );
        let mut m = MilitaryModifiers::default();
        m.dlcs.insert("D".into());
        let (a, _) = resolve(&e, "GER", &m, GameDate::START);
        assert_eq!(a[0].atom(), Some("modern"));
        assert!(!a.iter().any(|e| e.key == "set_technology"));
    }
    #[test]
    fn unknown_branch_cannot_fall_through_and_flags_are_evaluated_in_order() {
        let e=parse_pdx("if={limit={unknown=yes} set_oob=bad else={set_oob=bad2}} set_country_flag=ready if={limit={has_country_flag=ready} load_oob=good}").unwrap();
        let (a, d) = resolve(&e, "GER", &Default::default(), GameDate::START);
        assert_eq!(a.iter().filter(|e| e.key == "load_oob").count(), 1);
        assert!(!a.iter().any(|e| e.key == "set_oob"));
        assert_eq!(d.len(), 1);
    }
}
