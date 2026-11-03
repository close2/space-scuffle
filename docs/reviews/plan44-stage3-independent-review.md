# Independent review: mist44 Stage3 CPU admission

Status: read-only architecture review during the round10 source freeze. No product edits, compilation, timing or native execution were performed. Plan52 remains approved but held. This review does not release Stage3 or change its numerical budgets.

Reviewed sources: [plan44](../plans/44-round6-volume-mist-art-direction.md), `mist/emission.rs`, `mist/mod.rs`, `mist/gpu.rs`, `battle_pipeline/{clock,driver,timeline,session}.rs`, `scene_controller.rs`, `movement/locomotion.rs`, and the existing actual Blink transaction tests. Findings were discussed directly with the44 owner. Mathematical examples below are specification counterexamples, not executed legal-gameplay traces or visual evidence.

## 1. Position/time tolerances do not bound density loss

The proposed simplifier compares source vertices with a chord at their cumulative-emitted-arc fractions. For a nonzero source interval, this bounds continuous piecewise-linear position/time error between those vertices as well: their difference is linear on each original edge, and the norm is convex. This useful property does not bound tangent error, retained length, or the resulting summed density field.

A concrete synthetic example in units of the rendered radius is the collinear path `0 → 0.04 → 0.001`. Its source arc is0.079r, while the replacement chord is0.001r. Set birth time and retained phase coordinate linear in cumulative source arc. At the middle vertex, position error is approximately0.039494r, below0.05r; time and phase interpolation errors are zero. All current reduction criteria therefore permit the replacement. The path loses approximately98.73% of its length. The exact closed version `0 → 0.04 → 0` also passes those criteria but becomes a zero chord.

For the proposed field with constant attributes, integrating its axial smoothstep-window difference along the axis gives exactly the segment length. Integrating the radial kernel over the cross-section gives `πR²/3`. Thus total integrated density is proportional to the sum of segment lengths, independently of how overlaps are summed. Replacing the example by its chord can discard nearly all source-field mass while satisfying every current pointwise tolerance. This is not repaired by only splitting exactly zero chords: the near-closed example remains. Lifetime/startup envelopes can add further differences.

Required architecture resolution before claiming field fidelity: explicitly state whether the intended reference field preserves source arc/mass, and adopt a reviewed direct field-error or arc-deficit admission contract if it does. Alternatively revise the density representation with an independently justified reference contract. Do not silently multiply density by an arbitrarily large arc/chord ratio, discard a closed loop, or claim the existing0.05r tolerance proves a radiometric bound. Keep the proposed CPU field-reference tests for curvature, short segments, reversals and segment-selection changes; numerical limits need root review. The existing GPU comparison later remains necessary.

## 2. Degenerate arc and path-coordinate contracts need exact branches

`EmitterHistory` stores position, birth and `distance`. The last is cumulative **observed actor-center travel**, not emitted-centerline arc: `advance` increments it by observed displacement while emitted positions additionally receive a displacement-direction-dependent0.7r offset. The two coordinates must remain separate.

- A finite nonzero emitted-arc interval whose endpoints coincide must still be scored using its nonzero source arc; never treat the entire interval as an empty history. The density-loss issue above remains even if geometric splitting eventually produces nonzero chords.
- Adjacent equal positions with distinct birth/phase values have zero emitted arc but different attributes. Only exact position/age equality is currently named as a duplicate criterion. If the phase coordinate survives, equality must cover it too. Retain distinct temporal/phase endpoints rather than inventing a unique age as a function of zero arc.
- Specify the all-zero-arc branch and exact count/layout treatment of zero-measure controls. No unit axis or reciprocal length may be computed from zero. Preserving temporal endpoints for neighboring nonzero segments is different from rendering an independent puff. The44 owner's proposed zero-measure temporal controls are a reviewable direction, not yet an admitted implementation.
- Clipping at8r requires interpolating position, birth and retained actor-path coordinate at the same original edge fraction. Do not reset path phase to zero at the moving oldest boundary; that would introduce an eviction-dependent noise phase jump.
- Add an explicit retained-path interpolation bound because the field uses that coordinate. The owner's proposed0.05r phase tolerance needs its relationship to the modulation's authored wavelength stated and tested; it is not a radiometric guarantee.
- Check finite positions, radius, clocks, births, cumulative phase, emitted arc and derived denominators, with monotonic chronology/phase and nonnegative age. The current history API checks each input but does not globally bound accumulated clock/distance. A reducer must reject invalid input explicitly; repairing producer state is a separate boundary change.
- Calculate diagnostic errors in sufficient precision and recheck the final rounded control representation. Selecting an index under a high-precision score alone does not prove rounded GPU endpoints/ages meet the stated limit. Error diagnostics should name position, time, phase and any adopted arc/field criterion separately.

These are input/math admission requirements, not evidence that current shipped histories already contain every degenerate case. Preserve synthetic tests separately from actual command-derived traces.

## 3. Actual presentation sampling is part of the legal-trajectory test

The production world runs at25Hz (`BattleClockPolicy`40ms). The driver permits speeds0 through4 and at most5 catch-up ticks per frame. It publishes every authoritative tick, then presents only the selected previous/current pair for the display frame. Replaying every intermediate world snapshot directly into history would bypass that observable contract.

`timeline::compatible` checks entity/type/shape/owner/radius; interpolation uses positions and ground height. `scene_controller` maps that pose into the bubble's actual3D transform, including its radius altitude offset. Mist observes that transform. Its emission offset direction comes from observed displacement, **not actor heading or facing**. A facing-only change while stationary emits no new trail. Tests must issue real movement-producing commands; there is no justification for writing a fictional heading control or deriving wake curvature from turn-rate alone.

Required production-boundary corpus:

- The existing every-movable-catalog, one/two/three-tick direction-changing command cases, plus acceleration/deceleration, supported boosts, stop/restart, real pursuit/AttackMove, collisions and actual ramps/height transitions. Compare commands with outcomes; a rejected destination is not an adversarial successful-motion case. Ground/Air and immobile eligibility must follow actual capabilities.
-30/60/144Hz display schedules through the actual25Hz clock/timeline and transform conversion. Include speeds1 through4, pause/resume/speed0, a bounded long-frame catch-up/debt-limit schedule, and display-phase offsets around authoritative boundaries. At4×, one presentation lifetime can contain four times the authoritative ticks of1×.
- Owner disappearance, exact membership boundaries, reset/scene return and disabled/re-enabled wake history; use actual lifecycle paths.51 cargo transactions are low-level supported APIs, not invented player boarding controls. Retired owners age and slot reuse resets; neither may append stale points under a new generation.
- Report history source count, actual emitted arc, actor-path coordinate, birth range, ring eviction, continuity resets, clipping, reduced controls and separate error maxima at each presentation frame. Do not declare cadence-equivalent histories where the existing presentation boundary intentionally samples different trajectories. The required result is correct admission and bounded approximation of each actual observed history, with separately assessed visible stability.

## 4. Blink exposes an existing cadence-dependent continuity boundary

`EmitterHistory::advance` clears the ring when a single **observed frame** displacement exceeds `radius * continuity_radii` (currently8r). `timeline::compatible` has no teleport/discontinuity marker, so a successful Blink's compatible snapshot positions are interpolated. A jump can be observed as a large displacement at one cadence and several smaller displacements at another; the same authoritative ability may therefore reset history at one cadence and draw an interpolated bridge at another. This is a source-derived possibility, not an executed reproduction in this review.

Add a real supported Blink transaction case, capturing exact command/admission, world snapshots, displayed poses and each history/reset result across the three cadences and relevant phases. Sweep admitted endpoint distances around the observed8r boundary while respecting45 exact destination placement/range/cost rules. Do not manually teleport a rendered entity or substitute synthetic snapshots for the command-derived case.

Repository inspection finds no shipped `BlinkTeleport` effect definition in current asset catalogs; lore and upgrade ability-slug references do not create a playable ability. Existing `checked_movement_tests` author a generic `AbilitySpec` and execute the real Blink code. Use that same transparent capability-fixture approach for supported-mechanic boundary coverage, separate from unchanged shipped-catalog coverage; do not add a product capability or claim a shipped Blink actor. If the observed bridge/reset behavior fails the visual/retention contract, require an explicit presentation-boundary amendment before volume shipping. A teleport marker, new interpolation rule or emitter repair is outside the reducer implementation scope.

## 5. Smooth support is not automatically smooth in time

The proposed spatial caps are smooth for a fixed packet. Ring eviction,8r clipping and changed simplifier splits can still replace that packet's field discontinuously. A moved boundary can remove nonzero density instantaneously; changing tangents/segment counts can alter overlap density despite small positional errors. The existing plan correctly asks for consecutive eviction and scene-return observations; record those as actual field-change tests, not just support-AABB or source-vertex checks.

Pin down whether the header's support length/startup gain uses original retained emitted arc or reduced chord sum. Keep that choice coherent with the density-reference decision in finding1. Preserve selected packet age/time/path/generation together; pending/acknowledged culling unions do not repair field discontinuity.

## Disposition

Following this review, the44 owner relayed root's bounded direction: forbid zero-chord collapse, require a finite representable reciprocal, and record source/reduced arc plus deficit distributions without inventing an arc-deficit threshold. The CPU slice may establish control approximation only; the actual field oracle must separately reject optical mass/brightness failures before volume acceptance. This is consistent with the counterexample above, provided near-closed paths remain explicit unresolved field cases rather than being called accepted volume results.

Stage3 remains unimplemented and unadmitted. The16-segment,0.05r, lifetime/64,8r support and104-ray-step limits remain explicit proposals, not proven sufficient for authored gameplay. Root should review the owner's forthcoming exact degenerate/path/field admission amendment before release. One ordinary legal trace exceeding the control budget still blocks the representation; explicit runtime failure is evidence, not acceptable normal gameplay. No new fallback, tolerance relaxation, support shortening, fake assets or editor work is proposed here.
