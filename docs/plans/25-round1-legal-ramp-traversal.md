# Round 1 — Legal ramp traversal throughout game navigation

Status: architect blueprint, awaiting orchestrator approval before implementation.

## Goal and scope

Ground units must climb and descend only connected ramp surfaces. A ramp is not permission to cross an arbitrary cliff. A* search, clearance, funnel smoothing, steering, collision push, and flee movement must agree on the same terrain transition rule. The implementation must demonstrate this behavior with the existing versioned map files as well as deliberately hostile runtime maps.

This is game runtime work. Do not change the editor, map file schema, map loader, terrain renderer, existing map assets, or altitude interpolation. Do not add creature identifiers. Preserve the dirty working tree, including the existing `grid.rs`, `lib.rs`, `world.rs`, map format, and ramp-profile changes.

## Observed defect and integration points

F-03 in `docs/reviews/2026-10-26-codebase-findings.md` is still present. `pathfinding.rs` currently allows a cliff transition whenever either endpoint has `CellFlags::RAMP`. This is duplicated in clearance expansion, A* neighbor expansion, and portal widening. `SimWorld::is_position_pathable` independently grants the same exemption using `ramp.is_some()`, and checks only the destination cell. Its consumers include ordinary steering, chasing, sliding, collision displacement, and flee candidate selection. Fixing A* alone would therefore leave actual motion unsafe.

`GameMap::ramp_profile` already computes the cardinal ramp direction, lower cliff level, run index, and run length. Cardinal ramps are bounded by `MAX_RAMP_RUN_CELLS`. Legacy diagonal profiles exist for altitude compatibility, but map format version 1 rejects diagonal ramps. The new-format route already exists: `MapFile::load` → `decode` → `MapDocument::to_game_map`. The two real files are `assets/maps/default_arena.scmap.ron` and `assets/maps/showcase_canyon.scmap.ron`.

There is currently no navigation line-of-sight/segment traversal helper. Vision's Bresenham rays implement visibility, not continuous motion, and must not be repurposed as a movement oracle.

## Ownership

Implementation owns a new `crates/sim_map/src/traversal.rs`, a minimal module declaration in `crates/sim_map/src/lib.rs`, `crates/sim_map/src/pathfinding.rs`, new `crates/sim_map/tests/ramp_traversal_tests.rs`, and new `crates/sim_core/tests/ramp_movement_tests.rs`. It owns only the documentation/body of `SimWorld::is_position_pathable` in `world.rs`; coordinate this narrow edit with the checksum lane. Do not rewrite or format unrelated portions of dirty files. No grid change should be necessary: implement methods on `GameMap` from the new module.

## Authoritative terrain topology

Add a documented, allocation-free `GameMap::terrain_step_is_connected(from, to) -> bool` with integer cell coordinates. This answers terrain connectivity only. Structure occupancy, player knowledge, unit footprint, and unexplored-cell restrictions remain caller policy. Return false for out-of-bounds/nonadjacent cells and terrain cells lacking ground pathability or marked `UNPASSABLE_CLIFF`. Same-cell queries succeed only for usable ground.

Implement a private cardinal-edge predicate, then define diagonal connectivity in terms of it. Do not repeat ramp exceptions at callers.

For cardinal edges:

1. Two ordinary ground cells connect exactly when their cliff levels match.
2. Ground connects to the descending face of a cardinal ramp only at profile index zero, and only when its level equals the profile's lower level.
3. Ground connects to the ascending face only at the last run index, and only when its level equals lower level plus one. Use checked arithmetic; a level-255 ramp cannot wrap to level zero.
4. A ramp never connects sideways to flat ground, including flat ground whose stored level equals the ramp's lower level. Equal stored base levels do not imply equal surface heights.
5. Ramp cells connect along the run when direction, lower level, and run length match and their indices differ by exactly the displacement along the ascending axis. Adjacent parallel ramp lanes connect sideways when direction, lower level, run length, and index match. This preserves wide ramps and rejects mismatched lane profiles or oppositely oriented ramps.
6. Reject malformed profiles with zero length, index outside length, or length greater than the published maximum. Navigation for legacy diagonal ramp cells is explicitly unsupported and returns false; do not silently turn them into cardinal ramps. Preserve their existing altitude behavior and document the distinction. An isolated `RAMP` flag without `ramp` direction must never authorize an elevation transition. Read the direction/profile as the geometric source of truth.

The rule is symmetric: if A connects to B, B connects to A. For diagonal neighbors A and D, require both cardinal routes around their shared corner to be terrain-connected: A→B→D and A→C→D. This prevents diagonal clipping around cliff/ramp side boundaries. Structure corner squeezing is a separate existing clearance policy and must not be accidentally redefined by this terrain-only check.

The profile rule can alternatively be expressed using exact rational surface heights at both endpoints of the shared edge, but the explicit profile cases above avoid rounding ambiguity and make legal ramp entrances obvious. Do not compare converted floating-point altitudes or use a height tolerance.

## Continuous segment traversal

Add a documented no-allocation `GameMap` segment query usable by both navigation and simulation, with a cell-policy closure. Suggested API: `ground_segment_is_connected(start, end, cell_allowed) -> bool`. Positions are fixed-point world units; the closure receives integer cell coordinates and supplies occupancy/knowledge/clearance decisions. The query owns terrain connectivity and invokes the closure for traversed cells. Direct simulation supplies a closure rejecting occupied cells; pathfinding supplies its request-aware policy.

Use deterministic grid traversal that visits every cell entered by the segment. Compute the ordering of next X/Y grid-boundary crossings using widened raw fixed-point integer arithmetic and cross products, not floating point or a fixed sample spacing. Bound traversal by the Manhattan cell distance plus one; each step must advance toward the endpoint. Handle zero-length segments, horizontal/vertical segments, negative directions, endpoints exactly on boundaries, and simultaneous corner crossings explicitly. Treat the map's floor-based half-open cell convention consistently. At a corner crossing use the diagonal terrain predicate; evaluate dynamic obstacle corner policy consistently with the request's established clearance behavior. A segment cannot skip an intervening wall merely because its endpoints are legal.

Keep APIs narrow: the terrain rule should not depend on `PathRequestWithVision` or `SimWorld`. A private pathfinder adapter may combine terrain traversal with existing diagonal building/footprint policy. Document any raw arithmetic and prove its bounds, using `i128`/`u128` intermediates where needed. Do not add allocations, panic paths, broad lint allowances, or a world-clock dependency.

## Pathfinding integration

### Clearance

Replace the current ramp wildcard in `recompute_clearance`. A square can expand only when its constituent adjacent terrain edges are connected. Use the authoritative predicate for the current/right/down/diagonal 2×2 boundary connections; do not merely compare all three neighboring cells with the current stored cliff level. Preserve the existing dynamic/static occupancy distinction and dynamic navigation-version behavior. A wide aligned ramp must retain the clearance needed by large units, while a ramp beside a disconnected cliff must not enlarge a clearance square across that edge.

### A* and perception

Replace the elevation wildcard in neighbor expansion with the shared step predicate. Preserve the structure-knowledge masks, fog restrictions, required-clearance checks, deterministic neighbor order, heuristic, and costs. Retain the existing building-corner policy separately; terrain connectivity must be checked even for Small clearance. Neither invisible obstacles nor blocked unexplored ramps may be reintroduced by smoothing.

### Portals and smoothing

Replace `base_is_ramp`/`base_cliff` wildcard logic in portal widening. Each newly added portal pair must be mutually connected, and each side of the widening strip must remain connected to the preceding pair. This keeps a wide ramp portal inside its actual connected surface. Do not widen across a laterally adjacent disconnected plateau just because the original portal touched a ramp.

The existing diagonal portal decomposition always uses the X intermediate cell. Check that portal construction follows the same terrain route policy as A*. With both cardinal terrain routes required, this is geometrically safe; existing structure squeeze behavior still needs its own tests.

Validate every segment of the returned funnel path with the shared segment query and full request policy, including unexplored-ramp restrictions. Corner offsets can leave a valid corridor, so validating only corridor cells is insufficient. Repair invalid string-pulling geometrically: retain the ordered legal corridor-center chain, then greedily choose the furthest visible next chain point whose segment satisfies the same query. This is a documented deterministic constrained-smoothing stage, not an unchecked fallback or a reason to return an illegal path. Validate the final start/goal attachments as well. If a legal attachment genuinely cannot be constructed, return the existing typed path error. Reuse buffers and document any performance trade-off. Do not replace working open-ground funnel paths unconditionally or degrade all navigation to cell-center stair steps.

### Simulation movement

Replace the endpoint-only implementation of `SimWorld::is_position_pathable` with a delegation to the segment query and a physical occupancy policy. Its public signature remains unchanged. All current steering, chase, slide, collision-push, and flee callers inherit the same terrain check. This method must check long segments too: flee candidate selection passes positions several cells apart. Keep ability-specific teleport/blink rules outside this change unless they already call this method; report discovered coupling rather than silently imposing new ability semantics.

Implementation inspection identified three arrival snaps in `step_movement_and_steering` that assign the target/next waypoint directly when distance is below speed. These must also require `is_position_pathable` before assignment; proximity alone does not prove a legal segment across a ramp side. Add a real Move-order regression with source and destination less than one tick of movement apart on opposite sides of a ramp wall. This is a narrow additional world-method edit, separate from checksum plumbing.

## Fixtures and tests

Tests must assert independent expected outcomes, not just call the production predicate twice. Unit-test private cardinal and segment arithmetic in `traversal.rs`; use public APIs in integration tests.

1. Build a 16×16 synthetic map with a north-facing, three-cell-long, three-cell-wide ramp: x=6..8, y=5..7, lower level 1; lower landing y=4 at level 1, upper landing y=8 at level 2. Rotate/mirror this fixture for all four cardinal directions. Assert both directions across each legal entrance/run edge and sideways between aligned lanes.
2. Place pathable level-0, level-1, level-2, and level-3 flat neighbors against ramp side faces. Assert every side crossing is rejected, including level-1 same-base neighbors. Assert a level-3 upper landing and a level-0 lower landing are rejected. Block all alternative routes before asserting `NoPathFound`, so failure does not depend on accidental map connectivity.
3. Test mismatched ramp direction, lower level, adjacent lane run length/index, oversized run, missing direction with `RAMP` flag, top-level overflow, and legacy diagonal behavior. Inconsistent redundant flags must not grant a transition.
4. Test diagonal motion across the upper landing, within a wide ramp, across a ramp's side corner, and beside a cliff edge. Valid wide-ramp diagonals remain valid; a single invalid cardinal route prevents terrain corner cutting. Keep existing Small-versus-Large wall-off regression tests passing.
5. Segment fixtures: same cell; zero length; each axis and reverse; exact grid boundaries; simultaneous corner crossing; very shallow slope; a one-cell intermediate obstacle; an intermediate cliff with same-level start and goal; valid long ramp ascent/descent; out-of-bounds endpoints. Run reverse-direction symmetry checks for terrain-only queries. Check raw arithmetic boundary inputs without creating enormous maps.
6. Clearance assertions: a three-wide aligned ramp supports Large clearance along its intended route, a disconnected side plateau does not contribute clearance, placing/removing a real structure updates passability, and known versus unknown structures preserve the existing perception semantics.
7. Funnel regression: choose start/goal offset across opposite ramp sides so the unconstrained straight line cuts a ramp wall although a legal route exists. Assert successful reachability and verify emitted segments against an independently enumerated forbidden boundary list, including the start and goal. Also assert an open-ground request retains its straight two-point result. Add a fog-aware case to prevent smoothing through an unexplored ramp.
8. New-format integration: load both real asset paths using `CARGO_MANIFEST_DIR/../../assets/maps/...` exactly (no fallback path), decode, convert, and instantiate `Pathfinder`. Require at least one cardinal ramp in each map so coverage cannot pass vacuously. Discover each run's first and last cells from its profile, identify lower/upper landing cells, and verify ascending and descending route reachability for supported clearances where the physical aperture permits. For `default_arena.scmap.ron`, explicitly traverse from (31.5,29.5) to (31.5,34.5), assert legal crossings and monotonic altitude samples, and reject a direct segment through the adjacent cliff wall. Repeat identical requests and compare the complete `PathResult` bit-for-bit. Do not rewrite these assets to make assertions pass.
9. Runtime integration: call the public `SimWorld::is_position_pathable` on hostile/valid fixtures, including long segments. Then choose a ground-moving archetype through catalog capabilities (no slug or ID match), issue a real move order in a loaded default arena, advance deterministic ticks, and assert it reaches the opposite landing without crossing the forbidden side boundary. Run two identical worlds for several thousand ticks and compare checksums. Add a blocked target case proving steering/sliding cannot leak through the side wall. Keep tests bounded and avoid probabilistic criteria.

## Verification round and acceptance

First run the focused sim_map and sim_core traversal tests, plus existing funnel, wall-off, vision-pathfinding, and determinism suites. Inspect failures before changing any established fixture: a permissive old expectation may expose a real geometry defect, but do not weaken unrelated tests.

Full-workspace verification exposed old `sim_core/tests/fog_of_war_pathfinding_tests.rs` fixtures that set `RAMP` flags on different-level cells without any `RampDir`. Replace those nonexistent surfaces with three-wide east-facing lower-level ramp cells and ordinary upper landings. The wall-off must use real building placement on the lower landing, not flag mutation on an unbuildable ramp. Preserve the hidden-terrain/known-obstacle assertions; partial visibility may reveal only one of three entrance lanes, so require alternate-ramp repathing once the whole wall is known. The high-ground ascent test must assert that ascent actually occurs; its former assertion inside an optional branch allowed an impassable ramp to pass vacuously. Keep enemy behavior controlled by capabilities/HoldPosition so the visibility fixture measures ramp ascent rather than a scout dying under ranged fire.

Then run the required workspace gates: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --all -- --check`. Report exact failing tests or infrastructure issues, and coordinate shared compilation with the orchestrator. Only claim completion once gates pass or the orchestrator has recorded an external blocker explicitly. A verification agent must independently inspect the topology and segment corner cases and reproduce focused tests.

There is no visual output change in this lane; screenshots cannot prove movement topology. Future runtime terrain wiring is a separate visual round and must capture and evaluate screenshots before acceptance.

## Follow-up blueprint required for playable map loading

The client currently initializes LiveBattle with `SimWorld::new_default_battle_sandbox` in `scene_controller.rs`; the constructor creates a flat world while `arena_render.rs` draws unrelated arena geometry (F-01). A subsequent architect round should specify a runtime-selected `MapFile` path, typed load/decode/validation errors, catalog/scenario-driven spawn placement on legal cells, and rendering sourced from the exact same `GameMap`. It should reuse the existing terrain renderer without changing the editor. Define screenshot goals for matching plateau/ramp surfaces, unit altitude, camera framing, and actual shader pipeline success. That integration must own a coordinated world/client change after this lane and the checksum lane finish; it is not hidden inside this traversal fix.
