#[cfg(feature = "integration-tests")]
use crate::common::test_utils::TestRender;
use wgpu::Color;

#[cfg(feature = "integration-tests")]
mod common;

#[test]
#[cfg(feature = "integration-tests")]
fn should_have_correct_tangents_on_arms() {
    use flow_ngin::{
        context::{Context, InitContext},
        resources::load_model_gltf,
    };
    golden_image_test!(async move |ctx: InitContext| {
        let model = load_model_gltf(1, "woodcutter_updated.gltf", &ctx.device, &ctx.queue).await.unwrap();
        TestRender::new(
            model,
            &|ctx: &mut Context| {
                ctx.clear_colour = Color::WHITE;
                ctx.camera.camera.position = [0.0, 5.0, 2.0].into();
            },
            "tests/fixtures/woodcutter.png",
        )
    });
}
