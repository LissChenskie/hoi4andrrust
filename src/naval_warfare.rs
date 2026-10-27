//! Data-driven naval task forces and trade-interdiction simulation.
//! This is an explicitly provisional naval layer, NOT verified HOI4 1.19.3
//! combat RNG, positioning, targeting, spotting or convoy-loss parity.
//! Authoritative hulls/techs come from the user's imported game files.
use crate::military_model::{MilitaryCatalogue, MilitaryModifiers};
use crate::warfare::Warfare;
use crate::world::World;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// A JSON-safe, ordered representation of non-string map keys. An empty
/// JSON object produced by older saves remains readable during migration.
pub mod save_pairs {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;
    pub fn serialize<K: Ord + Serialize, V: Serialize, S: Serializer>(
        value: &BTreeMap<K, V>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.iter().collect::<Vec<_>>().serialize(serializer)
    }
    pub fn deserialize<
        'de,
        K: Ord + Deserialize<'de>,
        V: Deserialize<'de>,
        D: Deserializer<'de>,
    >(
        deserializer: D,
    ) -> Result<BTreeMap<K, V>, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Representation<K, V> {
            Pairs(Vec<(K, V)>),
            OldEmpty(BTreeMap<String, V>),
        }
        match Representation::<K, V>::deserialize(deserializer)? {
            Representation::Pairs(pairs) => Ok(pairs.into_iter().collect()),
            Representation::OldEmpty(m) if m.is_empty() => Ok(BTreeMap::new()),
            Representation::OldEmpty(_) => Err(serde::de::Error::custom(
                "Legacy JSON object cannot contain compound-key map entries",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavalMission {
    #[default]
    Hold,
    Patrol,
    StrikeForce,
    ConvoyRaiding,
    ConvoyEscort,
}
impl NavalMission {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hold => "Ожидание",
            Self::Patrol => "Патрулирование",
            Self::StrikeForce => "Ударное соединение",
            Self::ConvoyRaiding => "Рейдерство",
            Self::ConvoyEscort => "Сопровождение",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Hold => Self::Patrol,
            Self::Patrol => Self::StrikeForce,
            Self::StrikeForce => Self::ConvoyRaiding,
            Self::ConvoyRaiding => Self::ConvoyEscort,
            Self::ConvoyEscort => Self::Hold,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavalShip {
    pub name: String,
    pub model: String,
    pub definition: String,
    pub variant: Option<String>,
    pub strength: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavalTaskForce {
    pub id: u64,
    pub country: String,
    pub name: String,
    /// Controlled province with historical naval-base buildings.
    pub base: u32,
    pub mission: NavalMission,
    /// Strategic sea-region IDs imported from map/strategicregions.
    pub regions: BTreeSet<u32>,
    pub ships: Vec<NavalShip>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct NavalRules {
    /// Relative cargo exposure per raiding-stat point, pending version validation.
    pub raid_exposure: f64,
    /// Escort counter-detection effectiveness, pending version validation.
    pub escort_coverage: f64,
    /// Maximum fraction of committed ships lost in a single daily encounter.
    pub daily_loss_cap: f64,
    /// IC output of one naval dockyard/day (overridable from imported defines).
    pub dockyard_output: f64,
    pub dockyards_per_line: u32,
}
impl Default for NavalRules {
    fn default() -> Self {
        Self {
            raid_exposure: 0.035,
            escort_coverage: 0.06,
            daily_loss_cap: 0.35,
            dockyard_output: 2.5,
            dockyards_per_line: 5,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavalConstruction {
    pub id: u64,
    pub country: String,
    pub model: String,
    pub base: u32,
    pub requested_dockyards: u32,
    pub partial_ic: f64,
    pub completed: u64,
    pub last_day_ic: f64,
}
/// Country-owned commander imported from the scenario's historical history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavalAdmiral {
    pub country: String,
    pub name: String,
    pub skill: u32,
    pub attack: u32,
    pub defense: u32,
    pub maneuvering: u32,
    pub coordination: u32,
    pub traits: BTreeSet<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NavalState {
    pub task_forces: BTreeMap<u64, NavalTaskForce>,
    /// Country and name uniquely scope imported commanders.
    #[serde(default, with = "save_pairs")]
    pub admirals: BTreeMap<(String, String), NavalAdmiral>,
    /// Each task force can carry one commander; a commander cannot lead two.
    pub assigned_admirals: BTreeMap<u64, String>,
    pub next_id: u64,
    pub shipbuilding: BTreeMap<String, Vec<NavalConstruction>>,
    pub next_construction_id: u64,
    /// Country-owned immutable blueprints referenced by ship variants.
    pub designs: BTreeMap<String, crate::naval_designer::NavalDesign>,
    pub next_design_id: u64,
    pub rules: NavalRules,
    /// Fractional expected losses carried forward to preserve determinism.
    #[serde(default, with = "save_pairs")]
    pub loss_progress: BTreeMap<(String, String, String), f64>,
    pub last_events: Vec<ConvoyEvent>,
    pub diagnostics: Vec<String>,
    /// Persistent spotted-task-force contacts, bounded by assigned regions.
    #[serde(default, with = "save_pairs")]
    pub spotting: BTreeMap<(u32, u64, u64), f64>,
    #[serde(default, with = "save_pairs")]
    pub last_contact_battle: BTreeMap<(u32, u64, u64), u64>,
    #[serde(default, with = "save_pairs")]
    pub conditions: BTreeMap<(u64, usize), crate::naval_combat::ShipCondition>,
    pub battle_reports: Vec<crate::naval_combat::NavalBattleReport>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvoyEvent {
    pub buyer: String,
    pub seller: String,
    pub resource: String,
    pub convoy_losses: u64,
    pub delivered_after: u32,
    pub region: u32,
    pub raider_country: String,
}

/// A naval model is not any generic equipment with naval-looking statistics.
/// Resolve from the imported archetype or hull ID, never invent a hull.
pub fn is_ship_model(model: &str, catalogue: &MilitaryCatalogue) -> bool {
    catalogue.equipment.get(model).is_some_and(|e| {
        let a = e.archetype.to_ascii_lowercase();
        let id = e.id.to_ascii_lowercase();
        [
            "ship_hull_",
            "destroyer",
            "submarine",
            "cruiser",
            "battleship",
            "battlecruiser",
            "carrier",
        ]
        .iter()
        .any(|p| a.contains(p) || id.contains(p))
            && !a.contains("convoy")
            && !id.contains("convoy")
    })
}

impl NavalState {
    pub fn assign_admiral(&mut self, tag: &str, tf: u64, name: Option<&str>) -> Result<(), String> {
        if !self.task_forces.get(&tf).is_some_and(|t| t.country == tag) {
            return Err("Нельзя назначить адмирала чужому флоту".into());
        }
        if let Some(name) = name {
            if !self.admirals.contains_key(&(tag.into(), name.into())) {
                return Err("Адмирал не найден в истории этой страны".into());
            }
            if self.assigned_admirals.iter().any(|(&id, current)| {
                id != tf
                    && current == name
                    && self.task_forces.get(&id).is_some_and(|f| f.country == tag)
            }) {
                return Err("Этот адмирал уже руководит другим соединением".into());
            }
            self.assigned_admirals.insert(tf, name.into());
        } else {
            self.assigned_admirals.remove(&tf);
        }
        Ok(())
    }
    pub fn admiral_for(&self, id: u64) -> Option<&NavalAdmiral> {
        let tf = self.task_forces.get(&id)?;
        let name = self.assigned_admirals.get(&id)?;
        self.admirals.get(&(tf.country.clone(), name.clone()))
    }
    pub fn add_historical(
        &mut self,
        tag: &str,
        name: &str,
        base: u32,
        ships: Vec<NavalShip>,
        world: &World,
        warfare: &Warfare,
    ) -> Result<u64, String> {
        if !world.countries.contains_key(tag)
            || world.controller_of_province(base) != Some(tag)
            || warfare
                .conditions
                .get(&base)
                .is_none_or(|c| c.naval_base == 0)
            || ships.is_empty()
        {
            return Err(format!(
                "{tag}: invalid historical naval base or empty task force"
            ));
        }
        for ship in &ships {
            if !is_ship_model(&ship.model, &warfare.catalogue)
                || !ship.strength.is_finite()
                || !(0.0..=1.0).contains(&ship.strength)
            {
                return Err(format!(
                    "{tag}: unknown or invalid ship model {}",
                    ship.model
                ));
            }
        }
        self.next_id = self.next_id.saturating_add(1);
        let id = self.next_id;
        self.task_forces.insert(
            id,
            NavalTaskForce {
                id,
                country: tag.into(),
                name: name.into(),
                base,
                mission: NavalMission::Hold,
                regions: BTreeSet::new(),
                ships,
            },
        );
        Ok(id)
    }

    /// New construction/recruitment MUST be gated by actual research unlocks;
    /// historically scripted ships can exist before hulls become buildable.
    pub fn can_build_ship(
        catalogue: &MilitaryCatalogue,
        model: &str,
        country: &MilitaryModifiers,
    ) -> bool {
        is_ship_model(model, catalogue) && catalogue.equipment_available(model, country)
    }

    /// Queue only a researched and buildable ship model. Historical ships
    /// remain separate from a country's ability to construct new ones.
    pub fn queue_ship(
        &mut self,
        tag: &str,
        model: &str,
        base: u32,
        dockyards: u32,
        world: &World,
        warfare: &Warfare,
    ) -> Result<u64, String> {
        let modifiers = warfare
            .countries
            .get(tag)
            .map(|c| &c.modifiers)
            .ok_or("Неизвестная страна")?;
        if !Self::can_build_ship(&warfare.catalogue, model, modifiers) {
            return Err("Нужна исследованная и допустимая конструкция корабля".into());
        }
        if world.controller_of_province(base) != Some(tag)
            || warfare
                .conditions
                .get(&base)
                .is_none_or(|c| c.naval_base == 0)
        {
            return Err("Для выпуска нужен действующий собственный морской порт".into());
        }
        if dockyards == 0
            || warfare
                .catalogue
                .equipment
                .get(model)
                .is_none_or(|e| !e.cost.is_finite() || e.cost <= 0.0)
        {
            return Err("Недопустимое количество верфей или стоимость корабля".into());
        }
        self.next_construction_id = self.next_construction_id.saturating_add(1);
        let id = self.next_construction_id;
        self.shipbuilding
            .entry(tag.into())
            .or_default()
            .push(NavalConstruction {
                id,
                country: tag.into(),
                model: model.into(),
                base,
                requested_dockyards: dockyards.min(self.rules.dockyards_per_line.max(1)),
                partial_ic: 0.0,
                completed: 0,
                last_day_ic: 0.0,
            });
        Ok(id)
    }

    pub fn save_design(
        &mut self, tag: &str, name: &str, hull: &str,
        modules: &BTreeMap<String,String>, w: &Warfare,
        research: Option<&crate::research::CountryResearch>,
    ) -> Result<String,String> {
        let modifiers=&w.countries.get(tag).ok_or("Неизвестная страна")?.modifiers;
        let mut design=w.catalogue.naval_designer.construct(
            tag,name,hull,modules,&w.catalogue,modifiers,research
        )?;
        self.next_design_id=self.next_design_id.saturating_add(1);
        let id=format!("{tag}_naval_design_{}",self.next_design_id);
        design.id=id.clone();
        self.designs.insert(id.clone(),design);
        Ok(id)
    }
    pub fn queue_design(
        &mut self, tag:&str,design_id:&str,base:u32,dockyards:u32,
        world:&World,w:&Warfare,
    )->Result<u64,String> {
        let design=self.designs.get(design_id).ok_or("Проект не найден")?;
        if design.country != tag {return Err("Проект принадлежит другой стране".into());}
        if !Self::can_build_ship(&w.catalogue,&design.hull,&w.countries.get(tag)
            .ok_or("Страна отсутствует")?.modifiers) {return Err("Корпус ещё не исследован".into());}
        if dockyards==0 || world.controller_of_province(base)!=Some(tag)
            || w.conditions.get(&base).is_none_or(|c|c.naval_base==0)
        {return Err("Для постройки нужна доступная морская база".into());}
        self.next_construction_id=self.next_construction_id.saturating_add(1);
        let id=self.next_construction_id;
        self.shipbuilding.entry(tag.into()).or_default().push(NavalConstruction {
            id,country:tag.into(),model:design_id.into(),base,
            requested_dockyards:dockyards.min(self.rules.dockyards_per_line.max(1)),
            partial_ic:0.,completed:0,last_day_ic:0.,
        });
        Ok(id)
    }
    pub fn ship_stats<'a>(
        &'a self, ship: &crate::naval_warfare::NavalShip,
        w: &'a Warfare,
    ) -> Option<&'a crate::military_model::StatBlock> {
        if let Some(variant)=ship.variant.as_ref() {
            if let Some(design)=self.designs.get(variant) {
                if design.hull==ship.model {return Some(&design.stats);}
            }
        }
        w.catalogue.equipment.get(&ship.model).map(|e|&e.stats)
    }
    pub fn cancel_ship(&mut self, tag: &str, id: u64) -> bool {
        let Some(lines) = self.shipbuilding.get_mut(tag) else {
            return false;
        };
        let before = lines.len();
        lines.retain(|l| l.id != id);
        before != lines.len()
    }

    /// Daily dockyard production draws from the same finite strategic-resource
    /// pool as military factories. A ship is commissioned only after accumulating
    /// the imported model's IC cost and only while the technology remains usable.
    pub fn tick_shipbuilding(
        &mut self,
        tag: &str,
        dockyards: u32,
        world: &World,
        warfare: &Warfare,
        pool: &mut BTreeMap<String, u32>,
        shortage_penalty: f64,
    ) {
        let Some(lines) = self.shipbuilding.get_mut(tag) else {
            return;
        };
        let Some(modifiers) = warfare.countries.get(tag).map(|c| &c.modifiers) else {
            return;
        };
        let mut free = dockyards;
        let mut ships = Vec::new();
        for line in lines {
            line.last_day_ic = 0.0;
            let blueprint=self.designs.get(&line.model);
            let model=blueprint.map_or(line.model.as_str(),|d|d.hull.as_str());
            if blueprint.is_some_and(|d|d.country!=tag)
                || !Self::can_build_ship(&warfare.catalogue, model, modifiers)
                || world.controller_of_province(line.base) != Some(tag)
                || warfare
                    .conditions
                    .get(&line.base)
                    .is_none_or(|c| c.naval_base == 0)
            {
                continue;
            }
            let Some(def) = warfare.catalogue.equipment.get(model) else {
                continue;
            };
            let cost=blueprint.map_or(def.cost,|d|d.cost);
            if !cost.is_finite() || cost <= 0.0 { continue; }
            let assigned = free.min(
                line.requested_dockyards
                    .min(self.rules.dockyards_per_line.max(1)),
            );
            free -= assigned;
            let needs=if let Some(design)=blueprint {
                Some(&design.resources)
            } else {warfare.catalogue.equipment_resources.get(&line.model)};
            let mut ic = 0.0;
            for _ in 0..assigned {
                let mut maximum_missing = 0_u32;
                if let Some(needs) = needs {
                    for (resource, &needed) in needs {
                        let available = pool.entry(resource.clone()).or_default();
                        maximum_missing = maximum_missing.max(needed.saturating_sub(*available));
                        *available = available.saturating_sub(needed);
                    }
                }
                ic += self.rules.dockyard_output.max(0.0)
                    * (1.0 - f64::from(maximum_missing) * shortage_penalty).clamp(0.0, 1.0);
            }
            line.last_day_ic = ic;
            line.partial_ic += ic;
            let count = (line.partial_ic / cost).floor().clamp(0.0, 1000.0) as u32;
            line.partial_ic -= f64::from(count) * cost;
            line.completed = line.completed.saturating_add(u64::from(count));
            for _ in 0..count {
                ships.push((model.to_owned(), blueprint.map(|d|d.id.clone()),line.base));
            }
        }
        for (model, variant, base) in ships {
            let existing = self.task_forces.values_mut().find(|t| {
                t.country == tag
                    && t.base == base
                    && t.name == "Новые корабли"
                    && t.mission == NavalMission::Hold
            });
            let name = format!("{} №{}", model, self.next_id.saturating_add(1));
            let ship = NavalShip {
                name,
                model,
                definition: warfare.catalogue.equipment.get(&model)
                    .map_or_else(String::new,|d|d.archetype.clone()),
                variant,
                strength: 1.0,
            };
            if let Some(force) = existing {
                force.ships.push(ship);
            } else {
                self.next_id = self.next_id.saturating_add(1);
                let id = self.next_id;
                self.task_forces.insert(
                    id,
                    NavalTaskForce {
                        id,
                        country: tag.into(),
                        name: "Новые корабли".into(),
                        base,
                        mission: NavalMission::Hold,
                        regions: BTreeSet::new(),
                        ships: vec![ship],
                    },
                );
            }
        }
    }

    pub fn assign_mission(
        &mut self,
        tag: &str,
        id: u64,
        mission: NavalMission,
        region: u32,
        world: &World,
        warfare: &Warfare,
    ) -> Result<(), String> {
        let force = self.task_forces.get(&id).ok_or("Флот не найден")?;
        if force.country != tag {
            return Err("Невозможно командовать чужим флотом".into());
        }
        if world.controller_of_province(force.base) != Some(tag)
            || warfare
                .conditions
                .get(&force.base)
                .is_none_or(|b| b.naval_base == 0)
        {
            return Err("Морская база недоступна".into());
        }
        if mission != NavalMission::Hold {
            let sea = warfare
                .air
                .regions
                .get(&region)
                .ok_or("Неизвестный морской регион")?;
            if !sea.iter().any(|p| {
                world
                    .geography
                    .provinces
                    .get(p)
                    .is_some_and(|g| g.definition.kind == crate::map_graph::ProvinceKind::Sea)
            }) {
                return Err("Регион не содержит моря".into());
            }
        }
        let force = self.task_forces.get_mut(&id).unwrap();
        force.mission = mission;
        force.regions = if mission == NavalMission::Hold {
            BTreeSet::new()
        } else {
            BTreeSet::from([region])
        };
        Ok(())
    }

    fn strength_in(
        &self,
        w: &Warfare,
        ship_country: &str,
        mission: NavalMission,
        region: u32,
    ) -> f64 {
        self.task_forces
            .values()
            .filter(|f| {
                f.country == ship_country && f.mission == mission && f.regions.contains(&region)
            })
            .flat_map(|f| f.ships.iter())
            .filter_map(|s| {
                let e = w.catalogue.equipment.get(&s.model)?;
                let stat = match mission {
                    NavalMission::ConvoyRaiding => {
                        e.stats.get("torpedo_attack").copied().unwrap_or(0.0)
                            + e.stats.get("surface_detection").copied().unwrap_or(0.0) * 0.1
                    }
                    NavalMission::ConvoyEscort => {
                        e.stats.get("sub_detection").copied().unwrap_or(0.0)
                            + e.stats.get("surface_detection").copied().unwrap_or(0.0) * 0.3
                    }
                    NavalMission::Patrol | NavalMission::StrikeForce => {
                        e.stats.get("surface_detection").copied().unwrap_or(0.0)
                    }
                    NavalMission::Hold => 0.0,
                };
                Some(stat.max(0.0) * s.strength.clamp(0.0, 1.0))
            })
            .sum()
    }

    /// Resolve strictly abstract campaign convoy attrition. Does NOT simulate
    /// surface engagements, tactical naval combat, repairs or aircraft strikes.
    pub fn process_trade(
        &mut self,
        world: &World,
        warfare: &Warfare,
        shipping: &mut crate::resource_economy::ResourceEconomy,
        convoy_losses: &mut BTreeMap<String, u64>,
    ) {
        self.last_events.clear();
        let all_sea_regions: BTreeMap<u32, Vec<u32>> = warfare
            .air
            .regions
            .iter()
            .map(|(&region, provinces)| {
                (
                    region,
                    provinces
                        .iter()
                        .copied()
                        .filter(|id| {
                            world.geography.provinces.get(id).is_some_and(|p| {
                                p.definition.kind == crate::map_graph::ProvinceKind::Sea
                            })
                        })
                        .collect(),
                )
            })
            .collect();
        let mut remaining_convoys: BTreeMap<String, u64> = warfare
            .logistics
            .countries
            .iter()
            .map(|(tag, c)| (tag.clone(), c.convoys))
            .collect();
        for shipment in &mut shipping.last_shipments {
            let crate::trade_routes::TradeRoute::Sea { sea_provinces, .. } = &shipment.route else {
                continue;
            };
            if shipment.assigned_convoys == 0 || shipment.delivered == 0 {
                continue;
            }
            let crossed = all_sea_regions
                .iter()
                .filter_map(|(&r, ps)| {
                    if ps.iter().any(|p| sea_provinces.contains(p)) {
                        Some(r)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let mut strongest: Option<(f64, u32, String)> = None;
            for region in crossed {
                for raider in warfare.countries.keys() {
                    if !warfare.at_war(raider, &shipment.buyer) {
                        continue;
                    }
                    let strike =
                        self.strength_in(warfare, raider, NavalMission::ConvoyRaiding, region);
                    if strike <= 0.0 {
                        continue;
                    }
                    let escort = self.strength_in(
                        warfare,
                        &shipment.buyer,
                        NavalMission::ConvoyEscort,
                        region,
                    );
                    // Detected raiders are increasingly ineffective against
                    // covered cargo; rates are versioned, currently provisional.
                    let exposure = (strike * self.rules.raid_exposure
                        / (1.0 + escort * self.rules.escort_coverage))
                        .clamp(0.0, self.rules.daily_loss_cap);
                    if strongest.as_ref().is_none_or(|(best, ..)| exposure > *best) {
                        strongest = Some((exposure, region, raider.clone()));
                    }
                }
            }
            let Some((rate, region, raider_country)) = strongest else {
                continue;
            };
            let key = (
                shipment.buyer.clone(),
                shipment.seller.clone(),
                shipment.resource.clone(),
            );
            let debt = self.loss_progress.entry(key).or_default();
            *debt += rate * shipment.assigned_convoys as f64;
            let losses = (*debt)
                .floor()
                .max(0.0)
                .min(shipment.assigned_convoys as f64) as u64;
            *debt -= losses as f64;
            if losses == 0 {
                continue;
            }
            let budget = remaining_convoys.entry(shipment.buyer.clone()).or_default();
            let losses = losses.min(*budget);
            *budget -= losses;
            *convoy_losses.entry(shipment.buyer.clone()).or_default() += losses;
            let initial = shipment.delivered;
            shipment.assigned_convoys -= losses;
            shipment.delivered = if shipment.required_convoys > 0 {
                (u64::from(shipment.contracted) * shipment.assigned_convoys
                    / shipment.required_convoys) as u32
            } else {
                initial
            };
            shipment.delivered = shipment.delivered.min(initial);
            let undelivered = initial - shipment.delivered;
            if let Some(report) = shipping.last.get_mut(&shipment.buyer) {
                if let Some(n) = report.imported.get_mut(&shipment.resource) {
                    *n = n.saturating_sub(undelivered);
                }
                if let Some(n) = report.available.get_mut(&shipment.resource) {
                    *n = n.saturating_sub(undelivered);
                }
                report.trade_convoys_used = report.trade_convoys_used.saturating_sub(losses);
            }
            self.last_events.push(ConvoyEvent {
                buyer: shipment.buyer.clone(),
                seller: shipment.seller.clone(),
                resource: shipment.resource.clone(),
                convoy_losses: losses,
                delivered_after: shipment.delivered,
                region,
                raider_country,
            });
        }
    }
}

/// Import navy commanders from the *same selected bookmark* and country
/// history files used by the land army importer. Traits are preserved even
/// before every scripted trait effect has an implemented consumer.
pub fn import_navy_leaders(
    root: &Path,
    warfare: &Warfare,
    today: crate::game::GameDate,
    navy: &mut NavalState,
) {
    use crate::military_data::{atom, block, layered_files};
    let files = layered_files(
        &crate::military_data::data_layers(root),
        "history/countries",
        false,
        "txt",
    );
    for (tag, country) in &warfare.countries {
        let name = format!("{tag} -");
        let Some(file) = files.iter().find(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.starts_with(&name) || s == format!("{tag}.txt"))
        }) else {
            continue;
        };
        let Ok(source) = std::fs::read_to_string(file) else {
            continue;
        };
        let Ok(parsed) = crate::pdx::parse_pdx_compatible(&source) else {
            continue;
        };
        let (resolved, _) =
            crate::history_conditions::resolve(&parsed.entries, tag, &country.modifiers, today);
        for e in resolved.iter().filter(|e| e.key == "create_navy_leader") {
            let Some(props) = e.block() else { continue };
            let Some(name) = atom(props, "name") else {
                continue;
            };
            let val = |k: &str| {
                atom(props, k)
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(0)
            };
            let traits = block(props, "traits")
                .unwrap_or(&[])
                .iter()
                .map(|e| e.key.clone())
                .collect();
            let admiral = NavalAdmiral {
                country: tag.clone(),
                name: name.into(),
                skill: val("skill"),
                attack: val("attack_skill"),
                defense: val("defense_skill"),
                maneuvering: val("maneuvering_skill"),
                coordination: val("coordination_skill"),
                traits,
            };
            navy.admirals.insert((tag.clone(), name.into()), admiral);
        }
    }
}

/// Import HOI4 fleet -> task_force -> ship history records from the selected OOBs.
pub fn import_naval_oob(root: &Path, world: &World, warfare: &Warfare) -> NavalState {
    use crate::military_data::{atom, block, layered_files};
    let files = layered_files(
        &crate::military_data::data_layers(root),
        "history/units",
        false,
        "txt",
    );
    let mut out = NavalState::default();
    for (tag, army) in &warfare.countries {
        for oob in &army.history_oobs {
            let Some(path) = files
                .iter()
                .find(|p| p.file_stem().and_then(|s| s.to_str()) == Some(oob.as_str()))
            else {
                continue;
            };
            let Ok(source) = std::fs::read_to_string(path) else {
                continue;
            };
            let Ok(root) = crate::pdx::parse_pdx_compatible(&source) else {
                out.diagnostics
                    .push(format!("{tag}: unparseable naval OOB {oob}"));
                continue;
            };
            for unit in root.entries.iter().filter(|e| e.key == "units") {
                for fleet in unit
                    .block()
                    .unwrap_or(&[])
                    .iter()
                    .filter(|e| e.key == "fleet")
                {
                    let f = fleet.block().unwrap_or(&[]);
                    let fleet_base = atom(f, "naval_base").and_then(|v| v.parse::<u32>().ok());
                    for task in f.iter().filter(|e| e.key == "task_force") {
                        let t = task.block().unwrap_or(&[]);
                        let base = atom(t, "location")
                            .and_then(|v| v.parse::<u32>().ok())
                            .or(fleet_base);
                        let Some(base) = base else {
                            out.diagnostics
                                .push(format!("{tag}: {oob}: naval task force missing base"));
                            continue;
                        };
                        let mut ships = Vec::new();
                        for record in t.iter().filter(|e| e.key == "ship") {
                            let b = record.block().unwrap_or(&[]);
                            let Some(ship_data) = block(b, "equipment").and_then(|e| e.first())
                            else {
                                out.diagnostics
                                    .push(format!("{tag}: ship missing equipment"));
                                continue;
                            };
                            let model = ship_data.key.as_str();
                            let e = ship_data.block().unwrap_or(&[]);
                            let amount = atom(e, "amount")
                                .and_then(|s| s.parse::<u32>().ok())
                                .unwrap_or(1);
                            if amount != 1 {
                                out.diagnostics
                                    .push(format!("{tag}: {model}: ship amount must be 1"));
                                continue;
                            }
                            if !is_ship_model(model, &warfare.catalogue) {
                                out.diagnostics
                                    .push(format!("{tag}: unsupported ship model {model}"));
                                continue;
                            }
                            ships.push(NavalShip {
                                name: atom(b, "name").unwrap_or(model).into(),
                                model: model.into(),
                                definition: atom(b, "definition").unwrap_or("").into(),
                                variant: atom(e, "version_name").map(str::to_owned),
                                strength: 1.0,
                            });
                        }
                        if ships.is_empty() {
                            continue;
                        }
                        let name = atom(t, "name")
                            .or_else(|| atom(f, "name"))
                            .unwrap_or("Task Force");
                        if let Err(e) = out.add_historical(tag, name, base, ships, world, warfare) {
                            out.diagnostics.push(e);
                        }
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn naval_raiding_reduces_imports_and_escorts_protect_them() {
        use crate::map_graph::MapGraph;
        use crate::military_model::{CountryArmy, EquipmentDefinition};
        use crate::resource_economy::{CountryResources, ResourceEconomy, TradeShipment};
        use crate::trade_routes::TradeRoute;
        use std::collections::HashMap;
        let definition =
            "1;1;0;0;land;true;plains;1\n2;2;0;0;sea;false;ocean;0\n3;3;0;0;land;true;plains;1";
        let pixels = [1_u8, 2, 3]
            .into_iter()
            .flat_map(|id| [id, 0, 0, 255])
            .collect::<Vec<_>>();
        let mut world = World::from_map_data(
            &HashMap::from([(1, 1), (3, 3)]),
            &HashMap::from([(1, "A".into()), (3, "B".into())]),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );
        world.geography = MapGraph::from_rgba(3, 1, &pixels, definition, false).unwrap();
        let mut db = MilitaryCatalogue::default();
        for (model, attr, power) in [
            ("ship_hull_submarine_1", "torpedo_attack", 20.0),
            ("ship_hull_light_1", "sub_detection", 50.0),
        ] {
            db.equipment.insert(
                model.into(),
                EquipmentDefinition {
                    id: model.into(),
                    archetype: model.into(),
                    year: 1936,
                    active: true,
                    buildable: true,
                    stats: BTreeMap::from([(attr.into(), power)]),
                    cost: 100.0,
                    modular: false,
                    source: "test".into(),
                },
            );
        }
        let mut warfare = Warfare::new(db);
        for tag in ["A", "B", "R"] {
            warfare.countries.insert(tag.into(), CountryArmy::default());
        }
        warfare.wars.insert(("A".into(), "R".into()));
        warfare.air.regions.insert(10, BTreeSet::from([2]));
        warfare
            .logistics
            .countries
            .entry("A".into())
            .or_default()
            .convoys = 2;

        let make_force = |id, country: &str, mission, model: &str| NavalTaskForce {
            id,
            country: country.into(),
            name: format!("{country} Fleet"),
            base: 0,
            mission,
            regions: BTreeSet::from([10]),
            ships: vec![NavalShip {
                name: model.into(),
                model: model.into(),
                definition: String::new(),
                variant: None,
                strength: 1.0,
            }],
        };
        let mut naval = NavalState::default();
        naval.task_forces.insert(
            1,
            make_force(1, "R", NavalMission::ConvoyRaiding, "ship_hull_submarine_1"),
        );
        let key = ("A".into(), "B".into(), "steel".into());
        naval.loss_progress.insert(key.clone(), 0.5);
        let mut shipping = ResourceEconomy::default();
        shipping.last.insert(
            "A".into(),
            CountryResources {
                imported: BTreeMap::from([("steel".into(), 16)]),
                available: BTreeMap::from([("steel".into(), 16)]),
                trade_convoys_used: 2,
                ..Default::default()
            },
        );
        shipping.last_shipments.push(TradeShipment {
            buyer: "A".into(),
            seller: "B".into(),
            resource: "steel".into(),
            factories: 2,
            contracted: 16,
            delivered: 16,
            required_convoys: 2,
            assigned_convoys: 2,
            route: TradeRoute::Sea {
                origin_port: 1,
                destination_port: 3,
                sea_provinces: vec![2],
            },
        });
        let original = shipping.clone();
        let mut losses = BTreeMap::new();
        naval.process_trade(&world, &warfare, &mut shipping, &mut losses);
        assert_eq!(losses.get("A"), Some(&1));
        assert_eq!(shipping.last["A"].imported["steel"], 8);
        assert_eq!(shipping.last["A"].trade_convoys_used, 1);
        naval.task_forces.insert(
            2,
            make_force(2, "A", NavalMission::ConvoyEscort, "ship_hull_light_1"),
        );
        naval.loss_progress.insert(key, 0.5);
        shipping = original;
        losses.clear();
        naval.process_trade(&world, &warfare, &mut shipping, &mut losses);
        assert!(losses.is_empty());
        assert_eq!(shipping.last["A"].imported["steel"], 16);
    }

    #[test]
    fn json_saves_persist_contacts_damage_and_prior_trade_records() {
        let mut navy = NavalState::default();
        navy.spotting.insert((100, 1, 2), 0.8);
        navy.last_contact_battle.insert((100, 1, 2), 48);
        navy.loss_progress
            .insert(("A".into(), "B".into(), "steel".into()), 0.35);
        navy.conditions.insert(
            (1, 0),
            crate::naval_combat::ShipCondition {
                organisation: 0.3,
                critical_damage: 0.2,
            },
        );
        let saved = serde_json::to_string(&navy).unwrap();
        let loaded: NavalState = serde_json::from_str(&saved).unwrap();
        assert_eq!(loaded.spotting[&(100, 1, 2)], 0.8);
        assert_eq!(loaded.last_contact_battle[&(100, 1, 2)], 48);
        assert_eq!(
            loaded.loss_progress[&("A".into(), "B".into(), "steel".into())],
            0.35
        );
        assert_eq!(loaded.conditions[&(1, 0)].organisation, 0.3);
        let legacy: NavalState = serde_json::from_str(
            r#"{"admirals":{},"loss_progress":{},"spotting":{},"conditions":{}}"#,
        )
        .unwrap();
        assert!(legacy.admirals.is_empty() && legacy.loss_progress.is_empty());
    }

    #[test]
    fn navy_commanders_are_national_unique_and_survive_save() {
        let mut navy = NavalState::default();
        for (id, tag) in [(1, "A"), (2, "A"), (3, "B")] {
            navy.task_forces.insert(
                id,
                NavalTaskForce {
                    id,
                    country: tag.into(),
                    name: format!("Fleet {id}"),
                    base: 1,
                    mission: NavalMission::Hold,
                    regions: BTreeSet::new(),
                    ships: Vec::new(),
                },
            );
        }
        for tag in ["A", "B"] {
            navy.admirals.insert(
                (tag.into(), "Commander".into()),
                NavalAdmiral {
                    country: tag.into(),
                    name: "Commander".into(),
                    skill: 3,
                    attack: 2,
                    defense: 3,
                    maneuvering: 4,
                    coordination: 2,
                    traits: BTreeSet::new(),
                },
            );
        }
        navy.assign_admiral("A", 1, Some("Commander")).unwrap();
        assert!(navy.assign_admiral("A", 2, Some("Commander")).is_err());
        assert!(navy.assign_admiral("B", 1, Some("Commander")).is_err());
        navy.assign_admiral("B", 3, Some("Commander")).unwrap();
        let json = serde_json::to_string(&navy).unwrap();
        let mut restored: NavalState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.admiral_for(1).unwrap().maneuvering, 4);
        restored.assign_admiral("A", 1, None).unwrap();
        restored.assign_admiral("A", 2, Some("Commander")).unwrap();
        assert_eq!(restored.admiral_for(2).unwrap().skill, 3);
    }

    #[test]
    fn missions_cycle_through_actual_naval_commands() {
        let mut m = NavalMission::Hold;
        for _ in 0..5 {
            m = m.next();
        }
        assert_eq!(m, NavalMission::Hold);
    }
    #[test]
    fn dockyards_reject_locked_hulls_then_commission_researched_ships() {
        use crate::military_model::{CountryArmy, EquipmentDefinition};
        use crate::state::StateEconomy;
        use std::collections::HashMap;
        let mut world = World::from_map_data(
            &HashMap::from([(1, 1)]),
            &HashMap::from([(1, "A".into())]),
            &HashMap::new(),
            &HashMap::from([(
                1,
                StateEconomy {
                    dockyards: 2,
                    ..Default::default()
                },
            )]),
            &HashMap::new(),
        );
        let mut db = MilitaryCatalogue::default();
        db.equipment.insert(
            "ship_hull_light_1".into(),
            EquipmentDefinition {
                id: "ship_hull_light_1".into(),
                archetype: "ship_hull_light".into(),
                year: 1936,
                active: false,
                buildable: true,
                stats: BTreeMap::new(),
                cost: 5.0,
                modular: false,
                source: "fixture".into(),
            },
        );
        db.equipment_resources.insert(
            "ship_hull_light_1".into(),
            BTreeMap::from([("steel".into(), 1)]),
        );
        let mut warfare = Warfare::new(db);
        warfare.countries.insert("A".into(), CountryArmy::default());
        warfare.conditions.insert(
            1,
            crate::warfare::ProvinceConditions {
                naval_base: 1,
                ..Default::default()
            },
        );
        let mut navy = NavalState::default();
        assert!(
            navy.queue_ship("A", "ship_hull_light_1", 1, 2, &world, &warfare)
                .is_err()
        );
        warfare
            .countries
            .get_mut("A")
            .unwrap()
            .modifiers
            .unlocked_equipment
            .insert("ship_hull_light_1".into());
        navy.queue_ship("A", "ship_hull_light_1", 1, 2, &world, &warfare)
            .unwrap();
        let mut materials = BTreeMap::from([("steel".into(), 2)]);
        navy.tick_shipbuilding("A", 2, &world, &warfare, &mut materials, 0.05);
        assert_eq!(materials["steel"], 0);
        assert_eq!(navy.shipbuilding["A"][0].completed, 1);
        assert_eq!(
            navy.task_forces
                .values()
                .map(|tf| tf.ships.len())
                .sum::<usize>(),
            1
        );
        let before = navy.shipbuilding["A"][0].completed;
        // Fewer imported steel deposits produces a per-dockyard penalty;
        // finishing a ship still requires enough actual IC.
        navy.shipbuilding.get_mut("A").unwrap()[0].partial_ic = 0.0;
        materials.insert("steel".into(), 0);
        navy.tick_shipbuilding("A", 2, &world, &warfare, &mut materials, 0.10);
        assert!(navy.shipbuilding["A"][0].last_day_ic < 5.0);
        assert_eq!(navy.shipbuilding["A"][0].completed, before);
        // No magic dockyard construction in captured/foreign ports.
        world.set_province_controller(1, None);
        navy.tick_shipbuilding("A", 2, &world, &warfare, &mut materials, 0.05);
        assert_eq!(navy.shipbuilding["A"][0].last_day_ic, 0.0);
    }

    #[test]
    fn historical_fleet_import_preserves_named_hulls_and_owner() {
        use crate::military_model::{CountryArmy, EquipmentDefinition};
        use std::collections::HashMap;
        let root = std::env::temp_dir().join(format!("hoi4-naval-fixture-{}", std::process::id()));
        let units = root.join("history/units");
        std::fs::create_dir_all(&units).unwrap();
        let content = r#"units = { fleet = {
            name = "Historical fleet" naval_base = 1
            task_force = { name = "Task force 1" location = 1
                ship = { name = "Ship Alpha" definition = destroyer
                    equipment = { ship_hull_light_1 = { amount = 1 owner = A } }
                }
            }
        } }"#;
        std::fs::write(units.join("A_1936.txt"), content).unwrap();
        let world = World::from_map_data(
            &HashMap::from([(1, 1)]),
            &HashMap::from([(1, "A".into())]),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
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
                stats: BTreeMap::new(),
                cost: 4.0,
                modular: false,
                source: "fixture".into(),
            },
        );
        let mut warfare = Warfare::new(db);
        let mut army = CountryArmy::default();
        army.history_oobs.push("A_1936".into());
        warfare.countries.insert("A".into(), army);
        warfare.conditions.insert(
            1,
            crate::warfare::ProvinceConditions {
                naval_base: 2,
                ..Default::default()
            },
        );
        let navy = import_naval_oob(&root, &world, &warfare);
        assert_eq!(navy.task_forces.len(), 1);
        let force = navy.task_forces.values().next().unwrap();
        assert_eq!(force.name, "Task force 1");
        assert_eq!(force.ships[0].name, "Ship Alpha");
        assert_eq!(force.ships[0].model, "ship_hull_light_1");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn naval_building_rejects_unresearched_hulls() {
        use crate::military_model::EquipmentDefinition;
        let mut cat = MilitaryCatalogue::default();
        cat.equipment.insert(
            "ship_hull_light_2".into(),
            EquipmentDefinition {
                id: "ship_hull_light_2".into(),
                archetype: "ship_hull_light".into(),
                year: 1940,
                active: false,
                buildable: true,
                stats: BTreeMap::new(),
                cost: 600.0,
                modular: false,
                source: "test".into(),
            },
        );
        let mut modifiers = MilitaryModifiers::default();
        assert!(!NavalState::can_build_ship(
            &cat,
            "ship_hull_light_2",
            &modifiers
        ));
        modifiers
            .unlocked_equipment
            .insert("ship_hull_light_2".into());
        assert!(NavalState::can_build_ship(
            &cat,
            "ship_hull_light_2",
            &modifiers
        ));
    }
}
