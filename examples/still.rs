//! Runs a pour without a window, draws one frame to a PNG, and reports the cost
//! of the physics: `still SECONDS OUTPUT.png [DISKS] [STEPS_PER_SECOND] [PIXELS]`.

use diskpour::camera::Camera;
use diskpour::offscreen::Offscreen;
use diskpour::pour::Pour;
use diskpour::render::Palette;
use diskpour::world::World;
use std::time::Instant;

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
    let (disks, steps_per_second, pixels) = (count(0, 100_000), count(1, 900), count(2, 889));

    let pour = Pour::new(disks, steps_per_second);
    let mut world = World::new(pour.arena);
    let steps = (seconds * f64::from(steps_per_second)) as u32;
    println!(
        "{} disks in a {}-diameter box, {} rows every {} steps, gravity {} units",
        pour.disks, pour.arena.columns, pour.rows, pour.period, pour.arena.gravity,
    );

    // The cost grows with the number of disks, so it is reported once per second
    // of simulated time, as milliseconds per 60 Hz frame.
    // Steps run on a worker thread, as in the window, so the costs are comparable.
    rayon::scope(|_| {
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
    });

    let image =
        Offscreen::new(&pour, Palette::Age, pixels).draw(&world, Camera::whole(pour.side()));
    write_png(output, pixels, &image);
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
