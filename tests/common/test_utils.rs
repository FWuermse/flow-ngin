#[cfg(feature = "integration-tests")]
use flow_ngin::flow::ImageTestResult;
use flow_ngin::{
    context::Context,
    flow::{GraphicsFlow, Out},
    render::Render,
};

pub(crate) struct State {
    frame_counter: u32,
    init_invocations: u32,
    click_invocations: u32,
    update_invocations: u32,
    render_invocations: u32,
    pub dummy_state: String,
}
impl State {
    pub fn new() -> Self {
        Self {
            frame_counter: 0,
            init_invocations: 0,
            click_invocations: 0,
            update_invocations: 0,
            render_invocations: 0,
            dummy_state: String::new(),
        }
    }

    pub fn frame(&mut self) {
        self.frame_counter += 1;
    }

    pub fn init(&mut self) {
        self.init_invocations += 1;
    }

    pub fn click(&mut self) {
        self.click_invocations += 1;
    }

    pub fn update(&mut self) {
        self.update_invocations += 1;
    }

    pub fn frame_counter(&self) -> u32 {
        self.frame_counter
    }

    pub fn init_invocations(&self) -> u32 {
        self.init_invocations
    }

    pub fn update_invocations(&self) -> u32 {
        self.update_invocations
    }

    pub fn click_invocations(&self) -> u32 {
        self.click_invocations
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "integration-tests")]
pub(crate) trait ImageFlow<S, E> {
    fn test_setup(&mut self, ctx: &mut Context, state: &mut S) -> Out<S, E>;
    fn render_test<'pass>(&self) -> Render<'_, 'pass>;
    fn validate_render_output(
        &self,
        ctx: &Context,
        state: &mut S,
        texture: &mut image::ImageBuffer<image::Rgba<u8>, wgpu::BufferView>,
    ) -> Result<ImageTestResult, anyhow::Error>;
}

#[cfg(feature = "integration-tests")]
pub(crate) struct Flow<S, T>(pub(crate) Box<dyn ImageFlow<S, T>>);

#[cfg(feature = "integration-tests")]
impl<S, E> GraphicsFlow<S, E> for Flow<S, E> {
    fn on_init(&mut self, ctx: &mut Context, state: &mut S) -> Out<S, E> {
        self.0.test_setup(ctx, state)
    }

    fn on_click(&mut self, _: &Context, _: &mut S, _: u32) -> Out<S, E> {
        Out::Empty
    }

    fn on_update(&mut self, _: &Context, _: &mut S, _: std::time::Duration) -> Out<S, E> {
        Out::Empty
    }

    fn on_tick(&mut self, _: &Context, _: &mut S) -> Out<S, E> {
        Out::Empty
    }

    fn on_device_events(
        &mut self,
        _: &Context,
        _: &mut S,
        _: &flow_ngin::DeviceEvent,
    ) -> Out<S, E> {
        Out::Empty
    }

    fn on_window_events(
        &mut self,
        _: &Context,
        _: &mut S,
        _: &flow_ngin::WindowEvent,
    ) -> Out<S, E> {
        Out::Empty
    }

    fn on_custom_events(&mut self, _: &Context, _: &mut S, event: E) -> Option<E> {
        Some(event)
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        self.0.render_test()
    }

    fn render_to_texture(
        &self,
        ctx: &Context,
        state: &mut S,
        texture: &mut image::ImageBuffer<image::Rgba<u8>, wgpu::BufferView>,
    ) -> Result<ImageTestResult, anyhow::Error> {
        self.0.validate_render_output(ctx, state, texture)
    }
}

pub(crate) struct FrameCounter(pub(crate) u32);
impl Default for FrameCounter {
    fn default() -> Self {
        Self(0)
    }
}
impl FrameCounter {
    pub(crate) fn frame(&self) -> u32 {
        return self.0;
    }

    pub(crate) fn progress(&mut self) {
        self.0 += 1;
    }
}

#[cfg(feature = "integration-tests")]
pub(crate) struct TestRender<'a, 'pass> {
    pub(crate) setup: &'a dyn Fn(&mut Context, &mut FrameCounter),
    pub(crate) render: Render<'a, 'pass>,
    pub(crate) validate:
        &'a dyn Fn(&Context, &mut FrameCounter, &mut image::ImageBuffer<image::Rgba<u8>, wgpu::BufferView>) -> Result<ImageTestResult, anyhow::Error>,
}

#[cfg(feature = "integration-tests")]
impl<'a, 'b> GraphicsFlow<FrameCounter, ()> for TestRender<'a, 'b>
where
    'b: 'a,
{
    fn on_init(&mut self, ctx: &mut Context, s: &mut FrameCounter) -> Out<FrameCounter, ()> {
        (self.setup)(ctx, s);
        Out::Empty
    }

    fn on_render<'pass>(&self) -> flow_ngin::render::Render<'_, 'pass> {
        match &self.render {
            Render::None => Render::None,
            Render::Default(instanced) => Render::Default(instanced.clone()),
            Render::Defaults(instanceds) => Render::Defaults(instanceds.clone()),
            Render::Transparent(instanced) => Render::Transparent(instanced.clone()),
            Render::Transparents(instanceds) => Render::Transparents(instanceds.clone()),
            Render::GUI(flat) => Render::GUI(flat.clone()),
            Render::Terrain(flat) => Render::Terrain(flat.clone()),
            Render::Composed(_) => panic!("Custom not supported in Integration Tests"),
            Render::Custom(_) => panic!("Custom not supported in Integration Tests"),
        }
    }

    fn render_to_texture(
        &self,
        ctx: &Context,
        s: &mut FrameCounter,
        texture: &mut image::ImageBuffer<image::Rgba<u8>, wgpu::BufferView>,
    ) -> Result<ImageTestResult, anyhow::Error> {
        (self.validate)(ctx, s, texture)
    }
    
    fn on_click(&mut self, ctx: &Context, state: &mut FrameCounter, id: u32) -> Out<FrameCounter, ()> {
        Out::Empty
    }
    
    fn on_update(&mut self, ctx: &Context, state: &mut FrameCounter, dt: std::time::Duration) -> Out<FrameCounter, ()> {
        state.progress();
        Out::Empty
    }
    
    fn on_tick(&mut self, ctx: &Context, state: &mut FrameCounter) -> Out<FrameCounter, ()> {
        Out::Empty
    }
    
    fn on_device_events(&mut self, ctx: &Context, state: &mut FrameCounter, event: &flow_ngin::DeviceEvent) -> Out<FrameCounter, ()> {
        Out::Empty
    }
    
    fn on_window_events(&mut self, ctx: &Context, state: &mut FrameCounter, event: &flow_ngin::WindowEvent) -> Out<FrameCounter, ()> {
        Out::Empty
    }
    
    fn on_custom_events(&mut self, ctx: &Context, state: &mut FrameCounter, event: ()) -> Option<()> {
        Some(event)
    }
}

#[macro_export]
macro_rules! golden_image_test {
    ($graphics_elem:expr) => {{
        use crate::common::test_utils::FrameCounter;
        use flow_ngin::flow::FlowConsturctor;
        use flow_ngin::flow::GraphicsFlow;
        let model_constructor: FlowConsturctor<FrameCounter, ()> = Box::new(|_| {
            Box::pin(async move {
                let g_flow: Box<dyn GraphicsFlow<FrameCounter, ()>> = Box::new($graphics_elem);
                g_flow
            })
        });

        flow_ngin::flow::run(vec![model_constructor])
            .expect("Failed to run flow for integration test.");
    }};
}
