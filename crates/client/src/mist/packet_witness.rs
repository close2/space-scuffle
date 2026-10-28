//! Passive opt-in evidence of the uploader's actual selected complete packet.
//!
//! The fixed byte slab follows selection without controlling it. Retained frames
//! keep the previous bytes; failures invalidate diagnostic evidence, never mutate
//! staging ownership, production upload policy or GPU callbacks.

use super::gpu::{GpuResources, Packet, PacketLayout};
use bevy::prelude::*;
use bevy::render::render_resource::BufferId;
use std::sync::{Arc, Mutex};

/// Evidence failures are separate from production uploader failures.
#[derive(Clone, Debug, thiserror::Error)]
pub(crate) enum WitnessError {
    /// A prior panic invalidated the shared witness owner.
    #[error("selected packet witness poisoned")]
    Poisoned,
    /// The actual selected packet no longer fits the admitted fixed slab.
    #[error("selected packet witness layout changed")]
    Layout,
    /// A production upload failure makes selection evidence unavailable.
    #[error("selected packet uploader failed")]
    Uploader,
}

/// Complete actual selected CPU packet and typed buffer association.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PacketProof {
    /// At least one complete packet has been selected.
    pub ready: bool,
    /// This frame selected new staging rather than retaining the previous packet.
    pub freshly_selected: bool,
    /// Actual selected packet sequence, not the latest main-world sequence.
    pub sequence: u64,
    /// Exact packed headers, samples and frame block.
    pub bytes: Vec<u8>,
    /// Actual persistent GPU target IDs; absent before first selection.
    pub targets: [Option<BufferId>; 3],
    /// Actual fixed staging ring IDs, in ring order.
    pub staging: Vec<BufferId>,
}

struct State {
    proof: PacketProof,
    fault: Option<WitnessError>,
    staging_count: usize,
}

/// Shared preallocated diagnostic slab, absent from ordinary gameplay.
#[derive(Resource, Clone)]
pub(crate) struct PacketWitness(Arc<Mutex<State>>);

impl PacketWitness {
    /// Allocates exactly one declared packet slab before real selection starts.
    pub(crate) fn new(layout: PacketLayout, staging: usize) -> Result<Self, WitnessError> {
        let bytes = usize::try_from(layout.total_bytes).map_err(|_| WitnessError::Layout)?;
        if !(1..=8).contains(&staging) {
            return Err(WitnessError::Layout);
        }
        Ok(Self(Arc::new(Mutex::new(State {
            proof: PacketProof {
                ready: false,
                freshly_selected: false,
                sequence: 0,
                bytes: vec![0; bytes],
                targets: [None; 3],
                staging: Vec::with_capacity(staging),
            },
            fault: None,
            staging_count: staging,
        }))))
    }

    /// Records real selection; `None` retains the last complete selected bytes.
    pub(super) fn record(
        &self,
        packet: Option<&Packet>,
        resources: &GpuResources,
        staging: impl Iterator<Item = BufferId>,
        failed: bool,
    ) {
        let Ok(mut state) = self.0.lock() else { return };
        if state.fault.is_some() {
            return;
        }
        if failed {
            state.fault = Some(WitnessError::Uploader);
            return;
        }
        let result = update(&mut state.proof, packet);
        if let Err(error) = result {
            state.fault = Some(error);
            return;
        }
        state.proof.targets = [
            resources.headers.id(),
            resources.samples.id(),
            resources.frame.id(),
        ]
        .map(Some);
        state.proof.staging.clear();
        let mut count = 0;
        for (index, buffer) in staging.enumerate() {
            if index >= state.staging_count {
                state.fault = Some(WitnessError::Layout);
                return;
            }
            state.proof.staging.push(buffer);
            count += 1;
        }
        if count != state.staging_count {
            state.fault = Some(WitnessError::Layout);
        }
    }

    /// Copies bounded evidence only for opt-in diagnostic render observations.
    pub(crate) fn snapshot(&self) -> Result<PacketProof, WitnessError> {
        let state = self.0.lock().map_err(|_| WitnessError::Poisoned)?;
        if let Some(error) = &state.fault {
            return Err(error.clone());
        }
        Ok(state.proof.clone())
    }
}

fn update(proof: &mut PacketProof, packet: Option<&Packet>) -> Result<(), WitnessError> {
    proof.freshly_selected = packet.is_some();
    let Some(packet) = packet else {
        return Ok(());
    };
    let headers = bytemuck::cast_slice(&packet.headers);
    let samples = bytemuck::cast_slice(&packet.samples);
    let frame = bytemuck::bytes_of(&packet.frame);
    let samples_end = headers
        .len()
        .checked_add(samples.len())
        .ok_or(WitnessError::Layout)?;
    if samples_end.checked_add(frame.len()) != Some(proof.bytes.len()) {
        return Err(WitnessError::Layout);
    }
    proof.bytes[..headers.len()].copy_from_slice(headers);
    proof.bytes[headers.len()..samples_end].copy_from_slice(samples);
    proof.bytes[samples_end..].copy_from_slice(frame);
    proof.sequence = packet.sequence;
    proof.ready = true;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_selected_bytes_survive_retention_and_change_only_on_new_selection(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let layout = PacketLayout::new(2, 4)?;
        let witness = PacketWitness::new(layout, 3)?;
        let mut packet = Packet::new(layout);
        packet.sequence = 7;
        let mut proof = witness.snapshot()?;
        update(&mut proof, Some(&packet))?;
        let first = proof.bytes.clone();
        packet.headers[0].control.x = 31;
        update(&mut proof, None)?;
        assert_eq!(proof.bytes, first);
        assert_eq!(proof.sequence, 7);
        packet.sequence = 8;
        update(&mut proof, Some(&packet))?;
        assert_ne!(proof.bytes, first);
        assert_eq!(proof.sequence, 8);
        proof.bytes.pop();
        assert!(matches!(
            update(&mut proof, Some(&packet)),
            Err(WitnessError::Layout)
        ));
        Ok(())
    }

    #[test]
    fn poisoned_witness_is_an_explicit_error() -> Result<(), Box<dyn std::error::Error>> {
        let witness = PacketWitness::new(PacketLayout::new(1, 4)?, 3)?;
        let copy = witness.clone();
        let result = std::thread::spawn(move || {
            let _guard = copy.0.lock();
            std::panic::resume_unwind(Box::new("intentional witness poison fixture"));
        })
        .join();
        assert!(result.is_err());
        assert!(matches!(witness.snapshot(), Err(WitnessError::Poisoned)));
        Ok(())
    }
}
