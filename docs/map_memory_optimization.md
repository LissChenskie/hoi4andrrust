# Map memory optimization (October 2026)

This prototype loads a local HOI4-compatible `provinces.bmp` and corresponding
`definition.csv`. No map assets are distributed in this repository.

## Improvements

1. **State raster**: the pixel-to-state lookup is stored as `Vec<u16>` when every state
   ID fits in 16 bits, with an automatic lossless `Vec<u32>` fallback for mods
   using larger IDs. No full-image u32 buffer is built first.
2. **Map modes**: only the political texture is resident by default.
   Per-province political/resource/terrain colours are precomputed once in a
   small colour table. The terrain or resource texture is generated on demand
   and the previous mode texture is dropped when switching. The separate
   border texture remains resident. This trades a CPU colour pass on mode
   switches for lower persistent GPU memory usage.
3. **Border generation**: checks neighbouring source RGB colours and state IDs
   directly instead of creating separate full-image u32 province and owner
   rasters. Owners are mapped to compact IDs per state, not per pixel.
4. **Selected-state outline**: the bounding rectangle for every state is
   recorded while decoding the state raster. Only that region is uploaded as
   a highlight texture, and its drawing is clipped to the visible map window.
   For unusually scattered states, the bounds could still cover a large area.
5. **Map reload**: releases the previous map's graphics resources before
   constructing a replacement. On reload failure the old map is not retained.

## Theoretical image-buffer budget (not measured device RAM)

Example: a map of `5632 x 2048` pixels contains `11,534,336` pixels.
A full RGBA8 image or GPU texture is **44 MiB**; a 16-bit state raster is
**22 MiB**.

| Buffer / resource | Before | Now |
|---|---:|---:|
| State IDs (typical vanilla-width IDs) | 44 MiB | 22 MiB |
| Three base map-mode RGBA8 GPU textures | 132 MiB | 44 MiB political + 44 MiB *only while viewing another mode* |
| Province + owner temporary u32 rasters for border building | 88 MiB | 0 |
| Highlight when one state is selected | 44 MiB | 4 bytes x selected state's rectangular pixel area |

Theoretical steady-state reduction from mode textures and state IDs alone:
**110 MiB**, before accounting for driver allocation, CPU staging, metadata,
or a selected-state outline. These are byte-count calculations, **not FPS
measurements or device profiling**. Other allocations remain (especially the
border texture, raw RGB hit-test pixels and temporary construction heatmap).

## Correctness and next work

Run `cargo test` and `cargo check`. Unit tests cover u16/u32 state ID
fallback, local outline pixels, clipping, and resource/terrain colour parity.

Next for low-end Android phones:
- measure actual RAM/VRAM and frame pacing on a physical device;
- tile the map by GPU-supported texture size and render only visible tiles;
- avoid rebuilding a full-world heatmap when only one state's queue changes;
- expose local-file import through Android's document picker.

These are not yet implemented. In particular, avoiding permanent textures
does **not** solve devices with a maximum single-texture dimension smaller
than the imported map.
