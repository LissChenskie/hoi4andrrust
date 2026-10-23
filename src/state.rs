use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Atom(String),
    Open,
    Close,
    Equals,
}

#[derive(Debug)]
enum Value {
    Atom(String),
    Block(Vec<Entry>),
}

#[derive(Debug)]
enum Entry {
    Pair(String, Value),
    Bare(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StateEconomy {
    pub infrastructure: u16,
    pub civilian_factories: u16,
    pub military_factories: u16,
    pub dockyards: u16,
    pub synthetic_refineries: u16,
    pub fuel_silos: u16,
    pub rocket_sites: u16,
    pub nuclear_reactors: u16,
}

impl StateEconomy {
    /// Number of built structures that consume a shared state building slot.
    pub fn occupied_shared_slots(self) -> u16 {
        self.civilian_factories
            .saturating_add(self.military_factories)
            .saturating_add(self.dockyards)
            .saturating_add(self.synthetic_refineries)
            .saturating_add(self.fuel_silos)
            .saturating_add(self.rocket_sites)
            .saturating_add(self.nuclear_reactors)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDefinition {
    pub id: u32,
    pub province_ids: Vec<u32>,
    pub state_category: Option<String>,
    pub shared_building_slots: u16,
    pub economy: StateEconomy,
}

#[derive(Debug, Default)]
pub struct StateMembership {
    pub state_by_province: HashMap<u32, u32>,
    pub economy_by_state: HashMap<u32, StateEconomy>,
    pub shared_slots_by_state: HashMap<u32, u16>,
    pub state_count: usize,
}

fn read_building_counts(entries: &[Entry], economy: &mut StateEconomy) {
    for entry in entries {
        let Entry::Pair(key, Value::Atom(raw)) = entry else { continue };
        let Ok(value) = raw.parse::<u16>() else { continue };
        match key.as_str() {
            "infrastructure" => economy.infrastructure = value,
            "industrial_complex" => economy.civilian_factories = value,
            "arms_factory" => economy.military_factories = value,
            "dockyard" => economy.dockyards = value,
            "synthetic_refinery" => economy.synthetic_refineries = value,
            "fuel_silo" => economy.fuel_silos = value,
            "rocket_site" => economy.rocket_sites = value,
            "nuclear_reactor" => economy.nuclear_reactors = value,
            _ => {}
        }
    }
}

fn read_economy(fields: &[Entry]) -> StateEconomy {
    let mut economy = StateEconomy::default();
    for field in fields {
        match field {
            Entry::Pair(name, Value::Block(buildings)) if name == "buildings" => {
                read_building_counts(buildings, &mut economy);
            }
            Entry::Pair(name, Value::Block(history)) if name == "history" => {
                for history_field in history {
                    if let Entry::Pair(building_name, Value::Block(buildings)) = history_field {
                        if building_name == "buildings" {
                            read_building_counts(buildings, &mut economy);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    economy
}

fn tokenize(source: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        if ch == '#' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }

        match ch {
            '{' => {
                tokens.push(Token::Open);
                i += 1;
            }
            '}' => {
                tokens.push(Token::Close);
                i += 1;
            }
            '=' => {
                tokens.push(Token::Equals);
                i += 1;
            }
            '"' => {
                i += 1;
                let mut value = String::new();
                let mut closed = false;
                while i < chars.len() {
                    match chars[i] {
                        '"' => {
                            i += 1;
                            closed = true;
                            break;
                        }
                        '\\' if i + 1 < chars.len() => {
                            i += 1;
                            value.push(chars[i]);
                            i += 1;
                        }
                        other => {
                            value.push(other);
                            i += 1;
                        }
                    }
                }
                if !closed {
                    return Err("Unterminated quoted string in state file".to_owned());
                }
                tokens.push(Token::Atom(value));
            }
            _ => {
                let start = i;
                while i < chars.len()
                    && !chars[i].is_whitespace()
                    && !matches!(chars[i], '{' | '}' | '=' | '#' | '"')
                {
                    i += 1;
                }
                if start == i {
                    return Err(format!("Unexpected character '{}' in state file", chars[i]));
                }
                tokens.push(Token::Atom(chars[start..i].iter().collect()));
            }
        }
    }

    Ok(tokens)
}

fn parse_value(tokens: &[Token], cursor: &mut usize) -> Result<Value, String> {
    match tokens.get(*cursor) {
        Some(Token::Atom(value)) => {
            *cursor += 1;
            Ok(Value::Atom(value.clone()))
        }
        Some(Token::Open) => {
            *cursor += 1;
            Ok(Value::Block(parse_entries(tokens, cursor, true)?))
        }
        Some(token) => Err(format!("Expected a value, got {token:?}")),
        None => Err("Unexpected end of state file while reading a value".to_owned()),
    }
}

fn parse_entries(
    tokens: &[Token],
    cursor: &mut usize,
    expect_close: bool,
) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();

    while *cursor < tokens.len() {
        match &tokens[*cursor] {
            Token::Close => {
                *cursor += 1;
                if expect_close {
                    return Ok(entries);
                }
                return Err("Unexpected closing brace in state file".to_owned());
            }
            Token::Equals => return Err("Unexpected '=' in state file".to_owned()),
            Token::Open => return Err("Unexpected '{' without a key in state file".to_owned()),
            Token::Atom(key) => {
                let key = key.clone();
                *cursor += 1;
                if matches!(tokens.get(*cursor), Some(Token::Equals)) {
                    *cursor += 1;
                    let value = parse_value(tokens, cursor)?;
                    entries.push(Entry::Pair(key, value));
                } else {
                    entries.push(Entry::Bare(key));
                }
            }
        }
    }

    if expect_close {
        Err("Missing closing brace in state file".to_owned())
    } else {
        Ok(entries)
    }
}

fn numeric_atom(value: &Value, field: &str) -> Result<u32, String> {
    match value {
        Value::Atom(raw) => raw
            .parse::<u32>()
            .map_err(|_| format!("Invalid numeric {field}: {raw}")),
        Value::Block(_) => Err(format!("Expected numeric {field}, found a block")),
    }
}

/// Parse state IDs, province membership, and initial state-level industry from Clausewitz data.
pub fn parse_state_file(source: &str) -> Result<Vec<StateDefinition>, String> {
    let tokens = tokenize(source)?;
    let mut cursor = 0;
    let root = parse_entries(&tokens, &mut cursor, false)?;
    let mut states = Vec::new();

    for entry in root {
        let Entry::Pair(key, Value::Block(block)) = entry else {
            continue;
        };
        if key != "state" {
            continue;
        }

        let economy = read_economy(&block);
        let mut id = None;
        let mut provinces = None;
        let mut state_category = None;
        for field in block {
            match field {
                Entry::Pair(field_name, value) if field_name == "id" => {
                    id = Some(numeric_atom(&value, "state ID")?);
                }
                Entry::Pair(field_name, Value::Atom(category)) if field_name == "state_category" => {
                    state_category = Some(category);
                }
                Entry::Pair(field_name, Value::Block(values)) if field_name == "provinces" => {
                    let mut province_ids = Vec::new();
                    for value in values {
                        match value {
                            Entry::Bare(raw) => {
                                let province_id = raw.parse::<u32>().map_err(|_| {
                                    format!("Invalid province ID '{raw}' in state definition")
                                })?;
                                province_ids.push(province_id);
                            }
                            Entry::Pair(_, Value::Atom(raw)) => {
                                if let Ok(province_id) = raw.parse::<u32>() {
                                    province_ids.push(province_id);
                                }
                            }
                            _ => {}
                        }
                    }
                    provinces = Some(province_ids);
                }
                _ => {}
            }
        }

        let id = id.ok_or_else(|| "State block is missing its top-level id".to_owned())?;
        let province_ids =
            provinces.ok_or_else(|| format!("State {id} is missing its provinces block"))?;
        if id == 0 {
            return Err("State ID 0 is invalid".to_owned());
        }
        if province_ids.is_empty() {
            return Err(format!("State {id} contains no province IDs"));
        }

        let mut seen = std::collections::HashSet::new();
        for province_id in &province_ids {
            if *province_id == 0 {
                return Err(format!("State {id} contains invalid province ID 0"));
            }
            if !seen.insert(*province_id) {
                return Err(format!("State {id} lists province {province_id} more than once"));
            }
        }
        states.push(StateDefinition {
            id,
            province_ids,
            state_category,
            shared_building_slots: 0,
            economy,
        });
    }

    if states.is_empty() {
        return Err("No state = { ... } blocks found in state file".to_owned());
    }
    Ok(states)
}


fn parse_state_owner_history(entries: &[Entry], map_date: (u32, u32, u32)) -> Option<String> {
    let mut owner = None;
    let mut latest_date = None;

    // The undated owner is the default. A dated history block overrides it only
    // when its date is at or before the selected bookmark.
    for entry in entries {
        let Entry::Pair(key, value) = entry else { continue };
        if key == "owner" {
            if let Value::Atom(tag) = value {
                if !tag.is_empty() && tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    owner = Some(tag.clone());
                }
            }
            continue;
        }

        let Some(date) = parse_history_date(key) else { continue };
        if date > map_date || latest_date.is_some_and(|previous| date < previous) {
            continue;
        }
        let Value::Block(date_entries) = value else { continue };
        if let Some(tag) = date_entries.iter().find_map(|entry| match entry {
            Entry::Pair(field, Value::Atom(tag))
                if field == "owner"
                    && !tag.is_empty()
                    && tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') =>
            {
                Some(tag.clone())
            }
            _ => None,
        }) {
            owner = Some(tag);
            latest_date = Some(date);
        }
    }

    owner
}

fn parse_history_date(value: &str) -> Option<(u32, u32, u32)> {
    let mut parts = value.split('.');
    let year = parts.next()?.parse::<u32>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() || month == 0 || month > 12 || day == 0 || day > 31 {
        return None;
    }
    Some((year, month, day))
}

/// Read owners from the Clausewitz syntax tree, not from line patterns.
pub fn parse_state_owners(
    source: &str,
    map_date: (u32, u32, u32),
) -> Result<HashMap<u32, String>, String> {
    let tokens = tokenize(source)?;
    let mut cursor = 0;
    let root = parse_entries(&tokens, &mut cursor, false)?;
    let mut owners = HashMap::new();

    for entry in root {
        let Entry::Pair(key, Value::Block(state_fields)) = entry else { continue };
        if key != "state" {
            continue;
        }

        let mut state_id = None;
        let mut history = None;
        for field in &state_fields {
            match field {
                Entry::Pair(name, value) if name == "id" => {
                    state_id = Some(numeric_atom(value, "state ID")?);
                }
                Entry::Pair(name, Value::Block(entries)) if name == "history" => {
                    history = Some(entries.as_slice());
                }
                _ => {}
            }
        }

        let Some(id) = state_id else { continue };
        if let Some(entries) = history {
            if let Some(owner) = parse_state_owner_history(entries, map_date) {
                owners.insert(id, owner);
            }
        }
    }

    Ok(owners)
}

/// Parse state-category definitions and their base shared building-slot counts.
pub fn parse_state_categories(source: &str) -> Result<HashMap<String, u16>, String> {
    let tokens = tokenize(source)?;
    let mut cursor = 0;
    let root = parse_entries(&tokens, &mut cursor, false)?;
    let mut categories = HashMap::new();

    for entry in root {
        let Entry::Pair(root_name, Value::Block(category_entries)) = entry else { continue };
        if root_name != "state_categories" {
            continue;
        }
        for category_entry in category_entries {
            let Entry::Pair(category_name, Value::Block(fields)) = category_entry else { continue };
            for field in fields {
                if let Entry::Pair(name, Value::Atom(raw)) = field {
                    if name == "local_building_slots" {
                        let slots = raw.parse::<u16>().map_err(|_| {
                            format!("Invalid local_building_slots value '{raw}' in category {category_name}")
                        })?;
                        categories.insert(category_name.clone(), slots);
                    }
                }
            }
        }
    }

    Ok(categories)
}

fn default_state_category_slots() -> HashMap<String, u16> {
    [
        ("wasteland", 0), ("enclave", 0), ("tiny_island", 0),
        ("pastoral", 1), ("small_island", 1), ("rural", 2),
        ("town", 4), ("large_town", 5), ("city", 6),
        ("large_city", 8), ("metropolis", 10), ("megalopolis", 12),
    ].into_iter().map(|(name, slots)| (name.to_owned(), slots)).collect()
}

fn load_state_category_slots(directory: &Path) -> Result<HashMap<String, u16>, String> {
    let mut categories = default_state_category_slots();
    if !directory.is_dir() {
        return Ok(categories);
    }

    let entries = fs::read_dir(directory)
        .map_err(|error| format!("Could not read state-category directory {}: {error}", directory.display()))?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("txt"))
        .collect();
    paths.sort();

    for path in paths {
        let source = fs::read_to_string(&path)
            .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
        let parsed = parse_state_categories(&source)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        categories.extend(parsed);
    }
    Ok(categories)
}

pub fn load_state_membership(directory: &Path, category_directory: &Path) -> Result<StateMembership, String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("Could not read state directory {}: {error}", directory.display()))?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("txt"))
        .collect();
    paths.sort();

    if paths.is_empty() {
        return Err(format!("No state .txt files found in {}", directory.display()));
    }

    let category_slots = load_state_category_slots(category_directory)?;
    let mut membership = StateMembership::default();
    let mut state_ids = std::collections::HashSet::new();

    for path in paths {
        let source = fs::read_to_string(&path)
            .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
        let definitions = parse_state_file(&source)
            .map_err(|error| format!("{}: {error}", path.display()))?;

        for state in definitions {
            if !state_ids.insert(state.id) {
                return Err(format!("Duplicate state ID {} (found in {})", state.id, path.display()));
            }
            membership.economy_by_state.insert(state.id, state.economy);
            let shared_slots = state.state_category.as_ref()
                .and_then(|category| category_slots.get(category))
                .copied()
                .unwrap_or(state.shared_building_slots);
            membership.shared_slots_by_state.insert(state.id, shared_slots);
            for province_id in state.province_ids {
                if let Some(previous_state) = membership.state_by_province.insert(province_id, state.id) {
                    return Err(format!(
                        "Province {province_id} is assigned to both state {previous_state} and state {}",
                        state.id
                    ));
                }
            }
        }
    }

    membership.state_count = state_ids.len();
    Ok(membership)
}

#[cfg(test)]
mod tests {
    use super::{parse_state_categories, parse_state_file};


    #[test]
    fn parses_state_category_and_its_shared_slot_limit() {
        let categories = parse_state_categories(r#"
            state_categories = {
                rural = { local_building_slots = 2 color = { 1 2 3 } }
                metropolis = { local_building_slots = 10 }
            }
        "#).unwrap();
        assert_eq!(categories.get("rural"), Some(&2));
        assert_eq!(categories.get("metropolis"), Some(&10));
    }

    #[test]
    fn reads_state_category_from_state_definition() {
        let states = parse_state_file(
            r#"state = { id = 4 state_category = city provinces = { 41 42 } }"#
        ).unwrap();
        assert_eq!(states[0].state_category.as_deref(), Some("city"));
    }

    #[test]
    fn parses_owner_from_inline_history_block() {
        let source = r#"state = { id = 42 history = { owner = GER } provinces = { 100 101 } }"#;
        let owners = super::parse_state_owners(source, (1936, 1, 1)).unwrap();
        assert_eq!(owners.get(&42).map(String::as_str), Some("GER"));
    }

    #[test]
    fn uses_latest_owner_date_not_after_bookmark() {
        let source = r#"
            state = {
                id = 42
                history = {
                    owner = GER
                    1936.1.1 = { owner = POL }
                    1939.9.1 = { owner = SOV }
                }
                provinces = { 100 }
            }
        "#;
        let owners = super::parse_state_owners(source, (1936, 1, 1)).unwrap();
        assert_eq!(owners.get(&42).map(String::as_str), Some("POL"));
    }

    #[test]
    fn does_not_treat_add_owner_as_state_owner() {
        let source = r#"state = { id = 42 history = { add_owner = GER } provinces = { 100 } }"#;
        let owners = super::parse_state_owners(source, (1936, 1, 1)).unwrap();
        assert!(owners.get(&42).is_none());
    }

    #[test]
    fn parses_state_id_and_provinces_while_ignoring_nested_history() {
        let source = r#"
            state = {
                id = 12
                name = "STATE_TEST"
                provinces = { 101 102 103 }
                history = {
                    owner = GER
                    victory_points = { 102 5 }
                }
            }
        "#;
        let states = parse_state_file(source).expect("valid state file");
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].id, 12);
        assert_eq!(states[0].province_ids, vec![101, 102, 103]);
        assert_eq!(states[0].economy, super::StateEconomy::default());
    }

    #[test]
    fn parses_multiple_state_blocks_and_comments() {
        let source = r#"
            # first state
            state = { id = 1 provinces = { 11 12 } }
            state = { id = 2 provinces = { 21 } }
        "#;
        let states = parse_state_file(source).expect("valid state definitions");
        assert_eq!(states.iter().map(|state| state.id).collect::<Vec<_>>(), vec![1, 2]);
    }


    #[test]
    fn parses_state_industry_from_history_buildings_without_counting_province_blocks() {
        let source = r#"
            state = {
                id = 12
                provinces = { 101 102 }
                history = {
                    owner = GER
                    buildings = {
                        infrastructure = 6
                        industrial_complex = 4
                        arms_factory = 3
                        dockyard = 2
                        synthetic_refinery = 1
                        fuel_silo = 1
                        rocket_site = 2
                        nuclear_reactor = 1
                        101 = { naval_base = 4 }
                    }
                }
            }
        "#;
        let states = parse_state_file(source).unwrap();
        assert_eq!(states[0].economy, super::StateEconomy {
            infrastructure: 6,
            civilian_factories: 4,
            military_factories: 3,
            dockyards: 2,
            synthetic_refineries: 1,
            fuel_silos: 1,
            rocket_sites: 2,
            nuclear_reactors: 1,
        });
        assert_eq!(states[0].economy.occupied_shared_slots(), 14);
    }

    #[test]
    fn defaults_missing_industry_to_zero() {
        let source = r#"state = { id = 7 provinces = { 71 } history = { owner = FRA } }"#;
        let states = parse_state_file(source).unwrap();
        assert_eq!(states[0].economy, super::StateEconomy::default());
    }

    #[test]
    fn rejects_missing_closing_brace() {
        let source = "state = { id = 1 provinces = { 2 3 }";
        assert!(parse_state_file(source).unwrap_err().contains("Missing closing brace"));
    }

    #[test]
    fn rejects_duplicate_provinces_within_state() {
        let source = "state = { id = 1 provinces = { 2 2 } }";
        assert!(parse_state_file(source).unwrap_err().contains("lists province 2 more than once"));
    }
}
