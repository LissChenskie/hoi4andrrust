//! Headless simulation timing. Ignored by default: run with
//! `cargo test --release perf_probe -- --ignored --nocapture`.
//! Synthetic worlds only: no divisions, no map graph and no rendering, so the
//! numbers are a lower bound for the real game, not a measurement of it.
use crate::construction::BuildingRules;
use crate::production::ProductionRules;
use crate::research::{ResearchCatalogue, ResearchRules};
use crate::session::GameSession;
use crate::state::StateEconomy;
use crate::world::World;
use std::collections::HashMap;
use std::time::Instant;

fn session(countries: u32, states_per_country: u32) -> GameSession {
    let mut province_to_state = HashMap::new();
    let mut state_owner = HashMap::new();
    let mut economy = HashMap::new();
    let mut population = HashMap::new();
    let mut id = 1u32;
    for c in 0..countries {
        for _ in 0..states_per_country {
            province_to_state.insert(id * 10 + 1, id);
            state_owner.insert(id, format!("C{c:02}"));
            economy.insert(
                id,
                StateEconomy {
                    civilian_factories: 6,
                    military_factories: 2,
                    ..Default::default()
                },
            );
            population.insert(id, 100);
            id += 1;
        }
    }
    let world = World::from_map_data(
        &province_to_state,
        &state_owner,
        &HashMap::new(),
        &economy,
        &population,
    );
    let mut s = GameSession::new(world);
    s.clock.paused = false;
    s
}

#[test]
#[ignore]
fn perf_probe_simulation_scaling() {
    let (cat, rr) = (ResearchCatalogue::default(), ResearchRules::default());
    let (br, pr) = (BuildingRules::default(), ProductionRules::default());
    for (countries, states) in [(10, 5), (40, 8), (80, 10)] {
        let mut s = session(countries, states);
        let start = Instant::now();
        let hours = 24 * 30;
        for _ in 0..hours {
            s.advance_one_hour(&br, &pr, &cat, &rr);
        }
        let total = start.elapsed().as_secs_f64() * 1000.0;
        println!(
            "{countries:>3} countries / {:>4} states: {:.3} ms per game hour, {:.2} ms per game day",
            countries * states,
            total / f64::from(hours),
            total / 30.0
        );
    }
}

#[test]
#[ignore]
fn perf_probe_country_economy_cost() {
    for (countries, states) in [(10, 5), (40, 8), (80, 10)] {
        let s = session(countries, states);
        let tags: Vec<String> = s.world.countries.keys().cloned().collect();
        let start = Instant::now();
        let rounds = 200;
        for _ in 0..rounds {
            for t in &tags {
                std::hint::black_box(s.world.country_economy(t));
            }
        }
        let per_pass = start.elapsed().as_secs_f64() * 1000.0 / f64::from(rounds);
        println!(
            "{countries:>3} countries / {:>4} states: all-country economy pass = {per_pass:.3} ms",
            countries * states
        );
    }
}
