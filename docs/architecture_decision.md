# Initial architecture decisions

Date: 2026-10-23  
Status: prototype decisions; revisit after the first Android device test.

## 1. Scope the first milestone

Build only the main menu and a map viewer. Do not begin combat, country AI, focus trees, economy or diplomacy before the map can be loaded, selected and validated.

## 2. Rendering layer: Macroquad prototype

The first UI uses Macroquad because the initial milestone needs straightforward 2D drawing and touch-oriented interaction. This is a provisional choice, not a claim that Android packaging or performance has already been verified. The project should be tested on the target Android device before committing to a larger rendering architecture.

The map model must not depend on the UI library. If a later renderer is needed, map data and selection logic should remain reusable.

## 3. Map data is not a single image asset

Treat the source province raster and its metadata as a linked dataset:
- provinces.bmp: raster whose pixel RGB identifies a province;
- definition.csv: maps province IDs to RGB colours and province metadata;
- state history files: later source for grouping province IDs into states;
- further layers (terrain, rivers, heightmap, strategic regions, supply areas, positions and adjacencies) are later milestones.

Do not redraw or recolour the source province bitmap to make the first viewer work. Use nearest-neighbour texture filtering. Keep state borders as a derived overlay, not as destructive changes to the province image.

## 4. Province selection

Screen coordinates are converted into source-raster coordinates. The exact RGB value at that pixel is resolved through a reverse index (RGB -> province ID). This makes selection independent of the rendering scale.

The first prototype parses the essential numeric columns from definition.csv. It does not yet parse the full Clausewitz grammar, validate every file rule, or read state ownership.

## 5. Preserve IDs and report invalid data

Never renumber provinces in the viewer. Later validation should report:
- duplicate province IDs;
- duplicate province colours;
- raster colours with no definition;
- definitions not used by any raster pixel;
- disconnected components and unusual topology;
- malformed or duplicate state membership.

The current prototype checks duplicate RGB colours while loading definitions and reports basic file/bitmap errors. More complete validation remains future work.

## 6. Asset and licensing policy

The public repository contains source code and documentation, not a copy of the HOI4 map, game files, textures or decompiled game classes. Local map files are excluded through .gitignore. Use map tooling projects to understand formats and algorithms, but do not reuse code from repositories with missing or unclear licensing until rights are established.

AnalyticalEngine is an AOC3 modding layer, not a source of HOI4 geography. Its decompiled Java files should not be transplanted into this project.

## 7. Known limitations

- Android file picker / SAF import is not implemented.
- The app currently expects a relative local path.
- State files, state borders, zoom/pan, and map-layer switching are not implemented.
- The map view displays the source raster and province ID only; no simulation is present.
- No local compilation, test run, Android build, device launch or performance measurement has been performed.
