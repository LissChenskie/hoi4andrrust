# AI implementation progress

Branch: `feature/expert-sheep-ai-20261030` (base `main` @ `64b0a66`).
This file is updated after each stage. It lists only what was verified.

## Stage A — audit: done
`docs/ai-readiness-audit-2026-10-30.md`, `docs/ai-source-analysis.md`.

## Stage C (partial) — AI core + research: done and tested
Implemented in `src/ai/`:

* `state.rs` — `AiState` (seed, per-country `AiProfile`, bounded decision log, counters). Saved with the campaign as `GameSession.ai` (`#[serde(default)]`, so old saves load).
* `scheduler.rs` — game-hour cadences (strategic review every 30 days or on war start/end). **Provisional**, not profiled.
* `observation.rs` — read-only country view from the real `World`, `Warfare`, `CountryResearch`.
* `commands.rs` — AI calls `CountryResearch::start`, the same validated entry point as the player, and records each result.
* `research.rs` — ranked research planner. Replaces the old weekly `ResearchWorld::ai_choose` call in `GameSession` (`run_research_ai` in `session.rs`), now run daily and a no-op when no slot is free.

Tests (all pass): 8 unit tests in `ai/research.rs`, 3 session tests in `ai/tests.rs`
(two AI countries run 90 days with a stable result; the human country is not touched; an identical
run and a run with a JSON save/load at day 30 give identical decision logs, stats, profiles and
researched sets). Whole bin suite: 348 passed, 0 failed.

## Not done (everything else in the task)
Nothing below exists yet; none of it should be considered implemented:

* Economy/construction AI, production planner, recruitment and division design.
* Focus and politics AI (blocked on the engine gaps listed in `docs/focus-readiness-gate-2026-10-30.md`).
* Operational/army AI. The legacy attack AI (`warfare.rs:3070`) is **still active** and unchanged.
* Air, navy and amphibious planning; diplomacy and peace-conference AI.
* Historical/non-historical switch, difficulty modifiers, country profiles for major powers.
* Headless multi-country performance runs and profiling; Android/ARM64 verification.
* Real-data (1.19.3) campaigns: data is not in the repository.

## Known limitations
* Research category classification is by name patterns (`industry`, `armor`, ...) because category IDs come from imported data; verify against the real list.
* Scoring constants are provisional, not tuned.
* `cargo fmt --check` and `clippy -D warnings` fail on the baseline for reasons unrelated to this work.

## Next actions (in order)
1. Economy/construction planner on `enqueue_project` using `CountryObservation` plus per-state candidates.
2. Production planner from division template demand.
3. Operational planner driving `OperationalState`; then turn the legacy attack AI off for countries it controls.
4. Fix lazily created research entries (0 slots) for new countries.
