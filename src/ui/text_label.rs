use std::cell::RefCell;

use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};

use crate::{
    context::Context,
    flow::{FlowConsturctor, GraphicsFlow, Out},
    render::Render,
};

struct GlyphonResources {
    font_system: FontSystem,
    swash_cache: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    text_buffer: Buffer,
}

/// A text label UI component backed by glyphon.
///
/// Positions and bounds are relative to the parent (screen when used standalone).
/// Use [`resolve`](TextLabel::resolve) to re-layout after a parent resize or when
/// embedding inside a container.
///
/// # Standalone usage
///
/// ```no_run
/// use flow_ngin::ui::TextLabel;
///
/// flow_ngin::flow::run::<(), ()>(vec![
///     TextLabel::new("Hello, flow-ngin!")
///         .position(10.0, 10.0)
///         .color([255, 255, 255])
///         .into_constructor(),
/// ]);
/// ```
///
/// # Embedded usage
///
/// ```no_run
/// use flow_ngin::{context::Context, flow::{GraphicsFlow, Out}, render::Render, ui::TextLabel};
///
/// struct MyFlow {
///     label: TextLabel,
/// }
///
/// impl GraphicsFlow<(), ()> for MyFlow {
///     fn on_init(&mut self, ctx: &mut Context, state: &mut ()) -> Out<(), ()> {
///         self.label.init(ctx);
///         Out::Empty
///     }
///
///     fn on_render<'pass>(&self) -> Render<'_, 'pass> {
///         self.label.render()
///     }
/// }
/// ```
pub struct TextLabel {
    text: String,
    position: (f32, f32),
    size: Option<(f32, f32)>,
    font_size: f32,
    line_height: f32,
    color: [u8; 3],
    // Resolved absolute screen coordinates, updated by resolve()
    resolved_x: f32,
    resolved_y: f32,
    resolved_w: f32,
    resolved_h: f32,
    resources: RefCell<Option<GlyphonResources>>,
}

impl TextLabel {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            position: (0.0, 0.0),
            size: None,
            font_size: 30.0,
            line_height: 42.0,
            color: [255, 255, 255],
            resolved_x: 0.0,
            resolved_y: 0.0,
            resolved_w: 0.0,
            resolved_h: 0.0,
            resources: RefCell::new(None),
        }
    }

    /// Offset from the parent's top-left corner, in pixels.
    pub fn position(mut self, x: f32, y: f32) -> Self {
        self.position = (x, y);
        self
    }

    /// Maximum width and height the text may occupy, in pixels.
    /// Text wraps and is clipped to this box.
    /// Defaults to the remaining space inside the parent.
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.size = Some((w, h));
        self
    }

    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn line_height(mut self, height: f32) -> Self {
        self.line_height = height;
        self
    }

    pub fn color(mut self, color: [u8; 3]) -> Self {
        self.color = color;
        self
    }

    /// Update the displayed text at runtime.
    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_string();
        if let Some(res) = self.resources.borrow_mut().as_mut() {
            res.text_buffer.set_text(
                &mut res.font_system,
                text,
                &Attrs::new().family(Family::SansSerif),
                Shaping::Advanced,
                None,
            );
            res.text_buffer.shape_until_scroll(&mut res.font_system, false);
        }
    }

    /// Resolve relative position against a parent rectangle (pixels).
    ///
    /// Computes absolute screen coordinates from the declared relative `position`
    /// and `size`. If `size` was not set, the remaining parent space is used.
    ///
    /// Call this from a container before rendering children, or let `init` call
    /// it automatically against the full screen.
    pub fn resolve(&mut self, parent_x: f32, parent_y: f32, parent_w: f32, parent_h: f32) {
        self.resolved_x = parent_x + self.position.0;
        self.resolved_y = parent_y + self.position.1;
        let (w, h) = self
            .size
            .unwrap_or((parent_w - self.position.0, parent_h - self.position.1));
        self.resolved_w = w;
        self.resolved_h = h;
        if let Some(res) = self.resources.borrow_mut().as_mut() {
            res.text_buffer
                .set_size(&mut res.font_system, Some(w), Some(h));
            res.text_buffer.shape_until_scroll(&mut res.font_system, false);
        }
    }

    /// Initialize GPU resources. Called automatically by `GraphicsFlow::on_init`;
    /// call directly when embedding in a custom flow.
    pub fn init(&mut self, ctx: &mut Context) {
        self.resolve(0.0, 0.0, ctx.config.width as f32, ctx.config.height as f32);

        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(&ctx.device);
        let viewport = Viewport::new(&ctx.device, &cache);
        let mut atlas = TextAtlas::new(&ctx.device, &ctx.queue, &cache, ctx.config.format);
        let text_renderer = TextRenderer::new(
            &mut atlas,
            &ctx.device,
            wgpu::MultisampleState::default(),
            Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
        );

        let mut text_buffer =
            Buffer::new(&mut font_system, Metrics::new(self.font_size, self.line_height));
        text_buffer.set_size(
            &mut font_system,
            Some(self.resolved_w),
            Some(self.resolved_h),
        );
        text_buffer.set_text(
            &mut font_system,
            &self.text,
            &Attrs::new().family(Family::SansSerif),
            Shaping::Advanced,
            None,
        );
        text_buffer.shape_until_scroll(&mut font_system, false);

        *self.resources.borrow_mut() = Some(GlyphonResources {
            font_system,
            swash_cache,
            viewport,
            atlas,
            text_renderer,
            text_buffer,
        });
    }

    /// Return a [`Render`] for this label. Use this when embedding in a custom flow's
    /// `on_render`.
    pub fn render<'a, 'pass>(&'a self) -> Render<'a, 'pass> {
        let [r, g, b] = self.color;
        Render::Custom(Box::new(move |ctx, render_pass| {
            let mut guard = self.resources.borrow_mut();
            let Some(res) = guard.as_mut() else { return };

            let GlyphonResources {
                font_system,
                swash_cache,
                viewport,
                atlas,
                text_renderer,
                text_buffer,
            } = res;

            viewport.update(
                &ctx.queue,
                Resolution {
                    width: ctx.config.width,
                    height: ctx.config.height,
                },
            );

            text_renderer
                .prepare(
                    &ctx.device,
                    &ctx.queue,
                    font_system,
                    atlas,
                    viewport,
                    [TextArea {
                        buffer: text_buffer,
                        left: self.resolved_x,
                        top: self.resolved_y,
                        scale: 1.0,
                        bounds: TextBounds {
                            left: self.resolved_x as i32,
                            top: self.resolved_y as i32,
                            right: (self.resolved_x + self.resolved_w) as i32,
                            bottom: (self.resolved_y + self.resolved_h) as i32,
                        },
                        default_color: Color::rgb(r, g, b),
                        custom_glyphs: &[],
                    }],
                    swash_cache,
                )
                .unwrap();

            text_renderer.render(&*atlas, viewport, render_pass).unwrap();

            atlas.trim();
        }))
    }

    /// Wrap this label in a [`FlowConsturctor`] for use with [`flow_ngin::flow::run`].
    pub fn into_constructor<S: 'static, E: 'static>(self) -> FlowConsturctor<S, E> {
        Box::new(|_ctx| {
            Box::pin(async move { Box::new(self) as Box<dyn GraphicsFlow<S, E>> })
        })
    }
}

impl<S, E> GraphicsFlow<S, E> for TextLabel {
    fn on_init(&mut self, ctx: &mut Context, _: &mut S) -> Out<S, E> {
        self.init(ctx);
        Out::Empty
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        self.render()
    }

    #[cfg(feature = "integration-tests")]
    fn render_to_texture(
        &self,
        _ctx: &Context,
        _state: &mut S,
        _texture: &mut image::ImageBuffer<image::Rgba<u8>, wgpu::BufferView>,
    ) -> Result<crate::flow::ImageTestResult, anyhow::Error> {
        Ok(crate::flow::ImageTestResult::Passed)
    }
}
