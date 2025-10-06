use std::{
    ops::BitAnd,
    time::{Duration, Instant},
};

use cgmath::AbsDiffEq;

use crate::data_structures::{instance::Instance, scene_graph::SceneNode};

const EPSILON: f32 = 1e-2;

#[derive(Clone, Debug)]
pub enum Keyframes {
    Translation(Vec<cgmath::Vector3<f32>>),
    Rotation(Vec<cgmath::Quaternion<f32>>),
    Scale(Vec<cgmath::Vector3<f32>>),
    Other,
}

pub struct Animation {
    speed: f32,
    time: Instant,
}

impl<'a> Animation {
    pub fn new(speed: f32) -> Self {
        let time = Instant::now();
        Self { speed, time }
    }

    /**
     * This function checks whether the passed Scene Graph contains animation data and plays it
     * according to the time passed since this `Animation` struct was initialized.
     * 
     * Repeats the animation after 20s (TODO: make this a parameter)
     * 
     * TODO: interpolate similar to `animate_with(...args)`
     */
    pub fn animate(
        &mut self,
        graph: &'a mut Box<dyn SceneNode>,
        anim_idx: usize,
        queue: &wgpu::Queue,
        device: &wgpu::Device,
    ) {
        let current_time = &mut self.time;
        animate_graph(graph, anim_idx, current_time, self.speed);
        graph.update_world_transform_all();
        graph.write_to_buffers(queue, device);

        // repeat anim after x secs
        if self.time.elapsed().as_secs_f32() > 20.0 {
            self.time = Instant::now();
        }
    }

    /**
     * This function animates a single frame with interpolation between the current position of
     * the screne_graph and the position given in the animation List.
     * 
     * `graph` is the Scene Graph of the object to model
     * `current` is a mapping between Scene Graph nodes and the passed animation positions
     * `reference` is the desired position
     * `idx` is the index of the instance to animate
     * `dt` the duration since the last rendered frame
     */
    pub fn animate_with(
        &mut self,
        graph: &'a mut Box<dyn SceneNode>,
        current: &[&[usize]],
        reference: &[&Instance],
        idx: usize,
        dt: Duration,
    ) -> bool {
        let mut all_lts = Vec::new();
        let speed = self.speed.clone();

        for (curr, ref_pos) in current.iter().zip(reference) {
            let scene_node = curr
                .into_iter()
                .fold(&mut *graph, |g, &i| &mut g.get_children_mut()[i]);

            if let Some(local_transform) = scene_node.get_local_transform(idx) {
                let lt_epsilon = diff_lt_epsilon(&local_transform, ref_pos);
                all_lts.push(lt_epsilon);
                if !lt_epsilon {
                    let new_transform: Instance =
                        step(&local_transform, ref_pos, dt.as_secs_f32(), speed);
                    scene_node.set_local_transform(idx, new_transform);
                }
            } else {
                println!("Warning, animation with index {} not found.", idx)
            }
        }
        all_lts.into_iter().fold(true, BitAnd::bitand)
    }
}

fn animate_graph(graph: &mut Box<dyn SceneNode>, anim_idx: usize, time: &mut Instant, speed: f32) {
    let current_time = time.elapsed().as_secs_f32();
    let animations = graph.get_animation();
    let mut current_keyframe_index = 0;
    // pick desired animation
    if let Some(animation) = &animations.get(anim_idx) {
        // invariant: timestamp matches animations
        for timestamp in &animation.timestamps {
            // keyframe overdue
            if timestamp > &current_time {
                break;
            }
            if &current_keyframe_index < &(&animation.timestamps.len() - 1) {
                current_keyframe_index += 1;
            }
        }

        // Update locals with current animation
        // TODO: add something to animate different instances independently
        let ref_pos = &animation.instances[current_keyframe_index];
        graph.set_local_transform(0, ref_pos.clone());

    }

    for child in graph.get_children_mut() {
        animate_graph(child, anim_idx, time, speed);
    }
}

// linear interpolation between two positions
fn step(fst: &Instance, snd: &Instance, dt: f32, speed: f32) -> Instance {
    let position = fst.position + (snd.position - fst.position) * dt * speed;
    let rotation = fst.rotation.nlerp(snd.rotation, dt * speed);
    let scale = fst.scale + (snd.scale - fst.scale) * dt * speed;

    Instance {
        position,
        rotation,
        scale,
    }
}

fn diff_lt_epsilon(fst: &Instance, snd: &Instance) -> bool {
    let pos_diff = fst.position.abs_diff_eq(&snd.position, EPSILON);
    let rot_diff = fst.rotation.abs_diff_eq(&snd.rotation, EPSILON);
    let scale_diff = fst.scale.abs_diff_eq(&snd.scale, EPSILON);
    pos_diff && rot_diff && scale_diff
}
