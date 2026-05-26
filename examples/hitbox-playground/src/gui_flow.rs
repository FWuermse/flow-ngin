use std::sync::Arc;

use flow_ngin::{
    WindowEvent,
    context::{Context, InitContext},
    flow::{GraphicsFlow, Out},
    render::Render,
    ui::{
        Button, Grid, HAlign, Layout, Slider, VAlign, VStack, Value,
        image::{Atlas, Icon},
        text_label::TextLabel,
    },
};

use crate::{Event, ObjectShape, State, Strategy};

pub struct GuiFlow {
    #[allow(dead_code)]
    atlas: Arc<Atlas>,
    panel: Option<VStack<State, Event>>,
    // Sliders stored outside so we can read their values
    dim_value: Value<f32>,
    shape_value: Value<f32>,
    // FPS counter — held separately so we can call set_text each frame
    fps_label: Option<TextLabel>,
    fps_smoothed: f32,
}

impl GuiFlow {
    pub async fn new(ctx: InitContext) -> Self {
        let atlas = Arc::new(Atlas::new(&ctx.device, &ctx.queue, "card_atlas.png", 16, 16).await);
        Self {
            atlas,
            panel: None,
            dim_value: Value::new(0.5), // middle = 2D
            shape_value: Value::new(0.0), // 0.0 = Cube3D
            fps_label: None,
            fps_smoothed: 60.0,
        }
    }
}

impl GraphicsFlow<State, Event> for GuiFlow {
    fn on_init(&mut self, ctx: &mut Context, state: &mut State) -> Out<State, Event> {
        // Button dimensions
        let btn_h = 44u32;
        let panel_w = 340u32;

        // ── Strategy buttons ──────────────────────────────────────────────────
        let strategy_buttons = Grid::<State, Event>::new(4, 1)
            .width(panel_w)
            .height(btn_h)
            .with_child(
                0, 0,
                Button::<State, Event>::new()
                    .fill(Icon::from_color(ctx, [50, 120, 50, 220]))
                    .hover_fill(Icon::from_color(ctx, [70, 150, 70, 220]))
                    .click_fill(Icon::from_color(ctx, [30, 100, 30, 220]))
                    .with_text(TextLabel::new("Grid").font_size(18.0).color([255, 255, 255]))
                    .on_click(|_, _| Event::StrategyChanged(Strategy::Grid)),
            )
            .with_child(
                1, 0,
                Button::<State, Event>::new()
                    .fill(Icon::from_color(ctx, [50, 80, 160, 220]))
                    .hover_fill(Icon::from_color(ctx, [70, 100, 190, 220]))
                    .click_fill(Icon::from_color(ctx, [30, 60, 140, 220]))
                    .with_text(TextLabel::new("Sparse").font_size(18.0).color([255, 255, 255]))
                    .on_click(|_, _| Event::StrategyChanged(Strategy::SparseGrid)),
            )
            .with_child(
                2, 0,
                Button::<State, Event>::new()
                    .fill(Icon::from_color(ctx, [140, 60, 60, 220]))
                    .hover_fill(Icon::from_color(ctx, [170, 80, 80, 220]))
                    .click_fill(Icon::from_color(ctx, [120, 40, 40, 220]))
                    .with_text(TextLabel::new("Brute").font_size(18.0).color([255, 255, 255]))
                    .on_click(|_, _| Event::StrategyChanged(Strategy::BruteForce)),
            )
            .with_child(
                3, 0,
                Button::<State, Event>::new()
                    .fill(Icon::from_color(ctx, [120, 60, 160, 220]))
                    .hover_fill(Icon::from_color(ctx, [150, 80, 190, 220]))
                    .click_fill(Icon::from_color(ctx, [100, 40, 140, 220]))
                    .with_text(TextLabel::new("Tree").font_size(18.0).color([255, 255, 255]))
                    .on_click(|_, _| Event::StrategyChanged(Strategy::SpatialTree)),
            );

        // ── Detection dims slider ─────────────────────────────────────────────
        let dim_slider = Slider::<State, Event>::new()
            .width(panel_w)
            .height(32)
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

        // ── Object shape slider ───────────────────────────────────────────────
        let shape_slider = Slider::<State, Event>::new()
            .width(panel_w)
            .height(32)
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

        // ── Assemble panel VStack ─────────────────────────────────────────────
        let mut panel = VStack::<State, Event>::new()
            .width(panel_w)
            .halign(HAlign::Left)
            .valign(VAlign::Bottom)
            .with_child(
                24,
                TextLabel::new("Detection Space")
                    .font_size(16.0)
                    .color([200, 200, 200]),
            )
            .with_child(32, dim_slider)
            .with_child(
                24,
                TextLabel::new("Object Shape")
                    .font_size(16.0)
                    .color([200, 200, 200]),
            )
            .with_child(32, shape_slider)
            .with_child(
                22,
                TextLabel::new("Strategy")
                    .font_size(16.0)
                    .color([200, 200, 200]),
            )
            .with_child(btn_h, strategy_buttons)
            .with_child(
                22,
                TextLabel::new("Move: mouse  Y: scroll  Place: LClick")
                    .font_size(14.0)
                    .color([150, 150, 150]),
            )
            .with_child(
                20,
                TextLabel::new("White=idle  Yellow=broad  Red=overlap")
                    .font_size(14.0)
                    .color([150, 150, 150]),
            );

        panel.on_init(ctx, state);
        self.panel = Some(panel);

        // ── FPS label — top-right corner ──────────────────────────────────────
        let mut fps = TextLabel::new("FPS: --")
            .font_size(16.0)
            .color([180, 220, 140])
            .halign(HAlign::Right)
            .valign(VAlign::Top)
            .width(120)
            .height(24);
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
        // Exponentially smooth FPS to avoid flicker
        let dt_secs = dt.as_secs_f32().max(1e-6);
        let raw_fps = 1.0 / dt_secs;
        self.fps_smoothed = self.fps_smoothed * 0.9 + raw_fps * 0.1;
        if let Some(label) = &mut self.fps_label {
            label.set_text(&format!("FPS: {:.0}", self.fps_smoothed));
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
