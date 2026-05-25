use std::collections::HashSet;

use cgmath::Vector3;
use flow_ngin::data_structures::collision::{
    BruteForce, Bounds, CollisionTest, HitGridND, Hitbox, SparseHitGridND, SpatialTree,
    TaggedNDimBounds,
};
use flow_ngin::pick::PickId;

use crate::PlacedObject;

pub const WORLD_HALF: f32 = 10.0;
pub const CELL_SIZE: f32 = 2.0;
pub const CUBE_HALF: f32 = 0.5;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Strategy {
    Quadtree,
    Octree,
    Grid,
    SparseGrid,
    BruteForce,
}

/// Wraps every collision strategy variant so callers can switch at runtime.
pub enum CollisionBackend {
    Quadtree(SpatialTree<TaggedNDimBounds>),
    Octree(SpatialTree<TaggedNDimBounds>),
    Grid(HitGridND<TaggedNDimBounds, 3>),
    SparseGrid(SparseHitGridND<TaggedNDimBounds, 3>),
    BruteForce(BruteForce<TaggedNDimBounds>),
}

impl CollisionBackend {
    pub fn new(strategy: Strategy) -> Self {
        let w = WORLD_HALF;
        match strategy {
            Strategy::Quadtree => {
                let root = TaggedNDimBounds::new(
                    vec![Bounds::new(-w, w), Bounds::new(-w, w)],
                    PickId::from(0u32),
                );
                CollisionBackend::Quadtree(SpatialTree::new(4, root))
            }
            Strategy::Octree => {
                let root = TaggedNDimBounds::new(
                    vec![
                        Bounds::new(-w, w),
                        Bounds::new(-w, w),
                        Bounds::new(-w, w),
                    ],
                    PickId::from(0u32),
                );
                CollisionBackend::Octree(SpatialTree::new(4, root))
            }
            Strategy::Grid => {
                let origin = [-w, -w, -w];
                let dims = [w * 2.0, w * 2.0, w * 2.0];
                CollisionBackend::Grid(HitGridND::new(origin, dims, CELL_SIZE))
            }
            Strategy::SparseGrid => {
                CollisionBackend::SparseGrid(SparseHitGridND::new(CELL_SIZE))
            }
            Strategy::BruteForce => CollisionBackend::BruteForce(BruteForce::new()),
        }
    }

    pub fn query(
        &mut self,
        placed: &[PlacedObject],
        drag: Vector3<f32>,
    ) -> (HashSet<u32>, HashSet<u32>) {
        for obj in placed {
            let hb = make_hitbox(obj.position, obj.id);
            match self {
                CollisionBackend::Quadtree(t) => { t.insert(hb); }
                CollisionBackend::Octree(t)   => { t.insert(hb); }
                CollisionBackend::Grid(g)     => { g.insert(hb); }
                CollisionBackend::SparseGrid(s) => { s.insert(hb); }
                CollisionBackend::BruteForce(b) => { b.insert(hb); }
            }
        }

        // id = 0 is the drag cube sentinel; never stored as a placed object
        let drag_hb = make_hitbox(drag, PickId::from(0u32));
        let candidates: Vec<TaggedNDimBounds> = match self {
            CollisionBackend::Quadtree(t) => t.hit_candidates(drag_hb.clone()),
            CollisionBackend::Octree(t)   => t.hit_candidates(drag_hb.clone()),
            CollisionBackend::Grid(g)     => g.hit_candidates(drag_hb.clone()),
            CollisionBackend::SparseGrid(s) => s.hit_candidates(drag_hb.clone()),
            CollisionBackend::BruteForce(b) => b.hit_candidates(drag_hb.clone()),
        };

        let broad: HashSet<u32> = candidates.iter().map(|c| c.tag().0).collect();
        let narrow: HashSet<u32> = candidates.iter()
            .filter(|c| c.overlaps(&drag_hb))
            .map(|c| c.tag().0)
            .collect();
        (broad, narrow)
    }

    pub fn partition_lines(&self) -> Vec<[Vector3<f32>; 2]> {
        match self {
            CollisionBackend::Quadtree(tree) => tree_lines_2d(tree),
            CollisionBackend::Octree(tree)   => tree_lines_3d(tree),
            CollisionBackend::Grid(_)        => grid_lines(),
            CollisionBackend::SparseGrid(_)  => grid_lines(),
            CollisionBackend::BruteForce(_)  => vec![],
        }
    }
}

fn make_hitbox(pos: Vector3<f32>, id: PickId) -> TaggedNDimBounds {
    let h = CUBE_HALF;
    TaggedNDimBounds::new(
        vec![
            Bounds::new(pos.x - h, pos.x + h),
            Bounds::new(pos.y - h, pos.y + h),
            Bounds::new(pos.z - h, pos.z + h),
        ],
        id,
    )
}

fn tree_lines_2d(tree: &SpatialTree<TaggedNDimBounds>) -> Vec<[Vector3<f32>; 2]> {
    let mut lines = vec![];
    tree.visit_bounds(&mut |bounds, _depth| {
        let (x0, x1) = bounds.interval(0);
        let (z0, z1) = bounds.interval(1);
        let y = 0.0_f32;
        lines.push([Vector3::new(x0, y, z0), Vector3::new(x1, y, z0)]);
        lines.push([Vector3::new(x0, y, z1), Vector3::new(x1, y, z1)]);
        lines.push([Vector3::new(x0, y, z0), Vector3::new(x0, y, z1)]);
        lines.push([Vector3::new(x1, y, z0), Vector3::new(x1, y, z1)]);
    });
    lines
}

fn tree_lines_3d(tree: &SpatialTree<TaggedNDimBounds>) -> Vec<[Vector3<f32>; 2]> {
    let mut lines = vec![];
    tree.visit_bounds(&mut |bounds, _depth| {
        let (x0, x1) = bounds.interval(0);
        let (y0, y1) = bounds.interval(1);
        let (z0, z1) = bounds.interval(2);
        aabb_lines(x0, x1, y0, y1, z0, z1, &mut lines);
    });
    lines
}

fn grid_lines() -> Vec<[Vector3<f32>; 2]> {
    let w = WORLD_HALF;
    let s = CELL_SIZE;
    let mut lines = vec![];
    let steps = ((w * 2.0) / s).round() as i32 + 1;
    for i in 0..=steps {
        let v = -w + i as f32 * s;
        lines.push([Vector3::new(-w, 0.0, v), Vector3::new(w, 0.0, v)]);
        lines.push([Vector3::new(v, 0.0, -w), Vector3::new(v, 0.0, w)]);
        for j in 0..=steps {
            let u = -w + j as f32 * s;
            lines.push([Vector3::new(v, -w, u), Vector3::new(v, w, u)]);
        }
    }
    for i in 0..=steps {
        let y = -w + i as f32 * s;
        for j in 0..=steps {
            let x = -w + j as f32 * s;
            lines.push([Vector3::new(x, y, -w), Vector3::new(x, y, w)]);
        }
        for j in 0..=steps {
            let z = -w + j as f32 * s;
            lines.push([Vector3::new(-w, y, z), Vector3::new(w, y, z)]);
        }
    }
    lines
}

/// Emit the 12 edges of an axis-aligned bounding box.
fn aabb_lines(
    x0: f32, x1: f32,
    y0: f32, y1: f32,
    z0: f32, z1: f32,
    out: &mut Vec<[Vector3<f32>; 2]>,
) {
    // Bottom face
    out.push([Vector3::new(x0, y0, z0), Vector3::new(x1, y0, z0)]);
    out.push([Vector3::new(x1, y0, z0), Vector3::new(x1, y0, z1)]);
    out.push([Vector3::new(x1, y0, z1), Vector3::new(x0, y0, z1)]);
    out.push([Vector3::new(x0, y0, z1), Vector3::new(x0, y0, z0)]);
    // Top face
    out.push([Vector3::new(x0, y1, z0), Vector3::new(x1, y1, z0)]);
    out.push([Vector3::new(x1, y1, z0), Vector3::new(x1, y1, z1)]);
    out.push([Vector3::new(x1, y1, z1), Vector3::new(x0, y1, z1)]);
    out.push([Vector3::new(x0, y1, z1), Vector3::new(x0, y1, z0)]);
    // Vertical edges
    out.push([Vector3::new(x0, y0, z0), Vector3::new(x0, y1, z0)]);
    out.push([Vector3::new(x1, y0, z0), Vector3::new(x1, y1, z0)]);
    out.push([Vector3::new(x1, y0, z1), Vector3::new(x1, y1, z1)]);
    out.push([Vector3::new(x0, y0, z1), Vector3::new(x0, y1, z1)]);
}
