# Round 3 weapon-period implementation validation

Plan 32 implementation is frozen for the orchestrator's independent workspace checkpoint. This is an implementation evidence record, not a claim of independent visual or cross-platform verification.

## Delivered contract

- Positive attack rates use a validated newtype. The interval resolver performs signed addition and rate division in `i128`, then clamps once to the documented `1..=u16::MAX` timing representation. No-double-clamp regressions include 60,000 base + 60,000 delta at rate two yielding 60,000 ticks.
- Ordered buff multiplication uses exact wide intermediates with the established positive Q32.32 floor quantization; only representational overflow/underflow saturates. Tests establish ordinary reciprocal cancellation and the documented loss of associativity after saturation.
- Authored capability and ability rates reject nonpositive/quantized-zero values. Direct overclock and ability execution reject before health/energy/cooldown payment or buff refresh.
- Upgrade application returns typed checked failures and stages all effect categories. Research stages live HP changes and propagates invalid catalog identity errors before committing any player/entity state. The root's independent review identified the previous skip-on-invalid-identity branch; it was corrected and a late-failure rollback regression added.
- Invalid manually mutated runtime buffs produce typed per-tick entity/buff/value diagnostics. They suppress the affected attack deterministically. The normal client consumes the errors, logs evidence, retains the failure for UI display, pauses immediately and emits `AppExit::Error`; actual sync-system regression confirms catch-up stops after the first failed tick.
- Diagnostic outputs are excluded from checksums; the malformed source buff remains hashed. No authoritative encoding width changed, so schema 2 remains. Public `apply_upgrade` and `attack_rate_multiplier` now return checked results; repository call sites compile with the updated APIs.

## Recorded checks

- Debug simulation regressions: 59 passed across library math/catalog checks, weapon periods, research transactions, upgrade transactions, existing abilities/upgrades and checksum replay. `/tmp/space-scuffle-r3-weapon-period-regressions.log`.
- Release arithmetic/transaction regressions: 8 passed. `/tmp/space-scuffle-r3-weapon-period-release.log`.
- Additional invalid-identity transaction regression: passed. `/tmp/space-scuffle-r3-weapon-period-identity.log`.
- Real client simulation diagnostic integration: passed. `/tmp/space-scuffle-r3-weapon-period-client.log`.
- Strict sim_core/sim_types all-target Clippy and client library/diagnostic-test Clippy: passed. `/tmp/space-scuffle-r3-weapon-period-sim-clippy.log`, `/tmp/space-scuffle-r3-weapon-period-client-clippy.log`.
- Workspace formatting check: passed. `/tmp/space-scuffle-r3-weapon-period-fmt.log`.

An attempted all-client-target Clippy also exposed concurrent owner-file warnings in mist/runtime-battle tests; their owners were notified. The orchestrator coordinates the complete workspace test/Clippy/format checkpoint, which remains required before accepting the round. No visual output changed in this simulation/error-reporting lane, and no screenshot quality claim is made.

## Limits retained deliberately

The supported effective weapon interval remains `u16`; large valid intervals saturate explicitly instead of wrapping. Construction/research durations still need separate gameplay lifecycle work. Movement-rate arithmetic and unrelated combat-stat formulas are outside this bounded change. Repeated research authorization/cost handling was not redesigned. The dynamic digest still requires matching catalogs and executable behavior.
