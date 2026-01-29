//! Basic phong/standard lighting pipeline with diffuse and normal map support.
//!
//! This is the primary pipeline for rendering opaque objects. It combines
//! diffuse textures and normal maps for per-pixel lighting calculations.

use crate::{data_structures::{instance::InstanceRaw, model::{self, Vertex}, texture::Texture}, resources::texture::diffuse_normal_layout};

/// Create the basic lighting pipeline for opaque 3D objects.
///
/// The basic pipeline renders models with phong/standard lighting, supporting
/// both diffuse textures and normal maps.
pub fn mk_basic_pipeline(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
    direction: wgpu::FrontFace,
    light_bind_group_layout: &wgpu::BindGroupLayout,
    camera_bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let render_pipeline_layout =
        device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    &diffuse_normal_layout(&device),
                    &camera_bind_group_layout,
                    &light_bind_group_layout,
                ],
                push_constant_ranges: &[],
            });

    let shader = wgpu::ShaderModuleDescriptor {
        label: Some("Normal Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("block_shader.wgsl").into()),
    };

    mk_render_pipeline(
        &device,
        direction,
        &render_pipeline_layout,
        config.format,
        Some(wgpu::BlendState {
            alpha: wgpu::BlendComponent::REPLACE,
            color: wgpu::BlendComponent::REPLACE,
        }),
        Some(Texture::DEPTH_FORMAT),
        &[model::ModelVertex::desc(), InstanceRaw::desc()],
        shader,
    )
}

/// Generic helper to create a render pipeline with custom layout and shaders.
///
/// Handles boilerplate for creating WGPU render pipelines.
pub fn mk_render_pipeline(
    device: &wgpu::Device,
    front_face: wgpu::FrontFace,
    layout: &wgpu::PipelineLayout,
    color_format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
    depth_format: Option<wgpu::TextureFormat>,
    vertex_layouts: &[wgpu::VertexBufferLayout],
    shader: wgpu::ShaderModuleDescriptor,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(shader);

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        cache: None,
        label: Some("Basic Pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: vertex_layouts,
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face,
            cull_mode: Some(wgpu::Face::Back),
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: depth_format.map(|format| wgpu::DepthStencilState {
            format,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        // maybe useful for multiple textures on a mesh.
        multiview: None,
    })
}
