//! Local, versioned military data ingestion. Reads base -> explicit overlay layers.
//! Unsupported conditional data is reported; it is never treated as an unlock.
use crate::military_model::*;
use crate::research_data::{Entry, parse_pdx};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

pub fn find<'a>(b: &'a [Entry], key: &str) -> Option<&'a Entry> {
    b.iter().rev().find(|e| e.key == key)
}
pub fn atom<'a>(b: &'a [Entry], key: &str) -> Option<&'a str> {
    find(b, key)?.atom()
}
pub fn block<'a>(b: &'a [Entry], key: &str) -> Option<&'a [Entry]> {
    find(b, key)?.block()
}
pub fn number(b: &[Entry], key: &str, default: f64) -> f64 {
    atom(b, key)
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(default)
}
fn yes(b: &[Entry], key: &str, default: bool) -> bool {
    atom(b, key).map_or(default, |v| v == "yes" || v == "true")
}
pub fn names(b: &[Entry]) -> BTreeSet<String> {
    b.iter()
        .filter(|e| e.value.is_none() || e.atom() == Some("yes") || e.atom() == Some("1"))
        .map(|e| e.key.clone())
        .collect()
}

fn scalar_value(raw: &str, variables: &BTreeMap<String, String>, depth: u8) -> Option<f64> {
    if depth > 20 {
        return None;
    }
    raw.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .or_else(|| {
            variables
                .get(raw)
                .and_then(|v| scalar_value(v, variables, depth + 1))
        })
}
pub fn stats(b: &[Entry], variables: &BTreeMap<String, String>) -> StatBlock {
    b.iter()
        .filter_map(|e| {
            e.atom()
                .and_then(|raw| scalar_value(raw, variables, 0))
                .map(|v| (e.key.clone(), v))
        })
        .collect()
}
fn variables(b: &[Entry]) -> BTreeMap<String, String> {
    b.iter()
        .filter(|e| e.key.starts_with('@'))
        .filter_map(|e| e.atom().map(|v| (e.key.clone(), v.into())))
        .collect()
}

fn walk(
    directory: &Path,
    root: &Path,
    recursive: bool,
    extension: &str,
    out: &mut BTreeMap<PathBuf, PathBuf>,
) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if recursive && path.is_dir() {
            walk(&path, root, true, extension, out);
        } else if path.extension().and_then(|v| v.to_str()) == Some(extension) {
            if let Ok(relative) = path.strip_prefix(root) {
                out.insert(relative.to_path_buf(), path);
            }
        }
    }
}
pub fn layered_files(
    layers: &[PathBuf],
    relative: &str,
    recursive: bool,
    extension: &str,
) -> Vec<PathBuf> {
    let mut effective = BTreeMap::<PathBuf, PathBuf>::new();
    for layer in layers {
        if let Ok(source) = fs::read_to_string(layer.join("descriptor.mod")) {
            if let Ok(root) = parse_pdx(&source) {
                for entry in root.iter().filter(|e| e.key == "replace_path") {
                    if let Some(replaced) = entry.atom() {
                        effective.retain(|path, _| !path.starts_with(replaced));
                    }
                }
            }
        }
        walk(
            &layer.join(relative),
            layer,
            recursive,
            extension,
            &mut effective,
        );
    }
    effective.into_values().collect()
}
fn read_file(
    path: &Path,
    db: &mut MilitaryCatalogue,
) -> Result<(Vec<Entry>, BTreeMap<String, String>), String> {
    let source = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hash);
    db.source_files
        .insert(path.display().to_string(), hash.finish());
    let root = parse_pdx(&source).map_err(|e| format!("{}: {e}", path.display()))?;
    let vars = variables(&root);
    Ok((root, vars))
}

#[derive(Debug, Clone)]
struct RawEquipment {
    id: String,
    archetype: Option<String>,
    is_archetype: bool,
    active: Option<bool>,
    buildable: bool,
    year: u32,
    stats: StatBlock,
    modules: BTreeMap<String, String>,
    requires_modules: bool,
    source: String,
}
#[derive(Debug, Clone, Default)]
struct Module {
    add: StatBlock,
    multiply: StatBlock,
    average: StatBlock,
}

pub fn import_units(source: &str, path: &str, db: &mut MilitaryCatalogue) -> Result<(), String> {
    let root = parse_pdx(source)?;
    let vars = variables(&root);
    for node in root
        .iter()
        .filter(|e| e.key == "sub_units")
        .filter_map(Entry::block)
        .flatten()
    {
        let Some(b) = node.block() else {
            continue;
        };
        let group = atom(b, "group").map(str::to_owned).or_else(|| {
            block(b, "group")
                .and_then(|b| b.first())
                .map(|e| e.key.clone())
        });
        if group.is_none() && find(b, "combat_width").is_none() {
            continue;
        }
        let group = group.unwrap_or_else(|| "infantry".into());
        let raw = stats(b, &vars);
        // An icon category is presentation metadata, not the unit's domain.
        // Keep land battalions even when a mod introduces a new icon name.
        if group != "support" && raw.get("combat_width").copied().unwrap_or(0.0) <= 0.0 {
            continue;
        }
        let mut terrain = BTreeMap::new();
        for entry in b {
            if let Some(tb) = entry.block() {
                if ["attack", "defense", "movement", "attrition"]
                    .iter()
                    .any(|k| find(tb, k).is_some())
                {
                    terrain.insert(
                        entry.key.clone(),
                        TerrainAdjustment {
                            attack: number(tb, "attack", 0.0),
                            defense: number(tb, "defense", 0.0),
                            movement: number(tb, "movement", 0.0),
                            attrition: number(tb, "attrition", 0.0),
                        },
                    );
                }
            }
        }
        if find(b, "battalion_mult").is_some() {
            db.diagnostics.push(format!(
                "{}: battalion_mult scaling needs 1.19 UI validation ({path})",
                node.key
            ));
        }
        let need = block(b, "need")
            .map(|b| stats(b, &vars))
            .unwrap_or_default();
        if need.values().any(|n| *n < 0.0) {
            return Err(format!("Negative equipment need in {}", node.key));
        }
        db.units.insert(
            node.key.clone(),
            BattalionDefinition {
                id: node.key.clone(),
                group: group.clone(),
                categories: block(b, "categories").map(names).unwrap_or_default(),
                active: yes(b, "active", false),
                divisional: yes(b, "divisional", !yes(b, "regimental", false)),
                regimental: yes(b, "regimental", false),
                affects_speed: yes(b, "affects_speed", group != "support"),
                stats: raw,
                need,
                terrain,
                same_support_type: b
                    .iter()
                    .filter(|e| e.key == "same_support_type")
                    .filter_map(|e| e.atom().map(str::to_owned))
                    .collect(),
                source: path.into(),
            },
        );
    }
    Ok(())
}

fn resolve_equipment(
    id: &str,
    raw: &BTreeMap<String, RawEquipment>,
    modules: &BTreeMap<String, Module>,
    resolved: &mut BTreeMap<String, EquipmentDefinition>,
    visiting: &mut BTreeSet<String>,
) -> Result<EquipmentDefinition, String> {
    if let Some(e) = resolved.get(id) {
        return Ok(e.clone());
    }
    if !visiting.insert(id.into()) {
        return Err(format!("Equipment inheritance cycle at {id}"));
    }
    let r = raw
        .get(id)
        .ok_or_else(|| format!("Missing equipment definition {id}"))?;
    let parent = if let Some(archetype) = r.archetype.as_ref() {
        Some(resolve_equipment(
            archetype, raw, modules, resolved, visiting,
        )?)
    } else {
        None
    };
    let mut values = parent.as_ref().map(|p| p.stats.clone()).unwrap_or_default();
    values.extend(r.stats.clone());
    let mut module_average = StatBlock::new();
    let mut module_multiply = StatBlock::new();
    let mut count = 0usize;
    let mut unsupported = false;
    for module in r
        .modules
        .values()
        .filter(|v| v.as_str() != "empty" && v.as_str() != "inherit")
    {
        if let Some(m) = modules.get(module) {
            count += 1;
            for (k, v) in &m.add {
                *values.entry(k.clone()).or_default() += v;
            }
            for (k, v) in &m.average {
                *module_average.entry(k.clone()).or_default() += v;
            }
            for (k, v) in &m.multiply {
                *module_multiply.entry(k.clone()).or_default() += v;
            }
        } else {
            unsupported = true;
        }
    }
    if count > 0 {
        for (k, v) in module_average {
            *values.entry(k).or_default() += v / count as f64;
        }
    }
    for (k, v) in module_multiply {
        *values.entry(k).or_default() *= (1.0 + v).max(0.0);
    }
    let modular = unsupported
        || ((r.requires_modules || parent.as_ref().is_some_and(|p| p.modular))
            && r.modules.is_empty());
    let result = EquipmentDefinition {
        id: r.id.clone(),
        archetype: parent
            .as_ref()
            .map(|p| p.archetype.clone())
            .unwrap_or_else(|| id.into()),
        year: r.year,
        active: r
            .active
            .unwrap_or_else(|| parent.as_ref().is_some_and(|p| p.active)),
        buildable: r.buildable && !r.is_archetype,
        cost: values.get("build_cost_ic").copied().unwrap_or(0.0),
        stats: values,
        modular,
        source: r.source.clone(),
    };
    visiting.remove(id);
    resolved.insert(id.into(), result.clone());
    Ok(result)
}

pub fn import_tactics(source: &str, db: &mut MilitaryCatalogue) -> Result<(), String> {
    let root = parse_pdx(source)?;
    let nodes: Vec<_> = root
        .iter()
        .flat_map(|entry| {
            if entry.key == "tactics" {
                entry.block().unwrap_or(&[]).iter().collect::<Vec<_>>()
            } else {
                vec![entry]
            }
        })
        .collect();
    for node in nodes {
        if node.key == "phases" {
            continue;
        }
        let Some(b) = node.block() else {
            continue;
        };
        if find(b, "is_attacker").is_none() {
            continue;
        }
        let base = block(b, "base").unwrap_or(&[]);
        let trigger = block(b, "trigger").unwrap_or(&[]).to_vec();
        let required_phase = atom(block(b, "trigger").unwrap_or(&[]), "phase")
            .filter(|v| *v != "no")
            .map(str::to_owned);
        let phase = atom(b, "phase").filter(|v| *v != "no").map(str::to_owned);
        let mut weight_modifiers = Vec::new();
        for modifier in base
            .iter()
            .filter(|e| e.key == "modifier")
            .filter_map(Entry::block)
        {
            weight_modifiers.push((
                modifier
                    .iter()
                    .filter(|e| e.key != "factor")
                    .cloned()
                    .collect(),
                number(modifier, "factor", 1.0),
            ));
        }
        db.tactics.retain(|t| t.id != node.key);
        db.tactics.push(Tactic {
            id: node.key.clone(),
            attacker: yes(b, "is_attacker", true),
            active: yes(b, "active", true),
            weight: number(base, "factor", 1.0),
            trigger,
            weight_modifiers,
            countered_by: atom(b, "countered_by").map(str::to_owned),
            phase,
            required_phase,
            attacker_bonus: number(b, "attacker", 0.0),
            defender_bonus: number(b, "defender", 0.0),
            width_bonus: number(b, "combat_width", 0.0),
            attacker_org_damage: number(b, "attacker_org_damage_modifier", 0.0),
            defender_org_damage: number(b, "defender_org_damage_modifier", 0.0),
            movement: number(b, "attacker_movement_speed", 0.0),
        });
    }
    Ok(())
}

pub fn import_technologies(source: &str, db: &mut MilitaryCatalogue) -> Result<(), String> {
    let root = parse_pdx(source)?;
    let vars = variables(&root);
    for node in root
        .iter()
        .filter(|e| e.key == "technologies")
        .filter_map(Entry::block)
        .flatten()
    {
        let Some(b) = node.block() else {
            continue;
        };
        let mut effect = TechnologyEffects::default();
        effect.country = block(b, "modifier")
            .map(|b| stats(b, &vars))
            .unwrap_or_default();
        for e in b {
            if let Some(raw) = e.atom().and_then(|v| scalar_value(v, &vars, 0)) {
                if e.key.ends_with("_factor")
                    || matches!(
                        e.key.as_str(),
                        "reinforce_rate" | "coordination" | "max_dig_in"
                    )
                {
                    *effect.country.entry(e.key.clone()).or_default() += raw;
                }
            }
            if let Some(eb) = e.block() {
                match e.key.as_str() {
                    "enable_subunits" | "enable_equipments" => {
                        if let Some(condition) = block(eb, "limit") {
                            effect.conditional_unlocks.push(ConditionalUnlock {
                                kind: if e.key == "enable_subunits" {
                                    UnlockKind::Unit
                                } else {
                                    UnlockKind::Equipment
                                },
                                conditions: condition.to_vec(),
                                ids: names(eb),
                            });
                            continue;
                        }
                        if e.key == "enable_subunits" {
                            effect.unlock_units.extend(names(eb));
                        } else {
                            effect.unlock_equipment.extend(names(eb));
                        }
                    }
                    "enable_tactic" => effect.unlock_tactics.extend(names(eb)),
                    "modifier" | "folder" | "path" | "categories" | "dependencies" | "xor"
                    | "ai_will_do" | "allow" | "allow_branch" => {}
                    key => {
                        let values = stats(eb, &vars);
                        if db.equipment.contains_key(key) {
                            effect.equipment.insert(key.into(), values);
                        } else if db.units.contains_key(key) || key.starts_with("category_") {
                            effect.units.insert(key.into(), values);
                        }
                    }
                }
            } else if e.key == "enable_tactic" {
                if let Some(id) = e.atom() {
                    effect.unlock_tactics.insert(id.into());
                }
            }
        }
        db.technologies.insert(node.key.clone(), effect);
    }
    Ok(())
}

pub fn load(root: &Path) -> MilitaryCatalogue {
    let mut db = MilitaryCatalogue::default();
    let mut layers = vec![root.to_path_buf()];
    if let Ok(profile) = fs::read_to_string(root.join("military_profile.txt")) {
        if let Ok(entries) = parse_pdx(&profile) {
            if let Some(extras) = block(&entries, "data_layers") {
                for entry in extras {
                    if entry.value.is_none() {
                        let extra = PathBuf::from(&entry.key);
                        layers.push(if extra.is_absolute() {
                            extra
                        } else {
                            root.join(extra)
                        });
                    }
                }
            }
            if let Some(version) = atom(&entries, "target_version") {
                db.rules.target_version = version.into();
            }
        }
    }
    for path in layered_files(&layers, "common/defines", false, "lua") {
        if let Ok(source) = fs::read_to_string(&path) {
            db.rules
                .import_defines(&source, &path.display().to_string());
        }
    }
    let mut modules = BTreeMap::new();
    for path in layered_files(&layers, "common/units/equipment/modules", true, "txt") {
        match read_file(&path, &mut db) {
            Ok((root, vars)) => {
                for node in root
                    .iter()
                    .filter(|e| e.key == "equipment_modules")
                    .filter_map(Entry::block)
                    .flatten()
                {
                    if let Some(b) = node.block() {
                        modules.insert(
                            node.key.clone(),
                            Module {
                                add: block(b, "add_stats")
                                    .map(|b| stats(b, &vars))
                                    .unwrap_or_default(),
                                multiply: block(b, "multiply_stats")
                                    .map(|b| stats(b, &vars))
                                    .unwrap_or_default(),
                                average: block(b, "add_average_stats")
                                    .map(|b| stats(b, &vars))
                                    .unwrap_or_default(),
                            },
                        );
                    }
                }
            }
            Err(e) => db.diagnostics.push(e),
        }
    }
    let mut raw = BTreeMap::new();
    for path in layered_files(&layers, "common/units/equipment", false, "txt") {
        match read_file(&path, &mut db) {
            Ok((root, vars)) => {
                for node in root
                    .iter()
                    .filter(|e| e.key == "equipments")
                    .filter_map(Entry::block)
                    .flatten()
                {
                    if let Some(b) = node.block() {
                        let module_values = block(b, "default_modules")
                            .map(|b| {
                                b.iter()
                                    .filter_map(|e| e.atom().map(|v| (e.key.clone(), v.into())))
                                    .collect()
                            })
                            .unwrap_or_default();
                        raw.insert(
                            node.key.clone(),
                            RawEquipment {
                                id: node.key.clone(),
                                archetype: atom(b, "archetype").map(str::to_owned),
                                is_archetype: yes(b, "is_archetype", false),
                                active: atom(b, "active").map(|v| v == "yes"),
                                buildable: yes(b, "is_buildable", true),
                                year: number(b, "year", 0.0).max(0.0) as u32,
                                stats: stats(b, &vars),
                                modules: module_values,
                                requires_modules: find(b, "module_slots")
                                    .is_some_and(|e| e.atom() != Some("none")),
                                source: path.display().to_string(),
                            },
                        );
                    }
                }
            }
            Err(e) => db.diagnostics.push(e),
        }
    }
    let mut resolved = BTreeMap::new();
    for id in raw.keys() {
        if let Err(e) = resolve_equipment(id, &raw, &modules, &mut resolved, &mut BTreeSet::new()) {
            db.diagnostics.push(e);
        }
    }
    db.equipment = resolved;
    for path in layered_files(&layers, "common/units", false, "txt") {
        match fs::read_to_string(&path) {
            Ok(source) => {
                if let Err(e) = import_units(&source, &path.display().to_string(), &mut db) {
                    db.diagnostics.push(e)
                }
            }
            Err(e) => db.diagnostics.push(e.to_string()),
        }
    }
    for path in layered_files(&layers, "common/terrain", false, "txt") {
        match read_file(&path, &mut db) {
            Ok((root, _)) => {
                for entry in root
                    .iter()
                    .filter(|e| e.key == "categories")
                    .filter_map(Entry::block)
                    .flatten()
                {
                    if let Some(b) = entry.block() {
                        let units = block(b, "units").unwrap_or(&[]);
                        let fallback = db.default_terrain(&entry.key);
                        db.terrains.insert(
                            entry.key.clone(),
                            TerrainDefinition {
                                width: number(b, "combat_width", fallback.width),
                                extra_width: number(
                                    b,
                                    "combat_support_width",
                                    fallback.extra_width,
                                ),
                                movement_cost: number(b, "movement_cost", 1.0).max(0.01),
                                attack: number(units, "attack", 0.0),
                                defense: number(units, "defense", 0.0),
                                attrition: number(b, "attrition", 0.0),
                            },
                        );
                    }
                }
            }
            Err(e) => db.diagnostics.push(e),
        }
    }
    let mut tactic_files = layered_files(&layers, "common/combat_tactics", false, "txt");
    for layer in &layers {
        let path = layer.join("common/combat_tactics.txt");
        if path.is_file() {
            tactic_files.push(path);
        }
    }
    for path in tactic_files {
        if let Ok(source) = fs::read_to_string(&path) {
            if let Err(e) = import_tactics(&source, &mut db) {
                db.diagnostics.push(e);
            }
        }
    }
    for path in layered_files(&layers, "common/modifiers", false, "txt") {
        match read_file(&path, &mut db) {
            Ok((root, vars)) => {
                for entry in root {
                    if let Some(b) = entry.block() {
                        db.weather.insert(entry.key.clone(), stats(b, &vars));
                    }
                }
            }
            Err(e) => db.diagnostics.push(e),
        }
    }
    for path in layered_files(&layers, "common/technologies", false, "txt") {
        if let Ok(source) = fs::read_to_string(&path) {
            if let Err(e) = import_technologies(&source, &mut db) {
                db.diagnostics.push(e);
            }
        }
    }
    if db.units.is_empty() {
        db.diagnostics
            .push("No land unit definitions: add common/units/*.txt to enable armies".into());
    }
    if db.equipment.is_empty() {
        db.diagnostics
            .push("No equipment definitions: add common/units/equipment/*.txt".into());
    }
    if db.rules.pixels_to_km.is_none() {
        db.diagnostics.push(
            "MAP_SCALE_PIXEL_TO_KM is missing: movement requires a map-distance calibration".into(),
        );
    }
    if db.terrains.is_empty() {
        db.diagnostics.push(
            "Terrain penalties unavailable; only brief combat-width defaults are available".into(),
        );
    }
    db.diagnostics.extend(db.rules.diagnostics.clone());
    db.diagnostics.push(
        "Compatibility: reconstructed rules; original engine RNG/rounding parity is unverified"
            .into(),
    );
    db.diagnostics.push("Supply, fuel, weather and air are resolved scenario inputs; no upstream logistics/air simulation yet".into());
    db.diagnostics.push("1.19 mastery/doctrines, battalion_mult, custom variants and attrition require further import/validation".into());
    db.diagnostics.push("Operations: manual orders and basic AI; outgoing attacks are interrupted when their origin is attacked".into());
    db
}

pub fn parse_template(b: &[Entry]) -> Result<DivisionTemplate, String> {
    let mut t = DivisionTemplate {
        name: atom(b, "name").ok_or("Template has no name")?.into(),
        locked: yes(b, "is_locked", false),
        ..Default::default()
    };
    for unit in block(b, "regiments").unwrap_or(&[]) {
        if let Some(position) = unit.block() {
            t.line.push(BattalionSlot {
                unit: unit.key.clone(),
                column: number(position, "x", 0.0).max(0.0) as usize,
                row: number(position, "y", 0.0).max(0.0) as usize,
            });
        }
    }
    for (field, regimental) in [("support", false), ("regimental_support", true)] {
        for unit in block(b, field).unwrap_or(&[]) {
            if let Some(position) = unit.block() {
                t.support.push(SupportSlot {
                    unit: unit.key.clone(),
                    regiment: regimental.then(|| number(position, "x", 0.0).max(0.0) as usize),
                    row: number(position, "y", 0.0).max(0.0) as usize,
                });
            }
        }
    }
    Ok(t)
}

#[derive(Debug, Clone)]
pub struct StartingDivision {
    pub name: String,
    pub template: String,
    pub province: u32,
    pub experience: f64,
    pub equipment_factor: f64,
    pub manpower_factor: f64,
}
#[derive(Debug, Default)]
pub struct OrderOfBattle {
    pub templates: Vec<DivisionTemplate>,
    pub divisions: Vec<StartingDivision>,
    pub diagnostics: Vec<String>,
    pub warnings: Vec<String>,
    pub declared_divisions: usize,
}
pub fn parse_oob(source: &str) -> Result<OrderOfBattle, String> {
    parse_oob_for_country(
        source,
        "",
        &Default::default(),
        crate::game::GameDate::START,
    )
}
pub fn parse_oob_for_country(
    source: &str,
    tag: &str,
    modifiers: &MilitaryModifiers,
    today: crate::game::GameDate,
) -> Result<OrderOfBattle, String> {
    let parsed = crate::pdx::parse_pdx_compatible(source)?;
    let (root, diagnostics) =
        crate::history_conditions::resolve(&parsed.entries, tag, modifiers, today);
    let warnings = parsed
        .eof_terminated_blocks
        .into_iter()
        .map(|span| {
            format!(
                "{tag}: OOB outer block at {}:{} ends at EOF",
                span.line, span.column
            )
        })
        .collect();
    let mut result = OrderOfBattle {
        diagnostics,
        warnings,
        ..Default::default()
    };
    let default_equipment = number(&root, "start_equipment_factor", 1.0).clamp(0.0, 1.0);
    for entry in &root {
        if entry.key == "division_template" {
            if let Some(b) = entry.block() {
                match parse_template(b) {
                    Ok(t) => result.templates.push(t),
                    Err(e) => result.diagnostics.push(e),
                }
            }
        }
        if entry.key == "units" {
            for unit in entry
                .block()
                .unwrap_or(&[])
                .iter()
                .filter(|e| e.key == "division")
            {
                result.declared_divisions += 1;
                let Some(b) = unit.block() else {
                    continue;
                };
                if find(b, "force_equipment_variants").is_some() {
                    result.diagnostics.push(
                        "OOB forces equipment variants; variant assignments require validation"
                            .into(),
                    );
                }
                let Some(template) = atom(b, "division_template") else {
                    result
                        .diagnostics
                        .push("Division has no template; remaining OOB records retained".into());
                    continue;
                };
                let name = atom(b, "name")
                    .map(str::to_owned)
                    .or_else(|| {
                        block(b, "division_name").and_then(|n| {
                            atom(n, "name").map(str::to_owned).or_else(|| {
                                atom(n, "name_order").map(|order| format!("{order}. {template}"))
                            })
                        })
                    })
                    .unwrap_or_else(|| format!("{template} #{}", result.divisions.len() + 1));
                result.divisions.push(StartingDivision {
                    name,
                    template: template.into(),
                    province: number(b, "location", 0.0).max(0.0) as u32,
                    experience: number(b, "start_experience_factor", 0.1).clamp(0.0, 1.0),
                    equipment_factor: number(b, "start_equipment_factor", default_equipment)
                        .clamp(0.0, 1.0),
                    manpower_factor: number(b, "start_manpower_factor", 1.0).clamp(0.0, 1.0),
                });
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_variables_equipment_inheritance_and_unit_multipliers() {
        let mut db = MilitaryCatalogue::default();
        let raw = BTreeMap::from([
            (
                "rifle".into(),
                RawEquipment {
                    id: "rifle".into(),
                    archetype: None,
                    is_archetype: true,
                    active: Some(true),
                    buildable: false,
                    year: 0,
                    stats: StatBlock::from([
                        ("soft_attack".into(), 10.0),
                        ("build_cost_ic".into(), 1.0),
                    ]),
                    modules: BTreeMap::new(),
                    requires_modules: false,
                    source: "t".into(),
                },
            ),
            (
                "rifle_1".into(),
                RawEquipment {
                    id: "rifle_1".into(),
                    archetype: Some("rifle".into()),
                    is_archetype: false,
                    active: None,
                    buildable: true,
                    year: 1936,
                    stats: StatBlock::from([("soft_attack".into(), 12.0)]),
                    modules: BTreeMap::new(),
                    requires_modules: false,
                    source: "t".into(),
                },
            ),
        ]);
        let mut resolved = BTreeMap::new();
        resolve_equipment(
            "rifle_1",
            &raw,
            &BTreeMap::new(),
            &mut resolved,
            &mut BTreeSet::new(),
        )
        .unwrap();
        db.equipment = resolved;
        import_units("@men = 1000 sub_units = { infantry = { group = infantry map_icon_category = custom_land_icon active = yes manpower = @men max_strength = 25 max_organisation = 60 combat_width = 2 need = { rifle = 100 } soft_attack = -0.5 } }","test",&mut db).unwrap();
        let t = DivisionTemplate {
            name: "Inf".into(),
            line: vec![BattalionSlot {
                unit: "infantry".into(),
                column: 0,
                row: 0,
            }],
            ..Default::default()
        };
        let d = db
            .aggregate(&t, &MilitaryModifiers::default(), None)
            .unwrap();
        assert_eq!(d.stats.soft_attack, 6.0);
        assert_eq!(d.stats.manpower, 1000.0);
        assert_eq!(d.equipment_cost, 100.0);
    }
    #[test]
    fn oob_preserves_regimental_positions_and_independent_factors() {
        let oob=parse_oob("division_template = { name = T regiments = { infantry = { x = 0 y = 0 } } regimental_support = { r = { x = 0 y = 2 } } }
            start_equipment_factor=0.8 units = { division = { name=D division_template=T location=11 start_manpower_factor=0.6 } }").unwrap();
        assert_eq!(oob.templates[0].support[0].regiment, Some(0));
        assert_eq!(oob.templates[0].support[0].row, 2);
        assert_eq!(oob.divisions[0].equipment_factor, 0.8);
        assert_eq!(oob.divisions[0].manpower_factor, 0.6);
    }
    #[test]
    fn tactics_read_actual_root_schema_and_technology_effects() {
        let mut db = MilitaryCatalogue::default();
        import_tactics("phases = { default } tactic_attack = { is_attacker=yes active=no base={factor=2 modifier={factor=3 is_attacker=yes}} attacker=0.25 countered_by=defense combat_width=-0.25 }",&mut db).unwrap();
        assert_eq!(db.tactics[0].attacker_bonus, 0.25);
        assert_eq!(db.tactics[0].width_bonus, -0.25);
        assert!(!db.tactics[0].active);
        import_technologies("technologies={ radio={ modifier={reinforce_rate=0.05} enable_tactic=tactic_attack enable_equipments={rifle} } }",&mut db).unwrap();
        let m = MilitaryModifiers::from_research(&db, &BTreeSet::from(["radio".into()]));
        assert_eq!(m.value("reinforce_rate"), 0.05);
        assert!(m.unlocked_equipment.contains("rifle"));
    }
}
