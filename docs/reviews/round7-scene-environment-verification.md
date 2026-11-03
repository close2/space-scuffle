# Round46 scene environment lifecycle verification

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Blueprint: `docs/plans/46-round6-scene-environment-lifecycle.md`. Scope is game scene globals and actual light hierarchy visibility; no editor, mist shader, style or authored media changes.

## Implemented behavior

`SceneEnvironmentPlugin` captures actual arena `ClearColor` and `AmbientLight` after Startup. Successfully committed battle roots carry their immutable theme environment. PostUpdate chooses that committed root, applies its environment and toggles the arena/battle hierarchy before inherited visibility propagation. Returning to either Lab tab restores the captured arena globals. A failed candidate does not replace the active root or its environment, and a successful replacement in the same tab applies the new committed environment. Owned terrain creates its own light hierarchy without replacing globals; standalone terrain setup retains its prior behavior.

No environment resources are marked changed during steady frames. Multiple committed battle roots are an explicit scene invariant error, not an arbitrary choice of whichever query entry appears first.

## Focused checks

Six environment unit regressions passed with process exit0 in `/tmp/space-scuffle-r7-environment-unit03.log`: distinct startup/battle ambient and clear values through repeated tab switches, actual inherited directional-light visibility, pending-root handling and committed replacement, unchanged resource change ticks, duplicate-root and lost-baseline failures, and real authored-theme owned/standalone lighting behavior. The first compile attempt caught a removed import still used by map-object query filters and explicit closure lifetime lints; both were repaired before successful verification, with the original log retained.

All10 runtime battle integration tests passed with process exit0 in `/tmp/space-scuffle-r7-environment-runtime.log`. The real production plugin fixture selected AMD Radeon890M/RADV Vulkan, verified both inherited and renderer-facing `ViewVisibility` on every scene transition, and closed normally after joining actual shader workers. The missing terrain bundle error is deliberately exercised by the failed-candidate regression, which verifies the old committed scene/globals remain intact before a subsequent valid replacement. This is not a production fallback. Existing headless-egui missing node warnings remain in this windowless fixture.

## Pending acceptance and limits

Strict client all-target Clippy including dependencies passed in `/tmp/space-scuffle-r7-environment-clippy03.log`; workspace format check passed in `/tmp/space-scuffle-r7-environment-fmt.log`. The actual production probe rebuilt successfully and is copied to `/tmp/space-scuffle-r7-native-input-probe`; a separate exact-owned native driver is prepared for matched Lab→Battle→Lab→Battle/reset→Lab captures. Native image review and the next coordinated full workspace gate are still required. Prior round6 workspace gates do not cover these changes.

The separately documented custom-mist lighting query still ignores scene visibility/render layers. This change establishes real environment and standard-renderer light ownership; it does not claim every custom shader now chooses the correct light. The Lab floor, legacy media and ribbon-like mist remain visually unaccepted. Functional lifecycle evidence must not be described as whole-scene artistic acceptance.

## Native matched scene evidence

`/tmp/space-scuffle-r7-environment-native01` completed the real AMD Radeon890M/RADV Vulkan production probe with ordinary verified WM_DELETE and exit0. Exact PID/window identity was verified before each OS mutation and screenshots were decoded before acceptance. Images02 (initial Lab),04 (battle),06 (returned Lab),07 (second battle/reset) and08 (second Lab return) cover the requested sequence. No alternate renderer, synthetic scene state or screenshot-only replacement was used.

Implementer opened and inspected02,06 and08 at their actual2880×1800 dimensions. All preserve camera target(0,0), distance9.5 and selected/subject/inspector identity; the original gray background returns both times. After visual inspection, physical rectangle(864,90)–(2592,540), an unobstructed background region, was compared across all three images. Every one of its777600 pixels is RGB(43,44,47), with maximum channel difference0. Full evidence is in `background-comparison.json`, the raw PNGs, corresponding telemetry JSON and trace. This compares the real rendered background, not reconstructed expected-color images.

UI remains contained and actors/portrait remain visible. Actor/portrait animation frames and mist geometry vary over real elapsed time, so full-image equality is neither claimed nor required for the global restoration defect. The partial Lab floor, opaque legacy creature media and rigid ribbon-like mist are still rejected. Standard renderer light visibility is additionally verified through the real production integration fixture; custom mist's separate visibility-filtering defect remains explicitly outside this patch.

The1385-byte native runtime log contains no error, VUID, panic or audio flood; shutdown reports zero remaining shader workers. The native interval was released immediately after normal close. Focused verification is ready for independent review; the next coordinated workspace gate remains required for this source state.

Root independently opened02/06/08 and recomputed camera, identity and the full background ROI in `root-independent-comparison.json`, confirming every one of the777600 pixels matches and both returns preserve the camera. Bounded46 environment restoration is accepted pending the coordinated full workspace gate. The hard flat mist-trail ending visible in06 remains an independently rejected mist artifact, not a reason to broaden this scene lifecycle patch. Product source remains frozen; plan38 waits for the gate release.
