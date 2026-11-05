//! One disk and the fixed-point length unit every position is counted in.
//!
//! Physics reads positions only through their differences, so they form an
//! affine space. Integers represent that exactly: resolution is the same
//! everywhere in the box, and Verlet's `2x - x_prev + g` never rounds. In `f32`
//! the same sum would quantize gravity to a few units in the last place.

use bytemuck::{Pod, Zeroable};

/// log2 of the disk diameter in position units.
pub const DIAMETER_BITS: u32 = 20;
/// The disk diameter in position units.
pub const DIAMETER: i32 = 1 << DIAMETER_BITS;
/// The disk radius in position units.
pub const RADIUS: i32 = DIAMETER / 2;

/// A disk between two solves. The layout is shared with the vertex shader.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Pod, Zeroable)]
pub struct Disk {
    /// Where inertia and gravity carry it at the next step, before contacts.
    pub predicted: [i32; 2],
    /// Where it is at the current step, with contacts solved.
    pub position: [i32; 2],
    /// Its place in the order of emission. It names the disk for life, because the
    /// disks change places in the array at every step.
    pub serial: u32,
}

/// Returns the column of a horizontal position.
#[inline(always)]
pub fn column_of(x: i32) -> usize {
    (x >> DIAMETER_BITS) as usize
}
