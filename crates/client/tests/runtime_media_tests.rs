//! Actual startup registry and production scene admission without replacement media.
//!
//! These CPU/ECS tests load real own-file PNGs and deliberately damaged copies of
//! metadata in temporary bundles. They verify typed failure propagation, not shader
//! compilation or artistic quality; native capture separately covers those boundaries.

use asset_baker::{media::MediaLoadError, media_audit::MediaAuditStatus, FlipbookAtlasMeta};
use bevy::prelude::*;
use client::{
    media::{ClientMediaError, ClientMediaStatus, MediaFailurePolicy},
    runtime_battle::{BattleDefinition, DEFAULT_BATTLE_SCENARIO},
    BubbleDomeMaterial, FlipbookRegistry, LabState, LabTab,
};
use sim_core::UnitCatalog;
use sim_types::{AnimState, ShapeKind, UnitTypeId};
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn std::error::Error>>;
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

fn bundle() -> Result<BattleDefinition, Box<dyn std::error::Error>> {
    Ok(BattleDefinition::load(
        &root(),
        DEFAULT_BATTLE_SCENARIO.as_ref(),
    )?)
}

fn temporary_root() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    std::fs::create_dir(directory.path().join("shaders"))?;
    std::fs::create_dir(directory.path().join("textures"))?;
    std::fs::copy(
        root().join("shaders/bubble_dome.wgsl"),
        directory.path().join("shaders/bubble_dome.wgsl"),
    )?;
    Ok(directory)
}

fn copy_media(directory: &Path, slug: &str, image: bool) -> TestResult {
    for suffix in if image {
        vec![".png", ".flipbook.ron"]
    } else {
        vec![".flipbook.ron"]
    } {
        std::fs::copy(
            root().join("textures").join(format!("{slug}{suffix}")),
            directory.join("textures").join(format!("{slug}{suffix}")),
        )?;
    }
    Ok(())
}

fn app(
    directory: &Path,
    catalog: UnitCatalog,
    lab: LabState,
    policy: MediaFailurePolicy,
) -> Result<App, Box<dyn std::error::Error>> {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_event::<AppExit>()
        .add_event::<client::vfx::CombatVisualEvent>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<Image>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<Assets<BubbleDomeMaterial>>()
        .init_resource::<Assets<client::SelectionRingMaterial>>()
        .init_resource::<client::RenderEntityMap>()
        .init_resource::<client::selection::SelectionState>()
        .init_resource::<client::battle_selection::BattleSelection>()
        .insert_resource(client::ResolvedAssetsRoot::from_directory(directory)?)
        .insert_resource(client::CatalogResource(catalog))
        .insert_resource(policy)
        .insert_resource(lab)
        .add_systems(
            Startup,
            (
                client::setup_bubble_assets_system,
                client::setup_indicator_assets_system,
            ),
        );
    Ok(app)
}

fn has_error_exit(app: &App) -> bool {
    app.world()
        .resource::<Events<AppExit>>()
        .iter_current_update_events()
        .any(|event| matches!(event, AppExit::Error(_)))
}

#[test]
fn exact_registry_preserves_unavailable_ids_and_rejects_changed_bindings() -> TestResult {
    let definition = bundle()?;
    let archetype = definition
        .catalogs
        .units
        .archetypes()
        .find(|arch| arch.showcase_hero)
        .ok_or("showcase media")?;
    let (id, slug) = (archetype.unit_type_id, archetype.slug.clone());
    let directory = temporary_root()?;
    copy_media(directory.path(), &slug, true)?;
    let mut app = app(
        directory.path(),
        definition.catalogs.units.clone(),
        LabState::default(),
        MediaFailurePolicy::default(),
    )?;
    app.update();
    let status = app.world().resource::<ClientMediaStatus>();
    let registry = app.world().resource::<FlipbookRegistry>();
    status.require(registry, id, AnimState::Idle)?;
    assert_eq!(registry.get_meta(id).ok_or("exact metadata")?.name, slug);
    assert_eq!(registry.textures_by_id.len(), 1);
    let absent = definition
        .catalogs
        .units
        .archetypes()
        .find(|arch| arch.unit_type_id != id)
        .ok_or("other catalog entry")?
        .unit_type_id;
    assert!(registry.get_texture(absent).is_none());
    assert!(registry.get_meta(absent).is_none());
    assert_eq!(
        status.audit(absent).ok_or("missing status")?.status,
        MediaAuditStatus::MissingAsset
    );
    assert!(
        status.failure().is_none(),
        "unrequested missing files are inspectable, not silently rendered"
    );
    app.world_mut()
        .resource_mut::<FlipbookRegistry>()
        .textures_by_id
        .insert(id, Handle::default());
    assert!(matches!(
        app.world().resource::<ClientMediaStatus>().require(
            app.world().resource::<FlipbookRegistry>(),
            id,
            AnimState::Idle
        ),
        Err(ClientMediaError::BindingChanged { .. })
    ));
    Ok(())
}

/// The shipped registry, as the running client builds it, with its catalog.
fn shipped_media() -> Result<(App, UnitCatalog), Box<dyn std::error::Error>> {
    let catalog = bundle()?.catalogs.units;
    let mut app = app(
        &root(),
        catalog.clone(),
        LabState::default(),
        MediaFailurePolicy::default(),
    )?;
    app.update();
    Ok((app, catalog))
}

fn presentable(app: &App, unit_type: UnitTypeId) -> Result<(), ClientMediaError> {
    app.world()
        .resource::<ClientMediaStatus>()
        .presentable(app.world().resource::<FlipbookRegistry>(), unit_type)
}

#[test]
fn presentable_accepts_a_shipped_identity() -> TestResult {
    let (app, catalog) = shipped_media()?;
    let shipped = catalog
        .archetypes()
        .find(|arch| {
            root()
                .join("textures")
                .join(format!("{}.flipbook.ron", arch.slug))
                .exists()
        })
        .ok_or("the catalog must have an identity with shipped media")?;
    presentable(&app, shipped.unit_type_id)?;
    Ok(())
}

#[test]
fn presentable_refuses_an_identity_without_media() -> TestResult {
    let (app, catalog) = shipped_media()?;
    let missing = catalog
        .archetypes()
        .find(|arch| {
            !root()
                .join("textures")
                .join(format!("{}.flipbook.ron", arch.slug))
                .exists()
        })
        .ok_or("the catalog must have an identity without media")?;
    let Err(error) = presentable(&app, missing.unit_type_id) else {
        return Err("no media was shipped for it".into());
    };
    assert!(
        matches!(error, ClientMediaError::Unavailable { ref slug, .. } if *slug == missing.slug)
    );
    assert!(matches!(
        presentable(&app, UnitTypeId(u16::MAX)),
        Err(ClientMediaError::UnknownIdentity { .. })
    ));
    Ok(())
}

/// Boots the media registry on a temporary root holding `slug`'s real files, with `strip`
/// removed from its metadata. The directory must outlive the app.
fn media_without(
    catalog: &UnitCatalog,
    slug: &str,
    strip: AnimState,
) -> Result<(tempfile::TempDir, App), Box<dyn std::error::Error>> {
    let directory = temporary_root()?;
    copy_media(directory.path(), slug, true)?;
    let metadata_path = directory
        .path()
        .join("textures")
        .join(format!("{slug}.flipbook.ron"));
    let mut metadata: FlipbookAtlasMeta = ron::from_str(&std::fs::read_to_string(&metadata_path)?)?;
    assert!(
        metadata.actions.remove(&strip).is_some(),
        "{slug}'s shipped media authors {strip:?}"
    );
    std::fs::write(&metadata_path, ron::to_string(&metadata)?)?;
    let mut app = app(
        directory.path(),
        catalog.clone(),
        LabState::default(),
        MediaFailurePolicy::default(),
    )?;
    app.update();
    Ok((directory, app))
}

/// Whether `slug` ships its media metadata.
fn ships_media(slug: &str) -> bool {
    root()
        .join("textures")
        .join(format!("{slug}.flipbook.ron"))
        .exists()
}

#[test]
fn presentable_refuses_an_identity_missing_a_single_action() -> TestResult {
    let definition = bundle()?;
    let catalog = &definition.catalogs.units;
    // The rule names an action this identity can publish, so its absence must refuse it.
    let (archetype, strip) = catalog
        .archetypes()
        .filter(|arch| ships_media(&arch.slug))
        .find_map(|arch| {
            catalog
                .publishable_actions(arch)
                .iter()
                .find(|&action| action != AnimState::Idle)
                .map(|action| (arch, action))
        })
        .ok_or("an identity with shipped media that publishes more than Idle")?;
    let id = archetype.unit_type_id;
    let (_directory, app) = media_without(catalog, &archetype.slug, strip)?;

    let status = app.world().resource::<ClientMediaStatus>();
    let registry = app.world().resource::<FlipbookRegistry>();
    status.require(registry, id, AnimState::Idle)?;
    let Err(error) = status.presentable(registry, id) else {
        return Err("a snapshot may publish the missing action".into());
    };
    assert!(
        matches!(error, ClientMediaError::Unavailable { ref source, .. }
            if matches!(source.as_ref(), MediaLoadError::MissingAction { action, .. } if *action == strip))
    );
    Ok(())
}

/// D-9: media lacking an action the simulation never publishes for that identity, such as a
/// walk for a structure, must not keep the identity off the card. Since Veo batch 2 every
/// shipped structure atlas holds exactly its published actions (Idle and Constructing), so
/// the structure's real files are booted unchanged; the test checks that they lack Walk.
#[test]
fn presentable_ignores_an_action_the_simulation_never_publishes() -> TestResult {
    let definition = bundle()?;
    let catalog = &definition.catalogs.units;
    let structure = catalog
        .archetypes()
        .find(|arch| {
            matches!(arch.shape, ShapeKind::BuildingDome { .. }) && ships_media(&arch.slug)
        })
        .ok_or("a structure with shipped media")?;
    assert!(
        !catalog
            .publishable_actions(structure)
            .contains(AnimState::Walk),
        "the simulation never walks {}",
        structure.slug
    );
    let directory = temporary_root()?;
    copy_media(directory.path(), &structure.slug, true)?;
    let metadata: FlipbookAtlasMeta = ron::from_str(&std::fs::read_to_string(
        directory
            .path()
            .join("textures")
            .join(format!("{}.flipbook.ron", structure.slug)),
    )?)?;
    assert!(
        !metadata.actions.contains_key(&AnimState::Walk),
        "{}'s shipped media authors no Walk",
        structure.slug
    );
    let mut app = app(
        directory.path(),
        catalog.clone(),
        LabState::default(),
        MediaFailurePolicy::default(),
    )?;
    app.update();

    let status = app.world().resource::<ClientMediaStatus>();
    let registry = app.world().resource::<FlipbookRegistry>();
    assert!(status
        .require(registry, structure.unit_type_id, AnimState::Walk)
        .is_err());
    status.presentable(registry, structure.unit_type_id)?;
    Ok(())
}

#[test]
fn normal_live_game_retains_missing_png_error_and_later_close_fails() -> TestResult {
    let definition = bundle()?;
    let mut simulation = definition.instantiate()?;
    let entity = simulation
        .entities
        .values()
        .next()
        .ok_or("real scenario entity")?;
    let id = entity.unit_type_id;
    let slug = definition.catalogs.units.get(id)?.slug.clone();
    let directory = temporary_root()?;
    copy_media(directory.path(), &slug, false)?;
    let policy = MediaFailurePolicy::default();
    let mut session = client::battle_pipeline::session::BattleSession::prepare(
        client::battle_pipeline::commands::SessionGeneration::FIRST,
        &mut simulation,
        client::battle_pipeline::session::CommittedBattleMetadata::from_definition(&definition)?,
    )?;
    session
        .clock
        .observe(std::time::Duration::from_millis(80), 1)?;
    let mut app = app(
        directory.path(),
        definition.catalogs.units.clone(),
        LabState {
            current_tab: LabTab::LiveBattle,
            battle_paused: false,
            ..default()
        },
        policy.clone(),
    )?;
    app.insert_resource(session.with_world(simulation))
        .add_systems(
            Update,
            client::battle_pipeline::driver::advance_battle_ticks,
        );
    app.update();
    assert!(
        !has_error_exit(&app),
        "ordinary client keeps the diagnostic window open"
    );
    let status = app.world().resource::<ClientMediaStatus>();
    assert!(
        matches!(status.failure(), Some(ClientMediaError::Unavailable { source, .. }) if matches!(source.as_ref(), MediaLoadError::Io { path, .. } if path.ends_with(format!("{slug}.png"))))
    );
    assert!(app
        .world()
        .resource::<FlipbookRegistry>()
        .get_texture(id)
        .is_none());
    let lab = app.world().resource::<LabState>();
    assert!(lab.battle_paused);
    assert!(lab.battle_failure.is_some());
    assert_eq!(
        app.world()
            .resource::<client::battle_pipeline::session::BattleSession>()
            .world()
            .ok_or("retained world")?
            .current_tick,
        0
    );
    app.world_mut().resource_mut::<LabState>().battle_failure = None;
    app.world_mut().resource_mut::<LabState>().battle_paused = false;
    app.update();
    assert!(app.world().resource::<LabState>().battle_failure.is_some());
    assert!(app.world().resource::<LabState>().battle_paused);
    assert_eq!(
        app.world()
            .resource::<client::battle_pipeline::session::BattleSession>()
            .world()
            .ok_or("retained world")?
            .current_tick,
        0
    );
    assert!(matches!(
        policy.final_exit(AppExit::Success),
        AppExit::Error(_)
    ));
    assert!(matches!(
        policy.final_exit(AppExit::error()),
        AppExit::Error(_)
    ));
    assert_eq!(
        app.world_mut()
            .query::<&client::LiveBattleBubble>()
            .iter(app.world())
            .count(),
        0
    );
    Ok(())
}

#[test]
fn lab_and_swarm_reject_unavailable_requested_actions_before_material_creation() -> TestResult {
    let definition = bundle()?;
    let first_hero = definition
        .catalogs
        .units
        .archetypes()
        .filter(|arch| arch.showcase_hero)
        .min_by_key(|arch| arch.showcase_order.unwrap_or(u8::MAX))
        .ok_or("showcase")?
        .unit_type_id;
    let first_mobile = definition
        .catalogs
        .units
        .archetypes()
        .find(|arch| !matches!(arch.shape, ShapeKind::BuildingDome { .. }))
        .ok_or("mobile")?
        .unit_type_id;
    for (tab, id, action) in [
        (LabTab::VisualLab, first_hero, AnimState::Idle),
        (LabTab::SwarmStressTest, first_mobile, AnimState::Walk),
    ] {
        reject_action(&definition, tab, id, action)?;
    }
    Ok(())
}

fn reject_action(
    definition: &BattleDefinition,
    tab: LabTab,
    id: UnitTypeId,
    action: AnimState,
) -> TestResult {
    let slug = &definition.catalogs.units.get(id)?.slug;
    let directory = temporary_root()?;
    copy_media(directory.path(), slug, true)?;
    let path = directory
        .path()
        .join("textures")
        .join(format!("{slug}.flipbook.ron"));
    let mut metadata = FlipbookAtlasMeta::load_from_ron(&path)?;
    assert!(metadata.actions.remove(&action).is_some());
    metadata.save_to_ron(&path)?;
    let mut app = app(
        directory.path(),
        definition.catalogs.units.clone(),
        LabState {
            current_tab: tab,
            swarm_active: true,
            ..default()
        },
        MediaFailurePolicy::new(true),
    )?;
    app.insert_resource(definition.palette.clone());
    app.init_resource::<client::player_presentation::PresentationStatus>();
    match tab {
        LabTab::VisualLab => {
            app.add_systems(Update, client::sync_lab_preview_system);
        }
        LabTab::SwarmStressTest => {
            app.add_systems(Update, client::sync_swarm_stress_system);
        }
        LabTab::LiveBattle => return Err("not an action fixture".into()),
    }
    app.update();
    assert!(
        has_error_exit(&app),
        "bounded verifier must fail immediately"
    );
    assert!(
        matches!(app.world().resource::<ClientMediaStatus>().failure(), Some(ClientMediaError::Unavailable { source, .. }) if matches!(source.as_ref(), MediaLoadError::MissingAction { action: missing, .. } if *missing == action))
    );
    assert_eq!(
        app.world().resource::<Assets<BubbleDomeMaterial>>().len(),
        0
    );
    Ok(())
}

// --- Mode atlases (plan `2026-11-09-effect-visualization.md` WP13) -------------------------------

/// Every catalog mode whose atlas `<slug>-<mode>` ships: (archetype slug, unit type, mode index,
/// atlas name).
fn shipped_mode_atlases(catalog: &UnitCatalog) -> Vec<(String, UnitTypeId, u8, String)> {
    catalog
        .archetypes()
        .flat_map(|arch| {
            arch.modes.iter().enumerate().filter_map(|(index, mode)| {
                let name = asset_baker::mode_media::ModeAtlasId {
                    slug: &arch.slug,
                    stance: &mode.mode_name,
                }
                .atlas_name();
                let index = u8::try_from(index).ok()?;
                ships_media(&name).then(|| (arch.slug.clone(), arch.unit_type_id, index, name))
            })
        })
        .collect()
}

#[test]
fn shipped_mode_atlases_are_admitted_with_their_identity() -> TestResult {
    let (app, catalog) = shipped_media()?;
    let atlases = shipped_mode_atlases(&catalog);
    assert!(!atlases.is_empty(), "WP13 ships mode atlases");
    let registry = app.world().resource::<FlipbookRegistry>();
    for (slug, unit_type, mode, name) in &atlases {
        presentable(&app, *unit_type)?;
        let atlas = registry
            .get_mode(*unit_type, *mode)
            .ok_or_else(|| format!("{name} is not registered"))?;
        assert_eq!(&atlas.meta.name, name);
        assert!(atlas.meta.actions.contains_key(&AnimState::Idle), "{name}");
        assert_eq!(atlas.meta.morphs.len(), 2, "{name} holds both morphs");
        assert_ne!(
            Some(&atlas.texture),
            registry.get_texture(*unit_type).as_ref(),
            "{slug}: the mode atlas has its own texture"
        );
    }
    // A mode that ships no atlas keeps the default one: no entry, and its unit stays presentable.
    assert_eq!(registry.modes_by_id.len(), atlases.len());
    Ok(())
}

/// Boots the media registry on a temporary root holding `slug`'s default and mode files, after
/// `damage` edits the mode atlas's metadata. The directory must outlive the app.
fn mode_media_with(
    catalog: &UnitCatalog,
    slug: &str,
    mode_atlas: &str,
    damage: impl Fn(&mut FlipbookAtlasMeta),
) -> Result<(tempfile::TempDir, App), Box<dyn std::error::Error>> {
    let directory = temporary_root()?;
    copy_media(directory.path(), slug, true)?;
    copy_media(directory.path(), mode_atlas, true)?;
    let metadata_path = directory
        .path()
        .join("textures")
        .join(format!("{mode_atlas}.flipbook.ron"));
    let mut metadata: FlipbookAtlasMeta = ron::from_str(&std::fs::read_to_string(&metadata_path)?)?;
    damage(&mut metadata);
    std::fs::write(&metadata_path, ron::to_string(&metadata)?)?;
    let mut app = app(
        directory.path(),
        catalog.clone(),
        LabState::default(),
        MediaFailurePolicy::default(),
    )?;
    app.update();
    Ok((directory, app))
}

#[test]
fn a_mode_atlas_missing_a_morph_or_an_action_refuses_its_identity() -> TestResult {
    let catalog = bundle()?.catalogs.units;
    let (slug, unit_type, _, name) = shipped_mode_atlases(&catalog)
        .into_iter()
        .next()
        .ok_or("a shipped mode atlas")?;

    let (_directory, app) = mode_media_with(&catalog, &slug, &name, |meta| {
        meta.morphs.remove(&asset_baker::Morph::Leave);
    })?;
    let Err(error) = presentable(&app, unit_type) else {
        return Err("a mode atlas without its leave morph was admitted".into());
    };
    assert!(
        matches!(error, ClientMediaError::Unavailable { ref source, .. }
            if matches!(source.as_ref(), MediaLoadError::MissingMorph { morph: asset_baker::Morph::Leave, .. })),
        "{error}"
    );

    let (_directory, app) = mode_media_with(&catalog, &slug, &name, |meta| {
        meta.actions.remove(&AnimState::Idle);
    })?;
    let Err(error) = presentable(&app, unit_type) else {
        return Err("a mode atlas without its Idle was admitted".into());
    };
    assert!(
        matches!(error, ClientMediaError::Unavailable { ref source, .. }
            if matches!(source.as_ref(), MediaLoadError::MissingAction { action: AnimState::Idle, .. })),
        "{error}"
    );
    Ok(())
}

#[test]
fn a_mode_atlas_named_for_another_identity_is_refused() -> TestResult {
    let catalog = bundle()?.catalogs.units;
    let (slug, unit_type, _, name) = shipped_mode_atlases(&catalog)
        .into_iter()
        .next()
        .ok_or("a shipped mode atlas")?;
    let (_directory, app) = mode_media_with(&catalog, &slug, &name, |meta| {
        meta.name = slug.clone();
    })?;
    assert!(matches!(
        presentable(&app, unit_type),
        Err(ClientMediaError::Unavailable { ref source, .. })
            if matches!(source.as_ref(), MediaLoadError::Representation { .. })
    ));
    Ok(())
}

#[test]
fn a_unit_whose_modes_ship_no_atlas_keeps_its_default_atlas() -> TestResult {
    let catalog = bundle()?.catalogs.units;
    let (slug, unit_type, _, _) = shipped_mode_atlases(&catalog)
        .into_iter()
        .next()
        .ok_or("a shipped mode atlas")?;
    let directory = temporary_root()?;
    copy_media(directory.path(), &slug, true)?;
    let mut app = app(
        directory.path(),
        catalog.clone(),
        LabState::default(),
        MediaFailurePolicy::default(),
    )?;
    app.update();
    presentable(&app, unit_type)?;
    assert!(app
        .world()
        .resource::<FlipbookRegistry>()
        .modes_by_id
        .is_empty());
    Ok(())
}
