use crate::pick::PickId;

pub trait Hitbox {
    fn submerges(&self, other: &Self) -> bool;
    fn split(&self) -> Vec<Self>
    where
        Self: Sized;
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

impl<'a> CollisionTest<TaggedNDimBounds> for SpatialTree<TaggedNDimBounds> {
    fn hit_candidates(&self, hitbox: TaggedNDimBounds) -> Vec<TaggedNDimBounds> {
        todo!()
    }

    fn insert(&mut self, hitbox: TaggedNDimBounds) -> Vec<TaggedNDimBounds> {
        match &mut self.children {
            Some(quads) => {
                for bisection in quads {
                    if bisection.bounds.submerges(&hitbox) {
                        return [bisection.insert(hitbox), self.hitboxes.to_vec()].concat();
                    }
                }
                // if new hitbox cannot be submerged by any child area it will be stored in a
                // the parent node to avoid infinite recursion for multiple same-size hitboxes.
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
                    let mut sub_trees: Vec<SpatialTree<TaggedNDimBounds>> = sub_bounds
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

    fn insert_if_no_hit(&mut self, hitbox: TaggedNDimBounds) -> Vec<TaggedNDimBounds> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Bound;

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
