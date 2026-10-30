# National focus interpreter — recovery checkpoint

Branch: `feature/hoi4-focus-interpreter-1193`. Base: `64b0a66893a96c0deae02fc6be0682da5ee2e8e3`.
Last published ancestor: `a816802e37cf5479ea88be9b1221323fb8a3bb09` (this document is committed after that SHA; use branch HEAD for the latest checkpoint).

## Stage A: architecture audit

- `pdx.rs` preserves ordered entries, duplicate fields, operators and spans. Keep it.
- `research_import.rs` already resolves file layers/replace_path and installation/DLC profile; uses constants and caches. Extend it, do not build a second importer.
- `research_script.rs`: focus-tree records and source AST; shared-focus atom references are currently lost. Tree raw metadata is discarded. Duplicate IDs overwrite silently.
- `focus.rs`: validates OR-within/AND-between prerequisite groups; starts, cancels, ticks at midnight (`session.rs`). No bypass/select effects; ignores scalar lifecycle fields; ties PP spending to progress. Complex selection is rejected; unrelated unsupported selectors can be ignored accidentally.
- `campaign.rs`: bounded, transactional country/state effects; parameterized scripted calls, scheduler and options. Reuse it. Predicates need conditional/wrapper integration; unsupported commands must remain errors.
- `civil_war.rs`: real typed split with safety checks; native arbitrary DLC civil-war scripts still unsupported.
- `campaign_save.rs`: versioned save + content fingerprint and political validation; new focus fields require defaults and validation.
- `focus_ui.rs`: imported-ID list, not a spatial graph. `hoi_assets.rs` provides cached imported textures.
- `dependency_report.rs` and exporter: source inventory only, cannot label execution support or parity.
- Production/construction/research/warfare consumers already receive campaign journal mutations; do not fork mechanics per focus ID.

## Evidence and constraints

Only the assignment attachment was provided. The separate technical research attachment mentioned by it, the user's HOI4 1.19.3 installation/DLC corpus and A51 are unavailable here. No original-content import counts, native-game parity or device FPS can be measured. No Paradox content will be committed.

Baseline source checked against focus readiness gate and political handoff documents. Native git push has no credentials in this environment; publishing uses the authenticated GitHub connector. Remote branch was created successfully before source edits. Rust toolchain installation is in progress; tests not yet run.

## Next implementation

B: preserve tree AST, resolve shared-focus descendants, report duplicate/missing/cyclic references and add import tests. Then implement source-backed lifecycle operations, graph UI and compatibility audit. Keep all unverified DLC mechanisms and unknown commands explicitly blocked. Never merge this branch into main without user authorization.

## Stage B checkpoint

Implemented shared root/descendant resolution without attaching foreign ordinary nodes, retained full tree AST and shared references, diagnosed duplicate IDs instead of overwriting, checked missing references, relative coordinates/conditional offsets with recursion/cycle limits, and reused existing localisation loader for focus/event text. Cache version 8 invalidates old compiled data. New headless module `focus_data.rs` is used by importer and will serve UI/audit.

Checks: `cargo test --lib --locked`: 68 passed. Rust 1.99.1 installed locally. Cargo.lock required regeneration because the existing manifest enables PNG in image 0.25 but lock omitted that dependency; no manifest/dependency upgrades were requested. Original corpus count remains unavailable. Stage B does not certify all dynamic/DLC semantics.

Next: contextual tree scores and lifecycle select/bypass/cancel/pause handling through existing campaign transactions. Import gain_focus static modifier before correcting PP accounting; never assume focus speed equals PP cost.

## Stage C checkpoint

Implemented contextual tree selection through the existing campaign condition evaluator (DLC, scripted triggers, political state, original tag, additive/multiplicative scores). Unsupported candidate selectors block selection rather than silently choosing generic. Tree choice and allow_branch layout are persisted, with explicit mark_focus_tree_layout_dirty and load_focus_tree effects through the journal. copy_completed_from remains explicitly unsupported.

Lifecycle: fractional cost -> floor(point-days); select_effect once at start, automatic/explicit bypass and bypass_effect without completion_reward, bypass history exposed to has_completed_focus; dynamic available/cancel checks; default automatic cancellation, explicit pause/continue, cancel_effect, capitulation gate, source cancelable restriction and select-effect manual cancellation restriction. Paused focuses still pay imported gain_focus.political_power_gain, independent of progress. Missing/extra unsupported gain_focus consumers block instead of inventing PP cost. Completion reward sees focus not yet completed. Failed selection/bypass/completion effects do not partially mutate game state/payment. Cached focus UI selection was minimally wired; graph comes next.

Tests: full cargo test --all-targets --locked passed (counts recorded in next checkpoint). Fifteen focus tests include real 70-day building award, actual country-event/option/flag/next-focus chain, transactional failures and serialize/restore/resume with runtime catalogue reattached, one-time reward, DLC selection, bypass, cancel/pause/continue, independent PP cost and tree switch/layout refresh. These are authored engine fixtures, NOT parity measurements against HOI4.

Known gaps: original starting PP/base daily PP generation not established; idle saved focus progress/refunds not implemented; source speed modifiers, native DLC civil-war adapters, decisions/missions, on_actions, joint and continuous execution and dynamic localization remain incomplete. Schemas consulted: cwtools-hoi4-config at 297dd69c42aabe10078a8b7ab7f2d07ee631d4ef (not proof of 1.19.3 timing). Wiki direct access failed; no third-party code was copied. GPL TreeSnap was inspected for graph concepts only, no implementation reused.

Next: share campaign preflight with headless exporter and emit truthful focus-compatibility-report.json; render spatial tree with cached presentation, gestures and imported icons.

## Stage G (audit infrastructure) checkpoint

Shared `script_validation.rs` is now the exact structural preflight used by BOTH the runtime journal and headless exporter. `focus_compat.rs` reports parsed/resolved, unknown/dynamic predicates, effect blockers (including transitive scripted/event references), unsupported source fields, missing localisation, duplicate IDs, source parse/EOF diagnostics and explicitly unsupported continuous nodes. `supported` is null without runtime evidence; imported-node `tested`/`parity_verified` remain false. This intentionally does not turn successful parsing into fake gameplay coverage.

Continuous palette/focus and joint-focus AST are retained/indexed; continuous execution and joint rewards remain unsupported. Cache version 11. Static localisation retains only imported focus/event/decision text keys, resolving aliases against the full dictionary before discarding unrelated strings. Tree metadata no longer duplicates all source focus AST. Journal uses copy-on-write political state so read-only predicates don't clone the full scheduler and focus history per node.

Checks so far: cargo test --all-targets --locked: 429 passed (70 library + 349 game + 3 exporter + 7 integration), zero failures; Python UI art parser: 4 passed. Android aarch64 cargo check passed on the preceding source revision; final rerun follows UI checkpoint. CLI `audit /tmp/hoi4-focus-fixture` really emitted the JSON: **authored fixture only**: 1 tree, 2 nodes, 1 shared, 2 resolved, 1 structural candidate, 0 native-game parity. These are NOT counts of the user's original installation, which was not provided.

Next: publish/test cached tree UI, then final regression/check/Android compile. After that, continue original-content integration at Stage D/E: supply version-verified local 1.19.3 export, inspect the generated report, implement the first actual missing consumers and source-backed starting PP/daily income. Soviet/DLC parity is not complete.

## Stage F checkpoint — spatial graph (not certified original UI parity)

Replaced ID list with original x/y/relative/conditional-offset graph, prerequisite edges (OR group labels), XOR links, original/localised names and descriptions, current/available/blocked/completed/bypassed states, progress/cancel, selectable detail card with description/reward tabs and scroll, disabled reasons, mouse pan/wheel zoom and one-pointer drag/two-touch pinch. View caches nodes/links/text between state changes, clips offscreen nodes, uses existing bounded texture LRU. The original goal icon symbol resolves through GFX aliases into local `ui/focus/*.png`; existing Python GFX/DDS importer now categorises goal sprites for that cache. No protected assets included.

Branch/coordinate snapshots now persist together and change only on explicit tree load/layout invalidation. Saved layout revision invalidates the view even if no country facts changed. Shared focuses with existing country-specific external parents no longer invalidate the whole tree. Source absent relative coordinate references still error. All focus actions continue through simulation APIs; drawing cannot award effects. Active failures are visible in details rather than hiding a paused job.

Tests: 430 Rust tests (70 lib + 350 game + 3 exporter + 7 integration), zero failures; includes cached presentation/layout invalidation and pointer-anchored zoom/clamping. Python icon parser: 4 passed. Clippy passes with pre-existing and style warnings; not warning-free. Visual rendering on a device/desktop display was NOT verified: no display server/device in this environment. Android aarch64 Rust compile is checked separately; no APK/device/FPS parity claim.

Known UI limits: continuous-focus execution/palette navigation is not implemented; rewards currently show a structured script summary rather than every original game's localised effect tooltip; dynamic localisation/variables/color/icon control sequences are not complete (static aliases and colour stripping supported); conditional icon error diagnostics need further UI work; artwork tool does not yet resolve packaged active DLC/mod layers. Do not label Stage F original UI parity complete.

## Exact handoff / next session

1. Checkout this branch, fetch HEAD, run `cargo test --all-targets --locked`.
2. Get the user's LOCAL version-verified 1.19.3 installation export (no original corpus is in this checkout). `cargo run --bin hoi4-research -- audit data/hoi4` emits `focus-compatibility-report.json`; do not label a different version 1.19.3 with an override.
3. Start Stage D/E with the first actual industrial branch's blockers. In particular starting PP and base daily political income must be derived from original defines/static modifiers/history and integrated with all existing consumers; no guessed zero/free focus.
4. Implement native source-backed missing commands in campaign journal, not per-country focus IDs. Known incomplete examples: `complete_national_focus`, `load_focus_tree.copy_completed_from`, `start_civil_war` adapter to SplitPlan, `set_politics`, dynamic modifiers, original decision/missions and on_actions. Character scopes/arrays/DLC systems remain beyond current interpreter coverage.
5. Add original-installation differential fixtures without committing protected scripts. Test Germany/USSR/etc selection/duration/effects and Soviet paranoia/purges/civil-war distribution against the actual source/engine. Native parity verified so far: zero scenarios.
6. Test final linked Android APK on A51, touch gesture rendering and memory/FPS; cargo check is only Rust compilation.

Do not merge into main. No force pushes or history rewrites have been used. Last published SHA above is a checkpoint ancestor (a commit cannot contain its own SHA); branch HEAD and final report identify the latest published commit.

## Final validation checkpoint

Previous implementation checkpoint: `a816802e37cf5479ea88be9b1221323fb8a3bb09`. This follow-up also fixes transitive event dependency classification, argument-sensitive preflight and audit recursion bounds, and bypass chains which dirty/switch their own tree; it normalises formatting and records final checks. `cargo fmt --all -- --check` passes after formatting the existing main/production/construction/hierarchy source too (format-only changes in those unrelated systems). No mechanics changed there. Final `cargo check --all-targets --target aarch64-linux-android --locked` passed. Final native Rust tests: **434 passed** (73 library + 351 game + 3 exporter + 7 integration), zero failures; Python importer tests: **4 passed**. `cargo clippy --all-targets --locked` passes with warnings; no claim of a warning-free codebase. Local dev/test verification used opt-level=0, debug=0 environment overrides, without changing Cargo profiles.

The original-installation import count, fully executable original-focus count, native HOI4 differential parity and A51 device behavior remain **unmeasured**, not zero-percent claims or fabricated coverage. Synthetic CLI audit counts above are only fixture results. Final source stage is a checked, published incremental interpreter/graph implementation, NOT completion of all acceptance criteria in the assignment.

Final regression additions: parameterized scripted-effect calls cannot hide a later unsupported command; missing parameterized event dependencies affect resolved=false; self-referential trigger audit terminates with explicit budget/depth diagnostics; bypass effects can dirty/switch the tree mid-chain without stale-layout indexing or applying one bypass indefinitely in the same tick. Pass/recursion guards are technical engine bounds, not claimed HOI4 gameplay rules. All final Rust tests, Android check, formatter and Clippy were rerun after these code changes.
