# Round 5 — Explicit game audio policy

Approved plan 42 replaces non-editor graphical default-plugin construction with `GamePresentationPlugins`. This retains the real renderer and disables `AudioPlugin` before installation, so the currently unimplemented playback subsystem does not open a device. An explicit `AudioPolicy::DisabledNoPlaybackSubsystem` resource is logged once and serialized into production capture manifests and standalone mist metrics. The editor and Cargo features remain unchanged.

The actual production fixture uses the shared group and asserts that `AudioPlugin` is absent and the policy resource exists. The focused declaration/artifact test also checks the serialized policy and absence of audio source assets. No dummy device, fallback sound, stderr filtering or hardware-readiness claim is introduced.

Verification so far:

- `/tmp/space-scuffle-r5-audio-policy-tests.log`: focused policy test passes.
- `/tmp/space-scuffle-r5-audio-policy-production-test.log`: actual production GPU fixture passes in 36.93 seconds using llvmpipe in the sandbox. No audio callback/device messages; unrelated udev/egui warnings are retained. This is not native-driver-clean evidence.
- `/tmp/space-scuffle-r5-audio-policy-build.log`: actual `battle_capture` and `mist_preview` binaries build successfully.
- The initial strict client Clippy attempt reported two concurrent plan 36 findings, which their owner repaired. Fresh strict all-target client Clippy and formatting now pass; latest logs `/tmp/space-scuffle-r6-mist-provenance-clippy.log` and `/tmp/space-scuffle-r6-mist-provenance-fmt.log`. The first coordinated full workspace test gate still failed a separate plan 36 direct-schedule fixture and is being repeated after its repair.

Native verification is complete. `/tmp/space-scuffle-r5-audio-policy-mist/` contains all three acknowledged close captures and the explicit policy in `metrics.txt`; the unfiltered log is 1,658 bytes with no audio callback, validation or panic diagnostics. Root inspected the actual middle image and measured a mean absolute pixel difference of 0.00000615/255 from the previous optical baseline. This verifies preservation of that rendering, whose mist art remains rejected.

The first production attempt stopped at ramp entry because concurrent plan 36 had an incorrect viewport-coordinate conversion. After its repair, `/tmp/space-scuffle-r6-viewport-battle/manifest.ron` records `complete: true`, no failure, seven checkpoints, and `audio_policy: DisabledNoPlaybackSubsystem`; native process exit was zero. Its unfiltered 2,413-byte log contains one policy statement and no ALSA/POLLERR, validation or panic diagnostics. Root independently compared the simulation fields with run06 and found them equal. The build also includes plan 36 viewport/input changes and plan 39 exact media admission: visible unit counts changed with the corrected viewport, so this is not a claim of production pixel identity with plan 37. Actual combat and Lab images were inspected; the broader visual-quality work remains open.

These native results satisfy the bounded audio-policy goal and remove the unused-device callback flood from benchmark prerequisites. The coordinated full workspace gate remains pending; focused native success does not substitute for it.
