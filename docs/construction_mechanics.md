# Construction system: simulation rules and verification

Updated 2026-10-24. An independent Rust strategy-game implementation, not a copy of Clausewitz.

## Implemented

- Reads **owned local game data** from `data/hoi4/common/buildings/*.txt` and
  `data/hoi4/common/defines/*.lua` when present; no proprietary assets are bundled.
- Building IC cost:
  `base_cost + (existing state/province level) * per_level_extra_cost + controlled count * per_controlled_building_extra_cost`.
  Infrastructure does **not** reduce this recorded IC cost.
- Construction uses civilian factories, not military production efficiency.
  Factories are assigned to queued projects in player-configured state priority order,
  at most 15 per project (or the value imported from defines).
- Multiple queued projects in the same state can advance simultaneously if enough
  civilian factories are available.
- Infra speed multiplier for eligible buildings:
  `1 + min(infrastructure, max_infra_level) / max_infra_level * infra_max_effect`.
  With vanilla 5 infrastructure levels and effect=1, this is +20% per level.
  Imported max infrastructure level is respected.
- Total speed modifiers are additive first, then infrastructure multiplies resulting
  output. Separate country, building-type and state bonuses are accepted.
- Civilian factory daily IC uses 5 at full energy (default) and 4 at zero energy,
  interpolated by `ConstructionModifiers.powered_fraction`.
  Positive construction-speed bonuses are also scaled by energy satisfaction.
  Until the energy system is implemented, the default is **100% powered**.
- Consumer-goods percentage is computed as additive expected values times
  multiplicative factors, with configurable minimum and floor-rounding of factory
  count as explained by Paradox in the 2023 economy diary. Trade reservations reduce
  the remaining construction CIV pool.
- Two **manually selected example economy-law presets** expose 35% and 20% consumer
  goods; no starting law is inferred from a country's tag.
- Shared building slots and non-shared infrastructure level limits, queue-item cap,
  cancellation, construction priority, ETA and day-boundary completions are simulated.
- Unit tests cover costs, queue limits, allocations, infrastructure, laws, negative and
  positive modifiers, energy, cancellation, parser syntax and ETA.

## Important limits (not yet 1:1)

- **Nation-specific laws, trade imports, technology, focuses and ideas are not
  imported or applied automatically.** Laws currently selected in the UI are
  demonstration presets, not authoritative country starting settings.
- A queue is grouped by state; a fully independent global project-level drag-and-drop
  ordering has not been implemented. Individual queued items are nevertheless separate
  lines for assigning civilian factories.
- Provincial buildings (coastal/land forts, naval bases), airbases, radar, railways,
  supply hubs, repairs, conversions, captured/faction factory use, and all new building
  conditions/tech gates need their own data model and UI.
- `powered_fraction` is an injected input; country power generation, dams, coal,
  plants, demand and the October 2026 open-beta energy revisions are **not implemented**.
  The energy ratio logic is provided so those mechanics can be integrated later.
- Military factory **equipment production efficiency**, naval shipbuilding limits,
  equipment resource shortage, DLC-specific designers and repairs are **separate**
  systems and must not be folded into civilian building construction speed.
- Building defaults and law presets are fallbacks that should be overridden with data
  from the target HOI4 version. Behavior varies between vanilla patches and mods.
- Queue cap (1000) currently follows user research, not an independently verified
  game-engine limit; make it data-driven once a reliable versioned limit is available.

## Code entry points

- `BuildingRules::load`: load cost and infrastructure definitions.
- `enqueue_project`, `cancel_project`: validate and edit state project queues.
- `country_capacity_with_goods_modifiers`: consumer-goods / trade CIV budget.
- `allocate_factories`: schedule individual active construction lines.
- `daily_construction_points`: compute each line's daily IC progress.
- `advance_day_with_modifiers`: deterministic day boundary and completions.
- `estimated_days_remaining`: calculate ETA given assigned factories and modifiers.

## Reference checks

- Paradox 2023 small-features diary (factory consumer-goods floor and factors):
  https://devtrackers.gg/heartsofiron/p/e46d4a63-developer-diary-small-features-1
- Paradox 2025 thermodynamics diary (4/5 civilian output and energy modulation):
  https://devtrackers.gg/heartsofiron/p/78305ac2-dev-corner-thermodynamics
- October 2026 energy open beta (still version-specific):
  https://steamdb.info/patchnotes/25750229/
- Community building modding notes (costs, slot types and flags):
  https://github.com/thelight0211/hoi4-wiki/blob/main/Building%20modding%20-%20Hearts%20of%20Iron%204%20Wiki.md
