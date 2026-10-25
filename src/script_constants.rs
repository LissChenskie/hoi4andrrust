//! Scoped numeric script constants. Arithmetic is parsed without executing code.
use crate::pdx::{Entry, Value};
use std::collections::BTreeMap;
pub type Constants = BTreeMap<String, String>;
pub fn scope(entries: &[Entry], parent: &Constants) -> Constants {
    let mut values = parent.clone();
    for e in entries.iter().filter(|e| e.key.starts_with('@')) {
        if let Some(v) = e.atom() {
            values.insert(e.key.clone(), v.into());
        }
    }
    values
}
pub fn number(raw: &str, constants: &Constants) -> Option<f64> {
    number_at(raw, constants, 0)
}
fn number_at(raw: &str, constants: &Constants, depth: u8) -> Option<f64> {
    if depth > 32 {
        return None;
    }
    let raw = raw.trim();
    if let Ok(n) = raw.parse::<f64>() {
        return n.is_finite().then_some(n);
    }
    if let Some(value) = constants.get(raw) {
        return number_at(value, constants, depth + 1);
    }
    if let Some(body) = raw.strip_prefix("@[").and_then(|s| s.strip_suffix(']')) {
        let mut p = Expression {
            source: body.as_bytes(),
            at: 0,
            constants,
            depth,
        };
        let value = p.sum()?;
        p.space();
        return (p.at == p.source.len() && value.is_finite()).then_some(value);
    }
    None
}
struct Expression<'a> {
    source: &'a [u8],
    at: usize,
    constants: &'a Constants,
    depth: u8,
}
impl Expression<'_> {
    fn space(&mut self) {
        while self
            .source
            .get(self.at)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.at += 1;
        }
    }
    fn take(&mut self, c: u8) -> bool {
        self.space();
        if self.source.get(self.at) == Some(&c) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn sum(&mut self) -> Option<f64> {
        let mut n = self.product()?;
        loop {
            if self.take(b'+') {
                n += self.product()?;
            } else if self.take(b'-') {
                n -= self.product()?;
            } else {
                break;
            }
        }
        n.is_finite().then_some(n)
    }
    fn product(&mut self) -> Option<f64> {
        let mut n = self.unary()?;
        loop {
            if self.take(b'*') {
                n *= self.unary()?;
            } else if self.take(b'/') {
                let d = self.unary()?;
                if d == 0.0 {
                    return None;
                }
                n /= d;
            } else {
                break;
            }
        }
        n.is_finite().then_some(n)
    }
    fn unary(&mut self) -> Option<f64> {
        if self.depth > 32 {
            return None;
        }
        self.depth += 1;
        let result = if self.take(b'-') {
            self.unary().map(|v| -v)
        } else if self.take(b'+') {
            self.unary()
        } else if self.take(b'(') {
            let n = self.sum()?;
            if !self.take(b')') {
                return None;
            }
            Some(n)
        } else {
            self.atom()
        };
        self.depth -= 1;
        result
    }
    fn atom(&mut self) -> Option<f64> {
        self.space();
        let start = self.at;
        if self.source.get(self.at) == Some(&b'@') {
            self.at += 1;
            while self
                .source
                .get(self.at)
                .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
            {
                self.at += 1;
            }
            let id = std::str::from_utf8(&self.source[start..self.at]).ok()?;
            return number_at(id, self.constants, self.depth + 1);
        }
        while self
            .source
            .get(self.at)
            .is_some_and(|c| c.is_ascii_digit() || *c == b'.')
        {
            self.at += 1;
        }
        if self
            .source
            .get(self.at)
            .is_some_and(|c| *c == b'e' || *c == b'E')
        {
            self.at += 1;
            if self
                .source
                .get(self.at)
                .is_some_and(|c| *c == b'+' || *c == b'-')
            {
                self.at += 1;
            }
            while self.source.get(self.at).is_some_and(u8::is_ascii_digit) {
                self.at += 1;
            }
        }
        std::str::from_utf8(&self.source[start..self.at])
            .ok()?
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
    }
}
pub fn resolve(entries: &[Entry], parent: &Constants) -> Vec<Entry> {
    let constants = scope(entries, parent);
    entries
        .iter()
        .map(|e| {
            let mut e = e.clone();
            e.value = match &e.value {
                Some(Value::Atom(v)) if v.starts_with('@') => number(v, &constants)
                    .map(|n| Value::Atom(n.to_string()))
                    .or_else(|| e.value.clone()),
                Some(Value::Block(b)) => Some(Value::Block(resolve(b, &constants))),
                other => other.clone(),
            };
            e
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arithmetic_rejects_cycles_and_executable_expressions() {
        let vars = Constants::from([
            ("@base".into(), "2".into()),
            ("@step".into(), "@[ @base * 3 ]".into()),
            ("@cycle".into(), "@cycle".into()),
        ]);
        assert_eq!(number("@[ @step / (1 + @base) - 0.5 ]", &vars), Some(1.5));
        for s in [
            "@cycle",
            "@[ 1 / 0 ]",
            "@[ arbitrary_function() ]",
            "@[ 1 + ]",
        ] {
            assert!(number(s, &vars).is_none(), "{s}");
        }
    }
}
