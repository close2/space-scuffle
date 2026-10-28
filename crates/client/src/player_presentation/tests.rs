//! Validates canonical palette loading, color-space conversion and explicit
//! participant/preview admission. Invalid identity cannot acquire a fallback hue.

use super::*;
use crate::battle_pipeline::session::{CommittedBattleMetadata, ParticipantMetadata};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn file() -> Result<PaletteFile, Box<dyn std::error::Error>> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog/map_entity_visuals.ron");
    Ok(ron::from_str(&std::fs::read_to_string(path)?)?)
}

#[test]
fn actual_palette_resolves_sparse_owners_and_neutral_without_fallback() -> TestResult {
    let palette = PlayerPalette::prepare(file()?, 4)?;
    let mut metadata = CommittedBattleMetadata::default();
    for owner in [PlayerId(0), PlayerId(2), PlayerId(7)] {
        let style = palette.resolve(owner)?;
        metadata.players.push(ParticipantMetadata {
            owner,
            label: String::new(),
            color: style.ui,
        });
        assert!(style.linear.is_finite());
    }
    for owner in [PlayerId(0), PlayerId(2), PlayerId(7), PlayerId::NEUTRAL] {
        assert_eq!(
            palette.battle_style(owner, &metadata)?.ui,
            palette.resolve(owner)?.ui
        );
    }
    assert!(matches!(
        palette.battle_style(PlayerId(1), &metadata),
        Err(PresentationError::Participant { .. })
    ));
    for owner in [PlayerId(8), PlayerId(254)] {
        assert!(matches!(palette.resolve(owner), Err(PresentationError::Owner(id)) if id == owner));
        assert!(
            matches!(palette.preview_style(owner), Err(PresentationError::Owner(id)) if id == owner)
        );
    }
    Ok(())
}

#[test]
fn srgb_is_converted_once_and_metadata_alpha_is_not_energy() -> TestResult {
    let style = PlayerStyle::prepare((0.5, 0.0, 1.0))?;
    assert!((style.linear.x - 0.214_041_14).abs() < 0.000_001);
    assert_eq!(style.ui, egui::Color32::from_rgb(128, 0, 255));
    assert_eq!(style.linear.to_array(), [style.linear.x, 0.0, 1.0, 1.0]);
    assert_eq!(
        style.color().to_linear().to_f32_array(),
        style.linear.to_array()
    );
    for invalid in [f32::NAN, f32::INFINITY, -0.01, 1.01] {
        assert!(matches!(
            PlayerStyle::prepare((invalid, 0.0, 0.0)),
            Err(PresentationError::Color)
        ));
    }
    Ok(())
}

#[test]
fn explicit_preview_roles_and_zero_based_overrides_are_validated() -> TestResult {
    let mut authored = file()?;
    authored.preview_owners.primary = 7;
    authored.preview_owners.secondary = 2;
    authored.preview_owners.effect = 0;
    authored.preview_owners.showcase_overrides = vec![ShowcaseOverride { index: 2, owner: 2 }];
    let palette = PlayerPalette::prepare(authored, 5)?;
    assert_eq!(palette.showcase(0), PlayerId(7));
    assert_eq!(palette.showcase(2), PlayerId(2));
    assert_eq!(palette.showcase(4), PlayerId(7));
    for owner in [PlayerId(0), PlayerId(2), PlayerId(7), PlayerId::NEUTRAL] {
        palette.preview_style(owner)?;
    }
    assert!(matches!(
        palette.preview_style(PlayerId(1)),
        Err(PresentationError::Participant { .. })
    ));
    let mut duplicate = file()?;
    duplicate.preview_owners.showcase_overrides = vec![
        ShowcaseOverride { index: 1, owner: 0 },
        ShowcaseOverride { index: 1, owner: 1 },
    ];
    assert!(matches!(
        PlayerPalette::prepare(duplicate, 4),
        Err(PresentationError::Showcase { index: 1, .. })
    ));
    let mut outside = file()?;
    outside.preview_owners.showcase_overrides = vec![ShowcaseOverride { index: 4, owner: 0 }];
    assert!(matches!(
        PlayerPalette::prepare(outside, 4),
        Err(PresentationError::Showcase { index: 4, .. })
    ));
    let mut unsupported = file()?;
    unsupported.preview_owners.effect = PlayerId::NEUTRAL.0;
    assert!(matches!(
        PlayerPalette::prepare(unsupported, 4),
        Err(PresentationError::PreviewOwner(255))
    ));
    let mut missing = file()?;
    missing.player_colors_srgb.pop();
    assert!(matches!(
        PlayerPalette::prepare(missing, 4),
        Err(PresentationError::Slots(7))
    ));
    Ok(())
}

#[test]
fn legacy_catalog_without_neutral_and_roles_is_rejected() -> TestResult {
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog/map_entity_visuals.ron"),
    )?;
    let legacy = source
        .split("    neutral_color_srgb:")
        .next()
        .ok_or("palette boundary")?
        .to_owned()
        + ")";
    assert!(ron::from_str::<PaletteFile>(&legacy).is_err());
    Ok(())
}
