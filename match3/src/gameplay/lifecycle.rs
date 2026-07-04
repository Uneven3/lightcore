use bevy::prelude::*;
use rand::Rng;
use std::collections::HashSet;

use super::{
    CollectedCores, CoreReserve, CoresSpent, DragState, GameMode, LevelTimer, MovesLeft,
    ResetParams, Score, ShadowCount, StatsBook,
};
use crate::board::{
    generate_board, spawn_blocker, spawn_ingredient_exits, spawn_light, spawn_shadow, spawn_sparks,
};
use crate::core::campaign::CampaignProgress;
use crate::core::prelude::*;
use crate::core::run::RunState;
use crate::input::InputActions;
use crate::state::GameState;
use crate::visuals::EffectAnim;
use crate::visuals::assets::VisualCache;
use crate::visuals::light_trail::{LaserBolt, TravelingLight};
use crate::visuals::particles::Particle;
use crate::visuals::score_light::ScoreShard;

/// `LevelTimer` value a level should start with, derived from its goal — `Some` (ticking down)
/// only for timed-score levels, `None` (inert) for everything else.
fn level_timer_for(goal: &LevelGoal) -> LevelTimer {
    match goal {
        LevelGoal::TimedScore { secs, .. } => {
            LevelTimer(Some(Timer::from_seconds(*secs, TimerMode::Once)))
        }
        _ => LevelTimer(None),
    }
}

/// See `gameplay/mod.rs`'s `LevelTimer` doc comment for why this exists, and the plan note on
/// `NextState` being a single overwritable slot for why this uses `is_finished()` (stays true on
/// every tick after expiry) instead of `just_finished()` (true only once): a system elsewhere in
/// the swap/pop/fall/spawn/chain pipeline can call `next_state.set(...)` in the same frame and
/// silently clobber a one-shot transition. Retrying every frame until nothing else wins that
/// frame's `NextState` write is what actually guarantees the transition lands.
pub(crate) fn tick_level_timer(
    time: Res<Time>,
    mut level_timer: ResMut<LevelTimer>,
    level: Res<LevelConfig>,
    score: Res<Score>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if level_timer.0.is_none() {
        return;
    }
    let t = level_timer.0.as_mut().unwrap();
    t.tick(time.delta());
    if t.is_finished()
        && let LevelGoal::TimedScore { target, .. } = &level.goal
    {
        if score.0 >= *target {
            next_state.set(GameState::LevelComplete);
        } else {
            next_state.set(GameState::GameOver);
        }
    }
}

/// `GameMode::ConsumeAll` ("Blackhole") win condition: clearing the entire board wins. The board
/// refills every turn, so the only moment it's truly empty is right after a board-clearing wave
/// pops everything — caught here, on entry to `Falling` (before `Spawning` refills). Mode-gated, so
/// the Classic modes (and a Classic super-combo, which clears the board then respawns a row of
/// powers from `SuperComboPending`) are untouched. Safe from false positives: the board is only
/// ever fully empty after a total clear, never at setup or between ordinary cascades.
pub(crate) fn check_board_consumed(
    mode: Res<GameMode>,
    lights: Query<(), With<Light>>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if *mode == GameMode::ConsumeAll && lights.is_empty() {
        next_state.set(GameState::LevelComplete);
    }
}

#[derive(Component)]
pub(crate) struct GameOverOverlay;

#[derive(Component)]
pub(crate) struct LevelCompleteOverlay;

/// Spawns a full board (lights, plus sparks/shadow per the level's goal) for `level` —
/// shared by `setup_level`/`handle_level_advance`/`handle_restart` so the three can't drift
/// out of sync with each other.
fn populate_board(
    commands: &mut Commands,
    cache: &VisualCache,
    level: &LevelConfig,
    shadow_count: &mut u32,
) {
    let spark_positions: HashSet<GridPos> = if level.goal == LevelGoal::Sparks {
        spark_columns(level.sparks_total)
            .iter()
            .map(|&x| GridPos { x, y: GRID_H - 1 })
            .collect()
    } else {
        HashSet::new()
    };

    let blocker_positions: HashSet<GridPos> = level.blocker_positions.iter().copied().collect();
    let mut blocked_positions = spark_positions.clone();
    blocked_positions.extend(blocker_positions.iter().copied());

    let mut rng = rand::rng();
    for (pos, color) in generate_board(&mut rng, &blocked_positions) {
        if !spark_positions.contains(&pos) && !blocker_positions.contains(&pos) {
            spawn_light(
                commands,
                cache,
                pos,
                color,
                LightKind::Normal,
                to_world(pos),
            );
        }
    }

    for &pos in &level.blocker_positions {
        spawn_blocker(commands, cache, pos);
    }

    if level.goal == LevelGoal::Sparks {
        let columns = spark_columns(level.sparks_total);
        spawn_ingredient_exits(commands, cache);
        spawn_sparks(commands, cache, &columns);
    }

    if level.goal == LevelGoal::ClearShadow {
        *shadow_count = level.shadow_positions.len() as u32;
        for &pos in &level.shadow_positions {
            spawn_shadow(commands, cache, pos);
        }
    }
}

fn spark_columns(count: u32) -> Vec<i32> {
    match count {
        0 => vec![],
        1 => vec![4],
        2 => vec![3, 5],
        3 => vec![2, 4, 6],
        4 => vec![1, 3, 5, 7],
        _ => vec![0, 2, 4, 6, GRID_W - 1],
    }
}

/// A Blackhole board: just normal lights, no sparks/shadow. Power lights are forged and detonated
/// by the shared Classic pipeline (`gameplay::chain`/`swap`) during play.
fn populate_blackhole_board(commands: &mut Commands, cache: &VisualCache) {
    let mut rng = rand::rng();
    for (pos, color) in generate_board(&mut rng, &HashSet::new()) {
        spawn_light(
            commands,
            cache,
            pos,
            color,
            LightKind::Normal,
            to_world(pos),
        );
    }
}

/// A Sandbox board: every cell gets a *random* `LightKind` so adjacent powers can be swapped to
/// exercise the combo animations. Colors still come from `generate_board` (which avoids initial
/// 3-runs, so nothing pops on entry); the kind is rolled per cell, biased toward powers since the
/// whole point is to watch interactions. Normals are still seeded in so ordinary matches remain
/// possible.
fn populate_sandbox_board(commands: &mut Commands, cache: &VisualCache) {
    // Weighted roll: Normal kept common enough that plain matches still happen, the rest spread
    // across every power so all of them are reachable on a fresh board.
    const KINDS: [(LightKind, u32); 7] = [
        (LightKind::Normal, 6),
        (LightKind::RayH, 3),
        (LightKind::RayV, 3),
        (LightKind::Supernova, 2),
        (LightKind::Cross, 2),
        (LightKind::Starburst, 2),
        (LightKind::Blackhole, 1),
    ];
    let total: u32 = KINDS.iter().map(|(_, w)| w).sum();

    let mut rng = rand::rng();
    for (pos, color) in generate_board(&mut rng, &HashSet::new()) {
        let mut roll = rng.random_range(0..total);
        let kind = KINDS
            .iter()
            .find_map(|(k, w)| {
                if roll < *w {
                    Some(*k)
                } else {
                    roll -= *w;
                    None
                }
            })
            .unwrap_or(LightKind::Normal);
        spawn_light(commands, cache, pos, color, kind, to_world(pos));
    }
}

/// Builds the board for the chosen `GameMode` on `OnEnter(Loading)`, then advances to `Playing`.
/// Kept out of `OnEnter(Playing)` because `Playing` is re-entered on every cascade settle.
pub(crate) fn setup_match(
    mut commands: Commands,
    cache: Res<VisualCache>,
    mode: Res<GameMode>,
    mut run: ResMut<RunState>,
    mut level: ResMut<LevelConfig>,
    mut moves: ResMut<MovesLeft>,
    mut shadow_count: ResMut<ShadowCount>,
    mut reserve: ResMut<CoreReserve>,
    mut spent: ResMut<CoresSpent>,
    mut level_timer: ResMut<LevelTimer>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    match *mode {
        GameMode::Classic(level_n) => {
            *level = make_level(level_n);
            moves.0 = level.total_moves;
            shadow_count.0 = 0;
            reserve.0 = 0;
            spent.0 = 0;
            *level_timer = level_timer_for(&level.goal);
            populate_board(&mut commands, &cache, &level, &mut shadow_count.0);
        }
        GameMode::Run(depth) => {
            run.enter_depth(depth);
            *level = make_generated_level(depth, run.seed);
            moves.0 = level.total_moves;
            shadow_count.0 = 0;
            reserve.0 = 0;
            spent.0 = 0;
            *level_timer = level_timer_for(&level.goal);
            populate_board(&mut commands, &cache, &level, &mut shadow_count.0);
        }
        GameMode::ConsumeAll | GameMode::Sandbox => {
            // Sandbox modes: run the Classic detonation pipeline with no win/lose yet. An
            // unreachable goal + infinite moves means `check_chain_matches` FASE 3 never completes
            // nor game-overs (it just reshuffles if the player gets stuck). Esc returns to the menu.
            // (`ConsumeAll` still wins on a total clear via `check_board_consumed`.)
            *level = LevelConfig {
                level: 0,
                total_moves: u32::MAX,
                goal: LevelGoal::Score(u32::MAX),
                sparks_total: 0,
                shadow_positions: vec![],
                blocker_positions: vec![],
                grade_baseline: 0,
            };
            moves.0 = u32::MAX;
            reserve.0 = 0;
            spent.0 = 0;
            *level_timer = LevelTimer(None);
            shadow_count.0 = 0;
            if *mode == GameMode::Sandbox {
                populate_sandbox_board(&mut commands, &cache);
            } else {
                populate_blackhole_board(&mut commands, &cache);
            }
        }
    }
    next_state.set(GameState::Playing);
}

/// Tears the match down when returning to the menu: despawns every match entity (lights+sparks and
/// their core/glow children, shadows, particles, traveling beams, blast effects, score shards,
/// overlays) and resets all pipeline resources, so a fresh mode starts clean.
pub(crate) fn teardown_match(
    mut commands: Commands,
    mut level: ResMut<LevelConfig>,
    mut res: ResetParams,
    match_entities: Query<
        Entity,
        Or<(
            With<FallPhysics>,
            With<Shadow>,
            With<Particle>,
            With<TravelingLight>,
            With<EffectAnim>,
            With<ScoreShard>,
            With<GameOverOverlay>,
            With<LevelCompleteOverlay>,
            With<IngredientExit>,
        )>,
    >,
) {
    for e in &match_entities {
        commands.entity(e).try_despawn();
    }
    *level = make_level(1);
    *res.level_timer = LevelTimer(None);
    res.score.0 = 0;
    res.displayed.0 = 0;
    res.reserve.0 = 0;
    res.spent.0 = 0;
    res.moves.0 = level.total_moves;
    res.pending.0 = None;
    *res.drag = DragState::default();
    res.settled.0 = false;
    res.cascade.0 = 0;
    res.collected.0 = 0;
    res.shadow.0 = 0;
    res.queue.0.clear();
    res.super_combo.0.clear();
    res.collected_cores.0 = [0; 5];
    res.displayed_cores.0 = [0; 5];
    res.stats.reds = 0;
    res.stats.greens = 0;
    res.stats.blues = 0;
    res.stats.yellows = 0;
    res.stats.purples = 0;
    res.stats.lightkinds = 0;
    res.stats.max_cascade = 0;
    res.stats.total_chains = 0;
    res.popup_open.0 = false;
}

pub(crate) fn show_level_complete(
    mut commands: Commands,
    mode: Res<GameMode>,
    mut run: ResMut<RunState>,
    score: Res<Score>,
    reserve: Res<CoreReserve>,
    spent: Res<CoresSpent>,
    level: Res<LevelConfig>,
    mut progress: ResMut<CampaignProgress>,
    collected_cores: Res<CollectedCores>,
    stats: Res<StatsBook>,
) {
    let title = if *mode == GameMode::ConsumeAll {
        "Tablero Consumido!"
    } else {
        "Nivel Completado!"
    };

    let details = if *mode == GameMode::ConsumeAll {
        format!("Lightcores capturados: {}", score.0)
    } else {
        let unlock = progress.record_score(level.level, score.0);
        if mode.is_run() {
            run.complete_depth(level.level);
        }
        format!(
            "Lightcores capturados: {}\n{}",
            score.0,
            level_complete_meta(&unlock)
        )
    };

    bevy::log::info!(
        "match_end result=win mode={:?} level={} captured={} reserve={} spent={} outcome=\"{}\"",
        *mode,
        level.level,
        score.0,
        reserve.0,
        spent.0,
        if *mode == GameMode::ConsumeAll {
            "tablero consumido".to_string()
        } else {
            goal_outcome_summary(&level, score.0, 0, 0, &collected_cores)
        }
    );

    commands
        .spawn((
            LevelCompleteOverlay,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.65)),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        width: Val::Px(360.0),
                        padding: UiRect::all(Val::Px(24.0)),
                        border: UiRect::all(Val::Px(2.0)),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(16.0),
                        ..default()
                    },
                    BorderColor::all(Color::srgba(0.65, 0.85, 1.0, 0.35)),
                    BackgroundColor(Color::srgba(0.08, 0.08, 0.15, 0.95)),
                ))
                .with_children(|card| {
                    card.spawn((
                        Text::new(title),
                        TextFont {
                            font_size: FontSize::Px(26.0),
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.85, 0.2)),
                    ));

                    card.spawn((
                        Text::new(details),
                        TextFont {
                            font_size: FontSize::Px(15.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.8, 0.9, 1.0)),
                    ));

                    // Resumen de Estadisticas
                    card.spawn((Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(4.0),
                        margin: UiRect::vertical(Val::Px(8.0)),
                        ..default()
                    },))
                        .with_children(|summary| {
                            summary.spawn((
                                Text::new("Resumen de partida:"),
                                TextFont {
                                    font_size: FontSize::Px(13.0),
                                    ..default()
                                },
                                TextColor(Color::srgb(0.65, 0.85, 1.0)),
                            ));
                            if stats.reds > 0 {
                                summary.spawn((
                                    Text::new(format!("Rojos: {}", stats.reds)),
                                    TextFont {
                                        font_size: FontSize::Px(12.0),
                                        ..default()
                                    },
                                    TextColor(Color::srgb(0.9, 0.9, 0.9)),
                                ));
                            }
                            if stats.greens > 0 {
                                summary.spawn((
                                    Text::new(format!("Verdes: {}", stats.greens)),
                                    TextFont {
                                        font_size: FontSize::Px(12.0),
                                        ..default()
                                    },
                                    TextColor(Color::srgb(0.9, 0.9, 0.9)),
                                ));
                            }
                            if stats.blues > 0 {
                                summary.spawn((
                                    Text::new(format!("Azules: {}", stats.blues)),
                                    TextFont {
                                        font_size: FontSize::Px(12.0),
                                        ..default()
                                    },
                                    TextColor(Color::srgb(0.9, 0.9, 0.9)),
                                ));
                            }
                            if stats.yellows > 0 {
                                summary.spawn((
                                    Text::new(format!("Amarillos: {}", stats.yellows)),
                                    TextFont {
                                        font_size: FontSize::Px(12.0),
                                        ..default()
                                    },
                                    TextColor(Color::srgb(0.9, 0.9, 0.9)),
                                ));
                            }
                            if stats.purples > 0 {
                                summary.spawn((
                                    Text::new(format!("Morados: {}", stats.purples)),
                                    TextFont {
                                        font_size: FontSize::Px(12.0),
                                        ..default()
                                    },
                                    TextColor(Color::srgb(0.9, 0.9, 0.9)),
                                ));
                            }
                            if stats.lightkinds > 0 {
                                summary.spawn((
                                    Text::new(format!("Chispas: {}", stats.lightkinds)),
                                    TextFont {
                                        font_size: FontSize::Px(12.0),
                                        ..default()
                                    },
                                    TextColor(Color::srgb(0.9, 0.9, 0.9)),
                                ));
                            }
                            summary.spawn((
                                Text::new(format!("Max Cascada: {}", stats.max_cascade)),
                                TextFont {
                                    font_size: FontSize::Px(12.0),
                                    ..default()
                                },
                                TextColor(Color::srgb(0.9, 0.9, 0.9)),
                            ));
                            summary.spawn((
                                Text::new(format!("Cadenas: {}", stats.total_chains)),
                                TextFont {
                                    font_size: FontSize::Px(12.0),
                                    ..default()
                                },
                                TextColor(Color::srgb(0.9, 0.9, 0.9)),
                            ));
                        });

                    card.spawn((
                        Text::new("[Click/Tap o Espacio] para continuar"),
                        TextFont {
                            font_size: FontSize::Px(12.0),
                            ..default()
                        },
                        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.45)),
                    ));
                });
        });
}

/// Resetea todos los recursos de partida para el nivel dado. Compartido por
/// `handle_level_advance` y `handle_restart` — evita que los dos bloques diverjan.
fn reset_for_replay(replay_level: LevelConfig, level: &mut LevelConfig, res: &mut ResetParams) {
    *level = replay_level;
    *res.level_timer = level_timer_for(&level.goal);
    res.score.0 = 0;
    res.displayed.0 = 0;
    res.reserve.0 = 0;
    res.spent.0 = 0;
    res.moves.0 = level.total_moves;
    res.pending.0 = None;
    *res.drag = DragState::default();
    res.settled.0 = false;
    res.cascade.0 = 0;
    res.collected.0 = 0;
    res.shadow.0 = 0;
    res.queue.0.clear();
    res.super_combo.0.clear();
    res.collected_cores.0 = [0; 5];
    res.displayed_cores.0 = [0; 5];
    res.stats.reds = 0;
    res.stats.greens = 0;
    res.stats.blues = 0;
    res.stats.yellows = 0;
    res.stats.purples = 0;
    res.stats.lightkinds = 0;
    res.stats.max_cascade = 0;
    res.stats.total_chains = 0;
    res.popup_open.0 = false;
}

fn level_complete_meta(unlock: &crate::core::campaign::CampaignUnlockResult) -> String {
    if let Some(next_level) = unlock.unlocked_next {
        format!("Nivel {:02} desbloqueado", next_level)
    } else if unlock.new_best {
        "Nuevo mejor score registrado".to_string()
    } else {
        "Nivel ya completado".to_string()
    }
}

pub(crate) fn handle_level_advance(
    actions: Res<InputActions>,
    mode: Res<GameMode>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if !actions.confirm {
        return;
    }

    if mode.is_sandbox() || matches!(*mode, GameMode::Classic(_) | GameMode::Run(_)) {
        next_state.set(GameState::LevelMenu);
    }
}

pub(crate) fn show_game_over(
    mut commands: Commands,
    mode: Res<GameMode>,
    score: Res<Score>,
    reserve: Res<CoreReserve>,
    spent: Res<CoresSpent>,
    moves: Res<MovesLeft>,
    level: Res<LevelConfig>,
    sparks: Res<crate::gameplay::SparksCollected>,
    shadow_count: Res<ShadowCount>,
    collected_cores: Res<CollectedCores>,
) {
    let outcome = goal_outcome_summary(&level, score.0, sparks.0, shadow_count.0, &collected_cores);
    bevy::log::info!(
        "match_end result=loss mode={:?} level={} captured={} reserve={} spent={} moves_left={} outcome=\"{}\"",
        *mode,
        level.level,
        score.0,
        reserve.0,
        spent.0,
        moves.0,
        outcome
    );
    let details = format!("Lightcores capturados: {}\n{}", score.0, outcome);
    commands
        .spawn((
            GameOverOverlay,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.65)),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        width: Val::Px(360.0),
                        padding: UiRect::all(Val::Px(24.0)),
                        border: UiRect::all(Val::Px(2.0)),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(16.0),
                        ..default()
                    },
                    BorderColor::all(Color::srgba(1.0, 0.45, 0.45, 0.35)),
                    BackgroundColor(Color::srgba(0.12, 0.06, 0.06, 0.95)),
                ))
                .with_children(|card| {
                    card.spawn((
                        Text::new("Fin de Partida"),
                        TextFont {
                            font_size: FontSize::Px(26.0),
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.45, 0.45)),
                    ));

                    card.spawn((
                        Text::new(details),
                        TextFont {
                            font_size: FontSize::Px(15.0),
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.85, 0.85)),
                    ));

                    card.spawn((
                        Text::new("[Click/Tap o Espacio] para reiniciar"),
                        TextFont {
                            font_size: FontSize::Px(12.0),
                            ..default()
                        },
                        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.45)),
                    ));
                });
        });
}

pub(crate) fn handle_restart(
    mut commands: Commands,
    actions: Res<InputActions>,
    mode: Res<GameMode>,
    run: Res<RunState>,
    mut level: ResMut<LevelConfig>,
    mut res: ResetParams,
    mut next_state: ResMut<NextState<GameState>>,
    cache: Res<VisualCache>,
    physics_entities: Query<Entity, With<FallPhysics>>,
    shadow_entities: Query<Entity, With<Shadow>>,
    exit_entities: Query<Entity, With<IngredientExit>>,
    overlay: Query<Entity, With<GameOverOverlay>>,
    vfx_entities: Query<
        Entity,
        Or<(
            With<Particle>,
            With<TravelingLight>,
            With<EffectAnim>,
            With<ScoreShard>,
            With<LaserBolt>,
        )>,
    >,
) {
    if !actions.confirm {
        return;
    }

    if mode.is_sandbox() {
        next_state.set(GameState::LevelMenu);
        return;
    }

    for e in &physics_entities {
        commands.entity(e).try_despawn();
    }
    for e in &shadow_entities {
        commands.entity(e).try_despawn();
    }
    for e in &exit_entities {
        commands.entity(e).try_despawn();
    }
    for e in &overlay {
        commands.entity(e).try_despawn();
    }
    for e in &vfx_entities {
        commands.entity(e).try_despawn();
    }

    let replay_level = match *mode {
        GameMode::Classic(level_num) => make_level(level_num),
        GameMode::Run(depth) => make_generated_level(depth, run.seed),
        GameMode::ConsumeAll | GameMode::Sandbox => make_level(level.level),
    };
    reset_for_replay(replay_level, &mut level, &mut res);
    populate_board(&mut commands, &cache, &level, &mut res.shadow.0);
    next_state.set(GameState::Playing);
}

fn goal_outcome_summary(
    level: &LevelConfig,
    score: u32,
    sparks: u32,
    shadow_count: u32,
    collected_cores: &CollectedCores,
) -> String {
    match &level.goal {
        LevelGoal::Score(target) => {
            format!("Faltaron {} lightcores", target.saturating_sub(score))
        }
        LevelGoal::Sparks => {
            format!(
                "Faltaron {} chispas",
                level.sparks_total.saturating_sub(sparks)
            )
        }
        LevelGoal::ClearShadow => {
            format!("Quedaron {} sombras", shadow_count)
        }
        LevelGoal::TimedScore { target, .. } => {
            format!(
                "Faltaron {} lightcores antes del reloj",
                target.saturating_sub(score)
            )
        }
        LevelGoal::CollectColor { color, target } => {
            let current = collected_cores.0[color.index()];
            let color_name = match color {
                LightColor::Red => "rojos",
                LightColor::Green => "verdes",
                LightColor::Blue => "azules",
                LightColor::Yellow => "amarillos",
                LightColor::Purple => "morados",
            };
            format!(
                "Faltaron {} cores {}",
                target.saturating_sub(current),
                color_name
            )
        }
    }
}
