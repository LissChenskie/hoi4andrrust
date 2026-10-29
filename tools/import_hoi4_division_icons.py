#!/usr/bin/env python3
"""Import icon previews from the user's own installed HOI4. No assets are redistributed.

Usage: py -3 tools/import_hoi4_division_icons.py --game "C:/Games/Hearts of Iron IV"
Requires: py -3 -m pip install Pillow
"""
from __future__ import annotations
import argparse
import re
from pathlib import Path

def sprite_records(text: str):
    # SpriteType blocks in subuniticons.gfx carry flat name/textureFile
    # declarations. Ignore unrelated nested objects rather than guessing them.
    for match in re.finditer(r"\bspriteType\s*=\s*\{([^{}]*)\}", text, re.I | re.S):
        body = re.sub(r"(?m)#.*$", "", match.group(1))
        name = re.search(r"\bname\s*=\s*[\"']?([\w]+)", body, re.I)
        texture = re.search(r"\btextureFile\s*=\s*[\"']([^\"']+)", body, re.I)
        frames = re.search(r"\bnoOfFrames\s*=\s*(\d+)", body, re.I)
        if name and texture:
            yield name.group(1), texture.group(1), int(frames.group(1)) if frames else 1

def normalized_path(game: Path, name: str) -> Path | None:
    name = name.replace("\\", "/")
    components = [p for p in name.split("/") if p and p != "."]
    if any(p == ".." for p in components): return None
    path = game.joinpath(*components).resolve()
    return path if path.is_relative_to(game.resolve()) else None

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--game", required=True, type=Path)
    parser.add_argument("--out", type=Path, default=Path("data/hoi4/ui/division_icons"))
    args = parser.parse_args()
    try:
        from PIL import Image
    except ImportError:
        parser.error("Pillow required: py -3 -m pip install Pillow")
    game = args.game.resolve()
    src = game / "interface" / "subuniticons.gfx"
    if not src.is_file():
        parser.error(f"Missing {src}; provide the installed HOI4 root.")
    candidates = [src, game / "interface" / "divisiondesignerview.gfx"]
    copied = 0
    missed = 0
    args.out.mkdir(parents=True, exist_ok=True)
    for file in candidates:
        if not file.is_file(): continue
        text = file.read_text(encoding="utf-8-sig", errors="replace")
        for name, image_path, frames in sprite_records(text):
            kind = re.fullmatch(r"GFX_unit_(.+?)_icon_medium",name,re.I)
            group = re.fullmatch(r"GFX_group_(.+?)_icon",name,re.I)
            templ = re.fullmatch(r"GFX_div_templ_(\d+)_large",name,re.I)
            if kind: filename = kind.group(1)
            elif group: filename = "group_" + group.group(1)
            elif templ: filename = "division_" + templ.group(1)
            else: continue
            if not re.fullmatch(r"[\w-]{1,105}",filename): continue
            path = normalized_path(game,image_path)
            if path is None or not path.is_file():
                missed += 1
                continue
            try:
                with Image.open(path) as source:
                    source.load()
                    width = source.width // frames if frames > 1 else source.width
                    if width <= 0: raise ValueError("zero-width frame")
                    image = source.crop((0,0,width,source.height)).convert("RGBA")
                    image.thumbnail((128,128),Image.Resampling.LANCZOS)
                    image.save(args.out / f"{filename}.png",format="PNG")
                    copied += 1
            except (OSError, ValueError) as error:
                missed += 1
                print(f"Skipped {path.name}: {error}")
    print(f"Imported {copied} sprites to {args.out} (unresolved: {missed}).")
    print("Converted assets stay on this PC; do not commit or redistribute them.")
if __name__ == "__main__":
    main()
