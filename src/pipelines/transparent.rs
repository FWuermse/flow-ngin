use crate::{data_structures::{instance::InstanceRaw, model::{ModelVertex, Vertex}, texture::Texture}, pipelines::basic::mk_render_pipeline, resources::texture::diffuse_normal_layout};

/// Per-object transparency parameters sent to the transparent fragment shader.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TransparencyUniform {
    /// RGB tint, each channel in `0.0..=1.0`. Replaces the object's texture hue.
    pub tint: [f32; 3],
    /// Opacity in `0.0..=1.0` (0 = fully transparent, 1 = fully opaque).
    pub alpha: f32,
}

impl Default for TransparencyUniform {
    fn default() -> Self {
        Self {
            tint: [1.0, 1.0, 1.0],
            alpha: 0.4,
        }
    }
}

/// Bind group layout for the per-object transparency uniform.
pub fn mk_transparency_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
        label: Some("transparency_bind_group_layout"),
    })
}

/// Bind group wrapping a `TransparencyUniform` buffer.
pub fn mk_transparency_bind_group(
    device: &wgpu::Device,
    buffer: &wgpu::Buffer,
    layout: &wgpu::BindGroupLayout,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
        label: Some("transparency_bind_group"),
    })
}

/**
 * Sets a new pipeline for a BuildingBlock that makes it transparent.
 *
 * This includes all textures wrapped around a mesh regardless of whether they
 * had already partially set to a transparency value lower than `1.0`.
 *
 * The alpha and RGB tint are supplied per object via the
 * transparency uniform (see [`TransparencyUniform`]).
 */
pub fn mk_transparent_pipeline(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
    light_bind_group_layout: &wgpu::BindGroupLayout,
    camera_bind_group_layout: &wgpu::BindGroupLayout,
    sample_count: u32,
) -> wgpu::RenderPipeline {
    let render_pipeline_layout =
        device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    Some(&diffuse_normal_layout(&device)),
                    Some(&camera_bind_group_layout),
                    Some(&light_bind_group_layout),
                    Some(&mk_transparency_bind_group_layout(&device)),
                ],
                ..Default::default()
            });
    let shader = wgpu::ShaderModuleDescriptor {
        label: Some("Normal Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("transparent.wgsl").into()),
    };
    mk_render_pipeline(
        &device,
        wgpu::FrontFace::Ccw,
        &render_pipeline_layout,
        config.format,
        Some(wgpu::BlendState::ALPHA_BLENDING),
        Some(Texture::DEPTH_FORMAT),
        &[ModelVertex::desc(), InstanceRaw::desc()],
        shader,
        sample_count,
    )
}
