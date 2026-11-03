# Q0010: Phase 9 changes to the charter and the public simulation API

- **Asked:** 2026-11-03
- **Blocking:** no
- **Default until answered:** carry out the Phase 9 blueprint as written. The charter text itself only changes in its
  last package (WP13), so there is time to object before then.

## Question

Phase 9 (`docs/plans/2026-11-03-phase-9-structure-and-docs.md`) has to make the charter describe the code as it is,
with every claim checked by CI. Some charter claims are false or cannot be checked today, so the blueprint rewords
them. Section 6 of the charter is not touched (Q0003). The blueprint's §6 has the exact proposed text.

Please say which of these you want changed (a number and "no" is enough):

1. **Function length:** a hard limit of 100 lines per production function, enforced by clippy. Tests are exempt.
   A stricter limit, such as 60, is one number, but it roughly triples the work.
2. **Player identity:** `PlayerId` stays an open `u8` on the wire. A separate `PlayerSlot` type (0 to 7) indexes
   per-player storage and replaces the scattered `.min(7)` clamps.
3. **Test-only constructors:** `SimWorld::new`, `build_world`, the battle sandbox and the catalogs compiled into the
   binary leave the production API and move to a test-only `sim_fixtures` crate. The asset baker then loads the
   catalog files at run time, so adding a unit no longer needs a recompile anywhere.
4. **Factions and armour classes become data:** they move into a new `assets/catalog/factions.ron`. Their numeric
   IDs stay equal to today's, so no checksum or replay changes.
5. **Dependency graph (§5):** the `asset_baker → sim_core` edge that exists today is kept and drawn. Two tooling
   crates (`workspace_checks`, `sim_fixtures`) are added.
6. **Throughput (§3.1):** "10,000+ ticks/second" is not measured anywhere, and not true for full battles. It
   becomes "at least real time (25 ticks/s) in every benchmark scenario", enforced by a CI job.
7. **Client boundary (§3.2):** the charter says the client only consumes `RenderSnapshot` values. In fact the client
   owns the `SimWorld` and reads it directly to admit orders and place buildings. The text would say so, instead
   of the code being changed to match the old text.

### Update (2026-11-03, Phase 9 WP13)

The charter text is now committed (`0336642`), so you can read it in `AGENTS.md` instead of the blueprint. Each
rule carries a tag (`[CI: ...]`, `[Review]`, `[Procedure]`), and the new §9 lists the checks C1 to C21. Where the
committed text differs from the blueprint's proposal:

8. **Allocation claim (§3.3):** "battle sync systems allocate nothing in steady state" is tagged as a review rule,
   not a CI check, because the counting-allocator test waits on your answer to Q0011.
9. **Test table (§4):** two more rows, `sim_fixtures` and `workspace_checks`, so that every crate has one; the
   procedural-output rule of §1.2 cites `bake_integrity_tests.rs`, which is where it is tested.
10. **Gate commands (§4.3):** written with `--locked`, as CI and every agent already run them.
11. **§7 and §8** are exactly as they are on `main`; the blueprint was written before §7 was rewritten for the GPU
    test harness.
12. **Tick rate:** `sim_types::TICKS_PER_SECOND` is a `u16`, not the blueprint's `u32`, because every stored
    duration is a `u16` tick count and a `u16` converts to the float and integer types without casts.

Say "no" to any numbered item and the text goes back to what it said before Phase 9, or to what you prefer.

## Answer

Owner (2026-11-03): Agreed (all numbered items approved as committed).
