use flow_ngin::{
    InnerSpace, One, Point3, Quaternion, Rad, Rotation, Rotation3, Vector3, WindowEvent,
    context::{Context, GPUResource, InitContext},
    data_structures::{block::BuildingBlocks, instance::Instance},
    flow::{GraphicsFlow, Out},
    pick::PickId,
    render::Render,
};
use rand::Rng;
use winit::{
    event::MouseScrollDelta,
    keyboard::{KeyCode, PhysicalKey},
};

const DRAG_PICK_ID: u32 = u32::MAX - 1;
const ROTATE_STEP: f32 = 0.12;
const HEIGHT_STEP: f32 = 0.25;
const CUBE_SCALE: f32 = 0.5;

use crate::{Event, ObjectShape, PlacedObject, State, effective_rotation_axis, world_axis};
use crate::collision_backend::{HALF, PLANE_Y, WORLD_HALF};

fn cube_center_offset(rotation: Quaternion<f32>) -> Vector3<f32> {
    rotation.rotate_vector(Vector3::new(0.0, 0.5 * CUBE_SCALE, 0.0))
}

pub struct SceneFlow {
    drag_cube: BuildingBlocks,
    drag_plane: BuildingBlocks,
    placed_cubes: BuildingBlocks,
    placed_planes: BuildingBlocks,
    dirty: bool,
    cached_drag_pos: Vector3<f32>,
    cached_object_shape: ObjectShape,
    cached_drag_rotation: Quaternion<f32>,
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
            cached_drag_rotation: Quaternion::one(),
        }
    }

    fn rebuild_placed(&mut self, state: &State, ctx: &Context) {
        let cube_scale_vec = Vector3::new(CUBE_SCALE, CUBE_SCALE, CUBE_SCALE);

        let cubes: Vec<Instance> = state
            .placed
            .iter()
            .filter(|p| p.shape == ObjectShape::Cube3D)
            .map(|p| {
                let mut inst = Instance::new();
                inst.position = p.position - cube_center_offset(p.rotation);
                inst.rotation = p.rotation;
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
                inst.rotation = p.rotation;
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
            || state.drag_rotation != self.cached_drag_rotation
        {
            self.cached_drag_pos = state.drag_pos;
            self.cached_object_shape = state.object_shape;
            self.cached_drag_rotation = state.drag_rotation;

            let dp = state.drag_pos;
            match state.object_shape {
                ObjectShape::Cube3D => {
                    let cube_rot = state.drag_rotation;
                    let inst = &mut self.drag_cube.instances_mut_size_unchanged()[0];
                    inst.position = dp - cube_center_offset(cube_rot);
                    inst.rotation = cube_rot;
                    inst.scale = Vector3::new(CUBE_SCALE, CUBE_SCALE, CUBE_SCALE);
                    let plane_inst = &mut self.drag_plane.instances_mut_size_unchanged()[0];
                    plane_inst.scale = Vector3::new(0.0, 0.0, 0.0);
                }
                ObjectShape::Plane2D => {
                    let inst = &mut self.drag_plane.instances_mut_size_unchanged()[0];
                    inst.position = Vector3::new(dp.x, PLANE_Y, dp.z);
                    inst.rotation = state.drag_rotation;
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
                rotation: state.drag_rotation,
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
        match event {
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.05,
                };
                let axis = world_axis(effective_rotation_axis(state.object_shape, state.rotation_axis));
                let step = Quaternion::from_axis_angle(axis, Rad(scroll * ROTATE_STEP));
                state.drag_rotation = (step * state.drag_rotation).normalize();
            }
            WindowEvent::KeyboardInput { event: key_event, .. } => {
                if !key_event.state.is_pressed() {
                    return Out::Empty;
                }
                match key_event.physical_key {
                    PhysicalKey::Code(KeyCode::KeyU) => state.drag_pos.y += HEIGHT_STEP,
                    PhysicalKey::Code(KeyCode::KeyL) => state.drag_pos.y -= HEIGHT_STEP,
                    PhysicalKey::Code(KeyCode::KeyX) if !key_event.repeat => {
                        state.rotation_axis = (state.rotation_axis + 1) % 3;
                    }
                    _ => {}
                }
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
        match &event {
            Event::DetectionDimsChanged(_) => {
                state.placed.clear();
                self.dirty = true;
            }
            Event::ObjectShapeChanged(_) => {
                state.drag_rotation = Quaternion::one();
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
                    let tau = std::f32::consts::TAU;
                    let rotation = match state.object_shape {
                        ObjectShape::Plane2D => {
                            Quaternion::from_axis_angle(world_axis(1), Rad(rng.gen_range(0.0..tau)))
                        }
                        ObjectShape::Cube3D => {
                            Quaternion::from_axis_angle(world_axis(0), Rad(rng.gen_range(0.0..tau)))
                                * Quaternion::from_axis_angle(world_axis(1), Rad(rng.gen_range(0.0..tau)))
                                * Quaternion::from_axis_angle(world_axis(2), Rad(rng.gen_range(0.0..tau)))
                        }
                    };
                    state.placed.push(PlacedObject {
                        id,
                        shape: state.object_shape,
                        position: pos,
                        rotation,
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
