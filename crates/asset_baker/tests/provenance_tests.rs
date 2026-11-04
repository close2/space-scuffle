//! Provenance records, the data-defined required set and the strict audit (plan
//! `docs/plans/2026-11-04-phase-8-5-veo-media.md`, decision D-6).
//!
//! Most tests build a throw-away asset root: a tiny hand-written atlas, stand-in clip files (the
//! audit only hashes clips, it never decodes them) and a provenance file sealed over them. Each
//! test then breaks one thing and checks that the audit names it with a typed reason. One test
//! bakes real clips with FFmpeg end to end, and the last ones audit the shipped bundle.
//! Identities are chosen from the shipped catalog by what they publish, never by name.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

mod common;

use asset_baker::ledger::{
    read_ledger_numbered, sha256_hex, LedgerEvent, LedgerRecord, NumberedRecord,
};
use asset_baker::media_audit::{
    audit_media, AuditInputs, MediaAuditError, MediaAuditRecord, MediaAuditReport, MediaAuditStatus,
};
use asset_baker::provenance::{
    bake_with_provenance, provenance_path, BakeClip, BakeParameters, ClipOrigin, MediaProvenance,
    ProvenanceError, RequiredIdentity, RequiredSet, StateProvenance, Tier, UnverifiedReason,
    PROVENANCE_FORMAT_VERSION,
};
use asset_baker::{ActionSlice, FlipbookAtlasMeta, FlipbookLayout};
use common::cli::{baker, identity_with_actions, shipped_assets, text};
use sim_types::AnimState;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The commit that added the original Veo clips; any well-formed commit would do here.
const LEGACY_COMMIT: &str = "3d9a9c4428803292e470bb339454c892566dd44f";

/// The states of the fixture identity, in strip order.
const STATES: [AnimState; 3] = [AnimState::Idle, AnimState::Walk, AnimState::Attack];

/// Side of one square fixture frame, in pixels.
const FRAME: u32 = 2;

fn catalog() -> sim_core::UnitCatalog {
    sim_fixtures::shipped_catalogs().units
}

/// An identity that publishes exactly Idle, Walk and Attack.
fn combat_identity(catalog: &sim_core::UnitCatalog) -> String {
    identity_with_actions(catalog, &STATES, false, |_| true)
}

/// Another identity, with no media in a fixture root.
fn other_identity(catalog: &sim_core::UnitCatalog, not: &str) -> String {
    identity_with_actions(catalog, &STATES, false, |archetype| archetype.slug != not)
}

/// A throw-away asset root holding one identity's atlas, clips and provenance.
struct Fixture {
    root: tempfile::TempDir,
    slug: String,
    /// One grey level per frame of the strip.
    frames: Vec<u8>,
    meta: FlipbookAtlasMeta,
    origins: BTreeMap<AnimState, ClipOrigin>,
}

impl Fixture {
    /// Writes the atlas (one frame per state), a distinct stand-in clip per state, a concept
    /// atlas whose pixels differ from every fixture frame, and a sealed provenance file.
    fn new(slug: &str) -> Self {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(root.path().join("textures")).expect("textures dir");
        std::fs::create_dir_all(root.path().join("videos")).expect("videos dir");
        let actions = STATES
            .iter()
            .zip(0u32..)
            .map(|(state, frame)| (*state, slice(frame, 1)))
            .collect();
        let fixture = Self {
            root,
            slug: slug.to_string(),
            frames: vec![40, 80, 120],
            meta: FlipbookAtlasMeta {
                name: slug.to_string(),
                frame_width: FRAME,
                frame_height: FRAME,
                fps: 15,
                total_frames: 3,
                layout: FlipbookLayout::VerticalStrip,
                actions,
            },
            origins: STATES
                .iter()
                .map(|state| {
                    (
                        *state,
                        ClipOrigin::LegacyVeo {
                            commit: LEGACY_COMMIT.to_string(),
                        },
                    )
                })
                .collect(),
        };
        for state in STATES {
            std::fs::write(
                fixture.clip_path(state),
                format!("stand-in clip of {state:?}"),
            )
            .expect("clip");
        }
        write_strip(
            &fixture.root.path().join("textures/concept_video.png"),
            &[200],
        );
        let mut concept = fixture.meta.clone();
        concept.name = "concept_video".into();
        concept.total_frames = 1;
        concept.actions = BTreeMap::from([(AnimState::Idle, slice(0, 1))]);
        concept
            .save_to_ron(
                &fixture
                    .root
                    .path()
                    .join("textures/concept_video.flipbook.ron"),
            )
            .expect("concept manifest");
        fixture.write_atlas();
        fixture.seal();
        fixture
    }

    fn textures(&self) -> PathBuf {
        self.root.path().join("textures")
    }

    fn clip_name(&self, state: AnimState) -> String {
        format!("{}_{}.mp4", self.slug, format!("{state:?}").to_lowercase())
    }

    fn clip_path(&self, state: AnimState) -> PathBuf {
        self.root.path().join("videos").join(self.clip_name(state))
    }

    fn atlas_path(&self) -> PathBuf {
        self.textures().join(format!("{}.png", self.slug))
    }

    fn manifest_path(&self) -> PathBuf {
        self.textures().join(format!("{}.flipbook.ron", self.slug))
    }

    fn write_atlas(&self) {
        write_strip(&self.atlas_path(), &self.frames);
        self.meta
            .save_to_ron(&self.manifest_path())
            .expect("manifest");
    }

    /// Writes a provenance file that matches the files as they are now.
    fn seal(&self) {
        let states = self
            .meta
            .actions
            .iter()
            .map(|(state, slice)| {
                let clip = self.clip_name(*state);
                let bytes = std::fs::read(self.clip_path(*state)).expect("clip bytes");
                (
                    *state,
                    StateProvenance {
                        clip,
                        clip_sha256: sha256_hex(&bytes),
                        start_frame: slice.start_frame,
                        frame_count: slice.frame_count,
                        origin: self.origins[state].clone(),
                    },
                )
            })
            .collect();
        let provenance = MediaProvenance {
            format_version: PROVENANCE_FORMAT_VERSION,
            name: self.slug.clone(),
            atlas_sha256: sha256_hex(&std::fs::read(self.atlas_path()).expect("atlas")),
            manifest_sha256: sha256_hex(&std::fs::read(self.manifest_path()).expect("manifest")),
            bake: BakeParameters {
                frame_size: FRAME,
                fps: 15,
                max_frames_per_state: 1,
                ffmpeg_version: "ffmpeg version fixture".into(),
            },
            states,
        };
        let path = provenance_path(&self.textures(), &self.slug).expect("valid slug");
        provenance.save(&path).expect("provenance");
    }

    fn audit(&self, required: &RequiredSet, ledger: Option<&[NumberedRecord]>) -> MediaAuditReport {
        audit_media(
            self.root.path(),
            &catalog(),
            &AuditInputs { required, ledger },
        )
    }

    fn entry(&self, report: &MediaAuditReport) -> MediaAuditRecord {
        report
            .entries
            .iter()
            .find(|entry| entry.slug == self.slug)
            .cloned()
            .expect("the fixture identity is in the report")
    }

    fn required(&self) -> RequiredSet {
        RequiredSet::new(vec![RequiredIdentity {
            slug: self.slug.clone(),
            tier: Tier::Required,
        }])
    }

    /// The fixture's audit entry with only its own identity required and no ledger.
    fn audited(&self) -> MediaAuditRecord {
        self.entry(&self.audit(&self.required(), None))
    }
}

fn slice(start_frame: u32, frame_count: u32) -> ActionSlice {
    ActionSlice {
        start_frame,
        frame_count,
        looping: true,
        hold_last_frame: false,
    }
}

/// A vertical strip of opaque square frames, each filled with one grey level.
fn write_strip(path: &Path, frames: &[u8]) {
    let height = FRAME * u32::try_from(frames.len()).expect("few frames");
    let image = image::RgbaImage::from_fn(FRAME, height, |_, y| {
        let grey = frames[usize::try_from(y / FRAME).expect("small index")];
        image::Rgba([grey, grey, grey, 255])
    });
    image.save(path).expect("strip");
}

fn assert_unverified(entry: &MediaAuditRecord, expected: impl Fn(&UnverifiedReason) -> bool) {
    assert_eq!(
        entry.status,
        MediaAuditStatus::LegacyUnverified,
        "{entry:?}"
    );
    assert!(
        entry.unverified.iter().any(expected),
        "expected reason missing: {:?}",
        entry.unverified
    );
}

// --- Verified --------------------------------------------------------------------------------

#[test]
fn an_identity_with_matching_provenance_is_verified() {
    let fixture = Fixture::new(&combat_identity(&catalog()));
    let report = fixture.audit(&fixture.required(), None);
    let entry = fixture.entry(&report);
    assert_eq!(entry.status, MediaAuditStatus::Verified, "{entry:?}");
    assert!(entry.unverified.is_empty());
    assert_eq!(entry.required_tier, Some(Tier::Required));
    assert!(report.strict_accepted);
    report.require_verified().expect("strict passes");
}

#[test]
fn a_provenance_file_round_trips_and_uses_unix_newlines() {
    let fixture = Fixture::new(&combat_identity(&catalog()));
    let path = provenance_path(&fixture.textures(), &fixture.slug).expect("valid slug");
    let bytes = std::fs::read(&path).expect("provenance");
    assert!(!bytes.contains(&b'\r'));
    let loaded = MediaProvenance::load(&path).expect("parses");
    assert_eq!(loaded.name, fixture.slug);
    assert_eq!(loaded.states.len(), STATES.len());
    assert!(matches!(
        loaded.states[&AnimState::Idle].origin,
        ClipOrigin::LegacyVeo { ref commit } if commit == LEGACY_COMMIT
    ));
}

// --- Each break is named ----------------------------------------------------------------------

#[test]
fn a_missing_provenance_file_is_named() {
    let fixture = Fixture::new(&combat_identity(&catalog()));
    std::fs::remove_file(provenance_path(&fixture.textures(), &fixture.slug).unwrap()).unwrap();
    assert_unverified(&fixture.audited(), |reason| {
        matches!(reason, UnverifiedReason::ProvenanceMissing)
    });
}

#[test]
fn a_changed_atlas_byte_is_named() {
    let mut fixture = Fixture::new(&combat_identity(&catalog()));
    fixture.frames[1] += 1;
    write_strip(&fixture.atlas_path(), &fixture.frames);
    assert_unverified(&fixture.audited(), |reason| {
        matches!(reason, UnverifiedReason::AtlasHashMismatch { .. })
    });
}

#[test]
fn a_changed_manifest_is_named() {
    let mut fixture = Fixture::new(&combat_identity(&catalog()));
    fixture.meta.fps = 30;
    fixture.meta.save_to_ron(&fixture.manifest_path()).unwrap();
    assert_unverified(&fixture.audited(), |reason| {
        matches!(reason, UnverifiedReason::ManifestHashMismatch { .. })
    });
}

#[test]
fn a_changed_clip_byte_is_named() {
    let fixture = Fixture::new(&combat_identity(&catalog()));
    let path = fixture.clip_path(AnimState::Walk);
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[0] ^= 1;
    std::fs::write(&path, bytes).unwrap();
    assert_unverified(&fixture.audited(), |reason| {
        matches!(
            reason,
            UnverifiedReason::ClipHashMismatch {
                state: AnimState::Walk,
                ..
            }
        )
    });
}

#[test]
fn a_missing_clip_is_named() {
    let fixture = Fixture::new(&combat_identity(&catalog()));
    std::fs::remove_file(fixture.clip_path(AnimState::Attack)).unwrap();
    assert_unverified(&fixture.audited(), |reason| {
        matches!(
            reason,
            UnverifiedReason::ClipUnreadable {
                state: AnimState::Attack,
                ..
            }
        )
    });
}

#[test]
fn a_shared_frame_range_is_named_even_when_sealed() {
    let mut fixture = Fixture::new(&combat_identity(&catalog()));
    fixture.meta.actions.insert(AnimState::Walk, slice(0, 1));
    fixture.write_atlas();
    fixture.seal();
    assert_unverified(&fixture.audited(), |reason| {
        matches!(reason, UnverifiedReason::SharedFrameRange { .. })
    });
}

#[test]
fn a_shared_clip_is_named_even_when_sealed() {
    let fixture = Fixture::new(&combat_identity(&catalog()));
    std::fs::copy(
        fixture.clip_path(AnimState::Idle),
        fixture.clip_path(AnimState::Walk),
    )
    .unwrap();
    fixture.seal();
    assert_unverified(&fixture.audited(), |reason| {
        matches!(reason, UnverifiedReason::SharedClip { .. })
    });
}

#[test]
fn a_published_action_without_its_own_clip_is_named() {
    let mut fixture = Fixture::new(&combat_identity(&catalog()));
    fixture.meta.actions.remove(&AnimState::Attack);
    fixture.write_atlas();
    fixture.seal();
    assert_unverified(&fixture.audited(), |reason| {
        matches!(
            reason,
            UnverifiedReason::PublishedActionMissing {
                state: AnimState::Attack
            }
        )
    });
}

#[test]
fn a_frame_range_that_copies_concept_frames_is_named() {
    let mut fixture = Fixture::new(&combat_identity(&catalog()));
    fixture.frames[0] = 200;
    write_strip(&fixture.atlas_path(), &fixture.frames);
    fixture.seal();
    assert_unverified(&fixture.audited(), |reason| {
        matches!(
            reason,
            UnverifiedReason::ConceptFrames {
                state: AnimState::Idle
            }
        )
    });
}

#[test]
fn an_atlas_identical_to_the_concept_atlas_stays_a_known_duplicate() {
    let fixture = Fixture::new(&combat_identity(&catalog()));
    std::fs::copy(
        fixture.textures().join("concept_video.png"),
        fixture.atlas_path(),
    )
    .unwrap();
    let mut meta = fixture.meta.clone();
    meta.total_frames = 1;
    meta.actions = BTreeMap::from([(AnimState::Idle, slice(0, 1))]);
    meta.save_to_ron(&fixture.manifest_path()).unwrap();
    let entry = fixture.audited();
    assert_eq!(entry.status, MediaAuditStatus::KnownConceptDuplicate);
}

#[test]
fn a_malformed_legacy_commit_is_named() {
    let mut fixture = Fixture::new(&combat_identity(&catalog()));
    fixture.origins.insert(
        AnimState::Idle,
        ClipOrigin::LegacyVeo {
            commit: "HEAD".into(),
        },
    );
    fixture.seal();
    assert_unverified(&fixture.audited(), |reason| {
        matches!(
            reason,
            UnverifiedReason::InvalidLegacyCommit {
                state: AnimState::Idle,
                ..
            }
        )
    });
}

// --- Veo origins and the ledger ---------------------------------------------------------------

fn veo_origin(line: usize) -> ClipOrigin {
    ClipOrigin::Veo {
        model: "veo-test-model".into(),
        operation: "operations/fixture-1".into(),
        prompt_sha256: sha256_hex(b"prompt"),
        keyframe_sha256: None,
        ledger_line: line,
    }
}

fn downloaded(fixture: &Fixture, state: AnimState, operation: &str) -> LedgerRecord {
    let bytes = std::fs::read(fixture.clip_path(state)).unwrap();
    LedgerRecord {
        time_utc: "2026-11-04T00:00:00Z".into(),
        unix_seconds: 1_793_750_400,
        run_id: "fixture-run".into(),
        event: LedgerEvent::Downloaded {
            slug: fixture.slug.clone(),
            state,
            operation: operation.into(),
            bytes: u64::try_from(bytes.len()).unwrap(),
            sha256: sha256_hex(&bytes),
            duration_seconds: 4.0,
            frames: 96,
            width: 1280,
            height: 720,
            path: "clips/fixture.mp4".into(),
        },
    }
}

fn write_ledger(dir: &Path, records: &[LedgerRecord]) -> Vec<NumberedRecord> {
    let path = dir.join("ledger.jsonl");
    let text: String = records
        .iter()
        .map(|record| serde_json::to_string(record).unwrap() + "\n")
        .collect();
    std::fs::write(&path, text).unwrap();
    read_ledger_numbered(&path).expect("ledger parses")
}

#[test]
fn a_veo_origin_with_its_downloaded_line_is_verified() {
    let mut fixture = Fixture::new(&combat_identity(&catalog()));
    fixture.origins.insert(AnimState::Walk, veo_origin(2));
    fixture.seal();
    let other = downloaded(&fixture, AnimState::Idle, "operations/other");
    let line = downloaded(&fixture, AnimState::Walk, "operations/fixture-1");
    let ledger = write_ledger(fixture.root.path(), &[other, line]);
    let entry = fixture.entry(&fixture.audit(&fixture.required(), Some(&ledger)));
    assert_eq!(entry.status, MediaAuditStatus::Verified, "{entry:?}");
}

#[test]
fn a_veo_origin_without_a_ledger_is_named() {
    let mut fixture = Fixture::new(&combat_identity(&catalog()));
    fixture.origins.insert(AnimState::Walk, veo_origin(1));
    fixture.seal();
    assert_unverified(&fixture.audited(), |reason| {
        matches!(
            reason,
            UnverifiedReason::LedgerNotSupplied {
                state: AnimState::Walk
            }
        )
    });
}

#[test]
fn a_missing_ledger_line_is_named() {
    let mut fixture = Fixture::new(&combat_identity(&catalog()));
    fixture.origins.insert(AnimState::Walk, veo_origin(5));
    fixture.seal();
    let line = downloaded(&fixture, AnimState::Walk, "operations/fixture-1");
    let ledger = write_ledger(fixture.root.path(), &[line]);
    let entry = fixture.entry(&fixture.audit(&fixture.required(), Some(&ledger)));
    assert_unverified(&entry, |reason| {
        matches!(
            reason,
            UnverifiedReason::LedgerLineMismatch {
                state: AnimState::Walk,
                line: 5,
                ..
            }
        )
    });
}

#[test]
fn a_ledger_line_for_another_clip_is_named() {
    let mut fixture = Fixture::new(&combat_identity(&catalog()));
    fixture.origins.insert(AnimState::Walk, veo_origin(1));
    fixture.seal();
    let wrong = downloaded(&fixture, AnimState::Idle, "operations/fixture-1");
    let ledger = write_ledger(fixture.root.path(), &[wrong]);
    let entry = fixture.entry(&fixture.audit(&fixture.required(), Some(&ledger)));
    assert_unverified(&entry, |reason| {
        matches!(
            reason,
            UnverifiedReason::LedgerLineMismatch {
                state: AnimState::Walk,
                line: 1,
                ..
            }
        )
    });
}

#[test]
fn a_clip_rejected_in_review_is_named() {
    let mut fixture = Fixture::new(&combat_identity(&catalog()));
    fixture.origins.insert(AnimState::Walk, veo_origin(1));
    fixture.seal();
    let line = downloaded(&fixture, AnimState::Walk, "operations/fixture-1");
    let LedgerEvent::Downloaded { sha256, .. } = &line.event else {
        unreachable!("built as a download")
    };
    let rejected = LedgerRecord {
        event: LedgerEvent::RejectedInReview {
            slug: fixture.slug.clone(),
            state: AnimState::Walk,
            sha256: sha256.clone(),
            reason: "morphs into another object".into(),
        },
        ..line.clone()
    };
    let ledger = write_ledger(fixture.root.path(), &[line, rejected]);
    let entry = fixture.entry(&fixture.audit(&fixture.required(), Some(&ledger)));
    assert_unverified(&entry, |reason| {
        matches!(
            reason,
            UnverifiedReason::RejectedInReview {
                state: AnimState::Walk
            }
        )
    });
}

// --- Strict audit over the required set -------------------------------------------------------

#[test]
fn strict_fails_naming_each_unverified_required_identity() {
    let catalog = catalog();
    let fixture = Fixture::new(&combat_identity(&catalog));
    let missing = other_identity(&catalog, &fixture.slug);
    let required = RequiredSet::new(vec![
        RequiredIdentity {
            slug: fixture.slug.clone(),
            tier: Tier::Required,
        },
        RequiredIdentity {
            slug: missing.clone(),
            tier: Tier::Expansion,
        },
    ]);
    let report = fixture.audit(&required, None);
    assert!(!report.strict_accepted);
    match report.require_verified() {
        Err(MediaAuditError::Unverified { identities }) => {
            assert_eq!(identities.len(), 1, "{identities:?}");
            assert!(identities[0].contains(&missing), "{identities:?}");
            assert!(identities[0].contains("MissingAsset"), "{identities:?}");
        }
        other => panic!("strict must fail naming {missing}: {other:?}"),
    }
}

#[test]
fn identities_outside_the_required_set_are_reported_not_failed() {
    let catalog = catalog();
    let fixture = Fixture::new(&combat_identity(&catalog));
    let report = fixture.audit(&fixture.required(), None);
    assert!(report.strict_accepted);
    assert!(report.entries.iter().any(
        |entry| entry.status == MediaAuditStatus::MissingAsset && entry.required_tier.is_none()
    ));
}

#[test]
fn a_required_set_is_validated_against_the_catalog() {
    let catalog = catalog();
    let slug = combat_identity(&catalog);
    let parse = |text: &str| RequiredSet::from_ron_str(text, &catalog);
    let entry = |slug: &str| format!("(slug: \"{slug}\", tier: Required)");

    let good = parse(&format!(
        "(format_version: 1, identities: [{}])",
        entry(&slug)
    ))
    .expect("a known slug parses");
    assert_eq!(good.identities.len(), 1);
    assert!(matches!(
        parse("(format_version: 1, identities: [])"),
        Err(ProvenanceError::RequiredSetInvalid { .. })
    ));
    assert!(matches!(
        parse(&format!(
            "(format_version: 1, identities: [{}])",
            entry("no_such_identity")
        )),
        Err(ProvenanceError::RequiredSetInvalid { .. })
    ));
    assert!(matches!(
        parse(&format!(
            "(format_version: 1, identities: [{}, {}])",
            entry(&slug),
            entry(&slug)
        )),
        Err(ProvenanceError::RequiredSetInvalid { .. })
    ));
    assert!(matches!(
        parse(&format!(
            "(format_version: 2, identities: [{}])",
            entry(&slug)
        )),
        Err(ProvenanceError::RequiredSetInvalid { .. })
    ));
}

// --- The baker writes provenance (real FFmpeg) ------------------------------------------------

/// Re-encodes the fixture clip through `filter`, so that each state gets its own real clip.
fn variant_clip(dir: &Path, name: &str, filter: &str) -> PathBuf {
    let output = dir.join(name);
    let status = std::process::Command::new("/usr/bin/ffmpeg")
        .args(["-v", "error", "-y", "-i"])
        .arg(common::fixture_clip())
        .args(["-vf", filter])
        .arg(&output)
        .status()
        .expect("ffmpeg runs");
    assert!(status.success(), "ffmpeg failed for {filter}");
    output
}

#[test]
fn a_bake_writes_provenance_that_the_audit_verifies() {
    let catalog = catalog();
    let slug = combat_identity(&catalog);
    let root = tempfile::tempdir().unwrap();
    let videos = root.path().join("videos");
    std::fs::create_dir_all(&videos).unwrap();
    let filters = ["null", "negate", "lutrgb=r=0"];
    let clips: Vec<BakeClip> = STATES
        .iter()
        .zip(filters)
        .map(|(state, filter)| BakeClip {
            state: *state,
            path: variant_clip(&videos, &format!("{slug}_{state:?}.mp4"), filter),
            origin: ClipOrigin::LegacyVeo {
                commit: LEGACY_COMMIT.into(),
            },
        })
        .collect();
    let textures = root.path().join("textures");
    let provenance =
        bake_with_provenance(&slug, &clips, &videos, &textures, 32, 5).expect("bake succeeds");
    assert_eq!(provenance.states.len(), 3);
    assert!(provenance.bake.ffmpeg_version.starts_with("ffmpeg version"));
    assert!(provenance_path(&textures, &slug).unwrap().is_file());

    let required = RequiredSet::new(vec![RequiredIdentity {
        slug: slug.clone(),
        tier: Tier::Required,
    }]);
    let report = audit_media(
        root.path(),
        &catalog,
        &AuditInputs {
            required: &required,
            ledger: None,
        },
    );
    let entry = report.entries.iter().find(|e| e.slug == slug).unwrap();
    assert_eq!(entry.status, MediaAuditStatus::Verified, "{entry:?}");
}

#[test]
fn a_bake_refuses_a_clip_outside_the_videos_directory() {
    let catalog = catalog();
    let slug = combat_identity(&catalog);
    let root = tempfile::tempdir().unwrap();
    let videos = root.path().join("videos");
    std::fs::create_dir_all(&videos).unwrap();
    let clips = [BakeClip {
        state: AnimState::Idle,
        path: common::fixture_clip(),
        origin: ClipOrigin::LegacyVeo {
            commit: LEGACY_COMMIT.into(),
        },
    }];
    let textures = root.path().join("textures");
    assert!(matches!(
        bake_with_provenance(&slug, &clips, &videos, &textures, 32, 5),
        Err(ProvenanceError::ClipOutsideVideos { .. })
    ));
    assert!(!textures.join(format!("{slug}.png")).exists());
}

// --- The shipped bundle -------------------------------------------------------------------

#[test]
fn the_shipped_required_set_names_catalog_identities() {
    let set = RequiredSet::load(
        &shipped_assets().join(asset_baker::provenance::REQUIRED_SET_FILE),
        &catalog(),
    )
    .expect("the shipped required set loads");
    assert!(set
        .identities
        .iter()
        .any(|identity| identity.tier == Tier::Required));
}

#[test]
fn every_shipped_provenance_file_verifies() {
    let catalog = catalog();
    let required = RequiredSet::load(
        &shipped_assets().join(asset_baker::provenance::REQUIRED_SET_FILE),
        &catalog,
    )
    .unwrap();
    let report = audit_media(
        &shipped_assets(),
        &catalog,
        &AuditInputs {
            required: &required,
            ledger: None,
        },
    );
    let textures = shipped_assets().join("textures");
    let with_provenance: Vec<&MediaAuditRecord> = report
        .entries
        .iter()
        .filter(|entry| provenance_path(&textures, &entry.slug).unwrap().is_file())
        .collect();
    assert!(!with_provenance.is_empty(), "WP2 re-bakes two identities");
    for entry in with_provenance {
        let only_missing_actions = entry
            .unverified
            .iter()
            .all(|reason| matches!(reason, UnverifiedReason::PublishedActionMissing { .. }));
        assert!(
            entry.status == MediaAuditStatus::Verified
                || (entry.status == MediaAuditStatus::LegacyUnverified && only_missing_actions),
            "{entry:?}"
        );
    }
    assert!(report
        .entries
        .iter()
        .any(|entry| entry.status == MediaAuditStatus::Verified));
}

#[test]
fn strict_cli_audit_fails_naming_the_unverified_required_identities() {
    let catalog = catalog();
    let required = RequiredSet::load(
        &shipped_assets().join(asset_baker::provenance::REQUIRED_SET_FILE),
        &catalog,
    )
    .unwrap();
    let out = tempfile::tempdir().unwrap();
    let output = baker()
        .args(["audit-media", "--strict", "--report"])
        .arg(out.path().join("audit.json"))
        .output()
        .expect("run baker");
    assert!(!output.status.success(), "{}", text(&output));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.path().join("audit.json")).unwrap()).unwrap();
    for identity in &required.identities {
        let status = report["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["slug"] == identity.slug.as_str())
            .map(|entry| entry["status"].as_str().unwrap().to_string())
            .unwrap();
        assert_eq!(
            stderr.contains(&identity.slug),
            status != "Verified",
            "{}: {status}\n{stderr}",
            identity.slug
        );
    }
}
