//! Draws a world into any color target with wgpu.
//!
//! The disk array is uploaded as it is, one instance per disk: the vertex shader
//! reads each disk's position and birth directly and derives its color from age.
//! The target must be display-referred (not sRGB-encoding), since the palette is
//! written in the display values of the reference picture.

use crate::disk::{DIAMETER, Disk, RADIUS};
use crate::pour::Pour;
use crate::world::World;
use bytemuck::{Pod, Zeroable};
use std::mem::{offset_of, size_of};
use wgpu::util::{BufferInitDescriptor, DeviceExt};

/// The black frame around the box, as a fraction of the target's shorter side.
const FRAME: f32 = 12.0 / 889.0;

/// The uniform block of the shader.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct View {
    clip_per_unit: f32,
    pixels_per_unit: f32,
    radius: f32,
    crossings_per_step: f32,
    now: u32,
}

/// The GPU state that draws one pour.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    box_pipeline: wgpu::RenderPipeline,
    disk_pipeline: wgpu::RenderPipeline,
    instances: wgpu::Buffer,
    view: wgpu::Buffer,
    bindings: wgpu::BindGroup,
    side: f32,
    crossings_per_step: f32,
}

impl Renderer {
    /// Prepares to draw `pour` into targets of `format`.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        pour: &Pour,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("disk.wgsl"));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("view"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("view"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let attributes = [
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Sint32x2,
                offset: offset_of!(Disk, position) as u64,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint32,
                offset: offset_of!(Disk, birth) as u64,
                shader_location: 1,
            },
        ];
        let disk_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<Disk>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &attributes,
        })];
        let strip = |entry: (&'static str, &'static str),
                     buffers: &[Option<wgpu::VertexBufferLayout>],
                     blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry.0),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(entry.0),
                    compilation_options: Default::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry.1),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let box_pipeline = strip(("box_vertex", "box_fragment"), &[], None);
        let disk_pipeline = strip(
            ("disk_vertex", "disk_fragment"),
            &disk_buffers,
            Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        );

        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("disks"),
            size: u64::from(pour.disks) * size_of::<Disk>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("view"),
            contents: bytemuck::bytes_of(&View::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("view"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view.as_entire_binding(),
            }],
        });
        Self {
            device: device.clone(),
            queue: queue.clone(),
            box_pipeline,
            disk_pipeline,
            instances,
            view,
            bindings,
            side: pour.arena.columns as f32 * DIAMETER as f32,
            crossings_per_step: pour.muzzle_speed as f32 / pour.side() as f32,
        }
    }

    /// Draws `world` into `target`, whose size is `[width, height]` in pixels.
    pub fn draw(&self, target: &wgpu::TextureView, size: [u32; 2], world: &World) {
        let [width, height] = size.map(|pixels| pixels as f32);
        let shorter = width.min(height);
        let box_pixels = shorter * (1.0 - 2.0 * FRAME);
        let view = View {
            clip_per_unit: 2.0 / self.side,
            pixels_per_unit: box_pixels / self.side,
            radius: RADIUS as f32,
            crossings_per_step: self.crossings_per_step,
            now: world.now(),
        };
        self.queue
            .write_buffer(&self.view, 0, bytemuck::bytes_of(&view));
        self.queue
            .write_buffer(&self.instances, 0, bytemuck::cast_slice(world.disks()));

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_viewport(
                (width - box_pixels) / 2.0,
                (height - box_pixels) / 2.0,
                box_pixels,
                box_pixels,
                0.0,
                1.0,
            );
            pass.set_bind_group(0, &self.bindings, &[]);
            pass.set_pipeline(&self.box_pipeline);
            pass.draw(0..4, 0..1);
            pass.set_pipeline(&self.disk_pipeline);
            pass.set_vertex_buffer(0, self.instances.slice(..));
            pass.draw(0..4, 0..world.disks().len() as u32);
        }
        self.queue.submit([encoder.finish()]);
    }
}
