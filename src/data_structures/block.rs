use crate::{
    context::Context,
    data_structures::{
        instance::{Instance, InstanceRaw},
        model::{self, ModelVertex, Vertex},
        texture::Texture,
    },
    pipelines::{basic::mk_render_pipeline, pick},
    resources::{self, pick::load_pick_model, texture::diffuse_normal_layout},
};
use cgmath::{One, Rotation3, Zero};
use wgpu::{BindGroupLayout, Device, util::DeviceExt};

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
        ctx: &Context,
        start_position: cgmath::Vector3<f32>,
        start_rotation: cgmath::Quaternion<f32>,
        amount: u32,
        obj_file: &str,
    ) -> Self {
        let obj_model = resources::load_model_obj(obj_file, &ctx.device, &ctx.queue).await;
        if let Err(e) = obj_model {
            panic!("Error failed to load model: {}", e);
        }
        let obj_model = obj_model.unwrap();

        let render_pipeline_layout =
            ctx.device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Render Pipeline Layout"),
                    bind_group_layouts: &[
                        &diffuse_normal_layout(&ctx.device),
                        &ctx.camera.bind_group_layout,
                        &ctx.light.bind_group_layout,
                    ],
                    push_constant_ranges: &[],
                });

        let shader = wgpu::ShaderModuleDescriptor {
            label: Some("Normal Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("block_shader.wgsl").into()),
        };

        let render_pipeline = mk_render_pipeline(
            &ctx.device,
            &render_pipeline_layout,
            ctx.config.format,
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
        let instance_buffer = ctx
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
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

    /**
     * This constructor creates `amount` instances all located at (0.0, 0.0, 0.0).
     *
     * TODO: pass iter fn to choose the transformation
     */
    pub async fn mk_multiple(
        ctx: &Context,
        amount: u32,
        obj_files: &[&'static str],
    ) -> Vec<BuildingBlocks> {
        let mut output = vec![];
        for obj_file in obj_files {
            output.push(
                BuildingBlocks::new(
                    ctx,
                    cgmath::Vector3::zero(),
                    cgmath::Quaternion::one(),
                    amount,
                    obj_file,
                )
                .await,
            );
        }
        output
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
    pub fn to_clickable(
        &self,
        device: &Device,
        camera_bind_group_layout: &BindGroupLayout,
        color: u32,
    ) -> Self {
        let obj_model = load_pick_model(device, color, self.obj_model.meshes.clone()).unwrap();

        let render_pipeline = pick::mk_render_pipeline(device, camera_bind_group_layout);
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
            pipeline: render_pipeline,
            obj_model: obj_model,
            obj_file: self.obj_file.clone(),
            instances: self.instances.clone(),
            instance_buffer,
            id: color,
        }
    }

    /**
     * Sets a new pipeline for a BuildingBlock that makes it transparent.
     *
     * This includes all textures wrapped around a mesh regardless of whether they
     * had already partially set to a transparency value lower than `1.0`.
     *
     * TODO: use the basic pipeline and configure transparency via unform buffer.
     * It's overkill to set a new pipeline just for that.
     */
    pub fn to_transparent(&mut self, ctx: &Context) {
        let render_pipeline_layout =
            ctx.device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Render Pipeline Layout"),
                    bind_group_layouts: &[
                        &diffuse_normal_layout(&ctx.device),
                        &ctx.camera.bind_group_layout,
                        &ctx.light.bind_group_layout,
                    ],
                    push_constant_ranges: &[],
                });
        let shader = wgpu::ShaderModuleDescriptor {
            label: Some("Normal Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("transparent.wgsl").into()),
        };
        self.pipeline = mk_render_pipeline(
            &ctx.device,
            &render_pipeline_layout,
            ctx.config.format,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            Some(Texture::DEPTH_FORMAT),
            &[ModelVertex::desc(), InstanceRaw::desc()],
            shader,
        );
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

    pub fn write_to_buffer(&self, ctx: &Context) {
        let raws = self
            .instances
            .iter()
            .map(Instance::to_raw)
            .collect::<Vec<_>>();
        // TODO: track whether size changed 
        ctx.queue
            .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&raws));
    }
}
