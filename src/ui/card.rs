use std::sync::Arc;

use instant::Duration;
use winit::event::WindowEvent;

use crate::{
    context::Context,
    flow::{GraphicsFlow, Out},
    render::Render,
    ui::{
        HAlign, Placement, VAlign,
        background::{Background, BackgroundTexture},
        container::Container,
        image::Icon,
        layout::Layout,
        text_label::TextLabel,
        vstack::VStack,
    },
};

/// A card UI component with an icon at the top and text labels below.
///
/// # Example
///
/// ```ignore
/// use std::sync::Arc;
/// use flow_ngin::ui::{HAlign, VAlign, background::BackgroundTexture, card::Card, image::Icon, text_label::TextLabel};
///
/// // In on_init:
/// let icon = Icon::new(ctx, atlas, 0).width(64).height(64);
/// let card_bg = Arc::new(BackgroundTexture::new(&ctx.device, &ctx.queue, "card.png").await);
///
/// let card = Card::<State, Event>::new()
///     .width(200)
///     .height(300)
///     .with_background_texture(Arc::clone(&card_bg))
///     .with_icon(icon)
///     .with_label(TextLabel::new("Title").font_size(20.0))
///     .with_label(TextLabel::new("Subtitle"));
/// ```
pub struct Card<S, E> {
    placement: Placement,
    icon: Option<Icon>,
    labels: Vec<TextLabel>,
    background: Option<Background>,
    container: Option<Container<S, E>>,
}

impl<S: 'static, E: 'static> Card<S, E> {
    /// Create a card that fills its parent by default.
    ///
    /// Use `.width()`/`.height()` for explicit sizes, `.halign()`/`.valign()` for alignment.
    pub fn new() -> Self {
        Self {
            placement: Placement::default(),
            icon: None,
            labels: Vec::new(),
            background: None,
            container: None,
        }
    }

    pub fn halign(mut self, align: HAlign) -> Self {
        self.placement.halign = align;
        self
    }

    pub fn valign(mut self, align: VAlign) -> Self {
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

    /// Set the icon shown centred in the top section of the card.
    pub fn with_icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Append a text label to the bottom section. Labels are rendered in insertion order.
    pub fn with_label(mut self, label: TextLabel) -> Self {
        self.labels.push(label);
        self
    }

    /// Set a solid-colour background.
    pub fn with_background_color(mut self, rgba: [u8; 4]) -> Self {
        self.background = Some(Background::Color(rgba));
        self
    }

    /// Set a the default container texture.
    pub fn with_background_texture(mut self, texture: Arc<BackgroundTexture>) -> Self {
        self.background = Some(Background::Texture(texture));
        self
    }
}

impl<S: 'static, E: Send + 'static> Layout for Card<S, E> {
    fn resolve(&mut self, parent_x: u32, parent_y: u32, parent_w: u32, parent_h: u32, queue: &wgpu::Queue) {
        if let Some(container) = &mut self.container {
            Layout::resolve(container, parent_x, parent_y, parent_w, parent_h, queue);
        }
    }
}

impl<S: 'static, E: Send + 'static> GraphicsFlow<S, E> for Card<S, E> {
    fn on_init(&mut self, ctx: &mut Context, state: &mut S) -> Out<S, E> {
        let mut vstack = VStack::<S, E>::new();

        if let Some(icon) = self.icon.take() {
            let icon_h = icon.placement.height.unwrap_or(0);
            vstack = vstack.with_child(
                icon_h,
                Container::<S, E>::new().with_child(
                    icon.halign(HAlign::Center).valign(VAlign::Center),
                ),
            );
        }

        for label in self.labels.drain(..) {
            let row_h = label.get_line_height() as u32;
            vstack = vstack.with_child(row_h, label);
        }

        let mut container = Container::<S, E>::new();

        if let Some(w) = self.placement.width {
            container = container.width(w);
        }
        if let Some(h) = self.placement.height {
            container = container.height(h);
        }
        container = container
            .halign(self.placement.halign)
            .valign(self.placement.valign);

        if let Some(bg) = self.background.take() {
            container = match bg {
                Background::Color(rgba) => container.with_background_color(rgba),
                Background::Texture(tex) => container.with_background_texture(&tex),
            };
        }

        container = container.with_child(vstack);
        container.on_init(ctx, state);
        self.container = Some(container);
        Out::Empty
    }

    fn on_update(&mut self, ctx: &Context, state: &mut S, dt: Duration) -> Out<S, E> {
        if let Some(container) = &mut self.container {
            container.on_update(ctx, state, dt)
        } else {
            Out::Empty
        }
    }

    fn on_window_events(&mut self, ctx: &Context, state: &mut S, event: &WindowEvent) -> Out<S, E> {
        if let Some(container) = &mut self.container {
            container.on_window_events(ctx, state, event)
        } else {
            Out::Empty
        }
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        match &self.container {
            Some(container) => container.on_render(),
            None => Render::None,
        }
    }
}
