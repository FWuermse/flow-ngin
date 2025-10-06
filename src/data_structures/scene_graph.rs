use std::{collections::HashMap, ops::Range};

use log::warn;
use wgpu::{util::DeviceExt, Device, Queue};

use crate::{
    data_structures::{instance::{Instance, InstanceRaw}, model::{self, DrawModel}}, resources::{self, animation::Keyframes, load_model_obj, pick::load_pick_model}
};

#[derive(Clone, Debug)]
pub struct AnimationClip {
    pub name: String,
    pub keyframes: Keyframes,
    pub timestamps: Vec<f32>,
}

#[derive(Clone, Debug)]
pub struct ModelAnimation {
    pub name: String,
    pub instances: Vec<Instance>,
    pub timestamps: Vec<f32>,
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
        parents_world_transform: &Vec<Instance>,
        range: Range<usize>,
    );

    fn update_world_transform_all(&mut self);

    fn add_instance(&mut self, instance: Instance) -> usize;

    fn clone_instance(&mut self, i: usize) -> usize;

    fn get_animation(&self) -> &Vec<ModelAnimation>;
}

pub struct ContainerNode {
    pub children: Vec<Box<dyn SceneNode>>,
    pub instances: Vec<(Instance, Instance)>,
    animations: Vec<ModelAnimation>,
}

impl ContainerNode {
    pub fn new(amount: u32, animations: Vec<ModelAnimation>) -> Self {
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
        parents_world_transform: &Vec<Instance>,
        range: Range<usize>,
    ) {
        if parents_world_transform.len() > self.instances.len() {
            warn!(
                "You tried to transform with len {}, but there are only {} instances to transform.",
                parents_world_transform.len(),
                self.instances.len()
            );
            println!(
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
            println!(
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
            child.update_world_transforms(&world_transforms, range.clone());
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
        self.update_world_transforms(&default_instances, range);
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
}

pub struct ModelNode {
    children: Vec<Box<dyn SceneNode>>,
    instance_buffer: wgpu::Buffer,
    instances: Vec<(Instance, Instance)>,
    animations: Vec<ModelAnimation>,
    buffer_size_needs_change: bool,
    obj_model: model::Model,
}

impl ModelNode {
    pub async fn new(amount: u32, device: &Device, queue: &Queue, obj_file: &str) -> Self {
        let obj_model = load_model_obj(obj_file, &device, &queue).await;
        if let Err(e) = obj_model {
            panic!("Error failed to load model: {}, at {}", e, obj_file);
        }
        let obj_model = obj_model.unwrap();

        Self::from_model(amount, device, obj_model, Vec::new())
    }

    pub fn from_model(
        amount: u32,
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
            obj_model,
            buffer_size_needs_change: size_changed,
            animations,
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
        parents_world_transform: &Vec<Instance>,
        range: Range<usize>,
    ) {
        if parents_world_transform.len() > self.instances.len() {
            warn!(
                "You tried to transform with len {}, but there are only {} instances to transform.",
                parents_world_transform.len(),
                self.instances.len()
            );
            println!(
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
            println!(
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
            child.update_world_transforms(&world_transforms, range.clone());
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
                &self.obj_model,
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
        let obj_model =
            load_pick_model(&device, id, self.obj_model.meshes.clone()).unwrap();

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
            obj_model,
            animations: Vec::new(),
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
        self.update_world_transforms(&default_instances, range);
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
}

pub async fn mk_flat_scene_graph(
    amount: u32,
    models: Vec<&'static str>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Box<dyn SceneNode> {
    let mut parent: Box<dyn SceneNode> = Box::new(ContainerNode::new(amount, Vec::new()));
    for obj_name in models {
        let child = Box::new(ModelNode::new(amount, device, queue, obj_name).await);
        parent.add_child(child);
    }
    parent
}
