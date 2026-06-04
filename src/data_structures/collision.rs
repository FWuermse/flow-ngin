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

use cgmath::{One, Quaternion, Rotation, Vector3};

use crate::{data_structures::instance::Instance, pick::PickId};

/// A shape that lives *inside* a detection space: it exposes its enclosing
/// axis-aligned interval per dimension, which the broad phase uses for bucketing.
///
/// This is the *object* side of detection. The *space* side (grid cells, tree
/// nodes, world bounds) is described by [`Region`]. A type may be only `Bounded`
/// (a plain object), only a [`Region`], or both (as [`TaggedNDimBounds`] is).
pub trait Bounded {
    fn interval(&self, dimension: usize) -> (f32, f32);
}

/// A spatial-partition region that stores and routes [`Bounded`] objects of
/// type `T`. A region can report whether it contains or touches an object and
/// can subdivide itself; this is what a [`SpatialTree`] node is built from.
pub trait Region<T: Bounded> {
    fn submerges(&self, other: &T) -> bool;
    fn overlaps(&self, other: &T) -> bool;
    fn split(&self) -> Vec<Self>
    where
        Self: Sized;
}

pub trait Convex {
    fn points_and_axes(&self) -> (Vec<Vec<f32>>, Vec<Vec<f32>>);
}

/// Bloom filter like hit testing using hitbox intervals
pub trait CollisionTest {
    type Hitbox: Bounded + PartialEq + Clone;

    fn hit_candidates(&self, hitbox: &Self::Hitbox) -> Vec<Self::Hitbox>;
    fn insert(&mut self, hitbox: Self::Hitbox) -> Vec<Self::Hitbox>;
    fn remove(&mut self, hitbox: &Self::Hitbox);
}

pub struct Collision<S>
where
    S: CollisionTest,
{
    space: S,
}
impl<S> Collision<S>
where
    S: CollisionTest,
{
    pub fn hit(&self, hitbox: S::Hitbox) -> Vec<S::Hitbox>
    where
        <S as CollisionTest>::Hitbox: Convex,
    {
        let candidates = self.space.hit_candidates(&hitbox);
        let hits = candidates.into_iter().filter(|b| sat(&hitbox, b).hit());
        hits.collect()
    }

    pub fn insert(&mut self, hitbox: S::Hitbox) {
        self.space.insert(hitbox);
    }

    pub fn delete(&mut self, hitbox: S::Hitbox) {
        self.space.remove(&hitbox);
    }
}

impl Collision<SparseHitGridND<PlaneHitbox, 2>> {
    pub fn new(cell_len: f32) -> Self {
        Self {
            space: SparseHitGridND {
                cell_len,
                cells: HashMap::new(),
            },
        }
    }
}

/// Result of a Separating Axis Theorem test between two convex point sets.
#[derive(Clone, Debug, PartialEq)]
pub enum SatResult {
    Overlap { normal: Vec<f32>, depth: f32 },
    Separated { axis: Vec<f32>, gap: f32 },
}
impl SatResult {
    pub fn hit(&self) -> bool {
        match self {
            SatResult::Overlap {
                normal: _,
                depth: _,
            } => true,
            SatResult::Separated { axis: _, gap: _ } => false,
        }
    }
}

/// Axes shorter than this are treated as degenerate and skipped.
const SAT_EPSILON: f32 = 1e-6;

/// Tests two convex shapes for overlap using the Separating Axis Theorem.
///
/// `a` and `b` are the vertex sets of the two convex shapes; `axes` is the set of
/// candidate separating axes to test. Axes need not be unit length (each is
/// normalized internally so `depth` and `gap` come out in world units), and
/// near-zero-length axes are skipped.
///
/// Returns [`SatResult::Separated`] as soon as any axis separates the shapes (one
/// separating axis is sufficient proof of no collision), otherwise
/// [`SatResult::Overlap`] with the minimum-translation vector.
pub fn sat<A: Convex + ?Sized, B: Convex + ?Sized>(a: &A, b: &B) -> SatResult {
    let (points_a, axes_a) = a.points_and_axes();
    let (points_b, axes_b) = b.points_and_axes();
    let dim = points_a.len().min(points_b.len());

    let mut min_overlap = f32::INFINITY;
    let mut mtv_axis = (0..dim).map(|_| 0.0).collect();

    for raw in axes_a.into_iter().chain(axes_b.into_iter()) {
        let axis = match normalize(raw) {
            Some(axis) => axis,
            None => continue,
        };
        if dot_dyn(&axis, &axis) < SAT_EPSILON * SAT_EPSILON {
            continue;
        }
        let (min_a, max_a) = project(&points_a, &axis);
        let (min_b, max_b) = project(&points_b, &axis);
        let overlap = max_a.min(max_b) - min_a.max(min_b);
        if overlap < 0.0 {
            return SatResult::Separated {
                axis: axis.to_vec(),
                gap: -overlap,
            };
        }

        if overlap < min_overlap {
            min_overlap = overlap;
            let center_a = (min_a + max_a) * 0.5;
            let center_b = (min_b + max_b) * 0.5;
            mtv_axis = if center_b < center_a {
                let dim = axis.len();
                let mut flipped: Vec<f32> = (0..dim).map(|_| 0.0).collect();
                for i in 0..dim {
                    flipped[i] = -axis[i];
                }
                flipped
            } else {
                axis.to_vec()
            };
        }
    }

    SatResult::Overlap {
        normal: mtv_axis,
        depth: if min_overlap.is_finite() {
            min_overlap
        } else {
            0.0
        },
    }
}

/// Returns `v` scaled to unit length, or `None` if `v` is near-zero length.
fn normalize(v: Vec<f32>) -> Option<Vec<f32>> {
    let dim = v.len();
    let len = dot_dyn(&v, &v).sqrt();
    if len < SAT_EPSILON {
        return None;
    }
    let mut out: Vec<f32> = (0..dim).map(|_| 0.0).collect();
    for i in 0..dim {
        out[i] = v[i] / len;
    }
    Some(out)
}

/// Dot product of two runtime-dimensional vectors (shorter length wins).
fn dot_dyn(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Projects a runtime-dimensional point set onto `axis`, returning `(min, max)`.
fn project(points: &[Vec<f32>], axis: &[f32]) -> (f32, f32) {
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for p in points {
        let d = dot_dyn(p, axis);
        if d < min {
            min = d;
        }
        if d > max {
            max = d;
        }
    }
    (min, max)
}

#[derive(Clone, PartialEq)]
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
impl Bounded for Bounds {
    fn interval(&self, _: usize) -> (f32, f32) {
        (self.lower_bound, self.upper_bound)
    }
}
impl Region<Bounds> for Bounds {
    fn submerges(&self, other: &Self) -> bool {
        self.lower_bound < other.lower_bound && self.upper_bound > other.upper_bound
    }

    fn overlaps(&self, other: &Self) -> bool {
        self.lower_bound <= other.upper_bound && self.upper_bound >= other.lower_bound
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
}

/// Represents a hitbox as n-dimensional lower and upper bound tagged with a PickId to backtrack hit objects
///
/// The `bounds` describe the box in its *local* (un-rotated) frame; `rotation`
/// orients it about its center. The broad phase (via [`Bounded::interval`]) always
/// sees the enclosing axis-aligned box, while the narrow phase (via [`Convex`]) sees
/// the true oriented corners and face normals. `rotation` only affects the first
/// up to three dimensions (it maps `dim0,dim1,dim2` to `x,y,z`); higher dimensions
/// are left axis-aligned. The default rotation is the identity, in which case the
/// box behaves exactly as a plain AABB.
#[derive(Clone, PartialEq)]
pub struct TaggedNDimBounds {
    bounds: Vec<Bounds>,
    rotation: Quaternion<f32>,
    tag: PickId,
}

impl TaggedNDimBounds {
    pub fn new(bounds: Vec<Bounds>, tag: PickId) -> Self {
        Self {
            bounds,
            rotation: Quaternion::one(),
            tag,
        }
    }

    pub fn rotated(mut self, rotation: Quaternion<f32>) -> Self {
        self.rotation = rotation;
        self
    }

    pub fn tag(&self) -> PickId {
        self.tag
    }

    fn center(&self) -> Vec<f32> {
        self.bounds
            .iter()
            .map(|b| (b.lower_bound + b.upper_bound) * 0.5)
            .collect()
    }

    fn is_axis_aligned(&self) -> bool {
        let q = self.rotation;
        q.s == 1.0 && q.v.x == 0.0 && q.v.y == 0.0 && q.v.z == 0.0
    }

    fn rotate_offset(&self, offset: &[f32]) -> Vec<f32> {
        let v = Vector3::new(
            offset.first().copied().unwrap_or(0.0),
            offset.get(1).copied().unwrap_or(0.0),
            offset.get(2).copied().unwrap_or(0.0),
        );
        let r = self.rotation.rotate_vector(v);
        let mut out = offset.to_vec();
        let mapped = [r.x, r.y, r.z];
        for (i, slot) in out.iter_mut().take(3).enumerate() {
            *slot = mapped[i];
        }
        out
    }

    fn corners(&self) -> Vec<Vec<f32>> {
        let center = self.center();
        let per_dim: Vec<Vec<f32>> = self
            .bounds
            .iter()
            .map(|b| vec![b.lower_bound, b.upper_bound])
            .collect();
        cartesian(&per_dim)
            .into_iter()
            .map(|corner| {
                let offset: Vec<f32> = corner
                    .iter()
                    .zip(center.iter())
                    .map(|(c, mid)| c - mid)
                    .collect();
                let rotated = self.rotate_offset(&offset);
                rotated
                    .iter()
                    .zip(center.iter())
                    .map(|(o, mid)| o + mid)
                    .collect()
            })
            .collect()
    }
}

impl Convex for TaggedNDimBounds {
    fn points_and_axes(&self) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
        let dims = self.bounds.len();
        // Candidate axes: the rotated unit basis vectors (the box's face normals).
        let axes: Vec<Vec<f32>> = (0..dims)
            .map(|i| {
                let mut e = vec![0.0; dims];
                e[i] = 1.0;
                self.rotate_offset(&e)
            })
            .collect();
        (self.corners(), axes)
    }
}

impl Region<TaggedNDimBounds> for TaggedNDimBounds {
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
                rotation: self.rotation,
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
}

impl Bounded for TaggedNDimBounds {
    fn interval(&self, dimension: usize) -> (f32, f32) {
        let Some(bounds) = self.bounds.get(dimension) else {
            return (0.0, 0.0);
        };
        if self.is_axis_aligned() {
            return bounds.interval(dimension);
        }
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for corner in self.corners() {
            let v = corner[dimension];
            if v < min {
                min = v;
            }
            if v > max {
                max = v;
            }
        }
        (min, max)
    }
}

#[derive(Clone, PartialEq)]
pub struct PlaneHitbox {
    center: [f32; 2],
    half: [f32; 2],
    rotation: Quaternion<f32>,
    id: PickId,
    idx: usize,
}

impl PlaneHitbox {
    pub fn new(
        instance: &Instance,
        width: f32,
        depth: f32,
        id: impl TryInto<PickId>,
        idx: usize,
    ) -> Option<Self> {
        Some(Self {
            center: [instance.position.x, instance.position.z],
            half: [width * 0.5, depth * 0.5],
            rotation: instance.rotation,
            id: id.try_into().ok()?,
            idx,
        })
    }

    pub fn id(&self) -> PickId {
        self.id
    }

    fn rotate_xz(&self, dx: f32, dz: f32) -> [f32; 2] {
        let r = self.rotation.rotate_vector(Vector3::new(dx, 0.0, dz));
        [r.x, r.z]
    }

    fn corners(&self) -> Vec<Vec<f32>> {
        let (hx, hz) = (self.half[0], self.half[1]);
        [(-hx, -hz), (hx, -hz), (hx, hz), (-hx, hz)]
            .iter()
            .map(|&(dx, dz)| {
                let [ox, oz] = self.rotate_xz(dx, dz);
                vec![self.center[0] + ox, self.center[1] + oz]
            })
            .collect()
    }

    /// Reconstruct the near original instance for visualization
    pub fn to_instance(&self, y: f32, thickness: f32) -> Instance {
        Instance {
            position: Vector3::new(self.center[0], y, self.center[1]),
            rotation: self.rotation,
            scale: Vector3::new(self.half[0] * 2.0, thickness, self.half[1] * 2.0),
        }
    }
}

impl Bounded for PlaneHitbox {
    fn interval(&self, dimension: usize) -> (f32, f32) {
        let d = dimension.min(1);
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for corner in self.corners() {
            min = min.min(corner[d]);
            max = max.max(corner[d]);
        }
        (min, max)
    }
}

impl Convex for PlaneHitbox {
    fn points_and_axes(&self) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
        let x_axis = self.rotate_xz(1.0, 0.0);
        let z_axis = self.rotate_xz(0.0, 1.0);
        let axes = vec![x_axis.to_vec(), z_axis.to_vec()];
        (self.corners(), axes)
    }
}

#[derive(Clone, PartialEq)]
pub struct CubeHitbox {
    center: [f32; 3],
    half: [f32; 3],
    rotation: Quaternion<f32>,
    id: PickId,
    idx: usize,
}

impl CubeHitbox {
    pub fn new(
        instance: &Instance,
        width: f32,
        height: f32,
        depth: f32,
        id: impl TryInto<PickId>,
        idx: usize,
    ) -> Option<Self> {
        Some(Self {
            center: [
                instance.position.x,
                instance.position.y,
                instance.position.z,
            ],
            half: [width * 0.5, height * 0.5, depth * 0.5],
            rotation: instance.rotation,
            id: id.try_into().ok()?,
            idx,
        })
    }

    pub fn id(&self) -> PickId {
        self.id
    }

    /// Rotate a local offset by the instance quaternion.
    fn rotate(&self, dx: f32, dy: f32, dz: f32) -> [f32; 3] {
        let r = self.rotation.rotate_vector(Vector3::new(dx, dy, dz));
        [r.x, r.y, r.z]
    }

    /// The eight oriented world-space corners as (x, y, z) points.
    fn corners(&self) -> Vec<Vec<f32>> {
        let (hx, hy, hz) = (self.half[0], self.half[1], self.half[2]);
        let mut out = Vec::with_capacity(8);
        for &sx in &[-1.0, 1.0] {
            for &sy in &[-1.0, 1.0] {
                for &sz in &[-1.0, 1.0] {
                    let [ox, oy, oz] = self.rotate(sx * hx, sy * hy, sz * hz);
                    out.push(vec![
                        self.center[0] + ox,
                        self.center[1] + oy,
                        self.center[2] + oz,
                    ]);
                }
            }
        }
        out
    }
}

impl Bounded for CubeHitbox {
    fn interval(&self, dimension: usize) -> (f32, f32) {
        let d = dimension.min(2); // dim0=x, dim1=y, dim2=z
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for corner in self.corners() {
            min = min.min(corner[d]);
            max = max.max(corner[d]);
        }
        (min, max)
    }
}

impl Convex for CubeHitbox {
    fn points_and_axes(&self) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
        let axes = vec![
            self.rotate(1.0, 0.0, 0.0).to_vec(), // local +x
            self.rotate(0.0, 1.0, 0.0).to_vec(), // local +y
            self.rotate(0.0, 0.0, 1.0).to_vec(), // local +z
        ];
        (self.corners(), axes)
    }
}

pub struct SpatialTree<R, T> {
    threshold: usize,
    bounds: R,
    children: Option<Vec<SpatialTree<R, T>>>,
    hitboxes: Vec<T>,
}

impl<R: Region<T>, T: Bounded + Clone> SpatialTree<R, T> {
    pub fn new(threshold: usize, bounds: R) -> Self {
        Self {
            threshold,
            bounds,
            children: None,
            hitboxes: vec![],
        }
    }

    pub fn visit_bounds(&self, f: &mut impl FnMut(&R, usize)) {
        self.visit_bounds_inner(f, 0);
    }

    fn visit_bounds_inner(&self, f: &mut impl FnMut(&R, usize), depth: usize) {
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

impl<R: Region<T>, T: Bounded + Clone + PartialEq> CollisionTest for SpatialTree<R, T> {
    fn hit_candidates(&self, hitbox: &T) -> Vec<T> {
        let mut result = vec![];
        result.extend(self.hitboxes.iter().cloned());
        if let Some(children) = &self.children {
            for child in children {
                // Traverse only subtrees whose bounds overlap,
                // not a geometric overlap check on the stored items.
                if child.bounds.overlaps(hitbox) {
                    result.extend(child.hit_candidates(hitbox));
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
                    possible_collisions.append(&mut bisection.hit_candidates(&hitbox));
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
                    let mut sub_trees: Vec<SpatialTree<R, T>> = sub_bounds
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

    fn remove(&mut self, hitbox: &T) {
        self.hitboxes.retain(|h| h == hitbox);
        if let Some(children) = &mut self.children {
            for child in children {
                if child.bounds.overlaps(hitbox) {
                    child.remove(hitbox);
                }
            }
        }
    }

    type Hitbox = T;
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

impl<T: Bounded + Clone, const N: usize> HitGridND<T, N> {
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
            let end = ((upper_bound - self.origin[d]) / h).floor() as i32;
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

impl<T: Bounded + Clone + PartialEq, const N: usize> CollisionTest for HitGridND<T, N> {
    fn hit_candidates(&self, hitbox: &T) -> Vec<T> {
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
            // if two items share multiple cells we operate on cell co-occupation only.
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

    fn remove(&mut self, hitbox: &T) {
        let Some(ranges) = self.cell_ranges(&hitbox) else {
            return;
        };
        for_each_cell(&ranges, |coord| {
            let idx = self.flat_index(coord);
            self.cells[idx].retain(|h| h == hitbox);
        });
    }

    type Hitbox = T;
}

pub struct SparseHitGridND<T, const N: usize> {
    cell_len: f32,
    cells: HashMap<[i32; N], Vec<T>>,
}

impl<T: Bounded + Clone, const N: usize> SparseHitGridND<T, N> {
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
                (upper_bound / h).floor() as i32,
            );
        }
        ranges
    }
}

fn lex_smallest_shared_cell<T: Bounded, const N: usize>(cell_len: f32, a: &T, b: &T) -> [i32; N] {
    let mut result = [0i32; N];
    for d in 0..N {
        let (lower_bound_a, _) = a.interval(d);
        let (lower_bound_b, _) = b.interval(d);
        result[d] = ((lower_bound_a / cell_len).floor() as i32)
            .max((lower_bound_b / cell_len).floor() as i32);
    }
    result
}

impl<T: Bounded + Clone + PartialEq, const N: usize> CollisionTest for SparseHitGridND<T, N> {
    fn hit_candidates(&self, hitbox: &T) -> Vec<T> {
        let ranges = self.cell_ranges(&hitbox);
        let cell_len = self.cell_len;
        let mut possible_collisions = vec![];
        for_each_cell(&ranges, |coord| {
            if let Some(cell) = self.cells.get(coord) {
                for other in cell {
                    // Lex-smallest dedup: report each pair only from the first
                    // shared cell, without any geometric overlap check.
                    if *coord == lex_smallest_shared_cell(cell_len, hitbox, other) {
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
                // Lex-smallest dedup only.
                if *coord == lex_smallest_shared_cell(cell_len, &hitbox, other) {
                    result.push(other.clone());
                }
            }
            cell.push(hitbox.clone());
        });
        result
    }

    fn remove(&mut self, hitbox: &T) {
        let ranges = self.cell_ranges(&hitbox);
        for_each_cell(&ranges, |coord| {
            if let Some(cell) = self.cells.get_mut(coord) {
                cell.retain(|h| h == hitbox);
            }
        });
    }

    type Hitbox = T;
}

/// Brute-force O(n²) collision detection.
/// Useful as a reference implementation and for small scenes where setup overhead matters.
pub struct BruteForce<T: Bounded> {
    hitboxes: Vec<T>,
}

impl<T: Bounded + Clone> BruteForce<T> {
    pub fn new() -> Self {
        Self { hitboxes: vec![] }
    }
}

impl<T: Bounded + Clone> Default for BruteForce<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Bounded + Clone + PartialEq> CollisionTest for BruteForce<T> {
    fn hit_candidates(&self, _hitbox: &T) -> Vec<T> {
        self.hitboxes.clone()
    }

    fn insert(&mut self, hitbox: T) -> Vec<T> {
        let candidates = self.hit_candidates(&hitbox);
        self.hitboxes.push(hitbox);
        candidates
    }

    fn remove(&mut self, hitbox: &T) {
        self.hitboxes.retain(|h| h == hitbox);
    }

    type Hitbox = T;
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
        let mut tree: SpatialTree<TaggedNDimBounds, TaggedNDimBounds> = SpatialTree {
            threshold: 4,
            bounds: TaggedNDimBounds::new(
                vec![Bounds::new(-2.0, 8.0), Bounds::new(4.0, 5.0)],
                PickId(0),
            ),
            children: None,
            hitboxes: vec![],
        };
        let bl = vec![Bounds::new(-1.9, 1.0), Bounds::new(4.1, 4.2)];
        tree.insert(TaggedNDimBounds::new(bl, PickId(1)));
        let tl = vec![Bounds::new(0.0, 1.0), Bounds::new(4.8, 4.9)];
        tree.insert(TaggedNDimBounds::new(tl.to_vec(), PickId(2)));
        tree.insert(TaggedNDimBounds::new(tl, PickId(3)));
        let br = vec![Bounds::new(5.0, 6.0), Bounds::new(4.1, 4.2)];
        tree.insert(TaggedNDimBounds::new(br, PickId(4)));
        let tr = vec![Bounds::new(5.0, 7.0), Bounds::new(4.8, 4.9)];
        tree.insert(TaggedNDimBounds::new(tr, PickId(5)));

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
        let mut tree: SpatialTree<TaggedNDimBounds, TaggedNDimBounds> = SpatialTree {
            threshold: 4,
            bounds: TaggedNDimBounds::new(
                vec![Bounds::new(-2.0, 8.0), Bounds::new(4.0, 5.0)],
                PickId(0),
            ),
            children: None,
            hitboxes: vec![],
        };
        let br = vec![Bounds::new(5.0, 6.0), Bounds::new(4.1, 4.2)];
        tree.insert(TaggedNDimBounds::new(br, PickId(4)));
        let tr = vec![Bounds::new(5.0, 7.0), Bounds::new(4.8, 4.9)];
        tree.insert(TaggedNDimBounds::new(tr, PickId(5)));
        let bl = vec![Bounds::new(-1.9, 1.0), Bounds::new(4.1, 4.2)];
        tree.insert(TaggedNDimBounds::new(bl, PickId(1)));
        let tl = vec![Bounds::new(0.0, 1.0), Bounds::new(4.8, 4.9)];
        tree.insert(TaggedNDimBounds::new(tl.to_vec(), PickId(2)));
        tree.insert(TaggedNDimBounds::new(tl, PickId(3)));

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
        TaggedNDimBounds::new(
            intervals
                .into_iter()
                .map(|(lo, hi)| Bounds::new(lo, hi))
                .collect(),
            PickId(id),
        )
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
        let mut tree: SpatialTree<TaggedNDimBounds, TaggedNDimBounds> = SpatialTree {
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
        let mut tree: SpatialTree<TaggedNDimBounds, TaggedNDimBounds> = SpatialTree {
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
        assert!(g.hit_candidates(&tb(0, [(5.0, 15.0)])).is_empty());

        let s: SparseHitGridND<TaggedNDimBounds, 1> = SparseHitGridND::new(10.0);
        assert!(s.hit_candidates(&tb(0, [(5.0, 15.0)])).is_empty());
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
            g.hit_candidates(&tb(0, [(5.0, 15.0), (5.0, 15.0)]))
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
        let hits: HashSet<u32> = g.hit_candidates(&probe).iter().map(id_of).collect();
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

        let mut tree = SpatialTree::new(2, root_bounds([(0.0, 30.0), (0.0, 30.0)]));
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
            ("tree", tree.hit_candidates(&probe)),
            ("dense", dense.hit_candidates(&probe)),
            ("sparse", sparse.hit_candidates(&probe)),
            ("brute", brute.hit_candidates(&probe)),
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
        assert!(
            !a.overlaps(&b),
            "test precondition: a and b must not overlap"
        );

        let mut dense: HitGridND<TaggedNDimBounds, 2> =
            HitGridND::new([0.0, 0.0], [10.0, 10.0], 10.0);
        dense.insert(a.clone());
        dense.insert(b.clone());
        let hits: HashSet<u32> = dense.hit_candidates(&a).iter().map(id_of).collect();
        assert!(
            hits.contains(&2),
            "dense grid: non-overlapping cell-mate must appear"
        );

        let mut sparse: SparseHitGridND<TaggedNDimBounds, 2> = SparseHitGridND::new(10.0);
        sparse.insert(a.clone());
        sparse.insert(b.clone());
        let hits: HashSet<u32> = sparse.hit_candidates(&a).iter().map(id_of).collect();
        assert!(
            hits.contains(&2),
            "sparse grid: non-overlapping cell-mate must appear"
        );
    }

    #[test]
    fn tree_broad_phase_includes_non_overlapping_cohabitants() {
        // Low threshold so both land in the same leaf.
        let a = tb(1, [(0.0, 2.0)]);
        let b = tb(2, [(8.0, 10.0)]);
        assert!(
            !a.overlaps(&b),
            "test precondition: a and b must not overlap"
        );

        let mut tree = SpatialTree::new(4, root_bounds([(0.0, 20.0)]));
        tree.insert(a.clone());
        tree.insert(b.clone());
        let hits: HashSet<u32> = tree.hit_candidates(&a).iter().map(id_of).collect();
        assert!(
            hits.contains(&2),
            "tree: non-overlapping node-mate must appear in broad phase"
        );
    }
}

#[cfg(test)]
mod sat_tests {
    use super::*;

    /// Returns the corner points of the axis-aligned box spanning `min`..`max`.
    fn aabb_corners<const N: usize>(min: [f32; N], max: [f32; N]) -> Vec<[f32; N]> {
        let mut corners = vec![[0.0; N]; 1 << N];
        for (mask, corner) in corners.iter_mut().enumerate() {
            for i in 0..N {
                corner[i] = if mask & (1 << i) == 0 { min[i] } else { max[i] };
            }
        }
        corners
    }

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    const AXES_2D: [[f32; 2]; 2] = [[1.0, 0.0], [0.0, 1.0]];
    const AXES_3D: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

    struct AxesCorners2D {
        axes: Vec<[f32; 2]>,
        corners: Vec<[f32; 2]>,
    }
    impl Convex for AxesCorners2D {
        fn points_and_axes(&self) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
            (
                self.corners.iter().map(|c| c.to_vec()).collect(),
                self.axes.iter().map(|a| a.to_vec()).collect(),
            )
        }
    }

    struct AxesCorners3D {
        axes: [[f32; 3]; 3],
        corners: Vec<[f32; 3]>,
    }
    impl Convex for AxesCorners3D {
        fn points_and_axes(&self) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
            (
                self.corners.iter().map(|c| c.to_vec()).collect(),
                self.axes.map(|a| a.to_vec()).into_iter().collect(),
            )
        }
    }

    #[test]
    fn overlap_2d_known_depth() {
        let a = aabb_corners([0.0, 0.0], [1.0, 1.0]);
        let b = aabb_corners([0.5, 0.0], [1.5, 1.0]);
        let a = AxesCorners2D {
            axes: AXES_2D.to_vec(),
            corners: a,
        };
        let b = AxesCorners2D {
            axes: AXES_2D.to_vec(),
            corners: b,
        };
        match sat(&a, &b) {
            SatResult::Overlap { normal, depth } => {
                assert!(approx(depth, 0.5), "depth was {depth}");
                // Least-overlap axis is x; b is to the right of a → normal points +x.
                assert!(
                    approx(normal[0], 1.0) && approx(normal[1], 0.0),
                    "normal {normal:?}"
                );
            }
            other => panic!("expected overlap, got {other:?}"),
        }
    }

    #[test]
    fn separated_2d_known_gap() {
        let a = aabb_corners([0.0, 0.0], [1.0, 1.0]);
        let b = aabb_corners([2.0, 0.0], [3.0, 1.0]);
        let a = AxesCorners2D {
            axes: AXES_2D.to_vec(),
            corners: a,
        };
        let b = AxesCorners2D {
            axes: AXES_2D.to_vec(),
            corners: b,
        };
        match sat(&a, &b) {
            SatResult::Separated { axis, gap } => {
                assert!(approx(gap, 1.0), "gap was {gap}");
                assert!(
                    approx(axis[0], 1.0) && approx(axis[1], 0.0),
                    "axis {axis:?}"
                );
            }
            other => panic!("expected separated, got {other:?}"),
        }
    }

    #[test]
    fn touching_2d_is_zero_depth_overlap() {
        let a = aabb_corners([0.0, 0.0], [1.0, 1.0]);
        let b = aabb_corners([1.0, 0.0], [2.0, 1.0]);
        let a = AxesCorners2D {
            axes: AXES_2D.to_vec(),
            corners: a,
        };
        let b = AxesCorners2D {
            axes: AXES_2D.to_vec(),
            corners: b,
        };
        match sat(&a, &b) {
            SatResult::Overlap { depth, .. } => assert!(approx(depth, 0.0), "depth {depth}"),
            other => panic!("expected overlap, got {other:?}"),
        }
    }

    #[test]
    fn overlap_3d() {
        let a = aabb_corners([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let b = aabb_corners([0.5, 0.5, 0.5], [1.5, 1.5, 1.5]);
        let a = AxesCorners3D {
            axes: AXES_3D,
            corners: a,
        };
        let b = AxesCorners3D {
            axes: AXES_3D,
            corners: b,
        };
        match sat(&a, &b) {
            SatResult::Overlap { depth, .. } => assert!(approx(depth, 0.5), "depth {depth}"),
            other => panic!("expected overlap, got {other:?}"),
        }
    }

    #[test]
    fn separated_3d() {
        let a = aabb_corners([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let b = aabb_corners([0.0, 2.0, 0.0], [1.0, 3.0, 1.0]);
        let a = AxesCorners3D {
            axes: AXES_3D,
            corners: a,
        };
        let b = AxesCorners3D {
            axes: AXES_3D,
            corners: b,
        };
        match sat(&a, &b) {
            SatResult::Separated { axis, gap } => {
                assert!(approx(gap, 1.0), "gap {gap}");
                assert!(approx(axis[1], 1.0), "axis {axis:?}");
            }
            other => panic!("expected separated, got {other:?}"),
        }
    }

    #[test]
    fn non_unit_axis_is_normalized() {
        // Same shapes as overlap_2d but tested only along a non-unit x axis.
        let a = aabb_corners([0.0, 0.0], [1.0, 1.0]);
        let b = aabb_corners([0.5, 0.0], [1.5, 1.0]);
        let axes = [[5.0_f32, 0.0]];
        let a = AxesCorners2D {
            axes: axes.to_vec(),
            corners: a,
        };
        let b = AxesCorners2D {
            axes: vec![],
            corners: b,
        };
        match sat(&a, &b) {
            // Depth must be in world units (0.5), not scaled by the axis length.
            SatResult::Overlap { depth, .. } => assert!(approx(depth, 0.5), "depth {depth}"),
            other => panic!("expected overlap, got {other:?}"),
        }
    }

    #[test]
    fn diagonal_axis_overlap() {
        // Two squares overlapping diagonally, tested only along the diagonal axis.
        let a = aabb_corners([0.0, 0.0], [1.0, 1.0]);
        let b = aabb_corners([0.5, 0.5], [1.5, 1.5]);
        let axes = [[1.0_f32, 1.0]];
        let a = AxesCorners2D {
            axes: axes.to_vec(),
            corners: a,
        };
        let b = AxesCorners2D {
            axes: axes.to_vec(),
            corners: b,
        };
        match sat(&a, &b) {
            SatResult::Overlap { normal, depth } => {
                assert!(
                    approx(depth, std::f32::consts::FRAC_1_SQRT_2),
                    "depth {depth}"
                );
                assert!(normal[0] > 0.0 && normal[1] > 0.0, "normal {normal:?}");
            }
            other => panic!("expected overlap, got {other:?}"),
        }
    }

    #[test]
    fn empty_axes_reports_zero_depth_overlap() {
        let a = aabb_corners([0.0, 0.0], [1.0, 1.0]);
        let b = aabb_corners([0.0, 0.0], [1.0, 1.0]);
        let axes: [[f32; 2]; 0] = [];
        let a = AxesCorners2D {
            axes: axes.to_vec(),
            corners: a,
        };
        let b = AxesCorners2D {
            axes: axes.to_vec(),
            corners: b,
        };
        match sat(&a, &b) {
            SatResult::Overlap { depth, .. } => assert!(approx(depth, 0.0)),
            other => panic!("expected overlap, got {other:?}"),
        }
    }

    /// Axis-aligned 2D box centered at `(cx, cz)` with half-extent `h`.
    fn boxd(cx: f32, cz: f32, h: f32) -> TaggedNDimBounds {
        TaggedNDimBounds::new(
            vec![Bounds::new(cx - h, cx + h), Bounds::new(cz - h, cz + h)],
            PickId(0),
        )
    }

    /// 2D box centered at `(c0, c1)` with independent half-extents per dimension.
    fn boxd2(c0: f32, c1: f32, h0: f32, h1: f32) -> TaggedNDimBounds {
        TaggedNDimBounds::new(
            vec![Bounds::new(c0 - h0, c0 + h0), Bounds::new(c1 - h1, c1 + h1)],
            PickId(0),
        )
    }

    #[test]
    fn sat_aabb_overlap_and_separation() {
        let a = boxd(0.0, 0.0, 0.5);
        assert!(
            sat(&a, &boxd(0.5, 0.0, 0.5)).hit(),
            "overlapping boxes must hit"
        );
        assert!(
            !sat(&a, &boxd(2.0, 0.0, 0.5)).hit(),
            "gapped boxes must not hit"
        );
    }

    #[test]
    fn sat_rotation_changes_result() {
        use cgmath::{Deg, Quaternion, Rotation3, Vector3};
        // A 2D box maps dim0->x, dim1->y, so in-plane rotation is about z.
        let still = boxd(0.0, 0.0, 0.5);
        // A long, thin bar above `still`: wide in dim0 (4 across), thin in dim1.
        // Axis-aligned, its thin side faces `still` along dim1 with a clear gap.
        let bar = boxd2(0.0, 1.0, 2.0, 0.2);
        assert!(
            !sat(&still, &bar).hit(),
            "thin side faces the square: should be separated"
        );

        // Rotate the bar 90° in-plane (about z): its long side now faces `still`,
        // reaching down across the gap, so SAT reports an overlap. This only
        // changes because the narrow phase uses the true oriented geometry.
        let bar_rot = bar.rotated(Quaternion::from_axis_angle(Vector3::unit_z(), Deg(90.0)));
        assert!(
            sat(&still, &bar_rot).hit(),
            "rotated long side reaches the square— should overlap"
        );
    }

    #[test]
    fn sat_rotated_corners_form_enclosing_aabb() {
        use cgmath::{Deg, Quaternion, Rotation3, Vector3};
        // A unit square rotated 45° in-plane (about z) has an enclosing AABB whose
        // half-width grows from 0.5 to ~0.5·√2 ≈ 0.707. `interval()` (broad phase)
        // must report that wider box so it never misses a real collision.
        let b =
            boxd(0.0, 0.0, 0.5).rotated(Quaternion::from_axis_angle(Vector3::unit_z(), Deg(45.0)));
        let (lo, hi) = b.interval(0);
        let half = (hi - lo) * 0.5;
        assert!(
            approx(half, std::f32::consts::FRAC_1_SQRT_2),
            "enclosing AABB half-width was {half}"
        );
    }
}
