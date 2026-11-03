//! One owner per concern, and the client orderings that no schedule cycle would catch.
//!
//! # Responsibility
//! Proves the Phase 6 exit criterion "one owner per concern" (remediation plan §5) on the production
//! client, and pins the input-ownership ordering the Phase 9 blueprint (§9, finding of WP6) found
//! unpinned:
//! - `every_concern_has_its_declared_writers` boots the real client on the shipped skirmish, reads
//!   the access Bevy records for every system of every main-world schedule (`First` to `Last`, and
//!   also `Startup`, the fixed-step and scene schedules, so a writer cannot hide in one of them),
//!   and compares the
//!   systems that can write each concern with the table [`CONCERNS`]. A new writer fails the test
//!   with its name, so it must be argued in review and added to the table on purpose.
//! - `input_ownership_is_sampled_after_the_inspector_draws` proves from the declared constraints
//!   that `sample_input_ownership` runs after `lab_ui_system`, so the ownership the camera and
//!   selection read is the one of the UI pass just drawn.
//! - `the_order_model_follows_sets_and_ignores_membership` checks the helper in
//!   `support/schedule_order.rs` on a small schedule whose answers are known.
//!
//! # How it fits
//! Bevy enforces each system's access (`System::component_access`): a system without write access to
//! a resource or component cannot change it outside deferred commands. The table is therefore the
//! documentation a reviewer reads to learn who owns what, checked against what Bevy enforces. The
//! editor's matching ordering test lives in `editor_app_tests.rs`, beside the editor harness.
//!
//! # Limits
//! - Deferred `Commands` writes (inserting a resource, spawning or inserting a component) are not
//!   part of a system's access and are not seen here.
//! - Exclusive systems (`&mut World`, including Bevy's `apply_deferred` sync points) record no
//!   access in `bevy_ecs` 0.15, so they could write anything. They are not attributed to a concern;
//!   instead the set of exclusive systems is itself pinned in [`EXCLUSIVE_SYSTEMS`], so a new one
//!   fails the test just as a new writer does.
//! - Access is what a system *may* write. A system that takes `ResMut` but rarely writes is still
//!   listed; a wide access is itself worth a reviewer's look.
//! - The client is booted in `ClientMode::Skirmish`. Every mode registers the same systems; only
//!   the inserted resources differ.
//!
//! # Invariants
//! - Each writer is named `"<schedule>: <full system path>"`, the path Bevy reports.
//! - Tables are sorted, so a failure message shows a plain set difference.

#![allow(clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use bevy::ecs::component::{ComponentId, Components};
use bevy::ecs::schedule::{InternedScheduleLabel, Schedules};
use bevy::prelude::*;
use client::battle_hud::model::HudModel;
use client::battle_pipeline::fog::BattleFog;
use client::battle_pipeline::session::BattleSession;
use client::battle_selection::BattleSelection;
use client::bubble_shader::BubbleDomeMaterial;
use client::input_policy::InputOwnership;
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::smog::fluid::SmogGrid3d;
use client::smog::SmogGpuFrame;
use client::terrain::TerrainChunks;
use client::{ClientMode, RtsCamera};

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/battle_harness.rs"]
mod battle_harness;

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/schedule_order.rs"]
mod schedule_order;

/// What a concern is stored in: a resource or a component, found by its registered id.
enum Store {
    /// A resource; the function returns its id once the App has registered it.
    Resource(fn(&Components) -> Option<ComponentId>),
    /// A component; the function returns its id once the App has registered it.
    Component(fn(&Components) -> Option<ComponentId>),
}

/// One concern of the client, the data that holds it and the systems allowed to write it.
struct Concern {
    /// Human name of the concern, as the remediation plan uses it.
    name: &'static str,
    /// Where the concern is stored.
    store: Store,
    /// Every system that may write it, as `"<schedule>: <full system path>"`, sorted.
    writers: &'static [&'static str],
}

/// The declared owners of each concern.
///
/// A row lists Bevy's own systems too (`track_assets`, `asset_events`, `animate_targets`), because
/// they hold write access that the client cannot remove; they keep asset bookkeeping and animation
/// targets, not the concern's content.
const CONCERNS: &[Concern] = &[
    Concern {
        name: "Authoritative world",
        store: Store::Resource(Components::resource_id::<BattleSession>),
        writers: &[
            // Commits the scenario and the session's world.
            "Update: client::runtime_battle::scene::manage_battle_scene",
            // Advances the committed world.
            "Update: client::battle_pipeline::driver::advance_battle_ticks",
            // Drains the session's combat events for presentation.
            "Update: client::battle_pipeline::driver::publish_combat_events",
            // The command commit systems: each queues the player's orders on the session.
            "Update: client::battle_hud::actions::apply_hud_intent",
            "Update: client::battle_hud::actions::collect_card_keys",
            "Update: client::battle_input::collect_key_intent",
            "Update: client::battle_input::collect_pointer_intent",
            // The inspector's battle controls (restart, step) act on the session.
            "Update: client::lab_ui::lab_ui_system",
        ],
    },
    Concern {
        name: "Selection",
        store: Store::Resource(Components::resource_id::<BattleSelection>),
        writers: &[
            // Fog removes units that have left the player's sight.
            "Update: client::battle_hud::fog::apply_battle_fog",
            // The battle input systems that select.
            "Update: client::battle_input::collect_key_intent",
            "Update: client::battle_input::collect_pointer_intent",
            // Reconciles units that have left the battle.
            "Update: client::scene_controller::sync_live_battle_system",
        ],
    },
    Concern {
        name: "Bubble materials",
        store: Store::Resource(Components::resource_id::<Assets<BubbleDomeMaterial>>),
        writers: &[
            // Bevy's asset bookkeeping.
            "Last: bevy_asset::assets::Assets<client::bubble_shader::BubbleDomeMaterial>::asset_events",
            // Optics lanes.
            "PostUpdate: client::bubble_optics::sync_optics",
            // Bevy's asset bookkeeping.
            "PreUpdate: bevy_asset::assets::Assets<client::bubble_shader::BubbleDomeMaterial>::track_assets",
            // Spawn path: the smog's bouncing bubbles get their materials at startup.
            "Startup: client::smog::setup_smog_volume_system",
            // Snapshot lanes, one system per scene.
            "Update: client::scene_controller::sync_lab_preview_system",
            "Update: client::scene_controller::sync_live_battle_system",
            "Update: client::scene_controller::sync_swarm_stress_system",
        ],
    },
    Concern {
        name: "Smog field",
        store: Store::Resource(Components::resource_id::<SmogGrid3d>),
        writers: &[
            // Sizes the field and seeds its bubbles at startup.
            "Startup: client::smog::setup_smog_volume_system",
            "Update: client::smog::update_smog_field_system",
        ],
    },
    Concern {
        name: "Smog GPU frame",
        store: Store::Resource(Components::resource_id::<SmogGpuFrame>),
        writers: &["Update: client::smog::update_smog_field_system"],
    },
    Concern {
        name: "Fog",
        store: Store::Resource(Components::resource_id::<BattleFog>),
        writers: &["Update: client::battle_hud::fog::refresh_battle_fog"],
    },
    Concern {
        name: "HUD model",
        store: Store::Resource(Components::resource_id::<HudModel>),
        writers: &[
            // The card's keys move the card page held in the model.
            "Update: client::battle_hud::actions::collect_card_keys",
            "Update: client::battle_hud::model::refresh_hud_focus",
            "Update: client::battle_hud::model::refresh_hud_model",
        ],
    },
    Concern {
        name: "Camera",
        store: Store::Component(Components::component_id::<RtsCamera>),
        writers: &[
            // Bevy's animation system may write any component of an animated entity.
            "PostUpdate: bevy_animation::animate_targets",
            "Update: client::camera::camera_control_system",
            // Frames a newly committed battle.
            "Update: client::runtime_battle::scene::manage_battle_scene",
            "Update: client::runtime_battle::scene::sync_scene_camera",
            // Frames the swarm when the swarm scene opens.
            "Update: client::scene_controller::sync_swarm_stress_system",
        ],
    },
    Concern {
        name: "Input ownership",
        store: Store::Resource(Components::resource_id::<InputOwnership>),
        writers: &["Update: client::input_policy::sample_input_ownership"],
    },
    Concern {
        name: "Terrain",
        store: Store::Resource(Components::resource_id::<TerrainChunks>),
        writers: &["Update: client::terrain::update_terrain_system"],
    },
];

/// Every exclusive system of the main world, as `"<schedule>: <full system path>"`, sorted.
///
/// These record no access, so [`CONCERNS`] cannot attribute their writes; pinning the list makes
/// a new exclusive system as visible as a new writer.
///
/// All of them are Bevy's: the schedule runners, event and asset bookkeeping, the scene spawner and
/// the `apply_deferred` sync points that apply `Commands`. The client registers no exclusive system
/// of its own; its explicit `apply_deferred` calls appear here under Bevy's path.
const EXCLUSIVE_SYSTEMS: &[&str] = &[
    "First: bevy_ecs::event::update::event_update_system",
    "FixedMain: bevy_app::main_schedule::FixedMain::run_fixed_main",
    "Main: bevy_app::main_schedule::Main::run_main",
    "PostUpdate: bevy_ecs::schedule::executor::apply_deferred",
    "PreStartup: bevy_ecs::schedule::executor::apply_deferred",
    "PreUpdate: bevy_asset::server::handle_internal_asset_events",
    "PreUpdate: bevy_ecs::schedule::executor::apply_deferred",
    "RunFixedMainLoop: bevy_time::fixed::run_fixed_main_schedule",
    "SpawnScene: bevy_ecs::schedule::executor::apply_deferred",
    "SpawnScene: bevy_scene::scene_spawner::scene_spawner_system",
    "Startup: bevy_ecs::schedule::executor::apply_deferred",
    "Update: bevy_ecs::schedule::executor::apply_deferred",
];

/// The systems of every main-world schedule, by `"<schedule>: <full system path>"`, with their
/// recorded access. Exclusive systems are returned separately.
struct Inventory {
    /// Non-exclusive systems and the access Bevy enforces for each.
    systems: Vec<(String, bevy::ecs::query::Access<ComponentId>)>,
    /// Exclusive systems, which record no access.
    exclusive: BTreeSet<String>,
}

/// Initialises every schedule of the main world and reads each system's access.
fn inventory(app: &mut App) -> Inventory {
    let mut inventory = Inventory {
        systems: Vec::new(),
        exclusive: BTreeSet::new(),
    };
    let world = app.world_mut();
    let labels: Vec<InternedScheduleLabel> = world
        .resource::<Schedules>()
        .iter()
        .map(|(_, schedule)| schedule.label())
        .collect();
    for label in labels {
        // A schedule is taken out of `Schedules` while it initialises, because initialising
        // reads `Schedules` itself.
        world.schedule_scope(label, |world, schedule| {
            schedule
                .initialize(world)
                .expect("invariant: the production schedules built when the App ran");
            for (_, system) in schedule
                .systems()
                .expect("invariant: initialized on the line above")
            {
                let name = format!("{label:?}: {}", system.name());
                if system.is_exclusive() {
                    inventory.exclusive.insert(name);
                } else {
                    inventory
                        .systems
                        .push((name, system.component_access().clone()));
                }
            }
        });
    }
    inventory
}

/// The systems in `inventory` that may write `store`.
fn writers_of(app: &App, inventory: &Inventory, store: &Store) -> BTreeSet<String> {
    let components = app.world().components();
    let (id, is_resource) = match store {
        Store::Resource(id) => (id(components), true),
        Store::Component(id) => (id(components), false),
    };
    let id = id.expect("every concern's type is registered by the production client");
    inventory
        .systems
        .iter()
        .filter(|(_, access)| {
            if is_resource {
                access.has_resource_write(id)
            } else {
                access.has_component_write(id)
            }
        })
        .map(|(name, _)| name.clone())
        .collect()
}

/// The production client on the shipped skirmish, as `client --skirmish` boots it.
fn skirmish() -> battle_harness::Harness {
    battle_harness::Harness::boot_scenario(ClientMode::Skirmish, SKIRMISH_SCENARIO)
}

/// Every concern is writable by exactly its declared systems, and no undeclared exclusive system
/// exists.
#[test]
fn every_concern_has_its_declared_writers() {
    let mut harness = skirmish();
    let inventory = inventory(&mut harness.app);
    let mut mismatches = BTreeMap::new();
    for concern in CONCERNS {
        let found = writers_of(&harness.app, &inventory, &concern.store);
        let declared: BTreeSet<String> = concern.writers.iter().map(|s| (*s).to_owned()).collect();
        if found != declared {
            let undeclared: Vec<_> = found.difference(&declared).cloned().collect();
            let missing: Vec<_> = declared.difference(&found).cloned().collect();
            mismatches.insert(concern.name, (undeclared, missing));
        }
    }
    let declared_exclusive: BTreeSet<String> =
        EXCLUSIVE_SYSTEMS.iter().map(|s| (*s).to_owned()).collect();
    let undeclared_exclusive: Vec<_> = inventory
        .exclusive
        .difference(&declared_exclusive)
        .cloned()
        .collect();
    let missing_exclusive: Vec<_> = declared_exclusive
        .difference(&inventory.exclusive)
        .cloned()
        .collect();
    assert!(
        mismatches.is_empty() && undeclared_exclusive.is_empty() && missing_exclusive.is_empty(),
        "writers differ from the table (concern: (undeclared writers, declared but absent)): \
         {mismatches:#?}\nundeclared exclusive systems: {undeclared_exclusive:#?}\n\
         declared exclusive systems not found: {missing_exclusive:#?}"
    );
}

/// The camera and selection read the ownership of the UI pass just drawn, not the previous one.
#[test]
fn input_ownership_is_sampled_after_the_inspector_draws() {
    let harness = skirmish();
    let schedules = harness.app.world().resource::<Schedules>();
    let update = schedules
        .get(Update)
        .expect("invariant: every Bevy App has an Update schedule");
    assert!(
        schedule_order::runs_before(
            update,
            "client::lab_ui::lab_ui_system",
            "client::input_policy::sample_input_ownership",
        ),
        "sample_input_ownership must be ordered after lab_ui_system"
    );
}

/// Stages of the toy schedule in [`the_order_model_follows_sets_and_ignores_membership`].
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
enum Stage {
    /// Runs first.
    Early,
    /// Holds nothing; it only passes order through.
    Empty,
    /// Runs last.
    Late,
}

fn early_a() {}
fn early_b() {}
fn late() {}
fn after_early_a() {}
fn loose() {}

/// The order model answers as Bevy's own flattening would on a schedule whose answers are known:
/// order passes through chained sets (even an empty one) and explicit edges, and never through
/// membership of one set alone.
#[test]
fn the_order_model_follows_sets_and_ignores_membership() {
    let mut world = World::new();
    let mut schedule = Schedule::new(Update);
    schedule
        .configure_sets((Stage::Early, Stage::Empty, Stage::Late).chain())
        .add_systems((early_a, early_b).in_set(Stage::Early))
        .add_systems(late.in_set(Stage::Late))
        .add_systems(after_early_a.after(early_a))
        .add_systems(loose);
    schedule
        .initialize(&mut world)
        .expect("invariant: the toy schedule has no cycle");
    let path = |name: &str| format!("system_ownership_tests::{name}");
    let before = |first: &str, second: &str| {
        schedule_order::runs_before(&schedule, &path(first), &path(second))
    };
    assert!(
        before("early_a", "late"),
        "chained sets order their members"
    );
    assert!(
        before("early_b", "late"),
        "chained sets order their members"
    );
    assert!(
        before("early_a", "after_early_a"),
        "an explicit edge orders"
    );
    assert!(!before("late", "early_a"), "order is directed");
    assert!(
        !before("early_a", "early_b"),
        "one set does not order its members"
    );
    assert!(
        !before("early_b", "after_early_a"),
        "an edge orders only its own ends"
    );
    assert!(
        !before("loose", "late") && !before("late", "loose"),
        "unordered stays unordered"
    );
}
