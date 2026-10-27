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
    pub multiply: StatBlock,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NavalHull {
    pub parent: Option<String>,
    pub slots: BTreeMap<String, NavalSlot>,
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
        base.default_modules.extend(hull.default_modules.clone());
        base.base_stats.extend(hull.base_stats.clone());
        base.base_resources.extend(hull.base_resources.clone());
        seen.remove(id);
        Ok(base)
    }
    pub fn hull(&self, id: &str) -> Result<NavalHull, String> {
        self.expand_hull(id, &mut BTreeSet::new())
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
            let explicitly_unlocked = research.is_some_and(|r| {
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
                let base = values.entry(key.clone()).or_default();
                *base *= (1. + v).max(0.);
            }
        }
        let cost = values.get("build_cost_ic").copied().unwrap_or(def.cost);
        if !cost.is_finite() || cost <= 0. || values.values().any(|v| !v.is_finite()) {
            return Err("Неверная стоимость или характеристики проекта".into());
        }
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
                multiply: block(b, "multiply_stats")
                    .map(|v| stats(v, &variables))
                    .unwrap_or_default(),
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
                            if let Some(existing) = hull.slots.get(other).cloned() {
                                hull.slots.insert(entry.key.clone(), existing);
                            } else {
                                result.diagnostics.push(format!(
                                    "Unresolved slot alias {} -> {}",
                                    entry.key, other
                                ));
                            }
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
}
