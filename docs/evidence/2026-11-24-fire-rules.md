# Evidence: Fire Rules (B-179, B-180, B-197)

Measurements of plan [`2026-11-24-fire-rules`](../plans/2026-11-24-fire-rules.md): the survey of §2.3, the baseline
and probes of WP0 (§7.2), the golden proofs (§5), the comparisons of every package against WP0 (§7.3) and the traced
moves. Scratch material (binaries, trees, worlds, the probe patch and its summariser, the dump test, the red runs) is
under `~/scratch/fire-rules/`, never committed.

## WP0: survey, baseline and probes

### The survey (§2.3)

The game data is SC2Mapster SC2GameData at `5444e12f13` ("SC2 5.0.14.94342"), copied to
`~/scratch/combat_wp3_data/`. Every line naming one of the four options, mapped to its enclosing weapon with lines
inside `<!-- … -->` flagged, was listed with `~/scratch/fire-rules/scripts/options.awk`:

```bash
cd ~/scratch/combat_wp3_data
for l in core liberty libertymulti swarm swarmmulti void voidmulti balancemulti; do
  awk -f ~/scratch/fire-rules/scripts/options.awk $l/WeaponData.xml
done
```

The script tracks the current `<CWeapon… id="…">` (or the `default="1"` class), whether the line lies in an XML
comment, and prints `file:line`, the weapon and the option. Its output, condensed:

| Option | Lines | Weapons |
|---|---|---|
| `OnlyFireWhileInAttackOrder 1`, `OnlyFireAtAttackOrderTarget 1` | `core` :20, :21 | the `CWeapon` default |
| `FaceTargetWhileInCooldown 1` | `core` :34 | the `CWeaponLegacy` default; no other line in the eight layers |
| `LockTurretWhileFiring 1` | `liberty` :590 | `InfernalFlameThrower`; no other line |
| `OnlyFireAtAttackOrderTarget 0` | `liberty` :126, :140, :525; `libertymulti` :21; `swarm` :39, :83, :178; `swarmmulti` :130; `void` :192; `voidmulti` :12, :24, :35, :46, :206, :257; `balancemulti` :12, :24, :35, :46, :186, :230 | `PrismaticBeam`, `TwinIbiksCannon`, `MothershipBeam`, `IonCannons` (twice), `MothershipPurifyWeapon`, `BuildingShield`, `VoidRaySwarmEnhanced` (twice), `90mmCannonsFake`, `ATALaserBattery`, `ATSLaserBattery`, `BattlecruiserWeaponSwitch`, `PhaseDisruptors`, `ThermalLances` (each in `voidmulti` and `balancemulti`) |
| `OnlyFireAtAttackOrderTarget 0` inside a comment | `swarmmulti` :111, `void` :173 | `VoidRaySwarm` |
| `OnlyFireWhileInAttackOrder 0` | beside every `OnlyFireAtAttackOrderTarget 0` above except `liberty` :140 | `TwinIbiksCannon` clears only the target option (S7) |

No layer sets either attack-order option back to 1. The modelled weapons among them are the Phoenix's `IonCannons`,
the Immortal's `PhaseDisruptors`, the Colossus' `ThermalLances` and the Planetary Fortress' `TwinIbiksCannon`; the
Hellion's `InfernalFlameThrower` is the only locking weapon. The Siege Tank's `90mmCannonsFake` (`balancemulti`
WeaponData:7–20; on the tank at `balancemulti` UnitData:1473 and `voidmulti` UnitData:1575, `Turret="SiegeTank"`)
is a hidden weapon with a `CEffectDamage` of no amount (`90mmCannonsDummy`, `balancemulti` EffectData:1066–1072):
registered as B-198, out of scope.

### Records

`DESIGN.md` §4: B-179 and B-180 point to this plan; **B-197** (turrets frozen between shots) and **B-198** (the Siege
Tank's hidden weapon) are registered, both numbers provisional, next free at merge (B-196 is held by the
hidden-harvesters plan).

### Baseline

**Tree.** `f388f749` (the plan and Q0109 on `main` at `7dc356fc`, schema 43). The baseline worlds are
`~/scratch/fire-rules/world-wp0/`, measured by `scripts/measure.sh` (§7.1, `route_trace` with `--assets`): the matrix
on its seeds of record and 16 more (1,220 runs; 0 expectation failures on the seeds of record, 4 over all seeds),
battle 31 on 100 seeds, battle 33 on 64, and the showcase and showcase effects scenarios on their default seed and
seeds 1 to 8 for 6,000 ticks.

**Census of the measured outputs (§7.1 verify).** `matchup_trace --all-battles` writes `matrix.csv` and `matrix.md`
only: per battle and seed the end tick, each side's survivors and lead hull, the expectation, and the blocking and
reversal counts. A single battle's unit table (`sim_fixtures::trace::report`) has the columns `tick, id, team, x, y,
vx, vy, hp, shields, phase, phase_target, phase_remaining, target, activity`; `route_trace` writes positions,
waypoints and passes. No measured output records a turret angle, so a change of turret angle alone moves the goldens
(which hash `turret_angle`) and no measured output; a change of swing timing shows in the phase columns and outcomes.

### The probe

A scratch patch (`~/scratch/fire-rules/probe/src`, a copy of the WP0 tree with its own git history; the diff is
`probe/probe.diff`) counts events per world and seed and prints the first of each kind. It plays every world of §7.1
and the two goldens (through `golden_dump.rs`, a copy of `golden_replay_tests.rs` with a dump observer that also
writes each checkpoint's entities with their turret angle and weapon phase, `probe/dump-wp0/`). The probed worlds are
byte-identical to the baseline: every output file is equal, and the logs differ only by the probe's lines. Each
weapon step of a turreted weapon is also played on a **shadow**: a clone of the entity stepped with the package's
rule switched on (the wind-up lock of D-2 for (a); D-3, with the Hellion's backswing lock, for (c)), its turret angle
carried from tick to tick while its phase agrees with the real one. `scripts/probesum.py` sums the counts.

- **(a) Lock slews:** a wind-up tick of the locking weapon (`napalm_wheeler`) that changes its turret angle
  (`a-slew`), and a swing the real weapon starts on a tick the locked shadow does not (`a-delayed`).
- **(b) Off-order opportunities:** a weapon step of a unit whose weapon §2.3 marks `fires_at_other_targets`, under an
  `AttackTarget` whose target it cannot work (out of the band with no slop, or failing the weapon filter), while
  another available enemy lies in that weapon's band; tagged with the engagement, whether the unit is immobile,
  whether it chases by D-4's reading (mobile, order's target outside the band), whether pursuit stood it this tick
  (`aim_if_in_range` returned true; a **gate mismatch** is the two disagreeing) and the weapon's `allowed_movement`
  (**ungated** is `Moving` or not chasing).
- **(c) Frozen turrets:** a turreted weapon in `Backswing` or `Cooldown` with a target whose bearing differs from its
  turret (`c-bearing`); a tick the D-3 shadow starts a swing the real weapon does not (`c-delay`, the delay D-3
  removes) or the reverse (`c-later`); a tick ending in `RotatingToTarget` out of a cooldown, backswing or rotation
  (`c-rotating`); and a shadow whose turret angle differs while its phase agrees (`c-angle`).

| World | (a) slew | (a) delayed | (b) all / ungated / mismatch | (c) bearing | (c) delay | (c) later |
|---|---|---|---|---|---|---|
| Matrix, all battles and seeds | 311 | 0 | 54 / 0 / 0 | 162,653 | 970 | 6 |
| Battle 31, 100 seeds | 0 | 0 | 0 | 0 | 0 | 0 |
| Battle 33, 64 seeds | 0 | 0 | 0 | 0 | 0 | 0 |
| Showcase, default and seeds 1–8 | 0 | 0 | 0 | 0 | 0 | 0 |
| Showcase effects, default and seeds 1–8 | 33 | 0 | 0 | 8,937 | 0 | 0 |
| `golden_replay` | 0 | 0 | 0 | 267 | 0 | 0 |
| `golden_replay_orders` | 0 | 0 | 0 | 0 | 0 | 0 |

- **(a), by world.** Slews in battles 9a (97, every seed) and 10 (214, every seed), the first at seed 3203386110
  tick 2 of 9a (unit 1 turned −88 binary units in its wind-up), and in the showcase effects scenario (2 to 5 a seed,
  the Hellions of `showcase_raiders`). **No swing is delayed**: the analogue's turret turns 55.99° a tick and its
  ready step starts the swing on the tick it aligns. 9b has no Hellion wind-up with a moving target.
- **(b), by world.** Only battle 26 (52 events in 17 seeds, Colossus analogues) and battle 30 (2 events in 2 seeds,
  Immortal analogues), all under `Acquired` attacks whose unit chases (mobile, target outside the band) with a
  `Stopped` or `Slowing` weapon: every one gated out by rule 5. **No gate mismatch**: pursuit never stood a unit that
  chases by reach (or the reverse). The first event is battle 26 seed 11 tick 20 (unit 102 chases 29, 10 in its
  band). No Phoenix or Planetary Fortress analogue is under an attack order in any measured world, and no explicit
  attack exists in the matrix.
- **(c), by world.** Bearing events in every battle with a turret and a moving target: 9a, 10, 12a, 12b, 21, 26, 28,
  2b, 30, 37a, 37b, 39a, 39b, 40, 42, the showcase effects scenario (Hellions, Lurkers and the tank) and
  `golden_replay` (the unsieged `railgun_crawler` of `red_siege`, 180° arc, from tick 181). Delay events in ten
  battles only:

  | Battle | Delays | Seeds | Weapon (arc, rate a tick) | First |
  |---|---|---|---|---|
  | 10 | 6 | 4 | Hellion analogue (11.25°, 55.99°) | seed 22 tick 91 |
  | 21 | 16 | 13 | Lurker analogue, burrowed (11.25°, 156.79°) | seed 3203386110 tick 108 |
  | 26 | 72 | 18 | Colossus analogue (56.25°, 55.99°) | seed 11 tick 219 |
  | 28 | 727 | 20 | Immortal analogue (11.25°, 40.32°) | seed 3203386110 tick 112 |
  | 30 | 29 | 15 | Immortal analogue | seed 11 tick 205 |
  | 37b | 8 | 8 | sieged Siege Tank analogue (11.25°, 20.16°) | seed 3203386110 tick 163 |
  | 39a | 1 | 1 | sieged tank | seed 3 tick 297 |
  | 39b | 59 | 20 | sieged tank | seed 22 tick 133 |
  | 40 | 44 | 19 | sieged tank | seed 11 tick 180 |
  | 42 | 8 | 8 | sieged tank | seed 1 tick 129 |

  Every first delay is a weapon that ends its cooldown and rotates (`phase_before=cooldown`,
  `phase_after=rotating`). The six later events (28 twice, 30, 37b, 39b, 40, one each) are swings the real weapon
  starts and the shadow does not; WP4 traces them (P-13 concerns the same target). The unsieged tank's 180° arc
  fires without aiming: its bearing events never delay. The showcase effects scenario and the goldens count no
  delay.

### What WP2 to WP4 are expected to move

| World | WP2 (lock) | WP3 (off-order) | WP4 (between shots) |
|---|---|---|---|
| Matrix | byte-identical (no delayed swing) | byte-identical (54 opportunities, all gated) | battles 10, 21, 26, 28, 30, 37b, 39a, 39b, 40 and 42 may move, each from a counted delay; no other battle |
| Battles 31, 33 | byte-identical | byte-identical | byte-identical (no turret) |
| Showcase | byte-identical | byte-identical | byte-identical (no event) |
| Showcase effects | byte-identical | byte-identical | byte-identical (bearing events, no delay) |
| Goldens | unchanged | unchanged | `golden_replay` moves by `red_siege`'s turret angle from tick 181; `golden_replay_orders` counts nothing and is expected unchanged |

This revises §7.3's prediction in two places: battles 2a, 2b, 3, 9a, 9b and 12a to 12c count no delay (their turrets
are always back on target by the end of the cooldown), and the showcase effects scenario counts none. A world with a
zero count that moves is a defect of the package (§7.2).

## WP1: the fields, latent (schema 43 -> 45)

**Red runs** (`~/scratch/fire-rules/red/`). With the tests written first, `sim_core`'s lib tests,
`weapon_catalog_rules_tests` and `checksum_state_tests` did not compile (`wp1-compile.log`: no field
`fires_at_other_targets`, `locks_turret_while_firing` or `weapon_target`). On a field-only stub (the fields, left
unhashed, no data, no validation, no clearing; `wp1-stub.log`) seven tests failed: `b180_b179_fire_rules_follow_starcraft`
(P-8), `fire_rule_refusals_name_their_field` (P-11), the two validation unit tests, `commit_order_clears_the_weapon_
target_for_every_order`, `every_public_entity_field_changes_the_world_checksum` and `a_projectiles_fire_rules_change_
the_world_checksum`.

**Golden proof** (`scripts/proofcopy.sh`, each build in its own fresh target directory on a `git archive` copy with
fresh mtimes). With the data, validation and clearing in place and the three fields left out of the encoding at
schema 43 (commit object `9ca05d1c`), `golden_replay`, `golden_replay_orders` and `DEFAULT_SANDBOX_CHECKSUM`
(`17_697_741_365_715_682_877`) reproduce as recorded on native debug and release and `wasm32-wasip1` release and
debug (`proof-latent/status`, every run green). With the fields hashed at schema 45 the lists were re-recorded
(header in both fixtures; sandbox `16_215_936_981_767_392_313`), and the WP1 commit `34672217` reproduces them on the
same four builds (`proof-wp1/status`).

**Worlds.** WP1's tree (`scripts/wpcopy.sh`) is byte-identical to WP0 in every output file (483 files).

## WP2: the locked turret (B-179)

**Red run** on WP1's tree (`red/wp2.log`): `a_locked_turret_holds_through_its_wind_up` failed (the turret turned
from 88.20° to 86.40° on the second wind-up tick) and `an_authored_lock_off_turns_the_turret` failed (the shipped
turret turned 88.20°, 86.40°, 84.60°, 82.82°, 81.05° through its swing); `an_unlocked_turret_turns_through_its_
wind_up` and `the_lock_changes_neither_release_nor_damage` passed (guards).

**Worlds.** Byte-identical to WP0 in every output file, as probe (a) predicted (no delayed swing).

## WP3: targets off the attack order (B-180)

**Red run** on WP2's tree (`red/wp3-int-red.log`, `red/wp3.log`): `a_weapon_that_fires_while_moving_strikes_what_
it_passes_in_a_chase` (P-4), `a_standing_unit_fires_at_a_seen_enemy_while_its_target_is_unseen` (P-6),
`its_own_target_is_ranked_as_acquisition_ranks` (P-9: no own target) and `an_authored_order_target_only_weapon_holds_
in_a_chase` (P-10) failed. The guards passed before and after: `the_order_target_comes_first` (P-3), `a_weapon_
that_stands_to_fire_holds_in_a_chase` (P-5) and the two immobile tests (P-7). The test dummies hold position: a
struck idle unarmed unit flees (helpless flight), which first made the immobile test's order end.

**D-4's verify: reach against pursuit** (`target_acquisition::chase_reading_tests`). For a standing target, at
offsets −0.5, −0.01, 0, +0.01 and +0.5 world units from the edge of reach, acquisition's reading
(`reaches_from_here`, before the tick) and pursuit's (the unit stood on the tick) agree. A target that crosses the
edge in the movement phase before its attacker's pursuit runs (the legacy pipeline moves units one by one; the
target spawned first) splits them for that tick: walking out from 0.05 inside, reach says in reach while pursuit
chases; walking in from 0.05 outside, reach says out while pursuit stands. This is the gate mismatch that conditions
WP6 (§11): a `Stopped` or `Slowing` weapon may choose an own target on a tick its unit walks, or hold one tick too
long. Probe (b) counted no mismatch in the measured worlds (no own target would be set in them).

**Worlds.** Byte-identical to WP0 in every output file, as probe (b) predicted (every opportunity gated).

## WP4: turrets between shots (B-197) and the lock's backswing

**Red run** on WP3's tree (`red/wp4.log`): `a_turret_follows_its_target_between_shots` failed (tick 4: the
turret held 84.24° where it should have turned to 82.82°), `the_next_swing_starts_no_later` failed (the weapon
rotated after its cooldown) and `a_locked_turret_holds_through_its_backswing` failed (no turn in the cooldown,
ticks 17 to 45). The guards `between_shots_a_hull_weapon_does_not_turn_its_hull` and `a_swing_out_of_the_
backswing_turns_it` passed before and after.

**Re-derived** (named in the commit): `weapon_cadence_tests::b29_an_engaged_weapon_fires_within_the_engaged_arc_
without_turning` and `b29_a_target_that_leaves_the_engaged_arc_is_acquired_again` knocked the turret off its target
right after a shot and expected it to stay there through the cooldown; they now knock it on the tick the cooldown
ends, which keeps what they test (B-29's two arcs). `turret_tracking_tests.rs` and `hull_turning_tests.rs` pass
unchanged.

**R-7 (hull weapons).** Under an attack order pursuit aims a hull in its band every tick (`aim_if_in_range`), so S3
needs nothing new there. A unit on Hold Position has no pursuit, and between shots its hull stays where it fired
while its target walks across its front (`between_shots_a_hull_weapon_does_not_turn_its_hull`). Registered as
**B-199** (provisional), the hull half of B-197.

**Goldens.** Per-tick dumps of both goldens on WP3's tree and WP4's (`scripts/goldendump.sh`, `dump-wp3/`,
`dump-wp4/`): in `golden_replay` only the turret angles of entities 33 to 35 (`red_siege`, unsieged, 180° arc)
differ, on 273 entity-ticks from tick 181 (entity 33 in its backswing turns one binary unit towards entity 16) to
tick 549; every other field of every entity is equal on all 1000 ticks. Probe (c) counted exactly 273 shadow angle
differences there and no delay. The checkpoints at ticks 250, 300, 450 and 500 changed and were re-recorded.
`golden_replay_orders` and `DEFAULT_SANDBOX_CHECKSUM` are unchanged. The re-recorded list reproduces on native
debug and release and `wasm32-wasip1` release and debug (`proof-wp4/status`).

**Worlds** (`compare-wp4.txt`, `scripts/matrixdiff.py`). Battles 31 and 33, the showcase and the showcase effects
scenario are byte-identical; only `matrix.csv` and `matrix.md` differ:

| Battle | Seeds whose row moved (of record) | Columns | Mean survivors, blue / red | Mean end tick |
|---|---|---|---|---|
| 21 (Lurker) | 3 / 20 (0) | end tick, lead hull | 1.00 / 1.20, unchanged | 474.9 -> 474.8 |
| 26 (Colossi) | 17 / 20 (4) | end tick, survivors, blocking, reversals | 0.40 / 4.05, unchanged | 425.6 -> 418.1 |
| 28 (Immortals) | 20 / 20 (4) | end tick, survivors, blocking, reversals | lings 24.25 -> 23.55 / 0 | 643.8 -> 641.4 |
| 30 (Immortals) | 10 / 20 (3) | end tick, blocking, reversals | 0 / 10.00, unchanged | 247.6 -> 248.0 |
| 37b (sieged tank) | 6 / 20 (1) | lead hull, blocking | 1.00 / 6.00, unchanged | 600 |
| 39b (sieged tanks) | 2 / 20 (1) | lead hull, blocking | 1.00 / 2.50, unchanged | 600 |
| 40 (sieged tanks) | 15 / 20 (3) | end tick | 0 / 2.75, unchanged | 273.1 -> 272.1 |

No winner and no expectation changed: 0 failures on the seeds of record and 4 over all seeds, as at WP0. P-16's
direction holds where survivors move (battle 28: the Immortal analogues' enemies lose more) and battles 26, 28 and
40 end earlier; battle 30 ends 0.4 ticks later on average with the same survivors.

**Traces** (`scripts/trace.sh`, `scripts/firstdiff4.py`, `firstdiff-wp4.txt`). Per-tick unit tables of the ten
battles probe (c) counted delays in, on all 20 seeds, WP3 against WP4: 128 seeds move, in exactly those ten
battles (10: 4 seeds, 21: 13, 26: 18, 28: 20, 30: 16, 37b: 9, 39a: 1, 39b: 20, 40: 19, 42: 8); every seed without
a probe event is identical. In 125 the first difference is the probe's first delay for that battle and seed, tick
and unit (the WP3 weapon rotates after its cooldown, the WP4 one winds up). The other three (battle 30 seed 17
tick 177 unit 26, 37b seed 8 tick 151 unit 10, 40 seed 15 tick 129 unit 11) are the probe's "later" events, each a
target switch: the turret had followed a target that died or was lost during the cooldown, and the new target lay
nearer the angle the frozen turret had kept. P-13 concerns the same target; no swing at the same target starts
later. So every move is a counted event of B-197 (P-15).

## Client skirmish

The pilot trains `pulse_bot`, which has no turret; the skirmish's stage log is not affected by any package (§7.3).

## Merge with `main` `d92e40bb` (hidden harvesters, schema 44)

Schema 44 is the hidden-harvesters plan's, merged first; this plan keeps 45. On the merged tree, with the three
fields left out of the encoding at schema 44 and D-3 switched off (scratch commit object `9a568676`), both goldens
and `DEFAULT_SANDBOX_CHECKSUM` (`8_410_876_910_713_629_694`) reproduce main's schema-44 values on native debug and
release and `wasm32-wasip1` release and debug (`proof-latent44/status`). Per-tick dumps of the merged tree with
D-3 off and on (`dump-m-off/`, `dump-m-on/`) differ exactly as before the merge: in `golden_replay` the turret
angles of entities 33 to 35 on 273 entity-ticks from tick 181 to 549, nothing else; `golden_replay_orders` equal.
The lists were re-recorded once at schema 45 (sandbox `6_724_706_104_475_870_814`) and reproduce on the same four
builds (`proof-merged/status`).
