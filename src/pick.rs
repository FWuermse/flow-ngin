//! Object picking and selection.
//!
//! This module implements GPU-based object picking: rendering scene objects with
//! unique IDs to an offscreen texture, then reading the pixel under the mouse cursor
//! to determine which object was clicked. Supports picking for both 3D (instanced)
//! and 2D (GUI/flat) objects.
//!
//! The picking pipeline works as follows:
//! 1. Render all objects to an offscreen texture using unique IDs as RGBA values for the fragment shader
//! 2. Read the pixel at the mouse cursor position (scaled according to platform limitations on texture sizes)
//! 3. Map the pick ID back to the flow that owns the object (determined by the render tree)
//! 4. Return the selected object ID and owning flows
//!
//! Especially step 4 makes sure that only those flows are invoked that were responsible for selected object.

use std::{
    collections::{HashMap, HashSet},
    iter,
};

use crate::{
    context::{Context, MouseState},
    data_structures::model::DrawModel,
    flow::GraphicsFlow,
    render::{Flat, Instanced},
    resources::pick::{load_pick_model, load_pick_texture},
};

#[cfg(target_arch = "wasm32")]
use crate::flow::FlowEvent;

/// Render all flows to pick texture and determine which object was clicked.
///
/// # Arguments
///
/// * `async_runtime` using the tokio runtime for async resource loading if not on WASM
/// * `flows` represent all active graphics flows with their renderable objects
/// * `ctx` is the rendering context
/// * `mouse_state` is required for getting the mouse coordinates at the time of picking
/// * `proxy` WASM futures can only resolve using the winit event loop proxy by sending events
///
/// # Returns
///
/// `Some((pick_id, flow_ids))` if an object was picked, or `None` picking is done via the event loop.
pub fn draw_to_pick_buffer<State, Event>(
    #[cfg(not(target_arch = "wasm32"))] async_runtime: &tokio::runtime::Runtime,
    flows: &mut Vec<Box<dyn GraphicsFlow<State, Event>>>,
    ctx: &Context,
    mouse_state: &MouseState,
    #[cfg(target_arch = "wasm32")] proxy: winit::event_loop::EventLoopProxy<
        crate::flow::FlowEvent<State, Event>,
    >,
) -> Option<(u32, HashSet<usize>)> {
    // Prepare data for picking:
    let u32_size = std::mem::size_of::<u32>() as u32;
    // The img lib requires divisibility of 256...
    let width = ctx.config.width;
    let height = ctx.config.height;
    let width_offset = 256 - (width % 256);
    let height_offset = 256 - (height % 256);
    let width_factor = (f64::from(width) + f64::from(width_offset)) / f64::from(width);
    let height_factor = (f64::from(height) + f64::from(height_offset)) / f64::from(height);
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
    let mut translation: HashMap<u32, HashSet<usize>> = HashMap::new();

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
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
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

        let mut basics: Vec<Instanced> = Vec::new();
        let mut flats: Vec<Flat> = Vec::new();
        /*
           We support graphics flow that handle pick IDs internally. Thus, we store the
           correspondance of the flow index and the model picked so that each flow only
           gets invoked if one of the IDs it manages was picked.

           Example:
           flow1 at index 0 owns the pick IDs [1, 2, 3, 4, 5]
           flow2 at index 1 owns the pick IDs [5, 6, 7, 8, 9]

           Warning: Overlapping ID responsibility may not be the best design choice.

           On pick result 2 we invoke flow1.on_pick(2).
           On pick result 5 we invoke flow1.on_pick(5) followed by flow2.on_pick(5).
        */
        flows.iter_mut().enumerate().for_each(|(idx, flow)| {
            let render = flow.on_render();
            render.map_ids(idx, &mut translation);
            render.set_pick_pipelines(&ctx, &mut render_pass, &mut basics, &mut flats);
        });

        render_pass.set_pipeline(&ctx.pipelines.pick);
        for instanced in basics.iter_mut() {
            let pick_model =
                load_pick_model(&ctx.device, instanced.id, instanced.model.meshes.clone()).unwrap();
            render_pass.set_vertex_buffer(1, instanced.instance.slice(..));
            let amount: Result<u32, _> = instanced.amount.try_into();
            match amount {
                Err(e) => log::error!(
                    "Failed to render flat object with id {}. Maximum amount of supported instances is {}. Error: {}",
                    instanced.id,
                    u32::MAX,
                    e
                ),
                Ok(amount) => render_pass.draw_model_instanced(
                    &pick_model,
                    0..amount,
                    &ctx.camera.bind_group,
                    &ctx.light.bind_group,
                ),
            }
        }

        render_pass.set_pipeline(&ctx.pipelines.flat_pick);
        for flat in flats {
            let pick_group = load_pick_texture(flat.id, &ctx.device);
            render_pass.set_bind_group(0, &pick_group, &[]);
            render_pass.set_vertex_buffer(0, flat.vertex.slice(..));
            render_pass.set_index_buffer(flat.index.slice(..), wgpu::IndexFormat::Uint16);
            let amount: Result<u32, _> = flat.amount.try_into();
            match amount {
                Err(e) => log::error!(
                    "Failed to render flat object with id {}. Maximum amount of supported instances is {}. Error: {}",
                    flat.id,
                    u32::MAX,
                    e
                ),
                Ok(amount) => render_pass.draw_indexed(0..amount, 0, 0..1),
            }
        }
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
        if let Some(flow_ids) = translation.get(&id) {
            assert!(
                proxy
                    .send_event(FlowEvent::Id((id, flow_ids.clone())))
                    .is_ok()
            );
            output_buffer.unmap();
        };
    });
    #[cfg(target_arch = "wasm32")]
    return None;
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
        // Depending on the average timing this hould not block but rather always send an event
        let id = async_runtime.block_on(future_id);
        return translation.get(&id).map(|flow_ids| (id, flow_ids.clone()));
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
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .unwrap();
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

    let rgba_u32 = u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16 | u32::from(a) << 24;

    // This is great for debugging. I'll keep it as I need it often.
    /*use image::{ImageBuffer, Rgba};
    let buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, data).unwrap();
    buffer.save("image.png").unwrap();*/

    log::info!("Selected obj with id {}", rgba_u32);
    rgba_u32
}
