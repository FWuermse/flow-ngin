use std::fmt;

use cgmath::Rotation3;
use instant::Duration;
use wgpu::util::DeviceExt;

pub struct LightUpdate(Box<dyn FnMut(&mut LightUniform, &Camera, Duration)>);

impl LightUpdate {
    pub fn new(update: impl FnMut(&mut LightUniform, &Camera, Duration) + 'static) -> Self {
        Self(Box::new(update))
    }

    pub(crate) fn update(&mut self, light: &mut LightUniform, camera: &Camera, dt: Duration) {
        (self.0)(light, camera, dt);
    }
}

impl Default for LightUpdate {
    fn default() -> Self {
        Self::new(|light, _camera, dt| {
            let position: cgmath::Vector3<_> = light.position.into();
            light.position = (cgmath::Quaternion::from_axis_angle(
                cgmath::Vector3::unit_y(),
                cgmath::Deg(2.0 * dt.as_secs_f32()),
            ) * position)
                .into();
        })
    }
}

impl fmt::Debug for LightUpdate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LightUpdate { .. }")
    }
}

use crate::{camera::Camera, data_structures::{
    model::{Model, ModelVertex, Vertex},
    texture,
}};

#[derive(Debug)]
pub struct LightResources {
    pub model: Option<Model>,
    pub uniform: LightUniform,
    pub buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl LightResources {
    pub fn new(
        light_uniform: LightUniform,
        model: Option<Model>,
        device: &wgpu::Device,
    ) -> Self {
        let light_buffer = mk_buffer(&device, light_uniform);
        let light_bind_group_layout = mk_bind_group_layout(&device);
        let light_bind_group = mk_bind_group(
            &device,
            &light_bind_group_layout,
            light_buffer.as_entire_binding(),
        );
        Self {
            model,
            uniform: light_uniform,
            buffer: light_buffer,
            bind_group: light_bind_group,
            bind_group_layout: light_bind_group_layout.clone(),
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightUniform {
    // TODO: make private and create nicer API for light sources
    pub position: [f32; 3],
    // Due to uniforms requiring 16 byte (4 float) spacing, we need to use a padding field here
    pub _padding: u32,
    pub color: [f32; 3],
    // Due to uniforms requiring 16 byte (4 float) spacing, we need to use a padding field here
    pub _padding2: u32,
}

fn mk_buffer(device: &wgpu::Device, light_uniform: LightUniform) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Light Vertex Buffer"),
        contents: bytemuck::cast_slice(&[light_uniform]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    })
}

fn mk_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
        label: None,
    })
}

fn mk_bind_group(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    light_buffer: wgpu::BindingResource<'_>,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: light_buffer,
        }],
        label: None,
    })
}

pub fn mk_light_pipeline(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
    light_bind_group_layout: &wgpu::BindGroupLayout,
    camera_bind_group_layout: &wgpu::BindGroupLayout,
    sample_count: u32,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Light Pipeline Layout"),
        bind_group_layouts: &[Some(camera_bind_group_layout), Some(light_bind_group_layout)],
        ..Default::default()
    });
    let shader = wgpu::ShaderModuleDescriptor {
        label: Some("Light Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("light.wgsl").into()),
    };
    crate::pipelines::basic::mk_render_pipeline(
        &device,
        wgpu::FrontFace::Ccw,
        &layout,
        config.format,
        Some(wgpu::BlendState {
            alpha: wgpu::BlendComponent::REPLACE,
            color: wgpu::BlendComponent::REPLACE,
        }),
        Some(texture::Texture::DEPTH_FORMAT),
        &[ModelVertex::desc()],
        shader,
        sample_count,
    )
}

#[cfg(test)]
mod tests {
    use cgmath::Rad;

use super::*;

    fn light() -> LightUniform {
        LightUniform {
            position: [10.0, 5.0, 0.0],
            color: [1.0; 3],
            _padding: 0,
            _padding2: 0,
        }
    }

    #[test]
    fn default_orbit_preserves_height_and_colour() {
        let mut light = light();
        let camera = &Camera::new([0.0;3], Rad(0.0), Rad(0.0));
        LightUpdate::default().update(&mut light, camera, Duration::from_secs(45));
        assert!(light.position[0].abs() < 0.0001);
        assert!((light.position[1] - 5.0).abs() < 0.0001);
        assert!((light.position[2] + 10.0).abs() < 0.0001);
        assert_eq!(light.color, [1.0; 3]);
    }

    #[test]
    fn custom_update_keeps_state_and_replaces_default_orbit() {
        let mut elapsed = 0.0;
        let mut update = LightUpdate::new(move |light, _camera, dt| {
            elapsed += dt.as_secs_f32();
            light.position = [elapsed, 20.0, 30.0];
            light.color = [elapsed / 10.0, 0.5, 0.25];
        });
        let mut light = light();
        let camera = &Camera::new([0.0;3], Rad(0.0), Rad(0.0));
        update.update(&mut light, camera, Duration::from_millis(500));
        update.update(&mut light, camera, Duration::from_millis(1500));
        assert_eq!(light.position, [2.0, 20.0, 30.0]);
        assert_eq!(light.color, [0.2, 0.5, 0.25]);
    }

    #[test]
    fn no_op_disables_motion_and_default_can_be_restored() {
        let mut light = light();
        let camera = &Camera::new([0.0;3], Rad(0.0), Rad(0.0));
        let mut update = LightUpdate::new(|_, _, _| {});
        update.update(&mut light, camera, Duration::from_secs(45));
        assert_eq!(light.position, [10.0, 5.0, 0.0]);
        assert_eq!(light.color, [1.0; 3]);

        update = LightUpdate::default();
        update.update(&mut light, camera, Duration::ZERO);
        assert_eq!(light.position, [10.0, 5.0, 0.0]);
        update.update(&mut light, camera, Duration::from_secs(45));
        assert!((light.position[2] + 10.0).abs() < 0.0001);
    }
}
