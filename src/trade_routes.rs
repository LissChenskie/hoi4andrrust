//! Physical international-trade routing on the imported HOI4 province graph.
//! No fabricated ocean connections. Political transit through neutral states
//! is provisional until military/diplomatic access laws are integrated.
use crate::map_graph::{CrossingKind, ProvinceKind};
use crate::warfare::Warfare;
use crate::world::World;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeRoute {
    /// Same contiguous land network: no shipping required.
    Land { provinces: Vec<u32> },
    /// Ordered sea provinces; endpoint naval bases physically touch this path.
    Sea {
        origin_port: u32,
        destination_port: u32,
        sea_provinces: Vec<u32>,
    },
    Unavailable { reason: String },
}

impl TradeRoute {
    pub fn sea_provinces(&self) -> &[u32] {
        match self {
            Self::Sea { sea_provinces, .. } => sea_provinces,
            _ => &[],
        }
    }
}

fn at_war(wars: &BTreeSet<(String, String)>, a: &str, b: &str) -> bool {
    if a == b { return false }
    let (a,b) = if a < b { (a,b) } else { (b,a) };
    wars.contains(&(a.to_owned(), b.to_owned()))
}

fn is_land(world: &World, id: u32) -> bool {
    world.geography.provinces.get(&id)
        .is_some_and(|p| p.definition.kind == ProvinceKind::Land)
}
fn is_sea(world: &World, id: u32) -> bool {
    world.geography.provinces.get(&id)
        .is_some_and(|p| p.definition.kind == ProvinceKind::Sea)
}
fn ordinary_edge(edge: &crate::map_graph::Connection) -> bool {
    edge.kind == CrossingKind::Border && edge.rule.is_none()
}

fn trace(previous: &BTreeMap<u32, u32>, mut end: u32) -> Vec<u32> {
    let mut out = vec![end];
    while let Some(&parent) = previous.get(&end) {
        out.push(parent);
        end = parent;
    }
    out.reverse();
    out
}

fn own_land_network(world: &World, country: &str, capital: u32) -> BTreeSet<u32> {
    let mut reachable = BTreeSet::new();
    if !is_land(world, capital) || world.controller_of_province(capital) != Some(country) {
        return reachable;
    }
    let mut queue = VecDeque::from([capital]);
    reachable.insert(capital);
    while let Some(current) = queue.pop_front() {
        for edge in world.geography.connections_from(current) {
            if ordinary_edge(edge)
                && is_land(world, edge.to)
                && world.controller_of_province(edge.to) == Some(country)
                && reachable.insert(edge.to)
            {
                queue.push_back(edge.to);
            }
        }
    }
    reachable
}

fn port_entrances(
    world: &World,
    warfare: &Warfare,
    country: &str,
    capital: u32,
    closed_seas: &BTreeSet<u32>,
) -> BTreeMap<u32, u32> {
    let mut entrances = BTreeMap::new();
    for province in own_land_network(world, country, capital) {
        if warfare.conditions.get(&province).is_none_or(|c| c.naval_base == 0) {
            continue;
        }
        for edge in world.geography.connections_from(province) {
            if !closed_seas.contains(&edge.to)
                && ordinary_edge(edge)
                && is_sea(world, edge.to)
            {
                entrances.entry(edge.to).or_insert(province);
            }
        }
    }
    entrances
}

/// Determine route from each country's current historical capital.
/// Land transit over third-party neutral countries is allowed but wartime
/// enemies and undefined controllers are not. Scripted conditional straits
/// remain closed until their diplomatic conditions have been implemented.
pub fn find_route(
    world: &World,
    warfare: &Warfare,
    buyer: &str,
    seller: &str,
    closed_seas: &BTreeSet<u32>,
) -> TradeRoute {
    if buyer == seller || warfare.at_war(buyer, seller) {
        return TradeRoute::Unavailable { reason: "Страны воюют или совпадают".into() };
    }
    let a = warfare.logistics.countries.get(buyer).and_then(|c| c.capital);
    let b = warfare.logistics.countries.get(seller).and_then(|c| c.capital);
    let (Some(start), Some(goal)) = (a, b) else {
        return TradeRoute::Unavailable { reason: "Не определена столица одной из стран".into() };
    };
    if !is_land(world, start) || !is_land(world, goal)
        || world.controller_of_province(start) != Some(buyer)
        || world.controller_of_province(goal) != Some(seller)
    {
        return TradeRoute::Unavailable { reason: "Нет доступной столицы на карте".into() };
    }
    // Shortest allowed overland connection by province count.
    let mut queue = VecDeque::from([start]);
    let mut seen = BTreeSet::from([start]);
    let mut previous = BTreeMap::new();
    while let Some(current) = queue.pop_front() {
        if current == goal { return TradeRoute::Land { provinces: trace(&previous, current) }; }
        for edge in world.geography.connections_from(current) {
            if !ordinary_edge(edge) || !is_land(world, edge.to) { continue; }
            let Some(controller) = world.controller_of_province(edge.to) else { continue };
            if warfare.at_war(buyer, controller) || warfare.at_war(seller, controller) {
                continue;
            }
            if seen.insert(edge.to) {
                previous.insert(edge.to, current);
                queue.push_back(edge.to);
            }
        }
    }
    let starts = port_entrances(world, warfare, buyer, start, closed_seas);
    let goals = port_entrances(world, warfare, seller, goal, closed_seas);
    if starts.is_empty() || goals.is_empty() {
        return TradeRoute::Unavailable { reason: "Нет доступных портов для морской торговли".into() };
    }
    // Multi-source BFS: no teleporting between disconnected seas.
    let mut queue = VecDeque::new();
    let mut seen = BTreeSet::new();
    let mut previous = BTreeMap::new();
    let mut first_port = BTreeMap::new();
    for (&sea, &port) in &starts {
        if seen.insert(sea) {
            first_port.insert(sea, port);
            queue.push_back(sea);
        }
    }
    while let Some(current) = queue.pop_front() {
        if let Some(&destination_port) = goals.get(&current) {
            return TradeRoute::Sea {
                origin_port: first_port[&current],
                destination_port,
                sea_provinces: trace(&previous, current),
            };
        }
        for edge in world.geography.connections_from(current) {
            if !ordinary_edge(edge)
                || closed_seas.contains(&edge.to)
                || !is_sea(world, edge.to)
                || !seen.insert(edge.to)
            { continue; }
            previous.insert(edge.to, current);
            first_port.insert(edge.to, first_port[&current]);
            queue.push_back(edge.to);
        }
    }
    TradeRoute::Unavailable { reason: "Морской путь перекрыт или отсутствует".into() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{map_graph::MapGraph, logistics::CountryLogistics, warfare::ProvinceConditions};
    use std::collections::HashMap;
    fn world_and_war(land_route: bool) -> (World, Warfare) {
        let definition = (1..=5)
            .map(|i| format!("{i};{i};0;0;{};true;plains;1",
                if land_route || i <= 2 || i == 5 { "land" } else { "sea" }))
            .collect::<Vec<_>>().join("\n");
        let rgba = (1u8..=5).flat_map(|i| [i, 0, 0, 255]).collect::<Vec<_>>();
        let map = MapGraph::from_rgba(5, 1, &rgba, &definition, false).unwrap();
        let mut world = World::from_map_data(
            &HashMap::from([(1, 1), (2, 1), (5, 5)]),
            &HashMap::from([(1, "A".into()), (5, "B".into())]),
            &HashMap::new(), &HashMap::new(), &HashMap::new(),
        );
        world.geography = map;
        let mut war = Warfare::default();
        war.logistics.countries.insert("A".into(), CountryLogistics {
            capital: Some(1), convoys: 3, ..Default::default()
        });
        war.logistics.countries.insert("B".into(), CountryLogistics {
            capital: Some(5), convoys: 0, ..Default::default()
        });
        war.conditions.insert(2, ProvinceConditions { naval_base: 1, ..Default::default() });
        war.conditions.insert(5, ProvinceConditions { naval_base: 1, ..Default::default() });
        (world, war)
    }
    #[test]
    fn sea_route_uses_actual_coast_ports_and_water_adjacency() {
        let (world, war) = world_and_war(false);
        assert_eq!(find_route(&world, &war, "A", "B", &BTreeSet::new()),
            TradeRoute::Sea { origin_port: 2, destination_port: 5, sea_provinces: vec![3, 4] });
        assert!(matches!(find_route(&world, &war, "A", "B", &BTreeSet::from([4])),
            TradeRoute::Unavailable { .. }));
    }
    #[test]
    fn connected_land_route_needs_no_convoys() {
        let (world, war) = world_and_war(true);
        assert!(matches!(find_route(&world, &war, "A", "B", &BTreeSet::new()),
            TradeRoute::Land { .. }));
    }
    #[test]
    fn missing_capital_or_port_never_grants_free_transport() {
        let (world, mut war) = world_and_war(false);
        war.conditions.remove(&5);
        assert!(matches!(find_route(&world, &war, "A", "B", &BTreeSet::new()),
            TradeRoute::Unavailable { .. }));
        war.logistics.countries.get_mut("B").unwrap().capital = None;
        assert!(matches!(find_route(&world, &war, "A", "B", &BTreeSet::new()),
            TradeRoute::Unavailable { .. }));
    }
}
