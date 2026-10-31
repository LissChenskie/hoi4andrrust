# Branch survey — 31 October 2026

Remote: 36 branches, 2 PRs visible via the API at the time (the API call that lists them
returned an error for the PR query, so PR state is taken from `git` only).
Measured with `git rev-list --left-right --count origin/main...<branch>`.

## Branches with work NOT in `main` (5)

| Branch | Ahead / behind | Content | Merge with main |
|---|---|---|---|
| `feature/hoi4-focus-interpreter-1193` | 13 / 0 | Whole focus interpreter: shared-tree resolution, transactional focus lifecycle, political-power ledger, dynamic modifiers, decisions/missions, on_actions, spatial focus graph UI, compatibility audit (`hoi4-research audit`). +9325/-1022 lines, 34 files | clean |
| `feat/army-command-hierarchy-ui-20261030` | 15 / 8 | Theater / army group / army inspector, officer picker | clean with main; **conflicts with the focus branch** (`src/army_hierarchy_ui.rs`, add/add) |
| `feat/political-timers` | 1 / 74 | Timed national spirits | **conflicts with main** in `src/campaign.rs`; overlaps the focus branch's dynamic modifiers, probably superseded |
| `feat/land-command-1193-20261026` | 1 / 401 | Audit probes (`tools/audit/*`); the audit text itself is already on main | n/a |
| `fix/military-economy-integration-20261028` | 1 / 166 | Division designer audit, 12 findings (DD1–DD12), base `9f83d7c` | docs only |

The other 31 branches are 0 commits ahead of `main`, i.e. already merged (naval, carriers, convoys,
production UI, research fixes, peace/capitulation, mobilization, etc.).

## Verified in this session
* `feature/hoi4-focus-interpreter-1193` builds and passes: bin 393 tests, lib 78 tests, 0 failed
  (`cargo test --locked` in a separate worktree). Its own document reports 481 including exporter/integration tests; those were not re-run here.
* My AI branch merges with it without conflicts.
* Game data is read from `data/hoi4/...` relative to the working directory (`src/main.rs:93-103,533,1201`).
  It is not in the repository, so no original-content focus run was possible here.

## What this changes in `docs/ai-readiness-audit-2026-10-30.md`
The focus rows there describe `main` only. On `main` the focus system is not a full interpreter;
the interpreter exists only in the branch above and is documented there as "do not merge into main"
until original-installation differential checks are done.

## Open facts that still need the user
* Which branch / build the focus test was run on, and whether `hoi4-research audit data/hoi4` was run
  with the branch's importer (its cache version changed several times).
* Concrete production symptoms. Production defaults (`ProductionRules`: 10 % start efficiency, 50 % cap,
  0.001 growth, 150 factories per line) are code constants whose agreement with 1.19.3 was not checked.

## Air and navy: measured gaps on `main` (by text search of `src/`)
* Air missions implemented: Standby, Superiority, Interception, CloseAirSupport (`air_warfare.rs:24`).
  No match for strategic bombing, air supply, paradrop, radar, train busting, aces, wing experience.
  `air_warfare.rs` is 617 lines with 1 own test (more in `support_tests.rs`).
* Naval missions implemented: Hold, Patrol, StrikeForce, ConvoyRaiding, ConvoyEscort (`naval_warfare.rs:48`).
  No match for mines, shore bombardment, blockade, naval experience. Amphibious code appears only in `naval_transport.rs`.
* Text search shows absence of a name, not of behaviour; a mechanic may exist under another name.
