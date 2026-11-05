# Changelog

All notable user-visible changes to this project are documented here.

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
