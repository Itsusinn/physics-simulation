use std::sync::{Arc, Mutex};

use eframe::{egui, egui_wgpu, wgpu};
use wgpu::util::DeviceExt as _;

use crate::model::{FrameParams, HISTORY, InitialConditions, STATE_BYTES, Snapshot};

#[derive(Default)]
pub struct Readback {
    pub snapshot: Snapshot,
    pending: bool,
}

pub struct Gpu {
    state: wgpu::Buffer,
    uniform: wgpu::Buffer,
    staging: wgpu::Buffer,
    compute: wgpu::ComputePipeline,
    compute_group: wgpu::BindGroup,
    render_group: wgpu::BindGroup,
    background: wgpu::RenderPipeline,
    trails: wgpu::RenderPipeline,
    bodies: wgpu::RenderPipeline,
    pub readback: Arc<Mutex<Readback>>,
    frame: u32,
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
            contents: &InitialConditions::default().state_bytes(),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        });
        let history = buffer(
            "GPU trajectory ring",
            u64::from(HISTORY) * 3 * 16,
            wgpu::BufferUsages::STORAGE,
        );
        let uniform = buffer(
            "simulation parameters",
            64,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let staging = buffer(
            "diagnostic readback",
            STATE_BYTES,
            wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        );
        let make_layout = |read_only: bool, visibility| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(if read_only {
                    "render resources"
                } else {
                    "compute resources"
                }),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only },
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(STATE_BYTES),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only },
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
                            min_binding_size: wgpu::BufferSize::new(64),
                        },
                        count: None,
                    },
                ],
            })
        };
        let compute_layout = make_layout(false, wgpu::ShaderStages::COMPUTE);
        let render_layout = make_layout(true, wgpu::ShaderStages::VERTEX_FRAGMENT);
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
        let compute_group = make_group(&compute_layout);
        let render_group = make_group(&render_layout);
        let pipeline_layout = |layout: &wgpu::BindGroupLayout| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("three-body pipeline layout"),
                bind_group_layouts: &[Some(layout)],
                immediate_size: 0,
            })
        };
        let compute_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("velocity Verlet compute shader"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("shaders/common.wgsl"),
                    "\n",
                    include_str!("shaders/simulate.wgsl")
                )
                .into(),
            ),
        });
        let compute = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("three-body integrator"),
            layout: Some(&pipeline_layout(&compute_layout)),
            module: &compute_shader,
            entry_point: Some("simulate"),
            compilation_options: Default::default(),
            cache: None,
        });
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
            staging,
            compute,
            compute_group,
            render_group,
            background: render("orbital grid", "background_vs", "background_fs"),
            trails: render("orbit trails", "trail_vs", "trail_fs"),
            bodies: render("glowing bodies", "body_vs", "body_fs"),
            readback: Arc::default(),
            frame: 0,
            generation: None,
        }
    }

    pub fn encode(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        params: &FrameParams,
        reset: Option<&InitialConditions>,
        generation: u64,
        read: bool,
    ) {
        if let Some(initial) = reset {
            queue.write_buffer(&self.state, 0, &initial.state_bytes());
            self.generation = Some(generation);
        }
        queue.write_buffer(&self.uniform, 0, &params.bytes());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("advance three bodies"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.compute);
            pass.set_bind_group(0, &self.compute_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        if read {
            let mut shared = self.readback.lock().unwrap();
            if !shared.pending {
                shared.pending = true;
                drop(shared);
                encoder.copy_buffer_to_buffer(&self.state, 0, &self.staging, 0, STATE_BYTES);
                let staging = self.staging.clone();
                let shared = self.readback.clone();
                encoder.map_buffer_on_submit(
                    &self.staging,
                    wgpu::MapMode::Read,
                    ..,
                    move |result| {
                        let snapshot = match result {
                            Ok(()) => {
                                let snapshot = match staging.get_mapped_range(..) {
                                    Ok(bytes) => Snapshot::decode(&bytes, generation),
                                    Err(error) => Snapshot {
                                        generation,
                                        error: Some(error.to_string()),
                                        ..Default::default()
                                    },
                                };
                                staging.unmap();
                                snapshot
                            }
                            Err(error) => Snapshot {
                                generation,
                                error: Some(error.to_string()),
                                ..Default::default()
                            },
                        };
                        let mut shared = shared.lock().unwrap();
                        shared.snapshot = snapshot;
                        shared.pending = false;
                    },
                );
            }
        }
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
}

impl egui_wgpu::CallbackTrait for SimulationCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let gpu: &mut Gpu = resources.get_mut().unwrap();
        gpu.frame = gpu.frame.wrapping_add(1);
        // A discarded egui layout pass must not consume the only reset request.
        let reset = (gpu.generation != Some(self.generation)).then_some(&self.initial);
        let read = gpu.frame.is_multiple_of(6) || reset.is_some() || self.params.duration == 0.0;
        gpu.encode(queue, encoder, &self.params, reset, self.generation, read);
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
