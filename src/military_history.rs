//! Starting armies and explicit campaign inputs. No generated historical armies.
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::game::GameDate;
use crate::land_combat::{TacticContext, evaluate_conditions};
use crate::military_data::{atom, block, layered_files, number, parse_oob, stats};
use crate::military_model::{CountryArmy, StatBlock};
use crate::production::ProductionState;
use crate::research::ResearchWorld;
use crate::research_data::{Entry, parse_pdx};
use crate::warfare::Warfare;
use crate::world::World;

fn date(key: &str) -> Option<GameDate> {
    let p: Vec<_> = key.split('.').collect();
    if p.len() != 3 {
        return None;
    }
    Some(GameDate {
        year: p[0].parse().ok()?,
        month: p[1].parse().ok()?,
        day: p[2].parse().ok()?,
    })
}

fn effects(
    entries: &[Entry],
    tag: &str,
    warfare: &mut Warfare,
    production: &mut ProductionState,
    oobs: &mut Vec<String>,
    ideas: &mut BTreeSet<String>,
    today: GameDate,
) {
    let mut previous_if = None;
    let mut dated = Vec::new();
    for entry in entries {
        if let Some(at) = date(&entry.key) {
            if at <= today {
                dated.push((at, entry));
            }
            continue;
        }
        let army = warfare.countries.get_mut(tag).unwrap();
        match entry.key.as_str() {
            "oob" | "load_oob" => {
                if let Some(id) = entry.atom() {
                    oobs.push(id.into());
                }
            }
            "set_country_flag" => {
                if let Some(flag) = entry.atom() {
                    army.modifiers.flags.insert(flag.into());
                }
            }
            "add_manpower" => {
                if let Some(amount) = entry.atom().and_then(|v| v.parse::<i64>().ok()) {
                    army.manpower_pool = army.manpower_pool.saturating_add_signed(amount);
                }
            }
            "set_army_experience" | "army_experience" => {
                if let Some(v) = entry
                    .atom()
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite())
                {
                    army.army_xp = v.max(0.0);
                }
            }
            "add_army_experience" => {
                if let Some(v) = entry
                    .atom()
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite())
                {
                    army.army_xp = (army.army_xp + v).max(0.0);
                }
            }
            "add_ideas" => {
                if let Some(id) = entry.atom() {
                    ideas.insert(id.into());
                }
                for id in entry.block().unwrap_or(&[]) {
                    ideas.insert(id.key.clone());
                }
            }
            "remove_ideas" => {
                if let Some(id) = entry.atom() {
                    ideas.remove(id);
                }
                for id in entry.block().unwrap_or(&[]) {
                    ideas.remove(&id.key);
                }
            }
            "add_equipment_to_stockpile" => {
                if let Some(b) = entry.block() {
                    if let Some(model) = atom(b, "type") {
                        let amount = number(b, "amount", 0.0);
                        if warfare.catalogue.equipment.contains_key(model) && amount >= 0.0 {
                            let n = production
                                .country_mut(tag)
                                .model_stockpile
                                .entry(model.into())
                                .or_default();
                            *n = n.saturating_add(amount as u64);
                        } else {
                            warfare
                                .diagnostics
                                .push(format!("{tag}: unresolved stockpile model {model}"));
                        }
                    }
                }
            }
            "if" | "else_if" | "else" => {
                if let Some(b) = entry.block() {
                    if entry.key == "else_if" && previous_if != Some(false) {
                        continue;
                    }
                    let ctx = TacticContext {
                        country: tag,
                        attacker: false,
                        phase: None,
                        modifiers: &army.modifiers,
                        numbers: BTreeMap::new(),
                    };
                    let allowed = if entry.key == "else" {
                        previous_if.map(|v| !v)
                    } else {
                        evaluate_conditions(block(b, "limit").unwrap_or(&[]), &ctx)
                    };
                    if allowed == Some(true) {
                        let body: Vec<_> = b.iter().filter(|e| e.key != "limit").cloned().collect();
                        effects(&body, tag, warfare, production, oobs, ideas, today);
                    } else if allowed.is_none() {
                        warfare
                            .diagnostics
                            .push(format!("{tag}: unsupported conditional history block"));
                    }
                    previous_if = if previous_if == Some(true) && entry.key != "if" {
                        Some(true)
                    } else {
                        allowed
                    };
                }
            }
            "create_equipment_variant" => warfare.diagnostics.push(format!(
                "{tag}: custom equipment variant needs design import; no substitute is fabricated"
            )),
            _ => {}
        }
    }
    dated.sort_by_key(|(at, _)| *at);
    for (_, entry) in dated {
        effects(
            entry.block().unwrap_or(&[]),
            tag,
            warfare,
            production,
            oobs,
            ideas,
            today,
        );
    }
}

pub fn initialize(
    root: &Path,
    world: &World,
    research: &ResearchWorld,
    production: &mut ProductionState,
    today: GameDate,
) -> Warfare {
    let mut warfare = Warfare::new(crate::military_data::load(root));
    let mut layers = vec![root.to_path_buf()];
    let profile = fs::read_to_string(root.join("military_profile.txt"))
        .ok()
        .and_then(|s| parse_pdx(&s).ok())
        .unwrap_or_default();
    if let Some(entries) = block(&profile, "data_layers") {
        layers.extend(entries.iter().filter(|e| e.value.is_none()).map(|e| {
            let p = PathBuf::from(&e.key);
            if p.is_absolute() { p } else { root.join(p) }
        }));
    }
    warfare.strict_geography = atom(&profile, "strict_geography") == Some("yes");
    if let Some(seed) = atom(&profile, "seed").and_then(|s| s.parse().ok()) {
        warfare.rng = crate::combat_rules::SeededRandom::new(seed);
    }
    let dlcs = block(&profile, "dlcs")
        .map(crate::military_data::names)
        .unwrap_or_default();
    for tag in world.countries.keys() {
        warfare
            .countries
            .insert(tag.clone(), CountryArmy::default());
    }
    for entry in profile.iter().filter(|e| e.key == "country") {
        if let Some(b) = entry.block() {
            if let Some(army) = atom(b, "tag").and_then(|t| warfare.countries.get_mut(t)) {
                army.base_modifiers = block(b, "modifier")
                    .map(|b| stats(b, &BTreeMap::new()))
                    .unwrap_or_default();
            }
        }
    }
    warfare.refresh_modifiers(research);
    for army in warfare.countries.values_mut() {
        army.modifiers.dlcs = dlcs.clone();
    }
    let mut idea_modifiers = BTreeMap::<String, StatBlock>::new();
    for path in layered_files(&layers, "common/ideas", false, "txt") {
        if let Some(entries) = fs::read_to_string(path)
            .ok()
            .and_then(|s| parse_pdx(&s).ok())
        {
            for group in block(&entries, "ideas").unwrap_or(&[]) {
                for idea in group.block().unwrap_or(&[]) {
                    if let Some(b) = idea.block().and_then(|b| block(b, "modifier")) {
                        idea_modifiers.insert(idea.key.clone(), stats(b, &BTreeMap::new()));
                    }
                }
            }
        }
    }
    let histories = layered_files(&layers, "history/countries", false, "txt");
    let unit_files = layered_files(&layers, "history/units", false, "txt");
    let mut populations = BTreeMap::<String, u64>::new();
    // Starting core population and land forts; occupation manpower is not guessed.
    for path in layered_files(&layers, "history/states", false, "txt") {
        if let Some(entries) = fs::read_to_string(path)
            .ok()
            .and_then(|s| parse_pdx(&s).ok())
        {
            let state = block(&entries, "state").unwrap_or(&[]);
            let h = block(state, "history").unwrap_or(&[]);
            if let Some(owner) = atom(h, "owner") {
                let core = h
                    .iter()
                    .any(|e| e.key == "add_core_of" && e.atom() == Some(owner));
                if core {
                    *populations.entry(owner.into()).or_default() +=
                        number(state, "manpower", 0.0).max(0.0) as u64;
                }
            }
            for province in block(h, "buildings").unwrap_or(&[]) {
                if let (Ok(id), Some(b)) = (province.key.parse::<u32>(), province.block()) {
                    warfare.conditions.entry(id).or_default().fort_level =
                        number(b, "bunker", 0.0).clamp(0.0, 100.0) as u16;
                }
            }
        }
    }
    let tags: Vec<_> = world.countries.keys().cloned().collect();
    for tag in tags {
        let mut oobs = Vec::new();
        let mut ideas = BTreeSet::new();
        let path = histories.iter().find(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.starts_with(&format!("{tag} -")) || s == format!("{tag}.txt"))
        });
        if let Some(path) = path {
            match fs::read_to_string(path)
                .map_err(|e| e.to_string())
                .and_then(|s| parse_pdx(&s))
            {
                Ok(entries) => effects(
                    &entries,
                    &tag,
                    &mut warfare,
                    production,
                    &mut oobs,
                    &mut ideas,
                    today,
                ),
                Err(e) => warfare.diagnostics.push(format!("{tag}: {e}")),
            }
        }
        for id in ideas {
            if let Some(values) = idea_modifiers.get(&id) {
                for (k, v) in values {
                    *warfare
                        .countries
                        .get_mut(&tag)
                        .unwrap()
                        .base_modifiers
                        .entry(k.clone())
                        .or_default() += v;
                }
            }
        }
        // A deterministic known filename is allowed only when country history
        // supplies no OOB. Never import every bookmark into the same campaign.
        if oobs.is_empty()
            && unit_files
                .iter()
                .any(|p| p.file_stem().and_then(|s| s.to_str()) == Some(&format!("{tag}_1936")))
        {
            oobs.push(format!("{tag}_1936"));
            warfare
                .diagnostics
                .push(format!("{tag}: using explicit 1936 OOB filename fallback"));
        }
        let mut loaded = BTreeSet::new();
        let mut pending = Vec::new();
        for oob in oobs {
            if !loaded.insert(oob.clone()) {
                continue;
            }
            let file = unit_files
                .iter()
                .find(|p| p.file_stem().and_then(|s| s.to_str()) == Some(oob.as_str()));
            let result = file
                .ok_or_else(|| format!("{tag}: missing OOB {oob}"))
                .and_then(|p| fs::read_to_string(p).map_err(|e| e.to_string()))
                .and_then(|s| parse_oob(&s));
            match result {
                Ok(oob) => {
                    warfare.diagnostics.extend(oob.diagnostics);
                    for t in oob.templates {
                        warfare
                            .countries
                            .get_mut(&tag)
                            .unwrap()
                            .templates
                            .insert(t.name.clone(), t);
                    }
                    pending.extend(oob.divisions);
                }
                Err(e) => warfare.diagnostics.push(e),
            }
        }
        warfare.refresh_modifiers(research);
        for d in pending {
            if let Err(e) = warfare.spawn_history(
                &tag,
                &d.template,
                d.province,
                &d.name,
                d.equipment_factor,
                d.manpower_factor,
                d.experience,
                world,
            ) {
                warfare.diagnostics.push(format!("{tag} / {}: {e}", d.name));
            }
        }
        let army = warfare.countries.get_mut(&tag).unwrap();
        let rate = army
            .base_modifiers
            .get("conscription")
            .copied()
            .unwrap_or(0.0);
        let serving: u64 = warfare
            .divisions
            .values()
            .filter(|d| d.country == tag)
            .map(|d| u64::from(d.manpower))
            .sum();
        army.manpower_pool = army
            .manpower_pool
            .saturating_add(
                (populations.get(&tag).copied().unwrap_or(0) as f64 * rate.clamp(0.0, 1.0)) as u64,
            )
            .saturating_sub(serving);
        if army.templates.is_empty() {
            warfare
                .diagnostics
                .push(format!("{tag}: no starting land templates imported"));
        }
    }
    // Import dated war history independently from country political history.
    for path in layered_files(&layers, "history/diplomacy", true, "txt") {
        if let Some(root) = fs::read_to_string(&path)
            .ok()
            .and_then(|s| parse_pdx(&s).ok())
        {
            for war in root
                .iter()
                .filter(|e| e.key == "war")
                .filter_map(Entry::block)
            {
                let h = block(war, "history").unwrap_or(war);
                let mut attackers = BTreeSet::new();
                let mut defenders = BTreeSet::new();
                let mut events: Vec<_> = h
                    .iter()
                    .filter_map(|e| date(&e.key).filter(|d| *d <= today).map(|d| (d, e)))
                    .collect();
                events.sort_by_key(|(d, _)| *d);
                for (_, event) in events {
                    for effect in event.block().unwrap_or(&[]) {
                        if let Some(tag) = effect.atom() {
                            match effect.key.as_str() {
                                "add_attacker" => {
                                    attackers.insert(tag.to_owned());
                                }
                                "add_defender" => {
                                    defenders.insert(tag.to_owned());
                                }
                                "rem_attacker" | "remove_attacker" => {
                                    attackers.remove(tag);
                                }
                                "rem_defender" | "remove_defender" => {
                                    defenders.remove(tag);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                for a in &attackers {
                    for b in &defenders {
                        if let Err(e) = warfare.declare_war(a, b, world) {
                            warfare.diagnostics.push(format!("{}: {e}", path.display()));
                        }
                    }
                }
            }
        }
    }
    // Explicit scenario overrides permit controlled comparison fixtures.
    for entry in profile.iter().filter(|e| e.key == "province") {
        if let Some(b) = entry.block() {
            let id = number(b, "id", 0.0) as u32;
            if !world.geography.provinces.contains_key(&id) {
                continue;
            }
            let c = warfare.conditions.entry(id).or_default();
            c.weather = atom(b, "weather").map(str::to_owned);
            c.night_override = atom(b, "night").map(|v| v == "yes");
            c.fort_level =
                number(b, "fort_level", f64::from(c.fort_level)).clamp(0.0, 100.0) as u16;
            for (key, air) in [
                ("attacker_air", &mut c.attacker_air),
                ("defender_air", &mut c.defender_air),
            ] {
                if let Some(b) = block(b, key) {
                    air.ground_support_bonus = number(b, "ground_support_bonus", 0.0);
                    air.enemy_superiority = number(b, "enemy_superiority", 0.0).clamp(0.0, 1.0);
                    air.cas_planes =
                        number(b, "cas_planes", 0.0).clamp(0.0, f64::from(u32::MAX)) as u32;
                    air.plane_attack_rolls = number(b, "plane_attack_rolls", 0.0).max(0.0);
                    air.aa_damage_reduction = number(b, "aa_damage_reduction", 0.0).clamp(0.0, 1.0);
                }
            }
        }
    }
    for entry in profile.iter().filter(|e| e.key == "country") {
        if let Some(b) = entry.block() {
            if let Some(army) = atom(b, "tag").and_then(|t| warfare.countries.get_mut(t)) {
                army.manpower_pool =
                    number(b, "manpower_pool", army.manpower_pool as f64).max(0.0) as u64;
                army.army_xp = number(b, "army_xp", army.army_xp).max(0.0);
            }
        }
    }
    for entry in profile.iter().filter(|e| e.key == "division") {
        if let Some(b) = entry.block() {
            for d in warfare.divisions.values_mut().filter(|d| {
                atom(b, "country") == Some(d.country.as_str())
                    && atom(b, "name") == Some(d.name.as_str())
            }) {
                d.supply_fraction = number(b, "supply_fraction", d.supply_fraction).clamp(0.0, 1.0);
                d.fuel_fraction = number(b, "fuel_fraction", d.fuel_fraction).clamp(0.0, 1.0);
                d.commander_attack = number(b, "commander_attack", d.commander_attack);
                d.commander_defense = number(b, "commander_defense", d.commander_defense);
            }
        }
    }
    for entry in profile.iter().filter(|e| e.key == "strait_access") {
        if let Some(b) = entry.block() {
            if let Some(country) = atom(b, "country") {
                let a = number(b, "from", 0.0) as u32;
                let z = number(b, "to", 0.0) as u32;
                warfare
                    .strait_access
                    .insert((country.into(), a.min(z), a.max(z)));
            }
        }
    }
    for entry in profile.iter().filter(|e| e.key == "military_access") {
        if let Some(b) = entry.block() {
            if let Some((country, host)) = atom(b, "country").zip(atom(b, "host")) {
                warfare
                    .military_access
                    .insert((country.into(), host.into()));
            }
        }
    }
    for entry in profile.iter().filter(|e| e.key == "war") {
        if let Some(b) = entry.block() {
            if let Some((a, b)) = atom(b, "attacker").zip(atom(b, "defender")) {
                if let Err(e) = warfare.declare_war(a, b, world) {
                    warfare.diagnostics.push(e);
                }
            }
        }
    }
    warfare
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_starting_army_import_uses_history_laws_models_and_dated_wars() {
        let dir = std::env::temp_dir().join(format!(
            "hoi4-military-history-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fn put(dir: &Path, path: &str, source: &str) {
            let p = dir.join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, source).unwrap();
        }
        put(
            &dir,
            "common/defines/00.lua",
            "NDefines = {\nNMilitary = {\nMAP_SCALE_PIXEL_TO_KM = 1,\nBASE_REINFORCE_CHANCE = 0.02,\n},\n}",
        );
        put(
            &dir,
            "common/units/equipment/rifles.txt",
            "equipments={ rifles={is_archetype=yes active=yes soft_attack=2 build_cost_ic=1} rifle_1={archetype=rifles year=1936} }",
        );
        put(
            &dir,
            "common/units/infantry.txt",
            "sub_units={infantry={group=infantry active=yes max_strength=25 max_organisation=60 manpower=1000 combat_width=2 maximum_speed=4 need={rifles=100}}}",
        );
        put(
            &dir,
            "common/ideas/laws.txt",
            "ideas={conscription_law={law={modifier={conscription=0.1}}}}",
        );
        put(
            &dir,
            "history/countries/A - Test.txt",
            "oob=A_1936 add_ideas=law set_country_flag=ready set_army_experience=20 add_equipment_to_stockpile={type=rifle_1 amount=11} 1939.1.1={load_oob=A_1939}",
        );
        put(
            &dir,
            "history/states/1.txt",
            "state={id=1 manpower=100000 history={owner=A add_core_of=A buildings={1={bunker=3}}}}",
        );
        put(
            &dir,
            "history/units/A_1936.txt",
            "division_template={name=T regiments={infantry={x=0 y=0}}} units={division={name=D division_template=T location=1 start_equipment_factor=0.5 start_manpower_factor=0.6 start_experience_factor=0.3}}",
        );
        put(
            &dir,
            "history/units/A_1939.txt",
            "units={division={name=Future division_template=T location=1}}",
        );
        put(
            &dir,
            "history/diplomacy/test.txt",
            "war={history={1935.1.1={add_attacker=A add_defender=B} 1939.1.1={rem_defender=B}}}",
        );
        let world = crate::warfare::tests::world();
        let mut p = ProductionState::default();
        let w = initialize(
            &dir,
            &world,
            &ResearchWorld::default(),
            &mut p,
            GameDate::START,
        );
        assert_eq!(w.divisions.len(), 1);
        let d = &w.divisions[&1];
        assert_eq!(d.name, "D");
        assert_eq!(d.equipment["rifle_1"], 50);
        assert_eq!(d.manpower, 600);
        assert_eq!(d.hp, 12.5);
        assert_eq!(w.countries["A"].manpower_pool, 9400);
        assert_eq!(w.countries["A"].army_xp, 20.0);
        assert!(w.countries["A"].modifiers.flags.contains("ready"));
        assert_eq!(p.country("A").unwrap().model_stockpile["rifle_1"], 11);
        assert_eq!(w.conditions[&1].fort_level, 3);
        assert!(w.at_war("A", "B"));
        assert_eq!(w.catalogue.rules.pixels_to_km, Some(1.0));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn unresolved_if_cannot_fall_through_to_an_else_if() {
        let mut w = Warfare::default();
        w.countries.insert("A".into(), CountryArmy::default());
        let mut p = ProductionState::default();
        let mut o = Vec::new();
        let mut i = BTreeSet::new();
        let e=parse_pdx("if={limit={unknown=yes} load_oob=maybe} else_if={limit={always=yes} load_oob=unsafe} else={load_oob=unsafe_too}").unwrap();
        effects(&e, "A", &mut w, &mut p, &mut o, &mut i, GameDate::START);
        assert!(o.is_empty());
    }
    #[test]
    fn future_and_unsupported_negated_history_do_not_create_troops_or_stockpiles() {
        let mut w = Warfare::default();
        w.countries.insert("A".into(), CountryArmy::default());
        let mut p = ProductionState::default();
        let mut o = Vec::new();
        let mut i = BTreeSet::new();
        let e=parse_pdx("oob=A_1936 1939.1.1={load_oob=A_1939} if={limit={NOT={unimplemented=yes}} load_oob=bad} else={load_oob=also_bad} add_manpower=12").unwrap();
        effects(&e, "A", &mut w, &mut p, &mut o, &mut i, GameDate::START);
        assert_eq!(o, vec!["A_1936"]);
        assert_eq!(w.countries["A"].manpower_pool, 12);
        assert!(!w.diagnostics.is_empty());
    }
}
