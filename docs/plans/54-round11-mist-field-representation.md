# Round54: choose a mist field with explicit subdivision semantics

Status: advisory architecture and small independent offline mathematics only. Root approved this investigation, not a product field, shader, evaluator, asset, packet layout, quadrature budget increase or renderer change.52 remains frozen pending the combined gate. This document does not edit or supersede44's approved CPU Stage3 scope. Any subsequent field implementation requires that stage's real retained-source artifacts, a reviewed model decision and explicit root release.

## Goal and existing rejection

Resolve the demonstrated straight varying-age subdivision discrepancy without pretending that position/time/path control accuracy proves density fidelity. The current proposed field sums per-segment axial windows multiplied by attributes evaluated at each segment's **clamped** projection. On the retained mathematical line of length2r with affine normalized age1→0,32 source edges reduce to one with zero control error and zero arc deficit, yet mass changes by−5.176% and the sampled central broadside transmittance changes0.836810→0.996490. See [independent derivation and provenance](../reviews/round10-mist-field-subdivision.md) and [44's exact field proposal](44-field-oracle-review.md).

The error is in the model/representation relationship, before packing, shader execution or view-ray quadrature. A new model must be evaluated as a new declared field on **both** source and reduced representations; quietly redefining only the reference to bless coarse output is not a fidelity repair. No current ribbon defect, legal command trajectory, rendered image or visual acceptance is inferred from this mathematical example.

## Decisions before product code

1. **Candidate A: affine attributes across cap support.** Keeps the present per-sample/per-segment evaluation structure and has an exact straight globally-affine subdivision identity. It changes the unreduced source field too, retains geometric mass-loss failures and can expose narrow age features. It is an artistic field candidate, not a physically justified fix preserving the old32-edge field.
2. **Candidate B: attributes inside a source-arc convolution integral.** Has exact partition additivity and a useful unmodulated mass identity. It provides a principled reference candidate, including a continuum interpretation of finer old clamped segments. Its full nonlinear evaluation needs additional work not admitted by the104×16 proposal.
3. **Recommended next experiment:** compare both explicitly named fields offline against exact invariants and then the actual CPU3 source corpus. Do not choose A because it is cheaper or B because its equations look more physical. Select a model only after field fidelity, temporal behavior, finite arithmetic, sampling and cost are separately reviewable. The existing near-closed example currently prevents a blanket fidelity claim for either candidate under the approved geometric reducer.

Primary context: convolution-surface research defines a skeleton field by integrating kernels and discusses closed-form evaluation for particular families; that establishes the relevance of additive integration, not a ready-made efficient evaluator for our age-dependent radii, fades and path sine. [Hubert and Cani, polygonal curve skeletons](https://www.sciencedirect.com/science/article/pii/S0747717111002197), [Jin and Tai, polynomial weighted kernels](https://researchportal.hkust.edu.hk/en/publications/analytical-methods-for-polynomial-weighted-convolution-surfaces-w/). Beam transmittance depends on integrated extinction, and multiplies over adjacent ray intervals; control interpolation is not its error criterion. [PBRT, Transmittance](https://pbr-book.org/4ed/Volume_Scattering/Transmittance).

## Shared definitions and immutable contract

For one nonzero edge, use p(s)=p0+d*s,0≤s≤ell, unit direction d, projection t=dot(x−p0,d), and perpendicular vector v=x−p0−d*t. Let h=0.5r, R0=0.5r and S be clamped cubic smoothstep. Preserve the common startup factor computed from **original clipped source arc**, actual packet clock, lifetime, source path phase, selected owner/generation and current real-view lighting policy. Source edges are the admitted retained clipped source, not an invented smoother curve. Zero-measure edges contribute zero without an axis/inverse evaluation, while their distinct temporal/path endpoint obligations remain CPU3's responsibility.

The common axial window is

```
A(t,ell) = S((t+h)/(2*h)) - S((t-ell+h)/(2*h)).
```

Its derivative kernel representation is

```
w_h(q) = 3/(4*h) * (1-(q/h)^2), for |q|<h; otherwise0
A(t,ell) = integral_0^ell w_h(t-s) ds.
```

w has inverse-length units, is nonnegative, integrates to1, and has compact support. This identity follows by differentiating S; no external formula is assumed to match the proposed field automatically. Radial K(q)=(max(1−q²,0))² retains its finite disk support.

Keep44's exact authored age-to-radius law, fade, tint, world basis, boost and modulation definitions for the initial comparison. Any different value/function is a separate experiment and artifact version. Invalid source ages, nonfinite inputs, malformed clocks and phase values remain admission errors. The definitions below distinguish valid mathematical extension from accepting corrupted authored values.

## Candidate A: affine extension, bounded lifetime envelope

For every admitted nonzero segment, define an **unclamped** projection parameter u=t/ell. Extend age and actor path affinely:

```
a_ext = a0 + (a1-a0)*u
phi_ext = phi0 + (phi1-phi0)*u.
```

Define the density lifetime envelope explicitly: F_ext(a)=0 for a≤0 or a≥lifetime; inside use44's exact F(a). This is the field's support rule for extrapolated values, not a source-input clamp or an error repair. Compute/check finite extension arithmetic before classifying it. For active ages, derive radius and tint from age exactly as44 specifies, then evaluate the same axial/radial factors with phi_ext. Outside the density envelope, skip modulation and color because that contribution is mathematically zero. Do not globally clip space to one segment's endpoints: the axial caps remain, with their declared attributes.

For an oriented straight line with globally affine age and path, every subdivided segment evaluates the same a_ext(x),phi_ext(x), radial axis and nonlinear derived attributes at each x. Factor these common terms out of the sum; adjacent A windows telescope. Thus any number of subdivisions yields the same exact ideal field, including nonlinear radius/fade/tint and the path sine. This requires consistent orientation, identical h/r/style/clock and the same affine attribute functions. It does not apply to a bend, reversal, different attribute slopes, a discontinuous coincident temporal endpoint, or geometric curve simplification.

For the retained2r affine example, A's source and reduced fields both equal the old **one-edge** result. A therefore does not recover the old32-edge appearance. The new source semantics must be openly approved if A proceeds. At fixed constant attributes its mass remains proportional to represented arc; the0→0.04r→0.001r example still loses78/79 on reduction.

Finite support stays within the existing axial extent[−h,ell+h] and radius≤0.75r whenever density is nonzero. Nevertheless extension magnifies attribute differences by up to h/ell in the caps. A positive finite reciprocal is insufficient to establish acceptable error: tiny chords with finite endpoint differences can generate steep age/path gradients or ill-conditioned phase reduction. Validate actual operation order/rounded slopes, record extension extrema and failures, and test adjacent representable values. Do not replace an overflow with zero contribution, clamp slopes or discard short legal edges.

For an affine age spanning a full lifetime over geometric length D, the head rise spans0.06D. At D=2r this is0.12r, already below the proposed0.5r minimum-feature rationale; at D=0.13r it is0.0078r. This exists with modulation disabled. End-on/oblique rays can encounter it even though broadside radius is≥0.5r and axial caps have widthr. No sine-frequency bound or successful straight subdivision identity proves104 steps at0.125r sufficient. Slow motion, newly short histories and actual minimum-speed behavior must enter the corpus; no assumption of a minimum travel length is justified by radius alone.

A's path gradient continues through its caps, unlike44's current clamped path. Its combined-mode frequency and wrapped-phase admission must therefore be reviewed over the whole nonzero support, not reuse the old zero-cap-derivative argument. Its ideal algebraic identity does not guarantee bitwise equality after independently rounded axes, reciprocal lengths, endpoint ages and phase anchors.

## Candidate B: source attributes inside the integral

Let a(s),phi(s) be the admitted affine source attributes on each edge. Define the source-weighted segment field

```
rho_B,j(x) = startup * boost * integral_0^ell [
    w_h(t-s) * K(|v|/R(a(s)))
    * (R0/R(a(s)))^2 * F(a(s)) * M(x,phi(s),clock)
] ds.
```

Integrate the density-times-albedo numerator with the same source weights. The eventual sample's color is the numerator divided by positive total density, exactly as a mixing operation; this never divides/normalizes density itself. Modulation's world-space factors remain functions of x, but its path phase belongs to the source point s, rather than the query's clamped/extended projected phase. That is an explicit model change too.

Splitting one straight edge while preserving its exact geometry and affine source attributes simply partitions one integral. Additivity is exact for arbitrary nonlinear F,R,tint and M. This holds independently of segment lengths and cap overlap. For a piecewise-linear curve, subdividing any existing edge preserves the field. Merging a bend or replacing a different attribute function changes the integrand and has no invariance guarantee. A tangent change at a real source vertex belongs to the declared oriented-kernel field; this model is not an isotropic capsule convolution or a fluid simulation.

With modulation1, integration over all space gives the independent identity

```
Mass_B = startup * boost * (pi*R0^2/3)
         * sum_edges integral_0^ell F(a(s)) ds.
```

Proof: the axial kernel integrates to1; the radial kernel integrates to pi*R²/3; the inverse-square expansion cancels R². Fubini applies because the admitted nonnegative finite-support integrand is integrable. Overlapping edge support does not invalidate addition. This is model mass per geometric arc, not conservation of physical particles per elapsed time or actor-center distance. With modulation enabled there is no such simple mass equality, although positivity/bounds remain.

For the retained L=2r, age1→0 example, integral_0^1 F(z)dz=0.03+0.39+0.275=0.695, hence normalized mass is pi*1.39/12=0.36390114904081766. The previous finite32-edge field is not exactly B either. As edge size tends to zero along a fixed straight affine source, A_j approaches w_h(t−s)ds and clamped endpoint attributes approach source attributes. Under these bounded continuous attributes the finite sum tends to B. This explains the continuum connection without falsely relabeling B as the exact old discrete source field.

B's support fits the same conservative cylinder extent and radius bounds. Its spatial attribute values are attached to source integration points; short-edge age slopes do not directly become query-space radius/fade slopes. Kernel derivatives therefore offer spatial regularity bounds independent of h/ell extension. This is promising, but not proof that104 steps suffice: different tangents, overlap, world modulation, finite support boundaries and total arc still enter quantitative derivative/integration bounds. Nor does it make source quadrature easy: steep F(a(s)) or phase in the integration variable can require many source samples.

B also retains the near-closed geometric mass-loss counterexample: constant attributes make mass proportional to represented arc, so replacing0.079r by0.001r still loses78/79. Partition invariance is not invariance under curve simplification. Neither candidate clears that separate fidelity gate.

## Cost and representation consequences

The original worst-case work is104 view-ray steps×16 segment visits=1664 segment evaluations per affected fragment before material/light/compositing work. A keeps this outer structure, but changes arithmetic/conditioning; no GPU speed or packet-fit claim follows. Source/frame values must be rounded and packed explicitly before any actual comparison. Existing layout proposals predate the full field functions, so this plan promises no free uniform lanes.

B's varying radius introduces inverse powers of a nonlinear age function; F is piecewise nonlinear; the path modulation includes sine inside the integral. Polynomial-weight closed-form papers do not demonstrate an inexpensive exact solution for this combination. Removing expansion or path modulation to obtain simpler integrals would be an explicit art/model change, not an implementation optimization.

A fixed K-node source quadrature costs up to K×1664 kernel/attribute evaluations: K=2 gives3328, K=4 gives6656 and K=8 gives13312. That is additional work despite unchanged outer loop counts, and is **not authorized** by104×16. The listed counts are operation opportunities, not measured GPU time or a claim all work scales linearly. Source-domain clipping to t±h and shared world-sine/light terms may reduce actual work, but they require separately reviewed implementation and measurement. Splitting quadrature at age/radius/support knots adds subintervals and must count every resulting node; do not advertise K while secretly evaluating3K.

No per-fragment adaptive unbounded quadrature, dynamic node-count fallback, hidden extra segment records or density clamping is allowed. The offline reference may use a larger bounded integration budget with explicit nonconvergence; it is never benchmarked as production cost. An eventual fixed-K experiment must declare its total source-node ceiling, full operation/layout accounting and failure behavior before product authorization. Legal histories failing its accuracy gate require model/cost review, not silent production fallback or newly excluding those histories.

## Independent initial discriminator, already performed

Artifacts: `/tmp/space-scuffle-r11-field-representation54/experiment.py` and `results.json`. The self-recorded script SHA256 is `8c133f562d1c73e57822e0f4429ea33f0a8551affcf51752e8ca0bcb461c5aa3`. This scalar Python experiment is independent of product code and reads no live renderer/source history. It checks12 fixed axial points and one direct broadside ray; it is not a legal-motion, full-field, numerical-certification or performance run.

- A's32-edge and one-edge columns agree to at most5.56e−17 at the12 fixed points. At x=1.99151611328125r both equal0.003907078234923168r: it changes the source to the coarse result, as predicted algebraically.
- B uses composite Simpson source integration on explicitly clipped support and known head/tail knots. At16/32/64 intervals per partition, source-versus-reduced column differences decrease2.53e−8→1.58e−9→9.90e−11. The final values are0.221153741863 and0.221153741962r; corresponding T is approximately0.8195184497. These are convergence observations, not certified intervals or an approved production node count.
- Both retain the exact constant-attribute near-closed mass ratio1/79. Recorded head-rise widths are0.0078r,0.03r,0.12r and0.48r for D=0.13r,0.5r,2r and8r. Smaller-D values describe feature width, not opacity with an omitted startup factor; the implemented offline formulas include the common startup.

This discriminates semantics and cost questions. It does not decide which different field looks right, establish a universal numerical tolerance or accept either candidate.

## Next bounded offline verification stage

After CPU3 produces its unchanged declared artifacts, consume them directly: retained original samples/cuts/clock/path, rejected histories, chosen controls, rounded packet candidates and actual command/timeline provenance. Keep mathematical tests separate from actual legal-source tests. Retain current Blink interpolation/reset findings; neither field may erase a bridge or introduce teleport markers in this scope.

A standalone verification script/module outside product sources may evaluate three explicitly labeled fields: current proposed clamped model, A, and B. No production asset loader, shader or renderer imports are added. Inputs and results carry model version, exact immutable field values, source/control hashes, precision/rounding mode, coordinates/units and reference work limits. Preserve all failed/nonconverged cases. Any later repository evaluator needs its own approved file/API/error blueprint.

Required mathematical cases:

- Constant attributes: axial telescoping, analytic radial/cross-sectional mass, arbitrary straight subdivisions, short edges, unequal lengths and rigid transforms for the unmodulated model.
- The exact retained2r linear-age case, with age/path/radius/tint/modulation toggled one at a time for attribution. No model may be compared using a different startup or clock.
- Piecewise age/path slopes, exact and near-zero chords, distinct temporal endpoints at coincident positions,90-degree/acute turns, reversals, crossings and the retained near-closed mass failure.
- A extension extrema and adjacent-float perturbations; source/reduced rounded values are evaluated as actually packed, not reconditioned with cleaner axes or ages.
- B partition additivity, its unmodulated mass formula and zero-length measure. Compare declared fixed-node candidates against a separately bounded refined reference; reference nonconvergence is explicit, not a favorable omitted case.
- End-on, broadside, oblique and tangent rays through the thin head-rise examples. Report field error, optical depth, T and premultiplied color separately.104-step integration is assessed against the **same** chosen field, not used to disguise source/reduced error.

Required actual-source/temporal cases follow44: real25Hz TickBundle motion and presentation at30/60/144Hz/speeds0..4, turns, stop/restart, slow/near-stationary emission, support clipping, control split/merge, ring eviction, age expiry, owner reset and selected-packet retention. At fixed world probes compare each source field's change and the additional reduced-field error change. Artificial subdivision should be invariant where mathematically promised; real curve/attribute changes need resolved comparison, not an invariance assertion.

Neither C1 axial support nor exact artificial subdivision guarantees smooth eviction. B removes the measure of an evicted arc weighted by its actual age; a discrete young-arc eviction still changes the field. A can additionally change extended attribute gradients when topology changes. No source repair or history extension is bundled into field evaluation.

## Rejected shortcuts and deferred alternatives

- **Nearest-segment/max fields:** change the sum into a selection rule, introduce tie/transition behavior and do not satisfy additive subdivision/mass semantics. Not an authorized repair.
- **Divide by summed windows/segment count, arc-ratio rescale, or opacity compensation:** changes physical/model density, hides loss and can create singular/unstable limits. Matching one total mass would not preserve columns or temporal density. Rejected.
- **Tighter position/time tolerance or keeping a midpoint only for this line:** the retained failure has zero interpolation error. Arbitrary new controls do not fix the model and violate the bounded reduction decision.
- **Clamp extension gradients, phase, or legal age variation:** conceals valid input features to satisfy a sampling budget. Rejected without a separately justified game/presentation contract.
- **Constant attributes per edge:** reintroduces subdivision-dependent sampled weights; it is a quadrature approximation, not an exact general fix.
- **Polynomial/moment approximations or cached3D fields:** potentially principled approximation families, but require independently bounded error, representation sizes, lifecycle/bandwidth and cost. They are not silently substituted in this round. Published analytical kernels are not proof that our nonlinear full model fits existing lanes.
- **More steps/nodes or a different transparency algorithm:** changes separate budgets/pipelines and cannot repair the demonstrated density semantics by itself. Not authorized here.

## Approval and acceptance boundaries

The next decision is a named model experiment, not automatic shader implementation. Root and44's field owner must approve source semantics, field version/functions and a concrete bounded evaluator/packing blueprint after seeing CPU3 and offline failures. Source/reduced fidelity, packing, WGSL agreement, same-field integration, actual images, temporal quality and measured renderer cost remain separate gates. No new universal field-error or arc-deficit threshold is invented here; the known large failure remains rejected, and all unresolved cases remain visible.

If the fixed representation cannot meet agreed fidelity or104×16 cost, report that incompatibility and return for explicit architecture review. Do not replace it with an easier visual, discard inconvenient histories, or claim an offline mathematical identity proves high-quality art. All final visuals must use actual production assets/pipelines and real screenshots; all eventual source work still requires the coordinated workspace tests, strict Clippy and formatting. This advisory plan authorizes none of that source work by itself.
