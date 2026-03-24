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

/// A draggable slider that binds to a `Value<f32>` in the range `[0.0, 1.0]`.
///
/// The handle is always square (side = component height). Provide separate
/// `handle` and `active_handle` icons for idle vs dragging visuals, similar
/// to `Button`'s `fill` / `click_fill`.
///
/// # Example
///
/// ```ignore
/// use flow_ngin::ui::{slider::Slider, value::Value};
///
/// let volume = Value::new(0.5f32);
///
/// let slider = Slider::<State, Event>::new()
///     .width(200)
///     .height(24)
///     .track(track_icon)
///     .handle(idle_icon)
///     .active_handle(dragging_icon)
///     .bind(&volume);
/// ```
pub struct Slider<S, E: Send> {
    placement: Placement,
    x: u32,
    y: u32,
    width: u32,
    height: u32,

    track: Option<Icon>,
    handle: Option<Icon>,
    active_handle: Option<Icon>,
    track_height: u32,

    value: Option<Value<f32>>,
    on_change: Option<Box<dyn Fn(f32) -> Out<S, E>>>,
    dragging: bool,
}

impl<S: 'static, E: Send + 'static> Slider<S, E> {
    pub fn new() -> Self {
        Self {
            placement: Placement::default(),
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            track: None,
            handle: None,
            active_handle: None,
            track_height: 4,
            value: None,
            on_change: None,
            dragging: false,
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

    /// Set the track icon (background bar).
    pub fn track(mut self, icon: Icon) -> Self {
        self.track = Some(icon);
        self
    }

    /// Set the handle icon for the idle state.
    pub fn handle(mut self, icon: Icon) -> Self {
        self.handle = Some(icon);
        self
    }

    /// Set the handle icon shown while dragging.
    pub fn active_handle(mut self, icon: Icon) -> Self {
        self.active_handle = Some(icon);
        self
    }

    /// Height of the track bar in pixels (default: 4).
    pub fn track_height(mut self, h: u32) -> Self {
        self.track_height = h;
        self
    }

    /// Bind this slider to a `Value<f32>` cell.
    pub fn bind(mut self, value: &Value<f32>) -> Self {
        self.value = Some(value.clone());
        self
    }

    /// Optional callback fired when the slider value changes.
    pub fn on_change(mut self, f: impl Fn(f32) -> Out<S, E> + 'static) -> Self {
        self.on_change = Some(Box::new(f));
        self
    }

    /// Handle side length (square, equals component height).
    fn handle_size(&self) -> u32 {
        self.height
    }

    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x as f64
            && x < (self.x + self.width) as f64
            && y >= self.y as f64
            && y < (self.y + self.height) as f64
    }

    fn current_value(&self) -> f32 {
        self.value.as_ref().map_or(0.0, |v| v.get())
    }

    fn layout_track(&mut self, queue: &wgpu::Queue) {
        if let Some(track) = &mut self.track {
            track.width_px = self.width;
            track.height_px = self.track_height;
            let track_y = self.y + (self.height.saturating_sub(self.track_height)) / 2;
            track.set_position(self.x, track_y, queue);
        }
    }

    fn layout_handle_icon(icon: &mut Icon, x: u32, y: u32, size: u32, queue: &wgpu::Queue) {
        icon.width_px = size;
        icon.height_px = size;
        icon.set_position(x, y, queue);
    }

    fn layout_handles(&mut self, queue: &wgpu::Queue) {
        let val = self.current_value();
        let size = self.handle_size();
        let usable = self.width.saturating_sub(size);
        let handle_x = self.x + (val * usable as f32) as u32;

        if let Some(handle) = &mut self.handle {
            Self::layout_handle_icon(handle, handle_x, self.y, size, queue);
        }
        if let Some(active) = &mut self.active_handle {
            Self::layout_handle_icon(active, handle_x, self.y, size, queue);
        }
    }

    fn value_from_mouse(&self, mouse_x: f64) -> f32 {
        let size = self.handle_size();
        let usable = self.width.saturating_sub(size) as f64;
        if usable <= 0.0 {
            return 0.0;
        }
        let offset = mouse_x - self.x as f64 - size as f64 / 2.0;
        (offset / usable).clamp(0.0, 1.0) as f32
    }
}

impl<S: 'static, E: Send + 'static> Layout for Slider<S, E> {
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

        self.layout_track(queue);
        self.layout_handles(queue);
    }
}

impl<S: 'static, E: Send + 'static> GraphicsFlow<S, E> for Slider<S, E> {
    fn on_init(&mut self, ctx: &mut Context, _: &mut S) -> Out<S, E> {
        let (x, y, w, h) = self.placement.resolve(0, 0, ctx.config.width, ctx.config.height);
        self.x = x;
        self.y = y;
        self.width = w;
        self.height = h;

        self.layout_track(&ctx.queue);
        self.layout_handles(&ctx.queue);
        Out::Empty
    }

    fn on_update(&mut self, ctx: &Context, _state: &mut S, _dt: Duration) -> Out<S, E> {
        let pos = ctx.mouse.coords;
        let is_pressed = matches!(ctx.mouse.pressed, MouseButtonState::Left);

        if is_pressed && !self.dragging && self.contains(pos.x, pos.y) {
            self.dragging = true;
        }
        if !is_pressed {
            self.dragging = false;
        }

        if self.dragging {
            let new_val = self.value_from_mouse(pos.x);
            let old_val = self.current_value();

            if (new_val - old_val).abs() > f32::EPSILON {
                if let Some(value) = &self.value {
                    value.set(new_val);
                }
                self.layout_handles(&ctx.queue);

                if let Some(cb) = &self.on_change {
                    return cb(new_val);
                }
            }
        }

        Out::Empty
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        let track_render = match &self.track {
            Some(icon) => GraphicsFlow::<S, E>::on_render(icon),
            None => Render::None,
        };
        let handle_render = if self.dragging {
            match &self.active_handle {
                Some(icon) => GraphicsFlow::<S, E>::on_render(icon),
                None => match &self.handle {
                    Some(icon) => GraphicsFlow::<S, E>::on_render(icon),
                    None => Render::None,
                },
            }
        } else {
            match &self.handle {
                Some(icon) => GraphicsFlow::<S, E>::on_render(icon),
                None => Render::None,
            }
        };
        Render::Composed(vec![track_render, handle_render])
    }
}
