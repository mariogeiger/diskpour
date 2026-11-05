//! Runs a pour without a window, draws one frame to a PNG, and reports the cost
//! of the physics: `still SECONDS OUTPUT.png [DISKS] [STEPS_PER_SECOND] [PIXELS]`.

use diskpour::pour::Pour;
use diskpour::render::Renderer;
use diskpour::world::World;
use std::time::Instant;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let [seconds, output, rest @ ..] = &arguments[..] else {
        panic!("usage: still SECONDS OUTPUT.png [DISKS] [STEPS_PER_SECOND] [PIXELS]");
    };
    let seconds: f64 = seconds.parse().expect("SECONDS should be a number");
    let count = |index: usize, default: u32| {
        rest.get(index).map_or(default, |text| {
            text.parse().expect("counts should be whole numbers")
        })
    };
    let (disks, steps_per_second, pixels) = (count(0, 100_000), count(1, 1200), count(2, 889));

    let pour = Pour::new(disks, steps_per_second);
    let mut world = World::new(pour.arena);
    let steps = (seconds * f64::from(steps_per_second)) as u32;
    println!(
        "{} disks in a {}-diameter box, {} rows every {} steps, gravity {} units",
        pour.disks, pour.arena.columns, pour.rows, pour.period, pour.arena.gravity,
    );

    // The cost grows with the number of disks, so it is reported once per second
    // of simulated time, as milliseconds per 60 Hz frame.
    let mut second_started = Instant::now();
    for step in 1..=steps {
        pour.advance(&mut world);
        if step % steps_per_second == 0 || step == steps {
            let span = (step - 1) % steps_per_second + 1;
            let per_frame =
                second_started.elapsed().as_secs_f64() * 1e3 * f64::from(steps_per_second)
                    / 60.0
                    / f64::from(span);
            println!(
                "t={:5.1} s  {:6} disks  {per_frame:6.2} ms per frame",
                f64::from(step) / f64::from(steps_per_second),
                world.disks().len(),
            );
            second_started = Instant::now();
        }
    }

    let image = draw(&pour, &world, pixels);
    write_png(output, pixels, &image);
}

/// Draws the world offscreen and returns tightly packed RGBA bytes.
fn draw(pour: &Pour, world: &World, pixels: u32) -> Vec<u8> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("a GPU should be available");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("the GPU should provide a device");
    let extent = wgpu::Extent3d {
        width: pixels,
        height: pixels,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("still"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target = texture.create_view(&Default::default());
    Renderer::new(&device, &queue, FORMAT, pour).draw(&target, [pixels, pixels], world);

    let row_bytes = pixels * 4;
    let row_stride = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: u64::from(row_stride) * u64::from(pixels),
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
                bytes_per_row: Some(row_stride),
                rows_per_image: None,
            },
        },
        extent,
    );
    queue.submit([encoder.finish()]);
    readback.slice(..).map_async(wgpu::MapMode::Read, |result| {
        result.expect("the readback buffer should map");
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("the GPU should finish the frame");
    let mapped = readback
        .slice(..)
        .get_mapped_range()
        .expect("the readback buffer should be mapped");
    mapped
        .chunks_exact(row_stride as usize)
        .flat_map(|row| &row[..row_bytes as usize])
        .copied()
        .collect()
}

fn write_png(path: &str, pixels: u32, image: &[u8]) {
    let file = std::fs::File::create(path).expect("OUTPUT should be writable");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), pixels, pixels);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("the PNG header should write");
    writer
        .write_image_data(image)
        .expect("the PNG data should write");
}
