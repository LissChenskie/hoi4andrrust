//! Renderer-independent commander IR. Source text and ordered AST are retained;
//! this is an import-time projection, not a second campaign state.
use crate::pdx::{self, Entry};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA_VERSION: &str = "0.1.0";
pub const CONVERTER_VERSION: &str = "0.1.0";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMetadata {
    pub file: String,
    pub object_id: String,
    pub line: usize,
    pub source_version: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceDocument {
    /// Keeps comments and unknown syntax that the semantic projection ignores.
    pub text: String,
    pub entries: Vec<Entry>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Warning,
    Error,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub source: SourceMetadata,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommanderRole {
    General,
    FieldMarshal,
}
impl CommanderRole {
    pub fn source_key(self) -> &'static str {
        match self {
            Self::General => "corps_commander",
            Self::FieldMarshal => "field_marshal",
        }
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommanderStats {
    pub skill: Option<u32>,
    pub attack: Option<u32>,
    pub defense: Option<u32>,
    pub planning: Option<u32>,
    pub logistics: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Character {
    pub id: String,
    pub name_key: String,
    pub profile_ids: Vec<String>,
    /// GFX references only; no extracted images are copied.
    pub portraits: Vec<Entry>,
    pub source: SourceMetadata,
    pub unknown_fields: Vec<Entry>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommanderProfile {
    pub id: String,
    pub character_id: String,
    pub role: CommanderRole,
    pub stats: CommanderStats,
    pub trait_ids: BTreeSet<String>,
    pub initial_experience: Option<f64>,
    pub source: SourceMetadata,
    pub unknown_fields: Vec<Entry>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraitDefinition {
    pub id: String,
    pub name_key: String,
    pub description_key: String,
    /// Includes conditional modifiers, prerequisites and XP rules verbatim.
    /// Preservation does not imply that the runtime implements each rule.
    pub fields: Vec<Entry>,
    pub source: SourceMetadata,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalizationDb {
    pub languages: BTreeMap<String, BTreeMap<String, String>>,
}
impl LocalizationDb {
    pub fn get<'a>(&'a self, language: &str, key: &'a str) -> &'a str {
        self.languages
            .get(language)
            .and_then(|d| d.get(key))
            .or_else(|| self.languages.get("english").and_then(|d| d.get(key)))
            .map_or(key, String::as_str)
    }
    pub fn contains(&self, key: &str) -> bool {
        self.languages.values().any(|d| d.contains_key(key))
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportSummary {
    pub source_documents_retained: usize,
    pub characters: usize,
    pub profiles: usize,
    pub traits: usize,
    pub localization_values: usize,
    pub warnings: usize,
    pub errors: usize,
    pub preserved_unknown_fields: usize,
    pub broken_references: usize,
    pub missing_localization: usize,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandData {
    pub schema_version: String,
    pub converter_version: String,
    pub source_version: Option<String>,
    pub characters: BTreeMap<String, Character>,
    pub profiles: BTreeMap<String, CommanderProfile>,
    pub traits: BTreeMap<String, TraitDefinition>,
    pub localization: LocalizationDb,
    pub sources: BTreeMap<String, SourceDocument>,
    pub diagnostics: Vec<Diagnostic>,
}
impl Default for CommandData {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION.into(),
            converter_version: CONVERTER_VERSION.into(),
            source_version: None,
            characters: BTreeMap::new(),
            profiles: BTreeMap::new(),
            traits: BTreeMap::new(),
            localization: LocalizationDb::default(),
            sources: BTreeMap::new(),
            diagnostics: Vec::new(),
        }
    }
}
impl CommandData {
    pub fn summary(&self) -> ImportSummary {
        ImportSummary {
            source_documents_retained: self.sources.len(),
            characters: self.characters.len(),
            profiles: self.profiles.len(),
            traits: self.traits.len(),
            localization_values: self.localization.languages.values().map(|d| d.len()).sum(),
            warnings: self
                .diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Warning)
                .count(),
            errors: self
                .diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .count(),
            preserved_unknown_fields: self
                .characters
                .values()
                .map(|c| c.unknown_fields.len())
                .sum::<usize>()
                + self
                    .profiles
                    .values()
                    .map(|p| p.unknown_fields.len())
                    .sum::<usize>(),
            broken_references: self
                .diagnostics
                .iter()
                .filter(|d| {
                    matches!(
                        d.code.as_str(),
                        "unknown_trait" | "broken_character_ref" | "broken_profile_ref"
                    )
                })
                .count(),
            missing_localization: self
                .diagnostics
                .iter()
                .filter(|d| d.code == "missing_localization")
                .count(),
        }
    }
    pub fn source(&self, file: &str, object: &str, line: usize) -> SourceMetadata {
        SourceMetadata {
            file: file.into(),
            object_id: object.into(),
            line,
            source_version: self.source_version.clone(),
        }
    }
    pub fn diagnostic(
        &mut self,
        severity: Severity,
        code: &str,
        source: SourceMetadata,
        message: impl Into<String>,
    ) {
        self.diagnostics.push(Diagnostic {
            severity,
            code: code.into(),
            message: message.into(),
            source,
        });
    }
    fn parse(&mut self, file: &str, text: &str) -> Option<Vec<Entry>> {
        match pdx::parse_pdx(text) {
            Ok(entries) => {
                self.sources.insert(
                    file.into(),
                    SourceDocument {
                        text: text.into(),
                        entries: entries.clone(),
                    },
                );
                Some(entries)
            }
            Err(message) => {
                // Malformed files still survive in the report as raw text.
                self.sources.insert(
                    file.into(),
                    SourceDocument {
                        text: text.into(),
                        entries: Vec::new(),
                    },
                );
                self.diagnostic(Severity::Error, "parse", self.source(file, "", 0), message);
                None
            }
        }
    }
    fn duplicate_fields(&mut self, file: &str, id: &str, fields: &[Entry]) {
        let mut seen = BTreeSet::new();
        for e in fields {
            if !seen.insert(&e.key) {
                self.diagnostic(
                    Severity::Error,
                    "duplicate_property",
                    self.source(file, id, e.span.line),
                    format!("Repeated semantic property {}", e.key),
                );
            }
        }
    }
    fn uint(&mut self, fields: &[Entry], key: &str, source: &SourceMetadata) -> Option<u32> {
        let value = pdx::atom(fields, key).and_then(|v| v.parse::<u32>().ok());
        if value.is_none() {
            self.diagnostic(
                Severity::Error,
                "invalid_stat",
                source.clone(),
                format!("{key}: missing or non-literal unsigned integer"),
            );
        }
        value
    }
    pub fn import_characters(&mut self, file: &str, text: &str) {
        let Some(entries) = self.parse(file, text) else {
            return;
        };
        let Some(characters) = pdx::block(&entries, "characters") else {
            self.diagnostic(
                Severity::Error,
                "characters_container",
                self.source(file, "", 1),
                "Expected characters block",
            );
            return;
        };
        for entry in characters {
            let source = self.source(file, &entry.key, entry.span.line);
            let Some(fields) = entry.block() else {
                self.diagnostic(
                    Severity::Error,
                    "invalid_character",
                    source,
                    "Character must be a block",
                );
                continue;
            };
            if self.characters.contains_key(&entry.key) {
                self.diagnostic(Severity::Error, "duplicate_id", source, "Duplicate character ID; first definition retained, both source documents preserved");
                continue;
            }
            // Some non-military roles legitimately repeat; validate only the
            // singleton fields consumed by this projection.
            let singletons: Vec<_> = fields
                .iter()
                .filter(|e| {
                    matches!(
                        e.key.as_str(),
                        "name" | "portraits" | "corps_commander" | "field_marshal"
                    )
                })
                .cloned()
                .collect();
            self.duplicate_fields(file, &entry.key, &singletons);
            let mut character = Character {
                id: entry.key.clone(),
                name_key: pdx::atom(fields, "name").unwrap_or(&entry.key).into(),
                profile_ids: Vec::new(),
                portraits: pdx::block(fields, "portraits").unwrap_or_default().to_vec(),
                source: source.clone(),
                unknown_fields: fields
                    .iter()
                    .filter(|e| {
                        !matches!(
                            e.key.as_str(),
                            "name" | "portraits" | "corps_commander" | "field_marshal"
                        )
                    })
                    .cloned()
                    .collect(),
            };
            for role in [CommanderRole::General, CommanderRole::FieldMarshal] {
                let Some(role_entry) = fields.iter().find(|e| e.key == role.source_key()) else {
                    continue;
                };
                let Some(body) = role_entry.block() else {
                    self.diagnostic(
                        Severity::Error,
                        "invalid_role",
                        source.clone(),
                        "Commander role must be a block",
                    );
                    continue;
                };
                let profile_source = self.source(file, &entry.key, role_entry.span.line);
                self.duplicate_fields(file, &entry.key, body);
                let id = format!("{}::{}", entry.key, role.source_key());
                let stats = CommanderStats {
                    skill: self.uint(body, "skill", &profile_source),
                    attack: self.uint(body, "attack_skill", &profile_source),
                    defense: self.uint(body, "defense_skill", &profile_source),
                    planning: self.uint(body, "planning_skill", &profile_source),
                    logistics: self.uint(body, "logistics_skill", &profile_source),
                };
                let initial_experience = match pdx::atom(body, "experience") {
                    None => Some(0.0),
                    Some(raw) => raw
                        .parse::<f64>()
                        .ok()
                        .filter(|v| v.is_finite() && *v >= 0.0),
                };
                if initial_experience.is_none() {
                    self.diagnostic(
                        Severity::Error,
                        "invalid_experience",
                        profile_source.clone(),
                        "Unsupported experience value",
                    );
                }
                let profile = CommanderProfile {
                    id: id.clone(),
                    character_id: entry.key.clone(),
                    role,
                    stats,
                    initial_experience,
                    trait_ids: pdx::block(body, "traits")
                        .unwrap_or_default()
                        .iter()
                        .map(|e| e.key.clone())
                        .collect(),
                    source: profile_source,
                    unknown_fields: body
                        .iter()
                        .filter(|e| {
                            !matches!(
                                e.key.as_str(),
                                "skill"
                                    | "attack_skill"
                                    | "defense_skill"
                                    | "planning_skill"
                                    | "logistics_skill"
                                    | "traits"
                                    | "experience"
                            )
                        })
                        .cloned()
                        .collect(),
                };
                self.profiles.insert(id.clone(), profile);
                character.profile_ids.push(id);
            }
            self.characters.insert(character.id.clone(), character);
        }
    }
    pub fn import_traits(&mut self, file: &str, text: &str) {
        let Some(entries) = self.parse(file, text) else {
            return;
        };
        for entry in pdx::block(&entries, "leader_traits").unwrap_or(&entries) {
            let Some(fields) = entry.block() else {
                continue;
            };
            let source = self.source(file, &entry.key, entry.span.line);
            if self.traits.contains_key(&entry.key) {
                self.diagnostic(
                    Severity::Error,
                    "duplicate_id",
                    source,
                    "Duplicate trait ID; first definition retained",
                );
                continue;
            }
            self.traits.insert(
                entry.key.clone(),
                TraitDefinition {
                    id: entry.key.clone(),
                    name_key: entry.key.clone(),
                    description_key: format!("{}_desc", entry.key),
                    fields: fields.to_vec(),
                    source,
                },
            );
        }
    }
    pub fn import_localization(&mut self, file: &str, text: &str) {
        self.localization_file(file, text, None);
    }
    /// Runtime only retains requested keys, avoiding a second full localization
    /// database. Original line numbers are retained in diagnostics.
    pub fn import_selected_localization(
        &mut self,
        file: &str,
        text: &str,
        keys: &BTreeSet<String>,
    ) {
        self.localization_file(file, text, Some(keys));
    }
    fn localization_file(&mut self, file: &str, text: &str, keys: Option<&BTreeSet<String>>) {
        // This dialect is deliberately not fed to a generic YAML parser.
        if keys.is_none() {
            self.sources.insert(
                file.into(),
                SourceDocument {
                    text: text.into(),
                    entries: Vec::new(),
                },
            );
        }
        let mut language = None::<String>;
        for (i, line) in text.trim_start_matches('\u{feff}').lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(header) = line.strip_prefix("l_").and_then(|s| s.strip_suffix(':')) {
                language = Some(header.to_owned());
                continue;
            }
            let Some((key, value)) = line.split_once(':') else {
                self.diagnostic(
                    Severity::Warning,
                    "localization_syntax",
                    self.source(file, "", i + 1),
                    "Expected key:version quoted value",
                );
                continue;
            };
            if keys.is_some_and(|keys| !keys.contains(key)) {
                continue;
            }
            let Some(lang) = language.as_ref() else {
                self.diagnostic(
                    Severity::Error,
                    "localization_language",
                    self.source(file, key, i + 1),
                    "Missing l_language header",
                );
                continue;
            };
            match localization_value(value) {
                Ok(value) => {
                    self.localization
                        .languages
                        .entry(lang.clone())
                        .or_default()
                        .insert(key.into(), value);
                }
                Err(message) => self.diagnostic(
                    Severity::Error,
                    "localization_syntax",
                    self.source(file, key, i + 1),
                    message,
                ),
            }
        }
    }
    /// Repeatable: recomputes reference diagnostics without accumulating them.
    pub fn validate(&mut self) {
        self.diagnostics.retain(|d| {
            !matches!(
                d.code.as_str(),
                "unknown_trait"
                    | "missing_localization"
                    | "broken_character_ref"
                    | "broken_profile_ref"
            )
        });
        for profile in self.profiles.values().cloned().collect::<Vec<_>>() {
            if !self.characters.contains_key(&profile.character_id) {
                self.diagnostic(
                    Severity::Error,
                    "broken_character_ref",
                    profile.source.clone(),
                    format!("Missing character {}", profile.character_id),
                );
            }
        }
        for character in self.characters.values().cloned().collect::<Vec<_>>() {
            for id in &character.profile_ids {
                if !self
                    .profiles
                    .get(id)
                    .is_some_and(|p| p.character_id == character.id)
                {
                    self.diagnostic(
                        Severity::Error,
                        "broken_profile_ref",
                        character.source.clone(),
                        format!("Missing or foreign profile {id}"),
                    );
                }
            }
        }
        let profiles: Vec<_> = self.profiles.values().cloned().collect();
        for profile in profiles {
            for id in &profile.trait_ids {
                if !self.traits.contains_key(id) {
                    self.diagnostic(
                        Severity::Warning,
                        "unknown_trait",
                        profile.source.clone(),
                        format!("Unknown trait {id}; ID retained, no invented modifiers"),
                    );
                }
            }
        }
        let names: Vec<_> = self
            .characters
            .values()
            .map(|c| (c.name_key.clone(), c.source.clone()))
            .chain(self.traits.values().flat_map(|t| {
                [
                    (t.name_key.clone(), t.source.clone()),
                    (t.description_key.clone(), t.source.clone()),
                ]
            }))
            .collect();
        for (key, source) in names {
            if !self.localization.contains(&key) {
                self.diagnostic(
                    Severity::Warning,
                    "missing_localization",
                    source,
                    format!("No localization for {key}; fallback displays key/literal"),
                );
            }
        }
    }
    pub fn profile(&self, character: &str, role: CommanderRole) -> Option<&CommanderProfile> {
        self.profiles
            .get(&format!("{character}::{}", role.source_key()))
    }
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}
fn localization_value(value: &str) -> Result<String, String> {
    let value = value.trim_start();
    let prefix = value.find('"').ok_or("Missing opening quote")?;
    if !value[..prefix].trim().chars().all(|c| c.is_ascii_digit()) {
        return Err("Invalid localization version".into());
    }
    let mut out = String::new();
    let mut chars = value[prefix + 1..].chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let rest: String = chars.collect();
                if !rest.trim().is_empty() && !rest.trim().starts_with('#') {
                    return Err("Unexpected content after closing quote".into());
                }
                return Ok(out);
            }
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(c) => {
                    out.push('\\');
                    out.push(c);
                }
                None => break,
            },
            c => out.push(c),
        }
    }
    Err("Unterminated localization string".into())
}
