//! Resolves actual player ownership into one authored presentation palette.
//! Colors are validated and converted once at load; steady-state consumers copy
//! prepared styles. Preview roles and live participant membership remain separate
//! from identity hue, which never determines gameplay hostility or authorization.


use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_egui::egui;
use serde::Deserialize;
use sim_types::PlayerId;

/// A prepared ownership style: sRGB for UI and linear RGB for rendered materials.
#[derive(Clone, Copy, Debug)]
pub struct PlayerStyle {
    /// Authored sRGB channels, each in [0,1]; opacity is not an identity component.
    pub srgb: [f32; 3],
    /// Linear RGB with alpha1; effect energy and packed metadata remain separate.
    pub linear: Vec4,
    /// Quantized authored sRGB for egui labels, converted once at load.
    pub ui: egui::Color32,
}

impl PlayerStyle {
    fn prepare(rgb: (f32, f32, f32)) -> Result<Self, PresentationError> {
        let (r, g, b) = rgb;
        if [r, g, b]
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err(PresentationError::Color);
        }
        let linear = Color::srgb(r, g, b).to_linear();
        Ok(Self {
            srgb: [r, g, b],
            linear: Vec4::new(linear.red, linear.green, linear.blue, 1.0),
            ui: egui::Color32::from_rgb(
                (r * 255.0).round() as u8,
                (g * 255.0).round() as u8,
                (b * 255.0).round() as u8,
            ),
        })
    }

    /// Returns linear material color without performing a second sRGB conversion.
    #[must_use]
    pub fn color(self) -> Color {
        Color::linear_rgba(self.linear.x, self.linear.y, self.linear.z, 1.0)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ShowcaseOverride {
    index: usize,
    owner: u8,
}

#[derive(Debug, Clone, Deserialize)]
struct PreviewRoles {
    primary: u8,
    secondary: u8,
    effect: u8,
    showcase_overrides: Vec<ShowcaseOverride>,
}

#[derive(Deserialize)]
struct PaletteFile {
    player_colors_srgb: Vec<(f32, f32, f32)>,
    neutral_color_srgb: (f32, f32, f32),
    preview_owners: PreviewRoles,
}

/// Validated palette and explicit preview roles from the actual map visual catalog.
/// A replacement is prepared before commit and published with its scene generation.
#[derive(Resource, Clone, Debug)]
pub struct PlayerPalette {
    styles: Vec<PlayerStyle>,
    neutral: PlayerStyle,
    preview: PreviewRoles,
    preview_members: [bool; 8],
}

/// Explicit palette, owner-domain or committed participant mismatch.
#[derive(Debug, thiserror::Error)]
pub enum PresentationError {
    /// The actual authored palette could not be opened.
    #[error("cannot read player presentation catalog {path}: {source}")]
    Io {
        /// Exact attempted runtime path.
        path: PathBuf,
        /// Original I/O failure.
        #[source]
        source: std::io::Error,
    },
    /// Authored schema does not contain the required presentation data.
    #[error("invalid player presentation catalog {path}: {source}")]
    Parse {
        /// Exact attempted runtime path.
        path: PathBuf,
        /// Original parse failure.
        #[source]
        source: Box<ron::error::SpannedError>,
    },
    /// The supported simulation owner domain must have exactly eight authored entries.
    #[error("player palette must contain exactly eight colors, found {0}")]
    Slots(usize),
    /// Color values cannot be clamped or silently substituted.
    #[error("ownership colors must be finite sRGB channels in [0,1]")]
    Color,
    /// An unsupported identity cannot become a neutral or nearby valid slot.
    #[error("unsupported presentation owner {0}")]
    Owner(PlayerId),
    /// A valid palette identity is absent from the relevant participant registry.
    #[error("presentation owner {owner} is absent from {context}")]
    Participant {
        /// Captured actual owner identity.
        owner: PlayerId,
        /// Live or preview context that rejected this identity.
        context: &'static str,
    },
    /// Preview roles must resolve actual supported non-neutral owners.
    #[error("preview role references unsupported player slot {0}")]
    PreviewOwner(u8),
    /// Showcase overrides address the actual sorted catalog list using zero-based indices.
    #[error("showcase owner override index {index} is duplicate or outside {count} entries")]
    Showcase {
        /// Invalid zero-based index.
        index: usize,
        /// Actual number of showcase catalog entries.
        count: usize,
    },
}

impl PlayerPalette {
    /// Loads the canonical actual catalog beneath `assets_root` and validates
    /// zero-based preview overrides against the real catalog showcase count.
    /// # Errors
    /// Rejects missing/invalid files, colors, roles, duplicate and out-of-range overrides.
    pub fn load(assets_root: &Path, showcase_count: usize) -> Result<Self, PresentationError> {
        let path = assets_root.join("catalog/map_entity_visuals.ron");
        let text = std::fs::read_to_string(&path).map_err(|source| PresentationError::Io {
            path: path.clone(),
            source,
        })?;
        let file: PaletteFile =
            ron::from_str(&text).map_err(|source| PresentationError::Parse {
                path,
                source: Box::new(source),
            })?;
        Self::prepare(file, showcase_count)
    }

    fn prepare(mut file: PaletteFile, count: usize) -> Result<Self, PresentationError> {
        if file.player_colors_srgb.len() != 8 {
            return Err(PresentationError::Slots(file.player_colors_srgb.len()));
        }
        let styles = file
            .player_colors_srgb
            .into_iter()
            .map(PlayerStyle::prepare)
            .collect::<Result<Vec<_>, _>>()?;
        let neutral = PlayerStyle::prepare(file.neutral_color_srgb)?;
        let mut preview_members = [false; 8];
        for owner in [
            file.preview_owners.primary,
            file.preview_owners.secondary,
            file.preview_owners.effect,
        ]
        .into_iter()
        .chain(
            file.preview_owners
                .showcase_overrides
                .iter()
                .map(|v| v.owner),
        ) {
            if usize::from(owner) >= styles.len() {
                return Err(PresentationError::PreviewOwner(owner));
            }
            preview_members[usize::from(owner)] = true;
        }
        file.preview_owners
            .showcase_overrides
            .sort_by_key(|v| v.index);
        let mut previous = None;
        for entry in &file.preview_owners.showcase_overrides {
            if entry.index >= count || previous == Some(entry.index) {
                return Err(PresentationError::Showcase {
                    index: entry.index,
                    count,
                });
            }
            previous = Some(entry.index);
        }
        Ok(Self {
            styles,
            neutral,
            preview: file.preview_owners,
            preview_members,
        })
    }

    /// Resolves an actual supported owner or explicit neutral identity without fallback.
    /// # Errors
    /// Rejects IDs outside the supported eight player slots and neutral sentinel.
    pub fn resolve(&self, owner: PlayerId) -> Result<PlayerStyle, PresentationError> {
        if owner == PlayerId::NEUTRAL {
            return Ok(self.neutral);
        }
        self.styles
            .get(usize::from(owner.0))
            .copied()
            .ok_or(PresentationError::Owner(owner))
    }

    /// Resolves a live owner against the successfully committed participant list.
    /// # Errors
    /// Rejects supported palette entries absent from the actual battle; neutral is explicit.
    pub fn battle_style(
        &self,
        owner: PlayerId,
        metadata: &crate::battle_pipeline::session::CommittedBattleMetadata,
    ) -> Result<PlayerStyle, PresentationError> {
        let style = self.resolve(owner)?;
        if owner == PlayerId::NEUTRAL
            || metadata
                .players
                .binary_search_by_key(&owner, |p| p.owner)
                .is_ok()
        {
            return Ok(style);
        }
        Err(PresentationError::Participant {
            owner,
            context: "committed battle participants",
        })
    }

    /// Resolves the intended primary preview role from authored data.
    #[must_use]
    pub fn primary(&self) -> PlayerId {
        PlayerId(self.preview.primary)
    }

    /// Resolves the intended secondary preview role from authored data.
    #[must_use]
    pub fn secondary(&self) -> PlayerId {
        PlayerId(self.preview.secondary)
    }

    /// Resolves the intended effect-demo owner from authored data.
    #[must_use]
    pub fn effect(&self) -> PlayerId {
        PlayerId(self.preview.effect)
    }

    /// Resolves a zero-based showcase slot; non-overridden slots intentionally use primary.
    #[must_use]
    pub fn showcase(&self, index: usize) -> PlayerId {
        self.preview
            .showcase_overrides
            .binary_search_by_key(&index, |v| v.index)
            .map_or(self.primary(), |slot| {
                PlayerId(self.preview.showcase_overrides[slot].owner)
            })
    }

    /// Validates event/actor ownership against explicit preview roles, not a live battle.
    /// # Errors
    /// Rejects otherwise valid palette owners absent from authored preview roles.
    pub fn preview_style(&self, owner: PlayerId) -> Result<PlayerStyle, PresentationError> {
        let style = self.resolve(owner)?;
        if owner == PlayerId::NEUTRAL || self.preview_members[usize::from(owner.0)] {
            return Ok(style);
        }
        Err(PresentationError::Participant {
            owner,
            context: "preview roles",
        })
    }
}

/// Persistent typed failure surfaced to the ordinary UI and capture exit path.
#[derive(Resource, Default)]
pub struct PresentationStatus {
    failure: Option<PresentationError>,
    text: String,
}

impl PresentationStatus {
    /// Returns the cached readable failure; successful frames allocate nothing.
    #[must_use]
    pub fn failure_text(&self) -> Option<&str> {
        self.failure.as_ref().map(|_| self.text.as_str())
    }

    /// Retains the first exact failure and emits a nonzero exit for real verification.
    pub fn reject(&mut self, error: PresentationError, exit: &mut EventWriter<'_, AppExit>) {
        if self.failure.is_none() {
            self.text = error.to_string();
            error!("Player presentation failed: {error}");
            self.failure = Some(error);
            exit.send(AppExit::error());
        }
    }
}

/// Groups palette lookup and typed failure storage for scene synchronization.
/// The existing caller-owned exit writer is shared with other admission failures.
#[derive(bevy::ecs::system::SystemParam)]
pub struct PresentationContext<'w> {
    /// Current committed palette, prepared before spawning any presented entities.
    pub palette: Res<'w, PlayerPalette>,
    status: ResMut<'w, PresentationStatus>,
}

impl PresentationContext<'_> {
    /// Records a typed failure and preserves the nonzero application exit contract.
    pub fn reject(&mut self, error: PresentationError, exit: &mut EventWriter<'_, AppExit>) {
        self.status.reject(error, exit);
    }
}

#[cfg(test)]
mod tests;
