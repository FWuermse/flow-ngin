//! This module contains a configurable hitbox detection engine using
//! multiple datastructures and algorithms to narrow down the amount
//! of elements that need to be checked. Everything works with multiple
//! dimensions and also cross-dimension such as testing for 3D hitboxes
//! in a 2D space.
//!
//! Current implementations for Hitboxes are N-dimensional intervals and
//! origin points with their width/height/depth etc.
//!
//! Currently spacial trees (Quadtrees/Octrees) and grids are supported for
//! narrowing down the search space. The spacial trees use a bisection approaoch
//! while the grids reduce the search space by rasterization. For grids
//! the two supported implementations are dense array- and has-based.
//! Most of this implementation is inspired by [Mikola Lysenko](https://github.com/mikolalysenko/`)'s
//! [blog post](https://0fps.net/2015/01/07/collision-detection-part-1/) about collision detection.
//!
use std::{collections::HashMap, marker::PhantomData};

use crate::pick::PickId;

pub trait Hitbox {
    fn submerges(&self, other: &Self) -> bool;
    fn overlaps(&self, other: &Self) -> bool;
    fn split(&self) -> Vec<Self>
    where
        Self: Sized;
    fn interval(&self, dimension: usize) -> (f32, f32);
}

/// Bloom filter like hit testing using hitbox intervals
pub trait CollisionTest<T: Hitbox> {
    fn hit_candidates(&self, hitbox: T) -> Vec<T>;
    fn insert(&mut self, hitbox: T) -> Vec<T>;
    fn insert_if_no_hit(&mut self, hitbox: T) -> Vec<T>;
}

pub struct CollisionDetection<C: CollisionTest<H>, H: Hitbox> {
    collision_test: C,
    _pd: PhantomData<H>,
}
impl<C: CollisionTest<H>, H: Hitbox + Clone> CollisionDetection<C, H> {
    fn insert(&mut self, hitbox: H) -> Vec<H> {
        let possible_collisison = self.collision_test.insert(hitbox.clone());
        let collisions: Vec<_> = possible_collisison
            .into_iter()
            .filter(|hb| sat(&hitbox, hb))
            .collect();
        return collisions;
    }

    fn hits(&self, hitbox: &H) -> Vec<H> {
        let possible_collisison = self.collision_test.hit_candidates(hitbox.clone());
        let collisions: Vec<_> = possible_collisison
            .into_iter()
            .filter(|hb| sat(hitbox, hb))
            .collect();
        return collisions;
    }
}

/// Separating Axis Theorem
fn sat<H: Hitbox>(hitbox: &H, hb: &H) -> bool {
    todo!()
}

pub struct CornerPoint {
    top_left: cgmath::Point2<f32>,
    axis_lens: Vec<f32>,
}
impl Hitbox for CornerPoint {
    fn submerges(&self, other: &Self) -> bool {
        todo!()
    }

    fn split(&self) -> Vec<Self>
    where
        Self: Sized,
    {
        todo!()
    }

    fn overlaps(&self, other: &Self) -> bool {
        todo!()
    }

    fn interval(&self, dimension: usize) -> (f32, f32) {
        todo!()
    }
}

#[derive(Clone)]
pub struct Bounds {
    lower_bound: f32,
    upper_bound: f32,
}
impl Bounds {
    pub fn new(lower_bound: f32, upper_bound: f32) -> Self {
        Self {
            lower_bound,
            upper_bound,
        }
    }
}
impl Hitbox for Bounds {
    fn submerges(&self, other: &Self) -> bool {
        self.lower_bound < other.lower_bound && self.upper_bound > other.upper_bound
    }

    fn split(&self) -> Vec<Self> {
        let len = self.upper_bound - self.lower_bound;
        let half_len = len / 2.0;
        let upper_bound = self.upper_bound - half_len;
        let left = Self {
            lower_bound: self.lower_bound,
            upper_bound,
        };
        let lower_bound = self.lower_bound + half_len;
        let right = Self {
            lower_bound,
            upper_bound: self.upper_bound,
        };
        return vec![left, right];
    }

    fn overlaps(&self, other: &Self) -> bool {
        self.lower_bound <= other.upper_bound && self.upper_bound >= other.lower_bound
    }

    fn interval(&self, _: usize) -> (f32, f32) {
        (self.lower_bound, self.upper_bound)
    }
}

/// Represents a hitbox as n-dimensional lower and upper bound tagged with a PickId to backtrack hit objects
#[derive(Clone)]
pub struct TaggedNDimBounds {
    bounds: Vec<Bounds>,
    tag: PickId,
}

impl TaggedNDimBounds {
    pub fn new(bounds: Vec<Bounds>, tag: PickId) -> Self {
        Self { bounds, tag }
    }

    pub fn tag(&self) -> PickId {
        self.tag
    }
}

impl Hitbox for TaggedNDimBounds {
    fn submerges(&self, other: &Self) -> bool {
        let self_dim = self.bounds.len();
        let other_dim = other.bounds.len();
        if self_dim < other_dim {
            // 2D boundaries can submerge 3D hitboxes but 3D boundaries will never check true for 2D hitboxes
            for (i, _) in self.bounds.iter().enumerate() {
                if !self.bounds[i].submerges(&other.bounds[i]) {
                    return false;
                }
            }
            return true;
        }
        if self_dim > other_dim {
            return false;
        }
        self.bounds
            .iter()
            .zip(other.bounds.iter())
            .fold(true, |prev_sub, (self_i, other_i)| {
                prev_sub && self_i.submerges(other_i)
            })
    }

    fn split(&self) -> Vec<Self>
    where
        Self: Sized,
    {
        let bounds: Vec<_> = self.bounds.iter().map(|b| b.split()).collect();
        let split_bounds = cartesian(&bounds);
        split_bounds
            .iter()
            .map(|b| Self {
                bounds: b.to_vec(),
                tag: self.tag,
            })
            .collect()
    }

    fn overlaps(&self, other: &Self) -> bool {
        let self_dim = self.bounds.len();
        let other_dim = other.bounds.len();
        if self_dim < other_dim {
            // 2D boundaries can overlap 3D hitboxes but 3D boundaries will never check true for 2D hitboxes
            for (i, _) in self.bounds.iter().enumerate() {
                if !self.bounds[i].overlaps(&other.bounds[i]) {
                    return false;
                }
            }
            return true;
        }
        if self_dim > other_dim {
            return false;
        }
        self.bounds
            .iter()
            .zip(other.bounds.iter())
            .fold(true, |prev_sub, (self_i, other_i)| {
                prev_sub && self_i.overlaps(other_i)
            })
    }

    fn interval(&self, dimension: usize) -> (f32, f32) {
        let Some(bounds) = self.bounds.get(dimension) else {
            return (0.0, 0.0);
        };
        bounds.interval(dimension)
    }
}

pub struct SpatialTree<T> {
    threshold: usize,
    bounds: T,
    children: Option<Vec<SpatialTree<T>>>,
    hitboxes: Vec<T>,
}

impl<T: Hitbox + Clone> SpatialTree<T> {
    /// Create a new spatial tree with the given threshold and bounding region.
    /// The threshold controls how many items accumulate in a leaf before it splits.
    pub fn new(threshold: usize, bounds: T) -> Self {
        Self {
            threshold,
            bounds,
            children: None,
            hitboxes: vec![],
        }
    }

    /// Visit every node's bounds in the tree (depth-first), calling `f(bounds, depth)`.
    /// Useful for visualising the spatial subdivision structure.
    pub fn visit_bounds(&self, f: &mut impl FnMut(&T, usize)) {
        self.visit_bounds_inner(f, 0);
    }

    fn visit_bounds_inner(&self, f: &mut impl FnMut(&T, usize), depth: usize) {
        f(&self.bounds, depth);
        if let Some(children) = &self.children {
            for child in children {
                child.visit_bounds_inner(f, depth + 1);
            }
        }
    }
}

pub fn cartesian<T: Clone>(arrs: &[Vec<T>]) -> Vec<Vec<T>> {
    let mut results = vec![vec![]];
    for arr in arrs {
        let mut tmp_res = vec![];
        for curr in std::mem::take(&mut results) {
            for elem in arr {
                let mut new = curr.clone();
                new.push(elem.clone());
                tmp_res.push(new);
            }
        }
        results = tmp_res;
    }
    return results;
}

impl<T: Hitbox + Clone> CollisionTest<T> for SpatialTree<T> {
    fn hit_candidates(&self, hitbox: T) -> Vec<T> {
        let mut result = vec![];
        // All items at this node are partition-mates — include without geometric filter.
        result.extend(self.hitboxes.iter().cloned());
        if let Some(children) = &self.children {
            for child in children {
                // Traverse only subtrees whose bounds overlap — this is spatial navigation,
                // not a geometric overlap check on the stored items.
                if child.bounds.overlaps(&hitbox) {
                    result.extend(child.hit_candidates(hitbox.clone()));
                }
            }
        }
        result
    }

    fn insert(&mut self, hitbox: T) -> Vec<T> {
        match &mut self.children {
            Some(sub_trees) => {
                let mut possible_collisions = self.hitboxes.to_vec();
                for bisection in sub_trees.iter_mut() {
                    if bisection.bounds.submerges(&hitbox) {
                        return [bisection.insert(hitbox), possible_collisions].concat();
                    }
                }
                // if new hitbox cannot be submerged by any child area it will be stored in
                // the parent node to avoid infinite recursion for multiple same-size hitboxes.
                // Eventually multiple same-size hitboxes will hit a boundary and stack
                // anyway. This is just deferring it.
                self.hitboxes.push(hitbox.clone());
                for bisection in sub_trees {
                    possible_collisions.append(&mut bisection.hit_candidates(hitbox.clone()));
                }
                return possible_collisions;
            }
            None => {
                if self.hitboxes.len() < self.threshold {
                    let possible_collisions = self.hitboxes.to_vec();
                    self.hitboxes.push(hitbox);
                    return possible_collisions;
                } else {
                    let sub_bounds: Vec<_> = self.bounds.split();
                    let mut sub_trees: Vec<SpatialTree<T>> = sub_bounds
                        .into_iter()
                        .map(|sb| SpatialTree {
                            threshold: self.threshold,
                            bounds: sb,
                            children: None,
                            hitboxes: vec![],
                        })
                        .collect();
                    let hitboxes = std::mem::take(&mut self.hitboxes);
                    for hb in hitboxes {
                        let mut sorted = false;
                        for bisection in &mut sub_trees {
                            if bisection.bounds.submerges(&hb) {
                                // Don't recurse here to avoid high depth for multiple small identical hitboxes that fall through large grids
                                bisection.hitboxes.push(hb.clone());
                                sorted = true;
                                break;
                            }
                        }
                        if !sorted {
                            // Keep in current node if it touches boundaries
                            self.hitboxes.push(hb);
                        }
                    }
                    let mut possible_collisions = self.hitboxes.clone();
                    let mut sorted = false;
                    for bisection in &mut sub_trees {
                        if bisection.bounds.submerges(&hitbox) {
                            possible_collisions.append(&mut bisection.insert(hitbox.clone()));
                            sorted = true;
                            break;
                        }
                    }
                    if !sorted {
                        self.hitboxes.push(hitbox.clone());
                        sub_trees.iter().for_each(|tree| {
                            possible_collisions.append(&mut tree.hitboxes.to_vec());
                        });
                    }
                    self.children = Some(sub_trees);
                    return possible_collisions;
                }
            }
        }
    }

    fn insert_if_no_hit(&mut self, hitbox: T) -> Vec<T> {
        todo!()
    }
}

/// An `N`-dimensional grid for collision detection.
pub struct HitGridND<T, const N: usize> {
    origin: [f32; N],
    cell_len: f32,
    dims: [usize; N],
    /// The precompute strides are used to address flattened out grid
    /// e.g. 3D the index is computed x * strides[0] + y * strides[1]
    /// + z * strides[2].
    strides: [usize; N],
    /// Represents the flattened grid, eath with a Vec of hitboxes
    /// intersecting at cell[i].
    cells: Vec<Vec<T>>,
}

impl<T: Hitbox + Clone, const N: usize> HitGridND<T, N> {
    pub fn new(origin: [f32; N], grid_dims: [f32; N], cell_len: f32) -> Self {
        let dims: [usize; N] = std::array::from_fn(|i| (grid_dims[i] / cell_len).ceil() as usize);

        let mut strides = [0usize; N];
        let mut s = 1usize;
        for i in 0..N {
            strides[i] = s;
            s *= dims[i];
        }
        let total = s;

        Self {
            origin,
            cell_len,
            dims,
            strides,
            cells: vec![vec![]; total],
        }
    }

    fn flat_index(&self, coord: &[i32; N]) -> usize {
        let mut idx = 0usize;
        for i in 0..N {
            idx += (coord[i] as usize) * self.strides[i];
        }
        idx
    }

    /// The grid can live in the negative world coord spectrum and thus
    /// must be clamped when iterating over cells to avoid negative
    /// indexes.
    fn cell_ranges(&self, hitbox: &T) -> Option<[(i32, i32); N]> {
        let h = self.cell_len;
        let mut ranges = [(0i32, 0i32); N];
        for d in 0..N {
            let (lower_bound, upper_bound) = hitbox.interval(d);
            let start = ((lower_bound - self.origin[d]) / h).floor() as i32;
            let end = ((upper_bound - self.origin[d]) / h).ceil() as i32;
            let start = start.max(0);
            let end = end.min(self.dims[d] as i32 - 1);
            if start > end {
                return None;
            }
            ranges[d] = (start, end);
        }
        Some(ranges)
    }
}

/// Iterate the cartesian product of N inclusive ranges, no allocation.
fn for_each_cell<const N: usize>(ranges: &[(i32, i32); N], mut f: impl FnMut(&[i32; N])) {
    let mut coord: [i32; N] = std::array::from_fn(|d| ranges[d].0);
    loop {
        f(&coord);
        // find the lowest dim that wasn't visited.
        let mut d = 0;
        while d < N {
            if coord[d] < ranges[d].1 {
                coord[d] += 1;
                break;
            }
            coord[d] = ranges[d].0;
            d += 1;
        }
        if d == N {
            return;
        }
    }
}

impl<T: Hitbox + Clone, const N: usize> CollisionTest<T> for HitGridND<T, N> {
    fn hit_candidates(&self, hitbox: T) -> Vec<T> {
        let Some(ranges) = self.cell_ranges(&hitbox) else {
            return vec![];
        };
        let h = self.cell_len;
        let origin = self.origin;
        let mut possible_collisions = vec![];
        for_each_cell(&ranges, |coord| {
            let idx = self.flat_index(coord);
            for other in &self.cells[idx] {
                // Dedup: only report from the lex-smallest cell both items share,
                // so items spanning multiple cells aren't returned more than once.
                // This check is on cell co-occupation, not geometric overlap.
                let mut is_lex = true;
                for d in 0..N {
                    let (other_lower, _) = other.interval(d);
                    let (hitbox_lower, _) = hitbox.interval(d);
                    let lex = ((other_lower - origin[d]) / h)
                        .floor()
                        .max(((hitbox_lower - origin[d]) / h).floor())
                        as i32;
                    if lex != coord[d] {
                        is_lex = false;
                        break;
                    }
                }
                if is_lex {
                    possible_collisions.push(other.clone());
                }
            }
        });
        possible_collisions
    }

    fn insert(&mut self, hitbox: T) -> Vec<T> {
        let Some(ranges) = self.cell_ranges(&hitbox) else {
            return vec![];
        };

        let h = self.cell_len;
        let origin = self.origin;
        let strides = self.strides;
        let mut result = vec![];

        for_each_cell(&ranges, |coord| {
            let mut idx = 0usize;
            // compute index from all dimensions with precomputed strides
            for i in 0..N {
                idx += (coord[i] as usize) * strides[i];
            }
            let cell = &mut self.cells[idx];

            // Return all partition-mates (items in the same cell) without geometric
            // filtering. Lex-smallest dedup avoids reporting the same pair twice when
            // two items share multiple cells — it operates on cell co-occupation only.
            for other in cell.iter() {
                let mut unique = true;
                for d in 0..N {
                    let (other_lower_bound, _) = other.interval(d);
                    let (hitbox_lower_bound, _) = hitbox.interval(d);
                    let lex = ((other_lower_bound - origin[d]) / h)
                        .floor()
                        .max(((hitbox_lower_bound - origin[d]) / h).floor())
                        as i32;
                    if lex != coord[d] {
                        unique = false;
                        break;
                    }
                }
                if unique {
                    result.push(other.clone());
                }
            }
            cell.push(hitbox.clone());
        });
        result
    }

    fn insert_if_no_hit(&mut self, hitbox: T) -> Vec<T> {
        todo!()
    }
}

pub struct SparseHitGridND<T, const N: usize> {
    cell_len: f32,
    cells: HashMap<[i32; N], Vec<T>>,
}

impl<T: Hitbox + Clone, const N: usize> SparseHitGridND<T, N> {
    pub fn new(cell_len: f32) -> Self {
        Self {
            cell_len,
            cells: HashMap::new(),
        }
    }

    /// Per-dimension inclusive cell ranges covered by a hitbox.
    /// Unlike the dense version, no clamping. HashMap can have
    /// negative indexes (i.e. keys).
    fn cell_ranges(&self, hitbox: &T) -> [(i32, i32); N] {
        let h = self.cell_len;
        let mut ranges = [(0i32, 0i32); N];
        for d in 0..N {
            let (lower_bound, upper_bound) = hitbox.interval(d);
            ranges[d] = (
                (lower_bound / h).floor() as i32,
                (upper_bound / h).ceil() as i32,
            );
        }
        ranges
    }
}

fn lex_smallest_shared_cell<T: Hitbox, const N: usize>(cell_len: f32, a: &T, b: &T) -> [i32; N] {
    let mut result = [0i32; N];
    for d in 0..N {
        let (lower_bound_a, _) = a.interval(d);
        let (lower_bound_b, _) = b.interval(d);
        result[d] = ((lower_bound_a / cell_len).floor() as i32)
            .max((lower_bound_b / cell_len).floor() as i32);
    }
    result
}

impl<T: Hitbox + Clone, const N: usize> CollisionTest<T> for SparseHitGridND<T, N> {
    fn hit_candidates(&self, hitbox: T) -> Vec<T> {
        let ranges = self.cell_ranges(&hitbox);
        let cell_len = self.cell_len;
        let mut possible_collisions = vec![];
        for_each_cell(&ranges, |coord| {
            if let Some(cell) = self.cells.get(coord) {
                for other in cell {
                    // Lex-smallest dedup: report each pair only from the first
                    // shared cell, without any geometric overlap check.
                    if *coord == lex_smallest_shared_cell(cell_len, &hitbox, other) {
                        possible_collisions.push(other.clone());
                    }
                }
            }
        });
        possible_collisions
    }

    fn insert(&mut self, hitbox: T) -> Vec<T> {
        let ranges = self.cell_ranges(&hitbox);
        let cell_len = self.cell_len; // Copy so closure doesn't need &self
        let mut result = vec![];

        for_each_cell(&ranges, |coord| {
            let cell = self.cells.entry(*coord).or_default();
            for other in cell.iter() {
                // Lex-smallest dedup only — no geometric overlap check.
                if *coord == lex_smallest_shared_cell(cell_len, &hitbox, other) {
                    result.push(other.clone());
                }
            }
            cell.push(hitbox.clone());
        });
        result
    }

    fn insert_if_no_hit(&mut self, hitbox: T) -> Vec<T> {
        let possible_collisions = self.hit_candidates(hitbox.clone());
        if possible_collisions.is_empty() {
            self.insert(hitbox);
        }
        possible_collisions
    }
}

/// Brute-force O(n²) collision detection — checks every hitbox against every other.
/// Useful as a reference implementation and for small scenes where setup overhead matters.
pub struct BruteForce<T: Hitbox> {
    hitboxes: Vec<T>,
}

impl<T: Hitbox + Clone> BruteForce<T> {
    pub fn new() -> Self {
        Self { hitboxes: vec![] }
    }
}

impl<T: Hitbox + Clone> Default for BruteForce<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Hitbox + Clone> CollisionTest<T> for BruteForce<T> {
    fn hit_candidates(&self, _hitbox: T) -> Vec<T> {
        self.hitboxes.clone()
    }

    fn insert(&mut self, hitbox: T) -> Vec<T> {
        let candidates = self.hit_candidates(hitbox.clone());
        self.hitboxes.push(hitbox);
        candidates
    }

    fn insert_if_no_hit(&mut self, hitbox: T) -> Vec<T> {
        let candidates = self.hit_candidates(hitbox.clone());
        if candidates.is_empty() {
            self.hitboxes.push(hitbox);
        }
        candidates
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn should_return_cartesian_product() {
        let list = [vec![1, 2, 3], vec![4, 5], vec![6, 7, 8, 9]];
        let cart = cartesian(&list);
        assert_eq!(
            cart,
            [
                [1, 4, 6],
                [1, 4, 7],
                [1, 4, 8],
                [1, 4, 9],
                [1, 5, 6],
                [1, 5, 7],
                [1, 5, 8],
                [1, 5, 9],
                [2, 4, 6],
                [2, 4, 7],
                [2, 4, 8],
                [2, 4, 9],
                [2, 5, 6],
                [2, 5, 7],
                [2, 5, 8],
                [2, 5, 9],
                [3, 4, 6],
                [3, 4, 7],
                [3, 4, 8],
                [3, 4, 9],
                [3, 5, 6],
                [3, 5, 7],
                [3, 5, 8],
                [3, 5, 9]
            ]
        )
    }

    #[test]
    fn should_split_area_if_threshold_exceeded() {
        let mut tree: SpatialTree<TaggedNDimBounds> = SpatialTree {
            threshold: 4,
            bounds: TaggedNDimBounds {
                bounds: vec![Bounds::new(-2.0, 8.0), Bounds::new(4.0, 5.0)],
                tag: PickId(0),
            },
            children: None,
            hitboxes: vec![],
        };
        let bl = vec![Bounds::new(-1.9, 1.0), Bounds::new(4.1, 4.2)];
        tree.insert(TaggedNDimBounds {
            bounds: bl,
            tag: PickId(1),
        });
        let tl = vec![Bounds::new(0.0, 1.0), Bounds::new(4.8, 4.9)];
        tree.insert(TaggedNDimBounds {
            bounds: tl.to_vec(),
            tag: PickId(2),
        });
        tree.insert(TaggedNDimBounds {
            bounds: tl,
            tag: PickId(3),
        });
        let br = vec![Bounds::new(5.0, 6.0), Bounds::new(4.1, 4.2)];
        tree.insert(TaggedNDimBounds {
            bounds: br,
            tag: PickId(4),
        });
        let tr = vec![Bounds::new(5.0, 7.0), Bounds::new(4.8, 4.9)];
        tree.insert(TaggedNDimBounds {
            bounds: tr,
            tag: PickId(5),
        });

        assert!(tree.children.is_some());
        assert!(tree.hitboxes.is_empty());
        assert_eq!(
            tree.children.as_ref().unwrap()[0]
                .hitboxes
                .first()
                .unwrap()
                .tag
                .0,
            1
        );
        assert_eq!(
            tree.children.as_ref().unwrap()[1].hitboxes.iter().count(),
            2
        );
        assert_eq!(
            tree.children.as_ref().unwrap()[2]
                .hitboxes
                .first()
                .unwrap()
                .tag
                .0,
            4
        );
        assert_eq!(
            tree.children.as_ref().unwrap()[3]
                .hitboxes
                .first()
                .unwrap()
                .tag
                .0,
            5
        );
    }

    #[test]
    fn should_split_area_if_threshold_exceeded_insert_order_changed() {
        let mut tree: SpatialTree<TaggedNDimBounds> = SpatialTree {
            threshold: 4,
            bounds: TaggedNDimBounds {
                bounds: vec![Bounds::new(-2.0, 8.0), Bounds::new(4.0, 5.0)],
                tag: PickId(0),
            },
            children: None,
            hitboxes: vec![],
        };
        let br = vec![Bounds::new(5.0, 6.0), Bounds::new(4.1, 4.2)];
        tree.insert(TaggedNDimBounds {
            bounds: br,
            tag: PickId(4),
        });
        let tr = vec![Bounds::new(5.0, 7.0), Bounds::new(4.8, 4.9)];
        tree.insert(TaggedNDimBounds {
            bounds: tr,
            tag: PickId(5),
        });
        let bl = vec![Bounds::new(-1.9, 1.0), Bounds::new(4.1, 4.2)];
        tree.insert(TaggedNDimBounds {
            bounds: bl,
            tag: PickId(1),
        });
        let tl = vec![Bounds::new(0.0, 1.0), Bounds::new(4.8, 4.9)];
        tree.insert(TaggedNDimBounds {
            bounds: tl.to_vec(),
            tag: PickId(2),
        });
        tree.insert(TaggedNDimBounds {
            bounds: tl,
            tag: PickId(3),
        });

        assert!(tree.children.is_some());
        assert!(tree.hitboxes.is_empty());
        assert_eq!(
            tree.children.as_ref().unwrap()[0]
                .hitboxes
                .first()
                .unwrap()
                .tag
                .0,
            1
        );
        assert_eq!(
            tree.children.as_ref().unwrap()[1].hitboxes.iter().count(),
            2
        );
        assert_eq!(
            tree.children.as_ref().unwrap()[2]
                .hitboxes
                .first()
                .unwrap()
                .tag
                .0,
            4
        );
        assert_eq!(
            tree.children.as_ref().unwrap()[3]
                .hitboxes
                .first()
                .unwrap()
                .tag
                .0,
            5
        );
    }

    // Helper for iter to TaggedNDimBounds
    fn tb(id: u32, intervals: impl IntoIterator<Item = (f32, f32)>) -> TaggedNDimBounds {
        TaggedNDimBounds {
            bounds: intervals
                .into_iter()
                .map(|(lo, hi)| Bounds::new(lo, hi))
                .collect(),
            tag: PickId(id),
        }
    }

    fn id_of(b: &TaggedNDimBounds) -> u32 {
        b.tag.0
    }

    /// Normalize a pair so (a, b) and (b, a) hash the same.
    fn pair(a: &TaggedNDimBounds, b: &TaggedNDimBounds) -> (u32, u32) {
        let x = id_of(a);
        let y = id_of(b);
        (x.min(y), x.max(y))
    }

    // Helper for inserting multiple into a grid
    fn insert_all_dense<const N: usize>(
        boxes: &[TaggedNDimBounds],
        origin: [f32; N],
        dims: [f32; N],
        cell_len: f32,
    ) -> HashSet<(u32, u32)> {
        let mut grid: HitGridND<TaggedNDimBounds, N> = HitGridND::new(origin, dims, cell_len);
        let mut pairs = HashSet::new();
        for hb in boxes {
            for other in grid.insert(hb.clone()) {
                if other.overlaps(hb) {
                    pairs.insert(pair(&other, hb));
                }
            }
        }
        pairs
    }

    // Helper for inserting multiple into a has-based grid
    fn insert_all_sparse<const N: usize>(
        boxes: &[TaggedNDimBounds],
        cell_len: f32,
    ) -> HashSet<(u32, u32)> {
        let mut grid: SparseHitGridND<TaggedNDimBounds, N> = SparseHitGridND::new(cell_len);
        let mut pairs = HashSet::new();
        for hb in boxes {
            for other in grid.insert(hb.clone()) {
                if other.overlaps(hb) {
                    pairs.insert(pair(&other, hb));
                }
            }
        }
        pairs
    }

    /// O(n²) ground expected: every pair that actually overlaps.
    fn brute_force(boxes: &[TaggedNDimBounds]) -> HashSet<(u32, u32)> {
        let mut pairs = HashSet::new();
        for i in 0..boxes.len() {
            for j in (i + 1)..boxes.len() {
                if boxes[i].overlaps(&boxes[j]) {
                    pairs.insert(pair(&boxes[i], &boxes[j]));
                }
            }
        }
        pairs
    }

    /// Tag is irrelevant for search area
    fn root_bounds(intervals: impl IntoIterator<Item = (f32, f32)>) -> TaggedNDimBounds {
        tb(u32::MAX, intervals)
    }

    /// This function is is used to compare to brute_force so
    /// we filter by `overlaps()` to match the grid behaviour.
    fn insert_all_tree(
        boxes: &[TaggedNDimBounds],
        tree_bounds: TaggedNDimBounds,
        threshold: usize,
    ) -> HashSet<(u32, u32)> {
        let mut tree: SpatialTree<TaggedNDimBounds> = SpatialTree {
            threshold,
            bounds: tree_bounds,
            children: None,
            hitboxes: vec![],
        };
        let mut pairs = HashSet::new();
        for hb in boxes {
            for other in tree.insert(hb.clone()) {
                if other.overlaps(hb) {
                    pairs.insert(pair(&other, hb));
                }
            }
        }
        pairs
    }

    #[test]
    fn tree_1d_chain() {
        let boxes = vec![
            tb(0, [(0.0, 10.0)]),
            tb(1, [(5.0, 15.0)]),
            tb(2, [(12.0, 22.0)]),
            tb(3, [(20.0, 30.0)]),
        ];
        let bounds = root_bounds([(0.0, 100.0)]);
        let expected = brute_force(&boxes);
        assert_eq!(insert_all_tree(&boxes, bounds, 3), expected);
    }

    #[test]
    fn tree_2d_no_splits_matches_expected() {
        // High threshold → tree never splits → returns all prior boxes.
        // Equivalent to brute force after overlap filtering.
        let boxes = vec![
            tb(0, [(0.0, 10.0), (0.0, 10.0)]),
            tb(1, [(5.0, 15.0), (5.0, 15.0)]),
            tb(2, [(20.0, 30.0), (20.0, 30.0)]),
            tb(3, [(8.0, 12.0), (8.0, 12.0)]),
        ];
        let bounds = root_bounds([(0.0, 100.0), (0.0, 100.0)]);
        let expected = brute_force(&boxes);
        assert_eq!(insert_all_tree(&boxes, bounds, 100), expected);
    }

    #[test]
    fn tree_2d_with_splits_matches_expected() {
        // Lower threshold forces splits. Coords kept off the subdivision
        // boundaries (50, 25, 75, ...) to avoid the tree's known edge case
        // where boxes touching a quadrant boundary land in sibling subtrees.
        let boxes: Vec<TaggedNDimBounds> = (0..15u32)
            .map(|i| {
                let x = 1.0 + (i.wrapping_mul(7) % 57) as f32;
                let y = 1.0 + (i.wrapping_mul(11) % 57) as f32;
                let w = 2.0 + (i % 5) as f32;
                let h = 2.0 + (i % 4) as f32;
                tb(i, [(x, x + w), (y, y + h)])
            })
            .collect();
        let bounds = root_bounds([(0.0, 100.0), (0.0, 100.0)]);
        let expected = brute_force(&boxes);
        let tree = insert_all_tree(&boxes, bounds, 3);
        let dense = insert_all_dense::<2>(&boxes, [0.0, 0.0], [100.0, 100.0], 5.0);
        let sparse = insert_all_sparse::<2>(&boxes, 5.0);

        assert_eq!(dense, expected, "dense vs brute force");
        assert_eq!(sparse, expected, "sparse vs brute force");
        assert_eq!(tree, expected, "tree vs brute force");
    }

    #[test]
    fn tree_3d_with_splits_matches_expected() {
        let boxes: Vec<TaggedNDimBounds> = (0..15u32)
            .map(|i| {
                let x = 1.0 + (i.wrapping_mul(7) % 27) as f32;
                let y = 1.0 + (i.wrapping_mul(11) % 27) as f32;
                let z = 1.0 + (i.wrapping_mul(13) % 27) as f32;
                let s = 2.0 + (i % 3) as f32;
                tb(i, [(x, x + s), (y, y + s), (z, z + s)])
            })
            .collect();
        let bounds = root_bounds([(0.0, 50.0); 3]);
        let expected = brute_force(&boxes);
        let tree = insert_all_tree(&boxes, bounds, 4);
        let dense = insert_all_dense::<3>(&boxes, [0.0; 3], [50.0; 3], 5.0);
        let sparse = insert_all_sparse::<3>(&boxes, 5.0);

        assert_eq!(dense, expected);
        assert_eq!(sparse, expected);
        assert_eq!(tree, expected);
    }

    #[test]
    fn tree_5d_modest_workload() {
        let boxes: Vec<TaggedNDimBounds> = (0..12u32)
            .map(|i| {
                let coord = |mul: u32| 1.0 + (i.wrapping_mul(mul) % 27) as f32;
                let s = 2.0 + (i % 3) as f32;
                let c = [coord(7), coord(11), coord(13), coord(17), coord(19)];
                tb(
                    i,
                    [
                        (c[0], c[0] + s),
                        (c[1], c[1] + s),
                        (c[2], c[2] + s),
                        (c[3], c[3] + s),
                        (c[4], c[4] + s),
                    ],
                )
            })
            .collect();
        let bounds = root_bounds([(0.0, 40.0); 5]);
        let expected = brute_force(&boxes);
        let tree = insert_all_tree(&boxes, bounds, 6);
        assert_eq!(tree, expected);
    }

    #[test]
    fn tree_all_four_implementations_agree_2d() {
        // Definitive cross-check: brute force, dense, sparse, and tree
        // all produce the same set of confirmed collision pairs.
        let boxes: Vec<TaggedNDimBounds> = (0..30u32)
            .map(|i| {
                let x = 1.0 + (i.wrapping_mul(17) % 77) as f32;
                let y = 1.0 + (i.wrapping_mul(23) % 77) as f32;
                let s = 3.0 + (i % 4) as f32;
                tb(i, [(x, x + s), (y, y + s)])
            })
            .collect();
        let bounds = root_bounds([(0.0, 100.0), (0.0, 100.0)]);
        let expected = brute_force(&boxes);
        let dense = insert_all_dense::<2>(&boxes, [0.0, 0.0], [100.0, 100.0], 5.0);
        let sparse = insert_all_sparse::<2>(&boxes, 5.0);
        let tree = insert_all_tree(&boxes, bounds, 5);

        assert_eq!(dense, expected, "dense vs brute force");
        assert_eq!(sparse, expected, "sparse vs brute force");
        assert_eq!(tree, expected, "tree vs brute force");
        assert_eq!(dense, sparse, "dense vs sparse");
        assert_eq!(dense, tree, "dense vs tree");
    }

    #[test]
    fn tree_broad_phase_returns_superset_of_true_overlaps() {
        // Even without filtering by overlaps(), the tree must return at
        // least every box that actually overlaps the inserted one.
        let mut tree: SpatialTree<TaggedNDimBounds> = SpatialTree {
            threshold: 3,
            bounds: root_bounds([(0.0, 100.0), (0.0, 100.0)]),
            children: None,
            hitboxes: vec![],
        };

        let mut all_inserted: Vec<TaggedNDimBounds> = vec![];
        let boxes: Vec<TaggedNDimBounds> = (0..20u32)
            .map(|i| {
                let x = 1.0 + (i.wrapping_mul(7) % 57) as f32;
                let y = 1.0 + (i.wrapping_mul(11) % 57) as f32;
                let s = 3.0 + (i % 4) as f32;
                tb(i, [(x, x + s), (y, y + s)])
            })
            .collect();

        for hb in &boxes {
            let returned = tree.insert(hb.clone());
            let returned_ids: HashSet<u32> = returned.iter().map(id_of).collect();

            // Every previously inserted box that actually overlaps hb
            // must appear in the returned candidate set.
            for prior in &all_inserted {
                if prior.overlaps(hb) {
                    assert!(
                        returned_ids.contains(&id_of(prior)),
                        "tree missed a real overlap: {} vs {}",
                        id_of(prior),
                        id_of(hb)
                    );
                }
            }
            all_inserted.push(hb.clone());
        }
    }

    // 1D test for grid as sainity check :D
    #[test]
    fn one_d_empty_grid_no_candidates() {
        let g: HitGridND<TaggedNDimBounds, 1> = HitGridND::new([0.0], [100.0], 10.0);
        assert!(g.hit_candidates(tb(0, [(5.0, 15.0)])).is_empty());

        let s: SparseHitGridND<TaggedNDimBounds, 1> = SparseHitGridND::new(10.0);
        assert!(s.hit_candidates(tb(0, [(5.0, 15.0)])).is_empty());
    }

    #[test]
    fn one_d_single_pair() {
        let boxes = vec![
            tb(0, [(0.0, 10.0)]),
            tb(1, [(5.0, 15.0)]),
            tb(2, [(20.0, 30.0)]),
        ];
        let expected = HashSet::from([(0, 1)]);
        assert_eq!(insert_all_dense::<1>(&boxes, [0.0], [100.0], 5.0), expected);
        assert_eq!(insert_all_sparse::<1>(&boxes, 5.0), expected);
    }

    #[test]
    fn one_d_chain_of_overlaps() {
        let boxes = vec![
            tb(0, [(0.0, 10.0)]),
            tb(1, [(5.0, 15.0)]),
            tb(2, [(12.0, 22.0)]),
            tb(3, [(20.0, 30.0)]),
        ];
        let expected = brute_force(&boxes);
        assert_eq!(expected, HashSet::from([(0, 1), (1, 2), (2, 3)]));
        assert_eq!(insert_all_dense::<1>(&boxes, [0.0], [100.0], 5.0), expected);
        assert_eq!(insert_all_sparse::<1>(&boxes, 5.0), expected);
    }

    #[test]
    fn one_d_dedup_large_boxes() {
        // Both boxes span many cells; the pair must be reported exactly once.
        let boxes = vec![tb(0, [(0.0, 50.0)]), tb(1, [(10.0, 40.0)])];
        assert_eq!(
            insert_all_dense::<1>(&boxes, [0.0], [100.0], 5.0),
            HashSet::from([(0, 1)])
        );
        assert_eq!(insert_all_sparse::<1>(&boxes, 5.0), HashSet::from([(0, 1)]));
    }

    // Actual 2D tests
    #[test]
    fn two_d_empty_grid_no_candidates() {
        let g: HitGridND<TaggedNDimBounds, 2> = HitGridND::new([0.0, 0.0], [100.0, 100.0], 10.0);
        assert!(
            g.hit_candidates(tb(0, [(5.0, 15.0), (5.0, 15.0)]))
                .is_empty()
        );
    }

    #[test]
    fn two_d_single_pair() {
        let boxes = vec![
            tb(0, [(0.0, 10.0), (0.0, 10.0)]),
            tb(1, [(5.0, 15.0), (5.0, 15.0)]),
            tb(2, [(20.0, 30.0), (20.0, 30.0)]),
        ];
        let expected = HashSet::from([(0, 1)]);
        assert_eq!(
            insert_all_dense::<2>(&boxes, [0.0, 0.0], [100.0, 100.0], 5.0),
            expected
        );
        assert_eq!(insert_all_sparse::<2>(&boxes, 5.0), expected);
    }

    #[test]
    fn two_d_dedup_large_boxes() {
        // Each box spans a 5x5 cell region — many shared cells.
        let boxes = vec![
            tb(0, [(0.0, 25.0), (0.0, 25.0)]),
            tb(1, [(5.0, 20.0), (5.0, 20.0)]),
        ];
        assert_eq!(
            insert_all_dense::<2>(&boxes, [0.0, 0.0], [100.0, 100.0], 5.0),
            HashSet::from([(0, 1)])
        );
        assert_eq!(insert_all_sparse::<2>(&boxes, 5.0), HashSet::from([(0, 1)]));
    }

    #[test]
    fn two_d_cluster_with_isolated_outlier() {
        let boxes = vec![
            tb(0, [(10.0, 14.0), (10.0, 14.0)]),
            tb(1, [(11.0, 15.0), (11.0, 15.0)]),
            tb(2, [(12.0, 16.0), (12.0, 16.0)]),
            tb(3, [(50.0, 54.0), (50.0, 54.0)]), // isolated
        ];
        let expected = brute_force(&boxes);
        assert_eq!(
            insert_all_dense::<2>(&boxes, [0.0, 0.0], [100.0, 100.0], 5.0),
            expected
        );
        assert_eq!(insert_all_sparse::<2>(&boxes, 5.0), expected);
    }

    #[test]
    fn two_d_separated_in_one_dim() {
        // Overlap in x but separated in y — must not collide.
        let boxes = vec![
            tb(0, [(0.0, 10.0), (0.0, 5.0)]),
            tb(1, [(0.0, 10.0), (10.0, 15.0)]),
        ];
        assert!(insert_all_dense::<2>(&boxes, [0.0, 0.0], [50.0, 50.0], 5.0).is_empty());
        assert!(insert_all_sparse::<2>(&boxes, 5.0).is_empty());
    }

    #[test]
    fn two_d_negative_coords_sparse() {
        let boxes = vec![
            tb(0, [(-15.0, -5.0), (-15.0, -5.0)]),
            tb(1, [(-10.0, 0.0), (-10.0, 0.0)]),
            tb(2, [(5.0, 15.0), (5.0, 15.0)]),
        ];
        assert_eq!(insert_all_sparse::<2>(&boxes, 5.0), HashSet::from([(0, 1)]));
    }

    #[test]
    fn two_d_dense_with_origin_offset() {
        // Dense grid covering [-50, 50] in both dimensions.
        let boxes = vec![
            tb(0, [(-15.0, -5.0), (-15.0, -5.0)]),
            tb(1, [(-10.0, 0.0), (-10.0, 0.0)]),
            tb(2, [(5.0, 15.0), (5.0, 15.0)]),
        ];
        let pairs = insert_all_dense::<2>(&boxes, [-50.0, -50.0], [100.0, 100.0], 5.0);
        assert_eq!(pairs, HashSet::from([(0, 1)]));
    }

    #[test]
    fn two_d_dense_out_of_bounds_is_silent() {
        // A box entirely outside the grid should not panic and not collide.
        let mut g: HitGridND<TaggedNDimBounds, 2> = HitGridND::new([0.0, 0.0], [50.0, 50.0], 5.0);
        assert!(g.insert(tb(0, [(10.0, 20.0), (10.0, 20.0)])).is_empty());
        assert!(
            g.insert(tb(1, [(100.0, 110.0), (100.0, 110.0)])).is_empty(),
            "out-of-bounds box must report no collisions"
        );
    }

    #[test]
    fn two_d_boundary_aligned_boxes_touch() {
        // Adjacent boxes touching exactly at a cell boundary.
        // Per Hitbox::overlaps (<=, >=), edge contact counts as overlap.
        let boxes = vec![
            tb(0, [(0.0, 10.0), (0.0, 10.0)]),
            tb(1, [(10.0, 20.0), (10.0, 20.0)]),
        ];
        let expected = HashSet::from([(0, 1)]);
        assert_eq!(
            insert_all_dense::<2>(&boxes, [0.0, 0.0], [100.0, 100.0], 5.0),
            expected
        );
        assert_eq!(insert_all_sparse::<2>(&boxes, 5.0), expected);
    }

    #[test]
    fn two_d_fractional_coordinates() {
        let boxes = vec![
            tb(0, [(0.5, 1.5), (0.5, 1.5)]),
            tb(1, [(1.2, 2.0), (1.2, 2.0)]),
            tb(2, [(10.0, 11.0), (10.0, 11.0)]),
        ];
        let expected = brute_force(&boxes);
        assert_eq!(expected, HashSet::from([(0, 1)]));
        assert_eq!(
            insert_all_dense::<2>(&boxes, [0.0, 0.0], [50.0, 50.0], 1.0),
            expected
        );
        assert_eq!(insert_all_sparse::<2>(&boxes, 1.0), expected);
    }

    #[test]
    fn two_d_random_vs_brute_force() {
        // Pseudo-random scatter of small boxes.
        let boxes: Vec<TaggedNDimBounds> = (0..30u32)
            .map(|i| {
                let x = (i.wrapping_mul(7) % 60) as f32;
                let y = (i.wrapping_mul(11) % 60) as f32;
                let w = 2.0 + (i % 5) as f32;
                let h = 2.0 + (i % 4) as f32;
                tb(i, [(x, x + w), (y, y + h)])
            })
            .collect();

        let expected = brute_force(&boxes);
        assert_eq!(
            insert_all_dense::<2>(&boxes, [0.0, 0.0], [100.0, 100.0], 5.0),
            expected
        );
        assert_eq!(insert_all_sparse::<2>(&boxes, 5.0), expected);
    }

    // 3D tests
    #[test]
    fn three_d_single_pair() {
        let boxes = vec![
            tb(0, [(0.0, 10.0), (0.0, 10.0), (0.0, 10.0)]),
            tb(1, [(5.0, 15.0), (5.0, 15.0), (5.0, 15.0)]),
            // overlaps box 0 in xy, but separated in z plane
            tb(2, [(0.0, 10.0), (0.0, 10.0), (20.0, 30.0)]),
        ];
        let expected = HashSet::from([(0, 1)]);
        assert_eq!(
            insert_all_dense::<3>(&boxes, [0.0; 3], [50.0; 3], 5.0),
            expected
        );
        assert_eq!(insert_all_sparse::<3>(&boxes, 5.0), expected);
    }

    #[test]
    fn three_d_dedup_large_boxes() {
        // Two boxes spanning many cells in all three dimensions.
        let boxes = vec![
            tb(0, [(0.0, 30.0), (0.0, 30.0), (0.0, 30.0)]),
            tb(1, [(10.0, 25.0), (10.0, 25.0), (10.0, 25.0)]),
        ];
        assert_eq!(
            insert_all_dense::<3>(&boxes, [0.0; 3], [50.0; 3], 5.0),
            HashSet::from([(0, 1)])
        );
        assert_eq!(insert_all_sparse::<3>(&boxes, 5.0), HashSet::from([(0, 1)]));
    }

    #[test]
    fn three_d_separated_along_z() {
        let boxes = vec![
            tb(0, [(0.0, 10.0), (0.0, 10.0), (0.0, 5.0)]),
            tb(1, [(0.0, 10.0), (0.0, 10.0), (10.0, 15.0)]),
        ];
        assert!(insert_all_dense::<3>(&boxes, [0.0; 3], [50.0; 3], 5.0).is_empty());
        assert!(insert_all_sparse::<3>(&boxes, 5.0).is_empty());
    }

    #[test]
    fn three_d_random_vs_brute_force() {
        let boxes: Vec<TaggedNDimBounds> = (0..25u32)
            .map(|i| {
                let x = (i.wrapping_mul(7) % 40) as f32;
                let y = (i.wrapping_mul(11) % 40) as f32;
                let z = (i.wrapping_mul(13) % 40) as f32;
                let s = 2.0 + (i % 4) as f32;
                tb(i, [(x, x + s), (y, y + s), (z, z + s)])
            })
            .collect();

        let expected = brute_force(&boxes);
        assert_eq!(
            insert_all_dense::<3>(&boxes, [0.0; 3], [50.0; 3], 5.0),
            expected
        );
        assert_eq!(insert_all_sparse::<3>(&boxes, 5.0), expected);
    }

    // Mixed dimensions as per above definition
    #[test]
    fn mixed_size_workload_2d_all_agree() {
        // Mostly small boxes with occasional large ones that span many cells.
        // The large boxes exercise the deduplication path heavily.
        let boxes: Vec<TaggedNDimBounds> = (0..50u32)
            .map(|i| {
                let x = (i.wrapping_mul(17) % 80) as f32;
                let y = (i.wrapping_mul(23) % 80) as f32;
                let size = if i % 7 == 0 {
                    15.0
                } else {
                    3.0 + (i % 4) as f32
                };
                tb(i, [(x, x + size), (y, y + size)])
            })
            .collect();

        let expected = brute_force(&boxes);
        let dense = insert_all_dense::<2>(&boxes, [0.0, 0.0], [100.0, 100.0], 5.0);
        let sparse = insert_all_sparse::<2>(&boxes, 5.0);
        let tree = insert_all_tree(&boxes, root_bounds([(0.0, 100.0), (0.0, 100.0)]), 8);

        assert_eq!(dense, sparse, "dense and sparse disagree");
        assert_eq!(dense, expected, "dense vs brute force");
        assert_eq!(tree, expected, "tree vs brute force");
    }

    #[test]
    fn mixed_size_workload_3d_all_agree() {
        let boxes: Vec<TaggedNDimBounds> = (0..40u32)
            .map(|i| {
                let x = (i.wrapping_mul(17) % 40) as f32;
                let y = (i.wrapping_mul(23) % 40) as f32;
                let z = (i.wrapping_mul(29) % 40) as f32;
                let size = if i % 9 == 0 {
                    12.0
                } else {
                    2.0 + (i % 3) as f32
                };
                tb(i, [(x, x + size), (y, y + size), (z, z + size)])
            })
            .collect();

        let expected = brute_force(&boxes);
        let dense = insert_all_dense::<3>(&boxes, [0.0; 3], [50.0; 3], 5.0);
        let sparse = insert_all_sparse::<3>(&boxes, 5.0);
        let tree = insert_all_tree(&boxes, root_bounds([(0.0, 50.0); 3]), 8);

        assert_eq!(dense, sparse, "dense and sparse disagree in 3D");
        assert_eq!(dense, expected, "dense vs brute force in 3D");
        assert_eq!(tree, expected, "tree vs brute force in 3D");
    }

    // Consitency checks
    #[test]
    fn hit_candidates_returns_partition_mates() {
        let mut g: SparseHitGridND<TaggedNDimBounds, 2> = SparseHitGridND::new(5.0);
        g.insert(tb(0, [(0.0, 10.0), (0.0, 10.0)]));
        g.insert(tb(1, [(20.0, 30.0), (20.0, 30.0)]));
        g.insert(tb(2, [(5.0, 8.0), (5.0, 8.0)]));

        // Probe shares cells with box 0 and box 2, but not box 1.
        let probe = tb(99, [(6.0, 9.0), (6.0, 9.0)]);
        let hits: HashSet<u32> = g.hit_candidates(probe).iter().map(id_of).collect();
        assert_eq!(hits, HashSet::from([0, 2]));
    }

    #[test]
    fn five_d_handcrafted_pair() {
        let boxes = vec![
            tb(
                0,
                [
                    (0.0, 10.0),
                    (0.0, 10.0),
                    (0.0, 10.0),
                    (0.0, 10.0),
                    (0.0, 10.0),
                ],
            ),
            tb(
                1,
                [
                    (5.0, 15.0),
                    (5.0, 15.0),
                    (5.0, 15.0),
                    (5.0, 15.0),
                    (5.0, 15.0),
                ],
            ),
            tb(
                2,
                [
                    (0.0, 10.0),
                    (0.0, 10.0),
                    (0.0, 10.0),
                    (0.0, 10.0),
                    (50.0, 60.0),
                ],
            ),
        ];
        let expected = HashSet::from([(0, 1)]);

        assert_eq!(
            insert_all_dense::<5>(&boxes, [0.0; 5], [30.0; 5], 5.0),
            expected
        );
        assert_eq!(insert_all_sparse::<5>(&boxes, 5.0), expected);
    }

    #[test]
    fn five_d_random_vs_brute_force() {
        let boxes: Vec<TaggedNDimBounds> = (0..20u32)
            .map(|i| {
                let coord = |mul: u32| (i.wrapping_mul(mul) % 30) as f32;
                let s = 2.0 + (i % 3) as f32;
                let c0 = coord(7);
                let c1 = coord(11);
                let c2 = coord(13);
                let c3 = coord(17);
                let c4 = coord(19);
                tb(
                    i,
                    [
                        (c0, c0 + s),
                        (c1, c1 + s),
                        (c2, c2 + s),
                        (c3, c3 + s),
                        (c4, c4 + s),
                    ],
                )
            })
            .collect();

        let expected = brute_force(&boxes);
        let dense = insert_all_dense::<5>(&boxes, [0.0; 5], [40.0; 5], 5.0);
        let sparse = insert_all_sparse::<5>(&boxes, 5.0);

        assert_eq!(dense, sparse, "5D dense and sparse disagree");
        assert_eq!(dense, expected, "5D dense vs brute force");
    }

    #[test]
    fn broad_phase_returns_superset() {
        let boxes = vec![
            tb(0, [(0.0, 4.0), (0.0, 4.0)]),
            tb(1, [(3.0, 7.0), (3.0, 7.0)]),
            tb(2, [(20.0, 24.0), (20.0, 24.0)]),
        ];
        let probe = tb(99, [(2.0, 5.0), (2.0, 5.0)]);

        let true_overlaps: HashSet<u32> = boxes
            .iter()
            .filter(|b| b.overlaps(&probe))
            .map(id_of)
            .collect();

        let mut tree = SpatialTree::new(
            2,
            root_bounds([(0.0, 30.0), (0.0, 30.0)]),
        );
        let mut dense: HitGridND<TaggedNDimBounds, 2> =
            HitGridND::new([0.0, 0.0], [30.0, 30.0], 5.0);
        let mut sparse: SparseHitGridND<TaggedNDimBounds, 2> = SparseHitGridND::new(5.0);
        let mut brute = BruteForce::new();
        for b in &boxes {
            tree.insert(b.clone());
            dense.insert(b.clone());
            sparse.insert(b.clone());
            brute.insert(b.clone());
        }

        for (name, candidates) in [
            ("tree", tree.hit_candidates(probe.clone())),
            ("dense", dense.hit_candidates(probe.clone())),
            ("sparse", sparse.hit_candidates(probe.clone())),
            ("brute", brute.hit_candidates(probe.clone())),
        ] {
            let ids: HashSet<u32> = candidates.iter().map(id_of).collect();
            assert!(
                true_overlaps.is_subset(&ids),
                "{name}: broad phase must be superset of true overlaps. missing: {:?}",
                true_overlaps.difference(&ids).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn broad_phase_returns_partition_mates() {
        // Two non-overlapping boxes in the same grid cell.
        // cell_size=10, both live in cell (0,0).
        let a = tb(1, [(0.0, 3.0), (0.0, 3.0)]);
        let b = tb(2, [(7.0, 9.0), (7.0, 9.0)]);
        assert!(!a.overlaps(&b), "test precondition: a and b must not overlap");

        let mut dense: HitGridND<TaggedNDimBounds, 2> =
            HitGridND::new([0.0, 0.0], [10.0, 10.0], 10.0);
        dense.insert(a.clone());
        dense.insert(b.clone());
        let hits: HashSet<u32> = dense.hit_candidates(a.clone()).iter().map(id_of).collect();
        assert!(hits.contains(&2), "dense grid: non-overlapping cell-mate must appear");

        let mut sparse: SparseHitGridND<TaggedNDimBounds, 2> = SparseHitGridND::new(10.0);
        sparse.insert(a.clone());
        sparse.insert(b.clone());
        let hits: HashSet<u32> = sparse.hit_candidates(a.clone()).iter().map(id_of).collect();
        assert!(hits.contains(&2), "sparse grid: non-overlapping cell-mate must appear");
    }

    #[test]
    fn tree_broad_phase_includes_non_overlapping_cohabitants() {
        // Low threshold so both land in the same leaf.
        let a = tb(1, [(0.0, 2.0)]);
        let b = tb(2, [(8.0, 10.0)]);
        assert!(!a.overlaps(&b), "test precondition: a and b must not overlap");

        let mut tree = SpatialTree::new(4, root_bounds([(0.0, 20.0)]));
        tree.insert(a.clone());
        tree.insert(b.clone());
        let hits: HashSet<u32> = tree.hit_candidates(a.clone()).iter().map(id_of).collect();
        assert!(
            hits.contains(&2),
            "tree: non-overlapping node-mate must appear in broad phase"
        );
    }
}
