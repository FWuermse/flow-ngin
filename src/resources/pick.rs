use crate::{data_structures::model, pipelines::pick_gui::mk_bind_group_layout};

use wgpu::util::DeviceExt;

pub(crate) fn pick_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
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
        label: Some("pick_bind_group_layout"),
    })
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
    // cutting the significant bits is intended in this conversion
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
    let max_idx = meshes.iter().map(|m| m.material).max().unwrap_or(0);

    // We just do this to keep the API consistant. The pick material is just an ID stretched over the mesh
    let materials = (0..max_idx + 1)
        .map(|_| model::Material::new_pick_material(device, &"Pick Material", buffer.clone()))
        .collect();

    let model = model::Model { meshes, materials };
    Ok(model)
}

pub fn load_pick_texture(id: u32, device: &wgpu::Device) -> wgpu::BindGroup {
    let texture_bind_group_layout = mk_bind_group_layout(device);
    let color = id;
    // cutting the significant bits is intended in this conversion
    let r = color as u8;
    let g = (color >> 8) as u8;
    let b = (color >> 16) as u8;
    let a = (color >> 24) as u8;
    let mut buf = [0; 16];
    buf[..4].copy_from_slice(&[r, g, b, a]);
    let pick_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Pick color buffer"),
        contents: bytemuck::cast_slice(&buf),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &texture_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &pick_buffer,
                    offset: 0,
                    size: None,
                }),
            },
        ],
        label: Some("GUI pick bind_group"),
    })
}
