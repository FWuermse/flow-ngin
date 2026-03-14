# ADR-0002: Golden Image Test Hook Strategy

**Status:** Proposed
**Date:** 2026-03-10

---

## Context

Golden image tests capture a rendered frame and compare it pixel-by-pixel against a fixture to catch visual regressions. The test runner needs a way to receive the captured texture and run the comparison.

The current implementation adds `render_to_texture` as a required method on `GraphicsFlow<S, E>`, gated by `#[cfg(feature = "integration-tests")]`:

```rust
#[cfg(feature = "integration-tests")]
fn render_to_texture(
    &self,
    ctx: &Context,
    state: &mut S,
    texture: &mut image::ImageBuffer<image::Rgba<u8>, wgpu::BufferView>,
) -> Result<ImageTestResult, anyhow::Error>;
```

Because the method has no default body, every `GraphicsFlow` implementor — including all UI primitives (`Icon`, `Container`, `Card`, `Button`, `TextLabel`) — must provide it. Their implementations are identical no-ops (`Ok(ImageTestResult::Passed)`) that add noise with no value.

### Goals

- Run golden image tests that validate rendered output against fixture images.
- Keep test boilerplate minimal; test files should contain only test-specific logic.
- Keep UI component source files clean; no test concerns should appear in production code.

---

## Options Considered

### Option A — Default method body in `GraphicsFlow`

Add a default body to `render_to_texture` that returns `Ok(ImageTestResult::Passed)`. Only the top-level test flow wrapper overrides it with real comparison logic.

```rust
#[cfg(feature = "integration-tests")]
fn render_to_texture(
    &self,
    _ctx: &Context,
    _state: &mut S,
    _texture: &mut image::ImageBuffer<image::Rgba<u8>, wgpu::BufferView>,
) -> Result<ImageTestResult, anyhow::Error> {
    Ok(ImageTestResult::Passed)
}
```

**Pros**
- One-line change; zero impact on any UI component.
- No new types, no API changes.

**Cons**
- Test concern remains visible on the core trait interface.
- A contributor reading `GraphicsFlow` sees a test-only method with no obvious purpose.

**Effort:** Minimal
**Rating:** ⭐⭐⭐

---

### Option B — Closure-based test runner (no trait method)

Remove `render_to_texture` from `GraphicsFlow` entirely. Introduce a separate `run_golden_test` function that accepts the flow constructor plus a comparison closure. The runner captures the frame and invokes the closure; no trait method is needed anywhere.

```rust
// In test file — render_to_texture does not exist on any type
flow_ngin::flow::run_golden_test(
    constructor,
    |ctx: &Context, state: &mut FrameCounter, texture: &ImageBuffer<_>| {
        if state.frame() == 0 {
            return Ok(ImageTestResult::Waiting);
        }
        let expected = image::open("tests/fixtures/card_golden.png")?.to_rgba8();
        compare_pixels(texture, &expected)
    },
)?;
```

**Pros**
- `GraphicsFlow` has zero test surface — cleanest possible separation of concerns.
- UI components are completely untouched.
- Test logic is co-located with the test, not scattered across the trait hierarchy.

**Cons**
- Requires a new `run_golden_test` entry point in `flow.rs`.
- The closure captures state (e.g. frame counter timing) by reference; ownership must be managed carefully.

**Effort:** Medium
**Rating:** ⭐⭐⭐⭐⭐

---

### Option C — `GoldenImageFlow<F>` wrapper (test-only adapter)

Keep `render_to_texture` on `GraphicsFlow` with a default body (same as Option A), but provide a `GoldenImageFlow<F>` wrapper in `test_utils` that accepts a comparison closure. Every test constructs `GoldenImageFlow::new(my_component, |ctx, state, texture| { ... })` instead of hand-rolling a full `GraphicsFlow` impl.

```rust
// Defined once in tests/common/test_utils.rs — cfg-gated
pub struct GoldenImageFlow<F, S, E> {
    inner: F,
    compare: Box<dyn Fn(&Context, &mut S, &ImageBuffer<Rgba<u8>, BufferView>)
        -> Result<ImageTestResult, anyhow::Error>>,
}

impl<F: GraphicsFlow<S, E>, S, E> GraphicsFlow<S, E> for GoldenImageFlow<F, S, E> {
    fn on_init(&mut self, ctx: &mut Context, state: &mut S) -> Out<S, E> {
        self.inner.on_init(ctx, state)
    }
    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        self.inner.on_render()
    }
    // ... delegate remaining methods ...

    fn render_to_texture(&self, ctx: &Context, state: &mut S, texture: &mut ...) -> Result<...> {
        (self.compare)(ctx, state, texture)
    }
}

// Usage in test — no boilerplate struct, no manual GraphicsFlow impl
let flow = GoldenImageFlow::new(
    Card::new(50, 50, 200, 300)
        .with_background_texture(bg)
        .with_icon(icon)
        .with_label(TextLabel::new("Hero")),
    |_ctx, state, texture| {
        if state.frame() == 0 { return Ok(ImageTestResult::Waiting); }
        let expected = image::open("tests/fixtures/card_golden.png")?.to_rgba8();
        compare_pixels(texture, &expected)
    },
);
```

**Pros**
- UI components are untouched.
- Test files shrink to just the component setup + comparison closure.
- Compatible with the existing `flow::run` API; no runner changes needed.
- `GoldenImageFlow` is written once and reused for all future golden tests.

**Cons**
- `GoldenImageFlow` itself needs to delegate all `GraphicsFlow` methods — boilerplate, but contained to one file.
- Still requires Option A's default body to avoid compilation failures on UI components when the feature is enabled.

**Effort:** Medium
**Rating:** ⭐⭐⭐⭐

---

### Option D — Blanket impl via a separate `Testable<S>` trait

Split `render_to_texture` into its own `Testable<S>` trait with a blanket impl that returns `Passed` for all types. The test runner accepts `F: GraphicsFlow<S,E> + Testable<S>`. The test flow overrides `Testable<S>` with real comparison logic.

```rust
#[cfg(feature = "integration-tests")]
pub trait Testable<S> {
    fn render_to_texture(&self, ...) -> Result<ImageTestResult, anyhow::Error> {
        Ok(ImageTestResult::Passed)
    }
}

// Blanket impl — every type gets the no-op default
#[cfg(feature = "integration-tests")]
impl<T, S> Testable<S> for T {}
```

**Pros**
- `GraphicsFlow` is entirely free of test concerns.
- UI components need no changes.

**Cons**
- Rust's orphan and coherence rules make overriding a blanket impl impossible without a newtype — the test flow cannot specialize `Testable<S>` for itself.
- Adds a second trait to the public API surface for a testing-only concern.
- Trait object composition (`dyn GraphicsFlow + Testable`) is not object-safe in Rust.

**Effort:** High, with fundamental Rust coherence blockers
**Rating:** ⭐⭐

---

## Decision

**Pending.** Options B and C are the strongest candidates.

- **B** is the cleanest long-term architecture but requires a new `run_golden_test` API.
- **C** is the best incremental solution: combine Option A's default body fix with the `GoldenImageFlow` wrapper to eliminate per-test boilerplate while keeping the runner API stable.

A hybrid approach is also viable: implement C now to unblock testing, then migrate to B in a follow-up when the runner API can be revisited.

---

## Consequences

- Whichever option is chosen, UI component files (`icon.rs`, `container.rs`, `card.rs`, `button.rs`, `text_label.rs`) will require **no changes**.
- The `render_to_texture` method will either gain a default body (A/C) or be removed from the trait (B).
- Future golden image tests will require only a component setup block and a comparison closure — no `GraphicsFlow` boilerplate structs.
