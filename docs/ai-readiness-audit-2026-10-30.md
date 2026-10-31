# AI readiness audit — 30 October 2026

Base: `main` @ `64b0a66893a96c0deae02fc6be0682da5ee2e8e3`.
Work branch: `feature/expert-sheep-ai-20261030`.
Toolchain used for verification: rustc 1.99.1 (conda-forge), Linux x86-64. **No Android device or ARM64 build was available.**

Scope of this document: what was actually verified by reading code and running commands in this session.
Statuses use the requested vocabulary (READY / PARTIAL / BLOCKED / UNVERIFIED). Where a status
rests only on reading code, it says so. No percentages are given.

## Verified facts about the baseline

| Fact | Evidence |
|---|---|
| Crate compiles with `cargo check --all-targets` (warnings only, 22 in the test target) | command run this session, exit 0 |
| There is **no `src/ai/` module** and no strategic AI | `grep` for `ai`/`auto` over `src/` finds only the items below |
| Only existing AI-like code: weekly research pick | `ResearchWorld::ai_choose`, `src/research.rs:1170`, called from `src/session.rs` daily tick for every non-player country when `day_index % 7 == 0` |
| Only other automation: front/plan ticks that run for **every** country, player or not | `Warfare::tick_hour`, `src/warfare.rs:2810`: `fronts.tick` and `plans.tick` with `automatic_order = true` |
| `tick_hour` receives `player: Option<&str>` but the front/plan ticks do not use it to separate AI from human | `src/warfare.rs:2810-2850` |
| A legacy primitive attack AI exists and is on by default | `Warfare::ai_enabled` (`warfare.rs:68,113`), used at `warfare.rs:3070`: every 6 game hours it takes up to 8 non-player divisions that are not in any army, at `Hold`, adjacent to an enemy-controlled province, and calls `attack`. It has no force concentration, supply or threat logic. It must be replaced by the operational planner (not yet done, see progress doc) |
| Country research entries created lazily by the daily tick get **0 slots** | `ResearchWorld::country_mut(tag, 0, rules)` in `session.rs` daily tick. Slots normally come from the importer (`research_import.rs:1157`). A country first seen mid-game (e.g. civil-war split) would research nothing. UNVERIFIED whether civil_war.rs pre-creates the entry |
| Baseline is not `cargo fmt` clean | `cargo fmt --all` rewrites `production_ui.rs`, `construction_ui.rs`, `army_hierarchy_ui.rs`, `construction.rs` and parts of `main.rs`. The required `cargo fmt --check` gate cannot pass without an unrelated formatting commit |
| Baseline tests are green | `cargo test --bin hoi4-mobile`: 337 passed; lib 66, research bin 3, `tests/command_data.rs` 7 |
| Real HOI4 1.19.3 data is **not in the repository** (`data/` absent) | `ls data` fails; the focus gate doc confirms the importer needs the user's installation |
| Focus system exists but is explicitly not full parity | `docs/focus-readiness-gate-2026-10-30.md`: "No strategic AI changes", "Not ready to claim complete generic 1.19.3 parity" |

Consequence of the last row: campaigns driven by real 1.19.3 data cannot be run from this environment.
All integration tests added by this work must use synthetic fixtures. They prove functional
correctness of the AI plumbing, **not** numeric accuracy against the original game.

## Command layer (requirement 2.1)

There is no unified player/AI command bus. Player actions are separate methods on separate
subsystems, for example: `CommandState::create_army` / `set_general` (`src/command.rs`),
`CountryProduction::create` / `change_requested` (`src/production.rs`), `enqueue_project`
(`src/construction.rs:852`), `queue_recruitment` (`src/recruitment.rs:19`),
`GameSession::begin_focus` (`src/focus.rs:243`), `OperationalState::create` (`src/operations.rs:138`),
`PeaceSystem::submit_demand` (`src/peace.rs:577`).
These methods validate ownership/cost themselves and are reusable by an AI, so the AI can call them
instead of mutating fields. A thin `ai::commands` adapter that logs every issued command and its
`Result` is therefore the minimal correct design, rather than a second command system.
Status: **PARTIAL** (APIs exist and validate; no common recording/replay layer).

## Readiness matrix

"Evidence" lists the code that was read. "Tests" is the count of `#[test]` attributes in the module
(from `grep -c`), which is a count of tests, not a measure of coverage.

| Subsystem | Status | Evidence / reason | Tests (count) |
|---|---|---|---|
| Economy / resources / trade | PARTIAL | `resource_economy.rs`, daily `plan_automatic_trade` in `session.rs`. Reads fine; whether AI trade decisions are needed beyond the automatic planner is unverified | 7 |
| Construction | PARTIAL | `enqueue_project`, `cancel_project`, per-country `CountryConstructionPolicy`. Queue/cost/speed logic exists. Needs AI-callable "list candidate projects per state" query (to be added in `ai::observation`) | 28 |
| Production | PARTIAL | `CountryProduction::{create, change_requested, switch_model, allocations}`, resource-pool tick. Real consumption paths exist; stockpile demand forecast does not | 12 |
| Research | PARTIAL | Real prerequisites/XOR/slots/cost in `research.rs`. Existing `ai_choose` ignores production, doctrine, threat, country profile, and is a pure `ai_weight / (1 + 4·ahead)` ranking (`research.rs:1170-1199`) | 15 |
| Recruitment / training | PARTIAL | `queue_recruitment`, `deploy_recruitment_early`, equipment fraction. No AI-side demand model | module has few direct tests; UNVERIFIED for large-scale use |
| Land army / command | PARTIAL | `CommandState` armies, groups, theaters, commanders with caps. Creation APIs exist; no AI organises them | tests in `command_data.rs` / `tests/command_data.rs` |
| Land combat / rules | READY for use as an AI substrate (read-only for AI) | `land_combat.rs`, `combat_rules.rs` | 9 / 6 |
| Fronts / operations | PARTIAL | `OperationalState::{create, draw_order, edit_line, activate}`, `BattlePlan`, `ExecutionMode`, autoplan tests for empty neighbours and half-pockets (`operations.rs:1010,1083`). These already run for every country; the AI must drive the same objects rather than add a rival system | 14 |
| Logistics / supply / fuel | PARTIAL | `logistics.rs` hourly tick, reserve trucks, prepare_reinforcements | 3+ (UNVERIFIED depth) |
| Air | PARTIAL | `create_wing`, `assign`, `rebase_and_assign`, `support`, fuel via `replenish`. No AI mission planner | few |
| Navy | PARTIAL | `naval_warfare.rs`, `naval_transport.rs`, `naval_carrier.rs`, convoy reservation. No AI task-force or invasion planner | 8 / 7 |
| Politics / laws / advisors | PARTIAL | `CampaignState` country politics; focus doc states political power import must be verified, else begin is blocked | 17 (`campaign_tests`) |
| National focuses | PARTIAL | `GameSession::begin_focus/tick_focuses` with preflight; DLC selectors, bypass, select effects not complete (per repo's own doc) | 2 direct + campaign tests |
| Diplomacy / factions | PARTIAL | `PeaceSystem::{create_faction, join_faction, leave_faction, call_to_war, sync_wars}`. Wars are declared via `Warfare.wars`; no AI war-goal logic | 6 |
| Peace conference | PARTIAL | `surrender_status`, `submit_demand`, `finalize` exist; no AI demand chooser | 6 |
| Script engine (triggers/effects/variables/on_actions) | PARTIAL | Transactional interpreter in `campaign.rs` (`campaign_effects`, `campaign_conditions`); repo doc says "conservative predicate handling" | 17 |
| Save / load | PARTIAL | `campaign_save.rs`; `GameSession` is `serde`-derived so any AI state added as a field with `#[serde(default)]` is saved. Determinism after load is UNVERIFIED for the whole game; will be tested only for AI state | 9 |
| Determinism | PARTIAL | Docs/code state stable tag order (`BTreeMap`), country-tag ordering in `session.rs`. `BTreeMap`/`BTreeSet` are used throughout the files read | UNVERIFIED globally |
| Android | UNVERIFIED | No device, no ARM64 target installed here | — |

## Chains required by the task (section 1.3)

| Chain | Finding |
|---|---|
| Economy → production → stock → reinforcement → division strength | Code paths exist (`tick_country`, `prepare_reinforcements`, `tick_recruitment`); the repo's `docs/military-economy-fixes-2026-10-28.md` documents earlier fixes. Not re-proven end to end here. UNVERIFIED |
| Research → unlock → design → production → combat | Research and division designer exist. No automated test in this session proves a researched item reaches combat. UNVERIFIED |
| Construction → factories → capacity | `country_capacity*` in `construction.rs`; construction has the largest test set (28). PARTIAL, plausible |
| Focus → effects → systems | Effects run through the campaign interpreter, subject to its documented limits. PARTIAL |
| Capture province → front recompute → plan | `world.control_revision` change triggers building-slot refresh in `session.rs`; autoplan tests cover empty neighbours. PARTIAL |
| Capitulation → conference → new strategies | `peace.rs` implements conferences; nothing recomputes AI goals afterwards because there is no AI yet. BLOCKED for the AI part until profile re-evaluation exists |

## Blockers and plan

1. **No AI layer and no per-country AI state** → create `src/ai/` with a persisted `AiState` field on `GameSession` (`#[serde(default)]`).
2. **No AI/human separation in front & plan ticks** → introduce a single owner: when a country is AI-controlled the new planner issues orders via `OperationalState`; the legacy automatic ticking must not run a second independent policy for the same divisions. To be handled when integrating (Phase 7), with a test.
3. **No recording of AI commands** → `ai::commands` wrapper that calls existing validated APIs and records `Result`.
4. **Real data unavailable here** → synthetic fixtures; documented limitation.
5. **Focus/political prerequisites incomplete** → AI may only start a focus that `GameSession::begin_focus` accepts; blocked focuses are skipped with a diagnostic, never forced.

## Expert AI / Sheep's Mod: what was actually read

Archives extracted completely: Expert AI 660 files, Sheep's Mod 803 files (counts match `libarchive`
listings). Findings are in `docs/ai-source-analysis.md`.
