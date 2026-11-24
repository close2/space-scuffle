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
