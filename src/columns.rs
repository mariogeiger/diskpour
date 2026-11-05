//! Orders disks into one-diameter columns, each sorted by height.
//!
//! Two disks can touch only if their columns differ by at most one and their
//! heights by less than a diameter, so three sliding windows over sorted columns
//! find every contact. Memory scales with disks and columns, never with the area
//! of the box, which a two-dimensional grid would have to clear and scan.
//!
//! No disk moves farther than a diameter per step, so a column is rebuilt by
//! pulling from itself and its two neighbours. Each column is built independently
//! in parallel, and nearly sorted input keeps its insertion sort linear.

use crate::contact::{Block, LANES, Motion};
use crate::disk::{Disk, column_of};
use rayon::prelude::*;
use std::ops::Range;

/// A height no disk reaches, far enough for differences to stay in `i32`.
const ABOVE_EVERYTHING: i32 = 1 << 30;

/// A disk's place in its column.
#[derive(Clone, Copy, Debug)]
pub struct Key {
    /// The predicted height of the disk.
    y: i32,
    /// The index of the disk in the array that was binned.
    pub disk: u32,
}

/// The disks of one column, sorted by height, as side-by-side arrays.
///
/// The arrays continue past the last disk with two blocks' worth of padding above
/// everything, so a block can start at any disk and a height search needs no
/// bounds test. Padding is too far away to touch anything.
///
/// Columns are rebuilt on different threads, so each sits on its own cache lines,
/// which are 128 bytes on Apple silicon.
#[repr(align(128))]
#[derive(Clone, Default)]
pub struct Column {
    keys: Vec<Key>,
    x: Vec<i32>,
    y: Vec<i32>,
    u: Vec<i32>,
    v: Vec<i32>,
}

impl Column {
    fn padded() -> Self {
        let mut column = Self::default();
        column.lay_out(&[]);
        column
    }

    /// The disks of this column in height order.
    pub fn keys(&self) -> &[Key] {
        &self.keys
    }

    /// The arrays of this column, cut to one length so that one bounds test
    /// serves all four.
    #[inline(always)]
    pub fn lanes(&self) -> Lanes<'_> {
        let length = self.y.len();
        Lanes {
            x: &self.x[..length],
            y: &self.y[..length],
            u: &self.u[..length],
            v: &self.v[..length],
        }
    }

    fn pull_keys(&mut self, column: usize, disks: &[Disk], range: Range<usize>) {
        let first = range.start;
        for (offset, disk) in disks[range].iter().enumerate() {
            if column_of(disk.predicted[0]) == column {
                self.keys.push(Key {
                    y: disk.predicted[1],
                    disk: (first + offset) as u32,
                });
            }
        }
    }

    /// A stable insertion sort, linear in keys plus inversions.
    fn sort_keys(&mut self) {
        let keys = &mut self.keys[..];
        for sorted in 1..keys.len() {
            let key = keys[sorted];
            let mut slot = sorted;
            while slot > 0 && keys[slot - 1].y > key.y {
                keys[slot] = keys[slot - 1];
                slot -= 1;
            }
            keys[slot] = key;
        }
    }

    fn lay_out(&mut self, disks: &[Disk]) {
        let count = self.keys.len();
        let length = count + 2 * LANES;
        self.x.resize(length, 0);
        self.y.resize(length, 0);
        self.u.resize(length, 0);
        self.v.resize(length, 0);
        let (x, y) = (&mut self.x[..length], &mut self.y[..length]);
        let (u, v) = (&mut self.u[..length], &mut self.v[..length]);
        for (place, key) in self.keys.iter().enumerate() {
            let disk = disks[key.disk as usize];
            x[place] = disk.predicted[0];
            y[place] = disk.predicted[1];
            u[place] = disk.predicted[0] - disk.position[0];
            v[place] = disk.predicted[1] - disk.position[1];
        }
        x[count..].fill(0);
        y[count..].fill(ABOVE_EVERYTHING);
        u[count..].fill(0);
        v[count..].fill(0);
    }
}

/// The arrays of one column, read-only and all of one length.
#[derive(Clone, Copy)]
pub struct Lanes<'a> {
    x: &'a [i32],
    y: &'a [i32],
    u: &'a [i32],
    v: &'a [i32],
}

impl<'a> Lanes<'a> {
    /// The motion of the disk at a place in the column.
    #[inline(always)]
    pub fn motion(&self, place: usize) -> Motion {
        Motion {
            x: self.x[place],
            y: self.y[place],
            u: self.u[place],
            v: self.v[place],
        }
    }

    /// The height of the disk at a place, or of the padding above the last one.
    #[inline(always)]
    pub fn height(&self, place: usize) -> i32 {
        self.y[place]
    }

    /// The block of candidates starting at a place.
    #[inline(always)]
    pub fn block(&self, place: usize) -> Block<'a> {
        Block {
            x: self.x[place..place + LANES].first_chunk().unwrap(),
            y: self.y[place..place + LANES].first_chunk().unwrap(),
            u: self.u[place..place + LANES].first_chunk().unwrap(),
            v: self.v[place..place + LANES].first_chunk().unwrap(),
        }
    }
}

/// Every column of a box, between two empty ones standing in for the walls.
pub struct Columns {
    columns: Vec<Column>,
    /// `starts[c]..starts[c + 1]` is where column `c`'s disks lie in the disk
    /// array, and everything from the last entry on is newborn.
    starts: Vec<u32>,
}

impl Columns {
    pub fn new(count: usize) -> Self {
        Self {
            columns: vec![Column::padded(); count + 2],
            starts: vec![0; count + 1],
        }
    }

    /// Rebuilds every column from `disks`, whose storage must follow the order
    /// of the last rebuild, with any newborn disks at the end.
    pub fn rebuild(&mut self, disks: &[Disk]) {
        let count = self.starts.len() - 1;
        let starts = &self.starts[..];
        let newborn = starts[count] as usize;
        self.columns[1..=count]
            .par_iter_mut()
            .enumerate()
            .with_min_len(4)
            .for_each(|(column_index, column)| {
                let first = column_index.saturating_sub(1);
                let last = (column_index + 1).min(count - 1);
                column.keys.clear();
                column.pull_keys(
                    column_index,
                    disks,
                    starts[first] as usize..starts[last + 1] as usize,
                );
                column.pull_keys(column_index, disks, newborn..disks.len());
                column.sort_keys();
                column.lay_out(disks);
            });
        let mut total = 0;
        for (start, column) in self.starts.iter_mut().zip(&self.columns[1..]) {
            *start = total;
            total += column.keys.len() as u32;
        }
        self.starts[count] = total;
    }

    /// Every column between its neighbours.
    pub fn with_neighbours(&self) -> impl IndexedParallelIterator<Item = &[Column]> {
        self.columns.par_windows(3)
    }

    /// The number of disks in each column.
    pub fn lengths(&self) -> impl Iterator<Item = usize> + '_ {
        self.columns[1..self.columns.len() - 1]
            .iter()
            .map(|column| column.keys.len())
    }
}
