//! Colors each disk by the orientation of the hexagonal lattice around it:
//! `grains SECONDS OUTPUT.rgba`, a table for `frames --paint`.
//!
//! The orientation is the angle of the bond-orientational order parameter psi-6, the
//! mean of exp(6i theta) over a disk's neighbours. A settled pile turns out to be a
//! polycrystal: each grain shows one hue, and disks outside a clean lattice, on grain
//! boundaries and around vacancies, show white. Reports how many disks are which.

use diskpour::disk::{DIAMETER, column_of};
use diskpour::pour::Pour;
use diskpour::world::World;

const DISKS: u32 = 100_000;
const STEPS_PER_SECOND: u32 = 900;
/// Neighbours lie closer than this, in diameters: between the first two distances of
/// the hexagonal lattice, 1 and sqrt(3).
const NEIGHBOUR_DISTANCE: f64 = 1.32;
/// A disk in a clean lattice has six neighbours and at least this order.
const CLEAN_ORDER: f64 = 0.6;
const OUTSIDE_THE_LATTICE: [u8; 4] = [245, 245, 245, 255];

/// A fully saturated color of `hue`, a fraction of the color wheel, at brightness
/// `value`.
fn color_of(hue: f64, value: f64) -> [u8; 4] {
    let channel = |offset: f64| {
        let k = (offset + hue * 6.0) % 6.0;
        ((value - value * 0.85 * k.min(4.0 - k).clamp(0.0, 1.0)) * 255.0) as u8
    };
    [channel(5.0), channel(3.0), channel(1.0), 255]
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let [seconds, output] = &arguments[..] else {
        panic!("usage: grains SECONDS OUTPUT.rgba");
    };
    let seconds: f64 = seconds.parse().expect("SECONDS should be a number");

    let pour = Pour::new(DISKS, STEPS_PER_SECOND);
    let mut world = World::new(pour.arena);
    let last = (seconds * f64::from(STEPS_PER_SECOND)).round() as u32;
    rayon::scope(|_| {
        while world.now() < last {
            pour.advance(&mut world);
        }
    });
    let disks = world.disks();

    let columns = pour.arena.columns as usize;
    let cell_of = |c: i32| column_of(c).min(columns - 1);
    let mut cells = vec![Vec::new(); columns * columns];
    for (index, disk) in disks.iter().enumerate() {
        cells[cell_of(disk.position[1]) * columns + cell_of(disk.position[0])].push(index);
    }

    let reach = (NEIGHBOUR_DISTANCE * f64::from(DIAMETER)).powi(2);
    let mut table = vec![OUTSIDE_THE_LATTICE; DISKS as usize];
    let mut clean = 0;
    let mut with_six = 0;
    for (index, disk) in disks.iter().enumerate() {
        let (column, row) = (cell_of(disk.position[0]), cell_of(disk.position[1]));
        let (mut real, mut imaginary, mut neighbours) = (0.0, 0.0, 0);
        for y in row.saturating_sub(2)..=(row + 2).min(columns - 1) {
            for x in column.saturating_sub(2)..=(column + 2).min(columns - 1) {
                for &other in &cells[y * columns + x] {
                    let dx = f64::from(disks[other].position[0] - disk.position[0]);
                    let dy = f64::from(disks[other].position[1] - disk.position[1]);
                    if other != index && dx * dx + dy * dy < reach {
                        let angle = dy.atan2(dx);
                        real += (6.0 * angle).cos();
                        imaginary += (6.0 * angle).sin();
                        neighbours += 1;
                    }
                }
            }
        }
        with_six += usize::from(neighbours == 6);
        let order = f64::hypot(real, imaginary) / f64::from(neighbours.max(1));
        if neighbours == 6 && order > CLEAN_ORDER {
            clean += 1;
            let hue = f64::atan2(imaginary, real) / std::f64::consts::TAU + 0.5;
            table[disk.serial as usize] = color_of(hue, 0.35 + 0.65 * order);
        }
    }
    std::fs::write(output, bytemuck::cast_slice::<[u8; 4], u8>(&table))
        .expect("OUTPUT should be writable");
    let percent = |count: usize| 100.0 * count as f64 / disks.len() as f64;
    println!(
        "{:.1}% of disks have six neighbours, {:.1}% lie in a clean lattice",
        percent(with_six),
        percent(clean),
    );
}
