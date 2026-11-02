# Host-independent RON output

## Problem

The first Windows CI run (41538515234) failed `showcase_roundtrip_is_byte_identical` in
`crates/sim_map/tests/map_format_tests.rs`. The re-encoded map contains `\r\n` and the committed
file contains `\n`. `ron` 0.8.1 picks the default of `PrettyConfig::new_line` with
`cfg!(not(target_os = "windows"))`, and `map_format.rs` never overrides it. CI sets
`core.autocrlf` to false, so the checkout is not the cause.

## Consequence for CI

The step `cargo test -p sim_types -p sim_map -p sim_core --locked` stopped at this failure on
Windows, and the release golden replay step after it did not start. The sim_core tests and the
release golden replay have therefore not yet run on Windows. This fix does not show that the
simulation matches on that platform. Only the next Windows run can show that, and it may reveal
further failures.

## Design

A generated data file must not depend on the host that wrote it.

- Each crate that owns a data format gets one private named newline constant and one private
  function that applies it, `canonical_pretty_config(base)`. There are no scattered literals.
- The function takes the base config as a parameter. On Linux the `ron` default is already `\n`,
  so a test of `to_ron_string` alone cannot fail there. Passing a base with `new_line: "\r\n"`
  reproduces the Windows default on any host, so the test fails on Linux if the override is
  removed.
- Bytes written on Linux do not change, so no committed asset changes.

## Sites

| Site | Writes | Decision |
| --- | --- | --- |
| `sim_map/src/map_format.rs` | Committed `*.scmap.ron` maps, compared byte for byte | Change |
| `asset_baker/src/manifest.rs` | Committed `*.flipbook.ron`; bytes are digested by `media.rs` | Change |
| `client_diagnostics/.../artifacts.rs` | Per-run `manifest.ron` | Leave |
| `client_diagnostics/src/palette_fixture.rs` | Per-run `sparse-battle.ron` | Leave |
| `client_diagnostics/.../ownership/tests.rs` | In-memory round trip, no file | Leave |

`artifacts.rs` writes run evidence into a caller-chosen output directory. It holds run paths and
timings, is never committed, and is read back only by substring checks on single tokens.

`palette_fixture.rs` writes a scenario into a fresh directory and reads it back through the parser.
That text embeds the canonical absolute path of the sparse map, so its bytes differ by directory
and host whatever the newline is. Its sparse map goes through `save_atomic` and gets the fix.

## Tests

- `sim_map` unit tests: the config overrides a Windows-style base; encoding a committed map under
  that base reproduces the committed bytes; `to_ron_string` contains no `\r`.
- `asset_baker` unit tests: the same override check; `save_to_ron` writes no `\r` to disk.
- Remove each override temporarily and confirm that the tests fail on Linux.

## Audit of the simulation crates

Search `sim_types`, `sim_map` and `sim_core` for `cfg!`, target `cfg` attributes, `std::env::consts`,
`MAIN_SEPARATOR`, path-to-string conversion, native-endian or pointer-width encoding, and raw file
text that reaches a hash or serializer. Report the findings. Change code only if a real defect
appears.

## Verification

`RUSTC_WRAPPER=sccache CARGO_INCREMENTAL=0` with:

- `cargo test -p sim_map -p asset_baker --locked`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo fmt --all -- --check`

The fix cannot be run on Windows here. The Windows result comes from the next CI run.
