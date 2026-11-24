# Evidence: Vision and Detection (B-54 to B-57)

Measurements of plan [`2026-11-23-vision-and-detection`](../plans/2026-11-23-vision-and-detection.md): the baseline
and probes of WP0 (§7.2), the comparisons of every package against WP0 (§7.3), the golden dumps (§5) and the traced
moves. Scratch material (binaries, trees, worlds, the probe patch and the dump test) is under
`~/scratch/vision-detection/`, never committed.

## WP0: baseline and probes

**Tree.** `393cf1fc` (the plan, Q0101, and `main` at `36b914fd` merged in: the turret wind-up, schema 40). The
baseline worlds are `~/scratch/vision-detection/world-wp0/`, measured by `scripts/measure.sh` (§7.1): the matrix on
its seeds of record and 16 more (1,180 runs; 0 expectation failures on the seeds of record, 4 over all seeds), battle
31 on 100 seeds, battle 33 on 64, and the showcase and showcase effects scenarios on their default seed and seeds 1
to 8 for 6,000 ticks.

**The probe.** A scratch patch (`~/scratch/vision-detection/probe/src`, a copy of the WP0 tree with its own git
history for the diff) prints one line per event, labelled with the battle and seed (the matrix and single-battle
runs) or `-` (one scenario per process). The probed worlds are byte-identical to the baseline: every output file is
equal, and every log differs only by the probe's lines. The goldens are played by a scratch test
(`~/scratch/vision-detection/dump/golden_dump.rs`, copied into `crates/sim_core/tests/` only for the run), which
also writes the per-checkpoint dumps of §5 step 1 (`probe/dump-wp0/`).

- **(a) Lost missiles:** a `track_projectile_target` call that leaves the tracked branch, with its cause.
- **(b) Hidden air strikes:** a strike of a weapon of an `Air` or `CliffWalker` archetype whose attacker's cell is
  not `Visible` to the struck unit's owner. It is **tagged** when, at the strike or on any of the 53 following ticks
  (the reveal window of D-5), a unit of the struck side whose weapon can hit the attacker's plane and target flags
  has the attacker within its scan extent (scan range or weapon range, plus both radii) while the attacker's cell is
  still not `Visible` to that side.
- **(c) Blocker edges:** every tick at which some ground source reaches a blocker cell from outside, through the
  edge branch of `cast` (`c-sight`, with the number of (player, cell) pairs so reached); and every tick at which a ground unit stands
  in a blocker cell that an enemy player's ground source reaches through that branch (`c-unit`).

| World | (a) lost | (a) with splash | (b) hidden air strikes | (b) tagged | (c) edge sightings (ticks) | (c) unit in an edge |
|---|---|---|---|---|---|---|
| Matrix, all battles and seeds | 458 | 0 | 732 | 0 | 0 | 0 |
| Battle 31, 100 seeds | 837 | 0 | 0 | 0 | 0 | 0 |
| Battle 33, 64 seeds | 0 | 0 | 0 | 0 | 0 | 0 |
| Showcase, default and seeds 1–8 | 8 (1, 1, 2, 1, 0, 1, 1, 0, 1) | 0 | 0 | 0 | 418 to 460 per seed | 0 |
| Showcase effects, default and seeds 1–8 | 0 | 0 | 0 | 0 | 281 per seed | 0 |
| `golden_replay` | 1 | 0 | 0 | 0 | 1,000 (every tick) | 0 |
| `golden_replay_orders` | 0 | 0 | 0 | 0 | 0 | 0 |

- **(a)** Every lost missile's cause is `dead` (1,304 events); none is loaded, invulnerable or concealed, and none
  carries a splash. In the matrix they fall in battles 29 (42), 31 (133), 34 (111), 35 (131), 36a (12), 36b (11),
  39a (17) and 39b (1); the first is battle 35 seed 11 tick 45 (a `vector_interceptor` missile at a unit that died).
  In the showcase every one is a Roach analogue's missile (`rhino_beetle`), the first at tick 349 or 350. The
  showcase effects scenario has none: the plan's expectation of Roach missiles lost to the Lurker analogue's burrow
  did not occur. In `golden_replay` the only one is a Roach analogue's missile at tick 224 whose target died.
- **(b)** Only battles 26 (Colossus analogues striking Zergling analogues, 165 strikes) and 35 (Viking analogues
  striking Colossus analogues, 567) have hidden air strikes, every seed of both; none is tagged. In battle 35 the
  struck Colossus analogues cannot hit air; in battle 26 no Zergling analogue reaches a Colossus analogue that is
  still hidden within a reveal window. No measured world is a B-55 world, so **battle 41 is the only B-55 world**
  (§7.2 rule): WP5 adds it, and P-13 is judged on battles 26 and 35 as the worlds with counted (untagged) events.
- **(c)** No unit ever stands in a blocker edge cell that an enemy sees through the edge branch, in any world. Edge
  cells are reached in every showcase and showcase effects seed (from tick 107 in the effects scenario, tick 227 in
  the showcase) and on every tick of `golden_replay` (6 cells of the default arena's blocker from tick 1); the
  matrix (flat arenas) and `golden_replay_orders` reach none. So WP3 is expected to change vision states at blocker
  cells in those worlds and, by §8 rule 3 and P-9, no entity state anywhere: the measured traces byte-identical, and
  `golden_replay` re-recorded for the blocker cells' states alone.
