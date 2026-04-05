use std::sync::Arc;

use flow_ngin::{
    Color, Deg, One, Quaternion, Rotation3, Vector3,
    context::{Context, GPUResource, InitContext},
    data_structures::block::BuildingBlocks,
    flow::{FlowConstructor, GraphicsFlow, Out},
    ui::{
        Button, Checkbox, Grid, HAlign, VAlign, Value, image::{Atlas, Icon}
    },
};

/// This is an arbitraty state shared between all rendered objects
struct State {
    pub rotating: bool,
    pub checked: Value<bool>,
}
impl Default for State {
    fn default() -> Self {
        Self { rotating: false, checked: Value::new(false) }
    }
}

/// A collection of events that can be sent between flows
enum Event {
    Spin,
    Checked(bool),
}

/// Note that the Astroids struct only holds information neccessary for rendering
struct Astroids {
    astroids: BuildingBlocks,
    background: Color,
}
/// The constructor is usually async because it loads assets
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
        let background = Color::BLACK;
        Self {
            astroids,
            background,
        }
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

    fn on_custom_events(&mut self, _: &Context, state: &mut State, event: Event) -> Option<Event> {
        match event {
            Event::Spin => {
                state.rotating = !state.rotating;
                None
            }
            Event::Checked(checked) => {
                let background = if checked { Color::WHITE } else { Color::BLACK };
                self.background = background;
                None
            }
        }
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
        if self.background != ctx.clear_colour {
            let bg = self.background;
            return Out::Configure(Box::new(move |ctx: &mut Context| ctx.clear_colour = bg));
        }
        Out::Empty
    }

    fn on_render<'pass>(&self) -> flow_ngin::render::Render<'_, 'pass> {
        self.astroids.as_ref().into()
    }
}

struct GUI {
    atlas: Arc<Atlas>,
    grid: Option<Grid<State, Event>>,
}
impl GUI {
    async fn new(ctx: InitContext) -> GUI {
        let atlas = Arc::new(Atlas::new(&ctx.device, &ctx.queue, "card_atlas.png", 16, 16).await);
        Self { atlas, grid: None }
    }

    fn make_button(
        &self,
        ctx: &Context,
        icon_slot: u8,
        bg_start: u8,
        on_click: impl Fn() -> Event + 'static,
    ) -> Button<State, Event> {
        Button::new()
            .square(80)
            .halign(HAlign::Center)
            .valign(VAlign::Center)
            .with_icon(Icon::new(ctx, &self.atlas, icon_slot))
            .fill(Icon::new(ctx, &self.atlas, bg_start))
            .hover_fill(Icon::new(ctx, &self.atlas, bg_start + 1))
            .click_fill(Icon::new(ctx, &self.atlas, bg_start + 2))
            .on_click(on_click)
    }
}
impl<'a> GraphicsFlow<State, Event> for GUI {
    fn on_init(&mut self, ctx: &mut Context, state: &mut State) -> Out<State, Event> {
        let spin_btn = self.make_button(ctx, 28, 22, || Event::Spin);
        let btn2 = self.make_button(ctx, 29, 22 + 6 * 16, || Event::Spin);
        let btn3 = self.make_button(ctx, 13, 32, || Event::Spin);
        let btn4 = self.make_button(ctx, 12, 32, || Event::Spin);

        let grid = Grid::new(4, 2)
            .height(200)
            .valign(VAlign::Top)
            .with_child(0, 0, spin_btn)
            .with_child(1, 0, btn2)
            .with_child(2, 0, btn3)
            .with_child(
                0,
                1,
                Checkbox::new()
                    .on_change(|pressed| {
                        Out::FutEvent(vec![Box::new(async move { Event::Checked(pressed) })])
                    })
                    .valign(VAlign::Center)
                    .halign(HAlign::Center)
                    .checked(Icon::new(ctx, &self.atlas, 3 + 9 * 16))
                    .unchecked(Icon::new(ctx, &self.atlas, 3 + 8 * 16)).width(80).height(80)
                    .bind(&state.checked),
            )
            .with_child(3, 0, btn4);

        self.grid = Some(grid);
        self.grid.as_mut().unwrap().on_init(ctx, state)
    }

    fn on_update(
        &mut self,
        ctx: &Context,
        state: &mut State,
        dt: std::time::Duration,
    ) -> Out<State, Event> {
        if let Some(grid) = &mut self.grid {
            return grid.on_update(ctx, state, dt);
        }
        Out::Empty
    }

    fn on_window_events(
        &mut self,
        ctx: &Context,
        state: &mut State,
        event: &flow_ngin::WindowEvent,
    ) -> Out<State, Event> {
        if let Some(grid) = &mut self.grid {
            return grid.on_window_events(ctx, state, event);
        }
        Out::Empty
    }

    fn on_render<'pass>(&self) -> flow_ngin::render::Render<'_, 'pass> {
        match &self.grid {
            Some(g) => g.on_render(),
            None => flow_ngin::render::Render::None,
        }
    }
}

fn main() {
    let astroids: FlowConstructor<State, Event> = Box::new(|ctx| {
        Box::pin(async move { Box::new(Astroids::new(ctx).await) as Box<dyn GraphicsFlow<_, _>> })
    });
    let gui: FlowConstructor<State, Event> = Box::new(|ctx| {
        Box::pin(async move { Box::new(GUI::new(ctx).await) as Box<dyn GraphicsFlow<_, _>> })
    });

    let _ = flow_ngin::flow::run(vec![astroids, gui]);
}
