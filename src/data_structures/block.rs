use crate::{
    data_structures::{
        instance::{Instance, InstanceRaw},
        model::{self, ModelVertex, Vertex},
        texture::Texture,
    },
    pipelines::basic::mk_render_pipeline,
    resources::{
        self,
        pick::{load_pick_model, pick_render_pipeline_layout, pick_shader},
        texture::diffuse_normal_layout,
    },
};
use cgmath::{One, Rotation3, Zero};
use wgpu::{BindGroupLayout, Device, Queue, SurfaceConfiguration, util::DeviceExt};

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
        let obj_model = resources::load_model_obj(obj_file, &device, &queue).await;
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

    /**
     * This constructor creates `amount` instances all located at (0.0, 0.0, 0.0).
     * 
     * TODO: pass iter fn to choose the transformation
     */
    pub async fn mk_multiple(
        amount: u32,
        device: &Device,
        camera_bind_group_layout: &BindGroupLayout,
        light_bind_group_layout: &BindGroupLayout,
        config: &SurfaceConfiguration,
        queue: &Queue,
        obj_files: &[&'static str],
    ) -> Vec<BuildingBlocks> {
        let mut output = vec![];
        for obj_file in obj_files {
            output.push(
                BuildingBlocks::new(
                    cgmath::Vector3::zero(),
                    cgmath::Quaternion::one(),
                    amount,
                    device,
                    camera_bind_group_layout,
                    light_bind_group_layout,
                    config,
                    queue,
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
        let obj_model = load_pick_model(&device, color, self.obj_model.meshes.clone()).unwrap();

        let render_pipeline_layout = pick_render_pipeline_layout(device, camera_bind_group_layout);

        let shader = pick_shader(device);

        let color_format = wgpu::TextureFormat::R32Uint;

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            cache: None,
            label: Some("Pick Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[model::ModelVertex::desc(), InstanceRaw::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
                polygon_mode: wgpu::PolygonMode::Fill,
                // Requires Features::DEPTH_CLIP_CONTROL
                unclipped_depth: false,
                // Requires Features::CONSERVATIVE_RASTERIZATION
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });
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
    pub fn to_transparent(
        &mut self,
        config: &SurfaceConfiguration,
        device: &Device,
        camera_bind_group_layout: &BindGroupLayout,
        light_bind_group_layout: &BindGroupLayout,
    ) {
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
            source: wgpu::ShaderSource::Wgsl(include_str!("transparent.wgsl").into()),
        };
        self.pipeline = mk_render_pipeline(
            device,
            &render_pipeline_layout,
            config.format,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            Some(Texture::DEPTH_FORMAT),
            &[ModelVertex::desc(), InstanceRaw::desc()],
            shader,
        );
    }
}
