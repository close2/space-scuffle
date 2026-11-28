# Blueprint: The Waiting Movement Work on `Solver`: Wall Clearance WP2 and WP3, Colossus Walk-Under (B-209), Interceptor Strafing (B-229) and Transport Pickup (B-247), with No Redesign Round

The movement redesign is over. The contact layer alone failed its zig-zag check (Q0147), and the contact layer with
the gated intent (`gated-wp2-eval`) still had three red tests (evidence `2026-12-02-contact-layer-acceptance` §5).
Under the stop rule of the owner's Q0147 answer, today's movement (`MovementPipeline::Solver`) stays shipped. The
owner's order, verbatim: "immediately re-plan the waiting movement work (wall clearance WP2/WP3, Colossus walk-under
`B-209`, Interceptor strafing `B-229`, `B-247`) against today's movement with no further solver redesign rounds."

This plan does that. It re-bases five pieces of work on `Solver`, removes every dependency on `Gated` and the contact
layer, and pre-registers acceptance, stop rules, work packages, file ownership and the build order beside the work
already in flight.

**Status of this document.** A Stage 1 blueprint under charter §6, written on branch `movement-on-solver-plan`, cut
from `origin/main` at `46876ba8` (checksum schema 62). The file carries the date the orchestrator named; the session
date was 2026-11-28. Q0143 and Q0147 are closed by commit `c0d17532` on the local branch `docs-train-3`, which is not
yet on `main`; this plan cites those files by their `closed/` paths, where they will be once that branch lands. Line
numbers drift, so search for the quoted symbol. Claims not confirmed while planning are marked **verify**. Later
decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): …`.

**Why a full plan (charter §6).** Four of the six packages move battle outcomes or golden replays, three change the
checksum format, and the work spans several packages and plans.

**Result in one line.** Of the five pieces, only wall-clearance WP2 and WP3 ever depended on how crowds move. B-209
depended on a crowd fix that will not come now, so it goes to the owner with fresh figures. B-229 and B-247 only
waited for the `Gated` file freeze. B-247 turns out to need no movement file at all.

**Questions.**

| Question | State | Where |
|---|---|---|
| Q0143 (`docs/questions/closed/0143-movement-redesign-ended.md`, on `docs-train-3`) | Closed (a): `Solver` stays; the waiting work is re-planned on it | All |
| Q0147 (`docs/questions/closed/0147-contact-layer-failed-zigzag.md`, on `docs-train-3`) | Closed (a) by its stop rule: no further solver redesign round | §4.1, stop rule G-1 |
| [Q0134](../questions/closed/0134-wall-clearance.md) (a): fix the defects only, judged on the crowd scenes too | Standing | WP-W2, WP-W3, §5.3 |
| [Q0137](../questions/closed/0137-wall-clearance-wp1-mixed.md) (a): WP1 merged with mixed "not worse" figures | Standing, a precedent for Q0148 | WP-W2 |
| [Q0117](../questions/closed/0117-zerglings-under-colossus.md) (b): B-209 parked until friend crowding is fixed | Its condition cannot be met now (§2.2) | WP-C1, Q0149 |
| [Q0130](../questions/closed/0130-interceptors-hover-or-strafe.md) (a): interceptors hover until the movement work ends | Standing until WP-I1 merges | WP-I0, WP-I1 |
| [Q0052](../questions/closed/0052-sanctioned-sc2-deviations.md): sanctioned deviations, D-α to D-δ under `Solver` | Standing; none added or retired here | §4.2 |
| [Q0051](../questions/closed/0051-sc2-mechanics-standing-rule.md): StarCraft II 5.0.14; an unnamed difference is a bug | Standing | §3 |
| [Q0103](../questions/closed/0103-solver-matrix-claims.md), charter §8: natural movement outweighs a close battle's winner | Standing | §4.8 |
| [Q0124](../questions/closed/0124-new-movement-cost-budget.md) (a): at least 50 ticks/s on every benchmark | Standing | §4.7 |
| Owner, 2026-11-20: no fitting to the test set; simpler deviations allowed; survey first | Standing | §3, §4.1 |
| Owner, 2026-11-29: allies nudge attackers only within their band; Hold Position never moves; enemies never push | Standing; `Solver` breaks the first and third today (B-225, B-193) | §4.2 |
| [Q0148](../questions/open/0148-wall-clearance-wp2-on-solver.md): may WP-W2 merge with mixed figures and the zig-zag guards split? | **Filed here**; default (b) | WP-W2, WP-W3 |
| [Q0149](../questions/open/0149-colossus-walk-under-without-crowd-fix.md): B-209 without the crowd fix | **Filed here**; default (b) | WP-C1 |
| [Q0150](../questions/open/0150-remove-latent-gated-pipeline.md): delete the latent `Gated` pipeline? | **Filed here**; default (b) | §4.3 |

---

## 1. Summary

1. **What waited, and on what** (§2.2).
   - **Wall clearance WP2 (B-241)** stopped on two existing zig-zag guards. The re-planned route was right, but it
     passed a friend whom contact turned 13° and back. A fix in contact was forbidden then, and it is a redesign
     now. WP-W2 re-runs the two-line change on today's `main`. The owner decides, in Q0148, whether a contact bump
     on a neighbour counts against it.
   - **Wall clearance WP3 (B-242)** waited only for the `Gated` freeze to end. WP-W3 builds it on `Solver`
     unchanged.
   - **Wall clearance WP4** is not built under Q0134 (a). WP5's bookkeeping folds into WP-R9.
   - **B-209** was parked until friend crowding was fixed (Q0117 (b)). That fix ended with the redesign. WP-C1
     re-measures the parked one-line data change on today's `main`. It merges if every original check holds;
     otherwise the owner decides (Q0149).
   - **B-229** waited for the freeze. WP-I0 checks the sources and WP-I1 builds the strafe as a pursuit stage of
     weapons that carry `CWeaponStrafe`'s loiter radii.
   - **B-247** waited for the freeze too, but needs no movement file. A carrier-initiated load is a task order, which
     `walking_goal`'s existing task arm already walks (WP-T1).
2. **No redesign round** (§4.1). No package may change the contact solver, the intent side rule, `steering.rs`, the
   contact section of `movement_rules.ron` or the push table. A failure whose cause lies there is recorded as a bug
   row and stops the package; it is never tuned away.
3. **Judged under `Solver` only** (§4.3). Arms R (crowd cap 151) and G (`Gated`) are gone. `Legacy` and the latent
   `Gated` keep their tests green and are reported, not judged.
4. **The owner's contact rules as hard checks** (§4.2). `Solver` already breaks two of them (B-225, B-193), so every
   package must not make them worse, and Hold Position stays at zero.
5. **Schema.** `main` is at 62; hangar commands has 63, missiles 64 and diagonal walkability 65 provisionally.
   WP-W3 (`leg_from`), WP-I1 (the strafe's pass state) and WP-T1 (the `LoadUnit` tag) each take the **next free
   number at merge**. WP-W2 and WP-C1 add no hashed state.
6. **Order** (§8). WP-I0 and WP-C1's measurement can start now: neither edits a movement file. WP-W2 starts once
   `docs-train-3` is on `main` and diagonal walkability WP2 has merged, and WP-W3 follows it. WP-T1 starts after hangar commands merges,
   and WP-I1 after hangar commands and missiles WP1. At most two building agents run at once.

---

## 2. Starting point

### 2.1 What ended and what stays on `main`

| Item | State on `main` `46876ba8` | After this plan |
|---|---|---|
| `Solver` (plan `2026-11-20-contact-solver`, `contact.pipeline: Solver`) | Shipped | Shipped, unchanged |
| `Legacy` | Selectable in `movement_rules.ron`, used by tests as a reference | Unchanged; reported only |
| `Gated` (contact layer C′, merged latent at `a5a3df79`) | Latent; `gated` section present; its tests green | Latent unless Q0150 says delete; never judged |
| C-3′ (`contact.crowd_iterations`) | Latent at 21 (equal to `iterations`) | Unchanged |
| Branches `gated-wp2`, `gated-wp2-eval`, `contact-layer-a`, `friend-crowd-plan` | Parked, kept | Untouched |
| The `Gated` file freeze (`2026-11-27-gated-movement` §7) | Ended with Q0143 and Q0147, effective when `docs-train-3` lands on `main` | No package of this plan edits a movement file before that |
| Wall clearance WP0, WP1 (B-240 fixed, B-243 fixed elsewhere) | Merged (`edbac56b`); meter `sim_fixtures::trace::route_clearance`, crowd scenes `trace::crowd` | Reused as the instruments |
| Wall clearance WP2 code | `wall-clearance-plan` `496bd00d` (two predicate swaps), red tests `#[ignore]`d on `main` | Re-applied by WP-W2 |
| Colossus class | Mechanism merged (`6bcb6f8b`); data line parked on `collision-classes-wp3-wip` `d0c67ceb` (head `42761cf2`) | Re-applied by WP-C1 |

Open bugs that stay open under `Solver`, and that no package here may make worse: **B-193** (enemy contact moves
engaged units), **B-219** (dense friend crowds overlap far beyond D-α), **B-220** (a friend pair whose share-1 body
terrain holds is never resolved), **B-225** (allies move attackers beyond D-δ's nudge).

### 2.2 The waiting work and what it really depended on

| Work | Bug | Old dependency | Real dependency on `Solver` | Package |
|---|---|---|---|---|
| Wall clearance WP2: the held leg re-checked | B-241 | Window W between `Gated` packages; parked after two zig-zag guards failed (wall-clearance Amendment 2, item 4: "re-planned after `Gated` WP4 … because Gated's contact layer is what turns the friend") | The bump is `Solver`'s contact correction on a neighbour. It will not be redesigned, so the question is whether it counts (Q0148) | WP-W2 |
| Wall clearance WP3: route-line following, the whole-arc hand-off | B-242 | Window X, after `Gated` WP4 | None beyond the freeze. The aim feeds `Solver`'s intent unchanged | WP-W3 |
| Wall clearance WP4: a wider margin | B-242 | Only under Q0134 (b) | Q0134 was answered (a): not built | — |
| Wall clearance WP5: bookkeeping | — | — | Folded into each package and WP-R9 | WP-R9 |
| Colossus walk-under: the data line | B-209 | Q0117 (b): after the friend-crowd fix (Q0118, C-3′) and the redesign (Q0120) | Both are now off the table. Battle 26's B4, B6 and B7′ failed because the zerglings pack tighter, which is B-219 under `Solver`. That can only be re-measured, then accepted or not by the owner (Q0149) | WP-C1 |
| Interceptor strafe | B-229 | After `Gated` WP4, which owned the movement files (carrier plan D-10) | None beyond the freeze. A pursuit stage for one weapon kind, air only, in a collision class of its own | WP-I0, WP-I1 |
| Transport picks up a unit | B-247 | "Deferred until the Gated movement packages close" (rallies plan WP-D1: "a new `walking_goal` arm") | **None.** `walking_goal` already walks any task order to `actor.task.goal` (`Some(_) if actor.has_task_order() && actor.task.goal.is_some()`), as it walks a passenger's `Load` (`transport/orders.rs` `step_load`, `pursue_carrier`). A carrier-side `LoadUnit` task order needs `has_task_order`, the economy dispatch and a `step_load_unit`, and no movement file | WP-T1 |

### 2.3 What `Solver` is (plan `2026-11-20-contact-solver`, WP6 and Amendments 1 to 21)

- **Intent.** Each unit's preferred velocity is the direction to its next route point at its braked speed, bent by at
  most one time-to-collision side rule against the earliest oncoming or crossing body, with the side kept per pair.
- **Contact.** One position-based solver after every unit moved: Jacobi iterations over contact pairs (`iterations`
  21), shares from the push table, and `settle_on_terrain` as its terrain step, then a hard pass for enemies and
  fixed bodies.
- **The concave.** An attacker blocked by its own line takes a free slot on its weapon-range ring (D-γ). Allies may
  nudge an engaged melee unit along its ring (D-δ).
- **Routes.** Navigation mesh, triangle A*, radius funnel and group lanes. The route-following layer
  (`movement/locomotion.rs`: `drop_passed_waypoints`, `route_is_usable`, `propose_step`, `next_waypoint`,
  `step_toward`) sits between the route and the intent. Wall clearance WP2 and WP3 work only in that layer.

### 2.4 Work in flight, and the files it holds

| Work | Branch or plan | Files that matter here | Schema |
|---|---|---|---|
| Hangar commands (B-230) | `hangar-commands` (plan `2026-12-01-death-spawns-and-hangar-commands` part B) | `hangar/{mod,phase,commands}.rs`, `entity.rs`, `economy/mod.rs`, `world/commands.rs`, `player_view.rs`, `checksum*`, `sim_types/src/{commands,player_view,lib}.rs`, `units.ron`, client HUD and `battle_pipeline/{commands,status}.rs`, goldens | 63 (provisional) |
| Minimap WP3, WP4, WP6 | `minimap-wp3` | Client minimap and HUD, `sim_core/src/world/{presentation,snapshot,vision}.rs`, `player_view.rs`, `sim_types/src/player_view.rs` | None |
| Missile throws and escorts | plan `2026-11-27-missile-throws-and-escorts` (branch `missiles`) | `sim_types/src/weapon.rs`, `combat_resolution/missile_flight.rs`, `world/spawns.rs`, `checksum*`, `units.ron`, `structures.ron`, goldens | 64 (provisional) |
| Diagonal cliff walkability (B-251) | plan `2026-12-02-diagonal-cliff-walkability` | `sim_map` (`diagonal.rs`, `navmesh/mod.rs`, `pathfinding.rs`, `traversal.rs`, `grid.rs`, `validation.rs`), `movement/geometry.rs`, `scenario.rs`, `checksum*`, goldens. Its WP2 moves the showcase worlds (24 cut cells on `showcase_canyon`) | 65 (provisional) |

---

## 3. Sources and the survey

### 3.1 Wall clearance (WP-W2, WP-W3)

The survey of wall-clearance §3 stands: the radius funnel (Demyen and Buro 2006), Recast/Detour's corridor
visibility re-check (`dtPathCorridor::optimizePathVisibility`), Reynolds' path following and pure pursuit (Coulter
1992, CMU-RI-TR-92-01, **verify** the report number). No StarCraft II value changes. B-241 and B-242 are defects of our
own rules (Q0051).

### 3.2 B-209

5.0.14 `Colossus` carries the `Collide` flags `Colossus`, `Structure` and `Flying` only (`liberty` UnitData:4066–4068).
Small ground units therefore walk beneath it and it steps over them, as plan `2026-11-25-collision-classes` §2 sets
out. The data line authors `collision_class: Some("tall_walker")` on the colossus analogue. Structures still meet it
through their footprints (plan `2026-11-18-structure-contact`: structures are no bodies, only the footprint cut). Air
units do not touch it (B-211, Q0114). Wall contact keeps `probe_permits` and the settle (plan
`2026-11-11-contact-and-missiles` D-2, D-3). None of that changes.

### 3.3 B-229: what `CWeaponStrafe` says

Game data of SC2GameData `5444e12f13` ("SC2 5.0.14.94342"), the copy the carrier plan used (S1 there):

| # | Source | What it gives |
|---|---|---|
| T-1 | `core` WeaponData:40–44: `<CWeaponStrafe default="1">` with `LoiterInnerRadius 2`, `LoiterRadius 4` | The strafe's two parameters and their defaults [DATA] |
| T-2 | `liberty` WeaponData:626–637: `CWeaponStrafe id="InterceptorBeam"`, `Range 2`, `Arc 19.6875`, `Period 3`, no loiter field (so the defaults of T-1), `TeleportResetRange 0` | The interceptor's strafe uses loiter radii 4 and 2 [DATA, INF from the defaults] |
| T-3 | `liberty` UnitData:3544–3604: `Interceptor` `Speed 7.5`, `Acceleration 1000`, `TurningRate` and `StationaryTurningRate 999.8437`, `Radius 0.25`, `Mover Fly`, `Collide FlyingEscorts` | It turns almost at once (55.99° a tick in the catalog row) and keeps top speed [DATA] |
| T-4 | `voidmulti` WeaponData:306–337, `balancemulti` WeaponData:279–310: the unused `InterceptorBeamAA` and `AG` variants, also `CWeaponStrafe` | No loiter override in any layer [DATA] |
| T-5 | Carrier plan S11, S19 and D-6: launch at 8, leash 12, members follow the Carrier's target | Unchanged here [DATA, EXT] |

**What the data does not settle (verify in WP-I0).**
- Whether the loiter radii are measured centre to centre or edge to edge. Weapon ranges are edge to edge
  everywhere in this codebase (`within_band`). D-S2 takes edge to edge, so the inner radius 2 coincides with
  `Range 2`, and a pass begins as the member comes into range.
- The exact path. The editor's field names say the strafer flies out to the loiter radius and turns back no nearer
  than the inner radius.
- Whether `CWeaponStrafe` fires on the move. A strafe cannot work otherwise, so D-S4 sets
  `AllowedMovement::Moving` on the interceptor's beam [INF].

WP-I0 reads the editor documentation and community descriptions (Q0008 allows outside sources) before WP-I1 starts.
A source that contradicts D-S2's pass shape is a stop for an amendment (stop rule I-1), not an implementer's choice.

**Survey (owner, 2026-11-20: survey first; simpler deviations allowed).**
- (a) **Loiter-point passes:** fly at the target, and on coming into range fix an exit point on the loiter circle
  straight beyond it; fly there, then turn back. This is a point-goal per pass that reuses the air walk, and it needs
  one hashed point.
- (b) **Orbit strafing** on the loiter circle, a tangent pursuit as in flight-sim AI. It does not fly through the
  target, which T-1's inner radius and the game's look both say it does.
- (c) **Reynolds' "fly-by" seek with overshoot**, steering through the target at speed and letting the turn rate
  decide the loop. Our interceptor turns 56° a tick, so the loop would collapse to a reversal on the spot and the
  loiter radius would play no part.
- **Adopted: (a).** It is the simplest of the three that uses both data radii. It needs no new steering, since the
  member aims at a point as any air walk does. Simplification recorded: the turn back is the hull's turn at its
  authored rate, with no banked arc.

### 3.4 B-247

5.0.14 `CAbilTransport` sets `AllowSmartCmd 1` (`core` AbilData:1192–1204; rallies plan S3): a transport's own smart
command on a unit loads it, and the transport flies to it. Whether the passenger keeps its orders is not in the
data; the rallies plan WP-D1 took "keeps its orders" [INF, **verify**], and so does this plan. WP-T1 checks whether
`CAbilTransport` names a load range (`core` AbilData:1192–1204 and `liberty` AbilData:1478–1498). If it names one
that differs from `BOARDING_REACH` (1.0, edge to edge, `transport/orders.rs`), that is filed as a bug row and not
changed here.

---

## 4. Rules for every package (binding)

### 4.1 No redesign round (owner, Q0147's stop rule)

- **Not edited by any package:** `movement/solver.rs`, `movement/contact.rs`' pair rules and shares,
  `movement/intent.rs`' side rule, `movement/collision.rs`, `steering.rs`, `movement/ring.rs`, the `contact` and
  `gated` sections of `movement_rules.ron`, and the push table.
- `contact.rs`' `probe_permits` may be **called** (WP-W2, WP-W3 do), never changed.
- A package whose failure traces to a cause in those files records a **bug row** with the trace and stops (stop rule
  G-1). It never changes them, and it never adds a constant, tolerance or threshold chosen after a trace (charter
  §1.1; wall-clearance §7 rule 6).
- A single-cause bug fix in those files is still possible under charter §6, as its own short note, if the owner asks
  for it. It is not part of this plan.

### 4.2 The owner's contact rules (hard checks, under `Solver`)

`Solver` does not keep two of them today (B-225, B-193). So they are checked as "no worse than base", except Hold
Position, which `Solver` keeps exactly. They are measured with `sim_fixtures::trace::contact_checks`, the meter
`Gated` WP0b used to measure B-225 under `Solver`. The exclusions are its Amendment 3b ones: spell displacement such
as Abduct in battle 37a, and spawn placement in battle 20.

| ID | Check | Where | Rule |
|---|---|---|---|
| **H-S1** | Hold Position units moved by any body (H-1 holding) | matrix (24 seeds), harness `hold-line`, crowd scenes, showcases | **0**, as on base outside the exclusions |
| **H-S2** | Attackers moved by allies outside their weapon band (B-225's count, H-1b out of band) | matrix, harness `surround-moving` and `enemy-press`, clashes | Not above base beyond noise |
| **H-S3** | Engaged units moved by enemy contact (B-193's figure; H-2) | matrix, harness `enemy-press`, clashes | Not above base beyond noise |

"Beyond noise" means the candidate's per-seed mean lies outside the base's 95 % bootstrap interval of the mean (2,000
resamples, seed 7), on the worse side, as `Gated` §6.1 defines it.

### 4.3 Arms

- **S**, the shipped catalog (`contact.pipeline: Solver`), is the only arm judged.
- **L** (`--pipeline legacy`) is run where the instruments already run it, and reported. Its existing tests must stay
  green.
- **`Gated`** is not played. Its existing tests (`*_under_gated` in `contact_solver_tests.rs` and the rest) must stay
  green.
  - A package that turns one of them red without touching a `Gated` code path reports it. The orchestrator decides
    by amendment whether the test asserted latent behaviour that no longer matters (Q0150).
  - Until Q0150 is answered, the package does not delete or `#[ignore]` the test.
- R and G of `Gated` §6.1, and the `Gated` scripts' arm catalogs `{R,G}-*`, are not used.

### 4.4 Base and re-base

- A measured package's **base** is `main` at the commit its branch is cut from, built from a clean export into a
  fresh target directory.
- If `main` moves simulation code while the package is open, the package merges `main` and re-runs the base's
  instruments once on the merged base. Byte identity of what it measures means no re-measure, as wall clearance did
  for `e5ebc043`. The result is recorded in the evidence.
- A world-moving merge from in-flight work (diagonal WP2, hangar commands, missiles WP2a and WP2b) therefore costs one
  base re-measure, never a re-design.

### 4.5 Seeds

Development seeds are those of wall-clearance §6.4 and Amendment 1, and of collision-classes §7.2. Held-out sets are
registered now and each is run once, only to confirm a direction. Nothing is tuned or widened from them.

| Set | Package | Showcases (both scenarios) | Harness | Matrix (the tool's non-record seeds) | Crowd scenes |
|---|---|---|---|---|---|
| **HB** (wall-clearance §6.4, unused) | WP-W3 | 1,025–1,048 | 9,201,001–9,202,000 | 93rd–116th | HB-crowd 1,009–1,016 |
| **HC** (new) | WP-W2 | 1,049–1,072 | 9,202,001–9,203,000 | 117th–140th | 1,017–1,024 |
| **HD** (new) | WP-C1 | — | — | 141st–164th, every battle | — |

- **Never run here:** `Gated`'s acceptance seeds (300,001–301,000) and held-out sets (9,100,001–9,101,000; the 45th–68th
  matrix seeds), and HA (spent on wall-clearance WP1).
- The collision-classes plan's held-out set (21st–44th) is not reused. Whether another plan read it since is not
  recorded, so HD replaces it.

### 4.6 Goldens and schema

- **One golden re-record per behaviour package.** It covers `golden_replay` and `golden_replay_orders` in native
  debug and release, and is checked on `wasm32-wasip1` release and debug with `wasmtime --dir=<real tree path>`.
  `DEFAULT_SANDBOX_CHECKSUM` is re-recorded only if it moves.
- **The header** names this plan, the WP, the first changed checkpoint, and the entity and tick with what the rule
  did. It ends with the rule-off proof (H-W4).
- **A schema step** (WP-W3, WP-I1, WP-T1) first proves the new state present but unencoded at the old number. Both
  goldens and the sandbox must reproduce on the four builds, 8 of 8 plus the sandbox, as the death-spawns and missile
  plans do. Then the schema takes the **next free number at merge**, re-records, and moves the schema doc line in
  `checksum.rs` and the schema-byte tests.
- Today's provisional order is 63 hangar, 64 missiles, 65 diagonal walkability. The packages here follow in merge
  order. The number is fixed at the merge, never at the branch cut.

### 4.7 Throughput and cost (Q0124 (a))

- **Floor.** Every scenario of `cargo bench -p sim_core --bench throughput` must run at **at least 50 ticks/s under
  S**. The tightest is the canyon, at 80 ticks/s at the solver's WP6 (`DESIGN.md` §5).
- **Measurement.** By `perf stat -e instructions:u`, three runs, under the gate lock. A wall-clock figure is of record
  only at a load below 8, as `Gated` §7 required; otherwise it is projected from instructions against the last
  figure of record.
- Each package states its predicted change before measuring (§7). A measured cost above the prediction is explained
  in the evidence; **the floor binds**.

### 4.8 Battles and claims (charter §8, Q0103)

- **The close-battle list** is `Gated` WP0b's frozen list (evidence `2026-11-27-gated-movement` §3.3): 8, 9b, 10, 11a,
  14, 21, 22a, 26, 27, 28, 31, 32, 33, 35, 36a, 36b, 37a, 37b, 38a, 38b, 39a, 39b, 42. This is Class R, and every other
  battle is Class D. The list is not recomputed.
- **A moved battle** gets a first-difference trace of one moved seed: the tick, the entity and the rule that acted.
- **A flipped winner** follows wall-clearance §6.6:
  - a broken trace (standing with room, oscillation, a wedge entered, a corner cut, a hard check) is a bug and a
    stop;
  - a clean trace in Class R is restated, with a fixture comment citing AGENTS.md §8, Q0103, this plan and the
    mechanism;
  - a clean trace in Class D whose claim is a StarCraft II fact goes to the owner, and the battle carries the
    package's bug as `known_bug` meanwhile;
  - a clean trace in Class D otherwise is restated.
- **Battle 28** keeps `known_bug: Some("B-192")`. A change of its verdict is traced and reported.

### 4.9 Process (charter §4, §7)

- **Tests and gates.**
  - Red tests come first on the package's base, with the red run recorded in the evidence.
  - The implementation agent hands back after `cargo gate quick` and the tests its change needs. A verifier agent
    runs `cargo gate package` on the final tree, and the orchestrator runs `cargo gate merge`.
- **Builds and disk.**
  - Each worktree builds into `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>`.
  - Scratch material lives under `~/scratch/movement-on-solver/<wp>/`.
- **Locks and memory.**
  - Every workspace `cargo test` and every measurement runs under `flock ~/scratch/gate.lock`, one at a time across
    worktrees.
  - Heavy runs start only within the memory headroom of `docs/guides/gpu-and-disk.md`: the slice's anonymous memory
    below about 56 GiB.
  - At most two building agents run at once, counting the in-flight work's.
- **GPU.** WP-T1's client steady-state tests and any capture take the physical GPU per command through `with-gpu-lock`
  and `gpu_run.rs`, never for a session.

---

## 5. Work packages

Sizes: XS under half a day, S at most one day, M two to three days.

### WP-R0: this plan and its records (XS; docs; this commit)

- This plan, its row in `docs/plans/README.md`, and Q0148 to Q0150.
- `DESIGN.md` §4: B-209, B-229, B-241, B-242 and B-247 point to this plan.
- Amendments that point here: wall-clearance Amendment 3, collision-classes Amendment 1, carrier-interceptors
  Amendment 1 and rallies Amendment 2.

### WP-W2: the held leg is the step's too, on `Solver` (S to M; B-241)

**Base.** `main` with `docs-train-3` merged (the freeze lifted) and, preferably, diagonal walkability WP2 merged
(§8). The order follows the diagonal plan's own rule §8.3: if diagonal WP2 has not merged within five working days of
the freeze lifting, WP-W2 goes first, and its evidence hands the diagonal package the list of measured worlds it
moved.

**Code.** Re-apply `496bd00d` (wall-clearance D-2, items 1 and 2):
- `route_is_usable`'s first-leg test `permits_segment(position, first)` becomes `probe_permits(…, position, first)`;
- `propose_step`'s refusal test `!permits_segment(actor.position, next)` becomes `!probe_permits(…, next)`;
- the doc comments of `496bd00d` and `78148bd8` come with them.

Nothing else changes: no constant, cadence or state. The cadence is `ROUTE_CHECK_PERIOD` (8), the re-plan rate
`REPATH_COOLDOWN_TICKS` (12), and the held-leg bound 19 ticks (wall-clearance §4.6).

**Red first.** On the base, the four `route_following_tests.rs` tests now `#[ignore]`d for WP2 lose their `#[ignore]`.
- `a_body_pushed_off_its_leg_replans_before_it_reaches_the_corner` and
  `a_refused_step_whose_leg_the_disc_cannot_walk_drops_the_route` must be red on the base. Their scenes end the
  re-search cooldown first, as on `wall-clearance-plan` (evidence `2026-11-29-wall-clearance` §WP2).
- The narrow-gap and fresh-route guards are green on base and package.

**Acceptance (pre-registered; wall-clearance §6 with the arms of §4.3).**

| Kind | Item | Rule |
|---|---|---|
| Target | M-5b per held leg on the nine showcase seeds of both scenarios and the crowd scenes (seeds 1 to 8, all 18 geometry and size cells) | No held cutting walk leg longer than 19 ticks. The meter's episode across a re-search is reported (D-2 item 5; earlier run: up to 28) |
| Target | The case-A scenes | The two red tests pass |
| Target | Rhino beetle 13 on the default seed, ramp W, ticks 100 to 160 | No M-3 reversal, if the base still shows case A there. A base without case A at that place is recorded, and the target falls back to the per-leg bound |
| Hard | H-W1 | No unit-tick deeper than 2.4 × 10⁻⁵ (wall-clearance §4.6) |
| Hard | H-W2 | The overlap meter's deeper unit-ticks on the nine showcase seeds at most the base's. The plan text said 0, but its base was not 0, and WP1 was read this way; the clarification is recorded here before any new run |
| Hard | H-W3, H-W4, H-W6 | As wall-clearance §6.2: determinism on four builds; the rule off reproduces the base byte for byte; give-ups at most base + 1 per showcase seed, matrix totals at most the base's |
| Hard | H-S1 to H-S3 | §4.2 |
| Hard | The zig-zag guards `solver_pipeline_tests::the_canyon_march_has_no_zig_zag_under_the_solver` and `collision_and_immobility_tests::battlefield_bubble_trajectories_are_smooth_without_micro_jitter` | Green as they stand, **unless Q0148 is answered (a)**; then D-W2a below |
| Not worse | M-1 to M-4 per region on the 33 paired showcase seeds and per crowd cell; route boxes; harness step rule (S, seeds 1 to 1,000, all 13 families and the mirror) | Wall-clearance §6.3 and Amendment 1, item 2: read on the crowd scenes together with the showcases, never on the showcase alone |
| Battles | Matrix (24 seeds), battles 31 and 33, the clashes | §4.8 |
| Held-out | HC, once | No contradiction of a development direction |
| Cost | Prediction | The earlier run measured canyon S +2.71 % instructions (search +0.70 G Ir, about 700 re-searches; `probe_permits` +0.23 G) and the flat rows +0.13 % to +0.45 %. Predicted the same within ±1 percentage point. The floor binds |

**D-W2a: the guards split, only under Q0148 (a).**
- Each of the two guards asserts zero zig-zags of the unit's **own step**: consecutive `SimWorld::proposed_step`
  directions turning more than 8° each way on consecutive ticks, with the guards' existing window, seed, thresholds
  and sign rule unchanged. Achieved-velocity zig-zags whose proposals do not zig-zag are contact bumps. They are
  counted in the test, printed with `eprintln!`, and judged elsewhere: the harness T1 zig-zag figures (M1, M1-hi),
  not worse beyond noise.
- **Why this is no tolerance.** The guards were written for steering zig-zags. The canyon guard's own doc says its
  zig-zag "was traced to the parked-corner aim (L24) reading a body beyond the legacy reach", which is a proposal
  defect. Contact-caused and proposal-caused reversals are already split this way by plan
  `2026-11-19-crowd-edge-steering` D-5.
- **The split must still catch what it was written for.** A scratch build (never committed) reintroduces the L24
  reach (the parked-corner aim reading the side rule's wider neighbour list, **verify** the exact mutation against
  `parked_on_corner`'s reach). The own-step assertion must fail on it. If it does not, D-W2a is void and WP-W2 waits
  as under (b).
- **A residual own-step zig-zag** on a unit WP-W2's rule did not act on is traced. If it comes from steering or the
  side rule, it is a `Solver` bug row and a stop (G-1), not a waiver.

**Expected outcome from the earlier run** (wall-clearance evidence §WP2, on an older base):
- the targets hold, and every showcase battle ends earlier;
- the block is better beyond noise, and the ramp W channel worse (M-2 144 → 161, M-4 26.0 → 29.3);
- deeper unit-ticks 2 → 4 on the nine seeds, which fails H-W2 as clarified;
- the two guards red;
- only battle 40 moves in the matrix, with no flip.

So under the default (b), WP-W2 is likely to stop again. Its figures then go to Q0148 as an addendum, and the owner
may answer (a) without a re-run.

**Under Q0148 (a)** the mixed figures (H-W2 above base, ramp W worse) are recorded with traces and do not stop the
package, as Q0137 (a) did for WP1. H-W1 (the depth bound), H-W3, H-W4, H-W6, H-S1 to H-S3, the targets, the harness
step rule and the battle rules still bind.

**Records.** Golden re-record (§4.6; no schema). Evidence `docs/evidence/2026-11-29-wall-clearance.md` §WP2 on
`Solver`. B-241 fixed, or open with the figures.

### WP-W3: route-line following near walls, the whole-arc hand-off (M; B-242)

**Base.** `main` after WP-W2 merges, or after WP-W2 stops (WP3 needs neither WP1 nor WP2, wall-clearance §7
rule 6). Base figures: the meter on 33 seeds, the crowd scenes, the harness, the matrix and the cost attribution, run
once.

**Design.** Wall-clearance D-3 unchanged:
- `MovementMemory::leg_from: Option<SimVec2>` (world units, hashed), set in `adopt_route` and `pass_waypoint`, and
  cleared in `drop_route` and `finish`;
- a new module `movement/route_line.rs` holds the projection and the look-ahead;
- the look-ahead `L` is this tick's step budget, which never overshoots the line (wall-clearance §4.6), triggered only
  while the body's clearance is below `CORNER_SAFETY_MARGIN`;
- `drop_passed_waypoints` hands an arc run off only as a whole, at `min(r + ARC_CHORD_DIP, c(p) − WALL_PROBE_SLACK)`.

The aim feeds `step_toward` and `Solver`'s intent exactly as a waypoint does today. No intent, contact or steering
code changes (§4.1).

**Red first.** Wall-clearance §8 WP3's list:
- the `route_line.rs` unit tests, including no-overshoot over a grid of `d` and `s`;
- `a_body_pushed_onto_a_wall_returns_to_its_route_line`;
- `an_arc_is_handed_off_only_as_a_whole_and_never_at_less_than_the_funnels_clearance`;
- `a_body_in_open_ground_aims_at_its_waypoint_as_before`;
- `a_rounding_held_by_a_parked_body_keeps_its_arc`;
- `leg_from`'s encode and decode test in `checksum_state_tests.rs`.

**Acceptance.** Wall-clearance §6.3 WP3, under S:
- **Directional claims.** In the central block and both ramp channels, M-2 and the longest episode fall beyond noise
  on the 33 paired showcase seeds **and** per geometry on the crowd scenes' development seeds. M-1 does not rise.
  M-5a stays at its decision-time count of 0, and M-5b within 19 ticks.
- **Hard checks.** H-W1 to H-W4, H-W6 and H-S1 to H-S3.
- **The zig-zag guards** as WP-W2 states them, under Q0148's answer.
- **The rest.** Not worse beyond noise as WP-W2; the battle rules of §4.8; HB run once.
- **Schema.** Next free at merge, with §4.6's proof.
- **Cost.** One `wall_contact` at `r + m` per walking unit-tick: about +0.4 % on the canyon (wall-clearance evidence,
  cost attribution), unless shared with the settle's query under a `// PERFORMANCE RATIONALE:` block. The floor
  binds.

**Stop.** Wall-clearance §7 rules 1 to 8 and 10, with §4.1 here in place of rule 9 (sequencing). WP3 failing twice
under Q0134 (a) sends the figures to the owner as a Q0134 addendum (rule 8). A wider margin (WP4) or wall-aware
steering is then the owner's call, judged first on the crowd scenes (wall-clearance Amendment 1, item 3).

**Records.** Golden re-record. Evidence §WP3. B-242 closed under Q0134 (a)'s wording ("units still touch walls
briefly in a crowd"), or open with the figures. The addendum to plan `2026-11-09-sc2-pathfinding` (Amendment 3,
item 2a's lead closed).

### WP-C1: the colossus walks over small ground units, on `Solver` (S; data; B-209)

**Base.** Today's `main`, which ships `Solver`. No movement file is touched, so the package does not wait for
`docs-train-3`. A later merge to `main` that moves battle 26 or a colossus world triggers §4.4's re-measure.

**Change.** Re-apply `d0c67ceb`:
- the colossus analogue's `collision_class: Some("tall_walker")` and its comment in `assets/catalog/units.ron`;
- the tests A-11 and A-12 in `crates/sim_core/tests/collision_class_tests.rs`.

The data line was reverted from `main` after WP3 parked. If the merge with in-flight `units.ron` rows conflicts, it
is resolved by hand.

**Measure first, merge second.** The checks are the collision-classes plan's, unchanged:
- **B1 to B9** (§7.2), against the fresh base on 24 seeds, with the same probes (`~/scratch/collision-classes/` (a)
  and (b); the scripts are reused by path);
- **one cause** (stop rule 4): only worlds with a colossus analogue move, and each first difference is a pair, a
  steering neighbour or a ring blocker removed between it and a ground body;
- **battle 26** is Class R. It is restated if it moves with the checks holding, citing AGENTS.md §8, Q0103, this
  plan and B-209;
- **every other world** stays byte-identical. That covers the harness (no colossus), battles 31 and 33, the
  showcases, the goldens and the sandbox (§7.5 there);
- **H-S1 to H-S3**;
- **HD** run once at the end;
- **throughput** as collision-classes §11: within its budget, and the floor binds.

**Merge rule.**
- **Every check holds:** WP-C1 merges. That is Q0117 (b)'s own condition, switching on cleanly without a crowding
  regression.
- **B4, B6 or B7′ fail again as on 2026-11-25** (prediction: they do, since B-219 is unchanged): WP-C1 stays parked
  on its branch with the figures, and Q0149 decides.
  - **Under Q0149 (a)**, B4, B6 and B7′ are reported. Each needs a trace showing its cause is friend packing among
    the zerglings beneath the colossi, which is B-219. B-219's row gains the battle 26 figure.
  - B1 to B3, B5, B8 (standing while able to fight) and B9 (no cross-class correction) still bind.
  - B8 or B9 failing is broken behaviour and a stop under every answer.

**Records.** B-209 fixed (WP-C1's commit), or open with the figures. The collision-classes plan's `## As built` WP3
row. The evidence section `docs/evidence/2026-11-25-collision-classes.md` §5, "On `Solver` after the redesign".

### WP-I0: strafe sources and census (XS; docs and scratch; can start now)

1. **Sources.** The verify items of §3.3: the editor documentation's descriptions of `LoiterRadius` and
   `LoiterInnerRadius`, how they are measured, the pass shape, and firing on the move. The outcome is recorded in
   the evidence `docs/evidence/2026-12-03-interceptor-strafe.md` (new). A contradiction of D-S2 stops for an
   amendment before WP-I1 (stop rule I-1).
2. **Census.** Every measured world and test that launches a hangar member: `hangar_tests.rs`, the throughput hangar
   row, the client skirmish stage log, and the showcases (**verify**: no Carrier analogue). Only these may move in
   WP-I1.
3. **Baseline.** The hangar row's instructions, and per-member strike intervals in `hangar_tests.rs`' scenes,
   hovering.

### WP-I1: interceptors strafe (M; `sim_types`, `sim_core`, data; B-229)

**Base.** `main` after hangar commands (`hangar/`, `entity.rs`, `units.ron`, `checksum*`) and missiles WP1
(`sim_types/src/weapon.rs`, `checksum*`) have merged, and after WP-I0.

**Design.**
- **D-S1. Data.** `WeaponSpec::strafe: Option<StrafeSpec>`, serde default `None`, where
  `StrafeSpec { loiter_radius: SimScalar, loiter_inner_radius: SimScalar }` is in world units, measured edge to edge
  as weapon ranges are.
  - Validation, with a typed catalog error: `0 < loiter_inner_radius ≤ loiter_radius`, and `strafe` only on a weapon
    whose unit is `MoverType::Air` (a ground strafe would need routes, which nothing in 5.0.14's shipped data asks
    for).
  - The `saucer_interceptor` beam gains `strafe: Some((loiter_radius: 4.0, loiter_inner_radius: 2.0))`, citing T-1
    and T-2, and `allowed_movement: Moving` (D-S4).
- **D-S2. The pass.** While the front order is `AttackTarget { target }` and the weapon against the target carries a
  strafe, the pursuit stage hands the motion to `movement/strafe.rs` (new) instead of `Pursuit::aim_if_in_range`.
  That module keeps one hashed field, `MovementMemory::strafe_exit: Option<SimVec2>`:
  - **No exit:** the goal is the target's tick-start position (`pursued_motion`). When the edge separation falls to
    at most `loiter_inner_radius`, the exit is fixed at `t + u · (loiter_radius + r_m + r_t)`, where `u` is the unit
    direction from member to target at that tick (`movement/math.rs` `direction`; the member's heading if the two
    centres coincide).
  - **With an exit:** the goal is the exit. The pass ends when the member reaches it within this tick's step, and the
    exit is cleared. The next tick aims at the target again.
  - **Speed:** top speed throughout, with no braking onto a strafe goal. The turn is the hull's authored rate through
    the existing facing gate.
  - **Ending:** the field is cleared whenever the front order changes, as on a leash return, a dock or the target's
    death (D-6 of the carrier plan unchanged), and whenever the target is no longer valid.
- **D-S3. What does not change.** The leash, return, dock, launch and death rules (carrier plan D-6 to D-9), the
  `flying_escorts` class (members touch only members), acquisition, and every unit whose weapon has no strafe. The
  air walk is the one every air unit uses, so no route, steering, intent or contact code changes.
- **D-S4. Firing.** A strafe can only fire on the move, so the beam's `allowed_movement` is `Moving` [INF, **verify**
  in WP-I0]. The weapon phase's band and engaged-arc tests are unchanged. A member fires while it closes on the
  target within its band and arc, and holds fire while it flies away.

**Red first** (`crates/sim_core/tests/strafe_tests.rs`, new; unit tests in `movement/strafe.rs`):
- **I-A1.** A launched member attacking a still target never has zero velocity for more than
  ⌈180° ÷ turn rate⌉ = 4 consecutive ticks while its target is valid.
- **I-A2.** Over 1,000 ticks against a still target, its edge separation alternates between at most
  `loiter_inner_radius` and at least `loiter_radius − one step`. At least ⌊1,000 ÷ T_pass⌋ − 1 passes are counted,
  where `T_pass = ⌈2 · (loiter_radius + r_m + r_t) ÷ speed⌉ + 4` is derived and stated in the test.
- **I-A3.** Every strike happens with the target within the weapon's band and engaged arc.
- **I-A4.** No two consecutive strikes are more than `period + T_pass` ticks apart against a still target: a ready
  weapon fires on the next pass.
- **I-A5.** A weapon without a strafe behaves byte for byte as before (the existing hangar tests run with the field
  `None`).
- **I-A6.** `strafe_exit` is encoded and decoded, and it is cleared on an order change and on leash.
- **Catalog.** `StrafeSpec` validation errors (typed), RON round trip, and `runtime_catalog_tests.rs` (the strafe is
  authored data).

**Acceptance.**
- I-A1 to I-A6 and the existing `hangar_tests.rs` green.
- Every world of WP-I0's census outside the hangar tests and rows is byte-identical. That covers the matrix,
  harness, showcases and goldens, apart from the schema step's encoding.
- The schema step with §4.6's proof.
- The hangar throughput row's instructions change by at most the measured share of member motion. It is predicted
  under +1 %: about 6.6 launched members a tick, each one more goal computation. The floor binds.
- **The strike rate is reported, not judged.** It is predicted to fall by 0 to 25 % against hovering, because a ready
  weapon may wait for the next pass. That is the game's behaviour, if D-S2 is its pass.

**Records.** B-229 fixed. Q0130's resolution gains a line that the strafe was built. Carrier plan `## As built` row.
Evidence.

### WP-T1: a transport picks up its own ground unit (M; `sim_types`, `sim_core`, `client`; B-247)

The rallies plan's WP-D1, re-planned against today's code. It needs **no movement file**.

**Base.** `main` after hangar commands merges, since both edit `sim_types/src/commands.rs`, `entity.rs`,
`economy/mod.rs`, `world/commands.rs`, `checksum*` and the client's `battle_pipeline/commands.rs`.

**Design.**
- **The order.** `UnitOrder::LoadUnit { passenger: EntityId }`, appended after the last variant so every existing tag
  keeps its number. It is the carrier's `Load` with a unit target (S3 of the rallies plan).
- **Admission** (`world/commands.rs`, typed `TransportAdmissionError` rows):
  - a completed, mobile carrier of the player with free capacity for the passenger's slots;
  - a live, unloaded, uncarried, own ground passenger that `admit_load` would accept for this carrier.
- **The walk is a task.**
  - `Entity::has_task_order` gains `LoadUnit`, and the economy's task dispatch calls
    `step_load_unit(carrier, passenger)` (`transport/orders.rs`). That step mirrors `step_load`: it ends the task if
    the passenger is gone, loaded or another's; it loads through `load_transport_prevalidated(passenger, carrier)`
    within `radii + BOARDING_REACH`; it ends on a full carrier ("remove the move command", rallies S6).
  - Otherwise it keeps the carrier's task goal on the passenger, as `pursue_carrier` does the other way round: it
    re-aims when the passenger moves more than `BOARDING_REACH` from the goal. An air carrier aims at the passenger's
    position, clamped into its domain. A ground carrier, which nothing ships, aims at `approach_cell`, as
    `pursue_carrier` does. Both are tested with a test-catalog edit.
  - `walking_goal`'s existing arm (`Some(_) if actor.has_task_order() && actor.task.goal.is_some()`) walks it. **No
    file under `movement/` is edited.** The commit message lists `git diff --stat` to show it, and a needed movement
    edit is stop rule T-1.
- **The passenger keeps its orders** [INF, **verify**, §3.4].
- **The right click** (`client` `battle_pipeline/commands.rs`, `smart_plan`'s friendly rows): a carrier member
  right-clicking an own ground unit takes `LoadUnit` before `Follow { Move }`. That follows rallies S3 and S5: after
  `MedivacHeal`, which does not exist here.
- **Schema.** The new tag is hashed: next free at merge, with §4.6's proof. Unreachable in the goldens, so every
  checkpoint reproduces with the tag unencoded.

**Red first** (`crates/sim_core/tests/transport_pickup_tests.rs`, new):
- each admission refusal;
- pickup of a standing passenger and of a walking one;
- a full carrier ending the order;
- a passenger that dies or boards another carrier meanwhile;
- the ground-carrier path under a test catalog;
- a queued `LoadUnit` after a `Move`.

On the client side: the right-click row (`battle_context_order_tests.rs`), and a P-6-style steady-state variant
(`sync_steady_state_tests.rs`) and valgrind pair (`sync_allocation_count_tests.rs`, run exclusive) with a pickup in
flight.

**Acceptance.**
- The tests above green.
- Every measured world byte-identical apart from the encoding, since no scenario issues `LoadUnit`; the client
  skirmish stage log identical.
- Throughput unaffected, run once within 3 % of instructions (rallies plan P-8's method).

**Records.** B-247 fixed. Rallies plan `## As built`: WP-D1 done here. Its README row, `Partly done` → `Done` if
nothing else remains.

### WP-R9: bookkeeping (XS)

- This plan's `## As built`, its README row, and `DESIGN.md` §5's throughput figures if any row moved.
- The showcase storyboard (`assets/captures/showcase.ron`) is re-checked only if a scene's frame moved. A render
  takes the GPU lock per capture (`docs/guides/gpu-and-disk.md`).
- Q0148 to Q0150 resolutions carried out and moved to `closed/` as they are answered.

---

## 6. File ownership

Files outside a package's row are not edited by it. "After" names the in-flight work that must merge first because
it holds the same file.

| Package | Files | After |
|---|---|---|
| WP-R0 | this plan, `docs/plans/README.md`, `DESIGN.md` §4 (five rows), Q0148 to Q0150, the four plans' amendments | — |
| WP-W2 | `crates/sim_core/src/movement/locomotion.rs` (`route_is_usable`, `propose_step`, their docs), `crates/sim_core/tests/route_following_tests.rs`, under Q0148 (a) also `solver_pipeline_tests.rs` and `collision_and_immobility_tests.rs` (the two guards only), goldens, `DEFAULT_SANDBOX_CHECKSUM` if moved, wall-clearance evidence, `DESIGN.md` §4 | `docs-train-3`; diagonal WP2 (goldens, showcase worlds) by §8 |
| WP-W3 | `locomotion.rs` (`next_waypoint`, `drop_passed_waypoints`, `MotionProposal::{adopt_route, pass_waypoint, drop_route, finish}`), `movement/route_line.rs` (new), `movement/mod.rs` (module line), `entity.rs` (`MovementMemory::leg_from`), `checksum.rs`, `checksum/encoding.rs`, `crates/sim_core/tests/{route_following_tests,checksum_state_tests,golden_replay_tests}.rs`, goldens, sandbox, evidence, `DESIGN.md` §4 and §5, plan `2026-11-09-sc2-pathfinding` (addendum) | WP-W2 (same functions' neighbours, goldens); hangar commands and missiles WP1 if they hold `entity.rs` or `checksum*` open at the same time (merge one at a time) |
| WP-C1 | `assets/catalog/units.ron` (the colossus row only), `crates/sim_core/tests/collision_class_tests.rs`, `crates/sim_core/tests/fixtures/matchup_matrix.ron` (battle 26's restatement only), collision-classes evidence and plan, `DESIGN.md` §4 | Nothing to start. `units.ron` rows of hangar commands and missiles WP2a/2b merge by hand; battle 26 has none of their units |
| WP-I0 | evidence `2026-12-03-interceptor-strafe.md` (new), scratch | — |
| WP-I1 | `crates/sim_types/src/weapon.rs` (`StrafeSpec`, the field), `crates/sim_core/src/catalog/strike_validation.rs` (validation; **verify** the right validation module), `movement/strafe.rs` (new), `movement/mod.rs` (module line), `locomotion.rs` (the pursuit hand-off at `aim_if_in_range`'s call site only), `entity.rs` (`MovementMemory::strafe_exit`), `checksum*`, `assets/catalog/units.ron` (the interceptor beam), `crates/sim_core/tests/{strafe_tests (new),hangar_tests,runtime_catalog_tests,catalog_reference_values_tests,checksum_state_tests}.rs`, goldens, evidence, `DESIGN.md` §4 and §5 | Hangar commands (`entity.rs`, `units.ron`, `checksum*`, `hangar_tests.rs`); missiles WP1 (`weapon.rs`, `checksum*`); WP-W3 if both touch `locomotion.rs` at once (different functions; merge one at a time) |
| WP-T1 | `crates/sim_types/src/commands.rs`, `crates/sim_core/src/entity.rs` (`has_task_order`), `economy/mod.rs` (dispatch), `transport/orders.rs` (`step_load_unit`), `world/commands.rs` (admission), `checksum*`, `crates/client/src/battle_pipeline/commands.rs` (the row), `crates/sim_core/tests/transport_pickup_tests.rs` (new), client tests named above, goldens, evidence of the rallies plan, `DESIGN.md` §4 | Hangar commands (same `sim_types`, `entity.rs`, `economy/mod.rs`, `world/commands.rs`, client `commands.rs`, `checksum*`) |
| WP-R9 | records only | Last |

**Not touched by any package:** the files of §4.1, `sim_map` (diagonal walkability's), the client minimap and HUD
(minimap's), `hangar/` (hangar commands'), `combat_resolution/missile_flight.rs` and `world/spawns.rs` (missiles').

---

## 7. Determinism, checksum schema and throughput

| Package | New computation | Hashed state | Schema | Worlds that move | Cost predicted |
|---|---|---|---|---|---|
| WP-W2 | `probe_permits` in two more places (fixed point, existing predicate) | None | Unchanged | Showcases, crowd scenes, matrix battles with corners (earlier: battle 40 only), goldens | Canyon about +2.7 % instructions; flat rows under +0.5 % |
| WP-W3 | Projection and look-ahead (`movement/math.rs` checked operations); one `wall_contact` | `MovementMemory::leg_from: Option<SimVec2>` | Next free at merge | As WP-W2, plus walking near walls | About +0.4 % on the canyon |
| WP-C1 | None (data) | None | Unchanged | Battle 26 (and 12a/12b by steering only, collision-classes §7.5) | Within collision-classes §11's budget |
| WP-I1 | One goal per launched member per tick | `MovementMemory::strafe_exit: Option<SimVec2>`; `WeaponSpec::strafe` where the weapon spec is encoded (**verify** whether `ActiveProjectile`'s or any hashed copy carries it) | Next free at merge | Hangar tests and row only | Under +1 % on the hangar row |
| WP-T1 | One task step per `LoadUnit` carrier | The `UnitOrder::LoadUnit` tag | Next free at merge | None | None measurable |

- Integer fixed point throughout. Floats appear only in reporting (the meters in `sim_fixtures`), in the guards'
  measurement under D-W2a, and in the client.
- No `HashMap`, no clock, no new randomness. Iteration orders are unchanged.
- The determinism jobs (Linux, Windows, macOS ARM64, `wasm32-wasip1`, debug and release) cover each re-recorded
  golden. The simulation crates still build for `wasm32-unknown-unknown`, and no package adds a dependency.
- The throughput job's floor (`--floor 25`) and Q0124's 50 ticks/s are met with room: the canyon runs at 80 ticks/s.

---

## 8. Build order and parallel work

**Gate events.**
- **E1.** `docs-train-3` lands on `main`: Q0143 and Q0147 are closed, and the freeze is lifted.
- **E2.** Diagonal walkability WP2 merges, moving the showcase worlds.
- **E3.** Hangar commands merges.
- **E4.** Missiles WP1 merges.

| Step | Package | Starts when | Builds alongside | Merges |
|---|---|---|---|---|
| 0 | WP-R0 | Now | Anything | With this branch |
| 1 | WP-I0 | Now (docs and scratch, no build) | Anything | Its evidence file with WP-I1, or alone |
| 2 | WP-C1 measurement | Now | Hangar commands and minimap (no shared file). It is a building agent, so it counts towards the two | Only under its merge rule (§5) or Q0149's answer |
| 3 | WP-W2 | After E1 and E2 (or E1 plus five working days, §5) | Minimap; hangar commands; missiles. The goldens merge one at a time | After its acceptance (default (b)), or Q0148 (a) |
| 4 | WP-T1 | After E3 | WP-W2 or WP-W3 (no shared file but `checksum*`; schema merges one at a time) | After its tests; schema next free |
| 5 | WP-W3 | After WP-W2 merges or stops | WP-T1 or WP-I1, within the two-agent cap | After its acceptance |
| 6 | WP-I1 | After E3, E4 and WP-I0 | WP-W3 or WP-T1 | After its tests; schema next free |
| 7 | WP-R9 | After the last merge | — | Last |

- **Merges.** Merge one branch at a time, `cargo gate merge <branch>` per merge, and test a tree once (charter §4).
  A merge to `main` that moves a world another open package measures triggers §4.4's single re-measure there.
- **Questions.** Nothing blocks on Q0148 to Q0150. Each package is built and measured on its default, so an answer
  needs no re-run.

---

## 9. Stop rules (fixed in advance)

**Every package.**
- **G-1.** A failure whose cause lies in a file of §4.1 means a bug row with the trace, then stop. No change to those
  files.
- **G-2.** A hard check (§4.2, H-W1 to H-W6 as they apply) fails. H-W4 failing is a wiring defect to find and fix; it
  does not count as an amendment.
- **G-3.** Throughput below 50 ticks/s in any scenario under S: stop and report the attribution.
- **G-4.** One amendment per package. It must name a literature source or an earlier measured rule of this project.
  No constant, tolerance, hysteresis or tie-break may be chosen after a trace. A second failure ends the package; its
  bug stays open with the figures.
- **G-5.** A held-out run contradicts a development direction, or a restated claim fails two thirds of the held-out
  matrix seeds: stop.
- **G-6.** A flipped winner with a broken trace (§4.8): a bug and a stop.
- **G-7.** Not allowed in any package: a per-unit, per-family, per-battle or per-map switch; a rule fitted to the
  showcase (regions are reporting boxes); code that names a unit (charter §3.3).

**Per package.**
- **W-1.** The rules of wall-clearance §7, as §5 maps them.
- **C-1.** Collision-classes §8 rules 4 to 10. Battle 28 (B-192) moving at all is a stop.
- **I-1.** WP-I0 finds a source that contradicts D-S2's pass or D-S1's measure: stop for an amendment before WP-I1.
- **I-2.** Any world outside WP-I0's census moves in WP-I1.
- **I-3.** WP-I1 needs an edit outside its row of §6 in `movement/`.
- **T-1.** WP-T1 needs any file under `movement/`: stop and re-plan.
- **T-2.** A golden checkpoint differs with `LoadUnit`'s tag unencoded at the old schema.

---

## 10. Records

### 10.1 Bug rows (`DESIGN.md` §4, edited in WP-R0)

| Row | Edit |
|---|---|
| B-209 | Status: open; the precondition of Q0117 (b) is gone with the redesign; re-planned here (WP-C1), merges only if B1 to B9 hold on `Solver`, otherwise Q0149 |
| B-229 | Status: deferred until WP-I1 of this plan (no longer waits for the Gated plan, which ended); builds after hangar commands and missiles WP1; Q0130 (a) meanwhile |
| B-241 | Fix: this plan's WP-W2 on `Solver`; Q0148 |
| B-242 | Fix: this plan's WP-W3 on `Solver` |
| B-247 | Status: open; needs no movement change (a task order `walking_goal` already walks); planned by this plan's WP-T1, after hangar commands |

No new row is filed now. New rows take the next free number at their merge. The highest on `main` and every open
branch is B-252.

### 10.2 Questions filed

- [Q0148](../questions/open/0148-wall-clearance-wp2-on-solver.md): may WP-W2 merge with mixed figures and the
  zig-zag guards split by cause? Default (b): the original rules.
- [Q0149](../questions/open/0149-colossus-walk-under-without-crowd-fix.md): B-209 without the crowd fix. Default (b):
  re-measure, merge only if clean.
- [Q0150](../questions/open/0150-remove-latent-gated-pipeline.md): delete the latent `Gated` pipeline. Default (b):
  keep it latent, with its tests green.

### 10.3 Amendments to other plans (WP-R0)

- `2026-11-29-wall-clearance` Amendment 3: §5 and window X are superseded. WP2, WP3 and WP5 are re-planned here, and
  WP4 is not built under Q0134 (a).
- `2026-11-25-collision-classes` Amendment 1: WP3 and WP4 are re-planned here (WP-C1, HD).
- `2026-11-28-carrier-interceptors` Amendment 1: D-10's "after Gated WP4" becomes this plan's WP-I1.
- `2026-12-01-rallies-and-smart-command` Amendment 2: WP-D1 is re-planned here (WP-T1) with no movement file.

---

## 11. Risks

| ID | Risk | Mitigation |
|---|---|---|
| R-1 | WP-W2 stops again on the guards and H-W2, and B-241 stays open indefinitely | Q0148 is filed with the earlier figures; WP-W3 does not wait for WP-W2 |
| R-2 | WP-W3 draws walking crowds near a wall onto converging route lines (wall-clearance R-2), and contact bumps rise under `Solver` | The trigger confines it to bodies within the margin; T1 and the crowd scenes are judged; G-4, then the Q0134 addendum |
| R-3 | B-209 stays parked forever, since B-219 will not be fixed under `Solver` | Q0149 puts the trade to the owner with today's figures |
| R-4 | D-S2's pass is not the game's | WP-I0 checks the sources first (I-1); the strike rate is reported |
| R-5 | A latent `Gated` test turns red under a route-following change | §4.3: report, do not delete; Q0150 |
| R-6 | In-flight merges move the bases repeatedly | §4.4's single re-measure per merge; the merge order of §8 |
| R-7 | Schema numbers collide across five schema packages | Next free at merge, the four-build proof before each number is taken |

## 12. Out of scope (charter §8: scoping recorded)

- **B-193, B-219, B-220 and B-225.** They are `Solver`'s own contact defects, and their fix is a solver change (§4.1).
  They stay open. Their figures are guarded by H-S2 and H-S3, never worsened.
- **Q0134 (b) and (c)** (WP4, wall-aware steering). Only via WP-W3's stop rule 8 and the owner.
- **B-211** (a colossus that touches air units, Q0114) and **B-213** (cliff movers): unchanged.
- **The Carrier upgrades** (Graviton Catapult, leash range; carrier plan §14): no catalog rows.
- **The convex cliff corners** (B-250, Q0146): diagonal walkability's.
- **Deleting `Legacy`.** It is the reference arm of many tests; Q0150 asks only about `Gated`.

## 13. Material (scratch, not committed)

`~/scratch/movement-on-solver/`:
- `w2/`, `w3/`, `c1/`, `i0/`, `i1/`, `t1/`, `heldout/`;
- `scripts/`, which reuses by path the scripts of `~/scratch/wall-clearance/scripts/`, `~/scratch/gated-movement/
  scripts/` and `~/scratch/collision-classes/`, and does not copy them.

Branches kept: `wall-clearance-plan` (WP2 code `496bd00d`), `collision-classes-wp3-wip` (`d0c67ceb`, `42761cf2`),
`gated-wp2`, `gated-wp2-eval`.

## Amendment 1 (orchestrator, 2026-11-28): WP-I0's findings, before WP-I1

From `docs/evidence/2026-12-03-interceptor-strafe.md` (WP-I0, branch `wp-i0` 45188c61). Binding on WP-I1.

1. **Stop rule I-1, pass shape.** The SC2Mapster *Data/Weapons/Strafe* page gives `LoiterInnerRadius` the job "distance
   from the target before the attacking unit starts to turn around"; D-S2 uses it as the approach trigger that fixes
   the exit point. The path (through the target, out to the loiter radius, back) agrees. D-S2 stands unchanged and its
   use of the inner radius is recorded as a **minor deviation** in DESIGN.md §4 with B-229's fix; a banked turn
   between 2 and 4 beyond the target is out of scope (charter §8: minor details). I-1 is satisfied by this amendment.
2. **D-S3, the leash (Q0151).** As planned, the 12-unit leash would cut most passes (the first pass after every launch
   included) and fail I-A2 and I-A4. Until the owner answers Q0151, WP-I1 builds Q0151's **default (b)**: a member
   whose `strafe_exit` is set is not leashed until it reaches the exit; between passes the leash acts as today. The
   choice lives in one function so that (a) is a small change. WP-I1's §6 row gains
   `crates/sim_core/src/hangar/phase.rs` (the leash check only). D-S3's "leash unchanged" reads "leash unchanged
   between passes".
3. **B-254, strafe random delay.** `InterceptorBeam`'s `random_delay_ticks: (-1.1161, 2.2321)` is the `CWeaponLegacy`
   default, which `CWeaponStrafe` does not inherit; 5.0.14's value is (0, 0). Fixed inside WP-I1 (a data line in
   `units.ron`); it moves only WP-I0's census worlds.
4. **Census gap.** D-S2 applies to any unit whose weapon has a strafe. During its package run WP-I1 adds a scratch
   counter (not committed) in the strafe branch and lists every test that enters it; any test outside the census is
   stop rule I-2.
5. **Radii measure.** No source says; edge to edge stays (Q0057).
