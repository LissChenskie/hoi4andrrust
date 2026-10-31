//! Research planner.
//!
//! Ideas taken from the supplied Expert AI sources: a ranked queue that is
//! rebuilt whenever the researched set changes, tech chains are valued by what
//! they unlock, and one military specialisation steers research. Sheep's Mod
//! contributes the use of conditional weights (war / threat / industry) rather
//! than fixed lists. The scoring below is our own and is **not** a numeric copy
//! of either mod; constants are provisional and need tuning against real data.
//!
//! Script `ai_weight == 0` means "the AI never picks this", as in the original.
use std::collections::BTreeMap;

use super::commands::start_research;
use super::observation::CountryObservation;
use super::scheduler;
use super::state::{AiState, MAX_REASONS, Posture, Specialisation};
use crate::game::GameDate;
use crate::research::{CountryResearch, ResearchCatalogue, ResearchRules, Technology, years_ahead};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Economy,
    Land,
    Armor,
    Motorized,
    Air,
    Naval,
    Other,
}

/// Pattern based, because category names come from the imported game data.
fn class_of(category: &str) -> Class {
    let c = category.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| c.contains(n));
    if has(&[
        "industry",
        "construction",
        "production",
        "electronics",
        "excavation",
    ]) {
        Class::Economy
    } else if has(&["armor", "tank"]) {
        Class::Armor
    } else if has(&["motorized", "mechanized"]) {
        Class::Motorized
    } else if has(&["air", "fighter", "bomber", "cas_", "rocket"]) {
        Class::Air
    } else if has(&[
        "naval",
        "ship",
        "destroyer",
        "cruiser",
        "battleship",
        "carrier",
        "submarine",
        "convoy",
    ]) {
        Class::Naval
    } else if has(&[
        "infantry",
        "artillery",
        "support",
        "anti_tank",
        "land",
        "engineer",
    ]) {
        Class::Land
    } else {
        Class::Other
    }
}

fn class_weight(
    class: Class,
    spec: Specialisation,
    posture: Posture,
    obs: &CountryObservation,
) -> f64 {
    let war = posture == Posture::War;
    match class {
        Class::Economy => {
            if war {
                1.0
            } else {
                1.5
            }
        }
        Class::Land => {
            if war {
                1.3
            } else {
                1.0
            }
        }
        Class::Armor => {
            (if spec == Specialisation::Armor {
                1.6
            } else {
                0.7
            }) * if war { 1.2 } else { 1.0 }
        }
        Class::Motorized => {
            (if matches!(spec, Specialisation::Motorized | Specialisation::Armor) {
                1.4
            } else {
                0.8
            }) * if war { 1.2 } else { 1.0 }
        }
        Class::Air => {
            if spec == Specialisation::Air {
                1.6
            } else {
                0.8
            }
        }
        Class::Naval => {
            // A country with no dockyard cannot use ship technology effectively.
            if obs.dockyards == 0 {
                0.2
            } else if spec == Specialisation::Navy {
                1.6
            } else {
                0.5
            }
        }
        Class::Other => 1.0,
    }
}

/// Deterministic multiplier in [0.92, 1.08] from (seed, country, tech). It keeps
/// countries from choosing identical orders while staying replayable.
fn jitter(seed: u64, tag: &str, tech: &str) -> f64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ seed;
    for b in tag.bytes().chain([0u8]).chain(tech.bytes()) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    0.92 + 0.16 * ((h % 10_000) as f64 / 9_999.0)
}

fn dependents<'a>(
    cache: &'a mut Option<(String, BTreeMap<String, u32>)>,
    catalogue: &ResearchCatalogue,
) -> &'a BTreeMap<String, u32> {
    let stale = cache
        .as_ref()
        .is_none_or(|(fingerprint, _)| *fingerprint != catalogue.fingerprint);
    if stale {
        let mut counts = BTreeMap::new();
        for tech in catalogue.technologies.values() {
            for parent in &tech.prerequisites {
                *counts.entry(parent.clone()).or_insert(0u32) += 1;
            }
        }
        *cache = Some((catalogue.fingerprint.clone(), counts));
    }
    &cache.as_ref().expect("cache filled above").1
}

fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 1.0;
    }
    values.sort_by(f64::total_cmp);
    values[values.len() / 2].max(1.0)
}

/// Pick a specialisation from the country's real industry. Provisional rules
/// that are re-evaluated every strategic review, never fixed by tag or date.
pub fn choose_specialisation(obs: &CountryObservation) -> (Specialisation, String) {
    if obs.dockyards >= 10 && obs.dockyards * 2 >= obs.military_factories {
        (
            Specialisation::Navy,
            format!(
                "{} dockyards against {} military factories",
                obs.dockyards, obs.military_factories
            ),
        )
    } else if obs.military_factories >= 30 {
        (
            Specialisation::Armor,
            format!(
                "{} military factories support mass armour",
                obs.military_factories
            ),
        )
    } else {
        (
            Specialisation::Infantry,
            format!(
                "{} military factories: infantry and artillery",
                obs.military_factories
            ),
        )
    }
}

fn review_profile(state: &mut AiState, obs: &CountryObservation, hour: u64) {
    let at_war = obs.at_war();
    let due = {
        let p = state.profile_mut(&obs.tag);
        scheduler::strategic_review_due(p, hour) || scheduler::posture_changed(p, at_war)
    };
    if !due {
        return;
    }
    let (spec, why) = choose_specialisation(obs);
    let p = state.profile_mut(&obs.tag);
    p.specialisation = spec;
    p.posture = if at_war { Posture::War } else { Posture::Peace };
    p.last_strategic_review_hour = Some(hour);
    p.reasons.push(format!(
        "hour {hour}: {spec:?} ({why}), posture {:?}",
        p.posture
    ));
    if p.reasons.len() > MAX_REASONS {
        p.reasons.remove(0);
    }
    state.stats.strategic_reviews += 1;
}

/// Score every researchable technology for this country, best first.
pub fn rank(
    state: &mut AiState,
    obs: &CountryObservation,
    catalogue: &ResearchCatalogue,
    country: &CountryResearch,
    date: GameDate,
) -> Vec<(f64, String)> {
    let profile = state.profile_mut(&obs.tag).clone();
    let seed = state.seed;
    let deps = dependents(&mut state.dependents, catalogue);
    let candidates: Vec<&Technology> = catalogue
        .technologies
        .values()
        .filter(|t| t.ai_weight > 0.0 && catalogue.can_research(&t.id, country).is_ok())
        .collect();
    let mut costs: Vec<f64> = candidates.iter().map(|t| t.research_cost).collect();
    let typical_cost = median(&mut costs);
    let mut scored: Vec<(f64, String)> = candidates
        .into_iter()
        .map(|t| {
            let class_mult = t
                .categories
                .iter()
                .map(|c| class_weight(class_of(c), profile.specialisation, profile.posture, obs))
                .fold(None, |best: Option<f64>, w| {
                    Some(best.map_or(w, |b| b.max(w)))
                })
                .unwrap_or(1.0);
            let time = 1.0 / (1.0 + years_ahead(t, date) * 4.0);
            let cost = (typical_cost / t.research_cost.max(1.0))
                .powf(0.35)
                .clamp(0.6, 1.6);
            let chain = 1.0 + 0.1 * f64::from(deps.get(&t.id).copied().unwrap_or(0).min(5));
            // A free focus/idea research bonus is worth using on matching techs.
            let bonus = if country
                .available_bonuses
                .iter()
                .any(|b| b.uses > 0 && b.matches(t))
            {
                1.25
            } else {
                1.0
            };
            let score = t.ai_weight
                * class_mult
                * time
                * cost
                * chain
                * bonus
                * jitter(seed, &obs.tag, &t.id);
            (score, t.id.clone())
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    scored
}

/// Fill every free research slot. Cheap when no slot is free (one pass over the
/// slots), so it is safe to call daily for every AI country.
pub fn plan(
    state: &mut AiState,
    obs: &CountryObservation,
    catalogue: &ResearchCatalogue,
    rules: &ResearchRules,
    country: &mut CountryResearch,
    date: GameDate,
    hour: u64,
) {
    review_profile(state, obs, hour);
    if obs.free_research_slots == 0 {
        return;
    }
    state.stats.research_planner_runs += 1;
    let mut ranked = rank(state, obs, catalogue, country, date);
    for slot in 0..country.slots.len() {
        if country.slots[slot].active.is_some() {
            continue;
        }
        // The validated start() is the judge; try a few candidates, then stop.
        let mut attempts = 0;
        while attempts < 5 && !ranked.is_empty() {
            let (_, id) = ranked.remove(0);
            attempts += 1;
            if start_research(
                state, hour, &obs.tag, country, slot, &id, catalogue, rules, date,
            ) {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(tag: &str, dockyards: u32, mil: u32, enemies: u32) -> CountryObservation {
        CountryObservation {
            tag: tag.into(),
            states: 5,
            civilian_factories: 30,
            military_factories: mil,
            dockyards,
            enemies,
            free_research_slots: 2,
        }
    }

    fn tech(id: &str, cat: &str, year: u32, cost: f64) -> Technology {
        let mut t = Technology::sample(id, year, cost);
        t.categories.insert(cat.into());
        t
    }

    fn catalogue(techs: Vec<Technology>) -> ResearchCatalogue {
        let mut c = ResearchCatalogue::default();
        for t in techs {
            c.insert(t).unwrap();
        }
        c.validate().unwrap();
        c
    }

    const DATE: GameDate = GameDate::START;

    #[test]
    fn peacetime_prefers_economy_and_war_prefers_land() {
        let cat = catalogue(vec![
            tech("factory", "industry", 1936, 100.0),
            tech("rifle", "infantry_weapons", 1936, 100.0),
        ]);
        let rules = ResearchRules::default();
        let country = CountryResearch::new(1, &rules);
        let mut state = AiState::default();
        let peace = obs("AAA", 5, 10, 0);
        state.profile_mut("AAA").posture = Posture::Peace;
        let r = rank(&mut state, &peace, &cat, &country, DATE);
        assert_eq!(r[0].1, "factory");
        let mut state = AiState::default();
        state.profile_mut("AAA").posture = Posture::War;
        let war = obs("AAA", 5, 10, 1);
        let r = rank(&mut state, &war, &cat, &country, DATE);
        assert_eq!(r[0].1, "rifle", "{r:?}");
    }

    #[test]
    fn ship_tech_is_demoted_without_dockyards() {
        let cat = catalogue(vec![
            tech("destroyer_hull", "naval_techs", 1936, 100.0),
            tech("rifle", "infantry_weapons", 1936, 100.0),
        ]);
        let rules = ResearchRules::default();
        let country = CountryResearch::new(1, &rules);
        let mut state = AiState::default();
        let r = rank(&mut state, &obs("AAA", 0, 10, 0), &cat, &country, DATE);
        assert_eq!(r[0].1, "rifle");
    }

    #[test]
    fn zero_script_weight_is_never_chosen() {
        let mut never = tech("never", "industry", 1936, 10.0);
        never.ai_weight = 0.0;
        let cat = catalogue(vec![never, tech("ok", "infantry_weapons", 1936, 100.0)]);
        let rules = ResearchRules::default();
        let country = CountryResearch::new(1, &rules);
        let r = rank(
            &mut AiState::default(),
            &obs("AAA", 5, 10, 0),
            &cat,
            &country,
            DATE,
        );
        assert!(r.iter().all(|(_, id)| id != "never"));
    }

    #[test]
    fn techs_far_ahead_of_time_are_deferred() {
        let cat = catalogue(vec![
            tech("now", "industry", 1936, 100.0),
            tech("future", "industry", 1944, 100.0),
        ]);
        let rules = ResearchRules::default();
        let country = CountryResearch::new(1, &rules);
        let r = rank(
            &mut AiState::default(),
            &obs("AAA", 5, 10, 0),
            &cat,
            &country,
            DATE,
        );
        assert_eq!(r[0].1, "now");
    }

    #[test]
    fn same_seed_replays_and_countries_differ() {
        let cat = catalogue(
            (0..12)
                .map(|i| tech(&format!("t{i}"), "infantry_weapons", 1936, 100.0))
                .collect(),
        );
        let rules = ResearchRules::default();
        let country = CountryResearch::new(1, &rules);
        let order = |tag: &str, seed: u64| {
            rank(
                &mut AiState::with_seed(seed),
                &obs(tag, 5, 10, 0),
                &cat,
                &country,
                DATE,
            )
            .into_iter()
            .map(|(_, id)| id)
            .collect::<Vec<_>>()
        };
        assert_eq!(order("AAA", 7), order("AAA", 7));
        assert_ne!(order("AAA", 7), order("BBB", 7));
    }

    #[test]
    fn plan_fills_free_slots_through_validated_start() {
        let cat = catalogue(vec![
            tech("a", "industry", 1936, 100.0),
            tech("b", "infantry_weapons", 1936, 100.0),
            tech("c", "infantry_weapons", 1936, 100.0),
        ]);
        let rules = ResearchRules::default();
        let mut country = CountryResearch::new(2, &rules);
        let mut state = AiState::default();
        plan(
            &mut state,
            &obs("AAA", 5, 10, 0),
            &cat,
            &rules,
            &mut country,
            DATE,
            0,
        );
        assert!(country.slots.iter().all(|s| s.active.is_some()));
        let ids: Vec<_> = country
            .slots
            .iter()
            .map(|s| s.active.as_ref().unwrap().tech_id.clone())
            .collect();
        assert_ne!(ids[0], ids[1], "two slots must not research the same tech");
        assert_eq!(state.stats.commands_accepted, 2);
        assert_eq!(state.profiles["AAA"].last_strategic_review_hour, Some(0));
    }

    #[test]
    fn no_free_slot_means_no_work() {
        let cat = catalogue(vec![tech("a", "industry", 1936, 100.0)]);
        let rules = ResearchRules::default();
        let mut country = CountryResearch::new(1, &rules);
        let mut state = AiState::default();
        let mut o = obs("AAA", 5, 10, 0);
        o.free_research_slots = 0;
        plan(&mut state, &o, &cat, &rules, &mut country, DATE, 0);
        assert_eq!(state.stats.research_planner_runs, 0);
    }

    #[test]
    fn specialisation_follows_industry_not_tags() {
        assert_eq!(
            choose_specialisation(&obs("XXX", 20, 20, 0)).0,
            Specialisation::Navy
        );
        assert_eq!(
            choose_specialisation(&obs("XXX", 0, 40, 0)).0,
            Specialisation::Armor
        );
        assert_eq!(
            choose_specialisation(&obs("XXX", 0, 5, 0)).0,
            Specialisation::Infantry
        );
    }
}
