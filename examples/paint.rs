//! Colors each disk by where it ends up: `paint SECONDS IMAGE.png OUTPUT.rgba`.
//!
//! Runs a 100,000-disk pour to SECONDS and looks up each disk's place in the image,
//! which covers the whole box, and writes one RGBA color per serial number. Replayed
//! through `frames`, the picture forms by itself, and it forms sharply only if the
//! replay reproduces this run exactly. Transparent parts of the image show the backdrop.

use diskpour::pour::Pour;
use diskpour::world::World;

const DISKS: u32 = 100_000;
const STEPS_PER_SECOND: u32 = 900;
const BACKDROP: [u8; 3] = [64, 64, 64];

/// An image as RGBA bytes.
struct Image {
    width: usize,
    height: usize,
    rgba: Vec<u8>,
}

fn read_image(path: &str) -> Image {
    let file = std::fs::File::open(path).expect("IMAGE should be readable");
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(
        png::Transformations::normalize_to_color8() | png::Transformations::ALPHA,
    );
    let mut reader = decoder.read_info().expect("IMAGE should be a PNG");
    let size = reader
        .output_buffer_size()
        .expect("IMAGE should fit in memory");
    let mut rgba = vec![0; size];
    let info = reader.next_frame(&mut rgba).expect("IMAGE should decode");
    assert_eq!(
        info.color_type,
        png::ColorType::Rgba,
        "IMAGE should be color"
    );
    Image {
        width: info.width as usize,
        height: info.height as usize,
        rgba,
    }
}

impl Image {
    /// The color at a place in the box, given as fractions of its side from the
    /// bottom-left corner, over the backdrop.
    fn color_at(&self, x: f64, y: f64) -> [u8; 4] {
        let column = ((x * self.width as f64) as usize).min(self.width - 1);
        let row = (((1.0 - y) * self.height as f64) as usize).min(self.height - 1);
        let texel = &self.rgba[(row * self.width + column) * 4..][..4];
        let alpha = u32::from(texel[3]);
        let over = |channel: usize| {
            let (above, below) = (u32::from(texel[channel]), u32::from(BACKDROP[channel]));
            ((above * alpha + below * (255 - alpha) + 127) / 255) as u8
        };
        [over(0), over(1), over(2), 255]
    }
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let [seconds, image, output] = &arguments[..] else {
        panic!("usage: paint SECONDS IMAGE.png OUTPUT.rgba");
    };
    let seconds: f64 = seconds.parse().expect("SECONDS should be a number");
    let image = read_image(image);

    let pour = Pour::new(DISKS, STEPS_PER_SECOND);
    let mut world = World::new(pour.arena);
    let last = (seconds * f64::from(STEPS_PER_SECOND)).round() as u32;
    rayon::scope(|_| {
        while world.now() < last {
            pour.advance(&mut world);
        }
    });

    let side = f64::from(pour.side());
    let mut table = vec![[BACKDROP[0], BACKDROP[1], BACKDROP[2], 255]; DISKS as usize];
    for disk in world.disks() {
        let [x, y] = disk.position.map(|c| f64::from(c) / side);
        table[disk.serial as usize] = image.color_at(x, y);
    }
    std::fs::write(output, bytemuck::cast_slice::<[u8; 4], u8>(&table))
        .expect("OUTPUT should be writable");
    eprintln!(
        "{} threads, step {}, fingerprint {:016x}",
        rayon::current_num_threads(),
        world.now(),
        world.fingerprint(),
    );
}
