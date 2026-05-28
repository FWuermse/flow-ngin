use std::sync::Arc;

use flow_ngin::{
    WindowEvent,
    context::{Context, InitContext},
    flow::{GraphicsFlow, Out},
    pick::PickId,
    render::Render,
    ui::{
        Button, Container, Grid, HAlign, Layout, Slider, VAlign, VStack, Value,
        image::{Atlas, Icon},
        text_label::TextLabel,
    },
};

use crate::{Event, ObjectShape, State, Strategy};

pub struct GuiFlow {
    #[allow(dead_code)]
    atlas: Arc<Atlas>,
    panel: Option<Container<State, Event>>,
    dim_value: Value<f32>,
    shape_value: Value<f32>,
    fps_label: Option<TextLabel>,
    fps_smoothed: f32,
    fps_timer: f32,
}

impl GuiFlow {
    pub async fn new(ctx: InitContext) -> Self {
        let atlas = Arc::new(Atlas::new(&ctx.device, &ctx.queue, "card_atlas.png", 16, 16).await);
        Self {
            atlas,
            panel: None,
            dim_value: Value::new(0.5),
            shape_value: Value::new(0.0),
            fps_label: None,
            fps_smoothed: 60.0,
            fps_timer: 0.0,
        }
    }
}

impl GraphicsFlow<State, Event> for GuiFlow {
    fn on_init(&mut self, ctx: &mut Context, state: &mut State) -> Out<State, Event> {
        let btn_h = 52u32;
        let panel_w = 440u32;
        let panel_h = 32u32 + 40 + 32 + 40 + 32 + btn_h + 28 + 28 + btn_h;

        let strategy_buttons = Grid::<State, Event>::new(4, 1)
            .width(panel_w)
            .height(btn_h)
            .with_child(
                0, 0,
                Button::<State, Event>::new()
                    .fill(Icon::from_color(ctx, [50, 120, 50, 220]))
                    .hover_fill(Icon::from_color(ctx, [70, 150, 70, 220]))
                    .click_fill(Icon::from_color(ctx, [30, 100, 30, 220]))
                    .with_text(TextLabel::new("Grid").font_size(22.0).color([255, 255, 255]))
                    .on_click(|_, _| Event::StrategyChanged(Strategy::Grid)),
            )
            .with_child(
                1, 0,
                Button::<State, Event>::new()
                    .fill(Icon::from_color(ctx, [50, 80, 160, 220]))
                    .hover_fill(Icon::from_color(ctx, [70, 100, 190, 220]))
                    .click_fill(Icon::from_color(ctx, [30, 60, 140, 220]))
                    .with_text(TextLabel::new("Sparse").font_size(22.0).color([255, 255, 255]))
                    .on_click(|_, _| Event::StrategyChanged(Strategy::SparseGrid)),
            )
            .with_child(
                2, 0,
                Button::<State, Event>::new()
                    .fill(Icon::from_color(ctx, [140, 60, 60, 220]))
                    .hover_fill(Icon::from_color(ctx, [170, 80, 80, 220]))
                    .click_fill(Icon::from_color(ctx, [120, 40, 40, 220]))
                    .with_text(TextLabel::new("Brute").font_size(22.0).color([255, 255, 255]))
                    .on_click(|_, _| Event::StrategyChanged(Strategy::BruteForce)),
            )
            .with_child(
                3, 0,
                Button::<State, Event>::new()
                    .fill(Icon::from_color(ctx, [120, 60, 160, 220]))
                    .hover_fill(Icon::from_color(ctx, [150, 80, 190, 220]))
                    .click_fill(Icon::from_color(ctx, [100, 40, 140, 220]))
                    .with_text(TextLabel::new("Tree").font_size(22.0).color([255, 255, 255]))
                    .on_click(|_, _| Event::StrategyChanged(Strategy::SpatialTree)),
            );

        let dim_slider = Slider::<State, Event>::new()
            .width(panel_w)
            .height(40)
            .halign(HAlign::Left)
            .track(Icon::from_color(ctx, [80, 80, 80, 200]))
            .handle(Icon::from_color(ctx, [180, 220, 255, 255]))
            .active_handle(Icon::from_color(ctx, [220, 240, 255, 255]))
            .track_height(6)
            .bind(&self.dim_value)
            .on_change(|v| {
                let dims = if v < 0.33 { 1u8 } else if v < 0.67 { 2u8 } else { 3u8 };
                Out::FutEvent(vec![Box::new(async move { Event::DetectionDimsChanged(dims) })])
            });

        let shape_slider = Slider::<State, Event>::new()
            .width(panel_w)
            .height(40)
            .halign(HAlign::Left)
            .track(Icon::from_color(ctx, [80, 80, 80, 200]))
            .handle(Icon::from_color(ctx, [255, 220, 100, 255]))
            .active_handle(Icon::from_color(ctx, [255, 240, 150, 255]))
            .track_height(6)
            .bind(&self.shape_value)
            .on_change(|v| {
                let shape = if v < 0.5 { ObjectShape::Cube3D } else { ObjectShape::Plane2D };
                Out::FutEvent(vec![Box::new(async move { Event::ObjectShapeChanged(shape) })])
            });

        let panel = VStack::<State, Event>::new()
            .width(panel_w)
            .halign(HAlign::Left)
            .valign(VAlign::Bottom)
            .with_child(
                32,
                TextLabel::new("Detection Space")
                    .font_size(22.0)
                    .color([200, 200, 200]),
            )
            .with_child(40, dim_slider)
            .with_child(
                32,
                TextLabel::new("Object Shape")
                    .font_size(22.0)
                    .color([200, 200, 200]),
            )
            .with_child(40, shape_slider)
            .with_child(
                32,
                TextLabel::new("Strategy")
                    .font_size(22.0)
                    .color([200, 200, 200]),
            )
            .with_child(btn_h, strategy_buttons)
            .with_child(
                28,
                TextLabel::new("Move: mouse  Y: scroll  Place: LClick")
                    .font_size(18.0)
                    .color([150, 150, 150]),
            )
            .with_child(
                28,
                Grid::<State, Event>::new(3, 1)
                    .width(panel_w)
                    .height(28)
                    .with_child(0, 0, TextLabel::new("White = idle").font_size(18.0).color([255, 255, 255]))
                    .with_child(1, 0, TextLabel::new("Yellow = broad").font_size(18.0).color([255, 220, 50]))
                    .with_child(2, 0, TextLabel::new("Red = overlap").font_size(18.0).color([255, 80, 80])),
            )
            .with_child(
                btn_h,
                Button::<State, Event>::new()
                    .fill(Icon::from_color(ctx, [180, 100, 20, 220]))
                    .hover_fill(Icon::from_color(ctx, [210, 130, 40, 220]))
                    .click_fill(Icon::from_color(ctx, [150, 80, 10, 220]))
                    .with_text(TextLabel::new("Scatter 200").font_size(22.0).color([255, 255, 255]))
                    .on_click(|_, _| Event::PlaceRandom(200)),
            );

        let mut container = Container::<State, Event>::new()
            .width(panel_w)
            .height(panel_h)
            .halign(HAlign::Left)
            .valign(VAlign::Bottom)
            .with_background_color([20, 20, 25, 200])
            .clickable(PickId(u32::MAX - 2))
            .with_child(panel);
        container.on_init(ctx, state);
        self.panel = Some(container);

        let mut fps = TextLabel::new("FPS: --")
            .font_size(22.0)
            .color([180, 220, 140])
            .halign(HAlign::Right)
            .valign(VAlign::Top)
            .width(160)
            .height(32);
        fps.init(ctx);
        Layout::resolve(&mut fps, 0, 0, ctx.config.width, ctx.config.height, &ctx.queue);
        self.fps_label = Some(fps);

        Out::Empty
    }

    fn on_update(
        &mut self,
        ctx: &Context,
        state: &mut State,
        dt: std::time::Duration,
    ) -> Out<State, Event> {
        let dt_secs = dt.as_secs_f32().max(1e-6);
        let raw_fps = 1.0 / dt_secs;
        self.fps_smoothed = self.fps_smoothed * 0.9 + raw_fps * 0.1;
        // Throttle label update to ~2× per second so the digits are readable
        self.fps_timer += dt_secs;
        if self.fps_timer >= 0.5 {
            self.fps_timer = 0.0;
            if let Some(label) = &mut self.fps_label {
                label.set_text(&format!("FPS: {:.0}", self.fps_smoothed));
            }
        }

        if let Some(panel) = &mut self.panel {
            return panel.on_update(ctx, state, dt);
        }
        Out::Empty
    }

    fn on_window_events(
        &mut self,
        ctx: &Context,
        state: &mut State,
        event: &WindowEvent,
    ) -> Out<State, Event> {
        if let WindowEvent::Resized(_) = event {
            // Re-resolve FPS label position on resize
            if let Some(label) = &mut self.fps_label {
                Layout::resolve(label, 0, 0, ctx.config.width, ctx.config.height, &ctx.queue);
            }
        }
        if let Some(panel) = &mut self.panel {
            return panel.on_window_events(ctx, state, event);
        }
        Out::Empty
    }

    fn on_custom_events(
        &mut self,
        _ctx: &Context,
        state: &mut State,
        event: Event,
    ) -> Option<Event> {
        match &event {
            Event::StrategyChanged(s) => {
                state.strategy = *s;
            }
            Event::DetectionDimsChanged(d) => {
                state.detection_dims = *d;
            }
            Event::ObjectShapeChanged(shape) => {
                state.object_shape = *shape;
            }
            Event::PlaceRandom(_) => {} // handled by SceneFlow
        }
        Some(event)
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        let panel_render = match &self.panel {
            Some(p) => p.on_render(),
            None => Render::None,
        };
        let fps_render = match &self.fps_label {
            Some(label) => label.render(),
            None => Render::None,
        };
        Render::Composed(vec![panel_render, fps_render])
    }
}
