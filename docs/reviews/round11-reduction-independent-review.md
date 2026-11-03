# Round 11 independent CPU reduction review

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Scope: read-only review of `mist/reduction.rs` and its mathematical unit tests against the CPU3 section and rounding/support amendments in [Plan44](../plans/44-round6-volume-mist-art-direction.md). This review did not edit product sources or run Cargo, a native client, a shader or GPU work. Round52 sources remain frozen. The admission runner and complete legal-motion matrix were still in progress and are not reviewed or accepted here.

## Concrete findings

1. **Duplicate removal lost exact original-edge provenance.** In the initially inspected implementation, equal original samples0 and1 were compacted to a reference carrying identity0. A subsequent support cut between that reference and original2 reported edge0→2. The coordinates and interpolated attributes were correct, but the promised original outgoing edge was1→2. The owner has now added a separate last-duplicate index for outgoing interpolation while retaining the earliest identity for an unchanged control. `duplicate_cut_names_the_actual_original_outgoing_edge` exercises the reported edge and fraction. Static rereview finds the correction consistent with exact-cut earliest-boundary semantics; focused execution of the changed test remains owner evidence, not a test run performed by this reviewer.

2. **Positive support could vanish in floating-point subtraction and be accepted as a singleton.** The initial `clip` computed `cut = total_arc - support` and accepted an exact cut at any original sample, including the newest. For the valid raw two-sample input position0→`f32(1e20)`, birth0→1, path0→1, radius1 and default support8, the promoted total is `1.0000000200408773e20`, whose `f64` ULP is16384. Subtracting8 rounds back to the total. The exact-sample branch therefore removed every spatial interval, and success reported zero source arc and one control. The same issue occurs for positions0→1 and radius2^-60. All input fields are finite and satisfy the reducer's public raw-input admission; this is not evidence that the current emitter produces either history during legal gameplay.

   The independent Python probe reproduces these arithmetic cases. The owner has now added an explicit `positive support cut progress` representation error and an interior-fraction check. These preserve the direct-reference policy and do not search for an inward substitute. The new `positive_support_cannot_disappear_in_cut_subtraction` test covers both scale regimes. Static rereview confirms that the described singleton-success path is blocked. This narrow guard does not turn ordinary floating-point clipping into exact real-arithmetic clipping or authorize tolerance relaxation.

Both findings were sent directly to the owner and root before this report. Neither is an optical-model criticism; they concern CPU provenance and numerical admission.

## Other contract checks

- Policy, capacity, clock and radius checks run for empty inputs. All samples are admitted before deduplication or clipping, including finite/nonnegative chronology and cumulative path checks. Geometry uses promoted `f64` coordinates before differences and squared lengths.
- Relative ages are computed by subtracting the original birth from the immutable history clock before conversion to `f32`. Candidate error checks use the returned rounded position, relative age and path values promoted back to `f64`; original birth is diagnostic provenance, not a higher-precision replacement for packed age.
- The synthesized support reference is retained separately from the rounded output. Both interval endpoints are scored. Reconstructed reference arc beyond declared support is a typed refusal, with no inward search or acceptance epsilon.
- Positive-arc candidates use cumulative source arc; zero-arc candidates use birth span, then path span. Distinct chronological attributes are not removed as exact duplicates. Genuine zero-arc segments carry no density measure or reciprocal. Positive-arc zero-chord candidates force a split; adjacent unrepresentable length/reciprocal candidates fail explicitly.
- Split selection uses normalized error and chronological ties. Forced geometry failure takes precedence and stores the selected interval's component maxima, rather than a previous interval's aggregate. A direct regression for this last diagnostic association was requested because the initial20-test set did not exercise it explicitly.
- Fixed arrays bound storage and the maximum number of split passes. The structured performance-rationale comment required by the charter was absent in the reviewed initial slice and has been requested; this is documentation work, not evidence of a measured throughput defect.

No additional concrete acceptance error was found in the inspected rounded-error or zero-arc paths. This is a bounded source review, not an exhaustive numerical proof. The full legal-command/playback corpus and integration/workspace gates remain required.

## Field admission remains separate

The successful near-closed control test deliberately records more than98% source-to-chord arc loss. It must not be reclassified as a CPU failure by inventing a new deficit threshold, nor accepted as a faithful density representation. The separate straight varying-age counterexample in [the field subdivision review](round10-mist-field-subdivision.md) shows that even exact control interpolation and zero arc loss do not preserve the proposed summed clamped-age field. [Plan54](../plans/54-round11-mist-field-representation.md) is advisory investigation only; it selects no replacement field and releases no shader or integration-budget change.

## Evidence and limits

The independent arithmetic script and JSON are retained at `/tmp/space-scuffle-r11-reduction-review01/support_subtraction.py` and `support_subtraction.json`. They use Python scalar IEEE floating-point operations and explicit `f32` packing, without importing the Rust implementation or any field evaluator. The script reproduces the cancellation condition; the Rust success/error paths are established here by source inspection, not by executing the reducer.

The same directory contains reviewed source copies and `source-inventory.json`. Those copies were captured after the owner had saved the two corrections; they are not mislabeled as a pre-fix snapshot. The owner initially reported20 focused tests passing before these fixes. This report makes no independent Rust execution, full-matrix, visual, performance or final-workspace acceptance claim.
