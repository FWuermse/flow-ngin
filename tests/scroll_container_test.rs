#[cfg(feature = "integration-tests")]
mod common;

#[test]
#[cfg(feature = "integration-tests")]
fn scroll_container_should_clip_and_hide_scrolled_rows() {
    use crate::common::test_utils::TestUIRender;
    use flow_ngin::{
        context::InitContext,
        flow::ImageTestResult,
        ui::{HAlign, VAlign, image::Icon, scroll::ScrollContainer},
    };

    golden_image_test!(async move |_ctx: InitContext| {
        TestUIRender::with_validator(
            move |ctx| {
                // Viewport 150 px high, three 100 px rows, scrolled down by one row:
                // green fills the top, blue is cut off at the viewport's bottom edge.
                let mut list = ScrollContainer::<FrameCounter, ()>::new()
                    .width(200)
                    .height(150)
                    .halign(HAlign::Left)
                    .valign(VAlign::Center)
                    .with_row(100, Icon::from_color(ctx, [255, 0, 0, 255]))
                    .with_row(100, Icon::from_color(ctx, [0, 255, 0, 255]))
                    .with_row(100, Icon::from_color(ctx, [0, 0, 255, 255]));
                list.scroll_to(1, &ctx.queue);
                list
            },
            &|ctx, _state, image| {
                let top = (ctx.config.height - 150) / 2;
                let at = |y: u32| image.get_pixel(50, y).0;
                let red = |[r, g, b, _]: [u8; 4]| r > 200 && g < 60 && b < 60;
                let green = |[r, g, b, _]: [u8; 4]| g > 200 && r < 60 && b < 60;
                let blue = |[r, g, b, _]: [u8; 4]| b > 200 && r < 60 && g < 60;
                assert!(green(at(top + 50)), "first visible row: {:?}", at(top + 50));
                assert!(blue(at(top + 125)), "cut-off row: {:?}", at(top + 125));
                assert!(
                    !blue(at(top + 175)),
                    "not clipped below viewport: {:?}",
                    at(top + 175)
                );
                assert!(
                    !red(at(top - 20)),
                    "scrolled-out row drawn: {:?}",
                    at(top - 20)
                );
                Ok(ImageTestResult::Passed)
            },
        )
    });
}
