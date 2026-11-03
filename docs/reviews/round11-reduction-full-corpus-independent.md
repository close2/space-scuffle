# Round 11 independent CPU reduction corpus audit

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

The finalized experiment is fully accounted for, and its representation admission
**failed**. Independent read-only inspection found no missing/duplicated window,
aggregate-count mismatch, changed preserved input, or retained checksum-provenance
inconsistency. This is not a reducer pass, visual approval, independent Rust replay,
performance result or workspace-gate result.

The owner explicitly confirmed process closure before this audit read the final
summary/window stream. Original output remains at
`/tmp/space-scuffle-r11-reduction-full01`; the recorded process exited **1** after
1,583.097 seconds. No run was retried, output replaced, policy relaxed, product
source edited, build launched or GPU used by this reviewer.

## Complete accounting

I independently reconstructed the executable and unsupported case inventory from
the copied unit catalog and declared family/capability rules. The 42 mobile
archetypes yield 679 executable cases and 47 explicitly unsupported optional
families. The four authored mover classes are Ground, Air, CliffJumper and
CliffWalker; this inventory does not infer flight or special cliff behavior from
those names. Generic Blink representatives are selected per authored mover class,
as declared by the corpus, rather than every roster identity.

The exact Cartesian product is **679 × 4 speeds × 3 rates × 2 phase offsets =
16,296 windows**. Every expected identity appears exactly once in `windows.ronl`.
The check compares the full case record, speed, display rate and phase; matching
only a total count would not have sufficed.

| Accounting item | Independently checked result |
| --- | ---: |
| Passed windows | 13,018 |
| Failed windows | 3,278 |
| Attempted display frames | 6,885,592 |
| Completed display frames | 6,885,592 |
| Incomplete declared display horizons | 0 |
| Generated authoritative ticks | 923,440 |
| Additional no-cast control ticks | 4,128 |
| Artifact bytes before final summary | 26,233,191 |
| Largest encoded evidence record | 3,218,330 bytes |

Frame totals are sums of actual per-window counters, not requested cadence times
horizon. Each record obeys its frame bound and completed/attempted ordering. The
full generation tick total matches the declared horizon and speeds. Control ticks
match the Blink controls executed through and including each actual cast tick;
they are not incorrectly counted as full post-cast control runs.

Failure categories overlap within a window. The exact disjoint partition is:

| Categories observed in one window | Windows |
| --- | ---: |
| None | 13,018 |
| ControlBudget only | 136 |
| ControlBudget and Representation | 2,562 |
| Representation only | 190 |
| Discontinuity only | 240 |
| Coverage only | 132 |
| Coverage and Producer | 18 |

Thus the category totals are ControlBudget 2,698, Representation 2,752,
Discontinuity 240, Coverage 150 and Producer 18. Adding those category totals would
double-count windows. All Coverage/Producer cases are in the Pursuit family. The
producer cases include command refusal after the subject dies; they cannot be
presented as successful legal-motion reducer evidence. No failed window is hidden
inside the passed count.

## Retained evidence and checksum scope

For every observed category, its `first-<category>.ron` summary exactly matches the
first corresponding window in the complete stream. Its frame-record count equals
that window's completed counter. There are five category files but four distinct
full payloads: the earliest ControlBudget and Representation occur in the same
window, and their files are byte-identical. This is intentional shared evidence,
not two independently executed experiments.

Every retained authoritative trace has consecutive input/output tick identities,
and every retained post-step snapshot checksum equals its separately recorded
direct world checksum. Initial cached published checksums are explicitly distinct
from initial direct checksums; the audit preserves both rather than falsely
requiring equality before an authoritative tick has refreshed the cache. All
retained diagnostic lists are empty, including the Producer case whose failure
is an ordinary command outcome/coverage issue rather than a fatal simulation
diagnostic. These checks establish consistency of retained provenance. They do
not independently execute Rust simulation or prove platform-wide determinism.

Only the first full window per failure category retains complete tick/history
records. Other windows retain their exact summaries. The report does not claim
that all 6.9 million display records or all generated authoritative outputs are
individually persisted and independently rechecked. The corpus's reused output
playback is not mislabeled as a new authoritative simulation for every cadence.

The retained first capacity refusal also has an independent exhaustive
[source-vertex capacity diagnosis](round11-reduction-control-capacity.md): its
22-point frame requires at least 19 original source-vertex controls under the
unchanged metrics, exceeding the 17-control budget. That bounded result does not
prove arbitrary-control impossibility or field fidelity. Representation/support
rounding refusals, Blink display-history bridges and pursuit coverage failures
remain separate evidence and require separate architectural decisions.

## Preserved provenance

The independent post-run inventory rehashed all **489 preserved entries** under
`tmp/verification/reduction-r11-01`, including the sole declared internal
`crates/client/assets -> ../../assets` link. The link's literal target, target-byte
hash and containment in the preserved asset root match the manifest; no arbitrary
external link was followed. Every regular file's byte count and SHA-256 match.
The independent source-derived case list and unsupported reasons exactly match
`inputs.ron` in order.

All six copied runtime inputs are byte-identical to their declared preserved-root
files. I recorded independent SHA-256 hashes in addition to the artifact's declared
BLAKE3 strings. No Python BLAKE3 implementation/tool was installed, so this review
explicitly does not claim independent recomputation of those BLAKE3 strings.
Direct byte equality and the independently rehashed source inventory establish
the checked input identity without inventing a hash result.

The actual executable was independently rehashed after completion:

- Binary SHA-256: `47bd1468033d0256493090cbdce0e8b93c573176ba22e30afc76fd9763488391`.
- Preserved inventory identity: `2986e01d0d658eb4a896a952c1f7c91aefd9efdec8492e2e5181d6be5b9ab635`.
- Snapshot manifest SHA-256: `c11e66a7192ac3d01b54a5f65aadd58a8b2dcb4ff2d0bba6e37bb881fbe034a6`.
- Complete window stream SHA-256: `74ef41ffd95c0434a40561c1ea8fdc5bbb1cd1fe4b603d923455badc84d52b7c`.

The exact recorded command, release profile/toolchain and original process status
remain in `tmp/verification/reduction-r11-01-artifacts`. The original run's
`field_and_art_accepted` is false, and `inputs_unchanged` is true; the independent
checks support the latter without changing the former.

Independent scripts and results are separate from the owner's audit:
`/tmp/space-scuffle-r11-reduction-full-independent01/`.

| Independent artifact | SHA-256 |
| --- | --- |
| `audit.py` | `f26a1c7e9369e74049ff71a8946217d18c2a89432a2b8b49c64bc3a603cf4b7b` |
| `results.json` | `63a7a6cca351ae4ac0d4ec7ca4603e5d268f37cb27bedfde95182db3e0071e5c` |
| `provenance.py` | `9a6c29060b7c549770937b28fb23f7c71a81993d6b440c7a75fcda641b2fc706` |
| `provenance.json` | `53acd109e383e81708b82769864a2b8a83da41c5ff00f79a3c3dcba76aaeae8c` |
| `partitions.py` | `d691dc65bdacbe6770680cb477e8ad67a51f12a00680f9c03dd97b87e6a1850c` |
| `category-partitions.json` | `1ce26f3535aad02da7ce9de5f07f8211e9c49c52f76dc3d07f46464820d8fa52` |

The audit supports using this closed experiment as an immutable failed baseline.
It supplies no permission to expand control budgets, move support cuts inward,
replace authentic-media requirements, or treat control approximation as optical
admission. Plan55 remains a separately reviewed discontinuity correction; the
current full corpus cannot be declared accepted merely because its accounting is
complete.
