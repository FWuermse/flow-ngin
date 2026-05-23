use crate::pick::PickId;

pub struct CornerPoint {
    top_left: cgmath::Point2<f32>,
    axis_lens: Vec<f32>,
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

    fn submerges(&self, other: &Self) -> bool {
        self.lower_bound <= other.lower_bound && self.upper_bound >= other.upper_bound
    }

    fn half(&self) -> (Self, Self) {
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
        return (left, right);
    }
}

pub trait CollisionTest<T> {
    fn hits(&self, hitbox: T) -> Vec<T>;
    fn insert(&mut self, hitbox: T) -> Vec<T>;
    fn insert_if_no_hit(&mut self, hitbox: T) -> Vec<T>;
}

pub struct QuadTree<T> {
    threshold: usize,
    bounds: Vec<Bounds>,
    children: Option<[Box<QuadTree<T>>; 4]>,
    hitboxes: Vec<T>,
}
impl<T> QuadTree<T> {
    pub fn submerges(&self, other: &[Bounds]) -> bool {
        self.bounds
            .iter()
            .zip(other)
            .fold(true, |s, (a, b)| a.submerges(&b) && s)
    }
}

type B<'a> = (&'a [Bounds], PickId);

impl<'a> CollisionTest<B<'a>> for QuadTree<B<'a>> {
    fn hits(&self, hitbox: B<'a>) -> Vec<B<'a>> {
        todo!()
    }

    fn insert(&mut self, hitbox: B<'a>) -> Vec<B<'a>> {
        match &mut self.children {
            Some(quads) => {
                let [left_up, right_up, left_low, right_low] = quads;
                if left_up.submerges(&hitbox.0) {
                    return [left_up.insert(hitbox), self.hitboxes.to_vec()].concat();
                }
                if right_up.submerges(&hitbox.0) {
                    return [right_up.insert(hitbox), self.hitboxes.to_vec()].concat();
                }
                if left_low.submerges(&hitbox.0) {
                    return [left_low.insert(hitbox), self.hitboxes.to_vec()].concat();
                }
                if right_low.submerges(&hitbox.0) {
                    return [right_low.insert(hitbox), self.hitboxes.to_vec()].concat();
                }
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
                    let (x_left_bounds, x_right_bounds) = self.bounds[0].half();
                    let (y_low_bounds, y_up_bounds) = self.bounds[1].half();
                    let mut left_up = Vec::new();
                    let mut right_up = Vec::new();
                    let mut left_low = Vec::new();
                    let mut right_low = Vec::new();
                    let mut intersecting = Vec::new();
                    let hitboxes = std::mem::take(&mut self.hitboxes);
                    for hb in hitboxes {
                        let (x, y) = (&hb.0[0], &hb.0[1]);
                        if x_left_bounds.submerges(x) {
                            if y_up_bounds.submerges(y) {
                                left_up.push(hb);
                                continue;
                            }
                            if y_low_bounds.submerges(y) {
                                left_low.push(hb);
                                continue;
                            }
                        }
                        if x_right_bounds.submerges(x) {
                            if y_up_bounds.submerges(y) {
                                right_up.push(hb);
                                continue;
                            }
                            if y_low_bounds.submerges(y) {
                                right_low.push(hb);
                                continue;
                            }
                        }
                        intersecting.push(hb);
                    }
                    self.hitboxes.append(&mut intersecting);
                    self.children = Some([
                        Box::new(QuadTree {
                            threshold: self.threshold,
                            bounds: vec![x_left_bounds.clone(), y_up_bounds.clone()],
                            children: None,
                            hitboxes: left_up,
                        }),
                        Box::new(QuadTree {
                            threshold: self.threshold,
                            bounds: vec![x_right_bounds.clone(), y_up_bounds.clone()],
                            children: None,
                            hitboxes: right_up,
                        }),
                        Box::new(QuadTree {
                            threshold: self.threshold,
                            bounds: vec![x_left_bounds.clone(), y_low_bounds.clone()],
                            children: None,
                            hitboxes: left_low,
                        }),
                        Box::new(QuadTree {
                            threshold: self.threshold,
                            bounds: vec![x_right_bounds.clone(), y_low_bounds.clone()],
                            children: None,
                            hitboxes: right_low,
                        }),
                    ]);
                    let [left_up, right_up, left_low, right_low] = self.children.as_mut().unwrap();
                    if left_up.submerges(&hitbox.0) {
                        return [left_up.insert(hitbox), self.hitboxes.to_vec()].concat();
                    }
                    if right_up.submerges(&hitbox.0) {
                        return [right_up.insert(hitbox), self.hitboxes.to_vec()].concat();
                    }
                    if left_low.submerges(&hitbox.0) {
                        return [left_low.insert(hitbox), self.hitboxes.to_vec()].concat();
                    }
                    if right_low.submerges(&hitbox.0) {
                        return [right_low.insert(hitbox), self.hitboxes.to_vec()].concat();
                    }
                    let possible_collisions = self.hitboxes.to_vec();
                    self.hitboxes.push(hitbox);
                    return possible_collisions;
                }
            }
        }
    }

    fn insert_if_no_hit(&mut self, hitbox: B<'a>) -> Vec<B<'a>> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Bound;

    use super::*;

    #[test]
    fn should_split_area_if_threshold_exceeded() {
        let mut tree: QuadTree<B> = QuadTree {
            threshold: 4,
            bounds: vec![Bounds::new(-2.0, 8.0), Bounds::new(4.0, 5.0)],
            children: None,
            hitboxes: vec![],
        };
        let tl = [Bounds::new(0.0, 1.0), Bounds::new(4.8, 4.9)];
        tree.insert((&tl, PickId(0)));
        let tr = [Bounds::new(5.0, 7.0), Bounds::new(4.8, 4.9)];
        tree.insert((&tr, PickId(1)));
        tree.insert((&tr, PickId(2)));
        let bl = [Bounds::new(-2.0, 1.0), Bounds::new(4.0, 4.2)];
        tree.insert((&bl, PickId(3)));
        let br = [Bounds::new(5.0, 6.0), Bounds::new(4.0, 4.2)];
        tree.insert((&br, PickId(4)));

        assert!(tree.children.is_some());
        assert!(tree.hitboxes.is_empty());
        assert_eq!(tree.children.as_ref().unwrap()[0].hitboxes.first().unwrap().1.0, 0);
        assert_eq!(tree.children.as_ref().unwrap()[1].hitboxes.iter().count(), 2);
        assert_eq!(tree.children.as_ref().unwrap()[2].hitboxes.first().unwrap().1.0, 3);
        assert_eq!(tree.children.as_ref().unwrap()[3].hitboxes.first().unwrap().1.0, 4);
    }

    #[test]
    fn should_split_area_if_threshold_exceeded_different_order() {
        let mut tree: QuadTree<B> = QuadTree {
            threshold: 4,
            bounds: vec![Bounds::new(-2.0, 8.0), Bounds::new(4.0, 5.0)],
            children: None,
            hitboxes: vec![],
        };
        let br = [Bounds::new(5.0, 6.0), Bounds::new(4.0, 4.2)];
        tree.insert((&br, PickId(4)));
        let tr = [Bounds::new(5.0, 7.0), Bounds::new(4.8, 4.9)];
        tree.insert((&tr, PickId(1)));
        tree.insert((&tr, PickId(2)));
        let bl = [Bounds::new(-2.0, 1.0), Bounds::new(4.0, 4.2)];
        tree.insert((&bl, PickId(3)));
        let tl = [Bounds::new(0.0, 1.0), Bounds::new(4.8, 4.9)];
        tree.insert((&tl, PickId(0)));

        assert!(tree.children.is_some());
        assert!(tree.hitboxes.is_empty());
        assert_eq!(tree.children.as_ref().unwrap()[0].hitboxes.first().unwrap().1.0, 0);
        assert_eq!(tree.children.as_ref().unwrap()[1].hitboxes.iter().count(), 2);
        assert_eq!(tree.children.as_ref().unwrap()[2].hitboxes.first().unwrap().1.0, 3);
        assert_eq!(tree.children.as_ref().unwrap()[3].hitboxes.first().unwrap().1.0, 4);
    }
}
