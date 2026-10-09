use crate::{
    Action, Event,
    editor::{Editor, Mode},
    geometry,
    ui::PANEL,
};
use cgmath::{Deg, InnerSpace, Point3, Vector3};
use flow_ngin::{WindowEvent, camera::Camera, context::Context, flow::Out};
use std::collections::HashSet;
use winit::{
    event::{ElementState, MouseButton, MouseScrollDelta},
    keyboard::{KeyCode, ModifiersState, PhysicalKey},
};
struct Press {
    start: [f64; 2],
    hit: Option<u64>,
    floor: Option<[f32; 2]>,
    dragging: bool,
    placing: bool,
}
pub struct Input {
    keys: HashSet<KeyCode>,
    modifiers: ModifiersState,
    press: Option<Press>,
    right: bool,
    pub rectangle: Option<([f64; 2], [f64; 2])>,
    yaw: f32,
    pitch: f32,
    last: [f64; 2],
    zoom: f32,
    wheel: f32,
}
impl Default for Input {
    fn default() -> Self {
        Self {
            keys: HashSet::new(),
            modifiers: ModifiersState::empty(),
            press: None,
            right: false,
            rectangle: None,
            yaw: -90.,
            pitch: -40.,
            last: [0.; 2],
            zoom: 0.,
            wheel: 0.,
        }
    }
}
impl Input {
    pub fn floor(ctx: &Context) -> Option<[f32; 2]> {
        ctx.ray_to_floor().map(|p| [p.x, p.y])
    }
    pub fn in_scene(ctx: &Context) -> bool {
        ctx.mouse.coords.x >= PANEL
            && ctx.mouse.coords.x < ctx.config.width as f64
            && ctx.mouse.coords.y >= 0.
            && ctx.mouse.coords.y < ctx.config.height as f64
    }
    pub fn reset(&mut self) {
        self.keys.clear();
        self.press = None;
        self.right = false;
        self.rectangle = None;
        self.zoom = 0.;
        self.wheel = 0.;
    }
    pub fn window(
        &mut self,
        ctx: &Context,
        editor: &mut Editor,
        event: &WindowEvent,
        focused: bool,
    ) -> Option<Action> {
        let cursor = [ctx.mouse.coords.x, ctx.mouse.coords.y];
        match event {
            WindowEvent::Focused(false) => self.reset(),
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::CursorMoved { .. } => {
                if self.right && !focused {
                    self.yaw += (cursor[0] - self.last[0]) as f32 * 0.2;
                    self.pitch =
                        (self.pitch - (cursor[1] - self.last[1]) as f32 * 0.2).clamp(-89., 89.);
                }
                self.last = cursor;
                if !focused {
                    if let Some(press) = &mut self.press {
                        let moved =
                            (cursor[0] - press.start[0]).hypot(cursor[1] - press.start[1]) > 4.;
                        if moved
                            && !press.placing
                            && press.hit.is_none()
                            && editor.mode == Mode::Multi
                        {
                            self.rectangle = Some((press.start, cursor));
                        } else if moved
                            && !press.placing
                            && !press.dragging
                            && press.hit.is_some()
                            && editor.mode == Mode::Select
                        {
                            let id = press.hit.unwrap();
                            if !editor.selected.contains(&id) {
                                editor.selected.clear();
                                editor.selected.insert(id);
                            }
                            editor.begin_transform(false, press.floor);
                            press.dragging = true;
                        }
                    }
                    if editor.preview.is_some() {
                        editor.update_position(if Self::in_scene(ctx) {
                            Self::floor(ctx)
                        } else {
                            None
                        });
                    }
                }
            }
            WindowEvent::MouseInput {
                button: MouseButton::Right,
                state,
                ..
            } => {
                self.right = *state == ElementState::Pressed && Self::in_scene(ctx) && !focused;
                self.last = cursor;
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } if !focused => {
                if *state == ElementState::Pressed {
                    if !Self::in_scene(ctx) {
                        self.press = None;
                        return None;
                    }
                    let ray = ctx.camera.camera.cast_ray_from_mouse(
                        ctx.mouse.coords,
                        ctx.config.width as f32,
                        ctx.config.height as f32,
                        &ctx.projection,
                    );
                    self.press = Some(Press {
                        start: cursor,
                        hit: geometry::pick(ray.origin, ray.direction, &editor.document.blocks),
                        floor: Self::floor(ctx),
                        dragging: false,
                        placing: editor.preview.is_some(),
                    });
                } else if let Some(press) = self.press.take() {
                    if !Self::in_scene(ctx) {
                        self.rectangle = None;
                        return None;
                    }
                    if let Some((a, b)) = self.rectangle.take() {
                        if !self.modifiers.shift_key() {
                            editor.selected.clear();
                        }
                        let vp = ctx.projection.calc_matrix() * ctx.camera.camera.calc_matrix();
                        for block in &editor.document.blocks {
                            if let Some([x, y]) = geometry::project(
                                block.transform.center(),
                                vp,
                                [ctx.config.width as f32, ctx.config.height as f32],
                            ) {
                                if (x as f64) >= a[0].min(b[0])
                                    && (x as f64) <= a[0].max(b[0])
                                    && (y as f64) >= a[1].min(b[1])
                                    && (y as f64) <= a[1].max(b[1])
                                {
                                    editor.selected.insert(block.id);
                                }
                            }
                        }
                        editor.scene_revision += 1;
                    } else if press.placing || press.dragging {
                        editor.update_position(Self::floor(ctx));
                        editor.commit_preview();
                        editor.update_position(Self::floor(ctx));
                    } else {
                        editor.select(press.hit, self.modifiers.shift_key());
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } if Self::in_scene(ctx) && !focused => {
                let units = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 60.,
                };
                if editor.preview.is_some() {
                    self.wheel += units;
                    let steps = self.wheel.trunc();
                    self.wheel -= steps;
                    if steps != 0. {
                        editor.rotate(steps);
                        editor.update_position(Self::floor(ctx));
                    }
                } else {
                    self.zoom += units;
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(key) = event.physical_key else {
                    return None;
                };
                if !event.state.is_pressed() {
                    self.keys.remove(&key);
                    return None;
                }
                if key == KeyCode::Escape {
                    self.reset();
                    return Some(Action::Escape);
                }
                if focused {
                    self.keys.clear();
                    return None;
                }
                let command = self.modifiers.control_key() || self.modifiers.super_key();
                if command {
                    if event.repeat {
                        return None;
                    }
                    return match key {
                        KeyCode::KeyC => Some(Action::Copy),
                        KeyCode::KeyV => Some(Action::Paste),
                        KeyCode::KeyZ => Some(if self.modifiers.shift_key() {
                            Action::Redo
                        } else {
                            Action::Undo
                        }),
                        KeyCode::KeyY => Some(Action::Redo),
                        KeyCode::KeyS => Some(Action::Export(true)),
                        _ => None,
                    };
                }
                match key {
                    KeyCode::Escape => {
                        self.reset();
                        return Some(Action::Escape);
                    }
                    KeyCode::ArrowUp => editor.height(1.),
                    KeyCode::ArrowDown => editor.height(-1.),
                    KeyCode::Delete | KeyCode::Backspace if !event.repeat => {
                        return Some(Action::DeleteSelection);
                    }
                    KeyCode::KeyG if !event.repeat => return Some(Action::Move),
                    KeyCode::KeyR if !event.repeat => return Some(Action::Rotate),
                    KeyCode::KeyX => return Some(Action::Axis(0)),
                    KeyCode::KeyY => return Some(Action::Axis(1)),
                    KeyCode::KeyZ => return Some(Action::Axis(2)),
                    KeyCode::KeyW
                    | KeyCode::KeyA
                    | KeyCode::KeyS
                    | KeyCode::KeyD
                    | KeyCode::KeyQ
                    | KeyCode::KeyE => {
                        self.keys.insert(key);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        None
    }
    pub fn update(
        &mut self,
        ctx: &Context,
        dt: std::time::Duration,
        focused: bool,
    ) -> Out<Editor, Event> {
        if focused {
            self.keys.clear();
            self.zoom = 0.;
        }
        let yaw = self.yaw.to_radians();
        let pitch = self.pitch.to_radians();
        let forward = Vector3::new(yaw.cos(), 0., yaw.sin());
        let right = Vector3::new(-yaw.sin(), 0., yaw.cos());
        let mut delta = Vector3::new(0., 0., 0.);
        for key in &self.keys {
            delta += match key {
                KeyCode::KeyW => forward,
                KeyCode::KeyS => -forward,
                KeyCode::KeyD => right,
                KeyCode::KeyA => -right,
                KeyCode::KeyE => Vector3::unit_y(),
                KeyCode::KeyQ => -Vector3::unit_y(),
                _ => Vector3::new(0., 0., 0.),
            };
        }
        if delta.magnitude2() > 0. {
            delta = delta.normalize() * dt.as_secs_f32().min(0.1) * 10.;
        }
        delta += Vector3::new(
            pitch.cos() * yaw.cos(),
            pitch.sin(),
            pitch.cos() * yaw.sin(),
        ) * self.zoom;
        self.zoom = 0.;
        let position = ctx.camera.camera.position + delta;
        let yaw = self.yaw;
        let pitch = self.pitch;
        Out::Configure(Box::new(move |ctx| {
            ctx.camera.camera = Camera::new(position, Deg(yaw), Deg(pitch));
        }))
    }
    pub fn initial_camera() -> Camera {
        Camera::new(Point3::new(0., 12., 15.), Deg(-90.), Deg(-40.))
    }
}
