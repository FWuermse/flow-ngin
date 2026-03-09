use std::sync::Arc;

use flow_ngin::{
    Color, Deg, One, Quaternion, Rotation3, Vector3, context::{Context, GPUResource, InitContext}, data_structures::block::BuildingBlocks, flow::{FlowConsturctor, GraphicsFlow, Out}, render::Render, ui::image::{Atlas, Icon}
};

struct State {
    pub rotating: bool,
}
impl Default for State {
    fn default() -> Self {
        Self { rotating: false }
    }
}

enum Event {}

struct Astroids {
    astroids: BuildingBlocks,
}
impl Astroids {
    async fn new(ctx: InitContext) -> Astroids {
        let astroids = BuildingBlocks::new(
            0,
            &ctx.queue,
            &ctx.device,
            [0.0; 3].into(),
            flow_ngin::Quaternion::one(),
            10000,
            "Rock1.obj",
        )
        .await;
        Self { astroids }
    }
}
impl GraphicsFlow<State, Event> for Astroids {
    fn on_init(&mut self, ctx: &mut Context, _: &mut State) -> Out<State, Event> {
        ctx.clear_colour = Color::TRANSPARENT;
        self.astroids
            .instances_mut_size_unchanged()
            .iter_mut()
            .enumerate()
            .for_each(|(i, instance)| {
                // 20x20x20 cube
                let len = 20;
                let spacing = 5.0;
                let x = i % len;
                let y = (i / len) % len;
                let z = i / (len * len);
                let offset = len as f32 / 2.0;
                instance.position = Vector3::new(
                    (x as f32 - offset) * spacing,
                    (y as f32 - offset) * spacing,
                    (z as f32 - offset) * spacing,
                );
                instance.scale = [0.5; 3].into();
            });
        self.astroids.write_to_buffer(&ctx.queue, &ctx.device);
        Out::Empty
    }

    fn on_click(&mut self, _: &Context, state: &mut State, _: u32) -> Out<State, Event> {
        state.rotating = !state.rotating;
        Out::Empty
    }

    fn on_update(
        &mut self,
        ctx: &Context,
        state: &mut State,
        _: std::time::Duration,
    ) -> Out<State, Event> {
        if state.rotating {
            self.astroids
                .instances_mut_size_unchanged()
                .iter_mut()
                .enumerate()
                .for_each(|(i, astroid)| {
                    astroid.rotation = match i % 3 {
                        0 => astroid.rotation * Quaternion::from_angle_x(Deg(1.0)),
                        1 => astroid.rotation * Quaternion::from_angle_y(Deg(1.0)),
                        _ => astroid.rotation * Quaternion::from_angle_z(Deg(1.0)),
                    }
                });
            self.astroids.write_to_buffer(&ctx.queue, &ctx.device);
        }
        Out::Empty
    }

    fn on_render<'pass>(&self) -> flow_ngin::render::Render<'_, 'pass> {
        self.astroids.as_ref().into()
    }
}

struct GUI {
    atlas: Arc<Atlas>,
    bedrock: Option<Icon>,
}
impl GUI {
    async fn new(ctx: InitContext) -> GUI {
        let atlas = Arc::new(Atlas::new(&ctx.device, &ctx.queue, "minecraft_beta.png", 16, 16).await);
        Self { atlas, bedrock: None }
    }
}
impl<'a> GraphicsFlow<State, Event> for GUI {
    fn on_init(&mut self, ctx: &mut Context, _state: &mut State) -> Out<State, Event> {
        self.bedrock = Some(Icon::new(ctx, Arc::clone(&self.atlas), 100, 17, 1000, 1000));
        Out::Empty
    }

    fn on_render<'pass>(&self) -> flow_ngin::render::Render<'_, 'pass> {
        if let Some(bedrock) = &self.bedrock {
            return <Icon as GraphicsFlow<State, Event>>::on_render(bedrock);
        } else {
            return flow_ngin::render::Render::None
        }
    }
}

fn main() {
    let astroids: FlowConsturctor<State, Event> = Box::new(|ctx| {
        Box::pin(async move { Box::new(Astroids::new(ctx).await) as Box<dyn GraphicsFlow<_, _>> })
    });
    let gui: FlowConsturctor<State, Event> = Box::new(|ctx| {
        Box::pin(async move { Box::new(GUI::new(ctx).await) as Box<dyn GraphicsFlow<_, _>> })
    });

    let _ = flow_ngin::flow::run(vec![astroids, gui]);
}
