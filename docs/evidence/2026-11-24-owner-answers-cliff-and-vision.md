# Evidence: The Owner's Answers on Cliff Levels and Vision (Q0099, Q0101; B-200, B-201, B-184)

Measurements of plan [`2026-11-24-owner-answers-cliff-and-vision`](../plans/2026-11-24-owner-answers-cliff-and-vision.md):
the baseline and probes of WP0 (§7.2), the comparisons of every package against WP0 (§7.3), and the answers that
already held (§3 items 3 and 4). Scratch material (binaries, trees, worlds, the probe patch) is under
`~/scratch/owner-answers/`, never committed.

## WP0: what already holds

**Q0099 3(a): the level is read at the blast's centre point.** `strike_area` (`crates/sim_core/src/combat_resolution.rs`)
reads the level of each search point in `points`, which its caller builds: the target's centre, or with
`offset_by_unit_radius` its edge facing the attacker (`hit.position - direction * hit.radius`); the release position of
an attacker-centred area; the landing point of a missile that lands on a point; each point of a line. A struck unit
is compared with those points, never with the impact unit. Built by `e839bb4c` (splash-cliff-levels WP2). No test
pinned the one case where the two readings differ; WP1 adds `splash_cliff_tests.rs`
`the_level_is_read_at_the_blast_centre` (P-4), which passes on WP0's tree (below).

**Q0101 1(a): air units and Xel'Naga towers see into sight blockers.** `PlayerVisionMap::cast`
(`crates/sim_map/src/vision.rs`) sets `is_air` for `Air` and `CliffWalker` sources, and the blocker test
`!is_air && !in_blocker && target_is_blocker` stops only ground rays; `watchtower_source` gives towers
`MoverType::Air`. Built by `e52c0c83` (vision-and-detection WP3, B-57). Pinned by `vision_tests.rs`
`air_and_towers_see_into_a_blocker` and `blocker_edge_tests.rs` `a_unit_in_a_blocker_edge_is_not_acquired_from_outside`.

## WP0: baseline and probes

**Tree.** `09039527` (the plan on `main` at `20322ca6`, schema 43). The baseline worlds are
`~/scratch/owner-answers/world-wp0/`, measured by `scripts/measure.sh` (§7.1, `route_trace` with `--assets`): the
matrix on its seeds of record and 16 more (1,220 runs; 0 expectation failures on the seeds of record, 4 over all
seeds), battle 31 on 100 seeds, battle 33 on 64, and the showcase and showcase effects scenarios on their default seed
and seeds 1 to 8 for 6,000 ticks.

**The probe.** A scratch patch (`~/scratch/owner-answers/probe/src`, a copy of the WP0 tree with its own git history;
the diff is `probe/probe.diff`) prints one line per event, labelled with the battle and seed (matrix and single-battle
runs), the golden's name, or `-` (one scenario per process):

- **(a) Splash across a ramp or in the air.** In `strike_area`, for an area kept to its level, every unit in reach and
  passing the area's filters whose nearest same-level search point changes under D-1 and D-2 (tagged `air` when the
  area's plane or the unit is `Air`, else `ramp`), or that stays excluded (`cliff`); in
  `strike_target_on_other_level`, every fallback strike whose target's span overlaps the burst centre's (`fallback`).
- **(b) Detection beyond the rays.** Every cast of a detecting source, with the number of cells of its detection disc
  that its rays did not reach; and at every vision update, every concealed enemy within a detector's disc whose cell
  the detector's player does not detect.
- **(c) Revealed, detected, unseen.** Every weapon-filter (`is_entity_targetable`, `StrikeContext::is_available`),
  retaliation (`is_attacker_visible_to`) and view (`PlayerView::revealed`) check on a concealed enemy attack-revealed
  to the checking player, in a cell it detects and does not see.
- **(d) Detectors under construction.** Every vision update on which a detector foundation exists, and each concealed
  enemy within its detection radius.

**Counts.** Every kind is zero in every world: the matrix (all battles, seeds of record and 16 more), battle 31 (100
seeds), battle 33 (64 seeds), the showcase and showcase effects scenarios (default seed and seeds 1 to 8, 6,000
ticks), and both goldens (`golden_replay`, `golden_replay_orders`). The probed worlds are byte-identical to the
baseline, logs included (`probe/compare-to-wp0.txt`: 483 files, 0 differing), so no probe line was printed anywhere.
No first event exists for any kind.

| World | (a) ramp | (a) air | (a) fallback | (a) cliff | (b) casts | (b) enemies | (c) | (d) |
|---|---|---|---|---|---|---|---|---|
| Matrix, all battles and seeds | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| Battle 31, 100 seeds | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| Battle 33, 64 seeds | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| Showcase, default and seeds 1–8 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| Showcase effects, default and seeds 1–8 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| Goldens | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

**Controls.** The hooks fire where the constructed tests set the case up: on the probe tree, `splash_cliff_tests.rs`
prints `a ramp` (the old lower-landing test, unit on the ramp's top cell, old distance none, new 0.75), `a air` (the
old air-area test) and `a cliff` (every unit still spared across a cliff), and a detector test of
`attack_reveal_tests.rs`, `weapon_validation_tests.rs` or `blocker_edge_tests.rs` prints `b cast`. (c) and (d) need a
detecting source: zero `b cast` lines in every world means no detecting source was ever cast there, so (c) and (d)
are zero by construction.

**What each package may move.** With every count zero (§7.2's rule), WP1 to WP5 are byte-identical in every measured
world and golden; the answers are proved by the constructed tests (P-1 to P-13). Any difference is a defect of the
package (§8 rule 1).

**P-4 on WP0's tree.** `the_level_is_read_at_the_blast_centre`, run on WP0's tree through a scratch copy of the WP1
test file that reads spans from the terrain, passes: Q0099 3(a) held before this plan.

## WP1 to WP5: the worlds

Each package's trace binaries are built from its commit (`scripts/wpbuild.sh`) and measured with its own assets
(`scripts/measure.sh`), then compared file by file with WP0's worlds (`scripts/compare.sh`). Every output file is
byte-identical in every package, as the probes predicted (§7.2's rule); no package moved a world, so no first
difference exists to trace and P-6's superset check has no moved world to apply to.

| Package | Commit | Rule | Files compared | Differing |
|---|---|---|---|---|
| WP1 | `c0094e7e` | A ramp spans both landings (Q0099 1(c)) | 483 | 0 |
| WP2 | `b74aaa10` | Air blasts and air units ignore levels (Q0099 2(b)) | 483 | 0 |
| WP3 | `80ea859f` | Detection is a circle (Q0101 2(b)) | 483 | 0 |
| WP4 | `b0341876` | An attack reveal counts as sight of a detected concealed attacker (D-4) | 483 | 0 |
| WP5 | `a3e97544` | Detectors under construction do not detect (Q0101 3(b)) | 483 | 0 |

The matrix keeps 0 expectation failures on its seeds of record and 4 over all seeds in every package, as at WP0.

**Red runs** (each new test failing on the previous package's tree, logs under `~/scratch/owner-answers/red/`):

- WP1: `sim_map` does not compile against the span tests (`CliffSpan`, `cliff_span_at` unresolved). Run through a
  scratch copy of `splash_cliff_tests.rs` whose `cliff_span_at` reads the span from the terrain,
  `a_same_level_area_counts_a_ramp_as_both_landings` (the ramp's top cell lost 0, expected 10),
  `a_shot_centred_on_a_ramp_strikes_both_landings_and_spares_a_third_level` (the upper landing lost 0) and
  `a_burst_on_a_ramp_strikes_its_target_at_the_top_by_its_area` (the target took the fallback's 10, not the area's 5)
  fail; `the_level_is_read_at_the_blast_centre` (P-4) and `beside_a_cliff_away_from_the_ramp_the_levels_stay_apart`
  pass.
- WP2: `an_air_area_ignores_levels` (the air unit over the low ground lost 0) and
  `an_air_unit_in_a_ground_centred_area_ignores_levels` (the air unit over the low ground lost 0) fail.
- WP3: `detection_is_a_plain_circle` and `equal_radii_detect_exactly_the_seen_cells` (detection is not the disc at
  (16, 24) r 11) fail; `on_open_ground_the_rays_cover_the_detection_disc` (P-9) passes before the change, so the rays
  cover the disc on open ground for every whole radius 1 to 32, and air detectors on open ground detect nothing new
  (§8 rule 8 does not arise). In `detection_circle_tests.rs` the three tests that need the circle fail (the cloaked
  unit's cell is not detected) and the no-detection control passes.
- WP4: `a_detected_cloaked_attacker_is_answered_during_its_reveal` ("listed as revealed during the reveal") and
  `revealed_lists_an_attack_revealed_detected_attacker` ("listed during the reveal", after its premise that the
  cell is detected and not seen held) fail.
- WP5: `a_detector_foundation_detects_nothing`, `a_detector_detects_from_its_completion` and
  `a_completed_detector_recasts_to_a_full_recount` fail with the foundation detecting ((true, true), expected
  (false, true)).

**Timing of detection at completion (P-12).** Construction advances in the production phase, which runs before the
vision phase in the same tick, so a detector detects from the vision update of the tick its construction completes
(`a_detector_detects_from_its_completion`).

## WP4: a revealed, detected cloaked attacker is a target (D-4), and its grounding

D-4 goes beyond the owner's literal answers, so its grounding is recorded here. It composes two rules already in
force, each grounded on its own:

1. **The attack reveal is sight.** 5.0.14's `AttackReveal` behaviour (`core` BehaviorData:237–240), applied by the
   `Visibility Visible` of every air unit's and the Colossus's damage effect (`liberty` EffectData:2421, :1715, …),
   makes the attacker visible to the struck player for 3 Normal s; built by plan `2026-11-23-vision-and-detection`
   D-5 (B-55), where a revealed, non-concealed attacker is a target. That plan's §2.2 rule 3 adds that a reveal
   "grants visibility, not detection: a cloaked attacker stays cloaked" (`a_cloaked_attacker_stays_hidden_without_detection`,
   which passes unchanged after WP4).
2. **Detection is a circle and needs vision from elsewhere** (the owner's answer Q0101 2(b): "within its detection
   radius even up cliffs or past sight blockers whenever another friendly unit grants vision there"), with the
   weapon filter `TargetFilters Visible` (`core` WeaponData:29) asking for vision in any case.

A concealed attacker that the reveal makes visible to player p and that stands in a cell p detects therefore meets
both halves: it is visible (by the reveal) and detected (by the circle). Until WP4 the concealment test asked for a
`Visible` cell specifically, so the reveal counted as sight for a visible attacker and not for a concealed one; D-4
removes that asymmetry. The common game case is the one Liquipedia, *High Ground and Low Ground*, describes for air
units over high ground, "revealed if they attack" (vision plan S16): a Missile Turret below a cliff fires at a cloaked
Banshee attacking from above, within the turret's detection, with no other unit seeing the plateau. Without D-4, the
owner's answer 2(b) would leave that turret unable to fire. The rule is one parameter on one predicate
(`SimWorld::is_cloak_revealed`), in its own commit, and reverting that commit restores the old reading.

Before WP4 no shipped detector could reach the case (§3 item 6: detection equalled sight, along the same rays), and
probe (c) counted zero such checks in every world.

## WP6: the final tree

**Merge.** `main` at `6291223b` (question answers and closures, docs only) merged as `e3843faa`; nothing to re-run
for it.

**Client skirmish.** `skirmish_match_tests` in release on `radv` (`scripts/client.sh`), on WP0's tree (`064642ec`)
and on the final tree (`e3843faa`): 7 passed, 1 ignored, both times. The two stage logs (`client-wp0/stages.txt`,
`client-final/stages.txt`, 42 lines each) hold the same lines; only their order differs, as the lines of tests
running in parallel threads interleave differently (sorted, they are identical). No line names a detector.

**Slow targets the plan names.** `golden_replay_tests` (release: both goldens reproduce every checkpoint unchanged,
schema 43), `full_match_tests`, `damage_and_shields_tests`, `vision_tests`, `ramp_vision_tests` and every
`sim_fixtures` target (the sandbox) pass. `cargo gate quick` is green on the merged tree, `workspace_checks`
included.

**Merge of hidden-harvesters.** `main` at `d662274f` (plan `2026-11-24-hidden-harvesters`, schema 44) merged as
`d02ae75a`. The merged tree's worlds, measured as in §7.1, are byte-identical to `main`'s own (`world-main` against
`world-final`: 483 files, 0 differing; the matrix keeps 0 expectation failures on its seeds of record and 4 over all
seeds), so this branch moves nothing on top of `main`. On the merged tree `cargo gate quick` is green, workspace
clippy is clean, and the goldens (release, schema 44), `full_match_tests`, `damage_and_shields_tests`,
`detection_circle_tests`, `splash_cliff_tests`, `attack_reveal_tests` and every `sim_map` and `sim_fixtures` target
pass.
