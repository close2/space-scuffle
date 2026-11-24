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
