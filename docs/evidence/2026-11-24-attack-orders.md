# Attack orders by stance: measurements per package

This file records the measurements of plan [`2026-11-24-attack-orders`](../plans/2026-11-24-attack-orders.md)
(§7; B-174, B-175, B-186, B-187): the sources the plan's rules rest on, the S8 join, the world comparison of every
package against the WP0 tree, and the red runs of the new tests. WP0 creates it; WP1 to WP4 append.

B-186 and B-187 are provisional numbers, the next free at merge (`main` ends at B-181 and another branch holds
B-182 to B-185).

## Sources

All from SC2GameData `5444e12f13` (5.0.14), local copies under `~/scratch/combat_wp3_data/<layer>/`, read as data
(plan §2.1). The override chain is `core → liberty → libertymulti → swarm → swarmmulti → void → voidmulti →
balancemulti`.

| # | Fact | Source |
|---|---|---|
| S1 | The attack ability's only alliance filters are for acquisition (`AcquireFilters` excludes `Player, Ally, Neutral, Dead, Hidden, Invulnerable, Benign`) and the smart click (`SmartFilters` excludes `Player, Ally`); `CAbilAttack` has no `TargetFilters` of its own | `core` AbilData:115–139 (:117–119) |
| S2 | Its `Execute`, `Towards` and `Barrage` commands go to every selected unit that has them (`ToSelection`) | `core` AbilData:127–135 |
| S3 | Weapons accept own units unless they say otherwise: default `TargetFilters` `Visible;-`; ordinary weapons exclude `Missile, Stasis, Dead, Hidden, Invulnerable`; `PointDefenseLaser`, `BypassArmorDroneWeapon`, `WidowMineDummy`, `TwinIbiksCannon` and `BuildingShield` exclude their own side or themselves by name | `core` WeaponData:3–31; `liberty` WeaponData:9, :19, :35, :71, :136; `swarm` WeaponData:8, :76; `voidmulti` WeaponData:267; `balancemulti` WeaponData:240 |
| S4 | `AttackWarpPrism` excludes `Self, Player, Ally, Neutral, Enemy` in its acquire and smart filters and turns off `ToSelection` | `liberty` AbilData:1100–1110 |
| S5 | The move ability carries `Move`, `Patrol`, `HoldPos`, `AcquireMove` (Scan Move) and `Turn`, each `ToSelection` | `core` AbilData:533–561 (:555–557) |
| S6 | Every unarmed mobile unit's card puts `attack,Execute` and then `move,AcquireMove` in the Attack slot (row 0, column 4); these units carry `move` and no `attack`, so the slot is Scan Move | Medivac `liberty` UnitData:5475–5478, :5485–5486; Observer :3443–3445, :3452–3453; Overlord :7391–7395, :7400–7401; Overseer :7520–7521; Raven :5621–5622; Infestor :10108–10109; Warp Prism :3859–3860; Viper `swarm` UnitData:2363–2364; Swarm Host `swarm` :1214–1215 |
| S7 | The unburrowed Lurker has the same pair, and 5.0.14 adds `attack` to it, so its slot is Attack although it has no weapon | `liberty` UnitData:727–729, :735–736; `voidmulti` UnitData:974; `balancemulti` UnitData:909 |
| S8 | Every unit with a weapon carries an attack ability; the units with an attack ability and no weapon are listed below | the join below |
| S9 | The burrowed Roach's slot changes from Attack to Scan Move in LotV | `liberty` UnitData:7284; `voidmulti` :1472; `balancemulti` :1383 |
| S10 | The sieged Siege Tank and the burrowed Lurker carry `attack` | `liberty` UnitData:5172–5174, :793–796 |
| S11 | The Lurker's burrow and unburrow change the ability set at the start of the switch | `swarm` AbilData:1663–1673 |

Web sources (plan §2.1): Liquipedia's *Micro (StarCraft)*; the Blizzard forum answers
`us.forums.blizzard.com/en/starcraft/t/how-can-i-remove-building/2150` (StarCraft: Remastered, lineage only) and
`us.forums.blizzard.com/en/sc2/t/which-units-deal-friendly-fire/12103` (splash only). None states the 5.0.14 A-click
on an own unit, so B-175 stays **verify** (Q0104).

## The S8 join

A short Python script (stdlib `xml.etree`, run with `python3 -I`; kept in `~/scratch/attack-orders/s8/`) collects
every `CAbilAttack` id from the eight layers' `AbilData.xml`, then applies each layer's `AbilArray` and `WeaponArray`
entries per `CUnit` (`index` and `removed` honoured, `parent` inheritance not followed, plan R-6). Its output:

```text
attack abilities: AttackWarpPrism, BattlecruiserAttack, attack, attackProtossBuilding

# units with an attack ability and no weapon
AdeptPhaseShift	attack
FlyoverUnit	attack
LurkerMP	attack,attack
Nexus	attackProtossBuilding
PylonOvercharged	attackProtossBuilding,attackProtossBuilding
QueenMP	attack
Replicant	attack
WarpPrismPhasing	AttackWarpPrism

# units with a weapon and no attack ability
```

- The second list is empty: every unit with a weapon carries an attack ability (S8).
- `LurkerMP` (the unburrowed Lurker, S7), `AdeptPhaseShift` (the shade, plan §14), `Nexus` and `PylonOvercharged`
  (under `PurifyNexusRequirements`) are the multiplayer cases the plan names. `FlyoverUnit`, `QueenMP` and
  `Replicant` are the three campaign or flyover units.
- `WarpPrismPhasing` carries `AttackWarpPrism`, S4's ability that excludes every alliance and turns off
  `ToSelection`, so it gives no A command in practice.
- The Bunker does not appear: its Attack is a redirect (`CAbilRedirect`, stance-checked-orders S10), not a
  `CAbilAttack`.
- `LurkerMP` and `PylonOvercharged` list their ability twice because a later layer appends it without `index`; the
  join does not merge duplicates.

## How to reproduce

The scripts are the stance-checked-orders plan's, copied to `~/scratch/attack-orders/scripts/` with the worktree and
output paths changed. `matchup_trace` embeds the matrix fixture and the catalog it was compiled with, so it is
rebuilt for each tree; `route_trace` is given `--assets` explicitly.

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/attack-orders-plan
R=~/scratch/attack-orders
$R/scripts/base.sh <wp>        # builds matchup_trace and route_trace in release, runs measure.sh into world-<wp>
$R/scripts/compare.sh $R/world-wp0 $R/world-<wp>
$R/scripts/client.sh <wp>      # client targets in release, one at a time under ~/scratch/gate.lock
```

`measure.sh` plays the matchup matrix on its seeds of record and 16 more, battle 31 on 100 seeds, battle 33 on 64,
and the showcase and showcase effects scenarios on their default seed and seeds 1 to 8 for 6,000 ticks.

## WP0: baseline

The tree of the plan and its questions (`c72c6a4f`, exported with `git archive` to
`~/scratch/attack-orders/tree-wp0` so that later edits in the worktree cannot reach a measurement), measured into
`~/scratch/attack-orders/world-wp0`: 483 files; the matrix reports "1180 runs: expectation failures 0 on the seeds of
record, 4 over all seeds". `base.sh` now takes the commit to measure as its second argument for that reason: a first
WP0 run measured the worktree while WP1's `units.ron` edit landed in it, and its showcase-effects runs read the new
field (`route_trace: invalid catalog …`); that run was discarded.
