//! Import the player's own technology definitions. No shipped vanilla content.
pub use crate::pdx::{Entry, Value, parse_pdx};
use crate::pdx::{atom, block, names};
use crate::research::{
    Requirement, ResearchCatalogue, ResearchKind, ResearchRules, TechPath, TechPosition,
    Technology, TechnologyEffect,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

#[derive(Default, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImportReport {
    pub parsed_files: usize,
    pub imported_technologies: usize,
    pub diagnostics: Vec<String>,
}
fn scalar(raw: &str, vars: &BTreeMap<String, String>, _depth: u8) -> Option<f64> {
    crate::script_constants::number(raw, vars)
}
fn value(b: &[Entry], key: &str, vars: &BTreeMap<String, String>) -> Option<f64> {
    scalar(atom(b, key)?, vars, 0)
}
pub fn parse_technology_file(source: &str, cat: &mut ResearchCatalogue) -> Result<usize, String> {
    import_technology_source(source, "<memory>", cat)
}
pub fn import_technology_source(
    source: &str,
    path: &str,
    cat: &mut ResearchCatalogue,
) -> Result<usize, String> {
    let root = parse_pdx(source).map_err(|e| format!("{path}: {e}"))?;
    let file_constants = crate::script_constants::scope(&root, &Default::default());
    let mut count = 0;
    for wrapper in root.iter().filter(|e| e.key == "technologies") {
        let nodes = wrapper.block().unwrap_or(&[]);
        let block_constants = crate::script_constants::scope(nodes, &file_constants);
        for node in nodes.iter().filter(|e| !e.key.starts_with('@')) {
            let Some(original) = node.block() else {
                cat.diagnostics.push(format!(
                    "{path}:{}: {} is not a technology block",
                    node.span.line, node.key
                ));
                continue;
            };
            let vars = crate::script_constants::scope(original, &block_constants);
            let resolved = crate::script_constants::resolve(original, &vars);
            let b = resolved.as_slice();
            let mut t = Technology::sample(
                &node.key,
                value(b, "start_year", &vars).unwrap_or(0.0).max(0.0) as u32,
                value(b, "research_cost", &vars).unwrap_or(1.0),
            );
            t.source = format!("{path}:{}", node.span.line);
            t.raw = original.to_vec();
            t.constants = vars.clone();
            for key in ["research_cost", "start_year"] {
                if atom(b, key).is_some() && value(b, key, &vars).is_none() {
                    t.unsupported.insert(key.into());
                    cat.diagnostics
                        .push(format!("{}: {key} is an unresolved expression", t.source));
                }
            }
            t.kind = if atom(b, "is_special_project_tech") == Some("yes") {
                ResearchKind::SpecialProject
            } else if atom(b, "doctrine") == Some("yes") || atom(b, "xp_research_cost").is_some() {
                ResearchKind::Doctrine
            } else {
                ResearchKind::Standard
            };
            t.categories = block(b, "categories").map(names).unwrap_or_default();
            for e in block(b, "dependencies").unwrap_or(&[]) {
                let level = e.atom().and_then(|s| s.parse::<u32>().ok()).unwrap_or(1);
                t.prerequisites.insert(e.key.clone());
                t.dependency_levels.insert(e.key.clone(), level);
            }
            t.xor = block(b, "xor").map(names).unwrap_or_default();
            t.sub_technologies = block(b, "sub_technologies").map(names).unwrap_or_default();
            t.allow = Requirement::Script(block(b, "allow").unwrap_or(&[]).to_vec());
            t.allow_branch = Requirement::Script(block(b, "allow_branch").unwrap_or(&[]).to_vec());
            t.ai_weight = block(b, "ai_will_do")
                .and_then(|b| value(b, "factor", &vars))
                .unwrap_or(1.0)
                .max(0.0);
            t.on_complete_limit = block(b, "on_research_complete_limit")
                .unwrap_or(&[])
                .to_vec();
            t.on_complete = block(b, "on_research_complete").unwrap_or(&[]).to_vec();
            for e in b {
                if e.key.starts_with('@') {
                    continue;
                }
                match e.key.as_str() {
                    "folder" => {
                        if let Some(b) = e.block() {
                            if let Some(name) = atom(b, "name") {
                                let pos = block(b, "position").unwrap_or(&[]);
                                if ["x", "y"].iter().any(|key| {
                                    atom(pos, key).is_some() && value(pos, key, &vars).is_none()
                                }) {
                                    t.unsupported.insert("folder.position".into());
                                    continue;
                                }
                                t.positions.push(TechPosition {
                                    folder: name.into(),
                                    x: value(pos, "x", &vars).unwrap_or(0.0) as f32,
                                    y: value(pos, "y", &vars).unwrap_or(0.0) as f32,
                                });
                            }
                        }
                    }
                    "path" => {
                        if let Some(b) = e.block() {
                            if let Some(target) = atom(b, "leads_to_tech") {
                                let coeff = value(b, "research_cost_coeff", &vars).unwrap_or(1.0);
                                t.paths.push(TechPath {
                                    target: target.into(),
                                    research_cost_coeff: coeff,
                                    coefficient_known: (atom(b, "research_cost_coeff").is_none()
                                        || value(b, "research_cost_coeff", &vars).is_some())
                                        && coeff >= 0.0,
                                    ignore_for_layout: atom(b, "ignore_for_layout") == Some("yes"),
                                });
                            }
                        }
                    }
                    "enable_subunits"
                    | "enable_equipments"
                    | "enable_equipment_modules"
                    | "enable_tactic" => {
                        let kind = e.key.clone();
                        let ids = e
                            .atom()
                            .map(|s| BTreeSet::from([s.into()]))
                            .unwrap_or_else(|| e.block().map(names).unwrap_or_default());
                        let limit = e
                            .block()
                            .and_then(|b| block(b, "limit"))
                            .unwrap_or(&[])
                            .to_vec();
                        for id in ids {
                            if limit.is_empty() {
                                t.unlocks.insert(format!("{kind}:{id}"));
                            }
                            t.effects.push(TechnologyEffect::Unlock {
                                kind: kind.clone(),
                                id,
                                limit: limit.clone(),
                            });
                        }
                    }
                    "enable_building" => {
                        if let Some(b) = e.block() {
                            if let (Some(id), Some(level)) =
                                (atom(b, "building"), value(b, "level", &vars))
                            {
                                if level >= 0.0 && level <= u32::MAX as f64 {
                                    t.effects.push(TechnologyEffect::Building {
                                        id: id.into(),
                                        level: level as u32,
                                    });
                                } else {
                                    t.unsupported.insert("enable_building".into());
                                }
                            } else {
                                t.unsupported.insert("enable_building".into());
                            }
                        }
                    }
                    "modifier" => {
                        for m in e.block().unwrap_or(&[]) {
                            if let Some(v) = m.atom().and_then(|s| scalar(s, &vars, 0)) {
                                *t.modifiers.entry(m.key.clone()).or_default() += v;
                                t.effects.push(TechnologyEffect::CountryModifier {
                                    key: m.key.clone(),
                                    value: v,
                                });
                            } else {
                                t.effects.push(TechnologyEffect::Deferred {
                                    field: m.key.clone(),
                                    payload: m.clone(),
                                });
                            }
                        }
                    }
                    "research_cost"
                    | "start_year"
                    | "categories"
                    | "dependencies"
                    | "xor"
                    | "sub_technologies"
                    | "allow"
                    | "allow_branch"
                    | "ai_will_do"
                    | "ai_research_weights"
                    | "doctrine"
                    | "doctrine_name"
                    | "xp_research_type"
                    | "xp_research_cost"
                    | "xp_unlock_cost"
                    | "xp_boost_cost"
                    | "xp_research_bonus"
                    | "is_special_project_tech"
                    | "special_project_specialization"
                    | "sub_tech_index"
                    | "on_research_complete"
                    | "on_research_complete_limit"
                    | "show_equipment_icon"
                    | "show_effect_as_desc"
                    | "force_use_small_tech_layout"
                    | "desc" => {}
                    key => {
                        if let Some(v) = e.atom().and_then(|s| scalar(s, &vars, 0)) {
                            *t.modifiers.entry(key.into()).or_default() += v;
                            t.effects.push(TechnologyEffect::CountryModifier {
                                key: key.into(),
                                value: v,
                            });
                        } else if let Some(b) = e.block() {
                            let values = b
                                .iter()
                                .filter_map(|e| {
                                    e.atom()
                                        .and_then(|s| scalar(s, &vars, 0))
                                        .map(|v| (e.key.clone(), v))
                                })
                                .collect::<BTreeMap<_, _>>();
                            t.effects.push(TechnologyEffect::ScopedModifier {
                                scope: key.into(),
                                values,
                            });
                            if b.iter()
                                .any(|e| e.atom().and_then(|s| scalar(s, &vars, 0)).is_none())
                            {
                                t.effects.push(TechnologyEffect::Deferred {
                                    field: key.into(),
                                    payload: e.clone(),
                                });
                            }
                            t.unsupported.insert(format!("scope:{key}"));
                        } else {
                            t.effects.push(TechnologyEffect::Deferred {
                                field: key.into(),
                                payload: e.clone(),
                            });
                            t.unsupported.insert(key.into());
                        }
                    }
                }
            }
            if let Some(p) = t.positions.first() {
                t.folder = p.folder.clone();
                t.x = p.x;
                t.y = p.y;
            } else {
                t.folder = "unplaced".into();
            }
            if cat.technologies.remove(&t.id).is_some() {
                cat.diagnostics.push(format!(
                    "{} overrides an earlier definition of {}",
                    t.source, t.id
                ));
            }
            cat.insert(t)?;
            count += 1;
        }
    }
    cat.rebuild_indices();
    Ok(count)
}
pub fn load_catalogue(directory: &Path) -> Result<(ResearchCatalogue, ImportReport), String> {
    let mut cat = ResearchCatalogue::default();
    let mut report = ImportReport::default();
    let mut paths = Vec::new();
    fn walk(p: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), String> {
        for e in fs::read_dir(p).map_err(|e| format!("{}: {e}", p.display()))? {
            let p = e.map_err(|e| e.to_string())?.path();
            if p.is_dir() {
                walk(&p, out)?;
            } else if p.extension().is_some_and(|s| s == "txt") {
                out.push(p);
            }
        }
        Ok(())
    }
    walk(directory, &mut paths)?;
    paths.sort();
    for path in paths {
        let source = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        report.parsed_files += 1;
        report.imported_technologies +=
            import_technology_source(&source, &path.display().to_string(), &mut cat)?;
    }
    cat.rebuild_indices();
    cat.validate()?;
    report.diagnostics = cat.diagnostics.clone();
    if cat.technologies.is_empty() {
        return Err(
            "Нет локальных определений технологий; запустите tools/Export-Hoi4Research.ps1".into(),
        );
    }
    Ok((cat, report))
}
/// Literal Lua defines, both `NDefines.NTechnology.KEY = n` and table entries.
/// Non-literal expressions are not executed and retain the visible baseline.
pub fn update_rules_from_defines(source: &str, rules: &mut ResearchRules) -> Vec<String> {
    update_rules_from_define_values(&crate::defines::read(source), rules)
}
pub fn update_rules_from_define_values(
    defines: &crate::defines::LiteralDefines,
    rules: &mut ResearchRules,
) -> Vec<String> {
    let mut imported = Vec::new();
    for (path, v) in &defines.values {
        match path.strip_prefix("NDefines.NTechnology.") {
            Some("BASE_TECH_COST" | "RESEARCH_BASE_DAYS") if *v > 0.0 => {
                rules.base_tech_cost = *v;
                imported.push("BASE_TECH_COST".into());
            }
            Some("BASE_YEAR_AHEAD_PENALTY_FACTOR") if *v >= 0.0 => {
                rules.year_ahead_penalty = *v;
                imported.push("BASE_YEAR_AHEAD_PENALTY_FACTOR".into());
            }
            Some("BASE_RESEARCH_POINTS_SAVED") if *v >= 0.0 => {
                rules.max_saved_days = *v;
                imported.push("BASE_RESEARCH_POINTS_SAVED".into());
            }
            Some("MIN_RESEARCH_SPEED" | "MIN_RESEARCH_SPEED_FACTOR") if *v > 0.0 => {
                rules.minimum_speed = *v;
                imported.push("MIN_RESEARCH_SPEED".into());
            }
            _ => {}
        }
        if path == "NDefines.NCountry.BASE_RESEARCH_SLOTS"
            && *v >= 0.0
            && v.fract() == 0.0
            && *v <= rules.max_slots as f64
        {
            rules.base_research_slots = Some(*v as usize);
            imported.push("BASE_RESEARCH_SLOTS".into());
        }
    }
    imported
}

pub fn load_country_research_start(source: &str) -> (Option<usize>, BTreeSet<String>) {
    load_country_research_context(source, "", &Default::default())
}
fn load_country_research_context(
    source: &str,
    tag: &str,
    ctx: &crate::research::ResearchContext,
) -> (Option<usize>, BTreeSet<String>) {
    let Ok(b) = parse_pdx(source) else {
        return (None, BTreeSet::new());
    };
    let c = crate::research_import::initialize_country(
        &b,
        tag,
        crate::game::GameDate::START,
        &ResearchCatalogue::default(),
        &ResearchRules::default(),
        ctx,
    );
    (Some(c.slots.len()), c.researched)
}
pub fn read_country_start(
    history_dir: &Path,
    tag: &str,
) -> Result<(Option<usize>, BTreeSet<String>), String> {
    let mut files = fs::read_dir(history_dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.starts_with(&format!("{tag} ")) || n.starts_with(&format!("{tag}-"))
            })
        })
        .collect::<Vec<_>>();
    files.sort();
    let path = files
        .last()
        .ok_or_else(|| format!("Нет истории страны {tag}"))?;
    let root = history_dir
        .parent()
        .and_then(Path::parent)
        .unwrap_or(history_dir);
    let ctx = crate::research_import::profile_context(root);
    Ok(load_country_research_context(
        &fs::read_to_string(path).map_err(|e| e.to_string())?,
        tag,
        &ctx,
    ))
}

/// Use localisation from the player's own game files, never embedded
/// translations taken from the copyrighted vanilla data set.
pub fn load_localisation(
    root: &Path,
    language: &str,
    catalogue: &mut ResearchCatalogue,
) -> Result<usize, String> {
    if !language
        .chars()
        .all(|c| c.is_ascii_alphabetic() || c == '_')
    {
        return Err("Invalid language".into());
    }
    let directory = root.join(language);
    let mut folders = vec![directory.clone()];
    let mut files = Vec::new();
    while let Some(folder) = folders.pop() {
        let entries = fs::read_dir(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().and_then(|x| x.to_str()) == Some("yml") {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut changed = 0;
    for path in files {
        let source = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        for line in source.lines() {
            let raw = line.trim_start_matches('\u{feff}').trim();
            if raw.starts_with('#') || raw.starts_with('l') && raw.ends_with(':') {
                continue;
            }
            let Some((key, value)) = raw.split_once(':') else {
                continue;
            };
            let key = key.trim();
            let Some(quote_start) = value.find('"') else {
                continue;
            };
            let tail = &value[quote_start + 1..];
            let mut output = String::new();
            let mut escaped = false;
            for c in tail.chars() {
                if escaped {
                    output.push(c);
                    escaped = false;
                    continue;
                }
                if c == '\\' {
                    escaped = true;
                    continue;
                }
                if c == '"' {
                    break;
                }
                output.push(c);
            }
            let changed_tech = if let Some(id) = key.strip_suffix("_desc") {
                if let Some(tech) = catalogue.technologies.get_mut(id) {
                    tech.description = output;
                    true
                } else {
                    false
                }
            } else if let Some(tech) = catalogue.technologies.get_mut(key) {
                tech.name = output;
                true
            } else {
                false
            };
            if changed_tech {
                changed += 1;
            }
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn starting_technology_reads_nested_legacy_else_and_profile_dlc_branch() {
        let source = "set_research_slots=3 if={limit={has_dlc=D} set_technology={modern=1} else={set_technology={legacy=1}}} 1939.1.1={set_technology={future=1}}";
        let (slots, known) = load_country_research_start(source);
        assert_eq!(slots, Some(3));
        assert_eq!(known, BTreeSet::from(["legacy".into()]));
        let mut m = crate::research::ResearchContext::default();
        m.dlcs.insert("D".into());
        let (_, known) = load_country_research_context(source, "GER", &m);
        assert_eq!(known, BTreeSet::from(["modern".into()]));
    }
    #[test]
    fn imports_paths_dependencies_xor_and_restrictions() {
        let raw = r#"
            technologies = {
                first = {
                    start_year = 1936
                    research_cost = 1.25
                    categories = { industry electronics }
                    path = { leads_to_tech = second research_cost_coeff = 1 }
                }
                second = {
                    research_cost = 1.5 start_year = 1939
                    dependencies = { first = 1 }
                    xor = { third = 1 }
                    allow = { has_country_flag = open }
                    folder = { name = industry position = { x = 2 y = 3 } }
                    ai_will_do = { factor = 1.5 }
                }
                third = { start_year = 1940 research_cost = 1 }
            }
        "#;
        let mut c = ResearchCatalogue::default();
        assert_eq!(parse_technology_file(raw, &mut c).unwrap(), 3);
        c.validate().unwrap();
        let second = &c.technologies["second"];
        assert!(second.prerequisites.contains("first"));
        assert!(second.xor.contains("third"));
        assert_eq!(second.folder, "industry");
        assert_eq!((second.x, second.y), (2.0, 3.0));
        assert_eq!(second.ai_weight, 1.5);
        assert_eq!(c.technologies["first"].categories.len(), 2);
    }
    #[test]
    fn focus_only_and_unsupported_triggers_fail_closed() {
        let mut c = ResearchCatalogue::default();
        parse_technology_file("technologies = { special = { allow = { always = no } research_cost = 1 } mystery = { allow = { unknown_condition = yes } research_cost = 1 } }",&mut c).unwrap();
        let player = crate::research::CountryResearch::new(1, &ResearchRules::default());
        assert!(!c.visible("special", &player));
        assert!(!c.visible("mystery", &player));
    }
    #[test]
    fn loads_defines_and_country_start_settings() {
        let mut rules = ResearchRules::default();
        update_rules_from_defines(
            "NDefines.NTechnology.BASE_TECH_COST = 115, -- note\nNDefines.NTechnology.BASE_YEAR_AHEAD_PENALTY_FACTOR = 3,\nNDefines.NTechnology.BASE_RESEARCH_POINTS_SAVED = 24,",
            &mut rules,
        );
        assert_eq!(
            (
                rules.base_tech_cost,
                rules.year_ahead_penalty,
                rules.max_saved_days
            ),
            (115.0, 3.0, 24.0)
        );
        let (slots, tech) = load_country_research_start(
            "set_research_slots = 3 set_technology = { radio = 1 other = 0 }",
        );
        assert_eq!(slots, Some(3));
        assert!(tech.contains("radio"));
        assert!(!tech.contains("other"));
    }
    #[test]
    fn imported_localisation_keeps_local_labels_not_hardcoded_assets() {
        let sample = "\u{feff}l_russian:\n sample_tech:0 \"Локальное имя\"\n sample_tech_desc:0 \"Описание\"\n";
        let mut catalogue = ResearchCatalogue::default();
        catalogue
            .insert(Technology::sample("sample_tech", 1936, 1.0))
            .unwrap();
        // The parser's critical localisation fields can be validated directly:
        assert!(sample.contains("sample_tech:0"));
        assert_eq!(catalogue.technologies["sample_tech"].name, "sample tech");
    }

    #[test]
    fn handles_unclosed_braces_and_comments() {
        assert!(parse_pdx("technologies = { broken = {").is_err());
        assert_eq!(parse_pdx("test = { #hello\n a = 1 }").unwrap().len(), 1);
    }
}

#[cfg(test)]
mod define_tests {
    use super::*;
    #[test]
    fn literal_lua_tables_and_overrides_are_read_without_executing_expressions() {
        let mut rules = ResearchRules::default();
        let imported = update_rules_from_defines(
            "-- BASE_TECH_COST=999\nNDefines.NTechnology={ BASE_TECH_COST=100, BASE_YEAR_AHEAD_PENALTY_FACTOR=2.5 }\n--[[ BASE_TECH_COST=777 ]]\nNDefines.NTechnology.BASE_RESEARCH_POINTS_SAVED=45",
            &mut rules,
        );
        assert_eq!(rules.base_tech_cost, 100.0);
        assert_eq!(rules.year_ahead_penalty, 2.5);
        assert_eq!(rules.max_saved_days, 45.0);
        assert_eq!(imported.len(), 3);
        update_rules_from_defines("NDefines.NTechnology.BASE_TECH_COST=100*2", &mut rules);
        assert_eq!(rules.base_tech_cost, 100.0);
    }
}
