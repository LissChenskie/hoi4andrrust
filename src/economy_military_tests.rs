//! Authored fixtures test the real campaign scheduler, not asserted historical numbers.
use super::*;
use crate::game::GameDate;
use std::{fs, path::PathBuf};
struct Sources(PathBuf);
impl Sources {
    fn new() -> Self {
        Self(std::env::temp_dir().join(
            format!("hoi4-economy-chain-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()),
        ))
    }
    fn put(&self, name: &str, source: &str) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
}
impl Drop for Sources {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn default_law_scripted_stock_research_production_refit_and_combat_share_one_campaign() {
    let source = Sources::new();
    source.put("common/units/equipment/rifles.txt", "equipments={rifles={is_archetype=yes build_cost_ic=1 reliability=1 maximum_speed=4} rifle_1={archetype=rifles active=yes year=1930 soft_attack=50} rifle_2={archetype=rifles active=no year=1936 soft_attack=100}}");
    source.put("common/units/inf.txt", "sub_units={infantry={active=yes group=infantry max_strength=25 max_organisation=60 manpower=1000 maximum_speed=4 combat_width=2 need={rifles=100}}}");
    source.put(
        "common/defines/fixture.lua",
        "NDefines.NMilitary.MAP_SCALE_PIXEL_TO_KM = 1\n",
    );
    source.put("common/ideas/laws.txt", "ideas={mobilization={law=yes volunteer={default=yes modifier={conscription=0.1 industrial_capacity_factory=0.25}} extended={modifier={conscription=0.2}}}}");
    source.put("common/scripted_effects/stock.txt", "starting_stock={add_equipment_to_stockpile={type=$kind$ amount=$count$}} ready={set_country_flag=ready}");
    source.put("common/technologies/weapons.txt", "technologies={modern={research_cost=0.001 start_year=1936 enable_equipments={rifle_2} infantry={soft_attack=0.2} modifier={industrial_capacity_factory=0.5}}}");
    source.put(
        "history/countries/AAA - A.txt",
        "capital=1 set_research_slots=1 oob=A_1936 starting_stock={kind=rifle_1 count=20}",
    );
    source.put(
        "history/states/1.txt",
        "state={id=1 manpower=100000 history={owner=AAA add_core_of=AAA}}",
    );
    source.put("history/units/A_1936.txt", "division_template={name=Inf regiments={infantry={x=0 y=0}}} ready=yes units={if={limit={has_country_flag=ready} division={name=Existing division_template=Inf location=1 start_experience_factor=0.3}}} instant_effect={add_equipment_production={equipment={type=rifle_1} requested_factories=1}} ");
    let data = hoi4_mobile::research_import::ResearchDataset::load(&source.0, false).unwrap();
    let cached = hoi4_mobile::research_import::ResearchDataset::load(&source.0, true).unwrap();
    assert!(cached.from_cache);
    assert!(cached.registry.scripts.laws_indexed);
    let mut world = crate::warfare::tests::world();
    world.set_state_owner(1, Some("AAA"));
    world.set_state_owner(2, Some("BBB"));
    world.states.get_mut(&1).unwrap().economy.military_factories = 100;
    let mut s = GameSession::new(world);
    s.player_country = Some("AAA".into());
    s.initialize_research_dataset(&data, &source.0);
    s.initialize_military(&source.0);
    s.military.ai_enabled = false;
    assert_eq!(
        s.military.countries["AAA"].history_imported, 1,
        "{:?}",
        s.military.diagnostics
    );
    assert_eq!(s.military.countries["AAA"].core_population, 100000);
    assert_eq!(s.military.countries["AAA"].manpower_pool, 9000);
    assert_eq!(
        s.production.country("AAA").unwrap().model_stockpile["rifle_1"],
        20
    );
    let id = *s.military.divisions.keys().next().unwrap();
    assert_eq!(s.military.divisions[&id].equipment["rifle_1"], 100);
    assert_eq!(s.military.divisions[&id].derived.stats.soft_attack, 50.);
    assert!(
        s.military
            .add_production(
                "AAA",
                "rifle_2",
                &mut s.production,
                &ProductionRules::default()
            )
            .is_err()
    );
    s.research
        .country_mut("AAA", 1, &data.rules)
        .start(0, "modern", &data.registry, &data.rules, GameDate::START)
        .unwrap();
    let advance = |s: &mut GameSession, hours| {
        for _ in 0..hours {
            s.advance_one_hour(
                &BuildingRules::default(),
                &ProductionRules::default(),
                &data.registry,
                &data.rules,
            );
        }
    };
    advance(&mut s, 24);
    assert!(
        s.research
            .country("AAA")
            .unwrap()
            .researched
            .contains("modern")
    );
    // Unit technology improves existing gear, but unlocking a new model creates none.
    assert_eq!(s.military.divisions[&id].derived.stats.soft_attack, 60.);
    assert_eq!(
        s.military.divisions[&id]
            .equipment
            .get("rifle_2")
            .copied()
            .unwrap_or(0),
        0
    );
    assert!(
        (ProductionRules::default()
            .for_research(s.research.country("AAA"))
            .factory_output_bonus
            - 0.75)
            .abs()
            < 1e-9
    );
    let line = s
        .military
        .add_production(
            "AAA",
            "rifle_2",
            &mut s.production,
            &ProductionRules::default(),
        )
        .unwrap();
    s.production
        .country_mut("AAA")
        .change_requested(line, 99, &ProductionRules::default());
    advance(&mut s, 72);
    let production = s.production.country("AAA").unwrap();
    let made = production
        .lines
        .iter()
        .find(|l| l.id == line)
        .unwrap()
        .total_produced;
    let carried = u64::from(s.military.divisions[&id].equipment["rifle_2"]);
    let stored = production
        .model_stockpile
        .get("rifle_2")
        .copied()
        .unwrap_or(0);
    assert_eq!(made, carried + stored);
    assert_eq!(carried, 100);
    assert_eq!(s.military.countries["AAA"].manpower_pool, 9000);
    assert_eq!(s.military.divisions[&id].derived.stats.soft_attack, 120.);
    assert!(production.model_stockpile["rifle_1"] >= 120); // old equipment returned, not destroyed
    // Replacing the default law affects the single recruitable ledger exactly once.
    hoi4_mobile::research_script::apply_effects(
        s.research.country_mut("AAA", 1, &data.rules),
        &crate::pdx::parse_pdx("add_ideas=extended").unwrap(),
        &data.registry,
        &data.rules,
        s.clock.date,
        "authored law change",
    );
    s.military.refresh_modifiers(&s.research);
    assert_eq!(s.military.countries["AAA"].mobilization_target, Some(20000));
    assert_eq!(s.military.countries["AAA"].manpower_pool, 9000);
    s.military.tick_mobilization_day();
    assert_eq!(s.military.countries["AAA"].manpower_pool, 10000);
    s.military.refresh_modifiers(&s.research);
    assert_eq!(s.military.countries["AAA"].manpower_pool, 10000);
    let b = crate::military_model::tests::division(999, "BBB", 2);
    s.military.divisions.insert(999, b);
    s.military.conditions.entry(2).or_default().night_override = Some(false);
    s.military.declare_war("AAA", "BBB", &s.world).unwrap();
    let battle = s.military.attack("AAA", &[id], 2, &s.world).unwrap();
    s.military.battles.get_mut(&battle).unwrap().trace_enabled = true;
    advance(&mut s, 1);
    let trace = s.military.battles[&battle].history.back().unwrap();
    let shot = trace.attacks.iter().find(|a| a.from == id).unwrap();
    assert!(
        (shot.effective_attack - 150.).abs() < 1e-9,
        "actual {}",
        shot.effective_attack
    );
}

#[test]
fn extraction_research_removes_a_real_factory_resource_shortage_on_the_next_daily_settlement() {
    use crate::{military_model::EquipmentDefinition, production::Equipment, research::Technology};
    use std::collections::BTreeMap;
    let mut world = crate::warfare::tests::world();
    world.states.get_mut(&1).unwrap().economy.military_factories = 1;
    world
        .resources_by_state
        .insert(1, BTreeMap::from([("steel".into(), 1)]));
    let mut s = GameSession::new(world);
    s.player_country = Some("A".into());
    s.military.ai_enabled = false;
    let c = std::sync::Arc::make_mut(&mut s.military.catalogue);
    c.equipment.insert(
        "gun".into(),
        EquipmentDefinition {
            id: "gun".into(),
            archetype: "artillery".into(),
            year: 1936,
            active: true,
            buildable: true,
            modular: false,
            source: "authored".into(),
            stats: Default::default(),
            cost: 1.,
        },
    );
    c.equipment_resources
        .insert("gun".into(), BTreeMap::from([("steel".into(), 2)]));
    let production = ProductionRules {
        starting_efficiency: 1.,
        efficiency_cap: 1.,
        resource_lack_penalty: 1.,
        ..Default::default()
    };
    s.production
        .country_mut("A")
        .create_model("gun", Equipment::FieldEquipment, 1, &production);
    let mut cat = ResearchCatalogue::default();
    let mut technology = Technology::sample("extraction", 1936, 0.001);
    technology
        .modifiers
        .insert("resource_gain_efficiency".into(), 1.);
    cat.insert(technology).unwrap();
    let rules = ResearchRules::default();
    s.research.country_mut("A", 1, &rules);
    let advance = |s: &mut GameSession| {
        for _ in 0..24 {
            s.advance_one_hour(&BuildingRules::default(), &production, &cat, &rules);
        }
    };
    advance(&mut s);
    assert_eq!(s.resources.last["A"].extracted["steel"], 1);
    assert_eq!(
        s.production.country("A").unwrap().lines[0].last_day_units,
        0
    );
    s.research
        .country_mut("A", 1, &rules)
        .start(0, "extraction", &cat, &rules, s.clock.date)
        .unwrap();
    advance(&mut s);
    assert_eq!(s.resources.last["A"].extracted["steel"], 2);
    assert!(s.production.country("A").unwrap().model_stockpile["gun"] >= 4);
}
