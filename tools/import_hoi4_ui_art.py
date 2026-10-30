#!/usr/bin/env python3
"""Build *local* HOI4 artwork previews without redistributing proprietary art.

    py -3 -m pip install Pillow
    py -3 tools/import_hoi4_ui_art.py --game "D:\SteamLibrary\steamapps\common\Hearts of Iron IV"

Reads .gfx sprite declarations, country flags and character portrait references.
Images are saved to the ignored project's data/hoi4/ui folder as small PNGs.
"""
from __future__ import annotations
import argparse
import re
from collections import Counter, OrderedDict
from pathlib import Path

SAFE = re.compile(r"^[A-Za-z0-9_-]{1,120}$")
SPRITE = re.compile(r"\bspriteType\s*=\s*\{", re.I)
CHAR = re.compile(r"\b([A-Za-z0-9_.-]+)\s*=\s*\{")
IMG_EXT = {".dds", ".png", ".tga", ".bmp", ".jpg", ".jpeg"}

def strip_comments(text: str) -> str:
    return "\n".join(line.split("#", 1)[0] for line in text.splitlines())

def block_end(text: str, start: int) -> int:
    """Locate matching close brace, respecting comments and quoted paths."""
    depth = 0
    quoted = None
    comment = False
    escaped = False
    for pos in range(start, len(text)):
        ch = text[pos]
        if comment:
            if ch == "\n": comment = False
            continue
        if quoted:
            if escaped: escaped = False
            elif ch == "\\": escaped = True
            elif ch == quoted: quoted = None
            continue
        if ch == "#":
            comment = True
            continue
        if ch in ("'", '"'):
            quoted = ch
            continue
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0: return pos
    return -1

def blocks(source: str, offset: int = 0):
    """Only immediate children of a Paradox object; nested entries are skipped."""
    at = 0
    while (match := CHAR.search(source, at)):
        brace = match.end() - 1
        end = block_end(source, brace)
        if end == -1: break
        yield match.group(1), source[brace + 1:end]
        at = end + 1

def field(text: str, name: str) -> str | None:
    match = re.search(r"\b" + re.escape(name) + r"\s*=\s*(?:\"([^\"]+)\"|'([^']+)'|([\w./\\-]+))",
                      text, re.I)
    return next((group for group in match.groups() if group is not None), None) if match else None

def sprite_definitions(source: str):
    pos = 0
    while (match := SPRITE.search(source, pos)):
        brace = match.end() - 1
        end = block_end(source, brace)
        if end == -1: break
        chunk = source[brace + 1:end]
        name = field(chunk, "name")
        path = field(chunk, "texturefile")
        frames = field(chunk, "noOfFrames")
        if name and path:
            yield name, path, max(1, min(200, int(frames))) if frames and frames.isdigit() else 1
        pos = end + 1

def source_path(game: Path, path: str) -> Path | None:
    parts = path.replace("\\", "/").split("/")
    if ".." in parts: return None
    loc = game.joinpath(*[p for p in parts if p and p != "."]).resolve()
    if not loc.is_relative_to(game): return None
    return loc if loc.is_file() else None

def aliases(sprite: str):
    if not sprite.startswith("GFX_"): return []
    name = sprite[4:]
    variants = [name]
    for suffix in ("_medium", "_small", "_large", "_icon"):
        if name.endswith(suffix):
            variants.append(name[:-len(suffix)])
    if name.startswith("technology_"): variants.append(name[len("technology_"):])
    if name.startswith("tech_"): variants.append(name[len("tech_"):])
    return [alias for alias in dict.fromkeys(variants) if SAFE.fullmatch(alias)]

def categories(path: str):
    lower = path.replace("\\", "/").lower()
    if any(k in lower for k in ("/goals/", "national_focus", "focus_tree")):
        return ("focus",)
    if any(k in lower for k in ("technolog", "research", "/tech/")):
        return ("research",)
    if any(k in lower for k in ("equipment", "tank", "artillery", "infantry", "armor", "aircraft", "plane", "naval", "ship", "motorized", "trucks")):
        return ("equipment",)
    return ("research", "equipment")

def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--game", type=Path, required=True, help="Installed HOI4 root")
    ap.add_argument("--out", type=Path, default=Path("data/hoi4/ui"))
    ap.add_argument("--limit", type=int, default=7500)
    args = ap.parse_args()
    if args.limit < 1 or args.limit > 15000: ap.error("--limit must be between 1 and 15000")
    try:
        from PIL import Image
    except ImportError:
        ap.error("Pillow is required: py -3 -m pip install Pillow")
    game = args.game.resolve()
    if not (game / "gfx").is_dir() or not (game / "interface").is_dir():
        ap.error("Expected the original game folders gfx/ and interface/ under --game")
    out = args.out.resolve()
    counts = Counter()
    cache = OrderedDict()
    def write_preview(source: Path, destination: Path, frames=1):
        if destination.is_file(): return False
        destination.parent.mkdir(parents=True, exist_ok=True)
        try:
            key = (str(source),frames)
            if key not in cache:
                with Image.open(source) as image:
                    image.load()
                    width = image.width // frames
                    if width < 1: raise ValueError("invalid spritesheet")
                    image = image.crop((0,0,width,image.height)).convert("RGBA")
                    image.thumbnail((192,192),Image.Resampling.LANCZOS)
                    cache[key] = image.copy()
                    if len(cache) > 32:
                        cache.popitem(last=False)
            cache.move_to_end(key)
            cache[key].save(destination,"PNG")
            return True
        except (OSError, ValueError, MemoryError):
            counts["decode_failed"] += 1
            return False

    sprites = {}
    sprite_files = sorted(set((game/"interface").rglob("*.gfx")) | set((game/"gfx"/"interface").rglob("*.gfx")))
    for file in sprite_files:
        try:
            content = file.read_text(encoding="utf-8-sig",errors="replace")
            for name, file_path, frames in sprite_definitions(content):
                resolved = source_path(game,file_path)
                if resolved and resolved.suffix.lower() in IMG_EXT:
                    sprites.setdefault(name,(resolved,frames,file_path))
        except OSError:
            counts["gfx_read_failed"] += 1
    counts["sprite_defs"] = len(sprites)

    for name, (source,frames,declared) in sorted(sprites.items()):
        if counts["art_files"] >= args.limit: break
        for category in categories(declared):
            for key in aliases(name):
                target = out/category/(key+".png")
                if write_preview(source,target,frames):
                    counts["art_files"] += 1
    # Prefer explicit icons declared by real technology/equipment definitions.
    # A technology name is not always identical to its GFX sprite name.
    for directory, folder_names, target_category in [
        (game/"common"/"technologies", {"technologies"}, "research"),
        (game/"common"/"units"/"equipment", {"equipments"}, "equipment"),
    ]:
        if not directory.is_dir(): continue
        for filename in sorted(directory.rglob("*.txt")):
            try: content = filename.read_text(encoding="utf-8-sig",errors="replace")
            except OSError: continue
            for container, body in blocks(content):
                definitions = blocks(body) if container in folder_names else [(container,body)]
                for object_id, definition in definitions:
                    if not SAFE.fullmatch(object_id): continue
                    reference = field(definition,"icon") or field(definition,"picture")
                    if not reference: continue
                    # HOI4 picture IDs sometimes omit the GFX_ prefix.
                    options = [reference, "GFX_"+reference]
                    matched = next((sprites[k] for k in options if k in sprites), None)
                    if matched:
                        sprite_file, frames, _ = matched
                        if write_preview(sprite_file,out/target_category/(object_id+".png"),frames):
                            counts["explicit_icons"] += 1
    # Flags: country tags and special scripted flag variants are kept by name.
    flag_dir = game/"gfx"/"flags"
    for path in sorted(flag_dir.glob("*")) if flag_dir.is_dir() else []:
        if path.suffix.lower() not in IMG_EXT or not SAFE.fullmatch(path.stem): continue
        if write_preview(path,out/"flags"/(path.stem+".png")):
            counts["flags"] += 1

    civilians = {}
    # Parse character IDs from the original common/characters schema:
    # characters = { TAG_character = { portraits = { army = {...} civilian = {...} } } }
    character_dir = game/"common"/"characters"
    if character_dir.is_dir():
        for path in sorted(character_dir.rglob("*.txt")):
            try: source = path.read_text(encoding="utf-8-sig",errors="replace")
            except OSError: continue
            for container, body in blocks(source):
                if container != "characters": continue
                for char_id, definition in blocks(body):
                    if not SAFE.fullmatch(char_id): continue
                    portraits = next((p for n,p in blocks(definition) if n=="portraits"),"")
                    refs = {}
                    for role, content in blocks(portraits):
                        if role in ("army", "civilian", "navy", "operative"):
                            name = field(content,"large") or field(content,"small")
                            if name: refs[role]=name
                    def resolve_portrait(name: str | None):
                        if not name: return None
                        if name in sprites: return sprites[name]
                        file = source_path(game,name) if Path(name).suffix.lower() in IMG_EXT else None
                        return (file,1,name) if file else None
                    selected = resolve_portrait(refs.get("army") or refs.get("navy") or refs.get("civilian"))
                    if selected:
                        file, frames, _ = selected
                        if write_preview(file,out/"portraits"/(char_id+".png"),frames):
                            counts["commander_portraits"] += 1
                    civilian_image = resolve_portrait(refs.get("civilian"))
                    if civilian_image:
                        civilians[char_id] = civilian_image
    # Country portrait candidates: use explicit recruit_character references
    # where they resolve to civilian sprites. This is a historical candidate,
    # not a full dynamic ideology/party-leader selection algorithm.
    history = game/"history"/"countries"
    if history.is_dir():
        for path in sorted(history.glob("*.txt")):
            tag = path.name[:3]
            if not tag.isupper() or not SAFE.fullmatch(tag): continue
            try: source = path.read_text(encoding="utf-8-sig",errors="replace")
            except OSError: continue
            ids = re.findall(r"\brecruit_character\s*=\s*([A-Za-z0-9_-]+)",strip_comments(source))
            selected = next((civilians[id] for id in ids if id in civilians),None)
            if selected:
                file, frames, _ = selected
                if write_preview(file,out/"portraits"/("country_"+tag+".png"),frames):
                    counts["country_portraits"] += 1
    for key,value in sorted(counts.items()): print(f"{key}: {value}")
    print(f"Local previews: {out}")
    print("These images come from your installed game; do not commit or redistribute them.")

if __name__ == "__main__":
    main()
