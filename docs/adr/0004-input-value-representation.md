# ADR-0004: Input Value Representation and Composition

**Status:** Proposed
**Date:** 2026-03-17
**Priority:** Decide together with ADR-0003.

---

## Cross-references

- **ADR-0003** is a hard dependency: its adopted Option E (hybrid `Value<T>` + callbacks) influences but does not settle this ADR. If ADR-0003 had chosen Option C (shared value refs) instead, this ADR would need revisiting.
- **Input Widgets Plan** implements this: `TextInput` callbacks take `&str`, `Checkbox` takes `bool`, `Slider` takes `f32` — exactly the typed-widget pattern.
- If a **Form** component is added later (mentioned in ADR-0003 consequences and Option C here), it could be built on top of typed widgets without requiring a value enum — see Dynamic UI Plan Phase 1 for the runtime child management that a Form would also need.

---

## Context

UI input widgets will produce values of different types: strings (text input), booleans (checkbox), numbers (slider), and eventually composite values (forms with multiple fields). The engine needs a strategy for representing these values that balances type safety, ergonomics, and generality.

The ideas document proposes types "similar to what JSON supports" i.e. text, number, bool, list, composed, and considers two directions:
1. An enum of supported value types (like `serde_json::Value`).
2. Type generics on UI elements so values are statically typed.

This decision interacts with ADR-0003 (input data flow). If callbacks (Option D) are adopted, the value type appears as the callback parameter type, not as a shared data structure. This reduces the need for a universal value enum but still requires a decision about how widgets expose their typed values.

---

## Options Considered

### Option A — Universal value enum (`InputValue`)

```rust
pub enum InputValue {
    Text(String),
    Number(f64),
    Bool(bool),
    List(Vec<InputValue>),
    Composed(HashMap<String, InputValue>),
}
```

Widgets produce `InputValue` variants. Consumers downcast via pattern matching.

| Dimension | Assessment |
|---|---|
| Type safety | Weak — consumer must match and unwrap; wrong variant is a runtime error |
| Ergonomics | Easy to construct; tedious to consume (match + unwrap on every read) |
| Generality | Handles any shape; extensible via `Composed` |
| Compile-time checks | None — a text field could return `Bool` and compile fine |
| Trait requirements | Widgets implement a common `fn value() -> InputValue` |
| Rust idiom | Uncommon; Rust prefers static typing over dynamic value bags |

### Option B — Generic type parameter on input widgets

Each widget is parameterized by its value type:

```rust
pub struct TextInput<S, E> { /* on_change: Box<dyn Fn(&str) -> Out<S, E>> */ }
pub struct Checkbox<S, E> { /* on_change: Box<dyn Fn(bool) -> Out<S, E>> */ }
pub struct Slider<S, E>  { /* on_change: Box<dyn Fn(f32) -> Out<S, E>> */ }
```

The value type is implicit in the callback signature. No shared value enum is needed.

| Dimension | Assessment |
|---|---|
| Type safety | Strong — callback parameter type matches widget output at compile time |
| Ergonomics | Clean; no matching/unwrapping |
| Generality | Each widget type has a fixed value type; no dynamic composition |
| Compile-time checks | Full — wrong callback signature is a compile error |
| Trait requirements | None beyond existing `UIElement<S, E>` |
| Rust idiom | Standard; follows `Button`'s existing callback pattern |

### Option C — Hybrid: typed widgets + optional value enum for forms

Widgets are typed (Option B). For form-level collection, a `Form` component gathers values into a user-defined struct via typed accessors or builder closures. No runtime value enum unless the user explicitly opts into one.

```rust
// User defines their own form state
struct LoginForm { username: String, remember: bool }

// Form wiring
let form = Form::new()
    .with_text_input("username", |s: &mut LoginForm, v| s.username = v.into())
    .with_checkbox("remember", |s: &mut LoginForm, v| s.remember = v)
    .on_submit(|form_state: &LoginForm| { /* ... */ });
```

| Dimension | Assessment |
|---|---|
| Type safety | Strong — each field setter is typed |
| Ergonomics | Good for common cases; form struct is user-defined |
| Generality | User can define any struct shape; no engine-imposed schema |
| Compile-time checks | Full |
| Trait requirements | None beyond existing traits |
| Rust idiom | Idiomatic; user-defined types over engine-imposed enums |

---

## Recommendation

**Option B (typed widgets, no value enum)** is recommended.

---

## Consequences

If accepted:
- No `InputValue` enum is added to the engine.
- Each input widget has a fixed callback parameter type matching its natural output.
- Form composition is the responsibility of the application's state type `S`.
- Future composite widgets (e.g., `Form`) may provide convenience wiring but do not introduce a universal value type.
- The engine remains agnostic about value schemas, keeping complexity low.
