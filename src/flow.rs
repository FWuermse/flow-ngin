use std::{sync::Arc, time::{Duration, Instant}};

use winit::{application::ApplicationHandler, event::{DeviceEvent, DeviceId, WindowEvent}, event_loop::{ActiveEventLoop, EventLoop}, window::Window};

use crate::{camera::CameraResources, pipelines::light::LightResources};

// TODO: move camera to state and make ctx immutable
pub struct Context {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub camera: CameraResources,
    pub light: LightResources,
}
impl Context {
    fn new() -> Self {
        Self { surface: todo!(), device: todo!(), queue: todo!(), config: todo!(), camera: todo!(), light: todo!() }
    }
}

pub trait GraphicsFlow {
    fn init(&mut self, ctx: &mut Context, state: &mut State);
    fn update(&mut self, ctx: &mut Context, state: &mut State, dt: Duration);
    fn handle_device_events(&mut self, ctx: &mut Context, event: winit::event::DeviceEvent);
    fn handle_window_events(&mut self, ctx: &mut Context, event: winit::event::WindowEvent);
    fn render(&self);
}

type Flow = Box<dyn GraphicsFlow>;

pub struct State {
    pub ctx: Context,
    pub graphics_flows: Vec<Flow>,
}
impl State {
    async fn new(window: Arc<winit::window::Window>) -> anyhow::Result<Self> {
        Ok(Self { graphics_flows: Vec::new(), ctx: Context::new() })
    }
}

// TODO: make extensible
pub enum Event {
    State(State)
}

pub struct App {
    #[cfg(target_arch = "wasm32")]
    proxy: Option<winit::event_loop::EventLoopProxy<Event>>,
    state: Option<State>,
    last_time: Instant,
    // TODO use for configurable game ticks
    time_since_tick: Duration,
}

impl App {
    pub fn new(#[cfg(target_arch = "wasm32")] event_loop: &EventLoop<Event>) -> Self {
        #[cfg(target_arch = "wasm32")]
        let proxy = Some(event_loop.create_proxy());
        Self {
            state: None,
            #[cfg(target_arch = "wasm32")]
            proxy,
            last_time: Instant::now(),
            time_since_tick: Duration::from_millis(0),
        }
    }
}

impl ApplicationHandler<Event> for App {
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
            // If we are not on web we can use pollster to await the event
            // TODO: switch to tokio for non-wasm stuff
            self.state = Some(pollster::block_on(State::new(window)).unwrap());
        }

        #[cfg(target_arch = "wasm32")]
        {
            if let Some(proxy) = self.proxy.take() {
                wasm_bindgen_futures::spawn_local(async move {
                    assert!(proxy
                        .send_event(Event::State(
                            State::new(window, Some(proxy.clone()))
                                .await
                                .expect("Unable to create canvas!!!")
                        ))
                        .is_ok())
                });
            }
        }
    }

    #[allow(unused_mut)]
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, mut event: Event) {
        match event {
            // TODO: make extensible
            Event::State(mut state) => {
                #[cfg(target_arch = "wasm32")]
                {
                    state.window.request_redraw();
                    state.resize(
                        state.window.inner_size().width,
                        state.window.inner_size().height,
                    );
                }
                self.state = Some(state);
            }
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        let state = if let Some(state) = &mut self.state {
            state
        } else {
            return;
        };
        for flow in &mut state.graphics_flows {
            flow.handle_device_events(&mut state.ctx, event.clone());
        }
    }

    fn window_event(
        &mut self,
        // TODO include event_loop for wasm async stuff
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let state = match &mut self.state {
            Some(canvas) => canvas,
            None => return,
        };
        for flow in &mut state.graphics_flows {
            flow.handle_window_events(&mut state.ctx, event.clone());
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
