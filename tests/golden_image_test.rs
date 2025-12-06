use flow_ngin::
    render::Render
;
use wgpu::Color;
#[cfg(feature = "integration-tests")]
use crate::common::test_utils::TestRender;

#[cfg(feature = "integration-tests")]
mod common;

#[test]
#[cfg(feature = "integration-tests")]
fn should_not_be_emty_after_render() {
    golden_image_test!(TestRender {
        setup: &|ctx, _| ctx.clear_colour = Color::WHITE,
        render: Render::None,
        validate: &|_, state, texture| {
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
                return Ok(flow_ngin::flow::ImageTestResult::Passed)
            } else {
                return Ok(flow_ngin::flow::ImageTestResult::Waiting)
            }
        },
    });
}
