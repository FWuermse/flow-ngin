# ADR-0003: Input Data Flow Through the GraphicsFlow Hierarchy

**Status:** Accepted
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

### Option E — Hybrid: shared `Value<T>` cell + optional `on_change` callback

Input widgets bind to an engine-provided `Value<T>` cell (`Rc<RefCell<T>>` wrapper). The widget mutates the value internally on user interaction. An optional `on_change` callback returning `Out<S, E>` fires after the mutation, allowing the application to react via the existing output plumbing.

```rust
let checked = Value::new(false);

Checkbox::<S, E>::new()
    .bind(&checked)
    .on_change(|new_val| Out::FutFn(vec![Box::new(async move {
        Box::new(move |s: &mut S| s.accepted = new_val) as Box<dyn FnOnce(&mut S)>
    })]));
```

The application can also read `checked.get()` on demand (e.g., on form submit) without needing a callback at all.

| Dimension | Assessment |
|---|---|
| Simplicity | Clean — `.bind(&val)` for state, optional `.on_change()` for side-effects |
| Type safety | Full — `Value<T>` is generic; callback parameter matches widget output type |
| Composability | Good — each widget binds its own cell; forms collect cells on demand |
| Performance | No per-frame overhead; value mutation + callback only on actual interaction |
| Form submission | Natural — read `Value<T>` cells directly, no event stream assembly needed |
| Widget coupling | Widgets are generic over `S`/`E` only if `on_change` is used; `Value<T>` is standalone |
| Trait bloat | None — no changes to `UIElement` or `GraphicsFlow` |

---

## Decision

**Option E (hybrid: `Value<T>` + optional callback)** is adopted.

Rationale:
- Combines the strengths of Option C (on-demand value reading) and Option D (callback-driven output).
- Widgets own their display state via `Value<T>`, so they can render the current value without round-tripping through application state.
- The optional `on_change` callback follows the pattern established by `Button::on_click`, keeping the API consistent.
- No changes to the `GraphicsFlow` or `UIElement` traits are required.
- `Value<T>` is a simple `Rc<RefCell<T>>` — lightweight, no trait bloat, no value enum.
- Forms can read `Value<T>` cells directly on submit, avoiding the event-variant explosion of Option A and the event-stream assembly problem.
- The callback is optional: simple use cases only need `.bind()`, while reactive use cases add `.on_change()`.

Reference implementation: `Checkbox<S, E>` in `src/ui/checkbox.rs`.

---

## Consequences

- The engine provides `Value<T>` (`Rc<RefCell<T>>` wrapper) in `src/ui/value.rs`.
- Input widgets (`TextInput`, `Checkbox`, `Slider`) accept `.bind(&Value<T>)` to hold a shared value cell.
- Widgets mutate the bound `Value<T>` directly in `on_update` when internal state changes.
- Widgets optionally accept `.on_change(Fn(T) -> Out<S, E>)` callbacks; the callback fires after the value mutation.
- The returned `Out` is propagated through `Container`'s `merge_outs`.
- A convenience method on `Out` (e.g., `Out::mutate`) may be added to reduce closure boilerplate.
- No changes to `GraphicsFlow`, `UIElement`, or `Layout` traits.
- `Container::merge_outs` remains the aggregation point for child outputs.
