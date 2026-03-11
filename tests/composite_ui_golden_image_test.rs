#[cfg(feature = "integration-tests")]
mod common;

#[test]
#[cfg(feature = "integration-tests")]
fn should_match_composite_ui_render() {
    use std::sync::Arc;

    use crate::common::test_utils::TestUIRender;
    use flow_ngin::{
        context::InitContext,
        ui::{
            background::BackgroundTexture,
            button::Button,
            card::Card,
            container::Container,
            image::{Atlas, Icon},
            text_label::TextLabel,
        },
    };

    golden_image_test!(async move |ctx: InitContext| {
        let atlas = Arc::new(
            Atlas::new(&ctx.device, &ctx.queue, "card_atlas.png", 16, 16).await,
        );
        let card_bg = Arc::new(
            BackgroundTexture::new(&ctx.device, &ctx.queue, "card_bg.png").await,
        );

        TestUIRender::new(
            move |ctx| {
                let card = Card::<FrameCounter, ()>::new(20, 20, 200, 280)
                    .with_background_texture(card_bg)
                    .with_icon(Icon::new(ctx, atlas, 0, 0, 64, 64))
                    .with_label(TextLabel::new("Hero").font_size(22.0))
                    .with_label(TextLabel::new("Strength: 10"))
                    .with_label(TextLabel::new("Health: 100"));

                let attack_btn = Button::<FrameCounter, ()>::new(1, 240, 20, 160, 50)
                    .with_text(TextLabel::new("Attack").font_size(18.0).color([255, 255, 255]))
                    .normal_color([180, 60, 60, 255])
                    .hover_color([210, 80, 80, 255])
                    .pressed_color([140, 40, 40, 255]);

                let defend_btn = Button::<FrameCounter, ()>::new(2, 420, 20, 160, 50)
                    .with_text(TextLabel::new("Defend").font_size(18.0).color([255, 255, 255]))
                    .normal_color([60, 60, 180, 255])
                    .hover_color([80, 80, 210, 255])
                    .pressed_color([40, 40, 140, 255]);

                let info = TextLabel::new("flow-NGIN")
                    .position(240.0, 90.0)
                    .font_size(20.0)
                    .color([220, 220, 220]);

                Container::<FrameCounter, ()>::new(0, 0, 640, 480)
                    .with_background_color([30, 30, 40, 255])
                    .with_child(card)
                    .with_child(attack_btn)
                    .with_child(defend_btn)
                    .with_child(info)
            },
            "tests/fixtures/composite_ui_golden.png",
        )
    });
}
