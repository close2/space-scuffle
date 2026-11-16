# Independent Review of WP11 (the Weapons' Own Looks, the Default Death, Fungal Growth), Round 1

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md), WP11 row of
§9, §7.2 (checklist), Amendment 24. Packet: [`independent_review_packet.md`](independent_review_packet.md), section
"WP11 round 1" and the shared sections. The bar: [`independent_review_wp10_1.md`](independent_review_wp10_1.md) and
[`independent_review_wp10_4.md`](independent_review_wp10_4.md). Branch `effects-wp11`, head `670f2a0c`. This is
round 1 of at most five (Q0080). I did not build this work. I found no implementer self-assessment for this round,
and I formed these verdicts from the pictures, the data and the code alone.

## Method

I read the packet section and Amendment 24. I read the WP11 part of `assets/catalog/effects.ron` (ramps, curves,
emitters, the 26 new looks and `defaults`) and the diff of `client::effects` (`systems.rs` in full:
`travel_request`, `shot_clip`, `replaces_legacy`; the missile part of `areas.rs`; `runtime.rs`, `render.rs`
(E3-1), `catalog/units.rs`, the lab driver). I also read the new and re-baselined tests: `weapon_look_tests.rs`,
the coverage test in `effect_catalog_tests.rs`, `shipped_shots_draw_along_their_paths`, the Fungal child of
`area_link_gpu_tests.rs`, and the diffs of `status_gpu_tests.rs`, `battle_fog_tests.rs`,
`sync_steady_state_tests.rs`, `sync_allocation_count_tests.rs` and `runtime_capture/driver/verify.rs`. I checked
the simulation facts the code relies on: `CombatEventVisual::anchor`, the bubble radius in `world/snapshot.rs`,
the missile order in `effects/step.rs`, and the weapon timings in `units.ron`. I built and ran nothing.

All pictures are lavapipe only. The family sheets show each look at **half** its on-screen size. I cut every cell
out with ImageMagick and enlarged it 4 to 6 times without smoothing; the crops are in `~/scratch/wp11-review1/`.
Pixel values come from the committed JPEGs, so they are accurate to about ±3 levels. I give sizes in **screen
pixels at the default zoom**: twice the sheet's pixels. I cross-checked them against the scale implied by the
measured half widths, about 34 px per world unit (`needle_volley`: (0.07 + 0.12) × 34 = 6.5 px; `gauss_tracer`:
0.04 × 34 = 1.4 px). For comparison, a Pulse Bot's bubble is about 26 px across at this zoom.

## Verdict table

| Look / item | Verdict | Main reason |
|---|---|---|
| Kinetic slugs `gauss_tracer`, `cannon_tracer` (form) | **accept with A1-1, E1-1** | A short slug crossing the gap, not a line across it and not a speed line. The heavy round is clearly heavier. But the gauss slug is 3 px wide, pale on pale ground, and arrives after its own impact (E1-1) |
| `sniper_tracer` | **reject** | A 1 to 2 px hairline about 48 px long, almost colourless. It reads as a thin line (A1-2) |
| `bullet_hit`, `needle_hit`, `weld_sparks`, `impaler_strike` | **reject** | Lost at gameplay zoom. `bullet_hit` is a 15 px flash for two frames and then nothing on bright ground. `needle_hit` is not visible on either ground. `weld_sparks` is a 2 px dot. `impaler_strike` is an 8 px dark speck. Every spark emitter is under 2 px (A1-3) |
| `cannon_hit`, `rocket_hit` | **accept** | A 14 to 16 px flash turning into a small fireball, then a grey puff. Small, but it reads as a burst and peaks at once |
| `plasma_bolt` | **accept with A1-4** | Reads as a bolt with a short comet tail and flies the whole flight. On bright ground the blue halo washes out to white |
| `plasma_hit` | **accept with A1-3** | A cyan-white bloom of about 10 px over four ages. The cold motes do not show |
| `prism_beam`, `lance_beam` | **reject** | A uniform pale line 4 to 8 px wide: pale lilac on prism, pale orange on lance. The "soft column" body cannot be seen, and no flow or pulses show in any cell. The core peaks at (231, 207, 239) over a floor of (176, 168, 160). It reads as the flat line the brief rules out (A1-5) |
| `beam_burn`, `lance_burn` | **reject (with A1-5)** | A dot of about 8 px. No sparks show (A1-3) |
| `acid_spit` | **reject** | It reads as a lime tracer streak, the same shape as `gauss_tracer`. The dark glob does not show. The mouth spray does not show at age 0 (A1-6) |
| `spore_bolt` | **accept with A1-4** | A yellow-green glowing head with a tail. It reads as a bio bolt. The dark glob inside does not show |
| `needle_volley` | **reject** | Invisible on bright ground at every age, and a 2 px pale speck on dark ground (A1-6) |
| `grenade_lob`, `rocket_trail` | **reject** | The grenade is a pale 2 to 3 px dot, not a dark tumbling shell. On bright ground the rocket's smoke trail cannot be seen: only a yellow head of about 4 px shows, and the launch puff is invisible (A1-6) |
| `acid_hit` | **accept with A1-3** | It reads as a green splat and stain of about 20 px, so it is legible. But nothing shows at age 0, and the crown is not distinct from the stain (peak at about 0.11 s) |
| `psi_slash`, `claw_rake` | **reject** | Thin pale arcs of even width that stay still: a drawn parenthesis, not a blow. The psionic blue does not survive on bright ground. The claw's flecks do not show (A1-7) |
| `glass_shatter` (default death) | **accept with A1-8** | A soft pale flash of about 16 px and a little dust. The shards are 1 px specks, so it reads as a pop of light, not glass breaking. Acceptable as a fallback only once the shards read |
| Fungal Growth missile, impact and mark (F1) | **accept with P1-1, E1-6** | The missile reads as a green orb in flight. The impact (flash, ring racing to the radius, rising spore clouds, mould stain) is the best look of the round. The ring reads as the area without dominating |
| Colour authoring (linear ramps) | **accept** | Every WP11 ramp converts to its brief: `plasma` (0.01, 0.06, 0.30) is sRGB (25, 69, 149), `fungal_mark` (0.17, 0.22, 0.045) is (115, 130, 60), `claw_edge` (0.92, 0.86, 0.76) is (246, 239, 226). The colour misses come from additive blending on light ground (A1-4), not from sRGB authoring |
| Shot geometry: from the bubble's surface to the impact | **accept** | `travel_request` starts the line at the collision radius, which is the radius the snapshot draws the bubble with (`world/snapshot.rs`). The unit test would fail on a centre-to-centre line |
| Timing of instant shots against their impacts | **E1-1** | The impact spawns at t = 0. The slug arrives 0.10 to 0.16 s later, after the hit flash (0.08 s) has gone |
| Fog: along the path | **accept** | `shot_clip` is fail-closed along the axis (cut = unseen sample + step + margin), has unit tests, and the GPU fog take changes no pixel over the unseen half |
| Fog: beside the path, mid-flight changes, missile reach | **E1-2** | Fixed reach constants (0.35) are smaller than the looks' widest layers. A clip is fixed at spawn for the whole flight. Nothing tests fog beside the path or along the camera's depth axis |
| Fog: Fungal Growth, pixels | **E1-3** | The Fungal GPU test counts handles only. No pixel over unseen ground is checked for the enemy's missile or for its cut impact |
| Steady state and O(N²) (charter §3.3) | **accept with E1-7** | Binary searches, inserts within the retained capacity, and the shot path allocates nothing. Both windows exercise shots and a landing missile. The valgrind counts are not in the measurements |
| Test discrimination | **E1-4, E1-5** | `status_gpu_tests` lost its positive control and now accepts double spawns. No test checks that a look can be seen at all |
| Legacy cards retired by `replaces_legacy`, kept as the fallback | **accept** | Correct and tested both ways. Keeping `vfx.rs` for the ownership audit is a sound deferral |
| E3-1 (distortion pipeline warm-up) | **accept** | The pipeline is now asked for every frame, before the early return, as the card and decal queues are. The `PERFORMANCE RATIONALE` block is in order |
| Scope deferrals of Amendment 24 | **accept with E1-6** | B-122 is a correct reading and the right deferral. The silent weapons, the missile's landing offset and the cleave are acceptable only once they are registered (E1-6) |
| Battle evidence | **V1-1** | The battle capture shows almost no WP11 look (two glints). The two green objects with hard diamond shadows are map props that also appear in the WP10 captures, not acid hits |
| **Overall** | **REJECT** | The engineering is sound and nearly complete. But about half the families do not read at gameplay zoom (needles, grenade, rocket smoke, beams, melee, small impacts, sniper), and the instant shots land their impacts before their rounds arrive |

## 1. Kinetic rounds

Evidence: `wp11r1_family_kinetic_lavapipe.jpg`. My crops: `gauss_cells.png`, `ct.png`, `ch.png`, `bh.png`.

- **`gauss_tracer`.** At t = 0 there is a 4 px muzzle dot. Then a slug grows to about 28 px long and 3 px wide
  (Critical core 1.4 px) and crosses the 6-unit gap in 0.13 s. It is pale yellow and creamy at its head. This is
  the slug the brief asks for: short, crossing in about three frames, never spanning the gap. It is not a speed
  line. It is weak on the bright floor, though: the slug's outer ribbon (0.1 units) is 3.4 px, and its colour is
  close to the floor's.
- **`cannon_tracer`.** It reads as heavier: a 10 px flash, then a thick, rounded bar of about 36 × 4 px. The
  "breath of smoke" (`hit_dust`, a light grey-brown) cannot be seen on the bright floor in any cell.
- **`sniper_tracer`.** It is a 1 to 2 px cold white line about 48 px long. At 1:1 it is a hairline, and with its
  cold, near-colourless ramp it reads as a scratch or a UI line. It is the "thin line" the brief rules out.
- **`bullet_hit`.** On bright ground: a pale flash of about 15 px at ages 0 to 0.08, then **nothing**. The five
  sparks are 0.020 to 0.032 units (under 1.1 px, stretched to 2 px), and the dust puff is light on light. On dark
  ground a faint grey puff shows. A Marine-analogue's hit should be small, but it has to be there for the hit's
  0.1 s.
- **`cannon_hit`.** A 14 px white flash turns into a small orange fireball by age 0.08, and then into a grey puff
  of about 12 px that clears by age 0.75. **Accept.** It is small for a Planetary-Fortress analogue, but the
  splash belongs to the simulation's data, not to this look.

Checklist: item 1 fails for `bullet_hit` and `sniper_tracer`. Item 2 holds in structure (flash, slug, sparks,
dust), but two of the layers draw nothing visible (A1-3). Item 3 fails for the instant shots (E1-1). Items 4 and 7
pass.

## 2. Energy: plasma and beams

Evidence: `wp11r1_family_energy_lavapipe.jpg`. My crops: `bolt.png`, `pb.png`, `lb.png`, `bb_ph.png`.

- **`plasma_bolt`: accept with A1-4.** A white body of about 10 px with a 12 to 20 px comet tail flies the whole
  flight. The ages match the bound 0.6 s (body centred +0.04 px from where the flight reached). It reads as a
  bolt. Its blue halo is additive blue on a floor of about (176, 168, 160), so it clips to near white, and on
  bright ground the bolt is white with a faint cyan fringe. A Stalker's bolt should read blue.
- **`plasma_hit`: accept.** A cyan-white bloom of about 10 px holds over four ages and fades. The motes do not
  show.
- **`prism_beam`: reject.** Across the gap at every age there is one pale line, 3 to 4 sheet pixels (6 to 8 screen
  px) wide with a 2 px core. I profiled a column through the middle: (188, 168, 179), (201, 179, 200),
  **(231, 207, 239)**, (198, 174, 206), (190, 171, 191) over a floor of about (176, 168, 160). There is no violet
  body: `beam_body` thickens the 0.3-unit column, but `beam_pulse` erosion (0.45 to 0.5) plus a breakup of 0.85
  eat almost all of it. What survives is beaded into a stipple along the line. The stipple does not read as
  flowing pulses at 1:1. It reads as a noisy, flat line, the "flat UI line" of the packet's question.
- **`lance_beam`: reject.** Its structure is the same: a uniform pale orange line with no visible body or flow.
- **`beam_burn`, `lance_burn`.** A hot spot of about 8 px held for the beam's life, with no sparks visible.
  Acceptable as a hot spot once the beam is fixed. It needs to be larger so that it is the beam's focal point.
- **The Void-Ray analogue's beam blinks.** `prismatic_saucer` fires every 9 ticks (0.36 s) with a random delay of
  up to 2.2 ticks. Each beam lives 0.3 s, eases in from 0.3 and falls to 0 at its end. In a sustained attack the
  beam therefore switches off and on about three times a second. A StarCraft II Void Ray holds its beam on its
  target while it attacks. This belongs with A1-5.

## 3. Bio projectiles and impacts

Evidence: `wp11r1_family_bio_lavapipe.jpg`. My crops: `bio_shots.png`, `bio_hits.png`.

- **`acid_spit`: reject.** On both grounds the look is a lime-yellow streak about 24 px long, with the same
  silhouette as the gauss slug. The additive `acid_glow` trail is the only thing that shows. The `Critical` dark
  glob (0.22 units, 7 px) is lost inside it or under it, and at age 0 nothing is drawn. A Roach's spit should read
  as a wet, dark glob that is visibly not a bullet.
- **`spore_bolt`: accept with A1-4.** A yellow-green glowing head with a short tail reads as an organic bolt. The
  dark glob inside does not show (0.13 units, about 4 px).
- **`needle_volley`: reject.** The bright-ground row is empty at every age. On dark ground there is a pale speck of
  about 2 px. The needles are dark (`spine_chitin`), 0.07 units (2.4 px) across, and stretched by 3, so they are
  too small to read on any ground.
- **`acid_hit`: accept with A1-3.** A green splat-and-stain of about 20 px from age 0.08 to 0.5, fading by 0.75.
  It reads as acid. At ages 0 and 0.03 almost nothing shows, so the peak arrives at about 0.11 s, more than two
  frames late (item 3). The crown cannot be told from the stain.
- **`needle_hit`: reject.** Nothing on bright ground. On dark ground there is a speck at ages 0.3 and 0.5.

## 4. Explosive

Evidence: `wp11r1_family_explosive_lavapipe.jpg`. My crop: `expl.png`.

- **`rocket_trail`: reject as shipped.** On dark ground it reads: a yellow-white head of about 6 px with a pale
  grey trail of about 50 px. On bright ground only the head shows. The trail and the launch puff (`rocket_smoke`,
  at most sRGB (134, 133, 130) at alpha 0.75) are almost the colour of the floor. A Viking or Banshee missile
  needs its smoke to read on the bright tiles that most of the shipped map uses.
- **`grenade_lob`: reject.** A pale dot of 2 to 3 px with a faint wisp on dark ground. The "dark tumbling shell"
  (`debris_atlas`, 0.08 units, 2.7 px) is not visible.
- **`rocket_hit`: accept.** As `cannon_hit`: flash, fireball, grey puff, about 16 px at its peak.

## 5. Melee

Evidence: `wp11r1_family_melee_lavapipe.jpg`. My crops: `melee1.png`, `melee2.png`.

- **`psi_slash`: reject.** At ages 0 to 0.15 one pale arc of about 30 px spans about 120° at an even 2 to 3 px
  width. At 0.3 it has faded, and from 0.5 on it is gone. It does not move: `slash_grow` scales it 0.8 to 1.15 and
  the arc never sweeps. On bright ground it is white. On dark ground it is a pale cyan line. It reads as a drawn
  parenthesis or a UI arc, which is exactly the packet's question. A Zealot's blow must read as a swing: a crescent
  thick in its middle and pointed at both ends, swept across the target over two or three frames, with its blue
  intact on bright ground.
- **`claw_rake`: reject.** Two faint pale arcs, `( )`, about 20 px across. They are barely visible on bright
  ground and plain arcs on dark ground. The dark flecks are 0.03 to 0.05 units (1 to 2 px) and do not show.
- **`weld_sparks`: reject.** A 2 px white dot at ages 0 to 0.08. The eight sparks (0.016 to 0.026 units) are
  sub-pixel.
- **`impaler_strike`: reject.** A dark speck of about 6 to 8 px (0.4 units, but standing, `Upright`, seen from
  above) that grows and recedes. A spine crawler's impale should be a clearly visible spike under its target.

## 6. Death and Fungal Growth

Evidence: `wp11r1_family_death_lavapipe.jpg`, `wp11r1_fungal_own_lavapipe.jpg`, `wp11r1_fungal_fog_lavapipe.jpg`,
`wp11r1_fungal_battle_lavapipe.jpg`. My crops: `glass.png`, `fi.png`, `fown_mid.png`, `fown_pair.png`.

- **`glass_shatter`.** A soft pale flash of about 16 px, then a few 1 px shard specks on dark ground (none visible
  on bright ground), then a faint dust puff. It is acceptable as a default death next to `vehicle_wreck` and
  `bio_burst`, which name their own looks, but only once the shards read (A1-8). The packet does not say which
  `UnitRadius` the lab binds. State it, so that the size can be judged against a real bubble.
- **`fungal_missile`: accept.** In the own take it is a lime orb of about 12 px with a darker centre, flying from
  the top right to the target over about 20 frames, legible against the tiles. The shed motes are hard to see at
  this frame size. That is acceptable: the orb carries the read.
- **`fungal_impact`: accept with P1-1.** Age 0: a lime flash. Age 0.03: a thin, slightly broken ring at the
  search radius around a green glow. Ages 0.08 to 0.15: four or five yellow-green spore puffs rising inside the
  ring. Ages 0.3 to 0.75: an olive mould stain under the victims, fading to small patches. It reads as an area
  spell. The ring marks the area and fades within 0.45 s without dominating. The puffs drift up-screen, past the
  ring's far edge (P1-1, the WP10 cloud's issue).
- **Fog take.** Only a narrow sliver of the enemy's impact is drawn at the right edge, as the cut should leave it.
  As in WP10 round 1 (V1-3), the sheet does not show where the fog lies, so I cannot judge from the picture
  whether a rising puff crosses the edge (E1-3).
- **B-122 deferral: accept.** The snapshot publishes `DamageOverTime` by kind only, and a look keyed on it would
  put spores on a storm's victims. Keying on `DamageOverTime + Revealed` would rest on an accident of today's
  catalog. Showing the slow and the mark until a per-status key is published is the correct, honest state. The
  fix is small (a presentation key on `StatusView`) and is correctly placed in a simulation plan. It is
  registered as B-122.

## 7. Engineering

**Accepted by reading.**

- `travel_request` uses the source's collision radius, clamped to 0.8 of the shot. That is the radius
  `world/snapshot.rs` draws the bubble with. The unit test fails for a centre-to-centre line by exactly the
  radius.
- Projectiles fly from the simulation's launch point over `FlightTime`. `ride` and `trail` follow the layer's
  `life`, and `one_shot_lifetime` ends the effect with the flight; the CPU test samples 25, 50 and 75 %.
- Shots never merge (`merge_line` now filters on `EventKind::Line`).
- `replaces_legacy` covers tracer, beam, projectile, melee and death, and keeps splash on its own path.
- The coverage test classifies delivery by `projectile_speed` first. That is why `hive_matron`'s melee talons
  correctly carry a `needle_volley` travel.
- `missile_extent` walks the spell tree with an exhaustive match and a node bound. Missile slots are
  binary-searched and inserted within capacity, which relies on the simulation's ascending ids; `effects/step.rs`
  removes with `retain`, so the order holds.
- E3-1 is fixed as specified.
- No catalog identity appears in Rust.

**E1-1: an instant shot's impact precedes its round.** `spawn_request` lays the impact of a `HitscanTracer` or
`BeamVisual` with `delay: 0.0`. The travel's slug takes its layer's whole duration to reach the target: 0.13 s for
`gauss_tracer`, 0.16 s for `cannon_tracer`, 0.10 s for `sniper_tracer`. So `bullet_hit`'s flash (0.08 s) is over
before the slug arrives, and `cannon_hit`'s fireball is already burning. The player sees the hit, then the round
reaching an already burning point. Fix it either way:

- (a) Delay the impact of a shot whose travel has a riding slug by that slug's arrival time. Bind it from the
  travel's `Critical` trail layer at table build, not as a constant.
- (b) Make the slugs arrive within one presentation frame (0.04 s) and keep the impact at t = 0.

Add a test: the impact's first drawn frame is not earlier than the slug head's arrival frame minus one.

**E1-2: fog reach and clip lifetime.**

- (a) `SHOT_FOG_HALF_WIDTH` and `MISSILE_SIGHT_RADIUS` (both 0.35) are constants, smaller than the looks they
  guard. `cannon_tracer`'s flash is 0.36 units before `flash_grow`, and its smoke is 0.26 grown by `smoke_grow`
  and drifting 0.45 a second. `fungal_missile`'s glow is 0.45. Derive each effect's reach at table build from its
  layers (size × peak size curve + placement radius + drift × life + lift), as `critical_half_width` does in the
  test.
- (b) The clip is a window along the shot's axis only. Unseen cells beside the path, within the reach, stop the
  sampling, but only cut the shot along its length. A shot running parallel to a fog edge at 0.4 units draws its
  flash and smoke over the edge.
- (c) The clip is computed once at spawn. Projectiles fly for 0.6 s and more, and Fungal's motes and the shot's
  smoke live longer, while the viewer's sight changes under them. This is the WP10 round 1 E1-2 issue again.
  Either re-clip held shots each frame, as `keep_missiles` does, or record the bounded leak.
- (d) The GPU fog take puts the fog edge across a shot that runs along screen x. Add a take with the fog beside the
  path, and one with the shot running along the camera's depth axis, where lifted and drifting cards project
  across the edge.

**E1-3: the Fungal Growth fog has no pixel check.** `fungal_growth_flies_lands_and_afflicts` counts held missiles
and spawned impacts. `looks.missiles().0` counts a missile held with `CLIP_NOTHING` as "flying", so the claim
"drawn only once over seen ground" is untested on the GPU. Assert what WP10 asserted for areas: in the fog take,
the number of unseen ground samples changed by the enemy's missile and by its cut impact is 0, and the seen part
is non-empty. Mark the unseen cells in the evidence picture (an overlay for the evidence only).

**E1-4: `status_gpu_tests` lost its discrimination.**

- (a) Before WP11 the control unit's default death had to draw legacy shards (`peak > 0`). That proved the shard
  detector works. Now every dying unit asserts `peak == 0`, so a broken detector passes. Keep a positive control:
  a synthetic `BubbleShatter` of a type no block or default covers (as `replaces_legacy`'s fallback allows), or the
  same battle under a test vocabulary without `defaults.death`, which must show shards.
- (b) `glass_shatter` is now checked with `>=`. A double spawn per death passes, and the measurements show 2
  against an expected 1. Count the expected default deaths from the battle's own `BubbleShatter` events of types
  without a named death, and keep the equality.

**E1-5: no test checks that a look can be seen.** The shot test passes `needle_volley` with 16 `Critical`
pixels, and `gauss_tracer`'s `CHANGE_LEVEL`-lit core, although neither reads in the pictures. Impacts and melee
looks have no GPU measure at all. As WP10's E1-5 did for edges:

- for every shipped `Shot` and `Point` look, on both grounds and at the default zoom, at its peak age, the area of
  pixels changed by at least 32 levels (the luma or the largest channel) is at least a stated minimum per class;
- the `Critical` layer's median contrast against the floor is at least 32 levels;
- print both in the measurements.

**E1-6: register the deferrals in `DESIGN.md`.** Amendment 24 records them in the plan, but only B-122 is in the
register.

- (a) `jet_jumper` (Reaper analogue), `void_goliath` (Immortal analogue) and `colossus_mech`'s air weapon show no
  attack at all in a battle: no shot, no impact. That is a readability bug, not a scope note. Register it with its
  data fix (a visual kind that publishes a line, or a projectile speed, and its golden-replay consequence).
- (b) The Fungal impact lands up to one tick's flight short (0.67 units, about 23 px at the default zoom, 30 % of
  its radius). Its `Critical` ring is therefore off by more than the ±2 px of checklist item 1. Register it.
  Meanwhile, extrapolate half a tick along the last two published positions. That halves the worst error, and the
  slot already holds the last position.
- (c) The `DESIGN.md` "Effect presentation" row still says "every other weapon keeps the older combat looks", and
  B-107 still says that splash weapons have no presentation. Both are stale after WP11. B-122 sits before B-121.

**E1-7: steady state.** The windows are right. `sync_allocation_count_tests` now fires a `Shot` for every weapon,
and holds a missile that lands every eight frames under the partial fog. But the valgrind block counts (baseline
and window) are not in `wp11r1_measurements.txt`, as WP10 round 1 asked (its E1-4). Record them. Minor: the
landing loop in `keep_missiles_seen` uses `Vec::remove` inside a loop, which is quadratic when many missiles land
in one frame (at most 64). A single compaction pass (write index, then `truncate`) removes that cost.

**E1-8 (minor): the capture's combat check accepts any effect card.** `verify_combat_effect_visible` passes when
`runtime.frame.sorted` is non-empty, which a status or area look also satisfies. Count only cards of `Shot`,
`Point` or `Death` effects.

## 8. Scope deferrals of Amendment 24

| Deferral | Verdict |
|---|---|
| Legacy module kept as the fallback; its kinds retired from the shipped game | Accept. The ownership audit needs it, and the replacement belongs to a later cleanup |
| No lightning look | Accept. No shipped weapon delivers lightning |
| Silent weapons (Reaper, Immortal, Colossus air) | Accept as out of scope for a presentation-only package, **on condition of E1-6 (a)** |
| Cleave direction | Accept. Pairing it by event order would be guessing. Recorded since Amendment 5 |
| Fungal impact position | Accept **with E1-6 (b)** |
| B-122, no `DamageOverTime` look | Accept (section 6) |
| Units without media unpresented | Accept. It does mean that half the families are judged only in the lab, which raises the bar for the lab evidence (V1-1) |
| No team colour on weapon looks | Accept. StarCraft II does not tint weapon effects. Ownership reads from the units |

## Required changes

| ID | Change |
|---|---|
| A1-1 | Gauss and cannon slugs: keep the form. Make the gauss slug's visible body at least 4 px wide at the default zoom, with a warm core that stands out on the bright floor (median contrast of at least 32 levels, E1-5). Make the cannon's muzzle smoke visible on bright ground (a darker smoke ramp, as `rocket_smoke` must also get) |
| A1-2 | `sniper_tracer`: a cold slug that reads, not a hairline. At least 3 px wide with a brighter, bluish head and a short, fading tail; a visible cold muzzle flash. It may stay longer and faster than the gauss slug |
| A1-3 | Impacts and particles. Every emitter whose particles are under 2 px at the default zoom (`hit_sparks`, `energy_sparks`, `weld_sparks`, `claw_flecks`, `spit_spatter` at its small end, `glass_shards`) is either enlarged to 0.06 units or more, or dropped from the look and from its layer count. `bullet_hit` stays visible for at least 0.12 s (a hot point of at least 12 px, then a dust puff that reads on bright ground). `needle_hit` gets a visible hit (a chip burst and dust of at least 10 px). `weld_sparks` gets a visible arc point (at least 8 px) with a spray that falls visibly. `impaler_strike` gets a spike of at least 0.6 units standing visibly beside the target. `acid_hit` peaks within 2 frames (a crown or splash visible at age 0). Beam burns at least 12 px |
| A1-4 | Hue on bright ground: plasma halo, spore glow, psionic blue and prism violet must keep their hue on the bright floor. Under each additive layer, add an alpha-blended (darkening) tint layer of the intended hue, so that the colour reads where the additive light clips to white (WP10's lesson). Check sampled display colours in the measurements |
| A1-5 | Beams (`prism_beam`, `lance_beam`): a visible coloured body at least 8 px wide at the default zoom (alpha-blended edges of the hue, not only additive), a brighter core, and pulses that visibly travel towards the target in a 40 ms sequence. A glow at the muzzle and a hot spot on the target of at least 12 px. The Void-Ray analogue's beam stays on while consecutive shots of one source hit one target (lifetime at least the period plus the random delay, or a held look keyed on source and target) |
| A1-6 | Projectiles that read at gameplay zoom. `acid_spit`: a dark wet glob of at least 8 px leading a short green trail, visibly not a tracer. `needle_volley`: two dark spines at least 3 px wide and 12 px long, with a pale edge so that they read on dark ground. `grenade_lob`: a dark shell of at least 6 px with a dull glow. `rocket_trail`: a smoke trail that reads on bright ground (a mid-dark grey, alpha at least 0.5) and a visible launch puff |
| A1-7 | Melee: `psi_slash` a swept crescent (wide in the middle, tapering to points, its arc growing across the target over 2 to 3 frames), blue on bright ground, eroding from its tail. `claw_rake` two or three dark-edged rakes that read on bright ground, and flecks that show. Neither may read as an even-width drawn arc |
| A1-8 | `glass_shatter`: shards visible on both grounds (at least 3 px, pale with a dark edge), thrown outward to about 1.5 bubble radii. State the `UnitRadius` bound in the evidence |
| E1-1 | Impact timing of instant shots (section 7) |
| E1-2 | Fog reach derived per effect; lateral cut; clip kept current or the leak recorded; two further fog takes |
| E1-3 | Pixel fog assertions for the Fungal enemy's missile and impact; fog cells marked in the evidence |
| E1-4 | Positive control and exact count in `status_gpu_tests` |
| E1-5 | Visibility measures (area and contrast) for every shipped `Shot` and `Point` look |
| E1-6 | Register the silent weapons and the missile's landing offset in `DESIGN.md` (with the half-tick extrapolation); update the stale presentation row and B-107 |
| E1-7 | Record the valgrind block counts; one-pass compaction of landed missiles |
| E1-8 | Count only combat effect cards in the capture's combat check |
| V1-1 | A staged battle at the default zoom, on the shipped floor, with each presentable family firing (Pulse Bot, Rhino Beetle, Welder Drone, Railgun Crawler, Shock Golem, Specter, Psi Grey, Vector Interceptor, Cloak Gunship): a 40 ms sequence over one exchange, at 1:1 (not half size) and at 2x. The lab families also at 1:1 next to a real bubble, so that size can be judged against a unit |

## Non-blocking polish and advisories

- **P1-1** `fungal_impact`'s spore puffs drift up-screen past the ring's far edge. Lower their lift, or place
  them in the near half, so that the mass sits over the disc. This is the WP10 cloud's issue.
- **P1-2** The hitscan slug ends at the target's centre, inside its bubble. Consider ending the slug, though not
  the impact, at the target's surface, so that the round strikes the glass.
- **P1-3** `wp11r1_battle_combat_lavapipe.jpg` shows two green crystal props with hard, diamond-shaped shadows
  near the fight. They are map props (they appear in the WP10 battle frames too), but next to small green acid
  hits they read as effects. Stage the evidence away from them.
- **P1-4** The coverage test's `delivery` re-implements the simulation's rule for which event a weapon publishes.
  A test that fires one weapon of each kind through `SimWorld` and compares the published event kinds would make
  the classification a fact rather than a copy.
