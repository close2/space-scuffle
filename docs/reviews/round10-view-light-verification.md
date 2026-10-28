# Round 10 — Actual view-bound mist lighting verification

Goal: bind mist shading to the renderer's actual directional lights for each view, preserving visible color, illuminance, world direction, visibility/layer eligibility and retained geometry. This is a correctness prerequisite for the separately planned volume; it is not art acceptance.

## Independent GPU oracle

Root executed preserved `client-lighting-tests02`, SHA256 `bd68eb243226a333cd0ddc26692b2866af83746ac8a5fdd93091aac83cc65282`, using AMD Radeon890M/RADV Mesa26.2.4 Vulkan. The actual Bevy prepared LightMeta uniform buffer is bound with real ViewLightsUniformOffset values0/3904, exactly once; the compute shader calls the lighting helper read from the shipped shader. Root inspected the oracle code as well as running it. Test exit0, no warnings/errors, normal renderer shutdown; 1.669 seconds includes startup and is not a performance benchmark.

White10000lux and red20000lux produce the expected independent directional RGB responses at the10000lux reference. Nonparallel parent rotation, inherited hidden/restored parent visibility, per-view layer exclusion, two-light accumulation and zero-light cases pass. First attempt01 exited0 but emitted B0004 from an incomplete parent visibility fixture. It remains rejected as clean verification; attempt02 uses ordinary required parent components and explicitly tests parent-hidden/restored counts2/1→1/0→2/1.

Logs/results: `/tmp/space-scuffle-r10-lighting-amd{01,02}.log` and corresponding `-result.json` files. Shipped shader SHA256 remained `8e366eedc3156058e5ba20ea5f4b14d7b6aa864df7debb9bb33acbc02f0ff90b`.

## Actual fragment capture matrix

Preserved binary `tmp/verification/mist-lighting-r10-artifacts/mist_preview-view-lights01`, SHA256 `efcf512d3157ef53e97dc24d1f6fd67f19fcf36170984e1bdbebbecd45607d81`. Root runs each declared Close fixture with60Hz virtual cadence, MSAA4,1280×720 on actual AMD Vulkan. All asset files are hashed before/after each process. Virtual cadence is not measured frame performance. Three actual screenshot completion callbacks gate condition changes after the history freezes at frame120/pool1.9999988s.

| Mode | Native process | Root image inspection | Independent input/packet result |
| --- | --- | --- | --- |
| Hidden | exit0,28.14s,clean | All3 viewed; red rig changes the scene; restored image is exact | Count1/1→2/2→1/1; Y response(1,1,1)→(3,1,1)→(1,1,1) |
| Parented | exit0,19.50s,clean | All3 viewed; restored image is exact | World-X response1/√14→3/√14→1/√14, correct corresponding directions |
| Layered | exit0,19.15s,clean | All3 viewed; two simultaneous halves exchange white/red; both actors/wakes visible; restoration exact | Offsets0/3904, total2/eligible1 each, colors exchange |
| Zero | exit0,16.74s,clean | All3 viewed; direct lights disappear and return; restoration exact | Count1/1→0/0→1/1, all direct RGB axes zero in middle |
| Stalled | exit0,21.56s,clean | All3 viewed; red light affects retained geometry; held/released red images exact | Three actual completed callbacks held; selected sequence140, then150 after release, same complete packet bytes |

Every completed row has unchanged819280-byte packet contents and target/staging buffer identities across its three conditions. Root independently recomputes axis responses from the actual prepared packed RGB-times-lux, world directions and skip flags; it also rehashes packets and decodes all PNGs. This checks the recorded actual data without relying only on the fixture's pass result. Native lighting also changes ordinary terrain; these captures do not isolate per-pixel mist radiance from background lighting.

Artifacts: `/tmp/space-scuffle-r10-view-lights-{hidden,parented,layered,zero,stalled}01/`, sibling `.log`/`-result.json`, and each directory's `root-independent-audit.json`. Driver `/tmp/space-scuffle-r10-light-matrix01.py`; independent audit `/tmp/space-scuffle-r10-audit-view-lights.py`.

The stalled fixture preserves the production uploader and its real map callbacks, withholding only delivery of successful readiness. Root inspected this controller. After all three staging maps are held, it records the retained selected sequence before changing the view light and requires that same sequence at the acknowledged red-light checkpoint. The final checkpoint releases actual completed callbacks and requires a newer packet. Artifact checkpoint sequences are135/140/150 with held counts0/3/0; all three packets hash `1e922f03e0a5aafe5c024bb349887220d3b870749be95668f9297014a8f75e24`. Images1/2 are pixel-exact. These packet files are exact CPU-packed selected publication bytes, not newly claimed GPU buffer readbacks. The separate actual per-view compute response is GPU-readback evidence.

## Invalid light while actual callbacks are held

Root executed preserved `mist_preview-view-lights-negative01` (SHA256 `6966c77c2cf7cf95f29dedc6c36307242bf0823f02dec66b981c5e2f9de43d26`) on the same actual AMD adapter. The process exits1 in19.05s after exactly one valid pre-failure PNG. The original invalid-lux error identifies light539v1 /4294967835 in both main admission and uploader diagnostics; no capture-success conversion occurs. These are explicitly expected errors, not a clean-success run.

The real failed-frame observer reports one view, zero queued mist and actual shell entity4294967832 still queued. Three genuine completed callbacks remain held. Before/after counters are exactly equal:140 submitted packets/114699200bytes, three target buffers819280bytes, three staging buffers2457840bytes and two already-recorded staging stalls. Selected/publication sequence remains140. Both pre-injection and retained819280-byte packet files are byte-identical with SHA256 `1e922f03e0a5aafe5c024bb349887220d3b870749be95668f9297014a8f75e24`.

Root independently decoded/inspected the single pre-failure screenshot, compared packet bytes and every cumulative counter, checked exact error/entity/exit and all unchanged asset hashes. Evidence: `/tmp/space-scuffle-r10-view-lights-invalid01/` plus sibling log/result and `root-independent-audit.json`. No post-failure screenshot is represented as accepted. This closes the bounded admission/extraction/queue rejection contract, not general renderer recovery or art quality.

## Production scene transition attempt 01 — rejected

The first actual native Lab observer run exited1 before its first checkpoint. Its immediate request exposed an observer startup ordering bug: the diagnostic compute pipeline was queued in PrepareBindGroups but the directly indexing state getter ran before Bevy promoted that queued ID into the published cache. The log records index16 against length16. This is a diagnostic failure, not a successful scene transition or an accepted shader result.

Evidence is preserved in `/tmp/space-scuffle-r10-scene-lights01/`: native binary SHA256 `7f99250c0bab19dd14fdc2cb5d61c70bb839d43e4b529223f0b42af3b6931fbe`, actual AMD Vulkan,12.56s, unchanged runtime assets, no accepted image/checkpoint. Root reviewed the pinned Bevy implementation and approved the plan amendment for a checked published-entry lookup. Pending publication remains bounded by the existing180-second deadline; actual compiler errors remain fatal. A corrected real native retry is required.

## Production scene transition attempt 03 — bounded pass

The corrected observer waits for actual published pipeline entries while preserving bounded pending states and fatal compiler errors. Root ran preserved `native-input-view-lights02`, SHA256 `6744ffc0aa7ce929882db74c13c71f2b2d24240c36eafa354ae31bdc00bc750a`. Attempt02 produced a valid first checkpoint but failed the native input driver's pointer calibration; its partial evidence remains preserved. No product change was made for that driver failure.

Attempt03 used actual UI input with the exact process-owned window, measured cursor coordinates and fresh active-window compositor screenshots after each acknowledged GPU checkpoint. Lab → Battle → Lab completed with ordinary window close, exit0 in17.324s, AMD Vulkan and unchanged assets. Root inspected all three2880×1800 images and independently decoded/recomputed the actual GPU lighting records. Evidence: `/tmp/space-scuffle-r10-scene-lights03/`, including `root-independent-audit.json`.

The same camera4294967301 persists. Arena light4294967834 is visible, hidden, visible. Battle light4294967877 is created under root4294967859, becomes visible in Battle, and remains present but inherited-hidden on return. Each checkpoint contains exactly one eligible white14000lux light. Arena direct axis responses are(0.17232814,1.3786247,0.17232823), Battle responses(0,1.0404029,0); the independent recomputation error is at most4.1e−8. Initial and returned Lab readings are exact.

The acknowledged checkpoints each have zero queued mist. Later dynamic screenshots contain wakes, but this scene observer establishes actual scene-light ownership and prepared GPU response, not moving-mist fragment coverage or pixel equivalence. The separate fragment matrix above provides its stated coverage. The returned Lab screenshot still shows an unacceptable flat ribbon wake.

## Remaining acceptance

Stage2 lighting source is frozen. The coordinated full workspace gate remains pending while the ownership capture lane finishes its reviewed refinement. The prior round9 gate does not cover these active changes.

Root reopened the concept and the preceding accepted output-transform Close image. The current wake remains faint and flat against bright terrain; current captures retain that failure. No volume, full-game art, media-provenance or frame-budget acceptance is granted by this report.


Integrated checkpoint: root verified round10c full workspace gate on unchanged448 inputs:688 tests passed across88 harnesses,0failed/0ignored; strict workspace/all-target Clippy and formatting passed. `/tmp/space-scuffle-round10c-workspace-independent-audit.json` retains the independent count/hash audit. Earlier failed gates remain preserved; bounded functional acceptance does not approve art/media or whole-frame performance.
