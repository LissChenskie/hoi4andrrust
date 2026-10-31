//! Session-level tests: AI countries run inside the real daily simulation.
use crate::construction::BuildingRules;
use crate::game::GameDate;
use crate::production::ProductionRules;
use crate::research::{ResearchCatalogue, ResearchRules, Technology};
use crate::session::GameSession;
use crate::state::StateEconomy;
use crate::world::World;
use std::collections::HashMap;

fn catalogue() -> ResearchCatalogue {
    let mut c = ResearchCatalogue::default();
    for i in 0..10 {
        let cat = if i % 2 == 0 {
            "industry"
        } else {
            "infantry_weapons"
        };
        let mut t = Technology::sample(&format!("t{i}"), 1936, 0.2 + f64::from(i) * 0.02);
        t.categories.insert(cat.into());
        if i >= 4 {
            t.prerequisites.insert(format!("t{}", i - 4));
        }
        c.insert(t).unwrap();
    }
    c.validate().unwrap();
    c.fingerprint = "test".into();
    c
}

fn rules() -> ResearchRules {
    ResearchRules {
        base_research_slots: Some(2),
        ..ResearchRules::default()
    }
}

fn session() -> GameSession {
    let eco = |c, m| StateEconomy {
        civilian_factories: c,
        military_factories: m,
        ..Default::default()
    };
    let world = World::from_map_data(
        &HashMap::from([(11, 1), (21, 2)]),
        &HashMap::from([(1, "AAA".into()), (2, "BBB".into())]),
        &HashMap::new(),
        &HashMap::from([(1, eco(15, 1)), (2, eco(12, 3))]),
        &HashMap::from([(1, 25), (2, 25)]),
    );
    let mut s = GameSession::new(world);
    s.clock.date = GameDate::START;
    s.military.ai_enabled = false;
    s.clock.paused = false;
    // The real game creates country research from the importer with the proper
    // slot count; the daily tick only creates missing entries with zero slots.
    for tag in ["AAA", "BBB"] {
        s.research.country_mut(tag, 2, &rules());
    }
    s
}

fn run_days(s: &mut GameSession, cat: &ResearchCatalogue, days: u32) {
    let (br, pr, rr) = (
        BuildingRules::default(),
        ProductionRules::default(),
        rules(),
    );
    for _ in 0..days * 24 {
        s.advance_one_hour(&br, &pr, cat, &rr);
    }
}

#[test]
fn ai_countries_keep_research_slots_busy_and_finish_techs() {
    let cat = catalogue();
    let mut s = session();
    run_days(&mut s, &cat, 90);
    for tag in ["AAA", "BBB"] {
        let c = &s.research.countries[tag];
        assert!(!c.researched.is_empty(), "{tag} finished nothing");
        assert!(
            c.slots.iter().all(|slot| slot.active.is_some())
                || c.researched.len() == cat.technologies.len(),
            "{tag} left a slot idle while techs remained"
        );
    }
    assert_eq!(s.ai.stats.commands_rejected, 0, "{:?}", s.ai.log);
}

#[test]
fn human_country_is_left_alone() {
    let cat = catalogue();
    let mut s = session();
    s.player_country = Some("AAA".into());
    run_days(&mut s, &cat, 10);
    assert!(
        s.research.countries["AAA"]
            .slots
            .iter()
            .all(|x| x.active.is_none())
    );
    assert!(s.ai.log.iter().all(|d| d.tag != "AAA"));
    assert!(
        s.research.countries["BBB"]
            .slots
            .iter()
            .any(|x| x.active.is_some())
    );
}

#[test]
fn identical_runs_are_identical_and_ai_state_survives_save_load() {
    let cat = catalogue();
    let mut straight = session();
    run_days(&mut straight, &cat, 60);

    let mut split = session();
    run_days(&mut split, &cat, 30);
    let json = serde_json::to_string(&split).unwrap();
    let mut split: GameSession = serde_json::from_str(&json).unwrap();
    assert!(!split.ai.profiles.is_empty(), "profiles must be saved");
    assert!(
        !split.ai.log.is_empty(),
        "the AI must have acted before the save"
    );
    run_days(&mut split, &cat, 30);

    assert!(
        straight.ai.stats.commands_accepted > 4,
        "{:?}",
        straight.ai.stats
    );
    assert_eq!(straight.ai.log, split.ai.log);
    assert_eq!(straight.ai.stats, split.ai.stats);
    assert_eq!(straight.ai.profiles, split.ai.profiles);
    for tag in ["AAA", "BBB"] {
        assert_eq!(
            straight.research.countries[tag].researched,
            split.research.countries[tag].researched
        );
    }
}
