# Round 5: truthful, shared player presentation

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Status: implementation and focused verification complete in round10. Final normal05/sparse02 native runs each produced17 fresh actual-AMD images with exact effect timing/readback provenance; source is frozen at preservation05 for the coordinated full workspace gate. Earlier numeric-only visual rejections and native04 serialization failure remain preserved. Overall art quality remains separately rejected. This work integrates the accepted36/38/46 lifecycle contracts.

## Goal and scope

The same participant has the same authored identity hue in its battle label, bubble, selection indicator, map-object ownership and combat effects. Preserve ownership as ownership, distinguish neutral entities explicitly, and remove numeric team-to-color guesses. Support every currently valid player slot, including sparse participant sets. This is a client presentation correction with an explicit simulation-to-render ownership contract; it does not introduce alliances, change combat rules, edit the editor, redesign optics or hardcode creature identities.

## Confirmed defect and boundaries

- `sim_types::PlayerId` reserves255 for neutral and the scenario accepts slots0..7. `TeamId` instead documents0 as neutral. These are different domains.
- `SimWorld` currently constructs `TeamId(entity.owner.0)` in bubble snapshots and copies attacker/dead owner bytes into the same type in all combat visual events. Player0 therefore becomes nominal neutral team0.
- `bubble_shader.rs`, `indicators.rs` and five VFX branches interpret1 as blue,2 as red and everything else as gray. Their RGB values differ. The current player0 army is gray and player1 is blue despite the battle labels/map objects describing player0 as blue and player1 as red.
- `MapVisualCatalog.player_colors_srgb` already provides eight authored player colors. Runtime map objects index it by actual owner; the existing UI's fixed descriptions also assume0 blue/1 red. The map-object fallback to white hides invalid ownership.
- StandardMaterial paths construct `Color::srgb`, which converts to linear for rendering. Several custom uniforms instead receive the same sRGB numeric components directly as linear RGB. Centralizing lookup without fixing this transfer-function mismatch would still produce visibly different hues/brightness.
- VisualLab uses team1/team2 for its intended blue/red preview groups. Swarm and preview effect fixtures repeat that convention. These are display fixtures, not actual alliance declarations; their migration must preserve intended identities rather than blindly reinterpret their bytes.
- `VfxSharedAssets` declares blue/red beam materials whose references appear limited to initialization. Confirm all references before removing unused assets; do not keep parallel hardcoded palette paths.
- The UI's colorblind checkbox currently has no consuming color policy. It must not remain a control claiming an effect it does not produce.
- No authoritative alliance assignment/relation table exists. Current targeting, retaliation, hazards and friendly-fire checks largely compare owners. Command authorization also compares owners and must continue to do so. Giving two owners the same color or `TeamId` would not make them allies.

The supported gameplay owner domain remains0..7 plus the explicit neutral sentinel; this plan does not expand the simulation's fixed player storage. “Arbitrary identities” means every supported sparse owner identity is resolved by key, never by participant order or arithmetic offsets. Team identifiers, if future gameplay introduces them, must never index the player palette.

## 1. Repair the render ownership contract

Replace the misleading `team_id: TeamId` in `BubbleRenderInstance` and every ownership-bearing `CombatEventVisual` variant with an explicitly documented `owner: PlayerId`. Producers copy actual owner identity without conversion. Keep ownership in each event so a projectile, beam or death effect remains correctly attributed after its source entity disappears; never recover it through a current-world entity lookup.

Update all constructors, pattern matches, test fixtures, preview emitters and capture assertions together. Do not retain a parallel deprecated field whose meaning can disagree. `TeamId` itself can remain as an unused public domain type; do not reinterpret it as a palette index or claim alliance functionality.

Audit snapshot serialization consumers before changing the field. Current repository coverage uses round-trip snapshot RON; if any persisted/external snapshot reader is found, identify its schema and introduce an explicit version/migration boundary. Do not add a serde alias/default that guesses whether old `team_id:0` meant owner0 or a genuinely neutral preview team. Historical preview data and actual world snapshots have different semantics, so an unqualified byte-preserving compatibility shim is unsafe. Lockstep command payloads, authoritative ownership and simulation checksums do not change from renaming render provenance.

## 2. One authored palette, one validated lookup

Introduce a small client `player_presentation` module owning validated identity colors and conversions. Initially compile the existing `MapVisualCatalog.player_colors_srgb` into this resource: it is the canonical authored source already used by game map objects. Do not add a second independently maintained eight-color array or move editor-specific code into the game. Expose the palette loading/validation boundary so previews can load the same real catalog without constructing a battle/map scene.

Store keyed, validated player entries and an explicitly authored neutral style. Adding a neutral color field to the existing catalog is a backward-compatible data extension for consumers that ignore it; verify this without editing editor code. If the shared schema cannot safely accept that extension, use a dedicated runtime presentation catalog and explicitly migrate game consumers together, documenting the retained legacy editor field. Choose the minimal schema after inspecting the exact deserializers; never silently substitute a hardcoded neutral color.

Validation rejects nonfinite/out-of-range sRGB, missing supported slot entries, duplicate keys if a keyed schema is selected, and invalid neutral identity. Build the lookup once on catalog load. Participant membership is separate from palette availability: a valid palette slot absent from the current battle is not silently a valid snapshot owner. The loaded session validates owners against its participant registry; neutral is explicitly permitted where appropriate. An unknown/unsupported/unregistered owner produces a typed presentation failure with source/session context, surfaced through the existing failure/status path, and prevents a misleading successful capture. It must not panic, wrap/modulo, select the first entry, clamp to a nearby slot or silently turn gray/white.

The resolved style contains both authored sRGB and precomputed linear RGB. Convert sRGB to linear exactly once using the engine's standard transfer function at load time. Egui receives the appropriate sRGB color; StandardMaterial receives an explicitly linear color or the original sRGB color through its documented conversion; custom WGSL uniforms receive linear RGB. Name the representation in APIs and document alpha separately. Use known transfer-function samples in tests, including a middle value, so tests do not simply reproduce the same conversion call.

Identity hue and effect energy are separate concepts. VFX may apply existing effect-specific HDR intensity to the shared linear RGB, and shell optics may attenuate it, but no consumer invents another blue/red base hue. Preserve packed uniform lanes: melee's color `w` carries angle metadata, whereas other materials use alpha. Do not overwrite those lanes with generic opacity or normalize HDR colors to conceal clipping. Any color change is evaluated with the real shader and existing optics acceptance criteria.

## 3. Integrate every game and preview consumer

- Pass the resolved style into `BubbleUniformGpu::from_instance` rather than giving the low-level packer hidden global state or duplicating lookup. Preserve its160-byte GPU layout and optics fields; no shader ABI expansion is needed for this correction.
- Use the same style when creating/updating indicator materials. An ownership or palette revision must update existing entities/materials as well as newly spawned ones. Reuse prepared handles/styles where lifetimes permit; do not allocate a material or format labels every frame.
- Resolve combat event owner once as it is admitted to the VFX path. Preserve the selected style for that event's lifetime or refresh by an explicit palette revision policy consistently; do not recolor a projectile because its source died. Remove proven-unused fixed blue/red material handles.
- Route runtime map-object ownership through the shared palette. `None`/neutral uses the authored neutral style; a bad owner fails explicitly. Natural resource artwork remains authored artwork, not automatically a player-colored object.
- Have36's cached player labels consume the same resource. Participant display names/rosters remain data-derived; colors do not determine control selection, opponent selection or command recipients. Coordinate a narrow adapter if36 finishes first.
- Migrate VisualLab/swarm/effect-preview fixtures to explicit preview participants and owner identities. Map intended blue fixture roles to owner0 and red roles to owner1 in declarative preview configuration, not a scattering of literal replacements that merely happens to match the current catalog. Preserve selection, animation and grouping behavior. Real battle and previews load the same palette source.
- Audit mist for actual identity tint inputs. Its authored wake optics are not automatically team colors; only migrate an existing owner-bearing interface. Do not recolor neutral mist as an unrequested art change.
- Remove or disable the ineffective colorblind control with an honest unavailable state unless an authored alternate palette is supplied and fully wired. If an alternate profile is approved, it must resolve through the same resource/revision path and update UI, map objects, shells, indicators and effects together. Do not claim an accessibility profile based solely on two changed RGB constants.

The hot path receives prepared colors/handles and numeric owner keys; it must not rebuild maps, parse files, format strings or allocate lookup collections each frame. Any optimization diverging from ordinary idiomatic storage gets the required performance rationale. Palette reload, if supported, is a transactional validated resource replacement, not partial material mutation before validation succeeds.

## Alliance semantics: extension contract, not invented gameplay

Owner color identifies a participant. It does not encode hostility. Two actual allied players may retain distinct owner colors; an alliance badge/relationship treatment is a separate presentation channel. Never collapse owner identity to a shared team index, and never infer an ally because two colors or loadouts match.

For the current game, retain the current owner-based gameplay relationship and do not expose a selectable alliance mode or author team assignments that simulation ignores. A future alliance feature must supply an authoritative relation provider from simulation, with explicit neutral/self/ally/enemy semantics and validated team assignments. It needs a separately approved blueprint auditing targeting, explicit attacks, retaliation, friendly fire, hazards, collision/push, fog-sharing policy and checksum state. Shared ownership/control/resources must not follow merely from alliance membership. Once that authority exists, the presentation layer can resolve an additional relation style relative to the controlled player while continuing to use exact owner colors. Sparse team IDs then act only as relationship keys, never palette positions.

Do not add unused future relation-composition APIs or tests in this round. The extension contract above constrains future architecture, not today's implementation. This round's production API must not accept an unverified UI-only team assignment or expose a relation mode without real authority.

## Verification round

1. Unit tests validate sparse owner lookup, neutral handling, unknown/unsupported owners, registration mismatch, palette validation, color conversion and preservation of alpha/packed VFX metadata. Check all supported slots, particularly0,1,2,7, rather than only the former two teams. No hardcoded unit identities in Rust fixtures.
2. Simulation integration creates actual catalog-driven entities for noncontiguous owners and checks bubble and every generated combat-event owner. Source removal after firing/death must not lose provenance. Compare authoritative checksums before/after presentation-only processing to prove no simulation mutation.
3. Snapshot round trips cover all event variants with owner0, sparse owners and neutral. A legacy ambiguous field must be explicitly rejected or handled only by a documented versioned adapter; accidental default-to-neutral is a failure.
4. Client integration loads the actual palette/scenario and checks label sRGB, bubble uniform linear RGB, indicator material/ring values, map owner style and every VFX variant against the same resolved style. Exercise ownership changes and session reset. Assert unknown-owner diagnostics reach the real status path and fail capture completion.
5. Preview fixtures verify intended blue/red groups survive the ownership migration without using a numeric team-to-color match. Search all game sources for remaining hardcoded owner/team RGB branches; inspect results individually rather than treating a broad grep as proof.
6. Real native captures: current default0/1 battle with readable player labels and selected bubbles; a supported sparse-owner scenario; a neutral map object; representative hitscan/projectile/beam/melee/death effects; VisualLab and swarm fixtures. Use actual assets and shaders, and keep HDR energy distinct from identity assertions. Compare measured uniform/material evidence plus screenshots: tone mapping, lighting and translucency mean every screen pixel need not equal the catalog RGB, but labels/shells/indicators must be recognizably consistent without gray player0 or swapped sides. Inspect readability, clipping and effect clarity; reject poor results and iterate.
7. Preserve renderer31 shutdown and capture completion guards. Run coordinated workspace tests, strict all-targets clippy and fmt; then native verification on the rebuilt binary. No editor edits or claims that unrelated baseline editor renderer diagnostics are fixed.

## Proposed file ownership and coordination

Own new client presentation module/tests, render snapshot provenance fields/producers and their direct consumers, and the narrowly required authored palette/preview configuration. Coordinate `scene_controller.rs`, `lab_ui.rs` and shared plugin registration with36/38/39; metadata/media loading remains39's responsibility. Optics33 owns shell appearance and mistGPU35 owns wake rendering; preserve their shader layouts and tuning. Change runtime map-object consumption only and keep the shared palette schema compatible without editor modifications. Implementation starts after36 interfaces stabilize.

This blueprint is complete enough for a scoped owner-color repair. True alliance gameplay and broad accessibility design remain separate features requiring their own authority and acceptance evidence.

## Post38 / pre51 source refresh (Round9; documentation only)

The coordinated38/44/50 gate and immutable post50 preservation must finish before
implementation is released. Root has selected40 as this lane's next implementation;
this refresh does not authorize code during the gate.

### Confirmed compatibility and ownership boundaries

The current serialized render records remain unversioned serde structures; the
repository's consumers found in the audit are snapshot RON round trips and the
reusable-output equivalence tests. No production persisted snapshot loader or
external replay adapter was found. Require the new `owner` field with no alias or
default, and explicitly reject the obsolete `team_id` key (including an ambiguous
record containing both names). Test all ownership-bearing event variants. Keep
command payloads and authoritative checksum golden values unchanged: checksum
already encodes `Entity.owner` and excludes derived render/event buffers.51 alone
will change authoritative transport state/checksum schema.

There is exactly one non-game compile consumer of the removed field:
`editor/playtest.rs` imports `TeamId` and compares
`instance.0.team_id != TeamId(0)`. Root explicitly approved only removing that
import and changing the expression to `instance.0.owner != PlayerId(0)` using the
already imported `PlayerId`. This mechanical shared-game-API adapter preserves
its existing owner0 selection behavior. It is not authorization for editor
features, logic changes, refactoring, new editor tests or unrelated dirty hunks.
Do not retain duplicate/misleading provenance to avoid this two-line adaptation.

Stage40's owner-field/producers/direct-consumers slice first. Notify51 when it
compiles, then relinquish the shared `world.rs` constructors:51 adds loaded-actor
omission at snapshot loop entry, transport-aware reap/shatter emission and target
availability guards;40 owns owner provenance only. Coordinate the visual-event
fixture in `checksum_state_tests`, not the authoritative hash implementation.
The new38 `timeline::compatible` must compare actual owners, preserving its rule
against interpolation across ownership changes.51's after-load presentation
checks must reach the current display boundary (or exact paused current); pending
previous-frame membership is not an immediate disappearance defect.

### Palette schema and startup lifecycle

`assets/catalog/map_entity_visuals.ron` remains the single player-color source.
Both runtime `MapVisualCatalog` and editor `MapEntityVisualCatalog` use ordinary
serde Deserialize without deny_unknown_fields; their existing parsers already
consume different subsets of this same file. Add required runtime neutral and
preview-ownership fields as additive data, preserving every existing field and
all existing eight player RGB entries. The editor parser ignores the additions;
verify compatibility through the existing actual catalog load path during the
normal workspace gate, without changing editor sources/tests beyond the exact
compile adapter above. Missing runtime additions are typed errors, not hardcoded
fallback colors.

Keep the supported owner domain exactly0..7 plus255 neutral. A prepared palette
contains an eight-entry indexed table and one explicit neutral style; lookup
checks the domain before indexing. Participant membership is separately checked
against the successfully committed scenario, including sparse owner lists.
Define neutral as an authored style, not absence of validation. Preserve resource
artwork's ordinary untinted appearance where it has no ownership semantics;
watchtower neutral/controlled tint uses the appropriate prepared style.

Introduce a reusable palette-loader boundary usable by actual normal startup,
headless production fixtures and `mist::capture::run_capture`, which currently
constructs its own App after loading the exact catalog/map/theme. Normal
`build_graphical_app` already loads a real `BattleDefinition` before plugins;
prepare its palette before initial Lab entity spawn. Preview entrypoints must
load the same file via their actual `ResolvedAssetsRoot`, not create a test
palette, assume a battle exists, or inherit a compile-time default.

For declarative preview identity, author explicit primary/secondary/effect roles
and showcase-index overrides in that same presentation data (or one narrowly
named preview ownership catalog referencing the canonical palette). Existing
Lab third-showcase red, other showcase blue; swarm two groups; and preview effect
blue are fixture roles, not creature identifiers or alliances. Validate every
configured role against palette entries before spawning. Do not use modulo or
clamping to turn an unknown owner into a valid style. The existing ineffective
`colorblind_mode` checkbox remains removed/disabled as planned, rather than
pretending these roles implement an accessibility palette.

### Committed38 adapters and stable evidence

`CommittedBattleMetadata::from_definition` is now the sole cached label/color
preparation boundary. Route its color conversion through the prepared palette;
keep player labels, selected control/destination owners, roster counts and cached
order wording unchanged. `BattlePresentation` consumes that committed metadata,
not the latest candidate definition. Prepare palette/participant data before
world/session/root commit and publish it at the same successful commit boundary;
a failed replacement cannot recolor the retained scene. No independent hot
reload is required. If a prepared replacement carries different palette data,
its replacement must be transactional with that successful generation; existing
VFX retain the style resolved when their event was admitted, while active actor
and indicator materials update from the new committed palette without per-frame
allocation. Preview participants use the same prepared palette but their own
explicit fixture registry, not the live battle's participant list.

`update_map_presentation` currently consults the latest `BattleDefinition` and
falls back to white; replace this with the committed palette/style lookup so a
failed candidate cannot recolor map owners either. Unknown map/snapshot/event
owners must reach a typed presentation diagnostic and capture failure. Keep this
separate from39 exact-media errors and38 ordinary command rejection. Media,
command and owner errors must not masquerade as one another.

Capture `CombatProof`, event/source tuples, `event_matches_contact`, visible-owner
counts and passive probe records migrate to actual `PlayerId` provenance. Contact
matching continues to require exact owner/actor/tick/position, never equal color.
Do not discard38's historical `combat_generation`/`ramp_generation`, stable tracked
actor policy, scene readiness, completed-event/outcome evidence, or actual
compositor screenshot verification. The160-byte bubble ABI and44 output-transform
shader contract stay intact. Mist's wake optics have no gameplay owner tint;
only its bubble/event fixture constructors migrate.

Current fixed beam-blue/red handles have no reads beyond initialization according
to source search, so remove them after checking all call sites at implementation.
All shell, ring, facing-marker, hitscan, projectile, beam, melee and death identity
colors resolve from one linear style; existing effect gain/packed angle lanes
remain independent. Observe the existing no-allocation named-path contract:
prepare styles/registries at load/commit and pass numeric colors into packers;
no catalog lookup maps, formatted strings or new material handles per stable
presentation frame. Use snapshot fixture ownership changes or separately loaded
scenarios in tests; mutating a live authoritative owner directly would invalidate
43 supply/51 carrier ownership contracts and is not a valid fixture shortcut.

Root approved the refreshed architecture and explicit preview roles. Showcase
override indices are zero-based in the catalog-selected showcase list, validated
at load against that actual list. Reject duplicate indices and indices outside
the list. Every primary/secondary/effect/override owner must resolve to a valid
non-neutral player palette entry; newly added, non-overridden showcase entries
intentionally use the authored primary role. No modulo, hidden fallback or
creature identifier is involved.40 source is released only after post50
preservation confirmation; the now-passed38 workspace gate does not bypass that
preservation boundary.

## Native verification adapter proposal (Round10; pending review)

The existing seven-checkpoint production `battle_capture` can already prove the
normal0/1 battle, real combat, Lab return and reset, and can load a separate real
scenario and map for sparse owners. It cannot currently express a swarm checkpoint,
a required neutral tower, or each preview effect. Do not substitute fabricated
manifest records or passing CPU material checks for these missing images.

1. Preserve the current production assets. Prepare a separate temporary, versioned
   map fixture by decoding the real canyon `MapFile`, remapping only start slots
   0→2 and1→7, then encoding/loading it through the public map APIs. A separate
   scenario remaps its player/initial-order slots and references that exact file;
   catalogs/loadouts/media/terrain remain the same actual files. Record input/output
   hashes and decoded cell/resource/tower equality. Existing seven-checkpoint
   capture then uses `ramp_player:2`; owner7 is a real participant, not a render
   component edited after spawning. No editor source or shipped map is modified.
2. Add opt-in passive ownership proof to the capture artifacts: actual generation,
   visible entity ID/owner, prepared palette linear RGB, actual shell uniform RGB,
   existing selection-ring material RGB, committed label sRGB and actual event
   source owner. Exact typed identity remains separate from color equality. Retain
   effect entity/material identity and RGB before Lab and after Return while the
   existing zero-duration capture pause keeps effects alive; correlate disappearance
   only with real lifecycle expiry/reset. Historical effects retain their original
   generation and source owner. This is evidence, not control or new game state.
3. A declarative optional neutral-watchtower requirement chooses an actual neutral
   tower from the loaded map, requires its real billboard/material and projected
   bounds inside the gameplay ROI, and captures the scene with a camera focused on
   its actual map position. Natural resources remain untinted. A missing tower,
   offscreen tower, mismatched material or empty pixels fails the goal rather than
   broadening the ROI. This requires an optional `NeutralWatchtower` checkpoint
   following the standard sequence, not weakening Overview's existing assertions.
4. Append optional `Swarm` and `EffectPreview` checkpoints to the same full production
   app harness. Swarm uses normal Lab tab/state controls, the real registry and
   authored primary/secondary roles; it verifies actual instance counts/owners and
   uniforms before capturing a camera framing both groups. Effect previews are
   explicitly fixture demonstrations (never authoritative combat proof): issue the
   same `CombatVisualEvent` variants consumed by the existing UI preview path, with
   `palette.effect()` and positions relative to the actual visible showcase actor.
   Advance one controlled frame to consume/spawn, then retain zero-duration time
   through screenshot completion. Require each actual material branch and visible
   projected geometry. Capture hitscan/projectile/beam/melee/shatter separately;
   the real battle Combat checkpoint remains mandatory and unchanged.
5. Keep existing exact loader/media/pipeline readiness, real AMD adapter verification,
   compositor/native screenshot completion and nonzero failure guards. Decode every
   PNG and review actual owner readability/alignment. Palette identity may improve
   while underlying media/mist quality remains explicitly rejected. No visual
   approval follows merely from numeric RGB equality.

Adapter ownership is coordinated after51's announced config/driver/artifacts edits;
prefer a small dedicated ownership-evidence module plus narrow driver hooks. No
capture adapter implementation starts before root reviews this proposal. Existing
CPU integration and standard production capture preparation can proceed meanwhile.

Root review accepted the native adapter with a strict declared ownership profile:
retain the exact seven base goals in order, followed by the fixed neutral-tower,
swarm and individually named effect subtype goals. Mixed transport/ownership
profiles or goals are invalid. Fixture comparison must cover the entire decoded
map after reversing only the intended start-slot remap, plus all scenario fields
except map path and explicitly remapped player/order slots. Each preview subtype
requires its own actual geometry/material projection and fresh decoded image.

Root additionally approved `CombatEffectOwner(PlayerId)` on every real effect
entity, including each shatter child. Copy the admitted event identity directly;
never infer it from material RGB or query the possibly dead source actor. The
component does not alter rendering or simulation events. Tests inspect every
spawn branch. Capture records generation at first actual observation and retains
that origin through ordinary Lab/Return transitions; reset/expiry cannot silently
relabel existing IDs.

### Native01 correction: pending camera focus is readiness, not success

Preserve `/tmp/space-scuffle-r10-owner-native-normal01`: actual AMD run failed
nonzero after11 fresh images when entering the laser checkpoint. The previous
projectile still had a legitimate1.2-second lifetime. `OwnershipState::settle`
(`runtime_capture/ownership/control.rs`, old-effect branch) advances only real
production lifetime time and returns `None` until it can emit the next subtype
beside the actual selected actor. Bevy's virtual-time clamp means its requested
2seconds does not expire that projectile in one frame. Meanwhile the generic
`observe_inner` four-frame counter (`runtime_capture/driver.rs`, Stage::Settle)
reached `verify_frame`, whose camera invariant correctly rejected absent focus.

Treat this ownership-extension state explicitly as pending: while an ownership
goal lacks its prepared focus, reset its settle-frame counter and do not invoke
`verify_frame` or request a screenshot. Once normal lifetime processing and actor
availability produce focus, require the existing four subsequent settled frames
and every current loader/pipeline/geometry/material/ROI check. Keep wall/frame
limits active before the pending branch, do not manually clear/despawn effects,
and keep simulation paused. All normal seven and cargo goal policies remain
unchanged. Missing actors or effects that never expire fail through the same
bounded timeout; no fallback camera is substituted.

Add a targeted real-VFX lifecycle regression with a1.2-second projectile: begin
next-subtype preparation, run more than four real lifetime updates while it is
still alive, assert no prepared focus/admission to screenshot verification, then
assert focus becomes available only after normal expiry and actual event enqueue.
Also cover the counter boundary: pending preparation resets accumulated settle
frames; exactly four ready frames are required afterward. Preserve native01 as
failed evidence and rebuild/re-preserve before normal02 and then sparse01.

Native review limitations remain explicit: the existing arena platform occludes
many swarm actors;200 instantiated/frustum-visible actors does not establish200
readable actors. Acceptance requires actual visible representatives of BOTH
configured roles inside the unchanged gameplay ROI, correlated to exact snapshot
identity/material proof and inspected fresh pixels. The neutral billboard is
small/faint in native01; numerical owner/material agreement alone does not certify
readability or art quality. No unrelated arena/tower art change is included.

### Native02 correction: quiesce the actual current frame before fresh emission

Preserve native-normal02: pending-focus correction worked, but Laser failed with
zero actual geometries. Exact schedule explains the separate defect: Bevy updates
`Time` before Update; capture ExternalControl requests zero duration only for the
NEXT frame. The current frame still carries the prior real-expiry delta (clamped
to250ms). The newly enqueued laser is spawned in Present and its normal beam
lifetime system runs after spawning in that same frame, so a short beam can expire
before observation. Setting the future strategy to zero alone is insufficient.

After all prior effects have expired, explicitly request zero duration and wait
until actual current `Time::delta()` is zero before enqueuing the next subtype.
Keep focus pending through that quiescent transition. Spawn only in a zero-delta
frame, then use the existing40ms aging request to obtain a nonzero visible shader
age and freeze normally. This is capture sequencing only; do not alter production
schedules, timers, lifetimes or geometry checks. Existing timeout/error checks and
four ready-frame minimum remain unchanged.

Extend the real long-projectile regression through actual event consumption:
require at least one pending current-nonzero-delta frame after projectile removal,
then assert newly spawned laser survives the production ordered spawn→beam update
at age0, subsequently reaches nonzero age under40ms, and remains present when
frozen. This catches the schedule defect that the earlier enqueue-only assertion
did not cover. Rebuild/re-preserve before retry; retain both failed native runs.

### Native03 visual rejection: unobstructed foreground demonstration stage

Normal03 and sparse01 each complete all15 actual AMD callbacks, clean exit0 and
unchanged assets. Preserve this successful numeric evidence, but reject complete
six-subtype visual acceptance: projectile/lightning overlap the second showcase
shell and wake, so actual projected AABB/frustum checks alone are insufficient.

Capture-only refinement: derive the foremost world-Z extent from propagated actual
Lab shell AABBs and visible `MistRibbon` AABBs (the latter already include current
and acknowledged GPU history bounds). Stage the existing admitted preview events
in front of that maximum plus a documented4-world-unit empty demonstration gap,
keeping the existing actor-relative X and height and unchanged real event owner.
The gap is a capture arrangement for existing3-unit beams/0.8-radius shatter, not a
gameplay radius or authored unit constant. Keep every actual actor/wake rendered;
do not hide geometry, disable mist, modify shaders/media or remove scene objects.
Focus halfway between the selected actor's ground-Z and the foreground event-Z,
with the same X beam center; use18-world-unit camera distance for the six effect
checkpoints in both declarative scripts so selected actors and staged effects fit.
Neutral/swarm/mandatory seven camera goals remain unchanged.

Refactor the capture projection helper to expose raw projected AABB rectangles
without the ROI filter. Before accepting each effect checkpoint, compare every
actual effect rectangle against every currently visible Lab shell and mist ribbon
rectangle. Require at least12 physical pixels of separation; reject overlaps or
unprojectable relevant bounds. Continue requiring all effect rectangles fully
inside the unchanged gameplay ROI, positive shader age, correct exact owner and
material, actual pipeline and fresh image. This conservative screen-rectangle
check intentionally rejects even apparent transparent overlaps; pixels must still
be inspected. Record obstacle rectangles/minimum separation alongside the actual
effect rectangles so review can independently establish the clear stage.

Tests cover the screen-rectangle gap at inclusive/exclusive boundaries and actual
actor/ribbon extent selection (without introducing renderer substitutes). Existing
real TimePlugin lifetime tests remain. Rebuild/re-preserve and rerun both complete
15-goal sequences. Artistic VFX/media quality remains separately rejected; this
refinement establishes assessable actual effect evidence, not a VFX redesign.

Foreground amendment assessment before release: inspect the actual locked
`combat_fx.wgsl` mode4. At40ms/1.2s, age=1/30 and its lifetime multiplier is
1−age/2=0.98333. The head center has core5 + halo1.5×0.7 before clamping, so
head alpha is1, with no slow fade-in. Advancing the projectile would not increase
its footprint or solve the observed shell overlap. Keep the already meaningful
40ms sample and existing14-world-unit distance for all six effect goals (this
supersedes the proposed18 above), preserving the roughly65×34-pixel actual
projectile quad rather than shrinking it. The foreground separation and centered
camera, not a shader/scale/lifetime change, provide its assessable stage. Require
actual inspected pixels; a projected rectangle alone is not a coverage guarantee.

Lightning at40ms is22.22% through its actual180ms duration, with lifetime
factor0.7778, but mode2 also contains an explicit global-time/seed strobe that can
produce a fully transparent frame. This is a separate existing visual behavior,
not an ownership failure. A single sample therefore cannot guarantee an assessable lightning image. The
explicit three-phase amendment below supersedes this initial single-sample
proposal without altering shader, seed, owner or lifetime, and without searching
for favorable times.

Extent preparation must query actual `LabPreviewBubble`/`MistRibbon` identities
and propagated `GlobalTransform`/`Aabb`/visibility. At least one loaded Lab shell
must exist. Any relevant missing bounds or not-yet-propagated context is pending
under the existing deadline, never default-zero extent. A known nonfinite bound
is an explicit capture failure. Existing actual scene geometry remains rendered.

### Declared lightning lifecycle samples, preserving off phases

Root's review confirms that unobstructed geometry can legitimately be strobe-off.
Revise both strict ownership scripts to17 checkpoints: retain the original seven,
neutral, swarm, hitscan, projectile, laser, then lightning40ms, lightning80ms,
lightning120ms, melee and shatter. Add explicit middle/late lightning goals rather
than an open-ended retry mode; reject missing/reordered/mixed cargo sequences.
All three lightning samples refer to the SAME actual event/entity/material/seed
and owner. The first goal emits normally; middle/late goals only advance its real
production timer to the next declared elapsed time. Validate the actual spawned
180ms lifetime exceeds every target; never alter that lifetime or choose a target
from the observed strobe value. Preserve all three fresh images, including OFF
phases, and require at least one inspectably visible phase for visual acceptance.

Capture preparation records target elapsed Duration and retained lightning
identity. It requests only the remaining duration from an actual zero-delta frame,
waits for production timer/material update, then freezes through the existing four
settled-frame and GPU readback boundary. An elapsed target overrun, missing retained
effect, changed material/identity or insufficient actual lifetime fails explicitly.
Expose actual elapsed/duration, material seed and current presentation seconds in
effect evidence so each phase can be audited independently. Geometry/ROI/12px
separation remains required even for intentionally retained OFF images; it must
not be described as proof of shader coverage.

Extend the real ordered TimePlugin/VFX regression to lightning40→80→120ms with
unchanged entity/material/owner/seed and no accidental extra age in frozen frames;
keep all-subtype40ms tests. Script tests require exact17 goals and reject omitted
middle/late samples. Reports distinguish17 successful callback/identity records
from final image judgment; all prior15-image runs remain preserved, never relabeled.

Readback freeze amendment (reviewed/approved): save an exact fingerprint of relevant
actual combat effects (entity/material/owner/seed, elapsed and duration as integer
nanoseconds), actual presentation Time elapsed and wrappedf32 shader time with each
ownership record. Locked Bevy0.15 `bevy_render/src/globals.rs:77` assigns
`GlobalsUniform.time = Time::elapsed_secs_wrapped()`; record that actual value,
never infer it from effect elapsed time. Compare the fingerprint on every main-world
Readback observation and again in the actual screenshot callback before saving.
Missing identity/material or any time/state change is typed capture failure even
if simulation tick/checksum stayed unchanged. Keep actual camera/stage focus fixed
through continuation samples. Add a real TimePlugin/production VFX negative test
that advances40ms after sample acceptance and rejects the callback fingerprint.
No renderer/simulation scheduling or production time source changes.

Implementation precision accepted in review: source fingerprints include every
actual CombatFx uniform field (age/seed/aspect/color bits and kind), and finite
material age must equal the actual production Timer.fraction() before readiness.
A same-handle material mutation is therefore rejected even when its timer did not
change. The recorded default-Time elapsed/wrapped seconds are the exact sources
used by locked Bevy Globals extraction, not a readback of private GPU globals.
Frame count is deliberately not frozen: combat_fx reads global time, not frame
count. Four settling frames begin only after the real post-production timer equals
the target AND current delta is zero; a camera focus alone is insufficient.
Rectangle separation uses conventional Euclidean distance between closed projected
rectangles (overlap gives zero), with an exact12-physical-pixel minimum. Each corner
must project validly; depth-clipped corners are failures, never silently omitted.

Native-normal04 serialization correction: the locked RON serializer rejects u128,
so the first overview image saved but its manifest failed loudly. Preserve this
failed run. Encode nanosecond fields as checked u64 integers, returning typed
capture failure if Duration::as_nanos() exceeds u64. The bounded capture deadline
is far below this representation limit; do not truncate, cast or change clocks.
Add real actual-effect fingerprint RON round-trip verification (including full
uniform and exact timer values) before another native run. This changes artifact
representation only, without weakening frozen-state equality or geometry gates.

### Frozen-gate direct-system fixture amendment

The round10 full workspace gate exposed one direct-system fixture omission:
`simulation_diagnostics_tests::live_sync_reports_and_stops_on_invalid_simulation_rate`
registers actual sync/loader/catalog systems without the now-required PlayerPalette.
PresentationContext also requires its normal PresentationStatus diagnostic resource.
Root approved the narrow fixture repair: in that test only, load PlayerPalette from
its existing checked ResolvedAssetsRoot and the actual loaded catalog's showcase
count; initialize PresentationStatus before registering the production systems.
Retain every invalid-buff-rate, first-tick stop, debt, error-exit and UI-evidence
assertion. Do not make a production resource optional, inject fake/default palette
colors, bypass media loading or weaken the fatal-error path. Run the complete
focused test and relevant strict checks before the repeated whole-workspace gate.
