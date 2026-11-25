//! The scenario generator of the movement harness: a pure function of `(family, seed)`.
//!
//! # Responsibility
//! [`generate`] turns a family and a seed into a [`Scenario`]: the map, the scratch archetypes in
//! the catalog's RON form, the spawns, the order every unit is given, how many ticks it runs and
//! the known answer the metrics judge it by (plan `2026-11-20-contact-solver` §6.2; the eight
//! families of plan `2026-11-27-gated-movement` §5.2 are generated in [`families`]). The draws come
//! from one [`SplitMix64`] stream per scenario. Every coordinate and size of the scene is an
//! integer number of sixty-fourths of a world unit ([`Lattice`]), so its conversion to the
//! simulation's Q32.32 is exact and the corpus is the same on every platform; the speeds and weapon
//! ranges are catalog decimals, converted by the catalog loader as the shipped data is.
//!
//! # How it fits
//! [`super::run_scenario`] builds a world from a scenario and plays it under a
//! [`super::metrics::ScenarioMeter`]. The archetypes are scratch ones generated here (charter
//! §3.3: no shipped identity is named); only the mirror family ([`super::mirror`]) uses the
//! shipped armies.
//!
//! # Invariants
//! - `generate(family, seed, relabel)` depends on nothing else: the same arguments give the same
//!   scenario, bit for bit.
//! - A relabelled scenario is the original with its spawn list permuted, and nothing else changed:
//!   the entity ids the world gives out follow the spawn order, so relabelling permutes the ids.
//! - Every spawn and goal lies inside the map's terrain-radius domain on open ground.

mod families;

pub use families::circle_corners;

use super::answers::KnownAnswer;
use super::Family;
use std::fmt::Write as _;

/// A length or coordinate in sixty-fourths of a world unit.
pub type Lattice = i64;

/// Sixty-fourths per world unit.
pub const LATTICE: Lattice = 64;

/// The radii a scratch archetype is drawn from, in sixty-fourths: 0.375, 0.5, 0.5625, 0.625,
/// 0.75, 0.875 and 1.0 world units (§6.2).
pub const RADII: [Lattice; 7] = [24, 32, 36, 40, 48, 56, 64];

/// The speeds a scratch archetype is drawn from, world units per tick, as catalog decimals (§6.2).
pub const SPEEDS: [&str; 4] = ["0.1", "0.125", "0.15625", "0.1875"];

/// Weapon ranges of the arc family's attackers, world units, as catalog decimals (§6.2).
const ARC_RANGES: [&str; 5] = ["0.1", "0.5", "1", "2", "4"];

/// Weapon ranges of the corridor family's attackers, world units, as catalog decimals (§6.2).
const CORRIDOR_RANGES: [&str; 4] = ["0.25", "0.5", "0.75", "1"];

/// Push priority of the scenes' units, and of the gap family's second archetype (§6.2).
const PRIORITY: u8 = 10;
const SECOND_PRIORITY: u8 = 20;

/// Gap between the edges of two units of a spawn block, sixty-fourths (a quarter unit).
const BLOCK_SLACK: Lattice = 16;

/// Sixteen headings round the circle, `round(64 · (cos, sin))` of multiples of 22.5°: integer
/// directions, so a bearing never goes through a platform's trigonometric functions.
const HEADINGS: [(Lattice, Lattice); 16] = [
    (64, 0),
    (59, 24),
    (45, 45),
    (24, 59),
    (0, 64),
    (-24, 59),
    (-45, 45),
    (-59, 24),
    (-64, 0),
    (-59, -24),
    (-45, -45),
    (-24, -59),
    (0, -64),
    (24, -59),
    (45, -45),
    (59, -24),
];

/// The deterministic stream every scenario draws from: Steele, Lea and Flood's `SplitMix64`.
#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// A stream seeded with `seed`.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The stream of one scenario: the family's index and the seed mixed into one state.
    #[must_use]
    pub fn for_scenario(family: Family, seed: u64, salt: u64) -> Self {
        let mut mix = Self::new(seed ^ (family.index() << 56) ^ salt.rotate_left(29));
        let state = mix.next_u64();
        Self::new(state)
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A draw in `0..bound` (`bound` > 0), by rejection so every value is equally likely.
    pub fn below(&mut self, bound: u64) -> u64 {
        let bound = bound.max(1);
        let zone = u64::MAX - (u64::MAX % bound);
        loop {
            let value = self.next_u64();
            if value < zone {
                return value % bound;
            }
        }
    }

    /// A draw in `low..=high`.
    pub fn between(&mut self, low: i64, high: i64) -> i64 {
        let span = high.abs_diff(low).saturating_add(1);
        low.saturating_add_unsigned(self.below(span))
    }

    /// One element of `items`, which must not be empty.
    pub fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        let index = usize::try_from(self.below(items.len() as u64)).unwrap_or(0);
        items[index.min(items.len().saturating_sub(1))]
    }

    /// The fraction `1 / denominator` came up.
    pub fn one_in(&mut self, denominator: u64) -> bool {
        self.below(denominator) == 0
    }
}

/// A scratch unit archetype of one scenario, written into the catalog's RON form by
/// [`ScratchArchetype::ron`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScratchArchetype {
    /// Collision (and terrain) radius, sixty-fourths.
    pub radius: Lattice,
    /// Top speed, world units per tick, as a catalog decimal; `"0"` for an immobile body.
    pub speed: &'static str,
    /// Weapon range, world units, as a catalog decimal; `None` for an unarmed body.
    pub range: Option<&'static str>,
    /// Push priority.
    pub priority: u8,
}

impl ScratchArchetype {
    /// The archetype's entry in the catalog's `units.ron` form, with `slug`, `id` and `faction`.
    #[must_use]
    pub fn ron(&self, slug: &str, id: u16, faction: &str) -> String {
        let weapon = self.range.map_or_else(|| "None".to_owned(), weapon_ron);
        let radius = world(self.radius);
        format!(
            r#"(slug: "{slug}", unit_type_id: {id}, faction: "{faction}", display_name: "{slug}",
            lore: "Scratch archetype of the movement harness.", sc2_analogue: "None",
            showcase_hero: false, showcase_order: None, shape: UnitSphere, silhouette: LightProngs,
            mover_type: Ground, attributes: (bits: 1), max_hp: 1000000.0, max_shields: 0.0,
            max_energy: 0.0, start_energy: 0.0, life_armor: 0.0, shield_armor: 0.0,
            hp_regen_per_tick: 0.0, shield_regen_per_tick: 0.0, energy_regen_per_tick: 0.0,
            max_speed: {speed}, acceleration: 3.136, deceleration: {speed},
            turn_rate_deg_per_tick: 55.99, collision_radius: {radius}, separation_radius: {radius},
            push_priority: {priority}, sight_radius: 9.0, scan_range: 5.5,
            attack_target_priority: 20, primary_weapon: Either, acquire_tie_break: Closest,
            mineral_cost: 50, gas_cost: 0, supply_cost: 1, supply_provided: 0,
            build_time_ticks: 100, weapon: {weapon})"#,
            speed = self.speed,
            priority = self.priority,
        )
    }
}

/// Ticks between two releases of the scratch weapon ([`weapon_ron`]).
pub const WEAPON_PERIOD: u32 = 15;

/// A scratch weapon of `range` world units: hitscan, 6 damage every [`WEAPON_PERIOD`] ticks, no
/// random delay.
fn weapon_ron(range: &str) -> String {
    format!(
        r#"Some((name: "Scratch Rifle", visual_kind: Hitscan, target_flags: (bits: 255),
        min_range: 0.0, max_range: {range}, firing_arc_deg: 11.25, acquire_arc_deg: 0.0,
        has_turret: false, turret_turn_rate_deg_per_tick: 0.0, turret_idle: Hold, base_damage: 6.0,
        attribute_bonuses: [], attacks_count: 1, strike_interval_ticks: 0, period_ticks: {WEAPON_PERIOD},
        damage_point_ticks: 1, backswing_ticks: 13, random_delay_ticks: (0.0, 0.0), splash: None,
        projectile_speed: None, weapon_class: None, upgrade_bonus_per_level: 0.0,
        bonus_upgrade_per_level: 0.0))"#
    )
}

/// `value` sixty-fourths as a world-unit decimal (exact: a sixty-fourth has six binary digits).
#[must_use]
pub fn world(value: Lattice) -> String {
    let whole = value.div_euclid(LATTICE);
    let fraction = value.rem_euclid(LATTICE);
    if fraction == 0 {
        return format!("{whole}.0");
    }
    // 1/64 = 0.015625: six decimal digits are exact.
    let digits = format!("{:06}", fraction * 15_625);
    format!("{whole}.{}", digits.trim_end_matches('0'))
}

/// One unit placed at the start of a scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spawn {
    /// Index into [`Scenario::archetypes`].
    pub archetype: usize,
    /// Owner slot: 0 the units under test, 1 the enemy pillar.
    pub owner: u8,
    /// Centre, sixty-fourths.
    pub at: (Lattice, Lattice),
    /// What the unit is told to do.
    pub role: Role,
}

/// What one spawned unit is told to do, and how it is judged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Walks to `goal` (sixty-fourths) under one `Move` shared with every other walker.
    GroupMove {
        /// The destination.
        goal: (Lattice, Lattice),
    },
    /// Walks to `goal` (sixty-fourths) under a `Move` of its own.
    OwnMove {
        /// The destination.
        goal: (Lattice, Lattice),
    },
    /// Attacks the scenario's target: its pillar, or the unit that walks a circle.
    Attack,
    /// The immobile enemy pillar; it has no order.
    Pillar,
    /// Holds position where it spawned (`HoldPosition`, one command per owner).
    Hold,
    /// Stands where it spawned with no order: an idle body, or a dummy target.
    Stand,
    /// Attack-moves to `goal` (sixty-fourths) under one command shared with every other
    /// attack-mover of its owner.
    AttackMove {
        /// The destination.
        goal: (Lattice, Lattice),
    },
    /// Attacks the unit spawned at `target` (sixty-fourths) under an explicit order of its own.
    /// The target is named by its spawn point, not its index, so relabelling keeps it.
    AttackAt {
        /// The target's spawn point.
        target: (Lattice, Lattice),
    },
    /// Walks the sixteen-sided polygon of radius `radius` round `centre` (sixty-fourths), its
    /// corners at [`HEADINGS`], under queued `Move`s, starting at the corner nearest its spawn;
    /// the scenario's target when there is no pillar.
    Circle {
        /// The circle's centre.
        centre: (Lattice, Lattice),
        /// The circle's radius.
        radius: Lattice,
    },
}

/// An order given at a set tick after the opening, to every unit whose role is
/// [`Role::Attack`] (the stutter family's back-and-forth).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cue {
    /// The tick whose bundle carries the order (the opening's bundle is tick 0).
    pub tick: u32,
    /// What the units are told.
    pub order: CueOrder,
}

/// What a [`Cue`] tells the attackers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CueOrder {
    /// `Move` to the unit's position when the cue is given plus `offset` (sixty-fourths), one
    /// command per unit.
    MoveBy {
        /// The offset from the unit's position.
        offset: (Lattice, Lattice),
    },
    /// Attack the scenario's target again, one command for them all.
    Attack,
}

/// The terrain of a scenario: open ground with `blocked` cells and gas geysers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MapSpec {
    /// Width and height in cells.
    pub size: (u32, u32),
    /// Cells without ground.
    pub blocked: Vec<(u32, u32)>,
    /// Anchor cells of gas geysers, placed with the shipped footprint.
    pub geysers: Vec<(u32, u32)>,
}

/// The known answer a scenario is judged against (§6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// Walkers must pass the wall at column `wall` through the opening of rows `rows`; `None` when
    /// the opening is narrower than the walkers and nobody may pass.
    Wall {
        /// The wall's column.
        wall: u32,
        /// The opening's centre, sixty-fourths: the waypoint of the progress potential.
        opening: (Lattice, Lattice),
        /// Whether the gap is wide enough for the units of the first archetype.
        passable: bool,
    },
    /// Walkers must complete their own moves.
    Moves,
    /// At least `expected` attackers must reach range of the pillar.
    Ring,
}

/// One generated scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scenario {
    /// The family.
    pub family: Family,
    /// The seed.
    pub seed: u64,
    /// Whether the spawn order was permuted ([`relabel`]).
    pub relabel: bool,
    /// The drawn parameters as `key=value` pairs joined by `;`, for the CSV.
    pub params: String,
    /// The terrain.
    pub map: MapSpec,
    /// The scratch archetypes, unit type ids 1, 2, … in this order.
    pub archetypes: Vec<ScratchArchetype>,
    /// The units, in spawn order.
    pub spawns: Vec<Spawn>,
    /// Ticks the scenario runs: the family's bound.
    pub ticks: u32,
    /// Units expected to complete (gap, cross, corner) or to reach range (arc, corridor).
    pub expected: u32,
    /// What the scene is judged against.
    pub answer: Answer,
    /// Orders given after the opening, in tick order; empty but for the stutter family.
    pub script: Vec<Cue>,
    /// The known answer of a family of plan `2026-11-27-gated-movement` §5.2
    /// ([`super::answers`]); `None` for the older families.
    pub known: Option<KnownAnswer>,
}

/// The scenario of `family` and `seed`; `relabel` permutes its spawn order (§6.2).
///
/// # Panics
/// Never for the scene families; the mirror family is generated by [`super::mirror`] and gives an
/// empty scene here.
#[must_use]
pub fn generate(family: Family, seed: u64, relabel: bool) -> Scenario {
    let mut rng = SplitMix64::for_scenario(family, seed, 0);
    let mut scenario = match family {
        Family::Gap => gap(&mut rng, seed),
        Family::Arc => arc(&mut rng, seed),
        Family::Cross => cross(&mut rng, seed),
        Family::Corridor => corridor(&mut rng, seed),
        Family::Corner => corner(&mut rng, seed),
        Family::HoldLine => families::hold_line(&mut rng, seed),
        Family::EnemyPress => families::enemy_press(&mut rng, seed),
        Family::DeepQueue => families::deep_queue(&mut rng, seed),
        Family::Wedge => families::wedge(&mut rng, seed),
        Family::Hallway => families::hallway(&mut rng, seed),
        Family::Stutter => families::stutter(&mut rng, seed),
        Family::SurroundMoving => families::surround_moving(&mut rng, seed),
        Family::Lanes => families::lanes(&mut rng, seed),
        Family::Mirror => Scenario {
            family,
            seed,
            relabel: false,
            params: String::new(),
            map: MapSpec::default(),
            archetypes: Vec::new(),
            spawns: Vec::new(),
            ticks: 0,
            expected: 0,
            answer: Answer::Moves,
            script: Vec::new(),
            known: None,
        },
    };
    if relabel {
        let mut order = SplitMix64::for_scenario(family, seed, 0x05EE_D0F1_ABE1);
        permute(&mut scenario.spawns, &mut order);
        scenario.relabel = true;
    }
    scenario
}

/// Fisher–Yates over `items` with `rng`.
pub fn permute<T>(items: &mut [T], rng: &mut SplitMix64) {
    for last in (1..items.len()).rev() {
        let pick = usize::try_from(rng.below(last as u64 + 1)).unwrap_or(0);
        items.swap(last, pick.min(last));
    }
}

/// Positions of `count` units of radius `radius` in a square block centred on `centre`, rows and
/// columns along `axis` (a [`HEADINGS`] entry) and its left normal, edges [`BLOCK_SLACK`] apart.
fn block(
    count: usize,
    radius: Lattice,
    centre: (Lattice, Lattice),
    axis: (Lattice, Lattice),
) -> Vec<(Lattice, Lattice)> {
    let columns = (1..=count).find(|c| c * c >= count).unwrap_or(1);
    let rows = count.div_ceil(columns);
    let pitch = 2 * radius + BLOCK_SLACK;
    let offset = |index: usize, of: usize| -> Lattice {
        let index = i64::try_from(index).unwrap_or(0);
        let of = i64::try_from(of).unwrap_or(1);
        pitch * (2 * index - (of - 1)) / 2
    };
    (0..count)
        .map(|unit| {
            let (along, across) = (
                offset(unit / columns, rows),
                offset(unit % columns, columns),
            );
            let x = centre.0 + (along * axis.0 - across * axis.1) / LATTICE;
            let y = centre.1 + (along * axis.1 + across * axis.0) / LATTICE;
            (x, y)
        })
        .collect()
}

/// `params` with `key=value` appended.
fn param(params: &mut String, key: &str, value: impl std::fmt::Display) {
    if !params.is_empty() {
        params.push(';');
    }
    // Writing to a `String` cannot fail.
    if write!(params, "{key}={value}").is_err() {
        params.push('?');
    }
}

/// `value` sixty-fourths as the shortest decimal, for the parameter string.
fn decimal(value: Lattice) -> String {
    let text = world(value);
    text.strip_suffix(".0").map_or(text.clone(), str::to_owned)
}

/// The gap family: a one-cell wall across a 48 × 48 map with a gap of `w` cells (§6.2).
fn gap(rng: &mut SplitMix64, seed: u64) -> Scenario {
    let (w, radius) = loop {
        let w: Lattice = rng.between(1, 6);
        let radius = rng.pick(&RADII);
        // `w / 2r` in [0.7, 4.5] and never within 6 % of 1, in integer arithmetic.
        let ratio_permille = w * LATTICE * 1000 / (2 * radius);
        if (700..=4500).contains(&ratio_permille) && (ratio_permille - 1000).abs() > 60 {
            break (w, radius);
        }
    };
    let n = usize::try_from(rng.between(6, 24)).unwrap_or(6);
    let speed = rng.pick(&SPEEDS);
    let mixed = rng.one_in(4);
    let mut params = String::new();
    param(&mut params, "w", w);
    param(&mut params, "r", decimal(radius));
    param(&mut params, "n", n);
    param(&mut params, "v", speed);
    param(&mut params, "mixed", u8::from(mixed));
    let wall = 24_u32;
    let low = u32::try_from(24 - w / 2 + rng.between(-4, 4)).unwrap_or(20);
    let rows = low..low + u32::try_from(w).unwrap_or(1);
    let map = MapSpec {
        size: (48, 48),
        blocked: (0..48)
            .filter(|y| !rows.contains(y))
            .map(|y| (wall, y))
            .collect(),
        geysers: Vec::new(),
    };
    walkers_through_wall(
        rng,
        WallScene {
            family: Family::Gap,
            seed,
            walkers: (radius, speed, n, mixed),
            map,
            params,
            opening: (
                i64::from(wall) * LATTICE + 32,
                i64::from(low) * LATTICE + w * 32,
            ),
            passable: w * LATTICE > 2 * radius,
            width: w * LATTICE,
        },
    )
}

/// The drawn facts of a walled scene (gap, corner).
struct WallScene {
    family: Family,
    seed: u64,
    /// Radius (sixty-fourths), speed, count and whether half the walkers are the second archetype.
    walkers: (Lattice, &'static str, usize, bool),
    map: MapSpec,
    params: String,
    /// The opening's centre, sixty-fourths.
    opening: (Lattice, Lattice),
    passable: bool,
    /// The opening's narrowest width, sixty-fourths.
    width: Lattice,
}

/// The scene of a walled family (gap, corner): the walkers in a block west of the wall, one shared
/// `Move` to a point east of it; the bound is §6.2's, with the opening's narrowest width.
fn walkers_through_wall(rng: &mut SplitMix64, scene: WallScene) -> Scenario {
    let WallScene {
        family,
        seed,
        walkers: (radius, speed, n, mixed),
        map,
        params,
        opening,
        passable,
        width,
    } = scene;
    let mut archetypes = vec![ScratchArchetype {
        radius,
        speed,
        range: None,
        priority: PRIORITY,
    }];
    if mixed {
        archetypes.push(ScratchArchetype {
            radius: (radius * 3 + 2) / 4,
            speed,
            range: None,
            priority: SECOND_PRIORITY,
        });
    }
    let centre = (
        rng.between(8, 12) * LATTICE,
        (24 + rng.between(-6, 6)) * LATTICE,
    );
    let goal = (40 * LATTICE, (24 + rng.between(-8, 8)) * LATTICE);
    let spawns = block(n, radius, centre, HEADINGS[0])
        .into_iter()
        .enumerate()
        .map(|(index, at)| Spawn {
            archetype: usize::from(mixed && index % 2 == 1),
            owner: 0,
            at,
            role: Role::GroupMove { goal },
        })
        .collect();
    let lanes = ((width - 2 * radius) / (2 * radius)).max(0) + 1;
    let v = speed_value(speed);
    // 2 · (50 / v + n · 2r / (lanes · v)) + 100, in whole ticks.
    let diameter = f64_of(2 * radius);
    let bound = 2.0 * (50.0 / v + count_f64(n) * diameter / (lattice_count_f64(lanes) * v)) + 100.0;
    Scenario {
        family,
        seed,
        relabel: false,
        params,
        map,
        archetypes,
        spawns,
        ticks: ticks_of(bound),
        expected: if passable {
            u32::try_from(n).unwrap_or(0)
        } else {
            0
        },
        answer: Answer::Wall {
            wall: 24,
            opening,
            passable,
        },
        script: Vec::new(),
        known: None,
    }
}

/// The corner family (B-152, plan §7 WP1): the gap family's wall with a gas geyser at each side of
/// its opening, so the opening's mouth is bounded by rounded footprints. Only passable openings are
/// drawn: the narrowest width between the geysers' lobes, `w − 1/2`, is between 1.1 and 4.5
/// diameters.
fn corner(rng: &mut SplitMix64, seed: u64) -> Scenario {
    let (w, radius) = loop {
        let w: Lattice = rng.between(2, 6);
        let radius = rng.pick(&RADII);
        let ratio_permille = (w * LATTICE - 32) * 1000 / (2 * radius);
        if (1100..=4500).contains(&ratio_permille) {
            break (w, radius);
        }
    };
    let n = usize::try_from(rng.between(6, 24)).unwrap_or(6);
    let speed = rng.pick(&SPEEDS);
    let mut params = String::new();
    param(&mut params, "w", w);
    param(&mut params, "r", decimal(radius));
    param(&mut params, "n", n);
    param(&mut params, "v", speed);
    let wall = 24_u32;
    let w_cells = u32::try_from(w).unwrap_or(2);
    let low = u32::try_from(21 - w / 2 + rng.between(-3, 3)).unwrap_or(18);
    let span = low..low + w_cells + 6;
    let map = MapSpec {
        size: (48, 48),
        blocked: (0..48)
            .filter(|y| !span.contains(y))
            .map(|y| (wall, y))
            .collect(),
        geysers: vec![(wall, low + 1), (wall, low + w_cells + 4)],
    };
    // The narrowest opening is between the geysers' lobes, a quarter unit into it on each side.
    walkers_through_wall(
        rng,
        WallScene {
            family: Family::Corner,
            seed,
            walkers: (radius, speed, n, false),
            map,
            params,
            opening: (
                i64::from(wall) * LATTICE + 32,
                i64::from(low + 3) * LATTICE + w * 32,
            ),
            passable: true,
            width: w * LATTICE - 32,
        },
    )
}

/// The arc family: an immobile enemy pillar at the centre of a 48 × 48 map and a block of
/// attackers 10 to 16 units away, all told to attack it (§6.2).
fn arc(rng: &mut SplitMix64, seed: u64) -> Scenario {
    let pillar = rng.between(1, 5) * 32;
    let range = rng.pick(&ARC_RANGES);
    let radius = rng.pick(&RADII);
    let k = usize::try_from(rng.between(4, 30)).unwrap_or(4);
    let speed = rng.pick(&SPEEDS);
    let distance = rng.between(10 * LATTICE, 16 * LATTICE);
    let heading = rng.pick(&HEADINGS);
    // C = ⌊π / asin(r / (P + r + R))⌋: a count of discs on a circle, judged in f64 outside the
    // simulation; the inputs are exact.
    let ring = f64_of(pillar + radius) + range_value(range);
    let capacity = (std::f64::consts::PI / (f64_of(radius) / ring).asin()).floor();
    let capacity = capacity_count(capacity);
    let mut params = String::new();
    param(&mut params, "P", decimal(pillar));
    param(&mut params, "R", range);
    param(&mut params, "r", decimal(radius));
    param(&mut params, "k", k);
    param(&mut params, "v", speed);
    param(&mut params, "cap", capacity);
    let centre = (24 * LATTICE, 24 * LATTICE);
    let at = (
        centre.0 + distance * heading.0 / LATTICE,
        centre.1 + distance * heading.1 / LATTICE,
    );
    let inward = (-heading.0, -heading.1);
    let mut spawns = vec![pillar_spawn(centre)];
    spawns.extend(block(k, radius, at, inward).into_iter().map(attacker));
    let bound = 2.0 * f64_of(distance) / speed_value(speed) + 500.0;
    Scenario {
        family: Family::Arc,
        seed,
        relabel: false,
        params,
        map: MapSpec {
            size: (48, 48),
            blocked: Vec::new(),
            geysers: Vec::new(),
        },
        archetypes: ring_archetypes(radius, speed, range, pillar),
        spawns,
        ticks: ticks_of(bound),
        expected: u32::try_from(k).unwrap_or(0).min(capacity),
        answer: Answer::Ring,
        script: Vec::new(),
        known: None,
    }
}

/// The corridor family: a corridor `w` cells wide along a 64 × 16 map with no way round, plugged by
/// an enemy pillar of radius `w / 2`, and 3 to 11 attackers told to attack it (§6.2).
fn corridor(rng: &mut SplitMix64, seed: u64) -> Scenario {
    let w: Lattice = rng.between(2, 4);
    let range = rng.pick(&CORRIDOR_RANGES);
    let radius = loop {
        let radius = rng.pick(&RADII);
        if 2 * radius <= w * LATTICE - 8 {
            break radius;
        }
    };
    let k = usize::try_from(rng.between(3, 11)).unwrap_or(3);
    let speed = rng.pick(&SPEEDS);
    let capacity = u32::try_from(w * LATTICE / (2 * radius + 4)).unwrap_or(0);
    let mut params = String::new();
    param(&mut params, "w", w);
    param(&mut params, "R", range);
    param(&mut params, "r", decimal(radius));
    param(&mut params, "k", k);
    param(&mut params, "v", speed);
    param(&mut params, "cap", capacity);
    let low = 8 - u32::try_from(w / 2).unwrap_or(1);
    let rows = low..low + u32::try_from(w).unwrap_or(2);
    let blocked = (0..16_u32)
        .filter(|y| !rows.contains(y))
        .flat_map(|y| (0..64_u32).map(move |x| (x, y)))
        .collect();
    let middle = i64::from(low) * LATTICE + w * 32;
    let pillar_at = (rng.between(48, 54) * LATTICE, middle);
    let queue_at = (rng.between(4, 10) * LATTICE, middle);
    let mut spawns = vec![pillar_spawn(pillar_at)];
    // A single file along the corridor, edges a quarter unit apart, the last nearest the pillar.
    spawns.extend((0..k).map(|index| {
        let ahead = i64::try_from(index).unwrap_or(0) * (2 * radius + BLOCK_SLACK);
        attacker((queue_at.0 + ahead, middle))
    }));
    let distance = pillar_at.0 - queue_at.0;
    let bound = 2.0 * f64_of(distance) / speed_value(speed) + 500.0;
    Scenario {
        family: Family::Corridor,
        seed,
        relabel: false,
        params,
        map: MapSpec {
            size: (64, 16),
            blocked,
            geysers: Vec::new(),
        },
        archetypes: ring_archetypes(radius, speed, range, w * 32),
        spawns,
        ticks: ticks_of(bound),
        expected: u32::try_from(k).unwrap_or(0).min(capacity),
        answer: Answer::Ring,
        script: Vec::new(),
        known: None,
    }
}

/// The attacker (type 1) and the pillar (type 2) of the ring families.
fn ring_archetypes(
    radius: Lattice,
    speed: &'static str,
    range: &'static str,
    pillar: Lattice,
) -> Vec<ScratchArchetype> {
    vec![
        ScratchArchetype {
            radius,
            speed,
            range: Some(range),
            priority: PRIORITY,
        },
        ScratchArchetype {
            radius: pillar,
            speed: "0.0",
            range: None,
            priority: PRIORITY,
        },
    ]
}

/// The pillar of a ring family at `at`.
const fn pillar_spawn(at: (Lattice, Lattice)) -> Spawn {
    Spawn {
        archetype: 1,
        owner: 1,
        at,
        role: Role::Pillar,
    }
}

/// An attacker of a ring family at `at`.
const fn attacker(at: (Lattice, Lattice)) -> Spawn {
    Spawn {
        archetype: 0,
        owner: 0,
        at,
        role: Role::Attack,
    }
}

/// The cross family: two friendly groups of 4 to 14 on a 48 × 48 map, each unit with a `Move` of
/// its own through the centre, the groups' headings 90°, 135° or 180° apart (§6.2).
fn cross(rng: &mut SplitMix64, seed: u64) -> Scenario {
    let theta = rng.pick(&[90_u32, 135, 180]);
    let n = usize::try_from(rng.between(4, 14)).unwrap_or(4);
    let radius = rng.pick(&RADII);
    let speed = rng.pick(&SPEEDS);
    let mut params = String::new();
    param(&mut params, "theta", theta);
    param(&mut params, "n", n);
    param(&mut params, "r", decimal(radius));
    param(&mut params, "v", speed);
    let first = rng.below(16);
    let turn = u64::from(theta / 45) * 2;
    let centre = (24 * LATTICE, 24 * LATTICE);
    let mut spawns = Vec::with_capacity(2 * n);
    for heading in [
        HEADINGS[usize::try_from(first).unwrap_or(0)],
        HEADINGS[usize::try_from((first + turn) % 16).unwrap_or(0)],
    ] {
        let start = (centre.0 - 12 * heading.0, centre.1 - 12 * heading.1);
        for at in block(n, radius, start, heading) {
            let goal = (at.0 + 24 * heading.0, at.1 + 24 * heading.1);
            spawns.push(Spawn {
                archetype: 0,
                owner: 0,
                at,
                role: Role::OwnMove { goal },
            });
        }
    }
    let spacing = f64_of(2 * radius + BLOCK_SLACK);
    let bound = 2.0 * (24.0 + 2.0 * count_f64(n) * spacing) / speed_value(speed) + 100.0;
    Scenario {
        family: Family::Cross,
        seed,
        relabel: false,
        params,
        map: MapSpec {
            size: (48, 48),
            blocked: Vec::new(),
            geysers: Vec::new(),
        },
        archetypes: vec![ScratchArchetype {
            radius,
            speed,
            range: None,
            priority: PRIORITY,
        }],
        spawns,
        ticks: ticks_of(bound),
        expected: u32::try_from(2 * n).unwrap_or(0),
        answer: Answer::Moves,
        script: Vec::new(),
        known: None,
    }
}

/// `value` sixty-fourths in world units.
#[expect(
    clippy::cast_precision_loss,
    reason = "scene sizes are below 2^20 sixty-fourths, exact in f64"
)]
fn f64_of(value: Lattice) -> f64 {
    value as f64 / 64.0
}

/// A count as `f64`.
fn count_f64(count: usize) -> f64 {
    f64::from(u32::try_from(count).unwrap_or(u32::MAX))
}

/// A lane count as `f64`.
fn lattice_count_f64(count: Lattice) -> f64 {
    f64::from(i32::try_from(count).unwrap_or(i32::MAX))
}

/// A catalog speed decimal of [`SPEEDS`] as `f64`.
fn speed_value(text: &str) -> f64 {
    text.parse().unwrap_or(0.1)
}

/// A catalog range decimal as `f64`.
fn range_value(text: &str) -> f64 {
    text.parse().unwrap_or(0.0)
}

/// A bound in ticks, rounded down.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "bounds are positive and below a few thousand ticks"
)]
fn ticks_of(bound: f64) -> u32 {
    bound.max(0.0).floor() as u32
}

/// A ring capacity, already floored, as a count.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a ring capacity is a small positive whole number"
)]
fn capacity_count(value: f64) -> u32 {
    value.max(0.0) as u32
}
