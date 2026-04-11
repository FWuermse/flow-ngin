#[cfg(feature = "integration-tests")]
mod common;

/// Asserts that enabling MSAA 4× via `Out::Configure` produces a visibly
/// different render compared to the default (no anti-aliasing).
///
/// Frame 1: render without AA → capture baseline
/// on_update emits Out::Configure to enable MSAA4x
/// Frame 2: render with AA → compare against baseline → assert pixels differ
#[test]
#[cfg(feature = "integration-tests")]
fn anti_aliased_render_should_differ_from_non_aa() {
    use std::cell::RefCell;

    use cgmath::One;
    use flow_ngin::{
        context::{AntiAliasing, Context, GPUResource, InitContext},
        data_structures::block::BuildingBlocks,
        flow::{GraphicsFlow, ImageTestResult, Out},
        render::Render,
    };
    use wgpu::Color;

    use crate::common::test_utils::{to_rgba, FrameCounter};

    struct AAComparisonFlow {
        model: BuildingBlocks,
        baseline: RefCell<Option<image::RgbaImage>>,
    }

    impl GraphicsFlow<FrameCounter, ()> for AAComparisonFlow {
        fn on_init(
            &mut self,
            ctx: &mut Context,
            _state: &mut FrameCounter,
        ) -> Out<FrameCounter, ()> {
            ctx.clear_colour = Color::WHITE;
            ctx.camera.camera.position = [0.0, 40.0, 30.0].into();
            Out::Empty
        }

        fn on_render<'pass>(&self) -> Render<'_, 'pass> {
            self.model.get_render()
        }

        fn on_update(
            &mut self,
            ctx: &Context,
            state: &mut FrameCounter,
            _dt: std::time::Duration,
        ) -> Out<FrameCounter, ()> {
            state.progress();
            self.model.write_to_buffer(&ctx.queue, &ctx.device);

            if state.frame() == 2 {
                // After capturing the non-AA baseline, switch to MSAA 4×.
                Out::Configure(Box::new(|ctx: &mut Context| {
                    ctx.configure_anti_aliasing(AntiAliasing::MSAA4x);
                }))
            } else {
                Out::Empty
            }
        }

        fn render_to_texture(
            &self,
            ctx: &Context,
            s: &mut FrameCounter,
            texture: &mut image::ImageBuffer<image::Rgba<u8>, wgpu::BufferView>,
        ) -> Result<ImageTestResult, anyhow::Error> {
            if s.frame() == 0 {
                return Ok(ImageTestResult::Waiting);
            }

            let actual = to_rgba(ctx, texture);

            if s.frame() == 1 {
                // Store the non-AA baseline for later comparison.
                *self.baseline.borrow_mut() = Some(actual);
                return Ok(ImageTestResult::Waiting);
            }

            // frame >= 2: rendered with MSAA and compare against baseline.
            let baseline = self.baseline.borrow();
            let baseline = baseline.as_ref().expect("baseline should be captured by now");

            // Save both images for visual comparison.
            baseline
                .save("tests/fixtures/aa_off.png")
                .expect("failed to save non-AA image");
            actual
                .save("tests/fixtures/aa_on.png")
                .expect("failed to save AA image");
            eprintln!("Saved tests/fixtures/aa_off.png (no AA) and tests/fixtures/aa_on.png (MSAA 4×)");

            let diff_count = actual
                .enumerate_pixels()
                .filter(|(x, y, px)| *px != baseline.get_pixel(*x, *y))
                .count();

            assert!(
                diff_count > 0,
                "Expected MSAA render to differ from non-AA render, \
                 but images are identical ({} pixels checked).",
                actual.width() * actual.height(),
            );
            eprintln!(
                "Anti-aliasing test passed: {diff_count} pixels differ between AA and non-AA renders."
            );
            Ok(ImageTestResult::Passed)
        }
    }

    // Build the flow outside the macro since we need a custom GraphicsFlow impl.
    use flow_ngin::flow::FlowConstructor;
    let constructor: FlowConstructor<FrameCounter, ()> = Box::new(|ctx: InitContext| {
        Box::pin(async move {
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
            Box::new(AAComparisonFlow {
                model,
                baseline: RefCell::new(None),
            }) as Box<dyn GraphicsFlow<_, _>>
        })
    });

    flow_ngin::flow::run(vec![constructor]).expect("Integration test failed");
}
