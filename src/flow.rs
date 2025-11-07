use std::{
    iter,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};

use cgmath::Rotation3;
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::Window,
};

use crate::{
    context::{Context, InitContext, MouseButtonState},
    data_structures::{model::DrawLight, texture::Texture},
    pick::draw_to_pick_buffer,
};

pub trait GraphicsFlow<State, Event> {
    /**
     * This is the only place to modify the Context and configure things like
     * the default background colour or camera start position.
     */
    fn on_init(&mut self, ctx: &mut Context, state: &mut State);
    /**
     * `on_click` is triggered for all GraphicsFlows whenever the user clicks in the scene.
     *
     * `id` is the ID in the picking buffer that corresponds to an object.
     * It is advised to use a unique u32 id for each element that should be selectable
     * and pass that id to the underlying data structures (see `ScreneGraph` or `block`)
     * and match for it when `on_click` triggers.
     *
     * TODO: store flows in a HashMap and only trigger on_click if the key matches
     */
    fn on_click(&mut self, ctx: &Context, state: &mut State, id: u32);
    fn on_update(&mut self, ctx: &Context, state: &mut State, dt: Duration);
    fn on_tick(&mut self, ctx: &Context, state: &mut State);
    fn handle_device_events(&mut self, ctx: &Context, state: &mut State, event: &DeviceEvent);
    fn handle_window_events(&mut self, ctx: &Context, state: &mut State, event: &WindowEvent);
    // Events can only be consumed by one GraphicsFlow - non consumed events are returned
    fn handle_custom_events(
        &mut self,
        ctx: &Context,
        state: &mut State,
        event: Event,
    ) -> Option<Event>;
    fn on_render<'a>(
        &mut self,
        ctx: &'a Context,
        state: &mut State,
        render_pass: &mut wgpu::RenderPass<'a>,
    );
}

pub trait AsyncInit {
    fn new(ctx: InitContext) -> Self;
}

pub type FlowConsturctor<S, E> =
    Box<dyn FnOnce(InitContext) -> Pin<Box<dyn Future<Output = Box<dyn GraphicsFlow<S, E>>>>>>;

pub struct AppState<S> {
    pub(crate) ctx: Context,
    state: S,
    is_surface_configured: bool,
}
impl<'a, S: Default> AppState<S> {
    async fn new(window: Arc<Window>) -> Self {
        let ctx = Context::new(window).await;
        let state = S::default();
        let is_surface_configured = false;
        Self {
            ctx,
            state,
            is_surface_configured,
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.ctx.config.width = width;
            self.ctx.config.height = height;
            self.is_surface_configured = true;
            self.ctx.projection.resize(width, height);
            self.ctx
                .surface
                .configure(&self.ctx.device, &self.ctx.config);
            self.ctx.depth_texture = Texture::create_depth_texture(
                &self.ctx.device,
                [self.ctx.config.width, self.ctx.config.height],
                "depth_texture",
            );
            // TODO: re-render GUI
        }
    }

    fn render<E>(
        &'a mut self,
        graphics_flows: &mut Vec<Box<dyn GraphicsFlow<S, E>>>,
    ) -> Result<(), wgpu::SurfaceError> {
        // invoke main render loop
        self.ctx.window.request_redraw();

        // Rendering requires the surface to be configured
        if !self.is_surface_configured {
            return Ok(());
        }

        let output = self.ctx.surface.get_current_texture().unwrap();
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder: wgpu::CommandEncoder =
            self.ctx
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Render Encoder"),
                });
        {
            let mut render_pass: wgpu::RenderPass<'_> =
                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(self.ctx.clear_colour),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.ctx.depth_texture.view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });

            // Actual rendering:
            if let Some(_) = self.ctx.light.model {
                render_pass.set_pipeline(&self.ctx.pipelines.light);
                render_pass.draw_light_model(
                    &self.ctx.light.model.as_ref().unwrap(),
                    &self.ctx.camera.bind_group,
                    &self.ctx.light.bind_group,
                );
            }
            // TODO: sort by type and use appropriate pipeline (GraphicsFlow should'nt care about the pipeline)
            graphics_flows
                .iter_mut()
                .for_each(|f| f.on_render(&self.ctx, &mut self.state, &mut render_pass));
        }
        self.ctx.queue.submit(iter::once(encoder.finish()));
        output.present();
        // done with render stuff
        Ok(())
    }
}

pub struct App<S, E> {
    state: Option<AppState<S>>,
    // This will hold the fully initialized flows once they are ready.
    graphics_flows: Vec<Box<dyn GraphicsFlow<S, E>>>,
    // This holds the constructors at the start.
    // We use Option to `take()` it after use.
    constructors: Option<Vec<FlowConsturctor<S, E>>>,
    last_time: Instant,
    time_since_tick: Duration,
}

impl<'a, S, E> App<S, E> {
    pub fn new(constructors: Vec<FlowConsturctor<S, E>>) -> Self {
        Self {
            state: None,
            graphics_flows: Vec::new(),       // Starts empty
            constructors: Some(constructors), // Starts with constructors
            last_time: Instant::now(),
            time_since_tick: Duration::from_millis(0),
        }
    }
    // ...
}

enum FlowEvent<State, Event> {
    #[allow(dead_code)]
    Initialized {
        state: AppState<State>,
        flows: Vec<Box<dyn GraphicsFlow<State, Event>>>,
    },
    #[allow(dead_code)]
    Id(u32),
    #[allow(dead_code)]
    Custom(Event),
}

impl<S: 'static + Default, Event: 'static> ApplicationHandler<FlowEvent<S, Event>>
    for App<S, Event>
{
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        #[allow(unused_mut)]
        let mut window_attributes = Window::default_attributes();

        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            use winit::platform::web::WindowAttributesExtWebSys;

            const CANVAS_ID: &str = "canvas";

            let window = wgpu::web_sys::window().unwrap_throw();
            let document = window.document().unwrap_throw();
            let canvas = document.get_element_by_id(CANVAS_ID).unwrap_throw();
            let html_canvas_element = canvas.unchecked_into();
            window_attributes = window_attributes.with_canvas(Some(html_canvas_element));
        }

        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());

        let constructors = self
            .constructors
            .take()
            .expect("Constructors should be present");

        let init_future = async move {
            let app_state = AppState::new(window).await;
            let flow_futures: Vec<_> = constructors
                .into_iter()
                // The clone in into() leverages the internal Arcs of Device and Queue and thus only clones the ref
                .map(|constructor| constructor((&app_state.ctx).into()))
                .collect();
            let flows: Vec<_> = futures::future::join_all(flow_futures).await;
            (app_state, flows)
        };

        #[cfg(not(target_arch = "wasm32"))]
        {
            let (mut app_state, flows) = pollster::block_on(init_future);
            self.graphics_flows = flows;
            self.graphics_flows
                .iter_mut()
                .for_each(|f| f.on_init(&mut app_state.ctx, &mut app_state.state));
            self.state = Some(app_state);
        }

        #[cfg(target_arch = "wasm32")]
        {
            let proxy = event_loop.create_proxy();
            wasm_bindgen_futures::spawn_local(async move {
                let (app_state, flows) = init_future.await;
                proxy
                    .send_event(FlowEvent::Initialized {
                        state: app_state,
                        flows,
                    })
                    .expect("Failed to send initialized event");
            });
        }
    }

    #[allow(unused_mut)]
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, mut event: FlowEvent<S, Event>) {
        match event {
            FlowEvent::Initialized { state, flows } => {
                // This is the message from our wasm `spawn_local`
                self.state = Some(state);
                self.graphics_flows = flows;

                // Important: Trigger a resize and redraw now that we are initialized
                let state = self.state.as_mut().unwrap();
                let size = state.ctx.window.inner_size();
                state.resize(size.width, size.height);
                state.ctx.window.request_redraw();
            }
            FlowEvent::Id(id) => {
                if let Some(state) = &mut self.state {
                    self.graphics_flows.iter_mut().for_each(|f| {
                        f.on_click(&state.ctx, &mut state.state, id);
                    });
                }
            }
            // Events return Option<Event> because they must be consumed (moves contained data)
            FlowEvent::Custom(custom_event) => {
                if let Some(state) = &mut self.state {
                    self.graphics_flows
                        .iter_mut()
                        .fold(Some(custom_event), |event, flow| {
                            flow.handle_custom_events(&state.ctx, &mut state.state, event?)
                        });
                }
            }
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        let state = match &mut self.state {
            Some(state) => state,
            None => return,
        };
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            // TODO: make the below pattern/factor configurable
            let speed_factor = 5.0;
            if let MouseButtonState::Right = state.ctx.mouse.pressed {
                state
                    .ctx
                    .camera
                    .controller
                    .handle_mouse(dx * speed_factor, dy * speed_factor);
            }
        }
        self.graphics_flows
            .iter_mut()
            .for_each(|f| f.handle_device_events(&state.ctx, &mut state.state, &event));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let state = match &mut self.state {
            Some(state) => state,
            None => return,
        };

        // general stuff
        state.ctx.camera.controller.handle_window_events(&event);

        self.graphics_flows
            .iter_mut()
            .for_each(|f| f.handle_window_events(&state.ctx, &mut state.state, &event));

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => state.resize(size.width, size.height),
            WindowEvent::RedrawRequested => {
                let dt = self.last_time.elapsed();
                self.last_time = Instant::now();
                self.time_since_tick += dt;

                match state.render(&mut self.graphics_flows) {
                    Ok(_) => {
                        if self.time_since_tick >= Duration::from_millis(500) {
                            self.graphics_flows
                                .iter_mut()
                                .for_each(|f| f.on_tick(&state.ctx, &mut state.state));
                            self.time_since_tick = Duration::from_millis(0);
                        }
                        // Update the camera
                        state
                            .ctx
                            .camera
                            .controller
                            .update(&mut state.ctx.camera.camera, dt);
                        state
                            .ctx
                            .camera
                            .uniform
                            .update_view_proj(&state.ctx.camera.camera, &state.ctx.projection);
                        state.ctx.queue.write_buffer(
                            &state.ctx.camera.buffer,
                            0,
                            bytemuck::cast_slice(&[state.ctx.camera.uniform]),
                        );
                        // Update the light
                        let old_position: cgmath::Vector3<_> =
                            state.ctx.light.uniform.position.into();
                        state.ctx.light.uniform.position = (cgmath::Quaternion::from_axis_angle(
                            (0.0, 1.0, 0.0).into(),
                            cgmath::Deg(2.0 * dt.as_secs_f32()),
                        ) * old_position)
                            .into();
                        // Update custom stuff
                        self.graphics_flows.iter_mut().for_each(|f| {
                            let _ = f.on_update(&state.ctx, &mut state.state, dt);
                        });
                    }
                    // Reconfigure the surface if it's lost or outdated
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        let size = state.ctx.window.inner_size();
                        state.resize(size.width, size.height);
                    }
                    Err(e) => {
                        log::error!("Unable to render {}", e);
                    }
                }
            }
            WindowEvent::MouseInput {
                state: button_state,
                button,
                ..
            } => {
                if let Some(state) = &mut self.state {
                    match (button, button_state.is_pressed()) {
                        (MouseButton::Left, true) => {
                            state.ctx.mouse.pressed = MouseButtonState::Left;
                            if let Some(id) = draw_to_pick_buffer(&state.ctx, &state.ctx.mouse) {
                                // TODO: store flows in a HashMap and only trigger the matching on_click()
                                self.graphics_flows
                                    .iter_mut()
                                    .for_each(|f| f.on_click(&state.ctx, &mut state.state, id));
                            }
                        }
                        (MouseButton::Right, true) => {
                            state.ctx.mouse.pressed = MouseButtonState::Right;
                        }
                        (_, false) => state.ctx.mouse.pressed = MouseButtonState::None,
                        _ => (),
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn run<S: 'static + Default, E: 'static>(
    constructors: Vec<FlowConsturctor<S, E>>,
) -> anyhow::Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        env_logger::init();
    }

    #[cfg(target_arch = "wasm32")]
    {
        console_log::init_with_level(log::Level::Info).unwrap_throw();
    }

    let event_loop = EventLoop::with_user_event().build()?;

    let mut app: App<S, E> = App::new(constructors);

    event_loop.run_app(&mut app)?;

    Ok(())
}

// TODO: move to client
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn run_web() -> Result<(), wasm_bindgen::JsValue> {
    console_error_panic_hook::set_once();
    run().unwrap_throw();

    Ok(())
}
