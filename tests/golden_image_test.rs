#[cfg(feature = "integration-tests")]
use crate::common::test_utils::TestRender;
use flow_ngin::render::Render;
use wgpu::Color;

#[cfg(feature = "integration-tests")]
mod common;

#[test]
#[cfg(feature = "integration-tests")]
fn should_render_clear_colour() {
    use cgmath::One;
    use flow_ngin::{
        context::{Context, InitContext},
        data_structures::block::BuildingBlocks,
    };

    golden_image_test!(async move |ctx: InitContext| {
        let model = BuildingBlocks::new(
            0,
            &ctx.queue,
            &ctx.device,
            [0.0; 3].into(),
            flow_ngin::Quaternion::one(),
            1,
            "Rock1.obj",
        )
        .await;
        TestRender::new(
            model,
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

#[test]
#[cfg(feature = "integration-tests")]
fn should_match_rock_collection_render() {
    use cgmath::One;
    use flow_ngin::{
        context::{Context, InitContext},
        data_structures::block::BuildingBlocks,
    };
    use image::open;
    golden_image_test!(async move |ctx: InitContext| {
        let model = BuildingBlocks::new(
            0,
            &ctx.queue,
            &ctx.device,
            [0.0; 3].into(),
            flow_ngin::Quaternion::one(),
            1,
            "Rock1.obj",
        )
        .await;
        TestRender::new(
            model,
            &|ctx: &mut Context| {
                ctx.clear_colour = Color::WHITE;
                ctx.camera.camera.position = [0.0, 5.0, 2.0].into();
            },
            &|_, state: &mut FrameCounter, actual| {
                if state.frame() > 0 {
                    let expected = open("tests/fixtures/astroids.png")
                        .expect("failed to load fixture")
                        .to_rgba8();

                    assert_eq!(
                        actual.dimensions(),
                        expected.dimensions(),
                        "image sizes differ"
                    );

                    // Perform exact comparison
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
