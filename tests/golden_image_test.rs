use flow_ngin::{
    context::Context,
    flow::{FlowConsturctor, GraphicsFlow, Out},
    render::Render,
};
use wgpu::Color;

mod common;

struct Event();
struct State(u32);
impl Default for State {
    fn default() -> Self {
        Self(0)
    }
}

struct GraphicsElement();

#[cfg(feature = "integration-tests")]
impl GraphicsFlow<State, Event> for GraphicsElement {
    fn on_init(&mut self, ctx: &mut Context, _: &mut State) -> Out<State, Event> {
        ctx.clear_colour = Color::TRANSPARENT;
        Out::Empty
    }

    fn on_click(&mut self, _: &Context, _: &mut State, _: u32) -> Out<State, Event> {
        Out::Empty
    }

    fn on_update(
        &mut self,
        _: &Context,
        s: &mut State,
        _: std::time::Duration,
    ) -> Out<State, Event> {
        s.0 += 1;
        Out::Empty
    }

    fn on_tick(&mut self, _: &Context, _: &mut State) -> Out<State, Event> {
        Out::Empty
    }

    fn on_device_events(
        &mut self,
        _: &Context,
        _: &mut State,
        _: &flow_ngin::DeviceEvent,
    ) -> Out<State, Event> {
        Out::Empty
    }

    fn on_window_events(
        &mut self,
        _: &Context,
        _: &mut State,
        _: &flow_ngin::WindowEvent,
    ) -> Out<State, Event> {
        Out::Empty
    }

    fn on_custom_events(&mut self, _: &Context, _: &mut State, e: Event) -> Option<Event> {
        Some(e)
    }

    fn on_render<'pass>(&self) -> flow_ngin::render::Render<'_, 'pass> {
        Render::None
    }

    fn render_to_texture(
        &self,
        s: &mut State,
        texture: &mut image::ImageBuffer<image::Rgba<u8>, wgpu::BufferView>,
    ) -> Result<(), anyhow::Error> {
        println!("render to texture");
        if s.0 > 0 {
            println!("printing");
            texture.save("image.png").unwrap();
        }
        Ok(())
    }
}

#[test]
#[cfg(feature = "integration-tests")]
fn should_not_be_emty_after_render() {
    let model_constructor: FlowConsturctor<State, Event> = Box::new(|_| {
        Box::pin(async move { Box::new(GraphicsElement()) as Box<dyn GraphicsFlow<_, _>> })
    });

    let err = flow_ngin::flow::run(vec![model_constructor]);
    match err {
        Ok(_) => (),
        Err(e) => {
            println!("{}", e);
            panic!("{}", e);
        }
    }
}
