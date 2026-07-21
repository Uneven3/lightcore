//! Camera viewport helpers. The world camera draws the board and world-space VFX; the final camera
//! owns native Bevy UI. The responsive `GameLayout` supplies one effective viewport so camera,
//! pointer mapping and HUD placement agree.
//!
//! Coordenadas: el picking (cursor→mundo) pasa por `window_point_to_world` para que el viewport
//! móvil simulado y la cámara mundo usen la misma proyección.

use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
};

use crate::core::grid::{GRID_H, GRID_W, TILE};
use crate::presentation::{GameLayout, InternalResolution, PresentationSettings};
use crate::state::Screen;

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

/// Applies the effective presentation viewport and fits the match camera into `GameLayout`'s
/// reserved playfield. The world renders to an internal texture; only the final camera owns the
/// physical window viewport.
pub(crate) fn fit_canvas(
    window: Single<&Window>,
    mut last: Local<Option<(GameLayout, InternalResolution, Screen)>>,
    layout: Res<GameLayout>,
    settings: Res<PresentationSettings>,
    screen: Res<State<Screen>>,
    mut images: ResMut<Assets<Image>>,
    mut target: ResMut<InternalRenderTarget>,
    world_camera: Single<
        (&mut Camera, &mut Projection, &mut Transform),
        (
            With<WorldCamera>,
            Without<FinalCamera>,
            Without<InternalCanvas>,
        ),
    >,
    mut final_camera: Single<&mut Camera, (With<FinalCamera>, Without<WorldCamera>)>,
    canvas: Single<
        (&mut Sprite, &mut Transform),
        (
            With<InternalCanvas>,
            Without<WorldCamera>,
            Without<FinalCamera>,
        ),
    >,
) {
    let internal_resolution = settings.internal_resolution;

    if last
        .as_ref()
        .is_some_and(|(previous, resolution, previous_screen)| {
            previous == &*layout
                && *resolution == internal_resolution
                && *previous_screen == *screen.get()
        })
    {
        return;
    }
    *last = Some((layout.clone(), internal_resolution, *screen.get()));

    let physical = layout.viewport.physical;
    let full_window = UVec2::new(
        window.physical_width().max(1),
        window.physical_height().max(1),
    );
    let viewport = if physical.position == UVec2::ZERO && physical.size == full_window {
        None
    } else {
        Some(bevy::camera::Viewport {
            physical_position: physical.position,
            physical_size: physical.size,
            depth: 0.0..1.0,
        })
    };

    let viewport_size = physical.size.max(UVec2::ONE);
    let internal_size = internal_resolution.size_for_viewport(viewport_size);
    if target.size != internal_size {
        if let Some(mut image) = images.get_mut(&target.image) {
            *image = create_canvas_image(internal_size);
        }
        target.size = internal_size;
    }

    let (mut world_camera, mut projection, mut world_transform) = world_camera.into_inner();
    world_camera.viewport = None;
    final_camera.viewport = viewport;

    if let Projection::Orthographic(orthographic) = &mut *projection {
        if *screen.get() == Screen::Match {
            // Fit the board inside the playfield reserved by GameLayout, rather than against the
            // entire screen. The camera still renders a full-screen canvas so VFX may travel
            // behind the chrome.
            const BOARD_PADDING: f32 = 48.0;
            let board_size = Vec2::new(
                GRID_W as f32 * TILE + BOARD_PADDING,
                GRID_H as f32 * TILE + BOARD_PADDING,
            );
            let playfield_size = layout.playfield.size().max(Vec2::ONE);
            let world_units_per_pixel =
                (board_size.x / playfield_size.x).max(board_size.y / playfield_size.y);
            let logical_size = layout.viewport.size.max(Vec2::ONE);
            orthographic.scaling_mode = bevy::camera::ScalingMode::Fixed {
                width: logical_size.x * world_units_per_pixel,
                height: logical_size.y * world_units_per_pixel,
            };
            orthographic.scale = 1.0;

            // Camera coordinates are y-up, while GameLayout/UI coordinates are top-left/y-down.
            let playfield_center = layout.playfield.center();
            world_transform.translation.x =
                -(playfield_center.x - logical_size.x * 0.5) * world_units_per_pixel;
            world_transform.translation.y =
                (playfield_center.y - logical_size.y * 0.5) * world_units_per_pixel;
        } else {
            // World-space menus keep their established composition; only matches reserve HUD
            // chrome around the playfield.
            orthographic.scaling_mode = bevy::camera::ScalingMode::AutoMin {
                min_width: 600.0,
                min_height: 720.0,
            };
            orthographic.scale = 1.0;
            world_transform.translation.x = 0.0;
            world_transform.translation.y = 0.0;
        }
    }

    let final_logical_size = layout.viewport.size;
    let (mut sprite, mut transform) = canvas.into_inner();
    sprite.custom_size = Some(final_logical_size);
    transform.translation = Vec3::ZERO;
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
