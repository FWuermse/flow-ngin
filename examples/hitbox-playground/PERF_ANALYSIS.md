# Performance Analysis: Why BruteForce Appears Fastest

## TL;DR

BruteForce wins in the playground at typical object counts (N≤30) for two compounding reasons:

1. **Example code issue**: the collision backend is fully reconstructed from scratch on every frame in `overlay_flow.rs`, erasing any advantage spatial structures have in amortized query cost.
2. **Implementation issue**: the dense grid pre-allocates up to 1000 empty cell vectors on construction; the dedup logic in both grid variants runs extra floating-point and method-call overhead per insert that outweighs the partitioning benefit at small N.

BruteForce is not fundamentally faster at large N — it is just the only strategy with near-zero constant-factor overhead.

---

## 1. Per-Frame Reconstruction (Example Code Issue)

### `overlay_flow.rs` — every frame, unconditionally

```rust
// on_update() line 85 — fires every frame
let mut backend = CollisionBackend::new(state.strategy, state.detection_dims);

for placed in &state.placed {
    let hb = make_hitbox(placed.position, placed.shape, placed.id);
    backend.insert(hb);          // N inserts
}
let candidates = backend.hit_candidates(drag_hb.clone());  // 1 query
```

The backend is thrown away and rebuilt for every rendered frame regardless of whether anything changed. For a typical 120 FPS session with N=20 objects, this means 120 × 20 = **2400 inserts per second** that exist solely to answer one query.

### `partition_viz_flow.rs` — conditional rebuild (correct)

This flow correctly caches and only rebuilds when strategy, dims, or placed count changes. It is not a performance problem.

### What changes for each strategy at construction

| Strategy | `CollisionBackend::new()` cost |
|---|---|
| BruteForce | Allocate one `Vec` (capacity 0) — O(1) |
| SparseGrid | Allocate one `HashMap` (empty) — O(1) with init overhead |
| SpatialTree | Allocate one root `SpatialTree` node — O(1) |
| Grid 2D | Allocate `(WORLD_HALF/CELL_SIZE)^2 = (10/2)^2 = 25` cell Vecs — O(25) |
| Grid 3D | Allocate `(10/2)^3 = 125` cell Vecs — O(125) |

The dense grid constructor allocates 25–125 empty `Vec<T>` values every frame before the first insert. This alone explains much of the gap versus BruteForce.

---

## 2. Per-Insert Cost Breakdown (Implementation Analysis)

### BruteForce::insert (collision.rs ~line 646)

```rust
fn insert(&mut self, hitbox: T) -> Vec<T> {
    let candidates = self.hitboxes.clone();   // O(n) — return everyone
    self.hitboxes.push(hitbox);
    candidates
}
```

For N=20 objects, total clones across all inserts: 0+1+2+…+19 = **190 item clones**, 20 pushes. Zero floating-point arithmetic, zero partitioning logic. Single flat `Vec` — sequential memory access.

### HitGridND::insert (collision.rs ~line 487)

Per insert:
1. `cell_ranges()` — calls `hitbox.interval(d)` for each dimension, computes `floor()` twice per dim.
2. `for_each_cell()` — iterates K cells (K = cells the hitbox touches, typically 1–4 for a unit object in a 2.0-cell grid).
3. **Per cell**: runs the lex-smallest dedup check against every existing item in that cell:
   - Calls `other.interval(d)` and `hitbox.interval(d)` for each dimension
   - Computes `floor()`, `max()`, comparison per dimension
   - ~8 float ops + 2 method calls per dimension per existing item per cell
4. Clones each reported candidate and clones the hitbox into the cell.

For N=20 objects in 2D, assuming ~1 cell touched per object:
- **~160 extra float ops** for lex-dedup checks (20 inserts × 1 cell × ~4 existing items average × 2 dims × 2 ops)
- **~200 clones** (similar count to BruteForce, but scattered across cell Vecs)
- **Worse cache behavior** — iterating cells[idx] where idx jumps around the pre-allocated array

### SparseHitGridND::insert (collision.rs ~line 596)

Same cell iteration and dedup as the dense grid, plus:
- `HashMap::entry(*coord).or_default()` per cell — hash computation of `[i32; N]` on every access
- For K=2 cells per insert, N=20 inserts: **40 HashMap hash computations** per frame
- HashMap's pointer indirection on lookup hurts cache locality further

The Sparse grid is consistently slower than the dense grid for this example's small, dense object placement.

### SpatialTree::insert (collision.rs ~line 289)

Two regimes:

**Leaf phase (first threshold=4 objects):** O(m) clone of current leaf items per insert — comparable to BruteForce.

**Split phase (triggered at m ≥ threshold):**
- `bounds.split()` computes 2^N sub-bounds (4 for 2D, 8 for 3D)
- Allocates 2^N new `SpatialTree` nodes
- Re-inserts all existing items into children: O(m × 2^N) submerges checks and clones
- For 3D at first split: **O(4 × 8) = 32 operations** to redistribute 4 items into 8 subtrees

After the tree is built, individual inserts are O(log N) — but in this example the tree is rebuilt every frame from scratch, so the split cascade fires again at insert #5 every single frame. For N=20 objects, multiple splits cascade (at depths 1, 2, 3), meaning **several expensive split events per frame** that would otherwise be one-time costs.

---

## 3. Algorithmic Complexity at Scale

The playground forces every strategy into its worst case by choosing N that is too small to amortize spatial structure overhead:

| Strategy | Break-even vs BruteForce | At N=20 | At N=200 |
|---|---|---|---|
| BruteForce | never faster | fastest | slowest (O(n²)) |
| Dense Grid | ~N=50–100 | slower (alloc overhead) | faster |
| Sparse Grid | ~N=80–150 | slower (hash + alloc) | competitive |
| SpatialTree | ~N=30–60 (persistent) | slower (split overhead) | fastest |

The break-even numbers shift dramatically if the backend is **not** rebuilt every frame. A persistent SpatialTree with 100 objects and incremental inserts would run query in O(log N) per frame instead of O(N log N) for reconstruction.

---

## 4. Root Causes Summary

### Definitively example code problems

| Problem | Location | Impact |
|---|---|---|
| Full backend rebuild every frame | `overlay_flow.rs:85` | Grid pays 25–125 Vec allocations per frame; tree re-triggers split cascade |
| Two backends running in parallel | `overlay_flow.rs` + `partition_viz_flow.rs` | When partition_viz triggers rebuild, both pay reconstruction cost simultaneously |

### Definitively implementation inefficiencies

| Problem | Location | Impact |
|---|---|---|
| Dense grid pre-allocates all cells at construction | `collision.rs` HitGridND constructor | O(cells_per_dim^N) allocation even when most cells are empty |
| Lex-dedup calls `interval()` twice per dimension per existing item per cell | `collision.rs` ~line 510 | Extra method dispatch overhead vs BruteForce's zero-logic insert |
| SpatialTree re-runs full split cascade on every fresh construction | `collision.rs` ~line 313 | One-time tree-building cost paid every frame |

### Not a problem (works as designed)

- BruteForce returning all N candidates (correct; narrow phase in `overlay_flow` filters via `overlaps()`)
- Clone-heavy API (needed for the multi-flow ownership model)
- Grid cell sizes relative to object sizes (CELL_SIZE=2, HALF=0.5 gives good occupancy)

---

## 5. What Would Make Spatial Structures Win

1. **Persistent backend** — build once, insert incrementally. Grid and Tree amortize their construction cost over thousands of queries rather than rebuilding for one.
2. **Larger N** — at 100+ objects, BruteForce's O(n²) clone cost exceeds grid/tree query cost even with reconstruction overhead.
3. **Clustered placement** — SparseGrid shines when objects are concentrated in a small region; it avoids allocating empty cells.
4. **Batch query** — querying all N objects against each other (not just the drag cursor) would expose the grid's true O(n/k) per-query advantage over BruteForce's O(n).
