# Blueprint: Host-Independent Smog Math Through the `libm` Crate (Q0014 Option b)

This plan routes every transcendental `f32` function that reaches the smog output through the pure-Rust `libm`
crate, so that every smog golden in `crates/client/tests/smog_golden_tests.rs` becomes host-independent (class H),
and it measures whether that costs anything, because the owner approved the change "unless this becomes a
performance problem".

**Status of this document:** Stage 1 blueprint under charter §6, written on 2026-11-03 (UTC; the file carries the
date its work starts) against `main` at `9e693cf` (open-findings WP1 merged). Line numbers drift; search for the
quoted symbol. A claim about runtime behaviour that could not be confirmed while planning is marked **verify**, and
the first step of its package confirms or refutes it.

**Owner decision.** [Q0014](../questions/closed/0014-host-independent-smog-math.md), answered 2026-11-03: "Agreed
(option b: route client smog math through the `libm` crate as a separate plan after Phase 9) unless this becomes a
performance problem. I do not mind if visuals are marginally different depending on the platform, but I think that
using libm simplifies tests." Phase 9 is done (`docs/plans/README.md`).

**Out of scope:** any change to the simulation crates (they use no floating point); host-independence of client
code outside the smog output (camera, presentation, terrain, combat effects); the smog solver's speed (C-18, plan
`2026-11-04-open-findings-c17-c18-p9-gaps`); CI on macOS or Windows for the client.

---

## 1. Summary

| WP | Title | Size | Depends on |
|---|---|---|---|
| WP1 | Audit and proof harness: list every host-`libm` call on a hashed smog path; build the perturbing interposer; record the baseline cost of each call site | S-M (half a day) | open-findings WP9 merged (or WP8 merged and Q0020 answered "no") |
| WP2 | `client::smog::math` on the `libm` crate; every smog call site routed; goldens re-recorded as class H; crowd trig table and host canary removed; workspace check C22 | M (1 to 1.5 days) | WP1 |
| WP3 | Measurement against the stop rule; records | S-M (half a day, plus about 20 min of locked GPU) | WP2 |

One worktree, one agent, the three packages in order. Total about 2 to 3 days.

**Expected effect.** The class-L goldens (three fluid, four system, the noise golden, and WP6's four
characterisation goldens of the open-findings plan) become class H. `smog_golden_libm_canary`, the recorded crowd
trig table (`crates/client/tests/support/smog_crowd_trig.rs`), `crowd_lattice`, `table_trig`, `host_trig`,
`crowd_trig_table_matches_host` and the heading-relation workarounds go away; headings are hashed directly. A
`libm` change on a CI runner image can no longer turn the smog goldens red. Smog output changes by at most about one
ulp per transcendental call, which the owner accepted.

---

## 2. Current state

### 2.1 The `libm` crate is already locked

`Cargo.lock` holds `libm 0.2.16` (checksum `b6d2cec3…`), pulled in by `num-traits`, `pulp` and `rustybuzz`, and its
sources are in the local registry. A direct dependency `libm = "0.2.16"` in `crates/client/Cargo.toml` therefore
downloads nothing and changes no other locked version (WP2 checks `git diff Cargo.lock` shows only the new edge in
`client`'s dependency list).

Properties that matter here (from `libm-0.2.16/src/math/`): the transcendental functions (`expf`, `sinf`, `cosf`,
`sincosf`, `atan2f`, `powf`) are plain Rust ports of musl's, computed with ordinary IEEE `f32`/`f64` operations, and
Rust never contracts `a * b + c` into a fused multiply-add. The `arch` module replaces only exactly rounded
operations (`sqrt`, `floor`, `ceil`, `rint`, `trunc`, `fabs`, `fma`) with instructions on `x86_64`, `aarch64` and
`wasm32`, and those give the same bits as the software versions. So the crate's results are the same on every target
the client builds for. The crate's results may change between crate versions: the new canary of WP2 pins them, so a
`cargo update` that changes them fails loudly.

### 2.2 Host-`libm` calls on hashed smog paths

From reading `crates/client/src/smog/` at `9e693cf` (WP1 confirms the list with the interposer, §3 D-3):

| Site | Function | Calls | Reaches |
|---|---|---|---|
| `fluid.rs`, `advect_velocity` (`let decay = (-damping * dt).exp()`) | `expf` | 1 per step | every fluid and system golden |
| `fluid.rs`, `advect_density_and_tint` (`decay`, `glow_decay`) | `expf` | 2 per step | every fluid and system golden |
| `kinematics.rs`, `bubble_team_color` (beyond the palette) | `sinf` | 3 per call | fluid goldens with more than 10 sources; battle and preview system goldens |
| `kinematics.rs`, `generate_bouncing_bubbles` (crowd branch: jitter and launch direction) | `sinf`, `cosf` | 4 per bubble, once | crowd fluid golden, kinematics goldens (supplied by the recorded trig table today) |
| `kinematics.rs`, `generate_bouncing_bubbles` and `sync_headings` | `atan2f` | 1 per bubble per step | kinematics goldens (kept out of the hash today and checked by relation) |
| `mod.rs`, `smog_noise_texel` | `sinf`, `cosf`, `powf` | about 30 per texel, 32,768 texels, once at startup | noise golden, startup golden |
| `mod.rs`, bubble team colour `Color::srgb(..).to_linear()` | `powf` (through `bevy_math::ops::powf`) | 3 per bubble colour | startup and system goldens |
| `mod.rs`, showcase transform `Quat::from_rotation_y(-heading)` | `sinf`, `cosf` (glam `sin_cos`) | 1 per bubble per frame | system goldens that hash transforms (**verify** which) |

Not host-dependent, and left alone: `sqrt` (IEEE correctly rounded), `floor`, `ceil`, `round`, `abs`, `clamp`, and
`powi`. `powi` lowers to `llvm.powi`, which is either expanded into multiplications or calls `__powisf2` from Rust's
own `compiler_builtins`, the same code on every target; it never reaches the system `libm`. The interposer of D-3
proves this rather than relying on the argument.

`floor` in `trilinear_indices_and_weights` is a `floorf` call into the host `libm` on the `x86_64` baseline, but
`floorf` is exact; open-findings WP7 removes it anyway.

### 2.3 What the tests do today

`crates/client/tests/smog_golden_tests.rs` (plan `2026-11-03-host-independent-smog-goldens.md`) splits the goldens:

- **Class H** (four kinematics goldens): trig supplied by `CROWD_TRIG_BITS`, headings excluded from the hash and
  checked by `assert_heading_synced` (bit equality to this host's `atan2f`), showcase rotations checked against this
  host's `Quat::from_rotation_y`.
- **Class L** (three fluid, startup, noise, Visual Lab, swarm, live battle; after open-findings WP6 also its four
  characterisation goldens): `libm`-bound, guarded by `smog_golden_libm_canary`, whose message points at Q0014.

After open-findings WP9 (Q0020's default), the fluid and system goldens will have been re-recorded once already for
the quiescence change, still as class L.

---

## 3. Design decisions

### D-1. Order relative to the C-18 chain: after open-findings WP9, in its own commit

The smog solver is being changed by the open-findings plan's chain WP6 → WP7 → WP8 → WP9 (C-18). WP6 records four
characterisation goldens on the unmodified solver; WP7 and WP8 are bit-exact refactors proved by unchanged goldens;
WP9 makes one deliberate change (quiescence, Q0020) and re-records. The options:

| Option | Golden re-records | Proof quality | Disruption | Decision |
|---|---|---|---|---|
| a. Before WP6 | Two (this plan, then WP9) | WP6 to WP8 would be proved against class-H goldens: no gain, the server's glibc does not change during the chain | WP6's goldens recorded after this plan; the open-findings plan's D-3 text ("every class stays as it is") and its "unchanged and checked" list change; this plan would wait on nothing but would delay a chain already under way | Rejected |
| b. Between WP8 and WP9 | Two | Same as c | WP9's re-record would become class H; D-3 and Q0020's text describe class L | Rejected: no benefit over c, more text to amend |
| c. **After WP9, separate commit** | Two (WP9's, then this plan's) | Each deliberate change has its own commit, its own argument and its own list of changed hashes, so a reader can attribute every golden change | None to WP6 to WP8; one dependency line | **Chosen** |
| d. Folded into WP9's re-record commit | One | Worse: one hash change would carry two causes, and D-3's check that the kinematics, noise and canary goldens are unchanged would be lost | WP9's scope grows | Rejected |

Two re-records cost two commits of constants; the attribution is worth more. The C-18 chain also benefits from c:
open-findings WP6's `smog_solver` benchmark and D-6's A/B protocol exist by then, and this plan's WP3 reuses both.

**If Q0020 is answered "no"**, WP9 makes no output change, and this plan depends on WP8 merged instead. **If the
C-18 chain stops** (WP8's stop rule fires), this plan depends on whichever of WP6 to WP8 last merged, and converts
the goldens that exist then. The plan never runs concurrently with WP6 to WP9: they own the same two files
(`fluid.rs`, `smog_golden_tests.rs`).

The open-findings plan is amended accordingly (its §4 dependency note, D-7 ownership row, §6 risk row and §7 Q0014
bullet; WP1 to WP5 untouched).

### D-2. Scope of the routing: a smog-local math module, not `bevy_math`'s `libm` feature

Q0014's option text proposed also enabling `bevy_math`'s `libm` feature so that `Color::to_linear` and
`Quat::from_rotation_y` stop calling the host. That feature is global: with Cargo's feature unification it switches
`bevy_math::ops` and glam (`glam/libm`) to the `libm` crate for the whole client and for Bevy itself (transform
propagation, camera projections, `Vec3::length`, `floor`, `round`, every `sin_cos`), and for `client_diagnostics`.

| Option | Host-independent smog | Blast radius | Performance risk | Decision |
|---|---|---|---|---|
| a. `bevy_math` feature `libm` plus direct `libm` calls in smog | Yes | The whole client and Bevy's own math; every other client test that compares floats might move | glam's `floor`, `round` and `sqrt` all become `libm` calls in every hot loop of the client and Bevy; needs a client-wide measurement | Rejected as the default |
| b. **`client::smog::math`**: a small module over the `libm` crate; smog calls it for every transcendental function, including its own sRGB-to-linear conversion and yaw quaternion | Yes | `crates/client/src/smog/` only | Only the calls of §2.2; the solver's per-voxel loops call none of them | **Chosen** |

Option b reaches the owner's goal ("simplifies tests") with the smallest change, and keeps the rest of the client on
the host's fast `libm`, which nothing pins. It costs two short functions that restate formulas Bevy and glam already
have, so each carries a doc comment naming the original and a unit test that it agrees with it to within 2 ulp on a
sweep (bit equality is not required: the point is that they are not the host's). This is a scoping decision the owner
delegated (Q0002's answer: "Agent is allowed to decide scoping decisions but must document them"); it is recorded
here, in the module's `//!`, and in `DESIGN.md` by WP3. If a later plan wants the whole client host-independent,
option a is the way, and this module then becomes a thin re-export.

The module, `crates/client/src/smog/math.rs`:

```rust
//! Host-independent transcendental functions for the smog presentation.
//! Responsibility, fit and invariants per charter §2; states that every function here
//! returns the same bits on every target because it is the `libm` crate's pure-Rust code,
//! and that smog code must call these, never `f32::sin` and friends (workspace check C22).

/// e^x for an `f32` in any range. Same bits on every host (`libm::expf`).
pub(crate) fn exp(x: f32) -> f32;
pub(crate) fn sin(x: f32) -> f32;           // libm::sinf
pub(crate) fn cos(x: f32) -> f32;           // libm::cosf
pub(crate) fn sin_cos(x: f32) -> (f32, f32); // libm::sincosf
pub(crate) fn atan2(y: f32, x: f32) -> f32; // libm::atan2f
pub(crate) fn powf(x: f32, y: f32) -> f32;  // libm::powf

/// sRGB-encoded colour (each channel in [0, 1]) to linear RGBA, alpha 1, with Bevy 0.15's
/// formula (`bevy_color::Srgba::gamma_function`) evaluated through `powf` above.
pub(crate) fn srgb_to_linear(c: Vec3) -> LinearRgba;

/// Rotation by `angle` radians about +Y, built as glam 0.29's `Quat::from_rotation_y`
/// (`(s, c) = sin_cos(angle / 2)`, `(0, s, 0, c)`) through `sin_cos` above.
pub(crate) fn quat_from_rotation_y(angle: f32) -> Quat;
```

The functions are `#[inline]`. Visibility is `pub(crate)` except where the golden tests need a value
(`bubble_team_color` is already public; the tests need no direct access to `math`, see WP2).

Before writing `srgb_to_linear` and `quat_from_rotation_y`, the implementer reads
`bevy_color-0.15.4/src/srgba.rs` (`gamma_function`) and `glam-0.29.3/src/f32/sse2/quat.rs` (`from_rotation_y`) in the
local registry and copies the expressions and constants exactly, with the source path in the doc comment.

### D-3. Proof of host-independence: a perturbing `libm` interposer

A golden that passes on this host proves nothing about other hosts. The proof that no host-`libm` result reaches a
smog hash is to change every host-`libm` result and see that the hashes do not move:

- `~/scratch/plananswers-libm/perturb.c` (not committed; its source is in this plan's appendix): a shared object
  that defines `expf`, `sinf`, `cosf`, `sincosf`, `atan2f`, `powf`, `tanf`, `logf`, `exp2f`, `log2f`, `acosf`,
  `asinf`, `atanf` and their `double` versions, each calling the real function through `dlsym(RTLD_NEXT, …)` and
  returning `nextafter` of the result away from zero (one ulp). Built with
  `gcc -O2 -shared -fPIC -o perturb.so perturb.c -ldl -lm`.
- Run: `LD_PRELOAD=~/scratch/plananswers-libm/perturb.so <the smog_golden_tests binary> --test-threads 1`. Rust's
  `std` on `x86_64-unknown-linux-gnu` calls `libm.so.6` dynamically, so the interposer replaces every runtime call.
- **Before WP2** (WP1's red run): every class-L golden and the canary fail; the class-H kinematics goldens pass.
  This proves the interposer reaches the calls and that the current class split is right.
- **After WP2**: every smog golden passes under the interposer. Any golden that still fails names a call site the
  audit missed; WP2 is not done until there is none.

Calls that LLVM folds at compile time (constant arguments) never reach the interposer. They are host-independent at
run time but depend on the compiling host's folding; WP1 lists any it finds (`objdump -d` of the test binary shows no
call for a folded site) and routes them through `math` anyway, because `libm::sinf` is not folded.

The interposer is a verification procedure, not a committed test: it needs a C compiler and `LD_PRELOAD`, which CI's
Windows and macOS runners and the GPU test harness do not provide. What stays in the tree is the static check C22
(D-4) and the `libm` crate canary (D-5).

### D-4. Workspace check C22: no `std` transcendental call in smog

A new check in `crates/workspace_checks` (the next free ID; C21 is the last): production files under
`crates/client/src/smog/` other than `math.rs` contain no method call `.sin(`, `.cos(`, `.tan(`, `.sin_cos(`, `.exp(`,
`.exp2(`, `.exp_m1(`, `.ln(`, `.log(`, `.log2(`, `.log10(`, `.ln_1p(`, `.powf(`, `.atan2(`, `.atan(`, `.asin(`,
`.acos(`, `.sinh(`, `.cosh(`, `.tanh(`, `.hypot(`, `.cbrt(`, and no call of `to_linear(`, `from_rotation_y(`,
`from_rotation_x(`, `from_rotation_z(`, `from_axis_angle(`, `from_euler(` (the glam and Bevy entry points that reach
`sin_cos` or `powf`). Comments and `#[cfg(test)]` modules are skipped with the same source scanner the other checks
use (`crates/workspace_checks/src/source.rs`). A finding names the file, line and the call, and says "route it through
`client::smog::math` (Q0014, plan 2026-11-04-libm-smog-math)".

Why a workspace check and not clippy's `disallowed-methods`: clippy reads only the nearest `clippy.toml`, so a ban
would apply to the whole client, where host math is allowed (D-2). The check has its own unit tests in the style of
the existing ones: a fixture string with each banned call is reported, the same text in a comment or test module is
not, and `math.rs` is exempt. It is listed in `crates/workspace_checks/src/lib.rs`'s `//!` check table and in
`AGENTS.md`/`DESIGN.md` wherever the check table is mirrored (**verify** where C21 is listed and list C22 beside it).

### D-5. What the golden file becomes

- Every golden is class H. The `//!` "Two classes of golden" section is replaced by a "Host independence" section:
  every transcendental function on a hashed path is `client::smog::math` (the `libm` crate), checked by C22 and proved
  by D-3's interposer run (named with the commit it was run at).
- **Removed:** `smog_golden_libm_canary` and `GOLDEN_LIBM_CANARY`; `support/smog_crowd_trig.rs`, `CROWD_TRIG_BITS`,
  `table_trig`, `host_trig`, `crowd_lattice`, `crowd_generator_matches_lattice`, `crowd_trig_table_matches_host`,
  `ulp_distance` (if then unused); `expected_heading` and `assert_heading_synced` (headings are hashed instead);
  the showcase-rotation relation check (rotations are hashed instead). Crowd scenarios call
  `generate_bouncing_bubbles` directly.
- **Added:** `smog_libm_crate_canary`: hashes the `libm` crate's results for the exact arguments the goldens consume
  (the canary's current argument list: the three decay factors for both configurations and three `dt`; team colours
  for the listed indices; `srgb_to_linear` of the ten showcase colours; plus `atan2` and `sin_cos` on a fixed sweep).
  Its message: "the `libm` crate's results changed (crate version bump?); re-record the smog goldens in a commit that
  names the version change". This guards the one remaining way the hashes can move without a smog change.
- **Re-recorded, one commit:** every fluid, system, noise and kinematics golden (the kinematics ones change because
  headings and rotations now enter the hash, and the crowd scenarios use `libm` trig instead of glibc's), and
  open-findings WP6's four characterisation goldens. The commit message names Q0014 (b), this plan, the reason
  (host-`libm` results replaced by the `libm` crate's; at most about one ulp per call; headings and rotations now
  hashed), and lists every constant with its old and new value.
- **Must pass unchanged:** the semantic smog tests (`smog_bouncing_bubbles_tests.rs`), `fluid.rs`'s and
  `kinematics.rs`'s unit tests, and every other client test. If one fails, the change is visible after all: stop and
  report.

### D-6. What counts as "a performance problem" (the owner's condition)

Expected cost, from §2.2: the solver calls `exp` three times per step, so its per-voxel loops are untouched; the
other sites run per bubble (at most a few hundred per frame) or once at startup (the noise texture, about a million
calls). Option b of D-2 keeps every other client path on the host's `libm`. The expectation is no measurable change
per frame and a few milliseconds more at startup. WP3 measures it with four instruments:

1. **Per-call micro-benchmark** (`crates/client/benches/smog_math.rs`, `harness = false`, like `smog_solver`):
   ns per call of host `f32::{exp, sin, cos, sin_cos, atan2, powf}` versus `smog::math`, over a fixed argument sweep
   that matches each site's range. Informational: it explains a regression, it does not decide one.
2. **Startup**: wall time of `smog_noise_texture` (the 32³ texture), and of `setup_smog_volume_system`, median of 15
   runs per arm.
3. **Solver**: open-findings WP6's `cargo bench -p client --bench smog_solver`, `--contend 1` and `--contend 8`,
   before and after.
4. **Frame budget**: the open-findings plan's D-6 A/B protocol (two release `frame_budget` binaries, interleaved
   A, B, A, B under headless KWin on the RX 580, one lock per run), at 0 and 400 units, three pairs each.

**Stop rule (the owner's "performance problem").** The change is a performance problem if either holds:

- the frame-budget median of main-update p50 or frame-interval p95 at 400 units is worse for arm B by more than
  2 % **and** by more than the spread (max minus min) of arm A's three runs; or
- startup grows by more than 50 ms at the median.

If it fires, WP3 does not merge WP2's branch. It records the figures in the evidence file and files a question to
the owner with the measurement and two options: accept the cost, or keep Q0014's option (a) (class L with the
canary). If it does not fire, WP2 and WP3 merge.

---

## 4. Work packages

Common rules: one worktree, `CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/libm-smog`, logs under
`~/scratch/logs/libm_smog_*`, scratch files under `~/scratch/plananswers-libm/` (or a `libm_smog_` prefix);
`df -h / ~/scratch` checked first; target directory removed after merge (charter §7). Failing test (or mutation) red
run first, pasted into the agent's report. Every new Rust file begins with `//!`. No `unwrap`; `expect` only with
`invariant:`. No function over 100 lines outside tests (C8). Long commands run in the background and their logs are
read every few minutes. `docs/questions/open/` is checked before each push. Gate before each commit:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/libm-smog
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked    # plain: the GPU harness takes the lock per child; never wrap it
```

### WP1: Audit and proof harness

**Files:** none committed except the evidence file `docs/evidence/<date>-libm-smog-math.md` (started here).

1. Confirm the start condition (D-1): open-findings WP9 merged on `main` (or WP8 and Q0020 = no). If not, stop.
2. Confirm `libm 0.2.16` in `Cargo.lock` and in the local registry
   (`~/.cargo/registry/src/index.crates.io-*/libm-0.2.16`). Nothing is downloaded by this plan.
3. Re-run §2.2's audit on the current tree: `grep` `crates/client/src/smog/` for the method list of D-4, and follow
   every call into `bevy`/`glam` that can reach `sin_cos`, `powf`, `exp` (§2.2's last two rows). Record the table in
   the evidence file, with which goldens each site reaches.
4. Build the interposer (D-3, appendix) in `~/scratch/plananswers-libm/`, build the golden test binary
   (`cargo test -p client --test smog_golden_tests --no-run --locked`, take the path from the output), run it with
   and without `LD_PRELOAD`. Record which tests fail under the interposer. Expected: every class-L golden and the
   canary fail, every class-H golden passes. If a class-H golden fails, the current classification is wrong: report
   it before WP2 (it is still fixed by WP2, but the evidence must say so).
5. Record the baseline of D-6 instruments 1 to 3 on `main` (instrument 1 needs the bench of WP2; run it on `main`
   with the bench file copied in, uncommitted, or record instrument 1 for both arms in WP3).

### WP2: Route smog math through `libm`; goldens become class H; C22

**Files (owned):** `crates/client/Cargo.toml` (one dependency, one `[[bench]]`), new
`crates/client/src/smog/math.rs`, `crates/client/src/smog/{mod.rs, fluid.rs, kinematics.rs}` (call sites only),
`crates/client/tests/smog_golden_tests.rs`, deleted `crates/client/tests/support/smog_crowd_trig.rs`, new
`crates/client/benches/smog_math.rs`, `crates/workspace_checks/src/*` (C22) and its tests, the check-table mirrors
(D-4).

1. **C22 first, red.** Add the check and its unit tests. Run `cargo test --locked -p workspace_checks`: it fails,
   listing every site of §2.2 (paste the red run).
2. Add `libm = "0.2.16"` to `client`'s `[dependencies]`; confirm `git diff Cargo.lock` adds only the edge.
3. Write `smog/math.rs` (D-2) with unit tests: each function equals the `libm` function bit for bit; each agrees with
   the `std`/Bevy/glam original to within 2 ulp over a sweep of 10,000 arguments covering the call sites' ranges
   (`srgb_to_linear` over `[0, 1]`, `quat_from_rotation_y` over `[-2π, 2π]`); `srgb_to_linear` of 0 and 1 is exact.
4. Route every call site of the audit. Expressions stay the same apart from the function called (for example
   `(-damping * dt).exp()` becomes `math::exp(-damping * dt)`), so the only output change is the function's last bit.
   C22 turns green.
5. Rewrite the golden file per D-5. Run it: the re-recorded goldens fail with their new values; record them all in
   one commit with the message D-5 requires. Before committing, run D-3's interposer against the new test binary:
   every smog golden passes (paste the output). If one fails, a call site was missed: find it, route it, repeat.
6. Mutation red run: route one `math::sin` in `bubble_team_color` back to `f32::sin`; C22 fails, and under the
   interposer the fluid goldens fail; revert.
7. The bench `smog_math.rs` (D-6, instrument 1), with its `//!` stating that it is informational.

**Gate.** Common gate, plus the interposer run of step 5 recorded in the evidence file.

### WP3: Measurement, stop rule, records

**Files (owned):** the evidence file; `docs/plans/2026-11-01-codebase-remediation.md` (one note under Phase 6 or
the smog row it names, **verify**); `DESIGN.md` (the "Cross-platform determinism" or "Smog fog presentation" row
that says smog goldens are `libm`-bound, **verify** the wording); `crates/client/tests/smog_golden_tests.rs`'s `//!`
only if a figure is cited there; this plan's row in `docs/plans/README.md`; the Resolution of Q0014 is written by
whoever closes the question (the docs agent), from this plan's commits.

1. Measure D-6's instruments 1 to 4, arm A = `main` before WP2, arm B = WP2's branch, both release builds made
   outside the lock. Instrument 4 follows the open-findings plan's D-6 command exactly (about 70 s of lock per run,
   12 runs, released between runs; load recorded; a run above a one-minute load of 8 repeated at the end).
2. Apply the stop rule. Write the evidence file: the audit table, the interposer runs (before and after), the four
   instruments per arm, the stop-rule result.
3. If the rule did not fire: merge; update `DESIGN.md` (smog goldens are host-independent, checked by C22) and the
   remediation note; set this plan's row to `Done` with the commits. If it fired: do not merge WP2; file the
   question (D-6); set this plan's row to `Partly done` naming the question.

**Gate.** Common gate; for a documentation-only commit, `cargo fmt --all -- --check` and
`cargo test --locked -p workspace_checks`.

---

## 5. Risks

| Risk | Mitigation |
|---|---|
| A host-`libm` call the audit missed keeps a golden host-dependent | D-3's interposer run fails on it; WP2 is not done until it passes; C22 stops new ones in `smog/` |
| A site outside `smog/` (shared presentation code) reaches a smog hash | Same interposer run; if it names a file outside `smog/`, route that call through `smog::math` as well and extend C22's path list to that file, with the reason in the check's doc |
| `libm` crate result differs from glibc by more than one ulp somewhere visible | Owner accepted platform-dependent marginal differences; the semantic smog tests must pass unchanged or WP2 stops |
| A `cargo update` changes `libm` crate results | `smog_libm_crate_canary` fails with a message naming the cause |
| The C-18 chain is still running when this plan is picked up | D-1: hard dependency; the plan starts only after WP9 (or the chain's last merged WP) |
| Performance regression | D-6's stop rule; the routing is limited to smog (D-2), so the solver's per-voxel loops are untouched |
| The interposer itself is wrong (perturbs nothing) | WP1's red run: class-L goldens must fail under it before WP2 |

---

## 6. Decisions for the owner

None new. Q0014 is answered; D-2 narrows the option's text (no global `bevy_math` feature) under the scoping
authority of Q0002's answer, and records how to widen it. If D-6's stop rule fires, WP3 files a question with the
figures.

---

## Appendix: interposer source (for `~/scratch`, not committed)

```c
/* perturb.c: returns each libm result moved one ulp away from zero, to prove that no
 * host-libm result reaches a smog golden hash (plan 2026-11-04-libm-smog-math, D-3). */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <math.h>

#define WRAP1F(name) float name(float x) { static float (*real)(float); \
    if (!real) real = dlsym(RTLD_NEXT, #name); float r = real(x); \
    return nextafterf(r, r < 0 ? -INFINITY : INFINITY); }
#define WRAP2F(name) float name(float y, float x) { static float (*real)(float, float); \
    if (!real) real = dlsym(RTLD_NEXT, #name); float r = real(y, x); \
    return nextafterf(r, r < 0 ? -INFINITY : INFINITY); }
#define WRAP1D(name) double name(double x) { static double (*real)(double); \
    if (!real) real = dlsym(RTLD_NEXT, #name); double r = real(x); \
    return nextafter(r, r < 0 ? -INFINITY : INFINITY); }
#define WRAP2D(name) double name(double y, double x) { static double (*real)(double, double); \
    if (!real) real = dlsym(RTLD_NEXT, #name); double r = real(y, x); \
    return nextafter(r, r < 0 ? -INFINITY : INFINITY); }

WRAP1F(expf) WRAP1F(sinf) WRAP1F(cosf) WRAP1F(tanf) WRAP1F(logf) WRAP1F(exp2f) WRAP1F(log2f)
WRAP1F(acosf) WRAP1F(asinf) WRAP1F(atanf) WRAP2F(atan2f) WRAP2F(powf)
WRAP1D(exp) WRAP1D(sin) WRAP1D(cos) WRAP1D(tan) WRAP1D(log) WRAP1D(exp2) WRAP1D(log2)
WRAP1D(acos) WRAP1D(asin) WRAP1D(atan) WRAP2D(atan2) WRAP2D(pow)

void sincosf(float x, float *s, float *c) { *s = sinf(x); *c = cosf(x); }
void sincos(double x, double *s, double *c) { *s = sin(x); *c = cos(x); }
```

`sincosf` is defined through the wrapped `sinf` and `cosf`, so it is perturbed too. Note: `nextafterf` of a value at
`±1.0` (a sine or cosine at its extreme) can leave `[-1, 1]`; that is fine for a proof run and is not used for
anything else.
