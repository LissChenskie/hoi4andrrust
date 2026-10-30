# National focus interpreter — recovery checkpoint

Branch: `feature/hoi4-focus-interpreter-1193`. Base: `64b0a66893a96c0deae02fc6be0682da5ee2e8e3`.
Last published ancestor: `64b0a66893a96c0deae02fc6be0682da5ee2e8e3` (this document is committed after that SHA; use branch HEAD for the latest checkpoint).

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
