# Throughput history: the handover figures and the 400-unit residual (exits WP7)

This is the evidence for WP7 of `docs/plans/2026-11-03-close-phase-6-7-8-exits.md`, which settles items 2 and 3 of
its §2.5. It changes no code. The measurement of record stays `cargo bench -p sim_core --bench throughput`
(charter §3.1, `DESIGN.md` §5).

## Conclusions

1. **The handover's figures were host-specific.** Appendix A's `r12` repro, rebuilt from the source printed in
   `docs/plans/2026-11-01-codebase-remediation.md` Appendix A, gives the benchmark's figures on this server at
   `ae075fb`, at the handover's commit `b263734` and at `main`. The two differ by at most 4 % on any row, in
   either direction. `r12` and the benchmark measure the same thing. Neither comes near the handover's 11,114,
   2,706 and 639 ticks/s at any commit. Those figures came from the owner's notebook and do not transfer to this
   server. Nothing in the repository records the notebook's processor, so the factor cannot be checked. It is
   also not uniform: 2.2 to 4.0 times, depending on the row (table below).
2. **The 3 to 4 % residual at 400 units is not a code regression.** Between `b263734` and `main` the benchmark
   executes 0.22 % more user-space instructions in total. The rest of the measured slowdown comes from branch
   prediction, and that changes with the binary, not with the simulation source:
   - In the benchmark, `56f243f` and `ef411d0` run the same simulation code. Their instruction counts differ by
     0.0002 %, but their branch misses differ by 9 % (182 M against 198 M).
   - In `r12` the step lands on the merge `56f243f` (Phase 9 WP12, `99bb566`). There, by sample share,
     `acquire_targets` takes about 8.8 M more branch misses. That accounts for all of the binary's 7.4 M
     increase. Its share of instructions stays the same. Its
     source did not change in that merge. In the benchmark the same merge shows no step.
   - The step is below the plan's 2 % threshold in executed work, so the bisect stops here, as WP7 step 2 says.
     No finding is recorded. A later plan that wants those 2 to 4 % back should look at code layout (for example
     profile-guided optimisation or `codegen-units = 1`), not at a commit.

## Host and method

- **Host.** The shared development VM: AMD EPYC 7301 (Zen 1, 2.2 GHz base) under KVM. This work had 16
  dedicated cores (CPUs 32 to 47). The Rust toolchain was `1.99.0` from each tree's `rust-toolchain.toml`.
- **Trees.** Each commit was exported with `git archive -o <sha>.tar <sha>` and unpacked under
  `~/scratch/exits_wp7_trees/<sha>`. Each was built in its own target directory,
  `~/scratch/cargo-target/exits-wp7/<sha>`.
- **Benchmark.** `cargo bench -p sim_core --bench throughput --no-run --locked`, then the binary was run directly.
  It reports the median of three repeats of 300 ticks for each scenario.
- **Load.** Two other agents' gates ran on the same 16 cores throughout. The system load average also counts
  another user, so each run records the share of CPUs 32 to 47 that was busy while it ran, read from
  `/proc/stat` before and after. That share includes the measured process itself, about 6 %. Runs of the
  compared commits were interleaved, round by round.
- **Counters.** `perf stat -e instructions:u,cycles:u,branch-misses:u` and
  `perf record -e {cycles,instructions,branch-misses}:u` were used (perf 6.12, `perf_event_paranoid` 1). User-space
  instruction counts are exact and repeat to the instruction: the simulation is deterministic, and the counts
  do not depend on load. Cycles and wall-clock time do. The VM exposes no cache-miss counters.

Logs (scratch, may be cleaned): `~/scratch/logs/exits_wp7_r12.log`, `exits_wp7_bisect.log`,
`exits_wp7_r12_perf.log`, `exits_wp7_r12_bisect.log`, `exits_wp7_cachestat.log`, `exits_wp7_benchbranch.log`.
The tables below are the record.

## 1. The `r12` repro against the benchmark

### The repro crate

`~/scratch/exits-r12/<sha>/` follows Appendix A. It is a crate with path dependencies on `sim_core`, `sim_map` and
`sim_types` of the exported tree, an empty `[workspace]` table, and copies of that tree's `Cargo.lock` and
`rust-toolchain.toml`. Its `tests/repro.rs` holds only Appendix A's helpers `s`, `v`, `flat`, `unit`, `order`, `run`,
`bench` and the test `r12_throughput_report`, as printed. It was run as
`cargo test --release --test repro -- --nocapture --test-threads=1`. In practice that means building once with
`--no-run` and then running the test binary directly, interleaved with the benchmark.

**The port for `main`.** `SimWorld::new(map, seed)` left `sim_core` in `03d578d`. At `main`, and at the bisect
points `2797d4a` and `56f243f`, `flat` builds its world the way `throughput.rs` does:

```rust
fn flat(n: u32) -> SimWorld {
    let c = RuntimeCatalogs::load_from_directory("<tree>/assets/catalog").unwrap();
    SimWorld::with_catalogs(GameMap::new(n, n, MapCell::default_ground(CliffLevel(0))).unwrap(), 7, c.units, c.upgrades)
}
```

Nothing else differs from Appendix A. The catalog is loaded before the timed loop, as in the benchmark. At
`b263734`, `SimWorld::new` used the embedded starter roster. That roster is `include_str!` of the same
`assets/catalog` files, so both builds run the same data.

**How `r12` and the benchmark compare.** Both use `pulse_bot` and seed 7. Both place 20-wide formations 5 cells
from opposite corners. Both issue the two attack-move orders on ticks 0 and 1, to cells `n − 8` and 8, and both
time 300 empty ticks. The one difference is that `r12` measures each scenario once, where the benchmark takes the
median of three.

### Results

Measured 2026-11-03 18:21 to 18:24 UTC: three interleaved rounds, each running `r12` and then the benchmark for
each tree. The cells give the median, with the range over the three rounds in brackets. Busy is the share of our
16 cores in use during each run.

| Tree | Source | 64², 0 | 64², 100 | 64², 400 | 128², 400 | 256², 0 | Our cores busy |
|---|---|---:|---:|---:|---:|---:|---:|
| `ae075fb` | `r12` | 3364 (3240–3374) | 1226 (1146–1239) | 202 (196–204) | 219 (219–228) | 208 (207–212) | 33–72 % |
| `ae075fb` | bench | 3386 (3377–3387) | 1229 (1214–1230) | 201 (198–201) | 225 (223–226) | 211 (211–213) | 28–36 % |
| `b263734` | `r12` | 3278 (3238–3384) | 1206 (1174–1218) | 201 (199–202) | 226 (224–227) | 209 (195–211) | 26–44 % |
| `b263734` | bench | 3345 (3336–3370) | 1208 (1135–1208) | 200 (197–200) | 224 (224–226) | 212 (209–212) | 25–71 % |
| `ef411d0` (`main`) | `r12` | 3283 (2007–3359) | 1191 (1139–1201) | 191 (175–193) | 219 (185–221) | 212 (192–212) | 30–97 % |
| `ef411d0` (`main`) | bench | 3383 (3247–3392) | 1202 (1187–1207) | 199 (198–199) | 221 (218–222) | 213 (209–213) | 32–78 % |

The low minima (2007 and 175) come from the one run whose cores were 97 % busy.

**Against the handover.** The handover's row is its last run of `r12` on the owner's notebook at `b263734`
(handover §5). The factor is that figure divided by `b263734`'s `r12` median above:

| | 64², 0 | 64², 100 | 64², 400 | 128², 400 | 256², 0 |
|---|---:|---:|---:|---:|---:|
| Handover (notebook) | 11,114 | 2,706 | 639 | 762 | 835 |
| This server, `r12` at `b263734` | 3,278 | 1,206 | 201 | 226 | 209 |
| Factor | 3.4 | 2.2 | 3.2 | 3.4 | 4.0 |

Appendix A's own run, on the audit host at the audit's code, measured 969 on the empty 64×64 map, and this
server measures 774 to 813 there (plan §2.5): a factor of only 1.2. Nothing on record shows whether the audit
host and the handover's notebook were the same machine, or what processor each had. So the code is cleared, and
the hardware explanation stays unconfirmed beyond "a different host".

## 2. The 400-unit residual between `b263734` and `main`

### Bisect points

The plan named three candidates: `866563f` (`PlayerView` written every tick), `90d9e25` (`PerPlayer` indexing)
and `99bb566` (classes from data). Each of these, and `03d578d` (catalogs out of `sim_core`), arrived on `main`
in a merge. So the bisect measured the first-parent merges around them:

| Point | Contains | Instructions against the previous point |
|---|---|---:|
| `b263734` | the handover | baseline |
| `28a071d` | `866563f`, `PlayerView` | +0.000 % |
| `7c947ad` | the rest of Phase 5 (WP7 fog, WP9) | +0.01 % |
| `331ccb4` | `90d9e25`, `PerPlayer` | +0.12 % |
| `2797d4a` | `03d578d`, catalogs loaded from disk | −0.29 % (bench) / +0.01 % (`r12`) |
| `56f243f` | `25204fd`, `99bb566`, `a60cdb3` (Phase 9 WP12) | +0.39 % |
| `ef411d0` (`main`) | Phase 9 WP13 and the exits merges so far | +0.000 % |

Instruction counts of the benchmark binary, which runs every scenario three times and includes catalog loading
and world construction, measured 18:25 to 18:34 UTC over five interleaved rounds. Every round gave the same
count, to within a few thousand instructions:

| Tree | 64², 400 (median, range) | 128², 400 (median, range) | Our cores busy | Instructions | Cycles (median, min–max) |
|---|---:|---:|---:|---:|---:|
| `b263734` | 199 (184–201) | 223 (194–226) | 24–86 % | 87.614 G | 37.45 G (37.18–39.98) |
| `28a071d` | 195 (189–202) | 224 (210–227) | 21–100 % | 87.614 G | 37.16 G (36.99–38.02) |
| `7c947ad` | 191 (170–199) | 222 (216–224) | 44–90 % | 87.622 G | 38.34 G (37.31–40.07) |
| `331ccb4` | 194 (100–200) | 215 (108–225) | 36–82 % | 87.724 G | 37.71 G (37.34–39.21) |
| `2797d4a` | 199 (194–200) | 223 (222–225) | 23–83 % | 87.470 G | 37.53 G (37.16–38.28) |
| `56f243f` | 200 (197–200) | 224 (221–227) | 18–75 % | 87.810 G | 37.07 G (37.01–37.61) |
| `ef411d0` | 195 (190–199) | 218 (207–225) | 29–94 % | 87.810 G | 37.66 G (36.95–38.59) |

Under this load the benchmark's wall-clock figures cannot resolve 2 to 4 %. A run sharing the cores at 80 to
100 % loses up to half its speed (`331ccb4`'s minimum). The best cycle counts show no trend from `b263734` to
`main`.

The `r12` binary, which runs each scenario once, was measured 18:41 to 18:44 UTC over five interleaved rounds,
at a lower load:

| Tree | 64², 400 | 128², 400 | Our cores busy | Instructions | Cycles (median, min–max) |
|---|---:|---:|---:|---:|---:|
| `b263734` | 200 (191–203) | 226 (221–228) | 17–42 % | 29.215 G | 12.41 G (12.29–12.59) |
| `28a071d` | 201 (199–201) | 228 (226–228) | 16–61 % | 29.215 G | 12.34 G (12.31–12.40) |
| `7c947ad` | 203 (201–205) | 227 (226–230) | 19–33 % | 29.218 G | 12.30 G (12.20–12.34) |
| `331ccb4` | 201 (199–201) | 227 (226–227) | 13–35 % | 29.251 G | 12.34 G (12.34–12.79) |
| `2797d4a` | 201 (188–203) | 220 (210–227) | 11–42 % | 29.253 G | 12.88 G (12.29–13.22) |
| `56f243f` | 193 (193–195) | 220 (219–221) | 11–36 % | 29.369 G | 12.60 G (12.57–12.63) |
| `ef411d0` | 194 (190–195) | 220 (219–221) | 12–36 % | 29.369 G | 12.66 G (12.56–12.70) |

Here the residual is visible and lands on `56f243f`. At 64², 400 it is 201 → 193 ticks/s, about 4 %, which is
4.98 → 5.18 ms per tick, so +0.2 ms per tick at 400 units. At 128², 400 it is about 3 %. Cycles rise 2 % and
instructions 0.4 %.

### What the step at `56f243f` is

`perf stat` on the `r12` binaries of `2797d4a` and `56f243f`, three interleaved runs each
(`exits_wp7_cachestat.log`):

| Tree | Instructions | Cycles | Branch misses |
|---|---:|---:|---:|
| `2797d4a` | 29.253 G | 12.30–12.86 G | 61.1–61.3 M |
| `56f243f` | 29.369 G | 12.58–12.74 G | 68.5–68.7 M |

`perf record` by symbol:

| Symbol | Share of cycles, `2797d4a` → `56f243f` | Share of instructions | Share of branch misses |
|---|---|---|---|
| `SimWorld::acquire_targets` | 22.5–23.0 % → 23.9–24.8 % | 21.0 % → 20.9 % | 48.3 % (29.5 M) → 55.8 % (38.3 M) |
| `compute_simulation_checksum` | 23.2–23.9 % → 22.9–23.4 % | 27.1 % → 27.3 % | under 1 % |

The binary's extra 7.4 M branch misses all fall in `acquire_targets`: by sample share it gains about 8.8 M, and
the other symbols lose some. At roughly 20 cycles each on Zen 1, 7.4 M misses are about 1 % of the run. The extra
0.4 % of instructions was not traced to a symbol. `target_acquisition.rs` and `world/upgrade_stats.rs` did not change in
WP12. What changed is the layout of the data they read: `UnitArchetype::faction` and `armor_class` and
`WeaponSpec::weapon_class` became catalog IDs instead of Rust enums. The sim crates also grew by about 740
lines, so code generation and placement changed too.

The same comparison in the benchmark binary shows that this is an effect of the binary, not of the simulation
source. Two interleaved runs (`exits_wp7_benchbranch.log`):

| Tree | Instructions | Cycles | Branch misses |
|---|---:|---:|---:|
| `b263734` | 87.614 G | 37.29–39.92 G | 196.5–201.3 M |
| `2797d4a` | 87.470 G | 37.45–37.68 G | 188.6–189.2 M |
| `56f243f` | 87.810 G | 37.00–37.44 G | 181.3–182.5 M |
| `ef411d0` | 87.810 G | 37.22–37.43 G | 197.9–198.0 M |

In this binary, `56f243f` has the fewest branch misses of all four trees. `ef411d0` executes the same simulation
instructions: its 130 thousand extra come from the benchmark's own `--floor` argument parsing, added in WP13. Yet
it takes 9 % more branch misses. The step therefore moves from one binary to the next with code placement. Its
size, 2 to 4 % at 400 units on this host, sets the noise floor for comparing two builds by wall-clock time.

## Commands

```bash
# One tree, one target directory (bench binary)
cd ~/scratch/exits_wp7_trees/<sha>
CARGO_TARGET_DIR=~/scratch/cargo-target/exits-wp7/<sha> cargo bench -p sim_core --bench throughput --no-run --locked
# The r12 repro crate of that tree
cd ~/scratch/exits-r12/<sha>
CARGO_TARGET_DIR=~/scratch/cargo-target/exits-wp7/<sha>-r12 cargo test --release --test repro --no-run
<target>/release/deps/repro-<hash> --nocapture --test-threads=1 r12_throughput_report
# Counters
perf stat -x, -e instructions:u,cycles:u,branch-misses:u <binary>
perf record -e branch-misses:u -F 2000 <binary>; perf report --no-children --sort symbol --stdio
```

The exported trees, repro crates and target directories were deleted after the runs.
