#[cfg(feature = "integration-tests")]
mod common;

#[test]
#[cfg(feature = "integration-tests")]
fn should_match_card_render() {
    use std::sync::Arc;

    use crate::common::test_utils::TestUIRender;
    use flow_ngin::{
        context::InitContext,
        ui::{
            background::BackgroundTexture,
            card::Card,
            image::{Atlas, Icon},
            text_label::TextLabel,
        },
    };

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
            "tests/fixtures/card_golden.png",
        )
    });
}
