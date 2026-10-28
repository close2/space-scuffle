//! Capture-only delivery gate for genuine completed GPU mapping callbacks.
//!
//! No mapping or submission is mocked. Holding delivery drains the bounded staging
//! ring; the production uploader then retains its complete last packet and still
//! enforces the authored stall deadline. Normal gameplay allocates no gate state.

use super::gpu::{GpuResources, Packet, PacketLayout};
use bevy::prelude::*;
use bevy::render::render_resource::BufferId;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Explicit failures of bounded diagnostic ownership and packet capture.
#[derive(Debug, thiserror::Error)]
pub(super) enum GateError {
    /// Packet evidence was poisoned by a prior panicking owner.
    #[error("mist callback-gate evidence poisoned")]
    Poisoned,
    /// The fixed buffer layout or maximum staging count changed unexpectedly.
    #[error("mist callback-gate packet layout changed")]
    Layout,
    /// Actual failed-render evidence disagreed with the armed negative condition.
    #[error("mist callback-gate failure audit mismatch: {0}")]
    Failure(&'static str),
    /// Failed-condition evidence could not be persisted.
    #[error("mist callback-gate failure audit write failed: {0}")]
    Write(#[from] std::io::Error),
    /// Failed-condition evidence could not be encoded.
    #[error("mist callback-gate failure audit encoding failed: {0}")]
    Serialize(#[from] ron::Error),
}

/// Complete packet bytes and persistent buffer identities selected for rendering.
#[derive(Clone)]
pub(super) struct PacketProof {
    /// Actual selected sequence, unchanged while staging readiness is withheld.
    pub sequence: u64,
    /// Exact packed header, sample and frame bytes, with no geometry substitution.
    pub bytes: Vec<u8>,
    /// Actual persistent target buffer identities.
    pub targets: [Option<BufferId>; 3],
    /// Actual bounded staging identities; unused entries remain absent.
    pub staging: [Option<BufferId>; 8],
    /// Number of genuine completed maps whose readiness is currently withheld.
    pub held: usize,
}

struct ExpectedFailure {
    entity: Entity,
    out: PathBuf,
    retained: PacketProof,
    attempted: bool,
    accounting: super::gpu::GpuAccounting,
}

/// Shared opt-in gate and preallocated packet evidence for one capture process.
#[derive(Resource, Clone)]
pub(super) struct CallbackGate {
    release: Arc<AtomicBool>,
    proof: Arc<Mutex<PacketProof>>,
    failure: Arc<Mutex<Option<ExpectedFailure>>>,
}

impl CallbackGate {
    /// Allocates the diagnostic byte slab before capture, independently of production buffers.
    pub(super) fn new(layout: PacketLayout) -> Result<Self, GateError> {
        Ok(Self {
            release: Arc::new(AtomicBool::new(true)),
            failure: Arc::new(Mutex::new(None)),
            proof: Arc::new(Mutex::new(PacketProof {
                sequence: 0,
                bytes: vec![0; usize::try_from(layout.total_bytes).map_err(|_| GateError::Layout)?],
                targets: [None; 3],
                staging: [None; 8],
                held: 0,
            })),
        })
    }

    /// Withholds only successful map callback delivery; real failures remain failures.
    pub(super) fn hold(&self) {
        self.release.store(false, Ordering::Release);
    }
    /// Delivers all previously completed maps on the next production selection pass.
    pub(super) fn release(&self) {
        self.release.store(true, Ordering::Release);
    }
    /// Returns whether actual completed callbacks may restore CPU ownership.
    pub(super) fn released(&self) -> bool {
        self.release.load(Ordering::Acquire)
    }
    /// Shares the gate flag with a real asynchronous GPU mapping callback.
    pub(super) fn release_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.release)
    }
    /// Reads only the withheld map count while waiting, without copying packet bytes.
    pub(super) fn held(&self) -> Result<usize, GateError> {
        Ok(self.proof.lock().map_err(|_| GateError::Poisoned)?.held)
    }
    /// Reads the retained sequence without copying the bounded diagnostic byte slab.
    pub(super) fn sequence(&self) -> Result<u64, GateError> {
        Ok(self.proof.lock().map_err(|_| GateError::Poisoned)?.sequence)
    }
    /// Copies complete evidence once at a requested checkpoint, outside timed benchmarks.
    pub(super) fn snapshot(&self) -> Result<PacketProof, GateError> {
        Ok(self.proof.lock().map_err(|_| GateError::Poisoned)?.clone())
    }

    /// Records the selected complete packet in a fixed slab; retained frames keep its bytes.
    pub(super) fn record(
        &self,
        packet: Option<&Packet>,
        resources: &GpuResources,
        held: usize,
        staging: impl Iterator<Item = BufferId>,
    ) -> Result<(), GateError> {
        let mut proof = self.proof.lock().map_err(|_| GateError::Poisoned)?;
        proof.held = held;
        proof.targets = [
            Some(resources.headers.id()),
            Some(resources.samples.id()),
            Some(resources.frame.id()),
        ];
        proof.staging.fill(None);
        for (index, buffer) in staging.enumerate() {
            *proof.staging.get_mut(index).ok_or(GateError::Layout)? = Some(buffer);
        }
        if let Some(packet) = packet {
            let headers = bytemuck::cast_slice(&packet.headers);
            let samples = bytemuck::cast_slice(&packet.samples);
            let frame = bytemuck::bytes_of(&packet.frame);
            if headers.len() + samples.len() + frame.len() != proof.bytes.len() {
                return Err(GateError::Layout);
            }
            proof.bytes[..headers.len()].copy_from_slice(headers);
            proof.bytes[headers.len()..headers.len() + samples.len()].copy_from_slice(samples);
            proof.bytes[headers.len() + samples.len()..].copy_from_slice(frame);
            proof.sequence = packet.sequence;
        }
        Ok(())
    }

    /// Arms the one terminal invalid-light condition after actual callbacks are held.
    pub(super) fn arm_failure(
        &self,
        entity: Entity,
        out: PathBuf,
        accounting: super::gpu::GpuAccounting,
    ) -> Result<(), GateError> {
        let retained = self.snapshot()?;
        if retained.held == 0 {
            return Err(GateError::Failure("callbacks are not held"));
        }
        std::fs::write(
            out.join("view-light-held-before-invalid.packet.bin"),
            &retained.bytes,
        )?;
        let mut failure = self.failure.lock().map_err(|_| GateError::Poisoned)?;
        if failure.is_some() {
            return Err(GateError::Failure(
                "negative condition armed more than once",
            ));
        }
        *failure = Some(ExpectedFailure {
            entity,
            out,
            retained,
            attempted: false,
            accounting,
        });
        Ok(())
    }

    /// Records the original typed failure only after actual post-queue mist suppression.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn record_failure(
        &self,
        error: super::MistLightError,
        uploader_failed: bool,
        sequence: u64,
        accounting: super::gpu::GpuAccounting,
        views: usize,
        mist_draws: usize,
        unrelated_draws: usize,
        shell_entities: &[u64],
    ) -> Result<(), GateError> {
        #[derive(Serialize)]
        struct FailureReport {
            admission_error: String,
            light_entity: u64,
            uploader_failed: bool,
            publication_sequence: u64,
            selected_sequence: u64,
            held_callbacks: usize,
            views: usize,
            mist_draws: usize,
            unrelated_transparent_draws: usize,
            accounting: super::gpu::GpuAccounting,
            accounting_before: super::gpu::GpuAccounting,
            shell_entities: Vec<u64>,
            target_ids: String,
            staging_ids: String,
            packet_bytes: usize,
        }
        let mut failure = self.failure.lock().map_err(|_| GateError::Poisoned)?;
        let Some(expected) = failure.as_mut() else {
            return Ok(());
        };
        if expected.attempted {
            return Ok(());
        }
        expected.attempted = true;
        let proof = self.snapshot()?;
        if error != super::MistLightError::Illuminance(expected.entity) {
            return Err(GateError::Failure("original typed light failure changed"));
        }
        if !uploader_failed
            || mist_draws != 0
            || unrelated_draws == 0
            || shell_entities.is_empty()
            || views == 0
        {
            return Err(GateError::Failure(
                "actual render phase failed to suppress only mist",
            ));
        }
        if sequence != expected.retained.sequence
            || proof.sequence != sequence
            || proof.bytes != expected.retained.bytes
            || proof.targets != expected.retained.targets
            || proof.staging != expected.retained.staging
            || proof.held != expected.retained.held
        {
            return Err(GateError::Failure(
                "retained packet or buffer identities changed after invalid light",
            ));
        }
        let delta = accounting
            .checked_delta(expected.accounting)
            .map_err(|_| GateError::Failure("GPU accounting regressed"))?;
        if delta.submitted_packets != 0
            || delta.submitted_bytes != 0
            || delta.target_buffers != 0
            || delta.target_bytes != 0
            || delta.staging_buffers != 0
            || delta.staging_bytes != 0
        {
            return Err(GateError::Failure(
                "packet submission or persistent allocation changed after invalid light",
            ));
        }
        let report = FailureReport {
            admission_error: error.to_string(),
            light_entity: expected.entity.to_bits(),
            uploader_failed,
            publication_sequence: sequence,
            selected_sequence: proof.sequence,
            held_callbacks: proof.held,
            views,
            mist_draws,
            unrelated_transparent_draws: unrelated_draws,
            accounting,
            accounting_before: expected.accounting,
            shell_entities: shell_entities.to_vec(),
            target_ids: format!("{:?}", proof.targets),
            staging_ids: format!("{:?}", proof.staging),
            packet_bytes: proof.bytes.len(),
        };
        std::fs::write(
            expected.out.join("view-light-invalid-retained.packet.bin"),
            &proof.bytes,
        )?;
        std::fs::write(
            expected.out.join("view-light-failure.ron"),
            ron::ser::to_string_pretty(&report, ron::ser::PrettyConfig::default())?,
        )?;
        info!("ACTUAL VIEW-LIGHT FAILURE VERIFIED: original admission error retained, no mist draw or packet submission");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mist::gpu::GpuAccounting;

    fn armed_gate() -> Result<(CallbackGate, Entity, GpuAccounting), Box<dyn std::error::Error>> {
        let gate = CallbackGate::new(PacketLayout::new(1, 4)?)?;
        let entity = Entity::from_raw(17);
        gate.proof.lock().map_err(|_| "poisoned test proof")?.held = 3;
        let accounting = GpuAccounting {
            submitted_packets: 9,
            submitted_bytes: 900,
            ..default()
        };
        *gate.failure.lock().map_err(|_| "poisoned test failure")? = Some(ExpectedFailure {
            entity,
            out: PathBuf::new(),
            retained: gate.snapshot()?,
            attempted: false,
            accounting,
        });
        Ok((gate, entity, accounting))
    }

    #[test]
    fn failure_audit_rejects_new_submission_and_missing_real_shell_evidence(
    ) -> Result<(), Box<dyn std::error::Error>> {
        for change in 0..3 {
            let (gate, entity, mut accounting) = armed_gate()?;
            match change {
                0 => accounting.submitted_packets += 1,
                1 => accounting.submitted_bytes += 1,
                _ => accounting.target_buffers += 1,
            }
            assert!(matches!(
                gate.record_failure(
                    super::super::MistLightError::Illuminance(entity),
                    true,
                    0,
                    accounting,
                    1,
                    0,
                    1,
                    &[42]
                ),
                Err(GateError::Failure(
                    "packet submission or persistent allocation changed after invalid light"
                ))
            ));
        }
        let (gate, entity, accounting) = armed_gate()?;
        assert!(matches!(
            gate.record_failure(
                super::super::MistLightError::Illuminance(entity),
                true,
                0,
                accounting,
                1,
                0,
                1,
                &[]
            ),
            Err(GateError::Failure(
                "actual render phase failed to suppress only mist"
            ))
        ));
        Ok(())
    }

    #[test]
    fn failure_audit_rejects_changed_original_error_or_retained_bytes(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (gate, entity, accounting) = armed_gate()?;
        assert!(matches!(
            gate.record_failure(
                super::super::MistLightError::Illuminance(Entity::from_raw(18)),
                true,
                0,
                accounting,
                1,
                0,
                1,
                &[42]
            ),
            Err(GateError::Failure("original typed light failure changed"))
        ));
        let (gate, _, _) = armed_gate()?;
        gate.proof.lock().map_err(|_| "poisoned test proof")?.bytes[0] ^= 1;
        assert!(matches!(
            gate.record_failure(
                super::super::MistLightError::Illuminance(entity),
                true,
                0,
                accounting,
                1,
                0,
                1,
                &[42]
            ),
            Err(GateError::Failure(
                "retained packet or buffer identities changed after invalid light"
            ))
        ));
        Ok(())
    }
}
