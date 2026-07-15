use bevy::prelude::*;
use std::collections::HashSet;

use super::{GameMode, SpawnComplete, SuperComboPending};
use crate::board::{HOLLOW_BASE_CHANCE, random_basic_kind, spawn_light};
use crate::core::prelude::*;
use crate::core::run::RunState;
use crate::state::GameState;

/// Tiles above the grid's top edge that new lights enter from — shared by every light in
/// a column's refill, so they all come from near the visible top, never from deep inside
/// the grid (which would look like they popped into existence) and never from far off-screen.
const SPAWN_GAP: f32 = 2.0;

/// Fixed entry duration for every freshly spawned light, regardless of how far it has to
/// fall — achieved via a per-light `FallSpeed`, not the shared gravity speed. Lights with
/// less ground to cover fall slower; deeper ones fall faster — same arrival window, visibly
/// different speeds (the "falling together but at slightly different speeds" feel).
const SPAWN_FALL_DURATION: f32 = 0.25;

/// Empty cells that can actually be fed from a column's spawn entry. A blocker/shadow cuts the
/// vertical feed, so cells below it deliberately do not appear in this result.
fn refill_positions(
    layout: &GridLayout,
    entry: GridPos,
    occupied: &HashSet<GridPos>,
    blocked: &HashSet<GridPos>,
) -> Vec<GridPos> {
    let mut column: Vec<GridPos> = layout.cells_in_column(entry.x).collect();
    column.sort_by_key(|pos| std::cmp::Reverse(pos.y));
    let mut empty_positions = Vec::new();
    for pos in column {
        if blocked.contains(&pos) {
            break;
        }
        if !occupied.contains(&pos) {
            empty_positions.push(pos);
        }
    }
    empty_positions.sort_by_key(|pos| pos.y);
    empty_positions
}

pub(crate) fn spawn_new_lights(
    mut commands: Commands,
    mut super_combo: ResMut<SuperComboPending>,
    mode: Res<GameMode>,
    run: Res<RunState>,
    lights: Query<&GridPos, With<Light>>,
    sparks: Query<&GridPos, With<Spark>>,
    gravity_blockers: Query<&GridPos, With<BlocksGravity>>,
    layout: Res<GridLayout>,
) {
    let mut occupied = HashSet::new();
    for p in &lights {
        occupied.insert(*p);
    }
    for p in &sparks {
        occupied.insert(*p);
    }
    // Any gravity-blocking entity cuts the vertical feed. This includes cell obstacles and
    // gravity-locked lights, without coupling refill to a particular obstacle type.
    let blocked: HashSet<GridPos> = gravity_blockers.iter().copied().collect();

    // Super combo: place power lights at the top of newly spawned columns (visible entering from above)
    let power_row = std::mem::take(&mut super_combo.0);
    let mut power_placed = 0usize;

    let mut rng = rand::rng();
    let hollow_chance = if mode.is_run() {
        run.hollow_spawn_chance(HOLLOW_BASE_CHANCE)
    } else {
        HOLLOW_BASE_CHANCE
    };
    let weights = if mode.is_run() {
        run.color_weights()
    } else {
        [1.0; 5]
    };
    for entry in layout.spawn_entries() {
        // Only refill the section that is connected to this column's top entry. A shadow splits
        // the column: cells beneath it have no path from the spawn point and must stay empty
        // until gravity can reach them from a side route/portal.
        let empty_positions = refill_positions(&layout, entry, &occupied, &blocked);
        let topmost_index = empty_positions.len().saturating_sub(1);
        for (index, pos) in empty_positions.into_iter().enumerate() {
            let top_of_grid = to_world(entry);
            let above = top_of_grid + Vec3::Y * TILE * SPAWN_GAP;
            let fall_speed = (above - to_world(pos)).length() / SPAWN_FALL_DURATION;
            let color = LightColor::random_weighted(&mut rng, weights);
            // Place a power light at the topmost newly spawned slot for each column
            let kind = if index == topmost_index && power_placed < power_row.len() {
                let k = power_row[power_placed];
                power_placed += 1;
                k
            } else {
                random_basic_kind(&mut rng, hollow_chance)
            };
            let e = spawn_light(&mut commands, pos, color, kind, above);
            commands.entity(e).insert(FallSpeed(fall_speed));
            // The power's cores are built by `visuals::core_motion::rebuild_cores` (off the
            // `LightKind` `spawn_light` set) — no explicit indicator needed.
        }
    }
    // NOTE: do NOT signal completion here — the new lights are still falling in from above.
    // `wait_for_spawn_settle` fires `SpawnComplete` once they've visually arrived, so powers are
    // never consumed before the board looks full.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadow_cuts_refill_to_the_section_above_it() {
        let layout = GridLayout::default();
        let entry = GridPos { x: 3, y: GRID_H - 1 };
        let blocked = HashSet::from([GridPos { x: 3, y: 3 }]);

        let targets = refill_positions(&layout, entry, &HashSet::new(), &blocked);

        assert_eq!(targets, vec![GridPos { x: 3, y: 4 }, GridPos { x: 3, y: 5 }, GridPos { x: 3, y: 6 }, GridPos { x: 3, y: 7 }]);
    }
}

/// Holds `Spawning` until every light's `VisualPos` has caught up to its `GridPos` (same settle
/// criterion as `falling::apply_gravity`), then advances to `CheckingChain`. This is what keeps
/// power activations from firing while the refill is still visibly dropping in.
pub(crate) fn wait_for_spawn_settle(
    mut commands: Commands,
    lights: Query<(&GridPos, &VisualPos), With<Light>>,
) {
    let all_settled = lights
        .iter()
        .all(|(gp, vp)| vp.0.distance(to_world(*gp)) < TILE * 0.02);
    if all_settled {
        commands.trigger(SpawnComplete);
    }
}

pub(crate) fn on_spawn_complete(
    _: On<SpawnComplete>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    next_state.set(GameState::CheckingChain);
}
