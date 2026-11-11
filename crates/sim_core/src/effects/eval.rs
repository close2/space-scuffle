//! The walk evaluator: one synchronous evaluation of an effect node and everything under it
//! that runs at once (plan `2026-11-10-spellcaster-mechanics` D-15, D-16, D-18).
//!
//! A walk is depth-first and pre-order: a node acts when it is entered, then its children run
//! in authored order, each child's synchronous subtree finishing before the next sibling
//! starts. The evaluator is iterative over a fixed stack of [`MAX_EFFECT_DEPTH`] frames on the
//! native stack; catalog validation guarantees no walk is deeper. A `Search` gathers its
//! candidates into the world's one reused search buffer, which suffices because no search
//! runs inside another search's walk (D-17).

use super::filter::{passes_filter, unit_target};
use super::state::{ActiveMissile, ActivePersistent, ActiveStatus, Displacement, PersistentAnchor};
use crate::catalog::{EffectNode, SpellCatalog, MAX_EFFECT_DEPTH};
use crate::weapon_timing::SimulationDiagnostic;
use crate::SimWorld;
use sim_types::{
    EffectAnchor, EffectContext, EffectId, EffectTarget, EntityId, SimScalar, SimVec2, Tick,
};

/// One level of the walk stack.
#[derive(Copy, Clone)]
struct Frame {
    /// The node whose children are being run.
    node: EffectId,
    /// The context the children inherit or derive from.
    context: EffectContext,
    /// The next child (set index, search cursor, or 0/1 for a single child).
    next: u16,
    /// A search's centre, a persistent's anchor point: the source point of child contexts.
    point: SimVec2,
}

impl Frame {
    const fn new(node: EffectId, context: EffectContext, point: SimVec2) -> Self {
        Self {
            node,
            context,
            next: 0,
            point,
        }
    }
}

impl SimWorld {
    /// Runs one walk from `root` with `context`.
    ///
    /// Allocation-free once the world's buffers are warm: the stack is a fixed array and the
    /// search buffer is reused.
    pub(crate) fn run_effect_walk(&mut self, root: EffectId, context: EffectContext) {
        let spells = self.catalog.spells_handle();
        let mut stack = [Frame::new(root, context, SimVec2::ZERO); MAX_EFFECT_DEPTH];
        let mut len = 0_usize;
        if let Some(frame) = self.enter_node(&spells, root, context) {
            stack[0] = frame;
            len = 1;
        }
        while len > 0 {
            let Some((child, child_context)) = self.next_child(&spells, &mut stack[len - 1]) else {
                len -= 1;
                continue;
            };
            let Some(frame) = self.enter_node(&spells, child, child_context) else {
                continue;
            };
            if len == MAX_EFFECT_DEPTH {
                self.tick_diagnostics
                    .push(SimulationDiagnostic::EffectWalkTooDeep {
                        tick: Tick(self.current_tick),
                        node: child,
                    });
                continue;
            }
            stack[len] = frame;
            len += 1;
        }
    }

    /// The next child of `frame` to run, with its context, advancing the frame's cursor.
    fn next_child(
        &self,
        spells: &SpellCatalog,
        frame: &mut Frame,
    ) -> Option<(EffectId, EffectContext)> {
        let node = spells.node(frame.node)?;
        let index = frame.next;
        frame.next = frame.next.saturating_add(1);
        match node.kind {
            EffectNode::Set { first, len } => {
                let index = u8::try_from(index).ok().filter(|i| *i < len)?;
                Some((spells.set_child(first, index)?, frame.context))
            }
            EffectNode::Search { child, .. } => {
                let (_, found) = *self.effect_scratch.search.get(usize::from(index))?;
                let context = frame
                    .context
                    .retarget(EffectTarget::Unit(found), frame.point);
                Some((child, context))
            }
            EffectNode::Persistent { initial, .. } => {
                (index == 0).then_some(())?;
                Some((initial?, frame.context))
            }
            EffectNode::ApplyStatus { status } => {
                (index == 0).then_some(())?;
                Some((spells.status(status)?.initial_effect?, frame.context))
            }
            EffectNode::LaunchMissile { .. }
            | EffectNode::Damage { .. }
            | EffectNode::RemoveStatus { .. }
            | EffectNode::ModifyVitals { .. }
            | EffectNode::CancelOrders { .. }
            | EffectNode::Displace { .. } => None,
        }
    }

    /// The point a node acting on `context` is at: the point target, or the unit's position.
    pub(crate) fn effect_point(&self, context: &EffectContext) -> Option<SimVec2> {
        match context.target {
            EffectTarget::Point(point) => Some(point),
            EffectTarget::Unit(id) => unit_target(&self.entities, id).map(|unit| unit.position),
        }
    }

    /// Enters `id`: checks its target and validator, performs its action, and returns the
    /// frame that runs its children, if it has any to run.
    fn enter_node(
        &mut self,
        spells: &SpellCatalog,
        id: EffectId,
        context: EffectContext,
    ) -> Option<Frame> {
        self.effect_scratch.visits = self.effect_scratch.visits.wrapping_add(1);
        let node = spells.node(id)?;
        let unit = match context.target {
            EffectTarget::Unit(unit) => {
                unit_target(&self.entities, unit)?;
                Some(unit)
            }
            EffectTarget::Point(_) => None,
        };
        if let Some(filter) = node.require {
            let entity = unit_target(&self.entities, unit?)?;
            if !passes_filter(filter, &context, entity, &self.catalog, &self.effects) {
                return None;
            }
        }
        match node.kind {
            EffectNode::Set { .. } => Some(Frame::new(id, context, context.source_point)),
            EffectNode::Search {
                radius,
                filter,
                max_count,
                ..
            } => {
                let centre = self.effect_point(&context)?;
                self.gather_search(centre, radius, filter, max_count, &context);
                Some(Frame::new(id, context, centre))
            }
            EffectNode::Persistent { anchor, .. } => self.create_persistent(id, anchor, context),
            EffectNode::LaunchMissile { to, .. } => {
                self.launch_missile(id, to, context);
                None
            }
            EffectNode::Damage {
                amount,
                attribute_bonus,
                responses,
            } => {
                self.apply_spell_damage(unit?, amount, &attribute_bonus, responses, &context);
                None
            }
            EffectNode::ApplyStatus { status } => {
                let first = self.apply_status(spells, unit?, status, context)?;
                first.then(|| Frame::new(id, context, context.source_point))
            }
            EffectNode::RemoveStatus { status } => {
                let unit = unit?;
                if let Ok(index) = self.effects.find_status(unit, status) {
                    self.effects.statuses.remove(index);
                    self.refresh_status_modifiers(unit);
                }
                None
            }
            EffectNode::ModifyVitals {
                life,
                shields,
                energy,
            } => {
                self.modify_vitals(unit?, [life, shields, energy]);
                None
            }
            EffectNode::CancelOrders { force_unburrow } => {
                self.cancel_orders_by_effect(unit?, force_unburrow);
                None
            }
            EffectNode::Displace { .. } => {
                let entity = self.entities.get_mut(&unit?)?;
                if entity.displacement.is_none() {
                    entity.displacement = Some(Displacement {
                        node: id,
                        context,
                        from: entity.position,
                        to: context.origin,
                        start_tick: Tick(self.current_tick),
                    });
                    entity.velocity = SimVec2::ZERO;
                    entity.current_funnel_waypoints.clear();
                }
                None
            }
        }
    }

    // PERFORMANCE RATIONALE:
    // Why: a storm searches every ~10 ticks and a walk must not allocate (D-18).
    // What: candidates are written into one reused world buffer and sorted in place; the scan
    // walks the entity map in id order with an integer distance test before any square root.
    // Trade-off: a linear scan of all entities instead of the spatial grid, whose buckets are
    // rebuilt only in target acquisition (phase 4) and are stale in phases 1 and 2; at 400
    // entities a scan costs a few microseconds and is exact.
    /// Fills the search buffer with the units within `radius` of `centre` that pass `filter`,
    /// sorted by `(distance², EntityId)` and cut to `max_count`.
    fn gather_search(
        &mut self,
        centre: SimVec2,
        radius: SimScalar,
        filter: crate::catalog::ResolvedFilter,
        max_count: Option<u16>,
        context: &EffectContext,
    ) {
        let reach = i128::from(radius.raw());
        let buffer = &mut self.effect_scratch.search;
        buffer.clear();
        for (id, entity) in &self.entities {
            if !crate::movement::math::within_distance(centre, entity.position, reach) {
                continue;
            }
            if !passes_filter(filter, context, entity, &self.catalog, &self.effects) {
                continue;
            }
            buffer.push((centre.distance_sq(entity.position), *id));
        }
        buffer.sort_unstable();
        if let Some(max) = max_count {
            buffer.truncate(usize::from(max));
        }
    }

    /// Creates a persistent instance; returns the frame that runs its `initial` node.
    fn create_persistent(
        &mut self,
        id: EffectId,
        anchor: EffectAnchor,
        context: EffectContext,
    ) -> Option<Frame> {
        let (anchor, child_context) = match anchor {
            EffectAnchor::TargetPoint => {
                let point = self.effect_point(&context)?;
                (
                    PersistentAnchor::Point(point),
                    context.retarget(EffectTarget::Point(point), point),
                )
            }
            EffectAnchor::TargetUnit => {
                let EffectTarget::Unit(unit) = context.target else {
                    return None;
                };
                let position = unit_target(&self.entities, unit)?.position;
                (
                    PersistentAnchor::Unit(unit),
                    context.retarget(EffectTarget::Unit(unit), position),
                )
            }
        };
        let instance = self.effects.next_persistent_id;
        self.effects.next_persistent_id = instance.wrapping_add(1);
        self.effects.persistents.push(ActivePersistent {
            id: instance,
            node: id,
            context,
            anchor,
            created_tick: Tick(self.current_tick),
            pulses_done: 0,
        });
        Some(Frame::new(id, child_context, child_context.source_point))
    }

    /// Launches a missile from the caster (or the source point, if the caster is gone).
    fn launch_missile(&mut self, id: EffectId, to: EffectAnchor, context: EffectContext) {
        let target = match (to, context.target) {
            (EffectAnchor::TargetUnit, EffectTarget::Unit(unit)) => EffectTarget::Unit(unit),
            (EffectAnchor::TargetPoint, _) => match self.effect_point(&context) {
                Some(point) => EffectTarget::Point(point),
                None => return,
            },
            (EffectAnchor::TargetUnit, EffectTarget::Point(_)) => return,
        };
        let position = context
            .caster
            .and_then(|caster| unit_target(&self.entities, caster))
            .map_or(context.source_point, |caster| caster.position);
        let instance = self.effects.next_missile_id;
        self.effects.next_missile_id = instance.wrapping_add(1);
        self.effects.missiles.push(ActiveMissile {
            id: instance,
            node: id,
            context,
            position,
            target,
        });
    }

    /// Applies `status` to `unit` (D-5): inserts it, or refreshes its duration keeping its
    /// phase and first context. Returns whether it was inserted (its initial effect then
    /// runs), or `None` if the status is unknown.
    fn apply_status(
        &mut self,
        spells: &SpellCatalog,
        unit: EntityId,
        status: sim_types::StatusId,
        context: EffectContext,
    ) -> Option<bool> {
        let duration = spells.status(status)?.spec.duration_ticks;
        let now = self.current_tick;
        match self.effects.find_status(unit, status) {
            Ok(index) => {
                let entry = &mut self.effects.statuses[index];
                let elapsed = SimScalar::from_num(now.wrapping_sub(entry.applied_tick.0));
                entry.expires_at = elapsed.saturating_add(duration);
                Some(false)
            }
            Err(index) => {
                self.effects.statuses.insert(
                    index,
                    ActiveStatus {
                        entity: unit,
                        status,
                        applied_tick: Tick(now),
                        expires_at: duration,
                        pulses_done: 0,
                        context: context.retarget(EffectTarget::Unit(unit), context.source_point),
                    },
                );
                self.refresh_status_modifiers(unit);
                Some(true)
            }
        }
    }
}
