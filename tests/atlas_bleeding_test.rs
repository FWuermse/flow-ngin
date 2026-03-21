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
                        Icon::new(ctx, &atlas, 0),
                    )
            },
            &|_ctx, _state, image| {
                for (x, y, pixel) in image.enumerate_pixels() {
                    let [r, _g, b, a] = pixel.0;

                    if a > 0 && b > 0 {
                        assert!(
                            r == 255,
                            "Atlas bleeding detected at ({x}, {y}): red channel = {r}"
                        );
                    }
                }
                Ok(ImageTestResult::Passed)
            },
        )
    });
}
