# Round 11 independent transparent-shell audit review

Status: bounded static review of StageA in [Plan53](../plans/53-round10-stable-transparent-shell-ties.md). No product edits, Cargo execution, native run or GPU work were performed by this reviewer. StageB ordering changes remain unapproved. This review establishes neither the historical cause of the Swarm pixel difference nor visual equivalence.

## Findings and saved corrections

**Closed evidence was consumed before publication succeeded.** The initially inspected `Collector::finish` took the sole interval report and returned the collector to Idle. `AuditBridge::finish` then attached assets, and `runtime_capture::finish_audit` took the pending screenshot record before writing sidecars/report. An attachment, exact publication-size or I/O failure therefore left `save_audit_incomplete` with no interval to preserve. The PNG could exist while its candidate-epoch evidence was lost.

The saved correction keeps the classified closed report in the collector and clones it for publication. The driver similarly retains `audit_pending`, writes sources/report and updates the capture manifest before calling explicit `acknowledge_publication`. Static rereview confirms that failed attachment/publication leaves the interval available to the existing failure path. The new `publication_failure_retains_closed_interval_until_explicit_acknowledgement` regression checks failed encoding, preserved epochs/token, blocked rearming, repeat inspection and explicit release. It is a CPU protocol test, not proof of actual filesystem-fault recovery or native screenshot association. Exact-size exhaustion still deliberately forbids writing an oversized/truncated report; the plan explicitly preserves available binary sidecars and the ordinary failure manifest instead.

**Supported-material precedence could omit a second real draw source.** `source::capture` selected shell, otherwise standard, otherwise mist material without checking whether more than one supported component was attached. A transparent shell and opaque standard material on one mesh can both contribute, while the original source record described only the shell. That could conceal the opaque material's revision behind otherwise stable shell evidence. This was an unchecked coverage boundary, not a demonstrated configuration of the shipped Swarm scene.

The saved source extractor now records a limitation whenever more than one of the three supported material components is present. `Collector::finish` treats any source limitation as limited, and the capture driver rejects every classification other than stable. Thus adapter precedence no longer establishes complete coverage for this case. Unsupported future material/draw adapters are not implicitly certified by this review.

Both findings were sent to root and the owner before documentation. The owner implemented the corrections; this reviewer only inspected them. The earlier root-found unpublished-pipeline lookup, unknown-phase-association classification, partial accounting and fixture coordinate repairs were also present in the inspected source, but are not claimed as new findings here.

## Candidate interval and source retention

The inspected collector conservatively includes the latest completed ready epoch when arming, then appends every completed epoch while Armed or Closing. Completion requires consecutive epoch identities. A callback received after another extraction has begun does not make that in-flight extraction the closure: the next extraction assigns a new closure boundary, and its queued/sorted pair must complete before publication becomes available. The existing protocol test explicitly preserves epochs1,2,3 for this ordering. No order-based filtering or automatic restart discards an unfavorable candidate.

During the interval, an unavailable source, observer fault, missing phase pair, noncontiguous completion or capacity exhaustion prevents success. A source that becomes pending after arming cannot silently reenter the idle warmup path. Pending complete epochs are retained and make the report inconclusive. The runtime driver keeps ordinary failure/deadline checks and freezes presentation through Readback and AuditDrain. The completed PNG is saved before entering audit closure; the checkpoint does not advance until a stable report is published.

Asset retention is revision-based rather than handle-only: the inventory keeps immutable descriptors and binary contents, compares actual bytes with the previous revision, resets only its per-extraction lookup, and records revision indices in each source frame. Prior revisions remain available through closure and failure persistence. Changing contents under the same image/mesh/shader identity therefore changes source evidence. Reusing identical selected mist bytes in the same target buffers may advance packet sequence/staging identities without falsely implying a visible-source change; all original packet records remain in the report.

Source/order equivalence compares every candidate's source record, selected mist bytes/targets, view membership, camera matrices, viewport, HDR/color grading, lighting association and complete sorted phase vectors. Different queued order is deliberately permitted when the real engine sorter produces exactly the same sorted order; both queued vectors are retained. Unknown phase associations and known incomplete source coverage limit the claim even when the remainder is identical. These are within-run checks: process-local pipeline, asset and ECS numbers are not cross-run semantic identities.

## Remaining acceptance boundaries

- The observer records CPU sources, prepared-resource associations and selected upload bytes. It does not read private GPU uniform/texture contents or identify Bevy's private screenshot-copy epoch. A stable result associates the PNG with an agreeing conservative candidate interval, not a fabricated exact GPU frame.
- A budget/error path can retain only evidence admitted before that failure. No truncated report may be labeled complete. A final publication-size refusal preserves the failure and available sidecars, not an oversized full report.
- Stable source/order evidence is not an art-quality score and cannot retroactively prove that an earlier unobserved screenshot had that order. Actual natural/controlled native runs, complete artifacts and image inspection remain required.
- The report covers the active declared Swarm source adapters and observed transparent phases. It is not a generic certification of arbitrary custom renderer plugins or unrelated scene state.

No further concrete false-acceptance path was found in the inspected interval/closure, revision-retention or corrected known-material classification logic. This is a bounded code review, not an exhaustive renderer proof or a substitute for the owner's focused tests and root's coordinated workspace/native verification.

## Review evidence

Read-only copies of the inspected audit modules, runtime driver, tests and Plan53 are retained under `/tmp/space-scuffle-r11-shell-audit-independent01`, with exact SHA-256 identities in `inventory.json`. The files were evolving during owner corrections; this inventory is an inspection checkpoint, not a claim that every file belongs to a frozen executable. The final two-phase publication and multiple-material limitation were reread directly after root requested confirmation. No failed native/test artifact was removed or replaced by this review.
