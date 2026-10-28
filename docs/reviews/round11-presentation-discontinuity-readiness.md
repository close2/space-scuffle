# Round 11 preimplementation review of Plan55

This is a static review of the proposed core and selected production-renderer
playback boundary against the current implementation. It is not implementation
approval or execution evidence. Product sources remain frozen for the coordinated
gate. No code, build, GPU run, media or original CPU3 artifact was changed.

## Authoritative transaction boundary

The existing Blink worker in `sim_core/world.rs` already performs eligibility,
range, checked payment and exact destination preparation before acquiring the
mutable entity for effect commit. Prepare the checked next motion revision in that
same immutable stage, after ordinary eligibility, and retain it locally. Revision
exhaustion must reach the existing typed invariant diagnostic before energy,
cooldown, pose, velocity, funnel, channel state or pending-request commit changes.
Do not add a fallible increment after those writes. Existing ordinary rejection
before a cast is eligible need not diagnose an unused exhausted counter.

`transport/mod.rs::unload_transport_prevalidated` validates both participants and
destination, then directly clears the carrier link, sets position/velocity and
clears queues. Insert revision preparation before this mutable commit. On overflow,
carrier link, both entities, occupancy, captured supply and command queues must
remain unchanged; only the documented derived diagnostic may be added by the
public wrapper. Test both direct typed unload and its compatibility wrapper.
Loading alone must not increment, while a successful zero-distance unload/cast
still establishes the declared semantic boundary.

The private field needs boundary tests without creating a production setter that
bypasses the invariant. Use crate/unit-test access or a narrowly test-only fixture
helper to reach `u64::MAX`; do not make arbitrary revision assignment a public game
API. Encode it once in schema5, include exact-byte and mutation-sensitivity tests,
and migrate every real snapshot constructor with a required serialized field.

Atomicity means the attempted operation, not the entire simulation tick. Earlier
regeneration, cooldown updates, another successful command or another actor may
already have committed. Tests must inspect the cast boundary or distinguish those
normal phase changes; they must not claim the whole world rolled back. The queued
cast path currently pops on either ordinary success or ordinary refusal, but keeps
the request on a typed malformed-state error. Revision exhaustion belongs to that
latter path. Keep the pre-cast start-position bookkeeping and ordinary motion
proposal behavior from Plan50 unchanged.

## Timeline and event boundary

The actual `BattleFrameState::present` starts each record from the previous whole
snapshot and interpolates only compatible pairs. Adding revision equality to that
predicate therefore preserves the old whole record for a discontinuous pair at
fraction below one. It does not require a new interpolation branch or an event
queue. `pop_presented_event` already gates events by `presented.tick`.

Tests must exercise the common route where fraction never reaches one: current T
contains the break, but presentation remains T−1 until the next normal tick makes
T the previous boundary. Several consumed ticks may skip the exact cast output
pair; the persistent revision still changes relative to the last displayed slot.
Pause/hold-exact promotion and inactive-tab event suppression remain separate
existing policies. Do not snap the body early while retaining its events, or
release a fabricated Blink visual event: Blink currently emits none.

Unloading after a non-displayed load must still increment and break continuity;
loading that was displayed removes membership normally. Session replacement must
reset the actual ECS/timeline lifecycle independently of revision and protect
recycled numeric EntityIds. Owner/shape compatibility already has separate rules.
The checked proposal must not confuse any of these identities with the GPU slot
generation or use `pos_at_tick_start` as a presentation token.

## Concrete mist preparation risks requiring an explicit policy

The current `mist/mod.rs` loop writes `origin`, `radius`, `boosted`, virtual
position/offset and enable-state before calling `history.advance`. Simply inserting
a checked revision increment at that late call site would violate the proposed
no-partial-mutation contract for the semantic break. Prepare a compact slot action
from the observed record first: validate revision monotonicity, new slot generation,
position, radius, delta/resulting clock and any needed anchor arithmetic, then
commit the affected slot fields and one history action together. No world/slot
clone or new per-frame collection is necessary. Do not broaden this into repair
of every preexisting ordinary-emission arithmetic path.

Two cases need explicit precedence before implementation:

1. **Disabled existing emitter:** its current branch only ages retained history,
   which may still be rendered while fading. A semantic revision change must be
   consumed and retire old-generation history even when emission is disabled.
   Deferring revision handling until `enabled` would leave old spatial content
   associated with the relocated body. This is independent of whether new samples
   are permitted that frame.
2. **Re-enable and revision change together:** current re-enable performs full
   `history.reset()`, clearing clock/path, whereas semantic `discontinue_at` is
   specified to preserve clock/path and advance clock once. Doing both blindly
   changes the stated lifecycle and may age/reset twice. Root must confirm one
   explicit precedence: either existing full re-enable reset dominates while
   consuming the semantic revision/generation exactly once, or a separately
   justified revised enable lifecycle. The narrow recommendation preserves
   existing re-enable semantics, records the consumed revision and advances once;
   the pure semantic-break guarantee applies when no independent full lifecycle
   reset is required. Initial owner acquisition similarly consumes the actor's
   present revision as a new incarnation, rather than an additional semantic break.

These risks were sent to root before any implementation. Required regression
cases are disabled relocation, re-enable-plus-relocation, new-owner acquisition
with nonzero revision, same-revision repeated zero-delta frames, revision decrease,
clock overflow and slot-generation exhaustion. Assertions include history samples,
clock/path, residual/anchor, enable state, generation and remembered revision, not
only visible sample count.

The last-consumed revision must update in the successful CPU slot transaction,
**not** when the GPU later acknowledges a packet. Otherwise a busy uploader would
retrigger the reset/increment every display frame and could keep the ribbon hidden
forever. Conversely, failure before commit must not consume it. Repeated frames
while waiting for a new packet are a required no-perpetual-clear test.

## Retained GPU generation

The existing main-world visibility gate checks the acknowledged generation against
the slot generation. `align_transparent_packet` then validates any actual queued
ribbon against the actual selected packet and rejects a stale generation. Preserve
both gates. A new empty history alone does not retire bytes already selected on the
GPU; the proposed generation change is essential.

The real-device test must exercise actual retained selection, then prove the
broken actor is absent from actual queued draws until a new-generation packet is
selected, while an unrelated actor remains valid. Inspect render identity, packet
generation and queue membership, not a fake acknowledgement or CPU visibility
flag alone. A natural capture with no retained packet cannot substitute for this
case. The same-revision frames between break and acknowledgement must not keep
incrementing generation or clearing newly accumulated history.

## Selected playback and media boundaries

The normal `BattleCommandQueue` remains unchanged. Its generic ability targeting
and result behavior is not certified by this plan. The selected harness consumes
a genuine immutable real-loader/TickBundle trace through the actual timeline and
presentation plugins, with an exclusive replay tick owner and actual authoritative
mirror equality checks. Captures must say that they test renderer playback rather
than normal command/input admission. There must never be two tick owners.

The concrete primary fixture fixes the original Move goal at (20.5,12.5), the
Blink landing at (19.5,12.5), and resumed Move at (24.5,12.5). It requires actual
authoritative and first-visible displacement strictly below the loaded old distance
threshold, together with at least24 physical pixels under the declared fixed
camera. Nominal distance is not proof. No automatic endpoint, camera, actor or
schedule substitution may turn a failed fixture into success.

The full declared prelude must run through the actual timeline/history systems,
retaining clock and cumulative path. Its explicit zero-delta readiness wait at321
must not reinitialize the world or clear histories. Screenshot readback similarly
freezes the existing fraction rather than invoking `hold_exact`, which would
promote the current boundary and invalidate the old-displayed-pose checkpoint.
Check saved/current/previous/presented and mirror identities at every boundary.
Use the final concrete caps and prelude protocol where they supersede earlier
general warmup text; implementation should consolidate those comments into one
unambiguous contract rather than reproduce both alternatives.

Media39 establishes no authenticated animated Ground bundle. A current-media
LegacyUnverified diagnostic can inspect behavior only with explicit status and
root approval. It cannot close authenticated visual/art acceptance. This limits
what a successful screenshot run can claim even if all state/pipeline/readback
checks pass. The full CPU3 experiment remains an immutable failed baseline;
control-capacity, support-cut and field-model problems are not solved by Plan55.

The authoritative and timeline direction is coherent under these boundaries.
Before implementation, resolve the mist lifecycle precedence and preparation
ordering above, retain the approved closed baseline and complete the coordinated
gate. Final acceptance still requires focused transaction/timeline/history/device
tests, meaningful native inspection within the declared media limit and the full
quality gates; this review is not a substitute for them.
