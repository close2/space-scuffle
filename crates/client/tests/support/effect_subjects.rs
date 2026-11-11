//! The splash effects the effect GPU tests render, with the bindings their weapons publish
//! (plan `2026-11-09-effect-visualization` §5.3, §10.2, Amendments 1 and 3), shared by the coverage
//! test and the effect GPU tests.
//!
//! # Responsibility
//! - [`lab_units`]: the shipped unit catalog with the lab-only presentation entries of
//!   `tests/data/effect_lab_blocks.ron` added to its units' blocks. Those entries name effects
//!   no shipped unit can use yet (a burrowed unit's trail while the shipped burrowed mode cannot
//!   move, Amendment 2 R5): they stay in the vocabulary and under test.
//! - [`splash_subjects`]: walks every presentation block of [`lab_units`], finds the weapon
//!   each `splash` entry names (slot and mode) and returns the effect with the bound values a
//!   `SplashVisual` of that weapon carries: the zone radii and, for a line, its length and
//!   spacing as the client's event wiring computes them. No unit or effect is named here.
//!
//! # Invariants
//! - Subjects are sorted by effect name and unique per effect (the first unit naming it).
//! - A lab-only entry only adds to a unit's shipped block: an entry that repeats one of the
//!   shipped block's weapon, mode, status or ability entries, or its death effect, or a slug the
//!   catalog does not have, fails the fixture.

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

/// The shipped unit catalog with every lab-only block's entries added to its unit's block.
pub fn lab_units() -> UnitCatalog {
    let mut units = sim_fixtures::shipped_catalogs().units;
    for (slug, block) in lab_blocks() {
        let id = units
            .id_by_slug(&slug)
            .unwrap_or_else(|| panic!("lab block for unknown unit {slug}"));
        let archetype = units
            .get_mut(id)
            .expect("invariant: the id was just looked up");
        add_lab_entries(&slug, &mut archetype.effects, block);
    }
    units
}

/// Adds the lab-only entries of `lab` to `shipped`, failing on any entry the shipped block
/// already has (the lab entry would then be stale).
fn add_lab_entries(slug: &str, shipped: &mut ArchetypeEffects, lab: ArchetypeEffects) {
    for entry in lab.weapons {
        assert!(
            !shipped
                .weapons
                .iter()
                .any(|own| own.weapon == entry.weapon && own.mode == entry.mode),
            "{slug} already carries a shipped {:?} {:?} weapon entry; its lab entry is stale",
            entry.weapon,
            entry.mode
        );
        shipped.weapons.push(entry);
    }
    for entry in lab.modes {
        assert!(
            !shipped.modes.iter().any(|own| own.mode == entry.mode),
            "{slug} already carries a shipped {} mode entry; its lab entry is stale",
            entry.mode
        );
        shipped.modes.push(entry);
    }
    for (status, effect) in lab.status {
        let previous = shipped.status.insert(status, effect);
        assert!(
            previous.is_none(),
            "{slug} already carries a shipped {status:?} status entry; its lab entry is stale"
        );
    }
    for (ability, effect) in lab.abilities {
        assert!(
            !shipped.abilities.contains_key(&ability),
            "{slug} already carries a shipped {ability} ability entry; its lab entry is stale"
        );
        shipped.abilities.insert(ability, effect);
    }
    if let Some(death) = lab.death {
        assert!(
            shipped.death.is_none(),
            "{slug} already carries a shipped death effect; its lab entry is stale"
        );
        shipped.death = Some(death);
    }
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
