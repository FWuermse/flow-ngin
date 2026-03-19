# ADR-0003: Input Data Flow Through the GraphicsFlow Hierarchy

**Status:** Proposed
**Date:** 2026-03-17
**Priority:** Decide together with ADR-0004 (value representation). Both must be settled before implementing input widgets.

---

## Cross-references

- **ADR-0004** depends on this: its recommendation (typed widgets, no value enum) only makes sense if this ADR's callback approach (Option D) is chosen. If Option C (shared value refs) were chosen here, ADR-0004 would need an `InputValue` enum instead.
- **Input Widgets Plan** implements this decision — every widget's `on_change` callback follows the pattern chosen here.
- **`merge_outs` dropping `Configure`** is noted here and also hits the **Dynamic UI Plan** (Phase 1 runtime child addition wants `Out::Configure` to propagate). Consider fixing `merge_outs` as a shared prerequisite for both efforts.
- **`Out::mutate` convenience helper** suggested in Consequences would reduce boilerplate across all input widgets and the Dynamic UI Plan's Drawer toggle logic.

---

## Context

The UI library needs interactive input widgets (text fields, checkboxes, sliders) that produce values. These values must reach the application in a way that is composable — a form with several inputs of different types should be expressible without boilerplate.

Currently, the only output path from a UI element to the application is the `Out<S, E>` enum:

```rust
pub enum Out<S, E> {
    FutEvent(Vec<Box<dyn Future<Output = E>>>),
    FutFn(Vec<Box<dyn Future<Output = Box<dyn FnOnce(&mut S)>>>>),
    Configure(Box<dyn FnOnce(&mut Context)>),
    Empty,
}
```

`Button` uses `Out::FutEvent` to emit a user-defined event `E` on click. `Container` merges children's outputs via `merge_outs`, which flattens `FutEvent` and `FutFn` vecs but **drops `Configure`**.

For input widgets, the question is: how does the value (a string, a bool, a number) get from the widget to the application code?

---

## Options Considered

### Option A — Event-per-keystroke via `Out::FutEvent`

Each input widget emits an event on every state change (keystroke, toggle, drag). The application event enum `E` includes variants like `TextChanged(String)`, `CheckToggled(bool)`, `SliderMoved(f32)`.

| Dimension | Assessment |
|---|---|
| Simplicity | Straightforward; reuses existing `Out` plumbing |
| Type safety | Events are typed by `E`; application defines the variants |
| Composability | Each widget needs its own event variant — forms with N inputs need N variants |
| Performance | One async future per keystroke (overhead for high-frequency input) |
| Form submission | Application must assemble final form state from a stream of events |
| Widget coupling | Widgets must know about `E` to construct the event; requires closures or generic callbacks |

### Option B — State mutation via `Out::FutFn`

Each input widget mutates application state `S` directly via `Out::FutFn` closures. The widget is given a setter closure at construction time (e.g., `.on_change(|s, val| s.username = val)`).

| Dimension | Assessment |
|---|---|
| Simplicity | Similar to event approach but skips the event enum |
| Type safety | Closures are typed; compiler ensures state field types match |
| Composability | No event variants needed; each widget gets its own closure |
| Performance | Same async future overhead as Option A |
| Form submission | State is always up-to-date; "submit" just reads current state |
| Widget coupling | Widgets are generic over `S`; setter closures capture the field path |

### Option C — Shared value references (extractable inputs)

Input widgets hold their current value internally. A parent component (e.g., a Form) can extract values from children on demand. For example, when a submit button is clicked. This requires a way to query children for their values.

One approach: a new trait method `fn value(&self) -> Option<InputValue>` on `UIElement`, where `InputValue` is an enum of supported types. The form iterates children and collects values.

| Dimension | Assessment |
|---|---|
| Simplicity | More complex; adds a new trait method and a value enum |
| Type safety | `InputValue` enum loses per-field type safety at extraction time |
| Composability | Good — form collects all values in one pass, no event wiring |
| Performance | No per-keystroke overhead; values extracted on demand |
| Form submission | Natural — "submit" triggers extraction and validation |
| Widget coupling | Widgets are self-contained; no knowledge of `S` or `E` needed |
| Trait bloat | Adds a method to `UIElement` that non-input widgets must stub out |

### Option D — Callback closures (no trait changes)

Input widgets accept an `on_change` callback at construction time (like Button's `on_click`). The callback receives the current value and returns `Out<S, E>`. The widget calls this callback in `on_update` when the value changes, and the returned `Out` is propagated normally.

```rust
TextInput::new()
    .on_change(|value: &str| Out::FutFn(vec![Box::new(async move {
        let v = value.to_owned();
        Box::new(move |s: &mut S| s.username = v) as Box<dyn FnOnce(&mut S)>
    })]))
```

| Dimension | Assessment |
|---|---|
| Simplicity | Moderate — callback wiring is verbose but mechanical |
| Type safety | Full; callback and `Out` are typed |
| Composability | Good — each widget is independently wired |
| Performance | Callback only fires on change, not every frame |
| Form submission | State updated via callbacks; submit reads state |
| Widget coupling | Widgets are generic over callback return type |
| Trait bloat | None — no changes to `UIElement` or `GraphicsFlow` |

---

## Recommendation

**Option D (callback closures)** is recommended.

Rationale:
- It follows the pattern already established by `Button::on_click`, which accepts a callback that returns an event `E`. Extending this to `on_change` callbacks for input widgets is consistent.
- No changes to the `GraphicsFlow` or `UIElement` traits are required.
- The application chooses whether to use `FutEvent` or `FutFn` in the callback, keeping flexibility.
- Avoids trait method bloat from Option C's `value()` method.
- Avoids the event-variant explosion of Option A.
- The callback verbosity can be reduced with helper functions (e.g., `Out::mutate(|s| s.field = val)`).

For form-level extraction (collecting all inputs at once), a `Form` component can combine Option D with internal value storage: each child widget updates both its internal value (for display) and the application state (via callback). A submit button then just signals that the current state is final.

---

## Consequences

If accepted:
- Input widgets (`TextInput`, `Checkbox`, `Slider`) accept `on_change` callbacks returning `Out<S, E>`.
- Widgets call the callback in `on_update` when internal state changes.
- The returned `Out` is propagated through `Container`'s `merge_outs`.
- A convenience method on `Out` (e.g., `Out::mutate`) may be added to reduce closure boilerplate.
- No changes to `GraphicsFlow`, `UIElement`, or `Layout` traits.
- `Container::merge_outs` remains the aggregation point for child outputs.
