use std::sync::Arc;

use flow_ngin::{
    context::{Context, InitContext},
    flow::{FlowConsturctor, GraphicsFlow, Out},
    render::Render,
    ui::{
        Button, Container, HAlign, Layout, Slider, TextInput, VAlign, VStack, Value,
        image::{Atlas, Icon},
        text_label::TextLabel,
    },
};

struct State {
    drawer_open: bool,
    username: Value<String>,
    volume: Value<f32>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            drawer_open: false,
            username: Value::new(String::new()),
            volume: Value::new(0.5),
        }
    }
}

enum Event {
    ToggleDrawer,
}

struct DrawerExample {
    atlas: Arc<Atlas>,
    toggle_btn: Option<Button<State, Event>>,
    drawer: Option<Container<State, Event>>,
    drawer_progress: f32,
    drawer_width: u32,
}

impl DrawerExample {
    async fn new(ctx: InitContext) -> Self {
        let atlas = Arc::new(
            Atlas::new(&ctx.device, &ctx.queue, "card_atlas.png", 16, 16).await,
        );
        Self {
            atlas,
            toggle_btn: None,
            drawer: None,
            drawer_progress: 0.0,
            drawer_width: 300,
        }
    }

    fn resolve_drawer(&mut self, ctx: &Context) {
        if let Some(drawer) = &mut self.drawer {
            let screen_w = ctx.config.width;
            let screen_h = ctx.config.height;

            let open_x = screen_w - self.drawer_width;
            let closed_x = screen_w; // fully off-screen
            let current_x = closed_x - ((closed_x - open_x) as f32 * self.drawer_progress) as u32;

            Layout::resolve(drawer, current_x, 0, self.drawer_width, screen_h, &ctx.queue);
        }
    }
}

impl GraphicsFlow<State, Event> for DrawerExample {
    fn on_init(&mut self, ctx: &mut Context, state: &mut State) -> Out<State, Event> {
        let toggle = Button::new()
            .width(48)
            .height(48)
            .halign(HAlign::Left)
            .valign(VAlign::Top)
            .fill(Icon::from_color(ctx, [60, 60, 60, 220]))
            .hover_fill(Icon::from_color(ctx, [80, 80, 80, 220]))
            .click_fill(Icon::from_color(ctx, [40, 40, 40, 220]))
            .with_icon(Icon::new(ctx, &self.atlas, 28))
            .on_click(|| Event::ToggleDrawer);

        let mut toggle = toggle;
        toggle.on_init(ctx, state);
        self.toggle_btn = Some(toggle);

        let dw = self.drawer_width;

        let mut drawer = Container::<State, Event>::new()
            .width(dw)
            .with_background_color([30, 30, 30, 240])
            .with_child(
                VStack::<State, Event>::new()
                    .padding(12)
                    .spacing(8)
                    .with_child(
                        36,
                        TextLabel::new("Settings")
                            .font_size(24.0)
                            .color([255, 255, 255]),
                    )
                    .with_child(
                        28,
                        TextLabel::new("Username:")
                            .font_size(18.0)
                            .color([200, 200, 200]),
                    )
                    .with_child(
                        32,
                        TextInput::<State, Event>::new()
                            .width(dw - 24)
                            .height(32)
                            .halign(HAlign::Center)
                            .background(Icon::from_color(ctx, [40, 40, 40, 255]))
                            .font_size(18.0)
                            .text_color([255, 255, 255])
                            .bind(&state.username),
                    )
                    .with_child(
                        28,
                        TextLabel::new("Volume:")
                            .font_size(18.0)
                            .color([200, 200, 200]),
                    )
                    .with_child(
                        24,
                        Slider::<State, Event>::new()
                            .width(dw - 24)
                            .height(24)
                            .halign(HAlign::Center)
                            .track(Icon::from_color(ctx, [80, 80, 80, 255]))
                            .handle(Icon::from_color(ctx, [200, 200, 200, 255]))
                            .track_height(4)
                            .handle_width(16)
                            .bind(&state.volume),
                    )
                    .with_child(
                        36,
                        Button::<State, Event>::new()
                            .width(dw - 24)
                            .height(36)
                            .halign(HAlign::Center)
                            .fill(Icon::from_color(ctx, [60, 140, 60, 255]))
                            .hover_fill(Icon::from_color(ctx, [80, 160, 80, 255]))
                            .click_fill(Icon::from_color(ctx, [40, 120, 40, 255]))
                            .with_text(
                                TextLabel::new("Close")
                                    .font_size(18.0)
                                    .color([255, 255, 255]),
                            )
                            .on_click(|| Event::ToggleDrawer),
                    ),
            );

        drawer.on_init(ctx, state);

        self.drawer = Some(drawer);

        self.resolve_drawer(ctx);

        Out::Empty
    }

    fn on_custom_events(
        &mut self,
        _: &Context,
        state: &mut State,
        event: Event,
    ) -> Option<Event> {
        match event {
            Event::ToggleDrawer => {
                state.drawer_open = !state.drawer_open;
                None
            }
        }
    }

    fn on_update(
        &mut self,
        ctx: &Context,
        state: &mut State,
        dt: std::time::Duration,
    ) -> Out<State, Event> {
        let speed = 4.0;
        let target = if state.drawer_open { 1.0 } else { 0.0 };
        let delta = speed * dt.as_secs_f32();

        let old = self.drawer_progress;
        if self.drawer_progress < target {
            self.drawer_progress = (self.drawer_progress + delta).min(target);
        } else if self.drawer_progress > target {
            self.drawer_progress = (self.drawer_progress - delta).max(target);
        }

        if (self.drawer_progress - old).abs() > f32::EPSILON {
            self.resolve_drawer(ctx);
        }

        let mut out = Out::Empty;
        if let Some(btn) = &mut self.toggle_btn {
            out = btn.on_update(ctx, state, dt);
        }
        if let Some(drawer) = &mut self.drawer {
            let drawer_out = drawer.on_update(ctx, state, dt);
            // TODO: ideally merge these outputs; for now drawer takes priority.
            if matches!(out, Out::Empty) {
                out = drawer_out;
            }
        }
        out
    }

    fn on_window_events(
        &mut self,
        ctx: &Context,
        state: &mut State,
        event: &flow_ngin::WindowEvent,
    ) -> Out<State, Event> {
        if let flow_ngin::WindowEvent::Resized(_) = event {
            if let Some(btn) = &mut self.toggle_btn {
                Layout::resolve(btn, 0, 0, ctx.config.width, ctx.config.height, &ctx.queue);
            }
            self.resolve_drawer(ctx);
            return Out::Empty;
        }
        if let Some(drawer) = &mut self.drawer {
            let out = drawer.on_window_events(ctx, state, event);
            if !matches!(out, Out::Empty) {
                return out;
            }
        }
        Out::Empty
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        let mut renders = Vec::new();

        if let Some(btn) = &self.toggle_btn {
            renders.push(btn.on_render());
        }
        if let Some(drawer) = &self.drawer {
            renders.push(drawer.on_render());
        }

        Render::Composed(renders)
    }
}

fn main() {
    let drawer: FlowConsturctor<State, Event> = Box::new(|ctx| {
        Box::pin(async move {
            Box::new(DrawerExample::new(ctx).await) as Box<dyn GraphicsFlow<_, _>>
        })
    });

    let _ = flow_ngin::flow::run(vec![drawer]);
}
