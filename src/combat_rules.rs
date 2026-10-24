//! Versioned exposed constants; defaults follow the supplied 1.19.3 brief.
//! The profile is a reconstruction, not proof of binary-identical engine behavior.
use std::collections::BTreeMap;

pub trait CombatRandom {
    fn next_u64(&mut self) -> u64;
    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / 9007199254740992.0)
    }
    fn chance(&mut self, probability: f64) -> bool {
        self.unit() < probability.clamp(0.0, 1.0)
    }
    fn die(&mut self, sides: u32) -> u32 {
        let n = u64::from(sides.max(1));
        // Rejection removes modulo bias while keeping a deterministic stream.
        let threshold = n.wrapping_neg() % n;
        loop {
            let v = self.next_u64();
            if v >= threshold {
                return (v % n) as u32 + 1;
            }
        }
    }
    fn shuffle<T>(&mut self, values: &mut [T])
    where
        Self: Sized,
    {
        for i in (1..values.len()).rev() {
            let j = self.die((i + 1) as u32) as usize - 1;
            values.swap(i, j);
        }
    }
    fn stochastic_round(&mut self, value: f64) -> u32 {
        let value = if value.is_finite() {
            value.clamp(0.0, 1_000_000.0)
        } else {
            0.0
        };
        value.floor() as u32 + u32::from(self.chance(value.fract()))
    }
}

#[derive(Debug, Clone)]
pub struct SeededRandom {
    pub state: u64,
}
impl SeededRandom {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }
}
impl CombatRandom for SeededRandom {
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
}

#[derive(Debug, Clone)]
pub struct CombatRules {
    pub target_version: String,
    pub attack_scale: f64,
    pub defended_hit: f64,
    pub undefended_hit: f64,
    pub hp_die: u32,
    pub org_die: u32,
    pub armored_org_die: u32,
    pub hp_damage: f64,
    pub org_damage: f64,
    pub casualty_factor: f64,
    pub minimum_hours: u32,
    pub tactic_hours: u32,
    pub reinforce_chance: f64,
    pub engagement_width: f64,
    pub coordinated_base: f64,
    pub initiative_reinforce: f64,
    pub over_width: f64,
    pub over_width_cap: f64,
    pub stacking_start: u32,
    pub stacking_extra: u32,
    pub stacking_penalty: f64,
    pub piercing_thresholds: Vec<f64>,
    pub piercing_damage: Vec<f64>,
    pub armor_max_weight: f64,
    pub piercing_max_weight: f64,
    pub supply_attacker_attack: f64,
    pub supply_attacker_defense: f64,
    pub supply_defender_attack: f64,
    pub supply_defender_defense: f64,
    pub out_of_supply_recovery: f64,
    pub small_river_attack: f64,
    pub large_river_attack: f64,
    pub small_river_movement: f64,
    pub large_river_movement: f64,
    pub small_river_index: u8,
    pub large_river_index: u8,
    pub fort_penalty: f64,
    pub entrenchment_bonus: f64,
    pub dig_in_cap: f64,
    pub dig_in_daily: f64,
    pub planning_cap: f64,
    pub planning_daily: f64,
    pub planning_decay: f64,
    pub night_attack: f64,
    pub air_superiority: f64,
    pub aa_max_reduction: f64,
    pub cas_planes_per_width: f64,
    pub cas_hp: f64,
    pub cas_org: f64,
    pub regiment_columns: usize,
    pub regiment_rows: usize,
    pub support_slots: usize,
    pub line_edit_xp: f64,
    pub support_edit_xp: f64,
    pub new_group_xp: f64,
    pub pixels_to_km: Option<f64>,
    pub strategic_speed: Option<f64>,
    pub attack_delay: u32,
    pub movement_org_loss: f64,
    pub zero_org_speed: f64,
    pub retreat_speed: f64,
    pub strategic_org: f64,
    pub provenance: BTreeMap<String, String>,
    pub diagnostics: Vec<String>,
}

impl Default for CombatRules {
    fn default() -> Self {
        Self {
            target_version: "HOI4 1.19.3 reconstruction (not parity certified)".into(),
            attack_scale: 10.0,
            defended_hit: 0.1,
            undefended_hit: 0.4,
            hp_die: 2,
            org_die: 4,
            armored_org_die: 6,
            hp_damage: 0.06,
            org_damage: 0.053,
            casualty_factor: 0.7,
            minimum_hours: 4,
            tactic_hours: 12,
            reinforce_chance: 0.02,
            engagement_width: 2.0,
            coordinated_base: 0.35,
            initiative_reinforce: 0.25,
            over_width: 1.0,
            over_width_cap: 0.33,
            stacking_start: 5,
            stacking_extra: 3,
            stacking_penalty: 0.02,
            piercing_thresholds: vec![1.0, 0.75, 0.5, 0.0],
            piercing_damage: vec![1.0, 0.8, 0.65, 0.5],
            armor_max_weight: 0.4,
            piercing_max_weight: 0.4,
            supply_attacker_attack: -0.25,
            supply_attacker_defense: -0.65,
            supply_defender_attack: -0.35,
            supply_defender_defense: -0.15,
            out_of_supply_recovery: -0.2,
            small_river_attack: -0.3,
            large_river_attack: -0.6,
            small_river_movement: -0.25,
            large_river_movement: -0.5,
            small_river_index: 6,
            large_river_index: 11,
            fort_penalty: -0.15,
            entrenchment_bonus: 0.02,
            dig_in_cap: 5.0,
            dig_in_daily: 1.0,
            planning_cap: 0.3,
            planning_daily: 0.02,
            planning_decay: 0.01,
            night_attack: -0.5,
            air_superiority: -0.35,
            aa_max_reduction: 0.75,
            cas_planes_per_width: 3.0,
            cas_hp: 0.035,
            cas_org: 0.035,
            regiment_columns: 5,
            regiment_rows: 5,
            support_slots: 5,
            line_edit_xp: 5.0,
            support_edit_xp: 10.0,
            new_group_xp: 20.0,
            // No invented travel-time calibration when the map define is absent.
            pixels_to_km: None,
            strategic_speed: None,
            attack_delay: 24,
            movement_org_loss: -0.2,
            zero_org_speed: -0.8,
            retreat_speed: 0.25,
            strategic_org: 0.1,
            provenance: BTreeMap::new(),
            diagnostics: Vec::new(),
        }
    }
}

impl CombatRules {
    pub fn armor_factor(&self, piercing: f64, armor: f64) -> f64 {
        if armor <= 0.0 {
            return 1.0;
        }
        let ratio = piercing.max(0.0) / armor;
        self.piercing_thresholds
            .iter()
            .zip(&self.piercing_damage)
            .find(|(threshold, _)| ratio >= **threshold)
            .map_or(1.0, |(_, v)| *v)
    }
    pub fn width_penalty(&self, used: f64, available: f64) -> f64 {
        if available <= 0.0 {
            return self.over_width_cap;
        }
        ((used / available - 1.0).max(0.0) * self.over_width).min(self.over_width_cap)
    }
    pub fn can_fit(&self, used: f64, next: f64, width: f64) -> bool {
        used == 0.0
            || (used + next - width).max(0.0) / width.max(0.01) * self.over_width
                <= self.over_width_cap + 1e-9
    }
    pub fn stacking(&self, count: usize, directions: usize) -> f64 {
        let free = self.stacking_start as usize
            + self.stacking_extra as usize * directions.saturating_sub(1);
        (count.saturating_sub(free) as f64 * self.stacking_penalty).min(1.0)
    }
    pub fn import_defines(&mut self, source: &str, path: &str) {
        let mut lines = source.lines();
        let mut depth = 0i32;
        let mut scope: Option<(String, i32)> = None;
        while let Some(line) = lines.next() {
            let line = line.split("--").next().unwrap_or("");
            let Some((left, right)) = line.split_once('=') else {
                depth += line.chars().filter(|c| *c == '{').count() as i32
                    - line.chars().filter(|c| *c == '}').count() as i32;
                if scope.as_ref().is_some_and(|s| depth < s.1) {
                    scope = None;
                }
                continue;
            };
            let key = left.trim().rsplit('.').next().unwrap_or("").trim();
            let mut raw = right.trim().to_owned();
            let start_depth = depth;
            let namespaced = left
                .trim()
                .split('.')
                .find(|p| {
                    *p != "NDefines"
                        && p.starts_with('N')
                        && p.chars().any(|c| c.is_ascii_lowercase())
                })
                .map(str::to_owned);
            if raw.starts_with('{')
                && key.starts_with('N')
                && key.chars().any(|c| c.is_ascii_lowercase())
            {
                scope = Some((key.into(), depth + 1));
            }
            if matches!(
                key,
                "PIERCING_THRESHOLDS" | "PIERCING_THRESHOLD_DAMAGE_VALUES"
            ) && raw.starts_with('{')
                && !raw.contains('}')
            {
                for line in lines.by_ref() {
                    raw.push_str(line.split("--").next().unwrap_or(""));
                    if raw.contains('}') {
                        break;
                    }
                }
            }
            depth += raw.chars().filter(|c| *c == '{').count() as i32
                - raw.chars().filter(|c| *c == '}').count() as i32;
            let section = namespaced
                .as_deref()
                .filter(|s| *s != "NDefines")
                .or_else(|| scope.as_ref().map(|s| s.0.as_str()));
            let allowed = section.is_none()
                || section == Some("NMilitary")
                || (key == "MAP_SCALE_PIXEL_TO_KM" && section == Some("NGame"));
            if scope
                .as_ref()
                .is_some_and(|s| depth < s.1 && start_depth >= s.1)
            {
                scope = None;
            }
            if !allowed {
                continue;
            }
            if matches!(
                key,
                "PIERCING_THRESHOLDS" | "PIERCING_THRESHOLD_DAMAGE_VALUES"
            ) {
                let vals: Option<Vec<f64>> = raw
                    .trim()
                    .trim_end_matches(',')
                    .trim_start_matches('{')
                    .trim_end_matches('}')
                    .split(',')
                    .filter(|v| !v.trim().is_empty())
                    .map(|v| v.trim().parse::<f64>().ok().filter(|v| v.is_finite()))
                    .collect();
                if let Some(v) = vals {
                    if !v.is_empty() {
                        if key == "PIERCING_THRESHOLDS" {
                            self.piercing_thresholds = v;
                        } else {
                            self.piercing_damage = v;
                        }
                        self.provenance.insert(key.into(), path.into());
                    }
                }
                continue;
            }
            let raw = raw.split(',').next().unwrap_or("").trim();
            let Some(v) = raw.parse::<f64>().ok().filter(|v| v.is_finite()) else {
                continue;
            };
            let recognized = match key {
                "BASE_CHANCE_TO_AVOID_HIT" => {
                    self.defended_hit = (1.0 - v / 100.0).clamp(0.0, 1.0);
                    true
                }
                "CHANCE_TO_AVOID_HIT_AT_NO_DEF" => {
                    self.undefended_hit = (1.0 - v / 100.0).clamp(0.0, 1.0);
                    true
                }
                "LAND_COMBAT_STR_DICE_SIZE" => {
                    self.hp_die = v.max(1.0) as u32;
                    true
                }
                "LAND_COMBAT_ORG_DICE_SIZE" => {
                    self.org_die = v.max(1.0) as u32;
                    true
                }
                "LAND_COMBAT_ORG_ARMOR_ON_SOFT_DICE_SIZE" => {
                    self.armored_org_die = v.max(1.0) as u32;
                    true
                }
                "LAND_COMBAT_STR_DAMAGE_MODIFIER" => {
                    self.hp_damage = v.max(0.0);
                    true
                }
                "LAND_COMBAT_ORG_DAMAGE_MODIFIER" => {
                    self.org_damage = v.max(0.0);
                    true
                }
                "COMBAT_MINIMUM_TIME" => {
                    self.minimum_hours = v.max(0.0) as u32;
                    true
                }
                "TACTIC_SWAP_FREQUENCEY" => {
                    self.tactic_hours = v.max(1.0) as u32;
                    true
                }
                "ARMY_INITIATIVE_REINFORCE_FACTOR" => {
                    self.initiative_reinforce = v;
                    true
                }
                "BASE_REINFORCE_CHANCE" => {
                    self.reinforce_chance = v.clamp(0.0, 1.0);
                    true
                }
                "COMBAT_OVER_WIDTH_PENALTY" => {
                    self.over_width = v.abs();
                    true
                }
                "COMBAT_OVER_WIDTH_PENALTY_MAX" => {
                    self.over_width_cap = v.abs().min(1.0);
                    true
                }
                "COMBAT_STACKING_START" => {
                    self.stacking_start = v.max(0.0) as u32;
                    true
                }
                "COMBAT_STACKING_EXTRA" => {
                    self.stacking_extra = v.max(0.0) as u32;
                    true
                }
                "COMBAT_STACKING_PENALTY" => {
                    self.stacking_penalty = v.abs();
                    true
                }
                "ARMOR_VS_AVERAGE" => {
                    self.armor_max_weight = v.clamp(0.0, 1.0);
                    true
                }
                "PIERCING_VS_AVERAGE" | "PEN_VS_AVERAGE" => {
                    self.piercing_max_weight = v.clamp(0.0, 1.0);
                    true
                }
                "COMBAT_SUPPLY_LACK_ATTACKER_ATTACK" => {
                    self.supply_attacker_attack = v;
                    true
                }
                "COMBAT_SUPPLY_LACK_ATTACKER_DEFEND" => {
                    self.supply_attacker_defense = v;
                    true
                }
                "COMBAT_SUPPLY_LACK_DEFENDER_ATTACK" => {
                    self.supply_defender_attack = v;
                    true
                }
                "COMBAT_SUPPLY_LACK_DEFENDER_DEFEND" => {
                    self.supply_defender_defense = v;
                    true
                }
                "OUT_OF_SUPPLY_MORALE" => {
                    self.out_of_supply_recovery = v;
                    true
                }
                "RIVER_CROSSING_PENALTY" => {
                    self.small_river_attack = v;
                    true
                }
                "RIVER_CROSSING_PENALTY_LARGE" => {
                    self.large_river_attack = v;
                    true
                }
                "RIVER_CROSSING_SPEED_PENALTY" => {
                    self.small_river_movement = v;
                    true
                }
                "RIVER_CROSSING_SPEED_PENALTY_LARGE" => {
                    self.large_river_movement = v;
                    true
                }
                "SMALL_RIVER_LIMIT" | "RIVER_SMALL_STOP_INDEX" => {
                    self.small_river_index = v.clamp(0.0, 255.0) as u8;
                    true
                }
                "LARGE_RIVER_LIMIT" | "RIVER_LARGE_STOP_INDEX" => {
                    self.large_river_index = v.clamp(0.0, 255.0) as u8;
                    true
                }
                "FORT_PENALTY" | "BASE_FORT_PENALTY" => {
                    self.fort_penalty = v;
                    true
                }
                "DIG_IN_FACTOR" => {
                    self.entrenchment_bonus = v;
                    true
                }
                "UNIT_DIG_IN_CAP" => {
                    self.dig_in_cap = v.max(0.0);
                    true
                }
                "UNIT_DIG_IN_SPEED" => {
                    self.dig_in_daily = v.max(0.0);
                    true
                }
                "PLANNING_MAX" => {
                    self.planning_cap = v.max(0.0);
                    true
                }
                "PLANNING_GAIN" => {
                    self.planning_daily = v.max(0.0);
                    true
                }
                "PLANNING_DECAY" => {
                    self.planning_decay = v.max(0.0);
                    true
                }
                "LAND_NIGHT_ATTACK" => {
                    self.night_attack = v;
                    true
                }
                "ENEMY_AIR_SUPERIORITY_IMPACT" => {
                    self.air_superiority = v;
                    true
                }
                "ANTI_AIR_MAXIMUM_DAMAGE_REDUCTION_FACTOR" => {
                    self.aa_max_reduction = v.clamp(0.0, 1.0);
                    true
                }
                "LAND_AIR_COMBAT_MAX_PLANES_PER_ENEMY_WIDTH" => {
                    self.cas_planes_per_width = v.max(0.0);
                    true
                }
                "LAND_AIR_COMBAT_STR_DAMAGE_MODIFIER" => {
                    self.cas_hp = v.max(0.0);
                    true
                }
                "LAND_AIR_COMBAT_ORG_DAMAGE_MODIFIER" => {
                    self.cas_org = v.max(0.0);
                    true
                }
                "MAX_DIVISION_BRIGADE_WIDTH" => {
                    self.regiment_columns = v.clamp(1.0, 64.0) as usize;
                    true
                }
                "MAX_DIVISION_BRIGADE_HEIGHT" => {
                    self.regiment_rows = v.clamp(1.0, 64.0) as usize;
                    true
                }
                "MAX_DIVISION_SUPPORT_WIDTH" => {
                    self.support_slots = v.clamp(0.0, 64.0) as usize;
                    true
                }
                "HOURLY_ORG_MOVEMENT_IMPACT" => {
                    self.movement_org_loss = v;
                    true
                }
                "ZERO_ORG_MOVEMENT_MODIFIER" => {
                    self.zero_org_speed = v;
                    true
                }
                "RETREAT_SPEED_FACTOR" => {
                    self.retreat_speed = v;
                    true
                }
                "STRATEGIC_REDEPLOY_ORG_RATIO" => {
                    self.strategic_org = v.clamp(0.0, 1.0);
                    true
                }
                "STRATEGIC_SPEED_BASE" if v > 0.0 => {
                    self.strategic_speed = Some(v);
                    true
                }
                "MAP_SCALE_PIXEL_TO_KM" if v > 0.0 => {
                    self.pixels_to_km = Some(v);
                    true
                }
                "BASE_ATTACK_DELAY" => {
                    self.attack_delay = v.max(0.0) as u32;
                    true
                }
                _ => false,
            };
            if recognized {
                self.provenance.insert(key.into(), path.into());
            }
        }
        if self.piercing_thresholds.len() != self.piercing_damage.len()
            || self.piercing_thresholds.windows(2).any(|w| w[0] < w[1])
        {
            self.diagnostics.push(format!(
                "{path}: invalid piercing threshold arrays; restored brief defaults"
            ));
            self.piercing_thresholds = vec![1.0, 0.75, 0.5, 0.0];
            self.piercing_damage = vec![1.0, 0.8, 0.65, 0.5];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_lua_tables_and_namespaced_overrides_keep_land_and_naval_rules_separate() {
        let mut r = CombatRules::default();
        r.import_defines("NDefines = {\nNGame = {\nMAP_SCALE_PIXEL_TO_KM = 7.1,\n},\nNMilitary = {\nBASE_CHANCE_TO_AVOID_HIT = 85,\nENEMY_AIR_SUPERIORITY_IMPACT = -0.35,\nBASE_FORT_PENALTY = -0.17,\nPIERCING_THRESHOLDS = {\n1, 0.75, 0.5, 0\n},\n},\nNNavy = {\nENEMY_AIR_SUPERIORITY_IMPACT = -1,\n},\n}\nNDefines.NNavy.BASE_CHANCE_TO_AVOID_HIT = 1\nNDefines.NMilitary.BASE_CHANCE_TO_AVOID_HIT = 90", "fixture");
        assert!((r.defended_hit - 0.1).abs() < 1e-9);
        assert_eq!(r.air_superiority, -0.35);
        assert_eq!(r.fort_penalty, -0.17);
        assert_eq!(r.pixels_to_km, Some(7.1));
        assert_eq!(r.piercing_thresholds, vec![1.0, 0.75, 0.5, 0.0]);
    }
    #[test]
    fn stochastic_rounding_and_seed_replay_are_statistically_correct() {
        let mut a = SeededRandom::new(1);
        let mut b = a.clone();
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        let mean = (0..100000)
            .map(|_| a.stochastic_round(12.3) as f64)
            .sum::<f64>()
            / 100000.0;
        assert!((mean - 12.3).abs() < 0.01);
    }
    #[test]
    fn armor_thresholds_and_width_boundaries() {
        let r = CombatRules::default();
        for (p, v) in [
            (49.999, 0.5),
            (50.0, 0.65),
            (74.999, 0.65),
            (75.0, 0.8),
            (99.999, 0.8),
            (100.0, 1.0),
        ] {
            assert_eq!(r.armor_factor(p, 100.0), v);
        }
        assert!(r.can_fit(70.0, 23.1, 70.0));
        assert!(!r.can_fit(70.0, 23.101, 70.0));
        assert_eq!(r.stacking(9, 2), 0.02);
    }
    #[test]
    fn imports_namespaced_table_and_array_defines() {
        let mut r = CombatRules::default();
        r.import_defines("NDefines.NMilitary.TACTIC_SWAP_FREQUENCEY = 8 -- hi\nBASE_CHANCE_TO_AVOID_HIT=80,\nPIERCING_THRESHOLDS={1,0.5,\n0}\nPIERCING_THRESHOLD_DAMAGE_VALUES={1,0.7,0.4}\nMAP_SCALE_PIXEL_TO_KM = 7.1", "test.lua");
        assert_eq!(r.tactic_hours, 8);
        assert!((r.defended_hit - 0.2).abs() < 1e-9);
        assert_eq!(r.armor_factor(60.0, 100.0), 0.7);
        assert_eq!(r.pixels_to_km, Some(7.1));
    }
}
