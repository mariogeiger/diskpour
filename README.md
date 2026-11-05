# diskpour

A jet of 100,000 colliding disks pouring into a box, simulated on the CPU in real
time and drawn with wgpu.

Each disk is emitted from a lattice, flies an exact parabola, and splashes into
the box. Its color shows its age, so the flow writes its own history into the lake
it fills.

## Run

```bash
cargo run --release
```

`diskpour [DISKS] [STEPS_PER_SECOND]` changes the resolution and the stiffness.
Space pauses, R restarts, Escape quits. The title bar shows the frame rate and the
physics cost per frame. A machine that cannot keep up runs in slow motion.

```bash
cargo run --release --example still -- 9.7 frame.png
```

draws the state at a given simulated second to a PNG without opening a window, and
reports the physics cost per frame.

## Design principles

- **Positions are integers.** Physics reads only their differences, so fixed point
  gives uniform resolution and makes free flight exact to the last bit.
- **Every pair is solved at once.** Nothing depends on solve order, so a step is
  reproducible on any number of threads and mirror symmetric bit for bit.
- **Pushes are exactly opposite.** Each pair's push truncates toward zero, so
  contacts conserve momentum exactly.

Read the [project soul](SOUL.md) for how design decisions are made.
