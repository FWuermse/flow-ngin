use cgmath::{Rotation3, Zero};
use crate::{data_structures::{instance::{Instance, InstanceRaw}, model::{self, Vertex}, texture::Texture}, pipelines::basic::mk_render_pipeline, resources::{self, diffuse_normal_layout}};
use wgpu::{util::DeviceExt, BindGroupLayout, Device, Queue, SurfaceConfiguration};

/**
 * A `BuildingBlock` is a one-by-one voxel that uses instancing.
 * 
 * I don't recommend it for building entire voxel worlds similar to Minecraft at
 * this point as "hidden" blocks are still progressing through the pipeline until
 * the depth-buffer.
 */
pub struct BuildingBlocks {
    // TODO: create apis and make fields private
    pub id: u32,
    pub pipeline: wgpu::RenderPipeline,
    pub obj_model: model::Model,
    // TODO: retire this param
    #[allow(dead_code)]
    obj_file: String,
    pub instances: Vec<Instance>,
    pub instance_buffer: wgpu::Buffer,
}

impl BuildingBlocks {
    pub async fn new(
        start_position: cgmath::Vector3<f32>,
        start_rotation: cgmath::Quaternion<f32>,
        amount: u32,
        device: &Device,
        camera_bind_group_layout: &BindGroupLayout,
        light_bind_group_layout: &BindGroupLayout,
        config: &SurfaceConfiguration,
        queue: &Queue,
        obj_file: &str,
    ) -> Self {
        let obj_model = resources::load_model(obj_file, &device, &queue).await;
        if let Err(e) = obj_model {
            panic!("Error failed to load model: {}", e);
        }
        let obj_model = obj_model.unwrap();

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    &diffuse_normal_layout(device),
                    &camera_bind_group_layout,
                    &light_bind_group_layout,
                ],
                push_constant_ranges: &[],
            });

        let shader = wgpu::ShaderModuleDescriptor {
            label: Some("Normal Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("block_shader.wgsl").into()),
        };

        let render_pipeline = mk_render_pipeline(
            device,
            &render_pipeline_layout,
            config.format,
            Some(wgpu::BlendState {
                alpha: wgpu::BlendComponent::REPLACE,
                color: wgpu::BlendComponent::REPLACE,
            }),
            Some(Texture::DEPTH_FORMAT),
            &[model::ModelVertex::desc(), InstanceRaw::desc()],
            shader,
        );

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
            pipeline: render_pipeline,
            obj_model,
            instances,
            obj_file: obj_file.to_string(),
            instance_buffer,
            // Ids may be used later for picking, hitboxes, etc.
            id: 0,
        }
    }
}