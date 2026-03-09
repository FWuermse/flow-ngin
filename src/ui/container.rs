use std::marker::PhantomData;

use crate::{
    context::Context,
    flow::{FlowConsturctor, GraphicsFlow, Out},
    render::Render,
    ui::layout::UIElement,
};

/// A screen-space container that positions and renders child UI elements.
///
/// The container owns a pixel rect and lays out children via the [`Layout`] trait,
/// delegating rendering to each child via [`GraphicsFlow::on_render`].
///
/// # Example
///
/// ```no_run
/// use flow_ngin::ui::{HAlign, VAlign, container::Container, image::Icon, text_label::TextLabel};
///
/// // In on_init:
/// let icon = Icon::new(ctx, atlas, 100, 17, 64, 64)
///     .halign(HAlign::Center)
///     .valign(VAlign::Center);
///
/// let container = Container::<State, Event>::new(0, 0, ctx.config.width, ctx.config.height)
///     .with_child(icon)
///     .with_child(TextLabel::new("Score: 0").position(16.0, 16.0));
/// ```
pub struct Container<S, E> {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    children: Vec<Box<dyn UIElement<S, E>>>,
    _marker: PhantomData<fn(S, E)>,
}

impl<S: 'static, E: 'static> Container<S, E> {
    /// Create a container at absolute pixel position `(x, y)` with the given dimensions.
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            children: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// Add a child element. Any type implementing both [`GraphicsFlow`] and [`Layout`] is accepted.
    pub fn with_child(mut self, child: impl UIElement<S, E> + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    /// Compute and apply positions for all children.
    ///
    /// Called automatically from `on_init`; call manually when embedding in a custom flow.
    pub fn resolve(&mut self, queue: &wgpu::Queue) {
        for child in &mut self.children {
            child.resolve(self.x, self.y, self.width, self.height, queue);
        }
    }

    /// Wrap this container in a [`FlowConsturctor`] for use with [`flow_ngin::flow::run`].
    pub fn into_constructor(self) -> FlowConsturctor<S, E> {
        Box::new(|_ctx| {
            Box::pin(async move { Box::new(self) as Box<dyn GraphicsFlow<S, E>> })
        })
    }
}

impl<S: 'static, E: 'static> GraphicsFlow<S, E> for Container<S, E> {
    fn on_init(&mut self, ctx: &mut Context, state: &mut S) -> Out<S, E> {
        for child in &mut self.children {
            child.on_init(ctx, state);
        }
        self.resolve(&ctx.queue);
        Out::Empty
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        Render::Composed(
            self.children.iter().map(|c| c.on_render()).collect()
        )
    }
}
