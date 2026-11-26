# Evidence: a Render-Entity Pool for Units that Come and Go (B-237)

Measurements of plan [`2026-11-29-render-entity-pool`](../plans/2026-11-29-render-entity-pool.md): the base of
record, probe P-0, the red runs, the tests R-1 to R-11 with R-1v and R-8, and the frame-budget A/B of §7.4. Scratch
material (logs, binaries, DHAT profiles, captures and frame-budget reports) is under `~/scratch/render-pool/`; none of
it is committed.

## Base of record (WP0)

- `main` at `0f0608cd` (the broodlings plan merged), merged into the plan's branch at `35f9905a`.
- The broodlings plan's B-19 patch (`~/scratch/broodlings/b19-test.patch`), applied as is to that base: **6
  `Assets<BubbleDomeMaterial>` added and 6 removed** in its 300-tick window (`red_b19.log`), the figure of E-6.

## Probe P-0 (D-2's cost)

A scratch test binary (not committed; its text is `~/scratch/render-pool/p0_marker_swap_probe.rs`) builds a
`MinimalPlugins` app on the single-threaded executor with 16 entities (a parent with one child each), 8 live and 8
parked. After 64 warm frames, every frame parks the oldest live entity
(`remove::<(LiveBattleBubble, StanceTrack, NoAutomaticBatching)>`, `insert((parked marker, Visibility::Hidden))`) and
reuses the oldest parked one (the reverse swap), and a second system gives each live entity a `StanceTrack` and every
other one `NoAutomaticBatching`, as `present_stances` does. Run under `valgrind --tool=dhat --num-callers=256`, the
blocks whose stack passes through the non-inlined window function are counted, as `sync_allocation_count_tests` does:

| Variant | 64 frames | 128 frames |
|---|---:|---:|
| swap (the pool's marker swaps) | 0 | 0 |
| still (no swap) | 0 | 0 |
| planted (swap + one `Vec` and one `env::var_os` a frame) | 128 | 256 |

An archetype move between archetypes already seen allocates nothing; D-2 stands (no stop rule 1).

## Red runs (WP0)

On the WP0 commit `f2aafa12` (today's API), test binaries copied to `~/scratch/render-pool/red_wp0/`:

| Test | Result on WP0 |
|---|---|
| B-19 as is (the patch) | fails: 6 bubble materials added, 6 removed in 300 ticks |
| R-1 (B-19 extended) | fails: the pool kept spawning for 940 ticks (10 × `L_max` = 10 × 94): 49 spawned (stop rule 6's limit, never reached the regime) |
| R-2 (air cycling) | fails: kept spawning for 240 ticks (10 × 24): 66 spawned |
| R-1v (valgrind brood pair) | fails: windows of 188 and 376 ticks after 376 warm ticks; live 86 and 177 blocks, baseline 70 and 144: the synchroniser allocated **17** blocks over 188 ticks beyond the tick driver's |
| Schedule claim of §1 (`bubble_consumers_run_after_the_live_synchroniser`) | passes (a claim about today's code, verified) |

R-9 and R-10 were written with the module they test (WP1); against WP0 they fail to build (no `render_pool`).

## The pool's tests (WP1 to WP3)

R-8 renders the production client offscreen: the battle harness with a renderer (`Harness::boot_on_gpu`; the window
has no surface), the RTS camera pointed at an `OffscreenCanvas`, captures taken with virtual time paused. Captures of
the run of record are kept under `~/scratch/render-pool/final/keep/radv/`.

All on the final tree of the branch, debug builds in `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/render-pool`,
each target run on its own (never a workspace `cargo test`):

| Test | Where | Result |
|---|---|---|
| R-1 (B-19, Broodlings cycling) | `sync_steady_state_tests::broodlings_cycling_create_and_remove_no_bubble_material` | passes: regime reached, window of 3 × 94 ticks with Broodlings arriving and expiring; no asset added or removed; bubble materials modified within the redrawn pairs plus one per arrival; no entity spawned or despawned; no retained buffer moved (the pool's three vectors among them); every arrival reused |
| R-2 (air and ground cycling) | `sync_steady_state_tests::air_units_cycling_create_and_remove_nothing` | passes, same rules |
| R-1v (valgrind brood pair) | `sync_allocation_count_tests::brood_cycle_frames_allocate_nothing` | passes: live 70 and 144 blocks, baseline 70 and 144 (windows of 188 and 376 ticks after 376 warm ticks): **0** blocks beyond the tick driver's; the live child spawned no render entity in its window |
| The existing valgrind pairs | `sync_allocation_count_tests::steady_state_frames_allocate_nothing` | pass: every live pair's difference equals its baseline's (paused 0 and 0, running 3 and 3, effect stream 0 and 0); planted control 64 over 64 frames |
| R-3 (headless, field by field) | `render_pool_tests`, five variants | passes: same frame, after 5 hidden ticks, ground → air, air → ground, dome → sphere |
| R-4 (picking) | `render_pool_tests::a_parked_bubble_is_never_picked_or_selected` | passes |
| R-5 (looks released) | `render_pool_tests::a_departed_units_looks_are_released_and_never_reach_the_reused_bubble` | passes; the beam half waits for H-20 (no shipped unit with media holds a sustained beam) |
| R-6 (beyond the bound) | `render_pool_tests::a_battle_beyond_its_bound_draws_every_unit_and_says_so_once` and the unit tests | passes: every record drawn, `beyond_bound == 3`, one warning |
| R-7 (consumers) | `render_pool_tests::every_consumer_leaves_a_parked_bubble_alone` | passes |
| R-8 (GPU, bit-identical) | `render_pool_gpu_tests::a_reused_render_entity_renders_as_a_fresh_one` | passes on radv (RX 580): the two `after` captures, 1280 × 720, are bit-identical; the reused child reused 2 entities and the fresh child spawned 2; each arrival changed at least 20 % of its footprint from `before` and differs from its decoy's last frame |
| R-9, R-10, R-6 decisions | `client::render_pool` unit tests (22) | pass |
| R-11 | `render_pool_tests` (reset, tab change), `editor_app_tests::escape_after_a_death_retires_the_render_pool` | pass |
| §1's schedule claim | `system_ownership_tests::bubble_consumers_run_after_the_live_synchroniser` | passes |

Two findings while turning R-1 and R-2 green, both recorded in the plan's As built:

- The first WP2 build reset a reused material's shell to the all-zero default (D-5's table). The stance system writes
  every bubble's shell on its first frame to the shell of a unit with no look, whose `driven` lane is set, so each
  reuse cost two modifications: R-1 measured 307 bubble-material modifications against a bound of 301 (295 redrawn
  pairs and 6 arrivals), R-2 24 against 12. The reuse now writes that neutral shell itself, one modification per
  arrival.
- R-2 first cycled its units at the map's centre, where a map object changes its tint as units of either side come
  and go (6 `StandardMaterial` modifications in the window, on an entity that is no bubble's child). The stage now
  cycles each player's unit beside its own base; the modifications that remain are the tethers' team materials, one
  per arrival whose entity changed owner.

## No simulation change

`git diff --stat 0f0608cd..HEAD -- crates/sim_types crates/sim_map crates/sim_core assets/catalog assets/scenarios` is
empty (stop rule 5 holds), so goldens, measured worlds and the checksum schema are untouched by construction. On the
final tree, `sim_core`'s `golden_replay_tests` (the two goldens at every checkpoint) and the world comparison
`sc2_who_wins_matchup_matrix_tests` (every battle of `matchup_matrix.ron` against its recorded outcome): both pass (4 and 4 tests).

## Acceptance runs (§7.3)

Each target on its own, GPU children locking the RX 580 through `support/gpu_run.rs`: all pass on the final tree (`15b1723f` with the WP3 files):
`skirmish_match_tests` (a skirmish won through input alone; 8 passed, 2 ignored children), `battle_fog_tests` (34),
`battle_flipbook_tests` (9), `round2_visual_tests` (22, with D-6's changed assertion), `system_ownership_tests` (4),
`editor_app_tests` (41), `client_diagnostics`' `showcase_tests` (2), `sync_steady_state_tests` (15),
`render_pool_tests` (11), `render_pool_gpu_tests` (1), the 22 `render_pool` unit tests, and
`sync_allocation_count_tests` (13). Logs: `~/scratch/render-pool/final/`.

## Frame budget (§7.4)

**Binaries.** Base: `main` at `0f0608cd` (built from the WP0 commit `f2aafa12`, whose client code is `main`'s), pool:
the final tree's client code (`15b1723f`); `cargo build --release -p client_diagnostics --bin frame_budget --locked`,
copies in `~/scratch/render-pool/bin/`. Both read the same spec and assets (`--spec`, `--source-assets` of the branch's
tree, which the plan does not change). Fixture roots prepared once per binary on lavapipe under Xvfb
(`--warmup-frames 1 --measure-frames 1`).

**Runs.** `~/scratch/render-pool/fb_ab.sh`: scenarios 200 and 400 of `frame_budget.ron`, three interleaved pairs (pair
1 base first, pair 2 pool first, pair 3 base first), each run one `with-gpu-lock` command
(`WGPU_BACKEND=vulkan VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json`, `GPU_LOCK_TIMEOUT=1200`) with
`kwin_wayland --virtual --xwayland` at 1280×720 and the tool inside it, pinned to CPUs 32 to 47 with `taskset`. Before
each run the script waited up to 60 s for those CPUs to fall under 15 % busy; other agents of this user kept them
busier (gate runs and a movement harness), so most runs started without that quiet, and the slice was 38 to 61 % busy
during the runs (the run itself is about 16 %). Reports and logs: `~/scratch/render-pool/fb/`; statistics recomputed
from the reports' whole-window summaries by `~/scratch/render-pool/fb_parse.py`.

| Run | Interval p50 | p95 | p99 | max | Update p50 | Simulation p50 | Slice busy |
|---|---:|---:|---:|---:|---:|---:|---:|
| 200 base 1 | 9.66 | 14.98 | 17.53 | 43.74 | 7.75 | 0.101 | 42.5 % |
| 200 pool 1 | 9.63 | 24.51 | 32.43 | 58.44 | 7.85 | 0.103 | 47.3 % |
| 200 pool 2 | 8.96 | 15.80 | 24.86 | 44.59 | 7.28 | 0.092 | 38.1 % |
| 200 base 2 | 10.27 | 27.46 | 51.22 | 110.34 | 8.55 | 0.112 | 56.4 % |
| 200 base 3 | 10.06 | 15.37 | 17.95 | 27.83 | 8.13 | 0.109 | 42.7 % |
| 200 pool 3 | 10.23 | 18.23 | 31.11 | 131.28 | 8.30 | 0.102 | 44.4 % |
| 400 base 1 | 16.91 | 38.33 | 64.78 | 176.02 | 12.60 | 0.237 | 55.2 % |
| 400 pool 1 | 12.56 | 25.41 | 29.09 | 63.40 | 9.76 | 0.160 | 47.9 % |
| 400 pool 2 | 14.21 | 29.35 | 39.51 | 68.69 | 10.48 | 0.171 | 47.1 % |
| 400 base 2 | 21.20 | 67.83 | 102.39 | 206.14 | 17.50 | 2.479 | 61.0 % |
| 400 base 3 | 14.70 | 27.09 | 32.65 | 63.98 | 10.46 | 0.193 | 56.1 % |
| 400 pool 3 | 12.13 | 28.34 | 34.81 | 81.81 | 9.31 | 0.139 | 60.2 % |

Medians of three, ms:

| Units | Arm | Interval p50 | p95 | p99 | max | Update p50 | Simulation p50 |
|---:|---|---:|---:|---:|---:|---:|---:|
| 200 | base | 10.06 | 15.37 | 17.95 | 43.74 | 8.13 | 0.109 |
| 200 | pool | 9.63 | 18.23 | 31.11 | 58.44 | 7.85 | 0.102 |
| 400 | base | 16.91 | 38.33 | 64.78 | 176.02 | 12.60 | 0.237 |
| 400 | pool | 12.56 | 28.34 | 34.81 | 68.69 | 9.76 | 0.160 |

**The bar (§7.4) holds.** Pool minus base, interval p50: −0.43 ms at 200 (bar: the larger of 2 % of 10.06 and the
base's spread 0.61 ms), −4.34 ms at 400 (bar 6.50 ms, the base's spread); main update p50: −0.28 ms (bar 0.80) and
−2.84 ms (bar 7.04). No stop rule 4. The host was loaded throughout, so the differences, including the 400-unit
one in the pool's favour, are within the noise of these runs and are not a saving; the tails moved both ways. The
pool's counters at the end of a run are not reported: the tool's report has no field for them (As built).
