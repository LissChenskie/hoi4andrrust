//! The AI acts only through the game's own validated entry points. Each call is
//! recorded with its outcome so decisions can be audited and tested.
use super::state::{AiState, Decision, Domain};
use crate::game::GameDate;
use crate::research::{CountryResearch, ResearchCatalogue, ResearchRules};

/// Start research in `slot`. Same rules as the player: prerequisites, XOR,
/// slot availability and cost are all checked by `CountryResearch::start`.
#[allow(clippy::too_many_arguments)]
pub fn start_research(
    state: &mut AiState,
    hour: u64,
    tag: &str,
    country: &mut CountryResearch,
    slot: usize,
    tech: &str,
    catalogue: &ResearchCatalogue,
    rules: &ResearchRules,
    date: GameDate,
) -> bool {
    let result = country.start(slot, tech, catalogue, rules, date);
    let accepted = result.is_ok();
    state.record(Decision {
        hour,
        tag: tag.to_owned(),
        domain: Domain::Research,
        command: format!("start_research slot={slot} tech={tech}"),
        accepted,
        detail: result.err().unwrap_or("ok").to_owned(),
    });
    accepted
}
