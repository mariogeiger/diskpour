# Changelog

All notable user-visible changes to this project are documented here.

## 0.3.0

### Added

- A `Camera` that shows any part of the box, drawn by the GPU at that scale, so
  a disk stays sharp at any zoom, and a `CameraPath` of gentle moves between views.
- `frames --camera` follows such a path, and `frames --steps-per-second` changes
  the stiffness.
- A `grains` example that colors each disk by the orientation of its lattice, which
  shows that a settled pile is a polycrystal.

### Changed

- `Renderer::draw` and `Offscreen::draw` take a `Camera`.
- `frames` takes the paint table as `--paint`.

## 0.2.0

### Added

- A `frames` example that writes a stretch of a pour to standard output as raw
  RGBA frames, in real time, slow motion or time lapse.
- A `paint` example that colors each disk by where it ends up, so that a replay
  through `frames` forms a picture if and only if it reproduces the run exactly.
- `World::fingerprint`, a hash of every bit of the state.

### Changed

- Disks carry a serial number instead of their birth step, which the pour derives
  from it, so a disk can be followed for life at no extra memory.
- `Renderer::new` takes a `Palette`.

## 0.1.0

### Added

- A windowed real-time pour of 100,000 disks, with pause, restart and a title bar
  reporting frame rate, physics cost and speed relative to real time.
- A `still` example that draws any simulated moment to a PNG without a window.
