//! The splash effects the effect GPU tests render, with the bindings their weapons publish
//! (plan `2026-11-09-effect-visualization` §5.3, §10.2, Amendment 1), shared by the coverage
//! test and the effect GPU tests.
//!
//! # Responsibility
//! - [`lab_units`]: the shipped unit catalog with the lab-only presentation blocks of
//!   `tests/data/effect_lab_blocks.ron` applied. Those blocks name the effects the review gate
//!   rejected (Amendment 1): they stay in the vocabulary and under test, but no shipped unit
//!   names them.
//! - [`splash_subjects`]: walks every presentation block of [`lab_units`], finds the weapon
//!   each `splash` entry names (slot and mode) and returns the effect with the bound values a
//!   `SplashVisual` of that weapon carries: the zone radii and, for a line, its length and
//!   spacing as the client's event wiring computes them. No unit or effect is named here.
//!
//! # Invariants
//! - Subjects are sorted by effect name and unique per effect (the first unit naming it).
//! - A lab-only block only fills a unit with no shipped block: a unit that already carries
//!   one, or a slug the catalog does not have, fails the fixture.

use client::effects::binding::BoundValues;
use client::effects::catalog::schema::Binding;
use sim_core::catalog::{ArchetypeEffects, UnitArchetype, UnitCatalog};
use sim_types::{SplashShape, SplashSpec, WeaponSlot, WeaponSpec};
use std::collections::BTreeMap;

/// A splash effect of the shipped catalog and its weapon's bindings.
#[derive(Debug, Clone)]
pub struct SplashSubject {
    /// Effect name.
    pub effect: String,
    /// Slug of the unit that names it.
    pub unit: String,
    /// Bound values.
    pub bound: BoundValues,
    /// Whether the weapon's splash is a line.
    pub line: bool,
}

/// The weapon of `archetype` in `slot`, replaced by mode `mode`'s override when named.
pub fn weapon<'a>(
    archetype: &'a UnitArchetype,
    slot: WeaponSlot,
    mode: Option<&str>,
) -> Option<&'a WeaponSpec> {
    if let Some(mode) = mode {
        return archetype
            .modes
            .iter()
            .find(|candidate| candidate.mode_name == mode)?
            .ground_weapon_override
            .as_ref()?
            .as_ref();
    }
    match slot {
        WeaponSlot::Primary => archetype.weapon.as_ref(),
        WeaponSlot::Ground => archetype.ground_weapon.as_ref(),
        WeaponSlot::Air => archetype.air_weapon.as_ref(),
    }
}

/// The values a `SplashVisual` of `splash` binds: zones as `sim_core` publishes them
/// (innermost, middle, reach), and a line's length from its first point and its spacing.
pub fn bindings(splash: &SplashSpec) -> (BoundValues, bool) {
    let radius = |index: usize| {
        splash
            .zones
            .get(index)
            .map_or(0.0, |zone| zone.radius.to_f32())
    };
    let last = splash.zones.len().saturating_sub(1);
    let mut bound = BoundValues::default()
        .with(Binding::SplashFull, radius(0))
        .with(Binding::SplashMid, radius(last / 2))
        .with(Binding::SplashReach, splash.reach().to_f32());
    let SplashShape::Line(line) = &splash.shape else {
        return (bound, false);
    };
    let offsets: Vec<f32> = line.offsets.iter().map(|offset| offset.to_f32()).collect();
    let first = offsets.first().copied().unwrap_or(0.0);
    let far = offsets.iter().copied().fold(0.0_f32, f32::max);
    let spacing = match offsets.as_slice() {
        [a, b, ..] => (b - a).abs(),
        _ => 0.0,
    };
    bound.set(Binding::LineLength, far - first);
    bound.set(Binding::LineSpacing, spacing);
    (bound, true)
}

/// The lab-only presentation blocks by unit slug (`tests/data/effect_lab_blocks.ron`).
pub fn lab_blocks() -> BTreeMap<String, ArchetypeEffects> {
    ron::from_str(include_str!("../data/effect_lab_blocks.ron"))
        .expect("effect_lab_blocks.ron parses")
}

/// The shipped unit catalog with every lab-only block applied to its unit.
pub fn lab_units() -> UnitCatalog {
    let mut units = sim_fixtures::shipped_catalogs().units;
    for (slug, block) in lab_blocks() {
        let id = units
            .id_by_slug(&slug)
            .unwrap_or_else(|| panic!("lab block for unknown unit {slug}"));
        let archetype = units
            .get_mut(id)
            .expect("invariant: the id was just looked up");
        assert!(
            archetype.effects.is_empty(),
            "{slug} already carries a shipped presentation block; its lab block is stale"
        );
        archetype.effects = block;
    }
    units
}

/// Every splash effect the units of [`lab_units`] name, with its weapon's bindings, sorted by
/// name.
pub fn splash_subjects() -> Vec<SplashSubject> {
    let units = lab_units();
    let mut subjects: Vec<SplashSubject> = Vec::new();
    for archetype in units.archetypes() {
        for entry in &archetype.effects.weapons {
            let Some(effect) = &entry.splash else {
                continue;
            };
            let Some(splash) = weapon(archetype, entry.weapon, entry.mode.as_deref())
                .and_then(|weapon| weapon.splash.as_ref())
            else {
                continue;
            };
            if subjects.iter().any(|subject| &subject.effect == effect) {
                continue;
            }
            let (bound, line) = bindings(splash);
            subjects.push(SplashSubject {
                effect: effect.clone(),
                unit: archetype.slug.clone(),
                bound,
                line,
            });
        }
    }
    subjects.sort_by(|a, b| a.effect.cmp(&b.effect));
    subjects
}
