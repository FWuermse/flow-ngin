use crate::{data_structures::model, resources::diffuse_normal_layout};

use wgpu::{
    BindGroupLayout, PipelineLayout, ShaderModule, util::DeviceExt,
};

pub fn pick_render_pipeline_layout(
    device: &wgpu::Device,
    camera_bind_group_layout: &BindGroupLayout,
) -> PipelineLayout {
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Render Pipeline Layout (For picking)"),
        bind_group_layouts: &[&diffuse_normal_layout(device), &camera_bind_group_layout],
        push_constant_ranges: &[],
    })
}

pub fn pick_shader(device: &wgpu::Device) -> ShaderModule {
    let shader = wgpu::ShaderModuleDescriptor {
        label: Some("Normal Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("pick.wgsl").into()),
    };
    device.create_shader_module(shader)
}

/**
 * This is a representation of a Model which uses a uniform ID buffer instead of texture RGBA values
 * to render different objects. When backtracking the ID that's output from the fragment shader
 * we can do pixel-perfect picking which is essential for detecting clicks on UI-elements or meshes.
 */
pub fn load_pick_model(
    device: &wgpu::Device,
    color: u32,
    meshes: Vec<model::Mesh>,
) -> anyhow::Result<model::Model> {
    let r = color as u8;
    let g = (color >> 8) as u8;
    let b = (color >> 16) as u8;
    let a = (color >> 24) as u8;
    // Current browsers don't support downscaling Uniform Buffers so I have to provide the full 16B
    let mut buf = [0; 16];
    buf[..4].copy_from_slice(&[r, g, b, a]);
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Pick color buffer"),
        contents: bytemuck::cast_slice(&buf),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let materials = vec![model::Material::new_pick_material(
        device,
        &"Pick Material",
        buffer,
    )];

    let model = model::Model { meshes, materials };
    Ok(model)
}
