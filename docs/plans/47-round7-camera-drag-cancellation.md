# Round47: cancel camera drag ownership across focus loss

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Status: architecture approved, implementation released after the round7 combined workspace gate passed with unchanged source fingerprints. This bounded repair precedes38 camera integration. Focused implementation verification is in progress; native and subsequent integrated gates remain required.

## Evidence and precise problem

Root's `/tmp/space-scuffle-r7-input-held-focus01` proves native focus loss stops the camera, and the held W state clears on the next engine frame. It also records middle-button pressed state remaining1 through minimize and focus restoration. A later verified OS release/move did not produce the expected application event; the driver timed out and closed normally. This remains failed native delivery evidence, not proof that a stale drag actually moved the restored camera. The empty-world screenshot reflects the actual earlier W pan to approximately Z−10 and is not visual acceptance.

The second run `/tmp/space-scuffle-r7-input-held-drag02` repeats the retained middle state after twelve settled restored frames and a verified OS mouseup; mapped geometry and the owned window under the pointer were independently checked. It also fails its release observation and closes normally with exit0. Root inspected03 with all actors/UI visible. Neither run records raw MouseButtonInput press/release counters yet, so neither identifies whether the missing release originates in desktop delivery, the backend or later application handling. Preserve both failed runs.

Independent source inspection nevertheless establishes a bounded state-machine gap. `camera_control_system` clears its retained drag anchor whenever pointer ownership is denied, but arms a new anchor solely from `ButtonInput<MouseButton>::pressed(Middle)` when ownership returns. The next delivered cursor movement can resume a gesture that began before focus loss, without any new in-window middle press. The same predicate can begin a drag after a middle press originated over UI and subsequently entered the world. Clearing the anchor prevents an immediate delta burst; it does not cancel gesture ownership.

Locked primary source confirms the platform distinction: Bevy0.15.3 `bevy_input/src/keyboard.rs::keyboard_input_system` releases all keys on `KeyboardFocusLost`, while `mouse.rs::mouse_button_input_system` only processes actual button press/release events and clears transient bits. It does not release all mouse buttons on focus loss. Winit's `state.rs::forward_bevy_events` forwards an ordered `bevy_window::WindowEvent` stream as well as typed events, preserving cross-kind focus/button/cursor ordering. Do not patch Bevy or globally clear its input state to hide this application gesture problem.

Plan36 requires no stuck movement or delayed motion after held-input focus loss. An explicit middle-drag lifetime is justified by the existing source behavior, even though the native reproduction has not delivered the final post-restore movement. Keep that distinction in the verification report.

## Bounded gesture contract

Replace the optional anchor with a small private camera-drag state that owns a gesture only after a fresh primary-window middle press accepted by the current world/UI ownership policy. Holding a button is a continuation condition, never permission to create a gesture. Track the owning window and actual viewport geometry; retain the existing exact focus-plane ray intersection math.

Cancel the gesture on focus loss, owning-window destruction/replacement, pointer ownership loss, invalid/inactive viewport, changed viewport/scale, invalid ray intersection or a release. Returning focus or moving back into the world does not revive it. A new accepted native middle press establishes a new anchor and never applies displacement immediately from an obsolete coordinate. A valid press can rearm even if the engine's global held bit remained stale because a release happened outside the window; do not depend exclusively on `ButtonInput::just_pressed`, which may suppress a repeated press while its held set remains stale.

Use one ordered event path for middle-button/focus lifecycle so loss and restoration within one rendered frame still cancel the previous gesture. The locked engine's aggregate WindowEvent stream can supply that ordering without allocation or an engine patch. Retain only the primary window's latest delivered cursor position needed to identify where a press originated; do not reinterpret a final end-of-frame cursor point as the press location if intervening moves changed it. Require current egui ownership and a valid world press location. Unsupported or missing delivery leaves the gesture inactive rather than guessing a press. Read these events without consuming another system's reader or mutating global button/keyboard resources.

Do not expand this into a generalized action framework, change keyboard/bookmark bindings, change picking or wheel semantics, or alter camera projection/sensitivity. Preserve36's world viewport conversions, off-center drag geometry, focus-plane behavior and current UI bounds. Integrate the small gesture stage after current UI ownership and before camera pose updates, fitting38's approved collection/presentation boundaries.

## Tests and native acceptance

After approval and source-freeze release, schedule the real camera system with required input ownership/window/projection resources and native-style ordered events. Verify:

- A valid world press/move performs the existing exact grab, and release stops it.
- Press over UI then move into world does not acquire a gesture.
- Middle remains pressed in the engine resource through focus loss/restore; delivered cursor movement afterward leaves the camera unchanged until a fresh accepted press.
- Focus loss and restoration in one frame still cancel the old gesture; events for another window cannot arm or release this window's gesture.
- A fresh primary-window press after cancellation works even with the engine's stale pressed bit, while the first anchor sample causes no jump.
- Viewport/scale changes, invalid rays and owning-window removal cancel cleanly with ordinary shutdown; no deferred old motion reappears.

Keep direct scheduled event tests labeled integration tests, not native end-to-end proof. Add passive primary-window raw middle press/release counters, last event window/state and accepted/cancelled gesture state to the existing probe so delivery and policy decisions are distinguishable. This must observe the normal path without injecting input or changing ownership. Repeat the actual exact-PID native sequence during an exclusive window interval: establish delivered press/drag, focus loss, restore, delivered post-restore pointer movement, then delivered fresh press/move/release. Assert actual camera stationarity before the fresh press and expected movement afterward. If host automation again fails to deliver release/motion, retain the failed evidence and report the native limit; do not pass from an unchanged pose alone. Keep the camera within the visible Lab fixture for useful screenshots. Decode and inspect before/after images and close through verified WM_DELETE with clean renderer shutdown.

Run affected camera/input/real schedule regressions and the required coordinated workspace tests, Clippy and formatting gate. No editor changes. No media/visual-quality acceptance follows merely from correct gesture cancellation.

## Approved ordered-event and coordinate clarification

The locked versions are Bevy Input/Window/Winit0.15.3. These checked local primary sources establish the exact units and forwarding contract:

- `bevy_window::CursorMoved.position` is window-relative **logical window pixels**, and `delta` is the change in those logical coordinates. Winit's handler receives physical position, divides by `WindowResolution::scale_factor()` at the time the event is handled, and updates its cached physical cursor. Do not divide the resulting event position again, and do not confuse it with egui points or viewport-local coordinates. Existing `ViewportCoordinates.logical_rect` is in the matching logical window space; use its explicit origin removal before camera rays.
- `MouseButtonInput` identifies button, state and window but contains no position. The latest preceding delivered CursorMoved for that window supplies a press position; `Window::cursor_position()` at the end of the frame may already describe a later move and cannot reconstruct the press location. CursorEntered has no position and cannot establish an anchor by itself. CursorLeft cancels and invalidates retained pointer position.
- `WindowResized` contains logical width/height. `WindowScaleFactorChanged` identifies effective window scale; `WindowBackendScaleFactorChanged` identifies OS-reported scale, which may differ under a forced override. Neither should be silently treated as an egui zoom event. Either relevant native geometry event invalidates an ongoing gesture conservatively, even if effective final dimensions happen to equal the prior frame.
- `bevy_winit::WinitAppRunnerState::forward_bevy_events` iterates its buffered events in order, emits each typed event, then sends that original ordered batch into `Events<bevy_window::WindowEvent>`. Separate typed readers do not establish cross-kind ordering. The gesture's aggregate reader preserves focus, cursor, button and geometry order; the probe can independently read both aggregate and typed streams without draining another consumer.

Process **every** delivered primary-window cursor position against the active viewport, not only the final point. An active gesture that moves outside the viewport and back inside during one frame is canceled at the first outside sample. A later inside sample cannot resurrect it; only a later fresh accepted press can establish a new gesture. Current egui ownership is a conservative additional admission gate, not permission to replace press-time geometry with the final cursor. UI-owned or unknown press positions are denied.

For focus loss and restoration in one batch, the loss cancels first even if the final Window.focused flag is true. A new explicit press later in a valid focused/owned context is a distinct gesture; merely retained pressed state is not. Foreign-window events never grant ownership. Window destruction, cursor departure and relevant geometry events invalidate the appropriate retained position/anchor as well as the active flag.

A resize/scale event within the batch means earlier cursor coordinates and the camera's final computed viewport may belong to different geometry epochs. Do not guess intermediate camera projection. Conservatively cancel and disallow drag admission/motion for that geometry-transition frame, consume the events, then require a fresh valid press after the current projection/viewport is established. This also cancels resize-A→resize-B→resize-A or scale changes that end at the original rectangle. Engine/app viewport changes without native events are caught by actual viewport/scale comparison; there is no claim to observe an unreported transient mutation. The current UI has one viewport writer per update; if implementation introduces another writer, it must publish an invalidation generation rather than relying on final-value equality.

Add deterministic integration cases for inside→outside→inside with one frame's cursor events, an outside press followed by an inside move, focus loss/restore in one batch, and resize/scale round trips whose final rectangle equals the initial rectangle. Tests must use actual event logical units at both scale1 and nonunit scale. Existing off-center focus-plane drag tests remain meaningful and unchanged in geometry.

## Concrete passive native telemetry

Extend the existing reusable `Evidence` recorder with a fixed-size middle-button delivery record; do not create a per-frame event Vec or format unbounded event histories. Expose these numeric CSV fields alongside the existing global `middle` held bit, camera target and cursor count:

- Raw typed middle press/release totals for the primary window, foreign-window middle totals separately, and last middle event state/window entity bits. These count actual `MouseButtonInput` delivery regardless of whether the camera accepted a gesture.
- Monotonic aggregate WindowEvent sequence plus last primary middle, focus-loss, focus-gain, cursor and geometry-event sequence numbers. Counters use checked increments and the existing explicit recorder-failure path on overflow.
- Last delivered primary logical cursor position, primary cursor-leave count, current effective window scale and the existing actual viewport dimensions. Do not substitute queried desktop cursor coordinates for app event delivery.
- Read-only active gesture/window, accepted-press count and cancellation count with a small typed last-cancellation reason (focus, ownership, viewport/scale, cursor departure, release, invalid ray, missing window). This is derived presentation diagnostics, never authoritative simulation state. Record after the ordinary camera system so counters and camera result describe the same completed frame.

A verified OS mouseup followed by unchanged raw release count means the application has not observed a release; it does not identify which external/backend layer lost it. If raw release count advances but global held remains1 after the normal input schedule, that is a distinct engine/input-state finding. If raw release advances and global held clears but the gesture remains active, it is an application-policy failure. After focus cancellation, delivered cursor events with no fresh accepted press must leave camera pose unchanged even if global held remains1; only this delivered-motion case proves canceled-gesture behavior. A fresh native press must rearm and move normally afterward. The original failed native01/02 evidence remains intact regardless of later repair success.

## Immediate verification and handoff inventory

47 finishes before38's schedule migration. Its bounded source ownership is the camera middle-gesture implementation, a focused gesture helper/state module if needed, passive probe fields and directly affected fixtures. Preserve wheel/keyboard/selection behavior. The real WindowPlugin supplies aggregate WindowEvent; minimal `input_policy_tests` fixtures, the direct round2 schedule fixture and `run_headless_check` must explicitly register required event/state resources. Do not make the production event reader optional to avoid a missing-resource test failure. Test ordered engine events through the normal camera system before rebuilding the copied native probe. After independent native review, freeze this interface and let38 rebind its scheduling without changing the accepted gesture semantics. Shared GPU suites/native capture intervals remain serialized by the orchestrator.


## Implementation review clarifications

Track both primary-window and camera entity identity. A replacement camera with the same window and viewport cancels the prior gesture; a final geometry comparison alone cannot establish continuity. A second independent aggregate-event reader pre-scans the current native batch for relevant geometry events, so even a press preceding a late resize is rejected and never increments accepted-press telemetry. The ordered reader still processes every focus/cursor/button event; both readers consume their complete batches without a temporary event collection. An absent camera also consumes events, preventing obsolete input from being replayed for its replacement.
