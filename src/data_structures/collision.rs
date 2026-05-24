use std::collections::HashMap;

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
    fn new(lower_bound: f32, upper_bound: f32) -> Self {
        Self {
            lower_bound,
            upper_bound,
        }
    }
}
impl Hitbox for Bounds {
    fn submerges(&self, other: &Self) -> bool {
        self.lower_bound <= other.lower_bound && self.upper_bound >= other.upper_bound
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
        match &self.children {
            Some(sub_trees) => {
                for bisection in sub_trees {
                    if bisection.bounds.submerges(&hitbox) {
                        return [bisection.hit_candidates(hitbox), self.hitboxes.to_vec()].concat();
                    }
                }
                return self.hitboxes.to_vec();
            }
            None => return self.hitboxes.to_vec(),
        }
    }

    fn insert(&mut self, hitbox: T) -> Vec<T> {
        match &mut self.children {
            Some(sub_trees) => {
                for bisection in sub_trees {
                    if bisection.bounds.submerges(&hitbox) {
                        return [bisection.insert(hitbox), self.hitboxes.to_vec()].concat();
                    }
                }
                // if new hitbox cannot be submerged by any child area it will be stored in
                // the parent node to avoid infinite recursion for multiple same-size hitboxes.
                // Eventually multiple same-size hitboxes will hit a boundary and stack
                // anyway. This is just deferring it.
                let possible_collisions = self.hitboxes.to_vec();
                self.hitboxes.push(hitbox);
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
                    let mut possible_collisions = self.hitboxes.to_vec();
                    for bisection in &mut sub_trees {
                        if bisection.bounds.submerges(&hitbox) {
                            possible_collisions.append(bisection.hitboxes.clone().as_mut());
                            bisection.hitboxes.push(hitbox);
                            break;
                        }
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

pub struct HitGrid2D<T> {
    origin: (f32, f32),
    grid_dims: (f32, f32),
    cell_len: f32,
    cols: usize,
    rows: usize,
    cells: Vec<Vec<T>>,
}

impl<T: Hitbox + Clone> HitGrid2D<T> {
    pub fn new(origin: (f32, f32), grid_dims: (f32, f32), cell_len: f32) -> Self {
        let cols = (grid_dims.0 / cell_len).ceil() as usize;
        let rows = (grid_dims.1 / cell_len).ceil() as usize;
        Self {
            origin,
            grid_dims,
            cell_len,
            cols,
            rows,
            cells: vec![vec![]; cols * rows],
        }
    }

    /// Returns the inclusive cell range covering a hitbox, clamped to the grid.
    /// Returns None if the hitbox lies entirely outside the grid.
    pub fn cell_range(&self, hitbox: &T) -> Option<(i32, i32, i32, i32)> {
        let h = self.cell_len;
        let (x_start, x_end) = hitbox.interval(0);
        let (y_start, y_end) = hitbox.interval(1);

        let x_start = ((x_start - self.origin.0) / h).floor() as i32;
        let x_end = ((x_end - self.origin.0) / h).ceil() as i32;
        let y_start = ((y_start - self.origin.1) / h).floor() as i32;
        let y_end = ((y_end - self.origin.1) / h).ceil() as i32;

        // Clamp to grid bounds.
        let cs = x_start.max(0);
        let ce = x_end.min(self.cols as i32 - 1);
        let rs = y_start.max(0);
        let re = y_end.min(self.rows as i32 - 1);

        if cs > ce || rs > re {
            None
        } else {
            Some((cs, ce, rs, re))
        }
    }
}
impl<T: Hitbox + Clone> CollisionTest<T> for HitGrid2D<T> {
    fn hit_candidates(&self, hitbox: T) -> Vec<T> {
        let h = self.cell_len;
        let cols = (self.grid_dims.0 / h).ceil() as usize;

        let (x_start, x_end) = hitbox.interval(0);
        let (y_start, y_end) = hitbox.interval(1);

        let x_start = (x_start / h).floor() as usize;
        let x_end = (x_end / h).ceil() as usize;
        let y_start = (y_start / h).floor() as usize;
        let y_end = (y_end / h).ceil() as usize;

        let mut possible_collisions = vec![];
        for cx in x_start..=x_end {
            for cy in y_start..=y_end {
                let idx = cy * cols + cx;
                if let Some(cell) = self.cells.get(idx) {
                    for other in cell {
                        if hitbox.overlaps(other) {
                            possible_collisions.push(other.clone());
                        }
                    }
                }
            }
        }
        possible_collisions
    }

    fn insert(&mut self, hitbox: T) -> Vec<T> {
        let Some((x_start, x_end, y_start, y_end)) = self.cell_range(&hitbox) else {
            return vec![]; // entirely outside the grid
        };

        let h = self.cell_len;
        let mut result = vec![];

        for x in x_start..=x_end {
            for y in y_start..=y_end {
                let idx = (y as usize) * self.cols + (x as usize);
                let cell = &mut self.cells[idx];
                for other in cell.iter() {
                    if !hitbox.overlaps(other) {
                        continue;
                    }
                    let lexs = (0..2).map(|i| {
                        let (o_x_start, _) = other.interval(i);
                        let (h_x_start, _) = hitbox.interval(i);
                        ((o_x_start - self.origin.0) / h)
                            .floor()
                            .max(((h_x_start - self.origin.0) / h).floor())
                            as i32
                    });
                    let unique_overlap = lexs
                        .zip([x, y])
                        .fold(true, |overlap, (lex_i, i)| overlap && lex_i == i);
                    if unique_overlap {
                        result.push(other.clone());
                    }
                }
                cell.push(hitbox.clone());
            }
        }
        result
    }

    fn insert_if_no_hit(&mut self, hitbox: T) -> Vec<T> {
        todo!()
    }
}

pub struct SparseHitGridND<T> {
    cell_len: f32,
    dims: usize,
    cells: HashMap<Vec<i32>, Vec<T>>,
}

impl<T: Hitbox + Clone> SparseHitGridND<T> {
    /// Returns all cell coordinates covered by a hitbox via Cartesian product
    /// of per-dimension cell ranges.
    fn covering_cells(&self, hitbox: &T) -> Vec<Vec<i32>> {
        let h = self.cell_len;
        let per_dim_ranges: Vec<Vec<i32>> = (0..self.dims)
            .map(|d| {
                let (lower_bound, upper_bound) = hitbox.interval(d);
                let start = (lower_bound / h).floor() as i32;
                let end = (upper_bound / h).ceil() as i32;
                (start..=end).collect()
            })
            .collect();
        cartesian(&per_dim_ranges)
    }
}

fn lex_smallest_shared_cell<T: Hitbox>(dims: usize, cell_len: f32, a: &T, b: &T) -> Vec<i32> {
    (0..dims)
        .map(|d| {
            let (a_lower_bound, _) = a.interval(d);
            let (b_lower_bound, _) = b.interval(d);
            ((a_lower_bound / cell_len).floor() as i32).max((b_lower_bound / cell_len).floor() as i32)
        })
        .collect()
}

impl<T: Hitbox + Clone> CollisionTest<T> for SparseHitGridND<T> {
    fn hit_candidates(&self, hitbox: T) -> Vec<T> {
        let mut possible_collisions = vec![];
        for cell_coord in self.covering_cells(&hitbox) {
            if let Some(cell) = self.cells.get(&cell_coord) {
                for other in cell {
                    if hitbox.overlaps(other) {
                        possible_collisions.push(other.clone());
                    }
                }
            }
        }
        possible_collisions
    }

    fn insert(&mut self, hitbox: T) -> Vec<T> {
        let mut result = vec![];
        for cell_coord in self.covering_cells(&hitbox) {
            let cell = self.cells.entry(cell_coord.clone()).or_default();
            for other in cell.iter() {
                if !hitbox.overlaps(other) {
                    continue;
                }
                if cell_coord == lex_smallest_shared_cell(self.dims, self.cell_len, &hitbox, other)
                {
                    result.push(other.clone());
                }
            }
            cell.push(hitbox.clone());
        }
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

#[cfg(test)]
mod tests {
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
        let bl = vec![Bounds::new(-2.0, 1.0), Bounds::new(4.0, 4.2)];
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
        let br = vec![Bounds::new(5.0, 6.0), Bounds::new(4.0, 4.2)];
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
        let br = vec![Bounds::new(5.0, 6.0), Bounds::new(4.0, 4.2)];
        tree.insert(TaggedNDimBounds {
            bounds: br,
            tag: PickId(4),
        });
        let tr = vec![Bounds::new(5.0, 7.0), Bounds::new(4.8, 4.9)];
        tree.insert(TaggedNDimBounds {
            bounds: tr,
            tag: PickId(5),
        });
        let bl = vec![Bounds::new(-2.0, 1.0), Bounds::new(4.0, 4.2)];
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
}
