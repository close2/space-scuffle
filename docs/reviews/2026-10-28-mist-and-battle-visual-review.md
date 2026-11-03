# Actual renderer review — mist and runtime battle

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

## Evidence

The reference remains `tmp/Gemini_Generated_Image_12da9g12da9g12da.jpg`. It was re-opened alongside actual game output. Its important qualities are readable translucent shells, narrow controlled highlights, layered directional wakes, restrained terrain contrast, and a coherent visual hierarchy. Matching its exact style is not required.

Native renderer: AMD Radeon 890M, RADV, Mesa 26.2.4, Vulkan. No validation flags were disabled. The actual game used the selected scenario, external catalogs, production terrain, and production mist plugin.

- `/tmp/space-scuffle-round2-battle-long-startup/battle-1.png`: live battle at tick 642.
- `/tmp/space-scuffle-round2-battle-long-startup/battle-2.png`: live battle at tick 1485, visible attack and changed entity count.
- `/tmp/space-scuffle-mist-close-v1/Close-{0,1,2}-fps60-msaa4.png`: production mist preview, physical 1280×720, successfully compiled mist pipeline, one occupied/visible emitter, 29 retained samples, zero capacity overflows at final capture.
- Full logs: `/tmp/space-scuffle-round2-battle-long-startup/client.log` and `/tmp/space-scuffle-r2-mist-close-v1.log`.

The first 22-second game capture attempt was entirely black; the longer attempt rendered after roughly 32 seconds of cold startup. A black frame before readiness is rejected, not interpreted as the finished scene. Startup time is a separate usability/performance concern.

## Battle verdict: rendered, visually rejected

The authored deck's aligned panel texture is visibly corrected, and editor grid overlays are absent. This confirms the sampling correction in actual game output, rather than only uniform tests.

The scene does not meet the overall quality goal. Shell highlights occupy broad clipped-white bands, and the attacked structure becomes nearly featureless white/yellow. The background stars appear coarse and blurred. The starting camera leaves substantial empty space and misses most of the advancing army. Existing sandbox squad labels also do not describe the declarative session reliably. These are separately scoped follow-ups; the mist change must not claim to have repaired the whole composition.

## Close mist verdict: rejected

The new wake is barely visible as a faint gray strip on the bright deck. A conspicuous hard dark line behind the unit appears to be the ribbon casting an opaque shadow; this requires verification and correct transparent-effect shadow policy. The image lacks the readable flow, layered density, and depth required by the goal.

The close fixture also fails its framing purpose: the bubble is approximately 120 pixels across near the lower edge, while empty deck and background dominate. Reframe around the full bubble and tail, then capture again. Do not solve readability by turning the effect into an opaque or neon streak, and do not darken only the test fixture to conceal the real-game mismatch.

No visual acceptance follows from `visible=1`, a ready pipeline, three saved images, or exit status zero. The images themselves fail the stated bar.

## Renderer correctness remains open

Both fresh runs reproduce the existing SPIR-V layout and atomic diagnostics. The mist capture additionally reproduces missing acquire waits, write-after-present hazards, and semaphore acquisition validation errors. Plan 31 addresses these separately with actual compiler and presentation fixes; the logs remain failure evidence until genuinely corrected.

## Optical v3 state matrix and capture interruption finding

Root captured the actual production bubble shader across seven states for spheres and domes, plus healthy-sphere and shield-hit-dome HDR variants: `/tmp/space-scuffle-optics-v3-matrix/`. `artifact-audit.json` checks fresh PNG/CSV counts separately from process exits. Fourteen cases produced all three images; sphere hull-hit produced only two, and dome recovering only one. These two runs are incomplete failures even though the old capture binary returned zero. Concurrent native-window automation used an insufficient xdotool selector (`--pid` and `--name` combine with OR unless `--all` is present); that evidence is invalidated and future mutations require exact process ownership checked through `_NET_WM_PID`. A completion-mask guard is approved before fresh replacement runs. Original incomplete evidence remains untouched.

Root inspected the middle frames of every completed state and the available sphere hull-hit frame. The v3 healthy outline is narrower and restrained, with readable creature detail and no broad rainbow band. Opaque source media still makes the interior read as a disc. Attack draws an orange circular outline across the creature/building; shield-hit draws a cyan ring across the media. These remain visually diagrammatic and are rejected despite bounded brightness. Hull-hit retains a soft orange patch; damaged states show conspicuous RGB channel separation. The matrix does not establish concept-level art quality. HDR and non-HDR healthy/shield examples retain similar readable brightness; the sampled shell metrics report zero saturated-white shell pixels. This is color/clipping evidence only.

Actual battle run02 (`/tmp/space-scuffle-r4-runtime-captures-run02/`) records the same actor entering a ramp at tick162 and leaving at tick195. Root viewed overview, ramp entry/exit, combat and lab images. Rendering follows actual map elevation and the ramp is visible, but the screenshots omit egui because its render attachment bypasses Bevy screenshot output. These are rejected complete-game captures until the approved capture attachment correction is verified. The combat image also shows a hard rectangular shadow near a light effect; source review found that existing CombatFxMaterial quads lack NotShadowCaster. Mist ownership is not assumed to be the cause. A separate effect-shadow correction is needed.

The completion guard is now verified using an exact process-owned native window. `/tmp/space-scuffle-optics-v3-interrupted-guard/` records one saved image, typed incomplete failure and exit 1. Both replacement cases in `/tmp/space-scuffle-optics-v3-complete-reruns/` save three fresh PNG/CSV pairs and exit 0. Root inspected the formerly missing recovering-dome middle frame; its cyan ring and orange patch retain the previously stated visual rejection. All sixteen requested combinations now have complete artifacts, using replacements for the two interrupted runs.

Root independently opened UI-inclusive run04 overview, combat and lab captures. The UI is present and its controls render, while stale squad labels, large inspector footprint, rigid-looking trails and combat-card shadows remain visible defects. This is accepted as truthful capture evidence, subject to exact-media checks, not as game visual acceptance. The default-resolution layout problem remains a product task; a1920×900 capture does not repair it.

## Round 6 renewed mist comparison

Root re-opened the concept and `/tmp/space-scuffle-r5-audio-policy-mist/Close-1-fps60-msaa4.png`, then inspected the current production `/tmp/space-scuffle-r6-viewport-battle/04-combat.png`. The close wake nearly disappears into the bright deck, apart from straight narrow edge traces. In the battle it instead reads as a continuous pale strip joining consecutive unit positions. Neither image establishes a volume of dissipating material. The change in apparent visibility between fixtures is itself a readability concern, not evidence that one setting is satisfactory. The opaque creature disc and bright repetitive terrain also weaken the composition; those separate media/scene defects must not be hidden by an exaggerated wake.

The next visual round must be judged against these concrete conditions:

- At the actual gameplay scale, the direction of travel is readable without a continuous hard-edged band or a line that could be confused with a targeting link.
- The wake has a controlled near-source density and visibly disperses downstream; its silhouette and internal layers must not expose rectangular cards or repeated equal-width strands.
- Terrain remains visible through the effect. Terrain intersections and unit overlaps have no opaque cutout, hard clipping seam, or false cast shadow.
- Bright authored deck, darker cliff-adjacent ground, straight travel, turns, stopping, and overlapping groups all retain coherent readability. A single flattering test background is insufficient.
- Close, ordinary game and crowd captures preserve creature silhouette and team/selection cues. More glow, more noise, or more particles alone are not acceptance criteria.
- Temporal captures show continuous emission, bends and dissipation without popping or visible history/owner reassignment. Static images must be supplemented by successive actual rendered frames.

Performance and image quality are separate acceptance conditions. Persistent GPU storage35 may remove geometry upload cost while preserving exactly the currently rejected appearance. Its optimization cannot close the visual task.
