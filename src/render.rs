use std::collections::{HashMap, HashSet};

use wgpu::RenderPass;

use crate::{
    context::Context,
    data_structures::{block::BuildingBlocks, model::Model, scene_graph::SceneNode},
};

pub struct Instanced<'a> {
    pub instance: &'a wgpu::Buffer,
    pub model: &'a Model,
    pub amount: usize,
    pub id: u32,
}

pub struct Flat<'a> {
    pub vertex: &'a wgpu::Buffer,
    pub index: &'a wgpu::Buffer,
    pub group: &'a wgpu::BindGroup,
    pub amount: usize,
    pub id: u32,
}

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
