use crate::document::{Block, InstanceData};
use cgmath::{EuclideanSpace, InnerSpace, Matrix4, Point3, Rotation, Vector3, Vector4};
pub const EPS: f32 = 1e-4;
#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub min: Vector3<f32>,
    pub max: Vector3<f32>,
}
impl Bounds {
    pub fn center(self) -> Vector3<f32> {
        (self.min + self.max) * 0.5
    }
    pub fn footprint_overlaps(self, b: Self) -> bool {
        self.min.x < b.max.x - EPS
            && self.max.x > b.min.x + EPS
            && self.min.z < b.max.z - EPS
            && self.max.z > b.min.z + EPS
    }
}
pub fn axes(t: InstanceData) -> [Vector3<f32>; 3] {
    [Vector3::unit_x(), Vector3::unit_y(), Vector3::unit_z()].map(|v| t.rotation().rotate_vector(v))
}
pub fn corners(t: InstanceData) -> [Vector3<f32>; 8] {
    let center = t.center();
    let axes = axes(t);
    std::array::from_fn(|i| {
        center
            + (0..3).fold(Vector3::new(0., 0., 0.), |v, j| {
                v + axes[j] * t.scale[j] * if i & (1 << j) == 0 { -0.5 } else { 0.5 }
            })
    })
}
pub fn bounds(t: InstanceData) -> Bounds {
    let corners = corners(t);
    let mut min = corners[0];
    let mut max = min;
    for c in corners {
        for j in 0..3 {
            min[j] = min[j].min(c[j]);
            max[j] = max[j].max(c[j]);
        }
    }
    Bounds { min, max }
}
pub fn group_bounds(blocks: &[Block]) -> Option<Bounds> {
    let mut b = bounds(blocks.first()?.transform);
    for block in &blocks[1..] {
        let a = bounds(block.transform);
        for j in 0..3 {
            b.min[j] = b.min[j].min(a.min[j]);
            b.max[j] = b.max[j].max(a.max[j]);
        }
    }
    Some(b)
}
/// Full 15-axis OBB SAT. Face contact is allowed.
pub fn overlaps(a: InstanceData, b: InstanceData) -> bool {
    let aa = axes(a);
    let bb = axes(b);
    let d = b.center() - a.center();
    let mut candidates = Vec::from(aa);
    candidates.extend(bb);
    for x in aa {
        for y in bb {
            candidates.push(x.cross(y));
        }
    }
    for axis in candidates {
        if axis.magnitude2() < 1e-10 {
            continue;
        }
        let axis = axis.normalize();
        let ra = (0..3)
            .map(|i| a.scale[i] * 0.5 * aa[i].dot(axis).abs())
            .sum::<f32>();
        let rb = (0..3)
            .map(|i| b.scale[i] * 0.5 * bb[i].dot(axis).abs())
            .sum::<f32>();
        if d.dot(axis).abs() >= ra + rb - EPS {
            return false;
        }
    }
    true
}
pub fn valid_placement(preview: &[Block], placed: &[Block], excluded: &[u64]) -> bool {
    preview.iter().all(|b| {
        b.transform.valid()
            && bounds(b.transform).min.y >= -EPS
            && placed
                .iter()
                .filter(|p| !excluded.contains(&p.id))
                .all(|p| !overlaps(b.transform, p.transform))
    })
}
pub fn ray_box(origin: Point3<f32>, dir: Vector3<f32>, t: InstanceData) -> Option<f32> {
    let inv = t.rotation().conjugate();
    let o = inv.rotate_vector(origin - Point3::from_vec(t.center()));
    let d = inv.rotate_vector(dir);
    let mut near = 0f32;
    let mut far = f32::INFINITY;
    for i in 0..3 {
        let half = t.scale[i] * 0.5;
        if d[i].abs() < 1e-7 {
            if o[i].abs() > half {
                return None;
            }
        } else {
            let a = (-half - o[i]) / d[i];
            let b = (half - o[i]) / d[i];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return None;
            }
        }
    }
    (far >= 0.).then_some(near)
}
pub fn pick(origin: Point3<f32>, dir: Vector3<f32>, blocks: &[Block]) -> Option<u64> {
    blocks
        .iter()
        .filter_map(|b| ray_box(origin, dir, b.transform).map(|distance| (b.id, distance)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|p| p.0)
}
pub fn project(
    center: Vector3<f32>,
    view_projection: Matrix4<f32>,
    size: [f32; 2],
) -> Option<[f32; 2]> {
    let clip = view_projection * Vector4::new(center.x, center.y, center.z, 1.);
    if clip.w <= 0. || clip.z < 0. || clip.z > clip.w {
        return None;
    }
    Some([
        (clip.x / clip.w + 1.) * 0.5 * size[0],
        (1. - clip.y / clip.w) * 0.5 * size[1],
    ])
}
