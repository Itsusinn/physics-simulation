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

fn pixels(
    world: &mut World,
    callback: &Orbit3dCallback,
    size: [u32; 2],
    format: wgpu::TextureFormat,
) -> Vec<u8> {
    let device = world.resource::<RenderDevice>().clone();
    let queue = world.resource::<RenderQueue>().clone();
    let texture = device
        .wgpu_device()
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("3D composite test"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
    let view = texture.create_view(&Default::default());
    let rect =
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size[0] as f32, size[1] as f32));
    let info = || egui::PaintCallbackInfo {
        viewport: rect,
        clip_rect: rect,
        pixels_per_point: 1.0,
        screen_size_px: size,
    };
    let key = EguiPipelineKey {
        target_format: format,
    };
    let entity = RenderEntity::from(world.spawn_empty().id());
    callback.update(info(), entity, key, world);
    callback.update(info(), entity, key, world);
    let mut encoder = device
        .wgpu_device()
        .create_command_encoder(&Default::default());
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
        callback.render(
            info(),
            &mut TrackedRenderPass::new(&device, pass),
            entity,
            key,
            world,
        );
    }
    let row = (size[0] * 4).div_ceil(256) * 256;
    let readback = device.wgpu_device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("3D pixel readback"),
        size: u64::from(row * size[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
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
        .wgpu_device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(20)),
        })
        .unwrap();
    let pixels = readback.slice(..).get_mapped_range().unwrap().to_vec();
    readback.unmap();
    pixels
}

#[test]
fn depth_occlusion_scene_reentry_and_resize_render_correctly() {
    use crate::scenes::three_d::camera::Camera;
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = block_on(instance.request_adapter(&Default::default()))
        .expect("3D rendering test requires GPU");
    let (device, queue) = block_on(adapter.request_device(&Default::default())).unwrap();
    let mut world = World::new();
    world.insert_resource(RenderDevice::new(device));
    world.insert_resource(RenderQueue::new(queue));
    let mut camera = Camera {
        yaw: -std::f32::consts::FRAC_PI_2,
        pitch: 0.0,
        distance: 10.0,
        ..Default::default()
    };
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    ] {
        for front in [0, 1] {
            let mut state = InitialConditions::default();
            state.bodies[front].position = [0.0, -1.0, 0.0];
            state.bodies[1 - front].position = [0.0, 1.0, 0.0];
            state.bodies[2].position = [20.0, 0.0, 0.0];
            // Include a segment crossing the camera near plane.
            let history =
                std::collections::VecDeque::from([[[2.0, -20.0, 0.0]; 3], [[2.0, 3.0, 0.0]; 3]]);
            let callback = Orbit3dCallback {
                frame: Arc::new(Geometry::new(&state, &history)),
                uniform: camera.uniform([64.0; 2], 0.8, 2.0),
                // Isolate sphere occlusion from foreground grid lines at this edge-on angle.
                grid: false,
                trail_samples: 4096,
            };
            let bytes = pixels(&mut world, &callback, [64; 2], format);
            let center = &bytes[32 * 256 + 32 * 4..32 * 256 + 32 * 4 + 4];
            if front == 0 {
                assert!(
                    center[2] > center[0] + 20,
                    "blue sphere should occlude orange: {center:?}"
                );
            } else {
                assert!(
                    center[0] > center[2] + 20,
                    "orange sphere should occlude blue: {center:?}"
                );
            }
            assert!(Arc::ptr_eq(
                world.resource::<Gpu3d>().frame.as_ref().unwrap(),
                &callback.frame
            ));
        }
    }
    camera.orbit([40.0, 30.0]);
    let callback = Orbit3dCallback {
        frame: Arc::new(Geometry::new(
            &InitialConditions::default(),
            &Default::default(),
        )),
        uniform: camera.uniform([128.0, 64.0], 0.8, 2.0),
        grid: true,
        trail_samples: 4096,
    };
    let bytes = pixels(
        &mut world,
        &callback,
        [128, 64],
        wgpu::TextureFormat::Rgba8Unorm,
    );
    assert_eq!(world.resource::<Gpu3d>().size, [128, 64]);
    assert!(
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[0].max(p[1]).max(p[2]) > 100)
    );
}
