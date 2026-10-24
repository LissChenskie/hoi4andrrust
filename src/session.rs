//! Campaign state and ordered simulation updates. No graphics or input APIs.
//! Rendering/reloading a map cannot replace this state. UI windows borrow it.
use std::collections::BTreeMap;
use std::path::Path;

use crate::construction::{
    self, BuildingRules, ConstructionModifiers, ConstructionOrder, ConstructionQueues,
    CountryConstructionPolicy, EconomicLawPreset,
};
use crate::game::GameState;
use crate::production::{ProductionRules, ProductionState};
use crate::research::{ResearchCatalogue, ResearchRules, ResearchWorld};
use crate::world::World;

#[derive(Debug, Default)]
pub struct CountryConstruction {
    pub order: ConstructionOrder,
    /// Retained for the state-inspector controls; global scheduling uses order.
    pub state_priority: Vec<u32>,
    pub policy: CountryConstructionPolicy,
    pub law: EconomicLawPreset,
}

#[derive(Debug)]
pub struct GameSession {
    pub world: World,
    pub clock: GameState,
    pub player_country: Option<String>,
    pub construction_queues: ConstructionQueues,
    pub construction: BTreeMap<String, CountryConstruction>,
    pub production: ProductionState,
    pub research: ResearchWorld,
    pub military: crate::warfare::Warfare,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SimulationAdvance {
    pub hours: u32,
    pub days: u32,
}

impl GameSession {
    pub fn new(world: World) -> Self {
        let construction = world
            .countries
            .keys()
            .cloned()
            .map(|tag| (tag, CountryConstruction::default()))
            .collect();
        Self {
            world,
            construction,
            clock: GameState::default(),
            player_country: None,
            construction_queues: ConstructionQueues::new(),
            production: ProductionState::default(),
            research: ResearchWorld::default(),
            military: crate::warfare::Warfare::default(),
        }
    }

    /// Apply every country's starting history BEFORE any AI or daily tick.
    /// Missing/unsupported history is reported and uses an explicit 3-slot fallback.
    pub fn initialize_research(
        &mut self,
        history: &Path,
        catalogue: &ResearchCatalogue,
        rules: &ResearchRules,
    ) -> Vec<String> {
        let mut diagnostics = Vec::new();
        for tag in self.world.countries.keys() {
            if self.research.country(tag).is_some() {
                continue;
            }
            let (slots, known) = match crate::research_data::read_country_start(history, tag) {
                Ok(start) => start,
                Err(error) => {
                    diagnostics.push(error);
                    (Some(3), Default::default())
                }
            };
            let country = self.research.country_mut(tag, slots.unwrap_or(3), rules);
            for tech in known {
                if !country.grant(&tech, catalogue) {
                    diagnostics.push(format!(
                        "{tag}: starting technology {tech} was not imported"
                    ));
                }
            }
        }
        diagnostics
    }

    pub fn advance_frame(
        &mut self,
        delta_seconds: f32,
        buildings: &BuildingRules,
        production: &ProductionRules,
        catalogue: &ResearchCatalogue,
        research: &ResearchRules,
    ) -> SimulationAdvance {
        let hours = self.clock.accumulate_hours(delta_seconds);
        let mut result = SimulationAdvance { hours, days: 0 };
        for _ in 0..hours {
            if self.advance_one_hour(buildings, production, catalogue, research) {
                result.days += 1;
            }
        }
        result
    }

    pub fn initialize_military(&mut self, root: &Path) {
        self.military = crate::military_history::initialize(
            root,
            &self.world,
            &self.research,
            &mut self.production,
            self.clock.date,
        );
    }

    /// One simulation boundary for movement and combat. Simulation
    /// order is stable (country tag order), independent of UI and render FPS.
    /// Daily order: research -> production -> construction, all at midnight.
    pub fn advance_one_hour(
        &mut self,
        buildings: &BuildingRules,
        production: &ProductionRules,
        catalogue: &ResearchCatalogue,
        research: &ResearchRules,
    ) -> bool {
        let completed_day = self.clock.advance_one_hour();
        self.military.tick_hour(
            &mut self.world,
            self.clock.date,
            self.clock.hour,
            self.clock.elapsed_hours,
            self.player_country.as_deref(),
        );
        let Some(completed_day) = completed_day else {
            return false;
        };
        let tags: Vec<_> = self.world.countries.keys().cloned().collect();
        for tag in &tags {
            self.research.country_mut(tag, 3, research);
            self.research
                .tick_country(tag, catalogue, research, completed_day);
            if self.research.day_index % 7 == 0 && self.player_country.as_ref() != Some(tag) {
                self.research
                    .ai_choose(tag, catalogue, research, self.clock.date);
            }
        }
        self.research.day_index += 1;
        self.military.refresh_modifiers(&self.research);
        // Every country's production sees factories present at day start.
        // Newly completed MIL participates only in the next daily production.
        let costs = self
            .military
            .catalogue
            .equipment
            .iter()
            .map(|(id, e)| (id.clone(), e.cost))
            .collect();
        for tag in &tags {
            if let Some(economy) = self.world.country_economy(tag) {
                self.production.country_mut(tag).tick_with_models(
                    economy.military_factories,
                    production,
                    &costs,
                );
            }
        }
        self.military.replenish(&mut self.production);
        for tag in &tags {
            let country = self.construction.entry(tag.clone()).or_default();
            construction::advance_day_by_order(
                &mut self.world,
                tag,
                &mut self.construction_queues,
                &mut country.order,
                buildings,
                country.policy,
                &ConstructionModifiers::default(),
            );
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn campaign_combat_is_hourly_and_replays_independently_of_frame_partition() {
        fn campaign() -> GameSession {
            let mut s = GameSession::new(crate::warfare::tests::world());
            let mut c = crate::military_model::tests::catalogue();
            c.rules.pixels_to_km = Some(1.0);
            s.military = crate::warfare::Warfare::new(c);
            s.military.ai_enabled = false;
            for (id, tag, p) in [(1, "A", 1), (2, "B", 2)] {
                let d = crate::military_model::tests::division(id, tag, p);
                s.military.divisions.insert(id, d);
            }
            s.military.declare_war("A", "B", &s.world).unwrap();
            let id = s.military.attack("A", &[1], 2, &s.world).unwrap();
            s.military.battles.get_mut(&id).unwrap().trace_enabled = true;
            s.clock.paused = false;
            s
        }
        let (mut a, mut b) = (campaign(), campaign());
        let (cat, br, pr, rr) = (
            ResearchCatalogue::default(),
            BuildingRules::default(),
            ProductionRules::default(),
            ResearchRules::default(),
        );
        a.advance_frame(1.0, &br, &pr, &cat, &rr);
        for _ in 0..4 {
            b.advance_frame(0.25, &br, &pr, &cat, &rr);
        }
        assert_eq!(a.military.simulated_hours, 24);
        assert_eq!(a.military.rng.state, b.military.rng.state);
        for id in [1, 2] {
            assert_eq!(a.military.divisions[&id].hp, b.military.divisions[&id].hp);
            assert_eq!(
                a.military.divisions[&id].organization,
                b.military.divisions[&id].organization
            );
        }
        a.clock.paused = true;
        let before = a.military.rng.state;
        a.advance_frame(1.0, &br, &pr, &cat, &rr);
        assert_eq!(a.military.rng.state, before);
    }
    use crate::construction::{BuildingType, ConstructionProject};
    use crate::production::Equipment;
    use crate::research::Technology;
    use crate::state::StateEconomy;
    use std::collections::HashMap;

    fn session() -> GameSession {
        let world = World::from_map_data(
            &HashMap::from([(11, 1), (21, 2)]),
            &HashMap::from([(1, "AAA".into()), (2, "BBB".into())]),
            &HashMap::new(),
            &HashMap::from([
                (
                    1,
                    StateEconomy {
                        civilian_factories: 15,
                        military_factories: 1,
                        ..Default::default()
                    },
                ),
                (
                    2,
                    StateEconomy {
                        civilian_factories: 15,
                        military_factories: 1,
                        ..Default::default()
                    },
                ),
            ]),
            &HashMap::from([(1, 25), (2, 25)]),
        );
        let mut session = GameSession::new(world);
        session.player_country = Some("AAA".into());
        session
    }

    fn hours(s: &mut GameSession, count: u32) {
        let cat = ResearchCatalogue::default();
        for _ in 0..count {
            s.advance_one_hour(
                &BuildingRules::default(),
                &ProductionRules::default(),
                &cat,
                &ResearchRules::default(),
            );
        }
    }

    #[test]
    fn economy_runs_once_per_midnight_for_every_country_and_keeps_factory_delay() {
        let mut s = session();
        for tag in ["AAA", "BBB"] {
            s.production.country_mut(tag).create(
                Equipment::InfantryKit,
                150,
                &ProductionRules::default(),
            );
        }
        s.construction_queues
            .entry(2)
            .or_default()
            .push_back(ConstructionProject {
                building: BuildingType::MilitaryFactory,
                progress: 0.0,
                cost: 1.0,
            });
        hours(&mut s, 23);
        assert_eq!(
            s.production.country("BBB").unwrap().lines[0].last_day_ic,
            0.0
        );
        assert_eq!(
            s.world.country_economy("BBB").unwrap().military_factories,
            1
        );
        hours(&mut s, 1);
        assert_eq!(
            s.production.country("AAA").unwrap().lines[0].last_day_ic,
            0.45
        );
        assert_eq!(
            s.production.country("BBB").unwrap().lines[0].last_day_ic,
            0.45
        );
        assert_eq!(
            s.world.country_economy("BBB").unwrap().military_factories,
            2
        );
        hours(&mut s, 24);
        assert!(s.production.country("BBB").unwrap().lines[0].last_day_ic > 0.9);
        assert_eq!(s.research.day_index, 2);
    }

    #[test]
    fn switching_player_keeps_independent_policies_and_global_orders() {
        let mut s = session();
        for id in [1, 2] {
            for building in [BuildingType::MilitaryFactory, BuildingType::CivilianFactory] {
                construction::enqueue_project(
                    &s.world,
                    if id == 1 { "AAA" } else { "BBB" },
                    id,
                    &mut s.construction_queues,
                    &BuildingRules::default(),
                    building,
                )
                .unwrap();
            }
        }
        let a = s.construction.get_mut("AAA").unwrap();
        a.order.sync(&s.world, "AAA", &s.construction_queues);
        let first = a.order.lines[0];
        a.order.move_down(first);
        a.policy.consumer_goods_fraction = 1.0;
        let expected = a.order.clone();
        s.player_country = Some("BBB".into());
        hours(&mut s, 24);
        s.player_country = Some("AAA".into());
        assert_eq!(s.construction["AAA"].order, expected);
        assert_eq!(s.construction_queues[&1][0].progress, 0.0);
        assert!(s.construction_queues[&2][0].progress > 0.0);
        assert_eq!(s.construction["BBB"].policy.consumer_goods_fraction, 0.0);
    }

    #[test]
    fn country_histories_are_applied_to_foreign_countries_before_ai() {
        let directory =
            std::env::temp_dir().join(format!("hoi4-country-start-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("AAA - A.txt"), "set_research_slots = 2").unwrap();
        std::fs::write(
            directory.join("BBB - B.txt"),
            "set_research_slots = 5 set_technology = { known = 1 }",
        )
        .unwrap();
        let mut cat = ResearchCatalogue::default();
        cat.insert(Technology::sample("known", 1936, 1.0)).unwrap();
        let mut s = session();
        assert!(
            s.initialize_research(&directory, &cat, &ResearchRules::default())
                .is_empty()
        );
        assert_eq!(s.research.country("BBB").unwrap().slots.len(), 5);
        assert!(
            s.research
                .country("BBB")
                .unwrap()
                .researched
                .contains("known")
        );
        assert_eq!(s.research.country("AAA").unwrap().slots.len(), 2);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn frame_slicing_preserves_calendar_and_daily_results() {
        let mut a = session();
        let mut b = session();
        a.clock.paused = false;
        b.clock.paused = false;
        a.clock.set_speed(3);
        b.clock.set_speed(3);
        let buildings = BuildingRules::default();
        let production = ProductionRules::default();
        let cat = ResearchCatalogue::default();
        let rules = ResearchRules::default();
        a.advance_frame(0.5, &buildings, &production, &cat, &rules);
        for _ in 0..5 {
            b.advance_frame(0.1, &buildings, &production, &cat, &rules);
        }
        assert_eq!(a.clock.date, b.clock.date);
        assert_eq!(a.clock.hour, b.clock.hour);
        assert_eq!(a.research.day_index, b.research.day_index);
        assert_eq!(a.world, b.world);
    }

    #[test]
    fn research_at_year_boundary_uses_the_completed_day_not_the_new_date() {
        let mut s = session();
        s.clock.date = crate::game::GameDate {
            year: 1937,
            month: 12,
            day: 31,
        };
        s.clock.hour = 23;
        let mut cat = ResearchCatalogue::default();
        cat.insert(Technology::sample("future", 1938, 1.0)).unwrap();
        let rules = ResearchRules::default();
        s.research
            .country_mut("AAA", 1, &rules)
            .start(0, "future", &cat, &rules, s.clock.date)
            .unwrap();
        s.advance_one_hour(
            &BuildingRules::default(),
            &ProductionRules::default(),
            &cat,
            &rules,
        );
        let progress = s.research.country("AAA").unwrap().slots[0]
            .active
            .as_ref()
            .unwrap()
            .progress;
        assert!(progress > 0.98 && progress < 1.0);
        assert_eq!(s.clock.date.year, 1938);
        assert_eq!(s.research.day_index, 1);
    }
}
