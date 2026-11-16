# Independent Review of WP11 (the Weapons' Own Looks), Round 2

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md), WP11 row of
§9, §7.2 (checklist), Amendments 24 and 25. Packet: [`independent_review_packet.md`](independent_review_packet.md),
section "WP11 round 2" and the shared sections. Round 1: [`independent_review_wp11_1.md`](independent_review_wp11_1.md).
The bar: [`independent_review_wp10_4.md`](independent_review_wp10_4.md). Branch `effects-wp11`, head `a89bf600`. This
is round 2 of at most five (Q0080). I did not build this work and I did not write round 1.

## Method

I read the packet section, Amendment 25, round 1 in full, and the diff `e66e136a..a89bf600` of `crates/`, `assets/`
and `DESIGN.md`. I read these in full: the new code in `systems.rs` (`shot_clip`, the impact delay), `areas.rs`
(missile landing and compaction), `catalog/resolve.rs` (`effect_reach`), `catalog/units.rs` (`slug_arrival`), the
crescent path of `sprite.wgsl` and `runtime.rs`, the WP11 looks and curves of `effects.ron`, and the new and changed
tests (`weapon_look_tests.rs`, the look-visibility and beside-the-fog parts of `effect_slice_gpu_tests.rs`,
`weapon_battle_gpu_tests.rs`, the status and diagnostics diffs). I checked where the simulation anchors an instant
hit (`sim_core::combat_resolution::emit_instant_hit_visual`). I built and ran nothing.

All pictures are lavapipe. I cut every cell of both looks sheets and the 1:1 and 2x frames of all fifteen battle
sheets with ImageMagick, enlarged them 2 to 4 times without smoothing, and sampled pixels; crops are in
`~/scratch/wp11-review2/`. Sizes are screen pixels at the default zoom (about 34 px per world unit). The lab's
bright floor is sRGB (168, 158, 149), its "dark" floor (99, 146, 144), and the shipped battle floor about
(160, 165, 165).

## Verdict table

| Item | Verdict | Main reason |
|---|---|---|
| A1-1 gauss and cannon slugs | **fixed** | A warm slug about 16 × 4 px (gauss) and 40 × 4 px (cannon) with a white core, legible on both floors and in the battle |
| A1-2 `sniper_tracer` | **fixed** | A cold blue-white slug about 30 × 3 px with a brighter head and a cold flash. Not a hairline |
| A1-3 impacts and particles (lab) | **partly** | In the lab every impact now reads: `bullet_hit` a 12 px hot point then dark dust, `needle_hit` a dark puff, `weld_sparks` a 10 px hot point, `acid_hit` dark from its first frame, burns 16 px. Sparks still hardly show. `impaler_strike` is a dark fin about 10 × 8 px, smaller than asked (0.55 units, not 0.6). **In the battle most of them cannot be seen at all (A2-1)** |
| A1-4 hue on bright ground | **fixed in the pictures; E2-4** | Plasma bolt and hit are blue, spores lime, prism violet, the blade ice-blue on both floors. The sampled colours in the measurements do not show it (E2-4) |
| A1-5 beams | **fixed** | A violet (orange) body about 9 px wide with a white core; in the battle the pulses visibly travel to the target frame by frame, and the prismatic beam stays on through a sustained attack (frames 2 to 11). See P2-2 |
| A1-6 projectiles | **fixed, except `needle_volley` partly** | `acid_spit` a dark glob leading a lime trail, visibly not a tracer; `grenade_lob` a dark 6 px shell with an orange rim; `rocket_trail` a grey smoke trail that reads on both floors. `needle_volley` reads as one dark lozenge with a pale rim (about 20 × 5 px), a seed or a slot, not two spines (A2-3) |
| A1-7 melee (lab) | **partly** | `psi_slash` is now a crescent, pointed at both ends, that sweeps open over two frames: a blow, in the lab. `claw_rake` is a striped almond about 8 × 16 px: three crescents stacked so close that they read as one ribbed leaf. **Neither shows in the battle (A2-1)**, and the swing ignores the published attack angle (A2-2) |
| A1-8 `glass_shatter` | **partly** | A pale glossy ball with a dark rim (about 18 px) and two to four pale flakes of 4 to 6 px near it in the first 0.12 s. It reads as a bubble popping, which is acceptable; the shards are visible but few and not yet thrown to 1.5 radii (P2-3) |
| E1-1 impact timing | **fixed** | `slug_arrival` binds the delay from the travel's `Critical` riding strip at table build; beams correctly keep 0. The CPU test recomputes the same formula (E2-5), but the GPU take shows the slug head within 0.5 px of the end at arrival |
| E1-2 fog reach, lateral cut, leak, takes | **fixed, with E2-1** | Reach is derived per effect, the lateral samples step at most half a cell to the reach, the clip lifetime is recorded as a bounded leak. The beside-the-fog take cuts every shot whole: correct for the design, but the design over-cuts and the take has no positive case (E2-1) |
| E1-3 Fungal fog pixels | **fixed** | 0 of 357 unseen ground samples changed, up to 397 of 619 seen; the overlay shows the impact cut at the edge with the seen half drawn |
| E1-4 status discrimination | **fixed** | Equality against the battle's own `BubbleShatter` events; the positive control drew 16 legacy shards |
| E1-5 visibility measure | **partly (E2-2)** | Per look, area and `Critical` contrast, on two floors, printed. Meaningful as a floor against the invisible round-1 looks; not discriminating for what failed in the battle, because the lab draws impacts on bare ground at height 0 |
| E1-6 DESIGN.md | **fixed** | B-123 and B-124 registered with their fixes and plans; the presentation row and B-107 refreshed; B-122 after B-121 |
| E1-7 steady state | **fixed** | One-pass compaction; valgrind counts recorded: live, effect stream and baselines 0 blocks in the window, the running pair equal (384 = 384), the planted control 64 |
| E1-8 capture check | **fixed** | Counts only live `Shot`, `Point` and `Death` instances while cards are drawn |
| V1-1 staged battle | **partly (A2-1, E2-3)** | The staging is good: one family per shooter at 1:1 and 2x on the shipped floor, away from crystals. It shows that the instant and melee impacts are hidden, and its pass criterion would pass with no effect drawn |
| P1-1 Fungal puffs | **fixed** | The puffs gather over the disc |
| P1-3 crystal props | **fixed** | |
| Steady state and O(N²) (charter §3.3) | **accept** | No new per-frame allocation; `shot_clip` is O(path length × lateral samples) per shot at spawn, bounded; compaction linear |
| **Overall** | **REJECT** | The lab looks are now legible and most round-1 findings are fixed. But in the real game every impact of a hitscan or beam weapon and every melee look is drawn at the target's centre, half a unit up, inside its bubble, so the Marine, Marauder-sniper, tank-cannon, Zealot, Zergling, worker and spine-crawler analogues show no hit (A2-1). The battle evidence shows it and the battle test cannot see it |

## Findings

### A2-1 (blocking): instant and melee impacts are drawn inside the target's bubble

`emit_instant_hit_visual` publishes `to` (and `MeleeSlashVisual::pos`) at the target's position, at terrain height
+ 0.5. `spawn_request` anchors the impact there. That point is the middle of the target's bubble, and the bubble's
glass is drawn over it. Projectile impacts arrive where the projectile stops, at the bubble's near surface, and do
show.

What the battle sheets show, frame by frame at 1:1 and 2x:

- **`gauss_tracer` + `bullet_hit`** (Pulse Bot): the slug reaches the target's left surface on frame 4; frames 5 to
  11 show nothing at the target. My crop: `gauss_tgt.png`.
- **`sniper_tracer` + `bullet_hit`** (Specter): the slug reaches the target on frame 4; no hit on frames 5 to 11.
- **`cannon_tracer` + `cannon_hit`**: an orange speckle visible only through the target's glass.
- **`psi_slash`** (Psi Grey, six impacts spawned): no crescent on any frame, at 1:1 or 2x, only a faint orange
  rim on the target (`psi_zoom.png`). The crescent's radius (0.48 to 0.56, band 0.4 inward) lies inside a bubble of
  about 0.45.
- **`claw_rake`, `weld_sparks`, `impaler_strike`**: nothing visible; the weld's light tints the target's interior.
- **`beam_burn`, `lance_burn`**: a faint tint through the glass; the beams themselves end at the surface and read.

So the melee family, the most common rifle's hit and the cannon's hit are invisible in play, which is the round-1
reason for rejection in a different form. The lab cannot reveal it: `look_lab` puts every `Point` look at
`Vec3::ZERO`, on bare ground with no bubble.

Required: anchor instant and melee impacts at the target's near surface towards the attacker (the target's collision
radius along `from → to`, as `travel_request` already does at the source end; for melee, along the published
`angle_rad`), at a height where they draw in front of the glass or on the ground beside it. This also settles P1-2
(the slug should end where its impact is). The impaler spike must stand outside the bubble (placement radius at least
the target's radius plus the spike's half width). Re-judge every impact in the staged battle.

### A2-2: the blade ignores the attack's direction

`MeleeSlashVisual` carries `angle_rad`, the attack direction, but `psi_slash` and `claw_rake` roll `Together` at a
random angle. A swing across its target should be oriented by the blow: the crescent's opening facing away from the
attacker, swept across the attack line. Bind the roll to the published angle (a `Binding` or a rotation mode) when A2-1
moves the anchor to the near surface; the two belong together.

### A2-3: `needle_volley` reads as one lozenge

The two spines are placed in a `Disc` of radius 0.14 and overlap into a single dark oval with a pale rim, which reads as
a seed or a slot in the floor. Place them side by side at a fixed offset across the path (for example two `LineEnd`
or a fixed pair), so that two spines are seen.

### E2-1: the beside-the-fog cut is fail-closed but over-cuts, and the take has no positive case

To the implementer's question: yes, cutting the whole shot is the correct behaviour **for this design**. The clip is a
window along the shot's axis only; it cannot remove a strip beside the path, so unseen ground within the reach beside
the landing point leaves only "draw nothing" as the fail-closed answer. Nothing legitimate is shown that the viewer may
not see, which is what D-5 requires.

But the design hides legitimate shots more often than it needs to:

- One reach serves the whole path, and the largest layers are at the muzzle (`acid_spit`'s mouth spray gives 1.22
  units, `cannon_tracer`'s smoke 1.15). At the landing end the slug or glob is under 0.3 units wide. A shot into a
  unit that stands within 1.2 units of unseen ground (a cliff edge whose high ground is out of sight is the common
  case) is cut whole, so the player's unit takes damage with no visible shot.
- `effect_reach` counts a sprite's `stretch` (along the path) as lateral reach and adds `lift.max(1) × size × stretch`
  to every sprite, which roughly doubles it.

The GPU take puts the fog at 0.6 × reach, so it only shows the cut; a regression that cut every shot near any fog
would pass it. Required: a positive control in the same take (the fog at, say, 1.3 × reach draws the whole shot), and
either a reach per end (the muzzle layers' reach near the muzzle, the travel's and the landing's near the target) or
a registered bug for the over-cut with its measured worst case. A per-pixel cut (sampling the fog in the shader, as
areas do) would remove the over-cut entirely and is the sound long-term fix.

### E2-2: the visibility measure does not test what failed

`shipped_weapon_looks_can_be_seen` is a fair floor: it would have failed round 1's needles and some sparks, and the
thresholds (40, 80 and 150 px at 32 levels, median `Critical` contrast at 32) are stated and printed. Its limits:

- It draws impacts on bare ground at height 0; the game draws them in a bubble (A2-1). Measure `Point` looks at the
  game's anchor height beside a real bubble, or measure them in the staged battle (E2-3).
- It takes the best of three ages, so it says nothing of how long a look holds.
- The "dark" lab floor is a mid teal (luma about 130); the shipped battle floor (about (160, 165, 165)) is brighter.
- The margins are thin for `impaler_strike` (102 px dark against 80) and `grenade_lob` (105 bright against 40).

### E2-3: the staged battle's pass test is not discriminating

`most_changed` compares each frame with the subject's first frame, and the first frame of every sheet is empty floor:
the units appear on frame 1. The thousands of "changed" pixels (3416 to 9847) are the units, so a family with no
effect drawn passes `SEEN_PIXELS` (30). Compare against a frame from the same moment without the subject's events
(or the frame before its first shot, with the units already drawn), and count only pixels within a window round the
target and the path. Then A2-1 would have failed the test. The empty first frame is also a pop in the evidence.

### E2-4: the display-colour measurement samples bare floor

The A1-4 block samples the window's centre. For 14 of 24 looks (every shot, and `psi_slash`, `claw_rake`,
`impaler_strike`, `needle_hit`) that is bare floor, and the line reports the floor against itself, including the four
looks A1-4 named. The pictures do show the hues (`psi_slash`'s light is (210, 254, 255) over a (98, 152, 224) body;
prism violet (169, 109, 223) at the body). Sample at the `Critical` layer's pixels, or the median hue of the changed
pixels.

### E2-5 (minor): the impact-delay test is tautological

`an_instant_shots_impact_waits_for_its_slug` recomputes the arrival with the production formula and checks the
impact's birth against it, for one unit. The GPU take's "slug head -0.50 px from where the shot reached" is the real
evidence. Make the CPU test independent: sample the slug's head position from the built frame and assert the impact's
first drawn frame is no earlier than the frame in which the head reaches the end, for every shipped instant travel.

### P2-1: `cannon_hit`'s fireball is a hollow rosette

At ages 0.06 and 0.12 the fireball is a ring of six lobes around a hole (both floors). It reads as a wreath. The
erosion eats the centre first; erode from the rim, or keep a core.

### P2-2: a held beam is a fixed line

The prismatic beam is held by overlapping 0.46 s shots, each a line fixed at its spawn. Against a moving target the old
and new beams splay apart for about 0.1 s, and a beam outlives a dead target by up to 0.46 s. Round 1 offered a held
look keyed on source and target; record this as a known limit or do that.

### P2-3: `glass_shatter`'s shards

Two to four flakes in the first 0.12 s, within about one radius. More shards (six to ten), thrown to 1.5 radii within
the first 0.1 s, would make the glass break rather than pop.

### P2-4: `acid_hit`'s crown

The crown is an upright fountain glyph inside a hard-edged dark disc; at 1:1 the disc reads as a flat green button.
Break the disc's edge (breakup) so it reads as a splash.

## Notes on what is now good

- The kinetic, sniper, plasma, spore, acid, grenade and rocket shots read at 1:1 on both floors and in the battle; the
  slug form is right and nothing is a ray or speed line.
- The beams have body, a core and pulses that travel, and hold through a sustained attack.
- The blade is a crescent that sweeps, not a drawn arc. Once it is outside the bubble and turned by the blow, it should
  read as a strike.
- E1-3, E1-4, E1-6, E1-7 and E1-8 are done to the bar asked, and the B-124 half-step extrapolation is in.

## Required changes

| ID | Change |
|---|---|
| A2-1 | Anchor instant and melee impacts at the target's near surface (collision radius towards the attacker), in front of the glass; impaler spike outside the bubble; slug ends at that point (P1-2). Re-judge in the staged battle |
| A2-2 | Orient melee looks by the published `angle_rad` |
| A2-3 | `needle_volley`: two distinct spines side by side |
| E2-1 | Positive control in the beside-the-fog take; a reach per end or the over-cut registered with its worst case |
| E2-2 | Measure `Point` looks at the game's anchor beside a bubble (or in the battle) |
| E2-3 | Battle test against a baseline with the units drawn, windowed round target and path |
| E2-4 | Sample display colours on the look's own pixels |
| E2-5 | Independent impact-delay test over every instant travel |
