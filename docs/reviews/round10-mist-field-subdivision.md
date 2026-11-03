# Independent review: proposed mist field changes under exact subdivision

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Status: mathematical review only during the round10c source freeze. The proposed summed, segment-clamped age field is not admitted for reduction fidelity merely because the CPU reducer's control approximation is exact. No product code, build, GPU run, screenshot, legal-motion reproduction, art acceptance or performance measurement is involved. The approved Stage3 CPU reducer scope and16-segment budget are unchanged.

## Mathematical input and normalization

This review evaluates version1 in [the field oracle proposal](plan44-field-oracle-review.md). Set actor radius r=1, unboosted density, modulation1, and straight source length L=2. The32 source edges partition [0,2] uniformly; the reduced representation is one edge. Normalized age decreases linearly from1 to0, and path coordinate is affine. These endpoint ages are a mathematical special case, not a claim about the current retained history ring or a legal25Hz command trajectory. Position, source-arc age and path interpolation errors are zero; source and reduced arc lengths are identical.

For an edge [a,b], define clamped smoothstep S as in the proposal, then

```
A(x) = S(x-a+0.5) - S(x-b+0.5)
z(x) = 1 - (a + clamp(x-a,0,b-a))/2
R(x) = 0.5 + 0.25*S(z)
F(x) = S(z/0.06) * (1-S((z-0.45)/0.55))
K(q) = (max(1-q*q,0))^2
rho(x,y,w) = A(x)*F(x)*(0.5/R(x))^2*K(sqrt(y*y+w*w)/R(x))
```

The common startup factor is S(L/1)=1. The root's smaller-L cases omit their common nonunit startup factor; their reported transmittances therefore must not be presented as full production-style opacity. This report uses only L=2, where no such omission exists.

## Independent radial integrals

At fixed axial x, radius and age do not vary across the perpendicular plane. Integrating the radial kernel in polar coordinates gives

```
2*pi*integral_0^R s*(1-s*s/(R*R))^2 ds = pi*R*R/3.
```

The central broadside ray crosses that plane along a diameter. Its radial integral is

```
integral_-R^R (1-y*y/(R*R))^2 dy = 16*R/15.
```

Consequently the cross-sectional mass and central broadside column for one edge are exactly

```
m(x) = (pi/12)*A(x)*F(x)
C(x) = (4/(15*R(x)))*A(x)*F(x).
```

These reproduce root's formulas. Inverse-square expansion cancels radius from the mass expression exactly. Radius still affects column density; the mass discrepancy itself comes from how each segment clamps and weights age/fade. In physical units, mass scales with r cubed and column with r; extinction0.9/r makes the quoted transmittance dimensionless and radius-independent for this normalized family.

For constant attributes, adjacent axial windows telescope. For this varying-age model, the overlapping cap neighborhoods use different clamped z values. Their F values cannot be factored out of the sum. Exact age interpolation on the source centerline does not imply equal per-segment cap attributes throughout the field. There is no contradiction with the constant-attribute identity.

## Independent numerical check

Root retained `/tmp/space-scuffle-r10-field-subdivision-review01/results.json` and `experiment.py`. Its computation uses NumPy arrays and trapezoidal axial integration at8193/16385/32769 samples. The independent calculation used separately written scalar functions and composite Simpson integration over the common full support [-0.5,2.5], at2048/4096/8192 intervals. It imported neither root's script nor its computed arrays. The independent broadside calculation evaluated the stated ray directly with the analytically reduced column formula; it did not perform a new maximum search.

| Simpson intervals | Source mass | Reduced mass | Relative change |
| --- | --- | --- | --- |
|2048|0.363769879254|0.344940187420|−0.051762646959|
|4096|0.363769877534|0.344940176717|−0.051762671896|
|8192|0.363769877731|0.344940177928|−0.051762669082|

Root's finest retained values are0.363769877717 and0.344940177855, relative change−0.051762669246. Independent refinement agrees with a roughly5.176% loss. These refinements are numerical agreement evidence, not certified interval bounds.

At root's sampled maximum-column-difference location x=1.99151611328125r, the independent direct calculation gives:

| Quantity |32-edge source|1-edge reduced|
| --- | --- | --- |
|Column density / r|0.19795308549123145|0.003907078234923168|
|T=exp(−0.9*C/r)|0.8368103836891612|0.9964898047787984|
|Alpha=1−T|0.1631896163108388|0.0035101952212016|

The column difference is0.19404600725630827r. The corresponding transmittance difference is about0.1596794211. This verifies the reported fixed-ray result; it does not certify that the sampled point is the continuous-domain global maximum. The discrepancy is already resolved at this one ray, without relying on the maximum claim.

## Rejection boundary

This example supports rejecting the inference that exact CPU control approximation admits this summed clamped-age model for field fidelity. It has no geometric arc loss at all, so an arc-deficit statistic alone would not detect the demonstrated mismatch. It is also independent of modulation, GPU packing, shader precision, ray-step quadrature and transparency ordering.

The candidate field formulation therefore needs explicit further model/fidelity review before shipping as a representation-preserving reduction. No new global numerical acceptance threshold, density normalization, nearest-segment rule, added controls, age-gradient clamp or budget increase is authorized by this finding. The CPU reducer may still implement and verify its stated control-approximation contract; that contract is not promoted into a density/opacity guarantee. Actual legal history and temporal cases remain necessary before gameplay or artistic conclusions. The result is not evidence that the currently shipped ribbon has this mathematical defect, because that ribbon does not implement this proposed field.

## Provenance

- Root results SHA256: `6324502832595fe7d0a59b6c692306592876aff858502e5ae29d174015bf7498`.
- Root experiment SHA256: `39b778920901a0af140b87cab7d9c8a6918744be43ec353c8cc31b7156adfecd`.
- Independent scalar script: `/tmp/space-scuffle-r10-field-subdivision-independent.py`, SHA256 `91f22509e35e05fc52ee6eb6d1d04fed64c1756ef8ff13635b8d3b76bcdefce6`. It preserves the separately executed scalar algorithm with formatting changes only.
- Independent output: `/tmp/space-scuffle-r10-field-subdivision-independent.json`, SHA256 `86b9c7ad418c2f7a84649ff0fb81e6cf1b463def49f43e2cb3814e629f48d792`.

The small mathematical calculation is not a benchmark or repository test run. All product sources remain unchanged.
