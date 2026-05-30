mod collision_backend;
mod gui_flow;
mod overlay_flow;
mod partition_viz_flow;
mod scene_flow;

use std::collections::HashSet;

use flow_ngin::{
    One, Quaternion, Vector3,
    flow::{FlowConstructor, GraphicsFlow},
    pick::PickId,
};

use gui_flow::GuiFlow;
use overlay_flow::OverlayFlow;
use partition_viz_flow::PartitionVizFlow;
use scene_flow::SceneFlow;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

pub struct State {
    pub detection_dims: u8,
    pub object_shape: ObjectShape,
    pub strategy: Strategy,
    pub drag_pos: Vector3<f32>,
    pub drag_rotation: Quaternion<f32>,
    pub rotation_axis: u8,
    pub placed: Vec<PlacedObject>,
    pub next_id: u32,
    pub broad_ids: HashSet<u32>,
    pub overlap_ids: HashSet<u32>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            detection_dims: 2,
            object_shape: ObjectShape::Cube3D,
            strategy: Strategy::SparseGrid,
            drag_pos: Vector3::new(0.0, 0.0, 0.0),
            drag_rotation: Quaternion::one(),
            rotation_axis: 1,
            placed: Vec::new(),
            next_id: 1,
            broad_ids: HashSet::new(),
            overlap_ids: HashSet::new(),
        }
    }
}

/// The three world axes the block can rotate around, indexed by `rotation_axis`.
pub fn world_axis(index: u8) -> Vector3<f32> {
    match index % 3 {
        0 => Vector3::new(1.0, 0.0, 0.0),
        1 => Vector3::new(0.0, 1.0, 0.0),
        _ => Vector3::new(0.0, 0.0, 1.0),
    }
}

pub fn effective_rotation_axis(shape: ObjectShape, rotation_axis: u8) -> u8 {
    match shape {
        ObjectShape::Plane2D => 1,
        ObjectShape::Cube3D => rotation_axis,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ObjectShape {
    Plane2D,
    Cube3D,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Strategy {
    Grid,
    SparseGrid,
    BruteForce,
    SpatialTree,
}

#[derive(Clone)]
pub struct PlacedObject {
    pub id: PickId,
    pub shape: ObjectShape,
    pub position: Vector3<f32>,
    pub rotation: Quaternion<f32>,
}

#[derive(Clone)]
pub enum Event {
    DetectionDimsChanged(u8),
    ObjectShapeChanged(ObjectShape),
    StrategyChanged(Strategy),
    PlaceRandom(u32),
}

pub fn launch() {
    let scene: FlowConstructor<State, Event> = Box::new(|ctx| {
        Box::pin(async move {
            Box::new(SceneFlow::new(ctx).await) as Box<dyn GraphicsFlow<_, _>>
        })
    });

    let overlay: FlowConstructor<State, Event> = Box::new(|ctx| {
        Box::pin(async move {
            Box::new(OverlayFlow::new(ctx).await) as Box<dyn GraphicsFlow<_, _>>
        })
    });

    let partition_viz: FlowConstructor<State, Event> = Box::new(|ctx| {
        Box::pin(async move {
            Box::new(PartitionVizFlow::new(ctx).await) as Box<dyn GraphicsFlow<_, _>>
        })
    });

    let gui: FlowConstructor<State, Event> = Box::new(|ctx| {
        Box::pin(async move {
            Box::new(GuiFlow::new(ctx).await) as Box<dyn GraphicsFlow<_, _>>
        })
    });

    let _ = flow_ngin::flow::run(vec![scene, overlay, partition_viz, gui]);
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn run_web() {
    console_error_panic_hook::set_once();
    launch();
}
