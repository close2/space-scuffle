//! Behaviour of the battle trace on scripted battles with known answers.
//!
//! # Responsibility
//! Pins the measures the battle 33 investigation reads from `sim_fixtures::trace` against
//! battles small enough to work out by hand: the tick a melee unit reaches its target, the ticks
//! its swings land, a swing cancelled because another attacker killed the target first, a shot
//! wasted on a target killed earlier in the same tick, and a unit held up behind a friend. It
//! also checks that tracing a battle does not change it and that the runner writes its files.
//!
//! # How it fits
//! The trace infers releases, cancellations and wasted shots from consecutive public states
//! (`trace::swing`, `BattleTrace::count_wasted_shots`). These tests are the evidence that the
//! inference matches what the simulation did: each premise (both swings winding up, one swing
//! in progress at the kill) is read from the world directly, independently of the trace.
//!
//! # Invariants
//! - Scripted battles are matrix fixtures in RON, played by the same `BattleRun` as the matrix.
//! - Expected ticks are derived from the catalog values, not copied from a run.
//! - A test whose premise is a swing winding up plays a scratch melee archetype with a wind-up
//!   (`WINDUP_MELEE`, added to the shipped catalog in the test), so it does not depend on
//!   whether the shipped melee unit winds up.

use sim_core::catalog::RuntimeCatalogs;
use sim_core::{UnitCatalog, UpgradeCatalog, WeaponPhase};
use sim_fixtures::matchup::{Battle, BattleRun, Matrix, MATRIX_RON};
use sim_fixtures::trace::cli::{run, trace_battle, RunError, TraceArgs};
use sim_fixtures::trace::matrix::run_matrix;
use sim_fixtures::trace::{Activity, BattleTrace, Team};
use sim_types::EntityId;
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

/// A melee unit and a ranged unit of the shipped catalog, as the scripted battles use them.
const MELEE: &str = "psi_grey";
const RANGED: &str = "pulse_bot";

/// A scratch melee archetype whose swing winds up, for the tests whose premise is a swing in
/// its wind-up. Its values are the zealot's before plan `2026-11-04-combat-sc2-alignment` WP4
/// (damage point 4, period 22, range 0.3, backswing 6), so the hand-placed spawn positions of
/// those tests keep their meaning whatever the shipped zealot becomes.
const WINDUP_MELEE: &str = "trace_windup_melee";

/// [`WINDUP_MELEE`] in the catalog's RON form.
const WINDUP_MELEE_RON: &str = r#"(
            slug: "trace_windup_melee",
            unit_type_id: 900,
            faction: "cosmic_enclave",
            display_name: "Trace Wind-Up Melee",
            lore: "Scratch archetype of trace_tests.rs.",
            sc2_analogue: "None",
            showcase_hero: false,
            showcase_order: None,
            shape: UnitSphere,
            silhouette: MeleeSpikes,
            mover_type: Ground,
            attributes: (bits: 17),
            max_hp: 100.0,
            max_shields: 50.0,
            max_energy: 0.0,
            start_energy: 0.0,
            life_armor: 1.0,
            shield_armor: 0.0,
            hp_regen_per_tick: 0.0,
            shield_regen_per_tick: 0.08,
            shield_regen_delay_ticks: Some(250),
            energy_regen_per_tick: 0.0,
            max_speed: 0.126,
            acceleration: 0.126,
            deceleration: 0.126,
            turn_rate_deg_per_tick: 180.0,
            collision_radius: 0.5,
            separation_radius: 0.6,
            push_priority: 25,
            sight_radius: 9.0,
            scan_range: 9.0,
            attack_target_priority: 20,
            primary_weapon: Either,
            acquire_tie_break: Closest,
            mineral_cost: 100,
            gas_cost: 0,
            supply_cost: 2,
            supply_provided: 0,
            build_time_ticks: 100,
            weapon: Some((
                name: "Wind-Up Blades",
                visual_kind: MeleeSlash,
                target_flags: (bits: 255),
                min_range: 0.0,
                max_range: 0.3,
                firing_arc_deg: 180.0,
                has_turret: false,
                turret_turn_rate_deg_per_tick: 0.0,
                base_damage: 8.0,
                bonus_armored: 0.0,
                bonus_light: 0.0,
                attacks_count: 2,
                strike_interval_ticks: 0,
                period_ticks: 22,
                damage_point_ticks: 4,
                backswing_ticks: 6,
                splash: None,
                projectile_speed: None,
            )),
            is_detector: false,
            capabilities: (overclock: None),
        ),"#;

/// The shipped catalogs with [`WINDUP_MELEE`] added to the unit list.
fn catalogs_with_windup_melee() -> Result<RuntimeCatalogs, Box<dyn Error>> {
    let units = sim_fixtures::UNITS_RON;
    let list_end = units.rfind(']').ok_or("end of the unit list")?;
    let edited = format!(
        "{}{WINDUP_MELEE_RON}\n{}",
        &units[..list_end],
        &units[list_end..]
    );
    let factions = sim_fixtures::shipped_factions();
    let units = UnitCatalog::from_ron_str_bundle(&factions, &edited, sim_fixtures::STRUCTURES_RON)?;
    let upgrades = UpgradeCatalog::from_ron_str(&factions, sim_fixtures::UPGRADES_RON)?;
    units.validate_upgrade_references(&upgrades)?;
    Ok(RuntimeCatalogs { units, upgrades })
}

/// A one-battle matrix: `blue` holds position, `red` attacks blue's first unit.
fn scripted(blue: &str, red: &str) -> Result<Battle, Box<dyn Error>> {
    let text = format!(
        "(seeds: [1], spawn_jitter: 0.0, battles: [(name: \"scripted\", claim: \"test\", \
         map_cells: 64, ticks: 400, blue: (squads: [{blue}], order: HoldPosition), \
         red: (squads: [{red}], order: AttackFirstEnemy), expect: (blue: (0, 0), red: (0, 0)))])"
    );
    let matrix = Matrix::from_ron_str(&text)?;
    Ok(matrix.battle("scripted")?.clone())
}

fn squad(unit: &str, x: f32, y: f32) -> String {
    format!("(unit: \"{unit}\", count: 1, columns: 1, origin: ({x}, {y}), spacing: 1.0)")
}

/// Ticks for a melee unit `distance` away from a ranged target (centre to centre) to bring it
/// inside its band, computed from the catalog.
fn ticks_to_contact(distance: f64) -> Result<u32, Box<dyn Error>> {
    let catalogs = sim_fixtures::shipped_catalogs();
    let melee = catalogs
        .units
        .get(catalogs.units.id_by_slug(MELEE).ok_or("melee unit")?)?;
    let ranged = catalogs
        .units
        .get(catalogs.units.id_by_slug(RANGED).ok_or("ranged unit")?)?;
    let weapon = melee.weapon.as_ref().ok_or("melee weapon")?;
    let band =
        f64::from((weapon.max_range + melee.collision_radius + ranged.collision_radius).to_f32());
    let speed = f64::from(melee.max_speed.to_f32());
    let mut ticks = 0_u32;
    while distance - speed * f64::from(ticks) > band {
        ticks += 1;
    }
    Ok(ticks)
}

/// The melee weapon's damage point and period, in ticks.
fn melee_timing() -> Result<(u32, u32), Box<dyn Error>> {
    let catalogs = sim_fixtures::shipped_catalogs();
    let melee = catalogs
        .units
        .get(catalogs.units.id_by_slug(MELEE).ok_or("melee unit")?)?;
    let weapon = melee.weapon.as_ref().ok_or("melee weapon")?;
    Ok((
        u32::from(weapon.damage_point_ticks),
        u32::from(weapon.period_ticks),
    ))
}

/// Ticks on which `team` released a shot or swing.
fn release_ticks(trace: &BattleTrace, team: Team) -> Vec<u32> {
    trace
        .ticks()
        .iter()
        .filter(|r| r.teams[team.index()].shots_released > 0)
        .map(|r| r.tick)
        .collect()
}

#[test]
fn a_melee_unit_makes_contact_and_swings_on_the_ticks_the_catalog_predicts() -> TestResult {
    let battle = scripted(&squad(RANGED, 15.0, 32.0), &squad(MELEE, 10.0, 32.0))?;
    let trace = trace_battle(&battle, sim_fixtures::shipped_catalogs(), 1, 0.0)?;
    let summary = trace.summary();
    let contact = ticks_to_contact(5.0)?;
    let (damage_point, period) = melee_timing()?;

    // The ranged unit is in range from the start; the melee unit closes the gap.
    assert_eq!(summary.first_engaged_tick, [Some(0), Some(contact)]);
    // Its swing starts on contact, lands after the damage point, and repeats every period; the
    // third swing kills (6 strikes of 8 against 45 life).
    let first = contact + damage_point;
    assert_eq!(
        release_ticks(&trace, Team::Red),
        vec![first, first + period, first + 2 * period]
    );
    assert_eq!(summary.totals[1].swings_started, 3);
    assert_eq!(summary.totals[1].cancelled(), 0);
    assert_eq!(summary.survivors, [0, 1]);
    assert_eq!(summary.end_tick, first + 2 * period);
    assert!((summary.totals[0].damage_taken - 45.0).abs() < 1e-3);
    // Each of the ranged unit's shots costs the melee unit 6 (shields) or 5 (spillover or life).
    for record in &trace.ticks()[1..] {
        let shots = record.teams[0].shots_released;
        let lost = record.teams[1].damage_taken;
        assert_eq!(shots > 0, lost > 0.0, "tick {}", record.tick);
        if shots > 0 {
            assert!(
                (4.99..=6.01).contains(&lost),
                "tick {}: {lost}",
                record.tick
            );
        }
    }
    let red_health_lost = summary.initial_health[1] - summary.remaining_health[1];
    assert!((summary.totals[1].damage_taken - red_health_lost).abs() < 1e-3);
    Ok(())
}

/// The first shipped unit whose weapon has a damage point of 0, read from the catalog.
fn damage_point_zero_unit() -> Result<String, Box<dyn Error>> {
    let catalogs = sim_fixtures::shipped_catalogs();
    let slug = catalogs
        .units
        .archetypes()
        .find(|arch| {
            arch.weapon
                .as_ref()
                .is_some_and(|weapon| weapon.damage_point_ticks == 0)
        })
        .map(|arch| arch.slug.clone());
    slug.ok_or_else(|| "no shipped unit has a damage point of 0".into())
}

/// A swing with damage point 0 starts and releases in one tick (plan
/// `2026-11-04-combat-sc2-alignment` D-3) and never shows a wind-up; the trace still counts its
/// start, so every release of such a unit is also a swing started.
#[test]
fn a_swing_that_starts_and_releases_in_one_tick_counts_as_started() -> TestResult {
    let unit = damage_point_zero_unit()?;
    let battle = scripted(&squad(RANGED, 15.0, 32.0), &squad(&unit, 11.0, 32.0))?;
    let trace = trace_battle(&battle, sim_fixtures::shipped_catalogs(), 1, 0.0)?;
    let red = &trace.summary().totals[1];
    assert!(red.shots_released > 0, "{unit} must fire");
    assert_eq!(red.cancelled(), 0, "{unit} has no wind-up to cancel");
    assert_eq!(
        red.swings_started, red.shots_released,
        "every swing of {unit} starts and releases in one tick"
    );
    Ok(())
}

/// A traced battle, its target (blue's first unit), red's units, and red's weapon phases per tick.
type Watched = (
    BattleTrace,
    EntityId,
    Vec<EntityId>,
    Vec<Vec<Option<WeaponPhase>>>,
);

/// Plays `battle` with a trace, and returns the trace and, for each tick, the weapon phases of
/// red's units read directly from the world. `catalogs` are the catalogs the battle is played on.
fn play_watching_red(
    battle: &Battle,
    catalogs: RuntimeCatalogs,
) -> Result<Watched, Box<dyn Error>> {
    let mut run = BattleRun::new(battle, catalogs, 1, 0.0)?;
    let mut trace = BattleTrace::new(&run.world, &run.blue, &run.red)?;
    let mut phases = vec![Vec::new()];
    while !run.is_finished() {
        assert!(run.step().is_empty(), "no diagnostics");
        trace.record(&run.world)?;
        phases.push(
            run.red
                .iter()
                .map(|id| run.world.entities.get(id).map(|e| e.weapon_phase.clone()))
                .collect(),
        );
    }
    Ok((trace, run.blue[0], run.red.clone(), phases))
}

fn is_winding_up_on(phase: Option<&WeaponPhase>, target: EntityId) -> bool {
    matches!(phase, Some(WeaponPhase::Windup { target_id, .. }) if *target_id == target)
}

#[test]
fn a_second_swing_released_on_the_tick_of_the_kill_is_wasted() -> TestResult {
    // Two melee units approach one target symmetrically, so they swing on the same ticks: 2 x 16
    // on the first, then the first in id order kills with 13 life left and the second swing hits
    // a dead target. The premise is two swings winding up, so the melee units are the scratch
    // archetype with a wind-up.
    let battle = scripted(
        &squad(RANGED, 15.0, 32.0),
        &format!(
            "{}, {}",
            squad(WINDUP_MELEE, 10.5, 29.0),
            squad(WINDUP_MELEE, 10.5, 35.0)
        ),
    )?;
    let (trace, target, _red, phases) = play_watching_red(&battle, catalogs_with_windup_melee()?)?;
    let summary = trace.summary();
    let end = summary.end_tick;
    // Premise, read from the world: both swings were winding up on the target the tick before.
    let before = &phases[usize::try_from(end - 1)?];
    assert!(before.iter().all(|p| is_winding_up_on(p.as_ref(), target)));

    assert_eq!(summary.survivors[0], 0);
    assert_eq!(summary.totals[1].shots_released, 4);
    assert_eq!(summary.totals[1].wasted_shots, 1);
    assert_eq!(
        trace.ticks()[usize::try_from(end)?].teams[1].wasted_shots,
        1
    );
    assert_eq!(summary.totals[1].cancelled(), 0);
    Ok(())
}

#[test]
fn a_swing_winding_up_when_another_attacker_kills_the_target_is_cancelled() -> TestResult {
    // The second melee unit starts a little farther away, so its swings run two ticks behind:
    // its second swing is still winding up when the first unit's second swing kills. The test
    // needs a melee unit with a wind-up, so both are the scratch archetype.
    let battle = scripted(
        &squad(RANGED, 15.0, 32.0),
        &format!(
            "{}, {}",
            squad(WINDUP_MELEE, 10.5, 29.0),
            squad(WINDUP_MELEE, 10.292, 35.139)
        ),
    )?;
    let (trace, target, red, phases) = play_watching_red(&battle, catalogs_with_windup_melee()?)?;
    let summary = trace.summary();
    let end = summary.end_tick;
    // Premise, read from the world: the late unit was winding up on the target at the kill.
    let late = red.len() - 1;
    let at_kill = &phases[usize::try_from(end)?];
    assert!(
        is_winding_up_on(at_kill[late].as_ref(), target),
        "{at_kill:?}"
    );

    // The trace stops when a side is gone, so the cancellation shows on the next tick; play it.
    let mut run = BattleRun::new(&battle, catalogs_with_windup_melee()?, 1, 0.0)?;
    let mut extended = BattleTrace::new(&run.world, &run.blue, &run.red)?;
    while !run.is_finished() {
        assert!(run.step().is_empty(), "no diagnostics");
        extended.record(&run.world)?;
    }
    // One tick past the end of the battle: the bound target no longer exists.
    let next = sim_types::TickBundle {
        tick: sim_types::Tick(run.ticks_played()),
        commands: Vec::new(),
        state_checksum: None,
    };
    run.world.step(&next);
    extended.record(&run.world)?;
    let totals = extended.summary().totals[1];
    assert_eq!(totals.cancelled_target_gone, 1);
    assert_eq!(totals.cancelled_out_of_band + totals.cancelled_other, 0);
    assert_eq!(totals.wasted_shots, 0);
    assert_eq!(totals.shots_released, 3);
    Ok(())
}

#[test]
fn a_unit_behind_a_friend_is_held_up_before_it_reaches_the_target() -> TestResult {
    // Three ranged units in a column hold position; two melee units come in line, one behind the
    // other, so the rear one cannot walk straight to the shared target.
    let battle = scripted(
        &format!(
            "{}, {}, {}",
            squad(RANGED, 15.0, 32.0),
            squad(RANGED, 15.0, 33.0),
            squad(RANGED, 15.0, 31.0)
        ),
        &format!("{}, {}", squad(MELEE, 12.0, 32.0), squad(MELEE, 10.8, 32.0)),
    )?;
    let trace = trace_battle(&battle, sim_fixtures::shipped_catalogs(), 1, 0.0)?;
    let summary = trace.summary();
    let held_up = summary.unit_seconds[1][Activity::Blocked.index()]
        + summary.unit_seconds[1][Activity::Flanking.index()];
    assert!(held_up > 0.0, "{:?}", summary.unit_seconds[1]);
    // Only the rear unit (spawned second, so the higher id) is ever held up.
    let front = trace
        .unit_rows()
        .iter()
        .filter(|row| row.sample.team == Team::Red)
        .map(|row| row.sample.id)
        .min()
        .ok_or("red units")?;
    assert!(trace.unit_rows().iter().all(|row| row.sample.id != front
        || !matches!(row.sample.activity, Activity::Blocked | Activity::Flanking)));
    Ok(())
}

#[test]
fn tracing_a_battle_changes_nothing_and_reproduces_the_matrix_result() -> TestResult {
    let matrix = Matrix::from_ron_str(MATRIX_RON)?;
    let battle = matrix.battle("33_24_marines_vs_12_zealots")?;
    let seed = matrix.seeds[1];
    let jitter = matrix.jitter_for(seed);

    let mut plain = BattleRun::new(battle, sim_fixtures::shipped_catalogs(), seed, jitter)?;
    while !plain.is_finished() {
        assert!(plain.step().is_empty(), "no diagnostics");
    }
    let mut traced = BattleRun::new(battle, sim_fixtures::shipped_catalogs(), seed, jitter)?;
    let mut trace = BattleTrace::new(&traced.world, &traced.blue, &traced.red)?;
    while !traced.is_finished() {
        assert!(traced.step().is_empty(), "no diagnostics");
        trace.record(&traced.world)?;
    }
    assert_eq!(
        plain.world.compute_checksum(),
        traced.world.compute_checksum()
    );
    assert_eq!(plain.ticks_played(), traced.ticks_played());

    let summary = trace.summary();
    let expect = &battle.expect;
    let blue = usize::try_from(summary.survivors[0])?;
    let red = usize::try_from(summary.survivors[1])?;
    assert!((expect.blue.0..=expect.blue.1).contains(&blue), "{blue}");
    assert!((expect.red.0..=expect.red.1).contains(&red), "{red}");
    // Every unit is counted in exactly one activity on every tick.
    for record in trace.ticks() {
        for side in &record.teams {
            assert_eq!(side.activity.iter().sum::<u32>(), side.alive);
        }
    }
    // Blue's health lost is at most what red's released swings can deal (two strikes of 8).
    assert!(summary.totals[0].damage_taken <= f64::from(summary.totals[1].shots_released) * 16.0);
    assert!(summary.contact[1].max_engaged <= 12);
    Ok(())
}

#[test]
fn the_runner_writes_every_file_for_every_seed() -> TestResult {
    let out = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("trace_runner_test");
    if out.exists() {
        std::fs::remove_dir_all(&out)?;
    }
    let args = TraceArgs::parse(
        [
            "--battle",
            "25a_2_marines_vs_zealot",
            "--seed",
            "11",
            "--seed",
            "22",
            "--out",
        ]
        .into_iter()
        .map(str::to_owned)
        .chain([out.display().to_string()]),
    )?;
    let output = run(&args)?;
    assert_eq!(output.results.len(), 2);
    let directory = out.join("25a_2_marines_vs_zealot");
    for seed in [11, 22] {
        for suffix in ["_ticks.csv", "_units.csv", ".md"] {
            let path = directory.join(format!("seed_{seed}{suffix}"));
            assert!(path.is_file(), "{}", path.display());
        }
    }
    let summary = std::fs::read_to_string(directory.join("summary.md"))?;
    assert!(summary.contains("## Outcomes"));
    assert!(summary.contains("2 seeds"));
    assert!(summary.contains("Units on the most-targeted enemy, mean"));
    let seeds = std::fs::read_to_string(directory.join("seeds.csv"))?;
    assert_eq!(seeds.lines().count(), 3);
    let columns: Vec<usize> = seeds.lines().map(|line| line.split(',').count()).collect();
    assert!(
        columns.iter().all(|&count| count == columns[0]),
        "{columns:?}"
    );
    assert!(seeds
        .lines()
        .next()
        .is_some_and(|header| header.contains("blue_mean_focus")));
    // Both marines attack the one zealot: the blue focus reaches 2, the red at most 1.
    let focus = &output.results[0].summary.focus;
    assert_eq!(focus[0].units, 2);
    assert_eq!(focus[0].max_focus, 2);
    assert_eq!(focus[1].max_focus, 1);
    assert!(focus[0].mean_first_target_tick.is_some());
    // The ticks CSV has one row per tick plus the header and tick 0.
    let ticks = std::fs::read_to_string(directory.join("seed_11_ticks.csv"))?;
    let end = output.results[0].summary.end_tick;
    assert_eq!(ticks.lines().count(), usize::try_from(end)? + 2);
    Ok(())
}

/// A two-battle matrix: a marine and a stalker each duel a roach, with the seeds of record
/// `3203386110` (authored positions) and `11` (jittered).
const TWO_BATTLES: &str = r#"(seeds: [3203386110, 11], spawn_jitter: 0.12, battles: [
    (name: "marine_vs_roach", claim: "test", map_cells: 64, ticks: 400,
     blue: (squads: [(unit: "pulse_bot", count: 1, columns: 1, origin: (10.0, 32.0), spacing: 1.0)],
            order: AttackMove(16, 32)),
     red: (squads: [(unit: "rhino_beetle", count: 1, columns: 1, origin: (16.0, 32.0), spacing: 1.0)],
           order: AttackMove(10, 32)),
     expect: (blue: (0, 0), red: (1, 1))),
    (name: "stalker_vs_roach", claim: "test", map_cells: 64, ticks: 400,
     blue: (squads: [(unit: "warp_strider", count: 1, columns: 1, origin: (10.0, 32.0), spacing: 1.0)],
            order: AttackMove(16, 32)),
     red: (squads: [(unit: "rhino_beetle", count: 1, columns: 1, origin: (16.0, 32.0), spacing: 1.0)],
           order: AttackMove(10, 32)),
     expect: (blue: (1, 1), red: (0, 0))),
])"#;

/// A fresh, empty directory under the test target directory.
fn fresh_dir(name: &str) -> Result<std::path::PathBuf, Box<dyn Error>> {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// The shipped catalog written to `dir`, with the first weapon period after `slug`'s entry in
/// `units.ron` replaced by `period`.
fn scratch_catalog(dir: &std::path::Path, slug: &str, period: u16) -> TestResult {
    let units = sim_fixtures::UNITS_RON;
    let entry = units
        .find(&format!("slug: \"{slug}\""))
        .ok_or("unit entry")?;
    let field = entry + units[entry..].find("period_ticks: ").ok_or("period")?;
    let value = field + "period_ticks: ".len();
    let end = value + units[value..].find(',').ok_or("end of the period value")?;
    let edited = format!("{}{period}{}", &units[..value], &units[end..]);
    for (name, text) in [
        ("factions.ron", sim_fixtures::FACTIONS_RON),
        ("units.ron", edited.as_str()),
        ("structures.ron", sim_fixtures::STRUCTURES_RON),
        ("upgrades.ron", sim_fixtures::UPGRADES_RON),
        ("start_loadouts.ron", sim_fixtures::START_LOADOUTS_RON),
    ] {
        std::fs::write(dir.join(name), text)?;
    }
    Ok(())
}

/// `matchup_trace` arguments, split on whitespace.
fn matrix_args(text: &str) -> Result<TraceArgs, RunError> {
    TraceArgs::parse(text.split_whitespace().map(str::to_owned))
}

#[test]
fn the_matrix_mode_writes_every_battle_and_seed_and_compares_runs() -> TestResult {
    let root = fresh_dir("trace_matrix_mode_test")?;
    let fixture = root.join("fixture.ron");
    std::fs::write(&fixture, TWO_BATTLES)?;
    let (first, second, third) = (root.join("first"), root.join("second"), root.join("third"));

    // The seeds of record plus one more, so three runs of each battle.
    let output = run_matrix(&matrix_args(&format!(
        "--all-battles --seeds 1 --fixture {} --out {}",
        fixture.display(),
        first.display()
    ))?)?;
    assert_eq!(output.rows.len(), 6);
    let csv = std::fs::read_to_string(first.join("matrix.csv"))?;
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(
        lines[0],
        "battle,seed,of_record,end_tick,blue_survivors,red_survivors,blue_lead_hull,\
         red_lead_hull,expectation_met"
    );
    assert_eq!(lines.len(), 7);
    let first_row: Vec<&str> = lines[1].split(',').collect();
    assert_eq!(&first_row[..3], ["marine_vs_roach", "3203386110", "true"]);
    assert!(
        lines[3].starts_with("marine_vs_roach,1,false,"),
        "{}",
        lines[3]
    );
    // The marine dies, so its hull is empty; the roach's is printed exactly.
    assert_eq!(first_row[6], "");
    assert!(first_row[7].parse::<f64>()? > 0.0, "{}", first_row[7]);
    // Each row's verdict is the shared expectation check on that row's outcome.
    let matrix = Matrix::from_ron_str(TWO_BATTLES)?;
    for row in &output.rows {
        let battle = matrix.battle(&row.battle)?;
        assert_eq!(row.expectation_met, battle.expect.holds(&row.outcome));
    }
    let summary = std::fs::read_to_string(first.join("matrix.md"))?;
    assert!(summary.contains("| marine_vs_roach | 3 |"), "{summary}");
    assert!(summary.contains("| stalker_vs_roach | 3 |"), "{summary}");
    assert!(output.comparison.is_none());

    // The same inputs again, compared with the first run: nothing moved.
    let compare = first.join("matrix.csv");
    let again = run_matrix(&matrix_args(&format!(
        "--all-battles --seeds 1 --fixture {} --out {} --compare {}",
        fixture.display(),
        second.display(),
        compare.display()
    ))?)?;
    let comparison = again.comparison.ok_or("a comparison")?;
    assert_eq!(comparison.moved, Vec::<String>::new());
    assert_eq!(std::fs::read_to_string(second.join("matrix.csv"))?, csv);
    let summary = std::fs::read_to_string(second.join("matrix.md"))?;
    assert!(summary.contains("bit-identical to"), "{summary}");

    // A catalog in which the marine fires half as often moves its battle, and only that one.
    let catalog = root.join("catalog");
    std::fs::create_dir_all(&catalog)?;
    scratch_catalog(&catalog, "pulse_bot", 30)?;
    let changed = run_matrix(&matrix_args(&format!(
        "--all-battles --seeds 1 --fixture {} --catalog-dir {} --out {} --compare {}",
        fixture.display(),
        catalog.display(),
        third.display(),
        compare.display()
    ))?)?;
    let comparison = changed.comparison.ok_or("a comparison")?;
    assert_eq!(comparison.moved, vec!["marine_vs_roach".to_owned()]);
    let summary = std::fs::read_to_string(third.join("matrix.md"))?;
    assert!(summary.contains("## Moved"), "{summary}");
    assert!(!summary.contains("bit-identical"), "{summary}");
    Ok(())
}

#[test]
fn the_matrix_mode_and_a_single_battle_are_exclusive() {
    for bad in [
        "--out o",
        "--all-battles --battle b --out o",
        "--battle b --out o --compare m.csv",
        "--all-battles --out o --jobs 0",
    ] {
        assert!(matches!(matrix_args(bad), Err(RunError::Usage(_))), "{bad}");
    }
}
