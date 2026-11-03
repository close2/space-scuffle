# Codebase Findings — 2026-10-26

> Collected while researching [Plan 21 — Map Editor](../plans/21b-plan-map-editor.md).
> These findings are **outside** the map-editor scope unless marked *(fixed by Plan 21)*.
> Each entry: location, problem, why it matters, recommended fix. Severity: **High** (wrong
> behaviour / determinism risk), **Medium** (charter violation / latent bug), **Low** (hygiene).

---

## High

### F-01 LiveBattle renders terrain the simulation does not have
- **Where:** `crates/sim_core/src/world.rs` `SimWorld::new_default_battle_sandbox` (≈L226) builds a
  **flat** 64×64 level-0 map; `crates/client/src/arena_render.rs` `spawn_arena_environment` draws a
  hard-coded two-level arena (16×3×16 high-ground cuboid, ramp cuboid, rails).
- **Problem:** Units walk "through" drawn cliffs and stand at altitude 0 while the scene shows a
  plateau. The visual layer is not derived from simulation data (violates AGENTS.md §3.2: the
  client must render what the snapshot/sim says).
- **Fix:** Migrate LiveBattle to the shared terrain renderer introduced by Plan 21
  (`crates/client/src/terrain/`) and boot it from a `.scmap.ron` file. Deferred follow-up of Plan 21.

### F-02 Multi-cell ramps produce a saw-tooth altitude *(fixed by Plan 21, Phase 1)*
- **Where:** `crates/sim_map/src/grid.rs` `GameMap::terrain_altitude_scalar` (L388–421).
- **Problem:** Interpolation is per cell: every ramp cell rises a full cliff step (3.0) within one
  cell. `GameMap::default_arena` has a 4-row ramp (y 30..34), so altitude goes 3→6, drops to 3,
  rises to 6 … four times. Projectile/visual-event heights and snapshot altitudes are wrong.
- **Fix:** Interpolate across the whole contiguous ramp run (Plan 21 §2.5).

### F-03 Ramp cells allow transitions between *any* cliff levels
- **Where:** `crates/sim_map/src/pathfinding.rs` ≈L347–349 and ≈L594–596
  (`n_cell.cliff_level == cell.cliff_level || n_cell.RAMP || cell.RAMP`).
- **Problem:** A ramp cell at level 1 lets units step onto an adjacent level-3 cell (or drop into
  level 0) sideways. Only the ramp's run direction and the L→L+1 landing should connect.
- **Fix:** Passable iff same level, or the pair is ramp-connected along the ramp direction
  (use `GameMap::ramp_profile` from Plan 21 §2.5). Plan 21 validation rejects malformed ramps in
  editor-authored maps, but the pathfinder itself should enforce it.

### F-04 `SimScalar` serializes through `f64` (lossy for Q32.32)
- **Where:** `crates/sim_types/src/math.rs` `impl Serialize for SimScalar` (L23–30).
- **Problem:** `I32F32` has 64 significant bits; `f64` has 53. Values with a large integer part
  and full fractional precision do not round-trip bit-exactly. Any snapshot/replay/lockstep payload
  that serializes `SimScalar` can silently diverge across save/load.
- **Fix:** Serialize the raw `i64` bits (optionally a human-readable decimal string for RON only,
  parsed exactly). Add a property test over random raw values. Plan 21 avoids `SimScalar` in the
  map file for this reason (integer cells / tenths only).

---

## Medium

### F-05 Silent failure paths in the battle sandbox
- **Where:** `world.rs` `new_default_battle_sandbox`: `let _ = world.spawn_unit(..)` /
  `let _ = world.spawn_structure(..)` (L326–355), `unreachable!` (L230), `.expect` (L253, L298),
  magic seed `0x5EED_BEEF` and hard-coded coordinates.
- **Problem:** Violates AGENTS.md §1.1 (no silent failures). A catalog change that breaks a spawn
  produces a quietly smaller sandbox.
- **Fix:** Return `SimResult<Self>`; move sandbox layout into data (a scenario RON).
- **Status (2026-11-03): closed** by Phase 9 WP10 (`2797d4a`): the sandbox left `sim_core` for the test-only
  `sim_fixtures` crate and returns `Result<SimWorld, SandboxError>`, reporting any spawn it cannot make; check C12
  rejects `let _ =` on a result in the simulation crates.

### F-06 `spawn_structure` burns an `EntityId` on failure
- **Where:** `world.rs` L406–414: `next_entity_id` is incremented before `place_structure` can fail.
- **Problem:** A failed placement still advances the ID counter → divergent entity IDs between two
  otherwise identical command streams if one of them contains an invalid build.
- **Fix:** Validate with `can_place_building` first, allocate the ID only on success.

### F-07 Player slots ≥ 8 silently alias slot 7
- **Where:** `world.rs` `player_upgrade_state(_mut)` (L435) `(player.0 as usize).min(7)`; same
  pattern likely for resources/vision arrays.
- **Problem:** Player 9's upgrades are player 7's upgrades. Should be a typed error or a
  compile-time bound (`PlayerId` constructor validating `< MAX_PLAYERS`).
- **Status (2026-11-03): closed** by Phase 9 WP11 (`331ccb4`): per-player storage is `PerPlayer<T>`, indexed only
  by a `PlayerSlot` proven to be below `MAX_PLAYERS`; a neutral or out-of-range player has no slot.

### F-08 Unit catalog is compiled into the binary
- **Where:** `crates/sim_core/src/catalog.rs` `new_starter_roster` uses
  `include_str!("../../../assets/catalog/units.ron")` (L313–314); `SimWorld::new` always uses it.
- **Problem:** AGENTS.md §3.3 requires rebalancing with *zero recompilation*. Also `get_by_slug` is
  an O(n) case-insensitive scan.
- **Fix:** Inject the catalog into `SimWorld::new` (loaded from disk by the caller); keep a
  `BTreeMap<String, UnitTypeId>` slug index.
- **Status (2026-11-03): closed** by Phase 9 WP10 (`2797d4a`): production code loads every catalog from disk at
  run time and embeds none (check C6); only the test-only `sim_fixtures` crate embeds the shipped files.

### F-09 Shader "tests" do not compile shaders; GPU test can pass without a GPU
- **Where:** `crates/client/tests/round2_visual_tests.rs` (`test_shader_wgsl_syntax_and_uniform_alignment`
  etc. use `shader_source.contains(..)`); `test_real_gpu_startup_and_frame_limit_30` (L521–560)
  returns early ("Skipping…") → **passes** when no display, and only checks the exit code.
- **Problem:** Bevy logs pipeline compilation errors but keeps running, so a broken shader still
  exits 0. AGENTS.md §1.2 requires real end-to-end verification and loud failure.
- **Fix:** Use the render-verification pattern from Plan 21 §7 (inspect `PipelineCache` for
  `CachedPipelineState::Err`, fail loudly when no GPU unless an explicit opt-out env var is set).

### F-10 Per-frame heap allocations in `sync_live_battle_system`
- **Where:** `crates/client/src/scene_controller.rs` L509 (`build_render_snapshot()` each paused
  frame allocates snapshot vectors), L537 (`format!` checksum string every frame).
- **Problem:** Violates AGENTS.md §3.3 "zero per-frame allocations".
- **Fix:** Reuse a snapshot buffer (`build_render_snapshot_into(&mut RenderSnapshot)`); only
  re-format the checksum when it changes, into a reused `String` (`write!`).

### F-11 Legacy `ScMapData` format serializes runtime state and is unversioned
- **Where:** `crates/sim_map/src/loader.rs` `ScMapData` (L12–26), `GameMap::from_data`.
- **Problem:** Serializes `MapCell.occupying_structure` and `OCCUPIED_BY_STRUCTURE`, has no version
  field, performs no validation, drops placed structures. Same `.scmap.ron` extension as Plan 21's
  `MapFile` (they are distinguishable: `MapFile` requires `format_version`).
- **Fix:** Deprecate `ScMapData` in favour of `sim_map::map_format::MapFile` (Plan 21); port the
  Tiled importer to produce `MapDocument`.

### F-12 Redundant ramp state can disagree
- **Where:** `grid.rs` `MapCell { ramp: Option<RampDir>, flags: CellFlags::RAMP }`.
- **Problem:** Two sources of truth; nothing enforces `flags.contains(RAMP) == ramp.is_some()`.
  `can_place_building` checks the flag, `terrain_altitude_scalar` checks the field.
- **Fix:** Derive the flag (Plan 21's file format does: `RAMP` is never stored, always derived).

### F-13 `remove_structure` restores flags unconditionally
- **Where:** `grid.rs` L557–567 inserts `BUILDABLE | GROUND_PATHABLE` regardless of the cell's
  original flags.
- **Fix:** Store the pre-placement flags in `PlacedStructure` and restore them.

### F-14 `resolve_assets_dir` check is a no-op and falls back silently
- **Where:** `crates/client/src/lib.rs` L100/L113: `candidate.join("shaders/bubble_dome.wgsl").exists() || candidate.exists()`
  accepts any directory named `assets`; final fallback `PathBuf::from("assets")` hides misconfiguration.
- **Fix:** Require the marker file; return `Result<PathBuf, AssetsDirError>` and fail loudly.

---

## Low

### F-15 Camera ignores terrain and has no bounds
- **Where:** `crates/client/src/camera.rs`: no clamping of `target` to the map, `look_at_y` fixed,
  WASD pans even while Ctrl is held (Ctrl+S would pan). `bookmarks: HashMap` (fine outside sim,
  but a `[Option<..>; 4]` is simpler).
- **Note:** Plan 21 adds an editor-specific camera controller instead of modifying this one.

### F-16 Blanket clippy allows contradict the pedantic lint policy
- **Where:** crate-level `#![allow(clippy::cast_*, clippy::too_many_lines, …)]` in
  `crates/client/src/lib.rs` and `crates/sim_map/src/lib.rs`.
- **Fix:** Allow locally with a justification comment where a cast is proven safe.

### F-17 `CliffLevel(pub u8)` accepts values > 3
- **Where:** `grid.rs` L16. Public tuple field; no constructor validation. Plan 21's map format
  validates `0..=3` on load.

### F-18 Altitude query hides out-of-bounds and duplicates the step constant
- **Where:** `grid.rs` L389–396 return `ZERO` on out-of-bounds; L419 `step_height = 3` duplicates
  `CliffLevel::height_scalar`'s factor. *(Constant duplication fixed by Plan 21, Phase 1.)*
