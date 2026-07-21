//! User-facing settings shared by otherwise independent plugins.
//!
//! This resource deliberately contains no render or window geometry. Presentation-specific
//! settings live in `presentation`, while this module owns stable player preferences.

use bevy::prelude::*;

use crate::core::locale::Language;

#[derive(Resource)]
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
