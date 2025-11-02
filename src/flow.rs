use std::{
    iter,
    sync::Arc,
    time::{Duration, Instant},
};

use cgmath::Rotation3;
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::Window,
};

use crate::{
    context::Context,
    data_structures::{model::DrawLight, texture::Texture},
};

pub trait GraphicsFlow<State, Event> {
    fn on_init(&mut self, ctx: &Context, state: &mut State);
    /**
     * `on_click` is triggered for all GraphicsFlows whenever the user clicks in the scene.
     *
     * `id` is the ID in the picking buffer that corresponds to an object.
     * It is advised to use a unique u32 id for each element that should be selectable
     * and pass that id to the underlying data structures (see `ScreneGraph` or `block`)
     * and match for it when `on_click` triggers.
     */
    fn on_click(&mut self, ctx: &Context, state: &mut State, id: u32);
    fn on_update(&mut self, ctx: &Context, state: &mut State, dt: Duration) -> Vec<u32>;
    fn on_tick(&mut self, ctx: &Context, state: &mut State);
    fn handle_device_events(&mut self, ctx: &Context, state: &mut State, event: &DeviceEvent);
    fn handle_window_events(&mut self, ctx: &Context, state: &mut State, event: &WindowEvent);
    // Events can only be consumed by one GraphicsFlow - non consumed events are returned
    // TODO: reconsider the Event. Should it be used to carry data? If so, maybe only Clonable data.
    fn handle_custom_events(
        &mut self,
        ctx: &Context,
        state: &mut State,
        event: Event,
    ) -> Option<Event>;
    // Ctx must live as long as RenderPass while state must live shorter.
    // TODO: remove state here entirely. It's not self's responsibility to read from state.
    fn on_render<'a>(
        &mut self,
        ctx: &'a Context,
        state: &mut State,
        render_pass: &mut wgpu::RenderPass<'a>,
    );
}

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
        // render stuff
        self.ctx.window.request_redraw();

        // We can't render unless the surface is configured
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
                            // TODO: make background colour configurable
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.1,
                                g: 0.2,
                                b: 0.2,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &view,
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
                render_pass.set_pipeline(&self.ctx.light.render_pipeline);
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

pub struct App<S, T> {
    state: Option<AppState<S>>,
    pub(crate) graphics_flows: Vec<Box<dyn GraphicsFlow<S, T>>>,
    last_time: Instant,
    time_since_tick: Duration,
}

impl<'a, S, E> App<S, E> {
    pub fn new(
        #[cfg(target_arch = "wasm32")] event_loop: &EventLoop<Event>,
        graphics_flows: Vec<Box<dyn GraphicsFlow<S, E>>>,
    ) -> Self {
        #[cfg(target_arch = "wasm32")]
        let proxy = Some(event_loop.create_proxy());
        let state = None;
        Self {
            state,
            #[cfg(target_arch = "wasm32")]
            proxy,
            last_time: Instant::now(),
            time_since_tick: Duration::from_millis(0),
            graphics_flows,
        }
    }
    pub fn get_mut(&mut self) -> &mut AppState<S> {
        self.state.as_mut().unwrap()
    }
}

enum FlowEvent<State, Event> {
    #[allow(dead_code)]
    Id(u32),
    #[allow(dead_code)]
    State(AppState<State>),
    #[allow(dead_code)]
    Custom(Event),
}

fn run_async<F: IntoFuture>(fut: F) -> F::Output {
    #[cfg(not(target_arch = "wasm32"))]
    {
        pollster::block_on(fut)
    }

    #[cfg(target_arch = "wasm32")]
    {
        if let Some(proxy) = self.proxy.take() {
            wasm_bindgen_futures::spawn_local(async move {
                assert!(
                    proxy
                        .send_event(Event::State(
                            State::new(window, Some(proxy.clone()))
                                .await
                                .expect("Unable to create canvas!!!")
                        ))
                        .is_ok()
                )
            });
        }
    }
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

        #[cfg(not(target_arch = "wasm32"))]
        {
            // If we are not on web we can use pollster to
            // await the
            self.state = Some(pollster::block_on(AppState::new(window)));
        }

        #[cfg(target_arch = "wasm32")]
        {
            if let Some(proxy) = self.proxy.take() {
                wasm_bindgen_futures::spawn_local(async move {
                    assert!(
                        proxy
                            .send_event(Event::State(
                                State::new(window, Some(proxy.clone()))
                                    .await
                                    .expect("Unable to create canvas!!!")
                            ))
                            .is_ok()
                    )
                });
            }
        }
    }

    #[allow(unused_mut)]
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, mut event: FlowEvent<S, Event>) {
        let state = match &mut self.state {
            Some(state) => state,
            None => return,
        };
        Some(event)
            .and_then(|e| {
                if let FlowEvent::Id(id) = e {
                    self.graphics_flows.iter_mut().for_each(
                        |f: &mut Box<dyn GraphicsFlow<S, Event>>| {
                            f.on_click(&state.ctx, &mut state.state, id);
                        },
                    );
                    None
                } else {
                    Some(e)
                }
            })
            .and_then(|e| {
                if let FlowEvent::State(mut state) = e {
                    #[cfg(target_arch = "wasm32")]
                    {
                        state.window.request_redraw();
                        state.resize(
                            state.window.inner_size().width,
                            state.window.inner_size().height,
                        );
                    }
                    self.state = Some(state);
                    None
                } else {
                    Some(e)
                }
            });
        // TODO flatmap state.graphics_flows.handle_custom_event();
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
            _ => {}
        }
    }
}

pub struct NGIN<S, E> {
    app: App<S, E>,
}

impl<S, E> NGIN<S, E> {
    pub fn mk() -> Self {
        let app: App<S, E> = App::new(
            #[cfg(target_arch = "wasm32")]
            &event_loop,
            Vec::new(),
        );
        Self { app }
    }
    pub fn run<'a, F, Fut>(&mut self, graphics_flows: Vec<F>) -> anyhow::Result<()>
    where
        S: 'static + Default,
        E: 'static,
        F: for<'b> Fn(&'b Context) -> Fut,
        Fut: Future<Output = Result<(), anyhow::Error>>,
    {
        #[cfg(not(target_arch = "wasm32"))]
        {
            env_logger::init();
        }
        #[cfg(target_arch = "wasm32")]
        {
            console_log::init_with_level(log::Level::Info).unwrap_throw();
        }
        //self.app.graphics_flows = graphics_flows;

        let event_loop = EventLoop::with_user_event().build()?;

        event_loop.run_app(&mut self.app)?;

        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn run_web() -> Result<(), wasm_bindgen::JsValue> {
    console_error_panic_hook::set_once();
    run().unwrap_throw();

    Ok(())
}
