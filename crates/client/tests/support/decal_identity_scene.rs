//! The decal identity scene (plan `2026-11-19-retained-effect-instances` WP1, D-9, D-10), shared
//! by `decal_identity_gpu_tests` (two renders bit-identical) and `decal_age_gpu_tests` (the GPU
//! aging against the Rust reference).
//!
//! # Responsibility
//! Builds the scene's [`LabProgram`] and [`LabCrowd`] from the shipped vocabulary (no effect
//! named in Rust) and writes the vocabulary copy whose decal budget and class caps are
//! tightened. The including test binary declares the `effect_subjects` support module beside
//! this one.

use bevy::math::{UVec2, Vec2, Vec3};
use client::effects::binding::BoundValues;
use client::effects::catalog::schema::{Binding, EventKind};
use client::effects::catalog::{EffectCatalog, Pool, EFFECTS_CATALOG_PATH};
use client::effects::lab::{
    LabCrowd, LabCrowdMember, LabFogSweep, LabGround, LabProgram, LabShot, LabSubject, LabTake,
    LabZoom,
};
use client::ResolvedAssetsRoot;
use std::path::{Path, PathBuf};

/// Capture size.
pub const SIZE: UVec2 = UVec2::new(1280, 720);
/// Captures per render.
pub const SHOTS: usize = 16;
/// Length of the take, seconds.
pub const TAKE_SECONDS: f32 = 9.0;
/// Age of the first capture, seconds.
pub const FIRST_SHOT: f32 = 0.3;
/// Time step of the take, seconds (particles integrate the same steps on both renders).
pub const STEP: f32 = 0.08;
/// Grid spacing of the scene's effects, world units.
pub const SPACING: f32 = 2.6;
/// Length of a line effect's line, world units.
pub const LINE_LENGTH: f32 = 2.6;
/// Search point spacing of a line, world units.
pub const LINE_SPACING: f32 = 0.4;
/// Search points before the first one a fogged line shows.
pub const FOGGED_POINTS: u32 = 3;
/// Ages of the later waves of spawns, seconds.
pub const WAVES: [f32; 2] = [3.0, 6.0];
/// Age at which the held look is released, seconds.
pub const RELEASE_AT: f32 = 4.5;
/// The camera's distance grows by this share of the default per second.
pub const DOLLY: f32 = 0.25;
/// The tightened decal budget of the scene's vocabulary copy.
pub const DECAL_BUDGET: u32 = 160;
/// The tightened class caps (`Major`, `Minor`, `Ambient`).
pub const CLASS_CAPS: (u32, u32, u32) = (2, 6, 2);

/// The shipped vocabulary.
pub fn shipped() -> EffectCatalog {
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    EffectCatalog::load(&root.as_path().join(EFFECTS_CATALOG_PATH), root.as_path())
        .expect("the shipped vocabulary loads")
}

/// Writes the shipped vocabulary with the scene's tightened budget into `dir`; returns its path.
pub fn tightened_vocabulary(dir: &Path) -> PathBuf {
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let text = std::fs::read_to_string(root.as_path().join(EFFECTS_CATALOG_PATH))
        .expect("the shipped vocabulary reads");
    let decals = regex_replace(&text, "decals: ", &format!("decals: {DECAL_BUDGET},"));
    let caps = format!(
        "classes: {{ Major: {}, Minor: {}, Ambient: {} }},",
        CLASS_CAPS.0, CLASS_CAPS.1, CLASS_CAPS.2
    );
    let edited = regex_replace(&decals, "classes: ", &caps);
    let path = dir.join("effects.ron");
    std::fs::write(&path, edited).expect("the vocabulary copy writes");
    path
}

/// `text` with the one line that starts (after its indentation) with `key` replaced by `with`,
/// keeping the indentation. Panics unless exactly one line matches.
pub fn regex_replace(text: &str, key: &str, with: &str) -> String {
    let matching: Vec<usize> = text
        .lines()
        .enumerate()
        .filter(|(_, line)| line.trim_start().starts_with(key))
        .map(|(index, _)| index)
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "the shipped vocabulary has one `{key}` line in its budget"
    );
    text.lines()
        .enumerate()
        .map(|(index, line)| {
            if index == matching[0] {
                let indent = &line[..line.len() - line.trim_start().len()];
                format!("{indent}{with}")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The bound values a scene member is drawn with: a splash subject's own (the weapon that
/// names it), or a generic event's.
pub fn bound_of(name: &str, splashes: &[super::effect_subjects::SplashSubject]) -> BoundValues {
    if let Some(subject) = splashes.iter().find(|subject| subject.effect == name) {
        return subject.bound;
    }
    BoundValues::default()
        .with(Binding::SplashFull, 0.6)
        .with(Binding::SplashMid, 0.9)
        .with(Binding::SplashReach, 1.2)
        .with(Binding::LineLength, LINE_LENGTH)
        .with(Binding::LineSpacing, LINE_SPACING)
        .with(Binding::AreaRadius, 1.5)
        .with(Binding::AreaRemaining, 50.0)
        .with(Binding::AreaTotal, 60.0)
        .with(Binding::LinkLength, LINE_LENGTH)
        .with(Binding::UnitRadius, 0.5)
        .with(Binding::UnitHeight, 1.0)
        .with(Binding::BarrierRatio, 0.5)
}

/// A subject of effect `name` at `anchor`, with a line along +x for a line or tether event.
pub fn subject(name: &str, kind: EventKind, anchor: Vec3, bound: BoundValues) -> LabSubject {
    let line = matches!(kind, EventKind::Line | EventKind::Link | EventKind::Shot);
    let length = if kind == EventKind::Line {
        bound.get(Binding::LineLength).max(LINE_SPACING)
    } else {
        LINE_LENGTH
    };
    LabSubject {
        effect: name.to_owned(),
        bound,
        anchor,
        line_to: line.then(|| anchor + Vec3::X * length),
        line_seen: u64::MAX,
        fog_below_x: None,
        repeat: None,
    }
}

/// Whether effects of `kind` are held on a unit or a view rather than spawned once.
pub const fn held_kind(kind: EventKind) -> bool {
    matches!(
        kind,
        EventKind::Area
            | EventKind::Link
            | EventKind::Missile
            | EventKind::Status
            | EventKind::Mode
    )
}

/// The identity scene: the program's subject (the grid's first effect at the centre) and the
/// crowd.
pub fn scene(out: PathBuf, render: &str) -> (LabProgram, LabCrowd) {
    let catalog = shipped();
    let splashes = super::effect_subjects::splash_subjects();
    let mut decal_effects: Vec<(String, EventKind)> = catalog
        .effects
        .iter()
        .filter(|effect| {
            effect
                .layers
                .iter()
                .any(|layer| layer.block.pool() == Pool::Decals)
        })
        .map(|effect| (effect.name.clone(), effect.event))
        .collect();
    decal_effects.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(
        decal_effects.len() >= 4,
        "the shipped vocabulary has decal looks"
    );
    let columns = (decal_effects.len() as f32).sqrt().ceil() as usize;
    let cell = |index: usize| {
        let (column, row) = (index % columns, index / columns);
        let half = (columns as f32 - 1.0) * 0.5;
        Vec3::new(
            (column as f32 - half + 0.5) * SPACING,
            0.0,
            (row as f32 - half - 0.5) * SPACING,
        )
    };
    let member = |name: &str, kind: EventKind, anchor: Vec3, at: f32| LabCrowdMember {
        subject: subject(name, kind, anchor, bound_of(name, &splashes)),
        at,
        held: held_kind(kind),
        release_at: None,
        velocity: Vec3::ZERO,
        fog_sweep: None,
    };
    let mut members = Vec::new();
    // The grid, then two waves of the spawned looks beside their first spawn.
    for (index, (name, kind)) in decal_effects.iter().enumerate() {
        members.push(member(name, *kind, cell(index), 0.05 * index as f32));
    }
    for (wave, at) in WAVES.iter().enumerate() {
        let offset = Vec3::new(0.6, 0.0, 0.5) * (wave as f32 + 1.0);
        for (index, (name, kind)) in decal_effects.iter().enumerate() {
            if !held_kind(*kind) {
                members.push(member(
                    name,
                    *kind,
                    cell(index) + offset,
                    at + 0.03 * index as f32,
                ));
            }
        }
    }
    let first_of = |wanted: &[EventKind]| {
        decal_effects
            .iter()
            .find(|(_, kind)| wanted.contains(kind))
            .cloned()
    };
    let below = |row: f32| {
        Vec3::new(
            -2.0 * SPACING,
            0.0,
            (columns as f32 * 0.5 + row * 0.8) * SPACING,
        )
    };
    // Two lines whose first points are fogged.
    for (line, (name, kind)) in decal_effects
        .iter()
        .filter(|(_, kind)| *kind == EventKind::Line)
        .take(2)
        .enumerate()
    {
        let mut fogged = member(name, *kind, below(0.0) + Vec3::X * 4.0 * line as f32, 0.2);
        fogged.subject.line_seen = u64::MAX << FOGGED_POINTS;
        members.push(fogged);
    }
    // An area whose fog clip sweeps across it.
    if let Some((name, kind)) = first_of(&[EventKind::Area]) {
        let mut area = member(&name, kind, below(1.0), 0.1);
        let along = area.subject.anchor.x;
        area.fog_sweep = Some(LabFogSweep {
            axis: Vec2::X,
            from: along - 2.0,
            to: along + 2.0,
            speed: 0.45,
        });
        members.push(area);
    }
    // A held look released mid-take, and one that moves with its unit.
    if let Some((name, kind)) = first_of(&[EventKind::Status, EventKind::Mode]) {
        let mut released = member(&name, kind, below(1.0) + Vec3::X * 4.0, 0.1);
        released.release_at = Some(RELEASE_AT);
        members.push(released);
        let mut moving = member(&name, kind, below(1.0) + Vec3::X * 8.0, 0.1);
        moving.velocity = Vec3::new(0.0, 0.0, -0.3);
        members.push(moving);
    }
    // A tether between moving ends (any tether look: its line is what moves).
    if let Some(link) = catalog
        .effects
        .iter()
        .find(|effect| effect.event == EventKind::Link)
    {
        let mut tether = member(&link.name, EventKind::Link, below(2.0), 0.1);
        tether.velocity = Vec3::new(-0.2, 0.0, 0.0);
        members.push(tether);
    }
    // A lasting mark whose anchor moves (a held beam's burn follows the beam's end).
    if let Some((name, kind)) = first_of(&[EventKind::Splash, EventKind::Point]) {
        let mut burn = member(&name, kind, below(2.0) + Vec3::X * 6.0, 0.15);
        burn.velocity = Vec3::new(0.25, 0.0, 0.0);
        members.push(burn);
    }
    // The lab frames its subject: the first spawned look, at the scene's centre.
    let (main_name, main_kind) = first_of(&[
        EventKind::Splash,
        EventKind::Point,
        EventKind::Death,
        EventKind::Line,
    ])
    .expect("a spawned decal look");
    let main = subject(
        &main_name,
        main_kind,
        Vec3::ZERO,
        bound_of(&main_name, &splashes),
    );
    let takes = std::iter::once(render)
        .map(|render| LabTake {
            ground: LabGround::Bright,
            zoom: LabZoom::Default,
            effect: true,
            only: None,
            step: STEP,
            shots: (0..SHOTS)
                .map(|shot| LabShot {
                    age: shot_age(shot),
                    overlay: false,
                    name: format!("{render}_{shot:02}"),
                })
                .collect(),
        })
        .collect();
    let program = LabProgram {
        subject: main,
        takes,
        out,
    };
    (
        program,
        LabCrowd {
            members,
            dolly_per_second: DOLLY,
        },
    )
}

/// Age of capture `shot`, seconds.
pub fn shot_age(shot: usize) -> f32 {
    FIRST_SHOT + (TAKE_SECONDS - FIRST_SHOT) * shot as f32 / (SHOTS - 1) as f32
}
