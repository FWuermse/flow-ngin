use std::time::Duration;

use bytemuck::{Pod, Zeroable};
use cgmath::Vector3;
use flow_ngin::{
    context::{Context, InitContext},
    flow::{GraphicsFlow, Out},
    render::Render,
};
use wgpu::util::DeviceExt;

use crate::{Event, State, collision_manager::Strategy};

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct LineVertex {
    position: [f32; 3],
}

impl LineVertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            }],
        }
    }
}

const LINE_SHADER: &str = r#"
struct Camera {
    view_pos: vec4<f32>,
    view_proj: mat4x4<f32>,
}
@group(0) @binding(0) var<uniform> camera: Camera;

@vertex
fn vs_main(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
    return camera.view_proj * vec4<f32>(pos, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(0.4, 0.8, 1.0, 0.5);
}
"#;

pub struct PartitionVizFlow {
    pipeline: Option<wgpu::RenderPipeline>,
    vertex_buf: Option<wgpu::Buffer>,
    vertex_count: u32,
    last_strategy: Option<Strategy>,
    last_placed_count: usize,
}

impl PartitionVizFlow {
    pub fn new(_ctx: InitContext) -> Self {
        Self {
            pipeline: None,
            vertex_buf: None,
            vertex_count: 0,
            last_strategy: None,
            last_placed_count: 0,
        }
    }

    fn build_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat, msaa: u32) -> wgpu::RenderPipeline {
        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("line_camera_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("line_pipeline_layout"),
            bind_group_layouts: &[Some(&camera_bind_group_layout)],
            ..Default::default()
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("line_shader"),
            source: wgpu::ShaderSource::Wgsl(LINE_SHADER.into()),
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("line_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[LineVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: msaa,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        })
    }

    fn rebuild_buffer(&mut self, device: &wgpu::Device, lines: &[[Vector3<f32>; 2]]) {
        let vertices: Vec<LineVertex> = lines
            .iter()
            .flat_map(|[a, b]| {
                [
                    LineVertex { position: [a.x, a.y, a.z] },
                    LineVertex { position: [b.x, b.y, b.z] },
                ]
            })
            .collect();

        self.vertex_count = vertices.len() as u32;

        if vertices.is_empty() {
            self.vertex_buf = None;
            return;
        }

        self.vertex_buf = Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("line_vertex_buf"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        }));
    }
}

impl GraphicsFlow<State, Event> for PartitionVizFlow {
    fn on_init(&mut self, ctx: &mut Context, state: &mut State) -> Out<State, Event> {
        let msaa = ctx.anti_aliasing.sample_count();
        self.pipeline = Some(Self::build_pipeline(&ctx.device, ctx.config.format, msaa));

        let lines = state
            .collision_backend
            .as_ref()
            .map(|b| b.partition_lines())
            .unwrap_or_default();
        self.rebuild_buffer(&ctx.device, &lines);
        self.last_strategy = Some(state.strategy);

        Out::Empty
    }

    fn on_update(&mut self, ctx: &Context, state: &mut State, _dt: Duration) -> Out<State, Event> {
        let strategy_changed = self.last_strategy != Some(state.strategy);
        let placed_changed = self.last_placed_count != state.placed_objects.len();

        if strategy_changed || placed_changed {
            self.last_strategy = Some(state.strategy);
            self.last_placed_count = state.placed_objects.len();
            let lines = state
                .collision_backend
                .as_ref()
                .map(|b| b.partition_lines())
                .unwrap_or_default();
            self.rebuild_buffer(&ctx.device, &lines);
        }
        Out::Empty
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        let (Some(pipeline), Some(vbuf)) = (&self.pipeline, &self.vertex_buf) else {
            return Render::None;
        };
        if self.vertex_count == 0 {
            return Render::None;
        }

        let pipeline = pipeline.clone();
        let vbuf = vbuf.clone();
        let vc = self.vertex_count;

        Render::Custom(Box::new(move |ctx, pass| {
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &ctx.camera.bind_group, &[]);
            pass.set_vertex_buffer(0, vbuf.slice(..));
            pass.draw(0..vc, 0..1);
        }))
    }
}
