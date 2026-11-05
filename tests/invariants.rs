//! Exact properties of the simulation: closed-form flight, conservation of
//! momentum, independence of thread count, and mirror symmetry.

use diskpour::disk::{DIAMETER, Disk, RADIUS};
use diskpour::pour::Pour;
use diskpour::world::{Arena, World};
use rayon::ThreadPoolBuilder;

/// A deterministic scatter of disks at rest, some overlapping, away from every wall.
fn cluster(arena: Arena, disks: u32) -> Vec<Disk> {
    let center = arena.columns as i32 * DIAMETER / 2;
    let spread = 70 * DIAMETER;
    let mut state = 0x2545_f491_u32;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        center - spread / 2 + (state % spread as u32) as i32
    };
    (0..disks)
        .map(|birth| {
            let position = [next(), next()];
            Disk {
                predicted: position,
                position,
                birth,
            }
        })
        .collect()
}

fn run(arena: Arena, disks: Vec<Disk>, steps: u32) -> Vec<Disk> {
    let mut world = World::new(arena);
    world.insert(disks);
    for _ in 0..steps {
        world.step();
    }
    world.disks().to_vec()
}

fn sorted(mut disks: Vec<Disk>) -> Vec<Disk> {
    disks.sort_unstable();
    disks
}

#[test]
fn stream_in_flight_follows_the_parabola_to_the_last_bit() {
    let pour = Pour::new(100_000, 900);
    let mut world = World::new(pour.arena);
    let (speed, gravity) = (i64::from(pour.muzzle_speed), i64::from(pour.arena.gravity));
    // Well before the first column reaches the floor or the right wall.
    let steps = 1500;
    for _ in 0..steps {
        pour.advance(&mut world);
    }
    let now = i64::from(world.now());
    assert!(world.disks().len() > 2000, "the stream must be flowing");
    let mut seen = vec![false; world.disks().len()];
    for disk in world.disks() {
        let age = now - i64::from(disk.birth);
        let row = disk.position[1] as i64 + gravity * age * age / 2 - i64::from(pour.origin[1]);
        assert_eq!(row % i64::from(DIAMETER), 0, "{disk:?}");
        let row = row / i64::from(DIAMETER);
        assert!((0..i64::from(pour.rows)).contains(&row), "{disk:?}");
        assert_eq!(
            i64::from(disk.position[0]),
            i64::from(pour.origin[0]) + speed * age,
            "{disk:?}"
        );
        let column = i64::from(disk.birth) / i64::from(pour.period);
        let index = (column * i64::from(pour.rows) + row) as usize;
        assert!(!seen[index], "{disk:?}");
        seen[index] = true;
    }
}

#[test]
fn contacts_conserve_momentum_exactly() {
    let arena = Arena {
        columns: 200,
        gravity: 0,
    };
    let disks = run(arena, cluster(arena, 3000), 200);
    let momentum = |axis: usize| -> i64 {
        disks
            .iter()
            .map(|disk| i64::from(disk.predicted[axis] - disk.position[axis]))
            .sum()
    };
    assert!(
        disks.iter().any(|d| d.predicted != d.position),
        "the disks must be moving"
    );
    assert_eq!(momentum(0), 0);
    assert_eq!(momentum(1), 0);
}

#[test]
fn thread_count_does_not_change_a_single_bit() {
    let pour = Pour::new(4000, 900);
    let simulate = |threads: usize| {
        let pool = ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        pool.install(|| {
            let mut world = World::new(pour.arena);
            for _ in 0..2500 {
                pour.advance(&mut world);
            }
            sorted(world.disks().to_vec())
        })
    };
    assert_eq!(simulate(1), simulate(7));
}

#[test]
fn mirroring_the_box_mirrors_the_motion_exactly() {
    let arena = Arena {
        columns: 120,
        gravity: 100,
    };
    let side = arena.columns as i32 * DIAMETER;
    let mirror = |disk: &Disk| Disk {
        predicted: [side - disk.predicted[0], disk.predicted[1]],
        position: [side - disk.position[0], disk.position[1]],
        birth: disk.birth,
    };
    let start = cluster(arena, 3000);
    let direct = run(arena, start.clone(), 3000);
    let mirrored = run(arena, start.iter().map(mirror).collect(), 3000);
    assert_eq!(
        sorted(direct.iter().map(mirror).collect()),
        sorted(mirrored)
    );
}

#[test]
fn a_full_pour_keeps_every_disk_inside_the_box_and_settles() {
    let pour = Pour::new(5000, 900);
    let mut world = World::new(pour.arena);
    let pouring = pour.disks / pour.rows * pour.period;
    for _ in 0..pouring + 12 * 900 {
        pour.advance(&mut world);
    }
    let disks = world.disks();
    assert_eq!(disks.len(), pour.disks as usize);
    let far = pour.side() - RADIUS;
    let inside = |c: i32| (RADIUS..=far).contains(&c);
    assert!(disks.iter().all(|d| d.position.iter().copied().all(inside)));

    // All disks are born once.
    let mut births: Vec<_> = disks.iter().map(|d| d.birth).collect();
    births.sort_unstable();
    assert_eq!(
        births.last().copied(),
        Some((pour.disks - 1) / pour.rows * pour.period)
    );

    // The pile holds still and its disks do not sink into one another.
    let fastest = disks
        .iter()
        .map(|d| {
            (d.predicted[0] - d.position[0])
                .abs()
                .max((d.predicted[1] - d.position[1]).abs())
        })
        .max()
        .unwrap();
    assert!(fastest < DIAMETER / 20, "fastest stride {fastest} units");
    let top = disks.iter().map(|d| d.position[1]).max().unwrap();
    assert!(
        top < pour.side() * 9 / 10,
        "top at {top} of {}",
        pour.side()
    );
}
