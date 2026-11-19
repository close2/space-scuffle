# Evidence: Retained Effect Instances (B-137)

Plan: [`docs/plans/2026-11-19-retained-effect-instances.md`](../plans/2026-11-19-retained-effect-instances.md).

| Work packages | Evidence |
|---|---|
| WP1 to WP6: instruments, base figures, the CPU work packages, the GPU aging and its identity checks (D-10) | [`effects/review/retained_decals_baseline.md`](effects/review/retained_decals_baseline.md) |
| WP7: the series of record, the GPU passes, the kernel's validation, the cost per phase and the decision gate | [`effects/review/frame_budget_b137_retained.md`](effects/review/frame_budget_b137_retained.md) |
| WP8: the independent review of the identity evidence | Plan Amendment 3 |

## Summary

- **Build instructions per frame** at 200 splashes a second: 10.10 M → 4.51 M (S) and 11.89 M → 5.88 M (P).
- **Pictures:** identical to the base through WP4. WP5 re-baselined card ties. From WP6 the decals are aged on the GPU,
  bit for bit equal to the Rust reference on radv and lavapipe.
- **§10.3:** still missed by the CPU-time verdict (S +3.69 ms, P +5.18 ms per frame against a bar of 0.70 ms). The GPU
  passes are within their 1.5 ms (+0.63 ms).
- B-137 is open, and the next step is not chosen (plan Amendment 3).
