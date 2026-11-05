# diskpour soul

This file defines how diskpour should feel and how design decisions should be
made. Keep it shorter and more stable than the code.

## Mission

Pour 100,000 colliding disks into a box, on the CPU, in real time.

## Design rules

1. **Get the invariants exact.** Free flight, momentum, mirror symmetry and thread
   independence hold to the last bit, and tests check them bit for bit. A change
   that weakens one needs a better reason than speed.
2. **Derive constants, do not tune them per resolution.** The look comes from
   dimensionless numbers measured on the reference picture, so any disk count gives
   the same flow.
3. **Spend cycles on steps.** Stiffness grows with the square of the step rate and
   cost only linearly, so one contact solve per step beats several per frame.
4. **Name things by what they do.** A name states the operation or the quantity,
   with its unit where that is not obvious.
5. **Keep the boundary explicit.** The world knows disks and walls, the pour knows
   the jet, the renderer knows the GPU. Interaction beyond viewing belongs elsewhere.
6. **Slow down instead of falling behind.** When the machine cannot keep up, time
   dilates. The simulation never drops physics or changes its result.
7. **Keep each file bounded.** Give each file one coherent mission and split it
   before 1000 lines.
