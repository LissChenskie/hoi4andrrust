//! Imported commander abilities, command power and optional, version-pinned HQ rules.
//! This module contains no vanilla ability definitions or invented HQ constants.
use crate::{
    command::{ArmyId, Commander, CommanderBonuses},
    military_data::{atom, block},
    military_model::{MilitaryModifiers, StatBlock},
    research_data::Entry,
    warfare::Warfare,
    world::World,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbilityDefinition {
    pub id: String,
    pub name: String,
    pub source: String,
    pub allowed: Vec<Entry>,
    pub cost_per_battalion: Option<f64>,
    pub duration: Option<u64>,
    pub cooldown: Option<u64>,
    pub modifiers: StatBlock,
    pub blocked: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveAbility {
    pub leader: String,
    pub ability: String,
    pub expires: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HqBand {
    pub max_km: f64,
    pub planning_speed: f64,
    pub planning_max: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HqRules {
    pub unit_ids: BTreeSet<String>,
    pub required_dlc: BTreeSet<String>,
    pub distance_bands: Vec<HqBand>,
    pub follow_distance_km: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandProfile {
    pub version: String,
    pub checksum: Option<String>,
    pub provenance: String,
    #[serde(default)]
    pub level_thresholds: Vec<f64>,
    pub hq: Option<HqRules>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmyHq {
    pub division: u64,
    pub army: ArmyId,
    pub plan: Option<u64>,
    pub automatic: bool,
    pub distance_km: Option<f64>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AbilityState {
    pub definitions: BTreeMap<String, AbilityDefinition>,
    pub active: Vec<ActiveAbility>,
    pub cooldowns: BTreeMap<String, BTreeMap<String, u64>>,
    pub hq_rules: Option<HqRules>,
    pub headquarters: BTreeMap<u64, ArmyHq>,
}
const SUPPORTED: &[&str] = &[
    "offence",
    "defence",
    "army_attack_factor",
    "army_defence_factor",
    "breakthrough_factor",
    "planning_speed",
    "max_planning",
    "army_speed_factor",
    "supply_consumption_factor",
    "dig_in_speed",
    "land_reinforce_rate",
    "org_damage_multiplier",
    "str_damage_multiplier",
    "cannot_retreat_while_attacking",
    "cannot_retreat_while_defending",
    "combat_entrenchment",
];
impl AbilityState {
    pub fn import(&mut self, source: &str, path: &str) -> Result<(), String> {
        let doc = crate::pdx::parse_pdx_compatible(source)?;
        for e in block(&doc.entries, "ability").unwrap_or(&doc.entries) {
            let Some(b) = e.block() else { continue };
            if atom(b, "type") != Some("army_leader") {
                continue;
            }
            let raw = block(b, "unit_modifiers").unwrap_or(&[]);
            let mut blocked = Vec::new();
            for e in raw {
                if !SUPPORTED.contains(&e.key.as_str())
                    || e.atom()
                        .and_then(|s| s.parse::<f64>().ok())
                        .filter(|v| v.is_finite())
                        .is_none()
                {
                    blocked.push(format!("Неподдержанный эффект {}", e.key));
                }
            }
            for key in ["one_time_effect", "on_add", "on_remove", "modifier"] {
                if block(b, key).is_some() {
                    blocked.push(format!("Требуется исполнение {key}"));
                }
            }
            let literal = |key: &str| {
                atom(b, key)
                    .and_then(|s| s.parse::<f64>().ok())
                    .filter(|v| v.is_finite() && *v >= 0.0)
            };
            self.definitions.insert(
                e.key.clone(),
                AbilityDefinition {
                    id: e.key.clone(),
                    name: atom(b, "name").unwrap_or(&e.key).into(),
                    source: path.into(),
                    allowed: block(b, "allowed").unwrap_or(&[]).to_vec(),
                    cost_per_battalion: literal("cost"),
                    duration: literal("duration").filter(|v| *v > 0.0).map(|v| v as u64),
                    cooldown: literal("cooldown").map(|v| v as u64),
                    modifiers: crate::military_data::stats(raw, &Default::default()),
                    blocked,
                },
            );
        }
        Ok(())
    }
    pub fn tick(&mut self, hour: u64) {
        self.active.retain(|a| a.expires > hour);
    }
    pub fn modifiers(&self, leader: &str) -> StatBlock {
        let mut result = StatBlock::new();
        for a in self.active.iter().filter(|a| a.leader == leader) {
            if let Some(d) = self.definitions.get(&a.ability) {
                for (k, v) in &d.modifiers {
                    *result.entry(k.clone()).or_default() += v;
                }
            }
        }
        result
    }
}
fn allowed(
    entries: &[Entry],
    leader: &Commander,
    group: bool,
    traits: &BTreeSet<String>,
    m: &MilitaryModifiers,
) -> Option<bool> {
    let mut unknown = false;
    for e in entries {
        let value = match e.key.as_str() {
            "is_leading_army_group" => e.atom().map(|a| group == (a == "yes")),
            "is_border_war" => e.atom().map(|a| a == "no"),
            "has_ability" => e.atom().map(|a| traits.contains(a)),
            "has_trait" => e.atom().map(|a| leader.traits.contains(a)),
            "AND" | "and" => e.block().and_then(|b| allowed(b, leader, group, traits, m)),
            "NOT" | "not" => e
                .block()
                .and_then(|b| allowed(b, leader, group, traits, m))
                .map(|v| !v),
            "OR" | "or" => e.block().and_then(|b| {
                let values: Vec<_> = b
                    .iter()
                    .map(|e| allowed(std::slice::from_ref(e), leader, group, traits, m))
                    .collect();
                if values.contains(&Some(true)) {
                    Some(true)
                } else if values.contains(&None) {
                    None
                } else {
                    Some(false)
                }
            }),
            _ => crate::land_combat::evaluate_conditions(
                std::slice::from_ref(e),
                &crate::land_combat::TacticContext {
                    country: &leader.country,
                    attacker: false,
                    phase: None,
                    modifiers: m,
                    numbers: Default::default(),
                },
            ),
        };
        match value {
            Some(false) => return Some(false),
            None => unknown = true,
            _ => {}
        }
    }
    if unknown { None } else { Some(true) }
}
impl Warfare {
    pub fn command_power_max(&self, country: &str) -> Option<f64> {
        let base = *self
            .catalogue
            .literal_defines
            .get("NDefines.NCountry.BASE_MAX_COMMAND_POWER")?;
        let m = &self.countries.get(country)?.modifiers;
        Some(
            ((base + m.value("max_command_power")) * (1.0 + m.value("max_command_power_mult")))
                .max(0.0),
        )
    }
    pub fn tick_command_power(&mut self) {
        let base = self
            .catalogue
            .literal_defines
            .get("NDefines.NCountry.BASE_COMMAND_POWER_GAIN")
            .copied();
        let caps: BTreeMap<_, _> = self
            .countries
            .keys()
            .filter_map(|c| self.command_power_max(c).map(|cap| (c.clone(), cap)))
            .collect();
        for (tag, c) in &mut self.countries {
            if let (Some(power), Some(base), Some(cap)) =
                (&mut c.command_power, base, caps.get(tag))
            {
                let gain = ((base + c.modifiers.value("command_power_gain"))
                    * (1.0 + c.modifiers.value("command_power_gain_mult")))
                .max(0.0)
                    / 24.0;
                *power = (*power + gain).clamp(0.0, *cap);
            }
        }
    }
    fn ability_leader(
        &self,
        country: &str,
        army: ArmyId,
        marshal: bool,
    ) -> Result<&Commander, String> {
        let a = self.command.own_army(country, army)?;
        let id = if marshal {
            a.group
                .and_then(|id| self.command.groups.get(&id))
                .and_then(|g| g.field_marshal.as_ref())
        } else {
            a.general.as_ref()
        };
        id.and_then(|id| self.command.commanders.get(id))
            .ok_or_else(|| "Назначьте командующего".into())
    }
    pub fn ability_cost(
        &self,
        country: &str,
        army: ArmyId,
        marshal: bool,
        id: &str,
    ) -> Result<f64, String> {
        let leader = self.ability_leader(country, army, marshal)?;
        let definition = self
            .abilities
            .definitions
            .get(id)
            .ok_or("Нет определения способности")?;
        if !definition.blocked.is_empty() {
            return Err(definition.blocked.join("; "));
        }
        if definition.duration.is_none() {
            return Err("Не импортирована конечная длительность способности".into());
        }
        let m = &self
            .countries
            .get(country)
            .ok_or("Нет состояния страны")?
            .modifiers;
        let traits = leader
            .traits
            .iter()
            .filter_map(|id| self.command.rules.traits.get(id))
            .flat_map(|t| t.abilities.iter().cloned())
            .collect();
        match allowed(&definition.allowed, leader, marshal, &traits, m) {
            Some(true) => {}
            Some(false) => return Err("Условия способности не выполнены".into()),
            None => {
                return Err(
                    "Условие способности требует ещё не поддержанного игрового контекста".into(),
                );
            }
        }
        if self
            .abilities
            .active
            .iter()
            .any(|a| a.leader == leader.id && a.ability == id)
        {
            return Err("Способность уже действует".into());
        }
        if self
            .abilities
            .cooldowns
            .get(&leader.id)
            .and_then(|c| c.get(id))
            .is_some_and(|t| *t > self.simulated_hours)
        {
            return Err("Способность восстанавливается".into());
        }
        let army_ids: BTreeSet<_> = if marshal {
            let group = self.command.armies[&army].group;
            self.command
                .armies
                .values()
                .filter(|a| a.group == group)
                .map(|a| a.id)
                .collect()
        } else {
            BTreeSet::from([army])
        };
        let battalions: usize = self
            .command
            .membership
            .iter()
            .filter(|(_, a)| army_ids.contains(a))
            .filter_map(|(id, _)| self.divisions.get(id))
            .filter(|d| !d.destroyed)
            .map(|d| {
                self.countries
                    .get(country)
                    .and_then(|c| c.templates.get(&d.template))
                    .map_or(0, |t| t.line.len() + t.support.len())
            })
            .sum();
        if battalions == 0 {
            return Err("Нет назначенных батальонов".into());
        }
        Ok(definition
            .cost_per_battalion
            .ok_or("Не импортирована цена способности")?
            * battalions as f64
            * (1.0 + m.value("command_abilities_cost")).max(0.0))
    }
    pub fn activate_ability(
        &mut self,
        country: &str,
        army: ArmyId,
        marshal: bool,
        id: &str,
    ) -> Result<f64, String> {
        let cost = self.ability_cost(country, army, marshal, id)?;
        let leader = self.ability_leader(country, army, marshal)?.id.clone();
        let d = &self.abilities.definitions[id];
        let power = self
            .countries
            .get_mut(country)
            .and_then(|c| c.command_power.as_mut())
            .ok_or("Command Power не импортирован")?;
        if *power + 1e-9 < cost {
            return Err(format!(
                "Нужно {cost:.2} Command Power; доступно {power:.2}"
            ));
        }
        *power -= cost;
        let expires = self.simulated_hours + d.duration.unwrap();
        self.abilities.active.push(ActiveAbility {
            leader: leader.clone(),
            ability: id.into(),
            expires,
        });
        self.abilities
            .cooldowns
            .entry(leader)
            .or_default()
            .insert(id.into(), expires + d.cooldown.unwrap_or(0));
        Ok(cost)
    }
    pub fn unit_ability_modifiers(&self, id: u64) -> StatBlock {
        let Some(a) = self
            .command
            .membership
            .get(&id)
            .and_then(|a| self.command.armies.get(a))
        else {
            return Default::default();
        };
        let mut m = a
            .general
            .as_ref()
            .map(|l| self.abilities.modifiers(l))
            .unwrap_or_default();
        if let Some(leader) = a
            .group
            .and_then(|id| self.command.groups.get(&id))
            .and_then(|g| g.field_marshal.as_ref())
        {
            for (k, v) in self.abilities.modifiers(leader) {
                *m.entry(k).or_default() += v;
            }
        }
        m
    }
    pub fn deploy_hq(
        &mut self,
        country: &str,
        army: ArmyId,
        template: &str,
        province: u32,
        world: &World,
        production: &mut crate::production::ProductionState,
    ) -> Result<u64, String> {
        self.command.own_army(country, army)?;
        if self.abilities.headquarters.values().any(|h| h.army == army) {
            return Err("У армии уже есть штаб".into());
        }
        let r = self
            .abilities
            .hq_rules
            .as_ref()
            .ok_or("Правила Army HQ не импортированы")?;
        let c = self.countries.get(country).ok_or("Нет состояния страны")?;
        if !r.required_dlc.is_subset(&c.modifiers.dlcs) {
            return Err("Контент штаба не включён".into());
        }
        let t = c.templates.get(template).ok_or("Нет штабного шаблона")?;
        if t.line.is_empty() || !t.line.iter().all(|b| r.unit_ids.contains(&b.unit)) {
            return Err("Шаблон не является штабным".into());
        }
        let id = self.recruit(country, template, province, world, production)?;
        self.assign_hq(country, army, id, None)?;
        Ok(id)
    }
    pub fn assign_hq(
        &mut self,
        country: &str,
        army: ArmyId,
        division: u64,
        plan: Option<u64>,
    ) -> Result<(), String> {
        self.command.own_army(country, army)?;
        let r = self
            .abilities
            .hq_rules
            .as_ref()
            .ok_or("Правила Army HQ целевой версии не импортированы")?;
        let c = self.countries.get(country).ok_or("Нет состояния страны")?;
        if !r.required_dlc.is_subset(&c.modifiers.dlcs) {
            return Err("Требуемый контент Army HQ не включён".into());
        }
        let d = self
            .divisions
            .get(&division)
            .filter(|d| d.country == country && !d.destroyed)
            .ok_or("Выберите штабную дивизию")?;
        let t = c.templates.get(&d.template).ok_or("Нет шаблона штаба")?;
        if t.line.is_empty() || !t.line.iter().all(|b| r.unit_ids.contains(&b.unit)) {
            return Err("Выбранное соединение не имеет подтверждённого штабного шаблона".into());
        }
        if plan.is_some_and(|id| {
            !self
                .plans
                .plans
                .get(&id)
                .is_some_and(|p| p.army == army && p.country == country)
        }) {
            return Err("Штаб назначается приказу своей армии".into());
        }
        if self
            .abilities
            .headquarters
            .values()
            .any(|h| h.army == army && h.division != division)
        {
            return Err("У армии уже назначен штаб".into());
        }
        for f in self.fronts.fronts.values_mut() {
            f.assigned_divisions.remove(&division);
            f.assignments.remove(&division);
        }
        for p in self.plans.plans.values_mut() {
            p.assigned_divisions.remove(&division);
            p.assignments.remove(&division);
        }
        self.abilities.headquarters.insert(
            division,
            ArmyHq {
                division,
                army,
                plan,
                automatic: true,
                distance_km: None,
            },
        );
        self.fronts.detached.insert(division);
        Ok(())
    }
    pub fn tick_headquarters(&mut self, world: &World) {
        let Some(r) = self.abilities.hq_rules.clone() else {
            return;
        };
        let Some(scale) = self.catalogue.rules.pixels_to_km else {
            return;
        };
        let ids: Vec<_> = self.abilities.headquarters.keys().copied().collect();
        for id in ids {
            let h = self.abilities.headquarters[&id].clone();
            let Some(d) = self.divisions.get(&id).filter(|d| !d.destroyed) else {
                self.abilities.headquarters.remove(&id);
                continue;
            };
            let positions: BTreeSet<_> = self
                .fronts
                .fronts
                .values()
                .filter(|f| f.army == h.army)
                .filter(|f| {
                    h.plan.is_none_or(|id| {
                        self.plans
                            .plans
                            .get(&id)
                            .is_some_and(|p| p.source_front == Some(f.id))
                    })
                })
                .flat_map(|f| f.edges.iter().map(|e| e.friendly))
                .collect();
            let distance = |p: u32| {
                positions
                    .iter()
                    .filter_map(|&f| world.geography.distance_in_pixels(p, f))
                    .map(|v| v * scale)
                    .min_by(f64::total_cmp)
            };
            let at = distance(d.province);
            self.abilities
                .headquarters
                .get_mut(&id)
                .unwrap()
                .distance_km = at;
            if self.fronts.manual.contains(&id) {
                self.abilities.headquarters.get_mut(&id).unwrap().automatic = false;
            }
            if h.automatic
                && !self.fronts.manual.contains(&id)
                && d.battle.is_none()
                && matches!(d.order, crate::military_model::DivisionOrder::Hold)
            {
                let distances = self.movement_distances(&d.country, d.province, world);
                let target = distances
                    .iter()
                    .filter_map(|(&p, &travel)| {
                        distance(p).map(|front| (p, travel + (front - r.follow_distance_km).abs()))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|p| p.0);
                if let Some(p) = target.filter(|p| *p != d.province) {
                    let tag = d.country.clone();
                    let _ = self.issue_move(&tag, &[id], p, world, false, false);
                }
            }
        }
    }
    pub fn hq_bonus(&self, division: u64) -> CommanderBonuses {
        let Some(army) = self.command.membership.get(&division) else {
            return Default::default();
        };
        let Some(r) = &self.abilities.hq_rules else {
            return Default::default();
        };
        let mut b = CommanderBonuses::default();
        if let Some(h) = self
            .abilities
            .headquarters
            .values()
            .find(|h| &h.army == army)
        {
            if h.plan.is_some_and(|id| {
                !self
                    .plans
                    .plans
                    .get(&id)
                    .is_some_and(|p| p.assigned_divisions.contains(&division))
            }) {
                return b;
            }
            if let Some(band) = h
                .distance_km
                .and_then(|d| r.distance_bands.iter().find(|b| d <= b.max_km))
            {
                b.planning_speed = band.planning_speed;
                b.planning_max = band.planning_max;
            }
        }
        b
    }
}
pub fn load(root: &Path, w: &mut Warfare) {
    for path in crate::military_data::layered_files(
        &crate::military_data::data_layers(root),
        "common/abilities",
        true,
        "txt",
    ) {
        if let Ok(s) = std::fs::read_to_string(&path) {
            if let Err(e) = w.abilities.import(&s, &path.display().to_string()) {
                w.diagnostics.push(e);
            }
        }
    }
    let path = root.join("military-command-profile.json");
    if path.is_file() {
        match std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|b| serde_json::from_slice::<CommandProfile>(&b).map_err(|e| e.to_string()))
        {
            Ok(p)
                if Some(&p.version) == w.catalogue.source_version.as_ref()
                    && p.checksum == w.catalogue.source_checksum
                    && !p.provenance.trim().is_empty() =>
            {
                if p.level_thresholds.iter().all(|n| n.is_finite() && *n > 0.0) {
                    w.command.rules.level_thresholds = p.level_thresholds;
                }
                if let Some(mut h) = p.hq.filter(|h| {
                    p.version == "1.19.3"
                        && !h.unit_ids.is_empty()
                        && h.unit_ids
                            .iter()
                            .all(|id| w.catalogue.units.contains_key(id))
                        && h.distance_bands.iter().all(|b| {
                            b.max_km.is_finite()
                                && b.max_km >= 0.0
                                && b.planning_speed.is_finite()
                                && b.planning_max.is_finite()
                        })
                        && h.follow_distance_km.is_finite()
                        && h.follow_distance_km >= 0.0
                }) {
                    h.distance_bands
                        .sort_by(|a, b| a.max_km.total_cmp(&b.max_km));
                    w.abilities.hq_rules = Some(h);
                }
            }
            Ok(_) => w.diagnostics.push(
                "Command profile version/checksum/provenance mismatch: profile not applied".into(),
            ),
            Err(e) => w.diagnostics.push(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        command::{CommandRules, Rank, TraitModifiers},
        military_model::{
            CountryArmy,
            tests::{catalogue, division, template},
        },
    };
    fn commander(id: &str, rank: Rank) -> Commander {
        Commander {
            id: id.into(),
            country: "A".into(),
            name: id.into(),
            rank,
            skill: 1,
            attack: 2,
            defense: 3,
            planning: 4,
            logistics: 2,
            experience: 0.0,
            traits: Default::default(),
            source: "authored fixture".into(),
        }
    }
    fn setup() -> (Warfare, u64) {
        let mut db = catalogue();
        db.literal_defines.extend([
            ("NDefines.NCountry.BASE_MAX_COMMAND_POWER".into(), 80.0),
            ("NDefines.NCountry.BASE_COMMAND_POWER_GAIN".into(), 0.4),
            (
                "NDefines.NMilitary.CORPS_COMMANDER_DIVISIONS_CAP".into(),
                24.0,
            ),
        ]);
        let mut w = Warfare::new(db);
        w.command.rules = CommandRules::from_catalogue(&w.catalogue);
        w.countries.insert(
            "A".into(),
            CountryArmy {
                templates: BTreeMap::from([("test".into(), template())]),
                command_power: Some(10.0),
                ..Default::default()
            },
        );
        w.divisions.insert(1, division(1, "A", 1));
        w.divisions.insert(2, division(2, "A", 1));
        let army = w.create_army("A", &[1, 2], "").unwrap();
        w.command
            .commanders
            .insert("g".into(), commander("g", Rank::General));
        w.command.set_general("A", army, Some("g")).unwrap();
        (w, army)
    }
    #[test]
    fn command_power_ability_is_atomic_expires_and_obeys_cooldown() {
        let (mut w, a) = setup();
        w.abilities.import("ability={ prepare={ type=army_leader allowed={is_leading_army_group=no is_border_war=no} cost=0.5 duration=2 cooldown=3 unit_modifiers={planning_speed=1} } unsupported={type=army_leader cost=1 duration=1 unit_modifiers={invented_effect=99}}}","authored fixture").unwrap();
        assert_eq!(w.ability_cost("A", a, false, "prepare").unwrap(), 1.0);
        w.countries.get_mut("A").unwrap().command_power = Some(0.0);
        assert!(w.activate_ability("A", a, false, "prepare").is_err());
        assert!(w.abilities.active.is_empty());
        w.countries.get_mut("A").unwrap().command_power = Some(10.0);
        w.activate_ability("A", a, false, "prepare").unwrap();
        assert_eq!(w.countries["A"].command_power, Some(9.0));
        assert_eq!(w.unit_ability_modifiers(1)["planning_speed"], 1.0);
        assert!(w.activate_ability("A", a, false, "prepare").is_err());
        w.simulated_hours = 2;
        w.abilities.tick(2);
        assert!(w.unit_ability_modifiers(1).is_empty());
        assert!(w.activate_ability("A", a, false, "prepare").is_err());
        w.simulated_hours = 5;
        w.activate_ability("A", a, false, "prepare").unwrap();
        assert!(w.activate_ability("A", a, false, "unsupported").is_err());
        let old = w.countries["A"].command_power.unwrap();
        for _ in 0..24 {
            w.tick_command_power();
        }
        assert!((w.countries["A"].command_power.unwrap() - old - 0.4).abs() < 1e-9);
    }
    #[test]
    fn marshal_shares_only_shareable_traits_and_skill_leveling_uses_pinned_rules() {
        let (mut w, a) = setup();
        let group = w.command.create_group("A", a, "").unwrap();
        let mut fm = commander("fm", Rank::FieldMarshal);
        fm.traits.insert("t".into());
        w.command.commanders.insert("fm".into(), fm);
        w.command.rules.marshal_armies_cap = Some(5);
        w.command.rules.marshal_ratio = Some(0.5);
        w.command.rules.traits.insert(
            "t".into(),
            TraitModifiers {
                general: StatBlock::from([("army_attack_factor".into(), 9.0)]),
                shared: StatBlock::from([("army_attack_factor".into(), 0.2)]),
                field_marshal: StatBlock::from([("army_defence_factor".into(), 0.1)]),
                ..Default::default()
            },
        );
        w.command.set_marshal("A", group, Some("fm")).unwrap();
        let b = w.command.bonuses(1);
        assert!((b.attack - 0.175).abs() < 1e-9);
        assert!((b.defense - 0.2125).abs() < 1e-9);
        assert!((b.breakthrough - b.defense).abs() < 1e-9);
        assert!((b.planning_max - 0.12).abs() < 1e-9);
        w.command.rules.level_thresholds = vec![1.0];
        w.command.rules.level_stat_count = Some(3);
        w.command.rules.level_weights = vec![1.0, 0.0, 0.0, 0.0];
        w.command.commanders.get_mut("g").unwrap().experience = 1.0;
        w.command.advance_levels(&mut w.rng);
        assert_eq!(w.command.commanders["g"].skill, 2);
        assert_eq!(w.command.commanders["g"].attack, 5);
        assert_eq!(w.command.commanders["g"].experience, 0.0);
    }
    #[test]
    fn physical_hq_is_optional_and_has_distance_scoped_bonus_with_manual_control() {
        let (mut world, mut w) = crate::frontline::tests::scenario();
        w.countries.insert(
            "A".into(),
            CountryArmy {
                templates: BTreeMap::from([("test".into(), template())]),
                ..Default::default()
            },
        );
        let ids: Vec<_> = (1..=11).collect();
        let a = w.create_army("A", &ids, "").unwrap();
        let mut fronts = std::mem::take(&mut w.fronts);
        let f = fronts
            .create(
                &w,
                &world,
                "A",
                a,
                crate::frontline::FrontEdge {
                    friendly: 1,
                    enemy: 9,
                },
                crate::frontline::FrontEdge {
                    friendly: 8,
                    enemy: 16,
                },
                &Default::default(),
            )
            .unwrap();
        w.fronts = fronts;
        assert!(w.assign_hq("A", a, 12, None).is_err());
        w.abilities.hq_rules = Some(HqRules {
            unit_ids: BTreeSet::from(["infantry".into()]),
            required_dlc: Default::default(),
            distance_bands: vec![HqBand {
                max_km: 2.0,
                planning_speed: 0.5,
                planning_max: 0.1,
            }],
            follow_distance_km: 1.0,
        });
        w.assign_hq("A", a, 12, None).unwrap();
        w.tick_headquarters(&world);
        assert_eq!(w.hq_bonus(1).planning_speed, 0.5);
        assert!(w.fronts.fronts[&f].assigned_divisions.contains(&1));
        w.fronts.manual.insert(12);
        w.tick_headquarters(&world);
        assert!(!w.abilities.headquarters[&12].automatic);
        // Removing the physical HQ never removes the army or its ordinary members.
        w.divisions.get_mut(&12).unwrap().destroyed = true;
        w.tick_headquarters(&world);
        assert!(w.command.armies.contains_key(&a));
        assert_eq!(w.command.members(a).count(), 11);
        world.set_province_controller(1, Some("B"));
    }
    #[test]
    fn group_front_keeps_members_in_their_armies() {
        let (world, mut w) = crate::frontline::tests::scenario();
        let a = w.create_army("A", &[1, 2, 3, 4, 5, 6], "").unwrap();
        let b = w.create_army("A", &[7, 8, 9, 10, 11, 12], "").unwrap();
        let g = w.command.create_group("A", a, "").unwrap();
        w.command.set_group("A", b, Some(g)).unwrap();
        let mut fs = std::mem::take(&mut w.fronts);
        let f = fs
            .create(
                &w,
                &world,
                "A",
                a,
                crate::frontline::FrontEdge {
                    friendly: 1,
                    enemy: 9,
                },
                crate::frontline::FrontEdge {
                    friendly: 8,
                    enemy: 16,
                },
                &Default::default(),
            )
            .unwrap();
        fs.assign_group(&w, "A", f, &Default::default()).unwrap();
        fs.tick(&mut w, &world);
        assert_eq!(fs.fronts[&f].assigned_divisions.len(), 12);
        assert_eq!(fs.fronts[&f].assignments.len(), 12);
        assert_eq!(w.command.members(a).count(), 6);
        assert_eq!(w.command.members(b).count(), 6);
    }
}
