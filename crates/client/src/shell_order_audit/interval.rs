//! Bounded conservative screenshot intervals shared between main and render worlds.
//!
//! A callback does not reveal Bevy's private copy epoch. Every candidate from the
//! last completed observation through a subsequent closure extraction is retained.

use super::model::{AuditConfig, AuditError, EpochEvidence};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Idle,
    Armed {
        nonce: u64,
        screenshot: Option<u64>,
    },
    Closing {
        nonce: u64,
        screenshot: u64,
        epoch: Option<u64>,
    },
    Closed,
}

/// Complete conservative request/closure candidate set and explicit claim boundary.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct IntervalReport {
    /// Monotonic screenshot request identity.
    pub nonce: u64,
    /// Bound actual screenshot entity, absent only before binding.
    pub screenshot: Option<u64>,
    /// Every completed candidate epoch, without selective order filtering.
    pub epochs: Vec<EpochEvidence>,
    /// Stable, limited, inconclusive or still pending; not an art-quality judgment.
    pub classification: String,
    /// Sum of compact serialized epoch/asset components, excluding report wrapper and delimiters.
    /// This is not the complete file length or process RSS.
    pub retained_bytes: usize,
    /// Deduplicated exact CPU source descriptors and binary sidecar references.
    pub assets: Vec<super::model::AssetRecord>,
    /// Unique retained raw CPU source bytes, separately bounded.
    pub raw_source_bytes: usize,
}

impl IntervalReport {
    /// Encodes the whole report compactly and checks its exact UTF-8 publication length.
    pub(crate) fn encode_bounded(&self, byte_limit: usize) -> Result<String, AuditError> {
        let encoded =
            ron::ser::to_string(self).map_err(|error| AuditError::Source(error.to_string()))?;
        if encoded.len() > byte_limit {
            return Err(AuditError::Budget("complete compact report publication"));
        }
        Ok(encoded)
    }
}

/// Single-owner screenshot interval state, protected by the cross-world bridge.
pub(crate) struct Collector {
    config: AuditConfig,
    state: State,
    next_nonce: u64,
    last_epoch: u64,
    latest: Option<EpochEvidence>,
    report: Option<IntervalReport>,
    asset_bytes: usize,
}

impl Collector {
    /// Exposes the last real completed epoch to the renderer lifecycle regression.
    #[cfg(test)]
    pub(super) fn latest_for_test(&self) -> Option<&EpochEvidence> {
        self.latest.as_ref()
    }

    /// Validates declared limits and constructs an idle interval collector.
    pub(crate) fn new(config: AuditConfig) -> Result<Self, AuditError> {
        config.validate()?;
        Ok(Self {
            config,
            state: State::Idle,
            next_nonce: 0,
            last_epoch: 0,
            latest: None,
            report: None,
            asset_bytes: 0,
        })
    }

    /// Reserves summed serialized asset-component bytes against the shared evidence budget.
    pub(crate) fn reserve_asset_bytes(&mut self, bytes: usize) -> Result<(), AuditError> {
        if bytes
            .checked_add(
                self.report
                    .as_ref()
                    .map_or(0, |report| report.retained_bytes),
            )
            .map_or(true, |total| total > self.config.max_bytes)
        {
            return Err(AuditError::Budget("assets and phase records"));
        }
        self.asset_bytes = bytes;
        Ok(())
    }

    /// Advances the checked extraction identity and assigns a pending closure boundary.
    pub(crate) fn extract_epoch(&mut self) -> Result<u64, AuditError> {
        self.last_epoch = self
            .last_epoch
            .checked_add(1)
            .ok_or(AuditError::Exhausted)?;
        if let State::Closing { epoch, .. } = &mut self.state {
            if epoch.is_none() {
                *epoch = Some(self.last_epoch);
            }
        }
        Ok(self.last_epoch)
    }

    /// Returns whether a new interval has a complete ready lower-bound epoch.
    pub(crate) fn ready(&self) -> bool {
        self.state == State::Idle && self.latest.as_ref().is_some_and(|frame| frame.ready)
    }
    /// Reports whether candidate epochs must be retained for an outstanding screenshot.
    pub(crate) fn active(&self) -> bool {
        matches!(self.state, State::Armed { .. } | State::Closing { .. })
    }

    /// Starts an interval with the latest completed epoch as its inclusive lower bound.
    pub(crate) fn arm(&mut self) -> Result<u64, AuditError> {
        if !self.ready() {
            return Err(AuditError::Lifecycle("arm requires idle ready observation"));
        }
        let initial = self
            .latest
            .clone()
            .ok_or(AuditError::Lifecycle("missing lower bound"))?;
        self.next_nonce = self
            .next_nonce
            .checked_add(1)
            .ok_or(AuditError::Exhausted)?;
        self.report = Some(IntervalReport {
            nonce: self.next_nonce,
            screenshot: None,
            epochs: vec![],
            classification: "pending callback and closure".into(),
            retained_bytes: 0,
            raw_source_bytes: 0,
            assets: vec![],
        });
        self.state = State::Armed {
            nonce: self.next_nonce,
            screenshot: None,
        };
        self.append(initial)?;
        Ok(self.next_nonce)
    }

    /// Binds exactly one actual screenshot token to the matching request nonce.
    pub(crate) fn bind(&mut self, nonce: u64, screenshot: u64) -> Result<(), AuditError> {
        match &mut self.state {
            State::Armed {
                nonce: expected,
                screenshot: token,
            } if *expected == nonce && token.is_none() => {
                *token = Some(screenshot);
                self.report
                    .as_mut()
                    .ok_or(AuditError::Lifecycle("missing interval"))?
                    .screenshot = Some(screenshot);
                Ok(())
            }
            _ => Err(AuditError::Lifecycle(
                "duplicate or foreign screenshot binding",
            )),
        }
    }

    /// Begins conservative closure after the matching actual callback.
    pub(crate) fn callback(&mut self, screenshot: u64) -> Result<(), AuditError> {
        match self.state {
            State::Armed {
                nonce,
                screenshot: Some(expected),
            } if expected == screenshot => {
                self.state = State::Closing {
                    nonce,
                    screenshot,
                    epoch: None,
                };
                Ok(())
            }
            _ => Err(AuditError::Lifecycle("unexpected screenshot callback")),
        }
    }

    /// Publishes one contiguous completed render epoch and closes a satisfied boundary.
    pub(crate) fn complete(&mut self, frame: EpochEvidence) -> Result<(), AuditError> {
        if self
            .latest
            .as_ref()
            .is_some_and(|prior| prior.epoch.checked_add(1) != Some(frame.epoch))
        {
            return Err(AuditError::Lifecycle(
                "noncontiguous completed render epochs",
            ));
        }
        if frame.epoch > self.last_epoch {
            return Err(AuditError::Lifecycle(
                "incomplete extraction or relevant view",
            ));
        }
        if matches!(self.state, State::Armed { .. } | State::Closing { .. }) {
            self.append(frame.clone())?;
        }
        if matches!(self.state, State::Closing { epoch: Some(end), .. } if frame.epoch >= end) {
            self.state = State::Closed;
        }
        self.latest = Some(frame);
        Ok(())
    }

    fn append(&mut self, frame: EpochEvidence) -> Result<(), AuditError> {
        if frame.views.len() > self.config.max_views
            || frame
                .views
                .iter()
                .any(|view| view.queued.len().max(view.sorted.len()) > self.config.max_items)
        {
            return Err(AuditError::Budget("views or phase items"));
        }
        let report = self
            .report
            .as_mut()
            .ok_or(AuditError::Lifecycle("missing active report"))?;
        if report.epochs.len() >= self.config.max_epochs {
            return Err(AuditError::Budget("candidate epochs"));
        }
        let bytes = ron::ser::to_string(&frame)
            .map_err(|error| AuditError::Source(error.to_string()))?
            .len();
        let total = report
            .retained_bytes
            .checked_add(bytes)
            .ok_or(AuditError::Budget("byte accounting overflow"))?;
        if total
            .checked_add(self.asset_bytes)
            .map_or(true, |size| size > self.config.max_bytes)
        {
            return Err(AuditError::Budget("retained evidence bytes"));
        }
        report.retained_bytes = total;
        report.epochs.push(frame);
        Ok(())
    }

    /// Classifies and returns a closed interval; pending intervals remain retained.
    pub(crate) fn finish(&mut self) -> Result<Option<IntervalReport>, AuditError> {
        if self.state != State::Closed {
            return Ok(None);
        }
        let report = self
            .report
            .as_mut()
            .ok_or(AuditError::Lifecycle("closed report missing"))?;
        let initial = report
            .epochs
            .first()
            .ok_or(AuditError::Lifecycle("empty interval"))?;
        report.classification = if report.epochs.iter().any(|frame| !frame.ready) {
            "inconclusive: a candidate draw was not ready"
        } else if report
            .epochs
            .iter()
            .any(|frame| !initial.same_sources_and_order(frame))
        {
            "inconclusive: source or sorted order varied"
        } else if report.epochs.iter().any(|frame| {
            !frame.source.limitations.is_empty()
                || frame.views.iter().any(|view| {
                    view.queued.iter().chain(&view.sorted).any(|item| {
                        matches!(
                            item.material,
                            super::model::MaterialSource::Unsupported { .. }
                        ) || matches!(
                            item.association.kind,
                            super::model::AssociationKind::Unsupported(_)
                                | super::model::AssociationKind::Pending
                        )
                    })
                })
        }) {
            "limited: stable observed order; relevant source coverage is incomplete"
        } else {
            "stable source and order association; exact GPU copy epoch unknown"
        }
        .into();
        Ok(Some(report.clone()))
    }

    /// Clears a closed interval only after its report, sources and screenshot record were published.
    pub(crate) fn acknowledge_publication(&mut self) -> Result<(), AuditError> {
        if self.state != State::Closed || self.report.is_none() {
            return Err(AuditError::Lifecycle(
                "publication acknowledgement requires retained closed interval",
            ));
        }
        self.state = State::Idle;
        self.report = None;
        Ok(())
    }

    /// Clones all available partial evidence without advancing the protocol.
    pub(crate) fn incomplete(&self) -> Option<IntervalReport> {
        self.report.clone()
    }
}
