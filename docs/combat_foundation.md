# Campaign, hourly simulation and province geography

This milestone prepares the existing prototype for land units and combat. It does
not implement damage formulas, division movement, occupation economics, naval
blockades or diplomatic access, and does not claim HOI4 engine parity.

## State ownership

`GameSession` owns `World`, the clock, selected country, construction queues,
per-country construction orders/laws/policies, national production/stockpiles and
research. UI views keep only window/selection/scroll state. `ProvinceMap` owns
textures, pixel hit-testing and camera state, with no campaign world inside it.

Reload map rebuilds the view from the current session. It cannot reset completed
buildings, orders, research, stockpiles or time. The province bitmap and definition
fingerprint must match the active geography; changes require a new campaign.
If reloading fails, campaign state remains in memory and the view can be retried.
New game / Select country -> Start game imports a fresh campaign and resets it.
This is in-memory session management; disk saves are a separate milestone.

Every country gets its starting research history before simulation begins.
Missing history is logged and explicitly falls back to three slots. Production
and construction update for every country, including countries not selected by
the player. This does not create AI construction/production orders automatically.

## Simulation scheduling

The clock starts at 1936.01.01 00:00 and advances in whole hours. Frame time only
schedules work; the session commits each hour in chronological order. Existing
speed settings retain 1/2/4/8/16 game days per real second. At most 192 hours are
processed per frame, with bounded pending work (768 hours). Frame delta remains
capped at one real second after a stall/backgrounding. Pause freezes pending work.

At midnight the completed day runs exactly once, in this explicit prototype order:

1. Research for all countries, using the completed day's date; weekly AI choices
   are started at the new day's date.
2. Production for all countries, using factories present before construction.
3. Construction for all countries, using each country's own order and policy.

Factories completed in phase 3 enter production on the next daily update.
Future hourly movement/combat systems belong at the single `advance_one_hour`
boundary. Their exact phase order must be established against the target build.

## Geography and territory

`scenario::load` is a headless import. It reads `map/provinces.bmp`,
`map/definition.csv`, state histories/categories, and optionally:

- `map/adjacencies.csv`: scripted straits and impassable connections; overrides
  the ordinary raster border in both directions. Through-province and rule names
  are preserved. Invalid references/types reject the import.
- `map/adjacency_rules.txt`: retained script source, with syntax validation.
  Its conditions are not yet executed; an edge is not permission to cross it.
- `map/rivers.bmp`: uncompressed 8-bit indexed BMP, with row padding and both
  orientations supported. Missing data remains `RiverCrossing::Unknown`, not
  an assertion that there is no river.

Ordinary neighbors come from shared pixel sides, not corner contact. The map
wraps horizontally, not vertically. Import uses two row buffers instead of
another full-world province-ID image. Edges and neighbor lists have stable order.
Definitions retain land/sea/lake, terrain, coastal flag and continent. Raster
centroids provide positions and wrapped **pixel** distances; they are not yet
HOI4's calibrated movement distances or guaranteed unit-stack positions.

River crossing import traces centroid-to-centroid lines and preserves the widest
encountered source index in the standard 0..11 river palette range. Small/large
classification, exact positioning and thresholds must be validated with the
chosen game's data/defines before using them for movement or combat penalties.

The legal owner remains on the state. Actual control is stored per province and
can differ within one state. `set_province_controller` changes control without
transferring state ownership, factories or other provinces. Initial dated
controller history is imported separately from owner history. Peaceful ownership
transfer follows unoccupied control, retaining a third country's occupation.
Current factory aggregation still uses legal owners; occupation rules come later.

## Validation

Unit/integration checks cover midnight, leap/year boundaries, pause/backlog,
frame slicing, foreign economy/history, independent construction priorities,
single-province occupation, cardinal/seam adjacency, blocked/strait overrides,
invalid map references and indexed river decoding. A synthetic BMP/state/CSV
fixture exercises the complete scenario import without proprietary game assets
or a graphics context. Visual Android and full-map parity checks remain necessary.
