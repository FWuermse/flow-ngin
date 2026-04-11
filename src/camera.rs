use cgmath::num_traits::ToPrimitive;
use cgmath::*;
use core::f32;
use std::f32::consts::FRAC_PI_2;
use std::time::Duration;
use winit::event::*;
use winit::keyboard::KeyCode;
use winit::{dpi::PhysicalPosition, keyboard::PhysicalKey};

#[rustfmt::skip]
pub const OPENGL_TO_WGPU_MATRIX: cgmath::Matrix4<f32> = cgmath::Matrix4::from_cols(
    cgmath::Vector4::new(1.0, 0.0, 0.0, 0.0),
    cgmath::Vector4::new(0.0, 1.0, 0.0, 0.0),
    cgmath::Vector4::new(0.0, 0.0, 0.5, 0.0),
    cgmath::Vector4::new(0.0, 0.0, 0.5, 1.0),
);

const SAFE_FRAC_PI_2: f32 = FRAC_PI_2 - 0.0001;

pub(crate) fn screen_to_ndc(mouse_x: f32, mouse_y: f32, width: f32, height: f32) -> cgmath::Vector3<f32> {
    let x = if width == 0.0 { 0.0 } else { (2.0 * mouse_x / width) - 1.0 };
    let y = if height == 0.0 { 0.0 } else { 1.0 - (2.0 * mouse_y) / height };
    let z = 1.0;
    Vector3::new(x, y, z)
}

#[derive(Debug)]
pub struct Ray {
    pub origin: Point3<f32>,
    pub direction: Vector3<f32>,
}

// TODO: calculate intersection with depth buffer elem aswell for a picking alternative
impl Ray {
    /**
     * Calculates the intersection of the ray `self` with the floor (y = 0.0).
     *
     * Returns None if the ray is not pointed towards the floor.
     */
    pub fn intersect_with_floor(&self) -> Option<Point2<f32>> {
        if self.direction.y.abs() < f32::EPSILON {
            return None;
        }
        let t = -self.origin.y / self.direction.y;
        if t.is_sign_negative() {
            return None;
        }
        let intersection_point = self.origin + self.direction * t;
        Some(Point2::new(intersection_point.x, intersection_point.z))
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    view_position: [f32; 4],
    view_proj: [[f32; 4]; 4],
}

impl CameraUniform {
    pub fn new() -> Self {
        Self {
            view_position: [0.0; 4],
            view_proj: cgmath::Matrix4::identity().into(),
        }
    }

    pub fn update_view_proj(&mut self, camera: &Camera, projection: &Projection) {
        self.view_position = camera.position.to_homogeneous().into();
        self.view_proj = (projection.calc_matrix() * camera.calc_matrix()).into();
    }
}

#[derive(Debug, Clone)]
pub struct Camera {
    pub position: Point3<f32>,
    yaw: Rad<f32>,
    pitch: Rad<f32>,
}

impl Camera {
    pub fn new<V: Into<Point3<f32>>, Y: Into<Rad<f32>>, P: Into<Rad<f32>>>(
        position: V,
        yaw: Y,
        pitch: P,
    ) -> Self {
        Self {
            position: position.into(),
            yaw: yaw.into(),
            pitch: pitch.into(),
        }
    }

    pub fn calc_matrix(&self) -> Matrix4<f32> {
        let (sin_pitch, cos_pitch) = self.pitch.0.sin_cos();
        let (sin_yaw, cos_yaw) = self.yaw.0.sin_cos();

        Matrix4::look_to_rh(
            self.position,
            Vector3::new(cos_pitch * cos_yaw, sin_pitch, cos_pitch * sin_yaw).normalize(),
            Vector3::unit_y(),
        )
    }

    /**
     * This method casts a ray from the location of the mouse pointer using the camera's FOV and view.
     */
    pub fn cast_ray_from_mouse(
        &self,
        position: PhysicalPosition<f64>,
        width: f32,
        height: f32,
        projection: &Projection,
    ) -> Ray {
        let (mouse_x, mouse_y) = position.into();
        let ndc = screen_to_ndc(mouse_x, mouse_y, width, height);

        let inv_proj_view = (projection.calc_matrix() * self.calc_matrix())
            .invert()
            .unwrap();

        ray_from_ndc(ndc.x, ndc.y, inv_proj_view, self.position)
    }
}

pub(crate) fn ray_from_ndc(
    ndc_x: f32,
    ndc_y: f32,
    inv_proj_view: Matrix4<f32>,
    camera_position: Point3<f32>,
) -> Ray {
    let clip = cgmath::Vector4::new(ndc_x, ndc_y, 1.0, 1.0);
    let mut world_coords = inv_proj_view * clip;
    world_coords /= world_coords.w;
    // TODO: does it make sense to use Point3 or should I stay with Vector3?
    let world_point = Point3::new(world_coords.x, world_coords.y, world_coords.z);
    let ray_direction = (world_point - camera_position).normalize();
    Ray {
        origin: camera_position,
        direction: ray_direction,
    }
}

#[derive(Debug)]
pub struct Projection {
    aspect: f32,
    fovy: Rad<f32>,
    pub znear: f32,
    pub zfar: f32,
}

impl Projection {
    pub fn new<F: Into<Rad<f32>>>(
        width: u32,
        height: u32,
        fovy: F,
        znear: f32,
        zfar: f32,
    ) -> Result<Self, anyhow::Error> {
        let width = width.to_f32().ok_or(anyhow::anyhow!(
            "Width value {} is too large to be represented as f32.",
            width
        ))?;
        let height = height.to_f32().ok_or(
            anyhow::anyhow!(
                "Height value {} is too large to be represented as f32.",
                height
            )
        )?;
        let aspect = width / height;
        Ok(Self {
            aspect,
            fovy: fovy.into(),
            znear,
            zfar,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let width = width.to_f32().unwrap_or(f32::MAX);
        let height = height.to_f32().unwrap_or(f32::MAX);
        if height == 0.0 {
            return;
        }
        self.aspect = width / height;
    }

    pub fn calc_matrix(&self) -> Matrix4<f32> {
        OPENGL_TO_WGPU_MATRIX * perspective(self.fovy, self.aspect, self.znear, self.zfar)
    }
}

#[derive(Debug, Clone)]
pub struct CameraController {
    amount_left: f32,
    amount_right: f32,
    amount_forward: f32,
    amount_backward: f32,
    amount_up: f32,
    amount_down: f32,
    rotate_horizontal: f32,
    rotate_vertical: f32,
    scroll: f32,
    speed: f32,
    sensitivity: f32,
}

impl CameraController {
    pub fn new(speed: f32, sensitivity: f32) -> Self {
        Self {
            amount_left: 0.0,
            amount_right: 0.0,
            amount_forward: 0.0,
            amount_backward: 0.0,
            amount_up: 0.0,
            amount_down: 0.0,
            rotate_horizontal: 0.0,
            rotate_vertical: 0.0,
            scroll: 0.0,
            speed,
            sensitivity,
        }
    }

    pub fn handle_window_events(&mut self, event: &WindowEvent) -> bool {
        if let WindowEvent::KeyboardInput {
            event:
                KeyEvent {
                    physical_key: PhysicalKey::Code(key),
                    state: key_state,
                    ..
                },
            ..
        } = event
        {
            let amount = if key_state.is_pressed() { 1.0 } else { 0.0 };
            match key {
                KeyCode::KeyW | KeyCode::ArrowUp => {
                    self.amount_forward = amount;
                    true
                }
                KeyCode::KeyS | KeyCode::ArrowDown => {
                    self.amount_backward = amount;
                    true
                }
                KeyCode::KeyA | KeyCode::ArrowLeft => {
                    self.amount_left = amount;
                    true
                }
                KeyCode::KeyD | KeyCode::ArrowRight => {
                    self.amount_right = amount;
                    true
                }
                KeyCode::Space => {
                    self.amount_up = amount;
                    true
                }
                KeyCode::ShiftLeft => {
                    self.amount_down = amount;
                    true
                }
                _ => false,
            }
        } else {
            false
        }
    }

    pub fn handle_mouse(&mut self, mouse_dx: f64, mouse_dy: f64) {
        let dx = mouse_dx as f32;
        let dy = mouse_dy as f32;
        // handle f32 to f64 conversion without panicing:
        if dx.is_finite() && dy.is_finite() {
            self.rotate_horizontal = dx;
            self.rotate_vertical = dy;
        } else {
            log::warn!(
                "Mouse coordinates of ({}, {}) are out of bounds and are not updated. The maximum supported coordinate value is {}.",
                mouse_dx,
                mouse_dy,
                f32::MAX
            );
        }
    }

    pub fn handle_scroll(&mut self, delta: &MouseScrollDelta) {
        self.scroll = match delta {
            MouseScrollDelta::LineDelta(_, scroll) => -scroll * 0.5,
            MouseScrollDelta::PixelDelta(PhysicalPosition { y: scroll, .. }) => -*scroll as f32,
        };
    }

    pub fn update(&mut self, camera: &mut Camera, dt: Duration) {
        let dt = dt.as_secs_f32();

        let (yaw_sin, yaw_cos) = camera.yaw.0.sin_cos();
        let forward = Vector3::new(yaw_cos, 0.0, yaw_sin).normalize();
        let right = Vector3::new(-yaw_sin, 0.0, yaw_cos).normalize();
        camera.position += forward * (self.amount_forward - self.amount_backward) * self.speed * dt;
        camera.position += right * (self.amount_right - self.amount_left) * self.speed * dt;

        // Move in/out (aka. "zoom")
        // Note: this isn't an actual zoom. The camera's position
        // changes when zooming. I've added this to make it easier
        // to get closer to an object you want to focus on.
        let (pitch_sin, pitch_cos) = camera.pitch.0.sin_cos();
        let scrollward =
            Vector3::new(pitch_cos * yaw_cos, pitch_sin, pitch_cos * yaw_sin).normalize();
        camera.position += scrollward * self.scroll * self.speed * self.sensitivity * dt;
        self.scroll = 0.0;

        // Move up/down. Since we don't use roll, we can just
        // modify the y coordinate directly.
        camera.position.y += (self.amount_up - self.amount_down) * self.speed * dt;

        // Rotate
        camera.yaw += (Rad(self.rotate_horizontal) * self.speed * self.sensitivity * dt) / 10.0;
        camera.pitch += (Rad(-self.rotate_vertical) * self.speed * self.sensitivity * dt) / 10.0;

        // If process_mouse isn't called every frame, these values
        // will not get set to zero, and the camera will rotate
        // when moving in a non cardinal direction.
        self.rotate_horizontal = 0.0;
        self.rotate_vertical = 0.0;

        // Keep the camera's angle from going too high/low.
        if camera.pitch < -Rad(SAFE_FRAC_PI_2) {
            camera.pitch = -Rad(SAFE_FRAC_PI_2);
        } else if camera.pitch > Rad(SAFE_FRAC_PI_2) {
            camera.pitch = Rad(SAFE_FRAC_PI_2);
        }
    }
}

#[derive(Debug)]
pub struct CameraResources {
    pub camera: Camera,
    pub controller: CameraController,
    pub uniform: CameraUniform,
    pub buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    #[kani::proof]
    #[kani::unwind(1)]
    fn verify_screen_to_ndc_x_range() {
        let width: f32 = kani::any();
        let height: f32 = kani::any();
        kani::assume(width > 0.0 && width < 1e6 && width.is_finite());
        kani::assume(height > 0.0 && height < 1e6 && height.is_finite());
        let mouse_x: f32 = kani::any();
        kani::assume(mouse_x >= 0.0 && mouse_x <= width);
        let mouse_y: f32 = kani::any();
        kani::assume(mouse_y >= 0.0 && mouse_y <= height);
        let ndc = screen_to_ndc(mouse_x, mouse_y, width, height);
        kani::assert(ndc.x >= -1.0 && ndc.x <= 1.0, "NDC x in [-1, 1]");
        kani::assert(ndc.y >= -1.0 && ndc.y <= 1.0, "NDC y in [-1, 1]");
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn verify_intersect_no_panic() {
        let ox: f32 = kani::any();
        let oy: f32 = kani::any();
        let oz: f32 = kani::any();
        let dx: f32 = kani::any();
        let dy: f32 = kani::any();
        let dz: f32 = kani::any();
        kani::assume(ox.is_finite() && oy.is_finite() && oz.is_finite());
        kani::assume(dx.is_finite() && dy.is_finite() && dz.is_finite());
        let ray = Ray {
            origin: cgmath::Point3::new(ox, oy, oz),
            direction: cgmath::Vector3::new(dx, dy, dz),
        };
        let _ = ray.intersect_with_floor();
    }

    #[kani::proof]
    #[kani::unwind(1)]
    fn verify_intersect_upward_none() {
        // An upward ray (dy > 0) from strictly above the floor (oy > 0) returns None.
        let ox: f32 = kani::any();
        let oy: f32 = kani::any();
        let oz: f32 = kani::any();
        let dx: f32 = kani::any();
        let dy: f32 = kani::any();
        let dz: f32 = kani::any();
        kani::assume(ox.is_finite() && oz.is_finite());
        kani::assume(dx.is_finite() && dz.is_finite());
        kani::assume(oy > 0.0 && oy.is_finite());
        kani::assume(dy > f32::EPSILON && dy.is_finite());
        let ray = Ray {
            origin: cgmath::Point3::new(ox, oy, oz),
            direction: cgmath::Vector3::new(dx, dy, dz),
        };
        kani::assert(
            ray.intersect_with_floor().is_none(),
            "upward ray from above floor returns None",
        );
    }

    /// Verify that Projection::resize produces a finite aspect ratio
    /// for any valid (non-zero) dimensions. This proof SHOULD FAIL:
    /// resize(_, 0) divides by zero, producing infinity.
    #[kani::proof]
    #[kani::unwind(1)]
    fn verify_resize_aspect_finite() {
        let w: u32 = kani::any();
        let h: u32 = kani::any();
        kani::assume(w > 0 && w < 10000);
        kani::assume(h > 0 && h < 10000);
        let mut proj = Projection {
            aspect: 1.0,
            fovy: cgmath::Rad(std::f32::consts::FRAC_PI_4),
            znear: 0.1,
            zfar: 100.0,
        };
        proj.resize(w, h);
        kani::assert(proj.aspect.is_finite(), "aspect ratio must be finite for non-zero dimensions");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cgmath::{assert_relative_eq, Deg, InnerSpace, Rad, SquareMatrix};

    // --- screen_to_ndc ---

    #[test]
    fn centre_maps_to_origin() {
        let (w, h) = (800.0f32, 600.0f32);
        let ndc = screen_to_ndc(w / 2.0, h / 2.0, w, h);
        assert_relative_eq!(ndc.x, 0.0, epsilon = 1e-6);
        assert_relative_eq!(ndc.y, 0.0, epsilon = 1e-6);
        assert_relative_eq!(ndc.z, 1.0, epsilon = 1e-6);
    }

    #[test]
    fn top_left_maps_to_minus_one_one() {
        let (w, h) = (800.0f32, 600.0f32);
        let ndc = screen_to_ndc(0.0, 0.0, w, h);
        assert_relative_eq!(ndc.x, -1.0, epsilon = 1e-6);
        assert_relative_eq!(ndc.y, 1.0, epsilon = 1e-6);
    }

    #[test]
    fn bottom_right_maps_to_one_minus_one() {
        let (w, h) = (800.0f32, 600.0f32);
        let ndc = screen_to_ndc(w, h, w, h);
        assert_relative_eq!(ndc.x, 1.0, epsilon = 1e-6);
        assert_relative_eq!(ndc.y, -1.0, epsilon = 1e-6);
    }

    // Projection::resize should guard against zero height to avoid infinite aspect ratio.
    #[test]
    fn resize_with_zero_height_keeps_previous_aspect() {
        let mut proj = Projection::new(800, 600, Deg(45.0), 0.1, 100.0).unwrap();
        let aspect_before = proj.aspect;
        proj.resize(800, 0);
        assert!(proj.aspect.is_finite(), "resize with zero height must not produce infinite aspect");
        assert_relative_eq!(proj.aspect, aspect_before, epsilon = 1e-6);
    }

    // screen_to_ndc must return finite values even for degenerate zero dimensions.
    #[test]
    fn screen_to_ndc_zero_dimensions_returns_finite() {
        let ndc = screen_to_ndc(0.0, 0.0, 0.0, 600.0);
        assert!(ndc.x.is_finite(), "zero width must not produce non-finite NDC x");
        let ndc = screen_to_ndc(0.0, 0.0, 800.0, 0.0);
        assert!(ndc.y.is_finite(), "zero height must not produce non-finite NDC y");
    }

    // --- Ray::intersect_with_floor ---

    #[test]
    fn downward_ray_hits_floor() {
        let ray = Ray {
            origin: Point3::new(0.0, 1.0, 0.0),
            direction: Vector3::new(0.0, -1.0, 0.0),
        };
        let hit = ray.intersect_with_floor().expect("should hit floor");
        assert_relative_eq!(hit.x, 0.0, epsilon = 1e-6);
        assert_relative_eq!(hit.y, 0.0, epsilon = 1e-6);
    }

    #[test]
    fn angled_ray_hits_floor() {
        // Origin at (0,2,0), direction (1,-1,0).normalize()
        let dir = Vector3::new(1.0f32, -1.0, 0.0).normalize();
        let ray = Ray {
            origin: Point3::new(0.0, 2.0, 0.0),
            direction: dir,
        };
        let hit = ray.intersect_with_floor().expect("should hit floor");
        // t = -origin.y / dir.y = -2 / (-1/sqrt(2)) = 2*sqrt(2)
        // x = 0 + dir.x * t = (1/sqrt(2)) * 2*sqrt(2) = 2
        assert_relative_eq!(hit.x, 2.0, epsilon = 1e-5);
    }

    #[test]
    fn upward_ray_returns_none() {
        let ray = Ray {
            origin: Point3::new(0.0, 1.0, 0.0),
            direction: Vector3::new(0.0, 1.0, 0.0),
        };
        assert!(ray.intersect_with_floor().is_none());
    }

    #[test]
    fn horizontal_ray_returns_none() {
        let ray = Ray {
            origin: Point3::new(0.0, 1.0, 0.0),
            direction: Vector3::new(1.0, 0.0, 0.0),
        };
        assert!(ray.intersect_with_floor().is_none());
    }

    #[test]
    fn ray_below_floor_pointing_down_returns_none() {
        let ray = Ray {
            origin: Point3::new(0.0, -1.0, 0.0),
            direction: Vector3::new(0.0, -1.0, 0.0),
        };
        assert!(ray.intersect_with_floor().is_none());
    }

    #[test]
    fn ray_below_floor_pointing_up_hits() {
        let ray = Ray {
            origin: Point3::new(0.0, -1.0, 0.0),
            direction: Vector3::new(0.0, 1.0, 0.0),
        };
        let hit = ray.intersect_with_floor().expect("should hit floor");
        assert_relative_eq!(hit.x, 0.0, epsilon = 1e-6);
        assert_relative_eq!(hit.y, 0.0, epsilon = 1e-6);
    }

    // --- Camera::calc_matrix ---

    #[test]
    fn view_matrix_is_invertible() {
        let camera = Camera::new(
            Point3::new(0.0, 0.0, 5.0),
            Deg(-90.0),
            Deg(0.0),
        );
        let m = camera.calc_matrix();
        assert!(m.invert().is_some());
    }

    #[test]
    fn canonical_camera_matches_look_to_rh() {
        // Camera at origin, yaw = -90°, pitch = 0°
        // direction = (cos(0)*cos(-π/2), sin(0), cos(0)*sin(-π/2)) = (0, 0, -1)
        let camera = Camera::new(
            Point3::new(0.0f32, 0.0, 0.0),
            Rad(-FRAC_PI_2),
            Rad(0.0f32),
        );
        let expected = Matrix4::look_to_rh(
            Point3::new(0.0f32, 0.0, 0.0),
            Vector3::new(0.0f32, 0.0, -1.0),
            Vector3::unit_y(),
        );
        let m = camera.calc_matrix();
        for col in 0..4 {
            for row in 0..4 {
                assert_relative_eq!(m[col][row], expected[col][row], epsilon = 1e-5);
            }
        }
    }

    #[test]
    fn pitch_clamped_by_controller_update() {
        let mut camera = Camera::new(Point3::new(0.0, 0.0, 0.0), Deg(0.0), Deg(0.0));
        let mut ctrl = CameraController::new(1.0, 1.0);
        // Apply large upward rotation
        ctrl.rotate_vertical = -1e6;
        ctrl.update(&mut camera, std::time::Duration::from_secs_f32(1.0));
        assert!(camera.pitch.0 <= SAFE_FRAC_PI_2 + 1e-5);
        assert!(camera.pitch.0 >= -(SAFE_FRAC_PI_2 + 1e-5));
    }

    // --- Projection::calc_matrix ---

    #[test]
    fn projection_matrix_is_invertible() {
        let proj = Projection::new(800, 600, Deg(45.0), 0.1, 100.0).unwrap();
        assert!(proj.calc_matrix().invert().is_some());
    }

    #[test]
    fn projection_aspect_ratio_preserved() {
        let proj_wide = Projection::new(1600, 600, Deg(45.0), 0.1, 100.0).unwrap();
        let proj_square = Projection::new(600, 600, Deg(45.0), 0.1, 100.0).unwrap();
        let m_wide = proj_wide.calc_matrix();
        let m_square = proj_square.calc_matrix();
        // m[0][0] scales x by 1/aspect; wider viewport → smaller value
        assert!(m_wide[0][0] < m_square[0][0]);
    }
}
