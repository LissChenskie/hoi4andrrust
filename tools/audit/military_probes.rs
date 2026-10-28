// Diagnostic probes for ac0ee26. A passing assertion reproduces the documented
// defect; these are intentionally outside normal CI, not acceptance tests.
mod audit_probes {
    use super::*;
    use crate::military_model::{tests::template, CountryArmy, MilitaryModifiers, StatBlock};
    fn equipped() -> (World, Warfare) {
        let world = line(&["land"; 5]);
        let mut c = catalogue();
        let inf = c.units.get_mut("infantry").unwrap();
        inf.need.insert("rifle".into(), 100.);
        inf.stats.insert("soft_attack".into(), 0.);
        c.equipment.insert(
            "rifle_1".into(),
            EquipmentDefinition {
                id: "rifle_1".into(),
                archetype: "rifle".into(),
                year: 1936,
                active: true,
                buildable: true,
                modular: false,
                cost: 1.,
                source: "authored audit input".into(),
                stats: StatBlock::from([
                    ("soft_attack".into(), 50.),
                    ("maximum_speed".into(), 4.),
                    ("reliability".into(), 0.8),
                ]),
            },
        );
        let mut w = Warfare::new(c);
        w.countries.insert(
            "A".into(),
            CountryArmy {
                templates: BTreeMap::from([("test".into(), template())]),
                ..Default::default()
            },
        );
        let mut d = division(1, "A", 5);
        d.equipment.insert("rifle_1".into(), 100);
        d.derived = w
            .catalogue
            .aggregate(
                &template(),
                &MilitaryModifiers::default(),
                Some(&d.equipment),
            )
            .unwrap();
        w.divisions.insert(1, d);
        (world, w)
    }
    #[test]
    fn observe_partial_equipment_keeps_full_attack_and_strength() {
        let (_, mut w) = equipped();
        let full = w.divisions[&1].derived.stats.soft_attack;
        w.divisions
            .get_mut(&1)
            .unwrap()
            .equipment
            .insert("rifle_1".into(), 1);
        let reduced = w
            .catalogue
            .aggregate(
                &template(),
                &MilitaryModifiers::default(),
                Some(&w.divisions[&1].equipment),
            )
            .unwrap();
        w.divisions.get_mut(&1).unwrap().derived = reduced;
        let d = &w.divisions[&1];
        eprintln!(
            "GEAR: quantity 100 -> 1; SA {full} -> {}; HP strength {}",
            d.derived.stats.soft_attack,
            d.strength()
        );
        assert_eq!(d.derived.stats.soft_attack, full);
        assert_eq!(d.strength(), 1.);
    }
    #[test]
    fn observe_encircled_unit_receives_national_men_and_rifles() {
        let (_, mut w) = equipped();
        w.logistics.enabled = true;
        let d = w.divisions.get_mut(&1).unwrap();
        d.equipment.insert("rifle_1".into(), 1);
        d.manpower = 500;
        d.hp = 12.5;
        d.supply_fraction = 0.5;
        w.countries.get_mut("A").unwrap().manpower_pool = 500;
        let mut p = crate::production::ProductionState::default();
        p.country_mut("A")
            .model_stockpile
            .insert("rifle_1".into(), 99);
        assert!(w.logistics.serviced.is_empty());
        w.replenish(&mut p);
        eprintln!(
            "POCKET: paths=0; men 500 -> {}; rifles 1 -> {}; national stock {}",
            w.divisions[&1].manpower,
            w.divisions[&1].equipment["rifle_1"],
            p.country("A").unwrap().model_stockpile["rifle_1"]
        );
        assert_eq!(w.divisions[&1].manpower, 1000);
        assert_eq!(w.divisions[&1].equipment["rifle_1"], 100);
    }
    #[test]
    fn observe_reconnected_supply_never_refills_carried_reserve() {
        let (mut world, mut w) = setup();
        w.divisions.get_mut(&1).unwrap().supply_fraction = 0.;
        for h in 0..49 {
            tick(&mut w, &world, h);
        }
        eprintln!(
            "RESERVE: full delivery for 49 h; stock={}, fraction={}",
            w.logistics.divisions[&1].supply, w.divisions[&1].supply_fraction
        );
        assert_eq!(w.logistics.divisions[&1].supply, 0.);
        assert_eq!(w.divisions[&1].supply_fraction, 1.);
        world.set_province_controller(3, Some("B"));
        tick(&mut w, &world, 49);
        assert_eq!(w.divisions[&1].supply_fraction, 0.);
    }
    #[test]
    fn observe_planes_reinforce_an_isolated_airbase() {
        let (world, mut w) = air_setup();
        w.logistics.enabled = true;
        assert!(w.logistics.serviced.is_empty());
        let mut a = wing(1, "A", "cas", 1, AirMission::Standby);
        a.planes = 0;
        w.air.wings.insert(1, a);
        let mut p = crate::production::ProductionState::default();
        p.country_mut("A").model_stockpile.insert("cas".into(), 100);
        w.air.replenish(&mut p, &world);
        eprintln!(
            "AIR REFILL: paths=0; planes 0 -> {}; stock {}",
            w.air.wings[&1].planes,
            p.country("A").unwrap().model_stockpile["cas"]
        );
        assert_eq!(w.air.wings[&1].planes, 100);
    }
    fn battle_pair() -> (World, Warfare, u64, u64) {
        let mut world = line(&["land"; 5]);
        world.countries.insert(
            "B".into(),
            crate::world::Country {
                tag: "B".into(),
                state_ids: BTreeSet::new(),
            },
        );
        world.set_province_controller(3, Some("B"));
        world.set_province_controller(5, Some("B"));
        let mut c = catalogue();
        c.rules.pixels_to_km = Some(1.);
        let mut w = Warfare::new(c);
        w.ai_enabled = false;
        w.declare_war("A", "B", &world).unwrap();
        for (id, tag, p) in [(1, "A", 2), (2, "B", 3), (3, "A", 4), (4, "B", 5)] {
            let mut d = division(id, tag, p);
            d.derived.stats.soft_attack = 0.;
            d.derived.stats.hard_attack = 0.;
            w.divisions.insert(id, d);
        }
        let b1 = w.attack("A", &[1], 3, &world).unwrap();
        let b2 = w.attack("A", &[3], 5, &world).unwrap();
        w.air.enabled = true;
        w.air.regions.insert(1, (1..=5).collect());
        w.air.reindex();
        w.air.results.insert(
            1,
            BTreeMap::from([(
                "A".into(),
                crate::air_warfare::RegionResult {
                    hour: 8,
                    cas: 100,
                    ground_attack: 800.,
                    ..Default::default()
                },
            )]),
        );
        (world, w, b1, b2)
    }
    #[test]
    fn observe_cas_hour_budget_increases_when_first_battle_ends() {
        let (mut world, mut w, b1, b2) = battle_pair();
        w.battles.get_mut(&b1).unwrap().elapsed_hours = 3;
        // Keep the first defender alive when the hour starts, and avoid the
        // width-based CAS cap so actual damage reveals a duplicated budget.
        for d in w.divisions.values_mut() {
            d.derived.stats.width = 40.;
        }
        w.divisions.get_mut(&2).unwrap().organization = 0.001;
        let before = w.divisions[&2].hp + w.divisions[&4].hp;
        w.tick_hour(&mut world, crate::game::GameDate::START, 12, 9, Some("A"));
        let x = w.battles[&b1]
            .history
            .back()
            .unwrap()
            .attacker_air
            .plane_attack_rolls;
        let y = w.battles[&b2]
            .history
            .back()
            .unwrap()
            .attacker_air
            .plane_attack_rolls;
        let hp = before - w.divisions[&2].hp - w.divisions[&4].hp;
        eprintln!("CAS BUDGET: available 100 rolls/h; applied inputs {x} + {y} = {}; HP loss {hp}, conserved-budget maximum 3.5",x+y);
        assert!(w.battles[&b1].outcome.is_some());
        assert!((x + y - 150.).abs() < 1e-9);
        assert!((hp - 5.25).abs() < 1e-9);
    }
    #[test]
    fn observe_one_cas_plane_disappears_between_two_battles() {
        let (mut world, mut w, b1, b2) = battle_pair();
        let r = w.air.results.get_mut(&1).unwrap().get_mut("A").unwrap();
        r.cas = 1;
        r.ground_attack = 8.;
        let hp = w.divisions[&2].hp + w.divisions[&4].hp;
        w.tick_hour(&mut world, crate::game::GameDate::START, 12, 9, Some("A"));
        let x = &w.battles[&b1].history.back().unwrap().attacker_air;
        let y = &w.battles[&b2].history.back().unwrap().attacker_air;
        eprintln!(
            "CAS ROUNDING: planes {}+{}, attack inputs {}+{}, total HP loss {}",
            x.cas_planes,
            y.cas_planes,
            x.plane_attack_rolls,
            y.plane_attack_rolls,
            hp - w.divisions[&2].hp - w.divisions[&4].hp
        );
        assert_eq!(x.cas_planes + y.cas_planes, 0);
        assert_eq!(hp, w.divisions[&2].hp + w.divisions[&4].hp);
    }
    #[test]
    fn observe_fuel_refill_does_not_share_rail_throughput() {
        let (world, mut w) = setup();
        w.logistics.rules.rail_capacity_per_level = 0.0001;
        w.divisions.get_mut(&1).unwrap().fuel_fraction = 0.;
        tick(&mut w, &world, 0);
        let s = &w.logistics.divisions[&1];
        eprintln!(
            "FUEL PATH ONLY: supply delivery {} / {}; fuel delivered {}, carried {}, fraction {}",
            s.received,
            s.demand,
            w.logistics.countries["A"].fuel_used_last_hour,
            s.fuel,
            w.divisions[&1].fuel_fraction
        );
        assert!(s.received < 0.001);
        assert!(w.logistics.countries["A"].fuel_used_last_hour > 24.);
        assert_eq!(w.divisions[&1].fuel_fraction, 1.);
    }
    #[test]
    fn observe_training_advances_without_army_xp_income() {
        let (world, mut w) = equipped();
        w.ai_enabled = false;
        w.divisions.get_mut(&1).unwrap().order =
            crate::military_model::DivisionOrder::Train { hours: 0. };
        w.divisions.get_mut(&1).unwrap().derived.stats.training = 10.;
        let mut world = world;
        for h in 1..=48 {
            w.tick_hour(&mut world, crate::game::GameDate::START, 12, h, Some("A"));
        }
        eprintln!(
            "XP: after 48 h of exercise, Army XP={}",
            w.countries["A"].army_xp
        );
        assert_eq!(w.countries["A"].army_xp, 0.);
    }
    #[test]
    fn observe_new_recruitment_creates_carried_fuel_without_a_country_source() {
        let (world, mut w) = equipped();
        w.divisions.clear();
        w.ai_enabled = false;
        let c = std::sync::Arc::make_mut(&mut w.catalogue);
        c.units
            .get_mut("infantry")
            .unwrap()
            .stats
            .insert("fuel_consumption".into(), 24.);
        c.units
            .get_mut("infantry")
            .unwrap()
            .stats
            .insert("training_time".into(), 1.);
        w.countries.get_mut("A").unwrap().manpower_pool = 1000;
        w.logistics.enabled = true;
        w.logistics.countries.insert(
            "A".into(),
            CountryLogistics {
                capital: Some(1),
                ..Default::default()
            },
        );
        let mut p = crate::production::ProductionState::default();
        p.country_mut("A")
            .model_stockpile
            .insert("rifle_1".into(), 100);
        w.queue_recruitment("A", "test", 2, &world, &mut p).unwrap();
        let mut world = world;
        for h in 1..=25 {
            w.tick_hour(&mut world, crate::game::GameDate::START, 12, h, Some("A"));
        }
        let id = *w.divisions.keys().next().unwrap();
        eprintln!(
            "NEW UNIT FUEL: national={}, drawn={}, carried={}, readiness={}",
            w.logistics.countries["A"].fuel,
            w.logistics.countries["A"].fuel_used_last_hour,
            w.logistics.divisions[&id].fuel,
            w.divisions[&id].fuel_fraction
        );
        assert_eq!(w.logistics.countries["A"].fuel, 0.);
        assert_eq!(w.logistics.countries["A"].fuel_used_last_hour, 0.);
        assert!(w.logistics.divisions[&id].fuel > 23.);
        assert_eq!(w.divisions[&id].fuel_fraction, 1.);
    }
    #[test]
    fn observe_legacy_silo_capacity_is_counted_twice_during_migration() {
        let (mut world, mut w) = setup();
        w.divisions.clear();
        world.states.get_mut(&1).unwrap().economy.fuel_silos = 1;
        w.logistics.rules.base_fuel_capacity = 1000.;
        w.logistics.rules.silo_fuel_capacity = 1000.;
        // Legacy save: 1000 fixed + 1000 from its existing silo, no base field.
        let c = w.logistics.countries.get_mut("A").unwrap();
        c.fuel_capacity = 2000.;
        c.fuel_capacity_base = None;
        tick(&mut w, &world, 1);
        eprintln!(
            "LEGACY SILO: previous total 2000 -> {}, inferred base {:?}",
            w.logistics.countries["A"].fuel_capacity, w.logistics.countries["A"].fuel_capacity_base
        );
        assert_eq!(w.logistics.countries["A"].fuel_capacity, 3000.);
        world.set_province_controller(1, Some("B"));
        tick(&mut w, &world, 2);
        assert_eq!(w.logistics.countries["A"].fuel_capacity, 2000.);
    }
    #[test]
    fn observe_equipment_reliability_modifier_is_lost_in_aggregation() {
        let (_, w) = equipped();
        let mut m = MilitaryModifiers::default();
        m.equipment.insert(
            "rifle".into(),
            StatBlock::from([("reliability".into(), 0.25)]),
        );
        let result = w
            .catalogue
            .aggregate(&template(), &m, Some(&w.divisions[&1].equipment))
            .unwrap();
        eprintln!(
            "RELIABILITY: model=0.8, modifier=+25%, expected adjusted=1.0, final={}",
            result.stats.reliability
        );
        assert!((result.stats.reliability - 0.8).abs() < 1e-9);
    }
}
