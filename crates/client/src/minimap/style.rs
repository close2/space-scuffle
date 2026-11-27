//! The minimap's presentation data, `assets/catalog/minimap.ron` (plan `2026-12-01-minimap` D-9).
//!
//! # Responsibility
//! Reads and validates the file at run time from the runtime assets root, by the same pattern as
//! the player palette (`player_presentation`): one typed error naming the file and, for a bad
//! value, the field. The file holds presentation only (sizes, colours, levels); it names no unit,
//! structure or faction, and the simulation never reads it.
//!
//! # Units
//! - Colours are authored as sRGB channels in `[0, 1]` and kept both as authored and as linear
//!   RGBA, the form the shader blends in.
//! - `side_points` is in egui points; pixel sizes (`min_dot_px`, `border_px`) are physical pixels
//!   of the minimap image.
//!
//! # Invariants
//! - A loaded [`MinimapStyle`] passed every check of [`MinimapStyle::from_file`]: every colour is
//!   finite and in range, `level_brightness` has one entry per cliff level, sizes are positive.
//! - Nothing is substituted for a bad value: loading fails and says which field is wrong.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde::Deserialize;
use sim_map::CliffLevel;

/// Path of the file beneath the runtime assets root.
pub const MINIMAP_STYLE_PATH: &str = "catalog/minimap.ron";

/// The only `format_version` this loader reads.
pub const MINIMAP_STYLE_FORMAT: u32 = 1;

/// Number of cliff levels, `0..=CliffLevel::MAX`: the length `level_brightness` must have.
pub const CLIFF_LEVELS: usize = CliffLevel::MAX.0 as usize + 1;

/// Why the minimap's style could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum MinimapStyleError {
    /// The file could not be read.
    #[error("cannot read minimap style {path}: {source}")]
    Io {
        /// The path that was tried.
        path: PathBuf,
        /// The I/O failure.
        #[source]
        source: std::io::Error,
    },
    /// The file is not valid RON for the schema.
    #[error("invalid minimap style {path}: {source}")]
    Parse {
        /// The path that was read.
        path: PathBuf,
        /// The parse failure, with its position.
        #[source]
        source: Box<ron::error::SpannedError>,
    },
    /// A field parsed but holds a value the minimap cannot use.
    #[error("invalid minimap style {path}: field `{field}` {reason}")]
    Value {
        /// The path that was read.
        path: PathBuf,
        /// The field at fault, as written in the file.
        field: &'static str,
        /// What is wrong with it.
        reason: &'static str,
    },
}

/// One authored colour: sRGB channels in `[0, 1]`.
type SrgbTriple = (f32, f32, f32);

/// The file as authored.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct StyleFile {
    format_version: u32,
    side_points: f32,
    background: SrgbTriple,
    level_brightness: Vec<f32>,
    unpathable_color: SrgbTriple,
    edge_mix: f32,
    fog_darkening: FogFile,
    min_dot_px: f32,
    border_px: f32,
    border_color: SrgbTriple,
    resource_color: SrgbTriple,
    watchtower_color: SrgbTriple,
    watchtower_half: f32,
    team_colors: TeamFile,
}

/// `fog_darkening` as authored.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
struct FogFile {
    visible: f32,
    fogged: f32,
    unexplored: f32,
}

/// `team_colors` as authored (`core` `GameUIData:561–590` `OverrideColors`).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
struct TeamFile {
    #[serde(rename = "self")]
    own: SrgbTriple,
    ally: SrgbTriple,
    hostile: SrgbTriple,
    neutral: SrgbTriple,
    selected: SrgbTriple,
}

/// How far each visibility state darkens the terrain toward the background, in `[0, 1]`
/// (`StarCraft` II's `MinimapTerrain.fx` lerps to black by the fog's level).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FogDarkening {
    /// A cell the viewer sees now.
    pub visible: f32,
    /// A cell the viewer saw before.
    pub fogged: f32,
    /// A cell the viewer never saw.
    pub unexplored: f32,
}

/// `StarCraft` II's team colours (`OverrideColors`), used in team colour mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TeamColors {
    /// The viewer's own units (`SelfMinimap`).
    pub own: MinimapColor,
    /// Allied players' units (unused while the game has no teams).
    pub ally: MinimapColor,
    /// Every other player's units.
    pub hostile: MinimapColor,
    /// Neutral units.
    pub neutral: MinimapColor,
    /// The viewer's selected units.
    pub selected: MinimapColor,
}

/// One validated colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinimapColor {
    /// The authored sRGB channels, each in `[0, 1]`.
    pub srgb: [f32; 3],
    /// Linear RGB with alpha 1, the form the shader blends in.
    pub linear: [f32; 4],
}

impl MinimapColor {
    /// Validates the authored triple of `field`.
    fn prepare(field: &'static str, rgb: SrgbTriple) -> Result<Self, (&'static str, &'static str)> {
        let (r, g, b) = rgb;
        if [r, g, b]
            .iter()
            .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(channel))
        {
            return Err((field, "must hold finite sRGB channels in [0, 1]"));
        }
        let linear = Color::srgb(r, g, b).to_linear();
        Ok(Self {
            srgb: [r, g, b],
            linear: [linear.red, linear.green, linear.blue, 1.0],
        })
    }
}

/// The validated presentation values of the minimap.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct MinimapStyle {
    /// Side of the minimap square in the command panel, in egui points.
    pub side_points: f32,
    /// Colour outside the map and the colour fog darkens toward (`core` `GameUIData:694`).
    pub background: MinimapColor,
    /// Brightness factor of each cliff level, lowest first; ramps interpolate.
    pub level_brightness: [f32; CLIFF_LEVELS],
    /// Colour of ground no ground unit may walk.
    pub unpathable: MinimapColor,
    /// How far a cliff edge's texel is mixed toward the theme's cliff colour, in `[0, 1]`.
    pub edge_mix: f32,
    /// How far fog darkens the terrain, per visibility state.
    pub fog_darkening: FogDarkening,
    /// Smallest side of a dot, in image pixels (`MinUnitDotSize`, `core` `GameUIData:696`).
    pub min_dot_px: f32,
    /// Border width of a dot, in image pixels (`BorderSize`, `:693`).
    pub border_px: f32,
    /// Border colour of a dot (`UnitBorderColor`, `:691`).
    pub border: MinimapColor,
    /// Colour of resources (`ResourceUnitColor`, `:688`).
    pub resource: MinimapColor,
    /// Colour of watchtowers.
    pub watchtower: MinimapColor,
    /// Half-width of a watchtower's square, in world units.
    pub watchtower_half: f32,
    /// The team colours.
    pub team: TeamColors,
}

impl MinimapStyle {
    /// Loads and validates [`MINIMAP_STYLE_PATH`] beneath `assets_root`.
    ///
    /// # Errors
    /// [`MinimapStyleError::Io`] when the file cannot be read, [`MinimapStyleError::Parse`] when
    /// it does not parse, [`MinimapStyleError::Value`] naming the first field that is out of range.
    pub fn load(assets_root: &Path) -> Result<Self, MinimapStyleError> {
        let path = assets_root.join(MINIMAP_STYLE_PATH);
        let text = std::fs::read_to_string(&path).map_err(|source| MinimapStyleError::Io {
            path: path.clone(),
            source,
        })?;
        Self::from_text(&path, &text)
    }

    /// Parses and validates `text`, the contents of the file at `path` (named in errors only).
    ///
    /// # Errors
    /// As [`Self::load`], except that nothing is read from disk.
    pub fn from_text(path: &Path, text: &str) -> Result<Self, MinimapStyleError> {
        let file: StyleFile = ron::from_str(text).map_err(|source| MinimapStyleError::Parse {
            path: path.to_path_buf(),
            source: Box::new(source),
        })?;
        Self::from_file(&file).map_err(|(field, reason)| MinimapStyleError::Value {
            path: path.to_path_buf(),
            field,
            reason,
        })
    }

    /// Validates every field of `file`, in file order.
    fn from_file(file: &StyleFile) -> Result<Self, (&'static str, &'static str)> {
        if file.format_version != MINIMAP_STYLE_FORMAT {
            return Err(("format_version", "must be 1"));
        }
        if !file.side_points.is_finite() || file.side_points <= 0.0 {
            return Err(("side_points", "must be positive and finite"));
        }
        let background = MinimapColor::prepare("background", file.background)?;
        let level_brightness: [f32; CLIFF_LEVELS] =
            file.level_brightness.as_slice().try_into().map_err(|_| {
                (
                    "level_brightness",
                    "must have one entry per cliff level (4)",
                )
            })?;
        if level_brightness
            .iter()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
        {
            return Err(("level_brightness", "entries must be finite and in [0, 1]"));
        }
        let unpathable = MinimapColor::prepare("unpathable_color", file.unpathable_color)?;
        if !unit_interval(file.edge_mix) {
            return Err(("edge_mix", "must be finite and in [0, 1]"));
        }
        let fog = file.fog_darkening;
        if ![fog.visible, fog.fogged, fog.unexplored]
            .into_iter()
            .all(unit_interval)
        {
            return Err(("fog_darkening", "levels must be finite and in [0, 1]"));
        }
        if !file.min_dot_px.is_finite() || file.min_dot_px <= 0.0 {
            return Err(("min_dot_px", "must be positive and finite"));
        }
        if !file.border_px.is_finite() || file.border_px < 0.0 {
            return Err(("border_px", "must be finite and not negative"));
        }
        if !file.watchtower_half.is_finite() || file.watchtower_half <= 0.0 {
            return Err(("watchtower_half", "must be positive and finite"));
        }
        let team = file.team_colors;
        Ok(Self {
            side_points: file.side_points,
            background,
            level_brightness,
            unpathable,
            edge_mix: file.edge_mix,
            fog_darkening: FogDarkening {
                visible: fog.visible,
                fogged: fog.fogged,
                unexplored: fog.unexplored,
            },
            min_dot_px: file.min_dot_px,
            border_px: file.border_px,
            border: MinimapColor::prepare("border_color", file.border_color)?,
            resource: MinimapColor::prepare("resource_color", file.resource_color)?,
            watchtower: MinimapColor::prepare("watchtower_color", file.watchtower_color)?,
            watchtower_half: file.watchtower_half,
            team: TeamColors {
                own: MinimapColor::prepare("team_colors", team.own)?,
                ally: MinimapColor::prepare("team_colors", team.ally)?,
                hostile: MinimapColor::prepare("team_colors", team.hostile)?,
                neutral: MinimapColor::prepare("team_colors", team.neutral)?,
                selected: MinimapColor::prepare("team_colors", team.selected)?,
            },
        })
    }
}

/// Whether `value` is finite and in `[0, 1]`.
fn unit_interval(value: f32) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

/// The banner's text when the minimap's pipeline failed; the log holds the renderer's message.
const RENDER_FAILURE: &str =
    "the minimap shader (shaders/minimap.wgsl) failed to compile; see the log";

/// Whether the minimap's style loaded, and why not.
///
/// A failure is kept as text so the HUD's failure banner can show it with no formatting per
/// frame; it also ends the app with an error, as a bad palette does (charter §1.2: no silent
/// fallback).
#[derive(Resource, Debug, Default)]
pub struct MinimapStyleStatus {
    failure: Option<String>,
}

impl MinimapStyleStatus {
    /// The failure's text, if loading failed.
    #[must_use]
    pub fn failure_text(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    /// Records `error`, once.
    pub fn reject(&mut self, error: &MinimapStyleError) {
        if self.failure.is_none() {
            self.failure = Some(error.to_string());
        }
    }

    /// Records that the minimap's shader failed in the renderer, which logged the cause, once.
    pub fn reject_render(&mut self) {
        if self.failure.is_none() {
            self.failure = Some(RENDER_FAILURE.to_owned());
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed test is a panic"
)]
mod tests {
    use super::*;

    const GOOD: &str = r"(
        format_version: 1,
        side_points: 172.0,
        background: (0.0, 0.0, 0.0),
        level_brightness: [0.55, 0.70, 0.85, 1.0],
        unpathable_color: (0.06, 0.06, 0.08),
        edge_mix: 0.6,
        fog_darkening: (visible: 0.0, fogged: 0.45, unexplored: 0.85),
        min_dot_px: 3.0,
        border_px: 1.0,
        border_color: (0.0, 0.0, 0.0),
        resource_color: (0.494, 0.749, 0.945),
        watchtower_color: (0.9, 0.9, 0.9),
        watchtower_half: 1.0,
        team_colors: (self: (0.0, 0.7333, 0.0), ally: (1.0, 1.0, 0.0), hostile: (1.0, 0.0, 0.0),
                      neutral: (0.4941, 0.749, 0.945), selected: (0.0, 1.0, 0.0)),
    )";

    fn parse(text: &str) -> Result<MinimapStyle, MinimapStyleError> {
        MinimapStyle::from_text(Path::new("catalog/minimap.ron"), text)
    }

    fn field_of(text: &str) -> &'static str {
        match parse(text) {
            Err(MinimapStyleError::Value { field, .. }) => field,
            other => panic!("expected a value error, got {other:?}"),
        }
    }

    #[test]
    fn a_good_file_loads() {
        let style = parse(GOOD).expect("the sample is valid");
        assert_eq!(style.side_points, 172.0);
        assert_eq!(style.background.linear, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(style.level_brightness, [0.55, 0.70, 0.85, 1.0]);
    }

    #[test]
    fn each_bad_value_names_its_field() {
        assert_eq!(
            field_of(&GOOD.replace("(0.0, 0.0, 0.0)", "(0.0, 2.0, 0.0)")),
            "background"
        );
        assert_eq!(
            field_of(&GOOD.replace("[0.55, 0.70, 0.85, 1.0]", "[0.55, 0.70]")),
            "level_brightness"
        );
        assert_eq!(
            field_of(&GOOD.replace("edge_mix: 0.6", "edge_mix: -1.0")),
            "edge_mix"
        );
        assert_eq!(field_of(&GOOD.replace("172.0", "0.0")), "side_points");
        assert_eq!(
            field_of(&GOOD.replace("fogged: 0.45", "fogged: 1.45")),
            "fog_darkening"
        );
        assert_eq!(
            field_of(&GOOD.replace("min_dot_px: 3.0", "min_dot_px: 0.0")),
            "min_dot_px"
        );
        assert_eq!(
            field_of(&GOOD.replace("hostile: (1.0, 0.0, 0.0)", "hostile: (1.0, -0.1, 0.0)")),
            "team_colors"
        );
        assert_eq!(
            field_of(&GOOD.replace("format_version: 1", "format_version: 2")),
            "format_version"
        );
    }

    #[test]
    fn an_unknown_field_does_not_parse() {
        let text = GOOD.replace("edge_mix: 0.6,", "edge_mix: 0.6, unknown_field: 1.0,");
        assert!(matches!(parse(&text), Err(MinimapStyleError::Parse { .. })));
    }
}
