#[cfg(feature = "integration-tests")]
mod common;

#[test]
#[cfg(feature = "integration-tests")]
fn atlas_should_not_bleed_neighbouring_cell_colors() {
    use std::sync::Arc;

    use crate::common::test_utils::TestUIRender;
    use flow_ngin::{
        context::InitContext,
        flow::ImageTestResult,
        ui::{
            container::Container,
            image::{Atlas, Icon},
        },
    };

    golden_image_test!(async move |ctx: InitContext| {
        let atlas = Arc::new(
            Atlas::new(&ctx.device, &ctx.queue, "bleeding_atlas.png", 2, 1).await,
        );

        TestUIRender::with_validator(
            move |ctx| {
                Container::<FrameCounter, ()>::new()
                    .width(ctx.config.width)
                    .height(ctx.config.height)
                    .with_child(
                        Icon::new(ctx, &atlas, 0)
                            .width(64)
                            .height(64),
                    )
            },
            &|_ctx, _state, image| {
                const RED_THRESHOLD: u8 = 5;
                for (x, y, pixel) in image.enumerate_pixels() {
                    let [r, _g, b, a] = pixel.0;

                    if a > 0 && b > 128 {
                        assert!(
                            r <= RED_THRESHOLD,
                            "Atlas bleeding detected at ({x}, {y}): red channel = {r} (threshold {RED_THRESHOLD})"
                        );
                    }
                }
                Ok(ImageTestResult::Passed)
            },
        )
    });
}
