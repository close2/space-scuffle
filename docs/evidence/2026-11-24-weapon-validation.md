# Evidence: Weapon Validation (B-182, B-185 weapon half, B-194)

Measurements of plan [`2026-11-24-weapon-validation`](../plans/2026-11-24-weapon-validation.md): the baseline and
probes of WP0 (§7.2), the comparisons of every package against WP0 (§7.3), the golden proofs (§5) and the traced
moves. Scratch material (binaries, trees, worlds, the probe patch and its summariser, the dump test) is under
`~/scratch/weapon-validation/`, never committed.

## WP0: baseline and probes

**Tree.** `8f2c0542` (the plan and Q0106 on `main` at `cff46b37`, schema 41). The baseline worlds are
`~/scratch/weapon-validation/world-wp0/`, measured by `scripts/measure.sh` (§7.1, `route_trace` with `--assets`): the
matrix on its seeds of record and 16 more (1,200 runs; 0 expectation failures on the seeds of record, 4 over all
seeds), battle 31 on 100 seeds, battle 33 on 64, and the showcase and showcase effects scenarios on their default seed
and seeds 1 to 8 for 6,000 ticks. The client skirmish's stage log is `client-wp0/stages.txt` (`scripts/client.sh`).

**The probe.** A scratch patch (`~/scratch/weapon-validation/probe/src`, a copy of the WP0 tree with its own git
history; the diff is `probe/probe.diff`) prints one line per event, labelled with the battle and seed (the matrix and
single-battle runs) or `-` (one scenario per process). The probed worlds are byte-identical to the baseline: every
output file is equal, and every log differs only by the probe's lines. The goldens are played by a scratch test
(`golden_dump.rs`, a copy of `golden_replay_tests.rs` with a dump observer), which also writes per-checkpoint dumps
(`probe/dump-wp0/`). `scripts/probesum.py` counts the events.

- **(a) Unseen strikes:** a weapon step whose admitted target (`StrikeContext::is_available`) fails `sees_enemy`
  for the attacker's player (`step`, tagged with the attacker's archetype, whether it is sieged or below the
  target's level, and the centre distance against its sight), a pending strike so admitted (`pending`), and an
  `apply_weapon_hit` that lands on such a target (`hit`; instant and projectile).
- **(b) Unseen missiles:** a `track_projectile_target` call that follows a target failing `sees_enemy`, tagged with
  whether the weapon is the Brood Lord analogue's and whether it has a splash.
- **(c) Kept unseen targets:** a `decide_acquisition` call whose current target exists, passes
  `is_entity_targetable` and fails `sees_enemy`, by engagement (`Acquired`, `Explicit` mobile or immobile, `Hold`,
  `Track`) and decision (`engage` means the scan found a candidate).
- **(d) Teleports:** a `BlinkTeleport` commit, with the number of projectiles homing on the unit.
- **(e) Brood Lord missiles** whose target is concealed and undetected in flight.

| World | (a) step | (a) pending | (a) hit | (b) | (c) | (d) | (e) |
|---|---|---|---|---|---|---|---|
| Matrix, all battles and seeds | 19,681 | 183 | 384 | 2,609 | 19,507 | 0 | 0 |
| Battle 31, 100 seeds | 0 | 0 | 14 | 41 | 0 | 0 | 0 |
| Battle 33, 64 seeds | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| Showcase, default and seeds 1–8 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| Showcase effects, default and seeds 1–8 | 194 (20, 20, 18, 49, 12, 20, 18, 24, 13) | 0 | 7 | 0 | 209 (22, 22, 20, 51, 13, 22, 20, 25, 14) | 0 | 0 |
| `golden_replay` | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `golden_replay_orders` | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

- **Matrix, by battle.** Only three battles count any event:
  - **35** (Viking analogues against Colossus analogues), every one of its 20 seeds: (a) 19,676 steps, 183 pending
    strikes, 380 hits; (b) 2,598 missile ticks, none with a splash; (c) 19,499 kept targets, all `Acquired` (34 with a
    candidate to take instead, 19,465 with none; 18,133 of them out of reach, which pursuit chases today). Every
    attacker is a `vector_interceptor`; 19,658 of the steps have the target's centre beyond its sight of 10. The
    first event is seed 3203386110 tick 195, a missile at a Colossus analogue whose cell its player does not see.
  - **37a** (Abduct on a sieged tank), seeds 7, 11 and 14: 5 steps and 8 `Acquired` kept targets of the sieged
    `railgun_crawler` beyond its sight of 11 (first: seed 11 tick 205, distance 14.24).
  - **31** (Marauder analogues against Stalker analogues), seeds 1, 7, 14 and 17: 4 hits and 11 missile ticks of a
    `warp_strider` missile whose target its player stopped seeing in flight (first: seed 7 tick 523).
  - No other battle counts any event: the sieged tanks of battles 3, 37b, 39a, 39b and 40 never strike beyond their
    player's vision, nor do the Vikings of 12c and 19 (the plan's §7.2 expected some).
- **Battle 31 on 100 seeds:** 14 seeds (1, 7, 14, 17, 21, 31, 42, 44, 55, 61, 65, 72, 75, 80), all `warp_strider`
  missiles, 14 hits and 41 missile ticks.
- **Showcase effects:** every seed, from tick 703 to 718: the sieged `railgun_crawler` keeps an acquired target
  beyond its sight and shells it (`Acquired`, no candidate). The showcase and the goldens count nothing.
- **(c) by engagement:** every kept unseen target in every world is `Acquired`; none is `Explicit` (mobile or
  immobile), `Hold` or `Track`. So B-195's exposure in the measured worlds is zero.
- **(d), (e):** zero everywhere, as expected (no shipped Blink; no cloak against the Brood Lord analogue).

**Expected moves.** WP1 to WP4 move nothing (§8 rules 1 and 2). WP5 is expected to move exactly: matrix battle 35
(every seed), battle 37a seeds 7, 11 and 14, battle 31 seeds 1, 7, 14 and 17 (and the same 14 seeds of the
100-seed battle 31 run), every showcase effects seed, and the new battle 42. The showcase, battle 33, the goldens and
the sandbox are expected byte-identical.

## WP1: missile validation and teleport state, latent (schema 43)

**Red run.** The new tests did not compile on WP0's tree: `MissileValidation`, `WeaponSpec::missile_validation`,
`Entity::teleport_count`, `Entity::teleport_to` and `ActiveProjectile::target_teleport_count` did not exist
(`sim_types` unit test, `checksum_state_tests`, `weapon_validation_tests`, the `entity.rs` unit tests); and
`an_instant_weapon_authoring_missile_validation_is_refused` failed, the edited catalog loading with `Ok(())` (serde
ignored the unknown field).

**Worlds.** Every output of §7.1 is byte-identical to WP0 (`compare-wp1.txt`: 483 files, 0 differing), as §8 rule 1
requires.

## WP2: a missile drops a target that teleports (B-182)

**Red run** (on WP1's tree): `a_missile_at_a_blinker_lands_where_it_stood` ("tick 5: the missile still has a
target"), `a_splash_missile_at_a_blinker_strikes_the_old_point` (the bystander beside the old point was not struck)
and `runtime_catalog_tests::an_authored_teleport_validation_off_follows_the_blink` (the shipped missile struck the
blinker) failed; `a_missile_launched_after_the_blink_strikes` passed, as it should before and after.

**Worlds.** Every output of §7.1 is byte-identical to WP0 (`compare-wp2.txt`: 483 files, 0 differing), as probe (d)
predicted (no shipped Blink); both goldens and the sandbox pass unchanged on native debug.

## WP3: the Brood Lord's missiles validate at impact (B-194)

**Red run** (on WP2's tree, with WP3's tests and data): `a_missile_that_skips_the_weapon_check_follows_a_cloaking_target`
and `…_strikes_only_if_the_target_passes_at_impact` failed ("tick 5: the missile keeps its cloaked target": the missile
dropped it); `weapon_catalog_rules_tests::missile_validation_follows_starcraft` failed before the data rows (the
`spore_matriarch` weapons validated the weapon); `runtime_catalog_tests::an_authored_weapon_validation_off_follows_a_cloaking_target`
failed ("the authored flag keeps homing on it"). `…_lands_on_the_point_of_a_target_that_died` passed before and after,
as it should (a dead target is not followed either way).

**Worlds.** Every output of §7.1 is byte-identical to WP0 (`compare-wp3.txt`: 483 files, 0 differing), as probe (e)
predicted (no cloak against the Brood Lord analogue); the goldens and the sandbox pass unchanged on native debug.

## WP4: battle 42

**Positions (a deviation from §7.4's coordinates, by its own rule).** The plan's positions (tank at (50.0, 32.0),
workers at x = 41.5) do not satisfy its rule, because sight is whole cells: a cell is seen when its offset from the
viewer's cell is within the radius. The tank at x = 50.0 stands in cell 49 or 50 under the spawn jitter (±0.12), and
a worker in cell 41 at dy 0 sees cell 49 (offset 8, within a worker's 8): on the first try the struck workers
answered and killed the tank on 9 of 20 seeds. The battle now places the tank at a cell centre, (50.5, 32.5), and the
column at x = 41.85 (cell 41 under the jitter), rows 30.25 to 34.75: every worker is 9 cells from the tank (beyond its
8, inside the tank's 11), 8.56 to 9.10 world units away. §7.4's verify, on all 20 seeds (`scripts/b42check.py`):
no worker ever swings; every worker is struck, flees once, and stops 13.55 to 14.09 from the tank, outside its
sight and inside its reach of 14.25.

**Recorded expectation** (B-185 open): the first shell strikes three workers, which flee out of the tank's sight;
the tank takes the fourth, which flees too, and shells it beyond its sight until it dies (its last tick 180 to
246). On all 20 seeds 3 workers and the tank are left at tick 900: `expect: (blue: (3, 3), red: (1, 1))`.

**Worlds.** Every output of §7.1 is byte-identical to WP0 except the matrix's summary files, which gain battle 42's
rows (`matrix.csv` equal to WP0's with battle 42's rows removed; 1,220 runs, 0 expectation failures on the seeds of
record, 4 over all seeds, as on WP0).

## WP5: weapons strike only what their player sees (B-185 weapon half)

**Red run** (on WP4's tree, the final test file): 12 of the 23 `weapon_validation_tests` failed, each WP5 test but
the two guards that hold before and after (`an_acquired_target_lost_to_fog_gives_way_to_a_seen_one`, since the old
rule already traded an unseen current target for a seen candidate, and `a_seen_target_out_of_reach_is_still_chased`):
`no_swing_starts_on_an_unseen_target`, `a_spotter_lets_the_strike_land`, `an_attack_revealed_target_is_struck` ("tick 1:
the shooter swings"), `a_pending_strike_holds_on_an_unseen_target` (the second strike landed),
`a_missile_whose_target_walks_into_fog_lands_on_the_point` (the missile kept its target),
`a_missile_that_skips_the_weapon_check_follows_a_target_into_fog`, `…_with_none_the_attack_move_resumes`,
`a_hold_target_lost_to_fog_is_released`, `an_immobile_units_order_on_a_lost_target_ends`,
`a_players_order_is_kept_but_holds_fire`, `the_reveals_end_ends_the_answer` (the Hydra analogue kept firing after the
reveal), `a_sieged_tank_needs_a_spotter_beyond_its_sight` (the tank shelled the unseen target).

**Deviation (P-6's last clause).** "A player without a vision map fires as today" cannot be reached through the
public API: every player slot has a vision map and a unit of an owner outside the slots cannot be spawned
(`InvalidPlayer`). It is pinned by the unit test
`target_acquisition::weapon_filter_tests::a_player_without_a_vision_map_sees_every_enemy` instead.

**Re-derived tests** (they struck targets their player did not see): the `combat_resolution` unit test
`a_projectile_and_an_effect_missile_impact_on_the_same_step`, `target_selection_tests::swings_under` (both b90
tests), `transport_ownership_tests` (`prepared_mutual_lethal_shots_…`, and `loaded_ground_and_air_targets_…`, whose
target now stands at 2 inside the fixture's sight of 3) and `weapon_period_tests::battle` drive the combat phase
directly and now run the vision phase first, as `step` does; `attack_reveal_tests::a_dead_attacker_reveals_nothing`
and `combat_retaliation_and_projectile_tests::test_missile_lifetime_timeout_expiration` gain an unarmed or holding,
invulnerable spotter of the shooter's player beside the target.

**Worlds** (`compare-wp5.txt` against WP0; WP4's tree for the first differences):

| World | Moved | Counted by WP0's probe | First difference |
|---|---|---|---|
| Matrix battle 35 | every seed (20) | every seed | at or after the seed's first counted event (a strike or missile on an unseen Colossus analogue: the Colossus's life differs that tick; or a Viking analogue's acquired target dropped one tick after it is lost to fog) |
| Matrix battle 42 | every seed | (new) | the tank's acquired attack dropped, ticks 150 to 214 |
| Matrix, every other battle (37a and 31 included) | no outcome moved | 37a seeds 7, 11, 14; 31 seeds 1, 7, 14, 17 | the matrix records outcomes only |
| Battle 31, 100 seeds | exactly the 14 counted seeds; no outcome changed | 14 seeds | 0 to 4 ticks after each seed's first counted missile event |
| Showcase effects | default and seeds 1, 2, 3, 4, 5, 7 (positions only; 4 and 5 also the reversal summary) | all 9 | ticks 711 to 756, after each first counted event (703 to 711); seeds 6 and 8 counted but did not move |
| Showcase, battle 33 | none | none | — |
| Goldens, sandbox | none (native debug) | none | — |

Every moved world was counted and every first difference is at or after a counted event (P-11).

**P-12 (battle 42).** As predicted: the tank fires at no worker beyond its sight and all 4 workers survive on all 20
seeds (3 before). The expectation is re-recorded `(blue: (4, 4), red: (1, 1))`.

**Broken matrix claim (stop rule 3, reported, not rewritten): battle 35.** "Vikings lose nothing; a helpless colossus
may flee out of their scan range", `expect: (blue: (12, 12), red: (0, 1))`. Vikings still lose nothing, but on 9 of
20 seeds (22 and 33 of record; 3, 6, 7, 8, 9, 10, 14) 2 or 3 Colossus analogues are left at tick 900 (WP4: 0 or 1,
battles ending on ticks 320 to 530 unless one fled). Cause, traced: a Colossus analogue that flees out of the Viking
analogues' sight (10; the Vikings hit at full reach on centres beyond it, 19,658 of WP0's steps) is lost to fog; the
Vikings' acquired attacks are dropped (D-5, Q0106 part 2's default) and they resume their attack-move, so a fleeing
Colossus is no longer chased by its true position. The expectation is left as recorded; the matrix test
`every_battle_keeps_its_recorded_outcome_on_every_seed` fails on battle 35 seeds 22 and 33 until the orchestrator
decides (re-record with the trace, or revisit Q0106 part 2 or part 4: a Colossus's body inside sight while its centre
is not).

## WP6: merge of `main`, golden proofs and the final tree

**Merge.** `main` at `e313268f` (the attack orders, B-186 and B-187, no schema change) merged as `04341b82`. Its
worlds are byte-identical to WP0's (`world-main` against `world-wp0`: 483 files, 0 differing), and the merged tree
against `main` moves exactly WP5's set of files (`compare-merged-vs-main.txt` equals `compare-wp5.txt`); the merged
worlds equal WP5's but for battle 42's re-recorded expectation (matrix: 2 expectation failures on the seeds of
record, battle 35 seeds 22 and 33; 13 over all seeds, WP0's 4 and battle 35's 9).

**Golden proofs** (`scripts/proofcopy.sh`, a git-archive copy of the tree in its own target directory; each copy's
files are stamped with the present, since a shared target directory reused stale builds of an earlier copy, which
voided this branch's first proof runs):
1. *Latent step* (tree `05bc3b6f`: WP1's code with the three fields left out of the encoding, schema 41): both
   goldens and the sandbox reproduce the lists recorded for the vision-and-detection plan on native debug and
   release and `wasm32-wasip1` release and debug (`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<tree>"`).
2. *Final tree* (`04341b82`, schema 43): both goldens and the sandbox reproduce WP1's re-recorded values on the same
   four builds. WP2 to WP5 changed no golden (P-11: probes (a) to (c) counted nothing in either golden).

**Client.** The skirmish stage log (`skirmish_match_tests`, release, radv) on the merged tree holds the same lines
as WP0's; only the interleaving of two parallel tests' `tick 0: Mine` lines differs.

## Amendment 1: battle 35 restated

By the orchestrator's decision (plan Amendment 1), battle 35's claim reads "Vikings lose nothing; colossi that flee
out of their sight survive (5.0.14 TargetFilters Visible)" and its expectation is the range measured on the merged
tree's 20 seeds, `red (1, 3)` (per seed: 3203386110 1, 11 1, 22 2, 33 3, 1 1, 2 1, 3 2, 4 1, 5 1, 6 2, 7 2, 8 2, 9 2,
10 3, 12 1, 13 1, 14 3, 15 1, 16 1, 17 1; Vikings 12 on every seed). The trace is WP5's above.

## WP7: the chase to the last seen point (B-195, Amendment 2)

Scripts and logs under `~/scratch/weapon-wp7/` (not committed): `scripts/` (the measure, compare and proof scripts of
§7.1, adapted), `probe/` (the probe patch on a copy of `ffc6f298` and its outputs), `red/`, `world-*`, `proof-*`.

**Probe (on `main` `ffc6f298`).** A scratch patch in acquisition printed, for every unit under a player's
(`Explicit`) attack, the first tick its player saw the target under the order, and every visit on which the target
was targetable but unseen (mobile or immobile, with whether it had been seen before). It played every world of §7.1
and both goldens. Result: player's attack orders exist only in matrix battles 12c and 20 (one per seed, the Viking and
the Brood Lord analogues, all 20 seeds), each seen on tick 1 and never lost; **no world counted an unseen event**,
mobile or immobile, seen before or not. Battles 31 and 33, the showcase and showcase-effects scenarios (default and
seeds 1 to 8) and both goldens give no player's attack order at all. So WP7 was predicted to move no measured world.

**Red runs** (`red/`). (1) On `main` with Amendment 2 only, the new tests do not compile: `LastSeen` and
`MovementMemory::last_seen` do not exist (`red1-compile.log`). (2) With the one test that needs the type removed,
`a_players_order_on_a_lost_target_walks_to_where_it_was_last_seen_and_ends` and
`a_lost_target_seen_again_resumes_the_attack` fail on the solver pipeline: the shooter leaves its row after the
target (tick 7, `(10.77, 25.07)`; after 10 ticks `(10.87, 25.30)`), i.e. it chases the true position
(`red2-behaviour.log`).

**Re-derived test.** WP5's `a_players_order_is_kept_but_holds_fire` claimed B-195's interim behaviour and is replaced
by the walk test. The missile-into-fog bench (`a_missile_whose_target_walks_into_fog_lands_on_the_point`,
`a_missile_that_skips_the_weapon_check_follows_a_target_into_fog`) now puts its shooter on hold after the launch:
under its attack order the shooter walked to the last seen point, saw the target again and fired a second missile,
which those tests read as the first. The P-6 tests whose target is never seen keep their claims (Amendment 2).

**Worlds.** Every output of §7.1 is byte-identical to `main`: before the merge (`ffc6f298` against the WP7 tree with
`last_seen` unhashed, 483 files, 0 differing) and after it (`a59368a3` against the merged tree `169b0c98`, 483 files,
0 differing; `a59368a3` against `ffc6f298` also 0). No battle moved, so nothing is restated, and stop rule 3 holds.

**Goldens and sandbox.** Neither golden gives a player's attack order, so no behaviour moved there. Latent proof
(`proof-latent`, on the tree before the merge, and `proof-latentm`, on the merged tree `169b0c98`, each a git-archive
copy built in a fresh target directory): with `last_seen` unhashed at schema 46, both goldens and
`DEFAULT_SANDBOX_CHECKSUM` reproduce `main`'s values on native debug and release and `wasm32-wasip1` release and
debug. The field then went into the encoding at schema 47 (provisional) and the fixtures were re-recorded once on the
merged tree (`de5cf350`), where the same four builds reproduce the new lists and sandbox value (`proof-final`).

**Client.** The skirmish stage log (`skirmish_match_tests`, release, radv) on `de5cf350` holds the same 42 lines as
on `main` `a59368a3` (the pilot's only attack orders name enemy structures it sees).

**Throughput** (`cargo bench -p sim_core --bench throughput`, medians of three, `main` `a59368a3` and WP7 interleaved,
two rounds, load average 17 to 41 on the shared machine): every scenario stays above 50 ticks/s on WP7.

| Map | Units | `main` (rounds 1, 2) | WP7 (rounds 1, 2) |
|---|---:|---:|---:|
| 64x64 | 0 | 3135, 3120 | 2617, 2962 |
| 64x64 | 100 | 802, 782 | 810, 708 |
| 64x64 | 400 | 109, 62 | 111, 110 |
| 128x128 | 400 | 179, 168 | 180, 179 |
| 256x256 | 0 | 197, 189 | 196, 196 |
| showcase_canyon | 400 | 80, 81 | 77, 78 |
| 64x64, 67 casters | 400 | 199, 199 | 198, 204 |

A first pair taken at load average 95 read 43 ticks/s on `showcase_canyon` for `main` and 73 for WP7; the noise is
the load, not the change (one record lookup per attack-ordered unit in acquisition and one in movement).
