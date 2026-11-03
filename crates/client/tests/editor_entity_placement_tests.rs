//! Characterisation tests for the editor's single-cell entity placement under symmetry.
//!
//! # Responsibility
//! Pins the exact [`EditCommand`] that [`place_entity_command`] builds for minerals, gas geysers
//! and watchtowers: the entity kind and payload, the document's next entity ID for the primary,
//! the following ID for its mirror, and the mirrored cell. Phase 9 (WP6, 9.2) split the function
//! into one helper per entity family; these tests keep that refactor honest.
//!
//! # Invariants checked
//! 1. Without symmetry exactly one entity is added, with [`MapDocument::next_entity_id`].
//! 2. Under symmetry the mirror is added with the next ID after the primary, at the mirrored cell.
//! 3. A cell that mirrors onto itself (on the symmetry axis) is placed once.
//!
//! Start locations are covered by `editor_core_tests.rs`.

#![allow(clippy::expect_used, clippy::panic)]

use client::editor::commands::{EditCommand, MapEntity};
use client::editor::symmetry::SymmetryMode;
use client::editor::tools::{
    place_entity_command, EntityToolKind, DEFAULT_GAS_AMOUNT, DEFAULT_MINERAL_AMOUNT,
    DEFAULT_WATCHTOWER_CAPTURE_RADIUS_TENTHS, DEFAULT_WATCHTOWER_REVEAL_RADIUS_TENTHS,
};
use sim_map::grid::ResourceKind;
use sim_map::map_format::{MapDocument, MapEntityId, MapMetadata, MapResource, MapWatchtower};

/// A blank document of `width × height` cells with no entities, so its next entity ID is 1.
fn blank_document(width: u32, height: u32) -> MapDocument {
    let metadata = MapMetadata {
        title: "Placement Test Map".to_string(),
        author: "Editor Placement Tests".to_string(),
        description: "Fixture map for entity placement characterisation".to_string(),
        theme: "orbital_foundry".to_string(),
    };
    MapDocument::new_blank(width, height, metadata).expect("invariant: valid fixture dimensions")
}

/// The entity `kind` would place with ID `id` at `cell`.
fn expected_entity(kind: EntityToolKind, id: u32, cell: (u32, u32)) -> MapEntity {
    let id = MapEntityId(id);
    match kind {
        EntityToolKind::Mineral => MapEntity::Resource(MapResource {
            id,
            kind: ResourceKind::Mineral,
            cell,
            amount: DEFAULT_MINERAL_AMOUNT,
        }),
        EntityToolKind::Gas => MapEntity::Resource(MapResource {
            id,
            kind: ResourceKind::Gas,
            cell,
            amount: DEFAULT_GAS_AMOUNT,
        }),
        EntityToolKind::Watchtower => MapEntity::Watchtower(MapWatchtower {
            id,
            cell,
            capture_radius_tenths: DEFAULT_WATCHTOWER_CAPTURE_RADIUS_TENTHS,
            reveal_radius_tenths: DEFAULT_WATCHTOWER_REVEAL_RADIUS_TENTHS,
        }),
        EntityToolKind::StartLocation { .. } => {
            panic!("start locations are not single-cell entities")
        }
    }
}

const SINGLE_CELL_KINDS: [EntityToolKind; 3] = [
    EntityToolKind::Mineral,
    EntityToolKind::Gas,
    EntityToolKind::Watchtower,
];

#[test]
fn without_symmetry_one_entity_is_added_with_the_next_id() {
    let doc = blank_document(16, 16);
    for kind in SINGLE_CELL_KINDS {
        let command = place_entity_command(&doc, kind, (3, 4), SymmetryMode::None);
        assert_eq!(
            command,
            EditCommand::AddEntities {
                entities: vec![expected_entity(kind, 1, (3, 4))],
            },
            "{kind:?}"
        );
    }
}

#[test]
fn under_symmetry_the_mirror_takes_the_following_id_and_the_mirrored_cell() {
    let doc = blank_document(16, 16);
    for (symmetry, mirrored) in [
        (SymmetryMode::MirrorX, (12, 4)),
        (SymmetryMode::MirrorY, (3, 11)),
        (SymmetryMode::Rotate180, (12, 11)),
    ] {
        for kind in SINGLE_CELL_KINDS {
            let command = place_entity_command(&doc, kind, (3, 4), symmetry);
            assert_eq!(
                command,
                EditCommand::AddEntities {
                    entities: vec![
                        expected_entity(kind, 1, (3, 4)),
                        expected_entity(kind, 2, mirrored),
                    ],
                },
                "{kind:?} under {symmetry:?}"
            );
        }
    }
}

#[test]
fn a_cell_on_the_symmetry_axis_is_placed_once() {
    // On a 17-wide map (odd, so it has a centre column), column 8 mirrors onto itself under
    // MirrorX.
    let doc = blank_document(17, 17);
    for kind in SINGLE_CELL_KINDS {
        let command = place_entity_command(&doc, kind, (8, 3), SymmetryMode::MirrorX);
        assert_eq!(
            command,
            EditCommand::AddEntities {
                entities: vec![expected_entity(kind, 1, (8, 3))],
            },
            "{kind:?}"
        );
    }
}
