use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
use web_time::Instant;

use crate::visuals::render_target::{self, WorldCamera};

/// Target frames per second — cycles through presets via the Options screen.
#[derive(Resource, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FpsTarget {
    Unlimited,
    Fps30,
    Fps60,
    Fps120,
}

#[allow(clippy::derivable_impls)]
impl Default for FpsTarget {
    fn default() -> Self {
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            Self::Unlimited
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            Self::Fps60
        }
    }
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

pub(crate) fn setup_camera(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    settings: Res<crate::presentation::PresentationSettings>,
) {
    let initial_size = settings
        .internal_resolution
        .size_for_viewport(UVec2::new(1280, 720));
    let image_handle = images.add(render_target::create_canvas_image(initial_size));

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
        bevy::camera::RenderTarget::Image(image_handle.clone().into()),
        Tonemapping::TonyMcMapface,
        DebandDither::Enabled,
        Msaa::Off,
        WorldCamera,
    ));

    commands.spawn((
        Sprite::from_image(image_handle.clone()),
        Transform::default(),
        render_target::InternalCanvas,
        bevy::camera::visibility::RenderLayers::layer(1),
    ));
    commands.insert_resource(render_target::InternalRenderTarget {
        image: image_handle,
        size: initial_size,
    });
    render_target::spawn_blit(&mut commands);
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
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
