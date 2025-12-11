use flow_ngin::{
    Color, Deg, DeviceEvent, One, Quaternion, Rotation3, Vector3, WindowEvent,
    context::{BufferWriter, Context, InitContext},
    data_structures::block::BuildingBlocks,
    flow::{FlowConsturctor, GraphicsFlow, Out},
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
            .instances
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
        self.astroids.write_to_buffer(ctx);
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
                .instances
                .iter_mut()
                .enumerate()
                .for_each(|(i, astroid)| {
                    astroid.rotation = match i % 3 {
                     0 => astroid.rotation * Quaternion::from_angle_x(Deg(1.0)),
                     1 => astroid.rotation * Quaternion::from_angle_y(Deg(1.0)),
                     _ => astroid.rotation * Quaternion::from_angle_z(Deg(1.0)),
                    }
                });
            self.astroids.write_to_buffer(ctx);
        }
        Out::Empty
    }

    fn on_tick(&mut self, _: &Context, _: &mut State) -> Out<State, Event> {
        Out::Empty
    }

    fn on_device_events(
        &mut self,
        _: &Context,
        _: &mut State,
        _: &DeviceEvent,
    ) -> Out<State, Event> {
        Out::Empty
    }

    fn on_window_events(
        &mut self,
        _: &Context,
        _: &mut State,
        _: &WindowEvent,
    ) -> Out<State, Event> {
        Out::Empty
    }

    fn on_custom_events(
        &mut self,
        _: &Context,
        _: &mut State,
        event: Event,
    ) -> Option<Event> {
        Some(event)
    }

    fn on_render<'pass>(&self) -> flow_ngin::render::Render<'_, 'pass> {
        self.astroids.as_ref().into()
    }
}

fn main() {
    let astroids: FlowConsturctor<State, Event> = Box::new(|ctx| {
        Box::pin(async move { Box::new(Astroids::new(ctx).await) as Box<dyn GraphicsFlow<_, _>> })
    });

    let _ = flow_ngin::flow::run(vec![astroids]);
}
