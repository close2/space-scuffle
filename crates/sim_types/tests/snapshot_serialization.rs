//! Integration tests verifying `RenderSnapshot` serialization fidelity and determinism.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]

use sim_types::{
    AbilitySource, AnimState, AreaView, AttackSwing, BubbleRenderInstance, CombatEventVisual,
    Concealment, EffectSource, EntityId, LineView, LinkView, ModeTransitionView, PlayerId,
    PlayerView, RenderSnapshot, ShapeKind, SilhouetteClass, StanceView, StatusFlags, Tick,
    UnitTypeId, WeaponKey, WeaponSlot, WeaponVisualKind,
};

/// A weapon source with every field away from its default.
const SOURCE: EffectSource = EffectSource {
    unit_type: UnitTypeId(38),
    weapon: WeaponKey {
        slot: WeaponSlot::Air,
        mode: Some(2),
    },
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
            owner: PlayerId((i % 2 + 1) as u8),
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
            attack_swing: (i % 2 == 0).then_some(AttackSwing {
                elapsed_ticks: i as u16,
                damage_point_ticks: 2,
                period_ticks: 15,
            }),
            hp_ratio: 1.0 - (i as f32) * 0.08,
            shield_ratio: if i % 2 == 0 { 0.8 } else { 0.0 },
            shield_hit_flash: if i == 2 { 0.75 } else { 0.0 },
            hull_hit_angle_rad: 1.25,
            hull_hit_flash: if i == 5 { 0.5 } else { 0.0 },
            build_or_queue_progress: None,
            selected: i < 3,
            concealment: match i {
                7 => Concealment::Cloaked,
                8 => Concealment::Burrowed,
                _ => Concealment::None,
            },
            speed_boosted: i == 1,
            stance: StanceView {
                mode: (i % 3 == 0).then_some(0),
                transition: (i == 4).then_some(ModeTransitionView {
                    entering: Some(1),
                    remaining_ticks: 20,
                    nominal_ticks: 33,
                }),
            },
            status: StatusFlags {
                overclock: i == 1,
                dash: i == 2,
                slowed: i == 3,
                barrier_up: i == 5,
                barrier_hit: i == 5,
                damage_over_time: i == 6,
                stunned: i == 7,
                range_reduced: i == 8,
                revealed: i == 9,
            },
            barrier_ratio: if i == 5 { 0.4 } else { 0.0 },
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
        owner: PlayerId(1),
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
        attack_swing: None,
        hp_ratio: 1.0,
        shield_ratio: 1.0,
        shield_hit_flash: 0.0,
        hull_hit_angle_rad: 0.0,
        hull_hit_flash: 0.0,
        build_or_queue_progress: Some(0.65), // Under construction
        selected: false,
        concealment: Concealment::None,
        speed_boosted: false,
        stance: StanceView::default(),
        status: StatusFlags::default(),
        barrier_ratio: 0.0,
    });

    bubbles.push(BubbleRenderInstance {
        entity_id: EntityId(102),
        shape: ShapeKind::BuildingDome {
            footprint_w: 2,
            footprint_h: 2,
        },
        silhouette: SilhouetteClass::StructureBase,
        unit_type: UnitTypeId(51),
        owner: PlayerId(2),
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
        attack_swing: None,
        hp_ratio: 0.45,
        shield_ratio: 0.0,
        shield_hit_flash: 0.0,
        hull_hit_angle_rad: 0.8,
        hull_hit_flash: 0.3,
        build_or_queue_progress: None,
        selected: false,
        concealment: Concealment::None,
        speed_boosted: false,
        stance: StanceView::default(),
        status: StatusFlags::default(),
        barrier_ratio: 0.0,
    });

    // 5 combat visual events
    let combat_events = vec![
        CombatEventVisual::HitscanTracer {
            from: [10.0, 15.0, 0.5],
            to: [25.0, 30.0, 0.5],
            owner: PlayerId(1),
            source: SOURCE,
        },
        CombatEventVisual::ProjectileVisual {
            pos: [18.0, 22.0, 0.6],
            vel: [5.0, 5.0, 0.0],
            owner: PlayerId(2),
            duration_secs: Some(0.25),
            source: SOURCE,
        },
        CombatEventVisual::BeamVisual {
            from: [12.5, 13.5, 0.7],
            to: [25.0, 30.0, 0.5],
            is_lightning: false,
            owner: PlayerId(1),
            source: SOURCE,
        },
        CombatEventVisual::MeleeSlashVisual {
            pos: [24.0, 29.0, 0.3],
            angle_rad: 2.14,
            radius: 1.2,
            owner: PlayerId(1),
            source: SOURCE,
        },
        CombatEventVisual::BubbleShatter {
            pos: [25.0, 30.0, 0.0],
            radius: 0.65,
            is_dome: false,
            owner: PlayerId(2),
            unit_type: UnitTypeId(4),
        },
        CombatEventVisual::SplashVisual {
            center: [24.0, 29.0, 0.0],
            zones: [0.4687, 0.7812, 1.25],
            line: None,
            source: SOURCE,
            owner: PlayerId(1),
        },
        CombatEventVisual::SplashVisual {
            center: [12.0, 10.0, 0.0],
            zones: [0.5, 0.5, 0.5],
            line: Some(LineView {
                from: [10.0, 10.0, 0.0],
                to: [19.0, 10.0, 0.0],
                spacing: 1.0,
                index: 1,
                count: 9,
            }),
            source: SOURCE,
            owner: PlayerId(2),
        },
    ];

    let original = RenderSnapshot {
        tick: Tick(1250),
        checksum: 0xABCD_EF01_2345_6789,
        bubbles,
        combat_events,
        areas: vec![AreaView {
            id: 3,
            owner: PlayerId(1),
            center: [20.0, 20.0, 0.0],
            radius: 1.5,
            remaining_ticks: 40,
            total_ticks: 71,
            source: Some(AbilitySource {
                unit_type: UnitTypeId(12),
                ability: 1,
            }),
        }],
        links: vec![LinkView {
            from: EntityId(1),
            to: EntityId(2),
            owner: PlayerId(2),
            source: AbilitySource {
                unit_type: UnitTypeId(13),
                ability: 0,
            },
        }],
        ..RenderSnapshot::default()
    };

    for owner in [PlayerId(0), PlayerId(2), PlayerId(7), PlayerId::NEUTRAL] {
        let mut bubble = original.bubbles[0].clone();
        bubble.owner = owner;
        let text = ron::to_string(&bubble).expect("invariant: valid bubble serialization");
        let decoded: BubbleRenderInstance =
            ron::from_str(&text).expect("invariant: freshly serialized bubble is valid");
        assert_eq!(decoded.owner, owner);
        assert!(
            ron::from_str::<BubbleRenderInstance>(&text.replace("owner:", "team_id:")).is_err()
        );
        assert!(
            ron::from_str::<BubbleRenderInstance>(&text.replace("owner:", "team_id:0,owner:"))
                .is_err()
        );
    }

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

#[test]
fn every_event_preserves_sparse_and_neutral_owner_and_rejects_legacy_team_keys(
) -> Result<(), Box<dyn std::error::Error>> {
    for owner in [PlayerId(0), PlayerId(2), PlayerId(7), PlayerId::NEUTRAL] {
        let events = [
            CombatEventVisual::HitscanTracer {
                from: [0.0; 3],
                to: [1.0; 3],
                owner,
                source: SOURCE,
            },
            CombatEventVisual::ProjectileVisual {
                pos: [0.0; 3],
                vel: [1.0; 3],
                owner,
                duration_secs: Some(0.5),
                source: SOURCE,
            },
            CombatEventVisual::BeamVisual {
                from: [0.0; 3],
                to: [1.0; 3],
                is_lightning: false,
                owner,
                source: SOURCE,
            },
            CombatEventVisual::MeleeSlashVisual {
                pos: [0.0; 3],
                angle_rad: 0.0,
                radius: 1.0,
                owner,
                source: SOURCE,
            },
            CombatEventVisual::BubbleShatter {
                pos: [0.0; 3],
                radius: 1.0,
                is_dome: false,
                owner,
                unit_type: UnitTypeId(4),
            },
            CombatEventVisual::SplashVisual {
                center: [0.0; 3],
                zones: [0.5, 1.0, 1.5],
                line: None,
                source: SOURCE,
                owner,
            },
        ];
        for event in events {
            let text = ron::to_string(&event)?;
            let decoded: CombatEventVisual = ron::from_str(&text)?;
            assert_eq!(decoded.owner(), owner);
            assert_eq!(decoded, event);
            let legacy = text.replace("owner:", "team_id:");
            assert!(ron::from_str::<CombatEventVisual>(&legacy).is_err());
            let ambiguous = text.replace("owner:", "team_id:0,owner:");
            assert!(ron::from_str::<CombatEventVisual>(&ambiguous).is_err());
        }
    }
    Ok(())
}

/// The attack cycle travels as three integers, and the float `anim_progress` lane it replaced
/// (plan `2026-11-04-battle-flipbook-animation` D-7) is no longer accepted.
#[test]
fn attack_swing_round_trips_and_the_float_progress_lane_is_gone() {
    let swing = AttackSwing {
        elapsed_ticks: 3,
        damage_point_ticks: 2,
        period_ticks: u16::MAX,
    };
    let text = ron::to_string(&swing).expect("invariant: valid swing serialization");
    assert_eq!(
        text,
        "(elapsed_ticks:3,damage_point_ticks:2,period_ticks:65535)"
    );
    let decoded: AttackSwing = ron::from_str(&text).expect("invariant: freshly serialized swing");
    assert_eq!(decoded, swing);
    assert!(
        ron::from_str::<AttackSwing>(
            "(elapsed_ticks:3,damage_point_ticks:2,period_ticks:15,phase:0.5)"
        )
        .is_err(),
        "an unknown swing field is rejected"
    );

    let mut bubble = BubbleRenderInstance {
        entity_id: EntityId(1),
        shape: ShapeKind::UnitSphere,
        silhouette: SilhouetteClass::LightProngs,
        unit_type: UnitTypeId(1),
        owner: PlayerId(0),
        world_pos: [0.0; 3],
        ground_z: 0.0,
        radius: 0.5,
        velocity: [0.0; 2],
        heading_angle_rad: 0.0,
        aim_angle_rad: 0.0,
        is_attacking: true,
        weapon_visual: WeaponVisualKind::Hitscan,
        attack_target_pos: None,
        anim_state: AnimState::Attack,
        attack_swing: Some(swing),
        hp_ratio: 1.0,
        shield_ratio: 0.0,
        shield_hit_flash: 0.0,
        hull_hit_angle_rad: 0.0,
        hull_hit_flash: 0.0,
        build_or_queue_progress: None,
        selected: false,
        concealment: Concealment::None,
        speed_boosted: false,
        stance: StanceView::default(),
        status: StatusFlags::default(),
        barrier_ratio: 0.0,
    };
    for attack_swing in [Some(swing), None] {
        bubble.attack_swing = attack_swing;
        let text = ron::to_string(&bubble).expect("invariant: valid bubble serialization");
        let decoded: BubbleRenderInstance =
            ron::from_str(&text).expect("invariant: freshly serialized bubble is valid");
        assert_eq!(decoded, bubble);
        let legacy = text.replace("attack_swing:", "anim_progress:0.0,attack_swing:");
        assert!(
            ron::from_str::<BubbleRenderInstance>(&legacy).is_err(),
            "a record carrying the removed anim_progress lane is rejected"
        );
    }
}

/// A bubble with the fields of plan `2026-11-09-effect-visualization` WP1 at their defaults.
fn plain_bubble() -> BubbleRenderInstance {
    BubbleRenderInstance {
        entity_id: EntityId(5),
        shape: ShapeKind::UnitSphere,
        silhouette: SilhouetteClass::HeavyHex,
        unit_type: UnitTypeId(38),
        owner: PlayerId(1),
        world_pos: [1.0, 2.0, 0.0],
        ground_z: 0.0,
        radius: 0.9,
        velocity: [0.0; 2],
        heading_angle_rad: 0.0,
        aim_angle_rad: 0.0,
        is_attacking: false,
        weapon_visual: WeaponVisualKind::MeleeSlash,
        attack_target_pos: None,
        anim_state: AnimState::Idle,
        attack_swing: None,
        hp_ratio: 1.0,
        shield_ratio: 0.0,
        shield_hit_flash: 0.0,
        hull_hit_angle_rad: 0.0,
        hull_hit_flash: 0.0,
        build_or_queue_progress: None,
        selected: false,
        concealment: Concealment::None,
        speed_boosted: false,
        stance: StanceView::default(),
        status: StatusFlags::default(),
        barrier_ratio: 0.0,
    }
}

/// A record written before the effect fields existed reads with their defaults, and its boolean
/// `cloaked` key reads as the concealment it meant.
#[test]
fn a_bubble_written_before_the_effect_fields_still_reads() {
    let text = ron::to_string(&plain_bubble()).expect("invariant: valid bubble serialization");
    let old = text
        .replace(",stance:(mode:None,transition:None)", "")
        .replace(
            ",status:(overclock:false,dash:false,slowed:false,barrier_up:false,barrier_hit:false)",
            "",
        )
        .replace(",barrier_ratio:0.0", "");
    assert!(
        !old.contains("stance") && !old.contains("status") && !old.contains("barrier_ratio"),
        "every new field was removed from {old}"
    );
    for (flag, concealment) in [("false", Concealment::None), ("true", Concealment::Cloaked)] {
        let legacy = old.replace("concealment:None", &format!("cloaked:{flag}"));
        assert!(legacy.contains("cloaked:"), "{legacy}");
        let decoded: BubbleRenderInstance =
            ron::from_str(&legacy).expect("invariant: an old record still reads");
        assert_eq!(
            decoded,
            BubbleRenderInstance {
                concealment,
                ..plain_bubble()
            }
        );
    }
    assert!(
        ron::from_str::<BubbleRenderInstance>(
            &old.replace("concealment:None", "concealment:None,cloaked:false")
        )
        .is_err(),
        "a record cannot carry both keys"
    );
}

/// Events written before they carried a source read with the default source, and an old
/// snapshot without areas and links reads with empty lists.
#[test]
fn events_and_snapshots_written_before_sources_still_read() {
    let tracer: CombatEventVisual =
        ron::from_str("HitscanTracer(from:(0.0,0.0,0.0),to:(1.0,1.0,0.0),owner:(2))")
            .expect("invariant: an old tracer still reads");
    assert_eq!(tracer.source(), Some(EffectSource::default()));
    let shatter: CombatEventVisual =
        ron::from_str("BubbleShatter(pos:(1.0,2.0,0.0),radius:0.5,is_dome:false,owner:(1))")
            .expect("invariant: an old shatter still reads");
    assert_eq!(
        shatter,
        CombatEventVisual::BubbleShatter {
            pos: [1.0, 2.0, 0.0],
            radius: 0.5,
            is_dome: false,
            owner: PlayerId(1),
            unit_type: UnitTypeId::default(),
        }
    );

    let snapshot = RenderSnapshot {
        tick: Tick(9),
        bubbles: vec![plain_bubble()],
        ..RenderSnapshot::default()
    };
    let text = ron::to_string(&snapshot).expect("invariant: valid snapshot serialization");
    let old = text.replace(",areas:[],links:[]", "");
    assert_ne!(old, text, "the area and link lists were removed");
    let decoded: RenderSnapshot = ron::from_str(&old).expect("invariant: an old snapshot reads");
    assert_eq!(decoded, snapshot);
}

/// A splash is judged for visibility at its search centre, whatever line it belongs to.
#[test]
fn a_splash_is_anchored_at_its_centre() {
    let event = CombatEventVisual::SplashVisual {
        center: [7.5, 3.25, 2.5],
        zones: [0.5, 0.5, 0.5],
        line: Some(LineView {
            from: [0.0, 0.0, 0.0],
            to: [40.0, 0.0, 0.0],
            spacing: 5.0,
            index: 1,
            count: 8,
        }),
        source: SOURCE,
        owner: PlayerId(3),
    };
    assert_eq!(event.anchor(), [7.5, 3.25]);
}

/// Clearing a view empties its revealed list along with the rest.
#[test]
fn clearing_a_view_empties_revealed() {
    let mut view = PlayerView {
        revealed: vec![EntityId(4), EntityId(9)],
        ..PlayerView::default()
    };
    let capacity = view.revealed.capacity();
    view.clear();
    assert_eq!(view.revealed, Vec::<EntityId>::new());
    assert_eq!(view.revealed.capacity(), capacity, "the buffer is kept");
    assert_eq!(view, PlayerView::default());
}
