//! Behavioural integration tests for the mobile support model. These are not
//! golden measurements of the closed HOI4 engine.
use crate::{
    air_warfare::{AirMission, AirWing},
    logistics::{CountryLogistics, RailSegment},
    military_model::{
        EquipmentDefinition,
        tests::{catalogue, division},
    },
    warfare::Warfare,
    world::World,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};
fn line(kinds: &[&str]) -> World {
    let mut pixels = Vec::new();
    let mut definitions = String::new();
    let mut membership = HashMap::new();
    for (i, kind) in kinds.iter().enumerate() {
        let id = i as u32 + 1;
        pixels.extend([id as u8, 0, 0, 255]);
        definitions += &format!("{id};{id};0;0;{kind};true;plains;1\n");
        if *kind == "land" {
            membership.insert(id, id);
        }
    }
    let owners = membership.values().map(|&s| (s, "A".to_owned())).collect();
    let mut world = World::from_map_data(
        &membership,
        &owners,
        &Default::default(),
        &Default::default(),
        &Default::default(),
    );
    world.geography =
        crate::map_graph::MapGraph::from_rgba(kinds.len() as u32, 1, &pixels, &definitions, false)
            .unwrap();
    world
}
fn setup() -> (World, Warfare) {
    let world = line(&["land"; 5]);
    let mut w = Warfare::new(catalogue());
    w.logistics.enabled = true;
    w.logistics.rules.last_mile_hops = 0;
    w.logistics.rules.hub_capacity = 100.;
    w.logistics.rules.capital_capacity = 100.;
    w.logistics.rules.local_supply_per_infrastructure = 0.;
    w.logistics.rules.idle_fuel_factor = 1.;
    w.logistics.countries.insert(
        "A".into(),
        CountryLogistics {
            capital: Some(1),
            fuel: 1000.,
            fuel_capacity: 1000.,
            ..Default::default()
        },
    );
    for p in 1..5 {
        w.logistics.rails.push(RailSegment {
            from: p,
            to: p + 1,
            level: 5,
        });
    }
    w.conditions.entry(5).or_default().supply_node = true;
    let mut d = division(1, "A", 5);
    d.derived.stats.supply = 24.;
    d.derived.stats.fuel = 24.;
    w.divisions.insert(1, d);
    (world, w)
}
fn tick(w: &mut Warfare, world: &World, h: u64) {
    let mut s = std::mem::take(&mut w.logistics);
    s.tick(w, world, h);
    w.logistics = s;
}
fn air_tick(w: &mut Warfare, world: &World, h: u64) {
    let mut s = std::mem::take(&mut w.air);
    s.tick(w, world, h);
    w.air = s;
}
fn plane(w: &mut Warfare, id: &str, attack: f64, ground: f64) {
    w.catalogue = std::sync::Arc::new((*w.catalogue).clone());
    std::sync::Arc::get_mut(&mut w.catalogue)
        .unwrap()
        .equipment
        .insert(
            id.into(),
            EquipmentDefinition {
                id: id.into(),
                archetype: id.into(),
                year: 1936,
                active: true,
                buildable: true,
                modular: false,
                cost: 10.,
                source: "authored test".into(),
                stats: BTreeMap::from([
                    ("air_attack".into(), attack),
                    ("air_ground_attack".into(), ground),
                    ("air_defence".into(), 10.),
                    ("air_agility".into(), 10.),
                    ("reliability".into(), 1.),
                    ("fuel_consumption".into(), 3.),
                ]),
            },
        );
}
fn wing(id: u64, tag: &str, model: &str, base: u32, mission: AirMission) -> AirWing {
    AirWing {
        id,
        name: format!("Test {id}"),
        country: tag.into(),
        model: model.into(),
        base,
        planes: 100,
        target_planes: 100,
        mission,
        region: Some(1),
        sorties: 0,
        losses: 0,
    }
}
fn air_setup() -> (World, Warfare) {
    let mut world = line(&["land"; 3]);
    world.countries.insert(
        "B".into(),
        crate::world::Country {
            tag: "B".into(),
            state_ids: BTreeSet::new(),
        },
    );
    world.set_province_controller(3, Some("B"));
    let mut w = Warfare::new(catalogue());
    w.declare_war("A", "B", &world).unwrap();
    plane(&mut w, "fighter", 100., 0.);
    plane(&mut w, "cas", 5., 10.);
    w.air.enabled = true;
    w.air.regions.insert(1, BTreeSet::from([1, 2, 3]));
    w.air.bases = BTreeMap::from([(1, 1), (3, 1)]);
    w.air.reindex();
    for tag in ["A", "B"] {
        w.logistics.countries.insert(
            tag.into(),
            CountryLogistics {
                fuel: 10000.,
                fuel_capacity: 10000.,
                ..Default::default()
            },
        );
    }
    let mut d = division(10, "B", 3);
    d.battle = Some(1);
    w.divisions.insert(10, d);
    (world, w)
}
#[test]
fn severed_railway_drains_carried_supply_and_restoration_recovers_it() {
    let (mut world, mut w) = setup();
    // Ample transport for both supply and fuel, including reserve refill.
    w.logistics.rules.rail_capacity_per_level = 30.;
    tick(&mut w, &world, 0);
    assert_eq!(w.divisions[&1].supply_fraction, 1.);
    world.set_province_controller(3, Some("B"));
    tick(&mut w, &world, 1);
    assert!(w.divisions[&1].supply_fraction > 0.9);
    assert_eq!(w.logistics.divisions[&1].received, 0.);
    for h in 2..=49 {
        tick(&mut w, &world, h);
    }
    assert_eq!(w.divisions[&1].supply_fraction, 0.);
    assert!(w.logistics.divisions[&1].reason.contains("пути"));
    world.set_province_controller(3, Some("A"));
    tick(&mut w, &world, 50);
    assert!(w.divisions[&1].supply_fraction > 0.9);
}
#[test]
fn one_rail_bottleneck_is_shared_and_does_not_multiply_per_division() {
    let (world, mut w) = setup();
    w.logistics.rails[1].level = 1;
    let mut second = w.divisions[&1].clone();
    second.id = 2;
    w.divisions.insert(2, second);
    tick(&mut w, &world, 0);
    let total = w
        .logistics
        .divisions
        .values()
        .map(|s| s.received)
        .sum::<f64>();
    // Supply and fuel now spend the same finite rail capacity.
    let fuel_cargo: f64 = w
        .logistics
        .divisions
        .values()
        .map(|s| s.fuel_received * 24. * w.logistics.rules.fuel_transport_weight)
        .sum();
    assert!(total > 0. && total + fuel_cargo <= 5. + 1e-8);
    assert!(w.logistics.divisions.values().all(|s| s.received > 0.));
}
#[test]
fn naval_delivery_requires_two_ports_convoys_and_unblocked_sea() {
    let world = line(&["land", "sea", "land"]);
    let mut w = Warfare::new(catalogue());
    w.logistics.enabled = true;
    w.logistics.rules.last_mile_hops = 0;
    w.logistics.countries.insert(
        "A".into(),
        CountryLogistics {
            capital: Some(1),
            convoys: 100,
            ..Default::default()
        },
    );
    for p in [1, 3] {
        w.conditions.entry(p).or_default().naval_base = 1;
    }
    let mut d = division(1, "A", 3);
    d.derived.stats.supply = 10.;
    w.divisions.insert(1, d);
    tick(&mut w, &world, 0);
    assert!((w.logistics.divisions[&1].received - 3.).abs() < 1e-8);
    w.logistics.countries.get_mut("A").unwrap().convoys = 0;
    w.logistics.network_key = None;
    tick(&mut w, &world, 1);
    assert_eq!(w.logistics.divisions[&1].received, 0.);
    w.logistics.countries.get_mut("A").unwrap().convoys = 100;
    w.logistics.blocked_sea.insert(2);
    w.logistics.network_key = None;
    tick(&mut w, &world, 2);
    assert_eq!(w.logistics.divisions[&1].received, 0.);
}
#[test]
fn fuel_is_conserved_and_an_isolated_motorized_division_runs_out() {
    let (mut world, mut w) = setup();
    w.divisions.get_mut(&1).unwrap().fuel_fraction = 0.;
    tick(&mut w, &world, 0);
    let country = &w.logistics.countries["A"];
    let unit = &w.logistics.divisions[&1];
    let consumed = w.divisions[&1].fuel_fraction; // demand is one fuel unit/hour
    assert!((1000. - country.fuel - unit.fuel - consumed).abs() < 1e-8);
    world.set_province_controller(3, Some("B"));
    for h in 1..=26 {
        tick(&mut w, &world, h);
    }
    assert_eq!(w.divisions[&1].fuel_fraction, 0.);
    assert_eq!(w.logistics.divisions[&1].fuel, 0.);
}
#[test]
fn duplicate_hour_does_not_consume_fuel_or_supply_twice() {
    let (world, mut w) = setup();
    tick(&mut w, &world, 10);
    let before = serde_json::to_string(&w.logistics).unwrap();
    tick(&mut w, &world, 10);
    assert_eq!(before, serde_json::to_string(&w.logistics).unwrap());
}
#[test]
fn oil_and_refinery_income_follow_effective_state_control() {
    let (mut world, mut w) = setup();
    w.divisions.clear();
    w.logistics.countries.get_mut("A").unwrap().fuel = 0.;
    w.logistics.oil_by_state.insert(2, 1.);
    world
        .states
        .get_mut(&2)
        .unwrap()
        .economy
        .synthetic_refineries = 1;
    tick(&mut w, &world, 0);
    assert_eq!(w.logistics.countries["A"].fuel, 4.);
    world.set_province_controller(2, Some("B"));
    tick(&mut w, &world, 1);
    assert_eq!(w.logistics.countries["A"].fuel, 4.);
}
#[test]
fn attrition_loses_real_equipment_but_not_manpower_and_respects_reliability() {
    let (world, mut w) = setup();
    w.logistics.rules.exercise_attrition = 24.;
    w.logistics.rules.attrition_loss_factor = 1.;
    let d = w.divisions.get_mut(&1).unwrap();
    d.order = crate::military_model::DivisionOrder::Train { hours: 0. };
    d.derived.stats.reliability = 0.;
    d.equipment = BTreeMap::from([("unknown-test-model".into(), 1000)]);
    let men = d.manpower;
    tick(&mut w, &world, 0);
    assert_eq!(w.divisions[&1].equipment["unknown-test-model"], 0);
    assert_eq!(w.divisions[&1].manpower, men);
    assert_eq!(w.logistics.divisions[&1].equipment_lost, 1000);
    let d = w.divisions.get_mut(&1).unwrap();
    d.equipment.insert("unknown-test-model".into(), 1000);
    d.derived.stats.reliability = 1.;
    tick(&mut w, &world, 1);
    assert_eq!(w.divisions[&1].equipment["unknown-test-model"], 1000);
}
#[test]
fn save_resume_rebuilds_routes_without_resetting_carried_stocks_or_rng() {
    let (world, mut w) = setup();
    tick(&mut w, &world, 0);
    let saved = serde_json::to_string(&w).unwrap();
    let mut restored: Warfare = serde_json::from_str(&saved).unwrap();
    restored.catalogue = w.catalogue.clone();
    tick(&mut w, &world, 1);
    tick(&mut restored, &world, 1);
    assert_eq!(
        serde_json::to_value(&w.logistics).unwrap(),
        serde_json::to_value(&restored.logistics).unwrap()
    );
    assert_eq!(w.rng.state, restored.rng.state);
}
#[test]
fn air_sorties_consume_country_fuel_and_feed_cas_into_land_inputs() {
    let (world, mut w) = air_setup();
    w.air
        .wings
        .insert(1, wing(1, "A", "cas", 1, AirMission::CloseAirSupport));
    air_tick(&mut w, &world, 8);
    assert_eq!(w.air.results[&1]["A"].sorties, 100);
    assert_eq!(w.logistics.countries["A"].fuel, 9900.);
    let support = w.air.support("A", "B", 3, 8);
    assert_eq!(support.cas_planes, 100);
    assert!(support.plane_attack_rolls > 0.);
    assert_eq!(w.air.support("A", "B", 3, 16).cas_planes, 0);
}
#[test]
fn no_fuel_no_base_capacity_and_out_of_range_all_prevent_sorties() {
    let (mut world, mut w) = air_setup();
    w.air
        .wings
        .insert(1, wing(1, "A", "cas", 1, AirMission::CloseAirSupport));
    w.logistics.countries.get_mut("A").unwrap().fuel = 0.;
    air_tick(&mut w, &world, 8);
    assert!(w.air.results.is_empty());
    w.logistics.countries.get_mut("A").unwrap().fuel = 10000.;
    w.air.bases.clear();
    air_tick(&mut w, &world, 16);
    assert!(w.air.results.is_empty());
    w.air.bases.insert(1, 1);
    w.air.regions.get_mut(&1).unwrap().remove(&1);
    air_tick(&mut w, &world, 24);
    assert!(w.air.results.is_empty());
    world.set_province_controller(1, Some("B"));
    air_tick(&mut w, &world, 32);
    assert!(w.air.results.is_empty());
}
#[test]
fn interception_causes_real_aircraft_losses_and_reduces_surviving_cas() {
    let (world, mut w) = air_setup();
    w.air.rules.air_damage_scale = 10.;
    w.air
        .wings
        .insert(1, wing(1, "A", "cas", 1, AirMission::CloseAirSupport));
    w.air
        .wings
        .insert(2, wing(2, "B", "fighter", 3, AirMission::Interception));
    air_tick(&mut w, &world, 8);
    assert_eq!(w.air.wings[&1].planes, 0);
    assert_eq!(w.air.wings[&1].losses, 100);
    assert_eq!(w.air.support("A", "B", 3, 8).cas_planes, 0);
}
#[test]
fn reinforcement_transfers_aircraft_from_stockpile_once() {
    let (world, mut w) = air_setup();
    let mut production = crate::production::ProductionState::default();
    production
        .country_mut("A")
        .model_stockpile
        .insert("cas".into(), 12);
    let id = w
        .air
        .create_wing("A", "cas", 1, &world, &w.catalogue)
        .unwrap();
    w.air.replenish(&mut production, &world);
    w.air.replenish(&mut production, &world);
    assert_eq!(w.air.wings[&id].planes, 12);
    assert_eq!(production.country("A").unwrap().model_stockpile["cas"], 0);
}
#[test]
fn divisional_aa_causes_actual_aircraft_losses_before_cas_damage() {
    // The land core's existing AA/CAS tests cover dice/damage; this covers upstream
    // missions and the simultaneous aircraft loss that changes those inputs.
    let (world, mut w) = air_setup();
    w.air.rules.aa_loss_factor = 1.;
    w.divisions.get_mut(&10).unwrap().derived.stats.air_attack = 100.;
    w.air
        .wings
        .insert(1, wing(1, "A", "cas", 1, AirMission::CloseAirSupport));
    air_tick(&mut w, &world, 8);
    assert_eq!(w.air.wings[&1].planes, 0);
    assert_eq!(w.air.results[&1]["A"].cas, 0);
}
#[test]
fn importer_preserves_real_historical_aircraft_counts_and_rejects_bad_rails() {
    let (world, mut w) = air_setup();
    let es =
        crate::pdx::parse_pdx("air_wings={ 1={ Test={ equipment={cas={amount=37 owner=A}} } } }")
            .unwrap();
    crate::support_import::import_air_oob(&es, "A", &world, &mut w);
    assert_eq!(w.air.wings[&1].planes, 37);
    assert_eq!(w.air.wings[&1].mission, AirMission::Standby);
    assert_eq!(
        crate::support_import::parse_railways("2 3 1 2 3 # comment")
            .unwrap()
            .len(),
        2
    );
    assert!(crate::support_import::parse_railways("2 4 1 2 3").is_err());
}

#[test]
fn one_cas_sortie_is_shared_between_two_simultaneous_land_battles() {
    let (world, mut w) = air_setup();
    let a = division(11, "A", 2);
    let b = division(12, "B", 3);
    w.divisions.insert(11, a);
    w.divisions.insert(12, b);
    let env = crate::land_combat::BattleEnvironment::default();
    let battle1 = crate::land_combat::LandBattle::new(
        1,
        3,
        vec![11],
        vec![10],
        &w.divisions,
        &env,
        &w.catalogue.rules,
    );
    let battle2 = crate::land_combat::LandBattle::new(
        2,
        2,
        vec![11],
        vec![12],
        &w.divisions,
        &env,
        &w.catalogue.rules,
    );
    w.battles.insert(1, battle1);
    w.battles.insert(2, battle2);
    w.air
        .wings
        .insert(1, wing(1, "A", "cas", 1, AirMission::CloseAirSupport));
    air_tick(&mut w, &world, 8);
    w.simulated_hours = 8;
    let one = w.environment(&w.battles[&1], &world, 12);
    let two = w.environment(&w.battles[&2], &world, 12);
    assert_eq!(
        one.attacker_air.cas_planes + two.attacker_air.cas_planes,
        100
    );
    let total = w.air.support("A", "B", 3, 8).plane_attack_rolls;
    assert!(
        (one.attacker_air.plane_attack_rolls + two.attacker_air.plane_attack_rolls - total).abs()
            < 1e-8
    );
}
#[test]
fn motorization_reserves_actual_trucks_and_returns_them_when_disabled() {
    let (world, mut w) = setup();
    plane(&mut w, "motorized_equipment", 0., 0.);
    let mut production = crate::production::ProductionState::default();
    production
        .country_mut("A")
        .model_stockpile
        .insert("motorized_equipment".into(), 100);
    w.logistics.motorized_hubs.insert(5);
    w.logistics
        .reserve_trucks(&mut production, &world, &w.catalogue);
    assert_eq!(w.logistics.countries["A"].truck_count(), 40);
    assert_eq!(
        production.country("A").unwrap().model_stockpile["motorized_equipment"],
        60
    );
    w.logistics.motorized_hubs.clear();
    w.logistics
        .reserve_trucks(&mut production, &world, &w.catalogue);
    assert_eq!(w.logistics.countries["A"].truck_count(), 0);
    assert_eq!(
        production.country("A").unwrap().model_stockpile["motorized_equipment"],
        100
    );
}

#[test]
fn a_hundred_divisions_share_a_single_rail_bottleneck_without_id_starvation() {
    let (world, mut w) = setup();
    w.logistics.rails[1].level = 1;
    for id in 2..=100 {
        let mut d = w.divisions[&1].clone();
        d.id = id;
        w.divisions.insert(id, d);
    }
    let start = std::time::Instant::now();
    tick(&mut w, &world, 0);
    let delivered: Vec<_> = w.logistics.divisions.values().map(|s| s.received).collect();
    let total = delivered.iter().sum::<f64>();
    // Supply and fuel now spend the same finite rail capacity.
    let fuel_cargo: f64 = w
        .logistics
        .divisions
        .values()
        .map(|s| s.fuel_received * 24. * w.logistics.rules.fuel_transport_weight)
        .sum();
    assert!(total > 0. && total + fuel_cargo <= 5. + 1e-8);
    let mean = total / 100.;
    assert!(delivered.iter().all(|&n| n >= mean * 0.95));
    eprintln!("100-division shared bottleneck: {:?}", start.elapsed());
}

#[test]
fn antiaircraft_guns_cannot_be_deployed_or_imported_as_aircraft() {
    let (world, mut w) = air_setup();
    let mut gun = w.catalogue.equipment["fighter"].clone();
    gun.id = "anti_air".into();
    gun.archetype = "anti_air_equipment".into();
    gun.stats = BTreeMap::from([("air_attack".into(), 100.)]);
    std::sync::Arc::make_mut(&mut w.catalogue)
        .equipment
        .insert("anti_air".into(), gun);
    assert!(!crate::air_warfare::is_aircraft(
        &w.catalogue.equipment["anti_air"]
    ));
    assert!(
        w.air
            .create_wing("A", "anti_air", 1, &world, &w.catalogue)
            .is_err()
    );
    let es =
        crate::pdx::parse_pdx("air_wings={1={Bad={equipment={anti_air={amount=100 owner=A}}}}}")
            .unwrap();
    crate::support_import::import_air_oob(&es, "A", &world, &mut w);
    assert!(w.air.wings.is_empty());
}

#[test]
fn save_between_network_pulses_keeps_dispatch_and_does_not_supply_a_moved_unit_outside_coverage() {
    let (world, mut w) = setup();
    tick(&mut w, &world, 0);
    w.divisions.get_mut(&1).unwrap().province = 4;
    let saved = serde_json::to_string(&w).unwrap();
    let mut loaded: Warfare = serde_json::from_str(&saved).unwrap();
    loaded.catalogue = w.catalogue.clone();
    tick(&mut w, &world, 1);
    tick(&mut loaded, &world, 1);
    assert_eq!(w.logistics.divisions[&1].received, 0.);
    assert_eq!(
        serde_json::to_value(&w.logistics).unwrap(),
        serde_json::to_value(&loaded.logistics).unwrap()
    );
}

include!("support_regressions.rs");
