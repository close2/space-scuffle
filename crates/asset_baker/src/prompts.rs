//! Veo prompts: composed from data and validated before any spend (plan
//! `2026-11-04-phase-8-5-veo-media.md`, decision D-2).
//!
//! # Responsibility
//! Loads the house style (`assets/catalog/media_style.ron`), composes the prompt of one clip
//! from the identity's authored `veo_prompts` entry, the action description and the style
//! sentence, and checks a composed prompt and its keyframe against the rules in that file.
//!
//! # How it fits
//! Charter §3.3 puts media generation prompts in data: no prompt text lives in Rust. The plan
//! (`veo_plan`) composes every clip's prompt; `validate-prompts`, and `generate-veo` before it
//! reads a credential, refuse a run while any clip it would pay for has a problem.
//!
//! # Invariants
//! - A clip without an authored prompt still gets a composed prompt (from the display name and
//!   lore) so that a dry run can show it, but it is marked unauthored and refused for a paid run.
//! - Every problem of every clip is reported at once, so one edit pass can fix them all.

use crate::error::BakerError;
use serde::Deserialize;
use sim_types::enums::AnimState;
use std::fmt;
use std::path::{Path, PathBuf};

/// Path of the media style file inside an asset bundle.
pub const MEDIA_STYLE_FILE: &str = "catalog/media_style.ron";

/// One description per animation state, placed between an identity's prompt and the style.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionDescriptions {
    /// `Constructing`: a structure being raised.
    pub constructing: String,
    /// `Idle`: the ambient loop.
    pub idle: String,
    /// `Walk`: locomotion.
    pub walk: String,
    /// `Attack`: a weapon cycle.
    pub attack: String,
    /// `Cast`: harvesting or building in place.
    pub cast: String,
    /// `Die`: destruction.
    pub die: String,
}

impl ActionDescriptions {
    /// The description of `state`.
    #[must_use]
    pub fn get(&self, state: AnimState) -> &str {
        match state {
            AnimState::Constructing => &self.constructing,
            AnimState::Idle => &self.idle,
            AnimState::Walk => &self.walk,
            AnimState::Attack => &self.attack,
            AnimState::Cast => &self.cast,
            AnimState::Die => &self.die,
        }
    }
}

/// The house style and prompt rules of `media_style.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaStyle {
    /// Sentence that ends every prompt.
    pub style: String,
    /// Per-state action descriptions.
    pub actions: ActionDescriptions,
    /// Longest composed prompt accepted, in characters (Unicode scalar values).
    pub max_prompt_chars: usize,
    /// Real names a prompt must not contain, matched as whole words ignoring case.
    pub deny_list: Vec<String>,
    /// Text that marks an unfinished prompt (see [`contains_marker`]).
    pub placeholder_markers: Vec<String>,
}

impl MediaStyle {
    /// Loads `media_style.ron` from the asset bundle at `assets_root`.
    ///
    /// # Errors
    /// [`BakerError::DataFile`] if it cannot be read or parsed.
    pub fn load(assets_root: &Path) -> Result<Self, BakerError> {
        let path = assets_root.join(MEDIA_STYLE_FILE);
        let text = std::fs::read_to_string(&path).map_err(|source| BakerError::Io {
            path: path.clone(),
            source,
        })?;
        ron::from_str(&text).map_err(|error| BakerError::DataFile {
            path,
            reason: error.to_string(),
        })
    }
}

/// A composed prompt and whether its identity part was authored for this state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposedPrompt {
    /// The full prompt text sent to the model.
    pub text: String,
    /// False when the identity had no `veo_prompts` entry for the state and the display name
    /// and lore were used instead.
    pub authored: bool,
}

/// Composes the prompt of `slug`'s `state` clip.
///
/// `slug` must be an exact slug of `catalog`, the catalog loaded at run time: no generic prompt
/// exists for an unknown identity, because money must not be spent on a clip nothing can use.
///
/// # Errors
/// [`BakerError::UnknownArchetype`] if `slug` is not an exact catalog slug.
pub fn build_action_prompt(
    catalog: &sim_core::UnitCatalog,
    style: &MediaStyle,
    slug: &str,
    state: AnimState,
) -> Result<ComposedPrompt, BakerError> {
    let archetype = crate::veo_client::exact_catalog_archetype(slug, catalog)?;
    let action = style.actions.get(state);
    Ok(match archetype.veo_prompt(state) {
        Some(prompt) => ComposedPrompt {
            text: format!("{prompt} {action} {}", style.style),
            authored: true,
        },
        None => ComposedPrompt {
            text: format!(
                "{}: {} {action} {}",
                archetype.display_name, archetype.lore_description, style.style
            ),
            authored: false,
        },
    })
}

/// Where a clip's keyframe image comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyframeSource {
    /// Text-to-video: no image.
    None,
    /// An authored keyframe file.
    File(PathBuf),
    /// The frame at 1.0 s of the identity's Idle clip, extracted when the clip is generated.
    ChainedFromIdle,
}

impl fmt::Display for KeyframeSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => f.write_str("none (text to video)"),
            Self::File(path) => write!(f, "{}", path.display()),
            Self::ChainedFromIdle => f.write_str("frame at 1.0 s of the Idle clip"),
        }
    }
}

/// One thing wrong with one clip's prompt or keyframe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptIssue {
    /// No `veo_prompts` entry for the state; the display-name fallback is refused.
    NotAuthored,
    /// The composed prompt is too long.
    TooLong {
        /// Its length in characters.
        chars: usize,
        /// The limit.
        max: usize,
    },
    /// The prompt contains a placeholder marker.
    Placeholder {
        /// The marker found.
        marker: String,
    },
    /// The prompt names a denied term.
    Denied {
        /// The term found.
        term: String,
    },
    /// The keyframe file is missing or not a readable image.
    KeyframeUnreadable {
        /// The keyframe file.
        path: PathBuf,
        /// Why it cannot be used.
        reason: String,
    },
    /// The keyframe is chained from an Idle clip that neither exists nor is in the run.
    NoIdleToChain,
}

impl fmt::Display for PromptIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAuthored => f.write_str(
                "no authored prompt in veo_prompts (the display-name fallback is refused for a paid run)",
            ),
            Self::TooLong { chars, max } => {
                write!(f, "composed prompt is {chars} characters, longer than {max}")
            }
            Self::Placeholder { marker } => write!(f, "contains placeholder text '{marker}'"),
            Self::Denied { term } => write!(f, "names the denied term '{term}'"),
            Self::KeyframeUnreadable { path, reason } => {
                write!(f, "keyframe '{}' cannot be used: {reason}", path.display())
            }
            Self::NoIdleToChain => f.write_str(
                "its keyframe is chained from the Idle clip, which neither exists nor is in this run",
            ),
        }
    }
}

/// A [`PromptIssue`] of one clip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptProblem {
    /// Catalog slug.
    pub slug: String,
    /// Animation state.
    pub state: AnimState,
    /// What is wrong.
    pub issue: PromptIssue,
}

impl fmt::Display for PromptProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}: {}",
            self.slug,
            crate::veo_plan::state_slug(self.state),
            self.issue
        )
    }
}

/// Renders `problems` one per line, for an error message.
#[must_use]
pub fn render_problems(problems: &[PromptProblem]) -> String {
    problems.iter().fold(String::new(), |mut text, problem| {
        use std::fmt::Write as _;
        // Writing to a String cannot fail.
        let _ = write!(text, "\n  {problem}");
        text
    })
}

/// Every issue of a composed prompt under `style`'s rules.
#[must_use]
pub fn check_prompt(style: &MediaStyle, prompt: &ComposedPrompt) -> Vec<PromptIssue> {
    let mut issues = Vec::new();
    if !prompt.authored {
        issues.push(PromptIssue::NotAuthored);
    }
    let chars = prompt.text.chars().count();
    if chars > style.max_prompt_chars {
        issues.push(PromptIssue::TooLong {
            chars,
            max: style.max_prompt_chars,
        });
    }
    for marker in &style.placeholder_markers {
        if contains_marker(&prompt.text, marker) {
            issues.push(PromptIssue::Placeholder {
                marker: marker.clone(),
            });
        }
    }
    for term in &style.deny_list {
        if contains_term(&prompt.text, term) {
            issues.push(PromptIssue::Denied { term: term.clone() });
        }
    }
    issues
}

/// The issue of a keyframe file, if it is missing or not a decodable image.
#[must_use]
pub fn check_keyframe(source: &KeyframeSource) -> Option<PromptIssue> {
    let KeyframeSource::File(path) = source else {
        return None;
    };
    image::ImageReader::open(path)
        .map_err(|error| error.to_string())
        .and_then(|reader| reader.with_guessed_format().map_err(|e| e.to_string()))
        .and_then(|reader| reader.decode().map_err(|e| e.to_string()))
        .err()
        .map(|reason| PromptIssue::KeyframeUnreadable {
            path: path.clone(),
            reason,
        })
}

/// Whether `text` contains `term` as a whole word or phrase, ignoring ASCII case.
///
/// A match must not be preceded or followed by a letter or digit, so `Terran` does not match
/// `Subterranean`.
#[must_use]
pub fn contains_term(text: &str, term: &str) -> bool {
    let text = text.to_ascii_lowercase();
    let term = term.to_ascii_lowercase();
    if term.is_empty() {
        return false;
    }
    let is_word = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
    text.match_indices(&term).any(|(start, _)| {
        let before = text[..start].chars().next_back();
        let after = text[start + term.len()..].chars().next();
        !is_word(before) && !is_word(after)
    })
}

/// Whether `text` contains the placeholder `marker`: as a whole word ignoring case when the
/// marker is alphanumeric, anywhere otherwise (`{`, `}`).
#[must_use]
pub fn contains_marker(text: &str, marker: &str) -> bool {
    if marker.chars().all(char::is_alphanumeric) {
        contains_term(text, marker)
    } else {
        !marker.is_empty() && text.contains(marker)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn style() -> MediaStyle {
        MediaStyle::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"))
            .expect("shipped media style loads")
    }

    fn authored(text: &str) -> ComposedPrompt {
        ComposedPrompt {
            text: text.to_string(),
            authored: true,
        }
    }

    #[test]
    fn whole_words_only() {
        assert!(contains_term("A Terran outpost", "terran"));
        assert!(!contains_term("Subterranean scorpion", "Terran"));
        assert!(contains_term("like star wars, with", "Star Wars"));
        assert!(!contains_term("anything", ""));
        assert!(contains_marker("value {name}", "{"));
        assert!(contains_marker("tbd later", "TBD"));
        assert!(!contains_marker("tbdx later", "TBD"));
    }

    #[test]
    fn every_issue_of_a_prompt_is_reported() {
        let mut rules = style();
        rules.max_prompt_chars = 10;
        let marker = concat!("TO", "DO");
        let issues = check_prompt(
            &rules,
            &ComposedPrompt {
                text: format!("{marker} a StarCraft unit {{x}}"),
                authored: false,
            },
        );
        assert!(issues.contains(&PromptIssue::NotAuthored));
        assert!(issues
            .iter()
            .any(|i| matches!(i, PromptIssue::TooLong { max: 10, .. })));
        assert!(issues.contains(&PromptIssue::Placeholder {
            marker: marker.to_string()
        }));
        assert!(issues.contains(&PromptIssue::Placeholder {
            marker: "{".to_string()
        }));
        assert!(issues.contains(&PromptIssue::Denied {
            term: "StarCraft".to_string()
        }));
    }

    #[test]
    fn the_shipped_style_passes_its_own_rules_and_names_no_franchise() {
        let rules = style();
        let sentence = authored(&rules.style);
        assert_eq!(check_prompt(&rules, &sentence), Vec::new());
        for state in AnimState::ALL {
            assert_eq!(
                check_prompt(&rules, &authored(rules.actions.get(state))),
                Vec::new(),
                "{state:?}"
            );
        }
    }

    #[test]
    fn a_missing_or_corrupt_keyframe_is_an_issue() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("missing.png");
        assert!(matches!(
            check_keyframe(&KeyframeSource::File(missing)),
            Some(PromptIssue::KeyframeUnreadable { .. })
        ));
        let corrupt = dir.path().join("corrupt.jpg");
        std::fs::write(&corrupt, b"not an image").expect("write");
        assert!(check_keyframe(&KeyframeSource::File(corrupt)).is_some());
        let good = dir.path().join("good.png");
        image::RgbImage::new(2, 2).save(&good).expect("png");
        assert_eq!(check_keyframe(&KeyframeSource::File(good)), None);
        assert_eq!(check_keyframe(&KeyframeSource::ChainedFromIdle), None);
    }
}
