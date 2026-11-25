//! The generators of the eight families of plan `2026-11-27-gated-movement` (§5.2, `WP0a`).
//!
//! # Responsibility
//! [`generate`] draws one scene of `hold-line`, `enemy-press`, `deep-queue`, `wedge`, `hallway`,
//! `stutter`, `surround-moving` or `lanes` from the family's [`SplitMix64`] stream, exactly as the
//! plan's table states the scene, and attaches its [`KnownAnswer`]. Like the older families every
//! coordinate and size is a whole number of sixty-fourths, the radii and speeds come from the
//! corpus sets ([`RADII`], [`SPEEDS`]), and the units are scratch archetypes.
//!
//! # How it fits
//! A child of [`super`], whose `generate` calls one generator per family and relabels its
//! scenes; it shares the parent's
//! stream, block layout, parameter string and bound helpers. The answers are judged in
//! [`crate::harness::answers`] from the meters of [`crate::harness::metrics::known`].
//!
//! # Invariants
//! - Each generator is a pure function of its stream and seed.
//! - Every spawn and goal lies on open ground inside the map's terrain-radius domain, and no two
//!   spawns overlap.
//! - A unit that names another ([`Role::AttackAt`]) names it by its spawn point, so relabelling
//!   keeps the scene.

use super::{
    attacker, block, capacity_count, count_f64, decimal, f64_of, param, pillar_spawn,
    ring_archetypes, speed_value, ticks_of, Answer, Cue, CueOrder, Lattice, MapSpec, Role,
    Scenario, ScratchArchetype, Spawn, SplitMix64, BLOCK_SLACK, CORRIDOR_RANGES, HEADINGS, LATTICE,
    PRIORITY, RADII, SECOND_PRIORITY, SPEEDS, WEAPON_PERIOD,
};
use crate::harness::answers::{KnownAnswer, Stance};
use crate::harness::Family;

/// The surround-moving target's speeds: each of [`SPEEDS`] times four to seven eighths (0.5 to
/// 0.875 of the attackers'), as exact catalog decimals.
const TARGET_SPEEDS: [[&str; 4]; 4] = [
    ["0.05", "0.0625", "0.075", "0.0875"],
    ["0.0625", "0.078125", "0.09375", "0.109375"],
    ["0.078125", "0.09765625", "0.1171875", "0.13671875"],
    ["0.09375", "0.1171875", "0.140625", "0.1640625"],
];

/// Melee ranges of the surround-moving attackers, sixty-fourths (0.125, 0.25 and 0.5 world
/// units), exact on the lattice so the band of the known answer is.
const MELEE_RANGES: [Lattice; 3] = [8, 16, 32];

/// Ranges of the stutter family's ranged attackers, whole world units (4 to 6).
const STUTTER_RANGES: [Lattice; 3] = [4, 5, 6];

/// Ticks of the stutter family before its first move-back, and between two move-backs.
const STUTTER_FIRST: u32 = 40;
const STUTTER_CYCLE: u32 = 50;

/// Move-backs per stutter scene (the plan's "repeated 4 times").
const STUTTER_REPEATS: u32 = 4;

/// Laps of queued corners the surround-moving target is given: more than any scene's bound lets
/// it walk.
const CIRCLE_LAPS: usize = 4;

/// A scene of `family` and `seed` with the fields every generator sets, the rest empty: open
/// ground of `size` cells.
fn scene(family: Family, seed: u64, size: (u32, u32), params: String) -> Scenario {
    Scenario {
        family,
        seed,
        relabel: false,
        params,
        map: MapSpec {
            size,
            blocked: Vec::new(),
            geysers: Vec::new(),
        },
        archetypes: Vec::new(),
        spawns: Vec::new(),
        ticks: 0,
        expected: 0,
        answer: Answer::Moves,
        script: Vec::new(),
        known: None,
    }
}

/// An unarmed scratch archetype.
const fn body(radius: Lattice, speed: &'static str, priority: u8) -> ScratchArchetype {
    ScratchArchetype {
        radius,
        speed,
        range: None,
        priority,
    }
}

/// The x, sixty-fourths, at which a [`block`] of `count` units of `radius` along +x has its rear
/// two world units clear of the map's west edge.
fn block_centre_x(count: usize, radius: Lattice) -> Lattice {
    let columns = (1..=count).find(|c| c * c >= count).unwrap_or(1);
    let rows = i64::try_from(count.div_ceil(columns)).unwrap_or(1);
    (rows - 1) * (2 * radius + BLOCK_SLACK) / 2 + radius + 2 * LATTICE
}

/// `count` positions in a line along y centred on `centre`, `pitch` apart.
fn line(count: usize, centre: (Lattice, Lattice), pitch: Lattice) -> Vec<(Lattice, Lattice)> {
    let count = i64::try_from(count).unwrap_or(0);
    (0..count)
        .map(|i| (centre.0, centre.1 + pitch * (2 * i - (count - 1)) / 2))
        .collect()
}

/// A count drawn in `low..=high`.
fn count_between(rng: &mut SplitMix64, low: i64, high: i64) -> usize {
    usize::try_from(rng.between(low, high)).unwrap_or(0)
}

/// `hold-line`: 4 to 10 friends on Hold Position in a line across an open 48 × 48 field, and 6
/// to 30 friends, a quarter of them of push priority 20, given one `Move` to a point beyond it.
/// The bound is the gap family's with the line's length added to the distance, one lane.
pub(super) fn hold_line(rng: &mut SplitMix64, seed: u64) -> Scenario {
    let holders = count_between(rng, 4, 10);
    let holder_radius = rng.pick(&RADII);
    let n = count_between(rng, 6, 30);
    let radius = rng.pick(&RADII);
    let speed = rng.pick(&SPEEDS);
    let mut params = String::new();
    param(&mut params, "holders", holders);
    param(&mut params, "R", decimal(holder_radius));
    param(&mut params, "n", n);
    param(&mut params, "r", decimal(radius));
    param(&mut params, "v", speed);
    let mut scenario = scene(Family::HoldLine, seed, (48, 48), params);
    scenario.archetypes = vec![
        body(radius, speed, PRIORITY),
        body(radius, speed, SECOND_PRIORITY),
        body(holder_radius, speed, PRIORITY),
    ];
    let pitch = 2 * holder_radius + BLOCK_SLACK;
    let line_centre = (24 * LATTICE, (24 + rng.between(-3, 3)) * LATTICE);
    let start = (
        block_centre_x(n, radius) + rng.between(0, 2) * LATTICE,
        (24 + rng.between(-4, 4)) * LATTICE,
    );
    let goal = (40 * LATTICE, (24 + rng.between(-4, 4)) * LATTICE);
    scenario.spawns = line(holders, line_centre, pitch)
        .into_iter()
        .map(|at| Spawn {
            archetype: 2,
            owner: 0,
            at,
            role: Role::Hold,
        })
        .collect();
    scenario.spawns.extend(
        block(n, radius, start, HEADINGS[0])
            .into_iter()
            .enumerate()
            .map(|(index, at)| Spawn {
                archetype: usize::from(index % 4 == 3),
                owner: 0,
                at,
                role: Role::GroupMove { goal },
            }),
    );
    let length = f64_of(pitch) * count_f64(holders);
    let v = speed_value(speed);
    let bound = 2.0 * ((50.0 + length) / v + count_f64(n) * f64_of(2 * radius) / v) + 100.0;
    scenario.ticks = ticks_of(bound);
    scenario.expected = u32::try_from(n).unwrap_or(0);
    scenario.known = Some(KnownAnswer::HoldLine);
    scenario
}

/// `enemy-press`: 4 to 8 enemies in a line, idle, attacking a dummy behind them, or holding (the
/// three sub-families), and 20 to 60 attackers attack-moving into them from the west.
pub(super) fn enemy_press(rng: &mut SplitMix64, seed: u64) -> Scenario {
    let stance = [Stance::Idle, Stance::Attacking, Stance::Holding]
        [usize::try_from(rng.below(3)).unwrap_or(0)];
    let enemies = count_between(rng, 4, 8);
    let enemy_radius = rng.pick(&RADII);
    let enemy_speed = rng.pick(&SPEEDS);
    let k = count_between(rng, 20, 60);
    let radius = rng.pick(&RADII);
    let speed = rng.pick(&SPEEDS);
    let range = rng.pick(&CORRIDOR_RANGES);
    let mut params = String::new();
    param(&mut params, "stance", stance.name());
    param(&mut params, "e", enemies);
    param(&mut params, "R", decimal(enemy_radius));
    param(&mut params, "k", k);
    param(&mut params, "r", decimal(radius));
    param(&mut params, "range", range);
    param(&mut params, "v", speed);
    let mut scenario = scene(Family::EnemyPress, seed, (48, 48), params);
    let dummy = 24;
    scenario.archetypes = vec![
        ScratchArchetype {
            radius,
            speed,
            range: Some(range),
            priority: PRIORITY,
        },
        ScratchArchetype {
            radius: enemy_radius,
            speed: enemy_speed,
            range: (stance == Stance::Attacking).then_some("1"),
            priority: PRIORITY,
        },
        body(dummy, "0.0", PRIORITY),
    ];
    let line_x = 30 * LATTICE;
    let line_at = line(
        enemies,
        (line_x, 24 * LATTICE),
        2 * enemy_radius + BLOCK_SLACK,
    );
    for &at in &line_at {
        // The dummy stands half a unit behind its enemy's edge, inside the enemy's range of 1.
        let dummy_at = (at.0 + enemy_radius + dummy + LATTICE / 2, at.1);
        let role = match stance {
            Stance::Idle => Role::Stand,
            Stance::Attacking => Role::AttackAt { target: dummy_at },
            Stance::Holding => Role::Hold,
        };
        scenario.spawns.push(Spawn {
            archetype: 1,
            owner: 1,
            at,
            role,
        });
        if stance == Stance::Attacking {
            scenario.spawns.push(Spawn {
                archetype: 2,
                owner: 0,
                at: dummy_at,
                role: Role::Stand,
            });
        }
    }
    let start = (
        block_centre_x(k, radius) + rng.between(0, 2) * LATTICE,
        (24 + rng.between(-4, 4)) * LATTICE,
    );
    let goal = (44 * LATTICE, 24 * LATTICE);
    scenario.spawns.extend(
        block(k, radius, start, HEADINGS[0])
            .into_iter()
            .map(|at| Spawn {
                archetype: 0,
                owner: 0,
                at,
                role: Role::AttackMove { goal },
            }),
    );
    let bound = 2.0 * f64_of(line_x - start.0) / speed_value(speed) + 500.0;
    scenario.ticks = ticks_of(bound);
    scenario.known = Some(KnownAnswer::EnemyPress { stance });
    scenario
}

/// The drawn shape of a deep queue: corridor width (cells), attacker radius, ranks and files.
fn queue_shape(rng: &mut SplitMix64) -> (Lattice, Lattice, usize, usize) {
    loop {
        let w: Lattice = rng.between(3, 6);
        let radius = rng.pick(&RADII);
        let ranks = count_between(rng, 6, 12);
        let room = w * LATTICE - 8 - 2 * radius;
        if room < 0 {
            continue;
        }
        let most = usize::try_from(room / (2 * radius + BLOCK_SLACK)).unwrap_or(0) + 1;
        let files = count_between(rng, 1, i64::try_from(most).unwrap_or(1));
        if (20..=80).contains(&(files * ranks)) {
            break (w, radius, ranks, files);
        }
    }
}

/// `deep-queue`: a corridor 3 to 6 cells wide along a 64 × 16 map with no way round, plugged by
/// an immobile enemy pillar of half its width, and 20 to 80 melee attackers 6 to 12 ranks deep
/// told to attack it. The front fills to the corridor family's capacity.
pub(super) fn deep_queue(rng: &mut SplitMix64, seed: u64) -> Scenario {
    let (w, radius, ranks, files) = queue_shape(rng);
    let range = rng.pick(&CORRIDOR_RANGES);
    let speed = rng.pick(&SPEEDS);
    let k = files * ranks;
    let capacity = u32::try_from(w * LATTICE / (2 * radius + 4)).unwrap_or(0);
    let mut params = String::new();
    param(&mut params, "w", w);
    param(&mut params, "ranks", ranks);
    param(&mut params, "files", files);
    param(&mut params, "R", range);
    param(&mut params, "r", decimal(radius));
    param(&mut params, "v", speed);
    param(&mut params, "cap", capacity);
    let mut scenario = scene(Family::DeepQueue, seed, (64, 16), params);
    let low = 8 - u32::try_from(w / 2).unwrap_or(1);
    let rows = low..low + u32::try_from(w).unwrap_or(3);
    scenario.map.blocked = (0..16_u32)
        .filter(|y| !rows.contains(y))
        .flat_map(|y| (0..64_u32).map(move |x| (x, y)))
        .collect();
    let middle = i64::from(low) * LATTICE + w * 32;
    let pillar = w * 32;
    let pillar_at = (rng.between(48, 54) * LATTICE, middle);
    let pitch = 2 * radius + BLOCK_SLACK;
    let front = pillar_at.0 - pillar - radius - 3 * LATTICE;
    scenario.spawns.push(pillar_spawn(pillar_at));
    for rank in 0..ranks {
        let x = front - i64::try_from(rank).unwrap_or(0) * pitch;
        scenario
            .spawns
            .extend(line(files, (x, middle), pitch).into_iter().map(attacker));
    }
    scenario.archetypes = ring_archetypes(radius, speed, range, pillar);
    let rear = front - i64::try_from(ranks - 1).unwrap_or(0) * pitch;
    let bound = 2.0 * f64_of(pillar_at.0 - rear) / speed_value(speed) + 500.0;
    scenario.ticks = ticks_of(bound);
    scenario.expected = u32::try_from(k).unwrap_or(0).min(capacity);
    scenario.answer = Answer::Ring;
    scenario.known = Some(KnownAnswer::DeepQueue {
        front,
        pitch,
        ranks: u32::try_from(ranks).unwrap_or(0),
    });
    scenario
}

/// `wedge`: a walker whose straight route runs between two bodies (friends on Hold Position in
/// half the seeds, standing enemies in the other half) whose edges are 0.6 to 0.95 of its
/// diameter apart, with open ground round them.
pub(super) fn wedge(rng: &mut SplitMix64, seed: u64) -> Scenario {
    let radius = rng.pick(&RADII);
    let body_radius = rng.pick(&RADII);
    let speed = rng.pick(&SPEEDS);
    let permille = rng.between(600, 950);
    let gap = (2 * radius * permille / 1000) & !1;
    let enemies = rng.one_in(2);
    let mut params = String::new();
    param(&mut params, "r", decimal(radius));
    param(&mut params, "R", decimal(body_radius));
    param(&mut params, "gap_permille", permille);
    param(&mut params, "v", speed);
    param(&mut params, "enemies", u8::from(enemies));
    let mut scenario = scene(Family::Wedge, seed, (48, 48), params);
    scenario.archetypes = vec![
        body(radius, speed, PRIORITY),
        body(body_radius, speed, PRIORITY),
    ];
    let y = 24 * LATTICE;
    let offset = gap / 2 + body_radius;
    let bodies = [(24 * LATTICE, y - offset), (24 * LATTICE, y + offset)];
    for at in bodies {
        scenario.spawns.push(Spawn {
            archetype: 1,
            owner: u8::from(enemies),
            at,
            role: if enemies { Role::Stand } else { Role::Hold },
        });
    }
    scenario.spawns.push(Spawn {
        archetype: 0,
        owner: 0,
        at: (10 * LATTICE, y),
        role: Role::OwnMove {
            goal: (38 * LATTICE, y),
        },
    });
    scenario.ticks = ticks_of(2.0 * 50.0 / speed_value(speed) + 100.0);
    scenario.expected = 1;
    scenario.known = Some(KnownAnswer::Wedge { bodies, enemies });
    scenario
}

/// `hallway`: two single files of 1 to 6 units meeting head-on in a corridor two diameters of the
/// larger radius wide, each unit with a `Move` of its own to the far end.
pub(super) fn hallway(rng: &mut SplitMix64, seed: u64) -> Scenario {
    let w: Lattice = rng.between(2, 4);
    let larger = w * LATTICE / 4;
    let smaller = loop {
        let radius = rng.pick(&RADII);
        if radius <= larger {
            break radius;
        }
    };
    let (east_radius, west_radius) = if rng.one_in(2) {
        (larger, smaller)
    } else {
        (smaller, larger)
    };
    let (na, nb) = (count_between(rng, 1, 6), count_between(rng, 1, 6));
    let speed = rng.pick(&SPEEDS);
    let mut params = String::new();
    param(&mut params, "w", w);
    param(&mut params, "ra", decimal(east_radius));
    param(&mut params, "rb", decimal(west_radius));
    param(&mut params, "na", na);
    param(&mut params, "nb", nb);
    param(&mut params, "v", speed);
    let mut scenario = scene(Family::Hallway, seed, (64, 16), params);
    let low = 8 - u32::try_from(w / 2).unwrap_or(1);
    let rows = low..low + u32::try_from(w).unwrap_or(2);
    scenario.map.blocked = (0..16_u32)
        .filter(|y| !rows.contains(y))
        .flat_map(|y| (0..64_u32).map(move |x| (x, y)))
        .collect();
    scenario.archetypes = vec![
        body(east_radius, speed, PRIORITY),
        body(west_radius, speed, PRIORITY),
    ];
    let middle = i64::from(low) * LATTICE + w * 32;
    let travel = 44 * LATTICE;
    for (archetype, count, radius, sign) in [(0, na, east_radius, 1), (1, nb, west_radius, -1)] {
        let pitch = 2 * radius + BLOCK_SLACK;
        for i in 0..i64::try_from(count).unwrap_or(0) {
            let x = if sign > 0 {
                4 * LATTICE + radius + i * pitch
            } else {
                60 * LATTICE - radius - i * pitch
            };
            scenario.spawns.push(Spawn {
                archetype,
                owner: 0,
                at: (x, middle),
                role: Role::OwnMove {
                    goal: (x + sign * travel, middle),
                },
            });
        }
    }
    let files = f64_of(2 * larger + BLOCK_SLACK) * count_f64(na + nb);
    scenario.ticks = ticks_of(2.0 * (44.0 + files) / speed_value(speed) + 100.0);
    scenario.expected = u32::try_from(na + nb).unwrap_or(0);
    scenario.known = Some(KnownAnswer::Hallway);
    scenario
}

/// `stutter`: 1 to 6 ranged attackers (range 4 to 6) firing at an immobile enemy dummy, two
/// friends packed behind each; four times a `Move` straight back for 3 to 8 ticks, then `Attack`
/// again ([`Scenario::script`]).
pub(super) fn stutter(rng: &mut SplitMix64, seed: u64) -> Scenario {
    let a = count_between(rng, 1, 6);
    let radius = rng.pick(&RADII);
    let speed = rng.pick(&SPEEDS);
    let range = rng.pick(&STUTTER_RANGES);
    let back = u32::try_from(rng.between(3, 8)).unwrap_or(3);
    let mut params = String::new();
    param(&mut params, "a", a);
    param(&mut params, "r", decimal(radius));
    param(&mut params, "R", range);
    param(&mut params, "v", speed);
    param(&mut params, "m", back);
    let mut scenario = scene(Family::Stutter, seed, (48, 48), params);
    let dummy = 32;
    let range_text = ["4", "5", "6"][usize::try_from(range - 4).unwrap_or(0)];
    scenario.archetypes = ring_archetypes(radius, speed, range_text, dummy);
    scenario.archetypes.push(body(radius, speed, PRIORITY));
    let target = (34 * LATTICE, 24 * LATTICE);
    scenario.spawns.push(pillar_spawn(target));
    let pitch = 2 * radius + BLOCK_SLACK;
    // One unit inside the band's outer edge.
    let x = target.0 - (range * LATTICE + radius + dummy) + LATTICE;
    for at in line(a, (x, target.1), pitch) {
        scenario.spawns.push(attacker(at));
        for behind in 1..=2 {
            scenario.spawns.push(Spawn {
                archetype: 2,
                owner: 0,
                at: (at.0 - behind * pitch, at.1),
                role: Role::Stand,
            });
        }
    }
    for repeat in 0..STUTTER_REPEATS {
        let tick = STUTTER_FIRST + repeat * STUTTER_CYCLE;
        scenario.script.push(Cue {
            tick,
            order: CueOrder::MoveBy {
                offset: (-6 * LATTICE, 0),
            },
        });
        scenario.script.push(Cue {
            tick: tick + back,
            order: CueOrder::Attack,
        });
    }
    scenario.ticks = STUTTER_FIRST + STUTTER_REPEATS * STUTTER_CYCLE + 20;
    scenario.expected = u32::try_from(a).unwrap_or(0);
    scenario.answer = Answer::Ring;
    scenario.known = Some(KnownAnswer::Stutter {
        period: WEAPON_PERIOD,
    });
    scenario
}

/// `surround-moving`: 10 to 30 melee attackers chasing a target that walks a circle of radius 6
/// to 12 round the map's centre at 0.5 to 0.875 of their speed. The capacity `C` is the arc
/// family's, at the band's edge.
pub(super) fn surround_moving(rng: &mut SplitMix64, seed: u64) -> Scenario {
    let k = count_between(rng, 10, 30);
    let radius = rng.pick(&RADII);
    let speed_index = usize::try_from(rng.below(4)).unwrap_or(0);
    let eighths = usize::try_from(rng.between(4, 7)).unwrap_or(4);
    let target_radius = rng.pick(&RADII);
    let range = rng.pick(&MELEE_RANGES);
    let circle: Lattice = rng.between(6, 12);
    let distance = rng.between(10, 14) * LATTICE;
    let heading = HEADINGS[usize::try_from(rng.between(5, 11)).unwrap_or(8)];
    let (speed, target_speed) = (SPEEDS[speed_index], TARGET_SPEEDS[speed_index][eighths - 4]);
    let band = target_radius + radius + range;
    // C = ⌊π / asin(r / band)⌋, judged in f64 outside the simulation; the inputs are exact.
    let capacity =
        capacity_count((std::f64::consts::PI / (f64_of(radius) / f64_of(band)).asin()).floor());
    let mut params = String::new();
    param(&mut params, "k", k);
    param(&mut params, "r", decimal(radius));
    param(&mut params, "v", speed);
    param(&mut params, "vt", target_speed);
    param(&mut params, "P", decimal(target_radius));
    param(&mut params, "R", decimal(range));
    param(&mut params, "rho", circle);
    param(&mut params, "cap", capacity);
    let mut scenario = scene(Family::SurroundMoving, seed, (48, 48), params);
    let range_text =
        ["0.125", "0.25", "0.5"][MELEE_RANGES.iter().position(|&r| r == range).unwrap_or(0)];
    scenario.archetypes = vec![
        ScratchArchetype {
            radius,
            speed,
            range: Some(range_text),
            priority: PRIORITY,
        },
        body(target_radius, target_speed, PRIORITY),
    ];
    let centre = (24 * LATTICE, 24 * LATTICE);
    scenario.spawns.push(Spawn {
        archetype: 1,
        owner: 1,
        at: (
            centre.0 + circle * HEADINGS[0].0,
            centre.1 + circle * HEADINGS[0].1,
        ),
        role: Role::Circle {
            centre,
            radius: circle * LATTICE,
        },
    });
    let at = (
        centre.0 + distance * heading.0 / LATTICE,
        centre.1 + distance * heading.1 / LATTICE,
    );
    scenario.spawns.extend(
        block(k, radius, at, (-heading.0, -heading.1))
            .into_iter()
            .map(attacker),
    );
    let bound = 2.0 * f64_of(distance + circle * LATTICE) / speed_value(speed) + 350.0;
    scenario.ticks = ticks_of(bound);
    scenario.expected = u32::try_from(k).unwrap_or(0).min(capacity);
    scenario.known = Some(KnownAnswer::SurroundMoving { capacity, band });
    scenario
}

/// The corners a [`Role::Circle`] walker is sent to, in order: the sixteen [`HEADINGS`] round
/// `centre` at `radius` (sixty-fourths), counter-clockwise from the corner after the first,
/// [`CIRCLE_LAPS`] times.
#[must_use]
pub fn circle_corners(centre: (Lattice, Lattice), radius: Lattice) -> Vec<(Lattice, Lattice)> {
    (1..=CIRCLE_LAPS * HEADINGS.len())
        .map(|step| {
            let heading = HEADINGS[step % HEADINGS.len()];
            (
                centre.0 + radius * heading.0 / LATTICE,
                centre.1 + radius * heading.1 / LATTICE,
            )
        })
        .collect()
}

/// `lanes`: a group `Move` of 12 to 48 through a one-cell wall across a 48 × 48 field with two
/// parallel three-cell gaps placed symmetrically about the group's straight route.
pub(super) fn lanes(rng: &mut SplitMix64, seed: u64) -> Scenario {
    let n = count_between(rng, 12, 48);
    let radius = rng.pick(&RADII);
    let speed = rng.pick(&SPEEDS);
    let spread = u32::try_from(rng.between(3, 6)).unwrap_or(3);
    let mut params = String::new();
    param(&mut params, "n", n);
    param(&mut params, "r", decimal(radius));
    param(&mut params, "v", speed);
    param(&mut params, "s", spread);
    let mut scenario = scene(Family::Lanes, seed, (48, 48), params);
    let wall = 24_u32;
    let gaps = [
        (24 - spread - 2, 24 - spread + 1),
        (24 + spread - 1, 24 + spread + 2),
    ];
    scenario.map.blocked = (0..48_u32)
        .filter(|y| !gaps.iter().any(|&(low, high)| (low..high).contains(y)))
        .map(|y| (wall, y))
        .collect();
    scenario.archetypes = vec![body(radius, speed, PRIORITY)];
    let start = (
        block_centre_x(n, radius) + rng.between(0, 2) * LATTICE,
        24 * LATTICE,
    );
    let goal = (40 * LATTICE, 24 * LATTICE);
    scenario.spawns = block(n, radius, start, HEADINGS[0])
        .into_iter()
        .map(|at| Spawn {
            archetype: 0,
            owner: 0,
            at,
            role: Role::GroupMove { goal },
        })
        .collect();
    let lanes = 2 * ((3 * LATTICE - 2 * radius) / (2 * radius) + 1);
    let v = speed_value(speed);
    let lanes = f64::from(i32::try_from(lanes).unwrap_or(1));
    let bound = 2.0 * (50.0 / v + count_f64(n) * f64_of(2 * radius) / (lanes * v)) + 100.0;
    scenario.ticks = ticks_of(bound);
    scenario.expected = u32::try_from(n).unwrap_or(0);
    scenario.known = Some(KnownAnswer::Lanes { wall, gaps, radius });
    scenario
}
