use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::prelude::*;
use rand::Rng;
use web_time::Instant;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

use crate::core::grid::TILE;
use crate::gameplay::ChainPop;
use crate::visuals::render_target::{self, WorldCamera};

/// Target frames per second — cycles through presets via the Options screen. Default 60.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum FpsTarget {
    Unlimited,
    Fps30,
    #[default]
    Fps60,
    Fps120,
}

impl FpsTarget {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Fps30 => Self::Fps60,
            Self::Fps60 => Self::Fps120,
            Self::Fps120 => Self::Unlimited,
            Self::Unlimited => Self::Fps30,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Unlimited => "FPS: Sin limite",
            Self::Fps30 => "FPS: 30",
            Self::Fps60 => "FPS: 60",
            Self::Fps120 => "FPS: 120",
        }
    }
}

#[derive(Resource)]
pub(crate) struct FrameTimer(pub(crate) Instant);

impl Default for FrameTimer {
    fn default() -> Self {
        Self(Instant::now())
    }
}

pub(crate) fn record_frame_start(mut t: ResMut<FrameTimer>) {
    t.0 = Instant::now();
}

#[allow(unused_variables)]
pub(crate) fn cap_framerate(t: Res<FrameTimer>, target: Res<FpsTarget>) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let dur = match *target {
            FpsTarget::Unlimited => return,
            FpsTarget::Fps30 => Duration::from_secs_f64(1.0 / 30.0),
            FpsTarget::Fps60 => Duration::from_secs_f64(1.0 / 60.0),
            FpsTarget::Fps120 => Duration::from_secs_f64(1.0 / 120.0),
        };
        let elapsed = t.0.elapsed();
        if elapsed < dur {
            std::thread::sleep(dur - elapsed);
        }
    }
}

/// Live-tunable camera shake parameters, edited from the Options screen.
#[derive(Resource, Clone, Copy)]
pub(crate) struct ShakeSettings {
    pub(crate) max_offset: f32,
    /// Exponential decay coefficient, ~150-250ms to settle at the default.
    pub(crate) decay_rate: f32,
}

impl Default for ShakeSettings {
    fn default() -> Self {
        Self {
            max_offset: TILE * 0.12,
            decay_rate: 6.0,
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct CameraShake {
    pub(crate) trauma: f32,
}

pub(crate) fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::camera::ScalingMode::AutoMin {
                min_width: 600.0,
                min_height: 720.0,
            },
            ..OrthographicProjection::default_2d()
        }),
        Camera {
            order: -1,
            ..default()
        },
        Tonemapping::TonyMcMapface,
        DebandDither::Enabled,
        WorldCamera,
    ));

    render_target::spawn_blit(&mut commands);
}

pub(crate) fn toggle_slow_mo(
    keys: Res<ButtonInput<KeyCode>>,
    mut virtual_time: ResMut<Time<Virtual>>,
) {
    if keys.just_pressed(KeyCode::KeyR) {
        let new_speed = if virtual_time.relative_speed() < 0.5 {
            1.0
        } else {
            0.2
        };
        virtual_time.set_relative_speed(new_speed);
    }
}

pub(crate) fn on_chain_pop(trigger: On<ChainPop>, mut shake: ResMut<CameraShake>) {
    shake.trauma = (shake.trauma + trigger.removed as f32 * 0.04).min(1.0);
}

pub(crate) fn apply_camera_shake(
    time: Res<Time>,
    mut shake: ResMut<CameraShake>,
    shake_settings: Res<ShakeSettings>,
    mut camera_t: Single<&mut Transform, With<WorldCamera>>,
) {
    if shake.trauma <= 0.001 {
        shake.trauma = 0.0;
        camera_t.translation.x = 0.0;
        camera_t.translation.y = 0.0;
        return;
    }
    let mut rng = rand::rng();
    let amount = shake.trauma * shake.trauma;
    camera_t.translation.x = rng.random_range(-1.0..1.0) * shake_settings.max_offset * amount;
    camera_t.translation.y = rng.random_range(-1.0..1.0) * shake_settings.max_offset * amount;
    shake.trauma =
        (shake.trauma - shake_settings.decay_rate * shake.trauma * time.delta_secs()).max(0.0);
}
