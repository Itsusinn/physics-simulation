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

fn advance(
    gpu: &mut Gpu,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    params: &FrameParams,
    reset: Option<&InitialConditions>,
    generation: u64,
) -> Snapshot {
    let mut encoder = device.create_command_encoder(&Default::default());
    gpu.encode(queue, &mut encoder, params, reset, generation, true);
    queue.submit([encoder.finish()]);
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(20)),
        })
        .unwrap();
    let snapshot = gpu.readback.lock().unwrap().snapshot.clone();
    assert!(snapshot.ready, "GPU readback failed: {:?}", snapshot.error);
    snapshot
}

// Independent f64 Runge-Kutta 4 reference; production integration happens only in WGSL.
fn reference_rk4(initial: &InitialConditions, dt: f64, steps: usize) -> [[f64; 4]; 3] {
    let mut y = initial.bodies.map(|b| {
        [
            b.position[0] as f64,
            b.position[1] as f64,
            b.velocity[0] as f64,
            b.velocity[1] as f64,
        ]
    });
    let derivative = |y: [[f64; 4]; 3]| {
        let mut d = [[0.0; 4]; 3];
        for i in 0..3 {
            d[i][0] = y[i][2];
            d[i][1] = y[i][3];
            for j in 0..3 {
                if i == j {
                    continue;
                }
                let dx = y[j][0] - y[i][0];
                let dy = y[j][1] - y[i][1];
                let radius2 = dx * dx + dy * dy + f64::from(initial.softening).powi(2);
                let force = f64::from(initial.gravity * initial.bodies[j].mass) / radius2.powf(1.5);
                d[i][2] += dx * force;
                d[i][3] += dy * force;
            }
        }
        d
    };
    let add = |a: [[f64; 4]; 3], b: [[f64; 4]; 3], factor: f64| {
        std::array::from_fn(|i| std::array::from_fn(|j| a[i][j] + b[i][j] * factor))
    };
    for _ in 0..steps {
        let k1 = derivative(y);
        let k2 = derivative(add(y, k1, dt * 0.5));
        let k3 = derivative(add(y, k2, dt * 0.5));
        let k4 = derivative(add(y, k3, dt));
        y = std::array::from_fn(|i| {
            std::array::from_fn(|j| {
                y[i][j] + dt / 6.0 * (k1[i][j] + 2.0 * k2[i][j] + 2.0 * k3[i][j] + k4[i][j])
            })
        });
    }
    y
}

#[test]
fn gpu_matches_reference_and_handles_pause_reset_and_work_budget() {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("A GPU adapter is required for the integration test");
    eprintln!("GPU validation adapter: {:?}", adapter.get_info());
    let (device, queue) =
        block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    // Creation validates both compute and all three render pipelines.
    let mut gpu = Gpu::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let initial = InitialConditions::default();
    let mut params = FrameParams::default();
    let initial_snapshot = advance(&mut gpu, &device, &queue, &params, Some(&initial), 0);
    assert_eq!(initial_snapshot.time, 0.0);
    assert_eq!(
        initial_snapshot.positions,
        initial.bodies.map(|b| b.position)
    );
    assert!((f64::from(initial_snapshot.energy) + 769.0 / 60.0).abs() < 2e-6);
    params.duration = 0.01;
    let mut snapshot = initial_snapshot;
    for _ in 0..100 {
        snapshot = advance(&mut gpu, &device, &queue, &params, None, 0);
    }
    let reference = reference_rk4(&initial, 0.0001, 10_000);
    for (actual, expected) in snapshot.positions.iter().zip(reference) {
        for axis in 0..2 {
            assert!(
                (f64::from(actual[axis]) - expected[axis]).abs() < 5e-5,
                "position {actual:?} != {expected:?}"
            );
        }
    }
    assert!((snapshot.time - 1.0).abs() < 2e-5);
    assert!(snapshot.relative_energy_error() < 1e-4);
    assert!(snapshot.momentum[0].hypot(snapshot.momentum[1]) < 1e-4);
    assert!(snapshot.min_step > 0.0);
    assert!(snapshot.separation > 0.0);
    assert!(snapshot.angular_momentum.abs() < 1e-4);
    assert_eq!(snapshot.flags, 1);
    eprintln!(
        "t=1: relative energy drift = {:.3e}, |P| = {:.3e}",
        snapshot.relative_energy_error(),
        snapshot.momentum[0].hypot(snapshot.momentum[1])
    );

    params.duration = 0.0;
    let paused = advance(&mut gpu, &device, &queue, &params, None, 0);
    assert_eq!(paused.time, snapshot.time);
    assert_eq!(paused.positions, snapshot.positions);
    let reset = advance(&mut gpu, &device, &queue, &params, Some(&initial), 7);
    assert_eq!(reset.generation, 7);
    assert_eq!(reset.time, 0.0);
    assert_eq!(reset.positions, initial.bodies.map(|b| b.position));

    // Include close encounters, not only the smooth initial fall.
    params.duration = 0.05;
    let mut worst_drift = 0.0_f32;
    let mut close_encounter_snapshot = reset;
    for _ in 0..500 {
        close_encounter_snapshot = advance(&mut gpu, &device, &queue, &params, None, 7);
        if close_encounter_snapshot.relative_energy_error() > worst_drift + 0.01 {
            eprintln!(
                "drift at t={:.6}: E={:.6}, r={:.7}, dt={:.3e}, steps={}",
                close_encounter_snapshot.time,
                close_encounter_snapshot.energy,
                close_encounter_snapshot.separation,
                close_encounter_snapshot.min_step,
                close_encounter_snapshot.steps
            );
        }
        worst_drift = worst_drift.max(close_encounter_snapshot.relative_energy_error());
        assert_eq!(close_encounter_snapshot.flags & 4, 0);
        if close_encounter_snapshot.time >= 10.0 {
            break;
        }
    }
    assert!(close_encounter_snapshot.time >= 10.0);
    eprintln!("t=10 close encounters: maximum relative energy drift = {worst_drift:.3e}");
    assert!(worst_drift < 0.005);
    params.duration = 0.0;
    advance(&mut gpu, &device, &queue, &params, Some(&initial), 7);

    params.max_step = 0.00001;
    params.duration = 0.5;
    let bounded = advance(&mut gpu, &device, &queue, &params, None, 7);
    assert_eq!(bounded.steps, 2048);
    assert_eq!(bounded.flags & 2, 2);
    assert!(bounded.time < 0.021);
    assert!(bounded.relative_energy_error() < 1e-4);

    let mut softened = initial.clone();
    softened.bodies[1].position = softened.bodies[0].position;
    softened.softening = 0.1;
    params.softening = 0.1;
    params.duration = 0.001;
    let result = advance(&mut gpu, &device, &queue, &params, Some(&softened), 8);
    assert!(result.energy.is_finite());
    assert_eq!(result.flags & 4, 0);

    // Exercise the actual egui callback, including generation-based reset and a render pass.
    // The same initial conditions accompany every callback without resetting every frame.
    use egui_wgpu::CallbackTrait as _;
    let readback = gpu.readback.clone();
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
    for duration in [0.0, 0.001, 0.0] {
        let callback = SimulationCallback {
            params: FrameParams {
                duration,
                viewport: [64.0; 2],
                ..Default::default()
            },
            initial: InitialConditions::default(),
            generation: 9,
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
    let callback_snapshot = &readback.lock().unwrap().snapshot;
    assert_eq!(callback_snapshot.generation, 9);
    assert!((callback_snapshot.time - 0.001).abs() < 1e-7);
}
