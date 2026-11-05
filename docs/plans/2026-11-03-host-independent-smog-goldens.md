# Host-independent smog goldens

## Problem

Phase 9 WP7 added `crates/client/tests/smog_golden_tests.rs`: eleven FNV-1a hashes over `f32` bits. They were
recorded on this server (Debian 13, glibc 2.41). On CI (`ubuntu-24.04`, glibc 2.39, run for `61dfff8`, log
`~/scratch/logs/ci-61dfff8-job.log`) five of them fail: `kinematics_showcase_golden`, `kinematics_non_planar_golden`,
`kinematics_spatial_grid_golden`, `kinematics_all_pairs_fallback_golden` and `smog_visual_lab_systems_golden`. The
three fluid goldens, the startup golden and the swarm and live-battle system goldens pass. Main's CI is red.

The goldens exist to prove that refactors (Phase 9 and later) preserve behaviour. Charter §1.2 forbids weakening a
test so that it passes. Charter §3.1 determinism binds the simulation crates, not the client, so the client may use
the host's `libm`. A golden that hashes a host `libm` result therefore pins the host as well as the code.

## Findings

`+`, `-`, `*`, `/`, `sqrt`, `abs`, `clamp`, `round` and `%` are exact in IEEE 754, so they give the same bits on
every host. `powi` with a constant exponent is two multiplications, done by LLVM or by Rust's own
`compiler_builtins`, never by `libm`. Rust does not contract `a * b + c` into an FMA. Every other transcendental
function (`sin`, `cos`, `atan2`, `asin`, `exp`, `powf`) calls the platform `libm`, whose last bit varies between
glibc versions, musl, macOS and Windows.

### Where `libm` reaches a hashed value

| Call | Site | Feeds | Hashed by |
|---|---|---|---|
| `atan2f` | `kinematics.rs` generator (`vel.z.atan2(vel.x)`, lines 214 and 270) and `sync_headings` (line 580) | `heading_angle_rad` only | all four kinematics goldens (`Fnv64::state`); startup, visual-lab, swarm and live goldens through the showcase bubbles' `BubbleInstance` Debug string and `BubbleUniformGpu.heading_angle_rad`/`aim_angle_rad` |
| `sinf`, `cosf` | crowd generator, `count > 10`: jitter (lines 255-256) and direction `phi` (line 269) | initial positions and velocities, so every later value | `kinematics_spatial_grid_golden` (400), `kinematics_all_pairs_fallback_golden` (600), `fluid_mixed_sources_golden` (24), `fluid_crowd_golden` (48) |
| `sinf`, `cosf` | `Quat::from_rotation_y(-heading)` in `sync_bouncing_smog_bubbles_system` (`mod.rs:739`) | showcase `Transform::rotation` | visual-lab golden |
| `asinf`, `atan2f` | `transform.rotation.to_euler(EulerRot::YXZ)` (`mod.rs:702`) | the heading read back into the scratch state | nothing: `sync_headings` overwrites it whenever horizontal speed exceeds `1e-4`, which the showcase always does |
| `sinf` | `bubble_team_color(index)` for `index >= 10` (`kinematics.rs:156-158`) | source tint, so glow and density bytes | fluid mixed and crowd; swarm (ids 5000+), live battle (ids 9000+), visual-lab preview (id 77) |
| `expf` | `advect_velocity` and `advect_density_and_tint` decay factors (`fluid.rs:890, 955, 957`) | every fluid field after the first step | all three fluid goldens and all four system goldens |
| `powf` | `Color::srgb(..).to_linear()` in `showcase_style` (`mod.rs:522`); the noise texel (`mod.rs:453-461`) | showcase bubble material colour; noise image bytes | all four system goldens (the showcase bubbles exist, hidden, in every tab); startup golden (noise) |
| `sinf`, `cosf` | noise texel (`mod.rs:450-465`), quantised to `u8` | noise image bytes | startup golden |

Heading never feeds back into motion. `step_bouncing_kinematics` writes `heading_angle_rad` only in its last stage
and never reads it. The fluid sources take `transform.translation` and `BouncingSmogBubble::velocity`, not the
rotation or the heading.

### Why exactly these five fail on CI

glibc 2.41 replaced several binary32 functions with correctly rounded CORE-MATH versions; `atan2f` changed, while
`sinf`, `cosf`, `expf` and `powf` kept their implementations. This matches every observation:

- Kinematics: every golden hashes `heading_angle_rad` from `atan2f` after collisions, so all four fail.
- Startup, swarm and live battle never step the showcase bubbles (the first frame has `dt = 0`; the other tabs
  hide them), so the only heading they hash is the generator's `atan2f` of ten literal velocities. On those ten
  inputs both glibc versions agree, so these goldens pass.
- Visual lab steps the showcase bubbles, so it hashes `atan2f` of post-collision velocities and the rotation built
  from them, and fails.
- Fluid goldens hash no heading, and the `sinf`/`cosf`/`expf` results they use agree between the two versions.

### Perturbation evidence (this server)

An `LD_PRELOAD` shim (appendix A) moves every result of the named `libm` functions one ulp toward `+inf`. Running the
current test binary under it gives:

| Perturbed | Failing goldens |
|---|---|
| none | none |
| `atan2f` | all four kinematics; startup, visual lab, swarm, live battle |
| `asinf` | none |
| `sinf` (or `cosf`) | kinematics 400 and 600; fluid mixed and crowd; visual lab |
| `sincosf` | none |
| `expf` | all three fluid; all four system |
| `powf` | all four system |
| all functions | all eleven |

The binary imports these symbols dynamically from glibc (`nm -D --undefined-only`), so the shim intercepts every
call. The table confirms the inventory above: no golden is host-independent today, and four more (fluid mixed and
crowd, startup, swarm, live battle) pass on CI only because glibc 2.39 and 2.41 happen to agree on their inputs.

### Prototype (this server, not committed)

A throwaway edit of the test removed `heading_angle_rad` from `Fnv64::state` and added the heading relation
below. It also left out showcase heading, aim and rotation in `hash_smog_app`. Then it ran under the shim:

- The three fluid hashes stayed exactly the committed constants. Heading never reaches the fluid.
- With every function perturbed (`all`), `kinematics_showcase` and `kinematics_non_planar` kept their hashes, and
  the heading relation held. They are host-independent once heading is out of the hash.
- `atan2f` and `asinf` no longer change any hash.
- `sinf` and `cosf` still change kinematics 400 and 600 and fluid 24 and 48 (the crowd generator), and nothing else.
  The visual-lab golden's earlier `sinf` dependence was the rotation alone. The preview's `bubble_team_color(77)`
  absorbs a one-ulp change, but it stays listed as a dependency.
- `all` still changes the fluid, startup and system hashes (`expf`, `powf`).

## Design

Two classes of golden, both bit-exact.

- **Class H, host-independent.** No `libm` result reaches the hash. Values that `libm` computes are either supplied
  as recorded literals or removed from the hash and checked by an exact *relation* that holds on every host (for
  example, "the heading equals this host's `atan2f` of the final velocity, bit for bit").
- **Class L, `libm`-bound.** Production code calls `libm` on the hashed path and a test cannot remove it without
  changing smog output. The golden stays bit-exact and names its functions in its doc comment. A canary test pins
  the exact `libm` results these goldens consume, so a host with a different `libm` fails the canary with a message
  saying so, rather than looking like a behaviour change. Making class L host-independent needs a production change
  and is an owner decision (Q0012 below).

There are no per-host tables and no ignored tests. A tolerance is used only where noted and justified.

### Heading relation (replaces hashing heading bits)

`Fnv64::state` stops hashing `heading_angle_rad`. `kinematics_hash` keeps each bubble's previous heading and asserts
after every step, for every state:

```rust
let speed_xz = (v.x * v.x + v.z * v.z).sqrt();
let expected = if speed_xz > 1e-4 { v.z.atan2(v.x) } else { previous_heading };
assert_eq!(state.heading_angle_rad.to_bits(), expected.to_bits(), "...");
```

The expression is the one in `sync_headings`, so on any host both sides call the same `atan2f` on the same bits.
The relation holds everywhere, and it fails if the heading is synced from a pre-contact velocity, skipped, or kept
when it should change.

### Crowd inputs from a recorded table

New file `crates/client/tests/support/smog_crowd_trig.rs` (included with `#[path]`, module doc required) holds
`pub const CROWD_TRIG_BITS: [[u32; 4]; 600]`: for each `i` in `0..600`, the bits of
`(i as f32 * GOLDEN_RATIO).sin()`, `(i as f32 * PI).sin()`, `(i as f32 * 2.399_963_1).cos()` and
`(i as f32 * 2.399_963_1).sin()`, recorded on this server (glibc 2.41, x86_64). The doc names the host, glibc and
commit. The rows do not depend on `count`, so the 24, 48, 400 and 600 crowds all use one table.

The test file gains `fn crowd_lattice(count, bounds, trig: impl Fn(usize) -> [f32; 4]) -> Vec<BouncingBubbleState>`.
It is `generate_bouncing_bubbles` for `count > 10`, expression for expression, with the four trig calls replaced by
`trig(i)`. Its initial heading is `vel.z.atan2(vel.x)` (the relation makes that value unhashed).

Because the table equals this host's trig, on this server `crowd_lattice(n, table)` is bit-identical to
`generate_bouncing_bubbles(n)`. The four crowd scenarios therefore run exactly the scenario WP7 recorded.

### Test inventory after the change

| Test | Class | Inputs | Hash changes | Extra checks |
|---|---|---|---|---|
| `kinematics_showcase_golden` | H | `initial_bouncing_bubbles` (literal positions and velocities) | heading removed | heading relation |
| `kinematics_non_planar_golden` | H | same, lifted as today | heading removed | heading relation |
| `kinematics_spatial_grid_golden` | H | `crowd_lattice(400, table)` | heading removed | heading relation |
| `kinematics_all_pairs_fallback_golden` | H | `crowd_lattice(600, table)` | heading removed | heading relation |
| `crowd_generator_matches_lattice` (new) | H (relation) | `generate_bouncing_bubbles(n)` against `crowd_lattice(n, host trig)` for `n` in 11, 20, 21, 24, 48, 100, 101, 400, 600 | none (no hash) | index, position, velocity and radius bit-equal; heading relation |
| `crowd_trig_table_matches_host` (new) | tolerance | `CROWD_TRIG_BITS` against this host's four trig values | none | at most 2 ulps apart per entry; reports the largest gap |
| `fluid_showcase_golden` | L: `expf` | unchanged | unchanged | none |
| `fluid_mixed_sources_golden` | L: `expf`, `sinf` (team colour) | `crowd_lattice(24, table)` | unchanged on this server | none |
| `fluid_crowd_golden` | L: `expf`, `sinf` (team colour) | `crowd_lattice(48, table)` | unchanged on this server | none |
| `smog_startup_assets_golden` | L: `expf`, `powf` | unchanged | noise image bytes, showcase heading, aim and rotation removed | showcase relations (not stepped) |
| `smog_noise_texture_golden` (new) | L: `sinf`, `cosf`, `powf` | startup app | the three noise image fields moved here from startup | none |
| `smog_visual_lab_systems_golden` | L: `expf`, `powf`, `sinf` (preview colour) | unchanged | showcase heading, aim and rotation removed | showcase relations (stepped), checked every frame |
| `smog_swarm_systems_golden` | L: `expf`, `powf`, `sinf` | unchanged | showcase heading, aim and rotation removed | showcase relations (not stepped) |
| `smog_live_battle_systems_golden` | L: `expf`, `powf`, `sinf` | unchanged | showcase heading, aim and rotation removed | showcase relations (not stepped) |
| `smog_golden_libm_canary` (new) | canary | see below | its own recorded hash | none |

**Why the table check has a tolerance.** It checks where the table came from, not behaviour. glibc, musl and
macOS `sinf`/`cosf` are faithfully rounded on these arguments, so two hosts differ by at most one ulp. A table of
anything other than the generator's trig would be off by far more. The 2 ulp bound leaves one ulp of headroom
for a less accurate `libm`. The bit-exact pin of the generator comes from `crowd_generator_matches_lattice`.

**Showcase relations** (`check_showcase_headings(app, stepped: bool)`, used by every system golden). For each
`BouncingSmogBubble`, with `initial = initial_bouncing_bubbles(&SmogConfig::default().bounds)` computed on the host:

- `instance.aim_angle_rad` bits equal `initial[index].heading_angle_rad` bits (aim is never updated).
- If `stepped` is false: heading bits equal the initial heading's bits, and `transform.rotation == Quat::IDENTITY`
  bit for bit. Hash `Quat::IDENTITY`'s bits as well, so a rotation change fails the hash.
- If `stepped` is true: the heading relation above, and `transform.rotation`'s four component bits equal those of
  `Quat::from_rotation_y(-instance.heading_angle_rad)`.

`hash_smog_app` hashes a copy of the `BubbleRenderInstance` and of the `BubbleUniformGpu` with `heading_angle_rad`
and `aim_angle_rad` set to `0.0`, and hashes the rotation only through the relation above. All other fields stay
hashed.

**Canary** (`smog_golden_libm_canary`). It hashes the bits of every production `libm` result the class-L goldens
consume and compares the hash with one recorded constant:

- `(-c.velocity_damping * dt).exp()`, `(-c.dissipation_rate * dt).exp()` and
  `(-(1.8 * c.dissipation_rate).max(1.65) * dt).exp()`, written exactly as in `fluid.rs`, for `c` in
  `SmogConfig::default()` and `crowd_config()`, and `dt` in `STEP_DT`, `Duration::from_secs_f32(STEP_DT).as_secs_f32()`
  (the system clock) and `1.0 / 30.0` (prewarm). Also any other `SmogConfig` the system goldens build: check
  `update_smog_field_system`'s `effective_config`.
- `bubble_team_color(i)` for `i` in `10..48`, `77`, `5000..=5030`, `6000`, `9000..9022` and `9100`.
- `Color::srgb(c.x, c.y, c.z).to_linear()` for the ten `SHOWCASE_BUBBLE_PALETTE` colours.

On failure, the message says that this host's `libm` differs from the recording host (glibc 2.41, x86_64), that
the class-L goldens are not comparable on this host, and that the fix is Q0012, not re-recording. The noise
texture's `libm` calls are too many to list. `smog_noise_texture_golden` covers them and names its functions.

### Re-recording and continuity

New constants are needed for every kinematics golden, the four system goldens, the new noise golden and the
canary. The fluid constants must not change. A changed fluid constant means `crowd_lattice` differs from the
generator, so stop and fix the replica.

To show that re-recording hides no WP7 regression, record on the pre-WP7 code. Create a worktree at `9dbf715`, the
commit the WP7 constants were recorded on. Copy in the new test file and `support/smog_crowd_trig.rs`, run, and take
the printed hashes as the constants. Then run on current main: every constant must match unchanged. If the test
file does not compile against `9dbf715`, stop and report; do not adapt the old code.

## Steps

1. Write `support/smog_crowd_trig.rs`. Record the table with a temporary `#[test]` that prints the rows and is
   deleted before committing (no ignored tests, no printer left behind).
2. Change `smog_golden_tests.rs` as in the inventory: drop heading from `Fnv64::state`, add the heading relation,
   `crowd_lattice`, `check_showcase_headings`, the heading/aim/rotation exclusions in `hash_smog_app`, the
   startup/noise split, and the three new tests. Update the module doc: replace "every `f32` is hashed by its raw
   bits" with the two classes, the per-test `libm` list, and how the constants were recorded.
3. Record at `9dbf715` and confirm on main (above).
4. File `docs/questions/open/0014-host-independent-smog-math.md` (below).
5. Record what this plan found in the Phase 9 plan's findings list and in this plan's index row.

No production code changes.

## Mutation red runs

Each must fail the named test, then be reverted. Paste the failing output into the report.

| Mutation | Must fail |
|---|---|
| In `step_bouncing_kinematics`, call `sync_headings` before contact resolution | all four kinematics goldens (heading relation) and `smog_visual_lab_systems_golden` |
| In `resolve_contacts_all_pairs`, iterate `j` from `n - 1` down to `i + 1` | `kinematics_all_pairs_fallback_golden` |
| In `setup_smog_volume_system`, swap `density_image` and `velocity_image` | `smog_startup_assets_golden` |
| In the generator, jitter factor `0.25` to `0.26` | `crowd_generator_matches_lattice` |
| In `sync_bouncing_smog_bubbles_system`, `from_rotation_y(state.heading_angle_rad)` (sign dropped) | `smog_visual_lab_systems_golden` (rotation relation) |
| In `smog_noise_texel`, swap the R and G channels | `smog_noise_texture_golden` |

## Proof of host independence

1. **Perturbation matrix (required, runs here).** Build the test binary and run it under the shim (appendix A)
   with `SMOGGOLD_PERTURB=all`. Every class-H test, `crowd_generator_matches_lattice` and
   `crowd_trig_table_matches_host` must pass: a one-ulp change to every `libm` result leaves them untouched. Then run
   once per function (`atan2f`, `asinf`, `sinf`, `cosf`, `sincosf`, `expf`, `powf`). Each class-L test must fail only
   under functions its doc comment names, and the canary must fail under `expf`, `sinf` and `powf`. Put the
   resulting table in the report next to the "before" table above.
2. **Import check.** `nm -D --undefined-only` on the binary confirms the perturbed functions are dynamic glibc
   imports, so the shim saw every call.
3. **CI is the second host.** glibc 2.39 on `ubuntu-24.04` differs from this server in `atan2f`. The next CI run
   must pass all fifteen tests.
4. **Not available here:** a run on another target. `wasm32-wasip1` and `wasmtime` are installed, but `client`
   depends on Bevy with default features (windowing, audio), which are not expected to build for WASI. The
   blueprint author did not try it. The implementer may try `cargo test -p client --test smog_golden_tests
   --target wasm32-wasip1 --no-run` once, and report the result either way. No musl, macOS or Windows target is
   installed. Never claim a cross-target result that was not run.

## Gate

With `CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/<worktree> RUSTC_WRAPPER=sccache CARGO_INCREMENTAL=0`:

- `cargo test -p client --test smog_golden_tests --locked` and `cargo test -p client --test smog_bouncing_bubbles_tests --locked`
- the perturbation matrix above
- `cargo test --workspace --locked` (includes `workspace_checks`, so C18 sees this plan's row)
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo fmt --all -- --check`
- after merge, a green CI run on `main`; quote its run ID in this plan's index row and set it to `Done`

## Owner question to file (Q0012)

*Should the client's smog math become host-independent?* Class-L goldens (fluid, startup, noise, all system
goldens) depend on the host's `expf`, `powf`, `sinf` and `cosf`. They pass on glibc 2.39 and 2.41 today. A newer
runner image, macOS or Windows could fail them, and the canary would show why. Options:

- (a) Keep as is. Cost: a `libm` change on CI turns these goldens red until (b) is done.
- (b) Route the smog client paths (`fluid.rs` decays, `bubble_team_color`, the noise texel, the crowd generator,
  `sync_headings`) through the pure-Rust `libm` crate, already in `Cargo.lock` as 0.2.16, so no download. Also enable
  `bevy_math`'s `libm` feature for `to_linear` and `from_rotation_y`. Every smog golden then becomes class H and
  the canary and the trig table go away. Cost: a deliberate output change of at most about one ulp per call,
  re-recorded goldens, and a direct dependency. The `bevy_math` feature changes glam math in the whole client.
- Recommendation: (b), as a separate plan after Phase 9, which allows no behaviour change.
- **Default until answered:** (a), with the canary.

## Implementation record (2026-11-03)

Implemented as written, tests only, with these differences:

- **Question number.** Q0012 and Q0013 were taken by other work in flight, so the question is
  [Q0014](../questions/closed/0014-host-independent-smog-math.md). Read Q0014 wherever this plan says Q0012.
- **Showcase relations use the spawn state, not `initial_bouncing_bubbles`.** `setup_smog_volume_system` spawns the
  showcase from `prewarm_smog_field`, which steps the initial bubbles 26 times at 1/30 s, so the spawn heading and
  aim are `atan2f` of the pre-warmed velocity, not the initial heading. `check_showcase_headings` therefore checks:
  unstepped, heading bits equal aim bits equal `atan2f` of the bubble's velocity, and the rotation is
  `Quat::IDENTITY`; stepped, aim bits equal the spawn aim recorded after the startup frame, heading equals `atan2f` of
  the velocity, and the rotation equals `Quat::from_rotation_y(-heading)`. It also asserts every showcase bubble moves
  faster than `1e-4` horizontally (the relation's previous-heading branch would compare against a production value
  read back through `to_euler`), that the instance velocity equals the bubble's, and that the material uniform keeps
  the spawn heading as heading and aim, because the hash zeroes those two uniform fields.
- **All four noise-image fields moved.** Bytes, descriptor, sampler and usage of the noise image are hashed by
  `smog_noise_texture_golden` alone; the startup golden hashes the density and velocity images and the material.
- **`fluid_mixed_sources_golden` and `fluid_crowd_golden` use `crowd_lattice(.., table)`.** Their constants did not
  change, as the plan requires.

Constants: every new constant printed identically at `9dbf715` (this file and `support/smog_crowd_trig.rs` copied in
unchanged, compiled without edits) and on `main` at `3f57fec`. The fluid constants are unchanged. The trig table
equals this host's trig exactly (largest gap 0 ulp).

Perturbation matrix (shim, appendix A), failing tests per perturbed function:

| Perturbed | Failing |
|---|---|
| none | none |
| all | fluid showcase, mixed, crowd; startup; noise; visual lab; swarm; live battle; canary (9 of 15) |
| `atan2f`, `asinf`, `cosf`, `sincosf` | none |
| `sinf` | fluid mixed, fluid crowd, canary |
| `expf` | fluid showcase, mixed, crowd; startup; visual lab; swarm; live battle; canary |
| `powf` | startup; noise; visual lab; swarm; live battle; canary |

The four kinematics goldens, `crowd_generator_matches_lattice` and `crowd_trig_table_matches_host` pass under every
row. Each class-L test fails only under functions its doc comment names (the noise, swarm and live-battle goldens
absorb a one-ulp `sinf` or `cosf` change through `u8` quantisation or colour clamping). `nm -D --undefined-only`
lists `asinf`, `atan2f`, `cosf`, `sincosf`, `sinf` (`GLIBC_2.2.5`), `expf` and `powf` (`GLIBC_2.27`) as dynamic
imports of the test binary.

Second target: `cargo test -p client --test smog_golden_tests --target wasm32-wasip1 --no-run` was tried once and
does not build: `winit` 0.30.13 stops with "The platform you're compiling for is not supported by winit". No
cross-target result exists; CI (glibc 2.39) remains the second host.

Mutation red runs: all six failed the named tests. The heading-sync mutation also fails the startup, swarm and
live-battle goldens through the unstepped showcase relation, because the pre-warm syncs headings too.

## Appendix A: perturbation shim

A built copy is at `~/scratch/smoggold_shim/smoggold_perturb.so` (source beside it). Build it with `gcc -O2 -shared -fPIC -fno-builtin -o ~/scratch/smoggold_perturb.so smoggold_perturb.c -ldl -lm`
(local gcc, nothing downloaded) and run the test binary from `crates/client` with
`LD_PRELOAD=~/scratch/smoggold_perturb.so SMOGGOLD_PERTURB=<all|name,name,...>`.

```c
/* LD_PRELOAD shim: returns each selected libm result moved one ulp toward +inf. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <math.h>
#include <stdlib.h>
#include <string.h>
static int on(const char *n) {
  const char *s = getenv("SMOGGOLD_PERTURB");
  if (!s) return 0;
  if (!strcmp(s, "all")) return 1;
  size_t l = strlen(n);
  for (const char *p = s; (p = strstr(p, n)); p += l)
    if ((p == s || p[-1] == ',') && (p[l] == ',' || p[l] == 0)) return 1;
  return 0;
}
static float pf(float r) { return (isfinite(r) && r != 0.0f) ? nextafterf(r, INFINITY) : r; }
static double pd(double r) { return (isfinite(r) && r != 0.0) ? nextafter(r, INFINITY) : r; }
#define F1(n) float n(float x){static float(*o)(float);if(!o)o=dlsym(RTLD_NEXT,#n);float r=o(x);return on(#n)?pf(r):r;}
#define F2(n) float n(float x,float y){static float(*o)(float,float);if(!o)o=dlsym(RTLD_NEXT,#n);float r=o(x,y);return on(#n)?pf(r):r;}
#define D1(n) double n(double x){static double(*o)(double);if(!o)o=dlsym(RTLD_NEXT,#n);double r=o(x);return on(#n)?pd(r):r;}
#define D2(n) double n(double x,double y){static double(*o)(double,double);if(!o)o=dlsym(RTLD_NEXT,#n);double r=o(x,y);return on(#n)?pd(r):r;}
F1(sinf) F1(cosf) F1(tanf) F1(asinf) F1(acosf) F1(atanf) F1(expf) F1(exp2f) F1(expm1f) F1(logf) F1(log2f) F1(log10f) F1(log1pf) F1(sinhf) F1(coshf) F1(tanhf) F1(cbrtf)
F2(atan2f) F2(powf) F2(hypotf)
D1(sin) D1(cos) D1(tan) D1(asin) D1(acos) D1(atan) D1(exp) D1(exp2) D1(expm1) D1(log) D1(log2) D1(log10) D1(log1p) D1(sinh) D1(cosh) D1(tanh) D1(cbrt)
D2(atan2) D2(pow) D2(hypot)
void sincosf(float x,float*s,float*c){static void(*o)(float,float*,float*);if(!o)o=dlsym(RTLD_NEXT,"sincosf");o(x,s,c);if(on("sincosf")){*s=pf(*s);*c=pf(*c);}}
void sincos(double x,double*s,double*c){static void(*o)(double,double*,double*);if(!o)o=dlsym(RTLD_NEXT,"sincos");o(x,s,c);if(on("sincos")){*s=pd(*s);*c=pd(*c);}}
```
