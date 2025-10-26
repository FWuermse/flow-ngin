use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::Window,
};

use crate::context::Context;

pub trait GraphicsFlow<'a, State, Event> {
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
    fn on_render(&self, ctx: &'a Context, state: &State, render_pass: &mut wgpu::RenderPass<'a>);
}

pub struct AppState<'a, S, T> {
    pub ctx: Context,
    pub state: S,
    pub graphics_flows: Vec<Box<dyn GraphicsFlow<'a, S, T>>>,
}

pub struct App<'a, S, T> {
    state: Option<AppState<'a, S, T>>,
    last_time: Instant,
    time_since_tick: Duration,
}

enum FlowEvent<T> {
    Id(u32),
    Custom(T),
}

impl<'a, S, T: 'static> ApplicationHandler<FlowEvent<T>> for App<'a, S, T> {
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
            self.state = Some(pollster::block_on(AppState::new(window)).unwrap());
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
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, mut event: FlowEvent<T>) {
        let state = match &mut self.state {
            Some(state) => state,
            None => return,
        };
        Some(event).and_then(|e| {
            if let FlowEvent::Id(id) = e {
                state.graphics_flows.iter_mut().for_each(
                    |f: &mut Box<dyn GraphicsFlow<'_, S, T>>| {
                        f.on_click(&state.ctx, &mut state.state, id);
                    },
                );
                None
            } else {
                Some(e)
            }
            // TODO flatmap state.graphics_flows.handle_custom_event();
        });
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
        state.graphics_flows.iter_mut().for_each(|f| f.handle_device_events(&state.ctx, &mut state.state, &event));
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

        state.graphics_flows.iter_mut().for_each(|f| f.handle_window_events(&state.ctx, &mut state.state, &event));

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            //WindowEvent::Resized(size) => state.resize(size.width, size.height),
            WindowEvent::RedrawRequested => {
                let dt = self.last_time.elapsed();
                self.last_time = Instant::now();
                self.time_since_tick += dt;
                let render_pass = todo!();
                state.graphics_flows.iter_mut().for_each(|f| f.on_render(&state.ctx, &state.state, render_pass));
                // TODO: Handle draw errors
                if self.time_since_tick >= Duration::from_millis(500) {
                    state.graphics_flows.iter_mut().for_each(|f| f.on_tick(&state.ctx, &mut state.state));
                    self.time_since_tick = Duration::from_millis(0);
                }
                state.graphics_flows.iter_mut().for_each(|f| {let _ = f.on_update(&state.ctx, &mut state.state, dt);});
            }
            _ => {}
        }
    }
}

pub fn run() -> anyhow::Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        env_logger::init();
    }
    #[cfg(target_arch = "wasm32")]
    {
        console_log::init_with_level(log::Level::Info).unwrap_throw();
    }

    let event_loop = EventLoop::with_user_event().build()?;
    let mut app = App::new(
        #[cfg(target_arch = "wasm32")]
        &event_loop,
    );
    event_loop.run_app(&mut app)?;

    Ok(())
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn run_web() -> Result<(), wasm_bindgen::JsValue> {
    console_error_panic_hook::set_once();
    run().unwrap_throw();

    Ok(())
}
