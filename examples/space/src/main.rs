use flow_ngin::{
    DeviceEvent, One, Vector3, WindowEvent, context::{Context, InitContext}, data_structures::{
        block::BuildingBlocks,
        scene_graph::{ContainerNode, SceneNode},
    }, flow::{FlowConsturctor, GraphicsFlow, Out}
};

enum MouseButtonState {
    Right,
    Left,
    None,
}

struct MouseState {
    coords: flow_ngin::PhysicalPosition<f64>,
    pressed: MouseButtonState,
    selection: Option<u32>,
}

struct State {
    mouse: MouseState,
    selected_astroid: Option<u32>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            mouse: MouseState {
                coords: (0.0, 0.0).into(),
                pressed: MouseButtonState::None,
                selection: None,
            },
            selected_astroid: Default::default(),
        }
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

    fn on_click(&mut self, _: &Context, state: &mut State, id: u32) -> Out<State, Event> {
        state.selected_astroid = match state.selected_astroid {
            Some(_) => None,
            None => Some(id),
        };
        Out::Empty
    }

    fn on_update(&mut self, ctx: &Context, state: &mut State, _: std::time::Duration) -> Out<State, Event> {
        let mouse_ray = ctx.camera.camera.cast_ray_from_mouse(
            state.mouse.coords,
            ctx.config.width as f32,
            ctx.config.height as f32,
            &ctx.projection,
        );
        mouse_ray.intersect_with_floor().and_then(|point| {
            state
                .selected_astroid
                .and_then(|id| self.astroids.instances.get_mut(id as usize))
                .map(|astroid| {
                    astroid.position.x = point.x;
                    astroid.position.z = point.y;
                })
        });
        // TODO: rotate all
        self.astroids.write_to_buffer(ctx);
        Out::Empty
    }

    fn on_tick(&mut self, _: &Context, _: &mut State) -> Out<State, Event> {
        Out::Empty
    }

    fn on_device_events(&mut self, ctx: &Context, state: &mut State, event: &DeviceEvent) -> Out<State, Event> {
        Out::Empty
    }

    fn on_window_events(&mut self, ctx: &Context, state: &mut State, event: &WindowEvent) -> Out<State, Event> {
        Out::Empty
    }

    fn on_custom_events(
        &mut self,
        ctx: &Context,
        state: &mut State,
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
        Box::pin(async move {
            Box::new(Astroids::new(ctx).await) as Box<dyn GraphicsFlow<_, _>>
        })
    });

    let _ = flow_ngin::flow::run(vec![astroids]);
}
