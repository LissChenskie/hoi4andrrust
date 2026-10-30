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
        s.start_civil_war(&p, &Default::default(), &Default::default())
            .unwrap();
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
        assert!(
            s.start_civil_war(&p, &Default::default(), &Default::default())
                .is_err()
        );
        assert_eq!(snap(&s), before);
    }
    let mut s = fixture();
    let mut p = plan();
    p.divisions.clear();
    let before = snap(&s);
    assert!(
        s.start_civil_war(&p, &Default::default(), &Default::default())
            .is_err()
    );
    assert_eq!(snap(&s), before);
}
#[test]
fn civil_war_either_victor_preserves_stock_and_colliding_production_ids() {
    for winner in ["A", "D01"] {
        let mut s = fixture();
        s.start_civil_war(&plan(), &Default::default(), &Default::default())
            .unwrap();
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
        assert!(
            s.settle_civil_war("D01", winner, &Default::default(), &Default::default())
                .is_err()
        );
        assert_eq!(snap(&s), before);
        for p in [1, 2, 3] {
            s.world.set_province_controller(p, Some(winner));
        }
        s.settle_civil_war("D01", winner, &Default::default(), &Default::default())
            .unwrap();
        let p = s.production.country(winner).unwrap();
        assert_eq!(p.model_stockpile["rifle"], 101);
        assert_eq!(p.lines.len(), 2);
        assert_ne!(p.lines[0].id, p.lines[1].id);
        assert_eq!(p.lines.iter().map(|l| l.partial_ic).sum::<f64>(), 0.5);
        assert!(s.military.divisions.values().all(|d| d.country == winner));
        s.military.validate_references(&s.world).unwrap();
        assert!(
            s.settle_civil_war("D01", winner, &Default::default(), &Default::default())
                .is_err()
        );
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
    s.start_civil_war(&p, &Default::default(), &Default::default())
        .unwrap();
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

#[test]
fn political_setters_government_and_popularity_affect_real_conditions_atomically() {
    let mut s = fixture();
    let cat = ResearchCatalogue::default();
    let rules = ResearchRules::default();
    s.campaign.countries.get_mut("A").unwrap().power = None;
    let b=parse_pdx("set_political_power=75 set_stability=0.8 set_war_support=0.6 set_popularities={democratic=30 neutrality=70} set_politics={ruling_party=democratic elections_allowed=no name=party_short long_name=party_long}").unwrap();
    s.campaign_effects(&Context::country("A"), &b, "authored", &cat, &rules)
        .unwrap();
    assert_eq!(s.campaign.countries["A"].power, Some(75.));
    assert!(s.campaign_conditions(&Context::country("A"),&parse_pdx("has_government=democratic has_elections=no has_stability>0.7 has_war_support>0.5 has_political_power>50 democratic>0.2").unwrap(),&cat,&rules).unwrap());
    assert_eq!(
        s.research.countries["A"].context.party_names["democratic"]
            .0
            .as_deref(),
        Some("party_short")
    );
    let before = snap(&s);
    assert!(
        s.campaign_effects(
            &Context::country("A"),
            &parse_pdx("add_political_power=3 set_popularities={democratic=12 neutrality=70}")
                .unwrap(),
            "authored",
            &cat,
            &rules
        )
        .is_err()
    );
    assert_eq!(snap(&s), before);
    // No fake election scheduler: unsupported timer changes still block.
    assert!(
        s.campaign_effects(
            &Context::country("A"),
            &parse_pdx("set_politics={last_election=1936.1.1 election_frequency=48}").unwrap(),
            "authored",
            &cat,
            &rules
        )
        .is_err()
    );
}

fn dynamic_cat() -> ResearchCatalogue {
    let mut cat = ResearchCatalogue::default();
    cat.scripts.defines.extend([
        (
            "NDefines.NPolitics.BASE_POLITICAL_POWER_INCREASE".into(),
            2.,
        ),
        ("NDefines.NCountry.POLITICAL_POWER_LOWER_CAP".into(), -500.),
        ("NDefines.NCountry.POLITICAL_POWER_UPPER_CAP".into(), 2000.),
    ]);
    cat.scripts.dynamic_modifiers.insert("authored".into(),parse_pdx("enable={has_country_flag=enabled} remove_trigger={has_country_flag=remove} political_power_gain=var:ROOT.bonus?0 research_speed_factor=0.2 army_attack_factor=0.1").unwrap());
    cat
}
fn dynamic_run(s: &mut GameSession, cat: &ResearchCatalogue, txt: &str) -> Result<(), String> {
    s.campaign_effects(
        &Context::country("A"),
        &parse_pdx(txt).unwrap(),
        "authored/dynamic",
        cat,
        &Default::default(),
    )
}
#[test]
fn dynamic_country_modifier_updates_actual_consumers_and_enable_remove_conditions() {
    let mut s = fixture();
    let cat = dynamic_cat();
    dynamic_run(
        &mut s,
        &cat,
        "set_variable={var=bonus value=0.5} add_dynamic_modifier={modifier=authored days=2}",
    )
    .unwrap();
    assert_eq!(
        s.research.countries["A"].modifier("research_speed_factor"),
        0.
    );
    assert!(
        s.campaign_conditions(
            &Context::country("A"),
            &parse_pdx("has_dynamic_modifier=authored").unwrap(),
            &cat,
            &Default::default()
        )
        .unwrap()
    );
    dynamic_run(&mut s, &cat, "set_country_flag=enabled").unwrap();
    s.tick_dynamic_modifiers(&cat, &Default::default());
    assert_eq!(
        s.research.countries["A"].modifier("research_speed_factor"),
        0.2
    );
    assert!(
        (s.military.countries["A"]
            .modifiers
            .value("army_attack_factor")
            - 0.1)
            .abs()
            < 1e-9
    );
    assert_eq!(s.political_income("A", &cat).unwrap().net, 2.5);
    dynamic_run(&mut s, &cat, "set_variable={var=bonus value=1.5}").unwrap();
    s.clock.elapsed_hours += 24;
    s.tick_dynamic_modifiers(&cat, &Default::default());
    assert_eq!(s.political_income("A", &cat).unwrap().net, 3.5);
    dynamic_run(&mut s, &cat, "set_country_flag=remove").unwrap();
    s.tick_dynamic_modifiers(&cat, &Default::default());
    assert!(!s.campaign.dynamic_modifiers["A"].contains_key("authored"));
    assert_eq!(
        s.research.countries["A"].modifier("research_speed_factor"),
        0.
    );
}
#[test]
fn timed_dynamic_modifier_renews_without_stacking_and_survives_save_then_expires() {
    let mut s = fixture();
    let cat = dynamic_cat();
    dynamic_run(
        &mut s,
        &cat,
        "set_country_flag=enabled add_dynamic_modifier={modifier=authored days=2}",
    )
    .unwrap();
    assert_eq!(
        s.campaign.dynamic_modifiers["A"]["authored"].expires,
        Some(48)
    );
    s.clock.elapsed_hours = 24;
    dynamic_run(
        &mut s,
        &cat,
        "add_dynamic_modifier={modifier=authored days=2}",
    )
    .unwrap();
    assert_eq!(
        s.campaign.dynamic_modifiers["A"]["authored"].expires,
        Some(72)
    );
    assert_eq!(
        s.research.countries["A"].modifier("research_speed_factor"),
        0.2
    );
    let db = s.military.catalogue.clone();
    let bytes = serde_json::to_vec(&s).unwrap();
    s = serde_json::from_slice(&bytes).unwrap();
    s.military.catalogue = db;
    s.clock.elapsed_hours = 48;
    s.tick_dynamic_modifiers(&cat, &Default::default());
    assert_eq!(
        s.research.countries["A"].modifier("research_speed_factor"),
        0.2
    );
    s.clock.elapsed_hours = 72;
    s.tick_dynamic_modifiers(&cat, &Default::default());
    assert_eq!(
        s.research.countries["A"].modifier("research_speed_factor"),
        0.
    );
    assert_eq!(
        s.military.countries["A"]
            .modifiers
            .value("army_attack_factor"),
        0.
    );
}
#[test]
fn unsupported_dynamic_consumers_targeted_scope_and_bad_duration_are_atomic() {
    let mut s = fixture();
    let mut cat = dynamic_cat();
    cat.scripts
        .dynamic_modifiers
        .insert("bad".into(), parse_pdx("unknown_modifier=1").unwrap());
    for script in [
        "add_political_power=3 add_dynamic_modifier={modifier=bad}",
        "add_political_power=3 add_dynamic_modifier={modifier=authored scope=B}",
        "add_political_power=3 add_dynamic_modifier={modifier=authored days=-1}",
    ] {
        let before = snap(&s);
        assert!(dynamic_run(&mut s, &cat, script).is_err());
        assert_eq!(snap(&s), before);
    }
}
#[test]
fn failed_dynamic_refresh_removes_stale_bonuses_and_blocks_unverified_pp_income() {
    let mut s = fixture();
    let mut cat = dynamic_cat();
    dynamic_run(
        &mut s,
        &cat,
        "set_country_flag=enabled add_dynamic_modifier={modifier=authored}",
    )
    .unwrap();
    cat.scripts.dynamic_modifiers.insert(
        "authored".into(),
        parse_pdx("political_power_gain=unknown_variable").unwrap(),
    );
    s.tick_dynamic_modifiers(&cat, &Default::default());
    assert!(
        s.campaign.dynamic_modifiers["A"]["authored"]
            .blocked
            .is_some()
    );
    assert_eq!(
        s.research.countries["A"].modifier("research_speed_factor"),
        0.
    );
    assert!(s.political_income("A", &cat).is_err());
    dynamic_run(&mut s, &cat, "remove_dynamic_modifier={modifier=authored}").unwrap();
    assert_eq!(s.political_income("A", &cat).unwrap().net, 2.);
}

#[test]
fn active_dynamic_modifiers_require_explicit_split_inheritance_instead_of_leaking() {
    let mut s = fixture();
    let cat = dynamic_cat();
    dynamic_run(
        &mut s,
        &cat,
        "set_country_flag=enabled add_dynamic_modifier={modifier=authored}",
    )
    .unwrap();
    let before = snap(&s);
    assert!(
        s.start_civil_war(&plan(), &Default::default(), &Default::default())
            .unwrap_err()
            .contains("inheritance")
    );
    assert_eq!(snap(&s), before);
}

#[test]
fn dynamic_expiry_is_hourly_even_when_modifier_was_added_between_midnights() {
    let mut s = fixture();
    let cat = dynamic_cat();
    s.clock.elapsed_hours = 7;
    dynamic_run(
        &mut s,
        &cat,
        "set_country_flag=enabled add_dynamic_modifier={modifier=authored days=1}",
    )
    .unwrap();
    s.clock.elapsed_hours = 30;
    s.tick_campaign(&cat, &Default::default());
    assert_eq!(
        s.research.countries["A"].modifier("research_speed_factor"),
        0.2
    );
    s.clock.elapsed_hours = 31;
    s.tick_campaign(&cat, &Default::default());
    assert_eq!(
        s.research.countries["A"].modifier("research_speed_factor"),
        0.
    );
    assert_eq!(
        s.military.countries["A"]
            .modifiers
            .value("army_attack_factor"),
        0.
    );
}

#[test]
fn independent_historical_dynamic_effect_replays_once_after_military_initialization() {
    let mut s = fixture();
    let cat = dynamic_cat();
    dynamic_run(&mut s, &cat, "set_country_flag=enabled").unwrap();
    let job = crate::research::DeferredEffect {
        source: "history/A.txt".into(),
        field: "add_dynamic_modifier".into(),
        reason: "Research-only consumer".into(),
        payload: parse_pdx("add_dynamic_modifier={modifier=authored days=2}").unwrap(),
    };
    s.research
        .countries
        .get_mut("A")
        .unwrap()
        .pending_effects
        .push(job);
    assert!(s.political_income("A", &cat).is_err());
    s.initialize_campaign_dynamic_history(&cat, &Default::default());
    assert!(s.research.countries["A"].pending_effects.is_empty());
    assert_eq!(
        s.research.countries["A"].modifier("research_speed_factor"),
        0.2
    );
    s.clock.elapsed_hours = 10;
    s.initialize_campaign_dynamic_history(&cat, &Default::default());
    assert_eq!(
        s.campaign.dynamic_modifiers["A"]["authored"].expires,
        Some(48)
    );
}

fn hook_catalogue(raw: &str) -> ResearchCatalogue {
    let mut cat = ResearchCatalogue::default();
    for e in parse_pdx(raw).unwrap() {
        cat.scripts
            .on_actions
            .entry(e.key.clone())
            .or_default()
            .push(crate::research_script::ScriptSource {
                kind: "on_action".into(),
                id: e.key.clone(),
                source: "authored civil-war hooks".into(),
                conditions: vec![],
                effects: vec![],
                raw: e.block().unwrap().to_vec(),
            });
    }
    cat
}
#[test]
fn typed_civil_war_calls_both_governments_and_before_after_annex_at_real_boundaries() {
    let mut s = fixture();
    s.research
        .country_mut("A", 0, &Default::default())
        .context
        .ideology = Some("neutrality".into());
    let cat = hook_catalogue(
        "on_government_change={effect={set_country_flag=government_called}} on_ruling_party_change_immediate={effect={if={limit={check_variable={old_ideology_token=token:neutrality}} set_country_flag=old_parent_ideology}}} on_civil_war_end_before_annexation={effect={set_country_flag=before_annex FROM={add_equipment_to_stockpile={type=rifle amount=7}}}} on_annex={effect={if={limit={FROM={tag=D01}} set_country_flag=annexed_from_identity}}} on_civil_war_end={effect={set_country_flag=civil_end}}",
    );
    s.start_civil_war(&plan(), &cat, &Default::default())
        .unwrap();
    for t in ["A", "D01"] {
        assert!(
            s.research
                .country(t)
                .unwrap()
                .flags
                .contains("government_called")
        );
        assert!(
            s.research
                .country(t)
                .unwrap()
                .flags
                .contains("old_parent_ideology")
        );
    }
    for p in [1, 2, 3] {
        s.world.set_province_controller(p, Some("A"));
    }
    s.settle_civil_war("D01", "A", &cat, &Default::default())
        .unwrap();
    assert_eq!(
        s.production.country("A").unwrap().model_stockpile["rifle"],
        108
    );
    for k in ["before_annex", "annexed_from_identity", "civil_end"] {
        assert!(s.research.country("A").unwrap().flags.contains(k));
    }
    assert!(s.campaign.queue.values().all(|a| a.blocked.is_none()));
}
#[test]
fn failed_pre_annex_callback_or_staged_overflow_cannot_reward_or_destroy_loser() {
    for overflow in [false, true] {
        let mut s = fixture();
        let base = ResearchCatalogue::default();
        s.start_civil_war(&plan(), &base, &Default::default())
            .unwrap();
        for p in [1, 2, 3] {
            s.world.set_province_controller(p, Some("A"));
        }
        if overflow {
            s.military.countries.get_mut("A").unwrap().manpower_pool = u64::MAX - 1;
            s.military.countries.get_mut("D01").unwrap().manpower_pool = 1;
        }
        let cat = hook_catalogue(if overflow {
            "on_civil_war_end_before_annexation={effect={add_political_power=9 add_manpower=1}} on_annex={effect={set_country_flag=post}}"
        } else {
            "on_civil_war_end_before_annexation={effect={add_political_power=9 divide_variable={var=zero value=0}}} on_annex={effect={set_country_flag=post}}"
        });
        let before = snap(&s);
        assert!(
            s.settle_civil_war("D01", "A", &cat, &Default::default())
                .is_err()
        );
        assert_eq!(before, snap(&s));
    }
}
#[test]
fn post_annex_callback_failure_is_saved_and_not_repeated() {
    let mut s = fixture();
    let cat = hook_catalogue(
        "on_annex={effect={add_political_power=17 divide_variable={var=zero value=0}}} on_civil_war_end={effect={set_country_flag=after}}",
    );
    s.start_civil_war(&plan(), &cat, &Default::default())
        .unwrap();
    for p in [1, 2, 3] {
        s.world.set_province_controller(p, Some("A"));
    }
    s.settle_civil_war("D01", "A", &cat, &Default::default())
        .unwrap();
    assert!(s.campaign.inactive.contains("D01"));
    let power = s.campaign.countries["A"].power;
    assert_eq!(
        s.campaign
            .queue
            .values()
            .filter(|a| a.blocked.is_some())
            .count(),
        1
    );
    let bytes = serde_json::to_vec(&s).unwrap();
    s = serde_json::from_slice(&bytes).unwrap();
    s.flush_simulation_hooks(&cat, &Default::default());
    assert_eq!(s.campaign.countries["A"].power, power);
    assert!(!s.research.country("A").unwrap().flags.contains("after"));
    s.campaign
        .validate(&s.world, s.clock.elapsed_hours)
        .unwrap();
}
