#![cfg(all(feature = "integration-tests", not(target_arch = "wasm32")))]

use flow_ngin::pipelines::mipmapper::Mipmapper;

#[tokio::test]
async fn mipmaps_native() {
    check_mipmaps(wgpu::Backends::PRIMARY).await;
}

#[tokio::test]
async fn mipmaps_gl() {
    check_mipmaps(wgpu::Backends::GL).await;
}

async fn check_mipmaps(backends: wgpu::Backends) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        flags: wgpu::InstanceFlags::default(),
        memory_budget_thresholds: Default::default(),
        backend_options: Default::default(),
        display: None,
    });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .expect("GPU adapter required for mipmap regression test");
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            required_limits: wgpu::Limits::downlevel_webgl2_defaults(),
            ..Default::default()
        })
        .await
        .unwrap();
    let mipmapper = Mipmapper::new(&device);

    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    ] {
        for direct in [false, true] {
            // Rectangular full chain, single mip, and a black/white checkerboard.
            for (width, height, mip_count, checkerboard) in
                [(8, 4, 4, false), (1, 1, 1, false), (8, 8, 4, true)]
            {
                let mut usage = wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST;
                if direct {
                    usage |= wgpu::TextureUsages::RENDER_ATTACHMENT;
                }
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("mipmap regression texture"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: mip_count,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                });
                let mut pixels = Vec::new();
                for y in 0..height {
                    for x in 0..width {
                        let pixel = if checkerboard {
                            let value = if (x + y) % 2 == 0 { 0 } else { 255 };
                            [value, value, value, 160]
                        } else {
                            [64, 128, 192, 160]
                        };
                        pixels.extend_from_slice(&pixel);
                    }
                }
                queue.write_texture(
                    texture.as_image_copy(),
                    &pixels,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(width * 4),
                        rows_per_image: Some(height),
                    },
                    texture.size(),
                );
                mipmapper
                    .generate_mipmaps(&device, &queue, &texture)
                    .unwrap();
                assert_eq!(read_mip(&device, &queue, &texture, 0), pixels);

                let expected = if checkerboard {
                    let value = if format.is_srgb() { 188 } else { 128 };
                    [value, value, value, 160]
                } else {
                    [64, 128, 192, 160]
                };
                for mip in 1..mip_count {
                    for pixel in read_mip(&device, &queue, &texture, mip).chunks_exact(4) {
                        for (actual, expected) in pixel.iter().zip(expected) {
                            assert!(
                                actual.abs_diff(expected) <= 2,
                                "{backends:?}, {format:?}, direct={direct}, checkerboard={checkerboard}, mip={mip}: {pixel:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}

fn read_mip(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    mip: u32,
) -> Vec<u8> {
    let width = (texture.width() >> mip).max(1);
    let height = (texture.height() >> mip).max(1);
    let stride = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mipmap readback"),
        size: u64::from(stride * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            mip_level: mip,
            ..texture.as_image_copy()
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .unwrap();
    rx.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range();
    mapped
        .chunks_exact(stride as usize)
        .flat_map(|row| row[..width as usize * 4].iter().copied())
        .collect()
}
