use std::sync::Arc;

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
    },
};

const PADDING: u32 = 8;
const LABEL_HEIGHT: u32 = 42;

/// A card UI component with optional background and multiple text labels.
///
/// # Example
///
/// ```no_run
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
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    icon: Option<Icon>,
    labels: Vec<TextLabel>,
    background: Option<Background>,
    bg_container: Option<Container<S, E>>,
}

impl<S: 'static, E: 'static> Card<S, E> {
    /// Create a card that fills its parent by default.
    ///
    /// Use `.width()`/`.height()` for explicit sizes, `.halign()`/`.valign()` for alignment.
    pub fn new() -> Self {
        Self {
            placement: Placement::default(),
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            icon: None,
            labels: Vec::new(),
            background: None,
            bg_container: None,
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

    /// Compute and apply positions for the icon and labels.
    ///
    /// Called automatically from `on_init`; call manually after moving the card.
    fn layout_children(&mut self, queue: &wgpu::Queue) {
        if let Some(icon) = &mut self.icon {
            let ix = self.x + self.width.saturating_sub(icon.width_px) / 2;
            let iy = self.y + PADDING;
            icon.set_position(ix, iy, queue);
        }

        let icon_area_h = self
            .icon
            .as_ref()
            .map(|i| i.height_px + 2 * PADDING)
            .unwrap_or(0);
        let label_start_y = self.y + icon_area_h + PADDING;
        let label_x = (self.x + PADDING) as f32;
        let label_w = self.width.saturating_sub(2 * PADDING) as f32;

        for (i, label) in self.labels.iter_mut().enumerate() {
            let ly = (label_start_y + i as u32 * LABEL_HEIGHT) as f32;
            label.resolve(label_x, ly, label_w, LABEL_HEIGHT as f32);
        }
    }
}

impl<S: 'static, E: 'static> Layout for Card<S, E> {
    /// Resolve the card's position from parent bounds and re-layout children.
    fn resolve(&mut self, parent_x: u32, parent_y: u32, parent_w: u32, parent_h: u32, queue: &wgpu::Queue) {
        let (x, y, w, h) = self.placement.resolve(parent_x, parent_y, parent_w, parent_h);
        self.x = x;
        self.y = y;
        self.width = w;
        self.height = h;

        // Update bg_container to match our new absolute position.
        if let Some(bg) = &mut self.bg_container {
            Layout::resolve(bg, self.x, self.y, self.width, self.height, queue);
        }

        self.layout_children(queue);
    }
}

impl<S: 'static, E: 'static> GraphicsFlow<S, E> for Card<S, E> {
    fn on_init(&mut self, ctx: &mut Context, state: &mut S) -> Out<S, E> {
        // Resolve own placement against screen dimensions.
        // For nested cards, the parent's Layout::resolve will override afterward.
        let (x, y, w, h) = self.placement.resolve(0, 0, ctx.config.width, ctx.config.height);
        self.x = x;
        self.y = y;
        self.width = w;
        self.height = h;

        // Build a background-only container — resolve places it at the card's position.
        let mut bg = Container::<S, E>::new()
            .width(self.width)
            .height(self.height);
        if let Some(background) = self.background.take() {
            bg = match background {
                Background::Color(rgba) => bg.with_background_color(rgba),
                Background::Texture(tex) => bg.with_background_texture(&tex),
            };
        }
        bg.on_init(ctx, state);
        // Position the bg at the card's current absolute position.
        Layout::resolve(&mut bg, self.x, self.y, self.width, self.height, &ctx.queue);
        self.bg_container = Some(bg);

        // Labels must be initialised before resolve so set_size is available.
        for label in &mut self.labels {
            label.init(ctx);
        }

        self.layout_children(&ctx.queue);
        Out::Empty
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        let mut renders: Vec<Render<'_, 'pass>> = Vec::new();

        if let Some(bg) = &self.bg_container {
            renders.push(bg.on_render());
        }
        if let Some(icon) = &self.icon {
            renders.push(GraphicsFlow::<S, E>::on_render(icon));
        }
        for label in &self.labels {
            renders.push(GraphicsFlow::<S, E>::on_render(label));
        }

        Render::Composed(renders)
    }
}
