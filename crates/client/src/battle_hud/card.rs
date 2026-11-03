//! The command card: what the selection can do, as positions on a grid.
//!
//! # Responsibility
//! Owns [`CommandCard`] and [`derive_card`], the pure rule from the HUD's facts to fifteen positions
//! and two fixed entries. It decides what appears, in what order, and whether it can be used. It
//! issues nothing: [`actions`](super::actions) carries a pressed slot out, and
//! [`panels`](super::panels) draws the slots.
//!
//! # How it fits
//! Input is [`HudModel`], so nothing here reads the world, and everything shown was already scoped
//! to the viewer. [`refresh_command_card`] re-derives when the model reports a change or the page
//! changes, and [`actions`](super::actions) re-derives between two keys of one frame, so a key
//! always meets the page the previous key opened.
//!
//! The card is the union of what the selection can do (D-7), and a slot's key depends only on its
//! position (D-6), so no key belongs to a unit. A lock is shown rather than hidden (D-8). Shortage
//! of money or supply is not a lock: the simulation's typed rejection explains it.
//!
//! # Invariants
//! - Pure: [`derive_card`] depends only on its arguments, and no Bevy type but [`Resource`] is here.
//! - A slot is enabled exactly when it carries no [`CardLock`].
//! - The card is empty for an empty selection, an inspected enemy, a defeated player and a closed
//!   match.
//! - Entries that do not fit are paged, never dropped.
//! - Deriving allocates nothing once warm. Text is rewritten only for a slot that changed.

use std::fmt::Write;

use bevy::prelude::*;
use sim_types::{
    CommandOptionView, CommandSubject, EntityId, OptionLock, PlayerView, UnitOrder, UnitTypeId,
};

use super::catalog::ArchetypeCommands;
use super::model::{HudModel, SelectedFact};
use crate::battle_input::command_mode::CommandMode;
use crate::input_bindings::{
    COMMAND_CARD_KEYS, COMMAND_CARD_KEY_LABELS, COMMAND_CARD_SLOTS, HOLD_POSITION_KEY,
    HOLD_POSITION_KEY_LABEL, STOP_KEY, STOP_KEY_LABEL,
};

/// Entries on a page that needs [`More`](CardAction::NextPage) or [`Back`](CardAction::Back).
pub const PAGED_ENTRIES: usize = COMMAND_CARD_SLOTS - 2;

/// Position of the entry that opens the next page, on every paged card.
const MORE_POSITION: usize = PAGED_ENTRIES;

/// Position of the entry that goes back, on every paged card.
const BACK_POSITION: usize = PAGED_ENTRIES + 1;

/// Number of fixed entries: Stop and Hold.
pub const FIXED_SLOTS: usize = 2;

/// Tooltip of an identity whose media cannot be presented.
pub const NO_MEDIA_TEXT: &str = "No approved media";

/// One thing a command card position can do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardAction {
    /// Send this to the selection at once.
    Order(UnitOrder),
    /// Enter a targeted mode.
    Mode(CommandMode),
    /// Train at the selected capable structure with the shortest queue.
    Train(UnitTypeId),
    /// Research the upgrade with this catalog ID at the selected capable structure with the
    /// shortest queue.
    Research(u32),
    /// Open the first page of structures.
    OpenBuildMenu,
    /// Show the next page of this list.
    NextPage,
    /// Show the previous page, or the root from a list's first page.
    Back,
}

/// Which list the card is showing, and how far through it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardPage {
    /// What the selection can do.
    Root {
        /// Zero-based page. A root page beyond the first exists only for a mixed selection whose
        /// union does not fit.
        page: u8,
    },
    /// What the selected builders can raise.
    Build {
        /// Zero-based page.
        page: u8,
    },
}

impl CardPage {
    /// The first page of the root.
    pub const HOME: Self = Self::Root { page: 0 };

    /// The page [`CardAction::Back`] leads to.
    #[must_use]
    pub const fn previous(self) -> Self {
        match self {
            Self::Root { page } => Self::Root {
                page: page.saturating_sub(1),
            },
            Self::Build { page: 0 } => Self::HOME,
            Self::Build { page } => Self::Build { page: page - 1 },
        }
    }

    /// The page [`CardAction::NextPage`] leads to. [`derive_card`] clamps it to the list.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Root { page } => Self::Root {
                page: page.saturating_add(1),
            },
            Self::Build { page } => Self::Build {
                page: page.saturating_add(1),
            },
        }
    }

    const fn with_index(self, page: u8) -> Self {
        match self {
            Self::Root { .. } => Self::Root { page },
            Self::Build { .. } => Self::Build { page },
        }
    }

    const fn index(self) -> u8 {
        match self {
            Self::Root { page } | Self::Build { page } => page,
        }
    }
}

impl Default for CardPage {
    fn default() -> Self {
        Self::HOME
    }
}

/// The identity a slot shows, so that the slot owns no string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardLabel {
    /// Attack-move.
    Attack,
    /// Opens the structure list.
    Build,
    /// Carry a load home.
    ReturnCargo,
    /// The overclock ability.
    Overclock,
    /// Empty a carrier.
    Unload,
    /// Cancel a foundation.
    CancelBuild,
    /// Stop.
    Stop,
    /// Hold position.
    Hold,
    /// A catalog identity, trained or raised.
    Unit(UnitTypeId),
    /// The upgrade with this catalog ID.
    Upgrade(u32),
    /// The next page.
    More,
    /// The previous page.
    Back,
}

/// Why a slot cannot be used now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardLock {
    /// The simulation's own account of what is missing.
    Option(OptionLock),
    /// The product has no media the client could present.
    NoMedia,
    /// The ability needs this upgrade first, if the catalog names one.
    NeedsResearch(Option<u32>),
}

/// What a slot costs, for its tooltip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CardPrice {
    /// Mineral cost.
    pub minerals: u32,
    /// Gas cost.
    pub gas: u32,
    /// Supply the job reserves.
    pub supply: u32,
}

/// One filled position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardSlot {
    /// What pressing it does.
    pub action: CardAction,
    /// Whether it can be used now: `lock.is_none()`.
    pub enabled: bool,
    /// Which key shows on the button, and fires it.
    pub key: KeyCode,
    /// Identity in the catalog.
    pub label: CardLabel,
    /// First reason this cannot be used, if any.
    pub lock: Option<CardLock>,
    /// What it costs, if it costs anything.
    pub price: Option<CardPrice>,
}

/// The cached words of one slot.
#[derive(Debug, Default)]
pub struct SlotText {
    /// Key and name, as drawn on the button.
    pub caption: String,
    /// The reason it is locked, otherwise its price. Empty when it has neither.
    pub tooltip: String,
}

/// The card on show.
#[derive(Resource, Debug, Default)]
pub struct CommandCard {
    /// The fifteen positions, in the reading order of
    /// [`COMMAND_CARD_KEYS`](crate::input_bindings::COMMAND_CARD_KEYS).
    pub slots: [Option<CardSlot>; COMMAND_CARD_SLOTS],
    /// Stop and Hold, which keep their own keys.
    pub fixed: [Option<CardSlot>; FIXED_SLOTS],
    /// The list on show.
    pub page: CardPage,
    /// Words of `slots`, by position.
    pub text: [SlotText; COMMAND_CARD_SLOTS],
    /// Words of `fixed`, by position.
    pub fixed_text: [SlotText; FIXED_SLOTS],
    derived_page: Option<CardPage>,
    seen_refills: u64,
    seen_selection: u64,
    seen_mode: CommandMode,
}

impl CommandCard {
    /// Returns to the root if the selection changed since this card last looked.
    ///
    /// The refresh and a key between two refreshes both come here, so a page a key opens for the
    /// new selection is not closed again by the refresh that follows.
    pub fn follow_selection(&mut self, model: &HudModel) {
        if self.seen_selection != model.selection_changes() {
            self.seen_selection = model.selection_changes();
            self.page = CardPage::HOME;
            derive_card(model, model.selected_facts(), self);
        }
    }
}

/// One candidate entry, before it has a position.
struct CardEntry {
    action: CardAction,
    label: CardLabel,
    lock: Option<CardLock>,
    price: Option<CardPrice>,
}

impl CardEntry {
    const fn free(action: CardAction, label: CardLabel) -> Self {
        Self {
            action,
            label,
            lock: None,
            price: None,
        }
    }
}

/// A selected unit that can act on its archetype, which rules out a foundation.
fn completed<'a>(
    model: &'a HudModel,
    facts: &'a [SelectedFact],
) -> impl Iterator<Item = &'a ArchetypeCommands> {
    facts
        .iter()
        .filter(|fact| !fact.foundation)
        .filter_map(|fact| model.catalog.archetype(fact.unit_type))
}

fn sorted_contains(sorted: &[EntityId], id: EntityId) -> bool {
    sorted.binary_search(&id).is_ok()
}

fn carries_passengers(view: &PlayerView, id: EntityId) -> bool {
    view.carriers
        .binary_search_by_key(&id, |carrier| carrier.entity)
        .is_ok_and(|index| view.carriers[index].slots_used > 0)
}

/// The lock of `option`, media first: a player cannot do anything about that one.
fn option_lock(model: &HudModel, option: &CommandOptionView) -> Option<CardLock> {
    let product = match option.subject {
        CommandSubject::Train(unit_type) | CommandSubject::Raise(unit_type) => Some(unit_type),
        CommandSubject::Research(_) => None,
    };
    let unpresentable = product
        .and_then(|unit_type| model.catalog.archetype(unit_type))
        .is_some_and(|archetype| archetype.unpresentable.is_some());
    if unpresentable {
        Some(CardLock::NoMedia)
    } else {
        option.lock.map(CardLock::Option)
    }
}

fn option_entry(
    model: &HudModel,
    option: &CommandOptionView,
    action: CardAction,
    label: CardLabel,
) -> CardEntry {
    CardEntry {
        action,
        label,
        lock: option_lock(model, option),
        price: Some(CardPrice {
            minerals: option.minerals,
            gas: option.gas,
            supply: option.supply,
        }),
    }
}

/// The overclock entry, if a selected unit has that ability.
fn overclock_entry(model: &HudModel, facts: &[SelectedFact]) -> Option<CardEntry> {
    let holders = || {
        facts
            .iter()
            .filter(|fact| !fact.foundation)
            .filter_map(|fact| {
                let archetype = model.catalog.archetype(fact.unit_type)?;
                archetype
                    .overclock
                    .then_some((fact.unit_type, archetype.overclock_unlock))
            })
    };
    let (_, unlock) = holders().next()?;
    let ready = holders()
        .any(|(unit_type, _)| model.view.overclock_ready.binary_search(&unit_type).is_ok());
    Some(CardEntry {
        lock: (!ready).then_some(CardLock::NeedsResearch(unlock)),
        ..CardEntry::free(CardAction::Order(UnitOrder::UseStim), CardLabel::Overclock)
    })
}

/// Calls `emit` with each root entry, in the order of the card's table.
fn root_entries(model: &HudModel, facts: &[SelectedFact], mut emit: impl FnMut(CardEntry)) {
    let view = &model.view;
    if completed(model, facts).any(|archetype| archetype.mobile && archetype.armed) {
        emit(CardEntry::free(
            CardAction::Mode(CommandMode::AttackMove),
            CardLabel::Attack,
        ));
    }
    if completed(model, facts).any(|archetype| !archetype.raises.is_empty()) {
        emit(CardEntry::free(CardAction::OpenBuildMenu, CardLabel::Build));
    }
    if facts
        .iter()
        .any(|fact| sorted_contains(&view.laden, fact.id))
    {
        emit(CardEntry::free(
            CardAction::Order(UnitOrder::ReturnCargo),
            CardLabel::ReturnCargo,
        ));
    }
    if let Some(entry) = overclock_entry(model, facts) {
        emit(entry);
    }
    if facts.iter().any(|fact| carries_passengers(view, fact.id)) {
        emit(CardEntry::free(
            CardAction::Order(UnitOrder::UnloadAll),
            CardLabel::Unload,
        ));
    }
    if facts.iter().any(|fact| fact.foundation) {
        emit(CardEntry::free(
            CardAction::Order(UnitOrder::CancelConstruction),
            CardLabel::CancelBuild,
        ));
    }
    // `options` sorts every train before every research, and each list by catalog identity.
    for option in &view.options {
        match option.subject {
            CommandSubject::Train(unit_type)
                if completed(model, facts).any(|a| a.trains.contains(&unit_type)) =>
            {
                emit(option_entry(
                    model,
                    option,
                    CardAction::Train(unit_type),
                    CardLabel::Unit(unit_type),
                ));
            }
            CommandSubject::Research(upgrade)
                if completed(model, facts).any(|a| a.researches.contains(&upgrade)) =>
            {
                emit(option_entry(
                    model,
                    option,
                    CardAction::Research(upgrade),
                    CardLabel::Upgrade(upgrade),
                ));
            }
            _ => {}
        }
    }
}

/// Calls `emit` with each structure the selected builders can raise, in catalog order.
fn build_entries(model: &HudModel, facts: &[SelectedFact], mut emit: impl FnMut(CardEntry)) {
    for option in &model.view.options {
        if let CommandSubject::Raise(unit_type) = option.subject {
            if completed(model, facts).any(|a| a.raises.contains(&unit_type)) {
                emit(option_entry(
                    model,
                    option,
                    CardAction::Mode(CommandMode::Place(unit_type)),
                    CardLabel::Unit(unit_type),
                ));
            }
        }
    }
}

fn key_caption(key: KeyCode) -> &'static str {
    if key == STOP_KEY {
        return STOP_KEY_LABEL;
    }
    if key == HOLD_POSITION_KEY {
        return HOLD_POSITION_KEY_LABEL;
    }
    COMMAND_CARD_KEYS
        .iter()
        .position(|candidate| *candidate == key)
        .map_or("", |position| COMMAND_CARD_KEY_LABELS[position])
}

fn label_text(label: CardLabel, model: &HudModel) -> &str {
    match label {
        CardLabel::Attack => "Attack",
        CardLabel::Build => "Build",
        CardLabel::ReturnCargo => "Return cargo",
        CardLabel::Overclock => "Overclock",
        CardLabel::Unload => "Unload",
        CardLabel::CancelBuild => "Cancel build",
        CardLabel::Stop => "Stop",
        CardLabel::Hold => "Hold",
        CardLabel::Unit(unit_type) => model.catalog.label(unit_type),
        CardLabel::Upgrade(upgrade) => model
            .catalog
            .upgrade(upgrade)
            .map_or("", |upgrade| upgrade.label.as_str()),
        CardLabel::More => "More",
        CardLabel::Back => "Back",
    }
}

fn write_lock(tooltip: &mut String, lock: CardLock, model: &HudModel) {
    let _ = match lock {
        CardLock::NoMedia => tooltip.write_str(NO_MEDIA_TEXT),
        CardLock::Option(OptionLock::MissingStructure(unit_type)) => {
            write!(tooltip, "Requires {}", model.catalog.label(unit_type))
        }
        CardLock::Option(OptionLock::TierLocked) => {
            tooltip.write_str("Requires the previous level")
        }
        CardLock::Option(OptionLock::Researched) => tooltip.write_str("Researched"),
        CardLock::Option(OptionLock::Researching) => tooltip.write_str("In progress"),
        CardLock::NeedsResearch(upgrade) => {
            let name = upgrade.and_then(|upgrade| model.catalog.upgrade(upgrade));
            match name {
                Some(upgrade) => write!(tooltip, "Requires {}", upgrade.label),
                None => tooltip.write_str("Requires research"),
            }
        }
    };
}

fn write_price(tooltip: &mut String, price: CardPrice) {
    let _ = write!(tooltip, "{} minerals", price.minerals);
    if price.gas > 0 {
        let _ = write!(tooltip, ", {} gas", price.gas);
    }
    if price.supply > 0 {
        let _ = write!(tooltip, ", {} supply", price.supply);
    }
}

/// Rewrites `text` for `slot`.
fn write_text(text: &mut SlotText, slot: &CardSlot, model: &HudModel) {
    text.caption.clear();
    text.tooltip.clear();
    let _ = write!(
        text.caption,
        "{}  {}",
        key_caption(slot.key),
        label_text(slot.label, model)
    );
    if let Some(lock) = slot.lock {
        write_lock(&mut text.tooltip, lock, model);
    } else if let Some(price) = slot.price {
        write_price(&mut text.tooltip, price);
    }
}

/// Stores `slot` at a position, rewriting its words only if it changed.
fn store(
    position: &mut Option<CardSlot>,
    text: &mut SlotText,
    slot: Option<CardSlot>,
    model: &HudModel,
) {
    if *position == slot {
        return;
    }
    if let Some(slot) = &slot {
        write_text(text, slot, model);
    }
    *position = slot;
}

fn filled(entry: CardEntry, key: KeyCode) -> CardSlot {
    CardSlot {
        enabled: entry.lock.is_none(),
        action: entry.action,
        key,
        label: entry.label,
        lock: entry.lock,
        price: entry.price,
    }
}

/// Fills `card` for the selection described by `facts`, on `card.page`.
///
/// `facts` holds the viewer's selected units, as
/// [`HudModel::selected_facts`] gives them. Pure, so every case is tested without Bevy. A page that
/// no longer exists is clamped to the last one that does.
// PERFORMANCE RATIONALE:
// Why: With units selected this runs on every tick, and a card can hold a dozen strings.
// What: Entries are visited twice through a callback, once to count and once to place, rather than
// collected, and a slot's text is rewritten only when that slot changed.
// Trade-off: Each list is walked twice, over small sets, in exchange for no allocation when warm.
pub fn derive_card(model: &HudModel, facts: &[SelectedFact], card: &mut CommandCard) {
    let usable = model.orders_open && !model.view.status.defeated && !facts.is_empty();
    if !usable {
        card.page = CardPage::HOME;
        card.slots = Default::default();
        card.fixed = Default::default();
        return;
    }
    let raises = completed(model, facts).any(|archetype| !archetype.raises.is_empty());
    if matches!(card.page, CardPage::Build { .. }) && !raises {
        card.page = CardPage::HOME;
    }
    let building = matches!(card.page, CardPage::Build { .. });
    let visit = |emit: &mut dyn FnMut(CardEntry)| {
        if building {
            build_entries(model, facts, emit);
        } else {
            root_entries(model, facts, emit);
        }
    };

    let mut total = 0_usize;
    visit(&mut |_| total += 1);
    let paged = building || total > COMMAND_CARD_SLOTS;
    let capacity = if paged {
        PAGED_ENTRIES
    } else {
        COMMAND_CARD_SLOTS
    };
    let last_page = total.saturating_sub(1) / capacity;
    let page = usize::from(card.page.index()).min(last_page);
    card.page = card.page.with_index(u8::try_from(page).unwrap_or(u8::MAX));

    let first = page * capacity;
    let mut index = 0_usize;
    visit(&mut |entry| {
        if let Some(position) = index.checked_sub(first).filter(|at| *at < capacity) {
            let slot = filled(entry, COMMAND_CARD_KEYS[position]);
            store(
                &mut card.slots[position],
                &mut card.text[position],
                Some(slot),
                model,
            );
        }
        index += 1;
    });
    let placed = total.saturating_sub(first).min(capacity);
    let positions = card.slots.iter_mut().zip(&mut card.text);
    for (position, (stored, text)) in positions.enumerate().skip(placed) {
        let control = if !paged {
            None
        } else if position == MORE_POSITION && page < last_page {
            Some(CardEntry::free(CardAction::NextPage, CardLabel::More))
        } else if position == BACK_POSITION && (building || page > 0) {
            Some(CardEntry::free(CardAction::Back, CardLabel::Back))
        } else {
            None
        };
        let slot = control.map(|entry| filled(entry, COMMAND_CARD_KEYS[position]));
        store(stored, text, slot, model);
    }
    derive_fixed(model, facts, card);
}

/// Stop and Hold, shown when a selected unit can move.
fn derive_fixed(model: &HudModel, facts: &[SelectedFact], card: &mut CommandCard) {
    let mobile = completed(model, facts).any(|archetype| archetype.mobile);
    let entries = [
        (UnitOrder::Stop, CardLabel::Stop, STOP_KEY),
        (UnitOrder::HoldPosition, CardLabel::Hold, HOLD_POSITION_KEY),
    ];
    for ((order, label, key), (position, text)) in entries
        .into_iter()
        .zip(card.fixed.iter_mut().zip(&mut card.fixed_text))
    {
        let slot = mobile.then(|| filled(CardEntry::free(CardAction::Order(order), label), key));
        store(position, text, slot, model);
    }
}

/// Keeps [`CommandCard`] in step with the model and the page.
///
/// A new selection, or the end of a placement, returns the card to its root. Re-derives only when
/// the model was rewritten or the page moved.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn refresh_command_card(
    model: Res<'_, HudModel>,
    mode: Res<'_, CommandMode>,
    mut card: ResMut<'_, CommandCard>,
) {
    let card = card.bypass_change_detection();
    let placed =
        matches!(card.seen_mode, CommandMode::Place(_)) && !matches!(*mode, CommandMode::Place(_));
    card.seen_mode = *mode;
    if placed {
        card.page = CardPage::HOME;
    }
    card.follow_selection(&model);
    if card.seen_refills != model.refills() || card.derived_page != Some(card.page) {
        card.seen_refills = model.refills();
        derive_card(&model, model.selected_facts(), card);
        card.derived_page = Some(card.page);
    }
}

#[cfg(test)]
mod tests {
    use super::super::catalog::CommandCatalog;
    use super::*;
    use sim_core::entity::Cargo;
    use sim_core::{SimWorld, UnitArchetype};
    use sim_map::{CliffLevel, GameMap, MapCell, ResourceKind};
    use sim_types::PlayerSlot;
    use sim_types::{MatchResult, PlayerId, ShapeKind, SimScalar, SimVec2, Tick};

    type TestResult = Result<(), Box<dyn std::error::Error>>;
    type Outcome<T> = Result<T, Box<dyn std::error::Error>>;

    const ME: PlayerId = PlayerId(0);

    fn flat_world() -> Outcome<SimWorld> {
        let map = GameMap::new(96, 96, MapCell::default_ground(CliffLevel(0)))?;
        Ok(sim_fixtures::world(map, 21))
    }

    fn is_dome(archetype: &UnitArchetype) -> bool {
        matches!(archetype.shape, ShapeKind::BuildingDome { .. })
    }

    fn type_where(
        world: &SimWorld,
        wanted: impl Fn(&UnitArchetype) -> bool,
    ) -> Outcome<UnitTypeId> {
        Ok(world
            .catalog
            .archetypes()
            .find(|archetype| wanted(archetype))
            .map(|archetype| archetype.unit_type_id)
            .ok_or("the catalog has no such role")?)
    }

    fn unit_where(
        world: &mut SimWorld,
        wanted: impl Fn(&UnitArchetype) -> bool,
    ) -> Outcome<EntityId> {
        let unit_type = type_where(world, |a| !is_dome(a) && wanted(a))?;
        let slot = i32::try_from(world.entities.len())?;
        let at = SimVec2::new(SimScalar::from_i32(4 + 3 * slot), SimScalar::from_i32(10));
        Ok(world.spawn_unit(unit_type, ME, at)?)
    }

    fn worker(world: &mut SimWorld) -> Outcome<EntityId> {
        unit_where(world, |a| {
            a.capabilities.harvester.is_some() && a.capabilities.can_construct
        })
    }

    /// The first structure a worker's faction trains something at.
    fn factory(world: &mut SimWorld, corner: u32) -> Outcome<EntityId> {
        let builder = type_where(world, |a| {
            !is_dome(a) && a.capabilities.harvester.is_some() && a.capabilities.can_construct
        })?;
        let faction = world.catalog.get(builder)?.faction;
        let unit_type = type_where(world, |a| {
            is_dome(a) && a.faction == faction && !a.trains_units.is_empty()
        })?;
        Ok(world.spawn_structure(unit_type, ME, corner, corner)?)
    }

    /// The model of `world` as `ME` sees it, with media refused for every type in `unpresentable`.
    fn model_of(world: &SimWorld, unpresentable: &[UnitTypeId]) -> HudModel {
        let mut model = HudModel::default();
        model.catalog =
            CommandCatalog::build(&world.catalog, &world.upgrade_catalog, |unit_type| {
                unpresentable
                    .contains(&unit_type)
                    .then(|| "no media".to_owned())
            });
        model.orders_open = !world.match_state.result.is_over();
        world.write_player_view(ME, &mut model.view);
        model
    }

    fn fact(world: &SimWorld, id: EntityId) -> SelectedFact {
        SelectedFact {
            id,
            unit_type: world.entities[&id].unit_type_id,
            foundation: !world.entities[&id].is_completed,
            build_progress: 0.0,
        }
    }

    fn card_for(world: &SimWorld, selected: &[EntityId]) -> CommandCard {
        let facts: Vec<_> = selected.iter().map(|id| fact(world, *id)).collect();
        let mut card = CommandCard::default();
        derive_card(&model_of(world, &[]), &facts, &mut card);
        card
    }

    fn labels(card: &CommandCard) -> Vec<CardLabel> {
        card.slots.iter().flatten().map(|slot| slot.label).collect()
    }

    fn slot_with(card: &CommandCard, label: CardLabel) -> Option<&CardSlot> {
        card.slots.iter().flatten().find(|slot| slot.label == label)
    }

    /// Opens the build page for `selected`.
    fn build_page(world: &SimWorld, selected: &[EntityId], page: u8) -> CommandCard {
        let facts: Vec<_> = selected.iter().map(|id| fact(world, *id)).collect();
        let mut card = CommandCard {
            page: CardPage::Build { page },
            ..CommandCard::default()
        };
        derive_card(&model_of(world, &[]), &facts, &mut card);
        card
    }

    #[test]
    fn a_default_card_is_empty() {
        let card = CommandCard::default();
        assert!(card.slots.iter().all(Option::is_none));
        assert!(card.fixed.iter().all(Option::is_none));
        assert_eq!(card.page, CardPage::HOME);
    }

    #[test]
    fn a_worker_can_attack_build_stop_and_hold() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world)?;
        let card = card_for(&world, &[unit]);
        assert_eq!(labels(&card), [CardLabel::Attack, CardLabel::Build]);
        let [Some(stop), Some(hold)] = &card.fixed else {
            return Err("a mobile unit shows Stop and Hold".into());
        };
        assert_eq!(stop.key, STOP_KEY);
        assert_eq!(hold.key, HOLD_POSITION_KEY);
        Ok(())
    }

    #[test]
    fn a_position_keeps_its_key_whatever_fills_it() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world)?;
        let card = card_for(&world, &[unit]);
        for (slot, key) in card.slots.iter().zip(COMMAND_CARD_KEYS) {
            if let Some(slot) = slot {
                assert_eq!(slot.key, key);
            }
        }
        Ok(())
    }

    #[test]
    fn a_structure_shows_what_it_trains_and_no_fixed_entries() -> TestResult {
        let mut world = flat_world()?;
        let structure = factory(&mut world, 30)?;
        let card = card_for(&world, &[structure]);
        let trained = world
            .catalog
            .get(world.entities[&structure].unit_type_id)?
            .trains_units
            .iter()
            .filter_map(|slug| world.catalog.get_by_slug(slug))
            .filter(|product| !is_dome(product))
            .count();
        assert!(trained > 0);
        let trains = card
            .slots
            .iter()
            .flatten()
            .filter(|slot| matches!(slot.action, CardAction::Train(_)))
            .count();
        assert_eq!(trains, trained);
        assert!(card.fixed.iter().all(Option::is_none));
        Ok(())
    }

    #[test]
    fn a_foundation_offers_only_its_cancellation() -> TestResult {
        let mut world = flat_world()?;
        let structure = factory(&mut world, 30)?;
        world
            .entities
            .get_mut(&structure)
            .ok_or("absent")?
            .is_completed = false;
        let mut model = model_of(&world, &[]);
        model.view.options.clear();
        let mut card = CommandCard::default();
        derive_card(&model, &[fact(&world, structure)], &mut card);
        assert_eq!(labels(&card), [CardLabel::CancelBuild]);
        assert_eq!(
            card.slots[0].as_ref().ok_or("slot")?.action,
            CardAction::Order(UnitOrder::CancelConstruction)
        );
        Ok(())
    }

    #[test]
    fn a_mixed_group_shows_the_union_without_repeats() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world)?;
        let other = worker(&mut world)?;
        let structure = factory(&mut world, 30)?;
        let card = card_for(&world, &[unit, other, structure]);
        let shown = labels(&card);
        assert_eq!(shown[..2], [CardLabel::Attack, CardLabel::Build]);
        assert!(shown[2..]
            .iter()
            .all(|label| matches!(label, CardLabel::Unit(_))));
        let mut distinct = shown.clone();
        distinct.dedup();
        assert_eq!(distinct, shown);
        assert!(card.fixed.iter().all(Option::is_some));
        Ok(())
    }

    #[test]
    fn an_inspected_enemy_or_an_empty_selection_gives_an_empty_card() -> TestResult {
        let mut world = flat_world()?;
        worker(&mut world)?;
        let card = card_for(&world, &[]);
        assert!(card.slots.iter().all(Option::is_none));
        assert!(card.fixed.iter().all(Option::is_none));
        Ok(())
    }

    #[test]
    fn a_finished_match_empties_the_card_and_returns_home() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world)?;
        let mut card = build_page(&world, &[unit], 0);
        assert!(card.slots.iter().any(Option::is_some));

        world.match_state.result = MatchResult::Draw { tick: Tick(1) };
        derive_card(&model_of(&world, &[]), &[fact(&world, unit)], &mut card);

        assert!(card.slots.iter().all(Option::is_none));
        assert!(card.fixed.iter().all(Option::is_none));
        assert_eq!(card.page, CardPage::HOME);
        Ok(())
    }

    #[test]
    fn a_defeated_player_gets_no_card() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world)?;
        let mut model = model_of(&world, &[]);
        model.view.status.defeated = true;
        let mut card = CommandCard::default();
        derive_card(&model, &[fact(&world, unit)], &mut card);
        assert!(card.slots.iter().all(Option::is_none));
        Ok(())
    }

    #[test]
    fn a_laden_worker_can_return_its_cargo_and_an_unladen_one_cannot() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world)?;
        assert_eq!(
            slot_with(&card_for(&world, &[unit]), CardLabel::ReturnCargo),
            None
        );

        world.entities.get_mut(&unit).ok_or("absent")?.cargo = Some(Cargo {
            kind: ResourceKind::Mineral,
            amount: 5,
        });
        let card = card_for(&world, &[unit]);
        let slot = slot_with(&card, CardLabel::ReturnCargo).ok_or("no entry")?;
        assert_eq!(slot.action, CardAction::Order(UnitOrder::ReturnCargo));
        Ok(())
    }

    #[test]
    fn a_carrier_offers_unload_only_with_passengers() -> TestResult {
        let mut world = flat_world()?;
        let carrier = unit_where(&mut world, |a| a.capabilities.transport_capacity > 0)?;
        assert_eq!(
            slot_with(&card_for(&world, &[carrier]), CardLabel::Unload),
            None
        );

        let mut model = model_of(&world, &[]);
        model.view.carriers[0].slots_used = 2;
        let mut card = CommandCard::default();
        derive_card(&model, &[fact(&world, carrier)], &mut card);
        let slot = slot_with(&card, CardLabel::Unload).ok_or("no entry")?;
        assert_eq!(slot.action, CardAction::Order(UnitOrder::UnloadAll));
        Ok(())
    }

    #[test]
    fn overclock_is_disabled_until_researched_and_names_what_to_research() -> TestResult {
        let mut world = flat_world()?;
        let unit = unit_where(&mut world, |a| a.overclock.is_some())?;
        let unit_type = world.entities[&unit].unit_type_id;
        let mut model = model_of(&world, &[]);
        assert_eq!(model.view.overclock_ready, []);
        let mut card = CommandCard::default();
        derive_card(&model, &[fact(&world, unit)], &mut card);
        let (position, slot) = card
            .slots
            .iter()
            .flatten()
            .enumerate()
            .find(|(_, slot)| slot.label == CardLabel::Overclock)
            .ok_or("no entry")?;
        assert!(!slot.enabled);
        assert!(matches!(slot.lock, Some(CardLock::NeedsResearch(Some(_)))));
        let needed = world
            .upgrade_catalog
            .upgrades()
            .find(|upgrade| {
                Some(upgrade.upgrade_id)
                    == model
                        .catalog
                        .archetype(unit_type)
                        .and_then(|a| a.overclock_unlock)
            })
            .ok_or("no upgrade unlocks it")?;
        assert_eq!(
            card.text[position].tooltip,
            format!("Requires {}", needed.display_name)
        );

        model.view.overclock_ready.push(unit_type);
        derive_card(&model, &[fact(&world, unit)], &mut card);
        let slot = slot_with(&card, CardLabel::Overclock).ok_or("no entry")?;
        assert!(slot.enabled);
        assert_eq!(slot.action, CardAction::Order(UnitOrder::UseStim));
        Ok(())
    }

    #[test]
    fn the_build_page_lists_every_raisable_structure_across_pages() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world)?;
        let unit_type = world.entities[&unit].unit_type_id;
        let raisable = model_of(&world, &[])
            .catalog
            .archetype(unit_type)
            .ok_or("no entry")?
            .raises
            .clone();
        assert!(
            raisable.len() > PAGED_ENTRIES,
            "the test needs a list that pages"
        );

        let first = build_page(&world, &[unit], 0);
        let second = build_page(&world, &[unit], 1);
        let shown: Vec<_> = [&first, &second]
            .into_iter()
            .flat_map(|card| card.slots.iter().flatten())
            .filter_map(|slot| match slot.action {
                CardAction::Mode(CommandMode::Place(structure)) => Some(structure),
                _ => None,
            })
            .collect();
        assert_eq!(shown, raisable);

        assert_eq!(
            first.slots[MORE_POSITION].as_ref().ok_or("More")?.action,
            CardAction::NextPage
        );
        assert_eq!(
            first.slots[BACK_POSITION].as_ref().ok_or("Back")?.action,
            CardAction::Back
        );
        assert_eq!(second.slots[MORE_POSITION], None);
        assert_eq!(
            second.slots[BACK_POSITION].as_ref().ok_or("Back")?.action,
            CardAction::Back
        );
        Ok(())
    }

    #[test]
    fn a_page_past_the_end_is_clamped() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world)?;
        let card = build_page(&world, &[unit], 200);
        assert_eq!(card.page, CardPage::Build { page: 1 });
        assert!(card.slots.iter().any(Option::is_some));
        Ok(())
    }

    #[test]
    fn a_build_page_with_no_builder_left_falls_back_to_the_root() -> TestResult {
        let mut world = flat_world()?;
        let structure = factory(&mut world, 30)?;
        let card = build_page(&world, &[structure], 0);
        assert_eq!(card.page, CardPage::HOME);
        Ok(())
    }

    #[test]
    fn a_tech_lock_is_shown_disabled_with_what_is_missing() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world)?;
        let card = build_page(&world, &[unit], 0);
        let (position, slot) = card
            .slots
            .iter()
            .enumerate()
            .find_map(|(position, slot)| {
                let slot = slot.as_ref()?;
                matches!(
                    slot.lock,
                    Some(CardLock::Option(OptionLock::MissingStructure(_)))
                )
                .then_some((position, slot))
            })
            .ok_or("nothing on the first page is tech locked")?;
        assert!(!slot.enabled);
        let Some(CardLock::Option(OptionLock::MissingStructure(needed))) = slot.lock else {
            return Err("the lock changed".into());
        };
        let name = &world.catalog.get(needed)?.display_name;
        assert_eq!(card.text[position].tooltip, format!("Requires {name}"));
        Ok(())
    }

    #[test]
    fn an_unpresentable_product_is_disabled_and_outranks_a_tech_lock() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world)?;
        let raised = world.entities[&unit].unit_type_id;
        let everything = model_of(&world, &[])
            .catalog
            .archetype(raised)
            .ok_or("no entry")?
            .raises
            .clone();
        let mut card = CommandCard {
            page: CardPage::Build { page: 0 },
            ..CommandCard::default()
        };
        derive_card(
            &model_of(&world, &everything),
            &[fact(&world, unit)],
            &mut card,
        );
        for (slot, text) in card.slots.iter().zip(&card.text).take(PAGED_ENTRIES) {
            let slot = slot.as_ref().ok_or("a full page")?;
            assert_eq!(slot.lock, Some(CardLock::NoMedia));
            assert!(!slot.enabled);
            assert_eq!(text.tooltip, NO_MEDIA_TEXT);
        }
        Ok(())
    }

    #[test]
    fn a_price_is_a_tooltip_and_shortage_is_not_a_lock() -> TestResult {
        let mut world = flat_world()?;
        let structure = factory(&mut world, 30)?;
        world.player_resources[PlayerSlot::ALL[0]].minerals = 0;
        let card = card_for(&world, &[structure]);
        let (position, slot) = card
            .slots
            .iter()
            .enumerate()
            .find_map(|(position, slot)| Some((position, slot.as_ref()?)))
            .ok_or("no entry")?;
        assert!(slot.enabled, "income can change between frames");
        let price = slot.price.ok_or("a train entry has a price")?;
        assert!(card.text[position]
            .tooltip
            .contains(&format!("{} minerals", price.minerals)));
        Ok(())
    }

    #[test]
    fn a_selection_too_wide_for_one_card_pages_instead_of_dropping_entries() -> TestResult {
        let mut world = flat_world()?;
        // One structure that researches everything stands for several that research a part each.
        let researcher = type_where(&world, |a| is_dome(a) && a.trains_units.is_empty())?;
        world.catalog.get_mut(researcher)?.researches_upgrades = world
            .upgrade_catalog
            .upgrades()
            .map(|upgrade| upgrade.slug.clone())
            .collect();
        let mut model = model_of(&world, &[]);
        let upgrades: Vec<u32> = (1_u32..)
            .take(COMMAND_CARD_SLOTS + 2)
            .inspect(|upgrade| {
                assert!(model.catalog.upgrade(*upgrade).is_some());
            })
            .collect();
        model.view.options = upgrades
            .iter()
            .map(|upgrade| CommandOptionView {
                subject: CommandSubject::Research(*upgrade),
                lock: None,
                minerals: 1,
                gas: 1,
                supply: 0,
            })
            .collect();
        let structure = SelectedFact {
            id: EntityId(1),
            unit_type: researcher,
            foundation: false,
            build_progress: 0.0,
        };

        let mut seen = Vec::new();
        let mut card = CommandCard::default();
        loop {
            derive_card(&model, &[structure], &mut card);
            seen.extend(
                card.slots
                    .iter()
                    .flatten()
                    .filter_map(|slot| match slot.action {
                        CardAction::Research(upgrade) => Some(upgrade),
                        _ => None,
                    }),
            );
            if card.slots[MORE_POSITION].is_none() {
                break;
            }
            card.page = card.page.next();
        }
        assert_eq!(seen, upgrades);
        assert_eq!(
            card.slots[BACK_POSITION].as_ref().ok_or("Back")?.action,
            CardAction::Back
        );
        Ok(())
    }

    #[test]
    fn back_walks_pages_and_stops_at_home() {
        let page = CardPage::Build { page: 1 };
        assert_eq!(page.previous(), CardPage::Build { page: 0 });
        assert_eq!(page.previous().previous(), CardPage::HOME);
        assert_eq!(CardPage::HOME.previous(), CardPage::HOME);
        assert_eq!(
            CardPage::Root { page: 2 }.previous(),
            CardPage::Root { page: 1 }
        );
    }

    #[test]
    fn a_warm_card_rewrites_no_text_when_nothing_changed() -> TestResult {
        let mut world = flat_world()?;
        let unit = worker(&mut world)?;
        let model = model_of(&world, &[]);
        let facts = [fact(&world, unit)];
        let mut card = CommandCard::default();
        derive_card(&model, &facts, &mut card);
        let pointer = card.text[0].caption.as_ptr();
        card.text[0].caption.push('!');

        derive_card(&model, &facts, &mut card);

        assert_eq!(card.text[0].caption.as_ptr(), pointer);
        assert!(
            card.text[0].caption.ends_with('!'),
            "an unchanged slot keeps its text"
        );
        Ok(())
    }
}
