//! Turns the authored [`EffectsFile`] into the dense [`EffectCatalog`].
//!
//! # Responsibility
//! Resolves every name to its index, checks every rule of plan §5.2 and D-12, and computes
//! what the runtime needs precomputed (an effect's lifetime, a particle layer's duration).
//!
//! # How it fits
//! Called once by [`EffectCatalog::from_text`]; nothing here runs during a battle.
//!
//! # Invariants
//! - Ids follow the files' map order (names sorted), so the same file gives the same ids.
//! - The first failure is returned; nothing partially resolved escapes.

use super::error::{EffectCatalogError, NameKind, Site};
use super::schema::{
    BlockDef, BreakupDef, EffectClass, EffectDefRaw, EffectsFile, EmitterDef, EventKind, LayerDef,
    LiquidDef, OutlineDef, PlacementDef, SpawnDef, ValueDef,
};
use super::textures::TextureAtlas;
use super::{
    Block, Breakup, Budget, CardLook, CurveId, DecalLook, Defaults, DistortionLook, Effect,
    EffectCatalog, EffectId, ElementRef, Emitter, EmitterId, Layer, Pool, RampId, RibbonLook,
    RingRadius, EFFECTS_FORMAT_VERSION, MAX_LAYERS, MAX_PLACEMENTS,
};
use crate::effects::binding::provides;
use crate::effects::curve::{Curve, Ramp};
use crate::effects::placement::MAX_INSTANCES_PER_PLACEMENT;
use bevy::math::{Vec2, Vec3};
use std::collections::BTreeMap;
use std::path::Path;

/// Resolves `file`, read from `path`, with manifests relative to `root`.
pub(super) fn resolve(
    file: &EffectsFile,
    path: &Path,
    root: &Path,
) -> Result<EffectCatalog, EffectCatalogError> {
    if file.format_version != EFFECTS_FORMAT_VERSION {
        return Err(EffectCatalogError::FormatVersion {
            path: path.to_path_buf(),
            found: file.format_version,
            expected: EFFECTS_FORMAT_VERSION,
        });
    }
    let atlas = TextureAtlas::load(root, &file.textures)?;
    let mut resolver = Resolver {
        path,
        atlas,
        ramp_ids: dense_ids(file.ramps.keys()),
        curve_ids: dense_ids(file.curves.keys()),
        emitter_ids: dense_ids(file.emitters.keys()),
        effect_ids: dense_ids(file.effects.keys()),
        emitters: Vec::new(),
    };
    let ramps = resolver.ramps(file)?;
    let curves = resolver.curves(file)?;
    for (name, emitter) in &file.emitters {
        if let Some(land) = &emitter.land {
            if file
                .effects
                .get(land)
                .is_some_and(|effect| effect.event != EventKind::Point)
            {
                return Err(EffectCatalogError::LandEffectNotPoint {
                    path: path.to_path_buf(),
                    emitter: name.clone(),
                    effect: land.clone(),
                });
            }
        }
        let resolved = resolver.emitter(name, emitter)?;
        resolver.emitters.push(resolved);
    }
    let mut effects = Vec::with_capacity(file.effects.len());
    for (name, effect) in &file.effects {
        effects.push(resolver.effect(name, effect)?);
    }
    check_status_links(path, &effects)?;
    let budget = resolver.budget(file, &effects)?;
    let defaults = resolver.defaults(file)?;
    let effect_names = resolver
        .effect_ids
        .iter()
        .map(|(name, id)| (name.clone(), EffectId(*id)))
        .collect();
    Ok(EffectCatalog {
        path: path.to_path_buf(),
        atlas: resolver.atlas,
        ramps,
        curves,
        emitters: resolver.emitters,
        effects,
        effect_names,
        budget,
        defaults,
    })
}

/// Checks that every effect a `Status` effect lays or spawns on a hit is itself a `Status`
/// effect, so it binds what the held look binds (the unit's radius, the barrier's fraction), and
/// that every effect an `Area` effect lays is an `Area` effect (it binds the area's radius).
fn check_status_links(path: &Path, effects: &[Effect]) -> Result<(), EffectCatalogError> {
    for effect in effects {
        let links = [
            ("emit", effect.emit.map(|emit| emit.effect)),
            ("hit", effect.hit.map(|hit| hit.effect)),
        ];
        for (field, linked) in links {
            let Some(linked) = linked.and_then(|id| effects.get(usize::from(id.0))) else {
                continue;
            };
            if linked.event != effect.event {
                return Err(EffectCatalogError::InvalidValue {
                    path: path.to_path_buf(),
                    site: site(&effect.name, None),
                    field,
                    value: 0.0,
                    rule: "the effect it names must be of its own kind (`Status` or `Area`)",
                });
            }
        }
    }
    Ok(())
}

/// The deepest bite a torn front may declare, as a share of its radius (`edge_noise` times
/// `thickness`): the footprint test lets its rays end inside the reach by this much, so a
/// front must not buy a large tolerance with a large declaration (Amendment 7).
pub const MAX_FRONT_BITE: f32 = 0.2;

/// Names in map order mapped to dense ids.
fn dense_ids<'a>(names: impl Iterator<Item = &'a String>) -> BTreeMap<String, u16> {
    names
        .enumerate()
        .map(|(index, name)| (name.clone(), u16::try_from(index).unwrap_or(u16::MAX)))
        .collect()
}

/// Resolution state: the file's path, the atlas and every table's name map.
struct Resolver<'a> {
    /// The vocabulary file.
    path: &'a Path,
    /// The texture sets.
    atlas: TextureAtlas,
    /// Ramp names to ids.
    ramp_ids: BTreeMap<String, u16>,
    /// Curve names to ids.
    curve_ids: BTreeMap<String, u16>,
    /// Emitter names to ids.
    emitter_ids: BTreeMap<String, u16>,
    /// Effect names to ids.
    effect_ids: BTreeMap<String, u16>,
    /// Emitters resolved so far, for particle-layer durations.
    emitters: Vec<Emitter>,
}

impl Resolver<'_> {
    /// Bakes every ramp after checking that every channel lies in 0 to 1 (D-11).
    fn ramps(&self, file: &EffectsFile) -> Result<Vec<Ramp>, EffectCatalogError> {
        file.ramps
            .iter()
            .map(|(name, keys)| {
                self.ramp_in_range(name, keys)?;
                Ramp::bake(keys).map_err(|bad| self.unordered(name, bad.0))
            })
            .collect()
    }

    /// Fails on the first channel of `keys` outside 0 to 1.
    fn ramp_in_range(
        &self,
        name: &str,
        keys: &[super::schema::RampKey],
    ) -> Result<(), EffectCatalogError> {
        for (index, (_, (red, green, blue, opacity))) in keys.iter().enumerate() {
            if let Some(value) = [*red, *green, *blue, *opacity]
                .into_iter()
                .find(|value| !(0.0..=1.0).contains(value))
            {
                return Err(EffectCatalogError::RampOutOfRange {
                    path: self.path.to_path_buf(),
                    ramp: name.to_owned(),
                    index,
                    value,
                });
            }
        }
        Ok(())
    }

    /// Builds every curve.
    fn curves(&self, file: &EffectsFile) -> Result<Vec<Curve>, EffectCatalogError> {
        file.curves
            .iter()
            .map(|(name, keys)| Curve::new(keys.clone()).map_err(|bad| self.unordered(name, bad.0)))
            .collect()
    }

    /// The error for unordered keys of `name`.
    fn unordered(&self, name: &str, index: usize) -> EffectCatalogError {
        EffectCatalogError::UnorderedKeys {
            path: self.path.to_path_buf(),
            site: site(name, None),
            index,
        }
    }

    /// Looks `name` up in the table of `kind`.
    fn id(&self, kind: NameKind, name: &str, at: &Site) -> Result<u16, EffectCatalogError> {
        let table = match kind {
            NameKind::Ramp => &self.ramp_ids,
            NameKind::Curve => &self.curve_ids,
            NameKind::Emitter => &self.emitter_ids,
            NameKind::Effect => &self.effect_ids,
        };
        table
            .get(name)
            .copied()
            .ok_or_else(|| EffectCatalogError::UnknownName {
                path: self.path.to_path_buf(),
                site: at.clone(),
                kind,
                name: name.to_owned(),
            })
    }

    /// An optional curve.
    fn curve(
        &self,
        name: Option<&String>,
        at: &Site,
    ) -> Result<Option<CurveId>, EffectCatalogError> {
        name.map(|name| self.id(NameKind::Curve, name, at).map(CurveId))
            .transpose()
    }

    /// A drawable texture element.
    fn element(&self, name: &str, at: &Site) -> Result<ElementRef, EffectCatalogError> {
        let Some(element) = self.atlas.elements.get(name) else {
            return Err(EffectCatalogError::MissingTextureElement {
                path: self.path.to_path_buf(),
                site: at.clone(),
                element: name.to_owned(),
            });
        };
        if !element.drawable {
            return Err(EffectCatalogError::UndrawableElement {
                path: self.path.to_path_buf(),
                site: at.clone(),
                element: name.to_owned(),
            });
        }
        Ok(ElementRef {
            first_layer: element.first_layer,
            frames: element.frames,
            duration_secs: element.duration_secs,
        })
    }

    /// A breakup: its noise element and checked parameters.
    fn breakup(
        &self,
        def: Option<&BreakupDef>,
        at: &Site,
    ) -> Result<Option<Breakup>, EffectCatalogError> {
        let Some(def) = def else {
            return Ok(None);
        };
        let element = self.atlas.elements.get(&def.noise);
        let Some(element) = element.filter(|element| element.noise) else {
            return Err(EffectCatalogError::NotNoiseElement {
                path: self.path.to_path_buf(),
                site: at.clone(),
                element: def.noise.clone(),
            });
        };
        self.at_least(at, "breakup.scale", def.scale, 0.01)?;
        self.unit_range(at, "breakup.amount", def.amount, 0.0)?;
        self.unit_range(at, "breakup.softness", def.softness, 0.001)?;
        self.unit_range(at, "breakup.shade", def.shade, 0.0)?;
        Ok(Some(Breakup {
            noise_layer: element.first_layer,
            scale: def.scale,
            amount: def.amount,
            softness: def.softness,
            shade: def.shade,
        }))
    }

    /// Checks a decal's disc outline against the ranges [`OutlineDef`] states.
    fn outline(&self, at: &Site, outline: OutlineDef) -> Result<(), EffectCatalogError> {
        let checks: [(&'static str, f32, bool, &'static str); 3] = [
            (
                "outline.soft",
                outline.soft,
                (0.0..=0.8).contains(&outline.soft),
                "it must be in [0, 0.8]",
            ),
            (
                "outline.edge_noise",
                outline.edge_noise,
                (0.0..=0.25).contains(&outline.edge_noise),
                "it must be in [0, 0.25]",
            ),
            (
                "outline.fill",
                outline.fill,
                (0.25..=1.0).contains(&outline.fill),
                "it must be in [0.25, 1]",
            ),
        ];
        match checks.into_iter().find(|(_, _, ok, _)| !ok) {
            None => Ok(()),
            Some((field, value, _, rule)) => Err(EffectCatalogError::InvalidValue {
                path: self.path.to_path_buf(),
                site: at.clone(),
                field,
                value,
                rule,
            }),
        }
    }

    /// Checks a decal's liquid material against the ranges [`LiquidDef`] states.
    fn liquid(&self, at: &Site, liquid: &LiquidDef) -> Result<(), EffectCatalogError> {
        let checks: [(&'static str, f32, bool, &'static str); 5] = [
            (
                "liquid.relief",
                liquid.relief,
                liquid.relief > 0.0,
                "it must be positive",
            ),
            (
                "liquid.sheen",
                liquid.sheen,
                (0.0..=4.0).contains(&liquid.sheen),
                "it must be in [0, 4]",
            ),
            (
                "liquid.gloss",
                liquid.gloss,
                liquid.gloss >= 1.0,
                "it must be at least 1",
            ),
            (
                "liquid.value_noise",
                liquid.value_noise,
                (0.0..=1.0).contains(&liquid.value_noise),
                "it must be in [0, 1]",
            ),
            (
                "liquid.dry_below",
                liquid.dry_below,
                (0.0..1.0).contains(&liquid.dry_below),
                "it must be in [0, 1)",
            ),
        ];
        match checks.into_iter().find(|(_, _, ok, _)| !ok) {
            None => Ok(()),
            Some((field, value, _, rule)) => Err(EffectCatalogError::InvalidValue {
                path: self.path.to_path_buf(),
                site: at.clone(),
                field,
                value,
                rule,
            }),
        }
    }

    /// Largest decal width: a mark four times as wide as long, for a texture whose mark is a
    /// narrow band across a square element (the spine line's torn-earth scar, review round 4).
    const MAX_DECAL_WIDTH: f32 = 4.0;

    /// Checks a decal's `width` against (0, [`Self::MAX_DECAL_WIDTH`]].
    fn decal_width(&self, at: &Site, width: f32) -> Result<(), EffectCatalogError> {
        if width > 0.0 && width <= Self::MAX_DECAL_WIDTH {
            return Ok(());
        }
        Err(EffectCatalogError::InvalidValue {
            path: self.path.to_path_buf(),
            site: at.clone(),
            field: "width",
            value: width,
            rule: "it must be in (0, 4]",
        })
    }

    /// Checks that `value` lies in `(0, 1]`.
    fn positive_fraction(
        &self,
        at: &Site,
        field: &'static str,
        value: f32,
    ) -> Result<(), EffectCatalogError> {
        if value > 0.0 && value <= 1.0 {
            return Ok(());
        }
        Err(EffectCatalogError::InvalidValue {
            path: self.path.to_path_buf(),
            site: at.clone(),
            field,
            value,
            rule: "it must be in (0, 1]",
        })
    }

    /// Checks that `value` lies in `[min, 1]`.
    fn unit_range(
        &self,
        at: &Site,
        field: &'static str,
        value: f32,
        min: f32,
    ) -> Result<(), EffectCatalogError> {
        if (min..=1.0).contains(&value) {
            return Ok(());
        }
        Err(EffectCatalogError::InvalidValue {
            path: self.path.to_path_buf(),
            site: at.clone(),
            field,
            value,
            rule: "it must lie between its minimum and 1",
        })
    }

    /// Checks that `value` is finite and at least `min`.
    fn at_least(
        &self,
        at: &Site,
        field: &'static str,
        value: f32,
        min: f32,
    ) -> Result<(), EffectCatalogError> {
        if value.is_finite() && value >= min {
            return Ok(());
        }
        Err(EffectCatalogError::InvalidValue {
            path: self.path.to_path_buf(),
            site: at.clone(),
            field,
            value,
            rule: "it must be finite and not below its minimum",
        })
    }

    /// Resolves one emitter.
    fn emitter(&self, name: &str, emitter: &EmitterDef) -> Result<Emitter, EffectCatalogError> {
        let at = site(name, None);
        let SpawnDef::Burst { count } = emitter.spawn;
        for (field, value) in [
            ("speed", emitter.speed.0),
            ("speed", emitter.speed.1),
            ("lifetime", emitter.lifetime.0),
            ("size", emitter.size.0),
            ("size", emitter.size.1),
            ("cone_deg", emitter.cone_deg),
            ("drag", emitter.drag),
        ] {
            self.at_least(&at, field, value, 0.0)?;
        }
        self.at_least(
            &at,
            "lifetime",
            emitter.lifetime.1,
            emitter.lifetime.0.max(0.001),
        )?;
        Ok(Emitter {
            count,
            shape: emitter.shape,
            speed: emitter.speed,
            cone: emitter.cone_deg.to_radians(),
            gravity: emitter.gravity,
            drag: emitter.drag,
            lifetime: emitter.lifetime,
            size: emitter.size,
            element: self.element(&emitter.texture, &at)?,
            orient: emitter.orient,
            ramp: RampId(self.id(NameKind::Ramp, &emitter.ramp, &at)?),
            size_curve: self.curve(emitter.size_curve.as_ref(), &at)?,
            curve: self.curve(emitter.curve.as_ref(), &at)?,
            rotation: emitter.rotation,
            roll: emitter.roll_deg.to_radians(),
            land: emitter
                .land
                .as_ref()
                .map(|land| self.id(NameKind::Effect, land, &at).map(EffectId))
                .transpose()?,
        })
    }

    /// Resolves one effect.
    fn effect(&self, name: &str, effect: &EffectDefRaw) -> Result<Effect, EffectCatalogError> {
        if effect.layers.len() > MAX_LAYERS {
            return Err(EffectCatalogError::TooManyLayers {
                path: self.path.to_path_buf(),
                effect: name.to_owned(),
                count: effect.layers.len(),
                limit: MAX_LAYERS,
            });
        }
        let mut layers = Vec::with_capacity(effect.layers.len());
        for (index, layer) in effect.layers.iter().enumerate() {
            layers.push(self.layer(&site(name, Some(index)), effect.event, layer)?);
        }
        let lifetime = layers
            .iter()
            .filter(|layer| !layer.held)
            .map(|layer| layer.start + layer.duration)
            .fold(0.0_f32, f32::max);
        let held_exit = layers
            .iter()
            .filter(|layer| layer.held)
            .map(|layer| layer.exit)
            .fold(0.0_f32, f32::max);
        if let Some(span) = effect.span {
            self.positive_seconds(&site(name, None), "span", span)?;
            if effect.event != EventKind::Mode {
                return Err(EffectCatalogError::InvalidValue {
                    path: self.path.to_path_buf(),
                    site: site(name, None),
                    field: "span",
                    value: span,
                    rule: "only a `Mode` effect shows a mode switch's morph",
                });
            }
        }
        if let Some(repeat) = effect.repeat {
            // The negated comparison also refuses NaN.
            if !(repeat > 0.0 && repeat.is_finite()) {
                return Err(EffectCatalogError::InvalidValue {
                    path: self.path.to_path_buf(),
                    site: site(name, None),
                    field: "repeat",
                    value: repeat,
                    rule: "it must be a positive number of seconds",
                });
            }
        }
        let (emit, hit) = self.status_links(name, effect)?;
        Ok(Effect {
            name: name.to_owned(),
            class: effect.class,
            event: effect.event,
            layers,
            lifetime,
            held_exit,
            span: effect.span,
            repeat: effect.repeat,
            salt: effect.seed.unwrap_or_else(|| name_hash(name)),
            emit,
            hit,
        })
    }

    /// Resolves what a `Status` effect lays while it is held and spawns on a hit (plan WP9);
    /// any other effect may have neither. Each names a `Status` effect (checked once every
    /// effect is resolved, [`check_status_links`]) and a positive beat.
    fn status_links(
        &self,
        name: &str,
        effect: &EffectDefRaw,
    ) -> Result<(Option<super::Emit>, Option<super::Hit>), EffectCatalogError> {
        let at = site(name, None);
        // An area lays its own again and again too (a storm's flares, WP10); only a unit's
        // status is hit.
        for (field, present, allowed) in [
            (
                "emit",
                effect.emit.is_some(),
                matches!(effect.event, EventKind::Status | EventKind::Area),
            ),
            (
                "hit",
                effect.hit.is_some(),
                effect.event == EventKind::Status,
            ),
        ] {
            if present && !allowed {
                return Err(EffectCatalogError::InvalidValue {
                    path: self.path.to_path_buf(),
                    site: at,
                    field,
                    value: 0.0,
                    rule: "only a `Status` effect may lay or spawn another, and an `Area` \
                           effect lay one",
                });
            }
        }
        let emit = match &effect.emit {
            Some(emit) => {
                self.positive_seconds(&at, "emit.every", emit.every)?;
                if emit.moving && effect.event == EventKind::Area {
                    return Err(EffectCatalogError::InvalidValue {
                        path: self.path.to_path_buf(),
                        site: at,
                        field: "emit.moving",
                        value: 1.0,
                        rule: "an area does not move",
                    });
                }
                Some(super::Emit {
                    effect: EffectId(self.id(NameKind::Effect, &emit.effect, &at)?),
                    every: emit.every,
                    moving: emit.moving,
                })
            }
            None => None,
        };
        let hit = match &effect.hit {
            Some(hit) => {
                self.positive_seconds(&at, "hit.every", hit.every)?;
                Some(super::Hit {
                    effect: EffectId(self.id(NameKind::Effect, &hit.effect, &at)?),
                    every: hit.every,
                })
            }
            None => None,
        };
        Ok((emit, hit))
    }

    /// Checks that `value` is a positive, finite number of seconds.
    fn positive_seconds(
        &self,
        at: &Site,
        field: &'static str,
        value: f32,
    ) -> Result<(), EffectCatalogError> {
        // The negated comparison also refuses NaN.
        if value > 0.0 && value.is_finite() {
            return Ok(());
        }
        Err(EffectCatalogError::InvalidValue {
            path: self.path.to_path_buf(),
            site: at.clone(),
            field,
            value,
            rule: "it must be a positive number of seconds",
        })
    }

    /// Checks that a layer of an effect of kind `event` may hold or modulate the bubble: a held
    /// layer belongs to an effect held while a fact is published (a unit's `Mode` or `Status`,
    /// an `Area` or a `Link`, WP10), and a `Shell` to one held on a unit.
    fn unit_bound(
        &self,
        at: &Site,
        event: EventKind,
        layer: &LayerDef,
        block: &Block,
    ) -> Result<(), EffectCatalogError> {
        let shell = matches!(block, Block::Shell(_));
        let field = if layer.held {
            "held"
        } else if shell {
            "block"
        } else {
            return Ok(());
        };
        // A unit's stance and status shape its bubble; an area or a tether is held while the
        // simulation publishes it (WP10) but has no bubble to shape.
        let allowed = if shell {
            matches!(event, EventKind::Mode | EventKind::Status)
        } else {
            matches!(
                event,
                EventKind::Mode | EventKind::Status | EventKind::Area | EventKind::Link
            )
        };
        if !allowed {
            return Err(EffectCatalogError::InvalidValue {
                path: self.path.to_path_buf(),
                site: at.clone(),
                field,
                value: 0.0,
                rule: "only a `Mode`, `Status`, `Area` or `Link` effect is held and may hold a \
                       layer, and only a `Mode` or `Status` effect may shape a bubble",
            });
        }
        if matches!(block, Block::Particles(_)) && layer.held {
            return Err(EffectCatalogError::InvalidValue {
                path: self.path.to_path_buf(),
                site: at.clone(),
                field,
                value: 0.0,
                rule: "a particle burst fires once and cannot be held",
            });
        }
        self.positive_seconds(at, "exit", layer.exit)
    }

    /// Resolves one layer of an effect of kind `event`. A line placement needs the event's
    /// line, which a `Link` effect has of its own (WP10: a grip at each end of a tether).
    fn layer(
        &self,
        at: &Site,
        event: EventKind,
        layer: &LayerDef,
    ) -> Result<Layer, EffectCatalogError> {
        let block = self.block(at, &layer.block)?;
        self.at_least(at, "start", layer.start, 0.0)?;
        let duration = if let Block::Particles(id) = &block {
            self.emitters
                .get(usize::from(id.0))
                .map_or(0.0, |emitter| emitter.lifetime.1)
        } else {
            self.at_least(at, "duration", layer.duration, 0.001)?;
            layer.duration
        };
        let placements = self.placements(at, layer)?;
        let size = layer.size.unwrap_or(ValueDef {
            bind: super::schema::Binding::Const,
            scale: 1.0,
        });
        let mut values = vec![size];
        if let Some(radius) = &layer.radius {
            values.extend([radius.from, radius.to]);
        }
        if let Block::Ring { arc: Some(arc), .. } = &block {
            values.push(*arc);
        }
        self.unit_bound(at, event, layer, &block)?;
        self.fraction(at, "size_jitter", layer.size_jitter)?;
        if let Some(strength) = layer.strength {
            self.unit_range(at, "strength.floor", strength.floor, 0.0)?;
            self.require(at, event, strength.bind)?;
        }
        for placement in &placements {
            self.placement_values(at, event, placement, &mut values)?;
        }
        for value in &values {
            self.require(at, event, value.bind)?;
        }
        self.ribbon_spans_line(at, event, &block, &placements)?;
        if matches!(block, Block::Ring { .. }) && layer.radius.is_none() {
            return Err(EffectCatalogError::InvalidValue {
                path: self.path.to_path_buf(),
                site: at.clone(),
                field: "radius",
                value: 0.0,
                rule: "a ring needs `radius: (from: .., to: ..)`",
            });
        }
        let radius = match &layer.radius {
            Some(radius) => Some(RingRadius {
                from: radius.from,
                to: radius.to,
                curve: self.curve(radius.curve.as_ref(), at)?,
            }),
            None => None,
        };
        Ok(Layer {
            block,
            start: layer.start,
            duration,
            curve: self.curve(layer.curve.as_ref(), at)?,
            priority: layer.priority,
            size,
            size_curve: self.curve(layer.size_curve.as_ref(), at)?,
            ramp_curve: self.curve(layer.ramp_curve.as_ref(), at)?,
            size_jitter: layer.size_jitter,
            radius,
            placements,
            clip_to_line: matches!(
                layer.block,
                BlockDef::Particles {
                    clip_to_line: true,
                    ..
                }
            ),
            held: layer.held,
            exit: layer.exit,
            strength: layer.strength,
        })
    }

    /// Checks one placement of a layer of an effect of kind `event`, adding the values it binds
    /// to `values`.
    fn placement_values(
        &self,
        at: &Site,
        event: EventKind,
        placement: &PlacementDef,
        values: &mut Vec<ValueDef>,
    ) -> Result<(), EffectCatalogError> {
        match placement {
            PlacementDef::Point => {}
            PlacementDef::LineEnd { back } => {
                values.push(*back);
                self.require_line(at, event, super::schema::Binding::LineLength)?;
            }
            PlacementDef::Disc { radius, .. } | PlacementDef::Circle { radius, .. } => {
                values.push(*radius);
            }
            PlacementDef::Line {
                spacing,
                per_point,
                scatter,
                delay_jitter,
                ..
            } => {
                values.push(*spacing);
                self.require_line(at, event, super::schema::Binding::LineSpacing)?;
                if *per_point == 0 || *per_point > MAX_INSTANCES_PER_PLACEMENT {
                    return Err(EffectCatalogError::TooManyInstances {
                        path: self.path.to_path_buf(),
                        site: at.clone(),
                        count: *per_point,
                        limit: MAX_INSTANCES_PER_PLACEMENT,
                    });
                }
                self.at_least(at, "scatter", *scatter, 0.0)?;
                self.at_least(at, "delay_jitter", *delay_jitter, 0.0)?;
            }
        }
        Ok(())
    }

    /// Checks that an effect of kind `event` has a line: an event line providing `binding`, or a
    /// tether's own line from its caster's bubble to its target's (WP10).
    fn require_line(
        &self,
        at: &Site,
        event: EventKind,
        binding: super::schema::Binding,
    ) -> Result<(), EffectCatalogError> {
        if event == EventKind::Link {
            return Ok(());
        }
        self.require(at, event, binding)
    }

    /// Checks that a ribbon layer spans its event's line once: a `Line` or `Link` event and
    /// only the `Point` placement. Other blocks pass.
    fn ribbon_spans_line(
        &self,
        at: &Site,
        event: EventKind,
        block: &Block,
        placements: &[PlacementDef],
    ) -> Result<(), EffectCatalogError> {
        if !matches!(block, Block::Ribbon(_)) {
            return Ok(());
        }
        self.require_line(at, event, super::schema::Binding::LineLength)?;
        if placements
            .iter()
            .any(|placement| *placement != PlacementDef::Point)
        {
            return Err(EffectCatalogError::InvalidValue {
                path: self.path.to_path_buf(),
                site: at.clone(),
                field: "placement",
                value: 0.0,
                rule: "a ribbon spans its line once: only the `Point` placement",
            });
        }
        Ok(())
    }

    /// Checks that `value` lies in `[0, 1)`.
    fn fraction(
        &self,
        at: &Site,
        field: &'static str,
        value: f32,
    ) -> Result<(), EffectCatalogError> {
        if (0.0..1.0).contains(&value) {
            return Ok(());
        }
        Err(EffectCatalogError::InvalidValue {
            path: self.path.to_path_buf(),
            site: at.clone(),
            field,
            value,
            rule: "it must be in [0, 1)",
        })
    }

    /// Fails unless events of kind `event` provide `binding`.
    fn require(
        &self,
        at: &Site,
        event: EventKind,
        binding: super::schema::Binding,
    ) -> Result<(), EffectCatalogError> {
        if provides(event, binding) {
            return Ok(());
        }
        Err(EffectCatalogError::UnboundBinding {
            path: self.path.to_path_buf(),
            site: at.clone(),
            binding,
            event,
        })
    }

    /// The layer's placements, checked against D-12.
    fn placements(
        &self,
        at: &Site,
        layer: &LayerDef,
    ) -> Result<Vec<PlacementDef>, EffectCatalogError> {
        let mut placements: Vec<PlacementDef> = layer.placement.iter().copied().collect();
        placements.extend(layer.placements.iter().copied());
        if placements.is_empty() {
            placements.push(PlacementDef::Point);
        }
        if placements.len() > MAX_PLACEMENTS {
            return Err(EffectCatalogError::TooManyPlacements {
                path: self.path.to_path_buf(),
                site: at.clone(),
                count: placements.len(),
                limit: MAX_PLACEMENTS,
            });
        }
        for placement in &placements {
            if let PlacementDef::Circle { spread, .. } = placement {
                if !(0.0..=0.5).contains(spread) {
                    return Err(EffectCatalogError::InvalidValue {
                        path: self.path.to_path_buf(),
                        site: at.clone(),
                        field: "spread",
                        value: *spread,
                        rule: "it must be in [0, 0.5]",
                    });
                }
            }
            if let PlacementDef::Disc { count, .. } | PlacementDef::Circle { count, .. } = placement
            {
                if *count > MAX_INSTANCES_PER_PLACEMENT || *count == 0 {
                    return Err(EffectCatalogError::TooManyInstances {
                        path: self.path.to_path_buf(),
                        site: at.clone(),
                        count: *count,
                        limit: MAX_INSTANCES_PER_PLACEMENT,
                    });
                }
            }
        }
        Ok(placements)
    }

    /// Resolves a block's names.
    fn block(&self, at: &Site, block: &BlockDef) -> Result<Block, EffectCatalogError> {
        Ok(match block {
            BlockDef::Sprite {
                texture,
                ramp,
                orient,
                soft_depth,
                frame_blend,
                rotation,
                erosion,
                drift,
                fps,
                frame_curve,
                lift,
                lift_curve,
                lean_deg,
                spread,
                sort_bias,
                clip_to_line,
                shape,
                breakup,
            } => {
                self.at_least(at, "soft_depth", *soft_depth, 0.0)?;
                self.at_least(at, "lean_deg", *lean_deg, 0.0)?;
                self.at_least(at, "spread", *spread, 0.0)?;
                if let Some(fps) = fps {
                    self.at_least(at, "fps", *fps, 0.001)?;
                }
                if let Some(lift) = lift {
                    self.at_least(at, "lift", *lift, -1.0)?;
                }
                Block::Sprite(CardLook {
                    element: self.element(texture, at)?,
                    ramp: RampId(self.id(NameKind::Ramp, ramp, at)?),
                    orient: *orient,
                    soft_depth: *soft_depth,
                    frame_blend: *frame_blend,
                    rotation: *rotation,
                    erosion: self.curve(erosion.as_ref(), at)?,
                    drift: Vec3::new(drift.0, drift.1, drift.2),
                    fps: *fps,
                    frame_curve: self.curve(frame_curve.as_ref(), at)?,
                    lift: *lift,
                    mapping: super::schema::RingMapping::Card,
                    lift_curve: self.curve(lift_curve.as_ref(), at)?,
                    lean: lean_deg.to_radians(),
                    spread: *spread,
                    sort_bias: *sort_bias,
                    clip_to_line: *clip_to_line,
                    shape: *shape,
                    breakup: self.breakup(breakup.as_ref(), at)?,
                })
            }
            BlockDef::Ring { .. } | BlockDef::Decal { .. } => self.masked_block(at, block)?,
            BlockDef::Ribbon { .. } => self.ribbon(at, block)?,
            BlockDef::Distortion { .. } => self.distortion_block(at, block)?,
            BlockDef::Shell { .. } => self.shell(at, block)?,
            BlockDef::Light {
                color,
                intensity,
                height,
            } => {
                self.at_least(at, "intensity", *intensity, 0.0)?;
                self.at_least(at, "height", *height, 0.0)?;
                Block::Light {
                    color: Vec3::new(color.0, color.1, color.2),
                    intensity: *intensity,
                    height: *height,
                }
            }
            BlockDef::Particles { emitter, .. } => {
                Block::Particles(EmitterId(self.id(NameKind::Emitter, emitter, at)?))
            }
        })
    }

    /// Resolves a ring block.
    fn ring_block(&self, at: &Site, ring: &RingFields<'_>) -> Result<Block, EffectCatalogError> {
        let RingFields {
            texture,
            ramp,
            orient,
            thickness,
            erosion,
            mapping,
            height,
            sort_bias,
            breakup,
            edge_noise,
        } = *ring;
        self.at_least(at, "height", *height, 0.0)?;
        self.fraction(at, "edge_noise", *edge_noise)?;
        if edge_noise * thickness > MAX_FRONT_BITE {
            return Err(EffectCatalogError::InvalidValue {
                path: self.path.to_path_buf(),
                site: at.clone(),
                field: "edge_noise",
                value: *edge_noise,
                rule: "a front's bite (edge_noise times thickness) is at most 0.2 of its radius",
            });
        }
        if *edge_noise > 0.0 && *mapping != super::schema::RingMapping::Front {
            return Err(EffectCatalogError::InvalidValue {
                path: self.path.to_path_buf(),
                site: at.clone(),
                field: "edge_noise",
                value: *edge_noise,
                rule: "only a `Front` ring has a leading edge to break",
            });
        }
        let breakup = self.breakup(breakup.as_ref(), at)?;
        if *mapping == super::schema::RingMapping::Front && breakup.is_none() {
            return Err(EffectCatalogError::FrontWithoutBreakup {
                path: self.path.to_path_buf(),
                site: at.clone(),
            });
        }
        self.positive_fraction(at, "thickness", *thickness)?;
        Ok(Block::Ring {
            look: CardLook {
                element: self.element(texture, at)?,
                ramp: RampId(self.id(NameKind::Ramp, ramp, at)?),
                orient: *orient,
                soft_depth: 0.0,
                frame_blend: true,
                rotation: super::schema::RotationDef::Fixed,
                erosion: self.curve(erosion.as_ref(), at)?,
                drift: Vec3::ZERO,
                fps: None,
                frame_curve: None,
                lift: None,
                mapping: *mapping,
                lift_curve: None,
                lean: 0.0,
                spread: 0.0,
                sort_bias: *sort_bias,
                clip_to_line: false,
                shape: super::schema::CardShape::Texture,
                breakup,
            },
            thickness: *thickness,
            height: *height,
            edge_noise: *edge_noise,
            arc: None,
            waver: 0.0,
            darken: false,
        })
    }

    /// Resolves a distortion block definition, with its lens bend.
    fn distortion_block(&self, at: &Site, block: &BlockDef) -> Result<Block, EffectCatalogError> {
        let BlockDef::Distortion {
            noise,
            scale,
            scroll,
            strength,
            orient,
            height,
            lens,
        } = block
        else {
            return self.block(at, block);
        };
        if !(0.0..=0.25).contains(lens) {
            return Err(EffectCatalogError::InvalidValue {
                path: self.path.to_path_buf(),
                site: at.clone(),
                field: "lens",
                value: *lens,
                rule: "it must be in [0, 0.25]",
            });
        }
        Ok(
            match self.distortion(at, noise, *scale, *scroll, *strength, *orient, *height)? {
                Block::Distortion(look) => Block::Distortion(DistortionLook {
                    lens: *lens,
                    ..look
                }),
                other => other,
            },
        )
    }

    /// Resolves a ring block definition, with its arc.
    fn ring_definition(&self, at: &Site, block: &BlockDef) -> Result<Block, EffectCatalogError> {
        let BlockDef::Ring {
            texture,
            ramp,
            orient,
            thickness,
            erosion,
            mapping,
            height,
            sort_bias,
            breakup,
            edge_noise,
            arc,
            waver,
            darken,
        } = block
        else {
            return self.block(at, block);
        };
        if *darken && *orient != super::schema::Orient::Ground {
            return Err(EffectCatalogError::InvalidValue {
                path: self.path.to_path_buf(),
                site: at.clone(),
                field: "darken",
                value: 1.0,
                rule: "only a ring lying on the ground (`orient: Ground`) can darken the ground",
            });
        }
        self.fraction(at, "waver", *waver)?;
        if *waver > 0.0 && (*mapping == super::schema::RingMapping::Front || breakup.is_none()) {
            return Err(EffectCatalogError::InvalidValue {
                path: self.path.to_path_buf(),
                site: at.clone(),
                field: "waver",
                value: *waver,
                rule:
                    "a wavering band needs `Card` or `Polar` mapping and breakup noise to waver by",
            });
        }
        let block = self.ring_block(
            at,
            &RingFields {
                texture,
                ramp,
                orient,
                thickness,
                erosion,
                mapping,
                height,
                sort_bias,
                breakup,
                edge_noise,
            },
        )?;
        if arc.is_some() && *mapping == super::schema::RingMapping::Front {
            return Err(EffectCatalogError::InvalidValue {
                path: self.path.to_path_buf(),
                site: at.clone(),
                field: "arc",
                value: 0.0,
                rule: "a pressure front is a whole ring: an arc needs `Card` or `Polar` mapping",
            });
        }
        Ok(match block {
            Block::Ring {
                look,
                thickness,
                height,
                edge_noise,
                ..
            } => Block::Ring {
                look,
                thickness,
                height,
                edge_noise,
                arc: *arc,
                waver: *waver,
                darken: *darken,
            },
            other => other,
        })
    }

    /// Resolves a shell modulation, every value checked against its range.
    fn shell(&self, at: &Site, block: &BlockDef) -> Result<Block, EffectCatalogError> {
        let BlockDef::Shell {
            sink,
            tint,
            rim,
            veil,
            soil,
            progress,
            clip_sink,
            matte,
            shadow,
            pulse,
        } = *block
        else {
            return self.block(at, block);
        };
        if let Some(pulse) = pulse {
            self.positive_seconds(at, "pulse.period", pulse.period)?;
            self.unit_range(at, "pulse.depth", pulse.depth, 0.0)?;
        }
        let fields: [(&'static str, f32); 14] = [
            ("sink", sink),
            ("clip_sink", clip_sink.unwrap_or(0.0)),
            ("matte", matte),
            ("shadow", shadow),
            ("tint", tint.0),
            ("tint", tint.1),
            ("tint", tint.2),
            ("tint", tint.3),
            ("rim", rim.0),
            ("rim", rim.1),
            ("rim", rim.2),
            ("rim", rim.3),
            ("veil", veil),
            ("soil", soil),
        ];
        for (field, value) in fields {
            self.unit_range(at, field, value, 0.0)?;
        }
        Ok(Block::Shell(super::ShellLook {
            sink,
            tint: bevy::math::Vec4::new(tint.0, tint.1, tint.2, tint.3),
            rim: bevy::math::Vec4::new(rim.0, rim.1, rim.2, rim.3),
            veil,
            soil,
            progress,
            clip_sink,
            matte,
            shadow,
            pulse,
        }))
    }

    /// Resolves a ring or a decal.
    fn masked_block(&self, at: &Site, block: &BlockDef) -> Result<Block, EffectCatalogError> {
        match block {
            BlockDef::Ring { .. } => self.ring_definition(at, block),
            BlockDef::Decal {
                texture,
                ramp,
                erosion,
                depth,
                grazing,
                yaw,
                width,
                clip_to_line,
                clip_feather,
                breakup,
                liquid,
                outline,
                darken,
            } => {
                self.at_least(at, "depth", *depth, 0.01)?;
                if *darken && liquid.is_some() {
                    return Err(EffectCatalogError::InvalidValue {
                        path: self.path.to_path_buf(),
                        site: at.clone(),
                        field: "darken",
                        value: 1.0,
                        rule: "a liquid catches light and cannot only darken the ground",
                    });
                }
                self.at_least(at, "clip_feather", *clip_feather, 0.005)?;
                if let Some(liquid) = liquid {
                    self.liquid(at, liquid)?;
                }
                if let Some(outline) = outline {
                    self.outline(at, *outline)?;
                }
                self.decal_width(at, *width)?;
                if !(0.0..1.0).contains(grazing) {
                    return Err(EffectCatalogError::InvalidValue {
                        path: self.path.to_path_buf(),
                        site: at.clone(),
                        field: "grazing",
                        value: *grazing,
                        rule: "it must be in [0, 1)",
                    });
                }
                Ok(Block::Decal(DecalLook {
                    element: self.element(texture, at)?,
                    ramp: RampId(self.id(NameKind::Ramp, ramp, at)?),
                    erosion: self.curve(erosion.as_ref(), at)?,
                    depth: *depth,
                    grazing: *grazing,
                    yaw: *yaw,
                    width: *width,
                    clip_to_line: *clip_to_line,
                    clip_feather: *clip_feather,
                    breakup: self.breakup(breakup.as_ref(), at)?,
                    liquid: *liquid,
                    outline: *outline,
                    darken: *darken,
                }))
            }
            BlockDef::Sprite { .. }
            | BlockDef::Ribbon { .. }
            | BlockDef::Distortion { .. }
            | BlockDef::Light { .. }
            | BlockDef::Particles { .. }
            | BlockDef::Shell { .. } => self.block(at, block),
        }
    }

    /// Resolves a distortion disc.
    #[expect(
        clippy::too_many_arguments,
        reason = "the fields of one block definition, each checked on its own"
    )]
    fn distortion(
        &self,
        at: &Site,
        noise: &str,
        scale: f32,
        scroll: (f32, f32),
        strength: f32,
        orient: super::schema::Orient,
        height: f32,
    ) -> Result<Block, EffectCatalogError> {
        let Some(element) = self
            .atlas
            .elements
            .get(noise)
            .filter(|element| element.noise)
        else {
            return Err(EffectCatalogError::NotNoiseElement {
                path: self.path.to_path_buf(),
                site: at.clone(),
                element: noise.to_owned(),
            });
        };
        self.at_least(at, "scale", scale, 0.01)?;
        self.at_least(at, "height", height, 0.0)?;
        // The negated comparison also refuses NaN.
        if !(strength > 0.0 && strength <= 0.25) {
            return Err(EffectCatalogError::InvalidValue {
                path: self.path.to_path_buf(),
                site: at.clone(),
                field: "strength",
                value: strength,
                rule: "it must be in (0, 0.25]",
            });
        }
        for value in [scroll.0, scroll.1] {
            if !value.is_finite() {
                return Err(EffectCatalogError::InvalidValue {
                    path: self.path.to_path_buf(),
                    site: at.clone(),
                    field: "scroll",
                    value,
                    rule: "it must be a finite number of repeats per second",
                });
            }
        }
        let ground = match orient {
            super::schema::Orient::Camera => false,
            super::schema::Orient::Ground => true,
            super::schema::Orient::VerticalAlongLine | super::schema::Orient::Upright => {
                return Err(EffectCatalogError::InvalidValue {
                    path: self.path.to_path_buf(),
                    site: at.clone(),
                    field: "orient",
                    value: 0.0,
                    rule:
                        "a distortion faces the camera (`Camera`) or lies on the ground (`Ground`)",
                });
            }
        };
        Ok(Block::Distortion(DistortionLook {
            noise_layer: element.first_layer,
            scale,
            scroll: Vec2::new(scroll.0, scroll.1),
            strength,
            ground,
            height,
            lens: 0.0,
        }))
    }

    /// Resolves a ribbon.
    fn ribbon(&self, at: &Site, block: &BlockDef) -> Result<Block, EffectCatalogError> {
        let BlockDef::Ribbon {
            texture,
            ramp,
            orient,
            repeat,
            scroll,
            travel,
            tail_delay,
            taper,
            height,
            soft_depth,
            erosion,
            sort_bias,
            shape,
            breakup,
        } = block
        else {
            return self.block(at, block);
        };
        if *shape == super::schema::CardShape::Membrane {
            return Err(EffectCatalogError::InvalidValue {
                path: self.path.to_path_buf(),
                site: at.clone(),
                field: "shape",
                value: 0.0,
                rule: "a membrane is the outline of a body, which only a `Sprite` draws",
            });
        }
        self.at_least(at, "repeat", *repeat, 0.001)?;
        self.at_least(at, "travel", *travel, 0.0)?;
        self.at_least(at, "height", *height, 0.0)?;
        self.at_least(at, "soft_depth", *soft_depth, 0.0)?;
        if let Some(delay) = tail_delay {
            self.at_least(at, "tail_delay", *delay, 0.0)?;
        }
        for value in [taper.0, taper.1] {
            if !(0.0..=0.5).contains(&value) {
                return Err(EffectCatalogError::InvalidValue {
                    path: self.path.to_path_buf(),
                    site: at.clone(),
                    field: "taper",
                    value,
                    rule: "each end's taper must be in [0, 0.5]",
                });
            }
        }
        let ground = match orient {
            super::schema::Orient::Camera => false,
            super::schema::Orient::Ground => true,
            super::schema::Orient::VerticalAlongLine | super::schema::Orient::Upright => {
                return Err(EffectCatalogError::InvalidValue {
                    path: self.path.to_path_buf(),
                    site: at.clone(),
                    field: "orient",
                    value: 0.0,
                    rule: "a ribbon faces the camera (`Camera`) or lies on the ground (`Ground`)",
                });
            }
        };
        if !scroll.is_finite() {
            return Err(EffectCatalogError::InvalidValue {
                path: self.path.to_path_buf(),
                site: at.clone(),
                field: "scroll",
                value: *scroll,
                rule: "it must be a finite number of repeats per second",
            });
        }
        Ok(Block::Ribbon(RibbonLook {
            element: self.element(texture, at)?,
            ramp: RampId(self.id(NameKind::Ramp, ramp, at)?),
            ground,
            repeat: *repeat,
            scroll: *scroll,
            travel: *travel,
            tail_delay: *tail_delay,
            taper: *taper,
            height: *height,
            soft_depth: *soft_depth,
            erosion: self.curve(erosion.as_ref(), at)?,
            sort_bias: *sort_bias,
            shape: *shape,
            breakup: self.breakup(breakup.as_ref(), at)?,
        }))
    }

    /// The budget, checked against what the effects use.
    fn budget(&self, file: &EffectsFile, effects: &[Effect]) -> Result<Budget, EffectCatalogError> {
        let def = &file.budget;
        let budget = Budget {
            particles: def.particles,
            sprites: def.sprites,
            decals: def.decals,
            lights: def.lights,
            distortions: def.distortions,
            effects: def.effects,
            class_caps: EffectClass::ALL.map(|class| def.classes.get(&class).copied().unwrap_or(0)),
            min_screen_px: def.min_screen_px,
            full_particle_px: def.full_particle_px,
            crowd_threshold: def.crowd_threshold,
        };
        let at = site("budget", None);
        self.at_least(&at, "min_screen_px", budget.min_screen_px, 0.0)?;
        self.at_least(&at, "full_particle_px", budget.full_particle_px, 1.0)?;
        for effect in effects {
            for (index, layer) in effect.layers.iter().enumerate() {
                let (capacity, block) = match layer.block.pool() {
                    Pool::Sprites => (budget.sprites, "sprites"),
                    Pool::Decals => (budget.decals, "decals"),
                    Pool::Particles => (budget.particles, "particles"),
                    Pool::Lights => (budget.lights, "lights"),
                    Pool::Distortions => (budget.distortions, "distortions"),
                    // A shell changes the unit's own bubble and needs no pool slot.
                    Pool::Bubble => (u32::MAX, "bubble"),
                };
                let effects_capacity = budget.effects.min(budget.class_caps[effect.class.index()]);
                if capacity == 0 || effects_capacity == 0 {
                    let block = if effects_capacity == 0 {
                        "effects"
                    } else {
                        block
                    };
                    return Err(EffectCatalogError::NoCapacity {
                        path: self.path.to_path_buf(),
                        site: site(&effect.name, Some(index)),
                        block,
                    });
                }
            }
        }
        Ok(budget)
    }

    /// The defaults, resolved.
    fn defaults(&self, file: &EffectsFile) -> Result<Defaults, EffectCatalogError> {
        let at = site("defaults", None);
        let one = |name: Option<&String>| {
            name.map(|name| self.id(NameKind::Effect, name, &at).map(EffectId))
                .transpose()
        };
        let mut status = BTreeMap::new();
        for (kind, name) in &file.defaults.status {
            status.insert(*kind, EffectId(self.id(NameKind::Effect, name, &at)?));
        }
        let concealment = &file.defaults.concealment;
        Ok(Defaults {
            impact: one(file.defaults.impact.as_ref())?,
            splash: one(file.defaults.splash.as_ref())?,
            death: one(file.defaults.death.as_ref())?,
            status,
            concealment: super::ConcealmentDefaults {
                burrow_enter: one(concealment.burrow_enter.as_ref())?,
                burrow_leave: one(concealment.burrow_leave.as_ref())?,
                burrowed: one(concealment.burrowed.as_ref())?,
                cloaked_own: one(concealment.cloaked_own.as_ref())?,
                cloaked_enemy: one(concealment.cloaked_enemy.as_ref())?,
                detected: one(concealment.detected.as_ref())?,
            },
            anchor_transition: one(file.defaults.mode.transition.as_ref())?,
            anchored: one(file.defaults.mode.anchored.as_ref())?,
        })
    }
}

/// The fields of a `Ring` block definition, borrowed for [`Resolver::ring_block`].
struct RingFields<'a> {
    /// Texture element name.
    texture: &'a String,
    /// Ramp name.
    ramp: &'a String,
    /// Card orientation.
    orient: &'a super::schema::Orient,
    /// Thickness as a fraction of the radius.
    thickness: &'a f32,
    /// Erosion curve name.
    erosion: &'a Option<String>,
    /// Texture mapping.
    mapping: &'a super::schema::RingMapping,
    /// Height above the placed point.
    height: &'a f32,
    /// Sort bias.
    sort_bias: &'a f32,
    /// Noise breakup.
    breakup: &'a Option<BreakupDef>,
    /// Depth of a front's leading-edge noise.
    edge_noise: &'a f32,
}

/// The 32-bit FNV-1a hash of `name`.
fn name_hash(name: &str) -> u32 {
    name.bytes().fold(0x811C_9DC5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    })
}

/// A [`Site`] for `owner` and an optional layer.
fn site(owner: &str, layer: Option<usize>) -> Site {
    Site {
        owner: owner.to_owned(),
        layer,
    }
}
