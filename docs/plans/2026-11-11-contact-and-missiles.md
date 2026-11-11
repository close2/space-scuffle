# Blueprint: Disc Contact with Terrain (B-113) and Homing Missile Impact (B-115)

Two recorded bugs, both fixed by making one rule of ours match StarCraft II's:

- **B-113.** A unit that the crowd pushes to within its own radius of a route corner sees the corner's arc points
  only by terrain view and jitters. In the showcase, pulse bot 7 stands 0.30 from the vertex (35, 52) with a
  radius of 0.375 and turns 454° at the ramp E foot. The cause is that our ground bodies are discs against units
  and routes but points against terrain. The game's units are discs against terrain too (§2.1).
- **B-115.** A homing spell missile moves exactly to contact on its step and impacts only when the next tick's
  check finds it there. A target that moves away in between is never reached: Abduct's tongue trailed a flying
  unit for 20 ticks. The game's guided missiles impact when they arrive (§2.2).

Each bug moves battles or goldens, so charter §6 requires a full plan.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-11 against `main` at
`2b3c8d6`. It assumes branch `bugfix-113-114` (`09c439f`: B-114 fixed; B-115 registered; contact variants E to H
recorded in deviation-fix evidence §18.6) has landed on `main` first. If it has not, WP1 waits for it. Line
numbers drift, so search for the quoted symbol. Claims not confirmed while planning are marked **verify**.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, D-1, D-6 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §6 |
| Q0002: scoping is the agent's call if it is written down | Standing rule | §9 |
| Open questions Q0018, Q0034, Q0075, Q0076, Q0078, Q0080 | None touches this plan. Q0080 (effect presentation) is not affected: B-115 changes when a missile lands, not how it is drawn | — |
| None new | — | Every choice has a default recorded as a decision. §9 lists what was deferred, including B-116 (InnerRadius) |

---

## 1. Summary

| | B-115: missile impact | B-113: disc contact with terrain |
|---|---|---|
| Game rule | A `Guidance` missile "chases the target, regardless of where it moves" and impacts on arrival. The `ImpactRange` of 5.0.14's two missiles is unset | Terrain and structures collide with a unit's **disc** (`InnerRadius`, Blizzard 5.0.13 notes). Paths are found at the unit's size |
| Our rule today | Step `min(speed, gap)`. Impact happens only when a **later** tick starts with `gap ≤ 2⁻²⁰` | Bodies are discs against units and routes, but **points** against terrain (`MotionPolicy::permits_segment` tests the centre). A push may press a disc up to its whole radius into a wall |
| Chosen rule | Impact on the tick whose step reaches contact, `gap ≤ speed`. Weapon projectiles already do this | A ground body's disc never goes deeper into a wall. A move that would press it in is projected out along the contact normal, so the body slides. Applied at the two choke points that write walking positions (locomotion's commit and the collision phase) |
| Size | One function, S | A `sim_map` query, a `sim_core` module, a harness; M to L |
| Expected impact | Matrix battles 37a and 38a may move by a tick. Every other battle, battle 33, the showcase and the goldens bit-identical (**verify**) | Matrix and battle 33 are bit-identical, because their flat arenas have no walls (**verify**). The showcase moves, and its lane fight is sensitive (variant E flipped it). The goldens move |
| Order | WP1, first | WP2 to WP5 |

---

## 2. What StarCraft II does

Tags used in this section: **[DOC]** Blizzard or official material; **[DATA]** SC2GameData 5.0.14 (`5444e12f13`),
extracted under `~/scratch/combat_wp3_data`; **[2ND]** a secondhand account; **[COMM]** community observation;
**[INF]** an inference made by this plan.

### 2.1 Unit bodies against terrain (B-113)

| # | Source | What it says |
|---|---|---|
| C1 | Blizzard, "StarCraft II 5.0.13 PTR Patch Notes", <https://news.blizzard.com/en-us/article/24061893/starcraft-ii-5-0-13-ptr-patch-notes> | "Worker inner radius (collision with terrain / structures) reduced from 0.375 to 0.3125." [DOC] |
| C2 | SC2GameData 5.0.14 `UnitData.xml` | Every ground unit has `Radius` (against units) and most have `InnerRadius`. Marine 0.375 / 0.375; Zealot –/0.375; Stalker 0.625 / 0.5; Marauder 0.5625 / 0.375; Hydralisk 0.625 / 0.375; Roach –/0.625; SCV, Probe and Drone 0.375, with `InnerRadius` 0.3125 in `voidmulti` (the C1 change). `core` sets `InnerRadiusSafetyMultiplier` 1 [DATA] |
| C3 | sc2mapster, "what is radius", <https://www.sc2mapster.com/forums/development/data/161926-what-is-radius?comment=3>; "collision size of unit", <https://sc2mapster.com/forums/development/miscellaneous-development/175792-collision-size-of-unit> | Inner radius controls how close a unit gets to buildings. The engine uses the smaller of inner radius and radius. A pathing footprint, where a unit has one, overrides the radius [COMM]. The pathfinding plan's S7 says the same |
| C4 | Pathfinding plan §2.1, S2 to S5 and S11 (`2026-11-09-sc2-pathfinding`) | A CDT navigation mesh with one A\* per unit. The funnel runs in a "tunnel inside the expanded vertices", so each path accounts for the unit's size. Moving units are not in the mesh: steering and pushing handle them [2ND] |
| C5 | gamedev.net, "Does starcraft2 use navmesh A\* or grid A\* for pathfinding?", <https://www.gamedev.net/topic/624135-does-starcraft2-use-navmesh-a-or-grid-a-for-pathfinding/> | A CDT of terrain and buildings, A\* with a funnel that respects unit radii, then local steering and collision avoidance [COMM, unverified summary of the GDC 2011 talk] |

**What follows for us.**

- **Disc against terrain [DOC].** In the game, a unit's collision with terrain and structures is a disc of radius
  `min(InnerRadius, Radius)` (C1, C3). Our centre-only terrain physics, which lets a disc overlap a wall by up to
  its radius, is therefore a deviation. No owner decision names it, so under Q0051 it is a bug, and B-113 is its
  visible symptom.
- **Pushes slide [INF].** A unit pushed against a wall in the game ends at its terrain radius from the wall and
  keeps the push's component along it. No written source was found for this. It is how an engine with a hard
  disc constraint and boids-style pushing (C4) behaves, and the alternative, refusing the whole push, would pin
  crowds at every ramp wall. This plan treats it as an inference, and the tests pin it.
- **One radius or two [DATA].** The game separates the unit radius (`Radius`) from the terrain radius
  (`InnerRadius`). Ours has one `collision_radius`. Several 5.0.14 units have a smaller inner radius (C2). This
  plan builds the contact on a single accessor, `MotionPolicy::terrain_radius()`, which returns
  `collision_radius` for now. Adding the second radius moves every route of those units, so it is registered as
  **B-116** and deferred (§9).
- **Not known.** How the engine resolves a disc pressed into a concave corner by a unit (the two walls' normals at
  once) is not documented. D-3 iterates the projection a fixed number of times and otherwise refuses the move,
  as today.

### 2.2 Missile impact (B-115)

| # | Source | What it says |
|---|---|---|
| M1 | SC2 editor guides, "Missile Movers", <https://s2editor-guides.readthedocs.io/Classic_Tutorials/04_Misc/missile-movers/> | The Guidance driver "causes the missile to chase the target, regardless of where it moves". Arrival is decided by `ArrivalTestType`, by default `Adaptive` (2D for points, 3D for units). The Launch Missile effect's `Impact Range` is "the distance from the missile's target that will trigger impact". Missiles can be dodged only when they cannot turn (powerslide, insufficient turn rate) [COMM, editor documentation] |
| M2 | `swarm` MoverData:143-152; EffectData:1486-1494 | `YoinkMissile`: driver Guidance, `Acceleration` 3200 (instant), `MaxSpeed` 18.75 (1.05 world units per tick), `YawPitchRoll` 719.3. `YoinkLaunchMissile` sets no `ImpactRange` [DATA] |
| M3 | `void` and `swarmmulti` MoverData:13-26 and 3-16; `void` EffectData:1141-1146 | `FungalGrowthWeapon`: Guidance, `MaxSpeed` 12 (0.672 per tick), `IgnoresTerrain`. `FungalGrowthLaunchMissile` has `ImpactLocation TargetPoint` and no `ImpactRange` [DATA] |
| M4 | `core` EffectData:95-107 (`CEffectLaunchMissile` default), `core` MoverData:13-25 (`MissileDefault`) | No default `ImpactRange`. The default mover's second motion phase ("the kill zone ... hone in the target ASAP") sets `YawPitchRoll MAX` [DATA] |

**What follows for us.**

- **No guided missile is outrun.** A Guidance missile turning at about 720° a second, flying 1.05 a tick against
  ground speeds below 0.4 a tick, always reaches its unit target in the game (M1, M2). Our tongue trailing a
  receding target forever is a bug, not a dodge.
- **Point-targeted missiles land where they were aimed.** Fungal flies to its point (M3). Units "dodge" it only by
  leaving the impact area, which our rule already models. Our one-tick delay after reaching the point is the same
  bug as above.
- **Impact on arrival [INF].** With no `ImpactRange` set, the missile impacts when its arrival test passes, on the
  same game step as its arrival. We stop at the target's edge (`collision_radius`), as our weapon projectiles do.
  Whether the game's 3D arrival is at the centre or at the attach point is not known. The difference is at most
  `radius ÷ speed`, under 0.6 of a tick for the tongue, and is left as it is (§9).
- **Weapon projectiles already follow the rule.** `advance_projectile` (`combat_resolution.rs`) impacts when
  `distance ≤ speed + target_radius`, so on the tick whose step reaches contact. Only the effect engine's
  `step_effect_missiles` (`effects/step.rs`) is affected. The two shipped `LaunchMissile` nodes are `abduct`
  (`abduct_tongue` after B-114) and `fungal_growth` (`assets/catalog/ability_effects.ron`).

---

## 3. Diagnosis (from the records)

### 3.1 B-113

The records are pathfinding plan Amendments 4 to 6 and its evidence ("Amendment 5", variants A to C),
deviation-fix evidence §18.5 (D) and §18.6 (E to H), and the traces under `~/scratch/bugfix-113/` (`base/`, `v1/`
to `v4/`, `b113_v4.diff`).

- **Mechanism.** At tick 756 the crowd has pushed pulse bot 7 to 0.30 from the vertex (35, 52). It is "wedged":
  its disc does not fit where it stands. At tick 757 it passes the two arc points round that corner by terrain
  view, which is the D-3 exemption of `drop_passed_waypoints`. Its next leg cuts the corner and is refused at
  tick 762. It re-plans back west and steps back and forth until tick 769.
- **Route rules fail (A to D).** They act after the body is already inside its radius, and each moved another box
  or flipped the winner (D).
- **Contact rules applied in the collision phase only fail (E to H).** E and G throw a body that overlaps a wall
  out to its full radius in one tick. They change the lane fight: in E, player 0 has no attackers left at tick
  900, player 1's two survivors reach ramp W, and the match is not over at tick 5,000. F, a corner-only rule, is
  discontinuous: along a straight wall a 0.355 overlap is allowed, so a body passing the wall's end (29, 12) by
  0.003 is moved 0.35 south. H stops corner deepening only, so a body slides along a straight wall into the
  corner already deep.
- **What the four have in common.** Each is a correction after the fact on top of centre-only physics.
  Locomotion and steering still put discs into walls every tick, and the collision phase then either jumps them
  out (E, G), jumps them at a boundary (F), or misses the case (H).

**Conclusion.** The rule must be the physics itself. No move, whether a walk or a push, may press a disc deeper
into a wall, at every wall rather than at corners only. The correction must never exceed what the move itself
would have pressed in, so nothing is ever thrown. That is D-1 to D-3. Whether the showcase's lane fight then
keeps its winner is measured, not assumed (D-8, R-1).

### 3.2 B-115

`step_effect_missiles`:

1. `gap = max(0, |aim − position| − radius)`.
2. If `gap ≤ ARRIVAL_EPSILON` (2⁻²⁰), the missile impacts and is removed.
3. Otherwise it moves `min(speed, gap)` towards `aim`.

Arrival is tested at the start of a step and the move happens at its end. A missile that closes the gap on step
`k` impacts on step `k + 1`, and only if the target has moved by at most 2⁻²⁰ in between. A target receding by
`v < speed` a tick leaves a gap of `v` each tick, so the missile trails it at contact distance until it stops.
This is the evidence's trace of the second tongue (spellcaster evidence §7). For a point, or a target that stands
still, the same rule costs one whole tick: the missile covers the distance on step `n = ceil(d ÷ speed)` and impacts
on step `n + 1`. The tests that count the impact from a cast (`fungal_growth_tests`, `abduct_tests`,
`effect_tree_tests`) encode that extra step together with the cast-tick placement of spellcaster D-16 item 3
("`1 + ceil(d / 0.672)` steps after such a cast"). **verify** which of the two the `1 +` stands for in each test
before editing it.

---

## 4. Design decisions

### D-1. B-113: what a ground body's disc may do against terrain

**Definitions.** Let `walls` be the blocking edges of the navigation mesh for the body's physical request: true
terrain, every structure, no vision, and the access of the body's layer (`Access::new`, as `segment_is_passable`
uses). Let `r` be `policy.terrain_radius()`, which is `collision_radius` until B-116. For a centre `p`:

- `d(p)` is the exact distance to the nearest wall point;
- `depth(p) = max(0, r − d(p))`.

Map borders are walls like any other. `MotionPolicy::contains` stays as a guard.

**Rule (non-deepening).** A ground body's walking move, by locomotion or by a push, from `p` to `q` is admitted
only if both hold:

- the existing centre test passes (`permits_segment`, unchanged: it keeps the terrain connectivity truth);
- `depth(q) ≤ depth(p) + WALL_CONTACT_TOLERANCE`.

A body clear of walls (`depth = 0`) therefore never overlaps one. A body that already overlaps a wall (a spawn, a
placement, a structure built beside it: D-3 of the pathfinding plan) may move along the wall or out of it, but
never deeper. It is never moved out by force.

**Why every wall.** Variant F showed that any rule that tolerates overlap on straight walls but not at corners is
discontinuous where the two meet. The nearest-point distance `d` is continuous everywhere, including across the
Voronoi boundary between an edge and a convex vertex, so a rule on `d` has no jumps.

**Air and burrowed bodies.** Air bodies are unchanged: terrain does not touch them. A burrowed body walks on the
ground layer and is treated as a ground body. Cliff-walking access, if a policy has it, follows its request's
`Access`.

### D-2. B-113: sliding (the projection)

When a move `p → q` fails D-1 only because of depth, the move is projected rather than refused:

1. Let `c` be the nearest wall point to `q` and `n = (q − c) ÷ |q − c|`. If `q` lies on the wall (`|q − c| = 0`),
   `n` is the unit normal of the nearest blocking edge on its open side.
2. Set `t = r − depth(p)`, the distance from the wall the body may not cross, and `q' = c + n · t`.
3. Repeat from step 1 with `q'`, at most `WALL_CONTACT_PASSES` (3) times, while `d(q') < t − WALL_CONTACT_TOLERANCE`.
   This handles a concave corner, where two walls press at once.
4. `q'` is admitted if:
   - D-1 holds for `p → q'`;
   - the centre test holds for `pos_at_tick_start → q'`, keeping the existing rule that a tick's whole travel be
     walkable;
   - `|q' − p| ≤ |q − p|`, so a projection never moves a body further than the move it projects.

   If not, the existing axis slides of `slide_target` are tried with the same test, and the body otherwise stays
   at `p`, as a refused step does today.

Along a straight wall, the projection removes the push's normal component and keeps its tangential component. At a
convex corner, `n` is radial from the vertex, so a body pressed against the corner slides round it, which is the
behaviour B-113 lacks. In a gap narrower than `2r` no projection fits and the body stays. No route leads there,
because route widths are tested at `2r`.

**Arithmetic.** `c`, `d²` and the edge normal are exact on the mesh lattice in `i128` (`predicates.rs`, as the
segment predicate is). `n` and `q'` use `SimScalar::sqrt` and the checked vector operations of `movement/math.rs`.
Rounding is absorbed by `WALL_CONTACT_TOLERANCE = 2⁻²⁰` world units, the same order as `ARRIVAL_EPSILON`, so
repeated projections cannot accumulate depth. Every overflow is reported as a `MovementInvariant::Arithmetic`, and
the move is then refused.

### D-3. B-113: where the rule applies (one function, two callers)

A new module, `crates/sim_core/src/movement/contact.rs`, owns the function
`settle_on_terrain(map, pathfinder, policy, tick_start, from, to) -> Result<Option<SimVec2>, MovementMathError>`,
which implements D-1 and D-2.

Walking positions are written in exactly two places, and both call it:

- **Locomotion.** `step_toward` and `slide_target` (`locomotion.rs`) produce the proposal that `MotionProposal::commit`
  writes. The proposal's final position is settled once before the commit, so every steering mode (route
  following, the surround walk, `aim_past_parked_body`, flee) goes through it.
- **Collision.** `apply_correction` (`collision.rs`) settles the corrected position. It does this for friendly
  corrections after `soft_position` caps them, and for hostile corrections, which stay uncapped.

Other position writers are placements, not walks: transport unload (`transport/mod.rs`), the Abduct flight and
landing (`effects/step.rs`), ability teleports (`world/abilities.rs`), and the push helper in `steering.rs`
(**verify** what calls it). They keep their own rules. The WP3 agent lists them in the commit with a grep, and if
one of them is a walk after all, it routes it through `settle_on_terrain`.

Steering probes that ask `permits_segment` whether a step is possible (the steering corridor, friendly-blocking R1
gaps, `loses_view`) keep the centre test. They are predictions, and the settled step may slide where the probe
said "go". This is the same relation as between steering and the collision phase today. If WP3's measurements
show a probe disagreeing in a way that causes a stall, the probe gains a disc test in that WP, recorded.

### D-4. B-113: routes, waypoints and the D-3 exemptions

- **Routes are unchanged.** The radius funnel keeps every leg at least `r + CORNER_SAFETY_MARGIN ÷ 2` from every
  vertex, and D-7 of the pathfinding plan keeps every leg's disc clear of every wall. Following a route therefore
  never triggers a projection. A property test pins this (T-B6).
- **The wedged exemptions stay.** `search.rs`'s start-triangle rule, `route_is_usable`'s first legs and
  `drop_passed_waypoints`' terrain view stay. A body can still start wedged, through a spawn or a structure
  placed beside it. After this plan, a push can no longer make a body wedged, which is B-113's mechanism.
- **No route-following change.** Variants A to D are not revived.

### D-5. B-113: the contact query (`sim_map`)

`Pathfinder::wall_contact(map, request, p) -> Option<WallContact>` returns:

- `point`: the nearest point of the blocking edges within `request.radius` of `p`;
- `distance_sq`: the exact squared distance on the Q16 lattice;
- `normal`: the open-side unit normal of the nearest edge, used when `p` is on the wall.

It returns `None` when no blocking edge lies within the radius, or when `p` is off the mesh or in a closed
triangle. Its flood is the segment predicate's flood for the degenerate segment `[p, p]`, run to completion and
keeping the nearest edge. `nearest_blocking_point` in `~/scratch/bugfix-113/b113_v4.diff` is a reviewed starting
point. Its `corner` flag is not needed and is dropped.

```rust
// PERFORMANCE RATIONALE:
// Why: Every moving ground body asks for its wall contact once or twice a tick; in the 400-unit canyon scenario
//      that is up to 800 floods per tick.
// What: An exact early-out first: if the body's cell and its neighbours within `r` are all ground-pathable and
//      free of structures (one look-up per cell of the flat grid), no wall is within `r` and the flood is skipped.
//      The flood reuses the predicate's retained `SegmentScratch`, so it allocates nothing.
// Trade-off: A second code path that must agree with the flood; a property test compares them on both maps.
```

The WP3 agent decides from measurement whether the early-out is needed. If it is not, the block is not written.
The exact form of the early-out (a cell scan or the clearance field that `checksum.rs` still hashes) is the WP's
choice, provided it is exact.

### D-6. B-115: the impact rule

In `step_effect_missiles`, after `missile_aim` has refreshed the aim to the target's current position:

- If `gap ≤ speed`, the missile moves to contact and **impacts on this step**:
  - its position becomes `aim` for a point target (`radius = 0`), or the point at `radius` from `aim` on the
    line from its position for a unit target;
  - its `impact` walk runs with the context's source point there (spellcaster plan D-15, the table entry for
    `LaunchMissile { to: TargetUnit }`: "the missile's position at contact");
  - it is removed.
- Otherwise it moves `speed` towards `aim`.

`ARRIVAL_EPSILON` is no longer needed as a separate test. It is kept only if the contact-point arithmetic needs it
for the degenerate `gap = 0` direction, and the WP records which. The tick placement of spellcaster D-16 is unchanged: a missile is first stepped in the same tick or the next one,
exactly as today. A missile now impacts on its `n`-th step, `n = ceil(d ÷ speed)` for a still target, instead of
step `n + 1`. That `n` is what `launch_projectile`'s `ticks_to_impact` already uses for weapons.

The weapon path (`advance_projectile`) is not edited, because it already impacts on the reaching tick. A unit test
fixes the agreement: the same inputs give the same impact tick in both paths (T-M4).

Lifetimes: effect missiles have none, and none is added. The game's guided missiles in 5.0.14 do not run out
before reaching a slower target, so a tongue chases until it impacts or its target becomes invalid (D-8 of the
spellcaster plan), as today.

### D-7. Determinism

- No floats. The contact is exact integer geometry on the mesh lattice, plus `SimScalar::sqrt`, which is
  deterministic.
- No new collections and no hashed state. `settle_on_terrain` is a pure function of the positions and the map, and
  the missile rule changes values only. The checksum schema stays at its current value (**verify** after the
  merge of `bugfix-113-114`). If any WP must add state, it bumps the schema and records it.
- Loops run in index order. The flood is the predicate's flood, already deterministic.
- The golden replays are re-run in debug, release and on `wasm32-wasip1` in every WP that changes behaviour.

### D-8. Measurement

`<base>` is the tree before the WP and `<cand>` the WP's tree. Both are release builds of the tools, copied per
step as in the pathfinding evidence's "How to reproduce", with each tree building into its own target directory
under `~/scratch/cargo-target/`.

| What | Command | B-115 (WP1) | B-113 (WP3, WP4) |
|---|---|---|---|
| Matrix | `cargo run --release --locked -p sim_core --example matchup_trace -- --all-battles --seeds 16 --jobs 8 --out <dir> --compare <base>/matrix.csv` | 37a and 38a may move. Every other battle bit-identical | Bit-identical: the arenas are flat, with no structures (**verify**). Any movement is traced (an arena border contact?) and explained |
| Battle 33 | `... --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out <dir>` (68 seeds), `seeds.csv` compared with `diff` | Bit-identical | Bit-identical (**verify**) |
| Showcase | `route_trace --scenario scenarios/showcase.ron --ticks 1800 --out <dir>` (and to the end of the match if it runs longer) | Identical (no spells) | The acceptance of D-9 |
| Wall overlap (new, WP2) | `route_trace`'s overlap columns | — | No unit-tick with depth greater than at the tick's start; the base's figures recorded first |
| Goldens | `golden_replay_tests` in debug, release and on `wasm32-wasip1` | Expected unchanged (no spell in the fixtures; **verify**) | Re-recorded. Each entry names its first changed checkpoint and the traced cause (entity, tick, which wall), as the existing headers do |
| Throughput | `cargo bench -p sim_core --bench throughput`, under the gate lock, base and candidate alternated, at least three rounds each at low load | The spell scenario within noise | Every scenario at 25 ticks/s or more; `showcase_canyon` at most 10 % below base |

### D-9. Stop rules and acceptance

General (as pathfinding D-12): **a winner flip anywhere stops the WP**. The agent commits nothing beyond the WP and
reports with the trace (first divergent tick and entity, and why). A flip is accepted only with a cited StarCraft II
reason **and** the orchestrator's acceptance in an amendment. A movement that is not a flip is explained before the
WP proceeds.

**B-115 (WP1).**

- 37a and 38a may move by the tick each missile gains. Their claims (the spell's own effect, spellcaster
  Amendment 6) must still hold on the seeds of record.
- Any other battle moving stops the WP.

**B-113 (WP3).** The showcase is judged against the base tree's own run. The base figures are those of
deviation-fix evidence §18.6: ramp W foot P0 1.301 / 1.473, 157° / 301°; ramp E foot P0 1.198 / 1.300, 190° / 454°
(6 units); P1 1.149 / 1.168, 81° / 152°; reversals 17 / 7; give-ups 0 / 0; player 0 wins at tick 1,693.

1. **B-113 fixed.**
   - Pulse bot 7 no longer wedges at (35, 52).
   - The ramp E P0 maximum turn is at most 287°, the grid's figure. Variant F's 71° shows that far less is
     possible.
   - The wall-overlap metric shows no body pressed deeper during the run.
2. **No box worse.** On the lanes units actually take (pathfinding Amendment 4), each box's mean ratio is at most
   base + 0.05 and its mean turn at most base + 15°. Ramp W's lane-end turn at (29, 12) is legitimate (Amendment
   4, 2).
3. **Liveness.** 0 stall give-ups, reversals at most base + 10, and the match ends. A match that runs for 5,000
   ticks, as in E and G, is a defect of the implementation, not a battle shape: it is traced and fixed in the WP.
4. **Winner.** Player 0 wins. If the winner flips:
   - stop, and trace the lane fight (ticks 310 to 577) base against candidate, giving the first divergent tick,
     the entity and the wall;
   - report whether the cause is the disc rule itself (bodies no longer pressed into the lane's walls change who
     reaches range first) or an implementation defect (a stall, a refused projection);
   - the cited reason for the former is C1.

   The orchestrator then decides, by amendment, to accept the flip or to re-tune. If the flip is accepted, the
   storyboard (`assets/captures/showcase.ron`) is re-checked with a real render as in pathfinding Amendment 6, 5.
5. **Matrix, battle 33, throughput** as in D-8.

If the acceptance is not reached after the projection is shown correct by its tests, the WP stops and the numbers
are reported. No tuning constant is invented to save a box.

---

## 5. File ownership

| Area | Files | WP |
|---|---|---|
| Missile step | `crates/sim_core/src/effects/step.rs` | WP1 |
| Missile tests | `crates/sim_core/tests/abduct_tests.rs`, `crates/sim_core/tests/fungal_growth_tests.rs`, `crates/sim_core/src/combat_resolution.rs` (test only) | WP1 |
| Contact query | `crates/sim_map/src/navmesh/segment.rs`, `crates/sim_map/src/navmesh/search.rs`, `crates/sim_map/src/pathfinding.rs`, `crates/sim_map/src/lib.rs` | WP2 |
| Contact tests (`sim_map`) | `crates/sim_map/tests/wall_contact_tests.rs` (new) | WP2 |
| Overlap metric | `crates/sim_fixtures/src/trace/route.rs`, `crates/sim_core/examples/route_trace.rs` | WP2 |
| Contact rule | `crates/sim_core/src/movement/contact.rs` (new), `crates/sim_core/src/movement/mod.rs` (`terrain_radius`), `crates/sim_core/src/movement/locomotion.rs`, `crates/sim_core/src/movement/collision.rs`, `crates/sim_core/src/movement/tuning.rs` | WP3 |
| Contact tests (`sim_core`) | `crates/sim_core/tests/wall_contact_tests.rs` (new) | WP3 |
| Test catalog | `crates/gate/suites.ron` (the two new test targets, C22) | WP2, WP3 |
| Goldens | `crates/sim_core/tests/golden_replay_tests.rs` and its fixtures | WP4 |
| Documents | `DESIGN.md` (register, Movement row), this plan, `docs/plans/README.md`, `docs/evidence/2026-11-11-contact-and-missiles.md` (new) | every WP appends; WP5 closes |

---

## 6. Work packages (commit order)

Every package writes its tests first and shows them red on the base, except WP2, which adds behaviour-free
queries. Implementation agents hand back after `cargo gate quick` and the tests their change needs. A verifier
then runs `cargo gate package` and reports the stamp or the failing targets (Q0061). Each package appends its
measurements to `docs/evidence/2026-11-11-contact-and-missiles.md`, which WP1 creates with a "How to reproduce"
section.

### WP1: B-115, missiles impact on the reaching tick (S)

- **Tests first.**
  - T-M1 (`effects/step.rs` unit): a unit-target missile 0.5 from contact, at speed 1.05, impacts on its first
    step, at the contact point. A point-target missile reaches its point and impacts on the same step.
  - T-M2 (`abduct_tests::a_tongue_reaches_a_unit_walking_away_from_it`): the target walks directly away at its
    own speed from the cast. Contact happens within `ceil(gap₀ ÷ (1.05 − v)) + 1` ticks. On the base it does not
    happen while the target walks: the B-115 reproduction.
  - T-M3 (`abduct_tests`): the traced case of the evidence, a second tongue meeting a flown unit, is no longer
    reachable after B-114's mark. Its geometry is re-used with one walking target and one target flown by a
    `Displace` from a scratch catalog, so that a receding flown unit is also covered. **verify** that the scratch
    catalog can express it. If it cannot, T-M2 alone stands.
  - T-M4 (`combat_resolution.rs` unit): a weapon projectile and an effect missile with the same speed, start,
    target radius and receding target impact on the same tick.
- **Change.** D-6 in `step_effect_missiles`.
- **Expected test edits.** Each test that counts an impact or a contact tick loses exactly one step:
  - `fungal_growth_tests::the_missile_lands_ceil_distance_over_speed_later_and_is_published_while_it_flies`
    (`1 + 13`);
  - `abduct_tests::the_tongue_lands_ceil_gap_over_speed_later_and_the_pull_ends_twenty_ticks_after`, and the
    reference values derived from the flight;
  - the impact counts in `effect_tree_tests`.

  For each, the agent re-derives the count from the tick placement of spellcaster D-16 item 3 and writes the reason
  next to it. A test whose count does not drop by exactly one is a finding, and is reported. The spellcaster plan's
  sentence "the tests count the impact `1 + ceil(d / 0.672)` steps after such a cast" gets a one-line amendment
  pointing here.
- **Measure.** Matrix, battle 33, showcase, goldens, and the spell scenario of the throughput benchmark (D-8).
- **Register.** B-115 is removed from the register (fixed). The `DESIGN.md` Abilities row drops its B-115 clause.

### WP2: contact query and overlap harness, no behaviour change (M)

- `sim_map`: `WallContact` and `Pathfinder::wall_contact` (D-5).
- **Tests** (`crates/sim_map/tests/wall_contact_tests.rs`):
  - straight edge;
  - convex corner, where the nearest point is the vertex;
  - concave corner, where the nearest of two edges wins;
  - a structure's edges are walls, and ramp-connected level edges are not;
  - map border;
  - `p` on the wall, where the open-side normal is used;
  - no wall within `r` gives `None`;
  - a property test: on both shipped maps, at 2,000 seeded points and radii 0.25 to 1.5, the query equals a brute
    force over every blocking edge (exact `distance_sq`);
  - if D-5's early-out is written in WP3, the property test extends to it there.
- `route_trace`: per owner, the number of unit-ticks with `depth > 0`, the maximum depth with its entity, tick and
  position, and the number of ticks on which a body's depth rose. Recorded on the base: this measures how much of
  today's battle relies on overlap, and where.
- **Check the showcase's RNG use.** If the showcase consults the world's RNG, add `--seed` to `route_trace` so that
  WP3 can test the lane fight's sensitivity on 8 seeds, base against candidate. If it does not, record that, and
  the sensitivity check is dropped.
- **Re-read variant E's traces** (`~/scratch/bugfix-113/v1/`) for the first tick at which the lane fight diverges
  from base, and record the cause in the evidence. This tells WP3 what to watch.
- **Bit-identical.** The matrix, battle 33, showcase `units.csv` and goldens are unchanged.

### WP3: the non-deepening contact rule (M to L)

- **Tests first** (`crates/sim_core/tests/wall_contact_tests.rs`, synthetic maps built in the test, and the shipped
  scenario for T-B5):
  - T-B1 `a_crowd_never_presses_a_body_inside_its_radius_of_a_corner`: eight friends attack-move round an L-shaped
    cliff, each with a 0.375 body. On every tick no ground body's depth rises. Red on base.
  - T-B2 `a_push_into_a_straight_wall_slides_along_it`: a hostile body pushes at 45° into a wall. The body ends at
    `r` from the wall, and has moved by the push's tangential part.
  - T-B3 `a_body_passing_a_wall_end_is_never_thrown`: variant F's regression. A body with an existing overlap of
    0.355 walks along a wall past its end. Its per-tick displacement is at most its speed, and its depth never
    rises.
  - T-B4 `a_wedged_spawn_leaves_but_never_goes_deeper`: D-3 of the pathfinding plan is preserved. A body spawned
    0.2 into a wall leaves along its route, and pushes do not deepen it.
  - T-B5 `showcase_pulse_bot_rounds_the_ramp_e_corner`: the showcase scenario run to tick 800. The traced pulse
    bot's distance from (35, 52) stays at least `r − tolerance`, and its turning in ticks 740 to 800 is at most
    180°. Red on base (0.30 and the 454° episode). It is classed in `suites.ron` by its run time.
  - T-B6 (property): for seeded routes on both shipped maps, walking each leg never triggers a projection (D-4).
  - T-B7 (property): seeded random walls and pushes. Depth never rises, and `|q' − p| ≤ |q − p|`.
  - T-B8 (`contact.rs` unit): concave corner (the passes converge, or the move is refused); `q` on the wall (the
    normal path); arithmetic overflow, reported and refused.
- **Change.** D-1 to D-3, with `terrain_radius()` returning `collision_radius`. The position writers are listed in
  the commit (D-3).
- **Measure.** Everything in D-8. **Stop rules** of D-9 apply. If the acceptance is not reached, the branch is held
  and reported; nothing lands half-way.

### WP4: goldens and landing (S to M)

- Re-record `golden_replay` and `golden_replay_orders` on the final tree. Each records its first changed checkpoint
  and the traced cause (entity, tick, which wall), checked in debug, release and `wasm32-wasip1` (wasmtime). If
  `golden_replay_orders` loses its mid-route repath evidence, as it did in pathfinding Amendment 6, choose the
  smallest goal change that restores it and that a mutation still catches. Record it.
- Check the sandbox checksum (it hashes spawned state, and is expected unchanged).
- If D-9, 4 accepted a winner change, re-check the storyboard with a real render (`showcase_tests` through the GPU
  harness) and adjust scene ticks and captions.
- Throughput (D-8) under the gate lock. `DESIGN.md` §5 is updated if the figures move.

### WP5: bookkeeping (S)

- Register: B-113 removed (fixed). Add **B-116**, terrain radius versus unit radius (§2.1, C1 to C3), open, with
  this plan as its pointer (**verify** the next free number).
- The `DESIGN.md` Movement and pathfinding row says that bodies are discs against terrain, and lists
  `wall_contact_tests.rs`.
- Pathfinding plan: a one-line amendment pointing here for B-113. Deviation-fix evidence: §18.6 gets a forward
  pointer.
- This plan's index row becomes `Done`, with the commits.

---

## 7. Risks

| # | Risk | Mitigation |
|---|---|---|
| R-1 | The showcase's west-lane fight is near-tied, and variant E flipped it. A correct disc rule may flip it too | D-9, 4: trace, classify (rule or defect), orchestrator decision. WP2 records today's overlap use and E's divergence first. The sensitivity check runs if the RNG is used |
| R-2 | Crowds jam at ramp mouths that today spill over wall overlap | Sliding (D-2) keeps the tangential motion. Liveness rows of D-9, 3. A jam is traced to a probe or projection defect before any tuning |
| R-3 | Steering probes (centre test) and settled steps (disc test) disagree and cause stalls at walls | D-3: a probe gains a disc test in WP3 if measurement shows it, recorded |
| R-4 | Throughput: up to two floods per moving ground body per tick | D-5's exact early-out, retained scratch, measured against the 10 % bound |
| R-5 | Rounding in repeated projections creeps a body into a wall | `WALL_CONTACT_TOLERANCE` and T-B7. The depth comparison is on exact `distance_sq` |
| R-6 | Concave corners: three passes do not converge | The move is refused, as today's refused step is (T-B8). Bodies cannot enter gaps narrower than `2r` |
| R-7 | Arena borders become walls for the contact, and move the matrix | Expected rare. Any movement is traced. `contains` already keeps the disc inside the map, so a border contact only turns a refused push into a slide |
| R-8 | B-115: battles 37a and 38a move enough to break a claim | Their claims are about the spell's effect, and a tick earlier only strengthens it. A broken claim stops WP1 |
| R-9 | A placement writer (unload, landing, teleport) puts a disc into a wall, and the body stays wedged | Non-deepening lets it leave along its route (T-B4). Placement rules are out of scope (§9) |

---

## 8. Determinism summary

Exact integer geometry for the contact; `SimScalar::sqrt` for normals; no floats, no hashed collections, no new
hashed state, no schema bump expected (D-7). Golden replays re-run in debug, release and on `wasm32-wasip1` in WP1,
WP3 and WP4, and re-recorded with traced causes in WP4. The `sim_map` digests (`determinism_tests`,
`validation_golden_tests`) are unchanged: the query is additive.

---

## 9. Out of scope and follow-ups

Scoping decisions under Q0002, each of which can be brought back:

- **B-116, terrain radius (`InnerRadius`).** The game collides units with terrain and structures at
  `min(InnerRadius, Radius)` (C1 to C3), and paths at that size are likely [INF]. Adding the field moves the routes
  of Stalkers, Marauders, Hydralisks and workers. It is registered in WP5 and planned separately. This plan
  isolates it behind `MotionPolicy::terrain_radius()`, so the follow-up is a catalog field and a data change plus
  its measurement.
- **Missile arrival at the centre or the edge** (§2.2). The game's 3D arrival point is not documented. We keep the
  target's edge, as our weapons do.
- **Placement rules** for unload, landing and teleports (R-9).
- **Route-following variants A to D** are not revived (D-4).
- **Unit-against-unit contact** (pairs, push priorities, friendly blocking) is unchanged. Friendly-blocking's walls
  of bodies (its D-1.1) are bodies, not terrain, and are not touched.
