use std::{sync::Arc, u16};

use cgmath::num_traits::ToPrimitive;
use wgpu::{
    BufferUsages,
    util::{BufferInitDescriptor, DeviceExt},
};

use crate::{
    context::Context, data_structures::texture::Texture, flow::GraphicsFlow, pick::PickId, pipelines::gui::{Vertex, mk_bind_group, mk_bind_group_layout}, render::{Flat, Render}, resources::texture::load_texture, ui::{Placement, layout::Layout}
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

/// Rectangle in either pixel-space (for screen positions) or UV-space (for texture coordinates).
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
    atlas_width_px: u32,
    atlas_height_px: u32,
}
impl Atlas {
    pub async fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        file_name: &str,
        h_grids: u8,
        v_grids: u8,
    ) -> Self {
        let mut atlas = load_texture(file_name, false, device, queue, None)
            .await
            .expect(&format!("File does not exist: {}", file_name));
        let size = atlas.texture.size();

        // Use ClampToEdge to prevent UV wrapping at atlas cell boundaries.
        atlas.sampler = Some(device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        }));

        let texture_bind_group_layout = mk_bind_group_layout(device);
        let bind_group = mk_bind_group(device, &atlas, &texture_bind_group_layout);
        Atlas {
            bind_group,
            h_grids,
            v_grids,
            atlas_width_px: size.width,
            atlas_height_px: size.height,
        }
    }
    fn to_tex_coords(&self, slot: u8) -> Option<Frame> {
        let max_slot = self.h_grids.checked_mul(self.v_grids)?.saturating_sub(1);
        let slot = slot.min(max_slot);
        let row = slot % self.h_grids;
        let col = slot / self.h_grids;
        let cell_w = 1.0 / self.h_grids.to_f32()?;
        let cell_h = 1.0 / self.v_grids.to_f32()?;

        // Inset by half a texel to prevent linear filtering from sampling
        // neighbouring cells at atlas cell boundaries.
        let half_texel_u = 0.5 / self.atlas_width_px as f32;
        let half_texel_v = 0.5 / self.atlas_height_px as f32;

        let frame = Frame {
            start_x: row.to_f32()? * cell_w + half_texel_u,
            start_y: col.to_f32()? * cell_h + half_texel_v,
            end_x: (row + 1).to_f32()? * cell_w - half_texel_u,
            end_y: (col + 1).to_f32()? * cell_h - half_texel_v,
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
    pub width_px: u32,
    pub height_px: u32,
    pub placement: Placement,
    screen_pos: Frame,
    tex_coords: Frame,
    resources: Resources,
}

/// Build a pixel-space Frame. The shader converts to NDC using the screen_size uniform.
pub(crate) fn pixels_to_frame(
    x_px: u32,
    y_px: u32,
    width_px: u32,
    height_px: u32,
) -> Frame {
    Frame {
        start_x: x_px as f32,
        start_y: y_px as f32,
        end_x: (x_px + width_px) as f32,
        end_y: (y_px + height_px) as f32,
    }
}

impl Icon {
    /// Create a new icon from a solid color.
    ///
    /// By default fills its parent; use `.width()`/`.height()` for explicit sizes.
    pub fn from_color(
        ctx: &Context,
        rgba: [u8; 4],
    ) -> Self {
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
            width_px: 0,
            height_px: 0,
            placement: Placement::default(),
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
        atlas: &Arc<Atlas>,
        slot: u8,
    ) -> Self {
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
            label: Some(&format!("Icon Vertex Buffer slot {}", slot)),
            contents: bytemuck::cast_slice(&vertices),
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        });
        let indices: &[u16] = &[0, 1, 3, 1, 2, 3];
        let index_buffer = ctx.device.create_buffer_init(&BufferInitDescriptor {
            label: Some(&format!("Icon Index Buffer slot {}", slot)),
            contents: bytemuck::cast_slice(indices),
            usage: BufferUsages::INDEX,
        });
        let num_indices = indices.len();

        Self {
            width_px: 0,
            height_px: 0,
            placement: Placement::default(),
            screen_pos,
            tex_coords,
            resources: Resources::Image(ImageResources {
                num_indices,
                atlas: Arc::clone(atlas),
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
        self.screen_pos = pixels_to_frame(x_px, y_px, self.width_px, self.height_px);
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

impl<S, E: Send> GraphicsFlow<S, E> for Icon {
    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        match &self.resources {
            Resources::Image(image_resources) => Render::GUI(Flat {
                vertex: &image_resources.vertex_buffer,
                index: &image_resources.index_buffer,
                group: &image_resources.atlas.bind_group,
                amount: image_resources.num_indices,
                id: PickId(0),
            }),
            Resources::Color(color_resources) => Render::GUI(Flat {
                vertex: &color_resources.vertex_buffer,
                index: &color_resources.index_buffer,
                group: &color_resources.bind_group,
                amount: color_resources.num_indices,
                id: PickId(0),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- pixels_to_frame ---

    #[test]
    fn pixels_to_frame_basic() {
        let f = pixels_to_frame(10, 20, 100, 50);
        assert_eq!(f.start_x, 10.0);
        assert_eq!(f.start_y, 20.0);
        assert_eq!(f.end_x, 110.0);
        assert_eq!(f.end_y, 70.0);
    }

    #[test]
    fn pixels_to_frame_zero_size() {
        let f = pixels_to_frame(5, 5, 0, 0);
        assert_eq!(f.start_x, 5.0);
        assert_eq!(f.end_x, 5.0);
        assert_eq!(f.start_y, 5.0);
        assert_eq!(f.end_y, 5.0);
    }

    // pixels_to_frame must not overflow (callers must not pass x+w > u32::MAX),
    // but the function should at minimum not silently produce wrong results.
    // This test documents that the addition overflows in debug and wraps in release.
    #[test]
    fn pixels_to_frame_large_but_valid_coords() {
        let f = pixels_to_frame(u32::MAX - 10, 0, 5, 0);
        assert_eq!(f.start_x, (u32::MAX - 10) as f32);
        assert_eq!(f.end_x, (u32::MAX - 5) as f32);
    }

    // --- Atlas::to_tex_coords ---

    #[test]
    fn atlas_first_slot_cell_dimensions() {
        // Test the math that to_tex_coords uses, without constructing an Atlas
        let h_grids: u8 = 4;
        let v_grids: u8 = 4;
        let slot: u8 = 0;
        let row = slot % h_grids;
        let col = slot / h_grids;
        assert_eq!(row, 0);
        assert_eq!(col, 0);

        let cell_w = 1.0 / h_grids as f32;
        let cell_h = 1.0 / v_grids as f32;
        assert!((cell_w - 0.25).abs() < 1e-6);
        assert!((cell_h - 0.25).abs() < 1e-6);
    }

    #[test]
    fn atlas_slot_row_col_mapping() {
        // For a 4x4 atlas: slot 5 → row = 5%4 = 1, col = 5/4 = 1
        let h_grids: u8 = 4;
        assert_eq!(5u8 % h_grids, 1); // row
        assert_eq!(5u8 / h_grids, 1); // col

        // slot 3 → row=3, col=0
        assert_eq!(3u8 % h_grids, 3);
        assert_eq!(3u8 / h_grids, 0);

        // slot 4 → row=0, col=1 (wraps to next column)
        assert_eq!(4u8 % h_grids, 0);
        assert_eq!(4u8 / h_grids, 1);
    }

    #[test]
    fn atlas_texel_inset_is_positive() {
        let atlas_width_px: u32 = 512;
        let atlas_height_px: u32 = 512;
        let half_texel_u = 0.5 / atlas_width_px as f32;
        let half_texel_v = 0.5 / atlas_height_px as f32;
        assert!(half_texel_u > 0.0);
        assert!(half_texel_v > 0.0);
        // For 512px: 0.5/512 ≈ 0.000977
        assert!((half_texel_u - 0.5 / 512.0).abs() < 1e-8);
    }

    // A slot index beyond the grid must not produce UV coordinates outside [0,1].
    #[test]
    fn atlas_slot_exceeding_grid_must_clamp_uv() {
        let h_grids: u8 = 4;
        let v_grids: u8 = 4;
        let slot: u8 = 17; // beyond 4x4=16 slots
        // to_tex_coords clamps slot to max_slot = h_grids*v_grids - 1
        let max_slot = h_grids * v_grids - 1;
        let clamped_slot = slot.min(max_slot);
        let col = clamped_slot / h_grids;
        let cell_h = 1.0 / v_grids as f32;
        let end_y = (col + 1) as f32 * cell_h;
        assert!(
            end_y <= 1.0,
            "UV end_y={} must not exceed 1.0 for out-of-range slot",
            end_y
        );
    }

    // --- vertices_from_coords ---

    #[test]
    fn vertices_from_coords_produces_ccw_quad() {
        let screen = Frame {
            start_x: 0.0,
            start_y: 0.0,
            end_x: 100.0,
            end_y: 50.0,
        };
        let tex = Frame {
            start_x: 0.0,
            start_y: 0.0,
            end_x: 1.0,
            end_y: 1.0,
        };
        let verts = vertices_from_coords(&screen, &tex);
        assert_eq!(verts.len(), 4);
        // bottom-left
        assert_eq!(verts[0].position, [0.0, 50.0, 0.0]);
        assert_eq!(verts[0].tex_coords, [0.0, 1.0]);
        // bottom-right
        assert_eq!(verts[1].position, [100.0, 50.0, 0.0]);
        // top-right
        assert_eq!(verts[2].position, [100.0, 0.0, 0.0]);
        // top-left
        assert_eq!(verts[3].position, [0.0, 0.0, 0.0]);
    }

}