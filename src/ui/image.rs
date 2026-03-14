use std::{sync::Arc, u16};

use cgmath::num_traits::ToPrimitive;
use wgpu::{
    BufferUsages, Color,
    util::{BufferInitDescriptor, DeviceExt},
};

use crate::{
    context::Context,
    data_structures::texture::Texture,
    flow::GraphicsFlow,
    pipelines::gui::{Vertex, mk_bind_group, mk_bind_group_layout},
    render::{Flat, Render},
    resources::texture::load_texture,
    ui::{Placement, layout::Layout},
};

pub struct ImageResources {
    num_indices: usize,
    // This is an `Arc` to simplify the interface for a user of the lib (avoid handling lifetimes for a shared atlas)
    atlas: Arc<Atlas>,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
}

struct ColorResources {
    num_indices: usize,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

enum Resources {
    Image(ImageResources),
    Color(ColorResources),
}

/// NDC-space rectangle [start_x, end_x] x [end_y, start_y] (x left→right, y bottom→top).
#[derive(Clone, Copy)]
pub struct Frame {
    pub start_x: f32,
    pub start_y: f32,
    pub end_x: f32,
    pub end_y: f32,
}

pub struct Atlas {
    bind_group: wgpu::BindGroup,
    h_grids: u8,
    v_grids: u8,
}
impl Atlas {
    pub async fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        file_name: &str,
        h_grids: u8,
        v_grids: u8,
    ) -> Self {
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

#[derive(Clone, Copy, Debug, Default)]
pub enum HAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum VAlign {
    #[default]
    Top,
    Center,
    Bottom,
}

pub struct Icon {
    id: u32,
    pub width_px: u32,
    pub height_px: u32,
    pub placement: Placement,
    screen_width: u32,
    screen_height: u32,
    screen_pos: Frame,
    tex_coords: Frame,
    resources: Resources,
}

/// Convert a pixel-space rectangle to an NDC Frame.
///
/// Pixel origin is top-left; NDC origin is center with y pointing up.
pub(crate) fn pixels_to_ndc(
    x_px: u32,
    y_px: u32,
    width_px: u32,
    height_px: u32,
    screen_width: u32,
    screen_height: u32,
) -> Frame {
    let sw = screen_width as f32;
    let sh = screen_height as f32;
    let left = -1.0 + 2.0 * x_px as f32 / sw;
    let top = 1.0 - 2.0 * y_px as f32 / sh;
    Frame {
        start_x: left,
        start_y: top,
        end_x: left + 2.0 * width_px as f32 / sw,
        end_y: top - 2.0 * height_px as f32 / sh,
    }
}

impl Icon {
    /// Create a new icon from a solid color.
    ///
    /// By default fills its parent; use `.width()`/`.height()` for explicit sizes.
    pub fn from_color(
        ctx: &Context,
        rgba: [u8; 4],
        id: u32,
    ) -> Self {
        let screen_width = ctx.config.width;
        let screen_height = ctx.config.height;
        let screen_pos = Frame {
            start_x: 0.0,
            start_y: 0.0,
            end_x: 0.0,
            end_y: 0.0,
        };

        let tex = Texture::from_color(rgba, &ctx.device, &ctx.queue);
        let layout = mk_bind_group_layout(&ctx.device);
        let normal = mk_bind_group(&ctx.device, &tex, &layout);

        let vertices = vertices_from_coords(&screen_pos, &screen_pos);
        let vertex_buffer = ctx.device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Icon Color Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        });
        let indices: &[u16] = &[0, 1, 3, 1, 2, 3];
        let index_buffer = ctx.device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Icon Color Index Buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: BufferUsages::INDEX,
        });
        let num_indices = indices.len();

        Self {
            id,
            width_px: 0,
            height_px: 0,
            placement: Placement::default(),
            screen_width,
            screen_height,
            screen_pos,
            tex_coords: screen_pos,
            resources: Resources::Color(ColorResources {
                num_indices,
                vertex_buffer,
                index_buffer,
                bind_group: normal,
            }),
        }
    }

    /// Create a new icon from an atlas slot.
    ///
    /// By default fills its parent; use `.width()`/`.height()` for explicit sizes.
    pub fn new(
        ctx: &Context,
        atlas: Arc<Atlas>,
        id: u32,
        slot: u8,
    ) -> Self {
        let screen_width = ctx.config.width;
        let screen_height = ctx.config.height;
        let screen_pos = Frame {
            start_x: 0.0,
            start_y: 0.0,
            end_x: 0.0,
            end_y: 0.0,
        };

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
            width_px: 0,
            height_px: 0,
            placement: Placement::default(),
            screen_width,
            screen_height,
            screen_pos,
            tex_coords,
            resources: Resources::Image(ImageResources {
                num_indices,
                atlas,
                vertex_buffer,
                index_buffer,
            }),
        }
    }

    pub fn halign(mut self, align: HAlign) -> Self {
        self.placement.halign = align;
        self
    }

    pub fn valign(mut self, align: VAlign) -> Self {
        self.placement.valign = align;
        self
    }

    pub fn width(mut self, w: u32) -> Self {
        self.placement.width = Some(w);
        self
    }

    pub fn height(mut self, h: u32) -> Self {
        self.placement.height = Some(h);
        self
    }

    /// Reposition the icon to `(x_px, y_px)` and upload the new vertices.
    ///
    /// Intended to be called by containers that manage this icon's layout.
    pub fn set_position(&mut self, x_px: u32, y_px: u32, queue: &wgpu::Queue) {
        self.screen_pos = pixels_to_ndc(
            x_px,
            y_px,
            self.width_px,
            self.height_px,
            self.screen_width,
            self.screen_height,
        );
        let vertices = vertices_from_coords(&self.screen_pos, &self.tex_coords);
        match &self.resources {
            Resources::Image(image_resources) => queue.write_buffer(
                &image_resources.vertex_buffer,
                0,
                bytemuck::cast_slice(&vertices),
            ),
            Resources::Color(color_resource) => queue.write_buffer(
                &color_resource.vertex_buffer,
                0,
                bytemuck::cast_slice(&vertices),
            ),
        }
    }
}

pub(crate) fn vertices_from_coords(screen_pos: &Frame, tex_coords: &Frame) -> Vec<Vertex> {
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

impl Layout for Icon {
    fn resolve(
        &mut self,
        parent_x: u32,
        parent_y: u32,
        parent_w: u32,
        parent_h: u32,
        queue: &wgpu::Queue,
    ) {
        let (x, y, w, h) = self.placement.resolve(parent_x, parent_y, parent_w, parent_h);
        self.width_px = w;
        self.height_px = h;
        self.set_position(x, y, queue);
    }
}

impl<S, E> GraphicsFlow<S, E> for Icon {
    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        match &self.resources {
            Resources::Image(image_resources) => Render::GUI(Flat {
                vertex: &image_resources.vertex_buffer,
                index: &image_resources.index_buffer,
                group: &image_resources.atlas.bind_group,
                amount: image_resources.num_indices,
                id: self.id,
            }),
            Resources::Color(color_resources) => Render::GUI(Flat {
                vertex: &color_resources.vertex_buffer,
                index: &color_resources.index_buffer,
                group: &color_resources.bind_group,
                amount: color_resources.num_indices,
                id: self.id,
            }),
        }
    }

    // TODO: custom resize mechanism or custom event for resizing or re-use of resize window event.
}
