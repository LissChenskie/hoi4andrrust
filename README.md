# HOI4 Mobile — Rust prototype

An independent, early-stage grand-strategy prototype. The current milestone is deliberately small: a main menu, a local province-raster viewer, and province ID lookup. It is not yet a playable HOI4 replacement.

## Current milestone

- Rust application shell using Macroquad for a simple 2D UI.
- Main menu and navigation to the map viewer / map import guide.
- Loads a local BMP province raster and HOI4 definition table (semicolon-delimited vanilla format, comma-delimited also accepted).
- Displays the source raster with nearest-neighbour filtering so province colours are not blurred.
- Supports zoom buttons, mouse-wheel zoom, drag-to-pan, and tapping/clicking to resolve a pixel to a province ID.
- Parses local Clausewitz state files, shows a selected province's state ID, and draws state boundaries as a translucent overlay when membership data is available.
- Does not bundle Paradox map assets or decompiled game code.

## Local map input

The prototype expects files at:

    data/hoi4/map/provinces.bmp
    data/hoi4/map/definition.csv
    data/hoi4/history/states/*.txt (optional, for province-to-state membership)

Put only data you are legally entitled to use in this local folder. It is ignored by Git and should not be committed. Without those files, the app shows an explicit empty-state message rather than inventing fake province data.

The Android file-picker / Storage Access Framework import flow is not implemented yet; the initial data loader currently uses relative filesystem paths. The overlay is based on parsed province-to-state membership; country ownership colours and other map modes are not implemented.

## Run the desktop prototype

Install a current Rust toolchain, then run:

    cargo run

This command is documented but has **not been executed in the current environment**. Android packaging, device testing and performance profiling have not been completed.

## Project notes

- [HOI4 map tooling research](docs/research/hoi4_map_tooling.md)
- [AnalyticalEngine source audit](docs/research/analytical_engine.md)
- [AnalyticalEngine next steps](docs/research/analytical_engine_next_steps.md)
- [Architecture decision](docs/architecture_decision.md)

## Roadmap

1. Main menu and local raster viewer.
2. Province ID lookup, state membership parsing, state-border overlay, and basic zoom/pan (implemented in the prototype).
3. Improve touch gestures and add a true Android file-import flow.
4. Validate map completeness and cross-file references.
5. Add country ownership colours and further map layers.
6. Android import flow and on-device profiling.
7. Simulation and gameplay systems only after map foundations are stable.

This project is independent and is not affiliated with or endorsed by Paradox Interactive.
