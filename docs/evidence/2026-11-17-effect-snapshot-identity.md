# Evidence: Effect Snapshot Identity (B-122 to B-126, B-128), WP8

Plan [`2026-11-17-effect-snapshot-identity`](../plans/2026-11-17-effect-snapshot-identity.md), WP8. This packet
closes the plan. It gathers the picture evidence the plan asks for (the cross-plane hit, the held beam and the
Fungal sequences at 1:1 and 2x, on radv and lavapipe, with their measurements) and the simulation throughput
comparison that Amendment 1 deferred to it.

The looks were reviewed and accepted on radv in the independent reviews
[`independent_review_identity_2.md`](effects/review/independent_review_identity_2.md) (WP5/WP7 round 1) and
[`independent_review_identity_3.md`](effects/review/independent_review_identity_3.md) (round 2, accepted once R3-1 to
R3-3 were fixed). The WP4/WP6 review is in the packet
[`independent_review_packet.md`](effects/review/independent_review_packet.md), sections "Identity WP4/WP6" and
"Identity WP5/WP7". WP8 adds the lavapipe runs, which is the driver CI uses.

## 1. How the lavapipe runs were made

- Tree: `main` at `8662f22b` (every WP of the plan merged), branch `effects-identity-wp8`, debug build.
- Driver: `SPACE_SCUFFLE_GPU_TEST_DRIVER=lavapipe`, so the harness (`crates/client/tests/support/gpu_run.rs`) runs
  each child on software Vulkan under Xvfb at 1280x720, with no GPU lock (charter §7). Each test binary ran alone
  under `flock ~/scratch/gate.lock`, with `--test-threads=1`.
- Tests, the same ones that made the radv evidence:
  - `weapon_battle_gpu_tests::a_held_beam_follows_its_target_and_ends_with_it` (WP7, B-126);
  - `area_link_gpu_tests::fungal_growth_flies_lands_and_afflicts` (WP5 and WP6, B-122 and B-124);
  - `weapon_battle_gpu_tests::weapon_looks_fire_in_a_staged_battle` (WP4 and WP4b, B-125 and B-128: every wave, with
    the two `cross_plane` waves).
- Pictures were copied out through `SPACE_SCUFFLE_WEAPON_BATTLE_EVIDENCE_DIR` and `SPACE_SCUFFLE_AREA_EVIDENCE_DIR`.
  The held-beam and Fungal sheets were assembled from the children's own frames with ImageMagick: crops at 1:1, and
  enlargements by pixel repetition (no filtering).
- No bar, threshold or staging was changed. All three tests passed on the first run.

## 2. Measurements against the bars

Every line is printed by the test itself. The full lavapipe output is in
[`identity_wp8_measurements_lavapipe.txt`](effects/review/identity_wp8_measurements_lavapipe.txt). The radv
figures are the latest accepted ones: review round 2 for WP5/WP7
([`identity_wp5_wp7_measurements.txt`](effects/review/identity_wp5_wp7_measurements.txt)) and WP4b
([`identity_wp4b_measurements.txt`](effects/review/identity_wp4b_measurements.txt)).

### 2.1 Held beam (B-126)

| Check | Bar | radv | lavapipe |
|---|---|---|---|
| Frames with the beam held during the stream | ≥ 30 | 40 | 40 |
| Beams held / live instances on every frame of the stream | 1 / 1 | 1 / 1 | 1 / 1 |
| Beam end against the drawn target's outline point, worst frame | ≤ 3 px | 0.00 px | 0.00 px |
| Distance the end travelled | > 20 px | 163.5 px | 163.5 px |
| Youngest burn's centre against the beam's end, worst frame (R3-1) | ≤ 3 px, ≥ 20 frames with a burn | 0.00 px, 40 frames | 0.00 px, 40 frames |
| Mid-stream pair: changed pixels along the path / within 3 px of its end | both drawn | 656 / 20 | 656 / 20 |
| After the death: live instances after the exit plus a frame; changed pixels on the former path | none; 0 | none; 0 | none; 0 |
| Strikes in the 1:1 sequence (burn age 0.00) | — | frames 15 and 23 | frames 15 and 23 |

### 2.2 Fungal Growth (B-122, B-124)

| Check | Bar | radv | lavapipe |
|---|---|---|---|
| Impact laid against the aimed point (world units) | 0 | 0.00000 | 0.00000 |
| Impact's `Critical` ring centred on the projected aimed point | ≤ 2 px | 0.94 px | 0.90 px |
| Fog edge (E1-3): unseen ground samples changed | 0 | 0 of 357 | 0 of 357 |
| Fog edge: seen ground samples changed | the area is drawn | 488 of 619 | 486 of 619 |
| Fungal victims wearing the named look on every frame from 0.4 s after the status | 4 of 4 | 4 of 4 (62 of 62 frames each) | 4 of 4 (62 of 62 frames each) |
| Their rim strength | ≥ 0.2, rim colour ±0.02 | 0.30 to 0.80 | 0.30 to 0.80 |
| Storm stand-in victims (status borne on 41 frames each): named look, rim | none, 0 | none, 0 | none, 0 |
| Named status looks held (R3-3) | ≥ 1 per victim | 4 | 4 |

### 2.3 Cross-plane hits and projectiles (B-125, B-128)

| Subject | Bar | radv | lavapipe |
|---|---|---|---|
| `cross_plane` instant hit (`gauss_tracer`+`bullet_hit`, wave 8): impact beside the flyer | ≥ 2 × blank + 20 | 272 px, blank 25 (needs 70) | 274 px, blank 27 (needs 74) |
| The same: change beside the 3 ground units under the flyer | ≤ blank + 8 | 7 px, blank 7 (allowed 15) | 9 px, blank 9 (allowed 17) |
| `cross_plane` projectile (`rocket_trail`+`rocket_hit`, wave 9): impact beside the flyer | ≥ 2 × blank + 20 | 401 px, blank 1 (needs 22) | 402 px, blank 1 (needs 22) |
| The same: change beside the 3 ground units | ≤ blank + 8 | 1 px, blank 1 (allowed 9) | 1 px, blank 1 (allowed 9) |
| `needle_volley`+`needle_hit` (wave 4), the subject with no margin | ≥ 2 × blank + 20 | 34 px, blank 7 (needs 34) | 34 px, blank 7 (needs 34) |

The other 20 subjects of the staged battle pass with room on lavapipe, within a few pixels of radv. The largest
differences are the held-beam looks lent to a hitscan shooter (`prism_beam`+`beam_burn` 106 px, needs 46, against
radv's 86 at WP4b and 94 at WP7; `lance_beam`+`lance_burn` 143, needs 42, against 115 and 136; the burns gained
their strike flare in review round 1, after both radv runs) and `cannon_tracer`+`plasma_hit` (165 against 172,
needs 30). The test passed in 872 s (10 waves).

**Result: every bar passes on lavapipe.** The two drivers agree to the pixel on the held beam's geometry, the
burns and the victims' looks. The rendered counts differ by at most a few pixels, as rasterisation and blending
differ between the two drivers.

## 3. Sheets

| File | What it shows |
|---|---|
| [`identity_wp8_cross_plane_lavapipe.png`](effects/review/identity_wp8_cross_plane_lavapipe.png) | lavapipe counterpart of [`identity_wp4_cross_plane_radv.png`](effects/review/identity_wp4_cross_plane_radv.png): an anti-air shooter fires at a flyer over three packed ground units. Top: 12 frames at 40 ms, 1:1. Bottom: the first 6 at 2x. The hit lands on the flyer's glass, and nothing lands on the units under it |
| [`identity_wp8_cross_plane_projectile_lavapipe.png`](effects/review/identity_wp8_cross_plane_projectile_lavapipe.png) | lavapipe counterpart of [`identity_wp4b_cross_plane_projectile_radv.png`](effects/review/identity_wp4b_cross_plane_projectile_radv.png): the projectile wave, with the same layout |
| [`identity_wp8_needle_volley_lavapipe.png`](effects/review/identity_wp8_needle_volley_lavapipe.png) | lavapipe counterpart of [`identity_wp4b_needle_volley_radv.png`](effects/review/identity_wp4b_needle_volley_radv.png) |
| [`identity_wp8_held_beam_lavapipe.png`](effects/review/identity_wp8_held_beam_lavapipe.png) | lavapipe counterpart of [`identity_wp7_held_beam_radv.png`](effects/review/identity_wp7_held_beam_radv.png), from the round 2 staging (Void-Ray cadence). Top, 1:1: mid-stream drawn, the same instant bare, after the target's death drawn, and bare. Bottom: the mid-stream pair at 2x |
| [`identity_wp8_beam_burn_follows_lavapipe.png`](effects/review/identity_wp8_beam_burn_follows_lavapipe.png) | lavapipe counterpart of [`identity_wp7_beam_burn_follows_radv.png`](effects/review/identity_wp7_beam_burn_follows_radv.png). Top: frames 13 to 24 round the beam's end at 1:1. Bottom: frames 14, 15 (a strike), 16 and 22 at 2x (radv's sheet shows them at 3x) |
| [`identity_wp8_fungal_lavapipe.png`](effects/review/identity_wp8_fungal_lavapipe.png) | lavapipe counterpart of [`identity_wp5_fungal_own_radv.png`](effects/review/identity_wp5_fungal_own_radv.png) and [`identity_wp5_fungal_late_radv.png`](effects/review/identity_wp5_fungal_late_radv.png). Rows 1 to 3: 18 consecutive 40 ms frames at 1:1 from the impact's last ring on. Row 4: the four victims of the second row at 2x. Row 5: six late frames at 1:1, after the stain has faded, with only the victims' rims left. Row 6: those victims at 2x |
| [`identity_wp8_fungal_impact_lavapipe.png`](effects/review/identity_wp8_fungal_impact_lavapipe.png) | lavapipe counterpart of [`identity_wp6_fungal_impact_radv.png`](effects/review/identity_wp6_fungal_impact_radv.png): the impact's `Critical` ring 0.20 s after it was laid, at 1:1 and 2x, round the aimed point |

**No new review round.** The lavapipe sheets show the same looks, frame for frame, as the radv sheets the reviews
accepted. The beam's geometry and the burns' positions match radv to 0.00 px. The Fungal victims' rims fall in the
same strength band, and the fog edge changes no unseen sample. Nothing in them is a defect that radv could have
hidden, so under Q0080 no further round is opened.

## 4. Simulation throughput (Amendment 1, deferred to WP8)

Amendment 1 measured the writer at WP3 on a loaded server, where the figures could not show "within noise". Here the
identity simulation merge `c4bf2f16` (WP1 to WP4, WP4b and WP6) is compared with its first parent `bf4a7d2d`. That
merge holds every simulation change of the plan; WP5 and WP7 changed the client only. Both trees were exported with
`git archive` and built in release with the same toolchain (1.100.0). `cargo bench -p sim_core --bench throughput
--locked` ran five times on each, alternated before/after, each run under the gate lock, on 2026-11-18 between
05:44 and 06:09 UTC. Each run prints the median of three repeats; the table gives the median of the five runs, in
ticks per second.

| Map | Units | Before (`bf4a7d2d`) | After (`c4bf2f16`) | Change |
|---|---:|---:|---:|---:|
| 64x64 | 0 | 3264 | 3282 | +0.6 % |
| 64x64 | 100 | 950 | 947 | -0.3 % |
| 64x64 | 400 | 163 | 168 | +3.1 % |
| 128x128 | 400 | 191 | 198 | +3.7 % |
| 256x256 | 0 | 204 | 204 | 0 % |
| showcase_canyon | 400 | 119 | 122 | +2.5 % |
| 64x64, 67 casters | 400 | 236 | 239 | +1.3 % |

The single runs, before: 3257/951/164/186/204/121/236, 3191/931/159/191/202/119/238, 3264/950/163/191/202/116/232,
3290/966/165/195/206/121/236, 3269/915/163/194/207/119/238. After: 2401/486/112/111/172/112/216 (the first after-run
coincided with a load average of 107 from another user's test run), 3282/947/168/199/204/122/201,
3282/964/170/199/204/117/239, 3286/950/169/198/205/122/241, 3274/946/167/198/205/124/244. The load average was 5 to 30
for the other runs.

Every row is within noise of the tree before the plan, including the caster battle, which publishes about 100
statuses a tick through the new `publish_statuses` pass. The release benchmark's floor (25 ticks/s) is far below
every row.
