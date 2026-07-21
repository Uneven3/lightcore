//! User-facing settings shared by otherwise independent plugins.
//!
//! This resource deliberately contains no render or window geometry. Presentation-specific
//! settings live in `presentation`, while this module owns stable player preferences.
//!
//! Unlike render/window geometry, these preferences persist across launches through the shared
//! `core::storage` backend (a plain file natively, `localStorage` on wasm) — the same mechanism
//! `run`/`campaign` use. Before this, language/tutorial/FPS silently reset to defaults on every
//! launch even though run and campaign progress survived, which read as the game "forgetting" the
//! player's language every time.

use bevy::prelude::*;

use crate::core::locale::Language;
use crate::core::storage;

const SETTINGS_SAVE_VERSION: &str = "lightcore-settings-v1";
const SETTINGS_FILE: &str = "settings.txt";

#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct UserSettings {
    pub(crate) tutorial_enabled: bool,
    pub(crate) show_fps_watermark: bool,
    pub(crate) language: Language,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            tutorial_enabled: true,
            show_fps_watermark: true,
            language: Language::default(),
        }
    }
}

impl UserSettings {
    fn encode(&self) -> String {
        format!(
            "{SETTINGS_SAVE_VERSION}\n{}\n{}\n{}",
            self.tutorial_enabled as u8,
            self.show_fps_watermark as u8,
            match self.language {
                Language::Spanish => "es",
                Language::English => "en",
            }
        )
    }

    fn decode(raw: &str) -> Option<Self> {
        let mut lines = raw.lines();
        if lines.next()? != SETTINGS_SAVE_VERSION {
            return None;
        }
        let tutorial_enabled = lines.next()? != "0";
        let show_fps_watermark = lines.next()? != "0";
        let language = match lines.next()? {
            "en" => Language::English,
            _ => Language::Spanish,
        };
        Some(Self {
            tutorial_enabled,
            show_fps_watermark,
            language,
        })
    }
}

pub(crate) struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        // `PreStartup` so the loaded language is in place before any menu (`OnEnter`, which runs
        // after startup) spawns its localized text.
        app.init_resource::<UserSettings>()
            .add_systems(PreStartup, load_settings)
            .add_systems(
                Update,
                save_settings.run_if(resource_changed::<UserSettings>),
            );
    }
}

fn load_settings(mut settings: ResMut<UserSettings>) {
    if let Some(saved) =
        storage::load_save_file(SETTINGS_FILE).and_then(|raw| UserSettings::decode(&raw))
    {
        *settings = saved;
    }
}

fn save_settings(settings: Res<UserSettings>) {
    if let Err(err) = storage::write_save_file(SETTINGS_FILE, &settings.encode()) {
        bevy::log::warn!("No se pudo guardar la configuración: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_save_text() {
        let settings = UserSettings {
            tutorial_enabled: false,
            show_fps_watermark: false,
            language: Language::English,
        };
        let decoded = UserSettings::decode(&settings.encode()).unwrap();
        assert_eq!(decoded, settings);
    }

    #[test]
    fn unknown_or_corrupt_text_decodes_to_none() {
        assert!(UserSettings::decode("garbage").is_none());
        assert!(UserSettings::decode("").is_none());
    }
}
