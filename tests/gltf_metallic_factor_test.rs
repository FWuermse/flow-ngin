#[cfg(feature = "integration-tests")]
use crate::common::test_utils::TestRender;

#[cfg(feature = "integration-tests")]
mod common;

#[test]
#[cfg(feature = "integration-tests")]
fn should_render_metallic_factor() {
    use flow_ngin::{
        camera::Camera,
        context::{Context, InitContext},
        resources::load_model_gltf,
    };
    use wgpu::Color;
    golden_image_test!(async move |ctx: InitContext| {
        let model = load_model_gltf(1, "metal.gltf", &ctx.device, &ctx.queue).await.unwrap();
        TestRender::new(
            model,
            &|ctx: &mut Context| {
                ctx.clear_colour = Color { r: 0.04, g: 0.04, b: 0.04, a: 1.0 };
                ctx.camera.camera = Camera::new(
                    [0.0, 2.5, 1.5],
                    cgmath::Deg(-90.0),
                    cgmath::Deg(-58.0),
                );
                ctx.light.uniform.position = [0.0, 2.5, -1.5];
                ctx.light.uniform.color = [6.0, 6.0, 6.0];
                ctx.light.uniform.radius = 0.6;
                ctx.queue.write_buffer(
                    &ctx.light.buffer,
                    0,
                    bytemuck::cast_slice(&[ctx.light.uniform]),
                );
            },
            "tests/fixtures/metal.png",
        )
    });
}
