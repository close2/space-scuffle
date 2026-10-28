//! Fresh screenshot acknowledgments that survive ownership transfer into Bevy's runner.
//!
//! Native window closure can return a successful AppExit before all readbacks finish.
//! A per-image atomic mask distinguishes actual completed saves from that runner status.

use bevy::prelude::*;
use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc,
};

const EXPECTED_IMAGES: usize = 3;
const COMPLETE_MASK: u8 = 0b111;

/// Capture verification failure after the real renderer or screenshot callback returns.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CaptureVerificationError {
    /// The runner exited successfully before all fresh screenshot saves completed.
    #[error("capture incomplete: saved {completed} of {expected} required images")]
    Incomplete {
        /// Number of distinct successful callbacks in this run.
        completed: u32,
        /// Number required by the capture sequence.
        expected: usize,
    },
    /// A callback attempted to acknowledge an image outside its sequence.
    #[error("invalid capture acknowledgment index {0}")]
    InvalidIndex(usize),
}

#[derive(Resource, Clone, Default)]
pub(super) struct CaptureCompletion(Arc<AtomicU8>);

impl CaptureCompletion {
    /// Called only after the complete PNG/CSV/metrics save transaction succeeds.
    pub(super) fn acknowledge(&self, image: usize) -> Result<(), CaptureVerificationError> {
        if image >= EXPECTED_IMAGES {
            return Err(CaptureVerificationError::InvalidIndex(image));
        }
        self.0.fetch_or(1 << image, Ordering::Release);
        Ok(())
    }

    /// Preserves existing runner errors and rejects otherwise-successful incomplete runs.
    pub(super) fn finish(&self, exit: AppExit) -> Result<AppExit, CaptureVerificationError> {
        let mask = self.0.load(Ordering::Acquire);
        if matches!(exit, AppExit::Success) && mask != COMPLETE_MASK {
            return Err(CaptureVerificationError::Incomplete {
                completed: mask.count_ones(),
                expected: EXPECTED_IMAGES,
            });
        }
        Ok(exit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_success_requires_every_distinct_callback() -> Result<(), CaptureVerificationError> {
        let completion = CaptureCompletion::default();
        for count in 0..3 {
            assert_eq!(
                completion.finish(AppExit::Success),
                Err(CaptureVerificationError::Incomplete {
                    completed: count,
                    expected: 3
                })
            );
            completion.acknowledge(count as usize)?;
        }
        assert_eq!(completion.finish(AppExit::Success), Ok(AppExit::Success));
        Ok(())
    }

    #[test]
    fn duplicate_or_invalid_callbacks_cannot_fake_completion(
    ) -> Result<(), CaptureVerificationError> {
        let completion = CaptureCompletion::default();
        for _ in 0..3 {
            completion.acknowledge(1)?;
        }
        assert_eq!(
            completion.finish(AppExit::Success),
            Err(CaptureVerificationError::Incomplete {
                completed: 1,
                expected: 3
            })
        );
        assert_eq!(
            completion.acknowledge(3),
            Err(CaptureVerificationError::InvalidIndex(3))
        );
        Ok(())
    }

    #[test]
    fn runner_errors_are_preserved_even_when_captures_are_incomplete() {
        let completion = CaptureCompletion::default();
        assert_eq!(completion.finish(AppExit::error()), Ok(AppExit::error()));
    }
}
