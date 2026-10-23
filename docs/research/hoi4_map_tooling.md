# HOI4 map tooling — research for the map prototype

Research date: 2026-10-23. This document records source-level findings relevant to building a separate Rust/Android map viewer. It does not authorize copying game assets or decompiled code.

## Main conclusion

The HOI4 map is not one vector picture. A usable map pipeline needs at least:
- a province raster (provinces.bmp) whose RGB colors identify province IDs;
- a definition table (definition.csv) mapping IDs to colors and metadata;
- state files grouping province IDs and describing state-level information;
- later, strategic regions, terrain, rivers, heightmap, supply areas, positions, adjacencies and other layers.

For an accurate map, preserve original province IDs and pixel colors. Do not regenerate arbitrary colors or redraw boundaries. State boundaries are derived from the province-to-state mapping; they are not interchangeable with province boundaries.

## Repositories reviewed

### 1. HOI4-Map-Editor — AdriianCOE

URL: https://github.com/AdriianCOE/HOI4-Map-Editor  
Language: Rust; Cargo edition 2024. Repository metadata did not report a recognized license and no LICENSE file appeared in the inspected tree. Treat code reuse as **not cleared** until the author/license terms are confirmed.

Relevant source files inspected:
- src/app/format/definition.rs: a typed Definition structure stores province ID, RGB color, province kind, coastal flag, terrain and continent. Comments connect province color in the definition table to provinces.bmp.
- src/app/project/province_geometry.rs: read-only analysis derives province geometry from a bitmap; records pixel count, connected components, bounding boxes and representative points. It documents horizontal wraparound for the cylindrical world map but not vertical wraparound.
- src/app/map/province_id_index.rs: a bidirectional index maps province IDs to colors and colors to IDs; detects ID 0, duplicate IDs, duplicate colors and allocation overflow.
- src/app/state/model.rs: state files use Clausewitz-style date values and typed state data.
- Cargo.toml: uses Rust and image-processing, CSV, serialization and graphics libraries; the application is an editor, not a ready-made Android game engine.

Useful design ideas: typed IDs; separate color-to-ID index; bitmap-derived geometry; explicit validation for duplicate IDs/colors; distinguish raster facts from map-editing commands. Do not copy code until license status is resolved.

### 2. Provincial — ThomasSlade

URL: https://github.com/ThomasSlade/Provincial  
Language: Python; MIT LICENSE present in repository. Its README describes a toolset for province-map creation. Scripts include fillprovinces.py, assignprovinces.py, generatedefinitions.py and validatemap.py.

Potential use: understand the workflow for turning outlines into province fills, generating definition rows and validating map files. It is an authoring tool, not a renderer for the vanilla HOI4 map. Its README also documents a limitation around nested/donut-shaped states. We should not run its generation workflow over the original map if the goal is exact preservation.

### 3. hoi4-maptool — eliseypng

URL: https://github.com/eliseypng/hoi4-maptool  
Language metadata was not available; MIT LICENSE present. Inspected logic/province_generator.py: it takes a boundary image and/or land/ocean mask, builds masks, and uses image-processing operations to generate province regions.

Potential use: compare province-generation algorithms for a future custom map. It is not the right source for recreating the existing HOI4 boundaries 1:1 because it generates provinces from inputs rather than preserving the original province raster.

### 4. EarthToHoI4 — EdouardSaladier

URL: https://github.com/EdouardSaladier/EarthToHoI4  
Language: Python. A LICENSE file exists, but repository API metadata did not identify an SPDX license; inspect the actual license before reusing code. README describes generation of HOI4-compatible map layers from real-world geodata.

Potential use: study the staged pipeline (geometry, province generation, states, strategic regions, terrain and output). It creates a new map from geographic sources; that is a different task from reproducing vanilla HOI4. It also reads some files from a local HOI4 installation for its mod output. Do not bundle its outputs or game files without checking rights and provenance.

### 5. HOI4 Modding Tools Extension for VS Code — OlanderIAO

URL: https://github.com/OlanderIAO/HOI4-Modding-Tools-Extension-for-VSC  
The inspected repository tree contains a README, images and a VSIX but no LICENSE file. Therefore code reuse is **not cleared** absent explicit permission/license. Its documentation describes tools operating on provinces.bmp, definition.csv, state files and related layers, plus map validation and raster editing.

Potential use: feature checklist and format discovery only; do not copy implementation code until licensing is clear.

### 6. HOI4-MCP — cpntodd

URL: https://github.com/cpntodd/HOI4-MCP  
Language: Python; LICENSE file exists, though repository metadata did not resolve an SPDX identifier. It is a modding-assistant toolkit with parsers, validators and indexing, not a map renderer. Useful later for understanding structured Clausewitz parsing and validation. Not needed for the first map rendering milestone.

### 7. hoi4-mod-skills — kkkunny

URL: https://github.com/kkkunny/hoi4-mod-skills  
The inspected tree has map/state modding guides and a skills/hoi4-map/SKILL.md path, but no LICENSE file was found in the inspected tree. Use as reference material only unless rights are clarified. It may help identify map file roles; it does not supply a runtime renderer.

## AnalyticalEngine / AOC3 comparison

The AnalyticalEngine source has separate JavaScript helpers for map modes, provinces, terrain, resources and regions. Its map helpers call existing AOC3 APIs such as Game.getProvincesSize() and Game.getProvince(i), and its map mode code updates existing province drawing. This is a script/modding layer over the AOC3 model, not an independent source of HOI4 province geometry.

Some Java files in the repository are explicitly FernFlower-decompiled from class files. Use only high-level architectural observations; do not transplant decompiled AOC3 code or game assets into this project.

## Chosen initial map data contract

The first viewer should accept a user-supplied/local data folder and keep it out of Git:
- provinces.bmp: exact source raster, preserved without recoloring;
- definition.csv: ID/RGB mapping;
- later history/states/*.txt: state membership and metadata, parsed as Clausewitz syntax rather than by brittle line matching.

At load time:
1. Decode the bitmap and preserve its exact RGB pixels.
2. Parse definitions into province_id -> RGB and RGB -> province_id maps.
3. Validate duplicate IDs/colors and report pixels whose colors have no definition.
4. Render the bitmap with nearest-neighbor filtering to avoid blending province colors.
5. Convert clicks from screen coordinates to bitmap coordinates, then resolve RGB to province ID.
6. Derive province adjacency and state borders as separate layers; do not overwrite the source raster.
7. Keep source data paths and imported game assets local/ignored. The public repository should contain code and synthetic test fixtures only, not Paradox map data.

## Milestones

1. Main menu and navigation shell.
2. Bitmap viewer with zoom/pan and exact pixel sampling.
3. Province ID lookup from definition.csv and selected-province panel.
4. State parser and state-boundary overlay.
5. Additional map layers and validation.
6. Android file import flow and performance profiling on target devices.

No local compilation, app launch, or performance test has been performed as part of this research.
