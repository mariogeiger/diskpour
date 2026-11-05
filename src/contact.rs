//! The push one disk receives from the disks it touches.
//!
//! A touching pair is moved apart along its axis by a fraction of its overlap,
//! and loses a fraction of its relative velocity along that axis. The pair's push
//! is truncated to an integer, and truncation is an odd function, so the push on
//! `i` from `j` is exactly minus the push on `j` from `i`: Newton's third law
//! holds to the last bit. Integer sums commute, so the result cannot depend on
//! neighbour order or thread count.
//!
//! The kernel runs on whole vectors with no branch: disks that do not touch, the
//! disk itself included, are masked to exactly zero. Its float operations are
//! correctly rounded and never fused, so every machine computes the same bits.

use crate::disk::DIAMETER;
use wide::{f32x4, i32x4};

/// Fraction of a touching pair's overlap removed per step.
///
/// All pairs are solved at once, so with `w = STIFFNESS` and `d = DAMPING` a mode
/// of the contact matrix with eigenvalue `e` follows the Verlet recurrence
/// `z^2 - (2 - (2w + d)e) z + (1 - (w + d)e) = 0`. A lone pair has `e = 1` and a
/// hexagonal lattice reaches `e = 3`, where the roots stay in the unit circle iff
/// `w + d < 2/3` and `3w + 2d < 4/3`. These values give 0.5 and 1.3.
const STIFFNESS: f32 = 0.3;

/// Fraction of a touching pair's relative normal velocity removed per step.
///
/// Without it a lone pair rebounds with restitution 0.3, and pressure released at
/// a free surface sprays disks across the box. These values give restitution 0.14.
const DAMPING: f32 = 0.2;

/// The number of candidates processed at once, one 128-bit vector of `f32`.
pub const LANES: usize = 4;

/// Position units to diameters, an exact power of two.
const DIAMETERS_PER_UNIT: f32 = 1.0 / DIAMETER as f32;

/// A disk's predicted position and its stride, the displacement that led there.
#[derive(Clone, Copy, Debug)]
pub struct Motion {
    pub x: i32,
    pub y: i32,
    pub u: i32,
    pub v: i32,
}

/// A run of `LANES` candidates stored side by side.
#[derive(Clone, Copy)]
pub struct Block<'a> {
    pub x: &'a [i32; LANES],
    pub y: &'a [i32; LANES],
    pub u: &'a [i32; LANES],
    pub v: &'a [i32; LANES],
}

/// The running push on one disk, one partial sum per lane.
#[derive(Default)]
pub struct PushSum {
    x: i32x4,
    y: i32x4,
}

impl PushSum {
    /// Adds the pushes on `me` from a block of candidates.
    #[inline(always)]
    pub fn add_block(&mut self, me: Motion, block: Block<'_>) {
        let difference = |mine: i32, theirs: &[i32; LANES]| {
            (i32x4::splat(mine) - i32x4::new(*theirs)).round_float()
        };
        let dx = difference(me.x, block.x);
        let dy = difference(me.y, block.y);
        let du = difference(me.u, block.u);
        let dv = difference(me.v, block.v);

        let unit = f32x4::splat(DIAMETERS_PER_UNIT);
        let x = dx * unit;
        let y = dy * unit;
        let squared = x * x + y * y;
        let touching = squared.simd_gt(f32x4::ZERO) & squared.simd_lt(f32x4::ONE);
        let inverse_squared = f32x4::ONE / squared;
        let overlap_per_distance = inverse_squared.sqrt() - f32x4::ONE;
        let approach_per_distance = (du * x + dv * y) * unit * inverse_squared;
        let share = (f32x4::splat(0.5 * STIFFNESS) * overlap_per_distance
            - f32x4::splat(0.5 * DAMPING) * approach_per_distance)
            & touching;

        self.x += (share * dx).fast_trunc_int();
        self.y += (share * dy).fast_trunc_int();
    }

    /// Returns the total push.
    #[inline(always)]
    pub fn total(&self) -> [i32; 2] {
        [self.x.reduce_add(), self.y.reduce_add()]
    }
}
