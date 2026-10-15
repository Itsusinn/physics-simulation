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
fn bevy_callback_renders_immutable_cpu_frames_and_handles_reset() {
    render_and_check(wgpu::TextureFormat::Rgba8Unorm);
    render_and_check(wgpu::TextureFormat::Rgba8UnormSrgb);
}

fn render_and_check(format: wgpu::TextureFormat) {
    use crate::runtime::Laboratory;
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("A GPU adapter is required for the integration test");
    eprintln!("GPU validation adapter: {:?}", adapter.get_info());
    let (device, queue) =
        block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let render_device = RenderDevice::new(device.clone());
    let mut world = World::new();
    world.insert_resource(render_device.clone());
    world.insert_resource(RenderQueue::new(queue.clone()));
    let entity = RenderEntity::from(world.spawn_empty().id());
    let key = EguiPipelineKey {
        target_format: format,
    };
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
        format: key.target_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(64.0));
    let info = || egui::PaintCallbackInfo {
        viewport: rect,
        clip_rect: rect,
        pixels_per_point: 1.0,
        screen_size_px: [64; 2],
    };
    let mut lab = Laboratory::default();
    for duration in [0.0, 0.001, 0.0] {
        let params = FrameParams {
            duration,
            viewport: [64.0; 2],
            ..Default::default()
        };
        lab.advance(&params);
        let callback = OrbitCallback {
            params,
            frame: lab.frame.clone(),
        };
        callback.update(info(), entity, key, &mut world);
        callback.update(info(), entity, key, &mut world);
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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
            });
            let mut tracked = TrackedRenderPass::new(&render_device, pass);
            callback.render(info(), &mut tracked, entity, key, &world);
        }
        queue.submit([encoder.finish()]);
    }
    assert!((lab.simulation.snapshot.time - 0.001).abs() < 1e-15);
    assert!(Arc::ptr_eq(
        world.resource::<Gpu>().uploaded_frame.as_ref().unwrap(),
        &lab.frame
    ));

    // Read pixels back, rather than treating successful pipeline creation as a rendered frame.
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("pixels"),
        size: 64 * 64 * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: None,
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, |result| result.unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(20)),
        })
        .unwrap();
    let bytes = readback.slice(..).get_mapped_range().unwrap();
    assert!(
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[0].max(p[1]).max(p[2]) > 100),
        "body glow should be visible"
    );
    assert!(
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[0].max(p[1]).max(p[2]) < 40),
        "background should remain dark"
    );
    drop(bytes);
    readback.unmap();

    lab.reset(InitialConditions::default(), 1).unwrap();
    let callback = OrbitCallback {
        params: FrameParams::default(),
        frame: lab.frame.clone(),
    };
    callback.update(info(), entity, key, &mut world);
    assert!(Arc::ptr_eq(
        world.resource::<Gpu>().uploaded_frame.as_ref().unwrap(),
        &lab.frame
    ));
    assert_eq!(lab.simulation.snapshot.time, 0.0);

    // Re-entering a scene can reuse the revision of the previously uploaded frame.
    let mut initial = InitialConditions::default();
    initial.bodies[0].position[0] = 7.0;
    let frame = Arc::new(crate::runtime::DisplayFrame {
        state: initial.render_bytes(1, 1),
        history: vec![0; HISTORY as usize * 3 * 16],
        revision: lab.frame.revision,
    });
    let callback = OrbitCallback {
        params: FrameParams::default(),
        frame: frame.clone(),
    };
    callback.update(info(), entity, key, &mut world);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("scene re-entry state"),
        size: STATE_BYTES,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_buffer_to_buffer(&world.resource::<Gpu>().state, 0, &readback, 0, STATE_BYTES);
    queue.submit([encoder.finish()]);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, |result| result.unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(20)),
        })
        .unwrap();
    assert_eq!(
        &*readback.slice(..).get_mapped_range().unwrap(),
        frame.state.as_slice()
    );
    readback.unmap();
}
