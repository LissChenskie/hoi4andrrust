//! Hourly, stochastic land battle. Both sides fire from the same hour snapshot.
//! Exact Clausewitz RNG/counter selection/rounding parity requires game fixtures.
use crate::combat_modifiers::{CombatFactors as Factors, CombatStat, ModifierTrace};
use crate::combat_rules::{CombatRandom, CombatRules};
use crate::military_model::*;
use crate::research_data::Entry;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BattleOutcome {
    AttackerWon,
    DefenderWon,
    Draw,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BattleSide {
    pub active: Vec<DivisionId>,
    pub reserves: Vec<DivisionId>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AirSupport {
    pub ground_support_bonus: f64,
    pub enemy_superiority: f64,
    /// Upstream air subsystem supplies resolved plane attack rolls. This core
    /// applies land damage coefficients, width cap and AA damage reduction.
    pub cas_planes: u32,
    pub plane_attack_rolls: f64,
    pub aa_damage_reduction: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BattleEnvironment {
    pub terrain_id: String,
    pub terrain: TerrainDefinition,
    pub weather: StatBlock,
    pub night: bool,
    pub fort_level: u16,
    pub crossing_penalties: BTreeMap<DivisionId, f64>,
    pub attacker_air: AirSupport,
    pub defender_air: AirSupport,
}
impl Default for BattleEnvironment {
    fn default() -> Self {
        Self {
            terrain_id: "plains".into(),
            terrain: TerrainDefinition::default(),
            weather: StatBlock::new(),
            night: false,
            fort_level: 0,
            crossing_penalties: BTreeMap::new(),
            attacker_air: AirSupport::default(),
            defender_air: AirSupport::default(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DamageRoll {
    pub protected: bool,
    pub hp_die: u32,
    pub org_die: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackTrace {
    pub from: u64,
    pub to: u64,
    pub effective_attack: f64,
    pub rolls: u32,
    pub target_defensive_rolls: u32,
    pub protected: u32,
    pub protected_hits: u32,
    pub overflow_hits: u32,
    pub armor_factor: f64,
    pub hp_damage: f64,
    pub org_damage: f64,
    pub modifiers: Vec<ModifierTrace>,
    pub damage_rolls: Vec<DamageRoll>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HourTrace {
    pub hour: u32,
    pub rng_start: Option<u64>,
    pub rng_end: Option<u64>,
    pub width: f64,
    pub attacker_width: f64,
    pub attacker_over_width_penalty: f64,
    pub defender_over_width_penalty: f64,
    pub attacker_stacking_penalty: f64,
    pub defender_stacking_penalty: f64,
    pub attacker_frontline: Vec<u64>,
    pub defender_frontline: Vec<u64>,
    pub attacker_reserves: Vec<u64>,
    pub defender_reserves: Vec<u64>,
    pub defender_width: f64,
    pub attacker_tactic: String,
    pub defender_tactic: String,
    pub tactic_changed: bool,
    pub reinforced: Vec<u64>,
    pub exited: Vec<u64>,
    pub attacks: Vec<AttackTrace>,
    pub manpower_losses: u32,
    pub equipment_losses: u32,
    pub outcome: Option<BattleOutcome>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LandBattle {
    pub id: u64,
    pub province: u32,
    pub attackers: BattleSide,
    pub defenders: BattleSide,
    pub origins: BTreeMap<DivisionId, u32>,
    pub elapsed_hours: u32,
    pub phase: Option<String>,
    pub attacker_tactic: Option<String>,
    pub defender_tactic: Option<String>,
    pub outcome: Option<BattleOutcome>,
    pending_outcome: Option<BattleOutcome>,
    pub history: VecDeque<HourTrace>,
    pub trace_enabled: bool,
}

pub struct TacticContext<'a> {
    pub country: &'a str,
    pub attacker: bool,
    pub phase: Option<&'a str>,
    pub modifiers: &'a MilitaryModifiers,
    pub numbers: BTreeMap<String, f64>,
}

fn compare(actual: f64, operator: &str, wanted: f64) -> bool {
    match operator {
        ">" => actual > wanted,
        "<" => actual < wanted,
        ">=" => actual >= wanted,
        "<=" => actual <= wanted,
        "!=" => actual != wanted,
        _ => (actual - wanted).abs() < 1e-9,
    }
}
/// Three-valued evaluation: NOT(unsupported) stays unsupported, never true.
pub fn evaluate_conditions(items: &[Entry], context: &TacticContext<'_>) -> Option<bool> {
    fn all(values: Vec<Option<bool>>) -> Option<bool> {
        if values.contains(&Some(false)) {
            Some(false)
        } else if values.contains(&None) {
            None
        } else {
            Some(true)
        }
    }
    let mut results = Vec::new();
    for item in items {
        let atom = item.atom();
        let value = match item.key.as_str() {
            "always" => atom.map(|v| v == "yes"),
            "is_attacker" => atom.map(|v| context.attacker == (v == "yes")),
            "phase" => atom.map(|v| {
                if v == "no" {
                    context.phase.is_none()
                } else {
                    context.phase == Some(v)
                }
            }),
            "tag" => atom.map(|v| context.country == v),
            "has_tech" | "has_technology" => atom.map(|v| context.modifiers.researched.contains(v)),
            "has_country_flag" => atom.map(|v| context.modifiers.flags.contains(v)),
            "has_dlc" => atom.map(|v| context.modifiers.dlcs.contains(v)),
            "AND" | "and" => item.block().and_then(|v| evaluate_conditions(v, context)),
            "OR" | "or" => item.block().and_then(|v| {
                let values: Vec<_> = v
                    .iter()
                    .map(|e| evaluate_conditions(std::slice::from_ref(e), context))
                    .collect();
                if values.contains(&Some(true)) {
                    Some(true)
                } else if values.contains(&None) {
                    None
                } else {
                    Some(false)
                }
            }),
            "NOT" | "not" => item
                .block()
                .and_then(|v| evaluate_conditions(v, context))
                .map(|v| !v),
            key => context.numbers.get(key).and_then(|actual| {
                atom.and_then(|v| v.parse::<f64>().ok())
                    .map(|v| compare(*actual, &item.operator, v))
            }),
        };
        results.push(value);
    }
    all(results)
}

impl LandBattle {
    pub fn new(
        id: u64,
        province: u32,
        attacking: Vec<u64>,
        defending: Vec<u64>,
        units: &BTreeMap<u64, Division>,
        environment: &BattleEnvironment,
        rules: &CombatRules,
    ) -> Self {
        let origins = attacking
            .iter()
            .filter_map(|id| units.get(id).map(|d| (*id, d.province)))
            .collect();
        let mut battle = Self {
            id,
            province,
            attackers: BattleSide::default(),
            defenders: BattleSide::default(),
            origins,
            elapsed_hours: 0,
            phase: None,
            attacker_tactic: None,
            defender_tactic: None,
            outcome: None,
            pending_outcome: None,
            history: VecDeque::new(),
            trace_enabled: false,
        };
        let width = battle.base_width(environment);
        battle.attackers = Self::initial_side(attacking, units, width, rules);
        battle.defenders = Self::initial_side(defending, units, width, rules);
        battle
    }
    fn initial_side(
        ids: Vec<u64>,
        units: &BTreeMap<u64, Division>,
        width: f64,
        rules: &CombatRules,
    ) -> BattleSide {
        let mut result = BattleSide::default();
        let mut used = 0.0;
        let mut seen = BTreeSet::new();
        for id in ids {
            if !seen.insert(id) {
                continue;
            }
            let Some(unit) = units.get(&id).filter(|d| d.fighting()) else {
                continue;
            };
            let w = unit.derived.stats.width;
            if rules.can_fit(used, w, width) {
                result.active.push(id);
                used += w;
            } else {
                result.reserves.push(id);
            }
        }
        result
    }
    pub fn directions(&self) -> usize {
        self.origins
            .values()
            .copied()
            .collect::<BTreeSet<_>>()
            .len()
            .max(1)
    }
    pub fn base_width(&self, env: &BattleEnvironment) -> f64 {
        (env.terrain.width + env.terrain.extra_width * self.directions().saturating_sub(1) as f64)
            .max(1.0)
    }
    pub fn participants(&self) -> impl Iterator<Item = u64> + '_ {
        self.attackers
            .active
            .iter()
            .chain(&self.attackers.reserves)
            .chain(&self.defenders.active)
            .chain(&self.defenders.reserves)
            .copied()
    }
    fn used(side: &BattleSide, units: &BTreeMap<u64, Division>) -> f64 {
        side.active
            .iter()
            .filter_map(|id| units.get(id))
            .map(|d| d.derived.stats.width)
            .sum()
    }
    pub fn add_attacker(&mut self, id: u64, origin: u32) {
        if !self.participants().any(|v| v == id) {
            self.attackers.reserves.push(id);
            self.origins.insert(id, origin);
        }
    }
    pub fn add_defender(&mut self, id: u64) {
        if !self.participants().any(|v| v == id) {
            self.defenders.reserves.push(id);
        }
    }
    pub fn withdraw(&mut self, id: u64, rules: &CombatRules) -> Result<(), String> {
        if self.elapsed_hours < rules.minimum_hours {
            return Err(format!(
                "Cannot withdraw before {} combat hours",
                rules.minimum_hours
            ));
        }
        for side in [&mut self.attackers, &mut self.defenders] {
            side.active.retain(|v| *v != id);
            side.reserves.retain(|v| *v != id);
        }
        self.origins.remove(&id);
        Ok(())
    }

    fn choose_tactic<R: CombatRandom>(
        catalogue: &MilitaryCatalogue,
        side: &BattleSide,
        enemy: &BattleSide,
        attacker: bool,
        units: &BTreeMap<u64, Division>,
        country_modifiers: &BTreeMap<String, MilitaryModifiers>,
        phase: Option<&str>,
        rng: &mut R,
    ) -> Option<String> {
        let first = side.active.first().and_then(|id| units.get(id))?;
        let empty = MilitaryModifiers::default();
        let modifiers = country_modifiers.get(&first.country).unwrap_or(&empty);
        let average = |ids: &[u64], stat: fn(&Division) -> f64| {
            let n = ids.len().max(1) as f64;
            ids.iter()
                .filter_map(|id| units.get(id))
                .map(stat)
                .sum::<f64>()
                / n
        };
        let ctx = TacticContext {
            country: &first.country,
            attacker,
            phase,
            modifiers,
            numbers: BTreeMap::from([
                (
                    "hardness".into(),
                    average(&side.active, |d| d.derived.stats.hardness),
                ),
                (
                    "enemy_hardness".into(),
                    average(&enemy.active, |d| d.derived.stats.hardness),
                ),
                (
                    "recon".into(),
                    average(&side.active, |d| d.derived.stats.recon),
                ),
                ("num_units".into(), side.active.len() as f64),
            ]),
        };
        let choices: Vec<_> = catalogue
            .tactics
            .iter()
            .filter(|t| {
                t.attacker == attacker
                    && (t.active || modifiers.unlocked_tactics.contains(&t.id))
                    && t.required_phase.as_deref().is_none_or(|p| Some(p) == phase)
                    && evaluate_conditions(&t.trigger, &ctx) == Some(true)
            })
            .filter_map(|t| {
                let mut weight = t.weight;
                for (condition, factor) in &t.weight_modifiers {
                    match evaluate_conditions(condition, &ctx) {
                        Some(true) => weight *= factor,
                        Some(false) => {}
                        None => return None,
                    }
                }
                (weight > 0.0 && weight.is_finite()).then_some((t, weight))
            })
            .collect();
        let total = choices.iter().map(|(_, w)| w).sum::<f64>();
        if total <= 0.0 {
            return None;
        }
        let mut roll = rng.unit() * total;
        for (t, w) in &choices {
            if roll < *w {
                return Some(t.id.clone());
            }
            roll -= w;
        }
        choices.last().map(|(t, _)| t.id.clone())
    }

    fn factors(
        &self,
        division: &Division,
        attacker: bool,
        side: &BattleSide,
        units: &BTreeMap<u64, Division>,
        env: &BattleEnvironment,
        rules: &CombatRules,
        width: f64,
        tactic_bonus: f64,
    ) -> Factors {
        let mut f = Factors::identity();
        let stats = &division.derived.stats;
        let dig = if attacker {
            division.planning
        } else {
            division.entrenchment * rules.entrenchment_bonus
        };
        f.multiply(
            "experience",
            1.0 + division.experience.bonus(),
            1.0 + division.experience.bonus(),
        );
        f.multiply(
            if attacker { "planning" } else { "entrenchment" },
            1.0 + dig,
            1.0 + dig,
        );
        f.multiply(
            "commander",
            1.0 + division.commander_attack + division.command.attack,
            1.0,
        );
        f.apply(
            "commander",
            CombatStat::Defense,
            1.0 + division.commander_defense + division.command.defense,
        );
        f.apply(
            "commander",
            CombatStat::Breakthrough,
            1.0 + division.commander_defense + division.command.breakthrough,
        );
        let terrain = division
            .derived
            .terrain
            .get(&env.terrain_id)
            .cloned()
            .unwrap_or_default();
        f.multiply(
            "terrain",
            1.0 + if attacker { env.terrain.attack } else { 0.0 } + terrain.attack,
            1.0 + if attacker {
                env.terrain.attack
            } else {
                env.terrain.defense
            } + terrain.defense,
        );
        let supply_lack = 1.0 - division.supply_fraction.clamp(0.0, 1.0);
        f.multiply(
            "supply",
            1.0 + supply_lack
                * if attacker {
                    rules.supply_attacker_attack
                } else {
                    rules.supply_defender_attack
                },
            1.0 + supply_lack
                * if attacker {
                    rules.supply_attacker_defense
                } else {
                    rules.supply_defender_defense
                },
        );
        if attacker {
            let river = env
                .crossing_penalties
                .get(&division.id)
                .copied()
                .unwrap_or(0.0);
            let river_adjustment = division
                .derived
                .terrain
                .get("river")
                .cloned()
                .unwrap_or_default();
            f.multiply(
                "river",
                1.0 + river
                    + if river != 0.0 {
                        river_adjustment.attack
                    } else {
                        0.0
                    },
                1.0 + river
                    + if river != 0.0 {
                        river_adjustment.defense
                    } else {
                        0.0
                    },
            );
            let fort = if env.fort_level == 0 {
                0
            } else {
                usize::from(env.fort_level)
                    .saturating_sub(self.directions() - 1)
                    .max(1)
            };
            f.multiply(
                "fort",
                1.0 + fort as f64 * rules.fort_penalty,
                1.0 + fort as f64 * rules.fort_penalty,
            );
        }
        if env.night {
            f.multiply("night", 1.0 + rules.night_attack, 1.0);
        }
        let weather = |key: &str| env.weather.get(key).copied().unwrap_or(0.0);
        f.multiply("weather attack", 1.0 + weather("army_attack_factor"), 1.0);
        f.apply(
            "weather defense",
            CombatStat::Defense,
            1.0 + weather("army_defence_factor"),
        );
        f.apply(
            "weather defense",
            CombatStat::Breakthrough,
            1.0 + weather("army_defence_factor"),
        );
        f.apply(
            "weather breakthrough",
            CombatStat::Breakthrough,
            1.0 + weather("breakthrough_factor"),
        );
        let used = Self::used(side, units);
        let width_factor = 1.0 - rules.width_penalty(used, width);
        f.multiply("over width", width_factor, width_factor);
        let stack_factor = 1.0 - rules.stacking(side.active.len(), self.directions());
        f.multiply("stacking", stack_factor, stack_factor);
        f.multiply("tactic", 1.0 + tactic_bonus, 1.0);
        let air = if attacker {
            &env.attacker_air
        } else {
            &env.defender_air
        };
        f.multiply(
            "ground air support",
            1.0 + air.ground_support_bonus,
            1.0 + air.ground_support_bonus,
        );
        f.multiply(
            "enemy air superiority",
            1.0,
            1.0 + rules.air_superiority * air.enemy_superiority.clamp(0.0, 1.0),
        );
        // Only fuel-consuming components lose their equipment combat output.
        if stats.fuel > 0.0 {
            let loss = (1.0 - division.fuel_fraction.clamp(0.0, 1.0))
                * (1.0 - rules.fuel_attack_penalty.unwrap_or(0.0));
            let fs = &division.derived.fuel_stats;
            for (stat, affected, total) in [
                (CombatStat::SoftAttack, fs.soft_attack, stats.soft_attack),
                (CombatStat::HardAttack, fs.hard_attack, stats.hard_attack),
                (CombatStat::Defense, fs.defense, stats.defense),
                (
                    CombatStat::Breakthrough,
                    fs.breakthrough,
                    stats.breakthrough,
                ),
            ] {
                f.apply(
                    "fuel",
                    stat,
                    1.0 - loss * (affected / total.max(1e-9)).clamp(0.0, 1.0),
                );
            }
        }
        let ability = |key: &str| division.ability_modifiers.get(key).copied().unwrap_or(0.0);
        f.multiply(
            "commander ability",
            1.0 + ability("offence") + ability("army_attack_factor"),
            1.0,
        );
        f.apply(
            "commander ability",
            CombatStat::Defense,
            1.0 + ability("defence") + ability("army_defence_factor"),
        );
        f.apply(
            "commander ability",
            CombatStat::Breakthrough,
            1.0 + ability("breakthrough_factor") + ability("army_defence_factor"),
        );
        if !attacker {
            f.multiply(
                "ability entrenchment",
                1.0 + ability("combat_entrenchment"),
                1.0 + ability("combat_entrenchment"),
            );
        }
        let strength = division.strength();
        f.multiply("strength", strength, strength);
        f
    }

    fn reinforce<R: CombatRandom>(
        side: &mut BattleSide,
        units: &BTreeMap<u64, Division>,
        width: f64,
        rules: &CombatRules,
        rng: &mut R,
    ) -> Option<u64> {
        let used = Self::used(side, units);
        let mut candidates = side.reserves.clone();
        rng.shuffle(&mut candidates);
        for id in candidates {
            let Some(d) = units.get(&id).filter(|d| d.fighting()) else {
                continue;
            };
            if !rules.can_fit(used, d.derived.stats.width, width) {
                continue;
            }
            let chance = rules.reinforce_chance
                + d.reinforce_bonus
                + d.ability_modifiers
                    .get("land_reinforce_rate")
                    .copied()
                    .unwrap_or(0.0)
                + d.derived.stats.initiative * rules.initiative_reinforce;
            if rng.chance(chance) {
                side.reserves.retain(|v| *v != id);
                side.active.push(id);
                return Some(id);
            }
            break;
        }
        None
    }

    pub fn step<R: CombatRandom>(
        &mut self,
        units: &mut BTreeMap<u64, Division>,
        catalogue: &MilitaryCatalogue,
        country_modifiers: &BTreeMap<String, MilitaryModifiers>,
        env: &BattleEnvironment,
        rng: &mut R,
    ) -> Option<HourTrace> {
        if self.outcome.is_some() {
            return None;
        }
        let rules = &catalogue.rules;
        self.elapsed_hours += 1;
        let mut trace = HourTrace {
            hour: self.elapsed_hours,
            rng_start: rng.checkpoint(),
            ..Default::default()
        };
        if let Some(outcome) = self.pending_outcome {
            if let Some(last) = self.history.back() {
                trace.width = last.width;
                trace.attacker_width = last.attacker_width;
                trace.defender_width = last.defender_width;
                trace.attacker_tactic = last.attacker_tactic.clone();
                trace.defender_tactic = last.defender_tactic.clone();
            }
            if self.elapsed_hours >= rules.minimum_hours {
                self.outcome = Some(outcome);
                trace.outcome = Some(outcome);
            }
            self.record(&trace);
            return Some(trace);
        }
        for side in [&mut self.attackers, &mut self.defenders] {
            side.active
                .retain(|id| units.get(id).is_some_and(Division::fighting));
            side.reserves
                .retain(|id| units.get(id).is_some_and(Division::fighting));
        }
        if self.elapsed_hours == 1 || (self.elapsed_hours - 1) % rules.tactic_hours == 0 {
            self.attacker_tactic = Self::choose_tactic(
                catalogue,
                &self.attackers,
                &self.defenders,
                true,
                units,
                country_modifiers,
                self.phase.as_deref(),
                rng,
            );
            self.defender_tactic = Self::choose_tactic(
                catalogue,
                &self.defenders,
                &self.attackers,
                false,
                units,
                country_modifiers,
                self.phase.as_deref(),
                rng,
            );
            for id in [&self.attacker_tactic, &self.defender_tactic]
                .into_iter()
                .flatten()
            {
                if let Some(phase) = catalogue
                    .tactics
                    .iter()
                    .find(|t| &t.id == id)
                    .and_then(|t| t.phase.clone())
                {
                    self.phase = Some(phase);
                }
            }
            trace.tactic_changed = true;
        }
        let attacker_tactic = self
            .attacker_tactic
            .as_ref()
            .and_then(|id| catalogue.tactics.iter().find(|t| &t.id == id));
        let defender_tactic = self
            .defender_tactic
            .as_ref()
            .and_then(|id| catalogue.tactics.iter().find(|t| &t.id == id));
        let attacker_countered = attacker_tactic.is_some_and(|t| {
            t.countered_by.as_ref() == self.defender_tactic.as_ref() && t.countered_by.is_some()
        });
        let defender_countered = defender_tactic.is_some_and(|t| {
            t.countered_by.as_ref() == self.attacker_tactic.as_ref() && t.countered_by.is_some()
        });
        let tactics: Vec<_> = [
            (attacker_tactic, attacker_countered),
            (defender_tactic, defender_countered),
        ]
        .into_iter()
        .filter_map(|(t, c)| if c { None } else { t })
        .collect();
        let width = self.base_width(env)
            * (1.0 + tactics.iter().map(|t| t.width_bonus).sum::<f64>()).max(0.01);
        trace.width = width;
        trace.attacker_tactic = self
            .attacker_tactic
            .clone()
            .unwrap_or_else(|| "neutral/unavailable".into());
        trace.defender_tactic = self
            .defender_tactic
            .clone()
            .unwrap_or_else(|| "neutral/unavailable".into());
        if let Some(id) = Self::reinforce(&mut self.attackers, units, width, rules, rng) {
            trace.reinforced.push(id);
        }
        if let Some(id) = Self::reinforce(&mut self.defenders, units, width, rules, rng) {
            trace.reinforced.push(id);
        }
        trace.attacker_width = Self::used(&self.attackers, units);
        trace.defender_width = Self::used(&self.defenders, units);
        trace.attacker_over_width_penalty = rules.width_penalty(trace.attacker_width, width);
        trace.defender_over_width_penalty = rules.width_penalty(trace.defender_width, width);
        trace.attacker_stacking_penalty =
            rules.stacking(self.attackers.active.len(), self.directions());
        trace.defender_stacking_penalty =
            rules.stacking(self.defenders.active.len(), self.directions());
        trace.attacker_frontline = self.attackers.active.clone();
        trace.defender_frontline = self.defenders.active.clone();
        trace.attacker_reserves = self.attackers.reserves.clone();
        trace.defender_reserves = self.defenders.reserves.clone();
        let mut factors = BTreeMap::new();
        let mut defensive_budgets = BTreeMap::new();
        for (side, attacker) in [(&self.attackers, true), (&self.defenders, false)] {
            let tactic_bonus = tactics
                .iter()
                .map(|t| {
                    if attacker {
                        t.attacker_bonus
                    } else {
                        t.defender_bonus
                    }
                })
                .sum();
            for id in &side.active {
                let d = &units[id];
                let f = self.factors(d, attacker, side, units, env, rules, width, tactic_bonus);
                let defensive = if attacker {
                    d.derived.stats.breakthrough
                } else {
                    d.derived.stats.defense
                };
                defensive_budgets.insert(
                    *id,
                    rng.stochastic_round(
                        defensive * if attacker { f.breakthrough } else { f.defense }
                            / rules.attack_scale,
                    ),
                );
                factors.insert(*id, f);
            }
        }
        // Accumulate all damage before changing any combatant's HP/Org. No side
        // gets a first-strike advantage from map iteration order.
        let mut damage = BTreeMap::<u64, (f64, f64)>::new();
        for (side, enemy, attacker) in [
            (&self.attackers, &self.defenders, true),
            (&self.defenders, &self.attackers, false),
        ] {
            for id in &side.active {
                let d = &units[id];
                let f = &factors[id];
                let mut candidates = enemy.active.clone();
                rng.shuffle(&mut candidates);
                let mut targets = Vec::new();
                let mut used = 0.0;
                let budget = d.derived.stats.width * rules.engagement_width;
                for target in candidates {
                    let w = units[&target].derived.stats.width;
                    if targets.is_empty() || used + w <= budget + 1e-9 {
                        targets.push(target);
                        used += w;
                    }
                }
                if targets.is_empty() {
                    continue;
                }
                let priority = *targets
                    .iter()
                    .min_by(|a, b| {
                        units[a]
                            .organization
                            .total_cmp(&units[b].organization)
                            .then_with(|| a.cmp(b))
                    })
                    .unwrap();
                let coordinated = (rules.coordinated_base
                    + d.coordination * (1.0 + d.derived.stats.initiative))
                    .clamp(0.0, 1.0);
                for target in targets {
                    let opponent = &units[&target];
                    let fraction = (1.0 - coordinated) * opponent.derived.stats.width
                        / used.max(1e-9)
                        + if target == priority { coordinated } else { 0.0 };
                    let effective = (d.derived.stats.soft_attack
                        * f.soft_attack
                        * (1.0 - opponent.derived.stats.hardness)
                        + d.derived.stats.hard_attack
                            * f.hard_attack
                            * opponent.derived.stats.hardness)
                        * fraction;
                    let rolls = rng.stochastic_round(effective / rules.attack_scale);
                    let remaining = defensive_budgets.get_mut(&target).unwrap();
                    let target_defensive_rolls = *remaining;
                    let protected = rolls.min(*remaining);
                    *remaining -= protected;
                    let mut damage_rolls = Vec::new();
                    let (hp, org, protected_hits, overflow_hits) = resolve_hits_recorded(
                        rolls,
                        protected,
                        d,
                        opponent,
                        rules,
                        rng,
                        self.trace_enabled.then_some(&mut damage_rolls),
                    );
                    let org_tactic = 1.0
                        + tactics
                            .iter()
                            .map(|t| {
                                if attacker {
                                    t.attacker_org_damage
                                } else {
                                    t.defender_org_damage
                                }
                            })
                            .sum::<f64>();
                    let org = org
                        * org_tactic.max(0.0)
                        * f.org_damage
                        * (1.0
                            + opponent
                                .ability_modifiers
                                .get("org_damage_multiplier")
                                .copied()
                                .unwrap_or(0.0))
                        .max(0.0);
                    let hp = hp
                        * f.hp_damage
                        * (1.0
                            + opponent
                                .ability_modifiers
                                .get("str_damage_multiplier")
                                .copied()
                                .unwrap_or(0.0))
                        .max(0.0);
                    let delta = damage.entry(target).or_default();
                    delta.0 += hp;
                    delta.1 += org;
                    if self.trace_enabled {
                        trace.attacks.push(AttackTrace {
                            from: *id,
                            to: target,
                            effective_attack: effective,
                            rolls,
                            target_defensive_rolls,
                            protected,
                            protected_hits,
                            overflow_hits,
                            armor_factor: rules.armor_factor(
                                d.derived.stats.piercing,
                                opponent.derived.stats.armor,
                            ),
                            hp_damage: hp,
                            org_damage: org,
                            modifiers: f.trace.clone(),
                            damage_rolls,
                        });
                    }
                }
            }
            let air = if attacker {
                &env.attacker_air
            } else {
                &env.defender_air
            };
            if air.cas_planes > 0 && air.plane_attack_rolls > 0.0 && !enemy.active.is_empty() {
                let max_planes = (Self::used(enemy, units) * rules.cas_planes_per_width).max(0.0);
                let participation = (max_planes / f64::from(air.cas_planes)).min(1.0);
                let reduction = air.aa_damage_reduction.clamp(0.0, rules.aa_max_reduction);
                let amount = air.plane_attack_rolls * participation * (1.0 - reduction)
                    / enemy.active.len() as f64;
                for id in &enemy.active {
                    let delta = damage.entry(*id).or_default();
                    delta.0 += amount * rules.cas_hp;
                    delta.1 += amount * rules.cas_org;
                }
            }
        }
        for (id, (hp, org)) in damage {
            let d = units.get_mut(&id).unwrap();
            let (men, equipment) = d.apply_damage(hp, org, rules);
            trace.manpower_losses += men;
            trace.equipment_losses += equipment;
        }
        for side in [&mut self.attackers, &mut self.defenders] {
            side.active.retain(|id| {
                let keep = units.get(id).is_some_and(Division::fighting);
                if !keep {
                    trace.exited.push(*id);
                }
                keep
            });
        }
        let result = match (
            self.attackers.active.is_empty(),
            self.defenders.active.is_empty(),
        ) {
            (true, true) => Some(BattleOutcome::Draw),
            (true, false) => Some(BattleOutcome::DefenderWon),
            (false, true) => Some(BattleOutcome::AttackerWon),
            _ => None,
        };
        if let Some(result) = result {
            self.pending_outcome = Some(result);
            if self.elapsed_hours >= rules.minimum_hours {
                self.outcome = Some(result);
                trace.outcome = Some(result);
            }
        }
        trace.rng_end = rng.checkpoint();
        self.record(&trace);
        Some(trace)
    }
    fn record(&mut self, trace: &HourTrace) {
        self.history.push_back(trace.clone());
        while self.history.len() > 48 {
            self.history.pop_front();
        }
    }
}

pub fn resolve_hits<R: CombatRandom>(
    rolls: u32,
    protected: u32,
    attacker: &Division,
    target: &Division,
    rules: &CombatRules,
    rng: &mut R,
) -> (f64, f64, u32, u32) {
    resolve_hits_recorded(rolls, protected, attacker, target, rules, rng, None)
}
fn resolve_hits_recorded<R: CombatRandom>(
    rolls: u32,
    protected: u32,
    attacker: &Division,
    target: &Division,
    rules: &CombatRules,
    rng: &mut R,
    mut record: Option<&mut Vec<DamageRoll>>,
) -> (f64, f64, u32, u32) {
    let armor = rules.armor_factor(attacker.derived.stats.piercing, target.derived.stats.armor);
    let armored = attacker.derived.stats.armor > 0.0
        && target.derived.stats.piercing / attacker.derived.stats.armor < 0.5;
    let org_die = if armored {
        rules.armored_org_die
    } else {
        rules.org_die
    };
    let (mut hp, mut org, mut defended_hits, mut overflow_hits) = (0.0, 0.0, 0, 0);
    for i in 0..rolls {
        let chance = if i < protected {
            rules.defended_hit
        } else {
            rules.undefended_hit
        };
        if !rng.chance(chance) {
            continue;
        }
        if i < protected {
            defended_hits += 1;
        } else {
            overflow_hits += 1;
        }
        let hp_die = rng.die(rules.hp_die);
        let org_die = rng.die(org_die);
        if let Some(record) = record.as_deref_mut() {
            record.push(DamageRoll {
                protected: i < protected,
                hp_die,
                org_die,
            });
        }
        hp += f64::from(hp_die) * rules.hp_damage * armor;
        org += f64::from(org_die) * rules.org_damage * armor;
    }
    (hp, org, defended_hits, overflow_hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat_rules::SeededRandom;
    use crate::military_model::tests::{catalogue, division};
    #[test]
    fn typed_scopes_and_tactics_have_full_twelve_hour_intervals() {
        let c = catalogue();
        let mut a = division(1, "A", 1);
        let mut b = division(2, "B", 2);
        a.derived.stats.soft_attack = 0.0;
        b.derived.stats.soft_attack = 0.0;
        a.derived.stats.hard_attack = 0.0;
        b.derived.stats.hard_attack = 0.0;
        let mut units = BTreeMap::from([(1, a), (2, b)]);
        let mut env = BattleEnvironment::default();
        env.weather.insert("breakthrough_factor".into(), -0.6);
        let mut battle = LandBattle::new(1, 2, vec![1], vec![2], &units, &env, &c.rules);
        let f = battle.factors(
            &units[&1],
            true,
            &battle.attackers,
            &units,
            &env,
            &c.rules,
            70.0,
            0.0,
        );
        assert_eq!(f.soft_attack, 1.0);
        assert_eq!(f.defense, 1.0);
        assert_eq!(f.breakthrough, 0.4);
        let mut rng = SeededRandom::new(12);
        let mut changes = Vec::new();
        for hour in 1..=25 {
            let trace = battle
                .step(&mut units, &c, &BTreeMap::new(), &env, &mut rng)
                .unwrap();
            if trace.tactic_changed {
                changes.push(hour);
            }
        }
        assert_eq!(changes, vec![1, 13, 25]);
    }
    #[test]
    fn hourly_trace_is_replayable_and_records_dice_and_shared_defense() {
        let mut c = catalogue();
        c.rules.hp_damage = 0.0001;
        c.rules.org_damage = 0.0001;
        let mut a = division(1, "A", 1);
        let mut b = division(2, "B", 2);
        a.derived.stats.soft_attack = 1000.0;
        b.derived.stats.soft_attack = 1000.0;
        a.derived.stats.defense = 9000.0;
        a.derived.stats.breakthrough = 0.0;
        b.derived.stats.defense = 9000.0;
        b.derived.stats.breakthrough = 0.0;
        let mut units = BTreeMap::from([(1, a), (2, b)]);
        let mut other = units.clone();
        let env = BattleEnvironment::default();
        let mut battle = LandBattle::new(1, 2, vec![1], vec![2], &units, &env, &c.rules);
        battle.trace_enabled = true;
        let mut replay = battle.clone();
        let mut rng = SeededRandom::new(123);
        let mut rng2 = rng.clone();
        let t = battle
            .step(&mut units, &c, &BTreeMap::new(), &env, &mut rng)
            .unwrap();
        let r = replay
            .step(&mut other, &c, &BTreeMap::new(), &env, &mut rng2)
            .unwrap();
        assert_eq!(
            serde_json::to_value(&t).unwrap(),
            serde_json::to_value(r).unwrap()
        );
        let attacking = t.attacks.iter().find(|t| t.from == 1).unwrap();
        let defending = t.attacks.iter().find(|t| t.from == 2).unwrap();
        assert_eq!(attacking.protected, attacking.rolls);
        assert_eq!(defending.protected, 0);
        assert_eq!(
            defending.damage_rolls.len() as u32,
            defending.protected_hits + defending.overflow_hits
        );
        assert_eq!(t.rng_start, Some(123));
        assert_eq!(t.rng_end, Some(rng.state));
    }
    #[test]
    fn defense_and_breakthrough_are_selected_by_role_and_modifiers_keep_their_sources() {
        let a = division(1, "A", 1);
        let b = division(2, "B", 2);
        let units = BTreeMap::from([(1, a), (2, b)]);
        let mut env = BattleEnvironment {
            night: true,
            fort_level: 2,
            ..Default::default()
        };
        env.crossing_penalties.insert(1, -0.3);
        let rules = crate::combat_rules::CombatRules::default();
        let battle = LandBattle::new(1, 2, vec![1], vec![2], &units, &env, &rules);
        let fa = battle.factors(
            &units[&1],
            true,
            &battle.attackers,
            &units,
            &env,
            &rules,
            70.0,
            0.0,
        );
        let fd = battle.factors(
            &units[&2],
            false,
            &battle.defenders,
            &units,
            &env,
            &rules,
            70.0,
            0.0,
        );
        assert!((fa.soft_attack - 0.7 * 0.7 * 0.5).abs() < 1e-9);
        assert!((fa.breakthrough - 0.7 * 0.7).abs() < 1e-9);
        assert!((fd.soft_attack - 0.5).abs() < 1e-9);
        assert_eq!(fd.defense, 1.0);
        assert!(fa.trace.iter().any(|m| m.source == "river"));
        assert!(fa.trace.iter().any(|m| m.source == "fort"));
    }
    #[test]
    fn reinforcement_uses_initiative_addition_and_never_exceeds_width() {
        let mut d = division(2, "A", 1);
        d.derived.stats.width = 20.0;
        d.derived.stats.initiative = 0.2;
        let units = BTreeMap::from([(2, d)]);
        let mut side = BattleSide {
            active: vec![],
            reserves: vec![2],
        };
        struct Probe(f64);
        impl CombatRandom for Probe {
            fn next_u64(&mut self) -> u64 {
                1
            }
            fn chance(&mut self, p: f64) -> bool {
                self.0 = p;
                true
            }
        }
        let mut rng = Probe(0.0);
        let r = crate::combat_rules::CombatRules::default();
        assert_eq!(
            LandBattle::reinforce(&mut side, &units, 70.0, &r, &mut rng),
            Some(2)
        );
        assert!((rng.0 - 0.07).abs() < 1e-9);
        let mut side = BattleSide {
            active: vec![],
            reserves: vec![2],
        };
        side.active.push(2);
        // The initial oversized division exception is not permission for a second.
        assert_eq!(
            LandBattle::reinforce(&mut side, &units, 20.0, &r, &mut rng),
            None
        );
    }
    #[test]
    fn cas_width_limit_and_aa_cap_apply_to_both_damage_channels() {
        let mut c = catalogue();
        c.rules.minimum_hours = 1;
        let mut a = division(1, "A", 1);
        let mut b = division(2, "B", 2);
        a.derived.stats.soft_attack = 0.0;
        b.derived.stats.soft_attack = 0.0;
        b.derived.stats.width = 2.0;
        let hp = b.hp;
        let org = b.organization;
        let mut units = BTreeMap::from([(1, a), (2, b)]);
        let env = BattleEnvironment {
            attacker_air: AirSupport {
                cas_planes: 12,
                plane_attack_rolls: 100.0,
                aa_damage_reduction: 1.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut battle = LandBattle::new(1, 2, vec![1], vec![2], &units, &env, &c.rules);
        let mut rng = SeededRandom::new(1);
        battle.step(&mut units, &c, &BTreeMap::new(), &env, &mut rng);
        assert!((hp - units[&2].hp - 100.0 * 0.5 * 0.25 * 0.035).abs() < 1e-9);
        assert!((org - units[&2].organization - 100.0 * 0.5 * 0.25 * 0.035).abs() < 1e-9);
    }
    #[test]
    fn damage_distribution_matches_protected_and_overflow_expectation() {
        let rules = CombatRules::default();
        let a = division(1, "A", 1);
        let b = division(2, "B", 2);
        let mut rng = SeededRandom::new(712);
        let (mut hits, mut hp, mut org) = (0.0, 0.0, 0.0);
        for _ in 0..20000 {
            let (h, o, p, u) = resolve_hits(100, 50, &a, &b, &rules, &mut rng);
            hits += f64::from(p + u);
            hp += h;
            org += o;
        }
        assert!((hits / 20000.0 - 25.0).abs() < 0.15);
        assert!((hp / 20000.0 - 25.0 * 0.09).abs() < 0.02);
        assert!((org / 20000.0 - 25.0 * 0.1325).abs() < 0.03);
    }
    struct AlwaysHit;
    impl CombatRandom for AlwaysHit {
        fn next_u64(&mut self) -> u64 {
            u64::MAX / 2
        }
        fn chance(&mut self, p: f64) -> bool {
            p > 0.0
        }
        fn die(&mut self, _: u32) -> u32 {
            1
        }
    }
    #[test]
    fn simultaneous_fire_minimum_duration_and_zero_org_exit() {
        let mut a = division(1, "A", 1);
        let mut b = division(2, "B", 2);
        a.derived.stats.soft_attack = 1000.0;
        b.derived.stats.soft_attack = 1000.0;
        a.hp = 0.01;
        b.hp = 0.01;
        a.organization = 0.0001;
        b.organization = 0.0001;
        // Keep firing strength full while testing simultaneous organization collapse.
        a.hp = a.derived.stats.hp;
        b.hp = b.derived.stats.hp;
        let mut units = BTreeMap::from([(1, a), (2, b)]);
        let env = BattleEnvironment::default();
        let c = catalogue();
        let mut battle = LandBattle::new(1, 2, vec![1], vec![2], &units, &env, &c.rules);
        battle.trace_enabled = true;
        let first = battle
            .step(&mut units, &c, &BTreeMap::new(), &env, &mut AlwaysHit)
            .unwrap();
        assert_eq!(first.exited.len(), 2);
        assert!(first.outcome.is_none());
        for _ in 0..3 {
            battle.step(&mut units, &c, &BTreeMap::new(), &env, &mut AlwaysHit);
        }
        assert_eq!(battle.outcome, Some(BattleOutcome::Draw));
    }
    #[test]
    fn hourly_defense_budget_is_shared_between_attacks() {
        let mut a = division(1, "A", 1);
        a.derived.stats.soft_attack = 1000.0;
        let mut b = a.clone();
        b.id = 2;
        let mut d = division(3, "B", 2);
        d.derived.stats.defense = 1000.0;
        d.hp = 100000.0;
        d.derived.stats.hp = 100000.0;
        let mut units = BTreeMap::from([(1, a), (2, b), (3, d)]);
        let c = catalogue();
        let env = BattleEnvironment::default();
        let mut battle = LandBattle::new(1, 2, vec![1, 2], vec![3], &units, &env, &c.rules);
        battle.trace_enabled = true;
        let trace = battle
            .step(&mut units, &c, &BTreeMap::new(), &env, &mut AlwaysHit)
            .unwrap();
        let protected: u32 = trace
            .attacks
            .iter()
            .filter(|a| a.to == 3)
            .map(|a| a.protected)
            .sum();
        assert_eq!(protected, 100);
    }
    #[test]
    fn tactics_change_on_twelve_hour_boundaries_and_unknown_not_stays_blocked() {
        let mut c = catalogue();
        let mut a = division(1, "A", 1);
        let mut b = division(2, "B", 2);
        a.derived.stats.soft_attack = 0.0;
        b.derived.stats.soft_attack = 0.0;
        let mut units = BTreeMap::from([(1, a), (2, b)]);
        let env = BattleEnvironment::default();
        let mut rng = SeededRandom::new(1);
        let mut battle = LandBattle::new(1, 2, vec![1], vec![2], &units, &env, &c.rules);
        let mut swaps = Vec::new();
        for _ in 0..25 {
            let t = battle
                .step(&mut units, &c, &BTreeMap::new(), &env, &mut rng)
                .unwrap();
            if t.tactic_changed {
                swaps.push(t.hour);
            }
        }
        assert_eq!(swaps, [1, 13, 25]);
        let entries = crate::research_data::parse_pdx("NOT = { unknown = yes }").unwrap();
        let m = MilitaryModifiers::default();
        assert_eq!(
            evaluate_conditions(
                &entries,
                &TacticContext {
                    country: "A",
                    attacker: true,
                    phase: None,
                    modifiers: &m,
                    numbers: BTreeMap::new()
                }
            ),
            None
        );
        c.rules.minimum_hours = 0;
    }
}
