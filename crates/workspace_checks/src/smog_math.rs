//! Check C23: smog production code calls no host transcendental function (owner question Q0014).
//!
//! # Responsibility
//! The smog goldens of `crates/client/tests/smog_golden_tests.rs` are bit-exact and must not
//! depend on the host's C math library, whose last bit differs between glibc versions, musl,
//! macOS and Windows. Every exponential, sine, cosine, arctangent and power on a smog path
//! therefore goes through `client::smog::math` (`crates/client/src/smog/math.rs`), which is the
//! pure-Rust `libm` crate. This check finds the calls that bypass it: a `std` float method such as
//! `.sin(` or `f32::exp(`, and the Bevy and glam entry points that reach `sin_cos` or `powf`
//! (`to_linear(`, `from_rotation_y(`, `to_euler(`, ...). Plan:
//! `docs/plans/2026-11-04-libm-smog-math.md`, D-4.
//!
//! # How it fits
//! `tests/charter.rs` runs [`host_math_in_smog`] on the repository; the unit tests below run it
//! on planted trees. Clippy's `disallowed-methods` cannot do this: clippy reads only the nearest
//! `clippy.toml`, so a ban would cover the whole client, where host math stays allowed.
//!
//! # Invariants
//! - Only production code below [`SMOG_DIR`] is judged; [`SMOG_MATH_FILE`] is exempt, and so are
//!   comments, literals and `#[cfg(test)]` items (the shared scanner of `source.rs` blanks them).
//! - A method name of [`HOST_MATH_METHODS`] is reported when it is called (`(` follows) after a
//!   `.`, `f32::` or `f64::`; a name of [`HOST_MATH_ENTRY_POINTS`] is reported whenever it is
//!   called.
//! - Limit: the check reads text and does not follow calls into other client modules; a helper
//!   outside [`SMOG_DIR`] that reaches a smog hash is found by the plan's interposer run, and then
//!   added here.

use crate::source::{line_of, word_tokens};
use crate::{code_text, rust_files, CheckError, Finding, Scope};
use std::path::Path;

/// Directory, relative to the repository root, whose production code the check judges.
pub const SMOG_DIR: &str = "crates/client/src/smog";

/// The one smog file allowed to compute transcendental functions: the `libm` crate wrapper.
pub const SMOG_MATH_FILE: &str = "crates/client/src/smog/math.rs";

/// `f32` and `f64` methods whose result comes from the host's math library.
pub const HOST_MATH_METHODS: [&str; 22] = [
    "sin", "cos", "tan", "sin_cos", "exp", "exp2", "exp_m1", "ln", "log", "log2", "log10", "ln_1p",
    "powf", "atan2", "atan", "asin", "acos", "sinh", "cosh", "tanh", "hypot", "cbrt",
];

/// Bevy and glam functions that compute through the host's `sin_cos`, `atan2` or `powf`.
pub const HOST_MATH_ENTRY_POINTS: [&str; 7] = [
    "to_linear",
    "from_rotation_y",
    "from_rotation_x",
    "from_rotation_z",
    "from_axis_angle",
    "from_euler",
    "to_euler",
];

/// Check C23: no host transcendental call in smog production code outside [`SMOG_MATH_FILE`].
///
/// # Errors
/// [`CheckError::Io`] if a directory or file cannot be read.
pub fn host_math_in_smog(root: &Path) -> Result<Vec<Finding>, CheckError> {
    let smog_dir = root.join(SMOG_DIR);
    let exempt = root.join(SMOG_MATH_FILE);
    let mut findings = Vec::new();
    for path in rust_files(root, Scope::Production)? {
        if !path.starts_with(&smog_dir) || path == exempt {
            continue;
        }
        let chars: Vec<char> = code_text(&path, Scope::Production)?.chars().collect();
        for (start, token) in word_tokens(&chars) {
            let end = start + token.chars().count();
            if !next_non_space_is(&chars, end, '(') {
                continue;
            }
            let method = HOST_MATH_METHODS.contains(&token.as_str()) && follows_path(&chars, start);
            if method || HOST_MATH_ENTRY_POINTS.contains(&token.as_str()) {
                findings.push(Finding::new(
                    root,
                    &path,
                    line_of(&chars, start),
                    format!(
                        "`{token}(` computes through the host's math library, whose last bit \
                         differs between hosts; route it through `client::smog::math` (Q0014, \
                         plan 2026-11-04-libm-smog-math)"
                    ),
                ));
            }
        }
    }
    Ok(findings)
}

/// Whether the first character at or after `index` that is not whitespace is `expected`.
fn next_non_space_is(chars: &[char], index: usize, expected: char) -> bool {
    chars[index.min(chars.len())..]
        .iter()
        .find(|c| !c.is_whitespace())
        .is_some_and(|&c| c == expected)
}

/// Whether the token starting at `start` names a float method: preceded, after whitespace, by
/// `.` (a method call) or by `f32::` or `f64::` (a path call). `math::sin(` is the allowed route
/// and is not reported.
fn follows_path(chars: &[char], start: usize) -> bool {
    let mut tail: Vec<char> = chars[..start.min(chars.len())]
        .iter()
        .rev()
        .filter(|c| !c.is_whitespace())
        .take(5)
        .copied()
        .collect();
    tail.reverse();
    let tail: String = tail.into_iter().collect();
    tail.ends_with('.') || tail.ends_with("f32::") || tail.ends_with("f64::")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slash_path;
    use std::error::Error;
    use std::fs;

    /// A temporary repository with the given files.
    fn tree(files: &[(&str, &str)]) -> Result<tempfile::TempDir, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        for (path, text) in files {
            let full = dir.path().join(path);
            fs::create_dir_all(full.parent().ok_or("file has a parent")?)?;
            fs::write(full, text)?;
        }
        Ok(dir)
    }

    fn lines(findings: &[Finding]) -> Vec<String> {
        findings
            .iter()
            .map(|f| format!("{}:{}", slash_path(&f.path), f.line))
            .collect()
    }

    #[test]
    fn every_banned_method_and_entry_point_is_reported() -> Result<(), Box<dyn Error>> {
        let methods = HOST_MATH_METHODS.map(|name| format!("    let _a = x.{name}(x);\n"));
        let entries = HOST_MATH_ENTRY_POINTS.map(|name| format!("    let _b = Quat::{name}(x);\n"));
        let source = format!(
            "//! x\nfn f(x: f32) {{\n{}{}}}\n",
            methods.concat(),
            entries.concat()
        );
        let dir = tree(&[("crates/client/src/smog/fluid.rs", &source)])?;
        let findings = host_math_in_smog(dir.path())?;
        let expected: Vec<String> = (0..HOST_MATH_METHODS.len() + HOST_MATH_ENTRY_POINTS.len())
            .map(|i| format!("crates/client/src/smog/fluid.rs:{}", i + 3))
            .collect();
        assert_eq!(lines(&findings), expected, "{findings:?}");
        assert!(findings[0].message.contains("client::smog::math"));
        Ok(())
    }

    #[test]
    fn path_calls_and_spaced_calls_are_reported() -> Result<(), Box<dyn Error>> {
        let dir = tree(&[(
            "crates/client/src/smog/mod.rs",
            "//! x\nfn f(x: f32) -> f32 {\n    f32::exp(x) + x\n        .sin ()\n}\n",
        )])?;
        assert_eq!(
            lines(&host_math_in_smog(dir.path())?),
            [
                "crates/client/src/smog/mod.rs:3",
                "crates/client/src/smog/mod.rs:4"
            ]
        );
        Ok(())
    }

    #[test]
    fn comments_tests_math_file_other_dirs_and_non_calls_pass() -> Result<(), Box<dyn Error>> {
        let dir = tree(&[
            (
                "crates/client/src/smog/kinematics.rs",
                "//! x.sin(1.0)\n// y.exp(2.0)\nfn sin(x: f32) -> f32 { math::sin(x) }\n\
                 fn g(v: Vec3) -> f32 { let s = \"a.cos(b)\"; v.length() + sin(1.0) + v.exp }\n\
                 #[cfg(test)]\nmod tests {\n    fn t() { 1.0_f32.sin(); }\n}\n",
            ),
            (
                "crates/client/src/smog/math.rs",
                "//! x\npub fn sin(x: f32) -> f32 { x.sin() }\n",
            ),
            (
                "crates/client/src/camera.rs",
                "//! x\nfn f(x: f32) -> f32 { x.sin() }\n",
            ),
            (
                "crates/client/tests/smog_golden_tests.rs",
                "//! x\nfn f(x: f32) -> f32 { x.sin() }\n",
            ),
        ])?;
        let findings = host_math_in_smog(dir.path())?;
        assert!(findings.is_empty(), "{findings:?}");
        Ok(())
    }
}
