# WP4a: `bevy_hanabi` spike against the six criteria of D-10

Evidence for WP4a of [`docs/plans/2026-11-09-effect-visualization.md`](../../plans/2026-11-09-effect-visualization.md)
(decision D-10, §4.2, §5.6 "Particles"). Spike branch `effects-hanabi-spike`, from `dd38da5`; not merged anywhere.
Decision rule of the plan: adopt only if all six criteria pass.

## Verdict

**No-go.** Criteria 1, 2, 3 and 4 pass, and the GPU half of 5 passes with a wide margin. Two
criteria are not met as the plan states them:

- **5, frame budget half: not demonstrated.** Three interleaved pairs of 400-unit runs on the quietest
  window available (one-minute load 14 to 19 on a machine shared with two other building agents) give
  +5.0 % on the median frame-interval p50 (10.04 to 10.54 ms), +1.1 % on p95 and +1.5 % on p99. The
  run-to-run spread inside one arm (8.76 to 10.31 ms) is larger than the effect, so neither "≤ 2 %"
  nor "> 2 %" is shown. A further series was destroyed by load spikes to 150 to 290.
- **6, composition with our bubbles: fails.** Against opaque geometry hanabi is correct
  (0 particle pixels over a closer opaque cube; 420 to 429 behind it when it is hidden). But unit bubbles
  are `AlphaMode::Blend` (`BubbleDomeMaterial::new`), so they are in the transparent phase with no depth
  write, and hanabi sorts a whole effect instance as one `Transparent3d` item at its entity's translation
  (`render/mod.rs`, `distance_translation(&batch.translation)`). A 3 000-spark burst entirely inside a
  closed blended sphere that looks opaque (alpha 1.0), with the emitter at the sphere's centre as a unit's
  effect would be, showed **787 particle pixels over the sphere on both radv and lavapipe**: every spark,
  although all are behind the sphere's near surface. The bubbles do not write depth, so only the
  per-instance sort order can hide a particle behind a bubble, and one sort key per effect cannot be right
  for a cloud that straddles a bubble's shell.

By D-10 the spike's evidence decides: particles take the CPU fallback (fixed SoA pool, drawn by the
`Sprite` instancer). That path is ours, so the two structural findings here (one sort key per effect
instance, §6; hanabi's per-frame main-world allocations, §3) become design points we control: WP4 must
still order sprites against blended bubbles (for example a bubble depth target or per-sprite sorting), and
the frame budget must be re-measured with the five-run protocol of
`docs/evidence/2026-11-03-frame-budget.md` on a quiet machine.

## Summary table

| # | Criterion | Number | Verdict |
|---|---|---|---|
| 1 | Builds against Bevy 0.15.3 / wgpu 23.0.1, no second Bevy or wgpu | `bevy_hanabi` **0.15.1** (MIT OR Apache-2.0); `Cargo.lock` gains 3 packages (`bevy_hanabi`, `copyless` 0.1.5, `rand_pcg` 0.3.1); one `bevy` 0.15.3, one `wgpu` 23.0.1, one `naga` 23.1.0 (the workspace patch) | Pass |
| 2 | Burst on radv and lavapipe, pixel statistics within tolerance | burst A: lit 2 134 vs 2 134 (0.00 %), mean luminance Δ 0.21 levels, centroid Δ 0.00 px, bbox Δ 0 px; burst B: 0.19 %, 0.33 levels, 0.21 px, 1 px | Pass |
| 3 | Pooled re-arm adds no main-world allocation | re-arm − idle = **0** blocks over 64 frames (pool 16, one re-arm per frame); planted control +64 | Pass (with a finding: hanabi itself allocates ≈ 16 + 6.1 per effect entity blocks per frame) |
| 4 | Textured, flipbooked, ramp-coloured, velocity-stretched, premultiplied | all five modifiers in one emitter; captures inspected | Pass |
| 5 | 30 000 particles ≤ 1.0 ms GPU; 400-unit frame budget moves ≤ 2 % | GPU: simulation 0.076 ms mean (max 0.199) + draw ≈ 0.17 to 0.24 ms = **≈ 0.25 to 0.32 ms**; budget: +5.0 % p50, +1.1 % p95, +1.5 % p99, inside the noise | GPU pass; budget not demonstrated |
| 6 | Render ordering composes with our sprites | opaque cube: 0 leaks (420 to 429 px hidden behind it); blended bubble: 787 particle pixels drawn over a closed bubble that contains the whole burst (radv and lavapipe) | Opaque: pass. Blended bubbles: **fail** |

## What the spike added (branch only)

- `crates/client/Cargo.toml`: `bevy_hanabi = { version = "=0.15.1", default-features = false, features = ["3d"] }`.
- `crates/client/src/effects/particles.rs` (the only module naming hanabi types, D-10): `ParticlesPlugin`
  (hanabi, plus an optional GPU span `render/hanabi_simulate` around hanabi's simulation driver node),
  `spark_burst_asset`, `sustained_asset`, `pooled_burst`, `rearm`, `fixture_sprite_sheet` (a 2×2
  premultiplied sheet built in memory; a fixture, never under `assets/`).
- `crates/client/tests/hanabi_spike_gpu_tests.rs`: criteria 2, 4 and 6, through `support/gpu_run.rs` (C21).
  `support/gpu_run.rs` gained `virtual_gpu_command_for(binary, selection)` so one test can run the same
  child on radv (locked) and lavapipe (unlocked).
- `crates/client/tests/hanabi_spike_allocation_tests.rs`: criterion 3, the DHAT pattern of
  `sync_allocation_count_tests.rs`.
- `crates/client_diagnostics/src/frame_budget/mod.rs`: `--particles <N>` keeps N hanabi particles alive in
  a cloud of radius 10 world units, 4 units above the camera target, with the GPU span installed.
- Neither test file is in `crates/gate/suites.ron`; this branch is evidence only.

## 1. Build and licence

- `cargo info bevy_hanabi@0.15` → 0.15.1 is the latest 0.15.x (latest overall 0.20.0). Licence from the
  downloaded crate's `Cargo.toml`: `license = "MIT OR Apache-2.0"`; `LICENSE-MIT` and `LICENSE-APACHE2` ship
  in the crate.
- Its dependencies: `bevy` 0.15 (default features off; `bevy_core_pipeline`, `bevy_render`, `bevy_asset`,
  `x11`, `png`), `wgpu` 23.0.1, `naga` 23, `naga_oil` 0.16, `rand` 0.8, `rand_pcg` 0.3, `ron` 0.8,
  `thiserror` 1, `bytemuck`, `bitflags`, `fixedbitset` 0.5, `copyless`, `anyhow`, `serde`.
- `Cargo.lock` diff: +`bevy_hanabi` 0.15.1, +`copyless` 0.1.5, +`rand_pcg` 0.3.1, and the `client` entry.
  `cargo tree -d --locked -e normal` lists no `bevy*`, `wgpu*` or `naga*` duplicate. The workspace's
  `[patch.crates-io] naga` also serves hanabi.
- Built from source with cargo (dev and release); no downloaded binary was run.

## 2. Same burst on radv and lavapipe

**Method.** `a_hanabi_burst_renders_alike_on_both_drivers_and_respects_depth` re-executes its test binary
through the GPU harness: on the selected driver (radv, lock per child), then on lavapipe
(`select_driver(Lavapipe)`, no lock). Each child renders 480×270, black clear, `Tonemapping::None`,
`Msaa::Off`, a fixed 1/60 s step (`TimeUpdateStrategy::ManualDuration`). One pooled burst entity
(3 000 sparks, `prng_seed` fixed) is re-armed per shot; 20 frames after the re-arm the window is captured.
A pixel is a particle pixel when its red level is above 6 (the cubes are pure blue, the background black).
Statistics: lit count, mean Rec. 709 luminance of lit pixels, centroid, bounding box. Tolerances: lit count
10 %, mean luminance 8 levels, centroid 3 px, bounding box 6 px.

**Numbers (final run).**

| Shot | Driver | Lit | Mean luminance | Centroid (px) | Bounding box (px) |
|---|---|---:|---:|---|---|
| A: origin (0, 0, −1.5), speed 1.0 | radv | 2 134 | 196.72 | (235.40, 136.52) | (211, 107)–(255, 166) |
| A | lavapipe | 2 134 | 196.52 | (235.40, 136.52) | (211, 107)–(255, 166) |
| B: origin (−0.8, 0.5, −1.5), speed 0.7 | radv | 1 555 | 198.54 | (211.89, 119.20) | (189, 96)–(235, 142) |
| B | lavapipe | 1 552 | 198.21 | (211.95, 119.40) | (190, 96)–(235, 142) |

This table is from the comparison run before the look and bubble shots were added. The final run (with
them, both drivers, 2026-11-10) gave burst A 2 106 vs 2 134 lit (1.31 %), mean luminance Δ 0.45 levels,
centroid Δ 0.15 px, bounding box Δ 0 px; burst B 0.58 %, 1.19 levels, 0.27 px, 1 px. All within
tolerance; the radv count varies between runs (next paragraph).

**Run-to-run variation on one driver.** Nine radv runs of burst A gave lit counts 2 106 to 2 134 (B: 1 543
to 1 555). Hanabi seeds each particle with `pcg_hash(particle_index ^ prng_seed)`, and `particle_index`
comes from an atomic pop of the dead list (`vfx_init.wgsl`), whose order between GPU threads is not fixed.
So a fixed `prng_seed` does **not** make a capture repeatable on the same driver, contrary to the plan's
§5.6 "Determinism of the picture". Captures of particles can only be compared with tolerances.

## 3. Pooled re-arm allocates nothing in the main world

**API (0.15.1), verified.**

```rust
// At load: one entity per pool slot. EffectProperties must be inserted explicitly:
// hanabi 0.15 does not require it on ParticleEffect and only fills an existing one.
commands.spawn((
    ParticleEffect::new(effect_handle),       // asset: SpawnerSettings::once(n.into()).with_emit_on_start(false)
    EffectProperties::default(),
    EffectMaterial { images: vec![sheet] },
    Transform::IDENTITY,
    Visibility::Visible,
));
// Per event (EffectSpawner is added by hanabi's tick_spawners on the first PostUpdate):
fn rearm(spawner: &mut EffectSpawner, properties: &mut EffectProperties, origin: Vec3, speed: f32) {
    properties.set("origin", origin.into());    // Value::from(Vec3); declared with writer.add_property
    properties.set("speed_scale", speed.into());
    spawner.reset();                            // next tick_spawners emits the whole `once` burst again
}
```

`EffectSpawner::reset()` zeroes the cycle time and count; with `SpawnerSettings::once` the next
`tick_spawners` (in `PostUpdate`, `EffectSystems::TickSpawners`) sets `spawn_count = n`. Properties are
matched by name; `set` on an existing property does not allocate (`PropertyInstance` is overwritten in
place). `EffectProperties::set_if_changed(Mut, …)` exists and avoids change detection when the value is
equal; `set` always marks the component changed.

**Method.** `hanabi_spike_allocation_tests.rs` runs each child under `valgrind --tool=dhat` at 64 and 128
measured frames after 64 warm frames, counting blocks whose stack passes through the non-inlined
`measured_steady_frames`, on the single-threaded executor. The children build the main world only:
`MinimalPlugins`, `AssetPlugin`, `TransformPlugin`, `HierarchyPlugin`, `VisibilityPlugin` and hanabi's
`build` (its `finish` needs a render device, which a valgrind child must not create). Variants: no hanabi;
hanabi with an idle pool; the same with one re-arm per frame round robin, with a new origin and speed each
time; and the re-arm variant with one planted allocation per frame. Each re-arm child checks that every
pooled entity holds an `origin` property afterwards (all re-arms hit a real entity).

| Variant (pool 16) | Window at 64 frames | at 128 | Difference (64 frames) |
|---|---:|---:|---:|
| No hanabi | 1 088 | 2 176 | 1 088 |
| Hanabi, idle pool | 8 401 | 16 801 | 8 400 |
| Hanabi, one re-arm per frame | 8 401 | 16 801 | 8 400 |
| Re-arm + planted control | 8 465 | 16 929 | 8 464 |

Re-arm − idle = **0**; planted − re-arm = 64 = the frame count. Pass.

**Finding: hanabi allocates every frame in the main world, in proportion to its effect entities.** Idle
hanabi adds 65.25 blocks per frame with a pool of 8 and 114.25 with a pool of 16, so about 16 + 6.1 per
effect entity per frame. The stacks are `compile_effects` (`lib.rs:1627`): every frame it builds a
`HashMap<Entity, (ParticleLayout, …)>` and calls `EffectAsset::particle_layout()` for every effect entity,
which builds a `Vec<AttributeLayout>` and a hashbrown set. Our charter (§3.3) wants battle presentation to
allocate nothing in steady state, and WP4's acceptance asks for "no main-world allocation" with an effect
stream; with hanabi that holds only if the baseline includes hanabi, and the pool size then sets a
per-frame allocation rate we cannot remove without patching hanabi. In the render world (not in this
window) `extract_effects` serialises `EffectProperties` into a new `Vec<u8>` whenever the component
changed, so each re-arm costs at least one render-world allocation.

## 4. Looks: texture, flipbook, ramp, velocity stretch, premultiplied blending

One emitter uses all of them: `ParticleTextureModifier` (slot 0, `ImageSampleMapping::Modulate`) on a 2×2
sheet (soft disc, ring, plus, small dot; white, premultiplied, `Rgba8Unorm`); `FlipbookModifier`
(`sprite_grid_size` 2×2) driven by `SPRITE_INDEX = min(age / lifetime · 4, 3)`;
`ColorOverLifetimeModifier` with a premultiplied ramp (white-hot, yellow, orange with RGB above alpha, so
partly additive, then zero); `SizeOverLifetimeModifier` 0.30×0.08 to 0.12×0.04 world units;
`OrientModifier { mode: OrientMode::AlongVelocity }`; `with_alpha_mode(AlphaMode::Premultiply)`.
The `extra` child renders a 40-spark burst at 5× size, captured 8, 40 and 70 frames after re-arm
(`look_early.png`, `look_mid.png`, `look_late.png`, kept under
`~/scratch/effects/spike/gpu/<driver>/`). Inspected by eye on radv: at 8 frames white-hot to yellow soft
discs (cell 0), clearly stretched along their radial velocity; at 40 frames yellow-orange rings (cell 1);
at 70 frames small orange-red dots (cell 3). Where sparks cross the blue cube their colour adds over it
(premultiplied, partly additive ramp keys) without dark fringes. Lavapipe's captures match. Pass.

## 5. Cost of 30 000 particles

**GPU.** `frame_budget --scenario 400 --particles 30000 --render-diagnostics` (release, RX 580, headless
KWin, 1280×720, vsync off; each run one `with-gpu-lock` command). The cloud: 15 000 spawns per second,
2 s life (30 000 live, capacity 33 750), radius 10 world units, sparks 0.9×0.24 world units at birth,
about 8×2 px from the frame-budget camera (75 units away). The spike's `ParticlesPlugin` wraps hanabi's
simulation driver node (init, indirect dispatch, update, sort compute passes; all recorded on the frame's
command encoder) in a `RenderDiagnosticsPlugin` time span. Mean over the whole measured window:

| Run | Particles | `hanabi_simulate` | `main_transparent_pass_3d` | Sum of instrumented passes |
|---|---:|---:|---:|---:|
| D0 | 0 | — | 0.428 (max 1.548) | 3.466 |
| D1 | 30 000 (span not yet installed) | — | 0.599 (max 2.473) | 3.576 |
| D2 | 30 000 | 0.076 (max 0.199) | 0.667 (max 1.881) | 3.908 |

The particles cost 0.076 ms of simulation and 0.17 to 0.24 ms of drawing (the transparent pass grows by
that much; hanabi draws in `Transparent3d`), about **0.25 to 0.32 ms**, under the 1.0 ms line even with
the whole difference of the instrumented sums (0.44 ms, which also carries the shadow passes' run-to-run
variation). Limits: one run per arm; small sprites (fill-light); the opaque, shadow and prepass passes
should not change with particles and changed by ≤ 0.12 ms.

**Frame budget.** Same tool and path, no diagnostics, 400-unit scenario, interleaved, frame interval of
the whole 1 500-tick window (ms). Load is the one-minute load average at start and end.

| Series | Run | Particles | Load | Frames | p50 | p95 | p99 | Update p50 |
|---|---|---:|---|---:|---:|---:|---:|---:|
| 2 | C1 | 0 | 17.2 → 18.3 | 5 420 | 10.04 | 16.72 | 19.25 | 8.21 |
| 2 | P1 | 30 000 | 18.3 → 19.1 | 5 399 | 10.13 | 17.05 | 19.81 | 7.95 |
| 2 | C2 | 0 | 19.1 → 18.0 | 5 975 | 8.76 | 16.58 | 19.15 | 6.88 |
| 2 | P2 | 30 000 | 18.0 → 16.2 | 5 414 | 10.68 | 15.22 | 17.46 | 8.51 |
| 2 | C3 | 0 | 16.2 → 15.1 | 5 405 | 10.31 | 17.42 | 20.32 | 8.40 |
| 2 | P3 | 30 000 | 15.1 → 13.9 | 5 281 | 10.54 | 16.91 | 19.54 | 8.50 |
| **2** | **median** | **0 / 30 000** | | | **10.04 / 10.54 (+5.0 %)** | **16.72 / 16.91 (+1.1 %)** | **19.25 / 19.54 (+1.5 %)** | **8.21 / 8.50 (+3.5 %)** |
| 1 | A1, B1, A2, B2, A3, B3 | 0 / 30 000 alternating | 22 → 290 | | 12.72, 13.11, 15.24, 10.95, 10.08, 10.82 | | | |
| 3 | P4, C4, P5, C5, P6, C6 | 30 000 / 0 alternating | 13 → 150 | | 11.34, 8.34, 11.36, 13.82, 14.68, 9.90 | | | |

Series 1 and 3 ran while other agents drove the load average to 150 to 290; they are listed for
completeness and show the noise (±25 %). Series 2 is the only usable one, and its within-arm spread
(C: 8.76 to 10.31 ms) is three times the 2 % line. The +3.5 % on the main-world update p50 is consistent
with hanabi's per-frame main-world work (§3), but this measurement cannot separate it from noise. The
record protocol (16 dedicated cores, load below 8, five runs per arm) was not available today.

## 6. Composition with opaque geometry and with blended bubbles

**Opaque.** The `main` child has an unlit blue cube whose back face (z = 1.9) is in front of every spark,
covering the right part of the cloud, and a second cube behind the cloud. Burst A with the front cube:
**0** particle pixels inside its silhouette (9 940 px). The same burst with the front cube hidden: 420
(radv) and 429 (lavapipe) particle pixels inside that silhouette, so the occlusion is really tested.
Particles do draw over the farther cube (674 / 673 px). Hanabi's particles depth-test against the opaque
depth buffer. Pass.

**Blended bubbles.** Unit bubbles use `BubbleDomeMaterial` with `AlphaMode::Blend`, so they are drawn in
`Transparent3d`, sorted per mesh by distance, and write no depth. Hanabi adds one `Transparent3d` item per
effect instance at the instance entity's translation. The `extra` child places an unlit `StandardMaterial`
sphere with `AlphaMode::Blend` and alpha 1.0 (the same phase and depth behaviour as a bubble), radius 1.3 at
(0, 0, −1.5), and fires the pooled burst with its entity moved to the sphere's centre and speed 0.4, so
every spark stays within about 0.95 units of the centre, inside the sphere. Correct composition shows no
particle. Captured 20 frames after the re-arm: **787 particle pixels on radv and 787 on lavapipe**
(`bubble_inside.png`): the whole burst drawn over the bubble. With the emitter's distance equal to the
bubble's, the sort put the effect after the bubble; moving the entity further away would instead hide the
sparks that are in front of the shell. One per-instance key cannot order a cloud against a shell it
crosses, and hanabi offers no per-particle ordering against other items, no depth test against blended
geometry and no soft-depth hook. **Fail** for our bubbles. Our own sprite instancer would face the same
question, but it can test against a bubble depth target or split draws around shells; hanabi cannot without
patching.

## 7. Gotchas for WP4 (whichever way the decision goes)

1. **`EffectProperties` is not added automatically** in 0.15.1; insert `EffectProperties::default()` with
   `ParticleEffect`, or a query for it finds nothing and re-arms are silently skipped (the first
   allocation measurement of this spike measured nothing for that reason; fixed and re-run).
2. **Spawning before the pipelines exist corrupted a pooled burst on radv.** With `SpawnerSettings::once`
   and its default `emit_on_start = true`, the burst fired on its spawn frame while hanabi's pipelines were
   still compiling; 2 of 6 radv runs then drew every later burst as a single sprite at a fixed place
   (lit 16 px instead of 2 134), while lavapipe never failed. With `.with_emit_on_start(false)` and 60 idle
   frames before the first re-arm, all 11 later radv runs drew the full burst. The two changes were made together, so the
   exact trigger is not isolated. A pool must be spawned at load, never emit on start, and be warmed.
3. **Sorting is per effect instance at the entity's `Transform`,** not per particle and not at the
   property origin. A pooled entity re-armed only through an `origin` property keeps sorting at its old
   transform. Move the entity's `Transform` to the event position on re-arm; use properties for radius,
   speed and seed only.
4. **Validation warning in hanabi's render pipeline:** `WARNING-Shader-OutputNotConsumed`, "Vertex attribute
   at location 2 not consumed by vertex shader": hanabi binds the quad's normal at location 2
   (`render/mod.rs`, `ATTRIBUTE_NORMAL.at_shader_location(2)`) but `vfx_render.wgsl` declares it only under
   `NEEDS_NORMAL`. Harmless, but the GPU harness fails on it; the spike test allowlists it with that reason.
5. **The simulation driver's render-graph label is private** (`mod plugin` is not public, so
   `main_graph::node::HanabiDriverNode` cannot be named). The spike finds the node by its type name
   `VfxSimulateDriverNode`. The span plugin must `finish` after `HanabiPlugin` (register it after hanabi);
   `cleanup` is too late, because Bevy's pipelined rendering moves the render sub-app to its thread in its
   own `cleanup`.
6. **`EffectProperties::set` always marks the component changed,** which re-serialises and uploads all
   properties of that instance; use `set_if_changed` where values repeat.
7. **lavapipe runs hanabi's compute passes correctly** (identical statistics to radv within 0.2 %); no
   `RenderPlugin` or `WgpuSettings` change was needed on either driver. Timestamp queries inside encoders
   are available on radv (the span works with Bevy's default `WgpuSettings`).
8. **Not deterministic per driver** (see §2): fixed `prng_seed` does not fix particle placement.
9. The compiler hung twice in this session on `client` (0 CPU, futex wait) under heavy IO pressure from
   other agents; `CARGO_INCREMENTAL=0` and clearing `target/debug/incremental/client-*` recovered. Not a
   hanabi issue.

## Commands

```bash
export CARGO_TARGET_DIR=~/scratch/cargo-target/effects-spike
# 1
cargo info bevy_hanabi@0.15; cargo tree -d --locked -e normal
# 2, 4, 6 (radv through the harness, then lavapipe; never wrapped in with-gpu-lock)
SPACE_SCUFFLE_HANABI_SPIKE_KEEP=~/scratch/effects/spike/gpu \
  cargo test -p client --test hanabi_spike_gpu_tests -- --exact \
  a_hanabi_burst_renders_alike_on_both_drivers_and_respects_depth --nocapture
# 3
cargo test -p client --test hanabi_spike_allocation_tests -- --exact \
  rearming_a_pooled_burst_allocates_nothing_in_the_main_world --nocapture
# 5: one locked run per command, KWin and the tool inside the lock (~/scratch/effects/spike/fb_run.sh)
WGPU_BACKEND=vulkan VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json with-gpu-lock bash -c \
  "kwin_wayland --virtual --xwayland --no-lockscreen --no-global-shortcuts --width 1280 --height 720 \
   --socket spike-fb-ID --exit-with-session '$FB --display-path kwin-virtual --work-dir W \
   --scenario 400 --out run-ID.ron [--particles 30000] [--render-diagnostics]'"
```

Host: shared VM, 16 CPUs visible, AMD Radeon RX 580 (RADV POLARIS10), Mesa 25.0.7-2+deb13u1; Bevy 0.15.3,
wgpu 23.0.1, rustc 1.99.0. Raw reports, logs and captures are under `~/scratch/effects/spike/` (scratch
files that may be deleted; the tables here are the record).
