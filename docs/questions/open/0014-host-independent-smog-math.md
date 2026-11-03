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

