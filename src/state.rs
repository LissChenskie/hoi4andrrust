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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDefinition {
    pub id: u32,
    pub province_ids: Vec<u32>,
}

#[derive(Debug, Default)]
pub struct StateMembership {
    pub state_by_province: HashMap<u32, u32>,
    pub state_count: usize,
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

/// Parse one Clausewitz state file. Only top-level state IDs and province membership are read;
/// nested history/building data is tokenized safely but not interpreted yet.
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

        let mut id = None;
        let mut provinces = None;
        for field in block {
            match field {
                Entry::Pair(field_name, value) if field_name == "id" => {
                    id = Some(numeric_atom(&value, "state ID")?);
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
        states.push(StateDefinition { id, province_ids });
    }

    if states.is_empty() {
        return Err("No state = { ... } blocks found in state file".to_owned());
    }
    Ok(states)
}

pub fn load_state_membership(directory: &Path) -> Result<StateMembership, String> {
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
    use super::parse_state_file;

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
