use instant::Duration;

use winit::event::WindowEvent;

use crate::{
    context::Context,
    flow::{GraphicsFlow, Out},
    render::Render,
    ui::{
        Placement,
        container::merge_outs,
        layout::{Layout, UIElement},
    },
};

/// A vertical stack layout that arranges children top-to-bottom.
///
/// Each child is given an explicit row height. Use sub-containers or
/// centering (via `HAlign`/`VAlign` on the VStack itself).
///
/// # Example
///
/// ```ignore
/// use flow_ngin::ui::{HAlign, VAlign, container::Container, vstack::VStack, text_label::TextLabel};
///
/// let stack = VStack::<State, Event>::new()
///     .width(200)
///     .halign(HAlign::Center)
///     .with_child(36, TextLabel::new("Title").font_size(24.0))
///     .with_child(28, TextLabel::new("Subtitle").font_size(18.0));
/// ```
pub struct VStack<S, E> {
    placement: Placement,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    children: Vec<(u32, Box<dyn UIElement<S, E>>)>,
}

impl<S: 'static, E: Send + 'static> VStack<S, E> {
    pub fn new() -> Self {
        Self {
            placement: Placement::default(),
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            children: Vec::new(),
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

    /// Append a child with the given row height.
    pub fn with_child(mut self, row_height: u32, child: impl UIElement<S, E> + 'static) -> Self {
        self.children.push((row_height, Box::new(child)));
        self
    }

    fn resolve_children(&mut self, queue: &wgpu::Queue) {
        let mut current_y = self.y;
        for (row_h, child) in &mut self.children {
            child.resolve(self.x, current_y, self.width, *row_h, queue);
            current_y += *row_h;
        }
    }
}

impl<S: 'static, E: Send + 'static> Layout for VStack<S, E> {
    fn resolve(
        &mut self,
        parent_x: u32,
        parent_y: u32,
        parent_w: u32,
        parent_h: u32,
        queue: &wgpu::Queue,
    ) {
        let (x, y, w, h) = self.placement.resolve(parent_x, parent_y, parent_w, parent_h);
        self.x = x;
        self.y = y;
        self.width = w;
        self.height = h;
        self.resolve_children(queue);
    }
}

impl<S: 'static, E: Send + 'static> GraphicsFlow<S, E> for VStack<S, E> {
    fn on_init(&mut self, ctx: &mut Context, state: &mut S) -> Out<S, E> {
        let (x, y, w, h) = self.placement.resolve(0, 0, ctx.config.width, ctx.config.height);
        self.x = x;
        self.y = y;
        self.width = w;
        self.height = h;

        for (_, child) in &mut self.children {
            child.on_init(ctx, state);
        }

        self.resolve_children(&ctx.queue);
        Out::Empty
    }

    fn on_update(&mut self, ctx: &Context, state: &mut S, dt: Duration) -> Out<S, E> {
        merge_outs(self.children.iter_mut().map(|(_, c)| c.on_update(ctx, state, dt)))
    }

    fn on_window_events(&mut self, ctx: &Context, state: &mut S, event: &WindowEvent) -> Out<S, E> {
        if let WindowEvent::Resized(_) = event {
            Layout::resolve(self, 0, 0, ctx.config.width, ctx.config.height, &ctx.queue);
            return Out::Empty;
        }
        merge_outs(self.children.iter_mut().map(|(_, c)| c.on_window_events(ctx, state, event)))
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        Render::Composed(self.children.iter().map(|(_, c)| c.on_render()).collect())
    }

    fn on_tick(&mut self, ctx: &Context, state: &mut S) -> Out<S, E> {
        merge_outs(self.children.iter_mut().map(|(_, c)| c.on_tick(ctx, state)))
    }
}
