//! The camera path is exact where it can be and otherwise a true geodesic.

use diskpour::camera::{Camera, CameraPath};

const BOX: i32 = 340 << 20;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * f64::from(BOX)
}

fn same_view(a: Camera, b: Camera) -> bool {
    close(a.side, b.side) && close(a.center[0], b.center[0]) && close(a.center[1], b.center[1])
}

#[test]
fn a_move_starts_and_ends_on_its_views() {
    let (whole, near) = (Camera::whole(BOX), Camera::zoomed(BOX, [0.3, 0.07], 12.0));
    assert_eq!(whole.toward(near, 0.0), whole);
    assert!(same_view(whole.toward(near, 1.0), near));
}

#[test]
fn a_move_backward_retraces_the_move_forward() {
    let (a, b) = (
        Camera::zoomed(BOX, [0.5, 0.5], 1.5),
        Camera::zoomed(BOX, [0.2, 0.8], 20.0),
    );
    for tenth in 0..=10 {
        let progress = f64::from(tenth) / 10.0;
        assert!(same_view(
            a.toward(b, progress),
            b.toward(a, 1.0 - progress)
        ));
    }
}

#[test]
fn a_zoom_keeps_one_point_fixed_on_screen() {
    let (a, b) = (Camera::whole(BOX), Camera::zoomed(BOX, [0.3, 0.07], 12.0));
    // The point both views show at the same place on screen.
    let fixed =
        [0, 1].map(|axis| (a.center[axis] * b.side - b.center[axis] * a.side) / (b.side - a.side));
    let on_screen = |view: Camera, axis: usize| (fixed[axis] - view.center[axis]) / view.side;
    for tenth in 0..=10 {
        let view = a.toward(b, f64::from(tenth) / 10.0);
        for axis in 0..2 {
            assert!((on_screen(view, axis) - on_screen(a, axis)).abs() < 1e-9);
        }
    }
}

#[test]
fn a_move_between_views_inside_the_box_stays_inside() {
    let side = f64::from(BOX);
    let (a, b) = (Camera::whole(BOX), Camera::zoomed(BOX, [0.06, 0.06], 10.0));
    for hundredth in 0..=100 {
        let view = a.toward(b, f64::from(hundredth) / 100.0);
        for axis in 0..2 {
            assert!(view.center[axis] - view.side / 2.0 >= -1e-6 * side);
            assert!(view.center[axis] + view.side / 2.0 <= side * (1.0 + 1e-6));
        }
    }
}

#[test]
fn a_path_holds_its_views_and_is_halfway_at_half_time() {
    let (whole, near) = (Camera::whole(BOX), Camera::zoomed(BOX, [0.3, 0.07], 12.0));
    let path = CameraPath::new(vec![(1.0, whole), (2.0, whole), (4.0, near)]);
    assert_eq!(path.at(0.0), whole);
    assert_eq!(path.at(1.5), whole);
    assert_eq!(path.at(9.0), near);
    assert!(same_view(path.at(3.0), whole.toward(near, 0.5)));
}
