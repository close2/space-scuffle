# Round 10: actual-view mist lighting

Stage2 implementation follows the approved plan44 amendment. The round9 full workspace gate passed before this work (tests649.72s, Clippy1.98s, formatting0.42s, no source changes). Stage2 bounded native verification is complete: actual fragment, retained-packet, invalid-input and production scene-return evidence passed. Source is frozen for the next integrated workspace gate; final integrated acceptance remains pending. Neither brighter ribbons nor a successful arithmetic oracle constitutes art acceptance; the volume and global frame-time goals remain open.

## Production contract

The shader consumes Bevy's actual prepared directional-light array, count and per-view skip flags. It accumulates linear RGB-times-lux divided by the authored immutable reference, multiplied by the positive world-normal/light-direction dot product. The existing0.38 base response remains explicitly artistic. There is no CPU strongest-light selection, per-frame light vector copied into retained packets, second physical camera exposure, or artistic clamp.

`reference_illuminance_lux` is authored as10000lux, with the same named serde default for earlier style files. The private pool owns this reference for its lifetime; no runtime reload API is introduced. The existing80-byte frame packet retains its layout and uses the old light lane for the immutable divisor and reserved zeros. Stage1 tone mapping and single premultiplication remain intact.

Admission validates every authored directional rig, including hidden rigs: finite nonnegative linear RGB/lux, actual f32 RGB×lux packing, actual f32 division and reciprocal multiplication, and a conservative per-channel envelope derived from the engine's `MAX_DIRECTIONAL_LIGHTS`. Failure is sticky, typed, and suppresses only mist extraction/upload/queueing, including retained packets; it does not deactivate unrelated cameras or claim arbitrary invalid scene inputs are safe. Already-submitted valid frames can finish.

## Verified so far

- Four CPU admission/reference tests pass: `/tmp/space-scuffle-r10-lighting-cpu01.log`.
- The real queue-alignment system rejects mist while admission is pending/failed and preserves unrelated transparency, including when no uploader exists: `/tmp/space-scuffle-r10-lighting-queue01.log`.
- Actual AMD/RADV compute oracle02 passes with no WARN/ERROR: `/tmp/space-scuffle-r10-lighting-amd02.log` and `-result.json`. This binds the engine's real `LightMeta` buffer, not synthetic light bytes, using distinct dynamic offsets0/3904 in a7808-byte buffer. It validates linear color/lux response, world-space parent rotation, per-view layer skips, zero eligible lights, and actual parent-hidden/restored counts2/1→1/0→2/1.
- The first AMD oracle run exited0 but emitted BevyB0004 because its parent lacked inherited visibility. That run was rejected for clean hierarchy verification. The corrected fixture creates the ordinary visibility hierarchy and adds parent hide/restore assertions; the original log is retained.

The preserved oracle02 binary is `tmp/verification/mist-lighting-r10-artifacts/client-lighting-tests02`, SHA256 `bd68eb243226a333cd0ddc26692b2866af83746ac8a5fdd93091aac83cc65282`. Its shipped helper input remained SHA256 `8e366eedc3156058e5ba20ea5f4b14d7b6aa864df7debb9bb33acbc02f0ff90b` during the actual-device run.

## Actual capture scope

The opt-in Close fixture inventories and hides every pre-existing directional rig, then runs hidden/restored, parented, two-view/layered, zero/restored and genuine held-callback conditions. It freezes two seconds of mature history, media and presentation time. Three screenshot acknowledgments gate progression; each checkpoint requires actual prepared-view GPU readback, live frustum checks and queued mist evidence. The two-view fixture fits both actual640×720 viewports around the complete actor/wake bounds.

The callback gate withholds readiness delivery only after successful real GPU maps. It neither skips mapping nor synthesizes completion. Production staging capacity and the authored two-second timeout remain unchanged. Diagnostic packet storage is preallocated, and full packed bytes plus real target/staging identities are preserved per checkpoint. Byte equality is checked directly; external artifact auditing can additionally compute cryptographic hashes of these files. New view light data must affect the retained packet before readiness is released.

The first fragment-capture binary is `tmp/verification/mist-lighting-r10-artifacts/mist_preview-view-lights01`, SHA256 `efcf512d3157ef53e97dc24d1f6fd67f19fcf36170984e1bdbebbecd45607d81`. The authored style input is SHA256 `5b56a55dac2545d3f47dc13a739238717ea3567a6555c5b4d139717d82f77b4e`. Native hidden01 completed on actual AMD with exit0, a clean log and three decoded1280×720 PNGs. Parent review found images0/2 byte-identical, actual counts1/1→2/2→1/1 and world-Y RGB1,1,1→3,1,1→1,1,1, with one queued wake per view. The middle condition also lights the real terrain, so this is bounded fragment integration evidence, not isolated per-pixel mist radiance. All five positive conditions now passed on actual AMD with clean exits and unchanged asset hashes; weak mist art remains rejected. This binary also includes the concurrently implemented round40 authored player-palette admission; no historical whole-image equivalence claim is made across that ownership change.

## Remaining acceptance work

1. Positive material/fragment captures and independent image review are complete for all five sequences (details below).
2. Actual invalid-light-during-held-packet failure evidence is complete (details below).
3. Real production Lab→battle→Lab light-buffer and image evidence is complete (scene03 below).
4. Strict focused checks followed by the coordinated full workspace test/Clippy/format gate.
5. Separate review of volume implementation budgets and adversarial motion reduction; Stage2 lighting does not authorize or accept volume art.

## Five positive actual-device sequences

Root independently inspected all15 decoded1280×720 screenshots and recomputed the recorded RGB/direction/skip responses from actual prepared light data. Evidence directories are `/tmp/space-scuffle-r10-view-lights-{hidden,parented,layered,zero,stalled}01`, each with `root-independent-audit.json`; the independent orchestration report is `docs/reviews/round10-view-light-verification.md`. Hidden, parented, layered and zero sequences return to pixel-exact original images. Layered uses real dynamic offsets0/3904 with one eligible rig per view, exchanging white/red illumination across the two actual viewports. All819280-byte selected packets remain byte-identical within each sequence.

The stalled run exited0 in21.56s: initial white checkpoint sequence135/held0, changed red illumination at sequence140/held3, then released red checkpoint sequence150/held0. Images1/2 are pixel-exact despite genuinely held and subsequently released callbacks. Packet hashes, persistent target IDs and staging IDs remain identical. This establishes current-view lighting over retained geometry without forcing callback success.

The implementation agent also opened the actual layered and stalled middle screenshots. Both actual views and actors are visible, with the intended red/white distinction. The wake remains a faint flat strip and lacks convincing volume; this is rejected art, even though the bounded lighting contract is behaving correctly. Terrain lighting also changes, so neither visual comparison is presented as isolated mist radiometry.

Negative-fixture CPU tests now reject changed original errors, modified retained bytes, new submission/allocation counters, missing actual shell identity evidence, and invalid CLI combinations. These are evidence-guard tests, not substitutes for the pending native invalid-input exit.

## Actual retained-packet invalid-input result

Root executed preserved negative01 binary SHA256 `6966c77c2cf7cf95f29dedc6c36307242bf0823f02dec66b981c5e2f9de43d26` on actual AMD. It exited1 in19.05s with exactly one valid pre-failure screenshot, which root opened. The original typed invalid-lux diagnostic for entity539v1 was retained by main-world admission and the uploader. The actual post-queue failure artifact records one view, zero mist items and the real shell entity4294967832 still present. It is not inferred from an unrelated draw count.

The independently compared cumulative before/after accounting is identical:140 submitted packets/114699200 bytes, three targets/819280 bytes, three staging buffers/2457840 bytes and two staging stalls. All three genuine callbacks remain held; selected and publication sequences remain140. Both819280-byte packet files are identical, SHA256 beginning `1e922f` and ending `75e24`. All runtime asset hashes stayed unchanged. Full exact hashes and checks are in `/tmp/space-scuffle-r10-view-lights-invalid01/root-independent-audit.json`; the original error exit is an expected negative-test result, not a renderer validation failure.

The passive production scene observer is implemented next, using only actual native tab input and real prepared-light readback. Its three focused CPU tests cover lazy battle creation/identity retention, numeric eligibility guards and rejecting incomplete normal closes. These tests do not replace the pending real Lab→battle→Lab run.

## Scene-return startup failure and bounded correction

The first real production observer run exited1 in12.56s before accepting any checkpoint. Its immediate initial Lab request exposed a diagnostic cache-publication ordering bug: the probe queued pipeline16 in PrepareBindGroups and immediately indexed the old16-entry published cache. Mature Close fixtures leave the request zero during warmup, so their successful results did not cover this early path. The native failure is retained at `/tmp/space-scuffle-r10-scene-lights01/client.log`; no screenshot or scene-return success is claimed from it.

The reviewed correction reads the actual published cache entry through the locked public checked iterator. An unpublished ID stays pending under the original180-second deadline. Queued/creating and engine-retryable shader/import states also wait; genuine compiler failures remain typed errors, and an unexpected successful render-pipeline kind is rejected. It never forces queue processing, invents a pipeline or bypasses the real shader. Two focused guard tests pass; strict all-target client Clippy passes at `/tmp/space-scuffle-r10-light-scene-clippy06.log`.

Preserved retry binary `tmp/verification/mist-lighting-r10-artifacts/native-input-view-lights02` has SHA256 `6744ffc0aa7ce929882db74c13c71f2b2d24240c36eafa354ae31bdc00bc750a`. Adjacent provenance records no source changes during build and only `mist/light_probe.rs` changed among owned inputs from the failed01 binary. Shader/style inputs are unchanged. Actual retry remains required; CPU state tests are not GPU startup evidence.

The first retry with probe02 successfully initialized the actual renderer and wrote the initial Lab GPU sidecar without shader/pipeline diagnostics. Root's native pointer acquisition then failed (Xwayland delivered an off-window cursor), so normal close correctly exited1 because the remaining two epochs were incomplete. This is partial startup evidence and a retained driver failure, not accepted scene-return coverage. Root is retrying the same preserved binary using the previously verified exact-PID fullscreen input path; no product correction or shader change follows this driver failure.

## Completed production scene return

Same preserved probe02 completed the actual Lab→battle→Lab sequence in scene03 on AMD Vulkan: exit0 in17.324s, clean renderer log, unchanged assets, three2880×1800 active-window Spectacle frames with PID/focus verified. Root and the implementation agent opened all three actual images. Initial arena rig4294967834 is retained; selecting Battle lazily creates child light4294967877 under committed root4294967859. Returned Lab retains both identities while the battle hierarchy is hidden.

Actual prepared GPU counts are1/1 in each epoch, restored Lab readings are byte-exact, and root's independent recomputation finds maximum axis-response error4e-8. Artifacts and full audit: `/tmp/space-scuffle-r10-scene-lights03/root-independent-audit.json`. Queued mist is honestly zero at each acknowledged GPU epoch. The later screenshots contain evolving histories, so this test establishes actual scene ownership and prepared view illumination; it does not claim frozen-history image equivalence or replace the positive moving-fragment matrix.

The images confirm the correct scene switch and return. They also retain rejected art: the Lab wake is a broad flat rail, and the returning short history has a conspicuous straight cut at its end. Neither scene correctness nor stronger illumination closes the volume/readability goal. The next proposed CPU reducer admission is documented in plan44; no volume implementation has begun.

The latest focused tests, strict all-target client Clippy and exact workspace formatting check pass.44 product sources are frozen pending the coordinated integrated gate after the other lanes' native verification.
