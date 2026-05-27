use bytemuck::{Pod, Zeroable};
use flow_ngin::{
    context::{Context, InitContext},
    data_structures::texture::Texture,
    flow::{GraphicsFlow, Out},
    render::Render,
};
use wgpu::util::DeviceExt;

use crate::{Event, State, Strategy};
use crate::collision_backend::{CollisionBackend, make_hitbox_for};

const LINE_SHADER: &str = r#"
struct Camera {
    view_pos: vec4<f32>,
    view_proj: mat4x4<f32>,
}
@group(0) @binding(0)
var<uniform> camera: Camera;

struct VertexInput {
    @location(0) position: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(0.4, 0.75, 1.0, 0.55);
}
"#;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct LineVertex {
    position: [f32; 3],
}

impl LineVertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<LineVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            }],
        }
    }
}

pub struct PartitionVizFlow {
    pipeline: Option<wgpu::RenderPipeline>,
    vertex_buffer: Option<wgpu::Buffer>,
    num_vertices: u32,
    backend: CollisionBackend,
    cached_strategy: Strategy,
    cached_dims: u8,
    cached_placed_count: usize,
}

impl PartitionVizFlow {
    pub async fn new(_ctx: InitContext) -> Self {
        let backend = CollisionBackend::new(Strategy::SparseGrid, 2);
        Self {
            pipeline: None,
            vertex_buffer: None,
            num_vertices: 0,
            backend,
            cached_strategy: Strategy::SparseGrid,
            cached_dims: 2,
            cached_placed_count: 0,
        }
    }

    fn build_pipeline(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
        sample_count: u32,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Line Shader"),
            source: wgpu::ShaderSource::Wgsl(LINE_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Line Pipeline Layout"),
            bind_group_layouts: &[Some(camera_bind_group_layout)],
            ..Default::default()
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Line Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[LineVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: Texture::DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        })
    }

    fn sync_backend(&mut self, state: &State) -> bool {
        let needs_full_rebuild = state.strategy != self.cached_strategy
            || state.detection_dims != self.cached_dims
            || state.placed.len() < self.cached_placed_count; // objects were cleared

        if needs_full_rebuild {
            self.backend = CollisionBackend::rebuild(state.strategy, state.detection_dims, &state.placed);
            self.cached_strategy = state.strategy;
            self.cached_dims = state.detection_dims;
            self.cached_placed_count = state.placed.len();
            return true;
        }

        if state.placed.len() > self.cached_placed_count {
            // Incrementally insert only newly added tail objects
            for placed in &state.placed[self.cached_placed_count..] {
                self.backend.insert(make_hitbox_for(placed));
            }
            self.cached_placed_count = state.placed.len();
            return true;
        }

        false
    }

    fn update_lines(&mut self, state: &State, device: &wgpu::Device) {
        let lines = self.backend.partition_lines(state.detection_dims);

        let vertices: Vec<LineVertex> = lines
            .iter()
            .flat_map(|[a, b]| {
                [
                    LineVertex { position: [a.x, a.y, a.z] },
                    LineVertex { position: [b.x, b.y, b.z] },
                ]
            })
            .collect();

        self.num_vertices = vertices.len() as u32;

        if vertices.is_empty() {
            self.vertex_buffer = None;
            return;
        }

        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Line Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        self.vertex_buffer = Some(buf);
    }
}

impl GraphicsFlow<State, Event> for PartitionVizFlow {
    fn on_init(&mut self, ctx: &mut Context, state: &mut State) -> Out<State, Event> {
        let sample_count = ctx.anti_aliasing.sample_count();
        self.pipeline = Some(Self::build_pipeline(
            &ctx.device,
            &ctx.config,
            &ctx.camera.bind_group_layout,
            sample_count,
        ));
        // Sync backend against real initial State, then generate lines once.
        self.sync_backend(state);
        self.update_lines(state, &ctx.device);
        Out::Empty
    }

    fn on_update(
        &mut self,
        ctx: &Context,
        state: &mut State,
        _dt: std::time::Duration,
    ) -> Out<State, Event> {
        if self.sync_backend(state) {
            self.update_lines(state, &ctx.device);
        }
        Out::Empty
    }

    fn on_custom_events(
        &mut self,
        _ctx: &Context,
        _state: &mut State,
        event: Event,
    ) -> Option<Event> {
        Some(event)
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        let Some(pipeline) = &self.pipeline else {
            return Render::None;
        };
        let Some(vertex_buffer) = &self.vertex_buffer else {
            return Render::None;
        };
        if self.num_vertices == 0 {
            return Render::None;
        }
        let num = self.num_vertices;
        Render::Custom(Box::new(move |ctx: &Context, pass: &mut wgpu::RenderPass<'pass>| {
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &ctx.camera.bind_group, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.draw(0..num, 0..1);
        }))
    }
}
