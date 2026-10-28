# Round 5 — Explicit game audio initialization policy

Status: approved by the orchestrator. Complete before plan 35 native performance measurements.

## Problem and actual source evidence

The successful plan 37 production capture produced 23,560,046 ALSA `POLLERR` callback messages and approximately 2.52 GB of stderr. Root preserved the complete compressed stream at `/tmp/space-scuffle-r4-combat-shadow-after37.log.gz` after verifying decompression length and CRC. No Vulkan validation messages or panic were found; that does not make the audio failure acceptable or the measurement environment clean.

The workspace enables Bevy 0.15.3 default features, including `bevy_audio`. Locked `bevy_audio/src/lib.rs` initializes `AudioOutput` during `AudioPlugin::build`, even without any playback entities. `audio_output.rs:32` calls `rodio::OutputStream::try_default()` and deliberately forgets the stream on success. Rodio 0.19.0 `stream.rs:211` installs an error callback that prints every device error directly to stderr. Thus the unused output device is opened at plugin initialization; filtering the Bevy logger cannot fix its lifecycle.

Repository inspection finds no game `AudioPlayer`, `AudioSource`, audio loading, authored sound files or playback subsystem. This is an intentionally silent current game, not implemented audio that has failed and may be quietly replaced. Global Bevy feature changes would also change the map editor and trigger a broader dependency rebuild, so they are outside this fix.

## Goal and bounded implementation

Prevent non-editor game and verification applications from opening an audio output device until an actual audio feature is designed and implemented. Make that choice explicit in source, startup evidence and capture artifacts. Preserve all rendering plugins, real media, window lifecycle, diagnostics and simulation behavior.

Add `client/src/presentation.rs` with a documented `GamePresentationPlugins` plugin group. Its implementation builds the locked `DefaultPlugins` group and explicitly disables `bevy::audio::AudioPlugin` before any plugins are installed. A small policy plugin records the fixed current policy as a resource and emits one startup statement explaining that audio is intentionally disabled because no playback subsystem is implemented. The policy is named and serializable for artifacts; do not invent an available-but-muted device, dummy sink, success-returning audio API or fallback stream.

Use that group at exactly these non-editor graphical entry points:

- The shared production application builder in `client/src/lib.rs`, which also supplies `battle_capture`.
- `client/src/mist/capture.rs`, which supplies `mist_preview` and the upcoming benchmark.
- `client/src/bin/terrain_preview.rs`, replacing only the default plugin-group expression and preserving its existing rendering configuration.

Do not edit `map_editor.rs`, editor modules, global Cargo features, Bevy/rodio/CPAL versions, ALSA configuration or unrelated machine processes. Do not suppress stderr, redirect errors to a null sink, disable all logging or replace real rendering. Keep the existing `run_render_app` teardown wrapper.

Add the explicit audio policy to the production capture manifest and standalone mist metrics. This records the intended verification scope: graphics and simulation are exercised, audio playback is not implemented or tested. Coordinate the narrow runtime-capture artifact field with its owner; preserve its completion and media-admission checks. No capture may report audio-device readiness merely because no device was requested.

## Future audio boundary

Adding audio will require a separate approved design, real authored sound assets and capability-driven playback. That design must own the output stream lifetime, propagate typed initialization/device-loss/playback errors and define an explicit user-approved mute policy if desired. It cannot restore unconditional `DefaultPlugins` audio initialization or treat missing/broken hardware as successful playback. This round deliberately provides no placeholder playback API and no synthetic audio device.

## Verification round

Add a focused policy test and a narrow assertion in the existing real production application integration fixture that `AudioPlugin` is not installed and the declared policy resource exists. The policy test can exercise the policy declaration without GPU initialization; the actual production fixture/native runs prove integration. Do not substitute that small test for the real app path. Ensure that all non-editor default-plugin entry points route through the shared group, while the editor source remains untouched.

Rebuild actual `battle_capture` and `mist_preview`. Run the default production seven-checkpoint script and one actual close mist capture on the same real renderer, with complete unfiltered logs. Require all artifacts and callback acknowledgments, correct audio policy metadata, clean shader/renderer validation, no audio callback/device-open messages, and clean process termination. Compare the combat checkpoint manifest/camera/tick against plan 37 and inspect the actual screenshot to ensure the plugin change did not alter rendering. Record log bytes and actual elapsed runtime; do not call this an audio-hardware test.

Run focused tests, strict client Clippy and exact formatting, then the coordinated workspace test/Clippy/format gates. Plan 41's source optimization and focused pathfinding verification are accepted; the combined workspace gate still requires its full rerun. Only after the unused audio thread is absent from actual runs may plan 35 establish its baseline; retain identical audio policy for baseline and replacement measurements.
