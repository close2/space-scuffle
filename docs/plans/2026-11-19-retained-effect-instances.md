# Blueprint: Retained Effect Instances (B-137)

B-137 (`DESIGN.md` §4): with the shipped splash looks, a stream of 200 splashes a second adds about 3.6 to 4.1 ms
of main-world update to the 400-unit frame-budget battle. Plan
[`2026-11-09-effect-visualization`](2026-11-09-effect-visualization.md) §10.3 asks that the plan's stress stay
within +5 % (p50) of the frame-budget baseline, and Amendment 33 asks that this baseline be re-recorded first. Today
the stress meets that bar only against the 2026-11-03 baseline, which is 27 % slower than today's client.

Amendment 34 profiled the cost
([`frame_budget_b137.md`](../evidence/effects/review/frame_budget_b137.md), branch `effects-polish-b137`). The cost
is `EffectRuntime::build_frame` rebuilding every live effect instance on the CPU every frame, and it has no single
cause. The amendment proposed a design change, which this plan makes:

- lasting marks are kept in a retained decal table and aged on the GPU;
- layers whose time is over are skipped;
- per-frame constants are taken out of the instance loop;
- the spawn path's whole-pool scans are replaced;
- the baseline is re-recorded and the stress judged with a measurement that holds up on this loaded, shared host.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-19 against `main` at
`e6307be1` and branch `effects-polish-b137` at `98c08807` (Amendments 33 and 34: B-120 and B-121 fixed, B-137
profiled), which merges before this plan's first work package. A full plan is required because the change adds a
subsystem (a retained GPU decal table aged by a compute pass, the project's first compute shader), changes how
every effect frame is built, and spans `client` and `client_diagnostics`. Line numbers drift, so search for the
quoted symbol. Claims not confirmed while planning are marked **verify**.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0002: scoping is the agent's call if written down | Standing rule | §10, D-13 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §6 |
| Q0080: up to five independent review rounds for effect looks | Not triggered: no look changes (D-10). An independent review of the identity evidence is still held (WP8) | D-10, WP8 |
| Q0083: radv for performance and review captures | Standing rule | §8, D-10 |
| Q0089 (open: queued attackers) | Unrelated (simulation steering) | — |
| None new | — | Every choice has a default recorded as a decision |

---

## 1. Summary

| | Today | After this plan |
|---|---|---|
| Lasting decal (the ~3 300 of the S arm) | Placed, clipped, hashed, curve-sampled and written by the CPU every frame (about 1 650 instructions each) | Written once into a retained row when it is born or its effect changes; aged on the GPU by `decal_age.wgsl` every frame |
| Decal draw | `decal.wgsl` reads a `DecalInstance` buffer the CPU rewrote | `decal.wgsl` **unchanged**, reading the `DecalInstance` buffer the compute pass wrote, in the same order |
| Ended layers of a live effect (fire of an effect kept alive by its marks) | Every instance placed, then refused by `layer_clock` | Skipped before placement (D-3) |
| Camera `tan(fov/2)`, a line's clip axis and seen runs, a decal's along-line yaw | Recomputed per instance per frame | Once per frame, or once per effect when it changes (D-3) |
| Spawn path (`merge_line`, `retire_oldest_mark`, `retire_oldest_fire`, `retire_one`) | Scans of all 6 144 pool slots | Bounded indices (D-4) |
| Cards (sprites, rings, ribbons, particles) | CPU every frame | CPU every frame, with D-3's savings and a cheaper sort; retained cards are out of scope (§10) |
| Pictures | — | WP2 to WP5: bit-identical to today on radv. WP6 on: within D-10's tolerance, checked in the same run against the CPU evaluator on both drivers |
| Simulation, goldens, checksums, catalog data, media | — | Untouched |

---

## 2. Background

### 2.1 What the frame budget measured

[`frame_budget_wp14.md`](../evidence/effects/review/frame_budget_wp14.md) (radv, 400 units, load 4 to 14) has the
following figures.

| Arm | p50 frame interval | Main-world update p50 |
|---|---:|---:|
| B (shipped client) | 11.86 ms | 9.59 ms |
| P (§10.3 stress: 200 splashes/s, 20 areas, 50 held looks) | 14.97 ms | 12.84 ms |
| S over B (splashes alone), two pairs | | +3.6 and +4.1 ms |

The frame interval follows the main world. The effect passes cost +0.47 to +0.57 ms of GPU time, far under §10.3's
1.5 ms. A +5 % bar on B's 11.86 ms leaves **about 0.59 ms** for everything the stress adds to a frame.

### 2.2 How other engines keep many marks cheap

Engines with many lasting decals keep them resident and let the GPU age them. Examples are Unreal's deferred
decals, which fade in the material by a per-instance start time and lifetime, and the GPU particle systems of
Unreal (Niagara GPU sims) and Unity (VFX Graph). In both, the CPU writes a record when a decal is born and frees it
when it dies, and per-frame work on the CPU is proportional to the changes, not to the population. This plan
applies that pattern to the one block whose population is large and whose per-frame change is a pure function of
time: the `Decal` block. Cards stay on the CPU (D-13): they need a back-to-front sort against the other
transparent items, which the renderer cuts into runs on the CPU (`render.rs`, `cut_runs`).

---

## 3. Diagnosis (confirmed and refined while planning)

### 3.1 Method

The B-137 offline bench (`~/scratch/effects-polish-b137/b137_bench.rs`) was rebuilt at `e6307be1` with one
addition: an environment switch that removes, in memory after loading, either every decal layer (`nodecals`) or
every other layer (`decalsonly`) from the shipped vocabulary. Effect lifetimes are resolved at load and stay as they
were, so `nodecals` keeps exactly as many effects alive. The copy is at
`~/scratch/retained-decals-plan/b137_bench_variants.rs`. The runs were release builds pinned to one CPU
(`taskset -c 41`), using the S arm's stream at 200 splashes a second with 1/60 s frames. Each run was counted with
`perf stat -e instructions:u,task-clock`, and its per-symbol self time was taken with `perf record`. The
one-minute load was 55 to 72 throughout.

### 3.2 Results

**Wall time is unusable under this load, and instruction counts are not.** Two runs of the same binary on the same
stream measured 3.42 and 1.87 ms per `build_frame`, but retired 24 316 138 356 and 24 316 132 330 user instructions:
the same count to within 3 parts in 10⁷. This sets the measurement method (D-14).

Marginal instructions per frame (the 2 400-frame run less the 1 200-frame run, divided by 1 200):

| Variant | Instructions per frame | Live effects | Cards | Decals |
|---|---:|---:|---:|---:|
| Shipped | **10.07 M** | 758 | 1 790 | 3 309 |
| `nodecals` (decal layers removed) | 4.62 M | 758 | 1 789 | 0 |
| Decal share | **5.45 M (54 %)**, about 1 650 per decal per frame | | | |

In time on a pinned core, `nodecals` builds a frame in 0.83 to 1.03 ms against 1.87 to 3.42 ms shipped. The decal
share in time is 56 to 70 %, larger than the instruction share because a decal touches more memory per
instruction (the 144-byte record, the instance copy).

Self time by symbol (shipped / `nodecals`, % of samples):

| Symbol | Shipped | `nodecals` | Notes |
|---|---:|---:|---|
| `write_layers` (inlined per-instance evaluation) | 28.4 | 23.1 | |
| `placement::place` | 14.9 | 8.5 | Called **before** `layer_clock` refuses an ended layer's instance (`write_layer`) |
| `seen_clip_window` | 9.3 | 2.3 | Walks the line's seen bits outward from each instance's point, per instance per frame |
| `build_frame` (inlined `settle_ledger`/`held_marks`, `fire_bursts`, the slot scans) | 9.2 | 15.7 | `held_marks` re-counts every decal layer's placements for every effect every frame |
| `push_card` | 4.7 | 9.2 | |
| `tanf` | 4.5 | 3.5 | `projected_px` and `CameraView::sees` recompute `tan(fov/2)` per call |
| `atan2f` | 4.4 | — | A decal's `AlongLine` yaw, constant per effect, per instance per frame |
| `spawn` | 4.1 | 7.8 | `merge_line`'s scan of all 6 144 slots per line request; `retire_oldest_*` scans |
| `memmove` | 4.2 | 6.5 | `EffectInstance` copies (`slot_mut(..).copied()`, `*instance`), `sorted.extend` of 160-byte cards |
| `roundf`, `floorf` | 3.2 | 2.1 | `placed_seen`, `seen_clip_window`, `instance_count` |
| Card sort (`sort_unstable_by` on indices with an indirect depth compare) | 4.1 | 7.6 | |
| `retire_one` (fold over the pool) | 1.4 | 2.0 | Runs on every frame whose detail overflowed |

### 3.3 Findings

1. **The decal block is half the work, and its per-frame change is a pure function of time.** For a decal of a
   live effect, every field of its `DecalInstance` falls into one of three groups:
   - fields fixed by the effect's state: `center`, `yaw`, `grazing`, `half_extents.y`, the width factor, a
     variant's layers, `ramp_row`, the clip, every liquid, outline and breakup field, `darken`, the base size and
     its jitter, and the strength;
   - fields that are functions of the layer's life: the intensity, size, erosion and ramp curves, and the flipbook
     frame fitted to the life (`flipbook_frame(.., None, 0.0, life, true)`);
   - fields that are functions of time since one of three events: detail retirement (`retire_fade`), mark
     retirement (`mark_fade`) and release (`exit`).

   The LOD test (`lod_skips(camera.pixels(extent * 2, position))`) is the only per-frame camera dependency. So the
   CPU work per decal per frame (placement, the clip-window walk, the hashes, `atan2`, `Surface::of`) recomputes
   values that change only when the effect's state changes.
2. **Ended layers cost placements.** An effect lives until its last mark ends (6 to 8 s for the scorch, blast mark
   and acid stain). For all that time its fire layers (0.5 to 2 s) are walked instance by instance:
   `instance_count`, `placed_seen`, `placed_seed` and `place` (with `sin`/`cos` for discs and circles) run before
   `layer_clock` returns `None`. Amendment 34 measured skipping them: identical frames, 6 to 10 % saved.
3. **Per-frame constants are recomputed per instance.** These are `tan(fov/2)`; `line_clip_window` (a
   normalisation); the seen run around each point (a bit walk); and the along-line yaw (`atan2`).
4. **The spawn path scans the whole pool.** The pool grew from 2 048 to 6 144 slots in WP14 (Amendment 31). The
   `// PERFORMANCE RATIONALE:` blocks of `merge_line`, `retire_oldest_fire` and `retire_oldest_mark` still cite
   "2 048 slots, a few microseconds". At 200 splashes a second, each line request and each mark retirement walks
   6 144 slots. `retire_one` does too, on every overflowing frame.
5. **`held_marks` recounts every frame** what the ledger could keep incrementally: the decal slots of each effect's
   unended decal layers.
6. **Card sorting and copying** cost about 8 % of the shipped profile. They sort indices with an indirect float
   compare, then copy 1 790 cards of 160 bytes into `sorted`.
7. **No allocation appears in the profile** (`clear_page_rep` at 0.3 % is the bench's start-up). The retained
   vectors work as designed (`the_same_stream_builds_the_same_frames_and_reuses_every_buffer`).
8. **A second, render-side cost** (Amendment 34 §2.1) is the kernel re-validating buffers in VRAM at submission.
   The per-frame decal upload is about 3 300 × 144 B = 475 KB a frame, plus 286 KB of cards. Retained decals reduce
   the upload to patches (D-12), which should relieve it. It is measured (WP7), not promised.

**Expected effect.** Retained decals remove up to 5.45 M of 10.07 M instructions a frame. D-3 and D-4 remove part
of the remaining 4.62 M (findings 2 to 6 account for about 45 % of the `nodecals` profile). That points to roughly
2.5 to 3.5 M instructions a frame, about 0.5 to 0.7 ms on a quiet core. This is near the 0.59 ms budget, not
safely under it. The plan therefore measures after each step and has a decision gate (D-13, WP7).

---

## 4. Design decisions

### D-1. Presentation only; nothing hashed changes

`sim_types`, `sim_map` and `sim_core` are untouched, as are every golden replay, checksum, matrix result, catalog
file and media file. The work lives in `client::effects` and `client_diagnostics`. The C2 boundary is unaffected:
no simulation crate gains a dependency. Each WP's gate shows `golden_replay_tests` untouched, because no tracked
simulation path changes and the gate catalog selects no simulation suite.

### D-2. A decal is a static record plus a pure function of time and camera

A decal instance is split into:

- a **`DecalRecord`**: everything fixed by its effect's state (§3.3 item 1, first group), plus:
  - its time inputs: the effect's birth as two `f32` (`birth_hi`, `birth_lo`, D-8), the layer's
    `start + placed.delay` as one `f32` offset, the span, and whether it is held;
  - its curve ids: intensity, size, erosion, ramp;
  - its flipbook element (first layer, frames; a variant's chosen layer is resolved at write time);
  - its priority;
  - the event times of its effect: detail retired, marks retired, released, and the layer's `exit`;
- **`age_decal(record, now, camera) -> Option<DecalInstance>`**: the function that today's `write_instance` and
  `push_decal` compute from those inputs, written once in Rust (`effects::decal_table::age`) and once in WGSL
  (`assets/shaders/effects/decal_age.wgsl`).

The Rust function is the reference. The WGSL function is a port, bound to it by D-10's parity test.

The picture is a pure function of (the table, `now`, the camera). The table is a pure function of the event stream
and `Time<Virtual>`, as the effect frame is today (runtime module docs, "Layers keep no per-instance state").

### D-3. Exact CPU wins first, bit-identical by construction

Each of these leaves every byte of every `EffectFrame` unchanged. The proof is WP1's frame digest, equal at the base
and the head on the same host, plus unit tests per change.

- **Skip ended layers before placement.** A non-held layer whose `start + span + max_line_delay` (the bound used by
  `held_marks`) is behind the effect's age has no instance with a clock. Skip it in `write_layers` and
  `fire_bursts`. A held layer is never skipped this way.
- **`CameraView` caches `half_tan = tan(fov_y * 0.5)`** at construction (`camera_view`, tests, the lab, the bench).
  `pixels` and `sees` use it. `projected_px` keeps its signature for its tests and gains
  `projected_px_with(size, distance, half_tan, viewport_px)`, which `projected_px` calls. Both evaluate the same
  expression with the same operands, so the bits are equal.
- **Per-effect line constants.** On spawn, and whenever `line` or `seen` changes (`merge_line`,
  `update_held_line`), compute a `LineFrame` stored in the `EffectInstance`:
  - `line_clip_window` (axis, start, end);
  - the along-line yaw (`atan2` once);
  - the **seen runs**: for each of the line's at most 64 points, the low and high ends of its seen run, from the
    `seen` mask with bit operations (`trailing_ones`/`leading_ones` on the shifted mask), held as two `u64`
    boundary masks.

  `seen_clip_window` becomes a lookup of the instance's point. It keeps its signature as the reference, and a unit
  test compares the two over every point of 10 000 random masks and line lengths, bit for bit.
- **`held_marks` kept by the ledger.** An effect's held decal slots change only when one of its decal layers ends
  or is released. `EffectInstance` gains `marks_next_change: f64`, the earliest such end. `settle_ledger` recounts
  an effect only when `now` passes it, and otherwise adds the cached count. A unit test compares the cached ledger
  with the recount every frame of the WP1 stream.
- **No `EffectInstance` copies in the hot loops.** `write_layers` takes `&EffectInstance` through a split borrow
  of `pool` and `frame` instead of `slot_mut(..).copied()`. The `LayerContext` already borrows it.
- **Card sort on keys.** Sort `(depth_key: u32, index: u32)` pairs, where `depth_key` is the order-preserving
  integer map of the `f32` depth, descending. Then write `sorted` by index. `total_cmp` order equals the integer
  order of that map, and `sort_unstable` of distinct `(key, index)` pairs is a total order. The old indirect
  `sort_unstable_by` breaks equal-depth ties arbitrarily, so the new sort keeps the frame identical only when depths
  are distinct.

  **Verify** with the digest. If the WP1 stream has equal depths whose order differs, adopt the `(key, index)`
  order as the definition. That is a change of tie order, visible only between two cards at exactly the same
  depth. Record it, and re-record the digest in the same commit with the count of affected frames.

### D-4. Bounded indices for the spawn path

- **Recent lines.** `merge_line` only joins an effect younger than its merge window (`max_line_delay +
  LINE_MERGE_SLACK`, at most about 1 s). Keep a fixed-capacity ring of handles of live `Line` effects inside their
  window. Its capacity is the pool size, but in practice it holds the last second's lines. Prune it from the front
  each frame and scan it instead of the pool. Order of the scan: slot ascending, as today (the ring is scanned in
  slot order by sorting its at most few-hundred handles in place, or by keeping it sorted on insert), so the first
  match is the same effect.
- **Oldest burning, oldest marks, oldest detail.** Use three fixed-capacity binary min-heaps of
  `(birth, slot, generation)`, preallocated at the pool's capacity, with lazy deletion. An entry whose generation
  is stale, or whose condition no longer holds, is popped and dropped. The order key `(birth, slot)` reproduces
  `min_by(birth.total_cmp)` over slots ascending, which returns the first minimum (lowest slot) on ties.
  `retirement_candidate`'s own rule (class order, then birth) is kept by keying its heap on `(class rank, birth,
  slot)`. **Verify** against `budget::retirement_candidate`'s tie rule while implementing, and test that heap
  choice equals the scan's choice over the WP1 stream's every call.
- The `// PERFORMANCE RATIONALE:` blocks of the replaced scans are rewritten for the indices: Why, What, Trade-off
  (C11).

### D-5. The decal table and GPU aging

**Main world (`effects::decal_table`, owned by `EffectRuntime`):**

- **Rows.** `rows: Vec<DecalRecord>` at `budget.decals` capacity, plus a free list. Per pool slot, the first row
  of its chain, and per row, the next row. All are allocated at construction.

  An effect takes rows when it is placed. The rows are its decal layers' seen placed instances, and never more than
  the ledger granted it (`decal_slots`). The ledger already reserves whole or nothing, so `rows ≤ reserved ≤
  capacity`. A failed row allocation is a defect, counted in `EffectStats::critical_overflow` and tested never to
  happen under the stress streams.
- **Order key.** Each row has
  `key = (priority, pool slot, layer index, placement index, instance index)` packed in a `u64`. `order: Vec<u32>`
  holds live rows sorted by key. This is exactly the order `write_layers` emits decals today: the Critical pass over
  slots ascending, then the Detail pass. New rows are appended, and the list is re-sorted with `sort_unstable` (keys
  are unique) only on frames that added or removed rows.
- **Dirty effects.** Every mutation of an effect's decal inputs goes through the runtime API and marks its slot
  dirty in a fixed bitset plus a fixed-capacity list:
  - `spawn`, `hold`, `merge_line`, `release`, `retire`;
  - `update_held`, `update_held_line`, `move_anchor` and `set_fog_clip`, only when the value actually changes;
  - detail retirement, mark retirement, `free_ended`, `clear` and `reset`;
  - a change of `only`.

  `pool` becomes private with a read-only accessor (every external use today is read-only, §5), so no path
  mutates an instance without the mark. Once a frame, `build_frame` rewrites the rows of each dirty effect. It frees
  the old chain and writes new rows with the WP4 record writer, which runs the code `push_decal` runs today, split at
  the time boundary. Each written row goes into the frame's **patch list**: `(row, record)` pairs, unique per row.
- **Row ends.** A row is freed when its layer can no longer draw. For a non-held layer that is `birth + start +
  span + max_line_delay`, the bound `held_marks` uses. For a held layer it is release plus `exit`. Rows are also
  freed when the effect's decal slots are given back, or when the effect is freed. A fixed-capacity min-heap of
  `(free_at, row, generation)` replaces a scan.

**Render world (`effects::render`):**

- **Buffers**, created once per vocabulary at `budget.decals` capacity:
  - `decal_rows` (storage, `COPY_DST`);
  - `decal_order` (storage `u32`);
  - `decal_patches` (storage, capacity `budget.decals`);
  - the existing `effect_decals` `DecalInstance` buffer, which gains `STORAGE` write use for the compute pass;
  - `effect_curves`, a storage buffer of every catalog curve's keys `(t, value, easing)` with per-curve offsets,
    written once per vocabulary;
  - a uniform `DecalAgeUniform { now_hi, now_lo, camera position, forward, half_tan, viewport_px, min_screen_px,
    only }`.
- **Extraction** copies the frame's patch list and, when it changed, the order list into retained render-world
  vectors. Both are preallocated, so extraction allocates nothing. Extraction also copies the uniform's inputs.
- **Compute node** `EffectDecalAgeNode` runs in the `Core3d` graph after the prepass and before the main opaque pass
  (**verify** the 0.15 label names, `Node3d::Prepass` and `Node3d::MainOpaquePass`, and that the node runs only
  for the view the effects draw in). It dispatches two entry points:
  1. `apply_patches`: thread `i` copies `patches[i].record` to `rows[patches[i].row]`. Rows are unique, so the order
     of writes does not matter;
  2. `age`: thread `i` reads `rows[order[i]]`, runs `age_decal`, and writes `DecalInstance` `i`. A row that
     `age_decal` hides (not started, ended, eroded out, below the LOD threshold, filtered by `only`) is written with
     `half_extents = 0`, a degenerate box that rasterises nothing.
- **Draw.** `queue_effects` draws `0..order.len()` decals with the unchanged `decal.wgsl`, in both the darkening
  and the ordinary pipeline. `darkening_decals` becomes "a live row darkens", counted on row writes and frees.
- **Full resync.** The render world records the generation of the last patch list it applied. When a generation
  was missed, or `EffectGpu::rebuild` made new buffers (a new vocabulary), extraction uploads every live row and the
  whole order list instead of the patches. A render world that misses a frame therefore never draws stale rows.

**Why compute and not the vertex shader.** Ageing in `decal.wgsl`'s vertex stage would avoid the graph node but
would change the shader the pictures depend on. The fragment would read 36 aged fields from flat varyings instead
of the storage record, and each decal would be aged 36 times in each of two pipelines. With a compute pass the
drawing shader is byte-for-byte the shader of today's captures. The aged buffer is a real buffer that a test reads
back (D-10). Aging runs once per decal.

### D-6. Fog visibility per viewer

Nothing about fog moves to the GPU. Today a decal's visibility to the viewer is decided by three inputs, all of them
effect state the CPU owns:

- admission where the event lands (the event is not spawned at all otherwise);
- the line's `seen` mask (`placed_seen` drops instances at unseen points; `seen_clip_window` cuts the rest at the
  fog's cell edge);
- the effect's `fog_clip`, which areas and beams refresh every frame through `set_fog_clip`.

All three are inputs of the record writer, and any change of them marks the effect dirty (D-5), so its rows are
rewritten in the same frame. The GPU never sees an unseen instance (no row is written for it) and draws every row
with the clip the CPU wrote. A decal the viewer cannot see stays hidden on exactly the frames it is hidden today.
The test `a_line_whose_first_points_are_fogged_shows_where_it_is_seen` and the fog subjects of WP1's identity scene
pin it.

The viewer is fixed for a session. The battle pipeline admits events per viewer, and an effect keeps its admission
for its life, as today. **Verify** that no path changes the viewer of a live `EffectRuntime` (none was found:
`EffectRuntime` is made at catalog load, and `reset` runs only in the lab). If one is added, it must `clear` the
runtime, as a vocabulary reload does.

### D-7. Sorting and transparency order

Decals are one `Transparent3d` item at `f32::MIN`, before every bubble, card and smog item. Within the item, decals
blend in instance order: premultiplied `over`, so overlapping marks are order-dependent. The order list (D-5)
reproduces today's emission order exactly, so overlapping marks blend in the same sequence. The darkening pipeline
draws the same range and keeps only `darken` decals in its vertex stage, as today. Hidden rows sit in the order but
draw nothing, which equals their absence from today's compacted list. Card order, runs and items are unchanged.

### D-8. Time: pausing, scrubbing, epoch precision

- The effects run on `Time<Virtual>`. The client has no effect pause: a battle at speed 0 freezes the battle clock,
  not `Time<Virtual>`. It also has no replay scrubbing (searched: no `pause()`, `set_relative_speed`, `advance_*`
  on `Time<Virtual>` in `client` or `client_diagnostics`). The lab and the GPU tests step time manually.

  Because aging is a pure function of `now`, any `now` sequence draws what today's renderer draws for it. This
  includes a jump forward, a frozen `now` (each frame redraws the same picture), and time running backwards after
  `reset`, which restarts the sequence and clears the table. A future pause or scrub needs nothing from this design.
  Particles, which integrate, are untouched and keep today's behaviour.
- **Precision.** Today the age is `((now - birth) as f32) - layer.start - delay`, with `now` and `birth` in `f64`.
  WGSL has no `f64` without an optional feature. Both times are therefore split into `hi = now as f32` and
  `lo = (now - hi as f64) as f32`, and the shader computes `((now_hi - birth_hi) + (now_lo - birth_lo)) -
  offset`.

  The first difference is exact by Sterbenz's lemma whenever `birth` is within a factor of two of `now`, which
  holds for every row after the first seconds of a session. The `lo` terms are tiny. So the sum rounds to the same
  `f32` as the CPU's cast in all but rare one-ulp cases. The Rust reference `age_decal` uses the same split, so CPU
  and GPU agree. The table never needs an epoch rebase.

### D-9. Determinism of presentation

The table, its order and its patches are functions of the event stream, the API calls and `now`, built with
fixed-capacity structures and no hashing of addresses. Its heaps break ties by slot, and its sorts use unique keys.
The aged buffer is a function of the table, `now` and the camera. On one driver the compute shader's float results
are deterministic run to run, so a capture is the same on every run of the same driver, as plan §5.6 requires. The
WP1 identity scene checks this by rendering it twice.

### D-10. Visual identity and its tolerance

- **WP2 to WP5 (CPU only): bit-identical.**
  - The frame digest (WP1) of every frame of the S, P and identity streams equals the base's on the same host.
  - The radv captures of the identity scene equal the base's exactly (0 levels on every pixel).
  - Same bytes uploaded, same shaders, same driver.
- **WP6 on (GPU aging): within tolerance, judged in the same run.** The differences come only from the GPU's float
  arithmetic:
  - `+`, `-` and `*` are correctly rounded in WGSL, but the driver may contract them into fused multiply-adds;
  - WGSL division is accurate to 2.5 ulp;
  - `tan` and `atan2` are not used: `half_tan` and yaw arrive precomputed.

  Three checks apply.
  1. **Record parity** (`decal_age_gpu_tests.rs`, both drivers). Over the identity scene's frames, read back the
     aged `DecalInstance` buffer and compare it with the Rust `age_decal` of the same rows, `now` and camera:
     - integer fields equal;
     - float fields within 4 ulp or 1e-6 absolute;
     - a flipbook frame index that differs by one only where its `frame_mix` sits within 1e-5 of 0 or 1 (the
       blend is then the same);
     - visibility equal, except a row within 1e-4 px of the LOD threshold or 1e-5 s of a start or end, counted and
       at most 1 per 10 000 row-frames.
  2. **Pixel identity in the same run** (same test file). Render the same frames twice in one child, once with the
     compute pass and once with `EffectDecalSource::Cpu`, which uploads the Rust `age_decal` output into the same
     `DecalInstance` buffer and skips the compute dispatch. Every capture must have:
     - ≥ 99.9 % of pixels bit-identical;
     - every pixel within 2/255;
     - mean absolute difference ≤ 0.02/255;
     - or a failing frame explained by a counted boundary flip from check 1, printed with its row.

     The CPU source is a diagnostic mode chosen explicitly by tests and `frame_budget --decal-source cpu`. It is
     not a fallback: production never selects it, and a missing compute pipeline is an error that the renderer
     reports, as a missing card pipeline is.
  3. **Against today's captures** (evidence, radv). The identity scene captured at the base commit (WP1) is
     compared with the head after WP6 under check 2's numbers. A contact sheet of base, head and an amplified
     difference goes into the evidence.
- The 21 effect golden stills (`effect_slice_gpu_tests`, ≥ 99 % within 8/255) stay as they are and must still
  pass. None is re-recorded.

### D-11. Zero allocation in steady state

Every new structure is allocated once, when the runtime or the render resources are made at the vocabulary's
budget:

- the rows, the chains, the free lists, the order, the patch list, the dirty bitset and list, the four heaps and
  the recent-lines ring;
- in the render world, the extracted patches and order;
- on the GPU, the new buffers and the bind groups, made in `rebuild`.

No buffer, bind group or asset is created per frame. `EffectFrame::buffers` gains `DecalTable::buffers` (address
and capacity of every retained vector), and `sync_steady_state_tests` checks them over its effect window.
`sync_allocation_count_tests` (valgrind) keeps counting the live battle sync with the splash stream. Its effect
window must stay at zero allocations; this is unchanged as a rule and newly exercised for the table's paths.

### D-12. Upload volume

A steady frame uploads the patch list (the frame's new and changed rows, a few dozen at 200 splashes a second), the
order list only when it changed (up to 6 144 × 4 B), and the uniform. Today it uploads every decal (about 475 KB).
WP7 measures the effect on Amendment 34's kernel validation cost with `--render-diagnostics` and `perf`, as an
observation, not a bar.

### D-13. Cards stay CPU-built; the decision gate

Cards get D-3's savings. A retained card table would need to keep the back-to-front order against the bubbles and
the smog, which `cut_runs` decides per frame on the CPU, and cards are mostly short-lived fire. That is a separate
design (§10).

**Decision gate after WP7.** If the stress misses the bar (D-14), WP7 reports the remaining cost per phase from the
bench (instructions and `perf` self time). The next step comes from this ranked list, recorded as an amendment with
its own short note or plan:

1. **Parallel card build.** Evaluate slots in fixed chunks on Bevy's `ComputeTaskPool` into per-chunk retained
   buffers, concatenated in chunk order, so the emitted order is unchanged.
2. **Budget data first (§10.3: "a miss shrinks budgets in data first").** Shorter-lived Detail cards, a lower
   `min_screen_px`. This changes looks, so it needs Q0080 review rounds and the owner's look sign-off.
3. **Retained cards** (a plan of its own).

The gate never ships a partial look change silently.

### D-14. Measurement on a loaded shared host, and how acceptance is judged

The last attempt waited 90 minutes for a load below 15 and never got one (load 30 to 620). Wall time under this
load varies by a factor of two between identical runs (§3.2), while instruction counts do not. The method has three
layers:

1. **Instruments (WP1, committed).**
   - **Bench `effect_frame`** (`crates/client_diagnostics/benches/effect_frame.rs`, `harness = false`). It drives the
     production `EffectRuntime` with the frame-budget stress sources (`stress_sources`, `stress_event`,
     `mix_sources`), using the S stream and the P stream (areas, tethers, held looks and their per-frame updates),
     at 1/60 s frames, 600 warm-up and 2 400 measured, from a fixed seed. It reports:
     - per-frame thread CPU time (`CLOCK_THREAD_CPUTIME_ID`, via the `libc` crate, already in `Cargo.lock` as a transitive
       dependency and added to `client_diagnostics` as a direct one; `std` has no thread clock) of spawn,
       `build_frame` and, from WP6, the table's patch work;
     - live effects, cards, decals and rows;
     - `--digest`: a 64-bit FNV-1a of every frame's uploaded bytes (cards, the CPU decal evaluation, lights,
       distortions, counters), printed per 100 frames and in total.
   - **`frame_budget` records CPU time.** `FrameSample` gains `process_cpu_us`, the process CPU time
     (`CLOCK_PROCESS_CPUTIME_ID`) spent between two `First`s. The report gains its p50 and mean.
   - **`frame_budget_series`** (a new bin in `client_diagnostics`). It runs arms as child `frame_budget` processes
     in a balanced order (B P P B, repeated), each in its own `with-gpu-lock` command under the WP14 recipe (headless
     KWin, CPUs 32 to 47). It records the one-minute load at each run's start and end, and writes a RON series with
     every run's report plus the pair statistics below. Unit tests cover the statistics on fixed inputs.
2. **Per-WP check (binding for WP2 to WP6, load-proof).** For base vs head:
   - `perf stat -e instructions:u` of `effect_frame` S and P, three alternating runs each;
   - the counts agree within 0.1 % run to run, so one comparison is decisive;
   - each WP's evidence records the instructions per frame and the digest. A WP that claims a saving must show it
     in instructions;
   - thread CPU time is reported beside them as a secondary figure.
3. **Acceptance (WP7), against §10.3 and Amendment 33 step 4.**
   - **Re-record the baseline:** B's p50 frame interval from the series. With the CPU-time verdict below, it is
     the B arm's median of the series.
   - **Verdict, wall time, if possible.** Use the series' **quiet pairs**: those whose load stayed ≤ 16 (this user's
     CPU count) at both runs' starts and ends. With ≥ 4 quiet pairs, the verdict is the median over pairs of
     `p50(P) / p50(B)` ≤ 1.05 (and p99 ≤ 1.10), as §10.3 states.
   - **Verdict, CPU time, otherwise.** With fewer than 4 quiet pairs in a series of ≥ 6 pairs, the verdict is
     `median over pairs of (CPU per frame(P) − CPU per frame(B)) ≤ 0.05 × p50(B)`. The difference is in ms; `p50(B)`
     is the session's median B wall p50, or 11.86 ms (WP14) if that is higher.

     This bar is conservative: it charges every added CPU millisecond on any thread as if it were on the main
     world's critical path. It holds up under load because CPU time does not include time spent waiting for a core,
     and the interleaving cancels slow drifts. The wall-time verdict then stays an open confirmation item for a
     quiet window, recorded in B-137's row, not a blocker.
   - The GPU bar (effect passes ≤ 1.5 ms on the RX 580) is read from one `--render-diagnostics` P run. It now
     includes the compute node's time span (**verify** that a custom node can open a `time_span` in 0.15's
     `RenderDiagnosticsPlugin`; if not, the node records GPU timestamps of its own through the same diagnostics).

### D-15. What changes for diagnostics and tests

- `EffectFrame::decals` and `decal_owners` are no longer filled every frame from WP6 on. `EffectRuntime` gains
  `evaluate_decals(now, camera, out)`, which writes the CPU reference decals for tests and tools: the
  `effect_runtime_tests`, the GPU tests that count decals, `frame_budget`'s effect report and the runtime capture's
  `verify`. `EffectFrame::darkening_decals` is replaced by the table's live darkening count.
- `EffectStats::lod_skipped` counts cards only from WP6 on: a decal's LOD is decided on the GPU each frame. The
  field's doc says so. The test `layers_below_the_minimum_screen_size_are_skipped` asserts through `evaluate_decals`
  that the decal is hidden.

---

## 5. File ownership

| WP | Files |
|---|---|
| WP0 | This plan; `docs/plans/README.md` (row); `DESIGN.md` (B-137 row points here) |
| WP1 | `crates/client_diagnostics/benches/effect_frame.rs` (new), `Cargo.toml` (bench entry), `src/frame_budget/{recorder,report}.rs` (`process_cpu_us`), `src/bin/frame_budget_series.rs` and `src/frame_budget/series.rs` (new); tests `crates/client_diagnostics/tests/frame_budget_tests.rs` (series statistics, CPU column); `crates/client/tests/decal_identity_gpu_tests.rs` (new, the identity scene: base captures and run-to-run equality); `crates/gate/suites.ron`; evidence `docs/evidence/effects/review/retained_decals_baseline.md` |
| WP2 | `crates/client/src/effects/{runtime,budget,placement,ledger,pool}.rs`, `effects/systems.rs` (`camera_view`), `effects/lab/driver.rs` (camera construction); tests in those modules and `crates/client/tests/effect_runtime_tests.rs` |
| WP3 | `effects/runtime.rs` (spawn path), new `effects/indices.rs`; unit tests; `effect_runtime_tests.rs` |
| WP4 | New `effects/decal_table.rs` (rows, chains, keys, order, dirty set, heaps, record writer, `age_decal`), `effects/runtime.rs` (decals through the table, `pool` private), `effects/{areas,beams,stance}.rs` (through the API only if a read needs the accessor); `client_diagnostics/src/frame_budget/effect_stress.rs`, `runtime_capture/driver/verify.rs` (accessor); tests `effect_runtime_tests.rs`, `sync_steady_state_tests.rs` |
| WP5 | `effects/runtime.rs` (card sort), unit tests |
| WP6 | `assets/shaders/effects/decal_age.wgsl` (new), `effects/render.rs` (buffers, extraction, node, `EffectDecalSource`), `effects/decal_table.rs` (patches, generation), `effects/mod.rs`; tests `gpu_validation_tests.rs`, `decal_age_gpu_tests.rs` (new), `decal_identity_gpu_tests.rs`, `effect_runtime_tests.rs`, `area_link_gpu_tests.rs`, `stance_gpu_tests.rs`, `status_gpu_tests.rs`, `weapon_battle_gpu_tests.rs`, `weapon_look_tests.rs` (decal reads via `evaluate_decals`), `sync_steady_state_tests.rs`, `sync_allocation_count_tests.rs`; `client_diagnostics` `--decal-source`; `crates/gate/suites.ron` |
| WP7 | Evidence `docs/evidence/effects/review/frame_budget_b137_retained.md`; `docs/evidence/2026-11-19-retained-effect-instances.md` |
| WP8 | `DESIGN.md` (B-137 row, "Effect presentation" row), this plan's amendment, `docs/plans/README.md`, `docs/plans/2026-11-09-effect-visualization.md` (pointer amendment) |

No file of `sim_types`, `sim_map`, `sim_core`, `asset_baker` or `assets/catalog/` is touched.

---

## 6. Work packages (commit order, tests first)

Each WP: tests first (red where they assert new behaviour; for the identity WPs, a test that pins today's output
and stays green), then code, `cargo gate quick` and the WP's own targets, then hand back. A verifier runs
`cargo gate package` (Q0061). Branch `retained-effects`. Size: S ≤ 1 day, M 2 to 3 days, L 4 to 5 days. WPs run one
at a time: each depends on the previous, and two building agents at once are the limit (charter §4).

### WP0: the plan's row (docs)

This plan and its `README.md` row (`Proposed`) are committed with the plan. The B-137 row of `DESIGN.md` is
pointed here after `effects-polish-b137` merges (its row text changes there), with the first implementation commit.
Docs-only `package`.

### WP1: instruments and the base's pictures (M)

Tests first:

- `frame_budget_tests`:
  - the series' pair statistics: balanced order, the quiet-pair filter, the median ratio, the CPU-difference
    verdict, and the "too few quiet pairs" rule, on fixed report fixtures;
  - `process_cpu_us` is present and monotone in a smoke run's rows.
- `decal_identity_gpu_tests.rs` (GPU class, both drivers) builds the **identity scene** in the effect lab's
  production stack: `lab_app` offscreen, a program of several subjects, manual `Time<Virtual>`. The lab's
  `LabProgram` gains a list of extra subjects with their own spawn times, a small data-side extension; **verify**
  whether the lab driver's take model needs a `LabCrowd` variant. The scene has:
  - every shipped effect with a decal layer, found from the catalog and not named in Rust (C5), on a grid;
  - two lines whose first points are fogged;
  - an area cut by a fog clip that changes over the take;
  - a held status released mid-take;
  - a tether between moving anchors (`update_held_line`);
  - a burn moved by `move_anchor`;
  - a decal budget tightened in the test's catalog copy (`catalog_with`), so marks retire;
  - class caps that make details retire;
  - a camera that steps outward so LOD flips decals off;
  - 16 captures over 9 s.

  The child renders the take **twice** and the parent asserts both sets bit-identical (D-9). With
  `SPACE_SCUFFLE_DECAL_IDENTITY_SAVE=<dir>` it also writes the PNGs as the evidence base.

Code:

- `effect_frame` bench (D-14), `process_cpu_us` (recorder and report), `frame_budget_series`;
- the lab extension for the identity scene;
- gate catalog rules: the new GPU test classed `Gpu`, triggered by `effects/**`, `assets/shaders/effects/**` and
  `effects.ron` (C22).

Evidence (`retained_decals_baseline.md`):

- the base's `effect_frame` instructions per frame and digests (S, P), three runs each;
- the identity scene's radv and lavapipe captures at the base, kept under
  `~/scratch/retained-effects/base/`, with a contact sheet committed (JPEG, ≤ 1 MB);
- one `frame_budget_series` session of B, S and P (≥ 6 pairs) with its quiet-pair count, the first numbers of
  record for D-14's method.

### WP2: exact CPU wins (M)

D-3's items except the card sort (WP5).

Tests first:

- seen-run lookup equals `seen_clip_window` over every point of 10 000 random `(seen, length, spacing)`, bit for
  bit;
- cached `half_tan` paths equal `projected_px` and `sees` bit for bit over 1 000 random cameras and points;
- an ended non-held layer of a live effect writes and bursts nothing, and is never placed (a counting test hook on
  `place` under `cfg(test)`);
- the cached ledger equals the recount on every frame of the S and P streams (a `cfg(test)` assertion in
  `settle_ledger`).

Exit:

- `effect_frame` digest equal to WP1's on the same host;
- instructions per frame lower, recorded;
- `decal_identity_gpu_tests` captures bit-identical to WP1's base on radv (compared from the saved base, recorded
  in the evidence).

### WP3: bounded spawn-path indices (S to M)

D-4.

Tests first: over the S and P streams, every `merge_line`, `retire_oldest_fire`, `retire_oldest_mark` and
`retire_one` choice equals the scan's choice. The scan is kept as a `cfg(test)` oracle function in the indices'
test module, not in production. Also: heaps and the ring never grow past their capacity (address and capacity
unchanged across the stream).

Exit: digest equal, instructions lower, the rationale blocks rewritten (C11).

### WP4: the decal table on the CPU (L)

D-5's main-world half, with `age_decal` in Rust. `build_frame` fills `frame.decals` by aging every row in order, so
the upload and the pictures are unchanged.

Tests first:

- **incremental equals from scratch:** after every frame of the S, P and identity streams, a table rebuilt from
  scratch (every effect dirty) holds the same live rows in the same order with the same records;
- **dirty coverage:** for each API that mutates an effect (`spawn`, `hold`, `merge_line`, `update_held`,
  `update_held_line`, `move_anchor`, `set_fog_clip`, `release`, `retire`, detail and mark retirement, `only`), a
  test changes it and asserts that the next frame's decals equal a from-scratch build. One test per API, named for
  it;
- **row bounds:** rows never exceed the ledger's grant, and no row allocation fails over the streams;
- `pool` is private; every reader compiles against the accessor;
- `sync_steady_state_tests` includes `DecalTable::buffers`.

Exit: digest equal (the CPU evaluator emits the same decals in the same order), radv captures bit-identical to
the base, instructions lower (decals now cost aging only).

### WP5: card sort on keys (S)

D-3's last item. Test first: for random depth sets with and without ties, the key sort is a valid back-to-front
order and, for distinct depths, equal to the indirect sort. Exit: digest equal, or (with ties) re-recorded per D-3
with the affected frame count stated. Captures as WP4.

### WP6: GPU aging (L)

D-5's render half, D-8, D-10, D-12.

Tests first:

- `gpu_validation_tests`: `decal_age.wgsl` validates with naga and composes with `common.wgsl`;
- `decal_age_gpu_tests.rs` (GPU class, both drivers): D-10 checks 1 and 2 on the identity scene, and a missed patch
  generation forces a full resync that draws the same pictures;
- `effect_runtime_tests` and the GPU tests that counted `frame.decals` read `evaluate_decals`;
- `sync_steady_state_tests`: the render world's patch and order vectors are retained;
- `sync_allocation_count_tests`: the live sync's effect window still allocates nothing.

Code:

- the WGSL port;
- the curve buffer;
- the node, the buffers, extraction and the full resync;
- `EffectDecalSource`;
- `frame.decals` no longer filled per frame (D-15);
- `frame_budget --decal-source cpu|gpu` for D-10 check 3 and for A/B attribution.

Exit:

- D-10 checks 1 and 2 pass on radv and lavapipe;
- check 3 recorded against WP1's base with a contact sheet;
- `effect_slice_gpu_tests` goldens untouched and green;
- instructions per frame recorded (the main world now ages nothing).

### WP7: measurement of record and the decision gate (M)

- **Series.** One `frame_budget_series` session of B, S and P, ≥ 6 pairs each, judged under D-14 layer 3. The
  quiet-pair count and the verdict path taken (wall or CPU) are stated.
- **GPU passes.** One `--render-diagnostics` P run.
- **VRAM validation.** `perf` of one S run for Amendment 34's kernel validation share (D-12).
- **Baseline.** Re-recorded in the evidence and in plan 2026-11-09's §10.3 pointer amendment (WP8).
- **Bar met:** B-137 is fixed.
- **Bar missed:** the per-phase report and D-13's ranked next step, as an amendment of this plan. B-137 stays open
  with its new figures.

### WP8: independent review and bookkeeping (S)

- **Review.** An independent review of the identity evidence (D-10's sheets and numbers) and of the table's dirty
  coverage, up to five rounds as for looks (Q0080's process, applied to an identity claim).
- **Bookkeeping.** `DESIGN.md` (B-137 row, "Effect presentation" row), plan 2026-11-09's pointer amendment, this
  plan's row (`Done`, or `Partly done` naming the gate's next step), evidence file. C22: every new test file is
  selected by `crates/gate/suites.ron`.

| WP | Size | Gate and exit |
|---|---|---|
| WP0 | — | Docs-only `package` |
| WP1 | M | `package` with GPU (`decal_identity_gpu_tests`); base evidence recorded |
| WP2 | M | `package`; digest equal; captures bit-identical; instructions lower |
| WP3 | S to M | As WP2 |
| WP4 | L | As WP2, plus valgrind (`sync_allocation_count_tests`) |
| WP5 | S | As WP2 (or the recorded tie re-baseline) |
| WP6 | L | `package` with GPU and valgrind; D-10 checks on both drivers |
| WP7 | M | Evidence only; the verdict |
| WP8 | S | `merge`; plan row updated |

---

## 7. Test plan (summary)

| Claim | Test or evidence | Driver independence |
|---|---|---|
| Each CPU step changes no byte of any frame | `effect_frame --digest` equal base vs head, same host (evidence per WP); unit tests per change | Host-local (libm `tanf`/`atan2f` may differ between hosts, so no digest golden is committed) |
| Indices choose as the scans did | Oracle tests over the streams (WP3) | Pure CPU, CI |
| Incremental table equals from scratch; every mutation marks dirty | WP4 tests | Pure CPU, CI |
| GPU aging equals the Rust reference | `decal_age_gpu_tests` record parity | radv and lavapipe |
| Pictures equal within D-10 | `decal_age_gpu_tests` same-run pixel check; evidence vs base captures | radv and lavapipe; base comparison radv |
| Fog: unseen decals stay hidden | Identity scene's fog subjects; `effect_runtime_tests` fog tests | Both |
| Presentation deterministic run to run | `decal_identity_gpu_tests` renders twice | Both |
| Zero allocation in steady state | `sync_steady_state_tests`, `sync_allocation_count_tests` | CI |
| The shaders compile | `gpu_validation_tests` | CI |
| Looks unchanged | `effect_slice_gpu_tests` goldens untouched | Both |
| §10.3 bar | WP7 series | radv, D-14 |

---

## 8. Measurement protocol (WP1 and WP7)

```bash
# Instruction counts, base vs head (no GPU, no lock; pinned, alternating):
for run in 1 2 3; do
  for bin in base head; do
    taskset -c 41 perf stat -x, -e instructions:u,task-clock \
      ~/scratch/retained-effects/$bin/effect_frame --stream s --digest
  done
done

# Frame-budget series (each child takes the GPU lock itself, one run at a time):
GPU_LOCK_TIMEOUT=1200 frame_budget_series --arms B,S,P --pairs 6 \
  --display-path kwin-virtual --cpus 32-47 --out ~/scratch/retained-effects/series-wp7.ron
```

`frame_budget_series` runs each child as `WGPU_BACKEND=vulkan
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json with-gpu-lock frame_budget …`, one lock per run (charter
§7), and never holds the lock between runs. It refuses to run inside `with-gpu-lock`, as the GPU test harness does.

---

## 9. Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| A mutation path misses the dirty mark, so a decal draws stale (wrong place, wrong fog cut) | Medium | High (a fog leak is a fairness bug) | `pool` private; one dirty-coverage test per mutating API; incremental-vs-scratch over three streams; fog subjects in the identity scene |
| WGSL port drifts from the Rust reference | Medium | Medium | Record parity on both drivers on every gate that touches `effects/**` or the shaders; one source of constants (curve table built from the catalog, fades' seconds passed in the uniform) |
| Bevy 0.15 compute node in `Core3d` (labels, per-view run, ordering vs the prepass) differs from the plan | Medium | Medium | **Verify** in WP6's first hour with a spike; the alternative is a `Render`-schedule system with its own command encoder submitted before the graph (**verify**), keeping the same shader and buffers |
| lavapipe on CI is slow or differs in compute | Low | Medium | Compute is core Vulkan; D-10's checks run on lavapipe; the identity scene is sized to stay within the GPU test's 120 s child budget |
| The bar is still missed after WP6 | Medium | Medium | D-13's gate with a ranked next step; the instruction ledger tells where the rest is |
| Equal card depths change order under the key sort | Low | Low | D-3: verified by digest; recorded re-baseline if so |
| The series never gets 4 quiet pairs | High | Low | D-14's CPU-time verdict is defined in advance; the wall series stays open, not blocking |
| Host-local digest hides a difference CI would see | Low | Low | The digest is an identity proof between two binaries on one host; CI carries the structural tests and the GPU parity checks |
| Order list re-sort on busy frames costs more than saved | Low | Low | Sort only on frames that add or remove rows; 6 144 `u64` keys sort in about 50 µs worst case; measured in WP4 |
| The identity scene exceeds the GPU child's 120 s | Medium | Low | Two children (fog half, budget half), each rendering its take twice |

---

## 10. Out of scope (recorded, Q0002)

- **Retained cards** (sprites, rings, ribbons, particles). They need per-frame back-to-front order against other
  transparent items. Reconsidered only through D-13's gate.
- **Parallel card build**, unless the D-13 gate chooses it.
- **Budget or look changes** (§10.3's "shrink budgets in data first"). Not taken here, because looks stay as
  reviewed. The gate may propose it, with Q0080 review.
- **The kernel's VRAM validation** (Amendment 34 §2.1). Measured in WP7, but not a bar: the GPU is shared.
- **GPU-side LOD statistics** (`lod_skipped` for decals). Not counted (D-15). A GPU counter is a later diagnostic if
  needed.
- **`area_sight` cost.** Amendment 31 orders it only if P exceeds S. WP7's series reports P − S, and a material gap
  is registered as its own item.
- **A quiet-machine wall series.** Taken when a quiet window appears. Not a blocker under D-14.
- **Lab and showcase changes** beyond the identity scene's lab extension.
