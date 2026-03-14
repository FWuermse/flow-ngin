use std::sync::Arc;

use wgpu::{
    BufferUsages,
    util::{BufferInitDescriptor, DeviceExt},
};

use instant::Duration;

use crate::{
    context::Context,
    data_structures::texture::Texture,
    flow::{FlowConsturctor, GraphicsFlow, Out},
    pipelines::gui::{mk_bind_group, mk_bind_group_layout},
    render::{Flat, Render},
    ui::{
        background::{Background, BackgroundTexture},
        image::{Frame, pixels_to_ndc, vertices_from_coords},
        layout::{Layout, UIElement},
    },
};

fn merge_outs<S, E>(outs: impl Iterator<Item = Out<S, E>>) -> Out<S, E> {
    let mut events = Vec::new();
    let mut fns: Vec<Box<dyn std::future::Future<Output = Box<dyn FnOnce(&mut S)>>>> = Vec::new();
    for out in outs {
        match out {
            Out::FutEvent(mut v) => events.append(&mut v),
            Out::FutFn(mut v) => fns.append(&mut v),
            Out::Configure(_) | Out::Empty => {}
        }
    }
    if !events.is_empty() {
        Out::FutEvent(events)
    } else if !fns.is_empty() {
        Out::FutFn(fns)
    } else {
        Out::Empty
    }
}

/// Backing GPU resources for the container background quad.
enum BgSource {
    Color(wgpu::BindGroup),
    Texture(Arc<BackgroundTexture>),
}

struct BgResources {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    source: BgSource,
}

impl BgResources {
    fn bind_group(&self) -> &wgpu::BindGroup {
        match &self.source {
            BgSource::Color(bg) => bg,
            BgSource::Texture(arc) => &arc.bind_group,
        }
    }
}

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
    // Absolute screen position (computed by resolve or same as local for root)
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    // Position/size relative to parent, set at construction
    local_x: u32,
    local_y: u32,
    local_w: u32,
    local_h: u32,
    // Screen dimensions for NDC conversion in resolve
    screen_width: u32,
    screen_height: u32,
    children: Vec<Box<dyn UIElement<S, E>>>,
    background: Option<Background>,
    bg_resources: Option<BgResources>,
}

impl<S: 'static, E: 'static> Container<S, E> {
    /// Create a container at absolute pixel position `(x, y)` with the given dimensions.
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            local_x: x,
            local_y: y,
            local_w: width,
            local_h: height,
            screen_width: 0,
            screen_height: 0,
            children: Vec::new(),
            background: None,
            bg_resources: None,
        }
    }

    /// Add a child element. Any type implementing both [`GraphicsFlow`] and [`Layout`] is accepted.
    pub fn with_child(mut self, child: impl UIElement<S, E> + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    /// Set a solid-colour background for this container.
    pub fn with_background_color(mut self, rgba: [u8; 4]) -> Self {
        self.background = Some(Background::Color(rgba));
        self
    }

    /// Set a custom container texture as background.
    pub fn with_background_texture(mut self, texture: Arc<BackgroundTexture>) -> Self {
        self.background = Some(Background::Texture(texture));
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
            // TODO: find a way to limit the heavy boxing in general
            Box::pin(async move { Box::new(self) as Box<dyn GraphicsFlow<S, E>> })
        })
    }
}

impl<S: 'static, E: 'static> GraphicsFlow<S, E> for Container<S, E> {
    fn on_init(&mut self, ctx: &mut Context, state: &mut S) -> Out<S, E> {
        self.screen_width = ctx.config.width;
        self.screen_height = ctx.config.height;

        for child in &mut self.children {
            child.on_init(ctx, state);
        }

        if let Some(bg) = &self.background {
            let source = match bg {
                Background::Color(rgba) => {
                    let tex = Texture::from_color(*rgba, &ctx.device, &ctx.queue);
                    let layout = mk_bind_group_layout(&ctx.device);
                    let bind_group = mk_bind_group(&ctx.device, &tex, &layout);
                    BgSource::Color(bind_group)
                }
                Background::Texture(arc) => BgSource::Texture(Arc::clone(arc)),
            };

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
                label: Some("Container BG Vertex Buffer"),
                contents: bytemuck::cast_slice(&vertices),
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            });
            let indices: &[u16] = &[0, 1, 3, 1, 2, 3];
            let index_buffer = ctx.device.create_buffer_init(&BufferInitDescriptor {
                label: Some("Container BG Index Buffer"),
                contents: bytemuck::cast_slice(indices),
                usage: BufferUsages::INDEX,
            });
            self.bg_resources = Some(BgResources {
                vertex_buffer,
                index_buffer,
                source,
            });
        }

        self.resolve(&ctx.queue);
        Out::Empty
    }

    fn on_update(&mut self, ctx: &Context, state: &mut S, dt: Duration) -> Out<S, E> {
        merge_outs(self.children.iter_mut().map(|c| c.on_update(ctx, state, dt)))
    }

    fn on_click(&mut self, ctx: &Context, state: &mut S, id: u32) -> Out<S, E> {
        merge_outs(self.children.iter_mut().map(|c| c.on_click(ctx, state, id)))
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        let mut renders: Vec<Render<'_, 'pass>> = Vec::new();

        if let Some(bg) = &self.bg_resources {
            renders.push(Render::GUI(Flat {
                vertex: &bg.vertex_buffer,
                index: &bg.index_buffer,
                group: bg.bind_group(),
                amount: 6,
                id: 0,
            }));
        }

        for child in &self.children {
            renders.push(child.on_render());
        }

        Render::Composed(renders)
    }
}

impl<S: 'static, E: 'static> Layout for Container<S, E> {
    /// Offset the container by the parent's origin and re-resolve all children.
    fn resolve(&mut self, parent_x: u32, parent_y: u32, _parent_w: u32, _parent_h: u32, queue: &wgpu::Queue) {
        self.x = parent_x + self.local_x;
        self.y = parent_y + self.local_y;
        self.width = self.local_w;
        self.height = self.local_h;

        // Update the background vertex buffer to match the new absolute position.
        if let Some(bg) = &self.bg_resources {
            let screen_pos = pixels_to_ndc(
                self.x, self.y, self.width, self.height,
                self.screen_width, self.screen_height,
            );
            let full_tex = Frame { start_x: 0.0, start_y: 0.0, end_x: 1.0, end_y: 1.0 };
            let vertices = vertices_from_coords(&screen_pos, &full_tex);
            queue.write_buffer(&bg.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        }

        self.resolve(queue);
    }
}
