# Round 1 — Fail explicitly on an invalid runtime asset root

Status: planning complete; awaiting orchestrator approval before implementation.

## Goal and evidence

Ensure the normal game binary validates and resolves its runtime asset directory before initializing Bevy. An explicit `SPACE_SCUFFLE_ASSETS` override must either identify a real asset root or produce a useful error and unsuccessful process exit; a broken override must never silently select unrelated assets. Automatic discovery must ignore arbitrary directories named `assets` and return an absolute canonical path to a root containing the real `shaders/bubble_dome.wgsl` marker file.

Review F-14 is confirmed in `crates/client/src/lib.rs`: the existing condition `marker.exists() || candidate.exists()` accepts any existing assets directory and a final relative fallback hides failure. The legacy resolver is also called by `editor/app.rs`, `editor/ui.rs`, and editor tests. The user's explicit exclusion of map-editor work therefore constrains this round to an additive fallible API and its non-editor startup integration. The legacy editor API remains untouched and the remaining migration must be reported explicitly rather than falsely claiming all F-14 call sites are fixed.

Inspection also found separate manifest issues: `FlipbookAtlasMeta::validate` uses saturating arithmetic for action endpoints and grid capacity, and `setup_bubble_assets_system` silently skips invalid files and validates byte count rather than image shape. Those are independent follow-up goals, not changes in this round. Do not modify manifests, image packing, FFmpeg imports, shaders, or renderer behavior under this plan.

## Ownership and exclusions

Own a new `crates/client/src/assets.rs`, new `crates/client/tests/runtime_assets_tests.rs`, and a narrow integration hunk in `crates/client/src/lib.rs` (module/export declaration plus `run_app` startup). Also own only the `setup_bubble_assets_system` parameter and asset-root selection lines in `crates/client/src/bubble_shader.rs`; no image or rendering logic changes. If process testing requires it, own a small nonvisual change in `crates/client/src/main.rs`. `thiserror` and test `tempfile` are already dependencies; no dependency additions should be necessary.

Do not touch any `editor/` source or editor tests, `terrain/`, map files, catalog data, `sim_core`, or existing rendering logic. Preserve all preexisting modifications in `client/src/lib.rs`.

## Public API and error model

Introduce `try_resolve_assets_dir() -> Result<PathBuf, AssetsDirError>` in the new module and re-export it from the crate root. Keep the existing `resolve_assets_dir() -> PathBuf` signature and implementation for untouched legacy consumers; clearly document that normal startup uses the checked API and that migrating legacy editor consumers is outside this round.

Use a `thiserror` enum with distinguishable failures for an invalid explicit override, path inspection/canonicalization I/O errors, current-directory lookup failure if discovery needs it, and no valid root found. Diagnostics must name the supplied path or searched candidates and required marker. Retain underlying `std::io::Error` as a source where applicable. Handle non-Unicode environment paths with `var_os`, not lossy omission.

Factor path selection into a deterministic helper taking explicit override and search roots as arguments. Tests must not mutate process-global environment or working directory in parallel. The production wrapper reads environment/current directory once and delegates. The helper may be public with clear rustdoc if integration tests need it, or private with adjacent unit tests plus black-box public API tests.

## Resolution algorithm

1. If `SPACE_SCUFFLE_ASSETS` is set, validate exactly that directory. An empty override is invalid, not the current directory. Require a directory and a regular marker file `shaders/bubble_dome.wgsl`; a directory with that filename is invalid. Symlinks resolving to real directories/files are acceptable. Canonicalize the successful root and return it. On any failure, return a typed error immediately; never continue automatic discovery.
2. Otherwise inspect the runtime `CARGO_MANIFEST_DIR` search root, if present, walking ancestors and testing each `assets` child. Then inspect the current working directory and its ancestors. Keep documented precedence. Do not silently substitute a compile-time checkout path, which can select stale development assets for a packaged game.
3. Missing candidates or candidates missing the marker are ordinary discovery misses. Other filesystem errors should retain path context and become errors rather than being suppressed by `Path::exists`/`is_file` convenience methods. Use metadata with explicit error handling. Reject non-file markers. Canonicalize only validated successful roots.
4. If no candidate validates, return an error listing useful search context; never return `PathBuf::from("assets")`.

The marker establishes the identity of the asset root, not that every shader compiles or every media asset is valid. Keep the diagnostics and final report precise about this limit.

## Non-editor startup integration

In `run_app`, keep the existing intentionally headless simulation-check path independent of GPU assets. Before constructing `DefaultPlugins` for any graphical game mode, call the checked resolver. On success, feed that exact resolved path into Bevy's `AssetPlugin.file_path`. On failure, emit the error to stderr and terminate with a failing exit status before window/GPU initialization, or return a typed startup error to `main` if that can be done without touching legacy callers. Prefer existing application's explicit error exit convention for this narrow change.

Introduce a documented `ResolvedAssetsRoot` Bevy resource carrying the checked absolute path. Insert it in `run_app` before plugins/startup, and derive both `AssetPlugin.file_path` and this resource from the same resolved value. Add an optional resource parameter to `setup_bubble_assets_system`: when provided, load textures strictly from this path without re-running discovery. Preserve its legacy resolution only for direct embedding/editor callers that do not supply the resource. This distinction is essential: checked discovery can skip an empty nearby `assets` directory that the legacy resolver would select. A small pure path-selection helper may make this shared-root invariant directly testable. Do not claim direct construction of `ClientAppPlugin` without the resource gains checked startup guarantees.

## Verification round and acceptance criteria

Unit tests with isolated temporary filesystem trees must cover:

- Valid explicit root returns its canonical absolute path, including a relative root when provided through the deterministic helper.
- Missing override, empty override, regular-file root, missing marker, and marker that is itself a directory all return the expected error variant.
- Invalid explicit override does not fall back even when a valid discovered root exists.
- Discovery ignores an empty nearby `assets` directory and finds a valid ancestor; manifest-root precedence is deterministic; current-directory ancestry works without manifest roots.
- No valid root yields a typed not-found error with useful context.
- A stale nearby empty `assets` directory cannot override a supplied `ResolvedAssetsRoot`: assert the actual path-selection helper used by bubble setup chooses the validated root's `textures` directory, matching Bevy's configured root.
- Symlink/non-Unicode cases where supported by the target platform, gated only by actual platform capabilities.

Integration tests must resolve the repository's actual asset root through the checked API and read its real marker file. Also spawn the actual `client` binary with `--battle --max-frames 1`, an invalid asset override, and no display dependency: assert a nonzero exit plus a diagnostic identifying the asset configuration error. This proves early rejection before renderer startup rather than merely testing a helper. Do not use `--headless-check` for that assertion because it intentionally bypasses visual assets.

Run focused client asset tests, then coordinate all required workspace gates: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --all -- --check`. The change introduces no visual output and therefore needs no new screenshot claim. Real render verification remains required for future visual changes.

Acceptance means normal graphical startup cannot proceed with an invalid explicit root or arbitrary empty `assets` folder, diagnostics explain the failure, and editor files remain unchanged. The implementation report must disclose the retained legacy resolver and avoid claiming manifest validation or rendered appearance has been improved.
