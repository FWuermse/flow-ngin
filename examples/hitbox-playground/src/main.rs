mod collision_backend;
mod gui_flow;
mod overlay_flow;
mod partition_viz_flow;
mod scene_flow;

use std::collections::HashSet;

use flow_ngin::{
    Vector3,
    flow::{FlowConstructor, GraphicsFlow},
    pick::PickId,
};

use gui_flow::GuiFlow;
use overlay_flow::OverlayFlow;
use partition_viz_flow::PartitionVizFlow;
use scene_flow::SceneFlow;

// ── State shared across all flows ──────────────────────────────────────────

/// Currently selected detection dimensionality (1, 2, or 3).
pub struct State {
    pub detection_dims: u8,
    pub object_shape: ObjectShape,
    pub strategy: Strategy,
    /// Cursor position in world space (x, y, z).
    pub drag_pos: Vector3<f32>,
    pub placed: Vec<PlacedObject>,
    pub next_id: u32,
    /// IDs returned by broad-phase `hit_candidates` for the drag object.
    pub broad_ids: HashSet<u32>,
    /// IDs confirmed geometrically overlapping the drag object.
    pub overlap_ids: HashSet<u32>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            detection_dims: 2,
            object_shape: ObjectShape::Cube3D,
            strategy: Strategy::SparseGrid,
            drag_pos: Vector3::new(0.0, 0.0, 0.0),
            placed: Vec::new(),
            next_id: 1,
            broad_ids: HashSet::new(),
            overlap_ids: HashSet::new(),
        }
    }
}

// ── Object shapes ───────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ObjectShape {
    Plane2D,
    Cube3D,
}

// ── Spatial-partition strategies ────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Strategy {
    Grid,
    SparseGrid,
    BruteForce,
    SpatialTree,
}

// ── Placed object ────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct PlacedObject {
    pub id: PickId,
    pub shape: ObjectShape,
    pub position: Vector3<f32>,
}

// ── Events exchanged between flows ──────────────────────────────────────────

#[derive(Clone)]
pub enum Event {
    /// Sent when the detection-space dimensionality slider changes; clears all placed objects.
    DetectionDimsChanged(u8),
    /// Sent when the object-shape slider changes.
    ObjectShapeChanged(ObjectShape),
    /// Sent when a strategy button is clicked.
    StrategyChanged(Strategy),
}

// ── Entry point ──────────────────────────────────────────────────────────────

fn main() {
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
