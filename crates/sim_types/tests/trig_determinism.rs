//! Integration tests verifying trigonometric hash checksum determinism.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use sim_types::SimAngle;
use xxhash_rust::xxh64::Xxh64;

#[test]
fn test_trig_lookup_determinism_checksum() {
    // EXPECTED_TRIG_CHECKSUM anchors the exact bit-level sine table output across all targets.
    // If table values or quadrant interpolation change, this checksum will immediately fail.
    const EXPECTED_TRIG_CHECKSUM: u64 = 0x2F84_9264_F337_8129;

    let mut hasher = Xxh64::new(0);
    for a in 0..=65535u16 {
        let raw = SimAngle(a).sin().raw();
        hasher.update(&raw.to_le_bytes());
    }
    let checksum = hasher.digest();

    assert_eq!(
        checksum, EXPECTED_TRIG_CHECKSUM,
        "trig determinism checksum mismatch: got {checksum:#018X}, expected {EXPECTED_TRIG_CHECKSUM:#018X}"
    );
}
