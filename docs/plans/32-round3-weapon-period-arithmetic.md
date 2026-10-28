# Round 3: Deterministic weapon period modifier arithmetic

Status: approved by the orchestrator, including typed per-tick diagnostic output and normal-client failure propagation. Implementation stage authorized.

## Goal and concrete failures

Resolve authored signed cooldown adjustments and temporary attack-rate multipliers without signed overflow, fixed-point overflow, underflow to an accidental default, or narrowing wraparound. Preserve ordinary authored combat timing and the existing `u16` timing representation. Define limits from that representation and the 25 Hz simulation update resolution, rather than inventing balance caps.

Three current operations violate this goal:

1. `PlayerUpgradeState::apply_upgrade` adds `i32` cooldown deltas with unchecked `+=`. Multiple individually representable authored effects can overflow the cumulative value.
2. `effective_period_from_upgrades` adds a `u16` base period to the signed delta in `i32`, then casts to `u16`. A large positive delta can overflow the addition; a result above 65,535 wraps to an unrelated faster interval.
3. `Entity::attack_rate_multiplier` multiplies Q32.32 factors with unchecked arithmetic. The world then converts the upgraded period to Q32.32, divides, converts to `i32`, and narrows again to `u16`. Tiny positive rates can overflow the division; large factor products overflow; tiny products become zero and currently cause modifiers to be ignored.

The fix must retain the unclamped upgraded numerator until after buff scaling. Calling an already-clamped public upgrade-only helper before applying buffs would still produce incorrect results.

## Ownership and architectural boundaries

Own a small pure weapon-timing module in `sim_core`, narrow world timing integration and research transaction handling, the attack-rate fold and overclock admission in `entity.rs`, positive-rate authored validation in `catalog/validation.rs`, typed checked upgrade accumulation in `sim_types/src/upgrades.rs`, required error/reexport wiring, and focused tests. Coordinate `world.rs` hunks and keep other agents' movement, constructor, production and checksum work untouched.

No authored unit identities, new creatures, new balance values, map/editor edits, shader changes, weapon cooldown widening, research queue, construction lifecycle, or movement multiplier redesign. Existing weapon windup/backswing/cooldown state transitions stay intact; their actual use of the resolved period is tested. Do not claim this proves all combat fixed-point arithmetic safe.

## Explicit interval semantics

The effective weapon interval is an integer number of 25 Hz ticks in `[1, u16::MAX]`. One tick is the engine's minimum temporal resolution. The upper bound is the maximum representable existing weapon timing interval, not a unit-specific restriction. This is deliberate saturation to supported timing, never modulo reduction. Document it on the resolver and public `effective_attack_period` API.

Compute the upgraded numerator as `i64::from(base_period) + i64::from(cumulative_delta)`. This exact sum is bounded by the signed 32-bit delta plus 65,535 and cannot overflow `i64`. For a positive combined rate with Q32.32 raw integer R, compute:

```text
scaled_ticks = ((upgraded_numerator as i128) * 2^32) / (R as i128)
effective_ticks = clamp(scaled_ticks, 1, u16::MAX)
```

The multiplication is comfortably inside `i128`, even at the extreme signed delta. Integer division gives the existing truncation toward zero; for positive durations it is floor. Negative/zero upgraded values resolve to one tick after the final clamp. Only then convert to `u16`; document the immediately preceding bounds that prove the conversion lossless. No float conversions or intermediary Q32.32 duration are needed.

Use the same pure resolver for upgrade-only queries (rate one) and actual world attacks (combined rate). Actual attacks must supply the original base period plus signed cumulative delta to this resolver, not the result of `effective_attack_period`, so the upper/lower bound is applied once. For example, base 60,000 plus delta 60,000 at rate 2 resolves to 60,000, not 32,767. A negative upgraded numerator remains one after scaling; no intermediate one-tick minimum is multiplied into a different result.

## Bounded positive buff product

Define combined rate as an ordered Q32.32 fold over the stored buff vector. For each positive raw factor, multiply positive raw values in `i128`, shift/divide by `2^32` using the same positive fixed-point floor rule, then clamp only if the result cannot be represented as a strictly positive Q32.32 value: minimum raw 1, maximum raw `i64::MAX`. The product of two positive `i64` raw values fits `i128`; no heap allocation or arbitrary precision dependency is needed.

Ordinary products retain the previous quantization. Saturation changes only values outside the positive representable domain, including a mathematical positive value below the smallest positive quantum. Multiplication order remains the existing vector order. This fold is intentionally not associative once intermediate quantization or saturation occurs; a later reciprocal cannot recover information already outside Q32.32 precision. Document that limitation and test it explicitly, rather than claiming exact real-number cancellation across saturation. Within the representable domain, reciprocal factors with exact binary representation must cancel as expected. Do not cap factors merely because the current weapon would already be faster than one tick: later factors can restore a slower rate.

Expose the pure fold helper or keep it crate-private with public Entity coverage. `Entity::attack_rate_multiplier` should delegate to this implementation so the world and tests cannot disagree about aggregation.

## Invalid rate boundary and admission

Authored attack-rate multipliers must be strictly positive after conversion to Q32.32. Reject zero, negatives, and positive authored floats that quantize to zero with the existing field-specific `CatalogInvalidValue`; do not coerce bad configuration to one or epsilon. Check both capability DTO overclock data and typed `AbilityEffectSpec::Overclock` values in every supported authored ability list (top-level and capabilities, plus structures where present). A typed SimScalar deserializer prevents representational overflow but does not establish positivity.

Direct `apply_overclock` must reject a nonpositive rate before HP payment or buff refresh. `execute_ability` must include the same condition in its preconditions before energy deduction or ability cooldown assignment. Existing return-value conventions can remain `bool`; an invalid caller-supplied spec returns false with no changes. Do not duplicate the predicate inconsistently across these entry points.

The public `active_buffs` vector also permits callers to bypass all admission checks. Add a typed validation/result API for the fold, identifying the offending buff index/ID and nonpositive raw rate. Document the positive-rate invariant on public buff state. A zero sentinel by itself is rejected: silently suppressing firing would disguise malformed runtime state as legitimate gameplay.

**Approved reporting extension:** `step` is infallible and `SimWorld` had no diagnostic/error output channel; its only event queue contained visual combat events. Neither panicking nor overloading those visual events is appropriate. Add a typed per-tick simulation-diagnostic collection with a public accessor/drain API, cleared at the beginning of each tick. A malformed rate records entity/buff evidence and suppresses that entity's attack for that tick deterministically. Normal client simulation synchronization must consume the diagnostics and fail visibly through its existing application-error/logging path. Diagnostics are derived outputs and excluded from the checksum; the actual malformed buff is already hashed. Only invalid-state handling allocates diagnostics; ordinary weapon ticks retain no new heap allocation. The orchestrator approved this narrow extension into the client synchronization call site; a broader fallible `try_step` migration remains outside this round.

## Typed, atomic upgrade accumulation

Keep the authored `UpgradeEffect::ModifyUnitStat.attack_cooldown_delta` and cumulative field as `i32`; their declared representation remains unchanged. Change `PlayerUpgradeState::apply_upgrade` to return `Result<(), UpgradeApplicationError>`, with a typed error identifying the target slug and overflowing stat. Use a typed stat discriminator rather than an opaque formatted error. Propagate this through `SimCoreError` with the source preserved. Audit every call site; the production caller is `research_upgrade`. This is an explicit Rust API change; callers must handle the result.

Stage changes in a cloned player state and perform checked additions before committing it. Upgrade application is infrequent, so this allocation is appropriate and occurs outside per-tick weapon resolution. Check the companion fixed-point stat accumulations in this same transaction as well as the signed cooldown sum: otherwise a later effect could panic after an earlier successful mutation. Insertion into completed-upgrade slugs, ability unlocks and weapon/armor level changes must all remain staged until every effect succeeds. On any overflow the original state remains exactly equal to its pre-call value. Do not saturate signed aggregate deltas: doing so would silently lose later opposing adjustments.

`research_upgrade` currently applies upgrade state, then retroactively adds positive HP effects to matching live entities. It does not deduct resources or schedule research time. Prepare the prospective player state and all affected live HP/max-HP changes using checked additions before committing either. If any operation fails, preserve player resources, completed research slugs, numeric deltas, ability unlocks and all live entities. Preserve current successful semantics, including effect order and positive-only retroactive HP additions; repeat-research authorization and research timing are separate issues. Return typed errors instead of swallowing failures. Do not add resource charges as part of arithmetic repair.

No state field width or encoding changes are planned, so schema 2 remains the canonical byte schema. Gameplay arithmetic changes still require matching executable/rules for lockstep; the dynamic digest is not a content compatibility handshake.

## Verification matrix

Pure interval tests must cover base 0/1/65,535; delta `i32::MIN`/`i32::MAX`; exact bounds; values just above 65,535; non-integral positive division; smallest positive and maximum rate; negative/zero upgraded numerator; and the no-double-clamp example. An independent reference calculation using raw integer arithmetic should check a table or exhaustive bounded sample, not merely call the same helper twice.

Buff tests must cover no buffs, one, normal overclock, exact reciprocal cancellation, multiple slow/fast factors, positive product underflow, upper saturation, order-sensitive saturation followed by reciprocal restoration, and typed errors for invalid nonpositive factors. Assert the documented quantization and ordering; do not test only that the code avoids panicking. Check catalog rejection for capability and ability sources, including a positive float below Q32.32 resolution. Check direct and ability-based rejection before HP/energy/cooldown or refresh changes. Once the reporting integration is approved, require an actual malformed-runtime-state test proving error evidence reaches the public diagnostics and normal client failure path; no silent defensive outcome is acceptable.

Upgrade tests must exercise cumulative positive and negative signed overflow, a boundary sum that remains valid, multiple effects targeting the same slug, opposing deltas that cancel within bounds, and a later failure after earlier valid changes. Compare complete state before/after failure, including completed slugs/unlocks/levels. Exercise real `research_upgrade` with a loaded or injected upgrade definition and confirm typed errors preserve live entity stats and player resources. Include companion fixed-point overflow and live HP overflow to verify transaction atomicity.

Integration tests must use real catalog archetypes selected by weapon/buff capabilities, never hardcoded identities. Drive actual weapon ticks with a normal buff, a large positive delta plus rate two, and a tiny positive rate. Verify actual fired-shot events/HP changes and the cooldown stored after firing, accounting for existing damage-point subtraction and backswing. A near-completion initialized cooldown may verify the next firing transition without waiting 65,535 ticks; distinguish fixture setup from elapsed simulation. Include two identical-world replay runs comparing full entity state and checksums with modifiers active.

Run debug and release focused arithmetic tests to expose profile-dependent overflow behavior; successful output must agree. Run focused upgrade, weapon, ability and checksum regressions. At the shared source freeze run `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --all -- --check`, recording logs under `/tmp/space-scuffle-r3-weapon-period-*.log`. Independent review must inspect transaction boundaries, double clamping, admission coverage and actual weapon integration. No screenshot is required for a simulation-only arithmetic change.
