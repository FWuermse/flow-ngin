#[cfg(feature = "integration-tests")]
use flow_ngin::flow::ImageTestResult;
use flow_ngin::{
    context::Context,
    flow::{FlowConsturctor, GraphicsFlow, Out}, render::Render,
};
use wgpu::Color;

use crate::common::test_utils::State;

mod common;

enum Event {
    Test,
}

struct GraphicsElement(u32);

#[cfg(feature = "integration-tests")]
impl GraphicsFlow<State, Event> for GraphicsElement {
    fn on_init(&mut self, ctx: &mut Context, state: &mut State) -> Out<State, Event> {
        ctx.clear_colour = Color::TRANSPARENT;
        assert_eq!(state.frame_counter(), 0);
        assert_eq!(state.init_invocations(), 0);
        assert_eq!(state.click_invocations(), 0);
        assert_eq!(state.update_invocations(), 0);

        state.init();
        Out::Empty
    }

    fn on_click(&mut self, _: &Context, state: &mut State, _: u32) -> Out<State, Event> {
        state.click();
        Out::Empty
    }

    fn on_update(
        &mut self,
        ctx: &Context,
        state: &mut State,
        _: std::time::Duration,
    ) -> Out<State, Event> {
        assert_eq!(state.frame_counter(), state.update_invocations());
        assert_eq!(state.init_invocations(), 1);
        state.frame();
        state.update();

        // test scenarios:
        let serve_sencha: Box<dyn FnOnce(&mut State)> = Box::new(|state: &mut State| {
            state.dummy_state.push('🍵');
        });
        let serve_mate: Box<dyn FnOnce(&mut State)> = Box::new(|state: &mut State| {
            state.dummy_state.push('🧉');
        });
        match state.frame_counter() {
            3 => Out::FutEvent(vec![Box::new(async move { Event::Test })]),
            5 => Out::FutFn(vec![
                Box::new(async move { serve_sencha }),
                Box::new(async move { serve_mate }),
            ]),
            6 => {
                // Hopefully this will kill the program?
                println!("done");
                Out::Empty
            }
            x if x > 5 => {
                assert!(state.dummy_state.contains('🧉'));
                assert!(state.dummy_state.contains('🍵'));
                // emojis are 4chars wide.
                assert_eq!(state.dummy_state.len(), 8, "{}", state.dummy_state);
                Out::Empty
            }
            _ => Out::Empty,
        }
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

    fn on_custom_events(&mut self, _: &Context, state: &mut State, _: Event) -> Option<Event> {
        // we send the event in frame 3
        assert!(state.frame_counter() >= 3);
        assert!(state.update_invocations() >= 3);
        None
    }

    fn on_render<'pass>(&self) -> flow_ngin::render::Render<'_, 'pass> {
        Render::None
    }

    fn render_to_texture(
        &self,
        _: &Context,
        _: &mut State,
        _: &mut image::ImageBuffer<image::Rgba<u8>, wgpu::BufferView>,
    ) -> std::result::Result<ImageTestResult, anyhow::Error> {
        Ok(ImageTestResult::Passed)
    }
}

#[test]
#[cfg(feature = "integration-tests")]
fn should_not_be_emty_after_render() {
    let model_constructor: FlowConsturctor<State, Event> = Box::new(|_| {
        Box::pin(async move {
            Box::new(GraphicsElement(0)) as Box<dyn GraphicsFlow<_, _>>
        })
    });

    let err = flow_ngin::flow::run(vec![model_constructor]);
    match err {
        Ok(_) => (),
        Err(e) => {
            println!("{}", e);
            panic!("{}", e);
        },
    }
}
