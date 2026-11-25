# Blueprint: Collision Classes as Data, So Small Ground Units Walk Beneath the Colossus Analogue (B-209)

This plan settles bug **B-209** of `DESIGN.md` §4 against StarCraft II 5.0.14:

- **B-209: the colossus analogue (`tripod_annihilator`) collides with every ground body.** 5.0.14's `Colossus`
  carries the `Collide` flags `Colossus`, `Structure` and `Flying` only (`liberty` UnitData:4066–4068), while every
  small ground unit carries `Ground`, `Small`, `ForceField` and `Locust`. Two units touch only when their flag sets
  share a flag, so zerglings, marines and stalkers walk beneath a colossus, colossi walk over them, and colossi block
  only colossi. Ours pairs the colossus with every ground body in both contact pipelines, in steering, on attack
  rings, in displacement admission and at production exits. Matrix battle 26 therefore measures a ring of zerglings
  packed round bodies they would walk under. The finding is from plan
  [`2026-11-25-enemy-contact-and-ring-retarget`](2026-11-25-enemy-contact-and-ring-retarget.md) §2.6 (on branch
  `enemy-contact-plan`, parked under Q0113), whose row this plan registers on `main`.

**What this plan does.** It replaces the movement code's fixed three body classes (ground, air, burrowed;
`MotionPolicy::body_class`) with **collision classes**: the three built-in classes stay as they are, and an archetype
or a mode may name a class of its own in `assets/catalog/units.ron` (`collision_class: Some("tall_walker")`). Two
bodies touch only when their classes are equal. Every place that today asks "same body class?" asks "same collision
class?" instead, through one accessor. With no archetype naming a class, the catalog, every world, every checksum and
every golden are unchanged (WP1 proves it byte for byte). The colossus analogue then names its class (WP3), and the
battles with a colossus analogue move under pre-registered rules.

**Status.** Stage 1 blueprint under charter §6, written on 2026-11-25 on branch `collision-classes-plan`, cut from
`main` `be58ebc7` (checksum schema 47, contact solver the default pipeline).
- It builds on `main` **as it will be once B-208 lands** (branch `solver-pair-cache`: K-1, the solver's pair cache
  keyed by move count, and the enemy-contact plan's WP0 diagnostics in `crates/sim_fixtures/src/trace/contact_checks.rs`
  and `harness/room.rs`). WP0 starts only after that merge (stop rule 1).
- **Line numbers drift,** so search for the quoted symbol.
- **Claims not confirmed while planning** are marked **verify**.
- **Later decisions** are appended as `## Amendment N (role, YYYY-MM-DD): …`; an `## As built` section follows the
  merge.

**Why a full plan (charter §6).** It adds a capability to the catalog schema, changes a mechanic (which bodies touch),
moves battle outcomes, and spans four packages.

**Numbers (provisional, next free at merge):**
- **B-209** keeps the number the enemy-contact plan reserved for it (that plan is parked; whichever branch merges
  second drops its duplicate row and points at the first).
- New rows from **B-211**: B-211 (the Colossus' `Flying` flag: contact between a colossus and air units, not
  modelled, **verify**), B-212 (production exits ignore the burrowed class: a burrowed body blocks a ground
  product's exit, which the body-class rule elsewhere does not), B-213 (cliff-walking and cliff-jumping movers
  traverse as plain ground; 5.0.14 `Colossus` mover `PathMode Scaler`, `CliffJumper` `PathMode Jumper`, `liberty`
  MoverData:226–232). B-212 is fixed here (WP2); B-211 and B-213 are registered and deferred.
- **Question:** a draft **Q0114** (provisional, not filed) in §15, on B-211.
- **Checksum schema:** no hashed state changes (§9). The schema stays where `main` has it at merge (47; B-208 takes no
  number). If an implementation finds it must hash new state, it takes the next free number at merge and records it
  in an amendment.

**Questions and standing rules.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0103's guidance (charter §8): natural movement outweighs a close matchup's winner; a close battle that flips is restated with evidence; broken behaviour is a bug | Standing rule | §7.3, §8 |
| Q0008: where the data is silent, search for what StarCraft II does | Standing rule | §2.1 S-5 to S-8 |
| Q0002: scoping is the agent's call if written down | Standing rule | §2.5, §14 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §12 |
| Q0113 (open): "enemies block" (B-193) parked; its recommendation is a third attempt that starts with B-209 | Read | This plan does not touch the enemy rules (§14); it gives the third attempt a battle 26 that measures what it should |
| The owner's guidance: no fitting to fixed seed sets; held-out seeds only confirm; simpler deviations allowed; survey first | Followed | §3, §7.4 |
| `docs/questions/open/` on `be58ebc7`: Q0113 only | Read | — |

**Inputs read for this plan:** AGENTS.md; `docs/questions/README.md` and Q0113; the enemy-contact plan and its
evidence (`git show enemy-contact-plan:docs/plans/2026-11-25-enemy-contact-and-ring-retarget.md`,
`…:docs/evidence/2026-11-25-enemy-contact-and-ring-retarget.md`), its B-209 row in `DESIGN.md`; plan
`2026-11-25-wind-up-and-hold-facing` (for the acceptance form); plan `45-round6-authored-movement-layers`
(cliff movers); `crates/sim_core/src/movement/{mod,solver,collision,locomotion,ring}.rs`,
`production/exits.rs`, `entity.rs` (`passes_through_units`, `is_burrowed`), `catalog.rs` (`UnitModeSpecDef`),
`combat.rs` (`within_band`, `is_within_firing_arc`), `combat_resolution.rs` (`attack_direction`); the diagnostics on
`solver-pair-cache`; 5.0.14 game data under `~/scratch/combat_wp3_data/<layer>/UnitData.xml` and `MoverData.xml`,
resolved through every layer with each unit's parent (script in §16).

---

## 1. Summary

| | Today (`be58ebc7` + B-208) | After this plan |
|---|---|---|
| Which bodies touch | Equal `MotionPolicy::body_class`: ground (0), air (1), burrowed (2), three fixed classes | Equal **collision class**: the three built-ins, plus one class per name the catalog authors (D-1) |
| How a class is authored | Not authorable; derived from the mover layer and the burrowed flag | `collision_class: Option<String>` on an archetype and on a mode in `units.ron`, omitted by default (D-2) |
| Colossus analogue | Ground class: pushes, is pushed by, steers round and blocks the rings of every ground body | Class `"tall_walker"` (5.0.14's `Colossus` flag): touches only other colossus analogues; ground units walk beneath it, it walks over them (WP3) |
| Burrowed Lurker analogue | Burrowed class | Unchanged |
| Structures | Not bodies; met through footprints at the terrain radius | Unchanged: the colossus analogue still meets footprints (5.0.14 `Structure` flag) |
| Production exits | Blocked by every unit of the product's layer, burrowed ones included | Blocked by units of the product's collision class only (D-6; B-212) |
| Displacement admission (Blink, unload) | Refused onto a body of the same body class | Refused onto a body of the same collision class (automatic) |
| Routes, mesh, lanes | Never contain bodies | Unchanged |
| Targeting, melee | A melee unit attacks from its band; a band with `min_range` 0 has no near limit | Unchanged; a zergling beneath a colossus is in band and swings (D-7) |
| Hashed state, schema | — | None; schema unchanged (§9) |
| Worlds that move | — | Only worlds with a colossus analogue (§7.5): matrix 26, probably 12a and 12b; 12c and 35 predicted byte-identical |
| Size | — | M: WP0 records; WP1 mechanism (latent) M; WP2 exits S; WP3 data S; WP4 confirmation S |

---

## 2. What StarCraft II does

### 2.1 Sources

| # | Source | What it says | Confidence |
|---|---|---|---|
| S-1 | `core` UnitData:3–20 | The named flags are constants on `CUnit.Collide.index`: `Burrow` Land1, `Ground` Land2, `Larva` Land3, `Swarm` Land4, `Colossus` Land5, `Structure` Land6, `RoachBurrow` Land7, `TinyCritter` Land8, `ForceField` Land9, `CreepTumor` Land10, `Locust` Land11, `LocustForceField` Land12, `Phased` Land14, `DisruptorPhased` Land15, `Small` Land16; `Flying` Air1, `FlyingImmobile` Air2, `FlyingEscorts` Air3. A unit's `Collide` is a bit set | High (data) |
| S-2 | `liberty` UnitData:4066–4068 | `Colossus`: `Colossus`, `Structure`, `Flying`; no `Ground`, `Small`, `ForceField` or `Locust`, in any later layer | High (data) |
| S-3 | Resolved sets, every layer (§16) | See §2.2's table | High (data) |
| S-4 | `libertymulti` UnitData:574–576; :81 | The multiplayer `ForceField` clears `Ground`, `Small` **and `Colossus`**; the multiplayer `Archon` clears `ForceField`. Both edits only make sense if two units touch when their sets **share a flag**: a Force Field stops a unit whose set holds one of its flags, so clearing `Colossus` from it lets colossi walk through, and clearing `ForceField` from the Archon lets it walk through | High (inferred from data; the editor documentation is not available here, **verify** against the Galaxy editor's `Collide` help) |
| S-5 | Liquipedia, [Colossus (Legacy of the Void)](https://liquipedia.net/starcraft2/Colossus_(Legacy_of_the_Void)) | Its legs let it stack over units of its own army, traverse cliffs and see high ground | Medium (summary; the page could not be fetched in full while planning, **verify** the wording) |
| S-6 | StarCraft wiki (Fandom), Colossus | Steps over smaller units but not over other colossi or obstacles; climbs one level at a time | Medium (fan wiki) |
| S-7 | Blizzard Q&A batch 17 ([GosuGamers](https://www.gosugamers.net/news/7262-starcraft-ii-q-a-batch-17)) | "The Colossus does not block flying units" | Low to medium (pre-release; the 5.0.14 data gives it `Flying`, S-2; see §2.3) |
| S-8 | `liberty` MoverData:226–232 | `Colossus` mover: `HeightMap Glide`, `PathMode Scaler`; `CliffJumper`: `PathMode Jumper`. Cliff traversal is the mover's, not `Collide`'s | High (data) |

`Collide` carries no alliance field, so the rule applies to friend and enemy alike: enemy zerglings walk under an
enemy colossus as friendly stalkers walk under a friendly one (S-5 and S-6 speak of friends; the enemy case follows
from S-1 to S-4, high confidence).

### 2.2 Which shipped units have non-default sets

Resolved from all layers with parents (`core` → `balancemulti`; `-X` is a flag a later layer clears):

| 5.0.14 unit (our analogue) | Effective `Collide` | Consequence in 5.0.14 | Modelled here |
|---|---|---|---|
| Every small ground unit: Marine, Marauder, Reaper, Hellion, Hellbat, Siege Tank (both modes), Ghost, SCV, Probe, Drone, Zealot, Stalker, Sentry, Adept, Immortal, Disruptor, Zergling, Baneling, Roach, Ravager, Hydralisk, Queen, Infestor, Lurker (unburrowed), Swarm Host, Broodling, Changeling, Viking (assault) | `Ground`, `Small`, `ForceField`, `Locust` | Touch each other | Built-in ground class |
| Ultralisk, Thor; Archon (`-ForceField`, `libertymulti`:81) | `Ground`, `Small`, `Locust` | Touch small ground units; walk through Force Fields | Built-in ground class (no Force Field exists here) |
| **Colossus** (`tripod_annihilator`) | `Colossus`, `Structure`, `Flying` | Touches only colossi among ground units; structures by footprint; air units **verify** (§2.3) | **Class `tall_walker` (WP3)** |
| Burrowed Zergling, Baneling, Hydralisk, Ultralisk, Queen, Drone, Widow Mine, Lurker (`LurkerMPBurrowed` `void`:775); Roach and Ravager (`Burrow`, `-RoachBurrow`); Swarm Host burrowed | `Burrow` | Touch only burrowed units (and structures, which carry `Burrow`) | Built-in burrowed class (B-93); only the Lurker analogue burrows here |
| Infestor burrowed | `RoachBurrow` | Touches only units moving burrowed with that flag, and structures | Not modelled (our Infestor analogue does not burrow) |
| Every air unit but the Interceptor | `Flying` | Touch each other | Built-in air class |
| Interceptor | `FlyingEscorts` | Touches only interceptors | Not a body here (the Carrier analogue's weapon) |
| Locust (`LocustMP`) | `Locust`, `LocustForceField` (`-Ground`, `-Small`, `-ForceField`) | Touches every small ground unit (they carry `Locust`) but not each other's `Ground`; asymmetric sets | No analogue |
| Force Field | `Burrow`, `ForceField`, `LocustForceField` (`-Ground`, `-Small`, `-Colossus`) | Blocks small ground units, burrowed units and locusts; not the Colossus, Ultralisk, Thor, Archon | No Force Field ability exists here (the Sentry analogue has none) |
| Adept shade; Purification Nova; Larva | `Phased`; `DisruptorPhased`; `Larva` | Touch only their own kind and structures | No analogue as a body |
| Structures (Pylon, Nexus, Command Center, …) | `Burrow`, `Ground`, `Small`, `Structure`, `RoachBurrow`, `ForceField`, `Locust`, `Phased` | Placement and landing; units meet footprints through the pathing layer (a lowered Supply Depot keeps `Ground` yet is walked over, so the unit flags do not decide walking over structures, **verify**) | Not bodies; footprints (plan `2026-11-18-structure-contact`) |

The Reaper's cliff jump and the Colossus' cliff walk are mover features (S-8), not collision; the Reaper's set is the
default one. The Hellbat's set is the default one.

### 2.3 What the data does not settle

- **The Colossus' `Flying` flag.** By S-1 and S-4 a colossus would touch air units, which share `Flying`. S-7 says
  the opposite, and our movement keeps the ground and air layers apart by construction. This plan keeps them apart
  (the colossus analogue's class is a ground-layer class) and registers **B-211**, deferred; Q0114 drafts the
  question (§15). Default: no contact between a colossus analogue and air bodies, as today.
- **How the game places a produced unit.** Whether the free-spot search for a new unit tests the same `Collide` sets
  is not in the data read here (**verify**). D-6 makes our exits use the one class rule we have everywhere else.

### 2.4 The rule that follows

1. Two bodies touch when their collision sets intersect (S-1, S-4).
2. For every unit this project ships, each set intersects exactly the sets of its own kind among bodies (§2.2): the
   small ground units' sets are identical; burrowed sets are `{Burrow}` (and the Roach's `{Burrow}` too); air sets are
   `{Flying}`; the Colossus' set meets no other mobile ground unit's. So on our roster rule 1 reduces to **equality of
   a class**, with structures handled by footprints and the Colossus' `Flying` deferred.

### 2.5 Neighbouring rules: in or out

| Rule | In or out | Why |
|---|---|---|
| Colossus vs small ground units, friend and enemy | **In** (WP3) | B-209 |
| Colossus vs colossus | In (unchanged: same class) | S-2 |
| Colossus vs structures | In (unchanged: footprints at the terrain radius) | `Structure` flag; footprints already model it |
| Burrowed bodies | Unchanged (built-in class) | B-93 |
| Production exits by class | **In** (WP2, B-212) | One rule for every body test; today's exits disagree with B-93 |
| Colossus vs air units (`Flying`) | Out; B-211 | §2.3; Q0114 |
| Flag sets with asymmetric intersection (Locust, Force Field, Infested burrow, Interceptor) | Out | No unit of the roster needs them; D-1 is the restriction of sets to singletons, and §14 records the generalisation |
| Cliff walking (Colossus) and cliff jumping (Reaper) | Out; B-213 | Mover features (S-8); plan `45-round6-authored-movement-layers` records them as unresolved |
| Enemy push rules (B-193, E-2), the ring retarget (B-192) | Out | Parked work on `enemy-contact-plan` (Q0113); this plan changes only which pairs exist |

---

## 3. Survey: how our code decides that two bodies touch (on `be58ebc7`)

One concept decides it: `MotionPolicy::body_class` (`crates/sim_core/src/movement/mod.rs`), `2` when the body is
burrowed (`Entity::is_burrowed`), else the layer's index (ground 0, air 1); `BODY_CLASSES = 3`. Its readers:

| # | Site | What it does with the class |
|---|---|---|
| R-1 | `solver.rs` `ContactSolver::build_pairs` | One grid of every body; a pair needs equal classes; reach `radius + largest[class] + PAIR_SLACK`, `largest` a `[SimScalar; BODY_CLASSES]` over the bodies |
| R-2 | `collision.rs` legacy pass (after the `MovementPipeline::Solver` early return) | Same filter and reach on its own grid |
| R-3 | `locomotion.rs` phase set-up (`self.movement.obstacles[class]`, `largest_radius[class]`, `grids[class]`) | Steering obstacles in one list and grid per class; a hidden, mining or collision-suppressed body (`passes_through_units`) and a foundation are in none |
| R-4 | `locomotion.rs` `MotionScene::gather_neighbours` | Steering neighbours from the body's own class's list |
| R-5 | `ring.rs` `scan_ring` | Bodies that block a chaser's attack ring are those of the chaser's class |
| R-6 | `mod.rs` `validate_placement_unoccupied` | Blink, unload and other displacements refused onto a body of the same class |
| R-7 | `production/exits.rs` `exit_point_clear` | **Does not read the class:** filters by layer only (`MovementLayer::from_mover(arch.mover_type)`), so a burrowed body blocks a ground product's exit (B-212) |
| R-8 | `sim_fixtures` `trace/contact_checks.rs` and `harness/room.rs` (from `solver-pair-cache`) | The measurement's own class, `(MovementLayer, bool)`, for "touching", enemy and friend overlap and ring room |

Other facts the design must keep:
- **Terrain follows the layer**, not the class (`MotionPolicy::layer`, `permits_segment`, the solver's ground
  `connected` test, `contact.rs`). A colossus analogue is a ground-layer body on cliffs and footprints.
- **`passes_through_units`** (hidden, mineral walking, `suppress_collision`) removes a body from every pair and list;
  it is orthogonal to the class and stays.
- **Push priority, D-β (moving friends push), D-δ (nudge along the ring), the held and fixed rules** are per pair; a
  pair that does not exist runs none of them.
- **Routes** (navmesh, A*, funnel, group lanes) never contain bodies; they use the terrain radius. A colossus analogue
  routes exactly as today; only local steering stops seeing ground bodies.
- **Combat** never reads the class: `within_band` has no near limit for `min_range` 0, `is_within_firing_arc` accepts
  a coincident target, and `attack_direction` falls back to the attacker's heading for a zero offset, so a line splash
  at a target beneath its attacker has an axis.
- **Construction sites** (`construction.rs`, units that must leave a site) and **loading** (`transport/orders.rs`)
  read the layer; a colossus analogue still clears a site (5.0.14 `Structure`) and still boards (CargoSize 8).
- **The client** reads no class (**verify** in WP1 by `grep body_class crates/client`).

What a capability in `units.ron` must therefore cover, so no code names a unit: a class name per stance (the
archetype's default stance and each mode), resolved to a small index at catalog load and read by R-1 to R-8 through one
accessor.

---

## 4. Design decisions

### D-1. Collision classes: equality of an index

`crates/sim_core/src/movement/mod.rs` gains `pub struct CollisionClass(u8)` (documented, `Copy`, `Ord`), replacing
`body_class() -> usize` and `BODY_CLASSES`:
- Built-ins: `GROUND` 0, `AIR` 1, `BURROWED` 2, exactly today's indices.
- Authored classes: the distinct names the catalog authors, **sorted** (a `BTreeSet<String>` at load), take 3, 4, … in
  that order, so the index does not depend on file order. The catalog keeps the names (`UnitCatalog::collision_classes()`,
  for diagnostics and messages) and the count (`collision_class_count()`).
- Two bodies touch iff their classes are equal: `CollisionClass::touches(self, other) -> bool`. One function, so a later
  set model (§14) changes one place.
- An authored name never equals a built-in: authoring `"ground"` creates a new class named `ground` that touches only
  bodies authoring `"ground"`. The doc comment and the catalog comment say so; no name is reserved in code.

### D-2. The catalog field

`units.ron`, on an archetype and on a mode (`UnitModeSpecDef`), `#[serde(default)]`:

```ron
// Collision class (plan 2026-11-25-collision-classes D-2): bodies touch only bodies of the same class.
// 5.0.14 Colossus collides with Colossus, Structure and Flying only (liberty UnitData:4066-4068): small
// ground units walk beneath it. Structures meet it through footprints; air units do not (B-211).
collision_class: Some("tall_walker"),
```

A body's class (resolved in `MotionPolicy::resolve`, which already reads the entity and archetype):
1. its **active mode's** authored class, if the active mode authors one (the settled mode, as `is_burrowed` reads it:
   a unit still channelling a switch keeps its old stance's class);
2. else `BURROWED` if `Entity::is_burrowed`;
3. else the **archetype's** authored class;
4. else `GROUND` or `AIR` by `MotionPolicy::layer`.

`MotionPolicy` replaces `burrowed: bool` with `class: CollisionClass`. Absent everywhere, rules 1 and 3 never apply and
every body's class equals today's `body_class()`.

**Validation at load** (typed variants of the catalog's existing `thiserror` error, each with a test): a name is
non-empty ASCII `[a-z0-9_]`; every stance that resolves to one authored name has the same movement layer (the
archetype's `mover_type`, or the mode's `mover_type_override`), so no class spans ground and air; at most
`u8::MAX - 3` names. A mode's `burrowed: true` with an authored class is allowed (rule 1 wins; `burrowed` still drives
concealment and presentation).

### D-3. Readers use the class (R-1 to R-6, R-8)

- R-1, R-2: the filter `b.class == a.class` becomes `a.class.touches(b.class)`; `largest` becomes a workspace
  `Vec<SimScalar>` of `collision_class_count()` entries, cleared per tick (no allocation in steady state).
- R-3 to R-5: `obstacles`, `grids` and `largest_radius` become `Vec`s sized once to `collision_class_count()` when the
  movement workspace is created for the world's catalog (the catalog is fixed for a world; a catalog with a different
  count is a new world). Bodies enter their class's list in the same id order as today.
- R-6: unchanged code, new meaning (the class from `MotionPolicy`).
- R-8: `sim_core` exposes `SimWorld::collision_class(&self, id: EntityId) -> Option<CollisionClass>` (read-only,
  documented); `contact_checks.rs` and `harness/room.rs` replace their `(MovementLayer, bool)` with it, keeping the
  layer only for `walkable`. B3's enemy overlap, B6's friend overlap, D-SHOVE-E's "touching" and D-R10's ring room then
  count only pairs that touch.

With the shipped catalog unchanged, every list, grid, pair and iteration order is the one of today: WP1 is latent.

### D-4. Solver rules, D-δ and push priority

No change. They act on pairs; a zergling and a colossus analogue are no pair, so neither pushes, holds, nudges or
blocks the other. Colossus analogues among themselves keep every rule (priority 10, the same as today). Enemy pairs of
the same class keep C-8's even split (B-193 stays open and parked).

### D-5. Pathfinding and the mesh

No change. Routes contain no bodies; a colossus analogue's route is today's. What changes is local: its steering sees
only colossus analogues, so it walks straight through a crowd of ground units, as 5.0.14's colossus walks over them;
ground units' steering no longer swerves round it. The terrain radius (0.5625) against cliffs and footprints is
unchanged.

### D-6. Production exits by class (B-212)

`exit_point_clear` reads the product's class (from the product's archetype in its default stance: rule 3, else rule
4; a product is never in a mode) and skips every occupant whose class (D-2, from the occupant's `MotionPolicy`) is
different, in place of the layer test. Effects: a ground product's exit ignores burrowed bodies and colossus
analogues; a colossus analogue's exit ignores ground bodies; air unchanged. `rebuild_production_occupancy` keeps
every live, unhidden unit (its largest radius stays a valid bound). WP2 lands this before the data, so its moves (if
any) are separable from B-209's.

### D-7. Targeting and melee rings beneath a colossus

- **Can zerglings attack a colossus from beneath?** Yes. Melee range is measured between edges (`within_band`: centre
  distance at most `max_range + r_a + r_t`, no near limit at `min_range` 0), so a zergling whose disc lies under the
  colossus' is in band, swings and strikes, as in 5.0.14. Nothing changes in combat.
- **Its attack ring.** A zergling's ring round a colossus analogue is the circle at `rho = r_t + r_z + range`
  (1.0 + 0.375 + 0.1); `scan_ring` lists blockers of the zergling's class only, so other colossi no longer block it and
  only zerglings fill it. A zergling stops on reaching its band, so most stand at the rim. Friends that press into a
  full rim may push a zergling inward, under the colossus; it stays in band (no near limit) and keeps swinging. No
  ring rule is added.
- **The colossus' own fire at a unit beneath it.** The thermal lance's line (`origin: Target`, `axis: Across`) uses
  the attacker-to-target direction; at a zero offset `attack_direction` uses the attacker's heading, and
  `is_within_firing_arc` accepts the target. Tested (A-8), not changed.
- **Acquisition** does not read collision; unchanged.

### D-8. Force Field

No Force Field exists in the catalog (the Sentry analogue has no such ability). The 5.0.14 Force Field needs a set
that meets several classes (§2.2), which D-1 cannot express; §14 records the generalisation it needs.

### D-9. The data (WP3)

`tripod_annihilator` gains `collision_class: Some("tall_walker")` with the comment of D-2. No other row changes. The
class name is data, chosen for the analogue's description ("towering … war walker"); the 5.0.14 flag it stands for is
cited beside it.

### D-10. Records

`DESIGN.md` §4: B-209 registered in WP0 (text as on `enemy-contact-plan`, pointing here), fixed in WP3; B-211 and
B-213 registered in WP0 (deferred); B-212 registered in WP0, fixed in WP2. The Movement row of `DESIGN.md` §3 names the
collision class. The enemy-contact plan gets a pointer line when it is next amended (not by this plan's branch).

---

## 5. Alternatives rejected

| Alternative | Why not |
|---|---|
| **Flag sets with intersection** (5.0.14's exact model: `collision_flags: ["colossus"]`, a `u16` mask, touch when masks share a bit) | Every shipped unit reduces to equality (§2.4); intersection needs a body to sit in several steering lists or one shared list with a mask filter, which changes iteration order for every world (so WP1 would not be latent) and costs a scan of foreign bodies. Kept as the upgrade path (§14) |
| A `passes_through_ground: bool` capability on the archetype | A second ghost-like flag beside `passes_through_units` that would still need colossus–colossus contact as a special case; not a class |
| Making the colossus analogue a `CliffWalker`-specific rule in code | Branches on a mover variant for a collision fact (charter §3.3: capabilities, not identities); cliff walking is a separate mover feature (B-213) |
| An enum of named classes in `sim_types` (`Colossus`, `Burrow`, …) | A unit-specific variant in Rust (charter §3.3, C5's intent); names belong in data |
| Class names declared in `movement_rules.ron` | Adds a second place to edit for no rule it enforces; the names a catalog uses are the declaration, sorted at load |
| Re-using `burrowed: true` for the colossus | `burrowed` also drives concealment and the snapshot's burrowed look; a colossus is neither |

---

## 6. Determinism and checksum

- **No hashed field.** The class is derived at each tick from the catalog and the hashed `active_mode`, as `burrowed`
  is; `MovementWorkspace` is scratch (the checksum module's docs exclude it). Schema unchanged.
- **Order.** Class indices from a sorted set; bodies enter lists in entity-id order; pairs in the grid's order as
  today. No float in the simulation (C7); the diagnostics' `f64` stays in `sim_fixtures`.
- **Catalog identity.** A catalog with the field and one without are different catalogs; the checksum does not cover
  catalogs (its module docs). A replay recorded before WP3 replays identically with the catalog it was recorded with.
- **Goldens.** `golden_replay.ron` (Marine, Roach, Zealot, Siege Tank analogues), `golden_replay_orders.ron` (Marine,
  Roach, Zealot, Phoenix analogues) and `DEFAULT_SANDBOX_CHECKSUM` (the default scenario) contain no colossus analogue
  and no producer next to a burrowed body: **predicted byte-identical in every package.** WP4 holds the plan's one
  re-record slot on the final tree, used only if a golden moved and its first difference was traced to this plan's
  rule and judged correct (stop rule 6 otherwise).

---

## 7. Acceptance (pre-registered)

No acceptance is a figure from a seed. Constructed tests find archetypes by property (an injected scratch catalog,
`runtime_catalog_tests`' pattern, or "the archetype whose `collision_class` is authored"), never by slug.

### 7.1 Constructed tests

**WP1 (mechanism, latent):**
- **A-1 (shipped classes unchanged).** For every archetype and mode of the shipped catalog, the resolved class equals
  the old `body_class()` (ground 0, air 1, burrowed 2), the burrowed Lurker analogue included.
- **A-2 (resolution order).** A scratch archetype with an authored class and two modes (one authoring a class, one
  `burrowed: true` without): default stance → archetype class; first mode → its class; burrowed mode → `BURROWED`;
  while channelling into a mode → the old stance's class.
- **A-3 (validation).** Empty name, a character outside `[a-z0-9_]`, and one name on a ground and an air stance are
  each refused with their typed error; indices follow sorted names whatever the file order.
- **A-4 (no contact across classes, both pipelines).** A scratch body of class `x` and a default ground body placed
  overlapping by half their radius sum, idle, then walking through each other: both pipelines give zero correction to
  either (positions equal those of each alone in the same world), and neither appears in the other's steering
  neighbours. `ContactRules::pipeline` switched in the catalog, as `push_priority_tests` does.
- **A-5 (contact within a class).** Two class-`x` bodies are separated exactly as two default ground bodies of the
  same radii and priorities (bit-equal positions over 50 ticks), friend and enemy, holding and walking, D-δ on.
- **A-6 (rings).** A ground chaser's ring round a target is not blocked by a class-`x` body standing on it
  (`scan_ring` lists none); a class-`x` chaser's ring is not blocked by ground bodies; each is blocked by its own class.
- **A-7 (displacement admission).** A Blink or unload point overlapping a class-`x` body is admitted for a ground unit
  and refused for a class-`x` unit (and the converse).
- **A-8 (beneath and coincident).** A melee scratch unit placed at, and at a quarter radius from, a class-`x` enemy's
  centre: no `MovementInvariant` or `SimulationDiagnostic`, its swings start and land on the cadence of a melee unit at
  the rim; a class-`x` line-splash weapon firing at a unit at its own centre strikes along its heading's normal.
- **A-9 (the meter).** `ContactMeter` on a two-class scene counts no enemy overlap, touching or ring blocker across
  classes, and the same figures as before within a class.

**WP2 (exits):**
- **A-10.** A ground product's exit cell occupied by a burrowed body, or by a class-`x` body, is used; occupied by a
  ground body, it is skipped (today's order of cells otherwise). A class-`x` product skips class-`x` occupants only. Air
  unchanged. The B-212 pin test (red on WP1's tree) turns green.

**WP3 (data):**
- **A-11 (walk over a crowd).** The archetype that authors a class (found by property), ordered to move through a
  packed 6 × 6 block of idle friendly, then enemy, default-ground units: it arrives on the tick a solo walk of the same
  route arrives, and no block member moves (both pipelines).
- **A-12 (crowd beneath).** Twenty melee units attack-moving to a lone class-authoring enemy: none is held by its body,
  every one that reaches its band swings, and the most deeply overlapped pair reports no invariant.

### 7.2 Behaviour checks on measured worlds (B1–B7, as the enemy-contact plan's §7.1 and Amendment 2)

Measured per battle over the **24 seeds** (four of record and the matrix tool's first 20 non-record seeds) against
the **WP0 base**, with the diagnostics of `solver-pair-cache` after D-3's R-8 change. "Beyond noise": the candidate's
per-seed mean outside the base's 95 % bootstrap interval (2,000 resamples, seed 7), on the worse side. Judged only on
battles whose bytes move (§7.5).

| Check | Broken when (after WP3; for WP1 and WP2 every world is byte-identical instead) |
|---|---|
| **B1, standing with room** | D-R10 `free` or `Blocked` unit-seconds above the base beyond noise, in a battle without the B-192 marker |
| **B2, shoving** | D-SHOVE-E of either side above the base beyond noise (the colossi's is expected to fall: zerglings no longer touch them) |
| **B3, overlap with bodies that collide** | Largest overlap of two enemy bodies **of one class** above the base's, or such pair-ticks above T7's enemy bound above the base beyond noise. Overlap across classes is the fix and is reported, not judged |
| **B4, retarget churn** | D-CHURN `unexplained` above the base beyond noise |
| **B5, give-ups** | Give-ups of automatic chases above the base beyond noise |
| **B6, friend overlap** | Friend pair-ticks of one class above the moving bound above the base beyond noise (the colossi's and the zerglings' each) |
| **B7′, firing** | Shots per engaged unit-second of either side below the base beyond noise; shots per living unit-second reported |
| **B8, standing while able to fight** (new) | A scratch probe (§10.1 (a)) counts unit-ticks where a living armed unit has an available enemy in band, its weapon `Ready` at cooldown 0, starts no swing, does not turn and does not move. Above the base beyond noise is a failure; any such tick whose cause is a body of another class is a failure outright |
| **B9, no cross-class correction** (new) | Any solver or legacy correction between bodies of different classes (probe (b)); must be 0 everywhere |

### 7.3 Battle classes and the close-battle list (fixed in WP0)

WP0 computes, from WP0's 24-seed base and nothing else, and commits to the evidence file before WP1's first build:
1. every matrix battle whose minority side wins at least 3 of the 24 runs (≥ 10 %);
2. every battle the matrix fixture or a closed question already names close or restated under §8 (from `grep -n
   "close\|[Rr]estated" crates/sim_core/tests/fixtures/matchup_matrix.ron`: 10, 27, 33, 35, 36a, 36b, 38b, 39a on
   `be58ebc7`, and 26, whose restatement comments sit inside its entry), and the enemy-contact base's list (11a, 14, 21, 22a, 26, 28, 31, 32, 33, 35, 37a, 37b, 38b, 39b, 42,
   38a);
3. no other. Battle 28 carries `known_bug: Some("B-192")`; it contains no colossus and must stay byte-identical (stop
   rule 5).

**Class R** is that list; **Class D** every other battle. The list is not edited after WP1 starts.

- A Class R battle whose **outcome** moves (a winner changes on a seed, or an expectation fails where it held) is
  **restated** under charter §8 if every check of §7.2 passes for it: a first-difference trace of one flipped seed to
  the entity and rule (`firstdiff.py` and `matchup_trace --battle … --seed …`), the range re-derived over the seeds of
  record and the 20 development seeds, the claim text rewritten where the winner changes, and a fixture comment
  citing AGENTS.md §8, Q0103, this plan, B-209 and the mechanism.
- **Battle 26** is Class R. Its claim ("zerglings that pack round each colossus") describes contact that 5.0.14 does
  not have; if it moves, it is restated as above, and its comment says the zerglings now reach the colossi beneath
  and between them. It takes no `known_bug` marker: B-209 is fixed.
- Any **Class D** outcome move is a stop for review (stop rule 4).
- **Any §7.2 failure** is a stop, whatever the class: one amendment attempt within the package, then a register row
  and the package stops before merge.

### 7.4 Held-out check (confirmation only)

Run once, in WP4, on WP0's tree and the final tree: the matrix tool's 21st to 44th non-record seeds (all battles).
Every restated claim must hold on at least two thirds of them; B1–B9 must show the development seeds' directions; no
Class D outcome moves. A contradiction is a stop (rule 7). Nothing is widened, restated or tuned from them.

### 7.5 Predictions (recorded before measuring; not thresholds)

| World | WP1 | WP2 | WP3 |
|---|---|---|---|
| Matrix, all battles, both pipelines | Byte-identical (CSV incl. check columns) | Byte-identical (no producer) | 26 moves on every seed; 12a and 12b may move by steering only (ranged sides rarely touch the colossus), outcomes kept; 12c (air vs one holding colossus) and 35 (air vs colossi, which still touch each other) byte-identical; every other battle byte-identical |
| Battle 26 | — | — | Zerglings win at least as often as on the base (18/24 before B-208, 17/24 after it); colossi's D-SHOVE-E falls towards 0; enemy overlap of one class: colossus pairs only |
| Harness families (gap, cross, corner, arc, mirror) dev and held-out | Byte-identical | Byte-identical | Byte-identical (no colossus) |
| Battles 31, 33; showcase and showcase effects (default seed, 1–8) | Byte-identical | Byte-identical (**verify** no burrowed body beside a producer; the census of probe (c)) | Byte-identical (no colossus) |
| Goldens, sandbox | Byte-identical | Byte-identical | Byte-identical |
| Client skirmish stage log | Identical | Identical | Identical (the skirmish trains no colossus analogue; **verify**) |

---

## 8. Stop rules

1. **Base.** WP0 starts only on `main` after `solver-pair-cache` (B-208) merges; if it has not merged, WP0 waits.
   WP0 is records only.
2. **Latency.** WP1 not byte-identical to WP0 on every world of §7.5 (matrix CSVs with their check columns, harness,
   battles 31 and 33, showcases, goldens, sandbox, both pipelines): a defect; stop and find it.
3. **WP2's reach.** WP2 moves a world without a burrowed body or authored class beside a producer: stop.
4. **One cause.** In WP3, a world without a colossus analogue moves, or a moved world's first difference is not a
   pair, steering neighbour or ring blocker removed between a colossus analogue and a ground body: stop and trace. A
   Class D outcome move: stop for review, with the trace; not restated by the implementer.
5. **Battle 28** (B-192) moves at all in any package: stop.
6. **Goldens or sandbox move** in any package: stop and trace; the WP4 re-record slot is used only after that trace,
   in an amendment.
7. **A §7.2 check fails,** or the held-out run contradicts the development seeds: stop; report both. No iteration on
   held-out seeds.
8. **Broken behaviour** (B8: a unit standing while able to fight because of a body of another class; B9 non-zero): a
   stop, never restated (charter §8).
9. **Throughput** below 50 ticks/s on any scenario, or instructions above §11's budget: stop and profile.
10. **Not allowed:** a constant, a per-unit or per-battle switch, a tolerance chosen after seeing figures, a change to
    the solver's pair rules, or code that names a unit.
11. **Overlap.** No package touches the enemy rules parked on `enemy-contact-plan`; if that branch merges first, this
    plan's WP0 re-derives its base on the merged tree (amendment).

---

## 9. Client consequences

- A colossus analogue walks through ground units and they through it; both are drawn where they stand. The client
  already draws overlapping air and ground units; a zergling under a colossus is drawn beneath it (the colossus
  analogue's model is taller). **Verify** in WP4 with one capture (charter §7: physical GPU, `with-gpu-lock`, one
  capture) that a unit beneath is visible and pickable (picking by screen depth or by nearest centre: whichever the
  client does today). A defect found is registered (from B-214), not fixed here.
- No client code is expected; the package gate runs the client tests its path rules select.

---

## 10. Measurement

### 10.1 Probes (WP0, scratch, not committed; `~/scratch/collision-classes/`)

- **(a) Standing while able (B8):** after every tick, the count of §7.2's B8 condition per side, and for each such
  unit-tick whether any body touching it is of another class.
- **(b) Cross-class corrections (B9):** in both pipelines, a counter of corrections between bodies of different
  classes (0 by construction after WP1; the probe proves the construction on WP3's tree).
- **(c) Census:** matrix battles, scenarios, harness families and goldens that contain a colossus analogue, a burrowed
  body, or a producer; for WP2, every exit search that met a burrowed occupant.

### 10.2 Commands

The enemy-contact plan's scripts, copied to `~/scratch/collision-classes/scripts/` (`measure.sh`, `checks.py`,
`compare.sh`, `firstdiff.py`, `proof.sh`), one release binary per arm, each built in its own fresh target directory:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/collision-classes-<wp>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
mkdir -p ~/scratch/collision-classes/bin-<wp> && cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} ~/scratch/collision-classes/bin-<wp>/
flock -o ~/scratch/gate.lock ~/scratch/collision-classes/scripts/measure.sh ~/scratch/collision-classes/bin-<wp> <tree> ~/scratch/collision-classes/world-<wp>
python3 -I ~/scratch/collision-classes/scripts/checks.py ~/scratch/collision-classes/world-wp0 ~/scratch/collision-classes/world-<wp>
```

`measure.sh` runs the matrix on 24 seeds (Solver and Legacy), the harness families (development 1–1,000; held-out
9,000,001–9,001,000 in WP4 only), battles 31 (100 seeds) and 33 (64 seeds), the showcases (default seed and 1–8). One
run at a time across worktrees, in the background, never beside a `cargo gate`.

---

## 11. Throughput

`cargo bench -p sim_core --bench throughput` on WP0's tree and the final tree, release, `perf stat -e instructions:u`,
three runs, both pipelines, at the load the guide names:
- **Every scenario at 50 ticks per second or more** (the task's bar; the charter's floor is 25).
- **Instruction budget: within +1.0 % of WP0 on every scenario** under the solver pipeline (Legacy reported). The change
  is a `Vec` lookup in place of a fixed array and one byte comparison per candidate pair; no scenario of the bench has
  an authored class, so the measured cost is the indirection only. A scenario above +1.0 % is stop rule 9.
- Reported, not judged: battle 26's ticks per second on the base and the final tree (fewer pairs are expected to make
  it cheaper).

---

## 12. Work packages (commit order)

One branch from `main` after `solver-pair-cache` merges, one worktree, `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`;
one commit per package; failing tests first, the red run pasted in the hand-back; `cargo gate quick` and the tests the
change needs; one `cargo gate package` by a verifier on the final tree (Q0061, charter §4.3). No `unwrap`; `expect`
only with `invariant:`; no catalog identity in production Rust; every new file starts with `//!`. Check
`docs/questions/open/` before the last commit.

### WP0: records, baseline, close-battle list, probes (S, docs and scratch)
- `DESIGN.md`: B-209 (from the parked branch's row, pointing here), B-211, B-212, B-213 registered.
- Baseline of §10.2 on WP0's tree; probes (a) to (c); throughput baseline; the close-battle list (§7.3) and the
  census committed to `docs/evidence/2026-11-25-collision-classes.md` with the commands that derived them.

### WP1: collision classes, latent (M, `sim_core`, `sim_fixtures`)
- D-1 to D-3 (`CollisionClass`, the field on archetypes and modes, validation, resolution, R-1 to R-6, the accessor and
  R-8). Tests A-1 to A-9, red first.
- Measure: every world of §7.5 byte-identical to WP0 (stop rule 2).

### WP2: production exits by class (S, `sim_core`; B-212)
- D-6; A-10 (with the B-212 pin test red on WP1). Measure: §7.5's WP2 column; stop rule 3.

### WP3: the colossus analogue's class (S, data; B-209)
- D-9 in `units.ron`; A-11, A-12. Measure against WP0 and WP2; §7.2 B1–B9; §7.3 for outcomes; restate Class R battles
  that move, with traces, in this commit. B-209 fixed in `DESIGN.md`.

### WP4: confirmation and records (S)
- Held-out run (§7.4); throughput (§11); goldens and sandbox run on four builds (native debug and release,
  `wasm32-wasip1` debug and release; `proof.sh`); the one re-record slot used only under stop rule 6; the client
  capture of §9; `DESIGN.md` Movement row; README row; As built.

---

## 13. File ownership

| File | WP | Change |
|---|---|---|
| `DESIGN.md` | WP0, WP2, WP3, WP4 | Rows B-209, B-211 to B-213; fixed marks; Movement row |
| `docs/evidence/2026-11-25-collision-classes.md` (new) | WP0–WP4 | Baseline, close list, census, probes, comparisons, traces, held-out, throughput |
| `crates/sim_core/src/movement/mod.rs` | WP1 | `CollisionClass`, `MotionPolicy::class`, `touches`, workspace `Vec`s, `validate_placement_unoccupied`, `SimWorld::collision_class` |
| `crates/sim_core/src/movement/solver.rs`, `collision.rs`, `locomotion.rs`, `ring.rs` | WP1 | R-1 to R-5 |
| `crates/sim_core/src/catalog.rs`, `catalog/validation.rs` | WP1 | Field on `UnitArchetype` and `UnitModeSpec` (`UnitModeSpecDef`), class table, typed errors |
| `crates/sim_types/src/commands.rs` | WP1 | `UnitModeSpec::collision_class` (the resolved mode spec lives in `sim_types`; its RON form `UnitModeSpecDef` in `sim_core::catalog`) |
| `crates/sim_core/src/production/exits.rs` | WP2 | D-6 |
| `crates/sim_fixtures/src/trace/contact_checks.rs`, `harness/room.rs` | WP1 | R-8 |
| `crates/sim_core/tests/collision_class_tests.rs` (new) | WP1–WP3 | A-1 to A-9, A-11, A-12 (subject "movement"; `crates/gate/suites.ron` needs no entry: unlisted targets are `fast`) |
| `crates/sim_core/tests/production_transactions_tests.rs` or the exits unit tests in `production/exits.rs` | WP2 | A-10 |
| `crates/sim_fixtures/tests/trace_tests.rs` | WP1 | A-9 |
| `assets/catalog/units.ron` | WP3 | D-9 |
| `crates/sim_core/tests/fixtures/matchup_matrix.ron` | WP3, only under §7.3 | Restated Class R battles with traces |
| `crates/sim_core/tests/fixtures/golden_replay*.ron`, `crates/sim_fixtures/tests/fixture_tests.rs` | WP4, only under stop rule 6 | The re-record slot |
| `docs/plans/README.md` | plan, WP4 | Row |

---

## 14. Out of scope and decisions recorded (Q0002)

- **Flag sets with intersection** (§5): needed by Force Field, Locust, burrowed Infestor movement, Interceptors and
  the Colossus' `Flying`. Upgrade path: `CollisionClass` becomes a bit set authored as a list of names, `touches`
  becomes an intersection, and steering lists are keyed by set with a precomputed "touches" table between sets. Taken
  up with the first unit that needs it.
- **B-211**, the Colossus' `Flying` flag (§2.3, Q0114 draft): deferred; default no contact with air bodies.
- **B-213**, cliff walking and cliff jumping: deferred; a mover feature (plan `45-round6-authored-movement-layers`).
- **B-193, B-192** (enemy contact, ring retarget): parked under Q0113; untouched. This plan removes colossus–zergling
  pairs, so a third enemy-contact attempt measures battle 26 without them.
- **Structures' own `Collide` sets** (placement, landing): footprints already model what the roster needs.

---

## 15. Owner question (filed as [Q0114](../questions/closed/0114-colossus-and-air-units.md))

```markdown
# Q0114: Should a colossus touch air units?

- **Asked:** 2026-11-25
- **Blocking:** no
- **Default until answered:** (a), no contact between the colossus analogue and air units, as today (B-211 open).

## Question

In StarCraft II 5.0.14 the Colossus' collision flags are `Colossus`, `Structure` and `Flying` (`liberty`
UnitData:4066–4068). Plan `2026-11-25-collision-classes` gives it its own collision class, so ground units walk
beneath it and colossi block only colossi. The `Flying` flag would also make it touch air units (Vikings, Phoenixes,
Medivacs above it), but a Blizzard developer answer from before release says the Colossus does not block flying units,
and our movement keeps ground and air bodies apart by design.

- **(a) No contact with air units (default).** Simple; matches the developer answer; B-211 stays registered as an
  unverified difference.
- **(b) Contact with air units.** Needs a collision model where one body touches two kinds of bodies (flag sets),
  which changes how steering lists are built; a separate plan.

Recommendation: (a).

## Answer
```

---

## 16. Material (scratch, not committed)

- The 5.0.14 resolution script used while planning: `~/scratch/collision-classes/scripts/collide.py` (it reads each layer's `UnitData.xml` from `core` to
  `balancemulti`, follows `parent=`, and prints each unit's effective flags with layer and line). Run with
  `python3 -I collide.py ~/scratch/combat_wp3_data Colossus Zergling …`.
- `~/scratch/collision-classes/` for binaries, worlds, probes and traces.

---

## As built (implementation agent, 2026-11-25)

Branch `collision-classes-plan`, from `main` `f390c4a2` (B-208 landed) and merged with `main` `91350af1` (rallies,
schema 48). Figures in evidence [`2026-11-25-collision-classes`](../evidence/2026-11-25-collision-classes.md).

| WP | State | Commit |
|---|---|---|
| WP0 | Done: B-211, B-212, B-213 registered; base `0d3c6d0b`, the close-battle list (21 battles in Class R) and the census fixed before WP1's first build (evidence §1) | `cd95a3ad` |
| WP1 | Done, latent: D-1 to D-3 and R-8; every measured world byte-identical to WP0 under both pipelines (evidence §2); A-1 to A-9 red first | `6bcb6f8b` |
| WP2 | Done: D-6, B-212 fixed; every measured world byte-identical; A-10 red first | `82723f29` |
| WP3 | **Parked** on branch `collision-classes-wp3-wip` (head `42761cf2`), reverted here: only battle 26 moved (zerglings win 24 of 24, two seeds of record above the range), and B4, B6 and B7′ failed against the WP0 base, so it was not restated (§7.3, stop rule 7); pending the owner's question Q0117 | `d0c67ceb`, reverted |
| WP4 | Not run: held-out seeds and the client capture belong with WP3; throughput and the golden proof were measured on WP3's tree and the merged tree (evidence §4) | — |

**Deviations from the plan as written:**
- `CollisionClass` is defined in `sim_types` (`ids.rs`) and re-exported as `sim_core::movement::CollisionClass`, because
  the resolved mode spec `UnitModeSpec` lives in `sim_types` (§13).
- A-10 is a unit test in `production/exits.rs` using a `#[cfg(test)]` catalog helper
  (`UnitCatalog::author_collision_class_for_test`) to author a class on an edited archetype.
- A-1 checks the archetypes and modes that author no class (on WP3's tree the colossus analogue's class differs by
  design). A-8 and A-12 pick melee weapons that fire only at rest (`AllowedMovement::Stopped`); A-12 orders the attack
  explicitly on an unarmed, holding target (an unarmed unit is no automatic target), and reads "not held by its body"
  as: every unit outside its band still walks or touches a friend. A-11 and A-12 live on the parked branch.
- The schema stays at 48: no hashed state was added.
