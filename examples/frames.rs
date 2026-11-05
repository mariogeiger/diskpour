//! Writes a stretch of a 100,000-disk pour to standard output as raw RGBA frames,
//! for an encoder:
//! `frames FROM TO STEPS_PER_FRAME PIXELS [--steps-per-second N] [--paint PAINT.rgba]
//! [--camera SECONDS=X,Y,ZOOM ...]`.
//!
//! FROM and TO are simulated seconds. At the default 900 steps per second, 15 steps
//! per frame plays in real time at 60 frames per second, 3 is slow motion and 60 a
//! time lapse. PAINT is a table from `paint`, read instead of coloring by age. The
//! camera moves gently through views given as seconds into the clip, a place as
//! fractions of the box from its bottom-left corner, and a zoom, 1 being the whole box.
//! RAYON_NUM_THREADS changes the thread count, which must not change a bit: the
//! state's fingerprint is reported on standard error at the end.

use diskpour::camera::{Camera, CameraPath};
use diskpour::offscreen::Offscreen;
use diskpour::pour::Pour;
use diskpour::render::Palette;
use diskpour::world::World;
use std::io::Write;

const DISKS: u32 = 100_000;
const FRAMES_PER_SECOND: f64 = 60.0;

/// The options after the positional arguments.
struct Options {
    steps_per_second: u32,
    paint: Option<Vec<[u8; 4]>>,
    camera_keys: Vec<String>,
}

fn parse_options(arguments: &[String]) -> Options {
    let mut options = Options {
        steps_per_second: 900,
        paint: None,
        camera_keys: Vec::new(),
    };
    let mut rest = arguments.iter();
    while let Some(flag) = rest.next() {
        match flag.as_str() {
            "--steps-per-second" => {
                let value = rest.next().expect("--steps-per-second needs a value");
                options.steps_per_second = value.parse().expect("--steps-per-second");
            }
            "--paint" => {
                let path = rest.next().expect("--paint needs a file");
                let bytes = std::fs::read(path).expect("PAINT should be readable");
                options.paint = Some(bytemuck::cast_slice::<u8, [u8; 4]>(&bytes).to_vec());
            }
            "--camera" => options.camera_keys.extend(rest.by_ref().cloned()),
            other => panic!("unknown option {other}"),
        }
    }
    options
}

/// Reads `SECONDS=X,Y,ZOOM`.
fn parse_camera_key(key: &str, box_side: i32) -> (f64, Camera) {
    let number = |text: &str| -> f64 { text.parse().expect("camera numbers") };
    let (seconds, view) = key.split_once('=').expect("SECONDS=X,Y,ZOOM");
    let parts: Vec<f64> = view.split(',').map(number).collect();
    let [x, y, zoom] = parts[..] else {
        panic!("a camera view is X,Y,ZOOM");
    };
    (number(seconds), Camera::zoomed(box_side, [x, y], zoom))
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let [from, to, steps_per_frame, pixels, rest @ ..] = &arguments[..] else {
        panic!("usage: frames FROM TO STEPS_PER_FRAME PIXELS [OPTIONS]");
    };
    let options = parse_options(rest);
    let step_at = |seconds: &str| -> u32 {
        let seconds: f64 = seconds.parse().expect("times should be numbers");
        (seconds * f64::from(options.steps_per_second)).round() as u32
    };
    let (first, last) = (step_at(from), step_at(to));
    let steps_per_frame: u32 = steps_per_frame.parse().expect("STEPS_PER_FRAME");
    let pixels: u32 = pixels.parse().expect("PIXELS");
    let palette = options
        .paint
        .as_deref()
        .map_or(Palette::Age, Palette::Paint);

    let pour = Pour::new(DISKS, options.steps_per_second);
    let camera = CameraPath::new(if options.camera_keys.is_empty() {
        vec![(0.0, Camera::whole(pour.side()))]
    } else {
        let parse = |key: &String| parse_camera_key(key, pour.side());
        options.camera_keys.iter().map(parse).collect()
    });
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
        let view = camera.at(f64::from(frame) / FRAMES_PER_SECOND);
        output
            .write_all(&offscreen.draw(&world, view))
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
