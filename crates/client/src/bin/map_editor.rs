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

#![allow(
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown
)]

use std::path::PathBuf;

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::window::WindowPlugin;
use bevy_egui::EguiPlugin;
use clap::Parser;
use client::editor::MapEditorPlugin;
use client::resolve_assets_dir;

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

fn main() -> AppExit {
    let args = CliArgs::parse();
    if let Some(ref dir) = args.verify_render {
        let _ = std::fs::create_dir_all(dir);
    }
    let assets_dir = resolve_assets_dir();

    let mut app = App::new();

    app.add_plugins(
        DefaultPlugins
            .set(AssetPlugin {
                file_path: assets_dir.to_string_lossy().to_string(),
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Space Scuffle — Map Editor".to_string(),
                    resolution: (1600.0_f32, 900.0_f32).into(),
                    ..default()
                }),
                ..default()
            }),
    );

    app.add_plugins(EguiPlugin);

    app.add_plugins(MapEditorPlugin {
        map_path: args.map,
        new_dimensions: args.new,
        verify_render: args.verify_render,
        max_frames: args.max_frames,
    });

    app.run()
}
