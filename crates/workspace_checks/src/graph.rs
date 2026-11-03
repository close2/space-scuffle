//! Check C1: the dependency graph drawn in charter §5 is the one Cargo builds.
//!
//! # Responsibility
//! Reads the workspace-internal dependency edges from `cargo metadata --no-deps` and the edges of
//! the Mermaid flowchart in `AGENTS.md` §5, and reports every difference: an edge Cargo has that
//! the chart does not draw, an edge the chart draws that Cargo does not have, a workspace crate
//! with no node, and a node naming no workspace crate.
//!
//! # How it fits
//! Charter §5 is the map a reader uses to understand which crate may know about which. Before
//! Phase 9 it had drifted (finding Q-12: `asset_baker → sim_core` and two whole crates were
//! missing). Comparing it with Cargo's manifests on every test run keeps the map true.
//!
//! # Invariants
//! - A normal dependency is drawn `A --> B` and a dev-dependency `A -.-> B`, with the arrow
//!   pointing from the dependency to its dependent. A build-dependency counts as normal.
//! - When a crate is both a normal and a dev-dependency of the same crate, only the normal edge
//!   is expected: the normal edge already makes it available to tests.
//! - A node maps to a crate by the `crates/<name>` text that starts its label.

use crate::markdown::{self, MarkdownLine};
use crate::{CheckError, Finding};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

/// The charter file, relative to the repository root.
const CHARTER: &str = "AGENTS.md";

/// How one crate depends on another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum EdgeKind {
    /// A normal (or build) dependency, drawn `-->`.
    Normal,
    /// A dev-dependency only, drawn `-.->`.
    Dev,
}

impl fmt::Display for EdgeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Normal => "-->",
            Self::Dev => "-.->",
        })
    }
}

/// One edge: (dependency, dependent, kind).
type Edge = (String, String, EdgeKind);

/// Check C1: the Mermaid chart in `AGENTS.md` §5 has exactly the workspace-internal edges of
/// `metadata`, which is `cargo metadata --no-deps` output for the repository at `root`.
///
/// # Errors
/// [`CheckError::Io`] if `AGENTS.md` cannot be read; [`CheckError::MetadataShape`] if the
/// metadata lacks `packages`, a package name or a dependency list.
pub fn charter_graph(root: &Path, metadata: &Value) -> Result<Vec<Finding>, CheckError> {
    let path = root.join(CHARTER);
    let text = std::fs::read_to_string(&path).map_err(|source| CheckError::Io {
        path: path.clone(),
        source,
    })?;
    let (members, cargo) = cargo_edges(metadata)?;
    let all = markdown::lines(&text);
    let chart = Chart::parse(&markdown::section(&all, "5"));
    let mut findings = Vec::new();
    let mut report = |line: usize, message: String| {
        findings.push(Finding::new(root, &path, line, message));
    };
    let Some(fence_line) = chart.fence_line else {
        report(
            1,
            "§5 has no ```mermaid block drawing the crate graph".to_string(),
        );
        return Ok(findings);
    };
    for (line, message) in &chart.problems {
        report(*line, message.clone());
    }
    for member in &members {
        if !chart.nodes.values().any(|(name, _)| name == member) {
            report(
                fence_line,
                format!("workspace crate `{member}` has no node in §5"),
            );
        }
    }
    for (name, line) in chart.nodes.values() {
        if !members.contains(name) {
            report(
                *line,
                format!("node names `crates/{name}`, which is not a workspace crate"),
            );
        }
    }
    for edge in &cargo {
        if !chart.edges.contains_key(edge) {
            let (from, to, kind) = edge;
            report(
                fence_line,
                format!("Cargo has `{from} {kind} {to}`, which §5 does not draw (charter §5)"),
            );
        }
    }
    for (edge, line) in &chart.edges {
        if !cargo.contains(edge) {
            let (from, to, kind) = edge;
            report(
                *line,
                format!("§5 draws `{from} {kind} {to}`, which Cargo does not have (charter §5)"),
            );
        }
    }
    Ok(findings)
}

/// Workspace member names and their internal edges, from `cargo metadata --no-deps`.
fn cargo_edges(metadata: &Value) -> Result<(BTreeSet<String>, BTreeSet<Edge>), CheckError> {
    let packages = metadata["packages"]
        .as_array()
        .ok_or(CheckError::MetadataShape("packages"))?;
    let mut members = BTreeSet::new();
    let mut kinds: BTreeMap<(String, String), EdgeKind> = BTreeMap::new();
    for package in packages {
        let name = package["name"]
            .as_str()
            .ok_or(CheckError::MetadataShape("packages[].name"))?;
        members.insert(name.to_string());
        let dependencies = package["dependencies"]
            .as_array()
            .ok_or(CheckError::MetadataShape("packages[].dependencies"))?;
        for dependency in dependencies.iter().filter(|dep| dep["path"].is_string()) {
            let from = dependency["name"]
                .as_str()
                .ok_or(CheckError::MetadataShape("packages[].dependencies[].name"))?;
            let kind = if dependency["kind"].as_str() == Some("dev") {
                EdgeKind::Dev
            } else {
                EdgeKind::Normal
            };
            let entry = kinds
                .entry((from.to_string(), name.to_string()))
                .or_insert(kind);
            *entry = (*entry).min(kind);
        }
    }
    let edges = kinds
        .into_iter()
        .map(|((from, to), kind)| (from, to, kind))
        .collect();
    Ok((members, edges))
}

/// The crate graph as §5 draws it.
#[derive(Debug, Default)]
struct Chart {
    /// Line of the fence that opens the Mermaid block, if the section has one.
    fence_line: Option<usize>,
    /// Node ID to (crate name, line of its definition).
    nodes: BTreeMap<String, (String, usize)>,
    /// Drawn edges, with the line each is drawn on.
    edges: BTreeMap<Edge, usize>,
    /// Lines of the block that could not be read, with the reason.
    problems: Vec<(usize, String)>,
}

impl Chart {
    /// Reads the first Mermaid block of `section`.
    fn parse(section: &[MarkdownLine<'_>]) -> Self {
        let mut chart = Self::default();
        let Some(start) = section
            .iter()
            .position(|line| line.text.trim_start().starts_with("```mermaid"))
        else {
            return chart;
        };
        chart.fence_line = Some(section[start].number);
        let body: Vec<&MarkdownLine<'_>> = section[start + 1..]
            .iter()
            .take_while(|line| !line.text.trim_start().starts_with("```"))
            .collect();
        for line in &body {
            if let Some((id, name)) = node_definition(line.text) {
                chart.nodes.insert(id, (name, line.number));
            }
        }
        for line in &body {
            chart.read_edge(line);
        }
        chart
    }

    /// Records the edge drawn on `line`, if any; reports an edge between unknown nodes.
    fn read_edge(&mut self, line: &MarkdownLine<'_>) {
        let text = line.text.trim();
        let (kind, arrow) = if text.contains("-.->") {
            (EdgeKind::Dev, "-.->")
        } else if text.contains("-->") {
            (EdgeKind::Normal, "-->")
        } else {
            return;
        };
        let Some((from, to)) = text.split_once(arrow) else {
            return;
        };
        let names = (self.nodes.get(from.trim()), self.nodes.get(to.trim()));
        match names {
            (Some((from, _)), Some((to, _))) => {
                self.edges
                    .insert((from.clone(), to.clone(), kind), line.number);
            }
            _ => self.problems.push((
                line.number,
                format!("edge `{text}` names a node that is not defined in the block"),
            )),
        }
    }
}

/// `(id, crate)` for a node definition `ID["crates/name<br/>..."]`.
fn node_definition(line: &str) -> Option<(String, String)> {
    let (id, label) = line.trim().split_once("[\"crates/")?;
    let name: String = label
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    let id = id.trim();
    (!id.is_empty() && !name.is_empty()).then(|| (id.to_string(), name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slash_path;
    use serde_json::json;
    use std::error::Error;
    use std::fs;

    /// Metadata where `b` depends on `a` normally and on `c` as a dev-dependency only, and `d`
    /// on `a` both ways.
    fn metadata() -> Value {
        let path = json!("/repo/crates/x");
        json!({ "packages": [
            { "name": "a", "dependencies": [ { "name": "serde", "kind": null } ] },
            { "name": "b", "dependencies": [
                { "name": "a", "kind": null, "path": path },
                { "name": "c", "kind": "dev", "path": path },
            ]},
            { "name": "c", "dependencies": [] },
            { "name": "d", "dependencies": [
                { "name": "a", "kind": "dev", "path": path },
                { "name": "a", "kind": null, "path": path },
            ]},
        ]})
    }

    fn charter(chart: &str) -> Result<tempfile::TempDir, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        fs::write(
            dir.path().join(CHARTER),
            format!(
                "# Charter\n\n## 5. Graph\n\n```mermaid\nflowchart TD\n{chart}```\n\n## 6. Next\n"
            ),
        )?;
        Ok(dir)
    }

    fn rendered(findings: &[Finding]) -> Vec<String> {
        findings
            .iter()
            .map(|f| format!("{}:{}: {}", slash_path(&f.path), f.line, f.message))
            .collect()
    }

    const NODES: &str =
        "    A[\"crates/a<br/>(x)\"]\n    B[\"crates/b\"]\n    C[\"crates/c\"]\n    \
                         D[\"crates/d\"]\n";

    #[test]
    fn a_chart_equal_to_cargo_passes() -> Result<(), Box<dyn Error>> {
        let repo = charter(&format!("{NODES}    A --> B\n    C -.-> B\n    A --> D\n"))?;
        assert_eq!(charter_graph(repo.path(), &metadata())?, Vec::new());
        Ok(())
    }

    #[test]
    fn missing_extra_and_wrong_kind_edges_are_all_reported() -> Result<(), Box<dyn Error>> {
        let repo = charter(&format!("{NODES}    A --> B\n    C --> B\n    B --> C\n"))?;
        let found = rendered(&charter_graph(repo.path(), &metadata())?);
        assert_eq!(
            found,
            [
                "AGENTS.md:5: Cargo has `a --> d`, which §5 does not draw (charter §5)",
                "AGENTS.md:5: Cargo has `c -.-> b`, which §5 does not draw (charter §5)",
                "AGENTS.md:13: §5 draws `b --> c`, which Cargo does not have (charter §5)",
                "AGENTS.md:12: §5 draws `c --> b`, which Cargo does not have (charter §5)",
            ]
        );
        Ok(())
    }

    #[test]
    fn missing_nodes_unknown_crates_and_undefined_ids_are_reported() -> Result<(), Box<dyn Error>> {
        let chart = "    A[\"crates/a\"]\n    B[\"crates/b\"]\n    Z[\"crates/zz\"]\n    \
                     A --> B\n    Q --> B\n";
        let repo = charter(chart)?;
        let found = rendered(&charter_graph(repo.path(), &metadata())?);
        for expected in [
            "AGENTS.md:11: edge `Q --> B` names a node that is not defined in the block",
            "AGENTS.md:5: workspace crate `c` has no node in §5",
            "AGENTS.md:5: workspace crate `d` has no node in §5",
            "AGENTS.md:9: node names `crates/zz`, which is not a workspace crate",
        ] {
            assert!(
                found.iter().any(|line| line == expected),
                "{expected} in {found:#?}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_section_without_a_chart_is_a_finding() -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        fs::write(dir.path().join(CHARTER), "## 5. Graph\n\nNo chart.\n")?;
        assert_eq!(charter_graph(dir.path(), &metadata())?.len(), 1);
        Ok(())
    }
}
