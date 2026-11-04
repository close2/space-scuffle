//! Path globs in the style of Git's pathspec globs, plus `{a,b}` alternatives.
//!
//! # Responsibility
//! Compiles the patterns of the suite catalog and matches them against repository paths
//! (relative, `/`-separated, as `git diff --name-only` prints them).
//!
//! # How it fits
//! Every rule of `crates/gate/suites.ron` names its paths with these globs (blueprint §4.1); the
//! selection (`select.rs`) and the catalog test match every changed or tracked path against them.
//!
//! # Invariants
//! - `*` matches any run of characters except `/`; `?` matches one character except `/`.
//! - `**/` matches zero or more whole directories; any other `**` matches any run of characters
//!   including `/` (so `docs/**` matches everything under `docs/`, and `**/*.md` matches
//!   `README.md` as well as `docs/a/b.md`).
//! - `{a,b,c}` expands to one alternative per comma-separated part before matching; braces do
//!   not nest. A pattern without wildcards matches exactly that path.
//! - Matching is a dynamic program over (pattern position, path position), so it cannot take
//!   exponential time however many stars a pattern holds.

use crate::GateError;
use std::fmt;

/// One compiled piece of a pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    /// A character that must appear as written.
    Literal(char),
    /// `?`: one character other than `/`.
    One,
    /// `*`: any run of characters other than `/`.
    Star,
    /// `**/`: nothing, or any run of characters that ends with `/`.
    Directories,
    /// `**` not followed by `/`: any run of characters.
    Anything,
}

/// A compiled glob: one token list per brace alternative.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Glob {
    /// The pattern as written in the catalog, for reports.
    pattern: String,
    /// One token sequence per alternative of the brace expansion.
    alternatives: Vec<Vec<Token>>,
}

impl Glob {
    /// Compiles `pattern`.
    ///
    /// # Errors
    /// [`GateError::Glob`] when the pattern is empty, absolute, or has an unbalanced or nested
    /// brace.
    pub fn new(pattern: &str) -> Result<Self, GateError> {
        let fail = |reason| GateError::Glob {
            pattern: pattern.to_string(),
            reason,
        };
        if pattern.is_empty() {
            return Err(fail("is empty"));
        }
        if pattern.starts_with('/') {
            return Err(fail("must be relative to the repository root"));
        }
        let alternatives = expand_braces(pattern)
            .map_err(fail)?
            .iter()
            .map(|alternative| tokenize(alternative))
            .collect();
        Ok(Self {
            pattern: pattern.to_string(),
            alternatives,
        })
    }

    /// The pattern as written.
    #[must_use]
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// Whether `path` (relative to the repository root, `/`-separated) matches.
    #[must_use]
    pub fn matches(&self, path: &str) -> bool {
        let text: Vec<char> = path.chars().collect();
        self.alternatives
            .iter()
            .any(|tokens| matches_tokens(tokens, &text))
    }
}

impl fmt::Display for Glob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.pattern)
    }
}

/// Expands every `{a,b}` group into the cartesian product of its alternatives.
fn expand_braces(pattern: &str) -> Result<Vec<String>, &'static str> {
    let Some(open) = pattern.find('{') else {
        if pattern.contains('}') {
            return Err("has `}` without `{`");
        }
        return Ok(vec![pattern.to_string()]);
    };
    let rest = &pattern[open + 1..];
    let close = rest.find('}').ok_or("has `{` without `}`")?;
    let group = &rest[..close];
    if group.contains('{') {
        return Err("nests braces");
    }
    let prefix = &pattern[..open];
    if prefix.contains('}') {
        return Err("has `}` without `{`");
    }
    let suffixes = expand_braces(&rest[close + 1..])?;
    let mut out = Vec::new();
    for part in group.split(',') {
        for suffix in &suffixes {
            out.push(format!("{prefix}{part}{suffix}"));
        }
    }
    Ok(out)
}

/// Turns one brace-free pattern into tokens.
fn tokenize(pattern: &str) -> Vec<Token> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' if chars.get(i + 1) == Some(&'*') => {
                if chars.get(i + 2) == Some(&'/') {
                    tokens.push(Token::Directories);
                    i += 3;
                } else {
                    tokens.push(Token::Anything);
                    i += 2;
                }
            }
            '*' => {
                tokens.push(Token::Star);
                i += 1;
            }
            '?' => {
                tokens.push(Token::One);
                i += 1;
            }
            c => {
                tokens.push(Token::Literal(c));
                i += 1;
            }
        }
    }
    tokens
}

/// Whether `tokens` match all of `text`.
///
/// `table[p][t]` is true when `tokens[p..]` matches `text[t..]`; filled from the end.
fn matches_tokens(tokens: &[Token], text: &[char]) -> bool {
    let width = text.len() + 1;
    let mut table = vec![false; (tokens.len() + 1) * width];
    table[tokens.len() * width + text.len()] = true;
    for p in (0..tokens.len()).rev() {
        for t in (0..=text.len()).rev() {
            let next = |q: usize, s: usize| table[q * width + s];
            let here = text.get(t).copied();
            let value = match tokens[p] {
                Token::Literal(c) => here == Some(c) && next(p + 1, t + 1),
                Token::One => here.is_some_and(|c| c != '/') && next(p + 1, t + 1),
                Token::Star => {
                    next(p + 1, t) || (here.is_some_and(|c| c != '/') && table[p * width + t + 1])
                }
                Token::Anything => next(p + 1, t) || (here.is_some() && table[p * width + t + 1]),
                Token::Directories => {
                    next(p + 1, t)
                        || (t..text.len()).any(|end| text[end] == '/' && next(p + 1, end + 1))
                }
            };
            table[p * width + t] = value;
        }
    }
    table[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), GateError>;

    #[test]
    fn literal_patterns_match_exactly() -> TestResult {
        let g = Glob::new("Cargo.toml")?;
        assert!(g.matches("Cargo.toml"));
        assert!(!g.matches("crates/x/Cargo.toml"));
        assert!(!g.matches("Cargo.toml.bak"));
        Ok(())
    }

    #[test]
    fn single_star_stays_inside_one_directory() -> TestResult {
        let g = Glob::new("crates/*/Cargo.toml")?;
        assert!(g.matches("crates/sim_core/Cargo.toml"));
        assert!(!g.matches("crates/a/b/Cargo.toml"));
        assert!(Glob::new("rust-toolchain*")?.matches("rust-toolchain.toml"));
        assert!(!Glob::new("*.md")?.matches("docs/a.md"));
        Ok(())
    }

    #[test]
    fn double_star_crosses_directories() -> TestResult {
        let g = Glob::new("docs/**")?;
        assert!(g.matches("docs/plans/a.md"));
        assert!(g.matches("docs/a"));
        assert!(!g.matches("docsx/a"));
        let any_md = Glob::new("**/*.md")?;
        assert!(any_md.matches("README.md"));
        assert!(any_md.matches("docs/plans/x.md"));
        assert!(!any_md.matches("docs/plans/x.ron"));
        let middle = Glob::new("crates/**/tests.rs")?;
        assert!(middle.matches("crates/tests.rs"));
        assert!(middle.matches("crates/a/b/tests.rs"));
        Ok(())
    }

    #[test]
    fn question_mark_is_one_character_but_not_a_slash() -> TestResult {
        let g = Glob::new("a?c")?;
        assert!(g.matches("abc"));
        assert!(!g.matches("a/c"));
        assert!(!g.matches("ac"));
        Ok(())
    }

    #[test]
    fn braces_expand_to_alternatives() -> TestResult {
        let g = Glob::new("crates/client/src/{camera.rs,vfx/**,terrain/**}")?;
        assert!(g.matches("crates/client/src/camera.rs"));
        assert!(g.matches("crates/client/src/vfx/pool.rs"));
        assert!(g.matches("crates/client/src/terrain/mesh/a.rs"));
        assert!(!g.matches("crates/client/src/camera_drag.rs"));
        let two = Glob::new("{a,b}/{x,y}")?;
        assert!(two.matches("b/x"));
        assert!(!two.matches("a/z"));
        Ok(())
    }

    #[test]
    fn malformed_patterns_are_errors() {
        for bad in ["", "/abs", "a{b", "a}b", "{a,{b}}", "x}{y"] {
            assert!(Glob::new(bad).is_err(), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn many_stars_do_not_explode() -> TestResult {
        let g = Glob::new("*a*a*a*a*a*a*a*a*b")?;
        let text = "a".repeat(200);
        assert!(!g.matches(&text));
        Ok(())
    }
}
