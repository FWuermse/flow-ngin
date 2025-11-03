use wgpu::util::DeviceExt;

use crate::data_structures::{
    model::{Model, ModelVertex, Vertex},
    texture,
};

#[derive(Debug)]
pub struct LightResources {
    pub model: Option<Model>,
    pub uniform: LightUniform,
    pub buffer: wgpu::Buffer,
    pub render_pipeline: wgpu::RenderPipeline,
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl LightResources {
    pub fn new(
        light_uniform: LightUniform,
        model: Option<Model>,
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        camera: &wgpu::BindGroupLayout,
    ) -> Self {
        let light_buffer = mk_buffer(&device, light_uniform);
        let light_bind_group_layout = mk_bind_group_layout(&device);
        let light_bind_group = mk_bind_group(
            &device,
            &light_bind_group_layout,
            light_buffer.as_entire_binding(),
        );
        let light_render_pipeline =
            mk_render_pipeline(&device, &config, &light_bind_group_layout, &camera);

        Self {
            model,
            uniform: light_uniform,
            buffer: light_buffer,
            render_pipeline: light_render_pipeline,
            bind_group: light_bind_group,
            bind_group_layout: light_bind_group_layout.clone(),
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightUniform {
    // TODO: make private and create nicer API for light sources
    pub position: [f32; 3],
    // Due to uniforms requiring 16 byte (4 float) spacing, we need to use a padding field here
    pub _padding: u32,
    pub color: [f32; 3],
    // Due to uniforms requiring 16 byte (4 float) spacing, we need to use a padding field here
    pub _padding2: u32,
}

fn mk_buffer(device: &wgpu::Device, light_uniform: LightUniform) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Light Vertex Buffer"),
        contents: bytemuck::cast_slice(&[light_uniform]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    })
}

fn mk_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
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

fn mk_bind_group(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    light_buffer: wgpu::BindingResource<'_>,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: light_buffer,
        }],
        label: None,
    })
}

fn mk_render_pipeline(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
    light_bind_group_layout: &wgpu::BindGroupLayout,
    camera_bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Light Pipeline Layout"),
        bind_group_layouts: &[camera_bind_group_layout, light_bind_group_layout],
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
