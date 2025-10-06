use wgpu::util::DeviceExt;

use crate::data_structures::{model::{Model, ModelVertex, Vertex}, texture};

pub struct LightResources {
    pub model: Model,
    pub uniform: LightUniform,
    pub buffer: wgpu::Buffer,
    pub render_pipeline: wgpu::RenderPipeline,
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightUniform {
    position: [f32; 3],
    // Due to uniforms requiring 16 byte (4 float) spacing, we need to use a padding field here
    _padding: u32,
    color: [f32; 3],
    // Due to uniforms requiring 16 byte (4 float) spacing, we need to use a padding field here
    _padding2: u32,
}

pub fn mk_buffer(device: &wgpu::Device, light_uniform: LightUniform) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Light Vertex Buffer"),
        contents: bytemuck::cast_slice(&[light_uniform]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    })
}

pub fn mk_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        label: None,
    })
}

pub fn mk_bind_group(device: &wgpu::Device, bind_group_layout: wgpu::BindGroupLayout, light_buffer: wgpu::Buffer) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: light_buffer.as_entire_binding(),
        }],
        label: None,
    })
}

pub fn mk_render_pipeline(device: &wgpu::Device, config: wgpu::SurfaceConfiguration, camera_bind_group_layout: wgpu::BindGroupLayout) -> wgpu::RenderPipeline {
    let light_bind_group_layout = mk_bind_group_layout(device);
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Light Pipeline Layout"),
        bind_group_layouts: &[&camera_bind_group_layout, &light_bind_group_layout],
        push_constant_ranges: &[],
    });
    let shader = wgpu::ShaderModuleDescriptor {
        label: Some("Light Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("light.wgsl").into()),
    };
    crate::pipelines::basic::mk_render_pipeline(
        &device,
        &layout,
        config.format,
        Some(wgpu::BlendState {
            alpha: wgpu::BlendComponent::REPLACE,
            color: wgpu::BlendComponent::REPLACE,
        }),
        Some(texture::Texture::DEPTH_FORMAT),
        &[ModelVertex::desc()],
        shader,
    )
}
