# Round11 independent control-capacity diagnosis

Goal: distinguish a greedy split failure from a capacity limitation on the first fully preserved legal producer window. This is a read-only offline verification experiment, not a replacement reducer, shader or policy change. The complete corpus continues unchanged.

Use the first ControlBudget artifact, original f32 source positions/path values reconstructed from their round-trip decimals, original f64 births/clock, and actual preserved lifetime/policy. Select the first failing frame only if its full source is within support, avoiding any independent clipping interpretation. Exhaustively evaluate every chronological source-endpoint pair using the declared arc parameter and rounded output attributes; form a directed acyclic graph whose edges satisfy every vertex/endpoint tolerance and positive representable geometry rule. Dynamic programming computes the minimum number of source-vertex controls. Record all feasible edges and one optimal path, plus the original failing interval's independently recomputed errors.

This can establish a lower bound only within the declared source-vertex-control representation and metrics. It does not prove impossibility for arbitrary off-source controls, curved representations, another emission policy, another field definition or visually equivalent media. Offline exhaustive work is not a proposed real-time algorithm or performance result. Verify script/input hashes and preserve exact results.

## Root result; independent verification requested

The first failure at frame12 has22 distinct source vertices, original arc1.486811367636741, and support3 world units, so clipping is absent. All231 chronological endpoint pairs were checked. The shortest feasible graph path requires **19 controls (18 segments)** against the declared17 controls (16 segments). Its indices are `[0, 1, 2, 3, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21]`. The original failing interval17→19 independently yields position error0.028126586839755493 versus tolerance0.01875, ratio1.500084631453626; age/path ratios also match the retained actual Rust diagnostic within ordinary scalar rounding.

Within this source-vertex representation, the first refusal is therefore not explained solely by the greedy split choices. This does not justify widening the budget or altering the producer, nor does it claim arbitrary off-source controls are impossible. The graph and every accepted edge are retained in `/tmp/space-scuffle-r11-control-capacity.json`; script SHA256 `401e722c3e53a4b236290025d2f84a23812b98d2d848c6ef2b53fcc82ebab7e0`, source artifact SHA256 `c69fd0cac1aa99e4f74fae287abbd6a2f8a78c689ade64de6dcd1c77f464d1ab`. Independent second implementation/check is requested. Complete corpus accounting remains pending.

## Independent second implementation

The independent reviewer rebuilt every edge from the preserved actual frame12
source, reconstructing original f32 positions/path and rounded relative ages. It
used 80-digit Decimal geometry and barycentric attribute interpolation, then
breadth-layer reachability by segment count rather than importing the root's
prefix dynamic program. The only shared code was the generic RON parser. It
checked finite positive rounded chord/reciprocal/axis geometry, every endpoint and
interior vertex, chronological attributes, and absence of support clipping.

All231 endpoint pairs yielded exactly the same25 feasible edges as the root
calculation. The first reachable last vertex is at18 segments, requiring19
controls; one optimal path matches the root path exactly. The largest admitted
edge's maximum error ratio is0.010667677217963; the smallest rejected edge's ratio
is1.015399330714225. These margins and agreement of the independently evaluated
edge graph make the result insensitive to the observed ordinary f64 rounding
differences; this is not a formal platform-wide floating-point error proof.
The original17→19 interval has spatial error0.02812658683975482 and ratio
1.500084631453590, agreeing with the Rust/root diagnostic to ordinary numerical
rounding. Decimal total source arc remains below the actual support3.

The source artifact hash was checked before and after this independent probe and
still equals `c69fd0cac1aa99e4f74fae287abbd6a2f8a78c689ade64de6dcd1c77f464d1ab`.
All edge metrics and breadth-layer reachability are retained in
`/tmp/space-scuffle-r11-control-capacity-independent01/results.json` (SHA256
`83f344fd34417f1babf00828c5dc2ffdd3c982e130704655d93e9f19e6d777f4`), with separate
`check.py` (SHA256
`4d797548131ba875838c95b63fecae2bbcc7dd531b5fa7a52633cd446e7951e3`). No product
code, budget, tolerance, source artifact or full-corpus accounting was changed.
The conclusion remains restricted to original source-vertex controls under this
policy; it proves neither an arbitrary-control lower bound nor optical failure.
