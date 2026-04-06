#[cfg(feature = "integration-tests")]
use crate::common::test_utils::TestRender;

#[cfg(feature = "integration-tests")]
mod common;

#[cfg(feature = "integration-tests")]
struct TwoModels(
    flow_ngin::data_structures::block::BuildingBlocks,
    flow_ngin::data_structures::block::BuildingBlocks,
);

#[cfg(feature = "integration-tests")]
impl<'a, 'pass> flow_ngin::context::GPUResource<'a, 'pass> for TwoModels {
    fn write_to_buffer(&mut self, queue: &wgpu::Queue, device: &wgpu::Device) {
        self.0.write_to_buffer(queue, device);
        self.1.write_to_buffer(queue, device);
    }

    fn get_render(&'a self) -> flow_ngin::render::Render<'a, 'pass> {
        flow_ngin::render::Render::Composed(vec![
            self.0.get_render(),
            self.1.get_render(),
        ])
    }
}

/// Regression test: two OBJ models with different normal maps (standard Z vs inverted Z)
/// must be lit from the same direction when sharing the same light source.
#[test]
#[cfg(feature = "integration-tests")]
fn obj_normal_map_lighting_direction_must_be_consistent() {
    use cgmath::Rotation3;
    use flow_ngin::{
        context::{Context, InitContext},
        data_structures::block::BuildingBlocks,
    };
    use wgpu::Color;
    golden_image_test!(async move |ctx: InitContext| {
        let rotation = flow_ngin::Quaternion::from_angle_y(cgmath::Deg(45.0))
            * flow_ngin::Quaternion::from_angle_x(cgmath::Deg(15.0));
        let cube = BuildingBlocks::new(
            0, &ctx.queue, &ctx.device,
            [-1.5, 0.0, 0.0].into(), rotation, 1, "cube.obj",
        ).await;
        let slab = BuildingBlocks::new(
            1, &ctx.queue, &ctx.device,
            [1.5, 0.0, 0.0].into(), rotation, 1, "half_slab.obj",
        ).await;
        TestRender::new(
            TwoModels(cube, slab),
            &|ctx: &mut Context| {
                ctx.clear_colour = Color { r: 0.1, g: 0.1, b: 0.1, a: 1.0 };
                ctx.camera.camera.position = [0.0, 5.0, 2.0].into();
                ctx.light.uniform.position = [-3.0, 2.0, 1.0];
                ctx.queue.write_buffer(
                    &ctx.light.buffer,
                    0,
                    bytemuck::cast_slice(&[ctx.light.uniform]),
                );
            },
            "tests/fixtures/obj_normal_map_lighting_regression.png",
        )
    });
}
