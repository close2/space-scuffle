//! Reading the resolved dependency graph from `cargo metadata`, and check C2 on it.
//!
//! # Responsibility
//! Runs `cargo metadata` for the workspace and answers graph questions about the result, in
//! particular whether a simulation crate can reach an engine, GPU, window or audio package
//! through normal (non-dev, non-build) dependencies.
//!
//! # How it fits
//! Charter §3.1 forbids such dependencies in `sim_types`, `sim_map` and `sim_core`. Reading
//! Cargo's own resolve graph rather than the manifests catches transitive dependencies, and the
//! graph covers every target platform, not only the host's.
//!
//! # Invariants
//! - `cargo metadata` runs with `--locked`, so a check never rewrites `Cargo.lock`.
//! - It does **not** run with `--offline`. Unlike a build, `cargo metadata` reads the manifest
//!   of every package in the lock file, for every platform (for example `windows` crates on a
//!   Linux host), and a build never downloads those. With `--offline` the check failed on a fresh
//!   checkout even with a warm build cache, so it may download the missing crates once.
//! - The analysis is a pure function of the JSON, so it is tested on small synthetic graphs.

use crate::{CheckError, Finding};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;
use std::process::Command;

/// Package-name prefixes that may not appear among a simulation crate's normal dependencies:
/// game engines, GPU and shader stacks, windowing, input and audio (charter §3.1).
pub const FORBIDDEN_SIMULATION_DEPENDENCIES: [&str; 12] = [
    "bevy", "wgpu", "winit", "egui", "naga", "ash", "gilrs", "cpal", "rodio", "alsa", "x11",
    "wayland",
];

/// Runs `cargo metadata --format-version 1 --locked` on the repository and parses its output.
///
/// With `no_deps` the output lists only workspace members and has no `resolve` graph. The Cargo
/// executable is the one running the tests (`$CARGO`), falling back to `cargo` on `PATH` when the
/// test binary is started by hand.
///
/// # Errors
/// [`CheckError::MetadataSpawn`] if Cargo cannot be started, [`CheckError::MetadataFailed`] if it
/// exits unsuccessfully (its standard error names the cause), [`CheckError::MetadataJson`] if
/// it prints something other than JSON.
pub fn cargo_metadata(no_deps: bool) -> Result<Value, CheckError> {
    let program = std::env::var("CARGO").unwrap_or_else(|_| String::from("cargo"));
    let mut command = Command::new(&program);
    command
        .arg("metadata")
        .args(["--format-version", "1", "--locked"])
        .arg("--manifest-path")
        .arg(crate::repo_root().join("Cargo.toml"));
    if no_deps {
        command.arg("--no-deps");
    }
    let output = command
        .output()
        .map_err(|source| CheckError::MetadataSpawn { program, source })?;
    if !output.status.success() {
        return Err(CheckError::MetadataFailed {
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

/// Check C2: the normal-dependency closure of each simulation crate holds no package whose name
/// starts with one of [`FORBIDDEN_SIMULATION_DEPENDENCIES`].
///
/// `metadata` is full `cargo metadata` output (with `resolve`). `roots` names the crates to
/// check. A finding is reported once per (root, forbidden package), on the line of the root's
/// `Cargo.toml` that declares the first dependency on the path to it, with the whole path.
///
/// # Errors
/// [`CheckError::MetadataShape`] if `packages`, `workspace_members` or `resolve.nodes` is missing,
/// or a root is not a workspace member; [`CheckError::Io`] if a root's manifest cannot be read.
pub fn engine_free_simulation(
    metadata: &Value,
    root: &Path,
    roots: &[&str],
) -> Result<Vec<Finding>, CheckError> {
    let names = package_names(metadata)?;
    let members: BTreeSet<&str> = metadata["workspace_members"]
        .as_array()
        .ok_or(CheckError::MetadataShape("workspace_members"))?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let graph = normal_edges(metadata)?;
    let mut findings = Vec::new();
    for crate_name in roots {
        let root_id = members
            .iter()
            .copied()
            .find(|id| names.get(id) == Some(crate_name))
            .ok_or(CheckError::MetadataShape(
                "a simulation crate among workspace_members",
            ))?;
        let manifest = root.join("crates").join(crate_name).join("Cargo.toml");
        let manifest_text =
            std::fs::read_to_string(&manifest).map_err(|source| CheckError::Io {
                path: manifest.clone(),
                source,
            })?;
        for path in forbidden_paths(root_id, &graph, &names) {
            let chain: Vec<&str> = path
                .iter()
                .map(|&id| names.get(id).copied().unwrap_or(id))
                .collect();
            let first_hop = chain.get(1).copied().unwrap_or(crate_name);
            let line = declaration_line(&manifest_text, first_hop);
            let culprit = chain.last().copied().unwrap_or(crate_name);
            findings.push(Finding::new(
                root,
                &manifest,
                line,
                format!(
                    "simulation crate depends on `{culprit}` via {} (charter §3.1: no engine, \
                     GPU, window or audio dependency)",
                    chain.join(" -> ")
                ),
            ));
        }
    }
    Ok(findings)
}

/// Package ID to package name, for every package in the metadata.
fn package_names(metadata: &Value) -> Result<BTreeMap<&str, &str>, CheckError> {
    metadata["packages"]
        .as_array()
        .ok_or(CheckError::MetadataShape("packages"))?
        .iter()
        .map(
            |package| match (package["id"].as_str(), package["name"].as_str()) {
                (Some(id), Some(name)) => Ok((id, name)),
                _ => Err(CheckError::MetadataShape(
                    "packages[].id or packages[].name",
                )),
            },
        )
        .collect()
}

/// For each resolved package, the packages it depends on through at least one normal edge
/// (`dep_kinds[].kind == null`), in sorted order.
fn normal_edges(metadata: &Value) -> Result<BTreeMap<&str, Vec<&str>>, CheckError> {
    let nodes = metadata["resolve"]["nodes"]
        .as_array()
        .ok_or(CheckError::MetadataShape("resolve.nodes"))?;
    let mut graph = BTreeMap::new();
    for node in nodes {
        let id = node["id"]
            .as_str()
            .ok_or(CheckError::MetadataShape("resolve.nodes[].id"))?;
        let mut targets: Vec<&str> = node["deps"]
            .as_array()
            .ok_or(CheckError::MetadataShape("resolve.nodes[].deps"))?
            .iter()
            .filter(|dep| {
                dep["dep_kinds"]
                    .as_array()
                    .is_some_and(|kinds| kinds.iter().any(|kind| kind["kind"].is_null()))
            })
            .filter_map(|dep| dep["pkg"].as_str())
            .collect();
        targets.sort_unstable();
        targets.dedup();
        graph.insert(id, targets);
    }
    Ok(graph)
}

/// Breadth-first search from `start`; returns the shortest path to every reachable forbidden
/// package, in the order they are found.
fn forbidden_paths<'a>(
    start: &'a str,
    graph: &BTreeMap<&'a str, Vec<&'a str>>,
    names: &BTreeMap<&'a str, &'a str>,
) -> Vec<Vec<&'a str>> {
    let mut parent: BTreeMap<&str, &str> = BTreeMap::new();
    let mut seen: BTreeSet<&str> = BTreeSet::from([start]);
    let mut queue = VecDeque::from([start]);
    let mut paths = Vec::new();
    while let Some(id) = queue.pop_front() {
        for &next in graph.get(id).map_or(&[][..], Vec::as_slice) {
            if !seen.insert(next) {
                continue;
            }
            parent.insert(next, id);
            queue.push_back(next);
            let name = names.get(next).copied().unwrap_or(next);
            if FORBIDDEN_SIMULATION_DEPENDENCIES
                .iter()
                .any(|prefix| name.starts_with(prefix))
            {
                let mut path = vec![next];
                let mut cursor = next;
                while let Some(&up) = parent.get(cursor) {
                    path.push(up);
                    cursor = up;
                }
                path.reverse();
                paths.push(path);
            }
        }
    }
    paths
}

/// One-based line of `manifest` that declares dependency `name`, or 1 if none does.
fn declaration_line(manifest: &str, name: &str) -> usize {
    manifest
        .lines()
        .position(|line| {
            let line = line.trim_start();
            line.strip_prefix(name)
                .is_some_and(|rest| rest.starts_with([' ', '=', '.']))
        })
        .map_or(1, |index| index + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::error::Error;
    use std::fs;

    /// A graph where `sim_core -> helper -> bevy_ecs` is normal and `sim_core -> wgpu` is dev-only.
    fn synthetic() -> Value {
        let normal = json!([{ "kind": null, "target": null }]);
        let dev = json!([{ "kind": "dev", "target": null }]);
        json!({
            "packages": [
                { "id": "core", "name": "sim_core" },
                { "id": "helper", "name": "helper" },
                { "id": "ecs", "name": "bevy_ecs" },
                { "id": "gpu", "name": "wgpu" },
            ],
            "workspace_members": ["core"],
            "resolve": { "nodes": [
                { "id": "core", "deps": [
                    { "pkg": "helper", "dep_kinds": normal },
                    { "pkg": "gpu", "dep_kinds": dev },
                ]},
                { "id": "helper", "deps": [ { "pkg": "ecs", "dep_kinds": normal } ] },
                { "id": "ecs", "deps": [] },
                { "id": "gpu", "deps": [] },
            ]},
        })
    }

    #[test]
    fn transitive_engine_dependency_is_reported_on_the_declaring_line() -> Result<(), Box<dyn Error>>
    {
        let tree = tempfile::tempdir()?;
        let dir = tree.path().join("crates/sim_core");
        fs::create_dir_all(&dir)?;
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"sim_core\"\n\n[dependencies]\nhelper.workspace = true\n",
        )?;
        let findings = engine_free_simulation(&synthetic(), tree.path(), &["sim_core"])?;
        assert_eq!(findings.len(), 1, "the dev-only wgpu edge is not reported");
        let text = findings[0].to_string();
        assert!(
            text.starts_with("crates/sim_core/Cargo.toml:5: "),
            "unexpected location: {text}"
        );
        assert!(text.contains("sim_core -> helper -> bevy_ecs"), "{text}");
        Ok(())
    }

    #[test]
    fn missing_resolve_graph_is_an_error_not_a_pass() -> Result<(), Box<dyn Error>> {
        let tree = tempfile::tempdir()?;
        let mut metadata = synthetic();
        metadata["resolve"] = Value::Null;
        let result = engine_free_simulation(&metadata, tree.path(), &["sim_core"]);
        assert!(matches!(result, Err(CheckError::MetadataShape(_))));
        Ok(())
    }
}
