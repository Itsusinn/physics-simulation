use std::sync::{Arc, Mutex};

use eframe::{egui, egui_wgpu, wgpu};
use wgpu::util::DeviceExt as _;

use crate::{
    model::{FrameParams, HISTORY, InitialConditions, STATE_BYTES, Snapshot},
    simulation::Simulation,
};

#[derive(Default)]
pub struct Diagnostics {
    pub snapshot: Snapshot,
}

pub struct Gpu {
    state: wgpu::Buffer,
    uniform: wgpu::Buffer,
    history: wgpu::Buffer,
    simulation: Simulation,
    history_next: u32,
    history_count: u32,
    render_group: wgpu::BindGroup,
    background: wgpu::RenderPipeline,
    trails: wgpu::RenderPipeline,
    bodies: wgpu::RenderPipeline,
    pub diagnostics: Arc<Mutex<Diagnostics>>,
    last_frame: Option<u64>,
    generation: Option<u64>,
}

impl Gpu {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let buffer = |label, size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let state = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("three-body state"),
            contents: &InitialConditions::default().render_bytes(0, 0),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        });
        let history = buffer(
            "trajectory ring",
            u64::from(HISTORY) * 3 * 16,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let uniform = buffer(
            "render parameters",
            48,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let make_layout = |visibility| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("render resources"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(STATE_BYTES),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(u64::from(HISTORY) * 3 * 16),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(48),
                        },
                        count: None,
                    },
                ],
            })
        };
        let render_layout = make_layout(wgpu::ShaderStages::VERTEX_FRAGMENT);
        let make_group = |layout: &wgpu::BindGroupLayout| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("three-body bindings"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: state.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: history.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: uniform.as_entire_binding(),
                    },
                ],
            })
        };
        let render_group = make_group(&render_layout);
        let pipeline_layout = |layout: &wgpu::BindGroupLayout| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("three-body pipeline layout"),
                bind_group_layouts: &[Some(layout)],
                immediate_size: 0,
            })
        };
        let render_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("orbital visualization shader"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("shaders/common.wgsl"),
                    "\n",
                    include_str!("shaders/render.wgsl")
                )
                .into(),
            ),
        });
        let layout = pipeline_layout(&render_layout);
        let render = |label, vertex_entry, fragment_entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &render_shader,
                    entry_point: Some(vertex_entry),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &render_shader,
                    entry_point: Some(fragment_entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            state,
            uniform,
            history,
            simulation: Simulation::new(InitialConditions::default(), 0).unwrap(),
            history_next: 0,
            history_count: 0,
            render_group,
            background: render("orbital grid", "background_vs", "background_fs"),
            trails: render("orbit trails", "trail_vs", "trail_fs"),
            bodies: render("glowing bodies", "body_vs", "body_fs"),
            diagnostics: Arc::default(),
            last_frame: None,
            generation: None,
        }
    }

    pub fn update(
        &mut self,
        queue: &wgpu::Queue,
        params: &FrameParams,
        reset: Option<&InitialConditions>,
        generation: u64,
    ) {
        if let Some(initial) = reset {
            match Simulation::new(initial.clone(), generation) {
                Ok(simulation) => self.simulation = simulation,
                Err(error) => {
                    self.diagnostics.lock().unwrap().snapshot = Snapshot {
                        generation,
                        flags: 4,
                        error: Some(error.into()),
                        ..Default::default()
                    };
                    return;
                }
            }
            self.generation = Some(generation);
            self.history_next = 0;
            self.history_count = 0;
            self.sample_history(queue);
        }
        let before = self.simulation.snapshot.time;
        self.simulation.advance(params);
        if self.simulation.snapshot.time > before {
            self.sample_history(queue);
        }
        queue.write_buffer(
            &self.state,
            0,
            &self
                .simulation
                .state
                .render_bytes(self.history_next, self.history_count),
        );
        queue.write_buffer(&self.uniform, 0, &params.bytes());
        self.diagnostics.lock().unwrap().snapshot = self.simulation.snapshot.clone();
    }

    fn sample_history(&mut self, queue: &wgpu::Queue) {
        for (i, body) in self.simulation.state.bodies.iter().enumerate() {
            let mut bytes = Vec::with_capacity(16);
            for value in [body.position[0] as f32, body.position[1] as f32, 0.0, 0.0] {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            queue.write_buffer(
                &self.history,
                (i as u64 * u64::from(HISTORY) + u64::from(self.history_next)) * 16,
                &bytes,
            );
        }
        self.history_next = (self.history_next + 1) % HISTORY;
        self.history_count = (self.history_count + 1).min(HISTORY);
    }

    pub fn paint(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(0, &self.render_group, &[]);
        pass.set_pipeline(&self.background);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&self.trails);
        pass.draw(0..6, 0..(HISTORY - 1) * 3);
        pass.set_pipeline(&self.bodies);
        pass.draw(0..6, 0..3);
    }
}

pub struct SimulationCallback {
    pub params: FrameParams,
    pub initial: InitialConditions,
    pub generation: u64,
    pub frame_id: u64,
}

impl egui_wgpu::CallbackTrait for SimulationCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let gpu: &mut Gpu = resources.get_mut().unwrap();
        // Only a painted callback advances physics. Repeated preparation of the
        // same egui frame must not advance time or append duplicate trail samples.
        if gpu.last_frame != Some(self.frame_id) || gpu.generation != Some(self.generation) {
            let reset = (gpu.generation != Some(self.generation)).then_some(&self.initial);
            gpu.update(queue, &self.params, reset, self.generation);
            gpu.last_frame = Some(self.frame_id);
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        resources.get::<Gpu>().unwrap().paint(pass);
    }
}

#[cfg(test)]
#[path = "gpu_tests.rs"]
mod tests;
