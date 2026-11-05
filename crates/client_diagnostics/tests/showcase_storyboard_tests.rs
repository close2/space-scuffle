//! The showcase storyboard, its generated cards and its scenario, through the public API.
//!
//! # Responsibility
//! Plan `docs/plans/2026-11-05-ci-showcase-video.md`, WP1, tests 1 to 7 (test 8, the follow spring,
//! is a unit test of `showcase::pose`):
//! - the shipped storyboard validates against the real `DESIGN.md`, catalog and media;
//! - every validation rule of decision D-4 reports its own fault, first fault first;
//! - the cards count and list what the documents say, and fail with typed errors when a section is
//!   missing;
//! - the showcase scenario fields only identities with admitted media, and the Player profile applies
//!   its attack-move orders to the viewer's own army (the plan's **verify** item of D-5).
//!
//! # Invariants
//! - No unit identity is written here: slugs come from the shipped storyboard and catalog (C5).
//! - Nothing under `assets/` is modified; mutations are applied to parsed copies.
//! - The Skirmish test runs the production client headless, without a renderer (C21).

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "test assertions report failures by panicking"
)]
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "map cells and tick counts are small, exact in f32 and u32"
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "../../client/tests/support/battle_harness.rs"]
mod battle_harness;

use client_diagnostics::showcase::cards::{RepositoryCards, SubsystemStatus};
use client_diagnostics::showcase::storyboard::{
    CameraMove, Length, Overlay, Speed, Storyboard, StoryboardFault, ValidationContext,
};
use client_diagnostics::showcase::{ShowcaseError, ValidatedStoryboard, DEFAULT_STORYBOARD};
use std::error::Error;
use std::path::{Path, PathBuf};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets() -> PathBuf {
    repository().join("assets")
}

#[test]
fn the_shipped_storyboard_loads_and_validates() -> Result<(), Box<dyn Error>> {
    let validated =
        ValidatedStoryboard::load(&assets(), &repository(), Path::new(DEFAULT_STORYBOARD))?;
    let storyboard = &validated.storyboard;
    assert_eq!(
        storyboard.frame_rate,
        u32::from(sim_types::TICKS_PER_SECOND)
    );
    assert_ne!(storyboard.takes.len(), 0);
    for take in &storyboard.takes {
        assert!(assets().join(&take.scenario).is_file(), "{}", take.name);
    }
    Ok(())
}

/// The real storyboard and the context it validates against.
struct Fixture {
    storyboard: Storyboard,
    cards: RepositoryCards,
    catalogs: sim_core::catalog::RuntimeCatalogs,
    textures: PathBuf,
}

impl Fixture {
    fn load() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            storyboard: Storyboard::load(&assets().join(DEFAULT_STORYBOARD))?,
            cards: RepositoryCards::load(&repository())?,
            catalogs: sim_core::catalog::RuntimeCatalogs::load_from_directory(
                assets().join("catalog"),
            )?,
            textures: assets().join("textures"),
        })
    }

    fn validate(&self, storyboard: &Storyboard) -> Result<(), StoryboardFault> {
        storyboard.validate(&ValidationContext {
            subsystems: &self.cards.subsystems,
            catalog: &self.catalogs.units,
            textures: &self.textures,
        })
    }

    /// A catalog unit whose atlas is not on disk.
    fn slug_without_media(&self) -> String {
        self.catalogs
            .units
            .archetypes()
            .map(|archetype| archetype.slug.clone())
            .find(|slug| !self.textures.join(format!("{slug}.png")).exists())
            .expect("the catalog has identities without media")
    }
}

/// Whether a validation result is the fault a case expects.
type Expectation = fn(&StoryboardFault) -> bool;

/// The index of the first scene with `predicate`.
fn scene_index(
    storyboard: &Storyboard,
    predicate: impl Fn(&CameraMove, &Overlay) -> bool,
) -> usize {
    storyboard.takes[0]
        .scenes
        .iter()
        .position(|scene| predicate(&scene.camera, &scene.overlay))
        .expect("the shipped storyboard has such a scene")
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one mutation per rule, listed in order"
)]
fn validation_reports_the_first_fault_in_order() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::load()?;
    fixture.validate(&fixture.storyboard)?;
    let base = fixture.storyboard.clone();
    let caption = scene_index(&base, |_, overlay| {
        matches!(overlay, Overlay::Caption { .. })
    });
    let follow_unit = scene_index(&base, |camera, _| {
        matches!(camera, CameraMove::FollowUnit { .. })
    });
    let first = base.takes[0].scenes[0].name.clone();
    let mut cases: Vec<(&str, Storyboard, Expectation)> = Vec::new();
    let mut mutate = |name, change: &dyn Fn(&mut Storyboard), expected| {
        let mut storyboard = base.clone();
        change(&mut storyboard);
        cases.push((name, storyboard, expected));
    };
    mutate("version", &|s| s.format_version = 2, |f| {
        matches!(f, StoryboardFault::Version(2))
    });
    mutate("odd resolution", &|s| s.resolution = (1281, 720), |f| {
        matches!(f, StoryboardFault::Resolution(1281, 720))
    });
    mutate("frame rate", &|s| s.frame_rate = 30, |f| {
        matches!(f, StoryboardFault::FrameRate(30))
    });
    mutate(
        "duplicate scene",
        &|s| {
            let copy = s.takes[0].scenes[0].clone();
            s.takes[0].scenes.push(copy);
        },
        |f| matches!(f, StoryboardFault::DuplicateName(_)),
    );
    mutate(
        "unsafe name",
        &|s| s.takes[0].scenes[0].name = "../title".into(),
        |f| matches!(f, StoryboardFault::UnsafeName(_)),
    );
    mutate(
        "length 0",
        &|s| s.takes[0].scenes[0].length = Length::Frames(0),
        |f| matches!(f, StoryboardFault::Length(_)),
    );
    mutate(
        "until combat order",
        &|s| {
            s.takes[0].scenes[0].length = Length::UntilCombat { min: 300, max: 200 };
        },
        |f| matches!(f, StoryboardFault::UntilCombatOrder(_)),
    );
    mutate(
        "total",
        &|s| {
            for scene in &mut s.takes[0].scenes {
                scene.length = Length::Frames(1_000);
            }
        },
        |f| matches!(f, StoryboardFault::TotalFrames(_)),
    );
    mutate(
        "distance",
        &|s| {
            s.takes[0].scenes[0].camera = CameraMove::Fixed {
                focus: (10.0, 10.0),
                distance: 60.0,
                pitch_deg: 50.0,
            };
        },
        |f| matches!(f, StoryboardFault::Distance(_)),
    );
    mutate(
        "pitch",
        &|s| {
            s.takes[0].scenes[0].camera = CameraMove::Fixed {
                focus: (10.0, 10.0),
                distance: 20.0,
                pitch_deg: 89.0,
            };
        },
        |f| matches!(f, StoryboardFault::Pitch(_)),
    );
    mutate(
        "NaN",
        &|s| {
            s.takes[0].scenes[0].camera = CameraMove::Fixed {
                focus: (f32::NAN, 10.0),
                distance: 20.0,
                pitch_deg: 50.0,
            };
        },
        |f| matches!(f, StoryboardFault::NotFinite(_)),
    );
    mutate(
        "teaser cut past the minimum",
        &|s| {
            s.teaser.cuts[0].frames = s.takes[0].scenes[0].length.min_frames() + 1;
        },
        |f| matches!(f, StoryboardFault::Cut(_)),
    );
    mutate(
        "speed without badge",
        &|s| {
            s.takes[0].scenes[0].speed = Speed::Speed(3);
            s.takes[0].scenes[0].badge = None;
        },
        |f| matches!(f, StoryboardFault::MissingBadge(_)),
    );
    let mut storyboard = base.clone();
    if let Overlay::Caption { subsystems, .. } = &mut storyboard.takes[0].scenes[caption].overlay {
        subsystems.push("A subsystem nobody built".into());
    }
    cases.push(("unknown subsystem", storyboard, |f| {
        matches!(f, StoryboardFault::UnknownSubsystem { .. })
    }));
    let removed = fixture
        .cards
        .subsystems
        .iter()
        .find(|row| row.status == SubsystemStatus::Removed)
        .expect("DESIGN.md lists a removed subsystem")
        .name
        .clone();
    let mut storyboard = base.clone();
    if let Overlay::Caption { subsystems, .. } = &mut storyboard.takes[0].scenes[caption].overlay {
        subsystems.push(removed);
    }
    cases.push(("removed subsystem", storyboard, |f| {
        matches!(
            f,
            StoryboardFault::HiddenSubsystem {
                status: SubsystemStatus::Removed,
                ..
            }
        )
    }));
    let slugs: [(&str, String, Expectation); 2] = [
        ("unknown slug", "no_such_identity".to_owned(), |f| {
            matches!(f, StoryboardFault::UnknownUnit { .. })
        }),
        ("slug without media", fixture.slug_without_media(), |f| {
            matches!(f, StoryboardFault::UnitWithoutMedia { .. })
        }),
    ];
    for (name, slug, expected) in slugs {
        let mut storyboard = base.clone();
        if let CameraMove::FollowUnit { unit_types, .. } =
            &mut storyboard.takes[0].scenes[follow_unit].camera
        {
            unit_types.insert(0, slug);
        }
        cases.push((name, storyboard, expected));
    }
    // A storyboard with two faults reports the one earlier in the order.
    let mut storyboard = base.clone();
    storyboard.frame_rate = 30;
    storyboard.takes[0].scenes[0].length = Length::Frames(0);
    cases.push(("first of two", storyboard, |f| {
        matches!(f, StoryboardFault::FrameRate(30))
    }));
    for (name, storyboard, expected) in &cases {
        let result = fixture.validate(storyboard);
        assert!(
            result.as_ref().err().is_some_and(expected),
            "{name}: {result:?} (first scene `{first}`)"
        );
    }
    Ok(())
}

const DESIGN_FIXTURE: &str = "# Title

## 1. What It Is

The *Game* is a `test` game: deterministic. Second sentence.

**Intent, not built:** networking and replays. More text.

## 4. Subsystem Status

| Subsystem | Status | What exists |
|---|---|---|
| Alpha | Working | a \\| b |
| Beta | Partial | `x | y` |
| Gamma | Not built | none |
| Delta | Working | yes |
| Epsilon | Removed | gone |
| Zeta | Partial | some |

### Next
";

const PLANS_FIXTURE: &str = "# Plans

| ID | File | Title | Status | Evidence |
|---|---|---|---|---|
| 54 | [54.md](54.md) | Numbered plan | Active | old |
| 2026-11-01-old | [a](a) | Older plan (detail) | Active | x |
| 2026-11-03-done | [b](b) | Finished plan | Done | x |
| 2026-11-04-mid | [c](c) | Middle plan: with a colon | Proposed | x |
| 2026-11-05-new | [d](d) | Newest `plan` with a title far longer than seventy characters in total length | Active | x |
| 2026-11-02-two | [e](e) | Second oldest | Proposed | x |
| 2026-10-30-five | [f](f) | Fifth | Active | x |
";

#[test]
fn status_card_counts_design_rows_and_lists_heading_plans() -> Result<(), Box<dyn Error>> {
    let cards = RepositoryCards::from_texts(
        Path::new("DESIGN.md"),
        DESIGN_FIXTURE,
        Path::new("README.md"),
        PLANS_FIXTURE,
    )?;
    assert_eq!(cards.title.pitch, "The Game is a test game: deterministic.");
    let status = &cards.status;
    assert_eq!(
        status.counts,
        vec![
            (SubsystemStatus::Working, 2),
            (SubsystemStatus::Partial, 2),
            (SubsystemStatus::NotBuilt, 1),
        ]
    );
    assert_eq!(
        status.counts_line(),
        "2 subsystems working, 2 partial, 1 not built"
    );
    assert_eq!(status.partial, vec!["Beta", "Zeta"]);
    assert_eq!(status.not_built, vec!["Gamma"]);
    assert_eq!(status.intent, "networking and replays.");
    assert_eq!(
        status.plans,
        vec![
            "Newest plan with a title far longer than seventy characters in total",
            "Middle plan",
            "Second oldest",
            "Older plan",
        ]
    );
    assert!(status.plans.iter().all(|plan| plan.chars().count() <= 70));
    assert_eq!(cards.subsystems.len(), 6);
    Ok(())
}

#[test]
fn cards_fail_with_typed_errors_on_missing_sections() {
    let without_one = DESIGN_FIXTURE.replace("## 1. What It Is", "## 9. Other");
    let without_table = DESIGN_FIXTURE.replace("| Subsystem | Status |", "| Name | State |");
    let malformed =
        DESIGN_FIXTURE.replace("| Delta | Working | yes |", "| Delta | Sometimes | yes |");
    let short_row = DESIGN_FIXTURE.replace("| Delta | Working | yes |", "| Delta |");
    for (name, design, needle) in [
        ("missing section 1", without_one.as_str(), "section 1"),
        ("missing table", without_table.as_str(), "Subsystem, Status"),
        ("unknown status", malformed.as_str(), "Sometimes"),
        ("short row", short_row.as_str(), "malformed table row"),
    ] {
        let result = RepositoryCards::from_texts(
            Path::new("DESIGN.md"),
            design,
            Path::new("README.md"),
            PLANS_FIXTURE,
        );
        match result {
            Err(ShowcaseError::Cards {
                source_file,
                reason,
            }) => {
                assert_eq!(source_file, Path::new("DESIGN.md"), "{name}");
                assert!(reason.contains(needle), "{name}: {reason}");
            }
            other => panic!("{name}: {other:?}"),
        }
    }
    let result = RepositoryCards::from_texts(
        Path::new("DESIGN.md"),
        DESIGN_FIXTURE,
        Path::new("README.md"),
        "no table here",
    );
    assert!(
        matches!(&result, Err(ShowcaseError::Cards { source_file, .. }) if source_file == Path::new("README.md")),
        "{result:?}"
    );
}

#[test]
fn status_card_from_the_real_repository_is_non_empty() -> Result<(), Box<dyn Error>> {
    let cards = RepositoryCards::load(&repository())?;
    assert!(
        cards.title.pitch.starts_with("Space Scuffle is"),
        "{}",
        cards.title.pitch
    );
    assert!(cards.title.pitch.ends_with('.'));
    assert_ne!(cards.status.counts.len(), 0);
    assert_ne!(cards.status.counts_line(), "");
    assert_ne!(cards.status.intent, "");
    assert!(cards.status.plans.len() <= 4);
    Ok(())
}

/// The scenario of every take of the shipped storyboard.
fn showcase_scenarios() -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let storyboard = Storyboard::load(&assets().join(DEFAULT_STORYBOARD))?;
    Ok(storyboard
        .takes
        .iter()
        .map(|take| take.scenario.clone())
        .collect())
}

#[test]
fn the_showcase_scenario_fields_only_media_backed_identities() -> Result<(), Box<dyn Error>> {
    let textures = assets().join("textures");
    for scenario in showcase_scenarios()? {
        let definition = client::runtime_battle::BattleDefinition::load(&assets(), &scenario)?;
        let world = definition.instantiate()?;
        let mut slugs = std::collections::BTreeSet::new();
        for entity in world.entities.values() {
            slugs.insert(world.catalog.get(entity.unit_type_id)?.slug.clone());
        }
        assert!(slugs.len() >= 2, "{}: {slugs:?}", scenario.display());
        for slug in &slugs {
            let media = asset_baker::media::load_unit_media(&textures, slug, None)
                .map_err(|error| format!("{}: {slug}: {error}", scenario.display()))?;
            assert_eq!(&media.metadata.name, slug);
        }
    }
    Ok(())
}

#[test]
fn skirmish_applies_the_scenario_initial_orders_to_the_viewer() -> Result<(), Box<dyn Error>> {
    const TICKS: usize = 100;
    for scenario in showcase_scenarios()? {
        let scenario = scenario.to_string_lossy().into_owned();
        let mut harness =
            battle_harness::Harness::boot_scenario(client::ClientMode::Skirmish, &scenario);
        let target = {
            let definition = harness
                .app
                .world()
                .resource::<client::runtime_battle::BattleDefinition>();
            let start = definition
                .document
                .start_locations
                .iter()
                .find(|start| start.player_slot == 1)
                .ok_or("slot 1 has a start location")?;
            bevy::math::Vec2::new(start.cell.0 as f32, start.cell.1 as f32)
        };
        let army = harness.local_army();
        assert!(!army.is_empty(), "{scenario}: the viewer has an army");
        let mean_distance = |harness: &battle_harness::Harness| {
            army.iter()
                .filter(|id| harness.world().entities.contains_key(id))
                .map(|id| harness.simulated_position(*id).distance(target))
                .sum::<f32>()
                / army.len() as f32
        };
        let before = mean_distance(&harness);
        let tick = harness.world().current_tick;
        harness.resume();
        harness.step(TICKS);
        assert!(harness.world().current_tick >= tick + TICKS as u32 - 2);
        let after = mean_distance(&harness);
        assert!(
            after + 1.0 < before,
            "{scenario}: the viewer's army did not advance on slot 1 ({before} -> {after})"
        );
    }
    Ok(())
}
