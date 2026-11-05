//! Draws a world into memory instead of a window.
//!
//! The target is display-referred RGBA, so the bytes are what a screen would show.
//! Everything that makes pictures without a window goes through here.

use crate::pour::Pour;
use crate::render::{Palette, Renderer};
use crate::world::World;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// A square image in GPU memory and the means to read it back.
pub struct Offscreen {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    texture: wgpu::Texture,
    target: wgpu::TextureView,
    readback: wgpu::Buffer,
    pixels: u32,
    row_stride: u32,
}

impl Offscreen {
    /// Prepares to draw `pour` into an image of `pixels` by `pixels`. The whole GPU
    /// is requested, because a zoom into the pile wants a very large image.
    pub fn new(pour: &Pour, palette: Palette<'_>, pixels: u32) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .expect("a GPU should be available");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .expect("the GPU should provide a device");
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d {
                width: pixels,
                height: pixels,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target = texture.create_view(&Default::default());
        let row_stride = (pixels * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(row_stride) * u64::from(pixels),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let renderer = Renderer::new(&device, &queue, FORMAT, pour, palette);
        Self {
            device,
            queue,
            renderer,
            texture,
            target,
            readback,
            pixels,
            row_stride,
        }
    }

    /// Draws `world` and returns tightly packed RGBA bytes, top row first.
    pub fn draw(&self, world: &World) -> Vec<u8> {
        let size = [self.pixels, self.pixels];
        self.renderer.draw(&self.target, size, world);

        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            self.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.row_stride),
                    rows_per_image: None,
                },
            },
            self.texture.size(),
        );
        self.queue.submit([encoder.finish()]);
        let bytes = self.readback.slice(..);
        bytes.map_async(wgpu::MapMode::Read, |result| {
            result.expect("the readback buffer should map");
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("the GPU should finish the frame");

        let row_bytes = self.pixels as usize * 4;
        let image = {
            let mapped = bytes
                .get_mapped_range()
                .expect("the readback buffer should be mapped");
            let mut image = Vec::with_capacity(row_bytes * self.pixels as usize);
            for row in mapped.chunks_exact(self.row_stride as usize) {
                image.extend_from_slice(&row[..row_bytes]);
            }
            image
        };
        self.readback.unmap();
        image
    }
}
