//! The effect engine allocates nothing in steady state (plan `2026-11-10-spellcaster-mechanics`
//! D-18, §11): after a warm-up, 200 ticks with two live storms, a missile, a displacement and
//! twenty pulsing statuses allocate nothing in the engine. A second steady child does the same
//! with the four shipped spells (Psionic Storm, Abduct, Fungal Growth and Blinding Cloud) recast
//! throughout its window, so every shipped tree is live on every measured tick (plan WP6).
//!
//! **How it counts.** The workspace forbids `unsafe_code`, so no counting `#[global_allocator]`
//! can be installed (owner question Q0011; the plan's D-18 fallback). As
//! `crates/client/tests/sync_allocation_count_tests.rs` does, the parent test re-executes this
//! binary under `valgrind --tool=dhat`, which replaces `malloc` from outside the process and
//! records every heap block with the call stack that allocated it. Every Rust allocation reaches
//! `malloc` through `std::alloc::System`, so the record is complete for the child.
//!
//! **Why the count is scoped by call stack.** A whole tick allocates outside the engine (about
//! one block per unit per tick, in systems this plan does not own), so whole-process totals
//! cannot show that the engine adds nothing. The parent counts the *engine* blocks of each
//! profile: those whose stack passes through a frame of the engine ([`ENGINE_FRAMES`]: the
//! `sim_core::effects` module by path, its functions by name, because `sim_core` is optimised
//! in tests and inlined frames carry no path, and the render snapshot's effect views). DHAT
//! attributes a `realloc` to the program point of the block's first allocation (DHAT manual
//! §10.4), so a buffer the world reserved at construction and grew in a tick counts at its
//! construction stack; the engine's reservations are engine frames, so such growth is counted.
//! Each child therefore runs twice, with 0 and with [`MEASURED`] measured ticks, and the
//! difference of the engine counts is what the measured ticks allocated in the engine. The
//! *window* count, every block whose stack passes through [`WINDOW_FUNCTION`], the non-inlined
//! function that runs the measured ticks, is reported and used by the outside control.
//!
//! **Controls.** A counter that sees nothing would pass, so two children plant allocations:
//! - `planted_outside_ticks` allocates one block per measured tick in the window function
//!   itself. Its window must exceed the steady child's by exactly the tick count, and its
//!   engine difference must stay at zero: the scoping tells the engine apart.
//! - `planted_engine_ticks` starts more persistents inside the window than the world reserves
//!   (D-18), so the engine's own instance list and the snapshot's area list grow. Its engine
//!   difference must be positive: growth in the engine is attributed to it.
//!
//! No recorded stack may reach valgrind's depth limit, so no block can hide in a truncated
//! stack. A missing `valgrind` fails the parent with an instruction to install it; there is no
//! skip (charter §1.2). The children return at once unless [`TICKS_ENV`] is set, so a plain
//! `cargo test` runs them as no-ops.

#![allow(clippy::expect_used)]

#[path = "support/effect_world.rs"]
mod support;

use sim_core::SimWorld;
use sim_types::{EffectTarget, RenderSnapshot, SimVec2, Tick, TickBundle};
use std::path::Path;
use std::process::Command;
use support::{context, run, unit, world};

/// Environment variable carrying a child's measured tick count.
const TICKS_ENV: &str = "SPACE_SCUFFLE_EFFECT_TICKS";
/// Measured ticks of every child.
const MEASURED: u32 = 200;
/// Ticks run before the window, so first-use growth is outside it.
const WARM_TICKS: u32 = 20;
/// Persistents the engine-growth control starts inside its window: more than the 64 a world
/// reserves (`effects/state.rs`), so the instance list must grow.
const GROWTH_PERSISTENTS: u32 = 100;
/// Name of the function whose frames delimit the measured window in a recorded call stack.
const WINDOW_FUNCTION: &str = "measured_effect_ticks";
/// Frames that belong to the engine: the module by path, its functions by name (inlined
/// frames carry no path), and the snapshot's effect views (D-19).
const ENGINE_FRAMES: [&str; 31] = [
    "sim_core::effects::",
    "run_effect_walk",
    "enter_node",
    "next_child",
    "effect_point",
    "gather_search",
    "create_persistent",
    "launch_missile",
    "apply_status",
    "apply_spell_damage",
    "apply_spell_responses",
    "modify_vitals",
    "cancel_orders_by_effect",
    "step_effects",
    "step_effect_missiles",
    "missile_aim",
    "step_persistents",
    "step_statuses",
    "run_status_pulses",
    "step_displacements",
    "land_displaced",
    "refresh_status_modifiers",
    "first_valid_placement",
    "passes_filter",
    "unit_target",
    "statuses_of",
    "find_status",
    "write_effect_views",
    "write_areas",
    "push_link",
    "raise_status_flags",
];
/// Call-stack depth DHAT records; a stack that reaches it is truncated and fails the test.
const STACK_DEPTH: usize = 128;

/// What a child runs in its window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Variant {
    /// The busy effect world, ticking.
    Steady,
    /// The same, plus one allocation per tick in the window function.
    PlantedOutside,
    /// The same, plus persistents beyond the reserved capacity started in the window.
    PlantedEngine,
    /// The four shipped spells, recast on a schedule so each is live throughout.
    ShippedSpells,
}

impl Variant {
    /// Name of the child test that runs this variant, for `--exact`.
    const fn child_test(self) -> &'static str {
        match self {
            Self::Steady => "steady_effect_ticks",
            Self::PlantedOutside => "planted_outside_ticks",
            Self::PlantedEngine => "planted_engine_ticks",
            Self::ShippedSpells => "shipped_spell_ticks",
        }
    }
}

/// The world of plan §11: two storms, a missile, a displacement and twenty statuses.
fn busy_world() -> SimWorld {
    let mut w = world(
        r#"(name: "dot", duration_ticks: 2.5, period_ticks: Some(1.0),
            periodic_effect: Some("tick")),
           (name: "long", duration_ticks: 1000.0, period_ticks: Some(1.0),
            periodic_effect: Some("tick"))"#,
        r#"(name: "storm", node: Persistent((anchor: TargetPoint, period_ticks: 1.0, count: 1000,
            initial: Some(Use("search")), periodic: Some(Use("search"))))),
           (name: "search", node: Search((radius: 4.0, child: ApplyStatus((status: "dot"))))),
           (name: "tick", node: ModifyVitals((energy: Some((change: 0.0))))),
           (name: "long_status", node: ApplyStatus((status: "long"))),
           (name: "missile", node: LaunchMissile((to: TargetPoint, speed: 0.01,
            impact: Use("tick_point")))),
           (name: "tick_point", node: Search((radius: 1.0, child: Use("tick")))),
           (name: "pull", node: Displace((flight_ticks: 250))),
           (name: "marker", node: Persistent((anchor: TargetPoint, period_ticks: 1.0, count: 1000,
            periodic: Some(Use("tick_point")))))"#,
    );
    let mut units = Vec::new();
    for i in 0..20 {
        units.push(unit(&mut w, 0, 10 + (i % 5), 10 + (i / 5)));
    }
    for &id in &units {
        let ctx = context(&w, None, EffectTarget::Unit(id));
        run(&mut w, "long_status", ctx);
    }
    for centre in [SimVec2::from_i32(11, 11), SimVec2::from_i32(13, 12)] {
        let ctx = context(&w, None, EffectTarget::Point(centre));
        run(&mut w, "storm", ctx);
    }
    let ctx = context(
        &w,
        Some(units[0]),
        EffectTarget::Point(SimVec2::from_i32(40, 40)),
    );
    run(&mut w, "missile", ctx);
    let flier = unit(&mut w, 0, 30, 30);
    let mut ctx = context(&w, None, EffectTarget::Unit(flier));
    // The flight ends where it starts: the displacement sub-step runs every tick while the unit
    // stays in its cells, so no system outside the engine sees a moving unit.
    ctx.origin = SimVec2::from_i32(30, 30);
    run(&mut w, "pull", ctx);
    w
}

/// Ticks between two casts of each shipped spell in [`Variant::ShippedSpells`]: the areas and
/// statuses are cast again before the last one ends (storm 61 ticks, cloud 134, Fungal's
/// statuses 71 after its impact); Abduct, 30 ticks from the tongue's launch to the landing
/// here, is cast again only after it landed, so a tongue never meets a unit in flight.
const RECASTS: [(&str, u32); 4] = [
    ("psionic_storm", 60),
    ("blinding_cloud", 130),
    ("fungal_growth", 70),
    ("abduct", 40),
];

/// Twenty enemy units in a clump for the shipped spells, whose centre is (12, 11.5).
fn shipped_world() -> (SimWorld, Vec<sim_types::EntityId>) {
    let mut w = world("", "");
    let units: Vec<_> = (0..20)
        .map(|i| unit(&mut w, 1, 10 + (i % 5), 10 + (i / 5)))
        .collect();
    // Held units do not flee the storm's and Fungal's responses, so the clump stays put.
    for id in &units {
        w.entities
            .get_mut(id)
            .expect("spawned")
            .issue_order(sim_types::UnitOrder::HoldPosition, false)
            .expect("hold admitted");
    }
    (w, units)
}

/// Runs every shipped spell due on `tick` (a tick count from the child's start) at the clump
/// of [`shipped_world`], as player 0 without a caster unit; Abduct takes the clump's first
/// unit and lands it where it stands. The clump's life is restored first, so the storms and
/// Fungal Growths of the whole run kill nobody and every search keeps finding the same units.
fn cast_shipped(w: &mut SimWorld, units: &[sim_types::EntityId], tick: u32) {
    for id in units {
        let kind = w.entities[id].unit_type_id;
        let full = w.catalog.get(kind).expect("registered").max_hp;
        w.entities.get_mut(id).expect("alive").hp = full;
    }
    let centre = SimVec2::new(sim_types::SimScalar::from_i32(12), support::scalar("11.5"));
    for (spell, every) in RECASTS {
        if !tick.is_multiple_of(every) {
            continue;
        }
        let mut ctx = if spell == "abduct" {
            let target = units[0];
            let mut ctx = context(w, None, EffectTarget::Unit(target));
            ctx.origin = w.entities[&target].position;
            ctx
        } else {
            context(w, None, EffectTarget::Point(centre))
        };
        ctx.source_point = SimVec2::from_i32(12, 20);
        run(w, spell, ctx);
    }
}

// PERFORMANCE RATIONALE:
// Why: whole-process allocation records mix world construction and the systems outside the
// engine with the steady state under test.
// What: keep the measured window in a named, non-inlined function, so the parent can attribute
// each recorded allocation to the window, and to the engine, by its call stack.
// Trade-off: a function boundary kept solely for measurement scope.
#[inline(never)]
fn measured_effect_ticks(
    w: &mut SimWorld,
    output: &mut RenderSnapshot,
    bundle: &mut TickBundle,
    ticks: u32,
    (variant, units): (Variant, &[sim_types::EntityId]),
) {
    for tick in 0..ticks {
        if variant == Variant::ShippedSpells {
            cast_shipped(w, units, WARM_TICKS + tick);
        }
        bundle.tick = Tick(w.current_tick);
        w.step_into(bundle, output);
        match variant {
            Variant::Steady | Variant::ShippedSpells => {}
            Variant::PlantedOutside => drop(std::hint::black_box(Box::new(w.current_tick))),
            Variant::PlantedEngine => {
                if tick < GROWTH_PERSISTENTS {
                    let ctx = context(w, None, EffectTarget::Point(SimVec2::from_i32(50, 50)));
                    run(w, "marker", ctx);
                }
            }
        }
    }
}

/// Runs one child: returns at once without [`TICKS_ENV`], otherwise warms the busy world and
/// runs the measured ticks, then checks every kind of instance is still live.
fn run_child(variant: Variant) {
    let Some(ticks) = std::env::var(TICKS_ENV)
        .ok()
        .and_then(|t| t.parse::<u32>().ok())
    else {
        println!(
            "{}: {TICKS_ENV} is not set, so this child does nothing; \
             effect_engine_allocates_nothing_in_steady_state runs it under valgrind",
            variant.child_test()
        );
        return;
    };
    let (mut w, units) = if variant == Variant::ShippedSpells {
        shipped_world()
    } else {
        (busy_world(), Vec::new())
    };
    let mut output = RenderSnapshot::default();
    let mut bundle = TickBundle {
        tick: Tick(0),
        commands: Vec::new(),
        state_checksum: None,
    };
    for tick in 0..WARM_TICKS {
        if variant == Variant::ShippedSpells {
            cast_shipped(&mut w, &units, tick);
        }
        bundle.tick = Tick(w.current_tick);
        w.step_into(&bundle, &mut output);
    }
    measured_effect_ticks(&mut w, &mut output, &mut bundle, ticks, (variant, &units));
    assert_eq!(w.diagnostics().len(), 0);
    if variant == Variant::ShippedSpells {
        let spells = w.catalog.spells();
        for status in [
            "psionic_storm",
            "blinding_cloud",
            "fungal_growth",
            "abducted",
        ] {
            let id = spells.status_id(status).expect("a shipped status");
            assert!(
                w.effects()
                    .statuses()
                    .iter()
                    .any(|entry| entry.status == id),
                "{status} is live at the end of the window"
            );
        }
        assert!(w.effects().persistents().len() >= 2, "a storm and a cloud");
        return;
    }
    let storms = if variant == Variant::PlantedEngine {
        2 + GROWTH_PERSISTENTS.min(ticks) as usize
    } else {
        2
    };
    assert_eq!(w.effects().persistents().len(), storms, "the storms live");
    assert_eq!(w.effects().missiles().len(), 1, "the missile flies");
    assert!(
        w.effects().statuses().len() >= 40,
        "twenty long statuses and the storms'"
    );
    assert!(
        w.entities.values().any(|e| e.displacement.is_some()),
        "the unit flies"
    );
    assert_eq!(output.areas.len(), storms, "every storm is presented");
}

#[test]
fn steady_effect_ticks() {
    run_child(Variant::Steady);
}

#[test]
fn planted_outside_ticks() {
    run_child(Variant::PlantedOutside);
}

#[test]
fn planted_engine_ticks() {
    run_child(Variant::PlantedEngine);
}

#[test]
fn shipped_spell_ticks() {
    run_child(Variant::ShippedSpells);
}

/// The parts of a DHAT profile (`dhatFileVersion` 2, JSON) this test reads.
#[derive(serde::Deserialize)]
struct DhatProfile {
    /// Program points: one per distinct allocation stack.
    pps: Vec<ProgramPoint>,
    /// Frame table: the text of every stack frame, indexed by `ProgramPoint::fs`.
    ftbl: Vec<String>,
}

/// One allocation stack of a DHAT profile.
#[derive(serde::Deserialize)]
struct ProgramPoint {
    /// Total blocks allocated at this stack over the whole run.
    tbk: u64,
    /// Total bytes allocated at this stack over the whole run, growth by `realloc` included.
    tb: u64,
    /// The stack, innermost frame first, as indices into `DhatProfile::ftbl`.
    fs: Vec<usize>,
}

/// Blocks and bytes of one child's profile, by scope.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Counts {
    /// Blocks allocated with [`WINDOW_FUNCTION`] on the stack.
    window: u64,
    /// Blocks allocated with an engine frame on the stack, growth by `realloc` included.
    engine: u64,
    /// Bytes of those blocks.
    engine_bytes: u64,
}

impl Counts {
    /// What `self` allocated in the engine beyond `before`: (blocks, bytes).
    fn engine_since(self, before: Self) -> (u64, u64) {
        (
            self.engine.saturating_sub(before.engine),
            self.engine_bytes.saturating_sub(before.engine_bytes),
        )
    }
}

/// Counts the window and engine blocks of `profile`. Fails if a stack reached the recorded depth
/// limit.
fn counts(profile: &DhatProfile) -> Result<Counts, String> {
    let mark = |needles: &[&str]| -> Vec<bool> {
        profile
            .ftbl
            .iter()
            .map(|frame| needles.iter().any(|needle| frame.contains(needle)))
            .collect()
    };
    let window_frames = mark(&[WINDOW_FUNCTION]);
    let engine_frames = mark(&ENGINE_FRAMES);
    let on = |frames: &[bool], point: &ProgramPoint| {
        point
            .fs
            .iter()
            .any(|&frame| frames.get(frame).copied().unwrap_or(false))
    };
    let mut counts = Counts::default();
    for point in &profile.pps {
        if point.fs.len() >= STACK_DEPTH {
            return Err(format!(
                "a stack of {} frames reached the depth limit {STACK_DEPTH}",
                point.fs.len()
            ));
        }
        if on(&window_frames, point) {
            counts.window += point.tbk;
        }
        if on(&engine_frames, point) {
            counts.engine += point.tbk;
            counts.engine_bytes += point.tb;
        }
    }
    Ok(counts)
}

/// Reads a DHAT profile. DHAT writes JSON; every JSON document it writes is also valid RON, so
/// the workspace's existing `ron` reader parses it without a new dependency.
fn read_profile(path: &Path) -> Result<DhatProfile, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let value: ron::Value = ron::from_str(&text)
        .map_err(|error| format!("{} is not a DHAT profile: {error}", path.display()))?;
    value
        .into_rust()
        .map_err(|error| format!("{} has no `pps`/`ftbl`: {error}", path.display()))
}

/// Runs this test binary's child for `variant` with `ticks` measured ticks under DHAT, writing
/// its profile into `directory`, and returns its counts.
fn count_allocations(variant: Variant, ticks: u32, directory: &Path) -> Result<Counts, String> {
    let binary = std::env::current_exe().map_err(|error| format!("current_exe: {error}"))?;
    let profile = directory.join(format!("{}.{ticks}.json", variant.child_test()));
    let output = Command::new("valgrind")
        .arg("--tool=dhat")
        .arg(format!("--num-callers={STACK_DEPTH}"))
        .arg(format!("--dhat-out-file={}", profile.display()))
        .arg(&binary)
        .args([
            "--exact",
            variant.child_test(),
            "--nocapture",
            "--test-threads=1",
        ])
        .env(TICKS_ENV, ticks.to_string())
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => "valgrind is not installed. Install it (Debian or \
                 Ubuntu: `sudo apt-get install valgrind`); this test counts allocations with it \
                 and has no skip"
                .to_owned(),
            _ => format!("cannot start valgrind: {error}"),
        })?;
    if !output.status.success() {
        return Err(format!(
            "{variant:?} failed under valgrind ({}):\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    counts(&read_profile(&profile)?).map_err(|error| format!("{variant:?}: {error}"))
}

#[test]
fn effect_engine_allocates_nothing_in_steady_state() -> Result<(), String> {
    // Cargo's scratch directory for integration tests, inside the target directory.
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("effect_tree_allocation");
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("cannot create {}: {error}", directory.display()))?;
    let run = |variant| -> Result<(Counts, Counts), String> {
        Ok((
            count_allocations(variant, 0, &directory)?,
            count_allocations(variant, MEASURED, &directory)?,
        ))
    };
    let (steady_idle, steady) = run(Variant::Steady)?;
    let (outside_idle, outside) = run(Variant::PlantedOutside)?;
    let (engine_idle, engine) = run(Variant::PlantedEngine)?;
    let (shipped_idle, shipped) = run(Variant::ShippedSpells)?;
    println!(
        "over {MEASURED} ticks: steady {steady:?} (0 ticks {steady_idle:?}); planted outside \
         {outside:?} (0 ticks {outside_idle:?}); planted in the engine {engine:?} (0 ticks \
         {engine_idle:?}); shipped spells {shipped:?} (0 ticks {shipped_idle:?})"
    );
    assert_eq!(
        outside.window,
        steady.window + u64::from(MEASURED),
        "the planted control allocates once per tick in the window"
    );
    assert_eq!(
        outside.engine_since(outside_idle),
        (0, 0),
        "an allocation outside the engine is not the engine's"
    );
    assert!(
        engine.engine_since(engine_idle).0 > 0,
        "growth of the engine's instance list is counted as the engine's"
    );
    assert_eq!(
        steady.engine_since(steady_idle),
        (0, 0),
        "{MEASURED} ticks with two storms, a missile, a displacement and twenty statuses \
         allocated (blocks, bytes) in the engine"
    );
    assert_eq!(
        shipped.engine_since(shipped_idle),
        (0, 0),
        "{MEASURED} ticks with the four shipped spells recast throughout allocated (blocks, \
         bytes) in the engine"
    );
    Ok(())
}
