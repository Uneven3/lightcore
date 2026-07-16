use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use super::popping::accumulate_pop_delays;
use super::{
    CascadeDepth, ChainPop, CollectedCores, CoreReserve, DisplayedScore, MovesLeft, PendingSwap,
    PowerActivationQueue, PowerComboParams, RevertingSwap, Score, ShadowCount, StatsBook, SwapFailed,
    SwapHappened,
};
use super::{rewards, vfx};
use crate::board::clear_shadow_at;
use crate::core::prelude::*;
use crate::visuals::RaySettings;
use crate::core::run::RunState;
use crate::state::GameState;

#[derive(SystemParam)]
pub(crate) struct SwapScoreParams<'w> {
    score: ResMut<'w, Score>,
    displayed: ResMut<'w, DisplayedScore>,
    reserve: ResMut<'w, CoreReserve>,
    collected_cores: ResMut<'w, CollectedCores>,
    stats: ResMut<'w, StatsBook>,
    run: ResMut<'w, RunState>,
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
        With<AdjacentMatchDamage>,
    >,
    mut lights: Query<
        (Entity, &mut GridPos, &LightColor, &mut LightKind),
        (With<Light>, Without<AdjacentMatchDamage>, Without<Spark>),
    >,
    mut sparks: Query<
        (Entity, &mut GridPos),
        (With<Spark>, Without<Light>, Without<AdjacentMatchDamage>),
    >,
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
    let entity_info: EntityInfo = lights
        .iter()
        .map(|(e, p, c, k)| (e, (*p, *c, *k)))
        .collect();

    let Some(swap) = pending.0.as_ref() else {
        return;
    };
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
    let free = swap.free;

    // Path A: direct power+power swap
    if !is_spark_swap {
        if let Some(b_ent) = b_ent {
            if handle_power_combo_swap(
                &mut commands,
                &mut pending,
                &mut score_res,
                &mut moves,
                &mut next_state,
                cascade.0,
                &mut shadow_count,
                &mut shadow_q,
                &grid,
                &entity_info,
                a_ent,
                a_pos,
                a_kind,
                b_ent,
                b_pos,
                b_kind,
                free,
                &mut power.queue,
                &ray_settings,
            ) {
                return;
            }
        }
    }

    // Path B: normal match
    handle_normal_match_swap(
        &mut commands,
        &mut pending,
        &mut score_res,
        &mut moves,
        &mut next_state,
        cascade.0,
        &mut shadow_count,
        &mut shadow_q,
        &mut lights,
        &mut sparks,
        &mut reverting,
        &mut power,
        &grid,
        entity_info,
        a_ent,
        free,
        &ray_settings,
    );
}

fn handle_power_combo_swap(
    commands: &mut Commands,
    pending: &mut PendingSwap,
    score_res: &mut SwapScoreParams,
    moves: &mut MovesLeft,
    next_state: &mut NextState<GameState>,
    cascade_depth: u32,
    shadow_count: &mut ShadowCount,
    shadow_q: &mut Query<
        (Entity, &GridPos, Option<&mut HardShadow>),
        With<AdjacentMatchDamage>,
    >,
    grid: &Grid,
    entity_info: &EntityInfo,
    a_ent: Entity,
    a_pos: GridPos,
    a_kind: LightKind,
    b_ent: Entity,
    b_pos: GridPos,
    b_kind: LightKind,
    free: bool,
    queue: &mut PowerActivationQueue,
    ray_settings: &RaySettings,
) -> bool {
    let Some(compound) = resolve_swap_activation(
        a_ent,
        a_pos,
        a_kind,
        b_ent,
        b_pos,
        b_kind,
        grid,
        entity_info,
    ) else {
        return false;
    };

    // Any OTHER power lights hit by the compound are queued for post-refill activation.
    for e in &compound {
        if *e == a_ent || *e == b_ent {
            continue;
        }
        if let Some((pos, _, kind)) = entity_info.get(e) {
            if kind.is_power() {
                queue.0.push_back(PowerActivation {
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
        commands,
        shadow_q,
        &mut shadow_count.0,
    );

    let mut pop_delays: HashMap<Entity, f32> = HashMap::new();
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
        partner_color: if a_kind == LightKind::Starburst { star_partner_color } else { None },
    };
    let b_activation = PowerActivation {
        pos: b_pos,
        kind: b_kind,
        partner_color: if b_kind == LightKind::Starburst { star_partner_color } else { None },
    };

    let combo = classify_combo(a_kind, b_kind);
    if let Some(combo) = combo {
        vfx::trigger_combo(
            commands,
            grid,
            entity_info,
            &a_activation,
            &b_activation,
            combo,
            ray_settings,
            &mut pop_delays,
        );
    }

    if !matches!(
        combo,
        Some(ComboKind::StarLine) | Some(ComboKind::StarSupernova)
    ) {
        accumulate_pop_delays(&mut pop_delays, &a_activation, grid, entity_info, ray_settings);
        accumulate_pop_delays(&mut pop_delays, &b_activation, grid, entity_info, ray_settings);
    }

    vfx::stage_power_impact_jelly(
        commands,
        &[a_activation, b_activation],
        grid,
        entity_info,
        &compound,
        ray_settings,
    );

    let points = rewards::apply_removal_rewards(
        commands,
        &compound,
        entity_info,
        cascade_depth,
        false,
        0,
        &mut rewards::EconomyState {
            score: &mut score_res.score,
            displayed: &mut score_res.displayed,
            reserve: &mut score_res.reserve,
            collected_cores: &mut score_res.collected_cores,
            stats: &mut score_res.stats,
            moves: moves,
            run: &mut score_res.run,
        },
    );

    let pops = rewards::spawn_pops(
        commands,
        &compound,
        entity_info,
        &pop_delays,
        ray_settings.pop_duration,
    );

    commands.trigger(ChainPop {
        removed: compound.len() as u32,
        points,
        hollow: false,
        pops,
        supernova_origins: [a_activation, b_activation]
            .into_iter()
            .filter(|activation| activation.kind == LightKind::Supernova)
            .map(|activation| to_world(activation.pos).with_z(2.0))
            .collect(),
    });

    next_state.set(GameState::Popping);
    true
}

fn handle_normal_match_swap(
    commands: &mut Commands,
    pending: &mut PendingSwap,
    score_res: &mut SwapScoreParams,
    moves: &mut MovesLeft,
    next_state: &mut NextState<GameState>,
    cascade_depth: u32,
    shadow_count: &mut ShadowCount,
    shadow_q: &mut Query<
        (Entity, &GridPos, Option<&mut HardShadow>),
        With<AdjacentMatchDamage>,
    >,
    lights: &mut Query<
        (Entity, &mut GridPos, &LightColor, &mut LightKind),
        (With<Light>, Without<AdjacentMatchDamage>, Without<Spark>),
    >,
    sparks: &mut Query<
        (Entity, &mut GridPos),
        (With<Spark>, Without<Light>, Without<AdjacentMatchDamage>),
    >,
    reverting: &mut RevertingSwap,
    power: &mut PowerComboParams,
    grid: &Grid,
    mut entity_info: EntityInfo,
    a_ent: Entity,
    free: bool,
    ray_settings: &RaySettings,
) {
    let result = scan_runs(grid, &entity_info, Some(a_ent));

    if result.to_remove.is_empty() && result.to_upgrade.is_empty() {
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

    rewards::resolve_match_sequence(
        commands,
        grid,
        &mut entity_info,
        cascade_depth,
        result,
        swap_partner_color,
        ray_settings,
        lights,
        shadow_q,
        &mut shadow_count.0,
        &mut power.queue,
        &mut power.super_combo,
        &mut rewards::EconomyState {
            score: &mut score_res.score,
            displayed: &mut score_res.displayed,
            reserve: &mut score_res.reserve,
            collected_cores: &mut score_res.collected_cores,
            stats: &mut score_res.stats,
            moves,
            run: &mut score_res.run,
        },
    );
    next_state.set(GameState::Popping);
}
