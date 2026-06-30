use bevy::input::touch::Touches;
use bevy::prelude::*;

use super::LastInputDevice;
use crate::visuals::render_target::{FinalCamera, WorldCamera, window_point_to_world};

#[derive(Default, Clone, Copy, PartialEq)]
pub(crate) enum PointerSource {
    #[default]
    Mouse,
    Touch,
}

/// Estado del puntero primario por frame — unifica mouse y touch. Poblado en `PreUpdate` por
/// `gather_pointer_input`; todos los consumers del tablero leen de aquí en lugar de hacer queries
/// directas a `ButtonInput<MouseButton>` o `cursor_position()`.
#[derive(Resource, Default)]
pub(crate) struct PointerInput {
    pub(crate) just_pressed: bool,
    pub(crate) just_released: bool,
    pub(crate) held: bool,
    /// Última posición conocida en píxeles de ventana (None si el cursor está fuera de la ventana
    /// y no hay contacto táctil activo).
    pub(crate) position_window: Option<Vec2>,
    /// Posición ya convertida al espacio-mundo a través del RTT (via `window_point_to_world`).
    pub(crate) position_world: Option<Vec2>,
    pub(crate) source: PointerSource,
}

/// Rellena `PointerInput` cada frame en `PreUpdate`, después de que `gather_input` haya procesado
/// teclado/gamepad. Touch tiene prioridad sobre mouse dentro del mismo frame: en dispositivos puros
/// (Android/iOS) el mouse nunca genera eventos, así que no hay conflicto; en hybrid-touch el
/// último en establecer `LastInputDevice` gana (este sistema corre último).
pub(crate) fn gather_pointer_input(
    mut pointer: ResMut<PointerInput>,
    mut last: ResMut<LastInputDevice>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform), With<WorldCamera>>,
    final_camera: Single<&Camera, With<FinalCamera>>,
) {
    let (cam, cam_t) = *camera;
    let final_cam = *final_camera;
    let (vp_pos, vp_size) = if let Some(ref viewport) = final_cam.viewport {
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
    };

    let mut next = PointerInput::default();

    // Touch — solo contacto primario (el de menor ID entre los activos).
    let press_pos = touches
        .iter_just_pressed()
        .min_by_key(|t| t.id())
        .map(|t| t.position());
    let release_pos = touches
        .iter_just_released()
        .min_by_key(|t| t.id())
        .map(|t| t.position());
    let held_pos = touches.iter().min_by_key(|t| t.id()).map(|t| t.position());
    let had_touch = press_pos.is_some() || release_pos.is_some() || held_pos.is_some();

    if had_touch {
        next.source = PointerSource::Touch;
        next.just_pressed = press_pos.is_some();
        next.just_released = release_pos.is_some();
        next.held = held_pos.is_some();
        let win_pos = press_pos.or(held_pos).or(release_pos);
        next.position_window = win_pos;
        if let Some(pos) = win_pos {
            let local_pos = pos - vp_pos;
            next.position_world = window_point_to_world(cam, cam_t, vp_size, local_pos);
        }
        *last = LastInputDevice::Touch;
    } else {
        // Mouse
        next.source = PointerSource::Mouse;
        next.just_pressed = mouse.just_pressed(MouseButton::Left);
        next.just_released = mouse.just_released(MouseButton::Left);
        next.held = mouse.pressed(MouseButton::Left);
        if let Some(pos) = window.cursor_position() {
            next.position_window = Some(pos);
            let local_pos = pos - vp_pos;
            next.position_world = window_point_to_world(cam, cam_t, vp_size, local_pos);
        }
    }

    *pointer = next;
}
