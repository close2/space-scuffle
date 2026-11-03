//! Strongly-typed newtype identifiers for entities, players, teams, and ticks.
//!
//! # Responsibility
//! Owns every identity the simulation passes around, and the per-player storage types:
//! [`PlayerId`] is the open wire identity (any `u8`, 255 neutral), [`PlayerSlot`] is a player
//! proven to have storage (`0..MAX_PLAYERS`), and [`PerPlayer`] is the table only a slot can
//! index.
//!
//! # How it fits
//! Commands, snapshots and saved formats carry `PlayerId`. `sim_core` converts it with
//! [`PlayerId::slot`] at each per-player read or write, so neutral and out-of-range owners are
//! handled explicitly instead of being clamped onto another player's state.
//!
//! # Invariants
//! - A [`PlayerSlot`] always satisfies `index() < MAX_PLAYERS`; its field is private.
//! - [`PerPlayer`] has exactly [`MAX_PLAYERS`] entries in slot order, no `Index<usize>` and no
//!   `Deref`, and serializes exactly like the bare array.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Monotonic tick counter for the simulation.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[repr(transparent)]
pub struct Tick(pub u32);

impl Tick {
    /// Initial zero tick.
    pub const ZERO: Self = Self(0);

    /// Advance tick by one.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// Ticks elapsed between `self` and an earlier tick.
    #[must_use]
    pub const fn duration_since(self, earlier: Self) -> u32 {
        self.0.saturating_sub(earlier.0)
    }
}

impl fmt::Display for Tick {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tick({})", self.0)
    }
}

/// Unique deterministic entity handle.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[repr(transparent)]
pub struct EntityId(pub u32);

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Entity({})", self.0)
    }
}

/// Number of player slots the simulation stores state for.
///
/// Every per-player table ([`PerPlayer`], the snapshot's player records) has exactly this many
/// entries, and [`PlayerSlot`] is the only type that may index them.
pub const MAX_PLAYERS: usize = 8;

/// Open wire identity of a player: slots `0..MAX_PLAYERS`, 255 neutral.
///
/// `PlayerId` travels in commands, snapshots and saved formats, so it accepts every `u8`.
/// Only identities below [`MAX_PLAYERS`] own per-player state; convert with
/// [`PlayerId::slot`] before reading or writing it. [`PlayerId::NEUTRAL`] and any other
/// out-of-range identity have no slot.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[repr(transparent)]
pub struct PlayerId(pub u8);

impl PlayerId {
    /// Slot assigned to neutral map entities (minerals, destructible rocks, critters).
    pub const NEUTRAL: Self = Self(255);

    /// The storage slot of this player, or `None` for neutral and out-of-range identities.
    #[must_use]
    pub const fn slot(self) -> Option<PlayerSlot> {
        PlayerSlot::new(self.0)
    }
}

impl fmt::Display for PlayerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Player({})", self.0)
    }
}

/// A player identity proven to address per-player storage: `0..MAX_PLAYERS`.
///
/// The field is private, so a `PlayerSlot` exists only through [`PlayerSlot::new`],
/// [`PlayerId::slot`], `TryFrom<PlayerId>` or [`PlayerSlot::ALL`]. Holding one is the proof
/// that indexing a [`PerPlayer`] cannot go out of bounds, which is why per-player storage
/// needs no clamping and no fallible lookup once a slot is in hand.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlayerSlot(u8);

impl PlayerSlot {
    /// Every slot in ascending order.
    pub const ALL: [Self; MAX_PLAYERS] = [
        Self(0),
        Self(1),
        Self(2),
        Self(3),
        Self(4),
        Self(5),
        Self(6),
        Self(7),
    ];

    /// `Some` for `0..MAX_PLAYERS`, `None` otherwise (including `PlayerId::NEUTRAL`'s 255).
    #[must_use]
    pub const fn new(raw: u8) -> Option<Self> {
        // `MAX_PLAYERS` is 8, so the comparison is exact in `usize`.
        if (raw as usize) < MAX_PLAYERS {
            Some(Self(raw))
        } else {
            None
        }
    }

    /// Array index of this slot, always below [`MAX_PLAYERS`].
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// The wire identity of this slot.
    #[must_use]
    pub const fn id(self) -> PlayerId {
        PlayerId(self.0)
    }
}

impl From<PlayerSlot> for PlayerId {
    fn from(slot: PlayerSlot) -> Self {
        slot.id()
    }
}

impl TryFrom<PlayerId> for PlayerSlot {
    type Error = PlayerSlotError;

    fn try_from(player: PlayerId) -> Result<Self, Self::Error> {
        player.slot().ok_or(PlayerSlotError::OutOfRange(player))
    }
}

impl fmt::Display for PlayerSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PlayerSlot({})", self.0)
    }
}

/// Why a [`PlayerId`] could not become a [`PlayerSlot`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PlayerSlotError {
    /// The identity is neutral or not below [`MAX_PLAYERS`], so it owns no per-player state.
    #[error("{0} has no player slot (slots are 0..{MAX_PLAYERS})")]
    OutOfRange(PlayerId),
}

/// One value per player slot, indexable only by [`PlayerSlot`].
///
/// There is deliberately no `Index<usize>` and no `Deref` to the array: a raw integer, a
/// clamped index or a neutral owner cannot reach the storage. Callers holding a [`PlayerId`]
/// use [`PerPlayer::get`] / [`PerPlayer::get_mut`] and handle `None` explicitly.
///
/// Serialized as the bare array (`#[serde(transparent)]`), and [`PerPlayer::as_slice`] exposes
/// the same elements in slot order, so checksums and saved data encode identically to the
/// `[T; MAX_PLAYERS]` this type replaced.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PerPlayer<T>([T; MAX_PLAYERS]);

impl<T> PerPlayer<T> {
    /// Builds the table by calling `f` once per slot, in ascending slot order.
    pub fn from_fn(mut f: impl FnMut(PlayerSlot) -> T) -> Self {
        Self(std::array::from_fn(|index| f(PlayerSlot::ALL[index])))
    }

    /// Wraps an array already laid out in slot order.
    #[must_use]
    pub const fn from_array(values: [T; MAX_PLAYERS]) -> Self {
        Self(values)
    }

    /// The value of `player`, or `None` for neutral and out-of-range identities.
    #[must_use]
    pub fn get(&self, player: PlayerId) -> Option<&T> {
        player.slot().map(|slot| &self[slot])
    }

    /// The mutable value of `player`, or `None` for neutral and out-of-range identities.
    pub fn get_mut(&mut self, player: PlayerId) -> Option<&mut T> {
        player.slot().map(|slot| &mut self[slot])
    }

    /// Values in ascending slot order.
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.0.iter()
    }

    /// Mutable values in ascending slot order.
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.0.iter_mut()
    }

    /// Each slot with its value, in ascending slot order.
    pub fn enumerate(&self) -> impl Iterator<Item = (PlayerSlot, &T)> {
        PlayerSlot::ALL.into_iter().zip(self.0.iter())
    }

    /// Each slot with its mutable value, in ascending slot order.
    pub fn enumerate_mut(&mut self) -> impl Iterator<Item = (PlayerSlot, &mut T)> {
        PlayerSlot::ALL.into_iter().zip(self.0.iter_mut())
    }

    /// All values as a slice in slot order; its length is always [`MAX_PLAYERS`].
    #[must_use]
    pub const fn as_slice(&self) -> &[T] {
        &self.0
    }
}

impl<T> std::ops::Index<PlayerSlot> for PerPlayer<T> {
    type Output = T;

    fn index(&self, slot: PlayerSlot) -> &T {
        // PlayerSlot's constructor proves `index() < MAX_PLAYERS`, so this cannot panic.
        &self.0[slot.index()]
    }
}

impl<T> std::ops::IndexMut<PlayerSlot> for PerPlayer<T> {
    fn index_mut(&mut self, slot: PlayerSlot) -> &mut T {
        // PlayerSlot's constructor proves `index() < MAX_PLAYERS`, so this cannot panic.
        &mut self.0[slot.index()]
    }
}

impl<'a, T> IntoIterator for &'a PerPlayer<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut PerPlayer<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

/// Alliance team group identifier (0 = Neutral, 1 = Team 1, 2 = Team 2).
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[repr(transparent)]
pub struct TeamId(pub u8);

impl TeamId {
    /// Neutral team.
    pub const NEUTRAL: Self = Self(0);
}

impl fmt::Display for TeamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Team({})", self.0)
    }
}

/// Natural resource node identifier: the node's index in the map's stable resource list.
///
/// Resource nodes are map data, not entities, so an [`EntityId`] cannot name one. Nodes are
/// never removed from that list; an exhausted node keeps its identifier with nothing left.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[repr(transparent)]
pub struct ResourceNodeId(pub u32);

impl fmt::Display for ResourceNodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ResourceNode({})", self.0)
    }
}

/// Unit archetype identifier indexing into the static Unit Catalog.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[repr(transparent)]
pub struct UnitTypeId(pub u16);

impl fmt::Display for UnitTypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UnitType({})", self.0)
    }
}

/// Faction identity: the `faction_id` of a faction defined in `assets/catalog/factions.ron`.
///
/// Archetypes and upgrades name their faction by slug; the catalog loader resolves the slug to
/// this ID, which rules compare (a builder raises only structures of its own faction).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct FactionId(pub u8);

impl fmt::Display for FactionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Faction({})", self.0)
    }
}

/// Weapon-class identity: the `class_id` of a weapon class in `assets/catalog/factions.ron`.
///
/// Part of the checksum schema: a weapon's class and each player's researched weapon levels
/// (keyed and ordered by this ID) are hashed as this `u8`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct WeaponClassId(pub u8);

impl fmt::Display for WeaponClassId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "WeaponClass({})", self.0)
    }
}

/// Armor-class identity: the `class_id` of an armor class in `assets/catalog/factions.ron`.
///
/// Part of the checksum schema: each player's researched armor levels (keyed and ordered by
/// this ID) are hashed as this `u8`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct ArmorClassId(pub u8);

impl fmt::Display for ArmorClassId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ArmorClass({})", self.0)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn test_id_traits() {
        let e1 = EntityId(1);
        let e2 = EntityId(2);
        assert!(e1 < e2);

        let mut set = BTreeSet::new();
        set.insert(e2);
        set.insert(e1);
        let ordered: Vec<_> = set.into_iter().collect();
        assert_eq!(ordered, vec![e1, e2]);

        assert_eq!(e1.to_string(), "Entity(1)");
        assert_eq!(PlayerId(3).to_string(), "Player(3)");
        assert_eq!(PlayerId::NEUTRAL.to_string(), "Player(255)");
        assert_eq!(TeamId(1).to_string(), "Team(1)");
        assert_eq!(TeamId::NEUTRAL.to_string(), "Team(0)");
        assert_eq!(UnitTypeId(42).to_string(), "UnitType(42)");
    }

    #[test]
    fn player_slot_rejects_identities_without_storage() {
        assert_eq!(PlayerSlot::new(8), None);
        assert_eq!(PlayerSlot::new(u8::MAX), None);
        assert_eq!(PlayerId::NEUTRAL.slot(), None);
        assert_eq!(PlayerId(9).slot(), None);
        assert_eq!(
            PlayerSlot::try_from(PlayerId(9)),
            Err(PlayerSlotError::OutOfRange(PlayerId(9)))
        );
    }

    #[test]
    fn player_slot_round_trips_every_valid_identity() {
        for raw in 0..8_u8 {
            let slot = PlayerSlot::new(raw).expect("invariant: raw < MAX_PLAYERS");
            assert_eq!(slot.index(), usize::from(raw));
            assert_eq!(slot.id(), PlayerId(raw));
            assert_eq!(PlayerId(raw).slot(), Some(slot));
            assert_eq!(PlayerSlot::try_from(PlayerId(raw)), Ok(slot));
            assert_eq!(PlayerId::from(slot), PlayerId(raw));
        }
    }

    #[test]
    fn player_slot_all_is_ascending_and_complete() {
        assert_eq!(PlayerSlot::ALL.len(), MAX_PLAYERS);
        for (index, slot) in PlayerSlot::ALL.iter().enumerate() {
            assert_eq!(slot.index(), index);
        }
        assert!(PlayerSlot::ALL.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn per_player_indexes_by_slot_and_rejects_unslotted_ids() {
        let mut table = PerPlayer::from_fn(|slot| slot.index() * 10);
        assert_eq!(table.as_slice(), &[0, 10, 20, 30, 40, 50, 60, 70]);
        let seven = PlayerSlot::ALL[7];
        table[seven] += 1;
        assert_eq!(table[seven], 71);
        assert_eq!(table.get(PlayerId(7)), Some(&71));
        assert_eq!(table.get(PlayerId(8)), None);
        assert_eq!(table.get_mut(PlayerId::NEUTRAL), None);
        let pairs: Vec<_> = table.enumerate().map(|(s, v)| (s.index(), *v)).collect();
        assert_eq!(pairs[7], (7, 71));
        for (slot, value) in table.enumerate_mut() {
            *value = slot.index();
        }
        assert!(table.iter().copied().eq(0..MAX_PLAYERS));
        for value in &mut table {
            *value += 1;
        }
        assert!(table.iter().copied().eq(1..=MAX_PLAYERS));
    }

    #[test]
    fn per_player_serde_matches_the_bare_array() {
        let array = [1_u32, 2, 3, 4, 5, 6, 7, 8];
        let table = PerPlayer::from_array(array);
        let table_ron = ron::to_string(&table).expect("invariant: u32 array serializes");
        let array_ron = ron::to_string(&array).expect("invariant: u32 array serializes");
        assert_eq!(table_ron, array_ron);
        let table_bin = bincode::serialize(&table).expect("invariant: u32 array serializes");
        let array_bin = bincode::serialize(&array).expect("invariant: u32 array serializes");
        assert_eq!(table_bin, array_bin);
        let back: PerPlayer<u32> = ron::from_str(&array_ron).expect("invariant: array parses");
        assert_eq!(back, table);
    }

    #[test]
    fn test_tick_math() {
        let t0 = Tick::ZERO;
        assert_eq!(t0.0, 0);

        let t1 = t0.next();
        assert_eq!(t1.0, 1);
        assert_eq!(t1.duration_since(t0), 1);
        assert_eq!(t0.duration_since(t1), 0); // saturating
    }
}
