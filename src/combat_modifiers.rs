//! Typed combat scopes: a breakthrough penalty can never change soft attack.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CombatStat {
    SoftAttack,
    HardAttack,
    Defense,
    Breakthrough,
    HpDamage,
    OrgDamage,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModifierTrace {
    pub source: String,
    pub stat: CombatStat,
    pub factor: f64,
}
#[derive(Debug, Clone)]
pub struct CombatFactors {
    pub soft_attack: f64,
    pub hard_attack: f64,
    pub defense: f64,
    pub breakthrough: f64,
    pub hp_damage: f64,
    pub org_damage: f64,
    pub trace: Vec<ModifierTrace>,
}
impl CombatFactors {
    pub fn identity() -> Self {
        Self {
            soft_attack: 1.0,
            hard_attack: 1.0,
            defense: 1.0,
            breakthrough: 1.0,
            hp_damage: 1.0,
            org_damage: 1.0,
            trace: Vec::new(),
        }
    }
    pub fn apply(&mut self, source: &str, stat: CombatStat, factor: f64) {
        let factor = if factor.is_finite() {
            factor.max(0.0)
        } else {
            0.0
        };
        let target = match stat {
            CombatStat::SoftAttack => &mut self.soft_attack,
            CombatStat::HardAttack => &mut self.hard_attack,
            CombatStat::Defense => &mut self.defense,
            CombatStat::Breakthrough => &mut self.breakthrough,
            CombatStat::HpDamage => &mut self.hp_damage,
            CombatStat::OrgDamage => &mut self.org_damage,
        };
        *target *= factor;
        if (factor - 1.0).abs() > 1e-12 {
            self.trace.push(ModifierTrace {
                source: source.into(),
                stat,
                factor,
            });
        }
    }
    pub fn multiply(&mut self, source: &str, attack: f64, defense: f64) {
        for stat in [CombatStat::SoftAttack, CombatStat::HardAttack] {
            self.apply(source, stat, attack);
        }
        for stat in [CombatStat::Defense, CombatStat::Breakthrough] {
            self.apply(source, stat, defense);
        }
    }
}
