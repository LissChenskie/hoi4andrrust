//! Authoritative command hierarchy. Armies hold division IDs through one
//! membership index; they never own a second copy of a division.
use crate::military_data::{atom, block, data_layers, layered_files, number};
use crate::military_model::{DivisionId, MilitaryCatalogue, StatBlock};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub type ArmyId = u64;
pub type GroupId = u64;
pub type TheaterId = u64;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rank {
    General,
    FieldMarshal,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commander {
    pub id: String,
    pub country: String,
    pub name: String,
    pub rank: Rank,
    pub skill: u32,
    pub attack: u32,
    pub defense: u32,
    pub planning: u32,
    pub logistics: u32,
    pub experience: f64,
    pub traits: BTreeSet<String>,
    pub source: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theater {
    pub id: TheaterId,
    pub country: String,
    pub name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmyGroup {
    pub id: GroupId,
    pub country: String,
    pub name: String,
    pub theater: TheaterId,
    pub field_marshal: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Army {
    pub id: ArmyId,
    pub country: String,
    pub name: String,
    pub theater: TheaterId,
    pub group: Option<GroupId>,
    pub general: Option<String>,
    pub color: [u8; 3],
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct CommanderBonuses {
    pub attack: f64,
    pub breakthrough: f64,
    pub defense: f64,
    pub planning_speed: f64,
    pub planning_max: f64,
    pub supply: f64,
    pub movement: f64,
    pub recon: f64,
    pub tactic_skill: f64,
}
impl CommanderBonuses {
    pub fn add(&mut self, other: Self, scale: f64) {
        self.attack += other.attack * scale;
        self.breakthrough += other.breakthrough * scale;
        self.defense += other.defense * scale;
        self.planning_speed += other.planning_speed * scale;
        self.planning_max += other.planning_max * scale;
        self.supply += other.supply * scale;
        self.movement += other.movement * scale;
        self.recon += other.recon * scale;
        self.tactic_skill += other.tactic_skill * scale;
    }
    fn from_modifiers(m: &StatBlock) -> Self {
        let get = |key: &str| m.get(key).copied().unwrap_or(0.0);
        Self {
            attack: get("army_attack_factor"),
            breakthrough: get("army_defence_factor") + get("breakthrough_factor"),
            defense: get("army_defence_factor"),
            planning_speed: get("planning_speed"),
            planning_max: get("max_planning"),
            supply: get("supply_consumption_factor"),
            movement: get("army_speed_factor"),
            recon: get("recon_factor"),
            ..Default::default()
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TraitModifiers {
    pub shared: StatBlock,
    pub general: StatBlock,
    pub field_marshal: StatBlock,
    pub extra_divisions: i32,
    pub abilities: BTreeSet<String>,
    pub extra_armies: i32,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommandRules {
    pub general_cap: Option<i32>,
    pub marshal_divisions_cap: Option<i32>,
    pub garrison_factor: Option<f64>,
    pub marshal_armies_cap: Option<i32>,
    pub marshal_ratio: Option<f64>,
    pub marshal_xp_ratio: Option<f64>,
    pub combat_xp: Option<f64>,
    pub attack_per_skill: Option<f64>,
    pub defense_per_skill: Option<f64>,
    pub planning_per_skill: Option<f64>,
    pub max_planning_per_skill: Option<f64>,
    pub level_thresholds: Vec<f64>,
    pub level_weights: Vec<f64>,
    pub level_stat_count: Option<u32>,
    pub logistics_per_skill: Option<f64>,
    pub over_capacity_penalty: Option<f64>,
    pub traits: BTreeMap<String, TraitModifiers>,
}
impl CommandRules {
    pub fn from_catalogue(db: &MilitaryCatalogue) -> Self {
        let value = |name: &str| {
            db.literal_defines
                .get(&format!("NDefines.NMilitary.{name}"))
                .copied()
        };
        let stat = |names: &[&str], key: &str| {
            names
                .iter()
                .find_map(|name| db.weather.get(*name).and_then(|b| b.get(key)).copied())
        };
        Self {
            general_cap: value("CORPS_COMMANDER_DIVISIONS_CAP").map(|v| v as i32),
            marshal_divisions_cap: value("FIELD_MARSHAL_DIVISIONS_CAP").map(|v| v as i32),
            garrison_factor: value("GARRISON_ORDER_ARMY_CAP_FACTOR"),
            marshal_armies_cap: value("FIELD_MARSHAL_ARMIES_CAP").map(|v| v as i32),
            marshal_ratio: value("FIELD_MARSHAL_ARMY_BONUS_RATIO"),
            marshal_xp_ratio: value("FIELD_MARSHAL_XP_RATIO"),
            combat_xp: value("ARMY_LEADER_XP_GAIN_PER_UNIT_IN_COMBAT"),
            attack_per_skill: stat(
                &["army_attack_skill_factor", "army_leader_attack_skill"],
                "army_attack_factor",
            )
            .or(Some(0.025)),
            defense_per_skill: stat(
                &["army_defense_skill_factor", "army_leader_defense_skill"],
                "army_defence_factor",
            )
            .or(Some(0.025)),
            planning_per_skill: stat(
                &["army_planning_skill_factor", "army_leader_planning_skill"],
                "planning_speed",
            )
            .or(Some(0.05)),
            max_planning_per_skill: stat(
                &["army_planning_skill_factor", "army_leader_planning_skill"],
                "max_planning",
            )
            .or(Some(0.02)),
            level_weights: db
                .literal_arrays
                .get("NDefines.NMilitary.COMMANDER_LEVEL_UP_STAT_WEIGHTS")
                .cloned()
                .unwrap_or_default(),
            level_stat_count: value("COMMANDER_LEVEL_UP_STAT_COUNT").map(|n| n.max(0.0) as u32),
            logistics_per_skill: stat(
                &["army_logistics_skill_factor", "army_leader_logistics_skill"],
                "supply_consumption_factor",
            )
            .or(Some(-0.025)),
            over_capacity_penalty: value("UNIT_LEADER_DIVISIONS_CAP_PENALTY"),
            ..Default::default()
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommandState {
    pub armies: BTreeMap<ArmyId, Army>,
    pub groups: BTreeMap<GroupId, ArmyGroup>,
    pub theaters: BTreeMap<TheaterId, Theater>,
    pub commanders: BTreeMap<String, Commander>,
    pub membership: BTreeMap<DivisionId, ArmyId>,
    pub rules: CommandRules,
    pub revision: u64,
    pub garrison_armies: BTreeSet<ArmyId>,
    /// Immutable import projection, rebuilt from current sources after load.
    #[serde(skip)]
    pub data: hoi4_mobile::command_data::CommandData,
    next_id: u64,
}
impl CommandState {
    fn id(&mut self) -> u64 {
        self.next_id = self.next_id.checked_add(1).expect("command ID exhausted");
        self.next_id
    }
    pub fn identifiers_valid(&self) -> bool {
        self.armies
            .keys()
            .chain(self.groups.keys())
            .chain(self.theaters.keys())
            .all(|id| *id <= self.next_id)
    }
    pub fn members(&self, army: ArmyId) -> impl Iterator<Item = DivisionId> + '_ {
        self.membership
            .iter()
            .filter(move |(_, a)| **a == army)
            .map(|(id, _)| *id)
    }
    pub fn create_theater(&mut self, country: &str, name: &str) -> TheaterId {
        let id = self.id();
        self.theaters.insert(
            id,
            Theater {
                id,
                country: country.into(),
                name: name.into(),
            },
        );
        self.revision += 1;
        id
    }
    pub fn create_army(&mut self, country: &str, name: &str, divisions: &[DivisionId]) -> ArmyId {
        let theater = self
            .theaters
            .values()
            .find(|t| t.country == country)
            .map(|t| t.id)
            .unwrap_or_else(|| self.create_theater(country, "Основной театр"));
        let id = self.id();
        // Presentation palette, never a gameplay constant.
        let palette = [
            [72, 157, 214],
            [210, 87, 65],
            [124, 174, 80],
            [192, 148, 64],
            [153, 110, 190],
        ];
        self.armies.insert(
            id,
            Army {
                id,
                country: country.into(),
                name: if name.trim().is_empty() {
                    let mut number = 1;
                    while self
                        .armies
                        .values()
                        .any(|a| a.country == country && a.name == format!("Армия {number}"))
                    {
                        number += 1;
                    }
                    format!("Армия {number}")
                } else {
                    name.into()
                },
                theater,
                group: None,
                general: None,
                color: palette[id as usize % palette.len()],
            },
        );
        for d in divisions {
            self.membership.insert(*d, id);
        }
        self.revision += 1;
        id
    }
    pub fn delete_army(&mut self, country: &str, id: ArmyId) -> Result<(), String> {
        self.own_army(country, id)?;
        self.armies.remove(&id);
        self.membership.retain(|_, a| *a != id);
        self.revision += 1;
        Ok(())
    }
    pub fn own_army(&self, country: &str, id: ArmyId) -> Result<&Army, String> {
        self.armies
            .get(&id)
            .filter(|a| a.country == country)
            .ok_or_else(|| "Выберите свою армию".into())
    }
    pub fn assign(
        &mut self,
        country: &str,
        id: ArmyId,
        divisions: &[DivisionId],
    ) -> Result<(), String> {
        let army = self.own_army(country, id)?;
        if let Some(c) = army.general.as_ref().and_then(|c| self.commanders.get(c)) {
            let count = self
                .members(id)
                .chain(divisions.iter().copied())
                .collect::<BTreeSet<_>>()
                .len();
            if self
                .army_cap(c, id)
                .is_some_and(|cap| cap < 0 || cap > 0 && count > cap as usize)
                && self.rules.over_capacity_penalty.is_none()
            {
                return Err("Превышен лимит генерала; правило штрафа не импортировано".into());
            }
        }
        for d in divisions {
            self.membership.insert(*d, id);
        }
        self.revision += 1;
        Ok(())
    }
    pub fn create_group(
        &mut self,
        country: &str,
        army: ArmyId,
        name: &str,
    ) -> Result<GroupId, String> {
        let theater = self.own_army(country, army)?.theater;
        let id = self.id();
        self.groups.insert(
            id,
            ArmyGroup {
                id,
                country: country.into(),
                name: if name.trim().is_empty() {
                    format!("Группа армий {id}")
                } else {
                    name.into()
                },
                theater,
                field_marshal: None,
            },
        );
        self.armies.get_mut(&army).unwrap().group = Some(id);
        self.revision += 1;
        Ok(id)
    }
    pub fn set_group(
        &mut self,
        country: &str,
        army: ArmyId,
        group: Option<GroupId>,
    ) -> Result<(), String> {
        self.own_army(country, army)?;
        if group.is_some_and(|id| !self.groups.get(&id).is_some_and(|g| g.country == country)) {
            return Err("Чужая группа армий".into());
        }
        if let Some(g) = group.and_then(|id| self.groups.get(&id)) {
            if let Some(c) = g
                .field_marshal
                .as_ref()
                .and_then(|id| self.commanders.get(id))
            {
                let count = self
                    .armies
                    .values()
                    .filter(|a| a.group == Some(g.id) && a.id != army)
                    .count()
                    + 1;
                if self
                    .cap(c, true)
                    .is_some_and(|cap| cap < 0 || cap > 0 && count > cap as usize)
                {
                    return Err("Превышен лимит армий фельдмаршала".into());
                }
            }
        }
        self.armies.get_mut(&army).unwrap().group = group;
        self.revision += 1;
        Ok(())
    }
    pub fn delete_group(&mut self, country: &str, id: GroupId) -> Result<(), String> {
        if !self.groups.get(&id).is_some_and(|g| g.country == country) {
            return Err("Выберите свою группу армий".into());
        }
        self.groups.remove(&id);
        for a in self.armies.values_mut() {
            if a.group == Some(id) {
                a.group = None;
            }
        }
        self.revision += 1;
        Ok(())
    }
    pub fn cap(&self, commander: &Commander, group: bool) -> Option<i32> {
        let base = if group {
            self.rules.marshal_armies_cap
        } else if commander.rank == Rank::FieldMarshal {
            self.rules.marshal_divisions_cap.or(self.rules.general_cap)
        } else {
            self.rules.general_cap
        }?;
        if base <= 0 {
            return Some(base);
        }
        let extra: i32 = commander
            .traits
            .iter()
            .filter_map(|id| self.rules.traits.get(id))
            .map(|t| {
                if group {
                    t.extra_armies
                } else {
                    t.extra_divisions
                }
            })
            .sum();
        Some(base + extra)
    }
    pub fn army_cap(&self, c: &Commander, army: ArmyId) -> Option<i32> {
        self.cap(c, false).map(|n| {
            if n > 0 && self.garrison_armies.contains(&army) {
                (n as f64 * self.rules.garrison_factor.unwrap_or(1.0)) as i32
            } else {
                n
            }
        })
    }
    pub fn set_general(
        &mut self,
        country: &str,
        army: ArmyId,
        commander: Option<&str>,
    ) -> Result<(), String> {
        self.own_army(country, army)?;
        if let Some(id) = commander {
            let c = self
                .commanders
                .get(id)
                .filter(|c| c.country == country)
                .ok_or("Командующий не доступен стране")?;
            if self
                .armies
                .values()
                .any(|a| a.id != army && a.general.as_deref() == Some(id))
                || self
                    .groups
                    .values()
                    .any(|g| g.field_marshal.as_deref() == Some(id))
            {
                return Err("Командующий уже назначен".into());
            }
            if self
                .army_cap(c, army)
                .is_some_and(|cap| cap < 0 || cap > 0 && self.members(army).count() > cap as usize)
                && self.rules.over_capacity_penalty.is_none()
            {
                return Err("Превышен лимит командования; правило штрафа не импортировано".into());
            }
        }
        self.armies.get_mut(&army).unwrap().general = commander.map(str::to_owned);
        self.revision += 1;
        Ok(())
    }
    pub fn set_marshal(
        &mut self,
        country: &str,
        group: GroupId,
        commander: Option<&str>,
    ) -> Result<(), String> {
        if !self
            .groups
            .get(&group)
            .is_some_and(|g| g.country == country)
        {
            return Err("Выберите свою группу".into());
        }
        if let Some(id) = commander {
            let c = self
                .commanders
                .get(id)
                .filter(|c| c.country == country && c.rank == Rank::FieldMarshal)
                .ok_or("Нужен доступный фельдмаршал")?;
            if self
                .armies
                .values()
                .any(|a| a.general.as_deref() == Some(id))
                || self
                    .groups
                    .values()
                    .any(|g| g.id != group && g.field_marshal.as_deref() == Some(id))
            {
                return Err("Командующий уже назначен".into());
            }
            let count = self
                .armies
                .values()
                .filter(|a| a.group == Some(group))
                .count();
            if self
                .cap(c, true)
                .is_some_and(|cap| cap < 0 || cap > 0 && count > cap as usize)
            {
                return Err("Превышен лимит армий фельдмаршала".into());
            }
        }
        self.groups.get_mut(&group).unwrap().field_marshal = commander.map(str::to_owned);
        self.revision += 1;
        Ok(())
    }
    fn leader_base(&self, c: &Commander) -> CommanderBonuses {
        let mut b = self.leader_bonus(c, None);
        b.tactic_skill = c.skill as f64;
        b
    }
    fn leader_bonus(&self, c: &Commander, marshal: Option<bool>) -> CommanderBonuses {
        let mut b = CommanderBonuses {
            attack: c.attack as f64 * self.rules.attack_per_skill.unwrap_or(0.0),
            defense: c.defense as f64 * self.rules.defense_per_skill.unwrap_or(0.0),
            breakthrough: c.defense as f64 * self.rules.defense_per_skill.unwrap_or(0.0),
            planning_speed: c.planning as f64 * self.rules.planning_per_skill.unwrap_or(0.0),
            planning_max: c.planning as f64 * self.rules.max_planning_per_skill.unwrap_or(0.0),
            supply: c.logistics as f64 * self.rules.logistics_per_skill.unwrap_or(0.0),
            tactic_skill: c.skill as f64,
            ..Default::default()
        };
        for id in &c.traits {
            if let Some(t) = self.rules.traits.get(id) {
                b.add(CommanderBonuses::from_modifiers(&t.shared), 1.0);
                if let Some(marshal) = marshal {
                    b.add(
                        CommanderBonuses::from_modifiers(if marshal {
                            &t.field_marshal
                        } else {
                            &t.general
                        }),
                        1.0,
                    );
                }
            }
        }
        b
    }
    pub fn bonuses(&self, division: DivisionId) -> CommanderBonuses {
        let Some(army) = self
            .membership
            .get(&division)
            .and_then(|id| self.armies.get(id))
        else {
            return Default::default();
        };
        let mut b = CommanderBonuses::default();
        if let Some(c) = army.general.as_ref().and_then(|id| self.commanders.get(id)) {
            let count = self.members(army.id).count();
            let cap = self.army_cap(c, army.id).unwrap_or(0);
            let excess = if cap > 0 {
                count.saturating_sub(cap as usize)
            } else {
                0
            };
            let factor = if excess == 0 {
                1.0
            } else {
                (1.0 - excess as f64 * self.rules.over_capacity_penalty.unwrap_or(1.0)).max(0.0)
            };
            b.add(self.leader_bonus(c, Some(false)), factor);
        }
        if let Some(c) = army
            .group
            .and_then(|id| self.groups.get(&id))
            .and_then(|g| g.field_marshal.as_ref())
            .and_then(|id| self.commanders.get(id))
        {
            let mut shared = self.leader_base(c);
            shared.tactic_skill = 0.0;
            b.add(shared, self.rules.marshal_ratio.unwrap_or(0.0));
            for id in &c.traits {
                if let Some(t) = self.rules.traits.get(id) {
                    b.add(CommanderBonuses::from_modifiers(&t.field_marshal), 1.0);
                }
            }
        }
        b
    }
    pub fn advance_levels<R: crate::combat_rules::CombatRandom>(&mut self, rng: &mut R) {
        let weights = &self.rules.level_weights;
        if weights.len() != 4 || weights.iter().any(|v| !v.is_finite() || *v < 0.0) {
            return;
        }
        let total: f64 = weights.iter().sum();
        if total <= 0.0 {
            return;
        }
        let Some(count) = self.rules.level_stat_count else {
            return;
        };
        for c in self.commanders.values_mut() {
            while let Some(&required) = self
                .rules
                .level_thresholds
                .get(c.skill.saturating_sub(1) as usize)
            {
                if !required.is_finite() || required <= 0.0 || c.experience < required {
                    break;
                }
                c.experience -= required;
                c.skill += 1;
                for _ in 0..count {
                    let mut roll = rng.unit() * total;
                    let mut pick = 3;
                    for (i, w) in weights.iter().enumerate() {
                        if roll < *w {
                            pick = i;
                            break;
                        }
                        roll -= w;
                    }
                    match pick {
                        0 => c.attack += 1,
                        1 => c.defense += 1,
                        2 => c.planning += 1,
                        _ => c.logistics += 1,
                    }
                }
                self.revision += 1;
            }
        }
    }
    pub fn set_theater(
        &mut self,
        country: &str,
        army: ArmyId,
        theater: TheaterId,
    ) -> Result<(), String> {
        let a = self.own_army(country, army)?;
        let group = a.group;
        if !self
            .theaters
            .get(&theater)
            .is_some_and(|t| t.country == country)
        {
            return Err("Выберите свой театр".into());
        }
        if let Some(group) = group {
            self.groups.get_mut(&group).unwrap().theater = theater;
        }
        for a in self
            .armies
            .values_mut()
            .filter(|a| a.id == army || group.is_some() && a.group == group)
        {
            a.theater = theater;
        }
        self.revision += 1;
        Ok(())
    }
    pub fn gain_combat_xp(&mut self, units: &[DivisionId]) {
        let Some(rate) = self.rules.combat_xp else {
            return;
        };
        let mut gain = BTreeMap::<String, f64>::new();
        for d in units {
            if let Some(a) = self.membership.get(d).and_then(|id| self.armies.get(id)) {
                if let Some(id) = &a.general {
                    *gain.entry(id.clone()).or_default() += rate;
                }
                if let Some(id) = a
                    .group
                    .and_then(|id| self.groups.get(&id))
                    .and_then(|g| g.field_marshal.as_ref())
                {
                    *gain.entry(id.clone()).or_default() +=
                        rate * self.rules.marshal_xp_ratio.unwrap_or(0.0);
                }
            }
        }
        for (id, xp) in gain {
            if let Some(c) = self.commanders.get_mut(&id) {
                c.experience += xp;
            }
        }
    }
}

pub fn load(
    root: &Path,
    warfare: &crate::warfare::Warfare,
    date: crate::game::GameDate,
) -> CommandState {
    let mut state = CommandState {
        rules: CommandRules::from_catalogue(&warfare.catalogue),
        ..Default::default()
    };
    let layers = data_layers(root);
    use hoi4_mobile::command_data::{CommandData, CommanderRole, Severity};
    let mut data = CommandData {
        source_version: warfare.catalogue.source_version.clone(),
        ..Default::default()
    };
    let source_name = |path: &Path| {
        layers
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, layer)| {
                path.strip_prefix(layer)
                    .ok()
                    .map(|p| format!("layer-{i}/{}", p.to_string_lossy().replace('\\', "/")))
            })
            .unwrap_or_else(|| path.display().to_string())
    };
    for (directory, traits) in [("common/characters", false), ("common/unit_leader", true)] {
        for path in layered_files(&layers, directory, true, "txt") {
            let file = source_name(&path);
            match std::fs::read_to_string(&path) {
                Ok(text) if traits => data.import_traits(&file, &text),
                Ok(text) => data.import_characters(&file, &text),
                Err(error) => data.diagnostic(
                    Severity::Error,
                    "read",
                    data.source(&file, "", 0),
                    error.to_string(),
                ),
            }
        }
    }
    let keys: BTreeSet<_> = data
        .characters
        .values()
        .map(|c| c.name_key.clone())
        .chain(
            data.traits
                .values()
                .flat_map(|t| [t.name_key.clone(), t.description_key.clone()]),
        )
        .collect();
    for path in layered_files(&layers, "localisation", true, "yml") {
        let file = source_name(&path);
        match std::fs::read_to_string(&path) {
            Ok(text) => data.import_selected_localization(&file, &text, &keys),
            Err(error) => data.diagnostic(
                Severity::Warning,
                "read",
                data.source(&file, "", 0),
                error.to_string(),
            ),
        }
    }
    data.validate();
    let mut candidates = BTreeMap::new();
    for character in data.characters.values() {
        if data
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error && d.source.object_id == character.id)
        {
            continue;
        }
        // Preserve the existing runtime rank policy; both role profiles remain
        // available in the IR. No duplicate commander/XP/assignment is created.
        let Some(profile) = data
            .profile(&character.id, CommanderRole::FieldMarshal)
            .or_else(|| data.profile(&character.id, CommanderRole::General))
        else {
            continue;
        };
        let stats = &profile.stats;
        let (
            Some(skill),
            Some(attack),
            Some(defense),
            Some(planning),
            Some(logistics),
            Some(experience),
        ) = (
            stats.skill,
            stats.attack,
            stats.defense,
            stats.planning,
            stats.logistics,
            profile.initial_experience,
        )
        else {
            continue;
        };
        candidates.insert(
            character.id.clone(),
            Commander {
                id: character.id.clone(),
                country: String::new(),
                name: data.localization.get("russian", &character.name_key).into(),
                rank: match profile.role {
                    CommanderRole::General => Rank::General,
                    CommanderRole::FieldMarshal => Rank::FieldMarshal,
                },
                skill,
                attack,
                defense,
                planning,
                logistics,
                experience,
                traits: profile.trait_ids.clone(),
                source: profile.source.file.clone(),
            },
        );
    }
    for (tag, army) in &warfare.countries {
        let Some(path) = &army.history_source else {
            continue;
        };
        let file = source_name(Path::new(path));
        let doc = match std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|s| crate::pdx::parse_pdx_compatible(&s))
        {
            Ok(doc) => doc,
            Err(error) => {
                data.diagnostic(
                    Severity::Error,
                    "country_history",
                    data.source(&file, tag, 0),
                    error,
                );
                continue;
            }
        };
        let (entries, issues) =
            crate::history_conditions::resolve(&doc.entries, tag, &army.modifiers, date);
        for issue in issues {
            data.diagnostic(
                Severity::Warning,
                "history_condition",
                data.source(&file, tag, 0),
                issue,
            );
        }
        for e in entries.iter().filter(|e| e.key == "recruit_character") {
            if let Some(mut c) = e.atom().and_then(|id| candidates.get(id)).cloned() {
                c.country = tag.clone();
                if state
                    .commanders
                    .get(&c.id)
                    .is_some_and(|existing| existing.country != c.country)
                {
                    data.diagnostic(
                        Severity::Error,
                        "commander_country_conflict",
                        data.source(&file, &c.id, e.span.line),
                        "One character recruited by multiple countries; first assignment retained",
                    );
                } else {
                    state.commanders.insert(c.id.clone(), c);
                }
            } else if let Some(id) = e.atom().filter(|id| !data.characters.contains_key(*id)) {
                data.diagnostic(
                    Severity::Error,
                    "unknown_recruit_character",
                    data.source(&file, id, e.span.line),
                    format!("Cannot resolve recruited character {id}"),
                );
            }
        }
    }
    for (id, definition) in &data.traits {
        let b = &definition.fields;
        state.rules.traits.insert(
            id.clone(),
            TraitModifiers {
                abilities: block(b, "enable_ability")
                    .map(crate::military_data::names)
                    .unwrap_or_else(|| {
                        atom(b, "enable_ability")
                            .map(|s| BTreeSet::from([s.into()]))
                            .unwrap_or_default()
                    }),
                shared: block(b, "modifier")
                    .map(|b| crate::military_data::stats(b, &Default::default()))
                    .unwrap_or_default(),
                general: block(b, "non_shared_modifier")
                    .map(|b| crate::military_data::stats(b, &Default::default()))
                    .unwrap_or_default(),
                field_marshal: block(b, "field_marshal_modifier")
                    .map(|b| crate::military_data::stats(b, &Default::default()))
                    .unwrap_or_default(),
                extra_divisions: number(b, "command_limit", 0.0) as i32,
                extra_armies: number(b, "army_group_size", 0.0) as i32,
            },
        );
    }
    state.data = data;
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authored_source_to_existing_division_army_and_mobile_projection() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/command-data");
        let root = std::env::temp_dir().join(format!(
            "hoi4-command-slice-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let put = |name: &str, text: &str| {
            let p = root.join(name);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        };
        for name in [
            "common/characters/basic.txt",
            "common/characters/multirole.txt",
            "common/characters/marshal.txt",
            "common/unit_leader/traits.txt",
            "localisation/test_l_english.yml",
            "localisation/test_l_russian.yml",
            "history/units/TEST_1936.txt",
            "history/units/TEST_1939.txt",
        ] {
            put(name, &std::fs::read_to_string(fixture.join(name)).unwrap());
        }
        put(
            "history/countries/A.txt",
            &std::fs::read_to_string(fixture.join("history/countries/TEST.txt")).unwrap(),
        );
        put(
            "common/units/test.txt",
            "sub_units={infantry={group=infantry active=yes max_strength=25 max_organisation=60 manpower=1000 combat_width=2 maximum_speed=4 supply_consumption=1} engineer={group=support active=yes max_strength=1 max_organisation=20 manpower=100 maximum_speed=4}}",
        );
        let world = crate::warfare::tests::world();
        let mut w = crate::military_history::initialize(
            &root,
            &world,
            &Default::default(),
            &mut Default::default(),
            crate::game::GameDate::START,
        );
        assert_eq!(w.countries["A"].history_imported, 1);
        assert_eq!(w.command.commanders.len(), 3);
        assert_eq!(w.command.data.profiles.len(), 4);
        assert_eq!(w.command.data.summary().errors, 0);
        let division = *w.divisions.keys().next().unwrap();
        assert_eq!(w.divisions[&division].derived.stats.width, 4.0);
        assert_eq!(w.countries["A"].templates["TestInfantry"].line.len(), 2);
        assert_eq!(w.countries["A"].templates["TestInfantry"].support.len(), 1);
        let army = w.create_army("A", &[division], "Test Army").unwrap();
        w.command
            .set_general("A", army, Some("TEST_general"))
            .unwrap();
        let vm = crate::army_projection::army(&w, "A", army, "russian").unwrap();
        assert_eq!(vm.division_ids, vec![division]);
        let c = vm.commander.unwrap();
        assert_eq!(c.name_key, "TEST_GENERAL_NAME");
        assert_eq!(c.name, "Тестовый генерал");
        assert_eq!(c.traits[0].name, "Организатор");
        assert!(c.traits[0].definition_known);
        assert_eq!(c.experience, 7.5);
        assert_eq!(c.next_level_xp, None);
        assert!((w.command.bonuses(division).attack - 0.125).abs() < 1e-9);
        assert!(w.abilities.hq_rules.is_none());
        let report = crate::military_history::import_report(&w, crate::game::GameDate::START);
        assert_eq!(report["command_summary"]["profiles"], 4);
        assert_eq!(report["army_views"][0]["division_count"], 1);
        // An unknown recruitment reference must surface as an error, not vanish.
        put("history/countries/A.txt", "recruit_character=MISSING");
        w.command = load(&root, &w, crate::game::GameDate::START);
        assert!(
            w.command
                .data
                .diagnostics
                .iter()
                .any(|d| d.code == "unknown_recruit_character")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn hierarchy_moves_members_without_copying_divisions_or_deleting_them() {
        let mut w = crate::warfare::Warfare::new(crate::military_model::tests::catalogue());
        for id in 1..=12 {
            w.divisions
                .insert(id, crate::military_model::tests::division(id, "A", 1));
        }
        let ids: Vec<_> = (1..=12).collect();
        let a = w.create_army("A", &ids, "").unwrap();
        let b = w.create_army("A", &[1], "Second").unwrap();
        assert_eq!(w.command.members(a).count(), 11);
        let g = w.command.create_group("A", a, "").unwrap();
        w.command.set_group("A", b, Some(g)).unwrap();
        assert_eq!(w.command.armies[&b].group, Some(g));
        assert!(w.assign_army("B", a, &[2]).is_err());
        w.command.delete_group("A", g).unwrap();
        assert!(w.command.armies[&a].group.is_none());
        w.command.delete_army("A", a).unwrap();
        assert_eq!(w.divisions.len(), 12);
        assert_eq!(w.command.membership.len(), 1);
    }
    #[test]
    fn imported_caps_commanders_and_marshal_modifiers_apply_once() {
        let mut s = CommandState::default();
        s.rules.general_cap = Some(24);
        s.rules.marshal_armies_cap = Some(5);
        s.rules.attack_per_skill = Some(0.025);
        s.rules.marshal_ratio = Some(0.5);
        s.rules.combat_xp = Some(0.1);
        s.rules.marshal_xp_ratio = Some(0.3);
        let c = |id: &str, rank| Commander {
            id: id.into(),
            country: "A".into(),
            name: id.into(),
            rank,
            skill: 3,
            attack: 4,
            defense: 2,
            planning: 1,
            logistics: 1,
            experience: 0.0,
            traits: Default::default(),
            source: "authored fixture".into(),
        };
        s.commanders
            .insert("general".into(), c("general", Rank::General));
        s.commanders
            .insert("marshal".into(), c("marshal", Rank::FieldMarshal));
        let a = s.create_army("A", "", &[1, 2]);
        s.set_general("A", a, Some("general")).unwrap();
        let g = s.create_group("A", a, "").unwrap();
        s.set_marshal("A", g, Some("marshal")).unwrap();
        assert!((s.bonuses(1).attack - 0.15).abs() < 1e-9);
        s.gain_combat_xp(&[1, 2]);
        assert!((s.commanders["general"].experience - 0.2).abs() < 1e-9);
        assert!((s.commanders["marshal"].experience - 0.06).abs() < 1e-9);
        let other = s.create_army("A", "", &[3]);
        assert!(s.set_general("A", other, Some("general")).is_err());
        let snapshot = serde_json::to_string(&s).unwrap();
        let restored: CommandState = serde_json::from_str(&snapshot).unwrap();
        assert_eq!(restored.membership, s.membership);
    }
}
