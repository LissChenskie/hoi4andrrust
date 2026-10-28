//! Bounded, convoy-backed coastal voyages. Naval invasions are limited to
//! unopposed beaches until amphibious battle integration exists; no land battle
//! or naval superiority is invented by the transport order.
use crate::{
    logistics::LogisticsState,
    map_graph::{CrossingKind, ProvinceKind},
    military_model::DivisionOrder,
    naval_warfare::NavalState,
    warfare::Warfare,
    world::World,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoyageKind {
    Transfer,
    UnopposedLanding,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavalVoyage {
    pub id: u64,
    pub country: String,
    pub divisions: Vec<u64>,
    pub origin: u32,
    pub destination: u32,
    pub sea_path: Vec<u32>,
    pub convoys: u64,
    pub hours_remaining: u32,
    pub kind: VoyageKind,
    /// A contested beach is kept pending, rather than captured without combat.
    pub awaiting_clear_beach: bool,
}

fn sea_path(world: &World, blocked: &BTreeSet<u32>, origin: u32, goal: u32) -> Option<Vec<u32>> {
    let sea = |p| {
        !blocked.contains(&p)
            && world
                .geography
                .provinces
                .get(&p)
                .is_some_and(|g| g.definition.kind == ProvinceKind::Sea)
    };
    let targets: BTreeSet<_> = world
        .geography
        .connections_from(goal)
        .iter()
        .filter(|e| e.kind != CrossingKind::Impassable && sea(e.to))
        .map(|e| e.to)
        .collect();
    if targets.is_empty() {
        return None;
    }
    let mut previous = BTreeMap::new();
    let mut queue = VecDeque::new();
    for edge in world.geography.connections_from(origin) {
        if edge.kind != CrossingKind::Impassable && sea(edge.to) {
            previous.insert(edge.to, edge.to);
            queue.push_back(edge.to);
        }
    }
    while let Some(sea_province) = queue.pop_front() {
        if targets.contains(&sea_province) {
            let mut path = vec![sea_province];
            let mut current = sea_province;
            while previous[&current] != current {
                current = previous[&current];
                path.push(current);
            }
            path.reverse();
            return Some(path);
        }
        for edge in world.geography.connections_from(sea_province) {
            if edge.kind != CrossingKind::Impassable
                && sea(edge.to)
                && !previous.contains_key(&edge.to)
            {
                previous.insert(edge.to, sea_province);
                queue.push_back(edge.to);
            }
        }
    }
    None
}

impl NavalState {
    pub fn reserved_transport_convoys(&self, tag: &str) -> u64 {
        self.voyages
            .values()
            .filter(|v| v.country == tag)
            .map(|v| v.convoys)
            .sum()
    }

    pub fn sync_transport_convoys(&self, logistics: &mut LogisticsState) {
        let mut by_country = BTreeMap::<String, u64>::new();
        for voyage in self.voyages.values() {
            *by_country.entry(voyage.country.clone()).or_default() += voyage.convoys;
        }
        for (tag, c) in &mut logistics.countries {
            let reserved = by_country.get(tag).copied().unwrap_or(0);
            if c.transport_convoys_committed != reserved {
                c.transport_convoys_committed = reserved;
                logistics.network_key = None;
            }
        }
    }

    /// Validates route, equipment, ports, political access and shared convoy
    /// budget atomically. No movement or occupation occurs at order creation.
    pub fn queue_voyage(
        &mut self,
        w: &mut Warfare,
        world: &World,
        tag: &str,
        ids: &[u64],
        destination: u32,
        kind: VoyageKind,
    ) -> Result<u64, String> {
        if ids.is_empty() {
            return Err("Выберите дивизии для морской перевозки".into());
        }
        let unique: BTreeSet<_> = ids.iter().copied().collect();
        if unique.len() != ids.len() {
            return Err("Дивизия не может быть назначена дважды".into());
        }
        let first = w.divisions.get(&ids[0]).ok_or("Дивизия не найдена")?;
        let origin = first.province;
        if !w.conditions.get(&origin).is_some_and(|c| c.naval_base > 0)
            || world.controller_of_province(origin) != Some(tag)
        {
            return Err("Посадка возможна только в собственном действующем порту".into());
        }
        for id in ids {
            let d = w.divisions.get(id).ok_or("Дивизия не найдена")?;
            if d.country != tag
                || d.destroyed
                || d.province != origin
                || d.battle.is_some()
                || !matches!(d.order, DivisionOrder::Hold)
                || self.voyages.values().any(|v| v.divisions.contains(id))
            {
                return Err("Дивизии должны свободно находиться в одном собственном порту".into());
            }
        }
        let owner = world
            .controller_of_province(destination)
            .ok_or("Побережье не контролируется")?;
        if !world
            .geography
            .provinces
            .get(&destination)
            .is_some_and(|p| p.definition.kind == ProvinceKind::Land && p.definition.coastal)
        {
            return Err("Нужна прибрежная сухопутная провинция".into());
        }
        match kind {
            VoyageKind::Transfer => {
                if !w.friendly(tag, owner)
                    || !w
                        .conditions
                        .get(&destination)
                        .is_some_and(|c| c.naval_base > 0)
                {
                    return Err("Для перевозки нужен доступный дружественный порт".into());
                }
            }
            VoyageKind::UnopposedLanding => {
                if !w.at_war(tag, owner) {
                    return Err(
                        "Высадка разрешена только на вражеское побережье во время войны".into(),
                    );
                }
            }
        }
        let path = sea_path(world, &w.logistics.blocked_sea, origin, destination)
            .ok_or("Нет непрерывного доступного морского пути")?;
        // Provisional loading ratio, not an asserted vanilla HOI4 coefficient.
        let men: u64 = ids
            .iter()
            .map(|id| u64::from(w.divisions[id].manpower))
            .sum();
        let needed = men.div_ceil(1_000).max(1);
        self.sync_transport_convoys(&mut w.logistics);
        let available = w.logistics.countries.get(tag).map_or(0, |c| {
            c.convoys.saturating_sub(
                c.trade_convoys_committed
                    .saturating_add(c.transport_convoys_committed),
            )
        });
        if available < needed {
            return Err(format!(
                "Для перевозки требуется {needed} свободных конвоев"
            ));
        }
        self.next_voyage_id = self.next_voyage_id.saturating_add(1);
        let id = self.next_voyage_id;
        self.voyages.insert(
            id,
            NavalVoyage {
                id,
                country: tag.to_owned(),
                divisions: ids.to_vec(),
                origin,
                destination,
                sea_path: path.clone(),
                convoys: needed,
                hours_remaining: (path.len() as u32).saturating_mul(6).max(6),
                kind,
                awaiting_clear_beach: false,
            },
        );
        self.sync_transport_convoys(&mut w.logistics);
        Ok(id)
    }

    pub fn cancel_voyage(&mut self, tag: &str, id: u64, logistics: &mut LogisticsState) -> bool {
        if !self.voyages.get(&id).is_some_and(|v| v.country == tag) {
            return false;
        }
        self.voyages.remove(&id);
        self.sync_transport_convoys(logistics);
        true
    }

    /// Tick after the normal land movement/command phase. Units continue to be
    /// located at their embarkation port while a provisional voyage is pending.
    /// This is not a completed at-sea/contested-amphibious combat simulation.
    pub fn tick_voyages(&mut self, w: &mut Warfare, world: &mut World) {
        let ids: Vec<_> = self.voyages.keys().copied().collect();
        for id in ids {
            let Some(v) = self.voyages.get(&id) else {
                continue;
            };
            let cancelled = world.controller_of_province(v.origin) != Some(v.country.as_str())
                || v.divisions.iter().any(|id| {
                    !w.divisions.get(id).is_some_and(|d| {
                        !d.destroyed
                            && d.country == v.country
                            && d.province == v.origin
                            && d.battle.is_none()
                            && matches!(d.order, DivisionOrder::Hold)
                    })
                });
            if cancelled {
                self.voyages.remove(&id);
                continue;
            }
            // A convoy shortage suspends the voyage rather than silently
            // spawning missing transport hulls or a destination occupation.
            let pool = w.logistics.countries.get(&v.country).map_or(0, |c| {
                c.convoys.saturating_sub(c.trade_convoys_committed)
            });
            let reserved = self.reserved_transport_convoys(&v.country);
            if pool < reserved {
                continue;
            }
            let v = self.voyages.get_mut(&id).unwrap();
            if v.hours_remaining > 0 {
                v.hours_remaining -= 1;
                continue;
            }
            // A newly closed strait or interdicted sea route cannot be
            // crossed by a previously accepted naval transport.
            if v.sea_path
                .iter()
                .any(|p| w.logistics.blocked_sea.contains(p))
            {
                continue;
            }
            let Some(controller) = world.controller_of_province(v.destination) else {
                continue;
            };
            let accessible = match v.kind {
                VoyageKind::Transfer => {
                    w.friendly(&v.country, controller)
                        && w.conditions
                            .get(&v.destination)
                            .is_some_and(|c| c.naval_base > 0)
                }
                VoyageKind::UnopposedLanding => {
                    w.at_war(&v.country, controller)
                        && !w.divisions.values().any(|d| {
                            !d.destroyed
                                && d.province == v.destination
                                && w.at_war(&v.country, &d.country)
                        })
                        && !w
                            .battles
                            .values()
                            .any(|b| b.province == v.destination && b.outcome.is_none())
                }
            };
            if !accessible {
                v.awaiting_clear_beach = true;
                continue;
            }
            if v.kind == VoyageKind::UnopposedLanding {
                world.set_province_controller(v.destination, Some(&v.country));
            }
            for division_id in &v.divisions {
                let d = w.divisions.get_mut(division_id).unwrap();
                d.province = v.destination;
                d.entrenchment = 0.;
                d.planning = 0.;
            }
            self.voyages.remove(&id);
        }
        self.sync_transport_convoys(&mut w.logistics);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        logistics::CountryLogistics, map_graph::MapGraph, military_model::tests::division,
        warfare::ProvinceConditions,
    };
    use std::collections::HashMap;

    fn scene(enemy_beach: bool) -> (World, Warfare, NavalState) {
        let data = "1;1;0;0;land;true;plains;1\n2;2;0;0;sea;false;ocean;0\n3;3;0;0;sea;false;ocean;0\n4;4;0;0;land;true;plains;1";
        let rgba: Vec<u8> = (1u8..=4).flat_map(|v| [v, 0, 0, 255]).collect();
        let owner = if enemy_beach { "B" } else { "A" };
        let mut world = World::from_map_data(
            &HashMap::from([(1, 1), (4, 4)]),
            &HashMap::from([(1, "A".into()), (4, owner.into())]),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );
        world.geography = MapGraph::from_rgba(4, 1, &rgba, data, false).unwrap();
        let mut w = Warfare::default();
        w.conditions.insert(
            1,
            ProvinceConditions {
                naval_base: 3,
                ..Default::default()
            },
        );
        w.conditions.insert(
            4,
            ProvinceConditions {
                naval_base: 2,
                ..Default::default()
            },
        );
        w.logistics.countries.insert(
            "A".into(),
            CountryLogistics {
                capital: Some(1),
                convoys: 10,
                ..Default::default()
            },
        );
        w.divisions.insert(1, division(1, "A", 1));
        w.divisions.insert(2, division(2, "A", 1));
        (world, w, NavalState::default())
    }

    #[test]
    fn transfer_reserves_real_hulls_and_arrives_after_hours_not_instantly() {
        let (mut world, mut w, mut navy) = scene(false);
        let voyage = navy
            .queue_voyage(&mut w, &world, "A", &[1], 4, VoyageKind::Transfer)
            .unwrap();
        let reserved = navy.voyages[&voyage].convoys;
        assert!(reserved > 0);
        assert_eq!(
            w.logistics.countries["A"].transport_convoys_committed,
            reserved
        );
        assert_eq!(w.divisions[&1].province, 1);
        let hours = navy.voyages[&voyage].hours_remaining;
        for _ in 0..hours {
            navy.tick_voyages(&mut w, &mut world);
            assert_eq!(w.divisions[&1].province, 1);
        }
        navy.tick_voyages(&mut w, &mut world);
        assert_eq!(w.divisions[&1].province, 4);
        assert!(navy.voyages.is_empty());
        assert_eq!(w.logistics.countries["A"].transport_convoys_committed, 0);
    }

    #[test]
    fn invasion_only_claims_an_empty_hostile_coast() {
        let (mut world, mut w, mut navy) = scene(true);
        w.wars.insert(("A".into(), "B".into()));
        let id = navy
            .queue_voyage(&mut w, &world, "A", &[1], 4, VoyageKind::UnopposedLanding)
            .unwrap();
        let hours = navy.voyages[&id].hours_remaining;
        w.divisions.insert(99, division(99, "B", 4));
        for _ in 0..=hours {
            navy.tick_voyages(&mut w, &mut world);
        }
        assert_eq!(world.controller_of_province(4), Some("B"));
        assert_eq!(w.divisions[&1].province, 1);
        assert!(navy.voyages[&id].awaiting_clear_beach);
        w.divisions.remove(&99);
        navy.tick_voyages(&mut w, &mut world);
        assert_eq!(world.controller_of_province(4), Some("A"));
        assert_eq!(w.divisions[&1].province, 4);
    }

    #[test]
    fn blocked_sea_captured_port_and_issued_land_orders_prevent_teleporting() {
        let (mut world, mut w, mut navy) = scene(false);
        w.logistics.blocked_sea.insert(2);
        assert!(
            navy.queue_voyage(&mut w, &world, "A", &[1], 4, VoyageKind::Transfer)
                .is_err()
        );
        w.logistics.blocked_sea.clear();
        let id = navy
            .queue_voyage(&mut w, &world, "A", &[1], 4, VoyageKind::Transfer)
            .unwrap();
        let hours = navy.voyages[&id].hours_remaining;
        w.logistics.blocked_sea.insert(3);
        for _ in 0..=hours {
            navy.tick_voyages(&mut w, &mut world);
        }
        assert_eq!(w.divisions[&1].province, 1);
        assert!(navy.voyages.contains_key(&id));
        w.logistics.blocked_sea.clear();
        world.set_province_controller(1, Some("B"));
        navy.tick_voyages(&mut w, &mut world);
        assert!(navy.voyages.is_empty());
        assert_eq!(w.divisions[&1].province, 1);
    }

    #[test]
    fn convoy_budget_and_saved_voyages_do_not_duplicate_ships() {
        let (mut world, mut w, mut navy) = scene(false);
        w.logistics.countries.get_mut("A").unwrap().convoys = 1;
        let first = navy
            .queue_voyage(&mut w, &world, "A", &[1], 4, VoyageKind::Transfer)
            .unwrap();
        assert!(
            navy.queue_voyage(&mut w, &world, "A", &[2], 4, VoyageKind::Transfer)
                .is_err()
        );
        let restored: NavalState =
            serde_json::from_str(&serde_json::to_string(&navy).unwrap()).unwrap();
        assert_eq!(restored.voyages[&first].destination, 4);
        w.logistics
            .countries
            .get_mut("A")
            .unwrap()
            .transport_convoys_committed = 0;
        restored.sync_transport_convoys(&mut w.logistics);
        assert_eq!(w.logistics.countries["A"].transport_convoys_committed, 1);
        assert!(navy.cancel_voyage("A", first, &mut w.logistics));
        assert_eq!(w.logistics.countries["A"].transport_convoys_committed, 0);
        // The division may be reallocated after cancellation.
        assert!(
            navy.queue_voyage(&mut w, &world, "A", &[2], 4, VoyageKind::Transfer)
                .is_ok()
        );
        let _ = &mut world;
    }
}
