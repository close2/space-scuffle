# Q0092: May unit contact be redesigned around one push-based solver, with the deviations it needs?

- **Asked:** 2026-11-20
- **Blocking:** no
- **Default until answered:** a planning agent writes the full plan for the redesign, including an evaluation harness
  that does not depend on chosen seeds. No behaviour change lands, and no new movement rule plans start meanwhile.

## Background

You asked whether a different, simpler algorithm would serve better, and whether letting units push or overlap
slightly would help. The survey is `docs/evidence/2026-11-20-movement-survey.md`; every claim in it has a source.

- **The global layer is sound and stays:** the navigation mesh, TA* and the radius funnel. Their bugs are measured
  against an exact oracle on thousands of random layouts.
- **The local layer is the problem.** It has about 25 special-case rules and 29 tuning constants. Friend contact is
  enforced in three places with three different models: a hard velocity limit when a unit proposes its step, the
  surround-walk mode machine on top of that, and a separate soft collision pass after the move. Every class of
  back-and-forth we measured (R-A to R-F) is either a disagreement between two of those models, or a body switching
  class between ticks.

The recommendation is the published consensus:

- **Intent:** each unit wants to move along its route, with arrival braking and one side rule for oncoming or
  crossing bodies.
- **Contact:** one deterministic position solver pushes overlapping bodies apart, weighted by the existing push
  priorities.
- **The concave:** an attacker picks a free slot on its weapon-range arc, instead of walking around a crowd in a
  mode.

Precedent: position-based crowds, Continuum Crowds, Supreme Commander 2, 0 A.D. (fixed-point lockstep), Factorio,
and community accounts of StarCraft II itself. It removes about 15 to 20 of the 25 local rules, and several
classes of back-and-forth cannot occur at all.

## Deviations it needs

- **D-α:** friends may overlap briefly, within the solver's tolerance: at most about 5 % of the radius sum while
  moving and 1 % at rest. Never with enemies or terrain. The tolerances are data.
- **D-β:** moving friends push each other by priority. Sources disagree on what StarCraft II does, so it is a data
  switch.
- **D-γ:** the concave is reached by choosing a slot, not by a walk mode. Q0074 described what is visible, not how
  it works.

Every battle and golden replay would change, so this is a full plan with a staged migration behind a data switch.
It would be judged on stated properties over generated scenarios and held-out seed ranges, not on fixed battles:

- no oscillation, counted by amplitude;
- progress;
- no deadlock where there is room;
- chokepoint throughput that grows with gap width;
- mirror matches near 50 %.

## Options

- **(a)** Yes: plan and build the redesign with D-α, D-β and D-γ as sanctioned deviations (recommended).
- **(b)** Plan it, but come back with the plan and its harness results before any behaviour change lands.
- **(c)** No: keep the current local layer and continue fixing it rule by rule.
