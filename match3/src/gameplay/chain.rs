use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use super::popping::apply_pop_delay;
use super::vfx;
use super::{
    CascadeDepth, ChainPop, CollectedCores, CoreReserve, DisplayedScore, GameMode, MovesLeft,
    PowerActivationQueue, PowerCreated, Score, ScoreDrained, ShadowCount, SparksCollected,
    StatsBook, SuperComboPending,
};
use crate::board::{HOLLOW_BASE_CHANCE, clear_shadow_at, shuffle_board};
use crate::core::grid::RaySettings;
use crate::core::prelude::*;
use crate::core::run::RunState;
use crate::state::GameState;
use crate::visuals::assets::VisualCache;

#[derive(SystemParam)]
pub(crate) struct ChainParams<'w> {
    pub(crate) score: ResMut<'w, Score>,
    pub(crate) displayed: ResMut<'w, DisplayedScore>,
    pub(crate) reserve: ResMut<'w, CoreReserve>,
    pub(crate) moves: ResMut<'w, MovesLeft>,
    pub(crate) cascade: ResMut<'w, CascadeDepth>,
    pub(crate) level: Res<'w, LevelConfig>,
    pub(crate) collected: Res<'w, SparksCollected>,
    pub(crate) shadow_count: ResMut<'w, ShadowCount>,
    pub(crate) queue: ResMut<'w, PowerActivationQueue>,
    pub(crate) super_combo: ResMut<'w, SuperComboPending>,
    pub(crate) collected_cores: ResMut<'w, CollectedCores>,
    pub(crate) stats: ResMut<'w, StatsBook>,
    pub(crate) run: ResMut<'w, RunState>,
    pub(crate) mode: Res<'w, GameMode>,
}

pub(crate) fn check_chain_matches(
    mut commands: Commands,
    mut res: ChainParams,
    mut next_state: ResMut<NextState<GameState>>,
    cache: Res<VisualCache>,
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
        (Entity, &GridPos, &LightColor, &mut LightKind),
        (With<Light>, Without<Shadow>),
    >,
    ray_settings: Res<RaySettings>,
) {
    res.cascade.0 += 1;
    let grid: Grid = lights
        .iter()
        .map(|(e, p, c, k)| (*p, (e, *c, *k)))
        .collect();
    let mut entity_info: EntityInfo = lights
        .iter()
        .map(|(e, p, c, k)| (e, (*p, *c, *k)))
        .collect();

    // FASE 1: Drenar TODA la queue de una vez contra el mismo snapshot — evita pagar
    // un ciclo completo Popping→Falling→Spawning→CheckingChain por cada activación
    // encolada, y deja que FASE 2 (abajo) se alcance mucho antes en cascadas largas.
    if !res.queue.0.is_empty() {
        let activations: Vec<PowerActivation> = res.queue.0.drain(..).collect();
        // Each activation's own cell ends up in its own blast result (e.g. a RayH's row
        // scan includes its own column) — track activators so they aren't mistaken for newly
        // discovered power lights and re-queued against themselves.
        let mut activating_entities: HashSet<Entity> = HashSet::new();
        for activation in &activations {
            if let Some(&(e, _, _)) = grid.get(&activation.pos) {
                activating_entities.insert(e);
            }
        }
        let mut pop_delays: HashMap<Entity, f32> = HashMap::new();

        // Adjacent powers in the same wave combine (Star+Star clears all, Supernova+Supernova
        // 5x5, etc.); the rest fire individually. The VFX layer plays one unified animation per
        // combined pair and the standard flash+beam per lone power.
        let wave = resolve_wave(&activations, &grid, &entity_info);
        vfx::trigger_wave_vfx(
            &mut commands,
            &wave,
            &grid,
            &entity_info,
            &mut pop_delays,
            &ray_settings,
        );
        let to_remove: HashSet<Entity> = wave.to_remove;

        let wave_powers: Vec<PowerActivation> = to_remove
            .iter()
            .filter(|e| !activating_entities.contains(e))
            .filter_map(|e| entity_info.get(e))
            .filter(|(_, _, k)| k.is_power())
            .map(|(pos, _, kind)| PowerActivation {
                pos: *pos,
                kind: *kind,
                partner_color: None,
            })
            .collect();

        let final_remove = if wave_powers.len() >= 3 {
            res.super_combo.0 = wave_powers.iter().map(|a| a.kind).collect();
            let mut all = to_remove;
            for &e in entity_info.keys() {
                all.insert(e);
            }
            all
        } else {
            res.queue.0.extend(wave_powers.iter().copied());
            to_remove
        };

        let removed_positions: HashSet<GridPos> = final_remove
            .iter()
            .filter_map(|e| entity_info.get(e).map(|(p, _, _)| *p))
            .collect();
        clear_shadow_at(
            &removed_positions,
            &mut commands,
            &mut shadow_q,
            &mut res.shadow_count.0,
        );

        let points = final_remove
            .iter()
            .filter(|e| {
                entity_info
                    .get(e)
                    .is_some_and(|(_, _, kind)| !kind.is_hollow())
            })
            .count() as u32
            * res.cascade.0;

        let mut score_bonus = 0;
        let mut reserve_bonus = 0;
        let mut blue_count = 0;
        for e in &final_remove {
            if let Some((_, color, kind)) = entity_info.get(e) {
                if kind.is_hollow() {
                    continue;
                }
                res.collected_cores.0[color.index()] += res.cascade.0;
                let add = res.cascade.0;
                score_bonus += res.run.score_bonus_for_color(*color, add);
                reserve_bonus += res.run.reserve_bonus_for_color(*color, add);
                if *color == LightColor::Blue {
                    blue_count += add;
                }
                match color {
                    LightColor::Red => res.stats.reds += add,
                    LightColor::Green => res.stats.greens += add,
                    LightColor::Blue => res.stats.blues += add,
                    LightColor::Yellow => res.stats.yellows += add,
                    LightColor::Purple => res.stats.purples += add,
                }
                if kind.is_power() {
                    res.stats.lightkinds += add;
                }
            }
        }
        res.score.0 += points + score_bonus;
        res.reserve.0 += points + reserve_bonus;
        let move_bonus = res.run.blue_move_bonus(blue_count);
        if move_bonus > 0 && res.moves.0 != u32::MAX {
            res.moves.0 += move_bonus;
        }
        res.stats.max_cascade = res.stats.max_cascade.max(res.cascade.0);
        if res.cascade.0 >= 2 {
            res.stats.total_chains += 1;
        }
        let mut pops: Vec<(Vec3, LightColor, f32)> = Vec::new();
        for e in &final_remove {
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
            removed: final_remove.len() as u32,
            points: points + score_bonus,
            pops,
        });
        next_state.set(GameState::Popping);
        return;
    }

    // FASE 2: Queue vacía — buscar matches en cascada
    let result = scan_runs(&grid, &entity_info, None);

    if result.to_remove.is_empty() && result.to_upgrade.is_empty() {
        // FASE 3: No hay más matches — revisar condición de nivel
        let level_complete = match &res.level.goal {
            LevelGoal::Score(target) => res.score.0 >= *target,
            LevelGoal::Sparks => res.collected.0 >= res.level.sparks_total,
            LevelGoal::ClearShadow => res.shadow_count.0 == 0,
            LevelGoal::CollectColor { color, target } => {
                res.collected_cores.0[color.index()] >= *target
            }
            LevelGoal::TimedScore { target, .. } => res.score.0 >= *target,
            LevelGoal::TimedCollectColor { color, target, .. } => {
                res.collected_cores.0[color.index()] >= *target
            }
        };
        if level_complete {
            next_state.set(GameState::LevelComplete);
            return;
        }
        if res.moves.0 == 0 {
            next_state.set(GameState::GameOver);
            return;
        }
        let shadow_set: HashSet<GridPos> = shadow_q.iter().map(|(_, p, _)| *p).collect();
        if find_valid_swap(&grid, &shadow_set).is_none() {
            let light_pairs: Vec<(Entity, GridPos)> =
                lights.iter().map(|(e, p, _, _)| (e, *p)).collect();
            let hollow_chance = if res.mode.is_run() {
                res.run.hollow_spawn_chance(HOLLOW_BASE_CHANCE)
            } else {
                HOLLOW_BASE_CHANCE
            };
            shuffle_board(&mut commands, &cache, &light_pairs, hollow_chance);
        }
        next_state.set(GameState::Playing);
        return;
    }

    let mut to_remove = result.to_remove;

    let upgrades: Vec<(Entity, LightKind)> = result
        .to_upgrade
        .into_iter()
        .filter(|(e, _)| !to_remove.contains(e))
        .collect();
    for (e, kind) in &upgrades {
        // Setting `LightKind` is enough: `visuals::core_motion::rebuild_cores` reacts to the
        // change and rebuilds this light's cores into the power's signature cluster.
        if let Ok((_, _, _, mut k)) = lights.get_mut(*e) {
            *k = *kind;
        }
        if let Some(entry) = entity_info.get_mut(e) {
            entry.2 = *kind;
        }
    }

    // A power light that already occupied an upgrade-host cell still fires its own effect —
    // the host itself is excluded so it survives to receive the new kind. Anything else its
    // blast hits is merged into `to_remove` here and picked up by `cascade_powers` below;
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

    // Expand cascade-matched power light effects immediately.
    // Any power light HIT by these effects goes into the chain-reaction queue.
    let cascade_powers: Vec<PowerActivation> = to_remove
        .iter()
        .filter_map(|e| entity_info.get(e))
        .filter(|(_, _, k)| k.is_power())
        .map(|(pos, _, kind)| PowerActivation {
            pos: *pos,
            kind: *kind,
            partner_color: None,
        })
        .collect();

    if cascade_powers.len() >= 3 {
        res.super_combo.0 = cascade_powers.iter().map(|a| a.kind).collect();
        vfx::trigger_super_combo_vfx(
            &mut commands,
            &cascade_powers,
            &grid,
            &entity_info,
            &mut pop_delays,
            &ray_settings,
        );
        for &e in entity_info.keys() {
            to_remove.insert(e);
        }
    } else {
        // Combine adjacent cascade powers — each pair plays one unified animation, lone powers
        // fire on their own. Any OTHER power light caught in the blast goes into the chain-reaction
        // queue for the next wave (activators themselves are excluded).
        let wave = resolve_wave(&cascade_powers, &grid, &entity_info);
        vfx::trigger_wave_vfx(
            &mut commands,
            &wave,
            &grid,
            &entity_info,
            &mut pop_delays,
            &ray_settings,
        );
        let activator_positions: HashSet<GridPos> = cascade_powers.iter().map(|a| a.pos).collect();
        for e in wave.to_remove {
            if to_remove.contains(&e) {
                continue;
            }
            if let Some((pos, _, kind)) = entity_info.get(&e)
                && kind.is_power()
                && !activator_positions.contains(pos)
            {
                res.queue.0.push_back(PowerActivation {
                    pos: *pos,
                    kind: *kind,
                    partner_color: None,
                });
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
        &mut res.shadow_count.0,
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
            * res.cascade.0
    };

    let mut score_bonus = if result.score_reset {
        0
    } else {
        res.run.power_bonus(upgrades.len() as u32)
    };
    let mut reserve_bonus = 0;
    let mut blue_count = 0;
    for e in &to_remove {
        if let Some((_, color, kind)) = entity_info.get(e) {
            if kind.is_hollow() {
                continue;
            }
            res.collected_cores.0[color.index()] += res.cascade.0;
            let add = res.cascade.0;
            if !result.score_reset {
                score_bonus += res.run.score_bonus_for_color(*color, add);
                reserve_bonus += res.run.reserve_bonus_for_color(*color, add);
            }
            if *color == LightColor::Blue {
                blue_count += add;
            }
            match color {
                LightColor::Red => res.stats.reds += add,
                LightColor::Green => res.stats.greens += add,
                LightColor::Blue => res.stats.blues += add,
                LightColor::Yellow => res.stats.yellows += add,
                LightColor::Purple => res.stats.purples += add,
            }
            if kind.is_power() {
                res.stats.lightkinds += add;
            }
        }
    }
    if result.score_reset {
        res.score.0 = 0;
        res.displayed.0 = 0;
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
        res.score.0 += points + score_bonus;
        res.reserve.0 += points + reserve_bonus;
    }
    let move_bonus = res.run.blue_move_bonus(blue_count);
    if move_bonus > 0 && res.moves.0 != u32::MAX {
        res.moves.0 += move_bonus;
    }
    res.stats.max_cascade = res.stats.max_cascade.max(res.cascade.0);
    if res.cascade.0 >= 2 {
        res.stats.total_chains += 1;
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
        points: if result.score_reset {
            0
        } else {
            points + score_bonus
        },
        pops,
    });
    next_state.set(GameState::Popping);
}
