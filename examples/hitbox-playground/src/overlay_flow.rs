use std::collections::HashSet;

use flow_ngin::{
    One, Quaternion, Rotation, Vector3,
    context::{Context, GPUResource, InitContext},
    data_structures::{block::BuildingBlocks, collision::sat, instance::Instance},
    flow::{GraphicsFlow, Out},
    pick::PickId,
    render::Render,
};

use crate::{Event, ObjectShape, State, Strategy};
use crate::collision_backend::{CollisionBackend, PLANE_Y, make_hitbox};

const OVERLAY_SCALE: f32 = 1.07;
const PLANE_OVERLAY_Y_SCALE: f32 = 0.05;

pub struct OverlayFlow {
    // 3D cube overlays (3 colors)
    cube_white: BuildingBlocks,
    cube_yellow: BuildingBlocks,
    cube_red: BuildingBlocks,
    // 2D plane overlays (3 colors)
    plane_white: BuildingBlocks,
    plane_yellow: BuildingBlocks,
    plane_red: BuildingBlocks,
    // Drag-cursor overlay (white)
    drag_overlay_cube: BuildingBlocks,
    drag_overlay_plane: BuildingBlocks,
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

        let cube_white  = BuildingBlocks::new(0u32, &ctx.queue, &ctx.device, o, r, 0, "overlay-white.obj").await;
        let cube_yellow = BuildingBlocks::new(0u32, &ctx.queue, &ctx.device, o, r, 0, "overlay-yellow.obj").await;
        let cube_red    = BuildingBlocks::new(0u32, &ctx.queue, &ctx.device, o, r, 0, "overlay-red.obj").await;

        let plane_white  = BuildingBlocks::new(0u32, &ctx.queue, &ctx.device, o, r, 0, "overlay-white.obj").await;
        let plane_yellow = BuildingBlocks::new(0u32, &ctx.queue, &ctx.device, o, r, 0, "overlay-yellow.obj").await;
        let plane_red    = BuildingBlocks::new(0u32, &ctx.queue, &ctx.device, o, r, 0, "overlay-red.obj").await;

        let drag_overlay_cube  = BuildingBlocks::new(0u32, &ctx.queue, &ctx.device, o, r, 1, "overlay-white.obj").await;
        let drag_overlay_plane = BuildingBlocks::new(0u32, &ctx.queue, &ctx.device, o, r, 1, "overlay-white.obj").await;

        let backend = CollisionBackend::new(Strategy::SparseGrid, 2);

        Self {
            cube_white,
            cube_yellow,
            cube_red,
            plane_white,
            plane_yellow,
            plane_red,
            drag_overlay_cube,
            drag_overlay_plane,
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
        let mut objects_changed = needs_full_rebuild;

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
            objects_changed = true;
        }

        let drag_moved = state.drag_pos != self.cached_drag_pos
            || state.object_shape != self.cached_object_shape
            || state.drag_rotation != self.cached_drag_rotation;

        if !drag_moved && !objects_changed {
            return Out::Empty;
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
            .filter(|c| sat(&drag_hb, *c))
            .map(|c| c.tag().0)
            .collect();

        let n = state.placed.len();
        let mut cw = Vec::with_capacity(n);
        let mut cy = Vec::with_capacity(n);
        let mut cr = Vec::with_capacity(n);
        let mut pw = Vec::with_capacity(n);
        let mut py = Vec::with_capacity(n);
        let mut pr = Vec::with_capacity(n);

        for placed in &state.placed {
            let id = placed.id.0;
            let is_overlap = overlap_ids.contains(&id);
            let is_broad = broad_ids.contains(&id);

            match placed.shape {
                ObjectShape::Cube3D => {
                    if is_overlap {
                        cr.push(cube_overlay(placed.position, placed.rotation));
                    } else if is_broad {
                        cy.push(cube_overlay(placed.position, placed.rotation));
                    } else {
                        cw.push(cube_overlay(placed.position, placed.rotation));
                    }
                }
                ObjectShape::Plane2D => {
                    if is_overlap {
                        pr.push(plane_overlay(placed.position, placed.rotation));
                    } else if is_broad {
                        py.push(plane_overlay(placed.position, placed.rotation));
                    } else {
                        pw.push(plane_overlay(placed.position, placed.rotation));
                    }
                }
            }
        }

        state.broad_ids = broad_ids;
        state.overlap_ids = overlap_ids;

        *self.cube_white.instances_mut()  = cw;
        *self.cube_yellow.instances_mut() = cy;
        *self.cube_red.instances_mut()    = cr;
        *self.plane_white.instances_mut()  = pw;
        *self.plane_yellow.instances_mut() = py;
        *self.plane_red.instances_mut()    = pr;

        self.cube_white.write_to_buffer(&ctx.queue, &ctx.device);
        self.cube_yellow.write_to_buffer(&ctx.queue, &ctx.device);
        self.cube_red.write_to_buffer(&ctx.queue, &ctx.device);
        self.plane_white.write_to_buffer(&ctx.queue, &ctx.device);
        self.plane_yellow.write_to_buffer(&ctx.queue, &ctx.device);
        self.plane_red.write_to_buffer(&ctx.queue, &ctx.device);

        let dp = state.drag_pos;
        match state.object_shape {
            ObjectShape::Cube3D => {
                self.drag_overlay_cube.instances_mut_size_unchanged()[0] =
                    cube_overlay(dp, state.drag_rotation);
                self.drag_overlay_plane.instances_mut_size_unchanged()[0].scale =
                    Vector3::new(0.0, 0.0, 0.0);
            }
            ObjectShape::Plane2D => {
                self.drag_overlay_plane.instances_mut_size_unchanged()[0] =
                    plane_overlay(dp, state.drag_rotation);
                self.drag_overlay_cube.instances_mut_size_unchanged()[0].scale =
                    Vector3::new(0.0, 0.0, 0.0);
            }
        }
        self.drag_overlay_cube.write_to_buffer(&ctx.queue, &ctx.device);
        self.drag_overlay_plane.write_to_buffer(&ctx.queue, &ctx.device);

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
        let mut renders: Vec<Render<'_, 'pass>> = Vec::with_capacity(8);

        macro_rules! push_transparent {
            ($bb:expr) => {
                if !$bb.instances().is_empty() {
                    let inst = $bb.to_instanced();
                    renders.push(Render::Transparent(inst));
                }
            };
        }

        push_transparent!(self.cube_white);
        push_transparent!(self.cube_yellow);
        push_transparent!(self.cube_red);
        push_transparent!(self.plane_white);
        push_transparent!(self.plane_yellow);
        push_transparent!(self.plane_red);
        renders.push(Render::Transparent(self.drag_overlay_cube.to_instanced()));
        renders.push(Render::Transparent(self.drag_overlay_plane.to_instanced()));

        Render::Composed(renders)
    }
}
