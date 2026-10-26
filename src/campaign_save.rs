//! One atomic campaign snapshot: resources, clock, research and military orders
//! are restored together. Static game/map data must match before any mutation.
use crate::{research::ResearchCatalogue, session::GameSession};
use serde::{Deserialize, Serialize};
use std::{
    hash::{Hash, Hasher},
    io::{BufReader, BufWriter, Write},
    path::Path,
};
#[derive(Serialize)]
struct Save<'a> {
    schema: u32,
    map: u64,
    research: &'a str,
    military: u64,
    session: &'a GameSession,
}
#[derive(Deserialize)]
struct Load {
    schema: u32,
    map: u64,
    research: String,
    military: u64,
    session: GameSession,
}
fn military_fingerprint(w: &crate::warfare::Warfare) -> Result<u64, String> {
    let db = &w.catalogue;
    let mut abilities = w.abilities.definitions.clone();
    for a in abilities.values_mut() {
        a.source.clear();
    }
    let mut units = db.units.clone();
    for u in units.values_mut() {
        u.source.clear();
    }
    let mut equipment = db.equipment.clone();
    for e in equipment.values_mut() {
        e.source.clear();
    }
    let mut rules = db.rules.clone();
    rules.provenance.clear();
    rules.diagnostics.clear();
    let bytes = serde_json::to_vec(&(
        db.source_version.as_ref(),
        db.source_checksum.as_ref(),
        rules,
        units,
        equipment,
        &db.technologies,
        &db.terrains,
        &db.weather,
        &db.tactics,
        &db.literal_defines,
        &db.literal_arrays,
        &w.command.rules,
        abilities,
        &w.abilities.hq_rules,
    ))
    .map_err(|e| e.to_string())?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    Ok(h.finish())
}
impl GameSession {
    pub fn save_game(&mut self, path: &Path, cat: &ResearchCatalogue) -> Result<(), String> {
        self.military.clean_references();
        let data = Save {
            schema: 1,
            map: self.world.geography.snapshot_fingerprint(),
            research: &cat.fingerprint,
            military: military_fingerprint(&self.military)?,
            session: self,
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let temp = path.with_extension("json.tmp");
        let mut out = BufWriter::new(std::fs::File::create(&temp).map_err(|e| e.to_string())?);
        serde_json::to_writer(&mut out, &data).map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
        out.get_ref().sync_all().map_err(|e| e.to_string())?;
        drop(out);
        // Unix atomically replaces; Windows needs the existing file moved first.
        if let Err(first) = std::fs::rename(&temp, path) {
            if !path.is_file() {
                return Err(first.to_string());
            }
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos();
            let backup = path.with_extension(format!("{nonce}.previous"));
            std::fs::rename(path, &backup).map_err(|e| e.to_string())?;
            if let Err(e) = std::fs::rename(&temp, path) {
                let _ = std::fs::rename(&backup, path);
                return Err(e.to_string());
            }
            let _ = std::fs::remove_file(backup);
        }
        Ok(())
    }
    pub fn restore_game(&mut self, path: &Path, cat: &ResearchCatalogue) -> Result<(), String> {
        if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > 128 * 1024 * 1024 {
            return Err("Сохранение превышает 128 MiB".into());
        }
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut data: Load =
            serde_json::from_reader(BufReader::new(file)).map_err(|e| e.to_string())?;
        if data.schema != 1
            || data.map != self.world.geography.snapshot_fingerprint()
            || data.research != cat.fingerprint
            || data.military != military_fingerprint(&self.military)?
        {
            return Err(
                "Сохранение относится к другой карте, версии или набору игровых определений".into(),
            );
        }
        let s = &mut data.session;
        if s.clock.hour >= 24
            || s.clock.date.month == 0
            || s.clock.date.month > 12
            || s.clock.date.day == 0
            || s.clock.date.day > 31
        {
            return Err("Повреждён игровой календарь".into());
        }
        if s.world.provinces.keys().ne(self.world.provinces.keys())
            || s.world.states.keys().ne(self.world.states.keys())
        {
            return Err("Несовместимый набор провинций/штатов".into());
        }
        s.military.validate_references(&s.world)?;
        for (id, state) in &s.world.states {
            if state.province_ids != self.world.states[id].province_ids {
                return Err("Изменена география штатов".into());
            }
        }
        // All validation is complete; move the immutable shared data only now.
        s.world.geography = std::mem::take(&mut self.world.geography);
        s.military.catalogue = self.military.catalogue.clone();
        s.military.fronts.rebuild_after_restore(&s.world);
        let mut plans = std::mem::take(&mut s.military.plans);
        plans.rebuild_after_restore(&s.military, &s.world);
        s.military.plans = plans;
        s.clock.paused = true;
        *self = data.session;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        command::{Commander, Rank},
        military_model::{
            BattalionSlot, CountryArmy, EquipmentDefinition,
            tests::{division, template},
        },
        operations::{Activation, OrderType},
        production::ProductionState,
    };
    use std::{collections::BTreeMap, sync::Arc};
    fn file(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "hoi4-campaign-{label}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    fn configured() -> GameSession {
        let (world, mut w, _, _) = crate::operations::tests::configured(OrderType::Offensive);
        w.next_division_id = 13;
        for tag in ["A", "B"] {
            w.countries.insert(
                tag.into(),
                CountryArmy {
                    templates: BTreeMap::from([("test".into(), template())]),
                    manpower_pool: 10000,
                    army_xp: 100.0,
                    ..Default::default()
                },
            );
        }
        let mut s = GameSession::new(world);
        s.military = w;
        s.player_country = Some("A".into());
        s
    }
    fn tick(s: &mut GameSession) {
        s.clock.elapsed_hours += 1;
        s.clock.hour = (s.clock.elapsed_hours % 24) as u8;
        s.military.tick_hour(
            &mut s.world,
            s.clock.date,
            s.clock.hour,
            s.clock.elapsed_hours,
            Some("A"),
        );
    }
    #[test]
    fn full_chain_refit_front_battle_capture_and_save_replay() {
        let mut s = configured();
        let cat = ResearchCatalogue::default();
        let db = Arc::make_mut(&mut s.military.catalogue);
        let u = db.units.get_mut("infantry").unwrap();
        u.need.insert("rifles".into(), 100.0);
        u.stats.insert("soft_attack".into(), 0.0);
        let mut artillery = u.clone();
        artillery.id = "artillery".into();
        artillery.need = BTreeMap::from([("guns".into(), 12.0)]);
        artillery.stats.insert("combat_width".into(), 3.0);
        artillery.stats.insert("manpower".into(), 500.0);
        db.units.insert("artillery".into(), artillery);
        for (id, archetype, attack) in [("rifle", "rifles", 10.0), ("gun", "guns", 30.0)] {
            db.equipment.insert(
                id.into(),
                EquipmentDefinition {
                    id: id.into(),
                    archetype: archetype.into(),
                    year: 1936,
                    active: true,
                    buildable: true,
                    stats: BTreeMap::from([("soft_attack".into(), attack)]),
                    cost: 1.0,
                    modular: false,
                    source: "authored integration fixture".into(),
                },
            );
        }
        s.production.country_mut("A").model_stockpile =
            BTreeMap::from([("rifle".into(), 1200), ("gun".into(), 144)]);
        let mut t = template();
        t.line.push(BattalionSlot {
            unit: "artillery".into(),
            column: 1,
            row: 0,
        });
        s.military.save_template("A", Some("test"), t).unwrap();
        assert_eq!(s.military.countries["A"].army_xp, 95.0);
        assert_eq!(
            s.production.country("A").unwrap().model_stockpile["gun"],
            144
        );
        s.military.replenish(&mut s.production);
        assert_eq!(s.production.country("A").unwrap().model_stockpile["gun"], 0);
        assert_eq!(s.military.divisions[&1].derived.stats.width, 5.0);
        assert_eq!(s.military.divisions[&1].equipment["gun"], 12);
        let a = *s.military.command.armies.keys().next().unwrap();
        s.military.command.commanders.insert(
            "g".into(),
            Commander {
                id: "g".into(),
                country: "A".into(),
                name: "QA General".into(),
                rank: Rank::General,
                skill: 1,
                attack: 1,
                defense: 1,
                planning: 1,
                logistics: 1,
                experience: 0.0,
                traits: Default::default(),
                source: "authored fixture".into(),
            },
        );
        s.military.command.set_general("A", a, Some("g")).unwrap();
        for _ in 0..24 {
            tick(&mut s);
        }
        assert!(s.military.divisions.values().any(|d| d.planning > 0.0));
        let mut enemy = division(13, "B", 9);
        enemy.organization = 0.01;
        enemy.derived.stats.soft_attack = 0.0;
        enemy.derived.stats.defense = 0.0;
        s.military.divisions.insert(13, enemy);
        s.military.next_division_id = 14;
        let plan = *s.military.plans.plans.keys().next().unwrap();
        s.military.plans.activate("A", plan, true).unwrap();
        for _ in 0..24 {
            tick(&mut s);
            if !s.military.battles.is_empty() {
                break;
            }
        }
        assert!(!s.military.battles.is_empty());
        let path = file("replay");
        s.save_game(&path, &cat).unwrap();
        for _ in 0..96 {
            tick(&mut s);
        }
        let expected = serde_json::to_value(&s.military).unwrap();
        let control = s.world.provinces.clone();
        assert!(
            s.military
                .battles
                .values()
                .any(|b| b.outcome == Some(crate::land_combat::BattleOutcome::AttackerWon))
        );
        assert_eq!(
            s.military.plans.plans[&plan].activation,
            Activation::Completed
        );
        assert!(s.world.control_revision > 0);
        s.restore_game(&path, &cat).unwrap();
        assert!(s.clock.paused);
        for _ in 0..96 {
            tick(&mut s);
        }
        assert_eq!(s.world.provinces, control);
        assert_eq!(serde_json::to_value(&s.military).unwrap(), expected);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn incompatible_or_corrupt_save_never_partially_replaces_campaign() {
        let mut s = configured();
        let cat = ResearchCatalogue::default();
        let path = file("reject");
        s.save_game(&path, &cat).unwrap();
        let mut changed = cat.clone();
        changed.fingerprint = "different registry".into();
        assert!(s.restore_game(&path, &changed).is_err());
        assert_eq!(s.military.divisions.len(), 12);
        let mut json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        json["session"]["military"]["divisions"]["1"]["province"] = 999.into();
        std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
        assert!(s.restore_game(&path, &cat).is_err());
        assert_eq!(s.military.divisions[&1].province, 1);
        assert_eq!(s.world.geography.width, 8);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn encirclement_has_no_impossible_retreat_or_teleport() {
        let mut world = crate::warfare::tests::world();
        let mut w = crate::warfare::Warfare::new(crate::military_model::tests::catalogue());
        Arc::make_mut(&mut w.catalogue).rules.pixels_to_km = Some(1.0);
        w.ai_enabled = false;
        let mut a = division(1, "A", 1);
        a.derived.stats.soft_attack = 10000.0;
        let mut b = division(2, "B", 2);
        b.organization = 0.0001;
        w.divisions.insert(1, a);
        w.divisions.insert(2, b);
        world.set_province_controller(3, Some("A"));
        w.declare_war("A", "B", &world).unwrap();
        w.attack("A", &[1], 2, &world).unwrap();
        for h in 1..=30 {
            w.tick_hour(&mut world, crate::game::GameDate::START, 12, h, Some("A"));
        }
        assert!(w.divisions[&2].destroyed);
        assert_eq!(w.divisions[&2].manpower, 0);
        assert_eq!(world.controller_of_province(2), Some("A"));
    }
    #[test]
    fn snapshot_restores_economy_research_and_resources_together() {
        let mut s = configured();
        let cat = ResearchCatalogue::default();
        let path = file("economy");
        s.production
            .country_mut("A")
            .model_stockpile
            .insert("test equipment".into(), 47);
        s.world
            .states
            .get_mut(&1)
            .unwrap()
            .economy
            .military_factories = 6;
        s.research
            .country_mut("A", 2, &Default::default())
            .researched
            .insert("test tech".into());
        s.save_game(&path, &cat).unwrap();
        s.production = ProductionState::default();
        s.world
            .states
            .get_mut(&1)
            .unwrap()
            .economy
            .military_factories = 0;
        s.research = Default::default();
        s.restore_game(&path, &cat).unwrap();
        assert_eq!(s.world.states[&1].economy.military_factories, 6);
        assert_eq!(
            s.production.country("A").unwrap().model_stockpile["test equipment"],
            47
        );
        assert!(
            s.research
                .country("A")
                .unwrap()
                .researched
                .contains("test tech")
        );
        std::fs::remove_file(path).unwrap();
    }
}
