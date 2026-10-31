//! Read-only view of the simulation for one country. Uses only information a
//! human player of that country could see about their own state.
use crate::research::CountryResearch;
use crate::warfare::Warfare;
use crate::world::World;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountryObservation {
    pub tag: String,
    pub states: u32,
    pub civilian_factories: u32,
    pub military_factories: u32,
    pub dockyards: u32,
    pub enemies: u32,
    pub free_research_slots: usize,
}

impl CountryObservation {
    pub fn at_war(&self) -> bool {
        self.enemies > 0
    }
}

pub fn observe(
    tag: &str,
    world: &World,
    military: &Warfare,
    research: &CountryResearch,
) -> Option<CountryObservation> {
    let economy = world.country_economy(tag)?;
    let enemies = military
        .wars
        .iter()
        .filter(|(a, b)| a == tag || b == tag)
        .count() as u32;
    Some(CountryObservation {
        tag: tag.to_owned(),
        states: economy.states,
        civilian_factories: economy.civilian_factories,
        military_factories: economy.military_factories,
        dockyards: economy.dockyards,
        enemies,
        free_research_slots: research.slots.iter().filter(|s| s.active.is_none()).count(),
    })
}
