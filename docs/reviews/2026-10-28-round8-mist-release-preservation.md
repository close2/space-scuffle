# Persistent mist release preservation recipe

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Run this recipe only after the coordinated gate passes and while the orchestrator still holds the source freeze. The snapshot preserves the tested current workspace, including concurrent approved simulation/client work; it is not presented as a one-file derivative of the older dynamic-mesh baseline. Its inventory and complete baseline difference identify that distinction. The baseline snapshots and executable remain untouched. No snapshot, release build or measurement has been performed by writing this recipe.

The build/runtime allowlist is the root Cargo manifest/lock/toolchain/charter, all five crate trees, all actual assets and the reviewed vendored compiler tree. Git state, credentials, tool state, generated targets and verification artifacts under `tmp/verification`, plus external compatibility tooling, are excluded. Authored capture scripts under `assets/captures`, including `default_battle.ron`, are retained as runtime assets. The only permitted symlink is the existing `crates/client/assets -> ../../assets`, whose target must resolve inside the snapshot. A new root Cargo configuration or unexpected symlink stops preservation for review. Current crate files comprise Rust and Cargo manifests; current assets comprise RON, WGSL and actual PNG/JPEG/MP4 media. The root Cargo patch points to `third_party/naga`; all compile-time include paths remain inside these roots.

From `/home/cl/projects/space-scuffle`, execute this exact preservation block after the gate. It never overwrites a prior snapshot, copies file contents rather than hard-linking live sources, rechecks the live inventory after copying and verifies the independent destination inventory. The gate result file should be retained beside the resulting manifest.

```sh
python - <<'PY'
import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

source = Path('/home/cl/projects/space-scuffle')
snapshot = source / 'tmp/verification/mist-storage-r8-gated'
base_manifest = source / 'tmp/verification/mist-baseline-r6b-20261028/snapshot-manifest.json'
root_files = ['AGENTS.md', 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml']
roots = ['crates', 'assets', 'third_party']
excluded = {'target', 'tmp', '.git', '.cargo', '.agents', '.codex', '.aws', '__pycache__'}
allowed_links = {'crates/client/assets': '../../assets'}
if (source / '.cargo').exists():
    raise RuntimeError('New root Cargo configuration requires explicit inventory review')

def inventory(root):
    paths = [root / name for name in root_files]
    for tree in roots:
        for directory, dirs, files in os.walk(root / tree, followlinks=False):
            for name in list(dirs):
                path = Path(directory) / name
                if path.is_symlink():
                    paths.append(path)
                    dirs.remove(name)
                elif name in excluded:
                    dirs.remove(name)
            paths.extend(Path(directory) / name for name in files)
    result = {}
    for path in sorted(paths):
        relative = path.relative_to(root).as_posix()
        if path.is_symlink():
            link = os.readlink(path)
            if allowed_links.get(relative) != link:
                raise RuntimeError(f'Unreviewed symlink: {relative} -> {link}')
            path.resolve(strict=True).relative_to(root.resolve())
            result[relative] = {'type': 'relative_symlink', 'target': link,
                                'bytes': len(link.encode()),
                                'sha256': hashlib.sha256(link.encode()).hexdigest()}
        elif path.is_file():
            data = path.read_bytes()
            result[relative] = {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
        else:
            raise RuntimeError(f'Missing or unsupported input: {path}')
    return result

before = inventory(source)
snapshot.mkdir(parents=True, exist_ok=False)
for relative, entry in before.items():
    destination = snapshot / relative
    destination.parent.mkdir(parents=True, exist_ok=True)
    if entry.get('type') == 'relative_symlink':
        destination.symlink_to(entry['target'])
    else:
        shutil.copy2(source / relative, destination)
if inventory(source) != before or inventory(snapshot) != before:
    raise RuntimeError('Source changed or destination verification failed; snapshot is not accepted')
encoded = json.dumps(before, sort_keys=True, separators=(',', ':')).encode()
old = json.loads(base_manifest.read_text())
difference = {name: {'baseline': old['files'].get(name), 'current': before.get(name)}
              for name in sorted(set(old['files']) | set(before))
              if old['files'].get(name) != before.get(name)}
manifest = {
    'created_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'source_root': str(source), 'snapshot_root': str(snapshot),
    'inventory_sha256': hashlib.sha256(encoded).hexdigest(), 'files': before,
    'baseline_inventory_sha256': old['inventory_sha256'],
    'baseline_difference': difference,
    'rustc': subprocess.check_output(['rustc', '-Vv'], text=True).strip(),
    'cargo': subprocess.check_output(['cargo', '-V'], text=True).strip(),
    'build': 'cargo build --release -p client --bin mist_preview --locked --offline',
    'asset_root': str(snapshot / 'assets'),
}
(snapshot / 'snapshot-manifest.json').write_text(json.dumps(manifest, indent=2, sort_keys=True) + '\n')
print(json.dumps({'snapshot': str(snapshot), 'files': len(before),
                  'inventory_sha256': manifest['inventory_sha256'],
                  'baseline_changed_entries': len(difference)}, indent=2))
PY
```

Root should independently rehash the destination before releasing the live source lanes. The preserved snapshot remains unchanged thereafter. A future owner-reset verification fixture is deliberately not part of this gated performance slice; it cannot be used to claim that fixture was included in these measurements.

## Isolated build and artifact identity

The existing isolated baseline target directory is only a compilation cache; the baseline executable is already copied and hashed separately. The renderer dependency cache can be reused, but preserved source mtimes exposed stale workspace-crate reuse when switching snapshot roots. Before each switch, invalidate the five workspace packages in this isolated release cache and require the log to compile each from the intended snapshot. This retains external renderer dependencies while preventing an old simulation rlib from being linked against a newer client. Source hashes alone do not establish which rlib Cargo actually selected. Never use the shared live workspace `target` for this build. Build only after the approved pre50 quiet interval has ended; do not benchmark concurrently with any build or profiler.

```sh
cd /home/cl/projects/space-scuffle/tmp/verification/mist-storage-r8-gated
env CARGO_TARGET_DIR=/home/cl/projects/space-scuffle/tmp/verification/mist-baseline-r6-target cargo clean --release -p sim_types -p sim_map -p sim_core -p asset_baker -p client
env CARGO_TARGET_DIR=/home/cl/projects/space-scuffle/tmp/verification/mist-baseline-r6-target cargo build --release -p client --bin mist_preview --locked --offline > /tmp/space-scuffle-r8-mist-storage-release-build.log 2>&1
```

After exit0, use a fresh `tmp/verification/mist-storage-r8-artifacts` directory. Copy `mist-baseline-r6-target/release/mist_preview` to `mist-storage-r8-artifacts/mist_preview-storage`; record its SHA256, byte length, source inventory hash, exact build command, build exit/log and compiler versions in `build-evidence.json`. Rehash every manifest file/symlink after building and again before launching. Fail on missing/changed source or media. Store this record outside the immutable source inventory. A hash alone is insufficient if the referenced source inventory did not pass both checks.

The following artifact step verifies the inventoried inputs and creates the named fresh artifact directory only after the build has completed successfully:

```sh
python - <<'PYARTIFACT'
import hashlib
import json
import os
from pathlib import Path
import shutil

root = Path('/home/cl/projects/space-scuffle/tmp/verification')
snapshot = root / 'mist-storage-r8-gated'
manifest = json.loads((snapshot / 'snapshot-manifest.json').read_text())
for name, expected in manifest['files'].items():
    path = snapshot / name
    if expected.get('type') == 'relative_symlink':
        link = os.readlink(path)
        actual = {'type': 'relative_symlink', 'target': link,
                  'bytes': len(link.encode()),
                  'sha256': hashlib.sha256(link.encode()).hexdigest()}
        path.resolve(strict=True).relative_to(snapshot.resolve())
    else:
        data = path.read_bytes()
        actual = {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
    if actual != expected:
        raise RuntimeError(f'Preserved input changed: {name}')
artifacts = root / 'mist-storage-r8-artifacts'
artifacts.mkdir(exist_ok=False)
binary = artifacts / 'mist_preview-storage'
shutil.copy2(root / 'mist-baseline-r6-target/release/mist_preview', binary)
data = binary.read_bytes()
evidence = {
    'binary': str(binary), 'sha256': hashlib.sha256(data).hexdigest(), 'bytes': len(data),
    'snapshot': str(snapshot), 'source_inventory_sha256': manifest['inventory_sha256'],
    'build': manifest['build'], 'build_exit': 0,
    'build_log': '/tmp/space-scuffle-r8-mist-storage-release-build.log',
    'rustc': manifest['rustc'], 'cargo': manifest['cargo'],
    'snapshot_verified_after_build': True,
    'asset_root': str(snapshot / 'assets'),
}
(artifacts / 'build-evidence.json').write_text(json.dumps(evidence, indent=2, sort_keys=True) + '\n')
print(json.dumps(evidence, indent=2))
PYARTIFACT
```

## Matched native measurements

Run from the preserved snapshot directory with explicit `SPACE_SCUFFLE_ASSETS` and the preserved executable. Clear `SPACE_SCUFFLE_TRACE_MIST_ORDER`, `VK_ICD_FILENAMES`, `WGPU_ADAPTER_NAME` and `LIBGL_ALWAYS_SOFTWARE`; the actual report must still identify the intended AMD Vulkan adapter. Do not suppress logs or disable renderer validation. A representative command is:

```sh
env -u WAYLAND_DISPLAY -u SPACE_SCUFFLE_TRACE_MIST_ORDER -u VK_ICD_FILENAMES -u WGPU_ADAPTER_NAME -u LIBGL_ALWAYS_SOFTWARE WINIT_UNIX_BACKEND=x11 WGPU_BACKEND=vulkan SPACE_SCUFFLE_ASSETS=/home/cl/projects/space-scuffle/tmp/verification/mist-storage-r8-gated/assets /home/cl/projects/space-scuffle/tmp/verification/mist-storage-r8-artifacts/mist_preview-storage --case crowd --fps 60 --msaa 4 --benchmark-emitters 0 --benchmark-seconds 10 --out /tmp/space-scuffle-r8-mist-storage-release-0 > /tmp/space-scuffle-r8-mist-storage-release-0.log 2>&1
```

Use fresh destinations and sequential emitter counts `0,100,400,400,100,0`, distinguishing reverse repeats in their names. Coordinate an exclusive measurement interval with the other lanes and record unrelated machine load without touching its processes. Keep all 400 real actors, actual media identities, mature-history/wall warmup, 1280×720 physical resolution, MSAA4, non-HDR camera and configured AutoNoVsync policy. This policy does not prove the backend-resolved swapchain mode. Retain exit status, full log, all three PNG/CSV checkpoints, metrics and `benchmark.ron` for every case; any incomplete artifact or typed failure invalidates that case.

Compare actual admitted media hashes, camera/frustum bounds, trajectories, settings, sample counts and load against the preserved six dynamic baseline reports before interpreting cost. Report main update plus all four transfer CPU stages, GPU pass distributions, actual wall times, submitted bytes, stalls, mesh Modified/extraction counts and application-owned buffer counters separately. No per-frame CPU geometry work does not mean zero CPU transfer cost. Do not subtract unpaired percentile values or mistake virtual60 Hz for measured throughput. The prior zero-emitter baseline already failed the16.67ms wall p95 target; no isolated storage improvement erases that whole-scene limitation.

The known stale-cache incident affected the reset derivative test attempt: its log compiled only the new client and then reported missing `SimWorld::write_render_snapshot`, citing an old source line, despite the preserved reset source defining the API. `/tmp/space-scuffle-r8-reset-isolated-tests.log` is retained as failed verification. In contrast, the earlier preserved gated storage release log explicitly compiled all five workspace crates from its intended snapshot; that evidence distinguishes it from the failing later reuse. Rebuild the reset derivative after workspace-package invalidation, without mutating the snapshot or any preserved executable.
