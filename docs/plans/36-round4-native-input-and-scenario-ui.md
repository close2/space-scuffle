# Round 4: verify native input and present the selected battle truthfully

Status: approved, including responsive containment amendment; implementation held until the coordinated renderer/capture workspace gate releases the source freeze.

## Goal and boundary

A player using the real native window can pan, zoom, drag, bookmark, select, switch tabs and operate the visible controls predictably. The live battle panel describes the loaded map, participating player slots and actual roster, rather than the old fixed sandbox. Native input evidence is required: capture34's direct state assignments prove rendering checkpoints, not keyboard/mouse delivery or egui arbitration.

This round owns client camera/input policy, selection scheduling where necessary, read-only battle presentation metadata, the battle panel's copy and an explicit native-input verification harness. It does not edit the editor, map format, combat rules, terrain art, optical settings or renderer compiler. Preserve renderer31 safe shutdown and capture34's shared graphical builder. No hardcoded unit identities or silent fallback scenarios.

## Current source findings

- `camera_control_system` reads raw `ButtonInput`, `MouseMotion` and `MouseWheel` without egui keyboard/pointer ownership or focused-window checks. Unlike selection, wheel/drag can move the camera while interacting with UI. Mouse-wheel events are not filtered by their window. Scroll units are not distinguished.
- Keyboard W/up currently adds world Z. With the camera at positive Z looking toward its ground target, screen-forward points toward negative Z. This is a concrete sign hypothesis to verify by projection/native input before changing it; a pure `pan(Vec2)` test does not establish intuitive key behavior.
- Input, selection and UI systems share Update without a clear documented order relative to egui input processing or camera transform propagation. Native high-DPI pointer selection and stale GlobalTransform risk need measured tests, not assumed correctness.
- `RtsCamera` bookmarks are stored in the camera component, with tab presets managed by `sync_scene_camera`. Existing tests check helper math and direct tab state changes, not actual key/button delivery, focus loss or UI capture.
- `render_live_battle_tab` still labels fixed Squad A/B doctrines and colors, "2-Army Deterministic Sandbox", and an unavailable "Initialize Sandbox" action. The actual default scenario has two instances of the same declarative loadout. Player controls assume slots0/1 and always show Stimpack even when no eligible capability exists.
- A separate command-submission issue was found: `issue_sandbox_command` calls `world.step` directly from egui and discards the snapshot, whereas `sync_live_battle_system` owns fixed25Hz stepping and combat-event transfer. This can advance a paused world and lose visual events. Do not fold that simulation/control-flow refactor into an input verification patch; record it for the next approved command-queue blueprint. This round must not claim command timing fixed.

## Implementation stages

### 1. Establish native delivery evidence before altering behavior

Use the actual shared graphical builder and selected scenario, with a dedicated verification option/binary that adds read-only evidence recording. Record frame number, focused window, logical/physical viewport dimensions and scale factor, observed input events, camera target/distance, selected simulation entity, tab, and egui ownership. Allocate bounded evidence storage once; write it at checkpoints rather than allocating strings or growing maps every frame. The harness may pause the battle and position a starting camera as a documented fixture, but all tested actions thereafter must be real window keyboard/mouse events, not mutations of `RtsCamera` or `LabState`.

On the available X11 backend, discover the exact process-owned window, activate/focus it and verify focus before XTest events. Use press/release with measured hold intervals; `xdotool key --window` alone is not proof of focus or sustained state. Record tool exit codes and input evidence. Distinguish unsupported desktop automation from a game-input bug. Read the locked Winit/Bevy event handling when evidence is ambiguous. Do not silently replace native tests with injected ButtonInput state and call them end-to-end.

Required before/after screenshots and state evidence: W/A/S/D and arrows; diagonal speed; wheel over terrain; wheel over egui; middle drag over terrain and over UI; Ctrl+F-key save and recall; focus loss during held input; primary click on a visible bubble at the actual high-DPI screen coordinate; click over UI; real tab click/return preserving camera; pause/reset button hit tests. Verify no unintended action behind the UI and no camera jump when events queued during focus loss are later released.

### 2. Centralize input ownership and coordinate semantics

Define a small client input-ownership resource updated after egui has processed its input, before camera/selection consumers. Use explicit keyboard and pointer ownership plus focused primary-window status; do not ask independent systems to guess UI capture at different times. Non-Egui/minimal test setups need an explicitly configured policy, not an invisible permissive production fallback. Camera/selection consumers should drain irrelevant mouse events to prevent delayed jumps.

Specify directions in camera-ground space, derive forward/right from the camera convention, and normalize diagonal pan. Wheel zoom must distinguish line and pixel deltas with a documented configurable sensitivity and monotonic clamped distance; filter to the camera's window. Keep world bounds and terrain-following focus behavior for the loaded map. Middle-drag coordinate conversion must respect actual logical pointer deltas and zoom. Avoid adding per-frame allocation or a new generalized input framework for this small policy.

Schedule camera state before its transform update, and selection after the current camera transform is available. Keep renderer extraction after that state. Use a deliberate system-set order that coexists with runtime tab-camera synchronization and capture34 controls. No ad hoc duplicate camera systems. Bookmarks stay with the intended per-tab camera; document whether elevation is recomputed on recall.

### 3. Derive read-only UI from loaded data

Build a `BattlePresentation` snapshot when a definition/session is loaded or reset. Derive map title from `MapDocument` metadata, participating slots from scenario players, team color from the same runtime color catalog, and roster display names/counts from actual instantiated entities and `UnitCatalog`. Use stable slot/type order and reuse prepared label data. Dynamic live entity totals can update when counts change; avoid catalog-wide allocation in every egui frame.

Render a compact, truthful battle panel: selected map, player/roster summary, current tick and pause state, actual reset action and visible errors. Remove fixed fictional squad doctrines, stale initialization copy and hardcoded color prose. Controls should identify the actual controlled slot and enabled capability. Capability gating may hide/disable unsupported orders; do not branch on unit identity. Arbitrary noncontiguous scenario slots must not panic or silently target player1. If selecting a controlled player/opponent requires presentation state, initialize it from the actual scenario's ordered participants and validate on reset.

Keep implementation/debug metadata out of ordinary labels; checksum may remain in an explicit diagnostics section. Do not redesign the whole Visual Lab UI or add new unit-control mechanics. The direct-step command defect remains a separately stated follow-up unless the orchestrator approves a dedicated amendment before implementation.

## Tests and visual acceptance

- Unit tests cover camera-ground direction, diagonal normalization, line/pixel wheel conversion/clamps and event ownership decisions. Expected screen movement must come from projection math, not mirroring implementation signs.
- Integration tests exercise real Bevy input events and the scheduled ownership/camera/selection systems, egui focus blocking, focus loss, window IDs, current-frame transform selection, tab bookmark preservation and reset.
- Data-driven presentation fixtures use at least two different existing loadouts and noncontiguous slots; verify displayed roster derives from instantiated catalog entries. Missing catalog references are explicit errors, not placeholder names.
- Native process verification uses real assets/shaders/renderer, records actual event delivery and screenshots, and closes through renderer31. A test passes only if the expected camera/selection/tab changes occur and undesired changes do not. Black/stale captures, unsupported automation, missing evidence, timeout or Vulkan validation error fail explicitly.
- Inspect screenshots for legibility, honest labels, no clipping, sensible panel footprint and visible playable space. Reject stale descriptions, a panel that obscures the intended interaction target, or a technically correct but amateurish layout; iterate against the same screenshots and goals.
- Run coordinated `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, then repeat the native input run using the built binary.

## Ownership and subsequent round

Proposed files: `camera.rs`, `selection.rs`, narrow `lib.rs` scheduling, `lab_ui.rs` battle-only rendering, a focused input policy module and runtime battle presentation module, integration tests and native verification entrypoint. Coordinate exact `lib.rs` hunks with capture34; renderer31 and optics35 remain untouched.

Next blueprint: queued client commands at the fixed simulation tick boundary, capability-eligible recipients and preservation of every returned combat event/diagnostic. That work should independently verify pause/step semantics and determinism; it must not be hidden as a UI label change.

## Locked schedule details to preserve during implementation

Bevy-egui0.33 runs InitContexts→ProcessInput(after Bevy InputSystem)→BeginPass in PreUpdate. Ownership sampled only before BeginPass can be stale; the policy must consume the current pass's ownership after battle/VisualLab UI construction. Current camera Transform writes occur in Update while GlobalTransform propagation occurs in PostUpdate, so same-frame selection should explicitly follow TransformPropagate rather than accidentally use the previous frame's camera pose. Preserve capture34's controls-before-game-systems constraint when adding these orders; do not move its readback before final render output.

Native automation must combine xdotool search predicates explicitly with `--all` and independently verify `_NET_WM_PID` equals the spawned process before every targeted action. Its default search combination is OR and can select another concurrent Space Scuffle preview despite `--pid`. Coordinate exclusive focus/global-keyboard time with the orchestrator; a successful command exit alone does not prove it targeted the requested process. Record exact PID/window identity and invalidate evidence on any mismatch.

## Amendment: responsive containment at the actual default resolution

Capture34's actual1280×720 VisualLab image (`/tmp/space-scuffle-r4-runtime-captures-run03/05-visual-lab.png`) shows the inspector extending below the viewport and covering the first selected hero and left portrait area.1920×900 capture is useful additional evidence but does not repair the default product. This amendment adds narrow containment/visible-target responsibility to36; it does not authorize a wholesale VisualLab redesign.

Retain the existing controls and information hierarchy. Bound the existing inspector to available logical screen dimensions, reserve the existing bottom HUD where present, and scroll overflowing control content within that bounded panel. Derive available space from current window/egui rectangles and scale factor, with named layout spacing rather than screenshot-specific pixel offsets. Keep all controls reachable at1280×720,1920×900 and the actual high-DPI scale. Panel wheel input belongs to the panel and must not zoom the world.

A bounded panel alone is insufficient if it still covers the selected interactive hero. Define an explicit unobscured world interaction rectangle from measured panel/HUD bounds and verify the selected subject's projected footprint is visible within it. Prefer coherent viewport/framing adjustment for the primary camera if necessary, respecting logical-to-physical coordinates and existing viewport-aware picking; do not move units, assign unit-specific offsets, shrink subjects into unreadability or alter art to hide occlusion. Any viewport adjustment must be shared by projection, selection and capture evidence and must handle resize/tab transitions. Preserve the established per-tab camera state and34's scripted camera targets.

Add scheduled layout/viewport tests for bounds and DPI conversion plus actual native screenshots at default1280×720,1920×900 and non-unit DPI. Verify the selected hero, portrait, tab controls and scrollable inspector controls remain visible/reachable; test clicks on those actual visible targets. Reject clipped content, offscreen bottom controls, obscured selected subject or a layout requiring the larger capture resolution to function. Keep these changes in the existing panel/container and primary-camera interaction contract; defer visual redesign or unrelated HUD content changes.

## Native automation and assertion design prepared during the source freeze

This is a design only. Do not start another native app or send external input while the coordinated gate/test windows can steal focus.

### Identity, delivery and observation protocol

1. Start the actual shared graphical builder in a dedicated native-input probe with the normal production systems. A documented initial fixture may pause simulation and establish a reproducible camera once; no tested action may thereafter assign camera, selection or tab state directly.
2. Discover windows with `xdotool search --all --pid CHILD_PID --name ...`; require exactly one target. Before every capture, focus change, pointer/key action, resize and close, query `_NET_WM_PID` and require equality with the still-running child. Record process PID/start identity, window ID and property result with the command. Fail on missing/ambiguous/mismatched targets; never fall back to a title-only/global window.
3. Reserve exclusive desktop focus time with the orchestrator. Activate only the verified window and independently check actual input focus before XTest key actions. Relative pointer coordinates target the verified window. Global key events are permitted only during that exclusive interval and after focus proof; no global Alt+F4 is needed. Close through verified WM_DELETE_WINDOW (`windowquit`).
4. The probe passively records native KeyboardInput/MouseWheel/MouseMotion/window-focus delivery plus post-schedule camera/selection/UI state. Use a fixed numeric observation record and bounded/reused buffers, not a fresh HashMap/String/Vec per frame. Evidence contains frame/time, key/button bitsets and event sequence, window focus, window and egui scale factors, viewport rectangles, UI ownership, camera pose, selected simulation ID, tab and simulation tick/checksum. File errors/overflow are explicit probe failures.
5. Write observable checkpoint/state-change evidence without changing gameplay state. The driver waits for specific delivered-event and state predicates with deadlines; a fixed sleep or successful xdotool exit is not acceptance. Save native window screenshots before/after each visual action and correlate them with observed frames. Missing native delivery is a harness/prerequisite failure, distinct from a delivered event producing wrong game behavior.
6. Native screenshots can use the actual process-owned X window, retaining the real UI. Do not use a synthetic scene, overlay-free alternative renderer or edited image. Renderer31's checked exit path remains active. Every intended checkpoint and fresh artifact must be acknowledged before a run is marked complete.

### Coordinates and visible control targets

Expose measured semantic interaction rectangles from the existing UI as read-only evidence: inspector bounds, HUD bounds, tab buttons, pause/reset, and an existing editable slider value. This instrumentation must not add a fake text field or change control behavior. Use actual egui Response rectangles and `pixels_per_point`; camera projection uses its actual logical/physical viewport. Store these coordinate spaces explicitly instead of inferring them from a screenshot's resolution or hardcoding click positions.

Convert egui logical points to native window-relative physical coordinates using the recorded egui pixels-per-point, accounting for its relationship to window scale. Verify the target lies inside the recorded window/visible content rectangle before an action. Unit selection targets come from the actual entity projection and projected bounds, with an unobscured visible point selected; do not pick an arbitrary screenshot pixel or only assert that some entity became selected. Reject controls clipped by the inspector/HUD or outside the window; the responsive amendment must make them reachable.

### Action/assertion matrix

- **Keyboard canvas pan:** verify focused-window key down and release, expected camera-ground direction, finite displacement and matching screenshot movement. Test axes and diagonal normalized speed at a nonclamped fixture position. Repeat arrows. Establish baseline evidence before correcting signs.
- **Keyboard UI ownership:** activate the editable value of an existing slider, verify egui wants keyboard input, send a pan-key character, and require camera/bookmarks unchanged. The slider may reject a nonnumeric character; consumption still must prevent world panning. Restore through ordinary UI input.
- **Wheel canvas versus UI:** verify native wheel event and pointer location; over world, zoom changes monotonically within bounds. Over the inspector's scrollable content, the panel scrolls where possible and camera distance stays unchanged. Test line-unit input natively and pixel-unit conversion with scheduled event tests when this device cannot emit it; do not label injected pixels native evidence.
- **Middle drag:** over world, received motion moves the camera in the projected grab direction; over UI, camera is unchanged. Release and move without middle pressed: no queued jump. Check results at two zoom distances and high DPI.
- **Bookmarks:** actual Ctrl+F-key save, pan/zoom, actual recall; compare target/distance within declared floating-point tolerance and inspect matched framing. Verify tab-local state survives real tab clicks.
- **Focus loss:** hold a pan key, focus a separately owned neutral verification window during an exclusive desktop interval, and observe the game losing focus. The game stops consuming camera input; release externally, return focus, and require no stuck movement or delayed motion burst. Verify identity for both owned windows; do not focus another user's application.
- **Selection/UI routing:** click an actual visible projected bubble; require its exact entity ID and visual selection indicator. A click on a UI control must not change viewport selection. Test current-frame camera movement followed by selection to detect stale GlobalTransform use.
- **Tabs/pause/reset:** click measured actual widgets, assert resulting tab/pause/session state and screenshot content. Preserve camera restoration and selected scenario on reset. Record current direct-step command timing as the deferred38 defect; do not certify command timing merely because a button receives a click.
- **Responsive layout:**1280×720,1920×900 and non-unit DPI screenshots must contain all required controls through legitimate scrolling, selected subject and portrait without unwanted occlusion. Verify native hit targets after resize; no larger-resolution workaround may satisfy the default-size case.

Scheduled integration tests remain valuable for exact numerical boundaries, unsupported window IDs and impossible device event variants, but are reported separately from native delivery tests. Report each checkpoint as pass, product failure or unavailable/failed automation with its evidence; never silently substitute one category for another.

## Proposed narrow amendment: selected actor identity binding

The actual1280 baseline shows the archetype selector initialized to the first catalog entry while the HUD and selected bubble describe the first showcase actor. The controls currently write a global archetype string unrelated to that selected actor. Make the existing selector represent the selected showcase actor's catalog identity, synchronizing on actual selection changes and initializing from that actor, not catalog order. If the existing scene does not support changing a showcase actor's archetype, expose its identity as a read-only selected-actor label rather than a selector that falsely claims a replacement occurred. Preserve HUD, selection, capability controls and actual scene identity together. Test a different showcase selection without hardcoded unit names and inspect screenshots for agreement between selected bubble, HUD and inspector. This adds no unit-spawn/replacement mechanic and no media fallback.

## Approved grab-geometry clarification

The initial distance/FOV/pitch multiplier is a center-plane linear sensitivity approximation, not general pointer-anchor preservation. Replace it with previous/current viewport rays intersecting the same horizontal plane at the current camera focus elevation. Translate the camera target by previous intersection minus current intersection. These rays use the current camera pose and the correctly offset viewport coordinates; the gesture must not use an old GlobalTransform. Camera projection/viewport dimensions must be current, and a resize/viewport change invalidates the previous drag sample so a held pointer cannot jump across coordinate systems. Parallel/horizon, behind-camera, nonfinite and out-of-viewport intersections produce no drag and reset the anchor. Keyboard/focus/UI ownership still applies, and release clears retained state.

This defines grabbing on the current focus plane; it does not claim exact anchoring to every point of uneven terrain. Verify actual projected displacement at off-center positions and two zoom distances, plus numerical ray/plane boundaries and native dragging. A formula-only multiplier test is insufficient evidence for this behavior.

## Fatal-state and empty-viewport closure

All progression/order controls are disabled while either a retained media failure or a retained simulation failure is displayed. Tabs and the media audit remain readable. A simulation failure offers an explicit Reset failed battle action outside disabled controls, rebuilding the same validated scenario; media failures cannot be repaired by a simulation reset and offer no such recovery action. A nonexistent or empty world rectangle deactivates the world camera until a valid rectangle returns, rather than retaining a visible stale viewport across UI or a minimized window.

## Approved direct-schedule fixture migration

The full workspace gate and isolated backtrace identify `round2_visual_tests::test_bevy_ecs_schedule_all_systems_no_conflicts` as a direct system embedding that omitted the newly required `InputOwnership` resource. Register its explicit noninteractive default, matching `run_headless_check`: this fixture has no native window/egui ownership pass and must not implicitly authorize input. Keep the production resource required, all existing systems scheduled, and the real `setup_bubble_assets_system` loader/registry/media status behavior intact. Do not add an optional production fallback, skip the fixture or substitute fake media. Run the complete round2 visual suite and remaining runtime/diagnostic suites before the next full gate.
