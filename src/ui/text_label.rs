use std::cell::RefCell;

use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
    cosmic_text::Align,
};

use crate::{
    context::Context,
    flow::{FlowConstructor, GraphicsFlow, Out},
    render::Render,
    ui::{HAlign, Placement, VAlign, layout::Layout},
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
/// Uses [`Placement`] for positioning, like all other UI components.
/// Use `.halign()` / `.valign()` for alignment.
///
/// # Standalone usage
///
/// ```ignore
/// use flow_ngin::ui::TextLabel;
///
/// flow_ngin::flow::run::<(), ()>(vec![
///     TextLabel::new("Hello, flow-ngin!")
///         .color([255, 255, 255])
///         .into_constructor(),
/// ]);
/// ```
///
/// # Embedded usage
///
/// ```ignore
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
    placement: Placement,
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
            placement: Placement::default(),
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
                self.text_align(),
            );
            res.text_buffer.shape_until_scroll(&mut res.font_system, false);
        }
    }

    fn text_align(&self) -> Option<Align> {
        match self.placement.halign {
            HAlign::Left => None,
            HAlign::Center => Some(Align::Center),
            HAlign::Right => Some(Align::Right),
        }
    }

    /// Resolve position against parent bounds using [`Placement`].
    fn resolve_placement(&mut self, parent_x: u32, parent_y: u32, parent_w: u32, parent_h: u32) {
        let (x, y, w, h) = self.placement.resolve(parent_x, parent_y, parent_w, parent_h);
        self.resolved_x = x as f32;
        self.resolved_y = y as f32;
        self.resolved_w = w as f32;
        self.resolved_h = h as f32;
        if let Some(res) = self.resources.borrow_mut().as_mut() {
            res.text_buffer
                .set_size(&mut res.font_system, Some(w as f32), Some(h as f32));
            res.text_buffer.shape_until_scroll(&mut res.font_system, false);
        }
    }

    /// Initialize GPU resources. Called automatically by `GraphicsFlow::on_init`;
    /// call directly when embedding in a custom flow.
    pub fn init(&mut self, ctx: &mut Context) {
        self.resolve_placement(0, 0, ctx.config.width, ctx.config.height);

        #[cfg(not(target_arch = "wasm32"))]
        let mut font_system = FontSystem::new();
        #[cfg(target_arch = "wasm32")]
        let mut font_system = {
            let mut fs = FontSystem::new_with_locale_and_db(
                "en-US".into(),
                glyphon::cosmic_text::fontdb::Database::new(),
            );
            fs.db_mut()
                .load_font_data(include_bytes!("../../assets/fonts/Roboto-Regular.ttf").to_vec());
            fs
        };
        let swash_cache = SwashCache::new();
        let cache = Cache::new(&ctx.device);
        let viewport = Viewport::new(&ctx.device, &cache);
        let mut atlas = TextAtlas::new(&ctx.device, &ctx.queue, &cache, ctx.config.format);
        let text_renderer = TextRenderer::new(
            &mut atlas,
            &ctx.device,
            wgpu::MultisampleState {
                count: ctx.anti_aliasing.sample_count(),
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
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
            self.text_align(),
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

            let text_height: f32 = text_buffer
                .layout_runs()
                .map(|run| run.line_height)
                .sum();
            let top = match self.placement.valign {
                VAlign::Top => self.resolved_y,
                VAlign::Center => self.resolved_y + (self.resolved_h - text_height) / 2.0,
                VAlign::Bottom => self.resolved_y + self.resolved_h - text_height,
            };

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
                        top,
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

    /// Return the x-offset (in pixels) where a cursor at the given byte position
    /// should be placed, relative to the start of the text.
    pub fn cursor_x_for_byte_pos(&self, byte_pos: usize) -> f32 {
        let guard = self.resources.borrow();
        let Some(res) = guard.as_ref() else {
            return 0.0;
        };
        for run in res.text_buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                if byte_pos >= glyph.start && byte_pos < glyph.end {
                    return glyph.x;
                }
            }
            // Cursor is past last glyph => return end of line.
            return run.line_w;
        }
        0.0
    }

    /// Return the line height in pixels.
    pub fn get_line_height(&self) -> f32 {
        self.line_height
    }

    /// Wrap this label in a [`FlowConstructor`] for use with [`flow_ngin::flow::run`].
    pub fn into_constructor<S: 'static, E: Send + 'static>(self) -> FlowConstructor<S, E> {
        Box::new(|_ctx| {
            Box::pin(async move { Box::new(self) as Box<dyn GraphicsFlow<S, E>> })
        })
    }
}

impl Layout for TextLabel {
    fn resolve(&mut self, parent_x: u32, parent_y: u32, parent_w: u32, parent_h: u32, _queue: &wgpu::Queue) {
        self.resolve_placement(parent_x, parent_y, parent_w, parent_h);
    }
}

impl<S, E: Send> GraphicsFlow<S, E> for TextLabel {
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
