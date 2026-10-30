# Construction UI parity notes

## Implemented

The existing Macroquad construction screen now supports:
- Reordering the global construction queue by dragging a project's building icon to another visible row.
- Priority arrows for single-step changes, or Shift+arrow for moving to the beginning/end.
- Shift+click on a state in construction placement mode to enqueue as many valid buildings as remaining slots permit. Each project is validated independently.
- Ctrl+click to insert new construction at the start of the national queue; Ctrl+Shift+click performs top-priority batch placement.
- A selected-state information footer showing shared building slots (built + reserved), infrastructure level, building type count (built + queued), queue length, and control eligibility.

The existing factory scheduler, build-progress/ETA display and map availability overlay are retained. Priority updates change the actual global construction order used by the civilian-factory allocator.

## Known gaps

This is a focused UI improvement, **not** full HOI4 visual or mechanical parity. This codebase currently models eight state building types. Airfields, ports, fortifications, radar, and anti-air require additional gameplay models and province-level targeting before they can function properly. Do not show placeholder construction buttons that create nonfunctional orders. Original copyrighted HOI4 GUI assets are not bundled.

## Regression checklist

1. Build a factory in a state, verify correct resource allocation and queue date.
2. Shift+click until state slots are full; verify no overbooking.
3. Ctrl+click, check that the project moves to the top and gains priority.
4. Drag a lower-priority construction icon above the first one, verify assignments and completion times update.
5. Shift+up/down on a queue row, verify first/last position.
6. Cancel a project, confirm building slot reservation and queue indices are correct.
7. Select a different building and a state, verify state details update.
8. Close construction mode and verify the placement overlay is cleared.
9. Test a long scrolling construction queue on desktop and touch screens.

Run `cargo test` and `cargo fmt --check` locally or via CI to validate changes.
