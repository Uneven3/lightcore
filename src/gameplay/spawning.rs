use bevy::prelude::*;
use rand::Rng;
use std::collections::{HashSet, VecDeque};

use super::{GameMode, SpawnComplete, SuperComboPending};
use crate::board::{HOLLOW_BASE_CHANCE, random_basic_kind, spawn_light};
use crate::core::prelude::*;
use crate::core::run::RunState;
use crate::state::MatchPhase;

/// Tiles above the grid's top edge that new lights enter from — shared by every light in
/// a column's refill, so they all come from near the visible top, never from deep inside
/// the grid (which would look like they popped into existence) and never from far off-screen.
const SPAWN_GAP: f32 = 2.0;

/// Each drop lands quickly; the stream feel comes from scheduling, not from slowing the board.
const SPAWN_FALL_DURATION: f32 = 0.22;
/// One light enters at a time. This is short enough to keep refill responsive while avoiding the
/// synchronized row/column "window blind" produced by spawning every missing light in one frame.
const DRIP_INTERVAL: f32 = 0.028;

#[derive(Clone, Copy)]
struct SpawnRequest {
    entry: GridPos,
    pos: GridPos,
    color: LightColor,
    kind: LightKind,
}

/// The pending refill stream. It is a resource so the cadence is independent from board layout:
/// any number of subgrids and spawn entries can contribute drops to the same visual flow.
#[derive(Resource)]
pub(crate) struct RefillQueue {
    pending: VecDeque<SpawnRequest>,
    cadence: Timer,
    emitted_this_frame: bool,
}

impl Default for RefillQueue {
    fn default() -> Self {
        Self {
            pending: VecDeque::new(),
            cadence: Timer::from_seconds(DRIP_INTERVAL, TimerMode::Repeating),
            emitted_this_frame: false,
        }
    }
}

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
    mut super_combo: ResMut<SuperComboPending>,
    mode: Res<GameMode>,
    run: Res<RunState>,
    lights: Query<&GridPos, With<Light>>,
    sparks: Query<&GridPos, With<Spark>>,
    gravity_blockers: Query<&GridPos, With<BlocksGravity>>,
    layout: Res<GridLayout>,
    mut refill: ResMut<RefillQueue>,
) {
    *refill = RefillQueue::default();
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
    let mut columns = Vec::new();
    for entry in layout.spawn_entries() {
        // Only refill the section that is connected to this column's top entry. A shadow splits
        // the column: cells beneath it have no path from the spawn point and must stay empty
        // until gravity can reach them from a side route/portal.
        let empty_positions = refill_positions(&layout, entry, &occupied, &blocked);
        let topmost_index = empty_positions.len().saturating_sub(1);
        let mut requests = Vec::new();
        for (index, pos) in empty_positions.into_iter().enumerate() {
            let color = LightColor::random_weighted(&mut rng, weights);
            // Place a power light at the topmost newly spawned slot for each column
            let kind = if index == topmost_index && power_placed < power_row.len() {
                let k = power_row[power_placed];
                power_placed += 1;
                k
            } else {
                random_basic_kind(&mut rng, hollow_chance)
            };
            requests.push(SpawnRequest {
                entry,
                pos,
                color,
                kind,
            });
        }
        columns.push(requests);
    }

    // Interleave bottom-to-top streams from shuffled columns. A new drop never waits for an
    // entire column, so the board reads as a rapid drip/rainfall rather than a sweeping curtain.
    let max_depth = columns.iter().map(Vec::len).max().unwrap_or(0);
    let mut order: Vec<usize> = (0..columns.len()).collect();
    for depth in 0..max_depth {
        for i in (1..order.len()).rev() {
            let j = rng.random_range(0..=i);
            order.swap(i, j);
        }
        for &column in &order {
            if let Some(request) = columns[column].get(depth) {
                refill.pending.push_back(*request);
            }
        }
    }
}

/// Emits one refill drop per cadence beat. Keeping the actual spawn separate from planning lets
/// `wait_for_spawn_settle` know whether more lights are still scheduled.
pub(crate) fn emit_refill_drop(
    mut commands: Commands,
    mut refill: ResMut<RefillQueue>,
    time: Res<Time>,
) {
    refill.emitted_this_frame = false;
    if refill.pending.is_empty() || !refill.cadence.tick(time.delta()).just_finished() {
        return;
    }
    let request = refill.pending.pop_front().expect("pending was checked");
    let above = to_world(request.entry) + Vec3::Y * TILE * SPAWN_GAP;
    let fall_speed = (above - to_world(request.pos)).length() / SPAWN_FALL_DURATION;
    let entity = spawn_light(
        &mut commands,
        request.pos,
        request.color,
        request.kind,
        above,
    );
    commands.entity(entity).insert(FallSpeed(fall_speed));
    refill.emitted_this_frame = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadow_cuts_refill_to_the_section_above_it() {
        let layout = GridLayout::default();
        let entry = GridPos {
            x: 3,
            y: GRID_H - 1,
        };
        let blocked = HashSet::from([GridPos { x: 3, y: 3 }]);

        let targets = refill_positions(&layout, entry, &HashSet::new(), &blocked);

        assert_eq!(
            targets,
            vec![
                GridPos { x: 3, y: 4 },
                GridPos { x: 3, y: 5 },
                GridPos { x: 3, y: 6 },
                GridPos { x: 3, y: 7 }
            ]
        );
    }
}

/// Holds `Spawning` until every light's `VisualPos` has caught up to its `GridPos` (same settle
/// criterion as `falling::apply_gravity`), then advances to `CheckingChain`. This is what keeps
/// power activations from firing while the refill is still visibly dropping in.
pub(crate) fn wait_for_spawn_settle(
    mut commands: Commands,
    lights: Query<(&GridPos, &VisualPos), With<Light>>,
    refill: Res<RefillQueue>,
) {
    // A just-issued command is not visible to this query until the next frame. Waiting one frame
    // prevents the last drop from being skipped when the queue becomes empty.
    if !refill.pending.is_empty() || refill.emitted_this_frame {
        return;
    }
    let all_settled = lights
        .iter()
        .all(|(gp, vp)| vp.0.distance(to_world(*gp)) < TILE * 0.02);
    if all_settled {
        commands.trigger(SpawnComplete);
    }
}

pub(crate) fn on_spawn_complete(
    _: On<SpawnComplete>,
    mut next_state: ResMut<NextState<MatchPhase>>,
) {
    next_state.set(MatchPhase::CheckingChain);
}
