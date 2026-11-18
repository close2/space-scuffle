# Independent Review of Identity WP5 and WP7, Round 2 (Conditions of Round 1)

Plan: [`docs/plans/2026-11-17-effect-snapshot-identity.md`](../../../plans/2026-11-17-effect-snapshot-identity.md),
Amendments 4 and 5 with their "Review round 1" notes. Packet: [`independent_review_packet.md`](independent_review_packet.md),
section "Identity WP5/WP7, round 2". Previous round: [`independent_review_identity_2.md`](independent_review_identity_2.md).
Branch `effects-identity-looks`, head `2df511bc`; the answer to round 1 is `caa0738c..2df511bc`
(`f9c6d412`, `8a64bb72`, `2df511bc`). Review of Q0080 (D-9). I did not build this work.

## Method

I read the charter, Q0080, the packet section, both "Review round 1" notes of the plan and the round-1 review. I
read the whole answer diff: `effects/beams.rs`, `effects/systems.rs`, `assets/catalog/effects.ron` (`beam_burn`,
`lance_burn`), `client_diagnostics` (`effect_stress.rs`, `report.rs`), the new tests in `weapon_look_tests.rs`, the
changed check in `sync_allocation_count_tests.rs`, the held-beam child in `weapon_battle_gpu_tests.rs`, the Fungal
child's length in `area_link_gpu_tests.rs`, the B-132 row in `DESIGN.md` and the measurements file. I checked
`BattleFrameState::presented_instant` and `pop_presented_event` (when a strike's event is presented), and where
`load_effect_catalog` runs relative to `CatalogResource` (Startup, after the unit catalog is inserted).

I looked at both new pictures and cut and enlarged them with ImageMagick (nearest neighbour, 3x and 4x): the strike
sheet's twelve 1:1 tiles, the late Fungal frames 1 and 5 against the unafflicted units by the station. On the strike
sheet I located the bright violet pixels (R > 200, B > 220, R - G > 40) of tiles 13, 14, 15, 22 and 23 to measure
where the burn sits against the beam's end (the tile's centre). I ran `cargo clippy -p client -p client_diagnostics
--all-targets -- -D warnings`, `cargo test -p client --lib` and `cargo test -p client --test weapon_look_tests`
(results below). I ran no GPU test and no `cargo gate`.

## Verdict table

| Round-1 finding | Verdict | Main reason |
|---|---|---|
| R2-1 (condition): per-strike accent | **resolved** | The flare is plainly visible at 1:1 on the strike frames (15 and 23) and on the frame after; the held structure is unchanged. See "Look judgements" |
| R2-2: the 64-slot cap; refusals | **resolved, one weak test (R3-2)** | The table is sized at load from the catalog and a quarter of the pool (512 shipped); a pool refusal is counted; both are reported in the effect-stress line. Buffers are reserved for the limit and never grow |
| R2-3: beams vanish at battle speed 0 | **resolved** | Deadlines are in presented battle seconds, consistently in `strike` and `keep`; the unit test pauses ten effect-seconds with the battle clock still and finds the same beam, unreleased, and one hold in all. It also fixes fast battle speeds, where `Time<Virtual>` deadlines were too long |
| R2-4 to R2-6 | **no change needed** | As round 1 |
| R2-7: a strike with no line | **resolved** | `let Some(line) = line else { return true; }`; tested, not counted as a refusal |
| Note: the rim alone, late | **seen, accepted** | See "Look judgements" |
| Note: the fog-edge cut | **recorded** | B-132 in `DESIGN.md` §4 states the fault, the location and the owning plans |
| Code: charter rules | **met** | No `unwrap`, `todo!` or marker comment added; `unwrap_or` defaults only; module and item docs updated (the module invariants now name `capacity` and the battle clock); no function near 100 lines (clippy clean); no performance-motivated divergence needing a rationale |
| Code: allocation-free steady state | **met** | `with_capacity(limit)` for both lists, every insert guarded by `>= limit`; valgrind effect stream difference 0 against baseline 0. The relaxed `refused` bound in the valgrind check is argued and bounded by the planted strikes |
| Packet point 2: the burn on a moving target | **answer: yes, it should follow (R3-1)** | Measured 36 px behind the contact point, not "a few pixels" |
| **Overall** | **ACCEPT WITH CONDITIONS** | One look condition (R3-1) before WP8 closes B-126 |

## Look judgements

### The per-strike flare (packet point 1)

In `identity_wp7_beam_strikes_radv.png`, top row, the strike frames (the 3rd and 11th tiles, frames 15 and 23) show
a bright white-violet swell at the contact point about twice the beam's width, unmistakable at 1:1; the frame after
still shows it, smaller, and by the third frame it has gone. The 3x row confirms it sits on the target's rim where the
beam ends. With the Void-Ray analogue's cadence (strikes 8 to 9 frames apart in the measurement) this gives the beam a
clear rhythm without turning it into a stream of projectiles. **The flare is enough; the held core does not also need
a flicker.** A modest core flicker would be closer to the game but is a polish item, not a condition.

### The burn on a moving target (packet point 2)

The burn does not stay "a few pixels" behind. Measured on the 1:1 tiles (crop centred on the beam's end, the target
walking about 4 px a frame, 162.7 px over 40 frames):

| Tile | Youngest burn's age | Previous burn's bright core, centre | Distance from the beam's end (48, 48) |
|---|---|---|---|
| frame 13 | 0.32 s | (84, 51) | about 36 px |
| frame 14 | 0.36 s | (85, 51) | about 37 px |
| frame 22 | 0.28 s | (52, 84) | about 36 px |

The target's bubble is about 17 px in radius at 1:1, so the burn's core sits roughly 19 px clear of the rim: a crisp,
bright violet ring floating on the floor (in tiles 17 to 24 on the beam's line, where it reads as a bead travelling
the beam). Its `beam_steady` core keeps full strength for its whole 0.46 s, so it does not fade out of the eye before
it is far away. In the game the Void Ray's contact glow is on the target; a burn that stays on the ground behind a
moving target undoes exactly what B-126 fixes for the beam. **The burn should follow its target** (R3-1). A burn on a
stationary target is unaffected.

### Fungal affliction, late (round-1 note)

In `identity_wp5_fungal_late_radv.png`, with the stain gone, every victim's bubble has a thin yellow-green rim (about
2 px at 1:1) that pulses (frame 5 brighter than frame 1), and a faint body tint; the unafflicted units by the station
in the same frames have neutral grey rims, so the four victims are picked out at a glance. The rising motes are not
distinguishable at 1:1. It is weaker than the game's spore growth covering the model, but it reads as "afflicted,
green" for the rest of the status, and nothing named shows on the storm stand-ins (rim 0). **Accepted**; a stronger
late body (larger motes, or a slightly wider rim) is optional polish for a later look round.

No picture uses rays, stripes, sunbursts or speed lines.

## Findings

### R3-1 (condition, look): a held beam's burn is left behind on a moving target

`assets/catalog/effects.ron` `beam_burn`/`lance_burn` (Point effects, 0.46 s, core on `beam_steady`), laid by
`spawn_effects_from_events` where the strike landed; `effects/beams.rs` holds the beam but not its burn. Evidence: the
table above, 36 to 37 px between the previous burn's core and the beam's end at 0.28 to 0.36 s on a walking target;
the packet's own "purple dot right of the target". Scenario: a Void-Ray analogue channelling a fleeing unit (the case
B-126 exists for) leaves a trail of glowing beads behind the target, one per strike. Severity: medium (Q0080 "no
cheap looking effects"; it is the burns' behaviour since WP11, but it was invisible behind the old splayed beams and
is now the only thing that does not follow). Requested change: make a held beam's burn follow the beam's end, e.g.
`HeldBeams` keeps the handle of its slot's latest burn and `keep` moves that instance's anchor to the beam's end on the
same frame and geometry as the line (one handle per slot, no allocation; the previous burn retires or fades when the
next strike lays one); or, more generally, anchor a `Point` impact that names `StrikeEnds` to the struck bubble while
it is presented. Show it in the next packet with the same 1:1 sequence and a measurement: the youngest burn's centre
within 3 px of the beam's end on every frame of the stream. Also correct "a few pixels" in the packet and the plan's
note.

### R3-2 (low, test): the table-size test does not pin the catalog bound

`crates/client/tests/weapon_look_tests.rs`, `the_table_is_sized_from_the_catalog_and_the_budget`. The assertion
`capacity == share.min(bound.max(capacity))` holds for any `capacity <= share` whenever `bound < capacity`, so a
`capacity_for` that ignored the catalog and returned the pool share would pass, and with the shipped budget the share
(512) is always the smaller term, so the catalog bound (800) is never the result. The `None` path (a catalog with no
sustained look gives 0) is also untested. Requested change: assert `capacity == min(share, expected)` with `expected`
computed exactly (8 x 200 / 2 x 1 = 800 for the shipped data); add a case with a large `budget.effects` where the
catalog bound wins, and one with the sustained looks' `sustain` removed where the result is 0.

### R3-3 (info): a stale count in the Fungal child's log

`crates/client/tests/area_link_gpu_tests.rs`, around lines 2957 and 3420: "afflictions held" counts spawns of the
kind-keyed `DamageOverTime` status look, which WP5 removed, so it now always prints 0 (`afflictions held 0` in
`identity_wp5_wp7_measurements.txt`) beside "wears the named look on 62". It asserts nothing, but it misleads a
reader of the evidence. The next line's message also carries a run of spaces from a string continued without `\`
("of 357              unseen"); both predate this round. Requested change: count the named look (or drop the
count) and join the string properly when the file is next touched.

### R3-4 (info): the beam's own animation runs on during a pause

`effects/beams.rs`, `BeamContext::now`. With the battle at speed 0 the beam is held (R2-3 resolved) while its onset,
core scroll and erosion still run on `Time<Virtual>`, as the per-strike burns and the other held looks do. The core
therefore keeps scrolling over a frozen battle. This is consistent across the effect layer and is acceptable; record it
only if the owner later asks for a frozen picture on pause.

## Cargo results

- `cargo clippy -p client -p client_diagnostics --all-targets -- -D warnings`: clean for both crates (warnings only
  from the vendored `third_party/naga`, a dependency).
- `cargo test -p client --lib`: 464 passed, 0 failed.
- `cargo test -p client --test weapon_look_tests`: 18 passed, 0 failed (the four new tests included).

## Summary

**ACCEPT WITH CONDITIONS.** Round 1's condition R2-1 is met: the per-strike flare gives the steady beam a clear accent at
1:1, and no core flicker is needed. R2-2, R2-3 and R2-7 are fixed correctly, without allocation or charter breaches, and
B-132 is recorded. The late Fungal rim reads and is accepted. The one condition before WP8 closes B-126 is R3-1: the
per-strike burn must follow the target as the beam does (my answer to the packet's point 2), shown and measured in the
next packet. R3-2 should be fixed with it; R3-3 and R3-4 are notes.
