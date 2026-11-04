//! The suite catalog `crates/gate/suites.ron`: classes, path rules and downstream contracts.
//!
//! # Responsibility
//! Parses the catalog file into typed data: the globs that select the complete suite (`global`)
//! or nothing beyond the cheap checks (`docs`), the targets run in every tier (`always`), the
//! targets that are not `fast` with their class and trigger paths (`targets`), the path rules
//! (`rules`), the downstream contracts per crate (`downstream`) and the extra commands such as
//! the golden replays (`commands`). Blueprint §4 defines the shape.
//!
//! # How it fits
//! The selection (`select.rs`) reads only this typed form. Names of targets stay unresolved
//! [`TargetRef`]s here; they are resolved against `cargo metadata` when a selection is made, and
//! the catalog test (`tests/suite_catalog_tests.rs`, check C22) proves every one resolves.
//!
//! # Invariants
//! - A target the catalog does not list is [`TestClass::Fast`].
//! - Keys of `targets` name single targets, never a `@class` group.
//! - Unknown fields are rejected, so a misspelt key cannot silently select nothing.

use crate::glob::Glob;
use crate::workspace::{TargetId, TargetKind, Workspace};
use crate::GateError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

/// Where the catalog lives, relative to the repository root.
pub const CATALOG_PATH: &str = "crates/gate/suites.ron";

/// How expensive a test target is to run, which decides the tiers it runs in (blueprint §3.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TestClass {
    /// Under about 20 s on an idle machine, no GPU, no valgrind. The default.
    Fast,
    /// Headless but long: whole battles, whole matrices, ffmpeg.
    Slow,
    /// Launches GPU children through `crates/client/tests/support/gpu_run.rs`.
    Gpu,
    /// Counts allocations under valgrind.
    Valgrind,
}

impl TestClass {
    /// Every class.
    pub const ALL: [Self; 4] = [Self::Fast, Self::Slow, Self::Gpu, Self::Valgrind];

    /// The class's lower-case name, as written after `@` in a [`TargetRef`].
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Slow => "slow",
            Self::Gpu => "gpu",
            Self::Valgrind => "valgrind",
        }
    }

    /// Whether a crate change runs this class unconditionally (`fast`, `slow`), rather than only
    /// on a trigger (`gpu`, `valgrind`).
    #[must_use]
    pub fn runs_on_crate_change(self) -> bool {
        matches!(self, Self::Fast | Self::Slow)
    }
}

impl fmt::Display for TestClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A name for one target or for every target of one class in one crate.
///
/// Written `crate/lib`, `crate/bin/<name>`, `crate/<integration test>` or `crate/@<class>`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TargetRef {
    /// Exactly one target.
    Target(TargetId),
    /// Every target of `class` in `package`.
    Class {
        /// The workspace crate.
        package: String,
        /// The class all named targets have.
        class: TestClass,
    },
}

impl TargetRef {
    /// Parses a reference as written in the catalog.
    ///
    /// # Errors
    /// [`GateError::Catalog`] when the text has no `/`, an empty part, or an unknown class.
    pub fn parse(text: &str) -> Result<Self, GateError> {
        let bad = |why: &str| GateError::Catalog(format!("target reference `{text}` {why}"));
        let (package, rest) = text
            .split_once('/')
            .ok_or_else(|| bad("must be `crate/target`"))?;
        if package.is_empty() || rest.is_empty() {
            return Err(bad("has an empty part"));
        }
        if let Some(class) = rest.strip_prefix('@') {
            let class = TestClass::ALL
                .into_iter()
                .find(|known| known.name() == class)
                .ok_or_else(|| bad("names no class (fast, slow, gpu, valgrind)"))?;
            return Ok(Self::Class {
                package: package.to_string(),
                class,
            });
        }
        let kind = if rest == "lib" {
            TargetKind::Lib
        } else if let Some(bin) = rest.strip_prefix("bin/") {
            TargetKind::Bin(bin.to_string())
        } else {
            TargetKind::Test(rest.to_string())
        };
        if matches!(&kind, TargetKind::Bin(n) | TargetKind::Test(n) if n.is_empty() || n.contains('/'))
        {
            return Err(bad("names no single target"));
        }
        Ok(Self::Target(TargetId {
            package: package.to_string(),
            kind,
        }))
    }

    /// The crate the reference points into.
    #[must_use]
    pub fn package(&self) -> &str {
        match self {
            Self::Target(id) => &id.package,
            Self::Class { package, .. } => package,
        }
    }
}

impl fmt::Display for TargetRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Target(id) => write!(f, "{id}"),
            Self::Class { package, class } => write!(f, "{package}/@{class}"),
        }
    }
}

/// A target that is not `fast`: its class and the extra paths that trigger it.
#[derive(Clone, Debug)]
pub struct TargetEntry {
    /// The target's class.
    pub class: TestClass,
    /// Paths whose change selects this target, beyond its own source files.
    pub triggers: Vec<Glob>,
}

/// A path rule: a change under `paths` selects `targets` and the `fast`/`slow` targets of
/// `crates`.
#[derive(Clone, Debug)]
pub struct PathRule {
    /// The rule's name, printed by `explain`.
    pub name: String,
    /// The paths the rule covers.
    pub paths: Vec<Glob>,
    /// Targets selected whatever their class.
    pub targets: Vec<TargetRef>,
    /// Crates whose `fast` and `slow` targets are selected.
    pub crates: Vec<String>,
}

/// An extra command of the `package` and `full` tiers, such as a golden replay in release.
#[derive(Clone, Debug)]
pub struct CommandStep {
    /// The step's name, printed by `explain` and in the result table.
    pub name: String,
    /// Paths whose change makes `package` run the step; `full` always runs it.
    pub paths: Vec<Glob>,
    /// The program, `cargo` for every step today.
    pub program: String,
    /// Its arguments.
    pub args: Vec<String>,
    /// Extra environment variables.
    pub env: BTreeMap<String, String>,
}

/// The parsed catalog.
#[derive(Clone, Debug)]
pub struct SuiteCatalog {
    /// Paths whose change selects the complete suite.
    pub global: Vec<Glob>,
    /// Paths that select nothing beyond fmt and the `always` targets.
    pub docs: Vec<Glob>,
    /// Targets every tier runs (the charter checks).
    pub always: Vec<TargetRef>,
    /// Targets that are not `fast`.
    pub targets: BTreeMap<TargetId, TargetEntry>,
    /// Path rules, in file order.
    pub rules: Vec<PathRule>,
    /// Per changed crate, the targets of its reverse dependencies it selects.
    pub downstream: BTreeMap<String, Vec<TargetRef>>,
    /// Extra commands.
    pub commands: Vec<CommandStep>,
}

/// The file's syntax, before globs and references are checked.
#[derive(Deserialize)]
#[serde(rename = "SuiteCatalog", deny_unknown_fields)]
struct RawCatalog {
    global: Vec<String>,
    docs: Vec<String>,
    #[serde(default)]
    always: Vec<String>,
    #[serde(default)]
    targets: BTreeMap<String, RawEntry>,
    #[serde(default)]
    rules: Vec<RawRule>,
    #[serde(default)]
    downstream: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    commands: Vec<RawCommand>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    class: TestClass,
    #[serde(default)]
    triggers: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRule {
    name: String,
    paths: Vec<String>,
    #[serde(default)]
    targets: Vec<String>,
    #[serde(default)]
    crates: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCommand {
    name: String,
    paths: Vec<String>,
    program: String,
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
}

/// Compiles every pattern of `patterns`.
fn globs(patterns: &[String]) -> Result<Vec<Glob>, GateError> {
    patterns.iter().map(|pattern| Glob::new(pattern)).collect()
}

/// Parses every reference of `texts`.
fn refs(texts: &[String]) -> Result<Vec<TargetRef>, GateError> {
    texts.iter().map(|text| TargetRef::parse(text)).collect()
}

impl SuiteCatalog {
    /// Parses catalog text; `path` only names the file in errors.
    ///
    /// # Errors
    /// [`GateError::CatalogSyntax`] for invalid RON or unknown fields; [`GateError::Glob`] for a
    /// malformed pattern; [`GateError::Catalog`] for a malformed reference or a `targets` key
    /// naming a class group.
    pub fn parse(text: &str, path: &Path) -> Result<Self, GateError> {
        let raw: RawCatalog = ron::from_str(text).map_err(|source| GateError::CatalogSyntax {
            path: path.to_path_buf(),
            source,
        })?;
        let mut targets = BTreeMap::new();
        for (key, entry) in &raw.targets {
            let TargetRef::Target(id) = TargetRef::parse(key)? else {
                return Err(GateError::Catalog(format!(
                    "`targets` key `{key}` must name one target, not a class"
                )));
            };
            let entry = TargetEntry {
                class: entry.class,
                triggers: globs(&entry.triggers)?,
            };
            targets.insert(id, entry);
        }
        let rules = raw
            .rules
            .iter()
            .map(|rule| {
                Ok(PathRule {
                    name: rule.name.clone(),
                    paths: globs(&rule.paths)?,
                    targets: refs(&rule.targets)?,
                    crates: rule.crates.clone(),
                })
            })
            .collect::<Result<_, GateError>>()?;
        let downstream = raw
            .downstream
            .iter()
            .map(|(krate, list)| Ok((krate.clone(), refs(list)?)))
            .collect::<Result<_, GateError>>()?;
        let commands = raw
            .commands
            .iter()
            .map(|command| {
                Ok(CommandStep {
                    name: command.name.clone(),
                    paths: globs(&command.paths)?,
                    program: command.program.clone(),
                    args: command.args.clone(),
                    env: command.env.clone(),
                })
            })
            .collect::<Result<_, GateError>>()?;
        Ok(Self {
            global: globs(&raw.global)?,
            docs: globs(&raw.docs)?,
            always: refs(&raw.always)?,
            targets,
            rules,
            downstream,
            commands,
        })
    }

    /// Reads and parses [`CATALOG_PATH`] under `root`.
    ///
    /// # Errors
    /// [`GateError::Io`] if the file cannot be read, else as [`SuiteCatalog::parse`].
    pub fn load(root: &Path) -> Result<Self, GateError> {
        let path = root.join(CATALOG_PATH);
        let text = std::fs::read_to_string(&path).map_err(|source| GateError::Io {
            path: path.clone(),
            source,
        })?;
        Self::parse(&text, &path)
    }

    /// The class of `target`: its catalog entry's, or [`TestClass::Fast`].
    #[must_use]
    pub fn class_of(&self, target: &TargetId) -> TestClass {
        self.targets
            .get(target)
            .map_or(TestClass::Fast, |entry| entry.class)
    }

    /// The targets `reference` names in `workspace`, in target order.
    ///
    /// # Errors
    /// [`GateError::Catalog`] when the crate or the single target does not exist. A class
    /// group with no member resolves to nothing; the catalog test reports that separately.
    pub fn resolve(
        &self,
        reference: &TargetRef,
        workspace: &Workspace,
    ) -> Result<Vec<TargetId>, GateError> {
        let package = workspace
            .package(reference.package())
            .ok_or_else(|| GateError::Catalog(format!("`{reference}` names no workspace crate")))?;
        match reference {
            TargetRef::Target(id) => {
                if package.targets.iter().any(|target| &target.id == id) {
                    Ok(vec![id.clone()])
                } else {
                    Err(GateError::Catalog(format!(
                        "`{reference}` names no test target of `{}`",
                        package.name
                    )))
                }
            }
            TargetRef::Class { class, .. } => Ok(package
                .targets
                .iter()
                .filter(|target| self.class_of(&target.id) == *class)
                .map(|target| target.id.clone())
                .collect()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"SuiteCatalog(
        global: ["Cargo.toml"],
        docs: ["docs/**"],
        always: ["checks/@fast"],
        targets: {
            "app/gpu_tests": (class: Gpu, triggers: ["crates/app/src/render/**"]),
            "app/lib": (class: Slow),
        },
        rules: [(name: "shaders", paths: ["assets/shaders/**"], targets: ["app/@gpu"])],
        downstream: {"core": ["app/headless_tests"]},
        commands: [(name: "golden", paths: ["crates/core/**"], program: "cargo",
                    args: ["test"], env: {"A": "b"})],
    )"#;

    #[test]
    fn a_complete_catalog_parses() -> Result<(), GateError> {
        let catalog = SuiteCatalog::parse(SAMPLE, Path::new("suites.ron"))?;
        assert_eq!(catalog.global.len(), 1);
        assert_eq!(catalog.rules[0].name, "shaders");
        let lib = TargetId::lib("app");
        assert_eq!(catalog.class_of(&lib), TestClass::Slow);
        assert_eq!(
            catalog.class_of(&TargetId::test("app", "x")),
            TestClass::Fast
        );
        assert_eq!(catalog.commands[0].env["A"], "b");
        Ok(())
    }

    #[test]
    fn references_round_trip_through_display() -> Result<(), GateError> {
        for text in [
            "a/lib",
            "a/bin/tool",
            "a/some_tests",
            "a/@gpu",
            "a/@valgrind",
        ] {
            assert_eq!(TargetRef::parse(text)?.to_string(), text);
        }
        Ok(())
    }

    #[test]
    fn malformed_references_are_errors() {
        for bad in ["nocrate", "/x", "a/", "a/@huge", "a/bin/", "a/x/y"] {
            assert!(TargetRef::parse(bad).is_err(), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn unknown_fields_and_class_keys_are_rejected() {
        let typo = SAMPLE.replace("downstream:", "downstraem:");
        assert!(matches!(
            SuiteCatalog::parse(&typo, Path::new("s")),
            Err(GateError::CatalogSyntax { .. })
        ));
        let group_key = SAMPLE.replace("\"app/lib\"", "\"app/@slow\"");
        assert!(matches!(
            SuiteCatalog::parse(&group_key, Path::new("s")),
            Err(GateError::Catalog(_))
        ));
    }
}
