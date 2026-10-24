# Military factory production milestone

An **original** Rust gameplay prototype inspired by HOI4's abstract military
industry, integrated with the existing map, country economies, construction
queues, and daily simulation. This is not an imported copy of HOI4's engine.

## Player workflow

1. Load locally provided map/state data and select a country.
2. Open **PRODUCTION** (top bar or `P`).
3. Select a product category (INF / SUP / TRK / FLD / AIR).
4. Press **ADD LINE**, then assign factories with **+ / -**. Lines with
   higher priority (buttons **UP / DN**) claim factories first.
5. Run the game clock. Equipment is produced daily and added to the selected
   country's inventory. The UI shows assigned factories, IC/day, current
   efficiency and items completed last tick.
6. Select a line and use **CHANGE SELECTED** to change equipment. Switching
   discards the old unfinished item and retains only part of production
   efficiency. **X** deletes the line, not the completed stockpile.
7. Construct a new military factory in the construction screen. Production
   from the additional factory begins on the *following* day.

## Simulation model

- National military factories are obtained from the authoritative
  `World::country_economy`, so factory construction/ownership affects capacity.
- Each country's `CountryProduction` owns an ordered vector of production
  lines and a stockpile indexed by product category. Countries are isolated.
- Factories are allocated in priority order up to requested counts (maximum
  150 per line), never exceeding the country's available military factories.
- Base factory output interpolates between 3.5 and 4.5 abstract IC/day
  based on the configuration's powered fraction. Factory output and resource
  multipliers are configurable; neither is yet connected to the map's energy
  or resources.
- New lines begin at 10% efficiency, capped at 50% by default. A HOI4-like
  daily increase is `gain = 0.001 * cap² / max(current_efficiency, 0.01)`.
- `daily IC = assigned MIL * factory IC output * efficiency`. Fractional
  industrial work carries forward to the next day. Only whole equipment
  items enter a country's stockpile. The equipment costs are **temporary
  prototype constants**, not claims of accurate current HOI4 values.
- Simulation updates on day ticks, independently of rendering.
- `switch_equipment` uses a **temporary 50% efficiency-retention factor**,
  clamped to the starting efficiency.

## Scope still missing

- Historical starting production lines and equipment technology unlocks.
- Authentic equipment stats/costs and variant definitions from local data.
- Resource extraction, shortages, country trade, energy consumption and
  industry law/tech modifiers.
- Dockyard/ship lines (not covered by this MIL-focused prototype).
- Equipment demand, training, stockpile consumption and combat systems.
- Save/load, Android QA and actual frame-time profiling.

Reference research:
- [Paradox industry developer news](https://store.steampowered.com/news/posts/?appids=394360&enddate=1787058179&feed=steam_community_announcements)
- [Production formulas overview](https://cooderatom.github.io/hoi4-zh-wiki/Production.html)

Tests cover production output, IC carry, prioritization, stockpile isolation,
newly completed factories, efficiency and product switching.
