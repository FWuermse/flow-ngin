use std::{collections::HashSet, fmt::Debug, iter, pin::Pin, sync::Arc};

use instant::{Duration, Instant};

use cgmath::Rotation3;
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::Window,
};

use crate::{
    context::{Context, InitContext, MouseButtonState},
    data_structures::{
        model::{DrawLight, DrawModel},
        texture::Texture,
    },
    pick::draw_to_pick_buffer,
    render::{Flat, Instanced, Render},
};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/**
 * This is the Output Type for every lifecycle hook where the user can pass async events that are
 * handled according to the platform you're running on.
 *
 * `Out::FutEvent` can be used to resolve a future of an Event that is put in the Event Queue after
 * being resolved. The caller is responsible for handling the event later on and it will have no
 * side effects unless handled.
 *
 * `Out::FutFn` can be used to directly modify the state and the mutation is handled internally with
 * no further action required by the callee.
 *
 * `Out::Configure` can be used to modify the Context during runtime for instance to change the tick
 * speed or the clear colour.
 *
 * `Empty` is the default output used when no eventing/futures need to be handled.
 */
pub enum Out<S, E> {
    FutEvent(Vec<Box<dyn Future<Output = E>>>),
    FutFn(Vec<Box<dyn Future<Output = Box<dyn FnOnce(&mut S)>>>>),
    Configure(Box<dyn FnOnce(&mut Context)>),
    Empty,
}

impl<S, E> Default for Out<S, E> {
    fn default() -> Self {
        Self::Empty
    }
}

pub trait GraphicsFlow<S, E> {
    /**
     * This is the only place to modify the Context and configure things like
     * the default background colour or camera start position.
     */
    fn on_init(&mut self, ctx: &mut Context, state: &mut S) -> Out<S, E>;
    /**
     * `on_click` is triggered when something on the screen was clicked that has been rendered by `self`.
     *
     * `id` is the ID that correlates to a specific mesh set via `on_render`.
     * It is advised to use a unique u32 id for each element that should be selectable
     *
     * When the render type `Custom` is used then also picking has to be implemented by the caller.
     * See `flow_ngin::pick::draw_to_pick_buffer` for more information about custom picking.
     */
    fn on_click(&mut self, ctx: &Context, state: &mut S, id: u32) -> Out<S, E>;
    /**
     * `on_update` is invoked on every frame and can be used to progress animations or timers in the scene.
     */
    fn on_update(&mut self, ctx: &Context, state: &mut S, dt: Duration) -> Out<S, E>;
    /**
     * `on_tick` is invoked every `tick_duration_millis` milliseconds and the duration can be configured
     * initially in the `Context` during `on_init` or during runtime via setting the `Out::Configure` output.
     */
    fn on_tick(&mut self, ctx: &Context, state: &mut S) -> Out<S, E>;
    fn on_device_events(&mut self, ctx: &Context, state: &mut S, event: &DeviceEvent) -> Out<S, E>;
    fn on_window_events(&mut self, ctx: &Context, state: &mut S, event: &WindowEvent) -> Out<S, E>;
    // Events can only be consumed by one GraphicsFlow - non consumed events are returned
    fn on_custom_events(&mut self, ctx: &Context, state: &mut S, event: E) -> Option<E>;
    fn on_render<'pass>(&self) -> Render<'_, 'pass>;
}

// Dummy impl to make wasm work
impl<State, Event> Debug for (dyn GraphicsFlow<State, Event> + 'static) {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GraphicsFlow")
    }
}

pub type FlowConsturctor<S, E> =
    Box<dyn FnOnce(InitContext) -> Pin<Box<dyn Future<Output = Box<dyn GraphicsFlow<S, E>>>>>>;

#[derive(Debug)]
pub struct AppState<State: 'static> {
    pub(crate) ctx: Context,
    state: State,
    is_surface_configured: bool,
}
impl<'a, State: Default> AppState<State> {
    async fn new(window: Arc<Window>) -> Self {
        let ctx = Context::new(window).await;
        let ctx = match ctx {
            Ok(ctx) => ctx,
            Err(e) => panic!(
                "App initialization failed. Cannot create the main context: {}",
                e
            ),
        };
        let state = State::default();
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

    fn render<Event>(
        &'a mut self,
        graphics_flows: &mut Vec<Box<dyn GraphicsFlow<State, Event>>>,
    ) -> Result<(), wgpu::SurfaceError> {
        // invoke main render loop
        self.ctx.window.request_redraw();

        // Rendering requires the surface to be configured
        if !self.is_surface_configured {
            return Ok(());
        }

        let output = self
            .ctx
            .surface
            .get_current_texture()
            .expect("Failed to create surface.");
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
            let mut basics: Vec<Instanced> = Vec::new();
            let mut trans: Vec<Instanced> = Vec::new();
            let mut guis: Vec<Flat> = Vec::new();
            let mut terrain: Vec<Flat> = Vec::new();
            graphics_flows.iter_mut().for_each(|flow| {
                let render = flow.on_render();
                render.set_pipelines(
                    &self.ctx,
                    &mut render_pass,
                    &mut basics,
                    &mut trans,
                    &mut guis,
                    &mut terrain,
                );
            });

            render_pass.set_pipeline(&self.ctx.pipelines.basic);
            for instanced in basics {
                render_pass.set_vertex_buffer(1, instanced.instance.slice(..));
                render_pass.draw_model_instanced(
                    &instanced.model,
                    0..instanced.amount as u32,
                    &self.ctx.camera.bind_group,
                    &self.ctx.light.bind_group,
                );
            }

            render_pass.set_pipeline(&self.ctx.pipelines.transparent);
            for instanced in trans {
                render_pass.set_vertex_buffer(1, instanced.instance.slice(..));
                render_pass.draw_model_instanced(
                    &instanced.model,
                    0..instanced.amount as u32,
                    &self.ctx.camera.bind_group,
                    &self.ctx.light.bind_group,
                );
            }

            render_pass.set_pipeline(&self.ctx.pipelines.gui);
            for button in guis {
                render_pass.set_bind_group(0, button.group, &[]);
                render_pass.set_vertex_buffer(0, button.vertex.slice(..));
                render_pass.set_index_buffer(button.index.slice(..), wgpu::IndexFormat::Uint16);
                render_pass.draw_indexed(0..button.amount as u32, 0, 0..1);
            }
        }
        self.ctx.queue.submit(iter::once(encoder.finish()));
        output.present();
        // done with render stuff
        Ok(())
    }
}

pub struct App<State: 'static, Event: 'static> {
    #[cfg(not(target_arch = "wasm32"))]
    async_runtime: tokio::runtime::Runtime,
    proxy: winit::event_loop::EventLoopProxy<FlowEvent<State, Event>>,
    state: Option<AppState<State>>,
    // This will hold the fully initialized flows once they are ready.
    graphics_flows: Vec<Box<dyn GraphicsFlow<State, Event>>>,
    // This holds the constructors at the star.
    // We use Option to `take()` it after use.
    constructors: Option<Vec<FlowConsturctor<State, Event>>>,
    last_time: Instant,
    time_since_tick: Duration,
}

impl<'a, State, Event> App<State, Event>
where
    State: 'static,
    Event: 'static,
{
    fn new(
        event_loop: &EventLoop<FlowEvent<State, Event>>,
        constructors: Vec<FlowConsturctor<State, Event>>,
    ) -> Self {
        let proxy = event_loop.create_proxy();
        #[cfg(not(target_arch = "wasm32"))]
        let async_runtime = tokio::runtime::Runtime::new().unwrap();
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            async_runtime,
            proxy,
            state: None,
            graphics_flows: Vec::new(),
            constructors: Some(constructors),
            last_time: Instant::now(),
            time_since_tick: Duration::from_millis(0),
        }
    }
}

pub(crate) enum FlowEvent<State: 'static, Event: 'static> {
    #[allow(dead_code)]
    Initialized {
        state: AppState<State>,
        flows: Vec<Box<dyn GraphicsFlow<State, Event>>>,
    },
    #[allow(dead_code)]
    Id((u32, HashSet<usize>)),
    #[allow(dead_code)]
    Mut(Box<dyn FnOnce(&mut State)>),
    #[allow(dead_code)]
    Custom(Event),
}
impl<State, Event> Debug for FlowEvent<State, Event> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Initialized { state: _, flows } => {
                f.debug_struct("Initialized").field("flows", flows).finish()
            }
            Self::Id(arg0) => f.debug_tuple("Id").field(arg0).finish(),
            Self::Mut(_) => f.write_str("Mut(|&mut State| -> {...})"),
            Self::Custom(_) => f.write_str("Custom(E)"),
        }
    }
}

impl<State: 'static + Default, Event: 'static> ApplicationHandler<FlowEvent<State, Event>>
    for App<State, Event>
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

        let constructors = self.constructors.take().unwrap();

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
            let (mut app_state, flows) = self.async_runtime.block_on(init_future);
            self.graphics_flows = flows;
            self.graphics_flows.iter_mut().for_each(|flow| {
                let events = flow.on_init(&mut app_state.ctx, &mut app_state.state);
                let proxy = self.proxy.clone();
                handle_flow_output(
                    &self.async_runtime,
                    &mut app_state.state,
                    &mut app_state.ctx,
                    proxy,
                    events,
                );
            });
            self.state = Some(app_state);
        }

        #[cfg(target_arch = "wasm32")]
        {
            let proxy = self.proxy.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let (app_state, flows) = init_future.await;
                assert!(
                    proxy
                        .send_event(FlowEvent::Initialized {
                            state: app_state,
                            flows,
                        })
                        .is_ok()
                );
            });
        }
    }

    #[allow(unused_mut)]
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, mut event: FlowEvent<State, Event>) {
        match event {
            FlowEvent::Initialized { state, flows } => {
                // This is the message from our wasm `spawn_local`
                self.state = Some(state);
                self.graphics_flows = flows;

                // Important: Trigger a resize and redraw now that we are initialized
                let app_state = self.state.as_mut().unwrap();
                let size = app_state.ctx.window.inner_size();
                app_state.resize(size.width, size.height);
                self.graphics_flows.iter_mut().for_each(|flow| {
                    let events = flow.on_init(&mut app_state.ctx, &mut app_state.state);
                    let proxy = self.proxy.clone();
                    handle_flow_output(
                        &self.async_runtime,
                        &mut app_state.state,
                        &mut app_state.ctx,
                        proxy,
                        events,
                    );
                });
                app_state.ctx.window.request_redraw();
            }
            FlowEvent::Id((pick_id, flow_ids)) => {
                if let Some(state) = &mut self.state {
                    state.ctx.mouse.toggle(pick_id);
                    flow_ids.into_iter().for_each(|flow_id| {
                        self.graphics_flows
                            .get_mut(flow_id)
                            .map(|flow| flow.on_click(&state.ctx, &mut state.state, pick_id));
                    });
                }
            }
            FlowEvent::Custom(custom_event) => {
                if let Some(state) = &mut self.state {
                    let result = self
                        .graphics_flows
                        .iter_mut()
                        .fold(Some(custom_event), |event, flow| {
                            flow.on_custom_events(&state.ctx, &mut state.state, event?)
                        });
                    if result.is_some() {
                        log::warn!("Warning! Custom event was not consumed this cycle");
                    }
                }
            }
            FlowEvent::Mut(fn_once) => {
                if let Some(state) = &mut self.state {
                    fn_once(&mut state.state);
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
        self.graphics_flows.iter_mut().for_each(|f| {
            let events = f.on_device_events(&state.ctx, &mut state.state, &event);
            let proxy = self.proxy.clone();
            handle_flow_output(
                &self.async_runtime,
                &mut state.state,
                &mut state.ctx,
                proxy,
                events,
            );
        });
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

        if let WindowEvent::CursorMoved {
            device_id: _,
            position,
        } = event
        {
            state.ctx.mouse.coords = position;
        };

        self.graphics_flows.iter_mut().for_each(|f| {
            let events = f.on_window_events(&state.ctx, &mut state.state, &event);
            let proxy = self.proxy.clone();
            handle_flow_output(
                &self.async_runtime,
                &mut state.state,
                &mut state.ctx,
                proxy,
                events,
            );
        });

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => state.resize(size.width, size.height),
            WindowEvent::RedrawRequested => {
                let dt = self.last_time.elapsed();
                self.last_time = Instant::now();
                self.time_since_tick += dt;

                match state.render(&mut self.graphics_flows) {
                    Ok(_) => {
                        if self.time_since_tick
                            >= Duration::from_millis(state.ctx.tick_duration_millis)
                        {
                            self.graphics_flows.iter_mut().for_each(|f| {
                                let events = f.on_tick(&state.ctx, &mut state.state);
                                let proxy = self.proxy.clone();
                                handle_flow_output(
                                    &self.async_runtime,
                                    &mut state.state,
                                    &mut state.ctx,
                                    proxy,
                                    events,
                                );
                            });
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
                            let events = f.on_update(&state.ctx, &mut state.state, dt);
                            let proxy = self.proxy.clone();
                            handle_flow_output(
                                &self.async_runtime,
                                &mut state.state,
                                &mut state.ctx,
                                proxy,
                                events,
                            );
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
                            if let Some((pick_id, flow_ids)) = draw_to_pick_buffer::<State, Event>(
                                #[cfg(not(target_arch = "wasm32"))]
                                &self.async_runtime,
                                &mut self.graphics_flows,
                                &state.ctx,
                                &state.ctx.mouse,
                                #[cfg(target_arch = "wasm32")]
                                self.proxy.clone(),
                            ) {
                                state.ctx.mouse.toggle(pick_id);
                                if flow_ids.len() > 1 {
                                    log::warn!(
                                        "Multiple flows (incides {:?}) want to react to the render ID {}.",
                                        flow_ids,
                                        pick_id
                                    );
                                }
                                flow_ids.into_iter().for_each(|flow_id| {
                                    self.graphics_flows.get_mut(flow_id).map(|flow| {
                                        let events =
                                            flow.on_click(&state.ctx, &mut state.state, pick_id);
                                        let proxy = self.proxy.clone();
                                        handle_flow_output(
                                            &self.async_runtime,
                                            &mut state.state,
                                            &mut state.ctx,
                                            proxy,
                                            events,
                                        );
                                    });
                                });
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

fn handle_flow_output<State, Event>(
    #[cfg(not(target_arch = "wasm32"))] async_runtime: &tokio::runtime::Runtime,
    state: &mut State,
    ctx: &mut Context,
    proxy: winit::event_loop::EventLoopProxy<FlowEvent<State, Event>>,
    out: Out<State, Event>,
) {
    match out {
        // Send the events passed by the user to winit
        Out::FutEvent(futures) => {
            let fut =
                async move { futures::future::join_all(futures.into_iter().map(Pin::from)).await };
            #[cfg(not(target_arch = "wasm32"))]
            {
                let resolved = async_runtime.block_on(fut);
                resolved.into_iter().for_each(|event| {
                    let err = proxy.send_event(FlowEvent::Custom(event));
                    if let Err(err) = err {
                        log::error!("{}", err);
                        panic!("Event loop was cloesed before all events could be processed.")
                    }
                });
            }

            #[cfg(target_arch = "wasm32")]
            {
                wasm_bindgen_futures::spawn_local(async move {
                    let resolved = fut.await;
                    for event in resolved {
                        assert!(proxy.send_event(FlowEvent::Custom(event)).is_ok());
                    }
                });
            }
        }
        // Mutate the state if the arch supports async, create an event otherwise
        Out::FutFn(futures) => {
            let events: Vec<Pin<Box<dyn Future<Output = Box<dyn FnOnce(&mut State)>>>>> =
                futures.into_iter().map(Pin::from).collect();
            let fut = async move { futures::future::join_all(events.into_iter()).await };
            #[cfg(not(target_arch = "wasm32"))]
            {
                let resolved: Vec<Box<dyn FnOnce(&mut State)>> = async_runtime.block_on(fut);
                resolved.into_iter().for_each(|mutation| {
                    mutation(state);
                });
            }

            #[cfg(target_arch = "wasm32")]
            {
                wasm_bindgen_futures::spawn_local(async move {
                    let resolved = fut.await;
                    for mutation in resolved {
                        assert!(proxy.send_event(FlowEvent::Mut(mutation)).is_ok());
                    }
                });
            }
        }
        Out::Configure(f) => f(ctx),
        Out::Empty => (),
    }
}

pub fn run<State: 'static + Default, Event: 'static>(
    constructors: Vec<FlowConsturctor<State, Event>>,
) -> anyhow::Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        env_logger::init();
    }

    #[cfg(target_arch = "wasm32")]
    {
        console_log::init_with_level(log::Level::Info).unwrap_throw();
    }

    let event_loop: EventLoop<FlowEvent<State, Event>> = EventLoop::with_user_event().build()?;

    let mut app: App<State, Event> = App::new(&event_loop, constructors);

    event_loop.run_app(&mut app)?;

    Ok(())
}
