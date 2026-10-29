//! Import geography and optional normalized campaign support data. No aircraft,
//! convoys, fuel or railways are generated to fill missing historical data.
use crate::{
    air_warfare::{AirRules, AirWing},
    game::GameDate,
    logistics::{CountryLogistics, LogisticsRules, RailSegment},
    military_data::{atom, block, layered_files, number},
    pdx::parse_pdx,
    warfare::Warfare,
    world::World,
};
use serde::Deserialize;
use std::{collections::BTreeMap, fs, path::Path};
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Profile {
    schema: u32,
    logistics_rules: Option<LogisticsRules>,
    air_rules: Option<AirRules>,
    countries: BTreeMap<String, CountryLogistics>,
    wings: Vec<AirWing>,
    bases: BTreeMap<u32, u16>,
    motorized_hubs: Vec<u32>,
    blocked_sea: Vec<u32>,
}
pub fn parse_railways(source: &str) -> Result<Vec<RailSegment>, String> {
    let mut rails = BTreeMap::<(u32, u32), u32>::new();
    for (line, s) in source.lines().enumerate() {
        let s = s.split('#').next().unwrap_or("").trim();
        if s.is_empty() {
            continue;
        }
        let v = s
            .split_whitespace()
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| format!("railways:{}: invalid integer", line + 1))?;
        if v.len() < 4 || !(1..=5).contains(&v[0]) || v[1] as usize != v.len() - 2 {
            return Err(format!("railways:{}: invalid level/count", line + 1));
        }
        for edge in v[2..].windows(2) {
            let pair = (edge[0].min(edge[1]), edge[0].max(edge[1]));
            rails
                .entry(pair)
                .and_modify(|n| *n = (*n).max(v[0]))
                .or_insert(v[0]);
        }
    }
    Ok(rails
        .into_iter()
        .map(|((from, to), level)| RailSegment { from, to, level })
        .collect())
}
pub fn load(root: &Path, world: &World, w: &mut Warfare, today: GameDate) {
    let layers = crate::military_data::data_layers(root);
    let mapfiles = layered_files(&layers, "map", false, "txt");
    for path in mapfiles {
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !matches!(name, "railways.txt" | "supply_nodes.txt") {
            continue;
        }
        let Ok(source) = fs::read_to_string(&path) else {
            w.diagnostics
                .push(format!("Cannot read {}", path.display()));
            continue;
        };
        if name == "railways.txt" {
            match parse_railways(&source) {
                Ok(rails) => {
                    for r in rails {
                        if world
                            .geography
                            .connection(r.from, r.to)
                            .is_some_and(|e| e.kind == crate::map_graph::CrossingKind::Border)
                        {
                            w.logistics.rails.push(r);
                        } else {
                            w.diagnostics.push(format!(
                                "Rail {}–{} is not an accessible map border",
                                r.from, r.to
                            ));
                        }
                    }
                }
                Err(e) => w.diagnostics.push(e),
            }
        } else {
            for (n, line) in source.lines().enumerate() {
                let line = line.split('#').next().unwrap_or("").trim();
                if line.is_empty() {
                    continue;
                }
                let v: Vec<_> = line.split_whitespace().collect();
                if let Some(p) = v
                    .get(1)
                    .filter(|_| v.len() == 2)
                    .and_then(|s| s.parse::<u32>().ok())
                    .filter(|p| world.provinces.contains_key(p))
                {
                    w.conditions.entry(p).or_default().supply_node = true;
                } else {
                    w.diagnostics
                        .push(format!("supply_nodes:{}: invalid province record", n + 1));
                }
            }
        }
    }
    for path in layered_files(&layers, "map/strategicregions", false, "txt") {
        match fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|s| parse_pdx(&s))
        {
            Ok(es) => {
                for r in es
                    .iter()
                    .filter(|e| e.key == "strategic_region")
                    .filter_map(|e| e.block())
                {
                    let Some(id) = atom(r, "id").and_then(|s| s.parse::<u32>().ok()) else {
                        w.diagnostics
                            .push(format!("{}: region without ID", path.display()));
                        continue;
                    };
                    let ps = block(r, "provinces")
                        .unwrap_or(&[])
                        .iter()
                        .filter_map(|e| e.key.parse::<u32>().ok())
                        .filter(|p| world.geography.provinces.contains_key(p))
                        .collect();
                    if w.air.regions.insert(id, ps).is_some() {
                        w.diagnostics
                            .push(format!("Duplicate strategic region {id}"));
                    }
                }
            }
            Err(e) => w.diagnostics.push(format!("{}: {e}", path.display())),
        }
    }
    // Country histories use the same dated/conditional resolver as land OOBs.
    for path in layered_files(&layers, "history/countries", false, "txt") {
        let Some(tag) = path
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|s| s.split([' ', '.']).next())
            .filter(|t| world.countries.contains_key(*t))
        else {
            continue;
        };
        let Ok(es) = fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|s| crate::pdx::parse_pdx_compatible(&s).map(|d| d.entries))
        else {
            continue;
        };
        let (resolved, _) =
            crate::history_conditions::resolve(&es, tag, &w.countries[tag].modifiers, today);
        for e in resolved.iter().filter(|e| e.key == "capital") {
            let capital = e
                .atom()
                .and_then(|s| s.parse::<u32>().ok())
                .and_then(|id| world.states.get(&id))
                .and_then(|s| {
                    s.province_ids
                        .iter()
                        .filter(|&&p| world.controller_of_province(p) == Some(tag))
                        .max_by(|&&a, &&b| {
                            w.conditions
                                .get(&a)
                                .map_or(0., |c| c.victory_points)
                                .total_cmp(&w.conditions.get(&b).map_or(0., |c| c.victory_points))
                                .then_with(|| b.cmp(&a))
                        })
                        .copied()
                });
            w.logistics.countries.entry(tag.into()).or_default().capital = capital;
        }
    }
    for path in layered_files(&layers, "history/states", false, "txt") {
        let Ok(es) = fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|s| parse_pdx(&s))
        else {
            continue;
        };
        let state = block(&es, "state").unwrap_or(&[]);
        let history = block(state, "history").unwrap_or(&[]);
        let (dated_history, diagnostics) = crate::history_conditions::resolve(
            history,
            "",
            &crate::military_model::MilitaryModifiers::default(),
            today,
        );
        w.diagnostics.extend(
            diagnostics
                .into_iter()
                .map(|d| format!("{}: airbase history: {d}", path.display())),
        );
        if let Some(id) = atom(state, "id").and_then(|s| s.parse::<u32>().ok()) {
            let oil = block(state, "resources")
                .map_or(0., |r| number(r, "oil", 0.))
                .max(0.);
            if oil > 0. {
                w.logistics.oil_by_state.insert(id, oil);
            }
        }
        let level = dated_history.iter()
            .filter(|e| e.key == "buildings")
            .filter_map(|e| e.block())
            .filter_map(|b| atom(b, "air_base"))
            .filter_map(|v| v.parse::<f64>().ok())
            .last()
            .unwrap_or(0.)
            .clamp(0.,100.) as u16;
        if level == 0 {
            continue;
        }
        if let Some(state) = atom(state, "id")
            .and_then(|s| s.parse::<u32>().ok())
            .and_then(|id| world.states.get(&id))
        {
            if let Some(&p) = state
                .province_ids
                .iter()
                .find(|&&p| world.owner_of_province(p) == state.owner.as_deref())
            {
                w.air.bases.insert(p, level);
            }
        }
    }
    for tag in world.countries.keys() {
        w.logistics.countries.entry(tag.clone()).or_default();
    }
    let unit_files = layered_files(&layers, "history/units", false, "txt");
    let selected: Vec<_> = w
        .countries
        .iter()
        .flat_map(|(tag, a)| {
            a.history_oobs
                .iter()
                .chain(a.history_air_oobs.iter())
                .map(move |oob| (tag.clone(), oob.clone()))
        })
        .collect();
    let mut imported_air_oobs = std::collections::BTreeSet::new();
    for (tag, oob) in selected {
        if !imported_air_oobs.insert((tag.clone(), oob.clone())) {
            continue;
        }
        if let Some(path) = unit_files
            .iter()
            .find(|p| p.file_stem().and_then(|s| s.to_str()) == Some(oob.as_str()))
        {
            match fs::read_to_string(path)
                .map_err(|e| e.to_string())
                .and_then(|s| crate::pdx::parse_pdx_compatible(&s).map(|d| d.entries))
            {
                Ok(es) => import_air_oob(&es, &tag, world, w),
                Err(e) => w
                    .diagnostics
                    .push(format!("{}: air OOB: {e}", path.display())),
            }
        }
    }
    let profile_path = root.join("military_support.json");
    if profile_path.exists() {
        match fs::read_to_string(&profile_path)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<Profile>(&s).map_err(|e| e.to_string()))
        {
            Ok(profile) if profile.schema == 1 => {
                let mut logistics = w.logistics.clone();
                let mut air = w.air.clone();
                if let Some(r) = profile.logistics_rules {
                    logistics.rules = r;
                }
                if let Some(r) = profile.air_rules {
                    air.rules = r;
                }
                for (tag, mut c) in profile.countries {
                    c.starting_fuel_explicit = true;
                    if c.capital.is_none() {
                        c.capital = logistics.countries.get(&tag).and_then(|c| c.capital);
                    }
                    logistics.countries.insert(tag, c);
                }
                logistics.motorized_hubs.extend(profile.motorized_hubs);
                logistics.blocked_sea.extend(profile.blocked_sea);
                air.bases.extend(profile.bases);
                let mut duplicate = None;
                for a in profile.wings {
                    air.next_id = air.next_id.max(a.id);
                    let id = a.id;
                    if air.wings.insert(id, a).is_some() {
                        duplicate = Some(id);
                    }
                }
                let result = logistics
                    .validate(world)
                    .and_then(|_| air.validate(world, w))
                    .and_then(|_| {
                        duplicate.map_or(Ok(()), |id| Err(format!("Duplicate air wing {id}")))
                    });
                match result {
                    Ok(()) => {
                        w.logistics = logistics;
                        w.air = air;
                    }
                    Err(e) => w
                        .diagnostics
                        .push(format!("military_support.json rejected: {e}")),
                }
            }
            Ok(_) => w
                .diagnostics
                .push("military_support.json: unsupported schema".into()),
            Err(e) => w
                .diagnostics
                .push(format!("military_support.json rejected: {e}")),
        }
    }
    for c in w.logistics.countries.values_mut() {
        c.fuel_capacity_base
            .get_or_insert(c.fuel_capacity.max(w.logistics.rules.base_fuel_capacity));
    }
    w.logistics.enabled = !world.geography.provinces.is_empty();
    w.air.enabled = true;
    w.air.reindex();
    for tag in world.countries.keys() {
        if w.logistics
            .countries
            .get(tag)
            .and_then(|c| c.capital)
            .is_none()
        {
            w.diagnostics.push(format!("{tag}: supply has no resolved capital; only local output and carried reserves are available"));
        }
    }
    w.diagnostics.push(format!("Logistics: {} rails, {} countries; air: {} regions, {} wings. Rules {} / {} are approximations, not verified 1.19.3 parity.",w.logistics.rails.len(),w.logistics.countries.len(),w.air.regions.len(),w.air.wings.len(),w.logistics.rules.profile,w.air.rules.profile));
}

/// Supported historical shape: air_wings = { province = { "Name" = {
/// equipment = { model = { amount = N owner = TAG } } } } }.
/// Mixed models split into separate runtime wings; unsupported variants are diagnosed.
pub(crate) fn import_air_oob(es: &[crate::pdx::Entry], tag: &str, world: &World, w: &mut Warfare) {
    let units = block(es, "units").unwrap_or(&[]);
    for group in es
        .iter()
        .chain(units)
        .filter(|e| e.key == "air_wings")
        .filter_map(|e| e.block())
    {
        for base in group {
            let Some(id) = base
                .key
                .parse::<u32>()
                .ok()
                .filter(|p| world.provinces.contains_key(p))
            else {
                w.diagnostics
                    .push(format!("{tag}: air OOB base {} is unresolved", base.key));
                continue;
            };
            // State-level air bases are placed at their selected OOB province.
            // Never add a free level: reuse the imported base of the same state.
            if !w.air.bases.contains_key(&id) && world.controller_of_province(id) == Some(tag) {
                if let Some(state) = world.state_for_province(id) {
                    if let Some((&representative, &level)) = w
                        .air
                        .bases
                        .iter()
                        .find(|(p, _)| state.province_ids.contains(p))
                    {
                        if !w.air.wings.values().any(|a| a.base == representative) {
                            w.air.bases.remove(&representative);
                        }
                        w.air.bases.insert(id, level);
                    }
                }
            }
            if w.air.bases.get(&id).copied().unwrap_or(0) == 0 {
                w.diagnostics.push(format!(
                    "{tag}: air wing base {id} has no imported air_base"
                ));
                continue;
            }
            for wing in base.block().unwrap_or(&[]) {
                let b = wing.block().unwrap_or(&[]);
                let equipment = block(b, "equipment").unwrap_or(&[]);
                if equipment.is_empty() {
                    w.diagnostics.push(format!(
                        "{tag}: air wing {} has unsupported equipment syntax",
                        wing.key
                    ));
                }
                for record in equipment {
                    let def = w.catalogue.equipment.get(&record.key);
                    let amount = record
                        .block()
                        .and_then(|b| atom(b, "amount"))
                        .and_then(|s| s.parse::<u32>().ok());
                    let owner = record.block().and_then(|b| atom(b, "owner"));
                    let named = record
                        .block()
                        .and_then(|b| atom(b, "version"))
                        .is_some_and(|s| s != "0");
                    if def.is_none_or(|e| e.modular || !crate::air_warfare::is_aircraft(e))
                        || amount.is_none()
                        || owner.is_some_and(|o| o != tag)
                        || named
                    {
                        w.diagnostics.push(format!(
                            "{tag}: unresolved air model/variant {} in {}",
                            record.key, wing.key
                        ));
                        continue;
                    }
                    w.air.next_id += 1;
                    let n = w.air.next_id;
                    w.air.wings.insert(
                        n,
                        AirWing {
                            id: n,
                            name: wing.key.clone(),
                            country: tag.into(),
                            model: record.key.clone(),
                            base: id,
                            planes: amount.unwrap(),
                            target_planes: amount.unwrap(),
                            mission: crate::air_warfare::AirMission::Standby,
                            region: None,
                            sorties: 0,
                            losses: 0,
                        },
                    );
                }
            }
        }
    }
}
