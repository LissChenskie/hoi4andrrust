# HOI4-inspired research system — implementation and parity contract

**Status:** runnable first version in Rust. Not yet a one-to-one reconstruction
of the closed-source HOI4 research engine. This work follows the user's
research brief for the 1.19.x family; **exact 1.19.3 rules need comparison with
a matching local game installation**. The complete vanilla technology tree,
DLC content, localisation and art are **not distributed** with this repository.

## User workflow

1. Start a country in the map viewer.
2. Open **TECH** on compact screens, **RESEARCH** on desktop (or press `R`).
3. Navigate research folders with the folder arrows and the technology graph
   with the graph's horizontal and vertical arrows / mouse-wheel scrolling.
4. Choose a technology. The panel shows its research year, base cost,
   prerequisites and a calculated ETA for a free slot.
5. Press **RESEARCH IN SLOT**. A slot shows progress and current estimated
   completion date. **STOP** cancels research (does not refund consumed
   limited-use bonuses).
6. Empty research slots accumulate saved days, up to the configured maximum
   (default 30 days).
7. The in-game calendar advances research once for each simulated day.
   Opposing countries have basic AI research that picks new eligible techs
   on a weekly cadence. This is **not** the full HOI4 research AI.

## External content

The optional data-loader reads from **user-supplied** files:

```text
data/hoi4/common/technologies/*.txt
data/hoi4/common/defines/*.lua
data/hoi4/history/countries/<TAG> - *.txt
data/hoi4/localisation/english/**/*.yml
data/hoi4/localisation/russian/**/*.yml
```

- Local technologies are read from `technologies = { ... }` blocks, with
  `research_cost`, `start_year`, `categories`, `folder`, `path`,
  `dependencies`, `xor`, `allow`, `allow_branch`, `ai_will_do.factor`,
  simple numeric `modifier` fields and elementary `enable_*` references.
- `path.leads_to_tech` is converted into a prerequisite across files.
  `research_cost_coeff` and advanced path semantics are **not yet modeled**.
- A catalogue validator checks missing references, duplicates, self-edges
  and dependency cycles. On import failure the game displays a visibly
  labelled **small demo tree** rather than claiming all vanilla techs exist.
- Local defines can override `BASE_TECH_COST`,
  `BASE_YEAR_AHEAD_PENALTY_FACTOR`, `BASE_RESEARCH_POINTS_SAVED`.
  The fallback values are 110 research-days, 2 per year and 30 saved days,
  corresponding to the baseline described in the supplied research report.
  Changes in future game builds require a different version snapshot.
- `set_research_slots` and initial `set_technology` entries are loaded
  from local country-history files. In their absence, three slots are used
  for the playable prototype; this default is **not** a historical claim.
- The app does not automatically import proprietary Paradox content from
  GitHub, the internet, or a Steam library. The player must provide data
  they have rights to use. Android's Storage Access Framework import
  remains future work.

## Timing model

For an active technology on a calendar day:

```text
work_required = research_cost * base_tech_cost
ahead_years = max(0, fractional_years_until_start - ahead_reduction)
daily_work = max(0.1, 1 + additive_speed_modifiers + limited_bonus + sharing_bonus)
             / (1 + year_ahead_factor * ahead_years)
```

- The ahead-of-time penalty is recalculated on every game day.
- Saved days are applied when a technology is assigned to an empty slot.
- Limited-use bonus charges are consumed at the start and their values are
  fixed for that research. Ordinary speed modifiers are continuously used.
- Estimated completion is calculated by replaying the daily research loop
  with calendar advancement, not by multiplying a fixed early-year penalty.
- The default timing baseline is version-configurable. **Bit-for-bit
  matching is not proven**, particularly for day fractions, how finite focus
  bonuses combine, sharing locking, and other native C++ engine details.

## Technology restrictions and grants

Research supports mandatory parents, exclusive branches, country flags,
direct tech ownership, AND/OR/NOT predicates, direct focus/event grants
and generic unlock/modifier metadata. Unknown scripts are deliberately
**not treated as satisfied**; this prevents bypassing a restricted branch.
The engine records granted unlocks and numerical modifiers but **does not
yet** apply every effect to industry, units, diplomacy or politics.

## Known gaps before calling this "1:1"

- Full Clausewitz scripted trigger/effect interpreter and `on_research_complete`.
- Exact build-specific bonus order, technology sharing group rules,
  ahead-of-time day fractions and MIO/idea/focus integration.
- Experience-based doctrines, specialised XP mechanics and DLC-specific
  research categories.
- Historical AI strategy plans, category weight tables and stochastic
  weekly candidate-selection logic (basic AI now uses simplified weights).
- Exact branch layout and technology icons/illustrations.
- Production unlock enforcement, unit upgrades, full research-effect
  application, country-state saving/loading, multiplayer and Android QA.
- Full vanilla/DLC datasets, which must come from local sources of the
  intended game version, not invented placeholder tech names.

## Automated tests

The Rust unit suite includes simultaneous slots, research-year penalties,
days saved, focus-only restrictions, XOR and dependencies, additive speed
bonuses with 10% floor, grants and unlock metadata, dates/ETAs, data import
and versioned defines. Use `cargo test`.

Further high-fidelity development should add reference snapshots obtained
from a version-pinned local HOI4 install and golden timing tests for specific
techs/scenarios, plus real UI tests on Android.

Further reading: [CWTools configuration schema](https://github.com/cwtools/cwtools-hoi4-config)
is a description of the game's script format, not its C++ research logic.
