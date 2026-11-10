//! The prompt class "effect element" (plan `2026-11-09-effect-visualization.md` §6.1, §6.2).
//!
//! # Responsibility
//! [`EffectElementStyle`] is the `effect_element` section of `assets/catalog/media_style.ron`:
//! the texts that make a model draw one isolated effect element on pure black, side-on or top
//! down, with no floor, horizon or rays. [`build_effect_prompt`] composes the prompt of an
//! [`EffectElementRequest`] (which names a set and an element, never a unit) for a still (the
//! image model) or a clip (Veo), and checks it under the same rules as unit prompts (deny
//! list, placeholder markers, length). [`on_white_edit_prompt`] is the edit request of a
//! difference-matte pair.
//!
//! # How it fits
//! `asset_baker effect-prompt` prints a composed prompt for free; `asset_baker effect-image`
//! sends it through [`crate::effects::image_client`]. The texts are data: editing them needs no
//! rebuild (charter §3.3).
//!
//! # Invariants
//! - Composition order: the request's description, the view text, the medium text, the style.
//! - A prompt that breaks a rule is refused before anything is sent or spent.

use super::{check_name, EffectError};
use crate::prompts::{check_negative_prompt, check_prompt, ComposedPrompt, MediaStyle};
use serde::{Deserialize, Serialize};

/// The `effect_element` section of `media_style.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectElementStyle {
    /// Sentence that ends every effect prompt: pure black, isolated, nothing else in frame.
    pub style: String,
    /// View text for an element seen side-on.
    pub side_on: String,
    /// View text for an element seen from directly above.
    pub top_down: String,
    /// Medium text for a still image.
    pub still: String,
    /// Medium text for a Veo clip: it starts and ends black.
    pub clip: String,
    /// The edit request that renders the same element on pure white.
    pub on_white_edit: String,
    /// Nouns a clip request appends to the global negative prompt.
    pub negative_terms: String,
}

/// How the element is seen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectView {
    /// From the side, at eye level (camera-facing flipbooks).
    SideOn,
    /// From directly above (decals, ground elements).
    TopDown,
}

impl std::str::FromStr for EffectView {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "side-on" => Ok(Self::SideOn),
            "top-down" => Ok(Self::TopDown),
            other => Err(format!("unknown view '{other}': use side-on or top-down")),
        }
    }
}

/// What the request produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectMedium {
    /// A still image from the image model.
    Still,
    /// A Veo clip.
    Clip,
}

impl std::str::FromStr for EffectMedium {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "still" => Ok(Self::Still),
            "clip" => Ok(Self::Clip),
            other => Err(format!("unknown medium '{other}': use still or clip")),
        }
    }
}

/// One effect-element request: a set, an element and what to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectElementRequest {
    /// Texture set name.
    pub set: String,
    /// Element name.
    pub element: String,
    /// What the element is, in plain words ("a dense billowing smoke puff").
    pub description: String,
    /// How it is seen.
    pub view: EffectView,
    /// Still or clip.
    pub medium: EffectMedium,
}

/// Composes and checks the prompt of `request`.
///
/// # Errors
/// [`EffectError::InvalidName`] for a set or element that cannot name a file, and
/// [`EffectError::PromptInvalid`] for a prompt that breaks a rule of `style` (including an
/// empty description).
pub fn build_effect_prompt(
    style: &MediaStyle,
    request: &EffectElementRequest,
) -> Result<ComposedPrompt, EffectError> {
    check_name("set", &request.set)?;
    check_name("element", &request.element)?;
    let texts = &style.effect_element;
    let view = match request.view {
        EffectView::SideOn => &texts.side_on,
        EffectView::TopDown => &texts.top_down,
    };
    let medium = match request.medium {
        EffectMedium::Still => &texts.still,
        EffectMedium::Clip => &texts.clip,
    };
    let description = request.description.trim();
    let prompt = ComposedPrompt {
        text: format!("{description} {view} {medium} {}", texts.style),
        authored: !description.is_empty(),
    };
    let issues = check_prompt(style, &prompt);
    if issues.is_empty() {
        Ok(prompt)
    } else {
        Err(EffectError::PromptInvalid { issues })
    }
}

/// The edit prompt that renders the same element on pure white.
///
/// # Errors
/// [`EffectError::PromptInvalid`] if the text breaks a rule of `style`.
pub fn on_white_edit_prompt(style: &MediaStyle) -> Result<ComposedPrompt, EffectError> {
    let prompt = ComposedPrompt {
        text: style.effect_element.on_white_edit.clone(),
        authored: true,
    };
    let issues = check_prompt(style, &prompt);
    if issues.is_empty() {
        Ok(prompt)
    } else {
        Err(EffectError::PromptInvalid { issues })
    }
}

/// The negative prompt of an effect clip: the global one of `media/veo_request.ron` (if any)
/// and the effect class's terms.
///
/// # Errors
/// [`EffectError::PromptInvalid`] if the result breaks a rule of `style`.
pub fn effect_negative_prompt(
    style: &MediaStyle,
    global: Option<&str>,
) -> Result<String, EffectError> {
    let negative = crate::prompts::compose_negative_prompt(
        global,
        Some(style.effect_element.negative_terms.as_str()),
    )
    .unwrap_or_default();
    let issues = check_negative_prompt(style, &negative);
    if issues.is_empty() {
        Ok(negative)
    } else {
        Err(EffectError::PromptInvalid { issues })
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use std::path::Path;

    fn style() -> MediaStyle {
        MediaStyle::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"))
            .expect("shipped media style loads")
    }

    fn request(description: &str) -> EffectElementRequest {
        EffectElementRequest {
            set: "fx_test".to_string(),
            element: "smoke_puff".to_string(),
            description: description.to_string(),
            view: EffectView::SideOn,
            medium: EffectMedium::Still,
        }
    }

    #[test]
    fn a_prompt_is_description_view_medium_then_style() {
        let style = style();
        let prompt = build_effect_prompt(&style, &request("A dense smoke puff.")).expect("valid");
        let texts = &style.effect_element;
        assert_eq!(
            prompt.text,
            format!(
                "A dense smoke puff. {} {} {}",
                texts.side_on, texts.still, texts.style
            )
        );
        assert!(prompt.text.contains("pure black"));
    }

    #[test]
    fn an_empty_or_denied_description_is_refused() {
        let style = style();
        assert!(matches!(
            build_effect_prompt(&style, &request("  ")),
            Err(EffectError::PromptInvalid { .. })
        ));
        assert!(matches!(
            build_effect_prompt(&style, &request("A StarCraft explosion.")),
            Err(EffectError::PromptInvalid { .. })
        ));
        let mut bad = request("Smoke.");
        bad.element = "Smoke/Puff".to_string();
        assert!(matches!(
            build_effect_prompt(&style, &bad),
            Err(EffectError::InvalidName { .. })
        ));
    }

    #[test]
    fn the_shipped_edit_and_negative_texts_pass_the_rules() {
        let style = style();
        assert!(on_white_edit_prompt(&style)
            .expect("valid")
            .text
            .contains("pure white"));
        let negative = effect_negative_prompt(&style, Some("blurry")).expect("valid");
        assert!(negative.starts_with("blurry, "));
        assert!(negative.contains("horizon"));
    }
}
