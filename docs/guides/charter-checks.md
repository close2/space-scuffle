# Guide: The Charter Checks

Charter §9 (`AGENTS.md`) tags each rule with how it is checked. A **[CI: Cn]** tag names one of the checks below.
Each is a `#[test]` named after its ID in `crates/workspace_checks/tests/charter.rs`, except C22, which is in
`crates/gate/tests/suite_catalog_tests.rs`; a failing check prints every finding as `path:line: message`. Check C20
holds the paths named in this table to existence, as it does for the charter and the other guides.

| Check | Verifies |
|---|---|
| C1 | The `DESIGN.md` §2 graph equals the Cargo graph, and Cargo's edges follow charter §5's layers |
| C2 | Simulation crates have no engine, GPU, window or audio dependency |
| C3 | No `TODO`, `FIXME`, `XXX` |
| C4 | Every Rust file begins with `//!` |
| C5 | No catalog identity in production Rust |
| C6 | No embedded `assets/` data in production code |
| C7 | Floats in the simulation crates only at listed boundaries |
| C8 | No `too_many_lines` suppression outside tests; one threshold everywhere |
| C9 | Simulation crates' `clippy.toml` disallow `HashMap`, `HashSet` and clocks |
| C10 | `expect` messages state an invariant |
| C11 | `PERFORMANCE RATIONALE` blocks have Why, What, Trade-off |
| C12 | No discarded results in the simulation crates |
| C13 | No mutable statics |
| C14 | Library crates return typed errors |
| C15 | The test subjects of `docs/guides/test-gates.md` exist; every crate has unit and integration tests |
| C16 | CI runs the full gate of `docs/guides/test-gates.md`; every `cargo gate` tier that charter §4 or that guide names exists |
| C17 | `DESIGN.md` links and evidence resolve; its tick rate matches the code |
| C18 | `docs/plans/README.md` indexes every plan once |
| C19 | No `file:///` links; lost `/tmp` evidence is marked |
| C20 | Paths named in the charter and in the guides under `docs/guides/` exist |
| C21 | No test builds a render device in its own process; GPU tests go through the harness of charter §7 |
| C22 | `crates/gate/suites.ron` names existing targets, classes every GPU and valgrind test, and covers every tracked path |
| C23 | Smog production code calls no host transcendental function outside `crates/client/src/smog/math.rs` |
| C24 | `README.md` exists, its links resolve, and its showcase image and link use the site of `.github/workflows/showcase.yml` and the file names `showcase assemble` writes |
