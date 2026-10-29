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
use crate::resource_economy::ResourceEconomy;
use crate::world::World;

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CountryConstruction {
    pub order: ConstructionOrder,
    /// Retained for the state-inspector controls; global scheduling uses order.
    pub state_priority: Vec<u32>,
    pub policy: CountryConstructionPolicy,
    pub law: EconomicLawPreset,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct GameSession {
    #[serde(default)]
    pub campaign: crate::campaign::CampaignState,
    pub world: World,
    pub clock: GameState,
    pub player_country: Option<String>,
    pub construction_queues: ConstructionQueues,
    pub base_shared_slots: BTreeMap<u32, u16>,
    pub construction: BTreeMap<String, CountryConstruction>,
    pub production: ProductionState,
    #[serde(default)]
    pub resources: ResourceEconomy,
    pub research: ResearchWorld,
    pub military: crate::warfare::Warfare,
    #[serde(default)]
    pub peace: crate::peace::PeaceSystem,
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
            base_shared_slots: world
                .states
                .iter()
                .map(|(id, s)| (*id, s.shared_building_slots))
                .collect(),
            world,
            construction,
            clock: GameState::default(),
            player_country: None,
            construction_queues: ConstructionQueues::new(),
            production: ProductionState::default(),
            resources: ResourceEconomy::default(),
            research: ResearchWorld::default(),
            military: crate::warfare::Warfare::default(),
            peace: crate::peace::PeaceSystem::default(),
            campaign: Default::default(),
        }
    }

    /// Apply every country's starting history BEFORE any AI or daily tick.
    /// Missing history is reported and receives zero slots; no national start is invented.
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
                    (Some(0), Default::default())
                }
            };
            let country = self.research.country_mut(tag, slots.unwrap_or(0), rules);
            for tech in known {
                if !country.researched.contains(&tech) && !country.grant(&tech, catalogue) {
                    // Known history remains authoritative even when the UI
                    // technology catalogue cannot represent this node yet.
                    country.researched.insert(tech.clone());
                    diagnostics.push(format!(
                        "{tag}: starting technology {tech} retained; UI definition unavailable"
                    ));
                }
            }
        }
        diagnostics
    }

    pub fn initialize_research_dataset(
        &mut self,
        data: &hoi4_mobile::research_import::ResearchDataset,
        root: &Path,
    ) {
        let tags: std::collections::BTreeSet<_> = data
            .histories
            .keys()
            .chain(self.world.countries.keys())
            .cloned()
            .collect();
        for tag in tags {
            if let Some(c) = data.country(&tag, self.clock.date, root) {
                self.research.countries.insert(tag, c);
            } else {
                let c = self.research.country_mut(&tag, 0, &data.rules);
                c.context.tag = tag.clone();
                c.context.original_tag = tag.clone();
                eprintln!("[research] {tag}: история отсутствует, слоты не выдуманы");
            }
        }
        self.research.global_flags = self
            .research
            .countries
            .values()
            .flat_map(|c| c.context.global_flags.iter().cloned())
            .collect();
        for c in self.research.countries.values_mut() {
            c.context.global_flags = self.research.global_flags.clone();
        }
        self.research.refresh_sharing(&data.registry);
        self.refresh_research_building_slots();
    }
    pub fn refresh_research_building_slots(&mut self) {
        let controllers: BTreeMap<_, _> = self
            .world
            .states
            .keys()
            .filter_map(|&id| {
                self.world
                    .state_controller(id)
                    .map(|tag| (id, tag.to_owned()))
            })
            .collect();
        for (id, state) in &mut self.world.states {
            let bonus = controllers
                .get(id)
                .and_then(|tag| self.research.country(tag))
                .map_or(0.0, |c| c.modifier("global_building_slots_factor"));
            if let Some(base) = self.base_shared_slots.get(id) {
                state.shared_building_slots = (*base as f64 * (1.0 + bonus).max(0.0))
                    .floor()
                    .clamp(0.0, u16::MAX as f64)
                    as u16;
            }
        }
    }
    /// Research component only. A full campaign save is a separate, still missing subsystem.
    pub fn save_research_snapshot(
        &self,
        path: &Path,
        cat: &ResearchCatalogue,
    ) -> Result<(), String> {
        hoi4_mobile::research_import::save_snapshot(
            path,
            &self.research,
            cat,
            self.clock.date,
            self.clock.hour,
        )
    }
    pub fn restore_research_snapshot(
        &mut self,
        path: &Path,
        cat: &ResearchCatalogue,
    ) -> Result<(), String> {
        let (research, date, hour) = hoi4_mobile::research_import::load_snapshot(path, cat)?;
        if date != self.clock.date || hour != self.clock.hour {
            return Err(
                "Снимок компонента можно восстановить только на той же дате и часе кампании".into(),
            );
        }
        self.research = research;
        self.military.refresh_modifiers(&self.research);
        self.refresh_research_building_slots();
        Ok(())
    }

    pub fn refresh_research_contexts(&mut self) {
        for (tag, c) in &mut self.research.countries {
            let war = self.military.wars.iter().any(|(a, b)| a == tag || b == tag);
            let units = self.military.countries.get(tag).map(|army| {
                army.templates
                    .values()
                    .flat_map(|t| {
                        t.line
                            .iter()
                            .map(|s| s.unit.clone())
                            .chain(t.support.iter().map(|s| s.unit.clone()))
                    })
                    .collect()
            });
            if c.context.template_units != units {
                c.context.template_units = units;
                c.revision += 1;
            }
            if c.context.date != Some(self.clock.date) || c.context.at_war != Some(war) {
                c.context.date = Some(self.clock.date);
                c.context.at_war = Some(war);
                c.revision += 1;
            }
        }
    }
    pub fn advance_frame(
        &mut self,
        delta_seconds: f32,
        buildings: &BuildingRules,
        production: &ProductionRules,
        catalogue: &ResearchCatalogue,
        research: &ResearchRules,
    ) -> SimulationAdvance {
        self.refresh_research_contexts();
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
        // Naval OOBs use the same selected historical bookmarks as the land
        // and air forces. Hull types are never manufactured as placeholders.
        self.military.naval =
            crate::naval_warfare::import_naval_oob(root, &self.world, &self.military);
        let mut navy = std::mem::take(&mut self.military.naval);
        crate::naval_warfare::import_navy_leaders(root, &self.military, self.clock.date, &mut navy);
        self.military.naval = navy;
        if let Some(speed) = self
            .military
            .catalogue
            .literal_defines
            .get("NDefines.NProduction.BASE_FACTORY_SPEED_DOCKYARD")
            .copied()
            .filter(|v| v.is_finite() && *v > 0.0)
        {
            self.military.naval.rules.dockyard_output = speed;
        }
        // Starting motorized hubs must claim vehicles from historical stockpiles.
        self.military.logistics.reserve_trucks(
            &mut self.production,
            &self.world,
            &self.military.catalogue,
        );
        let mut logistics = std::mem::take(&mut self.military.logistics);
        if logistics.enabled {
            logistics.refresh_network(&self.military, &self.world);
        }
        self.military.logistics = logistics;
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
        construction::initialize_builders(&self.world, &mut self.construction_queues);
        // Persisted troop voyages reserve physical convoys before supply
        // and trade receive their shared pool allocations.
        self.military
            .naval
            .sync_transport_convoys(&mut self.military.logistics);
        // Reconcile the logistics allocation before recalculating hub ranges:
        // toggles and occupied hubs release trucks, new hubs reserve available stock.
        self.military.logistics.reserve_trucks(
            &mut self.production,
            &self.world,
            &self.military.catalogue,
        );
        // Only genuine unfilled equipment/manpower requests consume rail/port capacity.
        // Taking the logistics state out prevents aliasing Warfare and its transport fields.
        let mut logistics = std::mem::take(&mut self.military.logistics);
        logistics.prepare_reinforcements(&self.military, &self.production);
        self.military.logistics = logistics;
        let control_revision = self.world.control_revision;
        let completed_day = self.clock.advance_one_hour();
        self.tick_campaign(catalogue, research);
        self.military
            .air
            .replenish(&mut self.production, &self.world);
        self.military.tick_hour(
            &mut self.world,
            self.clock.date,
            self.clock.hour,
            self.clock.elapsed_hours,
            self.player_country.as_deref(),
        );
        // Sea operations are hourly, not tied to the midnight production tick.
        // Temporarily extract the naval state to avoid borrowing warfare twice.
        let mut naval = std::mem::take(&mut self.military.naval);
        naval.tick_naval_combat(&self.world, &mut self.military, self.clock.elapsed_hours);
        naval.tick_voyages(&mut self.military, &mut self.world);
        self.military.naval = naval;
        self.peace.tick(
            &mut self.world,
            &mut self.military,
            &self.research,
            self.clock.elapsed_hours,
        );
        if self.world.control_revision != control_revision {
            self.refresh_research_building_slots();
        }
        let Some(completed_day) = completed_day else {
            return false;
        };
        let tags: Vec<_> = self
            .world
            .countries
            .keys()
            .filter(|t| !self.campaign.inactive.contains(*t))
            .cloned()
            .collect();
        self.research.refresh_sharing(catalogue);
        for tag in &tags {
            self.research.country_mut(tag, 0, research);
            self.research
                .tick_country(tag, catalogue, research, completed_day);
            if self.research.day_index % 7 == 0 && self.player_country.as_ref() != Some(tag) {
                self.research
                    .ai_choose(tag, catalogue, research, self.clock.date);
            }
        }
        self.research.day_index += 1;
        self.refresh_research_contexts();
        self.refresh_research_building_slots();
        self.military.refresh_modifiers(&self.research);
        self.military.tick_mobilization_day();
        // Every country's production sees factories present at day start.
        // Newly completed MIL participates only in the next daily production.
        let costs = self
            .military
            .catalogue
            .equipment
            .iter()
            .map(|(id, e)| (id.clone(), e.cost))
            .collect();
        // Each midnight reroutes daily resources after the previous day's
        // territorial changes. Imported resources are a flow, not a stockpile.
        // Ignore yesterday's import payments when determining today's budget.
        let policies: BTreeMap<_, _> = tags
            .iter()
            .map(|tag| {
                let mut policy = self
                    .construction
                    .get(tag)
                    .map_or(CountryConstructionPolicy::default(), |c| c.policy);
                policy.trade_civilian_factories = 0;
                (tag.clone(), policy)
            })
            .collect();
        self.resources
            .settle_campaign(&mut self.world, &policies, &self.military, &self.research);
        // The fleet's abstract campaign-level convoy encounters run after
        // routes are calculated and before the day's resources are consumed.
        // Take the naval state out so route/war/country data stay read-only.
        let mut navy = std::mem::take(&mut self.military.naval);
        let mut losses = BTreeMap::new();
        navy.process_trade(
            &self.world,
            &self.military,
            &mut self.resources,
            &mut losses,
        );
        self.military.naval = navy;
        for (tag, sunk) in losses {
            if let Some(country) = self.military.logistics.countries.get_mut(&tag) {
                country.convoys = country.convoys.saturating_sub(sunk);
                self.military.logistics.network_key = None;
            }
        }
        // The naval supply network uses the convoys not reserved for commerce.
        for tag in &tags {
            let committed = self
                .resources
                .last
                .get(tag)
                .map_or(0, |report| report.trade_convoys_used);
            self.military.logistics.set_trade_convoys(tag, committed);
        }
        // Oil is not stockpiled as a raw material: the same delivered
        // oil flow funds fuel conversion every subsequent simulation hour.
        for tag in &tags {
            let delivered_oil = self
                .resources
                .last
                .get(tag)
                .and_then(|r| r.available.get("oil"))
                .copied()
                .unwrap_or(0);
            self.military
                .logistics
                .countries
                .entry(tag.clone())
                .or_default()
                .oil_supply_daily = if self.world.resources_by_state.is_empty() {
                None
            } else {
                Some(f64::from(delivered_oil))
            };
        }
        for tag in &tags {
            self.construction
                .entry(tag.clone())
                .or_default()
                .policy
                .trade_civilian_factories = self
                .resources
                .last
                .get(tag)
                .map_or(0, |r| r.factories_spent);
            if let Some(economy) = self.world.country_economy(tag) {
                let effective = production.for_research(self.research.country(tag));
                let mut pool = self
                    .resources
                    .last
                    .get(tag)
                    .map_or_else(BTreeMap::new, |r| r.available.clone());
                self.production.country_mut(tag).tick_with_resource_pool(
                    economy.military_factories,
                    &effective,
                    &costs,
                    &self.military.catalogue.equipment_resources,
                    &mut pool,
                );
                // Dockyards spend the same daily resources as other lines;
                // no mineral can be counted twice across industrial sectors.
                let mut navy = std::mem::take(&mut self.military.naval);
                navy.tick_shipbuilding(
                    tag,
                    economy.dockyards,
                    &self.world,
                    &self.military,
                    &mut pool,
                    effective.resource_lack_penalty,
                );
                self.military.naval = navy;
            }
        }
        // Carriers draw aircraft from the same real equipment stockpile as
        // land airfields and never receive free replacement aircraft.
        let mut navy = std::mem::take(&mut self.military.naval);
        navy.replenish_carriers(&self.world, &self.military, &mut self.production);
        self.military.naval = navy;
        self.military.replenish(&mut self.production);
        for tag in &tags {
            let effective = buildings.for_research(self.research.country(tag), catalogue);
            let modifiers = ConstructionModifiers::from_research(self.research.country(tag));
            let country = self.construction.entry(tag.clone()).or_default();
            let removed = construction::advance_day_by_order(
                &mut self.world,
                tag,
                &mut self.construction_queues,
                &mut country.order,
                &effective,
                country.policy,
                &modifiers,
            );
            for (other, country) in &mut self.construction {
                if other != tag {
                    for key in &removed {
                        country
                            .order
                            .on_project_removed(key.state_id, key.project_index);
                    }
                }
            }
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
    fn hourly_motorization_uses_stock_and_returns_it_to_the_same_country() {
        let mut s = session();
        let mut db = (*s.military.catalogue).clone();
        db.equipment.insert(
            "test_truck".into(),
            crate::military_model::EquipmentDefinition {
                id: "test_truck".into(),
                archetype: "motorized_equipment".into(),
                year: 1936,
                active: true,
                buildable: true,
                stats: Default::default(),
                cost: 2.5,
                modular: false,
                source: "test".into(),
            },
        );
        s.military.catalogue = std::sync::Arc::new(db);
        s.military
            .logistics
            .countries
            .insert("AAA".into(), Default::default());
        s.military.logistics.motorized_hubs.insert(11);
        s.production
            .country_mut("AAA")
            .model_stockpile
            .insert("test_truck".into(), 45);
        hours(&mut s, 1);
        assert_eq!(
            s.production.country("AAA").unwrap().model_stockpile["test_truck"],
            5
        );
        assert_eq!(
            s.military.logistics.countries["AAA"].truck_models["test_truck"],
            40
        );
        hours(&mut s, 1);
        assert_eq!(
            s.production.country("AAA").unwrap().model_stockpile["test_truck"],
            5
        );
        s.military.logistics.motorized_hubs.clear();
        hours(&mut s, 1);
        assert_eq!(
            s.production.country("AAA").unwrap().model_stockpile["test_truck"],
            45
        );
        assert_eq!(s.military.logistics.countries["AAA"].truck_count(), 0);
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
                builder: None,
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
    fn finishing_an_occupiers_project_preserves_the_other_countrys_paused_priorities() {
        let mut s = session();
        let project = |builder: &str, building, cost| ConstructionProject {
            builder: Some(builder.into()),
            building,
            cost,
            progress: 0.0,
        };
        s.construction_queues.insert(
            2,
            std::collections::VecDeque::from([
                project("BBB", BuildingType::Infrastructure, 1.0),
                project("AAA", BuildingType::MilitaryFactory, 10000.0),
                project("AAA", BuildingType::Dockyard, 10000.0),
            ]),
        );
        s.construction.get_mut("AAA").unwrap().order.lines = vec![
            construction::ProjectRef {
                state_id: 2,
                project_index: 2,
            },
            construction::ProjectRef {
                state_id: 2,
                project_index: 1,
            },
        ];
        hours(&mut s, 24);
        assert_eq!(s.construction_queues[&2].len(), 2);
        assert_eq!(
            s.construction["AAA"].order.lines,
            vec![
                construction::ProjectRef {
                    state_id: 2,
                    project_index: 1
                },
                construction::ProjectRef {
                    state_id: 2,
                    project_index: 0
                }
            ]
        );
        assert!(s.construction_queues[&2].iter().all(|p| p.progress == 0.0));
        let provinces = s.world.states[&2].province_ids.clone();
        for id in provinces {
            s.world.set_province_controller(id, Some("AAA"));
        }
        hours(&mut s, 24);
        assert!(s.construction_queues[&2].iter().all(|p| p.progress > 0.0));
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

#[cfg(test)]
mod research_integration_tests {
    use super::*;
    use crate::research::{CountryResearch, Technology, TechnologyEffect};
    #[test]
    fn technology_changes_real_factory_output_and_construction_speed_once() {
        let rules = ResearchRules::default();
        let mut c = CountryResearch::new(1, &rules);
        let mut cat = ResearchCatalogue::default();
        let mut t = Technology::sample("industry", 1936, 1.0);
        t.modifiers
            .insert("industrial_capacity_factory".into(), 0.25);
        t.modifiers
            .insert("production_speed_buildings_factor".into(), 0.20);
        t.modifiers
            .insert("production_factory_max_efficiency_factor".into(), 0.1);
        cat.insert(t).unwrap();
        c.grant("industry", &cat);
        c.grant("industry", &cat);
        let base = ProductionRules::default();
        let applied = base.for_research(Some(&c));
        assert!((applied.ic_per_factory() / base.ic_per_factory() - 1.25).abs() < 1e-10);
        assert!((applied.efficiency_cap - 0.6).abs() < 1e-10);
        let modifiers = ConstructionModifiers::from_research(Some(&c));
        assert!((modifiers.national_bonus - 0.2).abs() < 1e-6);
    }
    #[test]
    fn building_unlock_level_is_gated_until_technology_is_granted() {
        let mut cat = ResearchCatalogue::default();
        let mut t = Technology::sample("reactor", 1936, 1.0);
        t.effects.push(TechnologyEffect::Building {
            id: "nuclear_reactor".into(),
            level: 2,
        });
        cat.insert(t).unwrap();
        let rules = ResearchRules::default();
        let mut c = CountryResearch::new(1, &rules);
        let br = BuildingRules::default();
        let b = crate::construction::BuildingType::NuclearReactor;
        assert_eq!(br.for_research(Some(&c), &cat).max_state_level(b), 0);
        c.grant("reactor", &cat);
        assert_eq!(br.for_research(Some(&c), &cat).max_state_level(b), 2);
    }
}

#[cfg(test)]
#[path = "economy_military_tests.rs"]
mod economy_military_tests;
