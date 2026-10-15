use super::*;
use std::{
    future::Future,
    task::{Context, Poll, Wake, Waker},
    time::Duration,
};

// A test-only executor using std, so the application needs no extra async runtime.
fn block_on<T>(future: impl Future<Output = T>) -> T {
    struct Notify(std::thread::Thread);
    impl Wake for Notify {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Notify(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::park_timeout(Duration::from_secs(10)),
        }
    }
}

#[test]
fn rendering_callback_uploads_cpu_state_and_preserves_pause_reset_and_history() {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("A GPU adapter is required for the integration test");
    eprintln!("GPU validation adapter: {:?}", adapter.get_info());
    let (device, queue) =
        block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let gpu = Gpu::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    // Exercise the actual egui callback, including generation-based reset and a render pass.
    // The same initial conditions accompany every callback without resetting every frame.
    use egui_wgpu::CallbackTrait as _;
    let diagnostics = gpu.diagnostics.clone();
    let mut resources = egui_wgpu::CallbackResources::default();
    resources.insert(gpu);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen validation"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    for (frame_id, duration) in [0.0, 0.001, 0.0].into_iter().enumerate() {
        let callback = SimulationCallback {
            params: FrameParams {
                duration,
                viewport: [64.0; 2],
                ..Default::default()
            },
            initial: InitialConditions::default(),
            generation: 9,
            frame_id: frame_id as u64,
        };
        let mut encoder = device.create_command_encoder(&Default::default());
        callback.prepare(
            &device,
            &queue,
            &egui_wgpu::ScreenDescriptor {
                size_in_pixels: [64; 2],
                pixels_per_point: 1.0,
            },
            &mut encoder,
            &mut resources,
        );
        callback.prepare(
            &device,
            &queue,
            &egui_wgpu::ScreenDescriptor {
                size_in_pixels: [64; 2],
                pixels_per_point: 1.0,
            },
            &mut encoder,
            &mut resources,
        );
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(64.0));
            callback.paint(
                egui::PaintCallbackInfo {
                    viewport: rect,
                    clip_rect: rect,
                    pixels_per_point: 1.0,
                    screen_size_px: [64; 2],
                },
                &mut pass,
                &resources,
            );
        }
        queue.submit([encoder.finish()]);
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(20)),
            })
            .unwrap();
    }
    let callback_snapshot = diagnostics.lock().unwrap().snapshot.clone();
    assert_eq!(callback_snapshot.generation, 9);
    assert!((callback_snapshot.time - 0.001).abs() < 1e-15);
    let gpu = resources.get_mut::<Gpu>().unwrap();
    assert_eq!(gpu.history_count, 2);
    assert_eq!(gpu.history_next, 2);
    gpu.update(
        &queue,
        &FrameParams::default(),
        Some(&InitialConditions::default()),
        10,
    );
    assert_eq!(gpu.history_count, 1);
    assert_eq!(gpu.history_next, 1);
    assert_eq!(gpu.simulation.snapshot.time, 0.0);
}
