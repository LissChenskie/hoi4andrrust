# Simulation timing — 3 November 2026

Dev profile (`opt-level = 1`), desktop CPU, synthetic terrain. Run with
`cargo test --bin hoi4-mobile perf_probe -- --ignored --nocapture`.

* Empty world, no divisions: after indexing states by controller
  (`World::country_economy`), 800 states / 80 countries cost 1.8 ms per game day (was 41.7 ms).
* War on a 9600-province grid, border rows attacking every 24 h (80 battles running):

  | Divisions | ms per game hour | worst hour |
  |---|---|---|
  | 200 | 1.06 | 1.8 ms |
  | 1000 | 1.7 | 5.2 ms |
  | 4000 | 5.5 | 17.5 ms |

  Phases at 4000 divisions (ms per game hour, averaged): legacy attack AI 1.6 (it runs every 6th
  hour, so about 10 ms in that hour), HQ/command bonuses 1.3, battles 0.9, movement 0.9, training 0.7.

Caveats: every division in the test is outside any army, which overstates the legacy attack AI.
Rendering, UI, the logistics ticks driven by the session and the Android CPU are not included.
Frame-level numbers from the device are still needed: F3 or `HOI4_PROFILE=1` writes `perf.log`.
No optimisation of the war tick has been done yet.
