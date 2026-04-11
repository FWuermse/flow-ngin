//! Scene graph and hierarchical scene organization.
//!
//! Provides traits and structures for building a scene graph: a hierarchical
//! representation of objects in a scene, including animation support and
//! renderable object composition.

use std::{collections::HashMap, ops::Range};

use cgmath::SquareMatrix;
use log::warn;
use wgpu::{Device, Queue, util::DeviceExt};

use crate::{
    context::GPUResource, data_structures::{
        instance::{Instance, InstanceRaw},
        model::{self, DrawModel},
    }, pick::PickId, render::{Instanced, Render}, resources::{animation::Keyframes, load_model_obj, mesh::compute_tangents, pick::load_pick_model}
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
    id: impl Into<PickId>,
    node: gltf::scene::Node,
    buf: &Vec<Vec<u8>>,
    device: &wgpu::Device,
    mats: &Vec<model::Material>,
    anims: &HashMap<usize, Vec<AnimationClip>>,
) -> Box<dyn SceneNode> {
    let animations = match anims.get(&node.index()) {
        Some(clips) => merge(clips.clone()),
        None => Default::default(),
    };
    let id = id.into();
    // TODO: only select materials for current mesh
    let mut scene_node: Box<dyn SceneNode> = match node.mesh() {
        Some(mesh) => {
            let mut meshes = Vec::new();
            let primitives = mesh.primitives();

            primitives.for_each(|primitive| {
                let reader = primitive.reader(|buffer| Some(&buf[buffer.index()]));

                let mut indices = Vec::new();
                if let Some(indices_raw) = reader.read_indices() {
                    indices.append(&mut indices_raw.into_u32().collect::<Vec<u32>>());
                } else {
                    if let Some(positions) = reader.read_positions() {
                        indices = (0..positions.len() as u32).collect();
                    }
                }

                let mut vertices = Vec::with_capacity(indices.len());
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
                let texcoord_set = primitive
                    .material()
                    .pbr_metallic_roughness()
                    .base_color_texture()
                    .map(|t| t.tex_coord())
                    .unwrap_or(0);
                if let Some(tex_coord_attribute) = reader.read_tex_coords(texcoord_set).map(|v| v.into_f32()) {
                    let mut tex_coord_index = 0;
                    tex_coord_attribute.for_each(|tex_coord| {
                        vertices[tex_coord_index].tex_coords = tex_coord;

                        tex_coord_index += 1;
                    });
                }
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
                } else {
                    if !indices.is_empty() && !vertices.is_empty() {
                        compute_tangents(&mut vertices, &indices);
                    }
                };

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
                let mat_idx = primitive.material().index().unwrap_or(0);

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
    let instance = instance_from_gltf(decomp_pos.0, decomp_pos.1.into(), decomp_pos.2);
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
        // Use first frame as default (this is important as child nodes have offsets).
        // If a track is entirely empty, fill with identity/zero defaults.
        let default_trans = cgmath::Vector3::new(0.0, 0.0, 0.0);
        let default_rot = cgmath::Quaternion::new(1.0, 0.0, 0.0, 0.0);
        let default_scale = cgmath::Vector3::new(1.0, 1.0, 1.0);
        state.trans.resize(
            max_len,
            state.trans.first().copied().unwrap_or(default_trans),
        );
        state.rots.resize(
            max_len,
            state.rots.first().copied().unwrap_or(default_rot),
        );
        state.scals.resize(
            max_len,
            state.scals.first().copied().unwrap_or(default_scale),
        );
    }
    // now assume they're all the same length
    let mut instances = Vec::with_capacity(max_len);
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
    if clips.is_empty() {
        return Vec::new();
    }
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

pub trait SceneNode: Send {
    fn get_world_transforms(&self) -> Vec<Instance>;

    fn get_world_transform(&self, idx: usize) -> Option<&Instance>;

    fn get_local_transform(&self, idx: usize) -> Option<&Instance>;

    fn draw<'a, 'pass>(
        &self,
        camera_bind_group_layout: &'a wgpu::BindGroup,
        light_bind_group: &'a wgpu::BindGroup,
        render_pass: &'pass mut wgpu::RenderPass<'a>,
    ) where
        'a: 'pass;

    fn to_clickable(&self, device: &wgpu::Device, id: PickId) -> Box<dyn SceneNode>;

    fn get_children(&self) -> &Vec<Box<dyn SceneNode>>;

    /// Adds a child node to the tree and returns the childs index
    fn add_child(&mut self, child: Box<dyn SceneNode>) -> usize;

    fn remove_child(&mut self, idx: usize) -> Box<dyn SceneNode>;

    fn set_local_transform(&mut self, idx: usize, instance: Instance);

    fn set_local_transform_all(&mut self, mutation: &mut dyn FnMut(&mut Instance));

    fn get_children_mut(&mut self) -> &mut Vec<Box<dyn SceneNode>>;

    fn write_to_buffers(&mut self, queue: &wgpu::Queue, device: &wgpu::Device);

    /// Multiple instances of a parent can be passed down to multiple instances of multiple children.
    /// The argument `parents_world_transform` with a matching `range` size provides control over which instances are transformed.
    fn update_world_transforms(
        &mut self,
        range: Range<usize>,
        parents_world_transform: &Vec<Instance>,
    );

    fn update_world_transform_all(&mut self);

    /// Adds an instance to the scene node (and its children) and returns the index of the added instance
    fn add_instance(&mut self, instance: Instance) -> usize;

    /// Adds multiple instance to the scene node (and its children) and returns index of the last instance
    fn add_instances(&mut self, instances: Vec<Instance>) -> usize;

    fn set_instances(&mut self, instances: Vec<Instance>) -> usize;

    fn remove_instance(&mut self, idx: usize) -> (Instance, Instance);

    fn duplicate_instance(&mut self, i: usize) -> usize;

    fn get_animation(&self) -> &Vec<ModelAnimation>;

    fn get_render(&self) -> Vec<Instanced<'_>>;

    fn get_render_dir(&self) -> wgpu::FrontFace {
        wgpu::FrontFace::Ccw
    }

    fn render_inverted(&mut self);
}
impl dyn SceneNode {
    pub fn transform_local(&mut self, instance: Instance) -> Instance {
        let idx = self.add_child(Box::new(ContainerNode::from(instance)));
        self.update_world_transforms(idx..idx + 1, &vec![Instance::new()]);
        let child = self.remove_child(idx);
        child.get_world_transforms()[0].clone()
    }
    pub fn transform_locals(&mut self, instances: Vec<Instance>) -> Vec<Instance> {
        self.add_instances((0..instances.len()).map(|_| Instance::new()).collect());
        let idx = self.add_child(Box::new(ContainerNode::from(instances)));
        self.update_world_transforms(idx..idx + 1, &vec![Instance::new()]);
        let child = self.remove_child(idx);
        child.get_world_transforms()
    }
}

/// Returns the local transformation of `children` in the `parent` space coordinates
pub fn transform_locals(parent: &Instance, children: Vec<Instance>) -> Vec<Instance> {
    let len = children.len();
    let parents: Vec<_> = (0..len).map(|_| parent.clone()).collect();
    let mut scene = ContainerNode::from(parents);
    let child = scene.add_child(Box::new(ContainerNode::from(children)));
    scene.update_world_transforms(0..len, &(0..len).map(|_| Instance::new()).collect());
    scene.remove_child(child).get_world_transforms()
}

/// Returns the local transformation `child` in the `parent` coordinates
pub fn transform_local(parent: &Instance, child: Instance) -> Instance {
    parent * &child
}

/// Constructs an `Instance` from a glTF decomposed transform tuple.
pub(crate) fn instance_from_gltf(
    translation: [f32; 3],
    rotation: cgmath::Quaternion<f32>,
    scale: [f32; 3],
) -> Instance {
    Instance {
        position: translation.into(),
        rotation,
        scale: scale.into(),
    }
}

#[cfg(feature = "integration-tests")]
impl<'a, 'pass> GPUResource<'a, 'pass> for Box<dyn SceneNode> {
    fn write_to_buffer(&mut self, queue: &wgpu::Queue, device: &wgpu::Device) {
        (*self).write_to_buffers(queue, device);
    }
    fn get_render(&'a self) -> Render<'a, 'pass> {
        Render::Defaults((**self).get_render())
    }
}

#[cfg(feature = "integration-tests")]
impl<'a, 'pass> GPUResource<'a, 'pass> for Box<dyn SceneNode + Send> {
    fn write_to_buffer(&mut self, queue: &wgpu::Queue, device: &wgpu::Device) {
        (*self).write_to_buffers(queue, device);
    }
    fn get_render(&'a self) -> Render<'a, 'pass> {
        Render::Defaults((**self).get_render())
    }
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

impl From<Instance> for ContainerNode {
    fn from(value: Instance) -> Self {
        ContainerNode {
            children: vec![],
            instances: vec![(value, Instance::default())],
            animations: vec![],
        }
    }
}

impl From<Vec<Instance>> for ContainerNode {
    fn from(value: Vec<Instance>) -> Self {
        ContainerNode {
            children: vec![],
            instances: value
                .iter()
                .zip(value.iter())
                .map(|(fst, snd)| (fst.clone(), snd.clone()))
                .collect(),
            animations: vec![],
        }
    }
}

impl SceneNode for ContainerNode {
    fn remove_child(&mut self, idx: usize) -> Box<dyn SceneNode> {
        self.children.remove(idx)
    }

    fn add_child(&mut self, child: Box<dyn SceneNode>) -> usize {
        self.children.push(child);
        return self.children.len() - 1;
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

    fn get_local_transform(&self, idx: usize) -> Option<&Instance> {
        self.instances.get(idx).map(|(local, _)| local)
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

    fn to_clickable(&self, device: &Device, id: PickId) -> Box<dyn SceneNode> {
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
        self.instances.len() - 1
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
    fn duplicate_instance(&mut self, i: usize) -> usize {
        self.instances
            .push((self.instances[i].clone().0, self.instances[i].clone().1));
        for child in &mut self.children {
            child.duplicate_instance(i);
        }
        self.instances.len() - 1
    }

    fn get_animation(&self) -> &Vec<ModelAnimation> {
        &self.animations
    }

    fn get_render(&self) -> Vec<Instanced<'_>> {
        self.children
            .iter()
            .flat_map(|child| (**child).get_render())
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
        self.instances.len() - 1
    }

    fn get_world_transform(&self, idx: usize) -> Option<&Instance> {
        self.instances.get(idx).map(|(_, world)| world)
    }

    fn set_instances(&mut self, instances: Vec<Instance>) -> usize {
        let len = instances.len();
        self.instances = instances.to_vec().into_iter().zip(instances).collect();
        for child in &mut self.children {
            child.set_instances((0..len).map(|_| Instance::default()).collect());
        }
        self.instances.len() - 1
    }

    fn render_inverted(&mut self) {}
}

pub struct ModelNode {
    children: Vec<Box<dyn SceneNode>>,
    front_face: wgpu::FrontFace,
    instance_buffer: wgpu::Buffer,
    instances: Vec<(Instance, Instance)>,
    animations: Vec<ModelAnimation>,
    buffer_size_needs_change: bool,
    model: model::Model,
    id: PickId,
}

impl ModelNode {
    pub async fn new(
        amount: usize,
        id: impl Into<PickId>,
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
        id: impl Into<PickId>,
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
        let direction = wgpu::FrontFace::Ccw;

        Self {
            children: vec![],
            front_face: direction,
            instance_buffer,
            instances,
            model: obj_model,
            buffer_size_needs_change: size_changed,
            animations,
            id: id.into(),
        }
    }
}

impl SceneNode for ModelNode {
    fn add_child(&mut self, child: Box<dyn SceneNode>) -> usize {
        self.children.push(child);
        self.children.len() - 1
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

    fn get_local_transform(&self, idx: usize) -> Option<&Instance> {
        self.instances.get(idx).map(|(local, _)| local)
    }

    fn write_to_buffers(&mut self, queue: &wgpu::Queue, device: &wgpu::Device) {
        // If the underlying model is inverted then so are all instances (TODO: confirm)
        if let Some((_, world)) = self.instances.first() {
            let det = world.to_matrix().determinant().signum();
            if det < 0.0 {
                self.front_face = wgpu::FrontFace::Cw;
            } else {
                self.front_face = wgpu::FrontFace::Ccw;
            }
        }
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

    fn to_clickable(&self, device: &wgpu::Device, id: PickId) -> Box<dyn SceneNode> {
        let obj_model = load_pick_model(&device, id, self.model.meshes.clone()).unwrap();

        let children = self
            .children
            .iter()
            .map(|child| child.to_clickable(device, id))
            .collect();

        Box::new(Self {
            children,
            front_face: self.front_face,
            instance_buffer: self.instance_buffer.clone(),
            instances: self.instances.clone(),
            buffer_size_needs_change: false,
            model: obj_model,
            animations: Vec::new(),
            id: id.into(),
        })
    }

    fn add_instance(&mut self, instance: Instance) -> usize {
        self.instances.push((instance.clone(), instance));
        for child in &mut self.children {
            child.add_instance(Instance::default());
        }
        self.buffer_size_needs_change = true;
        self.instances.len() - 1
    }

    fn update_world_transform_all(&mut self) {
        let range = 0..self.instances.len();
        let default_instances = range.clone().map(|_| Instance::default()).collect();
        self.update_world_transforms(range, &default_instances);
    }

    fn duplicate_instance(&mut self, i: usize) -> usize {
        self.instances
            .push((self.instances[i].clone().0, self.instances[i].clone().1));
        for child in &mut self.children {
            child.duplicate_instance(i);
        }
        self.buffer_size_needs_change = true;
        self.instances.len() - 1
    }

    fn get_animation(&self) -> &Vec<ModelAnimation> {
        &self.animations
    }

    fn get_render(&self) -> Vec<Instanced<'_>> {
        self.children
            .iter()
            .flat_map(|child| (**child).get_render())
            .chain([Instanced {
                instance: &self.instance_buffer,
                model: &self.model,
                amount: self.instances.len(),
                front_face: self.front_face,
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
        self.instances.len() - 1
    }

    fn get_world_transform(&self, idx: usize) -> Option<&Instance> {
        self.instances.get(idx).map(|(_, world)| world)
    }

    fn remove_child(&mut self, idx: usize) -> Box<dyn SceneNode> {
        self.children.remove(idx)
    }

    fn set_instances(&mut self, instances: Vec<Instance>) -> usize {
        let len = instances.len();
        self.instances = instances.to_vec().into_iter().zip(instances).collect();
        for child in &mut self.children {
            child.set_instances((0..len).map(|_| Instance::default()).collect());
        }
        self.buffer_size_needs_change = true;
        self.instances.len() - 1
    }

    fn render_inverted(&mut self) {
        self.front_face = wgpu::FrontFace::Cw;
    }
}

pub async fn mk_flat_scene_graph(
    amount: usize,
    id: impl Into<PickId>,
    models: Vec<&'static str>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> anyhow::Result<Box<dyn SceneNode + Send>> {
    let mut parent: Box<dyn SceneNode> = Box::new(ContainerNode::new(amount, Vec::new()));
    let id = id.into();
    futures::future::join_all(
        models
            .into_iter()
            .map(|obj_file| ModelNode::new(amount, id, device, queue, obj_file)),
    )
    .await
    .into_iter()
    .map(Box::new)
    .for_each(|boxed_model_node| {
        parent.add_child(boxed_model_node);
    });
    Ok(parent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_number_of_children() {
        let parent = Instance::default();
        let children = vec![
            Instance::default(),
            Instance::default(),
            Instance::default(),
        ];

        let result = transform_locals(&parent, children.clone());

        assert_eq!(result.len(), children.len());
    }

    #[test]
    fn identity_parent_does_not_change_children() {
        let parent = Instance::default();
        let children = vec![
            Instance::from(cgmath::Vector3::from([1.0, 0.0, 0.0])),
            Instance::from(cgmath::Vector3::from([0.0, 2.0, 0.0])),
        ];

        let result = transform_locals(&parent, children.clone());

        for (a, b) in result.iter().zip(children.iter()) {
            assert_eq!(a.position, b.position);
            assert_eq!(a.scale, b.scale);
            assert_eq!(a.rotation, b.rotation);
        }
    }

    #[test]
    fn parent_translation_is_removed() {
        let parent = Instance::from(cgmath::Vector3::from([10.0, 0.0, 0.0]));

        let children = vec![
            Instance::from(cgmath::Vector3::from([1.0, 0.0, 0.0])),
            Instance::from(cgmath::Vector3::from([2.0, 0.0, 0.0])),
        ];

        let result = transform_locals(&parent, children);

        let expected = vec![
            Instance::from(cgmath::Vector3::from([11.0, 0.0, 0.0])),
            Instance::from(cgmath::Vector3::from([12.0, 0.0, 0.0])),
        ];

        for (a, b) in result.iter().zip(expected.iter()) {
            assert_eq!(a.position, b.position);
            assert_eq!(a.scale, b.scale);
            assert_eq!(a.rotation, b.rotation);
        }
    }

    #[test]
    fn empty_children_returns_empty() {
        let parent = Instance::default();
        let children = Vec::new();

        let result = transform_locals(&parent, children);

        assert!(result.is_empty());
    }

    #[test]
    fn non_identity_parent_translates_child() {
        let parent = Instance::from(cgmath::Vector3::from([1.0, 0.0, 0.0]));
        let child = Instance::from(cgmath::Vector3::from([0.0, 0.0, 0.0]));
        let result = transform_local(&parent, child);
        assert_eq!(result.position, cgmath::Vector3::new(1.0, 0.0, 0.0));
    }

    #[test]
    fn parent_scale_scales_child_position() {
        let mut parent = Instance::default();
        parent.scale = cgmath::Vector3::new(2.0, 2.0, 2.0);
        let child = Instance::from(cgmath::Vector3::from([1.0, 0.0, 0.0]));
        let result = transform_local(&parent, child);
        use cgmath::assert_relative_eq;
        assert_relative_eq!(result.position.x, 2.0, epsilon = 1e-5);
        assert_relative_eq!(result.position.y, 0.0, epsilon = 1e-5);
        assert_relative_eq!(result.position.z, 0.0, epsilon = 1e-5);
    }

    #[test]
    fn parent_rotation_rotates_child_position() {
        use cgmath::{assert_relative_eq, Deg, Quaternion, Rotation3, Vector3};
        let mut parent = Instance::default();
        parent.rotation = Quaternion::from_axis_angle(Vector3::new(0.0, 1.0, 0.0), Deg(90.0));
        let child = Instance::from(cgmath::Vector3::from([1.0, 0.0, 0.0]));
        let result = transform_local(&parent, child);
        assert_relative_eq!(result.position.x, 0.0, epsilon = 1e-5);
        assert_relative_eq!(result.position.y, 0.0, epsilon = 1e-5);
        assert_relative_eq!(result.position.z, -1.0, epsilon = 1e-5);
    }

    #[test]
    fn transform_locals_matches_sequential() {
        let parent = Instance::from(cgmath::Vector3::from([3.0, 0.0, 0.0]));
        let a = Instance::from(cgmath::Vector3::from([1.0, 0.0, 0.0]));
        let b = Instance::from(cgmath::Vector3::from([0.0, 2.0, 0.0]));
        let batch = transform_locals(&parent, vec![a.clone(), b.clone()]);
        let seq_a = transform_local(&parent, a);
        let seq_b = transform_local(&parent, b);
        assert_eq!(batch[0].position, seq_a.position);
        assert_eq!(batch[1].position, seq_b.position);
    }

    #[test]
    fn double_transform_equals_composed() {
        use cgmath::assert_relative_eq;
        let p = Instance::from(cgmath::Vector3::from([1.0, 0.0, 0.0]));
        let q = Instance::from(cgmath::Vector3::from([0.0, 1.0, 0.0]));
        let c = Instance::from(cgmath::Vector3::from([0.0, 0.0, 1.0]));
        let nested = transform_local(&p, transform_local(&q, c.clone()));
        let composed_parent = {
            use std::ops::Mul;
            (&p).mul(&q)
        };
        let direct = transform_local(&composed_parent, c);
        assert_relative_eq!(nested.position.x, direct.position.x, epsilon = 1e-5);
        assert_relative_eq!(nested.position.y, direct.position.y, epsilon = 1e-5);
        assert_relative_eq!(nested.position.z, direct.position.z, epsilon = 1e-5);
    }

    // --- instance_from_gltf ---

    #[test]
    fn identity_decomp() {
        use cgmath::{One, Quaternion, assert_relative_eq};
        let result = instance_from_gltf([0.0, 0.0, 0.0], Quaternion::one(), [1.0, 1.0, 1.0]);
        let expected = Instance::new();
        assert_relative_eq!(result.position.x, expected.position.x, epsilon = 1e-6);
        assert_relative_eq!(result.rotation.s, expected.rotation.s, epsilon = 1e-6);
        assert_relative_eq!(result.scale.x, expected.scale.x, epsilon = 1e-6);
    }

    #[test]
    fn translation_preserved() {
        use cgmath::{One, Quaternion};
        let t = [3.0f32, 4.0, 5.0];
        let result = instance_from_gltf(t, Quaternion::one(), [1.0, 1.0, 1.0]);
        assert_eq!(result.position.x, t[0]);
        assert_eq!(result.position.y, t[1]);
        assert_eq!(result.position.z, t[2]);
    }

    #[test]
    fn scale_preserved() {
        use cgmath::{One, Quaternion};
        let s = [2.0f32, 3.0, 4.0];
        let result = instance_from_gltf([0.0, 0.0, 0.0], Quaternion::one(), s);
        assert_eq!(result.scale.x, s[0]);
        assert_eq!(result.scale.y, s[1]);
        assert_eq!(result.scale.z, s[2]);
    }

    #[test]
    fn rotation_preserved() {
        use cgmath::{Deg, Quaternion, Rotation3, Vector3};
        let q = Quaternion::from_axis_angle(Vector3::new(0.0, 1.0, 0.0), Deg(45.0));
        let result = instance_from_gltf([0.0, 0.0, 0.0], q, [1.0, 1.0, 1.0]);
        assert_eq!(result.rotation, q);
    }

    // duplicate_instance must return the index of the newly created instance (len-1),
    // not len which is out of bounds.
    #[test]
    fn duplicate_instance_returns_valid_index() {
        let mut node = ContainerNode::new(2, Vec::new());
        let returned_idx = node.duplicate_instance(0);
        assert_eq!(
            returned_idx,
            node.instances.len() - 1,
            "duplicate_instance must return len-1 (the new instance's index)"
        );
        assert!(
            node.instances.get(returned_idx).is_some(),
            "returned index must be valid"
        );
    }

    // merge must not panic on empty input
    #[test]
    fn merge_empty_clips_returns_empty() {
        let result = merge(vec![]);
        assert!(result.is_empty());
    }

    // When only one track type has data, save_current_anim must pad the other
    // tracks to the same length rather than panicking on out-of-bounds access.
    #[test]
    fn save_current_anim_pads_missing_tracks() {
        use cgmath::{One, Quaternion};
        let clips = vec![AnimationClip {
            name: "anim1".into(),
            keyframes: Keyframes::Rotation(vec![Quaternion::one(), Quaternion::one()]),
            timestamps: vec![0.0, 1.0],
        }];
        let animations = merge(clips);
        assert_eq!(animations.len(), 1);
        assert_eq!(
            animations[0].instances.len(),
            2,
            "instances must match the rotation track length"
        );
    }
}


#[cfg(kani)]
mod kani_proofs {
    use super::*;
    use cgmath::{One, Quaternion, Vector3};

    fn bounded_instance() -> Instance {
        let px: f32 = kani::any();
        let py: f32 = kani::any();
        let pz: f32 = kani::any();
        kani::assume(px.abs() < 1e4 && py.abs() < 1e4 && pz.abs() < 1e4);
        let sx: f32 = kani::any();
        let sy: f32 = kani::any();
        let sz: f32 = kani::any();
        kani::assume(sx.abs() < 1e4 && sy.abs() < 1e4 && sz.abs() < 1e4);
        Instance {
            position: Vector3::new(px, py, pz),
            rotation: Quaternion::one(),
            scale: Vector3::new(sx, sy, sz),
        }
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn verify_transform_local_no_panic() {
        let parent = bounded_instance();
        let child = bounded_instance();
        let _ = transform_local(&parent, child);
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn verify_identity_parent() {
        let identity = Instance::new();
        let c = bounded_instance();
        let result = transform_local(&identity, c.clone());
        kani::assert(
            (result.position.x - c.position.x).abs() < 1e-3,
            "identity parent preserves position.x",
        );
        kani::assert(
            (result.scale.x - c.scale.x).abs() < 1e-3,
            "identity parent preserves scale.x",
        );
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn verify_instance_from_gltf_no_panic() {
        let tx: f32 = kani::any();
        let ty: f32 = kani::any();
        let tz: f32 = kani::any();
        kani::assume(tx.is_finite() && ty.is_finite() && tz.is_finite());
        let sx: f32 = kani::any();
        let sy: f32 = kani::any();
        let sz: f32 = kani::any();
        kani::assume(sx.is_finite() && sy.is_finite() && sz.is_finite());
        let _ = instance_from_gltf([tx, ty, tz], Quaternion::one(), [sx, sy, sz]);
    }

    // verify_transform_locals_length requires kani::unwind(4) for the Vec allocation
    // and is omitted here because transform_locals uses ContainerNode which involves
    // dynamic dispatch and heap allocation that exceeds Kani's modelling scope.
}
