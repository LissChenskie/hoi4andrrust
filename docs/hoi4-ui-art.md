# Local Hearts of Iron IV artwork for the Rust UI

This independent port does **not** bundle or redistribute HOI4 graphics.
All icons, flags and portraits come from the user's own local game installation.
The generated previews stay under \`data/hoi4/ui/\` (ignored by Git).

## Import on Windows

From the project root, using the installed HOI4 location:

\`\`\`powershell
git switch main
git pull --ff-only origin main
py -3 -m pip install Pillow
py -3 tools/import_hoi4_ui_art.py --game "E:\перекид с ссд\games\Hearts of Iron IV"
cargo run --release
\`\`\`

Run the script again after a HOI4 update or DLC changes, and **restart** the
game to invalidate the UI texture cache. Remove the local \`data/hoi4/ui\`
folder first if you need to replace previously generated images (the importer
avoids overwriting an existing preview).

## Asset locations and lookup

- \`data/hoi4/ui/research/<tech-id>.png\` – research tree and active slots.
- \`data/hoi4/ui/equipment/<equipment-id>.png\` – production lines/model picker.
- \`data/hoi4/ui/flags/<country-tag>.png\` – country flags (original .tga path is a fallback).
- \`data/hoi4/ui/portraits/<character-id>.png\` – generals and marshals.
- \`data/hoi4/ui/portraits/country_<tag>.png\` – **historical civilian portrait
  candidate** from the country's \`recruit_character\` references. The politics
  system does not yet dynamically select the active head of state.

The importer scans original \`interface/**/*.gfx\` sprite declarations,
\`common/characters/*.txt\`, \`history/countries/*.txt\` and \`gfx/flags\`.
Sprite references must resolve to real local textures; no arbitrary fallback
images are claimed as originals. Graphics missing from the user's installed
version (or unsupported DDS compression) keep the existing vector UI icon.

When a particular historical model/technology uses a differently named sprite,
add the correct locally converted PNG with the *exact* simulation ID, as above.
The importer never modifies game logic, researched technologies, army templates,
or equipment stockpiles.

**Do not upload the generated previews or copied game archives to GitHub.**
