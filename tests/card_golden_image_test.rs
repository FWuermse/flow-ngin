#[cfg(feature = "integration-tests")]
mod common;

#[test]
#[cfg(feature = "integration-tests")]
fn should_match_card_render() {
    use std::sync::Arc;

    use crate::common::test_utils::{FrameCounter, TestUIRender};
    use flow_ngin::{
        context::InitContext,
        flow::ImageTestResult,
        ui::{
            background::BackgroundTexture,
            card::Card,
            image::{Atlas, Icon},
            text_label::TextLabel,
        },
    };
    use image::open;

    golden_image_test!(async move |ctx: InitContext| {
        let atlas = Arc::new(
            Atlas::new(&ctx.device, &ctx.queue, "card_atlas.png", 1, 1).await,
        );
        let bg = Arc::new(
            BackgroundTexture::new(&ctx.device, &ctx.queue, "card_bg.png").await,
        );

        TestUIRender::new(
            move |ctx| {
                Card::<FrameCounter, ()>::new(50, 50, 200, 300)
                    .with_background_texture(bg)
                    .with_icon(Icon::new(ctx, atlas, 0, 0, 64, 64))
                    .with_label(TextLabel::new("Hero").font_size(22.0))
                    .with_label(TextLabel::new("Strength: 10"))
                    .with_label(TextLabel::new("Health: 100"))
            },
            &|_ctx, state: &mut FrameCounter, actual| {
                if state.frame() == 0 {
                    return Ok(ImageTestResult::Waiting);
                }
                let expected = open("tests/fixtures/card_golden.png")
                    .expect("golden image not found — place it at tests/fixtures/card_golden.png")
                    .to_rgba8();
                assert_eq!(actual.dimensions(), expected.dimensions(), "image sizes differ");
                for (x, y, pixel) in actual.enumerate_pixels() {
                    assert_eq!(
                        pixel,
                        expected.get_pixel(x, y),
                        "pixel mismatch at ({x}, {y})",
                    );
                }
                Ok(ImageTestResult::Passed)
            },
        )
    });
}
