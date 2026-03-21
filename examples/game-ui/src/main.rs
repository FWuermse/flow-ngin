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
    selected_id: u32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            drawer_open: false,
            username: Value::new(String::new()),
            volume: Value::new(0.5),
            selected_id: 1, // mock: set to 0 to hide card
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
    Build,
    DismissCard,
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
        let main_menu = Grid::new(1, 8)
            .with_child(0, 0, toggle)
            .with_child(0, 1, path)
            .with_child(0, 2, path_break);
        let mut actions = Container::new()
            .width(self.drawer_width/2)
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

        let build_menu = Grid::new(2, 8)
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
            _ => None,
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

struct DetailCard {
    atlas: Arc<Atlas>,
    bg: Arc<BackgroundTexture>,
    cards: Vec<(u32, Container<State, Event>)>,
    current_id: u32,
}

impl DetailCard {
    async fn new(ctx: InitContext) -> Self {
        let atlas = Arc::new(Atlas::new(&ctx.device, &ctx.queue, "atlas.png", 8, 8).await);
        let bg =
            Arc::new(BackgroundTexture::new(&ctx.device, &ctx.queue, "container-slim.png").await);
        Self {
            atlas,
            bg,
            cards: Vec::new(),
            current_id: 0,
        }
    }

    fn card_info(id: u32) -> Option<(u8, &'static str, &'static str, &'static str, f32)> {
        match id {
            1 => Some((11, "Mine", "Extracts precious ore from deep underground.", "Mine #1", 0.6)),
            2 => Some((15, "Farm", "Grows crops and raises livestock for food.", "Farm #1", 0.4)),
            _ => None,
        }
    }

    fn build_card(&self, ctx: &Context, id: u32) -> Option<Container<State, Event>> {
        let (icon_slot, title, desc, default_name, default_capacity) = Self::card_info(id)?;

        let icon = Icon::new(ctx, &self.atlas, icon_slot)
            .width(80)
            .height(80)
            .halign(HAlign::Center);

        let title = TextLabel::new(title)
            .font_size(24.0)
            .line_height(32.0)
            .halign(HAlign::Center);

        let desc = TextLabel::new(desc)
            .font_size(16.0)
            .line_height(22.0)
            .color([200, 200, 200]);

        let name_value = Value::new(default_name.to_string());
        let name_input = TextInput::<State, Event>::new()
            .width(248)
            .height(28)
            .font_size(16.0)
            .background(Icon::from_color(ctx, [40, 40, 40, 200]))
            .bind(&name_value);

        let capacity_value = Value::new(default_capacity);
        let capacity_slider = Slider::<State, Event>::new()
            .width(248)
            .height(24)
            .track(Icon::from_color(ctx, [60, 60, 60, 255]))
            .handle(Icon::from_color(ctx, [200, 200, 200, 255]))
            .active_handle(Icon::from_color(ctx, [255, 255, 255, 255]))
            .bind(&capacity_value);

        let btn_build = Button::new()
            .square(60)
            .halign(HAlign::Center)
            .valign(VAlign::Center)
            .fill(Icon::new(ctx, &self.atlas, 16))
            .hover_fill(Icon::new(ctx, &self.atlas, 24))
            .click_fill(Icon::new(ctx, &self.atlas, 32))
            .on_click(|| Event::Build);

        let btn_dismiss = Button::new()
            .square(60)
            .halign(HAlign::Center)
            .valign(VAlign::Center)
            .fill(Icon::new(ctx, &self.atlas, 17))
            .hover_fill(Icon::new(ctx, &self.atlas, 25))
            .click_fill(Icon::new(ctx, &self.atlas, 33))
            .on_click(|| Event::DismissCard);

        let buttons = Grid::new(2, 1)
            .with_child(0, 0, btn_build)
            .with_child(1, 0, btn_dismiss);

        let content = VStack::<State, Event>::new()
            .width(248)
            .halign(HAlign::Center)
            .valign(VAlign::Center)
            .with_child(96, icon)
            .with_child(36, title)
            .with_child(80, desc)
            .with_child(32, name_input)
            .with_child(28, capacity_slider)
            .with_child(60, buttons);

        Some(
            Container::new()
                .width(280)
                .height(480)
                .valign(VAlign::Center)
                .with_background_texture(&self.bg)
                .with_child(content),
        )
    }
}

impl GraphicsFlow<State, Event> for DetailCard {
    fn on_init(&mut self, ctx: &mut Context, state: &mut State) -> Out<State, Event> {
        for id in [1u32, 2] {
            if let Some(mut card) = self.build_card(ctx, id) {
                card.on_init(ctx, state);
                self.cards.push((id, card));
            }
        }
        self.current_id = state.selected_id;
        Out::Empty
    }

    fn on_custom_events(&mut self, _: &Context, state: &mut State, event: Event) -> Option<Event> {
        match event {
            Event::Build => {
                if let Some((_, title, _, _, _)) = Self::card_info(self.current_id) {
                    println!("Building {title}!");
                }
                state.selected_id = 0;
                None
            }
            Event::DismissCard => {
                state.selected_id = 0;
                None
            }
            _ => Some(event),
        }
    }

    fn on_update(
        &mut self,
        ctx: &Context,
        state: &mut State,
        dt: std::time::Duration,
    ) -> Out<State, Event> {
        self.current_id = state.selected_id;
        if let Some((_, card)) = self.cards.iter_mut().find(|(id, _)| *id == self.current_id) {
            return card.on_update(ctx, state, dt);
        }
        Out::Empty
    }

    fn on_window_events(
        &mut self,
        ctx: &Context,
        state: &mut State,
        event: &flow_ngin::WindowEvent,
    ) -> Out<State, Event> {
        for (_, card) in &mut self.cards {
            card.on_window_events(ctx, state, event);
        }
        Out::Empty
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        if let Some((_, card)) = self.cards.iter().find(|(id, _)| *id == self.current_id) {
            return card.on_render();
        }
        Render::None
    }
}

fn main() {
    let card: FlowConsturctor<State, Event> = Box::new(|ctx| {
        Box::pin(
            async move { Box::new(DetailCard::new(ctx).await) as Box<dyn GraphicsFlow<_, _>> },
        )
    });

    let drawer: FlowConsturctor<State, Event> = Box::new(|ctx| {
        Box::pin(
            async move { Box::new(DrawerExample::new(ctx).await) as Box<dyn GraphicsFlow<_, _>> },
        )
    });

    let _ = flow_ngin::flow::run(vec![card, drawer]);
}
