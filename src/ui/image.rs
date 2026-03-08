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

/// NDC-space rectangle [start_x, end_x] x [end_y, start_y] (x left→right, y bottom→top).
#[derive(Clone, Copy)]
pub struct Frame {
    pub start_x: f32,
    /// Top edge in NDC (larger y value).
    pub start_y: f32,
    pub end_x: f32,
    /// Bottom edge in NDC (smaller y value).
    pub end_y: f32,
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
    pub width_px: u32,
    pub height_px: u32,
    x_px: u32,
    y_px: u32,
    screen_width: u32,
    screen_height: u32,
    pub screen_pos: Frame,
    tex_coords: Frame,
    resources: ImageResources,
}

/// Convert a pixel-space rectangle to an NDC Frame.
///
/// Pixel origin is top-left; NDC origin is center with y pointing up.
fn pixels_to_ndc(x_px: u32, y_px: u32, width_px: u32, height_px: u32, screen_width: u32, screen_height: u32) -> Frame {
    let sw = screen_width as f32;
    let sh = screen_height as f32;
    let left  = -1.0 + 2.0 * x_px as f32 / sw;
    let top   =  1.0 - 2.0 * y_px as f32 / sh;
    Frame {
        start_x: left,
        start_y: top,
        end_x:   left + 2.0 * width_px  as f32 / sw,
        end_y:   top  - 2.0 * height_px as f32 / sh,
    }
}

impl Icon {
    /// Create a new icon from an atlas slot.
    ///
    /// `(x_px, y_px)` is the top-left pixel position on screen.
    /// `(width_px, height_px)` is the desired size in pixels.
    pub fn new(
        ctx: &Context,
        atlas: Arc<Atlas>,
        id: u32,
        slot: u8,
        x_px: u32,
        y_px: u32,
        width_px: u32,
        height_px: u32,
    ) -> Self {
        let screen_width  = ctx.config.width;
        let screen_height = ctx.config.height;
        let screen_pos = pixels_to_ndc(x_px, y_px, width_px, height_px, screen_width, screen_height);

        let Some(tex_coords) = atlas.to_tex_coords(slot) else {
            panic!("Texture coordinates overflowed when calculating UI for slot {slot}")
        };

        let vertices = vertices_from_coords(&screen_pos, &tex_coords);
        let vertex_buffer = ctx.device.create_buffer_init(&BufferInitDescriptor {
            label: Some(&format!("Icon Vertex Buffer {}", id)),
            contents: bytemuck::cast_slice(&vertices),
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        });
        let indices: &[u16] = &[0, 1, 3, 1, 2, 3];
        let index_buffer = ctx.device.create_buffer_init(&BufferInitDescriptor {
            label: Some(&format!("Icon Index Buffer {}", id)),
            contents: bytemuck::cast_slice(indices),
            usage: BufferUsages::INDEX,
        });
        let num_indices = indices.len();

        Self {
            id,
            width_px,
            height_px,
            x_px,
            y_px,
            screen_width,
            screen_height,
            screen_pos,
            tex_coords,
            resources: ImageResources {
                num_indices,
                atlas,
                vertex_buffer,
                index_buffer,
            },
        }
    }

    /// Reposition the icon to `(x_px, y_px)` and upload the new vertices.
    ///
    /// Intended to be called by containers that manage this icon's layout.
    pub fn set_position(&mut self, x_px: u32, y_px: u32, queue: &wgpu::Queue) {
        self.x_px = x_px;
        self.y_px = y_px;
        self.screen_pos = pixels_to_ndc(x_px, y_px, self.width_px, self.height_px, self.screen_width, self.screen_height);
        let vertices = vertices_from_coords(&self.screen_pos, &self.tex_coords);
        queue.write_buffer(&self.resources.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
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

    // TODO: custom resize mechanism or custom event for resizing or re-use of resize window event.
}
