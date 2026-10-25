//! Stable research geometry. Technology coordinates belong to individual GUI
//! grids, not to the whole folder. No graphics context is needed to build it.
use crate::pdx::{Entry, atom, block};
use crate::research::{ResearchCatalogue, ResearchKind, Technology};
use macroquad::prelude::{Vec2, vec2};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default)]
pub struct TreeLayout {
    pub positions: BTreeMap<String, Vec2>,
    pub horizontal: BTreeSet<String>,
}

#[derive(Clone, Copy)]
struct Grid {
    origin: Vec2,
    step: Vec2,
    horizontal: bool,
}

fn number(entries: &[Entry], key: &str) -> Option<f32> {
    atom(entries, key)?
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite())
}

fn collect_grids(entries: &[Entry], folder: &str, inside: bool, out: &mut BTreeMap<String, Grid>) {
    for e in entries {
        let Some(b) = e.block() else { continue };
        let inside = inside || atom(b, "name") == Some(folder);
        if inside && e.key.eq_ignore_ascii_case("gridboxtype") {
            if let (Some(name), Some(pos), Some(size)) =
                (atom(b, "name"), block(b, "position"), block(b, "slotsize"))
            {
                if let (Some(x), Some(y), Some(w), Some(h)) = (
                    number(pos, "x"),
                    number(pos, "y"),
                    number(size, "width"),
                    number(size, "height"),
                ) {
                    if w > 0.0 && h > 0.0 {
                        out.insert(
                            name.into(),
                            Grid {
                                origin: vec2(x, y),
                                step: vec2(w, h),
                                horizontal: atom(b, "format")
                                    .is_some_and(|s| s.eq_ignore_ascii_case("LEFT")),
                            },
                        );
                    }
                }
            }
        }
        collect_grids(b, folder, inside, out);
    }
}

fn position(tech: &Technology, folder: &str) -> Vec2 {
    tech.positions
        .iter()
        .find(|p| p.folder == folder)
        .map_or(vec2(tech.x, tech.y), |p| vec2(p.x, p.y))
}

pub fn folders(cat: &ResearchCatalogue) -> Vec<String> {
    let ordinary: BTreeSet<_> = cat
        .folder_nodes
        .iter()
        .filter(|(_, ids)| {
            ids.iter().any(|id| {
                cat.technologies
                    .get(id)
                    .is_some_and(|t| t.kind == ResearchKind::Standard)
            })
        })
        .map(|(f, _)| f.clone())
        .collect();
    let mut out: Vec<_> = cat
        .folder_order
        .iter()
        .filter(|f| ordinary.contains(*f))
        .cloned()
        .collect();
    for folder in ordinary {
        if !out.contains(&folder) {
            out.push(folder);
        }
    }
    out
}

pub fn build(cat: &ResearchCatalogue, folder: &str) -> TreeLayout {
    let nodes: BTreeMap<_, _> = cat
        .folder_nodes
        .get(folder)
        .into_iter()
        .flatten()
        .filter_map(|id| cat.technologies.get(id))
        .filter(|t| t.kind == ResearchKind::Standard)
        .map(|t| (t.id.clone(), t))
        .collect();
    let mut grids = BTreeMap::new();
    for source in cat
        .scripts
        .sources
        .values()
        .filter(|s| s.kind == "interface_geometry")
    {
        collect_grids(&source.raw, folder, false, &mut grids);
    }
    // Resolve each technology to its named root grid. Shared endpoints choose
    // the same root on every frame; ignored layout paths do not merge trees.
    fn root(
        id: &str,
        cat: &ResearchCatalogue,
        nodes: &BTreeMap<String, &Technology>,
        grids: &BTreeMap<String, Grid>,
        visiting: &mut BTreeSet<String>,
    ) -> Option<String> {
        if grids.contains_key(&format!("{id}_tree")) {
            return Some(id.into());
        }
        if !visiting.insert(id.into()) {
            return None;
        }
        let tech = nodes.get(id)?;
        for parent in tech.incoming_paths.iter().chain(&tech.prerequisites) {
            if !nodes.contains_key(parent) {
                continue;
            }
            let p = &cat.technologies[parent];
            if p.paths
                .iter()
                .any(|path| path.target == id && path.ignore_for_layout)
            {
                continue;
            }
            if let Some(root) = root(parent, cat, nodes, grids, visiting) {
                visiting.remove(id);
                return Some(root);
            }
        }
        visiting.remove(id);
        Some(id.into())
    }
    let mut groups: BTreeMap<String, Vec<&Technology>> = BTreeMap::new();
    for tech in nodes.values() {
        let root = root(&tech.id, cat, &nodes, &grids, &mut BTreeSet::new())
            .unwrap_or_else(|| tech.id.clone());
        groups.entry(root).or_default().push(tech);
    }
    let mut layout = TreeLayout::default();
    let mut fallback_y = grids.values().map(|g| g.origin.y).fold(0.0, f32::max) + 180.0;
    // The procedural cards are wider than the source icons. Scale GUI pixels
    // uniformly per axis so that neighboring source slots still fit cards.
    for (root, techs) in groups {
        let grid = grids
            .get(&format!("{root}_tree"))
            .copied()
            .unwrap_or_else(|| {
                let min = techs
                    .iter()
                    .map(|t| position(t, folder))
                    .fold(vec2(f32::INFINITY, f32::INFINITY), |a, b| a.min(b));
                let max = techs
                    .iter()
                    .map(|t| position(t, folder))
                    .fold(vec2(f32::NEG_INFINITY, f32::NEG_INFINITY), |a, b| a.max(b));
                let origin = vec2(-min.x * 70.0, fallback_y - min.y * 70.0);
                fallback_y += (max.y - min.y + 2.0) * 70.0;
                Grid {
                    origin,
                    step: vec2(70.0, 70.0),
                    horizontal: false,
                }
            });
        for tech in techs {
            let p = position(tech, folder);
            let offset = if grid.horizontal { vec2(p.y, p.x) } else { p };
            let p = (grid.origin + offset * grid.step) * vec2(2.4, 1.0);
            if !p.is_finite() {
                continue;
            }
            layout.positions.insert(tech.id.clone(), p);
            if grid.horizontal {
                layout.horizontal.insert(tech.id.clone());
            }
        }
    }
    let min = layout
        .positions
        .values()
        .copied()
        .fold(vec2(f32::INFINITY, f32::INFINITY), |a, b| a.min(b));
    for p in layout.positions.values_mut() {
        *p -= min;
    }
    layout
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research_data::parse_technology_file;
    use crate::research_script::ScriptSource;

    #[test]
    fn named_gui_grids_keep_separate_roots_and_honor_orientation() {
        let mut cat = ResearchCatalogue::default();
        parse_technology_file("technologies={a={folder={name=land position={x=0 y=0}} path={leads_to_tech=b}} b={folder={name=land position={x=0 y=2}}} c={folder={name=land position={x=0 y=0}}}}", &mut cat).unwrap();
        cat.scripts.sources.insert("gui".into(), ScriptSource {
            kind: "interface_geometry".into(), id: "gui".into(), source: "gui".into(), conditions: vec![], effects: vec![],
            raw: crate::pdx::parse_pdx("guiTypes={containerWindowType={name=land gridboxtype={name=a_tree position={x=100 y=200} slotsize={width=70 height=70} format=LEFT} gridboxtype={name=c_tree position={x=100 y=400} slotsize={width=70 height=70} format=UP}}}").unwrap(),
        });
        let layout = build(&cat, "land");
        assert_eq!(layout.positions["a"], Vec2::ZERO);
        assert_eq!(layout.positions["b"], vec2(336.0, 0.0));
        assert_eq!(layout.positions["c"], vec2(0.0, 200.0));
        assert!(layout.horizontal.contains("b"));
        assert!(!layout.horizontal.contains("c"));
    }

    #[test]
    fn ordinary_browser_excludes_doctrine_and_project_tabs_and_mixed_nodes() {
        let mut cat = ResearchCatalogue::default();
        parse_technology_file("technologies={a={folder={name=land}} doctrine={doctrine=yes folder={name=doctrines}} project={is_special_project_tech=yes folder={name=projects}} mixed={doctrine=yes folder={name=land}}}", &mut cat).unwrap();
        cat.folder_order = vec!["projects".into(), "land".into(), "doctrines".into()];
        assert_eq!(folders(&cat), vec!["land"]);
        assert_eq!(
            build(&cat, "land")
                .positions
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["a"]
        );
        let mut country = crate::research::CountryResearch::new(1, &Default::default());
        for id in ["doctrine", "project", "mixed"] {
            assert!(
                country
                    .start(
                        0,
                        id,
                        &cat,
                        &Default::default(),
                        crate::game::GameDate::START
                    )
                    .is_err()
            );
        }
    }
}
