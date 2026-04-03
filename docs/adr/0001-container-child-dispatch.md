# ADR-0001: Container Child Dispatch Strategy

**Status:** Accepted
**Date:** 2026-03-10

---

## Context

`Container` holds a collection of heterogeneous UI children (`Icon`, `TextLabel`, and future types) and must dispatch two operations to each child:

1. **Layout**: `resolve(parent_x, parent_y, parent_w, parent_h, queue)` computes and uploads the child's pixel position.
2. **Render**: `on_render()` returns a `Render` variant to be composed into the frame.

Two dispatch strategies were evaluated.

---

## Options Considered

### Option A: Closed enum (`UIChild`)

```rust
pub enum UIChild { Icon(Icon), Label(TextLabel) }
```

The container holds `Vec<UIChild>` and dispatches via `match`.

| Dimension | Assessment |
|---|---|
| Runtime perf | Zero-cost: branch prediction, no heap alloc per child |
| User creates UI | Simple — typed `with_icon` / `with_label` methods |
| User adds custom widget | **Not possible** without forking the library |
| Lib adds new type | Edit enum + every match arm — touches multiple files |
| Boilerplate per new lib type | ~5 lines (variant + match arms) |
| Boilerplate per new user type | N/A |

### Option B — Open supertrait (`UIElement<S, E>`)

```rust
pub trait UIElement<S, E>: GraphicsFlow<S, E> + Layout {}
impl<T, S, E> UIElement<S, E> for T where T: GraphicsFlow<S, E> + Layout {}
```

The container holds `Vec<Box<dyn UIElement<S, E>>>` and dispatches via vtable.

| Dimension | Assessment |
|---|---|
| Runtime perf | Vtable dispatch + one `Box` alloc per child (negligible for UI) |
| User creates UI | Simple — single `with_child` method, accepts any `UIElement` |
| User adds custom widget | **Yes** — implement `GraphicsFlow<S,E>` + `Layout`, blanket impl does the rest |
| Lib adds new type | Just implement the two traits — no central file to edit |
| Boilerplate per new lib type | ~1 line (blanket impl covers it automatically) |
| Boilerplate per new user type | ~15–20 lines (two trait impls) |

## Decision

**Option B (supertrait) is adopted.**

The performance difference is negligible: UI containers hold O(10–100) elements and render once per frame. A vtable call per child is not measurable. In exchange, the library becomes open for extension without modification — any type that implements `GraphicsFlow<S, E>` and `Layout` is automatically a valid container child via the blanket impl. This scales correctly as the widget set grows and enables user-defined widgets.

The `UIChild` enum is removed. `Container<S, E>` holds `Vec<Box<dyn UIElement<S, E>>>` and exposes a single `with_child` builder method.

## Consequences

- `UIElement<S, E>` is introduced as a public supertrait in `src/ui/layout.rs` (or a new `src/ui/element.rs`).
- The blanket impl `impl<T, S, E> UIElement<S, E> for T where T: GraphicsFlow<S, E> + Layout {}` means zero per-type registration in the library.
- `UIChild` enum and its match arms in `container.rs` are removed.
- `Container::with_icon` and `Container::with_label` are replaced by `Container::with_child`.
- Users implementing custom widgets must implement `GraphicsFlow<S, E>` and `Layout`; no other ceremony is required.
- `Layout` and `UIElement` are re-exported from `flow_ngin::ui` for discoverability.
