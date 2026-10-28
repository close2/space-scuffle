# Owner provenance and transport coordination audit

Read-only review during the round9 workspace gate. No source edits or builds were run. Plan40 changes render provenance; plan51 changes authoritative carrier identity and cargo lifecycle. The two changes must not be treated as one checksum migration.

## Material scope conflict and root's narrow resolution

`crates/client/src/editor/playtest.rs:185` reads `instance.0.team_id != TeamId(0)` and imports TeamId at22. Removing `BubbleRenderInstance.team_id` makes this existing editor consumer fail compilation. Retaining a second deprecated ownership field would violate40's single-source contract and permit disagreement. Root resolved this as an exact mechanical shared-API adapter only: remove the TeamId import and change the comparison to `instance.0.owner != PlayerId(0)`, using the already imported PlayerId. This preserves actual owner0 selection behavior; it authorizes no editor feature, logic change, refactor or development. Preserve all other preexisting dirty hunks. No edit has been made, and no code is released before the gate and post50 preservation.

## Shared hunks and proposed handoff

| Source area | Plan40 ownership | Plan51 ownership |
| --- | --- | --- |
| `sim_types/src/snapshot.rs` | Rename bubble and all five event-variant provenance fields to `owner: PlayerId`; docs and actual round-trip tests | No snapshot layout change; consume final40 fields |
| `sim_core/src/world.rs::write_render_snapshot` | Bubble constructor currently around3045 copies exact Entity.owner | Add loaded omission at entity-loop entry; preserve38 clear/reserve/event-drain behavior and capacities |
| `world.rs::step_weapons_and_projectiles` | Event constructors around2491 and2599–2636 copy attacker owner | Target-snapshot/dispatch and three splash loaded guards; no change to40 event owner values |
| `world.rs::reap_dead_and_emit_shatter` | Shatter constructor currently2967 uses actual dead owner | Replace single-entity reap with prepared carrier groups; suppress passenger shatters, preserve actual carrier owner without looking it up after removal |
| `sim_core/src/checksum.rs` and `checksum/encoding.rs` | No encoding/schema edits | Sole Option carrier encoding, version3→4, exact canonical bytes and mutation tests |
| `client/src/battle_pipeline/timeline.rs::compatible` | Replace team equality with owner equality at292 | No new interpolation policy; membership disappearance/reappearance follows38's existing displayed-tick boundary |
| `client/src/runtime_capture/driver.rs` | CombatProof identity fields, contact tuples/matches596–610, current-proof checks and visible-owner set966 migrate to PlayerId | Future cargo capture adapter only after separate review; retain real source/victim/contact correlation |

Prefer40's provenance conversion before51 edits overlapping constructors. If51 group extraction lands first,40 must migrate its final emitter module rather than reintroduce the old world shatter block. Only one owner edits each of those hunks at a time. Keep50's movement math/proposals unchanged; both rounds preserve38's output-buffer and event ordering contracts.

## Authoritative checksum and fixture risks

`checksum/encoding.rs:210` already encodes authoritative Entity.owner; projectile/hazard owners are encoded separately. `checksum.rs:15–17` explicitly excludes transient renderer events and snapshots. Renaming derived output to PlayerId neither changes owner bytes in simulation state nor requires a checksum version bump. `checksum_state_tests.rs:684` constructs an excluded visual event:40 owns that constructor spelling only; preserve the assertion that adding/draining it leaves the authoritative checksum unchanged.51 owns the existing loaded-field mutation test around173, sole carrier field encoding around248 and schema-header change. Do not rewrite expected authoritative hashes to accommodate40 without explaining an actual simulation-state change.

Plan40's requested ownership-change presentation test should mutate a **presentation snapshot fixture**, or instantiate a fresh scenario with authored owner references changed before construction. Directly changing a live Entity.owner across players while leaving43's captured ledger/resources untouched is not a supported ownership-transfer transaction and can corrupt supply accounting.51 will additionally reject cross-owner stored carrier links. Neither round implements gameplay ownership transfer; tests must not accidentally depend on it.

Plan51's before/load/unload image checks must align with38's timeline: intermediate presentation uses the preceding tick's discrete membership until the display boundary. Use the actual exact-current/finished displayed tick and record it when proving cargo disappearance or return. Do not weaken snapshot omission, and do not force a new interpolation policy merely to satisfy an immediate screenshot taken at arbitrary alpha.

## Consumer/test inventory

40 must migrate direct consumers in bubble_shader, indicators, vfx, scene_controller previews/swarm, lab_ui, runtime_capture/driver, native_input_probe/pipeline and battle_pipeline/timeline. Mist's actual wake parameters do not acquire a new tint policy: its capture fixture at `mist/capture.rs:464` and unit fixture at `mist/mod.rs:625` only construct snapshot records. Coordinate those narrow initializers with35; preserve shader/storage layouts and tuning. Preview role1/2 must be mapped deliberately to owner0/1 through40's authored roles rather than mechanical numeric preservation that would swap current intended colors.

Actual tests requiring constructor migration include `sim_types/src/snapshot.rs` unit roundtrip, `sim_types/tests/snapshot_serialization.rs` all five visual events, `sim_core/tests/checksum_state_tests.rs`, client `headless_tests.rs`, `combat_light_shadow_tests.rs`, `round2_visual_tests.rs`, and38's `battle_pipeline/{driver,timeline}/tests.rs`. `sim_core/tests/snapshot_output_tests.rs` compares reusable/compatibility output and verifies pure writer/capacity/membership semantics; preserve it and add cargo cases without replacing those assertions. No persisted external snapshot reader was found in the inspected repository; current concrete serialization coverage is RON roundtrip tests. That is not proof that outside embeddings do not exist. Reject ambiguous old `team_id` input explicitly; do not add guessed aliases/default neutral.

Runtime combat capture currently compares source/victim identity and actual event contact. PlayerId migration must preserve those strict conditions and visible distinct-owner checks, not compare colors or infer alliance. Event provenance must survive source removal;51's carrier-group death makes post-removal world lookup particularly unsafe. Sparse supported owners, owner0 and neutral sentinel are distinct from legacy TeamId's neutral0 convention.

This audit and the exact mechanical adapter resolution were communicated directly to the40 owner. No new authoritative-checksum risk was found beyond accidentally changing state in presentation tests, mixing51's version bump into40, or reconstructing event ownership after entity removal.
