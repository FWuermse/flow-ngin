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
    context::Context,
    data_structures::{block::BuildingBlocks, model::Model, scene_graph::SceneNode},
};

/// Data for instanced object rendering: a model, instance buffer, and pick ID.
///
/// Used for 3D objects rendered with GPU instancing. The instance buffer contains
/// per-instance transformation data and other per-instance attributes.
pub struct Instanced<'a> {
    pub instance: &'a wgpu::Buffer,
    pub model: &'a Model,
    pub amount: usize,
    pub id: u32,
}

/// Data for flat (2D / GUI) object rendering: vertex and index buffers with a bind group.
///
/// Used for 2D GUI elements, terrain, or other flat geometry. The bind group
/// contains textures and samplers for the rendered objects.
pub struct Flat<'a> {
    pub vertex: &'a wgpu::Buffer,
    pub index: &'a wgpu::Buffer,
    pub group: &'a wgpu::BindGroup,
    pub amount: usize,
    pub id: u32,
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
///
pub enum Render<'a, 'pass>
where
    'pass: 'a,
{
    None,
    Default(Instanced<'a>),
    Defaults(Vec<Instanced<'a>>),
    Transparent(Instanced<'a>),
    Transparents(Vec<Instanced<'a>>),
    GUI(Flat<'a>),
    Terrain(Flat<'a>),
    Composed(Vec<Render<'a, 'pass>>),
    Custom(Box<dyn 'a + FnOnce(&Context, &mut wgpu::RenderPass<'pass>) -> ()>),
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
        map: &mut HashMap<u32, HashSet<usize>>,
    ) {
        match self {
            Render::Default(instanced) => {
                map.entry(instanced.id)
                    .and_modify(|flows| _ = flows.insert(flow_id))
                    .or_insert([flow_id].into());
            }
            Render::Defaults(vec) => vec.into_iter().for_each(|instanced| {
                map.entry(instanced.id)
                    .and_modify(|flows| {
                        flows.insert(flow_id);
                    })
                    .or_insert([flow_id].into());
            }),
            Render::Transparents(vec) => vec.into_iter().for_each(|instanced| {
                map.entry(instanced.id)
                    .and_modify(|flows| {
                        flows.insert(flow_id);
                    })
                    .or_insert([flow_id].into());
            }),
            Render::Transparent(instanced) => {
                map.entry(instanced.id)
                    .and_modify(|flows| _ = flows.insert(flow_id))
                    .or_insert([flow_id].into());
            }
            Render::GUI(flat) => {
                map.entry(flat.id)
                    .and_modify(|flows| _ = flows.insert(flow_id))
                    .or_insert([flow_id].into());
            }
            Render::Terrain(flat) => {
                map.entry(flat.id)
                    .and_modify(|flows| _ = flows.insert(flow_id))
                    .or_insert([flow_id].into());
            }
            Render::Composed(renders) => renders
                .into_iter()
                .for_each(|render| render.map_ids(flow_id, map)),
            Render::None | Render::Custom(_) => (),
        }
    }

    pub(crate) fn set_pipelines(
        self,
        ctx: &Context,
        render_pass: &mut RenderPass<'pass>,
        basics: &mut Vec<Instanced<'a>>,
        trans: &mut Vec<Instanced<'a>>,
        guis: &mut Vec<Flat<'a>>,
        terrain: &mut Vec<Flat<'a>>,
    ) {
        match self {
            Render::Default(instanced) => {
                basics.push(instanced);
            }
            Render::Defaults(mut vec) => basics.append(&mut vec),
            Render::Transparent(instanced) => trans.push(instanced),
            Render::Transparents(mut vec) => trans.append(&mut vec),
            Render::GUI(flat) => guis.push(flat),
            Render::Terrain(flat) => terrain.push(flat),
            Render::Composed(renders) => renders
                .into_iter()
                .map(|render| render.set_pipelines(ctx, render_pass, basics, trans, guis, terrain))
                .collect(),
            Render::Custom(f) => f(ctx, render_pass),
            Render::None => (),
        }
    }

    pub(crate) fn set_pick_pipelines(
        self,
        ctx: &Context,
        render_pass: &mut RenderPass<'pass>,
        basics: &mut Vec<Instanced<'a>>,
        flats: &mut Vec<Flat<'a>>,
    ) {
        match self {
            Render::Default(instanced) => {
                basics.push(instanced);
            }
            Render::Defaults(mut vec) => basics.append(&mut vec),
            Render::Transparent(instanced) => basics.push(instanced),
            Render::Transparents(mut vec) => basics.append(&mut vec),
            Render::GUI(flat) => flats.push(flat),
            Render::Terrain(flat) => flats.push(flat),
            Render::Composed(renders) => renders
                .into_iter()
                .map(|render| render.set_pick_pipelines(ctx, render_pass, basics, flats))
                .collect(),
            // Picking is not supported for custom renders
            Render::Custom(_) => (),
            Render::None => (),
        }
    }
}
impl<'a, 'pass> From<&'a dyn SceneNode> for Render<'a, 'pass> {
    fn from(sn: &'a dyn SceneNode) -> Self {
        Render::Defaults(sn.get_render(Default::default()))
    }
}
impl<'a, 'pass> From<&'a BuildingBlocks> for Render<'a, 'pass> {
    fn from(blocks: &'a BuildingBlocks) -> Self {
        Render::Default(Instanced {
            instance: &blocks.instance_buffer,
            model: &blocks.obj_model,
            amount: blocks.instances.len(),
            id: blocks.id,
        })
    }
}
