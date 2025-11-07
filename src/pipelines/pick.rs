use wgpu::{BindGroupLayout, PipelineLayout, ShaderModule};

use crate::{data_structures::{
    instance::InstanceRaw,
    model::{self, Vertex},
}, resources::pick::pick_layout};

fn pick_render_pipeline_layout(
    device: &wgpu::Device,
    camera_bind_group_layout: &BindGroupLayout,
) -> PipelineLayout {
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Render Pipeline Layout (For picking)"),
        bind_group_layouts: &[&pick_layout(device), &camera_bind_group_layout],
        push_constant_ranges: &[],
    })
}

fn pick_shader(device: &wgpu::Device) -> ShaderModule {
    let shader = wgpu::ShaderModuleDescriptor {
        label: Some("Normal Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("pick_basic.wgsl").into()),
    };
    device.create_shader_module(shader)
}

pub fn mk_pick_pipeline(
    device: &wgpu::Device,
    camera_bind_group_layout: &BindGroupLayout,
) -> wgpu::RenderPipeline {
    let render_pipeline_layout = pick_render_pipeline_layout(device, camera_bind_group_layout);

    let shader = pick_shader(device);

    let color_format = wgpu::TextureFormat::R32Uint;
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
    })
}
