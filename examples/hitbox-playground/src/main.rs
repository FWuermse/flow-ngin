//! Interactive demo of flow-ngin's spatial collision data structures.
//!
//! Controls:
//!   Mouse move: drag the active cube along the XZ plane
//!   Scroll wheel: move the active cube up / down (Y axis)
//! Left click: place the current cube and start a new one
//!   WASD / Arrow keys: pan the camera (built-in)
//!
//! GUI buttons select the collision strategy. Yellow overlays are broad-phase
//! candidates; red overlays will be SAT-confirmed collisions once SAT is implemented

mod collision_manager;
mod gui;
mod hitbox_overlay;
mod partition_viz;
mod scene;

use std::collections::HashSet;

use cgmath::Vector3;
use collision_manager::{CollisionBackend, Strategy};
use flow_ngin::flow::{FlowConstructor, GraphicsFlow, run};
use flow_ngin::pick::PickId;

pub struct PlacedObject {
    pub position: Vector3<f32>,
    pub id: PickId,
}

pub struct State {
    pub strategy: Strategy,
    pub drag_position: Vector3<f32>,
    pub placed_objects: Vec<PlacedObject>,
    pub next_id: u32,
    pub broad_phase_candidates: HashSet<u32>,
    pub geometric_collisions: HashSet<u32>,
    pub collision_backend: Option<CollisionBackend>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            strategy: Strategy::Octree,
            drag_position: Vector3::new(0.0, std::f32::consts::FRAC_1_SQRT_2, 0.0),
            placed_objects: Vec::new(),
            next_id: 1,
            broad_phase_candidates: HashSet::new(),
            geometric_collisions: HashSet::new(),
            collision_backend: None,
        }
    }
}

#[derive(Clone, Copy)]
pub enum Event {
    StrategyChanged(Strategy),
}

fn main() {
    let scene: FlowConstructor<State, Event> = Box::new(|ctx| {
        Box::pin(async move {
            Box::new(scene::SceneFlow::new(ctx).await) as Box<dyn GraphicsFlow<_, _>>
        })
    });

    let overlay: FlowConstructor<State, Event> = Box::new(|ctx| {
        Box::pin(async move {
            Box::new(hitbox_overlay::HitboxOverlayFlow::new(ctx).await)
                as Box<dyn GraphicsFlow<_, _>>
        })
    });

    let partition: FlowConstructor<State, Event> = Box::new(|ctx| {
        Box::pin(async move {
            Box::new(partition_viz::PartitionVizFlow::new(ctx))
                as Box<dyn GraphicsFlow<_, _>>
        })
    });

    let gui: FlowConstructor<State, Event> = Box::new(|ctx| {
        Box::pin(async move {
            Box::new(gui::GuiFlow::new(ctx).await) as Box<dyn GraphicsFlow<_, _>>
        })
    });

    let _ = run(vec![scene, overlay, partition, gui]);
}
