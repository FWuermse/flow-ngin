//! Render composition and pipeline batching.
//!
//! This module defines the [`Render`] enum, which is used by scene nodes to specify
//! how they should be rendered. The engine uses `Render` to sort objects into batches
//! for different pipelines (basic, transparent, GUI, terrain, picking, etc.) and to
//! support custom per-object render passes.
//!
//! # Key types
//!
//! - [`Render<'a, 'pass>`] is the primary enum describing render operations
//! - [`Instanced<'a>`] contains data for instanced rendering (model + instance buffer)
//! - [`Flat<'a>`] contains data for flat (2D / GUI) rendering (vertex + index buffers)
//!

use std::collections::{HashMap, HashSet};

use wgpu::RenderPass;

use crate::{
    context::{Context, GPUResource},
    data_structures::{block::BuildingBlocks, model::Model, scene_graph::SceneNode},
    pick::PickId,
    pipelines::transparent::TransparencyUniform,
};

/// Data for instanced object rendering: a model, instance buffer, and pick ID.
///
/// Used for 3D objects rendered with GPU instancing. The instance buffer contains
/// per-instance transformation data and other per-instance attributes.
#[derive(Clone)]
pub struct Instanced<'a> {
    pub instance: &'a wgpu::Buffer,
    pub model: &'a Model,
    pub front_face: wgpu::FrontFace,
    /// Tile the texture by the instance scale instead of stretching it.
    pub tiled: bool,
    pub amount: usize,
    pub id: PickId,
}

/// Data for flat (2D / GUI) object rendering: vertex and index buffers with a bind group.
///
/// Used for 2D GUI elements, terrain, or other flat geometry. The bind group
/// contains textures and samplers for the rendered objects.
#[derive(Clone)]
pub struct Flat<'a> {
    pub vertex: &'a wgpu::Buffer,
    pub index: &'a wgpu::Buffer,
    pub group: &'a wgpu::BindGroup,
    pub amount: usize,
    pub id: PickId,
}

/// Data for custom instanced vertex rendering.
///
/// Used for 2D GUI elements, terrain, or other flat geometry. The bind group
/// contains textures and samplers for the rendered objects.
#[derive(Clone)]
pub struct Geometry<'a> {
    pub instance: &'a wgpu::Buffer,
    pub vertex: &'a wgpu::Buffer,
    pub index: &'a wgpu::Buffer,
    pub group: &'a wgpu::BindGroup,
    pub amount: usize,
    pub id: PickId,
}

/// Specifies how a scene object should be rendered.
///
/// `Render` is an enum that allows flexible composition of render operations.
/// It can represent a single instanced object, a batch of objects, transparent
/// objects, GUI elements, terrain, a composite of multiple renders, or a custom
/// render closure for special effects.
///
/// # Variants
///
/// - `None` renders nothing
/// - `Default(Instanced)` renders a single opaque instanced object
/// - `Defaults(Vec<Instanced>)` renders a batch of opaque instanced objects
/// - `Transparent(Instanced)` renders a single transparent instanced object
/// - `Transparents(Vec<Instanced>)` renders a batch of transparent objects
/// - `GUI(Flat)` renders 2D elements (flat geometry)
/// - `Terrain(Flat)` renders terrain mesh
/// - `Composed(Vec<Render>)` recursively renders composition of multiple renders
/// - `Custom(...)` invokes a user-defined closure for custom rendering
/// - `Clipped(rect, render)` restricts the GUI and custom draws of `render` to `rect` (nested
///   clips aren't intersected
///
#[derive(Default)]
pub enum Render<'a, 'pass>
where
    'pass: 'a,
{
    #[default]
    None,
    Default(Instanced<'a>),
    Defaults(Vec<Instanced<'a>>),
    Transparent(Instanced<'a>, TransparencyUniform),
    Transparents(Vec<Instanced<'a>>, TransparencyUniform),
    GUI(Flat<'a>),
    Terrain(Geometry<'a>),
    Composed(Vec<Render<'a, 'pass>>),
    Custom(Box<dyn 'a + FnOnce(&Context, &mut wgpu::RenderPass<'pass>) -> ()>),
    Clipped(ClipRect, Box<Render<'a, 'pass>>),
}

/// Screen-space rectangle in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl ClipRect {
    /// Intersection with a `width` × `height` render target; `None` if nothing is left.
    pub(crate) fn within(self, width: u32, height: u32) -> Option<ClipRect> {
        let x = self.x.min(width);
        let y = self.y.min(height);
        let right = self.x.saturating_add(self.width).min(width);
        let bottom = self.y.saturating_add(self.height).min(height);
        (right > x && bottom > y).then(|| ClipRect {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }
}

pub(crate) fn draw_flat(render_pass: &mut RenderPass<'_>, flat: &Flat<'_>) {
    render_pass.set_bind_group(0, flat.group, &[]);
    render_pass.set_vertex_buffer(0, flat.vertex.slice(..));
    render_pass.set_index_buffer(flat.index.slice(..), wgpu::IndexFormat::Uint16);
    render_pass.draw_indexed(0..flat.amount as u32, 0, 0..1);
}

pub(crate) fn map_id_list(
    ids: &[PickId],
    flow_id: usize,
    map: &mut HashMap<PickId, HashSet<usize>>,
) {
    for &id in ids {
        map.entry(id)
            .and_modify(|flows| {
                flows.insert(flow_id);
            })
            .or_insert([flow_id].into());
    }
}

impl<'a, 'pass> Render<'a, 'pass> {
    /// Map object IDs to flow IDs for picking and selection.
    ///
    /// Internal helper used during picking setup to associate which flow owns
    /// which object IDs. Walks the render tree and populates a map of object ID
    /// to set of flow IDs.
    pub(crate) fn map_ids(
        &self,
        // TODO: introduce id caching in ctx
        flow_id: usize,
        map: &mut HashMap<PickId, HashSet<usize>>,
    ) {
        match self {
            Render::Default(instanced) => map_id_list(&[instanced.id], flow_id, map),
            Render::Defaults(vec) => {
                let ids: Vec<PickId> = vec.iter().map(|i| i.id).collect();
                map_id_list(&ids, flow_id, map);
            }
            Render::Transparents(vec, _) => {
                let ids: Vec<PickId> = vec.iter().map(|i| i.id).collect();
                map_id_list(&ids, flow_id, map);
            }
            Render::Transparent(instanced, _) => map_id_list(&[instanced.id], flow_id, map),
            Render::GUI(flat) => map_id_list(&[flat.id], flow_id, map),
            Render::Terrain(flat) => map_id_list(&[flat.id], flow_id, map),
            Render::Composed(renders) => renders
                .into_iter()
                .for_each(|render| render.map_ids(flow_id, map)),
            Render::Clipped(_, render) => render.map_ids(flow_id, map),
            Render::None | Render::Custom(_) => (),
        }
    }

    pub(crate) fn set_pipelines(
        self,
        ctx: &Context,
        render_pass: &mut RenderPass<'pass>,
        basics: &mut Vec<Instanced<'a>>,
        trans: &mut Vec<(Instanced<'a>, TransparencyUniform)>,
        guis: &mut Vec<Flat<'a>>,
        terrain: &mut Vec<Geometry<'a>>,
        customs: &mut Vec<Box<dyn 'a + FnOnce(&Context, &mut wgpu::RenderPass<'pass>) -> ()>>,
    ) {
        match self {
            Render::Default(instanced) => {
                basics.push(instanced);
            }
            Render::Defaults(mut vec) => basics.append(&mut vec),
            Render::Transparent(instanced, transparency) => trans.push((instanced, transparency)),
            Render::Transparents(vec, transparency) => {
                trans.extend(vec.into_iter().map(|i| (i, transparency)))
            }
            Render::GUI(flat) => guis.push(flat),
            Render::Terrain(flat) => terrain.push(flat),
            Render::Composed(renders) => renders
                .into_iter()
                .map(|render| {
                    render.set_pipelines(ctx, render_pass, basics, trans, guis, terrain, customs)
                })
                .collect(),
            Render::Custom(f) => customs.push(f),
            Render::Clipped(rect, render) => {
                // Drawn as one custom step, so the clipped GUI quads and texts keep their order
                // relative to each other and the scissor only applies to them.
                let mut clipped_guis = Vec::new();
                let mut clipped_customs = Vec::new();
                render.set_pipelines(
                    ctx,
                    render_pass,
                    basics,
                    trans,
                    &mut clipped_guis,
                    terrain,
                    &mut clipped_customs,
                );
                customs.push(Box::new(
                    move |ctx: &Context, pass: &mut RenderPass<'pass>| {
                        let (width, height) = (ctx.config.width, ctx.config.height);
                        let Some(clip) = rect.within(width, height) else {
                            return;
                        };
                        pass.set_scissor_rect(clip.x, clip.y, clip.width, clip.height);
                        pass.set_pipeline(&ctx.pipelines.gui);
                        pass.set_bind_group(1, &ctx.screen_size.bind_group, &[]);
                        for flat in &clipped_guis {
                            draw_flat(pass, flat);
                        }
                        for custom in clipped_customs {
                            custom(ctx, pass);
                        }
                        pass.set_scissor_rect(0, 0, width, height);
                    },
                ));
            }
            Render::None => (),
        }
    }

    pub(crate) fn set_pick_pipelines(
        self,
        ctx: &Context,
        render_pass: &mut RenderPass<'pass>,
        basics: &mut Vec<Instanced<'a>>,
        flats: &mut Vec<Flat<'a>>,
        geoms: &mut Vec<Geometry<'a>>,
    ) {
        match self {
            Render::Default(instanced) => {
                basics.push(instanced);
            }
            Render::Defaults(mut vec) => basics.append(&mut vec),
            Render::Transparent(instanced, _) => basics.push(instanced),
            Render::Transparents(mut vec, _) => basics.append(&mut vec),
            Render::GUI(flat) => flats.push(flat),
            Render::Terrain(flat) => geoms.push(flat),
            Render::Composed(renders) => renders
                .into_iter()
                .map(|render| render.set_pick_pipelines(ctx, render_pass, basics, flats, geoms))
                .collect(),
            Render::Clipped(_, render) => {
                render.set_pick_pipelines(ctx, render_pass, basics, flats, geoms)
            }
            // Picking is not supported for custom renders
            Render::Custom(_) => (),
            Render::None => (),
        }
    }

    /// Transforms renders of type `Default` or `Defaults` to Transparent
    pub fn transparent(self, tu: TransparencyUniform) -> Self {
        match self {
            Render::Default(instanced) => Render::Transparent(
                Instanced {
                    instance: instanced.instance,
                    model: instanced.model,
                    amount: instanced.amount,
                    front_face: instanced.front_face,
                    tiled: instanced.tiled,
                    id: instanced.id,
                },
                tu,
            ),
            Render::Defaults(instanceds) => Render::Transparents(
                instanceds
                    .into_iter()
                    .map(|instanced| Instanced {
                        instance: instanced.instance,
                        model: instanced.model,
                        amount: instanced.amount,
                        front_face: instanced.front_face,
                        tiled: instanced.tiled,
                        id: instanced.id,
                    })
                    .collect(),
                tu,
            ),
            other => other,
        }
    }
}
impl<'a, 'pass> From<&'a dyn SceneNode> for Render<'a, 'pass> {
    fn from(sn: &'a dyn SceneNode) -> Self {
        Render::Defaults(sn.get_renders())
    }
}
impl<'a, 'pass> From<&'a (dyn SceneNode + Send)> for Render<'a, 'pass> {
    fn from(sn: &'a (dyn SceneNode + Send)) -> Self {
        Render::Defaults(sn.get_renders())
    }
}
impl<'a, 'pass> From<&'a BuildingBlocks> for Render<'a, 'pass> {
    fn from(blocks: &'a BuildingBlocks) -> Self {
        blocks.get_render()
    }
}
#[cfg(feature = "integration-tests")]
impl<'b, 'pass> From<&'b Box<dyn SceneNode>> for Render<'b, 'pass> {
    fn from(val: &'b Box<dyn SceneNode>) -> Self {
        // Dereference the Box to get &dyn SceneNode, which Render implements From for
        Render::from(val.as_ref())
    }
}
#[cfg(feature = "integration-tests")]
impl<'a, 'pass> From<&'a Box<dyn GPUResource<'a, 'pass> + Send>> for Render<'a, 'pass> {
    fn from(val: &'a Box<dyn GPUResource<'a, 'pass> + Send>) -> Self {
        val.get_render()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pick::PickId;

    // --- ClipRect ---

    #[test]
    fn clip_rect_is_cut_to_the_render_target() {
        let r = ClipRect {
            x: 100,
            y: 50,
            width: 300,
            height: 100,
        };
        assert_eq!(r.within(800, 600), Some(r));
        assert_eq!(
            r.within(200, 120),
            Some(ClipRect {
                x: 100,
                y: 50,
                width: 100,
                height: 70
            })
        );
        assert_eq!(r.within(100, 600), None);
        assert_eq!(
            ClipRect {
                x: 0,
                y: 0,
                width: 0,
                height: 10
            }
            .within(800, 600),
            None
        );
    }

    // --- map_id_list ---

    #[test]
    fn single_id_single_flow() {
        let mut map = HashMap::new();
        map_id_list(&[PickId(5)], 0, &mut map);
        assert_eq!(map.len(), 1);
        assert!(map[&PickId(5)].contains(&0));
    }

    #[test]
    fn single_id_two_flows() {
        let mut map = HashMap::new();
        map_id_list(&[PickId(5)], 0, &mut map);
        map_id_list(&[PickId(5)], 1, &mut map);
        assert_eq!(map[&PickId(5)].len(), 2);
        assert!(map[&PickId(5)].contains(&0));
        assert!(map[&PickId(5)].contains(&1));
    }

    #[test]
    fn two_ids_one_flow() {
        let mut map = HashMap::new();
        map_id_list(&[PickId(1), PickId(2)], 0, &mut map);
        assert!(map[&PickId(1)].contains(&0));
        assert!(map[&PickId(2)].contains(&0));
    }

    #[test]
    fn empty_ids_no_change() {
        let mut map = HashMap::new();
        map_id_list(&[], 0, &mut map);
        assert!(map.is_empty());
    }

    #[test]
    fn duplicate_flow_id_is_idempotent() {
        let mut map = HashMap::new();
        map_id_list(&[PickId(7)], 0, &mut map);
        map_id_list(&[PickId(7)], 0, &mut map);
        assert_eq!(map[&PickId(7)].len(), 1);
    }

    // --- Render::map_ids (GPU-free variants) ---

    #[test]
    fn none_maps_nothing() {
        let mut map = HashMap::new();
        Render::<'_, '_>::None.map_ids(0, &mut map);
        assert!(map.is_empty());
    }

    #[test]
    fn composed_empty_maps_nothing() {
        let mut map = HashMap::new();
        Render::<'_, '_>::Composed(vec![]).map_ids(0, &mut map);
        assert!(map.is_empty());
    }
}
