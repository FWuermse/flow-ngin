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
        value::Value,
    },
};

/// A togglable checkbox that binds to a `Value<bool>`.
///
/// # Example
///
/// ```no_run
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
pub struct Checkbox<S, E> {
    placement: Placement,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    screen_width: u32,
    screen_height: u32,
    checked_icon: Option<Icon>,
    unchecked_icon: Option<Icon>,
    value: Option<Value<bool>>,
    on_change: Option<Box<dyn Fn(bool) -> Out<S, E>>>,
    was_pressed: bool,
    _marker: PhantomData<S>,
}

impl<S: 'static, E: 'static> Checkbox<S, E> {
    pub fn new() -> Self {
        Self {
            placement: Placement::default(),
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            screen_width: 0,
            screen_height: 0,
            checked_icon: None,
            unchecked_icon: None,
            value: None,
            on_change: None,
            was_pressed: false,
            _marker: PhantomData,
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
        self.checked_icon = Some(icon);
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

impl<S: 'static, E: 'static> Layout for Checkbox<S, E> {
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

        Self::layout_icon(&mut self.checked_icon, x, y, w, h, queue);
        Self::layout_icon(&mut self.unchecked_icon, x, y, w, h, queue);
    }
}

impl<S: 'static, E: 'static> GraphicsFlow<S, E> for Checkbox<S, E> {
    fn on_init(&mut self, ctx: &mut Context, _: &mut S) -> Out<S, E> {
        self.screen_width = ctx.config.width;
        self.screen_height = ctx.config.height;

        let (x, y, w, h) = self.placement.resolve(0, 0, ctx.config.width, ctx.config.height);
        self.x = x;
        self.y = y;
        self.width = w;
        self.height = h;

        Self::layout_icon(&mut self.checked_icon, x, y, w, h, &ctx.queue);
        Self::layout_icon(&mut self.unchecked_icon, x, y, w, h, &ctx.queue);
        Out::Empty
    }

    fn on_update(&mut self, ctx: &Context, _state: &mut S, _dt: Duration) -> Out<S, E> {
        let pos = ctx.mouse.coords;
        let hovered = self.contains(pos.x, pos.y);
        let is_pressed = matches!(ctx.mouse.pressed, MouseButtonState::Left);

        let clicked = self.was_pressed && !is_pressed && hovered;
        self.was_pressed = is_pressed && hovered;

        if clicked {
            if let Some(value) = &self.value {
                let new_val = !value.get();
                value.set(new_val);
                if let Some(cb) = &self.on_change {
                    return cb(new_val);
                }
            }
        }
        Out::Empty
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
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
