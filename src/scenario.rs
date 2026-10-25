//! Headless import of campaign geography and starting state data.
//! Map rendering consumes the resulting World; it never imports/replaces it.
use crate::game::GameDate;
use crate::map_graph::MapGraph;
use crate::state::{self, StateMembership};
use crate::world::{Province, World};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug)]
pub struct LoadedScenario {
    pub world: World,
    pub diagnostics: Vec<String>,
}

pub fn load(root: &Path, names: &HashMap<u32, String>) -> Result<LoadedScenario, String> {
    load_at(root, names, GameDate::START)
}
pub fn load_at(
    root: &Path,
    names: &HashMap<u32, String>,
    start: GameDate,
) -> Result<LoadedScenario, String> {
    let map_directory = root.join("map");
    let bitmap = image::open(map_directory.join("provinces.bmp"))
        .map_err(|error| format!("Could not load province geography: {error}"))?
        .to_rgba8();
    let definition = fs::read_to_string(map_directory.join("definition.csv"))
        .map_err(|error| format!("Could not read province definitions: {error}"))?;
    let mut geography = MapGraph::from_rgba(
        bitmap.width(),
        bitmap.height(),
        bitmap.as_raw(),
        &definition,
        true,
    )?;
    drop(bitmap);
    let mut diagnostics = Vec::new();
    let adjacency_path = map_directory.join("adjacencies.csv");
    if adjacency_path.is_file() {
        let source = fs::read_to_string(&adjacency_path).map_err(|e| e.to_string())?;
        geography
            .import_adjacencies(&source)
            .map_err(|e| format!("adjacencies.csv: {e}"))?;
    } else {
        diagnostics.push("No adjacencies.csv: special/blocked crossings are unavailable".into());
    }
    let rule_path = map_directory.join("adjacency_rules.txt");
    if rule_path.is_file() {
        let source = fs::read_to_string(rule_path).map_err(|e| e.to_string())?;
        crate::research_data::parse_pdx(source.trim_start_matches('\u{feff}'))
            .map_err(|e| format!("adjacency_rules.txt: {e}"))?;
        geography.adjacency_rules_source = Some(source);
    }
    let river_path = map_directory.join("rivers.bmp");
    if river_path.is_file() {
        let bytes = fs::read(&river_path).map_err(|e| e.to_string())?;
        geography
            .import_rivers(&bytes, 11)
            .map_err(|e| format!("rivers.bmp: {e}"))?;
    } else {
        diagnostics.push("No rivers.bmp: river crossings remain Unknown".into());
    }
    // Named rules are retained on edges. Their diplomatic/naval conditions
    // are intentionally not interpreted until those game systems exist.
    if geography.provinces.keys().any(|&id| {
        geography
            .connections_from(id)
            .iter()
            .any(|e| e.rule.is_some())
    }) {
        diagnostics
            .push("Named strait rules require future diplomatic/naval permission checks".into());
    }
    let state_directory = root.join("history/states");
    let membership = match state::load_state_membership_at(
        &state_directory,
        &root.join("common/state_category"),
        start,
    ) {
        Ok(data) => data,
        Err(error) => {
            diagnostics.push(error);
            StateMembership::default()
        }
    };
    let date = (start.year, u32::from(start.month), u32::from(start.day));
    let mut owners = HashMap::new();
    let mut controllers = HashMap::new();
    if let Ok(entries) = fs::read_dir(&state_directory) {
        let mut paths: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "txt"))
            .collect();
        paths.sort();
        for path in paths {
            let source = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            owners.extend(state::parse_state_owners(&source, date)?);
            controllers.extend(state::parse_state_controllers(&source, date)?);
        }
    }
    for province in membership.state_by_province.keys() {
        if !geography.provinces.contains_key(province) {
            return Err(format!(
                "State references province {province} missing from definition.csv"
            ));
        }
    }
    let mut world = World::from_map_data(
        &membership.state_by_province,
        &owners,
        names,
        &membership.economy_by_state,
        &membership.shared_slots_by_state,
    );
    for &id in geography.provinces.keys() {
        world.provinces.entry(id).or_insert(Province {
            control_revision: 0,
            id,
            state_id: None,
            controller: None,
        });
    }
    let control_overrides: Vec<_> = world
        .provinces
        .iter()
        .filter_map(|(&id, province)| {
            province
                .state_id
                .and_then(|state| controllers.get(&state))
                .map(|tag| (id, tag.clone()))
        })
        .collect();
    for (id, tag) in control_overrides {
        world.set_province_controller(id, Some(&tag));
    }
    world.geography = geography;
    Ok(LoadedScenario { world, diagnostics })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_history_respects_bookmark_without_changing_owner() {
        let source = "state = { id = 1 provinces = { 11 12 } history = {
            owner = AAA controller = AAA
            1935.1.1 = { controller = BBB }
            1937.1.1 = { owner = CCC controller = CCC }
        }}";
        assert_eq!(
            state::parse_state_owners(source, (1936, 1, 1)).unwrap()[&1],
            "AAA"
        );
        assert_eq!(
            state::parse_state_controllers(source, (1936, 1, 1)).unwrap()[&1],
            "BBB"
        );
        let late_default =
            "state = { id = 1 history = { 1935.1.1 = { controller = BBB } controller = AAA } }";
        assert_eq!(
            state::parse_state_controllers(late_default, (1936, 1, 1)).unwrap()[&1],
            "BBB"
        );
    }

    #[test]
    fn real_file_import_builds_geography_economy_and_independent_control() {
        let root =
            std::env::temp_dir().join(format!("hoi4-scenario-import-{}", std::process::id()));
        fs::create_dir_all(root.join("map")).unwrap();
        fs::create_dir_all(root.join("history/states")).unwrap();
        let pixels = vec![1, 0, 0, 1, 0, 0, 2, 0, 0, 3, 0, 0, 4, 0, 0];
        image::RgbImage::from_raw(5, 1, pixels)
            .unwrap()
            .save(root.join("map/provinces.bmp"))
            .unwrap();
        fs::write(root.join("map/definition.csv"),
            "1;1;0;0;land;false;plains;1\n2;2;0;0;land;true;forest;1\n3;3;0;0;sea;true;ocean;1\n4;4;0;0;land;true;mountain;1").unwrap();
        fs::write(root.join("map/adjacencies.csv"),
            "1;2;impassable;-1;-1;-1;-1;-1;;\n1;4;sea;3;-1;-1;-1;-1;test_strait;\n2;4;;3;-1;-1;-1;-1;test_strait;default sea type\n-1;-1;-1;-1;-1;-1;-1;-1;-1").unwrap();
        fs::write(
            root.join("map/adjacency_rules.txt"),
            "adjacency_rules = { test_strait = { } }",
        )
        .unwrap();
        fs::write(
            root.join("history/states/one.txt"),
            "state = { id = 1 state_category = town provinces = { 1 2 4 } history = {
                owner = AAA controller = BBB buildings = { industrial_complex = 2 }
            }}",
        )
        .unwrap();
        let loaded = load(&root, &HashMap::new()).unwrap();
        let world = loaded.world;
        assert_eq!(world.provinces.len(), 4); // Includes the sea, absent from state membership.
        assert_eq!(world.owner_of_province(1), Some("AAA"));
        assert_eq!(world.controller_of_province(1), Some("BBB"));
        assert!(world.countries.contains_key("BBB"));
        assert_eq!(
            world.geography.connection(2, 4).unwrap().kind,
            crate::map_graph::CrossingKind::Strait
        );
        assert_eq!(world.country_economy("AAA").unwrap().civilian_factories, 2);
        assert_eq!(world.states[&1].shared_building_slots, 4);
        assert_eq!(
            world
                .geography
                .land_neighbors(1)
                .map(|e| e.to)
                .collect::<Vec<_>>(),
            [4]
        );
        assert!(
            world
                .geography
                .adjacency_rules_source
                .as_ref()
                .unwrap()
                .contains("test_strait")
        );
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|d| d.contains("river crossings remain Unknown"))
        );
        // An invalid override must not silently leave an unrestricted border.
        fs::write(
            root.join("map/adjacencies.csv"),
            "1;999;sea;-1;-1;-1;-1;-1;;",
        )
        .unwrap();
        assert!(
            load(&root, &HashMap::new())
                .unwrap_err()
                .contains("adjacencies.csv")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
