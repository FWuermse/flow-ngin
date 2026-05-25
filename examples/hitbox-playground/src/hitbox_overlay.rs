use std::time::Duration;

use cgmath::{One, Vector3, Zero};
use flow_ngin::{
    context::{Context, GPUResource, InitContext},
    data_structures::{block::BuildingBlocks, instance::Instance},
    flow::{GraphicsFlow, Out},
    render::Render,
};

use crate::{Event, State};

const OVERLAY_SCALE: f32 = 1.07;

pub struct HitboxOverlayFlow {
    default_hb: BuildingBlocks,
    candidate_hb: BuildingBlocks,
    collision_hb: BuildingBlocks,
    drag_hb: BuildingBlocks,
}

impl HitboxOverlayFlow {
    pub async fn new(ctx: InitContext) -> Self {
        let make = |file: &'static str, q: wgpu::Queue, d: wgpu::Device| async move {
            BuildingBlocks::new(
                0u32,
                &q,
                &d,
                Vector3::zero(),
                cgmath::Quaternion::one(),
                0,
                file,
            )
            .await
        };

        let (default_hb, candidate_hb, collision_hb, drag_hb) = tokio::join!(
            make("hitbox-default.obj", ctx.queue.clone(), ctx.device.clone()),
            make("hitbox-candidate.obj", ctx.queue.clone(), ctx.device.clone()),
            make("hitbox-collision.obj", ctx.queue.clone(), ctx.device.clone()),
            async {
                BuildingBlocks::new(
                    0u32,
                    &ctx.queue,
                    &ctx.device,
                    Vector3::zero(),
                    cgmath::Quaternion::one(),
                    1,
                    "hitbox-default.obj",
                )
                .await
            }
        );

        Self { default_hb, candidate_hb, collision_hb, drag_hb }
    }

    fn make_overlay_instance(pos: Vector3<f32>) -> Instance {
        let mut inst = Instance::new();
        inst.position = pos;
        inst.scale = Vector3::new(OVERLAY_SCALE, OVERLAY_SCALE, OVERLAY_SCALE);
        inst
    }
}

impl GraphicsFlow<State, Event> for HitboxOverlayFlow {
    fn on_update(&mut self, ctx: &Context, state: &mut State, _dt: Duration) -> Out<State, Event> {
        self.default_hb.instances_mut().clear();
        self.candidate_hb.instances_mut().clear();
        self.collision_hb.instances_mut().clear();

        for obj in &state.placed_objects {
            let instance = Self::make_overlay_instance(obj.position);
            if state.geometric_collisions.contains(&obj.id.0) {
                self.collision_hb.instances_mut().push(instance);
            } else if state.broad_phase_candidates.contains(&obj.id.0) {
                self.candidate_hb.instances_mut().push(instance);
            } else {
                self.default_hb.instances_mut().push(instance);
            }
        }

        let drag_inst = &mut self.drag_hb.instances_mut_size_unchanged()[0];
        drag_inst.position = state.drag_position;
        drag_inst.scale = Vector3::new(OVERLAY_SCALE, OVERLAY_SCALE, OVERLAY_SCALE);

        self.default_hb.write_to_buffer(&ctx.queue, &ctx.device);
        self.candidate_hb.write_to_buffer(&ctx.queue, &ctx.device);
        self.collision_hb.write_to_buffer(&ctx.queue, &ctx.device);
        self.drag_hb.write_to_buffer(&ctx.queue, &ctx.device);

        Out::Empty
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        let mut renders = vec![Render::Transparent(self.drag_hb.to_instanced())];

        if !self.default_hb.instances().is_empty() {
            renders.push(Render::Transparent(self.default_hb.to_instanced()));
        }
        if !self.candidate_hb.instances().is_empty() {
            renders.push(Render::Transparent(self.candidate_hb.to_instanced()));
        }
        if !self.collision_hb.instances().is_empty() {
            renders.push(Render::Transparent(self.collision_hb.to_instanced()));
        }

        Render::Composed(renders)
    }
}
