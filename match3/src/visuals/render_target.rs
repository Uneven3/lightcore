//! Render a resolución interna fija (render-to-texture). El MUNDO (tablero, estrellas, partículas,
//! el score `Text2d`) se dibuja con HDR+Bloom a un LIENZO de altura fija (`RenderScale`), y una
//! segunda cámara estira ese lienzo para llenar la ventana. Así el coste por-pixel del bloom —lo
//! caro— queda CLAVADO a la resolución interna por mucho que agrandes o maximices la ventana, y el
//! tablero siempre llena la pantalla (nunca "se ve pequeño"). El HUD (nodos UI) lo pinta la cámara
//! FINAL a resolución NATIVA, así que el texto sale nítido; solo el mundo y su glow van a baja-res.
//!
//! Coordenadas: la cámara MUNDO renderiza a una imagen, no a la ventana, así que su viewport ES el
//! lienzo. El picking (cursor→mundo) por eso pasa por `window_point_to_world`, que escala el cursor
//! ventana→lienzo antes de desproyectar. Como el lienzo conserva el aspecto de la ventana, el factor
//! es uniforme y no hay barras (letterbox) que compensar.

use bevy::prelude::*;

/// Resolución interna de render (ALTURA en píxeles; el ancho se deriva del aspecto de la ventana para
/// no distorsionar). Más alto = más nítido y más caro; la plataforma fija este valor al iniciar.
/// El coste del bloom escala con esto, NO con el tamaño de la ventana.
#[derive(Resource, Clone, Copy)]
pub(crate) struct RenderScale {
    pub(crate) internal_height: u32,
}

impl Default for RenderScale {
    fn default() -> Self {
        Self {
            internal_height: 1080,
        }
    }
}

/// Cámara que renderiza el MUNDO al lienzo (HDR+Bloom, shake, picking). La marcamos para que los
/// sistemas que antes hacían `Single<.., With<Camera2d>>` no choquen con la cámara FINAL.
#[derive(Component)]
pub(crate) struct WorldCamera;

/// Cámara que estira el lienzo a la ventana y ancla el HUD nativo (`IsDefaultUiCamera`).
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

/// Mapea un punto en píxeles de VENTANA al mundo, a través de la cámara que renderiza al lienzo.
/// Necesario porque el viewport de esa cámara es el lienzo (no la ventana): escalamos el punto
/// ventana→lienzo y desproyectamos. El factor es uniforme porque el lienzo conserva el aspecto.
pub(crate) fn window_point_to_world(
    camera: &Camera,
    cam_t: &GlobalTransform,
    _viewport_size: Vec2,
    point: Vec2,
) -> Option<Vec2> {
    camera.viewport_to_world_2d(cam_t, point).ok()
}
