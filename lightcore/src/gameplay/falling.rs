use bevy::prelude::*;
use std::collections::HashSet;

use super::{
    CoreReserve, DisplayedScore, FallComplete, GameMode, GravitySettled, Score, SparksCollected,
};
use crate::core::prelude::*;
use crate::core::run::RunState;
use crate::state::GameState;

/// Marks an entity actively dropping under gravity — stays inserted through the visual
/// settle period between rows, not just the instant a row-step happens. Read by
/// `visuals::motion::lerp_visual_pos` to accelerate falls the longer they continue.
#[derive(Component)]
pub(crate) struct Dropping;

pub(crate) fn reset_gravity(mut settled: ResMut<GravitySettled>) {
    settled.0 = false;
}

fn in_bounds(x: i32, y: i32) -> bool {
    (0..GRID_W).contains(&x) && (0..GRID_H).contains(&y)
}

#[cfg(test)]
fn choose_fall_target(
    pos: GridPos,
    occupied: &HashSet<(i32, i32)>,
    shadow_set: &HashSet<(i32, i32)>,
) -> Option<GridPos> {
    if shadow_set.contains(&(pos.x, pos.y)) || pos.y <= 0 {
        return None;
    }

    straight_fall_target(pos, occupied, shadow_set)
        .or_else(|| diagonal_fall_target(pos, occupied, shadow_set))
}

fn straight_fall_target(
    pos: GridPos,
    occupied: &HashSet<(i32, i32)>,
    shadow_set: &HashSet<(i32, i32)>,
) -> Option<GridPos> {
    if shadow_set.contains(&(pos.x, pos.y)) || pos.y <= 0 {
        return None;
    }

    let below = (pos.x, pos.y - 1);
    if !shadow_set.contains(&below) && !occupied.contains(&below) {
        Some(GridPos {
            x: pos.x,
            y: pos.y - 1,
        })
    } else {
        None
    }
}

fn diagonal_fall_target(
    pos: GridPos,
    occupied: &HashSet<(i32, i32)>,
    shadow_set: &HashSet<(i32, i32)>,
) -> Option<GridPos> {
    if shadow_set.contains(&(pos.x, pos.y)) || pos.y <= 0 {
        return None;
    }

    let below = (pos.x, pos.y - 1);
    if !shadow_set.contains(&below) && !occupied.contains(&below) {
        return None;
    }

    for dx in [-1, 1] {
        let diag = (pos.x + dx, pos.y - 1);
        if !in_bounds(diag.0, diag.1) {
            continue;
        }
        if shadow_set.contains(&diag) || occupied.contains(&diag) {
            continue;
        }
        if !vertical_feed_blocked_for_target(diag, occupied, shadow_set) {
            continue;
        }
        return Some(GridPos {
            x: diag.0,
            y: diag.1,
        });
    }

    None
}

#[allow(dead_code)]
fn ingredient_diagonal_fall_target(
    pos: GridPos,
    occupied: &HashSet<(i32, i32)>,
    shadow_set: &HashSet<(i32, i32)>,
) -> Option<GridPos> {
    if shadow_set.contains(&(pos.x, pos.y)) || pos.y <= 0 {
        return None;
    }

    let below = (pos.x, pos.y - 1);
    if !shadow_set.contains(&below) && !occupied.contains(&below) {
        return None;
    }

    for dx in [-1, 1] {
        let diag = (pos.x + dx, pos.y - 1);
        if !in_bounds(diag.0, diag.1) {
            continue;
        }
        if shadow_set.contains(&diag) || occupied.contains(&diag) {
            continue;
        }
        if target_has_vertical_feed(diag, occupied, shadow_set) {
            continue;
        }
        return Some(GridPos {
            x: diag.0,
            y: diag.1,
        });
    }

    None
}

#[allow(dead_code)]
fn target_has_vertical_feed(
    target: (i32, i32),
    occupied: &HashSet<(i32, i32)>,
    shadow_set: &HashSet<(i32, i32)>,
) -> bool {
    let (x, y) = target;
    for scan_y in y + 1..GRID_H {
        let cell = (x, scan_y);
        if shadow_set.contains(&cell) {
            return false;
        }
        if occupied.contains(&cell) {
            return true;
        }
    }
    false
}

fn vertical_feed_blocked_for_target(
    target: (i32, i32),
    occupied: &HashSet<(i32, i32)>,
    shadow_set: &HashSet<(i32, i32)>,
) -> bool {
    let (x, y) = target;
    for scan_y in y + 1..GRID_H {
        let cell = (x, scan_y);
        if shadow_set.contains(&cell) {
            return true;
        }
        if occupied.contains(&cell) {
            return false;
        }
    }
    false
}

pub(crate) fn update_shadow_set(
    shadow_q: Query<&GridPos, With<Shadow>>,
    added_shadows: Query<(), Added<Shadow>>,
    mut removed_shadows: RemovedComponents<Shadow>,
    mut shadow_set: ResMut<ShadowSet>,
) {
    if !added_shadows.is_empty() || removed_shadows.read().next().is_some() {
        shadow_set.0 = shadow_q.iter().map(|p| (p.x, p.y)).collect();
    }
}

pub(crate) fn apply_gravity(
    mut commands: Commands,
    mut settled: ResMut<GravitySettled>,
    mut entities: Query<
        (Entity, &mut GridPos, &VisualPos, Has<Spark>),
        (With<FallPhysics>, Without<PopAnim>),
    >,
    shadow_set: Res<ShadowSet>,
) {
    let shadow_set = &shadow_set.0;

    let mut sorted: Vec<(Entity, GridPos, Vec3, bool)> = entities
        .iter()
        .map(|(e, p, v, is_spark)| (e, *p, v.0, is_spark))
        .collect();
    sorted.sort_by_key(|(_, p, _, _)| p.y);
    let mut occupied: HashSet<(i32, i32)> = sorted.iter().map(|(_, p, _, _)| (p.x, p.y)).collect();
    let mut any_moved = false;
    let mut any_unsettled = false;

    // Each piece advances based on its own VisualPos catching up to its GridPos,
    // not the whole board's — so a deep column doesn't hold back a shallow one.
    let mut blocked_for_diagonal = Vec::new();
    for (e, pos, vis, is_spark) in &sorted {
        let unsettled = vis.distance(to_world(*pos)) >= TILE * 0.02;
        if unsettled {
            any_unsettled = true;
        }

        if shadow_set.contains(&(pos.x, pos.y)) {
            commands.entity(*e).remove::<Dropping>();
            continue;
        }
        if unsettled {
            continue;
        }

        if let Some(target) = straight_fall_target(*pos, &occupied, shadow_set) {
            occupied.remove(&(pos.x, pos.y));
            occupied.insert((target.x, target.y));
            entities.get_mut(*e).unwrap().1.set_if_neq(target);
            commands.entity(*e).insert(Dropping);
            any_moved = true;
        } else {
            blocked_for_diagonal.push((*e, *pos, *is_spark));
        }
    }

    for (e, pos, is_spark) in blocked_for_diagonal {
        if let Some(target) = straight_fall_target(pos, &occupied, shadow_set) {
            occupied.remove(&(pos.x, pos.y));
            occupied.insert((target.x, target.y));
            entities.get_mut(e).unwrap().1.set_if_neq(target);
            commands.entity(e).insert(Dropping);
            any_moved = true;
        } else if let Some(target) = if is_spark {
            None
        } else {
            diagonal_fall_target(pos, &occupied, shadow_set)
        } {
            occupied.remove(&(pos.x, pos.y));
            occupied.insert((target.x, target.y));
            entities.get_mut(e).unwrap().1.set_if_neq(target);
            commands.entity(e).insert(Dropping);
            any_moved = true;
        } else {
            commands.entity(e).remove::<Dropping>();
        }
    }

    if any_moved || any_unsettled {
        settled.0 = false;
    } else if !settled.0 {
        settled.0 = true;
        commands.trigger(FallComplete);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn occupied(cells: &[(i32, i32)]) -> HashSet<(i32, i32)> {
        cells.iter().copied().collect()
    }

    fn shadows(cells: &[(i32, i32)]) -> HashSet<(i32, i32)> {
        cells.iter().copied().collect()
    }

    #[test]
    fn falls_straight_down_when_cell_below_is_open() {
        let next = choose_fall_target(GridPos { x: 3, y: 4 }, &occupied(&[(3, 4)]), &shadows(&[]));
        assert_eq!(next, Some(GridPos { x: 3, y: 3 }));
    }

    #[test]
    fn slides_diagonally_around_shadow_when_left_path_is_open() {
        let next = choose_fall_target(
            GridPos { x: 3, y: 4 },
            &occupied(&[(3, 4)]),
            &shadows(&[(3, 3), (2, 4)]),
        );
        assert_eq!(next, Some(GridPos { x: 2, y: 3 }));
    }

    #[test]
    fn does_not_slide_into_target_with_normal_vertical_feed() {
        let next = choose_fall_target(
            GridPos { x: 3, y: 4 },
            &occupied(&[(3, 4)]),
            &shadows(&[(3, 3)]),
        );
        assert_eq!(next, None);
    }

    #[test]
    fn does_not_slide_around_normal_piece_below() {
        let next = choose_fall_target(
            GridPos { x: 3, y: 4 },
            &occupied(&[(3, 4), (3, 3)]),
            &shadows(&[]),
        );
        assert_eq!(next, None);
    }

    #[test]
    fn slides_when_normal_piece_below_sits_on_static_blocker() {
        let next = choose_fall_target(
            GridPos { x: 3, y: 4 },
            &occupied(&[(3, 4), (3, 3)]),
            &shadows(&[(3, 2), (2, 4)]),
        );
        assert_eq!(next, Some(GridPos { x: 2, y: 3 }));
    }

    #[test]
    fn slides_from_normal_stack_into_target_blocked_by_shadow_above() {
        let next = choose_fall_target(
            GridPos { x: 3, y: 4 },
            &occupied(&[(3, 4), (3, 3)]),
            &shadows(&[(2, 4)]),
        );
        assert_eq!(next, Some(GridPos { x: 2, y: 3 }));
    }

    #[test]
    fn does_not_slide_when_static_blocker_is_buried_under_stack() {
        let next = choose_fall_target(
            GridPos { x: 3, y: 5 },
            &occupied(&[(3, 5), (3, 4), (3, 3)]),
            &shadows(&[(3, 2)]),
        );
        assert_eq!(next, None);
    }

    #[test]
    fn stays_put_when_both_diagonals_are_blocked() {
        let next = choose_fall_target(
            GridPos { x: 3, y: 4 },
            &occupied(&[(3, 4), (2, 4), (4, 4), (2, 3), (4, 3)]),
            &shadows(&[(3, 3)]),
        );
        assert_eq!(next, None);
    }

    #[test]
    fn prefers_left_diagonal_when_both_sides_are_open() {
        let next = choose_fall_target(GridPos { x: 3, y: 4 }, &occupied(&[(3, 4)]), &shadows(&[]));
        assert_eq!(next, Some(GridPos { x: 3, y: 3 }));
    }

    #[test]
    fn prefers_left_diagonal_when_static_blocker_blocks_vertical() {
        let next = choose_fall_target(
            GridPos { x: 3, y: 4 },
            &occupied(&[(3, 4)]),
            &shadows(&[(3, 3), (2, 4)]),
        );
        assert_eq!(next, Some(GridPos { x: 2, y: 3 }));
    }

    #[test]
    fn does_not_slide_out_of_bounds_around_normal_piece() {
        let next = choose_fall_target(
            GridPos { x: 0, y: 2 },
            &occupied(&[(0, 2), (0, 1)]),
            &shadows(&[]),
        );
        assert_eq!(next, None);
    }

    #[test]
    fn slides_in_bounds_around_edge_static_blocker() {
        let next = choose_fall_target(
            GridPos { x: 0, y: 2 },
            &occupied(&[(0, 2)]),
            &shadows(&[(0, 1), (1, 2)]),
        );
        assert_eq!(next, Some(GridPos { x: 1, y: 1 }));
    }

    #[test]
    fn spark_like_piece_uses_same_rule() {
        let start = GridPos { x: 5, y: 5 };
        let mut board = HashMap::from([((5, 5), start)]);
        let next = choose_fall_target(
            start,
            &board.keys().copied().collect(),
            &shadows(&[(5, 4), (4, 5)]),
        );
        assert_eq!(next, Some(GridPos { x: 4, y: 4 }));
        board.remove(&(5, 5));
        board.insert((4, 4), GridPos { x: 4, y: 4 });
        assert!(board.contains_key(&(4, 4)));
    }

    #[test]
    fn ingredient_prefers_vertical_before_diagonal() {
        let next =
            straight_fall_target(GridPos { x: 7, y: 4 }, &occupied(&[(7, 4)]), &shadows(&[]))
                .or_else(|| {
                    ingredient_diagonal_fall_target(
                        GridPos { x: 7, y: 4 },
                        &occupied(&[(7, 4)]),
                        &shadows(&[]),
                    )
                });

        assert_eq!(next, Some(GridPos { x: 7, y: 3 }));
    }

    #[test]
    fn ingredient_can_slide_diagonally_around_blocked_exit_column() {
        let next = straight_fall_target(
            GridPos { x: 7, y: 4 },
            &occupied(&[(7, 4)]),
            &shadows(&[(7, 3)]),
        )
        .or_else(|| {
            ingredient_diagonal_fall_target(
                GridPos { x: 7, y: 4 },
                &occupied(&[(7, 4)]),
                &shadows(&[(7, 3)]),
            )
        });

        assert_eq!(next, Some(GridPos { x: 6, y: 3 }));
    }

    #[test]
    fn ingredient_does_not_slide_before_vertical_feed() {
        let next = straight_fall_target(
            GridPos { x: 7, y: 4 },
            &occupied(&[(7, 4), (6, 5)]),
            &shadows(&[(7, 3)]),
        )
        .or_else(|| {
            ingredient_diagonal_fall_target(
                GridPos { x: 7, y: 4 },
                &occupied(&[(7, 4), (6, 5)]),
                &shadows(&[(7, 3)]),
            )
        });

        assert_eq!(next, None);
    }

    #[test]
    fn straight_falls_get_priority_over_diagonal_competitors() {
        let mut occupied = occupied(&[(3, 4), (2, 4), (2, 3), (1, 3)]);
        let vertical = straight_fall_target(GridPos { x: 3, y: 4 }, &occupied, &shadows(&[]));
        assert_eq!(vertical, Some(GridPos { x: 3, y: 3 }));
        occupied.remove(&(3, 4));
        occupied.insert((3, 3));

        let diagonal = diagonal_fall_target(GridPos { x: 2, y: 4 }, &occupied, &shadows(&[]));
        assert_eq!(diagonal, None);
    }

    #[test]
    fn piece_rechecks_vertical_after_lower_piece_moves() {
        let mut occupied = occupied(&[(3, 4), (3, 3)]);

        let lower = straight_fall_target(GridPos { x: 3, y: 3 }, &occupied, &shadows(&[]));
        assert_eq!(lower, Some(GridPos { x: 3, y: 2 }));
        occupied.remove(&(3, 3));
        occupied.insert((3, 2));

        let upper = straight_fall_target(GridPos { x: 3, y: 4 }, &occupied, &shadows(&[]));
        assert_eq!(upper, Some(GridPos { x: 3, y: 3 }));

        let upper_diagonal = diagonal_fall_target(GridPos { x: 3, y: 4 }, &occupied, &shadows(&[]));
        assert_eq!(upper_diagonal, None);
    }

    #[test]
    fn bottom_row_never_falls_out_of_bounds() {
        let pos = GridPos { x: 4, y: 0 };
        assert_eq!(
            straight_fall_target(pos, &occupied(&[(4, 0)]), &shadows(&[])),
            None
        );
        assert_eq!(
            diagonal_fall_target(pos, &occupied(&[(4, 0)]), &shadows(&[])),
            None
        );
        assert_eq!(
            choose_fall_target(pos, &occupied(&[(4, 0)]), &shadows(&[])),
            None
        );
    }
}

pub(crate) fn on_fall_complete(
    _: On<FallComplete>,
    mut commands: Commands,
    mut next_state: ResMut<NextState<GameState>>,
    mut collected: ResMut<SparksCollected>,
    mut score: ResMut<Score>,
    mut displayed_score: ResMut<DisplayedScore>,
    mut reserve: ResMut<CoreReserve>,
    run: Res<RunState>,
    mut settled: ResMut<GravitySettled>,
    mode: Res<GameMode>,
    lights: Query<(), With<Light>>,
    sparks: Query<(Entity, &GridPos), With<Spark>>,
) {
    // ConsumeAll win condition: check_board_consumed (OnEnter Falling) already wrote LevelComplete.
    // Don't overwrite it with Spawning — the board is supposed to stay empty.
    if *mode == GameMode::ConsumeAll && lights.is_empty() {
        return;
    }
    let mut any_collected = false;
    for (e, gp) in &sparks {
        if gp.y == 0 {
            commands.entity(e).try_despawn();
            collected.0 += 1;
            let bonus = run.spark_bonus();
            score.0 += bonus;
            displayed_score.0 += bonus;
            reserve.0 += bonus;
            any_collected = true;
        }
    }
    if any_collected {
        // Removing the spark leaves a hole; let apply_gravity run again next frame
        // (still in GameState::Falling) so the column above drops to fill it.
        settled.0 = false;
    } else {
        next_state.set(GameState::Spawning);
    }
}
