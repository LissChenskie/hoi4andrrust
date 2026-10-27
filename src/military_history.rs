//! Starting armies and explicit campaign inputs. No generated historical armies.
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::game::GameDate;
use crate::military_data::{atom, block, layered_files, number, parse_oob_for_country, stats};
use crate::military_model::{CountryArmy, StatBlock};
use crate::production::{Equipment, ProductionRules, ProductionState, StartingProduction};
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

fn literal(b: &[Entry], key: &str, default: f64) -> Result<f64, String> {
    atom(b, key).map_or(Ok(default), |raw| {
        raw.parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .ok_or_else(|| format!("{key}: unsupported numeric expression {raw}"))
    })
}
fn stockpile_effect(
    b: &[Entry],
    tag: &str,
    w: &Warfare,
    production: &mut ProductionState,
) -> Result<(), String> {
    let kind = atom(b, "type").ok_or("stockpile entry has no type")?;
    if atom(b, "variant_name").is_some_and(|name| !name.is_empty()) {
        return Err(format!(
            "stockpile {kind}: named variant requires design import"
        ));
    }
    let amount = literal(b, "amount", 0.0)?;
    if amount.fract() != 0.0 || amount.abs() > i64::MAX as f64 {
        return Err("stockpile amount must be an integer".into());
    }
    let definition = w
        .catalogue
        .equipment
        .get(kind)
        .ok_or_else(|| format!("unresolved stockpile type {kind}"))?;
    let stock = &mut production.country_mut(tag).model_stockpile;
    if amount < 0.0 {
        let mut remaining = (-amount) as u64;
        let ids: Vec<_> = stock
            .keys()
            .filter(|id| {
                id.as_str() == kind
                    || (!definition.buildable
                        && w.catalogue
                            .equipment
                            .get(*id)
                            .is_some_and(|e| e.archetype == kind))
            })
            .cloned()
            .collect();
        for id in ids {
            let count = stock.get_mut(&id).unwrap();
            let taken = remaining.min(*count);
            *count -= taken;
            remaining -= taken;
        }
        return Ok(());
    }
    let model = if definition.buildable && !definition.modular {
        Some(definition)
    } else if !definition.buildable {
        w.catalogue
            .best_equipment(kind, &w.countries[tag].modifiers)
    } else {
        None
    }
    .ok_or_else(|| {
        format!("stockpile {kind}: no resolved model for the selected country's technology")
    })?;
    let n = stock.entry(model.id.clone()).or_default();
    *n = n.saturating_add(amount as u64);
    Ok(())
}
fn production_effect(
    b: &[Entry],
    tag: &str,
    source: &str,
    w: &Warfare,
    production: &mut ProductionState,
    rules: &ProductionRules,
) -> Result<(), String> {
    let e = block(b, "equipment").ok_or("production line has no equipment block")?;
    let kind = atom(e, "type").ok_or("production line has no equipment type")?;
    let creator = atom(e, "creator").map(str::to_owned);
    let variant = atom(e, "version_name").map(str::to_owned);
    let requested = literal(b, "requested_factories", 1.0)?;
    let progress = literal(b, "progress", 0.0)?;
    let amount = literal(b, "amount", 0.0)?;
    if amount < 0.0 || amount.fract() != 0.0 || amount > u64::MAX as f64 {
        return Err(format!("production {kind}: invalid amount"));
    }
    let efficiency = literal(b, "efficiency", rules.starting_efficiency * 100.0)? / 100.0;
    if requested < 0.0
        || requested.fract() != 0.0
        || requested > u32::MAX as f64
        || !(0.0..=1.0).contains(&progress)
        || efficiency < 0.0
    {
        return Err(format!(
            "production {kind}: invalid factories/progress/efficiency"
        ));
    }
    let definition = w.catalogue.equipment.get(kind);
    let archetype = definition.map_or(kind, |e| e.archetype.as_str());
    let naval = [
        "ship_",
        "convoy",
        "submarine",
        "cruiser",
        "battleship",
        "destroyer",
        "carrier",
    ]
    .iter()
    .any(|p| archetype.contains(p));
    let category = if archetype.contains("infantry") || archetype.contains("rifle") {
        Equipment::InfantryKit
    } else if archetype.contains("support") {
        Equipment::SupportKit
    } else if archetype.contains("motorized") || archetype.contains("truck") {
        Equipment::Truck
    } else if ["plane", "airframe", "fighter", "bomber", "cas_equipment"]
        .iter()
        .any(|p| archetype.contains(p))
    {
        Equipment::Aircraft
    } else {
        Equipment::FieldEquipment
    };
    let blocked = if naval {
        Some("Требуется морское производство на верфях".to_owned())
    } else if variant.as_ref().is_some_and(|v| !v.is_empty()) {
        Some("Именной вариант требует импорта конструкции".to_owned())
    } else if definition.is_none() {
        Some(format!("Не найдено определение {kind}"))
    } else if definition.is_some_and(|e| e.modular) {
        Some("Не разрешена модульная конструкция".to_owned())
    } else if !w
        .catalogue
        .equipment_available(kind, &w.countries[tag].modifiers)
    {
        Some("Модель недоступна по стартовым технологиям".to_owned())
    } else if definition.is_some_and(|e| !e.cost.is_finite() || e.cost <= 0.0) {
        Some("Неизвестна производственная стоимость".to_owned())
    } else {
        None
    };
    let country = production.country_mut(tag);
    country.create(category, requested as u32, rules);
    let line = country.lines.last_mut().unwrap();
    line.starting = Some(StartingProduction {
        equipment: kind.into(),
        creator,
        variant,
        source: source.into(),
    });
    line.blocked_reason = blocked;
    line.remaining = (amount > 0.0).then_some(amount as u64);
    line.efficiency = efficiency.min(rules.efficiency_cap.max(0.0));
    if line.blocked_reason.is_none() {
        line.model = Some(kind.into());
        line.partial_ic = progress * definition.unwrap().cost;
    }
    Ok(())
}

fn effects(
    entries: &[Entry],
    tag: &str,
    warfare: &mut Warfare,
    production: &mut ProductionState,
    oobs: &mut Vec<String>,
    ideas: &mut BTreeSet<String>,
    today: GameDate,
    rules: &ProductionRules,
    source: &str,
) -> bool {
    let initial = warfare.countries[tag].modifiers.clone();
    let (resolved, diagnostics) = crate::history_conditions::resolve(entries, tag, &initial, today);
    let resolved_all = diagnostics.is_empty();
    warfare
        .countries
        .get_mut(tag)
        .unwrap()
        .history_errors
        .extend(diagnostics.iter().cloned());
    warfare.diagnostics.extend(diagnostics);
    for entry in &resolved {
        let army = warfare.countries.get_mut(tag).unwrap();
        match entry.key.as_str() {
            "oob" | "set_oob" => {
                if let Some(id) = entry.atom() {
                    oobs.clear();
                    oobs.push(id.into());
                }
            }
            "load_oob" => {
                if let Some(id) = entry.atom() {
                    oobs.push(id.into());
                }
            }
            "set_country_flag" => {
                if let Some(flag) = entry.atom() {
                    army.modifiers.flags.insert(flag.into());
                }
            }
            "clr_country_flag" => {
                if let Some(flag) = entry.atom() {
                    army.modifiers.flags.remove(flag);
                }
            }
            "add_command_power" => {
                if let Some(v) = entry
                    .atom()
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite())
                {
                    army.command_power = Some((army.command_power.unwrap_or(0.0) + v).max(0.0));
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
            "add_equipment_to_stockpile" | "add_equipment_production" => {
                if !production
                    .country(tag)
                    .is_some_and(|c| c.starting_data_applied)
                {
                    let result = entry
                        .block()
                        .ok_or_else(|| "missing effect block".to_owned())
                        .and_then(|b| {
                            if entry.key == "add_equipment_to_stockpile" {
                                stockpile_effect(b, tag, warfare, production)
                            } else {
                                production_effect(b, tag, source, warfare, production, rules)
                            }
                        });
                    if let Err(error) = result {
                        warfare
                            .diagnostics
                            .push(format!("{tag}: {source}: {error}"));
                    }
                }
            }
            "create_equipment_variant" => warfare.diagnostics.push(format!(
                "{tag}: custom equipment variant needs design import; no substitute is fabricated"
            )),
            _ => {}
        }
    }
    resolved_all
}

pub fn initialize(
    root: &Path,
    world: &World,
    research: &ResearchWorld,
    production: &mut ProductionState,
    today: GameDate,
) -> Warfare {
    let mut warfare = Warfare::new(crate::military_data::load(root));
    let layers = crate::military_data::data_layers(root);
    let profile = fs::read_to_string(root.join("military_profile.txt"))
        .ok()
        .and_then(|s| parse_pdx(&s).ok())
        .unwrap_or_default();
    warfare.strict_geography = atom(&profile, "strict_geography") == Some("yes");
    if let Some(seed) = atom(&profile, "seed").and_then(|s| s.parse().ok()) {
        warfare.rng = crate::combat_rules::SeededRandom::new(seed);
    }
    let dlcs = crate::history_conditions::profile_dlcs(root);
    for tag in world.countries.keys() {
        warfare.countries.insert(
            tag.clone(),
            CountryArmy {
                command_power: warfare
                    .catalogue
                    .literal_defines
                    .get("NDefines.NCountry.STARTING_COMMAND_POWER")
                    .copied(),
                ..Default::default()
            },
        );
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
            for vp in h
                .iter()
                .filter(|e| e.key == "victory_points")
                .filter_map(Entry::block)
            {
                for pair in vp.chunks_exact(2) {
                    if let (Ok(id), Ok(value)) =
                        (pair[0].key.parse::<u32>(), pair[1].key.parse::<f64>())
                    {
                        warfare.conditions.entry(id).or_default().victory_points = value.max(0.0);
                    }
                }
            }
            for province in block(h, "buildings").unwrap_or(&[]) {
                if let (Ok(id), Some(b)) = (province.key.parse::<u32>(), province.block()) {
                    let c = warfare.conditions.entry(id).or_default();
                    c.fort_level = number(b, "bunker", 0.0).clamp(0.0, 100.0) as u16;
                    c.naval_base = number(b, "naval_base", 0.0).clamp(0.0, 100.0) as u16;
                    c.supply_node = number(b, "supply_node", 0.0) > 0.0;
                }
            }
        }
    }
    let tags: Vec<_> = world.countries.keys().cloned().collect();
    for tag in tags {
        let production_rules = ProductionRules::from_defines(&warfare.catalogue.literal_defines)
            .for_research(research.country(&tag));
        let mut oobs = Vec::new();
        let mut oob_history_known = true;
        let mut ideas = BTreeSet::new();
        let path = histories.iter().find(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.starts_with(&format!("{tag} -")) || s == format!("{tag}.txt"))
        });
        if let Some(path) = path {
            warfare.countries.get_mut(&tag).unwrap().history_source =
                Some(path.display().to_string());
            match fs::read_to_string(path)
                .map_err(|e| e.to_string())
                .and_then(|s| crate::pdx::parse_pdx_compatible(&s))
            {
                Ok(parsed) => {
                    for span in &parsed.eof_terminated_blocks {
                        warfare.diagnostics.push(format!(
                            "{tag}: country history outer block at {}:{} ends at EOF",
                            span.line, span.column
                        ));
                    }
                    oob_history_known = effects(
                        &hoi4_mobile::script_constants::resolve(
                            &parsed.entries,
                            &Default::default(),
                        ),
                        &tag,
                        &mut warfare,
                        production,
                        &mut oobs,
                        &mut ideas,
                        today,
                        &production_rules,
                        &path.display().to_string(),
                    );
                }
                Err(e) => {
                    oob_history_known = false;
                    let error = format!("{}: {e}", path.display());
                    warfare
                        .countries
                        .get_mut(&tag)
                        .unwrap()
                        .history_errors
                        .push(error.clone());
                    warfare.diagnostics.push(error);
                }
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
            && oob_history_known
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
        warfare.countries.get_mut(&tag).unwrap().history_oobs = oobs.clone();
        for oob in oobs {
            if !loaded.insert(oob.clone()) {
                continue;
            }
            let file = unit_files
                .iter()
                .find(|p| p.file_stem().and_then(|s| s.to_str()) == Some(oob.as_str()));
            let result = file
                .ok_or_else(|| format!("{tag}: missing OOB {oob}"))
                .and_then(|p| fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display())))
                .and_then(|s| {
                    parse_oob_for_country(&s, &tag, &warfare.countries[&tag].modifiers, today)
                })
                .map_err(|e| format!("{tag}: history/units/{oob}.txt: {e}"));
            let oob_id = oob;
            match result {
                Ok(oob) => {
                    effects(
                        &oob.initial_effects,
                        &tag,
                        &mut warfare,
                        production,
                        &mut Vec::new(),
                        &mut BTreeSet::new(),
                        today,
                        &production_rules,
                        &format!("history/units/{oob_name}.txt", oob_name = oob_id),
                    );
                    let army = warfare.countries.get_mut(&tag).unwrap();
                    army.history_expected += oob.declared_divisions;
                    army.history_errors.extend(oob.diagnostics.iter().cloned());
                    warfare.diagnostics.extend(oob.diagnostics);
                    warfare.diagnostics.extend(oob.warnings);
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
                Err(e) => {
                    warfare
                        .countries
                        .get_mut(&tag)
                        .unwrap()
                        .history_errors
                        .push(e.clone());
                    warfare.diagnostics.push(e);
                }
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
                let error = format!("{tag} / {}: {e}", d.name);
                warfare
                    .countries
                    .get_mut(&tag)
                    .unwrap()
                    .history_errors
                    .push(error.clone());
                warfare.diagnostics.push(error);
            } else {
                warfare.countries.get_mut(&tag).unwrap().history_imported += 1;
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
        production.country_mut(&tag).starting_data_applied = true;
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
    warfare.command = crate::command::load(root, &warfare, today);
    crate::command_abilities::load(root, &mut warfare);
    warfare
}

/// Local diagnostics retain the selected OOB and the exact failure instead of
/// reporting a failed import as a country that historically had no divisions.
pub fn import_report(warfare: &Warfare, date: GameDate) -> serde_json::Value {
    let countries: BTreeMap<_, _> = warfare
        .countries
        .iter()
        .map(|(tag, army)| {
            (
                tag,
                serde_json::json!({
                    "history_source": army.history_source,
                    "selected_oobs": army.history_oobs,
                    "declared_divisions": army.history_expected,
                    "imported_divisions": army.history_imported,
                    "templates": army.templates.keys().collect::<Vec<_>>(),
                    "errors": army.history_errors,
                }),
            )
        })
        .collect();
    serde_json::json!({"scenario": date.display(), "target_version":"1.19.3", "source_version": warfare.catalogue.source_version, "source_checksum": warfare.catalogue.source_checksum, "rule_sources": warfare.catalogue.rules.provenance, "original_engine_comparisons":0, "countries": countries, "command_data": warfare.command.data, "command_summary": warfare.command.data.summary(), "army_views": warfare.command.armies.values().filter_map(|a| crate::army_projection::army(warfare, &a.country, a.id, "russian").ok()).collect::<Vec<_>>(), "diagnostics": warfare.diagnostics})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conditional_bookmarks_load_the_selected_army_and_report_missing_sources() {
        let root = std::env::temp_dir().join(format!(
            "hoi4-oob-selection-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let put = |path: &str, text: &str| {
            let p = root.join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        };
        put(
            "common/units/infantry.txt",
            "sub_units={infantry={group=infantry active=yes max_strength=25 max_organisation=60 manpower=1000 combat_width=2 maximum_speed=4}}",
        );
        put("military_profile.txt", "dlcs={expansion}");
        // Authored fixture: country A, not an asserted historical French count.
        put(
            "history/countries/A - Test.txt",
            "if={limit={has_dlc=expansion} set_oob=A_dlc else={set_oob=A_legacy}} 1939.1.1={set_oob=A_late}",
        );
        let template = "division_template={name=Infantry regiments={infantry={x=0 y=0}}} ";
        put(
            "history/units/A_dlc.txt",
            &format!(
                "{template} set_country_flag=deployed units={{IF={{LIMIT={{has_country_flag=deployed has_dlc=expansion}} division={{name=Existing division_template=Infantry location=1}} ELSE={{division={{name=Wrong division_template=Infantry location=1}}}}}}"
            ),
        );
        put(
            "history/units/A_late.txt",
            &format!(
                "{template} units={{division={{name=Later division_template=Infantry location=1}}}}"
            ),
        );
        let world = crate::warfare::tests::world();
        let run = |date| {
            initialize(
                &root,
                &world,
                &Default::default(),
                &mut Default::default(),
                date,
            )
        };
        let early = run(GameDate::START);
        assert_eq!(early.countries["A"].history_oobs, vec!["A_dlc"]);
        assert_eq!(early.countries["A"].history_imported, 1);
        assert_eq!(early.divisions.values().next().unwrap().name, "Existing");
        assert!(early.countries["A"].history_errors.is_empty());
        assert!(
            early
                .diagnostics
                .iter()
                .any(|s| s.contains("OOB outer block") && s.contains("ends at EOF"))
        );
        let late = run(GameDate {
            year: 1939,
            month: 8,
            day: 14,
        });
        assert_eq!(late.countries["A"].history_oobs, vec!["A_late"]);
        assert_eq!(late.countries["A"].history_imported, 1);
        assert_eq!(late.divisions.values().next().unwrap().name, "Later");
        fs::remove_file(root.join("history/units/A_dlc.txt")).unwrap();
        let failed = run(GameDate::START);
        let report = import_report(&failed, GameDate::START);
        assert_eq!(report["countries"]["A"]["selected_oobs"][0], "A_dlc");
        assert!(
            report["countries"]["A"]["errors"][0]
                .as_str()
                .unwrap()
                .contains("history/units/A_dlc.txt")
        );
        assert_eq!(failed.countries["A"].history_imported, 0);
        put(
            "history/units/A_1936.txt",
            &format!(
                "{template} units={{division={{name=WrongFallback division_template=Infantry location=1}}}}"
            ),
        );
        put(
            "history/countries/A - Test.txt",
            "if={limit={unknown_predicate=yes} set_oob=A_dlc else={set_oob=A_1936}}",
        );
        let unknown = run(GameDate::START);
        assert!(unknown.countries["A"].history_oobs.is_empty());
        assert!(!unknown.countries["A"].history_errors.is_empty());
        assert_eq!(unknown.countries["A"].history_imported, 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn real_support_grid_defines_do_not_discard_armies_with_several_companies() {
        let root = std::env::temp_dir().join(format!(
            "hoi4-army-count-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fn put(root: &Path, path: &str, s: &str) {
            let p = root.join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, s).unwrap();
        }
        put(
            &root,
            "common/defines/00.lua",
            "NDefines = {\nNMilitary = {\nMAX_DIVISION_SUPPORT_WIDTH = 1,\nMAX_DIVISION_SUPPORT_HEIGHT = 5,\n},\n}",
        );
        put(
            &root,
            "common/units/parts.txt",
            "sub_units={infantry={group=infantry active=yes max_strength=25 max_organisation=60 manpower=1000 combat_width=2 maximum_speed=4 need={rifles=100}} engineer={group=support active=yes same_support_type=engineer max_strength=2 max_organisation=20 manpower=300 need={rifles=10}} artillery={group=support active=yes same_support_type=artillery max_strength=0.2 manpower=300 need={rifles=12}}}",
        );
        put(
            &root,
            "common/units/equipment/rifles.txt",
            "equipments={rifles={is_archetype=yes active=yes soft_attack=10 build_cost_ic=1} rifle_1={archetype=rifles year=1936}}",
        );
        put(
            &root,
            "history/countries/A - Test.txt",
            "if={limit={has_dlc=D} set_oob=wrong else={set_oob=A_1936}} ",
        );
        let mut oob = String::from(
            "division_template={name=Infantry regiments={infantry={x=0 y=0}} support={engineer={x=0 y=0} artillery={x=0 y=1}}} division_template={name=Mountain regiments={infantry={x=0 y=0}}} units={",
        );
        for i in 1..32 {
            oob.push_str(&format!("division={{division_name={{is_name_ordered=yes name_order={i}}} division_template=Infantry location=1}}"));
        }
        oob.push_str("division={name=Independent division_template=Mountain location=1}}");
        put(&root, "history/units/A_1936.txt", &oob);
        let world = crate::warfare::tests::world();
        let w = initialize(
            &root,
            &world,
            &Default::default(),
            &mut Default::default(),
            GameDate::START,
        );
        assert_eq!(w.countries["A"].history_expected, 32);
        assert_eq!(w.countries["A"].history_imported, 32);
        assert_eq!(
            w.divisions.values().filter(|d| d.country == "A").count(),
            32
        );
        assert!(w.countries["A"].history_errors.is_empty());
        assert_eq!(w.divisions[&1].name, "1. Infantry");
        assert_eq!(w.divisions[&31].name, "31. Infantry");
        fs::remove_dir_all(root).unwrap();
    }
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
        effects(
            &e,
            "A",
            &mut w,
            &mut p,
            &mut o,
            &mut i,
            GameDate::START,
            &ProductionRules::default(),
            "fixture",
        );
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
        effects(
            &e,
            "A",
            &mut w,
            &mut p,
            &mut o,
            &mut i,
            GameDate::START,
            &ProductionRules::default(),
            "fixture",
        );
        assert_eq!(o, vec!["A_1936"]);
        assert_eq!(w.countries["A"].manpower_pool, 12);
        assert!(!w.diagnostics.is_empty());
    }
}

#[cfg(test)]
mod starting_economy_regressions {
    use super::*;
    #[test]
    fn selected_oob_instant_effects_import_real_lines_stocks_and_progress_once_per_country() {
        let dir = std::env::temp_dir().join(format!(
            "hoi4-starting-industry-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let put = |path: &str, source: &str| {
            let p = dir.join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, source).unwrap();
        };
        put("military_profile.txt", "dlcs={Chosen}");
        put(
            "common/defines/production.lua",
            "NDefines.NProduction.BASE_FACTORY_MAX_EFFICIENCY_FACTOR = 30\n",
        );
        put(
            "common/units/equipment/rifles.txt",
            "equipments={rifles={is_archetype=yes active=yes build_cost_ic=2} rifle_1={archetype=rifles year=1936} rifle_future={archetype=rifles year=1945 active=no}}",
        );
        put(
            "history/countries/A.txt",
            "oob=A_Unselected IF={LIMIT={has_dlc=Chosen} set_oob=A_Selected} ELSE={set_oob=A_Legacy} load_oob=A_Selected add_equipment_to_stockpile={type=rifle_1 amount=11}",
        );
        put("history/countries/B.txt", "oob=B_1936");
        put(
            "history/units/A_Unselected.txt",
            "instant_effect={add_equipment_to_stockpile={type=rifle_1 amount=9000} add_equipment_production={equipment={type=rifle_1} requested_factories=99}}",
        );
        put(
            "history/units/A_Selected.txt",
            r#"@count=8
            instant_effect={
                if={limit={has_dlc=Chosen} add_equipment_to_stockpile={type=rifles amount=@count}}
                add_equipment_to_stockpile={type=rifles amount=-2}
                add_equipment_production={equipment={type=rifle_1 creator=A} requested_factories=2 progress=0.25 efficiency=40}
                add_equipment_production={equipment={type=rifle_1 creator=A version_name="Unknown design"} requested_factories=5 efficiency=100}
                add_equipment_production={equipment={type=convoy_1} requested_factories=2 amount=10}
                if={limit={unknown_condition=yes} add_equipment_to_stockpile={type=rifle_1 amount=10000}}
            }
            1939.1.1={instant_effect={add_equipment_to_stockpile={type=rifle_1 amount=9999}}}
        "#,
        );
        put(
            "history/units/B_1936.txt",
            "instant_effect={add_equipment_to_stockpile={type=rifle_1 amount=3} add_equipment_production={equipment={type=rifle_1 creator=B} requested_factories=1 efficiency=20}}",
        );
        let world = crate::warfare::tests::world();
        let mut p = ProductionState::default();
        let research = ResearchWorld::default();
        let w = initialize(&dir, &world, &research, &mut p, GameDate::START);
        let a = p.country("A").unwrap();
        assert_eq!(a.lines.len(), 3);
        assert_eq!(a.model_stockpile["rifle_1"], 17);
        assert!(!a.model_stockpile.contains_key("rifles"));
        assert!(!a.model_stockpile.contains_key("rifle_future"));
        assert_eq!(a.lines[0].model.as_deref(), Some("rifle_1"));
        assert_eq!(a.lines[0].requested_factories, 2);
        assert_eq!(a.lines[0].efficiency, 0.3);
        assert_eq!(a.lines[0].partial_ic, 0.5);
        assert_eq!(
            a.lines[0].starting.as_ref().unwrap().creator.as_deref(),
            Some("A")
        );
        assert!(a.lines[1].blocked_reason.is_some());
        assert!(a.lines[2].blocked_reason.is_some());
        assert_eq!(p.country("B").unwrap().model_stockpile["rifle_1"], 3);
        assert_eq!(p.country("B").unwrap().lines.len(), 1);
        assert!(
            w.diagnostics
                .iter()
                .any(|s| s.contains("unsupported conditional"))
        );
        let before = serde_json::to_string(&p).unwrap();
        initialize(&dir, &world, &research, &mut p, GameDate::START);
        assert_eq!(serde_json::to_string(&p).unwrap(), before);
        let rules = ProductionRules::from_defines(&w.catalogue.literal_defines);
        p.country_mut("A")
            .tick_with_models(2, &rules, &BTreeMap::from([("rifle_1".into(), 2.0)]));
        assert_eq!(p.country("A").unwrap().model_stockpile["rifle_1"], 18);
        assert!((p.country("A").unwrap().lines[0].last_day_ic - 2.7).abs() < 1e-12);
        assert!(p.country("A").unwrap().stockpile.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }
}
