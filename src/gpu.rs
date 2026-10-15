use std::sync::Arc;

use bevy::{
    prelude::{Resource, World},
    render::{
        render_phase::TrackedRenderPass,
        renderer::{RenderDevice, RenderQueue},
        sync_world::RenderEntity,
    },
};
use bevy_egui::{
    egui,
    render::{EguiBevyPaintCallbackImpl, EguiPipelineKey},
};
use wgpu::util::DeviceExt as _;

use crate::{
    model::{FrameParams, HISTORY, InitialConditions, STATE_BYTES},
    runtime::DisplayFrame,
};

#[derive(Resource)]
pub struct Gpu {
    state: wgpu::Buffer,
    uniform: wgpu::Buffer,
    history: wgpu::Buffer,
    render_group: wgpu::BindGroup,
    background: wgpu::RenderPipeline,
    trails: wgpu::RenderPipeline,
    bodies: wgpu::RenderPipeline,
    uploaded_frame: Option<Arc<DisplayFrame>>,
    format: wgpu::TextureFormat,
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
            render_group,
            background: render("orbital grid", "background_vs", "background_fs"),
            trails: render("orbit trails", "trail_vs", "trail_fs"),
            bodies: render("glowing bodies", "body_vs", "body_fs"),
            uploaded_frame: None,
            format,
        }
    }

    pub fn upload(&mut self, queue: &wgpu::Queue, params: &FrameParams, frame: &Arc<DisplayFrame>) {
        // Scene re-entry starts a fresh laboratory whose revisions can repeat.
        if self.uploaded_frame.as_ref().is_none_or(|previous| {
            previous.revision != frame.revision || !Arc::ptr_eq(previous, frame)
        }) {
            queue.write_buffer(&self.state, 0, &frame.state);
            queue.write_buffer(&self.history, 0, &frame.history);
            self.uploaded_frame = Some(frame.clone());
        }
        queue.write_buffer(&self.uniform, 0, &params.bytes(self.format.is_srgb()));
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

pub struct OrbitCallback {
    pub params: FrameParams,
    pub frame: Arc<DisplayFrame>,
}

impl EguiBevyPaintCallbackImpl for OrbitCallback {
    fn update(
        &self,
        _info: egui::PaintCallbackInfo,
        _entity: RenderEntity,
        key: EguiPipelineKey,
        world: &mut World,
    ) {
        if world
            .get_resource::<Gpu>()
            .is_none_or(|gpu| gpu.format != key.target_format)
        {
            let device = world.resource::<RenderDevice>();
            world.insert_resource(Gpu::new(device.wgpu_device(), key.target_format));
        }
        world.resource_scope(|world, mut gpu: bevy::prelude::Mut<Gpu>| {
            gpu.upload(world.resource::<RenderQueue>(), &self.params, &self.frame);
        });
    }

    fn render<'pass>(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut TrackedRenderPass<'pass>,
        _entity: RenderEntity,
        _key: EguiPipelineKey,
        world: &'pass World,
    ) {
        if let Some(gpu) = world.get_resource::<Gpu>() {
            gpu.paint(pass.wgpu_pass());
        }
    }
}

#[cfg(test)]
#[path = "gpu_tests.rs"]
mod tests;
