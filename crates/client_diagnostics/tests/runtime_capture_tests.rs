//! Script contracts and startup rejection for the production capture harness.
//!
//! These tests validate configuration/state boundaries; only the separate actual
//! GPU run supplies screenshots, pipeline evidence and visual acceptance.

use client_diagnostics::runtime_capture::{CaptureError, CaptureScript, Goal};

fn script() -> Result<CaptureScript, CaptureError> {
    CaptureScript::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/captures/default_battle.ron"),
    )
}

#[test]
fn real_capture_script_has_all_required_lifecycle_goals() -> Result<(), Box<dyn std::error::Error>>
{
    let script = script()?;
    assert_eq!(script.checkpoints.len(), 7);
    assert_eq!(script.checkpoints[0].goal, Goal::Overview);
    assert_eq!(script.checkpoints[6].goal, Goal::Reset);
    Ok(())
}

#[test]
fn scripts_reject_unsafe_names_unbounded_waits_and_ineffective_controls(
) -> Result<(), Box<dyn std::error::Error>> {
    let original = script()?;
    let mut invalid = original.clone();
    invalid.format_version = 99;
    assert!(invalid.validate().is_err());
    let mut invalid = original.clone();
    invalid.checkpoints[0].name = "../old-image".into();
    assert!(invalid.validate().is_err());
    let mut invalid = original.clone();
    invalid.checkpoints[1].name = invalid.checkpoints[0].name.clone();
    assert!(invalid.validate().is_err());
    let mut invalid = original.clone();
    invalid.timeout_seconds = 0;
    assert!(invalid.validate().is_err());
    let mut invalid = original.clone();
    invalid.max_frames = 0;
    assert!(invalid.validate().is_err());
    let mut invalid = original.clone();
    invalid.checkpoints[1].distance = f32::NAN;
    assert!(invalid.validate().is_err());
    let mut invalid = original.clone();
    invalid.checkpoints[1].distance = 500.0;
    assert!(invalid.validate().is_err());
    let mut invalid = original.clone();
    invalid.gameplay_region = [0.8, 0.0, 0.5, 1.0];
    assert!(invalid.validate().is_err());
    let mut invalid = original.clone();
    invalid.checkpoints.swap(1, 2);
    assert!(invalid.validate().is_err());
    let mut invalid = original;
    invalid.overview_focus[0] = f32::INFINITY;
    assert!(invalid.validate().is_err());
    Ok(())
}

#[test]
fn headless_fixture_cannot_substitute_for_graphical_capture() {
    let result = client::build_graphical_app(
        client::ClientMode::HeadlessCheck,
        None,
        std::path::Path::new("unused.ron"),
    );
    assert!(matches!(
        result,
        Err(client::ClientStartupError::HeadlessMode)
    ));
}

#[test]
fn malformed_and_missing_script_errors_retain_the_actual_path(
) -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("capture.ron");
    assert!(
        matches!(CaptureScript::load(&path),Err(CaptureError::Io { path: actual,.. }) if actual==path)
    );
    std::fs::write(&path, "invalid capture RON")?;
    assert!(
        matches!(CaptureScript::load(&path),Err(CaptureError::Parse { path: actual,.. }) if actual==path)
    );
    Ok(())
}

#[test]
fn unsupported_authored_controls_are_never_silently_ignored(
) -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("strict.ron");
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/captures/default_battle.ron"),
    )?;
    for invalid in [
        source.replacen("format_version:", "fake_control: true, format_version:", 1),
        source.replacen("name:", "fake_control: true, name:", 1),
    ] {
        std::fs::write(&path, invalid)?;
        assert!(matches!(
            CaptureScript::load(&path),
            Err(CaptureError::Parse { .. })
        ));
    }
    Ok(())
}

#[test]
fn transport_capture_is_a_strict_fixed_camera_alternative() -> Result<(), Box<dyn std::error::Error>>
{
    use client_diagnostics::runtime_capture::{Checkpoint, TransportCaptureSpec};
    let mut capture = script()?;
    capture.transport = Some(TransportCaptureSpec {
        owner: 0,
        passenger_ordinal: 2,
        carrier_ordinal: 3,
        destination: [12, 10],
    });
    assert!(capture.validate().is_err());
    capture.checkpoints = [Goal::CargoBefore, Goal::CargoLoaded, Goal::CargoUnloaded]
        .into_iter()
        .enumerate()
        .map(|(index, goal)| Checkpoint {
            name: format!("cargo{index}"),
            goal,
            distance: 15.0,
            max_ticks: 1,
        })
        .collect();
    capture.validate()?;
    let mut invalid = capture.clone();
    invalid.transport = None;
    assert!(invalid.validate().is_err());
    let mut invalid = capture.clone();
    invalid.checkpoints[1].max_ticks = 2;
    assert!(invalid.validate().is_err());
    let mut invalid = capture.clone();
    invalid.checkpoints[2].distance = 16.0;
    assert!(invalid.validate().is_err());
    let mut invalid = capture.clone();
    invalid.checkpoints[1].goal = Goal::Combat;
    assert!(invalid.validate().is_err());
    let mut invalid = capture;
    if let Some(spec) = &mut invalid.transport {
        spec.carrier_ordinal = spec.passenger_ordinal;
    }
    assert!(invalid.validate().is_err());
    assert!(ron::from_str::<TransportCaptureSpec>(
        "(owner:0,passenger_ordinal:2,carrier_ordinal:3,destination:(14,14))"
    )
    .is_ok());
    assert!(ron::from_str::<TransportCaptureSpec>(
        "(owner:0,passenger_ordinal:2,carrier_ordinal:3,destination:(14,14),nearest_free:true)"
    )
    .is_err());
    Ok(())
}

#[test]
fn ownership_profile_requires_every_native_goal_and_rejects_transport_mix(
) -> Result<(), Box<dyn std::error::Error>> {
    use client_diagnostics::runtime_capture::TransportCaptureSpec;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/captures/player_ownership.ron");
    let original = CaptureScript::load(&path)?;
    assert!(original.ownership);
    assert_eq!(original.checkpoints.len(), 17);
    assert_eq!(original.checkpoints[6].goal, Goal::Reset);
    assert_eq!(original.checkpoints[16].goal, Goal::EffectShatter);
    for index in [13, 14] {
        let mut reframed = original.clone();
        reframed.checkpoints[index].distance += 1.0;
        assert!(reframed.validate().is_err());
        let mut missing = original.clone();
        missing.checkpoints.remove(index);
        assert!(missing.validate().is_err());
    }
    let mut swapped = original.clone();
    swapped.checkpoints.swap(13, 14);
    assert!(swapped.validate().is_err());
    let mut invalid = original.clone();
    invalid.ownership = false;
    assert!(invalid.validate().is_err());
    let mut invalid = original.clone();
    invalid.transport = Some(TransportCaptureSpec {
        owner: 0,
        passenger_ordinal: 0,
        carrier_ordinal: 1,
        destination: [10, 10],
    });
    assert!(invalid.validate().is_err());
    let mut invalid = original.clone();
    invalid.checkpoints.swap(10, 11);
    assert!(invalid.validate().is_err());
    let mut invalid = original.clone();
    invalid.checkpoints.pop();
    assert!(invalid.validate().is_err());
    let mut invalid = original.clone();
    invalid.checkpoints[7].max_ticks = 1;
    assert!(invalid.validate().is_err());
    let mut invalid = original;
    invalid.checkpoints[8].goal = Goal::CargoLoaded;
    assert!(invalid.validate().is_err());
    Ok(())
}
