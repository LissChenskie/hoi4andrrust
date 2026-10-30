//! Prepared national-focus graph shared by import, audit and the game UI.
//! No source parsing or country-specific code on the simulation tick.
use crate::{
    pdx::{self, Entry},
    research_script::ResearchScripts,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub fn references<'a>(raw: &'a [Entry], field: &str) -> impl Iterator<Item = &'a str> {
    let field = field.to_owned();
    raw.iter()
        .filter(move |e| e.key == field)
        .filter_map(Entry::block)
        .flatten()
        .filter(|e| e.key == "focus")
        .filter_map(Entry::atom)
}
/// Importing a shared root also attaches its shared descendants. Keep ordinary
/// focuses private to their declaring tree; never attach another country's nodes.
pub fn resolve_shared(s: &mut ResearchScripts) {
    let mut children: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for id in &s.shared_focuses {
        if let Some(src) = s.sources.get(&format!("focus:{id}")) {
            for parent in references(&src.raw, "prerequisite") {
                children
                    .entry(parent.into())
                    .or_default()
                    .insert(id.clone());
            }
        }
    }
    for tree in s.focus_trees.values_mut() {
        tree.problems.clear();
        let mut queue: VecDeque<_> = tree.shared.iter().cloned().collect();
        let mut seen = BTreeSet::new();
        while let Some(id) = queue.pop_front() {
            if !seen.insert(id.clone()) {
                continue;
            }
            if !s.shared_focuses.contains(&id) {
                tree.problems.push(format!("Missing shared_focus {id}"));
                continue;
            }
            tree.focuses.insert(id.clone());
            queue.extend(children.get(&id).into_iter().flatten().cloned());
        }
        tree.problems.extend(
            s.focus_problems
                .get(&format!("tree:{}", tree.id))
                .into_iter()
                .flatten()
                .cloned(),
        );
        for id in &tree.focuses {
            tree.problems.extend(
                s.focus_problems
                    .get(&format!("focus:{id}"))
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
            let Some(src) = s.sources.get(&format!("focus:{id}")) else {
                tree.problems.push(format!("Missing focus definition {id}"));
                continue;
            };
            for dep in references(&src.raw, "prerequisite")
                .chain(references(&src.raw, "mutually_exclusive"))
                .chain(pdx::atom(&src.raw, "relative_position_id"))
            {
                if !tree.focuses.contains(dep) {
                    tree.problems
                        .push(format!("{id}: reference outside tree: {dep}"));
                }
            }
        }
    }
}

/// Resolve relative coordinates once per graph refresh; conditional offsets use
/// the caller's real country context. Cycles/unknown conditions are diagnostics.
pub fn positions(
    tree: &crate::research_script::FocusTreeRecord,
    scripts: &ResearchScripts,
    mut condition: impl FnMut(&[Entry]) -> Result<bool, String>,
) -> Result<BTreeMap<String, (f32, f32)>, String> {
    fn one(
        id: &str,
        tree: &crate::research_script::FocusTreeRecord,
        s: &ResearchScripts,
        out: &mut BTreeMap<String, (f32, f32)>,
        stack: &mut BTreeSet<String>,
        condition: &mut impl FnMut(&[Entry]) -> Result<bool, String>,
    ) -> Result<(f32, f32), String> {
        if let Some(pos) = out.get(id) {
            return Ok(*pos);
        }
        if stack.len() >= 64 || !stack.insert(id.into()) {
            return Err(format!("Relative-position cycle/depth at {id}"));
        }
        let src = s
            .sources
            .get(&format!("focus:{id}"))
            .ok_or_else(|| format!("Missing focus {id}"))?;
        let num = |b: &[Entry], key| -> Result<f32, String> {
            match pdx::atom(b, key) {
                None => Ok(0.),
                Some(v) => v
                    .parse::<f32>()
                    .ok()
                    .filter(|v| v.is_finite() && v.abs() <= 100000.)
                    .ok_or_else(|| format!("{id}: nonliteral/out-of-range coordinate {key}")),
            }
        };
        let mut pos = (num(&src.raw, "x")?, num(&src.raw, "y")?);
        if let Some(parent) = pdx::atom(&src.raw, "relative_position_id") {
            if !tree.focuses.contains(parent) {
                return Err(format!("Missing relative focus {parent}"));
            }
            let base = one(parent, tree, s, out, stack, condition)?;
            pos.0 += base.0;
            pos.1 += base.1;
        }
        for e in src.raw.iter().filter(|e| e.key == "offset") {
            let b = e.block().ok_or("offset requires block")?;
            if condition(pdx::block(b, "trigger").unwrap_or(&[]))? {
                pos.0 += num(b, "x")?;
                pos.1 += num(b, "y")?;
            }
        }
        stack.remove(id);
        out.insert(id.into(), pos);
        Ok(pos)
    }
    let mut out = BTreeMap::new();
    for id in &tree.focuses {
        one(
            id,
            tree,
            scripts,
            &mut out,
            &mut BTreeSet::new(),
            &mut condition,
        )?;
    }
    Ok(out)
}
