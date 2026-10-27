# HOI4 naval source audit — 2026-10-27

This is a **source ledger**, not a claim that our simulation is 1:1 with the proprietary engine.

## Source verification

| Primary/secondary source | Confirmed fact | Version/provenance | Implemented in this repo |
| --- | --- | --- | --- |
| [Paradox development diary: New Features](https://steamcommunity.com/games/394360/announcements/detail/603046864900588092) | Carrier stances split defensive/offensive sortie efficiency in five steps: Fully Defensive 100/0, Cautious 75/25, Balanced 50/50, Active 25/75, Aggressive 0/100 | First-party announcement, 2025; check current version's patch changes | `src/naval_carrier.rs` stance representation, allocation; `src/campaign_ui.rs` control |
| [No Compromise, No Surrender patch notes](https://devtrackers.gg/heartsofiron/p/e0b8d4af-no-compromise-no-surrender-patch-notes-live-stream-mods) | Carrier planes can perform air missions while their taskforce is on a naval mission; carrier defensive/offensive stances added | 2025 expansion; cached copy of first-party release notes | **Not yet complete:** map-side carrier air mission system remains absent |
| [HOI4 equipment documentation](https://github.com/Killeritch/Hearts-of-Iron-IV/blob/master/common/units/equipment/_documentation.info) | `module_slots`, inherited aliases and `module_count_limit` with strict count comparisons and parent overrides | Historical embedded game documentation; validate per current content | Already implemented earlier in PR #7/#8; see `src/naval_designer.rs` |
| [1.17.4 patch notes](https://steamdb.info/patchnotes/22001276/) | February 19, 2026: naval hit chance and strike-force repairs changed | Third-party mirror of patch notes, explicitly 1.17.4 | **Potential mismatch:** `src/naval_combat.rs` still needs version-specific comparison and tests |
| [HOI4 naval defines in open mods](https://github.com/LostPatrol/Heart-of-Iron-IV-mod/blob/master/mod1-CPMOD_C/common/defines/00_defines.lua) | Game script exposes named carrier sortie/air combat parameters | **Not authoritative for current vanilla values**; mod can change them | Read imported `NDefines` through `src/naval_carrier.rs`, no copying balance values from mods |
| [Carrier plane community reverse-engineering](https://www.reddit.com/r/hoi4/comments/1u5zxe8/i_need_help_understanding_the_carrier_stacking/) | Recent anecdotal tests claim some navy wiki stacking descriptions are obsolete | June 2026 discussion, **unverified**; no formula proved | Don't hard-code community guesses |

## Audited current code: what exists already

- PR #7: inherited naval module slot aliases and module-stat aggregation fixes.
- PR #8: module count limits, refit projects, docking/resource allocation.
- PR #9: carrier define namespace, captured-port replacement validation, destruction of air groups when the carrier sinks.
- PR #10 (this branch): five explicitly selectable carrier stances. They are serialized with old-save default, restricted to the owning task force, and affect the *existing* in-battle offensive sorties and defending carrier-fighter allocation.

## Critical parity gaps / research queue

1. **Carrier air missions versus carrier naval battle sorties are different mechanics.** The patch notes explicitly allow carrier planes to execute normal air missions while the task force is at sea; our `AirState` still requires land airbases. Do not describe the new stances as a complete implementation until those mission assignments exist. Avoid double-counting the same aircraft in sea encounters and map air missions.
2. **Defensive readiness/first-24-hours:** later official 2026 patch notes reference stance application during the initial 24 hours, but the exact combat scheduling and readiness are not established here. Our defensive share currently scales defending fighters during carrier strike resolution; the fully defensive stance does not yet model every possible counterstrike.
3. **Current vanilla define values:** do not adopt values from unrelated mods or old screenshots. Obtain the specific selected game's `common/defines/` files via legal user-provided local game content; prefer data-driven values. Current 1.19.3 rules require separate authoritative verification.
4. **Carrier strike target selection, nighttime/weather effects, stacking/overcrowding, fighter sorties and tactical naval combat:** coefficients in `naval_carrier.rs` remain approximations; write deterministic property/regression tests, compare against controlled in-game experiments.
5. **Save compatibility:** preserve absent `carrier_stances` as `Balanced`; ensure save roundtrip for all five stances and deny foreign-fleet assignments.
6. **Map fleet UI:** do not display unchecked claims of 1:1 accuracy; expose both defensive/offensive shares with full percentages and selected task-force identity.

## Next controlled comparison experiments

Use the same save and fleet, replay for each of the five stances (with weather/time fixed). Record number of aircraft launched, attrition, fuel, cooldown, air-mission participation and first-day defense after enemy initiation. Repeat with only carrier fighter wings and only carrier naval bombers. Repeat with land-based air present to expose double-accounting. Treat each measured value as patch-specific.

## License / rights

This repository should independently implement mechanics. Linked game documentation is for facts and data layout. Do not copy copyrighted artwork, ship model meshes, game assets or incompatible third-party source wholesale.
