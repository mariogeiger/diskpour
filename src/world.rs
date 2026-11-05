//! A square box of disks advanced by position Verlet with one contact solve.
//!
//! A step reads every disk's predicted position and writes its solved position
//! and next prediction into a second array, in column order. Nothing is mutated
//! while being read, so the step is free of solve-order bias: mirroring the box
//! mirrors the motion bit for bit.
//!
//! Many small steps with one solve each are the design: a position-based solve is
//! a stiff spring of stiffness proportional to 1/h^2, so shrinking the step
//! stiffens the disks quadratically while the cost grows only linearly.

use crate::columns::{Column, Columns};
use crate::contact::{LANES, PushSum};
use crate::disk::{DIAMETER, DIAMETER_BITS, Disk, RADIUS};
use bytemuck::Zeroable;
use rayon::prelude::*;

/// A disk touching a wall loses one part in this of its speed along the wall per
/// step. Without it the corners focus the flow into disks that shoot up the bare
/// walls, reach the emitter, and scatter the newborn stream. Integer division
/// rounds toward zero, which keeps the rule mirror symmetric.
const WALL_FRICTION_DIVISOR: i32 = 4096;
/// The offset basis of the 64-bit FNV-1a hash.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
/// The prime of the 64-bit FNV-1a hash.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// A square box with a uniform downward acceleration.
#[derive(Clone, Copy, Debug)]
pub struct Arena {
    /// The side of the box in diameters.
    pub columns: u32,
    /// The downward acceleration in position units per step squared.
    pub gravity: i32,
}

impl Arena {
    /// The largest coordinate a disk center can take.
    #[inline(always)]
    fn far_wall(self) -> i32 {
        ((self.columns as i32) << DIAMETER_BITS) - RADIUS
    }
}

/// The disks in an arena at one step.
pub struct World {
    arena: Arena,
    now: u32,
    disks: Vec<Disk>,
    solved: Vec<Disk>,
    columns: Columns,
}

impl World {
    pub fn new(arena: Arena) -> Self {
        assert!(
            i64::from(arena.columns) << (DIAMETER_BITS + 2) <= i64::from(i32::MAX),
            "positions and their Verlet sums must fit in i32"
        );
        Self {
            arena,
            now: 0,
            disks: Vec::new(),
            solved: Vec::new(),
            columns: Columns::new(arena.columns as usize),
        }
    }

    /// The number of steps taken.
    pub fn now(&self) -> u32 {
        self.now
    }

    pub fn arena(&self) -> Arena {
        self.arena
    }

    /// The disks in storage order, which changes at every step.
    pub fn disks(&self) -> &[Disk] {
        &self.disks
    }

    /// A hash of every bit of the state, for telling whether two runs agree.
    pub fn fingerprint(&self) -> u64 {
        bytemuck::cast_slice::<Disk, u8>(&self.disks)
            .iter()
            .fold(FNV_OFFSET, |hash, &byte| {
                (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
            })
    }

    /// Adds disks, bringing any that lie outside the box back inside.
    pub fn insert(&mut self, disks: impl IntoIterator<Item = Disk>) {
        let far = self.arena.far_wall();
        self.disks.extend(disks.into_iter().map(|disk| Disk {
            predicted: disk.predicted.map(|c| c.clamp(RADIUS, far)),
            position: disk.position.map(|c| c.clamp(RADIUS, far)),
            serial: disk.serial,
        }));
    }

    /// Advances the world by one step.
    pub fn step(&mut self) {
        self.columns.rebuild(&self.disks);
        self.solved.resize(self.disks.len(), Disk::zeroed());

        let mut unwritten = self.solved.as_mut_slice();
        let outputs: Vec<&mut [Disk]> = self
            .columns
            .lengths()
            .map(|length| {
                let (output, rest) = std::mem::take(&mut unwritten).split_at_mut(length);
                unwritten = rest;
                output
            })
            .collect();

        let (arena, disks) = (self.arena, &self.disks[..]);
        self.columns
            .with_neighbours()
            .zip(outputs)
            .with_min_len(4)
            .for_each(|(neighbours, output)| solve_column(arena, neighbours, disks, output));

        std::mem::swap(&mut self.disks, &mut self.solved);
        self.now = self.now.wrapping_add(1);
    }
}

/// Solves the middle of three adjacent columns, writing its disks to `output` in
/// height order.
fn solve_column(arena: Arena, neighbours: &[Column], disks: &[Disk], output: &mut [Disk]) {
    let reachable = [
        neighbours[0].lanes(),
        neighbours[1].lanes(),
        neighbours[2].lanes(),
    ];
    let own = reachable[1];
    let mut lowest = [0usize; 3];
    for (place, (key, output)) in neighbours[1].keys().iter().zip(output).enumerate() {
        let me = own.motion(place);
        let mut push = PushSum::default();
        for (column, lowest) in reachable.iter().zip(&mut lowest) {
            while column.height(*lowest) <= me.y - DIAMETER {
                *lowest += 1;
            }
            let mut candidate = *lowest;
            while column.height(candidate) < me.y + DIAMETER {
                push.add_block(me, column.block(candidate));
                candidate += LANES;
            }
        }
        *output = advance(arena, disks[key.disk as usize], push.total());
    }
}

/// Applies a disk's push and walls, then predicts its next position.
#[inline(always)]
fn advance(arena: Arena, disk: Disk, push: [i32; 2]) -> Disk {
    let far = arena.far_wall();
    let [x, y] = disk.predicted;
    let solved = [
        (x + push[0]).clamp(RADIUS, far),
        (y + push[1]).clamp(RADIUS, far),
    ];
    let velocity = [solved[0] - disk.position[0], solved[1] - disk.position[1]];
    let touching = |coordinate: i32| coordinate == RADIUS || coordinate == far;
    let rubbed = |along: i32, across: i32| {
        if touching(across) {
            along - along / WALL_FRICTION_DIVISOR
        } else {
            along
        }
    };
    let stride = limit_stride([
        solved[0] + rubbed(velocity[0], solved[1]) - x,
        solved[1] + rubbed(velocity[1], solved[0]) - arena.gravity - y,
    ]);
    Disk {
        predicted: [
            (x + stride[0]).clamp(RADIUS, far),
            (y + stride[1]).clamp(RADIUS, far),
        ],
        position: solved,
        serial: disk.serial,
    }
}

/// Keeps a prediction within one diameter of the last one.
///
/// A solve carries information one diameter per step, so this is the speed of
/// light of the scheme: a faster disk would pass through contacts unseen. It is
/// also what lets a column rebuild itself from its neighbours alone.
#[inline(always)]
fn limit_stride(stride: [i32; 2]) -> [i32; 2] {
    let squared = i64::from(stride[0]).pow(2) + i64::from(stride[1]).pow(2);
    if squared <= i64::from(DIAMETER).pow(2) {
        return stride;
    }
    let scale = f64::from(DIAMETER) / (squared as f64).sqrt();
    stride.map(|component| (f64::from(component) * scale) as i32)
}
