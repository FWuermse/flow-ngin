#[cfg(feature = "integration-tests")]
use crate::common::test_utils::TestRender;
use wgpu::Color;

#[cfg(feature = "integration-tests")]
mod common;

#[test]
#[cfg(feature = "integration-tests")]
fn should_have_correct_tex_coords() {
    use flow_ngin::{
        context::{Context, InitContext},
        resources::load_model_gltf,
    };
    use image::open;
    golden_image_test!(async move |ctx: InitContext| {
        let model = load_model_gltf(1, "wood_hut_v2.gltf", &ctx.device, &ctx.queue).await.unwrap();
        TestRender::new(
            model,
            &|ctx: &mut Context| {
                ctx.clear_colour = Color::WHITE;
                ctx.camera.camera.position = [0.0, 5.0, 2.0].into();
            },
            &|_, state: &mut FrameCounter, actual| {
                if state.frame() > 0 {
                    let expected = open("tests/fixtures/wood_hut.png")
                        .expect("failed to load fixture")
                        .to_rgba8();

                    assert_eq!(
                        actual.dimensions(),
                        expected.dimensions(),
                        "image sizes differ"
                    );

                    for (x, y, pixel_actual) in actual.enumerate_pixels() {
                        let pixel_expected = expected.get_pixel(x, y);
                        assert_eq!(
                            pixel_actual, pixel_expected,
                            "pixel mismatch at ({}, {})",
                            x, y
                        );
                    }
                    return Ok(flow_ngin::flow::ImageTestResult::Passed);
                } else {
                    return Ok(flow_ngin::flow::ImageTestResult::Waiting);
                }
            },
        )
    });
}
