# Round10 ownership presentation verification

Status: implementation and focused CPU verification in progress. Native image
acceptance and the coordinated workspace gate are pending; this is not an artistic
approval of existing creature media, mist or terrain.

## Implemented contract

Render bubbles and combat events now carry actual `PlayerId` ownership. Legacy
`team_id` fields, including ambiguous records containing both fields, are rejected
at the render serialization boundary. Authoritative checksum ownership did not
change in40 (51 independently changes transport state/schema).

The canonical actual `catalog/map_entity_visuals.ron` supplies eight player colors,
explicit neutral color and declared preview roles/zero-based showcase overrides.
Colors are validated, converted from sRGB to linear once, and shared by committed
labels, shells, rings/tethers, towers and combat effects. Invalid identities fail
explicitly. Natural resource artwork is intentionally untinted. Preview roles
preserve intended primary/secondary assignments without numeric team-color branches.
Failed candidate scenes retain the old committed palette and labels together.

The only editor change is the root-approved mechanical shared-API adapter in
`editor/playtest.rs`: remove its `TeamId` import and replace its existing owner-zero
comparison with `owner != PlayerId(0)`. No editor behavior/features were developed.

`CombatEffectOwner` records exact admitted event identity on every actual effect
entity and shatter child, independently of material color. Capture evidence will
retain the first observed generation across ordinary tab transitions. This adds
no simulation event or rendering change.

## Focused CPU evidence

- `/tmp/space-scuffle-r10-owner-check01.log`: client all-target compilation passed.
- `/tmp/space-scuffle-r10-owner-unit01.log`: four palette validation tests passed,
  including independent0.5sRGB→0.21404114linear expectation, sparse players0/2/7,
  explicit neutral, participant mismatch, invalid domains/schema/preview overrides.
- `/tmp/space-scuffle-r10-owner-fixtures01.log`:29 existing client tests passed:
  real-loader headless/live schedule/media/presentation and VFX lifetime coverage.
  The native GPU startup test was explicitly deferred while44 owned the GPU; this
  command is CPU preflight, not the required full workspace gate.
- `/tmp/space-scuffle-r10-owner-integration03.log`: four new material/owner tests
  and12runtime battle tests passed. Coverage includes prepared shell/label identity,
  every VFX branch/packed angle, retained ring/tether recoloring without new ring
  materials, neutral tower tint and failed-candidate palette/label commit.
  The real production-plugin GPU fixture remains deferred to the native interval.
- `/tmp/space-scuffle-r10-owner-snapshot02.log`: two snapshot serialization tests
  and one real combat-producer test passed. The latter exercises15owner/weapon
  combinations (0/2/7 × hitscan/projectile/laser/lightning/melee), source reap and
  retained emitted/death owner. No hardcoded unit IDs select those fixtures.
- `/tmp/space-scuffle-r10-owner-adapter-check02.log`: coherent all-target compile
  of the new optional ownership capture adapter passed. New component, preparation,
  adapter tests and strict lint followed this checkpoint as recorded below.

Failures remain preserved: first new-test compile used incorrect API names and a
Result-returning system unsupported by locked Bevy; corrected with real API calls
and an actually loaded prepared style fixture. One concurrent compile encountered
44's incomplete private capture-system parameter and was reported to its owner,
not bypassed. Clippy exposed nested parse-error size;40boxes the original typed
parse diagnostic, preserving its exact source/path. A concurrent51 exact-distance
lint was handed back and corrected by its owner. No test assertion was weakened
and no GPU/asset loader was replaced.

## Native matrix still required

The reviewed strict ownership profile preserves all seven real battle goals,
followed by neutral tower, swarm and one separate image for each actual preview
subtype. It cannot mix transport goals. Sparse2/7 fixtures are separate actual
MapFile/scenario files; full inverse-remap equality proves terrain/resources/towers
and other scenario data unchanged. Original shipped files remain untouched.

Native evidence must contain fresh decoded PNGs, actual AMD adapter/pipeline
readiness, owner/material/label/geometry proofs, retained effect identity and
original generation through Lab/Return. Preview event demonstrations are explicitly
labeled and never replace mandatory authoritative battle Combat evidence. Visual
quality will be judged from images; numeric hue agreement alone is insufficient.


## Coherent native-adapter checkpoint

`/tmp/space-scuffle-r10-owner-adapter-tests01.log` passes three real VFX
ownership/lifetime unit tests. Its integration targets were filtered to zero by
the unit-test filter, so those are not counted. The subsequent unfiltered
`/tmp/space-scuffle-r10-owner-adapter-tests02.log` passes all six palette/material
integration tests and all seven capture-script tests. Added cases verify complete
inverse-remap equality for the separate sparse map/scenario, exact owner on all17
shatter entities after source destruction, strict15-goal profile and rejection of
mixed cargo goals. `/tmp/space-scuffle-r10-owner-clippy-final01.log` passes strict
client all-target Clippy; `/tmp/space-scuffle-r10-owner-fmt01.log` passes workspace
format checking. Full workspace tests still require the coordinated final gate.

`tmp/verification/ownership-r10-01/preservation.json` records an actual build of
both `battle_capture` and `palette_fixture`, all463 source/asset files unchanged
across build, a copied source tree and binary hashes. The actual fixture executable
successfully generated `/tmp/space-scuffle-r10-owner-sparse-fixture01`; its source
and output hashes are recorded in
`/tmp/space-scuffle-r10-owner-fixture01-inventory.json`. Native GPU verification
remains pending the exclusive renderer interval. No image acceptance is implied by
these CPU/material/script checks.

## Preserved native01 failure and bounded correction

`/tmp/space-scuffle-r10-owner-native-normal01` ran the actual AMD Vulkan renderer,
wrote11 decoded fresh images, and exited1 entering the laser preview with
`capture focus missing`. Its assets remained unchanged and incomplete manifest
correctly retained the failure. The previous1.2-second projectile had not yet
expired under normal virtual-time clamping, so no next-subtype camera focus was
prepared. The four-frame generic settle counter incorrectly advanced anyway.

The reviewed correction treats missing ownership focus as bounded readiness and
resets the counter; four subsequent ready frames remain required. No rendering,
shader, lifetime, simulation or geometry acceptance rule changed. The entire
runtime-capture unit suite passes18tests in
`/tmp/space-scuffle-r10-owner-pending-tests03.log`, including real long-projectile
expiry beyond four updates and the exact four-ready-frame boundary. Earlier test
setup errors are retained in tests01/02 logs (immutable snapshot call/unpublished
presentation buffer); the final fixture reads a real simulation snapshot.
Strict client all-target Clippy passes pending-clippy01. Preservation02 contains
463 unchanged files and actual rebuilt binary
`79b0d06e2c6fd9a83dcad3591ff2551038de5017d625e725440aa29229617e78`.

Native01 image review confirms ordinary UI/actors, actual contact effect,
neutral tower and both swarm roles exist. It does not approve art: the tower is
faint/small, the arena occludes many swarm actors, and media/mist remain rejected.
For swarm09, unobstructed top-row representatives are entity1000/owner0 at physical
(894.869,321.045) and entity1100/owner1 at(1385.131,321.045). Their exact material
linearRGB is respectively(.05087609,.31854683,1) and(1,.10048152,.07323897),
independently of the screenshot. Neither200 instantiated/frustum-visible actors
nor those color values certify200 readable actors or artistic quality.

Native-normal02 also remains a failed11-image run, exit1: laser geometry count0.
The readiness correction revealed that the current frame still carried the previous
expiry delta even after selecting zero time for the next frame. Production beam
aging correctly expired the newly spawned short beam immediately. The reviewed
second correction waits for actual current zero delta before enqueue, and requests
40ms only from an actual zero-delta frame so a stale age0 cannot enqueue another
40ms. Production schedule/timers and all geometry checks remain unchanged.

`owner-quiescent-tests02.log` passes all18 runtime-capture unit tests. The expanded
real TimePlugin/ordered production VFX test exercises all six subtypes, requires
old projectile expiry beyond four updates and a quiescent transition, then proves
spawn elapsed0, exactly40ms elapsed/material age and unchanged counts through four
frozen frames (17 entities for shatter, one for each other subtype). Strict client
Clippy quiescent-clippy02 and whole-workspace fmt02 pass. Preservation03 again has
463 unchanged files, actual binary
`ac84ec66251e38b91fa45185690be379c5b78ffcf9077bf19feed5c99f7eb1ea`.
The first quiescent Clippy run found a test tuple type-complexity lint, resolved by
a named observation struct; no test or renderer policy was disabled.

## Numeric success, rejected images, and foreground lifecycle verification

Native-normal03 and sparse01 each completed15 actual AMD Vulkan screenshots,
exit0 with unchanged inputs. Root independently decoded both manifests and
recomputed owner0/1 versus2/7 palette conversion, UI/material/ring identity,
neutral255, retained exact combat effect IDs/material/source generation through
Lab/Return, and reset termination. Their existing `root-independent-numeric-audit.json`
files remain intact. These runs are NOT complete visual acceptance: the projectile
was a tiny overlapping glint and lightning was unassessable behind shells/wakes.

The reviewed capture-only refinement keeps production events/shaders/lifetimes,
places the demonstration beyond actual propagated shell/wake bounds, and requires
12 physical pixels Euclidean rectangle separation from every visible obstacle.
Missing relevant bounds/visibility remain pending; malformed or overflowing
geometry fails. The original event stage and camera focus remain fixed across
lightning40/80/120ms, with equal-distance validation and all OFF phases retained.
Both strict scripts now declare17 goals. Final native images are still required.

Foreground-tests02 passes25 capture library tests; its integration invocation
filtered all7 script tests and is NOT counted for those. Unfiltered
foreground-script-tests01 passes all7. Foreground-tests03 passes11 ownership tests,
including actual loaded-actor missing-context/malformed-bound admission. Real
TimePlugin/production VFX tests prove exact40ms for all six subtypes and one
lightning entity/material/owner/seed/stage across40/80/120ms. The actual generic
driver counter counts only four subsequent exact-target, zero-delta frames.

Saved readback fingerprints contain actual timer elapsed/duration nanoseconds,
owner/entity/material ID, every CombatFx uniform bit, actual default-Time elapsed
nanoseconds and its wrapped seconds bits. These time values are the exact CPU
sources consumed by locked Bevy Globals extraction, not a private GPU-uniform
readback. Every Readback frame and callback compare them; age must match the
actual production Timer.fraction(). Real extra40ms with unchanged simulation
checksum and same-handle stale shader-age mutation both reject. No frame-count
freeze is claimed. Initial strict lint failures (checked Duration subtraction,
import placement and redundant test qualification) were corrected without
weakening any test or acceptance condition.

Preservation04 recorded466 unchanged source/asset files and actual copied binary
37547e9f8625f8cd20fe76d88851b6d54063817eee63a377cc750cf89b46b87c.
Native-normal04 exited1 after its first fresh overview image: locked RON rejects
u128 fingerprint nanoseconds (`u128 is not supported`), including the subsequent
failure-manifest write. This is a failed artifact run, not partial acceptance.
The log/result/image remain preserved; sparse was not started. The proposed narrow
artifact repair uses checked u64 nanoseconds plus a real fingerprint round-trip.

## Final17-image checkpoint and remaining acceptance boundary

The checked-u64 correction passes28 capture library tests in
`/tmp/space-scuffle-r10-owner-encoding-tests01.log`, including actual RON round-trip
above u32 nanoseconds, explicit overflow rejection, real lifecycle, strict driver
readiness and both readback mutation negatives. Encoding-clippy01 passes strict
client all-target Clippy; encoding-fmt01 passes workspace formatting. The7 strict
script tests passed foreground-script-tests02, including omitted/reordered phases,
mixed transport and unequal lightning camera distances.

`tmp/verification/ownership-r10-05` preserves466 unchanged source/asset files,
12.50-second actual binary build and copied executable SHA256
`8ce4d5e70e908690dd918b73db6ba0bf075fe48b09f7ab5dffa5e18b879aa7df`.
Final native directories `/tmp/space-scuffle-r10-owner-native-normal05` and
`/tmp/space-scuffle-r10-owner-native-sparse02` each contain all17 decoded fresh PNGs,
complete manifests, exit0, actual AMD Radeon890M Vulkan adapter, unchanged assets
and no warning/error/validation diagnostics. Runs lasted55.52s and48.82s; these
are verification wall times, not performance measurements. Both windows closed
normally. Copies and SHA inventory are retained under preservation05/native.

Actual final images show the projectile on open foreground, separate from the
shells/wake. Lightning40ms is an authored OFF phase;80ms and120ms visibly show the
unobstructed arc. All phases remain preserved. Hitscan, laser, melee and all shatter
geometry likewise appear in the separated foreground; no shader, seed or lifetime
was changed to obtain these images. Actual normal05/sparse02 effect-to-obstacle
rectangle gaps range74.9516–156.9054 physical pixels, exceeding the12px contract.
Both runs retain event stage[-2.07,5.35,1.2750001] and focus[-1.0699999,2.675].
Lightning samples retain entity21474837414, actual duration180000007ns and
seed bits1027366912, at elapsed40000000/80000000/120000000ns; actual presentation
elapsed is28140000000/28180000000/28220000000ns and default-Time wrapped-f32 bits
1105272504/1105293476/1105314447, with delta0 throughout every accepted readback.
Each artifact's agent-phase-summary.json exposes these values without inferring
GPU globals or favorable strobe phases.

The full images still reject general art quality: faint neutral tower, visible
Lab floor boundary, arena occlusion of many swarm actors, rigid wakes and legacy
media. Swarm09's actual unobstructed role representatives remain entity1000/owner0
at(894.869,321.045) and1100/owner1 at(1385.131,321.045), including in the sparse
battle profile because preview roles remain explicitly authored0/1. This does not
claim200 unoccluded/readable actors. Sparse live battle labels/material provenance
use actual owners2/7. Overall art approval is not part of this bounded functional
palette/lifecycle acceptance. The final coordinated full workspace gate remains
pending; product source is frozen for that gate.

Root independently accepted final17-image numerical and bounded visibility
contracts for normal05/sparse02. Its all8 sparse effect PNGs are byte-identical to
the individually inspected normal images, including OFF40ms and visible80/120ms.
The first9 normal images and first8 sparse images match previously inspected
counterparts. Swarm does NOT have full-image equality: sparse02 differs2450 pixels
(max channel153) from finalnormal, and2060(max147) from prior sparse01 in central
overlapping shells. Root inspected the actual sparse image and confirmed both
unobstructed role representatives; all200 readability/art remains rejected. This
variance is retained and receives a separate read-only diagnosis in plan53.
The orchestrator has started the coordinated full workspace gate on frozen source.

The first combined round10 gate remained a FAILURE: workspace tests exited101
at423.14s because the direct-system `simulation_diagnostics_tests` fixture omitted
required PlayerPalette startup state. Strict workspace Clippy8.53s and fmt1.57s
passed, and all466 input fingerprints remained unchanged. The fixture also lacked
PresentationStatus. The root-approved narrow repair changes that test file only:
load actual palette from its checked root and loaded catalog showcase count, and
initialize the normal typed presentation diagnostic resource. All original fatal
rate/stop/debt/UI/exit assertions remain. A same-file loading helper keeps the test
under the existing strict line limit without a lint suppression.

Focused diagnostics-tests02 passes1/1 (17.90s), diagnostics-clippy02 passes targeted
strict Clippy, and diagnostics-fmt02 passes whole-workspace formatting. The first
focused test passed too; its strict lint run correctly rejected110/100 test lines
before helper extraction. Product sources remain identical to preservation05;
only this test fixture changed for the second coordinated gate. Gate result is not
yet claimed complete.


Integrated checkpoint: root verified round10c full workspace gate on unchanged448 inputs:688 tests passed across88 harnesses,0failed/0ignored; strict workspace/all-target Clippy and formatting passed. `/tmp/space-scuffle-round10c-workspace-independent-audit.json` retains the independent count/hash audit. Earlier failed gates remain preserved; bounded functional acceptance does not approve art/media or whole-frame performance.
