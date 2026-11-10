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
    LiquidDef, PlacementDef, SpawnDef, ValueDef,
};
use super::textures::TextureAtlas;
use super::{
    Block, Breakup, Budget, CardLook, CurveId, DecalLook, Defaults, Effect, EffectCatalog,
    EffectId, ElementRef, Emitter, EmitterId, Layer, Pool, RampId, RingRadius,
    EFFECTS_FORMAT_VERSION, MAX_LAYERS, MAX_PLACEMENTS,
};
use crate::effects::binding::provides;
use crate::effects::curve::{Curve, Ramp};
use crate::effects::placement::MAX_INSTANCES_PER_PLACEMENT;
use bevy::math::Vec3;
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
            .map(|layer| layer.start + layer.duration)
            .fold(0.0_f32, f32::max);
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
        Ok(Effect {
            name: name.to_owned(),
            class: effect.class,
            event: effect.event,
            layers,
            lifetime,
            repeat: effect.repeat,
        })
    }

    /// Resolves one layer of an effect of kind `event`.
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
        self.fraction(at, "size_jitter", layer.size_jitter)?;
        for placement in &placements {
            match placement {
                PlacementDef::Point => {}
                PlacementDef::Disc { radius, .. } => values.push(*radius),
                PlacementDef::Line {
                    spacing,
                    per_point,
                    scatter,
                    delay_jitter,
                    ..
                } => {
                    values.push(*spacing);
                    self.require(at, event, super::schema::Binding::LineSpacing)?;
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
        }
        for value in &values {
            self.require(at, event, value.bind)?;
        }
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
        })
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
            if let PlacementDef::Disc { count, .. } = placement {
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

    /// Resolves a ring or a decal.
    fn masked_block(&self, at: &Site, block: &BlockDef) -> Result<Block, EffectCatalogError> {
        match block {
            BlockDef::Ring {
                texture,
                ramp,
                orient,
                thickness,
                erosion,
                mapping,
                height,
                sort_bias,
                breakup,
            } => {
                self.at_least(at, "height", *height, 0.0)?;
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
                })
            }
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
            } => {
                self.at_least(at, "depth", *depth, 0.01)?;
                self.at_least(at, "clip_feather", *clip_feather, 0.005)?;
                if let Some(liquid) = liquid {
                    self.liquid(at, liquid)?;
                }
                self.positive_fraction(at, "width", *width)?;
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
                }))
            }
            BlockDef::Sprite { .. } | BlockDef::Light { .. } | BlockDef::Particles { .. } => {
                self.block(at, block)
            }
        }
    }

    /// The budget, checked against what the effects use.
    fn budget(&self, file: &EffectsFile, effects: &[Effect]) -> Result<Budget, EffectCatalogError> {
        let def = &file.budget;
        let budget = Budget {
            particles: def.particles,
            sprites: def.sprites,
            decals: def.decals,
            lights: def.lights,
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
        Ok(Defaults {
            impact: one(file.defaults.impact.as_ref())?,
            splash: one(file.defaults.splash.as_ref())?,
            death: one(file.defaults.death.as_ref())?,
            status,
        })
    }
}

/// A [`Site`] for `owner` and an optional layer.
fn site(owner: &str, layer: Option<usize>) -> Site {
    Site {
        owner: owner.to_owned(),
        layer,
    }
}
