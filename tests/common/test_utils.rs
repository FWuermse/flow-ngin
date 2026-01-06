#[cfg(feature = "integration-tests")]
use std::{
    rc::Rc,
    sync::{Arc, Mutex},
};

use flow_ngin::{
    context::Context,
    flow::{GraphicsFlow, Out},
    render::Render,
};
#[cfg(feature = "integration-tests")]
use flow_ngin::{context::GPUResource, data_structures::block::BuildingBlocks, flow::ImageTestResult};
#[cfg(feature = "integration-tests")]
use wgpu::RenderPass;

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

pub(crate) struct FrameCounter {
    id: u32,
}

impl FrameCounter {
    pub(crate) fn frame(&self) -> u32 {
        return self.id;
    }

    pub(crate) fn progress(&mut self) {
        self.id += 1;
    }
}

impl Default for FrameCounter {
    fn default() -> Self {
        Self {
            id: Default::default(),
        }
    }
}

/// This is a simplified flow that uses closures to represent lifecycle hook functions making construction
/// more convenient in test files.
#[cfg(feature = "integration-tests")]
pub(crate) struct TestRender<'a, T> {
    pub(crate) data: T,
    pub(crate) setup: &'a dyn Fn(&mut Context),
    pub(crate) validate: &'a dyn Fn(
        &Context,
        &mut FrameCounter,
        &mut image::ImageBuffer<image::Rgba<u8>, wgpu::BufferView>,
    ) -> Result<ImageTestResult, anyhow::Error>,
}
#[cfg(feature = "integration-tests")]
impl<'a, T> TestRender<'a, T> {
    pub(crate) fn new(
        data: T,
        setup: &'a dyn Fn(&mut Context),
        validate: &'a dyn Fn(
            &Context,
            &mut FrameCounter,
            &mut image::ImageBuffer<image::Rgba<u8>, wgpu::BufferView>,
        ) -> Result<ImageTestResult, anyhow::Error>,
    ) -> Self {
        Self {
            data,
            setup,
            validate,
        }
    }
}

#[cfg(feature = "integration-tests")]
impl<'a, T> GraphicsFlow<FrameCounter, ()> for TestRender<'a, T> where for<'pass, 'b> Render<'b, 'pass>: From<&'b T>, T: BufferWriter {
    fn on_init(&mut self, ctx: &mut Context, s: &mut FrameCounter) -> Out<FrameCounter, ()> {
        let f = self.setup;
        f(ctx);
        Out::Empty
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        (&self.data).into()
    }

    fn render_to_texture(
        &self,
        ctx: &Context,
        s: &mut FrameCounter,
        texture: &mut image::ImageBuffer<image::Rgba<u8>, wgpu::BufferView>,
    ) -> Result<ImageTestResult, anyhow::Error> {
        if format!("{:?}", ctx.config.format).starts_with('B') {
            // TODO: convert [Bgra8UnormSrgb, Rgba8UnormSrgb, Rgb10a2Unorm, Bgra8Unorm, Rgba8Unorm]
        }
        (self.validate)(ctx, s, texture)
    }

    fn on_click(&mut self, _: &Context, _: &mut FrameCounter, _: u32) -> Out<FrameCounter, ()> {
        Out::Empty
    }

    fn on_update(
        &mut self,
        ctx: &Context,
        state: &mut FrameCounter,
        _: std::time::Duration,
    ) -> Out<FrameCounter, ()> {
        state.progress();
        self.data.write_to_buffer(ctx);
        Out::Empty
    }

    fn on_tick(&mut self, _: &Context, _: &mut FrameCounter) -> Out<FrameCounter, ()> {
        Out::Empty
    }

    fn on_device_events(
        &mut self,
        _: &Context,
        _: &mut FrameCounter,
        _: &flow_ngin::DeviceEvent,
    ) -> Out<FrameCounter, ()> {
        Out::Empty
    }

    fn on_window_events(
        &mut self,
        _: &Context,
        _: &mut FrameCounter,
        _: &flow_ngin::WindowEvent,
    ) -> Out<FrameCounter, ()> {
        Out::Empty
    }

    fn on_custom_events(&mut self, _: &Context, _: &mut FrameCounter, event: ()) -> Option<()> {
        Some(event)
    }
}

#[macro_export]
macro_rules! golden_image_test {
    ($graphics_elem:expr) => {{
        use crate::common::test_utils::FrameCounter;
        use flow_ngin::flow::FlowConsturctor;
        use flow_ngin::flow::GraphicsFlow;
        let model_constructor: FlowConsturctor<FrameCounter, ()> = Box::new(|ctx| {
            Box::pin(
                async move { Box::new($graphics_elem(ctx).await) as Box<dyn GraphicsFlow<_, _>> },
            )
        });

        flow_ngin::flow::run(vec![model_constructor])
            .expect("Failed to run flow for integration test.");
    }};
}
