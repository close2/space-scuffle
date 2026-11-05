//! Check C5 of the charter: no catalog identity is named in production Rust.
//!
//! # Responsibility
//! Charter §3.3 says units, structures, upgrades, start loadouts, factions and their weapon and
//! armour classes exist only as data, so adding or renaming one needs no code change. This module
//! reads every `slug` the identity catalogs define and reports each production identifier or
//! string literal that spells one of them, and each identifier that spells a variant of the
//! Rust enums (`Faction`, `WeaponClass`, `ArmorClass`) those identities replaced in Phase 9 WP12.
//!
//! # How it fits
//! Like the other source checks it takes the repository root and returns every [`Finding`]. The
//! slugs come from the files themselves, so the check grows with the data and never needs a list
//! of names in Rust. Identifiers are judged in the blanked production text ([`code_text`]), and
//! string literals are read from the original source between the quote pairs that blanking keeps.
//!
//! # Invariants
//! - Names are compared *folded*: ASCII-lowercased with `_`, `-` and spaces removed, so
//!   `lantern_moth`, `LanternMoth`, `LANTERN_MOTH` and `"Lantern Moth"` are the same name.
//! - An identity of two or more words is also found inside a longer identifier or literal, as a
//!   run of whole words (`spawn_lantern_moth`, `"units/lantern_moth/idle.mp4"`); a single-word
//!   identity (`charge`) only whole, and never as letters inside a word (`overcharge`).
//! - Test code (scope [`Scope::Production`] drops it), comments and the dev-only crates are
//!   never judged; tests may read slugs from data and name them.
//! - Blanking keeps every string delimiter and blanks escaped quotes, so the quotes left in the
//!   blanked text pair up in order, opening then closing.

use crate::source::{line_of, word_tokens};
use crate::{code_text, rust_files, CheckError, Finding, Scope};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Catalog files, relative to the repository root, whose `slug` fields are identities (C5).
pub const IDENTITY_CATALOGS: [&str; 5] = [
    "assets/catalog/units.ron",
    "assets/catalog/structures.ron",
    "assets/catalog/upgrades.ron",
    "assets/catalog/start_loadouts.ron",
    "assets/catalog/factions.ron",
];

/// Variants of the enums that named factions and classes in Rust before Phase 9 WP12 (C5).
///
/// They are kept here, in the check and nowhere else, so that the names cannot return as types.
pub const FORMER_IDENTITY_VARIANTS: [&str; 14] = [
    "AutomatonDirectorate",
    "CosmicEnclave",
    "SylvanSwarm",
    "DirectorateInfantry",
    "DirectorateVehicle",
    "DirectorateShip",
    "DirectoratePlating",
    "EnclaveGround",
    "EnclaveAir",
    "SwarmMelee",
    "SwarmMissile",
    "SwarmFlyer",
    "SwarmGroundCarapace",
    "SwarmFlyerCarapace",
];

/// `name` folded for comparison: ASCII-lowercased, without `_`, `-` or spaces.
#[must_use]
pub fn folded(name: &str) -> String {
    name.chars()
        .filter(|c| !matches!(c, '_' | '-' | ' '))
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The words of `text`: maximal runs of ASCII letters and digits, split further where a lower-case
/// letter or digit is followed by an upper-case one, so `spawn_LanternMoth` has the words `spawn`,
/// `Lantern` and `Moth`.
#[must_use]
pub fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut previous: Option<char> = None;
    for c in text.chars() {
        let boundary = !c.is_ascii_alphanumeric()
            || (c.is_ascii_uppercase()
                && previous.is_some_and(|p| p.is_ascii_lowercase() || p.is_ascii_digit()));
        if boundary && !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
        if c.is_ascii_alphanumeric() {
            current.push(c);
        }
        previous = Some(c);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Every `slug: "..."` value in `text`, with its one-based line.
///
/// A `slug` key is a whole word: `target_slug` and `ability_slug` reference identities defined
/// elsewhere and are not read.
#[must_use]
pub fn slug_fields(text: &str) -> Vec<(usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut slugs = Vec::new();
    for (start, token) in word_tokens(&chars) {
        if token != "slug" {
            continue;
        }
        let rest: String = chars[start + token.len()..].iter().collect();
        let Some(after_colon) = rest.trim_start().strip_prefix(':') else {
            continue;
        };
        let Some(body) = after_colon.trim_start().strip_prefix('"') else {
            continue;
        };
        if let Some(end) = body.find('"') {
            slugs.push((line_of(&chars, start), body[..end].to_owned()));
        }
    }
    slugs
}

/// One identity C5 looks for.
#[derive(Debug)]
struct Identity {
    /// What it is: the slug and the file defining it, or a former enum variant.
    what: String,
    /// The number of [`words`] in its spelling: an identity of two or more words is also found
    /// as a run of words inside a longer name.
    words: usize,
}

/// The identities by folded name.
fn identity_names(root: &Path) -> Result<BTreeMap<String, Identity>, CheckError> {
    let mut names = BTreeMap::new();
    for variant in FORMER_IDENTITY_VARIANTS {
        names.insert(
            folded(variant),
            Identity {
                what: format!("the former enum variant `{variant}`"),
                words: words(variant).len(),
            },
        );
    }
    for file in IDENTITY_CATALOGS {
        let path = root.join(file);
        let text = fs::read_to_string(&path).map_err(|source| CheckError::Io {
            path: path.clone(),
            source,
        })?;
        for (_, slug) in slug_fields(&text) {
            names.entry(folded(&slug)).or_insert_with(|| Identity {
                what: format!("the slug \"{slug}\" from {file}"),
                words: words(&slug).len(),
            });
        }
    }
    Ok(names)
}

/// The identity `spelled` names, and whether it only contains it: the whole of `spelled`
/// folded, else a run of two or more of its [`words`] that spells an identity of two or more
/// words (`spawn_lantern_moth`, `"assets/units/lantern_moth/idle.mp4"`). A single-word identity
/// such as `charge` is matched only whole, since it is also an ordinary word.
fn named_identity<'a>(
    names: &'a BTreeMap<String, Identity>,
    longest: usize,
    spelled: &str,
) -> Option<(&'a Identity, bool)> {
    if let Some(identity) = names.get(&folded(spelled)) {
        return Some((identity, false));
    }
    let lower: Vec<String> = words(spelled)
        .iter()
        .map(|word| word.to_ascii_lowercase())
        .collect();
    for start in 0..lower.len() {
        let mut run = String::new();
        for (count, word) in lower[start..].iter().enumerate() {
            run.push_str(word);
            if run.len() > longest {
                break;
            }
            if count == 0 {
                continue;
            }
            if let Some(identity) = names.get(&run).filter(|identity| identity.words >= 2) {
                return Some((identity, true));
            }
        }
    }
    None
}

/// Check C5: no production identifier or string literal names a catalog identity.
///
/// The identities are every `slug` in [`IDENTITY_CATALOGS`] and every name in
/// [`FORMER_IDENTITY_VARIANTS`]; matching is on [`folded`] names, of the whole identifier or
/// literal, or of a run of its [`words`] for an identity of two or more words (so
/// `spawn_lantern_moth` and `"units/lantern_moth/idle.mp4"` are reported, `charge_time` is
/// not). A mechanic is reached through
/// a capability field or an ID resolved from data, never by naming the unit, structure, upgrade,
/// loadout, faction or class that has it (charter §3.3).
///
/// # Errors
/// [`CheckError::Io`] if a catalog, directory or source file cannot be read.
pub fn no_catalog_identities_in_code(root: &Path) -> Result<Vec<Finding>, CheckError> {
    let names = identity_names(root)?;
    let longest = names.keys().map(String::len).max().unwrap_or(0);
    let mut findings = Vec::new();
    for path in rust_files(root, Scope::Production)? {
        let original: Vec<char> = fs::read_to_string(&path)
            .map_err(|source| CheckError::Io {
                path: path.clone(),
                source,
            })?
            .chars()
            .collect();
        let code: Vec<char> = code_text(&path, Scope::Production)?.chars().collect();
        let mut report = |index: usize, spelled: &str, what: &str| {
            if let Some((identity, inside)) = named_identity(&names, longest, spelled) {
                let verb = if inside { "contains" } else { "names" };
                findings.push(Finding::new(
                    root,
                    &path,
                    line_of(&code, index),
                    format!(
                        "{what} `{spelled}` {verb} {}; production code must reach it \
                         through catalog data, not by name (charter §3.3)",
                        identity.what
                    ),
                ));
            }
        };
        for (start, token) in word_tokens(&code) {
            report(start, &token, "identifier");
        }
        let quotes: Vec<usize> = code
            .iter()
            .enumerate()
            .filter_map(|(i, &c)| (c == '"').then_some(i))
            .collect();
        let opening = quotes.iter().step_by(2);
        let closing = quotes.iter().skip(1).step_by(2);
        for (&open, &close) in opening.zip(closing) {
            let literal: String = original[open + 1..close].iter().collect();
            report(open, &literal, "string literal");
        }
    }
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    fn plant(root: &Path, files: &[(&str, &str)]) -> Result<(), std::io::Error> {
        for (relative, text) in files {
            let path = root.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, text)?;
        }
        Ok(())
    }

    /// A tree whose catalogs define `lantern_moth`, `charge_upgrade`, `field_kit` and
    /// `star_league`, and whose production code is clean.
    fn tree() -> Result<tempfile::TempDir, std::io::Error> {
        let dir = tempfile::tempdir()?;
        plant(
            dir.path(),
            &[
                (
                    "assets/catalog/units.ron",
                    "(units: [(slug: \"lantern_moth\", faction: \"star_league\")])",
                ),
                (
                    "assets/catalog/structures.ron",
                    "(structures: [(slug : \"hive_dome\", target_slug: \"not_an_identity\")])",
                ),
                (
                    "assets/catalog/upgrades.ron",
                    "(upgrades: [(slug: \"charge_upgrade\")])",
                ),
                (
                    "assets/catalog/start_loadouts.ron",
                    "(loadouts: [(slug: \"field_kit\")])",
                ),
                (
                    "assets/catalog/factions.ron",
                    "// slug: \"commented_identity\" counts too; it is harmless\n\
                     (factions: [(faction_id: 0, slug: \"star_league\")])",
                ),
                (
                    "crates/sim_core/src/lib.rs",
                    "//! x\nfn f(arch: &Arch) -> bool { arch.slug == \"other\" && !arch.charge }\n",
                ),
            ],
        )?;
        Ok(dir)
    }

    #[test]
    fn slug_fields_reads_whole_word_slug_keys_only() {
        let text = "(slug: \"a\",\n target_slug: \"b\", ability_slug: \"c\",\n slug :  \"d\")";
        assert_eq!(
            slug_fields(text),
            vec![(1, "a".to_owned()), (3, "d".to_owned())]
        );
    }

    #[test]
    fn folding_equates_snake_camel_screaming_and_display_spellings() {
        for spelling in [
            "lantern_moth",
            "LanternMoth",
            "LANTERN_MOTH",
            "Lantern Moth",
        ] {
            assert_eq!(folded(spelling), "lanternmoth", "{spelling}");
        }
    }

    #[test]
    fn a_clean_tree_has_no_findings() -> Result<(), Box<dyn Error>> {
        let dir = tree()?;
        assert_eq!(no_catalog_identities_in_code(dir.path())?, Vec::new());
        Ok(())
    }

    #[test]
    fn planted_identities_are_reported_with_path_and_line() -> Result<(), Box<dyn Error>> {
        let dir = tree()?;
        plant(
            dir.path(),
            &[
                (
                    "crates/client/src/badge.rs",
                    "//! x\n\
                     const A: &str = \"lantern_moth\";\n\
                     enum E { StarLeague, Other }\n\
                     fn g() -> &'static str { r#\"Field Kit\"# }\n\
                     fn h() { let x = \"esc\\\"aped\"; let y = \"HIVE_DOME\"; }\n\
                     fn k() { match v { Faction::CosmicEnclave => 1 } }\n\
                     // LanternMoth in a comment is fine\n",
                ),
                (
                    "crates/sim_core/tests/t.rs",
                    "//! x\nconst A: &str = \"lantern_moth\";\n",
                ),
                (
                    "crates/sim_core/src/m.rs",
                    "//! x\nfn f() {}\n#[cfg(test)]\nmod tests { const A: &str = \"field_kit\"; }\n",
                ),
            ],
        )?;
        let findings = no_catalog_identities_in_code(dir.path())?;
        let located: Vec<(String, usize)> = findings
            .iter()
            .map(|f| (crate::slash_path(&f.path), f.line))
            .collect();
        let badge = "crates/client/src/badge.rs".to_owned();
        assert_eq!(
            located,
            vec![
                (badge.clone(), 3),
                (badge.clone(), 6),
                (badge.clone(), 2),
                (badge.clone(), 4),
                (badge, 5),
            ],
            "{findings:?}"
        );
        assert!(findings[0].message.contains("star_league"));
        assert!(findings[1].message.contains("`CosmicEnclave`"));
        assert!(findings[2].message.contains("units.ron"));
        Ok(())
    }

    /// The cases `client/round2_visual_tests::test_zero_hardcoded_creature_names_in_rust_src`
    /// caught and C5 did not (tiered gates WP6): a multi-word identity as a run of words inside
    /// a longer identifier or string literal. Single-word identities (`charge`) and words that
    /// merely contain an identity's letters stay unreported.
    #[test]
    fn a_multi_word_identity_inside_a_longer_name_or_literal_is_reported(
    ) -> Result<(), Box<dyn Error>> {
        let dir = tree()?;
        plant(
            dir.path(),
            &[(
                "crates/client/src/inside.rs",
                "//! x\n\
                 fn spawn_lantern_moth() {}\n\
                 const LANTERN_MOTH_RANGE: u32 = 3;\n\
                 struct StarLeagueBanner;\n\
                 const P: &str = \"assets/units/lantern_moth/idle.mp4\";\n\
                 const Q: &str = \"hive_dome_ready\";\n\
                 fn charge_time() {}\n\
                 const R: &str = \"overcharge for a starleagues fan; slantern_mothy\";\n\
                 fn field() { let kit = 1; }\n",
            )],
        )?;
        let findings = no_catalog_identities_in_code(dir.path())?;
        let lines: Vec<usize> = findings.iter().map(|f| f.line).collect();
        assert_eq!(lines, vec![2, 3, 4, 5, 6], "{findings:?}");
        assert!(
            findings[0]
                .message
                .contains("`spawn_lantern_moth` contains"),
            "{findings:?}"
        );
        assert!(findings[0].message.contains("lantern_moth"));
        assert!(findings[2].message.contains("star_league"));
        Ok(())
    }

    #[test]
    fn words_split_at_separators_and_case_changes() {
        assert_eq!(
            words("spawn_LanternMoth-v2 assets/HIVE_DOME.png"),
            ["spawn", "Lantern", "Moth", "v2", "assets", "HIVE", "DOME", "png"]
        );
        assert_eq!(words("__"), Vec::<String>::new());
    }

    #[test]
    fn a_missing_identity_catalog_is_an_error_not_a_pass() -> Result<(), Box<dyn Error>> {
        let dir = tree()?;
        fs::remove_file(dir.path().join("assets/catalog/factions.ron"))?;
        assert!(matches!(
            no_catalog_identities_in_code(dir.path()),
            Err(CheckError::Io { .. })
        ));
        Ok(())
    }
}
