//! One viewer's knowledge of the battlefield, in the shape presentation asks about it.
//!
//! # Responsibility
//! Owns [`BattleFog`]: which map cells the viewer sees right now, which concealed units of other
//! players the viewer detects, and how each resource node looked when it was last in sight. It
//! answers the questions every presentation path shares: how may this unit reach the screen
//! ([`Presence`]), and may this combat event be delivered. It decides no policy of the simulation.
//!
//! # Detection (plan `2026-11-09-effect-visualization` D-5)
//! A unit of another player whose concealment is not `None` and which the view's `revealed` list
//! does not name is *undetected*. Undetected and burrowed, it is [`Presence::Hidden`]: nothing of
//! it is drawn. Undetected and cloaked, it is [`Presence::Shimmer`]: drawn with the cloak look a
//! player of the game would see, but never picked, inspected or given a health bar. This is the
//! one place the rule is written; the bubbles, picking, the selection panel and the bars all ask
//! [`presence_of`].
//!
//! # How it fits
//! The simulation writes a viewer-scoped [`PlayerView`] once per tick. The HUD copies the part
//! presentation needs into this resource, see [`crate::battle_hud::fog`], and the tick driver and
//! the map billboards read it from here. It lives in the pipeline rather than the HUD because those
//! readers sit below the HUD, which depends on them and never the reverse.
//!
//! The resource exists only under
//! [`BattleProfile::Player`](crate::battle_presentation::BattleProfile::Player). A reader takes it
//! as an `Option`, and `None` means nobody is being kept from anything. That is the sandbox, the
//! editor and the headless check, which all keep showing the truth.
//!
//! # Invariants
//! - It fails closed: with no viewer or no vision written, [`BattleFog::sees`] answers `false` and
//!   only a viewer's own unit could be shown, so an unwritten fog hides rather than reveals. An
//!   unwritten fog detects nothing either.
//! - The viewer's own units are always [`Presence::Shown`], concealed or not.
//! - A node's remembered presence changes only on a tick its cell is in sight.
//! - It knows nothing that its [`PlayerView`] did not say, so nothing here can leak.
//! - After the first refresh of a session, refreshing allocates nothing.

use bevy::prelude::*;
use sim_types::{
    BubbleRenderInstance, CombatEventVisual, Concealment, EntityId, PlayerId, PlayerView, Tick,
    VisibilityState,
};

use super::commands::SessionGeneration;

/// How one unit may reach the viewer's screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// Drawn as it is, and it may be picked, inspected and given a health bar.
    Shown,
    /// An undetected cloaked unit: drawn only with the cloak look, never picked, inspected or
    /// given a health bar.
    Shimmer,
    /// Nothing of it is drawn, and nothing of it may be picked.
    Hidden,
}

impl Presence {
    /// Whether any of the unit is drawn.
    #[must_use]
    pub const fn is_drawn(self) -> bool {
        !matches!(self, Self::Hidden)
    }

    /// Whether the unit may be picked, inspected, targeted and given a health bar.
    #[must_use]
    pub const fn is_pickable(self) -> bool {
        matches!(self, Self::Shown)
    }
}

/// The [`Presence`] of `bubble` under `fog`, where `None` is no fog: the sandbox, the editor and
/// the headless check, which show the truth.
#[must_use]
pub fn presence_of(fog: Option<&BattleFog>, bubble: &BubbleRenderInstance) -> Presence {
    fog.map_or(Presence::Shown, |fog| fog.presence(bubble))
}

/// What one viewer knows of the map, as of the latest refresh.
///
/// Positions are on the map plane in world units, where one unit is one grid cell. The simulation's
/// `y` is Bevy's `z`.
#[derive(Resource, Debug, Default)]
pub struct BattleFog {
    viewer: Option<PlayerId>,
    /// Width of the map in cells, the stride of `visible`.
    width: u32,
    /// Row-major: whether the viewer sees each cell. Empty until a view is written.
    visible: Vec<bool>,
    /// Last seen presence of each node, in map order.
    node_present: Vec<bool>,
    /// Concealed units of other players the viewer detects, sorted, as the view listed them.
    revealed: Vec<EntityId>,
    /// Session, viewer and tick this fog was written for.
    written: Option<(SessionGeneration, PlayerId, Tick)>,
    /// Session whose map seeded `node_present`.
    seeded: Option<SessionGeneration>,
    refills: u64,
}

impl BattleFog {
    /// Every growable buffer this fog retains, as `(name, address, capacity in elements)`.
    ///
    /// Evidence for the steady-state tests that a refill reuses storage. Not part of the fog's
    /// interface.
    #[doc(hidden)]
    pub fn retained_buffers(&self) -> impl Iterator<Item = (&'static str, usize, usize)> + '_ {
        [
            (
                "fog.visible",
                self.visible.as_ptr().addr(),
                self.visible.capacity(),
            ),
            (
                "fog.node_present",
                self.node_present.as_ptr().addr(),
                self.node_present.capacity(),
            ),
            (
                "fog.revealed",
                self.revealed.as_ptr().addr(),
                self.revealed.capacity(),
            ),
        ]
        .into_iter()
    }

    /// The player whose knowledge this is, if a battle is presented to one.
    #[must_use]
    pub const fn viewer(&self) -> Option<PlayerId> {
        self.viewer
    }

    /// Whether map position `(x, y)`, in world units, is in a `Visible` cell. False off the map.
    #[must_use]
    pub fn sees(&self, x: f32, y: f32) -> bool {
        // The negated comparison also refuses NaN.
        if !(x >= 0.0 && y >= 0.0) {
            return false;
        }
        // A saturating cast of a nonnegative value, so this is the floor `world_to_cell` takes, and
        // a coordinate too large for the map stays too large.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "both are nonnegative, and `as` saturates"
        )]
        let (column, row) = (x as u32, y as u32);
        if column >= self.width {
            return false;
        }
        let index = u64::from(row) * u64::from(self.width) + u64::from(column);
        usize::try_from(index)
            .ok()
            .and_then(|index| self.visible.get(index))
            .is_some_and(|visible| *visible)
    }

    /// Whether a thing of `owner` at map position `(x, y)` may reach the screen.
    ///
    /// The viewer's own things always may. Everything else needs a `Visible` cell, neutral units
    /// included.
    #[must_use]
    pub fn shows(&self, owner: PlayerId, x: f32, y: f32) -> bool {
        self.viewer == Some(owner) || self.sees(x, y)
    }

    /// Whether the viewer detects unit `id` of `owner`, concealed as `concealment`.
    ///
    /// An unconcealed unit and a unit of the viewer's own are always detected. Any other needs the
    /// view's `revealed` list to name it, which the simulation fills only for a unit under the
    /// viewer's detection and in a `Visible` cell.
    #[must_use]
    pub fn detects(&self, owner: PlayerId, id: EntityId, concealment: Concealment) -> bool {
        !concealment.is_concealed()
            || self.viewer == Some(owner)
            || self.revealed.binary_search(&id).is_ok()
    }

    /// How `bubble` may reach the screen: the one rule every reader of a unit asks (D-5).
    ///
    /// Out of sight, a unit of another player is [`Presence::Hidden`]. In sight but undetected,
    /// a burrowed unit is [`Presence::Hidden`] and a cloaked one [`Presence::Shimmer`]. The
    /// bubble is judged at the position it is drawn at, `world_pos`, in world units.
    #[must_use]
    pub fn presence(&self, bubble: &BubbleRenderInstance) -> Presence {
        let [x, y, _] = bubble.world_pos;
        if !self.shows(bubble.owner, x, y) {
            return Presence::Hidden;
        }
        if self.detects(bubble.owner, bubble.entity_id, bubble.concealment) {
            return Presence::Shown;
        }
        match bubble.concealment {
            Concealment::Cloaked => Presence::Shimmer,
            Concealment::Burrowed | Concealment::None => Presence::Hidden,
        }
    }

    /// Whether `event` may be delivered to the visual layer, judged where it lands.
    #[must_use]
    pub fn reveals(&self, event: &CombatEventVisual) -> bool {
        let [x, y] = event.anchor();
        self.shows(event.owner(), x, y)
    }

    /// Last seen presence of each node, in map order. Empty before the first refresh.
    #[must_use]
    pub fn node_present(&self) -> &[bool] {
        &self.node_present
    }

    /// Whether this fog already describes `written`, which is `None` when no viewer is presented.
    #[must_use]
    pub fn is_current(&self, written: Option<(SessionGeneration, PlayerId, Tick)>) -> bool {
        self.written == written
    }

    /// How many times this fog has been rewritten, for tests of its change detection.
    #[must_use]
    pub const fn refills(&self) -> u64 {
        self.refills
    }

    /// Address and capacity of each retained buffer, which an allocation would move or grow.
    #[must_use]
    pub fn buffers(&self) -> [(usize, usize); 3] {
        [
            (self.visible.as_ptr() as usize, self.visible.capacity()),
            (
                self.node_present.as_ptr() as usize,
                self.node_present.capacity(),
            ),
            (self.revealed.as_ptr() as usize, self.revealed.capacity()),
        ]
    }

    /// Stops presenting to anyone: from now on only `false` comes out of [`Self::sees`].
    pub fn close(&mut self) {
        self.viewer = None;
        self.width = 0;
        self.visible.clear();
        self.revealed.clear();
        self.written = None;
    }

    // PERFORMANCE RATIONALE:
    // Why: `sees` runs for every foreign bubble on every frame, and a view is rewritten at 25 Hz.
    // What: A flat row-major copy, refilled in place only when the tick changes. Every frame reads
    // it in O(1) per bubble with no allocation, and nothing rescans the bubbles against the nodes.
    // Trade-off: One byte per cell held a second time, so that presentation, which sits below the
    // HUD, does not depend on `HudModel` or on the fuller `PlayerView` enum. The detected units
    // are copied sorted, so each bubble asks with one binary search and nothing is allocated.
    /// Takes in `view`, written for `generation`, whose map loaded with the nodes `loaded` says.
    ///
    /// `loaded` runs once per session: how the map looked at load is public, because every node is
    /// drawn from it. After that, a node changes only if `view` lists it as in sight.
    pub fn refill(
        &mut self,
        generation: SessionGeneration,
        view: &PlayerView,
        loaded: impl Iterator<Item = bool>,
    ) {
        if self.seeded != Some(generation) {
            self.seeded = Some(generation);
            self.node_present.clear();
            self.node_present.extend(loaded);
        }
        self.viewer = Some(view.viewer);
        self.width = view.map_width;
        self.visible.clear();
        self.visible.extend(
            view.visibility
                .iter()
                .map(|state| *state == VisibilityState::Visible),
        );
        self.revealed.clear();
        self.revealed.extend_from_slice(&view.revealed);
        // The view promises a sorted list; sorting in place keeps the binary search sound if a
        // writer ever breaks that, and costs one pass over a sorted slice otherwise.
        self.revealed.sort_unstable();
        for node in &view.visible_nodes {
            if let Some(present) = self.node_present.get_mut(node.id.0 as usize) {
                *present = node.remaining > 0;
            }
        }
        self.written = Some((generation, view.viewer, view.tick));
        self.refills += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_types::{ResourceNodeId, VisibleNodeView};

    const ME: PlayerId = PlayerId(0);
    const THEM: PlayerId = PlayerId(1);

    /// A 3 by 2 map on which only cells `(1, 0)` and `(2, 1)` are in sight.
    fn view() -> PlayerView {
        use VisibilityState::{Fogged, Unexplored, Visible};
        PlayerView {
            viewer: ME,
            map_width: 3,
            map_height: 2,
            visibility: vec![Fogged, Visible, Unexplored, Unexplored, Fogged, Visible],
            ..PlayerView::default()
        }
    }

    fn fog_of(view: &PlayerView) -> BattleFog {
        let mut fog = BattleFog::default();
        fog.refill(SessionGeneration::FIRST, view, [true, true].into_iter());
        fog
    }

    fn tracer(to: [f32; 2], owner: PlayerId) -> CombatEventVisual {
        CombatEventVisual::HitscanTracer {
            from: [0.0; 3],
            to: [to[0], to[1], 0.5],
            owner,
            source: sim_types::EffectSource::default(),
        }
    }

    #[test]
    fn only_visible_cells_are_seen_and_memory_of_a_cell_is_not_sight() {
        let fog = fog_of(&view());
        assert!(fog.sees(1.5, 0.5));
        assert!(fog.sees(2.0, 1.0), "a cell owns its lower edges");
        assert!(!fog.sees(0.5, 0.5), "Fogged is not Visible");
        assert!(!fog.sees(2.5, 0.5), "Unexplored is not Visible");
    }

    #[test]
    fn the_cell_edge_rounds_as_the_map_does() {
        let fog = fog_of(&view());
        assert!(fog.sees(1.0, 0.0));
        assert!(fog.sees(1.999, 0.999));
        assert!(!fog.sees(0.999, 0.0));
        assert!(!fog.sees(2.0, 0.0));
    }

    #[test]
    fn off_the_map_is_never_seen() {
        let fog = fog_of(&view());
        for (x, y) in [
            (-0.5, 0.5),
            (1.5, -0.5),
            (3.0, 1.5),
            (1.5, 2.0),
            (f32::MAX, 0.5),
            (1.5, f32::MAX),
            (f32::NAN, 0.5),
            (1.5, f32::INFINITY),
        ] {
            assert!(!fog.sees(x, y), "({x}, {y})");
        }
    }

    #[test]
    fn an_unwritten_or_closed_fog_hides_everything() {
        let mut fog = BattleFog::default();
        assert!(!fog.sees(0.0, 0.0));
        assert!(!fog.shows(THEM, 1.5, 0.5));
        assert!(!fog.shows(ME, 1.5, 0.5), "no viewer owns nothing");
        assert!(!fog.reveals(&tracer([1.5, 0.5], ME)));

        fog = fog_of(&view());
        assert!(fog.sees(1.5, 0.5));
        fog.close();
        assert!(!fog.sees(1.5, 0.5));
        assert_eq!(fog.viewer(), None);
        assert!(fog.is_current(None));
    }

    #[test]
    fn the_viewers_own_things_are_shown_anywhere_and_others_only_in_sight() {
        let fog = fog_of(&view());
        assert!(fog.shows(ME, 0.5, 0.5));
        assert!(fog.shows(THEM, 1.5, 0.5));
        assert!(!fog.shows(THEM, 0.5, 0.5));
        assert!(!fog.shows(PlayerId::NEUTRAL, 0.5, 0.5));
    }

    #[test]
    fn an_event_is_judged_by_its_owner_and_where_it_lands() {
        let fog = fog_of(&view());
        assert!(fog.reveals(&tracer([1.5, 0.5], THEM)));
        assert!(!fog.reveals(&tracer([0.5, 0.5], THEM)));
        assert!(fog.reveals(&tracer([0.5, 0.5], ME)));
    }

    #[test]
    fn a_node_changes_only_when_it_is_in_sight() {
        let mut view = view();
        view.visible_nodes = vec![VisibleNodeView {
            id: ResourceNodeId(1),
            remaining: 0,
        }];
        let mut fog = fog_of(&view);
        assert_eq!(fog.node_present(), [true, false]);

        // Node 1 regrowing out of sight would not be known either.
        view.visible_nodes.clear();
        fog.refill(SessionGeneration::FIRST, &view, std::iter::empty());
        assert_eq!(fog.node_present(), [true, false]);
    }

    #[test]
    fn a_node_outside_the_seeded_map_is_ignored() {
        let mut view = view();
        view.visible_nodes = vec![VisibleNodeView {
            id: ResourceNodeId(9),
            remaining: 0,
        }];
        assert_eq!(fog_of(&view).node_present(), [true, true]);
    }

    #[test]
    fn a_new_session_starts_from_its_own_map() -> Result<(), Box<dyn std::error::Error>> {
        let mut view = view();
        view.visible_nodes = vec![VisibleNodeView {
            id: ResourceNodeId(0),
            remaining: 0,
        }];
        let mut fog = fog_of(&view);
        assert_eq!(fog.node_present(), [false, true]);

        view.visible_nodes.clear();
        fog.refill(SessionGeneration::FIRST.next()?, &view, [true].into_iter());
        assert_eq!(fog.node_present(), [true]);
        Ok(())
    }

    #[test]
    fn a_refill_reuses_its_buffers_and_counts_itself() {
        let mut view = view();
        view.revealed = vec![EntityId(4), EntityId(9)];
        let mut fog = fog_of(&view);
        let warmed = fog.buffers();
        for _ in 0..50 {
            fog.refill(SessionGeneration::FIRST, &view, std::iter::empty());
        }
        assert_eq!(fog.buffers(), warmed);
        assert_eq!(fog.refills(), 51);
    }

    /// A bubble of `owner`, unit `id`, concealed as `concealment`, standing at `(x, y)`.
    fn unit(
        owner: PlayerId,
        id: u32,
        concealment: Concealment,
        at: [f32; 2],
    ) -> BubbleRenderInstance {
        use sim_types::{AnimState, ShapeKind, SilhouetteClass, UnitTypeId, WeaponVisualKind};
        BubbleRenderInstance {
            entity_id: EntityId(id),
            shape: ShapeKind::UnitSphere,
            silhouette: SilhouetteClass::HeavyHex,
            unit_type: UnitTypeId::default(),
            owner,
            world_pos: [at[0], at[1], 0.5],
            ground_z: 0.0,
            radius: 0.45,
            velocity: [0.0; 2],
            heading_angle_rad: 0.0,
            aim_angle_rad: 0.0,
            is_attacking: false,
            weapon_visual: WeaponVisualKind::Hitscan,
            attack_target_pos: None,
            anim_state: AnimState::Idle,
            attack_swing: None,
            hp_ratio: 1.0,
            shield_ratio: 1.0,
            shield_hit_flash: 0.0,
            hull_hit_angle_rad: 0.0,
            hull_hit_flash: 0.0,
            build_or_queue_progress: None,
            selected: false,
            concealment,
            speed_boosted: false,
            stance: sim_types::StanceView::default(),
            status: sim_types::StatusFlags::default(),
            barrier_ratio: 0.0,
        }
    }

    /// The fog of [`view`] that detects units 4 and 9.
    fn detecting_fog() -> BattleFog {
        let mut view = view();
        view.revealed = vec![EntityId(4), EntityId(9)];
        fog_of(&view)
    }

    #[test]
    fn an_undetected_enemy_in_sight_is_hidden_if_burrowed_and_a_shimmer_if_cloaked() {
        let fog = detecting_fog();
        let in_sight = [1.5, 0.5];
        assert_eq!(
            fog.presence(&unit(THEM, 5, Concealment::Burrowed, in_sight)),
            Presence::Hidden
        );
        assert_eq!(
            fog.presence(&unit(THEM, 5, Concealment::Cloaked, in_sight)),
            Presence::Shimmer
        );
        assert_eq!(
            fog.presence(&unit(THEM, 5, Concealment::None, in_sight)),
            Presence::Shown
        );
    }

    #[test]
    fn a_detected_enemy_in_sight_is_shown_whatever_hides_it() {
        let fog = detecting_fog();
        for concealment in [Concealment::Burrowed, Concealment::Cloaked] {
            for id in [4, 9] {
                assert_eq!(
                    fog.presence(&unit(THEM, id, concealment, [1.5, 0.5])),
                    Presence::Shown,
                    "{id} {concealment:?}"
                );
            }
        }
    }

    #[test]
    fn detection_does_not_reach_out_of_sight() {
        let fog = detecting_fog();
        for concealment in [
            Concealment::None,
            Concealment::Burrowed,
            Concealment::Cloaked,
        ] {
            assert_eq!(
                fog.presence(&unit(THEM, 4, concealment, [0.5, 0.5])),
                Presence::Hidden,
                "{concealment:?}"
            );
        }
    }

    #[test]
    fn the_viewers_own_concealed_units_are_always_shown() {
        let fog = detecting_fog();
        for concealment in [
            Concealment::None,
            Concealment::Burrowed,
            Concealment::Cloaked,
        ] {
            for at in [[1.5, 0.5], [0.5, 0.5]] {
                assert_eq!(
                    fog.presence(&unit(ME, 5, concealment, at)),
                    Presence::Shown,
                    "{concealment:?} at {at:?}"
                );
            }
        }
    }

    #[test]
    fn no_fog_shows_the_truth() {
        for concealment in [
            Concealment::None,
            Concealment::Burrowed,
            Concealment::Cloaked,
        ] {
            assert_eq!(
                presence_of(None, &unit(THEM, 5, concealment, [0.5, 0.5])),
                Presence::Shown
            );
        }
        let fog = detecting_fog();
        assert_eq!(
            presence_of(
                Some(&fog),
                &unit(THEM, 5, Concealment::Burrowed, [1.5, 0.5])
            ),
            Presence::Hidden
        );
    }

    #[test]
    fn only_a_shown_unit_is_pickable_and_only_a_hidden_one_is_not_drawn() {
        assert!(Presence::Shown.is_pickable() && Presence::Shown.is_drawn());
        assert!(!Presence::Shimmer.is_pickable() && Presence::Shimmer.is_drawn());
        assert!(!Presence::Hidden.is_pickable() && !Presence::Hidden.is_drawn());
    }

    #[test]
    fn an_unsorted_reveal_list_still_detects_and_a_closed_fog_forgets_it() {
        let mut view = view();
        view.revealed = vec![EntityId(9), EntityId(2), EntityId(4)];
        let mut fog = fog_of(&view);
        for id in [2, 4, 9] {
            assert!(
                fog.detects(THEM, EntityId(id), Concealment::Cloaked),
                "{id}"
            );
        }
        assert!(!fog.detects(THEM, EntityId(3), Concealment::Cloaked));
        fog.close();
        assert!(!fog.detects(THEM, EntityId(4), Concealment::Burrowed));
        assert!(fog.detects(THEM, EntityId(3), Concealment::None));
    }

    #[test]
    fn a_tick_that_loses_detection_hides_the_unit_again() {
        let mut view = view();
        view.revealed = vec![EntityId(4)];
        let mut fog = fog_of(&view);
        let burrowed = unit(THEM, 4, Concealment::Burrowed, [1.5, 0.5]);
        assert_eq!(fog.presence(&burrowed), Presence::Shown);
        view.revealed.clear();
        fog.refill(SessionGeneration::FIRST, &view, std::iter::empty());
        assert_eq!(fog.presence(&burrowed), Presence::Hidden);
    }

    #[test]
    fn it_knows_which_view_it_describes() {
        let mut view = view();
        view.tick = Tick(4);
        let fog = fog_of(&view);
        assert!(fog.is_current(Some((SessionGeneration::FIRST, ME, Tick(4)))));
        assert!(!fog.is_current(Some((SessionGeneration::FIRST, ME, Tick(5)))));
        assert!(!fog.is_current(None));
    }
}
