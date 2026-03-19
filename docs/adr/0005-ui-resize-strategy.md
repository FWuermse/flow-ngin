# ADR-0005: UI Resize Strategy

**Status:** Accepted
**Date:** 2026-03-18

---

## Context

When the window is resized, the wgpu surface, depth texture, and 3D camera projection are correctly reconfigured (`flow.rs:231–247`). However, the UI layer does not respond: all components (`Container`, `Button`, `Card`, `Icon`, `TextLabel`, etc.) cache `screen_width` / `screen_height` at `on_init` time and never update them. Vertex buffers with stale NDC coordinates continue to be rendered against the new surface dimensions, causing the UI to appear stretched, misaligned, or clipped.

### Root cause

`pixels_to_ndc()` converts pixel positions to clip-space coordinates using a screen-size denominator. When the denominator becomes stale, every UI quad is rendered at the wrong position and scale. The `Layout::resolve()` tree is only walked once during initialization.

### What already works on resize

- `ctx.config.width/height` is updated
- `surface.configure()` is called
- Depth texture is recreated
- 3D projection aspect ratio is updated
- Picking texture is rendered at the correct new size
- Glyphon text viewport uses current dimensions

### What breaks on resize

- All UI component `screen_width/screen_height` fields remain stale
- `pixels_to_ndc()` produces wrong NDC values
- Vertex buffers are not regenerated
- Layout tree is not re-resolved
- Button/Icon hover detection uses stale screen-space rects

---

## Options Considered

### Option A — Re-resolve the layout tree on resize

After `state.resize()` updates `ctx.config`, walk every top-level `GraphicsFlow` that is also a `UIElement` and call `resolve(0, 0, new_w, new_h, &queue)`. Each component's `resolve()` already updates its vertex buffer via `queue.write_buffer`; it just needs the stored `screen_width/screen_height` refreshed first.

**Concrete changes:**

1. Add a method to `Layout` (or a new `Resizable` trait) that propagates the new screen size before re-resolving:
   ```rust
   fn resize_screen(&mut self, screen_w: u32, screen_h: u32);
   ```
2. In `AppState::resize()`, after reconfiguring the surface, iterate `graphics_flows` and call `resize_screen` + `resolve` on each.
3. Each component updates its cached `screen_width/screen_height` in `resize_screen` and forwards to children.

| Dimension | Assessment |
|---|---|
| Scope of change | Small — add one method per component, one call site in `resize()` |
| Correctness | Full — re-resolves the entire layout tree with correct dimensions |
| Performance | Re-uploads vertex buffers for every UI quad on each resize event; negligible for typical UI sizes (O(10–100) elements) |
| Existing API compatibility | `Layout::resolve()` signature unchanged; new method is additive |
| Handles nested layouts | Yes — `Container::resolve()` already recurses into children |

### Option B — Use a GPU uniform for screen dimensions instead of baking NDC into vertices

Replace the per-vertex NDC bake with a uniform buffer containing `(screen_width, screen_height)`. The vertex shader divides pixel coordinates by the uniform to produce clip-space positions. On resize, only the single uniform buffer needs updating.

**Concrete changes:**

1. Add a `screen_size: Buffer` uniform to the GUI pipeline bind group layout.
2. Change `vertices_from_coords` to emit pixel coordinates instead of NDC.
3. Modify the vertex shader to: `clip_x = -1.0 + 2.0 * pixel_x / screen_w` (and analogous for y).
4. On resize, `queue.write_buffer(&screen_size_uniform, ...)` — one write, all quads correct.

| Dimension | Assessment |
|---|---|
| Scope of change | Medium — shader change, pipeline layout change, vertex format change, all components that create vertices |
| Correctness | Full — all quads automatically correct after one uniform update |
| Performance | Best — single 8-byte buffer write per resize; no per-element vertex re-upload |
| Existing API compatibility | Breaking — vertex data format changes, shader changes, bind group layout changes |
| Handles nested layouts | Position resolution still needed, but NDC conversion moves to GPU |

### Option C — Handle `WindowEvent::Resized` in `on_window_events` per component

Each component that caches screen dimensions listens for `WindowEvent::Resized` in its existing `on_window_events` hook, updates its cached values, and re-resolves itself.

**Concrete changes:**

1. Each UI component implements `on_window_events` to match `WindowEvent::Resized`.
2. On match, update `self.screen_width/screen_height`, recompute NDC, re-upload vertex buffer.

| Dimension | Assessment |
|---|---|
| Scope of change | Small per component, but scattered across every UI type |
| Correctness | Fragile — each component must independently handle resize correctly; easy to miss one |
| Performance | Same as Option A (per-element vertex re-upload) |
| Existing API compatibility | No trait changes; uses existing `on_window_events` hook |
| Handles nested layouts | Partially — each component re-resolves independently, but parent→child dimension propagation requires the parent to re-resolve children too |
| Event ordering issue | Currently `on_window_events` is dispatched **before** `state.resize()` updates `ctx.config` (flow.rs:811–826), so `ctx.config.width/height` would still be stale when the component handles the event. This requires reordering the event dispatch or extracting size from the event directly. |

---

## Recommendation

**Option C** is not recommended: it scatters resize logic across every component, has an event-ordering problem, and doesn't naturally handle parent→child dimension propagation.

Both **A** and **B** are correct. The performance difference is irrelevant since window resizes are rare. Option B wins on simplicity: it eliminates `screen_width/screen_height` fields from every component and removes `pixels_to_ndc()` entirely. Components only deal in pixel coordinates; the GPU handles the conversion.

---

## Decision

**Option B (GPU uniform) is adopted.**

A `ScreenSize` uniform buffer containing `(width, height)` is added at bind group 1 for both the GUI and GUI-pick pipelines. The vertex shader performs the pixel→NDC conversion. On resize, a single `queue.write_buffer` call updates the uniform — all quads are immediately correct with no per-element work.

---

## Consequences

- `pixels_to_ndc()` is removed; replaced by `pixels_to_frame()` which stores raw pixel coordinates.
- `screen_width` / `screen_height` fields are removed from `Icon`, `Container`, `Button`, `Checkbox`, and `Grid`.
- `vertices_from_coords()` now emits pixel-space positions instead of NDC.
- `icon.wgsl` and `pick_gui.wgsl` vertex shaders read a `ScreenSize` uniform (group 1, binding 0) and compute NDC in the shader.
- `mk_gui_pipeline` and `mk_gui_pick_pipelin` accept a `&BindGroupLayout` for the screen size uniform.
- `Context` gains a `ScreenSizeResources` struct (buffer + bind group + layout).
- `AppState::resize()` writes the new dimensions to the uniform buffer — no TODO remains.
- The render pass sets `bind_group(1, screen_size, &[])` once before iterating GUI elements.
- New UI components no longer need to cache or propagate screen dimensions — they work purely in pixel space.
