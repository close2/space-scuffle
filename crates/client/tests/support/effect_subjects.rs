//! The shipped units' splash effects with the bindings their weapons publish (plan
//! `2026-11-09-effect-visualization` §5.3, §10.2), shared by the coverage test and the effect
//! GPU tests.
//!
//! # Responsibility
//! Walks every archetype's presentation block in the shipped unit catalog, finds the weapon
//! each `splash` entry names (slot and mode) and returns the effect with the bound values a
//! `SplashVisual` of that weapon carries: the zone radii and, for a line, its length and
//! spacing as the client's event wiring computes them. No unit or effect is named here.
//!
//! # Invariants
//! - Subjects are sorted by effect name and unique per effect (the first unit naming it).

use client::effects::binding::BoundValues;
use client::effects::catalog::schema::Binding;
use sim_core::catalog::UnitArchetype;
use sim_types::{SplashShape, SplashSpec, WeaponSlot, WeaponSpec};

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

/// Every splash effect the shipped units name, with its weapon's bindings, sorted by name.
pub fn splash_subjects() -> Vec<SplashSubject> {
    let units = sim_fixtures::shipped_catalogs().units;
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
