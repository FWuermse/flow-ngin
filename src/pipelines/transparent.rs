use crate::{data_structures::{instance::InstanceRaw, model::{ModelVertex, Vertex}, texture::Texture}, pipelines::basic::mk_render_pipeline, resources::texture::diffuse_normal_layout};

/**
 * Sets a new pipeline for a BuildingBlock that makes it transparent.
 *
 * This includes all textures wrapped around a mesh regardless of whether they
 * had already partially set to a transparency value lower than `1.0`.
 *
 * TODO: use the basic pipeline and configure transparency via unform buffer.
 * It's overkill to set a new pipeline just for that.
 */
pub fn mk_transparent_pipeline(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
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
        source: wgpu::ShaderSource::Wgsl(include_str!("transparent.wgsl").into()),
    };
    mk_render_pipeline(
        &device,
        &render_pipeline_layout,
        config.format,
        Some(wgpu::BlendState::ALPHA_BLENDING),
        Some(Texture::DEPTH_FORMAT),
        &[ModelVertex::desc(), InstanceRaw::desc()],
        shader,
    )
}
