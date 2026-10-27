//! Read-only mobile presentation boundary. No widget types or independent units.
//! Build on demand (inspection/import report), never for every army every frame.
use crate::{command::ArmyId, warfare::Warfare};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraitViewModel {
    pub id: String,
    pub name: String,
    pub description: String,
    pub definition_known: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommanderViewModel {
    pub id: String,
    pub name_key: String,
    pub name: String,
    pub rank: crate::command::Rank,
    pub skill: u32,
    pub attack: u32,
    pub defense: u32,
    pub planning: u32,
    pub logistics: u32,
    pub experience: f64,
    /// None when the imported XP curve is unknown, never a fictional 100 XP.
    pub next_level_xp: Option<f64>,
    pub traits: Vec<TraitViewModel>,
    pub source: Option<hoi4_mobile::command_data::SourceMetadata>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArmySupplySummary {
    pub ratio: Option<f64>,
    pub requested: f64,
    pub received: f64,
    /// Remaining carried supply, weighted by the army demand.
    pub reserve_days: Option<f64>,
    pub bottleneck_reason: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArmyViewModel {
    pub id: ArmyId,
    pub country: String,
    pub name: String,
    pub color: [u8; 3],
    pub division_ids: Vec<u64>,
    pub division_count: usize,
    pub organization_ratio: Option<f64>,
    pub strength_ratio: Option<f64>,
    pub planning: Option<f64>,
    pub supply: ArmySupplySummary,
    pub commander: Option<CommanderViewModel>,
    pub plan_ids: Vec<u64>,
    pub executing_plan_ids: Vec<u64>,
    pub headquarters_division: Option<u64>,
}

pub fn commander(w: &Warfare, id: &str, language: &str) -> Option<CommanderViewModel> {
    let c = w.command.commanders.get(id)?;
    let data = &w.command.data;
    let character = data.characters.get(id);
    let name_key = character.map_or(c.name.as_str(), |c| c.name_key.as_str());
    let name = if character.is_some() {
        data.localization.get(language, name_key)
    } else {
        &c.name
    };
    Some(CommanderViewModel {
        id: c.id.clone(),
        name_key: name_key.into(),
        name: name.into(),
        rank: c.rank,
        skill: c.skill,
        attack: c.attack,
        defense: c.defense,
        planning: c.planning,
        logistics: c.logistics,
        experience: c.experience,
        next_level_xp: c
            .skill
            .checked_sub(1)
            .and_then(|level| w.command.rules.level_thresholds.get(level as usize))
            .copied(),
        traits: c
            .traits
            .iter()
            .map(|id| {
                let definition = data.traits.get(id);
                TraitViewModel {
                    id: id.clone(),
                    name: data.localization.get(language, id).into(),
                    description: definition
                        .map(|t| data.localization.get(language, &t.description_key).into())
                        .unwrap_or_default(),
                    definition_known: definition.is_some(),
                }
            })
            .collect(),
        source: character.map(|c| c.source.clone()),
    })
}
pub fn army(
    w: &Warfare,
    country: &str,
    id: ArmyId,
    language: &str,
) -> Result<ArmyViewModel, String> {
    let a = w.command.own_army(country, id)?;
    let divisions: Vec<_> = w
        .command
        .members(id)
        .filter_map(|id| w.divisions.get(&id))
        .filter(|d| !d.destroyed)
        .collect();
    let count = divisions.len();
    let average = |sum: f64| (count > 0).then(|| sum / count as f64);
    let requested = divisions
        .iter()
        .map(|d| crate::logistics::supply_demand(d))
        .sum::<f64>();
    let received = divisions
        .iter()
        .map(|d| crate::logistics::supply_demand(d) * d.supply_fraction.clamp(0.0, 1.0))
        .sum::<f64>();
    let plans: Vec<_> = w.plans.plans.values().filter(|p| p.army == id).collect();
    Ok(ArmyViewModel {
        id,
        country: a.country.clone(),
        name: a.name.clone(),
        color: a.color,
        division_ids: divisions.iter().map(|d| d.id).collect(),
        division_count: count,
        organization_ratio: average(
            divisions
                .iter()
                .map(|d| (d.organization / d.derived.stats.organization.max(1e-9)).clamp(0.0, 1.0))
                .sum(),
        ),
        strength_ratio: average(divisions.iter().map(|d| d.strength()).sum()),
        planning: average(divisions.iter().map(|d| d.planning).sum()),
        supply: ArmySupplySummary {
            ratio: (requested > 0.0).then(|| received / requested),
            requested,
            received,
            reserve_days: (requested > 0.
                && w.logistics.enabled
                && divisions
                    .iter()
                    .all(|d| w.logistics.divisions.contains_key(&d.id)))
            .then(|| {
                divisions
                    .iter()
                    .filter_map(|d| w.logistics.divisions.get(&d.id))
                    .map(|s| s.supply)
                    .sum::<f64>()
                    / requested
            }),
            bottleneck_reason: w
                .logistics
                .enabled
                .then(|| {
                    divisions
                        .iter()
                        .filter_map(|d| w.logistics.divisions.get(&d.id))
                        .filter(|s| s.received + 1e-8 < s.demand)
                        .map(|s| s.reason.clone())
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .filter(|s| !s.is_empty()),
        },
        commander: a
            .general
            .as_deref()
            .and_then(|id| commander(w, id, language)),
        plan_ids: plans.iter().map(|p| p.id).collect(),
        executing_plan_ids: plans
            .iter()
            .filter(|p| p.activation == crate::operations::Activation::Executing)
            .map(|p| p.id)
            .collect(),
        headquarters_division: w
            .abilities
            .headquarters
            .values()
            .find(|h| h.army == id)
            .map(|h| h.division),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projection_is_read_only_country_scoped_and_supply_is_demand_weighted() {
        let mut w = Warfare::new(crate::military_model::tests::catalogue());
        for id in 1..=3 {
            w.divisions
                .insert(id, crate::military_model::tests::division(id, "A", 1));
        }
        w.divisions.get_mut(&1).unwrap().derived.stats.supply = 1.0;
        w.divisions.get_mut(&1).unwrap().supply_fraction = 1.0;
        w.divisions.get_mut(&2).unwrap().derived.stats.supply = 3.0;
        w.divisions.get_mut(&2).unwrap().supply_fraction = 0.0;
        w.divisions.get_mut(&3).unwrap().destroyed = true;
        let id = w.command.create_army("A", "First", &[1, 2, 3]);
        let revision = w.command.revision;
        let vm = army(&w, "A", id, "russian").unwrap();
        assert_eq!(vm.division_ids, vec![1, 2]);
        assert_eq!(vm.supply.ratio, Some(0.25));
        assert_eq!(vm.supply.received, 1.0);
        assert_eq!(w.command.revision, revision);
        assert!(army(&w, "B", id, "english").is_err());
        assert_eq!(
            serde_json::to_string(&vm).unwrap(),
            serde_json::to_string(&army(&w, "A", id, "russian").unwrap()).unwrap()
        );
        let empty = w.command.create_army("A", "Empty", &[]);
        let vm = army(&w, "A", empty, "english").unwrap();
        assert_eq!(vm.organization_ratio, None);
        assert_eq!(vm.supply.ratio, None);
    }
}
