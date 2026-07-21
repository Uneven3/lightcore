use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use super::{
    CaptureBatch, CascadeDepth, CollectedCores, CoreReserve, GameMode, MovesLeft,
    PowerActivationQueue, Score, ShadowCount, SparksCollected, StatsBook, SuperComboPending,
};
use super::{rewards, vfx};
use crate::board::{HOLLOW_BASE_CHANCE, clear_shadow_at, shuffle_board};
use crate::core::prelude::*;
use crate::core::run::RunState;
use crate::gameplay::MatchTiming;
use crate::state::MatchPhase;

#[derive(SystemParam)]
pub(crate) struct ChainParams<'w> {
    pub(crate) score: ResMut<'w, Score>,
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
    mut next_state: ResMut<NextState<MatchPhase>>,
    mut shadow_q: Query<(Entity, &GridPos, Option<&mut HardShadow>), With<AdjacentMatchDamage>>,
    mut lights: Query<
        (Entity, &mut GridPos, &LightColor, &mut LightKind),
        (With<Light>, Without<AdjacentMatchDamage>, Without<Spark>),
    >,
    ray_settings: Res<MatchTiming>,
) {
    res.cascade.0 += 1;
    let (grid, mut entity_info) =
        build_grid_info(lights.iter().map(|(e, p, c, k)| (e, *p, *c, *k)));

    // FASE 1: Drenar la queue UNA activación por ciclo, en orden de tier descendente — dispara
    // la primera activación encolada del tier más alto presente; todas las demás esperan su
    // propio ciclo completo Popping→Falling→Spawning→CheckingChain. Antes se drenaba TODO de
    // una vez y una cadena grande detonaba como un único flash simultáneo ilegible; así cada
    // detonación es un beat individual de la cascada (Blackholes primero, Rays al final), con
    // sus propios VFX/pops/refill. Nada de esto pierde combos: las activaciones encoladas
    // nunca se combinan en pares de todos modos, porque para cuando la queue drena sus celdas
    // ya fueron repobladas por el refill y `resolve_wave` ve ocupantes Normal (los pares solo
    // se forman en la ola inicial de `resolve_match_sequence`, donde los powers siguen vivos).
    if !res.queue.0.is_empty() {
        let top_tier = res
            .queue
            .0
            .iter()
            .map(|a| a.kind.corelights())
            .max()
            .expect("queue no vacía");
        let idx = res
            .queue
            .0
            .iter()
            .position(|a| a.kind.corelights() == top_tier)
            .expect("top_tier proviene de esta misma queue");
        let activations: Vec<PowerActivation> = vec![res.queue.0.remove(idx).expect("idx válido")];
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

        vfx::stage_power_impact_jelly(
            &mut commands,
            &activations,
            &grid,
            &entity_info,
            &final_remove,
            &ray_settings,
        );

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

        rewards::apply_removal_rewards(
            &mut commands,
            &final_remove,
            &entity_info,
            res.cascade.0,
            false,
            0,
            &mut rewards::EconomyState {
                score: &mut res.score,
                reserve: &mut res.reserve,
                collected_cores: &mut res.collected_cores,
                stats: &mut res.stats,
                moves: &mut res.moves,
                run: &mut res.run,
                color_values: res.level.color_values,
            },
        );
        let pops = rewards::spawn_pops(
            &mut commands,
            &final_remove,
            &entity_info,
            &pop_delays,
            ray_settings.pop_duration,
            &res.run,
            res.cascade.0,
            false,
        );
        commands.trigger(CaptureBatch {
            removed: final_remove.len() as u32,
            cascade_depth: res.cascade.0,
            hollow: false,
            captures: pops,
        });
        next_state.set(MatchPhase::Popping);
        return;
    }

    // FASE 2: Queue vacía — buscar matches en cascada
    let result = scan_runs(&grid, &entity_info, None);

    if result.to_remove.is_empty() && result.to_upgrade.is_empty() {
        // FASE 3: No hay más matches — revisar condición de nivel
        let level_complete = res
            .level
            .goal_status(GoalFacts {
                score: res.score.0,
                sparks: res.collected.0,
                shadows: res.shadow_count.0,
                collected_cores: res.collected_cores.0,
                ..default()
            })
            .complete;
        if level_complete {
            next_state.set(MatchPhase::LevelComplete);
            return;
        }
        if res.moves.0 == 0 {
            next_state.set(MatchPhase::GameOver);
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
        next_state.set(MatchPhase::Playing);
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
            reserve: &mut res.reserve,
            collected_cores: &mut res.collected_cores,
            stats: &mut res.stats,
            moves: &mut res.moves,
            run: &mut res.run,
            color_values: res.level.color_values,
        },
    );
    next_state.set(MatchPhase::Popping);
}
