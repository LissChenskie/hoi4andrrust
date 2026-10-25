//! Ordered script AST. Repeated keys, operators, quoted atoms and locations survive parsing.
//! It deliberately does not execute Lua or invent values for script expressions.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub offset: usize,
    pub line: usize,
    pub column: usize,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Atom(String),
    Block(Vec<Entry>),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub key: String,
    pub operator: String,
    pub value: Option<Value>,
    pub span: Span,
}
impl Entry {
    pub fn atom(&self) -> Option<&str> {
        match self.value.as_ref()? {
            Value::Atom(s) => Some(s),
            _ => None,
        }
    }
    pub fn block(&self) -> Option<&[Entry]> {
        match self.value.as_ref()? {
            Value::Block(b) => Some(b),
            _ => None,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Atom(String),
    Open,
    Close,
    Op(String),
}
#[derive(Debug)]
struct Token {
    kind: Kind,
    span: Span,
}
fn lex(source: &str) -> Result<Vec<Token>, String> {
    let mut out = Vec::new();
    let mut chars = source.char_indices().peekable();
    let (mut line, mut col) = (1, 1);
    while let Some((offset, c)) = chars.next() {
        if offset == 0 && c == '\u{feff}' {
            continue;
        }
        let span = Span {
            offset,
            line,
            column: col,
        };
        col += 1;
        if c == '\n' {
            line += 1;
            col = 1;
            continue;
        }
        if c.is_whitespace() || c == '\u{feff}' {
            continue;
        }
        if c == '#' {
            for (_, v) in chars.by_ref() {
                if v == '\n' {
                    line += 1;
                    col = 1;
                    break;
                }
                col += 1;
            }
            continue;
        }
        let kind = match c {
            '{' => Kind::Open,
            '}' => Kind::Close,
            '=' | '<' | '>' | '!' | '?' => {
                let mut op = c.to_string();
                if chars.peek().is_some_and(|(_, c)| *c == '=') {
                    chars.next();
                    col += 1;
                    op.push('=');
                }
                if op == "!" || op == "?" {
                    return Err(format!(
                        "{}:{}: invalid operator {op}",
                        span.line, span.column
                    ));
                }
                Kind::Op(op)
            }
            '"' => {
                let mut s = String::new();
                let mut closed = false;
                while let Some((_, ch)) = chars.next() {
                    col += 1;
                    if ch == '"' {
                        closed = true;
                        break;
                    }
                    if ch == '\n' {
                        line += 1;
                        col = 1;
                    }
                    if ch == '\\' {
                        if let Some((_, escaped)) = chars.next() {
                            col += 1;
                            match escaped {
                                '"' | '\\' => s.push(escaped),
                                'n' => s.push('\n'),
                                't' => s.push('\t'),
                                other => {
                                    s.push('\\');
                                    s.push(other);
                                }
                            }
                        }
                    } else {
                        s.push(ch);
                    }
                }
                if !closed {
                    return Err(format!(
                        "{}:{}: unterminated string",
                        span.line, span.column
                    ));
                }
                Kind::Atom(s)
            }
            _ => {
                let mut s = c.to_string();
                // @[ ... ] is one expression, not a collection of AST keys.
                if c == '@' && chars.peek().is_some_and(|(_, c)| *c == '[') {
                    let mut depth = 0usize;
                    let mut closed = false;
                    for (_, ch) in chars.by_ref() {
                        s.push(ch);
                        col += 1;
                        if ch == '\n' {
                            line += 1;
                            col = 1;
                        }
                        if ch == '[' {
                            depth += 1;
                        }
                        if ch == ']' {
                            depth = depth.saturating_sub(1);
                            if depth == 0 {
                                closed = true;
                                break;
                            }
                        }
                    }
                    if !closed {
                        return Err(format!(
                            "{}:{}: unclosed expression",
                            span.line, span.column
                        ));
                    }
                } else {
                    while let Some(&(_, ch)) = chars.peek() {
                        if ch.is_whitespace()
                            || matches!(ch, '{' | '}' | '=' | '<' | '>' | '!' | '?' | '#' | '"')
                        {
                            break;
                        }
                        chars.next();
                        col += 1;
                        s.push(ch);
                    }
                }
                Kind::Atom(s)
            }
        };
        out.push(Token { kind, span });
    }
    Ok(out)
}
fn entries(tokens: &[Token], pos: &mut usize, depth: usize) -> Result<Vec<Entry>, String> {
    if depth > 256 {
        return Err("Script nesting exceeds 256 levels".into());
    }
    let mut out = Vec::new();
    while let Some(token) = tokens.get(*pos) {
        if token.kind == Kind::Close {
            if depth == 0 {
                return Err(format!(
                    "{}:{}: unexpected closing brace",
                    token.span.line, token.span.column
                ));
            }
            *pos += 1;
            return Ok(out);
        }
        let Kind::Atom(key) = &token.kind else {
            return Err(format!(
                "{}:{}: expected key or list atom",
                token.span.line, token.span.column
            ));
        };
        let key = key.clone();
        let span = token.span.clone();
        *pos += 1;
        let mut operator = String::new();
        if let Some(Token {
            kind: Kind::Op(op), ..
        }) = tokens.get(*pos)
        {
            operator = op.clone();
            *pos += 1;
        }
        let value = match tokens.get(*pos) {
            Some(Token {
                kind: Kind::Open, ..
            }) => {
                *pos += 1;
                Some(Value::Block(entries(tokens, pos, depth + 1)?))
            }
            Some(Token {
                kind: Kind::Atom(value),
                ..
            }) if !operator.is_empty() => {
                let v = value.clone();
                *pos += 1;
                Some(Value::Atom(v))
            }
            _ if !operator.is_empty() => {
                return Err(format!(
                    "{}:{}: expected value for {key}",
                    span.line, span.column
                ));
            }
            _ => None,
        };
        out.push(Entry {
            key,
            operator,
            value,
            span,
        });
    }
    if depth > 0 {
        Err("Unclosed brace at end of script".into())
    } else {
        Ok(out)
    }
}
pub fn parse_pdx(source: &str) -> Result<Vec<Entry>, String> {
    entries(&lex(source)?, &mut 0, 0)
}
pub fn find<'a>(b: &'a [Entry], key: &str) -> Option<&'a Entry> {
    b.iter().rev().find(|e| e.key == key)
}
pub fn atom<'a>(b: &'a [Entry], key: &str) -> Option<&'a str> {
    find(b, key)?.atom()
}
pub fn block<'a>(b: &'a [Entry], key: &str) -> Option<&'a [Entry]> {
    find(b, key)?.block()
}
pub fn number(b: &[Entry], key: &str) -> Option<f64> {
    atom(b, key)?.parse().ok().filter(|x: &f64| x.is_finite())
}
pub fn names(b: &[Entry]) -> std::collections::BTreeSet<String> {
    b.iter()
        .filter(|e| e.value.is_none() || matches!(e.atom(), Some("1" | "yes")))
        .map(|e| e.key.clone())
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retains_strings_operators_repetition_and_positions() {
        let b = parse_pdx("\u{feff}# comment\nx={path=\"}\" path=\"# not comment\" date>1939.1.1 list={a b} text=\"a\\\"b\\nc\"}").unwrap();
        let x = b[0].block().unwrap();
        assert_eq!(x[0].atom(), Some("}"));
        assert_eq!(x[1].atom(), Some("# not comment"));
        assert_eq!(x[2].operator, ">");
        assert_eq!(x[4].atom(), Some("a\"b\nc"));
        assert_eq!(b[0].span.line, 2);
    }
    #[test]
    fn expressions_preserved_and_invalid_input_rejected() {
        let b = parse_pdx("@a=1 research_cost=@[ @a * 2 ]").unwrap();
        assert_eq!(b[1].atom(), Some("@[ @a * 2 ]"));
        for bad in ["a={b=", "a=}", "a=\"missing", "a=@[ 3", "a={}"] {
            if bad != "a={}" {
                assert!(parse_pdx(bad).is_err(), "{bad}");
            }
        }
        assert!(parse_pdx(&format!("{}{}", "a={".repeat(300), "}".repeat(300))).is_err());
    }
}
