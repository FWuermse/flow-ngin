//! Building blocks implemented via GPU instancing.
//!
//! Provides [`BuildingBlocks`], a collection of identically-shaped objects
//! (e.g., construction blocks or crowds) rendered efficiently using GPU instancing. Note that
//! hidden blocks are not culled, so this may not be optimal for large voxel worlds.
//!
//! Blocks can optionally tile their texture by instance scale ([`BuildingBlocks::with_tiled_texture`])
//! and swap to a camera-facing 2D model when far away ([`BuildingBlocks::with_lod`]).

use crate::{
    context::GPUResource,
    data_structures::{
        instance::{Instance, InstanceRaw},
        model::{self},
        texture::Texture,
    },
    pick::PickId,
    render::{Instanced, Render},
    resources::{self, pick::load_pick_model},
};
use cgmath::{EuclideanSpace, InnerSpace, One, Rotation3, Zero};
use wgpu::{Device, util::DeviceExt};

/// A collection of identically-shaped building blocks.
///
/// Uses GPU instancing to efficiently render many copies of the same model
/// with different transformations. Currently does not perform frustum culling
/// or occlusion culling, so performance may degrade with very large numbers of blocks.
pub struct BuildingBlocks {
    // TODO: create apis and make fields private
    pub id: PickId,
    pub obj_model: model::Model,
    // TODO: retire this param
    #[allow(dead_code)]
    obj_file: String,
    instances: Vec<Instance>,
    instance_buffer: wgpu::Buffer,
    buffer_size_needs_change: bool,
    tiled: bool,
    lod: Option<Lod>,
    near_amount: usize,
}

/// A 2D stand-in model drawn instead of the full model for instances beyond `distance`.
struct Lod {
    model: model::Model,
    distance: f32,
    instance_buffer: wgpu::Buffer,
    amount: usize,
}

/// Rotation around Y that turns a quad facing +Z towards the camera.
pub(crate) fn billboard_rotation(
    position: cgmath::Vector3<f32>,
    camera_pos: cgmath::Point3<f32>,
) -> cgmath::Quaternion<f32> {
    let d = camera_pos.to_vec() - position;
    cgmath::Quaternion::from_angle_y(cgmath::Rad(d.x.atan2(d.z)))
}

pub(crate) fn is_far(
    position: cgmath::Vector3<f32>,
    camera_pos: cgmath::Point3<f32>,
    distance: f32,
) -> bool {
    (camera_pos.to_vec() - position).magnitude2() > distance * distance
}

fn mk_instance_buffer(device: &wgpu::Device, label: &str, amount: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (amount * std::mem::size_of::<InstanceRaw>()) as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

pub(crate) fn uniform_instances(
    amount: usize,
    start_position: cgmath::Vector3<f32>,
    start_rotation: cgmath::Quaternion<f32>,
) -> Vec<Instance> {
    (0..amount)
        .map(|_| {
            let mut instance = Instance::new();
            instance.position = start_position;
            instance.rotation = if start_position.is_zero() {
                cgmath::Quaternion::from_axis_angle(cgmath::Vector3::unit_z(), cgmath::Deg(0.0))
            } else {
                start_rotation
            };
            instance
        })
        .collect()
}

impl AsRef<BuildingBlocks> for BuildingBlocks {
    fn as_ref(&self) -> &BuildingBlocks {
        self
    }
}

pub trait WorldCoordMesh {
    /// Returns an immutable reference to instances
    fn instances(&self) -> &Vec<Instance>;

    /// Returns a mutable reference to instances, assumes the instance size changes and recalculates buffers.
    /// If you only mutate some values but don't intend to change buffer sizes use `instances_mut_size_unchanged`
    fn instances_mut(&mut self) -> &mut Vec<Instance>;

    fn instances_mut_size_unchanged(&mut self) -> &mut [Instance];

    fn set_instances(&mut self, instances: Vec<Instance>);

    fn set_instance(&mut self, idx: usize, instance: Instance);

    fn add_instance(&mut self, instance: Instance);

    fn add_instances(&mut self, instances: Vec<Instance>);

    fn clear_first(&mut self, amount: usize);

    fn clear_at(&mut self, from: usize, to: usize);

    /// Returns the inner instanced of the `Default` render for possible optimizations with `Defaults`
    fn to_instanced(&self) -> Instanced<'_>;
}

impl BuildingBlocks {
    pub async fn new(
        id: impl Into<PickId>,
        queue: &wgpu::Queue,
        device: &wgpu::Device,
        start_position: cgmath::Vector3<f32>,
        start_rotation: cgmath::Quaternion<f32>,
        amount: usize,
        obj_file: &str,
    ) -> Self {
        let obj_model = resources::load_model_obj(obj_file, &device, &queue).await;
        if let Err(e) = obj_model {
            panic!("Error failed to load model {}: {}", obj_file, e);
        }
        let obj_model = obj_model.unwrap();

        let instances = uniform_instances(amount, start_position, start_rotation);

        let instance_data = instances.iter().map(Instance::to_raw).collect::<Vec<_>>();
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Instance Buffer"),
            contents: bytemuck::cast_slice(&instance_data),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            obj_model,
            instances,
            obj_file: obj_file.to_string(),
            instance_buffer,
            // Ids may be used later for picking, hitboxes, etc.
            id: id.into(),
            buffer_size_needs_change: false,
            tiled: false,
            lod: None,
            near_amount: amount,
        }
    }

    /// Tile the texture by the instance scale instead of stretching it, e.g. an instance
    /// scaled `(10, 1, 1)` repeats its texture 10 times along that axis.
    pub fn with_tiled_texture(mut self, tiled: bool) -> Self {
        self.tiled = tiled;
        self
    }

    /// Draw instances further than `distance` from the camera as `texture` on a quad,
    /// rotated around Y to face the camera. The texture's alpha is used as a cut-out mask.
    ///
    /// The quad is `height` model units tall (width from the image's aspect ratio) and
    /// stands bottom-centred on the instance origin.
    /// LOD only applies when updating via [`write_to_buffer_lod`](Self::write_to_buffer_lod)
    /// or [`write_to_buffer_lod_offset`](Self::write_to_buffer_lod_offset). Picking always uses
    /// the full model.
    pub fn with_lod(
        mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: Texture,
        height: f32,
        distance: f32,
    ) -> Self {
        let model = resources::sprite_model(device, queue, texture, height).expect("LOD sprite mesh");
        self.lod = Some(Lod {
            model,
            distance,
            instance_buffer: mk_instance_buffer(device, "LOD Instance Buffer", self.instances.len()),
            amount: 0,
        });
        // Both buffers are sized to all instances from here on
        self.buffer_size_needs_change = true;
        self
    }

    /// Like `write_to_buffer` but draws instances beyond the LOD distance from `camera_pos`
    /// as camera-facing LOD models. Falls back to `write_to_buffer` without a LOD.
    pub fn write_to_buffer_lod(
        &mut self,
        queue: &wgpu::Queue,
        device: &wgpu::Device,
        camera_pos: cgmath::Point3<f32>,
    ) {
        self.write_to_buffer_lod_offset(queue, device, camera_pos, &Instance::new());
    }

    /// Like `write_to_buffer_lod` but applies `offset` first, see `write_to_buffer_offset`.
    /// The LOD distance is measured from the offset (world) positions.
    pub fn write_to_buffer_lod_offset(
        &mut self,
        queue: &wgpu::Queue,
        device: &wgpu::Device,
        camera_pos: cgmath::Point3<f32>,
        offset: &Instance,
    ) {
        let Some(lod) = &mut self.lod else {
            return self.write_to_buffer_offset(queue, device, offset);
        };
        let (far, near): (Vec<Instance>, Vec<Instance>) = self
            .instances
            .iter()
            .map(|local| offset * local)
            .partition(|i| is_far(i.position, camera_pos, lod.distance));
        let near = near.iter().map(Instance::to_raw).collect::<Vec<_>>();
        let far = far
            .into_iter()
            .map(|i| {
                Instance {
                    rotation: billboard_rotation(i.position, camera_pos),
                    ..i
                }
                .to_raw()
            })
            .collect::<Vec<_>>();
        if self.buffer_size_needs_change {
            // Sized to all instances so a changing near/far split never reallocates
            self.instance_buffer = mk_instance_buffer(device, "Instance Buffer", self.instances.len());
            lod.instance_buffer = mk_instance_buffer(device, "LOD Instance Buffer", self.instances.len());
            self.buffer_size_needs_change = false;
        }
        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&near));
        queue.write_buffer(&lod.instance_buffer, 0, bytemuck::cast_slice(&far));
        self.near_amount = near.len();
        lod.amount = far.len();
    }

    /// All instances are drawn with the full model (used by the non-LOD write paths).
    fn reset_lod_split(&mut self) {
        self.near_amount = self.instances.len();
        if let Some(lod) = &mut self.lod {
            lod.amount = 0;
        }
    }

    pub async fn mk_multiple(
        queue: &wgpu::Queue,
        device: &wgpu::Device,
        amount: usize,
        descr: &[(PickId, &'static str)],
    ) -> Vec<BuildingBlocks> {
        let futures = descr.into_iter().map(|(id, file_name)| {
            BuildingBlocks::new(
                *id,
                queue,
                device,
                cgmath::Vector3::zero(),
                cgmath::Quaternion::one(),
                amount,
                file_name,
            )
        });
        futures::future::join_all(futures).await
    }

    /**
     * This method creates a copy of the original Block (and instances) where only the
     * fragment shader differs. The fragment shader is a U32 id referring to the object
     * that was drawn.
     *
     * This is used to draw a pick shader which allows identifying objects clicked on
     * with a mouse pointer.
     *
     * TODO: make this a trait if possible
     */
    pub fn to_clickable(&self, device: &Device, id: PickId) -> Self {
        let obj_model = load_pick_model(device, id, self.obj_model.meshes.clone()).unwrap();

        let instance_data = self
            .instances
            .iter()
            .map(Instance::to_raw)
            .collect::<Vec<_>>();
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Instance Buffer for Picking"),
            contents: bytemuck::cast_slice(&instance_data),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            obj_model: obj_model,
            obj_file: self.obj_file.clone(),
            instances: self.instances.clone(),
            instance_buffer,
            id,
            buffer_size_needs_change: false,
            tiled: false,
            lod: None,
            near_amount: self.instances.len(),
        }
    }
}

impl WorldCoordMesh for BuildingBlocks {
    fn instances(&self) -> &Vec<Instance> {
        &self.instances
    }

    fn instances_mut(&mut self) -> &mut Vec<Instance> {
        self.buffer_size_needs_change = true;
        &mut self.instances
    }

    fn instances_mut_size_unchanged(&mut self) -> &mut [Instance] {
        self.instances.as_mut_slice()
    }

    fn set_instances(&mut self, instances: Vec<Instance>) {
        self.instances = instances;
        self.buffer_size_needs_change = true;
    }

    fn set_instance(&mut self, idx: usize, instance: Instance) {
        self.instances[idx] = instance;
    }

    fn add_instance(&mut self, instance: Instance) {
        self.instances.push(instance);
        self.buffer_size_needs_change = true;
    }

    fn add_instances(&mut self, mut instances: Vec<Instance>) {
        self.instances.append(&mut instances);
        self.buffer_size_needs_change = true;
    }

    fn clear_first(&mut self, amount: usize) {
        self.buffer_size_needs_change = true;
        self.instances.drain(0..amount);
    }

    fn clear_at(&mut self, from: usize, to: usize) {
        self.buffer_size_needs_change = true;
        self.instances.drain(from..=to);
    }

    fn to_instanced(&self) -> Instanced<'_> {
        Instanced {
            instance: &self.instance_buffer,
            model: &self.obj_model,
            amount: self.near_amount,
            front_face: wgpu::FrontFace::Ccw,
            tiled: self.tiled,
            id: self.id,
        }
    }
}

impl<'a, 'pass> GPUResource<'a, 'pass> for BuildingBlocks {
    fn write_to_buffer(&mut self, queue: &wgpu::Queue, device: &wgpu::Device) {
        self.reset_lod_split();
        let raws = self
            .instances
            .iter()
            .map(Instance::to_raw)
            .collect::<Vec<_>>();
        if self.buffer_size_needs_change {
            self.instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Instance Buffer"),
                contents: bytemuck::cast_slice(&raws),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
            self.buffer_size_needs_change = false;
        } else {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&raws));
        }
    }

    fn get_render(&'a self) -> Render<'a, 'pass> {
        match &self.lod {
            Some(lod) if lod.amount > 0 => Render::Defaults(vec![
                self.to_instanced(),
                Instanced {
                    instance: &lod.instance_buffer,
                    model: &lod.model,
                    amount: lod.amount,
                    front_face: wgpu::FrontFace::Ccw,
                    tiled: false,
                    id: self.id,
                },
            ]),
            _ => Render::Default(self.to_instanced()),
        }
    }

    fn write_to_buffer_offset(
        &mut self,
        queue: &wgpu::Queue,
        device: &wgpu::Device,
        offset: &Instance,
    ) {
        self.reset_lod_split();
        let raws = self
            .instances
            .iter()
            .map(|local| (offset * local).to_raw())
            .collect::<Vec<_>>();
        if self.buffer_size_needs_change {
            self.instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Offset Instance Buffer"),
                contents: bytemuck::cast_slice(&raws),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
            self.buffer_size_needs_change = false;
        } else {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&raws));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cgmath::{Deg, Quaternion, Rotation3, Vector3, assert_relative_eq};

    #[test]
    fn uniform_instances_correct_count() {
        let instances = uniform_instances(5, Vector3::new(1.0, 2.0, 3.0), Quaternion::one());
        assert_eq!(instances.len(), 5);
    }

    #[test]
    fn uniform_instances_zero_count_is_empty() {
        let instances = uniform_instances(0, Vector3::new(1.0, 0.0, 0.0), Quaternion::one());
        assert!(instances.is_empty());
    }

    #[test]
    fn uniform_instances_all_share_position() {
        let pos = Vector3::new(3.0, 4.0, 5.0);
        let instances = uniform_instances(3, pos, Quaternion::one());
        for inst in &instances {
            assert_eq!(inst.position, pos);
        }
    }

    #[test]
    fn uniform_instances_zero_position_uses_identity_rotation() {
        let rot = Quaternion::from_axis_angle(Vector3::unit_y(), Deg(45.0));
        let instances = uniform_instances(2, Vector3::zero(), rot);
        // When position is zero, rotation is overridden to identity (Deg(0) around Z)
        for inst in &instances {
            // Identity quaternion from axis_angle(z, 0) should be (1, 0, 0, 0)
            assert_relative_eq!(inst.rotation.s, 1.0, epsilon = 1e-6);
            assert_relative_eq!(inst.rotation.v.x, 0.0, epsilon = 1e-6);
            assert_relative_eq!(inst.rotation.v.y, 0.0, epsilon = 1e-6);
            assert_relative_eq!(inst.rotation.v.z, 0.0, epsilon = 1e-6);
        }
    }

    #[test]
    fn uniform_instances_nonzero_position_preserves_rotation() {
        let rot = Quaternion::from_axis_angle(Vector3::unit_y(), Deg(45.0));
        let instances = uniform_instances(2, Vector3::new(1.0, 0.0, 0.0), rot);
        for inst in &instances {
            assert_relative_eq!(inst.rotation.s, rot.s, epsilon = 1e-6);
            assert_relative_eq!(inst.rotation.v.x, rot.v.x, epsilon = 1e-6);
            assert_relative_eq!(inst.rotation.v.y, rot.v.y, epsilon = 1e-6);
            assert_relative_eq!(inst.rotation.v.z, rot.v.z, epsilon = 1e-6);
        }
    }

    #[test]
    fn billboard_faces_camera() {
        use cgmath::Point3;
        let pos = Vector3::new(1.0, 0.0, 2.0);
        for cam in [Point3::new(1.0, 5.0, 10.0), Point3::new(-4.0, 0.0, 2.0), Point3::new(3.0, -1.0, -7.0)] {
            let facing = billboard_rotation(pos, cam) * Vector3::unit_z();
            let to_cam = Vector3::new(cam.x - pos.x, 0.0, cam.z - pos.z).normalize();
            assert_relative_eq!(facing.x, to_cam.x, epsilon = 1e-5);
            assert_relative_eq!(facing.y, 0.0, epsilon = 1e-5);
            assert_relative_eq!(facing.z, to_cam.z, epsilon = 1e-5);
        }
    }

    #[test]
    fn is_far_uses_distance_threshold() {
        use cgmath::Point3;
        let cam = Point3::new(0.0, 0.0, 0.0);
        assert!(!is_far(Vector3::new(3.0, 4.0, 0.0), cam, 5.0));
        assert!(is_far(Vector3::new(3.0, 4.0, 0.1), cam, 5.0));
        assert!(is_far(Vector3::new(0.0, 10.0, 0.0), cam, 5.0));
    }

    #[test]
    fn uniform_instances_near_zero_position_is_not_zero() {
        // is_zero() checks exact equality, not epsilon
        let rot = Quaternion::from_axis_angle(Vector3::unit_y(), Deg(90.0));
        let near_zero = Vector3::new(1e-10, 0.0, 0.0);
        let instances = uniform_instances(1, near_zero, rot);
        // near_zero.is_zero() returns false, so rotation should be preserved
        assert_relative_eq!(instances[0].rotation.s, rot.s, epsilon = 1e-6);
    }
}
