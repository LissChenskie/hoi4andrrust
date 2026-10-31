//! Persistent AI state. Everything here is saved with the campaign so a loaded
//! game continues the same strategy instead of restarting it.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

pub const DECISION_LOG_CAPACITY: usize = 256;
pub const MAX_REASONS: usize = 8;

/// Military specialisation. Research, and later production and recruitment,
/// read this one value so they pull in the same direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Specialisation {
    #[default]
    Infantry,
    Armor,
    Motorized,
    Air,
    Navy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Posture {
    #[default]
    Peace,
    War,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Domain {
    Research,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub hour: u64,
    pub tag: String,
    pub domain: Domain,
    pub command: String,
    pub accepted: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiProfile {
    pub tag: String,
    pub specialisation: Specialisation,
    pub posture: Posture,
    /// Game hour of the last strategic review; `None` before the first one.
    pub last_strategic_review_hour: Option<u64>,
    /// Why the current specialisation/posture was chosen (bounded).
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiStats {
    pub research_planner_runs: u64,
    pub commands_accepted: u64,
    pub commands_rejected: u64,
    pub strategic_reviews: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiState {
    /// Seed for the deterministic per-country tie-breaking. Part of the save.
    pub seed: u64,
    pub profiles: BTreeMap<String, AiProfile>,
    pub log: VecDeque<Decision>,
    pub stats: AiStats,
    /// Derived from the catalogue; rebuilt on demand, never saved.
    #[serde(skip)]
    pub(crate) dependents: Option<(String, BTreeMap<String, u32>)>,
}

impl Default for AiState {
    fn default() -> Self {
        Self {
            seed: 0x4849_4F34_4149_0001,
            profiles: BTreeMap::new(),
            log: VecDeque::new(),
            stats: AiStats::default(),
            dependents: None,
        }
    }
}

impl AiState {
    pub fn with_seed(seed: u64) -> Self {
        Self {
            seed,
            ..Self::default()
        }
    }

    pub fn profile_mut(&mut self, tag: &str) -> &mut AiProfile {
        self.profiles
            .entry(tag.to_owned())
            .or_insert_with(|| AiProfile {
                tag: tag.to_owned(),
                ..AiProfile::default()
            })
    }

    pub fn record(&mut self, decision: Decision) {
        if decision.accepted {
            self.stats.commands_accepted += 1;
        } else {
            self.stats.commands_rejected += 1;
        }
        if self.log.len() >= DECISION_LOG_CAPACITY {
            self.log.pop_front();
        }
        self.log.push_back(decision);
    }

    /// Drop profiles of countries that no longer exist (capitulated, merged).
    pub fn retain_countries(&mut self, alive: impl Fn(&str) -> bool) {
        self.profiles.retain(|tag, _| alive(tag));
    }
}
