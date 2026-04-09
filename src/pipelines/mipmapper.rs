//! GPU mipmap generation via blit (render) pipeline.
//!
//! [`Mipmapper`] holds a reusable render pipeline that downsamples each mip
//! level from the previous one using bilinear filtering. Create one instance
//! with [`Mipmapper::new`] and call [`Mipmapper::generate_mipmaps`] for each
//! texture that needs a full mip chain.

use anyhow::bail;

/// Generates a full mip chain for a texture by blitting each level from the
/// previous one through a fullscreen-triangle render pass.
pub struct Mipmapper {
    blit_pipeline: wgpu::RenderPipeline,
    blit_sampler: wgpu::Sampler,
}

impl Mipmapper {
    /// Create a new [`Mipmapper`] holding the blit render pipeline and sampler.
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../pipelines/blit.wgsl").into()),
        });

        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blit pipeline layout"),
            bind_group_layouts: &[Some(&blit_layout)],
            ..Default::default()
        });

        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit mipmap pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let blit_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blit sampler"),
            min_filter: wgpu::FilterMode::Linear,
            mag_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            blit_pipeline,
            blit_sampler,
        }
    }

    /// Fill every mip level of `texture` by successively downsampling from the
    /// previous level. The base mip (level 0) must already contain data.
    ///
    /// Only `Rgba8Unorm` and `Rgba8UnormSrgb` formats are supported. Textures
    /// with `mip_level_count <= 1` are returned unchanged.
    pub fn generate_mipmaps(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
    ) -> anyhow::Result<()> {
        match texture.format() {
            wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb => {}
            _ => bail!("Unsupported mipmap format {:?}", texture.format()),
        }

        if texture.mip_level_count() <= 1 {
            return Ok(());
        }

        let non_srgb_format = texture.format().remove_srgb_suffix();

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("mipmap encoder"),
        });

        // When the texture lacks RENDER_ATTACHMENT usage we render into a
        // temporary texture that does, then copy the results back.
        // TODO: check if that all works on wasm.
        let (mut src_view, maybe_temp) = if texture
            .usage()
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        {
            (
                texture.create_view(&wgpu::TextureViewDescriptor {
                    format: Some(non_srgb_format),
                    base_mip_level: 0,
                    mip_level_count: Some(1),
                    ..Default::default()
                }),
                None,
            )
        } else {
            let temp = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("mipmap temp texture"),
                size: texture.size(),
                mip_level_count: texture.mip_level_count(),
                sample_count: texture.sample_count(),
                dimension: texture.dimension(),
                format: non_srgb_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });

            encoder.copy_texture_to_texture(
                texture.as_image_copy(),
                temp.as_image_copy(),
                texture.size(),
            );

            (
                temp.create_view(&wgpu::TextureViewDescriptor {
                    mip_level_count: Some(1),
                    ..Default::default()
                }),
                Some(temp),
            )
        };

        for mip in 1..texture.mip_level_count() {
            let dst_view =
                src_view
                    .texture()
                    .create_view(&wgpu::TextureViewDescriptor {
                        format: Some(non_srgb_format),
                        base_mip_level: mip,
                        mip_level_count: Some(1),
                        ..Default::default()
                    });

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.blit_pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&src_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.blit_sampler),
                    },
                ],
            });

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mipmap pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &dst_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                ..Default::default()
            });
            pass.set_pipeline(&self.blit_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);

            src_view = dst_view;
        }

        // Copy generated mip chain back from temp into the original texture.
        if let Some(ref temp) = maybe_temp {
            let mut size = temp.size();
            for mip_level in 1..temp.mip_level_count() {
                size.width = (size.width / 2).max(1);
                size.height = (size.height / 2).max(1);
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        mip_level,
                        ..temp.as_image_copy()
                    },
                    wgpu::TexelCopyTextureInfo {
                        mip_level,
                        ..texture.as_image_copy()
                    },
                    size,
                );
            }
        }

        queue.submit([encoder.finish()]);

        Ok(())
    }
}
