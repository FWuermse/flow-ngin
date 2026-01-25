#[cfg(feature = "integration-tests")]
use wgpu::Color;

#[cfg(feature = "integration-tests")]
use crate::common::test_utils::TestRender;
#[cfg(feature = "integration-tests")]
mod common;
#[test]
#[cfg(feature = "integration-tests")]
fn should_render_clear_colour() {
    use flow_ngin::{
        context::{Context, GPUResource, InitContext},
        flow::GraphicsFlow,
        render::Render,
    };

    use crate::common::test_utils::FrameCounter;

    struct Empty();
    impl<'b, 'pass> From<&'b Empty> for Render<'b, 'pass> {
        fn from(_: &'b Empty) -> Self {
            Render::None
        }
    }
    impl<'a, 'pass> GPUResource<'a, 'pass> for Empty {
        fn write_to_buffer(&mut self, _: &wgpu::Queue, _: &wgpu::Device) {}

        fn get_render(&'a self) -> flow_ngin::render::Render<'a, 'pass> {
            Render::None
        }
    }

    impl<'a> From<TestRender<'a, Empty>> for Box<dyn GraphicsFlow<FrameCounter, ()>> {
        fn from(value: TestRender<Empty>) -> Self {
            value.into()
        }
    }

    golden_image_test!(async move |_: InitContext| {
        let empty = Empty();
        TestRender::new(
            empty,
            &|ctx: &mut Context| {
                ctx.clear_colour = Color::WHITE;
                ctx.camera.camera.position = [0.0, 5.0, 2.0].into();
            },
            &|_, state: &mut FrameCounter, texture| {
                {
                    let res = if state.frame() > 0 {
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
                        Ok(flow_ngin::flow::ImageTestResult::Passed)
                    } else {
                        Ok(flow_ngin::flow::ImageTestResult::Waiting)
                    };
                    res
                }
            },
        )
    });
}
