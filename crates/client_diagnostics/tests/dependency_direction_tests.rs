//! Keeps the workspace dependency between the client and its diagnostics one-way.
//!
//! Cargo rejects a normal dependency cycle but accepts a dev-dependency back-edge, so the manifest
//! itself is checked. Production code is scanned as well, because audit hooks must stay out of it.

use std::path::{Path, PathBuf};

fn rust_files(directory: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            rust_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

#[test]
fn client_manifest_never_names_its_diagnostics() -> std::io::Result<()> {
    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../client/Cargo.toml"))?;
    assert!(!manifest.contains("client_diagnostics"));
    Ok(())
}

#[test]
fn client_source_holds_no_audit_hooks() -> std::io::Result<()> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../client/src");
    let mut files = Vec::new();
    rust_files(&source, &mut files)?;
    assert!(!files.is_empty());
    for file in files {
        let text = std::fs::read_to_string(&file)?;
        for forbidden in [
            "client_diagnostics",
            "ControlledFixture",
            "shell_order_audit",
            "runtime_capture",
        ] {
            assert!(
                !text.contains(forbidden),
                "{} names {forbidden}",
                file.display()
            );
        }
    }
    Ok(())
}
