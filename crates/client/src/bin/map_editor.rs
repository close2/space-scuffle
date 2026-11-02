//! Space Scuffle standalone map editor binary.
//!
//! # Architectural Responsibilities
//! In accordance with [Plan 21 §7.1](file:///home/cl/projects/space-scuffle/docs/plans/21-plan-map-editor.md),
//! this binary boots the dedicated windowed Bevy application (1600×900) for interactive map authoring,
//! holographic asset inspection, and deterministic playtest battles.
//!
//! # Command-Line Interface
//! - `--map <path>`: Load the specified `.scmap.ron` map document on startup.
//! - `--new <W>x<H>`: Initialize a new blank map with grid dimensions `W × H`.
//! - `--verify-render <out_dir>`: Automated render verification mode capturing reference screenshots.
//! - `--max-frames <n>`: Cap execution to `n` frames before clean shutdown (used in headless testing).

use std::path::PathBuf;
use std::process::ExitCode;

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use clap::Parser;
use client::editor::{
    editor_window_plugin, EditorStartup, InitialDocument, MapEditorPlugin, VerifyOutputDir,
};

/// CLI arguments for the `map_editor` binary.
#[derive(Parser, Debug)]
#[command(
    name = "map_editor",
    about = "Space Scuffle Map Editor & Scenario Authoring Suite",
    version
)]
struct CliArgs {
    /// File path to an existing .scmap.ron map to open on startup.
    #[arg(long)]
    map: Option<PathBuf>,

    /// Initialize a new blank map with dimensions formatted as `<W>x<H>` (e.g. `64x64`).
    #[arg(long, value_parser = parse_dimensions)]
    new: Option<(u32, u32)>,

    /// Run automated render verification capturing reference poses into `<out_dir>`.
    #[arg(long)]
    verify_render: Option<PathBuf>,

    /// Limit application execution to a maximum number of frames.
    #[arg(long)]
    max_frames: Option<u64>,
}

fn parse_dimensions(s: &str) -> Result<(u32, u32), String> {
    let parts: Vec<&str> = s.split('x').collect();
    if parts.len() != 2 {
        return Err("Dimensions must be formatted as <W>x<H> (e.g. 64x64)".to_string());
    }
    let w = parts[0]
        .parse::<u32>()
        .map_err(|e| format!("Invalid width: {e}"))?;
    let h = parts[1]
        .parse::<u32>()
        .map_err(|e| format!("Invalid height: {e}"))?;
    Ok((w, h))
}

fn main() -> ExitCode {
    let args = CliArgs::parse();
    let initial = match (args.map, args.new) {
        (Some(path), _) => InitialDocument::Open(path),
        (None, Some((width, height))) => InitialDocument::Blank { width, height },
        (None, None) => InitialDocument::Default,
    };
    let startup = match EditorStartup::load(&initial) {
        Ok(startup) => startup,
        Err(error) => {
            eprintln!("map_editor: {error}");
            return ExitCode::FAILURE;
        }
    };
    let verify_render = match args.verify_render.map(VerifyOutputDir::prepare).transpose() {
        Ok(verify_render) => verify_render,
        Err(error) => {
            eprintln!("map_editor: {error}");
            return ExitCode::FAILURE;
        }
    };
    let asset_path = startup.assets_root.as_path().to_string_lossy().to_string();

    let mut app = App::new();

    app.add_plugins(
        DefaultPlugins
            .set(AssetPlugin {
                file_path: asset_path,
                ..default()
            })
            .set(editor_window_plugin()),
    );

    app.add_plugins(EguiPlugin);

    app.add_plugins(MapEditorPlugin {
        startup,
        verify_render,
        max_frames: args.max_frames,
    });

    match app.run() {
        AppExit::Success => ExitCode::SUCCESS,
        AppExit::Error(code) => ExitCode::from(code.get()),
    }
}
