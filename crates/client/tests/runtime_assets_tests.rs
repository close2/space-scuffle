//! Runtime asset-root discovery and real binary startup failure contracts.
//!
//! Temporary trees test path selection only; the repository integration reads the real
//! shader marker, and the subprocess test proves invalid configuration fails before GPU setup.

use client::assets::{
    discover_assets_dir, texture_assets_dir, AssetsDirError, InvalidAssetsRoot, ResolvedAssetsRoot,
};
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn make_root(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(path.join("shaders"))?;
    // Only the marker's filesystem identity is under test, not shader compilation.
    std::fs::write(path.join("shaders/bubble_dome.wgsl"), [])?;
    std::fs::canonicalize(path)
}

#[test]
fn explicit_valid_root_is_canonical_and_authoritative() -> TestResult {
    let temporary = tempfile::tempdir()?;
    let explicit = make_root(&temporary.path().join("chosen"))?;
    let discovered = temporary.path().join("automatic");
    make_root(&discovered.join("assets"))?;
    let resolved = discover_assets_dir(Some(&explicit.join(".")), &[discovered])?;
    assert_eq!(resolved, explicit);
    assert!(resolved.is_absolute());
    Ok(())
}

#[test]
fn invalid_override_never_falls_back_to_valid_search_root() -> TestResult {
    let temporary = tempfile::tempdir()?;
    make_root(&temporary.path().join("assets"))?;
    let missing = temporary.path().join("missing");
    assert!(matches!(
        discover_assets_dir(Some(&missing), &[temporary.path().to_path_buf()]),
        Err(AssetsDirError::InvalidRoot {
            reason: InvalidAssetsRoot::MissingDirectory,
            path,
        }) if path == missing
    ));
    Ok(())
}

#[test]
fn invalid_root_and_marker_types_are_rejected() -> TestResult {
    let temporary = tempfile::tempdir()?;
    let file = temporary.path().join("file");
    std::fs::write(&file, [])?;
    let empty = temporary.path().join("empty");
    std::fs::create_dir(&empty)?;
    let directory_marker = temporary.path().join("directory_marker");
    std::fs::create_dir_all(directory_marker.join("shaders/bubble_dome.wgsl"))?;
    for (path, expected) in [
        (file, InvalidAssetsRoot::NotDirectory),
        (empty, InvalidAssetsRoot::MissingMarker),
        (directory_marker, InvalidAssetsRoot::MarkerNotFile),
    ] {
        assert!(matches!(
            discover_assets_dir(Some(&path), &[]),
            Err(AssetsDirError::InvalidRoot { reason, .. }) if reason == expected
        ));
    }
    Ok(())
}

#[test]
fn discovery_skips_stale_nearby_assets_and_texture_loader_keeps_checked_root() -> TestResult {
    let temporary = tempfile::tempdir()?;
    let valid = make_root(&temporary.path().join("assets"))?;
    let nested = temporary.path().join("nested/client");
    std::fs::create_dir_all(nested.join("assets"))?;

    let discovered = discover_assets_dir(None, std::slice::from_ref(&nested))?;
    assert_eq!(discovered, valid);
    let resource = ResolvedAssetsRoot::from_directory(&discovered)?;
    assert_eq!(Path::new(resource.bevy_file_path()?), resource.as_path());
    assert_eq!(texture_assets_dir(Some(&resource)), valid.join("textures"));
    assert_ne!(
        texture_assets_dir(Some(&resource)),
        nested.join("assets/textures")
    );
    Ok(())
}

#[test]
fn search_root_precedence_and_ancestry_are_deterministic() -> TestResult {
    let temporary = tempfile::tempdir()?;
    let first = temporary.path().join("first");
    let second = temporary.path().join("second");
    let first_assets = make_root(&first.join("assets"))?;
    let second_assets = make_root(&second.join("assets"))?;
    assert_eq!(
        discover_assets_dir(None, &[first.join("crate"), second.clone()])?,
        first_assets
    );
    assert_eq!(
        discover_assets_dir(None, &[second.join("nested/working/directory")])?,
        second_assets
    );
    Ok(())
}

#[test]
fn missing_root_reports_search_context() -> TestResult {
    let temporary = tempfile::tempdir()?;
    let error = discover_assets_dir(None, &[temporary.path().to_path_buf()]);
    assert!(matches!(
        error,
        Err(AssetsDirError::NotFound { searched })
            if searched.first() == Some(&temporary.path().join("assets"))
    ));
    Ok(())
}

#[test]
fn real_repository_root_contains_readable_runtime_shader() -> TestResult {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let resolved = discover_assets_dir(None, std::slice::from_ref(&manifest))?;
    let expected = std::fs::canonicalize(manifest.join("../../assets"))?;
    assert_eq!(resolved, expected);
    let shader = std::fs::read_to_string(resolved.join("shaders/bubble_dome.wgsl"))?;
    assert_ne!(shader, "");
    Ok(())
}

#[test]
fn actual_game_rejects_invalid_override_before_graphics_startup() -> TestResult {
    let temporary = tempfile::tempdir()?;
    let invalid = temporary.path().join("absent-assets");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_client"))
        .args(["--battle", "--max-frames", "1"])
        .env("SPACE_SCUFFLE_ASSETS", &invalid)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()?;
    assert!(!output.status.success());
    let diagnostic = String::from_utf8(output.stderr)?;
    let expected = client::ClientStartupError::Assets(AssetsDirError::InvalidRoot {
        path: invalid,
        reason: InvalidAssetsRoot::MissingDirectory,
    });
    assert_eq!(
        diagnostic.trim(),
        format!("Client startup failed: {expected}")
    );
    assert!(diagnostic.contains("absent-assets"), "{diagnostic}");
    assert!(diagnostic.contains("does not exist"), "{diagnostic}");
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlink_root_resolves_to_canonical_directory() -> TestResult {
    let temporary = tempfile::tempdir()?;
    let expected = make_root(&temporary.path().join("real"))?;
    let link = temporary.path().join("linked");
    std::os::unix::fs::symlink(&expected, &link)?;
    assert_eq!(discover_assets_dir(Some(&link), &[])?, expected);
    Ok(())
}

#[cfg(unix)]
#[test]
fn non_unicode_root_is_preserved_and_never_lossily_passed_to_bevy() -> TestResult {
    use std::os::unix::ffi::OsStringExt;

    let temporary = tempfile::tempdir()?;
    let path = temporary
        .path()
        .join(std::ffi::OsString::from_vec(vec![0xff]));
    let expected = make_root(&path)?;
    let resolved = ResolvedAssetsRoot::from_directory(&path)?;
    assert_eq!(resolved.as_path(), expected);
    assert!(matches!(
        resolved.bevy_file_path(),
        Err(AssetsDirError::NonUnicodePath { .. })
    ));
    Ok(())
}
