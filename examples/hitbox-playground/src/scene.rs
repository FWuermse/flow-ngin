use std::time::Duration;

use cgmath::{One, Vector3, Zero};
use flow_ngin::{
    WindowEvent,
    camera::Camera,
    context::{Context, GPUResource, InitContext},
    data_structures::{block::BuildingBlocks, instance::Instance},
    flow::{GraphicsFlow, Out},
    pick::PickId,
    render::Render,
};
use winit::event::{ElementState, MouseButton, MouseScrollDelta};

use crate::{
    Event, PlacedObject, State,
    collision_manager::CollisionBackend,
};

const SCROLL_SPEED: f32 = 0.5;

pub struct SceneFlow {
    placed: BuildingBlocks,
    dragged: BuildingBlocks,
}

impl SceneFlow {
    pub async fn new(ctx: InitContext) -> Self {
        let placed = BuildingBlocks::new(
            0u32,
            &ctx.queue,
            &ctx.device,
            Vector3::zero(),
            cgmath::Quaternion::one(),
            0,
            "cube.obj",
        )
        .await;

        // One pre-allocated instance for the dragged cube
        let dragged = BuildingBlocks::new(
            0u32,
            &ctx.queue,
            &ctx.device,
            Vector3::zero(),
            cgmath::Quaternion::one(),
            1,
            "cube.obj",
        )
        .await;

        Self { placed, dragged }
    }

    fn place(&mut self, ctx: &Context, state: &mut State) {
        let id = PickId::from(state.next_id);
        state.next_id += 1;

        let mut instance = Instance::new();
        instance.position = state.drag_position;

        self.placed.add_instance(instance);
        self.placed.write_to_buffer(&ctx.queue, &ctx.device);

        state.placed_objects.push(PlacedObject {
            position: state.drag_position,
            id,
        });
    }

    fn update_collision(state: &mut State) {
        let mut backend = CollisionBackend::new(state.strategy);
        let (broad, narrow) = backend.query(&state.placed_objects, state.drag_position);
        state.broad_phase_candidates = broad;
        state.geometric_collisions = narrow;
        state.collision_backend = Some(backend);
    }
}

impl GraphicsFlow<State, Event> for SceneFlow {
    fn on_init(&mut self, ctx: &mut Context, _state: &mut State) -> Out<State, Event> {
        use cgmath::{Deg, Rad};
        ctx.camera.camera = Camera::new(
            (0.0_f32, 5.0_f32, 12.0_f32),
            Rad::from(Deg(-90.0_f32)),
            Rad::from(Deg(-25.0_f32)),
        );
        ctx.clear_colour = wgpu::Color { r: 0.835, g: 0.769, b: 0.631, a: 1.0 };
        Out::Empty
    }

    fn on_update(
        &mut self,
        ctx: &Context,
        state: &mut State,
        _dt: Duration,
    ) -> Out<State, Event> {
        if let Some(floor_pt) = ctx.ray_to_floor() {
            state.drag_position.x = floor_pt.x;
            state.drag_position.z = floor_pt.y; // Point2.y is world Z
        }

        let instance = &mut self.dragged.instances_mut_size_unchanged()[0];
        instance.position = state.drag_position;
        self.dragged.write_to_buffer(&ctx.queue, &ctx.device);

        Self::update_collision(state);
        Out::Empty
    }

    fn on_window_events(
        &mut self,
        ctx: &Context,
        state: &mut State,
        event: &WindowEvent,
    ) -> Out<State, Event> {
        match event {
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y * SCROLL_SPEED,
                    MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.02,
                };
                state.drag_position.y += dy;
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                self.place(ctx, state);
            }
            _ => {}
        }
        Out::Empty
    }

    fn on_custom_events(
        &mut self,
        _ctx: &Context,
        state: &mut State,
        event: Event,
    ) -> Option<Event> {
        match event {
            Event::StrategyChanged(s) => {
                state.strategy = s;
                None
            }
        }
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        if self.placed.instances().is_empty() {
            Render::Default(self.dragged.to_instanced())
        } else {
            Render::Composed(vec![
                Render::Default(self.placed.to_instanced()),
                Render::Default(self.dragged.to_instanced()),
            ])
        }
    }
}
