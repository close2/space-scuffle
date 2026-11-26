# Blueprint: a Render-Entity Pool for Units that Come and Go (B-237)

This plan fixes one row of `DESIGN.md` §4:

- **B-237:** "Units that arrive or leave during a battle allocate in the client": `sync_live_battle_system`
  (`crates/client/src/scene_controller.rs`) spawns a render entity with its own `BubbleDomeMaterial` (and its
  indicator children with three more materials) for each arrival and despawns it on departure, and `RenderEntityMap`, a
  `BTreeMap`, inserts and removes a node each time. A battle in which Broodlings expire, or Carrier interceptors launch
  and dock, is therefore not allocation-free in steady state (charter §3.3). The broodlings plan's test B-19 measured
  6 bubble materials added and 6 removed in a 300-tick window and is kept as `~/scratch/broodlings/b19-test.patch`; the
  Carrier plan's H-20 needs the same fix.

The design in one paragraph: a departed unit's render entity is **parked**, not despawned: it loses its
`LiveBattleBubble` marker, gains a `ParkedBattleBubble` marker, is hidden, and has its client-local presentation state
(its `StanceTrack`, whose removal releases the looks it held, and `NoAutomaticBatching`) removed. An arrival takes a
parked entity if there is one and **restyles it completely** from the arriving record through the same pure functions a
fresh spawn uses, so a reused bubble is a fresh one by construction; only when none is parked is a new entity spawned.
Every bubble gets the same set of indicator children (the air shadow and tether exist, hidden, on ground units too), so
any parked entity can take any arrival. `RenderEntityMap` becomes a retained, sorted vector of `(EntityId, Entity)`
walked in one merge with the sorted snapshot, so neither membership change nor lookup allocates. Its storage is reserved
at session start to a **bound derived from the session and the catalog** (supply cap, hangar size, strike magazine and
timed life); beyond the bound the pool still spawns, so no unit is ever undrawn, and says so once per session in the log
and in a counter. No simulation file changes.

**Status of this document:** a stage 1 blueprint under charter §6, written against `main` at `00330fcc`, with branch
`broodlings-plan` (`4dc37641`, merging now: the Brood Lord analogue `spore_matriarch`, its Broodling `brood_mite` and
their installed media) and plan [`2026-11-28-carrier-interceptors`](2026-11-28-carrier-interceptors.md) (merged; its
media are being generated now on another package) read for coordination.

- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.
- **Numbers are provisional, next free at merge.** New bug rows start at **B-238**; the draft question is **Q0134**
  (§13, not filed by this commit).
- **Prerequisite.** The broodlings plan is on `main` before WP0 (B-19 stages its Brood Lord analogue and needs the
  Broodling's media). H-20 has a prerequisite of its own (§9).

---

## Contents

1. [Evidence: what allocates today](#1-evidence-what-allocates-today)
2. [Requirements and charter mapping](#2-requirements-and-charter-mapping)
3. [Patterns already in the client](#3-patterns-already-in-the-client)
4. [Design decisions](#4-design-decisions)
5. [Who sees a parked entity: the consumer audit](#5-who-sees-a-parked-entity-the-consumer-audit)
6. [Tests](#6-tests)
7. [Acceptance](#7-acceptance)
8. [Work packages](#8-work-packages)
9. [Sequencing with the Carrier's media (H-20)](#9-sequencing-with-the-carriers-media-h-20)
10. [Stop rules](#10-stop-rules)
11. [Files](#11-files)
12. [Out of scope and recorded decisions](#12-out-of-scope-and-recorded-decisions)
13. [Bug and question drafts](#13-bug-and-question-drafts)

---

## 1. Evidence: what allocates today

Read on `main` `00330fcc`.

| # | Where | What happens on an arrival or a departure |
|---|---|---|
| E-1 | `scene_controller.rs`, `sync_live_battle_system` | Each snapshot record with no entry in `RenderEntityMap` calls `spawn_live_instance`, which `BubbleSceneAssets::spawn_bubble`s an entity with `bubble_materials.add(BubbleDomeMaterial::new(..))`. |
| E-2 | `indicators.rs`, `attach_bubble_indicators` | Every spawn adds a `StandardMaterial` for the team colour (`team_mat`), one for the air shadow (`shadow_mat`), and a `SelectionRingMaterial`, and spawns 5 children (ring, bar root, background, HP bar, shield bar), plus shadow and tether only when the record is airborne at spawn (`world_pos[2] > ground_z + 0.5`). A ground bubble's `shadow_mat` (and `team_mat`) is created and dropped at once, because no child holds it: an add and a remove for nothing. |
| E-3 | `scene_controller.rs`, `despawn_departed` | Each departed entry is `despawn_recursive`d, which drops the bubble's material, its ring material and its team material (three `AssetEvent::Removed`), and despawns 5 to 7 entities. |
| E-4 | `scene_controller.rs`, `RenderEntityMap(pub BTreeMap<EntityId, Entity>)` | An insert can split a B-tree node and a removal can merge one; ids grow monotonically (`SimWorld::next_entity_id`), so a cycle of arrivals at the right end and departures in the middle splits and frees nodes for ever. Its PERFORMANCE RATIONALE ("zero heap allocations during steady-state ticks") holds only while membership is constant. |
| E-5 | `effects/stance.rs`, `present_stances` | A bubble seen for the first time gets a `StanceTrack` by `commands.insert`; its `on_remove` hook (`release_held_looks`) releases the looks it held when the entity is despawned. `NoAutomaticBatching` is inserted and removed as a shell moves the shadow caster. |
| E-6 | B-19 (`b19-test.patch`) | With a Brood Lord analogue attacking a durable unarmed target, 400 warm ticks then a 300-tick window: 6 `Assets<BubbleDomeMaterial>` added and 6 removed. The patch counts only bubble materials, so E-2's ring and standard materials are not even in its figure. |

The valgrind test (`sync_allocation_count_tests.rs`) did not catch this because no child has an arrival or departure in
its window (its running children are idle but for a gas crew that stays in the snapshot, `housed`). And its gate
triggers do not name `scene_controller.rs` or `indicators.rs` (`crates/gate/suites.ron`, the
`client/sync_allocation_count_tests` entry), so a change to the synchroniser itself does not select it; WP1 adds them
(§8).

Facts the design relies on, each verified in the code:

- The snapshot's `bubbles` are sorted by `entity_id` (`despawn_departed`'s merge already relies on it).
- Simulation ids are never reused within a session (`next_entity_id` only grows), and ids restart with a new session
  (`BattleSelection::bind_generation`).
- Only `present_stances` inserts components on a live bubble after its spawn (`StanceTrack`, `NoAutomaticBatching`);
  nothing else in the client keeps a Bevy `Entity` of a live bubble (the Lab's `SelectionState::selected_entity` is the
  Lab preview's). Effects find bubbles by simulation id (`effects::struck`, `HeldBeams`).
- `refresh_battle_fog` runs `.after(sync_live_battle_system)` and `present_stances` and `update_indicators_system` in
  `BattleSet::Present`, so Bevy's automatic sync point applies the synchroniser's commands before any of them runs
  (**verify** in WP0 with `support/schedule_order.rs`, which the client tests already use for such claims).

## 2. Requirements and charter mapping

| Requirement | Where met |
|---|---|
| Pool of render entities with their materials: hide on departure, reuse on arrival | D-1, D-2, D-5 |
| Size by a derived bound, never a magic number | D-3 |
| Map storage that does not allocate per arrival | D-4 |
| A reused bubble takes the new unit's look, with no stale health ratio, snapshot record or animation clock | D-5, D-6; R-3, R-8 |
| Picking and selection never see parked entities | D-2, §5; R-4 |
| Effects attached to a unit are released or reassigned | D-7; R-5 |
| Exhaustion defined and tested; a unit's drawing is never dropped | D-8; R-6 |
| B-19 and H-20 as steady-state and valgrind tests | R-1, R-2, H-20 (§9) |
| No simulation change; skirmish and showcase keep passing; frame budget reported | §7 |
| Charter §2.3 | every retained structure carries a PERFORMANCE RATIONALE (D-3, D-4, D-6) |
| Charter §3.3 zero hardcoded identities | the bound and every test find units by property (strike magazine, hangar, mover type), never by slug |
| Charter §1.2 | no test skips; H-20 is not written as a skipping test while the Carrier has no media (§9) |

## 3. Patterns already in the client

The pool reuses three established patterns rather than inventing a fourth.

- **`AttackSnapshotPool`** (`battle_hud/attack_snapshots.rs`): pooled bubble entities, hidden when unused, restyled
  only when what they show changes; a pooled entity carries `AttackSnapshotBubble` and never `LiveBattleBubble`, so
  every consumer that queries `With<LiveBattleBubble>` never meets it, and the smog excludes it by its marker. It grows
  only when a list is longer than every list before it, and exposes `retained_buffers()` (`#[doc(hidden)]`) for the
  steady-state tests. D-2 is the same marker discipline; D-4's `retained_buffers` the same hook.
- **`CombatFxMaterialPool`** (`vfx/pool.rs`): material slots that are never removed within a session, so once the pool
  has grown to its peak no `AssetEvent` is raised again. D-1 keeps each bubble's materials with its entity the same
  way.
- **`HeldBeams::capacity_for`** (`effects/beams.rs`) with `units_per_player`: a table size derived once from the
  catalog through the standard supply cap, `MAX_PLAYERS`, each unit's supply cost ("a free unit counts as costing 1")
  and a hangar's `max_count`. D-3 derives the pool's bound the same way and adds the strike magazine and timed life.

## 4. Design decisions

### D-1. Pool entities, not materials alone

The unit of reuse is the whole render entity: the bubble with its `BubbleDomeMaterial`, its indicator children and
their materials. Pooling only the materials (as `CombatFxMaterialPool` does) would still spawn and despawn 6 to 8
entities per arrival, churn Bevy's entity and table storage, and re-create the ring and team materials. A parked entity
keeps every handle, so Bevy keeps every prepared GPU material; hidden, it is neither extracted nor drawn.

### D-2. Parking by marker swap

On departure (the merge of D-4 finds a map entry whose id left the snapshot), in one `EntityCommands` chain:

1. `remove::<(LiveBattleBubble, StanceTrack, NoAutomaticBatching)>()`; `StanceTrack`'s `on_remove` hook releases every
   look the unit held, exactly as its despawn did (D-7);
2. `insert((ParkedBattleBubble, Visibility::Hidden))`;
3. `BattleSelection::forget(id)` as today; the entity goes onto the parked list.

On reuse, the reverse swap: `remove::<ParkedBattleBubble>()`, `insert((LiveBattleBubble { entity_id }, Visibility::Inherited))`,
after D-5 has written every component in place. `Visibility::Inherited` is what a fresh spawn has; the fog then decides,
on the same frame, as it does for a fresh spawn.

`ParkedBattleBubble` is a new public marker in the new module `client::render_pool`. Why a marker swap rather than a
flag on `LiveBattleBubble`: every consumer that must not see a parked entity already filters `With<LiveBattleBubble>`
(§5), so removing the marker excludes it everywhere at once, as `AttackSnapshotBubble` does; a flag would need a guard
in each consumer, and the fog, which writes `Visibility` to every `LiveBattleBubble`, would un-hide parked entities.

Cost: an archetype move per arrival and per departure. Bevy caches archetype edges after the first move and table
columns keep their capacity, so a move between archetypes already seen allocates nothing (**verify**: WP0's probe P-0
measures it under DHAT before anything is built; stop rule 1 if it allocates).

A departure and an arrival on the same frame may use the same entity: departures are processed before arrivals (D-4),
both swaps are queued in order, and the sync point applies them before any consumer runs, so the consumers see a live
bubble of the new unit with no `StanceTrack`, exactly as after a fresh spawn.

### D-3. The bound, derived from the session and the catalog

The pool reserves its storage once per session to a bound `B` on the bubbles the session can present at once. Notation:
`S` = `sim_core::production::STANDARD_SUPPLY_CAP` (200), `P` = the session's players
(`BattleSession::metadata.players`, the committed scenario's; not `MAX_PLAYERS`, because the session is known),
`cost(a)` = `max(a.supply_cost, 1)` (as `units_per_player`: a free unit counts as costing 1).

- **Start term** `N₀`: the number of records in the first presented snapshot of the session (the scenario's placement,
  read, not estimated).
- **Army term** `A = ⌊S / min cost(a)⌋` over the archetypes some archetype trains (`trains_units`): the most
  supply-bounded units one player can add.
- **Transient term** `T`: the most units one player's supply can keep in the snapshot that arrive and leave without a
  player's command. For a host archetype `h`:
  - **hangar:** `hangar(h) = max_count` if `h.capabilities.hangar` is set (launched members at once never exceed the
    hangar's size), else 0;
  - **impact spawn with a timed life:** for `h`'s weapons whose `impact_spawn` has `timed_life: Some((t, r))`, a spawned
    unit lives at most `L = t + r` ticks, so the units alive at any tick were created within the last `L` ticks.
    - With `h.capabilities.strike_magazine = Some((capacity c, build_ticks b, ..))` every strike spends one charge and
      carries `per_strike` units, and at most `c` charges are armed or building at a window's start and at most
      `⌈L / b⌉` are built within it: `spawn(h) = max per_strike × (c + ⌈L_max / b⌉)` over the spawning weapons.
    - Without a magazine: `spawn(h) = Σ per_strike × attacks_count × (⌊L / period_ticks⌋ + 1)` over the spawning
      weapons (one release per period, each with `attacks_count` strikes).
  - **impact spawn without a timed life:** its units live until killed, like trained units; it adds nothing to `T`, and
    if such units cycle the pool says so beyond the bound (D-8).
  - `m(h) = hangar(h) + spawn(h)`, and `T = max over h of ⌊S × m(h) / cost(h)⌋`. Because
    `Σ nₕ m(h) ≤ Σ nₕ cost(h) × max(m/cost) ≤ S × max(m/cost)` for any mix of hosts within the cap, `T` bounds every
    mix, not only an army of one host type (`HeldBeams` takes the largest single type, `⌊S / cost⌋ × m`, which a mix
    can exceed by a few units; the difference is recorded, D-3 keeps the true bound).
- **Bound** `B = N₀ + |P| × (A + T)`.

Shipped values, which WP1's test computes from the shipped catalog rather than pins: `A = 200` (cheapest trained unit
costs 1); the Carrier analogue (`hangar` 8, supply 6) gives `⌊1600/6⌋ = 266`; the Brood Lord analogue (magazine
capacity 2, build 45, `per_strike` 1, `L = 89 + 5 = 94`, supply 2 in the data) gives `1 × (2 + ⌈94/45⌉) = 5` and
`⌊1000/2⌋ = 500`, so `T = 500`; a two-player skirmish has `B = N₀ + 1400`. Three retained vectors of 16-byte entries
(D-4) at that size are about 70 KB per session.

Structures built during a match are not bounded by data (no supply); a session that builds very many of them exceeds `B`
and is reported (D-8), which is the honest outcome.

The bound is a pure function, `RenderPoolBound::for_session(catalog, players, start) -> RenderPoolBound`, with its terms
kept (`start`, `army`, `transient`, `host` naming the archetype that set `T`) so the diagnostic can say why `B` is what
it is. It reads the catalog of the presented session's world (`BattleSession::world()`), which the client may read for
presentation (charter §3.2). PERFORMANCE RATIONALE: Why: reserving once per session keeps the arrival path free of
vector growth. What: the three vectors are reserved to `B` when a new session generation is bound. Trade-off: the
reservation is an upper bound, larger than most battles use.

### D-4. `RenderEntityMap` as a retained sorted vector, walked in one merge

`RenderEntityMap` (moved to `client::render_pool`, re-exported from `scene_controller` and `client` under its current
paths) stops being a tuple struct over a `BTreeMap`. Its fields are private:

- `live: Vec<(EntityId, Entity)>`, sorted by `EntityId`;
- `parked: Vec<Entity>`, a stack;
- `arrivals: Vec<(EntityId, Entity)>`, the frame's arrivals, cleared each frame;
- `bound: Option<RenderPoolBound>`, `generation: Option<SessionGeneration>`, and `stats: RenderPoolStats`.

Its public methods: `get(EntityId) -> Option<Entity>` (binary search), `iter()`, `len()`, `is_empty()`, `parked()`,
`stats()`, `bound()`, `release_parked(&mut Commands)`, `retire_all(&mut Commands)`, and `#[doc(hidden)]
retained_buffers()` for the steady-state tests (as `AttackSnapshotPool`'s).

The synchroniser walks the sorted snapshot and the sorted `live` together, per frame:

1. **Departures first.** `Vec::retain` on `live` with the existing peekable merge of `despawn_departed`: an entry whose
   id is not in the snapshot is parked (D-2) and dropped from `live`, in place.
2. **Updates and arrivals.** A second merge: a record whose id matches the next `live` entry is updated in place
   (`update_live_instance`, unchanged); a record with no entry is an arrival: it takes `parked.pop()` and is restyled
   (D-5), or is spawned (D-8), and its `(id, entity)` is pushed onto `arrivals` (which is sorted, because the snapshot
   is).
3. **Merge arrivals into `live`.** If every arrival's id is above `live`'s last (the common case: new ids are the
   largest), `extend`. Otherwise (a unit carried in a transport and unloaded, if carried units leave the snapshot;
   **verify**), the classic in-place merge from the back: grow `live` by `arrivals.len()` and fill from the end.
   Neither allocates within the reserved capacity.

Options rejected:
- **A `BTreeMap` with node reuse:** `std`'s `BTreeMap` takes no allocator on stable Rust, so its nodes cannot be
  recycled; a custom B-tree is far more code than a sorted vector for a few thousand entries.
- **A slot vector indexed by a dense id:** simulation ids are dense only at the start of a session and grow without
  bound as Broodlings spawn, so a vector indexed by id grows for ever; a dense index would have to come from the
  simulation (a change to `RenderSnapshot`, out of bounds for this plan) or from a client-side map, which is this map
  again.
- **A `HashMap`:** forbidden by no rule in the client, but it allocates on growth and its iteration order is not the
  snapshot's, so the departure pass would need lookups instead of a merge.

PERFORMANCE RATIONALE: Why: membership changes with every arrival and departure, and a node-based map allocates on
those. What: a sorted vector reserved to the session's bound, walked in one merge with the sorted snapshot, so an update
needs no lookup and a membership change only moves entries within reserved capacity. Trade-off: a removal or an
out-of-order arrival moves `O(n)` 16-byte entries (a few microseconds at 2,000 units), and the map relies on the
snapshot's id order, which `despawn_departed` already does.

### D-5. Restyle: a reused bubble is a fresh one by construction

A reused entity is written with the values a fresh spawn of the same record would have, through the same functions,
never by patching what it had. `spawn_live_instance` and the new `reuse_live_instance` both start from the
`LiveBubbleFrame` of the record built with **`phase: None`** (a new `FlipbookPhase::in_stance(action, stance,
presented)`, so no animation clock carries over), and both use the new pure function `IndicatorPose::of(instance,
style)` for every child.

| State | Fresh spawn | Reuse |
|---|---|---|
| `BubbleInstance` | the record, `selected` from `BattleSelection` | overwritten with the same |
| `Transform` | `frame.translation`, scale `radius` | set to the same |
| `Mesh3d` | dome or sphere by shape | handle set by shape (a parked dome may become a sphere) |
| `FlipbookPhase` | `in_stance(..)` at `presented` | set to the same (never `observed_in` the old phase) |
| Material `uniform` | `BubbleDomeMaterial::new(frame.uniform, ..)`; optics lanes written by `sync_optics` in `PostUpdate` | the whole uniform replaced by `frame.uniform` with the material's current optics lanes kept (they are the session's global optics, which `sync_optics` would write anyway) |
| Material `video_texture` | the record's atlas | set to the same |
| Material `shell` | `ShellUniformGpu::default()` | reset to `ShellUniformGpu::default()` (no held look's sink, tint or veil survives) |
| `StanceTrack`, `NoAutomaticBatching` | absent until `present_stances` | removed at parking, so absent |
| `Visibility` | `Inherited` | `Inherited` |
| Ring child | transform, visibility, `SelectionRingMaterial` uniform (team colour, radius, selected) from `IndicatorPose` | the same, written into its existing material |
| Bar root, HP bar, shield bar | transform and visibility from `IndicatorPose`; HP bar at full scale with the green material | reset to the same (no stale health ratio: the HP bar's scale and material are rewritten even though `update_indicators_system` changes them only while selected) |
| Air shadow and tether | from `IndicatorPose` (D-6) | the same |
| Team material | colour from the owner's style | colour written |

`IndicatorPose::of` is the only source of child values for both paths, and `attach_bubble_indicators` becomes "spawn
children with `IndicatorPose::of`". Writes compare first (`set_if_neq`, `sync_bubble_material`'s pattern), so a reuse
modifies only what differs; a reuse modifies at most one bubble material, one ring material and one team material, and
adds or removes none.

### D-6. One indicator layout for every bubble

Today a bubble gets its air shadow and tether only if it is airborne at spawn, and they are never moved afterwards. For
any parked entity to take any arrival, every bubble now gets the same children: a ground bubble's shadow and tether
exist with `Visibility::Hidden`. `IndicatorPose::of` sets them visible, with today's transforms, exactly when today's
spawn would have created them. Consequences:

- A ground bubble keeps the `shadow_mat` and `team_mat` it already allocated and dropped at once (E-2), so the asset
  churn of E-2 ends too.
- Hidden children are not extracted or drawn; a ground bubble gains two ECS entities.
- `round2_visual_tests.rs`' assertion "Ground unit must never spawn AirTetherMarker / AirShadowMarker" becomes "a ground
  unit's air shadow and tether are hidden"; `player_presentation_tests.rs` keeps reading the tether's material.

Rejected: one parked list per layout (ground, air). It keeps today's children, but a pool split by layout holds the
high-water mark of each layout separately, so the pool can exceed the peak population and the bound of D-3 would need a
split nobody can derive from data; and an air arrival with only ground bubbles parked would spawn. PERFORMANCE
RATIONALE (on `attach_bubble_indicators`): Why: a pool can reuse any parked bubble only if all bubbles have the same
children. What: every bubble carries the air shadow and tether, hidden on the ground. Trade-off: two hidden entities and
two retained materials per ground bubble.

The shadow and tether still do not follow a unit whose altitude changes after its arrival (today's behaviour, B-238,
out of scope: §12).

### D-7. Effects held on a unit

- **Looks held through `StanceTrack`** (stance, morph, cover, statuses, named statuses): removing the component at
  parking runs `release_held_looks`, as despawning did. The reused entity has no track, so `present_stances` gives it a
  `StanceTrack::default()` (`seen: false`), as for a fresh bubble.
- **Held beams** (`HeldBeams`) are keyed by the attacker's simulation id and follow bubbles found by id in the
  presented records; a departed id is never presented again in the session, so a beam can never follow the reused
  entity's new unit. No change; R-5 pins it.
- **Struck and launch placement** (`effects::struck`) find bubbles by simulation id in the presented records; no
  change.
- **Mode trails** (`spawn_mode_trails`) query `With<LiveBattleBubble>`; a parked entity lays none.
- **Death looks** are laid at the dead unit's position from the combat events, not on its entity; no change.

### D-8. Exhaustion: grow, count, and say so once

The pool is **exhausted** when an arrival finds `parked` empty. It then spawns a new entity (today's spawn, with
D-6's children); this is how the pool fills during warm-up, and the drawing is never dropped. The pool's entity count
`E = live + parked` grows only then, so `E` is the session's high-water mark of presented bubbles.

The pool is **beyond its bound** when such a spawn makes `E > B`. It still spawns (the unit is drawn), the vectors grow
past their reservation (an allocation, acknowledged), `RenderPoolStats::beyond_bound` counts every such spawn, and on
the first one of the session the synchroniser logs one `warn!` naming `E`, `B` and its terms (D-3). The battle goes on.

Rejected: a typed failure that ends the battle. Exceeding the bound means the catalog's data allowed more presented
units than the bound foresaw (a scenario that places hosts beyond the supply cap, a match that builds very many
structures, a spawn without a timed life); a presentation pool must not end a match for that, and the counter and the
log keep it from being silent. Q0134 (§13) drafts the question in case the owner prefers a failure in tests or debug
builds.

`RenderPoolStats { spawned, reused, parked_now, high_water, beyond_bound }` (all `u32`) is public and read by the tests
and by the frame-budget evidence.

### D-9. Session lifecycle

`RenderEntityMap` follows the presented session's generation, as `HeldBeams::follow` and `BattleSelection` do. When the
generation changes, the old session's entities have already been retired by their owner (below); the map then clears
its vectors (keeping capacity), resets `stats`, computes the new `bound` from the first presented snapshot and reserves
to it. The three places that despawn every bubble today call `RenderEntityMap::retire_all(&mut commands)`, which
despawns every live and parked entity the map holds and clears it, instead of their own loops over
`With<LiveBattleBubble>` (which would leak parked entities and double-despawn live ones):
`sync_live_battle_system`'s tab-change branch, `runtime_battle::scene::retire_previous_battle`, and
`editor::playtest::teardown_playtest_system`. `release_parked(&mut commands)` despawns only the parked entities; it is
the first half of `retire_all` and is what R-8's fresh child calls to force a fresh spawn.

### D-10. Module and system shape

- New module `crates/client/src/render_pool.rs` owns `RenderEntityMap`, `ParkedBattleBubble`, `RenderPoolBound`,
  `RenderPoolStats` and the merge (pure functions over slices where possible, unit-tested in the module).
- `scene_controller.rs` keeps `sync_live_battle_system`, `LiveBubbleFrame`, `update_live_instance`,
  `spawn_live_instance`, and gains `reuse_live_instance`. The parked entities' components are reached through a second
  query, made disjoint from `LiveBattleQuery` by `With<ParkedBattleBubble>` / `Without<ParkedBattleBubble>` filters.
- The children's writes go through a `SystemParam` `IndicatorWriter` in `indicators.rs` (the children's queries with
  disjoint marker filters, the three material stores and `IndicatorAssets`), so the synchroniser's parameter list grows
  by one; no production function exceeds 100 lines (charter §2.4).
- `scene_controller.rs` is 1,449 lines today; the new code goes to `render_pool.rs`, not into it.

## 5. Who sees a parked entity: the consumer audit

Every query over bubble entities in `client` and `client_diagnostics`, and what a parked entity (no
`LiveBattleBubble`, `ParkedBattleBubble`, `Visibility::Hidden`) does to it. WP2 re-runs this audit with
`grep -rn "BubbleInstance\|LiveBattleBubble\|MeshMaterial3d<BubbleDomeMaterial>"` and records any new consumer.

| Consumer | Filter today | Parked entity | Change |
|---|---|---|---|
| `battle_input` `BubbleQuery` (click, box, right-click target) | `With<LiveBattleBubble>`, `InheritedVisibility` | excluded twice | none |
| `battle_hud::fog::refresh_battle_fog` | `With<LiveBattleBubble>` | excluded (so the fog never un-hides it) | none |
| `effects::stance::present_stances` | `With<LiveBattleBubble>` | excluded | none |
| `effects::systems::spawn_mode_trails` | `With<LiveBattleBubble>` | excluded | none |
| `editor::playtest` (bubble positions), `editor::verification` | `With<LiveBattleBubble>` | excluded | teardown: D-9 |
| `indicators::update_indicators_system` | `(&BubbleInstance, &Children)`, no marker | **included**: would write its children's visibility and the ring uniform from the stale record | add `Without<ParkedBattleBubble>` |
| `smog::SmogBubbleQuery` | every `BubbleInstance` but the volume and attack snapshots; `Visibility` optional | **included** as a smog source (**verify** whether a hidden bubble pushes smog) | add `Without<ParkedBattleBubble>` beside `Without<AttackSnapshotBubble>` |
| Lab preview, Lab picking (`HeroLineupIndex`), swarm (`SwarmBubble`) | own markers | excluded | none |
| `client_diagnostics` capture verify, shell-order audit, input probe | skip bubbles whose `ViewVisibility` is false; input probe reads `LiveBattleBubble` | excluded | none (R-7 covers a capture with parked entities) |
| Picking by HUD lists, selection panel, inspection | read the snapshot by id | never meet entities | none |

## 6. Tests

Red first: every test below is written and seen failing (or, for an API that does not exist yet, failing to build in a
scratch commit) before the code that turns it green. Every test finds its units by property, never by slug.

### 6.1 Steady state and allocation

- **R-1 = B-19 (Broodlings cycling, steady state).** `sync_steady_state_tests.rs`: the broodlings plan's patch applied
  as is (a Brood Lord analogue, found by `capabilities.strike_magazine`, attacking a durable unarmed ground unit found by
  property, every other mobile unit removed), extended:
  - warm-up until the regime is reached, derived from the data rather than counted: run until `stats().spawned` has not
    changed for `2 × L_max` ticks (`L_max` read from the spawning weapon's timed life), failing after `10 × L_max`;
  - the window is `3 × L_max` ticks and must contain at least 2 arrivals and 2 departures of the spawned kind (counted
    from the snapshot, as the patch does);
  - assertions: no asset of any watched type added or removed; modifications only of bubble, ring and standard
    materials, bubble materials bounded as `a_gas_trip_in_and_out_of_the_extractor_spawns_nothing` bounds them
    (redrawn `(unit, frame)` pairs from `Motion`) plus one per arrival, ring and standard materials at most one each per
    arrival;
    `census` unchanged (no entity spawned or despawned); `retained_buffers` unchanged, with `RenderEntityMap`'s
    vectors added to the list; `stats().reused` grew by the window's arrivals.
- **R-2 (air cycling, steady state).** Same file: a shipped air unit with media (found by `mover_type: Air`, presentable),
  of each player, killed and spawned again by the test at a fixed cadence (the world written directly, as the
  attack-snapshot stage does), so air arrivals and departures cycle without the Carrier's media; same assertions as
  R-1. It also covers cross-layout reuse: half the cycles kill an air unit and spawn a ground one on the same tick.
- **R-1v = B-19 under valgrind.** `sync_allocation_count_tests.rs` gains `Variant::BroodCycleLive` and
  `Variant::BroodCycleBaseline` (the same staged battle; the baseline keeps `advance_battle_ticks`, so the simulation's
  own allocations for spawning and expiry cancel in the comparison, as the running pair does today):
  - the children run at 40 ms per frame (one tick per frame) and warm for `4 × L_max` ticks, a fixed number derived
    from the data, so both children of the pair measure the same ticks; the live child then checks that
    `stats().spawned` does not change across its window, which proves the warm-up reached the regime (a failure, never
    a skip);
  - windows of `2 × L_max` and `4 × L_max` ticks; the difference window must contain at least 2 arrivals and 2
    departures (a watcher like `GasCrew`, counting from the snapshot);
  - asserted: the live pair's difference equals the baseline pair's, as for the existing pairs.
- **Gate catalog.** The `client/sync_allocation_count_tests` triggers gain
  `crates/client/src/{scene_controller.rs,render_pool.rs,indicators.rs,bubble_shader.rs}` (the gap of §1).

### 6.2 Reuse correctness

- **R-3 (headless equality).** New `crates/client/tests/render_pool_tests.rs`, through `support/battle_harness.rs`.
  A decoy D is made as dirty as a bubble gets: other owner than Y, selected, at 30 % life (red bar), mid-attack, with a
  held look (a status found by property: a unit with an `overclock` capability using it, or a cloaked unit), a moved
  shell (`NoAutomaticBatching` present). D departs; Y (another archetype, 60 % life, selected on its arrival frame
  through `BattleSelection`) arrives and reuses D's entity (asserted through `RenderEntityMap::get` and
  `stats().reused`). A second harness runs the same script but calls `release_parked` before Y's arrival, so Y is
  spawned fresh. After `present_stances` has run, the two Y entities are compared field by field: `BubbleInstance`,
  `Transform`, `Mesh3d`, `FlipbookPhase`, `Visibility`, `StanceTrack`, `NoAutomaticBatching` presence, the material's
  uniform (optics lanes included), shell and texture, and every child's transform, visibility and material values.
  Variants: same-frame reuse, reuse after 5 hidden ticks, ground → air, air → ground, dome (a structure destroyed) →
  sphere.
- **R-8 (GPU pixel comparison).** New `crates/client/tests/render_pool_gpu_tests.rs`, one binary, two offscreen
  children through `support/gpu_run.rs` (radv with the shared lock locally, lavapipe in CI), real shaders and shipped
  media, the smog volume off (`LabState::opt_smog_volume`, the production toggle, so smog source order cannot differ),
  the clock stopped for the capture:
  - two subject pairs in one view: a ground decoy replaced by an air unit, and an air decoy replaced by a ground unit,
    each decoy dirty as in R-3, each new unit selected and damaged, arriving after 5 ticks hidden;
  - child **reused** runs the script; child **fresh** runs it and calls `release_parked` before the arrivals;
  - the parent asserts the two captures are **bit-identical** (the effect lab's decal identity test establishes that
    presentation is deterministic run to run on one driver, plan `2026-11-19-retained-effect-instances` D-9);
  - controls: each child also captures the frame before the arrivals, and at least `FOOTPRINT_SHARE` of each subject's
    footprint must change between that frame and the final one (the test sees the bubbles); and the decoy's last frame
    differs from Y's in the footprint (stale state would be visible).
  - Gate catalog: `client/render_pool_gpu_tests` (class `Gpu`, triggers the synchroniser, indicators, render pool and
    bubble shader paths).

### 6.3 Who may see a parked entity

- **R-4 (picking and selection).** `render_pool_tests.rs`: a selected unit dies with the pointer on its last position;
  a left click selects nothing there, a box over it selects nothing, a right click with an army selected issues the
  point order bare ground issues (as the attack-snapshot tests check for their image); `BattleSelection` no longer
  holds the id. Then an arrival reuses that entity elsewhere, and a click on the old position still selects nothing.
- **R-5 (effects released and not reassigned).** A unit holding a stance or status look departs: the `EffectRuntime`
  holds none of its looks after the frame (counted by handle); the reused entity starts with no track and only the new
  unit's looks. A unit holding a sustained beam departs: the beam ends, and the reused entity's new unit holds no beam.
- **R-7 (consumers).** One parked entity in a running battle: `refresh_battle_fog` leaves its `Visibility` `Hidden`;
  `update_indicators_system` changes none of its children; the smog's source list excludes it; the capture verifier
  (`client_diagnostics`) skips it.

### 6.4 Exhaustion and lifecycle

- **R-6 (beyond the bound).** Unit tests in `render_pool.rs` cover the decisions with explicit bounds (reuse when parked,
  spawn when exhausted, `beyond_bound` and the one warning when `E > B`, departures always parked). One integration test
  in `render_pool_tests.rs` builds the headless app of `pipeline_presentation_tests.rs` on the shipped battle with the
  catalog edited before `BattleSession::prepare` so the bound is small (every trained unit's and every host's supply cost
  raised to the cap: `A = 1`, `T = max m(h)`), then spawns `B − E + 3` units over a few ticks: every record is drawn
  (`RenderEntityMap` has a visible entity for each), `stats().beyond_bound == 3`, the warning flag is set once, and
  nothing panics.
- **R-9 (bound derivation).** Unit tests of `RenderPoolBound::for_session` on fixture catalogs: no host (`T = 0`); a
  hangar; a magazine spawn (`c + ⌈L/b⌉`); a magazine-free spawn (the period form); an untimed spawn (contributes 0); a free
  trained unit (`cost` 1); a mix where the LP bound exceeds `HeldBeams`' single-type bound. One test computes the shipped
  bound for the shipped skirmish and checks its terms against the catalog read by property (not a pinned number).
- **R-10 (map).** Unit tests of the merge: departures in the middle, arrivals at the end, arrivals in the middle (back
  merge), simultaneous departures and arrivals, an empty snapshot; capacity and address unchanged within the bound.
- **R-11 (session retire).** A battle reset, a tab change and the editor's playtest exit leave no `ParkedBattleBubble`
  and no `LiveBattleBubble` entity, and a new session binds a new bound.

### 6.5 H-20 (Carrier interceptors), after the Carrier's media

**H-20** as the Carrier plan defines it: `sync_steady_state_tests` and `sync_allocation_count_tests` with two Carriers
cycling their members (launch, attack, dock, relaunch) for 600 ticks after warm-up: no allocation in steady state, no
leaked visual. Here it gains R-1's assertions and a valgrind pair `HangarCycleLive`/`HangarCycleBaseline`. The cycle
is driven by observed state, not timers: the test orders the Carriers to attack the durable target until every member
is launched (counted from the snapshot), then to stop until every member has docked, and repeats; warm-up and windows
are multiples of the time one cycle took during warm-up, measured, with the same "no spawn in the window" proof as
R-1v. §9 says when it runs.

## 7. Acceptance

1. **Tests:** R-1 to R-11 pass; H-20 per §9. `cargo gate package` green on the final tree (a verifier runs it, charter
   §4).
2. **No simulation change:** `git diff --stat main...HEAD -- crates/sim_types crates/sim_map crates/sim_core
   assets/catalog assets/scenarios` is empty; so goldens, measured worlds and the checksum schema are untouched by
   construction, and the package gate selects no golden replay.
3. **Existing client behaviour:** `skirmish_match_tests` (a skirmish won through input alone), the GPU suites selected by
   the package gate, `battle_flipbook_tests`, `battle_fog_tests`, `round2_visual_tests` (with D-6's one changed
   assertion) and `client_diagnostics`' `showcase_tests` pass. Each is run on its own (GPU children lock through the
   harness), never as a workspace `cargo test`.
4. **Frame cost (reported, with a regression bar):** `frame_budget` on the RX 580 through the documented display path
   (`DESIGN.md` §5), release build, scenarios 200 and 400, three interleaved pairs (base = `main` at WP0, pool = the
   final tree), each run under `with-gpu-lock` on its own. Reported: frame interval p50, p95, p99, max; main update
   p50; simulation p50; and the pool's `stats()` at the end (the 400 fight's deaths are departures now, so the pool
   parks them). Bar: the pool's median frame interval p50 and main update p50 are not above the base's by more than the
   larger of 2 % and the base's own spread across its three runs; otherwise stop rule 4.
5. **Records:** `DESIGN.md` §4 B-237 closed (or "fixed; H-20 confirmation pending", §9), B-238 registered, §5's frame
   budget paragraph gains the A/B line with an evidence file `docs/evidence/2026-11-29-render-entity-pool.md`; this
   plan's `As built`; the README row.

## 8. Work packages

Each package ends with `cargo gate quick` and the tests its change needs; the package gate runs once, on the final
tree, after merging `main` (charter §4). One building agent at a time on this plan.

**WP0: base of record, probe, red tests.** After the broodlings plan is on `main`.
- Base: record `main`'s commit, the B-19 figure (6/6 bubble materials, plus the ring and standard materials E-2 adds),
  and the frame-budget base runs (§7.4).
- **Probe P-0** (scratch, not committed): under `valgrind --tool=dhat`, a headless app that, after warm-up, moves one
  entity between two archetypes by marker swap (with `StanceTrack`'s hook) every frame; count window blocks as
  `sync_allocation_count_tests` does. Zero: D-2 stands. Non-zero: stop rule 1.
- Verify the schedule claim of §1 (sync point before fog and stances) with `support/schedule_order.rs`.
- Red: R-1 (the patch, extended with census and buffers), R-1v, R-2, written against today's API and failing. Commit
  `test(client): B-19 and air cycling, red (plan 2026-11-29-render-entity-pool WP0)`.

**WP1: the map and the bound.** `render_pool.rs` with `RenderEntityMap` (D-4), `RenderPoolBound` (D-3),
`RenderPoolStats`; the synchroniser's merge; every test's `.0` access replaced by `get`/`is_empty`/`iter` (the files
of §11); the gate triggers. Still spawning and despawning, so behaviour is unchanged. Red then green: R-9, R-10.
**Acceptance:** R-9, R-10; every existing client test that reads the map passes; R-1 still fails on materials only (the
map no longer allocates).

**WP2: the pool.** D-1, D-2, D-5 to D-9: parking, reuse, `IndicatorPose` and the uniform layout, `IndicatorWriter`,
the consumer filters of §5, `retire_all`/`release_parked` at the three retire sites, the diagnostic. Red then green:
R-3 to R-7, R-11. **Acceptance:** R-1 to R-7, R-9 to R-11 pass; `round2_visual_tests` with its one changed assertion.

**WP3: pixels, suites, cost, records.** R-8; the acceptance runs of §7.3; the frame-budget pairs of §7.4; the records of
§7.5. **Acceptance:** §7.

**WP4: H-20.** Only when §9's condition holds; may be done by the Carrier media package instead.

## 9. Sequencing with the Carrier's media (H-20)

- **B-19 does not wait.** The Broodling's and the Brood Lord analogue's media are installed on `broodlings-plan`
  (`required_set.ron` lists `spore_matriarch` and `brood_mite`), which merges before WP0.
- **H-20 needs the Carrier's media.** `mothership_carrier` and `saucer_interceptor` have none on `main`, and the client
  keeps the Carrier locked (`NoMedia`). Writing H-20 now would mean a test that skips or stages a unit the client
  refuses to draw, which charter §1.2 forbids. Until the media land, the pool's air path is covered by R-2 (shipped air
  units with media, cycled by the test) and R-3/R-8's cross-layout pairs, so nothing the interceptors exercise is
  untested except their own launch and dock.
- **The condition** for WP4: `main` has the Carrier media package merged, `assets/media/required_set.ron` lists both
  identities, and the client presents a Carrier (its `NoMedia` lock gone).
- **Whichever lands second runs H-20.**
  - If the media are on `main` before this plan's package gate: WP4 runs here, and B-237 closes with H-20.
  - If this plan merges first: B-237 is recorded "fixed; H-20 confirmation pending the Carrier's media", and the
    Carrier media package runs H-20 as its acceptance (the Carrier plan's own text already says "When the media lands,
    H-20 is run then"); it reuses this plan's pool and R-1's helpers, and closes B-237 if green. If H-20 fails there, it
    is a bug in this plan's pool, registered and fixed under an amendment here.

## 10. Stop rules

1. **P-0 shows an allocation per marker swap** after warm-up: stop; amend D-2 before WP2 (the fallback is to keep
   `LiveBattleBubble` on parked entities and add a `Parked` guard to each consumer of §5, which is more code to audit).
2. **A consumer outside §5's table sees parked entities** in a way a filter cannot fix: stop and amend.
3. **R-8 is not bit-identical:** do not loosen it to a tolerance silently. Find the source (a reset D-5 missed, or a
   non-pool difference). If it is not the pool's, stop and amend with the measured difference and a justified bound.
4. **Frame cost above §7.4's bar:** stop and report before WP3's records.
5. **Any diff under the simulation crates, `assets/catalog/` or `assets/scenarios/`:** stop; this plan changes none.
6. **R-1 or R-1v needs a warm-up beyond `10 × L_max` ticks** to reach the regime: stop; the bound or the reuse is wrong.

## 11. Files

| File | Change | WP |
|---|---|---|
| `crates/client/src/render_pool.rs` (new) | `RenderEntityMap`, `ParkedBattleBubble`, `RenderPoolBound`, `RenderPoolStats`, merge, unit tests R-6 (decisions), R-9, R-10 | WP1, WP2 |
| `crates/client/src/scene_controller.rs` | merge-driven sync, `reuse_live_instance`, `retire_all` on tab change; `RenderEntityMap` re-exported | WP1, WP2 |
| `crates/client/src/indicators.rs` | `IndicatorPose`, uniform layout, `IndicatorWriter`, `Without<ParkedBattleBubble>` | WP2 |
| `crates/client/src/smog/mod.rs` | `Without<ParkedBattleBubble>` in `SmogBubbleQuery` | WP2 |
| `crates/client/src/runtime_battle/scene.rs`, `crates/client/src/editor/playtest.rs` | `retire_all` | WP2 |
| `crates/client/src/lib.rs` | module and re-exports | WP1 |
| `crates/client/tests/{sync_steady_state_tests,sync_allocation_count_tests,render_pool_tests,render_pool_gpu_tests,round2_visual_tests}.rs` | R-1 to R-11 | WP0 to WP3 |
| `crates/client/tests/{skirmish_match_tests,battle_fog_tests,pipeline_presentation_tests,battle_flipbook_tests,area_link_gpu_tests}.rs` | `.0` map access to methods | WP1 |
| `crates/gate/suites.ron` | valgrind triggers; `render_pool_gpu_tests` entry | WP1, WP3 |
| `DESIGN.md`, `docs/evidence/2026-11-29-render-entity-pool.md`, this plan, `docs/plans/README.md` | records | WP3 |

## 12. Out of scope and recorded decisions

- **B-238 (provisional): the air shadow and tether do not follow altitude.** Their transforms are set from the height at
  spawn and never updated (`indicators.rs`: only the tether's colour is maintained), so a unit that changes altitude
  after its arrival, or an air unit over a cliff, keeps a shadow at the wrong height. D-6 makes the fix easy (every
  bubble has the children); it is a presentation change of its own, deferred.
- **Pool trimming.** Parked entities are kept until the session ends; a fight's deaths leave the pool at the session's
  peak. Trimming would allocate again on the next growth; recorded, not done.
- **A shared shadow material.** `shadow_mat` is a constant colour and could be one shared asset instead of one per
  bubble; an independent saving, not needed for steady state once D-1 holds it.
- **The Lab preview and the swarm** have their own retained entities and are untouched.
- **Interceptors' strafe (B-229)** and anything of the simulation: untouched.
- **An observation for the orchestrator (not a bug filed by this plan, verify):** the Brood Lord analogue's
  `supply_cost` is 2 in `units.ron`, where StarCraft II's Brood Lord costs 4; the bound follows the data either way.

## 13. Bug and question drafts

Bug row for `DESIGN.md` §4 (number provisional, next free at merge):

```
| B-238 | The air shadow and tether keep their spawn height: `attach_bubble_indicators` (`indicators.rs`) places them from the record's altitude at spawn and nothing moves them afterwards, so a unit whose altitude changes after its arrival (or that flies over a cliff) draws its shadow and tether at the wrong height | open; deferred by plan [`2026-11-29-render-entity-pool`](docs/plans/2026-11-29-render-entity-pool.md) §12 |
```

Question draft, **not filed**; the default is what the plan builds, and filing it is optional (presentation
robustness is the agent's call under Q0020; it is drafted because it touches "fail loudly"):

```
# Q0134: When a battle presents more units than the render pool's derived bound, grow and warn, or stop?

- **Asked:** 2026-11-29
- **Blocking:** no
- **Default until answered:** (a)

## Question

The client now reuses the render entities of units that leave a battle (plan 2026-11-29-render-entity-pool). Its
storage is sized once per battle from the catalog: the units on the map at the start, plus per player what the supply
cap allows to be trained and the most Broodlings or interceptors a full army of their hosts can keep alive. A battle can
still exceed that, for example a scenario that places more Carriers than the supply cap allows, or a match that builds
very many structures. What should happen then?

- (a) **Grow and warn** (recommended): every unit is still drawn; the pool grows past its reservation, writes one
  warning to the log for the battle, and counts the extra units, so tests and the frame-budget report show it.
- (b) **Stop in tests, grow in play:** as (a) for players, but a debug or test build ends the battle with an error, so a
  test that exceeds the bound fails at once.
- (c) **Stop always:** the battle ends with an error naming the bound.

## Answer
```

## As built

Built on branch `render-pool-plan` from `35f9905a` (`main` `0f0608cd`, the broodlings plan merged; `main` did not move
during the work); evidence in
[`docs/evidence/2026-11-29-render-entity-pool.md`](../evidence/2026-11-29-render-entity-pool.md).

- **WP0** (`f2aafa12`): the base of record (the B-19 patch as is: 6 bubble materials added and 6 removed); probe P-0
  under DHAT, 0 blocks per marker swap in 64 and 128 frames (planted control 128 and 256), so D-2 stands; the
  schedule claim of §1 as a test (`system_ownership_tests::bubble_consumers_run_after_the_live_synchroniser`); the red
  tests R-1, R-2 and R-1v on today's API, staged through `tests/support/unit_cycling.rs` (R-1 and R-2 never reach the
  regime, 49 and 66 spawns; R-1v 17 blocks over 188 ticks beyond the tick driver's).
- **WP1** (`4e5ac204`): `client::render_pool` (`RenderEntityMap` as retained sorted vectors, `RenderPoolBound`,
  `RenderPoolStats`, `ParkedBattleBubble`), the synchroniser's two merges, `retire_all` at the three retire sites,
  the tests' map access through methods, the valgrind test's triggers. R-9 and R-10: 22 unit tests.
- **WP2** (`15b1723f`): parking and reuse (D-2, D-5), `IndicatorPose` and one indicator layout (D-6),
  `IndicatorWriter`, the consumer filters of §5 (indicators, smog), the live brood child's no-spawn check. R-3 to R-7,
  R-11; `round2_visual_tests`' changed assertion.
- **WP3** (`758dbe74`, `735f5fa3`): R-8 (`render_pool_gpu_tests`, bit-identical on radv), the acceptance runs, the
  frame-budget pairs and these records; then charter checks C21 (the GPU boot lives in the GPU test's own file) and
  C18 (the index row).
- **WP4** (after merging `main` at `db805c7a`, the Carrier's media): H-20 per §9, run here because the media landed
  before this plan's package gate. `tests/support/unit_cycling.rs` stages two hangar units (found by their hangar) of
  the local player beside its base, against a durable unarmed enemy air unit with media, and `HangarCycle::advance`
  drives the cycle from the presented snapshot: launch until no hangar has a member docked or building and every
  launched member is presented, then the target is killed and the members fly home until none is presented, then a
  new target. `sync_steady_state_tests::hangar_members_cycling_create_and_remove_nothing` measures one cycle during
  warm-up, warms to the regime and checks a window of three cycles (at least 600 ticks) with R-1's rules;
  `sync_allocation_count_tests::hangar_cycle_frames_allocate_nothing` is the `HangarCycleLive`/`HangarCycleBaseline`
  pair (both run the cycle's driver, a system before the tick), three warm-up cycles and windows of two and four
  cycles of the last warm-up cycle's length, with the live child's no-spawn check. R-5's beam half too. B-237 is
  fixed. The gate's selection test that pinned the valgrind test's old triggers is updated (`ee3ae0ae`).

Decisions and deviations, each recorded:

- **Q0134 not filed** (the orchestrator's decision): its default (a), grow and warn, is built. A spawn beyond the
  bound is counted in `RenderPoolStats::beyond_bound` and the first one of a session logs one `warn!` naming the
  bound and its terms; R-6 pins it.
- **The bound** counts each host's weapons and its modes' override weapons once each (the `weapon` field mirrors
  `ground_weapon`), with a magazine's charges taken as the maximum over its spawning weapons and the magazine-free
  form summed, as D-3 writes. A session presented without a world (none today) is bound by its start records alone
  (`RenderPoolBound::without_catalog`).
- **`RenderEntityMap::follow`** also despawns any entity the map still holds when the generation changes, so a
  missed retirement cannot leak (normally a no-op: the three retire sites call `retire_all` first). `retire_all`
  unbinds the map, so a tab change back to the battle binds the bound again from the first snapshot.
- **The reuse path reaches the reused entity through the parked query, or through the live query when its unit left
  on the same frame** (its park is queued, not applied); `ReuseAccess` groups the parked query, the bubbles' children
  and `IndicatorWriter`. `IndicatorWriter` holds the children's query and `IndicatorAssets`; the three material
  stores stay in `BubbleSceneAssets` (one system cannot hold them mutably twice) and are passed to it.
- **A reused material's shell** is reset to the shell the stance system writes for a unit with no look
  (`ShellSum::default().uniform(..)`, the all-zero default with its `driven` lane set), not to
  `ShellUniformGpu::default()` (D-5's table): the stance system rewrites every bubble's shell to it on the bubble's
  first frame, so the all-zero reset cost a second modification per arrival (R-1 measured 6 for 6 arrivals beyond
  its bound, R-2 12 for 12), against D-5's "a reuse modifies at most one bubble material". No look of the departed
  unit survives either way, and R-3 and R-8 compare after the stance system's frame.
- **R-2's stage** cycles each player's unarmed air unit beside its own base, not at the map's centre: a map object
  there changes its tint as units of either side come and go, a `StandardMaterial` modification that is not the
  pool's. The fresh-spawn twins of R-3 kill their decoy at least 3 ticks before the arrival (the presented snapshot
  lags the simulation by a tick), so the arrival is spawned on the same tick in both harnesses.
- **The hidden air shadow and tether of a ground bubble** are placed at the least airborne height (0.5 world units),
  so their transforms never collapse to a zero scale; visible ones keep today's transforms.
- **Tests.** R-1 and R-2 warm up on `stats().spawned`; on WP1, which still spawns, they fail on the regime, not on
  materials (the plan's WP1 acceptance expected materials). R-9 and R-10 were written with the module (red only as a
  failing build). R-3 to R-7 and R-11 were written before the WP2 code but not run red against WP1. R-5's
  sustained-beam half waited for WP4: until the Carrier's media landed, no shipped unit whose weapon holds a beam had
  media; it is now `render_pool_tests::a_departed_members_beam_ends_and_never_follows_the_reused_bubble`, with an
  interceptor holding `prism_beam`. R-11's playtest exit is `editor_app_tests::escape_after_a_death_retires_the_render_pool`.
  R-7's capture-verifier half is checked through the mechanism the verifier reads (every parked entity and child has
  `InheritedVisibility` false), not by running the verifier. R-8 runs the production client offscreen, booted as
  the battle harness boots it but with a renderer (`render_pool_gpu_tests::boot_on_gpu`, in the GPU test's own file,
  so charter check C21 sees it run only in a launched child): the window has no surface, the RTS camera renders
  into an `OffscreenCanvas`, and captures are taken with virtual time paused.
- **Frame budget (§7.4).** The pool's `stats()` at the end of a run are not reported: `frame_budget`'s report has no
  field for them, and adding one would change `client_diagnostics`, which §11 does not list. The A/B itself is in
  the evidence file.
