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
