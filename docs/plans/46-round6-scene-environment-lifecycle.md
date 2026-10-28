# Round46: restore scene presentation resources across game tabs

Status: implementation released after the repaired full workspace gate and independently verified plan35 source snapshot; focused implementation verification is in progress before plan38 scheduling changes. This blueprint addresses the confirmed presentation-resource leak; it does not change camera bindings or expand plan36's repair window.

## Goal and observed evidence

Returning from Live Battle to Visual Lab must restore the Lab's original clear color, ambient light and visible light rig together with its already-preserved camera. Scene presentation must depend on the active scene, not the sequence of tabs previously visited. The same contract applies to Swarm, repeated switches and battle reset. No editor development, shader/style adjustment, art replacement or simulation change belongs to this round.

Native run `/tmp/space-scuffle-r6-input-measured04` restored identical camera target/distance and actor identity, but root compared the initial Lab and returned Lab PNGs and found the gray background changed to near-black. Read-only source review confirms the mechanism:

- `arena_render::spawn_arena_environment` installs the Lab directional light and global AmbientLight during Startup. It leaves ClearColor at the renderer's original value.
- `terrain::theme_gpu::setup_theme_lighting_owned` parents its directional light to the battle root, but unconditionally replaces global AmbientLight and ClearColor even when an owner is supplied.
- `runtime_battle::update_map_presentation` switches scene-root/ArenaMapMarker Visibility only. Hiding a hierarchy does not restore resources.
- The current authored battle void color is sRGB(0.01,0.012,0.03). Its ambient settings happen to match the Lab today; this does not establish correct lifecycle for another authored theme.
- The locked Bevy renderer extracts global AmbientLight/ClearColor. Directional light extraction uses ViewVisibility; ownership and visibility propagation must therefore complete before extraction. Parent ownership alone does not control globals.

The material/shader optical settings and camera exposure are not changed by battle creation in the inspected code. Do not invent additional state restoration or overwrite unrelated render settings speculatively.

## One explicit game-scene environment owner

Introduce a small runtime-only environment value containing ClearColor and AmbientLight. Capture the actual Lab baseline once in PostStartup, after the real arena Startup commands have applied. Do not capture at plugin registration (before arena initialization), reconstruct magic duplicate colors, or capture after the first battle has overwritten globals. Swarm currently shares the same arena presentation and should explicitly use that baseline; do not invent a separate palette.

A successfully committed battle scene supplies its environment from the authored theme used to create that exact scene. Associate this value with the committed scene root/generation; never read the newest candidate BattleDefinition directly when deciding the active environment, because candidate replacement can fail while the old scene remains visible. A focused scene-environment state records the baseline and last applied committed scene generation. Update existing resources only on relevant scene/environment changes; there is no per-frame allocation or catalog parsing. Use a typed initialization/invariant failure if a live game tries to switch without its required captured baseline. Direct schedule fixtures must explicitly install/capture the required resources, not gain a silent default production fallback.

Global scene state has one writer in the normal game. Owned terrain construction must only create its owned light entities and backdrop; it must not mutate global environment resources. Preserve the existing standalone `spawn_terrain` / `setup_theme_lighting` behavior when no owner is supplied, so editor/standalone consumers remain backward-compatible without editing editor sources. Clarify the public owned-lighting contract in rustdoc and test both ownership modes.

A battle replacement must prepare/validate its required scene data before replacing the active scene. Only the active successfully prepared scene determines global presentation. Do not let a failed reset or partial terrain load leak the candidate theme into the old visible scene. If preparation fails, keep the previous valid environment and show the existing fatal diagnostic. This is a resource-lifecycle correction, not an asynchronous loading redesign; the independently observed cold-load stall remains recorded separately.

## Visibility and schedule contract

Keep camera state ownership in the existing camera lifecycle. Move only the root/ArenaMapMarker visibility policy into the same focused scene-presentation stage if necessary to make ordering explicit; map-object snapshot/billboard work remains its own responsibility.

Apply active environment and root visibility after Update's UI tab selection and battle setup/deferred commands, then before Bevy VisibilitySystems::VisibilityPropagate and light extraction. Do not write global clear/ambient state from the render world, consume UI events twice, or introduce a second scene controller. Ensure a reset in the same active tab re-evaluates its environment generation even though the tab enum is unchanged. Retain per-tab camera restoration and the input/viewport contract unchanged.

A just-clicked battle tab can precede its first committed root by one schedule boundary. Treat this as pending real scene creation: keep the previously active presentation until a valid root commits, rather than treating legitimate absence as corruption or inventing a fallback battle. A failed first load keeps that prior presentation plus the existing failure diagnostic. A failed replacement retains the previously committed battle profile/root. Test these boundaries explicitly.

Theme-owned directional lights and Lab-owned lights must not illuminate the wrong scene. Verify inherited/view visibility under the actual schedule rather than assuming hierarchy alone suffices. Preserve combat light37 shadow policy and renderer31 surface initialization/shutdown; the latter intentionally reads the active global ClearColor when initializing acquired native surfaces.

## Required tests

1. Capture a deliberately non-default Lab ClearColor/AmbientLight in a scheduled fixture, apply a distinct authored-style battle environment, and return to Lab/Swarm. Assert exact resource restoration, not approximate screenshot color guesses. Use arbitrary valid values so the current coincidental equal ambient settings cannot conceal a leak.
2. Repeat Lab→Battle→Lab and Battle→Swarm→Battle, including same-tab reset/replacement with a second distinct theme. Verify the baseline is captured once and is never replaced by a battle value. Check unchanged steady frames do not rewrite resources.
3. Test owned theme lighting creates the directional entity under its supplied root but leaves globals unchanged; standalone/no-owner behavior still applies its theme resources. No editor tests or sources are rewritten to force this result.
4. Exercise failed scene preparation/reset and verify the previously valid global environment remains intact alongside the typed diagnostic. No silent fallback scene or synthetic success is allowed.
5. Extend the real production schedule fixture to verify scene-root and light visibility after propagation at each tab. No ignored GPU teardown, leaked task or optional required-resource escape hatch.
6. Use the real native/capture builder and assets for a matched-camera initial Lab→Battle→returned Lab screenshot sequence. Keep selected actor, animation state, camera, viewport, exposure and image dimensions fixed where possible. Inspect background, actor lighting/shadows, visible light ownership and UI; compare a known unobstructed background region numerically only after inspecting the actual images. Battle art may remain rejected, but the environment must restore exactly. Include a second trip/reset and normal verified close with clean renderer diagnostics.

Run focused environment/schedule tests, the complete affected runtime suites, strict client Clippy/format checks and the coordinated full workspace gate. Report initial/returned image evidence and remaining unrelated art/loading/input limits explicitly.

## Ownership

Proposed files: new `client::runtime_battle::environment` module; narrow lifecycle/visibility hunks in runtime_battle/scene.rs and lib.rs; the owned-vs-standalone resource branch in terrain/theme_gpu.rs; runtime lifecycle tests and an appropriate actual capture sequence. Coordinate with approved plan38 before either edits scene reset/scheduling; avoid overlapping world/snapshot changes. No editor file, map format, map asset, theme style value or shader edit is authorized by this plan.

## Explicit custom-mist boundary

Independent review found that the current mist lighting synchronization queries directional lights without ViewVisibility/RenderLayers and chooses maximum illuminance before TransformPropagate. It can therefore select a hidden scene light even when46 correctly restores global environment and standard renderer light visibility. BubbleOptics already filters visibility/layers after CheckVisibility and provides a relevant existing contract. Do not edit mist during35's baseline/storage work or claim46 restores every custom shader's lighting. Record the mist behavior as a separate defect and coordinate a later narrow lighting amendment with the mist owner;46 validates global resources and actual standard-renderer light ownership only.
