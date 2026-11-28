# Evidence: Interceptor Strafing (B-229)

Measurements and source checks for the interceptor-strafe packages of plan
[`2026-12-03-movement-work-on-solver`](../plans/2026-12-03-movement-work-on-solver.md) (WP-I0, WP-I1). Scratch material
(web pages read, the tree copy and its probe, the probe log) is under `~/scratch/wp-i0/` and is never committed.

## WP-I0: sources, census and baseline

WP-I0's tree is `14417030` (branch `movement-on-solver-plan`, checksum schema 62). Tags follow the carrier plan's
convention: [DATA] is read from the game data, [EXT] from an outside description, and [INF] is inferred.

### 1. Sources (plan §3.3, the verify items)

**Game data.** SC2GameData `5444e12f13` ("SC2 5.0.14.94342"), the carrier plan's copy S1 at
`~/scratch/combat_wp3_data/`. The plan's T-1 to T-4 were re-read line by line, and every one holds as cited.

| # | Source | What it gives |
|---|---|---|
| D-1 | `core` WeaponData:40–44 | `<CWeaponStrafe default="1">`: `LoiterInnerRadius 2`, `LoiterRadius 4`, `Effect ##id##`, and nothing else [DATA] |
| D-2 | `core` WeaponData:3–32 | The `CWeapon` default holds `DamagePoint 0.167`, `Backswing 0.5`, `RangeSlop 1`, `ArcSlop 11.25`, `MinScanRange 5` and the options. It names no `AllowedMovement` and no `RandomDelay` [DATA] |
| D-3 | `core` WeaponData:33–39 | The `CWeaponLegacy` default holds `RandomDelayMin -0.0625`, `RandomDelayMax 0.125`, `FaceTargetWhileInCooldown` and `UninterruptibleDuration`. These are legacy-class fields, and `CWeaponStrafe` is a sibling class, not a child [DATA; that the strafe class does not inherit them is INF from the class layout] |
| D-4 | `liberty` WeaponData:626–637 | `InterceptorBeam`: `Range 2`, `Arc 19.6875`, `Period 3`, `DisplayAttackCount 2`, `Effect InterceptorBeamPersistent`, `TeleportResetRange 0`. It has no loiter, `AllowedMovement` or `RandomDelay` field [DATA] |
| D-5 | `voidmulti` WeaponData:306–337, `balancemulti` WeaponData:279–310 | `InterceptorBeamAA` and `InterceptorBeamAG`: no loiter or movement field either [DATA] |
| D-6 | Every `WeaponData.xml` layer, searched for `AllowedMovement` | 60 occurrences, all on `CWeaponLegacy` entries (for example `IonCannons Moving` in `libertymulti`). No `CWeaponStrafe` entry sets it [DATA] |
| D-7 | `liberty` EffectData:1943–1958 | `InterceptorBeamPersistent`: `PeriodCount 1`, periods 0 and 0, `InterceptorBeamDamage` 5 twice on one tick, at `WhichLocation TargetUnit` [DATA] |
| D-8 | `liberty` UnitData:3544–3597 | `Interceptor`: `Speed 7.5`, `Acceleration 1000`, `TurningRate` and `StationaryTurningRate 999.8437`, `Radius 0.25`, `Mover Fly` [DATA] |
| D-9 | `liberty` AbilData:2083–2098 | `CarrierHangar`: `Leash 12`, `Flags Retarget`, `LeashRetarget`. Nothing says what the leash is measured between [DATA] |

**Editor documentation.** SC2Mapster Wiki, *Data/Weapons/Strafe*, read in full as wikitext through the wiki's API
(`https://sc2mapster.fandom.com/api.php?action=parse&page=Data/Weapons/Strafe&prop=wikitext`, saved at
`~/scratch/wp-i0/web/strafe.json`; the HTML page refused the fetch). Verbatim:

- Description: "Weapon that forces the unit to constantly move forwards when attacking and turn around after having
  moved some distance away before attacking again" [EXT].
- **Loiter Inner Radius:** "Distance from the target before the attacking unit starts to turn around" [EXT].
- **Loiter Radius:** "Distance from the target before going in for another round of attack" [EXT].
- Hint: "Works best for ranged flying units with a low collision" [EXT].
- Neither radius says whether it is measured centre to centre or edge to edge. The page has no entry for
  `AllowedMovement`.

**Community sources that could not be read.** The SC2Mapster forum threads on interceptor movement and strafing units
(156120, 163950, 229773) now redirect to CurseForge's front page. web.archive.org cannot be fetched from this machine.
Team Liquid's *Design Club: The Carrier* returned 403, and Liquipedia returned 429 (rate limit) on every page. A web
search's summaries of those pages (not read directly, so they are only pointers) say that modders widen interceptor
strafes by raising both loiter radii, and that Liquipedia describes the leash as how far an interceptor may be "from its
Host Unit". Nothing here rests on those summaries alone except the leash reading in §3, which is marked as such.

**Results against the plan's verify items.**

1. **How the radii are measured: not settled by any source.** The wiki says "distance from the target". The owner's
   Q0057 measures every range edge to edge, and D-S1 follows that. Edge to edge stands [INF, Q0057].
2. **The pass shape: no contradiction of D-S2's path, but a different role for the inner radius.** The wiki and D-S2
   agree on the path: the unit flies forward through the target, goes out, turns, and comes back once it is at the loiter
   radius. They differ on what the inner radius does:
   - **Wiki:** the inner radius is where the outbound unit *starts to turn*, so the turn runs between 2 and 4 beyond the
     target.
   - **D-S2:** the inner radius is the approach trigger that fixes the exit, and the turn happens at 4.

   With our hull's 55.99° a tick at 0.42 a tick (D-8; the catalog row), a reversal sweeps an arc only about 0.5 wide.
   So a turn that started at 2 would end near 2.5 and never reach 4. The wiki's reading would need a banked arc of
   width about `LoiterRadius − LoiterInnerRadius`, which D-S2 deliberately left out. The game's own `TurningRate
   999.8` is effectively instant, so the visible loop in the game is the strafe's, not the hull's.

   This is a **partial contradiction for stop rule I-1**: the observable path agrees, the inner radius's role does not.
   The orchestrator decides whether an amendment records D-S2's use of the inner radius as a minor deviation, or
   replaces it with the wiki's turn-start reading. Recommendation: record it as a deviation. D-S2 uses both data radii
   and needs no arc steering, and with `Range 2` equal to `LoiterInnerRadius 2` its trigger coincides with coming into
   range.
3. **Firing on the move: supported.**
   - The class description says the unit "constantly move[s] forwards when attacking" [EXT].
   - No strafe weapon in any layer sets `AllowedMovement` (D-6). In the data that field is a `CWeaponLegacy` matter.
   - Our `AllowedMovement::Moving` is the field that lets a wind-up keep its unit moving (`sim_types/src/weapon.rs`), so
     D-S4's `allowed_movement: Moving` is the faithful encoding [INF, high].
4. **Damage point and backswing** (carrier plan S11, verify): they are `CWeapon` fields (D-2), so the strafe keeps 3 and
   9 ticks. **Confirmed** [DATA].
5. **New: the random delay is a legacy-class field.** The shipped `saucer_interceptor` beam (both `weapon` and
   `air_weapon`) authors `random_delay_ticks: (-1.1161, 2.2321)`, the `CWeaponLegacy` default (D-3). The carrier plan
   called it "the `CWeapon` default", which is wrong: `CWeapon` has no random delay (D-2), and `InterceptorBeam` is a
   `CWeaponStrafe` (D-4). The 5.0.14 value is therefore (0, 0) [INF from D-2 to D-4]. **Proposed bug row** for the
   orchestrator, number to be assigned: "The interceptor beam draws the legacy weapons' random delay; 5.0.14's
   `CWeaponStrafe` has none." It moves only the census worlds of §2. It fits WP-I1, which already edits the same row
   and re-records the same worlds.

### 2. Census: worlds and tests where a hangar member flies

There is one hangar unit, `mothership_carrier` (`unit_type_id` 32), with member `saucer_interceptor` (47, the highest
air id). A member enters a world only through a Carrier, or through a direct spawn of the member type.

**No measured world carries a Carrier** (re-read on `14417030` with `~/scratch/carrier/census/census.py`, plus
reading scenario loading):

- Its slug appears in none of: `matchup_matrix.ron`, both goldens, `start_loadouts.ron`, `throughput_scenarios.ron`,
  `assets/scenarios/{default_battle,showcase,showcase_effects,skirmish}.ron`, or `crates/sim_fixtures/src/**`.
- Its one trainer is `stellar_portal_dome` (`requires_structures: ["cybernetics_conduit_dome"]`).
- Scenarios spawn loadouts and give initial orders only (`scenario.rs`). No simulation-side AI trains anything, and the
  showcase, showcase-effects and skirmish worlds have no Carrier. That agrees with the carrier plan's WP0 probe, which
  measured zero Carriers in them at schema 56 (evidence `2026-11-28-carrier-interceptors` WP0).
- The skirmish pilot (`client/tests/support/skirmish_pilot.rs`) raises the first card structure that trains an army,
  and panics if that structure is locked. The portal is locked at the start, so the pilot never trains a Carrier. The
  client skirmish stage log is outside the census.
- The sandbox's roles (`sim_fixtures/src/sandbox.rs`) take the first archetype by id that fits each role. The member is
  the last air id, and the goldens re-recorded at schema 57 said "no hangar unit appears".

**Worlds that launch members.** Only these may move in WP-I1 (stop rule I-2):

| World or test | How the Carrier is found | Member weapon |
|---|---|---|
| `crates/sim_core/tests/hangar_tests.rs` H-14 `the_shipped_carrier_launches_four_and_builds_four`, H-15 `the_hangar_is_data_changed_without_recompiling` (`shipped_fight`) | First archetype with a hangar | **Shipped beam** (gains the strafe) |
| `hangar_tests.rs` H-2 to H-11 (`Bay`, authored hangar) | An authored hangar on the first plain air units by id | An **authored** `instant(...)` beam with no strafe. These stay byte-identical under I-A5 |
| `crates/sim_core/benches/throughput.rs`, the `hangar_units: 12` row | First archetype with a hangar | Shipped |
| `crates/client/tests/support/unit_cycling.rs` `stage_hangars`, used by `render_pool_tests.rs` (R-5), `sync_allocation_count_tests.rs` (`hangar_cycle_*`), `sync_steady_state_tests.rs` (the hangar window) | By hangar | Shipped |
| `crates/client/tests/battle_presentation_tests.rs` H-19 | By hangar | Shipped |
| `crates/client/tests/runtime_media_tests.rs` H-16 | Stand-ins with an authored hangar | Authored |

**Tests that use the Carrier's catalog row but fly no member:**

- `weapon_look_tests.rs`, `effects/beams.rs` (the member's cap per player);
- `effect_catalog_tests.rs` (the launch delivery kind);
- `weapon_catalog_rules_tests.rs`, `catalog_reference_values_tests.rs` (the row's values: these see the new
  `strafe` and `allowed_movement` fields and the D-1 values, and are WP-I1's catalog tests);
- `minimap_tests.rs`, `player_view_tests.rs`, `stance_orders_tests.rs` (an unselectable member);
- `broodling_tests.rs`, `death_spawn_tests.rs`, `reuse_stage.rs`, `weapon_battle_gpu_tests.rs` (these exclude hangar
  units or members).

**What a static census cannot close.** D-S2 applies to any unit whose weapon carries a strafe, not only a launched
member. About 116 test files pick archetypes by property, so a test that picks "an armed air unit" by some rule could
take the member or the Carrier. Reading them all was out of WP-I0's no-build scope. Recommendation for WP-I1: put a
scratch counter in the strafe branch for its package run, and list every test that enters it. Any test outside the
table above is either a census amendment (when the strafe is the right behaviour there) or stop rule I-2.

### 3. Finding that changes WP-I1's design: the leash cuts the passes

D-6 of the carrier plan (`hangar/phase.rs` `direct_member`) gives a launched member `Follow` home whenever
`centre_distance(Carrier, member) > leash` (12). It checks this every tick, and nothing makes it sticky. D-S2 clears
`strafe_exit` whenever the front order changes.

The geometry of `shipped_fight` (Carrier radius 1.25, target radius 0.625, member radius 0.25):

- The Carrier stands at its launch range 8, edge to edge, so the target's centre is about 9.9 from the Carrier's centre.
  The probe measured 8.99 to 12.52 over the launched ticks, most of them near 9 to 10.
- A pass's exit lies `4 + 0.25 + 0.625 = 4.875` beyond the target's centre.
- On a pass straight away from the Carrier, the member crosses the leash about 2.1 beyond the target's centre, which is
  an edge separation of about 1.25, inside range 2. That is long before the exit.
- A pass stays inside the leash only if it leaves at more than about 76° to the Carrier-to-target line
  (`9.875² + 4.875² + 2 · 9.875 · 4.875 · cos θ ≤ 144`).
- The first pass after every launch is at θ ≈ 0, because members fly from the Carrier at the target.

As planned, WP-I1 would give a member that flips between `Follow` and `AttackTarget` near the leash circle every few
ticks and that seldom completes a pass. That breaks I-A2 and I-A4, and it breaks D-S3's "the leash unchanged". Hovering
today, a member never comes further than 10.03 from its Carrier (probe), so the conflict is new with the strafe.

In the game, interceptors loop beyond a target while the Carrier holds at range 8. So the 5.0.14 engine either
measures `Leash` from the Carrier to the target, or does not apply it in the middle of a pass. The data does not say
which (D-9). Liquipedia's sentence "the Carrier can move back to a leash range of 12 before the Interceptors disengage"
(carrier plan S19; seen here only in search summaries) reads like Carrier to target.

**Filed as [Q0151](../questions/open/0151-strafe-passes-and-the-carrier-leash.md).** Its default is (b): a member with
an exit set is not leashed until it reaches the exit, and between passes the leash is checked as today. This needs an
amendment to D-S3 before WP-I1, and WP-I1 then also edits `hangar/phase.rs`.

### 4. Baseline, hovering

**Per-member strike intervals** come from a scratch probe, `~/scratch/wp-i0/tree/crates/sim_core/tests/wp_i0_probe.rs`,
on a `git archive` copy of `14417030` (`~/scratch/wp-i0/tree`). It runs the `shipped_fight` scene of H-14/H-15 for
1,000 ticks and records every `BeamVisual` whose `ends.attacker` is a member. The log is at `~/scratch/wp-i0/probe.log`.

| Members (in launch order) | First strike tick | Strike events | Intervals (ticks) | Ticks at zero velocity |
|---|---:|---:|---|---:|
| 4 start members | 56, 66, 71, 87 | 18, 17, 17, 17 | 53 to 77 | 546 to 570 |
| 3 built members | 674, 681, 687 | 6, 6, 6 | 53 to 67 | 214 to 238 |
| 1 built member | 895 | 2 | 57 | 68 |

- All 81 intervals: minimum 53, median 55, mean 56.12, maximum 77. That is the period of 54 plus the random delay of
  -1.1 to +2.2 ticks, with a few re-aims.
- Edge separation at strikes, read after the tick's motion: 1.03 to 2.50.
- The members are still for about 55 % of their launched ticks. This is the hover that B-229 describes.
- WP-I1's predicted 0 to 25 % fall in strike rate is to be read against the 56.12 mean, on WP-I1's own base.

**The hangar throughput row's instructions are not measured here.** The scoping decision:

- WP-I1's base comes after hangar commands, which rewrites the hangar code (plan §8).
- A figure from `14417030` would therefore not be the base WP-I1 is compared with.
- The caller's no-heavy-runs rule also ruled out a release callgrind run today.
- WP-I1 measures the row on its base and its tree in one session, as plan WP-I1's acceptance requires.

For scale, the latest recorded figure is 315 ticks/s at `a7c03026` (evidence
`2026-12-01-death-spawns-and-hangar-commands`, Throughput), with 6.6 launched members a measured tick (`DESIGN.md` §5).
