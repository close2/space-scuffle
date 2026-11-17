# Guide: Writing a `PERFORMANCE RATIONALE` Block

This guide holds the example of charter §2.3 (`AGENTS.md`), moved out of the charter word for word (Q0084, Q0085).
Check C11 (`crates/workspace_checks`) requires the `Why:`, `What:` and `Trade-off:` lines; reviewers judge whether
the optimisation is justified.

Default to clean, idiomatic, readable, and type-safe Rust. Whenever code diverges from the most idiomatic or elegant
form for performance reasons (e.g., flat structure-of-arrays layout, lookup tables for deterministic trig, GPU buffer
packing, avoiding allocations in hot loops), document it with a structured comment block:

```rust
// PERFORMANCE RATIONALE:
// Why: In a 400-unit battle, naive O(N^2) pairwise collision checks perform 160,000 tests/tick.
// What: We partition entities into a fixed-size spatial hash grid using pre-allocated flat slabs.
// Trade-off: Slightly more bookkeeping on position updates in exchange for O(N) neighborhood queries
// and zero heap allocations during the simulation tick.
```
