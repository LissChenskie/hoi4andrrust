//! Imported HOI4 naval-module schemas and validated, serializable blueprints.
//! The input comes only from the selected game data layers: no hard-coded
//! ship configurations, slots or research unlocks.
use crate::military_data::{atom, block, layered_files, stats};
use crate::military_model::{MilitaryCatalogue, MilitaryModifiers, StatBlock};
use crate::research::CountryResearch;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NavalSlot {
    pub required: bool,
    pub categories: BTreeSet<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NavalModule {
    pub category: String,
    pub active: bool,
    pub add: StatBlock,
    /// Average of values only from modules declaring this stat (including zero).
    #[serde(default)]
    pub average: StatBlock,
    /// Percentage modifiers combine additively before application.
    pub multiply: StatBlock,
    /// Other module categories which cannot coexist with this module.
    #[serde(default)]
    pub forbidden_categories: BTreeSet<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NavalHull {
    pub parent: Option<String>,
    pub slots: BTreeMap<String, NavalSlot>,
    /// References to slots of this hull, inherited hulls or later declarations.
    #[serde(default)]
    pub slot_aliases: BTreeMap<String, String>,
    pub default_modules: BTreeMap<String, String>,
    pub base_stats: StatBlock,
    pub base_resources: BTreeMap<String, u32>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NavalDesignerCatalogue {
    pub hulls: BTreeMap<String, NavalHull>,
    pub modules: BTreeMap<String, NavalModule>,
    pub diagnostics: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavalDesign {
    pub id: String,
    pub country: String,
    pub name: String,
    pub hull: String,
    pub modules: BTreeMap<String, String>,
    pub stats: StatBlock,
    pub resources: BTreeMap<String, u32>,
    pub cost: f64,
}
impl NavalDesignerCatalogue {
    fn expand_hull(&self, id: &str, seen: &mut BTreeSet<String>) -> Result<NavalHull, String> {
        if !seen.insert(id.into()) {
            return Err("Циклическое наследование корпуса".into());
        }
        let hull = self
            .hulls
            .get(id)
            .ok_or_else(|| format!("Корпус {id} отсутствует"))?;
        let mut base = if let Some(parent) = &hull.parent {
            self.expand_hull(parent, seen)?
        } else {
            NavalHull::default()
        };
        base.slots.extend(hull.slots.clone());
        base.slot_aliases.extend(hull.slot_aliases.clone());
        base.default_modules.extend(hull.default_modules.clone());
        base.base_stats.extend(hull.base_stats.clone());
        base.base_resources.extend(hull.base_resources.clone());
        seen.remove(id);
        Ok(base)
    }
    pub fn hull(&self, id: &str) -> Result<NavalHull, String> {
        let mut resolved = self.expand_hull(id, &mut BTreeSet::new())?;
        let mut pending = resolved.slot_aliases.clone();
        // An alias can point to an inherited or later-declared slot.
        while !pending.is_empty() {
            let before = pending.len();
            pending.retain(|alias, target| {
                if let Some(slot) = resolved.slots.get(target).cloned() {
                    resolved.slots.insert(alias.clone(), slot);
                    false
                } else {
                    true
                }
            });
            if pending.len() == before {
                return Err(format!("Не найдены слоты-источники: {:?}", pending));
            }
        }
        Ok(resolved)
    }
    pub fn allowed_modules(&self, hull: &str, slot: &str) -> Vec<String> {
        let Ok(spec) = self.hull(hull) else {
            return Vec::new();
        };
        let Some(s) = spec.slots.get(slot) else {
            return Vec::new();
        };
        self.modules
            .iter()
            .filter(|(_, m)| s.categories.contains(&m.category))
            .map(|(id, _)| id.clone())
            .collect()
    }
    /// Inherited hull slots, default modules and research gates are checked
    /// before calculating a single material/IC cost.
    pub fn construct(
        &self,
        country: &str,
        name: &str,
        hull: &str,
        chosen: &BTreeMap<String, String>,
        db: &MilitaryCatalogue,
        modifiers: &MilitaryModifiers,
        research: Option<&CountryResearch>,
    ) -> Result<NavalDesign, String> {
        let def = db.equipment.get(hull).ok_or("Неизвестный корпус")?;
        if !crate::naval_warfare::is_ship_model(hull, db)
            || !def.buildable
            || !(def.active || modifiers.unlocked_equipment.contains(hull))
        {
            return Err("Корпус не исследован".into());
        }
        let h = self.hull(hull)?;
        let mut combined = h.default_modules.clone();
        for (slot, module) in chosen {
            if !h.slots.contains_key(slot) {
                return Err(format!("Несуществующий слот: {slot}"));
            }
            combined.insert(slot.clone(), module.clone());
        }
        for (slot, _) in combined.iter() {
            if !h.slots.contains_key(slot) {
                return Err(format!("Неизвестный слот корпуса: {slot}"));
            }
        }
        let mut values = h.base_stats;
        let mut resources = h.base_resources;
        let mut multiply = StatBlock::new();
        let mut average_sum = StatBlock::new();
        let mut average_count = BTreeMap::<String, u32>::new();
        let mut placed = Vec::<(&str, &NavalModule)>::new();
        for (slot, rule) in &h.slots {
            let selected = combined.get(slot).map(String::as_str).unwrap_or("empty");
            if selected == "empty" || selected == "inherit" {
                if rule.required {
                    return Err(format!("Обязательный слот {slot} пуст"));
                }
                continue;
            }
            let module = self
                .modules
                .get(selected)
                .ok_or_else(|| format!("Модуль {selected} отсутствует"))?;
            if !rule.categories.contains(&module.category) {
                return Err(format!("Модуль {selected} не разрешён для {slot}"));
            }
            let is_default = h.default_modules.get(slot).is_some_and(|s| s == selected);
            let explicitly_unlocked = modifiers.unlocked_modules.contains(selected)
                || modifiers.unlocked_modules.contains(&module.category)
                || research.is_some_and(|r| {
                    r.unlocks
                        .contains(&format!("enable_equipment_modules:{selected}"))
                        || r.unlocks
                            .contains(&format!("enable_equipment_modules:{}", module.category))
                });
            if !module.active && !is_default && !explicitly_unlocked {
                return Err(format!("Модуль {selected} ещё не изучен"));
            }
            for (key, v) in &module.add {
                *values.entry(key.clone()).or_default() += v;
            }
            for (key, v) in &module.multiply {
                *multiply.entry(key.clone()).or_default() += v;
            }
            for (key, v) in &module.average {
                *average_sum.entry(key.clone()).or_default() += v;
                *average_count.entry(key.clone()).or_default() += 1;
            }
            placed.push((selected, module));
        }
        // Module exclusions are evaluated against the *complete* loadout,
        // independently of the order in which the slots were parsed.
        for (id, module) in &placed {
            if placed.iter().any(|(other_id, other)| {
                other_id != id && module.forbidden_categories.contains(&other.category)
            }) {
                return Err(format!("Модуль {id} конфликтует с другой категорией"));
            }
        }
        for (key, sum) in average_sum {
            if let Some(count) = average_count.get(&key) {
                *values.entry(key).or_default() += sum / f64::from(*count);
            }
        }
        // HOI4 combines percentage modifiers into (1 + Σ multipliers)
        // after collecting every module's additive and average contributions.
        for (key, bonus) in multiply {
            *values.entry(key).or_default() *= 1. + bonus;
        }
        if values.values().any(|v| !v.is_finite()) {
            return Err("Неверные характеристики проекта".into());
        }
        for key in ["reliability", "supply_consumption"] {
            if let Some(value) = values.get_mut(key) {
                *value = value.max(0.);
            }
        }
        let cost = values.get("build_cost_ic").copied().unwrap_or(def.cost).max(0.1);
        if !cost.is_finite() {
            return Err("Неверная стоимость проекта".into());
        }
        values.insert("build_cost_ic".into(), cost);
        // Resource costs have to be imported from the hull, not synthesized
        // from the number of modules.
        if resources.is_empty() {
            resources = db
                .equipment_resources
                .get(hull)
                .cloned()
                .unwrap_or_default();
        }
        Ok(NavalDesign {
            id: String::new(),
            country: country.into(),
            name: name.into(),
            hull: hull.into(),
            modules: combined,
            stats: values,
            resources,
            cost,
        })
    }
}

/// Read raw definitions *once at campaign load*. Modules and slots support
/// country overlays and ship-hull inheritance from the HOI4 equipment schema.
pub fn import_from_layers(root: &Path) -> NavalDesignerCatalogue {
    let layers = crate::military_data::data_layers(root);
    let mut result = NavalDesignerCatalogue::default();
    for path in layered_files(&layers, "common/units/equipment/modules", true, "txt") {
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(doc) = crate::pdx::parse_pdx_compatible(&content) else {
            result
                .diagnostics
                .push(format!("Invalid module data: {}", path.display()));
            continue;
        };
        let variables = doc
            .entries
            .iter()
            .filter(|e| e.key.starts_with('@'))
            .filter_map(|e| e.atom().map(|v| (e.key.clone(), v.to_string())))
            .collect();
        for node in doc
            .entries
            .iter()
            .filter(|e| e.key == "equipment_modules")
            .filter_map(|e| e.block())
            .flatten()
        {
            let Some(b) = node.block() else { continue };
            let category = atom(b, "category").unwrap_or("").to_owned();
            if category.is_empty() {
                result
                    .diagnostics
                    .push(format!("Module {} has no category", node.key));
                continue;
            }
            let m = NavalModule {
                category,
                active: atom(b, "active") == Some("yes"),
                add: block(b, "add_stats")
                    .map(|v| stats(v, &variables))
                    .unwrap_or_default(),
                average: block(b, "add_average_stats")
                    .map(|v| stats(v, &variables))
                    .unwrap_or_default(),
                multiply: block(b, "multiply_stats")
                    .map(|v| stats(v, &variables))
                    .unwrap_or_default(),
                forbidden_categories: block(b, "forbid_module_categories")
                    .unwrap_or(&[])
                    .iter()
                    .map(|item| item.key.clone())
                    .collect(),
            };
            result.modules.insert(node.key.clone(), m);
        }
    }
    for path in layered_files(&layers, "common/units/equipment", false, "txt") {
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(doc) = crate::pdx::parse_pdx_compatible(&content) else {
            continue;
        };
        let variables = doc
            .entries
            .iter()
            .filter(|e| e.key.starts_with('@'))
            .filter_map(|e| e.atom().map(|v| (e.key.clone(), v.to_string())))
            .collect();
        for node in doc
            .entries
            .iter()
            .filter(|e| e.key == "equipments")
            .filter_map(|e| e.block())
            .flatten()
        {
            if !node.key.starts_with("ship_hull_") {
                continue;
            }
            let Some(b) = node.block() else { continue };
            let mut hull = NavalHull {
                parent: atom(b, "archetype").map(str::to_owned),
                base_stats: stats(b, &variables),
                ..Default::default()
            };
            if let Some(slots) = block(b, "module_slots") {
                for entry in slots {
                    if let Some(data) = entry.block() {
                        hull.slots.insert(
                            entry.key.clone(),
                            NavalSlot {
                                required: atom(data, "required") == Some("yes"),
                                categories: block(data, "allowed_module_categories")
                                    .unwrap_or(&[])
                                    .iter()
                                    .map(|e| e.key.clone())
                                    .collect(),
                            },
                        );
                    } else if let Some(other) = entry.atom() {
                        if other != "inherit" {
                            hull.slot_aliases.insert(entry.key.clone(), other.into());
                        }
                    }
                }
            }
            if let Some(defaults) = block(b, "default_modules") {
                hull.default_modules = defaults
                    .iter()
                    .filter_map(|e| e.atom().map(|v| (e.key.clone(), v.to_owned())))
                    .collect();
            }
            if let Some(raw) = block(b, "resources") {
                for resource in raw {
                    if let Some(amount) = resource.atom().and_then(|v| v.parse::<u32>().ok()) {
                        hull.base_resources.insert(resource.key.clone(), amount);
                    }
                }
            }
            result.hulls.insert(node.key.clone(), hull);
        }
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::military_model::{EquipmentDefinition, MilitaryModifiers};
    #[test]
    fn required_slots_categories_and_unlocks_control_blueprints() {
        let mut data = NavalDesignerCatalogue::default();
        data.modules.insert(
            "engine".into(),
            NavalModule {
                category: "engine_type".into(),
                active: true,
                add: StatBlock::from([("build_cost_ic".into(), 50.)]),
                ..Default::default()
            },
        );
        data.modules.insert(
            "advanced".into(),
            NavalModule {
                category: "engine_type".into(),
                active: false,
                add: StatBlock::from([("build_cost_ic".into(), 100.)]),
                ..Default::default()
            },
        );
        data.modules.insert(
            "illegal".into(),
            NavalModule {
                category: "armor_type".into(),
                active: true,
                ..Default::default()
            },
        );
        data.hulls.insert(
            "ship_hull_light_1".into(),
            NavalHull {
                base_stats: StatBlock::from([("build_cost_ic".into(), 100.)]),
                slots: BTreeMap::from([(
                    "fixed_engine".into(),
                    NavalSlot {
                        required: true,
                        categories: BTreeSet::from(["engine_type".into()]),
                    },
                )]),
                ..Default::default()
            },
        );
        let mut db = MilitaryCatalogue::default();
        db.equipment.insert(
            "ship_hull_light_1".into(),
            EquipmentDefinition {
                id: "ship_hull_light_1".into(),
                archetype: "ship_hull_light".into(),
                year: 1936,
                active: true,
                buildable: true,
                stats: StatBlock::new(),
                cost: 100.,
                modular: false,
                source: "test".into(),
            },
        );
        let mods = MilitaryModifiers::default();
        assert!(
            data.construct(
                "A",
                "X",
                "ship_hull_light_1",
                &BTreeMap::new(),
                &db,
                &mods,
                None
            )
            .is_err()
        );
        let engine = BTreeMap::from([("fixed_engine".into(), "engine".into())]);
        assert_eq!(
            data.construct("A", "X", "ship_hull_light_1", &engine, &db, &mods, None)
                .unwrap()
                .cost,
            150.
        );
        let illegal = BTreeMap::from([("fixed_engine".into(), "illegal".into())]);
        assert!(
            data.construct("A", "X", "ship_hull_light_1", &illegal, &db, &mods, None)
                .is_err()
        );
        let blocked = BTreeMap::from([("fixed_engine".into(), "advanced".into())]);
        assert!(
            data.construct("A", "X", "ship_hull_light_1", &blocked, &db, &mods, None)
                .is_err()
        );
        let mut tech = CountryResearch::new(1, &crate::research::ResearchRules::default());
        tech.unlocks
            .insert("enable_equipment_modules:advanced".into());
        assert_eq!(
            data.construct(
                "A",
                "X",
                "ship_hull_light_1",
                &blocked,
                &db,
                &mods,
                Some(&tech)
            )
            .unwrap()
            .cost,
            200.
        );
    }
    fn fixture_equipment() -> MilitaryCatalogue {
        let mut db = MilitaryCatalogue::default();
        db.equipment.insert(
            "ship_hull_light_1".into(),
            EquipmentDefinition {
                id: "ship_hull_light_1".into(),
                archetype: "ship_hull_light".into(),
                year: 1936,
                active: true,
                buildable: true,
                stats: StatBlock::new(),
                cost: 100.,
                modular: true,
                source: "synthetic-test".into(),
            },
        );
        db
    }

    #[test]
    fn multiplier_is_added_once_after_all_flat_and_average_stats() {
        let mut data = NavalDesignerCatalogue::default();
        data.hulls.insert(
            "ship_hull_light_1".into(),
            NavalHull {
                base_stats: StatBlock::from([
                    ("build_cost_ic".into(), 100.),
                    ("reliability".into(), -1.),
                ]),
                slots: BTreeMap::from([
                    ("fixed_engine".into(), NavalSlot {
                        required: true,
                        categories: BTreeSet::from(["engine".into()]),
                    }),
                    ("custom_engine".into(), NavalSlot {
                        required: true,
                        categories: BTreeSet::from(["engine".into()]),
                    }),
                ]),
                ..Default::default()
            },
        );
        data.modules.insert(
            "a".into(),
            NavalModule {
                category: "engine".into(),
                active: true,
                add: StatBlock::from([("build_cost_ic".into(), 20.)]),
                average: StatBlock::from([("deck_size".into(), 10.)]),
                multiply: StatBlock::from([("build_cost_ic".into(), 0.1)]),
                ..Default::default()
            },
        );
        data.modules.insert(
            "b".into(),
            NavalModule {
                category: "engine".into(),
                active: true,
                average: StatBlock::from([("deck_size".into(), 0.)]),
                multiply: StatBlock::from([("build_cost_ic".into(), 0.1)]),
                ..Default::default()
            },
        );
        let mods = MilitaryModifiers::default();
        let variant = data.construct(
            "A", "average", "ship_hull_light_1",
            &BTreeMap::from([
                ("fixed_engine".into(), "a".into()),
                ("custom_engine".into(), "b".into()),
            ]),
            &fixture_equipment(), &mods, None,
        ).unwrap();
        // (100 + 20) * (1 + 0.1 + 0.1) = 144, not compounded 145.2.
        assert!((variant.cost - 144.).abs() < 1e-8);
        // Declared zero counts towards the average.
        assert_eq!(variant.stats["deck_size"], 5.);
        assert_eq!(variant.stats["reliability"], 0.);
    }

    #[test]
    fn module_exclusions_reject_conflicting_categories() {
        let mut data = NavalDesignerCatalogue::default();
        data.hulls.insert(
            "ship_hull_light_1".into(),
            NavalHull {
                base_stats: StatBlock::from([("build_cost_ic".into(), 100.)]),
                slots: BTreeMap::from([
                    ("engine".into(), NavalSlot {
                        required: true, categories: BTreeSet::from(["engine".into()]),
                    }),
                    ("armor".into(), NavalSlot {
                        required: true, categories: BTreeSet::from(["armor".into()]),
                    }),
                ]),
                ..Default::default()
            },
        );
        data.modules.insert("main_engine".into(), NavalModule {
            category: "engine".into(),
            active: true,
            forbidden_categories: BTreeSet::from(["armor".into()]),
            ..Default::default()
        });
        data.modules.insert("armor".into(), NavalModule {
            category: "armor".into(),
            active: true,
            ..Default::default()
        });
        let error = data.construct(
            "A", "forbidden", "ship_hull_light_1",
            &BTreeMap::from([
                ("engine".into(), "main_engine".into()),
                ("armor".into(), "armor".into()),
            ]),
            &fixture_equipment(), &MilitaryModifiers::default(), None,
        ).unwrap_err();
        assert!(error.contains("конфликтует"), "{error}");
    }

    #[test]
    fn slot_aliases_resolve_inherited_and_forward_references() {
        let mut data = NavalDesignerCatalogue::default();
        data.hulls.insert("ship_hull_light".into(), NavalHull {
            slots: BTreeMap::from([("parent_slot".into(), NavalSlot {
                required: true,
                categories: BTreeSet::from(["engine".into()]),
            })]),
            ..Default::default()
        });
        data.hulls.insert("ship_hull_light_1".into(), NavalHull {
            parent: Some("ship_hull_light".into()),
            slot_aliases: BTreeMap::from([
                ("first".into(), "second".into()),
                ("second".into(), "parent_slot".into()),
            ]),
            ..Default::default()
        });
        let resolved = data.hull("ship_hull_light_1").unwrap();
        assert!(resolved.slots["first"].required);
        assert_eq!(resolved.slots["first"].categories,
                   BTreeSet::from(["engine".to_owned()]));
        data.hulls.get_mut("ship_hull_light_1").unwrap().slot_aliases
            .insert("second".into(), "unknown_slot".into());
        assert!(data.hull("ship_hull_light_1").is_err());
    }

}
