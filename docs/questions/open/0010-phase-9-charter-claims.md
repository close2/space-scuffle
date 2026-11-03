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

## Answer
