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

use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};

/// Resolución interna de render (ALTURA en píxeles; el ancho se deriva del aspecto de la ventana para
/// no distorsionar). Más alto = más nítido y más caro: es la palanca calidad↔FPS que expone la opción
/// "Resolución" del menú. El coste del bloom escala con esto, NO con el tamaño de la ventana.
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

/// Capa de render del blit final: el sprite del lienzo y la cámara FINAL viven aquí, separados de las
/// entidades del mundo (capa 0 por defecto), para que ninguna cámara vea lo de la otra.
const BLIT_LAYER: usize = 1;

/// Cámara que renderiza el MUNDO al lienzo (HDR+Bloom, shake, picking). La marcamos para que los
/// sistemas que antes hacían `Single<.., With<Camera2d>>` no choquen con la cámara FINAL.
#[derive(Component)]
pub(crate) struct WorldCamera;

/// Cámara que estira el lienzo a la ventana y ancla el HUD nativo (`IsDefaultUiCamera`).
#[derive(Component)]
pub(crate) struct FinalCamera;

/// El sprite que muestra el lienzo; `fit_canvas` ajusta su `custom_size` para llenar la ventana.
#[derive(Component)]
pub(crate) struct CanvasSprite;

/// Handle del lienzo (render target del mundo), para que `fit_canvas` lo redimensione.
#[derive(Resource)]
pub(crate) struct Canvas(pub(crate) Handle<Image>);

/// Crea el lienzo (imagen usable como render target). Tamaño inicial provisional: `fit_canvas` lo
/// ajusta al aspecto real de la ventana en el primer frame.
pub(crate) fn create_canvas(images: &mut Assets<Image>, width: u32, height: u32) -> Handle<Image> {
    let size = Extent3d {
        width: width.max(1),
        height: height.max(1),
        depth_or_array_layers: 1,
    };
    let mut image = Image::new_fill(
        size,
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    // RENDER_ATTACHMENT: se puede dibujar encima. TEXTURE_BINDING: se puede muestrear (el blit final).
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    images.add(image)
}

/// El `RenderTarget` que apunta la cámara MUNDO al lienzo.
pub(crate) fn world_target(canvas: &Handle<Image>) -> RenderTarget {
    RenderTarget::Image(canvas.clone().into())
}

/// Spawnea el sprite del lienzo y la cámara FINAL (blit + HUD nativo). La cámara MUNDO la crea
/// `setup_camera` porque necesita la config de Bloom/tonemapping.
pub(crate) fn spawn_blit(commands: &mut Commands, canvas: Handle<Image>) {
    commands.spawn((
        Sprite::from_image(canvas),
        CanvasSprite,
        RenderLayers::layer(BLIT_LAYER),
    ));
    commands.spawn((
        Camera2d,
        Camera {
            order: 0,
            ..default()
        },
        // El lienzo ya viene tonemapeado por la cámara mundo; la FINAL solo lo estira → sin re-tonemap.
        bevy::core_pipeline::tonemapping::Tonemapping::None,
        FinalCamera,
        RenderLayers::layer(BLIT_LAYER),
        bevy::ui::IsDefaultUiCamera,
    ));
}

/// Mantiene el lienzo con el MISMO aspecto que la ventana (altura = `RenderScale`, sin barras ni
/// distorsión) y estira el sprite para llenar la ventana. Barato: usa un `Local` para actuar solo
/// cuando cambia el tamaño físico de la ventana o la resolución interna.
pub(crate) fn fit_canvas(
    render_scale: Res<RenderScale>,
    window: Single<&Window>,
    canvas: Res<Canvas>,
    mut images: ResMut<Assets<Image>>,
    mut sprite: Single<&mut Sprite, With<CanvasSprite>>,
    mut last: Local<(u32, u32, u32, crate::menu::options::DeviceMode)>,
    settings: Res<crate::menu::options::WindowSettings>,
    mut final_camera: Single<&mut Camera, With<FinalCamera>>,
) {
    let pw = window.physical_width().max(1);
    let ph = window.physical_height().max(1);
    let cap = render_scale.internal_height.max(1);
    let mode = settings.device_mode;

    // Calculate actual rendering height (ih) capped by the physical viewport height to prevent downscaling artifacts
    let ih = match mode {
        crate::menu::options::DeviceMode::Mobile => {
            let target_aspect = 9.0 / 16.0;
            let window_aspect = pw as f32 / ph as f32;
            let vp_h = if window_aspect < target_aspect {
                (pw as f32) / target_aspect
            } else {
                ph as f32
            };
            (vp_h.round() as u32).min(cap).max(1)
        }
        crate::menu::options::DeviceMode::Desktop => {
            ph.min(cap).max(1)
        }
    };

    if *last == (pw, ph, ih, mode) {
        return;
    }
    *last = (pw, ph, ih, mode);

    let (iw, logical_size) = match mode {
        crate::menu::options::DeviceMode::Mobile => {
            // Simulated mobile viewport: 9:16 aspect ratio in center
            let target_aspect = 9.0 / 16.0;
            let window_aspect = pw as f32 / ph as f32;

            let (vp_w, vp_h) = if window_aspect < target_aspect {
                // Window is narrower than 9:16
                let w = pw as f32;
                let h = w / target_aspect;
                (w, h)
            } else {
                // Window is wider than 9:16
                let h = ph as f32;
                let w = h * target_aspect;
                (w, h)
            };

            let vp_x = (pw as f32 - vp_w) / 2.0;
            let vp_y = (ph as f32 - vp_h) / 2.0;

            final_camera.viewport = Some(bevy::camera::Viewport {
                physical_position: UVec2::new(vp_x.round() as u32, vp_y.round() as u32),
                physical_size: UVec2::new(vp_w.round() as u32, vp_h.round() as u32),
                depth: 0.0..1.0,
            });

            let iw = (ih as f32 * target_aspect).round() as u32;
            let scale_factor = window.scale_factor();
            let logical_w = vp_w / scale_factor;
            let logical_h = vp_h / scale_factor;
            (iw, Vec2::new(logical_w, logical_h))
        }
        crate::menu::options::DeviceMode::Desktop => {
            final_camera.viewport = None;
            let iw = (ih as f32 * pw as f32 / ph as f32).round().max(1.0) as u32;
            (iw, window.size())
        }
    };

    if let Some(mut img) = images.get_mut(&canvas.0) {
        img.resize(Extent3d {
            width: iw,
            height: ih,
            depth_or_array_layers: 1,
        });
    }
    // La cámara FINAL usa ScalingMode::WindowSize, que mide en unidades LÓGICAS (= físico /
    // scale_factor). Por eso el sprite debe dimensionarse en LÓGICO, no físico: en Retina (sf 2.0) un
    // custom_size físico haría el sprite 2× el viewport y solo se vería el cuarto central magnificado
    // (lo que rompía el picking y se veía pixelado). Con el tamaño lógico el lienzo llena la ventana
    // exacta y uniformemente (el aspecto ya coincide, iw = ih·pw/ph), y el picking vuelve a ser exacto.
    sprite.custom_size = Some(logical_size);
}

/// Mapea un punto en píxeles de VENTANA al mundo, a través de la cámara que renderiza al lienzo.
/// Necesario porque el viewport de esa cámara es el lienzo (no la ventana): escalamos el punto
/// ventana→lienzo y desproyectamos. El factor es uniforme porque el lienzo conserva el aspecto.
pub(crate) fn window_point_to_world(
    camera: &Camera,
    cam_t: &GlobalTransform,
    viewport_size: Vec2,
    point: Vec2,
) -> Option<Vec2> {
    let canvas = camera.logical_viewport_size()?;
    if viewport_size.x <= 0.0 || viewport_size.y <= 0.0 {
        return None;
    }
    let scaled = point * canvas / viewport_size;
    camera.viewport_to_world_2d(cam_t, scaled).ok()
}
