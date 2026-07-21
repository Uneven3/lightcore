use bevy::prelude::*;

pub(crate) struct GridWaterPlugin;

impl Plugin for GridWaterPlugin {
    fn build(&self, _app: &mut App) {}
}

#[derive(Resource)]
pub(crate) struct GridWaterSettings {
    pub(crate) enabled: bool,
}

impl Default for GridWaterSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}
