use flow_ngin::{
    DeviceEvent, One, RenderPass, Vector3, WindowEvent, context::Context, data_structures::{
        block::BuildingBlocks,
        model::DrawModel,
        scene_graph::{ContainerNode, SceneNode},
    }, flow::{AsyncGraphicsFlowConstructor, GraphicsFlow}
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
    async fn new(ctx: &Context) -> Astroids {
        let astroids = BuildingBlocks::new(
            ctx,
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
    fn on_init(&mut self, ctx: &Context, _: &mut State) {
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
    }

    fn on_click(&mut self, _: &Context, state: &mut State, id: u32) {
        state.selected_astroid = match state.selected_astroid {
            Some(_) => None,
            None => Some(id),
        }
    }

    fn on_update(&mut self, ctx: &Context, state: &mut State, _: std::time::Duration) -> Vec<u32> {
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
        Vec::new()
    }

    fn on_tick(&mut self, _: &Context, _: &mut State) {}

    fn handle_device_events(&mut self, ctx: &Context, state: &mut State, event: &DeviceEvent) {}

    fn handle_window_events(&mut self, ctx: &Context, state: &mut State, event: &WindowEvent) {}

    fn handle_custom_events(
        &mut self,
        ctx: &Context,
        state: &mut State,
        event: Event,
    ) -> Option<Event> {
        Some(event)
    }

    fn on_render<'a>(
        &mut self,
        ctx: &'a Context,
        state: &mut State,
        render_pass: &mut RenderPass<'a>,
    ) {
        render_pass.set_pipeline(&self.astroids.pipeline);
        render_pass.set_vertex_buffer(1, self.astroids.instance_buffer.slice(..));
        render_pass.draw_model_instanced(
            &self.astroids.obj_model,
            0..self.astroids.instances.len() as u32,
            &ctx.camera.bind_group,
            &ctx.light.bind_group,
        );
    }
}

struct Spaceship {
    ship: Box<dyn SceneNode>,
}
impl Spaceship {
    async fn new(ctx: &Context) -> Spaceship {
        Spaceship {
            ship: Box::new(ContainerNode::new(1, Vec::new())),
        }
    }
}
impl GraphicsFlow<State, Event> for Spaceship {
    fn on_init(&mut self, _: &Context, _: &mut State) {}

    fn on_click(&mut self, _: &Context, _: &mut State, _: u32) {}

    fn on_update(&mut self, _: &Context, _: &mut State, dt: std::time::Duration) -> Vec<u32> {
        Vec::new()
    }

    fn on_tick(&mut self, _: &Context, _: &mut State) {}

    fn handle_device_events(&mut self, _: &Context, _: &mut State, _: &DeviceEvent) {}

    fn handle_window_events(&mut self, _: &Context, _: &mut State, _: &WindowEvent) {}

    fn handle_custom_events(&mut self, _: &Context, _: &mut State, event: Event) -> Option<Event> {
        Some(event)
    }

    fn on_render<'a>(
        &mut self,
        ctx: &'a Context,
        state: &mut State,
        render_pass: &mut RenderPass<'a>,
    ) {
    }
}

fn main() {
    let astroids: AsyncGraphicsFlowConstructor<State, Event> = Box::new(|ctx| {
        Box::pin(async move {
            Box::new(Astroids::new(&ctx.borrow()).await) as Box<dyn GraphicsFlow<_, _>>
        })
    });
    let spaceship: AsyncGraphicsFlowConstructor<State, Event> = Box::new(|ctx| {
        Box::pin(async move {
            Box::new(Spaceship::new(&ctx.borrow()).await) as Box<dyn GraphicsFlow<_, _>>
        })
    });

    let _ = flow_ngin::flow::run(vec![astroids, spaceship]);
}
