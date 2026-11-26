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
                acquire_arc_deg: 180.0,
                has_turret: false,
                turret_turn_rate_deg_per_tick: 0.0,
                turret_idle: Hold,
                base_damage: 8.0,
                attribute_bonuses: [],
                attacks_count: 2,
                first_strike_delay_ticks: 0,
                strike_interval_ticks: 0,
                period_ticks: 22,
                damage_point_ticks: 4,
                backswing_ticks: 6,
                random_delay_ticks: (0.0, 0.0),
                splash: None,
                projectile_speed: None,
            )),
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
fn melee_timing() -> Result<(u32, u32, u32), Box<dyn Error>> {
    let catalogs = sim_fixtures::shipped_catalogs();
    let melee = catalogs
        .units
        .get(catalogs.units.id_by_slug(MELEE).ok_or("melee unit")?)?;
    let weapon = melee.weapon.as_ref().ok_or("melee weapon")?;
    Ok((
        u32::from(weapon.damage_point_ticks),
        u32::from(weapon.period_ticks),
        u32::from(weapon.strike_interval_ticks),
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
    // The test predicts the swings from the period; the random firing delay (B-3) spreads it.
    let mut catalogs = sim_fixtures::shipped_catalogs();
    sim_fixtures::without_random_delays(&mut catalogs.units);
    let trace = trace_battle(&battle, catalogs, 1, 0.0)?;
    let summary = trace.summary();
    let contact = ticks_to_contact(5.0)?;
    let (damage_point, period, interval) = melee_timing()?;

    // The ranged unit is in range from the start; the melee unit closes the gap.
    assert_eq!(summary.first_engaged_tick, [Some(0), Some(contact)]);
    // Its swing starts on contact, lands after the damage point, and repeats every period; the
    // third swing's second strike, `interval` ticks after its first, kills (6 strikes of 8
    // against 45 life).
    let first = contact + damage_point;
    assert_eq!(
        release_ticks(&trace, Team::Red),
        vec![first, first + period, first + 2 * period]
    );
    assert_eq!(summary.totals[1].swings_started, 3);
    assert_eq!(summary.totals[1].cancelled(), 0);
    assert_eq!(summary.survivors, [0, 1]);
    assert_eq!(summary.end_tick, first + 2 * period + interval);
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
    // Only the rear unit (spawned second, so the higher id) is held up on the way in. Once the
    // front unit has engaged, the rear one may stand beside it, between it and its next target:
    // with the walk's end hysteresis (friendly-blocking plan Amendment 7) the front unit is held
    // up for one tick at its kill (tick 64), which is not the approach this test is about.
    let front = trace
        .unit_rows()
        .iter()
        .filter(|row| row.sample.team == Team::Red)
        .map(|row| row.sample.id)
        .min()
        .ok_or("red units")?;
    let engaged = trace
        .unit_rows()
        .iter()
        .find(|row| row.sample.id == front && row.sample.activity == Activity::Engaged)
        .map(|row| row.tick)
        .ok_or("the front unit engages")?;
    assert!(trace.unit_rows().iter().all(|row| row.sample.id != front
        || row.tick >= engaged
        || !matches!(row.sample.activity, Activity::Blocked | Activity::Flanking)));
    Ok(())
}

/// W0-T5 (evidence `2026-11-25-solver-pair-cache` §2): B7′'s engaged unit-ticks on
/// a constructed two-unit battle. The melee unit walks 5 world units to the holding ranged unit;
/// it counts exactly the observed states in which the ranged unit, its only target, lies within
/// its band (read here from the catalog's range and radii), and the ranged unit, in range of it
/// from the first tick, counts every state from its first acquisition on.
#[test]
fn engaged_unit_ticks_count_the_states_in_band_of_the_target() -> TestResult {
    use sim_fixtures::trace::contact_checks::ContactMeter;
    let battle = scripted(&squad(RANGED, 15.0, 32.0), &squad(MELEE, 10.0, 32.0))?;
    let mut catalogs = sim_fixtures::shipped_catalogs();
    sim_fixtures::without_random_delays(&mut catalogs.units);
    let melee = catalogs
        .units
        .get(catalogs.units.id_by_slug(MELEE).ok_or("melee unit")?)?;
    let ranged = catalogs
        .units
        .get(catalogs.units.id_by_slug(RANGED).ok_or("ranged unit")?)?;
    let weapon = melee.weapon.as_ref().ok_or("melee weapon")?;
    let band = weapon.max_range + melee.collision_radius + ranged.collision_radius;
    let mut run = BattleRun::new(&battle, catalogs, 1, 0.0)?;
    let (blue, red) = (run.blue[0], run.red[0]);
    let mut meter = ContactMeter::new();
    meter.observe(&run.world);
    let (mut in_band, mut states) = (0_u32, 1_u32);
    while !run.is_finished() {
        assert!(run.step().is_empty(), "no diagnostics");
        meter.observe(&run.world);
        let (Some(b), Some(r)) = (run.world.entities.get(&blue), run.world.entities.get(&red))
        else {
            break;
        };
        if !(b.is_alive() && r.is_alive()) {
            break;
        }
        states += 1;
        in_band += u32::from(b.position.distance(r.position) <= band);
    }
    let checks = meter.total();
    assert!(in_band > 0 && in_band < states, "{in_band} of {states}");
    assert_eq!(checks.sides[1].engaged_ticks, in_band);
    // The holding ranged unit is in range of the melee unit from its first acquisition on.
    assert!(
        checks.sides[0].engaged_ticks + 1 >= states - 1,
        "{checks:?}"
    );
    assert!(checks.sides[0].engaged_ticks <= states);
    Ok(())
}

/// A one-battle matrix of two `RANGED` units of blue `spacing` apart side by side under
/// `blue_order`, and one red `RANGED` unit far beyond every sight range, standing; no jitter.
fn friend_pair(spacing: f64, blue_order: &str) -> Result<Battle, Box<dyn Error>> {
    let text = format!(
        "(seeds: [1], spawn_jitter: 0.0, battles: [(name: \"pair\", claim: \"test\", \
         map_cells: 64, ticks: 12, blue: (squads: [(unit: \"{RANGED}\", count: 2, columns: 2, \
         origin: (10.0, 10.0), spacing: {spacing})], order: {blue_order}), \
         red: (squads: [{}]), expect: (blue: (0, 2), red: (0, 1)))])",
        squad(RANGED, 54.0, 54.0)
    );
    let matrix = Matrix::from_ron_str(&text)?;
    Ok(matrix.battle("pair")?.clone())
}

/// `RANGED`'s collision radius, world units.
fn ranged_radius() -> Result<f64, Box<dyn Error>> {
    let catalogs = sim_fixtures::shipped_catalogs();
    let arch = catalogs
        .units
        .get(catalogs.units.id_by_slug(RANGED).ok_or("ranged unit")?)?;
    Ok(f64::from(arch.collision_radius.to_f32()))
}

/// Plays `battle` to its end under a [`ContactMeter`] that also saw the starting state, and
/// returns the checks and the ticks played.
fn metered(
    battle: &Battle,
) -> Result<(sim_fixtures::trace::contact_checks::ContactChecks, u32), Box<dyn Error>> {
    use sim_fixtures::trace::contact_checks::ContactMeter;
    let mut run = BattleRun::new(battle, sim_fixtures::shipped_catalogs(), 1, 0.0)?;
    let mut meter = ContactMeter::new();
    meter.observe(&run.world);
    while !run.is_finished() {
        assert!(run.step().is_empty(), "no diagnostics");
        meter.observe(&run.world);
    }
    Ok((meter.total(), run.ticks_played()))
}

/// W0-T8 (plan `2026-11-26-friend-crowd-overlap` §2.2): two idle friends spawned at 30 %
/// overlap are separated by the first tick's solve. B6 is counted from the first stepped tick, so
/// the run reports no pair-tick above D-α's moving bound and a maximum below it; the spawn
/// state's 30 % is reported in the spawn-inclusive column only.
#[test]
fn friend_overlap_is_counted_from_the_first_stepped_tick() -> TestResult {
    let diameter = 2.0 * ranged_radius()?;
    let (checks, ticks) = metered(&friend_pair(0.7 * diameter, "None")?)?;
    assert!(ticks > 1, "{ticks} ticks");
    assert_eq!(checks.friend_overlap_ticks, 0, "{checks:?}");
    assert!(checks.friend_overlap < 0.05, "{checks:?}");
    let crowd = checks.crowd.ok_or("the meter reports its crowd columns")?;
    assert!(
        (crowd.friend_overlap_with_spawn - 0.3).abs() < 1e-3,
        "{crowd:?}"
    );
    assert_eq!(crowd.corrected_overlap_ticks, 0, "{crowd:?}");
    Ok(())
}

/// W0-T9 (plan `2026-11-26-friend-crowd-overlap` WP0): two friends holding position at 20 %
/// overlap are a pair the contact solver never corrects (C-2's "none"): they count in the
/// all-pairs columns on every stepped tick and not in the corrected-pairs columns.
#[test]
fn holding_friends_count_in_the_all_pairs_columns_only() -> TestResult {
    let diameter = 2.0 * ranged_radius()?;
    let (checks, ticks) = metered(&friend_pair(0.8 * diameter, "HoldPosition")?)?;
    assert!(ticks > 1, "{ticks} ticks");
    assert_eq!(checks.friend_overlap_ticks, ticks, "{checks:?}");
    assert!((checks.friend_overlap - 0.2).abs() < 1e-3, "{checks:?}");
    let crowd = checks.crowd.ok_or("the meter reports its crowd columns")?;
    assert_eq!(crowd.corrected_overlap_ticks, 0, "{crowd:?}");
    assert!(crowd.corrected_overlap.abs() < 1e-12, "{crowd:?}");
    Ok(())
}

/// B8 (plan `2026-11-26-friend-crowd-overlap` WP0, the collision-classes probe §10.1 (a)): two
/// ranged enemies spawned within each other's band, weapons ready. A state observed twice with
/// nothing done in between counts one unit-tick standing while able on each side; a tick in
/// which a unit starts its swing does not count for it.
#[test]
fn standing_while_able_counts_idle_ticks_and_not_swings() -> TestResult {
    use sim_fixtures::trace::contact_checks::ContactMeter;
    let battle = scripted(&squad(RANGED, 30.0, 32.0), &squad(RANGED, 33.0, 32.0))?;
    let mut run = BattleRun::new(&battle, sim_fixtures::shipped_catalogs(), 1, 0.0)?;
    let mut meter = ContactMeter::new();
    meter.observe(&run.world);
    meter.observe(&run.world);
    let crowd = meter.total().crowd.ok_or("crowd columns")?;
    assert_eq!(crowd.standing_able, [1, 1], "{crowd:?}");
    assert_eq!(crowd.standing_able_cross, [0, 0], "{crowd:?}");
    let units = [run.blue[0], run.red[0]];
    let mut swings = 0_u32;
    while !run.is_finished() && swings < 4 {
        let before = meter.total().crowd.ok_or("crowd columns")?.standing_able;
        assert!(run.step().is_empty(), "no diagnostics");
        meter.observe(&run.world);
        let after = meter.total().crowd.ok_or("crowd columns")?.standing_able;
        for (side, id) in units.iter().enumerate() {
            let Some(unit) = run.world.entities.get(id).filter(|u| u.is_alive()) else {
                continue;
            };
            let swung = !matches!(unit.weapon_phase, WeaponPhase::Ready)
                || unit.weapon_cooldown_remaining > 0;
            if swung {
                swings += 1;
                assert_eq!(after[side], before[side], "side {side} swung and counted");
            }
        }
    }
    assert!(swings > 0, "the units swing");
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
        ("movement_rules.ron", sim_fixtures::MOVEMENT_RULES_RON),
        ("placement_rules.ron", sim_fixtures::PLACEMENT_RULES_RON),
        ("statuses.ron", sim_fixtures::STATUSES_RON),
        ("ability_effects.ron", sim_fixtures::ABILITY_EFFECTS_RON),
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
         red_lead_hull,expectation_met,blocked_attacker_ticks,reversals,stall_giveups,\
         proposal_reversals,contact_reversals,blue_shove,red_shove,blue_r10_leash,\
         blue_r10_terrain,blue_r10_unruled,blue_r10_free,red_r10_leash,red_r10_terrain,\
         red_r10_unruled,red_r10_free,blue_churn_explained,blue_churn_unexplained,\
         red_churn_explained,red_churn_unexplained,blue_blocked,red_blocked,blue_shots,\
         red_shots,blue_unit_ticks,red_unit_ticks,enemy_overlap,blue_shove_e,red_shove_e,\
         blue_engaged_ticks,red_engaged_ticks,enemy_overlap_ticks,friend_overlap,\
         friend_overlap_ticks,friend_overlap_with_spawn,friend_overlap_corrected,\
         friend_overlap_corrected_ticks,blue_standing_able,red_standing_able,\
         blue_standing_able_cross,red_standing_able_cross,solver_extended_ticks,\
         solver_extended_iterations,blue_held_moved_ticks,red_held_moved_ticks,\
         blue_held_moved_max,red_held_moved_max,blue_moving_ticks,red_moving_ticks,\
         blue_reversals,red_reversals,blue_reversals_hi,red_reversals_hi,gate_rays,\
         orbit_refusals,terrain_transfers"
    );
    // The reversals split into proposal- and contact-caused ones that sum to them (plan
    // `2026-11-19-crowd-edge-steering` D-5).
    for line in &lines[1..] {
        let cells: Vec<u32> = line
            .split(',')
            .skip(9)
            .take(5)
            .map(str::parse)
            .collect::<Result<_, _>>()?;
        assert_eq!(cells[1], cells[3] + cells[4], "{line}");
    }
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

/// The wasted-shot replay counts only the strikes a release lands on its own tick (plan
/// `2026-11-23-strike-delays` §3): every strike of a volley, one of a spaced burst, and none of a
/// burst whose first strike is delayed. The weapons are copies of shipped instant weapons, found
/// by property, with the delay set.
#[test]
fn a_delayed_instant_weapons_release_replays_no_strike() -> TestResult {
    use sim_fixtures::trace::strikes_on_release;
    let catalogs = sim_fixtures::shipped_catalogs();
    let instant: Vec<sim_types::WeaponSpec> = catalogs
        .units
        .archetypes()
        .flat_map(|arch| [&arch.weapon, &arch.ground_weapon, &arch.air_weapon])
        .flatten()
        .filter(|weapon| weapon.projectile_speed.is_none() && weapon.attacks_count >= 2)
        .cloned()
        .collect();
    let volley = instant
        .iter()
        .find(|weapon| weapon.strike_interval_ticks == 0)
        .ok_or("a shipped instant volley")?;
    let spaced = instant
        .iter()
        .find(|weapon| weapon.strike_interval_ticks > 0)
        .ok_or("a shipped instant spaced burst")?;
    assert_eq!(strikes_on_release(volley), volley.attacks_count);
    assert_eq!(strikes_on_release(spaced), 1);
    for weapon in [volley, spaced] {
        let mut delayed = weapon.clone();
        delayed.first_strike_delay_ticks = 1;
        assert_eq!(strikes_on_release(&delayed), 0, "{}", weapon.name);
    }
    Ok(())
}

/// One unarmed scratch body of A-9 in the catalog's RON form: radius 0.5, the authored `class`.
fn meter_body(slug: &str, id: u16, class: Option<&str>) -> String {
    let class = class.map_or_else(String::new, |name| {
        format!("collision_class: Some(\"{name}\"),")
    });
    format!(
        r#"(
            slug: "{slug}", unit_type_id: {id}, faction: "automaton_directorate",
            display_name: "{slug}", lore: "Scratch archetype of trace_tests.rs.",
            sc2_analogue: "None", showcase_hero: false, showcase_order: None,
            shape: UnitSphere, silhouette: LightProngs, mover_type: Ground,
            attributes: (bits: 1), max_hp: 100.0, max_shields: 0.0, max_energy: 0.0,
            start_energy: 0.0, life_armor: 0.0, shield_armor: 0.0, hp_regen_per_tick: 0.0,
            shield_regen_per_tick: 0.0, energy_regen_per_tick: 0.0, max_speed: 0.15,
            acceleration: 0.15, deceleration: 0.15, turn_rate_deg_per_tick: 180.0,
            collision_radius: 0.5, separation_radius: 0.5, push_priority: 10, sight_radius: 8.0,
            scan_range: 8.0, attack_target_priority: 20, primary_weapon: Either,
            acquire_tie_break: Closest, mineral_cost: 50, gas_cost: 0, supply_cost: 1,
            supply_provided: 0, build_time_ticks: 100, weapon: None,
            {class}
        ),"#
    )
}

/// The figures A-9 reads from one scene: the enemy overlap, its pair-ticks, side 1's D-SHOVE and
/// side 0's D-R10 `free` unit-ticks.
type MeterFigures = (f64, u32, f64, u32);

/// A-9's scene (plan `2026-11-25-collision-classes` A-9): an enemy pair overlapping by 60 % of
/// its radius sum, of classes `pair` (`true` is the authored class `x`, `false` ground), whose
/// player 1 body is moved 0.1 away from the other between two observations (a shove), and a
/// ground melee chaser of player 0 standing on an automatic chase of a far target while another
/// enemy's ring is covered by eight friends of class `blockers`.
fn meter_scene(pair: (bool, bool), blockers: bool) -> Result<MeterFigures, Box<dyn Error>> {
    use sim_core::SimWorld;
    use sim_fixtures::trace::contact_checks::ContactMeter;
    use sim_types::{AttackOrigin, PlayerId, SimScalar, SimVec2, UnitOrder};
    let units = sim_fixtures::UNITS_RON;
    let list_end = units.rfind(']').ok_or("end of the unit list")?;
    let edited = format!(
        "{}{}\n{}\n{}",
        &units[..list_end],
        meter_body("meter_x", 240, Some("x")),
        meter_body("meter_ground", 241, None),
        &units[list_end..]
    );
    let factions = sim_fixtures::shipped_factions();
    let mut catalog =
        UnitCatalog::from_ron_str_bundle(&factions, &edited, sim_fixtures::STRUCTURES_RON)?;
    let upgrades = UpgradeCatalog::from_ron_str(&factions, sim_fixtures::UPGRADES_RON)?;
    let x = catalog.id_by_slug("meter_x").ok_or("x")?;
    let ground = catalog.id_by_slug("meter_ground").ok_or("ground")?;
    let weapon = catalog
        .archetypes()
        .filter_map(|arch| arch.ground_weapon.clone())
        .find(|w| {
            w.melee
                && w.splash.is_none()
                && w.min_range == SimScalar::ZERO
                && w.allowed_movement == sim_types::AllowedMovement::Stopped
        })
        .ok_or("a shipped melee weapon")?;
    let mut chaser_arch = catalog.get(ground)?.clone();
    chaser_arch.unit_type_id = sim_types::UnitTypeId(242);
    chaser_arch.slug = "meter_chaser".into();
    chaser_arch.weapon = Some(weapon.clone());
    chaser_arch.ground_weapon = Some(weapon);
    let chaser_type = chaser_arch.unit_type_id;
    catalog.register(chaser_arch);
    let map = sim_map::GameMap::new(
        64,
        64,
        sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
    )?;
    let mut world = SimWorld::with_catalogs(map, 3, catalog, upgrades)?;
    let at = |x: f64, y: f64| SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y));
    let kind = |authored: bool| if authored { x } else { ground };
    world.spawn_unit(kind(pair.0), PlayerId(0), at(30.0, 40.0))?;
    let shoved = world.spawn_unit(kind(pair.1), PlayerId(1), at(30.4, 40.0))?;
    let chaser = world.spawn_unit(chaser_type, PlayerId(0), at(10.0, 20.0))?;
    let far = world.spawn_unit(ground, PlayerId(1), at(40.0, 20.0))?;
    world.spawn_unit(ground, PlayerId(1), at(14.0, 20.0))?;
    for k in 0..8 {
        let angle = f64::from(k) * std::f64::consts::FRAC_PI_4;
        let (dx, dy) = (1.1 * angle.cos(), 1.1 * angle.sin());
        world.spawn_unit(kind(blockers), PlayerId(0), at(14.0 + dx, 20.0 + dy))?;
    }
    world
        .entities
        .get_mut(&chaser)
        .ok_or("chaser")?
        .order_queue
        .push_back(UnitOrder::AttackTarget {
            target: far,
            origin: AttackOrigin::Acquired,
        });
    let mut meter = ContactMeter::new();
    meter.observe(&world);
    world.entities.get_mut(&shoved).ok_or("shoved")?.position = at(30.5, 40.0);
    meter.observe(&world);
    let checks = meter.total();
    Ok((
        checks.enemy_overlap,
        checks.enemy_overlap_ticks,
        checks.sides[1].shove,
        checks.sides[0].r10[3],
    ))
}

/// A-9 (plan `2026-11-25-collision-classes`, D-3 R-8): the contact meter counts no enemy overlap,
/// no touching (D-SHOVE) and no ring blocker (D-R10) across collision classes, and the same
/// figures within an authored class as within the ground class.
#[test]
fn the_contact_meter_counts_only_pairs_of_one_class() -> TestResult {
    let ground = meter_scene((false, false), false)?;
    let authored = meter_scene((true, true), false)?;
    let across = meter_scene((true, false), true)?;
    assert!(
        ground.0 > 0.55 && ground.1 > 0 && ground.2 > 0.05,
        "{ground:?}"
    );
    assert_eq!(
        ground.3, 0,
        "a ring covered by the chaser's class has no room"
    );
    assert_eq!(
        authored, ground,
        "an authored class counts as the ground class"
    );
    assert_eq!(across.0, 0.0, "no enemy overlap across classes: {across:?}");
    assert_eq!(across.1, 0);
    assert_eq!(across.2, 0.0, "no touching across classes");
    assert!(
        across.3 > 0,
        "bodies of another class leave the ring free: {across:?}"
    );
    Ok(())
}

/// W0-T8 (plan `2026-11-27-gated-movement` `WP0a`): the battle meter's M1 and M1-hi are the
/// harness's: one synthetic track, fed to both, gives the same moving unit-ticks and the same
/// reversals.
#[test]
fn m1_of_the_meter_matches_the_harness_definition() -> TestResult {
    use sim_fixtures::harness::corpus::generate;
    use sim_fixtures::harness::metrics::ScenarioMeter;
    use sim_fixtures::harness::{build_world, Family, RunOptions};
    use sim_fixtures::trace::contact_checks::ContactMeter;
    use sim_types::{SimScalar, SimVec2};
    let scene = generate(Family::Cross, 3, false);
    let (mut world, ids) = build_world(&scene, &RunOptions::default())?;
    let mut harness = ScenarioMeter::new(&world, &scene, &ids);
    let mut battle = ContactMeter::new();
    battle.observe(&world);
    let id = ids[0];
    let start = world.entities[&id].position;
    let (x0, y0) = (f64::from(start.x.to_f32()), f64::from(start.y.to_f32()));
    // Steps of twice the top speed or more (every corpus speed is 0.1 to 0.1875 a tick), a turn
    // back of 0.06 (above a quarter of any corpus speed), a full turn back, a crawl below the
    // moving line, a sideways step and its turn back, and a turn back of 0.024: above an eighth
    // of every corpus speed and below a quarter, so it moves but never reverses.
    let mut x = x0;
    let mut y = y0;
    for (dx, dy) in [
        (0.2, 0.0),
        (0.2, 0.0),
        (-0.06, 0.0),
        (0.2, 0.0),
        (-0.2, 0.0),
        (0.001, 0.0),
        (0.0, 0.2),
        (0.0, -0.2),
        (0.0, 0.024),
    ] {
        x += dx;
        y += dy;
        let entity = world.entities.get_mut(&id).ok_or("unit")?;
        entity.position = SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y));
        world.current_tick += 1;
        harness.observe(&world);
        battle.observe(&world);
    }
    let row = harness.finish();
    let side = battle.movement().sides[0];
    assert!(row.rev_amp > 0 && row.rev_amp_hi > 0, "{row:?}");
    assert_eq!(
        (side.moving_ticks, side.reversals, side.reversals_hi),
        (row.moving_ticks, row.rev_amp, row.rev_amp_hi)
    );
    Ok(())
}

// The route-clearance meter (plan `2026-11-29-wall-clearance` §6.1, WP0). Its scratch world is a
// 32 × 32 map whose south-west quadrant, cells x 0–15 and y 0–15, is solid: a straight wall face
// along y = 16 for x below 16, and one convex corner at (16, 16). Bodies are placed tick by tick
// (`pos_at_tick_start` and `position`), so the meter is read against positions known exactly.

/// The meter's scratch world and the shipped mobile ground archetype it places, with its terrain
/// radius and top speed in world units (per tick).
fn clearance_world() -> Result<(sim_core::SimWorld, sim_types::UnitTypeId, f64, f64), Box<dyn Error>>
{
    use sim_map::{CellFlags, CliffLevel, GameMap, MapCell};
    use sim_types::{MoverType, SimScalar};
    let mut map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    for y in 0..16 {
        for x in 0..16 {
            map.cell_mut(x, y)?.flags = CellFlags::empty();
        }
    }
    let world = sim_fixtures::world(map, 0x00C1_EA2A);
    let arch = world
        .catalog
        .archetypes()
        .find(|a| a.mover_type == MoverType::Ground && a.max_speed > SimScalar::ZERO)
        .ok_or("the shipped catalog has a mobile ground unit")?;
    let (kind, radius, speed) = (
        arch.unit_type_id,
        f64::from(arch.terrain_radius.to_f32()),
        f64::from(arch.max_speed.to_f32()),
    );
    Ok((world, kind, radius, speed))
}

fn fixed(x: f64, y: f64) -> sim_types::SimVec2 {
    use sim_types::{SimScalar, SimVec2};
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

/// Moves `unit` as if it had stepped from `from` to `to` in the tick just played.
fn place(
    world: &mut sim_core::SimWorld,
    unit: EntityId,
    from: (f64, f64),
    to: (f64, f64),
) -> TestResult {
    let entity = world.entities.get_mut(&unit).ok_or("unit")?;
    entity.pos_at_tick_start = fixed(from.0, from.1);
    entity.position = fixed(to.0, to.1);
    Ok(())
}

/// Walks each of `units` along its own track: `track(unit index, tick)` is its position after
/// `tick`. Observes every tick from 1 to `ticks`, and returns the report.
fn walk_tracks(
    world: &mut sim_core::SimWorld,
    units: &[EntityId],
    ticks: u32,
    track: impl Fn(usize, u32) -> (f64, f64),
) -> Result<sim_fixtures::trace::route_clearance::ClearanceReport, Box<dyn Error>> {
    use sim_fixtures::trace::route_clearance::ClearanceMeter;
    let mut meter = ClearanceMeter::new(Vec::new());
    for tick in 1..=ticks {
        for (index, unit) in units.iter().enumerate() {
            place(world, *unit, track(index, tick - 1), track(index, tick))?;
        }
        meter.observe(world, tick);
    }
    Ok(meter.finish())
}

/// §6.1: a unit walking along a wall at a known clearance gives M-1 or M-2 exactly. Three units
/// walk east along the face y = 16 at clearances 0 (tangent), 0.03 (inside the 0.05 margin) and
/// 0.3: every walking tick of the first is M-1 and M-2, of the second M-2 only, of the third
/// neither.
#[test]
fn the_clearance_meter_counts_a_walk_along_a_wall_at_a_known_clearance() -> TestResult {
    use sim_types::PlayerId;
    let (mut world, kind, radius, _) = clearance_world()?;
    let clearances = [0.0, 0.03, 0.3];
    let mut units = Vec::new();
    for (owner, clearance) in (0_u8..).zip(clearances) {
        let at = fixed(3.0, 16.0 + radius + clearance);
        units.push(world.spawn_unit(kind, PlayerId(owner), at)?);
    }
    let report = walk_tracks(&mut world, &units, 20, |index, tick| {
        (
            3.0 + 0.1 * f64::from(tick),
            16.0 + radius + clearances[index],
        )
    })?;
    let figures: Vec<(u32, u32, u32)> = report
        .owners
        .iter()
        .map(|(_, f)| (f.walking, f.touching, f.within_margin))
        .collect();
    assert_eq!(figures, [(20, 20, 20), (20, 0, 20), (20, 0, 0)]);
    assert_eq!(report.whole.episodes, 2);
    assert!(report.deepest.is_none(), "{:?}", report.deepest);
    Ok(())
}

/// §6.1: a body pushed onto a wall and riding it gives one episode of known length and its M-4.
/// It walks east 0.5 clear of the face for five ticks, is pushed onto it (clearance 0) on the
/// sixth, rides it for nine more ticks at 0.04 a tick, and is pushed back off. The episode is ten
/// ticks long; its net displacement runs from where the push started to where the ride ended,
/// and the ticks lost are its length less that distance over the top speed.
#[test]
fn the_clearance_meter_finds_one_episode_and_its_ticks_lost_for_a_body_riding_a_wall() -> TestResult
{
    use sim_fixtures::trace::route_clearance::EpisodeKind;
    use sim_types::PlayerId;
    let (mut world, kind, radius, speed) = clearance_world()?;
    let (open, wall) = (16.0 + radius + 0.5, 16.0 + radius);
    let unit = world.spawn_unit(kind, PlayerId(0), fixed(3.0, open))?;
    let track = |tick: u32| match tick {
        0..=5 => (3.0 + 0.1 * f64::from(tick), open),
        6..=15 => (3.5 + 0.04 * f64::from(tick - 5), wall),
        _ => (3.9 + 0.1 * f64::from(tick - 15), open),
    };
    let report = walk_tracks(&mut world, &[unit], 20, |_, tick| track(tick))?;
    let episodes: Vec<_> = report
        .episodes
        .iter()
        .filter(|e| e.kind == EpisodeKind::Margin)
        .collect();
    assert_eq!(episodes.len(), 1, "{episodes:?}");
    let episode = episodes[0];
    assert_eq!(
        (episode.start_tick, episode.end_tick, episode.length),
        (6, 15, 10)
    );
    let net = (3.9_f64 - 3.5).hypot(open - wall);
    let lost = 10.0 - net / speed;
    assert!(
        (episode.ticks_lost - lost).abs() < 1e-4,
        "{} against {lost}",
        episode.ticks_lost
    );
    assert!((report.whole.ticks_lost - lost).abs() < 1e-4);
    // Nothing proposed a step: collision put the body there on every tick (the probe's class).
    assert_eq!(episode.classes, [0, 10, 0]);
    assert_eq!(report.whole.longest, Some((10, unit.0, 15)));
    Ok(())
}

/// §6.1: a reversal at the wall is counted, and one in open ground is not. Both units step east
/// and then west; one does so 0.01 from the face, the other 2 units from it.
#[test]
fn the_clearance_meter_counts_a_reversal_at_a_wall_and_not_one_in_open_ground() -> TestResult {
    use sim_types::PlayerId;
    let (mut world, kind, radius, _) = clearance_world()?;
    let lanes = [16.0 + radius + 0.01, 16.0 + radius + 2.0];
    let mut units = Vec::new();
    for (owner, y) in (0_u8..).zip(lanes) {
        units.push(world.spawn_unit(kind, PlayerId(owner), fixed(5.0, y))?);
    }
    let report = walk_tracks(&mut world, &units, 3, |index, tick| {
        let x = match tick {
            0 => 5.0,
            1 => 5.1,
            _ => 5.1 - 0.1 * f64::from(tick - 1),
        };
        (x, lanes[index])
    })?;
    let reversals: Vec<(u32, u32)> = report
        .owners
        .iter()
        .map(|(_, f)| (f.reversals, f.proposal_reversals))
        .collect();
    assert_eq!(reversals, [(1, 0), (0, 0)]);
    Ok(())
}

/// §6.1: a route whose first leg cuts a corner is M-5a and M-5b, and one that grazes it at
/// exactly the terrain radius is not. Both units walk from the north-west of the corner (16, 16)
/// along the line `x + y = c` towards a waypoint south-east of it; that line passes the corner at
/// `(c − 32) / √2`. The first unit's line passes 0.21 from it, inside its radius; the second's
/// at exactly its radius.
#[test]
fn the_clearance_meter_finds_a_route_that_cuts_a_corner_and_not_one_that_grazes_it() -> TestResult {
    use sim_fixtures::trace::route_clearance::TaskKind;
    use sim_types::PlayerId;
    let (mut world, kind, radius, _) = clearance_world()?;
    let lines = [32.3, 32.0 + radius * std::f64::consts::SQRT_2];
    let mut units = Vec::new();
    for (owner, c) in (0_u8..).zip(lines) {
        let unit = world.spawn_unit(kind, PlayerId(owner), fixed(14.0, c - 14.0))?;
        world
            .entities
            .get_mut(&unit)
            .ok_or("unit")?
            .current_funnel_waypoints = vec![fixed(c - 14.0, 14.0), fixed(30.0, 14.0)];
        units.push(unit);
    }
    let step = 0.1 / std::f64::consts::SQRT_2;
    let report = walk_tracks(&mut world, &units, 2, |index, tick| {
        let x = 14.0 + step * f64::from(tick);
        (x, lines[index] - x)
    })?;
    assert_eq!(report.routes.updates, 2, "one new route per unit");
    assert_eq!(report.routes.cutting_updates, 1);
    let (sample, update) = report
        .updates
        .iter()
        .find(|(_, update)| update.cutting)
        .ok_or("a cutting update")?;
    assert_eq!(sample.entity, units[0].0);
    let (depth, _) = update.sampled.ok_or("a sampled depth")?;
    let expected = 0.3 / std::f64::consts::SQRT_2 - radius;
    assert!(
        (depth - expected).abs() < 0.02,
        "sampled {depth}, exact {expected}"
    );
    let walk = TaskKind::Walk.index();
    assert_eq!(report.routes.held_cut_ticks[walk], 2);
    assert_eq!(report.routes.held_cut_episodes[walk], 1);
    assert_eq!(
        report.routes.held_cut_longest[walk],
        Some((2, units[0].0, 2))
    );
    Ok(())
}

/// §6.1: the meter leaves the trace's other outputs byte-identical. The showcase is traced for 40
/// ticks with the meter on and off; `units.csv`, `passes.csv`, `overlap.csv`, `summary.md`,
/// `reversals.csv` and `groups.md` are equal, and only the run with the meter writes its four
/// files.
#[test]
fn the_clearance_meter_leaves_the_route_traces_other_outputs_byte_identical() -> TestResult {
    use sim_fixtures::trace::route::{run_route_trace, RouteTraceArgs};
    let root = fresh_dir("route_clearance_identity_test")?;
    let mut outputs = Vec::new();
    for setting in ["on", "off"] {
        let out = root.join(setting);
        let args = RouteTraceArgs::parse(
            [
                "--scenario",
                "scenarios/showcase.ron",
                "--ticks",
                "40",
                "--out",
                out.to_str().ok_or("a UTF-8 path")?,
                "--clearance",
                setting,
            ]
            .map(str::to_owned),
        )?;
        let output = run_route_trace(&args)?;
        assert_eq!(output.clearance.is_some(), setting == "on");
        outputs.push(out);
    }
    for name in [
        "units.csv",
        "passes.csv",
        "overlap.csv",
        "summary.md",
        "reversals.csv",
        "groups.md",
    ] {
        let on = std::fs::read(outputs[0].join(name))?;
        let off = std::fs::read(outputs[1].join(name))?;
        assert!(on == off, "{name} differs with the clearance meter on");
    }
    for name in [
        "clearance.csv",
        "clearance_episodes.csv",
        "clearance_routes.csv",
        "clearance_summary.md",
    ] {
        assert!(outputs[0].join(name).is_file(), "{name} is written");
        assert!(
            !outputs[1].join(name).exists(),
            "{name} is not written with the meter off"
        );
    }
    Ok(())
}
