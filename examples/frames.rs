//! Writes a stretch of a 100,000-disk pour to standard output as raw RGBA frames,
//! for an encoder: `frames FROM TO STEPS_PER_FRAME PIXELS [PAINT.rgba]`.
//!
//! FROM and TO are simulated seconds. There are 900 steps per second, so 15 steps
//! per frame plays in real time at 60 frames per second, 3 is slow motion and 60
//! a time lapse. PAINT is a table from `paint`, read instead of coloring by age.
//! RAYON_NUM_THREADS changes the thread count, which must not change a bit: the
//! state's fingerprint is reported on standard error at the end.

use diskpour::offscreen::Offscreen;
use diskpour::pour::Pour;
use diskpour::render::Palette;
use diskpour::world::World;
use std::io::Write;

const DISKS: u32 = 100_000;
const STEPS_PER_SECOND: u32 = 900;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let [from, to, steps_per_frame, pixels, rest @ ..] = &arguments[..] else {
        panic!("usage: frames FROM TO STEPS_PER_FRAME PIXELS [PAINT.rgba]");
    };
    let step_at = |seconds: &str| -> u32 {
        let seconds: f64 = seconds.parse().expect("times should be numbers");
        (seconds * f64::from(STEPS_PER_SECOND)).round() as u32
    };
    let (first, last) = (step_at(from), step_at(to));
    let steps_per_frame: u32 = steps_per_frame.parse().expect("STEPS_PER_FRAME");
    let pixels: u32 = pixels.parse().expect("PIXELS");
    let paint: Option<Vec<[u8; 4]>> = rest.first().map(|path| {
        let bytes = std::fs::read(path).expect("PAINT should be readable");
        bytemuck::cast_slice::<u8, [u8; 4]>(&bytes).to_vec()
    });
    let palette = paint.as_deref().map_or(Palette::Age, Palette::Paint);

    let pour = Pour::new(DISKS, STEPS_PER_SECOND);
    let mut world = World::new(pour.arena);
    let offscreen = Offscreen::new(&pour, palette, pixels);
    let advance_to = |world: &mut World, step: u32| {
        rayon::scope(|_| {
            while world.now() < step {
                pour.advance(world);
            }
        });
    };

    advance_to(&mut world, first);
    let mut output = std::io::stdout().lock();
    let frames = (last - first) / steps_per_frame + 1;
    for frame in 0..frames {
        advance_to(&mut world, first + frame * steps_per_frame);
        output
            .write_all(&offscreen.draw(&world))
            .expect("standard output should accept the frame");
        if frame % 300 == 0 {
            eprintln!("frame {frame} of {frames}");
        }
    }
    eprintln!(
        "{frames} frames, {} threads, step {}, fingerprint {:016x}",
        rayon::current_num_threads(),
        world.now(),
        world.fingerprint(),
    );
}
