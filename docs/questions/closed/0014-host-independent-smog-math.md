# Q0014: Should the client's smog math become host-independent?

- **Asked:** 2026-11-03
- **Blocking:** no
- **Default until answered:** (a). The smog math keeps calling the host's `libm`. The class-L smog goldens stay
  bit-exact and `libm`-bound, and `smog_golden_libm_canary` reports when a host's `libm` differs from the recording
  host (glibc 2.41, `x86_64`).

## Question

`crates/client/tests/smog_golden_tests.rs` pins smog output with bit-exact hashes. The client may use the host's
`libm` (charter §3.1 binds only the simulation crates), and the last bit of `sinf`, `cosf`, `atan2f`, `expf` and
`powf` differs between glibc versions, musl, macOS and Windows. Main's CI went red when glibc 2.39 (CI) and 2.41
(this server) disagreed on `atan2f`.

The blueprint `docs/plans/2026-11-03-host-independent-smog-goldens.md` split the goldens into two classes. The
kinematics goldens are now class H: no `libm` result reaches their hashes. The rest are class L: the fluid, startup,
noise and all system goldens depend on the host's `expf`, `powf`, `sinf` and `cosf`, because production code calls
them on the hashed path and a test cannot remove those calls without changing smog output. They pass on glibc 2.39
and 2.41 today. A newer runner image, macOS or Windows could fail them, and the canary would show why.

The options:

- **(a) Keep as is.** Cost: a `libm` change on CI turns the class-L goldens red until (b) is done.
- **(b) Route the smog client paths through the pure-Rust `libm` crate.** That covers the `fluid.rs` decay factors,
  `bubble_team_color`, the noise texel, the crowd generator and `sync_headings`. The crate is already in
  `Cargo.lock` as 0.2.16, so nothing new is downloaded. Also enable `bevy_math`'s `libm` feature for `to_linear`
  and `from_rotation_y`. Every smog golden then becomes class H, and the canary and the recorded crowd trig table
  (`crates/client/tests/support/smog_crowd_trig.rs`) go away. Cost: a deliberate output change of at most about one
  ulp per call, re-recorded goldens, and a direct dependency. The `bevy_math` feature changes glam math in the whole
  client, not only smog.

Recommendation: (b), as a separate plan after Phase 9, because Phase 9 allows no behaviour change.

## Answer

Owner (2026-11-03): Agreed (option b: route client smog math through the `libm` crate as a separate plan after Phase 9) unless this becomes a performance problem. I do not mind if visuals are marginally different depending on the platform, but I think that using libm simplifies tests.

## Effect of the answer

2026-11-03. Option (b) is approved, as a separate plan after Phase 9: route the client smog math through the `libm`
crate so every smog golden becomes host-independent, unless that turns out to cost noticeable performance.
Platform-dependent visuals are acceptable to the owner. Until that plan lands, default (a) and
`smog_golden_libm_canary` stay in place. This question stays open until then.

## Resolution

2026-11-05. Option (b) is carried out by plan `docs/plans/2026-11-04-libm-smog-math.md`, without a performance
problem.

- WP2, commit `a8cbfaa`: every exponential, sine, cosine, arctangent and power on a smog path goes through
  `client::smog::math`, a wrapper over the pure-Rust `libm` crate 0.2.16 (already locked; a direct dependency of
  `client` now). The fluid decay factors, `bubble_team_color`, the crowd generator, the headings, the noise texel, the
  showcase colours' sRGB-to-linear conversion and the showcase rotation are routed; the showcase heading readback
  through `Quat::to_euler` is gone. Workspace check C23 keeps smog production code off the host's functions.
- Scope, decided under Q0002's answer and recorded in the plan (D-2): the change is smog-local. `bevy_math`'s `libm`
  feature was not enabled, so the rest of the client and Bevy keep the host's library.
- Every smog golden is host-independent: with every host `libm` result moved by one ulp (an `LD_PRELOAD`
  interposer), all 17 pass. The two golden classes, the recorded crowd trig table and `smog_golden_libm_canary` are
  retired; `smog_libm_crate_canary` reports a `libm` crate version that changes the results. The fluid and noise
  goldens did not change; eight kinematics and system goldens were re-recorded (headings and rotations are now
  hashed; a few values differ from glibc's by one ulp).
- Cost (WP3): smog startup about 12 ms longer (the noise texture's `powf`); per frame about 0.01 ms by `perf`; the
  400-unit frame budget on the RX 580 shows no difference beyond the shared machine's noise. The plan's stop rule did
  not fire. Evidence: `docs/evidence/2026-11-05-libm-smog-math.md`.
