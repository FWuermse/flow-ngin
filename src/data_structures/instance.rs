//! Instance transformation data for GPU rendering.
//!
//! Per-instance data like position, rotation, and scale is stored as
//! GPU buffers and passed to shaders for efficient multi-draw instancing.

use std::ops::{Add, Mul};

use cgmath::{InnerSpace, One, SquareMatrix};

use crate::data_structures::model;

/// Per-instance transformation: position, rotation (as quaternion), and scale.
///
/// Used for GPU instancing: multiple copies of the same model can be rendered
/// with different transforms in a single draw call. The instance data is packed
/// into a GPU buffer and accessible to vertex shaders.
#[derive(Clone, Debug)]
pub struct Instance {
    pub position: cgmath::Vector3<f32>,
    pub rotation: cgmath::Quaternion<f32>,
    pub scale: cgmath::Vector3<f32>,
}

impl Instance {
    /// Create a new instance with identity transformation (no move, rotate, or scale).
    pub fn new() -> Self {
        Self {
            position: cgmath::Vector3::new(0.0, 0.0, 0.0),
            // `Quaternion::one()` is the identity quaternion (no rotation)
            rotation: cgmath::Quaternion::one(),
            scale: cgmath::Vector3::new(1.0, 1.0, 1.0),
        }
    }

    pub fn to_matrix(&self) -> cgmath::Matrix4<f32> {
        cgmath::Matrix4::from_translation(self.position)
            * cgmath::Matrix4::from(self.rotation)
            * cgmath::Matrix4::from_nonuniform_scale(self.scale.x, self.scale.y, self.scale.z)
    }

    pub fn to_raw(&self) -> InstanceRaw {
        let world_matrix = self.to_matrix();
        let det = world_matrix.determinant();
        let handedness = det.signum();
        InstanceRaw {
            model: self.to_matrix().into(),
            normal: cgmath::Matrix3::from(self.rotation).into(),
            handedness: handedness,
        }
    }
}

impl Mul<Instance> for Instance {
    type Output = Self;

    fn mul(self, rhs: Instance) -> Self::Output {
        let new_rotation = self.rotation * rhs.rotation;

        let new_scale = cgmath::Vector3::new(
            self.scale.x * rhs.scale.x,
            self.scale.y * rhs.scale.y,
            self.scale.z * rhs.scale.z,
        );
        let scaled_rhs_pos = cgmath::Vector3::new(
            self.scale.x * rhs.position.x,
            self.scale.y * rhs.position.y,
            self.scale.z * rhs.position.z,
        );
        let new_position = self.position + (self.rotation * scaled_rhs_pos);

        Instance {
            position: new_position,
            rotation: new_rotation,
            scale: new_scale,
        }
    }
}

impl Add<Instance> for Instance {
    type Output = Self;

    fn add(self, rhs: Instance) -> Self::Output {
        Instance {
            position: self.position + rhs.position,
            rotation: (self.rotation * rhs.rotation).normalize(),
            scale: cgmath::Vector3::new(
                self.scale.x * rhs.scale.x,
                self.scale.y * rhs.scale.y,
                self.scale.z * rhs.scale.z,
            ),
        }
    }
}

impl<'a, 'b> Mul<&'b Instance> for &'a Instance {
    type Output = Instance;

    fn mul(self, rhs: &'b Instance) -> Self::Output {
        let new_rotation = self.rotation * rhs.rotation;

        let new_scale = cgmath::Vector3::new(
            self.scale.x * rhs.scale.x,
            self.scale.y * rhs.scale.y,
            self.scale.z * rhs.scale.z,
        );
        let scaled_rhs_pos = cgmath::Vector3::new(
            self.scale.x * rhs.position.x,
            self.scale.y * rhs.position.y,
            self.scale.z * rhs.position.z,
        );
        let new_position = self.position + (self.rotation * scaled_rhs_pos);

        Instance {
            position: new_position,
            rotation: new_rotation,
            scale: new_scale,
        }
    }
}

impl<'a, 'b> Add<&'b Instance> for &'a Instance {
    type Output = Instance;

    fn add(self, rhs: &'b Instance) -> Self::Output {
        Instance {
            position: self.position + rhs.position,
            rotation: (self.rotation * rhs.rotation).normalize(),
            scale: cgmath::Vector3::new(
                self.scale.x * rhs.scale.x,
                self.scale.y * rhs.scale.y,
                self.scale.z * rhs.scale.z,
            ),
        }
    }
}

impl From<cgmath::Vector3<f32>> for Instance {
    fn from(position: cgmath::Vector3<f32>) -> Self {
        Instance {
            position,
            ..Default::default()
        }
    }
}

impl Default for Instance {
    fn default() -> Self {
        Self::new()
    }
}

/**
 * The raw instance is the actual data stored on the GPU
 */
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
#[allow(dead_code)]
pub struct InstanceRaw {
    model: [[f32; 4]; 4],
    normal: [[f32; 3]; 3],
    handedness: f32,
}

/**
 * As we store vertex data directly in the GPU memory we need to tell what the bytes refer to:
 *
 * offset: zero as we want to use the full space.
 * stride: length of a vertex
 *
 * Stride layout here: position + rotation + scale as 4x4 matrix (hence the four 4d vectors)
 */
#[cfg(test)]
mod tests {
    use super::*;
    use cgmath::{assert_relative_eq, Deg, Matrix4, Quaternion, Rotation3, SquareMatrix, Vector3};

    fn approx_eq_instance(a: &Instance, b: &Instance) {
        assert_relative_eq!(a.position.x, b.position.x, epsilon = 1e-5);
        assert_relative_eq!(a.position.y, b.position.y, epsilon = 1e-5);
        assert_relative_eq!(a.position.z, b.position.z, epsilon = 1e-5);
        assert_relative_eq!(a.rotation.v.x, b.rotation.v.x, epsilon = 1e-5);
        assert_relative_eq!(a.rotation.v.y, b.rotation.v.y, epsilon = 1e-5);
        assert_relative_eq!(a.rotation.v.z, b.rotation.v.z, epsilon = 1e-5);
        assert_relative_eq!(a.rotation.s, b.rotation.s, epsilon = 1e-5);
        assert_relative_eq!(a.scale.x, b.scale.x, epsilon = 1e-5);
        assert_relative_eq!(a.scale.y, b.scale.y, epsilon = 1e-5);
        assert_relative_eq!(a.scale.z, b.scale.z, epsilon = 1e-5);
    }

    #[test]
    fn identity_matrix() {
        let m = Instance::new().to_matrix();
        assert_relative_eq!(m, Matrix4::identity(), epsilon = 1e-6);
    }

    #[test]
    fn identity_mul_left() {
        let identity = Instance::new();
        let a = Instance {
            position: Vector3::new(1.0, 2.0, 3.0),
            rotation: Quaternion::from_axis_angle(Vector3::new(0.0, 1.0, 0.0), Deg(45.0)),
            scale: Vector3::new(2.0, 3.0, 4.0),
        };
        let result = identity * a.clone();
        approx_eq_instance(&result, &a);
    }

    #[test]
    fn identity_mul_right() {
        let identity = Instance::new();
        let a = Instance {
            position: Vector3::new(1.0, 2.0, 3.0),
            rotation: Quaternion::from_axis_angle(Vector3::new(0.0, 1.0, 0.0), Deg(45.0)),
            scale: Vector3::new(2.0, 3.0, 4.0),
        };
        let result = a.clone() * identity;
        approx_eq_instance(&result, &a);
    }

    #[test]
    fn to_raw_handedness_positive() {
        let instance = Instance {
            position: Vector3::new(0.0, 0.0, 0.0),
            rotation: Quaternion::one(),
            scale: Vector3::new(1.0, 1.0, 1.0),
        };
        let raw = instance.to_raw();
        assert_eq!(raw.handedness, 1.0);
    }

    #[test]
    fn to_raw_handedness_negative() {
        // Flip one axis to make determinant negative
        let instance = Instance {
            position: Vector3::new(0.0, 0.0, 0.0),
            rotation: Quaternion::one(),
            scale: Vector3::new(-1.0, 1.0, 1.0),
        };
        let raw = instance.to_raw();
        assert_eq!(raw.handedness, -1.0);
    }

    #[test]
    fn add_positions() {
        let a = Instance {
            position: Vector3::new(1.0, 2.0, 3.0),
            rotation: Quaternion::one(),
            scale: Vector3::new(1.0, 1.0, 1.0),
        };
        let b = Instance {
            position: Vector3::new(4.0, 5.0, 6.0),
            rotation: Quaternion::one(),
            scale: Vector3::new(1.0, 1.0, 1.0),
        };
        let result = a.clone() + b.clone();
        assert_relative_eq!(result.position.x, a.position.x + b.position.x, epsilon = 1e-6);
        assert_relative_eq!(result.position.y, a.position.y + b.position.y, epsilon = 1e-6);
        assert_relative_eq!(result.position.z, a.position.z + b.position.z, epsilon = 1e-6);
    }

    #[test]
    fn mul_scales_component_wise() {
        let a = Instance {
            position: Vector3::new(0.0, 0.0, 0.0),
            rotation: Quaternion::one(),
            scale: Vector3::new(2.0, 3.0, 4.0),
        };
        let b = Instance {
            position: Vector3::new(0.0, 0.0, 0.0),
            rotation: Quaternion::one(),
            scale: Vector3::new(5.0, 6.0, 7.0),
        };
        let result = a.clone() * b.clone();
        assert_relative_eq!(result.scale.x, a.scale.x * b.scale.x, epsilon = 1e-6);
        assert_relative_eq!(result.scale.y, a.scale.y * b.scale.y, epsilon = 1e-6);
        assert_relative_eq!(result.scale.z, a.scale.z * b.scale.z, epsilon = 1e-6);
    }

    #[test]
    fn mul_translation_rotated() {
        // Parent at origin, rotated 90° around Y. Child at (1,0,0) local.
        // After composition the child should be at (0,0,-1) in world space.
        let parent = Instance {
            position: Vector3::new(0.0, 0.0, 0.0),
            rotation: Quaternion::from_axis_angle(Vector3::new(0.0, 1.0, 0.0), Deg(90.0)),
            scale: Vector3::new(1.0, 1.0, 1.0),
        };
        let child = Instance {
            position: Vector3::new(1.0, 0.0, 0.0),
            rotation: Quaternion::one(),
            scale: Vector3::new(1.0, 1.0, 1.0),
        };
        let result = parent * child;
        // 90° Y-rotation maps (1,0,0) → (0,0,-1)
        assert_relative_eq!(result.position.x, 0.0, epsilon = 1e-5);
        assert_relative_eq!(result.position.y, 0.0, epsilon = 1e-5);
        assert_relative_eq!(result.position.z, -1.0, epsilon = 1e-5);
    }

    #[test]
    fn from_vector3() {
        let v = Vector3::new(3.0, 4.0, 5.0);
        let instance = Instance::from(v);
        assert_relative_eq!(instance.position.x, v.x, epsilon = 1e-6);
        assert_relative_eq!(instance.position.y, v.y, epsilon = 1e-6);
        assert_relative_eq!(instance.position.z, v.z, epsilon = 1e-6);
    }

    // Instance::Add should combine scales additively relative to identity (1+1=1 not 2),
    // or alternatively should not use component-wise addition for scale at all.
    // The current behavior doubles the scale, breaking any usage of Add for accumulation.
    #[test]
    fn add_scale_is_not_doubled() {
        let a = Instance::new();
        let b = Instance::new();
        let result = a + b;
        assert_relative_eq!(result.scale.x, 1.0, epsilon = 1e-6);
        assert_relative_eq!(result.scale.y, 1.0, epsilon = 1e-6);
        assert_relative_eq!(result.scale.z, 1.0, epsilon = 1e-6);
    }

    // Instance::Add must be associative: (a + b) + c == a + (b + c).
    #[test]
    fn add_is_associative() {
        let a = Instance {
            position: Vector3::new(0.0, 0.0, 0.0),
            rotation: Quaternion::from_axis_angle(Vector3::new(0.0, 1.0, 0.0), Deg(0.0)),
            scale: Vector3::new(1.0, 1.0, 1.0),
        };
        let b = Instance {
            position: Vector3::new(0.0, 0.0, 0.0),
            rotation: Quaternion::from_axis_angle(Vector3::new(0.0, 1.0, 0.0), Deg(90.0)),
            scale: Vector3::new(1.0, 1.0, 1.0),
        };
        let c = Instance {
            position: Vector3::new(0.0, 0.0, 0.0),
            rotation: Quaternion::from_axis_angle(Vector3::new(0.0, 1.0, 0.0), Deg(180.0)),
            scale: Vector3::new(1.0, 1.0, 1.0),
        };
        let ab_c = (a.clone() + b.clone()) + c.clone();
        let a_bc = a + (b + c);
        assert_relative_eq!(ab_c.rotation.s,   a_bc.rotation.s,   epsilon = 1e-5);
        assert_relative_eq!(ab_c.rotation.v.x, a_bc.rotation.v.x, epsilon = 1e-5);
        assert_relative_eq!(ab_c.rotation.v.y, a_bc.rotation.v.y, epsilon = 1e-5);
        assert_relative_eq!(ab_c.rotation.v.z, a_bc.rotation.v.z, epsilon = 1e-5);
    }
}

#[cfg(kani)]
mod kani_proofs {
    use super::*;
    use cgmath::{One, Quaternion, Vector3};

    fn bounded_instance() -> Instance {
        let px: f32 = kani::any();
        let py: f32 = kani::any();
        let pz: f32 = kani::any();
        kani::assume(px.abs() < 1e4 && py.abs() < 1e4 && pz.abs() < 1e4);
        let sx: f32 = kani::any();
        let sy: f32 = kani::any();
        let sz: f32 = kani::any();
        kani::assume(sx.abs() < 1e4 && sy.abs() < 1e4 && sz.abs() < 1e4);
        // Unit quaternion: constrain to identity for tractability
        Instance {
            position: Vector3::new(px, py, pz),
            rotation: Quaternion::one(),
            scale: Vector3::new(sx, sy, sz),
        }
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn verify_to_raw_no_panic() {
        let a = bounded_instance();
        let _ = a.to_raw();
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn verify_mul_no_panic() {
        let a = bounded_instance();
        let b = bounded_instance();
        let _ = a * b;
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn verify_add_no_panic() {
        let a = bounded_instance();
        let b = bounded_instance();
        let _ = a + b;
    }

    /// `Add` for Instance adds quaternion components directly, which breaks the
    /// unit-norm invariant. This harness documents and catches that: given two
    /// unit quaternions (norm == 1), their component-wise sum will NOT have norm 1
    /// in general, meaning the resulting rotation is invalid.
    #[kani::proof]
    #[kani::unwind(1)]
    fn verify_add_rotation_unit_norm() {
        // Build two instances with arbitrary unit quaternions
        let s1: f32 = kani::any();
        let xi: f32 = kani::any();
        let yi: f32 = kani::any();
        let zi: f32 = kani::any();
        // Assume unit quaternion: s^2 + x^2 + y^2 + z^2 == 1
        kani::assume(s1.is_finite() && xi.is_finite() && yi.is_finite() && zi.is_finite());
        let norm_sq_a = s1 * s1 + xi * xi + yi * yi + zi * zi;
        kani::assume((norm_sq_a - 1.0).abs() < 1e-6);

        let s2: f32 = kani::any();
        let xj: f32 = kani::any();
        let yj: f32 = kani::any();
        let zj: f32 = kani::any();
        kani::assume(s2.is_finite() && xj.is_finite() && yj.is_finite() && zj.is_finite());
        let norm_sq_b = s2 * s2 + xj * xj + yj * yj + zj * zj;
        kani::assume((norm_sq_b - 1.0).abs() < 1e-6);

        let a = Instance {
            position: cgmath::Vector3::new(0.0, 0.0, 0.0),
            rotation: Quaternion::new(s1, xi, yi, zi),
            scale: cgmath::Vector3::new(1.0, 1.0, 1.0),
        };
        let b = Instance {
            position: cgmath::Vector3::new(0.0, 0.0, 0.0),
            rotation: Quaternion::new(s2, xj, yj, zj),
            scale: cgmath::Vector3::new(1.0, 1.0, 1.0),
        };
        let result = a + b;
        let rs = result.rotation.s;
        let rv = result.rotation.v;
        let norm_sq = rs * rs + rv.x * rv.x + rv.y * rv.y + rv.z * rv.z;
        // This assertion SHOULD FAIL: component-wise quaternion addition does not
        // preserve unit norm, so the resulting rotation is not a valid unit quaternion.
        kani::assert((norm_sq - 1.0).abs() < 1e-3, "Add preserves unit quaternion norm");
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn verify_identity_mul_left() {
        let identity = Instance::new();
        let a = bounded_instance();
        let result = identity * a.clone();
        kani::assert(
            (result.position.x - a.position.x).abs() < 1e-3,
            "identity * a position.x == a.position.x",
        );
        kani::assert(
            (result.scale.x - a.scale.x).abs() < 1e-3,
            "identity * a scale.x == a.scale.x",
        );
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn verify_identity_mul_right() {
        let identity = Instance::new();
        let a = bounded_instance();
        let result = a.clone() * identity;
        kani::assert(
            (result.position.x - a.position.x).abs() < 1e-3,
            "a * identity position.x == a.position.x",
        );
        kani::assert(
            (result.scale.x - a.scale.x).abs() < 1e-3,
            "a * identity scale.x == a.scale.x",
        );
    }

    /// Verify that Instance::Mul is associative for scale: (a*b)*c == a*(b*c).
    /// This should hold since scale multiplication is component-wise f32 multiplication.
    #[kani::proof]
    #[kani::unwind(1)]
    fn verify_mul_associative_scale() {
        let a = bounded_instance();
        let b = bounded_instance();
        let c = bounded_instance();
        let ab_c = (a.clone() * b.clone()) * c.clone();
        let a_bc = a * (b * c);
        kani::assert(
            (ab_c.scale.x - a_bc.scale.x).abs() < 1e-1,
            "Mul is associative for scale.x",
        );
    }

    /// Verify that to_raw produces finite handedness for bounded instances.
    #[kani::proof]
    #[kani::unwind(1)]
    fn verify_to_raw_handedness_finite() {
        let a = bounded_instance();
        let raw = a.to_raw();
        kani::assert(
            raw.handedness.is_finite(),
            "handedness must be finite for bounded instances",
        );
    }
}

impl model::Vertex for InstanceRaw {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<InstanceRaw>() as wgpu::BufferAddress,
            // We need to switch from using a step mode of Vertex to Instance
            // This means that our shaders will only change to use the next
            // instance when the shader starts processing a new instance
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // A mat4 takes up 4 vertex slots as it is technically 4 vec4s. We need to define a slot
                // for each vec4. We don't have to do this in code, though.
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    // corresponds to the @location in the shader file.
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 8]>() as wgpu::BufferAddress,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 12]>() as wgpu::BufferAddress,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // Tangent data will be stored as 3x3 matrix
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 16]>() as wgpu::BufferAddress,
                    shader_location: 9,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 19]>() as wgpu::BufferAddress,
                    shader_location: 10,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 22]>() as wgpu::BufferAddress,
                    shader_location: 11,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 25]>() as wgpu::BufferAddress,
                    shader_location: 12,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        }
    }
}
