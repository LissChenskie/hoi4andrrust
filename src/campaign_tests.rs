//! Authored engine/regression fixtures, not original-game parity experiments.
use crate::{
    campaign::*,
    civil_war::{Share, SplitPlan},
    military_model::{
        CountryArmy, EquipmentDefinition, StatBlock,
        tests::{catalogue, template},
    },
    pdx::parse_pdx,
    research::{ResearchCatalogue, ResearchRules},
    session::GameSession,
};
use std::collections::{BTreeMap, BTreeSet};
fn fixture() -> GameSession {
    let mut w = crate::warfare::tests::world();
    w.states.get_mut(&2).unwrap().owner = Some("A".into());
    w.countries.get_mut("B").unwrap().state_ids.clear();
    w.countries.get_mut("A").unwrap().state_ids.insert(2);
    w.set_province_controller(2, Some("A"));
    w.set_province_controller(3, Some("A"));
    let mut s = GameSession::new(w);
    let mut db = catalogue();
    db.units
        .get_mut("infantry")
        .unwrap()
        .need
        .insert("rifles".into(), 100.);
    db.units
        .get_mut("infantry")
        .unwrap()
        .stats
        .insert("training_time".into(), 1.);
    for (id, arch) in [
        ("rifle", "rifles"),
        ("fighter", "fighter_equipment"),
        ("ship", "destroyer"),
    ] {
        db.equipment.insert(
            id.into(),
            EquipmentDefinition {
                id: id.into(),
                archetype: arch.into(),
                year: 1936,
                active: true,
                buildable: true,
                modular: false,
                cost: 1.,
                source: "authored".into(),
                stats: StatBlock::from([("soft_attack".into(), 1.), ("air_range".into(), 1000.)]),
            },
        );
    }
    s.military = crate::warfare::Warfare::new(db);
    s.military.ai_enabled = false;
    s.military.countries.insert(
        "A".into(),
        CountryArmy {
            templates: BTreeMap::from([("test".into(), template())]),
            manpower_pool: 10001,
            army_xp: 1000.,
            ..Default::default()
        },
    );
    s.military
        .spawn_history("A", "test", 1, "loyal", 1., 1., 0., &s.world)
        .unwrap();
    s.military
        .spawn_history("A", "test", 2, "rebel", 1., 1., 0., &s.world)
        .unwrap();
    s.production
        .country_mut("A")
        .model_stockpile
        .insert("rifle".into(), 101);
    let c = s.research.country_mut("A", 2, &Default::default());
    c.context.tag = "A".into();
    c.context.original_tag = "A".into();
    c.context.variables.insert("paranoia".into(), 0.);
    s.campaign.countries.insert(
        "A".into(),
        CountryPolitics {
            power: Some(100.),
            stability: Some(0.5),
            war_support: Some(0.5),
        },
    );
    s
}
fn run(s: &mut GameSession, b: &str) -> Result<(), String> {
    s.campaign_effects(
        &Context::country("A"),
        &parse_pdx(b).unwrap(),
        "fixture",
        &Default::default(),
        &Default::default(),
    )
}
fn snap(s: &GameSession) -> serde_json::Value {
    serde_json::to_value(s).unwrap()
}
fn plan() -> SplitPlan {
    SplitPlan {
        parent: "A".into(),
        rebel: "D01".into(),
        original_tag: "A".into(),
        ideology: "democratic".into(),
        states: BTreeSet::from([2]),
        divisions: BTreeSet::from([2]),
        commanders: BTreeSet::new(),
        wings: BTreeSet::new(),
        fleets: BTreeSet::new(),
        admirals: BTreeSet::new(),
        production_lines: BTreeSet::new(),
        loyal_capital: 1,
        rebel_capital: 2,
        resources: Share {
            numerator: 1,
            denominator: 3,
        },
        population: Share {
            numerator: 1,
            denominator: 3,
        },
        inherit_flags: false,
        inherit_variables: false,
        inherit_focuses: false,
    }
}
fn add_event(cat: &mut ResearchCatalogue, id: &str, raw: &str) {
    cat.scripts.sources.insert(
        format!("event:{id}:immediate"),
        crate::research_script::ScriptSource {
            kind: "event".into(),
            id: format!("{id}:immediate"),
            source: "authored/events.txt".into(),
            conditions: vec![],
            effects: vec![],
            raw: parse_pdx(raw).unwrap(),
        },
    );
}
#[test]
fn campaign_unsupported_and_invalid_rewards_never_partially_commit() {
    for b in [
        "add_manpower=1 unknown_effect=yes",
        "if={limit={NOT={tag=FROM}} add_manpower=999}",
        "add_manpower=1 if={limit={always=no} unknown_effect=yes}",
        "add_manpower=1 divide_variable={var=paranoia value=0}",
        "add_manpower=-10002",
        "add_stability=NaN",
        "add_equipment_to_stockpile={type=missing amount=1}",
        "if={limit={NOT={unknown_trigger=yes}} add_manpower=999}",
    ] {
        let mut s = fixture();
        let before = snap(&s);
        assert!(run(&mut s, b).is_err(), "{b}");
        assert_eq!(snap(&s), before);
    }
}
#[test]
fn campaign_reward_changes_real_industry_inventory_and_country_flags() {
    let mut s = fixture();
    run(&mut s,"2={add_building_construction={type=arms_factory level=1 instant_build=yes} owner={add_manpower=7 PREV={set_state_flag=industrial}}} add_equipment_to_stockpile={type=rifle amount=11} set_variable={var=paranoia value=20} add_political_power=-5").unwrap();
    assert_eq!(s.world.country_economy("A").unwrap().military_factories, 1);
    assert_eq!(s.military.countries["A"].manpower_pool, 10008);
    assert_eq!(
        s.production.country("A").unwrap().model_stockpile["rifle"],
        112
    );
    assert!(s.campaign.state_flags[&2].contains("industrial"));
    assert_eq!(s.campaign.countries["A"].power, Some(95.));
}
#[test]
fn campaign_macros_are_bounded_parameterized_and_cannot_inject_source() {
    let mut s = fixture();
    let mut cat = ResearchCatalogue::default();
    cat.scripts
        .effects
        .insert("reward".into(), parse_pdx("add_manpower=$N$").unwrap());
    s.campaign_effects(
        &Context::country("A"),
        &parse_pdx("reward={N=7}").unwrap(),
        "fixture",
        &cat,
        &Default::default(),
    )
    .unwrap();
    assert_eq!(s.military.countries["A"].manpower_pool, 10008);
    let before = snap(&s);
    assert!(
        s.campaign_effects(
            &Context::country("A"),
            &parse_pdx("reward={N=\"1 } add_manpower=999\"}").unwrap(),
            "fixture",
            &cat,
            &Default::default()
        )
        .is_err()
    );
    assert_eq!(snap(&s), before);
    cat.scripts.effects.insert(
        "loop_effect".into(),
        parse_pdx("add_manpower=1 loop_effect=yes").unwrap(),
    );
    assert!(
        s.campaign_effects(
            &Context::country("A"),
            &parse_pdx("loop_effect=yes").unwrap(),
            "fixture",
            &cat,
            &Default::default()
        )
        .is_err()
    );
    assert_eq!(snap(&s), before);
}
#[test]
fn campaign_seeded_randomness_and_iterators_replay() {
    let mut a = fixture();
    let mut b = fixture();
    let script = "every_owned_state={limit={always=yes} add_building_construction={type=arms_factory level=1 instant_build=yes}} random_list={1={add_manpower=1} 2={add_manpower=2}} random_owned_state={set_state_flag=selected}";
    for _ in 0..20 {
        run(&mut a, script).unwrap();
        run(&mut b, script).unwrap();
    }
    assert_eq!(snap(&a), snap(&b));
    assert_eq!(a.world.country_economy("A").unwrap().military_factories, 40);
}
#[test]
fn campaign_failed_scheduled_action_is_quarantined() {
    let mut s = fixture();
    s.campaign
        .schedule(
            1,
            Context::country("A"),
            "bad".into(),
            Payload::Effects(
                parse_pdx("add_manpower=1 divide_variable={var=paranoia value=0}").unwrap(),
            ),
        )
        .unwrap();
    for h in 1..5 {
        s.clock.elapsed_hours = h;
        s.tick_campaign(&Default::default(), &Default::default());
    }
    assert_eq!(s.military.countries["A"].manpower_pool, 10001);
    assert!(s.campaign.queue[&1].blocked.is_some());
}
#[test]
fn campaign_event_choice_is_applied_once_and_delays_use_real_time() {
    let mut s = fixture();
    let mut cat = ResearchCatalogue::default();
    add_event(
        &mut cat,
        "test.1",
        "id=test.1 is_triggered_only=yes fire_only_once=yes immediate={add_to_variable={var=paranoia value=5}} option={name=accept add_manpower=10}",
    );
    s.campaign_effects(
        &Context::country("A"),
        &parse_pdx("country_event={id=test.1 hours=3}").unwrap(),
        "fixture",
        &cat,
        &Default::default(),
    )
    .unwrap();
    for _ in 0..2 {
        s.advance_one_hour(
            &Default::default(),
            &Default::default(),
            &cat,
            &Default::default(),
        );
    }
    assert!(s.campaign.pending.is_empty());
    s.advance_one_hour(
        &Default::default(),
        &Default::default(),
        &cat,
        &Default::default(),
    );
    assert_eq!(s.campaign.pending.len(), 1);
    let id = *s.campaign.pending.keys().next().unwrap();
    s.choose_campaign_event(id, 0, &cat, &Default::default())
        .unwrap();
    assert!(
        s.choose_campaign_event(id, 0, &cat, &Default::default())
            .is_err()
    );
    assert_eq!(
        s.research.country("A").unwrap().context.variables["paranoia"],
        5.
    );
}
#[test]
fn campaign_event_recursion_has_a_per_hour_budget() {
    let mut s = fixture();
    let mut cat = ResearchCatalogue::default();
    add_event(
        &mut cat,
        "loop.1",
        "id=loop.1 hidden=yes immediate={country_event={id=loop.1}}",
    );
    s.campaign_effects(
        &Context::country("A"),
        &parse_pdx("country_event={id=loop.1}").unwrap(),
        "fixture",
        &cat,
        &Default::default(),
    )
    .unwrap();
    s.clock.elapsed_hours = 1;
    s.tick_campaign(&cat, &Default::default());
    assert_eq!(s.campaign.next_id, 66);
    assert_eq!(s.campaign.queue.len(), 1);
    s.tick_campaign(&cat, &Default::default());
    assert_eq!(s.campaign.next_id, 66);
}
#[test]
fn campaign_transitive_unsupported_event_blocks_whole_reward() {
    let mut s = fixture();
    let mut cat = ResearchCatalogue::default();
    add_event(
        &mut cat,
        "bad.1",
        "id=bad.1 hidden=yes immediate={unimplemented_purge=yes}",
    );
    let before = snap(&s);
    assert!(
        s.campaign_effects(
            &Context::country("A"),
            &parse_pdx("add_manpower=1 country_event={id=bad.1}").unwrap(),
            "fixture",
            &cat,
            &Default::default()
        )
        .is_err()
    );
    assert_eq!(snap(&s), before);
}
#[test]
fn campaign_clearing_flags_reaches_military_state() {
    let mut s = fixture();
    s.military
        .countries
        .get_mut("A")
        .unwrap()
        .modifiers
        .flags
        .insert("history".into());
    run(&mut s, "clr_country_flag=history set_country_flag=runtime").unwrap();
    assert!(
        !s.military.countries["A"]
            .modifiers
            .flags
            .contains("history")
    );
    assert!(
        s.military.countries["A"]
            .modifiers
            .flags
            .contains("runtime")
    );
    run(&mut s, "clr_country_flag=runtime").unwrap();
    assert!(
        !s.military.countries["A"]
            .modifiers
            .flags
            .contains("runtime")
    );
}
#[test]
fn campaign_spirit_changes_actual_attack_and_unknown_consumer_aborts() {
    let mut s = fixture();
    let mut cat = ResearchCatalogue::default();
    cat.scripts.ideas.insert(
        "spirit".into(),
        parse_pdx("modifier={army_attack_factor=0.5}").unwrap(),
    );
    let before = s.military.divisions[&1].derived.stats.soft_attack;
    s.campaign_effects(
        &Context::country("A"),
        &parse_pdx("add_ideas=spirit").unwrap(),
        "fixture",
        &cat,
        &Default::default(),
    )
    .unwrap();
    assert!((s.military.divisions[&1].derived.stats.soft_attack - before * 1.5).abs() < 1e-9);
    cat.scripts.ideas.insert(
        "inert".into(),
        parse_pdx("modifier={not_implemented=1}").unwrap(),
    );
    let before = snap(&s);
    assert!(
        s.campaign_effects(
            &Context::country("A"),
            &parse_pdx("add_manpower=1 add_ideas=inert").unwrap(),
            "fixture",
            &cat,
            &Default::default()
        )
        .is_err()
    );
    assert_eq!(snap(&s), before);
}
#[test]
fn civil_war_conserves_resources_and_rejects_duplicate_and_invalid_splits() {
    for n in 0..=7 {
        let mut s = fixture();
        let mut p = plan();
        p.resources = Share {
            numerator: n,
            denominator: 7,
        };
        s.military.logistics.countries.insert(
            "A".into(),
            crate::logistics::CountryLogistics {
                fuel: 1000.,
                fuel_capacity: 2000.,
                fuel_capacity_base: Some(2000.),
                convoys: 101,
                ..Default::default()
            },
        );
        s.start_civil_war(&p, &Default::default()).unwrap();
        assert_eq!(
            s.production
                .by_country
                .values()
                .map(|p| p.model_stockpile["rifle"])
                .sum::<u64>(),
            101
        );
        assert_eq!(
            s.military
                .countries
                .values()
                .map(|a| a.manpower_pool)
                .sum::<u64>(),
            10001
        );
        assert_eq!(
            s.military
                .logistics
                .countries
                .values()
                .map(|a| a.convoys)
                .sum::<u64>(),
            101
        );
        assert!(
            (s.military
                .logistics
                .countries
                .values()
                .map(|a| a.fuel)
                .sum::<f64>()
                - 1000.)
                .abs()
                < 1e-9
        );
        assert_eq!(s.military.divisions[&2].country, "D01");
        s.military.validate_references(&s.world).unwrap();
        let before = snap(&s);
        assert!(s.start_civil_war(&p, &Default::default()).is_err());
        assert_eq!(snap(&s), before);
    }
    let mut s = fixture();
    let mut p = plan();
    p.divisions.clear();
    let before = snap(&s);
    assert!(s.start_civil_war(&p, &Default::default()).is_err());
    assert_eq!(snap(&s), before);
}
#[test]
fn civil_war_either_victor_preserves_stock_and_colliding_production_ids() {
    for winner in ["A", "D01"] {
        let mut s = fixture();
        s.start_civil_war(&plan(), &Default::default()).unwrap();
        for tag in ["A", "D01"] {
            let p = s.production.country_mut(tag);
            p.create_model(
                "rifle",
                crate::production::Equipment::InfantryKit,
                1,
                &Default::default(),
            );
            p.lines[0].partial_ic = 0.25;
        }
        let before = snap(&s);
        assert!(s.settle_civil_war("D01", winner).is_err());
        assert_eq!(snap(&s), before);
        for p in [1, 2, 3] {
            s.world.set_province_controller(p, Some(winner));
        }
        s.settle_civil_war("D01", winner).unwrap();
        let p = s.production.country(winner).unwrap();
        assert_eq!(p.model_stockpile["rifle"], 101);
        assert_eq!(p.lines.len(), 2);
        assert_ne!(p.lines[0].id, p.lines[1].id);
        assert_eq!(p.lines.iter().map(|l| l.partial_ic).sum::<f64>(), 0.5);
        assert!(s.military.divisions.values().all(|d| d.country == winner));
        s.military.validate_references(&s.world).unwrap();
        assert!(s.settle_civil_war("D01", winner).is_err());
    }
}
#[test]
fn civil_war_queued_transition_survives_save_load_and_replay() {
    let mut s = fixture();
    let cat = ResearchCatalogue::default();
    s.schedule_civil_war(plan(), 3).unwrap();
    for _ in 0..2 {
        s.advance_one_hour(
            &Default::default(),
            &Default::default(),
            &cat,
            &Default::default(),
        );
    }
    let path = std::env::temp_dir().join(format!("hoi-civil-{}.json", std::process::id()));
    s.save_game(&path, &cat).unwrap();
    let mut restored = fixture();
    restored.restore_game(&path, &cat).unwrap();
    for g in [&mut s, &mut restored] {
        for _ in 0..96 {
            g.advance_one_hour(
                &Default::default(),
                &Default::default(),
                &cat,
                &Default::default(),
            );
        }
    }
    assert!(restored.world.countries.contains_key("D01"));
    assert_eq!(
        serde_json::to_value(&s.campaign).unwrap(),
        serde_json::to_value(&restored.campaign).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&s.production).unwrap(),
        serde_json::to_value(&restored.production).unwrap()
    );
    std::fs::remove_file(path).unwrap();
}
#[test]
fn rename_during_recruitment_keeps_reserved_equipment_and_reference() {
    let mut s = fixture();
    let id = s
        .military
        .queue_recruitment("A", "test", 1, &s.world, &mut s.production)
        .unwrap();
    let mut t = template();
    t.name = "renamed".into();
    s.military.save_template("A", Some("test"), t).unwrap();
    assert_eq!(s.military.recruitment[&id].template.name, "renamed");
    for h in 1..=24 {
        s.military
            .tick_hour(&mut s.world, s.clock.date, 12, h, Some("A"));
    }
    assert!(!s.military.countries["A"].templates.contains_key("test"));
    assert!(
        s.military
            .divisions
            .values()
            .all(|d| d.template == "renamed")
    );
}
#[test]
fn mixed_equipment_replenishment_cannot_destroy_existing_infantry() {
    let mut s = fixture();
    let db = std::sync::Arc::make_mut(&mut s.military.catalogue);
    db.units
        .get_mut("tank")
        .unwrap()
        .need
        .insert("tank_equipment".into(), 50.);
    let mut eq = db.equipment["rifle"].clone();
    eq.id = "tank_eq".into();
    eq.archetype = "tank_equipment".into();
    db.equipment.insert(eq.id.clone(), eq);
    let mut t = template();
    t.line.push(crate::military_model::BattalionSlot {
        unit: "tank".into(),
        column: 1,
        row: 0,
    });
    let before = s.military.divisions[&1].hp;
    s.military.save_template("A", Some("test"), t).unwrap();
    for _ in 0..24 {
        s.military.replenish(&mut s.production);
    }
    assert!(s.military.divisions[&1].hp >= before);
    assert!(s.military.divisions[&1].fighting());
}
#[test]
fn equipment_speed_modifier_reaches_preview_and_actual_inventory() {
    let mut db = catalogue();
    let u = db.units.get_mut("tank").unwrap();
    u.need.insert("tank_equipment".into(), 50.);
    u.equipment_speed_multiplier = true;
    u.stats.insert("maximum_speed".into(), 0.);
    db.equipment.insert(
        "model".into(),
        EquipmentDefinition {
            id: "model".into(),
            archetype: "tank_equipment".into(),
            year: 1936,
            active: true,
            buildable: true,
            modular: false,
            cost: 1.,
            source: "authored".into(),
            stats: StatBlock::from([("maximum_speed".into(), 12.)]),
        },
    );
    let mut t = template();
    t.line[0].unit = "tank".into();
    let mut m = crate::military_model::MilitaryModifiers::default();
    m.equipment.insert(
        "tank_equipment".into(),
        StatBlock::from([("maximum_speed".into(), 0.5)]),
    );
    assert_eq!(db.aggregate(&t, &m, None).unwrap().stats.speed, 18.);
    assert_eq!(
        db.aggregate(&t, &m, Some(&BTreeMap::from([("model".into(), 50)])))
            .unwrap()
            .stats
            .speed,
        18.
    );
}

#[test]
fn civil_war_transfers_real_air_navy_training_and_construction_without_cloning() {
    let mut s = fixture();
    let mut p = plan();
    s.military.air.wings.insert(
        1,
        crate::air_warfare::AirWing {
            id: 1,
            name: "wing".into(),
            country: "A".into(),
            model: "fighter".into(),
            base: 2,
            planes: 37,
            target_planes: 50,
            mission: crate::air_warfare::AirMission::Standby,
            region: None,
            sorties: 0,
            losses: 0,
        },
    );
    s.military.air.next_id = 1;
    p.wings.insert(1);
    s.military.naval.task_forces.insert(
        1,
        crate::naval_warfare::NavalTaskForce {
            id: 1,
            country: "A".into(),
            name: "fleet".into(),
            base: 2,
            mission: crate::naval_warfare::NavalMission::Hold,
            regions: BTreeSet::new(),
            ships: vec![crate::naval_warfare::NavalShip {
                name: "ship".into(),
                model: "ship".into(),
                definition: "ship".into(),
                variant: None,
                strength: 0.75,
            }],
        },
    );
    s.military.naval.next_id = 1;
    p.fleets.insert(1);
    let job = s
        .military
        .queue_recruitment("A", "test", 2, &s.world, &mut s.production)
        .unwrap();
    s.military.recruitment.get_mut(&job).unwrap().training_hours = 5.;
    s.construction_queues.entry(2).or_default().push_back(
        crate::construction::ConstructionProject {
            builder: None,
            building: crate::construction::BuildingType::MilitaryFactory,
            progress: 17.,
            cost: 100.,
        },
    );
    s.construction
        .get_mut("A")
        .unwrap()
        .policy
        .consumer_goods_fraction = 0.2;
    let men =
        s.military.countries["A"].manpower_pool + u64::from(s.military.recruitment[&job].manpower);
    s.start_civil_war(&p, &Default::default()).unwrap();
    assert_eq!(s.military.air.wings[&1].country, "D01");
    assert_eq!(s.military.air.wings[&1].planes, 37);
    assert_eq!(s.military.naval.task_forces[&1].country, "D01");
    assert_eq!(s.military.naval.task_forces[&1].ships[0].strength, 0.75);
    assert_eq!(s.military.recruitment[&job].country, "D01");
    assert_eq!(s.military.recruitment[&job].training_hours, 5.);
    assert_eq!(
        s.military
            .countries
            .values()
            .map(|a| a.manpower_pool)
            .sum::<u64>()
            + u64::from(s.military.recruitment[&job].manpower),
        men
    );
    assert_eq!(
        s.production
            .by_country
            .values()
            .map(|p| p.model_stockpile["rifle"])
            .sum::<u64>()
            + u64::from(s.military.recruitment[&job].equipment["rifle"]),
        101
    );
    assert_eq!(s.construction_queues[&2][0].builder.as_deref(), Some("D01"));
    assert_eq!(s.construction_queues[&2][0].progress, 17.);
    assert_eq!(s.construction["D01"].policy.consumer_goods_fraction, 0.2);
}
