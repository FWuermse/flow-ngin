//! Scene graph and hierarchical scene organization.
//!
//! Provides traits and structures for building a scene graph: a hierarchical
//! representation of objects in a scene, including animation support and
//! renderable object composition.

use std::{collections::HashMap, ops::Range};

use log::warn;
use wgpu::{Device, Queue, util::DeviceExt};

use crate::{
    context::GPUResource,
    data_structures::{
        instance::{Instance, InstanceRaw},
        model::{self, DrawModel},
    },
    render::{Instanced, Render},
    resources::{animation::Keyframes, load_model_obj, pick::load_pick_model},
};

/// An animation clip: a named animation with keyframes and timing.
#[derive(Clone, Debug)]
pub struct AnimationClip {
    pub name: String,
    pub keyframes: Keyframes,
    pub timestamps: Vec<f32>,
}

#[derive(Clone, Debug, Default)]
pub struct ModelAnimation {
    pub name: String,
    pub instances: Vec<Instance>,
    pub timestamps: Vec<f32>,
}

/**
 * Intermediate state when converting between `AnimationClip` and `ModelAnimation`
 */
#[derive(Default)]
struct ModelState {
    animations: Vec<ModelAnimation>,
    trans: Vec<cgmath::Vector3<f32>>,
    rots: Vec<cgmath::Quaternion<f32>>,
    scals: Vec<cgmath::Vector3<f32>>,
    timestamps: Vec<f32>,
    current_clip: String,
}
impl ModelState {
    fn reset(&mut self, clip: &AnimationClip) {
        self.timestamps = vec![];
        self.trans = vec![];
        self.rots = vec![];
        self.scals = vec![];
        self.current_clip = clip.name.clone();
    }
}

pub fn to_scene_node(
    id: u32,
    node: gltf::scene::Node,
    buf: &Vec<Vec<u8>>,
    device: &wgpu::Device,
    mats: &Vec<model::Material>,
    anims: &HashMap<usize, Vec<AnimationClip>>,
) -> Box<dyn SceneNode> {
    // TODO: is node.index() correct?
    let animations = merge(anims[&node.index()].clone());
    // TODO: only select materials for current mesh
    let mut scene_node: Box<dyn SceneNode> = match node.mesh() {
        Some(mesh) => {
            let mut meshes = Vec::new();
            let primitives = mesh.primitives();

            primitives.for_each(|primitive| {
                let reader = primitive.reader(|buffer| Some(&buf[buffer.index()]));

                let mut vertices = Vec::new();
                if let Some(vertex_attribute) = reader.read_positions() {
                    vertex_attribute.for_each(|vertex| {
                        vertices.push(model::ModelVertex {
                            position: vertex,
                            tex_coords: Default::default(),
                            normal: Default::default(),
                            bitangent: Default::default(),
                            tangent: Default::default(),
                        })
                    });
                }
                if let Some(normal_attribute) = reader.read_normals() {
                    let mut normal_index = 0;
                    normal_attribute.for_each(|normal| {
                        vertices[normal_index].normal = normal;

                        normal_index += 1;
                    });
                }
                if let Some(tex_coord_attribute) = reader.read_tex_coords(0).map(|v| v.into_f32()) {
                    let mut tex_coord_index = 0;
                    tex_coord_attribute.for_each(|tex_coord| {
                        vertices[tex_coord_index].tex_coords = tex_coord;

                        tex_coord_index += 1;
                    });
                }
                // TODO: don't recalculate all tangents if the ModelVertex already contains them
                if let Some(tangent_attribute) = reader.read_tangents() {
                    let mut tangent_index = 0;
                    tangent_attribute.for_each(|tangent| {
                        // GLTF represents tangents as vec4 where the 4th elem can be used to calculate the bitangent
                        let tangent: cgmath::Vector4<f32> = tangent.into();
                        vertices[tangent_index].tangent = tangent.truncate().into();
                        let normal: cgmath::Vector3<f32> = vertices[tangent_index].normal.into();
                        let bitangent = normal.cross(tangent.truncate()) * tangent[3];
                        vertices[tangent_index].bitangent = bitangent.into();

                        tangent_index += 1;
                    });
                };

                let mut indices = Vec::new();
                if let Some(indices_raw) = reader.read_indices() {
                    indices.append(&mut indices_raw.into_u32().collect::<Vec<u32>>());
                }
                let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("{:?} Vertex Buffer", mesh.name())),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("{:?} Index Buffer", mesh.name())),
                    contents: bytemuck::cast_slice(&indices),
                    usage: wgpu::BufferUsages::INDEX,
                });
                let mat_idx = mesh
                    .primitives()
                    .filter_map(|prim| prim.material().index())
                    .next()
                    .unwrap_or(0);

                meshes.push(model::Mesh {
                    name: mesh.name().unwrap_or("unknown_mesh").to_string(),
                    vertex_buffer,
                    index_buffer,
                    num_elements: indices.len() as u32,
                    material: mat_idx,
                });
            });
            /* TOOD: don't store all materials in one place (insert Walter White meme here)
                Instead adjust the mesh/anim index above as well as the vec below
                e.g. mats [1,2,3,4] for mesh1[1,2] and mesh2[3,4] must become mats1 [1, 2] mesh1[1,2] and mats2 [1, 2] mesh2 [1, 2]
            */
            let model = model::Model {
                meshes,
                materials: mats.clone(),
            };
            Box::new(ModelNode::from_model(1, id, device, model, animations))
        }
        None => Box::new(ContainerNode::new(1, animations)),
    };
    let decomp_pos = node.transform().decomposed();
    let instance = Instance {
        position: decomp_pos.0.into(),
        rotation: decomp_pos.1.into(),
        scale: decomp_pos.2.into(),
    };
    scene_node.set_local_transform(0, instance);
    for child in node.children() {
        let child_node = to_scene_node(id, child, buf, device, mats, anims);
        scene_node.add_child(child_node);
    }

    scene_node
}

fn save_current_anim(state: &mut ModelState, clip: &AnimationClip) -> ModelAnimation {
    let t_len = state.trans.len();
    let r_len = state.rots.len();
    let s_len = state.scals.len();
    let max_len = t_len.max(r_len.max(s_len));
    if t_len != r_len || r_len != s_len {
        log::warn!(
            "warning, animation track len() doesn't match and will matched with defaults. previous animation: {}, current: {}",
            state.current_clip,
            clip.name
        );
        // Use first frame as default (this is important as child nodes have offsets)
        state.trans.append(
            &mut (t_len..max_len)
                .into_iter()
                .filter_map(|_| state.trans.first())
                .cloned()
                .collect(),
        );
        state.rots.append(
            &mut (r_len..max_len)
                .into_iter()
                .filter_map(|_| state.rots.first())
                .cloned()
                .collect(),
        );
        state.scals.append(
            &mut (s_len..max_len)
                .into_iter()
                .filter_map(|_| state.scals.first())
                .cloned()
                .collect(),
        );
    }
    // now assume the're all the same length
    let mut instances = Vec::with_capacity(t_len);
    for i in 0..max_len {
        let instance = Instance {
            position: state.trans[i],
            rotation: state.rots[i],
            scale: state.scals[i],
        };
        instances.push(instance);
    }
    // new clip, reset vecs
    let animation = ModelAnimation {
        name: clip.name.clone(),
        instances,
        timestamps: state.timestamps.clone(),
    };
    animation
}

/**
 * Merges keyframes with the same name to have all transformations in one place.
 *
 * GLTF:
 * AnimationClip {
 *      name: anim1
 *      keyframes: Scale(
 *          [[data]]
 *      )
 * }
 * AnimationClip {
 *      name: anim1
 *      keyframes: Rotation(
 *          [[data]]
 *      )
 * }
 * ...
 *
 * to
 *
 * ModelAnimation {
 *      name: anim1
 *      keyframes: [
 *          rot: []
 *          tr: []
 *          sc: []
 *      ]
 * }
 */
fn merge(clips: Vec<AnimationClip>) -> Vec<ModelAnimation> {
    let mut state = ModelState {
        current_clip: clips.first().unwrap().name.clone(),
        ..Default::default()
    };
    for clip in clips.iter() {
        if clip.name != state.current_clip {
            let animation = save_current_anim(&mut state, clip);
            state.animations.push(animation);
            state.reset(clip);
        }
        match &clip.keyframes {
            Keyframes::Translation(translations) => translations
                .into_iter()
                .for_each(|&tr| state.trans.push(tr)),
            Keyframes::Rotation(rotations) => {
                rotations.into_iter().for_each(|&rot| state.rots.push(rot));
            }
            Keyframes::Scale(scalations) => {
                scalations.into_iter().for_each(|&sc| state.scals.push(sc));
            }
            Keyframes::Other => todo!(),
        }
        // in case some tracks have fewer steps than others we want to have the largest set of timestamps for smooth animations
        if clip.timestamps.len() > state.timestamps.len() {
            state.timestamps = clip.timestamps.clone();
        }
    }
    if let Some(clip) = clips.last() {
        let animation = save_current_anim(&mut state, clip);
        state.animations.push(animation);
        state.reset(clip);
    }
    state.animations
}

pub trait SceneNode {
    fn get_world_transforms(&self) -> Vec<Instance>;

    fn get_local_transform(&self, idx: usize) -> Option<Instance>;

    fn draw<'a, 'pass>(
        &self,
        camera_bind_group_layout: &'a wgpu::BindGroup,
        light_bind_group: &'a wgpu::BindGroup,
        render_pass: &'pass mut wgpu::RenderPass<'a>,
    ) where
        'a: 'pass;

    fn to_clickable(&self, device: &wgpu::Device, id: u32) -> Box<dyn SceneNode>;

    fn get_children(&self) -> &Vec<Box<dyn SceneNode>>;

    fn add_child(&mut self, child: Box<dyn SceneNode>);

    fn set_local_transform(&mut self, idx: usize, instance: Instance);

    fn set_local_transform_all(&mut self, mutation: &mut dyn FnMut(&mut Instance));

    fn get_children_mut(&mut self) -> &mut Vec<Box<dyn SceneNode>>;

    fn write_to_buffers(&mut self, queue: &wgpu::Queue, device: &wgpu::Device);

    /**
     * Multiple instances of a parent can be passed down to multiple instances of multiple children.
     * The argument `parents_world_transform` with a matching `range` size provides control over which instances are transformed.
     */
    fn update_world_transforms(
        &mut self,
        range: Range<usize>,
        parents_world_transform: &Vec<Instance>,
    );

    fn update_world_transform_all(&mut self);

    fn add_instance(&mut self, instance: Instance) -> usize;

    fn add_instances(&mut self, instances: Vec<Instance>) -> usize;

    fn remove_instance(&mut self, idx: usize) -> (Instance, Instance);

    fn clone_instance(&mut self, i: usize) -> usize;

    fn get_animation(&self) -> &Vec<ModelAnimation>;

    fn get_render(&self) -> Vec<Instanced<'_>>;
}

impl<'a, 'pass, T> GPUResource<'a, 'pass> for T
where
    T: SceneNode,
{
    fn write_to_buffer(&mut self, queue: &wgpu::Queue, device: &wgpu::Device) {
        self.write_to_buffers(queue, device);
    }

    fn get_render(&'a self) -> Render<'a, 'pass> {
        Render::Defaults(self.get_render())
    }
}

pub struct ContainerNode {
    pub children: Vec<Box<dyn SceneNode>>,
    pub instances: Vec<(Instance, Instance)>,
    animations: Vec<ModelAnimation>,
}

impl ContainerNode {
    pub fn new(amount: usize, animations: Vec<ModelAnimation>) -> Self {
        let instances = (0..amount)
            .map(|_| (Instance::default(), Instance::default()))
            .collect();
        let children = vec![];
        Self {
            instances,
            children,
            animations,
        }
    }
}

impl SceneNode for ContainerNode {
    fn add_child(&mut self, child: Box<dyn SceneNode>) {
        self.children.push(child);
    }

    fn set_local_transform(&mut self, idx: usize, instance: Instance) {
        self.instances
            .get_mut(idx)
            .and_then(|(local, _)| Some(*local = instance));
    }

    fn set_local_transform_all(&mut self, mutation: &mut dyn FnMut(&mut Instance)) {
        self.instances.iter_mut().for_each(|(local, _)| {
            mutation(local);
        });
    }

    fn get_world_transforms(&self) -> Vec<Instance> {
        self.instances
            .iter()
            .map(|(_, world)| world)
            .cloned()
            .collect()
    }

    /**
     * Multiple instances of a parent can be passed down to multiple instances of multiple children.
     * The argument `parents_world_transform` with a matching `range` size provides control over which instances are transformed.
     */
    fn update_world_transforms(
        &mut self,
        range: Range<usize>,
        parents_world_transform: &Vec<Instance>,
    ) {
        if parents_world_transform.len() > self.instances.len() {
            warn!(
                "You tried to transform with len {}, but there are only {} instances to transform.",
                parents_world_transform.len(),
                self.instances.len()
            );
            return;
        }
        if let None = self.instances.get(range.clone()) {
            warn!(
                "You tried to transform range {}..{}, which is out of bounds for parent len {}.",
                range.clone().start,
                range.end,
                self.instances.len(),
            );
            return;
        }
        let world_transforms = self.instances[range.clone()]
            .iter_mut()
            .zip(parents_world_transform.into_iter())
            .map(|((local, world), parent)| {
                let world_transform = parent * local;
                *world = parent * local;
                world_transform
            })
            .collect::<Vec<_>>();
        for child in self.children.iter_mut() {
            child.update_world_transforms(range.clone(), &world_transforms);
        }
    }

    fn get_children_mut(&mut self) -> &mut Vec<Box<dyn SceneNode>> {
        &mut self.children
    }

    fn get_local_transform(&self, idx: usize) -> Option<Instance> {
        self.instances.get(idx).map(|(local, _)| local).cloned()
    }

    fn write_to_buffers(&mut self, queue: &wgpu::Queue, device: &wgpu::Device) {
        self.get_children_mut()
            .iter_mut()
            .for_each(|child| child.write_to_buffers(queue, device));
    }

    fn draw<'a, 'pass>(
        &self,
        camera_bind_group: &'a wgpu::BindGroup,
        light_bind_group: &'a wgpu::BindGroup,
        render_pass: &'pass mut wgpu::RenderPass<'a>,
    ) where
        'a: 'pass,
    {
        for child in &self.children {
            child.draw(camera_bind_group, light_bind_group, render_pass);
        }
    }

    fn get_children(&self) -> &Vec<Box<dyn SceneNode>> {
        &self.children
    }

    fn to_clickable(&self, device: &Device, id: u32) -> Box<dyn SceneNode> {
        let children = self
            .children
            .iter()
            .map(|child| child.to_clickable(device, id))
            .collect();

        Box::new(Self {
            children,
            instances: self.instances.clone(),
            animations: Vec::new(),
        })
    }

    fn add_instance(&mut self, instance: Instance) -> usize {
        self.instances.push((instance.clone(), instance));
        for child in &mut self.children {
            child.add_instance(Instance::default());
        }
        self.instances.len()
    }

    fn update_world_transform_all(&mut self) {
        let range = 0..self.instances.len();
        let default_instances = range.clone().map(|_| Instance::default()).collect();
        self.update_world_transforms(range, &default_instances);
    }

    /**
     * Inserts a new instance which is a clone of the instance with index `i`.
     *
     * The return value is the index of the newly created instance.
     */
    fn clone_instance(&mut self, i: usize) -> usize {
        self.instances
            .push((self.instances[i].clone().0, self.instances[i].clone().1));
        for child in &mut self.children {
            child.clone_instance(i);
        }
        self.instances.len()
    }

    fn get_animation(&self) -> &Vec<ModelAnimation> {
        &self.animations
    }

    fn get_render(&self) -> Vec<Instanced<'_>> {
        self.children
            .iter()
            .flat_map(|child| child.get_render())
            .collect()
    }

    fn remove_instance(&mut self, idx: usize) -> (Instance, Instance) {
        self.children.iter_mut().for_each(|c| {
            c.remove_instance(idx);
        });
        self.instances.remove(idx)
    }

    fn add_instances(&mut self, instances: Vec<Instance>) -> usize {
        let cloned = instances.clone();
        let len = instances.len();
        let mut instances = instances.into_iter().zip(cloned).collect();
        self.instances.append(&mut instances);
        for child in &mut self.children {
            child.add_instances((0..len).map(|_| Instance::default()).collect());
        }
        self.instances.len()
    }
}

pub struct ModelNode {
    children: Vec<Box<dyn SceneNode>>,
    instance_buffer: wgpu::Buffer,
    instances: Vec<(Instance, Instance)>,
    animations: Vec<ModelAnimation>,
    buffer_size_needs_change: bool,
    model: model::Model,
    id: u32,
}

impl ModelNode {
    pub async fn new(
        amount: usize,
        id: u32,
        device: &Device,
        queue: &Queue,
        obj_file: &str,
    ) -> Self {
        let obj_model = load_model_obj(obj_file, &device, &queue).await;
        if let Err(e) = obj_model {
            panic!("Error failed to load model: {}, at {}", e, obj_file);
        }
        let obj_model = obj_model.unwrap();

        Self::from_model(amount, id, device, obj_model, Vec::new())
    }

    pub fn from_model(
        amount: usize,
        id: u32,
        device: &Device,
        obj_model: model::Model,
        animations: Vec<ModelAnimation>,
    ) -> Self {
        let instances = (0..amount)
            .map(|_| (Instance::default(), Instance::default()))
            .collect::<Vec<_>>();

        let instance_data = instances
            .iter()
            .map(|(_, world)| world)
            .map(Instance::to_raw)
            .collect::<Vec<_>>();

        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Instance Buffer"),
            contents: bytemuck::cast_slice(&instance_data),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        let size_changed = false;

        Self {
            children: vec![],
            instance_buffer,
            instances,
            model: obj_model,
            buffer_size_needs_change: size_changed,
            animations,
            id,
        }
    }
}

impl SceneNode for ModelNode {
    fn add_child(&mut self, child: Box<dyn SceneNode>) {
        self.children.push(child);
    }

    fn set_local_transform(&mut self, idx: usize, instance: Instance) {
        self.instances
            .get_mut(idx)
            .and_then(|(local, _)| Some(*local = instance));
    }

    fn set_local_transform_all(&mut self, mutation: &mut dyn FnMut(&mut Instance)) {
        self.instances
            .iter_mut()
            .for_each(|(local, _)| mutation(local));
    }

    fn get_world_transforms(&self) -> Vec<Instance> {
        self.instances
            .iter()
            .map(|(_, world)| world)
            .cloned()
            .collect()
    }

    /**
     * Multiple instances of a parent can be passed down to multiple instances of multiple children.
     * The argument `parents_world_transform` with a matching `range` size provides control over which instances are transformed.
     */
    fn update_world_transforms(
        &mut self,
        range: Range<usize>,
        parents_world_transform: &Vec<Instance>,
    ) {
        if parents_world_transform.len() > self.instances.len() {
            warn!(
                "You tried to transform with len {}, but there are only {} instances to transform.",
                parents_world_transform.len(),
                self.instances.len()
            );
            return;
        }
        if let None = self.instances.get(range.clone()) {
            warn!(
                "you tried to transform range {}..{}, which is out of bounds for parent len {}.",
                range.clone().start,
                range.end,
                self.instances.len(),
            );
            return;
        }
        let world_transforms = self.instances[range.clone()]
            .iter_mut()
            .zip(parents_world_transform.into_iter())
            .map(|((local, world), parent)| {
                let world_transform = parent * local;
                *world = parent * local;
                world_transform
            })
            .collect::<Vec<_>>();
        for child in self.children.iter_mut() {
            child.update_world_transforms(range.clone(), &world_transforms);
        }
    }

    fn get_children_mut(&mut self) -> &mut Vec<Box<dyn SceneNode>> {
        &mut self.children
    }

    fn get_local_transform(&self, idx: usize) -> Option<Instance> {
        self.instances.get(idx).map(|(local, _)| local).cloned()
    }

    fn write_to_buffers(&mut self, queue: &wgpu::Queue, device: &wgpu::Device) {
        let raw_instances: Vec<InstanceRaw> = self
            .instances
            .iter()
            .map(|(_, world)| world.to_raw())
            .collect();
        if self.buffer_size_needs_change {
            self.instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Instance Buffer"),
                contents: bytemuck::cast_slice(&raw_instances),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
            self.buffer_size_needs_change = false;
        } else {
            queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&raw_instances),
            );
        }
        self.get_children_mut()
            .iter_mut()
            .for_each(|child| child.write_to_buffers(queue, device));
    }

    fn draw<'a, 'b>(
        &self,
        camera_bind_group: &'a wgpu::BindGroup,
        light_bind_group: &'a wgpu::BindGroup,
        render_pass: &'b mut wgpu::RenderPass<'a>,
    ) where
        'a: 'b,
    {
        let instances = self.get_world_transforms();
        if !instances.is_empty() {
            render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            render_pass.draw_model_instanced(
                &self.model,
                0..instances.len() as u32,
                &camera_bind_group,
                &light_bind_group,
            );
        }
        for child in &self.children {
            child.draw(camera_bind_group, light_bind_group, render_pass);
        }
    }

    fn get_children(&self) -> &Vec<Box<dyn SceneNode>> {
        &self.children
    }

    fn to_clickable(&self, device: &wgpu::Device, id: u32) -> Box<dyn SceneNode> {
        let obj_model = load_pick_model(&device, id, self.model.meshes.clone()).unwrap();

        let children = self
            .children
            .iter()
            .map(|child| child.to_clickable(device, id))
            .collect();

        Box::new(Self {
            children,
            instance_buffer: self.instance_buffer.clone(),
            instances: self.instances.clone(),
            buffer_size_needs_change: false,
            model: obj_model,
            animations: Vec::new(),
            id,
        })
    }

    fn add_instance(&mut self, instance: Instance) -> usize {
        self.instances.push((instance.clone(), instance));
        for child in &mut self.children {
            child.add_instance(Instance::default());
        }
        self.buffer_size_needs_change = true;
        self.instances.len()
    }

    fn update_world_transform_all(&mut self) {
        let range = 0..self.instances.len();
        let default_instances = range.clone().map(|_| Instance::default()).collect();
        self.update_world_transforms(range, &default_instances);
    }

    fn clone_instance(&mut self, i: usize) -> usize {
        self.instances
            .push((self.instances[i].clone().0, self.instances[i].clone().1));
        for child in &mut self.children {
            child.clone_instance(i);
        }
        self.instances.len()
    }

    fn get_animation(&self) -> &Vec<ModelAnimation> {
        &self.animations
    }

    fn get_render(&self) -> Vec<Instanced<'_>> {
        self.children
            .iter()
            .flat_map(|child| child.get_render())
            .chain([Instanced {
                instance: &self.instance_buffer,
                model: &self.model,
                amount: self.instances.len(),
                id: self.id,
            }])
            .collect()
    }

    fn remove_instance(&mut self, idx: usize) -> (Instance, Instance) {
        self.children.iter_mut().for_each(|c| {
            c.remove_instance(idx);
        });
        self.buffer_size_needs_change = true;
        self.instances.remove(idx)
    }

    fn add_instances(&mut self, instances: Vec<Instance>) -> usize {
        let cloned = instances.clone();
        let len = instances.len();
        let mut instances = instances.into_iter().zip(cloned).collect();
        self.instances.append(&mut instances);
        for child in &mut self.children {
            child.add_instances((0..len).map(|_| Instance::default()).collect());
        }
        self.buffer_size_needs_change = true;
        self.instances.len()
    }
}

pub async fn mk_flat_scene_graph(
    amount: usize,
    id: u32,
    models: Vec<&'static str>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Box<dyn SceneNode> {
    let mut parent: Box<dyn SceneNode> = Box::new(ContainerNode::new(amount, Vec::new()));
    futures::future::join_all(
        models
            .into_iter()
            .map(|obj_file| ModelNode::new(amount, id, device, queue, obj_file)),
    )
    .await
    .into_iter()
    .map(Box::new)
    .for_each(|boxed_model_node| parent.add_child(boxed_model_node));
    parent
}
