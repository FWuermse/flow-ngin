use std::sync::Arc;

use wgpu::{ExperimentalFeatures, util::DeviceExt};
use winit::{dpi::PhysicalPosition, window::Window};

use crate::{
    camera::{self, CameraResources, CameraUniform, Projection},
    data_structures::texture,
    pipelines::{
        basic::mk_basic_pipeline,
        gui::mk_gui_pipeline,
        light::{LightResources, LightUniform, mk_light_pipeline},
        pick::mk_pick_pipeline,
        pick_gui::mk_gui_pick_pipelin,
        terrain::mk_terrain_pipeline,
        transparent::mk_transparent_pipeline,
    },
};

#[derive(Debug)]
pub enum MouseButtonState {
    Right,
    Left,
    None,
}

#[derive(Debug)]
pub struct MouseState {
    pub coords: PhysicalPosition<f64>,
    pub pressed: MouseButtonState,
    pub selection: Option<u32>,
}
impl MouseState {
    pub(crate) fn toggle(&mut self, pick_id: u32) {
        self.selection = self
            .selection
            .is_none_or(|id| id != pick_id)
            .then_some(pick_id);
    }
}

#[derive(Debug)]
pub struct Pipelines {
    pub light: wgpu::RenderPipeline,
    pub basic: wgpu::RenderPipeline,
    pub pick: wgpu::RenderPipeline,
    pub gui: wgpu::RenderPipeline,
    pub transparent: wgpu::RenderPipeline,
    pub terrain: wgpu::RenderPipeline,
    pub flat_pick: wgpu::RenderPipeline,
}

#[derive(Debug)]
pub struct Context {
    pub(crate) window: Arc<Window>,
    pub(crate) depth_texture: texture::Texture,
    pub tick_duration_millis: u64,
    pub clear_colour: wgpu::Color,
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub mouse: MouseState,
    pub config: wgpu::SurfaceConfiguration,
    pub camera: CameraResources,
    pub projection: Projection,
    pub light: LightResources,
    pub pipelines: Pipelines,
}
impl Context {
    pub(crate) async fn new(window: Arc<Window>) -> Result<Self, anyhow::Error> {
        let size = window.inner_size();

        // The instance is a handle to our GPU
        // BackendBit::PRIMARY => Vulkan + Metal + DX12 + Browser WebGPU
        log::warn!("WGPU setup");
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            #[cfg(not(target_arch = "wasm32"))]
            backends: wgpu::Backends::PRIMARY,
            #[cfg(target_arch = "wasm32")]
            backends: wgpu::Backends::GL,
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;
        log::warn!("device and queue");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                // WebGL doesn't support all of wgpu's features, so if
                // we're building for the web we'll have to disable some.
                required_limits: if cfg!(target_arch = "wasm32") {
                    wgpu::Limits::downlevel_webgl2_defaults()
                } else {
                    wgpu::Limits::default()
                },
                memory_hints: Default::default(),
                trace: wgpu::Trace::Off,
                experimental_features: ExperimentalFeatures::disabled(),
            })
            .await?;

        log::warn!("Surface");
        let surface_caps = surface.get_capabilities(&adapter);
        // Shader code in this tutorial assumes an Srgb surface texture. Using a different
        // one will result all the colors comming out darker. If you want to support non
        // Srgb surfaces, you'll need to account for that when drawing to the frame.
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        // right/left, height, forward/backward - y axis rotation (turn head left/right) - x axis rotation (head up/down)
        let camera = camera::Camera::new((0.0, 30.0, 20.0), cgmath::Deg(-90.0), cgmath::Deg(-60.0));
        let projection =
            camera::Projection::new(config.width, config.height, cgmath::Deg(45.0), 0.1, 500.0)?;
        let camera_controller = camera::CameraController::new(10.0, 0.4);

        let mut camera_uniform = CameraUniform::new();

        camera_uniform.update_view_proj(&camera, &projection);

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group_layout =
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
                label: Some("camera_bind_group_layout"),
            });

        let bind_group_layout = camera_bind_group_layout.clone();

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
            label: Some("camera_bind_group"),
        });

        let camera = CameraResources {
            camera,
            controller: camera_controller,
            uniform: camera_uniform,
            buffer: camera_buffer,
            bind_group: camera_bind_group,
            bind_group_layout,
        };

        let depth_texture = texture::Texture::create_depth_texture(
            &device,
            [config.width, config.height],
            "depth_texture",
        );

        let light_uniform = LightUniform {
            position: [8.0, 80.0, 50.0],
            _padding: 0,
            // change when it's evening
            color: [1.0, 1.0, 1.0],
            _padding2: 0,
        };

        let light = LightResources::new(light_uniform, None, &device);

        let clear_colour = wgpu::Color {
            r: 0.1,
            g: 0.2,
            b: 0.2,
            a: 1.0,
        };

        // Generate pipelines once so they can be reused without being initialized every frame
        let light_pipeline = mk_light_pipeline(
            &device,
            &config,
            &light.bind_group_layout,
            &camera.bind_group_layout,
        );
        let basic_pipeline = mk_basic_pipeline(
            &device,
            &config,
            &light.bind_group_layout,
            &camera.bind_group_layout,
        );
        let pick_pipeline = mk_pick_pipeline(&device, &camera.bind_group_layout);
        let gui_pipeline = mk_gui_pipeline(&device, &config);
        let gui_pick_pipeline = mk_gui_pick_pipelin(&device);
        let transparent_pipeline = mk_transparent_pipeline(
            &device,
            &config,
            &light.bind_group_layout,
            &camera.bind_group_layout,
        );
        let terrain_pipeline = mk_terrain_pipeline(
            &device,
            &config,
            &camera.bind_group_layout,
            &light.bind_group_layout,
        );
        let pipelines = Pipelines {
            basic: basic_pipeline,
            gui: gui_pipeline,
            flat_pick: gui_pick_pipeline,
            light: light_pipeline,
            pick: pick_pipeline,
            transparent: transparent_pipeline,
            terrain: terrain_pipeline,
        };
        let mouse = MouseState {
            coords: (0.0, 0.0).into(),
            pressed: MouseButtonState::None,
            selection: None,
        };
        let tick_duration_millis = 500;

        Ok(Self {
            camera,
            clear_colour,
            config,
            depth_texture,
            device,
            light,
            mouse,
            pipelines,
            projection,
            queue,
            surface,
            tick_duration_millis,
            window,
        })
    }
}

#[derive(Clone)]
pub struct InitContext {
    pub queue: wgpu::Queue,
    pub device: wgpu::Device,
}
impl From<&Context> for InitContext {
    fn from(ctx: &Context) -> Self {
        Self {
            // Queue and Device can be cloned as they're internally handled as Arc
            queue: ctx.queue.clone(),
            device: ctx.device.clone(),
        }
    }
}
