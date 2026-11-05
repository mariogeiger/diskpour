//! What part of the box fills the picture, and how to move between such views.
//!
//! A view is a scaling and a translation, and those form a group. The straight
//! path between two views in that group scales exponentially about the one point
//! both views show in the same place, so a zoom has a constant perceived speed and
//! never wanders. In the plane of center and side that path is a straight line,
//! so it stays inside the box whenever both of its ends do.

/// A square window onto the box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// The center of the window in position units.
    pub center: [f64; 2],
    /// The side of the window in position units.
    pub side: f64,
}

impl Camera {
    /// The whole box of side `box_side`.
    pub fn whole(box_side: i32) -> Self {
        Self::zoomed(box_side, [0.5, 0.5], 1.0)
    }

    /// A window `zoom` times smaller than the box, centered on `place`, given as
    /// fractions of the box from its bottom-left corner.
    pub fn zoomed(box_side: i32, place: [f64; 2], zoom: f64) -> Self {
        let box_side = f64::from(box_side);
        Self {
            center: place.map(|fraction| fraction * box_side),
            side: box_side / zoom,
        }
    }

    /// The view a fraction `progress` of the way along the straight path to `other`.
    pub fn toward(self, other: Self, progress: f64) -> Self {
        let rate = (other.side / self.side).ln();
        // The distance covered is proportional to the change in side, which only
        // tends to `progress` as the two sides become equal.
        let along = if rate == 0.0 {
            progress
        } else {
            (progress * rate).exp_m1() / rate.exp_m1()
        };
        Self {
            center: [0, 1]
                .map(|axis| self.center[axis] + (other.center[axis] - self.center[axis]) * along),
            side: self.side * (progress * rate).exp(),
        }
    }
}

/// Views at given moments, with gentle moves between them.
#[derive(Clone, Debug)]
pub struct CameraPath {
    /// Moments in seconds, ascending, each with the view it shows.
    keys: Vec<(f64, Camera)>,
}

impl CameraPath {
    /// Makes a path through `keys`, which should be in ascending order of time. Two
    /// consecutive keys with the same view hold it between them.
    pub fn new(keys: Vec<(f64, Camera)>) -> Self {
        assert!(!keys.is_empty(), "a camera path needs at least one view");
        assert!(
            keys.windows(2).all(|pair| pair[0].0 <= pair[1].0),
            "the views of a camera path should be in order of time"
        );
        Self { keys }
    }

    /// The view at `seconds`.
    pub fn at(&self, seconds: f64) -> Camera {
        let next = self.keys.partition_point(|key| key.0 <= seconds);
        match (next, self.keys.get(next)) {
            (0, _) => self.keys[0].1,
            (_, None) => self.keys[next - 1].1,
            (_, Some(&(to_time, to))) => {
                let (from_time, from) = self.keys[next - 1];
                from.toward(to, gentle((seconds - from_time) / (to_time - from_time)))
            }
        }
    }
}

/// Eases from 0 to 1 with zero speed and zero acceleration at both ends, so a move
/// starts and stops without a visible jolt.
fn gentle(x: f64) -> f64 {
    x * x * x * (x * (6.0 * x - 15.0) + 10.0)
}
