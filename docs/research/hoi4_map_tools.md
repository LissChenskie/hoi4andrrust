# HOI4 map tooling — source audit for the first prototype

Date: 2026-10-23. Scope: only the projects supplied for the map/menu milestone. This document records what their source trees and documentation indicate; it does not claim that any project was built locally.

## Main finding

The exact HOI4 map is not just a single picture. The province raster defines geometry; a definition table maps raster RGB colours to province IDs; state files group provinces and contain political/economic history; strategic-region files group states/provinces for air and naval gameplay; other raster/text layers provide terrain, height, rivers, trees, positions, railways, supply nodes and adjacencies.

For the mobile prototype, the first concrete step is to load a user-supplied `provinces.bmp` and `definition.csv`, display province boundaries and allow selecting a province. State ownership colours and state borders are a separate next milestone. No vanilla HOI4 assets are committed to this repository.

## Repositories inspected

### 1. HOI4 Modding Tools Extension for VS Code
Repository: https://github.com/OlanderIAO/HOI4-Modding-Tools-Extension-for-VSC

- The root currently contains README.md, a VSIX bundle and images; no root LICENSE file was found in the inspected tree.
- Its documented map tools operate on real HOI4 mod files such as `provinces.bmp`, `definition.csv`, `heightmap.bmp`, state files and strategic-region files. It includes province/state/region editing, validation, generation of positions and adjacency data, and raster tools.
- Useful as a map-format checklist and for understanding dependencies between map layers.
- It is a VS Code extension/tool suite, not an embeddable Android renderer. Do not copy its implementation without confirming licensing.

### 2. HOI4 Map Editor
Repository: https://github.com/AdriianCOE/HOI4-Map-Editor

- Rust project; the inspected Cargo.toml declares package license metadata as MIT and uses the image crate with BMP support, CSV, and a desktop Piston/OpenGL stack.
- Its README and source tree describe province/state/strategic-region editing, validation, adjacency editing, backups, undo/redo and in-memory edits before saving.
- Useful design ideas: validate before writes, keep edits in memory, maintain undo/redo, validate cross-file references, and treat state/region membership as structured data.
- It is an editor, not a mobile game renderer. The root tree did not expose a LICENSE file at the path checked, despite Cargo metadata declaring MIT; verify licensing before copying code.

### 3. Provincial
Repository: https://github.com/ThomasSlade/Provincial

- Python source includes assignprovinces.py, fillprovinces.py, generatedefinitions.py, validatemap.py and supporting utilities.
- The code comments describe matching a province map to state areas, generating/updating province definitions and validating map data.
- A root LICENSE exists and states MIT, but its copyright holder/year remain placeholders. Treat that as a provenance/licensing caveat and do not copy source wholesale.
- Useful as a reference for map authoring workflows, not as a renderer.

### 4. hoi4-maptool
Repository: https://github.com/eliseypng/hoi4-maptool

- Python project with province_generator.py and territory_generator.py, plus import/export helpers and UI code.
- The province generator reads boundary/land masks and creates province regions; this is procedural map generation, not a source of exact vanilla HOI4 geometry.
- A root MIT LICENSE exists.
- Useful for understanding generation/segmentation concepts; not a substitute for the original province raster.

### 5. EarthToHoI4
Repository: https://github.com/EdouardSaladier/EarthToHoI4

- Python project with substantial modules for provinces, states, strategic regions, terrain, rivers, cities, climate, rendering and mod output.
- Its stated goal is to generate HOI4 mod maps from real-world geodata and rasters.
- A root MIT LICENSE exists.
- Useful for understanding the dependency graph between geographic layers and the pipeline that turns input geography into HOI4 map files. Its generated geography will not reproduce vanilla HOI4 exactly.

### 6. HOI4-MCP
Repository: https://github.com/cpntodd/HOI4-MCP

- Root LICENSE is GNU GPL v3; the repository describes structured access to mod data, syntax validation, local game data and error logs.
- Useful as a design reference for deterministic parsing, validation and avoiding guessed IDs.
- Keep code reuse separate from ideas: copying GPL code into this project would require a deliberate license decision. The first prototype does not need this server.

### 7. hoi4-mod-skills
Repository: https://github.com/kkkunny/hoi4-mod-skills

- The inspected `skills/hoi4-map/SKILL.md` describes the key map inputs: 24-bit RGB `map/provinces.bmp`, `map/definition.csv`, heightmap, indexed terrain/rivers/trees layers, adjacencies, railways, supply nodes and `default.map`; it also documents state history files.
- No root LICENSE was found at the checked path. Treat it as documentation/reference, not code to copy.
- Particularly useful as a checklist of data formats and bitmap constraints.

## Relevance of AnalyticalEngine / AOC3

The previous audit of AnalyticalEngine found separate JavaScript modules for map, province, terrain, region and map-mode behavior. The framework calls into existing AOC3 Java objects, so it is not a standalone map engine and its AOC3 province data is not HOI4 data. We can study its rendering/input flow as a reference, but the HOI4 map loader must use HOI4-format data independently.

## Implementation plan

### Milestone A — menu + province raster viewer
- Rust application shell with a main menu, map screen and settings/instructions screen.
- Load `assets/hoi4-map/provinces.bmp` and `assets/hoi4-map/definition.csv` when supplied by the developer.
- Build a province-ID raster by matching RGB pixels against the definition table.
- Render coloured province regions with boundary pixels and support tapping/clicking to show the selected province ID.
- If map files are absent, display an explicitly labelled synthetic demo map; never label it as the real HOI4 map.
- Keep vanilla files out of Git. The local asset directory is ignored by Git and documented for user-supplied files.

### Milestone B — state/country view
- Parse Clausewitz state files and province membership, then map province IDs to state IDs.
- Parse country tags/colours only after identifying the required source files and formats.
- Add state boundaries and owner/country map modes; report missing or conflicting references rather than silently guessing.

### Milestone C — real mobile packaging and performance
- Verify Android target/toolchain and packaging separately; desktop code compilation alone is not Android validation.
- Measure memory and frame time with a full-resolution map. Avoid repeated CPU-side raster rebuilds during interaction.
- Add pan/zoom and more map layers only after selection and map loading are correct.

## Data/legal boundary

This project does not commit proprietary game maps, textures, or other vanilla assets. The loader is designed for files supplied by the developer from an installation or a separately licensed source. A tool repository's MIT/GPL metadata does not grant rights to the game data that the tool processes.
