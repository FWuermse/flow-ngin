use instant::Duration;

use winit::event::{MouseScrollDelta, WindowEvent};

use crate::{
    context::Context,
    flow::{GraphicsFlow, Out},
    render::{ClipRect, Render},
    ui::{
        Placement,
        container::merge_outs,
        image::Icon,
        layout::{Layout, UIElement},
    },
};

/// Width of the optional scrollbar thumb in pixels.
const THUMB_PX: u32 = 6;

/// A vertical list of rows that scrolls inside a clipped viewport.
///
/// Scrolling moves by whole rows: the top visible row always starts at the viewport's top
/// edge (UI positions are unsigned, so rows can't start above the screen) and only the last
/// visible row can be cut off. Mouse buttons and the wheel only reach the rows while the
/// cursor is inside the viewport, so cut-off parts can't be clicked; other events (keyboard,
/// focus) always reach the visible rows.
///
/// Rows added after `on_init` must be initialised by the caller, followed by
/// [`relayout`](Self::relayout).
///
/// # Example
///
/// ```ignore
/// use flow_ngin::ui::{scroll::ScrollContainer, text_label::TextLabel, image::Icon};
///
/// let list = ScrollContainer::<State, Event>::new()
///     .width(300)
///     .height(200)
///     .thumb(Icon::from_color(ctx, [200, 160, 80, 255]))
///     .with_row(40, TextLabel::new("First"))
///     .with_row(40, TextLabel::new("Second"));
/// ```
pub struct ScrollContainer<S, E> {
    placement: Placement,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    rows: Vec<(u32, Box<dyn UIElement<S, E>>)>,
    first: usize,
    /// Rows from `first` that are at least partly visible.
    visible: usize,
    /// Wheel movement in rows that hasn't added up to a whole row yet.
    wheel: f32,
    thumb: Option<Icon>,
    was_inside: bool,
}

impl<S: 'static, E: Send + 'static> ScrollContainer<S, E> {
    pub fn new() -> Self {
        Self {
            placement: Placement::default(),
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            rows: Vec::new(),
            first: 0,
            visible: 0,
            wheel: 0.,
            thumb: None,
            was_inside: false,
        }
    }

    pub fn halign(mut self, align: crate::ui::HAlign) -> Self {
        self.placement.halign = align;
        self
    }

    pub fn valign(mut self, align: crate::ui::VAlign) -> Self {
        self.placement.valign = align;
        self
    }

    pub fn width(mut self, w: u32) -> Self {
        self.placement.width = Some(w);
        self
    }

    pub fn height(mut self, h: u32) -> Self {
        self.placement.height = Some(h);
        self
    }

    /// Show a scrollbar thumb at the right edge while the rows don't fit.
    pub fn thumb(mut self, icon: Icon) -> Self {
        self.thumb = Some(icon);
        self
    }

    /// Append a row with the given height.
    pub fn with_row<T: UIElement<S, E> + 'static>(mut self, row_height: u32, child: T) -> Self {
        self.rows.push((row_height, Box::new(child)));
        self
    }

    pub fn add_row(&mut self, row_height: u32, child: Box<dyn UIElement<S, E>>) {
        self.rows.push((row_height, child));
    }

    pub fn clear(&mut self) {
        self.rows.clear();
        self.first = 0;
        self.visible = 0;
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Index of the topmost visible row.
    pub fn first_row(&self) -> usize {
        self.first
    }

    /// Scroll so that `row` is the topmost visible row, as far as the content allows.
    pub fn scroll_to(&mut self, row: usize, queue: &wgpu::Queue) {
        self.first = row;
        self.relayout(queue);
    }

    /// Scroll by `rows` (negative: up).
    pub fn scroll_by(&mut self, rows: isize, queue: &wgpu::Queue) {
        self.scroll_to(self.first.saturating_add_signed(rows), queue);
    }

    /// Re-position the rows, e.g. after adding or removing some.
    pub fn relayout(&mut self, queue: &wgpu::Queue) {
        let heights: Vec<u32> = self.rows.iter().map(|(h, _)| *h).collect();
        let max_first = max_first(&heights, self.height);
        self.first = self.first.min(max_first);
        self.visible = visible_rows(&heights, self.first, self.height);
        let row_width = if max_first > 0 && self.thumb.is_some() {
            self.width.saturating_sub(THUMB_PX + 2)
        } else {
            self.width
        };
        let mut y = self.y;
        for (row_height, child) in &mut self.rows[self.first..self.first + self.visible] {
            child.resolve(self.x, y, row_width, *row_height, queue);
            y += *row_height;
        }
        if let Some(thumb) = &mut self.thumb {
            let total = self.rows.len().max(1) as u32;
            let size = (self.height * self.visible as u32 / total)
                .max(16)
                .min(self.height);
            let offset = if max_first > 0 {
                (self.height - size) * self.first as u32 / max_first as u32
            } else {
                0
            };
            thumb.width_px = if max_first > 0 { THUMB_PX } else { 0 };
            thumb.height_px = size;
            thumb.set_position(
                self.x + self.width.saturating_sub(THUMB_PX),
                self.y + offset,
                queue,
            );
        }
    }

    fn inside(&self, ctx: &Context) -> bool {
        let (x, y) = (ctx.mouse.coords.x, ctx.mouse.coords.y);
        x >= self.x as f64
            && x < (self.x + self.width) as f64
            && y >= self.y as f64
            && y < (self.y + self.height) as f64
    }

    fn visible_mut(&mut self) -> impl Iterator<Item = &mut Box<dyn UIElement<S, E>>> {
        self.rows[self.first..self.first + self.visible]
            .iter_mut()
            .map(|(_, child)| child)
    }
}

/// Largest first row that still fills the viewport, so the last row can be fully shown.
fn max_first(heights: &[u32], viewport: u32) -> usize {
    let mut below = 0;
    for (i, h) in heights.iter().enumerate().rev() {
        below += h;
        if below > viewport {
            // A last row taller than the viewport is shown from its top.
            return (i + 1).min(heights.len() - 1);
        }
    }
    0
}

/// Rows from `first` that are at least partly inside the viewport.
fn visible_rows(heights: &[u32], first: usize, viewport: u32) -> usize {
    let mut y = 0;
    heights[first.min(heights.len())..]
        .iter()
        .take_while(|h| {
            let start = y;
            y += **h;
            start < viewport
        })
        .count()
}

impl<S: 'static, E: Send + 'static> Layout for ScrollContainer<S, E> {
    fn resolve(
        &mut self,
        parent_x: u32,
        parent_y: u32,
        parent_w: u32,
        parent_h: u32,
        queue: &wgpu::Queue,
    ) {
        let (x, y, w, h) = self
            .placement
            .resolve(parent_x, parent_y, parent_w, parent_h);
        self.x = x;
        self.y = y;
        self.width = w;
        self.height = h;
        self.relayout(queue);
    }
}

impl<S: 'static, E: Send + 'static> GraphicsFlow<S, E> for ScrollContainer<S, E> {
    fn on_init(&mut self, ctx: &mut Context, state: &mut S) -> Out<S, E> {
        for (_, child) in &mut self.rows {
            child.on_init(ctx, state);
        }
        Layout::resolve(self, 0, 0, ctx.config.width, ctx.config.height, &ctx.queue);
        Out::Empty
    }

    fn on_update(&mut self, ctx: &Context, state: &mut S, dt: Duration) -> Out<S, E> {
        // One more update after leaving, so hover states can reset.
        let inside = self.inside(ctx);
        let forward = inside || self.was_inside;
        self.was_inside = inside;
        if !forward {
            return Out::Empty;
        }
        merge_outs(self.visible_mut().map(|c| c.on_update(ctx, state, dt)))
    }

    fn on_window_events(&mut self, ctx: &Context, state: &mut S, event: &WindowEvent) -> Out<S, E> {
        match event {
            WindowEvent::Resized(_) => {
                Layout::resolve(self, 0, 0, ctx.config.width, ctx.config.height, &ctx.queue);
                return Out::Empty;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if self.inside(ctx) {
                    let row_height = self.rows.get(self.first).map_or(1, |(h, _)| *h).max(1);
                    self.wheel -= match delta {
                        MouseScrollDelta::LineDelta(_, lines) => *lines,
                        MouseScrollDelta::PixelDelta(p) => p.y as f32 / row_height as f32,
                    };
                    let rows = self.wheel.trunc();
                    self.wheel -= rows;
                    if rows != 0. {
                        self.scroll_by(rows as isize, &ctx.queue);
                    }
                }
                return Out::Empty;
            }
            WindowEvent::MouseInput { .. } | WindowEvent::Touch(_) if !self.inside(ctx) => {
                return Out::Empty;
            }
            _ => {}
        }
        merge_outs(
            self.visible_mut()
                .map(|c| c.on_window_events(ctx, state, event)),
        )
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        let mut renders: Vec<_> = self.rows[self.first..self.first + self.visible]
            .iter()
            .map(|(_, c)| c.on_render())
            .collect();
        if let Some(thumb) = &self.thumb {
            renders.push(GraphicsFlow::<S, E>::on_render(thumb));
        }
        Render::Clipped(
            ClipRect {
                x: self.x,
                y: self.y,
                width: self.width,
                height: self.height,
            },
            Box::new(Render::Composed(renders)),
        )
    }

    fn on_tick(&mut self, ctx: &Context, state: &mut S) -> Out<S, E> {
        merge_outs(self.visible_mut().map(|c| c.on_tick(ctx, state)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_row_stays_reachable() {
        assert_eq!(max_first(&[100; 5], 250), 3);
        assert_eq!(max_first(&[100; 2], 250), 0);
        assert_eq!(max_first(&[], 250), 0);
        assert_eq!(max_first(&[50, 300], 250), 1);
    }

    #[test]
    fn partly_visible_rows_count_as_visible() {
        assert_eq!(visible_rows(&[100; 5], 0, 250), 3);
        assert_eq!(visible_rows(&[100; 5], 3, 250), 2);
        assert_eq!(visible_rows(&[100; 5], 0, 200), 2);
        assert_eq!(visible_rows(&[100; 5], 9, 250), 0);
        assert_eq!(visible_rows(&[], 0, 250), 0);
    }
}
