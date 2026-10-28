# Round 2: External rule catalogs and injected simulation construction

Status: architect blueprint awaiting orchestrator approval.

## Goal and integration contract

The normal game startup assembled by plan 28 must use the selected asset directory's actual `catalog/units.ron`, `catalog/structures.ron`, and `catalog/upgrades.ron`. Editing valid declarative rules and restarting the same compiled executable must change spawned entities and subsequent gameplay. Missing or invalid selected files must produce typed, path-bearing errors, never quietly load embedded replacements.

This lane supplies validated catalog loading and injection; plan 28 owns normal client startup, scenario configuration, and the visible map. Neither lane can claim normal-startup completion until that client caller passes the loaded catalogs into the APIs below. Preserve existing embedded constructors for explicit tests, compatibility, and tools. No editor source or map files are changed here.

## Evidence

`SimWorld::new` currently creates `UnitCatalog::new_starter_roster()` and `UpgradeCatalog::default_catalog()` internally. Both use compile-time `include_str!`. Existing `scenario::build_world` calls that constructor, so selecting external map/loadout assets still does not select external gameplay rules. `CatalogResource::default` also embeds rules. Unit slug lookup scans every archetype, and public `register` / `get_mut` permit changes that would invalidate a naive added lookup table. Structure IDs currently narrow `u32` to `u16`, potentially aliasing another archetype. These are concrete integration constraints, not reasons to substitute new defaults.

## Ownership

Own `crates/sim_core/src/catalog.rs`, narrowly scoped typed error additions in `error.rs`, constructor extraction in `world.rs`, wrapper/extraction in `scenario.rs`, catalog integration tests, and tests for injected scenarios. A small new `catalog/runtime.rs` module may keep filesystem loading separate. If needed, add narrow authored-identity validation to `upgrades.rs`; do not change upgrade mechanics. No movement/combat changes. Plan 28 owns client calls/resources and must consume these APIs. Preserve all prior dirty work and round 1 checksum changes.

## Public API agreed with plan 28

```rust
pub struct RuntimeCatalogs {
    pub units: UnitCatalog,
    pub upgrades: UpgradeCatalog,
}

impl RuntimeCatalogs {
    pub fn load_from_directory(directory: impl AsRef<Path>)
        -> Result<Self, CatalogLoadError>;
}

impl SimWorld {
    pub fn with_catalogs(
        map: SimMap,
        seed: u64,
        catalog: UnitCatalog,
        upgrade_catalog: UpgradeCatalog,
    ) -> Self;
}

pub fn build_world_with_catalogs(
    doc: &MapDocument,
    loadouts: &StartLoadoutCatalog,
    players: &[PlayerSetup],
    seed: u64,
    catalog: UnitCatalog,
    upgrade_catalog: UpgradeCatalog,
) -> Result<SimWorld, ScenarioError>;
```

Place `RuntimeCatalogs` and `CatalogLoadError` under `sim_core::catalog` so the client can import them directly without an unrelated crate-root API change. The directory argument is the catalog directory itself: the client calls `RuntimeCatalogs::load_from_directory(asset_root.join("catalog"))`.

Extract all current initialization from `SimWorld::new` into `with_catalogs`, consuming the provided catalogs directly. `new` becomes the explicit embedded-compatibility wrapper. Do not construct embedded catalogs first and overwrite them afterward: that would retain unnecessary compilation dependencies in the runtime path and could hide invalid file handling. Initial checksum must be computed only after the supplied catalog fields are installed.

Extract existing scenario construction into `build_world_with_catalogs`; `build_world` delegates with the existing embedded catalogs. Validation, ordered placement, typed scenario errors, and final checksum refresh remain unchanged. Scenario placement must resolve its slugs against the injected unit catalog. This function does not read disk or select client defaults.

## External loading and error model

Load the exact three files from the specified directory, in deterministic units/structures/upgrades order. No fallback roots, environment search, path replacement, embedded rescue, or success-with-empty-error handling. Unit and structure RON consolidate into one `UnitCatalog`; upgrades retain their separate catalog. Return only after all three have loaded and validated.

`CatalogLoadError` uses `thiserror` with structured variants such as `Io { path: PathBuf, source: io::Error }` and `Invalid { path: PathBuf, source: SimCoreError }`. Each path identifies the failing selected file. Reuse existing parsing APIs where possible while preserving path attribution (parse units first, then append structures so the failing file is known). Do not flatten the entire outer error into an untyped string.

Add explicit authored-catalog identity errors in `SimCoreError`: duplicate numeric ID, duplicate slug under the existing ASCII-insensitive lookup semantics, blank slug, and out-of-range structure ID/footprint. Reject duplicate unit/structure IDs across both input files and case-fold-equivalent slugs before any supplied definition silently replaces another. Reject `structure_type_id > u16::MAX`; validate footprint width and height before their `u8` conversion, requiring nonzero representable dimensions. Preserve the existing file schema and valid catalog behavior.

Existing `register` intentionally supports replacement in tests and tooling; do not turn it into a silently ignored `Result`. Keep this compatibility entry point and its documented replacement behavior. Introduce an internal or public checked registration route for authored data. `load_structures_from_ron_str` must be atomic on error: validate parsed additions and their identity conflicts against the existing catalog before mutation, or stage the modified catalog and commit only on success. A later failing structure must not leave earlier structures installed.

Upgrade authored parsing likewise rejects duplicate IDs/slugs if its existing parser would overwrite them. Keep current upgrade lookup semantics (case sensitivity) rather than silently changing authored reference resolution. This is identity validation only; a broad prerequisite-cycle or balance-validator project is not part of this round.

Runtime numeric input must not panic during DTO conversion. Check floating numeric fields that feed fixed-point conversion for finiteness and Q32.32 representability before calling existing converters; reject invalid values with a field/definition-specific typed error. Keep current f32/f64 quantization for valid values. The representable f32/f64 check is finite and in `[i32::MIN, i32::MAX + 1)` simulation units; for supported binary floats this correctly excludes the unrepresentable upper endpoint. Check angular input finiteness before existing bounded conversion. Walk nested weapons, splash, overclock, and mode overrides as well as base unit/structure fields. Existing `SimScalar` fields in ability definitions already use the checked deserializer introduced by round 1. Do not claim complete economic/balance validation; this checks safe decoding and identities.

## Deterministic slug index without stale-cache bugs

The steady-state slug lookup must use a sorted index rather than scan all archetypes or allocate a normalized query string every call. A private sorted `Vec<UnitTypeId>` ordered by ASCII-folded slug bytes and then numeric ID is sufficient. Compare query bytes with ASCII folding during binary search; this preserves case-insensitive behavior without allocating. Iterate `archetypes()` by numeric ID exactly as today.

Preserve the public `get_mut(id) -> SimResult<&mut UnitArchetype>` and `register` APIs. Because `get_mut` permits changing a slug after it returns, an eager index cannot remain sound. Use a per-catalog `std::sync::OnceLock<Vec<UnitTypeId>>` derived cache: invalidate it on every successful mutable borrow and registration, then rebuild on the next lookup from current archetypes. Fully authored loaded catalogs should initialize their index before returning so normal game queries do no initialization work. This is local derived state, not authoritative global state. It keeps Bevy's Send/Sync requirements and makes field mutation safe without a breaking guard type. Clone must preserve a valid cache or deliberately start uninitialized, never copy invalid contents.

Legacy unchecked edits can produce duplicate slugs; preserve the old deterministic first-by-ID lookup behavior by sorting ties by storage ID and selecting the first equal entry. Authored loading rejects those duplicates. Document this distinction explicitly. Cache entries must address storage slots, not trust a mutable `archetype.unit_type_id` field as an unchecked vector index. The public `get_by_name` human-facing search can retain its current behavior; do not broaden the round into display-name indexing.

Document the performance rationale: startup/mutation performs one O(N log N) rebuild and O(N) storage allocation; steady-state canonical or case-insensitive slug queries use O(log N) comparisons with no query allocation. Do not claim zero allocation for first lookup after a legacy mutable edit.

## Tests and proof of runtime behavior

1. Load the repository's exact external three-file catalog bundle and construct an injected world. Assert representative base HP, weapons, and upgrade data match the loaded definitions. Resolve archetypes by capabilities/iteration, never hardcode unit identities.
2. Create temporary complete catalog bundles from real assets, change one selected unit's HP or damage field using parsed RON data or a validated unique field transformation, and load both bundles with the same test executable. Spawn that dynamically selected archetype into separate worlds and assert the changed vital/damage is visible. This is the regression proving no recompilation is necessary; merely checking the file exists is insufficient.
3. Build a scenario with `build_world_with_catalogs` using an injected unit catalog whose selected loadout archetype has modified vitals. Verify actual starting entity values and stable placement, then compare two identical injected builds for deterministic initial checksums. Existing embedded scenario tests still pass.
4. Missing each file individually, malformed RON in each, duplicate IDs within units, duplicate ID across unit/structure files, duplicate case-folded unit slugs, duplicate upgrade identities, oversized structure ID, invalid footprint, and nonfinite/out-of-range nested numeric data must yield typed errors identifying the correct file or definition. Test atomicity when a later structure fails.
5. Index tests cover sorted and reverse registration order, mixed-case lookup, absent queries, replacement at an existing ID, renaming through `get_mut`, identity-field mutation through `get_mut`, cloning before/after initialization, and deterministic minimum-storage-ID tie resolution for legacy unchecked duplicate slugs. These prevent a fast but stale index from passing simple happy-path tests.
6. Client plan 28 must separately verify its actual normal startup resource uses the loaded bundle and selected map. Integration acceptance needs that call-site evidence; this backend lane alone does not demonstrate normal startup.

Use dedicated temporary directories with deterministic cleanup and no global CWD mutation. No fabricated replacement catalog should silently cover a missing real asset. Synthetic conflicting definitions are acceptable negative-test inputs, not production creature identifiers.

Run focused catalog/scenario tests and relevant existing combat tests after extraction. At the shared stable-source gate run `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --all -- --check`. An independent verification round must inspect actual normal client startup call paths, file error propagation, mutation/cache invalidation, and the same-binary edited-catalog regression. Backend changes require no screenshot; plan 28's client integration does.

## Explicit exclusions

Catalog compatibility fingerprints for network peers remain a separate goal. The dynamic checksum's identical-catalog prerequisite continues to apply. Do not imply that two worlds with different unspawned archetype definitions must have different dynamic digests. Do not add hot reload during an active match, new units, balancing changes, map-editor integration, or new mechanics. Restarting with the same executable and valid edited assets is the required authoring workflow for this round.
