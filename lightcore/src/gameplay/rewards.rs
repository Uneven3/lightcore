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
    PowerActivation, PowerCreated, ChainPop, PowerActivationQueue, SuperComboPending,
};
use crate::core::components::{AdjacentMatchDamage, HardShadow, Light, PopAnim, Spark};
use crate::core::grid::{to_world, GridPos};
use crate::core::light::{LightColor, LightKind};
use crate::core::matching::{EntityInfo, Grid, MatchResult, fire_single_activation, resolve_wave};
use crate::core::run::RunState;
use crate::gameplay::popping::apply_pop_delay;
use crate::board::clear_shadow_at;
use crate::gameplay::vfx;
use crate::visuals::RaySettings;

/// Bundles the mutable references to the game's economy resources to prevent
/// parameter explosion in [`apply_removal_rewards`].
pub(crate) struct EconomyState<'a> {
    pub(crate) score: &'a mut Score,
    #[allow(dead_code)]
    pub(crate) displayed: &'a mut DisplayedScore,
    pub(crate) reserve: &'a mut CoreReserve,
    pub(crate) collected_cores: &'a mut CollectedCores,
    pub(crate) stats: &'a mut StatsBook,
    pub(crate) moves: &'a mut MovesLeft,
    pub(crate) run: &'a mut RunState,
}

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
pub(super) fn apply_removal_rewards(
    commands: &mut Commands,
    to_remove: &HashSet<Entity>,
    entity_info: &EntityInfo,
    cascade: u32,
    score_reset: bool,
    power_bonus_for_upgrades: u32,
    economy: &mut EconomyState,
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
            economy.collected_cores.0[color.index()] += cascade;
            if !score_reset {
                score_bonus += economy.run.score_bonus_for_color(*color, cascade);
                reserve_bonus += economy.run.reserve_bonus_for_color(*color, cascade);
            }
            if *color == LightColor::Blue {
                blue_count += cascade;
            }
            match color {
                LightColor::Red => economy.stats.reds += cascade,
                LightColor::Green => economy.stats.greens += cascade,
                LightColor::Blue => economy.stats.blues += cascade,
                LightColor::Yellow => economy.stats.yellows += cascade,
                LightColor::Purple => economy.stats.purples += cascade,
            }
            if kind.is_power() {
                economy.stats.lightkinds += cascade;
            }
        }
    }

    if score_reset {
        economy.score.0 = 0;
        // economy.displayed.0 is NOT set to 0 here; let visuals::score_light::tick_score_drain handle it!
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
        economy.score.0 += points + score_bonus;
        economy.reserve.0 += points + reserve_bonus;
    }

    economy.stats.max_cascade = economy.stats.max_cascade.max(cascade);
    if cascade >= 2 {
        economy.stats.total_chains += 1;
    }

    let move_bonus = economy.run.blue_move_bonus(blue_count);
    if move_bonus > 0 && economy.moves.0 != u32::MAX {
        // Sandbox/debug modes represent unlimited moves with `u32::MAX`. A power swap can
        // transiently decrement that sentinel before a Blue boon restores moves; saturating keeps
        // the sentinel intact instead of panicking at `MAX - 1 + bonus`.
        economy.moves.0 = economy.moves.0.saturating_add(move_bonus);
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

/// Shared match sequence resolution: upgrades matched lights, processes power combos,
/// triggers combo or wave VFX, cleans shadows, calculates score rewards, and spawns pop animations.
/// Factored out here to eliminate major duplication between `swap.rs` and `chain.rs`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_match_sequence(
    commands: &mut Commands,
    grid: &Grid,
    entity_info: &mut EntityInfo,
    cascade_depth: u32,
    result: MatchResult,
    partner_color: Option<LightColor>,
    ray_settings: &RaySettings,
    lights: &mut Query<
        (Entity, &mut GridPos, &LightColor, &mut LightKind),
        (With<Light>, Without<AdjacentMatchDamage>, Without<Spark>),
    >,
    shadow_q: &mut Query<
        (Entity, &GridPos, Option<&mut HardShadow>),
        With<AdjacentMatchDamage>,
    >,
    shadow_count: &mut u32,
    queue: &mut PowerActivationQueue,
    super_combo: &mut SuperComboPending,
    economy: &mut EconomyState,
) {
    let mut to_remove = result.to_remove;

    let upgrades: Vec<(Entity, LightKind)> = result
        .to_upgrade
        .into_iter()
        .filter(|(e, _)| !to_remove.contains(e))
        .collect();
    for (e, kind) in &upgrades {
        if let Ok((_, _, _, mut k)) = lights.get_mut(*e) {
            *k = *kind;
        }
        if let Some(entry) = entity_info.get_mut(e) {
            entry.2 = *kind;
        }
    }

    let mut pop_delays: HashMap<Entity, f32> = HashMap::new();
    for replaced in &result.replaced_powers {
        vfx::trigger_single_vfx(
            commands,
            replaced,
            grid,
            entity_info,
            &mut pop_delays,
            ray_settings,
        );
        let host = grid.get(&replaced.pos).map(|(e, _, _)| *e);
        for e in fire_single_activation(replaced, grid, entity_info) {
            if Some(e) != host {
                to_remove.insert(e);
            }
        }
    }

    let initial_powers: Vec<PowerActivation> = to_remove
        .iter()
        .filter_map(|e| entity_info.get(e))
        .filter(|(_, _, k)| k.is_power())
        .map(|(pos, _, kind)| PowerActivation {
            pos: *pos,
            kind: *kind,
            partner_color,
        })
        .collect();

    if initial_powers.len() >= 3 {
        super_combo.0 = initial_powers.iter().map(|a| a.kind).collect();
        vfx::trigger_super_combo_vfx(
            commands,
            &initial_powers,
            grid,
            entity_info,
            &mut pop_delays,
            ray_settings,
        );
        for &e in entity_info.keys() {
            to_remove.insert(e);
        }
    } else {
        let wave = resolve_wave(&initial_powers, grid, entity_info);
        vfx::trigger_wave_vfx(
            commands,
            &wave,
            grid,
            entity_info,
            &mut pop_delays,
            ray_settings,
        );
        let activator_positions: HashSet<GridPos> = initial_powers.iter().map(|a| a.pos).collect();
        for e in wave.to_remove {
            if to_remove.contains(&e) {
                continue;
            }
            if let Some((pos, _, kind)) = entity_info.get(&e) {
                if kind.is_power() && !activator_positions.contains(pos) {
                    queue.0.push_back(PowerActivation {
                        pos: *pos,
                        kind: *kind,
                        partner_color: None,
                    });
                }
            }
            to_remove.insert(e);
        }
    }

    vfx::stage_power_impact_jelly(
        commands,
        &initial_powers,
        grid,
        entity_info,
        &to_remove,
        ray_settings,
    );

    let removed_positions: HashSet<GridPos> = to_remove
        .iter()
        .filter_map(|e| entity_info.get(e).map(|(p, _, _)| *p))
        .collect();
    clear_shadow_at(
        &removed_positions,
        commands,
        shadow_q,
        shadow_count,
    );

    let power_bonus = economy.run.power_bonus(upgrades.len() as u32);
    let points = apply_removal_rewards(
        commands,
        &to_remove,
        entity_info,
        cascade_depth,
        result.score_reset,
        power_bonus,
        economy,
    );
    for _ in &upgrades {
        commands.trigger(PowerCreated);
    }

    let pops = spawn_pops(
        commands,
        &to_remove,
        entity_info,
        &pop_delays,
        ray_settings.pop_duration,
    );
    commands.trigger(ChainPop {
        removed: to_remove.len() as u32,
        points,
        hollow: result.score_reset,
        pops,
        supernova_origins: initial_powers
            .iter()
            .filter(|activation| activation.kind == LightKind::Supernova)
            .map(|activation| to_world(activation.pos).with_z(2.0))
            .collect(),
    });
}
