//! Instance transformation data for GPU rendering.
//!
//! Per-instance data like position, rotation, and scale is stored as
//! GPU buffers and passed to shaders for efficient multi-draw instancing.

use std::ops::{Add, Mul};

use cgmath::{One, SquareMatrix};

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
            rotation: self.rotation + rhs.rotation,
            scale: self.scale + rhs.scale,
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
            rotation: self.rotation + rhs.rotation,
            scale: self.scale + rhs.scale,
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
