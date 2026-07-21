use bevy::prelude::*;

use super::shop::Shop;
use super::{DragState, PendingSwap, RevertingSwap, SwapData, SwapHappened};
use crate::core::easing::damped_squash;
use crate::core::prelude::*;
use crate::input::pointer::PointerInput;
use crate::input::{InputActions, LastInputDevice};
use crate::state::TutorialModalState;
use crate::state::{MatchPhase, Overlay};

pub(crate) fn handle_input(
    mut commands: Commands,
    mut pending: ResMut<PendingSwap>,
    mut drag: ResMut<DragState>,
    mut next_state: ResMut<NextState<MatchPhase>>,
    pointer: Res<PointerInput>,
    shop: Res<Shop>,
    tutorial: Res<TutorialModalState>,
    layout: Res<GridLayout>,
    movable: Query<(), With<Movable>>,
    mut lights: Query<
        (Entity, &mut GridPos, Has<Selected>),
        (With<Light>, Without<Spark>, Without<BlocksInteraction>),
    >,
    interaction_blockers: Query<&GridPos, With<BlocksInteraction>>,
    mut sparks: Query<
        (Entity, &mut GridPos),
        (With<Spark>, Without<Light>, Without<BlocksInteraction>),
    >,
) {
    if tutorial.open {
        return;
    }
    // While a shop booster is armed, clicks target lights for the ability (`shop::shop_targeting`),
    // not the drag-swap — bail so the two don't both consume the same press.
    if shop.blocks_board_input() {
        return;
    }
    let Some(world) = pointer.position_world.or_else(|| {
        if pointer.just_released && drag.active {
            drag.last_world
        } else {
            None
        }
    }) else {
        return;
    };

    if pointer.just_pressed {
        if pending.0.is_some() {
            return;
        }

        drag.active = true;
        drag.start_world = world;
        drag.last_world = Some(world);
        drag.locked_axis = None;
        drag.neighbor_entity = None;
        drag.neighbor_grid = None;
        drag.neighbor_is_empty = false;
        let gp = to_grid(world).filter(|pos| layout.contains(*pos));
        drag.start_grid = gp;
        drag.start_entity = gp.and_then(|gp| {
            if interaction_blockers.iter().any(|p| *p == gp) {
                return None;
            }
            lights
                .iter()
                .find(|(_, p, _)| **p == gp)
                .map(|(e, _, _)| e)
                .filter(|e| movable.get(*e).is_ok())
                .or_else(|| sparks.iter().find(|(_, p)| **p == gp).map(|(e, _)| e))
        });
        if let Some(e) = drag.start_entity {
            commands.entity(e).insert(Selected);
        }
        return;
    }

    if drag.active && pointer.position_world.is_some() {
        drag.last_world = Some(world);
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
                let blocked = interaction_blockers.iter().any(|p| *p == ngp);
                drag.neighbor_entity = if blocked {
                    None
                } else {
                    lights
                        .iter()
                        .find(|(_, p, _)| **p == ngp)
                        .map(|(e, _, _)| e)
                        .filter(|e| movable.get(*e).is_ok())
                        .or_else(|| sparks.iter().find(|(_, p)| **p == ngp).map(|(e, _)| e))
                };
                drag.neighbor_is_empty =
                    layout.contains(ngp) && !blocked && drag.neighbor_entity.is_none();
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
            let (Some(start_e), Some(start_gp), Some(neighbor_gp)) =
                (drag.start_entity, drag.start_grid, drag.neighbor_grid)
            else {
                return;
            };
            let neighbor_e = drag.neighbor_entity;
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
        } else if let (Some(start_e), Some(start_gp)) = (drag.start_entity, drag.start_grid) {
            commands
                .entity(start_e)
                .insert(VisualPos(to_world(start_gp)));
            if let (Some(neighbor_e), Some(neighbor_gp)) =
                (drag.neighbor_entity, drag.neighbor_grid)
            {
                commands
                    .entity(neighbor_e)
                    .insert(VisualPos(to_world(neighbor_gp)));
            }
        }

        drag.locked_axis = None;
        drag.neighbor_entity = None;
        drag.neighbor_is_empty = false;
        drag.neighbor_grid = None;
        drag.last_world = None;
        drag.start_entity = None;
        drag.start_grid = None;
    }
}

/// Steady-state scale for a selected light — `SelectJelly` eases into this same value, so the
/// punch hands off to this flat highlight seamlessly once it finishes.
const SELECTED_SCALE: f32 = 1.15;

pub(crate) fn highlight_selected(
    mut lights: Query<(&mut Transform, Has<Selected>), (With<FallPhysics>, Without<SelectJelly>)>,
) {
    for (mut t, sel) in &mut lights {
        let target = if sel {
            Vec3::splat(SELECTED_SCALE)
        } else {
            Vec3::ONE
        };
        if t.scale != target {
            t.scale = target;
        }
    }
}

const SELECT_JELLY_DURATION: f32 = 0.5;
/// Peak squash/stretch deviation from `SELECTED_SCALE`, at the very first instant (t=0) — same
/// strength as a single punch would be, just spent on ringing down instead of one settle.
const SELECT_JELLY_AMOUNT: f32 = 0.35;
/// Number of full squash↔stretch cycles over `SELECT_JELLY_DURATION` — "3 rebotes".
const SELECT_JELLY_CYCLES: f32 = 3.0;

/// A squash-stretch bounce the instant a light is grabbed (tapped/dragged): starts fully deformed
/// and rings down through `SELECT_JELLY_CYCLES` decaying oscillations (each one gentler than the
/// last) before settling into `SELECTED_SCALE` — a damped spring, not a single ease-out settle, so
/// picking one up reads as actual jelly wobbling instead of one snap. Hands scale control back to
/// `highlight_selected` once it finishes.
#[derive(Component)]
pub(crate) struct SelectJelly {
    timer: Timer,
}

/// Fires the punch the frame a light becomes `Selected` — tap-select and the keyboard/gamepad
/// cursor path both insert `Selected` the same way, so this one hook covers both.
pub(crate) fn on_light_selected(mut commands: Commands, q: Query<Entity, Added<Selected>>) {
    for e in &q {
        commands.entity(e).try_insert(SelectJelly {
            timer: Timer::from_seconds(SELECT_JELLY_DURATION, TimerMode::Once),
        });
    }
}

pub(crate) fn tick_select_jelly(
    mut commands: Commands,
    mut q: Query<(Entity, &mut Transform, &mut SelectJelly, Has<Selected>)>,
    time: Res<Time>,
) {
    for (e, mut t, mut jelly, sel) in &mut q {
        if !sel {
            // Deselected before the punch finished (fast tap-then-drag-away) — bail immediately so
            // `highlight_selected` regains control and snaps back to rest next frame instead of
            // fighting over `Transform::scale`.
            commands.entity(e).try_remove::<SelectJelly>();
            continue;
        }
        jelly.timer.tick(time.delta());
        let frac = jelly.timer.fraction();
        // Same damped-spring shape as `visuals::bounce`'s landing bounce, via the shared
        // `damped_squash` — starts at full amplitude (matching a grab impact) and rings through
        // `SELECT_JELLY_CYCLES` squash↔stretch swings, each smaller than the last.
        let squash = damped_squash(frac, SELECT_JELLY_AMOUNT, SELECT_JELLY_CYCLES);
        t.scale = Vec3::new(SELECTED_SCALE + squash, SELECTED_SCALE - squash, 1.0);
        if jelly.timer.is_finished() {
            t.scale = Vec3::splat(SELECTED_SCALE);
            commands.entity(e).try_remove::<SelectJelly>();
        }
    }
}

/// Commits a *validated* swap: records it in `PendingSwap`, moves both pieces' logical `GridPos`
/// (the visuals lerp the rest) and enters `SwapAnimating`. Shared by the mouse drag-release path
/// and the keyboard/gamepad cursor path so they can't diverge. `neighbor_e == None` is a swap into
/// an empty cell, which the downstream pipeline already handles.
pub(crate) fn commit_swap(
    pending: &mut PendingSwap,
    next_state: &mut NextState<MatchPhase>,
    lights: &mut Query<
        (Entity, &mut GridPos, Has<Selected>),
        (With<Light>, Without<Spark>, Without<BlocksInteraction>),
    >,
    sparks: &mut Query<
        (Entity, &mut GridPos),
        (With<Spark>, Without<Light>, Without<BlocksInteraction>),
    >,
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
    next_state.set(MatchPhase::SwapAnimating);
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
    lights: &Query<
        (Entity, &mut GridPos, Has<Selected>),
        (With<Light>, Without<Spark>, Without<BlocksInteraction>),
    >,
    sparks: &Query<
        (Entity, &mut GridPos),
        (With<Spark>, Without<Light>, Without<BlocksInteraction>),
    >,
) -> Option<Entity> {
    lights
        .iter()
        .find(|(_, p, _)| **p == gp)
        .map(|(e, _, _)| e)
        .or_else(|| sparks.iter().find(|(_, p)| **p == gp).map(|(e, _)| e))
}

/// Lights advertise swappability through `Movable`; sparks are the one non-Light piece that can
/// be moved by the player. Keeping this check capability-based prevents obstacle subtypes from
/// leaking into either input path.
fn is_movable_piece(
    entity: Entity,
    movable: &Query<(), With<Movable>>,
    sparks: &Query<
        (Entity, &mut GridPos),
        (With<Spark>, Without<Light>, Without<BlocksInteraction>),
    >,
) -> bool {
    movable.get(entity).is_ok() || sparks.get(entity).is_ok()
}

pub(crate) fn board_cursor_input(
    actions: Res<InputActions>,
    mut cursor: ResMut<BoardCursor>,
    mut commands: Commands,
    mut pending: ResMut<PendingSwap>,
    mut next_state: ResMut<NextState<MatchPhase>>,
    tutorial: Res<TutorialModalState>,
    layout: Res<GridLayout>,
    movable: Query<(), With<Movable>>,
    mut lights: Query<
        (Entity, &mut GridPos, Has<Selected>),
        (With<Light>, Without<Spark>, Without<BlocksInteraction>),
    >,
    mut sparks: Query<
        (Entity, &mut GridPos),
        (With<Spark>, Without<Light>, Without<BlocksInteraction>),
    >,
    interaction_blockers: Query<&GridPos, With<BlocksInteraction>>,
) {
    if tutorial.open {
        return;
    }
    // Never interfere mid-swap.
    if pending.0.is_some() {
        return;
    }
    if !(actions.any_nav() || actions.confirm || actions.cancel) {
        return;
    }

    let is_valid_cell = |gp: GridPos| layout.contains(gp);
    let is_blocked = |gp: GridPos| interaction_blockers.iter().any(|p| *p == gp);

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
            if is_valid_cell(to)
                && !is_blocked(from)
                && !is_blocked(to)
                && let Some(start_e) = entity_at(from, &lights, &sparks)
            {
                let neighbor_e = entity_at(to, &lights, &sparks);
                if is_movable_piece(start_e, &movable, &sparks)
                    && neighbor_e.is_none_or(|e| is_movable_piece(e, &movable, &sparks))
                {
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
            }
        } else {
            let target = GridPos {
                x: cursor.pos.x + d.x,
                y: cursor.pos.y + d.y,
            };
            if is_valid_cell(target) {
                cursor.pos = target;
            }
        }
    }

    if actions.confirm {
        if cursor.picked {
            clear_selected(&mut commands, &lights);
            cursor.picked = false;
        } else if !is_blocked(cursor.pos)
            && let Some(e) = entity_at(cursor.pos, &lights, &sparks)
            && is_movable_piece(e, &movable, &sparks)
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
    lights: &Query<
        (Entity, &mut GridPos, Has<Selected>),
        (With<Light>, Without<Spark>, Without<BlocksInteraction>),
    >,
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
    state: Option<Res<State<MatchPhase>>>,
    overlay: Res<State<Overlay>>,
    time: Res<Time>,
    highlight: Single<(&mut Transform, &mut Visibility), With<CursorHighlight>>,
) {
    let (mut t, mut vis) = highlight.into_inner();
    let show = state.is_some_and(|s| *s.get() == MatchPhase::Playing)
        && *overlay.get() == Overlay::None
        && *last == LastInputDevice::Cursor;
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
    mut reverting: ResMut<RevertingSwap>,
    mut next_state: ResMut<NextState<MatchPhase>>,
    lights: Query<(&GridPos, &VisualPos), With<Light>>,
    fallables: Query<(&GridPos, &VisualPos), With<Spark>>,
) {
    let Some(ref swap) = pending.0 else {
        if reverting.0.is_empty() {
            return;
        }
        let done = reverting
            .0
            .iter()
            .all(|&e| visual_at_grid(e, &lights, &fallables));
        if done {
            reverting.0.clear();
            next_state.set(MatchPhase::Playing);
        }
        return;
    };
    let a_done = visual_at_grid(swap.a, &lights, &fallables);

    let b_done = match swap.b {
        None => true,
        Some(b_ent) => visual_at_grid(b_ent, &lights, &fallables),
    };

    if a_done && b_done {
        commands.trigger(SwapHappened);
    }
}

fn visual_at_grid(
    entity: Entity,
    lights: &Query<(&GridPos, &VisualPos), With<Light>>,
    fallables: &Query<(&GridPos, &VisualPos), With<Spark>>,
) -> bool {
    let Some((gp, vp)) = lights
        .get(entity)
        .ok()
        .or_else(|| fallables.get(entity).ok())
    else {
        return false;
    };
    vp.0.distance(to_world(*gp)) < TILE * 0.05
}
