use bevy::prelude::*;
use std::collections::HashSet;

use super::{
    CoreReserve, DisplayedScore, FallComplete, GameMode, GravitySettled, Score, SparksCollected,
};
use crate::core::components::PopAnim;
use crate::core::grid::GravityBlockSet;
use crate::core::prelude::*;
use crate::core::run::RunState;
use crate::state::GameState;

/// Marks an entity actively dropping under gravity — stays inserted through the visual
/// settle period between rows, not just the instant a row-step happens. Read by
/// `visuals::motion::lerp_visual_pos` to accelerate falls the longer they continue.
#[derive(Component)]
pub(crate) struct Dropping;

// Legacy ingredient-only helper below still uses the classic rectangle. Active gravity uses
// `GridLayout` instead, so flexible boards are not constrained by this compatibility function.
fn in_bounds(x: i32, y: i32) -> bool {
    (0..GRID_W).contains(&x) && (0..GRID_H).contains(&y)
}

pub(crate) fn reset_gravity(mut settled: ResMut<GravitySettled>) {
    settled.0 = false;
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
    straight_fall_target_in(pos, occupied, shadow_set, &GridLayout::default())
}

fn straight_fall_target_in(
    pos: GridPos,
    occupied: &HashSet<(i32, i32)>,
    shadow_set: &HashSet<(i32, i32)>,
    layout: &GridLayout,
) -> Option<GridPos> {
    if shadow_set.contains(&(pos.x, pos.y)) {
        return None;
    }

    let below = (pos.x, pos.y - 1);
    if layout.contains(GridPos { x: below.0, y: below.1 })
        && !shadow_set.contains(&below)
        && !occupied.contains(&below)
    {
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
    diagonal_fall_target_in(pos, occupied, shadow_set, &GridLayout::default())
}

fn diagonal_fall_target_in(
    pos: GridPos,
    occupied: &HashSet<(i32, i32)>,
    shadow_set: &HashSet<(i32, i32)>,
    layout: &GridLayout,
) -> Option<GridPos> {
    if shadow_set.contains(&(pos.x, pos.y)) {
        return None;
    }

    let below = (pos.x, pos.y - 1);
    if !shadow_set.contains(&below) && !occupied.contains(&below) {
        return None;
    }

    for dx in [-1, 1] {
        let diag = (pos.x + dx, pos.y - 1);
        if !layout.contains(GridPos { x: diag.0, y: diag.1 }) {
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

pub(crate) fn update_gravity_block_set(
    gravity_blockers: Query<&GridPos, With<BlocksGravity>>,
    added_gravity_blockers: Query<(), Added<BlocksGravity>>,
    mut removed_gravity_blockers: RemovedComponents<BlocksGravity>,
    mut gravity_blocks: ResMut<GravityBlockSet>,
) {
    // Every entity that blocks gravity participates, regardless of its gameplay identity.
    // In particular, a Stasis light cuts vertical feed just like the old cyan shadow did,
    // so diagonals cannot flow through the column above/below it.
    if !added_gravity_blockers.is_empty() || removed_gravity_blockers.read().next().is_some() {
        gravity_blocks.0 = gravity_blockers.iter().map(|p| (p.x, p.y)).collect();
    }
}

pub(crate) fn apply_gravity(
    mut commands: Commands,
    mut settled: ResMut<GravitySettled>,
    mut entities: Query<
        (Entity, &mut GridPos, &mut VisualPos, &mut Transform, Has<Spark>),
        (
            With<FallPhysics>,
            Or<(With<Movable>, With<Spark>)>,
            Without<PopAnim>,
        ),
    >,
    gravity_blocks: Res<GravityBlockSet>,
    layout: Res<GridLayout>,
    locked_lights: Query<
        &GridPos,
        (With<Light>, With<BlocksGravity>, Without<Movable>, Without<Spark>),
    >,
) {
    let shadow_set = &gravity_blocks.0;

    let mut sorted: Vec<(Entity, GridPos, Vec3, bool)> = entities
        .iter()
        .map(|(e, p, v, _, is_spark)| (e, *p, v.0, is_spark))
        .collect();
    sorted.sort_by_key(|(_, p, _, _)| p.y);
    let mut occupied: HashSet<(i32, i32)> = sorted.iter().map(|(_, p, _, _)| (p.x, p.y)).collect();
    // A gravity-locked light does not move but remains solid, so falling lights cannot overlap it.
    occupied.extend(locked_lights.iter().map(|p| (p.x, p.y)));
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

        let portal_target = layout
            .fall_target(*pos)
            .filter(|exit| !occupied.contains(&(exit.x, exit.y)));
        if let Some(target) = portal_target.or_else(|| straight_fall_target_in(*pos, &occupied, shadow_set, &layout))
        {
            occupied.remove(&(pos.x, pos.y));
            occupied.insert((target.x, target.y));
            if let Ok((_, mut p, mut visual, mut transform, _)) = entities.get_mut(*e) {
                p.set_if_neq(target);
                if portal_target.is_some() {
                    // Never interpolate across the gap: the light exits below the left board,
                    // is repositioned above the right board off the visible path, then falls in
                    // vertically like a normal spawn.
                    let destination = to_world(target);
                    let portal_entry = destination + Vec3::Y * TILE * 1.35;
                    visual.0 = portal_entry;
                    transform.translation = portal_entry;
                    commands.entity(*e).insert(FallSpeed(
                        (portal_entry - destination).length() / 0.18,
                    ));
                }
            }
            commands.entity(*e).insert(Dropping);
            any_moved = true;
        } else {
            blocked_for_diagonal.push((*e, *pos, *is_spark));
        }
    }

    for (e, pos, is_spark) in blocked_for_diagonal {
        let portal_target = layout
            .fall_target(pos)
            .filter(|exit| !occupied.contains(&(exit.x, exit.y)));
        if let Some(target) = portal_target.or_else(|| straight_fall_target_in(pos, &occupied, shadow_set, &layout))
        {
            occupied.remove(&(pos.x, pos.y));
            occupied.insert((target.x, target.y));
            if let Ok((_, mut p, mut visual, mut transform, _)) = entities.get_mut(e) {
                p.set_if_neq(target);
                if portal_target.is_some() {
                    let destination = to_world(target);
                    let portal_entry = destination + Vec3::Y * TILE * 1.35;
                    visual.0 = portal_entry;
                    transform.translation = portal_entry;
                    commands.entity(e).insert(FallSpeed(
                        (portal_entry - destination).length() / 0.18,
                    ));
                }
            }
            commands.entity(e).insert(Dropping);
            any_moved = true;
        } else if let Some(target) = if is_spark {
            None
        } else {
            diagonal_fall_target_in(pos, &occupied, shadow_set, &layout)
        } {
            occupied.remove(&(pos.x, pos.y));
            occupied.insert((target.x, target.y));
            if let Ok((_, mut p, _, _, _)) = entities.get_mut(e) {
                p.set_if_neq(target);
            }
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

    fn validate_match_query_disjointness(
        mut lights: Query<
            &mut GridPos,
            (With<Light>, Without<AdjacentMatchDamage>, Without<Spark>),
        >,
        mut sparks: Query<
            &mut GridPos,
            (With<Spark>, Without<Light>, Without<AdjacentMatchDamage>),
        >,
        obstacles: Query<&GridPos, With<AdjacentMatchDamage>>,
    ) {
        // Iteration makes this the same mutable/shared GridPos access pattern used by the
        // swap and chain systems. Bevy validates the filters when the schedule is initialized.
        for _ in &mut lights {}
        for _ in &mut sparks {}
        for _ in &obstacles {}
    }

    #[test]
    fn match_lights_and_adjacent_obstacles_are_ecs_disjoint() {
        let mut app = App::new();
        app.add_systems(Update, validate_match_query_disjointness);
        app.update();
    }

    fn validate_input_query_disjointness(
        mut lights: Query<
            &mut GridPos,
            (With<Light>, Without<Spark>, Without<BlocksInteraction>),
        >,
        mut sparks: Query<
            &mut GridPos,
            (With<Spark>, Without<Light>, Without<BlocksInteraction>),
        >,
        blockers: Query<&GridPos, With<BlocksInteraction>>,
    ) {
        for _ in &mut lights {}
        for _ in &mut sparks {}
        for _ in &blockers {}
    }

    #[test]
    fn movable_input_pieces_and_interaction_blockers_are_ecs_disjoint() {
        let mut app = App::new();
        app.add_systems(Update, validate_input_query_disjointness);
        app.update();
    }

    #[derive(Resource, Default)]
    struct FallingPieceCount(usize);

    fn count_falling_pieces(
        pieces: Query<
            (),
            (
                With<FallPhysics>,
                Or<(With<Movable>, With<Spark>)>,
                Without<PopAnim>,
            ),
        >,
        locked_lights: Query<
            &GridPos,
            (With<Light>, With<BlocksGravity>, Without<Movable>, Without<Spark>),
        >,
        mut count: ResMut<FallingPieceCount>,
    ) {
        count.0 = pieces.iter().count();
        let _ = locked_lights.iter().count();
    }

    #[test]
    fn gravity_accepts_sparks_but_excludes_locked_lights() {
        let mut app = App::new();
        app.init_resource::<FallingPieceCount>()
            .add_systems(Update, count_falling_pieces);
        app.world_mut().spawn((Spark, FallPhysics, GridPos { x: 1, y: 4 }));
        app.world_mut().spawn((
            Light,
            Stasis,
            BlocksGravity,
            GridPos { x: 1, y: 3 },
        ));

        app.update();

        assert_eq!(app.world().resource::<FallingPieceCount>().0, 1);
    }

    #[test]
    fn stasis_enables_the_same_diagonal_cascade_as_a_gravity_blocker() {
        let mut app = App::new();
        app.init_resource::<GravityBlockSet>()
            .add_systems(Update, update_gravity_block_set);
        app.world_mut().spawn((
            // This is above the diagonal destination (2, 3). Its occupied light alone would
            // normally make that column look vertically fed, suppressing the diagonal.
            GridPos { x: 2, y: 4 },
            Stasis,
            BlocksGravity,
        ));

        app.update();

        let gravity_blocks = app.world().resource::<GravityBlockSet>().0.clone();
        let occupied = occupied(&[(3, 4), (3, 3), (2, 4)]);
        let next = choose_fall_target(GridPos { x: 3, y: 4 }, &occupied, &gravity_blocks);

        assert_eq!(next, Some(GridPos { x: 2, y: 3 }));
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
    layout: Res<GridLayout>,
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
        if layout.is_spark_exit(*gp) {
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
