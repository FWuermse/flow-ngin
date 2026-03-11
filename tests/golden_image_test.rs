#[cfg(feature = "integration-tests")]
use crate::common::test_utils::TestRender;

#[cfg(feature = "integration-tests")]
mod common;

#[test]
#[cfg(feature = "integration-tests")]
fn should_match_rock_collection_render() {
    use cgmath::One;
    use flow_ngin::{
        context::{Context, InitContext},
        data_structures::block::BuildingBlocks,
    };
    use wgpu::Color;
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
            "tests/fixtures/golden_image.png",
        )
    });
}
