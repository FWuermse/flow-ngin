use std::marker::PhantomData;

use instant::Duration;

use crate::{
    context::{Context, MouseButtonState},
    flow::{GraphicsFlow, Out},
    render::Render,
    ui::{
        Positioning,
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
/// Supported hooks:
/// - **Hover** just checks current context's coords agains button position.
/// - **Click** high accuracy (done via picking).
///
/// # Example
///
/// ```no_run
/// use flow_ngin::ui::button::Button;
///
/// let btn = Button::<State, Event>::new(1, 10, 10, 120, 40)
///     .normal_color([60, 60, 60, 255])
///     .hover_color([90, 90, 90, 255])
///     .pressed_color([30, 30, 30, 255])
///     .with_text(TextLabel::new("Click me"))
///     .on_click(|| Event::ButtonPressed);
/// ```
pub struct Button<S, E> {
    id: u32,
    pos: Positioning,
    local: Positioning,
    screen_width: u32,
    screen_height: u32,
    content: Option<ButtonContent>,
    fill: Option<Icon>,
    hover: Option<Icon>,
    pressed: Option<Icon>,
    on_click_fn: Option<Box<dyn Fn() -> E + 'static>>,
    visual_state: VisualState,
    _marker: PhantomData<S>,
}

impl<S: 'static, E: 'static> Button<S, E> {
    /// Create a button at pixel position `(x, y)` relative to its parent, with a unique pick `id`.
    ///
    /// The `id` must be non-zero and unique across all pickable objects in the scene.
    pub fn new(id: u32, x: u32, y: u32, width: u32, height: u32) -> Self {
        let pos = Positioning {
            x,
            y,
            width,
            height,
        };
        Self {
            id,
            pos,
            local: pos,
            screen_width: 0,
            screen_height: 0,
            content: None,
            fill: None,
            hover: None,
            pressed: None,
            on_click_fn: None,
            visual_state: VisualState::Normal,
            _marker: PhantomData,
        }
    }

    /// Set a text label as the button content.
    pub fn with_text(mut self, label: TextLabel) -> Self {
        self.content = Some(ButtonContent::Text(label));
        self
    }

    /// Set an icon as the button content.
    pub fn with_icon(mut self, icon: Icon) -> Self {
        self.content = Some(ButtonContent::Icon(icon));
        self
    }

    pub fn fill(mut self, icon: Icon) -> Self {
        self.fill = Some(icon);
        self
    }

    pub fn hover_fill(mut self, icon: Icon) -> Self {
        self.hover = Some(icon);
        self
    }

    pub fn click_fill(mut self, icon: Icon) -> Self {
        self.pressed = Some(icon);
        self
    }

    /// Register a callback that produces an event `E` when the button is clicked.
    pub fn on_click(mut self, f: impl Fn() -> E + 'static) -> Self {
        self.on_click_fn = Some(Box::new(f));
        self
    }

    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.pos.x as f64
            && x < (self.pos.x + self.pos.width) as f64
            && y >= self.pos.y as f64
            && y < (self.pos.y + self.pos.height) as f64
    }

    fn layout_content(&mut self, queue: &wgpu::Queue) {
        match &mut self.content {
            Some(ButtonContent::Icon(icon)) => {
                let ix = self.pos.x + self.pos.width.saturating_sub(icon.width_px) / 2;
                let iy = self.pos.y + self.pos.height.saturating_sub(icon.height_px) / 2;
                icon.set_position(ix, iy, queue);
            }
            Some(ButtonContent::Text(label)) => {
                // Centre vertically with a small horizontal inset.
                const INSET: u32 = 6;
                label.resolve(
                    (self.pos.x + INSET) as f32,
                    self.pos.y as f32,
                    self.pos.width.saturating_sub(2 * INSET) as f32,
                    self.pos.height as f32,
                );
            }
            None => {}
        }
    }

    fn layout_icon(&self, icon: Option<Icon>, queue: &wgpu::Queue) -> Option<Icon> {
        let mut icon = icon?;
        let ix = self.pos.x + self.pos.width.saturating_sub(icon.width_px) / 2;
        let iy = self.pos.y + self.pos.height.saturating_sub(icon.height_px) / 2;
        icon.set_position(ix, iy, queue);
        Some(icon)
    }
}

impl<S: 'static, E: 'static> Layout for Button<S, E> {
    fn resolve(
        &mut self,
        parent_x: u32,
        parent_y: u32,
        _parent_w: u32,
        _parent_h: u32,
        queue: &wgpu::Queue,
    ) {
        self.pos.x = parent_x + self.local.x;
        self.pos.y = parent_y + self.local.y;
        self.pos.width = self.local.width;
        self.pos.height = self.local.height;

        self.layout_content(queue);
        let fill = self.fill.take();
        self.fill = self.layout_icon(fill, queue);
        let hover = self.hover.take();
        self.hover = self.layout_icon(hover, queue);
        let pressed = self.pressed.take();
        self.pressed = self.layout_icon(pressed, queue);
    }
}

impl<S: 'static, E: 'static> GraphicsFlow<S, E> for Button<S, E> {
    fn on_init(&mut self, ctx: &mut Context, _: &mut S) -> Out<S, E> {
        self.screen_width = ctx.config.width;
        self.screen_height = ctx.config.height;

        // Init content GPU resources.
        match &mut self.content {
            Some(ButtonContent::Text(label)) => label.init(ctx),
            Some(ButtonContent::Icon(_)) | None => {}
        }

        self.layout_content(&ctx.queue);
        Out::Empty
    }

    fn on_update(&mut self, ctx: &Context, _state: &mut S, _dt: Duration) -> Out<S, E> {
        let pos = ctx.mouse.coords;
        let hovered = self.contains(pos.x, pos.y);
        self.visual_state = match (hovered, &ctx.mouse.pressed) {
            (true, MouseButtonState::Left) => VisualState::Pressed,
            (true, _) => VisualState::Hovered,
            (false, _) => VisualState::Normal,
        };
        Out::Empty
    }

    fn on_click(&mut self, _ctx: &Context, _state: &mut S, id: u32) -> Out<S, E> {
        if id != self.id {
            return Out::Empty;
        }
        if let Some(f) = &self.on_click_fn {
            let event = f();
            Out::FutEvent(vec![Box::new(async move { event })])
        } else {
            Out::Empty
        }
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
