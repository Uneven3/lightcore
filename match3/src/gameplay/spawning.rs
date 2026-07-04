use bevy::prelude::*;
use std::collections::HashSet;

use super::{SpawnComplete, SuperComboPending};
use crate::board::spawn_light;
use crate::core::prelude::*;
use crate::state::GameState;
use crate::visuals::assets::VisualCache;

/// Tiles above the grid's top edge that new lights enter from — shared by every light in
/// a column's refill, so they all come from near the visible top, never from deep inside
/// the grid (which would look like they popped into existence) and never from far off-screen.
const SPAWN_GAP: f32 = 2.0;

/// Fixed entry duration for every freshly spawned light, regardless of how far it has to
/// fall — achieved via a per-light `FallSpeed`, not the shared gravity speed. Lights with
/// less ground to cover fall slower; deeper ones fall faster — same arrival window, visibly
/// different speeds (the "falling together but at slightly different speeds" feel).
const SPAWN_FALL_DURATION: f32 = 0.25;

pub(crate) fn spawn_new_lights(
    mut commands: Commands,
    cache: Res<VisualCache>,
    mut super_combo: ResMut<SuperComboPending>,
    lights: Query<&GridPos, With<Light>>,
    sparks: Query<&GridPos, With<Spark>>,
    blockers: Query<&GridPos, With<Blocker>>,
) {
    let mut occupied = HashSet::new();
    for p in &lights {
        occupied.insert(*p);
    }
    for p in &sparks {
        occupied.insert(*p);
    }
    let blocked: HashSet<GridPos> = blockers.iter().copied().collect();

    // Super combo: place power lights at the top of newly spawned columns (visible entering from above)
    let power_row = std::mem::take(&mut super_combo.0);
    let mut power_placed = 0usize;

    let mut rng = rand::rng();
    for x in 0..GRID_W {
        let empty_positions: Vec<GridPos> = (0..GRID_H)
            .map(|y| GridPos { x, y })
            .filter(|pos| !occupied.contains(pos) && !blocked.contains(pos))
            .collect();
        let topmost_index = empty_positions.len().saturating_sub(1);
        for (index, pos) in empty_positions.into_iter().enumerate() {
            let top_of_grid = to_world(GridPos { x, y: GRID_H - 1 });
            let above = top_of_grid + Vec3::Y * TILE * SPAWN_GAP;
            let fall_speed = (above - to_world(pos)).length() / SPAWN_FALL_DURATION;
            let color = LightColor::random(&mut rng);
            // Place a power light at the topmost newly spawned slot for each column
            let kind = if index == topmost_index && power_placed < power_row.len() {
                let k = power_row[power_placed];
                power_placed += 1;
                k
            } else {
                LightKind::Normal
            };
            let e = spawn_light(&mut commands, &cache, pos, color, kind, above);
            commands.entity(e).insert(FallSpeed(fall_speed));
            // The power's cores are built by `visuals::core_motion::rebuild_cores` (off the
            // `LightKind` `spawn_light` set) — no explicit indicator needed.
        }
    }
    // NOTE: do NOT signal completion here — the new lights are still falling in from above.
    // `wait_for_spawn_settle` fires `SpawnComplete` once they've visually arrived, so powers are
    // never consumed before the board looks full.
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
