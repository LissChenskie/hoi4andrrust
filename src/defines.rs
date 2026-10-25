//! Literal Lua defines with qualified names and override provenance. No Lua execution.
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Default)]
pub struct LiteralDefines {
    pub values: BTreeMap<String, f64>,
    pub unresolved: BTreeSet<String>,
}
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Atom(String),
    Quoted(String),
    Symbol(char),
    Newline,
}
fn tokens(source: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut c = source.chars().peekable();
    while let Some(ch) = c.next() {
        if ch == '\n' || ch == '\r' {
            if ch == '\r' && c.peek() == Some(&'\n') {
                c.next();
            }
            out.push(Token::Newline);
            continue;
        }
        if ch.is_whitespace() || ch == '\u{feff}' {
            continue;
        }
        if ch == '-' && c.peek() == Some(&'-') {
            c.next();
            let mut probe = c.clone();
            let mut eq = 0;
            if probe.next() == Some('[') {
                while probe.peek() == Some(&'=') {
                    probe.next();
                    eq += 1;
                }
                if probe.next() == Some('[') {
                    c = probe;
                    let end = format!("]{}]", "=".repeat(eq));
                    let mut tail = String::new();
                    for ch in c.by_ref() {
                        tail.push(ch);
                        if tail.ends_with(&end) {
                            break;
                        }
                        if tail.len() > end.len() * 2 {
                            tail.remove(0);
                        }
                    }
                    continue;
                }
            }
            for ch in c.by_ref() {
                if ch == '\n' || ch == '\r' {
                    out.push(Token::Newline);
                    break;
                }
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            let mut text = String::new();
            while let Some(next) = c.next() {
                if next == ch {
                    break;
                }
                if next == '\\' {
                    if let Some(escaped) = c.next() {
                        text.push(escaped);
                    }
                } else {
                    text.push(next);
                }
            }
            out.push(Token::Quoted(text));
            continue;
        }
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '+' | '-') {
            let mut word = ch.to_string();
            while let Some(&next) = c.peek() {
                if next.is_ascii_alphanumeric() || matches!(next, '_' | '.') {
                    word.push(next);
                    c.next();
                } else {
                    break;
                }
            }
            out.push(Token::Atom(word));
        } else {
            out.push(Token::Symbol(ch));
        }
    }
    out
}
fn qualified(scope: &str, key: &str) -> String {
    if key == "NDefines" || key.starts_with("NDefines.") || scope.is_empty() {
        key.into()
    } else {
        format!("{scope}.{key}")
    }
}
pub fn read(source: &str) -> LiteralDefines {
    fn table(t: &[Token], pos: &mut usize, scope: &str, depth: usize, out: &mut LiteralDefines) {
        if depth > 64 {
            return;
        }
        while *pos < t.len() {
            if t[*pos] == Token::Symbol('}') {
                *pos += 1;
                return;
            }
            let key = match &t[*pos] {
                Token::Atom(s)
                    if s.chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_') =>
                {
                    s.clone()
                }
                _ => {
                    *pos += 1;
                    continue;
                }
            };
            *pos += 1;
            if t.get(*pos) != Some(&Token::Symbol('=')) {
                continue;
            }
            *pos += 1;
            let path = qualified(scope, &key);
            if t.get(*pos) == Some(&Token::Symbol('{')) {
                *pos += 1;
                table(t, pos, &path, depth + 1, out);
                continue;
            }
            let value = t.get(*pos).and_then(|v| match v {
                Token::Atom(s) => s.parse::<f64>().ok().filter(|n| n.is_finite()),
                _ => None,
            });
            let literal_end = matches!(
                t.get(*pos + 1),
                None | Some(Token::Newline) | Some(Token::Symbol(',' | ';' | '}'))
            );
            if let Some(value) = value.filter(|_| literal_end) {
                out.values.insert(path.clone(), value);
                out.unresolved.remove(&path);
            } else {
                out.unresolved.insert(path.clone());
                out.values.remove(&path);
            }
            while *pos < t.len()
                && !matches!(
                    t.get(*pos),
                    Some(Token::Newline) | Some(Token::Symbol(',' | ';' | '}'))
                )
            {
                *pos += 1;
            }
        }
    }
    let mut result = LiteralDefines::default();
    table(&tokens(source), &mut 0, "", 0, &mut result);
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tables_and_qualified_overrides_preserve_namespaces() {
        let d = read(
            "NDefines={NCountry={BASE_RESEARCH_SLOTS=2},NTechnology={BASE_TECH_COST=110},NMilitary={BASE_TECH_COST=60}}\nNDefines.NTechnology.BASE_TECH_COST=100\n--[[ NDefines.NTechnology.BASE_TECH_COST=999 ]]\n",
        );
        assert_eq!(d.values["NDefines.NTechnology.BASE_TECH_COST"], 100.0);
        assert_eq!(d.values["NDefines.NMilitary.BASE_TECH_COST"], 60.0);
        assert_eq!(d.values["NDefines.NCountry.BASE_RESEARCH_SLOTS"], 2.0);
    }
    #[test]
    fn unresolved_override_removes_an_earlier_literal() {
        let d = read(
            "NDefines.NTechnology.BASE_TECH_COST=110\nNDefines.NTechnology.BASE_TECH_COST=100*2",
        );
        assert!(!d.values.contains_key("NDefines.NTechnology.BASE_TECH_COST"));
        assert!(d.unresolved.contains("NDefines.NTechnology.BASE_TECH_COST"));
    }
}
