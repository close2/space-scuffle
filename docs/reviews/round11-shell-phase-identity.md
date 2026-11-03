# Round 11: render proxy identity and shell audit equivalence

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Status: independent read-only review and proposed diagnostic amendment to [Plan53](../plans/53-round10-stable-transparent-shell-ties.md). No product changes, builds, native launches or changes to the declared twelve-run matrix. This does not authorize StageB ordering changes. All original reports and failed/inconclusive results remain intact.

## Finding and actual artifact evidence

`EpochEvidence::same_sources_and_order` currently compares complete `PhaseEntry` values, including `render_entity`. In pinned Bevy 0.15.3, that field can identify a temporary per-frame proxy rather than persistent scene content. Requiring equality across extraction epochs can therefore reject an otherwise identical supported draw sequence.

An independent parser inspected all six candidate epochs (3 through 8) in each original report. It removed only each sorted phase item's `render_entity` field for an explicitly provisional comparison; it did not remove view identity, main identity, ordinal, logical identity, material contents, depth bits or any other phase field.

| Original run | Items per epoch | Result of provisional comparison to epoch 3 |
| --- | ---: | --- |
| Natural01 | 210: 200 shells and 10 standard draws | Different sorted sequences in epochs 4–8. Real logical shell order varies. |
| Forward01 | 8: 2 shells and 6 standard draws | Every complete projected sorted sequence is exactly equal. Shell order is 1000, 1100. |
| Reverse01 | 8: 2 shells and 6 standard draws | Every complete projected sorted sequence is exactly equal. Shell order is 1100, 1000. |

In all three runs, every observed epoch is ready with no pending reasons; source records, mist bytes/readiness/target buffers and complete view metadata equal their respective first epoch. Matching entries by `(main_entity, draw_function, pipeline)` finds unique keys and no changed payload fields except ordinal and render proxy identity. Every queued vector and its sorted vector have exactly equal raw payload multisets after removing only ordinal. All 210 Natural01 proxy IDs, or all eight controlled proxy IDs, differ from the first epoch in every subsequent epoch.

This confirms that the two controls have a false-inconclusive dependency on raw render identity under a plausible renderer lifecycle. It does **not** retrospectively prove that each actual proxy carried `TemporaryRenderEntity`: the original observer did not record that marker or validate the visible-pair association. The original classifications therefore remain inconclusive. Natural01 remains genuinely order-varying even under the proposed narrower identity relation. No comparison here identifies the PNG's private GPU-copy epoch or establishes visual/art acceptance.

## Pinned renderer basis

The inspected primary source is the locally locked `bevy_render-0.15.3` and `bevy_pbr-0.15.3` registry source, not an assumed current upstream version:

- `bevy_render/src/camera/camera.rs:1090–1107`: camera extraction iterates visible main-world entities. If the source has no `RenderEntity` mapping, it spawns `TemporaryRenderEntity` and stores `(render_entity, MainEntity(source))` in the actual view's `RenderVisibleEntities`. Such a newly spawned proxy has the temporary marker; this branch does not insert a `MainEntity` component on the proxy.
- `bevy_pbr/src/material.rs:728–737, 861–867`: material queueing iterates `RenderVisibleEntities::iter::<With<Mesh3d>>()`, resolves material and mesh through the main identity, and copies that exact render/main pair into `Transparent3d`.
- `bevy_render/src/sync_world.rs:233–247` and `bevy_render/src/lib.rs:496`: temporary proxies are despawned in `RenderSet::PostCleanup` each frame. Their raw generational entity IDs are not persistent logical identities.
- `bevy_render/src/sync_world.rs:198–226`: synchronized entities instead have a render-world `MainEntity` component and a main-world `RenderEntity` mapping. Component removal can also replace a synchronized render entity; that separate lifecycle is not automatically admitted by the proposed temporary-only exception.
- `bevy_pbr/src/material.rs:450–486` and `render/mesh.rs:2245–2276, 2353–2396`: the standard material draw tuple's material/mesh bindings and draw mesh lookup use `item.main_entity()`, with empty item queries. This supports the bounded known material adapters; it does not prove arbitrary custom render commands ignore the render entity.

## Proposed semantic relation and required epoch validation

Retain the full raw phase record. Add explicit observed association evidence, rather than deleting or overwriting the raw render ID. A supported entry can be classified as a validated temporary proxy, validated synchronized entity, or unsupported association.

For every actual queued and sorted entry, before any cross-epoch comparison:

1. Require that the render entity exists in this render world and that its exact `(render_entity, main_entity)` pair occurs in this actual view's `RenderVisibleEntities<With<Mesh3d>>`. Correlation must use full entity bits, including generation. Do not manufacture a mapping from source order, numerical similarity, or a default identity.
2. For temporary admission, require the actual `TemporaryRenderEntity` marker. Its visible-pair mapping is the authority; requiring a `MainEntity` component would incorrectly reject the pinned temporary path. Any contradictory explicit `MainEntity` association is an error. Record the supported adapter/draw identity whose render commands have been audited; an unknown custom draw is not certified by its temporary marker alone.
3. For synchronized admission, require a matching actual render-world `MainEntity`; validate any extracted main-world `RenderEntity` mapping used as additional evidence. Missing or contradictory mappings must not degrade to an accepted temporary proxy.
4. Preserve uniqueness/ambiguity checks for the supported logical shell and draw association. Require raw queued-to-sorted payload equality as a multiset except ordinal, and unchanged view/source/association metadata inside that one sort. Sorting may reorder entries, not rewrite their source identities. Duplicate or unsupported source cases remain explicitly limited or fail according to the existing admission policy.

Across epochs, require the existing source, mist-byte/target and view checks unchanged. Compare the complete sorted vector in order, including ordinal, main identity, logical domain/entity/generation, exact depth bits (including signed zero), pipeline and published descriptor, draw function, material source/uniform/image evidence, mesh, batch range and extra index. The association kind must also agree.

Only when **both** entries are admitted temporary proxies for supported draw adapters may their raw render IDs differ. Synchronized render IDs, view/camera IDs and unknown associations retain the existing exact comparison or limitation. This deliberately conservative first amendment need not solve legitimate synchronized-entity recreation. No sorting or grouping by logical identity is allowed in this equivalence check: doing so would erase the Natural01 defect being measured.

The within-run stable main identity is not a cross-process identity. Cross-run causal comparisons still need the declared logical fixture/source descriptors, not matching process-local ECS or pipeline numbers. All candidate epochs, source revisions, raw proxy IDs and original classified results remain retained. Offline projected results are separate derived evidence, not replacements for the original reports.

## Required verification before changing classification

- A real extraction lifecycle test observes a supported visible mesh across two frames: actual temporary markers and exact visible pairs are proved, raw proxy identity changes, logical/material payload remains equal, and the amended relation accepts only that difference.
- Reject or limit a missing proxy, wrong main identity, wrong view pair, contradictory mapping, unknown custom adapter, or missing marker. A temporary entity's lack of `MainEntity` alone is valid.
- Show that queued/sorted raw-payload mismatch cannot pass by normalization. Preserve duplicate/ambiguous association rejection.
- Keep Natural01-style same-depth logical reordering unequal, even though the set of main identities and source records is unchanged.
- Changed source generation, material/uniform/image revision, pipeline, depth bits, batch metadata, view or synchronized render ID stays unequal. A negative-zero depth test protects exact-bit semantics.
- Retain callback-to-closure candidate intervals and incomplete evidence. A successful semantic comparison cannot hide pending/limited epochs or reinterpret a known screenshot as a precisely identified GPU frame.

The frozen matrix should finish unchanged. Any implementation of this amendment and its native reruns require a separately declared source checkpoint and parent approval. Existing Forward01/Reverse01 PNG differences and source proofs can inform that review; this document does not turn a provisional projection into a passed native run.

## Reproducibility

Independent script and complete per-epoch comparison results are preserved in `/tmp/space-scuffle-r11-shell-equivalence-independent01/`. The script imports only the existing RON parser; it does not import the root's phase comparison algorithm or mutate source reports.

| Artifact | SHA-256 |
| --- | --- |
| `check.py` | `a0159cba67fd7fbeb38765f8c9c008478db4ea2752135244ba7bc107635d5bda` |
| `results.json` | `ef16599ab753a1426680bd1559e6a5020e7fc8640115bc337f123b5a70b3d8ea` |
| Natural01 `09-swarm-roles-shell-order.ron` | `b5bb783ce9289a9a48b1adf1c67f2b59f48b3b23f08aa51603fc5f1afbae659f` |
| Forward01 `09-swarm-roles-shell-order.ron` | `c24ab97325cacdc2a102541f5ef003079d10e900ca05b1601c1cacd99dc6bd5d` |
| Reverse01 `09-swarm-roles-shell-order.ron` | `1dd3bfd3b42507ab0e487e09d114765b6972baf1bacda6fe8d1f35b397cbb5ff` |

This review adds to [the earlier interval/source-retention review](round11-shell-audit-independent-review.md). Its full-vector-equality description remains accurate for the inspected implementation, but the temporary-identity issue here narrows its usefulness as a visible-order equivalence test.

## Implementation slice rereview

After root approved the bounded amendment, the owner saved `association.rs`, its
CPU tests, the revised model/phase/interval code and the actual lifecycle fixture.
Independent static review found no new concrete false-acceptance defect in that
slice. This reviewer did not run the tests or GPU fixture.

The adapter resolver uses the exact public material tuple type and fallible
`DrawFunctions::get_id`, comparing the actual draw ID against the material-specific
registration. The per-view map retains full render/main entity bits and rejects
repeated/contradictory proxy mappings. Inspection requires the real entity, exact
visible pair and compatible `MainEntity` metadata; temporary admission requires
the marker. Unsupported/pending classifications are included in final interval
coverage/readiness, so an unknown draw does not gain complete admission.

`same_draw` retains every existing semantic payload field and ordinal, and omits
a changed raw render ID only when association evidence agrees and proves a
supported temporary proxy. Raw `PartialEq` remains exact. The raw permutation
check serializes the complete record after zeroing only ordinal, compares sorted
full strings and preserves multiplicity; it does not apply the cross-epoch proxy
exception inside a sort. View metadata is now also checked inside the observed
queue/sort pair. Existing inclusive interval closure and retained publication
acknowledgement remain unchanged.

The real lifecycle fixture uses the loaded production scene/material pipeline,
adds actual `SyncToRenderWorld` to one declared actor and verifies its main-world
`RenderEntity` association. For the other actor it observes a temporary source,
checks retirement after actual PostCleanup, and compares a later complete frame
with a different full proxy identity. It drains rendering before propagating
verification failure. Its manually driven renderer disables pipelined rendering
for inspectable lifecycle ownership; later native matrix results remain needed
for the normal pipelined capture path.

Two coverage suggestions were sent to the owner: an unchanged-but-unsupported
association must still produce a limited interval, and material uniform/image
content mutations should remain unequal explicitly, beyond replacing a material
with an unsupported variant. These are test-strengthening requests, not discovered
production comparison failures. Acceptance still requires the owner's actual
focused results, preserved native evidence and parent-coordinated gates.
