use super::*;

#[tokio::test]
async fn animation_renders_and_changes_with_time_with_and_without_msaa() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .unwrap();
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .unwrap();
    preparation_waits_for_constructors_and_async_init(crate::context::InitContext {
        device: device.clone(),
        queue: queue.clone(),
    })
    .await;
    for samples in [1, 4] {
        let loading = LoadingScreen::create(
            &device,
            &queue,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            samples,
            512,
            512,
        );
        let first = frame(&device, &queue, &loading, samples, 0.0);
        let second = frame(&device, &queue, &loading, samples, 2.0);
        assert!(
            second.chunks_exact(4).any(|pixel| pixel[1] > 180),
            "The illuminated inner cube and labels must be visible"
        );
        assert!(
            first.iter().zip(&second).filter(|(a, b)| a != b).count() > 1000,
            "The animation must change between frames"
        );
        if samples == 1 {
            if let Ok(path) = std::env::var("FLOW_LOADING_PREVIEW") {
                image::save_buffer(&path, &second, 512, 512, image::ColorType::Rgba8).unwrap();
            }
        }
    }
}

fn frame(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    loading: &LoadingScreen,
    samples: u32,
    time: f32,
) -> Vec<u8> {
    queue.write_buffer(
        &loading.uniform,
        0,
        bytemuck::cast_slice(&[time, 512.0, 512.0, 0.0]),
    );
    let texture = |sample_count, format, usage| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Loading animation test target"),
            size: wgpu::Extent3d {
                width: 512,
                height: 512,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let color = texture(
        1,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let color_view = color.create_view(&Default::default());
    let msaa = texture(
        samples,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    let msaa_view = msaa.create_view(&Default::default());
    let depth = Texture::create_depth_texture(device, [512, 512], "Loading test depth", samples);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Loading animation readback"),
        size: 512 * 512 * 4,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: if samples == 1 {
                    &color_view
                } else {
                    &msaa_view
                },
                resolve_target: (samples > 1).then_some(&color_view),
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth.view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_pipeline(&loading.pipeline);
        pass.set_bind_group(0, &loading.group, &[]);
        pass.draw(0..3, 0..1);
    }
    encoder.copy_texture_to_buffer(
        color.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(512 * 4),
                rows_per_image: Some(512),
            },
        },
        color.size(),
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(10)),
        })
        .unwrap();
    rx.recv().unwrap().unwrap();
    buffer.slice(..).get_mapped_range().unwrap().to_vec()
}

async fn preparation_waits_for_constructors_and_async_init(gpu: crate::context::InitContext) {
    use crate::flow::FlowConstructor;
    use futures::channel::oneshot;
    use std::{cell::RefCell, rc::Rc};

    struct Probe {
        id: usize,
        gate: Option<oneshot::Receiver<()>>,
        started: Rc<RefCell<Vec<usize>>>,
        notify: tokio::sync::mpsc::UnboundedSender<usize>,
    }
    impl GraphicsFlow<(), ()> for Probe {
        fn on_render<'pass>(&self) -> Render<'_, 'pass> {
            self.started.borrow_mut().push(self.id);
            Render::None
        }

        fn on_init_async(
            &mut self,
            _: crate::context::InitContext,
        ) -> futures::future::LocalBoxFuture<'_, ()> {
            Box::pin(async move {
                self.started.borrow_mut().push(self.id);
                self.notify.send(self.id).unwrap();
                self.gate.take().unwrap().await.unwrap();
            })
        }
    }
    let started = Rc::new(RefCell::new(Vec::new()));
    let (notify, mut notifications) = tokio::sync::mpsc::unbounded_channel();
    let mut constructors: Vec<FlowConstructor<(), ()>> = Vec::new();
    let mut new_gates = Vec::new();
    let mut init_gates = Vec::new();
    for id in 0..2 {
        let (new_tx, new_rx) = oneshot::channel();
        let (init_tx, init_rx) = oneshot::channel();
        new_gates.push(new_tx);
        init_gates.push(init_tx);
        let started = started.clone();
        let notify = notify.clone();
        constructors.push(Box::new(move |_| {
            Box::pin(async move {
                new_rx.await.unwrap();
                tokio::time::sleep(Duration::from_millis(1)).await;
                Box::new(Probe {
                    id,
                    gate: Some(init_rx),
                    started,
                    notify,
                }) as Box<dyn GraphicsFlow<(), ()>>
            })
        }));
    }
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let task = tokio::task::spawn_local(startup::prepare_flows(constructors, gpu));
            assert!(started.borrow().is_empty());
            new_gates.pop().unwrap().send(()).unwrap();
            assert_eq!(notifications.recv().await, Some(1));
            init_gates.pop().unwrap().send(()).unwrap();
            assert!(
                !task.is_finished(),
                "The other constructor is still pending"
            );
            new_gates.pop().unwrap().send(()).unwrap();
            assert_eq!(notifications.recv().await, Some(0));
            assert!(
                !task.is_finished(),
                "Async initialization still gates activation"
            );
            assert_eq!(*started.borrow(), [1, 0]);
            init_gates.pop().unwrap().send(()).unwrap();
            let flows = task.await.unwrap();
            started.borrow_mut().clear();
            for flow in flows {
                flow.on_render();
            }
            assert_eq!(
                *started.borrow(),
                [0, 1],
                "Completion order must not change registration order"
            );
        })
        .await;
}
