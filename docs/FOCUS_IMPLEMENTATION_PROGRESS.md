# National focus interpreter — recovery checkpoint

Current state: Stage E country decisions/missions and startup/periodic on-actions implemented, connected to the actual game panel, and covered by a conservative campaign-script audit. Full original-game/DLC parity remains unverified.

Branch: `feature/hoi4-focus-interpreter-1193`. Base: `64b0a66893a96c0deae02fc6be0682da5ee2e8e3`.
Last published ancestor: `1f7a3b4423845e90ac8562ac511c6966f6c41569` (this document is committed after that SHA; use branch HEAD for the latest checkpoint).

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

## Continuation: daily political ledger

Base published checkpoint: b49e283198d84bae4154a1b2b4c35f170675caea. Current stage: D — political income and campaign effects.

- Corrected native static modifier import to `common/modifiers/` (legacy `common/static_modifiers/` still accepted); retained imported literal defines in ResearchScripts. Cache version 12.
- New `politics.rs`: country PP account begins at the engine's zero-before-history baseline, then ordered history set/add effects execute. DEFAULT_STABILITY/DEFAULT_WAR_SUPPORT come from imported NCountry defines. Unknown/nonliteral history amounts report an error rather than silently zeroing them.
- Daily income uses imported NPolitics.BASE_POLITICAL_POWER_INCREASE, country modifier gain/factor/cost, imported static stability good/bad interpolation and active gain_focus scalar/hidden modifiers. Caps are read from NCountry; no default +2/day, -1 focus/day, -500/+2000 caps are hardcoded. Country starting balances are not hardcoded.
- One saved per-country day marker prevents duplicate daily credit/debit across save/load. The main daily session tick credits the ledger before progressing focuses; focus progress no longer charges PP a second time. Missing rule inputs block income and active focus progression.
- Existing political-power modifiers on ideas now have an actual consumer. Top bar displays real PP/stability/war support and income breakdown, replacing those three placeholders.

Checks: `cargo test --all-targets --locked`: 439 passed (74 library + 355 game + 3 exporter + 7 integration). New fixtures cover native modifier directory, defines in cache, ordered starting history, missing defaults, additive/percentage/cost composition including focus, caps, once-per-day/save behavior and failed data blocking progression. These synthetic values are not original 1.19.3 data.

Limits: full effective stability composition from ruling-party popularity, war/characters and explicit difficulty selection is still incomplete; the ledger currently consumes stored stability plus existing registered national PP modifiers. No claim that all country's displayed original PP income matches. Initial zero-before-history semantics and formula ordering require final differential verification on the user's source version. Existing saves with unknown political balances are not silently reset; start a new campaign to import starting history or restore explicitly known values with set_political_power.

Next: transactional complete_national_focus, native focus triggers/copy_completed_from, explicit political setters and set_politics; then source-backed decisions/dynamic modifiers.

## Continuation: native focus/political effect dispatch

Parent checkpoint: 7a46f245c8223248f99613aefc8b65276bbe7475. Added transactional `complete_national_focus` scalar/block: actual reward executes before marking completion, bypasses prerequisite/availability like an explicit completion command, stops only the matching active focus, rewards once, and rejects recursive completion. One nested failure rolls back the whole journal. Preflight and compatibility analysis follow the forced-completion reward transitively (including missing events/unknown predicates).

Added native `has_focus_tree` and `focus_progress` triggers; `load_focus_tree.copy_completed_from` resolves real country scopes (including FROM), copies only matching target-tree history/bypass flags, leaves source intact and does not copy an active job or grant rewards. Added political set effects, has_political_power/has_stability/has_war_support aliases; `set_politics` supports ruling party, elections flag and party-name keys; `set_popularities` validates native percent totals and stores fractions for ideology-support triggers. has_elections reads actual state. Invalid popularities are rejected atomically; saves validate the new numeric context.

Checked source: generated game effect/trigger documentation in CWTools and its schemas. Election timer fields are explicitly unsupported at runtime until elections/on_actions actually exist; characters are not silently appointed by changing an ideology string. This is not a full election/leader system. Party-specific dynamic references and full native political stability formulas still need expansion.

Tests: cargo test --all-targets --locked: 443 passed (74 lib + 359 game + 3 exporter + 7 integration). Includes forced completion ordering/once behavior and active cancellation, nested failure and recursive rollback, native progress/tree predicates, copying FROM, actual government/popularity conditions and invalid popularity rollback. Next: real timed national dynamic modifiers with variable refresh, enable/remove triggers, expiry/save behavior and existing research/economy/military consumers; then decisions/missions and on_actions.

## Continuation: timed national dynamic modifiers

Parent checkpoint: 0a0efcac1f9715dcdf591641cd670f7f7bcab4d7. Current stage completed: source-driven **country-scoped** dynamic modifiers. Cache version 14 imports `common/dynamic_modifiers` into the existing catalogue.

Implemented actual add/remove/has_dynamic_modifier operations, optional positive integral days, saved absolute expiry, renewal instead of stacking, literal/country-variable/ROOT/FROM/PREV numeric references and explicit variable fallbacks. enable/remove_trigger conditions use the real campaign interpreter. Values enter the existing source_modifiers registry and reach research, production/construction and military consumers; unknown consumers, attacker/targeted/state/character scope forms remain explicit blockers. National modifier values are recomputed at the daily simulation boundary; expiry is removed on the hourly campaign scheduler so a modifier added mid-day cannot retain stale bonuses until the next midnight. Native refresh cadence still requires a differential check on the user's version.

Independent dynamic history effects deferred by the research-only importer replay once after military initialization. Conditional unknown history wrappers are preserved, never flattened into unconditional rewards. If a historical dynamic effect remains unresolved, or refreshing a dynamic coefficient fails, PP income is blocked rather than silently granting an optimistic balance. Failed refresh removes stale cached dynamic bonuses and records the reason on the active modifier. Compatibility report follows dynamic definitions/condition dependencies.

Civil-war consistency: typed split currently rejects active dynamic modifiers until it has an explicit native inheritance policy, preventing cloned nonexpiring orphan buffs. This is a **known blocker**, not completed DLC civil-war inheritance. Split preserves the political day marker; settlement clears losing-side dynamic/income/active-focus state. No native start_civil_war adapter or Soviet-specific behavior has been invented.

Tests: `cargo test --all-targets --locked`: **451 passed**, 0 failed (75 lib + 366 game + 3 exporter + 7 integration). Includes actual PP/research/military modifiers, variable refresh, enable/remove, unknown consumer/target scope rollback, renewal/save/expiry, hourly expiry away from midnight, unresolved-history handling, and explicit split inheritance rejection. `cargo fmt --all -- --check`, `git diff --check`, final `cargo check --all-targets --target aarch64-linux-android --locked` passed. Final `cargo clippy --all-targets --locked` passed with warnings; warning-free status is not claimed. Same local opt-level=0/debug=0 overrides, no Cargo profile modifications. No original corpus comparison, APK linking, A51 rendering or FPS benchmark was performed.

### Next precise task

Continue Stage E with **native decisions/missions and on_actions**, reusing the existing transactional journal and scheduler, with real PP payment, availability/recheck, completion/removal/timeout and persisted once-only processing. Then wire effective stability components, explicit difficulty/leader/advisor political modifiers, election timers and native dynamic-modifier civil-war inheritance using the user's exported source data. Do not reset old unknown PP balances, fabricate missing script outcomes, merge this branch into main, or claim complete DLC/native parity.

## Final continuation regression: guarded political history

Parent: 4978fb9d2d858fa99fdcd20d58be313caa5bb0a2. Political history now resolves with the selected DLC facts and source default laws using the existing ordered/dated resolver and parameterized scripted-effect expansion. An unverified DLC selection (including gates inside a scripted effect), unknown history predicate or unresolved expansion blocks the starting political balance explicitly instead of silently selecting a false branch and presenting zero as known. This remains conservative: unsupported historical selectors must be implemented before claiming a matching country balance.

Final checks after this change: **452 Rust tests passed**, 0 failed (75 library + 367 game + 3 exporter + 7 integration); format/diff checks and Android aarch64 all-target Rust check passed. Clippy passed with warnings. Native input corpus counts, original-game differential scenarios and A51 device rendering/performance remain unavailable. No main merge, force push or history rewrite.


## Continuation: country decisions/missions and additive on-actions

Parent checkpoint: 0e2b382d1dd8fdb2bd1fc7918d2344cf0ba77a69. Cache version 15 retains full decision/category AST, including missions with no complete_effect. Repeated on-action names and repeated effect blocks are additive in source load order instead of silently overwriting earlier definitions. Duplicate decision/category IDs are diagnosed and blocked.

The existing journal now commits decision payment, history, timers, RNG and effect changes atomically. Country decisions have start-only allowed snapshots, live visibility/availability/custom-cost checks, actual PP payment, persistent active state, registered national modifiers, remove/cancel callbacks, cooldown and fire-only-once. Missions support activation gates or explicit activate_mission, automatic vs selectable completion, timeout, cancel, explicit no-reward removal and timer adjustments. Mission completion can transition to days_remove; remaining days are available as days_mission_timeout@ID/days_decision_timeout@ID, including inside the terminal callback. Removing an old job before its terminal callback permits a source script to reactivate the same mission safely. Failed callbacks retain a blocked job, grant no partial reward and never repeatedly debit PP. Blocked bonuses are detached; PP refuses income from a blocked active decision.

Decision callbacks use a saved per-country/decision/phase seed by default (fixed_random_seed=no uses campaign RNG). This reproduces stable repeated outcomes, not the original binary's exact random sequence. Scripted starting/ending counters, defaults, allowed evaluation ordering, cancellation/expiry precedence and source timing still require differential tests on the user's version. Only positive integral or infinite (-1) durations are currently accepted; unsupported zero/fractional timers block explicitly. Targeted/state decisions, targeted modifiers, war preparation/notification fields, balance of power and automatic AI decision choice remain unsupported; no country/decision IDs are hardcoded. Civil-war splits reject active decisions until a native inheritance policy exists. Older saves without an allowed snapshot evaluate it on first use; matching original old-save snapshots cannot be reconstructed without evidence.

On-actions: native on_startup runs once with no implicit country scope on a new campaign; on_daily/on_weekly/on_monthly and their _TAG forms enqueue every imported block in stable order. Dispatch markers, queue, blocked reasons and RNG survive saves. Random-event lists check real event triggers/fire-once and enqueue real existing campaign events; a 0 entry means no event. Hourly execution is bounded by the existing scheduler budget. The numeric weekly schedule and event eligibility/weight ordering remain unverified against the original engine. Other hooks (war, civil war, characters, political changes etc.) are retained but not bound to simulation yet. Changing ruling party with imported on_ruling_party_change hooks is explicitly blocked until immediate dispatch/old_ideology_token semantics exist.

Source consulted: CWTools schema/generated effect docs, checked-out revision 297dd69c42aabe10078a8b7ab7f2d07ee631d4ef, https://github.com/cwtools/cwtools-hoi4-config . No third-party implementation or protected game content copied. This is a reconstruction, not native parity certification.

Core checks before UI integration: cargo test --all-targets --locked: **467 passed**, zero failures (77 library + 380 game + 3 exporter + 7 integration). Regressions cover source/cache preservation, duplicate diagnostics, real payment/modifiers/removal, rollback, automatic/selectable missions, remaining timer values, cancellation, expiry/reactivation, save/once behavior, fixed RNG, startup scope, additive/periodic hooks and actual random-event dispatch. Android/final checks follow the UI/report checkpoint. No main merge or history rewrite.


## Final Stage E checkpoint: application wiring, UI, audit and bounded persistence

Parent: 1f7a3b4423845e90ac8562ac511c6966f6c41569. A new campaign invokes startup scripts only after country selection; restoring a save does not replay startup. The live Decisions panel now has cached localized country decisions/mission rows with actual PP cost, status/remaining hours, source conditions/unknown blockers, paginated browsing and actual simulation requests. Events remain a separate tab; blocked scheduled scripts, including global startup blocks, are visible there. Rendering/readiness queries cannot mutate the campaign; a same-hour journal revision invalidates the view after flags/payment/events change. UI layout/touch/device behavior has not been visually verified.

Additional correctness/performance fixes: allowed snapshots retain only eligible IDs and explicit unknown errors after a country is initialized, rather than every foreign false predicate; eligibility is computed in one read/evaluation journal per country, with independent predicate budgets. Only missions and active decisions are considered by periodic lifecycle updates. Unknown allowed predicates disable the UI row explicitly. Blocked jobs cannot reattach old bonuses during modifier refresh. Missing active definitions are diagnosed. A failing daily hook retains one blocked queue item per source block/country instead of appending the same failure every day. Startup blocks beyond the scheduler's hourly budget resume before mission processing. Explicit mission removal preserves mission fire-only-once history; explicit remove_decision retains its native bypass behavior. Native differential ordering for these semantics is still unverified.

`cargo run --bin hoi4-research -- audit data/hoi4` now also writes **campaign-script-compatibility-report.json**. It distinguishes definitions, structural candidates, missing/unknown script dependencies and bound versus unbound hooks. Imported content is always supported=null, tested=false, parity_verified=false until actual evidence exists. Focus dependency traversal follows activated missions and their conditional/effect/event dependencies instead of hiding transitive blockers.

CLI audit was actually run on `/tmp/hoi4-focus-fixture`: **authored fixture only**, 3 decision definitions (2 structural candidates and 1 targeted/state blocker), 2 on-action blocks (daily bound; war hook unbound). These are not the user's installation counts. No original assets/scripts were imported or committed.

Final checks: **473 Rust tests passed**, zero failures (78 library + 385 game + 3 exporter + 7 integration); `cargo fmt --all -- --check`, `git diff --check`, `cargo check --all-targets --target aarch64-linux-android --locked`, and `cargo clippy --all-targets --locked` passed. Clippy has existing/style warnings; the codebase is not warning-free. Verification used the same temporary opt-level=0/debug=0 environment overrides; Cargo profiles are unchanged. This is an Android Rust compile, not APK linking, device rendering or an A51 performance measurement.

### Next precise task after this checkpoint

Bind political-change hooks with correct immediate/deferred order and old_ideology_token scope, then actual war/civil-war/state-control hooks at their existing simulation transitions. Add native targeted decisions/state missions and explicit civil-war inheritance before claiming those DLC chains run. Continue effective stability/difficulty/leader consumers and election timers as separate mechanics; keep all unsupported fields/errors visible. Full original-version integration still requires the user's verified local source export and source/engine differential scenarios; none have been fabricated. Continue on this feature branch, never merge main or rewrite history without explicit authorization.
