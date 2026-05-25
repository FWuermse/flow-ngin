//! GuiFlow — strategy selector buttons and FPS display.

use std::{sync::Arc, time::Duration};

use flow_ngin::{
    WindowEvent,
    context::{Context, InitContext},
    flow::{GraphicsFlow, Out},
    render::Render,
    ui::{
        Button, Grid, HAlign, VAlign,
        image::{Atlas, Icon},
        text_label::TextLabel,
    },
};

use crate::{Event, State, collision_manager::Strategy};

pub struct GuiFlow {
    atlas: Arc<Atlas>,
    grid: Option<Grid<State, Event>>,
    fps_label: TextLabel,
    frame_count: u32,
    time_acc: f32,
}

impl GuiFlow {
    pub async fn new(ctx: InitContext) -> Self {
        let atlas = Arc::new(
            Atlas::new(&ctx.device, &ctx.queue, "card_atlas.png", 16, 16).await,
        );
        Self {
            atlas,
            grid: None,
            fps_label: TextLabel::new("FPS: --")
                .font_size(20.0)
                .line_height(28.0)
                .color([255, 255, 255])
                .halign(HAlign::Left)
                .valign(VAlign::Bottom)
                .width(200)
                .height(30),
            frame_count: 0,
            time_acc: 0.0,
        }
    }

    fn strategy_button(
        &self,
        ctx: &Context,
        label_text: &str,
        bg_start: u8,
        strategy: Strategy,
    ) -> Button<State, Event> {
        // Use a simple text label button (no icon) to show strategy name
        let label = TextLabel::new(label_text)
            .font_size(14.0)
            .line_height(18.0)
            .color([255, 255, 255])
            .halign(HAlign::Center)
            .valign(VAlign::Center);
        Button::new()
            .width(140)
            .height(50)
            .halign(HAlign::Center)
            .valign(VAlign::Center)
            .with_text(label)
            .fill(Icon::new(ctx, &self.atlas, bg_start))
            .hover_fill(Icon::new(ctx, &self.atlas, bg_start + 1))
            .click_fill(Icon::new(ctx, &self.atlas, bg_start + 2))
            .on_click(move |_ctx, _state| Event::StrategyChanged(strategy))
    }
}

impl GraphicsFlow<State, Event> for GuiFlow {
    fn on_init(&mut self, ctx: &mut Context, state: &mut State) -> Out<State, Event> {
        let qt_btn = self.strategy_button(ctx, "Quadtree", 22, Strategy::Quadtree);
        let ot_btn = self.strategy_button(ctx, "Octree", 22 + 3, Strategy::Octree);
        let gr_btn = self.strategy_button(ctx, "Dense Grid", 32, Strategy::Grid);
        let sg_btn = self.strategy_button(ctx, "Sparse Grid", 32 + 3, Strategy::SparseGrid);
        let bf_btn = self.strategy_button(ctx, "Brute Force", 22 + 6 * 16, Strategy::BruteForce);

        let grid = Grid::new(5, 1)
            .height(60)
            .valign(VAlign::Bottom)
            .with_child(0, 0, qt_btn)
            .with_child(1, 0, ot_btn)
            .with_child(2, 0, gr_btn)
            .with_child(3, 0, sg_btn)
            .with_child(4, 0, bf_btn);

        self.grid = Some(grid);
        let grid_out = self.grid.as_mut().unwrap().on_init(ctx, state);

        // Init FPS label
        self.fps_label.init(ctx);

        grid_out
    }

    fn on_update(&mut self, ctx: &Context, state: &mut State, dt: Duration) -> Out<State, Event> {
        self.frame_count += 1;
        self.time_acc += dt.as_secs_f32();
        if self.time_acc >= 1.0 {
            let fps = self.frame_count as f32 / self.time_acc;
            self.fps_label.set_text(&format!("FPS: {:.0}", fps));
            self.frame_count = 0;
            self.time_acc = 0.0;
        }

        if let Some(g) = &mut self.grid {
            return g.on_update(ctx, state, dt);
        }
        Out::Empty
    }

    fn on_window_events(
        &mut self,
        ctx: &Context,
        state: &mut State,
        event: &WindowEvent,
    ) -> Out<State, Event> {
        if let Some(g) = &mut self.grid {
            return g.on_window_events(ctx, state, event);
        }
        Out::Empty
    }

    fn on_custom_events(
        &mut self,
        _ctx: &Context,
        state: &mut State,
        event: Event,
    ) -> Option<Event> {
        match event {
            Event::StrategyChanged(s) => {
                state.strategy = s;
                None
            }
        }
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        let mut renders = vec![];
        if let Some(g) = &self.grid {
            renders.push(g.on_render());
        }
        renders.push(<TextLabel as GraphicsFlow<State, Event>>::on_render(&self.fps_label));
        Render::Composed(renders)
    }
}
