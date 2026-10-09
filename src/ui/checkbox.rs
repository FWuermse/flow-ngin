use instant::Duration;

use crate::{
    context::{Context, MouseButtonState},
    flow::{GraphicsFlow, Out},
    render::Render,
    ui::{HAlign, Placement, VAlign, image::Icon, layout::Layout, value::Value},
};

/// A togglable checkbox that binds to a `Value<bool>`.
///
/// # Example
///
/// ```ignore
/// use flow_ngin::ui::{checkbox::Checkbox, value::Value};
///
/// let checked = Value::new(false);
///
/// let cb = Checkbox::<State, Event>::new()
///     .width(32)
///     .height(32)
///     .unchecked(unchecked_icon)
///     .checked(checked_icon)
///     .bind(&checked);
/// ```
pub struct Checkbox<S, E: Send> {
    placement: Placement,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    checked_icon: Option<Icon>,
    overlay_checked: bool,
    unchecked_icon: Option<Icon>,
    value: Option<Value<bool>>,
    on_change: Option<Box<dyn Fn(bool) -> Out<S, E>>>,
    was_pressed: bool,
    event_driven: bool,
}

impl<S: 'static, E: Send + 'static> Checkbox<S, E> {
    /// Initialize GPU resources without changing the application context.
    pub fn init(&mut self, ctx: &Context) {
        let (x, y, w, h) = self
            .placement
            .resolve(0, 0, ctx.config.width, ctx.config.height);
        self.x = x;
        self.y = y;
        self.width = w;
        self.height = h;

        Self::layout_icon(&mut self.checked_icon, x, y, w, h, &ctx.queue);
        Self::layout_icon(&mut self.unchecked_icon, x, y, w, h, &ctx.queue);
    }

    pub fn new() -> Self {
        Self {
            placement: Placement::default(),
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            checked_icon: None,
            overlay_checked: false,
            unchecked_icon: None,
            value: None,
            on_change: None,
            was_pressed: false,
            event_driven: false,
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

    /// Set the icon shown when unchecked.
    pub fn unchecked(mut self, icon: Icon) -> Self {
        self.unchecked_icon = Some(icon);
        self
    }

    /// Set the icon shown when checked.
    pub fn checked(mut self, icon: Icon) -> Self {
        self.overlay_checked = false;
        self.checked_icon = Some(icon);
        self
    }

    /// Draw this checkmark over the unchecked icon when checked.
    pub fn checked_overlay(mut self, icon: Icon) -> Self {
        self.checked_icon = Some(icon);
        self.overlay_checked = true;
        self
    }

    /// Bind this checkbox to a `Value<bool>` cell.
    pub fn bind(mut self, value: &Value<bool>) -> Self {
        self.value = Some(value.clone());
        self
    }

    /// Optional callback fired when the checked state changes.
    pub fn on_change(mut self, f: impl Fn(bool) -> Out<S, E> + 'static) -> Self {
        self.on_change = Some(Box::new(f));
        self
    }

    fn toggle(&self) -> Out<S, E> {
        if let Some(value) = &self.value {
            let new_val = !value.get();
            value.set(new_val);
            if let Some(cb) = &self.on_change {
                return cb(new_val);
            }
        }
        Out::Empty
    }

    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x as f64
            && x < (self.x + self.width) as f64
            && y >= self.y as f64
            && y < (self.y + self.height) as f64
    }

    fn is_checked(&self) -> bool {
        self.value.as_ref().map_or(false, |v| v.get())
    }

    fn layout_icon(icon: &mut Option<Icon>, x: u32, y: u32, w: u32, h: u32, queue: &wgpu::Queue) {
        if let Some(icon) = icon {
            icon.width_px = w;
            icon.height_px = h;
            icon.set_position(x, y, queue);
        }
    }
}

impl<S: 'static, E: Send + 'static> Layout for Checkbox<S, E> {
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

        Self::layout_icon(&mut self.checked_icon, x, y, w, h, queue);
        Self::layout_icon(&mut self.unchecked_icon, x, y, w, h, queue);
    }
}

impl<S: 'static, E: Send + 'static> GraphicsFlow<S, E> for Checkbox<S, E> {
    fn on_init(&mut self, ctx: &mut Context, _: &mut S) -> Out<S, E> {
        self.init(ctx);
        Out::Empty
    }

    fn on_update(&mut self, ctx: &Context, _state: &mut S, _dt: Duration) -> Out<S, E> {
        let pos = ctx.mouse.coords;
        let hovered = self.contains(pos.x, pos.y);
        let is_pressed = matches!(ctx.mouse.pressed, MouseButtonState::Left);

        if !self.event_driven {
            let clicked = self.was_pressed && !is_pressed && hovered;
            self.was_pressed = is_pressed && hovered;
            if clicked {
                return self.toggle();
            }
        }
        Out::Empty
    }

    fn on_window_events(
        &mut self,
        ctx: &Context,
        _: &mut S,
        event: &winit::event::WindowEvent,
    ) -> Out<S, E> {
        use winit::event::{MouseButton, WindowEvent};
        if let WindowEvent::MouseInput {
            button: MouseButton::Left,
            state: button,
            ..
        } = event
        {
            self.event_driven = true;
            let hovered = self.contains(ctx.mouse.coords.x, ctx.mouse.coords.y);
            if button.is_pressed() {
                self.was_pressed = hovered;
            } else {
                let clicked = std::mem::take(&mut self.was_pressed) && hovered;
                if clicked {
                    return self.toggle();
                }
            }
        } else if matches!(event, WindowEvent::Focused(false)) {
            self.was_pressed = false;
        }
        Out::Empty
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        if self.overlay_checked && self.is_checked() {
            return Render::Composed(
                [&self.unchecked_icon, &self.checked_icon].into_iter()
                    .filter_map(|icon| icon.as_ref())
                    .map(|icon| GraphicsFlow::<S, E>::on_render(icon))
                    .collect(),
            );
        }
        let icon = if self.is_checked() {
            &self.checked_icon
        } else {
            &self.unchecked_icon
        };
        match icon {
            Some(icon) => GraphicsFlow::<S, E>::on_render(icon),
            None => Render::None,
        }
    }
}
