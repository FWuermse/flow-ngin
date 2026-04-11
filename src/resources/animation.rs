use std::ops::BitAnd;

use instant::{Duration, Instant};

use cgmath::{AbsDiffEq, num_traits::Float};

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
    rep_after_sec: f32,
    time: Instant,
}

impl<'a> Animation {
    pub fn new(speed: f32, rep_after_sec: f32) -> Self {
        let time = Instant::now();
        Self {
            speed,
            time,
            rep_after_sec,
        }
    }

    pub fn set_rep_time(&mut self, new_time: f32) {
        self.rep_after_sec = new_time;
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
        instance_idx: usize,
    ) {
        let current_time = &mut self.time;
        let duration = animate_graph(graph, instance_idx, anim_idx, current_time, self.speed);
        self.set_rep_time(duration);

        if self.time.elapsed().as_secs_f32() > self.rep_after_sec {
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
                log::warn!("Warning, animation with index {} not found.", idx)
            }
        }
        all_lts.into_iter().fold(true, BitAnd::bitand)
    }
}

pub(crate) fn find_keyframe_index(timestamps: &[f32], current_time: f32) -> usize {
    let mut idx = 0;
    for timestamp in timestamps {
        if timestamp > &current_time {
            break;
        }
        if idx < timestamps.len().saturating_sub(1) {
            idx += 1;
        }
    }
    idx
}

/// Animates a given `SceneNode` and returns the duration of the longest sub-animation.
fn animate_graph(
    graph: &mut Box<dyn SceneNode>,
    instance_idx: usize,
    anim_idx: usize,
    time: &mut Instant,
    speed: f32,
) -> f32 {
    let current_time = time.elapsed().as_secs_f32();
    let animations = graph.get_animation();
    let mut longest_anim_duration = 0.0;
    // pick desired animation
    if let Some(animation) = &animations.get(anim_idx) {
        if let Some(timestamp) = animation.timestamps.last() {
            longest_anim_duration = longest_anim_duration.max(*timestamp)
        }
        let current_keyframe_index = find_keyframe_index(&animation.timestamps, current_time);

        // Update locals with current animation
        let ref_pos = &animation.instances[current_keyframe_index];
        graph.set_local_transform(instance_idx, ref_pos.clone());
    }

    for child in graph.get_children_mut() {
        let duration = animate_graph(child, instance_idx, anim_idx, time, speed);
        longest_anim_duration = longest_anim_duration.max(duration);
    }
    longest_anim_duration
}

// linear interpolation between two positions
pub(crate) fn step(fst: &Instance, snd: &Instance, dt: f32, speed: f32) -> Instance {
    let t = (dt * speed).clamp(0.0, 1.0);
    let position = fst.position + (snd.position - fst.position) * t;
    let rotation = fst.rotation.nlerp(snd.rotation, t);
    let scale = fst.scale + (snd.scale - fst.scale) * t;

    Instance {
        position,
        rotation,
        scale,
    }
}

pub(crate) fn diff_lt_epsilon(fst: &Instance, snd: &Instance) -> bool {
    let pos_diff = fst.position.abs_diff_eq(&snd.position, EPSILON);
    let rot_diff = fst.rotation.abs_diff_eq(&snd.rotation, EPSILON);
    let scale_diff = fst.scale.abs_diff_eq(&snd.scale, EPSILON);
    pos_diff && rot_diff && scale_diff
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
    fn verify_step_no_panic() {
        let a = bounded_instance();
        let b = bounded_instance();
        let dt: f32 = kani::any();
        let speed: f32 = kani::any();
        kani::assume(dt >= 0.0 && dt <= 10.0);
        kani::assume(speed >= 0.0 && speed <= 10.0);
        let _ = step(&a, &b, dt, speed);
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn verify_step_identity() {
        let a = bounded_instance();
        let dt: f32 = kani::any();
        let speed: f32 = kani::any();
        kani::assume(dt >= 0.0 && dt <= 10.0);
        kani::assume(speed >= 0.0 && speed <= 10.0);
        let result = step(&a, &a, dt, speed);
        kani::assert(
            (result.position.x - a.position.x).abs() < 1e-3,
            "step(a, a, t, s).position.x == a.position.x",
        );
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn verify_diff_lt_epsilon_symmetric() {
        let a = bounded_instance();
        let b = bounded_instance();
        let fwd = diff_lt_epsilon(&a, &b);
        let rev = diff_lt_epsilon(&b, &a);
        kani::assert(fwd == rev, "diff_lt_epsilon is symmetric");
    }

    #[kani::proof]
    #[kani::unwind(6)]
    fn verify_find_keyframe_in_bounds() {
        // Generate a slice of up to 4 timestamps
        let len: usize = kani::any();
        kani::assume(len > 0 && len <= 4);
        let mut ts = vec![0.0f32; len];
        for t in ts.iter_mut() {
            let v: f32 = kani::any();
            kani::assume(v.is_finite());
            *t = v;
        }
        let current_time: f32 = kani::any();
        kani::assume(current_time.is_finite());
        let idx = find_keyframe_index(&ts, current_time);
        kani::assert(idx < len, "find_keyframe_index result is in bounds");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cgmath::{assert_relative_eq, One, Quaternion, Vector3};

    fn make_instance(pos: [f32; 3], scale: [f32; 3]) -> Instance {
        Instance {
            position: Vector3::new(pos[0], pos[1], pos[2]),
            rotation: Quaternion::one(),
            scale: Vector3::new(scale[0], scale[1], scale[2]),
        }
    }

    // --- step ---

    #[test]
    fn step_at_zero() {
        let a = make_instance([1.0, 2.0, 3.0], [1.0, 1.0, 1.0]);
        let b = make_instance([4.0, 5.0, 6.0], [2.0, 2.0, 2.0]);
        let result = step(&a, &b, 0.0, 1.0);
        assert_relative_eq!(result.position.x, a.position.x, epsilon = 1e-6);
        assert_relative_eq!(result.position.y, a.position.y, epsilon = 1e-6);
        assert_relative_eq!(result.position.z, a.position.z, epsilon = 1e-6);
        assert_relative_eq!(result.scale.x, a.scale.x, epsilon = 1e-6);
    }

    #[test]
    fn step_at_one() {
        let a = make_instance([1.0, 2.0, 3.0], [1.0, 1.0, 1.0]);
        let b = make_instance([4.0, 5.0, 6.0], [2.0, 2.0, 2.0]);
        let result = step(&a, &b, 1.0, 1.0);
        assert_relative_eq!(result.position.x, b.position.x, epsilon = 1e-6);
        assert_relative_eq!(result.position.y, b.position.y, epsilon = 1e-6);
        assert_relative_eq!(result.position.z, b.position.z, epsilon = 1e-6);
        assert_relative_eq!(result.scale.x, b.scale.x, epsilon = 1e-6);
    }

    #[test]
    fn step_midpoint_position() {
        let a = make_instance([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let b = make_instance([2.0, 4.0, 6.0], [1.0, 1.0, 1.0]);
        let result = step(&a, &b, 0.5, 1.0);
        assert_relative_eq!(result.position.x, 1.0, epsilon = 1e-6);
        assert_relative_eq!(result.position.y, 2.0, epsilon = 1e-6);
        assert_relative_eq!(result.position.z, 3.0, epsilon = 1e-6);
    }

    #[test]
    fn step_speed_doubles_rate() {
        let a = make_instance([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let b = make_instance([2.0, 4.0, 6.0], [1.0, 1.0, 1.0]);
        let half_speed2 = step(&a, &b, 0.5, 2.0);
        let full_speed1 = step(&a, &b, 1.0, 1.0);
        assert_relative_eq!(half_speed2.position.x, full_speed1.position.x, epsilon = 1e-6);
        assert_relative_eq!(half_speed2.position.y, full_speed1.position.y, epsilon = 1e-6);
        assert_relative_eq!(half_speed2.position.z, full_speed1.position.z, epsilon = 1e-6);
    }

    #[test]
    fn step_identical_inputs() {
        let a = make_instance([3.0, 1.0, 4.0], [2.0, 3.0, 5.0]);
        let result = step(&a, &a, 0.7, 3.0);
        assert_relative_eq!(result.position.x, a.position.x, epsilon = 1e-5);
        assert_relative_eq!(result.position.y, a.position.y, epsilon = 1e-5);
        assert_relative_eq!(result.position.z, a.position.z, epsilon = 1e-5);
        assert_relative_eq!(result.scale.x, a.scale.x, epsilon = 1e-5);
        assert_relative_eq!(result.scale.y, a.scale.y, epsilon = 1e-5);
        assert_relative_eq!(result.scale.z, a.scale.z, epsilon = 1e-5);
    }

    // step() must clamp dt*speed to [0,1] so it never overshoots the target.
    #[test]
    fn step_does_not_overshoot_target() {
        let a = make_instance([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let b = make_instance([10.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let result = step(&a, &b, 1.0, 2.0);
        assert!(
            result.position.x <= b.position.x,
            "step must not overshoot: got {} but target is {}",
            result.position.x,
            b.position.x
        );
    }

    // --- diff_lt_epsilon ---

    #[test]
    fn same_instance_is_lt_epsilon() {
        let a = make_instance([1.0, 2.0, 3.0], [1.0, 1.0, 1.0]);
        assert!(diff_lt_epsilon(&a, &a));
    }

    #[test]
    fn large_difference_is_false() {
        let a = make_instance([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let b = make_instance([1.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        assert!(!diff_lt_epsilon(&a, &b));
    }

    #[test]
    fn just_below_epsilon_is_true() {
        let a = make_instance([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let b = make_instance([0.009, 0.0, 0.0], [1.0, 1.0, 1.0]);
        assert!(diff_lt_epsilon(&a, &b));
    }

    #[test]
    fn just_above_epsilon_is_false() {
        let a = make_instance([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let b = make_instance([0.011, 0.0, 0.0], [1.0, 1.0, 1.0]);
        assert!(!diff_lt_epsilon(&a, &b));
    }

    // --- find_keyframe_index ---

    #[test]
    fn before_first_frame() {
        let ts = [1.0f32, 2.0, 3.0];
        assert_eq!(find_keyframe_index(&ts, 0.0), 0);
    }

    #[test]
    fn exactly_at_first_frame() {
        let ts = [1.0f32, 2.0, 3.0];
        assert_eq!(find_keyframe_index(&ts, 1.0), 1);
    }

    #[test]
    fn between_frames() {
        let ts = [0.0f32, 1.0, 2.0, 3.0];
        assert_eq!(find_keyframe_index(&ts, 1.5), 2);
    }

    #[test]
    fn at_last_frame() {
        let ts = [0.0f32, 1.0, 2.0];
        // time beyond last timestamp
        assert_eq!(find_keyframe_index(&ts, 10.0), 2);
    }

    #[test]
    fn single_keyframe() {
        let ts = [1.0f32];
        assert_eq!(find_keyframe_index(&ts, 0.0), 0);
        assert_eq!(find_keyframe_index(&ts, 5.0), 0);
    }

    #[test]
    fn empty_timestamps() {
        let ts: [f32; 0] = [];
        assert_eq!(find_keyframe_index(&ts, 1.0), 0);
    }
}
