# Round47 camera gesture verification

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Bounded source implementation passed orchestrator review. Native verification includes five initial checks and a successful three-check ordinary-focus sequence, with two failed attempts preserved. Post-minimize requested pointer motion never arrives; that remains a platform/delivery limitation, not a passed stationarity test. The next combined workspace gate is still required; the successful round7 gate preceded this change.

## Implementation and independent-review corrections

The production camera owns one gesture only after an accepted raw primary-window middle press at its preceding delivered cursor position. An ordered `WindowEvent` reader handles focus, cursor and button transitions; a second reader rejects admission for an entire resize/scale batch before evaluating it. This prevents both outside→inside crossings and late geometry events from silently rearming the camera. Current UI ownership remains a conservative admission gate. The global held bit does not grant ownership.

Both camera and window entity identities are tracked. Replacing a camera at the same viewport cancels the gesture. Missing owners consume their event batches, invalid rays cancel, and finite fixed-size diagnostics use checked counters. The camera's existing focus-plane ray intersections and window/viewport conversion remain the geometry authority. The native probe separately records typed middle events, ordered focus/cursor/geometry sequence numbers, delivered cursor positions and gesture state, without injecting application input.

Direct schedule fixtures explicitly register the required gesture resource and aggregate window events. Production readers remain required rather than using optional fallbacks.

## Focused verification

- Twelve gesture state regressions pass: `/tmp/space-scuffle-r8-drag-unit03.log`. They cover transient focus loss/restore, geometry round trips, late-resize press denial, camera replacement, outside crossings, foreign windows, missing owners, invalid rays and counter overflow.
- Two native recorder unit regressions pass: `/tmp/space-scuffle-r8-drag-telemetry.log`. Raw delivery counters remain distinct from gesture ownership, with checked overflow.
- Three scheduled production-camera integration tests and six existing input-policy tests pass: `/tmp/space-scuffle-r8-drag-integration02.log`. The real engine projection system verifies actual off-center world-point projection after dragging at scales1/1.5 and distances9.5/18.4. Other cases retain the global middle bit through focus cancellation, deliver subsequent cursor motion, require a fresh raw press, and replace the camera at the same viewport. These are scheduled integration tests, not native OS evidence.
- Five headless and eighteen round2 visual/schedule tests pass: `/tmp/space-scuffle-r8-drag-fixtures01.log`. The existing round2 subprocess deliberately selects llvmpipe, so its startup result is not represented as native hardware verification.
- Initial integration build failed because Bevy's `ViewportConversionError` does not implement `std::error::Error`; the fixture now translates it explicitly. The failed log is retained as `/tmp/space-scuffle-r8-drag-integration01.log`.
- Strict all-target client Clippy01 identified test expectation/semicolon findings in this lane and in-progress35 findings. The former were fixed; Clippy02 reported only the then-current35 upload/mod findings. Formatting01 likewise reported only35's in-progress packet tests. These attempts are retained and do not count as passing integrated gates. After the concurrent findings were repaired, strict client all-target Clippy03 passed (`/tmp/space-scuffle-r8-drag-clippy03.log`) and workspace formatting02 passed (`/tmp/space-scuffle-r8-drag-fmt02.log`).

## Real native evidence

Copied executable: `/tmp/space-scuffle-r8-native-input-probe`, with SHA256 file beside it. Driver: `/tmp/space-scuffle-r8-drag-native.py`. Evidence: `/tmp/space-scuffle-r8-drag-native01`; trace, commands, screenshots and checks are retained. Every native mutation verifies exact process/window identity; input additionally verifies focus. Screenshots have a bounded capture timeout and decoded-image validation.

The actual adapter was AMD Radeon890M/RADV Vulkan. The window used the existing native Xwayland path and an explicitly overridden logical scale1; the driver's measured pointer transform is recorded separately from the host-reported scale1.59375. This is not evidence for every backend or scale.

Five native checks passed:

1. A delivered fresh middle press acquired a gesture; delivered motion moved the target to approximately(-0.10544,-0.08879).
2. A delivered raw release ended the gesture; subsequent delivered motion did not move it.
3. A press over the inspector followed by delivered world entry did not acquire a gesture.
4. Leaving the world viewport cancelled the gesture; delivered reentry did not revive it.
5. Minimizing while middle was held produced focus loss and cancelled gesture ownership; the unfocused camera remained stationary.

After restoration, the camera remained at approximately(-0.18591,-0.14808), global middle remained1, and gesture active was0 with cancellation reason Focus. Requested subsequent native pointer movement produced **no new delivered cursor event**: sequence50 remained unchanged. The driver timed out and exited1. Thus post-restore delivered-motion stationarity and fresh-press recovery are not claimed as native passes. The equivalent scheduled-event cases pass, but cannot replace missing OS evidence. Cleanup sent the verified native close; the application returned0 with pipeline workers0. The runtime log has no shader/validation/audio/panic error. Failed native evidence from earlier round7 remains intact too.

Screenshot03 was inspected: all four actors, the selected actor/HUD and inspector remain visible after a bounded drag. The existing rigid mist ribbons, static creature media and positive-quadrant floor remain rejected visual quality; this input repair does not accept them. Screenshot04 was also inspected: all four actors and the bounded UI remain visible at the recorded cancelled pose. It records the cancelled restored state, not a successful delivered-motion recovery.

## Handoff

47 owns `camera_drag`, the camera system's narrow adapter, probe telemetry and directly affected fixtures. Keep those semantics when38 migrates scheduling. No editor code changed.38 must retain current ownership sampling before gesture consumption and presentation/projection boundaries; held-state polling must not be reintroduced as gesture admission. Full workspace tests, strict Clippy and formatting remain coordinated with concurrent35/45 work.


## Ordinary focus follow-up: native02/03

The orchestrator requested a dedicated process-owned Xlib helper to distinguish unmapping from ordinary focus changes. The bounded helper publishes its actual PID, supports WM_DELETE, and self-terminates after120seconds. The driver verifies both PIDs before focus mutations and records `xwininfo` confirming the game remains IsViewable. No other application is used as a focus target.

`/tmp/space-scuffle-r8-drag-native02` passes mapped focus cancellation and subsequent delivered-motion stationarity, then fails a **driver assumption**: a raw release had already arrived during focus restoration (`middle_up`1→2, global middle0), but the driver demanded another release after sending a redundant mouseup. This is not evidence of missing release or a product failure. The run, failure and normal cleanup0 are retained.

`/tmp/space-scuffle-r8-drag-native03` corrects only that observation rule: release OS state, recognize an already-observed release, then independently require a fresh raw press and actual motion. All three checks pass and both owned windows close normally, application exit0, renderer workers0, clean AMD RADV Vulkan log:

- Mapped focus loss cancels the active gesture while global middle remains1; camera target stays(-0.10544363,-0.08879089).
- After focus return, delivered cursor sequence26→29 moves logical cursor(1640,795)→(1665,809) while target remains exactly unchanged and gesture remains inactive.
- A new raw middle press increments down1→2 and accepted presses1→2. Delivered cursor sequence29→32 then moves target to(-0.17941427,-0.14695978), proving fresh native recovery.

The second check occurs **after a raw release was already observed and global middle was0**. It must not be described as native motion with a retained global middle bit. That narrower retained-bit continuation case remains covered by scheduled integration, not native delivery. Native01's minimize-related lack of pointer delivery remains independently recorded.

Screenshot05 from native03 was decoded and inspected: all actors and the bounded inspector/HUD remain visible after fresh recovery. Existing media/mist/floor quality remains rejected. No product workaround was introduced for unavailable native events. The native interval was released to the orchestrator before38 implementation begins.


The orchestrator independently repeated the owned-helper recovery sequence at explicit window scale1.5 in `/tmp/space-scuffle-r8-drag-native-scale15`, using `/tmp/space-scuffle-r8-drag-focus-scale15.py`. All three checks passed and both windows closed normally0 on AMD Vulkan with clean logs. Actual dimensions were1920×1200logical and2880×1800physical; all five screenshots decoded, and root inspected05 with actors/UI intact. Delivered return motion sequence24→27 preserved target; new raw press down1→2 followed by cursor sequence27→30 moved target(-0.162673,-0.140668)→(-0.283862,-0.233857). Release had already arrived before the return-motion check (middle0), preserving the same retained-bit native limitation.
