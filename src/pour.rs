//! The jet: its integer constants and the lattice columns it emits.
//!
//! The shape of the flow is fixed by dimensionless numbers, so any disk count
//! gives the same picture at a different resolution. Columns are emitted on a
//! whole-step period, every one a time-shifted copy of the first, so the stream
//! in flight is a rigid sheared lattice. Its disks never touch: neighbours in a
//! column keep their vertical spacing, and all others are separated horizontally
//! by at least the column spacing, which is at least one diameter.

use crate::disk::{DIAMETER, Disk};
use crate::world::{Arena, World};

/// `g * L / v^2` for box side `L` and muzzle speed `v`: the parabola's shape.
const TRAJECTORY_CURVATURE: f64 = 1.663;
/// The emitter's distance from the left wall, as a fraction of the box.
const EMITTER_LEFT: f64 = 0.034;
/// The height of the emitter's center, as a fraction of the box.
const EMITTER_HEIGHT: f64 = 0.936;
/// The height of the emitter, as a fraction of the box.
const EMITTER_THICKNESS: f64 = 0.0691;
/// The fraction of the box height the settled disks reach.
const SETTLED_LEVEL: f64 = 0.75;
/// The area of a disk's cell when settled disks pack into a hexagonal lattice, in
/// squared diameters.
const SETTLED_CELL_AREA: f64 = 0.866_025_403_784_438_6;
/// The time a disk takes to cross the box at muzzle speed. Slower is stiffer: it
/// lowers the speed per step, which is what sets how far disks sink into a pile.
const CROSSING_SECONDS: f64 = 4.0;

/// The constants of one pour.
#[derive(Clone, Copy, Debug)]
pub struct Pour {
    /// The number of disks poured in total.
    pub disks: u32,
    /// The box and its gravity.
    pub arena: Arena,
    /// The number of disks in an emitted column.
    pub rows: u32,
    /// The number of steps between emitted columns.
    pub period: u32,
    /// The horizontal muzzle speed in position units per step.
    pub muzzle_speed: i32,
    /// The bottom-left emission site in position units.
    pub origin: [i32; 2],
    /// The number of steps per second of real time.
    pub steps_per_second: u32,
}

impl Pour {
    /// Derives the pour of `disks` disks at a physics rate.
    ///
    /// The rate sets how stiff the disks are: the overlap at the bottom of a pile
    /// falls with its square, while the cost grows only linearly.
    pub fn new(disks: u32, steps_per_second: u32) -> Self {
        let side = (f64::from(disks) * SETTLED_CELL_AREA / SETTLED_LEVEL)
            .sqrt()
            .ceil();
        let unit = f64::from(DIAMETER);

        let rows = (EMITTER_THICKNESS * side).round().max(1.0);
        let period = (CROSSING_SECONDS * f64::from(steps_per_second) / side)
            .round()
            .max(1.0) as i32;
        let muzzle_speed = (DIAMETER + period - 1) / period;

        // Gravity is even so that a disk's first stride of -g/2 is an integer,
        // which makes the discrete trajectory sample the true parabola exactly.
        let speed = f64::from(muzzle_speed);
        let half_gravity = (TRAJECTORY_CURVATURE * speed * speed / (side * unit) / 2.0).round();
        let gravity = 2 * (half_gravity as i32).max(1);

        let bottom = (EMITTER_HEIGHT * side - (rows - 1.0) / 2.0) * unit;
        Self {
            disks,
            arena: Arena {
                columns: side as u32,
                gravity,
            },
            rows: rows as u32,
            period: period as u32,
            muzzle_speed,
            origin: [(EMITTER_LEFT * side * unit) as i32, bottom as i32],
            steps_per_second,
        }
    }

    /// The side of the box in position units.
    pub fn side(&self) -> i32 {
        self.arena.columns as i32 * DIAMETER
    }

    /// The step at which the disk with this serial number was born.
    pub fn birth_of(&self, serial: u32) -> u32 {
        serial / self.rows * self.period
    }

    /// The disks born at step `now`.
    pub fn emitted_at(&self, now: u32) -> impl Iterator<Item = Disk> + use<> {
        let due = now.is_multiple_of(self.period);
        let already = u64::from(now / self.period) * u64::from(self.rows);
        let left = u64::from(self.disks).saturating_sub(already);
        let count = if due {
            left.min(u64::from(self.rows))
        } else {
            0
        };
        let [x, bottom] = self.origin;
        let (speed, half_gravity) = (self.muzzle_speed, self.arena.gravity / 2);
        (0..count as i32).map(move |row| {
            let y = bottom + row * DIAMETER;
            Disk {
                predicted: [x + speed, y - half_gravity],
                position: [x, y],
                serial: (already + row as u64) as u32,
            }
        })
    }

    /// Emits what is due and takes one step.
    pub fn advance(&self, world: &mut World) {
        world.insert(self.emitted_at(world.now()));
        world.step();
    }
}
