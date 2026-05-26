use std::collections::HashSet;

use flow_ngin::{
    One, Quaternion, Vector3,
    context::{Context, GPUResource, InitContext},
    data_structures::{block::BuildingBlocks, collision::Hitbox, instance::Instance},
    flow::{GraphicsFlow, Out},
    pick::PickId,
    render::Render,
};

use crate::{Event, ObjectShape, State};
use crate::collision_backend::{CollisionBackend, PLANE_Y, make_hitbox};

/// Scale factor for overlays so they enclose the visible geometry.
const OVERLAY_SCALE: f32 = 1.07;
/// Thin scale for 2D plane overlays (very flat slab).
const PLANE_OVERLAY_Y_SCALE: f32 = 0.05;

pub struct OverlayFlow {
    // 3D cube overlays (3 colors)
    cube_white: BuildingBlocks,
    cube_yellow: BuildingBlocks,
    cube_red: BuildingBlocks,
    // 2D plane overlays (3 colors) — rendered as flat slabs
    plane_white: BuildingBlocks,
    plane_yellow: BuildingBlocks,
    plane_red: BuildingBlocks,
    // Drag-cursor overlay (white)
    drag_overlay_cube: BuildingBlocks,
    drag_overlay_plane: BuildingBlocks,
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

        Self {
            cube_white,
            cube_yellow,
            cube_red,
            plane_white,
            plane_yellow,
            plane_red,
            drag_overlay_cube,
            drag_overlay_plane,
        }
    }
}

fn cube_overlay(pos: Vector3<f32>) -> Instance {
    let mut inst = Instance::new();
    inst.position = pos;
    inst.scale = Vector3::new(OVERLAY_SCALE, OVERLAY_SCALE, OVERLAY_SCALE);
    inst
}

fn plane_overlay(pos: Vector3<f32>) -> Instance {
    let mut inst = Instance::new();
    inst.position = Vector3::new(pos.x, PLANE_Y, pos.z);
    inst.scale = Vector3::new(OVERLAY_SCALE, PLANE_OVERLAY_Y_SCALE, OVERLAY_SCALE);
    inst
}

impl GraphicsFlow<State, Event> for OverlayFlow {
    fn on_update(
        &mut self,
        ctx: &Context,
        state: &mut State,
        _dt: std::time::Duration,
    ) -> Out<State, Event> {
        // ── Build collision backend and run queries ───────────────────────────
        let mut backend = CollisionBackend::new(state.strategy, state.detection_dims);

        for placed in &state.placed {
            let hb = make_hitbox(placed.position, placed.shape, placed.id);
            backend.insert(hb);
        }

        // Drag hitbox
        let drag_id = PickId(0);
        let drag_hb = make_hitbox(state.drag_pos, state.object_shape, drag_id);

        // Broad-phase candidates
        let candidates = backend.hit_candidates(drag_hb.clone());
        let broad_ids: HashSet<u32> = candidates.iter().map(|c| c.tag().0).collect();

        // Narrow-phase (geometric overlap)
        let overlap_ids: HashSet<u32> = candidates
            .iter()
            .filter(|c| {
                // Use overlaps() from TaggedNDimBounds
                // drag_hb checks against the candidate
                drag_hb.overlaps(c)
            })
            .map(|c| c.tag().0)
            .collect();

        state.broad_ids = broad_ids;
        state.overlap_ids = overlap_ids.clone();

        // ── Rebuild overlay instance lists ────────────────────────────────────
        let mut cw = Vec::new();
        let mut cy = Vec::new();
        let mut cr = Vec::new();
        let mut pw = Vec::new();
        let mut py = Vec::new();
        let mut pr = Vec::new();

        for placed in &state.placed {
            let id = placed.id.0;
            let is_overlap = overlap_ids.contains(&id);
            let is_broad = state.broad_ids.contains(&id);

            match placed.shape {
                ObjectShape::Cube3D => {
                    if is_overlap {
                        cr.push(cube_overlay(placed.position));
                    } else if is_broad {
                        cy.push(cube_overlay(placed.position));
                    } else {
                        cw.push(cube_overlay(placed.position));
                    }
                }
                ObjectShape::Plane2D => {
                    if is_overlap {
                        pr.push(plane_overlay(placed.position));
                    } else if is_broad {
                        py.push(plane_overlay(placed.position));
                    } else {
                        pw.push(plane_overlay(placed.position));
                    }
                }
            }
        }

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

        // ── Drag overlay ─────────────────────────────────────────────────────
        let dp = state.drag_pos;
        match state.object_shape {
            ObjectShape::Cube3D => {
                self.drag_overlay_cube.instances_mut_size_unchanged()[0] = {
                    let mut inst = Instance::new();
                    inst.position = dp;
                    inst.scale = Vector3::new(OVERLAY_SCALE, OVERLAY_SCALE, OVERLAY_SCALE);
                    inst
                };
                self.drag_overlay_plane.instances_mut_size_unchanged()[0].scale =
                    Vector3::new(0.0, 0.0, 0.0);
            }
            ObjectShape::Plane2D => {
                self.drag_overlay_plane.instances_mut_size_unchanged()[0] = {
                    let mut inst = Instance::new();
                    inst.position = Vector3::new(dp.x, PLANE_Y, dp.z);
                    inst.scale = Vector3::new(OVERLAY_SCALE, PLANE_OVERLAY_Y_SCALE, OVERLAY_SCALE);
                    inst
                };
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
        let mut renders: Vec<Render<'_, 'pass>> = Vec::new();

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
        // Drag overlay always rendered
        renders.push(Render::Transparent(self.drag_overlay_cube.to_instanced()));
        renders.push(Render::Transparent(self.drag_overlay_plane.to_instanced()));

        Render::Composed(renders)
    }
}
