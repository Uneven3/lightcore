//! Camera viewport helpers. The world camera draws the board and world-space VFX; the final camera
//! owns native Bevy UI. In mobile simulation both cameras share a centered 9:16 viewport so pointer
//! mapping and HUD placement agree.
//!
//! Coordenadas: el picking (cursor→mundo) pasa por `window_point_to_world` para que el viewport
//! móvil simulado y la cámara mundo usen la misma proyección.

use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
};

const PRESENTATION_LAYER: usize = 1;

/// Cámara que renderiza el mundo (shake, picking). La marcamos para que los
/// sistemas que antes hacían `Single<.., With<Camera2d>>` no choquen con la cámara FINAL.
#[derive(Component)]
pub(crate) struct WorldCamera;

/// Cámara final que ancla el HUD nativo (`IsDefaultUiCamera`).
#[derive(Component)]
pub(crate) struct FinalCamera;

#[derive(Component)]
pub(crate) struct InternalCanvas;

#[derive(Resource)]
pub(crate) struct InternalRenderTarget {
    pub(crate) image: Handle<Image>,
    pub(crate) size: UVec2,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum InternalResolution {
    #[default]
    Native,
    High,
    Medium,
    Low,
}

impl InternalResolution {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Native => Self::High,
            Self::High => Self::Medium,
            Self::Medium => Self::Low,
            Self::Low => Self::Native,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Native => "Resolucion interna: Nativa",
            Self::High => "Resolucion interna: Alta",
            Self::Medium => "Resolucion interna: Media",
            Self::Low => "Resolucion interna: Baja",
        }
    }

    fn target_height(self, native: UVec2) -> u32 {
        match self {
            Self::Native => native.y,
            Self::High => 900,
            Self::Medium => 720,
            Self::Low => 540,
        }
    }

    pub(crate) fn size_for_viewport(self, viewport_size: UVec2) -> UVec2 {
        let native = viewport_size.max(UVec2::ONE);
        if self == Self::Native {
            return native;
        }

        let target_h = self.target_height(native).min(native.y).max(1);
        let target_w = ((native.x as u64 * target_h as u64 + native.y as u64 / 2) / native.y as u64)
            .max(1) as u32;
        UVec2::new(target_w, target_h)
    }
}

/// Spawnea la cámara FINAL (HUD nativo) configurada para presentar el canvas interno y la UI.
pub(crate) fn spawn_blit(commands: &mut Commands) {
    commands.spawn((
        Camera2d,
        Camera {
            order: 0,
            clear_color: ClearColorConfig::Custom(Color::srgb(0.012, 0.012, 0.022)),
            ..default()
        },
        bevy::core_pipeline::tonemapping::Tonemapping::None,
        FinalCamera,
        bevy::camera::visibility::RenderLayers::layer(PRESENTATION_LAYER),
        bevy::ui::IsDefaultUiCamera,
    ));
}

pub(crate) fn create_canvas_image(size: UVec2) -> Image {
    let extent = Extent3d {
        width: size.x.max(1),
        height: size.y.max(1),
        depth_or_array_layers: 1,
    };
    let mut image = Image {
        texture_descriptor: TextureDescriptor {
            label: Some("lightcore_internal_canvas"),
            size: extent,
            dimension: TextureDimension::D2,
            format: TextureFormat::Bgra8UnormSrgb,
            mip_level_count: 1,
            sample_count: 1,
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_DST
                | TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        },
        ..default()
    };
    image.resize(extent);
    image
}

/// Mantiene las cámaras con el mismo viewport simulado si estamos en modo Mobile,
/// o las restablece al tamaño total en modo Desktop. El mundo se renderiza a textura interna;
/// solo la cámara final usa el viewport de pantalla.
pub(crate) fn fit_canvas(
    window: Single<&Window>,
    mut last: Local<(
        u32,
        u32,
        crate::menu::options::DeviceMode,
        InternalResolution,
    )>,
    settings: Res<crate::menu::options::WindowSettings>,
    mut images: ResMut<Assets<Image>>,
    mut target: ResMut<InternalRenderTarget>,
    mut world_camera: Single<&mut Camera, (With<WorldCamera>, Without<FinalCamera>)>,
    mut final_camera: Single<&mut Camera, (With<FinalCamera>, Without<WorldCamera>)>,
    canvas: Single<(&mut Sprite, &mut Transform), With<InternalCanvas>>,
) {
    let pw = window.physical_width().max(1);
    let ph = window.physical_height().max(1);
    let mode = settings.device_mode;
    let internal_resolution = settings.internal_resolution;

    if *last == (pw, ph, mode, internal_resolution) {
        return;
    }
    *last = (pw, ph, mode, internal_resolution);

    let viewport = match mode {
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

            Some(bevy::camera::Viewport {
                physical_position: UVec2::new(vp_x.round() as u32, vp_y.round() as u32),
                physical_size: UVec2::new(vp_w.round() as u32, vp_h.round() as u32),
                depth: 0.0..1.0,
            })
        }
        crate::menu::options::DeviceMode::Desktop => None,
    };

    let viewport_size = viewport
        .as_ref()
        .map(|v| v.physical_size)
        .unwrap_or(UVec2::new(pw, ph))
        .max(UVec2::ONE);
    let internal_size = internal_resolution.size_for_viewport(viewport_size);
    if target.size != internal_size {
        if let Some(mut image) = images.get_mut(&target.image) {
            *image = create_canvas_image(internal_size);
        }
        target.size = internal_size;
    }

    world_camera.viewport = None;
    final_camera.viewport = viewport;

    let final_logical_size = viewport_size.as_vec2() / window.scale_factor();
    let (mut sprite, mut transform) = canvas.into_inner();
    sprite.custom_size = Some(final_logical_size);
    transform.translation = Vec3::ZERO;
}

pub(crate) fn final_viewport_logical_rect(camera: &Camera, window: &Window) -> (Vec2, Vec2) {
    if let Some(ref viewport) = camera.viewport {
        let scale_factor = window.scale_factor();
        let pos = Vec2::new(
            viewport.physical_position.x as f32,
            viewport.physical_position.y as f32,
        ) / scale_factor;
        let size = Vec2::new(
            viewport.physical_size.x as f32,
            viewport.physical_size.y as f32,
        ) / scale_factor;
        (pos, size)
    } else {
        (Vec2::ZERO, window.size())
    }
}

/// Mapea un punto en píxeles de ventana al mundo usando la cámara de juego. Necesario para que el
/// viewport móvil simulado y el picking hablen el mismo sistema de coordenadas.
pub(crate) fn window_point_to_world(
    camera: &Camera,
    cam_t: &GlobalTransform,
    final_viewport_pos: Vec2,
    final_viewport_size: Vec2,
    point: Vec2,
) -> Option<Vec2> {
    let target_size = camera.logical_viewport_size()?;
    let relative = (point - final_viewport_pos) / final_viewport_size.max(Vec2::ONE);
    if relative.x < 0.0 || relative.y < 0.0 || relative.x > 1.0 || relative.y > 1.0 {
        return None;
    }
    let point = relative * target_size;
    camera.viewport_to_world_2d(cam_t, point).ok()
}
