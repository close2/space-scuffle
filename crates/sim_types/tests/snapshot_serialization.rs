//! Integration tests verifying `RenderSnapshot` serialization fidelity and determinism.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]

use sim_types::{
    AnimState, BubbleRenderInstance, CombatEventVisual, EntityId, RenderSnapshot, ShapeKind,
    SilhouetteClass, TeamId, Tick, UnitTypeId, WeaponVisualKind,
};

#[test]
fn test_render_snapshot_full_serialization() {
    let mut bubbles = Vec::new();

    // 10 unit bubbles with diverse properties
    for i in 0..10 {
        let silhouette = match i % 5 {
            0 => SilhouetteClass::LightProngs,
            1 => SilhouetteClass::HeavyHex,
            2 => SilhouetteClass::CasterHalo,
            3 => SilhouetteClass::MeleeSpikes,
            _ => SilhouetteClass::StructureBase,
        };

        let anim_state = match i % 4 {
            0 => AnimState::Idle,
            1 => AnimState::Walk,
            2 => AnimState::Attack,
            _ => AnimState::Cast,
        };

        let weapon_visual = match i % 4 {
            0 => WeaponVisualKind::Hitscan,
            1 => WeaponVisualKind::Projectile,
            2 => WeaponVisualKind::LaserBeam,
            _ => WeaponVisualKind::None,
        };

        bubbles.push(BubbleRenderInstance {
            entity_id: EntityId(i + 1),
            shape: ShapeKind::UnitSphere,
            silhouette,
            unit_type: UnitTypeId((i % 3 + 1) as u16),
            team_id: TeamId((i % 2 + 1) as u8),
            world_pos: [10.0 + (i as f32) * 2.5, 15.0 - (i as f32) * 1.5, 0.0],
            ground_z: 0.0,
            radius: 0.65,
            velocity: [0.1 * (i as f32), -0.05 * (i as f32)],
            heading_angle_rad: (i as f32) * 0.628,
            aim_angle_rad: (i as f32) * 0.628 + 0.1,
            is_attacking: i % 2 == 0,
            weapon_visual,
            attack_target_pos: if i % 2 == 0 {
                Some([25.0, 30.0, 0.0])
            } else {
                None
            },
            anim_state,
            anim_progress: (i as f32) * 0.1,
            hp_ratio: 1.0 - (i as f32) * 0.08,
            shield_ratio: if i % 2 == 0 { 0.8 } else { 0.0 },
            shield_hit_flash: if i == 2 { 0.75 } else { 0.0 },
            hull_hit_angle_rad: 1.25,
            hull_hit_flash: if i == 5 { 0.5 } else { 0.0 },
            build_or_queue_progress: None,
            selected: i < 3,
            cloaked: i == 7,
            speed_boosted: i == 1,
        });
    }

    // 2 building domes with footprint and construction progress
    bubbles.push(BubbleRenderInstance {
        entity_id: EntityId(101),
        shape: ShapeKind::BuildingDome {
            footprint_w: 3,
            footprint_h: 3,
        },
        silhouette: SilhouetteClass::StructureBase,
        unit_type: UnitTypeId(50),
        team_id: TeamId(1),
        world_pos: [5.0, 5.0, 0.0],
        ground_z: 0.0,
        radius: 2.1,
        velocity: [0.0, 0.0],
        heading_angle_rad: 0.0,
        aim_angle_rad: 0.0,
        is_attacking: false,
        weapon_visual: WeaponVisualKind::None,
        attack_target_pos: None,
        anim_state: AnimState::Idle,
        anim_progress: 0.0,
        hp_ratio: 1.0,
        shield_ratio: 1.0,
        shield_hit_flash: 0.0,
        hull_hit_angle_rad: 0.0,
        hull_hit_flash: 0.0,
        build_or_queue_progress: Some(0.65), // Under construction
        selected: false,
        cloaked: false,
        speed_boosted: false,
    });

    bubbles.push(BubbleRenderInstance {
        entity_id: EntityId(102),
        shape: ShapeKind::BuildingDome {
            footprint_w: 2,
            footprint_h: 2,
        },
        silhouette: SilhouetteClass::StructureBase,
        unit_type: UnitTypeId(51),
        team_id: TeamId(2),
        world_pos: [40.0, 40.0, 0.0],
        ground_z: 0.0,
        radius: 1.5,
        velocity: [0.0, 0.0],
        heading_angle_rad: 0.0,
        aim_angle_rad: 0.0,
        is_attacking: false,
        weapon_visual: WeaponVisualKind::None,
        attack_target_pos: None,
        anim_state: AnimState::Idle,
        anim_progress: 0.0,
        hp_ratio: 0.45,
        shield_ratio: 0.0,
        shield_hit_flash: 0.0,
        hull_hit_angle_rad: 0.8,
        hull_hit_flash: 0.3,
        build_or_queue_progress: None,
        selected: false,
        cloaked: false,
        speed_boosted: false,
    });

    // 5 combat visual events
    let combat_events = vec![
        CombatEventVisual::HitscanTracer {
            from: [10.0, 15.0, 0.5],
            to: [25.0, 30.0, 0.5],
            team_id: TeamId(1),
        },
        CombatEventVisual::ProjectileVisual {
            pos: [18.0, 22.0, 0.6],
            vel: [5.0, 5.0, 0.0],
            team_id: TeamId(2),
            duration_secs: Some(0.25),
        },
        CombatEventVisual::BeamVisual {
            from: [12.5, 13.5, 0.7],
            to: [25.0, 30.0, 0.5],
            is_lightning: false,
            team_id: TeamId(1),
        },
        CombatEventVisual::MeleeSlashVisual {
            pos: [24.0, 29.0, 0.3],
            angle_rad: 2.14,
            radius: 1.2,
            team_id: TeamId(1),
        },
        CombatEventVisual::BubbleShatter {
            pos: [25.0, 30.0, 0.0],
            radius: 0.65,
            is_dome: false,
            team_id: TeamId(2),
        },
    ];

    let original = RenderSnapshot {
        tick: Tick(1250),
        checksum: 0xABCD_EF01_2345_6789,
        bubbles,
        combat_events,
    };

    // Serialize to RON string
    let ron_serialized = ron::to_string(&original).expect("invariant: valid serialization to RON");

    // Deserialize back
    let deserialized: RenderSnapshot =
        ron::from_str(&ron_serialized).expect("invariant: valid deserialization from RON");

    // Verify deep structural equality
    assert_eq!(
        original, deserialized,
        "Snapshot did not survive RON serialization roundtrip"
    );
}
