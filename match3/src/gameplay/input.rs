use bevy::prelude::*;

use super::shop::Shop;
use super::{DragState, PendingSwap, SwapData, SwapHappened};
use crate::core::prelude::*;
use crate::input::pointer::PointerInput;
use crate::input::{InputActions, LastInputDevice};
use crate::state::GameState;

pub(crate) fn handle_input(
    mut commands: Commands,
    mut pending: ResMut<PendingSwap>,
    mut drag: ResMut<DragState>,
    mut next_state: ResMut<NextState<GameState>>,
    pointer: Res<PointerInput>,
    shop: Res<Shop>,
    mut lights: Query<(Entity, &mut GridPos, Has<Selected>), (With<Light>, Without<Spark>)>,
    shadow_q: Query<&GridPos, (With<Shadow>, Without<Light>, Without<Spark>)>,
    mut sparks: Query<(Entity, &mut GridPos), (With<Spark>, Without<Light>)>,
) {
    // While a shop booster is armed, clicks target lights for the ability (`shop::shop_targeting`),
    // not the drag-swap — bail so the two don't both consume the same press.
    if shop.is_armed() {
        return;
    }
    let Some(world) = pointer.position_world else {
        return;
    };

    if pointer.just_pressed {
        if pending.0.is_some() {
            return;
        }

        drag.active = true;
        drag.start_world = world;
        drag.locked_axis = None;
        drag.neighbor_entity = None;
        drag.neighbor_grid = None;
        drag.neighbor_is_empty = false;
        let gp = to_grid(world);
        drag.start_grid = gp;
        drag.start_entity = gp.and_then(|gp| {
            if shadow_q.iter().any(|p| *p == gp) {
                return None;
            }
            lights
                .iter()
                .find(|(_, p, _)| **p == gp)
                .map(|(e, _, _)| e)
                .or_else(|| sparks.iter().find(|(_, p)| **p == gp).map(|(e, _)| e))
        });
        if let Some(e) = drag.start_entity {
            commands.entity(e).insert(Selected);
        }
        return;
    }

    if drag.active && drag.locked_axis.is_none() && !pointer.just_released {
        let delta = world - drag.start_world;
        if delta.length() > 8.0 {
            let dir = if delta.x.abs() >= delta.y.abs() {
                IVec2::new(delta.x.signum() as i32, 0)
            } else {
                IVec2::new(0, delta.y.signum() as i32)
            };
            drag.locked_axis = Some(dir);
            if let Some(start_gp) = drag.start_grid {
                let ngp = GridPos {
                    x: start_gp.x + dir.x,
                    y: start_gp.y + dir.y,
                };
                drag.neighbor_grid = Some(ngp);
                let blocked = shadow_q.iter().any(|p| *p == ngp);
                drag.neighbor_entity = if blocked {
                    None
                } else {
                    lights
                        .iter()
                        .find(|(_, p, _)| **p == ngp)
                        .map(|(e, _, _)| e)
                        .or_else(|| sparks.iter().find(|(_, p)| **p == ngp).map(|(e, _)| e))
                };
                let in_bounds = ngp.x >= 0 && ngp.x < GRID_W && ngp.y >= 0 && ngp.y < GRID_H;
                drag.neighbor_is_empty = in_bounds && !blocked && drag.neighbor_entity.is_none();
            }
        }
    }

    if pointer.just_released && drag.active {
        drag.active = false;
        let delta = world - drag.start_world;

        let all_lights: Vec<Entity> = lights.iter().map(|(e, _, _)| e).collect();
        for e in all_lights {
            commands.entity(e).remove::<Selected>();
        }

        let should_swap = if let (Some(dir), Some(_start_gp), Some(_neighbor_gp), Some(_start_e)) = (
            drag.locked_axis,
            drag.start_grid,
            drag.neighbor_grid,
            drag.start_entity,
        ) {
            if drag.neighbor_entity.is_none() && !drag.neighbor_is_empty {
                false
            } else {
                let proj = if dir.x != 0 { delta.x } else { delta.y };
                let dir_sign = (if dir.x != 0 { dir.x } else { dir.y }) as f32;
                proj * dir_sign > TILE * 0.35
            }
        } else {
            false
        };

        if should_swap {
            let start_e = drag.start_entity.unwrap();
            let neighbor_e = drag.neighbor_entity;
            let start_gp = drag.start_grid.unwrap();
            let neighbor_gp = drag.neighbor_grid.unwrap();
            commit_swap(
                &mut pending,
                &mut next_state,
                &mut lights,
                &mut sparks,
                start_e,
                start_gp,
                neighbor_e,
                neighbor_gp,
            );
        }

        drag.locked_axis = None;
        drag.neighbor_entity = None;
        drag.neighbor_is_empty = false;
        drag.neighbor_grid = None;
        drag.start_entity = None;
        drag.start_grid = None;
    }
}

pub(crate) fn highlight_selected(
    mut lights: Query<(&mut Transform, Has<Selected>), With<FallPhysics>>,
) {
    for (mut t, sel) in &mut lights {
        t.scale = if sel { Vec3::splat(1.15) } else { Vec3::ONE };
    }
}

/// Commits a *validated* swap: records it in `PendingSwap`, moves both pieces' logical `GridPos`
/// (the visuals lerp the rest) and enters `SwapAnimating`. Shared by the mouse drag-release path
/// and the keyboard/gamepad cursor path so they can't diverge. `neighbor_e == None` is a swap into
/// an empty cell, which the downstream pipeline already handles.
pub(crate) fn commit_swap(
    pending: &mut PendingSwap,
    next_state: &mut NextState<GameState>,
    lights: &mut Query<(Entity, &mut GridPos, Has<Selected>), (With<Light>, Without<Spark>)>,
    sparks: &mut Query<(Entity, &mut GridPos), (With<Spark>, Without<Light>)>,
    start_e: Entity,
    start_gp: GridPos,
    neighbor_e: Option<Entity>,
    neighbor_gp: GridPos,
) {
    pending.0 = Some(SwapData {
        a: start_e,
        b: neighbor_e,
        a_pos: start_gp,
        b_pos: neighbor_gp,
        free: false,
    });
    if let Ok((_, mut pos, _)) = lights.get_mut(start_e) {
        pos.set_if_neq(neighbor_gp);
    } else if let Ok((_, mut pos)) = sparks.get_mut(start_e) {
        pos.set_if_neq(neighbor_gp);
    }
    if let Some(neighbor_e) = neighbor_e {
        if let Ok((_, mut pos, _)) = lights.get_mut(neighbor_e) {
            pos.set_if_neq(start_gp);
        } else if let Ok((_, mut pos)) = sparks.get_mut(neighbor_e) {
            pos.set_if_neq(start_gp);
        }
    }
    next_state.set(GameState::SwapAnimating);
}

/// The keyboard/gamepad cursor over the board: a highlighted cell the player moves with nav, picks
/// up with confirm, and swaps by pressing a direction while picked. Mutually friendly with the
/// mouse path (`handle_input`) — players use whichever; `LastInputDevice` decides which cursor
/// shows.
#[derive(Resource)]
pub(crate) struct BoardCursor {
    pub(crate) pos: GridPos,
    /// `true` once the player has "picked up" the cell — the next direction commits a swap.
    pub(crate) picked: bool,
}

impl Default for BoardCursor {
    fn default() -> Self {
        Self {
            pos: GridPos {
                x: GRID_W / 2,
                y: GRID_H / 2,
            },
            picked: false,
        }
    }
}

/// The visual square that marks `BoardCursor::pos`. Spawned once at startup; shown only while
/// playing with keyboard/gamepad.
#[derive(Component)]
pub(crate) struct CursorHighlight;

fn entity_at(
    gp: GridPos,
    lights: &Query<(Entity, &mut GridPos, Has<Selected>), (With<Light>, Without<Spark>)>,
    sparks: &Query<(Entity, &mut GridPos), (With<Spark>, Without<Light>)>,
) -> Option<Entity> {
    lights
        .iter()
        .find(|(_, p, _)| **p == gp)
        .map(|(e, _, _)| e)
        .or_else(|| sparks.iter().find(|(_, p)| **p == gp).map(|(e, _)| e))
}

pub(crate) fn board_cursor_input(
    actions: Res<InputActions>,
    mut cursor: ResMut<BoardCursor>,
    mut commands: Commands,
    mut pending: ResMut<PendingSwap>,
    mut next_state: ResMut<NextState<GameState>>,
    mut lights: Query<(Entity, &mut GridPos, Has<Selected>), (With<Light>, Without<Spark>)>,
    mut sparks: Query<(Entity, &mut GridPos), (With<Spark>, Without<Light>)>,
    shadow_q: Query<&GridPos, (With<Shadow>, Without<Light>, Without<Spark>)>,
) {
    // Never interfere mid-swap.
    if pending.0.is_some() {
        return;
    }
    if !(actions.any_nav() || actions.confirm || actions.cancel) {
        return;
    }

    let in_bounds = |gp: GridPos| gp.x >= 0 && gp.x < GRID_W && gp.y >= 0 && gp.y < GRID_H;
    let is_shadow = |gp: GridPos| shadow_q.iter().any(|p| *p == gp);

    // Cancel: drop whatever's picked.
    if actions.cancel && cursor.picked {
        clear_selected(&mut commands, &lights);
        cursor.picked = false;
        return;
    }

    if let Some(d) = actions.nav_delta() {
        if cursor.picked {
            // A direction while holding = try to swap toward that neighbor.
            let from = cursor.pos;
            let to = GridPos {
                x: from.x + d.x,
                y: from.y + d.y,
            };
            if in_bounds(to)
                && !is_shadow(from)
                && !is_shadow(to)
                && let Some(start_e) = entity_at(from, &lights, &sparks)
            {
                let neighbor_e = entity_at(to, &lights, &sparks);
                clear_selected(&mut commands, &lights);
                commit_swap(
                    &mut pending,
                    &mut next_state,
                    &mut lights,
                    &mut sparks,
                    start_e,
                    from,
                    neighbor_e,
                    to,
                );
                cursor.pos = to;
                cursor.picked = false;
            }
        } else {
            cursor.pos.x = (cursor.pos.x + d.x).clamp(0, GRID_W - 1);
            cursor.pos.y = (cursor.pos.y + d.y).clamp(0, GRID_H - 1);
        }
    }

    if actions.confirm {
        if cursor.picked {
            clear_selected(&mut commands, &lights);
            cursor.picked = false;
        } else if !is_shadow(cursor.pos)
            && let Some(e) = entity_at(cursor.pos, &lights, &sparks)
        {
            clear_selected(&mut commands, &lights);
            commands.entity(e).insert(Selected);
            cursor.picked = true;
        }
    }
}

/// Removes `Selected` from every light (so picking can reuse the same scale-up highlight as the
/// mouse path without leaving a stuck selection when switching devices).
fn clear_selected(
    commands: &mut Commands,
    lights: &Query<(Entity, &mut GridPos, Has<Selected>), (With<Light>, Without<Spark>)>,
) {
    for (e, _, sel) in lights.iter() {
        if sel {
            commands.entity(e).remove::<Selected>();
        }
    }
}

/// Spawns the single cursor-highlight square (hidden until a keyboard/gamepad device is in use).
pub(crate) fn setup_board_cursor(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let mesh = meshes.add(Rectangle::new(TILE * 0.96, TILE * 0.96));
    let mat = materials.add(ColorMaterial::from_color(Color::srgba(
        0.45, 0.85, 1.0, 0.18,
    )));
    commands.spawn((
        CursorHighlight,
        Mesh2d(mesh),
        MeshMaterial2d(mat),
        Transform::from_xyz(0.0, 0.0, 2.0),
        Visibility::Hidden,
    ));
}

/// Positions the cursor square at `BoardCursor::pos` and shows it only while playing with a
/// keyboard/gamepad; a subtle pulse signals the "picked" state.
pub(crate) fn update_board_cursor(
    cursor: Res<BoardCursor>,
    last: Res<LastInputDevice>,
    state: Res<State<GameState>>,
    time: Res<Time>,
    highlight: Single<(&mut Transform, &mut Visibility), With<CursorHighlight>>,
) {
    let (mut t, mut vis) = highlight.into_inner();
    let show = *state.get() == GameState::Playing && *last == LastInputDevice::Cursor;
    *vis = if show {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    if show {
        let p = to_world(cursor.pos);
        t.translation = Vec3::new(p.x, p.y, 2.0);
        let s = if cursor.picked {
            1.0 + (time.elapsed_secs() * 8.0).sin() * 0.06
        } else {
            1.0
        };
        t.scale = Vec3::splat(s);
    }
}

pub(crate) fn check_swap_visual_done(
    mut commands: Commands,
    pending: Res<PendingSwap>,
    lights: Query<(&GridPos, &VisualPos), With<Light>>,
    fallables: Query<(&GridPos, &VisualPos), With<Spark>>,
) {
    let Some(ref swap) = pending.0 else {
        return;
    };
    let (gp_a, vp_a) = if let Ok(x) = lights.get(swap.a) {
        x
    } else if let Ok(x) = fallables.get(swap.a) {
        x
    } else {
        return;
    };
    let a_done = vp_a.0.distance(to_world(*gp_a)) < TILE * 0.05;

    let b_done = match swap.b {
        None => true,
        Some(b_ent) => {
            let Some((gp_b, vp_b)) = lights.get(b_ent).ok().or_else(|| fallables.get(b_ent).ok())
            else {
                return;
            };
            vp_b.0.distance(to_world(*gp_b)) < TILE * 0.05
        }
    };

    if a_done && b_done {
        commands.trigger(SwapHappened);
    }
}
