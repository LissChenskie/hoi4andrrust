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
    pub defense: f64,
    pub planning_speed: f64,
    pub planning_max: f64,
    pub supply: f64,
    pub movement: f64,
    pub recon: f64,
    pub tactic_skill: f64,
}
impl CommanderBonuses {
    fn add(&mut self, other: Self, scale: f64) {
        self.attack += other.attack * scale;
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
    pub extra_armies: i32,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommandRules {
    pub general_cap: Option<i32>,
    pub marshal_armies_cap: Option<i32>,
    pub marshal_ratio: Option<f64>,
    pub marshal_xp_ratio: Option<f64>,
    pub combat_xp: Option<f64>,
    pub attack_per_skill: Option<f64>,
    pub defense_per_skill: Option<f64>,
    pub planning_per_skill: Option<f64>,
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
            marshal_armies_cap: value("FIELD_MARSHAL_ARMIES_CAP").map(|v| v as i32),
            marshal_ratio: value("FIELD_MARSHAL_ARMY_BONUS_RATIO"),
            marshal_xp_ratio: value("FIELD_MARSHAL_XP_RATIO"),
            combat_xp: value("ARMY_LEADER_XP_GAIN_PER_UNIT_IN_COMBAT"),
            attack_per_skill: stat(
                &["army_attack_skill_factor", "army_leader_attack_skill"],
                "army_attack_factor",
            ),
            defense_per_skill: stat(
                &["army_defense_skill_factor", "army_leader_defense_skill"],
                "army_defence_factor",
            ),
            planning_per_skill: stat(
                &["army_planning_skill_factor", "army_leader_planning_skill"],
                "planning_speed",
            ),
            logistics_per_skill: stat(
                &["army_logistics_skill_factor", "army_leader_logistics_skill"],
                "supply_consumption_factor",
            ),
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
    next_id: u64,
}
impl CommandState {
    fn id(&mut self) -> u64 {
        self.next_id = self.next_id.checked_add(1).expect("command ID exhausted");
        self.next_id
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
                    format!("Армия {id}")
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
        self.own_army(country, id)?;
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
                .cap(c, false)
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
    fn leader_bonus(&self, c: &Commander, marshal: bool) -> CommanderBonuses {
        let mut b = CommanderBonuses {
            attack: c.attack as f64 * self.rules.attack_per_skill.unwrap_or(0.0),
            defense: c.defense as f64 * self.rules.defense_per_skill.unwrap_or(0.0),
            planning_speed: c.planning as f64 * self.rules.planning_per_skill.unwrap_or(0.0),
            supply: c.logistics as f64 * self.rules.logistics_per_skill.unwrap_or(0.0),
            tactic_skill: c.skill as f64,
            ..Default::default()
        };
        for id in &c.traits {
            if let Some(t) = self.rules.traits.get(id) {
                b.add(CommanderBonuses::from_modifiers(&t.shared), 1.0);
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
            let cap = self.cap(c, false).unwrap_or(0);
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
            b.add(self.leader_bonus(c, false), factor);
        }
        if let Some(c) = army
            .group
            .and_then(|id| self.groups.get(&id))
            .and_then(|g| g.field_marshal.as_ref())
            .and_then(|id| self.commanders.get(id))
        {
            let mut shared = self.leader_bonus(c, false);
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
    let mut candidates = BTreeMap::new();
    for path in layered_files(&layers, "common/characters", true, "txt") {
        let Ok(s) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(doc) = crate::pdx::parse_pdx_compatible(&s) else {
            continue;
        };
        for c in block(&doc.entries, "characters").unwrap_or(&[]) {
            let Some(body) = c.block() else { continue };
            let (rank, b) = if let Some(b) = block(body, "field_marshal") {
                (Rank::FieldMarshal, b)
            } else if let Some(b) = block(body, "corps_commander") {
                (Rank::General, b)
            } else {
                continue;
            };
            candidates.insert(
                c.key.clone(),
                Commander {
                    id: c.key.clone(),
                    country: String::new(),
                    name: atom(body, "name").unwrap_or(&c.key).into(),
                    rank,
                    skill: number(b, "skill", 0.0) as u32,
                    attack: number(b, "attack_skill", 0.0) as u32,
                    defense: number(b, "defense_skill", 0.0) as u32,
                    planning: number(b, "planning_skill", 0.0) as u32,
                    logistics: number(b, "logistics_skill", 0.0) as u32,
                    experience: number(b, "experience", 0.0),
                    traits: block(b, "traits")
                        .map(crate::military_data::names)
                        .unwrap_or_default(),
                    source: path.display().to_string(),
                },
            );
        }
    }
    for (tag, army) in &warfare.countries {
        let Some(path) = &army.history_source else {
            continue;
        };
        let Ok(s) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(doc) = crate::pdx::parse_pdx_compatible(&s) else {
            continue;
        };
        let (entries, _) =
            crate::history_conditions::resolve(&doc.entries, tag, &army.modifiers, date);
        for e in entries.iter().filter(|e| e.key == "recruit_character") {
            if let Some(mut c) = e.atom().and_then(|id| candidates.get(id)).cloned() {
                c.country = tag.clone();
                state.commanders.insert(c.id.clone(), c);
            }
        }
    }
    for path in layered_files(&layers, "common/unit_leader", true, "txt") {
        let Ok(s) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(doc) = crate::pdx::parse_pdx_compatible(&s) else {
            continue;
        };
        for t in block(&doc.entries, "leader_traits").unwrap_or(&doc.entries) {
            let Some(b) = t.block() else { continue };
            state.rules.traits.insert(
                t.key.clone(),
                TraitModifiers {
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
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;
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
