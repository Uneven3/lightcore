//! Shared "what happens when a removal wave lands" logic — economy (score/reserve/collected
//! cores/stats/Run bonuses) and pop-animation spawning. Both `chain.rs` (cascade matches and
//! queued power activations) and `swap.rs` (the player's own swap, direct power+power combos)
//! remove a `HashSet<Entity>` and need the exact same consequences applied; before this module
//! existed, each of the 4 call sites reimplemented it by hand and `swap.rs` had silently drifted
//! out of sync with `chain.rs` (missing every `RunState` bonus — score/reserve/blue-move/power).

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use super::{
    CollectedCores, CoreReserve, DisplayedScore, MovesLeft, Score, ScoreDrained, StatsBook,
};
use crate::core::components::PopAnim;
use crate::core::grid::to_world;
use crate::core::light::LightColor;
use crate::core::matching::EntityInfo;
use crate::core::run::RunState;
use crate::gameplay::popping::apply_pop_delay;

/// Applies the full economy of a removal wave: base points, per-color `RunState` bonuses, the
/// booster reserve, collected-core stats, and — on a hollow-triggered `score_reset` — draining the
/// score to 0 and firing `ScoreDrained` instead of adding to it. Also converts any collected Blue
/// lights into bonus moves via `RunState::blue_move_bonus`.
///
/// `power_bonus_for_upgrades` should be 0 for paths that can't create upgrades this wave (a direct
/// power+power swap, or a queued activation wave) — pass `run.power_bonus(upgrades.len() as u32)`
/// only where `upgrades` is actually a fresh set of forged powers.
///
/// Returns the point total to report on the `ChainPop` event (0 when `score_reset`).
#[allow(clippy::too_many_arguments)]
pub(super) fn apply_removal_rewards(
    commands: &mut Commands,
    to_remove: &HashSet<Entity>,
    entity_info: &EntityInfo,
    cascade: u32,
    score_reset: bool,
    power_bonus_for_upgrades: u32,
    score: &mut Score,
    displayed: &mut DisplayedScore,
    reserve: &mut CoreReserve,
    collected_cores: &mut CollectedCores,
    stats: &mut StatsBook,
    moves: &mut MovesLeft,
    run: &mut RunState,
) -> u32 {
    let points = if score_reset {
        0
    } else {
        to_remove
            .iter()
            .filter(|e| {
                entity_info
                    .get(e)
                    .is_some_and(|(_, _, kind)| !kind.is_hollow())
            })
            .count() as u32
            * cascade
    };

    let mut score_bonus = if score_reset {
        0
    } else {
        power_bonus_for_upgrades
    };
    let mut reserve_bonus = 0;
    let mut blue_count = 0;
    for e in to_remove {
        if let Some((_, color, kind)) = entity_info.get(e) {
            if kind.is_hollow() {
                continue;
            }
            collected_cores.0[color.index()] += cascade;
            if !score_reset {
                score_bonus += run.score_bonus_for_color(*color, cascade);
                reserve_bonus += run.reserve_bonus_for_color(*color, cascade);
            }
            if *color == LightColor::Blue {
                blue_count += cascade;
            }
            match color {
                LightColor::Red => stats.reds += cascade,
                LightColor::Green => stats.greens += cascade,
                LightColor::Blue => stats.blues += cascade,
                LightColor::Yellow => stats.yellows += cascade,
                LightColor::Purple => stats.purples += cascade,
            }
            if kind.is_power() {
                stats.lightkinds += cascade;
            }
        }
    }

    if score_reset {
        score.0 = 0;
        displayed.0 = 0;
        commands.trigger(ScoreDrained {
            origins: to_remove
                .iter()
                .filter_map(|e| {
                    entity_info.get(e).and_then(|(pos, _, kind)| {
                        kind.is_hollow().then(|| to_world(*pos).with_z(6.0))
                    })
                })
                .collect(),
        });
    } else {
        score.0 += points + score_bonus;
        reserve.0 += points + reserve_bonus;
    }

    stats.max_cascade = stats.max_cascade.max(cascade);
    if cascade >= 2 {
        stats.total_chains += 1;
    }

    let move_bonus = run.blue_move_bonus(blue_count);
    if move_bonus > 0 && moves.0 != u32::MAX {
        moves.0 += move_bonus;
    }

    if score_reset { 0 } else { points + score_bonus }
}

/// Inserts `PopAnim` (+ any accumulated blast delay) on every removed entity and collects the
/// `(world_pos, color, delay)` triples `ChainPop` needs to spawn score shards — shared by every
/// removal path so the pop-spawn plumbing can't drift the way the economy math had.
pub(super) fn spawn_pops(
    commands: &mut Commands,
    to_remove: &HashSet<Entity>,
    entity_info: &EntityInfo,
    pop_delays: &HashMap<Entity, f32>,
    pop_duration: f32,
) -> Vec<(Vec3, LightColor, f32)> {
    let mut pops = Vec::new();
    for e in to_remove {
        commands
            .entity(*e)
            .insert(PopAnim(Timer::from_seconds(pop_duration, TimerMode::Once)));
        apply_pop_delay(commands, *e, pop_delays);
        if let Some((pos, color, kind)) = entity_info.get(e)
            && !kind.is_hollow()
        {
            let w = to_world(*pos);
            let delay = pop_delays.get(e).copied().unwrap_or(0.0);
            pops.push((w, *color, delay));
        }
    }
    pops
}
