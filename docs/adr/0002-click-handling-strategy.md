# ADR-0002: Click Handling Strategy for UI Elements

**Status:** Accepted
**Date:** 2026-03-17
**Priority:** Decide first as all interactive widget work depends on this decision.

---

## Cross-references

- **Input Widgets Plan** depends on this: Checkbox, Slider, and TextInput all use coordinate-based detection in `on_update`. If Option A (GPU picking) were chosen instead, every widget design changes.
- **Dynamic UI Plan** depends on this: Drawer toggle and popup "click outside to close" assume coordinate-based detection. The `Animated` visibility trait interacts with click-shielding i.e. a hidden container should not block picks.
- **ADR-0002 open question #3** (UI-to-UI overlap / click consumption) also affects the **focus model** introduced by TextInput in the Input Widgets Plan. "Click outside to unfocus" and "popup captures clicks" are two faces of the same problem.

---

## Context

The engine provides two mechanisms for detecting user clicks on UI elements:

1. **GPU picking via `on_click(id)`** — the existing pipeline renders all objects to an offscreen `R32Uint` texture, reads back the pixel under the cursor, and dispatches `on_click(id)` to the owning flow(s). Every `Flat` and `Instanced` render carries a `u32` pick ID.
2. **Coordinate-based detection in `on_update`** — the component reads `ctx.mouse.coords` and `ctx.mouse.pressed` each frame and performs a pixel-rect containment test.

Currently, `Button` uses approach 2 exclusively: it tracks a `VisualState` (Normal / Hovered / Pressed) in `on_update` via `contains(pos.x, pos.y)` and fires the click callback when the mouse transitions from pressed to released while hovering. It renders all quads with `id: 0` (non-pickable). No UI element overrides `on_click`.

Meanwhile, `Container` propagates `on_update` to children (enabling Button hover/click) but does **not** propagate `on_click`. `on_click` has a default no-op implementation in `GraphicsFlow`, so forgetting to propagate or override it fails silently.

This creates an inconsistency: 3D objects use GPU picking, UI elements use coordinate math, and the two systems are unaware of each other. Clicking a UI element that overlaps a 3D object can trigger the 3D object's `on_click` because the UI element has `id: 0` (transparent to picking).

---

## Options Considered

### Option A — Standardize on GPU picking (`on_click`)

Every clickable UI element assigns a unique non-zero pick ID to its foreground quad. `Container` propagates `on_click` to all children (mirroring how it already propagates `on_update`). Hover detection remains coordinate-based in `on_update` (GPU picking is per-click, not per-frame).

| Dimension | Assessment |
|---|---|
| Click accuracy | Pixel-perfect; respects actual rendered geometry, not bounding rects |
| Overlap / z-order | Handled automatically — GPU renders front-to-back; only the topmost ID survives |
| Event blocking | Container with non-zero ID catches clicks, preventing pass-through to 3D scene |
| Hover support | Still requires coordinate math in `on_update` (no change) |
| ID management | Every clickable widget needs a unique ID; needs an allocator or convention |
| Text clicks | Glyphon renders via `Render::Custom` closure — unclear how to assign a pick ID to custom passes |
| Propagation burden | `Container` must forward `on_click` to every child; forgetting this is a silent bug (default is no-op) |
| Render correctness | Parent must trust that children render with the correct pick ID — no compile-time or runtime check that a child's `on_render` sets the expected ID |
| Idiomatic | Consistent with how the engine handles 3D interactions; `on_click` is the "flow-ngin way" |
| Perf cost | One extra GPU pass per click (already exists); no per-frame cost |

### Option B — Standardize on coordinate-based detection (`on_update`)

All UI clickable elements continue to use coordinate math for both hover and click. GPU pick IDs remain `0` for all UI elements. A non-zero container background could optionally consume the pick to block 3D pass-through.

| Dimension | Assessment |
|---|---|
| Click accuracy | Bounding-rect only; cannot handle non-rectangular or overlapping UI without custom logic |
| Overlap / z-order | Must be resolved manually by checking children front-to-back in `on_update` |
| Event blocking | Requires explicit "consume" flag or ordering convention to prevent 3D pass-through |
| Hover support | Unified with click — same `contains()` check handles both |
| Separation of concerns | Each widget fully owns its interaction logic; no dependency on render pipeline correctness |
| ID management | No IDs needed for UI; simpler setup |
| Text clicks | Works naturally — text bounding rect is known from glyphon layout |
| Propagation burden | Already works via `on_update` propagation in `Container` |
| Widget author cost | Every clickable widget must reimplement the press/release/hover state machine in `on_update`; click handling lives in `on_update` rather than the more intuitive `on_click`, which can surprise widget authors |
| Perf cost | Per-frame `contains()` per clickable element; negligible for O(100) widgets |

### Option C — Hybrid: coordinate-based for UI, GPU picking for 3D, with click blocking

UI elements keep coordinate-based detection. A `Container` rendered with a non-zero pick ID acts as a "click shield" that absorbs the GPU pick, preventing 3D objects behind it from receiving `on_click`. This formalizes the current Button approach while solving the overlap problem.

| Dimension | Assessment |
|---|---|
| Click accuracy | Bounding-rect for UI; pixel-perfect for 3D |
| Overlap / z-order | UI-to-3D overlap solved by shield container; UI-to-UI overlap still manual |
| Event blocking | Container pick ID blocks 3D pass-through without propagating on_click to children |
| Hover support | Same as Option B |
| Separation of concerns | Same as Option B — UI widgets own their interaction; 3D objects use the engine's picking |
| ID management | Only shield containers need IDs; clickable widgets do not |
| Text clicks | Works naturally |
| Propagation burden | No `on_click` propagation needed for UI children |
| Render correctness | Not an issue for UI widgets (no pick IDs); only shield containers need a correct ID |
| Idiomatic | Hybrid — UI interaction is custom, 3D interaction uses engine picking. Two mental models to learn |
| Widget author cost | Same as Option B — click logic in `on_update`, not `on_click`. Could be mitigated with a shared `Clickable` helper or trait providing the state machine |
| Perf cost | Same as Option B for UI; one GPU pick pass for 3D (already exists) |

---

## Recommendation

**Option C (hybrid with click-shield containers)** is recommended.

Rationale:
- It matches the existing implementation most closely, requiring minimal changes.
- Coordinate-based UI detection is simpler and already working for hover + click in `Button`.
- The only gap in the current system — 3D objects receiving clicks through UI overlays — is solved by giving the outermost `Container` a non-zero pick ID.
- GPU picking remains the correct tool for 3D scenes where geometry is complex and overlapping.
- Avoids the ID allocation and propagation complexity of Option A.
- Avoids the silent-failure risk of `on_click` default implementations being forgotten.
- Avoids the render-correctness trust issue of Option A (parent trusting children to set IDs correctly).
- Text and custom render passes work without modification.

Acknowledged trade-offs:
- Click logic lives in `on_update` rather than `on_click`, which is less idiomatic and can surprise widget authors. Mitigated by providing a shared `Clickable` helper (trait or function) that encapsulates the press/release/hover state machine, so widget authors don't reimplement it.
- Two mental models (coordinate-based for UI, GPU picking for 3D) must be documented clearly.

---

## Open Questions

- Should `on_click` lose its default implementation to make missing propagation a compile error? This would force all `GraphicsFlow` implementors to handle it, which is noisy for non-UI flows. An alternative is a clippy-style lint or a separate `Clickable` trait.
- Should `Container` expose a `.clickable(bool)` builder that assigns a pick ID, making the shield opt-in?
- For UI-to-UI overlap (e.g., a popup over a button), should `on_update` propagation stop at the first child that claims the click, or should all children see it?

---

## Consequences

If accepted:
- `Button` continues using coordinate-based detection (no change).
- `Container` gains an optional non-zero pick ID for click shielding.
- `Container` does **not** need to propagate `on_click` to children.
- New clickable UI widgets follow Button's pattern: hover + click in `on_update`.
- Documentation should clarify the two click systems and when each applies.
