# Focus readiness gate — 30 October 2026

Source requirement: `HOI4-Mobile-Focus-DLC-Roadmap-2026-10-29.md`.
Baseline audited: main `6d197b5b39db08608794adcee29f59e83827b485`.
This implementation **does not** claim the original game or all DLC to be feature-complete.

## Existing components verified

- One transactional scripted-effect interpreter in `src/campaign.rs`, with stable RNG and scheduler, event option execution, conservative predicate handling and save validation.
- Typed country split and isolated civil-war settlement in `src/civil_war.rs`, not yet connected to arbitrary native DLC split scripts.
- B01 HP-empty-category correction, B06 rename references in recruitment, not a true global template-ID migration.
- Historical research and scripted source import from the user's local HOI4 export.
- No strategic AI changes.

## Added in this PR

1. `ResearchScripts.focus_trees` retains country selection metadata, original source and defined focus IDs. Import cache version increments to prevent using incomplete older records.
2. `GameSession::begin_focus/cancel_focus/tick_focuses` validates country tree, mutually exclusive and prerequisite groups, source conditions, reward preflight, original focus cost and `NFocus` timing defines. Saves an active job and error. Completion calls the existing transactional campaign effects, then records completion once.
3. The Focus panel shows **actual imported focus IDs**, progress and blocked reason instead of a fictional country tree. Pending country events use existing Decisions panel to present genuine options. Decisions with their own costs/missions are not yet supported.
4. Political values explicitly defined in selected dated country histories enter `CampaignState`. Unspecified values remain unknown, not guessed.
5. Import/audit generates `content-dependencies.json`: installation and DLC profile, script sources, transitive scripted calls and missing/dynamic references. Status is an evidence collection label; gameplay must pass preflight and consumer validation.

## How to check an installation

```powershell
cd "F:\hoi4andrrust"
git switch main
git pull --ff-only origin main
cargo run --bin hoi4-research -- audit data/hoi4 --version 1.19.3
```

Do not force `--version 1.19.3` if that is **not** the installed version: the override labels the data, it cannot convert different source game files. Prefer the actual installed game version from metadata and verify `research-import.json`. `content-dependencies.json` reports sources, not 1:1 behavior.

## Still blocking full first-focus parity

- Country politics must import or otherwise verify starting political power and daily income; if absent, focus begin is explicitly blocked.
- Focus progress cancellation/saved progress, bypass handling, select effects, availability changes and all dynamic DLC-specific selectors require source-backed implementations.
- Generic trees with unsupported selectors, shared focus membership and DLC gates require a complete dependency review and tests for the exact 1.19.3 export.
- Source inventory does **not** prove effect consumers, scripted conditions, event on_actions, decision/mission payment or original in-game scheduling.
- Template IDs independent from names, LawState with consumer goods, dynamic modifiers, political leader/advisor roles and Soviet paranoia/purges are not fully implemented.
- To claim a first complete playable branch: use the user's 1.19.3 DLC corpus, select a small industrial chain, prove start/cost/duration/effects/production/save-load with real source IDs. Only then move on to Soviet DLC.

## Ready/not-ready

**Ready for incremental content integration and fixture-driven verification.**
**Not ready to claim complete generic 1.19.3 parity or all DLC focus trees.**
