//! Scene graph and hierarchical scene organization.
//!
//! Provides traits and structures for building a scene graph: a hierarchical
//! representation of objects in a scene, including animation support and
//! renderable object composition.

use std::{collections::HashMap, ops::Range};

use cgmath::{InnerSpace, SquareMatrix, Zero};
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
    let animations = match anims.get(&node.index()) {
        Some(clips) => merge(clips.clone()),
        None => Default::default(),
    };

    let mut scene_node: Box<dyn SceneNode> = match node.mesh() {
        Some(mesh) => {
            let mut meshes = Vec::new();

            for primitive in mesh.primitives() {
                let reader = primitive.reader(|buffer| Some(&buf[buffer.index()]));

                let positions: Vec<[f32; 3]> = reader.read_positions().unwrap().collect();

                let normals: Option<Vec<[f32; 3]>> = reader.read_normals().map(|n| n.collect());

                let tex_coords: Option<Vec<[f32; 2]>> =
                    reader.read_tex_coords(0).map(|uv| uv.into_f32().collect());

                let tangents: Option<Vec<[f32; 4]>> = reader.read_tangents().map(|t| t.collect());

                let indices: Vec<u32> = if let Some(indices) = reader.read_indices() {
                    indices.into_u32().collect()
                } else {
                    (0..positions.len() as u32).collect()
                };

                let mut vertices = Vec::with_capacity(indices.len());

                for &i in &indices {
                    let i = i as usize;

                    let position = positions[i];
                    let normal = normals.as_ref().map(|n| n[i]).unwrap_or([0.0, 1.0, 0.0]);

                    let tex_coords = tex_coords.as_ref().map(|uv| uv[i]).unwrap_or([0.0, 0.0]);

                    let (tangent, bitangent) = if let Some(t) = tangents.as_ref() {
                        let t4 = t[i];
                        let t3 = [t4[0], t4[1], t4[2]];
                        let n = cgmath::Vector3::from(normal);
                        let t = cgmath::Vector3::from(t3);
                        let b = n.cross(t) * t4[3];

                        (t3, b.into())
                    } else {
                        ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0])
                    };

                    vertices.push(model::ModelVertex {
                        position,
                        normal,
                        tex_coords,
                        tangent,
                        bitangent,
                    });
                }

                // =========================
                // FIX 3: Index buffer is now trivial
                // (vertices already expanded)
                // =========================
                let gpu_indices: Vec<u32> = (0..vertices.len() as u32).collect();

                let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("{:?} Vertex Buffer", mesh.name())),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("{:?} Index Buffer", mesh.name())),
                    contents: bytemuck::cast_slice(&gpu_indices),
                    usage: wgpu::BufferUsages::INDEX,
                });

                let mat_idx = primitive.material().index().unwrap_or(0);

                meshes.push(model::Mesh {
                    name: mesh.name().unwrap_or("unknown_mesh").to_string(),
                    vertex_buffer,
                    index_buffer,
                    num_elements: gpu_indices.len() as u32,
                    material: mat_idx,
                });
            }

            let model = model::Model {
                meshes,
                materials: mats.clone(),
            };

            Box::new(ModelNode::from_model(1, id, device, model, animations))
        }
        None => Box::new(ContainerNode::new(1, animations)),
    };

    let (pos, rot, scale) = node.transform().decomposed();

    let instance = Instance {
        position: pos.into(),
        rotation: rot.into(),
        scale: scale.into(),
    };

    scene_node.set_local_transform(0, instance);

    for child in node.children() {
        let child_node = to_scene_node(id, child, buf, device, mats, anims);
        scene_node.add_child(child_node);
    }

    scene_node
}

fn compute_tangents(vertices: &mut Vec<model::ModelVertex>, indices: &[u32]) {
    // 1. Allocate temporary storage for tangent and bitangent accumulators.
    // We need these to accumulate contributions from all triangles sharing a vertex.
    let mut tan1 = vec![cgmath::Vector3::zero(); vertices.len()];
    let mut tan2 = vec![cgmath::Vector3::zero(); vertices.len()];

    // 2. Iterate over all triangles (chunks of 3 indices)
    for c in indices.chunks(3) {
        if c.len() < 3 {
            break;
        } // Safety check

        let i1 = c[0] as usize;
        let i2 = c[1] as usize;
        let i3 = c[2] as usize;

        let v1 = &vertices[i1];
        let v2 = &vertices[i2];
        let v3 = &vertices[i3];

        let p1: cgmath::Vector3<f32> = v1.position.into();
        let p2: cgmath::Vector3<f32> = v2.position.into();
        let p3: cgmath::Vector3<f32> = v3.position.into();

        let w1: cgmath::Vector2<f32> = v1.tex_coords.into();
        let w2: cgmath::Vector2<f32> = v2.tex_coords.into();
        let w3: cgmath::Vector2<f32> = v3.tex_coords.into();

        let x1 = p2.x - p1.x;
        let x2 = p3.x - p1.x;
        let y1 = p2.y - p1.y;
        let y2 = p3.y - p1.y;
        let z1 = p2.z - p1.z;
        let z2 = p3.z - p1.z;

        let s1 = w2.x - w1.x;
        let s2 = w3.x - w1.x;
        let t1 = w2.y - w1.y;
        let t2 = w3.y - w1.y;

        // Prevent division by zero if UVs are degenerate
        let r_denom = s1 * t2 - s2 * t1;
        let r = if r_denom.abs() < 1e-6 {
            0.0
        } else {
            1.0 / r_denom
        };

        let sdir = cgmath::Vector3::new(
            (t2 * x1 - t1 * x2) * r,
            (t2 * y1 - t1 * y2) * r,
            (t2 * z1 - t1 * z2) * r,
        );

        let tdir = cgmath::Vector3::new(
            (s1 * x2 - s2 * x1) * r,
            (s1 * y2 - s2 * y1) * r,
            (s1 * z2 - s2 * z1) * r,
        );

        // Accumulate for each vertex of the triangle
        tan1[i1] += sdir;
        tan1[i2] += sdir;
        tan1[i3] += sdir;

        tan2[i1] += tdir;
        tan2[i2] += tdir;
        tan2[i3] += tdir;
    }

    for (i, vert) in vertices.iter_mut().enumerate() {
        let n: cgmath::Vector3<f32> = vert.normal.into();
        let t = tan1[i];

        // Gram-Schmidt orthogonalize:
        let tangent_xyz = (t - n * n.dot(t)).normalize();

        let w = if n.cross(t).dot(tan2[i]) < 0.0 {
            -1.0
        } else {
            1.0
        };

        if tangent_xyz.x.is_nan() {
            vert.tangent = [1.0, 0.0, 0.0];
            vert.bitangent = [0.0, 1.0, 0.0];
        } else {
            vert.tangent = tangent_xyz.into();
            let bitangent = n.cross(tangent_xyz) * w;
            vert.bitangent = bitangent.into();
        }
    }
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

    fn get_world_transform(&self, idx: usize) -> Option<&Instance>;

    fn get_local_transform(&self, idx: usize) -> Option<&Instance>;

    fn draw<'a, 'pass>(
        &self,
        camera_bind_group_layout: &'a wgpu::BindGroup,
        light_bind_group: &'a wgpu::BindGroup,
        render_pass: &'pass mut wgpu::RenderPass<'a>,
    ) where
        'a: 'pass;

    fn to_clickable(&self, device: &wgpu::Device, id: u32) -> Box<dyn SceneNode>;

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

    fn clone_instance(&mut self, i: usize) -> usize;

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

#[cfg(feature = "integration-tests")]
impl<'a, 'pass> GPUResource<'a, 'pass> for Box<dyn SceneNode> {
    fn write_to_buffer(&mut self, queue: &wgpu::Queue, device: &wgpu::Device) {
        // Delegate to the inner dyn SceneNode
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
        let direction = wgpu::FrontFace::Ccw;

        Self {
            children: vec![],
            front_face: direction,
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

    fn to_clickable(&self, device: &wgpu::Device, id: u32) -> Box<dyn SceneNode> {
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
            id,
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

    fn clone_instance(&mut self, i: usize) -> usize {
        self.instances
            .push((self.instances[i].clone().0, self.instances[i].clone().1));
        for child in &mut self.children {
            child.clone_instance(i);
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
    id: u32,
    models: Vec<&'static str>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> anyhow::Result<Box<dyn SceneNode>> {
    let mut parent: Box<dyn SceneNode> = Box::new(ContainerNode::new(amount, Vec::new()));
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
    fn existing_children_unchanged() {
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
}
