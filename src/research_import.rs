//! Versioned local import, cached static registry, all-country scenario audit and snapshots.
use crate::game::GameDate;
use crate::pdx::{self, Entry, atom, block, names, parse_pdx};
use crate::research::*;
use crate::research_script::{self, ResearchScripts, ScriptSource, SharingGroup};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
const CACHE_VERSION: u32 = 6;
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Installation {
    pub version: Option<String>,
    #[serde(default)]
    pub version_source: Option<String>,
    #[serde(default)]
    pub executable_version: Option<String>,
    pub checksum: Option<String>,
    pub executable_sha256: Option<String>,
    pub enabled_dlcs: Vec<String>,
    pub installed_dlcs: Vec<String>,
    pub dlc_selection_verified: bool,
    #[serde(default)]
    pub dlc_selection_source: Option<String>,
    pub data_layers: Vec<String>,
    pub load_order_verified: bool,
    pub scenario: Option<String>,
    pub exported_at: Option<String>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileStamp {
    pub path: String,
    pub size: u64,
    pub modified: u128,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchDataset {
    pub cache_version: u32,
    pub registry: TechnologyRegistry,
    pub rules: ResearchRules,
    pub histories: BTreeMap<String, Vec<Entry>>,
    pub installation: Installation,
    pub files: Vec<FileStamp>,
    pub diagnostics: Vec<String>,
    pub from_cache: bool,
    pub bookmarks: Vec<GameDate>,
    pub source_technology_ids: BTreeSet<String>,
    pub failed_source_files: BTreeSet<String>,
    pub rule_sources: BTreeMap<String, String>,
    #[serde(default)]
    pub define_values: BTreeMap<String, f64>,
    #[serde(default)]
    pub source_country_ids: BTreeSet<String>,
    #[serde(default)]
    pub eof_terminated_sources: BTreeMap<String, Vec<pdx::Span>>,
    #[serde(default)]
    pub history_source_files: BTreeMap<String, String>,
}
pub fn game_version(root: &Path) -> Option<(String, String)> {
    let source = fs::read_to_string(root.join("launcher-settings.json")).ok()?;
    let value: serde_json::Value =
        serde_json::from_str(source.trim_start_matches('\u{feff}')).ok()?;
    for key in ["rawVersion", "gameVersion", "version"] {
        if let Some(version) = value
            .get(key)
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
        {
            return Some((version.into(), format!("launcher-settings.json:{key}")));
        }
    }
    None
}
const DIRECTORIES: &[(&str, &str)] = &[
    ("common/technologies", "txt"),
    ("common/technology_tags", "txt"),
    ("common/technology_sharing", "txt"),
    ("common/defines", "lua"),
    ("common/scripted_triggers", "txt"),
    ("common/scripted_effects", "txt"),
    ("common/ideas", "txt"),
    ("common/characters", "txt"),
    ("common/national_focus", "txt"),
    ("common/decisions", "txt"),
    ("common/on_actions", "txt"),
    ("events", "txt"),
    ("history/countries", "txt"),
    ("common/bookmarks", "txt"),
    ("common/special_projects", "txt"),
    ("common/doctrines", "txt"),
    ("localisation", "yml"),
    ("interface", "gui"),
];
fn walk(
    root: &Path,
    relative: &str,
    ext: &str,
    out: &mut BTreeMap<String, PathBuf>,
) -> Result<(), String> {
    let dir = root.join(relative);
    if !dir.is_dir() {
        return Ok(());
    }
    for e in fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = e.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            walk(root, &rel, ext, out)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some(ext) {
            out.insert(
                path.strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/"),
                path,
            );
        }
    }
    Ok(())
}
pub fn installation(root: &Path) -> Installation {
    fs::read(root.join("research-import.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_else(|| {
            let mut install = Installation::default();
            if let Some(profile) = fs::read_to_string(root.join("military_profile.txt"))
                .ok()
                .and_then(|s| parse_pdx(&s).ok())
            {
                if let Some(dlcs) = block(&profile, "dlcs") {
                    install.enabled_dlcs = names(dlcs).into_iter().collect();
                    install.dlc_selection_verified = true;
                }
                if let Some(layers) = block(&profile, "data_layers") {
                    install.data_layers = layers.iter().map(|e| e.key.clone()).collect();
                    install.load_order_verified = true;
                }
            }
            install
        })
}
/// Compatibility for the imported 1.15 branch: these former DLC are part of
/// the base game. Their legacy has_dlc checks still occur in the source data.
/// Do not apply a modern entitlement policy to unknown or other versions.
pub fn base_game_dlcs(install: &Installation) -> BTreeSet<String> {
    let Some(version) = install
        .version
        .as_deref()
        .filter(|_| install.version_source.is_some())
    else {
        return BTreeSet::new();
    };
    let mut parts = version.split('.');
    if parts.next() != Some("1")
        || parts.next() != Some("15")
        || parts.next().and_then(|s| s.parse::<u32>().ok()).is_none()
    {
        return BTreeSet::new();
    }
    [
        "Together for Victory",
        "Death or Dishonor",
        "Waking the Tiger",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
pub fn profile_context(root: &Path) -> ResearchContext {
    let mut ctx = ResearchContext::default();
    let install = installation(root);
    ctx.base_game_dlcs = base_game_dlcs(&install);
    if root.join("research-import.json").is_file() {
        ctx.dlcs = install.enabled_dlcs.into_iter().collect();
        ctx.dlc_selection_unverified = !install.dlc_selection_verified;
    } else if let Some(b) = fs::read_to_string(root.join("military_profile.txt"))
        .ok()
        .and_then(|s| parse_pdx(&s).ok())
    {
        ctx.dlcs = block(&b, "dlcs").map(names).unwrap_or_default();
    }
    ctx.dlcs.extend(ctx.base_game_dlcs.iter().cloned());
    ctx
}
pub fn effective_files(
    root: &Path,
    install: &Installation,
) -> Result<BTreeMap<String, PathBuf>, String> {
    effective_files_in(root, install, DIRECTORIES)
}
fn effective_files_in(
    root: &Path,
    install: &Installation,
    directories: &[(&str, &str)],
) -> Result<BTreeMap<String, PathBuf>, String> {
    let mut out = BTreeMap::new();
    let mut layers = vec![root.to_path_buf()];
    if install.data_layers.is_empty() {
        if let Some(b) = fs::read_to_string(root.join("military_profile.txt"))
            .ok()
            .and_then(|s| parse_pdx(&s).ok())
        {
            for e in block(&b, "data_layers").unwrap_or(&[]) {
                let p = PathBuf::from(&e.key);
                layers.push(if p.is_absolute() { p } else { root.join(p) });
            }
        }
    } else {
        for p in &install.data_layers {
            let p = PathBuf::from(p);
            layers.push(if p.is_absolute() { p } else { root.join(p) });
        }
    }
    for layer in layers {
        if let Some(b) = fs::read_to_string(layer.join("descriptor.mod"))
            .ok()
            .and_then(|s| parse_pdx(&s).ok())
        {
            for replaced in b
                .iter()
                .filter(|e| e.key == "replace_path")
                .filter_map(Entry::atom)
            {
                out.retain(|p: &String, _| !Path::new(p).starts_with(replaced));
            }
        }
        for (rel, ext) in directories {
            walk(&layer, rel, ext, &mut out)?;
        }
        // Modern doctrine folders/tracks/subdoctrines and specialization data are not ordinary techs.
        if let Ok(entries) = fs::read_dir(layer.join("common")) {
            for e in entries.filter_map(Result::ok) {
                let n = e.file_name().to_string_lossy().to_string();
                if n.starts_with("doctrine_") || n.starts_with("special_") {
                    walk(&layer, &format!("common/{n}"), "txt", &mut out)?;
                }
            }
        }
    }
    Ok(out)
}
fn stamps(files: &BTreeMap<String, PathBuf>) -> Result<Vec<FileStamp>, String> {
    files
        .iter()
        .map(|(rel, path)| {
            let m = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(FileStamp {
                path: format!("{rel}|{}", path.display()),
                size: m.len(),
                modified: m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_nanos()),
            })
        })
        .collect()
}
fn fingerprint(data: &[u8]) -> String {
    let mut h = 0xcbf29ce484222325u64;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}
fn source_nodes<'a>(b: &'a [Entry], key: &str) -> Vec<&'a Entry> {
    let mut out = Vec::new();
    for e in b {
        if e.key == key {
            out.push(e);
        }
        if let Some(n) = e.block() {
            out.extend(source_nodes(n, key));
        }
    }
    out
}
fn collect_sources(b: &[Entry], kind: &str, path: &str, scripts: &mut ResearchScripts) {
    match kind {
        "focus" => {
            for e in source_nodes(b, "focus")
                .into_iter()
                .chain(source_nodes(b, "shared_focus"))
            {
                let Some(raw) = e.block() else {
                    continue;
                };
                let Some(id) = atom(raw, "id") else {
                    continue;
                };
                scripts.sources.insert(
                    format!("focus:{id}"),
                    ScriptSource {
                        kind: kind.into(),
                        id: id.into(),
                        source: format!("{path}:{}", e.span.line),
                        conditions: block(raw, "available").unwrap_or(&[]).to_vec(),
                        effects: block(raw, "completion_reward").unwrap_or(&[]).to_vec(),
                        raw: raw.to_vec(),
                    },
                );
            }
        }
        "event" => {
            for e in b
                .iter()
                .filter(|e| matches!(e.key.as_str(), "country_event" | "news_event"))
            {
                let Some(raw) = e.block() else {
                    continue;
                };
                let Some(id) = atom(raw, "id") else {
                    continue;
                };
                if let Some(effects) = block(raw, "immediate") {
                    scripts.sources.insert(
                        format!("event:{id}:immediate"),
                        ScriptSource {
                            kind: kind.into(),
                            id: format!("{id}:immediate"),
                            source: format!("{path}:{}", e.span.line),
                            conditions: block(raw, "trigger").unwrap_or(&[]).to_vec(),
                            effects: effects.to_vec(),
                            raw: raw.to_vec(),
                        },
                    );
                }
                for (n, option) in raw.iter().filter(|e| e.key == "option").enumerate() {
                    let Some(opt) = option.block() else {
                        continue;
                    };
                    let name = atom(opt, "name")
                        .map(str::to_owned)
                        .unwrap_or_else(|| n.to_string());
                    let mut conditions = block(raw, "trigger").unwrap_or(&[]).to_vec();
                    conditions.extend_from_slice(block(opt, "trigger").unwrap_or(&[]));
                    scripts.sources.insert(
                        format!("event:{id}:{name}"),
                        ScriptSource {
                            kind: kind.into(),
                            id: format!("{id}:{name}"),
                            source: format!("{path}:{}", option.span.line),
                            conditions,
                            effects: opt
                                .iter()
                                .filter(|e| {
                                    !matches!(
                                        e.key.as_str(),
                                        "name"
                                            | "trigger"
                                            | "ai_chance"
                                            | "highlighted"
                                            | "default_option"
                                            | "log"
                                            | "fire_only_once"
                                    )
                                })
                                .cloned()
                                .collect(),
                            raw: opt.to_vec(),
                        },
                    );
                }
            }
        }
        "decision" => {
            for group in b {
                for e in group.block().unwrap_or(&[]) {
                    if let Some(raw) = e.block() {
                        if let Some(effects) = block(raw, "complete_effect") {
                            let mut conditions = block(raw, "visible").unwrap_or(&[]).to_vec();
                            conditions.extend_from_slice(block(raw, "available").unwrap_or(&[]));
                            scripts.sources.insert(
                                format!("decision:{}", e.key),
                                ScriptSource {
                                    kind: kind.into(),
                                    id: e.key.clone(),
                                    source: format!("{path}:{}", e.span.line),
                                    conditions,
                                    effects: effects.to_vec(),
                                    raw: raw.to_vec(),
                                },
                            );
                        }
                    }
                }
            }
        }
        _ => {}
    }
}
impl ResearchDataset {
    pub fn load(root: &Path, use_cache: bool) -> Result<Self, String> {
        let install = if root.join("research-import.json").is_file() {
            serde_json::from_slice(
                &fs::read(root.join("research-import.json")).map_err(|e| e.to_string())?,
            )
            .map_err(|e| format!("research-import.json: {e}"))?
        } else {
            installation(root)
        };
        let files = effective_files(root, &install)?;
        let mut inputs = stamps(&files)?;
        for name in ["research-import.json", "military_profile.txt"] {
            let p = root.join(name);
            if p.is_file() {
                inputs.extend(stamps(&BTreeMap::from([(name.into(), p)]))?);
            }
        }
        let cache = root.join("cache/research-registry-v6.bin");
        if use_cache {
            if let Ok(mut cached) = Self::read_compiled(&cache) {
                if cached.cache_version == CACHE_VERSION && cached.files == inputs {
                    cached.from_cache = true;
                    return Ok(cached);
                }
            }
        }
        let mut data = Self {
            cache_version: CACHE_VERSION,
            registry: ResearchCatalogue::default(),
            rules: ResearchRules::default(),
            histories: BTreeMap::new(),
            installation: install,
            files: inputs,
            diagnostics: Vec::new(),
            from_cache: false,
            bookmarks: Vec::new(),
            source_technology_ids: BTreeSet::new(),
            failed_source_files: BTreeSet::new(),
            rule_sources: BTreeMap::new(),
            define_values: BTreeMap::new(),
            source_country_ids: BTreeSet::new(),
            eof_terminated_sources: BTreeMap::new(),
            history_source_files: BTreeMap::new(),
        };
        let mut roots = BTreeMap::new();
        let mut define_sources = BTreeMap::new();
        for (rel, path) in &files {
            if rel.ends_with(".lua") {
                let source = fs::read_to_string(path).map_err(|e| format!("{rel}: {e}"))?;
                let defines = crate::defines::read(&source);
                for (key, value) in &defines.values {
                    data.define_values.insert(key.clone(), *value);
                    define_sources.insert(key.clone(), format!("{rel}:{key}"));
                }
                for key in &defines.unresolved {
                    data.define_values.remove(key);
                    define_sources.remove(key);
                    if key.starts_with("NDefines.NTechnology.")
                        || key == "NDefines.NCountry.BASE_RESEARCH_SLOTS"
                    {
                        data.diagnostics
                            .push(format!("{rel}: не вычислено Lua-выражение {key}"));
                    }
                }
                continue;
            }
            if rel.ends_with(".yml") || rel.ends_with(".gui") {
                continue;
            }
            if rel.starts_with("history/countries/") {
                if let Some(tag) = Path::new(rel)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.split([' ', '-', '.']).next())
                    .filter(|s| s.len() >= 3)
                {
                    data.source_country_ids.insert(tag.into());
                }
            }
            let source = fs::read_to_string(path).map_err(|e| format!("{rel}: {e}"))?;
            match pdx::parse_pdx_compatible(&source) {
                Ok(parsed) => {
                    data.record_eof_sources(rel, &parsed.eof_terminated_blocks);
                    let b = parsed.entries;
                    let resolved = if rel.starts_with("common/technologies/") {
                        b
                    } else {
                        crate::script_constants::resolve(&b, &Default::default())
                    };
                    roots.insert(rel.clone(), resolved);
                }
                Err(e) => {
                    data.failed_source_files.insert(rel.clone());
                    data.diagnostics.push(format!("{rel}: {e}"));
                }
            }
        }
        for (path, value) in &data.define_values {
            let one = crate::defines::LiteralDefines {
                values: BTreeMap::from([(path.clone(), *value)]),
                unresolved: BTreeSet::new(),
                arrays: BTreeMap::new(),
            };
            for key in crate::research_data::update_rules_from_define_values(&one, &mut data.rules)
            {
                data.rule_sources.insert(key, define_sources[path].clone());
            }
        }
        for (rel, b) in &roots {
            let scripts = &mut data.registry.scripts;
            if rel.starts_with("common/technologies/") {
                data.source_technology_ids.extend(
                    b.iter()
                        .filter(|e| e.key == "technologies")
                        .filter_map(Entry::block)
                        .flatten()
                        .filter(|e| !e.key.starts_with('@'))
                        .map(|e| e.key.clone()),
                );
            }
            if rel.starts_with("common/technology_tags/") {
                data.registry.category_tags.extend(
                    block(b, "technology_categories")
                        .map(names)
                        .unwrap_or_default(),
                );
                for e in block(b, "technology_folders").unwrap_or(&[]) {
                    if !data.registry.folder_order.contains(&e.key) {
                        data.registry.folder_order.push(e.key.clone());
                    }
                    data.registry
                        .folder_definitions
                        .insert(e.key.clone(), e.block().unwrap_or(&[]).to_vec());
                }
            }
            if rel.starts_with("common/scripted_triggers/") {
                for e in b {
                    if let Some(b) = e.block() {
                        scripts.triggers.insert(e.key.clone(), b.to_vec());
                    }
                }
            }
            if rel.starts_with("common/scripted_effects/") {
                for e in b {
                    if let Some(b) = e.block() {
                        scripts.effects.insert(e.key.clone(), b.to_vec());
                    }
                }
            }
            if rel.starts_with("common/ideas/") {
                for group in block(b, "ideas").unwrap_or(&[]) {
                    for idea in group.block().unwrap_or(&[]) {
                        if let Some(raw) = idea.block() {
                            scripts.ideas.insert(idea.key.clone(), raw.to_vec());
                        }
                    }
                }
            }
            if rel.starts_with("common/characters/") {
                for character in block(b, "characters").unwrap_or(&[]) {
                    if let Some(raw) = character.block() {
                        for advisor in raw.iter().filter(|e| e.key == "advisor") {
                            if let Some(raw) = advisor.block() {
                                let id = atom(raw, "idea_token").unwrap_or(&character.key);
                                scripts.ideas.insert(id.into(), raw.to_vec());
                            }
                        }
                    }
                }
            }
            if rel.starts_with("common/technology_sharing/") {
                for e in b {
                    if let Some(raw) = e.block() {
                        if let Some(id) = atom(raw, "id") {
                            scripts.sharing.insert(
                                id.into(),
                                SharingGroup {
                                    id: id.into(),
                                    per_country: pdx::number(
                                        raw,
                                        "research_sharing_per_country_bonus",
                                    )
                                    .unwrap_or(0.0),
                                    categories: block(raw, "categories")
                                        .map(names)
                                        .unwrap_or_default(),
                                    available: block(raw, "available").unwrap_or(&[]).to_vec(),
                                    faction: atom(raw, "is_faction_sharing") == Some("yes"),
                                },
                            );
                        }
                    }
                }
            }
            if rel.starts_with("common/national_focus/") {
                collect_sources(b, "focus", rel, scripts);
            }
            if rel.starts_with("events/") {
                collect_sources(b, "event", rel, scripts);
            }
            if rel.starts_with("common/decisions/") {
                collect_sources(b, "decision", rel, scripts);
            }
            if rel.starts_with("common/on_actions/") {
                for e in block(b, "on_actions").unwrap_or(b) {
                    if let Some(raw) = e.block() {
                        scripts.sources.insert(
                            format!("on_action:{}", e.key),
                            ScriptSource {
                                kind: "on_action".into(),
                                id: e.key.clone(),
                                source: format!("{rel}:{}", e.span.line),
                                conditions: Vec::new(),
                                effects: block(raw, "effect").unwrap_or(&[]).to_vec(),
                                raw: raw.to_vec(),
                            },
                        );
                    }
                }
            }
            if rel.starts_with("history/countries/") {
                if let Some(name) = Path::new(rel).file_name().and_then(|s| s.to_str()) {
                    let tag = name.split([' ', '-', '.']).next().unwrap_or("");
                    if tag.len() >= 3 {
                        data.histories.insert(tag.into(), b.to_vec());
                        data.history_source_files.insert(tag.into(), rel.clone());
                    }
                }
            }
            if rel.starts_with("common/bookmarks/") {
                for e in source_nodes(b, "date") {
                    if let Some(date) = e.atom().and_then(research_script::parse_date) {
                        if !data.bookmarks.contains(&date) {
                            data.bookmarks.push(date);
                        }
                    }
                }
            }
            if rel.starts_with("common/doctrine_") || rel.starts_with("common/special_projects/") {
                for e in b {
                    data.registry.scripts.sources.insert(
                        format!("definition:{rel}:{}", e.key),
                        ScriptSource {
                            kind: "separate_research_definition".into(),
                            id: e.key.clone(),
                            source: rel.clone(),
                            conditions: Vec::new(),
                            effects: Vec::new(),
                            raw: vec![e.clone()],
                        },
                    );
                }
                data.diagnostics.push(format!("{rel}: определение отдельной подсистемы сохранено; её игровой цикл не реализован"));
            }
        }
        for (rel, path) in files
            .iter()
            .filter(|(r, _)| r.starts_with("common/technologies/"))
        {
            let source = fs::read_to_string(path).map_err(|e| e.to_string())?;
            if let Err(e) =
                crate::research_data::import_technology_source(&source, rel, &mut data.registry)
            {
                data.diagnostics.push(e);
            }
        }
        for (rel, path) in files.iter().filter(|(rel, _)| {
            rel.ends_with(".gui")
                && (rel.contains("tech")
                    || rel.contains("research")
                    || rel.contains("doctrine")
                    || rel.contains("special_project"))
        }) {
            match fs::read_to_string(path)
                .map_err(|e| e.to_string())
                .and_then(|s| pdx::parse_pdx_compatible(&s))
            {
                Ok(parsed) => {
                    data.record_eof_sources(rel, &parsed.eof_terminated_blocks);
                    let raw = parsed.entries;
                    data.registry.scripts.sources.insert(
                        format!("gui:{rel}"),
                        ScriptSource {
                            kind: "interface_geometry".into(),
                            id: rel.clone(),
                            source: rel.clone(),
                            conditions: Vec::new(),
                            effects: Vec::new(),
                            raw,
                        },
                    );
                }
                Err(e) => {
                    data.failed_source_files.insert(rel.clone());
                    data.diagnostics.push(format!("{rel}: {e}"));
                }
            }
        }
        for key in [
            "BASE_TECH_COST",
            "BASE_YEAR_AHEAD_PENALTY_FACTOR",
            "BASE_RESEARCH_POINTS_SAVED",
            "BASE_RESEARCH_SLOTS",
        ] {
            if !data.rule_sources.contains_key(key) {
                data.diagnostics.push(if key=="BASE_RESEARCH_SLOTS" {"BASE_RESEARCH_SLOTS: базовое число ячеек неизвестно; нужен define или set_research_slots страны".into()}else{format!("{key}: параметр не прочитан; используется непроверенный базовый коэффициент")});
            }
        }
        let scripts = &data.registry.scripts;
        for t in data.registry.technologies.values_mut() {
            if let Requirement::Script(b) = &mut t.allow {
                *b = research_script::expand_triggers(b, scripts, 0);
            }
            if let Requirement::Script(b) = &mut t.allow_branch {
                *b = research_script::expand_triggers(b, scripts, 0);
            }
            t.on_complete_limit =
                research_script::expand_triggers(&t.on_complete_limit, scripts, 0);
            for effect in &mut t.effects {
                if let TechnologyEffect::Unlock { limit, .. } = effect {
                    *limit = research_script::expand_triggers(limit, scripts, 0);
                }
            }
        }
        for t in data.registry.technologies.values_mut() {
            for (field, requirement) in [("allow", &t.allow), ("allow_branch", &t.allow_branch)] {
                if let Requirement::Script(b) = requirement {
                    for predicate in research_script::unsupported_predicates(b) {
                        t.unsupported.insert(format!("{field}:{predicate}"));
                    }
                }
            }
            for predicate in research_script::unsupported_predicates(&t.on_complete_limit) {
                t.unsupported
                    .insert(format!("on_research_complete_limit:{predicate}"));
            }
        }
        apply_localisations(&files, &mut data.registry, &mut data.diagnostics);
        data.registry.rebuild_indices();
        if let Err(e) = data.registry.validate() {
            data.diagnostics.push(e);
        }
        for t in data.registry.technologies.values() {
            for p in &t.paths {
                if !p.coefficient_known {
                    data.diagnostics.push(format!(
                        "{}: неизвестный research_cost_coeff пути к {}",
                        t.source, p.target
                    ));
                }
                if !data.registry.technologies.contains_key(&p.target) {
                    data.diagnostics
                        .push(format!("{}: unresolved path to {}", t.source, p.target));
                }
            }
            for field in &t.unsupported {
                data.diagnostics
                    .push(format!("{}: частичная поддержка поля {field}", t.source));
            }
        }
        data.diagnostics.extend(data.registry.diagnostics.clone());
        data.registry.fingerprint = fingerprint(
            &serde_json::to_vec(&(
                &data.registry.technologies,
                &data.registry.scripts,
                &data.installation,
                &data.rules,
                &data.histories,
                &data.registry.folder_definitions,
            ))
            .map_err(|e| e.to_string())?,
        );
        if data.registry.technologies.is_empty() {
            return Err(format!(
                "Технологии не импортированы. Запустите tools/Export-Hoi4Research.ps1. {}",
                data.diagnostics.join("; ")
            ));
        }
        fs::create_dir_all(root.join("cache")).map_err(|e| e.to_string())?;
        if let Err(e) = data.write_compiled(&cache) {
            data.diagnostics.push(format!("Кэш не записан: {e}"));
        }
        Ok(data)
    }
    fn record_eof_sources(&mut self, path: &str, spans: &[pdx::Span]) {
        if spans.is_empty() {
            return;
        }
        self.eof_terminated_sources
            .insert(path.into(), spans.to_vec());
        for at in spans {
            self.diagnostics.push(format!("{path}: внешний блок {}:{} завершён концом файла; содержимое сохранено, исходник не изменён",at.line,at.column));
        }
    }
    pub fn read_compiled(path: &Path) -> Result<Self, String> {
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        let payload = bytes
            .strip_prefix(b"HOI4RESEARCH\0V6\0")
            .ok_or("Неизвестный формат реестра")?;
        let json =
            miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(payload, 512 * 1024 * 1024)
                .map_err(|e| format!("Повреждённый кэш: {e:?}"))?;
        let data: Self = serde_json::from_slice(&json).map_err(|e| e.to_string())?;
        if data.cache_version != CACHE_VERSION {
            return Err("Неподдерживаемая версия реестра".into());
        }
        Ok(data)
    }
    pub fn write_compiled(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        let mut bytes = b"HOI4RESEARCH\0V6\0".to_vec();
        bytes.extend(miniz_oxide::deflate::compress_to_vec_zlib(&json, 6));
        atomic_bytes(path, &bytes)
    }
    /// Include unparsed text and outer constants; flatten effective layers so
    /// this private bundle can be reimported without access to the installation.
    pub fn write_review_sources(&self, root: &Path) -> Result<PathBuf, String> {
        let destination = root.join("research-review-sources");
        let staging = root.join("research-review-sources.tmp");
        if staging.exists() {
            fs::remove_dir_all(&staging).map_err(|e| e.to_string())?;
        }
        fs::create_dir(&staging).map_err(|e| e.to_string())?;
        let operation = (|| {
            let mut manifest = BTreeMap::new();
            let mut review_files = effective_files(root, &self.installation)?;
            // Private review inputs only: adding an OOB must not invalidate the
            // research cache or accidentally interpret it as a research effect.
            review_files.extend(effective_files_in(
                root,
                &self.installation,
                &[
                    ("common/units", "txt"),
                    ("common/terrain", "txt"),
                    ("common/modifiers", "txt"),
                    ("common/unit_leader", "txt"),
                    ("common/abilities", "txt"),
                    ("history/units", "txt"),
                    ("history/states", "txt"),
                    ("history/diplomacy", "txt"),
                ],
            )?);
            for (relative, path) in review_files {
                if relative.ends_with(".yml")
                    && !["english", "russian"].iter().any(|lang| {
                        relative.ends_with(&format!("_l_{lang}.yml"))
                            || relative.contains(&format!("localisation/{lang}/"))
                    })
                {
                    continue;
                }
                let target = staging.join(&relative);
                fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
                let size = fs::copy(&path, &target).map_err(|e| format!("{relative}: {e}"))?;
                manifest.insert(relative, size);
            }
            let mut install = self.installation.clone();
            install.data_layers.clear();
            atomic_json(&staging.join("research-import.json"), &install)?;
            atomic_json(
                &staging.join("source-manifest.json"),
                &serde_json::json!({"original_installation":self.installation,"files":manifest,"flattened_layers":true,"localisation_languages":["english","russian"]}),
            )?;
            for name in [
                "launcher-settings.json",
                "version.txt",
                "research-runtime-evidence.json",
                "military_profile.txt",
                "military-command-profile.json",
                "military-import-report.json",
            ] {
                if root.join(name).is_file()
                    && (name != "research-runtime-evidence.json"
                        || fs::metadata(root.join(name))
                            .map_err(|e| e.to_string())?
                            .len()
                            <= 2 * 1024 * 1024)
                {
                    fs::copy(root.join(name), staging.join(name)).map_err(|e| e.to_string())?;
                }
            }
            Ok::<_, String>(())
        })();
        if let Err(error) = operation {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
        let backup = root.join("research-review-sources.previous");
        if backup.exists() {
            fs::remove_dir_all(&backup).map_err(|e| e.to_string())?;
        }
        if destination.exists() {
            fs::rename(&destination, &backup).map_err(|e| e.to_string())?;
        }
        if let Err(error) = fs::rename(&staging, &destination) {
            if backup.exists() {
                let _ = fs::rename(&backup, &destination);
            }
            return Err(error.to_string());
        }
        if backup.exists() {
            let _ = fs::remove_dir_all(backup);
        }
        Ok(destination)
    }
    pub fn start_date(&self) -> Result<GameDate, String> {
        if let Some(year) = self
            .installation
            .scenario
            .as_ref()
            .and_then(|s| s.parse::<u32>().ok())
        {
            if let Some(date) = self.bookmarks.iter().find(|d| d.year == year) {
                return Ok(*date);
            }
            if year != 1936 {
                return Err(format!(
                    "В экспорте отсутствует сценарий {year}; дата начала не выдумана"
                ));
            }
        }
        Ok(self
            .bookmarks
            .iter()
            .find(|d| d.year == 1936)
            .copied()
            .unwrap_or(GameDate::START))
    }
    pub fn country(&self, tag: &str, date: GameDate, root: &Path) -> Option<CountryResearch> {
        self.histories.get(tag).map(|b| {
            initialize_country(
                b,
                tag,
                date,
                &self.registry,
                &self.rules,
                &profile_context(root),
            )
        })
    }
    pub fn audit(&self, root: &Path) -> serde_json::Value {
        let dates = if self.bookmarks.is_empty() {
            vec![GameDate::START]
        } else {
            self.bookmarks.clone()
        };
        let scenarios=dates.into_iter().map(|date|{
            let countries=self.histories.keys().map(|tag|{let c=self.country(tag,date,root).unwrap();let mut deferred_counts=BTreeMap::new();for e in &c.pending_effects{*deferred_counts.entry(&e.field).or_insert(0usize)+=1;}let missing:Vec<_>=c.researched.iter().filter(|id|!self.registry.technologies.contains_key(*id)).collect();
                (tag.clone(),serde_json::json!({"slots":c.slots.len(),"slots_verified":c.research_slots_known,"source_file":self.history_source_files.get(tag),"source_eof_terminated":self.history_source_files.get(tag).is_some_and(|path|self.eof_terminated_sources.contains_key(path)),"technology_levels":c.technology_levels,"known_count":c.researched.len(),"missing_definitions":missing,"deferred_effects":c.pending_effects,"deferred_effect_counts":deferred_counts,"ideas":c.context.ideas}))}).collect::<BTreeMap<_,_>>();
            (date.display(),countries)
        }).collect::<BTreeMap<_,_>>();
        let actual_ids: BTreeSet<_> = self.registry.technologies.keys().cloned().collect();
        let diagnostic_summary = serde_json::json!({
            "total":self.diagnostics.len(),"failed_source_files":self.failed_source_files.len(),"eof_terminated_sources":self.eof_terminated_sources.len(),
            "missing_localised_names":self.diagnostics.iter().filter(|s|s.contains("нет локализованного имени")).count(),
            "partially_supported_fields":self.diagnostics.iter().filter(|s|s.contains("частичная поддержка поля")).count(),
            "unresolved_lua_defines":self.diagnostics.iter().filter(|s|s.contains("не вычислено Lua-выражение")).count()
        });
        serde_json::json!({"expected_technology_ids":self.source_technology_ids,"missing_technology_ids":self.source_technology_ids.difference(&actual_ids).collect::<Vec<_>>(),"unexpected_technology_ids":actual_ids.difference(&self.source_technology_ids).collect::<Vec<_>>(),"failed_source_files":self.failed_source_files,"eof_terminated_sources":self.eof_terminated_sources,"missing_country_ids":self.source_country_ids.iter().filter(|id|!self.histories.contains_key(*id)).collect::<Vec<_>>(),"define_values":self.define_values,"schema":CACHE_VERSION,"installation":self.installation,"base_game_dlc_features":base_game_dlcs(&self.installation),"fingerprint":self.registry.fingerprint,"source_files":self.files.len(),"technologies":self.registry.technologies.len(),"ids":self.registry.technologies.keys().collect::<Vec<_>>(),"ordinary_technologies":self.registry.technologies.values().filter(|t|t.kind==ResearchKind::Standard).count(),"doctrine_technology_definitions":self.registry.technologies.values().filter(|t|t.kind==ResearchKind::Doctrine).count(),"special_project_technology_definitions":self.registry.technologies.values().filter(|t|t.kind==ResearchKind::SpecialProject).count(),"separate_definition_files":self.registry.scripts.sources.values().filter(|s|s.kind=="separate_research_definition").map(|s|&s.source).collect::<BTreeSet<_>>(),"countries":self.histories.len(),"scenarios":scenarios,"diagnostics":self.diagnostics,"diagnostic_summary":diagnostic_summary,"research_rules":self.rules,"rule_sources":self.rule_sources,"technology_definitions":self.registry.technologies,"original_game_timing_comparisons":0,"parity_verified":false})
    }
}
fn apply_localisations(
    files: &BTreeMap<String, PathBuf>,
    cat: &mut ResearchCatalogue,
    diagnostics: &mut Vec<String>,
) {
    let mut translations = BTreeMap::<String, String>::new();
    for lang in ["english", "russian"] {
        for (rel, p) in files.iter().filter(|(r, _)| {
            r.ends_with(&format!("_l_{lang}.yml")) || r.contains(&format!("localisation/{lang}/"))
        }) {
            let Ok(source) = fs::read_to_string(p) else {
                diagnostics.push(format!("{rel}: localisation unreadable"));
                continue;
            };
            for line in source.lines() {
                let line = line.trim_start_matches('\u{feff}').trim();
                if line.starts_with('#') {
                    continue;
                }
                let Some((key, value)) = line.split_once(':') else {
                    continue;
                };
                let Some(start) = value.find('"') else {
                    continue;
                };
                let mut s = String::new();
                let mut escaped = false;
                let mut closed = false;
                for c in value[start + 1..].chars() {
                    if escaped {
                        match c {
                            'n' => s.push('\n'),
                            _ => s.push(c),
                        }
                        escaped = false;
                    } else if c == '\\' {
                        escaped = true;
                    } else if c == '"' {
                        closed = true;
                        break;
                    } else {
                        s.push(c);
                    }
                }
                if closed {
                    translations.insert(key.trim().into(), s);
                }
            }
        }
    }
    fn resolve(s: &str, dict: &BTreeMap<String, String>, depth: u8) -> String {
        if depth > 16 {
            return s.into();
        }
        let mut out = String::new();
        let mut rest = s;
        while let Some(i) = rest.find('$') {
            out.push_str(&rest[..i]);
            rest = &rest[i + 1..];
            if let Some(end) = rest.find('$') {
                let token = &rest[..end];
                let key = token.split('|').next().unwrap_or(token);
                if let Some(value) = dict.get(key) {
                    out.push_str(&resolve(value, dict, depth + 1));
                } else {
                    out.push('$');
                    out.push_str(token);
                    out.push('$');
                }
                rest = &rest[end + 1..];
            } else {
                out.push('$');
                break;
            }
        }
        out.push_str(rest);
        out
    }
    for f in cat.folders.iter().chain(cat.folder_definitions.keys()) {
        if let Some(s) = translations.get(f) {
            cat.folder_names
                .insert(f.clone(), resolve(s, &translations, 0));
        }
    }
    for t in cat.technologies.values_mut() {
        // Keep declared equipment order, not alphabetical set order.
        let equipment = t
            .raw
            .iter()
            .filter(|e| e.key == "enable_equipments")
            .flat_map(|e| {
                e.atom().into_iter().chain(
                    e.block()
                        .unwrap_or(&[])
                        .iter()
                        .filter(|e| e.value.is_none() || matches!(e.atom(), Some("yes" | "1")))
                        .map(|e| e.key.as_str()),
                )
            })
            .find(|id| translations.contains_key(*id));
        if let Some(s) = translations
            .get(&t.id)
            .or_else(|| equipment.and_then(|id| translations.get(id)))
        {
            t.name = resolve(s, &translations, 0);
        } else {
            t.name = t.id.clone();
            diagnostics.push(format!("{}: нет локализованного имени", t.id));
        }
        let desc = atom(&t.raw, "desc")
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{}_desc", t.id));
        let equipment_desc = equipment.map(|id| format!("{id}_desc"));
        if let Some(s) = translations
            .get(&desc)
            .or_else(|| equipment_desc.as_ref().and_then(|id| translations.get(id)))
        {
            t.description = resolve(s, &translations, 0);
        }
    }
}
pub fn initialize_country(
    b: &[Entry],
    tag: &str,
    date: GameDate,
    cat: &ResearchCatalogue,
    rules: &ResearchRules,
    context: &ResearchContext,
) -> CountryResearch {
    let mut c = CountryResearch::new(rules.base_research_slots.unwrap_or(0), rules);
    c.research_slots_known = rules.base_research_slots.is_some();
    c.context = context.clone();
    c.context.tag = tag.into();
    c.context.original_tag = tag.into();
    c.context.date = Some(date);
    fn run(
        c: &mut CountryResearch,
        b: &[Entry],
        cat: &ResearchCatalogue,
        rules: &ResearchRules,
        date: GameDate,
    ) {
        let mut dated = Vec::new();
        let mut effects = Vec::new();
        for e in b {
            if let Some(at) = research_script::parse_date(&e.key) {
                if at <= date {
                    dated.push((at, e));
                }
            } else {
                effects.push(e.clone());
            }
        }
        research_script::apply_effects(c, &effects, cat, rules, date, "country_history");
        dated.sort_by_key(|(at, _)| *at);
        for (_, e) in dated {
            run(c, e.block().unwrap_or(&[]), cat, rules, date);
        }
    }
    run(&mut c, b, cat, rules, date);
    c.completed.clear();
    c
}
pub fn atomic_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    atomic_bytes(path, &bytes)
}
fn atomic_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension("tmp");
    fs::write(&temp, bytes).map_err(|e| e.to_string())?;
    let backup = path.with_extension("bak");
    if path.exists() {
        if backup.exists() {
            fs::remove_file(&backup).map_err(|e| e.to_string())?;
        }
        fs::rename(path, &backup).map_err(|e| e.to_string())?;
    }
    if let Err(e) = fs::rename(&temp, path) {
        if backup.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Err(e.to_string());
    }
    if backup.exists() {
        let _ = fs::remove_file(backup);
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct ResearchSnapshot {
    schema: u32,
    fingerprint: String,
    date: GameDate,
    hour: u8,
    world: ResearchWorld,
}
pub fn save_snapshot(
    path: &Path,
    world: &ResearchWorld,
    cat: &ResearchCatalogue,
    date: GameDate,
    hour: u8,
) -> Result<(), String> {
    atomic_json(
        path,
        &ResearchSnapshot {
            schema: CACHE_VERSION,
            fingerprint: cat.fingerprint.clone(),
            date,
            hour,
            world: world.clone(),
        },
    )
}
pub fn load_snapshot(
    path: &Path,
    cat: &ResearchCatalogue,
) -> Result<(ResearchWorld, GameDate, u8), String> {
    let s: ResearchSnapshot = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if s.schema != CACHE_VERSION || s.fingerprint != cat.fingerprint {
        return Err("Снимок исследований относится к другому набору игровых данных".into());
    }
    if s.hour >= 24 || research_script::parse_date(&s.date.display()).is_none() {
        return Err("Некорректное время в снимке".into());
    }
    for c in s.world.countries.values() {
        let mut active = BTreeSet::new();
        for slot in &c.slots {
            if !slot.saved_days.is_finite() || slot.saved_days < 0.0 {
                return Err("Некорректный запас дней".into());
            }
            if let Some(a) = &slot.active {
                if !cat.technologies.contains_key(&a.tech_id)
                    || !active.insert(&a.tech_id)
                    || !a.progress.is_finite()
                    || a.progress < 0.0
                    || !a.cost_coefficient.is_finite()
                    || a.cost_coefficient < 0.0
                {
                    return Err("Некорректное активное исследование".into());
                }
            }
        }
    }
    Ok((s.world, s.date, s.hour))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "hoi4-research-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn put(&self, path: &str, s: &str) {
            let p = self.0.join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, s).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn fixture() -> Fixture {
        let f = Fixture::new();
        f.put("common/technologies/a.txt",r#"@cost=1.5 technologies={
            first={research_cost=@cost start_year=1936 categories={industry} path={leads_to_tech=third research_cost_coeff=0.75} folder={name=industry position={x=-1.5 y=0.25}} folder={name=engineering position={x=2 y=3}} enable_building={building=nuclear_reactor level=2} modifier={research_speed_factor=0.1 industrial_capacity_factory=0.1} on_research_complete={set_country_flag=finished} }
            alternative={research_cost=1 start_year=1936 path={leads_to_tech=third research_cost_coeff=1}}
            third={research_cost=1 start_year=1938 xor={exclusive} allow={OR={tag=AAA has_country_flag=can_research}} enable_equipment_modules={module_a}}
            exclusive={research_cost=1 start_year=1936}
            doctrine={research_cost=1 doctrine=yes xp_unlock_cost=100}
            special={research_cost=1 is_special_project_tech=yes}
        }"#);
        f.put(
            "common/bookmarks/start.txt",
            "bookmarks={bookmark={date=1936.1.1.12} bookmark={date=1939.8.14.12}}",
        );
        f.put("common/technology_tags/tags.txt","technology_categories={industry electronics} technology_folders={engineering industry}");
        f.put("history/countries/AAA - A.txt","set_research_slots=2 set_technology={first=1} add_ideas=scientists 1939.1.1={add_research_slot=1 set_technology={alternative=1}}");
        f.put("history/countries/BBB - B.txt","set_research_slots=1 set_technology={alternative=1} 1939.1.1={set_technology={alternative=0 exclusive=1}}");
        f.put("common/ideas/test.txt","ideas={country={scientists={modifier={research_speed_factor=0.2} research_bonus={industry=0.1}}}}");
        f.put("common/national_focus/test.txt","focus_tree={focus={id=award available={tag=AAA} completion_reward={add_tech_bonus={name=one_use bonus=0.5 ahead_reduction=1 uses=1 category=industry category=electronics technology=third} add_research_slot=1}}}");
        f.put(
            "common/technology_sharing/groups.txt",
            "technology_sharing={id=group research_sharing_per_country_bonus=0.1}",
        );
        f.put("common/defines/rules.lua","NDefines.NTechnology.BASE_TECH_COST=100\nNDefines.NTechnology.BASE_YEAR_AHEAD_PENALTY_FACTOR=2\nNDefines.NTechnology.BASE_RESEARCH_POINTS_SAVED=30");
        f.put("localisation/english/tech_l_english.yml","l_english:\n first:0 \"English\"\n first_desc:0 \"Line 1\\nLine 2\"\n industry:0 \"Industry\"");
        f.put("localisation/russian/tech_l_russian.yml","\u{feff}l_russian:\n FIRST_NAME:0 \"Оригинальное имя\"\n first:0 \"$FIRST_NAME$\"\n industry:0 \"Промышленность\"");
        f
    }
    #[test]
    fn registry_preserves_all_positions_fields_and_country_scenarios() {
        let f = fixture();
        let d = ResearchDataset::load(&f.0, false).unwrap();
        assert_eq!(d.registry.technologies.len(), 6);
        assert_eq!(d.histories.len(), 2);
        assert_eq!(d.bookmarks.len(), 2);
        let t = &d.registry.technologies["first"];
        assert_eq!(t.research_cost, 1.5);
        assert_eq!(t.positions.len(), 2);
        assert_eq!(t.positions[0].x, -1.5);
        assert_eq!(t.positions[0].y, 0.25);
        assert_eq!(t.paths[0].research_cost_coeff, 0.75);
        assert_eq!(t.name, "Оригинальное имя");
        assert_eq!(t.description, "Line 1\nLine 2");
        assert!(!t.source.is_empty());
        let a = d.country("AAA", GameDate::START, &f.0).unwrap();
        let b = d.country("BBB", GameDate::START, &f.0).unwrap();
        assert_eq!(a.slots.len(), 2);
        assert_eq!(b.slots.len(), 1);
        assert!(a.researched.contains("first"));
        assert!(!b.researched.contains("first"));
        assert_eq!(a.building_levels["nuclear_reactor"], 2);
        let a39 = d
            .country(
                "AAA",
                GameDate {
                    year: 1939,
                    month: 8,
                    day: 14,
                },
                &f.0,
            )
            .unwrap();
        assert_eq!(a39.slots.len(), 3);
        assert!(a39.researched.contains("alternative"));
        let b39 = d
            .country(
                "BBB",
                GameDate {
                    year: 1939,
                    month: 8,
                    day: 14,
                },
                &f.0,
            )
            .unwrap();
        assert!(!b39.researched.contains("alternative"));
        assert!(b39.researched.contains("exclusive"));
    }
    #[test]
    fn block_constants_drive_layout_without_becoming_technology_ids() {
        let f = fixture();
        f.put("common/technologies/constants.txt","@cost=1.5 technologies={@1938=4 @x=1 variable_tech={@x=2 research_cost=@cost folder={name=industry position={x=@[ @x + 0.5 ] y=@1938}}} unresolved_layout={folder={name=industry position={x=@missing y=0}}}} technologies={@1938=9 second_scope={folder={name=industry position={x=0 y=@1938}}}}");
        let d = ResearchDataset::load(&f.0, false).unwrap();
        let t = &d.registry.technologies["variable_tech"];
        assert_eq!(t.research_cost, 1.5);
        assert_eq!(t.positions[0].x, 2.5);
        assert_eq!(t.positions[0].y, 4.0);
        assert_eq!(t.constants["@1938"], "4");
        assert_eq!(d.registry.technologies["second_scope"].positions[0].y, 9.0);
        assert!(
            d.registry.technologies["unresolved_layout"]
                .positions
                .is_empty()
        );
        assert!(
            d.registry.technologies["unresolved_layout"]
                .unsupported
                .contains("folder.position")
        );
        assert!(
            d.source_technology_ids
                .iter()
                .all(|id| !id.starts_with('@'))
        );
        assert!(
            d.audit(&f.0)["missing_technology_ids"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn base_slots_are_inherited_and_unknown_slots_are_not_invented() {
        let f = fixture();
        f.put(
            "common/defines/zcountry.lua",
            "NDefines.NCountry.BASE_RESEARCH_SLOTS=2\nNDefines.NMilitary.BASE_TECH_COST=999\nNDefines.NAI.RESEARCH_BASE_DAYS=60",
        );
        f.put("history/countries/CCC - C.txt", "add_research_slot=1");
        let d = ResearchDataset::load(&f.0, false).unwrap();
        let c = d.country("CCC", GameDate::START, &f.0).unwrap();
        assert_eq!(c.slots.len(), 3);
        assert!(c.research_slots_known);
        assert_eq!(d.rules.base_tech_cost, 100.0);
        fs::remove_file(f.0.join("common/defines/zcountry.lua")).unwrap();
        let d = ResearchDataset::load(&f.0, false).unwrap();
        assert!(
            !d.country("CCC", GameDate::START, &f.0)
                .unwrap()
                .research_slots_known
        );
        assert!(
            d.country("AAA", GameDate::START, &f.0)
                .unwrap()
                .research_slots_known
        );
    }
    #[test]
    fn unresolved_override_removes_verified_define_provenance() {
        let f = fixture();
        f.put("common/defines/zzoverride.lua","NDefines.NTechnology.BASE_TECH_COST=100*2\nNDefines.NCountry.BASE_RESEARCH_SLOTS=some_function()");
        let d = ResearchDataset::load(&f.0, false).unwrap();
        assert!(!d.rule_sources.contains_key("BASE_TECH_COST"));
        assert!(
            !d.define_values
                .contains_key("NDefines.NTechnology.BASE_TECH_COST")
        );
        assert_eq!(
            d.rules.base_tech_cost,
            ResearchRules::default().base_tech_cost
        );
        assert!(d.rules.base_research_slots.is_none());
    }
    #[test]
    fn equipment_labels_preserve_order_and_explicit_tech_labels_win() {
        let f = fixture();
        f.put("common/technologies/equipment.txt","technologies={equipped={enable_equipments={z_equipment a_equipment}} explicit={enable_equipments={z_equipment}}}");
        f.put("localisation/russian/equipment_l_russian.yml","l_russian:\n z_equipment:0 \"Первая модель\"\n a_equipment:0 \"Вторая модель\"\n z_equipment_desc:0 \"Описание модели\"\n explicit:0 \"Своё название\"\n explicit_desc:0 \"Своё описание\"");
        let d = ResearchDataset::load(&f.0, false).unwrap();
        assert_eq!(d.registry.technologies["equipped"].name, "Первая модель");
        assert_eq!(
            d.registry.technologies["equipped"].description,
            "Описание модели"
        );
        assert_eq!(d.registry.technologies["explicit"].name, "Своё название");
        assert_eq!(
            d.registry.technologies["explicit"].description,
            "Своё описание"
        );
    }
    #[test]
    fn review_keeps_failed_sources_and_flattens_effective_layers() {
        let f = fixture();
        f.put("history/countries/NOR - N.txt", "unclosed={");
        f.put(
            "layers/one/common/technologies/a.txt",
            "technologies={@layout=7 imported={folder={name=industry position={x=0 y=@layout}}}}",
        );
        atomic_json(
            &f.0.join("research-import.json"),
            &Installation {
                data_layers: vec!["layers/one".into()],
                load_order_verified: true,
                ..Default::default()
            },
        )
        .unwrap();
        let d = ResearchDataset::load(&f.0, false).unwrap();
        assert_eq!(
            d.audit(&f.0)["missing_country_ids"],
            serde_json::json!(["NOR"])
        );
        let sources = d.write_review_sources(&f.0).unwrap();
        assert_eq!(
            fs::read_to_string(sources.join("history/countries/NOR - N.txt")).unwrap(),
            "unclosed={"
        );
        assert!(installation(&sources).data_layers.is_empty());
        let replay = ResearchDataset::load(&sources, false).unwrap();
        assert_eq!(replay.registry.technologies["imported"].positions[0].y, 7.0);
        assert_eq!(replay.source_technology_ids, d.source_technology_ids);
        assert_eq!(replay.failed_source_files, d.failed_source_files);
        assert_eq!(d.write_review_sources(&f.0).unwrap(), sources);
    }
    #[test]
    fn army_review_contains_effective_oob_without_polluting_research_cache() {
        let f = fixture();
        f.put(
            "history/units/AAA_1936.txt",
            "units={division={name=Source division_template=Infantry location=1}}",
        );
        f.put(
            "common/units/a.txt",
            "sub_units={infantry={combat_width=2}}",
        );
        let d = ResearchDataset::load(&f.0, false).unwrap();
        let fingerprint = d.registry.fingerprint.clone();
        let out = d.write_review_sources(&f.0).unwrap();
        assert!(out.join("history/units/AAA_1936.txt").is_file());
        assert!(out.join("common/units/a.txt").is_file());
        assert!(!d.files.iter().any(|f| f.path.starts_with("history/units/")));
        assert_eq!(
            ResearchDataset::load(&f.0, true)
                .unwrap()
                .registry
                .fingerprint,
            fingerprint
        );
    }
    #[test]
    fn game_version_has_a_recorded_source() {
        let f = Fixture::new();
        f.put(
            "launcher-settings.json",
            "{\"rawVersion\":\"1.test\",\"version\":\"placeholder\"}",
        );
        assert_eq!(
            game_version(&f.0),
            Some(("1.test".into(), "launcher-settings.json:rawVersion".into()))
        );
        f.put("launcher-settings.json", "{\"gameVersion\":\"2.test\"}");
        assert_eq!(game_version(&f.0).unwrap().0, "2.test");
    }
    #[test]
    fn bundled_115_features_are_known_without_assuming_paid_dlc_ownership() {
        let f = fixture();
        let mut install = Installation {
            version: Some("1.15.4".into()),
            version_source: Some("launcher-settings.json:rawVersion".into()),
            ..Default::default()
        };
        atomic_json(&f.0.join("research-import.json"), &install).unwrap();
        let context = profile_context(&f.0);
        assert_eq!(context.base_game_dlcs.len(), 3);
        assert!(context.dlc_selection_unverified);
        let c = initialize_country(
            &[],
            "AAA",
            GameDate::START,
            &ResearchCatalogue::default(),
            &ResearchRules::default(),
            &context,
        );
        assert_eq!(
            research_script::evaluate(&parse_pdx("has_dlc=\"Waking the Tiger\"").unwrap(), &c),
            Some(true)
        );
        assert_eq!(
            research_script::evaluate(&parse_pdx("has_dlc=\"No Step Back\"").unwrap(), &c),
            None
        );
        install.dlc_selection_verified = true;
        atomic_json(&f.0.join("research-import.json"), &install).unwrap();
        let context = profile_context(&f.0);
        assert!(context.dlcs.contains("Together for Victory"));
        assert!(!context.dlcs.contains("No Step Back"));
        let c = initialize_country(
            &[],
            "AAA",
            GameDate::START,
            &ResearchCatalogue::default(),
            &ResearchRules::default(),
            &context,
        );
        assert_eq!(
            research_script::evaluate(&parse_pdx("has_dlc=\"No Step Back\"").unwrap(), &c),
            Some(false)
        );
        for version in ["1.0", "1.14.2", "1.15.invalid", "1.17.0"] {
            install.version = Some(version.into());
            assert!(base_game_dlcs(&install).is_empty());
        }
        install.version = Some("1.15.4".into());
        install.version_source = None;
        assert!(base_game_dlcs(&install).is_empty());
    }
    #[test]
    fn eof_wrappers_preserve_country_dates_ideas_and_focus_rewards_with_provenance() {
        let f = fixture();
        f.put(
            "history/countries/CCC - C.txt",
            "set_research_slots=2 set_technology={first=1} 1939.1.1={add_research_slot=1",
        );
        f.put(
            "common/ideas/eof.txt",
            "ideas={country={eof_scientists={modifier={research_speed_factor=0.25}}}",
        );
        f.put(
            "common/national_focus/eof.txt",
            "focus_tree={focus={id=eof_award completion_reward={add_research_slot=1}}",
        );
        let country_source = fs::read(f.0.join("history/countries/CCC - C.txt")).unwrap();
        let d = ResearchDataset::load(&f.0, false).unwrap();
        assert_eq!(d.eof_terminated_sources.len(), 3);
        assert!(d.failed_source_files.is_empty());
        let c = d.country("CCC", GameDate::START, &f.0).unwrap();
        assert_eq!(c.slots.len(), 2);
        assert!(c.researched.contains("first"));
        let c = d
            .country(
                "CCC",
                GameDate {
                    year: 1939,
                    month: 8,
                    day: 14,
                },
                &f.0,
            )
            .unwrap();
        assert_eq!(c.slots.len(), 3);
        assert!(d.registry.scripts.ideas.contains_key("eof_scientists"));
        assert!(d.registry.scripts.sources.contains_key("focus:eof_award"));
        assert!(
            d.audit(&f.0)["scenarios"][GameDate::START.display()]["CCC"]["source_eof_terminated"]
                .as_bool()
                .unwrap()
        );
        assert_eq!(
            fs::read(f.0.join("history/countries/CCC - C.txt")).unwrap(),
            country_source
        );
        let cached = ResearchDataset::load(&f.0, true).unwrap();
        assert!(cached.from_cache);
        assert_eq!(cached.eof_terminated_sources, d.eof_terminated_sources);
    }
    #[test]
    fn uppercase_xor_makes_industry_choices_mutually_exclusive() {
        let f = fixture();
        f.put(
            "common/technologies/industry_choices.txt",
            "technologies={compact={XOR={spread}} spread={xor={compact}}}",
        );
        let d = ResearchDataset::load(&f.0, false).unwrap();
        let mut c = CountryResearch::new(1, &d.rules);
        assert!(d.registry.can_research("compact", &c).is_ok());
        assert!(d.registry.can_research("spread", &c).is_ok());
        assert!(c.grant("compact", &d.registry));
        assert!(d.registry.can_research("spread", &c).is_err());
        assert!(
            !d.registry.technologies["compact"]
                .unsupported
                .contains("scope:XOR")
        );
        c.revoke("compact", &d.registry);
        assert!(c.grant("spread", &d.registry));
        assert!(d.registry.can_research("compact", &c).is_err());
    }
    #[test]
    fn alternatives_xor_separate_subsystems_and_cost_coefficients_are_enforced() {
        let f = fixture();
        let d = ResearchDataset::load(&f.0, false).unwrap();
        let mut c = d.country("AAA", GameDate::START, &f.0).unwrap();
        assert!(d.registry.can_research("third", &c).is_ok());
        c.start(0, "third", &d.registry, &d.rules, GameDate::START)
            .unwrap();
        assert_eq!(c.slots[0].active.as_ref().unwrap().cost_coefficient, 0.75);
        assert!(d.registry.can_research("exclusive", &c).is_err());
        assert!(
            c.start(1, "doctrine", &d.registry, &d.rules, GameDate::START)
                .is_err()
        );
        assert!(
            c.start(1, "special", &d.registry, &d.rules, GameDate::START)
                .is_err()
        );
    }
    #[test]
    fn rewards_do_not_apply_until_completion_and_one_use_is_not_reused() {
        let f = fixture();
        let d = ResearchDataset::load(&f.0, false).unwrap();
        let c = d.country("AAA", GameDate::START, &f.0).unwrap();
        assert!(c.available_bonuses.is_empty());
        let mut w = ResearchWorld::default();
        w.countries.insert("AAA".into(), c);
        w.execute_source(
            "AAA",
            "focus",
            "award",
            &d.registry,
            &d.rules,
            GameDate::START,
        )
        .unwrap();
        assert_eq!(w.countries["AAA"].slots.len(), 3);
        assert!(
            w.execute_source(
                "AAA",
                "focus",
                "award",
                &d.registry,
                &d.rules,
                GameDate::START
            )
            .is_err()
        );
        let c = w.countries.get_mut("AAA").unwrap();
        c.start(0, "third", &d.registry, &d.rules, GameDate::START)
            .unwrap();
        let bonus = c.slots[0].active.as_ref().unwrap().locked_bonus;
        assert_eq!(bonus, 0.5);
        assert_eq!(c.available_bonuses[0].uses, 0);
        c.tick(&d.registry, &d.rules, GameDate::START);
        let progress = c.slots[0].active.as_ref().unwrap().progress;
        c.cancel(0);
        c.start(0, "third", &d.registry, &d.rules, GameDate::START)
            .unwrap();
        assert_eq!(c.slots[0].active.as_ref().unwrap().progress, progress);
        assert_eq!(c.slots[0].active.as_ref().unwrap().locked_bonus, bonus);
    }
    #[test]
    fn unknown_conditions_dont_choose_else_and_unknown_effects_are_reported() {
        let cat = ResearchCatalogue::default();
        let rules = ResearchRules::default();
        let raw=parse_pdx("set_research_slots=2 if={limit={unknown_trigger=yes} add_research_slot=3 else={add_research_slot=4}} unimplemented_effect={a=b}").unwrap();
        let c = initialize_country(
            &raw,
            "AAA",
            GameDate::START,
            &cat,
            &rules,
            &Default::default(),
        );
        assert_eq!(c.slots.len(), 2);
        assert_eq!(c.pending_effects.len(), 2);
        assert_eq!(
            research_script::evaluate(&parse_pdx("NOT={unknown_trigger=yes}").unwrap(), &c),
            None
        );
    }
    #[test]
    fn cache_invalidation_and_layer_replace_path_do_not_leave_stale_definitions() {
        let f = fixture();
        let first = ResearchDataset::load(&f.0, true).unwrap();
        assert!(!first.from_cache);
        let cached = ResearchDataset::load(&f.0, true).unwrap();
        assert!(cached.from_cache);
        assert_eq!(first.registry.fingerprint, cached.registry.fingerprint);
        f.put(
            "layer/descriptor.mod",
            "replace_path=\"common/technologies\"",
        );
        f.put(
            "layer/common/technologies/replacement.txt",
            "technologies={replacement={research_cost=2 start_year=1940}}",
        );
        atomic_json(
            &f.0.join("research-import.json"),
            &Installation {
                data_layers: vec!["layer".into()],
                ..Default::default()
            },
        )
        .unwrap();
        let changed = ResearchDataset::load(&f.0, true).unwrap();
        assert!(!changed.from_cache);
        assert_eq!(
            changed
                .registry
                .technologies
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["replacement"]
        );
    }
    #[test]
    fn sharing_requires_membership_and_known_technology() {
        let f = fixture();
        let d = ResearchDataset::load(&f.0, false).unwrap();
        let mut w = ResearchWorld::default();
        for tag in ["AAA", "BBB"] {
            let mut c = d.country(tag, GameDate::START, &f.0).unwrap();
            c.sharing_groups.insert("group".into());
            w.countries.insert(tag.into(), c);
        }
        w.refresh_sharing(&d.registry);
        assert_eq!(w.countries["BBB"].sharing_bonuses["first"], 0.1);
        assert_eq!(w.countries["AAA"].sharing_bonuses["first"], 0.0);
        w.countries.get_mut("BBB").unwrap().sharing_groups.clear();
        w.refresh_sharing(&d.registry);
        assert!(w.countries["BBB"].sharing_bonuses.is_empty());
    }
    #[test]
    fn snapshot_round_trip_restores_progress_bonus_and_rejects_other_data() {
        let f = fixture();
        let d = ResearchDataset::load(&f.0, false).unwrap();
        let mut w = ResearchWorld::default();
        let mut c = d.country("AAA", GameDate::START, &f.0).unwrap();
        c.start(0, "third", &d.registry, &d.rules, GameDate::START)
            .unwrap();
        c.tick(&d.registry, &d.rules, GameDate::START);
        w.countries.insert("AAA".into(), c);
        let path = f.0.join("snapshot.json");
        save_snapshot(&path, &w, &d.registry, GameDate::START, 12).unwrap();
        let (restored, date, hour) = load_snapshot(&path, &d.registry).unwrap();
        assert_eq!(date, GameDate::START);
        assert_eq!(hour, 12);
        assert_eq!(
            serde_json::to_value(&w).unwrap(),
            serde_json::to_value(&restored).unwrap()
        );
        let mut other = d.registry.clone();
        other.fingerprint = "other".into();
        assert!(load_snapshot(&path, &other).is_err());
    }
    #[test]
    fn completion_and_revocation_do_not_duplicate_modifiers_or_repeat_scripts() {
        let f = fixture();
        let d = ResearchDataset::load(&f.0, false).unwrap();
        let mut c = CountryResearch::new(1, &d.rules);
        c.grant("first", &d.registry);
        assert!(c.flags.contains("finished"));
        assert!(!c.grant("first", &d.registry));
        assert_eq!(c.speed_bonus, 0.1);
        c.revoke("first", &d.registry);
        assert_eq!(c.speed_bonus, 0.0);
        assert!(c.building_levels.is_empty());
        assert!(c.flags.contains("finished"));
    }
    #[test]
    fn cancellation_and_invalid_switch_are_transactional() {
        let f = fixture();
        let d = ResearchDataset::load(&f.0, false).unwrap();
        let mut c = d.country("AAA", GameDate::START, &f.0).unwrap();
        c.start(0, "third", &d.registry, &d.rules, GameDate::START)
            .unwrap();
        let before = serde_json::to_value(&c).unwrap();
        assert!(
            c.switch(0, "doctrine", &d.registry, &d.rules, GameDate::START)
                .is_err()
        );
        assert_eq!(before, serde_json::to_value(&c).unwrap());
        c.switch(0, "alternative", &d.registry, &d.rules, GameDate::START)
            .unwrap();
        assert!(c.parked.contains_key("third"));
    }
    #[test]
    fn no_demo_tree_or_invented_bookmark_on_missing_input() {
        let f = Fixture::new();
        assert!(ResearchDataset::load(&f.0, false).is_err());
        let f2 = fixture();
        f2.put("research-import.json",r#"{"scenario":"1941","enabled_dlcs":[],"installed_dlcs":[],"dlc_selection_verified":true,"data_layers":[],"load_order_verified":true}"#);
        let d = ResearchDataset::load(&f2.0, false).unwrap();
        assert!(d.start_date().is_err());
    }
}
