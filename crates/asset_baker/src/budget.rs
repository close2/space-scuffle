//! Spending caps for paid Veo generations (plan `2026-11-04-phase-8-5-veo-media.md`, D-4).
//!
//! # Responsibility
//! Loads the non-secret limits in `assets/media/veo_budget.ron` and decides, before any request,
//! whether a run may start: the run must fit its own `--max-generations`, that number must not
//! exceed the per-run cap in data, and the generations the ledger records in the rolling window
//! plus this run's must not exceed the weekly budget.
//!
//! # How it fits
//! `generate-veo` calls [`check_spend`] after the plan and prompt validation and before it reads
//! any credential. The run itself never starts more than `--max-generations` requests, retries
//! included (`veo_run`).
//!
//! # Invariants
//! - A refusal names the numbers that caused it; nothing is spent when it is returned.
//! - Limits are data: changing them needs no rebuild (charter §3.3).

use crate::ledger::{LedgerHistory, SECONDS_PER_DAY};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Path of the budget file inside an asset bundle.
pub const BUDGET_FILE: &str = "media/veo_budget.ron";

/// Failures to load the limits, and refusals to spend.
#[derive(Debug, thiserror::Error)]
pub enum BudgetError {
    /// The budget file could not be read.
    #[error("cannot read the Veo budget '{path}': {source}")]
    Io {
        /// The budget file.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The budget file is not valid.
    #[error("invalid Veo budget '{path}': {reason}")]
    Invalid {
        /// The budget file.
        path: PathBuf,
        /// What is wrong.
        reason: String,
    },
    /// A paid run was asked for without `--max-generations`.
    #[error("a paid run needs --max-generations <N>: the most generations this run may start, retries included")]
    MaxGenerationsRequired,
    /// `--max-generations` exceeds the per-run cap in data.
    #[error("--max-generations {requested} is above the per-run cap of {cap} in {BUDGET_FILE}")]
    AboveRunCap {
        /// The value given.
        requested: u32,
        /// The cap in data.
        cap: u32,
    },
    /// The run needs more generations than it was allowed.
    #[error(
        "this run needs {needed} generations but --max-generations is {allowed}; nothing was spent"
    )]
    RunNeedsMore {
        /// Generations the plan would start.
        needed: u32,
        /// The `--max-generations` value.
        allowed: u32,
    },
    /// The rolling window's budget would be exceeded.
    #[error("the rolling {days}-day budget has used {used} of {budget} generations; this run needs {needed}. Nothing was spent; wait for older generations to leave the window")]
    WindowExceeded {
        /// Window length in days.
        days: u32,
        /// Generations already started in the window.
        used: u32,
        /// The budget for the window.
        budget: u32,
        /// Generations the run would start.
        needed: u32,
    },
}

/// The limits in `veo_budget.ron`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetConfig {
    /// Largest `--max-generations` one invocation accepts.
    pub max_generations_per_run: u32,
    /// Generations allowed in any rolling window of `window_days`.
    pub weekly_generations: u32,
    /// Length of the rolling window, in days.
    pub window_days: u32,
}

impl BudgetConfig {
    /// Loads and checks the budget file at `path`.
    ///
    /// # Errors
    /// [`BudgetError::Io`] or [`BudgetError::Invalid`] (unparsable, or a zero window).
    pub fn load(path: &Path) -> Result<Self, BudgetError> {
        let text = std::fs::read_to_string(path).map_err(|source| BudgetError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let config: Self = ron::from_str(&text).map_err(|error| BudgetError::Invalid {
            path: path.to_path_buf(),
            reason: error.to_string(),
        })?;
        if config.window_days == 0 {
            return Err(BudgetError::Invalid {
                path: path.to_path_buf(),
                reason: "window_days must be at least 1".to_string(),
            });
        }
        Ok(config)
    }

    /// The window length in seconds.
    #[must_use]
    pub fn window_seconds(&self) -> u64 {
        u64::from(self.window_days) * SECONDS_PER_DAY
    }

    /// Generations `history` records in the window ending at `now_unix_seconds`.
    #[must_use]
    pub fn used_in_window(&self, history: &LedgerHistory, now_unix_seconds: u64) -> u32 {
        history.generations_since(now_unix_seconds.saturating_sub(self.window_seconds()))
    }
}

/// One run's request to spend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpendRequest {
    /// Generations the plan would start.
    pub needed: u32,
    /// The run's `--max-generations`, if given.
    pub max_generations: Option<u32>,
    /// The window budget for this run (`--weekly-budget`, else the data value).
    pub window_budget: u32,
    /// Generations already started in the window.
    pub used_in_window: u32,
}

/// Decides whether a run may start. See the module docs for the rules.
///
/// # Errors
/// The first [`BudgetError`] refusal that applies.
pub fn check_spend(config: &BudgetConfig, request: SpendRequest) -> Result<(), BudgetError> {
    let allowed = request
        .max_generations
        .ok_or(BudgetError::MaxGenerationsRequired)?;
    if allowed > config.max_generations_per_run {
        return Err(BudgetError::AboveRunCap {
            requested: allowed,
            cap: config.max_generations_per_run,
        });
    }
    if request.needed > allowed {
        return Err(BudgetError::RunNeedsMore {
            needed: request.needed,
            allowed,
        });
    }
    if request.used_in_window.saturating_add(request.needed) > request.window_budget {
        return Err(BudgetError::WindowExceeded {
            days: config.window_days,
            used: request.used_in_window,
            budget: request.window_budget,
            needed: request.needed,
        });
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    const CONFIG: BudgetConfig = BudgetConfig {
        max_generations_per_run: 12,
        weekly_generations: 25,
        window_days: 7,
    };

    fn request(needed: u32, max: Option<u32>, used: u32) -> SpendRequest {
        SpendRequest {
            needed,
            max_generations: max,
            window_budget: CONFIG.weekly_generations,
            used_in_window: used,
        }
    }

    #[test]
    fn every_limit_is_enforced_at_its_boundary() {
        assert!(matches!(
            check_spend(&CONFIG, request(1, None, 0)),
            Err(BudgetError::MaxGenerationsRequired)
        ));
        assert!(check_spend(&CONFIG, request(12, Some(12), 0)).is_ok());
        assert!(matches!(
            check_spend(&CONFIG, request(1, Some(13), 0)),
            Err(BudgetError::AboveRunCap {
                requested: 13,
                cap: 12
            })
        ));
        assert!(matches!(
            check_spend(&CONFIG, request(4, Some(3), 0)),
            Err(BudgetError::RunNeedsMore {
                needed: 4,
                allowed: 3
            })
        ));
        assert!(check_spend(&CONFIG, request(2, Some(2), 23)).is_ok());
        assert!(matches!(
            check_spend(&CONFIG, request(3, Some(3), 23)),
            Err(BudgetError::WindowExceeded {
                used: 23,
                budget: 25,
                needed: 3,
                ..
            })
        ));
    }

    #[test]
    fn the_shipped_budget_file_loads_with_the_q0024_defaults() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets")
            .join(BUDGET_FILE);
        let config = BudgetConfig::load(&path).expect("shipped budget loads");
        assert_eq!(config, CONFIG);
        assert_eq!(config.window_seconds(), 7 * 86_400);
    }
}
