use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use super::popping::{accumulate_pop_delays, apply_pop_delay};
use super::vfx;
use super::{
    CascadeDepth, ChainPop, CollectedCores, CoreReserve, DisplayedScore, MovesLeft, PendingSwap,
    PowerComboParams, PowerCreated, RevertingSwap, Score, ScoreDrained, ShadowCount, StatsBook,
    SwapFailed, SwapHappened,
};
use crate::board::clear_shadow_at;
use crate::core::grid::RaySettings;
use crate::core::prelude::*;
use crate::state::GameState;

#[derive(SystemParam)]
pub(crate) struct SwapScoreParams<'w> {
    score: ResMut<'w, Score>,
    displayed: ResMut<'w, DisplayedScore>,
    reserve: ResMut<'w, CoreReserve>,
    collected_cores: ResMut<'w, CollectedCores>,
    stats: ResMut<'w, StatsBook>,
}

#[allow(clippy::collapsible_if)]
pub(crate) fn on_swap_happened(
    _: On<SwapHappened>,
    mut commands: Commands,
    mut pending: ResMut<PendingSwap>,
    mut score_res: SwapScoreParams,
    mut moves: ResMut<MovesLeft>,
    mut next_state: ResMut<NextState<GameState>>,
    mut cascade: ResMut<CascadeDepth>,
    mut shadow_count: ResMut<ShadowCount>,
    mut shadow_q: Query<
        (Entity, &GridPos, Option<&mut HardShadow>),
        (
            With<Shadow>,
            Without<Blocker>,
            Without<Light>,
            Without<Spark>,
        ),
    >,
    mut lights: Query<
        (Entity, &mut GridPos, &LightColor, &mut LightKind),
        (With<Light>, Without<Shadow>),
    >,
    mut sparks: Query<(Entity, &mut GridPos), (With<Spark>, Without<Light>)>,
    mut reverting: ResMut<RevertingSwap>,
    mut power: PowerComboParams,
    ray_settings: Res<RaySettings>,
) {
    // New player swap: reset cascade and queue
    cascade.0 = 1;
    power.queue.0.clear();

    let grid: Grid = lights
        .iter()
        .map(|(e, p, c, k)| (*p, (e, *c, *k)))
        .collect();
    let mut entity_info: EntityInfo = lights
        .iter()
        .map(|(e, p, c, k)| (e, (*p, *c, *k)))
        .collect();

    let swap = pending.0.as_ref().unwrap();
    let is_spark_swap =
        !entity_info.contains_key(&swap.a) || swap.b.is_some_and(|b| !entity_info.contains_key(&b));
    let (a_pos, _, a_kind) = entity_info.get(&swap.a).copied().unwrap_or((
        swap.a_pos,
        LightColor::Red,
        LightKind::Normal,
    ));
    let (b_pos, _, b_kind) = swap
        .b
        .and_then(|b| entity_info.get(&b).copied())
        .unwrap_or((swap.b_pos, LightColor::Red, LightKind::Normal));
    let a_ent = swap.a;
    let b_ent = swap.b;
    // A shop "swap" booster: costs no move and never reverts on a non-match (see `SwapData::free`).
    let free = swap.free;

    // Path A: direct power+power swap — compound effect fires immediately;
    // any OTHER power lights hit by the compound are queued for post-refill activation.
    // Spark swaps and moves into an empty cell (no b entity) skip Path A entirely —
    // there's no second power light to combine with.
    if !is_spark_swap {
        if let Some(b_ent) = b_ent {
            if let Some(compound) = resolve_swap_activation(
                a_ent,
                a_pos,
                a_kind,
                b_ent,
                b_pos,
                b_kind,
                &grid,
                &entity_info,
            ) {
                for e in &compound {
                    if *e == a_ent || *e == b_ent {
                        continue;
                    }
                    if let Some((pos, _, kind)) = entity_info.get(e) {
                        if kind.is_power() {
                            power.queue.0.push_back(PowerActivation {
                                pos: *pos,
                                kind: *kind,
                                partner_color: None,
                            });
                        }
                    }
                }
                pending.0 = None;
                if !free {
                    moves.0 = moves.0.saturating_sub(1);
                }

                let removed_positions: HashSet<GridPos> = compound
                    .iter()
                    .filter_map(|e| entity_info.get(e).map(|(p, _, _)| *p))
                    .collect();
                clear_shadow_at(
                    &removed_positions,
                    &mut commands,
                    &mut shadow_q,
                    &mut shadow_count.0,
                );

                // The two swapped powers detonate as one interaction — fire a single unified `PowerCombo`
                // animation instead of two coincidental single-power effects. (`classify_combo` agrees with
                // `resolve_swap_activation`, which already returned `Some`, so this is always `Some`.) Pop
                // delays still ripple from each power's own blast (`Normal` accumulates nothing).
                let mut pop_delays: HashMap<Entity, f32> = HashMap::new();
                // Para combos de Starburst, partner_color determina qué lights se targetean.
                // Usar el color real del partner para que accumulate_pop_delays y los beams
                // del TravelingLight apunten exactamente al mismo set de lights.
                let star_partner_color = if a_kind == LightKind::Starburst {
                    grid.get(&b_pos).map(|(_, c, _)| *c)
                } else if b_kind == LightKind::Starburst {
                    grid.get(&a_pos).map(|(_, c, _)| *c)
                } else {
                    None
                };
                let a_activation = PowerActivation {
                    pos: a_pos,
                    kind: a_kind,
                    partner_color: if a_kind == LightKind::Starburst {
                        star_partner_color
                    } else {
                        None
                    },
                };
                let b_activation = PowerActivation {
                    pos: b_pos,
                    kind: b_kind,
                    partner_color: if b_kind == LightKind::Starburst {
                        star_partner_color
                    } else {
                        None
                    },
                };
                if let Some(combo) = classify_combo(a_kind, b_kind) {
                    vfx::trigger_combo(
                        &mut commands,
                        &grid,
                        &entity_info,
                        &a_activation,
                        &b_activation,
                        combo,
                        &ray_settings,
                    );
                }
                accumulate_pop_delays(
                    &mut pop_delays,
                    &a_activation,
                    &grid,
                    &entity_info,
                    &ray_settings,
                );
                accumulate_pop_delays(
                    &mut pop_delays,
                    &b_activation,
                    &grid,
                    &entity_info,
                    &ray_settings,
                );
                let points = compound
                    .iter()
                    .filter(|e| {
                        entity_info
                            .get(e)
                            .is_some_and(|(_, _, kind)| !kind.is_hollow())
                    })
                    .count() as u32
                    * cascade.0;

                score_res.score.0 += points;
                score_res.reserve.0 += points;
                for e in &compound {
                    if let Some((_, color, kind)) = entity_info.get(e) {
                        if kind.is_hollow() {
                            continue;
                        }
                        score_res.collected_cores.0[color.index()] += cascade.0;
                        let add = cascade.0;
                        match color {
                            LightColor::Red => score_res.stats.reds += add,
                            LightColor::Green => score_res.stats.greens += add,
                            LightColor::Blue => score_res.stats.blues += add,
                            LightColor::Yellow => score_res.stats.yellows += add,
                            LightColor::Purple => score_res.stats.purples += add,
                        }
                        if kind.is_power() {
                            score_res.stats.lightkinds += add;
                        }
                    }
                }
                score_res.stats.max_cascade = score_res.stats.max_cascade.max(cascade.0);
                if cascade.0 >= 2 {
                    score_res.stats.total_chains += 1;
                }
                let mut pops: Vec<(Vec3, LightColor, f32)> = Vec::new();
                for e in &compound {
                    commands.entity(*e).insert(PopAnim(Timer::from_seconds(
                        ray_settings.pop_duration,
                        TimerMode::Once,
                    )));
                    apply_pop_delay(&mut commands, *e, &pop_delays);
                    if let Some((pos, color, kind)) = entity_info.get(e)
                        && !kind.is_hollow()
                    {
                        let w = to_world(*pos);
                        let delay = pop_delays.get(e).copied().unwrap_or(0.0);
                        pops.push((w, *color, delay));
                    }
                }
                commands.trigger(ChainPop {
                    removed: compound.len() as u32,
                    points,
                    hollow: false,
                    pops,
                });
                next_state.set(GameState::Popping);
                return;
            }
        } // end if let Some(b_ent)
    } // end !is_spark_swap (Path A guard)

    // Path B: normal match — scan runs, queue any power light activations
    let result = scan_runs(&grid, &entity_info, Some(a_ent));

    if result.to_remove.is_empty() && result.to_upgrade.is_empty() {
        // A free (shop) swap keeps the new arrangement even with no match — the player paid to
        // break the rules; only a normal swap snaps the two pieces back.
        if !free {
            if let Some(swap) = pending.0.take() {
                reverting.0.clear();
                if let Ok((_, mut pos, _, _)) = lights.get_mut(swap.a) {
                    pos.set_if_neq(swap.a_pos);
                } else if let Ok((_, mut pos)) = sparks.get_mut(swap.a) {
                    pos.set_if_neq(swap.a_pos);
                }
                reverting.0.push(swap.a);
                if let Some(b) = swap.b {
                    if let Ok((_, mut pos, _, _)) = lights.get_mut(b) {
                        pos.set_if_neq(swap.b_pos);
                    } else if let Ok((_, mut pos)) = sparks.get_mut(b) {
                        pos.set_if_neq(swap.b_pos);
                    }
                    reverting.0.push(b);
                }
            }
            commands.trigger(SwapFailed);
        } else {
            pending.0 = None;
        }
        next_state.set(if reverting.0.is_empty() {
            GameState::Playing
        } else {
            GameState::SwapAnimating
        });
        return;
    }

    let swap_partner_color = pending
        .0
        .as_ref()
        .and_then(|s| s.b)
        .and_then(|b| entity_info.get(&b))
        .map(|(_, c, _)| *c);
    pending.0 = None;
    if !free {
        moves.0 = moves.0.saturating_sub(1);
    }

    let mut to_remove = result.to_remove;

    let upgrades: Vec<(Entity, LightKind)> = result
        .to_upgrade
        .into_iter()
        .filter(|(e, _)| !to_remove.contains(e))
        .collect();
    for (e, kind) in &upgrades {
        // `visuals::core_motion::rebuild_cores` reacts to the `LightKind` change and rebuilds
        // this light's cores into the power's signature cluster.
        if let Ok((_, _, _, mut k)) = lights.get_mut(*e) {
            *k = *kind;
        }
        if let Some(entry) = entity_info.get_mut(e) {
            entry.2 = *kind;
        }
    }

    // A power light that already occupied an upgrade-host cell still fires its own effect —
    // the host itself is excluded so it survives to receive the new kind. Anything else its
    // blast hits is merged into `to_remove` here and picked up by `initial_powers` below;
    // deliberately not re-triggered/queued here too, to avoid firing it twice.
    let mut pop_delays: HashMap<Entity, f32> = HashMap::new();
    for replaced in &result.replaced_powers {
        vfx::trigger_single_vfx(
            &mut commands,
            replaced,
            &grid,
            &entity_info,
            &mut pop_delays,
            &ray_settings,
        );
        let host = grid.get(&replaced.pos).map(|(e, _, _)| *e);
        for e in fire_single_activation(replaced, &grid, &entity_info) {
            if Some(e) != host {
                to_remove.insert(e);
            }
        }
    }

    // Expand power light effects immediately into the same pop wave.
    // Any power light HIT by these effects goes into the chain-reaction queue.
    let initial_powers: Vec<PowerActivation> = to_remove
        .iter()
        .filter_map(|e| entity_info.get(e))
        .filter(|(_, _, k)| k.is_power())
        .map(|(pos, _, kind)| PowerActivation {
            pos: *pos,
            kind: *kind,
            partner_color: swap_partner_color,
        })
        .collect();

    if initial_powers.len() >= 3 {
        // Super combo: one unified board-clearing animation, save the kinds, then clear the board.
        power.super_combo.0 = initial_powers.iter().map(|a| a.kind).collect();
        vfx::trigger_super_combo_vfx(
            &mut commands,
            &initial_powers,
            &grid,
            &entity_info,
            &mut pop_delays,
            &ray_settings,
        );
        for &e in entity_info.keys() {
            to_remove.insert(e);
        }
    } else {
        // Combine adjacent powers caught in this match — each pair plays one unified animation,
        // lone powers fire on their own. Any OTHER power light hit by the blast goes into the
        // chain-reaction queue (activators themselves excluded).
        let wave = resolve_wave(&initial_powers, &grid, &entity_info);
        vfx::trigger_wave_vfx(
            &mut commands,
            &wave,
            &grid,
            &entity_info,
            &mut pop_delays,
            &ray_settings,
        );
        let activator_positions: HashSet<GridPos> = initial_powers.iter().map(|a| a.pos).collect();
        for e in wave.to_remove {
            if to_remove.contains(&e) {
                continue;
            }
            if let Some((pos, _, kind)) = entity_info.get(&e) {
                if kind.is_power() && !activator_positions.contains(pos) {
                    power.queue.0.push_back(PowerActivation {
                        pos: *pos,
                        kind: *kind,
                        partner_color: None,
                    });
                }
            }
            to_remove.insert(e);
        }
    }

    let removed_positions: HashSet<GridPos> = to_remove
        .iter()
        .filter_map(|e| entity_info.get(e).map(|(p, _, _)| *p))
        .collect();
    clear_shadow_at(
        &removed_positions,
        &mut commands,
        &mut shadow_q,
        &mut shadow_count.0,
    );

    let points = if result.score_reset {
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
            * cascade.0
    };

    if result.score_reset {
        score_res.score.0 = 0;
        score_res.displayed.0 = 0;
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
        score_res.score.0 += points;
        score_res.reserve.0 += points;
    }
    for e in &to_remove {
        if let Some((_, color, kind)) = entity_info.get(e) {
            if kind.is_hollow() {
                continue;
            }
            score_res.collected_cores.0[color.index()] += cascade.0;
            let add = cascade.0;
            match color {
                LightColor::Red => score_res.stats.reds += add,
                LightColor::Green => score_res.stats.greens += add,
                LightColor::Blue => score_res.stats.blues += add,
                LightColor::Yellow => score_res.stats.yellows += add,
                LightColor::Purple => score_res.stats.purples += add,
            }
            if kind.is_power() {
                score_res.stats.lightkinds += add;
            }
        }
    }
    score_res.stats.max_cascade = score_res.stats.max_cascade.max(cascade.0);
    if cascade.0 >= 2 {
        score_res.stats.total_chains += 1;
    }
    for _ in &upgrades {
        commands.trigger(PowerCreated);
    }

    let mut pops: Vec<(Vec3, LightColor, f32)> = Vec::new();
    for e in &to_remove {
        commands.entity(*e).insert(PopAnim(Timer::from_seconds(
            ray_settings.pop_duration,
            TimerMode::Once,
        )));
        apply_pop_delay(&mut commands, *e, &pop_delays);
        if let Some((pos, color, kind)) = entity_info.get(e)
            && !kind.is_hollow()
        {
            let w = to_world(*pos);
            let delay = pop_delays.get(e).copied().unwrap_or(0.0);
            pops.push((w, *color, delay));
        }
    }
    commands.trigger(ChainPop {
        removed: to_remove.len() as u32,
        points: if result.score_reset { 0 } else { points },
        hollow: result.score_reset,
        pops,
    });
    next_state.set(GameState::Popping);
}
