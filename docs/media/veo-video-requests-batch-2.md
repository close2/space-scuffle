# Veo video requests, batch 2: the Cosmic Enclave and the Sylvan Swarm

- **Written:** 2026-11-28, on the branch `veo-media-requests`, after the first request document
  ([`veo-video-requests.md`](veo-video-requests.md), "batch 1")
- **For:** the same outside agent, which generates with its own free tokens and hands in on the branch
  `veo-media-requests`
- **Reviewed and installed by:** this project's orchestrator, after hand-in (§9)
- **Machine-readable twin:** [`veo-video-requests-batch-2.json`](veo-video-requests-batch-2.json), generated from
  the same dry run as this document. It holds every request below (ID, identity, state, picture, install name, prompt,
  negative prompt, `prompt_sha256`, priority, attempt cap). Where the two differ, this document wins; tell the
  orchestrator.

This document asks for the start pictures and clips of every catalog identity that still has no real media: 56
identities, 56 start pictures and 133 clips. A lower-priority polish pass (§8) redoes the imperfect
loops of four shipped identities. It is self-contained, except that it reuses three sections of batch 1 by reference:
the request parameters (batch 1 §2), the hand-in format (batch 1 §4) and the acceptance criteria (batch 1 §5). Read
those three sections first; this document says only where batch 2 adds to them.

Batch 1's two clips (`brood_mite_escort` Idle and `colossus_mech` Walk) are not repeated here. `colossus_mech` Walk is
still batch 1 clip 2.

---

## 1. Ground rules

Batch 1 §1 applies unchanged: real generated media only, regenerate rather than fake, stop and say so when blocked,
free to this project, touch only `docs/media/veo-handin/`, no secrets. Batch 2 adds one rule for pictures:

- **Start pictures are generated too.** The owner asked on 2026-11-28 that every missing start picture be generated
  as part of this request ("if keyframes are missing, then add them to the todo list as well (the generation should
  be free as well)"). A picture is the untouched output of an image model, or frame 0 of a real Veo clip (§4.2). It is
  never drawn, painted over, composited, retouched or colour-corrected. The only permitted change is the scale and
  centre crop to exactly 1280×720 that §4.3 describes, recorded in the hand-in.

---

## 2. Request parameters

Batch 1 §2 applies to every clip unchanged: `veo-3.1-fast-generate-001` (Veo 3.1 standard is allowed for a clip that
keeps failing on Fast; record it), 16:9, 720p, 4 seconds, one sample per request, sound on, a new seed per attempt,
the clip's own negative prompt as `negativePrompt`, and the prompt sent character for character.

**Start and end picture.** Every clip of an identity starts on that identity's pin, the picture
`<slug>_idle.png` of §4, sent as `image`. The clip's last frame is pinned to the same picture (`lastFrame`) for every
state of this batch: the project's loop policy (`assets/catalog/media_style.ron`, `loops`) pins Idle, Walk, Cast and
Constructing at both ends, and batch 2 pins every Attack at its end as well, so that the switch back to Idle does not
snap (`loops.overrides` in `media_style.ron`). So every request of this batch sends the pin as both `image` and `lastFrame`.

---

## 3. What is requested, and in what order

### 3.1 Summary

| | Enclave | Swarm | Total |
|---|---|---|---|
| Start pictures (§4) | 27 | 29 | 56 |
| Clips (§5) | 65 | 68 | 133 |

Clips by state: Idle 56, Walk 24, Attack 19, Cast 2,
Constructing 32. The polish pass (§8) adds 3 pictures and 11 clips.

| Priority | What | Pictures | Clips |
|---|---|---|---|
| P1 | What a player needs to field either faction at all: both workers (`grav_harvester`, `nectar_worker`), both town halls (`citadel_nexus_dome`, `ancient_tree_dome`), supply (`psi_pylon_dome`, `amber_pollen_dome`), gas (`rift_siphon_dome`, `spore_vent_dome`), the first production structure (`warp_gate_dome`, `spawning_hollow_dome`) and the first army unit (`warp_strider`, `razor_mantis`) | 12 | 30 |
| P2 | Every other mobile unit of both factions, in tech order | 19 | 53 |
| P3 | Every other structure of both factions, in tech order | 25 | 50 |
| P4 | The polish pass (§8) | 3 | 11 |

A unit without media is never shown in a battle (`DESIGN.md` §4, the "Media coverage" row). So mobile units come
before the rest of the structures.

### 3.2 Order of work

Finish one priority before starting the next. Within a priority, work in three rounds:

1. **Pictures.** Make the pin of every identity of the priority (§4). A clip cannot start before its pin exists.
2. **Idle clips.** Generate every Idle of the priority on its pin. The Idle is the test of the pin: a pin on which no
   Idle passes within its attempts is replaced (§6.3) before anything else is generated on it.
3. **The other states** (Walk, Attack, Cast, Constructing), each on the same pin, once its identity's Idle passed.

Within a round, follow the order of the tables in §4 and §5.

**Why this order.** The baker plans an identity's states Idle first (`asset_baker/src/veo_plan.rs`), and every other
state starts on the identity's one picture: the authored `<slug>_idle.png` when it passes the pin rules, otherwise
frame 0 of the Idle clip (loops plan D-3, `resolve_keyframe`). With the pins of §4 in place, every state of this batch
starts on the pin itself, so no state is chained from another clip. `keyframe-from-clip` is needed only for the
fallback picture of §4.2 (frame 0 of a seed clip). It would also make the picture of a catalog mode (the enter morph's
last frame becomes `<slug>-<mode>.png`), but no mode is requested in this batch (§10).

---

## 4. Start pictures (pins)

### 4.1 Why they are needed

The audit found that none of the 56 identities has a usable start picture in `assets/ai_keyframes/`. 54 have none at
all. Two, `flying_saucer_idle.jpg` and `tesla_sentinel_dome_idle.jpg`, were square 1024×1024 legacy JPEGs. A pin must
be PNG or JPEG, 16:9 within 1 %, at least 1280×720 and at most 20 MB (`check_pin`, loops plan D-2), so a square
picture cannot pin a loop. Both JPEGs are removed in this commit, as batches 2 and 3 retired the square pins of their
identities. The baker would otherwise have used them as the Idle's start picture and refused the run.

Every existing pin in `assets/ai_keyframes/*.png` is frame 0 of a Veo text-to-video seed clip, extracted with
`asset_baker keyframe-from-clip --at 0` (batch evidence in `docs/evidence/veo/`, for example
[batch 6](../evidence/veo/2026-11-09-batch-6.md), "Pin 2"). No image model has made a pin yet. The project uses an
image model only for effect textures (`asset_baker effect-image`, models `gemini-3.1-flash-image` and
`gemini-nano-banana-2.1` in `assets/media/veo_budget.ron`).

### 4.2 Method

- **Primary: an image model.** Generate the picture from the image prompt of Appendix A. Use
  `gemini-3.1-flash-image`, the project's own image model, if you can. Any other image model you have free access to is
  allowed. Record the exact model id in the hand-in. If the model takes an aspect ratio, ask for 16:9. If it takes a
  negative prompt, send the identity's clip negative prompt (Appendix B). Up to **6** image attempts per pin.
- **Fallback: a Veo seed.** If no image attempt passes §4.4, make one text-to-video Veo request with the identity's
  **Idle clip prompt** from Appendix B and no start picture. Use the parameters of batch 1 §2, without `image` and
  without `lastFrame`. Its frame 0 is the pin, extracted untouched as a 1280×720 PNG
  (`asset_baker keyframe-from-clip --clip <seed>.mp4 --at 0 --output <slug>_idle.png`, or
  `ffmpeg -i <seed>.mp4 -frames:v 1 <slug>_idle.png`). This is how every existing pin was made. Up to **4** seeds per
  pin. The seed clip is a picture source, not a loop: hand it in with the pin (§7), never as the Idle clip. A seed
  whose frame 0 is clean but whose later frames push in or draw rays is still a good pin; the earlier batches kept
  many such frames.
- **When both fail,** hand in the best picture as `best_effort` with notes, and generate no clips on it. The
  orchestrator decides.

The dry run before pins exist reports the Idle as `start: none (text to video)` with the note
`loop: no start picture yet, this clip seeds one`, and every other state as `first frame (0 s) of the Idle clip`.
That is the "pin missing" state. `validate-prompts` and `generate-veo --dry-run` still pass, because a missing pin is
not a problem: the baker would plan the Idle as the seed. The pins are never faked to make a check pass. Once a pin is
installed, the same dry run shows `start: assets/ai_keyframes/<slug>_idle.png` and `end: same as start` for every
state. The prompt texts and hashes do not change.

### 4.3 Specification of every picture

- **File:** PNG, exactly 1280×720 pixels, at most 20 MB, named `<slug>_idle.png`. That is the name the baker looks
  for (`authored_keyframe` in `veo_plan.rs`: `<slug>_idle.jpg`, then `<slug>_idle.png`).
- **Install name:** `assets/ai_keyframes/<slug>_idle.png`. **Hand-in path:**
  `docs/media/veo-handin/<slug>/pin/<slug>_idle.png`.
- **Size from an image model.** If the model returns another size, the only permitted change is a uniform scale so
  that the picture covers 1280×720, then a centred crop to exactly 1280×720. No other edit is permitted. Record the
  original file's SHA-256 and size and the exact command in the hand-in, and commit the original next to the pin as
  `<slug>_idle.original.<ext>`. A picture whose aspect ratio is far from 16:9, so that the crop would cut the
  subject or its margin, is regenerated instead.
- **Backdrop:** the house backdrop, one flat even dark slate-indigo (#0B101B) from edge to edge, with no floor,
  ground plane, horizon, cast shadow, light shafts, rays, stripes, gradient, vignette or pattern. The clips are
  letterboxed with bars measured from the clip's own border, so the backdrop must be even.
- **View:** a 3/4 isometric view from above, the subject's front toward the lower right of the frame. Flying
  identities are seen from above, looking down on their top. Structures show their front and one side.
- **Framing:** the whole subject, centred, with a clear margin on every side; nothing of it touches an edge. Match
  the scale of the faction's existing pins: the subject spans roughly 55 to 70 % of the frame width and at most about
  80 % of its height. Examples: `mothership_carrier_idle.png` and `saucer_interceptor_idle.png` (Enclave aircraft),
  `spore_matriarch_idle.png`, `burrow_scorpion_idle.png` and `brood_mite_idle.png` (Swarm creatures), and
  `plasma_siphon_dome_idle.png` (a structure).
- **Pose:** the pose the Idle prompt describes, which every clip of the identity starts and ends on: standing still,
  crouched, held still in the air with wings spread, and so on. Weapons, claws and legs are where the text puts them.
- **Style:** the house style sentence (the image prompt ends with it): stylized, hand-painted 3D, painterly, crisp rim
  lighting, glowing cyan and amber energy accents, bold proportions.
- **Faction look.** Enclave: polished silver and gold alien alloy, violet and cyan crystal light, Grey aliens with
  large bald heads and big eyes (as the legacy `psi_grey`, the Carrier and the interceptor). Swarm: dark green and
  brown chitin, bark and leathery hide, glowing amber bands, pods and sacs (as the scorpion, the Brood Lord analogue
  and the Broodling).
- **Original designs.** No likeness of any existing game's unit or building. The prompts name none, and the picture
  must not look like a copy of one.

### 4.4 Acceptance of a picture

A picture is proposed only if it meets all of these:

1. **Real.** The untouched output of the image model or Veo frame 0, apart from the scale and crop of §4.3.
2. **One subject, whole, centred, with a margin,** as §4.3. No second creature, unit or building, no text, logo or
   watermark.
3. **Backdrop and view** as §4.3. No floor, shadow, rays or gradient anywhere.
4. **Matches its text.** Every part the Idle prompt names is there, in its pose. Nothing it rules out (for example
   turning rings or a tall mast) is there.
5. **Readable silhouette at game zoom.** Scaled down to 128 pixels wide, the identity is still recognisable and
   distinct from its faction's other identities.
6. **Consistent with its faction** (§4.3).

### 4.5 The pins

| ID | Identity | Faction | Kind | Install name | Clips that start on it | Priority |
|---|---|---|---|---|---|---|
| P-01 | `grav_harvester` | Enclave | aircraft | `grav_harvester_idle.png` | C-001, C-002, C-003, C-004 | P1 |
| P-02 | `nectar_worker` | Swarm | creature | `nectar_worker_idle.png` | C-005, C-006, C-007, C-008 | P1 |
| P-03 | `citadel_nexus_dome` | Enclave | structure | `citadel_nexus_dome_idle.png` | C-009, C-010 | P1 |
| P-04 | `ancient_tree_dome` | Swarm | structure | `ancient_tree_dome_idle.png` | C-011, C-012 | P1 |
| P-05 | `psi_pylon_dome` | Enclave | structure | `psi_pylon_dome_idle.png` | C-013, C-014 | P1 |
| P-06 | `amber_pollen_dome` | Swarm | flyer | `amber_pollen_dome_idle.png` | C-015, C-016 | P1 |
| P-07 | `rift_siphon_dome` | Enclave | structure | `rift_siphon_dome_idle.png` | C-017, C-018 | P1 |
| P-08 | `spore_vent_dome` | Swarm | structure | `spore_vent_dome_idle.png` | C-019, C-020 | P1 |
| P-09 | `warp_gate_dome` | Enclave | structure | `warp_gate_dome_idle.png` | C-021, C-022 | P1 |
| P-10 | `spawning_hollow_dome` | Swarm | structure | `spawning_hollow_dome_idle.png` | C-023, C-024 | P1 |
| P-11 | `warp_strider` | Enclave | walker | `warp_strider_idle.png` | C-025, C-026, C-027 | P1 |
| P-12 | `razor_mantis` | Swarm | creature | `razor_mantis_idle.png` | C-028, C-029, C-030 | P1 |
| P-13 | `astral_vagrant` | Enclave | infantry | `astral_vagrant_idle.png` | C-031, C-032, C-033 | P2 |
| P-14 | `prism_weaver` | Enclave | aircraft | `prism_weaver_idle.png` | C-034, C-035, C-036 | P2 |
| P-15 | `stealth_eye_orb` | Enclave | aircraft | `stealth_eye_orb_idle.png` | C-037, C-038 | P2 |
| P-16 | `phase_prism_saucer` | Enclave | aircraft | `phase_prism_saucer_idle.png` | C-039, C-040 | P2 |
| P-17 | `void_goliath` | Enclave | walker | `void_goliath_idle.png` | C-041, C-042, C-043 | P2 |
| P-18 | `flying_saucer` | Enclave | aircraft | `flying_saucer_idle.png` | C-044, C-045, C-046 | P2 |
| P-19 | `prismatic_saucer` | Enclave | aircraft | `prismatic_saucer_idle.png` | C-047, C-048, C-049 | P2 |
| P-20 | `shadow_visitor` | Enclave | infantry | `shadow_visitor_idle.png` | C-050, C-051, C-052 | P2 |
| P-21 | `cerebral_elder` | Enclave | infantry | `cerebral_elder_idle.png` | C-053, C-054, C-055 | P2 |
| P-22 | `tripod_annihilator` | Enclave | walker | `tripod_annihilator_idle.png` | C-056, C-057, C-058 | P2 |
| P-23 | `volatile_tick` | Swarm | creature | `volatile_tick_idle.png` | C-059, C-060, C-061 | P2 |
| P-24 | `thorn_viper` | Swarm | creature | `thorn_viper_idle.png` | C-062, C-063, C-064 | P2 |
| P-25 | `mortar_stag` | Swarm | creature | `mortar_stag_idle.png` | C-065, C-066, C-067 | P2 |
| P-26 | `hive_matron` | Swarm | creature | `hive_matron_idle.png` | C-068, C-069, C-070 | P2 |
| P-27 | `sonic_bat` | Swarm | flyer | `sonic_bat_idle.png` | C-071, C-072, C-073 | P2 |
| P-28 | `sky_leviathan` | Swarm | flyer | `sky_leviathan_idle.png` | C-074, C-075, C-076 | P2 |
| P-29 | `mycelium_shaman` | Swarm | creature | `mycelium_shaman_idle.png` | C-077, C-078 | P2 |
| P-30 | `tongue_chameleon` | Swarm | flyer | `tongue_chameleon_idle.png` | C-079, C-080 | P2 |
| P-31 | `stag_behemoth` | Swarm | creature | `stag_behemoth_idle.png` | C-081, C-082, C-083 | P2 |
| P-32 | `cybernetics_conduit_dome` | Enclave | structure | `cybernetics_conduit_dome_idle.png` | C-084, C-085 | P3 |
| P-33 | `crystalline_forge_dome` | Enclave | structure | `crystalline_forge_dome_idle.png` | C-086, C-087 | P3 |
| P-34 | `tesla_sentinel_dome` | Enclave | structure | `tesla_sentinel_dome_idle.png` | C-088, C-089 | P3 |
| P-35 | `harmonic_recharge_dome` | Enclave | structure | `harmonic_recharge_dome_idle.png` | C-090, C-091 | P3 |
| P-36 | `templar_conclave_dome` | Enclave | structure | `templar_conclave_dome_idle.png` | C-092, C-093 | P3 |
| P-37 | `robotic_foundry_dome` | Enclave | structure | `robotic_foundry_dome_idle.png` | C-094, C-095 | P3 |
| P-38 | `stellar_portal_dome` | Enclave | structure | `stellar_portal_dome_idle.png` | C-096, C-097 | P3 |
| P-39 | `psionic_archives_dome` | Enclave | structure | `psionic_archives_dome_idle.png` | C-098, C-099 | P3 |
| P-40 | `void_sanctuary_dome` | Enclave | structure | `void_sanctuary_dome_idle.png` | C-100, C-101 | P3 |
| P-41 | `robotic_assembly_bay_dome` | Enclave | structure | `robotic_assembly_bay_dome_idle.png` | C-102, C-103 | P3 |
| P-42 | `celestial_beacon_dome` | Enclave | structure | `celestial_beacon_dome_idle.png` | C-104, C-105 | P3 |
| P-43 | `canopy_grove_dome` | Swarm | structure | `canopy_grove_dome_idle.png` | C-106, C-107 | P3 |
| P-44 | `primal_world_tree_dome` | Swarm | structure | `primal_world_tree_dome_idle.png` | C-108, C-109 | P3 |
| P-45 | `metamorphic_chamber_dome` | Swarm | structure | `metamorphic_chamber_dome_idle.png` | C-110, C-111 | P3 |
| P-46 | `thorn_lasher_dome` | Swarm | structure | `thorn_lasher_dome_idle.png` | C-112, C-113 | P3 |
| P-47 | `spore_spire_dome` | Swarm | structure | `spore_spire_dome_idle.png` | C-114, C-115 | P3 |
| P-48 | `carapace_mound_dome` | Swarm | structure | `carapace_mound_dome_idle.png` | C-116, C-117 | P3 |
| P-49 | `caustic_nursery_dome` | Swarm | structure | `caustic_nursery_dome_idle.png` | C-118, C-119 | P3 |
| P-50 | `spine_den_dome` | Swarm | structure | `spine_den_dome_idle.png` | C-120, C-121 | P3 |
| P-51 | `burrow_spire_dome` | Swarm | structure | `burrow_spire_dome_idle.png` | C-122, C-123 | P3 |
| P-52 | `chitin_spire_dome` | Swarm | structure | `chitin_spire_dome_idle.png` | C-124, C-125 | P3 |
| P-53 | `apex_spire_dome` | Swarm | structure | `apex_spire_dome_idle.png` | C-126, C-127 | P3 |
| P-54 | `infestation_nursery_dome` | Swarm | structure | `infestation_nursery_dome_idle.png` | C-128, C-129 | P3 |
| P-55 | `subterranean_network_dome` | Swarm | structure | `subterranean_network_dome_idle.png` | C-130, C-131 | P3 |
| P-56 | `behemoth_den_dome` | Swarm | structure | `behemoth_den_dome_idle.png` | C-132, C-133 | P3 |

Kinds: `aircraft` (saucers and drones, seen from above), `flyer` (winged and floating creatures), `walker` (alien
legged machines), `infantry` (Grey aliens), `creature` (Swarm ground creatures), `structure`.

The exact image prompt of every pin is in Appendix A. It is the identity's Idle text from the catalog
(`assets/catalog/units.ron` or `structures.ron`, `veo_prompts`), then this sentence, then the house style:

```text
One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts.
```

---

## 5. The clips

Every clip below was planned by the free dry run on this commit:

```text
asset_baker generate-veo --unit <each of the 56 identities> --dry-run \
  --ledger docs/evidence/veo/ledger.jsonl --clips-dir ~/scratch/veo/clips
```

It printed `Clips to generate: 134` (the 133 below plus batch 1's `colossus_mech` Walk), and
`asset_baker validate-prompts` printed `Prompts valid for 134 clip(s) to generate.` `prompt_sha256` is the SHA-256 of the
prompt's UTF-8 text, as in batch 1. The exact prompt and negative prompt of every clip are in Appendix B, grouped by
identity.

**Install name** is the file name under `assets/videos/`. The hand-in folder is
`docs/media/veo-handin/<slug>/<state>/` (§7).

| ID | Identity | Faction | State | Start and end picture | Install name | `prompt_sha256` | Priority | Attempts |
|---|---|---|---|---|---|---|---|---|
| C-001 | `grav_harvester` | Enclave | Idle | pin P-01 | `grav_harvester_idle.mp4` | `11f36182c166dab12dea2e97578d9ec3c899dcf14fd84e79b9d56de0fe28146b` | P1 | 8 |
| C-002 | `grav_harvester` | Enclave | Walk | pin P-01 | `grav_harvester_walk.mp4` | `4b9c13da6e180a03159b9257aa3fd3c96e837000423d011a93f165df7a66d94b` | P1 | 8 |
| C-003 | `grav_harvester` | Enclave | Attack | pin P-01 | `grav_harvester_attack.mp4` | `0026e2f8f44ea6c647ebb61ba4f0698d0984b0963ba2cd98c28eb851912d166a` | P1 | 8 |
| C-004 | `grav_harvester` | Enclave | Cast | pin P-01 | `grav_harvester_cast.mp4` | `f86dce71b60990c25fbdea7cf2f5d99e9af3ee0a8b8a645980d5fa4d5c02d82c` | P1 | 8 |
| C-005 | `nectar_worker` | Swarm | Idle | pin P-02 | `nectar_worker_idle.mp4` | `e152607d1be563327a10652c1cadb1c7d453edac88c591dc4e2a08492eec246e` | P1 | 8 |
| C-006 | `nectar_worker` | Swarm | Walk | pin P-02 | `nectar_worker_walk.mp4` | `a091e9dae87c4c11d73367e4f1a0b33ae7a6d523f8e7951453bdb4816a32022f` | P1 | 12 |
| C-007 | `nectar_worker` | Swarm | Attack | pin P-02 | `nectar_worker_attack.mp4` | `502ceb91dc9d00d8f5dc700ce266f082b67529048120c8b16c0cd4b1520484fb` | P1 | 8 |
| C-008 | `nectar_worker` | Swarm | Cast | pin P-02 | `nectar_worker_cast.mp4` | `a5c90ff56de77cafb533444b8b8ef113f872cf542ff001162420c8c8b6b823ef` | P1 | 8 |
| C-009 | `citadel_nexus_dome` | Enclave | Idle | pin P-03 | `citadel_nexus_dome_idle.mp4` | `e6f88f144b35c82adc9eed2b0da244f04f53935a53ac1885a9854a8d8be0fde9` | P1 | 8 |
| C-010 | `citadel_nexus_dome` | Enclave | Constructing | pin P-03 | `citadel_nexus_dome_constructing.mp4` | `6ccd148c1242ea4e92b7f9547e5dcb2f4c12e7054e66f528e77849dc31f2ba92` | P1 | 8 |
| C-011 | `ancient_tree_dome` | Swarm | Idle | pin P-04 | `ancient_tree_dome_idle.mp4` | `cd6c4d04e59857108120567d98f8094257c2dca86dc99d69e3e907fba2c1f101` | P1 | 8 |
| C-012 | `ancient_tree_dome` | Swarm | Constructing | pin P-04 | `ancient_tree_dome_constructing.mp4` | `ea79dba63586b189b921821aac8b12f10c29fe1f2b60a974f6d81285870b6c1b` | P1 | 8 |
| C-013 | `psi_pylon_dome` | Enclave | Idle | pin P-05 | `psi_pylon_dome_idle.mp4` | `b3078f250291ee52c9e82407f415b404059f75819b3f195cdb3d5d0b5f51704f` | P1 | 8 |
| C-014 | `psi_pylon_dome` | Enclave | Constructing | pin P-05 | `psi_pylon_dome_constructing.mp4` | `c8807e75171e54f2d08288b3b47eff16f9edc3f0826cf5941e5606f0e2c1959a` | P1 | 8 |
| C-015 | `amber_pollen_dome` | Swarm | Idle | pin P-06 | `amber_pollen_dome_idle.mp4` | `dc7cc62e285b4d58d5f737a73e295f01279b565f0ceed3c5ec97e51f3667d04f` | P1 | 8 |
| C-016 | `amber_pollen_dome` | Swarm | Walk | pin P-06 | `amber_pollen_dome_walk.mp4` | `86059d7de83045c149ac4c34ef836bcb5712c90fd7a540d514f114c32be37758` | P1 | 8 |
| C-017 | `rift_siphon_dome` | Enclave | Idle | pin P-07 | `rift_siphon_dome_idle.mp4` | `6396f15e42f635be487b3f8eba146be9f9d9676ca1b407c379e3ac78d0168d7d` | P1 | 8 |
| C-018 | `rift_siphon_dome` | Enclave | Constructing | pin P-07 | `rift_siphon_dome_constructing.mp4` | `b7373cd1b37f153940be069041d8b6a7f28ae5e36b671ef2d5742128279c9de9` | P1 | 8 |
| C-019 | `spore_vent_dome` | Swarm | Idle | pin P-08 | `spore_vent_dome_idle.mp4` | `0b0f5a286b8e0b1e3eb46f11d8abcef2c28984c2039a5a3f720137d8ee20cd21` | P1 | 8 |
| C-020 | `spore_vent_dome` | Swarm | Constructing | pin P-08 | `spore_vent_dome_constructing.mp4` | `9472857cc1bb566f3c1b0a2192535d25a682d713d8e64e07b3bb137315205d4d` | P1 | 8 |
| C-021 | `warp_gate_dome` | Enclave | Idle | pin P-09 | `warp_gate_dome_idle.mp4` | `d73184f75d59a0c52481b6aaf270fd7484a97316b8b0044bc3cde479da6e4e10` | P1 | 8 |
| C-022 | `warp_gate_dome` | Enclave | Constructing | pin P-09 | `warp_gate_dome_constructing.mp4` | `a5c02fa072ced86a355cfb9444102d247a85c393cdbd26c5dbe8a29fd5106fce` | P1 | 8 |
| C-023 | `spawning_hollow_dome` | Swarm | Idle | pin P-10 | `spawning_hollow_dome_idle.mp4` | `f978bb22a379ca6da5b6867853671fda8ab56f76374035495381f510df161a28` | P1 | 8 |
| C-024 | `spawning_hollow_dome` | Swarm | Constructing | pin P-10 | `spawning_hollow_dome_constructing.mp4` | `4282ba987a032b1100c41fa313d2a8caa2698b655393e4f097839ddb687d20e7` | P1 | 8 |
| C-025 | `warp_strider` | Enclave | Idle | pin P-11 | `warp_strider_idle.mp4` | `b905259399252816a114cb6f02827ae446172dd3cb9bcb97feab7051d58c3428` | P1 | 8 |
| C-026 | `warp_strider` | Enclave | Walk | pin P-11 | `warp_strider_walk.mp4` | `ba8b6ee78f385011f820faa07edef037608c9296dfaf85d20f394976badc2597` | P1 | 12 |
| C-027 | `warp_strider` | Enclave | Attack | pin P-11 | `warp_strider_attack.mp4` | `547d3eb4d585eee2444606ed4691048e04669da5c08e9ed6e24ce5ab28b7f62c` | P1 | 8 |
| C-028 | `razor_mantis` | Swarm | Idle | pin P-12 | `razor_mantis_idle.mp4` | `092ed7b828324f7f856e64eb0e21a10a81bb982e4dd5c9d0a6403e521d69412d` | P1 | 8 |
| C-029 | `razor_mantis` | Swarm | Walk | pin P-12 | `razor_mantis_walk.mp4` | `1c66d2edb70994f27cb738188bd8bfc80580d35319b6b66d683953b7cabd302f` | P1 | 12 |
| C-030 | `razor_mantis` | Swarm | Attack | pin P-12 | `razor_mantis_attack.mp4` | `583522e380fed19e6fc14fe3bf89d72b6303413f11798a225edaf08a6ccb6fe7` | P1 | 8 |
| C-031 | `astral_vagrant` | Enclave | Idle | pin P-13 | `astral_vagrant_idle.mp4` | `56ec5e53ab8932ac293b281fee801a433beafcbfc2b317f5be200a5f7b25c9b9` | P2 | 8 |
| C-032 | `astral_vagrant` | Enclave | Walk | pin P-13 | `astral_vagrant_walk.mp4` | `25514a09426cecca1dc571dcadea8aec9a4139f848630432402a526cf46c6de9` | P2 | 12 |
| C-033 | `astral_vagrant` | Enclave | Attack | pin P-13 | `astral_vagrant_attack.mp4` | `4bc09872d86a6fa7837f372041cce66127bab85bed5d6235060ee87463d8f709` | P2 | 8 |
| C-034 | `prism_weaver` | Enclave | Idle | pin P-14 | `prism_weaver_idle.mp4` | `d03c5baba20cf3c2e6473d55ce858b7b478432fee3761a8839b193e67ccc0f77` | P2 | 8 |
| C-035 | `prism_weaver` | Enclave | Walk | pin P-14 | `prism_weaver_walk.mp4` | `d3813e3b6a97d7a7bb9e24823accb9d52b7856635f3c352b853787acf26be74d` | P2 | 8 |
| C-036 | `prism_weaver` | Enclave | Attack | pin P-14 | `prism_weaver_attack.mp4` | `4e2e7bd398167960ba1176ff5c44d83502c92614b7c3ce552de615991e4547dc` | P2 | 8 |
| C-037 | `stealth_eye_orb` | Enclave | Idle | pin P-15 | `stealth_eye_orb_idle.mp4` | `93e11ef3bbc3c3930e74d683f3da20b6bad954a37f9826551bc29e6790c003fb` | P2 | 8 |
| C-038 | `stealth_eye_orb` | Enclave | Walk | pin P-15 | `stealth_eye_orb_walk.mp4` | `7a9eb7c6355c705341f29c0ad1a29dc2c054692c57e584a05bcd6ec0d1a827f7` | P2 | 8 |
| C-039 | `phase_prism_saucer` | Enclave | Idle | pin P-16 | `phase_prism_saucer_idle.mp4` | `7cea64f575ea1402d55a722c4bd27b61586cb338c9065d30ab55c8322b95db3c` | P2 | 8 |
| C-040 | `phase_prism_saucer` | Enclave | Walk | pin P-16 | `phase_prism_saucer_walk.mp4` | `cc26222fc0b80f02d14938819cde559d077e801cb1def1116326c395bcc2c828` | P2 | 8 |
| C-041 | `void_goliath` | Enclave | Idle | pin P-17 | `void_goliath_idle.mp4` | `e38d6ce1254de041138cfcb09e97811ee1b6ff6a98bbcbf7d808dca311d9ddfd` | P2 | 8 |
| C-042 | `void_goliath` | Enclave | Walk | pin P-17 | `void_goliath_walk.mp4` | `bdfc6ae93cf243afc4526a5cf17c75739344f6fcd7cfa5e32e98b16095a4b5c4` | P2 | 12 |
| C-043 | `void_goliath` | Enclave | Attack | pin P-17 | `void_goliath_attack.mp4` | `b33589876b9adebd0508d158f3bf62c697fed359653f7a975d95d278abc85464` | P2 | 8 |
| C-044 | `flying_saucer` | Enclave | Idle | pin P-18 | `flying_saucer_idle.mp4` | `fddc0bc3b1ec4bc22c1ca1e222340468fc681addbe5c43e2c55e75b023aad2d1` | P2 | 8 |
| C-045 | `flying_saucer` | Enclave | Walk | pin P-18 | `flying_saucer_walk.mp4` | `e4c9a99a4f6c5bb607d1ffd17386e2d51d7976dd3a5c42d951049a57712a9af6` | P2 | 8 |
| C-046 | `flying_saucer` | Enclave | Attack | pin P-18 | `flying_saucer_attack.mp4` | `8f6108f362d56197dd96015b83066d51dc04aa24d417cdf20bee2e33fb432839` | P2 | 8 |
| C-047 | `prismatic_saucer` | Enclave | Idle | pin P-19 | `prismatic_saucer_idle.mp4` | `ce5cada45addd3e07c81742fb3fa0dc05d0a8ede13e98f5e689ddaeea3d20c44` | P2 | 8 |
| C-048 | `prismatic_saucer` | Enclave | Walk | pin P-19 | `prismatic_saucer_walk.mp4` | `6b923c936ef51a358a3b30fb9de0167a9262f4ba970b4ae401af857bf09781ce` | P2 | 8 |
| C-049 | `prismatic_saucer` | Enclave | Attack | pin P-19 | `prismatic_saucer_attack.mp4` | `9062afc1800f705bd2458bcbef426ea9f10f4ad91e8b5b753ace8a88207cc4c7` | P2 | 8 |
| C-050 | `shadow_visitor` | Enclave | Idle | pin P-20 | `shadow_visitor_idle.mp4` | `97be8ffe01ecf3b7753ffb5fe27aa24d0e148201827e6c5992f486b42f7709cc` | P2 | 8 |
| C-051 | `shadow_visitor` | Enclave | Walk | pin P-20 | `shadow_visitor_walk.mp4` | `735d3fb052fd227acfc45d478e91efcac066c0aa7165eb39ef8862f7e6bff0c5` | P2 | 12 |
| C-052 | `shadow_visitor` | Enclave | Attack | pin P-20 | `shadow_visitor_attack.mp4` | `a0e4405a07e3dbc5aedbf8bfb18bec39aeb487b5e9bf985cbb94731faf4855e9` | P2 | 8 |
| C-053 | `cerebral_elder` | Enclave | Idle | pin P-21 | `cerebral_elder_idle.mp4` | `112814d6c6791d8238db2632d6274eda5c4ec7b77619f8f920a905d6651ea65e` | P2 | 8 |
| C-054 | `cerebral_elder` | Enclave | Walk | pin P-21 | `cerebral_elder_walk.mp4` | `285eb6cffc1db53d2deb9ed24c3596e4073e6a852b90e8d9c8b6a9aee022603f` | P2 | 12 |
| C-055 | `cerebral_elder` | Enclave | Attack | pin P-21 | `cerebral_elder_attack.mp4` | `87592b989c1d2082b475f04697d7cf235abe3cf948601ab0a317e3b85a7623d9` | P2 | 8 |
| C-056 | `tripod_annihilator` | Enclave | Idle | pin P-22 | `tripod_annihilator_idle.mp4` | `fc57fd1a5168f5da6f978b163677c755e4f6edaa48066e3a661dd5bc0a5ed1d2` | P2 | 8 |
| C-057 | `tripod_annihilator` | Enclave | Walk | pin P-22 | `tripod_annihilator_walk.mp4` | `0b040cb2a43e434b8b7949bb51922462015e4090a339233282f086b5fd3e2fb6` | P2 | 12 |
| C-058 | `tripod_annihilator` | Enclave | Attack | pin P-22 | `tripod_annihilator_attack.mp4` | `deedfa781ef3e496c7554549716baa27c6c898ae63a59e252e9d5384ab344a21` | P2 | 8 |
| C-059 | `volatile_tick` | Swarm | Idle | pin P-23 | `volatile_tick_idle.mp4` | `b0ee964af9170a4aab992e8ddebebfaf2b707535bc627a64f3c56e70fa09119d` | P2 | 8 |
| C-060 | `volatile_tick` | Swarm | Walk | pin P-23 | `volatile_tick_walk.mp4` | `7324545897776270a309f479e84bbac6feaaeff370ec9abb42ec36ad12b194b0` | P2 | 12 |
| C-061 | `volatile_tick` | Swarm | Attack | pin P-23 | `volatile_tick_attack.mp4` | `e0030309f7ddba85214beb9198e04f4b9ffe115a31f95ffc9fd61391c436a2f7` | P2 | 8 |
| C-062 | `thorn_viper` | Swarm | Idle | pin P-24 | `thorn_viper_idle.mp4` | `68d7189c76ff0fb5c523be0a5de3b34737e3994cf7873cfa07c0313389fb72f3` | P2 | 8 |
| C-063 | `thorn_viper` | Swarm | Walk | pin P-24 | `thorn_viper_walk.mp4` | `db44b6a6ddc3cd00d803cfdd65a1a4bee9b0ef08083b85c24525eba9b47f602b` | P2 | 12 |
| C-064 | `thorn_viper` | Swarm | Attack | pin P-24 | `thorn_viper_attack.mp4` | `ee53050d2f45b726f471d46adfc4db7b851ba90b3616fbf3cd0f619479e7c2f2` | P2 | 8 |
| C-065 | `mortar_stag` | Swarm | Idle | pin P-25 | `mortar_stag_idle.mp4` | `c11cde92bab2a94120536a823f38a4d075e7f4ba29f277e4ba3e37cd1ec58a2b` | P2 | 8 |
| C-066 | `mortar_stag` | Swarm | Walk | pin P-25 | `mortar_stag_walk.mp4` | `ea320540844a52aa36e6ad66772219c5f661ae3efc9bfc5262a36e099f5ca4fc` | P2 | 12 |
| C-067 | `mortar_stag` | Swarm | Attack | pin P-25 | `mortar_stag_attack.mp4` | `ee05476a42124e28afbc005e384204c3fb6418c741591715ef978dc98260b2c8` | P2 | 8 |
| C-068 | `hive_matron` | Swarm | Idle | pin P-26 | `hive_matron_idle.mp4` | `e91ad7b23adcc086359157e39d5dd6c00e589f0466fad0606e51e5fad4f7def3` | P2 | 8 |
| C-069 | `hive_matron` | Swarm | Walk | pin P-26 | `hive_matron_walk.mp4` | `a385c6ed2af398bb1d335766d36a871692bc5255843fa3dac55e19a3a53aa05c` | P2 | 12 |
| C-070 | `hive_matron` | Swarm | Attack | pin P-26 | `hive_matron_attack.mp4` | `f1731be28e0f15a151062d13a287b5949b97d4306c6f5db3975bffecaa6550eb` | P2 | 8 |
| C-071 | `sonic_bat` | Swarm | Idle | pin P-27 | `sonic_bat_idle.mp4` | `b4d7d65c78614f44924dcf3ef5241ab9237a2c0f4aa6a60250076d2a0846ea43` | P2 | 8 |
| C-072 | `sonic_bat` | Swarm | Walk | pin P-27 | `sonic_bat_walk.mp4` | `a52e00ddfc7e2968c078b416e0cd60937dc597b126cb57977fa028827fdbcfa0` | P2 | 8 |
| C-073 | `sonic_bat` | Swarm | Attack | pin P-27 | `sonic_bat_attack.mp4` | `b7735059a5fb1b68a80e96ef878df1d10a597f5849c2b292d14be83f234f8006` | P2 | 8 |
| C-074 | `sky_leviathan` | Swarm | Idle | pin P-28 | `sky_leviathan_idle.mp4` | `eb765cb17998b5c42954468dffaec7c5c090993e74734201bdd85cde4e4f18d8` | P2 | 8 |
| C-075 | `sky_leviathan` | Swarm | Walk | pin P-28 | `sky_leviathan_walk.mp4` | `62644458b93ce1340cf15082b6286e2ae2883124827583751821e1d4901db2ce` | P2 | 8 |
| C-076 | `sky_leviathan` | Swarm | Attack | pin P-28 | `sky_leviathan_attack.mp4` | `41d6137bc63ea0b1b0d81379c88b8f49bb08f3ad623503a3216dfe277a66165e` | P2 | 8 |
| C-077 | `mycelium_shaman` | Swarm | Idle | pin P-29 | `mycelium_shaman_idle.mp4` | `78113c6e445477d2d1b9ed7f92e8b8238c3e2212d29e408da344a6bed426360a` | P2 | 8 |
| C-078 | `mycelium_shaman` | Swarm | Walk | pin P-29 | `mycelium_shaman_walk.mp4` | `8a10afa412a08dd808855f707ce2927a2b8e4d1c484788323daee9cd45dcf3ab` | P2 | 12 |
| C-079 | `tongue_chameleon` | Swarm | Idle | pin P-30 | `tongue_chameleon_idle.mp4` | `56a6d7f9e5a9b669b937931b970561ebba57c9ed75cd2eb72d088b1fda379cd3` | P2 | 8 |
| C-080 | `tongue_chameleon` | Swarm | Walk | pin P-30 | `tongue_chameleon_walk.mp4` | `639bba6e1ed801a7a12f04e572ce283715ba0f4b0adf02741c371632258706a9` | P2 | 8 |
| C-081 | `stag_behemoth` | Swarm | Idle | pin P-31 | `stag_behemoth_idle.mp4` | `b7fd1354a63dca57b272539b773aaaa6a12caab780d54a58ba97b90063218484` | P2 | 8 |
| C-082 | `stag_behemoth` | Swarm | Walk | pin P-31 | `stag_behemoth_walk.mp4` | `73909f423fe5b311190d22d538722f447288f44faf68243a5b4f9ac6aa18bfcc` | P2 | 12 |
| C-083 | `stag_behemoth` | Swarm | Attack | pin P-31 | `stag_behemoth_attack.mp4` | `a531740d4edc0420b3b5c77834aaf533617ce6e231bdeb4066024c2b38759335` | P2 | 8 |
| C-084 | `cybernetics_conduit_dome` | Enclave | Idle | pin P-32 | `cybernetics_conduit_dome_idle.mp4` | `a90b4151673ea064be3b4920f248281cfedd1a01e35ef1d6090c188783aa5c94` | P3 | 8 |
| C-085 | `cybernetics_conduit_dome` | Enclave | Constructing | pin P-32 | `cybernetics_conduit_dome_constructing.mp4` | `0bdae6308df201ce6139deb989cdc67f2d4e0fa4685ef494bb82cd5102f18c59` | P3 | 8 |
| C-086 | `crystalline_forge_dome` | Enclave | Idle | pin P-33 | `crystalline_forge_dome_idle.mp4` | `313327f51d197f85e1cecc6abcb507b76408ced842b7459a88c9b1fffa927c69` | P3 | 8 |
| C-087 | `crystalline_forge_dome` | Enclave | Constructing | pin P-33 | `crystalline_forge_dome_constructing.mp4` | `41f1336f6b945a04d3a8365fe01a316cf9a3516b05a0400d9c9dbb6fb5632672` | P3 | 8 |
| C-088 | `tesla_sentinel_dome` | Enclave | Idle | pin P-34 | `tesla_sentinel_dome_idle.mp4` | `227f4b616c60fcf00770bea6c7e610f64909ef68732a33ea6206f617ef5d9e6b` | P3 | 8 |
| C-089 | `tesla_sentinel_dome` | Enclave | Constructing | pin P-34 | `tesla_sentinel_dome_constructing.mp4` | `6e59eb4a5f10e912e4a6712c3ca2fec8031ece117a2ee927582755af41128dda` | P3 | 8 |
| C-090 | `harmonic_recharge_dome` | Enclave | Idle | pin P-35 | `harmonic_recharge_dome_idle.mp4` | `2cd7a7e4b96a16c9c07d8d7ee925b7baa0bbe07a1e71eb595b609d8cb7f74718` | P3 | 8 |
| C-091 | `harmonic_recharge_dome` | Enclave | Constructing | pin P-35 | `harmonic_recharge_dome_constructing.mp4` | `ce785dda720558ab22bcf283bb4b6a5909525746621ce96e5a77be3e0cb4016f` | P3 | 8 |
| C-092 | `templar_conclave_dome` | Enclave | Idle | pin P-36 | `templar_conclave_dome_idle.mp4` | `a48bf68ae58b88c905cd2df5f9bcc5ea7900f0eb441f8f1a551adc7a805efdff` | P3 | 8 |
| C-093 | `templar_conclave_dome` | Enclave | Constructing | pin P-36 | `templar_conclave_dome_constructing.mp4` | `b565d6796dfb21e2d12312d343c0ca1c89b1425f6bb3291a19e5215eb3a8fc06` | P3 | 8 |
| C-094 | `robotic_foundry_dome` | Enclave | Idle | pin P-37 | `robotic_foundry_dome_idle.mp4` | `cbbec08ec475086bea91fec4919467748dfaff17c4a14c515169283a5d4deae8` | P3 | 8 |
| C-095 | `robotic_foundry_dome` | Enclave | Constructing | pin P-37 | `robotic_foundry_dome_constructing.mp4` | `2b3efcd5054767a48eae23a60ad35367a5d3ebc7987f97dea3c5d043cbc37970` | P3 | 8 |
| C-096 | `stellar_portal_dome` | Enclave | Idle | pin P-38 | `stellar_portal_dome_idle.mp4` | `57adf9fa86889731a3208d6575c13317c2cc4a0fd456411060a0c613796bc3c3` | P3 | 8 |
| C-097 | `stellar_portal_dome` | Enclave | Constructing | pin P-38 | `stellar_portal_dome_constructing.mp4` | `af325a5ced46f2c8528e1ade6a8a5a34a381e6c40606cfdf1802682e9b1c27ff` | P3 | 8 |
| C-098 | `psionic_archives_dome` | Enclave | Idle | pin P-39 | `psionic_archives_dome_idle.mp4` | `f712c2703e438748d86c11c897974a7f357b450e765ad1746ea14a9830d775cf` | P3 | 8 |
| C-099 | `psionic_archives_dome` | Enclave | Constructing | pin P-39 | `psionic_archives_dome_constructing.mp4` | `bfd07207f0c8ca3810bcf9dc72e212efe76709def249f91d95b141d89ae2c30b` | P3 | 8 |
| C-100 | `void_sanctuary_dome` | Enclave | Idle | pin P-40 | `void_sanctuary_dome_idle.mp4` | `ea9570ca37af7aba519c77e0e8657e7a5fd7abe316b334a3b5b6d114eff7abff` | P3 | 8 |
| C-101 | `void_sanctuary_dome` | Enclave | Constructing | pin P-40 | `void_sanctuary_dome_constructing.mp4` | `d3eefcc9846302b48a046a0b2022265285efe681eb6f93c8aa540f3756de8cf6` | P3 | 8 |
| C-102 | `robotic_assembly_bay_dome` | Enclave | Idle | pin P-41 | `robotic_assembly_bay_dome_idle.mp4` | `56bfa97f171335ba451c127785a134a6a64bef30026d22b37486b2eed96490fa` | P3 | 8 |
| C-103 | `robotic_assembly_bay_dome` | Enclave | Constructing | pin P-41 | `robotic_assembly_bay_dome_constructing.mp4` | `8e0d903d1854100db61c996ed79244d5ee7623749bf804723e1b5a7d8ffe41aa` | P3 | 8 |
| C-104 | `celestial_beacon_dome` | Enclave | Idle | pin P-42 | `celestial_beacon_dome_idle.mp4` | `05b2afb1ff31056f49c24b2baec69f3121ec207183130cc6df98501b0ebbbf05` | P3 | 8 |
| C-105 | `celestial_beacon_dome` | Enclave | Constructing | pin P-42 | `celestial_beacon_dome_constructing.mp4` | `08e5c93e4baa6e265f9026043ed5146e90107f4a7782cf1ad351a933fbb0654b` | P3 | 8 |
| C-106 | `canopy_grove_dome` | Swarm | Idle | pin P-43 | `canopy_grove_dome_idle.mp4` | `731e8e6da4bb55513257f271829c78cd2bf982925f099a9c0a0cec743759007d` | P3 | 8 |
| C-107 | `canopy_grove_dome` | Swarm | Constructing | pin P-43 | `canopy_grove_dome_constructing.mp4` | `d7843a49fd552b2c14fe3b860026864e8327561c5ef9e946f8b352140cf7341d` | P3 | 8 |
| C-108 | `primal_world_tree_dome` | Swarm | Idle | pin P-44 | `primal_world_tree_dome_idle.mp4` | `49993e38d1593f91dde1d3d657bb4ca271d27cfcd0ebc3968e93d11b74dca594` | P3 | 8 |
| C-109 | `primal_world_tree_dome` | Swarm | Constructing | pin P-44 | `primal_world_tree_dome_constructing.mp4` | `baa02dbce8d0d9e8e27ac472ae5ecd8097fdf007c81b37a1ce7cc8ba15529448` | P3 | 8 |
| C-110 | `metamorphic_chamber_dome` | Swarm | Idle | pin P-45 | `metamorphic_chamber_dome_idle.mp4` | `9a833e7e07fe91ca89df458628101f644de69912b41ac76ad420b6fb0f884213` | P3 | 8 |
| C-111 | `metamorphic_chamber_dome` | Swarm | Constructing | pin P-45 | `metamorphic_chamber_dome_constructing.mp4` | `1fa6e99a2e26c93c7ce1c301bd067f833962a712e5b75cc476a8fb7c18c57123` | P3 | 8 |
| C-112 | `thorn_lasher_dome` | Swarm | Idle | pin P-46 | `thorn_lasher_dome_idle.mp4` | `3cee34c4ecf007eabc8021ffd16e6af23b992c4b84aade2ae485b05436310ec9` | P3 | 8 |
| C-113 | `thorn_lasher_dome` | Swarm | Constructing | pin P-46 | `thorn_lasher_dome_constructing.mp4` | `ff15965c3b6ee1be79dc8b88d76bdef1c7b7048ff882277d57bf73e3de0ce92e` | P3 | 8 |
| C-114 | `spore_spire_dome` | Swarm | Idle | pin P-47 | `spore_spire_dome_idle.mp4` | `9e9abaca7ccef886fbbb7739606abaca0cfd98c8114fb78834918cfc1d99121d` | P3 | 8 |
| C-115 | `spore_spire_dome` | Swarm | Constructing | pin P-47 | `spore_spire_dome_constructing.mp4` | `bc83e43e852ff7e0d167a44a6ff8ed201fdae15069b4aa6c41a5382df3532280` | P3 | 8 |
| C-116 | `carapace_mound_dome` | Swarm | Idle | pin P-48 | `carapace_mound_dome_idle.mp4` | `a9bfc1b8130b79faded5d1c6f987cfaee055ec7a7521a7f763e5ac290d613fb8` | P3 | 8 |
| C-117 | `carapace_mound_dome` | Swarm | Constructing | pin P-48 | `carapace_mound_dome_constructing.mp4` | `a964606e4fe34e0a03b5fba0b781187accac5fb7a1321cd1dbdc9c9cded52cae` | P3 | 8 |
| C-118 | `caustic_nursery_dome` | Swarm | Idle | pin P-49 | `caustic_nursery_dome_idle.mp4` | `98c0fe0af5e9e128897bc35307e9e85de97e0afd2d8433ddcc6ba976ceee6dc8` | P3 | 8 |
| C-119 | `caustic_nursery_dome` | Swarm | Constructing | pin P-49 | `caustic_nursery_dome_constructing.mp4` | `9ed2de2584caaeada1015cecbf9feb72c357b0e95ddb237aaa3d0afcf66ce678` | P3 | 8 |
| C-120 | `spine_den_dome` | Swarm | Idle | pin P-50 | `spine_den_dome_idle.mp4` | `1142273a795feb90748a106c9572bc694fae23ba09b64aefc1a750146cf134b6` | P3 | 8 |
| C-121 | `spine_den_dome` | Swarm | Constructing | pin P-50 | `spine_den_dome_constructing.mp4` | `a4daa77abbdfa074594323ed7550ba0953f7a4af4d5068e1cbe33ea0e7c81fff` | P3 | 8 |
| C-122 | `burrow_spire_dome` | Swarm | Idle | pin P-51 | `burrow_spire_dome_idle.mp4` | `827608ecc7d536226de966a40d33c4e56d891d671eff4413569c7c0fa6351f2c` | P3 | 8 |
| C-123 | `burrow_spire_dome` | Swarm | Constructing | pin P-51 | `burrow_spire_dome_constructing.mp4` | `76471bd4927af7c8995798f0d6ee01c13a519bc7622d1e4413aa8665e63b21bc` | P3 | 8 |
| C-124 | `chitin_spire_dome` | Swarm | Idle | pin P-52 | `chitin_spire_dome_idle.mp4` | `899d8cca77e36bc645c08d2d024a9b4592c2a07317654fd87ebb23b79481f5aa` | P3 | 8 |
| C-125 | `chitin_spire_dome` | Swarm | Constructing | pin P-52 | `chitin_spire_dome_constructing.mp4` | `ec900c56f4520db19b4388409ff95c2936b875f91da81feb035cf7646b8e8554` | P3 | 8 |
| C-126 | `apex_spire_dome` | Swarm | Idle | pin P-53 | `apex_spire_dome_idle.mp4` | `c27d14243250fb819b6ea1011a51996b72c2890b294ef831a1697b4dcdb5b213` | P3 | 8 |
| C-127 | `apex_spire_dome` | Swarm | Constructing | pin P-53 | `apex_spire_dome_constructing.mp4` | `24d1a0d64a3f8ce664fac6addb3b14a30bda1f6a0fdb386e82852834b2349175` | P3 | 8 |
| C-128 | `infestation_nursery_dome` | Swarm | Idle | pin P-54 | `infestation_nursery_dome_idle.mp4` | `bfed3620141aa35fa243177c309cd34bc420223201b8aac1644eeb9b5a906327` | P3 | 8 |
| C-129 | `infestation_nursery_dome` | Swarm | Constructing | pin P-54 | `infestation_nursery_dome_constructing.mp4` | `eb069231c36c3ab8a6d8a7a835920b668aed13e424e16e438282a1765e1d78db` | P3 | 8 |
| C-130 | `subterranean_network_dome` | Swarm | Idle | pin P-55 | `subterranean_network_dome_idle.mp4` | `6b72fa4678c1d8e98ea11748bb8cb2b19b780fec346e3c8eec234c1e480e6970` | P3 | 8 |
| C-131 | `subterranean_network_dome` | Swarm | Constructing | pin P-55 | `subterranean_network_dome_constructing.mp4` | `50e7b7851e2209cbbc7e4114f4fc3b31b3fd4c2e3f6cc651b16d6a083659dd9f` | P3 | 8 |
| C-132 | `behemoth_den_dome` | Swarm | Idle | pin P-56 | `behemoth_den_dome_idle.mp4` | `8de80828632ec10c0c20841fd4112fd84021f71c6402316a927e5b076b0e5a39` | P3 | 8 |
| C-133 | `behemoth_den_dome` | Swarm | Constructing | pin P-56 | `behemoth_den_dome_constructing.mp4` | `2ad87201a7f9d544a1860693818356b18702b60c9f5637d6337bfe9b2e1be4d3` | P3 | 8 |

### 5.1 How each kind of clip should look

These add to batch 1 §5, criterion 6 ("the action is right, and reads small"). All of them keep criteria 1 to 5:
real, one subject, the same object as its pin, a static camera with at most 1.5 % growth, and a plain backdrop.

- **Idle (every identity):** the subject holds its pose as the text says. Units do not turn, nod, sway or bob. Only
  the named lights glow. Structures are completely still, and only small ambient effects move (glow, vapour, small
  lights).
- **Walk, ground units (walker, infantry, creature):** the legs clearly lift and step in turn through the whole clip,
  as if on a treadmill. The body keeps its place, size and three-quarter facing, and never travels or turns toward the
  camera. A serpent's coils ripple in place. These are the hardest clips of the batch: seven Walks of the mech and ten
  of the scorpion were rejected before one passed.
- **Walk, aircraft and flyers:** flight in place, as if held in a wind tunnel. Aircraft show light trails streaming
  toward the upper left; winged flyers show wings rippling or beating evenly. Nothing travels or turns, and nothing
  touches a ground.
- **Attack:** the strike or shot goes toward the lower right (a lobbed bile glob goes up and out at the upper right),
  and the subject returns to the pin's pose by the last frame. The clip is pinned at its end to the pin.
- **Cast (the two workers):** a steady loop. The drone holds a thin green extraction beam toward a point just outside
  the frame. The ant works its mandibles in an even rhythm. Body and legs hold still.
- **Constructing (structures):** the finished building keeps its shape, size and place while holographic scaffolding
  and seam sparks play over it and fade, and the last frame matches the pin.

**Loop treatment the baker will apply** (`media_style.ron`, `loops`): Idle takes the default (both ends pinned, a
scale lock, a 2-frame cross-fade). Ground Walks take no scale lock and no cross-fade, so their raw frames must loop on
their own. Flight Walks take no scale lock with a 2-frame cross-fade. Attacks are held to their pinned end.
Constructing is pinned at both ends with no repair. Judge your attempts with that in mind; `asset_baker loop-check
--unit <slug> --state <state> --clip <file> --pin <pin>` measures it offline and for free (batch 1 §5, criterion 7).

---

## 6. Batching guidance

### 6.1 Attempt caps

| Request | Attempts |
|---|---|
| A pin (§4.2) | up to 6 image attempts, then up to 4 Veo seeds |
| A clip, by default | up to **8** |
| A ground Walk (kinds walker, infantry, creature; the "Attempts" column says 12) | up to **12** |

Stop at the first attempt that passes. The caps exist to keep the history readable, not to save money. Record every
attempt (batch 1 §4.3), refused and filtered requests included.

### 6.2 Throughput

- One request per clip, one sample per request (batch 1 §2). You may run several requests at once.
- Commit per identity, or per clip folder, as you go (batch 1 §4.2), so that a stopped session loses nothing. A pin
  is its own commit, before its clips.
- If the service limits you by quota, stop, record the refusal as a `failed` attempt, and resume later in the same
  order.

### 6.3 When an identity is stuck

1. **The Idle fails on its pin** for all 8 attempts, for the same reason each time (for example the pose drifts, a
   part nods, the backdrop changes): the pin is probably the cause. Make **one** new pin (§4.2) and generate the Idle
   on it, with up to 8 more attempts. Hand in both pins and say which one the clips use. All states of an identity
   must start on the same pin.
2. **A later state fails** on a pin whose Idle passed: do not change the pin, because a new pin would withdraw the
   accepted Idle. Hand in the best attempt as `best_effort` (batch 1 §5) and move on.
3. **A wording problem.** After 6 failed attempts of one clip that fail for the same reason, you may try **one**
   variant of the prompt: a short added clause that names the fault ("its tail stays still", "the backdrop stays
   unchanged"). Record the exact text sent and its SHA-256, as batch 1 §3.2 allowed for the mech. Never remove the
   camera, loop or backdrop clauses. The orchestrator moves an accepted variant into the catalog.
4. **A policy or quality block** (a filtered prompt, refused content, a model that will not draw the subject): stop
   that identity, record it in `notes`, and go on with the next one. Do not change the design to get past a filter.
5. **Never** hand in a clip or picture that fails criteria 1 to 5 of batch 1 §5 or 1 to 3 of §4.4, not even as
   `best_effort`.

---

## 7. Hand-in format

Batch 1 §4 applies to clips unchanged: one folder per clip, `handin.json` per folder, only the proposed attempts
committed (the best and up to two runners-up), one commit per folder, unsigned, no force-push. For batch 2, the folder
of a clip is `docs/media/veo-handin/<slug>/<state>/` and of a polish clip
`docs/media/veo-handin/polish/<slug>/<state>/`. Put the request's ID (`C-001`, `PC-01`, ...) in `notes`.

**Pins** get a folder of their own, next to their identity's clip folders:

```text
docs/media/veo-handin/<slug>/pin/
  handin.json                    # required, below
  <slug>_idle.png                # the proposed pin, 1280x720
  <slug>_idle.original.<ext>     # only if an image model's output was scaled and cropped (4.3)
  <operation-stem>.mp4           # only for a Veo seed (4.2): the seed clip the pin is frame 0 of
```

```json
{
  "format_version": 1,
  "slug": "warp_strider",
  "state": "pin",
  "request_id": "P-11",
  "generator": { "environment": "...", "agent": "...", "cost_to_this_project_usd": 0 },
  "attempts": [
    {
      "attempt_id": "img-01",
      "method": "image_model | veo_seed",
      "date_utc": "2026-11-29T10:15:00Z",
      "model": "gemini-3.1-flash-image",
      "prompt": "the exact text sent (Appendix A for an image, the Idle clip prompt for a seed)",
      "prompt_sha256": "lowercase hex SHA-256 of the UTF-8 prompt",
      "negative_prompt": "the exact negative text sent, or null",
      "parameters": { "aspect_ratio": "16:9", "seed": 7 },
      "operation": "the Veo operation name for a seed, else null",
      "original_file_sha256": "SHA-256 of the file the service returned (the .mp4 for a seed)",
      "original_size": "1344x768",
      "transform": "null, or the exact scale-and-crop command (4.3), or the frame-0 extraction command for a seed",
      "pin_sha256": "SHA-256 of <slug>_idle.png, or null if this attempt gave no pin",
      "file_committed": true,
      "verdict": "proposed | runner_up | rejected | best_effort | failed",
      "reason": "one line against the criteria of 4.4"
    }
  ],
  "notes": "anything the reviewer should know"
}
```

In every clip's `handin.json`, `start_image_sha256` and `last_frame_sha256` are the SHA-256 of the pin the clip
started on. That is how the orchestrator ties a clip to its pin.

---

## 8. Polish pass (priority P4)

The owner's answers to [Q0131](../questions/closed/0131-carrier-media-light-shafts.md) and
[Q0133](../questions/closed/0133-brood-lord-media-light-shafts.md) accepted four identities' flaws as a temporary
exception and deferred a polish pass. That pass is now requested, after P1 to P3. Its prompts are the catalog texts the
shipped clips were made with, unchanged; the dry run for these four identities, against an empty install folder,
planned exactly these clips.

| Identity | What is wrong now ([carrier](../evidence/veo/2026-11-26-batch-carrier.md) and [broodlings](../evidence/veo/2026-11-28-batch-broodlings.md) evidence) | What is requested |
|---|---|---|
| `mothership_carrier` | Its pin and clips show a soft dark shadow to the lower left on a slightly graded backdrop | A new pin, then Idle, Walk and Attack on it |
| `saucer_interceptor` | Its pin and all three clips show faint diagonal light shafts at the upper right; its Idle's worst loop step and its Walk's trails were accepted below the bar | A new pin, then Idle, Walk and Attack on it |
| `spore_matriarch` | Its pin's left wing tip touches the frame edge, with faint shafts at the top; its Idle and Walk beat their wings (+17.65 % growth) | A new pin, then Idle, Walk and Attack on it |
| `brood_mite` | Its Idle nods its head slightly; its Walk's worst loop step is just under the bar. Its pin and Attack are clean | Idle and Walk on the **current** pin |

**All or nothing per identity.** A new pin withdraws every installed clip of its identity, so a polish identity is
installed only when its new pin and all of its clips pass. Otherwise the shipped clips stay. A new pin must be
free of light shafts, shadow and edge contact: that was the defect. The negative prompts already carry these
identities' extra terms (light shafts for the saucers and the Brood Lord analogue, "flapping wings" for the Brood Lord
analogue, "head turning, nodding" for the Broodling).

| ID | Identity | State | Start and end picture | Install name | `prompt_sha256` | Attempts |
|---|---|---|---|---|---|---|
| PP-1 | `mothership_carrier` | new pin | text: Appendix C | `mothership_carrier_idle.png` (replaces `106e64bb…`) | image prompt (Appendix C) | 6 + 4 |
| PP-2 | `saucer_interceptor` | new pin | text: Appendix C | `saucer_interceptor_idle.png` (replaces `b303d4b9…`) | image prompt (Appendix C) | 6 + 4 |
| PP-3 | `spore_matriarch` | new pin | text: Appendix C | `spore_matriarch_idle.png` (replaces `473270ff…`) | image prompt (Appendix C) | 6 + 4 |
| PC-01 | `mothership_carrier` | Idle | PP-1 | `mothership_carrier_idle.mp4` | `4ba8db05ff56cbd6377b1502f036d6b66d455bc57262be7312551b37e69b2d25` | 8 |
| PC-02 | `mothership_carrier` | Walk | PP-1 | `mothership_carrier_walk.mp4` | `df006cefaca96a4b2945c8817e7de875979312e0e82bbee60a6386ac5744beff` | 8 |
| PC-03 | `mothership_carrier` | Attack | PP-1 | `mothership_carrier_attack.mp4` | `b5ab939a28aefd475c75e0d7800c84d52db11bea8eb2dc9f32157cb768094662` | 8 |
| PC-04 | `saucer_interceptor` | Idle | PP-2 | `saucer_interceptor_idle.mp4` | `56a2bea1740da1ba57a7a557b0f6092b0cfd8a4aac542c9d98613411af218f11` | 8 |
| PC-05 | `saucer_interceptor` | Walk | PP-2 | `saucer_interceptor_walk.mp4` | `7aa51c260cc85efe42a0fe00bff11f84763583eef929b56def5e7f6bfcb286d4` | 8 |
| PC-06 | `saucer_interceptor` | Attack | PP-2 | `saucer_interceptor_attack.mp4` | `ffef78ce91823897eadcf2af655d163a52263a0ee69720641f8fa010772e1451` | 8 |
| PC-07 | `spore_matriarch` | Idle | PP-3 | `spore_matriarch_idle.mp4` | `336ef61e3c683a591ae53bc810685c2d7010d51b0b4f38d538baaed303f5484a` | 8 |
| PC-08 | `spore_matriarch` | Walk | PP-3 | `spore_matriarch_walk.mp4` | `c902d9c4fbed7424a5b99aad7841fbf0733beacc4963b208898a9306ddf20837` | 8 |
| PC-09 | `spore_matriarch` | Attack | PP-3 | `spore_matriarch_attack.mp4` | `9fde072c8b68fa09cada7b2a2620cd1ec738db59f181f7b35faa950f9e28a35c` | 8 |
| PC-10 | `brood_mite` | Idle | the current `brood_mite_idle.png` (`46ce66af…`) | `brood_mite_idle.mp4` | `4406264c04b9f4035b302fc3a5b27903f35470ca2204a96076222e7e25e7c06c` | 8 |
| PC-11 | `brood_mite` | Walk | the current `brood_mite_idle.png` (`46ce66af…`) | `brood_mite_walk.mp4` | `182ca4003ec8a5d7dc906a3a9ebcd41484029d20b20c3e5d4ec4df0f1f3846fa` | 12 |

The image prompts of the three new pins are in Appendix C, and every polish clip's prompt and negative prompt in
Appendix D.

---

## 9. What happens here after hand-in

Batch 1 §6 applies: check the SHA-256s and `ffprobe`, record provenance, review with contact sheets and
`loop-check`, install, bake, add to `assets/media/required_set.ron` as `Expansion`, run the strict audit and the tests,
and write a batch evidence file. Batch 2 adds:

1. **Recording an outside generation (the install blocker).** `install-clip` installs only a clip whose SHA-256 has a
   `downloaded` line in `docs/evidence/veo/ledger.jsonl`, and a baked atlas's provenance origin is only `Veo` or
   `LegacyVeo`. Before installing any clip of this batch, the orchestrator adds an "external generation" ledger event
   and provenance origin to `asset_baker` (a small change under its own plan note). It then writes the ledger entries
   from your `handin.json` files. **You need not do anything about this.** Keep `handin.json` complete; it is the only
   record the ledger entries come from.
2. **Pins first.** An accepted pin moves to `assets/ai_keyframes/<slug>_idle.png` before any of its clips are
   reviewed. The orchestrator then reruns the free dry run, which must show the pin as every clip's start and end, and
   checks each clip's `start_image_sha256` against it. An image-model pin is recorded in the batch evidence with its
   model id and prompt, as the effect textures record theirs.
3. **Identities unlock one at a time.** An identity's atlas is baked from all of its published states, so it is
   shown in the game once every one of its clips is accepted: Idle, Walk and Attack (and Cast for a worker) for a
   unit, Idle and Constructing for a structure.
4. **Tests.** Besides batch 1 §6 step 6: `prompt_tests.rs` (the golden list of unit prompt hashes,
   `tests/fixtures/unit_prompts_golden.txt`, already holds this batch's texts)
   and the catalog loaders' `runtime_catalog_tests.rs` (in `sim_core` and `asset_baker`).

---

## 10. Decisions and what is left out

- **Prompts are data and were vetted against the evidence.** Every identity's texts are in the catalog
  (`units.ron`, `structures.ron`) and every unit's per-state wording in `media_style.ron` (`actions.overrides`), with
  comments on why. They avoid what made earlier clips fail: floors, dust and ground ("over terrain", "uneven ground"),
  scenery (clouds, sky, breeze), light shafts (no "hovering", "high above" or light from above; flyers carry the
  Carrier's negative terms), turning parts (rings, prongs, radar, sensor heads), camera moves, colour change and
  see-through bodies. They follow the house patterns that passed: the Carrier's for aircraft, the Brood Lord
  analogue's for winged creatures, the scorpion's and the Broodling's for ground creatures, and the golem's and the
  mech's for walkers.
- **Structures keep the house action texts.** The baker's tests pin every structure to the `building_dome` Idle and
  Constructing texts (`actions_every_structure_uses_the_building_dome_texts`). So the Swarm's living buildings are
  also built under "holographic wireframe scaffolding ... welding sparks". A grown, organic Constructing look for the
  Swarm would need a structure override, which the baker does not allow. It is deferred, and noted here so it can be
  brought back. Each structure's look is carried by its identity text, and every structure takes the negative terms
  "floor, ground plane, cast shadow, horizon".
- **Tests that needed an unauthored identity.** Until this batch, the shipped catalog held structures without
  prompts, and the baker's tests of the display-name fallback and of its refusal picked one of them. Every identity
  is authored now, so those tests (`prompt_tests.rs`, `veo_cli_tests.rs`) empty one structure's prompts in a
  temporary copy of the data (`tests/common/unauthored.rs`) instead. The shipped data is never written, and what the
  tests check is unchanged. The golden list of unit prompt hashes holds the 69 new unit texts.
- **No mode clips.** No identity of this batch has a catalog mode (only `railgun_crawler` and `burrow_scorpion`
  have one, and both have their mode media), so no enter, leave or mode-state clip is requested.
- **`psi_grey` and `rhino_beetle`** keep their legacy atlases without provenance. They have installed clips, so the
  dry run does not plan them, and they are not requested. Regenerating them with provenance would be a separate
  request.
- **Legacy square JPEGs removed.** `flying_saucer_idle.jpg` and `tesla_sentinel_dome_idle.jpg` cannot pin a loop
  (§4.1). With them in place, `validate-prompts` refused both identities' Idles.

---

## Appendix A: image prompts of the pins

#### P-01 `grav_harvester`

```text
Levitation Drone: a single small silver alien worker drone, a rounded teardrop body with one large cyan crystal eye at its front, two short golden grippers and a soft cyan glow under it, held still, seen from above at a three-quarter angle looking down on its top, front toward the lower right, the whole drone in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-02 `nectar_worker`

```text
Forager Ant: a single large worker ant of iridescent dark green chitin, a big head with broad mandibles, two short antennae with amber tips, six legs, standing still, seen from above at a three-quarter angle, head toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-03 `citadel_nexus_dome`

```text
Citadel Nexus Dome: a single large golden alien pyramid hub on a broad octagonal base, curved gold plates with violet seams, a huge cyan crystal in its apex, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The crystal glows steadily and small violet lights flicker along the seams. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-04 `ancient_tree_dome`

```text
Ancient Tree Dome: a single large living hive mound like a squat hollow tree stump of ridged bark and dark brown chitin, amber egg sacs around its wide top, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The egg sacs glow steadily and thin amber vapour rises gently. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-05 `psi_pylon_dome`

```text
Psi Pylon Dome: a single small alien power pylon, one large upright cyan crystal shard held in a squat round golden base by four short curved golden prongs, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The crystal glows steadily and faint cyan motes drift up from it. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-06 `amber_pollen_dome`

```text
Amber Spore Balloon: a single huge gas-sac creature, a round translucent amber balloon body with glowing veins, a small dark chitin head plate at its front and a fringe of short tentacles hanging below it, seen from above at a three-quarter angle, head toward the lower right, the whole creature in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-07 `rift_siphon_dome`

```text
Rift Siphon Dome: a single squat golden alien extractor dome capping a gas vent, curved gold plates around a violet opening on its top, small cyan crystals, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The opening glows steadily and a thin wisp of green vapour rises from it. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-08 `spore_vent_dome`

```text
Spore Vent Dome: a single squat living extractor capping a gas vent, a bulbous funnel of dark brown chitin and bark with a wide green mouth, amber pores, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. A thin wisp of green vapour rises from the mouth and the pores glow steadily. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-09 `warp_gate_dome`

```text
Warp Gate Dome: a single golden alien gateway, a thick round golden arch on a low curved base, a sheet of blue-violet energy filling the arch, a cyan crystal on each side, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The energy in the arch glows steadily and shimmers faintly. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-10 `spawning_hollow_dome`

```text
Spawning Hollow Dome: a single living nest, a low round basin of curved bark ribs and brown chitin holding a pool of glowing amber fluid, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The fluid glows steadily and small bubbles rise slowly in it. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-11 `warp_strider`

```text
Phase Tripod: a single sleek alien three-legged walker, a compact silver-and-violet armoured body on three long jointed legs, a short cannon under its front, a cyan eye, standing still, seen from above at a three-quarter angle, front toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-12 `razor_mantis`

```text
Razor Mantis: a single small lean predatory mantis of emerald green chitin, two long serrated scythe forelegs folded in front, four thin legs, a narrow head with amber eyes, crouched, seen from above at a three-quarter angle, head toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-13 `astral_vagrant`

```text
Astral Projecting Grey: a single slender grey-skinned alien warrior with a large bald head and big violet eyes, silver armour with violet trim, a curved violet glaive held low, standing still, seen from above at a three-quarter angle, front toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-14 `prism_weaver`

```text
Force-Matrix Grey: a single small alien support drone, a smooth silver sphere with one large cyan eye inside two thin fixed golden rings set with small violet crystals, held still, seen from above at a three-quarter angle looking down on its top, front toward the lower right, the whole drone in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-15 `stealth_eye_orb`

```text
Stealth Eye Orb: a single small alien scout probe, a smooth dark silver oval body with one large glowing cyan lens at its front and two short fixed fins, held still, seen from above at a three-quarter angle looking down on its top, front toward the lower right, the whole probe in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-16 `phase_prism_saucer`

```text
Phase Transporter: a single alien transport craft, a long faceted violet crystal hull set in a curved golden frame, two fixed golden side pods and a glowing cyan core inside the crystal, held still, seen from above at a three-quarter angle looking down on its top, front toward the lower right, the whole craft in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-17 `void_goliath`

```text
Hardened Shield Walker: a single heavy alien four-legged walker, a broad silver-and-gold armoured body on four thick jointed legs, two long forward cannons with blue muzzles, standing still, seen from above at a three-quarter angle, front toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-18 `flying_saucer`

```text
Roswell Flying Saucer: a single sleek silver saucer fighter, a thin flat disc with a small clear cockpit dome, fixed violet lights around its rim and a violet emitter lens at its front edge, held still, seen from above at a three-quarter angle looking down on its top, front toward the lower right, the whole saucer in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-19 `prismatic_saucer`

```text
Prismatic Saucer: a single alien warship, a long flat gold-and-silver arrowhead hull with three fixed curved prongs at its front around a glowing white-violet prism core, held still, seen from above at a three-quarter angle looking down on its top, front toward the lower right, the whole warship in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-20 `shadow_visitor`

```text
Cloaked Abductor: a single tall lean grey-skinned alien assassin in close dark violet armour and a hood, two green eyes under the hood, a long curved green crystal scythe held low, standing still, seen from above at a three-quarter angle, front toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-21 `cerebral_elder`

```text
Mind-Storm Elder: a single ancient grey-skinned alien elder with a huge bald head and glowing violet eyes, in long gold-and-violet robes, seated cross-legged on a small flat golden disc, its hands resting on its knees, seen from above at a three-quarter angle, front toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-22 `tripod_annihilator`

```text
Thermal Tripod: a single tall alien war walker, a compact gold-and-silver body with an orange lens on each side on three very long thin legs, two heat lances under it, standing still, seen from above at a three-quarter angle, front toward the lower right, whole from body to feet and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-23 `volatile_tick`

```text
Acid-Burst Beetle: a single small round tick-like beetle with a swollen translucent bright green acid sac on its back, dark brown chitin plates, six short legs, crouched low, seen from above at a three-quarter angle, head toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-24 `thorn_viper`

```text
Spine-Spitter Cobra: a single serpent creature rearing upright, a coiled body of dark green scales and brown chitin plates, a wide hood of bone spines, two small clawed arms, amber eyes, seen from above at a three-quarter angle, head toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-25 `mortar_stag`

```text
Bile-Artillery Beetle: a single large stag beetle of dark brown chitin on long legs, two curved horns at its front, a tall curved tube cannon on its back glowing orange at its mouth, seen from above at a three-quarter angle, head toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-26 `hive_matron`

```text
Hive Matron: a single tall regal swarm creature on six thin legs, an upright dark green chitin body, a crowned head, two long talon arms folded in front, amber sacs on its back, seen from above at a three-quarter angle, front toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-27 `sonic_bat`

```text
Glaive Wasp: a single winged predator wasp of dark green chitin, two pairs of long translucent amber-veined wings spread wide, a long segmented tail ending in a hooked stinger, glowing amber eyes, seen from above at a three-quarter angle, head toward the lower right, the whole wasp in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-28 `sky_leviathan`

```text
Sky Leviathan: a single heavy armoured flying beast shaped like a squid, a bulky body of dark green and brown chitin plates, a crown of short thick tentacles trailing behind it, glowing amber spore vents along its sides, seen from above at a three-quarter angle, head toward the lower right, the whole beast in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-29 `mycelium_shaman`

```text
Mycelium Shaman: a single bulbous fungal creature, a round swollen body of mottled brown and green skin dotted with amber spore sacs, small dark eyes at its front, stubby legs, crouched low, seen from above at a three-quarter angle, front toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-30 `tongue_chameleon`

```text
Tongue Chameleon: a single flying serpent creature of iridescent green scales, two narrow membrane wings spread wide, a long curled tail, a chameleon-like head with bulging amber eyes, its long tongue rolled in its mouth, seen from above at a three-quarter angle, head toward the lower right, the whole creature in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-31 `stag_behemoth`

```text
Armored Stag Behemoth: a single enormous four-legged beast of thick dark brown chitin plates glowing amber between them, a low head with two huge curved scythe horns, standing still, seen from above at a three-quarter angle, head toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-32 `cybernetics_conduit_dome`

```text
Cybernetics Conduit Dome: a single golden alien research core, a rounded hexagonal golden block, a large cyan crystal sphere in its front, thick conduits arching over it, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The crystal sphere glows steadily and small lights flicker along the conduits. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-33 `crystalline_forge_dome`

```text
Crystalline Forge Dome: a single low wide golden alien forge dome of curved gold plates, three short cyan crystal spikes on top, orange heat vents at its base, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The crystals glow steadily and thin vapour rises gently from the vents. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-34 `tesla_sentinel_dome`

```text
Tesla Sentinel Dome: a single small alien defence turret, a violet energy orb held in a cradle of four curved golden claws on a squat round golden base, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The orb glows steadily and tiny violet sparks crackle on its surface. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-35 `harmonic_recharge_dome`

```text
Harmonic Recharge Dome: a single small golden alien battery, a rounded golden drum ringed with cyan crystal panels, a shallow bowl of pale blue light on top, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The panels and the bowl glow steadily and small lights flicker. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-36 `templar_conclave_dome`

```text
Templar Conclave Dome: a single elegant golden alien council hall, round and tiered, violet windows, three short golden spires around a violet crystal on top, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The crystal glows steadily and soft violet lights flicker in the windows. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-37 `robotic_foundry_dome`

```text
Robotic Foundry Dome: a single heavy golden alien robotics factory, a broad squat block of gold and silver plates, a large round sealed hatch in front, cyan crystals, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The crystals glow steadily and small lights flicker around the hatch. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-38 `stellar_portal_dome`

```text
Stellar Portal Dome: a single large golden alien stargate, a wide flat golden ring standing upright on a low curved base, a veil of deep blue energy filling the ring, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The energy veil glows steadily and shimmers faintly. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-39 `psionic_archives_dome`

```text
Psionic Archives Dome: a single golden alien archive temple, a stepped round tower with violet glass panels, a violet crystal on top, cyan crystals at its base, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The crystal glows steadily and soft violet light flickers behind the glass. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-40 `void_sanctuary_dome`

```text
Void Sanctuary Dome: a single dark alien shrine, a low ring of tall pointed black-and-silver stone blades around a dark violet crystal orb fixed at its centre, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The orb glows steadily and faint violet mist curls gently around it. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-41 `robotic_assembly_bay_dome`

```text
Robotic Assembly Bay Dome: a single low wide golden alien engineering bay with two fixed curved crane arms folded over its roof and a cyan crystal panel on its front, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The panel glows steadily and small cyan lights flicker. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-42 `celestial_beacon_dome`

```text
Celestial Beacon Dome: a single golden alien beacon, a broad round base with three curved golden arms rising to hold a large white-blue star crystal between their tips, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The crystal glows steadily and tiny sparks of light drift around it. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-43 `canopy_grove_dome`

```text
Canopy Grove Dome: a single large living hive mound of ridged bark and brown chitin under a low spreading canopy of dark green leathery leaf plates hung with glowing amber pods, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The pods glow steadily and thin amber vapour rises gently. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-44 `primal_world_tree_dome`

```text
Primal World Tree Dome: a single huge gnarled living tree hive, a thick twisted trunk of bark and chitin under a broad dome of dark green leaves with amber pods, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The pods glow steadily and thin amber vapour rises gently. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-45 `metamorphic_chamber_dome`

```text
Metamorphic Chamber Dome: a single living gestation organ, a round swollen pod of dark green hide in brown chitin ribs, amber veins over it, a slit on its top, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The veins and the slit glow steadily and thin vapour rises gently. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-46 `thorn_lasher_dome`

```text
Thorn Lasher Dome: a single living defence growth, a low mound of dark brown chitin with a thick spiked tentacle coiled on top, a long bone spike tip, amber eyes, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The eyes glow steadily and the tentacle stays coiled and still. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-47 `spore_spire_dome`

```text
Spore Spire Dome: a single living anti-air growth, a short thick stalk of dark green chitin on a low round base, topped by a swollen bulb ringed with amber pores, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The pores glow steadily and faint amber spores drift up gently. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-48 `carapace_mound_dome`

```text
Carapace Mound Dome: a single low armoured burrow mound of overlapping thick brown carapace plates like a beetle shell, a dark round tunnel mouth at its front lit amber inside, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The tunnel mouth glows steadily and thin vapour rises gently. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-49 `caustic_nursery_dome`

```text
Caustic Nursery Dome: a single living nursery, a cluster of round translucent bright green acid sacs nested in curved dark brown chitin ribs, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The acid sacs glow steadily and small bubbles drift inside them. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-50 `spine_den_dome`

```text
Spine Den Dome: a single living den, a hunched dome of dark green scaled hide bristling with long bone spines along its back, a dark arched opening at its front lit amber inside, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The opening glows steadily and thin vapour rises gently. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-51 `burrow_spire_dome`

```text
Burrow Spire Dome: a single living den, a low spiral shell of dark brown chitin coiled like a snail shell, jagged bone spikes along its ridge and amber light in its opening, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The opening glows steadily and thin vapour rises gently. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-52 `chitin_spire_dome`

```text
Chitin Spire Dome: a single slender living spire of twisted dark green and brown chitin, ridged like a horn and curving up to a point, glowing amber pores along it, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The pores glow steadily and thin amber vapour rises gently. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-53 `apex_spire_dome`

```text
Apex Spire Dome: a single tall twisted living spire of dark chitin with three curving horn branches at its top and large glowing amber pods along it, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The pods glow steadily and thin amber vapour rises gently. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-54 `infestation_nursery_dome`

```text
Infestation Nursery Dome: a single living nursery, a wide round fleshy mound, thick chitin lips around a green opening on top, small spore sacs on its sides, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The opening glows steadily and thin green vapour rises gently. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-55 `subterranean_network_dome`

```text
Subterranean Network Dome: a single living tunnel hub, a large hunched mound of dark brown chitin with a huge round toothed maw at its front, held open and glowing amber inside, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The maw glows steadily and thin vapour rises gently. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### P-56 `behemoth_den_dome`

```text
Behemoth Den Dome: a single large living cavern mound of massive brown chitin plates, two huge curved tusks framing a dark cave mouth at its front lit amber inside, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The cave mouth glows steadily and thin vapour rises gently. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```


## Appendix B: clip prompts and negative prompts, by identity

### `grav_harvester` (Levitation Drone, Enclave, aircraft)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, light shafts, beam of light from above, volumetric light, haze
```

**C-001 Idle** (1005 characters, `prompt_sha256` `11f36182c166dab12dea2e97578d9ec3c899dcf14fd84e79b9d56de0fe28146b`):

```text
Levitation Drone: a single small silver alien worker drone, a rounded teardrop body with one large cyan crystal eye at its front, two short golden grippers and a soft cyan glow under it, held still, seen from above at a three-quarter angle looking down on its top, front toward the lower right, the whole drone in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the drone keeps the same size and place in frame. The drone hangs still in the air: it never turns, tilts or bobs, and only its crystal eye and small golden lights glow and blink softly. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-002 Walk** (861 characters, `prompt_sha256` `4b9c13da6e180a03159b9257aa3fd3c96e837000423d011a93f165df7a66d94b`):

```text
Levitation Drone: the same small silver drone gliding level and forward, flying in place in the middle of the frame and fully visible, no other machines. Walk animation, one static tripod shot: the camera never moves, zooms or pans, and the drone flies forward in place, as if held in a wind tunnel, keeping the same size, pose and place in frame. From the first frame to the last the glow under it brightens and thin pale cyan light trails stream back from it toward the upper left, flickering at an even pace; no ground contact. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-003 Attack** (701 characters, `prompt_sha256` `0026e2f8f44ea6c647ebb61ba4f0698d0984b0963ba2cd98c28eb851912d166a`):

```text
Levitation Drone: the same small silver drone zaps a short bright cyan electric spark forward from its crystal eye. Attack animation, one static tripod shot: the camera never moves, zooms or pans, and the drone keeps its place in frame, hovering. A short bright cyan electric spark leaps from its crystal eye straight toward the lower right and out of the frame, then the spark fades and it hovers in its starting pose. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-004 Cast** (821 characters, `prompt_sha256` `f86dce71b60990c25fbdea7cf2f5d99e9af3ee0a8b8a645980d5fa4d5c02d82c`):

```text
Levitation Drone: the same small silver drone holds a steady thin green extraction beam from its crystal eye toward a point just outside the frame at the lower right. Cast animation, one static tripod shot: the camera never moves, zooms or pans, and the drone keeps the same size, pose and place in frame. Its crystal eye holds a steady thin green extraction beam toward a point just outside the frame at the lower right, with small showering sparks, while its body and grippers hold still. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `nectar_worker` (Forager Ant, Swarm, creature)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, head turning, nodding, twitching, colour change, floor, cast shadow
```

**C-005 Idle** (1008 characters, `prompt_sha256` `e152607d1be563327a10652c1cadb1c7d453edac88c591dc4e2a08492eec246e`):

```text
Forager Ant: a single large worker ant of iridescent dark green chitin, a big head with broad mandibles, two short antennae with amber tips, six legs, standing still, seen from above at a three-quarter angle, head toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the ant keeps the same size, pose and place in frame. It stands as still as a statue: its body, head, antennae, legs and mandibles never move, turn or nod; only the amber tips of its antennae glow, and its colours never change. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-006 Walk** (881 characters, `prompt_sha256` `a091e9dae87c4c11d73367e4f1a0b33ae7a6d523f8e7951453bdb4816a32022f`):

```text
Forager Ant: the same large green worker ant scuttling quickly on its six legs, staying in place in the middle of the frame and fully visible. Walk animation, one static tripod shot: the camera never moves, zooms or pans. The ant scuttles on a treadmill that holds it in place: its six legs clearly lift and step in turn at an even pace from the first frame to the last, while its body stays at exactly the same place, size and three-quarter facing in frame; it never moves forward, back or toward the camera. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-007 Attack** (672 characters, `prompt_sha256` `502ceb91dc9d00d8f5dc700ce266f082b67529048120c8b16c0cd4b1520484fb`):

```text
Forager Ant: the same large green worker ant snaps its broad mandibles forward in a quick bite. Attack animation, one static tripod shot: the camera never moves, zooms or pans, and the ant keeps its place in frame. It lunges and snaps its broad mandibles shut twice toward the lower right, then settles smoothly back to its starting pose. The plain dark backdrop stays unchanged throughout. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-008 Cast** (766 characters, `prompt_sha256` `a5c90ff56de77cafb533444b8b8ef113f872cf542ff001162420c8c8b6b823ef`):

```text
Forager Ant: the same large green worker ant works its mandibles at a point just outside the frame at the lower right. Cast animation, one static tripod shot: the camera never moves, zooms or pans, and the ant keeps the same size, pose and place in frame. Its mandibles open and close in a quick even rhythm at a point just outside the frame at the lower right, a soft amber glow between them, while its body, head and legs hold still. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `citadel_nexus_dome` (Citadel Nexus Dome, Enclave, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-009 Idle** (1006 characters, `prompt_sha256` `e6f88f144b35c82adc9eed2b0da244f04f53935a53ac1885a9854a8d8be0fde9`):

```text
Citadel Nexus Dome: a single large golden alien pyramid hub on a broad octagonal base, curved gold plates with violet seams, a huge cyan crystal in its apex, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The crystal glows steadily and small violet lights flicker along the seams. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-010 Constructing** (914 characters, `prompt_sha256` `6ccd148c1242ea4e92b7f9547e5dcb2f4c12e7054e66f528e77849dc31f2ba92`):

```text
Citadel Nexus Dome: a single large golden alien pyramid hub on a broad octagonal base, curved gold plates with violet seams, a huge cyan crystal in its apex, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `ancient_tree_dome` (Ancient Tree Dome, Swarm, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-011 Idle** (992 characters, `prompt_sha256` `cd6c4d04e59857108120567d98f8094257c2dca86dc99d69e3e907fba2c1f101`):

```text
Ancient Tree Dome: a single large living hive mound like a squat hollow tree stump of ridged bark and dark brown chitin, amber egg sacs around its wide top, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The egg sacs glow steadily and thin amber vapour rises gently. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-012 Constructing** (913 characters, `prompt_sha256` `ea79dba63586b189b921821aac8b12f10c29fe1f2b60a974f6d81285870b6c1b`):

```text
Ancient Tree Dome: a single large living hive mound like a squat hollow tree stump of ridged bark and dark brown chitin, amber egg sacs around its wide top, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `psi_pylon_dome` (Psi Pylon Dome, Enclave, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-013 Idle** (995 characters, `prompt_sha256` `b3078f250291ee52c9e82407f415b404059f75819b3f195cdb3d5d0b5f51704f`):

```text
Psi Pylon Dome: a single small alien power pylon, one large upright cyan crystal shard held in a squat round golden base by four short curved golden prongs, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The crystal glows steadily and faint cyan motes drift up from it. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-014 Constructing** (913 characters, `prompt_sha256` `c8807e75171e54f2d08288b3b47eff16f9edc3f0826cf5941e5606f0e2c1959a`):

```text
Psi Pylon Dome: a single small alien power pylon, one large upright cyan crystal shard held in a squat round golden base by four short curved golden prongs, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `amber_pollen_dome` (Amber Spore Balloon, Swarm, flyer)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, light shafts, tractor beam, beam of light from above, volumetric light, haze, light streaks, head turning, nodding, twitching, colour change, floor, cast shadow
```

**C-015 Idle** (960 characters, `prompt_sha256` `dc7cc62e285b4d58d5f737a73e295f01279b565f0ceed3c5ec97e51f3667d04f`):

```text
Amber Spore Balloon: a single huge gas-sac creature, a round translucent amber balloon body with glowing veins, a small dark chitin head plate at its front and a fringe of short tentacles hanging below it, seen from above at a three-quarter angle, head toward the lower right, the whole creature in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the creature keeps the same size and place in frame. It holds still: it never turns, tilts or bobs and its tentacles hang still; only its veins glow softly. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-016 Walk** (819 characters, `prompt_sha256` `86059d7de83045c149ac4c34ef836bcb5712c90fd7a540d514f114c32be37758`):

```text
Amber Spore Balloon: the same amber balloon creature drifting slowly forward in place, fully visible in the middle of the frame, no other creatures. Walk animation, one static tripod shot: the camera never moves, zooms or pans, and the creature glides forward in place, as if held in a wind tunnel, keeping the same size, place and three-quarter facing in frame: its hanging tentacles sway back in slow even waves from the first frame to the last. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `rift_siphon_dome` (Rift Siphon Dome, Enclave, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-017 Idle** (1003 characters, `prompt_sha256` `6396f15e42f635be487b3f8eba146be9f9d9676ca1b407c379e3ac78d0168d7d`):

```text
Rift Siphon Dome: a single squat golden alien extractor dome capping a gas vent, curved gold plates around a violet opening on its top, small cyan crystals, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The opening glows steadily and a thin wisp of green vapour rises from it. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-018 Constructing** (913 characters, `prompt_sha256` `b7373cd1b37f153940be069041d8b6a7f28ae5e36b671ef2d5742128279c9de9`):

```text
Rift Siphon Dome: a single squat golden alien extractor dome capping a gas vent, curved gold plates around a violet opening on its top, small cyan crystals, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `spore_vent_dome` (Spore Vent Dome, Swarm, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-019 Idle** (1004 characters, `prompt_sha256` `0b0f5a286b8e0b1e3eb46f11d8abcef2c28984c2039a5a3f720137d8ee20cd21`):

```text
Spore Vent Dome: a single squat living extractor capping a gas vent, a bulbous funnel of dark brown chitin and bark with a wide green mouth, amber pores, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. A thin wisp of green vapour rises from the mouth and the pores glow steadily. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-020 Constructing** (910 characters, `prompt_sha256` `9472857cc1bb566f3c1b0a2192535d25a682d713d8e64e07b3bb137315205d4d`):

```text
Spore Vent Dome: a single squat living extractor capping a gas vent, a bulbous funnel of dark brown chitin and bark with a wide green mouth, amber pores, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `warp_gate_dome` (Warp Gate Dome, Enclave, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-021 Idle** (1004 characters, `prompt_sha256` `d73184f75d59a0c52481b6aaf270fd7484a97316b8b0044bc3cde479da6e4e10`):

```text
Warp Gate Dome: a single golden alien gateway, a thick round golden arch on a low curved base, a sheet of blue-violet energy filling the arch, a cyan crystal on each side, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The energy in the arch glows steadily and shimmers faintly. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-022 Constructing** (928 characters, `prompt_sha256` `a5c02fa072ced86a355cfb9444102d247a85c393cdbd26c5dbe8a29fd5106fce`):

```text
Warp Gate Dome: a single golden alien gateway, a thick round golden arch on a low curved base, a sheet of blue-violet energy filling the arch, a cyan crystal on each side, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `spawning_hollow_dome` (Spawning Hollow Dome, Swarm, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-023 Idle** (972 characters, `prompt_sha256` `f978bb22a379ca6da5b6867853671fda8ab56f76374035495381f510df161a28`):

```text
Spawning Hollow Dome: a single living nest, a low round basin of curved bark ribs and brown chitin holding a pool of glowing amber fluid, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The fluid glows steadily and small bubbles rise slowly in it. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-024 Constructing** (894 characters, `prompt_sha256` `4282ba987a032b1100c41fa313d2a8caa2698b655393e4f097839ddb687d20e7`):

```text
Spawning Hollow Dome: a single living nest, a low round basin of curved bark ribs and brown chitin holding a pool of glowing amber fluid, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `warp_strider` (Phase Tripod, Enclave, walker)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, floor, cast shadow, turning toward the camera
```

**C-025 Idle** (1007 characters, `prompt_sha256` `b905259399252816a114cb6f02827ae446172dd3cb9bcb97feab7051d58c3428`):

```text
Phase Tripod: a single sleek alien three-legged walker, a compact silver-and-violet armoured body on three long jointed legs, a short cannon under its front, a cyan eye, standing still, seen from above at a three-quarter angle, front toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the walker keeps the same size, pose and place in frame. It stands still, keeping its three-quarter view: it never turns toward the camera, sways or shifts, and its head never turns or nods; only its cyan eye and small violet lights glow steadily. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-026 Walk** (827 characters, `prompt_sha256` `ba8b6ee78f385011f820faa07edef037608c9296dfaf85d20f394976badc2597`):

```text
Phase Tripod: the same three-legged walker striding briskly, its three long legs stepping in turn. Walk animation, one static tripod shot: the camera never moves, zooms or pans, and the walker never turns, leans or steps toward the camera. It strides briskly on the spot as if on a treadmill: its three long legs lift and step in turn at an even pace from the first frame to the last, while its body keeps the same place, size and three-quarter facing in frame. Nothing else appears in the frame. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-027 Attack** (759 characters, `prompt_sha256` `547d3eb4d585eee2444606ed4691048e04669da5c08e9ed6e24ce5ab28b7f62c`):

```text
Phase Tripod: the same three-legged walker fires a bright cyan particle bolt from the cannon under its front. Attack animation, one static tripod shot: the camera never moves, zooms or pans, and the walker keeps its place in frame. It fires one bright cyan particle bolt from the cannon under its front straight toward the lower right and out of the frame with a short recoil, then settles smoothly back to its starting pose. The plain dark backdrop stays unchanged throughout. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `razor_mantis` (Razor Mantis, Swarm, creature)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, head turning, nodding, twitching, colour change, floor, cast shadow
```

**C-028 Idle** (1002 characters, `prompt_sha256` `092ed7b828324f7f856e64eb0e21a10a81bb982e4dd5c9d0a6403e521d69412d`):

```text
Razor Mantis: a single small lean predatory mantis of emerald green chitin, two long serrated scythe forelegs folded in front, four thin legs, a narrow head with amber eyes, crouched, seen from above at a three-quarter angle, head toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the mantis keeps the same size, pose and place in frame. It crouches as still as a statue: its body, head, scythes and legs never move, turn or nod; only its amber eyes glow, and its colours never change. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-029 Walk** (888 characters, `prompt_sha256` `1c66d2edb70994f27cb738188bd8bfc80580d35319b6b66d683953b7cabd302f`):

```text
Razor Mantis: the same small green mantis sprinting low on its four thin legs, staying in place in the middle of the frame and fully visible. Walk animation, one static tripod shot: the camera never moves, zooms or pans. The mantis sprints on a treadmill that holds it in place: its four thin legs clearly lift and step in turn at an even pace from the first frame to the last, while its body stays at exactly the same place, size and three-quarter facing in frame; it never moves forward, back or toward the camera. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-030 Attack** (666 characters, `prompt_sha256` `583522e380fed19e6fc14fe3bf89d72b6303413f11798a225edaf08a6ccb6fe7`):

```text
Razor Mantis: the same small green mantis slashes with both serrated scythe forelegs. Attack animation, one static tripod shot: the camera never moves, zooms or pans, and the mantis keeps its place in frame. It slashes twice with both serrated scythe forelegs toward the lower right, then settles smoothly back to its starting pose. The plain dark backdrop stays unchanged throughout. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `astral_vagrant` (Astral Projecting Grey, Enclave, infantry)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, head turning, nodding, floor, cast shadow
```

**C-031 Idle** (1002 characters, `prompt_sha256` `56ec5e53ab8932ac293b281fee801a433beafcbfc2b317f5be200a5f7b25c9b9`):

```text
Astral Projecting Grey: a single slender grey-skinned alien warrior with a large bald head and big violet eyes, silver armour with violet trim, a curved violet glaive held low, standing still, seen from above at a three-quarter angle, front toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the warrior keeps the same size, pose and place in frame. It stands still, keeping its three-quarter view: it never turns toward the camera, sways or shifts, and its head never turns or nods; only its eyes and the glaive glow steadily. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-032 Walk** (811 characters, `prompt_sha256` `25514a09426cecca1dc571dcadea8aec9a4139f848630432402a526cf46c6de9`):

```text
Astral Projecting Grey: the same slender Grey warrior running lightly with its glaive held low. Walk animation, one static tripod shot: the camera never moves, zooms or pans, and the warrior never turns, leans or steps toward the camera. It runs lightly on the spot as if on a treadmill: its legs lift and step in turn at an even pace from the first frame to the last, while its body keeps the same place, size and three-quarter facing in frame. Nothing else appears in the frame. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-033 Attack** (750 characters, `prompt_sha256` `4bc09872d86a6fa7837f372041cce66127bab85bed5d6235060ee87463d8f709`):

```text
Astral Projecting Grey: the same slender Grey warrior throws a glowing violet glaive blade forward. Attack animation, one static tripod shot: the camera never moves, zooms or pans, and the warrior keeps its place in frame. It swings its arm and throws a glowing violet glaive blade straight toward the lower right and out of the frame; a new glaive glows in its hand, then settles smoothly back to its starting pose. The plain dark backdrop stays unchanged throughout. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `prism_weaver` (Force-Matrix Grey, Enclave, aircraft)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, light shafts, beam of light from above, volumetric light, haze, rotating rings, spinning rings
```

**C-034 Idle** (1003 characters, `prompt_sha256` `d03c5baba20cf3c2e6473d55ce858b7b478432fee3761a8839b193e67ccc0f77`):

```text
Force-Matrix Grey: a single small alien support drone, a smooth silver sphere with one large cyan eye inside two thin fixed golden rings set with small violet crystals, held still, seen from above at a three-quarter angle looking down on its top, front toward the lower right, the whole drone in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the drone keeps the same size and place in frame. The drone hangs still in the air: it never turns, tilts or bobs, and only its cyan eye and the violet crystals on its fixed rings glow and blink softly. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-035 Walk** (867 characters, `prompt_sha256` `d3813e3b6a97d7a7bb9e24823accb9d52b7856635f3c352b853787acf26be74d`):

```text
Force-Matrix Grey: the same silver sphere drone in its two fixed golden rings gliding level and forward, flying in place in the middle of the frame and fully visible. Walk animation, one static tripod shot: the camera never moves, zooms or pans, and the drone flies forward in place, as if held in a wind tunnel, keeping the same size, pose and place in frame. From the first frame to the last its eye glows bright and thin pale cyan light trails stream back from it toward the upper left, flickering at an even pace; no ground contact. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-036 Attack** (669 characters, `prompt_sha256` `4e2e7bd398167960ba1176ff5c44d83502c92614b7c3ce552de615991e4547dc`):

```text
Force-Matrix Grey: the same silver sphere drone holds a thin steady azure beam forward from its eye. Attack animation, one static tripod shot: the camera never moves, zooms or pans, and the drone keeps its place in frame, hovering. A thin steady azure beam streams from its eye straight toward the lower right and out of the frame, then the beam fades and it hovers in its starting pose. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `stealth_eye_orb` (Stealth Eye Orb, Enclave, aircraft)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, light shafts, tractor beam, beam of light from above, volumetric light, haze, light streaks
```

**C-037 Idle** (963 characters, `prompt_sha256` `93e11ef3bbc3c3930e74d683f3da20b6bad954a37f9826551bc29e6790c003fb`):

```text
Stealth Eye Orb: a single small alien scout probe, a smooth dark silver oval body with one large glowing cyan lens at its front and two short fixed fins, held still, seen from above at a three-quarter angle looking down on its top, front toward the lower right, the whole probe in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the probe keeps the same size and place in frame. The probe hangs still in the air: it never turns, tilts or bobs, and only its cyan lens and small lights glow and blink softly. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-038 Walk** (840 characters, `prompt_sha256` `7a9eb7c6355c705341f29c0ad1a29dc2c054692c57e584a05bcd6ec0d1a827f7`):

```text
Stealth Eye Orb: the same small dark silver probe gliding level and forward, flying in place in the middle of the frame and fully visible. Walk animation, one static tripod shot: the camera never moves, zooms or pans, and the probe flies forward in place, as if held in a wind tunnel, keeping the same size, pose and place in frame. From the first frame to the last its lens glows bright and thin pale cyan light trails stream back from it toward the upper left, flickering at an even pace; no ground contact. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `phase_prism_saucer` (Phase Transporter, Enclave, aircraft)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, light shafts, tractor beam, beam of light from above, volumetric light, haze, light streaks
```

**C-039 Idle** (1003 characters, `prompt_sha256` `7cea64f575ea1402d55a722c4bd27b61586cb338c9065d30ab55c8322b95db3c`):

```text
Phase Transporter: a single alien transport craft, a long faceted violet crystal hull set in a curved golden frame, two fixed golden side pods and a glowing cyan core inside the crystal, held still, seen from above at a three-quarter angle looking down on its top, front toward the lower right, the whole craft in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the craft keeps the same size and place in frame. The craft hangs still in the air: it never turns, tilts or bobs, and only its cyan core and small golden lights glow and blink softly. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-040 Walk** (862 characters, `prompt_sha256` `cc26222fc0b80f02d14938819cde559d077e801cb1def1116326c395bcc2c828`):

```text
Phase Transporter: the same violet crystal transport in level forward flight, flying in place in the middle of the frame and fully visible, no other aircraft. Walk animation, one static tripod shot: the camera never moves, zooms or pans, and the craft flies forward in place, as if held in a wind tunnel, keeping the same size, pose and place in frame. From the first frame to the last its core glows bright and long thin violet light trails stream back from it toward the upper left, flickering at an even pace; no ground contact. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `void_goliath` (Hardened Shield Walker, Enclave, walker)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, floor, cast shadow, turning toward the camera
```

**C-041 Idle** (1011 characters, `prompt_sha256` `e38d6ce1254de041138cfcb09e97811ee1b6ff6a98bbcbf7d808dca311d9ddfd`):

```text
Hardened Shield Walker: a single heavy alien four-legged walker, a broad silver-and-gold armoured body on four thick jointed legs, two long forward cannons with blue muzzles, standing still, seen from above at a three-quarter angle, front toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the walker keeps the same size, pose and place in frame. It stands still, keeping its three-quarter view: it never turns toward the camera, sways or shifts, and its head never turns or nods; only its cannon muzzles and small lights glow steadily. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-042 Walk** (831 characters, `prompt_sha256` `bdfc6ae93cf243afc4526a5cf17c75739344f6fcd7cfa5e32e98b16095a4b5c4`):

```text
Hardened Shield Walker: the same heavy four-legged walker marching steadily, its four legs stepping in turn. Walk animation, one static tripod shot: the camera never moves, zooms or pans, and the walker never turns, leans or steps toward the camera. It marches heavily on the spot as if on a treadmill: its four legs lift and step in turn at an even pace from the first frame to the last, while its body keeps the same place, size and three-quarter facing in frame. Nothing else appears in the frame. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-043 Attack** (772 characters, `prompt_sha256` `b33589876b9adebd0508d158f3bf62c697fed359653f7a975d95d278abc85464`):

```text
Hardened Shield Walker: the same heavy four-legged walker fires both disruptor cannons with heavy blue plasma bolts. Attack animation, one static tripod shot: the camera never moves, zooms or pans, and the walker keeps its place in frame. It fires both disruptor cannons: a short heavy recoil and two bright blue plasma bolts streaking straight toward the lower right and out of the frame, then settles smoothly back to its starting pose. The plain dark backdrop stays unchanged throughout. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `flying_saucer` (Roswell Flying Saucer, Enclave, aircraft)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, light shafts, tractor beam, beam of light from above, volumetric light, haze, light streaks
```

**C-044 Idle** (1011 characters, `prompt_sha256` `fddc0bc3b1ec4bc22c1ca1e222340468fc681addbe5c43e2c55e75b023aad2d1`):

```text
Roswell Flying Saucer: a single sleek silver saucer fighter, a thin flat disc with a small clear cockpit dome, fixed violet lights around its rim and a violet emitter lens at its front edge, held still, seen from above at a three-quarter angle looking down on its top, front toward the lower right, the whole saucer in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the saucer keeps the same size and place in frame. The saucer hangs still in the air: it never turns, tilts or bobs, and only its violet rim lights and emitter lens glow and blink softly. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-045 Walk** (897 characters, `prompt_sha256` `e4c9a99a4f6c5bb607d1ffd17386e2d51d7976dd3a5c42d951049a57712a9af6`):

```text
Roswell Flying Saucer: the same sleek silver saucer in fast level forward flight, its rim lights glowing bright, flying in place in the middle of the frame and fully visible, no other aircraft. Walk animation, one static tripod shot: the camera never moves, zooms or pans, and the saucer flies forward in place, as if held in a wind tunnel, keeping the same size, pose and place in frame. From the first frame to the last its rim lights blaze and long thin violet light trails stream back from it toward the upper left, flickering at an even pace; no ground contact. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-046 Attack** (720 characters, `prompt_sha256` `8f6108f362d56197dd96015b83066d51dc04aa24d417cdf20bee2e33fb432839`):

```text
Roswell Flying Saucer: the same sleek silver saucer fires a thin steady violet beam forward from the emitter at its front edge. Attack animation, one static tripod shot: the camera never moves, zooms or pans, and the saucer keeps its place in frame, hovering. A thin steady violet beam streams from the emitter at its front edge straight toward the lower right and out of the frame, then the beam fades and it hovers in its starting pose. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `prismatic_saucer` (Prismatic Saucer, Enclave, aircraft)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, light shafts, tractor beam, beam of light from above, volumetric light, haze, light streaks
```

**C-047 Idle** (987 characters, `prompt_sha256` `ce5cada45addd3e07c81742fb3fa0dc05d0a8ede13e98f5e689ddaeea3d20c44`):

```text
Prismatic Saucer: a single alien warship, a long flat gold-and-silver arrowhead hull with three fixed curved prongs at its front around a glowing white-violet prism core, held still, seen from above at a three-quarter angle looking down on its top, front toward the lower right, the whole warship in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the warship keeps the same size and place in frame. The warship hangs still in the air: it never turns, tilts or bobs, and only its prism core and small lights glow and blink softly. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-048 Walk** (874 characters, `prompt_sha256` `6b923c936ef51a358a3b30fb9de0167a9262f4ba970b4ae401af857bf09781ce`):

```text
Prismatic Saucer: the same golden arrowhead warship in steady level forward flight, flying in place in the middle of the frame and fully visible, no other aircraft. Walk animation, one static tripod shot: the camera never moves, zooms or pans, and the warship flies forward in place, as if held in a wind tunnel, keeping the same size, pose and place in frame. From the first frame to the last its drive glows bright cyan and long thin cyan light trails stream back from it toward the upper left, flickering at an even pace; no ground contact. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-049 Attack** (719 characters, `prompt_sha256` `9062afc1800f705bd2458bcbef426ea9f10f4ad91e8b5b753ace8a88207cc4c7`):

```text
Prismatic Saucer: the same golden arrowhead warship fires a violet beam forward from its prism core. Attack animation, one static tripod shot: the camera never moves, zooms or pans, and the warship keeps its place in frame, hovering. A violet beam streams from its prism core, growing from a thin thread to a thick bright beam, straight toward the lower right and out of the frame, then the beam fades and it hovers in its starting pose. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `shadow_visitor` (Cloaked Abductor, Enclave, infantry)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, head turning, nodding, floor, cast shadow
```

**C-050 Idle** (1007 characters, `prompt_sha256` `97be8ffe01ecf3b7753ffb5fe27aa24d0e148201827e6c5992f486b42f7709cc`):

```text
Cloaked Abductor: a single tall lean grey-skinned alien assassin in close dark violet armour and a hood, two green eyes under the hood, a long curved green crystal scythe held low, standing still, seen from above at a three-quarter angle, front toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the assassin keeps the same size, pose and place in frame. It stands still, keeping its three-quarter view: it never turns toward the camera, sways or shifts, and its head never turns or nods; only its eyes and the scythe glow steadily. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-051 Walk** (840 characters, `prompt_sha256` `735d3fb052fd227acfc45d478e91efcac066c0aa7165eb39ef8862f7e6bff0c5`):

```text
Cloaked Abductor: the same hooded assassin stalking forward with long quiet strides, its scythe held low. Walk animation, one static tripod shot: the camera never moves, zooms or pans, and the assassin never turns, leans or steps toward the camera. It stalks with long quiet strides on the spot as if on a treadmill: its legs lift and step in turn at an even pace from the first frame to the last, while its body keeps the same place, size and three-quarter facing in frame. Nothing else appears in the frame. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-052 Attack** (719 characters, `prompt_sha256` `a0e4405a07e3dbc5aedbf8bfb18bec39aeb487b5e9bf985cbb94731faf4855e9`):

```text
Cloaked Abductor: the same hooded assassin cleaves forward with its long curved green crystal scythe. Attack animation, one static tripod shot: the camera never moves, zooms or pans, and the assassin keeps its place in frame. It sweeps its long green scythe in one fast cleave toward the lower right, leaving a short green arc of light, then settles smoothly back to its starting pose. The plain dark backdrop stays unchanged throughout. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `cerebral_elder` (Mind-Storm Elder, Enclave, infantry)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, head turning, nodding, floor, cast shadow
```

**C-053 Idle** (1011 characters, `prompt_sha256` `112814d6c6791d8238db2632d6274eda5c4ec7b77619f8f920a905d6651ea65e`):

```text
Mind-Storm Elder: a single ancient grey-skinned alien elder with a huge bald head and glowing violet eyes, in long gold-and-violet robes, seated cross-legged on a small flat golden disc, its hands resting on its knees, seen from above at a three-quarter angle, front toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the elder keeps the same size, pose and place in frame from start to end. It sits on its disc as still as a statue: its head, hands and robes never move, turn or nod, and the disc never bobs; only its eyes glow softly. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-054 Walk** (842 characters, `prompt_sha256` `285eb6cffc1db53d2deb9ed24c3596e4073e6a852b90e8d9c8b6a9aee022603f`):

```text
Mind-Storm Elder: the same robed elder seated on its golden disc, gliding forward in place in the middle of the frame and fully visible. Walk animation, one static tripod shot: the camera never moves, zooms or pans, and the elder glides forward in place on its disc, as if held in a wind tunnel, keeping the same size, pose, place and three-quarter facing in frame. From the first frame to the last its robes ripple back toward the upper left at an even pace and a faint violet glow streams back under the disc. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-055 Attack** (774 characters, `prompt_sha256` `87592b989c1d2082b475f04697d7cf235abe3cf948601ab0a317e3b85a7623d9`):

```text
Mind-Storm Elder: the same robed elder raises both hands and a short crackling violet psionic bolt leaps from its palms. Attack animation, one static tripod shot: the camera never moves, zooms or pans, and the elder keeps its place in frame. It raises both hands and a short crackling violet psionic bolt leaps from its palms straight toward the lower right and out of the frame, then it lowers its hands smoothly back to its starting pose. The plain dark backdrop stays unchanged throughout. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `tripod_annihilator` (Thermal Tripod, Enclave, walker)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, floor, cast shadow, turning toward the camera
```

**C-056 Idle** (1004 characters, `prompt_sha256` `fc57fd1a5168f5da6f978b163677c755e4f6edaa48066e3a661dd5bc0a5ed1d2`):

```text
Thermal Tripod: a single tall alien war walker, a compact gold-and-silver body with an orange lens on each side on three very long thin legs, two heat lances under it, standing still, seen from above at a three-quarter angle, front toward the lower right, whole from body to feet and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the walker keeps the same size, pose and place in frame. It stands still, keeping its three-quarter view: it never turns toward the camera, sways or shifts, and its head never turns or nods; only its orange lenses glow steadily. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-057 Walk** (831 characters, `prompt_sha256` `0b040cb2a43e434b8b7949bb51922462015e4090a339233282f086b5fd3e2fb6`):

```text
Thermal Tripod: the same tall three-legged walker striding with long slow steps of its three long legs. Walk animation, one static tripod shot: the camera never moves, zooms or pans, and the walker never turns, leans or steps toward the camera. It strides slowly on the spot as if on a treadmill: its three long legs lift and step in turn at an even pace from the first frame to the last, while its body keeps the same place, size and three-quarter facing in frame. Nothing else appears in the frame. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-058 Attack** (781 characters, `prompt_sha256` `deedfa781ef3e496c7554549716baa27c6c898ae63a59e252e9d5384ab344a21`):

```text
Thermal Tripod: the same tall three-legged walker fires two thin orange heat beams forward from the lances under its body. Attack animation, one static tripod shot: the camera never moves, zooms or pans, and the walker keeps its place in frame. It fires two thin straight orange heat beams from the lances under its body toward the lower right and out of the frame, the beams fading after a moment, then settles smoothly back to its starting pose. The plain dark backdrop stays unchanged throughout. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `volatile_tick` (Acid-Burst Beetle, Swarm, creature)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, head turning, nodding, twitching, colour change, floor, cast shadow
```

**C-059 Idle** (991 characters, `prompt_sha256` `b0ee964af9170a4aab992e8ddebebfaf2b707535bc627a64f3c56e70fa09119d`):

```text
Acid-Burst Beetle: a single small round tick-like beetle with a swollen translucent bright green acid sac on its back, dark brown chitin plates, six short legs, crouched low, seen from above at a three-quarter angle, head toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the tick keeps the same size, pose and place in frame. It crouches as still as a statue: its body, head, acid sac and legs never move, turn or nod; only the acid sac glows, and its colours never change. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-060 Walk** (893 characters, `prompt_sha256` `7324545897776270a309f479e84bbac6feaaeff370ec9abb42ec36ad12b194b0`):

```text
Acid-Burst Beetle: the same small acid tick scuttling quickly on its six short legs, staying in place in the middle of the frame and fully visible. Walk animation, one static tripod shot: the camera never moves, zooms or pans. The tick scuttles on a treadmill that holds it in place: its six short legs clearly lift and step in turn at an even pace from the first frame to the last, while its body stays at exactly the same place, size and three-quarter facing in frame; it never moves forward, back or toward the camera. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-061 Attack** (736 characters, `prompt_sha256` `e0030309f7ddba85214beb9198e04f4b9ffe115a31f95ffc9fd61391c436a2f7`):

```text
Acid-Burst Beetle: the same small acid tick swells and sprays a burst of glowing green acid forward. Attack animation, one static tripod shot: the camera never moves, zooms or pans, and the tick keeps its place in frame. Its acid sac swells and flares bright and a burst of glowing green acid sprays straight toward the lower right and out of the frame, then settles smoothly back to its starting pose. The plain dark backdrop stays unchanged throughout. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `thorn_viper` (Spine-Spitter Cobra, Swarm, creature)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, head turning, nodding, twitching, colour change, floor, cast shadow
```

**C-062 Idle** (1007 characters, `prompt_sha256` `68d7189c76ff0fb5c523be0a5de3b34737e3994cf7873cfa07c0313389fb72f3`):

```text
Spine-Spitter Cobra: a single serpent creature rearing upright, a coiled body of dark green scales and brown chitin plates, a wide hood of bone spines, two small clawed arms, amber eyes, seen from above at a three-quarter angle, head toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the serpent keeps the same size, pose and place in frame. It rears as still as a statue: its body, head, hood, arms and coils never move, turn or nod; only its amber eyes glow, and its colours never change. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-063 Walk** (886 characters, `prompt_sha256` `db44b6a6ddc3cd00d803cfdd65a1a4bee9b0ef08083b85c24525eba9b47f602b`):

```text
Spine-Spitter Cobra: the same rearing spined serpent slithering forward, staying in place in the middle of the frame and fully visible. Walk animation, one static tripod shot: the camera never moves, zooms or pans. The serpent slithers on a treadmill that holds it in place: its long coiled body ripples in even S-shaped waves from the first frame to the last, while its raised head and hood stay at exactly the same place, size and three-quarter facing in frame; it never moves forward, back or toward the camera. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-064 Attack** (722 characters, `prompt_sha256` `ee53050d2f45b726f471d46adfc4db7b851ba90b3616fbf3cd0f619479e7c2f2`):

```text
Spine-Spitter Cobra: the same rearing spined serpent spits a quick volley of bone spines from its hood. Attack animation, one static tripod shot: the camera never moves, zooms or pans, and the serpent keeps its place in frame. It flares its hood and spits a quick volley of bone spines straight toward the lower right and out of the frame, then settles smoothly back to its starting pose. The plain dark backdrop stays unchanged throughout. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `mortar_stag` (Bile-Artillery Beetle, Swarm, creature)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, head turning, nodding, twitching, colour change, floor, cast shadow
```

**C-065 Idle** (1010 characters, `prompt_sha256` `c11cde92bab2a94120536a823f38a4d075e7f4ba29f277e4ba3e37cd1ec58a2b`):

```text
Bile-Artillery Beetle: a single large stag beetle of dark brown chitin on long legs, two curved horns at its front, a tall curved tube cannon on its back glowing orange at its mouth, seen from above at a three-quarter angle, head toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the beetle keeps the same size, pose and place in frame. It stands as still as a statue: its body, head, horns, cannon and legs never move, turn or nod; only the cannon's mouth glows, and its colours never change. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-066 Walk** (913 characters, `prompt_sha256` `ea320540844a52aa36e6ad66772219c5f661ae3efc9bfc5262a36e099f5ca4fc`):

```text
Bile-Artillery Beetle: the same horned artillery beetle scuttling on its long legs, its cannon held up, staying in place in the middle of the frame and fully visible. Walk animation, one static tripod shot: the camera never moves, zooms or pans. The beetle scuttles on a treadmill that holds it in place: its six long legs clearly lift and step in turn at an even pace from the first frame to the last, while its body stays at exactly the same place, size and three-quarter facing in frame; it never moves forward, back or toward the camera. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-067 Attack** (759 characters, `prompt_sha256` `ee05476a42124e28afbc005e384204c3fb6418c741591715ef978dc98260b2c8`):

```text
Bile-Artillery Beetle: the same horned artillery beetle lobs a glowing orange glob of bile from the cannon on its back. Attack animation, one static tripod shot: the camera never moves, zooms or pans, and the beetle keeps its place in frame. It braces and the cannon on its back lobs a glowing orange glob of bile in a high arc forward and out of the frame at the upper right, then settles smoothly back to its starting pose. The plain dark backdrop stays unchanged throughout. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `hive_matron` (Hive Matron, Swarm, creature)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, head turning, nodding, twitching, colour change, floor, cast shadow
```

**C-068 Idle** (1015 characters, `prompt_sha256` `e91ad7b23adcc086359157e39d5dd6c00e589f0466fad0606e51e5fad4f7def3`):

```text
Hive Matron: a single tall regal swarm creature on six thin legs, an upright dark green chitin body, a crowned head, two long talon arms folded in front, amber sacs on its back, seen from above at a three-quarter angle, front toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the creature keeps the same size, pose and place in frame. It stands as still as a statue: its body, head, crown, talons and legs never move, turn or nod; only the amber sacs on its back glow, and its colours never change. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-069 Walk** (888 characters, `prompt_sha256` `a385c6ed2af398bb1d335766d36a871692bc5255843fa3dac55e19a3a53aa05c`):

```text
Hive Matron: the same tall crowned swarm creature walking on its six thin legs, staying in place in the middle of the frame and fully visible. Walk animation, one static tripod shot: the camera never moves, zooms or pans. The creature walks on a treadmill that holds it in place: its six thin legs clearly lift and step in turn at an even pace from the first frame to the last, while its body stays at exactly the same place, size and three-quarter facing in frame; it never moves forward, back or toward the camera. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-070 Attack** (672 characters, `prompt_sha256` `f1731be28e0f15a151062d13a287b5949b97d4306c6f5db3975bffecaa6550eb`):

```text
Hive Matron: the same tall crowned swarm creature slashes forward with its long talon arms. Attack animation, one static tripod shot: the camera never moves, zooms or pans, and the creature keeps its place in frame. It slashes forward twice with its long talon arms toward the lower right, then settles smoothly back to its starting pose. The plain dark backdrop stays unchanged throughout. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `sonic_bat` (Glaive Wasp, Swarm, flyer)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, light shafts, tractor beam, beam of light from above, volumetric light, haze, light streaks, head turning, nodding, twitching, colour change, floor, cast shadow
```

**C-071 Idle** (948 characters, `prompt_sha256` `b4d7d65c78614f44924dcf3ef5241ab9237a2c0f4aa6a60250076d2a0846ea43`):

```text
Glaive Wasp: a single winged predator wasp of dark green chitin, two pairs of long translucent amber-veined wings spread wide, a long segmented tail ending in a hooked stinger, glowing amber eyes, seen from above at a three-quarter angle, head toward the lower right, the whole wasp in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the wasp keeps the same size and place in frame. It holds still, wings spread: it never turns, tilts or bobs; only its amber eyes and wing veins glow softly. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-072 Walk** (808 characters, `prompt_sha256` `a52e00ddfc7e2968c078b416e0cd60937dc597b126cb57977fa028827fdbcfa0`):

```text
Glaive Wasp: the same winged green wasp flying forward in place, its wings beating, fully visible in the middle of the frame, no other creatures. Walk animation, one static tripod shot: the camera never moves, zooms or pans, and the wasp glides forward in place, as if held in a wind tunnel, keeping the same size, place and three-quarter facing in frame: its translucent wings beat in a fast even blur from the first frame to the last. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-073 Attack** (729 characters, `prompt_sha256` `b7735059a5fb1b68a80e96ef878df1d10a597f5849c2b292d14be83f234f8006`):

```text
Glaive Wasp: the same winged green wasp flicks its tail and shoots a glowing amber barbed glaive forward. Attack animation, one static tripod shot: the camera never moves, zooms or pans, and the wasp keeps its place in frame. It flicks its tail and a glowing amber barbed glaive shoots from its stinger straight toward the lower right and out of the frame, then holds still in its starting pose. The plain dark backdrop stays unchanged throughout. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `sky_leviathan` (Sky Leviathan, Swarm, flyer)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, light shafts, tractor beam, beam of light from above, volumetric light, haze, light streaks, head turning, nodding, twitching, colour change, floor, cast shadow
```

**C-074 Idle** (975 characters, `prompt_sha256` `eb765cb17998b5c42954468dffaec7c5c090993e74734201bdd85cde4e4f18d8`):

```text
Sky Leviathan: a single heavy armoured flying beast shaped like a squid, a bulky body of dark green and brown chitin plates, a crown of short thick tentacles trailing behind it, glowing amber spore vents along its sides, seen from above at a three-quarter angle, head toward the lower right, the whole beast in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the beast keeps the same size and place in frame. It holds still: it never turns, tilts or bobs and its tentacles stay still; only its spore vents glow softly. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-075 Walk** (798 characters, `prompt_sha256` `62644458b93ce1340cf15082b6286e2ae2883124827583751821e1d4901db2ce`):

```text
Sky Leviathan: the same armoured squid beast gliding forward in place, fully visible in the middle of the frame, no other creatures. Walk animation, one static tripod shot: the camera never moves, zooms or pans, and the beast glides forward in place, as if held in a wind tunnel, keeping the same size, place and three-quarter facing in frame: its trailing tentacles ripple in slow even waves from the first frame to the last. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-076 Attack** (712 characters, `prompt_sha256` `41d6137bc63ea0b1b0d81379c88b8f49bb08f3ad623503a3216dfe277a66165e`):

```text
Sky Leviathan: the same armoured squid beast spews a stream of glowing amber spores forward from its front. Attack animation, one static tripod shot: the camera never moves, zooms or pans, and the beast keeps its place in frame. It spews a stream of glowing amber spores from its front straight toward the lower right and out of the frame, then holds still in its starting pose. The plain dark backdrop stays unchanged throughout. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `mycelium_shaman` (Mycelium Shaman, Swarm, creature)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, head turning, nodding, twitching, colour change, floor, cast shadow
```

**C-077 Idle** (1009 characters, `prompt_sha256` `78113c6e445477d2d1b9ed7f92e8b8238c3e2212d29e408da344a6bed426360a`):

```text
Mycelium Shaman: a single bulbous fungal creature, a round swollen body of mottled brown and green skin dotted with amber spore sacs, small dark eyes at its front, stubby legs, crouched low, seen from above at a three-quarter angle, front toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the creature keeps the same size, pose and place in frame. It crouches as still as a statue: its body, head, eyes and legs never move, turn or nod; only its spore sacs glow, and its colours never change. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-078 Walk** (900 characters, `prompt_sha256` `8a10afa412a08dd808855f707ce2927a2b8e4d1c484788323daee9cd45dcf3ab`):

```text
Mycelium Shaman: the same bulbous fungal creature creeping forward on its stubby legs, staying in place in the middle of the frame and fully visible. Walk animation, one static tripod shot: the camera never moves, zooms or pans. The creature creeps on a treadmill that holds it in place: its short stubby legs clearly lift and step in turn at an even pace from the first frame to the last, while its body stays at exactly the same place, size and three-quarter facing in frame; it never moves forward, back or toward the camera. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `tongue_chameleon` (Tongue Chameleon, Swarm, flyer)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, light shafts, tractor beam, beam of light from above, volumetric light, haze, light streaks, flapping wings, head turning, nodding, twitching, colour change, floor, cast shadow
```

**C-079 Idle** (966 characters, `prompt_sha256` `56a6d7f9e5a9b669b937931b970561ebba57c9ed75cd2eb72d088b1fda379cd3`):

```text
Tongue Chameleon: a single flying serpent creature of iridescent green scales, two narrow membrane wings spread wide, a long curled tail, a chameleon-like head with bulging amber eyes, its long tongue rolled in its mouth, seen from above at a three-quarter angle, head toward the lower right, the whole creature in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the creature keeps the same size and place in frame. It holds still, wings spread: it never turns, tilts or bobs; only its amber eyes glow softly. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-080 Walk** (820 characters, `prompt_sha256` `639bba6e1ed801a7a12f04e572ce283715ba0f4b0adf02741c371632258706a9`):

```text
Tongue Chameleon: the same winged green serpent gliding forward in place, its wings rippling, fully visible in the middle of the frame, no other creatures. Walk animation, one static tripod shot: the camera never moves, zooms or pans, and the creature glides forward in place, as if held in a wind tunnel, keeping the same size, place and three-quarter facing in frame: its membrane wings ripple in slow even waves from the first frame to the last. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `stag_behemoth` (Armored Stag Behemoth, Swarm, creature)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, head turning, nodding, twitching, colour change, floor, cast shadow
```

**C-081 Idle** (1017 characters, `prompt_sha256` `b7fd1354a63dca57b272539b773aaaa6a12caab780d54a58ba97b90063218484`):

```text
Armored Stag Behemoth: a single enormous four-legged beast of thick dark brown chitin plates glowing amber between them, a low head with two huge curved scythe horns, standing still, seen from above at a three-quarter angle, head toward the lower right, whole and centred, plain dark backdrop, lit softly from the front, no floor, no shadow. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the beast keeps the same size, pose and place in frame. It stands as still as a statue: its body, head, horns and legs never move, turn or nod; only the amber light between its plates glows, and its colours never change. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-082 Walk** (908 characters, `prompt_sha256` `73909f423fe5b311190d22d538722f447288f44faf68243a5b4f9ac6aa18bfcc`):

```text
Armored Stag Behemoth: the same huge horned beast charging heavily on its four thick legs, staying in place in the middle of the frame and fully visible. Walk animation, one static tripod shot: the camera never moves, zooms or pans. The beast charges heavily on a treadmill that holds it in place: its four thick legs clearly lift and step in turn at an even pace from the first frame to the last, while its body stays at exactly the same place, size and three-quarter facing in frame; it never moves forward, back or toward the camera. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-083 Attack** (694 characters, `prompt_sha256` `a531740d4edc0420b3b5c77834aaf533617ce6e231bdeb4066024c2b38759335`):

```text
Armored Stag Behemoth: the same huge horned beast swings its great scythe horns in a sweeping cleave. Attack animation, one static tripod shot: the camera never moves, zooms or pans, and the beast keeps its place in frame. It swings its great scythe horns in one sweeping sideways cleave toward the lower right, then settles smoothly back to its starting pose. The plain dark backdrop stays unchanged throughout. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `cybernetics_conduit_dome` (Cybernetics Conduit Dome, Enclave, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-084 Idle** (1022 characters, `prompt_sha256` `a90b4151673ea064be3b4920f248281cfedd1a01e35ef1d6090c188783aa5c94`):

```text
Cybernetics Conduit Dome: a single golden alien research core, a rounded hexagonal golden block, a large cyan crystal sphere in its front, thick conduits arching over it, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The crystal sphere glows steadily and small lights flicker along the conduits. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-085 Constructing** (927 characters, `prompt_sha256` `0bdae6308df201ce6139deb989cdc67f2d4e0fa4685ef494bb82cd5102f18c59`):

```text
Cybernetics Conduit Dome: a single golden alien research core, a rounded hexagonal golden block, a large cyan crystal sphere in its front, thick conduits arching over it, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `crystalline_forge_dome` (Crystalline Forge Dome, Enclave, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-086 Idle** (1004 characters, `prompt_sha256` `313327f51d197f85e1cecc6abcb507b76408ced842b7459a88c9b1fffa927c69`):

```text
Crystalline Forge Dome: a single low wide golden alien forge dome of curved gold plates, three short cyan crystal spikes on top, orange heat vents at its base, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The crystals glow steadily and thin vapour rises gently from the vents. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-087 Constructing** (916 characters, `prompt_sha256` `41f1336f6b945a04d3a8365fe01a316cf9a3516b05a0400d9c9dbb6fb5632672`):

```text
Crystalline Forge Dome: a single low wide golden alien forge dome of curved gold plates, three short cyan crystal spikes on top, orange heat vents at its base, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `tesla_sentinel_dome` (Tesla Sentinel Dome, Enclave, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-088 Idle** (995 characters, `prompt_sha256` `227f4b616c60fcf00770bea6c7e610f64909ef68732a33ea6206f617ef5d9e6b`):

```text
Tesla Sentinel Dome: a single small alien defence turret, a violet energy orb held in a cradle of four curved golden claws on a squat round golden base, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The orb glows steadily and tiny violet sparks crackle on its surface. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-089 Constructing** (909 characters, `prompt_sha256` `6e59eb4a5f10e912e4a6712c3ca2fec8031ece117a2ee927582755af41128dda`):

```text
Tesla Sentinel Dome: a single small alien defence turret, a violet energy orb held in a cradle of four curved golden claws on a squat round golden base, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `harmonic_recharge_dome` (Harmonic Recharge Dome, Enclave, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-090 Idle** (994 characters, `prompt_sha256` `2cd7a7e4b96a16c9c07d8d7ee925b7baa0bbe07a1e71eb595b609d8cb7f74718`):

```text
Harmonic Recharge Dome: a single small golden alien battery, a rounded golden drum ringed with cyan crystal panels, a shallow bowl of pale blue light on top, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The panels and the bowl glow steadily and small lights flicker. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-091 Constructing** (914 characters, `prompt_sha256` `ce785dda720558ab22bcf283bb4b6a5909525746621ce96e5a77be3e0cb4016f`):

```text
Harmonic Recharge Dome: a single small golden alien battery, a rounded golden drum ringed with cyan crystal panels, a shallow bowl of pale blue light on top, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `templar_conclave_dome` (Templar Conclave Dome, Enclave, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-092 Idle** (1005 characters, `prompt_sha256` `a48bf68ae58b88c905cd2df5f9bcc5ea7900f0eb441f8f1a551adc7a805efdff`):

```text
Templar Conclave Dome: a single elegant golden alien council hall, round and tiered, violet windows, three short golden spires around a violet crystal on top, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The crystal glows steadily and soft violet lights flicker in the windows. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-093 Constructing** (915 characters, `prompt_sha256` `b565d6796dfb21e2d12312d343c0ca1c89b1425f6bb3291a19e5215eb3a8fc06`):

```text
Templar Conclave Dome: a single elegant golden alien council hall, round and tiered, violet windows, three short golden spires around a violet crystal on top, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `robotic_foundry_dome` (Robotic Foundry Dome, Enclave, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-094 Idle** (1009 characters, `prompt_sha256` `cbbec08ec475086bea91fec4919467748dfaff17c4a14c515169283a5d4deae8`):

```text
Robotic Foundry Dome: a single heavy golden alien robotics factory, a broad squat block of gold and silver plates, a large round sealed hatch in front, cyan crystals, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The crystals glow steadily and small lights flicker around the hatch. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-095 Constructing** (923 characters, `prompt_sha256` `2b3efcd5054767a48eae23a60ad35367a5d3ebc7987f97dea3c5d043cbc37970`):

```text
Robotic Foundry Dome: a single heavy golden alien robotics factory, a broad squat block of gold and silver plates, a large round sealed hatch in front, cyan crystals, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `stellar_portal_dome` (Stellar Portal Dome, Enclave, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-096 Idle** (992 characters, `prompt_sha256` `57adf9fa86889731a3208d6575c13317c2cc4a0fd456411060a0c613796bc3c3`):

```text
Stellar Portal Dome: a single large golden alien stargate, a wide flat golden ring standing upright on a low curved base, a veil of deep blue energy filling the ring, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The energy veil glows steadily and shimmers faintly. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-097 Constructing** (923 characters, `prompt_sha256` `af325a5ced46f2c8528e1ade6a8a5a34a381e6c40606cfdf1802682e9b1c27ff`):

```text
Stellar Portal Dome: a single large golden alien stargate, a wide flat golden ring standing upright on a low curved base, a veil of deep blue energy filling the ring, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `psionic_archives_dome` (Psionic Archives Dome, Enclave, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-098 Idle** (1009 characters, `prompt_sha256` `f712c2703e438748d86c11c897974a7f357b450e765ad1746ea14a9830d775cf`):

```text
Psionic Archives Dome: a single golden alien archive temple, a stepped round tower with violet glass panels, a violet crystal on top, cyan crystals at its base, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The crystal glows steadily and soft violet light flickers behind the glass. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-099 Constructing** (917 characters, `prompt_sha256` `bfd07207f0c8ca3810bcf9dc72e212efe76709def249f91d95b141d89ae2c30b`):

```text
Psionic Archives Dome: a single golden alien archive temple, a stepped round tower with violet glass panels, a violet crystal on top, cyan crystals at its base, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `void_sanctuary_dome` (Void Sanctuary Dome, Enclave, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-100 Idle** (1001 characters, `prompt_sha256` `ea9570ca37af7aba519c77e0e8657e7a5fd7abe316b334a3b5b6d114eff7abff`):

```text
Void Sanctuary Dome: a single dark alien shrine, a low ring of tall pointed black-and-silver stone blades around a dark violet crystal orb fixed at its centre, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The orb glows steadily and faint violet mist curls gently around it. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-101 Constructing** (916 characters, `prompt_sha256` `d3eefcc9846302b48a046a0b2022265285efe681eb6f93c8aa540f3756de8cf6`):

```text
Void Sanctuary Dome: a single dark alien shrine, a low ring of tall pointed black-and-silver stone blades around a dark violet crystal orb fixed at its centre, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `robotic_assembly_bay_dome` (Robotic Assembly Bay Dome, Enclave, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-102 Idle** (995 characters, `prompt_sha256` `56bfa97f171335ba451c127785a134a6a64bef30026d22b37486b2eed96490fa`):

```text
Robotic Assembly Bay Dome: a single low wide golden alien engineering bay with two fixed curved crane arms folded over its roof and a cyan crystal panel on its front, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The panel glows steadily and small cyan lights flicker. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-103 Constructing** (923 characters, `prompt_sha256` `8e0d903d1854100db61c996ed79244d5ee7623749bf804723e1b5a7d8ffe41aa`):

```text
Robotic Assembly Bay Dome: a single low wide golden alien engineering bay with two fixed curved crane arms folded over its roof and a cyan crystal panel on its front, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `celestial_beacon_dome` (Celestial Beacon Dome, Enclave, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-104 Idle** (1010 characters, `prompt_sha256` `05b2afb1ff31056f49c24b2baec69f3121ec207183130cc6df98501b0ebbbf05`):

```text
Celestial Beacon Dome: a single golden alien beacon, a broad round base with three curved golden arms rising to hold a large white-blue star crystal between their tips, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The crystal glows steadily and tiny sparks of light drift around it. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-105 Constructing** (925 characters, `prompt_sha256` `08e5c93e4baa6e265f9026043ed5146e90107f4a7782cf1ad351a933fbb0654b`):

```text
Celestial Beacon Dome: a single golden alien beacon, a broad round base with three curved golden arms rising to hold a large white-blue star crystal between their tips, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `canopy_grove_dome` (Canopy Grove Dome, Swarm, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-106 Idle** (1009 characters, `prompt_sha256` `731e8e6da4bb55513257f271829c78cd2bf982925f099a9c0a0cec743759007d`):

```text
Canopy Grove Dome: a single large living hive mound of ridged bark and brown chitin under a low spreading canopy of dark green leathery leaf plates hung with glowing amber pods, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The pods glow steadily and thin amber vapour rises gently. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-107 Constructing** (934 characters, `prompt_sha256` `d7843a49fd552b2c14fe3b860026864e8327561c5ef9e946f8b352140cf7341d`):

```text
Canopy Grove Dome: a single large living hive mound of ridged bark and brown chitin under a low spreading canopy of dark green leathery leaf plates hung with glowing amber pods, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `primal_world_tree_dome` (Primal World Tree Dome, Swarm, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-108 Idle** (993 characters, `prompt_sha256` `49993e38d1593f91dde1d3d657bb4ca271d27cfcd0ebc3968e93d11b74dca594`):

```text
Primal World Tree Dome: a single huge gnarled living tree hive, a thick twisted trunk of bark and chitin under a broad dome of dark green leaves with amber pods, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The pods glow steadily and thin amber vapour rises gently. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-109 Constructing** (918 characters, `prompt_sha256` `baa02dbce8d0d9e8e27ac472ae5ecd8097fdf007c81b37a1ce7cc8ba15529448`):

```text
Primal World Tree Dome: a single huge gnarled living tree hive, a thick twisted trunk of bark and chitin under a broad dome of dark green leaves with amber pods, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `metamorphic_chamber_dome` (Metamorphic Chamber Dome, Swarm, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-110 Idle** (999 characters, `prompt_sha256` `9a833e7e07fe91ca89df458628101f644de69912b41ac76ad420b6fb0f884213`):

```text
Metamorphic Chamber Dome: a single living gestation organ, a round swollen pod of dark green hide in brown chitin ribs, amber veins over it, a slit on its top, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The veins and the slit glow steadily and thin vapour rises gently. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-111 Constructing** (916 characters, `prompt_sha256` `1fa6e99a2e26c93c7ce1c301bd067f833962a712e5b75cc476a8fb7c18c57123`):

```text
Metamorphic Chamber Dome: a single living gestation organ, a round swollen pod of dark green hide in brown chitin ribs, amber veins over it, a slit on its top, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `thorn_lasher_dome` (Thorn Lasher Dome, Swarm, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-112 Idle** (999 characters, `prompt_sha256` `3cee34c4ecf007eabc8021ffd16e6af23b992c4b84aade2ae485b05436310ec9`):

```text
Thorn Lasher Dome: a single living defence growth, a low mound of dark brown chitin with a thick spiked tentacle coiled on top, a long bone spike tip, amber eyes, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The eyes glow steadily and the tentacle stays coiled and still. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-113 Constructing** (919 characters, `prompt_sha256` `ff15965c3b6ee1be79dc8b88d76bdef1c7b7048ff882277d57bf73e3de0ce92e`):

```text
Thorn Lasher Dome: a single living defence growth, a low mound of dark brown chitin with a thick spiked tentacle coiled on top, a long bone spike tip, amber eyes, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `spore_spire_dome` (Spore Spire Dome, Swarm, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-114 Idle** (999 characters, `prompt_sha256` `9e9abaca7ccef886fbbb7739606abaca0cfd98c8114fb78834918cfc1d99121d`):

```text
Spore Spire Dome: a single living anti-air growth, a short thick stalk of dark green chitin on a low round base, topped by a swollen bulb ringed with amber pores, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The pores glow steadily and faint amber spores drift up gently. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-115 Constructing** (919 characters, `prompt_sha256` `bc83e43e852ff7e0d167a44a6ff8ed201fdae15069b4aa6c41a5382df3532280`):

```text
Spore Spire Dome: a single living anti-air growth, a short thick stalk of dark green chitin on a low round base, topped by a swollen bulb ringed with amber pores, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `carapace_mound_dome` (Carapace Mound Dome, Swarm, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-116 Idle** (1011 characters, `prompt_sha256` `a9bfc1b8130b79faded5d1c6f987cfaee055ec7a7521a7f763e5ac290d613fb8`):

```text
Carapace Mound Dome: a single low armoured burrow mound of overlapping thick brown carapace plates like a beetle shell, a dark round tunnel mouth at its front lit amber inside, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The tunnel mouth glows steadily and thin vapour rises gently. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-117 Constructing** (933 characters, `prompt_sha256` `a964606e4fe34e0a03b5fba0b781187accac5fb7a1321cd1dbdc9c9cded52cae`):

```text
Carapace Mound Dome: a single low armoured burrow mound of overlapping thick brown carapace plates like a beetle shell, a dark round tunnel mouth at its front lit amber inside, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `caustic_nursery_dome` (Caustic Nursery Dome, Swarm, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-118 Idle** (979 characters, `prompt_sha256` `98c0fe0af5e9e128897bc35307e9e85de97e0afd2d8433ddcc6ba976ceee6dc8`):

```text
Caustic Nursery Dome: a single living nursery, a cluster of round translucent bright green acid sacs nested in curved dark brown chitin ribs, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The acid sacs glow steadily and small bubbles drift inside them. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-119 Constructing** (898 characters, `prompt_sha256` `9ed2de2584caaeada1015cecbf9feb72c357b0e95ddb237aaa3d0afcf66ce678`):

```text
Caustic Nursery Dome: a single living nursery, a cluster of round translucent bright green acid sacs nested in curved dark brown chitin ribs, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `spine_den_dome` (Spine Den Dome, Swarm, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-120 Idle** (1008 characters, `prompt_sha256` `1142273a795feb90748a106c9572bc694fae23ba09b64aefc1a750146cf134b6`):

```text
Spine Den Dome: a single living den, a hunched dome of dark green scaled hide bristling with long bone spines along its back, a dark arched opening at its front lit amber inside, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The opening glows steadily and thin vapour rises gently. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-121 Constructing** (935 characters, `prompt_sha256` `a4daa77abbdfa074594323ed7550ba0953f7a4af4d5068e1cbe33ea0e7c81fff`):

```text
Spine Den Dome: a single living den, a hunched dome of dark green scaled hide bristling with long bone spines along its back, a dark arched opening at its front lit amber inside, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `burrow_spire_dome` (Burrow Spire Dome, Swarm, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-122 Idle** (1003 characters, `prompt_sha256` `827608ecc7d536226de966a40d33c4e56d891d671eff4413569c7c0fa6351f2c`):

```text
Burrow Spire Dome: a single living den, a low spiral shell of dark brown chitin coiled like a snail shell, jagged bone spikes along its ridge and amber light in its opening, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The opening glows steadily and thin vapour rises gently. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-123 Constructing** (930 characters, `prompt_sha256` `76471bd4927af7c8995798f0d6ee01c13a519bc7622d1e4413aa8665e63b21bc`):

```text
Burrow Spire Dome: a single living den, a low spiral shell of dark brown chitin coiled like a snail shell, jagged bone spikes along its ridge and amber light in its opening, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `chitin_spire_dome` (Chitin Spire Dome, Swarm, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-124 Idle** (997 characters, `prompt_sha256` `899d8cca77e36bc645c08d2d024a9b4592c2a07317654fd87ebb23b79481f5aa`):

```text
Chitin Spire Dome: a single slender living spire of twisted dark green and brown chitin, ridged like a horn and curving up to a point, glowing amber pores along it, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The pores glow steadily and thin amber vapour rises gently. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-125 Constructing** (921 characters, `prompt_sha256` `ec900c56f4520db19b4388409ff95c2936b875f91da81feb035cf7646b8e8554`):

```text
Chitin Spire Dome: a single slender living spire of twisted dark green and brown chitin, ridged like a horn and curving up to a point, glowing amber pores along it, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `apex_spire_dome` (Apex Spire Dome, Swarm, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-126 Idle** (981 characters, `prompt_sha256` `c27d14243250fb819b6ea1011a51996b72c2890b294ef831a1697b4dcdb5b213`):

```text
Apex Spire Dome: a single tall twisted living spire of dark chitin with three curving horn branches at its top and large glowing amber pods along it, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The pods glow steadily and thin amber vapour rises gently. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-127 Constructing** (906 characters, `prompt_sha256` `24d1a0d64a3f8ce664fac6addb3b14a30bda1f6a0fdb386e82852834b2349175`):

```text
Apex Spire Dome: a single tall twisted living spire of dark chitin with three curving horn branches at its top and large glowing amber pods along it, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `infestation_nursery_dome` (Infestation Nursery Dome, Swarm, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-128 Idle** (993 characters, `prompt_sha256` `bfed3620141aa35fa243177c309cd34bc420223201b8aac1644eeb9b5a906327`):

```text
Infestation Nursery Dome: a single living nursery, a wide round fleshy mound, thick chitin lips around a green opening on top, small spore sacs on its sides, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The opening glows steadily and thin green vapour rises gently. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-129 Constructing** (914 characters, `prompt_sha256` `eb069231c36c3ab8a6d8a7a835920b668aed13e424e16e438282a1765e1d78db`):

```text
Infestation Nursery Dome: a single living nursery, a wide round fleshy mound, thick chitin lips around a green opening on top, small spore sacs on its sides, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `subterranean_network_dome` (Subterranean Network Dome, Swarm, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-130 Idle** (1003 characters, `prompt_sha256` `6b72fa4678c1d8e98ea11748bb8cb2b19b780fec346e3c8eec234c1e480e6970`):

```text
Subterranean Network Dome: a single living tunnel hub, a large hunched mound of dark brown chitin with a huge round toothed maw at its front, held open and glowing amber inside, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The maw glows steadily and thin vapour rises gently. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-131 Constructing** (934 characters, `prompt_sha256` `50e7b7851e2209cbbc7e4114f4fc3b31b3fd4c2e3f6cc651b16d6a083659dd9f`):

```text
Subterranean Network Dome: a single living tunnel hub, a large hunched mound of dark brown chitin with a huge round toothed maw at its front, held open and glowing amber inside, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `behemoth_den_dome` (Behemoth Den Dome, Swarm, structure)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, breathing motion, pulsing size, swelling, shrinking, moving parts, rotating parts, antenna changing shape, floor, ground plane, cast shadow, horizon
```

**C-132 Idle** (997 characters, `prompt_sha256` `8de80828632ec10c0c20841fd4112fd84021f71c6402316a927e5b076b0e5a39`):

```text
Behemoth Den Dome: a single large living cavern mound of massive brown chitin plates, two huge curved tusks framing a dark cave mouth at its front lit amber inside, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. The cave mouth glows steadily and thin vapour rises gently. Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building, fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**C-133 Constructing** (921 characters, `prompt_sha256` `2ad87201a7f9d544a1860693818356b18702b60c9f5637d6337bfe9b2e1be4d3`):

```text
Behemoth Den Dome: a single large living cavern mound of massive brown chitin plates, two huge curved tusks framing a dark cave mouth at its front lit amber inside, one stationary building centred in frame and fully visible with empty space around it, no characters, no vehicles. Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```


## Appendix C: image prompts of the polish pins

#### PP-1 `mothership_carrier`

```text
Mothership Carrier: a single large golden carrier aircraft, a wide flat round disc with a low dome, a ring of glowing amber docking bays around its rim and a small cyan energy core glowing in the dome, held still, seen from above at a three-quarter angle looking down on its top, front toward the lower right, the whole carrier in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### PP-2 `saucer_interceptor`

```text
Saucer Interceptor: a single small golden drone aircraft, a smooth flat round disc with a small domed top, a glowing amber core in the dome and a ring of small fixed amber lights around its rim, held still, seen from above at a three-quarter angle looking down on its top, front toward the lower right, the whole drone in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

#### PP-3 `spore_matriarch`

```text
Spore Matriarch: a single manta-ray-shaped swarm beast, broad dark green chitin wings edged with glowing amber spore pods, a thin tail, two pale larvae on its belly, seen from above at a three-quarter angle, head toward the lower right, wings spread wide and motionless, the whole beast in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. One still picture in 16:9 landscape format: the whole subject centred in the frame with a clear margin on every side, nothing else in the picture, no floor, no cast shadow, no light shafts. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```


## Appendix D: polish clip prompts and negative prompts

### `mothership_carrier` (Mothership Carrier, Enclave, polish)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, light shafts, tractor beam, beam of light from above, volumetric light, haze, light streaks
```

**PC-01 Idle** (1016 characters, `prompt_sha256` `4ba8db05ff56cbd6377b1502f036d6b66d455bc57262be7312551b37e69b2d25`):

```text
Mothership Carrier: a single large golden carrier aircraft, a wide flat round disc with a low dome, a ring of glowing amber docking bays around its rim and a small cyan energy core glowing in the dome, held still, seen from above at a three-quarter angle looking down on its top, front toward the lower right, the whole carrier in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the carrier keeps the same size and place in frame. The carrier hangs still in the air: it never turns, tilts or bobs, and only its rim bays and central core glow and blink softly. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**PC-02 Walk** (904 characters, `prompt_sha256` `df006cefaca96a4b2945c8817e7de875979312e0e82bbee60a6386ac5744beff`):

```text
Mothership Carrier: the same large golden saucer carrier in slow, heavy level forward flight, its rim bays glowing, flying in place in the middle of the frame and fully visible, no other aircraft. Walk animation, one static tripod shot: the camera never moves, zooms or pans, and the carrier flies forward in place, as if held in a wind tunnel, keeping the same size, pose and place in frame. From the first frame to the last its rim bays glow bright and long thin vapour trails stream back from its rim toward the upper left, flickering at an even pace; no ground contact. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**PC-03 Attack** (802 characters, `prompt_sha256` `b5ab939a28aefd475c75e0d7800c84d52db11bea8eb2dc9f32157cb768094662`):

```text
Mothership Carrier: the same large golden saucer carrier launches tiny golden saucer drones from the glowing docking bays on its front rim. Attack animation, one static tripod shot: the camera never moves, zooms or pans, and the carrier keeps its place in frame, hovering. The docking bays on its front rim flash open and a quick stream of tiny golden saucer drones flies out of them, one after another, straight toward the lower right and out of the frame, then the bays dim and the carrier hovers in its starting pose. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `saucer_interceptor` (Saucer Interceptor, Enclave, polish)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, light shafts, tractor beam, beam of light from above, volumetric light, haze, light streaks
```

**PC-04 Idle** (1009 characters, `prompt_sha256` `56a2bea1740da1ba57a7a557b0f6092b0cfd8a4aac542c9d98613411af218f11`):

```text
Saucer Interceptor: a single small golden drone aircraft, a smooth flat round disc with a small domed top, a glowing amber core in the dome and a ring of small fixed amber lights around its rim, held still, seen from above at a three-quarter angle looking down on its top, front toward the lower right, the whole drone in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the drone keeps the same size and place in frame. The drone hangs still in the air: it never turns, tilts or bobs, and only its amber core and small rim lights glow and blink softly. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**PC-05 Walk** (910 characters, `prompt_sha256` `7aa51c260cc85efe42a0fe00bff11f84763583eef929b56def5e7f6bfcb286d4`):

```text
Saucer Interceptor: the same small golden saucer drone in fast level forward flight, its rim lights glowing bright, flying in place in the middle of the frame and fully visible, no other aircraft. Walk animation, one static tripod shot: the camera never moves, zooms or pans, and the drone flies at full speed in place, as if held in a wind tunnel, keeping the same size, pose and place in frame. From the first frame to the last its rim lights blaze and long thin bright light trails stream back from its rim toward the upper left, flickering at an even pace; no ground contact. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**PC-06 Attack** (733 characters, `prompt_sha256` `ffef78ce91823897eadcf2af655d163a52263a0ee69720641f8fa010772e1451`):

```text
Saucer Interceptor: the same small golden saucer drone fires two short amber energy beams forward from the front of its rim. Attack animation, one static tripod shot: the camera never moves, zooms or pans, and the drone keeps its place in frame, hovering. It fires forward: two short bright amber energy beams flash from the front of its rim straight toward the lower right and out of the frame, then the beams fade and it hovers in its starting pose. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `spore_matriarch` (Spore Matriarch, Swarm, polish)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, light shafts, tractor beam, beam of light from above, volumetric light, haze, flapping wings
```

**PC-07 Idle** (945 characters, `prompt_sha256` `336ef61e3c683a591ae53bc810685c2d7010d51b0b4f38d538baaed303f5484a`):

```text
Spore Matriarch: a single manta-ray-shaped swarm beast, broad dark green chitin wings edged with glowing amber spore pods, a thin tail, two pale larvae on its belly, seen from above at a three-quarter angle, head toward the lower right, wings spread wide and motionless, the whole beast in the middle of the frame with wide margins, plain dark backdrop, lit softly from the front. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the beast keeps the same size and place in frame. It holds still, wings spread: it never turns, tilts or bobs; only its amber spore pods pulse softly. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**PC-08 Walk** (835 characters, `prompt_sha256` `c902d9c4fbed7424a5b99aad7841fbf0733beacc4963b208898a9306ddf20837`):

```text
Spore Matriarch: the same huge winged beast gliding forward in place, its broad wings rippling in slow even waves, fully visible in the middle of the frame, no other creatures. Walk animation, one static tripod shot: the camera never moves, zooms or pans, and the beast glides forward in place, as if held in a wind tunnel, keeping the same size, place and three-quarter facing in frame: its broad wings ripple in slow even waves from the first frame to the last. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**PC-09 Attack** (737 characters, `prompt_sha256` `9fde072c8b68fa09cada7b2a2620cd1ec738db59f181f7b35faa950f9e28a35c`):

```text
Spore Matriarch: the same huge winged beast flings two small pale parasite larvae from under its belly toward the lower right. Attack animation, one static tripod shot: the camera never moves, zooms or pans, and the beast keeps its place in frame. It flings two small wriggling pale larvae from under its belly straight toward the lower right and out of the frame, then holds still in its starting pose. The plain dark backdrop stays unchanged throughout. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

### `brood_mite` (Brood Mite, Swarm, polish)

Negative prompt, the same for every clip of this identity:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, head turning, nodding, twitching, colour change
```

**PC-10 Idle** (1019 characters, `prompt_sha256` `4406264c04b9f4035b302fc3a5b27903f35470ca2204a96076222e7e25e7c06c`):

```text
Brood Mite: a single small parasitic larva creature, a pale segmented grub-like body with dark chitin plates, short hooked legs and two curved needle claws at its front, crouched still and twitching, seen from above at a three-quarter angle, its front toward the lower right, whole in the middle of the frame, on a plain dark backdrop, lit softly from the front, no other creatures. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the larva keeps the same size, pose and place in frame. It crouches as still as a statue: its body, head, legs and claws never move, turn or nod; its colours and light never change. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**PC-11 Walk** (937 characters, `prompt_sha256` `182ca4003ec8a5d7dc906a3a9ebcd41484029d20b20c3e5d4ec4df0f1f3846fa`):

```text
Brood Mite: the same small parasitic larva scuttling quickly forward on its hooked legs, staying in place in the middle of the frame and fully visible, no other creatures. Walk animation, one static tripod shot: the camera never moves, zooms or pans. The larva scuttles on a treadmill that holds it in place: its short hooked legs clearly lift and step in quick turns at an even pace from the first frame to the last, while its body and claws stay at exactly the same place, size and three-quarter facing in frame; it never moves forward, back or toward the camera. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

