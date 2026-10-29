//! Campaign territorial rights, faction membership, capitulation and peace settlements.
//! This is an explicit, deterministic approximation of HOI4 1.19.3, not a
//! claim that the closed-source game's internal war/peace formulas are known.
//! Surrender is measured from *controlled core victory points*, NOT warscore.
//! War participation and the price of peace demands are separate ledgers.
use crate::game::GameDate;
use crate::research::ResearchWorld;
use crate::warfare::Warfare;
use crate::world::World;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateRights {
    pub cores: BTreeSet<String>,
    pub claims: BTreeSet<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Faction {
    pub name: String,
    pub leader: String,
    pub members: BTreeSet<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct War {
    pub id: u64,
    pub attackers: BTreeSet<String>,
    pub defenders: BTreeSet<String>,
    pub capitulated: BTreeSet<String>,
    /// Currently a deterministic, occupation-based participation proxy.
    pub contribution: BTreeMap<String, f64>,
    pub conference_id: Option<u64>,
}
impl War {
    fn side(&self, tag: &str) -> Option<bool> {
        if self.attackers.contains(tag) {
            Some(true)
        } else if self.defenders.contains(tag) {
            Some(false)
        } else {
            None
        }
    }
    pub fn enemies<'a>(&'a self, tag: &str) -> Option<&'a BTreeSet<String>> {
        match self.side(tag)? {
            true => Some(&self.defenders),
            false => Some(&self.attackers),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SurrenderStatus {
    pub controlled_vp: f64,
    pub total_vp: f64,
    pub progress: f64,
    pub limit: f64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConferencePhase {
    Demands,
    Finalized,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeaceAction {
    Annex,
    ReturnCore { country: String },
    Liberate { country: String },
    Puppet { country: String },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeaceDemand {
    pub claimant: String,
    pub state: u32,
    pub action: PeaceAction,
    pub price: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeaceConference {
    pub id: u64,
    pub war_id: u64,
    pub winners: BTreeSet<String>,
    pub losers: BTreeSet<String>,
    pub points: BTreeMap<String, u64>,
    pub spent: BTreeMap<String, u64>,
    pub demands: Vec<PeaceDemand>,
    pub phase: ConferencePhase,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PeaceRules {
    /// HOI4 commonly uses 80% lost core VP as a base surrender threshold;
    /// versioned data and country modifiers must override this approximation.
    pub base_surrender: f64,
    pub min_surrender: f64,
    pub max_surrender: f64,
    /// Not a verified 1.19.3 peace conference formula: an explicit fallback.
    pub state_price_per_vp: f64,
    pub state_price_per_factory: f64,
    pub minimum_state_price: u64,
    pub core_price_multiplier: f64,
    pub claim_price_multiplier: f64,
}
impl Default for PeaceRules {
    fn default() -> Self {
        Self {
            base_surrender: 0.80,
            min_surrender: 0.20,
            max_surrender: 0.99,
            state_price_per_vp: 2.0,
            state_price_per_factory: 3.0,
            minimum_state_price: 10,
            core_price_multiplier: 0.25,
            claim_price_multiplier: 0.75,
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PeaceSystem {
    pub rights: BTreeMap<u32, StateRights>,
    pub factions: BTreeMap<String, Faction>,
    pub wars: BTreeMap<u64, War>,
    pub capitulated: BTreeSet<String>,
    pub conferences: BTreeMap<u64, PeaceConference>,
    /// Subject -> overlord, kept independently of diplomatic faction membership.
    pub subjects: BTreeMap<String, String>,
    pub rules: PeaceRules,
    pub diagnostics: Vec<String>,
    pub next_war_id: u64,
    pub next_conference_id: u64,
    pub last_checked_control_revision: Option<u64>,
}
impl PeaceSystem {
    pub fn add_core(&mut self, world: &World, state: u32, country: &str) -> Result<(), String> {
        if !world.states.contains_key(&state) || country.is_empty() {
            return Err("Unknown core state or empty country".into());
        }
        self.rights
            .entry(state)
            .or_default()
            .cores
            .insert(country.into());
        Ok(())
    }
    pub fn remove_core(&mut self, state: u32, country: &str) {
        if let Some(r) = self.rights.get_mut(&state) {
            r.cores.remove(country);
        }
    }
    pub fn add_claim(&mut self, world: &World, state: u32, country: &str) -> Result<(), String> {
        if !world.states.contains_key(&state) || country.is_empty() {
            return Err("Unknown claim state or empty country".into());
        }
        self.rights
            .entry(state)
            .or_default()
            .claims
            .insert(country.into());
        Ok(())
    }
    pub fn is_core(&self, state: u32, country: &str) -> bool {
        self.rights
            .get(&state)
            .is_some_and(|r| r.cores.contains(country))
    }
    pub fn is_claimed(&self, state: u32, country: &str) -> bool {
        self.rights
            .get(&state)
            .is_some_and(|r| r.claims.contains(country))
    }
    pub fn faction_of(&self, tag: &str) -> Option<&Faction> {
        self.factions.values().find(|f| f.members.contains(tag))
    }
    pub fn create_faction(
        &mut self,
        world: &World,
        name: &str,
        leader: &str,
    ) -> Result<(), String> {
        if name.trim().is_empty()
            || !world.countries.contains_key(leader)
            || self.factions.contains_key(name)
            || self.faction_of(leader).is_some()
        {
            return Err("Invalid faction name or leader already in a faction".into());
        }
        self.factions.insert(
            name.into(),
            Faction {
                name: name.into(),
                leader: leader.into(),
                members: BTreeSet::from([leader.into()]),
            },
        );
        Ok(())
    }
    pub fn join_faction(&mut self, world: &World, name: &str, tag: &str) -> Result<(), String> {
        if !world.countries.contains_key(tag) || self.faction_of(tag).is_some() {
            return Err("Unknown country or already in faction".into());
        }
        let f = self.factions.get_mut(name).ok_or("Unknown faction")?;
        f.members.insert(tag.into());
        Ok(())
    }
    pub fn leave_faction(&mut self, tag: &str) -> Result<(), String> {
        let name = self
            .faction_of(tag)
            .ok_or("Country is not in a faction")?
            .name
            .clone();
        let f = self.factions.get_mut(&name).unwrap();
        f.members.remove(tag);
        if f.members.is_empty() {
            self.factions.remove(&name);
        } else if f.leader == tag {
            f.leader = f.members.iter().next().unwrap().clone();
        }
        Ok(())
    }
    /// Faction membership alone does NOT cause automatic participation in each
    /// member's wars. Joining is an explicit call to arms.
    pub fn call_to_war(
        &mut self,
        world: &World,
        military: &mut Warfare,
        war_id: u64,
        caller: &str,
        member: &str,
    ) -> Result<(), String> {
        if self.faction_of(caller).map(|f| &f.name) != self.faction_of(member).map(|f| &f.name)
            || self.faction_of(caller).is_none()
            || caller == member
        {
            return Err("Call requires distinct members of the same faction".into());
        }
        if !world.countries.contains_key(member) {
            return Err("Unknown called country".into());
        }
        let war = self.wars.get(&war_id).ok_or("Unknown war")?;
        let side = war.side(caller).ok_or("Caller is not in this war")?;
        if war.side(member).is_some() {
            return Err("Country already participates in the war".into());
        }
        let enemies = war.enemies(caller).unwrap().clone();
        if enemies.iter().any(|enemy| {
            self.faction_of(member)
                .is_some_and(|f| f.members.contains(enemy))
        }) {
            return Err("Cannot call a country against its own faction".into());
        }
        let war = self.wars.get_mut(&war_id).unwrap();
        if side {
            war.attackers.insert(member.into());
        } else {
            war.defenders.insert(member.into());
        }
        for enemy in enemies {
            if !military.at_war(member, &enemy) {
                military.declare_war(member, &enemy, world)?;
            }
        }
        Ok(())
    }
    pub fn sync_wars(&mut self, military: &Warfare) {
        for (a, b) in &military.wars {
            if self
                .wars
                .values()
                .any(|w| w.side(a).is_some() && w.side(b).is_some())
            {
                continue;
            }
            self.next_war_id += 1;
            self.wars.insert(
                self.next_war_id,
                War {
                    id: self.next_war_id,
                    attackers: BTreeSet::from([a.clone()]),
                    defenders: BTreeSet::from([b.clone()]),
                    capitulated: BTreeSet::new(),
                    contribution: BTreeMap::new(),
                    conference_id: None,
                },
            );
        }
    }
    pub fn surrender_status(
        &self,
        world: &World,
        military: &Warfare,
        research: &ResearchWorld,
        country: &str,
    ) -> Option<SurrenderStatus> {
        if !self
            .wars
            .values()
            .any(|w| w.side(country).is_some() && w.conference_id.is_none())
        {
            return None;
        }
        let mut total = 0.0;
        let mut held = 0.0;
        for s in world.states.values() {
            if s.owner.as_deref() != Some(country) || !self.is_core(s.id, country) {
                continue;
            }
            for province_id in &s.province_ids {
                // This low base value represents otherwise unvalued core provinces.
                // Not claimed as exact HOI4 1.19.3 weighting.
                let value = 0.1
                    + world
                        .geography
                        .provinces
                        .get(province_id)
                        .map_or(0.0, |g| g.victory_points.max(0.0));
                total += value;
                let controller = world.controller_of_province(*province_id);
                let occupied_by_enemy = controller.is_some_and(|other| {
                    military.at_war(country, other)
                        || self.wars.values().any(|w| {
                            w.enemies(country)
                                .is_some_and(|enemies| enemies.contains(other))
                        })
                });
                if !occupied_by_enemy {
                    held += value;
                }
            }
        }
        if total <= 0.0 {
            return None;
        } // Never invent core rights if history is missing.
        let c = research.country(country);
        let modifier = c.map_or(0.0, |r| r.modifier("surrender_limit"));
        let forced = c.map_or(0.0, |r| r.modifier("forced_surrender_limit"));
        let limit = (self.rules.base_surrender + modifier)
            .clamp(self.rules.min_surrender, self.rules.max_surrender)
            .mul_add(1.0, forced)
            .clamp(0.01, 1.0);
        Some(SurrenderStatus {
            controlled_vp: held,
            total_vp: total,
            progress: (1.0 - held / total).clamp(0.0, 1.0),
            limit,
        })
    }
    fn leading_occupier(&self, war: &War, loser: &str, world: &World) -> Option<String> {
        let enemies = war.enemies(loser)?;
        let mut scores: BTreeMap<&str, f64> = enemies.iter().map(|s| (s.as_str(), 0.0)).collect();
        for s in world
            .states
            .values()
            .filter(|s| s.owner.as_deref() == Some(loser))
        {
            for id in &s.province_ids {
                if let Some(tag) = world.controller_of_province(*id) {
                    if let Some(v) = scores.get_mut(tag) {
                        *v += world
                            .geography
                            .provinces
                            .get(id)
                            .map_or(0.1, |p| p.victory_points.max(0.0) + 0.1);
                    }
                }
            }
        }
        scores
            .into_iter()
            .max_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.0.cmp(a.0)))
            .map(|(tag, _)| tag.to_owned())
    }
    pub fn tick(
        &mut self,
        world: &mut World,
        military: &mut Warfare,
        research: &ResearchWorld,
        elapsed_hours: u64,
    ) {
        self.sync_wars(military);
        if self.last_checked_control_revision == Some(world.control_revision)
            && !elapsed_hours.is_multiple_of(24)
        {
            return;
        }
        self.last_checked_control_revision = Some(world.control_revision);
        let ids: Vec<u64> = self.wars.keys().copied().collect();
        for id in ids {
            let (participants, ready) = match self.wars.get(&id) {
                Some(w) => (
                    w.attackers
                        .iter()
                        .chain(&w.defenders)
                        .cloned()
                        .collect::<Vec<_>>(),
                    w.conference_id.is_none(),
                ),
                None => continue,
            };
            if !ready {
                continue;
            }
            for tag in participants {
                if self.capitulated.contains(&tag) {
                    continue;
                }
                let Some(status) = self.surrender_status(world, military, research, &tag) else {
                    continue;
                };
                if status.progress + 1e-9 < status.limit {
                    continue;
                }
                let Some(war) = self.wars.get(&id) else {
                    continue;
                };
                let Some(winner) = self.leading_occupier(war, &tag, world) else {
                    continue;
                };
                self.capitulated.insert(tag.clone());
                if let Some(war) = self.wars.get_mut(&id) {
                    war.capitulated.insert(tag.clone());
                }
                // A capitulation changes *control*, never legal state ownership
                // or national core rights. Victory is resolved at peace.
                let provinces: Vec<u32> = world
                    .states
                    .values()
                    .filter(|s| s.owner.as_deref() == Some(tag.as_str()))
                    .flat_map(|s| s.province_ids.iter().copied())
                    .filter(|p| world.controller_of_province(*p) == Some(tag.as_str()))
                    .collect();
                for province in provinces {
                    world.set_province_controller(province, Some(&winner));
                }
                // Save structure intact but deactivate surrendered formations.
                for d in military.divisions.values_mut().filter(|d| d.country == tag) {
                    d.destroyed = true;
                    d.battle = None;
                    d.destination = None;
                    d.waypoints.clear();
                }
                military.notice(format!("{tag} capitulated to {winner}"));
            }
            let Some(war) = self.wars.get(&id) else {
                continue;
            };
            let attack_lost = war.attackers.iter().all(|x| war.capitulated.contains(x));
            let defend_lost = war.defenders.iter().all(|x| war.capitulated.contains(x));
            // A participant's capitulation is not itself a peace conference.
            // Only an entirely defeated belligerent side opens settlement.
            if attack_lost ^ defend_lost {
                self.open_conference(id, world, military);
            }
        }
        self.last_checked_control_revision = Some(world.control_revision);
    }
    fn open_conference(&mut self, war_id: u64, world: &World, military: &Warfare) {
        let Some(w) = self.wars.get(&war_id) else {
            return;
        };
        if w.conference_id.is_some() {
            return;
        }
        let attack_lost = w.attackers.iter().all(|x| w.capitulated.contains(x));
        let (winners, losers) = if attack_lost {
            (w.defenders.clone(), w.attackers.clone())
        } else {
            (w.attackers.clone(), w.defenders.clone())
        };
        // Occupation and combat-loss proxy; original 1.19.3 uses additional
        // factors and its private peace-conference score/turn algorithms.
        let mut weights = BTreeMap::new();
        for tag in &winners {
            let occupied = world
                .states
                .values()
                .filter(|s| s.owner.as_ref().is_some_and(|o| losers.contains(o)))
                .flat_map(|s| &s.province_ids)
                .filter(|p| world.controller_of_province(**p) == Some(tag.as_str()))
                .map(|p| {
                    world
                        .geography
                        .provinces
                        .get(p)
                        .map_or(0.1, |x| x.victory_points.max(0.0) + 0.1)
                })
                .sum::<f64>();
            let casualties = military
                .divisions
                .values()
                .filter(|d| d.country == *tag)
                .map(|d| d.recent_losses as f64)
                .sum::<f64>();
            weights.insert(tag.clone(), 1.0 + occupied + casualties * 0.01);
        }
        let sum: f64 = weights.values().sum();
        self.next_conference_id += 1;
        let cid = self.next_conference_id;
        let pool = (world
            .states
            .values()
            .filter(|s| s.owner.as_ref().is_some_and(|o| losers.contains(o)))
            .map(|s| self.base_cost(world, s.id))
            .sum::<u64>()
            * 2)
        .max(1);
        let mut available = BTreeMap::new();
        for (tag, value) in &weights {
            available.insert(
                tag.clone(),
                ((pool as f64 * value / sum).floor() as u64).max(1),
            );
        }
        self.conferences.insert(
            cid,
            PeaceConference {
                id: cid,
                war_id,
                winners,
                losers,
                points: available,
                spent: BTreeMap::new(),
                demands: vec![],
                phase: ConferencePhase::Demands,
            },
        );
        self.wars.get_mut(&war_id).unwrap().conference_id = Some(cid);
    }
    fn base_cost(&self, world: &World, state: u32) -> u64 {
        let Some(s) = world.states.get(&state) else {
            return self.rules.minimum_state_price;
        };
        let vp: f64 = s
            .province_ids
            .iter()
            .map(|p| {
                world
                    .geography
                    .provinces
                    .get(p)
                    .map_or(0.1, |g| g.victory_points.max(0.0) + 0.1)
            })
            .sum();
        let buildings = u32::from(s.economy.civilian_factories)
            + u32::from(s.economy.military_factories)
            + u32::from(s.economy.dockyards);
        (vp * self.rules.state_price_per_vp
            + f64::from(buildings) * self.rules.state_price_per_factory)
            .ceil()
            .max(self.rules.minimum_state_price as f64) as u64
    }
    pub fn demand_price(
        &self,
        world: &World,
        state: u32,
        claimant: &str,
        action: &PeaceAction,
    ) -> Result<u64, String> {
        if !world.states.contains_key(&state) {
            return Err("Unknown state".into());
        }
        let beneficiary = match action {
            PeaceAction::Annex => claimant,
            PeaceAction::ReturnCore { country }
            | PeaceAction::Liberate { country }
            | PeaceAction::Puppet { country } => country,
        };
        let mut modifier = 1.0;
        if self.is_core(state, beneficiary) {
            modifier *= self.rules.core_price_multiplier;
        } else if self.is_claimed(state, beneficiary) {
            modifier *= self.rules.claim_price_multiplier;
        }
        Ok(((self.base_cost(world, state) as f64 * modifier).ceil() as u64).max(1))
    }
    /// Reserve a demand without changing the map; no claim can spend more than
    /// the claimant's ledger. The final conference commit applies transfers.
    pub fn submit_demand(
        &mut self,
        world: &World,
        conference_id: u64,
        claimant: &str,
        state: u32,
        action: PeaceAction,
    ) -> Result<u64, String> {
        let conference = self
            .conferences
            .get(&conference_id)
            .ok_or("Unknown conference")?;
        if conference.phase != ConferencePhase::Demands || !conference.winners.contains(claimant) {
            return Err("Conference closed or claimant is not a victor".into());
        }
        let owned_by_loser = world
            .states
            .get(&state)
            .and_then(|s| s.owner.as_ref())
            .is_some_and(|owner| conference.losers.contains(owner));
        if !owned_by_loser || conference.demands.iter().any(|d| d.state == state) {
            return Err("State cannot be demanded twice or is not a losing state".into());
        }
        match &action {
            PeaceAction::Annex => {}
            PeaceAction::ReturnCore { country } => {
                if !self.is_core(state, country) {
                    return Err("Target has no core on the state".into());
                }
                if !world.countries.contains_key(country) && country.len() != 3 {
                    return Err("Invalid recipient country".into());
                }
            }
            PeaceAction::Liberate { country } | PeaceAction::Puppet { country } => {
                if !self.is_core(state, country) || country.len() != 3 {
                    return Err(
                        "Liberation and puppeting require a recognized three-letter core tag"
                            .into(),
                    );
                }
            }
        }
        let price = self.demand_price(world, state, claimant, &action)?;
        let c = self.conferences.get_mut(&conference_id).unwrap();
        let remaining = c
            .points
            .get(claimant)
            .copied()
            .unwrap_or(0)
            .saturating_sub(c.spent.get(claimant).copied().unwrap_or(0));
        if remaining < price {
            return Err("Insufficient conference points".into());
        }
        *c.spent.entry(claimant.into()).or_default() += price;
        c.demands.push(PeaceDemand {
            claimant: claimant.into(),
            state,
            action,
            price,
        });
        Ok(price)
    }
    /// Book a group of states as a single indivisible peace settlement.
    /// Multiple states with the same national-rights recipient can form one
    /// puppet, while a failure cannot leave only half its territory claimed.
    pub fn submit_demand_batch(
        &mut self,
        world: &World,
        conference_id: u64,
        claimant: &str,
        states: &BTreeSet<u32>,
        action: PeaceAction,
    ) -> Result<u64, String> {
        if states.is_empty() {
            return Err("Select at least one state".into());
        }
        let mut staged = self.clone();
        let mut total = 0u64;
        for &state in states {
            total = total
                .checked_add(staged.submit_demand(
                    world,
                    conference_id,
                    claimant,
                    state,
                    action.clone(),
                )?)
                .ok_or("Conference price overflow")?;
        }
        *self = staged;
        Ok(total)
    }
    /// Withdraw a previously reserved demand without changing territory.
    /// Only the author may reclaim its own points.
    pub fn withdraw_latest_demand(
        &mut self,
        conference_id: u64,
        claimant: &str,
    ) -> Result<PeaceDemand, String> {
        let conference = self
            .conferences
            .get_mut(&conference_id)
            .ok_or("Unknown conference")?;
        if conference.phase != ConferencePhase::Demands || !conference.winners.contains(claimant) {
            return Err("Cannot withdraw demands at this conference".into());
        }
        let pos = conference
            .demands
            .iter()
            .rposition(|d| d.claimant == claimant)
            .ok_or("No own demands to undo")?;
        let demand = conference.demands.remove(pos);
        let spent = conference.spent.entry(claimant.into()).or_default();
        *spent = spent.saturating_sub(demand.price);
        Ok(demand)
    }
    /// Atomically finalize a conference: validates against a cloned world and
    /// only commits successful transfers. Unclaimed states return to their owner.
    pub fn finalize(
        &mut self,
        world: &mut World,
        military: &mut Warfare,
        conference_id: u64,
    ) -> Result<(), String> {
        let c = self
            .conferences
            .get(&conference_id)
            .ok_or("Unknown conference")?
            .clone();
        if c.phase != ConferencePhase::Demands {
            return Err("Conference already finalized".into());
        }
        let mut candidate = world.clone();
        let mut new_subjects = self.subjects.clone();
        for d in &c.demands {
            if !c.losers.contains(
                candidate
                    .states
                    .get(&d.state)
                    .and_then(|s| s.owner.as_ref())
                    .ok_or("State owner changed during conference")?,
            ) {
                return Err("Conference state changed ownership".into());
            }
            let recipient = match &d.action {
                PeaceAction::Annex => d.claimant.as_str(),
                PeaceAction::ReturnCore { country }
                | PeaceAction::Liberate { country }
                | PeaceAction::Puppet { country } => country.as_str(),
            };
            if !candidate.set_state_owner(d.state, Some(recipient)) {
                return Err("Unable to transfer peace demand".into());
            }
            let provinces = candidate.states[&d.state].province_ids.clone();
            for id in provinces {
                candidate.set_province_controller(id, Some(recipient));
            }
            if let PeaceAction::Puppet { country } = &d.action {
                if country == &d.claimant {
                    return Err("Country cannot puppet itself".into());
                }
                new_subjects.insert(country.clone(), d.claimant.clone());
            }
        }
        // Restore control for unclaimed territories; ownership does not change.
        for state in candidate
            .states
            .values()
            .filter(|s| s.owner.as_ref().is_some_and(|o| c.losers.contains(o)))
            .map(|s| (s.owner.clone().unwrap(), s.province_ids.clone()))
            .collect::<Vec<_>>()
        {
            for p in state.1 {
                candidate.set_province_controller(p, Some(&state.0));
            }
        }
        *world = candidate;
        self.subjects = new_subjects;
        let w = self
            .wars
            .remove(&c.war_id)
            .ok_or("Conference war missing")?;
        // Other wars may still involve these countries; never globally clear
        // every military relation of a participant.
        for a in &w.attackers {
            for b in &w.defenders {
                let key = if a < b {
                    (a.clone(), b.clone())
                } else {
                    (b.clone(), a.clone())
                };
                military.wars.remove(&key);
            }
        }
        self.capitulated
            .retain(|tag| self.wars.values().any(|w| w.capitulated.contains(tag)));
        self.conferences.get_mut(&conference_id).unwrap().phase = ConferencePhase::Finalized;
        self.last_checked_control_revision = None;
        Ok(())
    }
    /// Import explicitly declared historical core/claim rights at a bookmark.
    /// Owner is NOT implicitly a core; only add_core_of/add_claim_by count.
    pub fn import_state_history(
        &mut self,
        root: &Path,
        bookmark: GameDate,
    ) -> Result<usize, String> {
        let dir = root.join("history/states");
        if !dir.is_dir() {
            return Ok(0);
        }
        let mut paths = std::fs::read_dir(dir)
            .map_err(|e| e.to_string())?
            .map(|p| p.map(|e| e.path()).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        paths.sort();
        let mut count = 0;
        for path in paths
            .into_iter()
            .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        {
            let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let parsed = crate::pdx::parse_pdx_compatible(&raw)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            for state in parsed.entries.iter().filter(|e| e.key == "state") {
                let Some(b) = state.block() else { continue };
                let Some(id) = crate::pdx::atom(b, "id").and_then(|x| x.parse::<u32>().ok()) else {
                    continue;
                };
                let Some(history) = crate::pdx::block(b, "history") else {
                    continue;
                };
                let mut scheduled = Vec::new();
                let base = history
                    .iter()
                    .filter(|e| crate::research_script::parse_date(&e.key).is_none());
                for e in base {
                    scheduled.push((
                        GameDate {
                            year: 0,
                            month: 1,
                            day: 1,
                        },
                        e.clone(),
                    ));
                }
                for e in history {
                    if let Some(date) = crate::research_script::parse_date(&e.key) {
                        if date <= bookmark {
                            if let Some(nested) = e.block() {
                                for effect in nested {
                                    scheduled.push((date, effect.clone()));
                                }
                            }
                        }
                    }
                }
                scheduled.sort_by_key(|(date, _)| *date);
                let rights = self.rights.entry(id).or_default();
                for (_, e) in scheduled {
                    if let Some(tag) = e.atom() {
                        match e.key.as_str() {
                            "add_core_of" => {
                                rights.cores.insert(tag.into());
                            }
                            "remove_core_of" => {
                                rights.cores.remove(tag);
                            }
                            "add_claim_by" | "add_claim_of" => {
                                rights.claims.insert(tag.into());
                            }
                            "remove_claim_by" | "remove_claim_of" => {
                                rights.claims.remove(tag);
                            }
                            _ => {}
                        }
                    }
                }
                count += 1;
            }
        }
        Ok(count)
    }
    pub fn validate(&self, world: &World) -> Result<(), String> {
        for (id, rights) in &self.rights {
            if !world.states.contains_key(id)
                || rights
                    .cores
                    .iter()
                    .chain(&rights.claims)
                    .any(|c| c.is_empty())
            {
                return Err("Invalid state rights".into());
            }
        }
        for f in self.factions.values() {
            if f.members.is_empty()
                || !f.members.contains(&f.leader)
                || f.members
                    .iter()
                    .any(|tag| !world.countries.contains_key(tag))
            {
                return Err("Invalid faction".into());
            }
        }
        let mut seen = BTreeSet::new();
        for f in self.factions.values() {
            for tag in &f.members {
                if !seen.insert(tag) {
                    return Err("Country belongs to multiple factions".into());
                }
            }
        }
        for c in self.conferences.values() {
            if c.winners.is_empty()
                || c.losers.is_empty()
                || c.winners.iter().any(|tag| c.losers.contains(tag))
                || c.demands
                    .iter()
                    .map(|d| d.state)
                    .collect::<BTreeSet<_>>()
                    .len()
                    != c.demands.len()
                || c.spent
                    .iter()
                    .any(|(tag, spent)| *spent > c.points.get(tag).copied().unwrap_or(0))
            {
                return Err("Invalid peace conference".into());
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    fn world() -> World {
        let mut w = World::from_map_data(
            &HashMap::from([(1, 1), (2, 1), (3, 2), (4, 2)]),
            &HashMap::from([(1, "AAA".into()), (2, "BBB".into())]),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );
        w.geography = crate::map_graph::MapGraph::from_rgba(
            4, 1,
            &[1,1,1,255,2,1,1,255,3,1,1,255,4,1,1,255],
            "1;1;1;1;land;false;plains;1\n2;2;1;1;land;false;plains;1\n3;3;1;1;land;false;plains;1\n4;4;1;1;land;false;plains;1", false,
        ).unwrap();
        w.geography.provinces.get_mut(&3).unwrap().victory_points = 10.0;
        w.geography.provinces.get_mut(&4).unwrap().victory_points = 10.0;
        w
    }
    #[test]
    fn core_rights_are_not_granted_by_annexation_or_occupation() {
        let mut w = world();
        let mut peace = PeaceSystem::default();
        peace.add_core(&w, 2, "BBB").unwrap();
        assert!(!peace.is_core(2, "AAA"));
        w.set_state_owner(2, Some("AAA"));
        assert!(peace.is_core(2, "BBB"));
        assert!(!peace.is_core(2, "AAA"));
    }
    #[test]
    fn multiple_countries_have_independent_cores_on_the_same_state() {
        let w = world();
        let mut peace = PeaceSystem::default();
        for country in ["AAA", "BBB", "UKR"] {
            peace.add_core(&w, 2, country).unwrap();
        }
        assert_eq!(peace.rights[&2].cores.len(), 3);
        peace.remove_core(2, "AAA");
        assert!(peace.is_core(2, "BBB"));
        assert!(peace.is_core(2, "UKR"));
    }
    #[test]
    fn puppet_demands_for_multiple_core_states_are_atomic() {
        let mut w = world();
        let mut p = PeaceSystem::default();
        for id in [1, 2] {
            p.add_core(&w, id, "UKR").unwrap();
        }
        p.add_core(&w, 2, "BBB").unwrap();
        let mut m = Warfare::default();
        m.declare_war("AAA", "BBB", &w).unwrap();
        for id in [3, 4] {
            w.set_province_controller(id, Some("AAA"));
        }
        p.tick(&mut w, &mut m, &ResearchWorld::default(), 24);
        let cid = *p.conferences.keys().next().unwrap();
        let before = p.conferences[&cid].demands.len();
        let both = BTreeSet::from([1, 2]);
        assert!(
            p.submit_demand_batch(
                &w,
                cid,
                "AAA",
                &both,
                PeaceAction::Puppet {
                    country: "UKR".into()
                }
            )
            .is_err()
        );
        assert_eq!(p.conferences[&cid].demands.len(), before);
        assert_eq!(
            p.conferences[&cid].spent.get("AAA").copied().unwrap_or(0),
            0
        );
        assert!(
            p.submit_demand_batch(
                &w,
                cid,
                "AAA",
                &BTreeSet::from([2]),
                PeaceAction::Puppet {
                    country: "UKR".into()
                }
            )
            .is_ok()
        );
        p.finalize(&mut w, &mut m, cid).unwrap();
        assert_eq!(w.states[&2].owner.as_deref(), Some("UKR"));
        assert_eq!(p.subjects.get("UKR").map(String::as_str), Some("AAA"));
        assert!(p.is_core(2, "BBB"));
        assert!(p.is_core(2, "UKR"));
    }
    #[test]
    fn faction_membership_is_not_an_automatic_declaration() {
        let w = world();
        let mut p = PeaceSystem::default();
        let mut military = Warfare::default();
        p.create_faction(&w, "Test", "AAA").unwrap();
        p.join_faction(&w, "Test", "BBB").unwrap();
        assert!(military.wars.is_empty());
        military.declare_war("AAA", "BBB", &w).unwrap();
        assert!(p.call_to_war(&w, &mut military, 99, "AAA", "BBB").is_err());
    }
    #[test]
    fn capitulation_uses_core_vp_and_opens_conference_only_on_side_defeat() {
        let mut w = world();
        let mut p = PeaceSystem::default();
        p.add_core(&w, 2, "BBB").unwrap();
        let mut m = Warfare::default();
        m.declare_war("AAA", "BBB", &w).unwrap();
        p.tick(&mut w, &mut m, &ResearchWorld::default(), 24);
        assert!(p.conferences.is_empty());
        w.set_province_controller(3, Some("AAA"));
        p.tick(&mut w, &mut m, &ResearchWorld::default(), 25);
        assert!(p.conferences.is_empty()); // 50% VP does not meet surrender limit.
        w.set_province_controller(4, Some("AAA"));
        p.tick(&mut w, &mut m, &ResearchWorld::default(), 26);
        assert!(p.capitulated.contains("BBB"));
        assert_eq!(p.conferences.len(), 1);
        assert_eq!(w.states[&2].owner.as_deref(), Some("BBB"));
    }
    #[test]
    fn demands_spend_points_and_change_ownership_only_at_finalization() {
        let mut w = world();
        let mut p = PeaceSystem::default();
        p.add_core(&w, 2, "BBB").unwrap();
        let mut m = Warfare::default();
        m.declare_war("AAA", "BBB", &w).unwrap();
        for province in [3, 4] {
            w.set_province_controller(province, Some("AAA"));
        }
        p.tick(&mut w, &mut m, &ResearchWorld::default(), 24);
        let id = *p.conferences.keys().next().unwrap();
        let price = p
            .submit_demand(&w, id, "AAA", 2, PeaceAction::Annex)
            .unwrap();
        assert!(price > 0);
        assert_eq!(w.states[&2].owner.as_deref(), Some("BBB"));
        assert!(
            p.submit_demand(&w, id, "AAA", 2, PeaceAction::Annex)
                .is_err()
        );
        p.finalize(&mut w, &mut m, id).unwrap();
        assert_eq!(w.states[&2].owner.as_deref(), Some("AAA"));
        assert_eq!(w.controller_of_province(4), Some("AAA"));
        assert!(!m.at_war("AAA", "BBB"));
        assert!(!p.is_core(2, "AAA"));
        assert!(p.is_core(2, "BBB"));
    }
}
