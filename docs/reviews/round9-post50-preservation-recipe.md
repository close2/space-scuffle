# Post50 candidate preservation recipe

Status: the combined workspace gate passed all three checks with unchanged source. Root authorized source preservation; corrected script02 completed successfully and independent verification passed. Builds, fixture smoke, timing and51 product implementation remain held pending root's acceptance/release. The first preservation attempt rejected a known internal asset symlink before creating any candidate; this was a capture-tool assumption failure, not a product failure. Corrected script02 follows the exact reviewed exception below. The candidate was preserved **before any51 product edit**.

The immutable baseline is `/tmp/space-scuffle-r7-pre50`. Its source-inventory SHA256 is `92387e5244704928f8ebabd425661b85fcf5b99f4f4aac47058cf7cdead67886`; final fixture02 binary is `02d7b7fbd253d97df6e8a8f65fdc293f94f34c09c1a819f759ce5dd31991583a`. Preserve its existing files unchanged. Three complete baseline matrices were inconclusive, not passing performance evidence.

## Preservation command, only after release

Run from the repository after root's source freeze. The snapshot includes complete crate and patched-dependency trees, not only `.rs` files: the original inventory contains shaders, crate media, manifests, licenses and build inputs. Include new50 files and retain the workspace lock and exact catalog inputs. Materialize only the reviewed internal `crates/client/assets -> ../../assets` link, recording its literal and resolved target. Reject every other symlink instead of silently dereferencing inputs outside the recorded tree. Hash each copied buffer, verify source inventories before/after, and refuse to reuse an existing candidate directory. A failed partial snapshot is retained as failure evidence and is never accepted as a candidate.

```sh
python3 /tmp/space-scuffle-r9-preserve-post50-02.py > /tmp/space-scuffle-r9-preserve-post50-02.log 2>&1
```

The original failed script remains `/tmp/space-scuffle-r9-preserve-post50.py`. Corrected script02 is separate and preserves the original baseline/harness/hash checks. Its sole link exception requires the logical link path exactly `crates/client/assets`, literal target exactly `../../assets`, and strict resolved target exactly `/home/cl/projects/space-scuffle/assets`, which must itself be a regular directory. It recursively inventories **every file** of that asset tree under logical `crates/client/assets`, rejects nested/unexpected links and special files, and materializes regular read-only files in the snapshot. No media is skipped; the original baseline also materialized this directory. Normal crate/dependency inventory still excludes build `target` and `.git` trees; those exclusions do not omit anything inside the reviewed asset tree. The destination contains no symlinks.

The script records link provenance and its own SHA256, checks the link before and after copy, requires same-buffer copied hashes and equal before/after live inventories, verifies copied inventory independently, and aborts on changed input. Catalog and byte-identical runner checks remain unchanged. It does not build or run the candidate. This narrow capture assumption repair was explicitly authorized by root after the first failed attempt.

Review `preservation.json`, compare the source inventory against the completed gate's frozen source fingerprint, and independently rehash both inventories before accepting the snapshot. Snapshot hashes, not Git HEAD, identify the dirty working source. Save the gate result JSON/hash beside the provenance. The read-only pre-release check found all eight baseline catalog files match; the command repeats that check at preservation time. Unexpected deletion, symlink or configuration requires review, not an exclusion added to force success.

## Isolated release build, separately coordinated

Before building, record `rustc -Vv` and `cargo -V` and require exact equality to `build-provenance02.json`; otherwise review toolchain drift. Record the path and SHA256 of applicable ancestor/CARGO_HOME Cargo configuration, compiler wrappers and explicit build flags without dumping unrelated environment or secrets. Baseline provenance does not record all environment/configuration details, so a discovered nonempty custom wrapper/flag/config is an unresolved comparison confound, not automatically baseline-equivalent. The normal empty-override environment uses the following exact build command from the preserved runner directory:

```sh
cd /tmp/space-scuffle-r9-post50/runner
cargo build --release --locked --target-dir target > /tmp/space-scuffle-r9-post50/build.log 2>&1
```

The runner is its own standalone workspace; its unchanged relative paths select preserved sim crates. Do not copy the workspace target or use the live workspace release binary. Require exit0, byte-identical runner source/manifest/lock after build, unchanged copied source inventory and exact executable hash. Record the complete command, working directory, toolchain, host/target, release profile, relevant configuration hashes, exit status, build-log hash, binary path/hash and input inventory hashes in candidate `build-provenance.json`. Make the executable read-only after hashing. Lockfile changes or attempted resolution failure are evidence to investigate, never justification to update the frozen lock.

## Harness compatibility and permitted smoke

The runner never calls the50-changed public helper signatures. Its source uses only catalog/world constructors, setup spawn, typed Move admission, public movement/collision phases, diagnostics/ledger and checksum; those APIs remain unchanged. The exact source/manifest/lock hashes above prove there is no candidate-specific adapter. New50 modules must appear in the candidate inventory. This comparison includes integrated checkpoint changes, so report any additional sim source differences relative to pre50 rather than attributing all differences to50 by assumption.

Only after root coordinates a non-acceptance smoke, invoke the preserved executable for `ground`, `air`, `dense`, each with100 and400 actors, and two samples each (`movement_benchmark ground 100 2`, etc.). Retain JSONL and stderr. Require each sample's fixed128 warmup/256 measured phases, exact actor count,100% active intents/moving actors/changed positions, zero diagnostics, successful internal supply ledger validation and expected dense contact counts (100:50 friendly/40 enemy;400:200/180). Require identical checksums across the two samples of each revision/case. Candidate checksum need not equal baseline because widened normalization may change trajectories. Reject changed workload behavior before performance comparison. Discard smoke durations as acceptance evidence.

The later quiet paired collector remains separately authorized: seven sample pairs per case/count, alternating AB/BA order on CPU3 with read-only sibling/topology/frequency evidence, verified binaries/inventories and raw per-phase values. Mean/p95/CV and tiny-baseline thresholds remain exactly plan50's. No fourth automatic baseline, relaxed threshold, favorable subset or smoke timing is an accepted substitute.

## Executed source-preservation evidence

`/tmp/space-scuffle-r9-preserve-post50-02.py` completed exit0; log `/tmp/space-scuffle-r9-preserve-post50-02.log`. Script SHA256: `b7c94b44bebf27e98d878810f0df0da53dfffc6ec357e12b97d23fa96308eefd`. Original failed script is retained unchanged. Candidate `/tmp/space-scuffle-r9-post50` contains446 regular read-only source files, including all16 new paths since the430-file baseline. Its source-inventory SHA256 is `c0a670382d1a2194844a1cdc40ad3f336046ac9242476b1b897cea2f90ba65bf`; runner-inventory SHA256 is `503a91cf1f1f82c51e82c07a434c31efe22ff0951e41ceaffa697e27f78b1e91`.

A separate read-only verification rehashed every preserved source file against both inventory and live source, checked no snapshot symlinks/write permissions, checked all three harness files against baseline, and matched all eight catalog files. It covered all343 gate fingerprint entries, mapping root asset paths to the materialized logical asset tree when necessary. Gate fingerprint SHA256 is `1b9d270a52d9feba331663ba00e73cda7342978d90266d4867a6338d31fe69ea`. Evidence is candidate `independent-verification.json`, `preservation.json` and copied gate result/before/after/change JSON files. No candidate build, fixture smoke or timing was executed.
