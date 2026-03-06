use std::{sync::Arc, u16};

use cgmath::num_traits::ToPrimitive;
use wgpu::{
    BufferUsages,
    util::{BufferInitDescriptor, DeviceExt},
};

use crate::{
    context::Context,
    flow::GraphicsFlow,
    pipelines::gui::{Vertex, mk_bind_group, mk_bind_group_layout},
    render::{Flat, Render},
    resources::texture::load_texture,
};

struct ImageResources {
    num_indices: usize,
    // This is an `Arc` to simplify the interface for a user of the lib (avoid handling lifetimes for a shared atlas)
    atlas: Arc<Atlas>,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
}

pub struct Frame {
    start_x: f32,
    start_y: f32,
    end_x: f32,
    end_y: f32,
}

pub struct Atlas {
    bind_group: wgpu::BindGroup,
    h_grids: u8,
    v_grids: u8,
}
impl Atlas {
    pub async fn new(device: &wgpu::Device, queue: &wgpu::Queue, file_name: &str, h_grids: u8, v_grids: u8) -> Self {
            let atlas = load_texture(file_name, false, device, queue, None)
                .await
                .unwrap();
            let texture_bind_group_layout = mk_bind_group_layout(device);
            let bind_group = mk_bind_group(device, &atlas, &texture_bind_group_layout);
            Atlas {
                bind_group,
                h_grids,
                v_grids,
            }
    }
    fn to_tex_coords(&self, slot: u8) -> Option<Frame> {
        let row = slot % self.h_grids;
        let col = slot / self.h_grids;
        let row_len = 1.0 / self.h_grids.to_f32()?;
        let col_len = 1.0 / self.v_grids.to_f32()?;
        let frame = Frame {
            start_x: row.to_f32()? * row_len,
            start_y: col.to_f32()? * col_len,
            end_x: (row + 1).to_f32()? * row_len,
            end_y: (col + 1).to_f32()? * col_len,
        };
        Some(frame)
    }
}

pub struct Icon {
    id: u32,
    width: f32,
    height: f32,
    enabled: bool,
    screen_pos: Frame,
    resources: ImageResources,
}

impl Icon {
    pub fn new(
        ctx: &Context,
        atlas: Arc<Atlas>,
        id: u32,
        slot: u8,
        width: u32,
        height: u32,
    ) -> Self {
        let pixel_width = 1.0 / ctx.config.width.to_f32().expect("Screen size too large");
        let pixel_height = 1.0 / ctx.config.height.to_f32().expect("Screen size too large");
        let width = pixel_width * width.to_f32().unwrap();
        let height = pixel_height * height.to_f32().unwrap();
        let screen_pos = Frame {
            start_x: 1.0,
            start_y: 1.0,
            end_x: 0.0,
            end_y: 0.0,
        };
        let Some(tex_coords) = atlas.to_tex_coords(slot) else {
            panic!("Texture coordinates overflowed when calculating UII")
        };

        let vertices = vertices_from_coords(&screen_pos, &tex_coords);
        let vertex_buffer = ctx.device.create_buffer_init(&BufferInitDescriptor {
            label: Some(&format!("Button Vertex Buffer {}", id)),
            contents: bytemuck::cast_slice(&vertices),
            usage: BufferUsages::VERTEX,
        });
        let indices: &[u16] = &[0, 1, 3, 1, 2, 3];
        let index_buffer = ctx.device.create_buffer_init(&BufferInitDescriptor {
            label: Some(&format!("Icon UI Element Index Buffer {}", id)),
            contents: bytemuck::cast_slice(&indices),
            usage: BufferUsages::INDEX,
        });
        let num_indices = indices.len();

        Self {
            id,
            width,
            height,
            enabled: false,
            screen_pos,
            resources: ImageResources {
                num_indices,
                atlas,
                vertex_buffer,
                index_buffer,
            },
        }
    }
}

fn vertices_from_coords(screen_pos: &Frame, tex_coords: &Frame) -> Vec<Vertex> {
    vec![
        Vertex {
            position: [screen_pos.start_x, screen_pos.end_y, 0.0],
            tex_coords: [tex_coords.start_x, tex_coords.end_y],
        },
        Vertex {
            position: [screen_pos.end_x, screen_pos.end_y, 0.0],
            tex_coords: [tex_coords.end_x, tex_coords.end_y],
        },
        Vertex {
            position: [screen_pos.end_x, screen_pos.start_y, 0.0],
            tex_coords: [tex_coords.end_x, tex_coords.start_y],
        },
        Vertex {
            position: [screen_pos.start_x, screen_pos.start_y, 0.0],
            tex_coords: [tex_coords.start_x, tex_coords.start_y],
        },
    ]
}

impl<S, E> GraphicsFlow<S, E> for Icon {
    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        Render::GUI(Flat {
            vertex: &self.resources.vertex_buffer,
            index: &self.resources.index_buffer,
            group: &self.resources.atlas.bind_group,
            amount: self.resources.num_indices,
            id: self.id,
        })
    }

    // TODO: custom rezie machanism or custom event for rezising or re-use of resize window event.
}
