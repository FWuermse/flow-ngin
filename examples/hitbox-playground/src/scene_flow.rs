use flow_ngin::{
    Deg, One, Point3, Quaternion, Rotation, Rotation3, Vector3, WindowEvent,
    context::{Context, GPUResource, InitContext},
    data_structures::{block::BuildingBlocks, instance::Instance},
    flow::{GraphicsFlow, Out},
    pick::PickId,
    render::Render,
};
use rand::Rng;
use winit::event::MouseScrollDelta;

const DRAG_PICK_ID: u32 = u32::MAX - 1;

use crate::{Event, ObjectShape, PlacedObject, State};
use crate::collision_backend::{HALF, PLANE_Y, WORLD_HALF};

pub struct SceneFlow {
    drag_cube: BuildingBlocks,
    drag_plane: BuildingBlocks,
    placed_cubes: BuildingBlocks,
    placed_planes: BuildingBlocks,
    dirty: bool,
    cached_drag_pos: Vector3<f32>,
    cached_object_shape: ObjectShape,
}

impl SceneFlow {
    pub async fn new(ctx: InitContext) -> Self {
        let origin = Vector3::new(0.0, 0.0, 0.0);
        let rot = Quaternion::one();

        let drag_cube = BuildingBlocks::new(DRAG_PICK_ID, &ctx.queue, &ctx.device, origin, rot, 1, "cube.obj").await;
        let drag_plane = BuildingBlocks::new(DRAG_PICK_ID, &ctx.queue, &ctx.device, origin, rot, 1, "plane.obj").await;
        let placed_cubes = BuildingBlocks::new(0u32, &ctx.queue, &ctx.device, origin, rot, 0, "cube.obj").await;
        let placed_planes = BuildingBlocks::new(0u32, &ctx.queue, &ctx.device, origin, rot, 0, "plane.obj").await;

        Self {
            drag_cube,
            drag_plane,
            placed_cubes,
            placed_planes,
            dirty: false,
            cached_drag_pos: Vector3::new(f32::NAN, f32::NAN, f32::NAN),
            cached_object_shape: ObjectShape::Cube3D,
        }
    }

    fn rebuild_placed(&mut self, state: &State, ctx: &Context) {
        // 45° rotation on all axes to visually separate the cube from its hitbox overlay
        let cube_rot = Quaternion::from_axis_angle(Vector3::unit_x(), Deg(45.0))
            * Quaternion::from_axis_angle(Vector3::unit_y(), Deg(45.0))
            * Quaternion::from_axis_angle(Vector3::unit_z(), Deg(45.0));
        // Scale 0.5: after 45°-all-axes rotation the rotated AABB extends to ~±0.43,
        // safely inside the ±0.5 hitbox. Instance transform is T*R*S*v, so the mesh
        // center (0, 0.5*scale, 0) is displaced by R*(0, 0.25, 0) relative to
        // inst.position — subtract that to keep the visual center at the hitbox center.
        let cube_scale = 0.5_f32;
        let cube_scale_vec = Vector3::new(cube_scale, cube_scale, cube_scale);
        // Mesh center in model space after scaling
        let scaled_center = Vector3::new(0.0, 0.5 * cube_scale, 0.0);
        // Rotate the center offset so translation cancels it out
        let center_offset = cube_rot.rotate_vector(scaled_center);

        let cubes: Vec<Instance> = state
            .placed
            .iter()
            .filter(|p| p.shape == ObjectShape::Cube3D)
            .map(|p| {
                let mut inst = Instance::new();
                inst.position = p.position - center_offset;
                inst.rotation = cube_rot;
                inst.scale = cube_scale_vec;
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
        // Override camera to sit closer to the scene (engine default is 30/20)
        ctx.camera.camera.position = Point3::new(0.0, 15.0, 12.0);
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

        if state.drag_pos != self.cached_drag_pos
            || state.object_shape != self.cached_object_shape
        {
            self.cached_drag_pos = state.drag_pos;
            self.cached_object_shape = state.object_shape;

            let dp = state.drag_pos;
            match state.object_shape {
                ObjectShape::Cube3D => {
                    let cube_rot = Quaternion::from_axis_angle(Vector3::unit_x(), Deg(45.0))
                        * Quaternion::from_axis_angle(Vector3::unit_y(), Deg(45.0))
                        * Quaternion::from_axis_angle(Vector3::unit_z(), Deg(45.0));
                    let cube_scale = 0.5_f32;
                    let center_offset = cube_rot.rotate_vector(Vector3::new(0.0, 0.5 * cube_scale, 0.0));
                    let inst = &mut self.drag_cube.instances_mut_size_unchanged()[0];
                    inst.position = dp - center_offset;
                    inst.rotation = cube_rot;
                    inst.scale = Vector3::new(cube_scale, cube_scale, cube_scale);
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
        }

        if self.dirty {
            self.rebuild_placed(state, ctx);
        }

        Out::Empty
    }

    fn on_click(&mut self, _ctx: &Context, state: &mut State, id: PickId) -> Out<State, Event> {
        if id.0 == DRAG_PICK_ID || id.0 == 0 {
            let obj_id = PickId(state.next_id);
            state.next_id += 1;
            let pos = state.drag_pos;
            let actual_pos = match state.object_shape {
                ObjectShape::Plane2D => Vector3::new(pos.x, PLANE_Y, pos.z),
                ObjectShape::Cube3D => pos,
            };
            state.placed.push(PlacedObject {
                id: obj_id,
                shape: state.object_shape,
                position: actual_pos,
            });
            self.dirty = true;
        }
        Out::Empty
    }

    fn on_window_events(
        &mut self,
        _ctx: &Context,
        state: &mut State,
        event: &WindowEvent,
    ) -> Out<State, Event> {
        if let WindowEvent::MouseWheel { delta, .. } = event {
            let scroll = match delta {
                MouseScrollDelta::LineDelta(_, y) => *y,
                MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.05,
            };
            state.drag_pos.y += scroll * 0.5;
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
        match &event {
            Event::DetectionDimsChanged(_) => {
                state.placed.clear();
                self.dirty = true;
            }
            Event::PlaceRandom(count) => {
                let mut rng = rand::thread_rng();
                // Keep objects fully within the grid boundary
                let extent = WORLD_HALF - HALF;
                for _ in 0..*count {
                    let id = PickId(state.next_id);
                    state.next_id += 1;
                    let x = rng.gen_range(-extent..=extent);
                    let z = rng.gen_range(-extent..=extent);
                    // For 3D detection space also scatter along Y; otherwise stay flat
                    let y = if state.detection_dims >= 3 {
                        rng.gen_range(-extent..=extent)
                    } else {
                        0.0
                    };
                    let pos = match state.object_shape {
                        ObjectShape::Plane2D => Vector3::new(x, PLANE_Y, z),
                        ObjectShape::Cube3D => Vector3::new(x, y, z),
                    };
                    state.placed.push(PlacedObject {
                        id,
                        shape: state.object_shape,
                        position: pos,
                    });
                }
                self.dirty = true;
            }
            _ => {}
        }
        Some(event)
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        // Drag previews + placed objects
        let mut renders = Vec::with_capacity(4);

        // Using the feature/bug that emitting pickId 0 will always make this
        // get click events when clicked into void. As no cube is rendered at
        // the very beginning an initial ghost render emits 0 👻
        renders.push(self.placed_cubes.as_ref().into());
        if !self.placed_planes.instances().is_empty() {
            renders.push(self.placed_planes.as_ref().into());
        }

        // Drag preview (always show one)
        renders.push(self.drag_cube.as_ref().into());
        renders.push(self.drag_plane.as_ref().into());

        Render::Composed(renders)
    }
}
