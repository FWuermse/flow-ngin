use flow_ngin::{
    One, Quaternion, Vector3, WindowEvent,
    context::{Context, GPUResource, InitContext},
    data_structures::{block::BuildingBlocks, instance::Instance},
    flow::{GraphicsFlow, Out},
    pick::PickId,
    render::Render,
};
use winit::event::{ElementState, MouseButton, MouseScrollDelta};

use crate::{Event, ObjectShape, PlacedObject, State};
use crate::collision_backend::PLANE_Y;

pub struct SceneFlow {
    /// One instance of the cube model for the active drag preview.
    drag_cube: BuildingBlocks,
    /// One instance of the plane model for the active drag preview.
    drag_plane: BuildingBlocks,
    /// All placed cubes.
    placed_cubes: BuildingBlocks,
    /// All placed planes.
    placed_planes: BuildingBlocks,
    /// Whether placed instance buffers need rebuilding this frame.
    dirty: bool,
}

impl SceneFlow {
    pub async fn new(ctx: InitContext) -> Self {
        let origin = Vector3::new(0.0, 0.0, 0.0);
        let rot = Quaternion::one();

        let drag_cube = BuildingBlocks::new(0u32, &ctx.queue, &ctx.device, origin, rot, 1, "cube.obj").await;
        let drag_plane = BuildingBlocks::new(0u32, &ctx.queue, &ctx.device, origin, rot, 1, "plane.obj").await;
        let placed_cubes = BuildingBlocks::new(0u32, &ctx.queue, &ctx.device, origin, rot, 0, "cube.obj").await;
        let placed_planes = BuildingBlocks::new(0u32, &ctx.queue, &ctx.device, origin, rot, 0, "plane.obj").await;

        Self {
            drag_cube,
            drag_plane,
            placed_cubes,
            placed_planes,
            dirty: false,
        }
    }

    fn rebuild_placed(&mut self, state: &State, ctx: &Context) {
        let cubes: Vec<Instance> = state
            .placed
            .iter()
            .filter(|p| p.shape == ObjectShape::Cube3D)
            .map(|p| {
                let mut inst = Instance::new();
                inst.position = p.position;
                inst
            })
            .collect();

        let planes: Vec<Instance> = state
            .placed
            .iter()
            .filter(|p| p.shape == ObjectShape::Plane2D)
            .map(|p| {
                let mut inst = Instance::new();
                inst.position = Vector3::new(p.position.x, PLANE_Y, p.position.z);
                inst
            })
            .collect();

        *self.placed_cubes.instances_mut() = cubes;
        self.placed_cubes.write_to_buffer(&ctx.queue, &ctx.device);

        *self.placed_planes.instances_mut() = planes;
        self.placed_planes.write_to_buffer(&ctx.queue, &ctx.device);

        self.dirty = false;
    }
}

impl GraphicsFlow<State, Event> for SceneFlow {
    fn on_init(&mut self, ctx: &mut Context, _state: &mut State) -> Out<State, Event> {
        ctx.clear_colour = wgpu::Color {
            r: 0.12,
            g: 0.12,
            b: 0.14,
            a: 1.0,
        };
        Out::Empty
    }

    fn on_update(
        &mut self,
        ctx: &Context,
        state: &mut State,
        _dt: std::time::Duration,
    ) -> Out<State, Event> {
        // Update drag position from mouse ray
        if let Some(floor_pos) = ctx.ray_to_floor() {
            state.drag_pos.x = floor_pos.x;
            state.drag_pos.z = floor_pos.y; // ray_to_floor returns Point2(x, z)
        }

        // Update the drag preview position — only show the active shape
        let dp = state.drag_pos;
        match state.object_shape {
            ObjectShape::Cube3D => {
                let inst = &mut self.drag_cube.instances_mut_size_unchanged()[0];
                inst.position = dp;
                inst.scale = Vector3::new(1.0, 1.0, 1.0);
                let plane_inst = &mut self.drag_plane.instances_mut_size_unchanged()[0];
                plane_inst.scale = Vector3::new(0.0, 0.0, 0.0);
            }
            ObjectShape::Plane2D => {
                let inst = &mut self.drag_plane.instances_mut_size_unchanged()[0];
                inst.position = Vector3::new(dp.x, PLANE_Y, dp.z);
                inst.scale = Vector3::new(1.0, 1.0, 1.0);
                let cube_inst = &mut self.drag_cube.instances_mut_size_unchanged()[0];
                cube_inst.scale = Vector3::new(0.0, 0.0, 0.0);
            }
        }
        self.drag_cube.write_to_buffer(&ctx.queue, &ctx.device);
        self.drag_plane.write_to_buffer(&ctx.queue, &ctx.device);

        if self.dirty {
            self.rebuild_placed(state, ctx);
        }

        Out::Empty
    }

    fn on_window_events(
        &mut self,
        _ctx: &Context,
        state: &mut State,
        event: &WindowEvent,
    ) -> Out<State, Event> {
        match event {
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.05,
                };
                state.drag_pos.y += scroll * 0.5;
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let id = PickId(state.next_id);
                state.next_id += 1;
                let pos = state.drag_pos;
                let actual_pos = match state.object_shape {
                    ObjectShape::Plane2D => Vector3::new(pos.x, PLANE_Y, pos.z),
                    ObjectShape::Cube3D => pos,
                };
                state.placed.push(PlacedObject {
                    id,
                    shape: state.object_shape,
                    position: actual_pos,
                });
                self.dirty = true;
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
        // SceneFlow only needs to clear placed objects when dims change
        if let Event::DetectionDimsChanged(_) = &event {
            state.placed.clear();
            self.dirty = true;
        }
        Some(event)
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        // Drag previews + placed objects
        let mut renders = Vec::new();

        // Placed objects
        if !self.placed_cubes.instances().is_empty() {
            renders.push(self.placed_cubes.as_ref().into());
        }
        if !self.placed_planes.instances().is_empty() {
            renders.push(self.placed_planes.as_ref().into());
        }

        // Drag preview (always show one)
        renders.push(self.drag_cube.as_ref().into());
        renders.push(self.drag_plane.as_ref().into());

        Render::Composed(renders)
    }
}
