# Expert AI 5.0 and Sheep's Mod — what the supplied sources show

Sources: the two archives supplied with the task, extracted completely in this session
(`expert-ai-8_08_26.rar`: 660 files, `sheeps-mod-13_02_26.rar`: 803 files).
Both are **Paradox script**, not engine code. Consequently they define *inputs to* the original
Clausewitz AI (flags, variables, `ai_strategy` weights), while the part that finally places factories,
moves divisions and picks targets lives in the closed original engine and is not available.
Algorithms below are therefore re-implemented from observed behaviour, not translated from code.

No code from either mod is copied. This repository is MIT-licensed; neither archive contains a licence
file that was checked in this session, so nothing is vendored.

## Confirmed by reading files

### Expert AI

* Scheduling: `EAI_UPDATE_effects.txt` has `EAI_hourly_update` (iterates `every_country` with
  `is_ai = yes`, calls `EAI_PC_hourly_update`) and `EAI_daily_update`, which guards itself with a
  one-day global flag so the daily pass runs once, then runs per-country updates. Takeaway:
  **layered cadences** (hourly / daily / slower) with a guard against re-entry.
* Research (`EAI_RESEARCH_PRIORITY_effects.txt`): a **priority queue** per country.
  When `researched_techs^num` increases the queue is cleaned and rebuilt; a tech is selected from the
  sorted queue; research is *blocked* for a few days when all slots are busy so that chained techs
  (e.g. `concentrated_industry2 → 3`) are picked in order instead of by the engine. Separate tree
  files exist for industry, infantry, air and navy (`EAI_RESEARCH_PRIORITY_TREE_*.txt`).
* Military focuses (`EAI_MILITARY_FOCUS_update.txt`): per country, after a config phase, three groups of
  focuses are re-evaluated (land: doctrine, infantry/armor/motorized/AT design and width,
  attachments, special ops; air: fighter … maritime patrol, doctrine; navy: destroyer … carrier,
  submarine). This is the "AI Army Focus" idea: a country has a **military specialisation** that
  drives templates, production and research together.
* Production (`EAI_PRODUCTION_strategy_effects.txt`): sets or clears *flags* such as
  `EAI_MW_armor_army_production_strategy` when `num_of_military_factories > 20` and the corresponding
  armour production condition holds. `common/ai_strategy/EAI_PRODUCTION_*` then turn the flags into
  weights. So production steering is **hysteresis on thresholds**, not exact planning.
* Strategy plans (`ai_strategy_plans/*_historical_strategy_plan.txt` etc., 59 files):
  `allowed` (tag) + `enable` (historical focus on, game rule, DLC, flags) + `abort` (world conditions
  that invalidate the plan, e.g. Sweden going communist, USSR no longer communist) + strategies.
  This is the "if conditions hold, evaluate the plan; abort when the world changes" pattern the task
  asks for instead of hard dates.
* The mod carries its own map tables (`EAI_MAP_province_neighbors.txt` ≈79k lines, terrain, rivers,
  railways, coordinates) to run **pathfinding in script**. Our engine already has `map_graph.rs`,
  so this data is not needed.
* Strategy-type frequency over `ai_strategy` + `ai_strategy_plans` (counted with `grep -o`):
  `build_building` 6023, `invade` 1301, `diplo_action_desire` 545, `building_target` 532,
  `send_lend_lease_desire` 439, `strategic_air_importance` 429, `dont_defend_ally_borders` 398,
  `send_volunteers_desire` 383, `equipment_production_min_factories_archetype` 148,
  `naval_avoid_region` 142, `put_unit_buffers` 134, `role_ratio` 122, `front_control` 96,
  `alliance` 94, `front` 92, `front_unit_request` 74.
  Construction and invasion dominate the content.

### Sheep's Mod

* Mostly a **content mod**: 76 files in `common/ai_strategy`, plus events, focus trees, ideas, characters,
  technologies, peace-conference definitions, game rules and many per-country files (`LSM_*.txt`).
  The AI part is declarative per-country strategy weights.
* Strategy-type frequency in its `ai_strategy` + plans: `build_building` 411, `role_ratio` 400,
  `equipment_production_factor` 375, `unit_ratio` 304, `alliance` 228, `equipment_production_min_factories` 194,
  `front_unit_request` 186, `area_priority` 178, `factory_build_score_factor` 152, `research_tech` 132,
  `front_control` 128, `diplo_action_desire` 126, `building_target` 119, `front` 117, `antagonize` 96,
  `protect` 93, `research_weight_factor` 90, `template_prio` 81, `put_unit_buffers` 80,
  `befriend` 77, `send_volunteers_desire` 75, `invasion_unit_request` 73.
  Compared with Expert AI it spends far more rules on **unit mix (`role_ratio`, `unit_ratio`, `template_prio`),
  production factors, research weights and `invasion_unit_request`**, and less on construction.
* Example verified in `LSM_AI.txt`: conditional rules such as "if threat > 0.80 or at war, raise
  military-to-civilian factory ratio" and "UK/USA with ≥100 civilian factories, a large fighter force
  after a date: raise `atomic_research` weight". Pattern: `enable` / `abort` conditions around a weight.
* `invasion_unit_request` and `invade` strategies exist in `GER.txt`, `ENG.txt`, `FRA.txt`, `ITA.txt`,
  `JAP.txt`, `CHI.txt`, `LSM_AI.txt`, `LSM_FIN.txt`, `LSM_VIC.txt` (grep result). The *coordination
  logic* of amphibious operations is not in these files; they only request units for an invasion.

## Not confirmed / hypotheses

* That Sheep's Mod "coordinates allies", "prepares joint operations" or "adapts to changing war": these are
  descriptions from the task statement. The supplied files show weights and conditions, not a
  coordination algorithm. Anything we build for this is our own design, labelled as such.
* Any exact numeric balance beyond the examples quoted here. Values are tuned for the original engine's
  AI scoring and must not be copied blindly.
* Behaviour of Workshop revisions newer than the supplied archives.

## What is adapted, and how

| Idea | Origin | Planned Rust form |
|---|---|---|
| Layered cadences with re-entry guard | Expert AI update effects | `ai::scheduler`: per-layer next-due game hour stored in `AiState` |
| Research priority queue, rebuilt on completion, chain ordering | Expert AI research | `ai::research`: ranked candidates, rebuilt when the researched set changes |
| Military specialisation drives research, templates and production | Expert AI military focuses | `AiProfile.specialisation` consumed by research, production, recruitment |
| Threshold hysteresis for production lines | Expert AI production flags | enter/exit thresholds, to avoid flip-flopping |
| `enable` / `abort` conditions for plans | Expert AI plans, Sheep's `enable`/`abort` | `Goal` with `enabled_when` / `abort_when` evaluated on observations |
| Unit mix targets | Sheep's `role_ratio`, `unit_ratio`, `template_prio` | targets used by recruitment |
| Conditional factory ratio, research weights | Sheep's `LSM_AI.txt` | utility terms over threat / war state |

Conflict resolution: terms are not summed. Each decision domain computes a utility per candidate from
shared observations (threat, resources, capacity, war state), and the highest utility wins.
Budgets (factories, build slots, research slots) are allocated greedily by utility so competing
priorities share one limited pool.
