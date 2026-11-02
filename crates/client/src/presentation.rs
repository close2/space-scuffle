//! Shared non-editor presentation plugins and explicit current audio capability.
//!
//! Game and capture applications use the real default renderer but never open an
//! unused audio device. Future playback requires an owned stream lifecycle and
//! typed device/playback failure handling, not silently restoring default audio.

use bevy::app::PluginGroupBuilder;
use bevy::audio::AudioPlugin;
use bevy::prelude::*;
use serde::Serialize;

/// Audio capability recorded by the game and its verification artifacts.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum AudioPolicy {
    /// No playback subsystem or sound assets are implemented; no device is opened.
    DisabledNoPlaybackSubsystem,
}

/// Real Bevy presentation plugins with the game's explicit audio policy.
///
/// Window, asset and renderer settings remain configurable through normal
/// [`PluginGroup::set`]. This group is intentionally separate from editor setup.
pub struct GamePresentationPlugins;

impl PluginGroup for GamePresentationPlugins {
    fn build(self) -> PluginGroupBuilder {
        DefaultPlugins
            .build()
            .disable::<AudioPlugin>()
            .add(AudioPolicyPlugin)
    }
}

struct AudioPolicyPlugin;

impl Plugin for AudioPolicyPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(AudioPolicy::DisabledNoPlaybackSubsystem)
            .add_systems(Startup, report_audio_policy);
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn report_audio_policy(policy: Res<'_, AudioPolicy>) {
    info!(
        "Game audio policy: {:?}; no audio output device requested because playback is not implemented",
        *policy
    );
}

#[cfg(test)]
mod tests {
    use super::{AudioPolicy, AudioPolicyPlugin};
    use bevy::audio::AudioPlugin;
    use bevy::prelude::*;

    #[test]
    fn policy_is_explicit_without_installing_audio_or_playback_assets(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut app = App::new();
        app.add_plugins(AudioPolicyPlugin);
        app.update();
        let policy = app.world().resource::<AudioPolicy>();
        assert_eq!(*policy, AudioPolicy::DisabledNoPlaybackSubsystem);
        assert_eq!(ron::to_string(policy)?, "DisabledNoPlaybackSubsystem");
        assert!(!app.is_plugin_added::<AudioPlugin>());
        assert!(!app.world().contains_resource::<Assets<AudioSource>>());
        Ok(())
    }
}
