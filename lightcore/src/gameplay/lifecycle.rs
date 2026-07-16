use bevy::prelude::*;
use rand::Rng;
use std::collections::HashSet;

use super::{
    CollectedCores, CoreReserve, CoresSpent, DragState, GameMode, LevelTimer, MovesLeft,
    ResetParams, Score, ShadowCount, StatsBook,
};
use crate::board::{
    HOLLOW_BASE_CHANCE, generate_board, spawn_blocker, spawn_ingredient_exits,
    spawn_light, spawn_shadow, spawn_sparks, spawn_stasis_cover,
};
use crate::core::campaign::CampaignProgress;
use crate::core::locale::{Language, TrKey};
use crate::core::prelude::*;
use crate::core::run::{BoonKind, RUN_LEVELS, RunState};
use crate::input::InputActions;
use crate::menu::options::WindowSettings;
use crate::state::GameState;
use crate::visuals::EffectAnim;
use crate::visuals::assets::VisualCache;
use crate::visuals::light_trail::{LaserBolt, TravelingLight};
use crate::visuals::particles::Particle;
use crate::visuals::score_light::{ScoreShard, ScoreShardAbsorb, ScoreShardScatter};

/// Every transient entity kind spawned during a match — board pieces, shadows, VFX, score shards
/// in every phase (capture flight, drain flight, drain scatter), and the end-of-match overlays.
/// One shared definition so `teardown_match` and `handle_restart` can't drift the way they used to
/// (a prior version had `teardown_match` never clearing `LaserBolt`, and NEITHER clearing
/// `ScoreShardAbsorb`/`ScoreShardScatter` — the shards `visuals::score_light::on_score_drained`
/// spawns when the shop drains the reserve — leaving orphaned shards on screen after a
/// restart/menu-exit mid-drain).
type TransientMatchEntity = Or<(
    With<FallPhysics>,
    With<Shadow>,
    With<Particle>,
    With<TravelingLight>,
    With<EffectAnim>,
    With<ScoreShard>,
    With<ScoreShardAbsorb>,
    With<ScoreShardScatter>,
    With<LaserBolt>,
    With<GameOverOverlay>,
    With<LevelCompleteOverlay>,
    With<IngredientExit>,
    With<StasisCover>,
)>;

/// `LevelTimer` value a level should start with, derived from its goal — `Some` (ticking down)
/// only for timed-score levels, `None` (inert) for everything else.
fn level_timer_for(goal: &LevelGoal) -> LevelTimer {
    match goal {
        LevelGoal::TimedScore { secs, .. } | LevelGoal::TimedCollectColor { secs, .. } => {
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
    collected_cores: Res<CollectedCores>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    let Some(t) = level_timer.0.as_mut() else {
        return;
    };
    t.tick(time.delta());
    if t.is_finished() {
        match &level.goal {
            LevelGoal::TimedScore { target, .. } => {
                if score.0 >= *target {
                    next_state.set(GameState::LevelComplete);
                } else {
                    next_state.set(GameState::GameOver);
                }
            }
            LevelGoal::TimedCollectColor { color, target, .. } => {
                if collected_cores.0[color.index()] >= *target {
                    next_state.set(GameState::LevelComplete);
                } else {
                    next_state.set(GameState::GameOver);
                }
            }
            _ => {}
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

#[derive(Component, Clone, Copy)]
pub(crate) struct LevelRewardButton(BoonKind);

#[derive(Component)]
pub(crate) struct LevelRewardInstructionText;

#[derive(Resource, Default)]
pub(crate) struct LevelRewardOffer {
    offered: Vec<BoonKind>,
    purchased: Vec<BoonKind>,
}

impl LevelRewardOffer {
    fn reset(&mut self, offered: Vec<BoonKind>) {
        self.offered = offered;
        self.purchased.clear();
    }

    fn active(&self) -> bool {
        !self.offered.is_empty()
    }
}

/// Spawns a full board (lights, plus sparks/shadow per the level's goal) for `level` —
/// shared by `setup_level`/`handle_level_advance`/`handle_restart` so the three can't drift
/// out of sync with each other.
fn populate_board(
    commands: &mut Commands,
    cache: &VisualCache,
    level: &LevelConfig,
    shadow_count: &mut u32,
    hollow_chance: f32,
    weights: [f32; 5],
    layout: &GridLayout,
) {
    let spark_positions: HashSet<GridPos> = if level.goal == LevelGoal::Sparks {
        spark_columns(level.sparks_total)
            .into_iter()
            .filter_map(|x| layout.top_cell_in_column(x))
            .collect()
    } else {
        HashSet::new()
    };

    let blocker_positions: HashSet<GridPos> = level.blocker_positions.iter().copied().collect();
    let mut blocked_positions = spark_positions.clone();
    blocked_positions.extend(blocker_positions.iter().copied());
    let stasis_positions: HashSet<GridPos> = level.shadow_positions.iter().copied().collect();

    let mut rng = rand::rng();
    for (pos, color, kind) in generate_board(&mut rng, &blocked_positions, hollow_chance, weights) {
        if !spark_positions.contains(&pos) && !blocker_positions.contains(&pos) {
            let light = spawn_light(commands, pos, color, kind, to_world(pos));
            if stasis_positions.contains(&pos) {
                commands
                    .entity(light)
                    .insert((Stasis, BlocksGravity, BlocksInteraction))
                    .remove::<Movable>();
            }
        }
    }

    for &pos in &level.blocker_positions {
        spawn_blocker(commands, cache, pos);
    }

    if level.goal == LevelGoal::Sparks {
        spawn_ingredient_exits(commands, cache, layout.spark_exits());
        spawn_sparks(commands, cache, spark_positions.iter().copied());
    }

    if level.goal == LevelGoal::ClearShadow {
        // Preserve the level's old composition: both covers sit over their existing lightcores.
        // Only the future `DeepShadow` variant is an empty cell.
        *shadow_count = (level.shadow_positions.len() + level.hard_shadow_positions.len()) as u32;
        for &pos in &level.shadow_positions {
            spawn_stasis_cover(commands, cache, pos);
        }
        for &pos in &level.hard_shadow_positions {
            spawn_shadow(commands, cache, pos);
        }
    }
}

/// Stasis lights are valid match members and disappear through the normal pop pipeline. Keep the
/// clear-obstacle objective in sync when that happens, just as `clear_shadow_at` does for cells.
pub(crate) fn account_removed_stasis(
    mut removed: RemovedComponents<Stasis>,
    mut shadow_count: ResMut<ShadowCount>,
) {
    for _ in removed.read() {
        shadow_count.0 = shadow_count.0.saturating_sub(1);
    }
}

/// The cyan cover is a separate render entity so it must be removed when its owning stasis light
/// is matched and despawned. Matching by cell keeps the visual layer independent from gameplay.
pub(crate) fn despawn_orphan_stasis_covers(
    mut commands: Commands,
    covers: Query<(Entity, &GridPos), With<StasisCover>>,
    stasis: Query<&GridPos, With<Stasis>>,
) {
    for (entity, pos) in &covers {
        if !stasis.iter().any(|stasis_pos| stasis_pos == pos) {
            commands.entity(entity).try_despawn();
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
fn populate_blackhole_board(commands: &mut Commands) {
    let mut rng = rand::rng();
    for (pos, color, kind) in generate_board(&mut rng, &HashSet::new(), HOLLOW_BASE_CHANCE, [1.0; 5]) {
        spawn_light(commands, pos, color, kind, to_world(pos));
    }
}

/// A Sandbox board: every cell gets a *random* `LightKind` so adjacent powers can be swapped to
/// exercise the combo animations. Colors still come from `generate_board` (which avoids initial
/// 3-runs, so nothing pops on entry); the kind is rolled per cell, biased toward powers since the
/// whole point is to watch interactions. Normals are still seeded in so ordinary matches remain
/// possible.
fn populate_sandbox_board(commands: &mut Commands) {
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
    for (pos, color, _) in generate_board(&mut rng, &HashSet::new(), 0.0, [1.0; 5]) {
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
        spawn_light(commands, pos, color, kind, to_world(pos));
    }
}

/// One combo interaction to isolate per `GameMode::Debug(index)` — `(a_kind, a_color, b_kind,
/// b_color)`, placed at `DEBUG_ANCHOR_A`/`DEBUG_ANCHOR_B` (adjacent) so swapping them fires exactly
/// this interaction on the player's first move. Mirrors `core::matching::resolve_swap_activation`'s
/// arms one-to-one (see there for why each pairing produces the effect named in the comment) —
/// order matches `menu::level_menu`'s `MenuEntryKind::Debug` node order, so keep the two in sync.
/// `StarLine`/`StarSupernova`/`StarColor` name a partner color deliberately present in
/// `populate_debug_board`'s filler pattern, so the "clear every light of this color" effect has
/// several targets scattered around the board to actually demonstrate against, not just itself.
pub(crate) const DEBUG_SCENARIOS: [(LightKind, LightColor, LightKind, LightColor); 9] = [
    (
        LightKind::RayH,
        LightColor::Blue,
        LightKind::RayV,
        LightColor::Yellow,
    ), // DoubleLine
    (
        LightKind::RayH,
        LightColor::Yellow,
        LightKind::Supernova,
        LightColor::Red,
    ), // LineSupernova
    (
        LightKind::Supernova,
        LightColor::Red,
        LightKind::Supernova,
        LightColor::Blue,
    ), // DoubleSupernova
    (
        LightKind::Starburst,
        LightColor::Green,
        LightKind::RayH,
        LightColor::Green,
    ), // StarLine
    (
        LightKind::Starburst,
        LightColor::Purple,
        LightKind::Supernova,
        LightColor::Purple,
    ), // StarSupernova
    (
        LightKind::Starburst,
        LightColor::Red,
        LightKind::Starburst,
        LightColor::Blue,
    ), // StarStar
    (
        LightKind::Starburst,
        LightColor::Yellow,
        LightKind::Normal,
        LightColor::Green,
    ), // StarColor
    (
        LightKind::Blackhole,
        LightColor::Red,
        LightKind::RayH,
        LightColor::Blue,
    ), // Blackhole
    (
        LightKind::Starburst,
        LightColor::Blue,
        LightKind::Cross,
        LightColor::Blue,
    ), // StarLine (shuriken partner — sweeps its row *and* column)
];

const DEBUG_ANCHOR_A: GridPos = GridPos { x: 3, y: 4 };
const DEBUG_ANCHOR_B: GridPos = GridPos { x: 4, y: 4 };

/// A single isolated combo (`DEBUG_SCENARIOS[scenario]`) on an otherwise inert board. Every cell is
/// `Normal`, colored by `(x + y) % 5` — two orthogonally adjacent cells always land on different
/// colors under that pattern (moving one step always changes `x + y` by 1), so the filler can never
/// 3-match on its own — except `DEBUG_ANCHOR_A`/`_B`, which get the scenario's own kind/color pair.
/// The result: the only thing that can possibly happen on this board is the one interaction the
/// player came here to test, and it's guaranteed to fire the instant those two tiles are swapped.
fn populate_debug_board(commands: &mut Commands, scenario: u8) {
    let (a_kind, a_color, b_kind, b_color) =
        DEBUG_SCENARIOS[scenario as usize % DEBUG_SCENARIOS.len()];
    for x in 0..GRID_W {
        for y in 0..GRID_H {
            let pos = GridPos { x, y };
            let (kind, color) = if pos == DEBUG_ANCHOR_A {
                (a_kind, a_color)
            } else if pos == DEBUG_ANCHOR_B {
                (b_kind, b_color)
            } else {
                (
                    LightKind::Normal,
                    LightColor::from_index((x + y).rem_euclid(5) as usize),
                )
            };
            spawn_light(commands, pos, color, kind, to_world(pos));
        }
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
    mut layout: ResMut<GridLayout>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    *layout = GridLayout::default();
    match *mode {
        GameMode::Classic(level_n) => {
            *level = make_level(level_n);
            moves.0 = level.total_moves;
            shadow_count.0 = 0;
            reserve.0 = 0;
            spent.0 = 0;
            *level_timer = level_timer_for(&level.goal);
            populate_board(
                &mut commands,
                &cache,
                &level,
                &mut shadow_count.0,
                HOLLOW_BASE_CHANCE,
                [1.0; 5],
                &layout,
            );
        }
        GameMode::Run(depth) => {
            // The booster wallet (`CoreReserve`) is the run's persistent currency: it carries over
            // level to level so the shop stays meaningful across a run, and only wipes when a
            // fresh run actually starts (new run picked, or the previous one ended — see
            // `show_game_over`/`show_level_complete` for how `RunState::active` goes false).
            let new_run = run.enter_depth(depth);
            *level = make_generated_level(depth, run.seed);
            moves.0 = level.total_moves;
            shadow_count.0 = 0;
            if new_run {
                reserve.0 = 0;
                spent.0 = 0;
            }
            *level_timer = level_timer_for(&level.goal);
            let hollow_chance = run.hollow_spawn_chance(HOLLOW_BASE_CHANCE);
            let weights = run.color_weights();
            populate_board(
                &mut commands,
                &cache,
                &level,
                &mut shadow_count.0,
                hollow_chance,
                weights,
                &layout,
            );
        }
        GameMode::ConsumeAll | GameMode::Sandbox | GameMode::Debug(_) | GameMode::TeleportTest => {
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
                hard_shadow_positions: vec![],
                blocker_positions: vec![],
                grade_baseline: 0,
            };
            moves.0 = u32::MAX;
            reserve.0 = 0;
            spent.0 = 0;
            *level_timer = LevelTimer(None);
            shadow_count.0 = 0;
            match *mode {
                GameMode::Sandbox => populate_sandbox_board(&mut commands),
                GameMode::Debug(scenario) => populate_debug_board(&mut commands, scenario),
                GameMode::TeleportTest => populate_teleport_board(&mut commands, &mut layout),
                _ => populate_blackhole_board(&mut commands),
            }
        }
    }
    next_state.set(GameState::Playing);
}

fn populate_teleport_board(commands: &mut Commands, layout: &mut GridLayout) {
    // Two compact boards fit in the same horizontal footprint as the normal 8×8 grid. The two
    // empty logical columns between them prevent matches/swaps crossing the teleport gap.
    const SUBGRID_W: i32 = 3;
    const SUBGRID_H: i32 = 5;
    const SUBGRID_Y: i32 = 1;
    const RIGHT_X: i32 = 5;
    *layout = GridLayout::rectangles(&[
        (0, SUBGRID_Y, SUBGRID_W, SUBGRID_H),
        (RIGHT_X, SUBGRID_Y, SUBGRID_W, SUBGRID_H),
    ]);
    for x in 0..SUBGRID_W {
        layout.add_fall_route(
            GridPos { x, y: SUBGRID_Y },
            GridPos {
                x: RIGHT_X + x,
                y: SUBGRID_Y + SUBGRID_H - 1,
            },
        );
    }
    // Only the terminal (right) subgrid consumes sparks. A future ingredient level can reuse
    // this layout without teaching the falling system where its last board happens to be.
    layout.set_spark_exits((0..SUBGRID_W).map(|x| GridPos {
        x: RIGHT_X + x,
        y: SUBGRID_Y,
    }));
    let mut rng = rand::rng();
    for offset_x in [0, RIGHT_X] {
        for (mut pos, color, kind) in generate_board(&mut rng, &HashSet::new(), 0.0, [1.0; 5])
            .into_iter()
            .filter(|(pos, _, _)| pos.x < SUBGRID_W && pos.y < SUBGRID_H)
        {
            pos.x += offset_x;
            pos.y += SUBGRID_Y;
            spawn_light(commands, pos, color, kind, to_world(pos));
        }
    }
}

/// Tears the match down when returning to the menu: despawns every match entity (lights+sparks and
/// their core/glow children, shadows, particles, traveling beams, blast effects, score shards,
/// overlays) and resets all pipeline resources, so a fresh mode starts clean.
pub(crate) fn teardown_match(
    mut commands: Commands,
    mode: Res<GameMode>,
    run: Res<RunState>,
    mut level: ResMut<LevelConfig>,
    mut res: ResetParams,
    match_entities: Query<Entity, TransientMatchEntity>,
) {
    for e in &match_entities {
        commands.entity(e).try_despawn();
    }
    reset_for_replay(make_level(1), &mut level, &mut res, &mode, &run);
    *res.level_timer = LevelTimer(None);
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
    mut reward: ResMut<LevelRewardOffer>,
    settings: Res<WindowSettings>,
) {
    let lang = settings.language;
    let title = if *mode == GameMode::ConsumeAll {
        lang.tr(TrKey::BoardConsumedTitle)
    } else {
        lang.tr(TrKey::LevelCompleteTitle)
    };

    let mut reward_offer = Vec::new();
    let details = if *mode == GameMode::ConsumeAll {
        lang.tr(TrKey::LightcoresCaptured)
            .replace("{}", &score.0.to_string())
    } else {
        let unlock = progress.record_score(level.level, score.0);
        if mode.is_run() {
            run.complete_depth(level.level);
            if level.level < RUN_LEVELS {
                reward_offer = run.reward_offer(level.level, 3);
            }
        }
        format!(
            "{}\n{}",
            lang.tr(TrKey::LightcoresCaptured)
                .replace("{}", &score.0.to_string()),
            level_complete_meta(&unlock, lang)
        )
    };
    reward.reset(reward_offer.clone());

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
                        width: Val::Px(if reward_offer.is_empty() {
                            360.0
                        } else {
                            430.0
                        }),
                        max_width: Val::Percent(90.0),
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
                                Text::new(lang.tr(TrKey::MatchSummary)),
                                TextFont {
                                    font_size: FontSize::Px(13.0),
                                    ..default()
                                },
                                TextColor(Color::srgb(0.65, 0.85, 1.0)),
                            ));
                            summary
                                .spawn((Node {
                                    flex_direction: FlexDirection::Row,
                                    flex_wrap: FlexWrap::Wrap,
                                    justify_content: JustifyContent::Center,
                                    column_gap: Val::Px(12.0),
                                    row_gap: Val::Px(4.0),
                                    ..default()
                                },))
                                .with_children(|grid| {
                                    if stats.reds > 0 {
                                        grid.spawn((
                                            Text::new(format!(
                                                "{}: {}",
                                                lang.tr(TrKey::StatsRed),
                                                stats.reds
                                            )),
                                            TextFont {
                                                font_size: FontSize::Px(12.0),
                                                ..default()
                                            },
                                            TextColor(Color::srgb(0.9, 0.9, 0.9)),
                                        ));
                                    }
                                    if stats.greens > 0 {
                                        grid.spawn((
                                            Text::new(format!(
                                                "{}: {}",
                                                lang.tr(TrKey::StatsGreen),
                                                stats.greens
                                            )),
                                            TextFont {
                                                font_size: FontSize::Px(12.0),
                                                ..default()
                                            },
                                            TextColor(Color::srgb(0.9, 0.9, 0.9)),
                                        ));
                                    }
                                    if stats.blues > 0 {
                                        grid.spawn((
                                            Text::new(format!(
                                                "{}: {}",
                                                lang.tr(TrKey::StatsBlue),
                                                stats.blues
                                            )),
                                            TextFont {
                                                font_size: FontSize::Px(12.0),
                                                ..default()
                                            },
                                            TextColor(Color::srgb(0.9, 0.9, 0.9)),
                                        ));
                                    }
                                    if stats.yellows > 0 {
                                        grid.spawn((
                                            Text::new(format!(
                                                "{}: {}",
                                                lang.tr(TrKey::StatsYellow),
                                                stats.yellows
                                            )),
                                            TextFont {
                                                font_size: FontSize::Px(12.0),
                                                ..default()
                                            },
                                            TextColor(Color::srgb(0.9, 0.9, 0.9)),
                                        ));
                                    }
                                    if stats.purples > 0 {
                                        grid.spawn((
                                            Text::new(format!(
                                                "{}: {}",
                                                lang.tr(TrKey::StatsPurple),
                                                stats.purples
                                            )),
                                            TextFont {
                                                font_size: FontSize::Px(12.0),
                                                ..default()
                                            },
                                            TextColor(Color::srgb(0.9, 0.9, 0.9)),
                                        ));
                                    }
                                    if stats.lightkinds > 0 {
                                        grid.spawn((
                                            Text::new(format!(
                                                "{}: {}",
                                                lang.tr(TrKey::StatsSparks),
                                                stats.lightkinds
                                            )),
                                            TextFont {
                                                font_size: FontSize::Px(12.0),
                                                ..default()
                                            },
                                            TextColor(Color::srgb(0.9, 0.9, 0.9)),
                                        ));
                                    }
                                    grid.spawn((
                                        Text::new(format!(
                                            "{}: {}",
                                            lang.tr(TrKey::StatsMaxCombo),
                                            stats.max_cascade
                                        )),
                                        TextFont {
                                            font_size: FontSize::Px(12.0),
                                            ..default()
                                        },
                                        TextColor(Color::srgb(0.9, 0.9, 0.9)),
                                    ));
                                    grid.spawn((
                                        Text::new(format!(
                                            "{}: {}",
                                            lang.tr(TrKey::StatsChains),
                                            stats.total_chains
                                        )),
                                        TextFont {
                                            font_size: FontSize::Px(12.0),
                                            ..default()
                                        },
                                        TextColor(Color::srgb(0.9, 0.9, 0.9)),
                                    ));
                                });
                        });

                    if !reward_offer.is_empty() {
                        card.spawn((
                            Text::new(lang.tr(TrKey::ChooseOneModifier)),
                            TextFont {
                                font_size: FontSize::Px(13.0),
                                ..default()
                            },
                            TextColor(Color::srgb(0.72, 0.88, 1.0)),
                        ));
                        card.spawn((Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(8.0),
                            ..default()
                        },))
                            .with_children(|list| {
                                for boon in reward_offer.iter().copied() {
                                    list.spawn((
                                        Button,
                                        LevelRewardButton(boon),
                                        crate::ui::get_item_tooltip(
                                            crate::gameplay::shop::ShopItem::Boon(boon),
                                            lang,
                                        ),
                                        Node {
                                            width: Val::Percent(100.0),
                                            min_height: Val::Px(50.0),
                                            justify_content: JustifyContent::SpaceBetween,
                                            align_items: AlignItems::Center,
                                            column_gap: Val::Px(12.0),
                                            padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                                            border: UiRect::all(Val::Px(1.5)),
                                            ..default()
                                        },
                                        BackgroundColor(Color::srgba(0.09, 0.13, 0.20, 0.94)),
                                        BorderColor::all(Color::srgba(0.50, 0.74, 1.0, 0.28)),
                                    ))
                                    .with_children(|button| {
                                        button.spawn((
                                            Text::new(format!("{}  {}", boon.notation(), boon.label(lang))),
                                            TextFont {
                                                font_size: FontSize::Px(16.0),
                                                ..default()
                                            },
                                            TextColor(Color::WHITE),
                                        ));
                                        button.spawn((
                                            Text::new(format!("{} · {}c", boon.status_label(lang), boon.cost(run.level(boon)))),
                                            TextFont {
                                                font_size: FontSize::Px(11.0),
                                                ..default()
                                            },
                                            TextColor(Color::srgb(0.64, 0.81, 0.98)),
                                        ));
                                    });
                                }
                            });
                    }

                    card.spawn((
                        LevelRewardInstructionText,
                        Text::new(lang.tr(TrKey::BoonContinueInstruction)),
                        TextFont {
                            font_size: FontSize::Px(12.0),
                            ..default()
                        },
                        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.45)),
                    ));
                });
        });
}

pub(crate) fn level_reward_button_system(
    mut reward: ResMut<LevelRewardOffer>,
    interactions: Query<(&Interaction, &LevelRewardButton), Changed<Interaction>>,
    mut buttons: Query<(&LevelRewardButton, &mut BackgroundColor, &mut BorderColor)>,
    mut instruction: Query<&mut Text, With<LevelRewardInstructionText>>,
    settings: Res<WindowSettings>,
    mut run: ResMut<RunState>,
    mut reserve: ResMut<CoreReserve>,
    mut spent: ResMut<CoresSpent>,
) {
    if !reward.active() {
        return;
    }

    let mut picked = None;
    for (interaction, button) in &interactions {
        if *interaction == Interaction::Pressed {
            picked = Some(button.0);
            break;
        }
    }

    let Some(boon) = picked else {
        return;
    };
    if !reward.offered.contains(&boon) {
        return;
    }

    if reward.purchased.contains(&boon) {
        return;
    }
    let Some(cost) = run.boon_cost(boon) else {
        return;
    };
    if reserve.0 < cost || !run.buy(boon) {
        return;
    }
    reserve.0 -= cost;
    spent.0 += cost;
    reward.purchased.push(boon);
    for (button, mut bg, mut border) in &mut buttons {
        if button.0 == boon {
            bg.0 = Color::srgba(0.08, 0.24, 0.14, 0.96);
            *border = BorderColor::all(Color::srgba(0.42, 1.0, 0.63, 0.88));
        }
    }
    for mut text in &mut instruction {
        text.0 = format!("{} · {}", settings.language.tr(TrKey::BoonPurchased), settings.language.tr(TrKey::BoonContinueInstruction));
    }
}

/// Resetea todos los recursos de partida para el nivel dado. Compartido por
/// `handle_level_advance` y `handle_restart` — evita que los dos bloques diverjan.
fn reset_for_replay(
    replay_level: LevelConfig,
    level: &mut LevelConfig,
    res: &mut ResetParams,
    mode: &GameMode,
    run: &RunState,
) {
    *level = replay_level;
    *res.level_timer = level_timer_for(&level.goal);
    res.score.0 = 0;
    res.displayed.0 = 0;
    // Same rule as `teardown_match`: the run's booster wallet only survives a retry while the run
    // is still active — retrying a level with lives left must NOT wipe it (previously did,
    // unconditionally, contradicting both that invariant and the "puedes reintentar" UI copy in
    // `show_game_over`, which never warns about losing the reserve on a plain retry).
    if !(mode.is_run() && run.active) {
        res.reserve.0 = 0;
        res.spent.0 = 0;
    }
    res.moves.0 = level.total_moves;
    res.pending.0 = None;
    res.reverting.0.clear();
    *res.drag = DragState::default();
    res.settled.0 = false;
    res.cascade.0 = 0;
    res.collected.0 = 0;
    res.shadow.0 = 0;
    res.queue.0.clear();
    res.super_combo.0.clear();
    res.collected_cores.0 = [0; 5];
    res.displayed_cores.0 = [0; 5];
    *res.stats = StatsBook::default();
    res.popup_open.0 = false;
    res.special_moves.clear();
}

fn level_complete_meta(
    unlock: &crate::core::campaign::CampaignUnlockResult,
    lang: Language,
) -> String {
    if let Some(next_level) = unlock.unlocked_next {
        lang.tr(TrKey::LevelUnlocked)
            .replace("{:02}", &format!("{:02}", next_level))
    } else if unlock.new_best {
        lang.tr(TrKey::NewHighScore).to_string()
    } else {
        lang.tr(TrKey::LevelAlreadyCompleted).to_string()
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
    mut run: ResMut<RunState>,
    score: Res<Score>,
    reserve: Res<CoreReserve>,
    spent: Res<CoresSpent>,
    moves: Res<MovesLeft>,
    level: Res<LevelConfig>,
    sparks: Res<crate::gameplay::SparksCollected>,
    shadow_count: Res<ShadowCount>,
    collected_cores: Res<CollectedCores>,
) {
    let can_retry = mode.is_run() && run.active && run.lives > 0;
    let ends_run = mode.is_run() && run.active && run.lives == 0;
    if ends_run {
        run.active = false;
    }

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
    let title = if ends_run {
        "Run Terminado"
    } else if can_retry {
        "Nivel Fallido"
    } else {
        "Fin de Partida"
    };
    let details = if ends_run {
        format!(
            "Lightcores capturados: {}\n{}\n¡Te quedaste sin vidas!\nSe perdieron los boons y el reserve acumulados.",
            score.0, outcome
        )
    } else if can_retry {
        format!(
            "Lightcores capturados: {}\n{}\nTe quedan {} vidas de reserva.\nPuedes reintentar este nivel.",
            score.0, outcome, run.lives
        )
    } else {
        format!("Lightcores capturados: {}\n{}", score.0, outcome)
    };
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
                        max_width: Val::Percent(90.0),
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
                        Text::new(title),
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
                        Text::new(if ends_run {
                            "[Click/Tap o Espacio] para volver al mapa"
                        } else if can_retry {
                            "[Click/Tap o Espacio] para reintentar (Cuesta 1 vida)"
                        } else {
                            "[Click/Tap o Espacio] para reiniciar"
                        }),
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
    mut run: ResMut<RunState>,
    mut level: ResMut<LevelConfig>,
    mut res: ResetParams,
    mut next_state: ResMut<NextState<GameState>>,
    cache: Res<VisualCache>,
    layout: Res<GridLayout>,
    match_entities: Query<Entity, TransientMatchEntity>,
) {
    if !actions.confirm {
        return;
    }

    if mode.is_sandbox() {
        next_state.set(GameState::LevelMenu);
        return;
    }
    if mode.is_run() {
        if run.lives > 0 {
            run.lives -= 1;
        } else {
            next_state.set(GameState::LevelMenu);
            return;
        }
    }

    for e in &match_entities {
        commands.entity(e).try_despawn();
    }

    let replay_level = match *mode {
        GameMode::Classic(level_num) => make_level(level_num),
        GameMode::Run(depth) => make_generated_level(depth, run.seed),
        GameMode::ConsumeAll | GameMode::Sandbox | GameMode::Debug(_) | GameMode::TeleportTest => make_level(level.level),
    };
    reset_for_replay(replay_level, &mut level, &mut res, &mode, &run);
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
    populate_board(
        &mut commands,
        &cache,
        &level,
        &mut res.shadow.0,
        hollow_chance,
        weights,
        &layout,
    );
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
        LevelGoal::TimedCollectColor { color, target, .. } => {
            let current = collected_cores.0[color.index()];
            let color_name = match color {
                LightColor::Red => "rojos",
                LightColor::Green => "verdes",
                LightColor::Blue => "azules",
                LightColor::Yellow => "amarillos",
                LightColor::Purple => "morados",
            };
            format!(
                "Faltaron {} cores {} antes del reloj",
                target.saturating_sub(current),
                color_name
            )
        }
    }
}
