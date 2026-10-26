//! # Integration Test: High-Ground Vision & Combat Occlusion
//!
//! Verifies that low-ground units cannot auto-acquire or fire at high-ground enemies
//! without allied line-of-sight (e.g. from an aerial spotter like the SkyScout).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::too_many_lines
)]

use sim_core::{SimWorld, WeaponPhase};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{PlayerId, SimScalar, SimVec2, Tick, TickBundle, VisibilityState};

#[test]
fn test_high_ground_combat_occlusion_and_spotter_reveal() {
    let width = 32;
    let height = 32;

    // Construct map with Cliff Level 1 on left (x < 12) and Cliff Level 2 on right (x >= 12)
    let mut cells = vec![MapCell::default_ground(CliffLevel(1)); (width * height) as usize];
    for y in 0..height {
        for x in 12..width {
            let idx = (y * width + x) as usize;
            cells[idx] = MapCell::default_ground(CliffLevel(2));
        }
    }
    let map = GameMap::from_cells(width, height, cells, Vec::new(), Vec::new())
        .expect("invariant: valid map");

    let mut world = SimWorld::new(map, 0xC001_D00D);

    let player_blue = PlayerId(0);
    let player_red = PlayerId(1);

    let pulse_bot_id = world
        .catalog
        .id_by_slug("pulse_bot")
        .expect("pulse_bot in catalog");
    let psi_grey_id = world
        .catalog
        .id_by_slug("psi_grey")
        .expect("psi_grey in catalog");

    // Marine on Low Ground (Cliff Level 1) at (10.0, 10.0)
    let marine_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10));
    let marine_id = world
        .spawn_unit(pulse_bot_id, player_blue, marine_pos)
        .expect("spawn marine");

    // Enemy Zealot on High Ground (Cliff Level 2) at (12.0, 10.0) (within Marine's 5.0 range!)
    let zealot_pos = SimVec2::new(SimScalar::from_num(12), SimScalar::from_num(10));
    let zealot_id = world
        .spawn_unit(psi_grey_id, player_red, zealot_pos)
        .expect("spawn zealot");

    let empty_bundle = |tick: u32| TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    };

    // Step 5 ticks with only low-ground Marine
    for tick in 0..5 {
        world.step(&empty_bundle(tick));
    }

    // 1. High ground tile at (12, 10) must NOT be visible to Player Blue
    assert_ne!(
        world.vision_maps[0].visibility_at(12, 10),
        VisibilityState::Visible,
        "High ground tile must be occluded by cliff ledge from low ground"
    );

    // 2. Marine must not acquire or fire at the Zealot
    {
        let marine = world.entities.get(&marine_id).expect("marine exists");
        assert_eq!(
            marine.weapon_phase,
            WeaponPhase::Ready,
            "Marine must not begin attack cycle without vision"
        );
        let zealot = world.entities.get(&zealot_id).expect("zealot exists");
        assert_eq!(
            zealot.shields,
            SimScalar::from_num(50),
            "Zealot must take zero damage when hidden in high-ground fog"
        );
    }

    // 3. Spawn an aerial spotter: Flying Saucer (MoverType::Air) for Player Blue at (10.0, 10.0)
    let saucer_id = world
        .catalog
        .id_by_slug("flying_saucer")
        .expect("flying_saucer in catalog");
    let scout_id = world
        .spawn_unit(saucer_id, player_blue, marine_pos)
        .expect("spawn Flying Saucer spotter");

    // Step tick 6: SkyScout provides vision over the cliff ledge
    world.step(&empty_bundle(6));

    // 4. High ground tile at (12, 10) is now Visible!
    assert_eq!(
        world.vision_maps[0].visibility_at(12, 10),
        VisibilityState::Visible,
        "High ground tile must be revealed by aerial spotter"
    );

    // 5. Low-ground Marine immediately acquires Zealot and engages!
    let marine = world.entities.get(&marine_id).expect("marine exists");
    assert!(
        matches!(
            marine.weapon_phase,
            WeaponPhase::RotatingToTarget { .. } | WeaponPhase::Windup { .. }
        ),
        "Marine must acquire target once revealed by spotter, got {:?}",
        marine.weapon_phase
    );

    let _ = scout_id;
}
