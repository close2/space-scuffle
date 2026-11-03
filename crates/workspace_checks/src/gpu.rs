//! Check C21: no test builds a real render device in its own process (charter §7).
//!
//! # Responsibility
//! A test that builds a Bevy renderer with a real backend inside the test process picks a GPU
//! adapter with no driver selection and no GPU lock: Vulkan may pick the shared RX 580, and
//! without `WGPU_BACKEND` wgpu may even fall back to OpenGL on it. Such a test must instead run
//! its body as an `#[ignore]`d child launched through the client's GPU test harness
//! (`crates/client/tests/support/gpu_run.rs`, `run_test_child` or a `GpuCommand`), which selects
//! the driver and holds the lock only while the child runs, or build the renderer headless with
//! `WgpuSettings { backends: None, .. }`. Plan: `docs/plans/2026-11-03-gpu-test-harness.md`,
//! section 11.
//!
//! # How it fits
//! `tests/charter.rs` runs [`in_process_render_devices`] on the repository; the unit tests below
//! run it on planted trees.
//!
//! # Invariants
//! - Only test code is judged: files below `tests/`, test-named files, dev-only crates, and the
//!   `#[cfg(test)]` items of production files.
//! - A marker ([`RENDER_DEVICE_MARKERS`]) passes when `backends: None` follows within
//!   [`HEADLESS_WINDOW`] lines, when `HeadlessCheck` follows within [`HEADLESS_CHECK_WINDOW`] lines
//!   (the client refuses to start a renderer in that mode), or when the file uses `gpu_run` and
//!   the marker is in an `#[ignore]`d test (a launched child) or in a helper that is not a test.
//! - Limit: a helper is trusted to be called only by launched children; the check does not
//!   follow calls.

use crate::source::{line_of, word_tokens};
use crate::{code_text, rust_files, strip_test_items, CheckError, Finding, Scope};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Names that build an application with Bevy's renderer, and so a render device unless the
/// backends are switched off.
pub const RENDER_DEVICE_MARKERS: [&str; 6] = [
    "DefaultPlugins",
    "GamePresentationPlugins",
    "build_graphical_app",
    "build_graphical_app_off_main_thread",
    "build_graphical_app_with_options",
    "request_adapter",
];

/// Lines after a marker searched for `backends: None`.
pub const HEADLESS_WINDOW: usize = 15;

/// Lines after a marker searched for `HeadlessCheck`.
pub const HEADLESS_CHECK_WINDOW: usize = 3;

/// Lines above a function searched for its attributes.
const ATTRIBUTE_WINDOW: usize = 8;

/// Check C21: every render device in test code is built in a GPU child or headless.
///
/// # Errors
/// [`CheckError::Io`] if a directory or file cannot be read.
pub fn in_process_render_devices(root: &Path) -> Result<Vec<Finding>, CheckError> {
    let production: BTreeSet<PathBuf> = rust_files(root, Scope::Production)?.into_iter().collect();
    let mut findings = Vec::new();
    for path in rust_files(root, Scope::All)? {
        let code = code_text(&path, Scope::All)?;
        let judged = if production.contains(&path) {
            test_items_only(&code)
        } else {
            code.clone()
        };
        let lines: Vec<&str> = code.lines().collect();
        let uses_harness = word_tokens(&code.chars().collect::<Vec<_>>())
            .iter()
            .any(|(_, token)| token == "gpu_run");
        let chars: Vec<char> = judged.chars().collect();
        for (start, token) in word_tokens(&chars) {
            if !RENDER_DEVICE_MARKERS.contains(&token.as_str()) {
                continue;
            }
            let line = line_of(&chars, start);
            if marker_is_isolated(&lines, line, uses_harness) {
                continue;
            }
            findings.push(Finding::new(
                root,
                &path,
                line,
                format!(
                    "`{token}` builds a render device inside the test process, with no driver \
                     selection and no GPU lock (charter §7); run the test body as an `#[ignore]`d \
                     child through gpu_run's `run_test_child`, or build it headless with \
                     `backends: None`"
                ),
            ));
        }
    }
    Ok(findings)
}

/// Keeps only the text of `#[cfg(test)]` items: the characters [`strip_test_items`] blanks.
fn test_items_only(code: &str) -> String {
    code.chars()
        .zip(strip_test_items(code).chars())
        .map(|(original, stripped)| {
            if original == '\n' || stripped != original {
                original
            } else {
                ' '
            }
        })
        .collect()
}

/// Whether the marker on one-based `line` of `lines` is headless or inside a launched GPU child.
fn marker_is_isolated(lines: &[&str], line: usize, uses_harness: bool) -> bool {
    let index = line.saturating_sub(1);
    let text = lines.get(index).copied().unwrap_or_default();
    if text.trim_start().starts_with("use ") {
        return true;
    }
    let after = |window: usize| {
        lines
            .iter()
            .skip(index)
            .take(window + 1)
            .copied()
            .collect::<Vec<_>>()
            .join("\n")
    };
    if after(HEADLESS_WINDOW)
        .split_whitespace()
        .collect::<String>()
        .contains("backends:None")
    {
        return true;
    }
    if after(HEADLESS_CHECK_WINDOW).contains("HeadlessCheck") {
        return true;
    }
    if !uses_harness {
        return false;
    }
    match enclosing_attributes(lines, index) {
        Some(attributes) => !attributes.contains("#[test]") || attributes.contains("#[ignore"),
        None => true,
    }
}

/// The attribute text above the nearest function that starts at or before `index`, or `None` if
/// no function encloses it.
fn enclosing_attributes(lines: &[&str], index: usize) -> Option<String> {
    let function = (0..=index).rev().find(|&at| {
        let chars: Vec<char> = lines[at].chars().collect();
        word_tokens(&chars).iter().any(|(_, token)| token == "fn")
    })?;
    let mut attributes = Vec::new();
    for at in (function.saturating_sub(ATTRIBUTE_WINDOW)..function).rev() {
        let trimmed = lines[at].trim_end();
        if trimmed.ends_with('}') || trimmed.ends_with(';') {
            break;
        }
        attributes.push(lines[at]);
    }
    Some(attributes.join("\n"))
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
    fn in_process_devices_fail_and_children_and_headless_apps_pass() -> Result<(), Box<dyn Error>> {
        let dir = tree(&[
            (
                "crates/client/tests/bad_tests.rs",
                "//! x\nuse bevy::prelude::DefaultPlugins;\n#[test]\nfn t() {\n    \
                 App::new().add_plugins(GamePresentationPlugins);\n}\n",
            ),
            (
                "crates/client/tests/headless_tests.rs",
                "//! x\n#[test]\nfn t() {\n    App::new().add_plugins(DefaultPlugins.set(RenderPlugin {\n        \
                 render_creation: RenderCreation::Automatic(WgpuSettings {\n            \
                 backends: None,\n            ..default()\n        }),\n    }));\n}\n",
            ),
            (
                "crates/client/tests/child_tests.rs",
                "//! x\nmod gpu_run;\n#[test]\nfn parent() {\n    gpu_run::run_test_child(\"child\", \"c\");\n}\n\
                 #[test]\n#[ignore = \"launched\"]\nfn child() {\n    \
                 App::new().add_plugins(GamePresentationPlugins);\n}\n\
                 #[test]\nfn unlaunched() {\n    App::new().add_plugins(DefaultPlugins);\n}\n",
            ),
            (
                "crates/client/tests/refusal_tests.rs",
                "//! x\n#[test]\nfn t() {\n    let r = client::build_graphical_app(\n        \
                 client::ClientMode::HeadlessCheck,\n    );\n}\n",
            ),
            (
                "crates/client/src/lib.rs",
                "//! x\npub fn app() { App::new().add_plugins(DefaultPlugins); }\n\
                 #[cfg(test)]\nmod tests {\n    #[test]\n    fn t() { \
                 App::new().add_plugins(DefaultPlugins); }\n}\n",
            ),
        ])?;
        let findings = in_process_render_devices(dir.path())?;
        assert_eq!(
            lines(&findings),
            [
                "crates/client/src/lib.rs:6",
                "crates/client/tests/bad_tests.rs:5",
                "crates/client/tests/child_tests.rs:14",
            ],
            "{findings:?}"
        );
        assert!(findings[0].message.contains("run_test_child"));
        Ok(())
    }
}
