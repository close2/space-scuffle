# Round 1 — Exact fixed-point serialization

Status: planning complete; awaiting orchestrator approval before implementation.

## Goal and evidence

Close review finding F-04: every `SimScalar` emitted by Serde must recover all 64 Q32.32 raw bits when read back. Preserve the existing catalog's unquoted integer and decimal input syntax. Invalid input must return a deserialization error instead of panicking or saturating.

`crates/sim_types/src/math.rs` currently serializes through `f64`, whose 53 significant bits cannot retain all signed Q32.32 values. Its visitor calls unchecked `I32F32::from_num` and accepts a sequence without checking for trailing elements. Existing snapshot tests use visual floating-point snapshots and do not establish exact authoritative scalar persistence.

## Ownership and exclusions

Own `crates/sim_types/src/math.rs`, a new `crates/sim_types/tests/scalar_serialization.rs`, and, if necessary, an isolated test in `crates/sim_core/tests/catalog_serialization_compatibility.rs`. A dev dependency addition to `crates/sim_types/Cargo.toml` requires orchestrator coordination. Preserve existing changes in every other file, especially `weapon.rs` and `sim_core/src/catalog.rs`. Do not edit maps, editor code, rendering, unit identities, or catalog values.

## Encoding contract

1. For human-readable serializers, emit a string `"q32.32:v1:<signed raw i64 in base ten>"`. Example: scalar one emits `"q32.32:v1:4294967296"`. This is deliberately distinct from unquoted legacy integers, which continue to mean whole simulation units. The prefix carries representation and version; reject unknown versions and malformed values. Document this contract in `SimScalar` rustdoc and the serializer implementation.
2. For non-human-readable serializers, emit `serialize_i64(self.raw())`; deserialize with `deserialize_i64` and construct with `from_raw`. Do not use `deserialize_any` for binary readers. The enclosing binary persistence protocol must version this schema when introduced; no binary persistence protocol currently exists in this repository, and historical floating encodings must never be guessed from raw bytes.
3. The new writer never emits floating-point scalars. Existing human-readable integer and finite floating-point tokens remain readable through checked `I32F32::checked_from_num`. These are compatibility input with the same representable-value rounding behavior as the existing catalog loader, not a claim that an arbitrary decimal token reaches Serde without floating-point conversion.
4. Continue accepting the existing single-element numeric tuple/newtype compatibility syntax if currently exercised. Require exactly one item: reject empty and overlong sequences. Nested exact scalar strings may also be supported without ambiguity, but do not broaden input to arbitrary string numerals or untagged raw bits.
5. Reject non-finite floats, signed or unsigned integers outside Q32.32's whole-unit range, malformed tagged strings, raw integers outside `i64`, and unsupported encoded versions. Return the format's `serde::de::Error`; never panic, silently clamp, or wrap.

## Implementation sequence

1. Add regression cases demonstrating `i64::MAX`, `i64::MIN + 1`, and values above the `f64` exact integer boundary lose bits with the old writer.
2. Split human-readable and binary paths at the Serde entry points. Keep small visitors/helpers private, and use standard checked integer parsing for tagged raw bits. Document why legacy numeric input and tagged raw input have different units.
3. Replace unchecked numeric deserialization conversions with checked conversion plus descriptive errors. Tighten sequence cardinality.
4. Add the public-API tests below. Do not introduce a custom decimal arithmetic parser or redesign fixed-point arithmetic: the explicit exact representation removes the ambiguity directly.

## Verification round and acceptance criteria

- Exact RON round-trip for zero, positive and negative one raw bit, both `i64` extrema and their adjacent values, Q32.32 whole-unit boundaries, large integer-plus-fraction values, and values adjacent to powers of two. Compare `raw()`, never floating-point conversions or epsilon tolerances.
- Deterministic seeded property sweep over at least 10,000 full-width `i64` raw values using the existing `rand_chacha` dev dependency, plus explicit adversarial values. The generator must cover all raw bits rather than a narrowed integer range.
- Round-trip nested `SimVec2` and a real `PlayerCommand` / `UnitOrder::Move` payload with adversarial coordinates through RON. Existing `RenderSnapshot` tests must still pass, but are not a substitute for authoritative-value tests.
- Assert the emitted RON representation contains the versioned exact tag, and verify unquoted `1` still means `ONE`, not raw bit one. Exercise legacy positive/negative integers and decimal values representative of the real catalog.
- Verify binary encoding and decoding do not call `deserialize_any`, using a small test-only non-human-readable Serde adapter around typed i64 primitives if no binary codec dependency is already available. Prefer a real already-available codec if present; do not add a production dependency solely for tests. Assert full-width raw bits in both directions.
- Negative tests for overflowing raw strings, unsupported version, malformed tags, numeric overflow, NaN/infinities, empty/overlong tuples, and valid numeric boundary values. Tests return `Result` where useful and must not hide panic paths.
- Load the actual `assets/catalog/units.ron` and `structures.ron` through `UnitCatalog::from_ron_str_bundle` from a `sim_core` integration test. This avoids a dependency cycle in `sim_types`. Existing catalog integration tests can provide this evidence if they already explicitly exercise both real files.
- Implementation runs `cargo test -p sim_types`, relevant `sim_core` catalog tests, then all required workspace gates: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --all -- --check`. Coordinate shared compilation with the orchestrator.

Acceptance requires raw-bit equality in every exact round-trip and unchanged readability of the checked-in catalog. This is nonvisual work; no screenshot can validate serialization correctness, and no visual claims should be made.

## Compatibility and review notes

New textual writes use a new explicit encoding; old releases will not read them. Current releases continue to read old numeric RON. This is a deliberate forward format change and must be called out in the implementation report. Never reinterpret old unquoted integers as raw bits. No catalog migration is required and no rebalancing is authorized.
