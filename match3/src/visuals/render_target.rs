//! Camera viewport helpers. The world camera draws the board and world-space VFX; the final camera
//! owns native Bevy UI. In mobile simulation both cameras share a centered 9:16 viewport so pointer
//! mapping and HUD placement agree.
//!
//! Coordenadas: el picking (cursor→mundo) pasa por `window_point_to_world` para que el viewport
//! móvil simulado y la cámara mundo usen la misma proyección.

use bevy::prelude::*;

/// Cámara que renderiza el mundo (shake, picking). La marcamos para que los
/// sistemas que antes hacían `Single<.., With<Camera2d>>` no choquen con la cámara FINAL.
#[derive(Component)]
pub(crate) struct WorldCamera;

/// Cámara final que ancla el HUD nativo (`IsDefaultUiCamera`).
#[derive(Component)]
pub(crate) struct FinalCamera;

/// Spawnea la cámara FINAL (HUD nativo) configurada para no limpiar la pantalla y renderizar directamente al viewport.
pub(crate) fn spawn_blit(commands: &mut Commands) {
    commands.spawn((
        Camera2d,
        Camera {
            order: 0,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        bevy::core_pipeline::tonemapping::Tonemapping::None,
        FinalCamera,
        bevy::camera::visibility::RenderLayers::layer(1),
        bevy::ui::IsDefaultUiCamera,
    ));
}

/// Mantiene las cámaras con el mismo viewport simulado si estamos en modo Mobile,
/// o las restablece al tamaño total en modo Desktop. Ya no escala ni redimensiona ninguna textura.
pub(crate) fn fit_canvas(
    window: Single<&Window>,
    mut last: Local<(u32, u32, crate::menu::options::DeviceMode)>,
    settings: Res<crate::menu::options::WindowSettings>,
    mut world_camera: Single<&mut Camera, (With<WorldCamera>, Without<FinalCamera>)>,
    mut final_camera: Single<&mut Camera, (With<FinalCamera>, Without<WorldCamera>)>,
) {
    let pw = window.physical_width().max(1);
    let ph = window.physical_height().max(1);
    let mode = settings.device_mode;

    if *last == (pw, ph, mode) {
        return;
    }
    *last = (pw, ph, mode);

    match mode {
        crate::menu::options::DeviceMode::Mobile => {
            // Simulated mobile viewport: 9:16 aspect ratio in center
            let target_aspect = 9.0 / 16.0;
            let window_aspect = pw as f32 / ph as f32;

            let (vp_w, vp_h) = if window_aspect < target_aspect {
                let w = pw as f32;
                let h = w / target_aspect;
                (w, h)
            } else {
                let h = ph as f32;
                let w = h * target_aspect;
                (w, h)
            };

            let vp_x = (pw as f32 - vp_w) / 2.0;
            let vp_y = (ph as f32 - vp_h) / 2.0;

            let viewport = Some(bevy::camera::Viewport {
                physical_position: UVec2::new(vp_x.round() as u32, vp_y.round() as u32),
                physical_size: UVec2::new(vp_w.round() as u32, vp_h.round() as u32),
                depth: 0.0..1.0,
            });

            world_camera.viewport = viewport.clone();
            final_camera.viewport = viewport;
        }
        crate::menu::options::DeviceMode::Desktop => {
            world_camera.viewport = None;
            final_camera.viewport = None;
        }
    }
}

/// Mapea un punto en píxeles de ventana al mundo usando la cámara de juego. Necesario para que el
/// viewport móvil simulado y el picking hablen el mismo sistema de coordenadas.
pub(crate) fn window_point_to_world(
    camera: &Camera,
    cam_t: &GlobalTransform,
    _viewport_size: Vec2,
    point: Vec2,
) -> Option<Vec2> {
    camera.viewport_to_world_2d(cam_t, point).ok()
}
