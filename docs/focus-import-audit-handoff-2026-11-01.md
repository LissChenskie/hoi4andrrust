# HOI4 focus/import audit handoff — 2026-11-01

This document records the user's local HOI4 export audit and the follow-up report supplied by the external coding model. It is an evidence snapshot, not a claim of full HOI4 compatibility.

## Source installation and export context

| Field | Observed value |
|---|---|
| Installed HOI4 version | 1.15.4 |
| Version source | `launcher-settings.json:rawVersion` |
| Target project version | 1.19.3 |
| Scenario | 1936 |
| EXE ProductVersion | 1.0 (not the HOI4 game patch version) |
| DLC selection verified | No |
| Enabled DLC list recorded by exporter | Empty / unknown |

The export was made from 1.15.4. It must not be treated as a complete 1.19.3 content baseline. The installed/active DLC selection is unresolved. `examples/military_profile.txt` reportedly names Death or Dishonor, Together for Victory, and Waking the Tiger, but the export did not verify which DLC were active. Do not infer active DLC from that profile.

## Technology import diagnostics

Read from the user's `research-report.json` using Python JSON parsing:

| Diagnostic | Count |
|---|---:|
| Total diagnostics | 551 |
| Failed source files | 0 |
| Partially supported fields | 513 |
| Missing localized names | 20 |
| EOF-terminated sources | 5 |
| Unresolved Lua defines | 0 |

Files reported with EOF termination:

- `common/ideas/SOV.txt` — line 1, column 9
- `common/ideas/switzerland.txt` — line 1, column 9
- `common/national_focus/austria.txt` — line 11, column 14
- `history/countries/NOR - Norway.txt` — line 416, column 12
- `interface/ger_monroe_doctrine_scripted_gui.gui` — line 2, column 12

The failed-source-files collection was empty. These EOF diagnostics still need investigation; they should not be silently suppressed.

PowerShell's `ConvertFrom-Json` rejected `research-report.json` because it encountered case-distinct keys `IF` and `if`. Python's JSON parser was used to read the report for this snapshot.

## Baseline compatibility reports

### National focuses

| Metric | Baseline result |
|---|---:|
| Focus trees | 52 |
| Focuses | 7,428 |
| Shared focuses | 409 |
| Resolved focuses | 6,232 |
| Structural candidates | 970 |
| Tested against original HOI4 | 0 |
| Parity verified | 0 |

Examples of unsupported or unresolved constructs shown in the compatibility report included:

- Effects: `add_popularity`, `custom_effect_tooltip`, `random_country`, `set_cosmetic_tag`, `random_core_state`, `add_timed_idea`, `add_opinion_modifier`, `add_country_leader_trait`, `remove_country_leader_trait`, `add_extra_state_shared_building_slots`, `add_dynamic_modifier`.
- Triggers/conditions: `has_civil_war`, `is_owned_and_controlled_by`, `has_war_with`, `any_other_country`, `is_faction_leader`.
- Context and control-flow forms: `IF`, `ROOT`, `SOU`, `USA`, and nested `country_event` usage.

Important: the report's 970 `structural_candidates` is a different metric from the later model-reported runtime/preflight “clear” count. They must not be compared as if they were the same gate.

### Decisions and on-actions

| Content | Imported entries | Structural candidates |
|---|---:|---:|
| Decisions | 3,158 | 36 |
| On-actions | 250 | 1 |

Reported blockers included unsupported effects/modifiers, unsupported decision fields such as `state_target`, unknown events, and hooks imported without being bound to simulation transitions.

## Follow-up report from the external coding model

The following are claims from the external model's report; they were not independently rerun while preparing this document.

### Reported focus-preflight change

| Metric | Before | After |
|---|---:|---:|
| Clear / passing focus preflight | 1,162 | 1,680 |
| Hard-blocked focuses | 6,266 | 5,748 |
| Focuses with unresolved dependencies | 1,196 | 24 |

The model reports that its audit on the restored `research-review.zip` dataset matched the user's counts, including 6,266 blocked focuses and 551 diagnostics.

### Reported fixes

1. **Event registration:** previously, event lookup found 812 of 10,093 events because only events/fragments with an `immediate` block were indexed. The model reports that full event definitions are now registered, reducing focus dependency blockers from 1,196 to 24.
2. **Uppercase control flow:** normalize `IF`, `ELSE`, and `ELSE_IF` to the same handling as lowercase spellings. The model reported these upper/lowercase source counts:
   - `IF`: 1,694 uppercase vs 4,952 lowercase
   - `ELSE`: 342 uppercase vs 1,214 lowercase
   - `ELSE_IF`: 366 uppercase vs 774 lowercase
3. **Display-only tooltip effects:** accept `custom_effect_tooltip`, `effect_tooltip`, `show_ideas_tooltip`, and `unlock_decision_tooltip` as no-op display commands. The model explicitly says the inner effects of `effect_tooltip` are not executed.

The model reports regression coverage for all three fixes and says 398 binary tests and 79 library tests passed. It also states it did not run the new tests against the old code, so the measured before/after report is the stated evidence. These test counts/results are attributed to that model, not independently verified in this handoff.

### Reported next effect candidates

The model's table gives the following estimated focus-unlock contribution when implementing effects in sequence:

| Effect | Focuses containing it | Model's cumulative clear count |
|---|---:|---:|
| `random_owned_controlled_state` | 372 | 1,861 |
| `add_timed_idea` | 370 | 2,032 |
| `add_popularity` | 456 | 2,205 |
| `add_extra_state_shared_building_slots` | 318 | 2,368 |
| `create_wargoal` | 336 | 2,519 |
| `add_doctrine_cost_reduction` | 330 | 2,659 |

The model estimates that ten effects in sequence could bring the clear count to roughly 3,120 (a little over 40% of all focuses). These are estimates from the model's audit, not measured gameplay-parity results.

## Compatibility limitations and next verification gates

- The source export is HOI4 1.15.4, while the project's target is 1.19.3.
- Active DLC selection remains unverified.
- No original-engine parity tests were reported: the baseline report lists zero tested original focuses and zero parity-verified focuses.
- A focus passing parser/preflight checks does not prove its reward, event chain, decision, on-action, AI behavior, save/load behavior, or gameplay outcome matches original HOI4.
- The next implementation priority in the external model's report is `random_owned_controlled_state`, followed by `add_timed_idea`, `add_popularity`, `add_extra_state_shared_building_slots`, `create_wargoal`, and `add_doctrine_cost_reduction`.

