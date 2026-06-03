use std::collections::HashSet;

use flow_ngin::{
    One, Quaternion, Rotation, Vector3,
    context::{Context, GPUResource, InitContext},
    data_structures::{block::BuildingBlocks, collision::sat, instance::Instance},
    flow::{GraphicsFlow, Out},
    pick::PickId,
    pipelines::transparent::TransparencyUniform,
    render::Render,
};

use crate::{Event, ObjectShape, State, Strategy};
use crate::collision_backend::{CollisionBackend, PLANE_Y, make_hitbox};

const OVERLAY_SCALE: f32 = 1.07;
const PLANE_OVERLAY_Y_SCALE: f32 = 0.05;

pub struct OverlayFlow {
    overlay_clear: BuildingBlocks,
    overlay_broad: BuildingBlocks,
    overlay_overlap: BuildingBlocks,
    drag_overlay: BuildingBlocks,
    // Persistent collision backend
    backend: CollisionBackend,
    cached_strategy: Strategy,
    cached_dims: u8,
    cached_placed_count: usize,
    cached_drag_pos: Vector3<f32>,
    cached_object_shape: ObjectShape,
    cached_drag_rotation: Quaternion<f32>,
}

impl OverlayFlow {
    pub async fn new(ctx: InitContext) -> Self {
        let o = Vector3::new(0.0, 0.0, 0.0);
        let r = Quaternion::one();

        let overlay_clear   = BuildingBlocks::new(0u32, &ctx.queue, &ctx.device, o, r, 0, "overlay-white.obj").await;
        let overlay_broad   = BuildingBlocks::new(0u32, &ctx.queue, &ctx.device, o, r, 0, "overlay-white.obj").await;
        let overlay_overlap = BuildingBlocks::new(0u32, &ctx.queue, &ctx.device, o, r, 0, "overlay-white.obj").await;

        let drag_overlay = BuildingBlocks::new(0u32, &ctx.queue, &ctx.device, o, r, 1, "overlay-white.obj").await;

        let backend = CollisionBackend::new(Strategy::SparseGrid, 2);

        Self {
            overlay_clear,
            overlay_broad,
            overlay_overlap,
            drag_overlay,
            backend,
            cached_strategy: Strategy::SparseGrid,
            cached_dims: 2,
            cached_placed_count: 0,
            cached_drag_pos: Vector3::new(f32::NAN, f32::NAN, f32::NAN),
            cached_object_shape: ObjectShape::Cube3D,
            cached_drag_rotation: Quaternion::one(),
        }
    }
}

fn cube_overlay(pos: Vector3<f32>, rotation: Quaternion<f32>) -> Instance {
    let mut inst = Instance::new();
    let center_offset = rotation.rotate_vector(Vector3::new(0.0, 0.5 * OVERLAY_SCALE, 0.0));
    inst.position = pos - center_offset;
    inst.rotation = rotation;
    inst.scale = Vector3::new(OVERLAY_SCALE, OVERLAY_SCALE, OVERLAY_SCALE);
    inst
}

fn plane_overlay(pos: Vector3<f32>, rotation: Quaternion<f32>) -> Instance {
    let mut inst = Instance::new();
    inst.position = Vector3::new(pos.x, PLANE_Y, pos.z);
    inst.rotation = rotation;
    inst.scale = Vector3::new(OVERLAY_SCALE, PLANE_OVERLAY_Y_SCALE, OVERLAY_SCALE);
    inst
}

impl GraphicsFlow<State, Event> for OverlayFlow {
    fn on_init(&mut self, _ctx: &mut flow_ngin::context::Context, state: &mut State) -> Out<State, Event> {
        self.backend = CollisionBackend::rebuild(state.strategy, state.detection_dims, &state.placed);
        self.cached_strategy = state.strategy;
        self.cached_dims = state.detection_dims;
        self.cached_placed_count = state.placed.len();
        Out::Empty
    }

    fn on_update(
        &mut self,
        ctx: &Context,
        state: &mut State,
        _dt: std::time::Duration,
    ) -> Out<State, Event> {
        let needs_full_rebuild = state.strategy != self.cached_strategy
            || state.detection_dims != self.cached_dims
            || state.placed.len() < self.cached_placed_count;

        if needs_full_rebuild {
            self.backend = CollisionBackend::rebuild(state.strategy, state.detection_dims, &state.placed);
            self.cached_strategy = state.strategy;
            self.cached_dims = state.detection_dims;
            self.cached_placed_count = state.placed.len();
        } else if state.placed.len() > self.cached_placed_count {
            for placed in &state.placed[self.cached_placed_count..] {
                self.backend.insert(make_hitbox(
                    placed.position,
                    placed.shape,
                    placed.id,
                    placed.rotation,
                ));
            }
            self.cached_placed_count = state.placed.len();
        }

        self.cached_drag_pos = state.drag_pos;
        self.cached_object_shape = state.object_shape;
        self.cached_drag_rotation = state.drag_rotation;

        let drag_id = PickId(0);
        let drag_hb = make_hitbox(state.drag_pos, state.object_shape, drag_id, state.drag_rotation);

        let candidates = self.backend.hit_candidates(drag_hb.clone());
        let broad_ids: HashSet<u32> = candidates.iter().map(|c| c.tag().0).collect();

        let overlap_ids: HashSet<u32> = candidates
            .iter()
            .filter(|c| sat(&drag_hb, *c).hit())
            .map(|c| c.tag().0)
            .collect();

        let n = state.placed.len();
        let mut clear = Vec::with_capacity(n);
        let mut broad = Vec::with_capacity(n);
        let mut overlap = Vec::with_capacity(n);

        for placed in &state.placed {
            let id = placed.id.0;
            let inst = match placed.shape {
                ObjectShape::Cube3D => cube_overlay(placed.position, placed.rotation),
                ObjectShape::Plane2D => plane_overlay(placed.position, placed.rotation),
            };

            if overlap_ids.contains(&id) {
                overlap.push(inst);
            } else if broad_ids.contains(&id) {
                broad.push(inst);
            } else {
                clear.push(inst);
            }
        }

        state.broad_ids = broad_ids;
        state.overlap_ids = overlap_ids;

        *self.overlay_clear.instances_mut()   = clear;
        *self.overlay_broad.instances_mut()   = broad;
        *self.overlay_overlap.instances_mut() = overlap;

        self.overlay_clear.write_to_buffer(&ctx.queue, &ctx.device);
        self.overlay_broad.write_to_buffer(&ctx.queue, &ctx.device);
        self.overlay_overlap.write_to_buffer(&ctx.queue, &ctx.device);

        let dp = state.drag_pos;
        self.drag_overlay.instances_mut_size_unchanged()[0] = match state.object_shape {
            ObjectShape::Cube3D => cube_overlay(dp, state.drag_rotation),
            ObjectShape::Plane2D => plane_overlay(dp, state.drag_rotation),
        };
        self.drag_overlay.write_to_buffer(&ctx.queue, &ctx.device);

        Out::Empty
    }

    fn on_custom_events(
        &mut self,
        _ctx: &Context,
        _state: &mut State,
        event: Event,
    ) -> Option<Event> {
        Some(event)
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        let mut renders: Vec<Render<'_, 'pass>> = Vec::with_capacity(4);

        const WHITE: [f32; 3] = [1.0, 1.0, 1.0];
        const YELLOW: [f32; 3] = [1.0, 1.0, 0.0];
        const RED: [f32; 3] = [1.0, 0.0, 0.0];

        macro_rules! push_transparent {
            ($bb:expr, $tint:expr) => {
                if !$bb.instances().is_empty() {
                    renders.push(Render::Transparent(
                        $bb.to_instanced(),
                        TransparencyUniform { tint: $tint, alpha: 0.4 },
                    ));
                }
            };
        }

        push_transparent!(self.overlay_clear, WHITE);
        push_transparent!(self.overlay_broad, YELLOW);
        push_transparent!(self.overlay_overlap, RED);
        push_transparent!(self.drag_overlay, WHITE);

        Render::Composed(renders)
    }
}
