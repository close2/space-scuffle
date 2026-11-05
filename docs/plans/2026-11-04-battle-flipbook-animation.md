# Blueprint: Battle Flipbook Animation, a Presentation Clock for Unit and Structure Flipbooks (finding F-1)

**Status of this document:** Stage 1 blueprint under charter §6, written on 2026-11-04 against `main` at `9a1e940`.
**Implementation status (2026-11-04):** complete. WP1 (`9d5884c`), WP4 (`4bd48cf`), WP5 (`ec04bf4`), WP3
(`1940231`), WP6 (`d5779f7`) and WP2 (`6f0e13d`, checksum schema 10, goldens re-recorded with no behaviour change:
render snapshots and the whole battle matrix bit-identical) are merged. WP7's measurement is in
[`docs/evidence/2026-11-04-battle-flipbook-animation.md`](../evidence/2026-11-04-battle-flipbook-animation.md). The text below is the blueprint as approved. Line numbers drift; search for the quoted symbol. A claim that could not be confirmed while
planning is marked **verify**, and the first step of its package confirms or refutes it.

**Finding being fixed.** F-1 of [`2026-11-04-veo-seamless-loops.md`](2026-11-04-veo-seamless-loops.md) §5: the
battle view never animates a flipbook. Every unit and structure bubble shows the first frame of its state's slice.
Only the Visual Lab and the swarm preview animate, at fixed periods that ignore the media. Real Veo media (Idle,
Walk, Attack, Constructing, Cast) is arriving through the Veo plan, so this gap is now visible. Confirmed in §2.1,
with a red run of a probe in §2.6.

**Rules that bound this plan.** Charter §3 (the simulation is deterministic and carries no float presentation
state; the client converts at the `RenderSnapshot` boundary; per-frame sync allocates nothing and does no
O(N²) search), §3.3 (no identity names in code: every timing comes from the manifest or the catalog), §1.2 (real
end-to-end tests, GPU parity on the real shader). Q0020: the agent decides visual questions, the owner may
overrule. Every visual choice below (play speed, pause behaviour, frame blending, how Attack is timed) is such a
decision.

**Out of scope.** A Die animation (the simulation never publishes `Die`, §7 F-2); the per-state frame budget of
the bake (§7 F-3); audio; the shader's `globals.time` jitter (§7 F-4).

---

## 1. Summary

| WP | Title | Size | Depends on | Lane |
|---|---|---|---|---|
| WP1 | Snapshot: `AttackSwing` from the weapon state machine; `BubbleRenderInstance::anim_progress` removed | M (1 day) | none | sim (no checksum change) |
| WP2 | `Entity::anim_progress` removed; checksum schema 9 → 10; goldens re-recorded | S (2 to 3 h) | WP1 | **lane B** of `2026-11-04-combat-sc2-alignment` (one slot, behaviour-neutral) |
| WP3 | Manifest: optional per-slice `duration_ms`, written by the bake | S (half a day) | `2026-11-04-veo-seamless-loops` WP2 merged (same file) | asset_baker |
| WP4 | GPU flipbook clock: shared clock buffer written in place, 176-byte uniform, integer frame function in WGSL and Rust, frame blending, GPU parity test | M-L (1 to 2 days) | none (parallel with WP1) | client |
| WP5 | Battle: presented time, displayed action, `FlipbookPhase` per bubble, packing, writer table, behaviour and churn tests | M (1 day) | WP1, WP4 | client |
| WP6 | Lab, swarm and inspector portrait on the same clock; ad hoc periods deleted | S (half a day) | WP4 | client |
| WP7 | Measurement: frame budget at 400 units before and after (headless KWin, RX 580); capture determinism; evidence; plan index | S (half a day) | WP5, WP6 | measurement |

WP1 and WP4 own different files and run in two worktrees. WP3 runs whenever the seamless-loops WP2 has merged.
WP2 waits for its slot in lane B, because every lane B package edits the golden fixtures (§4 WP2).

---

## 2. Facts this design rests on

### 2.1 The finding, confirmed in the code

| Where | What it does |
|---|---|
| `crates/sim_core/src/entity.rs:259` | `pub anim_progress: SimScalar` on `Entity`, documented "Normalized animation progress within loop" |
| `crates/sim_core/src/entity.rs:350` | `Entity::from_archetype` sets it to `SimScalar::ZERO` |
| whole `crates/sim_core/src` | no other write: `grep -rn "anim_progress" crates/sim_core/src` finds only the field, its initialiser, the snapshot read and the checksum encoder |
| `crates/sim_core/src/world/snapshot.rs:171` | `anim_progress: entity.anim_progress.to_f32()`, so every published record carries `0.0` |
| `crates/sim_core/src/checksum/encoding.rs:295` | the constant zero is part of every world checksum |
| `crates/sim_types/src/snapshot.rs:43` | `BubbleRenderInstance::anim_progress: f32`, "Normalized animation progress [0.0, 1.0] within the current loop" |
| `crates/client/src/bubble_shader.rs:236-261` | `BubbleUniformGpu::from_instance` turns `anim_progress` into `frame_index`: `start_frame + ⌊progress · frame_count⌋`. With progress 0 this is always `start_frame` |
| `crates/client/src/scene_controller.rs:681-700` | `LiveBubbleFrame::new`, the battle's only packing path, calls `from_instance` with the snapshot record unchanged |
| `assets/shaders/bubble_dome.wgsl:175` | `let layer_index = i32(bubble.frame_index);` the shader samples exactly that layer |
| `crates/client/src/scene_controller.rs:179` | the Lab preview sets `anim_progress: (elapsed_seconds * 1.5).fract()`, a 0.67 s period for every state and atlas |
| `crates/client/src/scene_controller.rs:1153` | the swarm uses `(time.elapsed_secs() * 2.0).fract()`, a 0.5 s period |
| `crates/client/src/lab_ui.rs:985` | the inspector portrait plays the whole atlas (all states) in 0.67 s |

### 2.2 Media timing

- `FlipbookAtlasMeta::fps` is per atlas; `ActionSlice` has `start_frame`, `frame_count`, `looping`,
  `hold_last_frame` and no duration (`crates/asset_baker/src/manifest.rs:53-86`). All eight shipped manifests say
  `fps: 15`.
- The one Veo-derived atlas, `pulse_bot`, bakes three clips of 4.01 s each (`ffprobe`, `assets/videos/pulse_bot_*.mp4`)
  into 10 frames each (`assets/textures/pulse_bot.provenance.ron`, `max_frames_per_state: 10`). Played at the
  manifest's `fps`, a 4 s clip would last `10 / 15 = 0.67 s`, six times too fast. The bake knows the clip's
  duration and throws it away: `fit_clip_to_budget` samples evenly over the whole clip
  (`ffmpeg_importer.rs:179`), and only `fps` is written.
- The seamless-loops plan's loop sampling (`evenly_spaced_loop_indices`, its D-5 step 1) spaces the kept frames
  evenly over the clip, the wrap included. One loop of a slice therefore lasts exactly the source clip's duration.
- Structure states get 16 frames, unit states 10 (seamless-loops plan D-5).

### 2.3 Presentation time

- `BattleFrameState::present(alpha)` (`battle_pipeline/timeline.rs:228`) shows the previous tick's record, with
  positions and angles interpolated towards the current one by `alpha`. Discrete fields stay at the previous record
  until the current tick is reached. When paused or held it shows the current tick exactly (`alpha` taken as 1).
  The fraction is not stored: nothing downstream can know the presented time inside a tick today.
- `BattleSession::advance` (`driver.rs:108`) runs up to `ticks_per_frame = 5` ticks per frame, scaled by the
  battle speed 0 to 4; speed 1 is real time at 25 Hz (`clock.rs:10`). Pause and speed 0 call `present(1.0)`.
- The capture tool (`client_diagnostics/src/runtime_capture/mod.rs`) sets
  `TimeUpdateStrategy::ManualDuration(Duration::ZERO)`, so Bevy's virtual time is frozen, and its driver steps the
  simulation one tick per frame and fails on a skipped tick (`runtime_capture/driver.rs:671-681`). A clock built on
  simulation ticks is therefore reproducible in captures; a clock built on Bevy time is frozen there.

### 2.4 Material churn

- `sync_bubble_material` (`bubble_shader.rs:166`) calls `Assets::get_mut` only when `renders_same_as` says a lane
  the shader reads changed. `renders_same_as` ignores `world_pos` and `anim_progress`, because the shader reads
  `frame_index` instead (`round2_visual_tests.rs:233` pins this).
- A modified material is prepared again by Bevy: `as_bind_group` builds a new uniform buffer and a new bind group
  (`bevy_pbr-0.15.3/src/material.rs:969`). The frame-budget evidence measured what per-material preparation costs
  in a fight (C-17: about 0.44 ms of main-thread time per frame, plus a visibly longer frame interval in heavy
  combat frames; `docs/evidence/2026-11-03-frame-budget.md` conclusion 4).
- So a CPU-side frame index would modify every animating bubble's material each time its frame steps: 400 units at
  10 to 15 frame steps per second is 65 to 100 material preparations per rendered frame at 60 frames per second,
  more than C-17's whole effect pool. The paused-frame test (`sync_steady_state_tests.rs:666`) would still pass,
  since nothing moves when paused, but every running frame would churn.
- Two modules already bypass the asset pipeline for per-frame data and write the GPU buffer in place during
  extraction with `RenderQueue::write_buffer`: `vfx/gpu_upload.rs` (C-17's fix) and `smog/gpu_upload.rs` (C-2).
- A `ShaderStorageBuffer` asset that is modified gets a brand new GPU buffer
  (`bevy_render-0.15.3/src/storage.rs:115-129`); materials prepared earlier keep the old one. A shared buffer must
  therefore be written in place, never modified as an asset.
- `AsBindGroup` in Bevy 0.15 accepts `#[storage(n, read_only)] Handle<ShaderStorageBuffer>`
  (`bevy_render_macros-0.15.3/src/as_bind_group.rs:231`), and retries a material whose buffer is not prepared yet.

### 2.5 Weapon timing and the published action

- Tick order (`world.rs` `step_into`): movement (which writes `current_anim`) runs before weapons. A swing that
  starts on tick `T` is published as `Attack` from tick `T + 1` (`locomotion.rs:1014-1020` reads the weapon phase
  left by the previous tick).
- `Attack` is published only during `Windup` and `Backswing`. Catalog values: damage point 0 to 4 ticks, backswing
  1 to 6, period 9 to 54 (`assets/catalog/units.ron`). So `Attack` lasts 1 to 10 ticks of a 9 to 54 tick cycle;
  the rest of the cycle is published as `Idle`. Played as published, an attack clip would flash for 40 to 400 ms
  and snap back to idle.
- At release (`combat.rs` `step_windup`) the cooldown is set to `period − damage_point` and then falls by one per
  tick. From the swing's start, the elapsed time is therefore `damage_point − remaining` in `Windup` and
  `period − weapon_cooldown_remaining` from the release until the cooldown ends, with no new state.
- The resolved period depends on upgrades and buffs (`combat_resolution.rs:244-253`, `resolve_attack_period`), and
  the weapon depends on the target's mover (`weapon_against`). The snapshot builder can recompute both from
  world state it already reads.
- An immobile actor never publishes `Walk` or `Attack` (`published_actions.rs` module docs): locomotion returns
  early for it. An armed structure is always `Idle` in the snapshot.
- Construction progress is published as `build_or_queue_progress`, in steps of 1/65536 (`world/snapshot.rs:34`).

### 2.6 Red run of the finding

A temporary probe (not committed) booted the production client on the default battle with the test harness,
resumed it, and recorded every live bubble's material `frame_index` for 300 ticks:

```text
ticks 0..300 (entity, state) pairs 46 advancing 0 states {"Idle", "Walk"} distinct anim_progress [0.0]
test result: FAILED. 0 passed; 1 failed   (assert: "no live bubble ever showed a second flipbook frame of one state")
```

46 (entity, state) pairs over 300 ticks of `Idle` and `Walk`, none of which ever showed a second frame, and the
only `anim_progress` ever published was `0.0`. Run on 2026-11-04 at `9a1e940` with
`cargo test --locked -p client --test plananim_probe`, debug build, no GPU (the harness has no renderer). WP5's
`idle_unit_frames_advance` is the committed form of this probe.

---

## 3. Design decisions

### D-1. The time base is presented simulation time, in 1/1024 of a tick

The animation clock is `now = presented_tick · 1024 + ⌊presented_fraction · 1024⌋`, a `u32` in units of 1/1024
tick (39.06 µs of game time at speed 1). `presented_fraction` is the `alpha` of the last `present` call, or 0 when
the record shown is the current tick.

- **Pause freezes animation; speed scales it.** As in StarCraft II, game speed is the speed of the whole picture.
  A paused battle's frames change nothing, which keeps `paused_battle_frames_create_modify_and_remove_nothing`
  green without exceptions.
- **Captures are reproducible.** The capture tool steps one tick per frame with frozen Bevy time, and this clock
  depends on ticks only.
- **Interpolation is continuous.** At `alpha → 1` the clock reaches `previous_tick + 1 = current_tick`, the value
  the next frame starts from. Pausing snaps forward by less than a tick, exactly as positions do (`hold_exact`).
- **Integers end to end.** Frame selection (D-2) is integer arithmetic on this value, so radv, lavapipe and the
  Rust mirror pick the same frame at every boundary. Floats near `k/n` boundaries would not.
- **Wrap.** `u32` wraps after 2³² / 25 600 s = 46.6 h of game time; differences use wrapping subtraction, and a
  loop glitches once at the wrap. Documented, not handled.
- The Lab and the swarm have no battle; there the clock is Bevy's `Time` elapsed seconds × 25 × 1024 (WP6).

Rejected: wall time (animates while paused, breaks the paused-frame test and capture reproducibility); a clock in
`sim_core` (presentation state in the simulation and in the checksum, charter §3).

### D-2. The GPU picks the frame from one global clock and per-bubble lanes that change only with the action

The shader computes the layer from `now` (D-3) and lanes in the bubble's uniform that depend only on what the
bubble is doing, not on time. A bubble's material therefore changes when its action changes or a new attack swing
starts, never because time passed. This keeps the C-5 rule ("a material changes only when a lane the shader reads
changes") true, and removes the churn of §2.4.

The frame function, identical in WGSL and in Rust (`flipbook_layer`), with `e = now −% origin` (wrapping):

| `mode` | layer |
|---|---|
| `FIXED` | `base` |
| `LOOP` | `base + (((e % period) + offset) % period) · count / period` |
| `ONCE` (hold last) | `base + min(e, period − 1) · count / period` |
| `ONCE` (no hold) | as hold last while `e < period`, then `base` |

All terms are `u32`; `period · count` stays below 2³² for any slice under 60 s with up to 128 frames (checked by a
debug assertion in the packer and a unit test at the limit). Frame blending (D-6) also uses `(e' · count) % period`
as the blend weight's numerator, converted to `f32` once.

Rejected: a CPU frame index written through `get_mut` (§2.4, 65 to 100 preparations per frame at 400 units);
writing each bubble's uniform in place every frame like the effect pool (N buffer writes per frame, and two writers
of one uniform, the asset copy and the in-place copy, that would disagree after the next `get_mut`).

### D-3. One 16-byte clock buffer, written in place during extraction

- A `ShaderStorageBuffer` holding `FlipbookClockGpu { now: u32, _pad: [u32; 3] }`, created once at plugin build
  under a fixed weak handle `FLIPBOOK_CLOCK_BUFFER` (a `Handle::weak_from_u128` constant, as Bevy's internal shaders
  are), usage `STORAGE | COPY_DST`. **Verify** that an asset inserted under a weak handle id survives with no strong
  handle; if not, the plugin keeps one strong handle in a resource.
- `BubbleDomeMaterial` gains `#[storage(3, read_only)] pub flipbook_clock: Handle<ShaderStorageBuffer>`, defaulting
  to that handle. Every constructor goes through one `BubbleDomeMaterial::new(uniform, texture)` so no path can
  forget it (the smog's bouncing bubbles, the Lab, the swarm, the battle).
- Main world: resource `FlipbookClock { now: u32 }`, written by one system (D-8) with `set_if_neq`.
- Render world: `upload_flipbook_clock_system` in `ExtractSchedule` reads `Extract<Res<FlipbookClock>>` and, only
  when it changed, writes the 16 bytes into `RenderAssets<GpuShaderStorageBuffer>`'s buffer for the handle with
  `RenderQueue::write_buffer` (the `vfx/gpu_upload.rs` pattern, with its byte-layout test). The asset is never
  modified after creation, so no material is prepared again and the steady-state tally sees no
  `ShaderStorageBuffer` event.

### D-4. The bubble uniform grows from 160 to 176 bytes

| Offset | Today | After |
|---|---|---|
| 52 | `anim_progress: f32` (never read by the shader) | `anim_origin: u32` |
| 92 | `frame_index: u32` | `frame_base: u32` |
| 160 | (end) | `anim_period: u32`, `frame_count: u32`, `anim_mode: u32` (mode in bits 0-1, blend enable in bit 2), `anim_offset: u32` |

The WGSL struct mirrors it; the invariant in the module docs, `headless_verification` (`lib.rs:725`) and the layout
tests change from 160 to 176. `renders_same_as` compares every animation lane (the shader reads all of them) and
keeps ignoring `world_pos` only. `round2_visual_tests.rs::test_bubble_fragment_does_not_read_cpu_only_uniform_fields`
drops `anim_progress` from its list and requires that the fragment reads `anim_origin`, `anim_period` and
`frame_count`.

### D-5. What each action shows, and from when

The client derives a **displayed action** from the presented record, then its lanes:

| Displayed action | When | Lanes |
|---|---|---|
| `Attack` | published `Attack`, or published `Idle` with `attack_swing` present (§2.5: the cooldown part of the cycle, and the first tick of a swing), and the atlas has an `Attack` slice | `LOOP` over `period = swing.period_ticks · 1024`, `origin = 0`, `offset` such that the slice's release frame shows at the release tick (below) |
| `Constructing` | published `Constructing` | `FIXED`, `base = start + min(⌊progress · count⌋, count − 1)` from `build_or_queue_progress`. Exact on the CPU (progress is a multiple of 1/65536). It already changes on every tick a worker builds, so it adds no churn. A stalled foundation stays on its frame |
| `Idle`, `Walk`, `Cast` | published as such | `LOOP` (or `ONCE` if the slice says `looping: false`) over the slice period (D-6), `origin` = the tick the bubble began this displayed action (`FlipbookPhase`, below) |
| any | the atlas has no slice for it, or no atlas | `FIXED`, `base = 0` (the media validation in `media.rs` already rejects a published state with no media; the old 8-frame fallback in `from_instance` is deleted) |

**Attack timing.** A new optional manifest field `release_frame: Option<u32>` is not added; the release frame is
frame 0 of the slice. Veo attack clips are generated to open on the strike (the reviewer checks this in the Veo
plan's review sheet; §7 F-5). With the release tick
`R = presented_tick − elapsed + damage_point`, the offset is `(−R · 1024) rem_euclid period`, computed in `i64`, so
the phase `(now + offset) mod period` is 0 at the release tick. Sustained fire at a constant period gives the same offset on every swing:
consecutive swings change no lane, and the material is not touched during a steady fight.

**Free loops reset on change.** A component `FlipbookPhase { action: AnimState, since: Tick }` on each live bubble
(inserted in the spawn bundle, mutated in place) records the presented tick at which the displayed action began.
For a loop the lanes are canonicalised to `origin = 0`, `offset = (−since · 1024) mod period`, so they are constant
while the action lasts. A unit that starts walking shows the walk clip's first frame, the anchor picture all its
states share (seamless-loops plan D-3).

`since` is observed per rendered frame. When every tick is presented (one tick or less per frame: speed 1 at any
frame rate above 25 Hz, and every capture) it is exact. When a frame runs several ticks it is late by at most the
ticks skipped (at speed 4 and 60 frames per second, one tick: 10 ms of real time). Attack and Constructing never
depend on it: their lanes come from the snapshot alone.

Rejected: publishing a "since" tick from `sim_core` (presentation bookkeeping in the authoritative state and the
checksum for a 10 ms gain); one global phase per identity with no reset (the walk would start mid-stride, and the
brief's reset test would not hold).

### D-6. Play speed is the clip's own speed, with frame blending

- **Period of a slice:** `duration_ms` from the manifest when present (WP3), else `frame_count / fps` as today.
  Converted once per packing to clock units: `duration_ms · 1024 / 40`.
- **Why the clip's own speed:** a 4 s Veo idle loop played in 0.67 s (§2.2) reads as a frantic flicker; real speed
  matches the motion the prompt described. Ten frames over 4 s is 2.5 frames per second, which steps visibly, so:
- **Frame blending:** the shader samples the current and the next layer of the slice (the next of the last frame
  in a loop is the first) and mixes them linearly by the sub-frame fraction. Hold and fixed modes do not blend. The
  seamless-loops repair makes the wrap a short step, so blending across it is a fade, not a cut. Blending is a new
  feature flag `FLAG_FRAME_BLEND` (bit 10) in `DEFAULT_FEATURE_FLAGS`, with a Lab toggle like the other optics, so
  the two can be compared on the same clip. Cost: three more texture samples per covered pixel (WP7 measures it).
- Legacy atlases without `duration_ms` keep `frame_count / fps`; nothing is re-baked by this plan.

### D-7. The snapshot contract

- `BubbleRenderInstance::anim_progress: f32` is removed: it never carried data, and its meaning (a float phase)
  now belongs to the client. In its place:

  ```rust
  /// The attack cycle this entity is in, from the start of its current swing until its weapon is ready again.
  pub attack_swing: Option<AttackSwing>,

  pub struct AttackSwing {
      /// Ticks since the swing began (0 on the tick its windup started).
      pub elapsed_ticks: u16,
      /// Ticks from the swing's start to the release of its shot (the resolved damage point).
      pub damage_point_ticks: u16,
      /// Ticks from one swing's start to the earliest next swing (the resolved attack period).
      pub period_ticks: u16,
  }
  ```

  All integers, so the record stays free of float presentation state. `Some` in `Windup`
  (`elapsed = damage_point − remaining`), in `Backswing` and in `Cooldown` while the cooldown runs
  (`elapsed = period − weapon_cooldown_remaining`, clamped below `period`), `None` in `Ready` and
  `RotatingToTarget`.
- The weapon is resolved by one new function, `SimWorld::resolved_weapon(entity, arch, target_mover)`, extracted
  from `advance_weapon_cycles` (range and period after upgrades and buffs) and called from both places, so the two
  can never disagree. The target mover is the swing's bound target in `Windup` and `Backswing`; in `Cooldown` the
  phase holds no target, so it is `current_target`'s mover, else ground. A unit whose ground and air weapons have
  different timings may show a slightly wrong cadence in cooldown against the other layer; WP1 lists the catalog
  identities this affects (**verify**: the count may be zero).
- `Entity::anim_progress` is removed from `sim_core` with a checksum schema bump (WP2). It is dead state today, and
  the charter forbids placeholders.

### D-8. Ownership and order

- `update_flipbook_clock_system` (new, `client/src/flipbook_clock.rs`) is the only writer of `FlipbookClock`. It
  runs in `BattleSet::PreparePresentation` before `sync_live_battle_system` in both registrations (`lib.rs:417` and
  `lib.rs:856`), and reads `LabState::current_tab`, `BattleSession` and `Time`.
- `FlipbookPhase` is written only by `sync_live_battle_system` (and Bevy's `animate_targets`, which holds write
  access to every component; **verify** against the test's output).
- `system_ownership_tests.rs` gains two rows: "Flipbook clock" (resource) and "Flipbook phases" (component).
- `BattleFrameState` stores `presented_fraction` and exposes `presented_instant() -> (Tick, f32)`; it is written
  where `present` already runs, by `advance_battle_ticks`, so the "Authoritative world" row is unchanged.

### D-9. One clock for every scene

The Lab preview, the swarm and the inspector portrait use the same lanes and the same Rust `flipbook_layer`, on the
`Time`-based clock of D-1. `swarm_frame_index`, `refresh_swarm_material`'s frame write, the Lab's
`(elapsed · 1.5).fract()` and the portrait's own frame arithmetic are deleted. The portrait shows the selected
state's slice, not the whole atlas. The swarm's two shared materials then change only when a wall bounce flips the
heading, so its steady-state test tightens from "at most one modification per shared material per frame" to the
heading flips counted in the window.

---

## 4. Work packages

Every package ends with `cargo test --workspace --locked`, `cargo clippy --workspace --all-targets -- -D warnings`
and `cargo fmt --all -- --check`, in `CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/<worktree>`. GPU
tests run through the existing harness; ad hoc GPU runs use `with-gpu-lock` per command (charter §7). Each package
writes its tests first and records the red run in its commit message.

### WP1: `AttackSwing` in the snapshot (sim, no checksum change)

Files: `sim_types/src/snapshot.rs`, `sim_core/src/world/snapshot.rs`, `sim_core/src/combat_resolution.rs`
(extraction of `resolved_weapon`), mechanical updates of every `BubbleRenderInstance` literal (client sources and
tests listed by `grep -rn "anim_progress" crates`), `sim_types/tests/snapshot_serialization.rs`, new
`sim_core/tests/attack_swing_snapshot_tests.rs`.

1. Tests first, red because the field does not exist (compile failure is the red run, plus the behaviour asserts):
   - a single attacker against a dummy: on the windup's first tick `elapsed = 0`; on the release tick (the tick the
     target loses life) `elapsed = damage_point`; one tick before the next windup `elapsed = period − 1`; `None`
     once `Ready` with no target;
   - stim (the catalog's overclock capability, found by capability, not by name) shortens `period_ticks` to the
     value combat uses on the same tick (both read `resolved_weapon`);
   - a stutter step (`Move` during backswing) publishes `Walk` and still `Some` in cooldown;
   - the world checksum of a 1 000-tick run is unchanged by this package (the snapshot is not part of it).
2. Implement D-7's snapshot part. The client keeps today's picture: `from_instance` uses frame `start_frame` where
   it read `anim_progress` (it was always 0). `BubbleUniformGpu::anim_progress` stays until WP4.
3. List the identities with both ground and air weapons of different timings in the commit message.

### WP2: `Entity::anim_progress` removed, schema 10 (lane B slot)

Files: `sim_core/src/entity.rs`, `sim_core/src/checksum/encoding.rs`, `sim_core/tests/checksum_state_tests.rs`,
`sim_core/tests/fixtures/golden_replay.ron` and `golden_replay_orders.ron` (`checksum_schema: 10`, checksums
re-recorded), and whatever else pins schema 9 (**verify** with `grep -rn "checksum_schema\|SCHEMA" crates/sim_core`).

1. Red run: bump the schema constant alone and watch the golden tests fail on the schema; then remove the field and
   re-record. The commit says why every checksum moved: an encoded constant-zero field was removed, no behaviour
   changed. As evidence, the 1 000-tick snapshot sequences (positions, life, events) before and after are compared
   byte for byte in a temporary test, and the comparison is quoted in the commit.
2. Scheduling: the combat plan's lane B serialises golden edits (`2026-11-04-combat-sc2-alignment.md` §1). This
   package takes the next free slot and tells the lane's owner; it has no data dependency on any lane B package.

### WP3: `duration_ms` in the manifest (asset_baker)

Files: `asset_baker/src/manifest.rs`, `ffmpeg_importer.rs` (`pack_action` and the per-clip decode result),
`asset_baker/tests/` (manifest round trip, bake integrity).

1. Tests first: a manifest without `duration_ms` parses and re-serialises byte for byte (committed atlases keep
   their provenance hashes); one with it round-trips; a fixture clip of known length (made by ffmpeg in the test,
   as the existing ffmpeg tests do) bakes with `duration_ms` within one source frame of its length, for both loop
   and held sampling; a still-image bake writes `None`.
2. `#[serde(default, skip_serializing_if = "Option::is_none")] pub duration_ms: Option<u32>`, the source clip's
   duration in milliseconds, measured from the decoded frame count at the decode rate.
3. Starts after the seamless-loops plan's WP2 has merged, because both edit the decode path of
   `ffmpeg_importer.rs`.

### WP4: The GPU flipbook clock (client)

Files: new `client/src/flipbook_clock.rs` (`FlipbookClock`, `FlipbookClockGpu`, `FLIPBOOK_CLOCK_BUFFER`,
`flipbook_layer`, `FlipbookLanes`, the upload system), `bubble_shader.rs` (layout D-4, `BubbleDomeMaterial::new`,
storage binding, `renders_same_as`, `FLAG_FRAME_BLEND`), `assets/shaders/bubble_dome.wgsl`, `lib.rs` (plugin
wiring, 176 in `headless_verification`), `round2_visual_tests.rs`, `bubble_optics_tests.rs`,
`headless_tests.rs`, `dome_fill_tests.rs`, new `client/tests/flipbook_clock_gpu_tests.rs`.

1. Unit tests first (`flipbook_clock.rs`): `flipbook_layer` for each mode at `e = 0`, at every frame boundary
   `k · period / count`, one unit before it, at `period − 1`, after the wrap of `e`, and with
   `period · count` at its limit; the canonical loop offset gives the same layers as `origin = since`; `bytemuck`
   and `encase` agree on `FlipbookClockGpu` and on the 176-byte uniform (as the vfx test does).
2. GPU parity test (red until the shader is changed: today's shader ignores the clock): an atlas fixture whose
   layers are solid, distinct colours; a material with `LOOP` lanes; for 40 clock values covering every boundary,
   render one frame, read back the bubble's centre pixel, identify the layer, and compare with `flipbook_layer`.
   With blending off the match is exact; with blending on, the colour is the expected mix within 2/255. Runs
   through `support/gpu_run.rs` like the other in-process GPU tests.
3. Shader-IR test: the fragment reads `flipbook_clock.now`, `anim_origin`, `anim_period`, `frame_count`,
   `anim_offset`, `anim_mode`, `frame_base`.
4. Implement D-2, D-3, D-4 and D-6's blending. `from_instance` takes the lanes as an argument (computed by WP5);
   until WP5, callers pass `FlipbookLanes::fixed(start_frame)`, which renders as today.

### WP5: The battle animates (client)

Files: `battle_pipeline/timeline.rs` (`presented_fraction`, `presented_instant`), `flipbook_clock.rs`
(`update_flipbook_clock_system`, the displayed-action rule and lane packing of D-5), `scene_controller.rs`
(`FlipbookPhase` in the spawn bundle, in-place update in `update_live_instance`), `lib.rs` (system order),
`system_ownership_tests.rs` (two rows), `sync_steady_state_tests.rs`, `sync_allocation_count_tests.rs`, new
`client/tests/battle_flipbook_tests.rs`.

Tests first; each is red on `main` (the probe of §2.6 is the first one's red run) unless marked guard:

| Test | Asserts |
|---|---|
| `idle_unit_frames_advance` | on the skirmish, a worker left idle for 4 s of game time shows every layer of its Idle slice in order (layer = `flipbook_layer(clock, lanes)`), each for `period / count` within one tick, and its material is modified 0 times in the window |
| `state_change_resets_the_loop` | a unit ordered to move shows its Walk slice's first layer on the first presented tick of `Walk`, and the second layer `period / count` later |
| `attack_release_shows_the_release_frame` | on the default battle after the armies meet, for every swing of one attacker whose identity has an Attack slice: at its release tick (the target's life drops) the layer is the slice's frame 0; one tick before it, the slice's last frame (when the period is longer than `count` ticks); through cooldown the displayed action stays `Attack` |
| `sustained_fire_changes_no_material` | over 10 consecutive swings at a constant period the attacker's material is modified 0 times (constant offset, D-5) |
| `construction_follows_progress` | a foundation's layer is `start + ⌊progress · count⌋` on every tick, and stays put while no worker builds |
| `paused_frames_freeze_the_clock` (guard) | 100 paused frames leave `FlipbookClock` and every lane unchanged; the existing paused test stays green |
| `speed_scales_the_clock` | at speed 2, 1 s of real (manual) time advances the clock by 50 ticks |
| `frame_grouping_does_not_change_attack` | the same 300 ticks run at 1 and at 5 ticks per frame give identical Attack and Constructing lanes at every common tick, and Idle and Walk `since` values that differ by at most 4 ticks |
| `capture_frames_are_reproducible` | two runs with `ManualDuration(ZERO)` stepping one tick per frame (the capture tool's regime) give identical `(clock, lanes)` sequences |

Then the existing churn proofs, updated:

- `sync_steady_state_tests.rs`: `redrawn` packs lanes from the presented record plus the bubble's `FlipbookPhase`,
  through the same production function, so "modified ≤ redrawn" keeps its meaning. A new window
  `running_idle_army_modifies_no_bubble_material`: a resumed skirmish whose units are all idle runs 200 frames with
  0 `BubbleDomeMaterial` modifications and 0 `ShaderStorageBuffer` events, while the clock advances.
- `sync_allocation_count_tests.rs`: `update_flipbook_clock_system` joins the live child's systems; the difference
  must still equal the baseline's. Add a running variant of the live child (resumed, not paused) if the harness
  allows it within the same valgrind budget (**verify** run time; if it is over 4 minutes, run it in background).
- `retained_buffers` is unchanged: `FlipbookPhase` is a component, not a buffer.

### WP6: Lab, swarm and portrait on the same clock (client)

Files: `scene_controller.rs` (`preview_instance`, `update_preview_actor`, `swarm_instance`,
`sync_swarm_stress_system`; `swarm_frame_index` deleted), `lab_ui.rs` (portrait, Lab toggle for frame blending),
`smog/mod.rs` (material constructor), `sync_steady_state_tests.rs` (`swarm_stress_frames…` tightened, D-9),
`lab_scene_tests.rs`.

Tests first: the Lab's selected hero plays the selected state's slice at its D-6 period (red: today 0.67 s for all);
the portrait shows only that slice; the swarm's shared materials are modified only on heading flips.

### WP7: Measurement and bookkeeping

1. **Frame budget.** Build `frame_budget` in release at `main` before WP4 and at the merged WP6, and run scenario
   400 through headless KWin (`--display-path kwin-virtual`) on the RX 580, five interleaved runs each, exactly as
   `docs/evidence/2026-11-03-frame-budget.md` "How each run was taken" (one `with-gpu-lock` per run, pinned cores,
   load recorded). Report p50, p95 and p99 of the frame interval and the main-update time, and add one attribution
   run (`--render-diagnostics`) each for the GPU pass time of the bubble pass, which is where blending's extra
   samples land. Expected: main-update unchanged within noise (the per-bubble packing gains a few integer
   operations, no allocation); GPU bubble pass up by less than 0.3 ms. If p50 moves by more than 3 %, attribute it
   before closing, and if blending is the cause, record the numbers for Q0020 review and keep it on (the CPU, not
   the GPU, bounds this client: conclusion 2 of that evidence).
2. **Capture A/B.** Run `battle_capture` twice on the same script on the RX 580 and compare the PNGs byte for byte
   per checkpoint; then once on `lavapipe` and compare the layer chosen per bubble (read from the manifest the tool
   writes, not from pixels).
3. Evidence in `docs/evidence/2026-11-xx-battle-flipbook-animation.md`; the plan index row updated; F-1 in the
   seamless-loops plan marked fixed with the commits; `docs/` architecture text that describes `anim_progress`
   (**verify**: `grep -rn "anim_progress" docs/*.md DESIGN.md`) updated.

---

## 5. How the brief's questions are answered

| Question | Answer |
|---|---|
| Where the phase lives | Attack and Constructing: derived from the snapshot every frame, no memory. Idle, Walk, Cast: `FlipbookPhase { action, since }` on the live bubble entity, which `RenderEntityMap` keys by `EntityId`; reset when the displayed action changes (D-5) |
| How it advances | Presented simulation ticks plus the interpolation fraction, as a `u32` in 1/1024 tick, in one GPU buffer written in place (D-1, D-3) |
| Pause | Freezes; speed scales (D-1) |
| Replay and capture determinism | Integer clock from ticks; integer frame function on GPU and CPU; captures step one tick per frame, where `since` is exact (D-1, D-5); tested in WP5 and WP7 |
| Per-state rates from the manifest | `fps` and `frame_count` today; `duration_ms` per slice when the bake writes it (D-6, WP3) |
| Looping and one-shot | Manifest `looping` and `hold_last_frame` honoured for free actions; Attack is a loop of one weapon period aligned to the release; Constructing follows build progress; Die is never published (D-5, §7 F-2) |
| Attack synced to the damage point | Yes: the release tick shows the slice's frame 0, from `AttackSwing` (D-5, D-7) |
| `BubbleUniformGpu`, `renders_same_as` and churn | The ignored `anim_progress` lane is gone; all new lanes are compared because the shader reads them; they change only with the action, so time alone never modifies a material (D-2, D-4) |
| `sim_core`'s `anim_progress` | Removed (dead state, WP2). The attack phase comes from the snapshot, derived from the weapon state machine with no new state (D-7) |

---

## 6. Risks

| Risk | Consequence | Handling |
|---|---|---|
| The storage binding fails on a backend the tests use | material never prepared | the GPU parity test runs on `lavapipe` (CI) and radv; storage buffers in fragment shaders are core Vulkan |
| The weak-handle asset is dropped | materials retry forever, nothing draws | D-3 verify step; the GPU test catches it |
| Two writers of `FlipbookPhase` reported by the ownership test | test red | add Bevy's `animate_targets` to the row, as the Camera row does |
| A swing's resolved weapon differs from the one combat used | Attack cadence off by a tick | one function for both (D-7); WP1 test compares them on the same tick |
| Blending ghosts on fast motion | visible double image on Walk | Lab toggle; Q0020 lets the agent turn it off per decision; WP7 records a capture with and without |
| Lane B is long | WP2 waits | WP2 blocks nothing: WP1 already stops reading the field |

---

## 7. Findings not fixed here

- **F-2. No Die animation in battle.** `Die` is never published: a dead entity leaves the snapshot on the tick it
  dies and its shatter is a combat event (`published_actions.rs`). Playing a Die clip would need the client to keep
  a departed bubble alive for the clip's length. A visual question for a later plan.
- **F-3. Ten frames per four-second clip.** The 16 384-pixel strip at 512-pixel frames leaves 10 frames per unit
  state (2.5 per second of source). Blending (D-6) hides the steps; a smaller frame size or a texture array that is
  not a single strip would keep more frames. For the Veo plan.
- **F-4. Shader jitter on Bevy time.** `bubble_dome.wgsl` jitters critically damaged bubbles with `globals.time`,
  Bevy's virtual time: it moves while the battle is paused and freezes in captures. It could read the flipbook clock
  instead. Small; noted for the next shader change.
- **F-5. Armed structures never publish `Attack`.** Locomotion skips immobile actors, so a turret structure stays
  `Idle` while firing. D-5's rule shows `Attack` for it whenever its atlas has an `Attack` slice and a swing is
  under way, which is the intended look. Release-frame placement for Veo attack clips (frame 0 is the strike) is a
  review criterion for the Veo plan's batch.

---

## 8. Owner questions

None. Every open decision here is visual (play speed, pause behaviour, blending, attack alignment), which Q0020
gives to the agent, or engineering inside existing processes (the checksum schema bump follows the golden fixture's
own rule: "a simulation change that alters behaviour updates this list in the commit that explains why").
