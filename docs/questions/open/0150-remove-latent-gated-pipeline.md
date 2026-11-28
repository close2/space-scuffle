# Q0150: Delete the unused new-movement code (the `Gated` pipeline) now that the redesign has ended?

- **Asked:** 2026-11-28 (plan `2026-12-03-movement-work-on-solver` §4.3)
- **Blocking:** no
- **Default until answered:** (b). The code stays on `main`, switched off. Movement changes keep its tests green
  and do not judge it.

## Background

The redesign's contact layer was merged switched off (`contact.pipeline: Gated`, plan `2026-11-27-gated-movement`
WP1) so that it could be measured. Under your Q0147 answer it will not ship, and no further redesign round follows.
It still costs every movement change something:
- its tests (the `*_under_gated` tests and its catalog checks) must stay green;
- its `gated` section stays in `movement_rules.ron`;
- its paths sit beside today's in the solver.

The branches `gated-wp2`, `gated-wp2-eval` and `contact-layer-a` keep the full work either way.

## Options

- **(a) Delete it** in one package: the `Gated` variant, its code paths in the solver, the `gated` catalog section,
  its tests, and any checksum field only it uses. The kept branches and `main`'s history keep it recoverable.
  Every shipped world plays exactly as before, which the package proves. The older `Legacy` pipeline stays: many tests use it as a
  reference.
- **(b) Keep it switched off (default).**

Recommendation: (a). The project is a teaching reference, and a pipeline that cannot ship is a trap for readers. It
also makes every remaining movement change smaller to test.

## Answer

