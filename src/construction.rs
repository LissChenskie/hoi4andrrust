//! Data-driven starter model for HOI4-style state construction.
//! Costs and speed constants are isolated here so they can later be loaded from
//! common/buildings and common/defines instead of being scattered through UI code.
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::Path;
use crate::world::World;

pub const BASE_CONSTRUCTION_POINTS_PER_CIVILIAN_FACTORY: f32 = 5.0;
pub const MAX_CIVILIAN_FACTORIES_PER_LINE: u32 = 15;
pub const MAX_SHARED_BUILDING_SLOTS: u16 = 30;
pub const BUILDING_DEFINITIONS_DIRECTORY: &str = "data/hoi4/common/buildings";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuildingType {
    CivilianFactory,
    MilitaryFactory,
    Dockyard,
    SyntheticRefinery,
    FuelSilo,
    RocketSite,
    NuclearReactor,
}

impl BuildingType {
    pub const ALL: [Self; 7] = [
        Self::CivilianFactory, Self::MilitaryFactory, Self::Dockyard,
        Self::SyntheticRefinery, Self::FuelSilo, Self::RocketSite, Self::NuclearReactor,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::CivilianFactory => "Civilian factory",
            Self::MilitaryFactory => "Military factory",
            Self::Dockyard => "Dockyard",
            Self::SyntheticRefinery => "Synthetic refinery",
            Self::FuelSilo => "Fuel silo",
            Self::RocketSite => "Rocket site",
            Self::NuclearReactor => "Nuclear reactor",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::CivilianFactory => "Civ",
            Self::MilitaryFactory => "Mil",
            Self::Dockyard => "Dock",
            Self::SyntheticRefinery => "Synth",
            Self::FuelSilo => "Fuel",
            Self::RocketSite => "Rocket",
            Self::NuclearReactor => "Nuke",
        }
    }

    pub fn glyph(self) -> &'static str {
        match self {
            Self::CivilianFactory => "c",
            Self::MilitaryFactory => "m",
            Self::Dockyard => "n",
            Self::SyntheticRefinery => "s",
            Self::FuelSilo => "f",
            Self::RocketSite => "r",
            Self::NuclearReactor => "a",
        }
    }

    pub fn default_base_cost(self) -> f32 {
        match self {
            Self::CivilianFactory | Self::MilitaryFactory => 10_800.0,
            Self::Dockyard => 6_400.0,
            Self::SyntheticRefinery => 14_000.0,
            Self::FuelSilo => 3_000.0,
            Self::RocketSite => 12_000.0,
            Self::NuclearReactor => 30_000.0,
        }
    }

    /// Base cost for the next level in the same state. Values are placeholders
    /// until the game's common/buildings definitions are parsed.
    pub fn default_per_level_extra_cost(self) -> f32 {
        0.0
    }

    pub fn built_count(self, economy: crate::state::StateEconomy) -> u16 {
        match self {
            Self::CivilianFactory => economy.civilian_factories,
            Self::MilitaryFactory => economy.military_factories,
            Self::Dockyard => economy.dockyards,
            Self::SyntheticRefinery => economy.synthetic_refineries,
            Self::FuelSilo => economy.fuel_silos,
            Self::RocketSite => economy.rocket_sites,
            Self::NuclearReactor => economy.nuclear_reactors,
        }
    }

    pub fn add_one(self, economy: &mut crate::state::StateEconomy) {
        match self {
            Self::CivilianFactory => economy.civilian_factories = economy.civilian_factories.saturating_add(1),
            Self::MilitaryFactory => economy.military_factories = economy.military_factories.saturating_add(1),
            Self::Dockyard => economy.dockyards = economy.dockyards.saturating_add(1),
            Self::SyntheticRefinery => economy.synthetic_refineries = economy.synthetic_refineries.saturating_add(1),
            Self::FuelSilo => economy.fuel_silos = economy.fuel_silos.saturating_add(1),
            Self::RocketSite => economy.rocket_sites = economy.rocket_sites.saturating_add(1),
            Self::NuclearReactor => economy.nuclear_reactors = economy.nuclear_reactors.saturating_add(1),
        }
    }
}


#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildingDefinition {
    pub base_cost: f32,
    pub per_level_extra_cost: f32,
    pub per_controlled_building_extra_cost: f32,
    pub infrastructure_construction_effect: bool,
}

#[derive(Debug, Clone)]
pub struct BuildingRules {
    definitions: HashMap<BuildingType, BuildingDefinition>,
}

impl Default for BuildingRules {
    fn default() -> Self {
        let mut definitions = HashMap::new();
        for building in BuildingType::ALL {
            definitions.insert(building, BuildingDefinition {
                base_cost: building.default_base_cost(),
                per_level_extra_cost: building.default_per_level_extra_cost(),
                per_controlled_building_extra_cost: 0.0,
                infrastructure_construction_effect: true,
            });
        }
        Self { definitions }
    }
}

impl BuildingRules {
    pub fn load(directory: impl AsRef<Path>) -> Self {
        let mut rules = Self::default();
        let Ok(entries) = fs::read_dir(directory) else { return rules; };
        let mut paths: Vec<_> = entries.flatten().map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "txt"))
            .collect();
        paths.sort();
        for path in paths {
            let Ok(text) = fs::read_to_string(path) else { continue; };
            for (building, key) in [
                (BuildingType::CivilianFactory, "industrial_complex"),
                (BuildingType::MilitaryFactory, "arms_factory"),
                (BuildingType::Dockyard, "dockyard"),
                (BuildingType::SyntheticRefinery, "synthetic_refinery"),
                (BuildingType::FuelSilo, "fuel_silo"),
                (BuildingType::RocketSite, "rocket_site"),
                (BuildingType::NuclearReactor, "nuclear_reactor"),
            ] {
                if let Some(block) = find_definition_block(&text, key) {
                    let current = rules.definitions.get(&building).copied().unwrap_or(BuildingDefinition {
                        base_cost: building.default_base_cost(),
                        per_level_extra_cost: 0.0,
                        per_controlled_building_extra_cost: 0.0,
                        infrastructure_construction_effect: true,
                    });
                    let definition = BuildingDefinition {
                        base_cost: top_level_number(block, "base_cost").unwrap_or(current.base_cost),
                        per_level_extra_cost: top_level_number(block, "per_level_extra_cost").unwrap_or(current.per_level_extra_cost),
                        per_controlled_building_extra_cost: top_level_number(block, "per_controlled_building_extra_cost").unwrap_or(current.per_controlled_building_extra_cost),
                        infrastructure_construction_effect: top_level_yes_no(block, "infrastructure_construction_effect").unwrap_or(current.infrastructure_construction_effect),
                    };
                    rules.definitions.insert(building, definition);
                }
            }
        }
        rules
    }

    pub fn definition(&self, building: BuildingType) -> BuildingDefinition {
        self.definitions.get(&building).copied().unwrap_or_else(|| BuildingRules::default().definitions[&building])
    }

    pub fn project(&self, building: BuildingType, level: u16) -> ConstructionProject {
        let definition = self.definition(building);
        ConstructionProject {
            building,
            progress: 0.0,
            cost: (definition.base_cost
                + f32::from(level) * definition.per_level_extra_cost
                + definition.per_controlled_building_extra_cost * 0.0).max(1.0),
        }
    }
}

fn find_definition_block<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let bytes = text.as_bytes();
    let mut offset = 0;
    while offset < bytes.len() {
        let relative = text[offset..].find(key)?;
        let start = offset + relative;
        let before_ok = start == 0 || !(bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_');
        let mut cursor = start + key.len();
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() { cursor += 1; }
        if !before_ok || cursor >= bytes.len() || bytes[cursor] != b'=' {
            offset = start + key.len();
            continue;
        }
        cursor += 1;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() { cursor += 1; }
        if cursor >= bytes.len() || bytes[cursor] != b'{' {
            offset = start + key.len();
            continue;
        }
        let block_start = cursor + 1;
        let mut depth = 1i32;
        let mut in_quote = false;
        let mut in_comment = false;
        cursor += 1;
        while cursor < bytes.len() {
            let b = bytes[cursor];
            if in_comment {
                if b == b'\n' { in_comment = false; }
            } else if in_quote {
                if b == b'"' && (cursor == 0 || bytes[cursor - 1] != b'\\') { in_quote = false; }
            } else if b == b'#' {
                in_comment = true;
            } else if b == b'"' {
                in_quote = true;
            } else if b == b'{' {
                depth += 1;
            } else if b == b'}' {
                depth -= 1;
                if depth == 0 { return text.get(block_start..cursor); }
            }
            cursor += 1;
        }
        return None;
    }
    None
}

fn top_level_number(block: &str, field: &str) -> Option<f32> {
    let mut depth = 1i32;
    for raw_line in block.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if depth == 1 {
            if let Some((name, value)) = line.split_once('=') {
                if name.trim() == field {
                    let token = value.split_whitespace().next()?.trim_matches('"');
                    if let Ok(number) = token.parse::<f32>() { return Some(number); }
                }
            }
        }
        let clean = raw_line.split('#').next().unwrap_or("");
        depth += clean.matches('{').count() as i32;
        depth -= clean.matches('}').count() as i32;
    }
    None
}

fn top_level_yes_no(block: &str, field: &str) -> Option<bool> {
    let mut depth = 1i32;
    for raw_line in block.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if depth == 1 {
            if let Some((name, value)) = line.split_once('=') {
                if name.trim() == field {
                    return match value.trim() { "yes" => Some(true), "no" => Some(false), _ => None };
                }
            }
        }
        let clean = raw_line.split('#').next().unwrap_or("");
        depth += clean.matches('{').count() as i32;
        depth -= clean.matches('}').count() as i32;
    }
    None
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstructionProject {
    pub building: BuildingType,
    pub progress: f32,
    pub cost: f32,
}

impl ConstructionProject {
    pub fn new(building: BuildingType, state_level: u16) -> Self {
        BuildingRules::default().project(building, state_level)
    }

    pub fn progress_fraction(&self) -> f32 {
        if self.cost <= 0.0 { 1.0 } else { (self.progress / self.cost).clamp(0.0, 1.0) }
    }
}

pub type ConstructionQueues = HashMap<u32, VecDeque<ConstructionProject>>;

/// Advance one in-game day. All owned-state civilian factories form a shared
/// pool; up to 15 factories are assigned to each active project in state-ID order.
/// Consumer-goods laws, national modifiers and scripted effects are not yet modeled.
pub fn advance_day(world: &mut World, country_tag: &str, queues: &mut ConstructionQueues, rules: &BuildingRules) {
    let mut state_ids: Vec<u32> = match world.countries.get(country_tag) {
        Some(country) => country.state_ids.iter().copied().collect(),
        None => return,
    };
    state_ids.sort_unstable();

    let mut available_factories: u32 = state_ids.iter()
        .filter_map(|id| world.states.get(id))
        .map(|state| u32::from(state.economy.civilian_factories))
        .sum();

    if available_factories == 0 { return; }

    for state_id in state_ids {
        if available_factories == 0 { break; }
        let project = match queues.get_mut(&state_id).and_then(|q| q.front_mut()) {
            Some(project) => project,
            None => continue,
        };
        let state = match world.states.get(&state_id) {
            Some(state) => state,
            None => continue,
        };
        let assigned = available_factories.min(MAX_CIVILIAN_FACTORIES_PER_LINE);
        available_factories -= assigned;
        let infrastructure_bonus = if rules.definition(project.building).infrastructure_construction_effect {
            (1.0 + f32::from(state.economy.infrastructure) * 0.1).min(1.5)
        } else { 1.0 };
        project.progress += assigned as f32 * BASE_CONSTRUCTION_POINTS_PER_CIVILIAN_FACTORY * infrastructure_bonus;

        if project.progress + f32::EPSILON >= project.cost {
            let building = project.building;
            if let Some(state) = world.states.get_mut(&state_id) {
                building.add_one(&mut state.economy);
            }
            if let Some(queue) = queues.get_mut(&state_id) {
                queue.pop_front();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::StateEconomy;
    use crate::world::{Country, Province, State, World};
    use std::collections::{BTreeSet, BTreeMap};

    fn test_world() -> World {
        let mut countries = BTreeMap::new();
        countries.insert("TST".to_owned(), Country { tag: "TST".to_owned(), state_ids: BTreeSet::from([1]) });
        let mut states = BTreeMap::new();
        states.insert(1, State { id: 1, name: None, owner: Some("TST".to_owned()), province_ids: vec![],
            shared_building_slots: 5, economy: StateEconomy { civilian_factories: 2, infrastructure: 5, ..StateEconomy::default() } });
        World { countries, states, provinces: BTreeMap::<u32, Province>::new() }
    }

    #[test]
    fn project_cost_and_progress_are_explicit() {
        let project = ConstructionProject::new(BuildingType::MilitaryFactory, 2);
        assert_eq!(project.cost, 10_800.0);
        assert_eq!(project.progress_fraction(), 0.0);
    }

    #[test]
    fn daily_tick_uses_civilian_factories_and_infrastructure() {
        let mut world = test_world();
        let mut queues = ConstructionQueues::new();
        queues.insert(1, VecDeque::from([ConstructionProject { building: BuildingType::CivilianFactory, progress: 10_790.0, cost: 10_800.0 }]));
        advance_day(&mut world, "TST", &mut queues, &BuildingRules::default());
        assert_eq!(world.states[&1].economy.civilian_factories, 3);
        assert!(queues.get(&1).is_none_or(VecDeque::is_empty));
    }

    #[test]
    fn country_without_civilian_factories_does_not_progress() {
        let mut world = test_world();
        world.states.get_mut(&1).unwrap().economy.civilian_factories = 0;
        let mut queues = ConstructionQueues::new();
        queues.insert(1, VecDeque::from([ConstructionProject::new(BuildingType::Dockyard, 0)]));
        advance_day(&mut world, "TST", &mut queues, &BuildingRules::default());
        assert_eq!(queues[&1][0].progress, 0.0);
    }
}
