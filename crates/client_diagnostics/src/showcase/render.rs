//! `showcase render` and `showcase assemble`: the whole video, as CI and a developer run it.
//!
//! # Responsibility
//! Plan `docs/plans/2026-11-05-ci-showcase-video.md`, decisions D-1, D-12 and D-16.
//! [`render`] runs one `showcase take` child per take of the storyboard on the selected driver,
//! then [`assemble`]s the site. [`take_command`] decides, from an injectable [`Host`], exactly how a
//! child is launched:
//! - `radv` (the shared physical GPU): under `with-gpu-lock`, one lock per take and never for the
//!   whole render; it refuses to start when `with-gpu-lock` is not on `PATH` (charter §7);
//! - `lavapipe` (software Vulkan, CI): the first existing `lavapipe` manifest, and the terrain
//!   anisotropy override when its Mesa is older than 25.1 (Q0009), decided by
//!   `client::render_compatibility`.
//!
//! Both run under `xvfb-run` with a 1920×1080 screen, `WGPU_BACKEND=vulkan` and
//! `SPACE_SCUFFLE_VIRTUAL_GPU=1`, as the GPU test harness does. Each take child leads its own
//! process group, killed whole when the take ends ([`super::process_group`]).
//!
//! # Layout of `--out`
//! `takes/<take>.mkv`, `takes/<take>.manifest.ron` (the lossless mezzanines), and `site/` with
//! `index.html`, `showcase.mp4`, `showcase.webp`, `showcase-poster.png` and `showcase.json`, the
//! directory the Pages deployment publishes.

use super::encode::{
    self, CutPlan, TakeInput, PAGE_FILE, POSTER_FILE, PROVENANCE_FILE, TEASER_FILE, VIDEO_FILE,
};
use super::recorder::TakeManifest;
use super::site::{self, FileRecord, PageFacts, Provenance, TakeRecord};
use super::{ShowcaseError, ValidatedStoryboard};
use std::path::{Path, PathBuf};
use std::process::Command;

/// `lavapipe` manifests, in order of preference (Debian, then Ubuntu).
pub const LAVAPIPE_MANIFESTS: [&str; 2] = [
    "/usr/share/vulkan/icd.d/lvp_icd.json",
    "/usr/share/vulkan/icd.d/lvp_icd.x86_64.json",
];

/// `radv` manifests, in order of preference.
pub const RADV_MANIFESTS: [&str; 2] = [
    "/usr/share/vulkan/icd.d/radeon_icd.json",
    "/usr/share/vulkan/icd.d/radeon_icd.x86_64.json",
];

/// The program that serialises use of the shared physical GPU (charter §7).
pub const LOCK_TOOL: &str = "with-gpu-lock";

/// The Xvfb screen the takes render on; it must hold the storyboard's window.
pub const XVFB_SCREEN: &str = "-screen 0 1920x1080x24";

/// Where a take renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Driver {
    /// The shared physical GPU through `radv`, under `with-gpu-lock`.
    Radv,
    /// Software Vulkan, as on CI.
    Lavapipe,
}

/// What [`take_command`] needs to know about the machine; injectable for tests.
pub trait Host {
    /// The full path of `program` on `PATH`, if any.
    fn find_on_path(&self, program: &str) -> Option<PathBuf>;
    /// Whether `path` exists.
    fn exists(&self, path: &Path) -> bool;
    /// `vulkaninfo --summary` for the driver of `manifest`, if it runs.
    fn vulkan_summary(&self, manifest: &Path) -> Option<String>;
}

/// The real machine.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemHost;

impl Host for SystemHost {
    fn find_on_path(&self, program: &str) -> Option<PathBuf> {
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|directory| directory.join(program))
                .find(|candidate| candidate.is_file())
        })
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn vulkan_summary(&self, manifest: &Path) -> Option<String> {
        let output = Command::new("vulkaninfo")
            .arg("--summary")
            .env("VK_ICD_FILENAMES", manifest)
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

/// How one take child is launched: program, arguments and environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildPlan {
    /// The program run first (`with-gpu-lock` or `xvfb-run`).
    pub program: PathBuf,
    /// Its arguments, ending with the `showcase take` command line.
    pub arguments: Vec<String>,
    /// Variables set for the child.
    pub environment: Vec<(String, String)>,
    /// Variables removed from the child.
    pub removed: Vec<String>,
}

/// Plans the launch of `take_arguments` (the `showcase take ...` command line, program first) on
/// `driver`.
///
/// # Errors
/// [`ShowcaseError::Render`] when `with-gpu-lock` (on `radv`), `xvfb-run` or a Vulkan manifest is
/// missing.
pub fn take_command(
    driver: Driver,
    host: &dyn Host,
    take_arguments: &[String],
) -> Result<ChildPlan, ShowcaseError> {
    let xvfb = host
        .find_on_path("xvfb-run")
        .ok_or_else(|| render_error("xvfb-run is not on PATH"))?;
    let first = |candidates: &[&str]| {
        candidates
            .iter()
            .map(PathBuf::from)
            .find(|path| host.exists(path))
    };
    let mut environment = vec![
        ("WGPU_BACKEND".to_owned(), "vulkan".to_owned()),
        ("SPACE_SCUFFLE_VIRTUAL_GPU".to_owned(), "1".to_owned()),
        ("WINIT_UNIX_BACKEND".to_owned(), "x11".to_owned()),
    ];
    let mut arguments = Vec::new();
    let mut removed = vec!["WAYLAND_DISPLAY".to_owned()];
    let program = match driver {
        Driver::Radv => {
            let lock = host.find_on_path(LOCK_TOOL).ok_or_else(|| {
                render_error("refusing to render on the shared physical GPU without with-gpu-lock on PATH (charter §7)")
            })?;
            let manifest = first(&RADV_MANIFESTS)
                .ok_or_else(|| render_error("no radv Vulkan manifest found"))?;
            environment.push((
                "VK_ICD_FILENAMES".to_owned(),
                manifest.to_string_lossy().into_owned(),
            ));
            environment.push(("MESA_VK_WSI_DEBUG".to_owned(), "sw".to_owned()));
            arguments.push(xvfb.to_string_lossy().into_owned());
            lock
        }
        Driver::Lavapipe => {
            let manifest = first(&LAVAPIPE_MANIFESTS)
                .ok_or_else(|| render_error("no lavapipe Vulkan manifest found"))?;
            let release = host
                .vulkan_summary(&manifest)
                .as_deref()
                .and_then(client::render_compatibility::mesa_release);
            if let Some(value) = client::render_compatibility::lavapipe_anisotropy_override(release)
            {
                environment.push((
                    super::take::ANISOTROPY_VARIABLE.to_owned(),
                    value.to_owned(),
                ));
            }
            environment.push((
                "VK_ICD_FILENAMES".to_owned(),
                manifest.to_string_lossy().into_owned(),
            ));
            removed.push("MESA_VK_WSI_DEBUG".to_owned());
            xvfb
        }
    };
    arguments.extend(["-a".to_owned(), "-s".to_owned(), XVFB_SCREEN.to_owned()]);
    arguments.extend(take_arguments.iter().cloned());
    Ok(ChildPlan {
        program,
        arguments,
        environment,
        removed,
    })
}

fn render_error(reason: &str) -> ShowcaseError {
    ShowcaseError::Render(reason.to_owned())
}

/// Options of `showcase render`.
#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// Where the takes render.
    pub driver: Driver,
    /// New output directory.
    pub out: PathBuf,
    /// Storyboard, relative to the assets root unless absolute.
    pub storyboard: PathBuf,
    /// Commit rendered; `git rev-parse HEAD` when absent.
    pub revision: Option<String>,
    /// Commit date; the commit's date when absent.
    pub revision_date: Option<String>,
    /// The CI run that renders, if any.
    pub run_url: Option<String>,
}

/// Options of `showcase assemble`.
#[derive(Debug, Clone)]
pub struct AssembleOptions {
    /// Storyboard, relative to the assets root unless absolute.
    pub storyboard: PathBuf,
    /// Directory holding the takes' mezzanines and manifests.
    pub takes: PathBuf,
    /// Commit rendered.
    pub revision: String,
    /// Commit date.
    pub revision_date: String,
    /// The CI run, if any.
    pub run_url: Option<String>,
    /// New site directory.
    pub out: PathBuf,
}

/// The assets root and repository root the tool reads.
fn roots() -> Result<(PathBuf, PathBuf), ShowcaseError> {
    let assets = client::ResolvedAssetsRoot::resolve()?.as_path().to_owned();
    let repository = assets
        .parent()
        .map(Path::to_owned)
        .ok_or_else(|| render_error("the assets root has no parent"))?;
    Ok((assets, repository))
}

/// `git` output for the repository, trimmed.
fn git(repository: &Path, arguments: &[&str]) -> Result<String, ShowcaseError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()
        .map_err(|error| ShowcaseError::Render(format!("cannot run git: {error}")))?;
    if !output.status.success() {
        return Err(ShowcaseError::Render(format!(
            "git {}: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Renders every take of the storyboard on `options.driver`, then assembles the site.
///
/// # Errors
/// Any take or assembly failure; the output directory must not exist yet.
pub fn render(options: &RenderOptions) -> Result<PathBuf, ShowcaseError> {
    super::take::require_tool("ffmpeg")?;
    let (assets, repository) = roots()?;
    let validated = ValidatedStoryboard::load(&assets, &repository, &options.storyboard)?;
    if options.out.exists() {
        return Err(ShowcaseError::Render(format!(
            "{} exists; render writes a new directory",
            options.out.display()
        )));
    }
    let revision = match &options.revision {
        Some(revision) => revision.clone(),
        None => git(&repository, &["rev-parse", "HEAD"])?,
    };
    let revision_date = match &options.revision_date {
        Some(date) => date.clone(),
        None => git(&repository, &["show", "-s", "--format=%cs", "HEAD"])?,
    };
    let takes = options.out.join("takes");
    std::fs::create_dir_all(&takes).map_err(|source| ShowcaseError::Io {
        path: takes.clone(),
        source,
    })?;
    let program = std::env::current_exe().map_err(|source| ShowcaseError::Io {
        path: PathBuf::from("current executable"),
        source,
    })?;
    let storyboard = validated.path.to_string_lossy().into_owned();
    for take in &validated.storyboard.takes {
        let take_arguments: Vec<String> = vec![
            program.to_string_lossy().into_owned(),
            "take".into(),
            "--storyboard".into(),
            storyboard.clone(),
            "--take".into(),
            take.name.clone(),
            "--out".into(),
            takes.to_string_lossy().into_owned(),
            "--revision".into(),
            revision.clone(),
            "--revision-date".into(),
            revision_date.clone(),
            "--repository".into(),
            repository.to_string_lossy().into_owned(),
        ];
        let plan = take_command(options.driver, &SystemHost, &take_arguments)?;
        run_child(&take.name, &plan)?;
    }
    let site = options.out.join("site");
    assemble(&AssembleOptions {
        storyboard: options.storyboard.clone(),
        takes,
        revision,
        revision_date,
        run_url: options.run_url.clone(),
        out: site.clone(),
    })?;
    Ok(site)
}

/// Runs one take child with inherited output, so its log reaches the console or CI.
///
/// The child leads its own process group, which is killed when it ends: no descendant (the X
/// server of `xvfb-run`, an encoder) may outlive the take and keep `with-gpu-lock`'s inherited
/// lock descriptor open ([`super::process_group`]).
fn run_child(take: &str, plan: &ChildPlan) -> Result<(), ShowcaseError> {
    let mut command = Command::new(&plan.program);
    command.args(&plan.arguments);
    for (name, value) in &plan.environment {
        command.env(name, value);
    }
    for name in &plan.removed {
        command.env_remove(name);
    }
    println!(
        "showcase render: take `{take}`: {} {}",
        plan.program.display(),
        plan.arguments.join(" ")
    );
    let status =
        super::process_group::run_in_own_group(&mut command, &format!("take `{take}`"))?.status;
    if !status.success() {
        return Err(ShowcaseError::Render(format!(
            "take `{take}` failed: {status}"
        )));
    }
    Ok(())
}

/// The first line of `ffmpeg -version`.
fn encoder_version() -> Result<String, ShowcaseError> {
    let output = Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map_err(|error| ShowcaseError::Render(format!("cannot run ffmpeg: {error}")))?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned())
}

/// Reads every take's manifest from `directory`; each must be complete.
fn load_takes(
    validated: &ValidatedStoryboard,
    directory: &Path,
) -> Result<Vec<TakeInput>, ShowcaseError> {
    validated
        .storyboard
        .takes
        .iter()
        .map(|take| {
            let manifest = TakeManifest::load(&TakeManifest::path(directory, &take.name))
                .map_err(ShowcaseError::Render)?;
            if !manifest.complete {
                return Err(ShowcaseError::Render(format!(
                    "take `{}` is incomplete: {}",
                    take.name,
                    manifest.failure.as_deref().unwrap_or("no reason recorded")
                )));
            }
            Ok(TakeInput {
                mezzanine: directory.join(&manifest.mezzanine),
                manifest,
            })
        })
        .collect()
}

/// Cuts and encodes the site from the takes in `options.takes`.
///
/// # Errors
/// [`ShowcaseError::Encode`] or [`ShowcaseError::OverBudget`] (nothing is published), or I/O.
pub fn assemble(options: &AssembleOptions) -> Result<PathBuf, ShowcaseError> {
    super::take::require_tool("ffmpeg")?;
    let (assets, repository) = roots()?;
    let validated = ValidatedStoryboard::load(&assets, &repository, &options.storyboard)?;
    assemble_with(&validated, options)
}

/// [`assemble`] with an already validated storyboard.
///
/// # Errors
/// As [`assemble`].
pub fn assemble_with(
    validated: &ValidatedStoryboard,
    options: &AssembleOptions,
) -> Result<PathBuf, ShowcaseError> {
    let storyboard = &validated.storyboard;
    let takes = load_takes(validated, &options.takes)?;
    let plan = CutPlan::new(storyboard, &takes)?;
    if options.out.exists() {
        return Err(ShowcaseError::Render(format!(
            "{} exists; assemble writes a new directory",
            options.out.display()
        )));
    }
    std::fs::create_dir_all(&options.out).map_err(|source| ShowcaseError::Io {
        path: options.out.clone(),
        source,
    })?;
    let video = options.out.join(VIDEO_FILE);
    encode::run_ffmpeg(
        "video",
        &encode::video_arguments(storyboard, &takes, &video),
    )?;
    let video_bytes = FileRecord::of(&video)?.bytes;
    if video_bytes > storyboard.video.max_bytes {
        return Err(ShowcaseError::OverBudget {
            output: VIDEO_FILE.to_owned(),
            budget: storyboard.video.max_bytes,
            attempts: vec![format!("crf {}: {video_bytes} bytes", storyboard.video.crf)],
        });
    }
    let teaser = options.out.join(TEASER_FILE);
    let (kept, attempts) = encode::ladder(
        TEASER_FILE,
        &storyboard.teaser.widths,
        &storyboard.teaser.qualities,
        storyboard.teaser.max_bytes,
        |width, quality| {
            let attempt = options.out.join(format!("teaser-{width}-q{quality}.webp"));
            encode::run_ffmpeg(
                "teaser",
                &encode::teaser_arguments(storyboard, &takes, &plan, (width, quality), &attempt),
            )?;
            let bytes = FileRecord::of(&attempt)?.bytes;
            if bytes <= storyboard.teaser.max_bytes {
                std::fs::rename(&attempt, &teaser).map_err(|source| ShowcaseError::Io {
                    path: teaser.clone(),
                    source,
                })?;
            } else {
                std::fs::remove_file(&attempt).map_err(|source| ShowcaseError::Io {
                    path: attempt.clone(),
                    source,
                })?;
            }
            Ok(bytes)
        },
    )?;
    let poster = options.out.join(POSTER_FILE);
    encode::run_ffmpeg("poster", &encode::poster_arguments(&takes, &plan, &poster))?;
    let provenance = provenance(
        options,
        &takes,
        (kept.width, kept.quality),
        attempts.iter().map(ToString::to_string).collect(),
    )?;
    write(
        &options.out.join(PROVENANCE_FILE),
        &serde_json::to_string_pretty(&provenance)
            .map_err(|error| render_error(&error.to_string()))?,
    )?;
    let first = takes.first().map(|take| &take.manifest);
    let page = site::render_page(
        site::TEMPLATE,
        &PageFacts {
            pitch: validated.cards.title.pitch.clone(),
            revision: options.revision.clone(),
            revision_date: options.revision_date.clone(),
            adapter: first
                .map(|manifest| manifest.adapter.name.clone())
                .unwrap_or_default(),
            anisotropy: first.and_then(|manifest| manifest.anisotropy_override.clone()),
            run_url: options.run_url.clone(),
            scenes: takes
                .iter()
                .flat_map(|take| take.manifest.scenes.clone())
                .collect(),
        },
    )?;
    write(&options.out.join(PAGE_FILE), &page)?;
    Ok(options.out.clone())
}

fn write(path: &Path, text: &str) -> Result<(), ShowcaseError> {
    std::fs::write(path, text).map_err(|source| ShowcaseError::Io {
        path: path.to_owned(),
        source,
    })
}

/// The provenance record of an assembled site.
fn provenance(
    options: &AssembleOptions,
    takes: &[TakeInput],
    teaser: (u32, u32),
    teaser_attempts: Vec<String>,
) -> Result<Provenance, ShowcaseError> {
    let files = [VIDEO_FILE, TEASER_FILE, POSTER_FILE]
        .iter()
        .map(|name| FileRecord::of(&options.out.join(name)))
        .collect::<Result<Vec<_>, _>>()?;
    let takes = takes
        .iter()
        .map(|take| {
            let manifest = &take.manifest;
            let (p50, p95) = manifest.wall_percentiles().unwrap_or((0.0, 0.0));
            TakeRecord {
                name: manifest.take.clone(),
                adapter: manifest.adapter.name.clone(),
                driver_info: manifest.adapter.driver_info.clone(),
                mesa_release: client::render_compatibility::mesa_release(&format!(
                    "driverInfo = {}",
                    manifest.adapter.driver_info
                ))
                .map(|(major, minor)| format!("{major}.{minor}")),
                anisotropy_override: manifest.anisotropy_override.clone(),
                frames: manifest.frames.len() as u64,
                readiness_seconds: manifest.readiness_seconds,
                render_seconds: manifest.render_seconds,
                frame_wall_ms_p50: p50,
                frame_wall_ms_p95: p95,
                scenes: manifest.scenes.clone(),
            }
        })
        .collect();
    Ok(Provenance {
        revision: options.revision.clone(),
        revision_date: options.revision_date.clone(),
        run_url: options.run_url.clone(),
        encoder: encoder_version()?,
        teaser,
        teaser_attempts,
        takes,
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// A machine described by sets of paths and programs.
    struct FakeHost {
        programs: BTreeSet<&'static str>,
        files: BTreeSet<&'static str>,
        summary: Option<&'static str>,
    }

    impl Host for FakeHost {
        fn find_on_path(&self, program: &str) -> Option<PathBuf> {
            self.programs
                .contains(program)
                .then(|| PathBuf::from("/usr/bin").join(program))
        }
        fn exists(&self, path: &Path) -> bool {
            self.files.contains(path.to_string_lossy().as_ref())
        }
        fn vulkan_summary(&self, _manifest: &Path) -> Option<String> {
            self.summary.map(str::to_owned)
        }
    }

    fn arguments() -> Vec<String> {
        vec![
            "/bin/showcase".into(),
            "take".into(),
            "--take".into(),
            "battle".into(),
        ]
    }

    fn variable<'a>(plan: &'a ChildPlan, name: &str) -> Option<&'a str> {
        plan.environment
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    #[test]
    fn render_refuses_radv_without_with_gpu_lock() {
        let host = FakeHost {
            programs: ["xvfb-run"].into(),
            files: [RADV_MANIFESTS[0]].into(),
            summary: None,
        };
        let result = take_command(Driver::Radv, &host, &arguments());
        assert!(
            matches!(&result, Err(ShowcaseError::Render(reason)) if reason.contains("with-gpu-lock")),
            "{result:?}"
        );
        let host = FakeHost {
            programs: ["xvfb-run", LOCK_TOOL].into(),
            files: [RADV_MANIFESTS[0]].into(),
            summary: None,
        };
        let plan =
            take_command(Driver::Radv, &host, &arguments()).map_err(|error| error.to_string());
        let plan = plan.as_ref().map_err(String::as_str);
        assert_eq!(
            plan.map(|plan| plan.program.clone()),
            Ok(PathBuf::from("/usr/bin/with-gpu-lock"))
        );
        if let Ok(plan) = plan {
            assert_eq!(plan.arguments[0], "/usr/bin/xvfb-run");
            assert!(plan.arguments.ends_with(&arguments()));
            assert_eq!(variable(plan, "VK_ICD_FILENAMES"), Some(RADV_MANIFESTS[0]));
            assert_eq!(variable(plan, "MESA_VK_WSI_DEBUG"), Some("sw"));
            assert_eq!(variable(plan, "WGPU_BACKEND"), Some("vulkan"));
            assert_eq!(
                variable(plan, super::super::take::ANISOTROPY_VARIABLE),
                None
            );
        }
    }

    #[test]
    fn render_selects_the_lavapipe_manifest_and_override() {
        let old = FakeHost {
            programs: ["xvfb-run"].into(),
            files: [LAVAPIPE_MANIFESTS[1]].into(),
            summary: Some("driverInfo = Mesa 25.0.7 (LLVM 19.1.7)"),
        };
        let plan =
            take_command(Driver::Lavapipe, &old, &arguments()).map_err(|error| error.to_string());
        assert_eq!(
            plan.as_ref().map(|plan| plan.program.clone()),
            Ok(PathBuf::from("/usr/bin/xvfb-run"))
        );
        if let Ok(plan) = &plan {
            assert_eq!(
                variable(plan, "VK_ICD_FILENAMES"),
                Some(LAVAPIPE_MANIFESTS[1])
            );
            assert_eq!(
                variable(plan, super::super::take::ANISOTROPY_VARIABLE),
                Some("1")
            );
            assert!(plan.removed.iter().any(|name| name == "MESA_VK_WSI_DEBUG"));
            assert!(plan.arguments.ends_with(&arguments()));
        }
        let new = FakeHost {
            programs: ["xvfb-run"].into(),
            files: LAVAPIPE_MANIFESTS.into(),
            summary: Some("driverInfo = Mesa 25.2.8-0ubuntu0.24.04.4 (LLVM 20.1.2)"),
        };
        let plan =
            take_command(Driver::Lavapipe, &new, &arguments()).map_err(|error| error.to_string());
        assert!(plan.is_ok(), "{plan:?}");
        if let Ok(plan) = &plan {
            assert_eq!(
                variable(plan, "VK_ICD_FILENAMES"),
                Some(LAVAPIPE_MANIFESTS[0])
            );
            assert_eq!(
                variable(plan, super::super::take::ANISOTROPY_VARIABLE),
                None
            );
        }
        let none = FakeHost {
            programs: ["xvfb-run"].into(),
            files: BTreeSet::new(),
            summary: None,
        };
        assert!(matches!(
            take_command(Driver::Lavapipe, &none, &arguments()),
            Err(ShowcaseError::Render(_))
        ));
    }
}
