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

fn idea_timers(s: &GameSession) -> BTreeMap<String, BTreeMap<String, u64>> {
    s.research
        .countries
        .iter()
        .filter(|(_, r)| !r.timed_ideas.is_empty())
        .map(|(t, r)| (t.clone(), r.timed_ideas.clone()))
        .collect()
}
fn timed_catalogue() -> ResearchCatalogue {
    let mut cat = ResearchCatalogue::default();
    cat.scripts.ideas.insert(
        "temporary".into(),
        parse_pdx("modifier={army_attack_factor=0.5 industrial_capacity_factory=0.2 research_speed_factor=0.1 global_building_slots_factor=0.25} research_bonus={industry=0.15}").unwrap(),
    );
    cat.scripts.ideas.insert("replacement".into(), vec![]);
    cat
}
fn with_cat(s: &mut GameSession, script: &str, cat: &ResearchCatalogue) -> Result<(), String> {
    s.campaign_effects(
        &Context::country("A"),
        &parse_pdx(script).unwrap(),
        "authored/timers",
        cat,
        &Default::default(),
    )
}
#[test]
fn timed_spirit_expires_before_same_hour_rewards_and_removes_actual_consumers() {
    let mut s = fixture();
    let cat = timed_catalogue();
    let base = s.military.divisions[&1].derived.stats.soft_attack;
    with_cat(&mut s, "add_timed_idea={idea=temporary days=1}", &cat).unwrap();
    assert_eq!(idea_timers(&s)["A"]["temporary"], 24);
    assert!((s.military.divisions[&1].derived.stats.soft_attack - base * 1.5).abs() < 1e-9);
    for _ in 0..23 {
        s.advance_one_hour(
            &Default::default(),
            &Default::default(),
            &cat,
            &Default::default(),
        );
    }
    assert!(
        s.research
            .country("A")
            .unwrap()
            .context
            .ideas
            .contains("temporary")
    );
    s.advance_one_hour(
        &Default::default(),
        &Default::default(),
        &cat,
        &Default::default(),
    );
    assert!(idea_timers(&s).is_empty());
    let r = s.research.country("A").unwrap();
    assert!(!r.context.ideas.contains("temporary"));
    for key in [
        "army_attack_factor",
        "industrial_capacity_factory",
        "research_speed_factor",
        "global_building_slots_factor",
    ] {
        assert_eq!(r.modifier(key), 0.);
    }
    assert_eq!(r.source_category_bonuses.get("industry"), None);
    assert!((s.military.divisions[&1].derived.stats.soft_attack - base).abs() < 1e-9);
    assert!(
        !s.military.countries["A"]
            .modifiers
            .ideas
            .contains("temporary")
    );
    let before = snap(&s);
    s.tick_campaign(&cat, &Default::default());
    assert_eq!(before, snap(&s));

    with_cat(&mut s, "add_timed_idea={idea=temporary days=1}", &cat).unwrap();
    s.campaign
        .schedule(
            48,
            Context::country("A"),
            "authored/regrant".into(),
            Payload::Effects(parse_pdx("add_timed_idea={idea=temporary days=2}").unwrap()),
        )
        .unwrap();
    s.clock.elapsed_hours = 48;
    s.tick_campaign(&cat, &Default::default());
    assert_eq!(idea_timers(&s)["A"]["temporary"], 96);
    assert!((s.military.divisions[&1].derived.stats.soft_attack - base * 1.5).abs() < 1e-9);
}
#[test]
fn timed_spirit_duration_arithmetic_and_no_stale_expiry() {
    let cat = timed_catalogue();
    let mut s = fixture();
    with_cat(&mut s, "set_variable={var=duration value=2} add_timed_idea={idea=temporary years=1 months=2 days=duration}", &cat).unwrap();
    assert_eq!(idea_timers(&s)["A"]["temporary"], (365 + 60 + 2) * 24);
    with_cat(
        &mut s,
        "modify_timed_idea={idea=temporary years=-1 months=-2 days=1}",
        &cat,
    )
    .unwrap();
    assert_eq!(idea_timers(&s)["A"]["temporary"], 72);
    s.clock.elapsed_hours = 25;
    with_cat(&mut s, "add_timed_idea={idea=temporary days=5}", &cat).unwrap();
    assert_eq!(idea_timers(&s)["A"]["temporary"], 145);
    s.clock.elapsed_hours = 72;
    s.tick_campaign(&cat, &Default::default());
    assert!(
        s.research
            .country("A")
            .unwrap()
            .context
            .ideas
            .contains("temporary")
    );
    with_cat(&mut s, "modify_timed_idea={idea=temporary days=-100}", &cat).unwrap();
    assert!(
        !s.research
            .country("A")
            .unwrap()
            .context
            .ideas
            .contains("temporary")
    );
    assert!(idea_timers(&s).is_empty());
    assert!(with_cat(&mut s, "modify_timed_idea={idea=temporary days=1}", &cat).is_err());
}
#[test]
fn timed_spirit_permanent_grant_removal_and_swap_cancel_timer() {
    let cat = timed_catalogue();
    for action in [
        "add_ideas=temporary",
        "remove_ideas=temporary",
        "swap_ideas={remove_idea=temporary add_idea=replacement}",
    ] {
        let mut s = fixture();
        with_cat(&mut s, "add_timed_idea={idea=temporary days=1}", &cat).unwrap();
        with_cat(&mut s, action, &cat).unwrap();
        assert!(idea_timers(&s).is_empty());
        let expected = s.research.country("A").unwrap().context.ideas.clone();
        s.clock.elapsed_hours = 24;
        s.tick_campaign(&cat, &Default::default());
        assert_eq!(s.research.country("A").unwrap().context.ideas, expected);
    }
}
#[test]
fn timed_spirit_invalid_duration_and_failed_rewards_are_atomic() {
    let mut s = fixture();
    let cat = timed_catalogue();
    for script in [
        "add_timed_idea={idea=temporary days=0}",
        "add_timed_idea={idea=temporary days=-1}",
        "add_timed_idea={idea=temporary days=0.5}",
        "add_timed_idea={idea=temporary days=NaN}",
        "add_timed_idea={idea=temporary days=missing}",
        "add_timed_idea={idea=temporary years=9223372036854775807}",
        "add_timed_idea={idea=temporary}",
        "add_timed_idea={idea=temporary days=1 days=2}",
        "add_timed_idea={idea=temporary days={bad=1} years=1}",
        "add_timed_idea={idea=missing days=1}",
        "modify_timed_idea={idea=temporary days=1}",
        "add_timed_idea={idea=temporary days=1} divide_variable={var=paranoia value=0}",
        "add_timed_idea={idea=temporary days=1} unknown_effect=yes",
    ] {
        let before = snap(&s);
        assert!(
            with_cat(&mut s, &format!("add_manpower=1 {script}"), &cat).is_err(),
            "{script}"
        );
        assert_eq!(before, snap(&s), "{script}");
    }
    s.clock.elapsed_hours = u64::MAX - 10;
    let before = snap(&s);
    assert!(with_cat(&mut s, "add_timed_idea={idea=temporary days=1}", &cat).is_err());
    assert_eq!(before, snap(&s));
}
#[test]
fn timed_spirit_expirations_are_not_limited_by_event_budget_or_other_countries() {
    let mut s = fixture();
    let mut cat = timed_catalogue();
    let mut script = String::new();
    for i in 0..70 {
        let id = format!("timed{i}");
        cat.scripts.ideas.insert(id.clone(), vec![]);
        script.push_str(&format!("add_timed_idea={{idea={id} days=1}} "));
    }
    with_cat(&mut s, &script, &cat).unwrap();
    s.campaign_effects(
        &Context::country("B"),
        &parse_pdx("add_timed_idea={idea=temporary days=2}").unwrap(),
        "fixture",
        &cat,
        &Default::default(),
    )
    .unwrap();
    s.clock.elapsed_hours = 24;
    s.tick_campaign(&cat, &Default::default());
    assert!(!idea_timers(&s).contains_key("A"));
    assert_eq!(idea_timers(&s)["B"]["temporary"], 48);
    assert!(s.research.country("A").unwrap().context.ideas.is_empty());
}
#[test]
fn timed_spirit_save_load_deadlines_validation_and_legacy_defaults() {
    let mut s = fixture();
    let cat = timed_catalogue();
    with_cat(&mut s, "add_timed_idea={idea=temporary days=2}", &cat).unwrap();
    s.clock.elapsed_hours = 17;
    let path = std::env::temp_dir().join(format!("hoi-timed-{}.json", std::process::id()));
    s.save_game(&path, &cat).unwrap();
    let mut restored = fixture();
    restored.restore_game(&path, &cat).unwrap();
    for g in [&mut s, &mut restored] {
        for h in 18..=49 {
            g.clock.elapsed_hours = h;
            g.tick_campaign(&cat, &Default::default());
        }
    }
    assert_eq!(snap(&s), snap(&restored));
    assert!(idea_timers(&s).is_empty());
    let mut saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    saved["session"]["research"]["countries"]["A"]["timed_ideas"]["missing"] = 99.into();
    std::fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
    let before = snap(&restored);
    assert!(restored.restore_game(&path, &cat).is_err());
    assert_eq!(snap(&restored), before);
    let mut legacy = serde_json::to_value(crate::research::CountryResearch::new(
        0,
        &Default::default(),
    ))
    .unwrap();
    legacy.as_object_mut().unwrap().remove("timed_ideas");
    assert!(
        serde_json::from_value::<crate::research::CountryResearch>(legacy)
            .unwrap()
            .timed_ideas
            .is_empty()
    );
    std::fs::remove_file(path).unwrap();
}
#[test]
fn timed_spirit_split_keeps_deadline_and_settlement_cleans_losing_timers() {
    let cat = timed_catalogue();
    for winner in ["A", "D01"] {
        let mut s = fixture();
        with_cat(&mut s, "add_timed_idea={idea=temporary days=3}", &cat).unwrap();
        s.start_civil_war(&plan(), &Default::default()).unwrap();
        assert_eq!(idea_timers(&s)["A"]["temporary"], 72);
        assert_eq!(idea_timers(&s)["D01"]["temporary"], 72);
        for province in s.world.provinces.keys().copied().collect::<Vec<_>>() {
            s.world.set_province_controller(province, Some(winner));
        }
        s.settle_civil_war("D01", winner).unwrap();
        assert_eq!(idea_timers(&s).len(), 1);
        assert_eq!(idea_timers(&s)[winner]["temporary"], 72);
        s.clock.elapsed_hours = 72;
        s.tick_campaign(&cat, &Default::default());
        assert!(idea_timers(&s).is_empty());
        assert!(
            !s.research
                .country(winner)
                .unwrap()
                .context
                .ideas
                .contains("temporary")
        );
    }
}
#[test]
fn timed_spirit_partition_free_deadline_properties_and_capacity() {
    let cat = timed_catalogue();
    // Exact arithmetic property: any extension followed by its inverse restores
    // the deadline, even when the game clock moves between those operations.
    let mut cases = 0;
    for now in [0, 1, 23, 24, 719, 8760, 1_000_000] {
        for days in 2..=19 {
            let mut s = fixture();
            s.clock.elapsed_hours = now;
            with_cat(
                &mut s,
                &format!("add_timed_idea={{idea=temporary days={days}}}"),
                &cat,
            )
            .unwrap();
            let deadline = now + days * 24;
            for delta in -1..=3 {
                s.clock.elapsed_hours = now + 1;
                with_cat(
                    &mut s,
                    &format!("modify_timed_idea={{idea=temporary days={delta}}}"),
                    &cat,
                )
                .unwrap();
                let expected = (i128::from(deadline) + i128::from(delta * 24)) as u64;
                assert_eq!(idea_timers(&s)["A"]["temporary"], expected);
                with_cat(
                    &mut s,
                    &format!("modify_timed_idea={{idea=temporary days={}}}", -delta),
                    &cat,
                )
                .unwrap();
                assert_eq!(idea_timers(&s)["A"]["temporary"], deadline);
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 630);
    let mut s = fixture();
    let mut cat = cat;
    for i in 0..4096 {
        let id = format!("timer{i}");
        s.research
            .countries
            .get_mut("A")
            .unwrap()
            .timed_ideas
            .insert(id.clone(), 240);
        s.research
            .countries
            .get_mut("A")
            .unwrap()
            .context
            .ideas
            .insert(id);
    }
    s.research
        .countries
        .get_mut("A")
        .unwrap()
        .ideas_authoritative = true;
    cat.scripts.ideas.insert("timer0".into(), vec![]);
    // Updating an existing timer at capacity succeeds without allocating a job.
    with_cat(&mut s, "add_timed_idea={idea=timer0 days=2}", &cat).unwrap();
    assert_eq!(idea_timers(&s)["A"]["timer0"], 48);
    let before = snap(&s);
    assert!(
        with_cat(
            &mut s,
            "add_manpower=1 add_timed_idea={idea=temporary days=1}",
            &cat
        )
        .is_err()
    );
    assert_eq!(before, snap(&s));
}
#[test]
fn timed_spirit_legacy_research_rewards_cannot_leave_stale_timer() {
    let cat = timed_catalogue();
    for action in [
        "add_ideas=temporary",
        "remove_ideas=temporary",
        "swap_ideas={remove_idea=temporary add_idea=replacement}",
    ] {
        let mut s = fixture();
        with_cat(&mut s, "add_timed_idea={idea=temporary days=1}", &cat).unwrap();
        crate::research_script::apply_effects(
            s.research.countries.get_mut("A").unwrap(),
            &parse_pdx(action).unwrap(),
            &cat,
            &Default::default(),
            s.clock.date,
            "authored/research-reward",
        );
        assert!(idea_timers(&s).is_empty());
        s.campaign
            .validate(&s.world, &s.research, s.clock.elapsed_hours)
            .unwrap();
        s.clock.elapsed_hours = 24;
        s.tick_campaign(&cat, &Default::default());
        assert_eq!(
            s.research
                .country("A")
                .unwrap()
                .context
                .ideas
                .contains("temporary"),
            action.starts_with("add_ideas")
        );
    }
    // Inheritance must be checked before the split mutates even one province.
    let mut s = fixture();
    let r = s.research.countries.get_mut("A").unwrap();
    r.ideas_authoritative = true;
    for i in 0..2049 {
        let id = format!("timer{i}");
        r.context.ideas.insert(id.clone());
        r.timed_ideas.insert(id, 240);
    }
    let before = snap(&s);
    assert!(
        s.start_civil_war(&plan(), &Default::default())
            .unwrap_err()
            .contains("capacity")
    );
    assert_eq!(before, snap(&s));
}
