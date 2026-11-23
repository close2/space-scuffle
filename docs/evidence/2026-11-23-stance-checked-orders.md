# Stance-checked orders: measurements per package

This file records the measurements of plan
[`2026-11-23-stance-checked-orders`](../plans/2026-11-23-stance-checked-orders.md) (§7; B-171, B-172, B-173): the
sources the plan's rules rest on, the world comparison of every package against the WP0 tree, and the red runs of
the new tests. WP0 creates it; WP1 to WP4 append.

## Sources

All from SC2GameData `5444e12f13` (5.0.14), local copies under `~/scratch/combat_wp3_data/<layer>/`, read as data
(plan §2.1). The override chain is `core → liberty → libertymulti → swarm → swarmmulti → void → voidmulti →
balancemulti`.

| # | Fact | Source |
|---|---|---|
| S1 | Stop is its own ability (`CAbilStop`); Hold Position is the `HoldPos` command of `move` | `core` AbilData:1013–1027, :1362; :552–554 |
| S2 | Stop, Hold Position and Attack go to every selected unit that has them (`ToSelection`) | `core` AbilData:1015–1016, :127–129, :553 |
| S3 | The sieged Siege Tank carries `stop`, `attack` and `Unsiege`, and no `move` | `liberty` UnitData:5172–5174 |
| S4 | Its Attack button has face `AttackBuilding`; the unsieged tank's has `Attack` | `liberty` UnitData:5187; :5116 |
| S5 | The burrowed Lurker carries `attack`, both burrow abilities, `move` and `stop`; Attack face `AttackBuilding` | `liberty` UnitData:793–796, :800; `void` UnitData:767 |
| S6 | Armed structures carry `stop` and `attack` (Photon Cannon, Missile Turret, Planetary Fortress, Spine and Spore Crawler) | `liberty` UnitData:11083–11084, :6064–6065, :10696–10697, :8652–8653, :8784–8785 |
| S7 | Unarmed structures carry neither (Command Center, Orbital Command, Nexus) | `liberty` UnitData:10411–10418, :10557–10564, :4218–4223 |
| S8 | The Shield Battery's `liberty` `stop` (entry 2) is removed in 5.0.14 | `liberty` UnitData:1603–1605; `voidmulti` UnitData:1557; `balancemulti` UnitData:1463 |
| S9 | The Nexus's Stop and Attack require `PurifyNexusRequirements` | `swarm` AbilData:954–960, :968–970 |
| S10 | The Bunker's Stop and Attack redirect to its passengers | `liberty` UnitData:6192–6193; AbilData:3606–3611, :3633–3638 |
| S11 | The unburrowed Lurker carries `attack` without a weapon | `voidmulti` UnitData:974; `balancemulti` UnitData:909; `liberty` UnitData:735 |
| S12 | The attack ability skips own and allied units for acquisition and the smart click only | `core` AbilData:118–119 |
| S13 | Stop drops the unit's orders; nothing in `CAbilStop` treats a structure differently | `core` AbilData:1013–1027 |

## How to reproduce

The scripts are the anchored-commands plan's, copied to `~/scratch/stance-orders/scripts/` with the worktree and
output paths changed (`client.sh` also runs `battle_card_path_tests`). `matchup_trace` embeds the matrix fixture and
the catalog it was compiled with, so it is rebuilt for each tree; `route_trace` is given `--assets` explicitly.

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/stance-orders-plan
R=~/scratch/stance-orders
$R/scripts/base.sh <wp>        # builds matchup_trace and route_trace in release, runs measure.sh into world-<wp>
$R/scripts/compare.sh $R/world-wp0 $R/world-<wp>
$R/scripts/client.sh <wp>      # client targets in release, one at a time under ~/scratch/gate.lock
```

`measure.sh` plays the matchup matrix on its 4 seeds of record and 16 more (20 a battle), battle 31 on 100 seeds,
battle 33 on 64, and the showcase and showcase effects scenarios on their default seed and seeds 1 to 8 for 6,000
ticks.

## WP0: baseline

The tree of the plan and Q0098 (`b78206d6`, behaviour-identical to `main` at `aa92b094`), measured into
`~/scratch/stance-orders/world-wp0`: 483 files; the matrix reports "1160 runs: expectation failures 0 on the seeds
of record, 4 over all seeds". The four battles whose orders WP1 restates read (20 seeds each):

| Battle | Blue wins | Red wins | Unresolved | Survivors blue – red | End tick | Red lead hull |
|---|---|---|---|---|---|---|
| 37a, Abduct on a sieged tank | 0 | 0 | 20 | 1.00 – 4.65 | 600.0 | — (dead on 20) |
| 37b, control | 0 | 0 | 20 | 1.00 – 6.00 | 600.0 | 141.45 (20) |
| 39a, Blinding Cloud on sieged tanks | 20 | 0 | 0 | 7.90 – 0.00 | 280.1 | — |
| 39b, control | 0 | 0 | 20 | 1.00 – 2.50 | 600.0 | 82.35 (17) |
