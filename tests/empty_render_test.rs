#[cfg(feature = "integration-tests")]
use wgpu::Color;

use crate::common::test_utils::TestRender;
#[cfg(feature = "integration-tests")]
mod common;
#[test]
#[cfg(feature = "integration-tests")]
fn should_render_clear_colour() {
    use flow_ngin::{
        context::{BufferWriter, Context, InitContext},
        render::Render,
    };

    struct Empty();
    impl<'b, 'pass> From<&'b Empty> for Render<'b, 'pass> {
        fn from(_: &'b Empty) -> Self {
            Render::None
        }
    }
    impl BufferWriter for Empty {
        fn write_to_buffer(&mut self, _: &Context) {} 
    }

    golden_image_test!(async move |_: InitContext| {
        TestRender::new(
            Empty(),
            &|ctx: &mut Context| {
                ctx.clear_colour = Color::WHITE;
                ctx.camera.camera.position = [0.0, 5.0, 2.0].into();
            },
            &|_, state: &mut FrameCounter, texture| {
                if state.frame() > 0 {
                    let colour = Color::WHITE;
                    let f_to_u8 = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                    let desired_pixel = image::Rgba([
                        f_to_u8(colour.r),
                        f_to_u8(colour.g),
                        f_to_u8(colour.b),
                        f_to_u8(colour.a),
                    ]);
                    let pixels = texture.pixels();

                    for pixel in pixels {
                        assert_eq!(*pixel, desired_pixel);
                    }
                    return Ok(flow_ngin::flow::ImageTestResult::Passed);
                } else {
                    return Ok(flow_ngin::flow::ImageTestResult::Waiting);
                }
            },
        )
    });
}