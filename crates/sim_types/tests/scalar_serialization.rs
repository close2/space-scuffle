//! Persistence contracts for authoritative Q32.32 values and player commands.
//!
//! Exact encodings preserve every raw bit; legacy numeric input remains in simulation
//! units, while malformed or unrepresentable input must fail without panicking.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use sim_types::{EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, UnitOrder};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn assert_exact_round_trip(raw: i64) -> TestResult {
    let scalar = SimScalar::from_raw(raw);
    let text = ron::to_string(&scalar)?;
    assert_eq!(text, format!("\"q32.32:v1:{raw}\""));
    assert_eq!(ron::from_str::<SimScalar>(&text)?.raw(), raw);

    let binary = bincode::serialize(&scalar)?;
    assert_eq!(binary, raw.to_le_bytes());
    assert_eq!(bincode::deserialize::<SimScalar>(&binary)?.raw(), raw);
    Ok(())
}

#[test]
fn boundary_values_preserve_every_raw_bit() -> TestResult {
    for raw in [
        i64::MIN,
        i64::MIN + 1,
        i64::MAX - 1,
        i64::MAX,
        -1,
        0,
        1,
        -(1_i64 << 32),
        1_i64 << 32,
    ] {
        assert_exact_round_trip(raw)?;
    }
    for bit in 1..63 {
        let power = 1_i64 << bit;
        for raw in [power - 1, power, power + 1, -power - 1, -power, -power + 1] {
            assert_exact_round_trip(raw)?;
        }
    }
    Ok(())
}

#[test]
fn seeded_full_width_values_round_trip_exactly() -> TestResult {
    let mut rng = ChaCha8Rng::seed_from_u64(0x5153_3232_5345_5244);
    for _ in 0..10_000 {
        assert_exact_round_trip(rng.gen())?;
    }
    Ok(())
}

#[test]
fn authoritative_move_command_round_trips_nested_coordinates() -> TestResult {
    let command = PlayerCommand {
        player_id: PlayerId(0),
        entities: vec![EntityId(1)],
        order: UnitOrder::Move {
            target: SimVec2::new(
                SimScalar::from_raw(i64::MIN + 1),
                SimScalar::from_raw(i64::MAX),
            ),
        },
        queue: false,
    };
    assert_eq!(
        ron::from_str::<PlayerCommand>(&ron::to_string(&command)?)?,
        command
    );
    assert_eq!(
        bincode::deserialize::<PlayerCommand>(&bincode::serialize(&command)?)?,
        command
    );
    Ok(())
}

#[test]
fn legacy_numbers_retain_simulation_units() -> TestResult {
    for (input, expected) in [
        ("0", SimScalar::ZERO),
        ("1", SimScalar::ONE),
        ("-1", -SimScalar::ONE),
        ("0.5", SimScalar::HALF),
        ("-0.5", -SimScalar::HALF),
        ("0.1124", SimScalar::from_num(0.1124_f64)),
        ("2147483647", SimScalar::from_i32(i32::MAX)),
        ("-2147483648", SimScalar::MIN),
        ("(1.5,)", SimScalar::from_num(1.5_f64)),
        ("(\"q32.32:v1:1\",)", SimScalar::EPSILON),
    ] {
        assert_eq!(ron::from_str::<SimScalar>(input)?, expected, "{input}");
    }
    Ok(())
}

#[test]
fn malformed_and_out_of_range_input_is_rejected() {
    for input in [
        "2147483648",
        "-2147483649",
        "18446744073709551615",
        "1e99",
        "NaN",
        "inf",
        "-inf",
        "()",
        "(1.0, 2.0)",
        "\"q32.32:v1:9223372036854775808\"",
        "\"q32.32:v1:-9223372036854775809\"",
        "\"q32.32:v2:1\"",
        "\"q32.32:v1:\"",
        "\"q32.32:v1:1.0\"",
        "\"q32.32:v1: 1\"",
        "\"1\"",
    ] {
        assert!(ron::from_str::<SimScalar>(input).is_err(), "{input}");
    }
}
