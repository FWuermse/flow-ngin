use std::sync::Arc;

use wgpu::{
    BufferUsages,
    util::{BufferInitDescriptor, DeviceExt},
};

use instant::Duration;
use winit::event::WindowEvent;

use crate::{
    context::Context,
    data_structures::texture::Texture,
    flow::{FlowConsturctor, GraphicsFlow, Out},
    pipelines::gui::{mk_bind_group, mk_bind_group_layout},
    render::{Flat, Render},
    ui::{
        HAlign, Placement, VAlign,
        background::{Background, BackgroundTexture},
        image::{Frame, pixels_to_frame, vertices_from_coords},
        layout::{Layout, UIElement},
    },
};

pub(crate) fn merge_outs<S, E>(outs: impl Iterator<Item = Out<S, E>>) -> Out<S, E> {
    let mut events = Vec::new();
    let mut fns: Vec<Box<dyn std::future::Future<Output = Box<dyn FnOnce(&mut S)>>>> = Vec::new();
    let mut configs: Vec<Box<dyn FnOnce(&mut Context)>> = Vec::new();
    for out in outs {
        match out {
            Out::FutEvent(mut v) => events.append(&mut v),
            Out::FutFn(mut v) => fns.append(&mut v),
            Out::Configure(f) => configs.push(f),
            Out::Empty => {}
        }
    }
    if !events.is_empty() {
        Out::FutEvent(events)
    } else if !fns.is_empty() {
        Out::FutFn(fns)
    } else if !configs.is_empty() {
        Out::Configure(Box::new(|ctx| {
            for f in configs {
                f(ctx);
            }
        }))
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
/// let icon = Icon::new(ctx, atlas, 17)
///     .halign(HAlign::Center)
///     .valign(VAlign::Center);
///
/// let container = Container::<State, Event>::new()
///     .width(ctx.config.width)
///     .height(ctx.config.height)
///     .with_child(icon)
///     .with_child(TextLabel::new("Score: 0").position(16.0, 16.0));
/// ```
pub struct Container<S, E> {
    placement: Placement,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    children: Vec<Box<dyn UIElement<S, E>>>,
    background: Option<Background>,
    bg_resources: Option<BgResources>,
    pick_id: u32,
}

impl<S: 'static, E: 'static> Container<S, E> {
    /// Create a container that fills its parent by default.
    ///
    /// Use `.width()`/`.height()` for explicit sizes, `.halign()`/`.valign()` for alignment.
    pub fn new() -> Self {
        Self {
            placement: Placement::default(),
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            children: Vec::new(),
            background: None,
            bg_resources: None,
            pick_id: 0,
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
    pub fn with_background_texture(mut self, texture: &Arc<BackgroundTexture>) -> Self {
        self.background = Some(Background::Texture(Arc::clone(&texture)));
        self
    }

    /// Make this container a click shield with the given pick ID.
    ///
    /// A non-zero pick ID causes the background quad to absorb GPU picks,
    /// preventing 3D objects behind this container from receiving `on_click`.
    pub fn clickable(mut self, pick_id: u32) -> Self {
        self.pick_id = pick_id;
        self
    }

    /// Add a child at runtime. The child must already be initialized via `on_init`.
    pub fn add_child(&mut self, child: impl UIElement<S, E> + 'static) {
        self.children.push(Box::new(child));
    }

    /// Remove a child by index. Returns `None` if out of bounds.
    pub fn remove_child(&mut self, index: usize) -> Option<Box<dyn UIElement<S, E>>> {
        if index < self.children.len() {
            Some(self.children.remove(index))
        } else {
            None
        }
    }

    pub fn clear_children(&mut self) {
        self.children.clear();
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
        // Resolve own placement against screen dimensions.
        // For nested containers, the parent's Layout::resolve will override afterward.
        let (x, y, w, h) = self.placement.resolve(0, 0, ctx.config.width, ctx.config.height);
        self.x = x;
        self.y = y;
        self.width = w;
        self.height = h;

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

            let screen_pos = pixels_to_frame(self.x, self.y, self.width, self.height);
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

    fn on_window_events(&mut self, ctx: &Context, state: &mut S, event: &WindowEvent) -> Out<S, E> {
        merge_outs(self.children.iter_mut().map(|c| c.on_window_events(ctx, state, event)))
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        let mut renders: Vec<Render<'_, 'pass>> = Vec::new();

        if let Some(bg) = &self.bg_resources {
            renders.push(Render::GUI(Flat {
                vertex: &bg.vertex_buffer,
                index: &bg.index_buffer,
                group: bg.bind_group(),
                amount: 6,
                id: self.pick_id,
            }));
        }

        for child in &self.children {
            renders.push(child.on_render());
        }

        Render::Composed(renders)
    }
}

impl<S: 'static, E: 'static> Layout for Container<S, E> {
    /// Resolve the container's position from parent bounds and re-resolve all children.
    fn resolve(&mut self, parent_x: u32, parent_y: u32, parent_w: u32, parent_h: u32, queue: &wgpu::Queue) {
        let (x, y, w, h) = self.placement.resolve(parent_x, parent_y, parent_w, parent_h);
        self.x = x;
        self.y = y;
        self.width = w;
        self.height = h;

        // Update the background vertex buffer to match the new absolute position.
        if let Some(bg) = &self.bg_resources {
            let screen_pos = pixels_to_frame(self.x, self.y, self.width, self.height);
            let full_tex = Frame { start_x: 0.0, start_y: 0.0, end_x: 1.0, end_y: 1.0 };
            let vertices = vertices_from_coords(&screen_pos, &full_tex);
            queue.write_buffer(&bg.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        }

        self.resolve(queue);
    }
}
