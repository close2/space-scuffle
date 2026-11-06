//! Check C24: `README.md` exists, its links resolve, and its showcase points at what CI publishes.
//!
//! # Responsibility
//! The README is the repository's front page. Its teaser image and the link to the player page are
//! absolute URLs of the GitHub Pages site the showcase workflow deploys, so neither the Markdown
//! renderer nor C17 notices when they drift from what CI actually publishes. This check compares
//! them with their two sources of truth, read as text:
//! - the site: the `SHOWCASE_SITE` value of `.github/workflows/showcase.yml`;
//! - the file names: the `pub const ..._FILE: &str` items of the showcase encoder
//!   (`crates/client_diagnostics/src/showcase/encode.rs`), which `showcase assemble` writes.
//!
//! # How it fits
//! `tests/charter.rs` runs it on the repository. The library depends on no workspace crate, so the
//! file names are read from the source rather than imported, as C16 reads the CI workflow.
//! Relative links follow C17's rule ([`crate::design_doc`]); HTML `href` and `src` attributes,
//! which the README uses for the linked image, are judged the same way.
//!
//! # Invariants
//! - A missing README, workflow site or encoder constant is a finding, never a silent pass.
//! - The README embeds the teaser file and links to the site's page; every URL it names under the
//!   site is the site itself or one of the published files.

use crate::markdown::{self, MarkdownLine};
use crate::{design_doc, CheckError, Finding};
use std::path::Path;

/// The front page, relative to the repository root.
pub const README: &str = "README.md";

/// The workflow that publishes the showcase, relative to the repository root.
pub const SHOWCASE_WORKFLOW: &str = ".github/workflows/showcase.yml";

/// The workflow's environment key naming the published site.
pub const SHOWCASE_SITE_KEY: &str = "SHOWCASE_SITE:";

/// The source declaring the published file names, relative to the repository root.
pub const SHOWCASE_FILES_SOURCE: &str = "crates/client_diagnostics/src/showcase/encode.rs";

/// The constant among [`SHOWCASE_FILES_SOURCE`]'s file names that the README embeds.
pub const TEASER_CONSTANT: &str = "TEASER_FILE";

/// The constant among [`SHOWCASE_FILES_SOURCE`]'s file names that the README links to.
pub const PAGE_CONSTANT: &str = "PAGE_FILE";

/// HTML attributes whose values are link targets.
const URL_ATTRIBUTES: [&str; 2] = ["href=\"", "src=\""];

/// Check C24 on the repository under `root`.
///
/// # Errors
/// [`CheckError::Io`] if a file that exists cannot be read.
pub fn readme(root: &Path) -> Result<Vec<Finding>, CheckError> {
    let path = root.join(README);
    let Some(text) = read_if_present(&path)? else {
        return Ok(vec![Finding::new(
            root,
            &path,
            1,
            "the repository has no README.md (charter §9, C24)",
        )]);
    };
    let mut findings = Vec::new();
    let site = published_site(root, &mut findings)?;
    let files = published_files(root, &mut findings)?;
    let all = markdown::lines(&text);
    let mut problems = Vec::new();
    design_doc::link_problems(root, &all, &mut problems);
    attribute_problems(root, &all, &mut problems);
    if let (Some(site), Some(files)) = (site, files) {
        showcase_problems(&all, &site, &files, &mut problems);
    }
    findings.extend(
        problems
            .into_iter()
            .map(|(line, message)| Finding::new(root, &path, line, message)),
    );
    Ok(findings)
}

/// The file's text, or `None` if it does not exist.
fn read_if_present(path: &Path) -> Result<Option<String>, CheckError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(CheckError::Io {
            path: path.to_owned(),
            source,
        }),
    }
}

/// The site the workflow publishes to, without a trailing slash.
fn published_site(root: &Path, findings: &mut Vec<Finding>) -> Result<Option<String>, CheckError> {
    let path = root.join(SHOWCASE_WORKFLOW);
    let Some(text) = read_if_present(&path)? else {
        findings.push(Finding::new(
            root,
            &path,
            1,
            "the showcase workflow is missing; C24 reads the published site from it",
        ));
        return Ok(None);
    };
    let site = text.lines().find_map(|line| {
        line.trim().strip_prefix(SHOWCASE_SITE_KEY).map(|value| {
            value
                .trim()
                .trim_matches('"')
                .trim_end_matches('/')
                .to_owned()
        })
    });
    if site.as_deref().is_none_or(str::is_empty) {
        findings.push(Finding::new(
            root,
            &path,
            1,
            format!("no `{SHOWCASE_SITE_KEY}` value naming the published site"),
        ));
        return Ok(None);
    }
    Ok(site)
}

/// The published file names, as `(constant, file name)`, from the encoder's source.
fn published_files(
    root: &Path,
    findings: &mut Vec<Finding>,
) -> Result<Option<Vec<(String, String)>>, CheckError> {
    let path = root.join(SHOWCASE_FILES_SOURCE);
    let Some(text) = read_if_present(&path)? else {
        findings.push(Finding::new(
            root,
            &path,
            1,
            "the showcase encoder is missing; C24 reads the published file names from it",
        ));
        return Ok(None);
    };
    let files: Vec<(String, String)> = text.lines().filter_map(file_constant).collect();
    let missing: Vec<&str> = [TEASER_CONSTANT, PAGE_CONSTANT]
        .into_iter()
        .filter(|wanted| !files.iter().any(|(name, _)| name == wanted))
        .collect();
    if !missing.is_empty() {
        findings.push(Finding::new(
            root,
            &path,
            1,
            format!("no `pub const` file name for {missing:?}"),
        ));
        return Ok(None);
    }
    Ok(Some(files))
}

/// `("NAME_FILE", "value")` from a line `pub const NAME_FILE: &str = "value";`.
fn file_constant(line: &str) -> Option<(String, String)> {
    let rest = line.trim().strip_prefix("pub const ")?;
    let (name, rest) = rest.split_once(": &str = \"")?;
    let value = rest.strip_suffix("\";")?;
    name.ends_with("_FILE")
        .then(|| (name.to_owned(), value.to_owned()))
}

/// The values of the `href` and `src` attributes on `line`, in order.
fn attribute_targets(line: &str) -> Vec<&str> {
    let mut targets = Vec::new();
    for attribute in URL_ATTRIBUTES {
        let mut rest = line;
        while let Some(start) = rest.find(attribute) {
            let after = &rest[start + attribute.len()..];
            let Some(end) = after.find('"') else {
                break;
            };
            targets.push(&after[..end]);
            rest = &after[end..];
        }
    }
    targets
}

/// Relative `href` and `src` targets outside code that do not exist.
fn attribute_problems(root: &Path, all: &[MarkdownLine<'_>], problems: &mut Vec<(usize, String)>) {
    for line in all.iter().filter(|line| !line.fenced) {
        for target in attribute_targets(line.text) {
            if design_doc::is_external(target) {
                continue;
            }
            let file = target.split('#').next().unwrap_or(target);
            if !file.is_empty() && !root.join(file).exists() {
                problems.push((
                    line.number,
                    format!("link target `{target}` does not exist"),
                ));
            }
        }
    }
}

/// Every URL of the README, Markdown link or HTML attribute, with its line number.
fn urls<'a>(all: &[MarkdownLine<'a>]) -> Vec<(usize, &'a str)> {
    all.iter()
        .filter(|line| !line.fenced)
        .flat_map(|line| {
            markdown::link_targets(line.text)
                .into_iter()
                .chain(attribute_targets(line.text))
                .map(move |target| (line.number, target))
        })
        .collect()
}

/// The teaser is embedded, the page is linked, and nothing else under the site is named.
fn showcase_problems(
    all: &[MarkdownLine<'_>],
    site: &str,
    files: &[(String, String)],
    problems: &mut Vec<(usize, String)>,
) {
    let file_of = |constant: &str| {
        files
            .iter()
            .find(|(name, _)| name == constant)
            .map_or("", |(_, file)| file.as_str())
    };
    let teaser = format!("{site}/{}", file_of(TEASER_CONSTANT));
    let page = [
        site.to_owned(),
        format!("{site}/"),
        format!("{site}/{}", file_of(PAGE_CONSTANT)),
    ];
    let found = urls(all);
    let embeds_teaser = all
        .iter()
        .any(|line| !line.fenced && line.text.contains(&format!("<img src=\"{teaser}\"")));
    if !embeds_teaser {
        problems.push((
            1,
            format!("the README must embed the showcase teaser as `<img src=\"{teaser}\"`"),
        ));
    }
    if !found
        .iter()
        .any(|(_, url)| page.iter().any(|page| page == url))
    {
        problems.push((
            1,
            format!("the README must link to the showcase page `{site}/`"),
        ));
    }
    let host = site.split('/').take(3).collect::<Vec<_>>().join("/");
    for (line, url) in found {
        if !url.starts_with(&host) {
            continue;
        }
        let published = page.iter().any(|page| page == url)
            || files
                .iter()
                .any(|(_, file)| url == format!("{site}/{file}"));
        if !published {
            problems.push((
                line,
                format!(
                    "`{url}` is not a file the showcase workflow publishes under `{site}` \
                     (`{SHOWCASE_FILES_SOURCE}`)"
                ),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::fs;

    const SITE: &str = "https://owner.github.io/game";

    const ENCODER: &str = "//! Encoder.\n\
                           pub const VIDEO_FILE: &str = \"showcase.mp4\";\n\
                           pub const TEASER_FILE: &str = \"showcase.webp\";\n\
                           pub const PAGE_FILE: &str = \"index.html\";\n";

    fn good_readme() -> String {
        format!(
            "# Game\n\n<a href=\"{SITE}/\"><img src=\"{SITE}/showcase.webp\" alt=\"x\"></a>\n\n\
             [Full video]({SITE}/showcase.mp4), [design](DESIGN.md#4-status), \
             [web](https://example.org/a)\n"
        )
    }

    fn repo(readme: Option<&str>) -> Result<tempfile::TempDir, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        fs::write(dir.path().join("DESIGN.md"), "d")?;
        fs::create_dir_all(dir.path().join(".github/workflows"))?;
        fs::write(
            dir.path().join(SHOWCASE_WORKFLOW),
            format!("name: S\nenv:\n  {SHOWCASE_SITE_KEY} {SITE}\n"),
        )?;
        fs::create_dir_all(dir.path().join("crates/client_diagnostics/src/showcase"))?;
        fs::write(dir.path().join(SHOWCASE_FILES_SOURCE), ENCODER)?;
        if let Some(readme) = readme {
            fs::write(dir.path().join(README), readme)?;
        }
        Ok(dir)
    }

    fn messages(root: &Path) -> Result<Vec<String>, Box<dyn Error>> {
        Ok(readme(root)?.iter().map(ToString::to_string).collect())
    }

    #[test]
    fn c24_accepts_a_readme_that_matches_the_workflow_and_encoder() -> Result<(), Box<dyn Error>> {
        let dir = repo(Some(&good_readme()))?;
        let found = messages(dir.path())?;
        assert!(found.is_empty(), "{found:#?}");
        Ok(())
    }

    #[test]
    fn c24_reports_a_missing_readme() -> Result<(), Box<dyn Error>> {
        let dir = repo(None)?;
        assert_eq!(
            messages(dir.path())?,
            ["README.md:1: the repository has no README.md (charter §9, C24)"]
        );
        Ok(())
    }

    #[test]
    fn c24_reports_broken_relative_links_in_markdown_and_html() -> Result<(), Box<dyn Error>> {
        let readme = format!(
            "{}[gone](docs/nope.md)\n<img src=\"art/x.png\">\n",
            good_readme()
        );
        let dir = repo(Some(&readme))?;
        assert_eq!(
            messages(dir.path())?,
            [
                "README.md:6: link target `docs/nope.md` does not exist",
                "README.md:7: link target `art/x.png` does not exist",
            ]
        );
        Ok(())
    }

    #[test]
    fn c24_reports_a_site_the_workflow_does_not_publish() -> Result<(), Box<dyn Error>> {
        let readme = good_readme().replace("/game", "/other");
        let dir = repo(Some(&readme))?;
        let found = messages(dir.path())?;
        assert!(
            found[0].contains("must embed the showcase teaser")
                && found[1].contains("must link to the showcase page")
                && found[2].contains("`https://owner.github.io/other/` is not a file"),
            "{found:#?}"
        );
        assert_eq!(found.len(), 5, "{found:#?}");
        Ok(())
    }

    #[test]
    fn c24_reports_a_teaser_file_name_assemble_does_not_write() -> Result<(), Box<dyn Error>> {
        let readme = good_readme().replace("showcase.webp", "showcase.gif");
        let dir = repo(Some(&readme))?;
        assert_eq!(
            messages(dir.path())?,
            [
                format!(
                    "README.md:1: the README must embed the showcase teaser as \
                     `<img src=\"{SITE}/showcase.webp\"`"
                ),
                format!(
                    "README.md:3: `{SITE}/showcase.gif` is not a file the showcase workflow \
                     publishes under `{SITE}` (`{SHOWCASE_FILES_SOURCE}`)"
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn c24_reports_missing_sources_of_truth() -> Result<(), Box<dyn Error>> {
        let dir = repo(Some(&good_readme()))?;
        fs::write(dir.path().join(SHOWCASE_WORKFLOW), "name: S\n")?;
        fs::write(
            dir.path().join(SHOWCASE_FILES_SOURCE),
            "pub const VIDEO_FILE: &str = \"v.mp4\";\n",
        )?;
        let found = messages(dir.path())?;
        assert_eq!(found.len(), 2, "{found:#?}");
        assert!(found[0].contains("no `SHOWCASE_SITE:` value"), "{found:#?}");
        assert!(found[1].contains("TEASER_FILE"), "{found:#?}");
        fs::remove_file(dir.path().join(SHOWCASE_WORKFLOW))?;
        let found = messages(dir.path())?;
        assert!(
            found[0].contains("the showcase workflow is missing"),
            "{found:#?}"
        );
        Ok(())
    }

    #[test]
    fn attribute_targets_read_href_and_src() {
        assert_eq!(
            attribute_targets("<a href=\"a\"><img src=\"b\" alt=\"c\"></a> <a href=\"d\">"),
            ["a", "d", "b"]
        );
        assert_eq!(attribute_targets("href=\"open"), Vec::<&str>::new());
    }

    #[test]
    fn file_constants_are_read_from_source_lines() {
        assert_eq!(
            file_constant("pub const TEASER_FILE: &str = \"showcase.webp\";"),
            Some(("TEASER_FILE".into(), "showcase.webp".into()))
        );
        assert_eq!(file_constant("pub const LIMIT: &str = \"x\";"), None);
        assert_eq!(file_constant("const PAGE_FILE: &str = \"x\";"), None);
    }
}
