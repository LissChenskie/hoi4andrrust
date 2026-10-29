# Local HOI4 division icon import

The designer uses **the user's installed HOI4 game assets**, without putting any
Paradox-owned raster images in the public repository. Source definitions:
`interface/subuniticons.gfx` (unit and group icons),
`interface/divisiondesignerview.gfx` (optional division insignia).
Source texture paths are resolved **relative to the original game installation**.

On Windows in the project root:

```powershell
py -3 -m pip install Pillow
py -3 tools/import_hoi4_division_icons.py --game "E:\перекид с ссд\games\Hearts of Iron IV"
cargo run --release
```

If your game is elsewhere, change `--game`; the importer prints a summary
and skips sprite formats unsupported by your local Pillow. Generated previews
go in `data/hoi4/ui/division_icons/*.png`; no game originals are shipped.
The engine dynamically looks for unit IDs or `group_<group>` as a fallback.
If an icon is absent, the designer draws a simple original vector symbol.

This is a local-only import process; do not commit or redistribute converted
game sprites. Icon availability depends on installed DLCs and mods. The runtime
never silently grants a researched battalion or changes template data.
