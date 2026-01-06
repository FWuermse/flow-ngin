//! Building blocks implemented via GPU instancing.
//!
//! Provides [`BuildingBlocks`], a collection of identically-shaped objects
//! (e.g., construction blocks or crowds) rendered efficiently using GPU instancing. Note that
//! hidden blocks are not culled, so this may not be optimal for large voxel worlds.

use crate::{
    context::{Context, GPUResource},
    data_structures::{
        instance::Instance,
        model::{self},
    },
    render::{Instanced, Render},
    resources::{self, pick::load_pick_model},
};
use cgmath::{One, Rotation3, Zero};
use wgpu::{Device, util::DeviceExt};

/// A collection of identically-shaped building blocks.
///
/// Uses GPU instancing to efficiently render many copies of the same model
/// with different transformations. Currently does not perform frustum culling
/// or occlusion culling, so performance may degrade with very large numbers of blocks.
pub struct BuildingBlocks {
    // TODO: create apis and make fields private
    pub id: u32,
    pub obj_model: model::Model,
    // TODO: retire this param
    #[allow(dead_code)]
    obj_file: String,
    instances: Vec<Instance>,
    instance_buffer: wgpu::Buffer,
    buffer_size_needs_change: bool,
}

impl AsRef<BuildingBlocks> for BuildingBlocks {
    fn as_ref(&self) -> &BuildingBlocks {
        self
    }
}

impl BuildingBlocks {
    pub async fn new(
        #[allow(unused)] id: u32,
        queue: &wgpu::Queue,
        device: &wgpu::Device,
        start_position: cgmath::Vector3<f32>,
        start_rotation: cgmath::Quaternion<f32>,
        amount: u32,
        obj_file: &str,
    ) -> Self {
        let obj_model = resources::load_model_obj(obj_file, &device, &queue).await;
        if let Err(e) = obj_model {
            panic!("Error failed to load model: {}", e);
        }
        let obj_model = obj_model.unwrap();

        let instances = (0..amount)
            .map(|_| {
                let mut instance = Instance::new();
                instance.position = start_position;
                let rotation = if start_position.is_zero() {
                    cgmath::Quaternion::from_axis_angle(cgmath::Vector3::unit_z(), cgmath::Deg(0.0))
                } else {
                    start_rotation
                };
                instance.rotation = rotation;
                instance
            })
            .collect::<Vec<_>>();

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
            id: 0,
            buffer_size_needs_change: false,
        }
    }

    /// Returns an immutable reference to instances
    pub fn instances(&self) -> &Vec<Instance> {
        &self.instances
    }

    pub fn add_instance(&mut self, instance: Instance) {
        self.instances.push(instance);
        self.buffer_size_needs_change = true;
    }

    pub fn add_instances(&mut self, mut instances: Vec<Instance>) {
        self.instances.append(&mut instances);
        self.buffer_size_needs_change = true;
    }

    /**
     * This constructor creates `amount` instances all located at (0.0, 0.0, 0.0).
     *
     * TODO: pass iter fn to choose the transformation
     */
    pub async fn mk_multiple(
        queue: &wgpu::Queue,
        device: &wgpu::Device,
        amount: u32,
        descr: &[(u32, &'static str)],
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
    pub fn to_clickable(&self, device: &Device, color: u32) -> Self {
        let obj_model = load_pick_model(device, color, self.obj_model.meshes.clone()).unwrap();

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
            id: color,
            buffer_size_needs_change: false,
        }
    }

    pub fn clear_first(&mut self, device: &Device, amount: usize) {
        self.instances.drain(0..amount);
        let instance_data = self
            .instances
            .iter()
            .map(Instance::to_raw)
            .collect::<Vec<_>>();
        self.instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Instance Buffer"),
            contents: bytemuck::cast_slice(&instance_data),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
    }
}

impl<'a, 'pass> GPUResource<'a, 'pass> for BuildingBlocks {
    fn write_to_buffer(&mut self, ctx: &Context) {
        let raws = self
            .instances
            .iter()
            .map(Instance::to_raw)
            .collect::<Vec<_>>();
        ctx.queue
            .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&raws));

        if self.buffer_size_needs_change {
            self.instance_buffer =
                ctx.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Instance Buffer"),
                        contents: bytemuck::cast_slice(&raws),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    });
            self.buffer_size_needs_change = false;
        } else {
            ctx.queue
                .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&raws));
        }
    }

    fn get_render(&'a self) -> Render<'a, 'pass> {
        Render::Default(Instanced {
            instance: &self.instance_buffer,
            model: &self.obj_model,
            amount: self.instances.len(),
            id: self.id,
        })
    }
}
