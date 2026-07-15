use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use super::{
    CascadeDepth, ChainPop, CollectedCores, CoreReserve, DisplayedScore, GameMode, MovesLeft,
    PowerActivationQueue, Score, ShadowCount, SparksCollected, StatsBook,
    SuperComboPending,
};
use super::{rewards, vfx};
use crate::board::{HOLLOW_BASE_CHANCE, clear_shadow_at, shuffle_board};
use crate::core::prelude::*;
use crate::visuals::RaySettings;
use crate::core::run::RunState;
use crate::state::GameState;

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

        let points = rewards::apply_removal_rewards(
            &mut commands,
            &final_remove,
            &entity_info,
            res.cascade.0,
            false,
            0,
            &mut rewards::EconomyState {
                score: &mut res.score,
                displayed: &mut res.displayed,
                reserve: &mut res.reserve,
                collected_cores: &mut res.collected_cores,
                stats: &mut res.stats,
                moves: &mut res.moves,
                run: &mut res.run,
            },
        );
        let pops = rewards::spawn_pops(
            &mut commands,
            &final_remove,
            &entity_info,
            &pop_delays,
            ray_settings.pop_duration,
        );
        commands.trigger(ChainPop {
            removed: final_remove.len() as u32,
            points,
            hollow: false,
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
            shuffle_board(&mut commands, &light_pairs, hollow_chance);
        }
        next_state.set(GameState::Playing);
        return;
    }

    rewards::resolve_match_sequence(
        &mut commands,
        &grid,
        &mut entity_info,
        res.cascade.0,
        result,
        None,
        &ray_settings,
        &mut lights,
        &mut shadow_q,
        &mut res.shadow_count.0,
        &mut res.queue,
        &mut res.super_combo,
        &mut rewards::EconomyState {
            score: &mut res.score,
            displayed: &mut res.displayed,
            reserve: &mut res.reserve,
            collected_cores: &mut res.collected_cores,
            stats: &mut res.stats,
            moves: &mut res.moves,
            run: &mut res.run,
        },
    );
    next_state.set(GameState::Popping);
}
