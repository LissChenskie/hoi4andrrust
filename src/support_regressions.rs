// Positive regression tests for defects reproduced by the October 28 audit.
mod regressions {
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
    fn reinforcement_consumes_network_cargo_credit_without_inventing_equipment() {
        let (_, mut w) = equipped();
        let d = w.divisions.get_mut(&1).unwrap();
        d.equipment.insert("rifle_1".into(), 50);
        d.derived = w.catalogue.aggregate(
            &template(), &MilitaryModifiers::default(), Some(&d.equipment)
        ).unwrap();
        let mut production = crate::production::ProductionState::default();
        production.country_mut("A").model_stockpile.insert("rifle_1".into(), 50);
        w.logistics.enabled = true;
        w.logistics.last_hour = Some(1);
        w.logistics.serviced.insert(("A".into(), 5));
        w.logistics.rules.equipment_transport_weight = 0.01;
        w.logistics.reinforcement_credit.insert(1, 0.05);
        w.replenish(&mut production);
        assert_eq!(w.divisions[&1].equipment["rifle_1"], 55);
        assert_eq!(production.country("A").unwrap().model_stockpile["rifle_1"], 45);
        assert!(w.logistics.reinforcement_credit[&1] < 1e-8);
        w.replenish(&mut production);
        assert_eq!(w.divisions[&1].equipment["rifle_1"], 55);
    }
    #[test]
    fn reinforcement_requests_require_stockpile_and_real_deficits() {
        let (_, mut w) = equipped();
        let mut production = crate::production::ProductionState::default();
        w.logistics.enabled = true;
        let mut logistics = std::mem::take(&mut w.logistics);
        logistics.prepare_reinforcements(&w, &production);
        assert!(logistics.reinforcement_requests.is_empty());
        let d = w.divisions.get_mut(&1).unwrap();
        d.equipment.insert("rifle_1".into(), 10);
        d.derived = w.catalogue.aggregate(
            &template(), &MilitaryModifiers::default(), Some(&d.equipment)
        ).unwrap();
        production.country_mut("A").model_stockpile.insert("rifle_1".into(), 100);
        logistics.prepare_reinforcements(&w, &production);
        assert!(logistics.reinforcement_requests[&1] > 0.);
    }
    #[test]
    fn replacing_actual_combat_losses_restores_hp_without_permanent_ghost_damage() {
        let (_, mut w) = equipped();
        let (men, rifles) = w.divisions.get_mut(&1).unwrap().apply_damage(10., 5., &w.catalogue.rules);
        let wounded_hp = w.divisions[&1].hp;
        let mut p = crate::production::ProductionState::default();
        w.replenish(&mut p);
        assert_eq!(w.divisions[&1].hp, wounded_hp); // nothing to deliver
        w.countries.get_mut("A").unwrap().manpower_pool = u64::from(men);
        p.country_mut("A").model_stockpile.insert("rifle_1".into(), u64::from(rifles));
        w.replenish(&mut p);
        assert_eq!(w.divisions[&1].manpower, 1000);
        assert_eq!(w.divisions[&1].equipment["rifle_1"], 100);
        assert!((w.divisions[&1].hp - w.divisions[&1].derived.stats.hp).abs() < 1e-9);
        assert_eq!(w.countries["A"].manpower_pool, 0);
        assert_eq!(p.country("A").unwrap().model_stockpile["rifle_1"], 0);
    }
    #[test]
    fn partial_equipment_reduces_actual_attack_without_double_strength_scaling() {
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

        assert!((d.derived.stats.soft_attack - full * 0.01).abs() < 1e-9);
        assert!((d.strength() - 0.01).abs() < 1e-9);
        assert_eq!(d.combat_strength(), 1.);
        // Matching personnel/equipment/HP losses must not square the loss.
        let mut half = d.clone();
        half.manpower = 10;
        half.hp = half.derived.stats.hp * 0.01;
        assert_eq!(half.combat_strength(), 1.);
    }
    #[test]
    fn encircled_unit_keeps_its_men_rifles_and_national_stock_unchanged() {
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

        assert_eq!(w.divisions[&1].manpower, 500);
        assert_eq!(w.divisions[&1].equipment["rifle_1"], 1);
        assert_eq!(p.country("A").unwrap().model_stockpile["rifle_1"], 99);
    }
    #[test]
    fn reconnected_supply_refills_reserves_and_survives_a_second_cut() {
        let (mut world, mut w) = setup();
        w.logistics.rules.rail_capacity_per_level = 30.;
        w.divisions.get_mut(&1).unwrap().supply_fraction = 0.;
        for h in 0..49 {
            tick(&mut w, &world, h);
        }

        assert!(w.logistics.divisions[&1].supply > 0.);
        assert_eq!(w.divisions[&1].supply_fraction, 1.);
        world.set_province_controller(3, Some("B"));
        tick(&mut w, &world, 49);
        assert!(w.divisions[&1].supply_fraction > 0.);
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
    fn cas_hour_budget_is_conserved_when_first_battle_ends() {
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

        assert!(w.battles[&b1].outcome.is_some());
        assert!((x + y - 100.).abs() < 1e-9);
        assert!((hp - 3.5).abs() < 1e-9);
    }
    #[test]
    fn one_cas_plane_is_allocated_to_one_battle_instead_of_disappearing() {
        let (mut world, mut w, b1, b2) = battle_pair();
        let r = w.air.results.get_mut(&1).unwrap().get_mut("A").unwrap();
        r.cas = 1;
        r.ground_attack = 8.;
        let hp = w.divisions[&2].hp + w.divisions[&4].hp;
        w.tick_hour(&mut world, crate::game::GameDate::START, 12, 9, Some("A"));
        let x = &w.battles[&b1].history.back().unwrap().attacker_air;
        let y = &w.battles[&b2].history.back().unwrap().attacker_air;

        assert_eq!(x.cas_planes + y.cas_planes, 1);
        assert!(hp > w.divisions[&2].hp + w.divisions[&4].hp);
    }
    #[test]
    fn fuel_refill_respects_rail_bottlenecks_and_shares_transport_capacity() {
        let (world, mut w) = setup();
        w.logistics.rules.rail_capacity_per_level = 0.0001;
        w.divisions.get_mut(&1).unwrap().fuel_fraction = 0.;
        tick(&mut w, &world, 0);
        let s = &w.logistics.divisions[&1];

        assert!(s.received < 0.001);
        assert!(w.logistics.countries["A"].fuel_used_last_hour < 0.001);
        assert!(s.received + s.fuel_received * 24. * w.logistics.rules.fuel_transport_weight <= 0.000500001);
        assert!(w.divisions[&1].fuel_fraction < 0.001);
    }

    #[test]
    fn new_recruitment_cannot_create_fuel_without_a_country_source() {
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
        let mut logistics = std::mem::take(&mut w.logistics);
        logistics.refresh_network(&w, &world);
        w.logistics = logistics;
        w.queue_recruitment("A", "test", 2, &world, &mut p).unwrap();
        let mut world = world;
        for h in 1..=25 {
            w.tick_hour(&mut world, crate::game::GameDate::START, 12, h, Some("A"));
        }
        let id = *w.divisions.keys().next().unwrap();

        assert_eq!(w.logistics.countries["A"].fuel, 0.);
        assert_eq!(w.logistics.countries["A"].fuel_used_last_hour, 0.);
        assert_eq!(w.logistics.divisions[&id].fuel, 0.);
        assert_eq!(w.divisions[&id].fuel_fraction, 0.);
    }

    #[test]
    fn equipment_reliability_modifier_survives_aggregation() {
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

        assert!((result.stats.reliability - 1.0).abs() < 1e-9);
    }
}

mod further_regressions {
    use super::*;
    use crate::military_model::MilitaryModifiers;
    #[test]
    fn aircraft_sorties_consume_tech_adjusted_fuel_and_apply_actual_ground_attack() {
        let (world, mut w) = air_setup();
        w.air.wings.insert(1, wing(1, "A", "cas", 1, AirMission::CloseAirSupport));
        let modifiers = &mut w.countries.entry("A".into()).or_default().modifiers;
        modifiers.equipment.insert("cas".into(), BTreeMap::from([
            ("air_ground_attack".into(), 1.), ("fuel_consumption".into(), -0.5),
        ]));
        air_tick(&mut w, &world, 8);
        assert_eq!(w.air.wings[&1].sorties, 100);
        assert_eq!(w.air.results[&1]["A"].ground_attack, 2000.);
        assert_eq!(w.logistics.countries["A"].fuel, 9950.);
    }
    #[test]
    fn effective_reliability_changes_actual_wear_without_manpower_or_hp_loss() {
        let run = |protected| {
            let (world, mut w) = setup();
            let c = std::sync::Arc::make_mut(&mut w.catalogue);
            let infantry = c.units.get_mut("infantry").unwrap();
            infantry.need.insert("rifle".into(), 100.);
            infantry.stats.insert("soft_attack".into(), 0.);
            c.equipment.insert("rifle_1".into(), EquipmentDefinition {
                id: "rifle_1".into(), archetype: "rifle".into(), year: 1936, active: true,
                buildable: true, modular: false, cost: 1., source: "authored".into(),
                stats: BTreeMap::from([("reliability".into(), 0.8), ("soft_attack".into(), 50.)]),
            });
            let mut modifiers = MilitaryModifiers::default();
            if protected { modifiers.equipment.insert("rifle".into(), BTreeMap::from([("reliability".into(), 0.25)])); }
            w.countries.entry("A".into()).or_default().modifiers = modifiers.clone();
            w.countries.get_mut("A").unwrap().templates.insert("test".into(), crate::military_model::tests::template());
            let d = w.divisions.get_mut(&1).unwrap();
            d.equipment.insert("rifle_1".into(), 100);
            d.derived = w.catalogue.aggregate(&crate::military_model::tests::template(), &modifiers, Some(&d.equipment)).unwrap();
            d.order = crate::military_model::DivisionOrder::Train { hours: 0. };
            w.logistics.rules.exercise_attrition = 24.;
            w.logistics.rules.attrition_loss_factor = 1.;
            tick(&mut w, &world, 0);
            w.divisions.remove(&1).unwrap()
        };
        let healthy = run(true);
        let worn = run(false);
        assert_eq!(healthy.equipment["rifle_1"], 100);
        assert_eq!(worn.equipment["rifle_1"], 80);
        assert_eq!(worn.derived.stats.soft_attack, 40.);
        assert_eq!(healthy.derived.stats.soft_attack, 50.);
        assert_eq!(healthy.hp, worn.hp);
        assert_eq!(healthy.manpower, worn.manpower);
    }
}
