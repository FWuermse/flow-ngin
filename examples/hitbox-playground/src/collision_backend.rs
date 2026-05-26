use flow_ngin::{
    Vector3,
    data_structures::collision::{
        BruteForce, Bounds, CollisionTest, Hitbox, HitGridND, SparseHitGridND, SpatialTree,
        TaggedNDimBounds,
    },
    pick::PickId,
};

use crate::{ObjectShape, Strategy};

/// Half-size of each hitbox (unit-sized objects).
pub const HALF: f32 = 0.5;

/// World half-extent for the dense grid (covers [-WORLD_HALF, WORLD_HALF] per axis).
pub const WORLD_HALF: f32 = 20.0;
/// Cell size for both grid variants.
pub const CELL_SIZE: f32 = 2.0;

/// Y position at which 2D planes are rendered and at which the 2D grid is drawn.
pub const PLANE_Y: f32 = 0.0;

// ── Hitbox factory ────────────────────────────────────────────────────────────
//
// Dimension convention used everywhere in this example:
//   dim 0 = X  (horizontal)
//   dim 1 = Z  (horizontal depth)
//   dim 2 = Y  (vertical)
//
// This ensures cross-dimensional semantics are intuitive:
//   A 2D grid (N=2) checks [X, Z] and correctly captures ground-plane collisions.
//   A 3D grid (N=3) additionally checks [Y].
//   A 2D hitbox [X,Z] can be tested in a 3D space (it just ignores Y).
//   A 3D hitbox [X,Z,Y] in a 2D space → treated as 2D (cross-dim rule: Y ignored).

pub fn make_hitbox(pos: Vector3<f32>, shape: ObjectShape, id: PickId) -> TaggedNDimBounds {
    let h = HALF;
    match shape {
        ObjectShape::Plane2D => TaggedNDimBounds::new(
            vec![
                Bounds::new(pos.x - h, pos.x + h), // dim 0 = X
                Bounds::new(pos.z - h, pos.z + h), // dim 1 = Z
            ],
            id,
        ),
        ObjectShape::Cube3D => TaggedNDimBounds::new(
            vec![
                Bounds::new(pos.x - h, pos.x + h), // dim 0 = X
                Bounds::new(pos.z - h, pos.z + h), // dim 1 = Z
                Bounds::new(pos.y - h, pos.y + h), // dim 2 = Y
            ],
            id,
        ),
    }
}

// ── CollisionBackend ──────────────────────────────────────────────────────────

/// A type-erased wrapper over all supported collision backends.
///
/// Grid and SparseGrid variants require const `N` at compile time, so they are
/// monomorphised here as 1D/2D/3D specialisations. SpatialTree and BruteForce
/// work at any runtime dimension.
pub enum CollisionBackend {
    Grid1D(HitGridND<TaggedNDimBounds, 1>),
    Grid2D(HitGridND<TaggedNDimBounds, 2>),
    Grid3D(HitGridND<TaggedNDimBounds, 3>),
    SparseGrid1D(SparseHitGridND<TaggedNDimBounds, 1>),
    SparseGrid2D(SparseHitGridND<TaggedNDimBounds, 2>),
    SparseGrid3D(SparseHitGridND<TaggedNDimBounds, 3>),
    Tree(SpatialTree<TaggedNDimBounds>),
    BruteForce(BruteForce<TaggedNDimBounds>),
}

impl CollisionBackend {
    pub fn new(strategy: Strategy, detection_dims: u8) -> Self {
        let wh = WORLD_HALF;
        let cl = CELL_SIZE;
        match (strategy, detection_dims) {
            (Strategy::Grid, 1) => Self::Grid1D(HitGridND::new([-wh], [wh * 2.0], cl)),
            (Strategy::Grid, 2) => {
                Self::Grid2D(HitGridND::new([-wh, -wh], [wh * 2.0, wh * 2.0], cl))
            }
            (Strategy::Grid, _) => Self::Grid3D(HitGridND::new(
                [-wh, -wh, -wh],
                [wh * 2.0, wh * 2.0, wh * 2.0],
                cl,
            )),

            (Strategy::SparseGrid, 1) => Self::SparseGrid1D(SparseHitGridND::new(cl)),
            (Strategy::SparseGrid, 2) => Self::SparseGrid2D(SparseHitGridND::new(cl)),
            (Strategy::SparseGrid, _) => Self::SparseGrid3D(SparseHitGridND::new(cl)),

            (Strategy::SpatialTree, dims) => {
                // Build root bounds matching the chosen dimensionality.
                let bounds = match dims {
                    1 => TaggedNDimBounds::new(
                        vec![Bounds::new(-wh, wh)],
                        PickId(0),
                    ),
                    2 => TaggedNDimBounds::new(
                        vec![Bounds::new(-wh, wh), Bounds::new(-wh, wh)],
                        PickId(0),
                    ),
                    _ => TaggedNDimBounds::new(
                        vec![
                            Bounds::new(-wh, wh),
                            Bounds::new(-wh, wh),
                            Bounds::new(-wh, wh),
                        ],
                        PickId(0),
                    ),
                };
                Self::Tree(SpatialTree::new(4, bounds))
            }

            (Strategy::BruteForce, _) => Self::BruteForce(BruteForce::new()),
        }
    }

    pub fn insert(&mut self, hb: TaggedNDimBounds) -> Vec<TaggedNDimBounds> {
        match self {
            Self::Grid1D(g) => g.insert(hb),
            Self::Grid2D(g) => g.insert(hb),
            Self::Grid3D(g) => g.insert(hb),
            Self::SparseGrid1D(g) => g.insert(hb),
            Self::SparseGrid2D(g) => g.insert(hb),
            Self::SparseGrid3D(g) => g.insert(hb),
            Self::Tree(t) => t.insert(hb),
            Self::BruteForce(b) => b.insert(hb),
        }
    }

    pub fn hit_candidates(&self, hb: TaggedNDimBounds) -> Vec<TaggedNDimBounds> {
        match self {
            Self::Grid1D(g) => g.hit_candidates(hb),
            Self::Grid2D(g) => g.hit_candidates(hb),
            Self::Grid3D(g) => g.hit_candidates(hb),
            Self::SparseGrid1D(g) => g.hit_candidates(hb),
            Self::SparseGrid2D(g) => g.hit_candidates(hb),
            Self::SparseGrid3D(g) => g.hit_candidates(hb),
            Self::Tree(t) => t.hit_candidates(hb),
            Self::BruteForce(b) => b.hit_candidates(hb),
        }
    }

    /// Returns line segment pairs `[start, end]` for visualising the partition structure.
    /// For grids: regular cell-boundary lines.
    /// For SpatialTree: node AABB edges.
    /// For BruteForce: world bounding box.
    pub fn partition_lines(&self, detection_dims: u8) -> Vec<[Vector3<f32>; 2]> {
        let wh = WORLD_HALF;
        let cl = CELL_SIZE;
        match self {
            Self::Grid1D(_) | Self::SparseGrid1D(_) => grid_lines_1d(wh, cl),
            Self::Grid2D(_) | Self::SparseGrid2D(_) => grid_lines_2d(wh, cl),
            Self::Grid3D(_) | Self::SparseGrid3D(_) => grid_lines_3d(wh, cl),
            Self::Tree(t) => tree_lines(t, detection_dims),
            Self::BruteForce(_) => bounding_box_lines(wh, detection_dims),
        }
    }
}

// ── Line generators ───────────────────────────────────────────────────────────

fn grid_lines_1d(wh: f32, cl: f32) -> Vec<[Vector3<f32>; 2]> {
    let mut lines = Vec::new();
    let y = PLANE_Y;
    // Main axis line along X
    lines.push([
        Vector3::new(-wh, y, 0.0),
        Vector3::new(wh, y, 0.0),
    ]);
    // Tick marks at each cell boundary
    let ticks = ((wh * 2.0) / cl).ceil() as i32 + 1;
    for i in 0..=ticks {
        let x = -wh + i as f32 * cl;
        lines.push([
            Vector3::new(x, y, -0.3),
            Vector3::new(x, y, 0.3),
        ]);
    }
    lines
}

fn grid_lines_2d(wh: f32, cl: f32) -> Vec<[Vector3<f32>; 2]> {
    let mut lines = Vec::new();
    let y = PLANE_Y;
    let cells = ((wh * 2.0) / cl).ceil() as i32 + 1;
    // Lines parallel to X axis (varying Z)
    for i in 0..=cells {
        let z = -wh + i as f32 * cl;
        lines.push([
            Vector3::new(-wh, y, z),
            Vector3::new(wh, y, z),
        ]);
    }
    // Lines parallel to Z axis (varying X)
    for i in 0..=cells {
        let x = -wh + i as f32 * cl;
        lines.push([
            Vector3::new(x, y, -wh),
            Vector3::new(x, y, wh),
        ]);
    }
    lines
}

fn grid_lines_3d(wh: f32, cl: f32) -> Vec<[Vector3<f32>; 2]> {
    let mut lines = Vec::new();
    let cells = ((wh * 2.0) / cl).ceil() as i32 + 1;
    // For each Y plane: draw XZ grid
    for iy in 0..=cells {
        let y = -wh + iy as f32 * cl;
        // Lines parallel to X
        for iz in 0..=cells {
            let z = -wh + iz as f32 * cl;
            lines.push([
                Vector3::new(-wh, y, z),
                Vector3::new(wh, y, z),
            ]);
        }
        // Lines parallel to Z
        for ix in 0..=cells {
            let x = -wh + ix as f32 * cl;
            lines.push([
                Vector3::new(x, y, -wh),
                Vector3::new(x, y, wh),
            ]);
        }
    }
    // Vertical lines (parallel to Y) at each (X,Z) corner
    for ix in 0..=cells {
        for iz in 0..=cells {
            let x = -wh + ix as f32 * cl;
            let z = -wh + iz as f32 * cl;
            lines.push([
                Vector3::new(x, -wh, z),
                Vector3::new(x, wh, z),
            ]);
        }
    }
    lines
}

fn tree_lines(
    tree: &SpatialTree<TaggedNDimBounds>,
    detection_dims: u8,
) -> Vec<[Vector3<f32>; 2]> {
    let mut lines = Vec::new();
    tree.visit_bounds(&mut |bounds: &TaggedNDimBounds, _depth| {
        let (x0, x1) = bounds.interval(0);
        let (z0, z1) = bounds.interval(1);
        let (y0, y1) = if detection_dims >= 3 {
            bounds.interval(2)
        } else {
            (PLANE_Y, PLANE_Y)
        };

        if detection_dims <= 2 {
            let y = PLANE_Y;
            // 4 edges of the XZ rectangle at fixed Y
            lines.push([Vector3::new(x0, y, z0), Vector3::new(x1, y, z0)]);
            lines.push([Vector3::new(x1, y, z0), Vector3::new(x1, y, z1)]);
            lines.push([Vector3::new(x1, y, z1), Vector3::new(x0, y, z1)]);
            lines.push([Vector3::new(x0, y, z1), Vector3::new(x0, y, z0)]);
        } else {
            // 12 edges of the 3D AABB
            // Bottom face
            lines.push([Vector3::new(x0, y0, z0), Vector3::new(x1, y0, z0)]);
            lines.push([Vector3::new(x1, y0, z0), Vector3::new(x1, y0, z1)]);
            lines.push([Vector3::new(x1, y0, z1), Vector3::new(x0, y0, z1)]);
            lines.push([Vector3::new(x0, y0, z1), Vector3::new(x0, y0, z0)]);
            // Top face
            lines.push([Vector3::new(x0, y1, z0), Vector3::new(x1, y1, z0)]);
            lines.push([Vector3::new(x1, y1, z0), Vector3::new(x1, y1, z1)]);
            lines.push([Vector3::new(x1, y1, z1), Vector3::new(x0, y1, z1)]);
            lines.push([Vector3::new(x0, y1, z1), Vector3::new(x0, y1, z0)]);
            // Vertical edges
            lines.push([Vector3::new(x0, y0, z0), Vector3::new(x0, y1, z0)]);
            lines.push([Vector3::new(x1, y0, z0), Vector3::new(x1, y1, z0)]);
            lines.push([Vector3::new(x1, y0, z1), Vector3::new(x1, y1, z1)]);
            lines.push([Vector3::new(x0, y0, z1), Vector3::new(x0, y1, z1)]);
        }
    });
    lines
}

fn bounding_box_lines(wh: f32, detection_dims: u8) -> Vec<[Vector3<f32>; 2]> {
    if detection_dims <= 2 {
        let y = PLANE_Y;
        let (x0, x1, z0, z1) = (-wh, wh, -wh, wh);
        vec![
            [Vector3::new(x0, y, z0), Vector3::new(x1, y, z0)],
            [Vector3::new(x1, y, z0), Vector3::new(x1, y, z1)],
            [Vector3::new(x1, y, z1), Vector3::new(x0, y, z1)],
            [Vector3::new(x0, y, z1), Vector3::new(x0, y, z0)],
        ]
    } else {
        let (x0, x1) = (-wh, wh);
        let (y0, y1) = (-wh, wh);
        let (z0, z1) = (-wh, wh);
        vec![
            // Bottom
            [Vector3::new(x0, y0, z0), Vector3::new(x1, y0, z0)],
            [Vector3::new(x1, y0, z0), Vector3::new(x1, y0, z1)],
            [Vector3::new(x1, y0, z1), Vector3::new(x0, y0, z1)],
            [Vector3::new(x0, y0, z1), Vector3::new(x0, y0, z0)],
            // Top
            [Vector3::new(x0, y1, z0), Vector3::new(x1, y1, z0)],
            [Vector3::new(x1, y1, z0), Vector3::new(x1, y1, z1)],
            [Vector3::new(x1, y1, z1), Vector3::new(x0, y1, z1)],
            [Vector3::new(x0, y1, z1), Vector3::new(x0, y1, z0)],
            // Verticals
            [Vector3::new(x0, y0, z0), Vector3::new(x0, y1, z0)],
            [Vector3::new(x1, y0, z0), Vector3::new(x1, y1, z0)],
            [Vector3::new(x1, y0, z1), Vector3::new(x1, y1, z1)],
            [Vector3::new(x0, y0, z1), Vector3::new(x0, y1, z1)],
        ]
    }
}
