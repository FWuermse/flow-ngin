use std::sync::Arc;

use flow_ngin::{
    context::{Context, InitContext},
    flow::{FlowConsturctor, GraphicsFlow, Out},
    render::Render,
    ui::{
        BackgroundTexture, Button, Container, Grid, HAlign, Layout, Slider, TextInput, VAlign,
        VStack, Value,
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
    Mine,
    Wood,
    Farm,
    Clay,
    SawMill,
    Barrack,
    Path,
    PathBreak,
}

struct DrawerExample {
    atlas: Arc<Atlas>,
    bg: Arc<BackgroundTexture>,
    actions: Option<Container<State, Event>>,
    drawer: Option<Container<State, Event>>,
    drawer_progress: f32,
    drawer_width: u32,
}

impl DrawerExample {
    async fn new(ctx: InitContext) -> Self {
        let atlas = Arc::new(Atlas::new(&ctx.device, &ctx.queue, "atlas.png", 8, 8).await);
        let bg =
            Arc::new(BackgroundTexture::new(&ctx.device, &ctx.queue, "container-slim.png").await);
        Self {
            atlas,
            bg,
            actions: None,
            drawer: None,
            drawer_progress: 0.0,
            drawer_width: 240,
        }
    }

    fn resolve_drawer(&mut self, ctx: &Context) {
        if let Some(drawer) = &mut self.drawer {
            let screen_w = ctx.config.width;
            let screen_h = ctx.config.height;

            let open_x = screen_w - self.drawer_width;
            let closed_x = screen_w; // fully off-screen
            let current_x = closed_x - ((closed_x - open_x) as f32 * self.drawer_progress) as u32;

            Layout::resolve(
                drawer,
                current_x,
                0,
                self.drawer_width,
                screen_h,
                &ctx.queue,
            );
        }
    }
    fn make_button(
        &self,
        ctx: &Context,
        icon_slot: u8,
        on_click: impl Fn() -> Event + 'static,
    ) -> Button<State, Event> {
        Button::new()
            .square(100)
            .halign(HAlign::Center)
            .valign(VAlign::Center)
            .with_icon(Icon::new(ctx, &self.atlas, icon_slot))
            .fill(Icon::new(ctx, &self.atlas, 0))
            .hover_fill(Icon::new(ctx, &self.atlas, 1))
            .click_fill(Icon::new(ctx, &self.atlas, 2))
            .on_click(on_click)
    }
    fn make_arrow(
        &self,
        ctx: &Context,
        on_click: impl Fn() -> Event + 'static,
    ) -> Button<State, Event> {
        Button::new()
            .square(100)
            .halign(HAlign::Center)
            .valign(VAlign::Center)
            .fill(Icon::new(ctx, &self.atlas, 8))
            .hover_fill(Icon::new(ctx, &self.atlas, 9))
            .click_fill(Icon::new(ctx, &self.atlas, 9))
            .on_click(on_click)
    }
}

impl GraphicsFlow<State, Event> for DrawerExample {
    fn on_init(&mut self, ctx: &mut Context, state: &mut State) -> Out<State, Event> {
        let toggle = self.make_button(ctx, 4, || Event::ToggleDrawer);
        let path = self.make_button(ctx, 5, || Event::Path);
        let path_break = self.make_button(ctx, 6, || Event::PathBreak);
        let main_menu = Grid::new(1, 6)
            .with_child(0, 0, toggle)
            .with_child(0, 1, path)
            .with_child(0, 2, path_break);
        let mut actions = Container::new()
            .width(self.drawer_width)
            .with_background_texture(&self.bg)
            .halign(HAlign::Right)
            .with_child(main_menu);
        actions.on_init(ctx, state);
        self.actions = Some(actions);

        let mine = self.make_button(ctx, 11, || Event::Mine);
        let wood = self.make_button(ctx, 14, || Event::Wood);
        let farm = self.make_button(ctx, 15, || Event::Farm);
        let clay = self.make_button(ctx, 13, || Event::Clay);
        let sawmill = self.make_button(ctx, 12, || Event::SawMill);
        let barrack = self.make_button(ctx, 10, || Event::Barrack);
        let arrow = self.make_arrow(ctx, || Event::ToggleDrawer);

        let build_menu = Grid::new(2, 6)
            .with_child(0, 0, mine)
            .with_child(0, 1, wood)
            .with_child(0, 2, farm)
            .with_child(0, 3, clay)
            .with_child(0, 4, sawmill)
            .with_child(0, 5, barrack)
            .with_child(1, 0, arrow);
        let mut build_menu = Container::new()
            .width(240)
            .with_child(build_menu)
            .with_background_texture(&self.bg);
        build_menu.on_init(ctx, state);

        self.drawer = Some(build_menu);

        self.resolve_drawer(ctx);

        Out::Empty
    }

    fn on_custom_events(&mut self, _: &Context, state: &mut State, event: Event) -> Option<Event> {
        match event {
            Event::ToggleDrawer => {
                state.drawer_open = !state.drawer_open;
                None
            }
            Event::Mine => {
                println!("Built a mine!");
                None
            }
            Event::Wood => {
                println!("Built a wood!");
                None
            }
            Event::Farm => {
                println!("Built a farm!");
                None
            }
            Event::Clay => {
                println!("Built a clay!");
                None
            }
            Event::SawMill => {
                println!("Built a saw!");
                None
            }
            Event::Barrack => {
                println!("Built a barrack!");
                None
            }
            Event::Path => {
                println!("Built a path!");
                None
            }
            Event::PathBreak => {
                println!("Break a path!");
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
        if let Some(btn) = &mut self.actions {
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
            if let Some(btn) = &mut self.actions {
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

        if let Some(btn) = &self.actions {
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
        Box::pin(
            async move { Box::new(DrawerExample::new(ctx).await) as Box<dyn GraphicsFlow<_, _>> },
        )
    });

    let _ = flow_ngin::flow::run(vec![drawer]);
}
