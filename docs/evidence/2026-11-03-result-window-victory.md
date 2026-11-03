# Result window on a real display (Phase 5 addendum A3)

`2026-11-03-result-window-victory.jpg` is a grab of a whole X display while the production client showed the
result window of the skirmish. It is not a Bevy screenshot: Bevy's capture path shows egui panels flat, so it
cannot prove what the window looks like.

## What it shows

The production skirmish (`ClientMode::Skirmish`, `assets/scenarios/skirmish.ron`) in its window of 1280 by 720
pixels: the resource bar, the world view, the empty command panel of a finished match, and the centred result
window with `Victory`, the match length, `Play again` and `Exit`. The end was staged: the child of
`battle_result_window_tests.rs` zeroes the enemy structures' health through the world, so the match length reads
0:00. How a real match reaches this window is proven by `skirmish_match_tests.rs`.

## How it was made

1. Build the test binary: `cargo test -p client --test battle_result_window_tests --no-run`.
2. From `crates/client`, run its child on Xvfb and lavapipe (Mesa 25.0.7, LLVM 19.1.7, Debian 13), with the
   environment `tests/support/gpu_run.rs` gives a windowed child, plus two variables:
   - `SPACE_SCUFFLE_SHOW_WINDOW=1` maps the window. Virtual-GPU runs keep it unmapped otherwise, which is why
     the WP8 display grabs came back black.
   - `SPACE_SCUFFLE_RESULT_GRAB=<file>.xwd` makes the child run `xwd -root -silent` three frames after the
     result window's rectangle has stopped moving, and before it presses `Exit`.

   ```sh
   env -u WAYLAND_DISPLAY WINIT_UNIX_BACKEND=x11 WGPU_BACKEND=vulkan SPACE_SCUFFLE_VIRTUAL_GPU=1 \
     VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json WGPU_ADAPTER_NAME=llvmpipe \
     LIBGL_ALWAYS_SOFTWARE=1 SPACE_SCUFFLE_TERRAIN_MAX_ANISOTROPY=1 \
     SPACE_SCUFFLE_RESULT_CHILD_OUT=/path/report.ron \
     SPACE_SCUFFLE_SHOW_WINDOW=1 SPACE_SCUFFLE_RESULT_GRAB=/path/result.xwd \
     xvfb-run -a -s "-screen 0 1280x720x24" \
     <target>/debug/deps/battle_result_window_tests-<hash> --ignored --exact --nocapture windowed_result_child
   ```
3. Convert: `convert result.xwd result.png`, then `convert result.png -strip -quality 82 <name>.jpg` to keep the
   repository small.

The child's own report for this run: verdict `Victory`, window `(513, 237)` to `(767, 331)` inside the world
rectangle `(0, 28)` to `(1280, 540)` in egui points, both buttons inside the window, and the window centred. The
run took 18 s. `SPACE_SCUFFLE_TERRAIN_MAX_ANISOTROPY=1` is the lavapipe workaround of
`allow_for_hex_tiling_driver_defect`, which Mesa 25.0.7 still needs.
