//! Units, equipment, templates and runtime divisions. Independent from graphics.
use crate::combat_rules::CombatRules;
use crate::research_data::Entry;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub type DivisionId = u64;
pub type StatBlock = BTreeMap<String, f64>;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    pub soft_attack: f64,
    pub hard_attack: f64,
    pub defense: f64,
    pub breakthrough: f64,
    pub hp: f64,
    pub organization: f64,
    pub recovery: f64,
    pub width: f64,
    pub armor: f64,
    pub piercing: f64,
    pub hardness: f64,
    pub speed: f64,
    pub manpower: f64,
    pub supply: f64,
    pub fuel: f64,
    pub weight: f64,
    pub training: f64,
    pub recon: f64,
    pub initiative: f64,
    pub entrenchment: f64,
    pub air_attack: f64,
    pub reliability: f64,
}

impl Stats {
    pub fn from_block(b: &StatBlock) -> Self {
        let get = |k: &str| b.get(k).copied().unwrap_or(0.0);
        Self {
            soft_attack: get("soft_attack"),
            hard_attack: get("hard_attack"),
            defense: get("defense"),
            breakthrough: get("breakthrough"),
            hp: get("max_strength"),
            organization: get("max_organisation"),
            recovery: get("default_morale"),
            width: get("combat_width"),
            armor: get("armor_value"),
            piercing: get("ap_attack"),
            hardness: get("hardness"),
            speed: get("maximum_speed"),
            manpower: get("manpower"),
            supply: get("supply_consumption"),
            fuel: get("fuel_consumption"),
            weight: get("weight"),
            training: get("training_time"),
            recon: get("recon"),
            initiative: get("initiative"),
            entrenchment: get("entrenchment"),
            air_attack: get("air_attack"),
            reliability: get("reliability"),
        }
    }
    pub fn scaled_attacks(&self, factor: f64) -> Self {
        let mut s = self.clone();
        s.soft_attack *= factor;
        s.hard_attack *= factor;
        s.defense *= factor;
        s.breakthrough *= factor;
        s
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TerrainAdjustment {
    pub attack: f64,
    pub defense: f64,
    pub movement: f64,
    pub attrition: f64,
}
impl TerrainAdjustment {
    pub fn add(&mut self, other: &Self, weight: f64) {
        self.attack += other.attack * weight;
        self.defense += other.defense * weight;
        self.movement += other.movement * weight;
        self.attrition += other.attrition * weight;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquipmentDefinition {
    pub id: String,
    pub archetype: String,
    pub year: u32,
    pub active: bool,
    pub buildable: bool,
    pub stats: StatBlock,
    pub cost: f64,
    pub modular: bool,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BattalionDefinition {
    pub id: String,
    pub group: String,
    pub categories: BTreeSet<String>,
    pub active: bool,
    pub divisional: bool,
    pub regimental: bool,
    pub affects_speed: bool,
    #[serde(default)]
    pub equipment_speed_multiplier: bool,
    pub stats: StatBlock,
    pub need: BTreeMap<String, f64>,
    pub terrain: BTreeMap<String, TerrainAdjustment>,
    pub same_support_type: BTreeSet<String>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattalionSlot {
    pub unit: String,
    pub column: usize,
    pub row: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportSlot {
    pub unit: String,
    pub regiment: Option<usize>,
    pub row: usize,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DivisionTemplate {
    pub name: String,
    pub line: Vec<BattalionSlot>,
    pub support: Vec<SupportSlot>,
    pub locked: bool,
    pub icon: String,
    pub division_type: Option<String>,
    pub equipment_preferences: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TechnologyEffects {
    pub units: BTreeMap<String, StatBlock>,
    pub equipment: BTreeMap<String, StatBlock>,
    pub country: StatBlock,
    pub unlock_units: BTreeSet<String>,
    pub unlock_equipment: BTreeSet<String>,
    pub unlock_modules: BTreeSet<String>,
    pub unlock_tactics: BTreeSet<String>,
    pub conditional_unlocks: Vec<ConditionalUnlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UnlockKind {
    Unit,
    Equipment,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalUnlock {
    pub kind: UnlockKind,
    pub conditions: Vec<Entry>,
    pub ids: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainDefinition {
    pub width: f64,
    pub extra_width: f64,
    pub movement_cost: f64,
    pub attack: f64,
    pub defense: f64,
    pub attrition: f64,
}
impl Default for TerrainDefinition {
    fn default() -> Self {
        Self {
            width: 70.0,
            extra_width: 35.0,
            movement_cost: 1.0,
            attack: 0.0,
            defense: 0.0,
            attrition: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tactic {
    pub id: String,
    pub attacker: bool,
    pub active: bool,
    pub weight: f64,
    pub trigger: Vec<Entry>,
    pub weight_modifiers: Vec<(Vec<Entry>, f64)>,
    pub countered_by: Option<String>,
    pub phase: Option<String>,
    pub required_phase: Option<String>,
    pub attacker_bonus: f64,
    pub defender_bonus: f64,
    pub width_bonus: f64,
    pub attacker_org_damage: f64,
    pub defender_org_damage: f64,
    pub movement: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MilitaryCatalogue {
    pub rules: CombatRules,
    pub source_version: Option<String>,
    pub infrastructure_max: Option<u16>,
    pub source_checksum: Option<String>,
    pub literal_defines: BTreeMap<String, f64>,
    pub literal_arrays: BTreeMap<String, Vec<f64>>,
    pub localisation: BTreeMap<String, String>,
    pub units: BTreeMap<String, BattalionDefinition>,
    pub equipment: BTreeMap<String, EquipmentDefinition>,
    /// Raw data-driven hull slots and replaceable equipment modules.
    #[serde(default)]
    pub naval_designer: crate::naval_designer::NavalDesignerCatalogue,
    /// Imported per-factory material demand for each concrete equipment model.
    #[serde(default)]
    pub equipment_resources: BTreeMap<String, BTreeMap<String, u32>>,
    pub technologies: BTreeMap<String, TechnologyEffects>,
    pub terrains: BTreeMap<String, TerrainDefinition>,
    pub weather: BTreeMap<String, StatBlock>,
    pub tactics: Vec<Tactic>,
    pub diagnostics: Vec<String>,
    pub source_files: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MilitaryModifiers {
    pub country: StatBlock,
    pub unit: BTreeMap<String, StatBlock>,
    pub equipment: BTreeMap<String, StatBlock>,
    pub unlocked_units: BTreeSet<String>,
    pub unlocked_equipment: BTreeSet<String>,
    #[serde(default)]
    pub unlocked_modules: BTreeSet<String>,
    pub unlocked_tactics: BTreeSet<String>,
    pub researched: BTreeSet<String>,
    pub flags: BTreeSet<String>,
    pub dlcs: BTreeSet<String>,
    #[serde(default)]
    pub ideas: BTreeSet<String>,
}

fn merge_stats(a: &mut StatBlock, b: &StatBlock) {
    for (k, v) in b {
        *a.entry(k.clone()).or_default() += v;
    }
}
impl MilitaryModifiers {
    pub fn from_research(catalogue: &MilitaryCatalogue, known: &BTreeSet<String>) -> Self {
        let mut result = Self {
            researched: known.clone(),
            ..Default::default()
        };
        for id in known {
            if let Some(effect) = catalogue.technologies.get(id) {
                merge_stats(&mut result.country, &effect.country);
                for (k, b) in &effect.units {
                    merge_stats(result.unit.entry(k.clone()).or_default(), b);
                }
                for (k, b) in &effect.equipment {
                    merge_stats(result.equipment.entry(k.clone()).or_default(), b);
                }
                result
                    .unlocked_units
                    .extend(effect.unlock_units.iter().cloned());
                result
                    .unlocked_equipment
                    .extend(effect.unlock_equipment.iter().cloned());
                result
                    .unlocked_modules
                    .extend(effect.unlock_modules.iter().cloned());
                result
                    .unlocked_tactics
                    .extend(effect.unlock_tactics.iter().cloned());
            }
        }
        result
    }
    pub fn value(&self, key: &str) -> f64 {
        self.country.get(key).copied().unwrap_or(0.0)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DerivedTemplate {
    /// Actual equipment stats already include per-archetype shortages. None is a designer preview.
    #[serde(default)]
    pub equipment_fill: Option<f64>,
    pub fuel_stats: Stats,
    pub stats: Stats,
    pub equipment: BTreeMap<String, f64>,
    pub model_requirements: BTreeMap<String, f64>,
    pub terrain: BTreeMap<String, TerrainAdjustment>,
    pub equipment_cost: f64,
    pub contributions: Vec<String>,
}

impl MilitaryCatalogue {
    /// Shared equipment resolver for ground units, attrition and air missions.
    pub fn effective_equipment_stat(&self, id: &str, key: &str, m: &MilitaryModifiers) -> f64 {
        let Some(e) = self.equipment.get(id) else {
            return 0.;
        };
        let adjustment = |id: &str| {
            m.equipment
                .get(id)
                .and_then(|b| b.get(key))
                .copied()
                .unwrap_or(0.)
        };
        let bonus = adjustment(&e.id)
            + if e.archetype != e.id {
                adjustment(&e.archetype)
            } else {
                0.
            };
        e.stats.get(key).copied().unwrap_or(0.) * (1. + bonus).max(0.)
    }
    pub fn label<'a>(&'a self, id: &'a str) -> &'a str {
        self.localisation.get(id).map_or(id, String::as_str)
    }

    pub fn default_terrain(&self, id: &str) -> TerrainDefinition {
        self.terrains.get(id).cloned().unwrap_or_else(|| {
            let (width, extra_width) = match id {
                "forest" | "jungle" => (60.0, 30.0),
                "mountain" | "marsh" => (50.0, 25.0),
                "urban" => (80.0, 40.0),
                _ => (70.0, 35.0),
            };
            TerrainDefinition {
                width,
                extra_width,
                ..Default::default()
            }
        })
    }
    pub fn equipment_available(&self, id: &str, modifiers: &MilitaryModifiers) -> bool {
        self.equipment.get(id).is_some_and(|e| {
            e.buildable && !e.modular && (e.active || modifiers.unlocked_equipment.contains(id))
        })
    }
    pub fn best_equipment(
        &self,
        archetype: &str,
        m: &MilitaryModifiers,
    ) -> Option<&EquipmentDefinition> {
        self.equipment
            .values()
            .filter(|e| e.archetype == archetype && self.equipment_available(&e.id, m))
            .max_by(|a, b| a.year.cmp(&b.year).then_with(|| a.id.cmp(&b.id)))
    }
    pub fn validate_template(
        &self,
        t: &DivisionTemplate,
        m: &MilitaryModifiers,
        check_unlocks: bool,
    ) -> Result<(), String> {
        if t.line.is_empty() {
            return Err("A division needs at least one line battalion".into());
        }
        let mut positions = BTreeSet::new();
        let mut groups = BTreeMap::<usize, &str>::new();
        for slot in &t.line {
            let u = self
                .units
                .get(&slot.unit)
                .ok_or_else(|| format!("Unknown battalion {}", slot.unit))?;
            if u.group == "support" {
                return Err("A support company cannot occupy a line slot".into());
            }
            if slot.column >= self.rules.regiment_columns
                || slot.row >= self.rules.regiment_rows
                || !positions.insert((slot.column, slot.row))
            {
                return Err("Invalid/duplicate line position".into());
            }
            if groups
                .insert(slot.column, &u.group)
                .is_some_and(|g| g != u.group)
            {
                return Err("Incompatible battalion groups in one regiment".into());
            }
            if check_unlocks && !u.active && !m.unlocked_units.contains(&u.id) {
                return Err(format!("{} requires a technology", u.id));
            }
        }
        let mut normal = 0;
        let mut regiments = BTreeSet::new();
        let mut support_types: Vec<(Option<usize>, &BattalionDefinition)> = Vec::new();
        let mut normal_rows = BTreeSet::new();
        for slot in &t.support {
            let u = self
                .units
                .get(&slot.unit)
                .ok_or_else(|| format!("Unknown support {}", slot.unit))?;
            if u.group != "support" {
                return Err("Line battalion in support slot".into());
            }
            if let Some(column) = slot.regiment {
                if !u.regimental
                    || !groups.contains_key(&column)
                    || slot.row >= self.rules.regiment_rows
                    || !regiments.insert((column, slot.row))
                {
                    return Err("Invalid/duplicate regimental support slot".into());
                }
            } else {
                normal += 1;
                if !u.divisional
                    || normal > self.rules.support_slots
                    || slot.row >= self.rules.support_slots
                    || !normal_rows.insert(slot.row)
                {
                    return Err("Division support slots exceeded".into());
                }
            }
            // same_support_type labels a family (often the unit's own ID).
            // Compare different slots; never reject a company against itself.
            if support_types.iter().any(|(scope, other)| {
                *scope == slot.regiment
                    && (other.id == u.id
                        || other.same_support_type.contains(&u.id)
                        || u.same_support_type.contains(&other.id)
                        || !u.same_support_type.is_disjoint(&other.same_support_type))
            }) {
                return Err("Duplicate/incompatible support type".into());
            }
            support_types.push((slot.regiment, u));
            if check_unlocks && !u.active && !m.unlocked_units.contains(&u.id) {
                return Err(format!("{} requires a technology", u.id));
            }
        }
        Ok(())
    }
    pub fn edit_cost(&self, old: &DivisionTemplate, new: &DivisionTemplate) -> Result<f64, String> {
        self.edit_cost_with_modifiers(old, new, &MilitaryModifiers::default())
    }
    pub fn edit_cost_with_modifiers(
        &self,
        old: &DivisionTemplate,
        new: &DivisionTemplate,
        m: &MilitaryModifiers,
    ) -> Result<f64, String> {
        if old.locked && (old.line != new.line || old.support != new.support) {
            return Err("This template is locked".into());
        }
        self.validate_template(new, m, false)?;
        let change = |old: Vec<String>, new: Vec<String>, base: f64| {
            let mut before = BTreeMap::<String, usize>::new();
            let mut after = BTreeMap::<String, usize>::new();
            for u in old {
                *before.entry(u).or_default() += 1;
            }
            for u in new {
                *after.entry(u).or_default() += 1;
            }
            let ids: BTreeSet<_> = before.keys().chain(after.keys()).collect();
            let mut add = 0.0;
            let mut remove = 0.0;
            for id in ids {
                let a = after.get(id).copied().unwrap_or(0);
                let b = before.get(id).copied().unwrap_or(0);
                let price =
                    base * (1.0 + m.value(&format!("unit_{id}_design_cost_factor"))).max(0.0);
                add += a.saturating_sub(b) as f64 * price;
                remove += b.saturating_sub(a) as f64 * price;
            }
            add.max(remove)
        };
        let line = change(
            old.line.iter().map(|s| s.unit.clone()).collect(),
            new.line.iter().map(|s| s.unit.clone()).collect(),
            self.rules.line_edit_xp,
        );
        let support = change(
            old.support
                .iter()
                .filter(|s| s.regiment.is_none())
                .map(|s| s.unit.clone())
                .collect(),
            new.support
                .iter()
                .filter(|s| s.regiment.is_none())
                .map(|s| s.unit.clone())
                .collect(),
            self.rules.support_edit_xp,
        );
        let old_reg: Vec<_> = old
            .support
            .iter()
            .filter(|s| s.regiment.is_some())
            .map(|s| format!("{}:{}", s.regiment.unwrap(), s.unit))
            .collect();
        let new_reg: Vec<_> = new
            .support
            .iter()
            .filter(|s| s.regiment.is_some())
            .map(|s| format!("{}:{}", s.regiment.unwrap(), s.unit))
            .collect();
        let regimental = if old_reg == new_reg {
            0.0
        } else {
            change(
                old_reg,
                new_reg,
                self.rules
                    .regimental_support_edit_xp
                    .ok_or("Regimental Support edit cost was not imported for this version")?,
            )
        };
        let groups = |t: &DivisionTemplate| {
            t.line
                .iter()
                .filter_map(|s| self.units.get(&s.unit).map(|u| u.group.clone()))
                .collect::<BTreeSet<_>>()
        };
        let old_groups = groups(old);
        let new_groups = groups(new);
        let extra = new_groups
            .difference(&old_groups)
            .count()
            .saturating_sub(usize::from(old_groups.is_empty()));
        Ok(line + support + regimental + extra as f64 * self.rules.new_group_xp)
    }
    /// Equipment combat stats are averages per archetype, not multiplied by the
    /// number of rifles/tanks. Need counts determine cost and completeness.
    pub fn aggregate(
        &self,
        t: &DivisionTemplate,
        m: &MilitaryModifiers,
        inventory: Option<&BTreeMap<String, u32>>,
    ) -> Result<DerivedTemplate, String> {
        self.validate_template(t, m, false)?;
        let mut result = DerivedTemplate::default();
        let mut requirements = BTreeMap::<String, f64>::new();
        for slot in t
            .line
            .iter()
            .map(|s| &s.unit)
            .chain(t.support.iter().map(|s| &s.unit))
        {
            for (kind, n) in &self.units[slot].need {
                *requirements.entry(kind.clone()).or_default() += n;
            }
        }
        let fills: BTreeMap<_, _> = requirements
            .iter()
            .map(|(kind, need)| {
                let actual = inventory.map_or(*need, |stock| {
                    stock
                        .iter()
                        .filter_map(|(id, n)| {
                            self.equipment
                                .get(id)
                                .filter(|e| &e.archetype == kind)
                                .map(|_| f64::from(*n))
                        })
                        .sum()
                });
                (kind.clone(), (actual / need.max(1e-9)).clamp(0., 1.))
            })
            .collect();
        if inventory.is_some() && !requirements.is_empty() {
            result.equipment_fill = Some(fills.values().copied().fold(1., f64::min));
        }
        let mut reliability_sum = 0.;
        let mut reliability_count = 0.;
        let mut armor = Vec::new();
        let mut piercing = Vec::new();
        let mut hardness = 0.0;
        let mut speed = f64::INFINITY;
        let mut count = 0usize;
        let mut supply_factor = m.value("supply_consumption_factor");
        let mut fuel_factor = m.value("fuel_consumption_factor");
        for (id, is_support) in t
            .line
            .iter()
            .map(|s| (s.unit.as_str(), false))
            .chain(t.support.iter().map(|s| (s.unit.as_str(), true)))
        {
            let u = &self.units[id];
            let mut values = u.stats.clone();
            for tag in std::iter::once(&u.id).chain(u.categories.iter()) {
                if let Some(bonus) = m.unit.get(tag) {
                    merge_stats(&mut values, bonus);
                }
            }
            let mut s = Stats::from_block(&values);
            let mut eq = StatBlock::new();
            let mut equipment_speed: f64 = 0.0;
            for (archetype, need) in &u.need {
                *result.equipment.entry(archetype.clone()).or_default() += *need;
                let selected: Vec<(&EquipmentDefinition, f64)> = if let Some(inventory) = inventory
                {
                    let total: u64 = inventory
                        .iter()
                        .filter_map(|(id, n)| {
                            self.equipment
                                .get(id)
                                .filter(|e| &e.archetype == archetype)
                                .map(|_| u64::from(*n))
                        })
                        .sum();
                    inventory
                        .iter()
                        .filter_map(|(id, n)| {
                            self.equipment
                                .get(id)
                                .filter(|e| &e.archetype == archetype && total > 0)
                                .map(|e| (e, f64::from(*n) / total as f64))
                        })
                        .collect()
                } else {
                    t.equipment_preferences
                        .get(archetype)
                        .and_then(|id| self.equipment.get(id))
                        .filter(|e| e.archetype == *archetype && self.equipment_available(&e.id, m))
                        .or_else(|| self.best_equipment(archetype, m))
                        .map(|e| vec![(e, 1.0)])
                        .unwrap_or_default()
                };
                if selected.is_empty() && inventory.is_none() {
                    return Err(format!("No usable equipment for {archetype}"));
                }
                let archetype_speed: f64 = selected
                    .iter()
                    .map(|(e, w)| self.effective_equipment_stat(&e.id, "maximum_speed", m) * w)
                    .sum();
                equipment_speed = equipment_speed.max(archetype_speed);
                for (e, weight) in selected {
                    let reliability = self.effective_equipment_stat(&e.id, "reliability", m);
                    reliability_sum += need * weight * reliability;
                    reliability_count += need * weight;
                    result.equipment_cost += need * e.cost * weight;
                    *result.model_requirements.entry(e.id.clone()).or_default() += need * weight;
                    for k in e.stats.keys() {
                        *eq.entry(k.clone()).or_default() += self
                            .effective_equipment_stat(&e.id, k, m)
                            * weight
                            * fills.get(archetype).copied().unwrap_or(1.);
                    }
                }
            }
            if !u.need.is_empty() {
                // Unit attack/armor entries are equipment multipliers. Org/HP,
                // manpower and utility entries remain absolute unit values.
                let adjusted = |key: &str| {
                    eq.get(key).copied().unwrap_or(0.0)
                        * (1.0 + values.get(key).copied().unwrap_or(0.0)).max(0.0)
                };
                s.soft_attack = adjusted("soft_attack");
                s.hard_attack = adjusted("hard_attack");
                s.defense = adjusted("defense");
                s.breakthrough = adjusted("breakthrough");
                s.armor = adjusted("armor_value");
                s.piercing = adjusted("ap_attack");
                s.air_attack = adjusted("air_attack");
                let equipped = u
                    .need
                    .keys()
                    .filter_map(|kind| fills.get(kind))
                    .copied()
                    .fold(1., f64::min);
                s.hardness = (s.hardness * equipped + eq.get("hardness").copied().unwrap_or(0.0))
                    .clamp(0.0, 1.0);
                if equipment_speed > 0.0 {
                    s.speed = if u.equipment_speed_multiplier {
                        equipment_speed
                            * (1.0 + values.get("maximum_speed").copied().unwrap_or(0.0)).max(0.0)
                    } else if s.speed > 0.0 {
                        s.speed.min(equipment_speed)
                    } else {
                        equipment_speed
                    };
                }
                if u.equipment_speed_multiplier {
                    s.fuel = adjusted("fuel_consumption")
                        * values
                            .get("own_equipment_fuel_consumption_mult")
                            .copied()
                            .unwrap_or(1.0)
                            .max(0.0);
                }
                s.reliability = eq.get("reliability").copied().unwrap_or(0.0);
            }
            supply_factor += values
                .get("supply_consumption_factor")
                .copied()
                .unwrap_or(0.0);
            fuel_factor += values
                .get("fuel_consumption_factor")
                .copied()
                .unwrap_or(0.0);
            if s.fuel > 0.0 {
                result.fuel_stats.soft_attack += s.soft_attack;
                result.fuel_stats.hard_attack += s.hard_attack;
                result.fuel_stats.defense += s.defense;
                result.fuel_stats.breakthrough += s.breakthrough;
            }
            let a = &mut result.stats;
            a.soft_attack += s.soft_attack;
            a.hard_attack += s.hard_attack;
            a.defense += s.defense;
            a.breakthrough += s.breakthrough;
            a.hp += s.hp;
            a.organization += s.organization;
            a.recovery += s.recovery;
            a.manpower += s.manpower;
            a.supply += s.supply;
            a.fuel += s.fuel;
            a.weight += s.weight;
            a.training = a.training.max(s.training);
            a.recon = a.recon.max(s.recon);
            a.initiative += s.initiative;
            a.entrenchment += s.entrenchment;
            a.air_attack += s.air_attack;
            if !is_support {
                a.width += s.width;
                hardness += s.hardness;
            }
            if u.affects_speed && s.speed > 0.0 {
                speed = speed.min(s.speed);
            }
            armor.push(s.armor);
            piercing.push(s.piercing);
            count += 1;
            let terrain_weight = if is_support {
                1.0
            } else {
                1.0 / t.line.len() as f64
            };
            for (k, v) in &u.terrain {
                result
                    .terrain
                    .entry(k.clone())
                    .or_default()
                    .add(v, terrain_weight);
            }
            result
                .contributions
                .push(format!("{} ({})", u.id, u.source));
        }
        let a = &mut result.stats;
        a.organization /= count as f64;
        a.recovery /= count as f64;
        a.hardness = hardness / t.line.len() as f64;
        let combine = |v: &[f64], w: f64| {
            w * v.iter().copied().fold(0.0, f64::max)
                + (1.0 - w) * v.iter().sum::<f64>() / v.len() as f64
        };
        a.armor = combine(&armor, self.rules.armor_max_weight);
        a.piercing = combine(&piercing, self.rules.piercing_max_weight);
        a.reliability = if reliability_count > 0. {
            reliability_sum / reliability_count
        } else {
            0.
        };
        a.speed = if speed.is_finite() { speed } else { 0.0 };
        a.supply *= (1.0 + supply_factor).max(0.0);
        a.fuel *= (1.0 + fuel_factor).max(0.0);
        a.organization *= (1.0 + m.value("army_org_factor")).max(0.0);
        a.soft_attack *= (1.0 + m.value("army_attack_factor")).max(0.0);
        a.hard_attack *= (1.0 + m.value("army_attack_factor")).max(0.0);
        a.defense *= (1.0 + m.value("army_defence_factor")).max(0.0);
        a.breakthrough *= (1.0 + m.value("army_defence_factor")).max(0.0)
            * (1.0 + m.value("breakthrough_factor")).max(0.0);
        result.fuel_stats.soft_attack *= (1.0 + m.value("army_attack_factor")).max(0.0);
        result.fuel_stats.hard_attack *= (1.0 + m.value("army_attack_factor")).max(0.0);
        result.fuel_stats.defense *= (1.0 + m.value("army_defence_factor")).max(0.0);
        result.fuel_stats.breakthrough *= (1.0 + m.value("army_defence_factor")).max(0.0)
            * (1.0 + m.value("breakthrough_factor")).max(0.0);
        Ok(result)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExperienceLevel {
    Green,
    Trained,
    Regular,
    Seasoned,
    Veteran,
}
impl ExperienceLevel {
    pub fn bonus_with_factor(self, factor: f64) -> f64 {
        let tier = match self {
            Self::Green => -1.,
            Self::Trained => 0.,
            Self::Regular => 1.,
            Self::Seasoned => 2.,
            Self::Veteran => 3.,
        };
        tier * factor
    }
    pub fn bonus(self) -> f64 {
        self.bonus_with_factor(0.25)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DivisionOrder {
    Hold,
    Move { path: Vec<u32>, progress_km: f64 },
    Attack { province: u32 },
    SupportAttack { province: u32 },
    Retreat { destination: u32, progress_km: f64 },
    StrategicRedeployment { path: Vec<u32>, progress_km: f64 },
    Train { hours: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveObjective {
    pub province: u32,
    pub strategic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Division {
    pub id: DivisionId,
    pub name: String,
    pub country: String,
    pub template: String,
    pub province: u32,
    pub derived: DerivedTemplate,
    pub hp: f64,
    pub organization: f64,
    pub manpower: u32,
    pub equipment: BTreeMap<String, u32>,
    pub equipment_loss_carry: BTreeMap<String, f64>,
    pub manpower_loss_carry: f64,
    pub experience: ExperienceLevel,
    pub entrenchment: f64,
    pub planning: f64,
    pub supply_fraction: f64,
    pub fuel_fraction: f64,
    pub command: crate::command::CommanderBonuses,
    pub ability_modifiers: StatBlock,
    pub commander_attack: f64,
    pub commander_defense: f64,
    pub reinforce_bonus: f64,
    pub coordination: f64,
    pub order: DivisionOrder,
    pub destination: Option<MoveObjective>,
    pub waypoints: VecDeque<MoveObjective>,
    pub attack_delay: u32,
    pub battle: Option<u64>,
    pub destroyed: bool,
    pub recent_losses: u64,
}
impl Division {
    pub fn fighting(&self) -> bool {
        !self.destroyed && self.hp > 1e-9 && self.organization > 1e-9
    }
    pub fn hitpoint_fraction(&self) -> f64 {
        if self.derived.stats.hp > 0. {
            (self.hp / self.derived.stats.hp).clamp(0., 1.)
        } else {
            0.
        }
    }
    pub fn strength(&self) -> f64 {
        let hp = self.hitpoint_fraction();
        match self.derived.equipment_fill {
            Some(fill) => hp
                .min(fill)
                .min(f64::from(self.manpower) / self.derived.stats.manpower.max(1.)),
            None => hp,
        }
    }
    /// Do not apply the same material loss twice: actual component stats already
    /// contain shortages; only a further personnel/HP bottleneck scales them.
    pub fn combat_strength(&self) -> f64 {
        match self.derived.equipment_fill {
            Some(fill) => (self
                .hitpoint_fraction()
                .min(f64::from(self.manpower) / self.derived.stats.manpower.max(1.))
                / fill.max(1e-9))
            .clamp(0., 1.),
            None => self.hitpoint_fraction(),
        }
    }
    pub fn apply_damage(&mut self, hp: f64, org: f64, rules: &CombatRules) -> (u32, u32) {
        let hp = hp.max(0.0).min(self.hp);
        self.hp = (self.hp - hp).max(0.0);
        self.organization = (self.organization - org.max(0.0)).max(0.0);
        let fraction = hp / self.derived.stats.hp.max(1e-9) * rules.casualty_factor;
        self.manpower_loss_carry += self.derived.stats.manpower * fraction;
        let men = ((self.manpower_loss_carry + 1e-9).floor() as u32).min(self.manpower);
        self.manpower -= men;
        self.manpower_loss_carry = (self.manpower_loss_carry - f64::from(men)).max(0.0);
        let mut lost = 0;
        for (id, quantity) in &mut self.equipment {
            let carry = self.equipment_loss_carry.entry(id.clone()).or_default();
            *carry += self
                .derived
                .model_requirements
                .get(id)
                .copied()
                .unwrap_or(f64::from(*quantity))
                * fraction;
            let n = (*carry + 1e-9).floor().max(0.0) as u32;
            let n = n.min(*quantity);
            *quantity -= n;
            *carry = (*carry - f64::from(n)).max(0.0);
            lost += n;
        }
        self.recent_losses += u64::from(men);
        if self.hp <= 1e-9 {
            self.destroyed = true;
        }
        (men, lost)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CountryArmy {
    pub templates: BTreeMap<String, DivisionTemplate>,
    pub manpower_pool: u64,
    #[serde(default)]
    pub core_population: u64,
    #[serde(default)]
    pub recruitable_total: u64,
    /// Target from current conscription law; None only on initial import / legacy save.
    #[serde(default)]
    pub mobilization_target: Option<u64>,
    /// Fractional mobilization carried between daily ticks for small nations.
    #[serde(default)]
    pub mobilization_carry: f64,
    pub army_xp: f64,
    pub command_power: Option<f64>,
    pub modifiers: MilitaryModifiers,
    pub base_modifiers: StatBlock,
    /// The portion of base_modifiers contributed by starting ideas/laws.
    /// Runtime research/script state may replace it without applying bonuses twice.
    #[serde(default)]
    pub idea_modifiers: StatBlock,
    pub history_expected: usize,
    pub history_imported: usize,
    pub history_source: Option<String>,
    pub history_oobs: Vec<String>,
    pub history_errors: Vec<String>,
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    #[test]
    fn support_family_does_not_conflict_with_itself_but_rejects_another_member() {
        let mut c = catalogue();
        c.units
            .get_mut("engineer")
            .unwrap()
            .same_support_type
            .insert("engineer".into());
        let mut t = template();
        t.support.push(SupportSlot {
            unit: "engineer".into(),
            regiment: None,
            row: 0,
        });
        assert!(c.validate_template(&t, &Default::default(), false).is_ok());
        let mut second = c.units["engineer"].clone();
        second.id = "engineer_variant".into();
        c.units.insert(second.id.clone(), second);
        t.support.push(SupportSlot {
            unit: "engineer_variant".into(),
            regiment: None,
            row: 1,
        });
        assert!(c.validate_template(&t, &Default::default(), false).is_err());
        let mut a = c.units["engineer"].clone();
        a.id = "mot_recon".into();
        a.same_support_type = BTreeSet::from(["recon".into()]);
        let mut b = a.clone();
        b.id = "armored_recon".into();
        c.units.insert(a.id.clone(), a);
        c.units.insert(b.id.clone(), b);
        t.support = vec![
            SupportSlot {
                unit: "mot_recon".into(),
                regiment: None,
                row: 0,
            },
            SupportSlot {
                unit: "armored_recon".into(),
                regiment: None,
                row: 1,
            },
        ];
        assert!(c.validate_template(&t, &Default::default(), false).is_err());
    }
    #[test]
    fn support_replacement_costs_one_edit_and_duplicate_slots_are_rejected() {
        let mut c = catalogue();
        let mut e = c.units["engineer"].clone();
        e.id = "signal".into();
        c.units.insert(e.id.clone(), e);
        let mut a = template();
        a.support.push(SupportSlot {
            unit: "engineer".into(),
            regiment: None,
            row: 0,
        });
        let mut b = a.clone();
        b.support[0].unit = "signal".into();
        assert_eq!(c.edit_cost(&a, &b).unwrap(), 10.0);
        b.support.push(SupportSlot {
            unit: "engineer".into(),
            regiment: None,
            row: 0,
        });
        assert!(
            c.validate_template(&b, &MilitaryModifiers::default(), false)
                .is_err()
        );
    }
    #[test]
    fn repeated_regimental_support_is_scoped_to_its_regiment() {
        let mut c = catalogue();
        c.units.get_mut("engineer").unwrap().regimental = true;
        let mut t = template();
        t.line.push(BattalionSlot {
            unit: "infantry".into(),
            column: 1,
            row: 0,
        });
        for column in 0..2 {
            t.support.push(SupportSlot {
                unit: "engineer".into(),
                regiment: Some(column),
                row: 0,
            });
        }
        assert!(
            c.validate_template(&t, &MilitaryModifiers::default(), false)
                .is_ok()
        );
    }
    #[test]
    fn equipment_casualties_preserve_fractional_losses_across_hits() {
        let mut a = division(1, "A", 1);
        a.equipment.insert("rifle".into(), 100);
        a.derived.model_requirements.insert("rifle".into(), 100.0);
        let mut b = a.clone();
        let r = CombatRules::default();
        a.apply_damage(5.0, 0.0, &r);
        for _ in 0..500 {
            b.apply_damage(0.01, 0.0, &r);
        }
        assert_eq!(a.equipment, b.equipment);
        assert_eq!(a.manpower, b.manpower);
        assert!((a.hp - b.hp).abs() < 1e-9);
    }
    pub fn catalogue() -> MilitaryCatalogue {
        let mut c = MilitaryCatalogue::default();
        for (id, group, hp, org, width, hardness) in [
            ("infantry", "infantry", 25.0, 60.0, 2.0, 0.0),
            ("tank", "armor", 2.0, 20.0, 2.0, 0.8),
            ("engineer", "support", 2.0, 20.0, 0.0, 0.0),
        ] {
            c.units.insert(
                id.into(),
                BattalionDefinition {
                    id: id.into(),
                    group: group.into(),
                    categories: BTreeSet::new(),
                    active: true,
                    divisional: true,
                    regimental: false,
                    affects_speed: group != "support",
                    equipment_speed_multiplier: false,
                    stats: StatBlock::from([
                        ("max_strength".into(), hp),
                        ("max_organisation".into(), org),
                        ("combat_width".into(), width),
                        ("hardness".into(), hardness),
                        ("manpower".into(), 1000.0),
                        ("maximum_speed".into(), 4.0),
                        ("soft_attack".into(), 10.0),
                        ("armor_value".into(), if id == "tank" { 100.0 } else { 0.0 }),
                    ]),
                    need: BTreeMap::new(),
                    terrain: BTreeMap::new(),
                    same_support_type: BTreeSet::new(),
                    source: "test".into(),
                },
            );
        }
        c
    }
    pub fn template() -> DivisionTemplate {
        DivisionTemplate {
            name: "test".into(),
            line: vec![BattalionSlot {
                unit: "infantry".into(),
                column: 0,
                row: 0,
            }],
            ..Default::default()
        }
    }
    pub fn division(id: u64, country: &str, province: u32) -> Division {
        let c = catalogue();
        let t = template();
        let derived = c
            .aggregate(&t, &MilitaryModifiers::default(), None)
            .unwrap();
        Division {
            id,
            name: format!("Unit {id}"),
            country: country.into(),
            template: "test".into(),
            province,
            hp: derived.stats.hp,
            organization: derived.stats.organization,
            manpower: 1000,
            derived,
            equipment: BTreeMap::new(),
            equipment_loss_carry: BTreeMap::new(),
            manpower_loss_carry: 0.0,
            experience: ExperienceLevel::Trained,
            entrenchment: 0.0,
            planning: 0.0,
            supply_fraction: 1.0,
            fuel_fraction: 1.0,
            command: Default::default(),
            ability_modifiers: Default::default(),
            commander_attack: 0.0,
            commander_defense: 0.0,
            reinforce_bonus: 0.0,
            coordination: 0.0,
            order: DivisionOrder::Hold,
            destination: None,
            waypoints: VecDeque::new(),
            attack_delay: 0,
            battle: None,
            destroyed: false,
            recent_losses: 0,
        }
    }
    #[test]
    fn aggregates_sum_mean_peak_and_line_only_hardness() {
        let c = catalogue();
        let mut t = template();
        t.line.push(BattalionSlot {
            unit: "tank".into(),
            column: 1,
            row: 0,
        });
        t.support.push(SupportSlot {
            unit: "engineer".into(),
            regiment: None,
            row: 0,
        });
        let a = c
            .aggregate(&t, &MilitaryModifiers::default(), None)
            .unwrap();
        assert_eq!(a.stats.hp, 29.0);
        assert_eq!(a.stats.width, 4.0);
        assert_eq!(a.stats.hardness, 0.4);
        assert!((a.stats.organization - 100.0 / 3.0).abs() < 1e-9);
        assert_eq!(a.stats.armor, 60.0);
    }
    #[test]
    fn designer_cost_and_regiment_group_validation() {
        let c = catalogue();
        let old = template();
        let mut new = old.clone();
        new.line.push(BattalionSlot {
            unit: "tank".into(),
            column: 1,
            row: 0,
        });
        assert_eq!(c.edit_cost(&old, &new).unwrap(), 25.0);
        new.line[1].column = 0;
        new.line[1].row = 1;
        assert!(
            c.validate_template(&new, &MilitaryModifiers::default(), false)
                .is_err()
        );
    }
    #[test]
    fn fractional_casualties_accumulate_and_do_not_create_equipment() {
        let mut d = division(1, "AAA", 1);
        d.equipment.insert("rifle".into(), 100);
        let mut men = 0;
        for _ in 0..100 {
            men += d.apply_damage(0.01, 0.0, &CombatRules::default()).0;
        }
        assert_eq!(men, 28);
        assert_eq!(d.manpower, 972);
        assert!(d.equipment["rifle"] < 100);
    }
}
