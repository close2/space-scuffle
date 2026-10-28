# Round 5 media integrity verification

Scope: approved plan39 stage one: truthful read-only audit, retired unsafe single-concept roster baker, exact CPU/GPU registration, and typed production admission. Existing image/video files are unchanged. Declared source baking, immutable video staging and provenance publishing remain a separately staged follow-up; this implementation does not invent historical provenance.

## Implemented contract

- `asset_baker::media` reads exactly the selected portable slug's PNG/RON. Metadata identity, supported layout, nonempty explicit actions, checked frame ranges/strip heights/byte counts, actual PNG dimensions and actual renderer texture limits are checked together. Hashing and parsing/decoding consume the same read buffers. GPU limits are only claimed when supplied by the actual device; CPU tests make no rendering claim.
- Metadata rejects unknown fields and duplicate action keys rather than accepting last-wins maps. Portable filename admission accepts ASCII letters/digits/underscore/hyphen and rejects Windows device names on every host.
- `audit-media` uses the external runtime catalog, emits stable slug-ordered JSON and explicit duplicate/action/alpha evidence, refuses existing report destinations, and reports exact-path output failures. Digests describe encoded file bytes, not artistic authenticity.
- The retired `generate-defaults` command and deprecated batch API fail without reading/generating outputs. Procedural algorithm APIs and old source files remain intact; no production fallback or new media is generated.
- Production registration retains typed unavailable entries and only publishes exact own-file handles/metadata. Neutral/faction/shape/first-loaded texture substitution is removed. Requests compare current registry binding against retained admission evidence, including exact metadata and texture handle. Missing actions never use another slice.
- Live gameplay checks initial media before ticking and every emitted snapshot before rendering. Lab and swarm admit required actions before materials. Standalone mist/shell capture admits its actual Idle/Walk action. Capture manifests consume production media failures.
- Normal unbounded gameplay retains a readable failure and pauses. Typed evidence and cached failure text survive tab/reset changes; native runner closing after a failure returns nonzero after orderly teardown. Bounded frame-limit/capture runs fail immediately. Native UI acceptance is recorded below; setting a resource alone was not used as proof of readable presentation.

## Actual evidence so far

- First asset integration round: five tests pass, `/tmp/space-scuffle-r5-media-tests01.log`. They cover real catalog media, dimensions with identical byte counts, arithmetic/device bounds, exact missing/corrupt paths, ambiguous metadata, portable names and no-write retired CLI/API behavior.
- Repeated portable-policy round: five pass, `/tmp/space-scuffle-r5-media-tests02.log`.
- New media plus existing manifest tests: five + five pass, `/tmp/space-scuffle-r5-media-tests03.log`. Also verifies report refusal preserves existing bytes and report output errors retain the requested path.
- Production ECS admission: three new tests plus existing simulation-diagnostic test pass, `/tmp/space-scuffle-r5-client-media-tests01.log`. Uses actual PNGs and deliberately damaged metadata copies in temporary roots, never replacement game art. Tests exact registry, absent unrequested IDs, mutated handle rejection, missing starting PNG before tick advance, retained ordinary failure/nonzero eventual close, and immediate Lab/Swarm missing-action failure before material creation.
- Actual CLI audit returns **1** under strict policy; `/tmp/space-scuffle-r5-current-media-audit.json` and `.log`: **91 identities = 81 MissingAsset + 6 KnownConceptDuplicate + 4 LegacyUnverified**. `strict_accepted` is false. All legacy art remains unapproved.
- Initial strict Clippy found one new test assertion style issue, repaired. Subsequent runs were blocked by concurrent plan36 missing-module/MSRV/semicolon work; these are recorded rather than claiming a clean gate. Final coordinated checks are pending.

Native negative-root setup is reproducible with `/tmp/space-scuffle-r5-media-negative-root.py --assets assets --out NEW_DIRECTORY --missing-relative textures/SELECTED_REQUIRED_SLUG.png`; it links unchanged files and omits only the explicitly selected PNG. The previous proven required-building fixture `/tmp/space-scuffle-r4-missing-media-assets` remains available. No source asset is changed.

## Acceptance boundaries

Structural/runtime integrity is separate from provenance and artistic quality. Current normal startup may preserve existing own-file legacy imagery under the approved migration policy, with explicit unapproved status. Strict audit remains failing; no default artwork is accepted by this round. The 81 absent assets and six concept copies still require real unit-specific media; four other named source sets still lack verified build records and artistic acceptance.

The last shared full workspace gate failed the unchanged pathfinding performance threshold while deterministic checks and earlier client/simulation suites passed. The traversal owner is addressing that separately. This report does not count the failed gate as acceptance, and no editor changes are part of this work.

Final focused source checkpoint: `runtime_capture_tests` 5 and `runtime_media_tests` 3 pass in `/tmp/space-scuffle-r5-client-media-tests02.log`, including attempted reset clearing the presentation string while the typed failure remains retained and progression stays paused. Strict asset_baker + client all-target Clippy passes `/tmp/space-scuffle-r5-media-clippy04.log`; full workspace fmt check passes `/tmp/space-scuffle-r5-media-fmt01.log`. Baseline third-party Naga lifetime warnings remain printed, with no targeted workspace lint failure. Product source is frozen for the independent native check. Plan36's new nonzero camera viewport exposed a viewport-relative versus full-window pixel mismatch in the capture tool; the owner is correcting the shared conversion while preserving the visibility/ROI assertions. That capture failure is not a media acceptance result.

Independent root native negative: `/tmp/space-scuffle-r5-independent-missing-media-capture` exits **1**, writes an incomplete manifest naming the exact required missing PNG, and accepts **zero checkpoints**. Its `.log` records the real AMD renderer joining 11 workers on error, no audio initialization/validation error/panic; one egui node-data warning during shutdown is retained as evidence. The original source image still exists unchanged. Ordinary readable-panel screenshot and eventual-close verification subsequently passed as recorded below.


## Independent native verification after viewport integration

The orchestrator captured `/tmp/space-scuffle-r6-media-failure-window/failure-panel.png` from the actual ordinary client at physical **1280×720**, native scale **1.59375**. This agent opened and inspected the image: the red error is readable near the top of the persistent sidebar, includes the exact required PNG path and operating-system cause, and the game reports **Tick 0**. The failure is not hidden in a collapsed inspector or replaced by fallback imagery. The visible Resume control does not establish that progression can resume; the focused retained-failure regression verifies that progression stays paused after attempts to clear the presentation state.

The orchestrator verified process/window ownership before capture and WM_DELETE; normal close returned **1**. Its 1637-byte log contains no POLLERR, validation error or panic. This proves readable ordinary failure retention plus eventual nonzero close, complementing the automated capture's immediate rejection and zero accepted checkpoints. Source art remains unchanged.

The rebuilt positive production run `/tmp/space-scuffle-r6-viewport-battle` completed all **seven checkpoints** and exited **0**. The orchestrator's independent comparison, `/tmp/space-scuffle-r6-independent-capture-review.json`, reports unchanged simulation values against the baseline; viewport visibility counts differ as expected after the actual viewport correction. This establishes runtime/media admission compatibility, not acceptance of the legacy artwork or proof of source provenance. The broader source-baking stage remains pending, and full workspace acceptance awaits the next coordinated gate.
