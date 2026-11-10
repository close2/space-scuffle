//! Reproducible GPU captures driven through the normal production client systems, and the
//! effect lab's contact sheets (`--effect-lab <effect> --bind NAME=VALUE ...`).

use bevy::app::AppExit;
use clap::Parser;
use client_diagnostics::effect_lab::{run_effect_lab, EffectLabOptions};
use client_diagnostics::runtime_capture::{run_capture, CaptureOptions};

fn main() -> AppExit {
    let options = CaptureOptions::parse();
    let result = match options.effect_lab.clone() {
        Some(effect) => run_effect_lab(&EffectLabOptions {
            effect,
            bindings: options.bindings.clone(),
            out: options.out.clone(),
            sheet_dir: options.sheet_dir.clone(),
        }),
        None => run_capture(options),
    };
    match result {
        Ok(exit) => exit,
        Err(error) => {
            eprintln!("Battle capture failed: {error}");
            AppExit::error()
        }
    }
}
