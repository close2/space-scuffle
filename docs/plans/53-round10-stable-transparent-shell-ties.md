# Round 10: diagnose and stabilize exact-depth shell ordering

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Status: Stage A is implemented, with focused CPU/actual-GPU checks and two preserved
native matrices complete. Integrated acceptance awaits the coordinated round11
workspace gate. Stage B production correction remains held for explicit root
release; diagnostic mechanism evidence does not itself authorize that change.

## Goal and evidence boundary

Make repeated rendering of identical actual shell geometry, material inputs,
lighting, camera and presentation time insensitive to parallel visibility insertion
order when several shell centers have exactly the same view-space depth. Preserve
normal unequal-depth ordering and the accepted35 mist-before-shell tie contract.
This does not solve intersecting transparent surfaces, approve art, change swarm
formations/physics, recolor owners, or develop the editor.

Final40 normal05 and sparse02 each passed all17 ownership records and actual image
visibility checks. Root independently found that their effect images are byte
identical, while sparse Swarm09 differs from normal05 in2450 pixels (maximum channel
difference153), concentrated in the central overlapping actors. Sparse02 versus
prior sparse01 differs2060 pixels (maximum147), within x1094..1180/y309..645. The
unobstructed primary/secondary representatives remain visible and identity-correct.
These differences are preserved, not relabeled as equal images.

Read-only source evidence supports a hypothesis, not a proven pixel cause:

- `scene_controller.rs::sync_swarm_stress_system` creates two100-actor grids at
  x=8+1.2*column and x=24−1.2*column, with equal row z=10+1.2*row and y=.5,
  sphere radius.45. The groups overlap across x13.2..18.8. Current animation moves
  same-row actors by the same z-based displacement; zero capture delta freezes it.
- `RtsCamera::compute_transform` uses pitch about X and no yaw. Locked
  `ViewRangefinder3d::distance_translation` is inverse-view row2 dotted with
  translation, i.e. view-space Z, NOT Euclidean camera distance. Same-Y/Z actors
  can therefore tie exactly regardless of their X separation.
- Locked Bevy0.15 visibility uses `par_iter_mut`, pushes to thread-local queues and
  drains those into `VisibleEntities`. `queue_material_meshes` iterates that list.
  `Transparent3d::sort` is stable radix sorting by distance only. Equal-depth
  insertion order is therefore observable in alpha blending.
- Accepted35 explicitly orders mist queue → packet alignment → shell queue. It
  fixes ties between those material classes; it does not order multiple shells
  within the shell queue. That policy must remain intact.

Primary references are the actual locked sources under bevy_render0.15
`view/visibility/mod.rs`, `render_phase/rangefinder.rs`, bevy_pbr0.15 `material.rs`,
bevy_core_pipeline0.15 `core_3d/mod.rs`, and current project code. No inference from
newer online Bevy APIs is required.

## Stage A: bounded actual-render diagnosis

Add an opt-in capture observer with a fixed bounded record budget, disabled in
ordinary production. Record real queued and post-PhaseSort entries for each actual
view: logical shell identity, main/render entity correlation, material/pipeline/draw
function identity, actual view-space distance bits, phase position, complete camera
matrix/projection/viewport, and presentation time. Source material uniforms,
geometry/transform/AABB values and actual media/lighting source values accompany
that record. Diagnostics must reject truncation or ambiguous identity rather than
silently sampling only convenient actors. Keep shader, formations, asset files and
actual capture controls unchanged.

Use the preserved normal/sparse Swarm scene and a bounded two-shell fixture made
from actual catalog/media/material loading. The two-shell fixture is a renderer
experiment, not substitute game evidence. Repeat the same copied executable and
inputs at least three times, retaining every image and record. Establish whether
changed central pixels coincide with changed equal-depth shell order while the
listed source state is equal. A changed geometry/time/material source invalidates
that causal comparison and must be reported separately. Observer registration may
itself perturb formerly unordered scheduling (35 demonstrated this); therefore
also retain unobserved runs and perform an explicit controlled reversal of equal-
depth shell insertion in the renderer fixture. Do not claim an unobserved phase
order merely from its image. No engine/compiler patch is justified by hypothesis. Records carry actual render
frame/packet/capture epochs; correlate queued and sorted evidence to the screenshot
readback rather than assuming its callback's CPU frame is the rendered frame.
The public screenshot callback does not expose its private render-copy epoch. Retain
all candidate queued/sorted render epochs across request through callback; associate
an image with a specific order only when every candidate agrees on the complete
source fingerprint and that order. Otherwise report ambiguous readback/order
correlation, not a proven pixel cause. Pipeline/material readiness must hold in
all relevant view epochs. Retain the entire bounded request-to-callback interval;
never discard varying epochs or wait selectively for a favorable random order.
The controlled two-order renderer fixture
must hold each declared insertion order across the whole accepted readback window.
Do not patch the renderer or fabricate an exact frame label to bypass ambiguity.
A controlled mechanism demonstration alone does not prove the historical Swarm
image difference had that cause. Report that narrower result for root review before
any correction release if historical/native correlation remains inconclusive.

If order is not the cause, stop the proposed correction and report the falsified
hypothesis. Audit actual geometry/time/texture/lighting differences next under a
separately reviewed amendment; do not quietly change art or thresholds.

## Stage B: conditional bounded correction

Use a dedicated client renderer module/plugin, independent of mist shader/style
and simulation. Extract a small `ShellSortIdentity` for actual BubbleDomeMaterial
entities from their immutable logical instance identity. The key is logical scene
scope plus EntityId; generation is retained for admission/provenance and stale-key
rejection, not a hue/material-derived priority. Live battle uses its committed
session generation, Lab/Swarm use explicit scene domains, standalone native preview
uses an explicit fixture domain. Register the same identity producer in all actual
game/preview shell spawn paths. Scene return/reset must never reuse a stale key for
a different logical instance. Same logical identities in a rebuilt scene retain
their relative order. A missing/duplicate identity within one view/domain is a
clear typed presentation diagnostic, not a fallback to incidental Bevy allocation,
owner number, palette color, asset handle or arbitrary epsilon. Untagged unrelated
transparent materials are outside this policy.

Concretely, the source enum is `Battle`, `VisualLab`, `Swarm`, or `StandalonePreview`;
these identify real renderer producers, not units/owners. Resolve exactly one source
from existing scene markers, with an explicit marker for standalone preview fixtures.
The total sort key is `(source, logical_entity_id)`. For Battle, extracted provenance
also carries the actual committed `SessionGeneration`; compare it with the view's
admitted session and reject mixed stale generations. Lab and Swarm roles keep their
logical IDs across their ordinary despawn/recreate lifecycle; no artificial frame
counter enters their sort key. Standalone replacement fixtures carry their existing
explicit replacement generation as provenance. Extraction removes keys when the
actual entity leaves the main world, and render identity is used only to correlate
that lifecycle. A source-marker ambiguity is a typed error. Thus a rebuilt logical
scene has the same ordering without depending on incidental ECS generations.

Canonicalize each contiguous shell-only queued block after the real shell material queue
and before `sort_phase_system::<Transparent3d>`, without changing any distance.
The existing35 chain remains mist queue → packet alignment → shell queue → shell
canonicalization → PhaseSort. Preserve every non-shell phase slot AND its relative relationship to shell blocks:
never move a shell across an intervening unrelated draw. The current single shell
material queue should produce one contiguous block per view; verify that in actual
phase evidence. If another producer intentionally creates multiple shell blocks,
canonicalize each independently and preserve their intervening draw ordering; do
not claim cross-block ordering was canonicalized. Move each complete shell phase item (pipeline, draw function, entity pair, batch
range and extra index) together. This works across heterogeneous specialized shell
pipelines; pipeline IDs are evidence, not tie keys. Stable engine distance sorting
then preserves logical shell ordering only for exact depth ties; near ties retain
unaltered geometric order. No comparator may mix “compare by identity for two
shells” with “otherwise retain insertion order”: that relation is not transitive.

A concrete in-place permutation, separately for each contiguous block, uses retained
scratch arrays: ascending shell phase positions; `(logical_key, original_ordinal)` records sorted by unique logical key;
and inverse destination ordinals. Fill the inverse permutation and apply its cycles
with `phase.items.swap`, updating destination entries until identity. This needs no
Clone implementation for Transparent3d and never moves a non-shell slot. Sorting
is O(N log N), permutation O(N). Scratch capacity grows only at declared extracted
membership/view lifecycle changes, before steady-state queue processing; reuses
capacity across frames and camera changes. Record retained capacity/bytes and prove
no allocator calls in the steady-state canonicalizer. Missing capacity is a typed
failure, not per-frame hidden growth. Document this bounded bookkeeping with the
charter's performance-rationale block.

Process each view independently using that view's actual queued items and depth.
Two cameras can require opposite unequal-depth order and must remain correct.
The locked radix sorter distinguishes −0/+0 float keys; leave its complete distance
sorting untouched. A diagnostic exact-tie group uses raw distance bits, not f32==
or an epsilon. Never quantize nearby distances. Reject nonfinite shell
sort inputs. Validate duplicate/missing/stale logical identity across the ENTIRE
view before mutating any block; block-local uniqueness is insufficient. Renderer
faults cross an explicit shared first-fault channel into the existing main-world
presentation failure policy (readable retained diagnostic for ordinary client,
nonzero fail-fast for bounded capture), not a RenderApp log-only path. Preflight
failure suppresses the invalid shell submissions for that view and prevents any
capture acceptance; it does not invent a default identity or silently preserve an
unverified order. Other rendering remains available to show the failure UI.
Do not impose a global order on unrelated transparent systems or
modify Bevy's phase item types/dependencies. Editor plugin registration and editor
sources remain untouched.

## Verification and acceptance

- Pure permutation tests preserve every full item/other-material slot, handle0/1,
  shuffled identities, heterogeneous pipelines, multiple shell blocks separated by
  unrelated draws (pin each shell/draw relationship), and repeated frames, and reject
  duplicate/missing/stale generation identities. Nonfinite key inputs fail.
- Real render-phase tests use the locked queue/sort path, not a model-only sorter:
  reverse shell insertion at equal depth and require identical final logical order;
  perturb by a representable near-depth amount and require geometric ordering;
  cover signed zero explicitly; cover two different camera views.
- Preserve35 actual mist-before-shell ties and verify its packet-alignment chain
  with both fresh and retained packets. Cross-material interleaving slots remain
  unchanged; no sorting epsilon or queue-wide material regrouping is allowed.
- Exercise actual Lab/Swarm/live transitions, committed-generation reset and
  standalone preview replacement. Validate key removal/admission and no stale
  entity association. No simulation state, authoritative checksum or owner mapping
  changes are expected or accepted by this renderer work.
- Repeat the same actual native Swarm capture at least three times with correction,
  and normal/sparse profiles at matched state. Require exact source fingerprints
  and actual phase keys, then compare PNG pixels in the overlapping shell region.
  If the native pipeline's separately measured one-channel numerical noise exists,
  state it explicitly; do not grant a new tolerance to conceal differing blend
  order. Large central ordering changes must disappear. Retain complete images
  and observer-on/off results; all failure runs remain evidence.
- Inspect full images, not only numerical deltas. Existing arena occlusion, fixed
  formations, legacy media and interpenetrating-surface artifacts remain separate
  rejected quality problems. No claim of full200-actor readability follows.
- Run focused real-render tests and then coordinated `cargo test --workspace`,
  strict workspace all-target Clippy and workspace formatting. Preserve executable,
  all source/assets and exact scripts before native runs. No editor changes.

Ownership: this lane may implement the new shell ordering module and coordinated
scene/preview identity producers only after approval. Coordinate mist registration
and its existing queue dependency with44/35 owner; no shader/light/storage changes.
Coordinate runtime generation reads with38/51 owners. Root owns full gate and native
reservation. Round10 product source remains frozen while this plan is reviewed.

## Stage A implementation boundary and capture protocol

Root approved one source-hook exception after inspecting the locked uploader:
`mist/packet_witness.rs` owns a passive optional preallocated packet witness, with
one module declaration and one hook immediately after `select_packet_inner`.
It copies the actual newly selected packet or retains the previous complete bytes
when no staging slot was selected, recording actual sequence/buffer identities and
readiness. It never installs CallbackGate, changes callback/staging state, retries
selection, or calls `UploadState::fail` for an observer error. Typed witness faults
reach the diagnostic capture channel. The absent-resource branch allocates nothing.
Test fresh → retained → fresh, layout mismatch and poisoned evidence ownership.
This is selected CPU byte evidence, not GPU readback. Coordinate the declaration
with44's reduction module edits; uploader policy and shader source remain unchanged.

### Concrete controlled-fixture setup (awaiting narrow review)

The proposed controlled fixture uses the existing actual Swarm producer and
admitted media, not newly invented proxy shells. Explicit fixture data names two
logical Swarm EntityIds and two world transforms, plus Forward or Reverse declared
insertion. At the Swarm checkpoint only, a capture-only PostUpdate system after
production presentation and before transform/visibility propagation hides other
Swarm shells and sets those two render transforms and corresponding snapshot/
material center fields. Both actual selected IDs must exist, have distinct logical
identity and resolve the real material/media; a mismatch is fatal. This does not
mutate authoritative entities or the normal producer. The declaration remains
active through callback and closure; ordinary captures do not install it.

The fixture camera uses the existing declared Swarm view. The chosen two center
Y/Z values must produce exactly equal actual queued view-space distance bits;
reject differing bits, clipping or absent actual draws before intervention. After
the real material queue, swap only these two complete shell phase entries to match
the declared order, preserving every other item index. Record the original queue,
the declared intervention and the actual engine-sorted result. The two shells must
occupy one contiguous shell-only block, so this experiment does not reorder either
shell across an unrelated draw. Require the same order throughout request/closure.
This is a controlled blending-mechanism experiment only; natural Swarm observations
retain unmodified production transforms, membership and phase order. The existing
17-goal normal capture remains the natural source; a dedicated fixture script may
select the same validated profile with its experimental Swarm checkpoint explicitly
labeled. Any requested deviation from these boundaries needs separate review.

This section specifies the approved diagnostic stage against the final40/44/51
source. It does not authorize canonicalization, new production ordering, Stage B
identity admission, or an engine patch. Gate10b's timing failure and the unchanged
gate10c retry keep implementation held until root explicitly releases it.

### Files and ownership

- New `client/src/shell_order_audit/{mod,source,phase,interval,artifact,tests}.rs`
  owns opt-in source extraction, actual phase observation, bounded interval joining,
  typed diagnostic errors and serialized evidence. `lib.rs` adds only the module
  export; ordinary `ClientAppPlugin` does not install this observer.
- `runtime_capture/config.rs` adds an optional strict diagnostic configuration,
  default absent, with declared record budgets. Existing ownership and transport
  profile validation remains unchanged. No new gameplay goals are needed for the
  real Swarm observation: arm only at the existing Swarm checkpoint.
- `runtime_capture/mod.rs::run_capture` conditionally installs the observer after
  the actual graphical app and capture pipeline evidence exist. The same binary
  with the option absent is the observer-off control, not a different build.
- `runtime_capture/driver.rs` changes only the request/readback handshake and a
  diagnostic-drain stage, preserving `verify_frame`, ownership fingerprints,
  screenshot-token validation and all original fatal checks. `artifacts.rs` adds
  an optional reference to the completed sidecar, not a duplicate copy of every
  phase record in each normal capture record.
- A separate bounded renderer experiment and its declaration file own the actual
  two-shell controlled-order fixture. It uses the shared actual catalog/media,
  palette, optics and material setup. Its explicit fixture-only queue intervention
  runs after real shell queuing, swaps the two complete shell phase entries into
  the declared forward/reverse order, and holds that declaration throughout the
  interval. It never installs in ordinary captures or changes distance/materials.
  The intervention must be labeled in every artifact; its evidence is a controlled
  mechanism experiment, not a natural queue observation or Stage B correction.
- No sim/world, editor, shader, palette, mist packet/light/style or normal scene
  producer edits belong to Stage A. Resolve existing Lab/Swarm/live markers during
  diagnostic extraction. A standalone fixture supplies its own explicit domain.
  Coordinate any required public read accessor with its owner before extending it.

### Evidence model and limits

Use checked monotonic `AuditEpoch(u64)` for completed extraction frames and a
separate `CaptureNonce(u64)` for request ownership. Neither is a simulation tick,
screenshot frame number, or arbitrary Bevy entity allocation. Epoch overflow is a
typed failure before publishing a new epoch. Main/render entity IDs are recorded
for correlation only, alongside `(scene domain, logical EntityId, provenance
generation)` derived from actual source membership. Reject missing, ambiguous,
duplicate or stale logical identities across the whole relevant view, not merely
inside a contiguous shell block.

`SourceFrame` records exact finite source bit patterns: camera transform/projection,
viewport and render target, default-Time elapsed/wrapped shader time, shell logical
identity and owner, transform/AABB/mesh identity, full material uniform bits,
actual texture/media identity and retained checked asset-content inventory. Keep
the identity-to-value table, not just a hash that cannot explain a mismatch.
Record actual applicable light/optics source state and prepared-view association;
do not describe CPU source values or a GPU buffer handle as read-back shader bytes.
If required prepared metadata is unavailable, mark the epoch pending or failed,
never substitute zero/defaults. Dynamic source values must come from the same
extraction epoch as the phase records they annotate.

`ViewEpoch` holds the extracted view/main identity, source epoch, exact view matrix
and viewport, and complete `Queued` and `Sorted` phase vectors. Each item records
phase ordinal, entity pair, optional logical shell identity, raw distance bits,
specialized pipeline ID/state, draw-function ID, material ID and its source-table
association, mesh identity, batch range and extra index. Unrelated transparent
items remain in the vectors, so shell blocks and mist/class precedence are
inspectable. Asset/pipeline numeric IDs are process-local evidence; cross-process
comparison resolves them to descriptors/source data rather than treating handle
numbers as semantic equality. Raw floating bits retain signed-zero distinctions.

The full phase payload is available for every unrelated transparent item, but
typed material/uniform resolution is not a generic property of `Transparent3d`.
Represent its association explicitly as a supported material kind plus typed
source reference, or `Unsupported { reason }` with no material/uniform reference.
Never invent an asset ID, infer a material from color, or fault merely because an
unrelated draw has no supported material adapter. If that item contributes to the
declared comparison region and its relevant state cannot be established, retain
its complete phase payload and classify the causal comparison as limited or
inconclusive. This is a bounded shell/mist/scene experiment, not a universal
renderer inspector.

For the actual Swarm overlap comparison, shell data alone is insufficient. Reuse
accepted35's selected-packet provenance at the actual queue boundary: retain the
exact selected CPU packet bytes, packet/owner generation and sequence, selected
GPU buffer/resource identity, and submission/selection association. Record which
packet was actually selected for those draw entries, rather than the newest
main-world packet, since a retained older packet is valid rendering behavior.
Include the applicable mist style/material values and already available active
view-light source association. This is CPU packet and resource-selection evidence,
not GPU buffer readback or proof of private GPU contents. Do not add a new readback
infrastructure or change the accepted packet-selection policy for this observer.

Also retain the bounded active Swarm scene inventory that can contribute behind
or beside those shells: actual arena/rig and active terrain/environment membership,
mesh/material identities and resolved supported source values, transforms, clear
color, relevant lights, camera postprocessing, and checked texture/shader asset
content inventory. Explicitly record absent terrain where the active Lab/Swarm
scene has none; do not substitute the inactive battle environment. These values
must be associated with the same candidate extraction/render epochs as the shell
and mist evidence. Use existing scene markers and source resources, without editor
changes or an exhaustive inspector for unrelated renderer internals. Unsupported
or unobserved relevant state prevents a complete natural-Swarm causal claim. It
does not become stable by omission from a fingerprint. A controlled two-shell
mechanism result may still be reported under its narrower declared scene scope,
with the historical Swarm cause explicitly inconclusive.

`IntervalEvidence` records the nonce, request/callback/closure observations,
inclusive epoch bounds, every candidate `ViewEpoch`, decoded image path/hash, and
an explicit result: stable association, inconclusive variation, or typed failure.
Do not reduce changing sequences to a favored order or silently discard epochs.
Stable association requires exact source equivalence and the same semantic sorted
order for every relevant candidate view epoch; it is not an exact GPU-copy epoch.

Budgets are declared in the diagnostic script, with bounded maxima of 4 relevant
views, 4096 total phase entries per view/epoch, 256 candidate epochs per request,
and 64 MiB retained evidence per request. Checked accounting includes both phase
vectors and source tables. Validate all budgets before app startup; report budget
exhaustion/truncation as failure and retain the incomplete sidecar. Existing wall
deadlines still apply. Diagnostics may allocate within these explicit limits and
are excluded from production performance claims. No unbounded worker channel or
per-frame log stream is introduced.

Storage refinement approved during source review: the64MiB figure bounds compact
serialized evidence, not total process heap/RSS. Exact decoded CPU mesh/image/shader
content is deduplicated once per actual asset identity within an extraction epoch,
and changed revisions remain observable on subsequent epochs. Store descriptors,
byte counts and explicitly labeled BLAKE3-256 digests in the report. Retain unique
raw content in separately named binary sidecars, under a separately declared raw
source-byte budget (maximum64MiB); a digest never means GPU-readback evidence.
Within-process revision comparison still checks exact cached content bytes. Avoid
200 repeated hashes/copies for a shared Swarm sphere or atlas in one epoch. The
source-file/binary preservation inventory remains independently required. Latest
epoch, pre/post-sort vectors, callback reports and serialization scratch can coexist:
these bounded diagnostic working copies consume additional memory, so neither
storage budget is advertised as a64MiB heap cap or a production allocation claim.
Record unique raw bytes and compact artifact bytes separately; reject exhaustion
instead of silently dropping source content or raising limits after a failed run.

### Locked schedule points

Extract the source table and nonce state in the real `ExtractSchedule`; associate
main entities through the locked `MainEntity` mapping, not assumed numeric equality
between worlds. Publish one source epoch only after the entire table validates.
Observe queued `ViewSortedRenderPhases<Transparent3d>` in `RenderSet::PhaseSort`
before `sort_phase_system::<Transparent3d>`, after QueueMeshes and the existing
mist light-admission phase guard. Existing35's mist queue → packet alignment →
shell queue ordering remains intact. Observe sorted vectors in the same set after
that exact engine sorter, before PrepareResources/batching alters the entries.
These systems only read phase items in natural-observation mode. Do not use a
handwritten substitute sorter to generate evidence.

At the phase observation inspect the exact queued specialized pipeline IDs through
`PipelineCache`, prepared `RenderAssets<PreparedMaterial<BubbleDomeMaterial>>`,
`RenderMaterialInstances` and mesh associations. Existing aggregate
`PipelineEvidence::ready` is still necessary but cannot establish that these exact
draws were ready. Pending imports/compilation produce a pending epoch; terminal
errors produce a typed diagnostic failure. Keep all pending/error epochs inside
an armed interval. A request containing a nonready candidate cannot be accepted
as a stable source/order-associated image even if later frames become ready.

### Request through callback: conservative epoch closure

1. Keep ordinary actual-loader warmup and four-frame settle behavior. Before
   arming, require at least one complete relevant ready render epoch and current
   actual source agreement. This is readiness gating, not selection for any
   particular random shell order. Historical warmup records are explicitly outside
   the request interval; never restart an armed interval to discard variation.
2. In the same `Last` branch that has accepted `verify_frame`, arm the nonce before
   spawning `Screenshot::primary_window()`. Store the latest fully published render
   epoch as a conservative inclusive lower bound and retain its evidence. Rendering
   can be pipelined: do not assume the next main frame is the image's render frame.
3. Every extraction while armed tags the active nonce. Retain every complete queued
   and sorted candidate epoch for all relevant views. Continue the existing frozen
   world/effect/material checks on every Readback frame. Main/render exchange uses
   a bounded shared evidence resource with a typed first-fault field; lock poison,
   overflow, dropped source association or missing expected phase is an error,
   not a `try_lock` skip or fallback record.
4. The actual `ScreenshotCaptured` callback validates the exact screenshot entity,
   current simulation and existing ownership visual fingerprint, then decodes and
   preserves the actual PNG. It records the callback and requests closure, but
   does **not** advance the checkpoint, mark the manifest complete or change
   camera/time/source state yet. The public callback cannot supply the private
   copy epoch; no such label is fabricated.
5. The next real extraction observes closure and publishes a closure epoch.
   Complete its queued and sorted observations before acknowledging closure.
   Collect all epochs from the inclusive lower bound through this acknowledgement,
   including in-flight frames at callback time. This deliberate superset may be
   inconclusive; it must not omit a potentially relevant render epoch. Verify
   contiguous epoch coverage and complete per-view phase pairs. Missing frames or
   an unexpected relevant-view lifecycle change reject the association.
6. A main-world diagnostic-drain stage keeps the scene frozen and retains ordinary
   deadlines/fatal checks until closure acknowledgement. Write the full sidecar,
   classify stable versus inconclusive association, then finish the checkpoint.
   An inconclusive diagnostic capture preserves its PNG and all epochs but cannot
   report successful causal verification; bounded diagnostic exit is nonzero.
   Render-side faults likewise reach the existing capture failure manifest and
   nonzero exit, never just a RenderApp log. Ordinary observer-off behavior remains
   unchanged; broader production fault policy belongs to conditional Stage B.

Tests cover out-of-order/duplicate callbacks, two views with different valid
geometric orders, a pending exact pipeline during an armed window, missing epoch
or half-phase, source mutation after request, callback before render completion,
closure with an in-flight candidate, budget/epoch overflow, and varying-order
intervals that preserve every record and remain inconclusive. Real-render tests
then prove the locked queue/sort hooks surround the actual sorter and that the
two declared fixture orders remain stable through callback/closure. Native
comparison starts only after these tests and source/binary/script preservation;
all image and art-acceptance limitations above still apply.

### Evidence failure accounting clarification

`max_bytes` bounds the sum of compact serialized epoch and asset-descriptor
components. It does not include the enclosing report's field labels, scalar
accounting fields, or separators and is not an exact whole-file or resident-memory
limit. The final file length is independently observable from the written artifact.
Both successful and incomplete reports attach the same asset revision inventory,
component-byte sum and unique raw-sidecar byte count. Callback/freeze/readback
failures persist the available partial interval and source sidecars before the
existing error exit. Unsupported queued associations in any view/epoch prevent a
complete source-stability claim even if the main source table is unchanged.

Before any report publication, separately encode the entire report as compact
RON and reject if its exact UTF-8 length exceeds `max_bytes`. Component accounting
is an early retention check, not proof that this final bound passes. Publication
exhaustion remains a typed capture failure and no oversized/truncated report is
written; available source sidecars and the ordinary failure manifest remain.

### Declared controlled fixture datum

Both controlled scripts use actual Swarm logical IDs 1000 and 1100, at Bevy XYZ
centers (15.75, 1.5, 10) and (16.25, 1.5, 10), camera distance 10. Natural scripts
retain camera distance 32. Source extraction checks actual propagated center,
snapshot XY-altitude center, and shader XYZ center agree exactly. The instance uses
[x,z,y], whereas the material uniform uses Bevy XYZ; these contracts are not
interchangeable. This fixture has 200 instantiated actors but only two selected
shells visible and does not establish ordinary 200-shell visibility or art quality.

### Transactional diagnostic publication

Classification is a nondestructive peek of a retained closed interval. Keep its
screenshot record as well until source-sidecar persistence, exact bounded report
encoding/write and capture-record manifest update succeed. Only then acknowledge
publication and clear both. Encoding/IO/accounting failure leaves the full closed
interval available to the existing incomplete-evidence path; no retry discards
unfavorable candidate epochs or changes the closed source/order classification.

### Actual queued-cache regression lifecycle

The first actual AMD test failed because `app.cleanup()` transfers the RenderApp
into the pipelined renderer before the test attempted direct inspection. Its early
error bypassed joined shutdown and the process aborted; preserve gpu01 as failed.
The corrected cache test inspects the real queued ID after finish and before
cleanup, then always performs normal cleanup plus `finish_rendering` before
propagating the assertion outcome. This changes only test ownership sequencing,
not the production pipeline, shader, cache publication or shutdown mechanism.

Controlled-only camera focus is the midpoint of the two actual selected propagated
world centers, with exactly one matching entity per declared logical ID. Missing
actors remain pending; duplicate/nonfinite centers fail. This avoids centering on
the 198 intentionally hidden fixture actors. Ordinary Swarm focus still uses all
200 actors and distance 32; actual camera/projection is retained in every epoch.

## Stage A amendment: validated temporary proxy equivalence

Status: complete planning amendment requested by root after review of the
independent phase-identity investigation. Implementation and new native runs remain
held until explicit release. The current declared twelve-run matrix continues
unchanged. This is a diagnostic classification correction, not Stage B queue-order
implementation or visual acceptance.

### Demonstrated problem and immutable evidence

[The independent review](../reviews/round11-shell-phase-identity.md) records the
pinned Bevy source, exact report hashes, comparison script, six candidate epochs
per run and required limitations. Natural01 genuinely changes sorted logical shell
order. Forward01 and Reverse01 each retain an identical complete sorted phase
payload across epochs 3–8 after removing only the raw item `render_entity`; all
other sorted fields, sources, mist bytes/targets and view metadata are unchanged.
All eight controlled render IDs change each frame. Natural01's 210 render IDs
also change, but removing them does not remove its logical-order variation.

Original reports lack the actual temporary marker/visible-pair validation needed
to admit that exclusion. Their recorded inconclusive classifications, PNGs,
partial/full reports, sources and nonzero exits remain preserved. The offline
projection is separate derived evidence, not a retroactive passed capture.

### Pinned identity and supported draw contract

Bevy 0.15.3 camera extraction (`bevy_render/src/camera/camera.rs:1090–1107`)
creates a marker-only `TemporaryRenderEntity` when a visible main entity lacks a
`RenderEntity` mapping. The actual camera `RenderVisibleEntities` stores the pair
`(render_entity, MainEntity(source))`. The temporary entity need not have a
`MainEntity` component. Material queueing (`bevy_pbr/src/material.rs:728–737,
861–867`) copies this exact pair to `Transparent3d`. PostCleanup destroys these
proxies (`sync_world.rs:233–247`, render `lib.rs:496`). A persistent main identity
and changing per-frame proxy identity are therefore compatible.

The exception is limited to the three existing material adapters:
`BubbleDomeMaterial`, `StandardMaterial`, and `MistWakeMaterial`, each submitted by
the actual registered Bevy material draw tuple:

```text
(SetItemPipeline,
 SetMeshViewBindGroup<0>,
 SetMeshBindGroup<1>,
 SetMaterialBindGroup<M, 2>,
 DrawMesh)
```

Declare a local type alias for this exact tuple. Bevy's private `DrawMaterial<M>`
alias denotes the same Rust type, so it does not require copying a renderer or
accessing a private API. Resolve its ID using
`DrawFunctions<Transparent3d>::read().get_id::<Tuple<M>>()`; do not use the
panicking `id()` or infer a draw type from its formatted number/material label.
Bevy registers the tuple type itself with `add_with::<C, _>`; looking up
`RenderCommandState<Transparent3d, C>` would use the wrong registration key.

Before temporary admission, the actual item's draw function must equal the
resolved ID for its actual supported material variant. Preserve existing checks
for source uniqueness, actual prepared material, exact published pipeline,
mesh/image/uniform source and mist-buffer binding. Missing required draw registry
or missing expected registration is pending during idle warmup and makes an armed
candidate unready. An unknown or mismatched registered draw function is explicitly
unsupported coverage, even when its source material is known; it cannot receive
the temporary-ID exception. A contradictory entity mapping is an error.

This restriction matters: the inspected standard tuple uses empty item queries
and resolves material/mesh bindings through `item.main_entity()`
(`material.rs:450–486`, `render/mesh.rs:2245–2276,2353–2396`). Arbitrary custom
render commands may query the render entity and are outside this proof. Do not
expand the adapter whitelist just to make an unsupported scene pass.

### Files, model and observation responsibilities

Keep the change within `shell_order_audit` and its tests. No simulation, ordinary
renderer, queue/sort ordering, shader, material, asset, editor or screenshot
scheduling change is needed.

- `model.rs`: retain every existing raw phase field. Add explicit association
  evidence with documented main/render coordinate spaces: actual visible pair
  confirmation, temporary marker presence, optional actual render-world
  `MainEntity`, and a typed supported draw adapter/association classification.
  An admitted temporary, admitted synchronized, and unsupported association are
  distinct. Raw render bits remain serialized. Add small explicit semantic
  comparison methods; do not implement lossy global `PartialEq` or serialize a
  normalized replacement record. Keep exact equality available for raw records.
- `phase.rs` (or a narrowly separated `association.rs` if clarity warrants):
  inspect actual per-view `RenderVisibleEntities`, entity components and registered
  draw IDs at both existing observation hooks. Validate the raw sort permutation
  before completing an epoch. All missing/contradictory states use the existing
  typed first-fault/pending/limitation path.
- `source.rs`: only if necessary to expose a checked persistent source mapping,
  add the actual optional main-world `RenderEntity` to the extracted source
  evidence. Absence is expected for temporary sources; never populate it from a
  guessed render ID. If included, retain it in source comparison and accounting.
- `interval.rs`: call the semantic ordered-phase relation after readiness and
  coverage checks; do not change inclusive lower-bound/callback/closure retention,
  nondestructive classification or publication acknowledgement.
- Existing protocol tests plus a focused association test module cover CPU
  validation. `gpu_tests.rs` owns the real extraction lifecycle regression with
  unconditional normal renderer shutdown on success or failure. New modules
  require architecture/rustdoc documentation and explicit error contracts.

The observer is capture-only. Bounded temporary lookup/sort scratch is allowed
within the existing four-view/4096-item/256-epoch/64-MiB limits; it is not evidence
of production allocation-free synchronization. Use a deterministic sorted pair
index or equivalent bounded lookup rather than rescanning every visible list for
every phase entry. Additional serialized evidence counts toward the same exact
final report-size limit; no automatic budget increase or omission of raw data.

### Per-epoch proof before semantic comparison

For every actual queued and sorted item in a populated relevant view:

1. The actual render entity must exist. Its exact full-generational-bit
   `(render_entity, main_entity)` pair must occur exactly once in this view's
   `RenderVisibleEntities::iter::<With<Mesh3d>>()`. Require unambiguous mapping;
   another view's pair, reused index with different generation, duplicate pair or
   one proxy associated with inconsistent main identities is not equivalent.
   A populated supported phase missing its actual visibility association is a
   typed source error, not invisible content that can be skipped.
2. An admitted temporary proxy has the actual `TemporaryRenderEntity` marker and
   a validated supported draw ID. No `MainEntity` component is required on this
   marker-only path. If one exists it must agree with the pair; contradictory
   explicit metadata fails. Any extracted persistent mapping, if retained,
   must not contradict this temporary classification.
3. An admitted synchronized entity lacks the temporary classification and has a
   matching actual render-world `MainEntity`. Any recorded main-world
   `RenderEntity` mapping must agree. Missing proof stays unsupported or errors
   according to the specific missing/contradictory state; it never receives a
   temporary exemption. Recreated synchronized render IDs remain unequal across
   epochs in this bounded amendment.
4. Supported draw identity must match the actual material variant as specified
   above. Unknown custom commands/materials and preexisting multiple-material
   source limitations continue to prevent complete coverage. A correct proxy
   mapping alone does not certify those draws.
5. The queued and sorted observations of one epoch must have identical view and
   source metadata and be exact multisets of raw phase payloads after removing
   only ordinal. This includes raw render IDs and the new association evidence.
   Compare full values, not only a hash, preserving multiplicity. Sorting may
   reorder a payload; it must not change a pair, draw, material or count. Existing
   logical-identity uniqueness checks remain in force.

### Exact cross-epoch relation

Require the current exact source, selected mist bytes/readiness/target buffers and
view membership/identity/matrices/projection/viewport/HDR/color/lighting checks.
Compare complete sorted vectors position by position. Ordinal, main identity,
logical domain/entity/generation, exact finite depth bits including signed zero,
pipeline ID and published descriptor, draw-function ID, material/mesh/uniform/image
source evidence, batch range and extra index all remain exact. New supported
adapter and association classifications must agree.

Only a pair of already validated temporary entries may differ in their raw
`render_entity`. Their remaining association facts and all semantic payload fields
must agree. Synchronized render IDs and camera/view IDs remain exact. Unsupported
entries never become complete merely because a normalized vector matches.
Different queued insertion order is still allowed when the real sorted sequence
agrees, and both original queues remain recorded. Do not sort or group the final
comparison by main/logical identity: that would erase the Natural01 defect.

This relation is within one run. Main-world ECS IDs and pipeline numbers remain
process-local. It neither provides cross-process equivalence nor identifies the
private GPU-copy epoch. A stable conservative interval is still the only proposed
PNG association claim. Keep every candidate epoch and source revision through
closure, including unfavorable, pending and limited evidence.

### Verification stages and negative cases

First run focused CPU tests, then strict affected checks. Tests must establish:

- Only two validated supported temporary entries can differ solely in raw proxy
  ID and remain semantically equal; ordinary raw `PartialEq` still distinguishes
  them. Marker-only temporary entities without `MainEntity` are valid.
- Missing render entity, absent/wrong-view visible pair, duplicated/ambiguous
  pair, wrong main identity, generation reuse and contradictory component/source
  mappings fail explicitly. Missing marker is not admitted as a temporary proxy.
- Missing draw registration is pending/unready; a wrong material-specific draw ID
  or custom draw retains unsupported coverage even with a temporary marker.
- A synchronized render ID change or temporary/synchronized kind change remains
  unequal; synchronized matching IDs and correct mappings preserve the existing
  behavior. Unchanged camera ID is still required.
- Every changed semantic field remains unequal: logical/main generation, sorted
  order at an exact-depth tie, positive versus negative zero depth, pipeline,
  descriptor, draw function, source/uniform/image revision, mesh, batch range,
  extra index and view/source state. Prefer table-driven mutations with one
  baseline admitted record, retaining targeted cases for logical order.
- A queued/sorted mutation, loss or duplication fails even if normalization might
  otherwise hide it. Legal permutation passes while preserving raw values.
- Pending/unsupported evidence cannot be labeled stable. Callback-before-render,
  in-flight closure, every candidate epoch, exact accounting, failed publication
  retention and explicit acknowledgement remain covered by existing tests plus
  an interval case containing temporary-ID churn and a later real order change.

The real extraction lifecycle regression uses the actual production renderer,
material plugin, asset-root loader and supported scene fixture. It must obtain
an actual visible loaded mesh and actual material draw, not inject fabricated
`PhaseEntry` values as end-to-end evidence. Across at least two completed real
extractions, observe the actual view list and marker, the same main/logical source,
a different temporary render ID, unchanged supported payload and successful
semantic comparison. Record/prove that the old temporary proxy has been cleaned
up after its frame; allocator index reuse is allowed but full entity bits must not
alias. Use bounded readiness/deadline, no sleeps that assume GPU completion.
Always drain/join the actual renderer before propagating a test error, preserving
the earlier lifecycle failure lesson. A synchronized fixture exercises its actual
persistent mapping separately; CPU negatives cover malformed states deliberately.

After CPU/lifecycle checks and independent review, preserve the exact changed
source, script, binary and asset inventories. Root declares any new native matrix
before execution. Do not interrupt or revise the current twelve frozen runs, reuse
an old filename, cherry-pick an interval, or automatically repeat until stable.
The new controlled forward/reverse runs should prove complete stable semantic
intervals under actual temporary associations; Natural may remain inconclusive
because real logical order varies. Preserve that result. All PNGs still require
inspection, and no Stage B product tie correction or art acceptance follows from
this classification amendment alone. Parent-coordinated workspace test, strict
Clippy and formatting gates remain required before integrated acceptance.


### Root release of the amended observer's native verification

After reviewing the implemented association/equivalence code, focused negatives and actual AMD lifecycle test, root approves a separately preserved `shell-audit-r11-02` checkpoint. All focused CPU and strict checks must cover the final metadata helper. The headless lifecycle test's five egui-node-data warnings remain a disclosed limitation; it proves actual mesh proxy lifecycle, not native UI readiness.

Before launch root will independently rehash preserved inputs/binary and the declared matrix. The exact eight new pipelined native runs are Off01, Natural01, Forward01, Reverse01, Forward02, Reverse02, Forward03, Reverse03, using new `/tmp/space-scuffle-r11-shell-proxy-<profile><NN>` directories and the unchanged authored17-goal scripts. The prior twelve-run series remains sealed and unchanged. One new Off/Natural pair checks the observer-only correction against that baseline; three new controlled repeats per order test validated stable intervals. Preserve any failed or inconclusive result and finish the declared matrix without source changes or favorable retries. Natural's actual ordering variation may still produce an honest inconclusive result.

Root must inspect or prove exact pixel equality to an already inspected image, decode actual per-epoch association/source/order evidence and compare all preserved inputs. This release changes no ordinary shell ordering, shader, geometry, media or quality threshold. StageB and art acceptance remain held.

### Proxy-correction implementation checkpoint and separate native matrix

The original twelve-run candidate is sealed unchanged at
`tmp/verification/shell-audit-r11-01/matrix-results.json`. The correction is
implemented only in the observer and its tests. Seven focused association tests,
the thirteen preexisting source/protocol tests, strict client all-target Clippy
and workspace formatting pass. Actual AMD image-target lifecycle verification
passes with five explicitly retained headless egui-node-data warnings; it does
not claim native UI rendering. The test uses the same actual loader/plugins,
non-pipelined completed-frame inspection, and unconditional renderer shutdown.

Root approved exactly eight independent ordinary pipelined native runs for the
separate preserved `shell-audit-r11-02` candidate: Off01, Natural01, Forward01,
Reverse01, Forward02, Reverse02, Forward03, Reverse03. Authored seventeen-goal
scripts are unchanged. Outputs use fresh `/tmp/space-scuffle-r11-shell-proxy-*`
directories. The one off/natural pair checks the observer-only amendment; three
controlled repetitions per order retain the original repeat criterion. All
results, including any Natural inconclusive interval, remain recorded. No retries,
source edits, order policy changes or art changes occur within this matrix.
Root independently rehashes candidate inputs before native launch. Stage B remains
held pending separate evidence review and authorization.

### Known shadow-view metadata label: narrow follow-up, implementation held

The frozen Stage A report string calls `ViewLightEntities.lights`
`selected_local_lights`. Locked Bevy's `prepare_lights` instead fills it with
shadow-view entities, including one `LightEntity::Directional` view per cascade
(`bevy_pbr-0.15.3/src/render/light.rs:1430–1466`). `queue_shadows` resolves each
entry's `LightEntity` at lines1511 onward. The retained raw entity values are
correct, but their label is inaccurate. No frozen report or original result may be
rewritten to hide this defect.

After planner/root review and the current gate, the proposed bounded correction
renames only this diagnostic label to `shadow_view_entities`, updates its docs,
and adds the assertion to the existing actual-render lifecycle fixture that the
new label is emitted and the misleading label is absent. No resource selection,
phase scheduling, lighting behavior, shader, production renderer or editor change
is needed. The artifact-name transition must be recorded explicitly in the new
checkpoint; consumers must not infer local-light properties from either old or
new raw IDs. Existing reports keep their original schema/string and companion
erratum. This is not authorization to implement before the gate.

Actual per-shadow-view `LightEntity`, cascade transform and shadow phase-content
correlation is a separate unimplemented evidence extension. It is not required to
establish the directly observed within-run shell-order invariant, and must not be
silently added as a universal renderer inspector. Current cross-run causal
statements retain that source-coverage limit and make no complete GPU-state claim.

### Read-only Stage B audit against completed Natural01 records

During the frozen round11 gate, an offline prediction inspected all six actual
Natural01 candidate epochs from the corrected observer. Artifacts:
`/tmp/space-scuffle-r11-shell-stageb-prediction.py`, `.json`, and `.log`.
The script first reproduces each recorded engine-sorted raw vector exactly from
its recorded queue using the finite f32 radix transformation, before predicting
any canonicalization. This validates the offline model only for these observed
keys; it is not a substitute for real Stage B sorter tests.

Every epoch has one contiguous shell block at queued indices10..209, containing
200 unique `(Swarm, EntityId, generation0)` identities. Standard draws occupy
queued indices0..9. The observed shell material uses one specialized pipeline/draw
pair, so this recording does not exercise heterogeneous pipelines. After sorting,
standard items occupy0..5 and206..209; the predicted permutation preserves their
complete raw payload and both queued/sorted positions. Full raw payload multisets
remain identical. There are no unrelated draws sharing these shell depth keys in
this particular scene; mixed-material exact ties still need dedicated tests.

Ten exact shell depth-bit groups each contain20 identities. Their expected
canonical sequence is `1000+r,1010+r,...,1190+r` for row `r` from0 through9, with
unchanged raw depth bits:

| Row | Actual f32 depth bits | Expected first / last logical ID |
| --- | ---: | --- |
| 0 | 3255685664 | 1000 / 1190 |
| 1 | 3255518966 | 1001 / 1191 |
| 2 | 3255352268 | 1002 / 1192 |
| 3 | 3255185569 | 1003 / 1193 |
| 4 | 3255018871 | 1004 / 1194 |
| 5 | 3254852173 | 1005 / 1195 |
| 6 | 3254591046 | 1006 / 1196 |
| 7 | 3254257649 | 1007 / 1197 |
| 8 | 3253924253 | 1008 / 1198 |
| 9 | 3253590857 | 1009 / 1199 |

These are observed renderer identities and fixture geometry, not production unit
constants or algorithm parameters. The proposed implementation derives keys from
actual instances. The predicted complete sorted semantic vector is equal across
all six epochs after the already validated temporary-proxy equivalence. No Stage B
renderer implementation has run, and no post-correction pixel result is claimed.

### Stage B integration risks and required refinements before release

- Main-world `LabPreviewBubble`, `SwarmBubble`, and `LiveBattleBubble` already
  distinguish the normal producers. Validate exactly one domain and the actual
  `BubbleInstance` identity; `LiveBattleBubble.entity_id` must agree. Ownership,
  team color and material handles never enter a key. The existing Stage A fallback
  generation0 for an unexpected live source is diagnostic-only and must not be
  reused for production admission.
- Battle provenance comes from the successfully committed `BattleSession.frames`
  generation, not candidate `BattleDefinition`, current tab or a guessed tick.
  `manage_battle_scene` prepares before commit, despawns old bubbles and clears
  `RenderEntityMap`; explicit `apply_deferred` follows CommitSession and live
  presentation sync. Extraction must run after these real boundaries. A failed
  candidate retains its old world/generation/identity. Lab return recreates battle
  entities under the same committed generation; reset changes generation while
  preserving logical ordering for repeated EntityIds. Render proxy churn is not
  a producer-generation change.
- `mist/capture.rs::CaptureBubble` currently contains origin/index only. Its reset
  fixture replaces all actors while retaining logical instances and assets;
  existing recorded GPU slot generations belong to the mist allocator, not a
  general standalone shell generation. Therefore the earlier wording “existing
  explicit replacement generation” is insufficient as a shell API. Before code,
  approve a small independent standalone preview epoch resource/component,
  initialized explicitly and checked before the existing replacement commit.
  Do not borrow mist-slot generation or default an unknown producer to a preview.
  Preserve key `(StandalonePreview, logical EntityId)` while changing epoch only
  as provenance. This requires coordinated capture/reset producer hunks, not
  shader/style/upload changes.
- Existing `PresentationStatus::reject` is palette-specific and always sends
  AppExit, including an ordinary client. It does not itself implement the Stage B
  promised readable retained normal-window error policy. A dedicated typed
  shell-order first-fault bridge and explicit runner/capture consumption must be
  specified before wiring: bounded captures fail nonzero; ordinary windows retain
  a readable diagnostic and disabled progression, then return error on normal
  close. Do not repurpose a palette error or silently log a RenderApp fault.
- Queue policy remains mist material queue → actual packet alignment → shell
  material queue → shell canonicalization → engine PhaseSort. No changes to
  `align_transparent_packet` or its admission/removal behavior are needed. The
  passive audit hooks must observe the admitted canonical queue when verifying
  Stage B. Any diagnostic controlled-order intervention needs an explicit declared
  before-canonicalization boundary so it tests the correction rather than
  overwrites it afterward.
- Allocate/reuse per-view scratch before queue processing from actual extracted
  shell membership and real view lifecycle; camera visibility changes alone must
  not trigger hidden growth. Validate all view keys and capacity before mutating
  a block. Unique keys permit allocation-free unstable key sorting followed by
  the documented in-place permutation; duplicate/missing/stale keys fail before
  mutation. Membership/view changes and error publication are distinct from
  steady-state allocation evidence.
- Existing tests/captures that directly spawn real bubble materials must explicitly
  supply supported fixture provenance when they install the new plugin. Keep
  editor plugin registration and its scene sources untouched; do not install a
  global identity fallback to accommodate test/editor consumers. Multiple views,
  heterogeneous pipelines, signed zero, near ties, interleaved unrelated draws,
  replacement and failure propagation remain required tests beyond this one-view
  Natural recording.

All of the above is architecture/read-only audit. Stage B code and the metadata
label correction remain held until the shared gate and explicit root release.
