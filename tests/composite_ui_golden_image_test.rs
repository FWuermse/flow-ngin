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
            HAlign, VAlign,
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
                let card = Card::<FrameCounter, ()>::new()
                    .width(200)
                    .height(280)
                    .with_background_texture(card_bg)
                    .with_icon(Icon::new(ctx, &atlas, 0).width(64).height(64))
                    .with_label(TextLabel::new("Hero").font_size(22.0))
                    .with_label(TextLabel::new("Strength: 10"))
                    .with_label(TextLabel::new("Health: 100"));

                let attack_btn = Button::<FrameCounter, ()>::new()
                    .width(160)
                    .height(50)
                    .with_text(TextLabel::new("Attack").font_size(18.0).color([255, 255, 255]))
                    .fill(Icon::from_color(ctx, [180, 60, 60, 255]))
                    .halign(HAlign::Center);

                let defend_btn = Button::<FrameCounter, ()>::new()
                    .width(160)
                    .height(50)
                    .with_text(TextLabel::new("Defend").font_size(18.0).color([255, 255, 255]))
                    .fill(Icon::from_color(ctx, [60, 60, 180, 255]))
                    .halign(HAlign::Right);

                let info = TextLabel::new("flow-NGIN")
                    .position(240.0, 90.0)
                    .font_size(20.0)
                    .color([220, 220, 220]);

                Container::<FrameCounter, ()>::new()
                    .width(640)
                    .height(480)
                    .with_background_color([30, 30, 40, 255])
                    .with_child(card)
                    .with_child(attack_btn)
                    .with_child(defend_btn)
                    .with_child(info)
                    .halign(HAlign::Right)
                    .valign(VAlign::Bottom)
            },
            "tests/fixtures/composite_ui_golden.png",
        )
    });
}
