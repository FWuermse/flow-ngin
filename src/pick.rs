use std::iter;

use winit::dpi::PhysicalPosition;

use crate::{flow::Context, pipelines::pick};

#[cfg(target_arch = "wasm32")]
use crate::Event;

enum MouseButtonState {
    Right,
    Left,
    None,
}

struct MouseState {
    coords: PhysicalPosition<f64>,
    pressed: MouseButtonState,
    selection: Option<u32>,
}

pub fn draw_to_pick_buffer(ctx: &Context, mouse_state: MouseState) {
    // Prepare data for picking:
    let u32_size = std::mem::size_of::<u32>() as u32;
    // The img lib requires divisibility of 256...
    let width = ctx.config.width;
    let height = ctx.config.height;
    let width_offset = 256 - (width % 256);
    let height_offset = 256 - (height % 256);
    let width_factor = (width as f64 + width_offset as f64) / width as f64;
    let height_factor = (height as f64 + height_offset as f64) / height as f64;
    let width = width + width_offset;
    let height = height + height_offset;

    let extent3d = wgpu::Extent3d {
        width: width,
        height: height,
        depth_or_array_layers: 1,
    };

    let pick_texture = &ctx.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Pick texture"),
        size: extent3d,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R32Uint,
        usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });

    let pick_depth_texture = &ctx.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Pick depth texture"),
        size: extent3d,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth24Plus,
        usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });

    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Pick Encoder"),
        });

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &pick_texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("Render texture"),
                    format: Some(wgpu::TextureFormat::R32Uint),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    usage: None,
                    aspect: wgpu::TextureAspect::All,
                    base_mip_level: 0,
                    mip_level_count: None,
                    base_array_layer: 0,
                    array_layer_count: None,
                }),
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 0.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &pick_depth_texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("Stencil texture"),
                    format: Some(wgpu::TextureFormat::Depth24Plus),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    usage: None,
                    aspect: wgpu::TextureAspect::All,
                    base_mip_level: 0,
                    mip_level_count: None,
                    base_array_layer: 0,
                    array_layer_count: None,
                }),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        let pick_pipeline = pick::mk_render_pipeline(&ctx.device, &ctx.camera.bind_group_layout);
        render_pass.set_pipeline(&pick_pipeline);

        /* TODO: call .draw() on all GraphicsFlows and make sure GraphicsFlows don't set
         pipelines themselves. */
    }

    let output_buffer_size = (u32_size * (width) * (height)) as wgpu::BufferAddress;
    let output_buffer_desc = wgpu::BufferDescriptor {
        size: output_buffer_size,
        usage: wgpu::BufferUsages::COPY_DST
                    // this tells wpgu that we want to read this buffer from the cpu
                    | wgpu::BufferUsages::MAP_READ,
        label: None,
        mapped_at_creation: false,
    };
    let output_buffer = ctx.device.create_buffer(&output_buffer_desc);

    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            aspect: wgpu::TextureAspect::All,
            texture: &pick_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &output_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(u32_size * (width)),
                rows_per_image: Some(height),
            },
        },
        extent3d,
    );

    ctx.queue.submit(iter::once(encoder.finish()));
    let binding = ctx.device.clone();
    let mouse_coords = mouse_state.coords.clone();
    // Try this:     proxy: Option<winit::event_loop::EventLoopProxy<State>>,
    #[cfg(target_arch = "wasm32")]
    // TODO: introduce event proxy for WASM
    let proxy = event_proxy.clone().unwrap();

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(async move {
        let buffer_slice = output_buffer.slice(..);
        let future_id = read_texture_buffer(
            buffer_slice,
            &binding,
            width_factor,
            height_factor,
            width,
            height,
            mouse_coords,
        );
        let id = future_id.await;
        // TODO: predefine some custom events such as Id
        assert!(proxy.send_event(Event::Id(id)).is_ok());
        output_buffer.unmap();
    });
    #[cfg(not(target_arch = "wasm32"))]
    {
        let buffer_slice = output_buffer.slice(..);
        let future_id = read_texture_buffer(
            buffer_slice,
            &binding,
            width_factor,
            height_factor,
            width,
            height,
            mouse_coords,
        );
        let id = pollster::block_on(future_id);
        // TODO: call on_click(id) on all GraphicsFlows.
        output_buffer.unmap();
    }
}

async fn read_texture_buffer(
    buffer_slice: wgpu::BufferSlice<'_>,
    device: &wgpu::Device,
    width_factor: f64,
    height_factor: f64,
    width: u32,
    _height: u32,
    mouse_coords: winit::dpi::PhysicalPosition<f64>,
) -> u32 {
    // NOTE: We have to create the mapping THEN device.poll() before await
    // the future. Otherwise the application will freeze.
    let (tx, rx) = futures_intrusive::channel::shared::oneshot_channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).unwrap();
    });
    #[cfg(target_arch = "wasm32")]
    device.poll(wgpu::PollType::Poll).unwrap();
    #[cfg(not(target_arch = "wasm32"))]
    device.poll(wgpu::PollType::Wait).unwrap();
    rx.receive().await.unwrap().unwrap();

    let data = buffer_slice.get_mapped_range();
    // [(0, 0, 0, 0), (0`, 255, 0, 255), (0, 0, 0, 0),
    // (0, 0, 0, 0), (0, 255, 0, 255), (0, 0, 0, 0)]
    let x = mouse_coords.x * width_factor;
    let y = mouse_coords.y * height_factor;
    let bytes_per_pixel = 4;
    let pick_index = (y as usize * width as usize + x as usize) * bytes_per_pixel;
    // TODO: bounds check.
    let r = data[pick_index];
    let g = data[pick_index + 1];
    let b = data[pick_index + 2];
    let a = data[pick_index + 3];

    let rgba_u32 = (r as u32) | ((g as u32) << 8) | ((b as u32) << 16) | (a as u32) << 24;

    // This is great for debugging. I'll keep it as I need it often.
    /*use image::{ImageBuffer, Rgba};
    let buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, data).unwrap();
    buffer.save("image.png").unwrap();*/

    println!("Selected obj with id {}", rgba_u32);
    log::info!("Selected obj with id {}", rgba_u32);
    rgba_u32
}
