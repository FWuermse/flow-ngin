use std::marker::PhantomData;

use instant::Duration;

use crate::{
    context::{Context, MouseButtonState},
    flow::{GraphicsFlow, Out},
    render::Render,
    ui::{
        HAlign, Placement, VAlign,
        image::Icon,
        layout::Layout,
        text_label::TextLabel,
    },
};

#[derive(Default, PartialEq)]
enum VisualState {
    #[default]
    Normal,
    Hovered,
    Pressed,
}

/// Button is either of text/icon.
pub enum ButtonContent {
    Text(TextLabel),
    Icon(Icon),
}

/// A clickable button with text or icon content.
///
/// Click detection is coordinate-based: the button tracks mouse state transitions
/// and fires when the mouse is released while hovering over the button.
///
/// # Example
///
/// ```ignore
/// use flow_ngin::ui::button::Button;
///
/// let btn = Button::<State, Event>::new()
///     .width(120)
///     .height(40)
///     .fill(normal_icon)
///     .hover_fill(hover_icon)
///     .click_fill(pressed_icon)
///     .with_text(TextLabel::new("Click me"))
///     .on_click(|| Event::ButtonPressed);
/// ```
pub struct Button<S, E> {
    placement: Placement,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    content: Option<ButtonContent>,
    fill: Option<Icon>,
    hover: Option<Icon>,
    pressed: Option<Icon>,
    on_click_fn: Option<Box<dyn Fn(&Context, &S) -> E + 'static>>,
    content_scale: f32,
    visual_state: VisualState,
    was_pressed: bool,
    _marker: PhantomData<S>,
}

impl<S: 'static, E: 'static> Button<S, E> {
    /// Create a button that fills its parent by default.
    ///
    /// Use `.width()`/`.height()` for explicit sizes, `.halign()`/`.valign()` for alignment.
    pub fn new() -> Self {
        Self {
            placement: Placement::default(),
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            content: None,
            fill: None,
            hover: None,
            pressed: None,
            on_click_fn: None,
            content_scale: 0.8,
            visual_state: VisualState::Normal,
            was_pressed: false,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn halign(mut self, align: HAlign) -> Self {
        self.placement.halign = align;
        self
    }

    #[inline]
    pub fn valign(mut self, align: VAlign) -> Self {
        self.placement.valign = align;
        self
    }

    #[inline]
    pub fn width(mut self, w: u32) -> Self {
        self.placement.width = Some(w);
        self
    }

    #[inline]
    pub fn height(mut self, h: u32) -> Self {
        self.placement.height = Some(h);
        self
    }

    #[inline]
    pub fn square(mut self, dim: u32) -> Self {
        self.placement.height = Some(dim);
        self.placement.width = Some(dim);
        self
    }

    /// Set a text label as the button content.
    #[inline]
    pub fn with_text(mut self, label: TextLabel) -> Self {
        self.content = Some(ButtonContent::Text(label));
        self
    }

    /// Set an icon as the button content.
    #[inline]
    pub fn with_icon(mut self, icon: Icon) -> Self {
        self.content = Some(ButtonContent::Icon(icon));
        self
    }

    #[inline]
    pub fn fill(mut self, icon: Icon) -> Self {
        self.fill = Some(icon);
        self
    }

    #[inline]
    pub fn hover_fill(mut self, icon: Icon) -> Self {
        self.hover = Some(icon);
        self
    }

    #[inline]
    pub fn click_fill(mut self, icon: Icon) -> Self {
        self.pressed = Some(icon);
        self
    }

    /// Register a callback that produces an event `E` when the button is clicked.
    #[inline]
    pub fn on_click(mut self, f: impl Fn(&Context, &S) -> E + 'static) -> Self {
        self.on_click_fn = Some(Box::new(f));
        self
    }

    /// Set the scale of the content icon relative to the button size (default: `0.8`).
    pub fn content_scale(mut self, scale: f32) -> Self {
        self.content_scale = scale.clamp(0.0, 1.0);
        self
    }

    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x as f64
            && x < (self.x + self.width) as f64
            && y >= self.y as f64
            && y < (self.y + self.height) as f64
    }

    fn layout_content(&mut self, queue: &wgpu::Queue) {
        match &mut self.content {
            Some(ButtonContent::Icon(icon)) => {
                let icon_w = (self.width as f32 * self.content_scale) as u32;
                let icon_h = (self.height as f32 * self.content_scale) as u32;
                icon.width_px = icon_w;
                icon.height_px = icon_h;
                let ix = self.x + (self.width - icon_w) / 2;
                let iy = self.y + (self.height - icon_h) / 2;
                icon.set_position(ix, iy, queue);
            }
            Some(ButtonContent::Text(label)) => {
                Layout::resolve(
                    label,
                    self.x,
                    self.y,
                    self.width,
                    self.height,
                    queue,
                );
            }
            None => {}
        }
    }

    fn layout_fill(&self, icon: Option<Icon>, queue: &wgpu::Queue) -> Option<Icon> {
        let mut icon = icon?;
        icon.width_px = self.width;
        icon.height_px = self.height;
        icon.set_position(self.x, self.y, queue);
        Some(icon)
    }
}

impl<S: 'static, E: 'static> Layout for Button<S, E> {
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

        self.layout_content(queue);
        let fill = self.fill.take();
        self.fill = self.layout_fill(fill, queue);
        let hover = self.hover.take();
        self.hover = self.layout_fill(hover, queue);
        let pressed = self.pressed.take();
        self.pressed = self.layout_fill(pressed, queue);
    }
}

impl<S: 'static, E: 'static> GraphicsFlow<S, E> for Button<S, E> {
    fn on_init(&mut self, ctx: &mut Context, _: &mut S) -> Out<S, E> {
        // Resolve own placement against screen dimensions.
        // For nested buttons, the parent's Layout::resolve will override afterward.
        let (x, y, w, h) = self.placement.resolve(0, 0, ctx.config.width, ctx.config.height);
        self.x = x;
        self.y = y;
        self.width = w;
        self.height = h;

        // Init content GPU resources.
        match &mut self.content {
            Some(ButtonContent::Text(label)) => label.init(ctx),
            Some(ButtonContent::Icon(_)) | None => {}
        }

        self.layout_content(&ctx.queue);
        Out::Empty
    }

    fn on_update(&mut self, ctx: &Context, state: &mut S, _dt: Duration) -> Out<S, E> {
        let pos = ctx.mouse.coords;
        let hovered = self.contains(pos.x, pos.y);
        let is_pressed = matches!(ctx.mouse.pressed, MouseButtonState::Left);

        self.visual_state = match (hovered, is_pressed) {
            (true, true) => VisualState::Pressed,
            (true, false) => VisualState::Hovered,
            (false, _) => VisualState::Normal,
        };

        // Detect click: was pressed last frame, now released, still hovering.
        let clicked = self.was_pressed && !is_pressed && hovered;
        self.was_pressed = is_pressed && hovered;

        if clicked {
            if let Some(f) = &self.on_click_fn {
                let event = f(ctx, state);
                return Out::FutEvent(vec![Box::new(async move { event })]);
            }
        }
        Out::Empty
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        let bind_group = match self.visual_state {
            VisualState::Normal => if let Some(fill) = &self.fill {
                GraphicsFlow::<S, E>::on_render(fill)
            } else {
                Render::None
            },
            VisualState::Hovered => if let Some(hover) = &self.hover {
                GraphicsFlow::<S, E>::on_render(hover)
            } else {
                Render::None
            },
            VisualState::Pressed => if let Some(pressed) = &self.pressed {
                GraphicsFlow::<S, E>::on_render(pressed)
            } else {
                Render::None
            },
        };

        match &self.content {
            Some(content) => {
                let content_render = match content {
                    ButtonContent::Text(label) => label.render(),
                    ButtonContent::Icon(icon) => GraphicsFlow::<S, E>::on_render(icon),
                };
                Render::Composed(vec![bind_group, content_render])
            }
            None => bind_group,
        }
    }
}
