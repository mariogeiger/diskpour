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
Space pauses, R restarts, Escape quits. The title bar shows the frame rate, the
physics cost per frame, and the speed relative to real time. A machine that cannot
keep up runs in slow motion at a smooth frame rate.

```bash
cargo run --release --example still -- 13.5 frame.png
```

draws the state at a given simulated second to a PNG without opening a window, and
reports the physics cost per frame.

```bash
cargo run --release --example frames -- 0 60 15 2160 | ffmpeg -f rawvideo -pix_fmt rgba -s 2160x2160 -r 60 -i - out.mp4
```

renders a stretch of the pour to video, at any playback speed. `--camera` zooms with
the GPU, so disks stay sharp: `--camera 0=.5,.5,1 2=.5,.5,1 5=.5,.06,10` holds the
whole box for two seconds, then moves gently to a tenth of it at the bottom. And

```bash
cargo run --release --example paint -- 80 picture.png paint.rgba
RAYON_NUM_THREADS=1 cargo run --release --example frames -- 0 80 15 2160 --paint paint.rgba | ffmpeg -f rawvideo -pix_fmt rgba -s 2160x2160 -r 60 -i - replay.mp4
```

colors each disk by where it ends up, then replays the pour on another thread count:
the picture forms only if the replay reproduces the run to the last bit. Both print
a fingerprint of the final state to compare.

`cargo run --release --example grains -- 80 grains.rgba` makes a table for `--paint`
that colors each disk by the orientation of its lattice. The thin dark lines in a
settled pile turn out to be the boundaries between crystal grains.

## Design principles

- **Positions are integers.** Physics reads only their differences, so fixed point
  gives uniform resolution and makes free flight exact to the last bit.
- **Every pair is solved at once.** Nothing depends on solve order, so a step is
  reproducible on any number of threads and mirror symmetric bit for bit.
- **Pushes are exactly opposite.** Each pair's push truncates toward zero, so
  contacts conserve momentum exactly.

Read the [project soul](SOUL.md) for how design decisions are made.
