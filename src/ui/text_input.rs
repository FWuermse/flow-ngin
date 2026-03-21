use instant::Duration;

use winit::event::{ElementState, WindowEvent};
use winit::keyboard::Key;

use crate::{
    context::{Context, MouseButtonState},
    flow::{GraphicsFlow, Out},
    render::Render,
    ui::{
        HAlign, Placement, VAlign,
        image::Icon,
        layout::Layout,
        text_label::TextLabel,
        value::Value,
    },
};

const CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(500);
const CURSOR_WIDTH_PX: u32 = 2;

/// A single-line text input that binds to a `Value<String>`.
///
/// Click to focus, click outside to unfocus. When focused, keyboard events
/// are handled in `on_window_events`.
///
/// # Example
///
/// ```ignore
/// use flow_ngin::ui::{text_input::TextInput, value::Value};
///
/// let username = Value::new(String::new());
///
/// let input = TextInput::<State, Event>::new()
///     .width(200)
///     .height(32)
///     .background(bg_icon)
///     .bind(&username);
/// ```
pub struct TextInput<S, E> {
    placement: Placement,
    x: u32,
    y: u32,
    width: u32,
    height: u32,

    background: Option<Icon>,
    label: TextLabel,
    cursor: Option<Icon>,
    cursor_visible: bool,
    cursor_timer: Duration,

    text: String,
    cursor_pos: usize,
    focused: bool,
    was_pressed: bool,

    value: Option<Value<String>>,
    on_change: Option<Box<dyn Fn(&str) -> Out<S, E>>>,
    on_submit: Option<Box<dyn Fn(&str) -> Out<S, E>>>,
}

impl<S: 'static, E: 'static> TextInput<S, E> {
    pub fn new() -> Self {
        Self {
            placement: Placement::default(),
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            background: None,
            label: TextLabel::new(""),
            cursor: None,
            cursor_visible: true,
            cursor_timer: Duration::from_millis(0),
            text: String::new(),
            cursor_pos: 0,
            focused: false,
            was_pressed: false,
            value: None,
            on_change: None,
            on_submit: None,
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

    /// Set the background icon (e.g. `Icon::from_color` for a solid fill).
    pub fn background(mut self, icon: Icon) -> Self {
        self.background = Some(icon);
        self
    }

    /// Set the cursor icon (thin vertical bar). If not set, a white 2px bar is used
    /// after `on_init` creates it.
    pub fn cursor_icon(mut self, icon: Icon) -> Self {
        self.cursor = Some(icon);
        self
    }

    pub fn font_size(mut self, size: f32) -> Self {
        self.label = self.label.font_size(size);
        self
    }

    pub fn text_color(mut self, color: [u8; 3]) -> Self {
        self.label = self.label.color(color);
        self
    }

    /// Bind this text input to a `Value<String>` cell.
    pub fn bind(mut self, value: &Value<String>) -> Self {
        self.value = Some(value.clone());
        self
    }

    /// Callback fired when the text changes.
    pub fn on_change(mut self, f: impl Fn(&str) -> Out<S, E> + 'static) -> Self {
        self.on_change = Some(Box::new(f));
        self
    }

    /// Callback fired when Enter is pressed.
    pub fn on_submit(mut self, f: impl Fn(&str) -> Out<S, E> + 'static) -> Self {
        self.on_submit = Some(Box::new(f));
        self
    }

    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x as f64
            && x < (self.x + self.width) as f64
            && y >= self.y as f64
            && y < (self.y + self.height) as f64
    }

    fn layout_background(&mut self, queue: &wgpu::Queue) {
        if let Some(bg) = &mut self.background {
            bg.width_px = self.width;
            bg.height_px = self.height;
            bg.set_position(self.x, self.y, queue);
        }
    }

    fn layout_label(&mut self, queue: &wgpu::Queue) {
        Layout::resolve(
            &mut self.label,
            self.x,
            self.y,
            self.width,
            self.height,
            queue,
        );
    }

    fn layout_cursor(&mut self, queue: &wgpu::Queue) {
        if let Some(cursor) = &mut self.cursor {
            let cursor_x =
                self.x + self.label.cursor_x_for_byte_pos(self.cursor_pos) as u32;
            let cursor_h = (self.label.get_line_height() as u32).min(self.height);

            cursor.width_px = CURSOR_WIDTH_PX;
            cursor.height_px = cursor_h;
            cursor.set_position(cursor_x, self.y, queue);
        }
    }

    fn text_changed(&mut self, queue: &wgpu::Queue) {
        self.label.set_text(&self.text);
        if let Some(value) = &self.value {
            value.set(self.text.clone());
        }
        self.layout_cursor(queue);
        self.cursor_visible = true;
        self.cursor_timer = Duration::from_millis(0);
    }

    /// Move cursor left by one character boundary.
    fn move_cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            let mut pos = self.cursor_pos - 1;
            while pos > 0 && !self.text.is_char_boundary(pos) {
                pos -= 1;
            }
            self.cursor_pos = pos;
        }
    }

    /// Move cursor right by one character boundary.
    fn move_cursor_right(&mut self) {
        if self.cursor_pos < self.text.len() {
            let mut pos = self.cursor_pos + 1;
            while pos < self.text.len() && !self.text.is_char_boundary(pos) {
                pos += 1;
            }
            self.cursor_pos = pos;
        }
    }
}

impl<S: 'static, E: 'static> Layout for TextInput<S, E> {
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

        self.layout_background(queue);
        self.layout_label(queue);
        self.layout_cursor(queue);
    }
}

impl<S: 'static, E: 'static> GraphicsFlow<S, E> for TextInput<S, E> {
    fn on_init(&mut self, ctx: &mut Context, _: &mut S) -> Out<S, E> {
        let (x, y, w, h) = self.placement.resolve(0, 0, ctx.config.width, ctx.config.height);
        self.x = x;
        self.y = y;
        self.width = w;
        self.height = h;

        if self.label.get_line_height() > self.height as f32 {
            self.label = std::mem::replace(&mut self.label, TextLabel::new(""))
                .line_height(self.height as f32);
        }
        self.label.init(ctx);
        self.layout_label(&ctx.queue);

        self.layout_background(&ctx.queue);

        if self.cursor.is_none() {
            self.cursor = Some(Icon::from_color(ctx, [255, 255, 255, 255]));
        }
        self.layout_cursor(&ctx.queue);

        if let Some(value) = &self.value {
            let initial = value.get();
            if !initial.is_empty() {
                self.text = initial;
                self.cursor_pos = self.text.len();
                self.label.set_text(&self.text);
                self.layout_cursor(&ctx.queue);
            }
        }

        Out::Empty
    }

    fn on_update(&mut self, ctx: &Context, _state: &mut S, dt: Duration) -> Out<S, E> {
        let pos = ctx.mouse.coords;
        let is_pressed = matches!(ctx.mouse.pressed, MouseButtonState::Left);
        let clicked = self.was_pressed && !is_pressed;
        self.was_pressed = is_pressed;

        if clicked {
            self.focused = self.contains(pos.x, pos.y);
            self.cursor_visible = true;
            self.cursor_timer = Duration::from_millis(0);
        }

        if self.focused {
            self.cursor_timer += dt;
            if self.cursor_timer >= CURSOR_BLINK_INTERVAL {
                self.cursor_timer -= CURSOR_BLINK_INTERVAL;
                self.cursor_visible = !self.cursor_visible;
            }
        }

        Out::Empty
    }

    fn on_window_events(&mut self, ctx: &Context, _state: &mut S, event: &WindowEvent) -> Out<S, E> {
        if !self.focused {
            return Out::Empty;
        }

        if let WindowEvent::KeyboardInput { event, .. } = event {
            if event.state != ElementState::Pressed {
                return Out::Empty;
            }

            match &event.logical_key {
                Key::Named(named) => {
                    use winit::keyboard::NamedKey;
                    match named {
                        NamedKey::Backspace => {
                            if self.cursor_pos > 0 {
                                let old_pos = self.cursor_pos;
                                self.move_cursor_left();
                                self.text.drain(self.cursor_pos..old_pos);
                                self.text_changed(&ctx.queue);
                                if let Some(cb) = &self.on_change {
                                    return cb(&self.text);
                                }
                            }
                        }
                        NamedKey::Delete => {
                            if self.cursor_pos < self.text.len() {
                                let mut end = self.cursor_pos + 1;
                                while end < self.text.len() && !self.text.is_char_boundary(end) {
                                    end += 1;
                                }
                                self.text.drain(self.cursor_pos..end);
                                self.text_changed(&ctx.queue);
                                if let Some(cb) = &self.on_change {
                                    return cb(&self.text);
                                }
                            }
                        }
                        NamedKey::ArrowLeft => {
                            self.move_cursor_left();
                            self.layout_cursor(&ctx.queue);
                            self.cursor_visible = true;
                            self.cursor_timer = Duration::from_millis(0);
                        }
                        NamedKey::ArrowRight => {
                            self.move_cursor_right();
                            self.layout_cursor(&ctx.queue);
                            self.cursor_visible = true;
                            self.cursor_timer = Duration::from_millis(0);
                        }
                        NamedKey::Home => {
                            self.cursor_pos = 0;
                            self.layout_cursor(&ctx.queue);
                            self.cursor_visible = true;
                            self.cursor_timer = Duration::from_millis(0);
                        }
                        NamedKey::End => {
                            self.cursor_pos = self.text.len();
                            self.layout_cursor(&ctx.queue);
                            self.cursor_visible = true;
                            self.cursor_timer = Duration::from_millis(0);
                        }
                        NamedKey::Enter => {
                            if let Some(cb) = &self.on_submit {
                                return cb(&self.text);
                            }
                        }
                        NamedKey::Space => {
                            self.text.insert(self.cursor_pos, ' ');
                            self.cursor_pos += 1;
                            self.text_changed(&ctx.queue);
                            if let Some(cb) = &self.on_change {
                                return cb(&self.text);
                            }
                        }
                        _ => {}
                    }
                }
                Key::Character(_) => {
                    if let Some(text) = &event.text {
                        let s = text.as_str();
                        if !s.chars().all(|c| c.is_control()) {
                            self.text.insert_str(self.cursor_pos, s);
                            self.cursor_pos += s.len();
                            self.text_changed(&ctx.queue);
                            if let Some(cb) = &self.on_change {
                                return cb(&self.text);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        Out::Empty
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        let mut renders = Vec::new();

        if let Some(bg) = &self.background {
            renders.push(GraphicsFlow::<S, E>::on_render(bg));
        }

        renders.push(self.label.render());

        if self.focused && self.cursor_visible {
            if let Some(cursor) = &self.cursor {
                renders.push(GraphicsFlow::<S, E>::on_render(cursor));
            }
        }

        Render::Composed(renders)
    }
}
