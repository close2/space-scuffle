# Independent Review Packet: Effect Vertical Slice (Round 5) and Scale-Out (Rounds 1 to 5)

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md) §7.3.
This packet is for the second reviewer of the vertical slice. It lists the material to judge and the checklist to
judge it by, and nothing else: it holds no verdict of the implementer, on purpose. Please judge each of the three
effects against each of the seven checklist items, write **accept** or **reject** with a concrete reason (which
sheet, which cell or frame, what you see), and put your review in a new file next to this one
(round 5: `independent_review_round5.md`, see "Round 5" below).

## WP10 round 1 (areas and tethers)

WP10 adds looks for the persistent areas and the tethers the simulation publishes (`RenderSnapshot::areas` and
`links`, filled by the spellcaster plan). The caster's presentation block names the look per ability. The decisions
are plan Amendment 20. This is round 1 of at most five (Q0080). Please write your review to a new file next to this
one, `independent_review_wp10_1.md`, in the same form as the WP9 reviews. Judge `psionic_storm`, `blinding_cloud`
and `abduct_tether` against each of the seven checklist items, and check the fog rules and the engineering items
below. Nothing was spent and no texture was made. Every look is built from the existing sets (`smoke_puff`,
`droplet`, `noise_tile`).

The pictures come from `crates/client/tests/area_link_gpu_tests.rs` on the production skirmish. It ran as a debug
build under Xvfb at 1280×720, at the battle camera's default distance, on radv and on lavapipe. The files start
with `wp10r1_`.

### What the looks are meant to be (plan §5.5; art rules: no rays, stripes, sunbursts, beams or speed lines)

| Look | Ability (caster) | Meant to read as |
|---|---|---|
| `psionic_storm` | Psionic Storm (High Templar analogue) | A churning indigo-violet cloud over the area, lit from within by pockets of discharge that swell and fade all over it (laid every 0.11 s, `storm_flare`), a violet glow on the ground under it, and a dark violet band whose outer lip is the area's true edge (`Critical`). The discharge is light inside the cloud, never a stroke of lightning. |
| `blinding_cloud` | Blinding Cloud (Viper analogue) | A low pall of dark grey murk with a cold cast, rolling as billows swell and thin in it (every 0.2 s, `murk_billow`), over ground darkened in its shade; the darkened front's outer lip is the area's true edge (`Critical`). |
| `abduct_tether` | Abduct (Viper analogue) | A dark red-brown tongue (opaque, no glow, no bright core) from the caster's body to the victim's, its surface rippling back towards the caster, with a grip clamped on the victim. Its ends follow the two bubbles every frame. |

### How a look is held, and the fog (Amendment 20)

- A look is held while its area or tether is published. It erodes out over its `exit` when the area or tether
  ends. An area seen on the tick it opens comes in; one first seen later is shown already settled.
- An area of the viewer's own is drawn whole. An enemy's area is drawn only where the viewer sees it:
  - wholly in fog, it holds no look;
  - seen in part, it is cut by a straight line across the disc, and the cut fails closed (no unseen ground is
    drawn). Over the fog's staircase of 1-unit cells the straight cut gives up some seen ground next to the edge.
- A tether is drawn only while both of its bubbles are drawn as they are. A tether from a caster in fog is never
  drawn.
- The storm had a point light in an earlier draft. The light lit unseen ground across the fog edge, and the fog test
  caught it. Area and tether looks now carry no light, and a catalog test enforces this.

### Not presented in a battle, and not covered

- The three shipped casters (`cerebral_elder`, `mycelium_shaman`, `tongue_chameleon`) have no unit media, so the
  client does not present them in a battle yet. Every picture here comes from a presentable stand-in unit that was
  given copies of the shipped abilities and blocks in the test world's catalog only. The stand-in casts through the
  simulation's own cast order.
- Fungal Growth has no area look. The simulation publishes no area for it: its impact is one search. A player sees
  it today only through the rooted units' slow look (`concussive_daze`). The battle sheet therefore shows the storm,
  the cloud and the tether, but no fungal.

### Material

| What | Files |
|---|---|
| Each look's sequence at the default zoom, from the cast, every sixth 40 ms frame (0.24 s apart), cropped round it: the storm, the cloud, the tether (cast after the two areas, so its frames come later) | [storm](wp10r1_storm_sequence_radv.jpg), [cloud](wp10r1_cloud_sequence_radv.jpg), [tether](wp10r1_tether_sequence_radv.jpg); lavapipe: the same names ending `_lavapipe` |
| Whole battle window with all three at 0, 0.8, 1.6 and 2.4 s after the casts | [radv](wp10r1_battle_sequence_radv.jpg), [lavapipe](wp10r1_battle_sequence_lavapipe.jpg) |
| The measured area shots, 2x: whole look, `Critical` layers only, no effect layer (the cloud is at the top left, the storm below it) | [radv](wp10r1_areas_radv.jpg), [lavapipe](wp10r1_areas_lavapipe.jpg) |
| The measured tether shots, 3x, top to bottom: whole look, `Critical` layer only, no effect layer | [radv](wp10r1_tether_radv.jpg), [lavapipe](wp10r1_tether_lavapipe.jpg) |
| The fog case, 2x: an enemy storm across the edge of the viewer's sight, shown whole, with its `Critical` layers only, and with no effect layer. The cut sliver at the right is what the viewer sees of it. The enemy's tether, both of whose ends stand in the fog, is not drawn. The pulled unit at the top is the viewer's own earlier tether target. | [radv](wp10r1_fog_radv.jpg), [lavapipe](wp10r1_fog_lavapipe.jpg) |
| Measurements on both drivers: edge error per ray, tether end errors, fog sample counts | [wp10r1_measurements.txt](wp10r1_measurements.txt) |

The test checks, on each driver:

- **Area edge.** Along 32 rays per area, the outermost pixel that the `Critical` layers change must lie within
  ±2 px of the projected `AreaRadius`.
- **Tether ends.** The `Critical` tongue must run from each bubble's surface point to within ±2 px.
- **Fog.** The enemy storm's `Critical` layers may change no unseen ground sample. The whole look must change at
  least a quarter of the seen samples. The enemy's tether must hold no look.

A valgrind window (`sync_allocation_count_tests.rs`, the effect stream) also holds a live area, which lays its own
every 0.1 s, and a live tether every frame.

### Points to look at

- Whether the storm reads as a storm at gameplay zoom: a roiling energy cloud with its true edge clear, rather than
  a flat glowing disc. Check for any line- or ray-like element.
- Whether the blinding cloud reads as a low murk whose edge is clear on a light floor, or only as a grey stain.
- Whether the tongue reads as a fleshy tongue and not as a laser beam or a plain rod, at the default zoom and in the
  3x crop.
- Whether the partly seen enemy storm in the fog case reads acceptably: how much of its seen part the fail-closed
  cut gives up.
- Whether anything pops at the start or the end of the sequences: the areas come in over 0.12 to 0.3 s and erode out
  over 0.3 to 0.45 s.

## WP9 round 3 (status and death looks)

Round 2's review ([`independent_review_wp9_2.md`](independent_review_wp9_2.md), kept as written) accepted
`stim_surge`, the art of `barrier_shell`, `barrier_absorb`, E1-1, E1-2, E1-4, E1-6 and the fog rules. It rejected
`concussive_daze` (narrowly), `charge_streak`, `bio_burst`, `vehicle_wreck` (its round-2 embers) and the A1-6 mask,
and asked about the barrier shell's first 0.8 s (E2-1). The orchestrator's decisions are plan Amendment 18. This is
round 3 of at most five (Q0080). Please write it to a new file next to this one, `independent_review_wp9_3.md`, in
the same form: judge `concussive_daze`, `charge_streak`, `bio_burst` and `vehicle_wreck` against each of the seven
checklist items, check A2-5 and E2-1, and say whether anything accepted in round 2 regressed. Nothing was spent and
no texture was made.

The pictures come from `crates/client/tests/status_gpu_tests.rs` on the production skirmish (debug build, Xvfb,
1280×720, the battle camera's default distance) on radv and lavapipe, and from the effect lab on both drivers;
files start with `wp9r3_`. Only the evidence of what changed was re-taken; rounds 1 and 2's files are unchanged
(the stim, onset and pair pictures of round 2 stand).

### What changed

| Round-2 finding | Change |
|---|---|
| A2-1 `concussive_daze`: the drag dust cannot be seen | Dropped, as the review allowed: `daze_drag` and its ramp are removed. The greyed, dulled body and its one pale flare at the blow carry the slow; nothing else of the look changed. |
| A2-2 `charge_streak`: scuffs fade in over 0.5 s; dust has no value contrast on a light floor; the scuffs merge into one band | A scuff is whole from its first frame (new curve `scuff_hold`: full strength from 0, fading over its last quarter; erosion only at its end, `erode_tail`). It is three quarters of a radius long, narrower (width 0.45) and less broken up, laid every 0.1 s (was 0.06 s) so that lays are separate marks. The dust is much darker (`charge_kick`: near-black brown to a dark umber, opacity 0.70 to 0.82) and larger (0.95 radii). |
| A2-3 `bio_burst`: the stain is a round disc with a hair fringe; drops alike | The stain is a smaller core with three large soft lobes thrown out round it (each a soft-edged disc whose outline wavers only gently: edge noise 0.06, soft 0.45 to 0.5, breakup at a lower frequency), so its silhouette is irregular at low frequency and its edge filtered. Drops: four round ones of very different sizes further out (size jitter 0.6) and three elongated drips (width 0.35). |
| A2-4 `vehicle_wreck`: the round-2 ember decal read as flat filled shapes | Removed. The brief ember glow of round 1 is back under the blast, and 12 to 20 burning coals (`wreck_coals`, new additive ramp `coal`) lie in the wreck, small points seen through its smoke, cooling from yellow-white through orange to dull red as they age and flickering (`coal_flicker`) until they go out after 1.2 to 2.2 s. |
| A2-5 the exclusion disc round a node | Removed: no decal has a hole any more (`DecalInstance` is 144 bytes again; `collect_ground_obstacles`, `nearest_obstacles` and the shader's exclusion loop are gone). Nodes and structures are already drawn after the ground marks: the marks are the first transparent items, and node billboards (additive) and structure domes (blended) come after them. A node's own light is therefore unchanged over a stain; what changes inside its outline is the floor seen through its additive billboard. Covering that floor would need an alpha-blended node material (new node art), which is outside this plan. The node shots are now: stain as shipped, stain with the node hidden, no effect layers, and no effect layers with the node hidden. The node's pixels are where its billboard adds 24 levels or more over the floor; the test asserts that the stain dims the node's own light on at most 5 % of them and that the floor round it stays stained. |
| E2-1 the barrier shell invisible for the first 0.8 s of the battle take | A defect, fixed. The effect renderer asked for its card and decal pipelines only once cards existed, and a pipeline compiles on first use, so the first effects of a session went undrawn while it compiled; the barrier, staged before anything else drew, was its victim. The shell's card was built from the unit's third frame in round 2 as well (now printed and asserted), so the hold was sound. The four pipelines are now asked for every frame (`queue_effects`, with a performance rationale). A runtime test also checks that a look first seen settled draws as fully as one that came in. |
| P2-2 `barrier_absorb` (advisory) | The flash's peak is whiter and hotter (`barrier_flash` top `(1.00, 1.00, 0.94)` at opacity 0.75). |
| P2-1, ranged absorb take (advisories) | Not taken this round. |
| P2-3 the smog's pink wake | Outside this plan (D-9: the smog is untouched); recorded for the owner in Amendment 18. It is still visible behind the charger. |

The dust test now counts a lay as the charge's when either of the two frames it shows between charges: the lay and
the sample are not ordered against each other, and on lavapipe one lay landed in the frame the charge began.

### Material

| What | Files |
|---|---|
| Battle sequences at 1.5x: the barrier under fire, every 40 ms frame of the first 1.2 s (E2-1), the charge (crop follows the charger), every 40 ms frame; the slowed unit and the deaths at t = 1.0 s, every third frame | [barrier](wp9r3_barrier_sequence_radv.jpg), [charge](wp9r3_charge_sequence_radv.jpg), [daze](wp9r3_daze_sequence_radv.jpg), [deaths](wp9r3_deaths_sequence_radv.jpg); lavapipe: the same names ending `_lavapipe` |
| Whole battle at 0.4, 1.2, 2.0, 2.8 s | `wp9r3_battle_{0..3}_{radv,lavapipe}.png` |
| A death beside a resource node (A2-5), 3x: stain as shipped, stain with the node hidden, no effect layers, no effect layers with the node hidden (the hidden shots exist only to measure) | [radv](wp9r3_node_stain_radv.jpg), [lavapipe](wp9r3_node_stain_lavapipe.jpg) |
| Effect-lab contact sheets and sequences on both drivers: `vehicle_wreck` (`UnitRadius` 0.6), `bio_burst` (0.5), `barrier_absorb` (0.5), and `charge_dust` (0.5, six lays 0.1 s apart on one spot: in the lab the lays overlap, in a battle each is left behind the moving charger) | `wp9r3_lab/<driver>/<effect>_contact.jpg` and `_sequence.jpg` (legend `_contact.txt`) |
| Measurements on both drivers | [wp9r3_measurements.txt](wp9r3_measurements.txt) |

Where the subjects stand in the battle frames (radv, window pixels): as in round 2. The slowed unit at (551, 249),
the charger at (684, 219) charging the enemy at (595, 221), the shielded unit at (793, 220) and its attacker at
(800, 262), the dying units at (892, 231) and (898, 263) (`vehicle_wreck`), (909, 288) (`bio_burst`) and the
control at (920, 361). The red-white crescents next to the shielded unit are the legacy melee slash, and the white
streaks the legacy hitscan tracer; WP11 migrates them.

### Points to look at

- Whether the slowed unit still reads as slowed without the dust.
- Whether the charge's trail reads as torn ground and kicked-up dust at gameplay zoom, as separate marks, and from
  the frame it is laid.
- Whether `bio_burst`'s stain reads as an organic splash with irregular, soft lobes and varied drops.
- Whether `vehicle_wreck`'s coals read as hot points cooling in the wreck, not as painted shapes.
- Whether the node reads as standing over the stain, and whether the stained floor seen through its additive
  billboard is acceptable.
- Whether the barrier shell is there from the first frames of the battle take.

## WP9 round 2 (status and death looks)

Round 1's review ([`independent_review_wp9_1.md`](independent_review_wp9_1.md), kept as written) accepted
`vehicle_wreck` and every engineering and fog item; it rejected `stim_surge`, `concussive_daze`, `charge_streak`,
`barrier_shell` and `bio_burst`, and asked for evidence on `barrier_absorb`. The orchestrator's decisions are plan
Amendment 17. This is round 2 of at most five (Q0080). Please write it to a new file next to this one,
`independent_review_wp9_2.md`, in the same form: judge `stim_surge`, `concussive_daze`, `charge_streak`,
`barrier_shell`, `barrier_absorb` and `bio_burst` against each of the seven checklist items, check A1-6, E1-1, E1-2,
E1-4 and E1-6, say whether `vehicle_wreck` (changed for P1-1 and P1-2) or anything else regressed. Nothing was spent
and no texture was made.

The pictures come from `crates/client/tests/status_gpu_tests.rs` on the production skirmish (debug build, Xvfb,
1280×720, the battle camera's default distance) on radv and lavapipe; files start with `wp9r2_`. Round 1's files
are unchanged.

### What changed

| Round-1 finding | Change |
|---|---|
| A1-1 `stim_surge`: fumes invisible; flare never pictured; body not visibly warmed | The picture is warmed far more (tint share 0.14 → 0.34). The fumes are 0.6 radii (was 0.3), denser (opacity 0.30 to 0.58), lifted over the crown, laid every 0.35 s. The onset is pictured (V1-1). |
| A1-2 `concussive_daze`: rim like every bubble's; body not greyed; drag dust invisible | No rim. The picture is greyed and darkened (tint share 0.62) and the glass dulled (matte 0.75). Drag dust of 0.55 radii in a denser grey-brown, every 0.25 s. |
| A1-3 `charge_streak`: violet rim on a violet unit; dust a speck; no lunge | A hot white outline (rim 0.88). Every 0.06 s a darker scuff of one radius with a broken outline and two dense light-tan dust puffs of 0.75 radii. The charge now crosses clear ground and the crop follows the charger (V1-2). Nothing line-like. |
| A1-4 `barrier_shell`: faint, mostly the lower arc, gone after hits | The membrane stands at 1.32 radii, its ramp is denser (opacity up to 0.82), its floor 0.75 (was 0.45), its face crackled less. The upper arc was there but faint on the bright floor. The ring figures at 8 and at 24 levels are in the measurements. |
| A1-5 `bio_burst`: translucent mauve disc, then a round coin; no globs | An opaque dark splash crown (thickened), larger and more globs and a fine dark spray. The stain has a lobed disc outline (edge noise 0.22, soft edge) and six satellite drops round it; it darkens the floor (dark red-brown) instead of tinting it. |
| A1-6 decals paint a resource node | Every ground mark leaves the nearest resource nodes and structures (up to four within 4 world units) unpainted, fading out over 0.15 world units at the edge of each one's disc. Marks still spawn next to them. |
| E1-1 deaths proven only by a total count | Each named death's effect is counted by effect id (two `vehicle_wreck`, one `bio_burst` in the take). A control unit whose block names no death dies with them and must draw legacy shards (16 near it), the named deaths none. |
| E1-2 dust and fumes not asserted | The charge's dust is counted per frame: laid only while the charger moves under its status, never otherwise. The stim's fumes are counted against their beat (10 over 10.1 beats). |
| E1-4 the two radv "down" pixels | The test prints them. In this round's take "down" changes no ring pixel on either driver. |
| E1-6 the pink haze | It is the smog's wake behind a moving bubble (the bubbles' velocity stirs the smog volume), not an effect layer: `wp9r2_battle_haze_*` and `wp9r2_battle_bare_*` are consecutive frames with and without effect layers, and the haze is in both. Smog is untouched by this plan. |
| V1-1, V1-3, V1-4 onsets and pairs | A third child stages pairs of one type side by side. At t = 1.00 s one of each takes its status while the camera watches: the stim (the simulation's own overclock), a concussive slow (the simulation's own slow; the shipped shells do not yet apply it), and a barrier raised by the first hit of a ranged (hitscan) attacker and, in a second pair, of a melee one. |
| P1-1, P1-2 `vehicle_wreck` (accepted; advisories) | Black smoke, its own heavier dark wreckage, and embers that glow on in its scorch for 2 s, cooling from orange to dull red (a shell's die within a second). |
| D-5 | Recorded in Amendment 17: a death of an undetected enemy in a visible cell is shown, as the glass shatter was and as StarCraft II shows it. |

### Material

| What | Files |
|---|---|
| Onsets, every second 40 ms frame from t = 0.76 s (the status sets in at 1.00 s): stim, slow, barrier raised by a ranged attacker, by a melee attacker | [stim](wp9r2_onset_stim_radv.jpg), [daze](wp9r2_onset_daze_radv.jpg), [barrier, ranged](wp9r2_onset_barrierranged_radv.jpg), [barrier, melee](wp9r2_onset_barriermelee_radv.jpg); lavapipe: the same names ending `_lavapipe` |
| Pairs (V1-4): before the onset and with the status, at 1:1 and 3x, the acting unit on the left, its twin on the right | [stim](wp9r2_pair_stim_radv.jpg), [daze](wp9r2_pair_daze_radv.jpg), [barrier, ranged](wp9r2_pair_barrierranged_radv.jpg), [barrier, melee](wp9r2_pair_barriermelee_radv.jpg); lavapipe likewise |
| Battle sequences, every third 40 ms frame at 1.5x: stimmed and running, slowed and walking, the charge (crop follows the charger), the barrier under fire, the deaths at t = 1.0 s | [stim](wp9r2_stim_sequence_radv.jpg), [daze](wp9r2_daze_sequence_radv.jpg), [charge](wp9r2_charge_sequence_radv.jpg), [barrier](wp9r2_barrier_sequence_radv.jpg), [deaths](wp9r2_deaths_sequence_radv.jpg); lavapipe likewise |
| Whole battle at 0.4, 1.2, 2.0, 2.8 s; and at 1.6 s with and without effect layers (E1-6) | `wp9r2_battle_{0..3}_{radv,lavapipe}.png`, `wp9r2_battle_haze_*`, `wp9r2_battle_bare_*`; the onset take: `wp9r2_onset_whole_*` |
| Barrier pose, default zoom, 4x: standing, 0.5 left, 0.5 without effect layers, 0.05 left, down | [radv](wp9r2_barrier_pose_radv.jpg), [lavapipe](wp9r2_barrier_pose_lavapipe.jpg) |
| A death beside a resource node (A1-6), 3x: mask off, mask on as shipped, no effect layers | [radv](wp9r2_node_stain_radv.jpg), [lavapipe](wp9r2_node_stain_lavapipe.jpg) |
| Effect-lab sheets (lavapipe) of `vehicle_wreck`, `bio_burst`, `barrier_absorb`, and of what the statuses lay (`charge_dust`, `stim_fumes`, `daze_drag`, repeat takes), with the bindings of round 1. The radv lab sheets were not re-taken this round: a package gate was running and long GPU captures would have starved its test children; every battle picture above is on both drivers | `wp9r2_lab/lavapipe/` |
| Measurements on both drivers, and the clean radv `gpu_validation_tests` run | [wp9r2_measurements.txt](wp9r2_measurements.txt) |

Where the subjects stand in the battle frames (radv, window pixels): the stimmed unit at (586, 383), the slowed unit
at (551, 249), the charger at (684, 219) charging the enemy at (595, 221), the shielded unit at (793, 220) and its
attacker at (800, 262), the dying units at (892, 231), (898, 263), (909, 288) and the control at (920, 361). In the
onset take: stim pair (581, 384) and (622, 384), slow pair (550, 251) and (586, 251), ranged-barrier pair (691, 219)
and (726, 219), melee-barrier pair (793, 218) and (829, 218); the acting unit is the left one of each.

The red-white crescents next to the shielded units are the legacy melee slash (`combat_fx`), and the white streaks
the legacy hitscan tracer; WP11 migrates them.

### Points to look at

- Whether the stim, the slow and the charge each read apart from an ordinary unit of the same type at gameplay zoom
  (the pairs), and whether the slowed unit reads as slowed rather than as a selected, cloaked or dead unit.
- Whether the charge's trail reads as torn ground and kicked-up dust, with nothing line-like.
- Whether the barrier reads round the whole body at full, half and a twentieth, and the absorb flash on its own
  (the ranged take) and next to the melee slash.
- Whether `bio_burst` reads as an organic splash, and its stain as a splash rather than a coin.
- The node stain: whether the unpainted disc round the node reads acceptably.

## WP9 round 1 (status and death looks)

Plan [Amendment 16](../../../plans/2026-11-09-effect-visualization.md) adds WP9: the looks of four statuses and
death effects chosen by a unit's presentation block. Please write this review to a new file next to this one,
`independent_review_wp9_1.md`, in the same form as the WP8 reviews: each look below against each of the seven
checklist items (at the end of this packet), **accept** or **reject** with a concrete reason (which picture, which
frame, what you see), and say whether any shipped effect or WP8 look regressed. This is round 1 of at most five
(Q0080). Nothing was spent and no texture was made: every look is built from the existing texture sets.

### What the looks are meant to be (StarCraft II analogues; looks stay original)

| Look | Shown when | Meant to read like |
|---|---|---|
| `stim_surge` | the overclock (stim) runs: Marine and Marauder analogues `pulse_bot`, `shock_golem` | the body running hot: a deep red rim beating at 0.6 s over a warmed picture; one flare and a breath of red vapour as the stim goes in; thin warm vapour off its crown every 0.45 s, left behind as it runs (`stim_fumes`) |
| `concussive_daze` | a unit is slowed (the Marauder analogue's concussive shells) | dulled and heavy: the picture cooled and greyed, glass highlight dimmed, a dull cold rim; one pale flare as the blow lands; a little dust at its feet while it drags itself on (`daze_drag`) |
| `charge_streak` | the attack dash (charge) runs: Zealot analogue `psi_grey` | a lunge that tears up the ground: a cold violet-white rim on the body, and warm dust and dark scuffs left behind every 0.08 s (`charge_dust`). Deliberately no streaks, rays or speed lines |
| `barrier_shell` and `barrier_absorb` | the barrier is raised and still has absorption left: Immortal analogue `void_goliath` | an energy membrane round the body, a little larger than its bubble: faint where it faces the camera, amber glass and a pale gold light towards its edge, a fine crackle open in its face; it thins as it absorbs and snaps up with one bright pass; every absorbed hit flares it and crackles away, with a short warm light |
| `vehicle_wreck` | death of `railgun_crawler` (Siege Tank) and `napalm_wheeler` (Hellion) | a vehicle destroyed: its glass bubble bursts into pale shards, a short hot flash and a ball of fire, sparks and wreckage thrown out, black smoke for two seconds, a charred scorch |
| `bio_burst` | death of `burrow_scorpion` (Lurker) | a creature burst open: glass shards, a dark ichor splash and globs on arcs, low foul vapour, a dark wet stain |

Every other unit still dies with the legacy glass shatter (the default death). A named death replaces it.

**Shown only in tests.** The only shipped unit with a barrier (`void_goliath`) has no unit media, so the client
does not present it in a battle yet. The pictures below give a presentable unit a copy of the shipped barrier in
the test world's catalog only.

### Material

The battle pictures come from the new `crates/client/tests/status_gpu_tests.rs` on the production skirmish (debug
build, Xvfb, 1280×720, the battle camera's default distance, which is gameplay zoom) on radv and lavapipe. The
sequences are 260×180 window crops round each subject, every third 40 ms frame (120 ms apart, labelled with their time since the subjects were staged), shown at 1.5 times their size; the
whole-window frames are at 1:1. Files start with `wp9r1_`. Where the subjects stand in the whole-window frames
(radv, window pixels): the stimmed unit at about (586, 383) left of the headquarters; the slowed unit at (551, 249);
the charging unit at (691, 225) and the enemy it charges at (695, 301); the shielded unit at (793, 220) and the
enemy firing on it at (800, 266); the three dying units at (892, 231), (899, 270) and (913, 303).

| What | Files |
|---|---|
| Stimmed unit running, from the moment it is staged | [radv](wp9r1_stim_sequence_radv.jpg), [lavapipe](wp9r1_stim_sequence_lavapipe.jpg) |
| Slowed unit walking | [radv](wp9r1_daze_sequence_radv.jpg), [lavapipe](wp9r1_daze_sequence_lavapipe.jpg) |
| A real charge (the dash unlocked, an attack order on an enemy 3.8 units away) | [radv](wp9r1_charge_sequence_radv.jpg), [lavapipe](wp9r1_charge_sequence_lavapipe.jpg) |
| Barrier raised in full when staged, then absorbing an enemy's fire (three absorbed hits, three flashes) | [radv](wp9r1_barrier_sequence_radv.jpg), [lavapipe](wp9r1_barrier_sequence_lavapipe.jpg) |
| Three deaths at t = 1.0 s (two vehicles, one creature) | [radv](wp9r1_deaths_sequence_radv.jpg), [lavapipe](wp9r1_deaths_sequence_lavapipe.jpg) |
| The whole battle at 0.4, 1.2, 2.0 and 2.8 s | `wp9r1_battle_0_radv.png` to `wp9r1_battle_3_radv.png`; lavapipe likewise |
| Barrier pose, default zoom: standing, barrier 0.5 left, the same without effect layers, 0.05 left, down | [radv](wp9r1_barrier_pose_radv.jpg), [lavapipe](wp9r1_barrier_pose_lavapipe.jpg) |
| Effect-lab contact sheets and sequences of the one-shot parts (the slice's layout: eight ages across, bright and dark ground, default and close zoom), radv and lavapipe | `wp9r1_lab/<driver>/<effect>_contact.jpg` and `_sequence.jpg` (legend `_contact.txt`) for `vehicle_wreck` (`UnitRadius` 0.6), `bio_burst` (0.5), `barrier_absorb` (0.5); and `<effect>_repeat_contact.jpg` and `_repeat_sequence.jpg` for what the statuses lay: `charge_dust` (0.5, six lays 0.08 s apart), `stim_fumes` (0.45, four lays 0.45 s apart), `daze_drag` (0.45, four lays 0.3 s apart). The legends list only splash and line bindings, so they show no `UnitRadius`; the values are given here. In the lab the effect stands still on its spot, where in a battle each lay is left behind a moving unit |
| The tests' printed measurements on both drivers | [wp9r1_measurements.txt](wp9r1_measurements.txt) |

"Without effect layers" clears the effect renderer's cards, decals, distortions and lights only; a unit's Shell
(its rim, tint and veil) still applies, as in the WP8 rounds.

### Engineering to check

- Status looks are held per unit in `client::effects::stance` (`keep_statuses`), only on a unit the viewer sees as
  it is; a barrier only while `barrier_ratio > 0` (`status_shown`).
- New vocabulary: `Shell.pulse`, a layer's `strength` bound to a value, a `Status` effect's `emit` and `hit`, the
  `Membrane` card shape; each with typed load errors (`effect_catalog_tests.rs`).
- A death named by a block is spawned from `BubbleShatter` and the legacy shatter is left out for that unit only
  (`effects::systems::replaces_shatter`, used by `vfx.rs`); the battle test counts legacy shards after the deaths.
- The legacy stim arc of `bubble_dome.wgsl` is off on effect-driven bubbles.
- No simulation crate changed; `units.ron` gains three `death` names (presentation data, not hashed).

### Points to look at

- Whether the stim's beat reads as a pulse and not a blinking light; whether the daze reads as "slowed" rather
  than "selected" or "cloaked".
- Whether the charge's trail reads as torn ground and dust, with nothing line-like.
- Whether the barrier membrane reads as a shell round the body at gameplay zoom without hiding the unit, and
  whether its outline reads as a drawn circle.
- Whether the deaths read as a vehicle and a creature dying, and whether the glass shards keep the bubble's
  identity.

## WP8 round 5 (concealment and stance: engineering only)

Round 4's review ([`independent_review_wp8_4.md`](independent_review_wp8_4.md), kept as written) accepted every
look and rejected the round narrowly for one engineering defect, E4-1. This round, plan Amendment 15, changes no
look. It is round 5, the last of the owner's five (Q0080). Please write it to a new file next to this one,
`independent_review_wp8_5.md`: check E4-1, A4-1 and A4-3, and confirm that no look and no shipped effect changed.
There are no new pictures. The figures are in [wp8r5_measurements.txt](wp8r5_measurements.txt).

### What changed

| Round-4 finding | Change |
|---|---|
| E4-1: the ordinary card runs were cut with the darkening prefix's real distances | `queue_effects` cuts the ordinary runs over the cards after the prefix only, and `cut_runs` offsets its ranges by the first card's index. The first run is queued at the first ordinary card's distance. New test `effects::render::tests::the_darkening_prefix_stays_out_of_the_ordinary_runs`: a prefix at −1.0 before cards at −10, −9 and −8, with other items at −9.5 and −8.5. No ordinary range contains the prefix, and the cuts equal those without it, offset by one. The test also shows the defect: cut with the prefix, the three cards fall into one run with it. |
| A4-1: a non-finite depth could sort before a darkening card | An ordinary card's sort depth is finite and below the darkening cards'; a non-finite depth sorts as 0. Unit test `no_card_sorts_before_a_darkening_card` covers NaN, infinities and `f32::MAX`. |
| A4-3: the back-of-furrow check had no negative control | The unit now stands just inside the furrow, 1.3 world units behind the hull and clear of it, so its body rises over the band on screen. On the furrow line, as in round 4, its body hardly overlapped the band. A first control run there passed even with the old order, so the check was insensitive. A one-off radv run with the darkening card item queued at its card's own distance (the order before E3-1) fails: 18 of 214 body pixels keep less than 0.85 of their light (lowest 0.332). With the order of record the check passes on both drivers: 0 of 214, lowest 0.891 and 0.892. The temporary queue was not kept. |
| P4-1, P4-2, A4-2, the burrow crest | Not taken. They are recorded in the plan's open polish list. |

`main` was already contained in the branch (`573a88c`), so the merge brought nothing new.

## WP8 round 4 (concealment and stance)

Round 3's review ([`independent_review_wp8_3.md`](independent_review_wp8_3.md), kept as written) accepted every look
but the settled `siege_anchor` (A3-1). It asked for the darkening cards' order (E3-1), with E3-2 and E3-3 advisory.
The orchestrator's decisions are plan Amendment 14. This is round 4 of at most five (Q0080). Please write it to a new
file next to this one, `independent_review_wp8_4.md`, in the same form:
- judge `siege_anchor` against each of the seven checklist items, and `mode_seal_sweep`'s softened band;
- check E3-1 to E3-3;
- say whether anything accepted earlier, or any shipped splash effect, regressed.

The pictures come from `crates/client/tests/stance_gpu_tests.rs` on the production skirmish, as before (debug build,
Xvfb, 1280×720, default camera distance), on radv and lavapipe. This round's files start with `wp8r4_`; earlier
rounds' files are unchanged. As before, "bare" keeps the unit's Shell and clears only the effect renderer's layers.
Nothing was spent and no texture was made.

### What changed

| Round-3 finding | Change |
|---|---|
| A3-1: the settled anchor reads as a hard-edged, evenly tinted round pad with a darker rim; the cracks do not show | The held ring and the cracks are gone. The anchor is soil pressed down under and round the hull, a soft disc fading out from about 0.8 to 1.6 radii, plus four darker compaction patches round it where the stabilisers press, which break the round boundary. Everything darkens only. A decal's disc outline gains `soft`: the mark fades out over that share of its half length inside its edge, as the square of the distance in from it, after the coverage is clamped, so thickening does not sharpen it. The pressed soil has no edge noise, because a trial with a wavering edge under the soft fade read as rays. |
| A3-1: the sweep band's crisp edges | A wavering band now fades over half its width at each edge, its mask applied after the intensity. The furrow erodes out over 0.6 s as the anchor comes in. |
| A3-1: a falloff figure | The test prints the anchored look's outer falloff: the width over which the ground keeps from 0.95 down to 0.75 of its light, as the median over near-horizontal rays from the unit's centre. It requires at least 0.15 of the unit's radius. The figures are in the measurements file. |
| E3-1: a darkening card queued at its run's distance | Darkening cards sort as the farthest of all, so they are the first cards of the sorted buffer. One darkening item at `f32::MIN` draws that prefix, before every other transparent item. In the pose shot of the switch half way, a unit of the viewer's stands on the back half of the furrow; the test requires its body to keep its light while the furrow darkens the ground round its foot. |
| E3-2 (advisory) | A headless test steps a unit through neutral, cloaked, neutral, burrowing, burrowed and emerging. On every frame it asserts that the bubble is unbatched exactly while its shell moves its shadow caster. |
| E3-3 (advisory) | Taken in the contained form: the single darkening item draws only the darkening cards, with no second item per card run, so no frame-budget run was taken for it. |
| Optional lit crest on the burrow heaps | Not done, so `burrow_mound` stays as accepted. |

### Material

| What | Files |
|---|---|
| The anchored unit, close: in the tableau, without effect layers | [wp8r4_anchored_crop_radv.png](wp8r4_anchored_crop_radv.png), [wp8r4_anchored_crop_lavapipe.png](wp8r4_anchored_crop_lavapipe.png) |
| Siege switch half way, `Critical` layers only, the tank's bubble hidden, a unit of the viewer's on the back of the furrow; and without effect layers | [wp8r4_siege_mid_radv.png](wp8r4_siege_mid_radv.png), [wp8r4_siege_mid_lavapipe.png](wp8r4_siege_mid_lavapipe.png) |
| Sequences at 40 ms steps: siege (the furrow closing, then the anchor coming in) and unsiege | [siege](wp8r4_siege_sequence_radv.png), [unsiege](wp8r4_unsiege_sequence_radv.png); lavapipe: the same names ending `_lavapipe` |
| The same sequences for dig and emerge, unchanged looks | [dig](wp8r4_dig_sequence_radv.png), [emerge](wp8r4_emerge_sequence_radv.png); lavapipe likewise |
| Battle: before the tableau; the tableau; without effect layers; detected; detected without effect layers | [before](wp8r4_battle_before_radv.png), [tableau](wp8r4_battle_undetected_radv.png), [bare](wp8r4_battle_bare_radv.png), [detected](wp8r4_battle_detected_radv.png), [detected, bare](wp8r4_battle_detected_bare_radv.png); lavapipe: the same names ending `_lavapipe` |
| Burrow poses, shimmer pose and the player's cloaked unit (for regressions) | `wp8r4_burrow_pose_*`, `wp8r4_plain_burrow_pose_*`, `wp8r4_shimmer_pose_*`, `wp8r4_own_cloaked_crop_*` |
| The tests' printed measurements on both drivers | [wp8r4_measurements.txt](wp8r4_measurements.txt) |

The anchored Siege-Tank analogue stands at about (548, 244) in the battle captures, at the upper left corner of the
platform. Its hull's shadow falls to its lower right.

### Points to look at

- Whether the settled anchor reads as soil pressed down by the hull and its stabilisers, with no outline, ring, rim,
  aura or range language, and whether its edge is soft and its boundary broken.
- Whether the sweep's band still reads as ploughed earth with its softened edges.
- E3-1: the unit on the back of the furrow in `wp8r4_siege_mid_*`.

## WP8 round 3 (concealment and stance)

Round 2's review ([`independent_review_wp8_2.md`](independent_review_wp8_2.md), kept as written) accepted the fog
rule, the shimmer check, `cloak_shimmer`, `detected_mark`, `burrow_dig` and `burrow_emerge`, and found no regression
in the shipped effects. It rejected `cloak_own` (a regression), `burrow_mound`, `mode_seal_sweep` and `siege_anchor`,
and asked for S-1, S-2, B2-1 to B2-3 and F2-1 to F2-3. The orchestrator's decisions are plan Amendment 13. This is
round 3 of at most five (Q0080). Please write it to a new file next to this one, `independent_review_wp8_3.md`, in
the same form: judge `cloak_own`, `burrow_mound`, `mode_seal_sweep` and `siege_anchor` against each of the seven
checklist items, check S-1 and S-2, and say whether anything accepted in rounds 1 and 2 or any shipped splash effect
regressed (the effect renderer gained a darkening pass that the shipped effects do not use).

The pictures come from `crates/client/tests/stance_gpu_tests.rs` on the production skirmish, as before (debug build,
Xvfb, 1280×720, default camera distance), on radv and lavapipe. This round's files start with `wp8r3_`; the
earlier rounds' files are unchanged. Nothing was spent and no texture was made.

**About "bare" (F2-3).** Every "without effect layers" or "bare" picture in this packet, as in rounds 1 and 2,
clears the effect renderer's sprites, decals, distortions and lights only. The unit's Shell (its sink, veil, rim,
matte and shadow lane) still applies. A bare-against-full difference therefore shows what the ground and air layers
add, not the unit without any look.

### What changed

| Round-2 finding | Change |
|---|---|
| S-1: the fading shadow detaches; `cloak_own` leaves a small dark dot beside the unit | The shadow caster shrinks about the sphere's lowest point, where it stands, so the shadow contracts to the foot. `cloak_own` keeps a smaller shadow at its foot (`shadow: 0.6`). |
| S-2: unbatched shadows for all 400 units | `NoAutomaticBatching` only on a bubble whose shell moves its shadow caster (shadow lane or sink above 0), inserted on the frame that starts and removed on the frame the shell is neutral again. The pose test checks that a neutral unit among the batched ones casts the same shadow while the burrowed unit's is gone. A 400-unit frame-budget comparison with `main` is in [`2026-11-12-frame-budget-wp8-shadows.md`](../../2026-11-12-frame-budget-wp8-shadows.md). |
| B2-1: the burrowed ground hidden under the near-full body | Twelve heaps of earth on a circle of 1.2 radii (a new `Circle` placement; round 2's `Disc` scattered most of them under the body), and a hollow out to 1.4 radii whose lip shows outside the outline. The test asks the looks to change at least half of the ground between 1.05 and 1.6 radii that shows beside the body. |
| B2-2: the furrow's head a hard cut; the furrow and soil lit over the shadow | A darkening mode: the furrow, hollow, heaps, pressed soil and cracks multiply the ground by their colour, so they never lighten it, shadows included. The sweep's head is a soft end 18 degrees wide (its fade showing over about 12), freshly turned earth lit behind it. |
| B2-3: a straight edge across the tank's shadow; a pale smear | The edge was the shadow's own straight flank, made visible where round 2's lighter soil lightened the shadow but not the floor beside it. Multiplied soil leaves no such step. The pressed soil is a darker, warm compaction, and the anchor's dust is shorter. The test asks the anchored unit's looks to lighten at most 0.5 % of the pixels within 2.2 radii. |
| F2-1, F2-2 | The unit without mode media is pictured burrowed with its looks. Every measurement line is in the measurements file. |
| `shimmer_check` comment | It now states what it accepts (the box of one 2×2 block; channels mixed within a block pass, across blocks fail). |

The battle tableau is now taken 5 s after it is staged, so the anchoring switch and its dust have ended.

### Material

| What | Files |
|---|---|
| Burrow pose, unit with its clip: empty ground, standing, burrowed, burrowed without effect layers | [wp8r3_burrow_pose_radv.png](wp8r3_burrow_pose_radv.png), [wp8r3_burrow_pose_lavapipe.png](wp8r3_burrow_pose_lavapipe.png) |
| Burrow pose, unit without mode media: empty, standing, burrowed with its looks, burrowed without effect layers | [wp8r3_plain_burrow_pose_radv.png](wp8r3_plain_burrow_pose_radv.png), [wp8r3_plain_burrow_pose_lavapipe.png](wp8r3_plain_burrow_pose_lavapipe.png) |
| Undetected cloaked enemy, pose: empty, shimmer, without effect layers, amplified difference | [wp8r3_shimmer_pose_radv.png](wp8r3_shimmer_pose_radv.png), [wp8r3_shimmer_pose_lavapipe.png](wp8r3_shimmer_pose_lavapipe.png) |
| Siege switch half way, `Critical` layers only, bubble hidden; and without effect layers | [wp8r3_siege_mid_radv.png](wp8r3_siege_mid_radv.png), [wp8r3_siege_mid_lavapipe.png](wp8r3_siege_mid_lavapipe.png) |
| Sequences at 40 ms steps, labelled `t` since the switch was asked for: dig, emerge, siege, unsiege | [dig](wp8r3_dig_sequence_radv.png), [emerge](wp8r3_emerge_sequence_radv.png), [siege](wp8r3_siege_sequence_radv.png), [unsiege](wp8r3_unsiege_sequence_radv.png); lavapipe: the same names ending `_lavapipe` |
| The player's cloaked unit, close: before the tableau (not there yet), in the tableau, without effect layers | [wp8r3_own_cloaked_crop_radv.png](wp8r3_own_cloaked_crop_radv.png), [wp8r3_own_cloaked_crop_lavapipe.png](wp8r3_own_cloaked_crop_lavapipe.png) |
| The anchored unit, close: in the tableau, without effect layers | [wp8r3_anchored_crop_radv.png](wp8r3_anchored_crop_radv.png), [wp8r3_anchored_crop_lavapipe.png](wp8r3_anchored_crop_lavapipe.png) |
| Battle: before the tableau; the tableau; without effect layers; a detector beside the cloaked enemy; that scene without effect layers | [before](wp8r3_battle_before_radv.png), [tableau](wp8r3_battle_undetected_radv.png), [bare](wp8r3_battle_bare_radv.png), [detected](wp8r3_battle_detected_radv.png), [detected, bare](wp8r3_battle_detected_bare_radv.png); lavapipe: the same names ending `_lavapipe` |
| The tests' printed measurements on both drivers | [wp8r3_measurements.txt](wp8r3_measurements.txt) |

Where things stand in the battle captures (window pixels, radv): the anchored Siege-Tank analogue at about
(548, 244), the player's cloaked unit at about (691, 212), the undetected cloaked enemy at about (893, 230), the
undetected burrowed enemy at about (796, 211), and the player's burrowed unit at about (580, 385), left of the
headquarters.

### Points to look at

- Whether the owner's burrowed unit now reads as set into the floor (`wp8r3_burrow_pose_*` cell 3, and the battle),
  and whether the heaps read as earth.
- Whether the furrow reads as ploughed ground during the switch (its head) and anchored, darker inside the tank's
  shadow and never lighter.
- Whether `cloak_own`'s shadow is attached and faint.
- S-2: the frame-budget comparison.

## WP8 round 2 (concealment and stance)

Round 1's review ([`independent_review_wp8_1.md`](independent_review_wp8_1.md), kept as written) accepted
`cloak_shimmer`, `cloak_own`, `detected_mark` (with check C-1), the fog rule and the distortion fix. It rejected
`burrow_mound`, `burrow_dig`, `burrow_emerge`, `mode_seal_sweep` and `siege_anchor`. The orchestrator's decisions
are plan Amendment 12. This is round 2 of at most five (Q0080). Please write it to a new file next to this one,
`independent_review_wp8_2.md`, in the same form as round 1: judge the five rejected looks against each of the seven
checklist items, check C-1 and F-1, and say whether anything accepted in round 1 or any shipped splash effect
regressed.

The pictures come from `crates/client/tests/stance_gpu_tests.rs` on the production skirmish, as in round 1 (debug
build, Xvfb, 1280×720, default camera distance), on radv and on lavapipe. Round 1's files are unchanged; this
round's files start with `wp8r2_`. Nothing was spent and no texture was made.

### What changed

| Round-1 finding | Change |
|---|---|
| B-1: the owner's burrowed unit reads as a marble; only the middle strip of its burrowed clip shows | A unit whose burrowed mode has its own clip sinks 0.12 of its diameter (`clip_sink`), not 0.92, and its glass highlight is off while burrowed (`matte`). The "dug in" read comes from the ground: the shadow fades out, a dark hollow, heaped earth, the soil band. Only a unit without mode media still sinks deep. |
| B-2: the shadow pops off at the start of the dig and back on at the emerge | The shadow fades with the sink and the veil: the bubble's shadow caster shrinks into the body (`bubble_shadow.wgsl`, the shell's `shadow` lane). Bevy drew every bubble's shadow with one bubble's material (bubbles share a mesh), so bubbles are now drawn unbatched and each casts with its own. |
| B-3, B-4: grey haze, a hard straight edge, clods and dust not visible | The earth is warm brown, darker and more opaque. Its marks have a noise-edged outline with no box edge, and they lie inside 1.8 radii. Clods are larger and darker, and the dust is warmer. |
| B-5, B-6: the furrow reads as a drawn ring over the shadow; no lit head | The furrow's width and radius waver round the turn (`waver`), its breakup leaves a broken circle, its berm is opaque earth lighter on top, its head is lit, and it is drawn under the unit's shadow. The stabilisers' pressed soil and cracks lie out from under the bubble (1.35 to 1.5 radii). |
| F-1: no undetected burrowed enemy in the battle | The tableau has one (the stand-in, burrowed, at about (796, 211) in the radv window). The test compares its patch with the scene captured just before the tableau was staged. |
| F-2: sequences without a fixed interval or times | Every frame of an act advances presentation time by exactly 40 ms, and every frame is captured, labelled with its time since the switch was asked for. |
| C-1: the detected enemy looks milky | The detected tableau is also captured without effect layers (`wp8r2_battle_detected_bare_*`). |
| Regression check per channel | `shimmer_check` now judges whole colours: a bent pixel must lie within 6 levels (Euclidean RGB) of what one filtered read of a 2×2 block of the reference within 6 px can give. |
| Distortion fix | Registered in `DESIGN.md` as B-119 (`flame_line`'s and `lance_sweep`'s heat haze sampled the wrong screen region in every battle until the fix). |

A fix found while building the evidence: an emerging unit with a clip dropped for one frame to the deep sink of a
unit without a clip when its channel ended, because the unit then has no mode to ask about. The track now keeps
the decision while the emerge settles (`kept_clip`), and the sequence test checks that the body never sinks during
the emerge.

### Material

| What | Files |
|---|---|
| Burrow pose, unit with its clip: empty ground, standing, burrowed, burrowed without effect layers | [wp8r2_burrow_pose_radv.png](wp8r2_burrow_pose_radv.png), [wp8r2_burrow_pose_lavapipe.png](wp8r2_burrow_pose_lavapipe.png) |
| Burrow pose, unit without mode media (a small shipped archetype given the burrowed mode in the test world's catalog only): empty, standing, burrowed without effect layers | [wp8r2_plain_burrow_pose_radv.png](wp8r2_plain_burrow_pose_radv.png), [wp8r2_plain_burrow_pose_lavapipe.png](wp8r2_plain_burrow_pose_lavapipe.png) |
| Undetected cloaked enemy, pose: empty, shimmer, without effect layers, amplified difference | [wp8r2_shimmer_pose_radv.png](wp8r2_shimmer_pose_radv.png), [wp8r2_shimmer_pose_lavapipe.png](wp8r2_shimmer_pose_lavapipe.png) |
| Siege switch half way, `Critical` layers only, bubble hidden; and without effect layers | [wp8r2_siege_mid_radv.png](wp8r2_siege_mid_radv.png), [wp8r2_siege_mid_lavapipe.png](wp8r2_siege_mid_lavapipe.png) |
| Sequences at 40 ms steps, labelled `t` since the switch was asked for (cells cut 160×120 round the unit, enlarged 2×): dig 2.4 s, emerge 1.4 s, siege 3.6 s, unsiege 3.2 s | [dig](wp8r2_dig_sequence_radv.png), [emerge](wp8r2_emerge_sequence_radv.png), [siege](wp8r2_siege_sequence_radv.png), [unsiege](wp8r2_unsiege_sequence_radv.png); lavapipe: the same names ending `_lavapipe` |
| Battle: the scene before the tableau; the tableau; without effect layers; a detector of the player's beside the cloaked enemy; that scene without effect layers | [before](wp8r2_battle_before_radv.png), [tableau](wp8r2_battle_undetected_radv.png), [bare](wp8r2_battle_bare_radv.png), [detected](wp8r2_battle_detected_radv.png), [detected, bare](wp8r2_battle_detected_bare_radv.png); lavapipe: the same names ending `_lavapipe` |
| The tests' printed measurements on both drivers | [wp8r2_measurements.txt](wp8r2_measurements.txt) |

The tableau's units: the player's burrowed Lurker analogue, the player's cloaked stand-in, the enemy cloaked
stand-in (undetected, about (893, 230) in the radv window), the undetected burrowed enemy stand-in (about
(796, 211)) and the anchored Siege-Tank analogue. All stand on the upper part of the skirmish platform, which ends
in open space close to the base, so some looks lie near its edge.

### Points to look at

- Whether the owner's burrowed unit now reads as its own burrowed clip, dug in (no marble, no highlight, no
  shadow), and whether the ground round it reads as earth at the default zoom.
- Whether the dig and the emerge, at 40 ms steps, show the shadow fading rather than popping, the clods and the
  dust, and the heaped earth staying after the emerge.
- Whether the furrow reads as ploughed ground during the siege and unsiege and while anchored, rather than as a
  drawn ring, and whether its head is lit.
- C-1: the detected enemy with and without its looks.
- F-1: the undetected burrowed enemy's place in the tableau against the scene before it.

## WP8 round 1 (concealment and stance)

This is the first review round of WP8 (plan Amendment 11; Q0080 allows up to five). Please write it to a new file
next to this one, `independent_review_wp8_1.md`, in the same form as earlier rounds: judge each look below against
each of the seven checklist items (item 7, fog, carries most weight here), write **accept** or **reject** with a
concrete reason (file, cell or frame, what you see), and say whether the shipped seven splash effects regressed
(they were not changed; the effect shaders gained an arc mask and a lens bend that are off for them).

Unlike the earlier rounds, these looks are not drawn in the effect lab but on units in the production client
playing the shipped skirmish, because they modify the units' own bubbles. The pictures are screenshots of
`crates/client/tests/stance_gpu_tests.rs`'s two children (debug build, Xvfb, 1280×720 window), on radv (the
physical GPU) and on lavapipe, at the camera's default distance. The burrowing unit is the shipped Lurker
analogue (a scorpion picture, with WP13's burrowed and morph clips), the anchoring unit the shipped Siege-Tank
analogue (with WP13's sieged clip); the cloaked units are an unarmed stand-in `battle_harness::burrower` picks.
Nothing was spent and no texture was made.

### The looks (what each is meant to be)

| Look | Shown to | Meant to read as |
|---|---|---|
| `burrow_dig` | anyone who sees the unit | the bubble sinks into the ground over the dig (1.8 s; the stats change at 33 ticks, the dig goes on to 45, D-6), soil creeps up its glass from the ground, clods are thrown off its rim, dust rises, loose earth heaps round the hole |
| `burrow_mound` | the owner and an enemy that detects it | a low, soil-caked dome in a hollow with heaped earth round it; nothing else |
| `burrow_emerge` | anyone who sees the unit | the body heaves up out of its hole in 11 ticks, throwing soil and dust; the heaped earth stays a while |
| `cloak_own` | the owner of a cloaked unit | a hollow, cool glass shape with an outline light and a faint bend of what is behind it: "hidden, but here" |
| `cloak_shimmer` | an enemy that does not detect it | only a faint refraction of what lies behind its outline, slowly crawling; no colour, no bubble, no ring or bar (Q0076) |
| `detected_mark` | an enemy that detects a cloaked unit | the unit drawn as it is, with a thin cold outline light and a slight bend at its edge |
| `mode_seal_sweep` | anyone who sees the unit | while an anchoring mode switch runs (siege and unsiege), a furrow ploughed round the hull whose arc closes with the switch's progress, its head lit; dust as it begins |
| `siege_anchor` | anyone who sees the unit | while anchored, the closed furrow stays, with soil pressed down under the stabilisers |

An undetected **burrowed** enemy shows nothing at all (WP3's rule, unchanged), and its spine line still shows where
the viewer sees the ground (R6, unchanged).

### Material

| What | Files |
|---|---|
| Burrow pose: empty ground, the unit standing, burrowed, burrowed without the effect layers | [wp8_burrow_pose_radv.png](wp8_burrow_pose_radv.png), [wp8_burrow_pose_lavapipe.png](wp8_burrow_pose_lavapipe.png) |
| Undetected cloaked enemy: empty ground, its shimmer, without the effect layers, the shimmer's amplified difference from the empty ground | [wp8_shimmer_pose_radv.png](wp8_shimmer_pose_radv.png), [wp8_shimmer_pose_lavapipe.png](wp8_shimmer_pose_lavapipe.png) |
| Siege switch caught half way, only `Critical` layers, the unit's bubble hidden so the arc can be measured; and the same without effect layers | [wp8_siege_mid_radv.png](wp8_siege_mid_radv.png), [wp8_siege_mid_lavapipe.png](wp8_siege_mid_lavapipe.png) |
| Sequences through real mode commands, one picture whenever the previous one was written (about every 0.15 to 0.5 s; frame numbers are picture indices) | dig [wp8_dig_sequence_radv.png](wp8_dig_sequence_radv.png), emerge [wp8_emerge_sequence_radv.png](wp8_emerge_sequence_radv.png), siege [wp8_siege_sequence_radv.png](wp8_siege_sequence_radv.png), unsiege [wp8_unsiege_sequence_radv.png](wp8_unsiege_sequence_radv.png); lavapipe: the same names ending `_lavapipe` |
| Battle capture: a burrowed unit of the player's, a cloaked unit of the player's, a cloaked enemy undetected, the Siege-Tank analogue anchored; then the same scene without effect layers; then a detector of the player's brought beside the enemy | [wp8_battle_undetected_radv.png](wp8_battle_undetected_radv.png), [wp8_battle_bare_radv.png](wp8_battle_bare_radv.png), [wp8_battle_detected_radv.png](wp8_battle_detected_radv.png); lavapipe: the same names ending `_lavapipe` |
| Own units, close: the player's unit standing and burrowed (pose shots); the player's cloaked unit with and without its effect layers (battle capture) | [wp8_own_burrowed_crop_radv.png](wp8_own_burrowed_crop_radv.png), [wp8_own_burrowed_crop_lavapipe.png](wp8_own_burrowed_crop_lavapipe.png), [wp8_own_cloaked_crop_radv.png](wp8_own_cloaked_crop_radv.png) |
| The tests' printed measurements on both drivers | [wp8_measurements.txt](wp8_measurements.txt) |

Where things stand in the battle capture (window pixels): the anchored Siege-Tank analogue upper left of the
headquarters dome, the player's burrowed unit left of the dome, the player's cloaked unit above the dome, the
enemy cloaked unit right of it (to the upper right of the row of workers).

### Points to look at

- Whether the burrowed unit reads as dug in rather than as a small disc on the ground, at the default zoom, and
  whether its owner still reads the unit clearly (Q0076: own concealed units stay readable). The burrowed dome
  shows the unit's own picture unveiled, with only a thin soil band at the ground line and an earth-coloured
  outline. Since WP13 merged, the burrowing unit in these captures is the shipped Lurker analogue, and its dome
  shows its own burrowed-mode clip (the Siege-Tank analogue likewise plays its sieged clip). The dome is sunk to
  at most 35 % of the standing bubble's area, as WP8's test requires, so only the clip's middle shows; whether
  that is readable enough for its owner is a question for this review. The player's own cloaked unit keeps its
  picture at about half opacity under a cool outline.
- Whether the shimmer is faint and colourless (item 7): compare `wp8_shimmer_pose_*` cells 1 and 2, and the battle
  capture's enemy before and after detection.
- Before hand-off, the implementer's battle capture showed a small shadow-coloured spot at the undetected
  enemy's feet. Cause: the distortion block read the scene copy at viewport-relative coordinates, while the copy
  covers the whole window and the battle camera's viewport excludes the HUD panels, so every distortion in a
  battle copied the scene from elsewhere on the screen (here a shadow). It now reads the copy where the fragment
  is (`distortion.wgsl`; the effect lab's full-window viewport is unaffected). The battle test now compares the
  capture with and without effect layers around that enemy: before the fix 133 of 2 074 bent pixels took a
  colour not near them and the mean colour moved 4.10 levels; after it, none, and 0.18 levels. Please check the
  battle captures for any remaining trace (item 7).
- Whether the siege furrow reads during the switch and while anchored, and whether it reads as ploughed ground or
  as a drawn ring.

## Scale-out round 5 (WP7)

Round 4's review ([`independent_review_scaleout4.md`](independent_review_scaleout4.md), kept as written) accepted
everything except `cleave_impact`, whose front had become round again. The orchestrator adopted that review's
construction for the front (plan Amendment 8). This is round 5, the last of the owner's five review rounds
(Q0080). Please write it to a new file next to this one, `independent_review_scaleout5.md`, in the same form:

- judge `cleave_impact` against each of the seven checklist items;
- confirm that nothing else regressed.

The images are lavapipe captures from a debug build under Xvfb, as before. Nothing was spent and no texture was made.

### Material

| What | Files |
|---|---|
| `cleave_impact` sheet and sequence | [cleave_impact_contact.png](cleave_impact_contact.png) ([legend](cleave_impact_contact.txt)), [cleave_impact_sequence.png](cleave_impact_sequence.png) |
| four hits on one spot, 0.6 s apart | [cleave_impact_repeat_contact.png](cleave_impact_repeat_contact.png) ([legend](cleave_impact_repeat_contact.txt)), [cleave_impact_repeat_sequence.png](cleave_impact_repeat_sequence.png) |
| per-pixel steps between consecutive frames, rounds 2 to 5 | [cleave_impact_pops.txt](cleave_impact_pops.txt) |

### What changed

| Round-4 finding | Change |
|---|---|
| A ring again: the edge about 40 % smoother, no gaps, the bite halved | Round 3's bite is restored and slightly deepened (the declared bite is 0.198 of the radius, under the 0.2 cap), with no crest widening. The second noise octave keeps its spread. Between twelve guaranteed crests, the noise bites the edge freely. |
| The per-sector clause met by tuning one seed | Each crest keeps the edge on the radius over 5° (plus a 1.5° soft edge). The crests are 30° apart, with a per-hit rotation and a ±7° jitter drawn from the ring card's own seed. Their largest gap is at most 44°, so every 45° sector holds a crest for every hit. Unit tests check this for 64 hits. |
| The same outline on every hit | The crests turn and jitter per hit, and the noise between them has its own phase per hit. A unit test over 64 hits requires the crests of consecutive hits to differ. The implementer reports the outlines of the four hits in the capture. |
| Wording: "no Critical decal dropped" | Corrected in the round-4 section: the 444 effects refused room drew none of their marks, Critical ones included. |

The implementer's measurements (edge spread, mean bite, gaps, outline correlation between hits, pops) are in
`slice_review.md`, "Scale-out round 5". As in earlier rounds, please form your verdicts before reading them.

Not changed this round: the front's dark value on teal, the square clods and the crack (optional notes), and a
screen-size floor for the flame's burning ground at the far camera (optional; recorded in Amendment 8).

## Scale-out round 4 (WP7)

Round 3's review ([`independent_review_scaleout3.md`](independent_review_scaleout3.md), kept as written) accepted
`cleave_impact`, the 6 144 decal pool and the degradation frames. It asked for a stronger footprint clause, for
stable behaviour at the decal cap, and for jets that burn in a real battle. The orchestrator decided those changes
(plan Amendment 7). This is round 4, the fourth of the owner's five. Please write it to a new file next to this
one, `independent_review_scaleout4.md`, in the same form:

- judge `cleave_impact` again (its front changed; see below) and `flame_line` in the crowd;
- check that the other effects did not regress;
- judge the crowd (item 6) on the new takes, including the behaviour at the decal cap and the jets;
- say whether the footprint rule is now acceptable.

The images are lavapipe captures from a debug build under Xvfb, as before. Nothing was spent and no texture was made.

### Material

| What | Files |
|---|---|
| `cleave_impact` | [cleave_impact_contact.png](cleave_impact_contact.png) ([legend](cleave_impact_contact.txt)), [cleave_impact_sequence.png](cleave_impact_sequence.png), repeat take [cleave_impact_repeat_contact.png](cleave_impact_repeat_contact.png), [cleave_impact_repeat_sequence.png](cleave_impact_repeat_sequence.png), per-pixel steps [cleave_impact_pops.txt](cleave_impact_pops.txt) (round 4 row) |
| crowd frame, tick 150, 200 splashes a second over all seven effects | [stress_capture_400.png](stress_capture_400.png), [stress_capture_400_detail.png](stress_capture_400_detail.png) |
| eight consecutive frames, 40 ms apart | [stress_degradation_400.png](stress_degradation_400.png) |
| the same with the decal pool forced to 2 048 | [stress_decal_cap_400.png](stress_decal_cap_400.png) |
| the battle without the stream, jets, ticks 200–209, at the battle camera's default distance | [stress_jets_400.png](stress_jets_400.png) |
| the same at the frame-budget camera (75 units away) | [stress_jets_far_400.png](stress_jets_far_400.png), [stress_jets_far_400_detail.png](stress_jets_far_400_detail.png) |
| figures | [stress_capture.txt](stress_capture.txt), paragraph "Scale-out round 4" |

### What changed

| Point | Change |
|---|---|
| Footprint clause (b) | A torn front's farthest ray must now be within 2 px of the reach in each of eight 45° sectors, not once somewhere. Its declared bite is capped at 20 % of the reach. A test shows that a front squashed on one side, or missing a quarter, fails. |
| `cleave_impact` front | Round 3's front failed the per-sector clause (one sector ended 3.6 px short). Its torn edge now reads two noise octaves (the round-3 note on regular bump spacing), and its clumps' crests are broader. It passes: 97 % of rays, mean −2.66 px, nothing beyond the reach. |
| Decals at the cap | Each effect reserves its decal slots at birth and draws them whole or not at all. Above 85 % of the pool, the oldest lasting marks erode out over 0.25 s to make room. With the pool forced to 2 048, no decal an effect was granted is ever dropped from a frame (round 3: about 2 050 Critical decals a frame). *Corrected in round 5:* 444 new effects were refused room and drew none of their marks, Critical ones included (whole set or none). |
| Jets | A class cap now counts an effect only while its fire is alive. Over the cap, the oldest burning effect gives its detail up, never the newborn. `flame_line`'s body of fire and its burning ground are always kept; their look in the lab is unchanged. |

**Points to look at.**

- At the normal stress load, 853 lasting marks erode out early over the run. Reservations also count decals too small
  to draw at the frame-budget camera, so the 85 % mark is reached while about 3 900 decals are drawn. No effect is
  refused room at that load.
- At the far camera, the flame's billows are below the 6-pixel floor. What remains is its body of fire and its
  burning ground.
- The cleave front's value on teal was not changed: a lighter front loses its contrast on the bright ground, which
  the footprint needs.

## Scale-out round 3 (WP7)

Round 2's review ([`independent_review_scaleout2.md`](independent_review_scaleout2.md), kept as written) accepted
`flame_line` and `lance_sweep`, found `shell_impact` restored, and rejected `cleave_impact`. This is round 3. Please
write it to a new file next to this one, `independent_review_scaleout3.md`, in the same form:

- judge `cleave_impact` afresh against each of the seven checklist items (at the end of this packet), **accept** or
  **reject** with a concrete reason (sheet, cell or frame, what you see);
- check that the other six did not regress (their sheets are unchanged since round 2; their GPU goldens pass
  unchanged);
- judge the crowd (item 6) on the new frames;
- give your view on the footprint trade-off below.

The images are lavapipe captures from a debug `battle_capture --effect-lab` under Xvfb, as before, with the same
sheet layout. Nothing was spent and no texture was made.

### Material

| What | Files |
|---|---|
| `cleave_impact` sheet and sequence | [cleave_impact_contact.png](cleave_impact_contact.png) ([legend](cleave_impact_contact.txt)), [cleave_impact_sequence.png](cleave_impact_sequence.png) |
| four hits on one spot, 0.6 s apart | [cleave_impact_repeat_contact.png](cleave_impact_repeat_contact.png) ([legend](cleave_impact_repeat_contact.txt)), [cleave_impact_repeat_sequence.png](cleave_impact_repeat_sequence.png) |
| per-pixel change between consecutive frames, round 2 and round 3 | [cleave_impact_pops.txt](cleave_impact_pops.txt) |
| crowd frame (tick 150) and its middle enlarged | [stress_capture_400.png](stress_capture_400.png), [stress_capture_400_detail.png](stress_capture_400_detail.png) |
| eight consecutive crowd frames, 40 ms apart, across budget retirements | [stress_degradation_400.png](stress_degradation_400.png) |
| the same eight frames with the decal pool forced to half the old size (what happens at the cap) | [stress_decal_cap_400.png](stress_decal_cap_400.png) |
| the battle without the stream, at the battle camera's default distance, ten consecutive frames while jets burn | [stress_jets_400.png](stress_jets_400.png) |
| figures for all crowd frames | [stress_capture.txt](stress_capture.txt), paragraph "Scale-out round 3" |

### What changed

| Round-2 finding | What changed |
|---|---|
| Glass bubble: a translucent disc with a smooth, unbroken elliptical rim | The front is a thinner band, torn inside from its first frame, with no tinted disc inside. Its leading edge is bitten back where its own noise is low, so its outline is a run of dust clumps. |
| The same circle on every hit | The bite follows each hit's own noise phase, so each hit has its own outline (repeat take). |
| Pop at frame 7→8 (370 px changing by more than 3 %, against 46–78 px before) | The front's intensity now falls linearly over the last 60 % of its life while it erodes. Measured per pixel on full-resolution frames: round 2 had 719 px at 6→7 and 2 638 px at 7→8; round 3 has 696–1 034 px for every step from frame 4 on ([cleave_impact_pops.txt](cleave_impact_pops.txt)). |
| Crack not readable; square clods | Not changed. |

**The footprint trade-off.** The slice test's Critical contract asked that 85 % of rays end within ±2 px of the
reach, which is 7 % of the cleave's radius at the default zoom. A torn edge deeper than that cannot pass it: this
front scores 0 % under that rule. The only fronts that passed it were nearly round outlines. The test now lets a
front that declares a torn edge end inside the reach by its declared bite (15 % of the reach here). Two rules still
hold: nothing may end beyond the reach, and the farthest clumps must lie within 2 px of the reach. This front
scores 89 %, with its farthest ray at −1.68 px. The plan (Amendment 6) records this as a proposal that can be
reversed. Is the reach still readable from the outermost dust?

**Crowd notes.**

- The crowd frames now step one simulation tick per frame, so consecutive frames are 40 ms apart. The decal load at
  that step is 4 206–4 296 a frame, above the old pool of 4 096, so the pool is now 6 144.
- The forced-cap take shows the failure mode the headroom avoids: lasting marks flicker between consecutive frames.
- The jets take is at the battle camera's default distance, without the stream, ticks 200 to 209. It shows what the
  budget does to the jets in a real battle: more than 96 `Major` effects are alive (a flame line lives 3.5 s for
  its band), so a new jet's fire layers are retired at birth. The jet shows as its core, the tracer and its band.
  The plan (Amendment 6) records options; nothing was changed. At the frame-budget camera (75 units away) the fire's
  cards would also fall below the 6-pixel level-of-detail floor.
- Fading acid stains after their first second in crowds is recorded as a proposal (Amendment 6), not taken: it would
  change `acid_burst`'s accepted sheets.

## Scale-out round 2 (WP7)

Scale-out round 1's review ([`independent_review_scaleout1.md`](independent_review_scaleout1.md), kept as written)
accepted `flak_burst`, rejected `flame_line`, `lance_sweep` and `cleave_impact`, and found that `shell_impact` had
regressed. This is round 2. Please write it to a new file next to this one, `independent_review_scaleout2.md`, in the
same form: judge `flame_line`, `lance_sweep` and `cleave_impact` afresh against each of the seven checklist items
(at the end of this packet), **accept** or **reject** with a concrete reason (sheet, cell or frame, what you see);
check `shell_impact` against its round-5 pictures; judge the crowd (item 6) for all seven effects and the fog
sheet of `flame_line`; and say whether the seven are consistent with each other. The images are lavapipe captures
from a debug `battle_capture --effect-lab` under Xvfb, as before; the sheets' layout is unchanged (eight ages across;
rows: bright ground default zoom, the same with the 1-unit grid and the bound extent in red, bright close zoom and
its reference, then the same four rows on dark teal ground). The meaning of each effect and its simulated extent
are in the table of "Scale-out round 1" below. Nothing was spent and no texture was made this round: the new looks
are built from the existing texture sets.

### Material

| Effect | Contact sheet | Sequence (40 ms steps) | Extra |
|---|---|---|---|
| `flame_line` | [flame_line_contact.png](flame_line_contact.png) ([legend](flame_line_contact.txt)) | [flame_line_sequence.png](flame_line_sequence.png) | fog take: [flame_line_fog_contact.png](flame_line_fog_contact.png) ([legend](flame_line_fog_contact.txt)), [flame_line_fog_sequence.png](flame_line_fog_sequence.png) |
| `lance_sweep` | [lance_sweep_contact.png](lance_sweep_contact.png) ([legend](lance_sweep_contact.txt)) | [lance_sweep_sequence.png](lance_sweep_sequence.png) | |
| `cleave_impact` | [cleave_impact_contact.png](cleave_impact_contact.png) ([legend](cleave_impact_contact.txt)) | [cleave_impact_sequence.png](cleave_impact_sequence.png) | four consecutive hits on one spot, 0.6 s apart (the attacker's cadence): [cleave_impact_repeat_contact.png](cleave_impact_repeat_contact.png) ([legend](cleave_impact_repeat_contact.txt)), [cleave_impact_repeat_sequence.png](cleave_impact_repeat_sequence.png) |
| `shell_impact` | [shell_impact_contact.png](shell_impact_contact.png) ([legend](shell_impact_contact.txt)) | [shell_impact_sequence.png](shell_impact_sequence.png) | |

**The fog take** shows the flame line with the viewer seeing its search points from the fifth on; the points
before it and the attacker behind them lie in the fog (the ground there darkened to a quarter with a hard edge, as
the battle terrain draws fog).

**The repeat take** spawns the cleave four times on one spot, 0.6 s apart, each with its own random draw as
consecutive hits in a battle have; its columns span from the first hit to the end of the last (2.75 s).

### What changed, point by point

| Effect | Round-1 finding | What changed |
|---|---|---|
| `flame_line` | Bubble outlines on the flames (foam) | The fireball-flipbook tongues are gone. The fire is soft-eroding billows of the smoke element in a fire ramp, and flame tongues stretched along their rising velocity; both are born hot and die to soot. |
| | Pale cream, not fire | The jet and the body of fire have opaque, saturated orange ramps. The jet is yellow-white on its axis only for its first two frames. |
| | Beaded streak | One continuous scorched band of torn ground, of uneven width, with napalm embers glowing in it for about 0.6 s. |
| | Flames behind the first search point; overshoot past the end | Every fire layer is cut at the line's ends. Every line placement travels with the jet, so nothing appears ahead of its tip. |
| `lance_sweep` | Ruler-line scar | A burnt groove of torn ground whose pieces vary about twofold in length and width, in a fading scorched halo. It glows from the inside as it cools (the glow shrinks into the groove's middle) and through short glowing cracks that outlast the glow. |
| | Capsule head, no airborne part | The stroke and the flares at its head are torn by noise from the first frame. The wide round glow is removed. Molten spatter is thrown off every point (in round 1 its burst was too small to spawn at all). |
| | Dotted row of smoke | Puffs of their own size, place, time and drift. |
| `cleave_impact` | No punch at frame 0 | A warm flash, a short light and sparks at the struck point, a crack and churned earth under it, dark clods on arcs. |
| | Pencil ring that pops | The front is a wider, noise-broken dust skirt with small dust puffs kicked out behind it. It thins out over its last six frames while it erodes. |
| | Pinwheel of dust | A dense burst at the struck point and puffs of their own size and place, barely pushed outward. Each hit draws its own. A cone along the blow's direction is not possible: a splash event carries no direction (plan Amendment 5). |
| | One value | The flash and the sparks are the bright focal point at frames 0–1. The clods and the crack are the darkest, the dust lies between. |
| `shell_impact` | Regressed: blast mark 45–55 % weaker | Restored: it draws its round-5 random choices again. Its sheet and sequence here are the round-5 files, and a fresh capture equals them pixel for pixel. |

Not changed this round: `flak_burst` (accepted; its polish items are open), `acid_burst` and `spine_line` (no
regression found), and the shell's own polish items (a new blast-mark texture, the ring's width variation).

### Crowd capture (item 6), scale-out round 2

[`stress_capture_400.png`](stress_capture_400.png) is the 400-unit battle at tick 150 with 200 splashes a second
streamed round-robin over all seven effects. The battle is changed for this round so that it fields
`napalm_wheeler` (real jets that cross), `sky_bastion` and `vector_interceptor` (real anti-air bursts among air
units). The stream now bursts `flak_burst` only at airborne units, in their plane.
[`stress_capture_400_baseline.png`](stress_capture_400_baseline.png) is the same tick without the stream; it still
shows the battle's own jets and bursts. [`stress_capture_400_detail.png`](stress_capture_400_detail.png) and
[`stress_capture_400_baseline_detail.png`](stress_capture_400_baseline_detail.png) enlarge the middle of each twice.
Figures (coverage, peak footprints per cell, retired instances): [`stress_capture.txt`](stress_capture.txt),
paragraph "Scale-out round 2". The cleave's attacker and the lance's cannot take part in a battle yet (they have no
unit media), so their effects come from the stream; the repeat take above shows the cleave at its real cadence.

## Scale-out round 1 (WP7)

The slice's review gate passed in round 5. The scale-out adds four splash effects and polishes two of the
accepted ones. Please write this review to a new file next to this one, `independent_review_scaleout1.md`, in
the same form as the slice's reviews: each effect below against each of the seven checklist items (at the end of
this packet), **accept** or **reject** with a concrete reason (sheet, cell or frame, what you see), and for the
two polished effects whether the change regressed anything. The images are lavapipe captures from a debug
`battle_capture --effect-lab` under Xvfb, as for the slice; the sheets' layout is the slice's (eight ages across;
rows: bright ground default zoom, the same with the 1-unit grid and the bound extent in red, bright close zoom
and its reference, then the same four rows on dark teal ground). For a line the grid row draws the line and the
full-damage radius at each search point.

### What the new effects are meant to be (StarCraft II analogues; looks stay original)

| Effect | Unit (analogue) | Simulated extent | Meant to read like |
|---|---|---|---|
| `flame_line` | `napalm_wheeler` (Hellion) | a line of 25 searches of radius 0.15 from 0.25 to 6.5 units ahead of the attacker | a short jet of burning napalm racing from the nozzle along the whole line, burning for a moment and leaving a burnt streak |
| `lance_sweep` | `tripod_annihilator` (Colossus) | a line of 11 searches of radius 0.15, 2.5 units long, across the struck point | a thermal lance sweeping across the target, burning a glowing line into the ground that cools |
| `flak_burst` | `sky_bastion` air weapon (Liberator's anti-air rockets) | one zone of radius 1.5 in the air units' plane | an anti-air burst among air units (drawn on the ground in the lab; in battle it is at the air units' altitude) |
| `cleave_impact` | `stag_behemoth` (Ultralisk) | zones 0.5 / 1.0 / 1.5 at the struck unit's near edge | a heavy blow driven into the ground, dust and soil thrown out to the cleave's reach; frequent (every 0.6 s), so it must not stack |

Two new building blocks are used: a **ribbon** (a strip along the line, racing out and sweeping) and a
**distortion** (a heat haze that bends the ground behind it; subtle by design).

### Material

| Effect | Contact sheet | Sequence (40 ms steps) |
|---|---|---|
| `flame_line` | [flame_line_contact.png](flame_line_contact.png) ([legend](flame_line_contact.txt)) | [flame_line_sequence.png](flame_line_sequence.png) |
| `lance_sweep` | [lance_sweep_contact.png](lance_sweep_contact.png) ([legend](lance_sweep_contact.txt)) | [lance_sweep_sequence.png](lance_sweep_sequence.png) |
| `flak_burst` | [flak_burst_contact.png](flak_burst_contact.png) ([legend](flak_burst_contact.txt)) | [flak_burst_sequence.png](flak_burst_sequence.png) |
| `cleave_impact` | [cleave_impact_contact.png](cleave_impact_contact.png) ([legend](cleave_impact_contact.txt)) | [cleave_impact_sequence.png](cleave_impact_sequence.png) |

Measured on the GPU (`effect_slice_gpu_tests.rs`, lavapipe): the Critical marks of each line end within 1.3 px of
the line's projected ends; the fronts of the two circles end within ±2 px of the reach on at least 97 % of 72 rays;
coverage of the full-damage area within 80 ms is 65–99 %; the largest
frame-to-frame change after the impact is at most 1.63/255; nothing differs from the empty ground after the end.
With a line seen by the viewer only from its fifth search point on (fog), nothing is drawn more than 1 unit
before that point.

### Polish of the accepted effects (round 5's remaining improvements)

| Effect | Round-5 finding | What changed |
|---|---|---|
| `acid_burst` | Item 6 of the list: frame 0 shows a concentric inner ring and sparkle dots | The front and its dark edge start at 0.3 (was 0.55) of the full-damage radius, under the crown; the fine spray takes the globs' dark wet ramp. [acid_burst_contact.png](acid_burst_contact.png), [acid_burst_sequence.png](acid_burst_sequence.png) |
| `spine_line` | The scar's blotches repeat at about the point spacing | The scar's pieces are scattered 0.3 (was 0.1) along and across the line. [spine_line_contact.png](spine_line_contact.png), [spine_line_sequence.png](spine_line_sequence.png) |

The `acid_burst` and `spine_line` sheets of this directory are now the polished ones, and `shell_impact`'s is
re-taken too: the random choices of every effect (rotations, placements, variants, noise phases) are now drawn from
the effect's name rather than its position in the vocabulary, so each of the three shows a different, equally
random draw than in round 5 ([shell_impact_contact.png](shell_impact_contact.png),
[shell_impact_sequence.png](shell_impact_sequence.png)). Please check all three for regressions. Round 5's sheets
are in commit `58b8da2`. Not changed this round: `shell_impact`; the acid stain's meniscus, inner rims, glob shapes, crowd variation and
dried crust; the spine scar's torn clods.

## Round 5

Round 4 accepted `shell_impact` and `spine_line` and rejected `acid_burst` narrowly
([`independent_review_round4.md`](independent_review_round4.md), kept as written with rounds 1 to 3). This is
round 5, the last of Q0080's five rounds. Please write the round-5 review to a new file next to this one,
`independent_review_round5.md`, in the same form, and judge all three effects afresh against the checklist (the
two accepted ones too, so that a regression would show). The tables below map each round-4 finding to what
changed; they list changes and measurements, not verdicts. The images of this directory are the round-5 ones
(same path as rounds 3 and 4: lavapipe, debug `battle_capture`, Xvfb); round 4's are in commit `b72b693`. The
sheets' ages are unchanged.

### `acid_burst`

| Round-4 finding | What changed |
|---|---|
| Ranked change 1 / item 1: the stain's visible edge lies at 0.87 of the reach (its box at 0.98; the ragged, eroded edge about 13 % of the radius inside it) | The stain no longer uses its texture's outline as its edge. A decal may now have a **disc outline**: its visible edge is a disc whose edge wavers by a fine noise of ±2 % of the radius (about ±2 px at default zoom), and the texture (relief, pooling, rim) is read from its middle inside it. The stain is sized so that its mean edge lies on the reach, and it holds that edge **without erosion until about 1.8 s**; the deep bites come only with the drying after that. Measured on the round-5 frames along the centre row (bright ground; the outermost pixel more than 16 levels from the clean ground, against the middle of the reach circle's line): default zoom, 0.18, 0.48, 0.9, 1.8 and 3.0 s, 0 px left and right, diameter 1.000 of the reach; close zoom, 0.48–1.8 s, +2 px left, 0 to +1 px right. The GPU footprint test now measures the held stain's visible edge (72 rays, 1.39 s): 100 % of rays within ±2 px of the reach, mean −0.18 px, largest overshoot +1.27 px, on radv and lavapipe (it used to check only that the stain stayed inside). |
| Ranked change 2 / item 4a: a pale, translucent band between stain and lip in frames 2–8, about 0.1 of the radius wide all round | With the stain on the reach, the lip lies over the stain. The lip is also thickened (its coverage doubled) and its inner side is the stain's wet emerald at near-full opacity. Sampled (3×3, sequence frames, centre row from the reach inward, default zoom): frame 2 lip and inner side (41–50, 111–125, 36–50), body (47–57, 115–134, 47–57); frame 4 (40–48, 109–117, 36–48), body (52–57, 127–136, 52–58); frame 8 (41–47, 109–115, 38–47), body (54–58, 125–140, 52–59). Round 4's band sampled (98–109, 131–132, 88–98) at default zoom against a stain of (45, 103, 42). |
| Item 3: an 8 % contraction when the front leaves (frames 7–10) | The stain's edge is on the reach before the front fades, so the front fades onto stain. |
| Ranked change 3 / item 4b: the globs are one teardrop sprite, identical pale-lime commas | A new texture element of **three glob shapes** (round with a blunt trailing bulge, tearing in two along a thin neck, stretched with a ragged tail; image-model stills, [`texture_review.md`](texture_review.md) "Review round 5"), each instance one of them, of more varied size (0.22–0.46), oriented along its velocity, with its own ramp: a dark wet emerald body and lime only on its one small highlight. |
| Ranked change 4 / item 4c: dense near-white fumes | Thinner and fainter: smaller wisps broken harder by noise, in a yellow-green of lower value and about 60 % of the opacity. |
| Ranked change 5 (optional): a small "Y" crown | The crown card is 1.3 of the reach (was 1.0) and torn harder. Its texture fills about half its cell, so it still reads as a central splash, now larger. |

### `spine_line` (accepted in round 4; polish only)

| Round-4 note | What changed |
|---|---|
| Ranked 1 / 4c: the first clump is cut by the line's start into a single tall spike, an "L" or hook (frames 3–20) | A standing clump that a line end would cut is made smaller and moved inward so that it stands whole inside the line (it keeps its far edge); the first clump is now a smaller whole clump. The last clump is treated the same way. |
| Ranked 2 / 4c: on teal the scar is a grey-tan film of the ground's value ((119, 126, 116) on (95, 138, 133)) | The scar's ramp now only darkens: every colour lies below its opacity times the teal ground's darkest channel, so it is a darker shade of whatever ground it lies on. Sampled on the round-5 sheet (dark close, 1.622 s, 3-row averages of a 500 px run along the line): (79, 102, 97) against the clean ground's (92, 137, 133) on the same rows. The clods remain a step lighter than the furrows. |
| Ranked 3: a lone dark chip at the far end at 1.6 s | It was the last clump's tip sinking into the ground at the very end of its life; the clumps now fade over their last 15 % as they sink. |

The fog sheet and sequence are re-taken with the round-5 data ([`spine_line_fog_contact.png`](spine_line_fog_contact.png),
[`spine_line_fog_sequence.png`](spine_line_fog_sequence.png)); the fog clipping is unchanged.

### `shell_impact` (accepted; optional changes)

| Round-4 improvement | What changed |
|---|---|
| 1: let the mark under the ring appear only once the ring has passed (no grey veil inside the ring) | The scoured blast mark now starts at 0.12 s, after the shock ring has reached the reach (about 0.115 s), and ends as before (8.03 s). |
| 1: an irregular streak fringe (new blast-mark texture) | Not changed (scale-out). |
| 2: push the ring's width variation further | Not changed, to avoid regressing an accepted effect in the last round. |

### Crowd capture, round 5

[`stress_capture_400.png`](stress_capture_400.png) is re-taken with the round-5 effects the same way as rounds 3
and 4 (debug `frame_budget`, lavapipe under Xvfb, a scratch copy of `assets/` whose `units.ron` gives
`volatile_tick` and `burrow_scorpion` the lab blocks, tick 150, 200 splashes a second); figures in
[`stress_capture.txt`](stress_capture.txt), round-5 paragraph. Round 4's frame is in commit `b72b693`.

## Round 4

Round 3 accepted `shell_impact` again and rejected `acid_burst` and `spine_line` (narrowly)
([`independent_review_round3.md`](independent_review_round3.md), kept as written with rounds 1 and 2). This is
round 4 of up to five (Q0080). Please write the round-4 review to a new file next to this one,
`independent_review_round4.md`, in the same form, and judge all three effects afresh against the checklist
(`shell_impact` too, so that a regression would show). The tables below map each round-3 finding to what changed;
they list changes, not verdicts. The images of this directory are the round-4 ones; round 3's are in commit
`fd24c31`, round 2's in `d03ed19`, round 1's in `bb5a61c`.

**What else changed in the material.** The contact sheets' last column is now the effect's true end, the moment
the runtime frees it: for the line that is its lifetime plus the travel of its last point, so the spine sheet's
ages are 0, 0.099, 0.263, 0.493, 0.985, 1.642, 2.463 and 3.284 s (round 3 stopped at 2.53 s, before the far
points' marks had faded; see `spine_line` item 3). The splash sheets' ages are unchanged. A new **fog sheet** and
sequence show the line running out of the fog (section "Fog" below).

### `spine_line`

| Round-3 finding | What changed |
|---|---|
| 4a / ranked change 1: dark pen-stroke ruptures and leaning single spikes cross the line; at 1.27 s only "/ \| — \\" strokes remain | The ruptures are now short, wide, **ragged tears of torn earth running along the line** (the `earth_scar` texture turned along the line, 0.84 units long and about as wide as the scar, of their own size, scattered at most 0.1 off the line), soft-edged (erosion softness 0.25) and lower in contrast (their own ramp, opacity 0.55 to 0.66, was 0.85 to 0.92); they open a frame or two before the clump and are gone by 1.0 s, with the spines. The **single leaning spikes are removed**: each clump now carries three to five heavy spikes, so nothing slanted stands on its own at any age. |
| 4b / ranked change 2: clumps small and twiggy, flat silhouettes with almost no shading, about half a unit tall | New clump textures ([`texture_review.md`](texture_review.md), "Review round 4"): heavy conical spikes, about a third as wide at the base as tall, a bright warm-umber left flank and a shadowed right flank (the key light from the upper left), and a **collar of torn soil clods** at the root. The card is larger (0.9 of the spacing, was 0.62) and stands higher, so the spikes rise about 0.7 to 0.9 units over the ground. The newest clump's highlight is gentler (ramp input 1.8×, was 2.4×), so it stays shaded while it is the brightest thing in the line. |
| 4c / ranked change 3: the scar is a thin, pencil-like grey line; the torn earth does not show | The scar's box is three times as wide (a decal's `width` may now exceed 1), so the furrow of torn earth is about 0.3 to 0.5 across and of uneven width; its own ramp keeps the dark furrows and the upturned clods a step darker than the ground, with the faint parts of the texture transparent, and less noise breakup lets the texture's clods show. |
| 3 note / ranked change 3: a faint stretch of fissure still visible at 2.53 s | The sheet now runs to the true end (3.28 s): its last column shows the ground clean. The runtime always kept the effect until then and every mark's intensity reaches zero at its own end. |

### `acid_burst`

| Round-3 finding | What changed |
|---|---|
| 1 / ranked change 2: after the front leaves (about 0.36 s) the stain covers only 75–85 % of the reach, with deep bites; it visibly contracts between frames 8 and 10 | The stain's erosion **holds the full disc until about 1.8 s** (only a noise-broken edge, threshold 0.10 to 0.12), then dries from its edge and from holes. It is at its full size from about 0.12 s, before the front reaches the reach, so nothing contracts when the front hands over. Its size is 0.98 of the reach: a first round-4 take at 1.14 drew the stain up to 11 px (of 87) beyond the reach, which a `Critical` layer must not do (it would show damage where there is none, plan §10.2), so the ragged edge now lies just inside the reach at every age, wet or dried, and the front's dark lip marks the reach itself while it lasts. |
| 4a / 5 / ranked change 1: from 0.9 s a flat, mottled mid-green moss patch; no darker wet rim at any age | The liquid **keeps its relief as it dries** (the crust keeps 75 % of the relief light; only the wet sheen dries away). A **dark wet rim** is drawn by the liquid material along every edge of the liquid, its outline and the holes it dries into, at every age: a band about 0.1 to 0.2 wide, darker and more saturated than the body (colour ×0.24, 0.44, 0.24 at the edge). Sampled on the round-4 sheet (bright ground, close zoom, a ray from the centre to the right edge, sRGB): at 0.9 s the body is about (57, 113, 50) and the band inside the edge about (62, 96, 50); at 1.8 s the body is about (77, 88, 57) and the band about (70, 80, 53). |
| 3 note / ranked change 1: the colour is still emerald at 1.8 s; olive only at 4.5 s | The stain's ramp has a wide olive-brown range below the emerald, and the drying curve walks the body there faster: olive-green by 0.9 s, **olive-brown by about 1.8 s** (sample above; round 3 sampled (77, 140, 64) at 1.8 s). |
| 4b / ranked change 3: the front is a pale, translucent, even glass or soap-bubble rim with a lighter halo; a pale gap between stain and rim | The front's ramp is now a **dark, wet, nearly opaque lip** (opacity 0.80 to 0.96, darkest at its leading edge), with no pale or translucent part; its trailing edge is broken by stronger noise, so its width varies about twofold around the circle. The stain lies under it from the first frame, so no ground shows between them. |
| 4c: the contraction is a soft pop | See the first row. |
| 4d / ranked change 4: frame 0 is a smooth, glowing lime oval of half the reach with white sparkle dots | The radial ground glow is removed. At frame 0 the stain itself is there, thick, at 0.85 of its size (about 0.9 of the reach, ragged), with the splash crown above it and a small hot core for two frames. |
| 2 note / ranked change 4: no airborne part at default zoom; tiny crown; globs not visible; vapour lies on the stain | The crown has its own opaque dark-wet ramp (lime only on its highlights), is larger from frame 0 (1.0 of the reach, growing to 1.18) and thrown higher, and torn harder by noise. **Eleven globs** of 0.28 to 0.44 units fly on visible arcs from across the burst (the particle level of detail had cut them at default zoom, because it read the emitter's small size). The fumes are fainter and pale yellow-grey, start from five places over the stain and rise at 2.2 units per second. |
| 4e / ranked change 5: the relief is regular and blobby, uniform over the disc | The relief's strength varies with low-frequency pooling noise, from flat pools (0.2) to large swells (1.6), and is read three mips above the footprint, so only broad swells catch the light. |
| Crowd: flat, light, pastel-green fields | Follows from the rim, the kept relief and the early olive (see the crowd capture). |

### `shell_impact` (accepted in rounds 2 and 3; optional changes)

| Round-3 optional change | What changed |
|---|---|
| 1: vary the shock ring's width by about two around its circumference; drop the haze inside it | The front's trailing-edge noise is stretched in contrast, so the band's width varies about twofold around the circle (its outer edge, the reach, is unchanged). The dust kicked out along the ground starts at 0.12 s, after the front has left the reach (was 0.03 s), so no haze sits inside the ring while it races out. |
| 2: a less regular streak fringe | Not changed: it needs a new blast-mark texture; recorded for the scale-out. |
| New note: four soft lobes at the flash's edge on teal at age 0 | Not changed (not visible at gameplay zoom). |

### Fog (checklist item 7), round 4

The round-3 review asked for a picture of the line running out of the fog. [`spine_line_fog_contact.png`](spine_line_fog_contact.png)
(legend [`spine_line_fog_contact.txt`](spine_line_fog_contact.txt)) and [`spine_line_fog_sequence.png`](spine_line_fog_sequence.png)
are the same program as the other sheets with the burrowed attacker and the line's first four search points in
the viewer's fog. The fog is drawn as the battle terrain draws it: fogged ground keeps a quarter of its colour, with
a hard edge at a cell boundary (`terrain.wgsl`). The line's points sit at cell centres, as for units standing on
cells, so the fog edge lies half a spacing before the first seen point.

What it showed, and what changed for it. Before round 4 every layer was clipped only to the whole line, so the
scar and fissure pieces of the first seen point ran on up to about a unit into the fog. Now the ground-hugging
layers (scar, tears, heave, fissure, dust, chips) end where the line enters the fog, the cell edge half a spacing
before the first seen point, fading in over 0.3 units across it (the same feather as at the line's ends). A first
fog take also clipped the clumps there, and the first clump, whose splayed spikes reach about 0.9 to each side of
its point, was cut in half at the edge; so a standing clump is now drawn whole whenever its point is seen (only the
line's own ends cut it): it never pops or appears cut, and its outer spikes may lean up to about 0.4 over the
fogged ground beside the first seen point. Nothing else is drawn on the fogged side, and nothing at the attacker.

### Crowd capture, round 4

[`stress_capture_400.png`](stress_capture_400.png) is re-taken with the round-4 effects (final data, the stain at
0.98 of the reach), the same way as round 3: debug build of `frame_budget`, lavapipe under Xvfb (the driver and
build of the contact sheets), a scratch copy of `assets/` whose `units.ron` gives `volatile_tick` and
`burrow_scorpion` the lab blocks of `crates/client/tests/data/effect_lab_blocks.ron`, tick 150, 200 splashes a
second; the figures are in [`stress_capture.txt`](stress_capture.txt), round-4 paragraph. Round 3's frame is in
commit `fd24c31`. The baseline is unchanged (it draws no effect).

### Fog capture, round 4

New in this round, see "Fog (checklist item 7), round 4" above: [`spine_line_fog_contact.png`](spine_line_fog_contact.png)
with its legend [`spine_line_fog_contact.txt`](spine_line_fog_contact.txt), and
[`spine_line_fog_sequence.png`](spine_line_fog_sequence.png), the line running out of the viewer's fog.

## Round 3

Rounds 1 and 2 were rejected for `acid_burst` and `spine_line` ([`independent_review.md`](independent_review.md),
[`independent_review_round2.md`](independent_review_round2.md), both kept as written); `shell_impact` was accepted
in round 2. The owner chose to continue, for up to five rounds in all (plan Amendment 2, Q0080), working from
round 2's ranked changes. Please write the round-3 review to a new file next to this one,
`independent_review_round3.md`, in the same form, and judge all three effects afresh against the checklist
(`shell_impact` too, so that a regression would show). The table below maps each round-2 finding to what changed;
it is a list of changes, not of verdicts. The images of this directory are the round-3 ones; round 2's are in
commit `d03ed19`, round 1's in `bb5a61c`.

### `spine_line`

| Round-2 finding | What changed |
|---|---|
| 4a: curved spine variants read as handwritten glyphs; C-, J- and claw-shaped hooks | New textures (image-model stills, [`texture_review.md`](texture_review.md), "Review round 3"): the clump at each search point is one card showing three to five **straight, tapering conical chitin spikes splayed outward from a shared base** (three variants: a fan of four, an uneven five, an uneven three), wide at the base and needle-sharp at the tip, upright within a few degrees, mirrored at random. The hook, claw, thorn and S-curve variants are gone. |
| 4b: sparse, one or two spines per point | Each point has its clump (three to five spikes) plus two single straight spikes leaning out beside it (up to ±26°), a frame later: four to seven spikes per point. |
| Ranked change 1: taller, 0.6–0.9 units | The clump card is sized to the spacing (1 unit) with ±15 % jitter, and the spikes stand about 0.6 to 0.9 units tall at the default binding; the single spikes are smaller (0.6 of the clump). |
| 5 / ranked change 2: no focal point; newest clump no brighter; tips not visible | The newest clump is lit for its first two to three frames: its ramp input is raised to 2.4× (the dark umber body goes to warm bone and the tips and the lit left edges to bone-ivory), easing back to its own colours by about 0.2 s after it breaks the ground. A short, light dust burst (pale, about 0.3 s) rises at each eruption. The tips stay paler than the bodies on every spike. |
| 4c / ranked change 3: hard straight cut at both ends of the churned earth and fissure | Every ground mark of the line (scar, fissure, cracks) **feathers over 0.3 units at each end** across the end (60 % of the fade beyond it, so the mark is still about two thirds strong at the end), instead of the 0.015-unit cut. Nothing is drawn more than 0.18 before the first search point (which lies 1.0 from the attacker's centre), and that last 0.18 is the faint tip of the fade. |
| 4d / ranked change 3: leftover marks alternate stars and donut rings at exactly 1-unit spacing | The churned earth is a new texture, a ragged furrow of torn earth, stretched along the line (each piece about 1.9 units long, 1 unit apart, scattered and of its own size), so the pieces merge into **one continuous broken scar** of uneven width. The crack bursts are now two narrow, elongated ruptures (the donut-shaped torn hole and the radial star are dropped), scattered off the line by up to 0.25, randomly turned, ±40 % in size, and gone by 1.5 s, before the scar. Each eruption also heaves a noise-broken patch of soil over its full-damage disc, which erodes away within 0.7 s, so the point that is hit reads at once without leaving a row of patches. |
| 4e: dust puffs pale flat blobs on teal | The settling dust is fainter (opacity 0.22 to 0.38, was 0.35 to 0.55) and darker; the new light dust burst is short and small. |
| Crowd note (b): the churned earth reads as dirt over the whole battlefield | The scar is lighter (opacity 0.34 to 0.52, was 0.45 to 0.62) and a little shorter-lived (2.3 s, was 2.4 s); the fissure lasts 2.5 s (was 2.6 s), so the marks still state the line once the spines have sunk. |

### `acid_burst`

| Round-2 finding | What changed |
|---|---|
| 4a / 5 / ranked change 1: one flat, uniform mid-green fill, no sheen, depth or value variation; the rim lighter, not darker | The stain is drawn as a **liquid** (a new decal material): noise varies its value and thickness so it pools darker and thinner in places; its thickness is lit as a surface by the key light from the upper left (the units' studio light), so swells and the thick rim catch a wet specular sheen and the far sides fall darker; the sheen and the relief light dry away with the stain. The colour ramp follows the art direction: wet emerald in the middle, a **darker, more saturated wet rim**, olive-brown crust when dry. |
| 3 note / ranked change 1: drying starts late (still saturated green at 1.8 s) | Drying starts at about 0.4 s and is well under way by 1.5 s (ramp factor 0.64 at 1.5 s, was 0.85); the sheen is gone by about 1.5 s. |
| 4b: evenly spaced round darker dots | The flecks the landing globs left (they were small copies of the stain, which read as dots) are removed; the stain's erosion noise is coarser (scale 1.4, was 2.6), so its holes are irregular rather than round. |
| 4c / ranked change 2: the pop is a glossy lime ellipse with a bevelled rim, like a button | The glowing dome is replaced by a **ragged burst**: the splash-crown flipbook (a real liquid splash from a Veo clip) drawn as liquid matter (dark wet body, lime highlights) and torn by coarse noise, plus a small hot core broken by noise for two to three frames and a ragged ground flash. The front's leading edge is a darker wet green, no longer a glowing lime rim. |
| 2 note / ranked change 2: globs only tiny white specks; larger globs with dark wet bodies | Nine globs of 0.17 to 0.30 units (were sixteen of 0.10 to 0.18), dark green bodies with a lime highlight. |
| 2 note / ranked change 2: vapour a flat lichen-like patch to one side | The vapour is four soft puffs at the centre (within 0.15 of the reach), rising (0.7 units per second) and spreading, gone by 1 s. |
| 4e / 6 / ranked change 3: in the crowd only thin pale-green front rings, like selection circles | The stain is now **Critical**: when the crowd budget retires the Detail layers, the stain stays and marks the area as a liquid disc, and the front sits on it. The front ring and its dark edge are more broken by noise. |
| 4d / ranked change 3: a pale grey-white fringe at the stain's edge | The stain's edge is now its darker wet rim (the ramp's top) over an olive-brown low end, shaded by the liquid relief; the round-2 vapour patch that lay beside the stain, and the flecks along its edge, are gone. Please check the edge at 0.48 to 1.8 s (rows 3 and 4). |

### `shell_impact` (accepted in round 2; optional changes)

| Round-2 optional change | What changed |
|---|---|
| 3: darken the char core towards 0.01–0.03 linear | Done: the char ramp is now 0.007 to 0.024 linear (was 0.010 to 0.035). |
| 1: vary the shock ring's width by about two, drop the faint fill | Not changed: it alters the accepted Critical layer and its footprint test; recorded for the scale-out. |
| 2: a less regular streak fringe | Not changed: it needs a new blast-mark texture; recorded for the scale-out. |

### Fog (checklist item 7), round 3

The owner extended the fog rules (Q0076, plan Amendment 2): an undetected burrowed attacker stays hidden, but its
spine line is shown to the viewer **wherever the viewer sees it**, from its first search point on. In round 2 the
line appeared only if its first search point was in sight; now a line that runs out of the fog into sight draws
at exactly the search points in sight (tested in pixels). Judge item 7 on the sheets as before: nothing may be
drawn on or behind the attacker (before the first search point and its 0.15-unit feather).

### Crowd capture, round 3

[`stress_capture_400.png`](stress_capture_400.png) is re-taken with the round-3 effects (see
[`stress_capture.txt`](stress_capture.txt)); the baseline is unchanged.

## Round 2

Round 1 was rejected ([`independent_review.md`](independent_review.md), kept as it was written). This is the
second and last round: if it is rejected too, the effect work stops (plan, WP6c gate). Please write the round-2
review to a new file next to this one, `independent_review_round2.md`, in the same form, and judge the material
afresh against the checklist; the table below says what changed for each finding of round 1 so that you can check
each one, but it is a list of changes, not of verdicts. The images of this directory are the round-2 ones; round 1's
are in commit `bb5a61c`.

**One art direction** now governs every effect: painted realism to match the units, one focal point, a palette per
element, soft eroded edges, irregular shapes from noise with a phase of its own per instance, and an airborne part
for every impact. It is written as concrete rules in [`../reference.md`](../reference.md) ("Art direction").

| Round-1 finding | What changed |
|---|---|
| Shell 1: the ring is cut to two arcs after 120 ms; the scorch reaches only about 0.8; nothing marks the 0.47 core | The shock front is a whole ring drawn over the fireball's base (sorted in front of it), wavering in width and brightness, brightest at its leading edge, thinner as it expands, at the reach from about 0.1 s. The ground mark is two layers: a charred core sized to the full-damage zone and a new scoured blast mark (radial soot streaks) sized to the reach. |
| Shell 4a: cotton-ball flash | The flash is a soft radial glow computed in the shader (white core, warm halo), broken slightly by noise, two frames long. |
| Shell 4b: bright rims on fireball lobes | Self-shading darkens thin parts of the fireball (and of smoke and dust) instead of lighting them. |
| Shell 4c: a clean ring of constant width | See item 1: the front is computed from noise, uneven and asymmetric per burst. |
| Shell 4d: a uniform grey scorch with stair-stepped edges, the same silhouette each time | New blast-mark texture (dense core, radial streaks, feathered fringe) over a near-black char core; erosion by noise with its own phase per instance and a soft step; decals sample the mip their screen size needs. |
| Shell 3 note / ranked change 3: no outward throw; the fireball holds one silhouette | Three overlapping fireball cards of their own rotation, size and noise phase, growing by about 50 % over the first 0.2 s; larger, darker debris chunks on ballistic arcs; longer sparks; a dust ring kicked outward along the ground. |
| Acid 2: everything is a coplanar decal | A glowing pop dome over the centre, sixteen globs thrown up on arcs that each leave a small fleck where they land, a fine spray, and a low vapour puff that is gone by 0.6 s. |
| Acid 3: four symmetric wedges; no visible drying; uniform fade | The front's break-up is driven by noise with its own phase per burst; the stain dries from wet emerald to an olive crust and is eaten by erosion from its edge and from holes. |
| Acid 4: clip-art splat; two stacked stickers; notched ring | One new stain texture (an irregular viscous puddle with a thick wet rim, two variants, random turn and noise phase) over the whole disc; no stacking; the splat texture is gone. |
| Acid 5: no focal point; pastel lime; washes out on teal ground | A lime-yellow pop and wet centre over a deep emerald field with a darker wet rim and a thin dark rim under the front. |
| Acid 1 note: 55 % of the radius at age 0 | The stain starts at about 60 % of the reach and spreads with the front to the reach by 0.17 s. |
| Spine 1 and 5: pale spines on pale sand; a dashed line | New chitin spines (four curved variants, dark umber with bone tips), larger, in clumps of three per point; the newest clump is lit as the travelling front. |
| Spine 2 and 3: two layers; no anticipation, no overshoot, fading retraction; regular dashes | Six layers: churned earth, a crack burst per point that opens about two frames before its spines, the fissure, the spines (rising out of the ground, overshooting by 10 %, settling, sinking back into the ground), dirt puffs and chips. The regular oval dashes are gone. |
| Spine 4: a comb of identical flat wedges leaning one way | Hashed variant, mirror, lean (±14° about the base), height (±30 %), scatter and stagger per spine; cards stand upright and turn to the camera, so they are never seen edge-on. |
| Spine 7: streaks about 0.6 units behind the first search point | Every layer (cards, ground marks and particles) is clipped to the segment from the first search point to the last. The first search point lies 1.0 from the attacker's centre, just beyond its body (radius 0.9375), so nothing is drawn on or behind the burrowed attacker. |
| Cross-cutting: flatness, three styles, symmetric erosion, alpha-clip stair-stepping | The art direction above; airborne parts in every effect; per-instance noise in every erosion; soft erosion steps and mip-matched decal sampling. |

## What the effects are meant to be

The game is a real-time strategy game whose unit mechanics follow StarCraft II. Each effect must read like the
StarCraft II behaviour named here, in an original look:

| Effect | Fired by | Must read like | Gameplay size (world units; 1 unit is one grid cell) |
|---|---|---|---|
| `shell_impact` | a siege tank analogue in siege mode | a heavy shell exploding on impact, leaving a burn mark | full damage radius 0.47, middle 0.78, reach 1.25 |
| `acid_burst` | a Baneling analogue (a creature that bursts) | a burst of green acid covering its whole area at once, leaving a drying stain | one zone, radius 2.2 |
| `spine_line` | a burrowed Lurker analogue | a straight line of spines erupting one after another from the attacker towards the target and on to full range, then retracting | 8 units long, a search point every 1 unit, radius 0.5 at each |

## Material

All files are in this directory unless a path is given. Open every image at full size.

### Contact sheets (one per effect)

Each sheet is a grid of real captures of the production renderer (1280×720, the battle camera at its 58° pitch)
at eight ages of the effect (columns; the legend file gives the ages in seconds). Rows, top to bottom: bright
ground at the battle's default zoom; the same with a 1-unit grid and the gameplay radii drawn as reference
(red: full damage, yellow: reach; for the line, a red circle at each search point); bright ground at close
zoom; its reference; dark ground at default zoom; its reference; dark ground at close zoom; its reference. The
reference rows are drawn only to let you check sizes; they are not part of the effect.

| Effect | Sheet | Legend |
|---|---|---|
| `shell_impact` | [shell_impact_contact.png](shell_impact_contact.png) | [shell_impact_contact.txt](shell_impact_contact.txt) |
| `acid_burst` | [acid_burst_contact.png](acid_burst_contact.png) | [acid_burst_contact.txt](acid_burst_contact.txt) |
| `spine_line` | [spine_line_contact.png](spine_line_contact.png) | [spine_line_contact.txt](spine_line_contact.txt) |

### Sequences (timing)

The first 3 seconds on the bright ground at default zoom, one frame every 40 ms (the game's presentation
step), read left to right, top to bottom; frame 0 is the moment of the event.

- [shell_impact_sequence.png](shell_impact_sequence.png)
- [acid_burst_sequence.png](acid_burst_sequence.png)
- [spine_line_sequence.png](spine_line_sequence.png)

### Crowd capture (checklist item 6)

A frame of the real client during a measured battle of 400 units (eight armies on a flat map, seen from far
above), with 200 splash events per second streamed over the battle, round-robin over the three effects, each
at a random live unit; and the same moment of the same battle without the stream, for comparison. The
effects' footprints (cards and ground marks, including their transparent parts) cover the share of the screen
given in [`stress_capture.txt`](stress_capture.txt).

- [stress_capture_400.png](stress_capture_400.png): with the stream
- [stress_capture_400_baseline.png](stress_capture_400_baseline.png): without the stream
- [stress_capture.txt](stress_capture.txt): tick, live effects, coverage and overdraw

### Fog (checklist item 7)

The fog rules are enforced in code and tested, not visible on a contact sheet: an effect is shown when the
point where it lands is visible to the viewer, and no effect has a layer at its source. For the spine line that
means the spines show where they erupt and nothing is drawn at the burrowed attacker. Judge item 7 on the
sheets (is anything drawn at the source end of the line, or outside the area?) and on this description.

## Checklist (plan §7.2, verbatim)

1. **Reads at gameplay zoom:** the dangerous area or line is identifiable within 0.1 s; a Critical layer marks its
   true size (±2 px at default zoom); one focal point; secondary elements do not compete with it.
2. **Layered:** at least three layers for a Major effect (for example flash or light, core, smoke or debris, ground
   mark); no single flat shape.
3. **Timing:** peak within 2 frames of the event (≤ 80 ms at 25 Hz presentation), eased fall-off, a dissipation that
   erodes rather than fades, nothing lingers past its duration except authored decals.
4. **No tells of cheapness:** no visible quad edges, no hard line where a card meets the ground (soft depth), no
   visible tiling or repetition (random rotation, flip, frame offset, variants), no pops on spawn or despawn, no
   banding in gradients, no colour fringes from keying.
5. **Value and colour:** brightest at the focal point, HDR core where the element emits, ramp from hot to dark; team
   colour only in secondary elements; consistent with `media_style.ron`'s palette.
6. **Crowd behaviour:** 200 splashes per second over a 400-unit fight still shows the units (overdraw and coverage
   measured, §10.3); the budget's degradation is not visible as popping.
7. **Fog:** nothing shows that the viewer may not see (D-5); an undetected Lurker's spines show, the Lurker does not.

Note on item 5: the project decided against an HDR camera with bloom (no value above display white exists in the
picture), so "HDR core" is to be read as "the core is the brightest, display-white part of the element".
