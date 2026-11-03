# Round7 movement layers and fixed-body verification

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Status:45/49 focused tests, corrected complete sim_core regression, strict sim_core+sim_map all-target Clippy and scoped formatting pass. Owned product source is stable. Plan45, narrow49, the single checked dynamic-core API migration and coincident fixed-body clarification are approved. No full-workspace acceptance for this source revision is claimed. The preceding round7 workspace gate accepted43 before these changes.

## Implemented behavior

- Current catalog mover policy distinguishes Air from conservative Ground/CliffJumper/CliffWalker without unit identity branches. Ground retains existing navigation clearance and continuous center traversal; Air uses full-radius map bounds and direct goals.
- Move/AttackMove admissions prepare the entire selection before order changes. One typed result preserves original command index/execution tick; deterministic retained sorted scratch deduplicates recipients before committing. Production outcomes remain separate.
- Route revalidation uses the exposed existing pathfinder segment predicate with the current clearance/knowledge request. Air discards ground funnel state. Same-layer obstacle slices govern steering and cluster arrival; self is excluded from the arrival neighbors.
- Layer-aware checks cover movement and waypoint/final snaps, pursuit and slides, hazard/retaliation flee and both push application passes. Invalid current full-radius domains and invalid Ground center terrain emit diagnostics before displacement.
- Destination-only Blink/unload validation reuses43's exact conservative footprint geometry, including tangent exclusion. Live circular blockers are checked only within the same layer. Failed immediate placement preserves actor fields/payment; no nearest-free substitution. Registered foundations reject relocation without a map transaction.
- Production's ground emergence geometry, source exemption, clockwise candidates, captured supply and queue semantics remain intact. Shared geometry extraction retains all43 tests.
- Conservative broad-phase reach includes the querying radius plus maximum same-layer radius, explicitly ceiled before the shared floor-based spatial query. Raw widened circle overlap/distance avoids squared-Q32 overflow.
- Fixed collision weights derive from foundation registration or actual immobile/unpushable flags, before enemy/priority rules. Fixed actors receive no correction and a final application guard preserves the invariant. HoldPosition remains the existing dynamic friendly-priority rule.

## Evidence so far

- `/tmp/space-scuffle-r7-movement-tests01.log`: seven new movement tests, all19 production transaction tests, existing ramp3, spell5 and flee4 tests pass.
- `/tmp/space-scuffle-r7-movement-foundation.log`: retained failing **simulation integration** evidence. A real Air dome moved `(17.5,17.5)`→`(16.125,17.5)` without its foundation moving; this prompted approved49. This was not a native renderer capture.
- `/tmp/space-scuffle-r7-movement-tests03.log`: the same foundation regression now passes; movement10 plus existing collision/immobility4 and push-priority4 pass.
- `/tmp/space-scuffle-r7-movement-tests04.log`: library16 and movement15 pass, including actual Air AttackTarget pursuit across a cliff, actual hazard evacuation, exact radius boundary arrival, no-op failed foundation placement, invalid current Ground domain after catalog mutation, and 2,000-tick mixed-layer checksum replay. Draining derived movement outcomes does not change checksums.
- `/tmp/space-scuffle-r7-movement-clippy04.log`: strict sim_core+sim_map all-target Clippy passed for that checkpoint. Further tests/API corrections require rerun before final freeze.
- Wide fixed-normal tests exercise30k/40k/50k geometry, swapped pair direction, coincident centers, tangency, negative/inconsistent supplied distance and final raw distance overflow. Dynamic-core preflight was found insufficient during independent review and replaced by one checked core, as verified in the later evidence below.

## Ownership and integration

Affected production files: new `sim_core/src/movement/{mod,geometry}.rs`; narrow movement/commands/ability/flee/collision hunks and workspace initialization in `world.rs`; diagnostics variant in `weapon_timing.rs`; shared geometry/layer vocabulary in `production/exits.rs`; `steering.rs`; module registration in `lib.rs`; public read-only segment query in `sim_map/src/pathfinding.rs`. New integration tests are `sim_core/tests/movement_layers_tests.rs`;49 additionally owns typed-result migration of three pair-helper calls in existing push/collision tests and `fixed_body_transactions_tests.rs`.

38 owns step/snapshot extraction and client consumption.45's movement outcome clear is saved beside43's clear and handed off; subsequent45 changes avoid those methods.38 must drain movement results after each actual tick with its original command index→submission sequence and generation mapping. The current mechanics report does not claim that the client adapter or its catch-up tests are complete.

## Explicit limits and open acceptance

Actual Air visual acceptance is blocked by media provenance: root independently found the shipped `flying_saucer.png` hash equals `concept_video.png` (`a1175d021d402bdd03ea13a894e567d110bf46f300401ed81253aa26362a787d`). A proper static keyframe exists but is not an authentic Walk sequence. No concept duplicate, ground atlas substitution or static-frame fake animation can establish flight visual acceptance. No native flight screenshot is accepted in this round.

Specialized cliff traversal is still conservative Ground pending authored jump/walk capabilities. Ordinary ground continuous traversal retains its existing clearance contract; stricter production/teleport/unload square geometry is not represented as universal exact swept-circle clearance.

Reusable obstacle, recipient and outcome buffers retain capacity, but this is not a global zero-allocation claim: spatial buckets can grow with new crowd concentrations and existing route/collision maps still allocate. Existing world push accumulation/capping and buff/speed aggregation are not a phase-wide checked arithmetic guarantee.49 now checks the actual selected pair branch rather than hiding unchecked work behind preflight; this pair guarantee does not extend to the remaining world aggregates.

Remaining gates:38 client outcome integration and coordinated frozen workspace tests/Clippy/fmt. Report failures without weakening assertions. No editor or source media was changed.


## Checked pair correction and final focused evidence

Root review found two unintended signs in the first checked-core translation: the moving/moving second wedge and the idle-first lateral ID tie. Both were restored to their original branch meaning and now have direct sign regressions. The first regression attempt required exact negation across independently rounded normalization/scaling; it failed at raw precision. The retained log `/tmp/space-scuffle-r7-fixed-bodies-tests02.log` records that attempt. The final test requires both correct physical signs and a justified two-raw-unit signed-rounding bound. This is not bit-identical old arithmetic: widened normal calculations can change low bits.

`compute_push_displacement` and `resolve_push_interaction` intentionally return typed Results. There is one checked dynamic priority core, not duplicate unchecked algorithms or a default-displacement bridge. The entity-only wrapper prepares both positions before writing either and respects actual immobile/unpushable flags. Its lack of map access is explicit; registered foundation enforcement belongs to world collision/placement. New Fixed/Dynamic coincidence uses an EntityId-antisymmetric axis, while legacy Dynamic/Dynamic keeps argument-ordered +X. Identical actor IDs are a typed pair-identity error.

- `/tmp/space-scuffle-r7-fixed-bodies-tests03.log`: library16, collision4, push4, and new checked/fixed transactions7 all pass. Coverage includes all dynamic branches with large representable normals, raw quantization and maximum overlap, both exact reviewed signs, atomic two-entity failure, flags, coincident reversal across both ID orders and duplicate identity rejection.
- `/tmp/space-scuffle-r7-movement-tests05.log`: movement16 passes, including actual training→Air movement across a cliff→catalog supply-price mutation→death/reap retaining the captured supply contract, and2,000-tick replay. Existing collision/push suites also pass.
- `/tmp/space-scuffle-r7-movement-produced-air.log`: isolated real production-to-flight ledger lifecycle passes.
- `/tmp/space-scuffle-r7-movement-capacities.log`: retained per-layer obstacles and recipient scratch capacities remain unchanged over repeated work after32-actor warmup. This measures those buffers only, not global allocations or throughput.
- `/tmp/space-scuffle-r7-movement-clippy05.log`: owned library/test checks passed until three lint errors in38's new snapshot tests; owner notified. This complete invocation is a failure, not an accepted lint gate; rerun remains required.
- `/tmp/space-scuffle-r7-movement-full-sim.log`: first complete simulation attempt failed on the existing stutter-step fixture requesting illegal full-radius x=0. Root-approved migration uses the actual selected radius boundary and asserts actual MovementOutcome acceptance, retaining damage/cancellation/three-tick displacement assertions. The focused fixture passes in `/tmp/space-scuffle-r7-movement-stutter-fixture.log`.
- `/tmp/space-scuffle-r7-movement-full-sim02.log`: corrected complete sim_core suite and doctests pass, process exit0. Includes38 snapshot equivalence tests.
- `/tmp/space-scuffle-r7-movement-clippy07.log`: strict sim_core+sim_map all-target Clippy passes.06 had one test-documentation backtick lint, repaired without behavior changes.
- `/tmp/space-scuffle-r7-movement-fmt-scoped.log`: sim_core+sim_map formatting check passes. Workspace formatting attempt in `/tmp/space-scuffle-r7-movement-fmt-final.log` failed because35 was saving a module that referenced not-yet-present `mist/upload_tests.rs`; owner notified. This remains a failed workspace formatting attempt until the combined source stabilizes.
- `/tmp/space-scuffle-r7-movement-fmt-final02.log`: after35 saved that real module, workspace formatting check passes. This does not replace the still-pending combined workspace test/lint checkpoint.

Root identified two further boundaries for separately planned50: unchecked aggregate movement/steering arithmetic, and registered Air foundation self-motion if mutable entity flags are corrupted to movable.49 prevents collision/teleport/unload detachment; it does not claim to implement a general atomic lift or relocation workflow. No50 product changes are included in45/49.


## Concrete next-round evidence

Plan50 is architecture-only at `docs/plans/50-round7-checked-movement-arithmetic.md`. The standalone actual-library probe `/tmp/space-scuffle-r7-movement-extremes.rs` records two failures in `/tmp/space-scuffle-r7-movement-extremes.log`: a legal50k-unit Air move on a60,000×4 map panics in the remaining squared-Q32 movement path, and a registered Air foundation with corrupted movable flags moves itself without its map footprint or a diagnostic. The probe intentionally catches the panic to record both cases; its exit0 is not passing mechanics evidence. These demonstrate why45/49 acceptance remains bounded and why neither flight visual acceptance nor a fully checked movement phase is claimed.
