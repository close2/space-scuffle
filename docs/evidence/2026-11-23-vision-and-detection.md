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

**Golden dumps (§5 step 1).** `~/scratch/vision-detection/probe/dump-wp0/{golden_replay,golden_replay_orders}.dump`:
at each checkpoint the checksum and the RNG word position, every entity (ID, owner, type, position, life, shields,
front order), every projectile (ID, owner, attacker, target, aim point, position, lifetime) and each player's state,
visible and detector counts at the default arena's blocker cells (x 15–17, y 15–17).

## WP1: a lost missile lands on its point (B-54)

- **Red run** on WP0's tree (`~/scratch/vision-detection/red/wp1.log`): `lost_missile_tests` 7 of 9 fail (each cause
  fails with "the missile vanished 4.95 short of its point"; the shipped default is `Fizzle`), the two variant checks
  pass; `an_authored_fizzle_drops_a_lost_missiles_area` fails on "the shipped missile lands on the point and its area
  strikes".
- **Worlds:** `compare.sh world-wp0 world-wp1`: 483 files, 0 differ. As predicted (§7.3): no measured lost missile
  carries a splash, so landing on the point strikes nobody where vanishing struck nobody.
- **Goldens:** the per-checkpoint dumps of WP1 equal WP0's line for line, checksums included, in both fixtures, and
  the sandbox checksum is unchanged. The plan expected a move of the projectile lists (§5); there is none, because
  `golden_replay`'s only lost missile (probe a: a Roach analogue's at tick 224) lands before the next checkpoint, and
  no projectile is in flight at any checkpoint, so the changed default (`target_loss_behavior`, hashed with each
  in-flight projectile's weapon) is never encoded. No re-record.

## WP2: detection has its own radius, latent (B-56)

- **Red run** on WP1's tree (`red/wp2.log`): the new tests do not compile (`VisionSource` has no field
  `detection_radius`; `UnitArchetype` has no field `detection_radius` and no method `is_detector`).
- **Latent:** every test of `sim_map`, `sim_core` and `sim_fixtures` passes unchanged, the goldens, the sandbox
  checksum and `sim_map`'s determinism hashes included.

## WP3: a sight blocker hides its edge (B-57)

- **Red run** on WP2's tree (`red/wp3.log`): `a_blocker_hides_every_cell_from_outside` and
  `a_source_inside_a_blocker_sees_it_and_out` ("a second blocker hides itself": the second blocker's first cell is
  `Visible`) fail, and `blocker_edge_tests::a_unit_in_a_blocker_edge_is_not_acquired_from_outside` fails ("ground
  sight from outside does not reveal the edge cell": the attacker acquires the unit in the edge cell);
  `air_and_towers_see_into_a_blocker` passes before and after.
- **Goldens:** `golden_replay`'s dumps of WP3 (`dump-wp3/`) equal WP0's on every entity and projectile line at
  every checkpoint; they differ only in the vision lines of the blocker cells (from checkpoint 1: player 0's edge
  cells go from `Visible` to `Fogged`, (17, 15) from 15 sources to 0) and so in all 20 checksums. Probe (c) counted
  edge sightings on every tick of this golden and no unit in an edge cell, so this is the move §5 allows: the first
  difference is a blocker cell's state, and no entity differs. Re-recorded under its own header.
  `golden_replay_orders` and the sandbox checksum are unchanged (their maps' rays reach no blocker).
- `sim_map`'s `test_cross_platform_determinism_checksum` hashes the vision states of 1,000 random sources on the
  default arena, so its reference moved (`0x672c257d0fcacc1b` to `0x08753f04b63df2ee`), re-recorded with the reason.
- **Worlds:** WP2 and WP3 each leave every measured world byte-identical to WP0 (`compare-wp2.txt`,
  `compare-wp3.txt`: 483 files, 0 differ). For WP3 this is the prediction of probe (c): the showcase scenarios'
  edge sightings change vision states only, which no measured file records, and no unit stood in an edge cell.
- **Proof** (`proof-wp3/`, a `git archive` copy of `e52c0c83`): both goldens and the sandbox reproduce on native
  debug and release and `wasm32-wasip1` release and debug.

## WP4: attack visibility and reveal state, latent (schema 41)

- **Red run** on WP3's tree (`red/wp4.log`): nothing compiles: `AttackVisibility` is unresolved, and `WeaponSpec`,
  `WeaponSpecDef`, `Entity` and `PlayerView` lack `attack_visibility`, `attack_reveal_ticks`,
  `is_attack_revealed_to` and `attack_revealed`.
- **Latent proof, step 1** (`proof-wp4-latent/`, a copy of the working tree with both fields in the encoding's
  unhashed lists and the schema at 40): both goldens and the sandbox (`5_438_070_395_987_993_470`) reproduce as
  recorded for WP3 on native debug and release and `wasm32-wasip1` release and debug.
- **Step 2:** the fields hashed, schema 41; every checkpoint of both goldens changed and the sandbox is
  `1_550_645_596_559_574_539`. The encoding unit tests' schema byte is 41.
- **Worlds:** `compare-wp4.txt`: 483 files, 0 differ.
- **Proof** (`proof-wp4/`, a copy of `cf846dc3`): the re-recorded goldens and sandbox reproduce on native debug and
  release and `wasm32-wasip1` release and debug.

## WP5: battle 41

- `41_void_ray_above_a_cliff_vs_hydras` on 20 seeds before the reveal (`b41/wp5/`): the Hydra analogues reach the
  foot of the cliff (x 44 to 45) and stop, within the Void Ray analogue's reach and it within theirs. Struck by an
  attacker they cannot see, they flee as helpless out of its reach (`HELPLESS_FLEE_DISTANCE`) and never start a
  swing: 0 Hydra shots on every seed, the Void Ray untouched (150 life, 100 shields), 18 to 38 Void Ray strikes per
  seed. The battle runs to tick 600 with 5 or 6 Hydras left (5 on seeds 3, 7 and 14). The plan predicted a Void
  Ray win; the "never fire" half of P-14's premise holds, the win does not, because the helpless flee takes the
  Hydras out of reach. They stop at the foot within reach, so §7.4's Hold Position fallback is not needed.
- **Worlds:** `compare-wp5.txt`: only `matrix.csv`, `matrix.md` and the matrix log differ, by battle 41's 20 new rows
  (1,200 runs instead of 1,180; still 0 expectation failures on the seeds of record and 4 over all seeds); every
  other row and every other file is byte-identical.

## WP6: an air attacker is revealed to the player it strikes (B-55)

- **Red run** on WP5's tree (`red/wp6.log`): `attack_reveal_tests` fail where they need a shipped air weapon that is
  `Visible` (none is), and so do `attack_visibility_follows_starcraft` (P-11) and
  `an_edited_attack_visibility_hides_an_air_attacker` (P-16); `a_fresh_unit_is_revealed_to_nobody` and
  `a_ground_attacker_above_stays_unseen` (P-12) pass before and after. The client's P-15 tests
  (`battle_fog_tests`) pass on WP5's tree too: WP4 already writes `attack_revealed` from the counters, so a counter
  set by hand is listed, drawn and pickable before any strike sets one (As built).
- **Battle 41** (`b41/wp6/`, P-14): on every seed the Void Ray analogue's first strike lands on tick 30 to 33, the
  Hydra analogues start their first swing 4 or 5 ticks later and release their first shot 7 ticks later; every
  Hydra shot (21 to 23 per seed) falls at most 10 ticks after a Void Ray strike, inside the 54-tick reveal
  window that strike opened, and none before the first strike; no B-185 tail occurs because the Void Ray keeps
  striking until it dies. The Hydras win all 20 seeds with all 6 left, ending on ticks 103 to 108 (before: no shot,
  a stalemate to tick 600). The prediction "the Void Ray's time to win grows or it loses" holds as "it loses".
- **Worlds (P-13):** `compare-wp6.txt`: only the matrix files differ, by battle 41's rows alone. Every row of battles
  26 and 35, the two battles with hidden air strikes but no answerable one (probe b), is byte-identical, as is every
  other world. (The trace binary of this run was built before battle 41's expectation was re-recorded, so its log
  counts battle 41's four seeds of record as expectation failures; the rows themselves are the WP6 rule's, and the
  matrix test checks the re-recorded expectation.)
- **Goldens and sandbox:** unchanged (no air-plane unit in them), as §5 requires.
