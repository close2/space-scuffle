//! Deduplicated exact CPU asset revisions for the bounded render experiment.
//!
//! Equality checks actual content, not only handles or noncryptographic digests.
//! The declared serialized-evidence budget is not a process resident-memory cap.

use super::model::{AssetRecord, AuditError};
use bevy::prelude::*;
use std::collections::BTreeMap;

#[derive(Default)]
/// Deduplicated exact CPU content revisions and separately bounded raw sidecars.
pub(super) struct Inventory {
    /// Serializable descriptors in stable revision-index order.
    pub records: Vec<AssetRecord>,
    latest: BTreeMap<String, usize>,
    /// Sum of serialized descriptor component lengths, excluding enclosing report syntax.
    pub serialized_bytes: usize,
    /// Total unique raw CPU content bytes retained in sidecar storage.
    pub raw_bytes: usize,
    blobs: BTreeMap<String, Vec<u8>>,
    epoch: BTreeMap<String, usize>,
}

impl Inventory {
    /// Clears only the per-extraction shared-asset lookup, preserving prior exact revisions.
    pub(super) fn begin_epoch(&mut self) {
        self.epoch.clear();
    }
    /// Writes exact retained source bytes under digest-named binary sidecar paths.
    pub(super) fn persist(&self, out: &std::path::Path) -> Result<(), AuditError> {
        let directory = out.join("shell-sources");
        std::fs::create_dir_all(&directory)
            .map_err(|error| AuditError::Source(error.to_string()))?;
        for (digest, bytes) in &self.blobs {
            std::fs::write(directory.join(format!("{digest}.bin")), bytes)
                .map_err(|error| AuditError::Source(error.to_string()))?;
        }
        Ok(())
    }

    /// Records actual loaded shader sources and import/definition descriptors.
    pub(super) fn shaders(
        &mut self,
        world: &World,
        limit: usize,
    ) -> Result<Vec<usize>, AuditError> {
        let shaders = world.resource::<Assets<Shader>>();
        let mut sources: Vec<_> = shaders.iter().collect();
        sources.sort_by_key(|(id, _)| format!("{id:?}"));
        sources
            .into_iter()
            .map(|(id, shader)| {
                let source = format!("{:?}", shader.source);
                self.admit(
                    format!("shader:{id:?}"),
                    format!(
                        "path={}; import={:?}; imports={:?}; additional={:?}; defs={:?}",
                        shader.path,
                        shader.import_path,
                        shader.imports,
                        shader.additional_imports,
                        shader.shader_defs
                    ),
                    source.as_bytes(),
                    limit,
                )
            })
            .collect()
    }
    fn admit(
        &mut self,
        identity: String,
        descriptor: String,
        bytes: &[u8],
        limit: usize,
    ) -> Result<usize, AuditError> {
        if let Some(index) = self.latest.get(&identity) {
            let prior = &self.records[*index];
            if prior.descriptor == descriptor
                && self
                    .blobs
                    .get(&prior.blake3_256)
                    .is_some_and(|content| content == bytes)
            {
                self.epoch.insert(identity, *index);
                return Ok(*index);
            }
        }
        if bytes.len() > limit {
            return Err(AuditError::Budget("single source asset"));
        }
        let digest = blake3::hash(bytes).to_hex().to_string();
        if !self.blobs.contains_key(&digest) {
            let raw_total = self
                .raw_bytes
                .checked_add(bytes.len())
                .ok_or(AuditError::Budget("raw source byte overflow"))?;
            if raw_total > limit {
                return Err(AuditError::Budget("raw source revisions"));
            }
            self.blobs.insert(digest.clone(), bytes.to_vec());
            self.raw_bytes = raw_total;
        }
        let record = AssetRecord {
            identity: identity.clone(),
            descriptor,
            byte_length: bytes.len(),
            blob: format!("shell-sources/{digest}.bin"),
            blake3_256: digest,
        };
        let added = ron::ser::to_string(&record)
            .map_err(|error| AuditError::Source(error.to_string()))?
            .len();
        let total = self
            .serialized_bytes
            .checked_add(added)
            .ok_or(AuditError::Budget("asset accounting overflow"))?;
        if total > limit {
            return Err(AuditError::Budget("source asset revisions"));
        }
        let index = self.records.len();
        self.records.push(record);
        self.latest.insert(identity.clone(), index);
        self.epoch.insert(identity, index);
        self.serialized_bytes = total;
        Ok(index)
    }

    /// Records one actual CPU image revision once per extraction epoch.
    pub(super) fn image(
        &mut self,
        world: &World,
        handle: &Handle<Image>,
        limit: usize,
    ) -> Result<usize, AuditError> {
        let key = format!("image:{:?}", handle.id());
        if let Some(index) = self.epoch.get(&key) {
            return Ok(*index);
        }
        let image = world
            .resource::<Assets<Image>>()
            .get(handle)
            .ok_or_else(|| AuditError::NotReady(format!("image {:?}", handle.id())))?;
        self.admit(
            format!("image:{:?}", handle.id()),
            format!(
                "{:?}; sampler={:?}; view={:?}",
                image.texture_descriptor, image.sampler, image.texture_view_descriptor
            ),
            &image.data,
            limit,
        )
    }

    /// Records one actual CPU mesh revision once per extraction epoch.
    pub(super) fn mesh(
        &mut self,
        world: &World,
        handle: &Handle<Mesh>,
        limit: usize,
    ) -> Result<usize, AuditError> {
        let key = format!("mesh:{:?}", handle.id());
        if let Some(index) = self.epoch.get(&key) {
            return Ok(*index);
        }
        let mesh = world
            .resource::<Assets<Mesh>>()
            .get(handle)
            .ok_or_else(|| AuditError::NotReady(format!("mesh {:?}", handle.id())))?;
        let length = mesh
            .attributes()
            .try_fold(0_usize, |size, (_, attribute)| {
                size.checked_add(attribute.get_bytes().len())
            })
            .ok_or(AuditError::Budget("mesh source size"))?;
        if length > limit {
            return Err(AuditError::Budget("single mesh source"));
        }
        let mut bytes = Vec::with_capacity(length);
        let mut descriptors = Vec::new();
        for (attribute, data) in mesh.attributes() {
            descriptors.push(format!("{attribute:?}:{}", data.get_bytes().len()));
            bytes.extend_from_slice(data.get_bytes());
        }
        self.admit(
            format!("mesh:{:?}", handle.id()),
            format!(
                "{:?}; attributes={descriptors:?}; indices={:?}",
                mesh.primitive_topology(),
                mesh.indices()
            ),
            &bytes,
            limit,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_revisions_are_deduplicated_and_raw_budget_is_separate() -> Result<(), AuditError> {
        let mut inventory = Inventory::default();
        let first = inventory.admit(
            "actual CPU unit fixture".into(),
            "format".into(),
            &[1, 2, 3],
            4096,
        )?;
        let same = inventory.admit(
            "actual CPU unit fixture".into(),
            "format".into(),
            &[1, 2, 3],
            4096,
        )?;
        assert_eq!(first, same);
        assert_eq!(inventory.raw_bytes, 3);
        assert_eq!(
            inventory.records[first].blake3_256,
            blake3::hash(&[1, 2, 3]).to_hex().as_str()
        );
        let next = inventory.admit(
            "actual CPU unit fixture".into(),
            "format".into(),
            &[1, 2, 4],
            4096,
        )?;
        assert_ne!(first, next);
        assert_eq!(inventory.raw_bytes, 6);
        assert!(matches!(
            inventory.admit("another".into(), "format".into(), &[5; 7], 7),
            Err(AuditError::Budget(_))
        ));
        Ok(())
    }
}
