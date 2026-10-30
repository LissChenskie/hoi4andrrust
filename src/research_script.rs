//! Research-facing trigger/effect interpreter. Unknown predicates remain Unknown,
//! including under NOT; unsupported effects retain their full AST and origin.
use crate::game::GameDate;
use crate::pdx::{Entry, atom, block, number};
use crate::research::{
    CountryResearch, DeferredEffect, ResearchBonus, ResearchCatalogue, ResearchRules,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SharingGroup {
    pub id: String,
    pub per_country: f64,
    pub categories: BTreeSet<String>,
    pub available: Vec<Entry>,
    pub faction: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptSource {
    pub kind: String,
    pub id: String,
    pub source: String,
    pub conditions: Vec<Entry>,
    pub effects: Vec<Entry>,
    pub raw: Vec<Entry>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FocusTreeRecord {
    pub id: String,
    pub source: String,
    pub country: Vec<Entry>,
    pub default: bool,
    pub focuses: BTreeSet<String>,
    #[serde(default)]
    pub shared: BTreeSet<String>,
    #[serde(default)]
    pub raw: Vec<Entry>,
    #[serde(default)]
    pub problems: Vec<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResearchScripts {
    #[serde(default)]
    pub laws_indexed: bool,
    #[serde(default)]
    pub default_laws: BTreeMap<String, String>,
    #[serde(default)]
    pub law_groups: BTreeMap<String, String>,
    pub triggers: BTreeMap<String, Vec<Entry>>,
    pub effects: BTreeMap<String, Vec<Entry>>,
    pub ideas: BTreeMap<String, Vec<Entry>>,
    pub sharing: BTreeMap<String, SharingGroup>,
    pub sources: BTreeMap<String, ScriptSource>,
    #[serde(default)]
    pub focus_trees: BTreeMap<String, FocusTreeRecord>,
    #[serde(default)]
    pub shared_focuses: BTreeSet<String>,
    #[serde(default)]
    pub focus_problems: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub localisation: BTreeMap<String, String>,
    #[serde(default)]
    pub static_modifiers: BTreeMap<String, Vec<Entry>>,
}
pub fn parse_date(s: &str) -> Option<GameDate> {
    let p: Vec<_> = s.split('.').collect();
    if p.len() < 3 || p.len() > 4 {
        return None;
    }
    let date = GameDate {
        year: p[0].parse().ok()?,
        month: p[1].parse().ok()?,
        day: p[2].parse().ok()?,
    };
    let end = match date.month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if date.year % 4 == 0 && (date.year % 100 != 0 || date.year % 400 == 0) => 29,
        2 => 28,
        _ => return None,
    };
    (date.day > 0 && date.day <= end).then_some(date)
}
fn compare<T: PartialOrd + PartialEq>(a: T, op: &str, b: T) -> Option<bool> {
    Some(match op {
        "=" | "==" => a == b,
        "!=" => a != b,
        ">" => a > b,
        "<" => a < b,
        ">=" => a >= b,
        "<=" => a <= b,
        _ => return None,
    })
}
fn conjunction(v: impl IntoIterator<Item = Option<bool>>) -> Option<bool> {
    let mut unknown = false;
    for b in v {
        match b {
            Some(false) => return Some(false),
            None => unknown = true,
            _ => {}
        }
    }
    if unknown { None } else { Some(true) }
}
fn disjunction(v: impl IntoIterator<Item = Option<bool>>) -> Option<bool> {
    let mut unknown = false;
    for b in v {
        match b {
            Some(true) => return Some(true),
            None => unknown = true,
            _ => {}
        }
    }
    if unknown { None } else { Some(false) }
}
pub fn evaluate(entries: &[Entry], country: &CountryResearch) -> Option<bool> {
    evaluate_depth(entries, country, 0)
}
fn evaluate_depth(entries: &[Entry], c: &CountryResearch, depth: u8) -> Option<bool> {
    if depth > 64 {
        return None;
    }
    conjunction(parts(entries, c, depth + 1))
}
fn parts(entries: &[Entry], c: &CountryResearch, depth: u8) -> Vec<Option<bool>> {
    if depth > 64 {
        return vec![None];
    }
    fn alternatives<'a>(body: &'a [Entry], out: &mut Vec<(bool, &'a [Entry])>) {
        for e in body
            .iter()
            .filter(|e| e.key.eq_ignore_ascii_case("else") || e.key.eq_ignore_ascii_case("else_if"))
        {
            if let Some(b) = e.block() {
                out.push((e.key.eq_ignore_ascii_case("else"), b));
                alternatives(b, out);
            }
        }
    }
    let mut result = Vec::new();
    let mut index = 0;
    while let Some(e) = entries.get(index) {
        index += 1;
        if !e.key.eq_ignore_ascii_case("if") {
            result.push(
                if e.key.eq_ignore_ascii_case("else") || e.key.eq_ignore_ascii_case("else_if") {
                    None
                } else {
                    predicate(e, c, depth)
                },
            );
            continue;
        }
        let Some(body) = e.block() else {
            result.push(None);
            continue;
        };
        let mut chain = vec![(false, body)];
        alternatives(body, &mut chain);
        while index < entries.len()
            && (entries[index].key.eq_ignore_ascii_case("else")
                || entries[index].key.eq_ignore_ascii_case("else_if"))
        {
            let branch = &entries[index];
            index += 1;
            if let Some(body) = branch.block() {
                chain.push((branch.key.eq_ignore_ascii_case("else"), body));
                alternatives(body, &mut chain);
            }
        }
        let mut value = Some(true);
        for (otherwise, body) in chain {
            match if otherwise {
                Some(true)
            } else {
                evaluate_depth(
                    crate::pdx::keyword_block(body, "limit").unwrap_or(&[]),
                    c,
                    depth + 1,
                )
            } {
                Some(false) => continue,
                None => {
                    value = None;
                    break;
                }
                Some(true) => {
                    let selected = body
                        .iter()
                        .filter(|e| {
                            !(e.key.eq_ignore_ascii_case("limit")
                                || e.key.eq_ignore_ascii_case("else")
                                || e.key.eq_ignore_ascii_case("else_if"))
                        })
                        .cloned()
                        .collect::<Vec<_>>();
                    value = evaluate_depth(&selected, c, depth + 1);
                    break;
                }
            }
        }
        result.push(value);
    }
    result
}
pub fn numeric(raw: &str, c: &CountryResearch) -> Option<f64> {
    if let Ok(value) = raw.parse::<f64>() {
        return value.is_finite().then_some(value);
    }
    let raw = raw.strip_prefix("var:").unwrap_or(raw);
    let (id, fallback) = raw
        .split_once('?')
        .map_or((raw, None), |(id, n)| (id, n.parse::<f64>().ok()));
    let id = if let Some((scope, id)) = id.split_once('.') {
        if scope != c.context.tag && scope != "ROOT" && scope != "THIS" {
            return None;
        }
        id
    } else {
        id
    };
    c.context
        .variables
        .get(id)
        .copied()
        .or(fallback)
        .filter(|n| n.is_finite())
}
fn predicate(e: &Entry, c: &CountryResearch, d: u8) -> Option<bool> {
    let ctx = &c.context;
    let raw = e.atom();
    let nested = e.block().unwrap_or(&[]);
    let yesno = |value: Option<bool>| match raw {
        Some("yes" | "true" | "1") => value,
        Some("no" | "false" | "0") => value.map(|v| !v),
        _ => None,
    };
    match e.key.as_str() {
        "AND" | "and" | "ROOT" | "THIS" if e.block().is_some() => evaluate_depth(nested, c, d),
        "OR" | "or" => disjunction(parts(nested, c, d)),
        "NOT" | "not" | "NOR" | "nor" => disjunction(parts(nested, c, d)).map(|v| !v),
        "NAND" | "nand" => evaluate_depth(nested, c, d).map(|v| !v),
        "always" => yesno(Some(true)),
        "tag" => raw.map(|v| ctx.tag == v),
        "original_tag" => raw.map(|v| ctx.original_tag == v),
        "has_country_flag" => raw.map(|v| c.flags.contains(v)),
        "has_global_flag" => raw.map(|v| ctx.global_flags.contains(v)),
        "has_tech" | "has_technology" => raw.map(|v| c.researched.contains(v)),
        "has_completed_focus" => raw.map(|v| ctx.completed_focuses.contains(v)),
        "has_idea" => raw.map(|v| ctx.ideas.contains(v)),
        "has_template_containing_unit" => ctx
            .template_units
            .as_ref()
            .map(|units| raw.map(|id| units.contains(id)))
            .flatten(),
        "has_dlc" => {
            if raw.is_some_and(|id| ctx.base_game_dlcs.contains(id)) {
                Some(true)
            } else if ctx.dlc_selection_unverified {
                None
            } else {
                raw.map(|v| ctx.dlcs.contains(v))
            }
        }
        "has_government" => Some(ctx.ideology.as_deref()? == raw?),
        "is_in_faction" => yesno(ctx.faction.as_ref().map(|v| !v.is_empty())),
        "has_war" => yesno(ctx.at_war),
        "is_subject" => yesno(ctx.is_subject),
        "is_puppet_of" => Some(ctx.overlord.as_deref()? == raw?),
        "date" => compare(ctx.date?, &e.operator, parse_date(raw?)?),
        "has_game_rule" => {
            let rule = atom(nested, "rule")?;
            Some(ctx.rules.get(rule)?.as_str() == atom(nested, "option")?)
        }
        "check_variable" => {
            let key = atom(nested, "var")?;
            let v = *ctx.variables.get(key)?;
            let target = number(nested, "value")
                .or_else(|| ctx.variables.get(atom(nested, "value")?).copied())?;
            let op = atom(nested, "compare").unwrap_or("greater_than_or_equals");
            compare(
                v,
                match op {
                    "greater_than" => ">",
                    "less_than" => "<",
                    "equals" => "=",
                    "less_than_or_equals" => "<=",
                    "not_equals" => "!=",
                    _ => ">=",
                },
                target,
            )
        }
        "custom_trigger_tooltip" | "hidden_trigger" => evaluate_depth(
            &nested
                .iter()
                .filter(|e| e.key != "tooltip")
                .cloned()
                .collect::<Vec<_>>(),
            c,
            d,
        ),
        // Scope switches to other countries cannot be evaluated using THIS country's facts.
        key if key == ctx.tag && e.block().is_some() => evaluate_depth(nested, c, d),
        key if ctx.variables.contains_key(key) => {
            compare(ctx.variables[key], &e.operator, raw?.parse::<f64>().ok()?)
        }
        _ => None,
    }
}
/// Expand parameter-free scripted triggers once at import, not each frame.
pub fn expand_triggers(b: &[Entry], scripts: &ResearchScripts, depth: u8) -> Vec<Entry> {
    if depth > 32 {
        return b.to_vec();
    }
    let mut out = Vec::new();
    for e in b {
        if let Some(body) = scripts.triggers.get(&e.key) {
            if matches!(e.atom(), Some("yes" | "no")) {
                let mut x = e.clone();
                x.key = if e.atom() == Some("no") { "NOT" } else { "AND" }.into();
                x.value = Some(crate::pdx::Value::Block(expand_triggers(
                    body,
                    scripts,
                    depth + 1,
                )));
                out.push(x);
                continue;
            }
        }
        let mut x = e.clone();
        if let Some(b) = e.block() {
            x.value = Some(crate::pdx::Value::Block(expand_triggers(
                b,
                scripts,
                depth + 1,
            )));
        }
        out.push(x);
    }
    out
}
fn defer(c: &mut CountryResearch, e: &Entry, source: &str, reason: &str) {
    defer_block(c, std::slice::from_ref(e), source, reason);
}
fn defer_block(c: &mut CountryResearch, entries: &[Entry], source: &str, reason: &str) {
    fn changes_slots(e: &Entry) -> bool {
        matches!(e.key.as_str(), "set_research_slots" | "add_research_slot")
            || e.block().is_some_and(|b| b.iter().any(changes_slots))
    }
    if entries.iter().any(changes_slots) {
        c.research_slots_known = false;
    }
    let pending = DeferredEffect {
        source: source.into(),
        field: entries.first().map(|e| e.key.clone()).unwrap_or_default(),
        reason: reason.into(),
        payload: entries.to_vec(),
    };
    if !c.pending_effects.iter().any(|p| {
        p.source == pending.source && p.field == pending.field && p.payload == pending.payload
    }) {
        c.pending_effects.push(pending);
    }
}
pub fn apply_effects(
    c: &mut CountryResearch,
    b: &[Entry],
    cat: &ResearchCatalogue,
    rules: &ResearchRules,
    date: GameDate,
    source: &str,
) {
    c.context.date = Some(date);
    let mut budget = 10_000;
    execute(c, b, cat, rules, date, source, 0, &mut budget);
    refresh_ideas(c, cat);
    c.revision += 1;
}
fn execute(
    c: &mut CountryResearch,
    b: &[Entry],
    cat: &ResearchCatalogue,
    rules: &ResearchRules,
    date: GameDate,
    source: &str,
    depth: u8,
    budget: &mut usize,
) {
    if depth > 32 || *budget == 0 {
        if let Some(e) = b.first() {
            defer(c, e, source, "Превышен предел рекурсии/бюджет скрипта");
        }
        return;
    }
    let mut index = 0;
    while let Some(e) = b.get(index) {
        index += 1;
        if *budget == 0 {
            defer(c, e, source, "Превышен бюджет выполнения");
            return;
        }
        *budget -= 1;
        let nested = e.block().unwrap_or(&[]);
        if e.key.eq_ignore_ascii_case("if") {
            let chain_start = index - 1;
            let mut alternatives = vec![(false, nested)];
            fn collect<'a>(b: &'a [Entry], out: &mut Vec<(bool, &'a [Entry])>) {
                for e in b.iter().filter(|e| {
                    e.key.eq_ignore_ascii_case("else") || e.key.eq_ignore_ascii_case("else_if")
                }) {
                    if let Some(b) = e.block() {
                        out.push((e.key.eq_ignore_ascii_case("else"), b));
                        collect(b, out);
                    }
                }
            }
            collect(nested, &mut alternatives);
            while index < b.len()
                && (b[index].key.eq_ignore_ascii_case("else")
                    || b[index].key.eq_ignore_ascii_case("else_if"))
            {
                let e = &b[index];
                index += 1;
                if let Some(b) = e.block() {
                    alternatives.push((e.key.eq_ignore_ascii_case("else"), b));
                    collect(b, &mut alternatives);
                }
            }
            for (otherwise, body) in alternatives {
                match if otherwise {
                    Some(true)
                } else {
                    evaluate(
                        &expand_triggers(
                            crate::pdx::keyword_block(body, "limit").unwrap_or(&[]),
                            &cat.scripts,
                            0,
                        ),
                        c,
                    )
                } {
                    Some(true) => {
                        let selected: Vec<_> = body
                            .iter()
                            .filter(|e| {
                                !(e.key.eq_ignore_ascii_case("limit")
                                    || e.key.eq_ignore_ascii_case("else")
                                    || e.key.eq_ignore_ascii_case("else_if"))
                            })
                            .cloned()
                            .collect();
                        execute(c, &selected, cat, rules, date, source, depth + 1, budget);
                        break;
                    }
                    Some(false) => {}
                    None => {
                        defer_block(
                            c,
                            &b[chain_start..index],
                            source,
                            "Неизвестное условие if: ни одна ветка не исполнена",
                        );
                        break;
                    }
                }
            }
            continue;
        }
        match e.key.as_str() {
            "else" | "else_if" | "limit" => {}
            "hidden_effect" | "effect_tooltip" => {
                execute(c, nested, cat, rules, date, source, depth + 1, budget)
            }
            "set_technology" => {
                for tech in nested {
                    if tech.key == "popup" {
                        continue;
                    }
                    let level = tech.atom().and_then(|v| {
                        if v == "yes" {
                            Some(1)
                        } else if v == "no" {
                            Some(0)
                        } else {
                            v.parse::<u32>().ok()
                        }
                    });
                    match level {
                        Some(level) if level > 0 => {
                            if !c.grant(&tech.key, cat) {
                                if !cat.technologies.contains_key(&tech.key) {
                                    c.researched.insert(tech.key.clone());
                                    defer(
                                        c,
                                        tech,
                                        source,
                                        "Определение стартовой технологии отсутствует",
                                    );
                                }
                            }
                            c.technology_levels.insert(tech.key.clone(), level);
                        }
                        Some(0) => {
                            c.revoke(&tech.key, cat);
                        }
                        _ => defer(c, tech, source, "Некорректный уровень технологии"),
                    }
                }
            }
            "set_research_slots" | "add_research_slot" => {
                if let Some(n) = e
                    .atom()
                    .and_then(|v| numeric(v, c))
                    .filter(|n| n.fract() == 0.0)
                {
                    let count = if e.key == "add_research_slot" {
                        c.slots.len() as f64 + n
                    } else {
                        n
                    };
                    if count < 0.0
                        || count > rules.max_slots as f64
                        || c.resize_slots(count as usize, rules).is_err()
                    {
                        defer(
                            c,
                            e,
                            source,
                            "Недопустимое число слотов или занятые удаляемые ячейки",
                        );
                    } else if e.key == "set_research_slots" {
                        c.research_slots_known = true;
                    }
                } else {
                    defer(c, e, source, "Не вычислено количество слотов");
                }
            }
            "add_tech_bonus" => {
                let speed = number(nested, "bonus").unwrap_or(0.0);
                let ahead = number(nested, "ahead_reduction").unwrap_or(0.0);
                let uses = number(nested, "uses").unwrap_or(1.0);
                if uses > 0.0 && uses <= u32::MAX as f64 && speed.is_finite() && ahead.is_finite() {
                    c.available_bonuses.push(ResearchBonus {
                        name: atom(nested, "name").unwrap_or(source).into(),
                        category: None,
                        technology: None,
                        categories: nested
                            .iter()
                            .filter(|e| e.key == "category")
                            .filter_map(|e| e.atom().map(str::to_owned))
                            .collect(),
                        technologies: nested
                            .iter()
                            .filter(|e| e.key == "technology")
                            .filter_map(|e| e.atom().map(str::to_owned))
                            .collect(),
                        speed,
                        ahead_reduction_years: ahead,
                        uses: uses as u32,
                    });
                } else {
                    defer(c, e, source, "Некорректный исследовательский бонус");
                }
            }
            "set_country_flag" | "clr_country_flag" | "set_global_flag" | "clr_global_flag" => {
                if let Some(id) = e.atom() {
                    let set = if e.key.contains("global") {
                        &mut c.context.global_flags
                    } else {
                        &mut c.flags
                    };
                    if e.key.starts_with("clr") {
                        set.remove(id);
                    } else {
                        set.insert(id.into());
                    }
                } else {
                    defer(c, e, source, "Флаги с таймером пока не исполнены");
                }
            }
            "add_ideas" | "remove_ideas" => {
                let ids: Vec<_> = e
                    .atom()
                    .map(|s| vec![s.to_owned()])
                    .unwrap_or_else(|| crate::pdx::names(nested).into_iter().collect());
                for id in ids {
                    if e.key == "add_ideas" {
                        if let Some(group) = cat.scripts.law_groups.get(&id) {
                            c.context
                                .ideas
                                .retain(|old| cat.scripts.law_groups.get(old) != Some(group));
                        }
                        c.context.ideas.insert(id);
                    } else {
                        c.context.ideas.remove(&id);
                    }
                }
                refresh_ideas(c, cat);
            }
            "swap_ideas" => {
                if let Some(id) = atom(nested, "remove_idea") {
                    c.context.ideas.remove(id);
                }
                if let Some(id) = atom(nested, "add_idea") {
                    if let Some(group) = cat.scripts.law_groups.get(id) {
                        c.context
                            .ideas
                            .retain(|old| cat.scripts.law_groups.get(old) != Some(group));
                    }
                    c.context.ideas.insert(id.into());
                }
                refresh_ideas(c, cat);
            }
            "set_politics" => {
                if let Some(s) = atom(nested, "ruling_party") {
                    c.context.ideology = Some(s.into());
                }
            }
            "add_to_tech_sharing_group" | "remove_from_tech_sharing_group" => {
                if let Some(id) = e.atom() {
                    if e.key.starts_with("add_") {
                        c.sharing_groups.insert(id.into());
                    } else {
                        c.sharing_groups.remove(id);
                    }
                }
            }
            "modify_tech_sharing_bonus" => {
                if let (Some(id), Some(v)) = (atom(nested, "id"), number(nested, "bonus")) {
                    *c.sharing_adjustments.entry(id.into()).or_default() += v;
                } else {
                    defer(c, e, source, "Некорректная группа обмена технологиями");
                }
            }
            "set_variable"
            | "add_to_variable"
            | "subtract_from_variable"
            | "multiply_variable"
            | "divide_variable" => {
                if let (Some(id), Some(v)) = (atom(nested, "var"), number(nested, "value")) {
                    let x = c.context.variables.entry(id.into()).or_default();
                    match e.key.as_str() {
                        "set_variable" => *x = v,
                        "add_to_variable" => *x += v,
                        "subtract_from_variable" => *x -= v,
                        "multiply_variable" => *x *= v,
                        _ if v != 0.0 => *x /= v,
                        _ => defer(c, e, source, "Деление на ноль"),
                    }
                } else {
                    defer(c, e, source, "Не вычислено значение переменной");
                }
            }
            "complete_national_focus" => {
                if let Some(id) = e.atom() {
                    c.context.completed_focuses.insert(id.into());
                }
            }
            key if cat.scripts.effects.contains_key(key) && e.atom() == Some("yes") => execute(
                c,
                &cat.scripts.effects[key],
                cat,
                rules,
                date,
                source,
                depth + 1,
                budget,
            ),
            _ => defer(
                c,
                e,
                source,
                "Эффект сохранён; соответствующая подсистема/сфера действия не реализована",
            ),
        }
    }
}
pub fn refresh_ideas(c: &mut CountryResearch, cat: &ResearchCatalogue) {
    c.source_modifiers
        .retain(|source, _| !source.starts_with("idea:"));
    c.source_category_bonuses.clear();
    for id in &c.context.ideas {
        if let Some(raw) = cat.scripts.ideas.get(id) {
            for e in block(raw, "research_bonus").unwrap_or(&[]) {
                if let Some(v) = e
                    .atom()
                    .and_then(|s| s.parse::<f64>().ok())
                    .filter(|v| v.is_finite())
                {
                    *c.source_category_bonuses.entry(e.key.clone()).or_default() += v;
                }
            }
            let m = block(raw, "modifier")
                .unwrap_or(&[])
                .iter()
                .filter_map(|e| {
                    e.atom()
                        .and_then(|v| v.parse::<f64>().ok())
                        .filter(|v| v.is_finite())
                        .map(|v| (e.key.clone(), v))
                })
                .collect();
            c.source_modifiers.insert(format!("idea:{id}"), m);
        }
    }
}
impl crate::research::ResearchWorld {
    /// Hook for the future focus/event/decision engines. Merely importing rewards never grants them.
    pub fn execute_source(
        &mut self,
        tag: &str,
        kind: &str,
        id: &str,
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
        date: GameDate,
    ) -> Result<(), String> {
        let source = cat
            .scripts
            .sources
            .get(&format!("{kind}:{id}"))
            .ok_or("Источник отсутствует")?;
        let c = self.countries.get_mut(tag).ok_or("Страна отсутствует")?;
        c.context.date = Some(date);
        c.context.global_flags = self.global_flags.clone();
        let before = self.global_flags.clone();
        if evaluate(&expand_triggers(&source.conditions, &cat.scripts, 0), c) != Some(true) {
            return Err("Условия источника не подтверждены".into());
        }
        if kind == "focus" && !c.context.completed_focuses.insert(id.into()) {
            return Err("Фокус уже завершён".into());
        }
        apply_effects(
            c,
            &source.effects,
            cat,
            rules,
            date,
            &format!("{kind}:{id}"),
        );
        for flag in before.difference(&c.context.global_flags) {
            self.global_flags.remove(flag);
        }
        self.global_flags
            .extend(c.context.global_flags.difference(&before).cloned());
        for c in self.countries.values_mut() {
            c.context.global_flags = self.global_flags.clone();
        }
        Ok(())
    }
    pub fn refresh_sharing(&mut self, cat: &ResearchCatalogue) {
        let mut bonuses = BTreeMap::new();
        for (tag, c) in &self.countries {
            let mut values = BTreeMap::new();
            for group in cat.scripts.sharing.values() {
                if !c.sharing_groups.contains(&group.id)
                    || evaluate(&group.available, c) != Some(true)
                {
                    continue;
                }
                for t in cat.technologies.values() {
                    if !group.categories.is_empty() && group.categories.is_disjoint(&t.categories) {
                        continue;
                    }
                    let count = self
                        .countries
                        .iter()
                        .filter(|(other, p)| {
                            *other != tag
                                && p.sharing_groups.contains(&group.id)
                                && p.researched.contains(&t.id)
                                && evaluate(&group.available, p) == Some(true)
                                && (!group.faction
                                    || c.context.faction.as_ref().is_some_and(|f| {
                                        !f.is_empty() && p.context.faction.as_ref() == Some(f)
                                    }))
                        })
                        .count();
                    let value = count as f64 * group.per_country
                        + c.sharing_adjustments.get(&group.id).copied().unwrap_or(0.0);
                    let x = values.entry(t.id.clone()).or_insert(0.0_f64);
                    *x = (*x).max(value);
                }
            }
            bonuses.insert(tag.clone(), values);
        }
        for (tag, v) in bonuses {
            let c = self.countries.get_mut(&tag).unwrap();
            if c.sharing_bonuses != v {
                c.sharing_bonuses = v;
                c.revision += 1;
            }
        }
    }
}

/// Static audit of predicates. Missing runtime facts and unimplemented syntax
/// are separate: the latter is named in the import report before a game starts.
pub fn unsupported_predicates(entries: &[Entry]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for e in entries {
        if ["if", "else", "else_if", "limit"]
            .iter()
            .any(|key| e.key.eq_ignore_ascii_case(key))
        {
            out.extend(unsupported_predicates(e.block().unwrap_or(&[])));
            continue;
        }
        match e.key.as_str() {
            "AND" | "and" | "OR" | "or" | "NOT" | "not" | "NOR" | "nor" | "NAND" | "nand"
            | "ROOT" | "THIS" | "hidden_trigger" | "if" | "else_if" | "else" | "limit" => {
                out.extend(unsupported_predicates(e.block().unwrap_or(&[])))
            }
            "custom_trigger_tooltip" => {
                let filtered = e
                    .block()
                    .unwrap_or(&[])
                    .iter()
                    .filter(|e| e.key != "tooltip")
                    .cloned()
                    .collect::<Vec<_>>();
                out.extend(unsupported_predicates(&filtered));
            }
            "always"
            | "tag"
            | "original_tag"
            | "has_country_flag"
            | "has_global_flag"
            | "has_tech"
            | "has_technology"
            | "has_completed_focus"
            | "has_idea"
            | "has_dlc"
            | "has_government"
            | "is_in_faction"
            | "has_war"
            | "is_subject"
            | "is_puppet_of"
            | "date"
            | "has_game_rule"
            | "check_variable"
            | "has_template_containing_unit" => {}
            other => {
                out.insert(other.into());
            }
        }
    }
    out
}

#[cfg(test)]
mod global_flag_tests {
    use super::*;
    #[test]
    fn uppercase_control_keywords_preserve_technology_ids_and_unknown_guards() {
        let rules = ResearchRules::default();
        let mut cat = ResearchCatalogue::default();
        crate::research_data::parse_technology_file("technologies={CaSeD={} Legacy={}}", &mut cat)
            .unwrap();
        let b=crate::pdx::parse_pdx("IF={LIMIT={has_dlc=D} set_technology={CaSeD=1}} ELSE_IF={LIMIT={always=no} set_technology={Legacy=1}} ELSE={set_country_flag=Fallback}").unwrap();
        let mut c = CountryResearch::new(1, &rules);
        c.context.dlc_selection_unverified = true;
        apply_effects(&mut c, &b, &cat, &rules, GameDate::START, "fixture");
        assert!(c.researched.is_empty());
        assert!(!c.flags.contains("Fallback"));
        c.context.dlc_selection_unverified = false;
        c.context.dlcs.insert("D".into());
        apply_effects(&mut c, &b, &cat, &rules, GameDate::START, "fixture");
        assert!(c.researched.contains("CaSeD"));
        assert!(!c.researched.contains("Legacy"));
        let trigger =
            crate::pdx::parse_pdx("IF={LIMIT={has_dlc=D} has_tech=CaSeD} ELSE={always=no}")
                .unwrap();
        assert_eq!(evaluate(&trigger, &c), Some(true));
        assert!(unsupported_predicates(&trigger).is_empty());
    }
    #[test]
    fn conditional_triggers_select_branches_and_keep_unknown() {
        let mut c = CountryResearch::new(1, &ResearchRules::default());
        c.context.tag = "AAA".into();
        for source in [
            "if={limit={tag=AAA} always=yes} else={always=no}",
            "if={limit={tag=BBB} always=no} else_if={limit={tag=AAA} always=yes} else={always=no}",
            "if={limit={tag=BBB} always=no else={always=yes}}",
            "if={limit={tag=BBB} always=no}",
        ] {
            assert_eq!(
                evaluate(&crate::pdx::parse_pdx(source).unwrap(), &c),
                Some(true),
                "{source}"
            );
        }
        for source in [
            "if={limit={unknown_fact=yes} always=no} else={always=yes}",
            "NOT={if={limit={unknown_fact=yes} always=no}}",
        ] {
            assert_eq!(evaluate(&crate::pdx::parse_pdx(source).unwrap(), &c), None);
        }
    }
    #[test]
    fn template_facts_and_slot_variable_fallbacks_are_explicit() {
        let rules = ResearchRules::default();
        let cat = ResearchCatalogue::default();
        let mut c = CountryResearch::new(2, &rules);
        c.context.tag = "AAA".into();
        let p = crate::pdx::parse_pdx("has_template_containing_unit=marines").unwrap();
        assert_eq!(evaluate(&p, &c), None);
        c.context.template_units = Some(BTreeSet::new());
        assert_eq!(evaluate(&p, &c), Some(false));
        c.context
            .template_units
            .as_mut()
            .unwrap()
            .insert("marines".into());
        assert_eq!(evaluate(&p, &c), Some(true));
        assert_eq!(numeric("var:ROOT.slots?3", &c), Some(3.0));
        assert_eq!(numeric("var:BBB.slots?3", &c), None);
        c.context.variables.insert("slots".into(), 4.0);
        assert_eq!(numeric("var:ROOT.slots?3", &c), Some(4.0));
        apply_effects(
            &mut c,
            &crate::pdx::parse_pdx("set_research_slots=var:ROOT.slots?3").unwrap(),
            &cat,
            &rules,
            GameDate::START,
            "fixture",
        );
        assert_eq!(c.slots.len(), 4);
        let chain = crate::pdx::parse_pdx(
            "if={limit={unknown_fact=yes} set_country_flag=a} else={add_research_slot=1}",
        )
        .unwrap();
        apply_effects(&mut c, &chain, &cat, &rules, GameDate::START, "fixture");
        assert!(!c.research_slots_known);
        assert_eq!(c.slots.len(), 4);
        assert_eq!(c.pending_effects[0].payload.len(), 2);
        apply_effects(
            &mut c,
            &crate::pdx::parse_pdx("set_research_slots=2").unwrap(),
            &cat,
            &rules,
            GameDate::START,
            "fixture",
        );
        assert!(c.research_slots_known);
    }
    #[test]
    fn technology_and_event_global_flags_are_visible_to_other_countries() {
        let mut cat = ResearchCatalogue::default();
        crate::research_data::parse_technology_file("technologies={signal={research_cost=0.01 on_research_complete={set_global_flag=signal_ready}}}",&mut cat).unwrap();
        let rules = ResearchRules::default();
        let mut world = crate::research::ResearchWorld::default();
        world.country_mut("BBB", 1, &rules);
        world
            .country_mut("AAA", 1, &rules)
            .start(0, "signal", &cat, &rules, GameDate::START)
            .unwrap();
        world.tick_country("AAA", &cat, &rules, GameDate::START);
        world.tick_country("AAA", &cat, &rules, GameDate::START);
        assert!(world.global_flags.contains("signal_ready"));
        assert!(
            world.countries["BBB"]
                .context
                .global_flags
                .contains("signal_ready")
        );
        cat.scripts.sources.insert(
            "event:clear".into(),
            ScriptSource {
                kind: "event".into(),
                id: "clear".into(),
                source: "test".into(),
                conditions: Vec::new(),
                effects: crate::pdx::parse_pdx("clr_global_flag=signal_ready").unwrap(),
                raw: Vec::new(),
            },
        );
        world
            .execute_source("AAA", "event", "clear", &cat, &rules, GameDate::START)
            .unwrap();
        assert!(
            !world.countries["BBB"]
                .context
                .global_flags
                .contains("signal_ready")
        );
    }
}
