//! The total-spend budget for paid Veo generations (plan `2026-11-04-phase-8-5-veo-media.md`,
//! D-4 as amended by D-12).
//!
//! # Responsibility
//! Loads the non-secret budget in `assets/media/veo_budget.ron` (the total in US cents and the
//! list price of one generation per model), sums what the ledger says was spent, and decides,
//! before any request, whether a run may start: the run must fit its own `--max-generations`,
//! and the spend so far plus the cost of every generation the run may start must not exceed the
//! total. There is no per-run, per-batch or time-window cap of our own (owner, 2026-11-05);
//! Google's quota refusals are handled by the run itself (`veo_run`).
//!
//! # How it fits
//! `generate-veo` calls [`BudgetConfig::spent_cents`] on the ledger history and [`check_spend`]
//! after the plan and prompt validation and before it reads any credential. The run itself
//! never starts more than `--max-generations` requests, retries included (`veo_run`), so that
//! number bounds what a run can cost.
//!
//! # Invariants
//! - Amounts are integer US cents; dollars appear only in messages ([`format_usd`]).
//! - Every accepted start costs its model's price: a start whose model has no price is an
//!   error, never a free generation. A start the service refused for quota costs nothing.
//! - A refusal names the amounts that caused it; nothing is spent when it is returned.
//! - The budget and prices are data: changing them needs no rebuild (charter §3.3).
//! - Image-model requests for effect textures (`image_request_started` lines, plan
//!   `2026-11-09-effect-visualization.md` §6.6) are paid starts like Veo starts, counted at
//!   their own model's price against the same total.

use crate::ledger::LedgerHistory;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Path of the budget file inside an asset bundle.
pub const BUDGET_FILE: &str = "media/veo_budget.ron";

/// Failures to load the budget, and refusals to spend.
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
    /// A model has no price in the budget file, so its generations cannot be counted.
    #[error("the model '{model}' has no price in {BUDGET_FILE}; add its list price per generation (US cents) before any run. Nothing was spent")]
    UnpricedModel {
        /// The model identifier.
        model: String,
    },
    /// A paid run was asked for without `--max-generations`.
    #[error("a paid run needs --max-generations <N>: the most generations this run may start, retries included")]
    MaxGenerationsRequired,
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
    /// The total budget would be exceeded.
    #[error("the total Veo budget is {}: {} spent, and this run needs {} for its --max-generations at list price. Nothing was spent", format_usd(*.budget_cents), format_usd(*.spent_cents), format_usd(*.needed_cents))]
    TotalExceeded {
        /// US cents the ledger records as spent.
        spent_cents: u64,
        /// US cents the run may spend (`--max-generations` × the model's price).
        needed_cents: u64,
        /// The total budget in US cents.
        budget_cents: u64,
    },
}

/// The budget in `veo_budget.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetConfig {
    /// The most every accepted generation together may cost at list price, in US cents.
    pub total_budget_usd_cents: u64,
    /// List price of one generation, in US cents, keyed by model identifier.
    pub price_per_generation_usd_cents: BTreeMap<String, u64>,
}

impl BudgetConfig {
    /// Loads and checks the budget file at `path`.
    ///
    /// # Errors
    /// [`BudgetError::Io`] or [`BudgetError::Invalid`] (unparsable, a zero total, no price, or
    /// a zero price, which would make a generation free).
    pub fn load(path: &Path) -> Result<Self, BudgetError> {
        let text = std::fs::read_to_string(path).map_err(|source| BudgetError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let invalid = |reason: String| BudgetError::Invalid {
            path: path.to_path_buf(),
            reason,
        };
        let config: Self = ron::from_str(&text).map_err(|error| invalid(error.to_string()))?;
        if config.total_budget_usd_cents == 0 {
            return Err(invalid(
                "total_budget_usd_cents must be at least 1".to_string(),
            ));
        }
        if config.price_per_generation_usd_cents.is_empty() {
            return Err(invalid(
                "price_per_generation_usd_cents must price at least one model".to_string(),
            ));
        }
        if let Some((model, _)) = config
            .price_per_generation_usd_cents
            .iter()
            .find(|(_, price)| **price == 0)
        {
            return Err(invalid(format!(
                "the price of '{model}' is 0; no generation is free"
            )));
        }
        Ok(config)
    }

    /// List price of one generation of `model`, in US cents.
    ///
    /// # Errors
    /// [`BudgetError::UnpricedModel`] if the budget file has no price for it.
    pub fn price_cents(&self, model: &str) -> Result<u64, BudgetError> {
        self.price_per_generation_usd_cents
            .get(model)
            .copied()
            .ok_or_else(|| BudgetError::UnpricedModel {
                model: model.to_string(),
            })
    }

    /// US cents spent by every accepted start `history` records, at each model's list price.
    ///
    /// # Errors
    /// [`BudgetError::UnpricedModel`] for the first start whose model has no price.
    pub fn spent_cents(&self, history: &LedgerHistory) -> Result<u64, BudgetError> {
        history.paid_start_models().try_fold(0_u64, |total, model| {
            Ok(total.saturating_add(self.price_cents(model)?))
        })
    }

    /// US cents left of the total after `spent_cents` (zero once the total is reached).
    #[must_use]
    pub fn remaining_cents(&self, spent_cents: u64) -> u64 {
        self.total_budget_usd_cents.saturating_sub(spent_cents)
    }

    /// One line stating the spend: `$S spent of $T, $R remaining`.
    #[must_use]
    pub fn spend_line(&self, spent_cents: u64) -> String {
        format!(
            "{} spent of {}, {} remaining",
            format_usd(spent_cents),
            format_usd(self.total_budget_usd_cents),
            format_usd(self.remaining_cents(spent_cents))
        )
    }
}

/// One run's request to spend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpendRequest {
    /// Generations the plan would start.
    pub needed: u32,
    /// The run's `--max-generations`, if given: the most it may start, retries included.
    pub max_generations: Option<u32>,
    /// US cents the ledger records as spent ([`BudgetConfig::spent_cents`]).
    pub spent_cents: u64,
    /// List price of one generation of the run's model, in US cents.
    pub price_cents: u64,
}

/// Decides whether a run may start. See the module docs for the rules.
///
/// The run is charged for its whole `--max-generations`, not only the generations its plan
/// needs, because retries may start up to that many and each start is paid.
///
/// # Errors
/// The first [`BudgetError`] refusal that applies.
pub fn check_spend(config: &BudgetConfig, request: SpendRequest) -> Result<(), BudgetError> {
    let allowed = request
        .max_generations
        .ok_or(BudgetError::MaxGenerationsRequired)?;
    if request.needed > allowed {
        return Err(BudgetError::RunNeedsMore {
            needed: request.needed,
            allowed,
        });
    }
    let needed_cents = u64::from(allowed).saturating_mul(request.price_cents);
    if request.spent_cents.saturating_add(needed_cents) > config.total_budget_usd_cents {
        return Err(BudgetError::TotalExceeded {
            spent_cents: request.spent_cents,
            needed_cents,
            budget_cents: config.total_budget_usd_cents,
        });
    }
    Ok(())
}

/// Formats US cents as dollars, `$D.CC`.
#[must_use]
pub fn format_usd(cents: u64) -> String {
    format!("${}.{:02}", cents / 100, cents % 100)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::ledger::{ApiMode, LedgerEvent, LedgerRecord};
    use crate::veo_client::VeoParameters;
    use sim_types::enums::AnimState;

    const MODEL: &str = "fixture-model";

    /// $200.00 in total; `MODEL` costs 40 cents a generation.
    fn config() -> BudgetConfig {
        BudgetConfig {
            total_budget_usd_cents: 20_000,
            price_per_generation_usd_cents: BTreeMap::from([(MODEL.to_string(), 40)]),
        }
    }

    fn request(needed: u32, max: Option<u32>, spent_cents: u64) -> SpendRequest {
        SpendRequest {
            needed,
            max_generations: max,
            spent_cents,
            price_cents: 40,
        }
    }

    fn started(index: usize, model: &str) -> LedgerRecord {
        LedgerRecord {
            time_utc: "1970-01-01T00:00:00Z".to_string(),
            unix_seconds: 0,
            run_id: "r".to_string(),
            event: LedgerEvent::RequestStarted {
                request_id: format!("r/{index}"),
                slug: "fixture_identity".to_string(),
                state: AnimState::Idle,
                model: model.to_string(),
                mode: ApiMode::VertexExpress,
                prompt_sha256: String::new(),
                prompt: String::new(),
                keyframe_sha256: None,
                last_frame_sha256: None,
                parameters: VeoParameters {
                    aspect_ratio: None,
                    duration_seconds: None,
                    sample_count: None,
                    generate_audio: None,
                    negative_prompt: None,
                    seed: None,
                },
            },
        }
    }

    #[test]
    fn max_generations_is_required_and_the_run_must_fit_it() {
        assert!(matches!(
            check_spend(&config(), request(1, None, 0)),
            Err(BudgetError::MaxGenerationsRequired)
        ));
        assert!(matches!(
            check_spend(&config(), request(4, Some(3), 0)),
            Err(BudgetError::RunNeedsMore {
                needed: 4,
                allowed: 3
            })
        ));
    }

    #[test]
    fn crossing_the_total_by_one_generation_is_refused_and_reaching_it_is_allowed() {
        // 497 generations spent ($198.80): 3 more reach $200.00 exactly; 4 cross it by one.
        assert!(check_spend(&config(), request(3, Some(3), 19_880)).is_ok());
        assert!(matches!(
            check_spend(&config(), request(4, Some(4), 19_880)),
            Err(BudgetError::TotalExceeded {
                spent_cents: 19_880,
                needed_cents: 160,
                budget_cents: 20_000
            })
        ));
    }

    #[test]
    fn the_run_is_charged_for_every_generation_it_may_start() {
        // Retries count against --max-generations, so the run may start that many.
        assert!(matches!(
            check_spend(&config(), request(3, Some(4), 19_880)),
            Err(BudgetError::TotalExceeded {
                needed_cents: 160,
                ..
            })
        ));
    }

    #[test]
    fn there_is_no_run_or_weekly_cap() {
        // 13 in one run, after 61 in the last seven days: both above the old caps.
        assert!(check_spend(&config(), request(13, Some(13), 61 * 40)).is_ok());
        let records: Vec<LedgerRecord> = (0..61).map(|i| started(i, MODEL)).collect();
        let history = LedgerHistory::from_records(&records);
        assert_eq!(config().spent_cents(&history).expect("priced"), 61 * 40);
    }

    #[test]
    fn spend_is_summed_from_accepted_starts_and_a_quota_refusal_is_free() {
        let refused = LedgerRecord {
            event: LedgerEvent::QuotaRefused {
                request_id: "r/2".to_string(),
                slug: "fixture_identity".to_string(),
                state: AnimState::Idle,
                http_status: 429,
                status: None,
                reason: None,
            },
            ..started(2, MODEL)
        };
        let history = LedgerHistory::from_records(&[started(1, MODEL), started(2, MODEL), refused]);
        assert_eq!(config().spent_cents(&history).expect("priced"), 40);
    }

    #[test]
    fn an_unpriced_model_is_an_error_never_a_free_generation() {
        let history = LedgerHistory::from_records(&[started(1, MODEL), started(2, "unknown")]);
        assert!(matches!(
            config().spent_cents(&history),
            Err(BudgetError::UnpricedModel { model }) if model == "unknown"
        ));
        assert!(matches!(
            config().price_cents("unknown"),
            Err(BudgetError::UnpricedModel { .. })
        ));
    }

    #[test]
    fn dollars_are_printed_from_cents() {
        assert_eq!(format_usd(0), "$0.00");
        assert_eq!(format_usd(40), "$0.40");
        assert_eq!(format_usd(1_640), "$16.40");
        assert_eq!(format_usd(20_000), "$200.00");
    }

    #[test]
    fn invalid_budget_files_are_refused() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("veo_budget.ron");
        for text in [
            "(total_budget_usd_cents: 0, price_per_generation_usd_cents: {\"m\": 40})",
            "(total_budget_usd_cents: 100, price_per_generation_usd_cents: {})",
            "(total_budget_usd_cents: 100, price_per_generation_usd_cents: {\"m\": 0})",
            "(max_generations_per_run: 12, weekly_generations: 60, window_days: 7)",
        ] {
            std::fs::write(&path, text).expect("write");
            assert!(
                matches!(BudgetConfig::load(&path), Err(BudgetError::Invalid { .. })),
                "{text}"
            );
        }
    }

    #[test]
    fn the_shipped_budget_file_loads_with_the_owner_limit_and_the_price_table() {
        // Owner, 2026-11-05: $200.00 in total (D-12); Veo 3.1 Fast at 720p with sound is 40 cents.
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets")
            .join(BUDGET_FILE);
        let config = BudgetConfig::load(&path).expect("shipped budget loads");
        assert_eq!(config.total_budget_usd_cents, 20_000);
        assert_eq!(
            config
                .price_cents(crate::DEFAULT_VEO_MODEL)
                .expect("the default model is priced"),
            40
        );
        // Image models (effect plan §6.6): budgeted at the dearer model's 2K price, $0.101,
        // rounded up to 11 cents.
        for model in ["gemini-nano-banana-2.1", "gemini-3.1-flash-image"] {
            assert_eq!(config.price_cents(model).expect("image model priced"), 11);
        }
    }
}
