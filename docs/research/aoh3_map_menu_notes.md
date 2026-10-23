# AOC3 / AnalyticalEngine map and menu notes

Date: 2026-10-23. Scope: source files actually present in CTD-Games/AnalyticalEngine. These notes are architectural observations only; no AOC3 game assets or code are copied into HOI4 Mobile.

## What is present in the repository

The original AOC3 package tree contains many .class files for map and province classes, including:
- src/aoc/kingdoms/lukasz/map/map/Map.class
- src/aoc/kingdoms/lukasz/map/map/Map_Data.class
- src/aoc/kingdoms/lukasz/map/map/MapScale.class
- src/aoc/kingdoms/lukasz/map/province/Province.class
- src/aoc/kingdoms/lukasz/map/province/ProvinceDraw.class

The corresponding Java source for several of these core classes is not present at the expected .java paths. Do not claim their full implementation was inspected. Some surrounding source files are decompiled and explicitly marked by FernFlower.

## Renderer observations

File: src/aoc/kingdoms/lukasz/jakowski/Renderer/Renderer.java  
GitHub: https://github.com/CTD-Games/AnalyticalEngine/blob/main/src/aoc/kingdoms/lukasz/jakowski/Renderer/Renderer.java

The file is explicitly marked as recreated from a .class file by IntelliJ IDEA / FernFlower. In the inspected source:
- libGDX OrthographicCamera and FitViewport are used for the world view;
- SpriteBatch is used for rendering, with a separate SpriteBatch for borders;
- the frame render path calls Game.map.drawMap(this.oSB);
- scale changes are based on Game.mapScale.getCurrentScale(), and update the camera world size / viewport and projection matrices;
- this is a camera-and-batch renderer connected to the existing AOC3 model, not a neutral map-rendering library.

Design ideas for our own project:
1. Keep camera transform / zoom independent from province data.
2. Render the province raster with nearest-neighbour filtering; avoid smoothing that blends province RGB identifiers.
3. Render optional borders and overlays as separate layers, so map modes do not mutate the source province raster.
4. Convert input through the inverse camera transform before sampling map pixels.
5. Profile memory use before caching extra full-size bitmaps on Android.

## Province detail / selection UI

File: src/AnalyticalEngine/Java/MapEditor/ProvinceDrawDetails.java  
GitHub: https://github.com/CTD-Games/AnalyticalEngine/blob/main/src/AnalyticalEngine/Java/MapEditor/ProvinceDrawDetails.java

This is also decompiled code. Its public method defaultDrawDetails(SpriteBatch, int nProvinceID) receives a province ID and draws a province information panel using the game model and numerous managers (resources, terrain, regions, technology, plague, wonders, etc.). It demonstrates a useful UI pattern: selection resolves to a stable province ID, and the detail panel queries domain data by that ID. It is not a reusable Rust panel and depends on AOC3-specific classes.

## Main menu observations

File: src/AnalyticalEngine/Java/MPCompatibility/AnalyticalEngine_MainMenu.java  
GitHub: https://github.com/CTD-Games/AnalyticalEngine/blob/main/src/AnalyticalEngine/Java/MPCompatibility/AnalyticalEngine_MainMenu.java

The main-menu class builds a list of MenuElement objects and positions them using CFG / image dimensions and padding. It conditionally shows a Continue button when a saved game can be resumed, then adds a New Game button whose action changes the current view to the scenario-selection screen. The UI is tightly coupled to AOC3's MenuManager, View, images, save managers, localisation and game state.

Transferable design:
- Main menu is a list of explicit actions, not game simulation logic.
- Continue should only be shown/enabled when a valid save exists.
- New Game should transition to scenario selection, then initialize a game world.
- The view/navigation state should be separate from simulation and map data.

Do not transfer:
- AOC3 class names, game data, textures or decompiled implementations.
- Its global singleton coupling as the architecture for the new Rust project.

## How this affects our current prototype

The Rust prototype uses a small screen-state enum (MainMenu, MapViewer, Settings) and loads a local province raster plus definition.csv. This follows the broad separation of menu actions from map rendering, but does not yet implement camera zoom/pan, state boundaries, scenario selection or save-game continuation.

Next implementation steps:
1. Add touch/mouse zoom and pan using a camera transform.
2. Make map coordinate conversion use that same transform.
3. Add state-file parsing and draw state borders as an overlay.
4. Add map-data validation and report missing/duplicate IDs.
5. Only after desktop source checks, configure and test Android packaging.

No local build or runtime test has been performed.
