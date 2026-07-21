//! Match-result presentation. Gameplay finalizes domain state; this adapter owns every UI entity,
//! localized string and interaction shown for that result.

use bevy::prelude::*;

use super::{HudIcons, get_item_tooltip};
use crate::core::campaign::CampaignUnlockResult;
use crate::core::locale::{Language, TrKey};
use crate::core::prelude::*;
use crate::core::run::{BoonKind, RunState};
use crate::gameplay::lifecycle::{
    LevelCompletionReport, LevelRewardOffer, LevelRewardPurchaseRequested,
};
use crate::gameplay::shop::{ShopItem, ShopPurchaseRequested};
use crate::gameplay::{
    self, CollectedCores, CoreReserve, GameMode, Score, ShadowCount, SparksCollected, StatsBook,
};
use crate::input::InputActions;
use crate::settings::UserSettings;
use crate::state::{AttemptScoped, MatchPhase, MatchScoped};

pub(super) struct MatchResultUiPlugin;

impl Plugin for MatchResultUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(MatchPhase::LevelComplete),
            spawn_level_complete.after(gameplay::lifecycle::finalize_level_complete),
        )
        .add_systems(
            OnEnter(MatchPhase::GameOver),
            spawn_game_over.after(gameplay::lifecycle::finalize_game_over),
        )
        .add_systems(
            Update,
            (
                emit_reward_purchase_requests,
                refresh_reward_purchase_feedback.run_if(resource_changed::<LevelRewardOffer>),
            )
                .run_if(in_state(MatchPhase::LevelComplete)),
        )
        .add_systems(
            Update,
            (
                emit_game_over_life_purchase.before(gameplay::lifecycle::handle_restart),
                refresh_game_over_life_offer
                    .run_if(resource_changed::<RunState>.or_else(resource_changed::<CoreReserve>)),
            )
                .run_if(in_state(MatchPhase::GameOver)),
        );
    }
}

#[derive(Component)]
struct MatchResultOverlay;

#[derive(Component, Clone, Copy)]
struct LevelRewardButton(BoonKind);

#[derive(Component)]
struct LevelRewardInstructionText;

#[derive(Component)]
struct GameOverTitleText;

#[derive(Component)]
struct GameOverContinueHintText;

#[derive(Component)]
struct GameOverLifeButton;

#[derive(Component)]
struct GameOverLifeButtonText;

fn overlay_card(
    parent: &mut ChildSpawnerCommands,
    border: Color,
    width: f32,
    build: impl FnOnce(&mut ChildSpawnerCommands),
) {
    parent
        .spawn((
            Node {
                width: Val::Px(width),
                max_width: Val::Percent(90.0),
                max_height: Val::Percent(92.0),
                overflow: Overflow::scroll_y(),
                padding: UiRect::all(Val::Px(24.0)),
                border: UiRect::all(Val::Px(2.0)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(14.0),
                ..default()
            },
            BorderColor::all(border),
            BackgroundColor(Color::srgba(0.07, 0.07, 0.14, 0.97)),
        ))
        .with_children(build);
}

fn spawn_overlay(commands: &mut Commands, build: impl FnOnce(&mut ChildSpawnerCommands)) {
    commands
        .spawn((
            MatchResultOverlay,
            AttemptScoped,
            MatchScoped,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.68)),
        ))
        .with_children(build);
}

fn result_title(parent: &mut ChildSpawnerCommands, value: impl Into<String>, color: Color) {
    parent.spawn((
        Text::new(value),
        TextFont {
            font_size: FontSize::Px(26.0),
            ..default()
        },
        TextColor(color),
        TextLayout::no_wrap(),
    ));
}

fn result_body(parent: &mut ChildSpawnerCommands, value: impl Into<String>, color: Color) {
    parent.spawn((
        Text::new(value),
        TextFont {
            font_size: FontSize::Px(14.0),
            ..default()
        },
        TextColor(color),
        TextLayout::justify(Justify::Center),
    ));
}

fn level_complete_meta(unlock: Option<CampaignUnlockResult>, lang: Language) -> String {
    let Some(unlock) = unlock else {
        return String::new();
    };
    if let Some(next_level) = unlock.unlocked_next {
        lang.tr(TrKey::LevelUnlocked)
            .replace("{:02}", &format!("{next_level:02}"))
    } else if unlock.new_best {
        lang.tr(TrKey::NewHighScore).to_string()
    } else {
        lang.tr(TrKey::LevelAlreadyCompleted).to_string()
    }
}

fn stats_summary(stats: &StatsBook, lang: Language) -> String {
    format!(
        "{}\n{}: {} · {}: {} · {}: {}\n{}: {} · {}: {}\n{}: {} · {}: {}x · {}: {}",
        lang.tr(TrKey::MatchSummary),
        lang.tr(TrKey::StatsRed),
        stats.reds,
        lang.tr(TrKey::StatsGreen),
        stats.greens,
        lang.tr(TrKey::StatsBlue),
        stats.blues,
        lang.tr(TrKey::StatsYellow),
        stats.yellows,
        lang.tr(TrKey::StatsPurple),
        stats.purples,
        lang.tr(TrKey::StatsSpecials),
        stats.lightkinds,
        lang.tr(TrKey::StatsMaxCombo),
        stats.max_cascade,
        lang.tr(TrKey::StatsChains),
        stats.total_chains,
    )
}

fn spawn_level_complete(
    mut commands: Commands,
    mode: Res<GameMode>,
    score: Res<Score>,
    stats: Res<StatsBook>,
    run: Res<RunState>,
    reward: Res<LevelRewardOffer>,
    report: Res<LevelCompletionReport>,
    settings: Res<UserSettings>,
    icons: Res<HudIcons>,
) {
    let lang = settings.language;
    let title = if *mode == GameMode::ConsumeAll {
        lang.tr(TrKey::BoardConsumedTitle)
    } else {
        lang.tr(TrKey::LevelCompleteTitle)
    };
    let details = format!(
        "{}\n{}",
        lang.tr(TrKey::LightcoresCaptured)
            .replace("{}", &score.0.to_string()),
        level_complete_meta(report.unlock, lang)
    );
    spawn_overlay(&mut commands, |overlay| {
        overlay_card(
            overlay,
            Color::srgba(0.65, 0.85, 1.0, 0.35),
            if reward.active() { 430.0 } else { 360.0 },
            |card| {
                result_title(card, title, Color::srgb(1.0, 0.85, 0.2));
                result_body(card, details, Color::srgb(0.8, 0.9, 1.0));
                result_body(
                    card,
                    stats_summary(&stats, lang),
                    Color::srgb(0.78, 0.86, 0.94),
                );
                if reward.active() {
                    result_body(
                        card,
                        lang.tr(TrKey::ChooseOneModifier),
                        Color::srgb(0.72, 0.88, 1.0),
                    );
                    for boon in reward.offered.iter().copied() {
                        card.spawn((
                            Button,
                            LevelRewardButton(boon),
                            get_item_tooltip(ShopItem::Boon(boon), lang),
                            Node {
                                width: Val::Percent(100.0),
                                min_height: Val::Px(48.0),
                                justify_content: JustifyContent::SpaceBetween,
                                align_items: AlignItems::Center,
                                padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                                border: UiRect::all(Val::Px(1.5)),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.09, 0.13, 0.20, 0.94)),
                            BorderColor::all(Color::srgba(0.50, 0.74, 1.0, 0.28)),
                        ))
                        .with_children(|button| {
                            button
                                .spawn(Node {
                                    align_items: AlignItems::Center,
                                    column_gap: Val::Px(8.0),
                                    ..default()
                                })
                                .with_children(|label| {
                                    label.spawn((
                                        ImageNode {
                                            image: icons.boons[boon.index()].clone(),
                                            ..default()
                                        },
                                        Node {
                                            width: Val::Px(26.0),
                                            height: Val::Px(26.0),
                                            ..default()
                                        },
                                    ));
                                    label.spawn((
                                        Text::new(boon.label(lang)),
                                        TextFont {
                                            font_size: FontSize::Px(15.0),
                                            ..default()
                                        },
                                        TextColor(Color::WHITE),
                                    ));
                                });
                            button.spawn((
                                Text::new(format!(
                                    "{} · {}c",
                                    boon.status_label(lang),
                                    boon.cost(run.level(boon))
                                )),
                                TextFont {
                                    font_size: FontSize::Px(11.0),
                                    ..default()
                                },
                                TextColor(Color::srgb(0.64, 0.81, 0.98)),
                            ));
                        });
                    }
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
            },
        );
    });
}

fn goal_outcome(
    level: &LevelConfig,
    score: u32,
    sparks: u32,
    shadows: u32,
    cores: [u32; 5],
    lang: Language,
) -> String {
    let status = level.goal_status(GoalFacts {
        score,
        sparks,
        shadows,
        collected_cores: cores,
        ..default()
    });
    match status.kind {
        GoalKind::ClearShadow => {
            format!("{}: {}", lang.tr(TrKey::GoalClearShadows), status.current)
        }
        GoalKind::Sparks => format!(
            "{}: {}",
            lang.tr(TrKey::GoalRescueSparks),
            status
                .target
                .unwrap_or_default()
                .saturating_sub(status.current)
        ),
        GoalKind::CollectColor | GoalKind::TimedCollectColor => format!(
            "{}: {}",
            lang.tr(TrKey::GoalCollectColor),
            status
                .target
                .unwrap_or_default()
                .saturating_sub(status.current)
        ),
        GoalKind::Score | GoalKind::TimedScore => format!(
            "{}: {}",
            lang.tr(TrKey::GoalReachTarget),
            status
                .target
                .unwrap_or_default()
                .saturating_sub(status.current)
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_game_over(
    mut commands: Commands,
    mode: Res<GameMode>,
    run: Res<RunState>,
    reserve: Res<CoreReserve>,
    score: Res<Score>,
    level: Res<LevelConfig>,
    sparks: Res<SparksCollected>,
    shadows: Res<ShadowCount>,
    cores: Res<CollectedCores>,
    settings: Res<UserSettings>,
) {
    let lang = settings.language;
    let can_retry = mode.is_run() && run.active && run.lives > 0;
    let ends_run = mode.is_run() && run.active && run.lives == 0;
    let life_cost = ShopItem::Life.cost(&run).unwrap_or_default();
    let title = game_over_title(lang, ends_run, can_retry);
    let continue_hint = game_over_continue_hint(lang, can_retry, ends_run, reserve.0 >= life_cost);
    let details = format!(
        "{}\n{}",
        lang.tr(TrKey::LightcoresCaptured)
            .replace("{}", &score.0.to_string()),
        goal_outcome(&level, score.0, sparks.0, shadows.0, cores.0, lang)
    );
    spawn_overlay(&mut commands, |overlay| {
        overlay_card(
            overlay,
            Color::srgba(1.0, 0.45, 0.45, 0.35),
            360.0,
            |card| {
                card.spawn((
                    GameOverTitleText,
                    Text::new(title),
                    TextFont {
                        font_size: FontSize::Px(26.0),
                        ..default()
                    },
                    TextColor(Color::srgb(1.0, 0.45, 0.45)),
                    TextLayout::no_wrap(),
                ));
                result_body(card, details, Color::srgb(1.0, 0.85, 0.85));
                if ends_run {
                    spawn_game_over_life_button(card, lang, reserve.0, life_cost);
                }
                card.spawn((
                    GameOverContinueHintText,
                    Text::new(continue_hint),
                    TextFont {
                        font_size: FontSize::Px(14.0),
                        ..default()
                    },
                    TextColor(Color::srgba(1.0, 1.0, 1.0, 0.45)),
                    TextLayout::justify(Justify::Center),
                ));
            },
        );
    });
}

/// Shared Game Over headline — spawned once and refreshed live, so both paths (`spawn_game_over`
/// and `refresh_game_over_life_offer`) stay in lockstep instead of carrying two copies that drift.
fn game_over_title(lang: Language, ends_run: bool, can_retry: bool) -> &'static str {
    match (lang, ends_run, can_retry) {
        (Language::English, true, _) => "Out of lives",
        (Language::English, _, true) => "Level failed",
        (Language::English, _, _) => "Game over",
        (_, true, _) => "Sin vidas",
        (_, _, true) => "Nivel fallido",
        _ => "Fin de partida",
    }
}

fn game_over_continue_hint(
    lang: Language,
    can_retry: bool,
    ends_run: bool,
    can_buy_life: bool,
) -> &'static str {
    match (lang, can_retry, ends_run, can_buy_life) {
        (Language::English, true, _, _) => "[Click/Tap or Space] to retry",
        (Language::English, false, true, true) => {
            "Buy a life to retry · click outside to end the run"
        }
        (Language::English, false, true, false) => {
            "Not enough reserve · click outside to end the run"
        }
        (Language::English, false, false, _) => "[Click/Tap or Space] to return",
        (_, true, _, _) => "[Click/Tap o Espacio] para reintentar",
        (_, false, true, true) => {
            "Compra una vida para reintentar · pulsa fuera para terminar el run"
        }
        (_, false, true, false) => "Reserva insuficiente · pulsa fuera para terminar el run",
        (_, false, false, _) => "[Click/Tap o Espacio] para volver",
    }
}

fn game_over_life_label(lang: Language, reserve: u32, cost: u32) -> String {
    if reserve >= cost {
        if lang == Language::English {
            format!("Buy +1 Life · {cost}c")
        } else {
            format!("Comprar +1 Vida · {cost}c")
        }
    } else {
        let missing = cost.saturating_sub(reserve);
        if lang == Language::English {
            format!("+1 Life · {cost}c · need {missing}")
        } else {
            format!("+1 Vida · {cost}c · faltan {missing}")
        }
    }
}

fn spawn_game_over_life_button(
    card: &mut ChildSpawnerCommands,
    lang: Language,
    reserve: u32,
    cost: u32,
) {
    let affordable = reserve >= cost;
    card.spawn((
        Button,
        GameOverLifeButton,
        Node {
            width: Val::Percent(100.0),
            min_height: Val::Px(48.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
            border: UiRect::all(Val::Px(1.5)),
            ..default()
        },
        BackgroundColor(if affordable {
            Color::srgba(0.16, 0.24, 0.14, 0.96)
        } else {
            Color::srgba(0.13, 0.13, 0.16, 0.88)
        }),
        BorderColor::all(if affordable {
            Color::srgba(0.52, 1.0, 0.58, 0.82)
        } else {
            Color::srgba(0.42, 0.42, 0.48, 0.42)
        }),
    ))
    .with_children(|button| {
        button.spawn((
            GameOverLifeButtonText,
            Text::new(game_over_life_label(lang, reserve, cost)),
            TextFont {
                font_size: FontSize::Px(15.0),
                ..default()
            },
            TextColor(if affordable {
                Color::srgb(0.72, 1.0, 0.76)
            } else {
                Color::srgb(0.58, 0.58, 0.64)
            }),
        ));
    });
}

/// Consumes the same pointer press that activated the button, preventing the generic result-card
/// click handler from interpreting a life purchase as "end run" in the same frame.
fn emit_game_over_life_purchase(
    interactions: Query<&Interaction, (Changed<Interaction>, With<GameOverLifeButton>)>,
    reserve: Res<CoreReserve>,
    run: Res<RunState>,
    mut actions: ResMut<InputActions>,
    mut commands: Commands,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        actions.confirm = false;
        let Some(cost) = ShopItem::Life.cost(&run) else {
            continue;
        };
        if run.lives == 0 && reserve.0 >= cost {
            commands.trigger(ShopPurchaseRequested(ShopItem::Life));
        }
    }
}

fn refresh_game_over_life_offer(
    run: Res<RunState>,
    reserve: Res<CoreReserve>,
    settings: Res<UserSettings>,
    mut titles: Query<&mut Text, With<GameOverTitleText>>,
    mut hints: Query<&mut Text, (With<GameOverContinueHintText>, Without<GameOverTitleText>)>,
    mut buttons: Query<&mut Node, With<GameOverLifeButton>>,
    mut labels: Query<
        (&mut Text, &mut TextColor),
        (
            With<GameOverLifeButtonText>,
            Without<GameOverTitleText>,
            Without<GameOverContinueHintText>,
        ),
    >,
) {
    let lang = settings.language;
    let can_retry = run.active && run.lives > 0;
    let ends_run = run.active && run.lives == 0;
    let cost = ShopItem::Life.cost(&run).unwrap_or_default();
    let affordable = reserve.0 >= cost;

    for mut title in &mut titles {
        title.0 = game_over_title(lang, ends_run, can_retry).to_string();
    }
    for mut hint in &mut hints {
        hint.0 = game_over_continue_hint(lang, can_retry, ends_run, affordable).to_string();
    }
    for mut node in &mut buttons {
        node.display = if ends_run {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (mut text, mut color) in &mut labels {
        text.0 = game_over_life_label(lang, reserve.0, cost);
        color.0 = if affordable {
            Color::srgb(0.72, 1.0, 0.76)
        } else {
            Color::srgb(0.58, 0.58, 0.64)
        };
    }
}

fn emit_reward_purchase_requests(
    interactions: Query<(&Interaction, &LevelRewardButton), Changed<Interaction>>,
    mut commands: Commands,
) {
    for (interaction, button) in &interactions {
        if *interaction == Interaction::Pressed {
            commands.trigger(LevelRewardPurchaseRequested(button.0));
        }
    }
}

fn refresh_reward_purchase_feedback(
    reward: Res<LevelRewardOffer>,
    mut buttons: Query<(&LevelRewardButton, &mut BackgroundColor, &mut BorderColor)>,
    mut instruction: Query<&mut Text, With<LevelRewardInstructionText>>,
    settings: Res<UserSettings>,
) {
    for (button, mut background, mut border) in &mut buttons {
        if reward.purchased.contains(&button.0) {
            background.0 = Color::srgba(0.08, 0.24, 0.14, 0.96);
            *border = BorderColor::all(Color::srgba(0.42, 1.0, 0.63, 0.88));
        }
    }
    if !reward.purchased.is_empty() {
        for mut text in &mut instruction {
            text.0 = format!(
                "{} · {}",
                settings.language.tr(TrKey::BoonPurchased),
                settings.language.tr(TrKey::BoonContinueInstruction)
            );
        }
    }
}
