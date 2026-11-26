# Evidence: Carrier Interceptors — a Hangar of Launched Units (B-22, B-228)

Measurements of plan [`2026-11-28-carrier-interceptors`](../plans/2026-11-28-carrier-interceptors.md): WP0's census,
probe, positive control, close-battle list, four-build baseline and throughput (§7.1), and the world checks, red runs,
proofs and confirmations of the later packages. Scratch material (trees, binaries, worlds, the probe patch and its
control, the census script, the red runs) is under `~/scratch/carrier/`, never committed.

## WP0: census, probe, baseline and close battles

WP0's tree is the branch head `a9c0a248` (`main` `2afbad04` merged in: weapon delivery, checksum schema 56; Q0129
closed, Q0130 filed). Its commit adds only the records of plan §13 (B-22 and B-228 restated, B-229 and B-230
registered) and this file; every measurement below is of `a9c0a248`'s code and assets.

### Census (§7.1 (c))

`python3 -I ~/scratch/carrier/census/census.py <tree>` (output `~/scratch/carrier/census/census.txt`) reads every place
the Carrier analogue (`mothership_carrier`, `unit_type_id: 32`) can enter a world:

| Place | Mentions |
|---|---:|
| `crates/sim_core/tests/fixtures/matchup_matrix.ron` (every battle's squads, battles 31 and 33) | 0 |
| `golden_replay.ron`, `golden_replay_orders.ron` (squads and structures) | 0 |
| `assets/catalog/start_loadouts.ron` | 0 |
| `crates/sim_core/benches/throughput_scenarios.ron` | 0 |
| `assets/scenarios/{default_battle,showcase,showcase_effects,skirmish}.ron` | 0 |
| `crates/sim_fixtures/src/**` (the sandbox, the trace and harness worlds) | 0 |
| Production code of every crate (C5 names no slug) | 0 |
| `assets/catalog/structures.ron`: `stellar_portal_dome` `trains_units` | the one trainer |

So the Carrier enters a world only by being trained at the Stargate analogue (by a player or an AI); no measured world
names it. **The census lists no measured world as carrying the Carrier**, so a first difference of class V or H
(§6.6) in any measured world is a stop (stop rule 2).

### Probe (§7.1)

The scratch probe (`~/scratch/carrier/probe/probe.rs`, applied to a `git archive` copy by `apply_probe.py`; it names the
slug, being scratch) counts (p) every Carrier entity committed to the world (spawned or trained, at
`SimWorld::commit_spawn`, the one insertion path), (r) every release of the Carrier's salvo, (k) every salvo strike that
takes a living target to zero life, and (d) every weapon hit on a Carrier (strikes, and life plus shields lost). It
changes no state: its worlds are byte-identical to WP0's (`compare.sh`: 490 files, none differing).

**Positive control.** A scratch test in the probe copy (`probe/probe_control.rs`) spawns a Carrier beside an enemy
`pulse_bot` and an enemy air-attacking saucer for 300 ticks: the probe logs (p) 1, (r) 2 missile releases, (k) 1, (d)
10 strikes and 88.0 life plus shields lost (`~/scratch/carrier/probe-wp0/control.log`). So a zero below is a measured
zero.

| World | (p) Carriers | (r) releases | (k) kills | (d) hits taken |
|---|---:|---:|---:|---:|
| Matrix, every battle (seeds of record and 1–16, 1,220 runs) | 0 | 0 | 0 | 0 |
| Battles 31 (100 seeds) and 33 (64 seeds) | 0 | 0 | 0 | 0 |
| Showcase, default seed and 1–8 (6,000 ticks) | 0 | 0 | 0 | 0 |
| Showcase effects, default seed and 1–8 (6,000 ticks) | 0 | 0 | 0 | 0 |
| Skirmish scenario (`scenarios/skirmish.ron`, 6,000 ticks) | 0 | 0 | 0 | 0 |
| `golden_replay`, `golden_replay_orders` | 0 | 0 | 0 | 0 |

**No measured world carries a Carrier**, as §3 item 10 predicted, including the showcase, showcase effects and skirmish
worlds whose AI could train one; stop rule 9 holds (no world the plan predicted empty carries it). The game data was
not re-read: §2.1's values are the plan's, read at `5444e12f13`.

### Baseline on four builds (§7.1)

A `git archive` copy of `a9c0a248`, every file touched, built in a fresh target directory
(`~/scratch/carrier/scripts/proofcopy.sh wp0 a9c0a248`, `CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<copy>"`):
`golden_replay_tests` (every recorded checkpoint of both goldens) and `fixture_tests` (`DEFAULT_SANDBOX_CHECKSUM`) pass
on native debug and release and on `wasm32-wasip1` release and debug, 8 of 8 runs green (schema 56).

### The close-battle list (§6.7), fixed before WP1

A run is won by the side with survivors while the other has none; otherwise it is a draw. A battle is close when the
side that wins fewer of its baseline runs wins at least 2 of its 20 (battles 31 and 33: the same share of their 100 and
64 seeds). From WP0's worlds alone (`python3 -I ~/scratch/carrier/scripts/close.py ~/scratch/carrier/world-wp0`):

| Battle | Runs | Blue wins | Red wins | Draws | Minority share | Item |
|---|---:|---:|---:|---:|---:|---|
| 14_thor_vs_ultralisk | 20 | 5 | 15 | 0 | 0.25 | close |
| 26_100_lings_vs_6_colossi | 20 | 16 | 4 | 0 | 0.20 | close; named by Q0103, Q0044 |
| 28_100_lings_vs_10_immortals | 20 | 6 | 14 | 0 | 0.30 | `known_bug` B-192: not close |
| 31_15_marauders_vs_15_stalkers | 20 | 17 | 3 | 0 | 0.15 | close; its claim says so |
| 31_15_marauders_vs_15_stalkers, traced | 104 | 71 | 33 | 0 | 0.32 | close |
| 33_24_marines_vs_12_zealots, traced | 68 | 67 | 1 | 0 | 0.01 | named by its comment and claim, Q0041, Q0044 |
| 32_30_marines_vs_15_banelings | 20 | 15 | 1 | 4 | 0.05 | not close |

Every other matrix battle has one winner on all 20 runs, or one winner and draws, or draws only. `grep -n -i
"close\|restated" crates/sim_core/tests/fixtures/matchup_matrix.ron` finds battle 31's claim, battle 33's comment and
claim, battle 35's claim and restatement, and the restatements of 26, 38a and 38b; Q0103 and Q0044 name 26, 33, 38a
and 38b. **The list: 14, 26, 31, 33, 35, 38a, 38b**, the weapon-delivery plan's list. It is not edited after WP1
starts.

### Throughput (§5)

`cargo bench -p sim_core --bench throughput`, the release binary of `a9c0a248` (`~/scratch/carrier/bench-bin-wp0/`), one
run under `perf stat -e instructions:u` (load average 8.8 to 4.9, another agent's measurements running): 64x64 0 units
3140 ticks/s, 100 units 800, 400 units 108; 128x128 400 units 177; 256x256 0 units 195; `showcase_canyon` 400 units 81;
67 casters 199. Every scenario is above 50 ticks/s. User-space instructions of the whole run: 270,018,499,441. WP6
compares the final tree against this binary, interleaved.

## WP1: the Carrier's vitals

Red first (`~/scratch/carrier/red/wp1-red.log`): on WP0's tree with WP1's test edits, `catalog_reference_values_tests`
fails 9 tests (the label `"Carrier"` names no archetype while the row still reads "Carrier / Mothership", and the new
separation test finds no Carrier). Green after D-1 (life and shields 300/150, radius and separation 1.25, the analogue
"Carrier"): `catalog_reference_values_tests` 52, `weapon_catalog_rules_tests` 45, `runtime_catalog_tests` 53,
`collision_class_tests` 9 passed (`~/scratch/carrier/logs/wp1-green.log`).

**Worlds.** WP1's worlds (binaries and assets of `57864951`) against WP0's: `compare.sh` finds 490 files, none
differing (`~/scratch/carrier/compare-wp1.txt`). The commit was later amended to `b04f4247` for a clippy fix in a test
file only (`the_carrier_separation_radius_follows_starcraft` returns nothing); its assets and code are those
measured. No world carries the Carrier (census), so no class V difference arises; stop rules 2 and 4 hold.

## WP2: the hangar mechanism, latent

Red first (`~/scratch/carrier/red/wp2-red.log`): a `git archive` copy of WP1 with `hangar_tests.rs` added fails to
compile (no `HangarSpec`, `HangarDeath`, `MagazineLaunch`, no `hangar`/`hangar_link` accessors, no
`magazine_launch` field, no `UnselectableRecipient`/`UntargetableTarget`, no `selectable`/`order_targetable`). Green
after the change: `hangar_tests` 13 (H-2 to H-9, H-11's determinism), the `hangar` unit tests (a release restarts
the channel only on a new target; every field of the hangar state, a docked record, the channel and the link is
hashed; a corrupted count is reported and leaves the hangar alone), the validation tests of H-10
(`catalog/hangar_validation.rs`: every D-2, D-4 rule refused naming its field; every member rule refused on the
complete catalog; omitted fields read their defaults), the encoding test of a magazine launch and `the_schema_is_57`.

**Worlds.** WP2's worlds against WP1's: 490 files, none differing (`~/scratch/carrier/compare-wp2.txt`). The shipped
catalog has no hangar yet, so the hangar phase is skipped.

**Latent proof (§5, stop rule 1).** A `git archive` copy of `268a437e` whose encoding leaves `Entity::hangar`,
`Entity::hangar_link` and `WeaponSpec::magazine_launch` unhashed with the schema at `main`'s 56
(`~/scratch/carrier/scripts/unhash.py`), built in a fresh target directory with every file touched
(`proofcopy.sh wp2-latent 268a437e unhash.py`, `wasmtime --dir=<copy>`): both goldens' every checkpoint and
`DEFAULT_SANDBOX_CHECKSUM` reproduce on native debug and release and `wasm32-wasip1` release and debug
(`~/scratch/carrier/proof-wp2-latent/status`). The goldens of the tree itself fail by the schema alone (57) until
WP6's re-record.

## WP3: the data

Red first (`~/scratch/carrier/red/wp3-red.log`): on WP2's tree with WP3's test edits, `catalog_reference_values_tests`,
`hangar_tests` and `weapon_catalog_rules_tests` fail (no shipped hangar, no member row, the census still lists the
salvo with `known_bug: "B-22"`). Green after D-13 (`9b40d6ab`).

**Worlds.** WP3's worlds against WP2's: 490 files, none differing (`~/scratch/carrier/compare-wp3.txt`). No measured
world carries the Carrier (census), so no class H difference arises; stop rules 2 and 4 hold.

**Fallout found later.** The full simulation run of WP3 (`~/scratch/carrier/logs/wp3-simcore-all.log`) used a build
from before the commit's last edits; a run on `43b0529e` (`~/scratch/carrier/logs/sim-all-wp4.log`) confirmed that
`effect_snapshot_tests` passes (14 of 14) and found four tests the new member or the removed salvo broke:
`stance_orders_tests`' P-1 and P-2 and `player_view_tests`' `can_raise` check iterate every shipped archetype
through a player command, which admission now refuses for the member whatever the rule under test (D-8), so they
leave out archetypes that are not `selectable` (H-9 pins the refusal); `missile_mover_tests`' W-15 found its subject
as the shipped missile weapon with a `Hidden` launch, the salvo WP3 replaced, and now makes the shipped ground missile
weapon's launch `Hidden` on a copy (`e56831c1`). The asset baker's golden list of unit prompts lacked the member's
three lines; WP5 adds them.

## WP4: client

`43b0529e`. H-16 (`runtime_media_tests`) and H-17 (`battle_input`'s `hangar_pick_tests`) as left by the previous
agent, both red on WP3's tree (no `member` in the media entry; `can_pick` read no flag). The two client failures after
WP3 (`~/scratch/carrier/logs/wp4-tests.log`) and their causes:

- `effect_catalog_tests::every_shipped_weapon_names_the_looks_of_the_event_it_publishes`: the Carrier's two launch
  weapons (the same weapon authored for both planes) were counted as silent weapons by B-123's check. A launch weapon
  is now its own delivery kind in the test's helper (D-11 item 4): it publishes no event and names no look, and the
  launch weapons are exactly the hangar units' weapons (H-18).
- `weapon_look_tests::the_table_is_sized_from_the_catalog_and_the_budget`: `HeldBeams::capacity_for` bounded the
  supply-free member as a unit costing 1 supply (200 per player), which is neither the member's bound nor the shipped
  figure the test pins. A member is fielded only through its hangar units, so its bound is the cap over the hangar
  unit's supply times `max_count` (`units_per_player`, `effects/beams.rs`): 33 × 8 = 264 per player, 8 × 264 × 2
  beam weapons = 4,224, above the cheapest supply-bounded beam units' 800. The table takes the smaller of that and the
  pool share, 1,536 (was 800). A unit test (`bound_tests`) checks both bounds and that a bigger magazine raises the
  member's in the same binary.

H-19 (`battle_presentation_tests::a_docking_member_draws_no_death_and_a_killed_one_does`): the shipped Carrier,
found by property, launches its four start members at a passive air enemy; the enemy dies (its own death is of another
kind), the members fly home and dock: no `BubbleShatter` of the member's kind for the whole return, no death spawn
asked of the effect system (`spawn_request` with the shipped tables) and no legacy shatter, and the docked members
leave the snapshot; a relaunched member killed draws exactly one death. It pins behaviour WP2 built (the simulation's
`members_return_and_dock_when_the_target_is_gone` already counts no shatter at a dock), so it passed on its first run
rather than red. Logs `~/scratch/carrier/logs/wp4b-tests.log`, `wp4c-tests.log`.

**H-20 is not done:** it needs a Carrier on the field in the client, and the client refuses an identity without media
(H-16); WP5 produced none (below). It waits for the Carrier's media.

## WP5: media, stopped

[`docs/evidence/veo/2026-11-26-batch-carrier.md`](veo/2026-11-26-batch-carrier.md): the preflight gave its usual
Express-mode 404 (Q0042); four seeds of each identity, 8 generations, $3.20 at list price ($116.85 of $230.00 spent),
all drew diagonal light shafts across the backdrop (the owner's no-rays rule) and were rejected; stop rule 8, and the
owner is asked ([Q0131](../questions/closed/0131-carrier-media-light-shafts.md), default: no more spend, the Carrier
stays locked by `NoMedia`). H-21 is not met; nothing was installed, baked or added to the required set.

**Continued after the owner's answer to Q0131** (2026-11-26, branch `carrier-media`): per-identity negative terms and
up to four more seeds per identity, then the best available. 13 more generations, $5.20 (the batch 21, $8.40 of
$10.00; the ledger $128.45 of $230.00); six clips installed and baked, both identities in the required set,
`audit-media --strict` 34 of 34, so H-21 is met and the client unlocks the Carrier. The accepted flaws are in the
[batch evidence](veo/2026-11-26-batch-carrier.md). H-20 still waits: a battle with interceptors cycling allocates
render entities in the client (B-237), which the render-entity pool package fixes.

## WP6: confirmation, re-record, throughput

**Merge.** `main` `b1dbb43f` (Q0129 closed, Q0130 filed) was already an ancestor of the branch; `main` had not moved, so
no merge commit was needed. Schema 57 is free (`main` is at 56).

**Final worlds.** Trace binaries and assets of `0fcce364` (the final simulation code; the later commits change
fixtures and records only) against WP3's: `compare.sh` finds 490 files, none differing
(`~/scratch/carrier/compare-final.txt`). With WP1 to WP3 each byte-identical to its predecessor, the final worlds are
WP0's: no class S, V, H or P difference outside the goldens, no outcome moved, the close list is untouched.

**Held-out seeds (§7.3, confirmation only).** `heldout.sh` (matrix seeds 7001 to 7016, battles 31 and 33 on 7001 to
7064, the showcase and showcase effects on seeds 9 to 16, the skirmish scenario on seeds 9 to 12) on `main` `b1dbb43f`
and on `0fcce364`: 409 files, none differing (`~/scratch/carrier/compare-heldout.txt`). Stop rule 3 holds.

**Re-record and proof (§5).** `golden_replay.ron`, `golden_replay_orders.ron` and `DEFAULT_SANDBOX_CHECKSUM`
(`10_676_883_327_660_124_161`) re-recorded at schema 57 in `201411e3`, with the plan's header. `proofcopy.sh final
201411e3`: a `git archive` copy, every file touched, a fresh target directory, `wasmtime --dir=<real path>`: both goldens
and the sandbox pass on native debug and release and `wasm32-wasip1` release and debug, 8 of 8
(`~/scratch/carrier/proof-final/status`). Latent proof of the final tree: `proofcopy.sh final-latent 201411e3
unhash_final.py` leaves `hangar`, `hangar_link` and `magazine_launch` unhashed at schema 56 and restores `main`'s
fixtures and sandbox value (`6_457_533_297_407_649_319`): 8 of 8 (`~/scratch/carrier/proof-final-latent/status`).
So the goldens moved by the encoding alone (class S).

**Throughput (§5, stop rule 5).** `wp6-bench.sh`: the benchmark of WP0 (`a9c0a248`), of the final tree without the new
row (`final-x`, built from a copy of `201411e3` with the `hangar` row removed, so the runs are like for like) and of the
final tree, interleaved, two rounds, each run under the gate lock and `perf stat -e instructions:u`. The server was
heavily loaded by other agents (load average 21 to 183), so the ticks per second are noisy; instructions are not:

| Binary | Round 1 instructions | Round 2 instructions |
|---|---:|---:|
| WP0 | 270,018,508,127 | 270,018,529,959 |
| final without the `hangar` row | 270,141,721,337 | 270,141,739,692 |
| final | 297,769,687,286 | 297,769,685,994 |

The seven existing rows take **+0.046 %** instructions (limit +0.5 %): one members pass per tick over entities with
no link. Ticks per second of the final binary, rounds 1 and 2: 64x64 0 units 2853, 3022; 100 units 760, 786; 400
units 88, 77; 128x128 400 units 156, 171; 256x256 0 units 117, 189; `showcase_canyon` 55, 77; 67 casters 194, 179;
**`hangar` (64x64, 200 units, 12 hangar units) 348, 373**. Every scenario is at 50 ticks/s or more. (WP0's own binary
fell to 44 on the canyon in round 2 at load average 183, so the floor is a property of the load, not of the change.)

The `hangar` row launches 6.6 members per measured tick on average. A 900-tick trace of the row
(`~/scratch/carrier/src-hdebug`, scratch only) shows why it is not about 100: the enemy `pulse_bot` army shoots the
engaged Carriers' members down, a rebuild takes 214 ticks, and the Carriers out of reach keep theirs docked (at most
18 in flight, 37 to 60 docked). The engaged Carriers relaunch every rebuilt member at once, and their channels restart
on each new target, as D-4 and D-5 say. Recorded in the plan's As built, deviation 14.
