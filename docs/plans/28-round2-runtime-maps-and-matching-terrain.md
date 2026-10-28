# Round 2 — Playable versioned maps with matching terrain

Status: architect blueprint, awaiting orchestrator approval.

## Goal

The normal game client's Live Battle mode must load a versioned `.scmap.ron` map and a declarative starting scenario, simulate that map, and render that same terrain. Entering, resetting and returning to Live Battle must use this one initialization route. Units and structures must sit on their authoritative surfaces, the camera must frame the actual player start, and the terrain must present a readable game scene without editor overlays.

This is runtime integration, not map-editor work. Do not modify editor source, editing tools, serialization format, undo/redo, or editor UI. Existing `MapFile`, `MapDocument`, scenario placement and terrain code are inputs. Do not introduce unit slugs, formations, spawn coordinates or faction-specific branches into Rust.

## Evidence and current defects

`ClientAppPlugin::build`, `sync_live_battle_system`, and the Live Battle reset button independently call `SimWorld::new_default_battle_sandbox`. That constructor creates flat terrain, hardcodes army formation coordinates, and discards spawn errors. In contrast, `spawn_arena_environment` always spawns a fixed floor, cuboid plateau, cuboid ramp, glowing rails and decorative resource clusters. Neither geography nor resource positions follow the simulation map.

The real format and scenario path already exist: `MapFile::load` → `decode` → `scenario::build_world`, with `StartLoadoutCatalog` and `PlayerSetup`. Both real map files contain player starts; `showcase_canyon.scmap.ron` is exercised by current scenario tests. Plan 25 has now made ramp transitions, path smoothing, movement segments and arrival snaps share legal terrain traversal.

The shared `terrain::spawn_terrain` builds actual map chunks, baked texture arrays, theme lighting and a backdrop. `TerrainPlugin` updates only dirty chunks. However, its `TerrainParams::from_theme` enables the grid overlay by default, and the helper spawns lighting/backdrop without an explicit runtime scene owner. Gameplay must disable overlay bits and own the environment lifecycle.

The native baseline screenshots are `/tmp/space-scuffle-round1-terrain-native/{overview,p0_base,mesa,chasm}.png`. The architect inspected `overview.png`: the coarse bright fragmented deck surface overwhelms the scene, debug grid lines remain visible, the backdrop has large blurred stars, and no resource entities appear. These screenshots prove real rendering exists; they are not an accepted visual-quality target. Do not call the current terrain polished merely because its shader compiles.

## Coordination and ownership

This lane owns new `client/src/runtime_battle.rs`, a runtime map-scene module if needed, new runtime integration tests and a live-battle verification binary, `assets/scenarios/default_battle.ron`, and a declarative battle loadout addition if the shipped worker-only loadout is insufficient. Narrow edits are expected in `client/src/main.rs`, `lib.rs`, `lab_ui.rs`, `scene_controller.rs`, `arena_render.rs`, `camera.rs`, and shared terrain lifecycle helpers. Do not change existing map assets to hide a spawn or path defect.

Plan 29 owns catalog injection and the following agreed APIs:

```rust
SimWorld::with_catalogs(map, seed, UnitCatalog, UpgradeCatalog) -> SimWorld

build_world_with_catalogs(
    doc: &MapDocument,
    loadouts: &StartLoadoutCatalog,
    players: &[PlayerSetup],
    seed: u64,
    catalog: UnitCatalog,
    upgrade_catalog: UpgradeCatalog,
) -> Result<SimWorld, ScenarioError>
```

Its proposed `RuntimeCatalogs::load_from_directory(asset_root.join("catalog"))` supplies `units` and `upgrades`. Coordinate the final loader name with that lane; do not implement competing constructors or loaders. This lane consumes these APIs and owns runtime orchestration.

Plan 27 owns the mist subsystem, old mist removal from `vfx.rs`, new shader/configuration, and its observer systems reading bubble transforms. Reserve the old mist-call removal in `scene_controller.rs` and plugin-registration hunks for that lane. It can insert `DepthPrepass` through its own camera system. Terrain/camera ownership here must not overwrite those changes.

## Declarative runtime configuration

Add a typed `BattleScenarioFile` owned by the game runtime, with version, relative map path, RNG seed, and `Vec<PlayerSetup>`. Its default file belongs in `assets/scenarios/default_battle.ron`. Map/loadout/unit choices live in that file and existing catalogs. Start with `showcase_canyon.scmap.ron`, whose two-player scenario construction is already covered by tests; both versioned map files must remain selectable and tested. The runtime default path is a general asset location, not a creature identity.

Use existing map start locations and `build_world_with_catalogs` for all placement. Add a battle start loadout declaratively if necessary to provide actual mobile combatants in addition to harvesting units. Validate count, slug existence and placement through the existing scenario API. Do not silently substitute a different archetype or drop failed spawns. The chosen scene should retain sufficient open space for real movement and camera readability.

The default Live Battle must remain a meaningful combat session, not an idle-worker terrain demo. Its scenario must select combat-capable loadouts and include explicit initial-order data such as `AttackMoveToStart { player_slot, target_player_slot }`. Resolve targets from the referenced map starts and issue the order only to that player's mobile, armed combatants by capability; workers retain their own economy role. Validate source/target slots and reject a combat scenario with no eligible combatants. Keep this command policy declarative and reusable for other maps. UI reset reissues the same scenario orders; no hardcoded attack destination belongs in Rust.

Expose a `--scenario <path>` game CLI option (and optionally `--map <path>` as an explicit override if it does not complicate the contract). Resolve relative scenario and referenced asset paths against the already validated `ResolvedAssetsRoot`. Use explicit absolute paths for loading and the same root in Bevy's `AssetPlugin`. No current-directory search fallback after a user specifies a path. Reject unsupported versions, malformed configuration, missing map/theme/catalog assets, duplicate/invalid player slots, and scenario placement failures with typed errors retaining path context.

Keep legacy `run_app(mode, max_frames)` compatibility if tests/embed callers need it; introduce an options-based runtime entry point rather than scattering environment switches. `ClientAppPlugin` must not perform fallible disk IO inside `Plugin::build` or initialize a different embedded world before startup.

## Initialization and reset transaction

Create a pure fallible loader returning a prepared battle value: scenario metadata, decoded document, injected-catalog `SimWorld`, resolved terrain theme, immutable initial map presentation data, and camera focus information. Read and validate everything before mutating active ECS state. Load unit catalogs through Plan 29 and share exactly those definitions with rendering; do not instantiate a separate embedded renderer catalog.

Represent lifecycle explicitly, for example `BattleSessionState::{Uninitialized, Ready, Failed}` with a typed stored diagnostic. A reset request is a request consumed by the loader, not `LabState` directly constructing a world. Prepare the replacement first; once preparation succeeds, replace the world, clear tick accumulation, selection, battle render-entity mappings and transient FX, then install the replacement scene. A failed replacement must show/log its specific error and must not quietly launch the flat sandbox. Initial startup failure returns nonzero in automated/CLI verification; interactive reset failure can retain the previous playable session with a visible concise error.

Remove all normal-game Live Battle calls to `new_default_battle_sandbox`. Legacy simulation fixtures may retain that constructor for compatibility until Plan 29 handles them. Entering Live Battle lazily and launching with `--battle` must reach the same loader. The reset button should read “Reset battle” and request the same scenario again.

Audit battle UI command targets currently hardcoded to `(10,32)` and `(45,32)`. Resolve training commands from the selected player's actual start/army and the opponent's actual start, or remove a command whose meaning cannot be defined for arbitrary maps. Do not keep buttons that send every loaded map to unrelated coordinates. This is runtime correctness, not an editor feature.

## One terrain source and explicit scene ownership

Initialize the terrain renderer from a clone of the exact scenario world's map after placement. This is a render-side snapshot made on scene creation/reset; do not clone maps every frame and do not let rendering mutate authoritative simulation terrain. Fixed-point-to-float conversion remains at the presentation boundary. Resource positions, watchtower positions and terrain heights must come from that same map/document, never from the old decorative arena loops.

Add a runtime scene owner/root marker for terrain chunks, lighting, backdrop and map decorations. The shared terrain spawning helper should expose created entity ownership in a backwards-compatible way, or accept an optional scene owner through a new runtime wrapper. Preserve existing editor call signatures and behavior without modifying editor source. Avoid enumerating unrelated world entities after spawn to guess which ones belong to the battle. Cleanup must remove precisely the entities and terrain resources belonging to the replaced scene, with no accumulated lights, duplicate chunks or dangling handles.

Set gameplay `TerrainParamsState.overlay_flags` to zero and synchronize the actual material parameters before first accepted capture. Do not change the editor's overlay defaults. Keep topology and surface elevation invariant; shading tuning cannot move a ramp away from its legal traversal corridor. Ensure units remain grounded on `RenderSnapshot.world_pos[2]`, with sphere radius offset once and structure base offset zero.

Restrict `spawn_arena_environment` to the showcase/stress context if those views still need their current staging. It must never coexist with the map-derived Live Battle terrain. On tab switches, scope visibility and lighting so the staging arena cannot bleed into battle and the battle scene cannot obscure the close-up hero view. Reusing initialized battle state on return is fine; duplicate scene construction is not.

## Map resources and visual presentation

Use the existing authored mineral, gas and watchtower images referenced by `assets/catalog/map_entity_visuals.ron` where the runtime scene renders those objects. They are real assets, not permission to synthesize substitute crystals or use debug cubes. A small runtime reader can consume the presentation fields it needs; it must not instantiate `EditorDocument` or enable editor marker/UI systems. Generic resource-kind dispatch is appropriate; creature-specific dispatch is forbidden. Place each visual at its actual map position and altitude, anchor it to the ground, and ensure the black sprite background does not appear as a rectangle. Share materials/meshes and update billboarding in place without per-frame allocations. Do not draw start-location editing emblems in gameplay.

Track dynamic depletion/capture from immutable world presentation data at a deliberate update boundary. If the current snapshot lacks those fields, introduce a small documented map-entity presentation snapshot rather than read authoritative state from arbitrary rendering systems. Do not leave decorative resources visible after the game removes them. Limit updates to changed entities and reuse buffers.

First tune the runtime scene using actual camera exposure, existing theme lighting, roughness, normal strength and texture scale. Diagnose why the native baseline looks bright and fragmented before changing assets: verify texture array layer mapping, normal map convention, color space and material parameter packing. Do not solve an incorrect binding by darkening unrelated colors. If authored deck texture remains unsuitable after correct shading, reject it and schedule/regenerate a proper asset through the image-generation workflow; do not hide it with fog or make a procedural fake. Any material/theme adjustment must retain a real before/after screenshot and pass shared terrain tests.

## Camera and interaction

Initial camera focus comes from the active player's actual start/army bounds, with terrain elevation included. Keep a sensible readable RTS distance that shows its structures and first mobile formation, rather than framing empty map center or the entire map at unreadable unit scale. Add a map-aware optional camera-bounds resource and clamp game panning to useful terrain extents; preserve hero-mode behavior when absent. Compute the camera eye and target from a consistent elevated focus so moving onto high ground does not bury the camera or distort pitch. Respect UI focus and control chords.

Movement command picking must intersect the actual terrain via `terrain::pick_terrain` where the existing command path projects onto a flat plane. Verify coordinate mapping: simulation `(x,y)` maps to Bevy `(x,height,z=y)`. Reject hits outside legal map surfaces; do not issue commands to the backdrop plane. Selection and resource entities must remain inside the active camera's frustum at the start pose.

## Functional and GPU verification round

1. Pure loader tests load the real scenario and both existing map files through exact runtime paths. Assert map dimensions/cells, player starts, archetype counts, owner IDs, nonoverlapping valid placements, theme selection and injected catalog identity. Run duplicate initialization/steps and compare authoritative checksums. Negative tests cover every new error variant, including missing actual asset files; no dummy loader path is allowed.
2. ECS lifecycle tests launch Live Battle, reset, switch to the lab and back, and assert exactly one battle scene, one intended lighting rig, matching terrain map cells, unchanged chosen scenario, valid render mappings and cleared transient effects. Assert all overlay bits are zero in both CPU parameters and uploaded material data.
3. Run real move commands along a loaded ramp and near a forbidden side cliff using the production world and renderer. Record the unit's snapshot altitude and rendered transform; verify legal traversal and surface contact throughout. Keep Plan 25's multi-thousand-tick determinism regression intact.
4. Use a dedicated game verification binary or production `--verify-render` path that shares the normal runtime loader, renderer and asset root. It must inspect `PipelineCache` for compilation errors, require texture load success, wait for stable chunks and assets, and capture the actual game scene with simulation tick/frame identifiers. Fail on missing GPU, failed shader, absent scene entities, stalled readiness or timeout; do not report a skipped render as success.
5. Capture at least player-base overview, close ramp ascent, high-ground structure/units, and reset/reentry. Include a fixed-seed frozen-tick still for direct comparison and a short frame sequence during movement for grounding/mist compatibility. Record resolution, adapter, asset root and scenario path with captures.
6. Inspect every screenshot. Acceptance requires readable mobile units and building silhouettes, consistent terrain and unit heights, no duplicate old floor/plateau, no debug grid/brush/validation overlays, no detached opaque sprite rectangles, no material washout, and a balanced backdrop that does not compete with gameplay. Compare the native baseline explicitly; merely creating screenshots is insufficient. Reject and iterate on any amateurish result, retaining the failed evidence and reason.
7. Run `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --all -- --check` after coordinated source freeze. Reviewers should rerun the loader/lifecycle tests and inspect the captured images independently. Rendering success and simulation success are separate required evidence.

## Completion boundary

This round is complete only when the default game session actually uses the configured map/scenario, reset and tab lifecycle are reliable, map entities correspond to simulation data, and real screenshots satisfy the stated visual criteria. Catalog injection is supplied by Plan 29 and mist by Plan 27; integrate their final APIs and rerun shared verification. Do not conceal unresolved renderer defects or call screenshot capture alone a visual approval.
