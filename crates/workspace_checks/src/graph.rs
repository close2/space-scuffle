//! Check C1: the dependency graph `DESIGN.md` draws is the one Cargo builds, and Cargo's edges
//! follow the layers of charter §5.
//!
//! # Responsibility
//! Reads the workspace-internal dependency edges from `cargo metadata --no-deps` and compares
//! them with two documents:
//! - the Mermaid flowchart in `DESIGN.md` §2, reporting every difference: an edge Cargo has that
//!   the chart does not draw, an edge the chart draws that Cargo does not have, a workspace crate
//!   with no node, and a node naming no workspace crate;
//! - the three crate lists of `AGENTS.md` §5, reporting every edge that breaks them: an edge
//!   between two **Layers** crates that does not point forward in their order, a normal
//!   dependency on a **Test support** crate, any workspace edge of a **Standalone** crate, and a
//!   crate that is in no list or in more than one.
//!
//! # How it fits
//! Charter §5 is the rule a reader uses to know which crate may know about which; `DESIGN.md` §2
//! is the full map. Before Phase 9 the map had drifted (finding Q-12: `asset_baker → sim_core`
//! and two whole crates were missing). Comparing both with Cargo's manifests on every test run
//! keeps the map true and the rule kept.
//!
//! # Invariants
//! - A normal dependency is drawn `A --> B` and a dev-dependency `A -.-> B`, with the arrow
//!   pointing from the dependency to its dependent. A build-dependency counts as normal.
//! - When a crate is both a normal and a dev-dependency of the same crate, only the normal edge
//!   is expected: the normal edge already makes it available to tests.
//! - A node maps to a crate by the `crates/<name>` text that starts its label.
//! - A §5 list is the code spans on the one line that starts with its marker.

use crate::design_doc::DESIGN_DOC;
use crate::markdown::{self, MarkdownLine};
use crate::{CheckError, Finding};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

/// The charter file, relative to the repository root.
const CHARTER: &str = "AGENTS.md";

/// The section of `DESIGN.md` that draws the crate graph.
const GRAPH_SECTION: &str = "2";

/// The section of the charter that states the layers.
const LAYER_SECTION: &str = "5";

/// The markers that start charter §5's three crate lists, in the order [`Layers`] holds them.
const LIST_MARKERS: [&str; 3] = ["- **Layers:**", "- **Test support:**", "- **Standalone:**"];

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

/// Check C1: the Mermaid chart in `DESIGN.md` §2 has exactly the workspace-internal edges of
/// `metadata`, which is `cargo metadata --no-deps` output for the repository at `root`, and
/// those edges follow the crate lists of `AGENTS.md` §5.
///
/// # Errors
/// [`CheckError::Io`] if `DESIGN.md` or `AGENTS.md` cannot be read;
/// [`CheckError::MetadataShape`] if the metadata lacks `packages`, a package name or a
/// dependency list.
pub fn charter_graph(root: &Path, metadata: &Value) -> Result<Vec<Finding>, CheckError> {
    let (members, cargo) = cargo_edges(metadata)?;
    let mut findings = chart_findings(root, &members, &cargo)?;
    findings.extend(layer_findings(root, &members, &cargo)?);
    Ok(findings)
}

/// Reads `name` under `root`, returning its path and text.
fn read(root: &Path, name: &str) -> Result<(PathBuf, String), CheckError> {
    let path = root.join(name);
    let text = std::fs::read_to_string(&path).map_err(|source| CheckError::Io {
        path: path.clone(),
        source,
    })?;
    Ok((path, text))
}

/// C1 findings of the `DESIGN.md` §2 chart against Cargo's members and edges.
fn chart_findings(
    root: &Path,
    members: &BTreeSet<String>,
    cargo: &BTreeSet<Edge>,
) -> Result<Vec<Finding>, CheckError> {
    let (path, text) = read(root, DESIGN_DOC)?;
    let all = markdown::lines(&text);
    let chart = Chart::parse(&markdown::section(&all, GRAPH_SECTION));
    let mut findings = Vec::new();
    let mut report = |line: usize, message: String| {
        findings.push(Finding::new(root, &path, line, message));
    };
    let Some(fence_line) = chart.fence_line else {
        report(
            1,
            "§2 has no ```mermaid block drawing the crate graph".to_string(),
        );
        return Ok(findings);
    };
    for (line, message) in &chart.problems {
        report(*line, message.clone());
    }
    for member in members {
        if !chart.nodes.values().any(|(name, _)| name == member) {
            report(
                fence_line,
                format!("workspace crate `{member}` has no node in §2"),
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
    for edge in cargo {
        if !chart.edges.contains_key(edge) {
            let (from, to, kind) = edge;
            report(
                fence_line,
                format!("Cargo has `{from} {kind} {to}`, which §2 does not draw (C1)"),
            );
        }
    }
    for (edge, line) in &chart.edges {
        if !cargo.contains(edge) {
            let (from, to, kind) = edge;
            report(
                *line,
                format!("§2 draws `{from} {kind} {to}`, which Cargo does not have (C1)"),
            );
        }
    }
    Ok(findings)
}

/// One crate list of charter §5: the line it is on and its crates, in order.
type CrateList = (usize, Vec<String>);

/// The three crate lists of charter §5, in the order of [`LIST_MARKERS`].
struct Layers {
    /// The ordered layers.
    order: CrateList,
    /// Crates that are never a normal dependency.
    test_support: CrateList,
    /// Crates with no workspace edge at all.
    standalone: CrateList,
}

impl Layers {
    /// Reads the lists from §5's lines, or names the first marker §5 lacks.
    fn parse(section: &[MarkdownLine<'_>]) -> Result<Self, &'static str> {
        let list = |marker: &'static str| {
            section
                .iter()
                .find(|line| !line.fenced && line.text.trim_start().starts_with(marker))
                .map(|line| {
                    let crates = markdown::code_spans(line.text)
                        .into_iter()
                        .map(str::to_string)
                        .collect();
                    (line.number, crates)
                })
                .ok_or(marker)
        };
        let [layers, support, standalone] = LIST_MARKERS;
        Ok(Self {
            order: list(layers)?,
            test_support: list(support)?,
            standalone: list(standalone)?,
        })
    }

    /// The three lists, with the marker of each.
    fn lists(&self) -> [(&'static str, &CrateList); 3] {
        let [layers, support, standalone] = LIST_MARKERS;
        [
            (layers, &self.order),
            (support, &self.test_support),
            (standalone, &self.standalone),
        ]
    }
}

/// C1 findings of Cargo's members and edges against the crate lists of charter §5.
fn layer_findings(
    root: &Path,
    members: &BTreeSet<String>,
    cargo: &BTreeSet<Edge>,
) -> Result<Vec<Finding>, CheckError> {
    let (path, text) = read(root, CHARTER)?;
    let all = markdown::lines(&text);
    let section = markdown::section(&all, LAYER_SECTION);
    let heading = section.first().map_or(1, |line| line.number);
    let mut findings = Vec::new();
    let mut report = |line: usize, message: String| {
        findings.push(Finding::new(root, &path, line, message));
    };
    let layers = match Layers::parse(&section) {
        Ok(layers) => layers,
        Err(marker) => {
            report(heading, format!("§5 has no `{marker}` list of crates (C1)"));
            return Ok(findings);
        }
    };
    for member in members {
        let homes = layers
            .lists()
            .iter()
            .filter(|(_, (_, crates))| crates.contains(member))
            .count();
        if homes != 1 {
            report(
                heading,
                format!("workspace crate `{member}` is in {homes} of §5's crate lists, not 1 (C1)"),
            );
        }
    }
    for (marker, (line, crates)) in layers.lists() {
        for name in crates.iter().filter(|name| !members.contains(*name)) {
            report(
                *line,
                format!("§5's `{marker}` list names `{name}`, which is not a workspace crate"),
            );
        }
    }
    for (from, to, kind) in cargo {
        if let Some(message) = layer_violation(&layers, from, to, *kind) {
            report(message.0, message.1);
        }
    }
    Ok(findings)
}

/// The line and message of the §5 rule the Cargo edge `from kind to` breaks, if it breaks one.
fn layer_violation(
    layers: &Layers,
    from: &str,
    to: &str,
    kind: EdgeKind,
) -> Option<(usize, String)> {
    let named = |list: &CrateList, name: &str| list.1.iter().position(|item| item == name);
    let edge = format!("`{from} {kind} {to}`");
    if named(&layers.standalone, from).is_some() || named(&layers.standalone, to).is_some() {
        return Some((
            layers.standalone.0,
            format!("Cargo has {edge}, but §5 makes a crate of it standalone (C1)"),
        ));
    }
    if kind == EdgeKind::Normal && named(&layers.test_support, from).is_some() {
        return Some((
            layers.test_support.0,
            format!("Cargo has {edge}, a normal dependency on a test-support crate of §5 (C1)"),
        ));
    }
    match (named(&layers.order, from), named(&layers.order, to)) {
        (Some(dependency), Some(dependent)) if dependency >= dependent => Some((
            layers.order.0,
            format!("Cargo has {edge}, but §5's layers put `{from}` after `{to}` (C1)"),
        )),
        _ => None,
    }
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

/// The crate graph as `DESIGN.md` §2 draws it.
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

    /// §5 lists that [`metadata`] keeps.
    const LAYERS: &str = "- **Layers:** `a` → `b` → `d`.\n- **Test support:** `c`.\n\
                          - **Standalone:** none.\n";

    /// A repository whose `DESIGN.md` §2 draws `chart` and whose charter §5 holds `layers`.
    fn write_repo(chart: &str, layers: &str) -> Result<tempfile::TempDir, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        fs::write(
            dir.path().join(DESIGN_DOC),
            format!(
                "# Design\n\n## 2. Architecture\n\n```mermaid\nflowchart TD\n{chart}```\n\n\
                 ## 3. Next\n"
            ),
        )?;
        fs::write(
            dir.path().join(CHARTER),
            format!("# Charter\n\n## 5. Graph\n\n{layers}\n## 6. Next\n"),
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

    const EQUAL_CHART: &str = "    A --> B\n    C -.-> B\n    A --> D\n";

    #[test]
    fn a_chart_equal_to_cargo_and_kept_layers_pass() -> Result<(), Box<dyn Error>> {
        let repo = write_repo(&format!("{NODES}{EQUAL_CHART}"), LAYERS)?;
        assert_eq!(charter_graph(repo.path(), &metadata())?, Vec::new());
        Ok(())
    }

    #[test]
    fn missing_extra_and_wrong_kind_edges_are_all_reported() -> Result<(), Box<dyn Error>> {
        let repo = write_repo(
            &format!("{NODES}    A --> B\n    C --> B\n    B --> C\n"),
            LAYERS,
        )?;
        let found = rendered(&charter_graph(repo.path(), &metadata())?);
        assert_eq!(
            found,
            [
                "DESIGN.md:5: Cargo has `a --> d`, which §2 does not draw (C1)",
                "DESIGN.md:5: Cargo has `c -.-> b`, which §2 does not draw (C1)",
                "DESIGN.md:13: §2 draws `b --> c`, which Cargo does not have (C1)",
                "DESIGN.md:12: §2 draws `c --> b`, which Cargo does not have (C1)",
            ]
        );
        Ok(())
    }

    #[test]
    fn missing_nodes_unknown_crates_and_undefined_ids_are_reported() -> Result<(), Box<dyn Error>> {
        let chart = "    A[\"crates/a\"]\n    B[\"crates/b\"]\n    Z[\"crates/zz\"]\n    \
                     A --> B\n    Q --> B\n";
        let repo = write_repo(chart, LAYERS)?;
        let found = rendered(&charter_graph(repo.path(), &metadata())?);
        for expected in [
            "DESIGN.md:11: edge `Q --> B` names a node that is not defined in the block",
            "DESIGN.md:5: workspace crate `c` has no node in §2",
            "DESIGN.md:5: workspace crate `d` has no node in §2",
            "DESIGN.md:9: node names `crates/zz`, which is not a workspace crate",
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
        let dir = write_repo("", LAYERS)?;
        fs::write(dir.path().join(DESIGN_DOC), "## 2. Graph\n\nNo chart.\n")?;
        assert_eq!(charter_graph(dir.path(), &metadata())?.len(), 1);
        Ok(())
    }

    #[test]
    fn backward_layer_edges_are_reported() -> Result<(), Box<dyn Error>> {
        let layers = "- **Layers:** `d` → `a` → `b`.\n- **Test support:** `c`.\n\
                      - **Standalone:** none.\n";
        let repo = write_repo(&format!("{NODES}{EQUAL_CHART}"), layers)?;
        assert_eq!(
            rendered(&charter_graph(repo.path(), &metadata())?),
            ["AGENTS.md:5: Cargo has `a --> d`, but §5's layers put `a` after `d` (C1)"]
        );
        Ok(())
    }

    #[test]
    fn test_support_standalone_and_unlisted_crates_are_reported() -> Result<(), Box<dyn Error>> {
        let layers = "- **Layers:** `b` → `d` → `zz`.\n- **Test support:** `a`.\n\
                      - **Standalone:** `c`.\n";
        let repo = write_repo(&format!("{NODES}{EQUAL_CHART}"), layers)?;
        let found = rendered(&charter_graph(repo.path(), &metadata())?);
        assert_eq!(
            found,
            [
                "AGENTS.md:5: §5's `- **Layers:**` list names `zz`, which is not a workspace crate",
                "AGENTS.md:6: Cargo has `a --> b`, a normal dependency on a test-support crate \
                 of §5 (C1)",
                "AGENTS.md:6: Cargo has `a --> d`, a normal dependency on a test-support crate \
                 of §5 (C1)",
                "AGENTS.md:7: Cargo has `c -.-> b`, but §5 makes a crate of it standalone (C1)",
            ]
        );
        let twice = "- **Layers:** `a` → `b`.\n- **Test support:** `c`, `a`.\n\
                     - **Standalone:** none.\n";
        let repo = write_repo(&format!("{NODES}{EQUAL_CHART}"), twice)?;
        let found = rendered(&charter_graph(repo.path(), &metadata())?);
        for expected in [
            "AGENTS.md:3: workspace crate `a` is in 2 of §5's crate lists, not 1 (C1)",
            "AGENTS.md:3: workspace crate `d` is in 0 of §5's crate lists, not 1 (C1)",
        ] {
            assert!(
                found.iter().any(|line| line == expected),
                "{expected} in {found:#?}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_charter_without_a_crate_list_is_a_finding() -> Result<(), Box<dyn Error>> {
        let repo = write_repo(
            &format!("{NODES}{EQUAL_CHART}"),
            "- **Layers:** `a` → `b` → `d`.\n- **Test support:** `c`.\n",
        )?;
        assert_eq!(
            rendered(&charter_graph(repo.path(), &metadata())?),
            ["AGENTS.md:3: §5 has no `- **Standalone:**` list of crates (C1)"]
        );
        Ok(())
    }
}
