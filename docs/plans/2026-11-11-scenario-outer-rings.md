# Blueprint: Starting Units on Outer Rings, and the Two-Army Frame Budget (Q0018 option (b))

The owner's update to [Q0018](../questions/open/0018-frame-budget-army-layout.md) (2026-11-11) asks for option (b)
now. The scenario builder continues placing a player's starting units on rings 4, 5, … around its town hall when
rings 1 to 3 are full, so two armies of 100 and 200 units can load. Placement on rings 1 to 3 stays exactly as it
is, so no existing scenario's checksum changes. The 200- and 400-unit frame budget is then measured again with two
armies, and Q0018 is closed.

The change is to `sim_core`'s world construction and it replaces the frame-budget figures of record, so charter §6
requires a full plan. It does not move battle outcomes or golden replays (§4).

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-11 against `main` at
`959ce4c`. Line numbers drift, so search for the quoted symbol. Claims not confirmed while planning are marked
**verify**.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0018: how the frame-budget scenarios field 200 and 400 units | Answered 2026-11-11: option (b), now | Whole plan; closed by WP3 |
| Q0015: frame budget is measured and published against 16.7 and 33.3 ms, no pass or fail | Standing | §7 |
| Q0019: the display path of record is headless KWin (`kwin-virtual`) | Standing | §7 |
| Q0051 (charter §8): mechanics follow StarCraft II | Not engaged: §3.6 | §3.6 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing | §10 |
| Q0002: scoping is the agent's call if written down | Standing | §12 |
| None new | — | Every choice is a decision below |

---

## 1. Summary

| | Today | After this plan |
|---|---|---|
| Rings searched | `d = 1, 2, 3` around the town hall (`place_ring_units`, `crates/sim_core/src/scenario.rs`) | `d = 1, 2, 3` unchanged, then `d = 4, 5, …` only while units remain |
| Capacity around a 5×5 hall on open ground | 24 + 32 + 40 = 96 | 96, then 48 (ring 4), 56 (ring 5), … ring `d` holds `16 + 8d`; 200 units fill rings 1 to 5 exactly |
| Rule on rings 1 to 3 | Predicate *P* (below) | *P*, byte-for-byte the same code path |
| Rule on rings 4+ | — | *P* plus three outer-ring rules (D-2 to D-4) |
| Too many units | `NotEnoughSpawnCells { slot }` after ring 3 | `NotEnoughSpawnCells { slot, requested, placed }` once the hall's region is exhausted |
| Frame-budget scenarios | Eight players × 0, 25, 50 units | Two players × 0, 100, 200 units on the same flat arena |
| Figures of record | `c918aa9`, eight-player arena | A new five-run series at the merged commit; `DESIGN.md` §5 replaced |

---

## 2. What the code does today

- `build_world_with_catalogs` validates the map, builds the `SimWorld`, then visits start locations in ascending
  slot order. For each one with a `PlayerSetup` it resolves the loadout (`resolve_loadout`), applies the bank,
  centres and spawns the hall (`place_town_hall`, which returns the hall's inclusive cell rectangle `PlacedHall`
  and its anchor cliff level), and calls `place_ring_units`.
- `place_ring_units` loops `for d in 1..=3`. Ring `d` is the perimeter of the rectangle
  `[x_min − d, x_max + d] × [y_min − d, y_max + d]`, visited by `ring_perimeter` (south edge x ascending, east edge
  y ascending, north edge x descending, west edge y descending). A cell is used when predicate **P** holds:
  in bounds; same `cliff_level` as the hall's anchor; `GROUND_PATHABLE`; not `Abyss`; no ramp (`cell.ramp` and
  `CellFlags::RAMP`); no `occupying_structure`; not in `claimed_cells` (the set shared by all players of the build).
  Units are spawned in loadout order at the cell centre. The loop stops the moment all units are placed. Fewer
  than needed after ring 3 returns `ScenarioError::NotEnoughSpawnCells { slot }`.
- `ring_perimeter` is public and also used by `economy/mod.rs`, `transport/orders.rs` and
  `tests/group_routing_tests.rs`. It does not change.
- Resource nodes and watchtowers are points, not obstacles: `MapDocument::to_game_map` records them in
  `GameMap::resources()` and `GameMap::watchtowers()` and leaves their cells pathable.
- Map validation (`sim_map::validation`) keeps start footprint centres at least `MIN_START_SEPARATION_CELLS` = 16
  apart (Chebyshev).
- Setup spawns capture supply but do not enforce the supply cap (eight players of 50 units already exceed a
  lone hall's provision) (**verify** for 200 units of `battle_automaton`'s mix in WP2's fixture test).
- Every production and test path that builds a world from a scenario goes through `build_world_with_catalogs`:
  `load_scenario_world`, `load_scenario_battle`, `client::runtime_battle::BattleDefinition`, the editor's
  playtest, `sim_fixtures::scenario_world`. The golden replays (`golden_replay_tests.rs`) do **not**: they spawn
  their squads directly on `default_arena`.
- The only test that relies on the 96-unit limit is `error_not_enough_spawn_cells` in
  `crates/sim_core/tests/scenario_tests.rs` (100 `welder_drone`s). After this plan that loadout loads.

---

## 3. Design

### 3.1 The placement rule

```text
place_ring_units(world, slot, hall, units, claimed, reserved):
    placed = 0
    for d in 1..=INNER_RING_COUNT:                 # INNER_RING_COUNT = 3; the code path of today
        place on ring d with predicate P           # unchanged, including early exit
    if placed == units.len(): return Ok
    region = hall_region(world, hall, reserved)    # computed only now (D-3)
    for d in (INNER_RING_COUNT + 1)..=region.max_ring:
        for (x, y) in ring_perimeter(hall grown by d):
            if placed == units.len(): return Ok
            if P(x, y) and Q(x, y): spawn units[placed] at the cell centre; claim; placed += 1
    return Err(NotEnoughSpawnCells { slot, requested: units.len(), placed })
```

`Q` is the outer-ring predicate: the cell is in `region` (D-3), it is not in `reserved` (D-2), and it holds no
resource node or watchtower (D-4). Order within an outer ring is `ring_perimeter`'s, the same as rings 1 to 3
(owner's requirement), and units keep loadout order across rings.

### 3.2 Decisions

- **D-1. Rings 1 to 3 keep their code path.** The existing loop over `1..=3` stays as it is, with `3` named
  `INNER_RING_COUNT` and documented as the rings whose placement is pinned for compatibility. The outer-ring loop
  runs after it and is entered only when units remain. A shared helper that places one ring under a given
  predicate is allowed only if rings 1 to 3 still evaluate exactly *P* in the same order; the implementation must
  not add *Q*'s tests to rings 1 to 3, even where they would be harmless (D-2's zones cannot reach rings 1 to 3 by
  the separation rule, but the guarantee is by construction, not by argument).
- **D-2. Other starts' inner zones are reserved.** For every start location of the document other than the
  player's own, assigned to a player or not, the footprint grown by `INNER_RING_COUNT` cells on every side is
  reserved: no outer-ring unit lands there. A later slot's hall is not on the map yet when an earlier slot places
  its units, so *P*'s structure test cannot protect it. With the reservation, one player's outer rings never
  change another player's hall or rings 1 to 3 placement: each player's inner placement is independent of how
  many units the others field. The reservation is a `Vec<CellRect>` built once per build from
  `doc.start_locations`, in the document's id order.
- **D-3. Outer rings stay in the hall's ground region.** Same cliff level is not enough at distance 4 and beyond:
  a ring can cross a moat or cliff onto a separate plateau of the same level. The region is the 4-connected set
  of cells satisfying the terrain part of *P* and *Q* (in bounds, same cliff level, ground-pathable, not abyss,
  not ramp, no structure, not reserved), seeded by every such cell 4-adjacent to the hall's rectangle, found by a
  breadth-first fill over a `Vec<bool>` of `width × height` cells with a `VecDeque` visited in fixed neighbour
  order (east, north, west, south). Claims are not part of the region, so it depends on terrain and structures
  only. The fill also returns `max_ring`, the largest Chebyshev distance from the hall's rectangle to a region
  cell. It runs at most once per player, at setup, only for loadouts larger than rings 1 to 3 hold.
- **D-4. Outer rings skip resource and watchtower cells.** These cells are pathable (§2), but a unit spawned on a
  mineral node or tower is a poor start. Rings 1 to 3 keep today's rule (D-1).
- **D-5. Termination is region exhaustion, bounded by the map.** No ring constant: `max_ring ≤ max(width,
  height)`, so the loop is finite, and every ring beyond `max_ring` holds no region cell. A loadout that does not
  fit returns `NotEnoughSpawnCells { slot: u8, requested: usize, placed: usize }`; the world is not returned, so
  nothing is silently truncated. The variant's message and doc ("within distance 3") are rewritten.
- **D-6. The public surface stays small.** `ring_perimeter` is unchanged. `place_ring_units` gains the reserved
  zones as a parameter; `build_world_with_catalogs` builds them. The region fill is a private function in
  `scenario.rs` or, if `scenario.rs` would exceed a readable size, in a new private submodule
  `crates/sim_core/src/scenario/placement.rs` with its own `//!` doc (the implementer's choice, recorded in the
  commit). No function exceeds the `too_many_lines` threshold.
- **D-7. Frame budget: two players opposite each other.** The two-army arena keeps the existing map, seed,
  source loadout, camera pose and windows, and keeps slots 0 and 4 of today's circle as slots 0 and 1:
  start cells `(84, 62)` and `(40, 62)`, 44 cells apart, each attack-moving to the other (§6).

### 3.3 Determinism and charter compliance

- Integer cell arithmetic only; positions are `SimScalar` cell centres as today. No float, no `HashMap`/`HashSet`
  (charter §3.1, C7, C9). `BTreeSet` for claims (unchanged), `Vec<bool>` and `VecDeque` for the fill.
- The fill and the reservation list allocate once per build, never per tick; no PERFORMANCE RATIONALE is needed.
- Ring bounds are `i32` from `PlacedHall`; growing by `d ≤ max(width, height)` cannot overflow for maps within
  `sim_map`'s maximum dimension, and conversions use `try_from` as the current code does.
- No catalog identity in Rust (C5); tests pick archetypes by shape from the shipped catalog, as the existing unit
  test does.
- Typed error, no `unwrap`, no discarded results (C12, C14).

### 3.4 Module docs

`scenario.rs`'s `//!` "Deterministic Placement" invariant and `build_world_with_catalogs`'s "Placement Algorithm"
list state the inner rings, the outer rings with D-2 to D-5, and the compatibility guarantee of D-1.

### 3.5 What the frame-budget tool needs

Nothing in Rust beyond docs: `FrameBudgetSpec` already takes any number of players up to eight and any
`units_per_player`, `fixture.rs` scales the source loadout with largest-remainder rounding, and the run already
fails on a wrong unit count or fewer than 95 % of units in view. The module doc of
`crates/client_diagnostics/src/frame_budget/mod.rs` (the eight-player paragraph) and the header comment of
`frame_budget.ron` are rewritten.

### 3.6 StarCraft II

No StarCraft II mechanic changes. Where a scenario places its starting units is a setup convention of this
project's scenario format; StarCraft II's melee start (a town hall and twelve workers) has no counterpart for
armies of 100. No unit value, timing or behaviour changes, and no deviation is recorded.

---

## 4. Proof that existing checksums do not change

1. **By construction (D-1).** For a loadout that rings 1 to 3 hold, the function runs the same loop with the same
   predicate and returns before the new code; it spawns the same archetypes on the same cells in the same order,
   so entity ids, positions, the world and its checksum are identical. A loadout that rings 1 to 3 do not hold
   failed before; it is the only case whose result changes.
2. **D-2 keeps this true across players.** Outer-ring units of one player never occupy another player's inner
   zone, so a scenario in which one player newly loads (previously an error) cannot alter another player's inner
   placement either.
3. **Oracle test.** `scenario_placement_properties.rs` (§5.2) carries a reference implementation of today's
   three-ring rule in test code and checks, on generated maps and on every shipped scenario, that the builder's
   placement equals the oracle's for every loadout the oracle can place.
4. **Recorded before and after.** WP1 records, in its commit message, `last_checksum` of every scenario under
   `assets/scenarios/` (built with `load_scenario_world` from the real assets root) and of the three scenarios of
   today's eight-player `frame_budget.ron`, computed at the base commit and at WP1's head, and shows they are
   equal. The command used is a throwaway outside the repository (`~/scratch`), not committed.
5. **Existing suites unchanged.** Every existing test that builds a scenario world (§2's list, the client's
   battle, presentation and skirmish tests, `frame_budget_tests.rs`) passes without edits. The only edited test is
   `error_not_enough_spawn_cells` (§5.3).
6. **Golden replays** do not use the builder (§2); they run in the package gate as for any `sim_core` change and
   must reproduce every checkpoint on Linux release and `wasm32-wasip1`.

---

## 5. Tests

### 5.1 Unit tests (`scenario.rs`, `mod tests`)

On the existing `open_document` (32×32, starts at `(6, 6)` and `(24, 24)`) or small variants of it:

| Test | Asserts |
|---|---|
| `units_beyond_ring_three_continue_on_ring_four_in_perimeter_order` | 100 units: the 96 of rings 1 to 3 as today, then 4 on ring 4 in `ring_perimeter` order, loadout order kept |
| `outer_rings_skip_blocked_cells` | A map with a structure, an abyss cell, a ramp, a non-pathable cell, a cell of another cliff level, a resource and a watchtower on ring 4: each is skipped, the next eligible cell is used |
| `outer_rings_never_enter_another_starts_inner_zone` | Slot 1's start at the minimum separation; slot 0 fields enough units to reach it: no slot-0 unit inside slot 1's footprint grown by 3, and slot 1's placement equals its placement when slot 0 fields 96 |
| `outer_rings_do_not_cross_onto_a_separate_plateau` | A non-pathable moat at distance 5 with same-level ground beyond: units stop at the moat, and the build fails with exact `requested` and `placed` when the enclosed region is too small |
| `exhausted_region_is_a_typed_error` | A hall enclosed by abyss: `NotEnoughSpawnCells { slot: 0, requested, placed }` with `placed` equal to the region's free cell count |
| `two_hundred_units_fill_rings_one_to_five_exactly` | Open ground, a 5×5 hall, 200 units: rings 1 to 5, no cell skipped, none of ring 6 |

The existing `units_fill_the_nearest_ring_first_in_perimeter_and_loadout_order` stays unchanged.

### 5.2 Property tests (new `crates/sim_core/tests/scenario_placement_properties.rs`, `proptest`)

Generated documents: a 32×32 or 48×48 map, two starts at legal separation, sparse random blockers (non-pathable,
abyss, other cliff level) outside the footprints, documents that fail `validate_map` discarded with
`prop_assume!`; unit counts 0 to 250 per player from the shipped catalog's mobile archetypes.

- **P1 (compatibility).** The first `min(n, oracle capacity)` placements equal the three-ring oracle's, cell for
  cell and archetype for archetype, for both players.
- **P2 (validity).** Every unit is on a distinct cell that satisfies *P*, and every unit beyond the oracle's
  capacity also satisfies *Q*.
- **P3 (order).** The ring index of placed units never decreases in spawn order, and within a ring the cells
  appear in `ring_perimeter` order.
- **P4 (completeness).** Either all units are placed, or the build fails with `requested == n`, `placed < n`, and
  no eligible cell is left unclaimed (no cell of rings 1 to 3 satisfying *P*, and no cell of the region beyond
  ring 3 satisfying *P* and *Q*).
- **P5 (prefix).** Placement of `n` units is a prefix of placement of `n + k`.
- **P6 (determinism).** Two builds of the same input have equal `last_checksum`.

The file also holds the shipped-scenario oracle check of §4.3 (a plain `#[test]`, reading `assets/scenarios/*.ron`
through `load_scenario_world`). Its `//!` doc states the oracle's purpose. If check C22 requires the new test
target to be classed in `crates/gate/suites.ron`, add it to the `sim_core` fast class (**verify**).

### 5.3 Integration tests (`crates/sim_core/tests/scenario_tests.rs`)

- `error_not_enough_spawn_cells` keeps its name and variant check (now `{ slot: 0, .. }`), but uses a map where
  the hall's region is enclosed, so it still exercises the error.
- New `hundred_units_load_with_four_on_ring_four`: the same 100-unit loadout on `base_valid_doc` now loads; 100
  units, 96 within Chebyshev distance 3 of the hall, 4 at distance 4.
- The module doc's invariant 3 ("within Chebyshev distance 1..=3") is rewritten.

### 5.4 Frame-budget tests

`fixture_root_scenarios_spawn_exactly_their_units` needs no edit: it checks one hall per player and
`units_per_player` units each through the production loader, and its totals map (`0`, `200`, `400`) is
unchanged. The GPU smoke run still measures the `400` scenario. Its doc comment ("fits its spawn rings") stays
true.

---

## 6. `frame_budget.ron`

```ron
players: [
    (slot: 0, start_cell: (84, 62), target_slot: 1),
    (slot: 1, start_cell: (40, 62), target_slot: 0),
],
scenarios: [
    (name: "0", units_per_player: 0),
    (name: "200", units_per_player: 100),
    (name: "400", units_per_player: 200),
],
```

- Everything else is kept: `battle_automaton` as source, seed, the 128×128 flat map, the 5×5 footprints, the
  camera pose, 180 warm-up frames, 1,500 measured ticks, the 750-tick combat window, 0.95 in view, the frame cap.
- **The 0 scenario is the baseline of the same scene**: the same two halls, no unit. It is not comparable with the
  old eight-hall baseline, and the evidence says so.
- Geometry: 200 units fill rings 1 to 5, a 15×15 block around each hall (x 79 to 93 and 35 to 49, y 57 to 71),
  inside the region the eight-player armies already occupied in view. If a smoke or real run fails the 95 %
  in-view check, the camera target or distance is changed in this file, the change recorded in the evidence, and
  the 0 scenario re-run with it.
- The header comment is rewritten: two armies, rings 4+ (this plan), Q0018 option (b), with the link to
  `docs/questions/closed/`.

---

## 7. Measurement on the RX 580

The protocol is the one of record in `docs/evidence/2026-11-03-frame-budget.md` ("How each run was taken"),
repeated here so the evidence can cite it:

1. **Commit.** `main` after WP1 and WP2 have merged; the evidence names it. Release build:
   `CARGO_TARGET_DIR=~/scratch/cargo-target/outer-rings-wp3 cargo build --release -p client_diagnostics --bin frame_budget --locked`.
   No build runs during the measurement.
2. **Fixture root once, without the GPU:** `lavapipe` under `xvfb-run`, `--warmup-frames 1 --measure-frames 1`,
   work directory `~/scratch/outer_rings_wp3/work`.
3. **CPU.** Record `nproc`, the process's affinity and the memory `anon` line before starting. If 16 dedicated
   cores are still available, pin as before with `taskset`; otherwise pin to the same 16-core set for every run
   and record it. The evidence states the difference from 2026-11-03.
4. **Fifteen runs, interleaved** `0, 200, 400` × 5, each **one** `with-gpu-lock` command with KWin and the tool
   inside it (KWin renders on the RX 580 too), released between runs with a 10 s gap:
   ```bash
   WGPU_BACKEND=vulkan VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json \
   with-gpu-lock bash -c "date -u +%s.%N > $W/run-NN.lockgot; \
     taskset -c <cores> kwin_wayland --virtual --xwayland --no-lockscreen --no-global-shortcuts \
       --width 1280 --height 720 --socket outer-rings-NN \
       --exit-with-session '$FB --display-path kwin-virtual --work-dir $W/work --scenario S --out $W/run-NN.ron'"
   ```
   Each run holds the lock about 70 s, inside the 600 s default. Load average and lock wait are recorded per run
   as before.
5. **Acceptance of a run:** its report file exists, its log has no `frame_budget failed`, and the tool's own
   checks passed (exact unit count, at least 95 % in view on every frame, no early match end). A run started at a
   one-minute load above 8 is kept and marked; if more than one run of a scenario is loaded, those runs are
   repeated at the end.
6. **Acceptance of the series:** five accepted runs per scenario. Statistics are recomputed from each report's rows
   with the tool's nearest-rank definition and must match the tool's summaries to 0.01 ms. Figures of record are
   the per-scenario medians of frame interval p50, p95, p99, max, main update p50 and p95, simulation p50 and p95.
   No pass or fail line (Q0015); the 16.7 and 33.3 ms reference lines are stated.
7. One screenshot of the 400 scenario in contact, as the 2026-11-03 evidence has. Attribution runs (`perf`,
   GPU pass timings, quarter resolution) are out of scope (§12).

Raw reports stay under `~/scratch/outer_rings_wp3/`; the tables in the evidence are the record.

---

## 8. Documentation

- **New evidence** `docs/evidence/2026-11-11-frame-budget-two-armies.md`: host, build, commit, display path, CPU
  set, the two-army scenarios, the command, the per-run table, the medians, a comparison paragraph with the
  2026-11-03 eight-player figures (different scene; not like for like), the screenshot.
- **Old evidence** `docs/evidence/2026-11-03-frame-budget.md`: a note under the title that the figures of record
  are now the two-army series (link), and its Q0018 link updated to `closed/`. Its tables are history and stay.
- **`DESIGN.md` §5, "Frame budget"**: the paragraph and table replaced by the two-army figures, commit and date;
  the "Since that measurement" paragraph reworded as history (the C-17 and C-18 work now lies inside the measured
  commit). Check C17 must stay green (links and evidence).
- **`DESIGN.md` §3 or §4** only if it describes scenario placement (none found while planning; **verify**).
- **Links** to `docs/questions/open/0018-…` updated to `closed/` in `crates/client_diagnostics/src/frame_budget/mod.rs`,
  `crates/client_diagnostics/data/frame_budget.ron`, `docs/evidence/2026-11-03-frame-budget.md` and
  `docs/plans/2026-11-03-close-phase-6-7-8-exits.md`.
- **Q0018**: a `## Resolution` section (what changed, the WP1, WP2 and WP3 commits, the new figures in one line),
  then `git mv docs/questions/open/0018-frame-budget-army-layout.md docs/questions/closed/`.
- **This plan's index row** set to `Done` with the commits.

---

## 9. Work packages

| WP | Content | Files | Done when |
|---|---|---|---|
| WP1 | Outer rings (§3), tests (§5.1 to §5.3), before/after checksum record (§4.4) | `crates/sim_core/src/scenario.rs` (or `scenario/placement.rs`), `crates/sim_core/tests/scenario_tests.rs`, new `crates/sim_core/tests/scenario_placement_properties.rs`, `crates/gate/suites.ron` if C22 needs it | Tests green under §10's tier; commit message has the checksum table |
| WP2 | Two-army spec (§6) and docs of the tool | `crates/client_diagnostics/data/frame_budget.ron`, `crates/client_diagnostics/src/frame_budget/mod.rs` (doc only) | `fixture_root_scenarios_spawn_exactly_their_units` green; one `lavapipe` smoke run of `400` with `--warmup-frames 1 --measure-frames 1` finishes |
| WP3 | Measurement (§7) and documentation (§8), Q0018 closed | evidence, `DESIGN.md`, Q0018, link updates, `docs/plans/README.md` | Fifteen accepted runs; docs merged; Q0018 in `closed/` |

WP1 and WP2 may be one branch and one implementation agent (WP2 depends on WP1 to load). WP3 runs after they
are merged to `main`, because the measured commit must be on `main`.

---

## 10. Test tiers

- **Implementation agent (WP1, WP2):** `cargo gate quick`, then the targets the change needs:
  `cargo test -p sim_core --lib scenario`, `cargo test -p sim_core --test scenario_tests --test scenario_placement_properties`,
  and `cargo test -p client_diagnostics --test frame_budget_tests fixture_root` (non-GPU tests only), with
  `CARGO_TARGET_DIR=~/scratch/cargo-target/<worktree>` and, for any workspace-wide `cargo test`, under
  `flock ~/scratch/gate.lock`. It hands back after these (Q0061).
- **Verifier / orchestrator:** `cargo gate package` on the branch after merging `main`. As a `sim_core` change it
  includes clippy on the workspace, `workspace_checks`, every `sim_core` and downstream test the catalog selects
  (including `frame_budget_tests`' GPU smoke run through the harness and `skirmish_match_tests`), and the golden
  replays in release and on `wasm32-wasip1`.
- **WP3** is docs plus measurement: `cargo gate package` runs fmt and `workspace_checks` only (C17, C18, C19).

---

## 11. Risks

| Risk | Mitigation |
|---|---|
| A test or tool somewhere expects the 96-unit failure | Only `error_not_enough_spawn_cells` was found (`grep NotEnoughSpawnCells`); the package gate runs every dependent |
| The outer-ring code path leaks into rings 1 to 3 (for example by refactoring into one predicate) | D-1 forbids it; the oracle property P1 and the shipped-scenario oracle check catch it; the before/after table proves it on shipped data |
| A 15×15 army leaves the camera's view, or the two-line fight differs in density from the eight-way melee | The tool fails runs below 95 % in view; the camera is adjusted in data (§6). The evidence states that the scene differs and the old figures are not like for like |
| The machine no longer has 16 dedicated cores, or carries more load than on 2026-11-03 | §7 step 3 records the CPU set and load; loaded runs are marked and repeated per §7 step 5 |
| 200 units per player at setup trip a supply or allocation invariant | WP2's fixture test loads both armies through the production loader before any GPU time is spent (**verify** at WP2) |
| The CI smoke run of the `400` scenario gets slower | Same unit count as today; only the layout changes |
| The region fill on large maps | One fill per oversized loadout at setup, `width × height` cells; maps are bounded by `sim_map`'s maximum dimension |

---

## 12. Out of scope

- Changing rings 1 to 3 in any way, including making them skip resources or other starts (D-1, D-4).
- A per-scenario or per-loadout ring limit or formation shape; a capacity display in the map editor.
- Attribution runs (`perf`, GPU pass timings, quarter resolution) for the two-army scenes; finding C-18's remaining
  smog cost. They belong to a later plan if the new figures call for one.
- Keeping the eight-player arena as a second measured spec. Its figures remain in the 2026-11-03 evidence as
  history.
- Any StarCraft II mechanic (§3.6).

---

## Amendment 1 (WP1 and WP2 implementation, 2026-11-11): verify items and what landed

- **Module layout (D-6).** The outer rings, the reservation and the region fill are in the private submodule
  `crates/sim_core/src/scenario/placement.rs`; `scenario.rs` keeps the rings 1 to 3 loop, now
  `for d in 1..=INNER_RING_COUNT`, otherwise unchanged, and calls `placement::place_outer_rings` only when units
  remain. `PlacedHall` gains the id of its start location, which identifies the player's own inner zone. The
  §5.1 unit tests live in `placement.rs`'s `mod tests` (the module they exercise), not in `scenario.rs`; three of
  them (`outer_rings_skip_blocked_cells`, `outer_rings_do_not_cross_onto_a_separate_plateau`,
  `exhausted_region_is_a_typed_error`) change the built world's cells and call `place_ring_units` directly,
  because map validation rejects a structure on ring 4, an enclosed start and lone ramp or abyss cells.
- **Supply at setup (§2, §11): verified, not enforced.** `outer_rings_never_enter_another_starts_inner_zone` spawns
  500 units for one player and the property tests up to 1,100; `fixture_root_scenarios_spawn_exactly_their_units`
  loads both 200-unit armies of `battle_automaton`'s mix through the production loader. None trips a supply or
  allocation check.
- **C22 and `crates/gate/suites.ron`: no entry needed.** The catalog classes only targets that are not `fast`; a
  new integration test is `fast` by default. `cargo test -p gate --test suite_catalog_tests` passes with
  `scenario_placement_properties.rs` committed and no catalog change.
- **`error_not_enough_spawn_cells` (§5.3).** Its enclosed region is the map itself: on `base_valid_doc` (32×32)
  slot 0's region is 1,024 cells less its 25-cell hall and slot 1's reserved 11×11 zone, 878 cells, so 900
  units fail with `requested: 900, placed: 878`. An enclosure by terrain would fail map validation (starts
  unreachable).
- **Property generator (§5.2).** Starts are separated by construction (16 or more cells apart on one axis, axis and
  slot order drawn) rather than by `prop_assume!`, which rejected most draws. Abyss blockers are also made
  non-pathable, because validation rejects traversable abyss. Unit counts are 0 to 250 in two draws of three and
  850 to 1,100 otherwise, so that 32×32 maps exhaust the region and P4's failure half runs (about a third of the
  48 cases).
- **Checksums (§4.4).** Every shipped scenario and the three eight-player frame-budget scenarios have the same
  `last_checksum` before and after WP1; the table is in WP1's commit message.
- **WP2 done-when.** `fixture_root_scenarios_spawn_exactly_their_units` passes on the two-army spec (totals 0,
  200 and 400), and the harness's smoke run of the `400` scenario on `lavapipe`
  (`SPACE_SCUFFLE_GPU_TEST_DRIVER=lavapipe`, `frame_budget_smoke_run_writes_a_valid_report`: no warm-up frame,
  10 measured frames, every frame at least 95 % of 400 units in view) finished in 68 s with the camera pose
  unchanged.
