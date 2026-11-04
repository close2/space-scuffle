# Blueprint: Structure Action Text per Drawing Kind, and a Bounded Regeneration Series for the Siphon Idle

An amendment to the seamless-loops plan [`2026-11-04-veo-seamless-loops.md`](2026-11-04-veo-seamless-loops.md) (its
WP5) and, through it, to the Veo media plan [`2026-11-04-phase-8-5-veo-media.md`](2026-11-04-phase-8-5-veo-media.md)
(its WP4, batch 1). Structures get action text of their own for each state, the way `framing` already treats
`unit_sphere` and `building_dome` differently. `plasma_siphon_dome` Idle is then regenerated in a bounded series with
a written reason for every rejection.

**Status of this document:** Stage 1 blueprint under charter §6, written on 2026-11-04 against `main` at `b35d793`.
No Veo request was made while planning. Line numbers drift, so search for the quoted symbol. A claim that could not
be confirmed while planning is marked **verify**, and the first step of its package checks it.

**Decision being carried out.** The orchestrator decided this under the owner's Q0020 rule, which makes visual
questions the agent's to decide (the owner may overrule):

- Structures, drawn as domes, get their own action text for each state.
- Idle shows a completely still building: fixed geometry, small ambient effects only, no moving parts, no breathing
  or pulsing of the shape, and a locked camera.
- Constructing shows the build-up on the finished shape, with no camera motion.
- Further pinned generations of `plasma_siphon_dome` Idle are approved until one passes `loop-check` and the visual
  review.

Owner rules that bound this plan: Q0024 and Q0026 (a 200 EUR budget, regenerate whenever a clip misses the criteria,
prefer quality, keep sound, no loop jumps), Q0047 (60 generations per rolling 7 days, 12 per run), and the
plain-backdrop rule (2026-11-04). No owner question is needed, because every open point is a visual one (Q0020).

**Spending.** WP1 spends nothing. WP2 spends at most 4 generations (**$1.60** at list price, $0.40 for each 4 s Veo
3.1 Fast clip with sound). The running total before WP2 is 7 generations, **$2.80**, and the rolling window holds 7
of 60.

**Out of scope:** changes to unit action texts (unit prompts stay byte for byte as they are), the seam repair, and
`loop-check` thresholds. Those belong to the seamless-loops plan and stay as it set them.

---

## 1. Summary

| WP | Title | Spends | Size | Depends on |
|---|---|---|---|---|
| WP1 | Action text per drawing kind in data, with overrides; a negative prompt per kind; `--seed`; validation and dry run | No | S-M (half a day to 1 day) | none |
| WP2 | Regeneration series for `plasma_siphon_dome` Idle: at most 4 attempts, a written reason for each rejection | ≤ 4 | S | WP1 merged |
| WP3 | Resume batch 1 (seamless-loops WP5, Veo plan WP4) with the structure texts and the series rule | as that WP | M | WP2 accepted a clip, or its rethink is decided |

---

## 2. Facts

### 2.1 The pilot's failures and their likely cause

From the evidence [`2026-11-04-batch-1.md`](../evidence/veo/2026-11-04-batch-1.md), section "Pilot of `lastFrame`".
Pinning the start frame as `lastFrame` fixed the seam (raw wrap 0.994). Both clips still fail inside the loop:

- `8c1e48be`: the roof antenna changes between a lattice mast and a thin rod, and the lower plasma pipes go dark and
  light up again (`worst` 0.899, below the 0.919 threshold).
- `5fde1879`: the camera pushes in by about 2 % and pulls back out in the middle of the clip (`worst` 0.910, below
  0.917; growth 2.21 %, above 1.5 %).

The composed Idle prompt has three parts:

1. The identity text in `assets/catalog/structures.ron`, which ends "Plasma pulses slowly through the pipes".
2. The shared Idle action text in `assets/catalog/media_style.ron`: "steady rhythmic breathing movement, subtle weapon
   glow pulse, head and radar scanning smoothly in place".
3. The style sentence.

That action text was written for units. "Breathing" plausibly invites the camera breathing seen in `5fde1879`.
"Radar scanning" plausibly invites the antenna changes in `8c1e48be`. "Pulse" appears in both parts 1 and 2, and
plausibly invites the pipes going dark.

### 2.2 What the code does today

- `prompts.rs`: `ActionDescriptions` is one struct with six required `String` fields, one per state.
  `build_action_prompt` composes `"{identity} {action} {style}"`, or `"{display_name}: {lore} {action} {style}"` when
  the identity has no authored text. Nothing depends on the drawing kind.
- `framing.rs::FramingPolicy` is the pattern to mirror. It has `unit_sphere`, `building_dome` and `overrides`
  (slug to value). `framing_for(slug, &ShapeKind)` chooses an override first, then the kind. `check` refuses an
  override slug that the catalog does not hold (`UnknownFramingOverride`).
- `sim_core::UnitCatalog::publishable_actions` publishes:
  - Idle always.
  - Constructing when something can raise the identity.
  - Walk and Attack only for mobile identities.
  - Cast for harvesters and constructors.

  So the shipped structures publish Idle and Constructing only.
- `media/veo_request.ron` holds one `negative_prompt` for every request. `VeoClient` sends the same `parameters` on
  each start (`client.parameters()`). The ledger's `request_started` already records `parameters` for each request.
- Prompt length: `max_prompt_chars: 1024`. The siphon's Idle identity text is 362 characters and the style sentence
  281, so a structure Idle action text may be at most 379 characters for this identity. The text in D-2 is 375, and
  with the identity edit of D-2 the composed prompt is 1,016 characters.

### 2.3 `seed` on Vertex Veo (checked 2026-11-04, no request made)

The Vertex AI page [Generate videos from an image](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/video/generate-videos-from-an-image)
lists `seed` in `parameters`, next to `negativePrompt` and `sampleCount`: "Optional. An uint32 value that the model
uses to generate deterministic videos. Specifying a seed number with your request without changing other parameters
guides the model to produce the same videos. The accepted range of values is `0`-`4294967295`." The same page names
no model restriction for it (it restricts only `resolution`), and the
[text-to-video page](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/video/generate-videos-from-text)
documents the same field.

Consequences:

- A seed makes an attempt reproducible as far as the service allows ("guides", not a guarantee).
- Keeping the seed fixed while the wording changes makes the wording the only deliberate variable.
- **Verify** on WP2's first request that Vertex Express accepts `seed` together with `lastFrame`. A refusal arrives
  as a start error, which is not billed. It is recorded as `start_failed`, which counts against the window
  conservatively. The series then continues without a seed and with wording changes only.
- The Gemini API mode is not checked here. A run there that asks for a seed is refused before any request, with a
  typed error `SeedNeedsVertex`, as `LastFrameNeedsVertex` does today.

---

## 3. Design decisions

### D-1. Action text per drawing kind, with overrides

The `actions` section of `media_style.ron` changes shape to mirror `framing`:

```ron
actions: (
    // Text for identities drawn as a UnitSphere (mobile units). Today's six texts, unchanged.
    unit_sphere: (
        texts: { Constructing: "...", Idle: "...", Walk: "...", Attack: "...", Cast: "...", Die: "..." },
        negative_terms: None,
    ),
    // Text for identities drawn as a BuildingDome (structures).
    building_dome: (
        texts: { Constructing: "...", Idle: "..." },
        negative_terms: Some("..."),
    ),
    // Exact catalog slug -> state -> text, for one identity whose clip needs other wording.
    overrides: {},
),
```

New types in `prompts.rs`:

- `KindActions { texts: BTreeMap<AnimState, String>, negative_terms: Option<String> }`.
- `ActionPolicy { unit_sphere: KindActions, building_dome: KindActions, overrides: BTreeMap<String,
  BTreeMap<AnimState, String>> }`.

Both are `deny_unknown_fields`. `ActionPolicy` replaces `ActionDescriptions` in `MediaStyle`, and
`ActionDescriptions` is removed.

`ActionPolicy::text_for(slug, &ShapeKind, state) -> Option<ActionText>` returns:

- the text;
- its source, `Kind(UnitSphere | BuildingDome)` or `Override`, which the dry run prints;
- the kind's `negative_terms`, which an override does not change.

`build_action_prompt` uses this. Composition order and spacing do not change.

`ActionPolicy::check`, called by `MediaStyle::load_for_catalog` next to the framing and loop checks:

- Every state that `publishable_actions` publishes for an identity has a text for that identity's kind, or an
  override. Otherwise it fails with `MissingActionText { path, kind, state, slug }`, naming the first identity that
  needs the text.
- An override slug that the catalog does not hold exactly fails with `UnknownActionOverride`, as
  `UnknownFramingOverride` does.
- An override for a state that its identity does not publish fails with `DataFile`.
- A text for a state that no identity of its kind publishes is allowed. It is still checked by `check_prompt`, so it
  cannot hold a placeholder.

The old flat shape (`actions: (idle: "...", ...)`) fails to load, with the RON error naming the unknown field. No
identity appears in Rust (charter §3.3), and changing wording for a kind or for one identity is a data edit.

**Units are unchanged.** `unit_sphere.texts` holds today's six texts verbatim. WP1 proves that every unit prompt is
byte for byte identical by comparing it with a golden list taken before the change (WP1 step 1).

### D-2. The structure texts (Q0020, the agent's decision)

`building_dome.texts.Idle` (375 characters):

> Idle animation, one locked-off static shot: the camera never moves, zooms or drifts. A completely still building,
> fixed rigid geometry, no moving parts: shell, roof, antenna and pipes keep the same shape, size and place
> throughout. Only small ambient effects move: a steady glow, gentle vapour rising, small lights flickering. Seamless
> loop: the last frame matches the first.

`building_dome.texts.Constructing` (359 characters). Structures use it instead of today's shared text. The shared text stays in `unit_sphere` verbatim, in case a unit publishes Constructing (WP1 step 2 checks the shipped catalog):

> Constructing animation, one locked-off static shot: the camera never moves, zooms or drifts. The finished building
> keeps its final shape, size and place in frame throughout while construction plays over it: holographic wireframe
> scaffolding shimmering around it and welding sparks along its seams, fading away toward the end. The last frame
> matches the first.

Constructing is pinned so that it ends on Idle's start picture (seamless-loops D-3). Fading the effects out toward
the end lets the last frame equal the clean pin.

**Identity text.** Remove the word "pulses" from the siphon's authored Idle text in `structures.ron`, which today
contradicts "steady glow". "Plasma pulses slowly through the pipes and a thin wisp of vapour rises from the stack."
becomes "Plasma glows steadily in the pipes and a thin wisp of vapour rises from the stack." This makes the composed
prompt 1,016 characters, inside the 1,024 limit. The other structures have no authored Idle text, so their
lore-based prompts are shorter.

**Negative prompt.** It is plain nouns, as Google's prompt guide advises:

- `veo_request.ron`'s global `negative_prompt` gains "morphing, shape-shifting geometry". No identity should change
  shape. The unit prompts' positive text does not change, but every future request's negative text does. The ledger
  records it for each request.
- `building_dome.negative_terms` = "breathing motion, pulsing size, swelling, shrinking, moving parts, rotating
  parts, antenna changing shape". These words cannot go in the global list, because the unit Idle text asks for
  breathing.

"Flickering geometry" was considered and left out, because a negative "flickering" may suppress the flickering
lights that the positive text asks for.

The negative prompt a clip sends is the global text, then `", "`, then its kind's terms. `PlannedClip` gains
`negative_prompt: Option<String>`. The run sends it for each request (`VeoParameters` built per clip instead of once
per client), so the ledger's `request_started.parameters` records exactly what was sent. The composed negative
prompt is checked with the deny list and placeholder markers, as prompts are.

### D-3. `--seed` and what the dry run and validation show

- `generate-veo --seed <u32>` sets `VeoParameters.seed: Option<u32>` (`#[serde(default, skip_serializing_if =
  "Option::is_none")]`, so every committed ledger line still parses). Every request of that run sends it, and the
  ledger records it in `parameters`. Without the flag, no field is sent and the service picks a seed.
- A run in Gemini mode with `--seed` fails with `SeedNeedsVertex` before any request (§2.3).
- The seed is a run argument and not data, as `--max-generations` is. It picks one sample of a fixed request and
  describes no identity.
- `--dry-run` prints for each clip `action: building_dome Idle` (or `override`), `negative: <full text>` and
  `seed: N` or `seed: none (service picks)`.
- `validate-prompts` checks every text of every kind and override with `check_prompt`. It also checks every paid
  clip's composed prompt and negative prompt, as it does now, and reads no credential.

### D-4. The regeneration series: at most 4 attempts, then stop and rethink

A **series** is a run of attempts at one identity and state with one pin. It holds at most **4 generations**. After
the fourth rejection, or earlier if the reasons show that wording cannot fix the fault, the agent stops spending on
that clip and writes a rethink (below).

**Why 4.** If a wording passes in at least half of its samples, the chance that 4 attempts all fail is
0.5⁴ ≈ 6 %. Four failures in a row therefore say the wording or the method is wrong, and more samples would only
spend money. Four attempts also give the 2 × 2 design below, which can tell sampling noise from a wording effect. A
series costs at most $1.60. The owner prefers quality, so the cap limits spending on a method that does not work. It
does not limit the number of clips.

**What varies.** Only the wording and the seed. The pin, `veo_request.ron` (other than the D-2 negative terms
already committed), model, length and framing stay fixed, so a passing clip can be explained.

| Attempt | Wording | Seed | Purpose |
|---|---|---|---|
| A1 | D-2 texts as committed by WP1 | 1 | Does the structure text fix the faults? |
| A2 | Unchanged | 2 | If A1 failed: is it sampling noise or the wording? |
| A3 | One reviewed rewording aimed at A1's and A2's written reasons | 1 | The wording change is the only deliberate variable relative to A1 |
| A4 | Unchanged from A3 | 2 | As A2 |

Rules:

- **Stop at the first pass.**
- **Seeds 1 and 2 have no meaning beyond being fixed and recorded.** The documented range is 0 to 4294967295.
- **What a rewording may change.** It may change only `building_dome.texts.Idle`, the identity's authored Idle text,
  or `building_dome.negative_terms`. It must change the words that the rejection reasons point to, and nothing else.
- **Every rewording is committed before the spend.** It passes `validate-prompts` and the dry run, so the ledger's
  `prompt_sha256` links to a commit.
- **A rewording meant only for this identity goes in `actions.overrides`.** Edit the kind text only when the reason
  applies to every dome.

**Written reason for every rejection.** The `reject` ledger line's `reason` and a row in the evidence carry four
things:

1. The D-7 criterion that failed.
2. The source frames that show it.
3. The `loop-check` numbers, raw and repaired.
4. A one-line cause hypothesis and the change it implies for the next attempt.

An example: "morph: antenna lattice→rod frames 32–64; worst 0.899; hypothesis: 'antenna' needs a shape; A3 names
'a single thin antenna rod'".

**Rethink after a failed series.** Before any further spend on that clip, write it in the batch evidence. It covers:

- the four reasons side by side;
- whether the faults differ between seeds (noise) or repeat (cause);
- the next method.

Candidate next methods, in the order to consider them:

1. A second series with a different wording hypothesis.
2. An 8 s clip with the atlas sampling only a stable window (needs a plan amendment, because the bake samples the
   whole clip).
3. Veo 3.1 (not Fast), with the price checked and recorded first.

The orchestrator decides which. Under Q0020 a visual choice needs no owner question. A change of model or a higher
price per clip is reported in chat before spending.

### D-5. How this feeds the resumed batch 1

- Every structure Idle and Constructing in batch 1 uses the D-2 texts. Unit clips are unchanged.
- The series rule (D-4) applies to every clip of the batch: at most 4 attempts per identity and state, with a written
  reason for each rejection. It replaces the Veo plan's "at most 3 retries per batch" (Veo plan D-4, WP4). That
  allowance was set before the owner's answers to Q0024 and Q0047 (prefer quality, 60 per 7 days). The run cap
  (12) and the weekly cap (60) still apply, and the quota stop (exit code 3) still waits for the window.
- A seed clip (seamless-loops D-2: frame 0 becomes the pin) is the series' first attempt. If it passes outright it
  is accepted. If its only fault is loop continuity, frame 0 becomes the pin and the attempt counts.
- The siphon's Constructing runs in WP3 from the pin that WP2 accepted, under the series rule.
- Worst case for batch 1: 13 clips (9 plus about 4 seeds) × 4 attempts = 52 generations. With WP2's 4 and the 7
  spent, that is 63, which crosses the window of 60. The run then stops at the cap, which is the existing behaviour,
  and resumes when the window frees up (the first 5 starts leave it on 2026-11-11 from 10:01Z). The money stays far
  inside 200 EUR (63 × $0.40 = $25.20).

---

## 4. Work packages

Common rules: as the seamless-loops plan §4.

- Use your own worktree, with `CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/actiontext-wpN`.
- Put logs under `~/scratch/logs/veo_actiontext_*`.
- Write failing tests first and paste the red run in the report.
- Write `//!` and `///` docs, and use typed errors. No `unwrap`, and `expect` only with `invariant:`.
- Put no catalog identity in Rust.
- Never read `.env` or `tmp/`.
- Check `docs/questions/open/` before each push.

Gate: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` and `cargo test
--workspace --locked`. No test makes a network call.

### WP1: Action text per kind, negative terms, seed (no spend)

Files: `crates/asset_baker/src/{prompts.rs, veo_plan.rs, veo_run.rs, veo_client.rs, error.rs, main.rs}`,
`assets/catalog/media_style.ron`, `assets/catalog/structures.ron` (the siphon's Idle wording only),
`assets/media/veo_request.ron`, tests `prompt_tests.rs`, `veo_tests.rs`, `veo_cli_tests.rs`, `ledger_tests.rs`, and
a new fixture `crates/asset_baker/tests/fixtures/unit_prompts_golden.txt`.

1. **Before any change**, on the unchanged code, write the golden list: one line per unit identity (`UnitSphere`)
   and published state, holding `slug state sha256(prompt)`. Commit it in the WP's first commit.
2. Write these tests first and watch them fail:
   - every unit prompt matches the golden list;
   - the composed `plasma_siphon_dome` Idle prompt contains the `building_dome` Idle text, and contains neither
     "breathing" nor "radar";
   - every structure's Constructing uses the `building_dome` text;
   - an override changes only its slug and state;
   - load fails with `MissingActionText` when `building_dome` Idle is removed;
   - load fails with `UnknownActionOverride` for an unknown slug;
   - an override for an unpublished state fails with `DataFile`;
   - the old flat `actions` shape is refused;
   - every shipped text of every kind passes `check_prompt` (replacing
     `the_shipped_style_passes_its_own_rules_and_names_no_franchise`'s loop);
   - every paid clip's composed prompt fits `max_prompt_chars`;
   - on the `FakeServer`, a structure request's `parameters.negativePrompt` is the global text plus the
     `building_dome` terms, and a unit request's is the global text only, and the ledger records each;
   - `--seed 7` sends `parameters.seed: 7` and the ledger records it; without the flag there is no `seed` field;
     every line of the committed `docs/evidence/veo/ledger.jsonl` still parses;
   - Gemini mode with `--seed` fails with `SeedNeedsVertex` and the fake server sees no request;
   - the dry run prints the `action:`, `negative:` and `seed:` lines.
3. Implement D-1 to D-3. Write the data edits of D-2 exactly as quoted, and update the comments at the head of
   `media_style.ron` and `veo_request.ron`.
4. Run `validate-prompts` on the shipped data, and `generate-veo --unit plasma_siphon_dome --states idle --seed 1
   --dry-run`. Paste both outputs in the report.

### WP2: The siphon Idle series (at most 4 generations)

1. **Verify** the pin's antenna: look at `assets/ai_keyframes/plasma_siphon_dome_idle.png` at full size and record in
   the evidence what the antenna is (lattice mast or thin rod). If A3 needs to name it, the name describes the pin,
   not a new design.
2. Run the series of D-4. Each attempt is one logged background run: `generate-veo --unit plasma_siphon_dome --states
   idle --max-generations 1 --allow-regeneration --seed N`, with the ledger and directories as in the
   seamless-loops WP4. After each attempt:
   - run `loop-check` (raw and repaired);
   - make the detail and seam sheets into `docs/evidence/veo/review/`;
   - do the D-7 review;
   - accept it, or reject it with the written reason of D-4.

   Every rewording (A3) is committed before its spend.
3. Evidence: a section "Structure action text series" in `docs/evidence/veo/2026-11-04-batch-1.md`. It holds:
   - one row per attempt (run id, ledger lines, clip, seed, prompt SHA, loop-check numbers, verdict, reason);
   - whether the service accepted `seed` with `lastFrame`;
   - the running total.
4. On a pass: install the clip with `install-clip`, run `audit-media`, and record the result. After 4 rejections:
   write the rethink of D-4 and stop. The orchestrator decides the next step.

### WP3: Resume batch 1

Run the seamless-loops WP5 as written, with D-5 here:

- structure texts for every dome;
- the series rule for every clip, replacing the batch retry allowance;
- the siphon's Constructing from the accepted pin.

The evidence carries a reason row for each rejection, as in WP2.

---

## 5. Risks

| Risk | Handling |
|---|---|
| A still building reads as a still image ("action readable at 512 px" fails) | The text keeps the vapour, the glow and the flickering lights. If review finds too little motion, A3 asks for more vapour, never for moving parts |
| Locked-camera wording still breathes | The negative terms add breathing and pulsing size. `loop-check` growth measures it. A3 or a rethink |
| Vertex refuses `seed` with `lastFrame` | The start error is not billed. The series continues on wording only (§2.3) |
| The same seed with new wording gives an unrelated sample | Expected ("guides"). The 2 × 2 design compares across both seeds, not single pairs |
| Negative "morphing" changes future unit clips | Intended: no identity should morph. The ledger records the exact text of each request |
| Prompt length near 1,024 | A test checks every paid clip. A longer rewording shortens the kind text or moves wording to an override |

---

## 6. Changes to other plans

- **Seamless-loops plan, WP5:** structure texts and the series rule as D-5 here. Its index row says so.
- **Veo plan, D-4 and WP4:** "at most 3 retries per batch" becomes the per-clip series of D-4 here for batch 1 and
  later batches. The run cap and the weekly cap are unchanged.
