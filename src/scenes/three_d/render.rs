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

use super::model::InitialConditions;

pub const COLORS: [[f32; 3]; 3] = [[0.30, 0.77, 1.0], [1.0, 0.65, 0.27], [0.78, 0.57, 1.0]];

/// Display-only f32 data. The integrator and its diagnostics remain f64.
pub struct Geometry {
    pub bodies: Vec<u8>,
    pub trails: Vec<u8>,
    pub segments: u32,
}

pub fn floats(bytes: &mut Vec<u8>, values: impl IntoIterator<Item = f32>) {
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}

impl Geometry {
    pub fn new(
        state: &InitialConditions,
        history: &std::collections::VecDeque<[[f64; 3]; 3]>,
    ) -> Self {
        let mut bodies = Vec::with_capacity(48);
        for body in state.bodies {
            floats(&mut bodies, body.position.map(|v| v as f32));
            floats(&mut bodies, [body.mass as f32]);
        }
        let mut trails = Vec::new();
        let length = history.len().saturating_sub(1);
        // Interleave bodies for each time sample, so a draw range selects recent history.
        for (sample, (a, b)) in history.iter().zip(history.iter().skip(1)).enumerate() {
            for body in 0..3 {
                floats(&mut trails, a[body].map(|v| v as f32));
                floats(&mut trails, [0.0]);
                floats(&mut trails, b[body].map(|v| v as f32));
                floats(&mut trails, [0.0]);
                floats(&mut trails, COLORS[body]);
                floats(
                    &mut trails,
                    [0.2 + 0.8 * (sample + 1) as f32 / length.max(1) as f32],
                );
            }
        }
        Self {
            bodies,
            trails,
            segments: (length * 3) as u32,
        }
    }
}

pub struct Orbit3dCallback {
    pub frame: Arc<Geometry>,
    pub uniform: Vec<u8>,
    pub grid: bool,
    pub trail_samples: u32,
}

#[derive(Resource)]
struct Gpu3d {
    format: wgpu::TextureFormat,
    size: [u32; 2],
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    uniform: wgpu::Buffer,
    bodies: wgpu::Buffer,
    trails: wgpu::Buffer,
    group: wgpu::BindGroup,
    blit_group: wgpu::BindGroup,
    spheres: wgpu::RenderPipeline,
    lines: wgpu::RenderPipeline,
    grid: wgpu::RenderPipeline,
    blit: wgpu::RenderPipeline,
    frame: Option<Arc<Geometry>>,
}

impl Gpu3d {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, size: [u32; 2]) -> Self {
        let texture = |format, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("3D scene target"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let color = texture(
            format,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let depth = texture(
            wgpu::TextureFormat::Depth32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let buffer = |size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("3D data"),
                size,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let uniform = buffer(128, wgpu::BufferUsages::UNIFORM);
        let bodies = buffer(48, wgpu::BufferUsages::STORAGE);
        let trails = buffer(4095 * 3 * 48, wgpu::BufferUsages::STORAGE);
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("3D scene layout"),
            entries: &std::array::from_fn::<_, 3, _>(|binding| wgpu::BindGroupLayoutEntry {
                binding: binding as u32,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: if binding == 0 {
                        wgpu::BufferBindingType::Uniform
                    } else {
                        wgpu::BufferBindingType::Storage { read_only: true }
                    },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }),
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("3D scene data"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: bodies.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: trails.as_entire_binding(),
                },
            ],
        });
        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("3D composite layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let blit_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("3D composite image"),
            layout: &blit_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("3D gravity scene"),
            source: wgpu::ShaderSource::Wgsl(include_str!("render.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("3D pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let composite_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("3D composite pipeline layout"),
            bind_group_layouts: &[Some(&layout), Some(&blit_layout)],
            immediate_size: 0,
        });
        let pipeline = |vertex, fragment, depth: bool, write: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(vertex),
                layout: Some(if depth {
                    &pipeline_layout
                } else {
                    &composite_layout
                }),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vertex),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: depth.then_some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(write),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            format,
            size,
            color,
            depth,
            uniform,
            bodies,
            trails,
            group,
            blit_group,
            spheres: pipeline("sphere_vs", "sphere_fs", true, true),
            lines: pipeline("trail_vs", "line_fs", true, false),
            grid: pipeline("grid_vs", "line_fs", true, false),
            blit: pipeline("blit_vs", "blit_fs", false, false),
            frame: None,
        }
    }

    fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, callback: &Orbit3dCallback) {
        if self
            .frame
            .as_ref()
            .is_none_or(|frame| !Arc::ptr_eq(frame, &callback.frame))
        {
            queue.write_buffer(&self.bodies, 0, &callback.frame.bodies);
            if !callback.frame.trails.is_empty() {
                queue.write_buffer(&self.trails, 0, &callback.frame.trails);
            }
            self.frame = Some(callback.frame.clone());
        }
        let mut uniform = callback.uniform.clone();
        uniform[112..116].copy_from_slice(&u32::from(self.format.is_srgb()).to_le_bytes());
        queue.write_buffer(&self.uniform, 0, &uniform);
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("3D depth render"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.color,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(if self.format.is_srgb() {
                            wgpu::Color {
                                r: 0.0025,
                                g: 0.004,
                                b: 0.008,
                                a: 1.0,
                            }
                        } else {
                            wgpu::Color {
                                r: 0.032,
                                g: 0.051,
                                b: 0.086,
                                a: 1.0,
                            }
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_bind_group(0, &self.group, &[]);
            // Do not bind the sampled color target while rendering into it.
            pass.set_pipeline(&self.spheres);
            pass.draw(0..16 * 24 * 6, 0..3);
            if callback.grid {
                pass.set_pipeline(&self.grid);
                pass.draw(0..6, 0..45);
            }
            pass.set_pipeline(&self.lines);
            let count = callback.trail_samples.saturating_sub(1) * 3;
            pass.draw(
                0..6,
                callback.frame.segments.saturating_sub(count)..callback.frame.segments,
            );
        }
        queue.submit([encoder.finish()]);
    }
}

impl EguiBevyPaintCallbackImpl for Orbit3dCallback {
    fn update(
        &self,
        info: egui::PaintCallbackInfo,
        _entity: RenderEntity,
        key: EguiPipelineKey,
        world: &mut World,
    ) {
        let limit = world
            .resource::<RenderDevice>()
            .limits()
            .max_texture_dimension_2d
            .min(4096) as f32;
        let pixels = info.viewport.size() * info.pixels_per_point;
        let scale = (limit / pixels.x.max(pixels.y).max(1.0)).min(1.0);
        let size = [
            (pixels.x * scale).round().max(1.0) as u32,
            (pixels.y * scale).round().max(1.0) as u32,
        ];
        if world
            .get_resource::<Gpu3d>()
            .is_none_or(|gpu| gpu.format != key.target_format || gpu.size != size)
        {
            let gpu = Gpu3d::new(
                world.resource::<RenderDevice>().wgpu_device(),
                key.target_format,
                size,
            );
            world.insert_resource(gpu);
        }
        world.resource_scope(|world, mut gpu: bevy::prelude::Mut<Gpu3d>| {
            gpu.prepare(
                world.resource::<RenderDevice>().wgpu_device(),
                world.resource::<RenderQueue>(),
                self,
            );
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
        if let Some(gpu) = world.get_resource::<Gpu3d>() {
            let pass = pass.wgpu_pass();
            pass.set_pipeline(&gpu.blit);
            pass.set_bind_group(0, &gpu.group, &[]);
            pass.set_bind_group(1, &gpu.blit_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
