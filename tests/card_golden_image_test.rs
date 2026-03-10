#[cfg(feature = "integration-tests")]
mod common;

#[cfg(feature = "integration-tests")]
use crate::common::test_utils::FrameCounter;

#[cfg(feature = "integration-tests")]
mod card_test {
    use std::sync::Arc;

    use flow_ngin::{
        context::Context,
        flow::{GraphicsFlow, ImageTestResult, Out},
        render::Render,
        ui::{
            background::BackgroundTexture,
            card::Card,
            image::Atlas,
            text_label::TextLabel,
        },
    };

    use super::FrameCounter;

    pub(super) struct CardFlow {
        card: Option<Card<FrameCounter, ()>>,
    }

    impl CardFlow {
        pub(super) fn new() -> Self {
            Self { card: None }
        }
    }

    impl GraphicsFlow<FrameCounter, ()> for CardFlow {
        fn on_init(&mut self, ctx: &mut Context, state: &mut FrameCounter) -> Out<FrameCounter, ()> {
            // Atlas and background texture files are provided by the user (see test fixture notes).
            let atlas = Arc::new(futures::executor::block_on(Atlas::new(
                &ctx.device,
                &ctx.queue,
                "card_atlas.png",
                1,
                1,
            )));
            let bg = Arc::new(futures::executor::block_on(BackgroundTexture::new(
                &ctx.device,
                &ctx.queue,
                "card_bg.png",
            )));

            let icon = flow_ngin::ui::image::Icon::new(ctx, atlas, 0, 0, 64, 64);

            let mut card = Card::<FrameCounter, ()>::new(50, 50, 200, 300)
                .with_background_texture(bg)
                .with_icon(icon)
                .with_label(TextLabel::new("Hero").font_size(22.0))
                .with_label(TextLabel::new("Strength: 10"))
                .with_label(TextLabel::new("Health: 100"));

            card.on_init(ctx, state);
            self.card = Some(card);
            Out::Empty
        }

        fn on_render<'pass>(&self) -> Render<'_, 'pass> {
            self.card
                .as_ref()
                .map(|c| c.on_render())
                .unwrap_or(Render::None)
        }

        fn render_to_texture(
            &self,
            _ctx: &Context,
            state: &mut FrameCounter,
            texture: &mut image::ImageBuffer<image::Rgba<u8>, wgpu::BufferView>,
        ) -> Result<ImageTestResult, anyhow::Error> {
            if state.frame() == 0 {
                return Ok(ImageTestResult::Waiting);
            }

            let is_bgra = format!("{:?}", _ctx.config.format).starts_with('B');
            let mut bytes = texture.as_raw().to_vec();
            if is_bgra {
                for pixel in bytes.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                }
            }
            let (width, height) = texture.dimensions();
            let actual = image::RgbaImage::from_raw(width, height, bytes).unwrap();

            let expected = image::open("tests/fixtures/card_golden.png")
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
        }

        fn on_update(
            &mut self,
            _ctx: &Context,
            state: &mut FrameCounter,
            _dt: std::time::Duration,
        ) -> Out<FrameCounter, ()> {
            state.progress();
            Out::Empty
        }

        fn on_tick(&mut self, _: &Context, _: &mut FrameCounter) -> Out<FrameCounter, ()> {
            Out::Empty
        }

        fn on_click(
            &mut self,
            _: &Context,
            _: &mut FrameCounter,
            _: u32,
        ) -> Out<FrameCounter, ()> {
            Out::Empty
        }

        fn on_device_events(
            &mut self,
            _: &Context,
            _: &mut FrameCounter,
            _: &flow_ngin::DeviceEvent,
        ) -> Out<FrameCounter, ()> {
            Out::Empty
        }

        fn on_window_events(
            &mut self,
            _: &Context,
            _: &mut FrameCounter,
            _: &flow_ngin::WindowEvent,
        ) -> Out<FrameCounter, ()> {
            Out::Empty
        }

        fn on_custom_events(
            &mut self,
            _: &Context,
            _: &mut FrameCounter,
            event: (),
        ) -> Option<()> {
            Some(event)
        }
    }
}

#[test]
#[cfg(feature = "integration-tests")]
fn should_match_card_render() {
    use flow_ngin::flow::{FlowConsturctor, GraphicsFlow};

    let constructor: FlowConsturctor<FrameCounter, ()> = Box::new(|_ctx| {
        Box::pin(async move {
            Box::new(card_test::CardFlow::new()) as Box<dyn GraphicsFlow<_, _>>
        })
    });

    flow_ngin::flow::run(vec![constructor]).expect("card golden image test failed");
}
