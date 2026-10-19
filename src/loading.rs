mod startup;
pub(crate) use startup::Startup;
mod labels;

use std::time::Duration;
use wgpu::util::DeviceExt;

use crate::{
    context::Context,
    data_structures::texture::Texture,
    flow::{GraphicsFlow, Out},
    render::Render,
};

pub(crate) struct LoadingScreen {
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    group: wgpu::BindGroup,
    elapsed: f32,
    target: (wgpu::TextureFormat, u32),
}

impl LoadingScreen {
    pub(crate) fn new(ctx: &Context) -> Self {
        Self::create(
            &ctx.device,
            &ctx.queue,
            ctx.config.format,
            ctx.anti_aliasing.sample_count(),
            ctx.config.width,
            ctx.config.height,
        )
    }

    fn create(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        sample_count: u32,
        width: u32,
        height: u32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Nested loading cubes shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("loading/loading.wgsl").into()),
        });
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Startup animation time and viewport"),
            contents: bytemuck::cast_slice(&[0.0f32, width as f32, height as f32, 0.0]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Startup animation pipeline"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: sample_count,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        });
        let labels = Texture::from_image(
            device,
            queue,
            &image::DynamicImage::ImageRgba8(labels::atlas()),
            Some("Loading labels"),
            false,
        )
        .expect("Loading label atlas is valid");
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Startup animation uniforms"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&labels.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(labels.sampler.as_ref().unwrap()),
                },
            ],
        });
        Self {
            pipeline,
            uniform,
            group,
            elapsed: 0.0,
            target: (format, sample_count),
        }
    }
}

impl<S, E: Send> GraphicsFlow<S, E> for LoadingScreen {
    fn on_update(&mut self, ctx: &Context, _: &mut S, dt: Duration) -> Out<S, E> {
        if self.target != (ctx.config.format, ctx.anti_aliasing.sample_count()) {
            let elapsed = self.elapsed;
            *self = Self::new(ctx);
            self.elapsed = elapsed;
        }
        self.elapsed = (self.elapsed + dt.as_secs_f32()) % 3600.0;
        ctx.queue.write_buffer(
            &self.uniform,
            0,
            bytemuck::cast_slice(&[
                self.elapsed,
                ctx.config.width as f32,
                ctx.config.height as f32,
                0.0,
            ]),
        );
        Out::Empty
    }

    fn on_render<'pass>(&self) -> Render<'_, 'pass> {
        Render::Custom(Box::new(move |_, pass| {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.group, &[]);
            pass.draw(0..3, 0..1);
        }))
    }

    #[cfg(feature = "integration-tests")]
    fn render_to_texture(
        &self,
        _: &Context,
        _: &mut S,
        _: &mut image::ImageBuffer<image::Rgba<u8>, wgpu::BufferView>,
    ) -> Result<crate::flow::ImageTestResult, anyhow::Error> {
        Ok(crate::flow::ImageTestResult::Waiting)
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
