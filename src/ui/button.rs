use std::marker::PhantomData;

use instant::Duration;
use wgpu::{
    BufferUsages,
    util::{BufferInitDescriptor, DeviceExt},
};

use crate::{
    context::{Context, MouseButtonState},
    data_structures::texture::Texture,
    flow::{GraphicsFlow, Out},
    pipelines::gui::{mk_bind_group, mk_bind_group_layout},
    render::{Flat, Render},
    ui::{
        image::{Frame, Icon, pixels_to_ndc, vertices_from_coords},
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

struct ButtonBgResources {
    vertex_buffer: wgpu::Buffer, // shared across all three states
    index_buffer: wgpu::Buffer,
    normal: wgpu::BindGroup,
    hover: wgpu::BindGroup,
    pressed: wgpu::BindGroup,
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
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    content: Option<ButtonContent>,
    normal_color: [u8; 4],
    hover_color: [u8; 4],
    pressed_color: [u8; 4],
    on_click_fn: Option<Box<dyn Fn() -> E + 'static>>,
    visual_state: VisualState,
    resources: Option<ButtonBgResources>,
    _marker: PhantomData<fn(S, E)>,
}

impl<S: 'static, E: 'static> Button<S, E> {
    /// Create a button at absolute pixel position `(x, y)` with a unique pick `id`.
    ///
    /// The `id` must be non-zero and unique across all pickable objects in the scene.
    pub fn new(id: u32, x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            id,
            x,
            y,
            width,
            height,
            content: None,
            normal_color: [80, 80, 80, 255],
            hover_color: [110, 110, 110, 255],
            pressed_color: [50, 50, 50, 255],
            on_click_fn: None,
            visual_state: VisualState::Normal,
            resources: None,
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

    pub fn normal_color(mut self, rgba: [u8; 4]) -> Self {
        self.normal_color = rgba;
        self
    }

    pub fn hover_color(mut self, rgba: [u8; 4]) -> Self {
        self.hover_color = rgba;
        self
    }

    pub fn pressed_color(mut self, rgba: [u8; 4]) -> Self {
        self.pressed_color = rgba;
        self
    }

    /// Register a callback that produces an event `E` when the button is clicked.
    pub fn on_click(mut self, f: impl Fn() -> E + 'static) -> Self {
        self.on_click_fn = Some(Box::new(f));
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
                let ix = self.x + self.width.saturating_sub(icon.width_px) / 2;
                let iy = self.y + self.height.saturating_sub(icon.height_px) / 2;
                icon.set_position(ix, iy, queue);
            }
            Some(ButtonContent::Text(label)) => {
                // Centre vertically with a small horizontal inset.
                const INSET: u32 = 6;
                label.resolve(
                    (self.x + INSET) as f32,
                    self.y as f32,
                    self.width.saturating_sub(2 * INSET) as f32,
                    self.height as f32,
                );
            }
            None => {}
        }
    }

    fn mk_bind_group_for(
        &self,
        rgba: [u8; 4],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> wgpu::BindGroup {
        let tex = Texture::from_color(rgba, device, queue);
        let layout = mk_bind_group_layout(device);
        mk_bind_group(device, &tex, &layout)
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
        self.x = parent_x;
        self.y = parent_y;
        self.width = parent_w;
        self.height = parent_h;
        self.layout_content(queue);
    }
}

impl<S: 'static, E: 'static> GraphicsFlow<S, E> for Button<S, E> {
    fn on_init(&mut self, ctx: &mut Context, _: &mut S) -> Out<S, E> {
        // Init content GPU resources.
        match &mut self.content {
            Some(ButtonContent::Text(label)) => label.init(ctx),
            Some(ButtonContent::Icon(_)) | None => {}
        }

        // Build three 1×1 textures similar to default normal map.
        let normal = self.mk_bind_group_for(self.normal_color, &ctx.device, &ctx.queue);
        let hover = self.mk_bind_group_for(self.hover_color, &ctx.device, &ctx.queue);
        let pressed = self.mk_bind_group_for(self.pressed_color, &ctx.device, &ctx.queue);

        let screen_pos = pixels_to_ndc(
            self.x,
            self.y,
            self.width,
            self.height,
            ctx.config.width,
            ctx.config.height,
        );
        let full_tex = Frame {
            start_x: 0.0,
            start_y: 0.0,
            end_x: 1.0,
            end_y: 1.0,
        };
        let vertices = vertices_from_coords(&screen_pos, &full_tex);
        let vertex_buffer = ctx.device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Button Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: BufferUsages::VERTEX,
        });
        let indices: &[u16] = &[0, 1, 3, 1, 2, 3];
        let index_buffer = ctx.device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Button Index Buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: BufferUsages::INDEX,
        });

        self.resources = Some(ButtonBgResources {
            vertex_buffer,
            index_buffer,
            normal,
            hover,
            pressed,
        });

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
        let Some(res) = &self.resources else {
            return Render::None;
        };
        let bind_group = match self.visual_state {
            VisualState::Normal => &res.normal,
            VisualState::Hovered => &res.hover,
            VisualState::Pressed => &res.pressed,
        };

        let bg = Render::GUI(Flat {
            vertex: &res.vertex_buffer,
            index: &res.index_buffer,
            group: bind_group,
            amount: 6,
            id: self.id,
        });

        match &self.content {
            Some(content) => {
                let content_render = match content {
                    ButtonContent::Text(label) => label.render(),
                    ButtonContent::Icon(icon) => GraphicsFlow::<S, E>::on_render(icon),
                };
                Render::Composed(vec![bg, content_render])
            }
            None => bg,
        }
    }
}
