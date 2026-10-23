//! Data-driven starter model for HOI4-style state construction.
//! Construction costs are loaded from common/buildings and the base factory speed
//! from common/defines when the local HOI4 data directory is available.
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::Path;
use crate::world::World;

pub const BASE_CONSTRUCTION_POINTS_PER_CIVILIAN_FACTORY: f32 = 5.0;
pub const MAX_CIVILIAN_FACTORIES_PER_LINE: u32 = 15;
pub const MAX_CONSTRUCTION_QUEUE_ITEMS: usize = 1000;
pub const MAX_SHARED_BUILDING_SLOTS: u16 = 25;
pub const BUILDING_DEFINITIONS_DIRECTORY: &str = "data/hoi4/common/buildings";
pub const DEFINES_DIRECTORY: &str = "data/hoi4/common/defines";
pub const DEFAULT_INFRA_MAX_CONSTRUCTION_COST_EFFECT: f32 = 1.0;
pub const MAX_INFRASTRUCTURE_LEVEL: u16 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuildingType {
    Infrastructure,
    CivilianFactory,
    MilitaryFactory,
    Dockyard,
    SyntheticRefinery,
    FuelSilo,
    RocketSite,
    NuclearReactor,
}

impl BuildingType {
    pub const ALL: [Self; 8] = [
        Self::Infrastructure,
        Self::CivilianFactory, Self::MilitaryFactory, Self::Dockyard,
        Self::SyntheticRefinery, Self::FuelSilo, Self::RocketSite, Self::NuclearReactor,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Infrastructure => "Infrastructure",
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
            Self::Infrastructure => "Infra",
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
            Self::Infrastructure => "i",
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
            Self::Infrastructure => 6_000.0,
            Self::CivilianFactory => 10_800.0,
            Self::MilitaryFactory => 7_200.0,
            Self::Dockyard => 6_400.0,
            Self::SyntheticRefinery => 14_500.0,
            Self::FuelSilo => 5_000.0,
            Self::RocketSite => 6_400.0,
            Self::NuclearReactor => 30_000.0,
        }
    }

    /// Base cost for the next level in the same state. Values are placeholders
    /// until the game's common/buildings definitions are parsed.
    pub fn default_per_level_extra_cost(self) -> f32 {
        0.0
    }

    pub fn shares_slots(self) -> bool { self != Self::Infrastructure }

    pub fn default_max_level(self) -> u16 {
        match self {
            Self::Infrastructure => MAX_INFRASTRUCTURE_LEVEL,
            Self::CivilianFactory | Self::MilitaryFactory | Self::Dockyard => 20,
            Self::SyntheticRefinery | Self::RocketSite => 3,
            Self::FuelSilo => 15,
            Self::NuclearReactor => 1,
        }
    }

    pub fn built_count(self, economy: crate::state::StateEconomy) -> u16 {
        match self {
            Self::Infrastructure => economy.infrastructure,
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
            Self::Infrastructure => economy.infrastructure = economy.infrastructure.saturating_add(1),
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
    state_level_caps: HashMap<BuildingType, u16>,
    shares_slots_overrides: HashMap<BuildingType, bool>,
    pub base_factory_speed: f32,
    pub infra_max_construction_cost_effect: f32,
    pub max_civilian_factories_per_line: u32,
    pub max_shared_slots: u16,
}

impl Default for BuildingRules {
    fn default() -> Self {
        let mut definitions = HashMap::new();
        let mut state_level_caps = HashMap::new();
        let shares_slots_overrides = HashMap::new();
        for building in BuildingType::ALL {
            state_level_caps.insert(building, building.default_max_level());
            definitions.insert(building, BuildingDefinition {
                base_cost: building.default_base_cost(),
                per_level_extra_cost: building.default_per_level_extra_cost(),
                per_controlled_building_extra_cost: 0.0,
                infrastructure_construction_effect: true,
            });
        }
        Self { definitions, state_level_caps, shares_slots_overrides, base_factory_speed: BASE_CONSTRUCTION_POINTS_PER_CIVILIAN_FACTORY, infra_max_construction_cost_effect: DEFAULT_INFRA_MAX_CONSTRUCTION_COST_EFFECT, max_civilian_factories_per_line: MAX_CIVILIAN_FACTORIES_PER_LINE, max_shared_slots: MAX_SHARED_BUILDING_SLOTS }
    }
}

impl BuildingRules {
    pub fn load(directory: impl AsRef<Path>) -> Self {
        let mut rules = Self::default();
        // Missing local building files must NOT skip loading the defines.
        let mut paths: Vec<_> = fs::read_dir(directory).into_iter().flatten()
            .flatten().map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "txt"))
            .collect();
        paths.sort();
        for path in paths {
            let Ok(text) = fs::read_to_string(path) else { continue; };
            for (building, key) in [
                (BuildingType::Infrastructure, "infrastructure"),
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
                    if let Some(cap_block) = find_definition_block(block, "level_cap") {
                        if let Some(cap) = top_level_number(cap_block, "state_max") {
                            if cap.is_finite() && (1.0..=u16::MAX as f32).contains(&cap) {
                                rules.state_level_caps.insert(building, cap as u16);
                            }
                        }
                        if let Some(shares) = top_level_yes_no(cap_block, "shares_slots") {
                            rules.shares_slots_overrides.insert(building, shares);
                        }
                    }
                }
            }
        }
        // Read baseline daily output from HOI4's Lua defines when available.
        // Missing game files are expected in CI and fall back to the vanilla default.
        if let Ok(entries) = fs::read_dir(DEFINES_DIRECTORY) {
            let mut define_paths: Vec<_> = entries.flatten().map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "lua"))
                .collect();
            define_paths.sort();
            for path in define_paths {
                if let Ok(text) = fs::read_to_string(path) {
                    if let Some(value) = find_lua_number(&text, "BASE_FACTORY_SPEED") {
                        if value.is_finite() && value > 0.0 {
                            rules.base_factory_speed = value;
                        }
                    }
                    if let Some(value) = find_lua_number(&text, "INFRA_MAX_CONSTRUCTION_COST_EFFECT") {
                        if value.is_finite() && value >= 0.0 {
                            rules.infra_max_construction_cost_effect = value;
                        }
                    }
                    if let Some(value) = find_lua_number(&text, "MAX_CIV_FACTORIES_PER_LINE") {
                        if value.is_finite() && value >= 1.0 {
                            rules.max_civilian_factories_per_line = value as u32;
                        }
                    }
                    if let Some(value) = find_lua_number(&text, "MAX_SHARED_SLOTS") {
                        if value.is_finite() && value >= 1.0 {
                            rules.max_shared_slots = (value as u16).max(1);
                        }
                    }
                }
            }
        }
        rules
    }

    pub fn definition(&self, building: BuildingType) -> BuildingDefinition {
        self.definitions.get(&building).copied().unwrap_or_else(|| BuildingRules::default().definitions[&building])
    }

    pub fn max_state_level(&self, building: BuildingType) -> u16 {
        self.state_level_caps.get(&building).copied().unwrap_or(building.default_max_level())
    }

    /// A building's level_cap.shares_slots flag overrides the fallback classification.
    pub fn shares_slots(&self, building: BuildingType) -> bool {
        self.shares_slots_overrides.get(&building).copied().unwrap_or_else(|| building.shares_slots())
    }

    /// Cost is fixed IC. Infrastructure changes DAILY IC throughput, not this price.
    /// The unused infrastructure parameter preserves the old project constructor API.
    pub fn project(&self, building: BuildingType, level: u16, controlled_building_count: u32, _infrastructure: u16) -> ConstructionProject {
        let definition = self.definition(building);
        let raw_cost = definition.base_cost
            + f32::from(level) * definition.per_level_extra_cost
            + definition.per_controlled_building_extra_cost * controlled_building_count as f32;
        ConstructionProject {
            building,
            progress: 0.0,
            cost: raw_cost.max(1.0),
        }
    }

    pub fn infrastructure_speed_multiplier(&self, infrastructure: u16, building: BuildingType) -> f32 {
        if !self.definition(building).infrastructure_construction_effect { return 1.0; }
        1.0 + f32::from(infrastructure.min(MAX_INFRASTRUCTURE_LEVEL))
            * self.infra_max_construction_cost_effect / f32::from(MAX_INFRASTRUCTURE_LEVEL)
    }
}

fn find_lua_number(text: &str, field: &str) -> Option<f32> {
    let mut last = None;
    for raw_line in text.lines() {
        let line = raw_line.split("--").next().unwrap_or("")
            .split('#').next().unwrap_or("").trim();
        let Some((name, value)) = line.split_once('=') else { continue; };
        // Both BASE_FACTORY_SPEED and NDefines.NBuildings.BASE_FACTORY_SPEED
        // are used by HOI4 define files/overrides.
        if name.trim().rsplit('.').next() != Some(field) { continue; }
        let Some(token) = value.split_whitespace().next() else { continue; };
        let token = token.trim_end_matches(',');
        if let Ok(number) = token.parse::<f32>() { last = Some(number); }
    }
    last
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

/// Read scalar fields only at the root of a building block. Handles compact
/// Clausewitz lines (e.g. state_max=5 shares_slots=yes) and nested blocks.
fn top_level_scalar<'a>(block: &'a str, field: &str) -> Option<&'a str> {
    let bytes = block.as_bytes();
    let mut depth = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'#' => {
                while i < bytes.len() && bytes[i] != b'\n' { i += 1; }
            }
            b'"' => {
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' { i = (i + 2).min(bytes.len()); continue; }
                    if bytes[i] == b'"' { i += 1; break; }
                    i += 1;
                }
            }
            b'{' => { depth += 1; i += 1; }
            b'}' => { depth = depth.saturating_sub(1); i += 1; }
            b'=' | b' ' | b'\n' | b'\r' | b'\t' => i += 1,
            _ => {
                let key_start = i;
                while i < bytes.len() && !matches!(bytes[i], b'{' | b'}' | b'=' | b'#' | b'"' | b' ' | b'\n' | b'\r' | b'\t') {
                    i += 1;
                }
                if depth != 0 || block.get(key_start..i) != Some(field) { continue; }
                let mut cursor = i;
                while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() { cursor += 1; }
                if cursor >= bytes.len() || bytes[cursor] != b'=' { continue; }
                cursor += 1;
                while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() { cursor += 1; }
                if cursor == bytes.len() { return None; }
                let quoted = bytes[cursor] == b'"';
                if quoted { cursor += 1; }
                let start = cursor;
                while cursor < bytes.len() && !matches!(bytes[cursor], b'{' | b'}' | b'#' | b' ' | b'\n' | b'\r' | b'\t' | b'"') {
                    cursor += 1;
                }
                let value = block.get(start..cursor)?;
                return if quoted && bytes.get(cursor) != Some(&b'"') { None } else { Some(value) };
            }
        }
    }
    None
}

fn top_level_number(block: &str, field: &str) -> Option<f32> {
    top_level_scalar(block, field)?.trim_end_matches(',').parse().ok()
}

fn top_level_yes_no(block: &str, field: &str) -> Option<bool> {
    match top_level_scalar(block, field)? {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstructionProject {
    pub building: BuildingType,
    pub progress: f32,
    pub cost: f32,
}

impl ConstructionProject {
    pub fn new(building: BuildingType, state_level: u16) -> Self {
        BuildingRules::default().project(building, state_level, 0, 0)
    }

    pub fn progress_fraction(&self) -> f32 {
        if self.cost <= 0.0 { 1.0 } else { (self.progress / self.cost).clamp(0.0, 1.0) }
    }
}

/// Parameters for independently stacking national, per-building and state speed
/// modifiers. Additive bonuses are combined BEFORE the infrastructure multiplier.
/// These are CONSTRUCTION bonuses, not military equipment production efficiency.
#[derive(Debug, Clone, Default)]
pub struct ConstructionModifiers {
    pub national_bonus: f32,
    pub building_bonuses: HashMap<BuildingType, f32>,
    pub state_bonuses: HashMap<u32, f32>,
}

impl ConstructionModifiers {
    pub fn speed_factor(&self, state_id: u32, building: BuildingType) -> f32 {
        let bonus = self.national_bonus
            + self.building_bonuses.get(&building).copied().unwrap_or(0.0)
            + self.state_bonuses.get(&state_id).copied().unwrap_or(0.0);
        if !bonus.is_finite() { return 1.0; }
        (1.0 + bonus).max(0.0)
    }
}

/// Two presets explicitly documented in the user's research. Unset keeps the
/// preexisting no-law fallback until real national laws are imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EconomicLawPreset {
    #[default]
    Unset,
    CivilianEconomy,
    PartialMobilization,
}

impl EconomicLawPreset {
    pub fn next(self) -> Self {
        match self {
            Self::Unset => Self::CivilianEconomy,
            Self::CivilianEconomy => Self::PartialMobilization,
            Self::PartialMobilization => Self::Unset,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Unset => "Law: not imported",
            Self::CivilianEconomy => "Civilian economy (35%)",
            Self::PartialMobilization => "Partial mobilisation (20%)",
        }
    }

    pub fn consumer_goods_fraction(self) -> f32 {
        match self {
            Self::Unset => 0.0,
            Self::CivilianEconomy => 0.35,
            Self::PartialMobilization => 0.20,
        }
    }
}

pub type ConstructionQueues = HashMap<u32, VecDeque<ConstructionProject>>;

/// Country-specific economy inputs. Zeroed by default until economic laws/trade
/// are loaded; we must not invent starting laws for imported countries.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CountryConstructionPolicy {
    pub consumer_goods_fraction: f32,
    pub trade_civilian_factories: u32,
    pub construction_speed_bonus: f32,
}

impl Default for CountryConstructionPolicy {
    fn default() -> Self {
        Self { consumer_goods_fraction: 0.0, trade_civilian_factories: 0, construction_speed_bonus: 0.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConstructionCapacity {
    pub total_civilian: u32,
    pub consumer_goods: u32,
    pub trade: u32,
    pub available: u32,
}

pub fn country_capacity(world: &World, tag: &str, policy: CountryConstructionPolicy) -> Option<ConstructionCapacity> {
    let economy = world.country_economy(tag)?;
    let ratio = if policy.consumer_goods_fraction.is_finite() {
        policy.consumer_goods_fraction.clamp(0.0, 1.0)
    } else { 0.0 };
    let consumer_goods = ((economy.civilian_factories + economy.military_factories) as f32 * ratio)
        .ceil() as u32;
    let available = economy.civilian_factories
        .saturating_sub(consumer_goods).saturating_sub(policy.trade_civilian_factories);
    Some(ConstructionCapacity {
        total_civilian: economy.civilian_factories,
        consumer_goods: consumer_goods.min(economy.civilian_factories),
        trade: policy.trade_civilian_factories.min(economy.civilian_factories.saturating_sub(consumer_goods)),
        available,
    })
}

pub fn queued_shared_slots(queue: &VecDeque<ConstructionProject>) -> usize {
    queue.iter().filter(|p| p.building.shares_slots()).count()
}

pub fn queued_shared_slots_with_rules(queue: &VecDeque<ConstructionProject>, rules: &BuildingRules) -> usize {
    queue.iter().filter(|p| rules.shares_slots(p.building)).count()
}

pub fn shared_slot_limit(world: &World, state_id: u32, rules: &BuildingRules) -> Option<usize> {
    let state = world.states.get(&state_id)?;
    let occupied = usize::from(state.economy.occupied_shared_slots());
    Some(usize::from(state.shared_building_slots).max(occupied).min(usize::from(rules.max_shared_slots)))
}

/// Validate without mutating the queue on failure, including non-shared level caps.
pub fn enqueue_project(
    world: &World, country_tag: &str, state_id: u32, queues: &mut ConstructionQueues,
    rules: &BuildingRules, building: BuildingType,
) -> Result<(), &'static str> {
    let state = world.states.get(&state_id).ok_or("Unknown state")?;
    if state.owner.as_deref() != Some(country_tag) { return Err("State is not controlled by this country"); }
    let national_queued: usize = queues.iter()
        .filter(|(id, _)| world.states.get(id).is_some_and(|state| state.owner.as_deref() == Some(country_tag)))
        .map(|(_, queue)| queue.len()).sum();
    if national_queued >= MAX_CONSTRUCTION_QUEUE_ITEMS { return Err("Country construction queue is full"); }
    let existing = queues.get(&state_id);
    let queued_type = existing.map_or(0usize, |queue| queue.iter().filter(|p| p.building == building).count());
    let current_level = usize::from(building.built_count(state.economy));
    if current_level + queued_type >= usize::from(rules.max_state_level(building)) {
        return Err("Maximum building level reached");
    }
    if rules.shares_slots(building) {
        let occupied = usize::from(state.economy.occupied_shared_slots());
        let reserved = existing.map_or(0, |queue| queued_shared_slots_with_rules(queue, rules));
        if occupied + reserved >= shared_slot_limit(world, state_id, rules).unwrap_or(0) {
            return Err("No shared building slots available");
        }
    }
    let controlled_count = world.countries.get(country_tag).map_or(0u32, |country| {
        country.state_ids.iter().filter_map(|id| world.states.get(id))
            .map(|s| u32::from(building.built_count(s.economy))).sum()
    });
    let level = (current_level + queued_type).min(u16::MAX as usize) as u16;
    let project = rules.project(building, level, controlled_count, state.economy.infrastructure);
    queues.entry(state_id).or_default().push_back(project);
    Ok(())
}

/// Remove a queued project, reclaiming its reserved shared slot immediately.
pub fn cancel_project(queues: &mut ConstructionQueues, state_id: u32, index: usize) -> bool {
    let Some(queue) = queues.get_mut(&state_id) else { return false; };
    let removed = queue.remove(index).is_some();
    if queue.is_empty() { queues.remove(&state_id); }
    removed
}

/// One independently scheduled project. Up to 15 CIV are assigned per line.
/// Two queued projects in the same state can use separate lines concurrently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineAllocation {
    pub state_id: u32,
    pub project_index: usize,
    pub civilian_factories: u32,
}

/// An ordered state list is kept for the existing UI. All projects in a state
/// retain insertion order, followed by unlisted states in ascending ID order.
pub fn allocate_factories(
    world: &World, country_tag: &str, queues: &ConstructionQueues,
    priority: &[u32], rules: &BuildingRules, policy: CountryConstructionPolicy,
) -> Vec<LineAllocation> {
    let Some(capacity) = country_capacity(world, country_tag, policy) else { return Vec::new(); };
    let mut available = capacity.available;
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    let mut ordered = Vec::new();
    for id in priority {
        if seen.insert(*id) { ordered.push(*id); }
    }
    let mut other: Vec<u32> = queues.keys().copied().filter(|id| seen.insert(*id)).collect();
    other.sort_unstable();
    ordered.extend(other);
    for state_id in ordered {
        if available == 0 { break; }
        let Some(state) = world.states.get(&state_id) else { continue };
        if state.owner.as_deref() != Some(country_tag) { continue; }
        let Some(queue) = queues.get(&state_id) else { continue; };
        for (project_index, project) in queue.iter().enumerate() {
            if available == 0 { break; }
            if !project.cost.is_finite() || !project.progress.is_finite() { continue; }
            if project.progress >= project.cost { continue; }
            let assigned = available.min(rules.max_civilian_factories_per_line);
            if assigned == 0 { break; }
            available -= assigned;
            result.push(LineAllocation { state_id, project_index, civilian_factories: assigned });
        }
    }
    result
}

pub fn daily_construction_points(
    rules: &BuildingRules, policy: CountryConstructionPolicy,
    modifiers: &ConstructionModifiers, state_id: u32, building: BuildingType,
    infrastructure: u16, assigned_civilian: u32,
) -> f32 {
    let policy_bonus = if policy.construction_speed_bonus.is_finite() { policy.construction_speed_bonus } else { 0.0 };
    let bonuses = (modifiers.speed_factor(state_id, building) + policy_bonus).max(0.0);
    let infrastructure_factor = rules.infrastructure_speed_multiplier(infrastructure, building);
    let speed = assigned_civilian as f32 * rules.base_factory_speed * bonuses * infrastructure_factor;
    if speed.is_finite() { speed.max(0.0) } else { 0.0 }
}

pub fn estimated_days_remaining(project: &ConstructionProject, daily_points: f32) -> Option<u32> {
    if !project.progress.is_finite() || !project.cost.is_finite() { return None; }
    if project.progress >= project.cost { return Some(0); }
    if !daily_points.is_finite() || daily_points <= 0.0 { return None; }
    let days = ((project.cost - project.progress) / daily_points).ceil();
    if days > u32::MAX as f32 { None } else { Some(days as u32) }
}

/// Fully configurable daily simulation. Compute every line's output against the
/// state of the world at day start, then commit all completed buildings together.
/// This avoids a newly completed infrastructure level affecting a different line
/// on the SAME day, or a freshly completed civilian factory working that day.
pub fn advance_day_with_modifiers(
    world: &mut World, country_tag: &str, queues: &mut ConstructionQueues,
    priority: &[u32], rules: &BuildingRules, policy: CountryConstructionPolicy,
    modifiers: &ConstructionModifiers,
) {
    let allocations = allocate_factories(world, country_tag, queues, priority, rules, policy);
    let mut completed: Vec<(u32, usize, BuildingType)> = Vec::new();
    for line in allocations {
        let Some(state) = world.states.get(&line.state_id) else { continue };
        let infrastructure = state.economy.infrastructure;
        let Some(project) = queues.get_mut(&line.state_id)
            .and_then(|queue| queue.get_mut(line.project_index)) else { continue; };
        let speed = daily_construction_points(
            rules, policy, modifiers, line.state_id, project.building,
            infrastructure, line.civilian_factories,
        );
        project.progress += speed;
        if project.progress + f32::EPSILON >= project.cost {
            completed.push((line.state_id, line.project_index, project.building));
        }
    }
    // Indices are removed in descending order, or the next removal would shift.
    completed.sort_unstable_by(|a, b| (b.0, b.1).cmp(&(a.0, a.1)));
    for (state_id, index, building) in completed {
        if let Some(state) = world.states.get_mut(&state_id) {
            building.add_one(&mut state.economy);
        }
        cancel_project(queues, state_id, index);
    }
}

pub fn advance_day_ordered(
    world: &mut World, country_tag: &str, queues: &mut ConstructionQueues,
    priority: &[u32], rules: &BuildingRules, policy: CountryConstructionPolicy,
) {
    advance_day_with_modifiers(
        world, country_tag, queues, priority, rules, policy, &ConstructionModifiers::default(),
    );
}

/// Legacy entry point for a default economy (laws are not inferred from a tag).
pub fn advance_day(world: &mut World, country_tag: &str, queues: &mut ConstructionQueues, rules: &BuildingRules) {
    advance_day_ordered(world, country_tag, queues, &[], rules, CountryConstructionPolicy::default());
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
    fn parser_reads_top_level_building_cost_fields() {
        let source = r#"
            industrial_complex = {
                base_cost = 12345
                per_level_extra_cost = 250
                per_controlled_building_extra_cost = 75
                infrastructure_construction_effect = yes
                icon = {
                    x = 1
                }
            }
        "#;
        let block = find_definition_block(source, "industrial_complex").unwrap();
        assert_eq!(top_level_number(block, "base_cost"), Some(12345.0));
        assert_eq!(top_level_number(block, "per_level_extra_cost"), Some(250.0));
        assert_eq!(top_level_number(block, "per_controlled_building_extra_cost"), Some(75.0));
        assert_eq!(top_level_yes_no(block, "infrastructure_construction_effect"), Some(true));
        assert_eq!(top_level_number(block, "x"), None);
    }

    #[test]
    fn defines_parser_reads_base_factory_speed() {
        let source = "NBuildings = {\n BASE_FACTORY_SPEED = 5.0\n INFRA_MAX_CONSTRUCTION_COST_EFFECT = 1.0\n}";
        assert_eq!(find_lua_number(source, "BASE_FACTORY_SPEED"), Some(5.0));
        assert_eq!(find_lua_number(source, "INFRA_MAX_CONSTRUCTION_COST_EFFECT"), Some(1.0));
    }

    #[test]
    fn project_cost_includes_controlled_building_surcharge() {
        let mut rules = BuildingRules::default();
        rules.definitions.insert(BuildingType::Dockyard, BuildingDefinition {
            base_cost: 100.0,
            per_level_extra_cost: 20.0,
            per_controlled_building_extra_cost: 5.0,
            infrastructure_construction_effect: false,
        });
        assert_eq!(rules.project(BuildingType::Dockyard, 2, 3, 0).cost, 155.0);
    }

    #[test]
    fn project_cost_and_progress_are_explicit() {
        let project = ConstructionProject::new(BuildingType::MilitaryFactory, 2);
        assert_eq!(project.cost, 7_200.0);
        assert_eq!(project.progress_fraction(), 0.0);
    }

    #[test]
    fn infrastructure_boosts_daily_output_without_changing_cost() {
        let rules = BuildingRules::default();
        let no_infra = rules.project(BuildingType::CivilianFactory, 0, 0, 0);
        let max_infra = rules.project(BuildingType::CivilianFactory, 0, 0, 5);
        assert_eq!(no_infra.cost, 10_800.0);
        assert_eq!(max_infra.cost, 10_800.0);

        let mut world = test_world();
        let mut queues = ConstructionQueues::new();
        queues.insert(1, VecDeque::from([ConstructionProject { building: BuildingType::CivilianFactory, progress: 0.0, cost: 1_000.0 }]));
        advance_day(&mut world, "TST", &mut queues, &rules);
        assert_eq!(queues[&1][0].progress, 20.0);
    }

    #[test]
    fn national_factory_pool_is_shared_between_active_state_projects() {
        let mut world = test_world();
        world.countries.get_mut("TST").unwrap().state_ids.insert(2);
        world.states.insert(2, State {
            id: 2, name: None, owner: Some("TST".to_owned()), province_ids: vec![],
            shared_building_slots: 5,
            economy: StateEconomy { infrastructure: 0, ..StateEconomy::default() },
        });
        let mut queues = ConstructionQueues::new();
        queues.insert(1, VecDeque::from([ConstructionProject {
            building: BuildingType::MilitaryFactory, progress: 0.0, cost: 1_000.0,
        }]));
        queues.insert(2, VecDeque::from([ConstructionProject {
            building: BuildingType::Dockyard, progress: 0.0, cost: 1_000.0,
        }]));

        advance_day(&mut world, "TST", &mut queues, &BuildingRules::default());

        assert_eq!(queues[&1][0].progress, 20.0);
        assert_eq!(queues[&2][0].progress, 0.0);
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
    #[test]
    fn infrastructure_can_be_queued_in_full_shared_state_and_has_a_cap() {
        let mut world = test_world();
        world.states.get_mut(&1).unwrap().shared_building_slots = 2;
        let mut q = ConstructionQueues::new();
        assert!(enqueue_project(&world, "TST", 1, &mut q, &BuildingRules::default(), BuildingType::MilitaryFactory).is_err());
        assert!(enqueue_project(&world, "TST", 1, &mut q, &BuildingRules::default(), BuildingType::Infrastructure).is_err());
        world.states.get_mut(&1).unwrap().economy.infrastructure = 4;
        assert!(enqueue_project(&world, "TST", 1, &mut q, &BuildingRules::default(), BuildingType::Infrastructure).is_ok());
        assert_eq!(queued_shared_slots(&q[&1]), 0);
        assert!(enqueue_project(&world, "TST", 1, &mut q, &BuildingRules::default(), BuildingType::Infrastructure).is_err());
    }

    #[test]
    fn reserve_slots_and_cancel_queued_buildings() {
        let mut world = test_world();
        world.states.get_mut(&1).unwrap().shared_building_slots = 3;
        let mut q = ConstructionQueues::new();
        let rules = BuildingRules::default();
        assert!(enqueue_project(&world, "TST", 1, &mut q, &rules, BuildingType::CivilianFactory).is_ok());
        assert!(enqueue_project(&world, "TST", 1, &mut q, &rules, BuildingType::MilitaryFactory).is_err());
        assert!(cancel_project(&mut q, 1, 0));
        assert!(!q.contains_key(&1));
        assert!(enqueue_project(&world, "TST", 1, &mut q, &rules, BuildingType::MilitaryFactory).is_ok());
        assert!(enqueue_project(&world, "OTHER", 1, &mut q, &rules, BuildingType::Dockyard).is_err());
    }

    #[test]
    fn priority_assigns_full_lines_and_respects_economic_policy() {
        let mut world = test_world();
        world.countries.get_mut("TST").unwrap().state_ids.insert(2);
        world.states.insert(2, State { id: 2, name: None, owner: Some("TST".to_owned()),
            province_ids: vec![], shared_building_slots: 4,
            economy: StateEconomy { civilian_factories: 18, ..StateEconomy::default() }});
        let rules = BuildingRules::default();
        let mut q = ConstructionQueues::new();
        q.insert(1, VecDeque::from([ConstructionProject { building: BuildingType::MilitaryFactory, progress: 0.0, cost: 10000.0 }]));
        q.insert(2, VecDeque::from([ConstructionProject { building: BuildingType::MilitaryFactory, progress: 0.0, cost: 10000.0 }]));
        advance_day_ordered(&mut world, "TST", &mut q, &[2, 1], &rules, CountryConstructionPolicy::default());
        assert_eq!(q[&2][0].progress, 75.0);
        assert_eq!(q[&1][0].progress, 50.0);
        let cap = country_capacity(&world, "TST", CountryConstructionPolicy {
            consumer_goods_fraction: 0.5, trade_civilian_factories: 2, construction_speed_bonus: 0.0,
        }).unwrap();
        assert_eq!(cap.total_civilian, 20);
        assert_eq!(cap.consumer_goods, 10);
        assert_eq!(cap.available, 8);
    }

    #[test]
    fn infra_bonus_updates_dynamically_after_upgrade() {
        let mut world = test_world();
        let rules = BuildingRules::default();
        let mut q = ConstructionQueues::new();
        q.insert(1, VecDeque::from([ConstructionProject { building: BuildingType::CivilianFactory, progress: 0.0, cost: 1000.0 }]));
        world.states.get_mut(&1).unwrap().economy.infrastructure = 0;
        advance_day(&mut world, "TST", &mut q, &rules);
        assert_eq!(q[&1][0].progress, 10.0);
        world.states.get_mut(&1).unwrap().economy.infrastructure = 5;
        advance_day(&mut world, "TST", &mut q, &rules);
        assert_eq!(q[&1][0].progress, 30.0);
    }

    #[test]
    fn construction_ic_costs_use_zero_based_existing_level() {
        let mut rules = BuildingRules::default();
        rules.definitions.insert(BuildingType::RocketSite, BuildingDefinition {
            base_cost: 170.0,
            per_level_extra_cost: 130.0,
            per_controlled_building_extra_cost: 7.0,
            infrastructure_construction_effect: false,
        });
        assert_eq!(rules.project(BuildingType::RocketSite, 0, 0, 0).cost, 170.0);
        assert_eq!(rules.project(BuildingType::RocketSite, 1, 0, 0).cost, 300.0);
        assert_eq!(rules.project(BuildingType::RocketSite, 4, 3, 0).cost, 711.0);
    }

    #[test]
    fn construction_bonuses_are_additive_before_infrastructure() {
        let rules = BuildingRules::default();
        let policy = CountryConstructionPolicy {
            construction_speed_bonus: 0.10,
            ..Default::default()
        };
        let modifiers = ConstructionModifiers {
            national_bonus: 0.20,
            building_bonuses: HashMap::from([(BuildingType::MilitaryFactory, 0.10)]),
            state_bonuses: HashMap::from([(1, 0.10)]),
        };
        // 15 x 5 IC per day x (1 + .10+.20+.10+.10) x 2 = 225 IC/day
        let speed = daily_construction_points(
            &rules, policy, &modifiers, 1, BuildingType::MilitaryFactory, 5, 15,
        );
        assert!((speed - 225.0).abs() < 0.01);
        assert_eq!(
            daily_construction_points(
                &rules, policy, &modifiers, 1, BuildingType::CivilianFactory, 0, 1,
            ),
            7.0,
        );
    }

    #[test]
    fn consumer_goods_are_rounded_up_and_trading_reduces_building_capacity() {
        let mut world = test_world();
        world.states.get_mut(&1).unwrap().economy.civilian_factories = 13;
        world.states.get_mut(&1).unwrap().economy.military_factories = 8;
        let civilian = country_capacity(&world, "TST", CountryConstructionPolicy {
            consumer_goods_fraction: EconomicLawPreset::CivilianEconomy.consumer_goods_fraction(),
            trade_civilian_factories: 2,
            construction_speed_bonus: 0.0,
        }).unwrap();
        assert_eq!(civilian.consumer_goods, 8); // ceil(21 x .35)
        assert_eq!(civilian.trade, 2);
        assert_eq!(civilian.available, 3);
        let partial = country_capacity(&world, "TST", CountryConstructionPolicy {
            consumer_goods_fraction: EconomicLawPreset::PartialMobilization.consumer_goods_fraction(),
            trade_civilian_factories: 2,
            construction_speed_bonus: 0.0,
        }).unwrap();
        assert_eq!(partial.consumer_goods, 5);
        assert_eq!(partial.available, 6);
    }

    #[test]
    fn two_projects_in_one_state_are_independent_lines() {
        let mut world = test_world();
        world.states.get_mut(&1).unwrap().economy.civilian_factories = 20;
        world.states.get_mut(&1).unwrap().economy.infrastructure = 0;
        let mut queues = ConstructionQueues::new();
        queues.insert(1, VecDeque::from([
            ConstructionProject { building: BuildingType::MilitaryFactory, progress: 0.0, cost: 1000.0 },
            ConstructionProject { building: BuildingType::Dockyard, progress: 0.0, cost: 1000.0 },
        ]));
        let assigned = allocate_factories(
            &world, "TST", &queues, &[1], &BuildingRules::default(), CountryConstructionPolicy::default(),
        );
        assert_eq!(assigned.len(), 2);
        assert_eq!(assigned[0].civilian_factories, 15);
        assert_eq!(assigned[1].civilian_factories, 5);
        advance_day(&mut world, "TST", &mut queues, &BuildingRules::default());
        assert_eq!(queues[&1][0].progress, 75.0);
        assert_eq!(queues[&1][1].progress, 25.0);
    }

    #[test]
    fn completed_infrastructure_only_benefits_other_lines_next_day() {
        let mut world = test_world();
        world.states.get_mut(&1).unwrap().economy.infrastructure = 4;
        world.states.get_mut(&1).unwrap().economy.civilian_factories = 30;
        let mut queues = ConstructionQueues::new();
        queues.insert(1, VecDeque::from([
            ConstructionProject { building: BuildingType::Infrastructure, progress: 9.0, cost: 10.0 },
            ConstructionProject { building: BuildingType::MilitaryFactory, progress: 0.0, cost: 1000.0 },
        ]));
        advance_day(&mut world, "TST", &mut queues, &BuildingRules::default());
        assert_eq!(world.states[&1].economy.infrastructure, 5);
        assert_eq!(queues[&1].len(), 1);
        assert_eq!(queues[&1][0].building, BuildingType::MilitaryFactory);
        assert!((queues[&1][0].progress - 135.0).abs() < 0.01);
        advance_day(&mut world, "TST", &mut queues, &BuildingRules::default());
        assert!((queues[&1][0].progress - 285.0).abs() < 0.01);
    }

    #[test]
    fn enforces_maximum_of_one_thousand_queued_items_per_country() {
        let world = test_world();
        let mut queues = ConstructionQueues::new();
        queues.insert(1, std::iter::repeat_n(
            ConstructionProject::new(BuildingType::Infrastructure, 0),
            MAX_CONSTRUCTION_QUEUE_ITEMS,
        ).collect());
        assert_eq!(enqueue_project(
            &world, "TST", 1, &mut queues, &BuildingRules::default(), BuildingType::MilitaryFactory,
        ), Err("Country construction queue is full"));
    }

    #[test]
    fn eta_handles_stopped_lines_and_exact_completion_day() {
        let project = ConstructionProject {
            building: BuildingType::CivilianFactory, progress: 40.0, cost: 110.0,
        };
        assert_eq!(estimated_days_remaining(&project, 35.0), Some(2));
        assert_eq!(estimated_days_remaining(&project, 0.0), None);
        assert_eq!(estimated_days_remaining(&project, -1.0), None);
        assert_eq!(estimated_days_remaining(&ConstructionProject { progress: 110.0, ..project }, 0.0), Some(0));
    }

    #[test]
    fn parses_namespaced_lua_defines_and_trailing_comments() {
        let lua = "NDefines.NBuildings.BASE_FACTORY_SPEED = 4.0, -- old\nNDefines.NBuildings.BASE_FACTORY_SPEED = 5.0, -- final";
        assert_eq!(find_lua_number(lua, "BASE_FACTORY_SPEED"), Some(5.0));
    }

    #[test]
    fn building_slot_flags_are_read_from_level_caps() {
        let block = find_definition_block(
            "infrastructure = { level_cap = { state_max = 7 shares_slots = no } }", "infrastructure",
        ).unwrap();
        let cap = find_definition_block(block, "level_cap").unwrap();
        assert_eq!(top_level_number(cap, "state_max"), Some(7.0));
        assert_eq!(top_level_yes_no(cap, "shares_slots"), Some(false));
    }

}
