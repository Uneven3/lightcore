use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::sprite::Anchor;

use crate::core::locale::{Language, TrKey};
use crate::core::prelude::*;
use crate::core::run::{BoonKind, RunState};
use crate::embedded;
use crate::gameplay::shop::{
    BTN_BORDER_ARMED, BTN_BORDER_BROKE, BTN_BORDER_IDLE, BTN_IDLE, Shop, ShopBar, ShopButton,
    ShopCard, ShopItem,
};
use crate::gameplay::{
    CoreReserve, DisplayedCollectedCores, DisplayedScore, GameMode, LevelTimer, MovesLeft,
    ScoreAnchor, ScoreGlow, ShadowCount, SparksCollected, StatsBook, StatsPopupOpen,
};
use crate::menu::options::WindowSettings;
use crate::state::GameState;
use crate::visuals::assets::VisualCache;
use crate::visuals::render_target::{FinalCamera, WorldCamera, window_point_to_world};

const SCORE_NEON_BASE: f32 = 1.7;
const SCORE_NEON_PULSE: f32 = 2.6;
const SCORE_PULSE_FREQ: f32 = 22.0;
const SCORE_PULSE_DECAY: f32 = 2.2;

pub(crate) struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GoalHintTouchTimer>()
            .init_resource::<TutorialState>()
            .init_resource::<LevelTutorialShown>()
            .add_systems(Startup, (setup_ui, setup_watermark))
            // The HUD is only meaningful during a match — hide it on every menu screen (the app
            // boots straight into `MainMenu`, so this also covers first launch) and bring it back
            // the moment a mode starts loading, rather than tying it to one specific state's
            // `OnExit` (which would also fire on LevelMenu → MainMenu's "Volver").
            .add_systems(OnEnter(GameState::MainMenu), hide_hud)
            .add_systems(OnEnter(GameState::LevelMenu), hide_hud)
            .add_systems(OnEnter(GameState::Options), hide_hud)
            .add_systems(
                OnEnter(GameState::Loading),
                (show_hud, reset_level_tutorial_shown),
            )
            // The match stays alive while paused — keep the HUD up; this also restores it when
            // returning from Options (which hid it) back to the pause overlay.
            .add_systems(OnEnter(GameState::Paused), show_hud)
            .add_systems(OnEnter(GameState::Playing), check_show_tutorial_on_start)
            .add_systems(OnExit(GameState::Playing), reset_tutorial_state)
            .add_systems(
                Update,
                (
                    update_score_text.run_if(resource_changed::<DisplayedScore>),
                    update_score_glow,
                    position_score,
                    update_moves_text.run_if(
                        resource_changed::<MovesLeft>
                            .or_else(resource_changed::<LevelConfig>)
                            .or_else(resource_changed::<GameMode>),
                    ),
                    update_goal_text.run_if(
                        resource_changed::<DisplayedScore>
                            .or_else(resource_changed::<SparksCollected>)
                            .or_else(resource_changed::<ShadowCount>)
                            .or_else(resource_changed::<LevelTimer>)
                            .or_else(resource_changed::<DisplayedCollectedCores>)
                            .or_else(resource_changed::<LevelConfig>)
                            .or_else(resource_changed::<GameMode>),
                    ),
                    pause_button_system,
                    shop_toggle_system,
                    update_shop_toggle_button.run_if(resource_changed::<Shop>),
                    update_shop_reserve_text.run_if(resource_changed::<CoreReserve>),
                    update_shop_bar_visibility.run_if(resource_changed::<Shop>),
                    update_shop_button_texts.run_if(
                        resource_changed::<CoreReserve>
                            .or_else(resource_changed::<RunState>)
                            .or_else(resource_changed::<Shop>)
                            .or_else(resource_changed::<WindowSettings>),
                    ),
                    update_shop_active_badge.run_if(
                        resource_changed::<Shop>.or_else(resource_changed::<WindowSettings>),
                    ),
                    update_slow_mo_badge,
                    update_watermark_fps,
                    update_static_hud_labels.run_if(resource_changed::<WindowSettings>),
                ),
            )
            .add_systems(
                Update,
                (
                    update_goal_hint,
                    stats_button_system,
                    update_stats_popup.run_if(
                        resource_changed::<StatsPopupOpen>
                            .or_else(resource_changed::<StatsBook>)
                            .or_else(resource_changed::<WindowSettings>),
                    ),
                    update_boon_indicators.run_if(
                        resource_changed::<RunState>.or_else(resource_changed::<WindowSettings>),
                    ),
                    tutorial_close_button_system,
                    tutorial_overlay_toggle_system,
                    update_tutorial_overlay_toggle_text.run_if(resource_changed::<WindowSettings>),
                    update_tutorial_visibility.run_if(resource_changed::<TutorialState>),
                    update_lives_text
                        .run_if(resource_changed::<RunState>.or_else(resource_changed::<GameMode>)),
                    update_tooltip_system,
                ),
            );
    }
}

#[derive(Component)]
pub(crate) struct ScoreText;
#[derive(Component)]
pub(crate) struct MovesText;
#[derive(Component)]
pub(crate) struct MovesNumberText;
#[derive(Component)]
pub(crate) struct GoalText;
#[derive(Component)]
pub(crate) struct GoalIcon;
/// The goal icon's actual visual: a tinted swatch of the real in-game asset the player needs to
/// consume for the current goal (a core, an ingredient, a jelly tile...), not a text glyph — some
/// glyphs (e.g. `▧`) render as an empty tofu box on fonts missing that codepoint.
#[derive(Component)]
pub(crate) struct GoalIconImage;
#[derive(Component)]
pub(crate) struct GoalPrimaryText;
#[derive(Component)]
pub(crate) struct GoalTargetText;
#[derive(Component)]
pub(crate) struct GoalHintContainer;
#[derive(Component)]
pub(crate) struct GoalHintText;
#[derive(Component)]
pub(crate) struct PauseButton;
#[derive(Component)]
pub(crate) struct ShopToggleButton;
#[derive(Component)]
pub(crate) struct StatsButton;
#[derive(Component)]
pub(crate) struct StatsPopupContainer;
#[derive(Component)]
pub(crate) struct StatsPopupText;
#[derive(Component)]
pub(crate) struct ShopReserveText;
#[derive(Component)]
pub(crate) struct ShopActiveBadge;
#[derive(Component)]
pub(crate) struct ShopActiveBadgeText;
#[derive(Component)]
pub(crate) struct SlowMoBadge;
#[derive(Component)]
pub(crate) struct SlowMoBadgeText;
#[derive(Component)]
pub(crate) struct ShopButtonStatusText(pub(crate) ShopItem);
#[derive(Component)]
pub(crate) struct ShopButtonCostText(pub(crate) ShopItem);

// Marker components for static labels that must be updated when the language changes.
#[derive(Component)]
struct MovesUnitLabel;
#[derive(Component)]
struct LivesUnitLabel;
#[derive(Component)]
struct ShopHeaderLabel;
#[derive(Component)]
struct ShopCoresLabel;
#[derive(Component)]
struct ShopModifiersLabel;
#[derive(Component)]
struct TutorialCloseBtnLabel;
#[derive(Component)]
struct FpsWatermarkText;

#[derive(Resource, Default)]
struct GoalHintTouchTimer(Option<Timer>);

#[derive(Default)]
struct FpsWatermarkState {
    visible: bool,
    last_fps: Option<u32>,
    elapsed_since_refresh: f32,
}

#[derive(Default)]
struct SlowMoBadgeState {
    active: bool,
    last_tenths: i32,
}

fn setup_ui(mut commands: Commands, cache: Res<VisualCache>, settings: Res<WindowSettings>) {
    // ScoreText is in world space (Text2d), so keep it independent of Bevy UI HudRoot.
    commands.spawn((
        ScoreText,
        Text2d::new("0"),
        TextFont {
            font_size: FontSize::Px(34.0),
            ..default()
        },
        TextColor(Color::srgb(0.65, 0.85, 1.0)),
        Anchor::CENTER,
        Transform::default(),
        Visibility::Hidden,
    ));

    // Fullscreen transparent container that centers HudRoot horizontally
    let mut screen = commands.spawn((Node {
        position_type: PositionType::Absolute,
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::FlexStart,
        ..default()
    },));

    let mut hud_root_id = Entity::PLACEHOLDER;
    screen.with_children(|screen_parent| {
        hud_root_id = screen_parent
            .spawn((
                HudRoot,
                Node {
                    width: Val::Px(600.0),
                    max_width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                Visibility::Hidden,
            ))
            .with_children(|hud| {
                // StatsButton
                hud.spawn((
                    Button,
                    StatsButton,
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(12.0),
                        left: Val::Percent(50.0),
                        width: Val::Px(120.0),
                        height: Val::Px(44.0),
                        margin: UiRect::left(Val::Px(-60.0)),
                        ..default()
                    },
                    Visibility::Hidden,
                ));

                // StatsPopupContainer
                hud.spawn((
                    StatsPopupContainer,
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(80.0),
                        left: Val::Percent(50.0),
                        width: Val::Px(240.0),
                        margin: UiRect::left(Val::Px(-120.0)),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        padding: UiRect::all(Val::Px(12.0)),
                        border: UiRect::all(Val::Px(1.5)),
                        display: Display::None,
                        ..default()
                    },
                    BorderColor::all(Color::srgba(0.65, 0.85, 1.0, 0.35)),
                    BackgroundColor(Color::srgba(0.08, 0.08, 0.15, 0.95)),
                    Visibility::Hidden,
                ))
                .with_children(|parent| {
                    parent.spawn((
                        StatsPopupText,
                        Text::new(""),
                        TextFont {
                            font_size: FontSize::Px(14.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.65, 0.85, 1.0)),
                    ));
                });

                // PauseButton
                hud.spawn((
                    Button,
                    PauseButton,
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(12.0),
                        left: Val::Px(12.0),
                        width: Val::Px(36.0),
                        height: Val::Px(36.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(1.5)),
                        ..default()
                    },
                    BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.2)),
                    BackgroundColor(Color::srgba(0.1, 0.1, 0.18, 0.7)),
                    Visibility::Hidden,
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new("||"),
                        TextFont {
                            font_size: FontSize::Px(15.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });

                // MovesText
                hud.spawn((
                    MovesText,
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(12.0),
                        right: Val::Px(12.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                        border: UiRect::all(Val::Px(1.5)),
                        ..default()
                    },
                    BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.2)),
                    BackgroundColor(Color::srgba(0.1, 0.1, 0.18, 0.7)),
                    Visibility::Hidden,
                ))
                .with_children(|m| {
                    m.spawn((
                        MovesNumberText,
                        Text::new("30"),
                        TextFont {
                            font_size: FontSize::Px(18.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                    m.spawn((
                        MovesUnitLabel,
                        Text::new("moves"),
                        TextFont {
                            font_size: FontSize::Px(9.0),
                            ..default()
                        },
                        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.5)),
                    ));
                });

                // LivesText
                hud.spawn((
                    LivesText,
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(72.0),
                        right: Val::Px(12.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                        border: UiRect::all(Val::Px(1.5)),
                        ..default()
                    },
                    BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.2)),
                    BackgroundColor(Color::srgba(0.12, 0.05, 0.05, 0.7)),
                    Visibility::Hidden,
                ))
                .with_children(|l| {
                    l.spawn((
                        LivesNumberText,
                        Text::new("2"),
                        TextFont {
                            font_size: FontSize::Px(18.0),
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.45, 0.45)),
                    ));
                    l.spawn((
                        LivesUnitLabel,
                        Text::new("vidas"),
                        TextFont {
                            font_size: FontSize::Px(9.0),
                            ..default()
                        },
                        TextColor(Color::srgba(1.0, 0.6, 0.6, 0.5)),
                    ));
                });

                hud.spawn((
                    SlowMoBadge,
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(132.0),
                        right: Val::Px(12.0),
                        padding: UiRect::axes(Val::Px(9.0), Val::Px(5.0)),
                        border: UiRect::all(Val::Px(1.5)),
                        display: Display::None,
                        ..default()
                    },
                    BorderColor::all(Color::srgba(1.0, 0.86, 0.46, 0.6)),
                    BackgroundColor(Color::srgba(0.20, 0.13, 0.03, 0.82)),
                    Visibility::Hidden,
                ))
                .with_children(|badge| {
                    badge.spawn((
                        SlowMoBadgeText,
                        Text::new("SLOW"),
                        TextFont {
                            font_size: FontSize::Px(12.0),
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.86, 0.46)),
                    ));
                });

                // BoonIndicatorBar
                hud.spawn((
                    BoonIndicatorBar,
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(56.0),
                        left: Val::Px(12.0),
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(6.0),
                        ..default()
                    },
                    Visibility::Hidden,
                ));

                // GoalText
                hud.spawn((
                    Button,
                    GoalText,
                    Node {
                        position_type: PositionType::Absolute,
                        bottom: Val::Px(16.0),
                        left: Val::Px(12.0),
                        min_width: Val::Px(96.0),
                        min_height: Val::Px(40.0),
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(8.0),
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                        border: UiRect::all(Val::Px(1.5)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BorderColor::all(Color::srgba(0.8, 1.0, 0.8, 0.2)),
                    BackgroundColor(Color::srgba(0.08, 0.15, 0.08, 0.7)),
                    Visibility::Hidden,
                ))
                .with_children(|goal| {
                    goal.spawn((
                        GoalIcon,
                        Node {
                            width: Val::Px(24.0),
                            height: Val::Px(24.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                    ))
                    .with_children(|icon| {
                        icon.spawn((
                            GoalIconImage,
                            ImageNode {
                                image: cache.core_image.clone(),
                                color: Color::srgb(0.8, 1.0, 0.8),
                                ..default()
                            },
                            Node {
                                width: Val::Px(22.0),
                                height: Val::Px(22.0),
                                ..default()
                            },
                        ));
                    });
                    goal.spawn((
                        GoalPrimaryText,
                        Text::new("0"),
                        TextFont {
                            font_size: FontSize::Px(18.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.90, 1.0, 0.90)),
                    ));
                    goal.spawn((
                        GoalTargetText,
                        Text::new(""),
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::srgba(0.90, 1.0, 0.90, 0.68)),
                    ));
                });

                // GoalHintContainer
                hud.spawn((
                    GoalHintContainer,
                    Node {
                        position_type: PositionType::Absolute,
                        bottom: Val::Px(64.0),
                        left: Val::Px(12.0),
                        max_width: Val::Px(180.0),
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(7.0)),
                        border: UiRect::all(Val::Px(1.5)),
                        display: Display::None,
                        ..default()
                    },
                    BorderColor::all(Color::srgba(0.70, 0.90, 1.0, 0.35)),
                    BackgroundColor(Color::srgba(0.04, 0.06, 0.09, 0.94)),
                    Visibility::Hidden,
                ))
                .with_children(|hint| {
                    hint.spawn((
                        GoalHintText,
                        Text::new(""),
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });

                // ShopToggleButton
                hud.spawn((
                    Button,
                    ShopToggleButton,
                    Node {
                        position_type: PositionType::Absolute,
                        bottom: Val::Px(16.0),
                        right: Val::Px(12.0),
                        width: Val::Px(78.0),
                        height: Val::Px(76.0),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(2.0),
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(8.0)),
                        border: UiRect::all(Val::Px(1.5)),
                        ..default()
                    },
                    BorderColor::all(BTN_BORDER_IDLE),
                    BackgroundColor(Color::srgba(0.07, 0.10, 0.17, 0.88)),
                    Visibility::Hidden,
                ))
                .with_children(|b| {
                    b.spawn((
                        ShopHeaderLabel,
                        Text::new("SHOP"),
                        TextFont {
                            font_size: FontSize::Px(10.0),
                            ..default()
                        },
                        TextColor(Color::srgb(0.70, 0.86, 1.0)),
                    ));
                    b.spawn((
                        ShopReserveText,
                        Text::new("0"),
                        TextFont {
                            font_size: FontSize::Px(24.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                    b.spawn((
                        ShopCoresLabel,
                        Text::new("cores"),
                        TextFont {
                            font_size: FontSize::Px(10.0),
                            ..default()
                        },
                        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.58)),
                    ));
                });
            })
            .id();
    });

    // BoonTooltipContainer
    commands.entity(hud_root_id).with_children(|parent| {
        parent
            .spawn((
                BoonTooltipContainer,
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(120.0),
                    left: Val::Percent(50.0),
                    margin: UiRect::left(Val::Px(-200.0)),
                    width: Val::Px(400.0),
                    max_width: Val::Percent(90.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    padding: UiRect::all(Val::Px(10.0)),
                    border: UiRect::all(Val::Px(1.5)),
                    ..default()
                },
                BorderColor::all(Color::srgba(0.85, 0.65, 0.18, 0.75)),
                BackgroundColor(Color::srgba(0.06, 0.07, 0.10, 0.95)),
                Visibility::Hidden,
            ))
            .with_children(|t| {
                t.spawn((
                    BoonTooltipText,
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(13.0),
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });
    });

    spawn_shop_bar(&mut commands, hud_root_id, settings.language);
    spawn_shop_active_badge(&mut commands, hud_root_id);
    spawn_tutorial_overlay(&mut commands);
}

#[derive(Component)]
struct Watermark;

fn setup_watermark(mut commands: Commands, asset_server: Res<AssetServer>) {
    let bird = asset_server.load(embedded::watermark_path());
    let cyan = Color::srgba(0.3, 0.9, 1.0, 0.35);
    commands
        .spawn((
            Watermark,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(4.0),
                left: Val::Percent(44.0), // Centrado en la parte inferior para evitar sobrelapes
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(5.0),
                ..default()
            },
        ))
        .with_children(|row| {
            row.spawn((
                ImageNode {
                    image: bird,
                    color: cyan,
                    ..default()
                },
                Node {
                    width: Val::Px(18.0),
                    height: Val::Px(18.0),
                    ..default()
                },
            ));
            row.spawn((
                Text::new("0.19"),
                TextFont {
                    font_size: FontSize::Px(12.0),
                    ..default()
                },
                TextColor(cyan),
            ));
            row.spawn((
                FpsWatermarkText,
                Text::new("FPS --"),
                TextFont {
                    font_size: FontSize::Px(12.0),
                    ..default()
                },
                TextColor(cyan),
                Visibility::Hidden,
            ));
        });
}

fn spawn_shop_bar(commands: &mut Commands, parent: Entity, lang: Language) {
    let bar = commands
        .spawn((
            ShopBar,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(102.0),
                right: Val::Px(12.0),
                width: Val::Px(360.0),
                max_width: Val::Percent(93.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Stretch,
                row_gap: Val::Px(10.0),
                padding: UiRect::all(Val::Px(14.0)),
                border: UiRect::all(Val::Px(1.5)),
                ..default()
            },
            BorderColor::all(Color::srgba(0.50, 0.74, 1.0, 0.28)),
            BackgroundColor(Color::srgba(0.05, 0.08, 0.13, 0.94)),
        ))
        .with_children(|bar| {
            bar.spawn((
                ShopModifiersLabel,
                Text::new("MODIFICADORES DE TIENDA"),
                TextFont {
                    font_size: FontSize::Px(12.0),
                    ..default()
                },
                TextColor(Color::srgb(0.72, 0.88, 1.0)),
            ));
            bar.spawn((Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(6.0),
                row_gap: Val::Px(6.0),
                ..default()
            },))
                .with_children(|list| {
                    for item in ShopItem::ALL {
                        list.spawn((
                            Button,
                            ShopCard,
                            ShopButton(item),
                            get_item_tooltip(item, lang),
                            Node {
                                width: Val::Percent(48.0),
                                min_height: Val::Px(52.0),
                                justify_content: JustifyContent::SpaceBetween,
                                flex_direction: FlexDirection::Column,
                                align_items: AlignItems::Stretch,
                                padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                                border: UiRect::all(Val::Px(1.5)),
                                ..default()
                            },
                            BackgroundColor(BTN_IDLE),
                            BorderColor::all(BTN_BORDER_IDLE),
                        ))
                        .with_children(|b| {
                            b.spawn((Node {
                                width: Val::Percent(100.0),
                                justify_content: JustifyContent::SpaceBetween,
                                align_items: AlignItems::Center,
                                ..default()
                            },))
                                .with_children(|row| {
                                    row.spawn((
                                        Text::new(item.label(lang)),
                                        TextFont {
                                            font_size: FontSize::Px(14.0),
                                            ..default()
                                        },
                                        TextColor(Color::WHITE),
                                    ));
                                    row.spawn((
                                        ShopButtonCostText(item),
                                        Text::new(""),
                                        TextFont {
                                            font_size: FontSize::Px(13.0),
                                            ..default()
                                        },
                                        TextColor(Color::srgb(1.0, 0.86, 0.48)),
                                    ));
                                });
                            b.spawn((
                                ShopButtonStatusText(item),
                                Text::new(item.status_label(lang)),
                                TextFont {
                                    font_size: FontSize::Px(10.0),
                                    ..default()
                                },
                                TextColor(Color::srgb(0.64, 0.81, 0.98)),
                            ));
                        });
                    }
                });
        })
        .id();
    commands.entity(parent).add_child(bar);
}

fn spawn_tutorial_overlay(commands: &mut Commands) {
    // Tutorial Overlay (inicialmente oculto)
    commands
        .spawn((
            TutorialOverlayRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_direction: FlexDirection::Column,
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.02, 0.85)),
            Visibility::Hidden,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Px(360.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    padding: UiRect::all(Val::Px(20.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    row_gap: Val::Px(16.0),
                    ..default()
                },
                BorderColor::all(Color::srgb(0.65, 0.85, 1.0)),
                BackgroundColor(Color::srgba(0.05, 0.05, 0.10, 0.95)),
            ))
            .with_children(|box_node| {
                box_node.spawn((
                    TutorialTitleText,
                    Text::new("TUTORIAL - CÓMO JUGAR"),
                    TextFont {
                        font_size: FontSize::Px(22.0),
                        ..default()
                    },
                    TextColor(Color::srgb(1.2, 1.4, 2.0)),
                ));

                box_node.spawn((
                    TutorialText,
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(14.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.85, 0.90, 1.0)),
                ));

                // Fila de controles del tutorial (Toggle Checkbox + Botón Entendí)
                box_node
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(16.0),
                        ..default()
                    })
                    .with_children(|row| {
                        // Checkbox / Toggle
                        row.spawn((
                            Button,
                            TutorialOverlayToggle,
                            Node {
                                width: Val::Px(160.0),
                                height: Val::Px(40.0),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                border: UiRect::all(Val::Px(1.5)),
                                ..default()
                            },
                            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.2)),
                            BackgroundColor(Color::srgba(0.1, 0.1, 0.18, 0.7)),
                        ))
                        .with_children(|b| {
                            b.spawn((
                                TutorialOverlayToggleText,
                                Text::new(""),
                                TextFont {
                                    font_size: FontSize::Px(14.0),
                                    ..default()
                                },
                                TextColor(Color::WHITE),
                            ));
                        });

                        // Botón "Entendí" (verde)
                        row.spawn((
                            Button,
                            TutorialCloseButton,
                            Node {
                                width: Val::Px(120.0),
                                height: Val::Px(40.0),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                border: UiRect::all(Val::Px(1.5)),
                                ..default()
                            },
                            BorderColor::all(Color::srgb(0.4, 1.0, 0.4)),
                            BackgroundColor(Color::srgba(0.05, 0.18, 0.05, 0.90)),
                        ))
                        .with_children(|b| {
                            b.spawn((
                                TutorialCloseBtnLabel,
                                Text::new("Entendí"),
                                TextFont {
                                    font_size: FontSize::Px(16.0),
                                    ..default()
                                },
                                TextColor(Color::srgb(0.85, 1.0, 0.85)),
                            ));
                        });
                    });
            });
        });
}

fn spawn_shop_active_badge(commands: &mut Commands, parent: Entity) {
    let badge = commands
        .spawn((
            ShopActiveBadge,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(102.0),
                left: Val::Px(12.0),
                max_width: Val::Px(280.0),
                padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                border: UiRect::all(Val::Px(1.5)),
                display: Display::None,
                ..default()
            },
            BorderColor::all(Color::srgba(1.0, 0.86, 0.46, 0.88)),
            BackgroundColor(Color::srgba(0.18, 0.14, 0.04, 0.92)),
            Visibility::Hidden,
        ))
        .with_children(|badge| {
            badge.spawn((
                ShopActiveBadgeText,
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.95, 0.78)),
            ));
        })
        .id();
    commands.entity(parent).add_child(badge);
}

/// The in-match HUD entities (spawned once at `Startup`) are hidden while the main menu is up and
/// shown again when a match begins, so "Nivel 1 / Moves / Meta" don't sit behind the menu.
type HudFilter = Or<(
    With<ScoreText>,
    With<MovesText>,
    With<GoalText>,
    With<LivesText>,
    // The booster bar root — its children inherit visibility, so hiding the root hides the bar.
    With<ShopBar>,
    With<PauseButton>,
    With<ShopToggleButton>,
    With<ShopActiveBadge>,
    With<SlowMoBadge>,
    With<GoalHintContainer>,
    With<StatsButton>,
    With<StatsPopupContainer>,
    With<BoonIndicatorBar>,
    With<HudRoot>,
)>;

fn hide_hud(mut q: Query<&mut Visibility, HudFilter>) {
    for mut v in &mut q {
        *v = Visibility::Hidden;
    }
}

fn show_hud(mut q: Query<&mut Visibility, HudFilter>) {
    for mut v in &mut q {
        *v = Visibility::Visible;
    }
}

fn update_watermark_fps(
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    settings: Res<WindowSettings>,
    text: Single<(&mut Text, &mut Visibility), With<FpsWatermarkText>>,
    mut state: Local<FpsWatermarkState>,
) {
    let (mut text, mut visibility) = text.into_inner();
    if !settings.show_fps_watermark {
        if state.visible {
            *visibility = Visibility::Hidden;
            state.visible = false;
        }
        return;
    }
    if !state.visible {
        *visibility = Visibility::Visible;
        state.visible = true;
    }
    state.elapsed_since_refresh += time.delta_secs();
    if state.elapsed_since_refresh < 0.25 && state.last_fps.is_some() {
        return;
    }
    state.elapsed_since_refresh = 0.0;
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0)
        .round()
        .max(0.0) as u32;
    if state.last_fps != Some(fps) {
        text.0 = format!("FPS {fps:>4}");
        state.last_fps = Some(fps);
    }
}

fn update_slow_mo_badge(
    virtual_time: Res<Time<Virtual>>,
    badge: Single<(&mut Visibility, &mut Node), With<SlowMoBadge>>,
    mut text: Single<&mut Text, With<SlowMoBadgeText>>,
    mut state: Local<SlowMoBadgeState>,
) {
    let speed = virtual_time.relative_speed();
    let active = speed < 0.99;
    let tenths = (speed * 10.0).round() as i32;
    if state.active == active && (!active || state.last_tenths == tenths) {
        return;
    }
    state.active = active;
    state.last_tenths = tenths;

    let (mut visibility, mut node) = badge.into_inner();
    *visibility = if active {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    node.display = if active { Display::Flex } else { Display::None };
    if active {
        text.0 = format!("SLOW x{speed:.1}");
    }
}

fn update_score_text(score: Res<DisplayedScore>, mut text: Single<&mut Text2d, With<ScoreText>>) {
    text.0 = format!("{}", score.0);
}

fn update_score_glow(
    time: Res<Time>,
    mut glow: ResMut<ScoreGlow>,
    mut color: Single<&mut TextColor, With<ScoreText>>,
) {
    glow.pulse = (glow.pulse - SCORE_PULSE_DECAY * time.delta_secs()).max(0.0);
    let osc = (time.elapsed_secs() * SCORE_PULSE_FREQ).sin() * 0.5 + 0.5; // 0..1, fast
    let brightness = SCORE_NEON_BASE + glow.pulse * SCORE_NEON_PULSE * osc;
    let c = glow.rgb * brightness;
    let max_val = c.x.max(c.y).max(c.z);
    if max_val > 1.0 {
        color.0 = Color::srgb(c.x / max_val, c.y / max_val, c.z / max_val);
    } else {
        color.0 = Color::srgb(c.x, c.y, c.z);
    }
}

/// Parks the world-space score anchor at `SCORE_ANCHOR_SCREEN` projected through the camera,
/// every frame — so it stays put regardless of window size — and publishes that world point in
/// `ScoreAnchor` for the core-shards to home in on.
fn position_score(
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform), With<WorldCamera>>,
    mut anchor: ResMut<ScoreAnchor>,
    mut score_t: Single<&mut Transform, With<ScoreText>>,
    final_camera: Single<&Camera, With<FinalCamera>>,
) {
    let (cam, cam_t) = *camera;
    let final_cam = *final_camera;
    let (vp_pos, vp_size) = if let Some(ref viewport) = final_cam.viewport {
        let scale_factor = window.scale_factor();
        let pos = Vec2::new(
            viewport.physical_position.x as f32,
            viewport.physical_position.y as f32,
        ) / scale_factor;
        let size = Vec2::new(
            viewport.physical_size.x as f32,
            viewport.physical_size.y as f32,
        ) / scale_factor;
        (pos, size)
    } else {
        (Vec2::ZERO, window.size())
    };
    // Score siempre centrado en el tercio superior de la pantalla
    let score_screen_pos = Vec2::new(vp_size.x * 0.5, 36.0);
    let score_window_pos = score_screen_pos + vp_pos;
    let Some(world) = window_point_to_world(cam, cam_t, vp_size, score_window_pos) else {
        return;
    };
    let pos = world.extend(6.0); // above the board and shards
    anchor.0 = pos;
    score_t.translation = pos;
}

fn update_moves_text(
    mode: Res<GameMode>,
    level: Res<LevelConfig>,
    moves: Res<MovesLeft>,
    mut q_num: Query<&mut Text, With<MovesNumberText>>,
    mut q_badge: Query<&mut Node, With<MovesText>>,
) {
    let is_unbounded = mode.is_sandbox()
        || matches!(
            level.goal,
            LevelGoal::TimedScore { .. } | LevelGoal::TimedCollectColor { .. }
        );

    if let Ok(mut text) = q_num.single_mut() {
        if is_unbounded {
            text.0 = String::new();
        } else {
            text.0 = format!("{}", moves.0);
        }
    }

    if let Ok(mut node) = q_badge.single_mut() {
        if is_unbounded {
            node.display = Display::None;
        } else {
            node.display = Display::Flex;
        }
    }
}

fn update_goal_text(
    mode: Res<GameMode>,
    level: Res<LevelConfig>,
    score: Res<DisplayedScore>,
    collected: Res<SparksCollected>,
    shadow_count: Res<ShadowCount>,
    level_timer: Res<LevelTimer>,
    displayed_cores: Res<DisplayedCollectedCores>,
    cache: Res<VisualCache>,
    panel: Single<(&mut BackgroundColor, &mut BorderColor), With<GoalText>>,
    icon: Single<
        &mut ImageNode,
        (
            With<GoalIconImage>,
            Without<GoalPrimaryText>,
            Without<GoalTargetText>,
        ),
    >,
    primary: Single<
        (&mut Text, &mut TextColor),
        (
            With<GoalPrimaryText>,
            Without<GoalIconImage>,
            Without<GoalTargetText>,
        ),
    >,
    target_text: Single<
        (&mut Text, &mut TextColor),
        (
            With<GoalTargetText>,
            Without<GoalIconImage>,
            Without<GoalPrimaryText>,
        ),
    >,
) {
    // The goal icon is a tinted swatch of the actual asset the player needs to consume: a round
    // "core" for anything that collects lightcores/ingredients/colors, a square for jelly tiles —
    // deliberately not a text glyph, since some (e.g. `▧`) render as an empty box on fonts missing
    // that codepoint (see `GoalIconImage`'s doc comment).
    let (icon_image, icon_color, primary_value, target_value) = if mode.is_sandbox() {
        (
            cache.core_image.clone(),
            Color::srgb(0.65, 0.85, 1.0),
            format!("{}", score.0),
            String::new(),
        )
    } else {
        match &level.goal {
            LevelGoal::Score(target) => (
                cache.core_image.clone(),
                Color::srgb(0.65, 0.85, 1.0),
                format!("{}", score.0),
                format!("/ {}", target),
            ),
            LevelGoal::Sparks => (
                cache.core_image.clone(),
                Color::srgb(1.0, 0.58, 0.12),
                format!("{}", collected.0),
                format!("/ {}", level.sparks_total),
            ),
            LevelGoal::ClearShadow => (
                cache.square_image.clone(),
                Color::srgba(0.22, 0.55, 1.0, 0.82),
                format!("{}", shadow_count.0),
                String::new(),
            ),
            LevelGoal::TimedScore { target, .. } => {
                let remaining = level_timer
                    .0
                    .as_ref()
                    .map(Timer::remaining_secs)
                    .unwrap_or(0.0)
                    .max(0.0);
                let (mins, secs) = (remaining as u32 / 60, remaining as u32 % 60);
                (
                    cache.core_image.clone(),
                    Color::srgb(1.0, 0.86, 0.34),
                    format!("{mins:02}:{secs:02}"),
                    format!("{} / {}", score.0, target),
                )
            }
            LevelGoal::CollectColor { color, target } => {
                let current = displayed_cores.0[color.index()];
                (
                    cache.core_image.clone(),
                    color.bevy_color(),
                    format!("{}", current),
                    format!("/ {}", target),
                )
            }
            LevelGoal::TimedCollectColor { color, target, .. } => {
                let remaining = level_timer
                    .0
                    .as_ref()
                    .map(Timer::remaining_secs)
                    .unwrap_or(0.0)
                    .max(0.0);
                let (mins, secs) = (remaining as u32 / 60, remaining as u32 % 60);
                let current = displayed_cores.0[color.index()];
                (
                    cache.core_image.clone(),
                    color.bevy_color(),
                    format!("{mins:02}:{secs:02}"),
                    format!("{} / {}", current, target),
                )
            }
        }
    };

    let (mut bg, mut border) = panel.into_inner();
    bg.0 = Color::srgba(0.05, 0.08, 0.10, 0.78);
    *border = BorderColor::all(icon_color.with_alpha(0.38));
    let mut icon_node = icon.into_inner();
    icon_node.image = icon_image;
    icon_node.color = icon_color;

    let (mut primary_text, mut primary_color) = primary.into_inner();
    primary_text.0 = primary_value;
    primary_color.0 = Color::WHITE;

    let (mut target, mut target_color) = target_text.into_inner();
    target.0 = target_value;
    target_color.0 = icon_color.with_alpha(0.78);
}

fn update_goal_hint(
    time: Res<Time>,
    mode: Res<GameMode>,
    level: Res<LevelConfig>,
    state: Res<State<GameState>>,
    settings: Res<WindowSettings>,
    mut touch_timer: ResMut<GoalHintTouchTimer>,
    interaction: Single<&Interaction, With<GoalText>>,
    hint: Single<(&mut Visibility, &mut Node), With<GoalHintContainer>>,
    mut hint_text: Single<&mut Text, With<GoalHintText>>,
) {
    let (mut visibility, mut node) = hint.into_inner();
    if *state.get() != GameState::Playing {
        *visibility = Visibility::Hidden;
        node.display = Display::None;
        touch_timer.0 = None;
        return;
    }

    if **interaction == Interaction::Pressed {
        touch_timer.0 = Some(Timer::from_seconds(2.0, TimerMode::Once));
    }
    if let Some(timer) = touch_timer.0.as_mut() {
        timer.tick(time.delta());
        if timer.is_finished() {
            touch_timer.0 = None;
        }
    }

    let show = matches!(**interaction, Interaction::Hovered | Interaction::Pressed)
        || touch_timer.0.is_some();
    *visibility = if show {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    node.display = if show { Display::Flex } else { Display::None };

    if show {
        hint_text.0 = goal_hint_text(&mode, &level, settings.language);
    }
}

fn goal_hint_text(mode: &GameMode, level: &LevelConfig, lang: Language) -> String {
    if mode.is_sandbox() {
        return lang.tr(TrKey::GoalFreePlay).to_string();
    }
    match &level.goal {
        LevelGoal::Score(_) => lang.tr(TrKey::GoalReachTarget).to_string(),
        LevelGoal::Sparks => lang.tr(TrKey::GoalRescueSparks).to_string(),
        LevelGoal::ClearShadow => lang.tr(TrKey::GoalClearShadows).to_string(),
        LevelGoal::TimedScore { .. } => lang.tr(TrKey::GoalScoreOnClock).to_string(),
        LevelGoal::CollectColor { .. } => lang.tr(TrKey::GoalCollectColor).to_string(),
        LevelGoal::TimedCollectColor { .. } => lang.tr(TrKey::GoalColorOnClock).to_string(),
    }
}

fn stats_button_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<StatsButton>)>,
    mut open: ResMut<StatsPopupOpen>,
    state: Res<State<GameState>>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    if *state.get() != GameState::Playing {
        return;
    }
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    for interaction in &interactions {
        if *interaction == Interaction::Pressed {
            open.0 = !open.0;
        }
    }
}

fn update_stats_popup(
    stats: Res<StatsBook>,
    open: Res<StatsPopupOpen>,
    settings: Res<WindowSettings>,
    mut q_visible: Single<&mut Visibility, With<StatsPopupContainer>>,
    mut q_node: Single<&mut Node, With<StatsPopupContainer>>,
    mut q_text: Single<&mut Text, With<StatsPopupText>>,
) {
    let lang = settings.language;
    if open.0 {
        **q_visible = Visibility::Visible;
        q_node.display = Display::Flex;
        q_text.0 = format!(
            "{}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}x\n{}: {}",
            lang.tr(TrKey::StatsTitle),
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
            stats.total_chains
        );
    } else {
        **q_visible = Visibility::Hidden;
        q_node.display = Display::None;
    }
}

fn pause_button_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<PauseButton>)>,
    state: Res<State<GameState>>,
    tutorial: Res<TutorialState>,
    mut next: ResMut<NextState<GameState>>,
) {
    if *state.get() != GameState::Playing || tutorial.open {
        return;
    }
    for interaction in &interactions {
        if *interaction == Interaction::Pressed {
            next.set(GameState::Paused);
        }
    }
}

fn shop_toggle_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<ShopToggleButton>)>,
    state: Res<State<GameState>>,
    tutorial: Res<TutorialState>,
    mut shop: ResMut<Shop>,
) {
    if *state.get() != GameState::Playing || tutorial.open {
        return;
    }
    for interaction in &interactions {
        if *interaction == Interaction::Pressed {
            shop.open = !shop.open;
        }
    }
}

fn update_shop_toggle_button(
    shop: Res<Shop>,
    button: Single<(&mut BackgroundColor, &mut BorderColor), With<ShopToggleButton>>,
) {
    let (mut bg, mut border) = button.into_inner();
    if shop.open {
        bg.0 = Color::srgba(0.19, 0.15, 0.04, 0.94);
        *border = BorderColor::all(BTN_BORDER_ARMED);
    } else {
        bg.0 = Color::srgba(0.07, 0.10, 0.17, 0.88);
        *border = BorderColor::all(BTN_BORDER_IDLE);
    }
}

pub(crate) fn update_shop_bar_visibility(
    shop: Res<Shop>,
    mut v: Single<&mut Visibility, With<ShopBar>>,
) {
    **v = if shop.open {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
}

fn update_shop_reserve_text(
    reserve: Res<CoreReserve>,
    mut text: Single<&mut Text, With<ShopReserveText>>,
) {
    text.0 = reserve.0.to_string();
}

fn update_shop_button_texts(
    reserve: Res<CoreReserve>,
    run: Res<RunState>,
    shop: Res<Shop>,
    settings: Res<WindowSettings>,
    mut texts: Query<(&ShopButtonStatusText, &mut Text, &mut TextColor)>,
    mut costs: Query<
        (&ShopButtonCostText, &mut Text, &mut TextColor),
        Without<ShopButtonStatusText>,
    >,
) {
    let lang = settings.language;
    for (status, mut text, mut color) in &mut texts {
        let item = status.0;
        if shop.armed_item() == Some(item) {
            text.0 = if item == ShopItem::Swap && shop.has_first_pick() {
                lang.tr(TrKey::ActiveChooseTarget).to_string()
            } else {
                lang.tr(TrKey::ActiveReady).to_string()
            };
            color.0 = Color::srgb(1.0, 0.95, 0.78);
        } else if let Some(cost) = item.cost(&run) {
            if reserve.0 >= cost {
                text.0 = item.status_label(lang).to_string();
                color.0 = Color::srgb(0.64, 0.81, 0.98);
            } else {
                text.0 = lang.tr(TrKey::NotEnoughCores).to_string();
                color.0 = BTN_BORDER_BROKE.with_alpha(0.92);
            }
        } else {
            text.0 = "MAX".to_string();
            color.0 = Color::srgb(1.0, 0.95, 0.78);
        }
    }

    for (cost_item, mut text, mut color) in &mut costs {
        if let Some(cost) = cost_item.0.cost(&run) {
            text.0 = format!("{}c", cost);
            color.0 = if reserve.0 >= cost {
                Color::srgb(1.0, 0.86, 0.48)
            } else {
                BTN_BORDER_BROKE.with_alpha(0.92)
            };
        } else {
            text.0 = "MAX".to_string();
            color.0 = Color::srgb(1.0, 0.95, 0.78);
        }
    }
}

fn update_shop_active_badge(
    shop: Res<Shop>,
    settings: Res<WindowSettings>,
    badge: Single<(&mut Visibility, &mut Node), With<ShopActiveBadge>>,
    mut text: Single<&mut Text, With<ShopActiveBadgeText>>,
) {
    let (mut visibility, mut node) = badge.into_inner();
    if let Some(label) = shop.active_badge_text(settings.language) {
        *visibility = Visibility::Visible;
        node.display = Display::Flex;
        text.0 = label;
    } else {
        *visibility = Visibility::Hidden;
        node.display = Display::None;
        text.0.clear();
    }
}

#[derive(Resource, Default)]
pub(crate) struct TutorialState {
    pub(crate) open: bool,
}

#[derive(Component)]
pub(crate) struct TutorialOverlayRoot;
#[derive(Component)]
struct TutorialCloseButton;

fn check_show_tutorial_on_start(
    settings: Res<WindowSettings>,
    level: Res<LevelConfig>,
    mut state: ResMut<TutorialState>,
    mut shown: ResMut<LevelTutorialShown>,
    mut q_title: Single<&mut Text, (With<TutorialTitleText>, Without<TutorialText>)>,
    mut q_text: Single<&mut Text, (With<TutorialText>, Without<TutorialTitleText>)>,
) {
    if settings.tutorial_enabled && !shown.0 {
        state.open = true;
        shown.0 = true;

        let lang = settings.language;
        let (title, description) = match &level.goal {
            LevelGoal::Score(target) => (
                lang.tr(TrKey::TutorialScoreTitle),
                if lang == crate::core::locale::Language::English {
                    format!(
                        "• Slide adjacent nuclei to align them in groups of 3 or more of the same color.\n\n\
                         • GOAL: Reach at least {} points.\n\n\
                         • After winning or choosing a boon, tap anywhere to advance to the next level.\n\n\
                         • You can disable this tutorial with the button below or in Options.",
                        target
                    )
                } else {
                    format!(
                        "• Desliza núcleos adyacentes para alinearlos en grupos de 3 o más del mismo color.\n\n\
                         • OBJETIVO: Consigue al menos {} puntos en total.\n\n\
                         • Al ganar o elegir un boon (mejora), haz click/tap en cualquier parte de la pantalla para avanzar al siguiente nivel en el mapa.\n\n\
                         • Puedes desactivar este tutorial con el botón de abajo o en Opciones.",
                        target
                    )
                },
            ),
            LevelGoal::Sparks => (
                lang.tr(TrKey::TutorialSparksTitle),
                if lang == crate::core::locale::Language::English {
                    "• GOAL: Bring sparks (hexagonal ingredients) to the bottom row to collect them.\n\n\
                     • Sparks only fall vertically (they don't slide diagonally like normal pieces).\n\n\
                     • After winning or choosing a boon, tap anywhere to advance to the next level.\n\n\
                     • You can disable this tutorial with the button below or in Options.".to_string()
                } else {
                    "• OBJETIVO: Lleva las chispas (ingredientes con forma hexagonal) hasta el final de su columna (fila inferior) para recolectarlas.\n\n\
                     • Las chispas solo caen en vertical (no deslizan diagonalmente como las piezas normales).\n\n\
                     • Al ganar o elegir un boon (mejora), haz click/tap en cualquier parte de la pantalla para avanzar al siguiente nivel en el mapa.\n\n\
                     • Puedes desactivar este tutorial con el botón de abajo o en Opciones.".to_string()
                },
            ),
            LevelGoal::ClearShadow => (
                lang.tr(TrKey::TutorialShadowTitle),
                if lang == crate::core::locale::Language::English {
                    "• GOAL: Clear all dark tiles (shadows) from the board.\n\n\
                     • To clear a shadow, make a match of 3 or more pieces on top of it.\n\n\
                     • After winning or choosing a boon, tap anywhere to advance to the next level.\n\n\
                     • You can disable this tutorial with the button below or in Options.".to_string()
                } else {
                    "• OBJETIVO: Limpia todas las casillas oscuras (sombras) del tablero.\n\n\
                     • Para limpiar una sombra, realiza una combinación de 3 o más piezas sobre ella.\n\n\
                     • Al ganar o elegir un boon (mejora), haz click/tap en cualquier parte de la pantalla para avanzar al siguiente nivel en el mapa.\n\n\
                     • Puedes desactivar este tutorial con el botón de abajo o en Opciones.".to_string()
                },
            ),
            LevelGoal::TimedScore { target, .. } => (
                lang.tr(TrKey::TutorialTimedScoreTitle),
                if lang == crate::core::locale::Language::English {
                    format!(
                        "• GOAL: Reach at least {} points before the timer hits zero.\n\n\
                         • No move limit! Combine fast to maximize your score.\n\n\
                         • After winning or choosing a boon, tap anywhere to advance to the next level.\n\n\
                         • You can disable this tutorial with the button below or in Options.",
                        target
                    )
                } else {
                    format!(
                        "• OBJETIVO: Consigue al menos {} puntos antes de que el reloj de arriba llegue a cero.\n\n\
                         • ¡No hay límite de movimientos! Combina rápido para maximizar tu puntuación.\n\n\
                         • Al ganar o elegir un boon (mejora), haz click/tap en cualquier parte de la pantalla para avanzar al siguiente nivel en el mapa.\n\n\
                         • Puedes desactivar este tutorial con el botón de abajo o en Opciones.",
                        target
                    )
                },
            ),
            LevelGoal::CollectColor { color, target } => {
                let color_name = lang.tr(match color {
                    LightColor::Red => TrKey::ColorRed,
                    LightColor::Green => TrKey::ColorGreen,
                    LightColor::Blue => TrKey::ColorBlue,
                    LightColor::Yellow => TrKey::ColorYellow,
                    LightColor::Purple => TrKey::ColorPurple,
                });
                (
                    lang.tr(TrKey::TutorialCollectColorTitle),
                    if lang == crate::core::locale::Language::English {
                        format!(
                            "• GOAL: Collect at least {} {} nuclei.\n\n\
                             • Only nuclei of this color count toward your goal, but you can match others to clear the board.\n\n\
                             • After winning or choosing a boon, tap anywhere to advance to the next level.\n\n\
                             • You can disable this tutorial with the toggle below.",
                            target, color_name
                        )
                    } else {
                        format!(
                            "• OBJETIVO: Junta al menos {} núcleos de color {}.\n\n\
                             • Solo los núcleos de este color sumarán a tu meta, pero puedes combinar los otros para despejar el tablero.\n\n\
                             • Al ganar o elegir un boon (mejora), haz click/tap en cualquier parte de la pantalla para avanzar al siguiente nivel en el mapa.\n\n\
                             • Puedes desactivar este tutorial con el toggle de abajo.",
                            target, color_name
                        )
                    },
                )
            }
            LevelGoal::TimedCollectColor { color, target, .. } => {
                let color_name = lang.tr(match color {
                    LightColor::Red => TrKey::ColorRed,
                    LightColor::Green => TrKey::ColorGreen,
                    LightColor::Blue => TrKey::ColorBlue,
                    LightColor::Yellow => TrKey::ColorYellow,
                    LightColor::Purple => TrKey::ColorPurple,
                });
                (
                    lang.tr(TrKey::TutorialTimedColorTitle),
                    if lang == crate::core::locale::Language::English {
                        format!(
                            "• GOAL: Collect at least {} {} nuclei before the timer hits zero.\n\n\
                             • No move limit! Combine fast, focused on this color.\n\n\
                             • After winning or choosing a boon, tap anywhere to advance to the next level.\n\n\
                             • You can disable this tutorial with the toggle below.",
                            target, color_name
                        )
                    } else {
                        format!(
                            "• OBJETIVO: Junta al menos {} núcleos de color {} antes de que el reloj llegue a cero.\n\n\
                             • ¡No hay límite de movimientos! Combina rápido enfocado en este color.\n\n\
                             • Al ganar o elegir un boon (mejora), haz click/tap en cualquier parte de la pantalla para avanzar al siguiente nivel en el mapa.\n\n\
                             • Puedes desactivar este tutorial con el toggle de abajo.",
                            target, color_name
                        )
                    },
                )
            }
        };

        q_title.0 = title.to_string();
        q_text.0 = description;
    }
}

fn reset_tutorial_state(mut state: ResMut<TutorialState>) {
    state.open = false;
}

fn update_tutorial_visibility(
    state: Res<TutorialState>,
    mut q_visible: Single<&mut Visibility, With<TutorialOverlayRoot>>,
    mut q_node: Single<&mut Node, With<TutorialOverlayRoot>>,
) {
    if state.open {
        **q_visible = Visibility::Visible;
        q_node.display = Display::Flex;
    } else {
        **q_visible = Visibility::Hidden;
        q_node.display = Display::None;
    }
}

fn tutorial_close_button_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<TutorialCloseButton>)>,
    mut state: ResMut<TutorialState>,
) {
    for interaction in &interactions {
        if *interaction == Interaction::Pressed {
            state.open = false;
        }
    }
}

// Removed tutorial_disable_button_system as global toggle is controlled from menus.

#[derive(Component)]
pub(crate) struct BoonIndicatorBar;

#[derive(Component)]
pub(crate) struct HudRoot;

#[derive(Component)]
struct TutorialTitleText;

#[derive(Component)]
struct TutorialText;

fn update_boon_indicators(
    run: Res<RunState>,
    settings: Res<WindowSettings>,
    mut commands: Commands,
    bar: Single<Entity, With<BoonIndicatorBar>>,
) {
    let bar_entity = *bar;
    let lang = settings.language;

    // Clear old indicators
    commands.entity(bar_entity).despawn_children();

    // Spawn new indicators for active boons
    commands.entity(bar_entity).with_children(|parent| {
        for boon in BoonKind::ALL {
            let lvl = run.level(boon);
            if lvl > 0 {
                let (label, color) = match boon {
                    BoonKind::RedValue => ("R", Color::srgba(1.2, 0.4, 0.4, 0.85)),
                    BoonKind::GreenReserve => ("G", Color::srgba(0.4, 1.2, 0.4, 0.85)),
                    BoonKind::BlueMoves => ("B", Color::srgba(0.4, 0.6, 1.3, 0.85)),
                    BoonKind::SparkBounty => ("S", Color::srgba(1.3, 0.7, 0.1, 0.85)),
                    BoonKind::PowerBounty => ("P", Color::srgba(1.1, 0.4, 1.2, 0.85)),
                    BoonKind::HollowWard => ("H", Color::srgba(0.5, 0.5, 0.6, 0.85)),
                };

                parent
                    .spawn((
                        Interaction::default(),
                        get_item_tooltip(ShopItem::Boon(boon), lang),
                        Node {
                            width: Val::Px(24.0),
                            height: Val::Px(24.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.25)),
                        BackgroundColor(color),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            Text::new(format!("{}{}", label, lvl)),
                            TextFont {
                                font_size: FontSize::Px(11.0),
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                    });
            }
        }
    });
}

#[derive(Component)]
struct TutorialOverlayToggle;

#[derive(Component)]
struct TutorialOverlayToggleText;

#[derive(Resource, Default)]
pub(crate) struct LevelTutorialShown(pub(crate) bool);

fn reset_level_tutorial_shown(mut shown: ResMut<LevelTutorialShown>) {
    shown.0 = false;
}

fn tutorial_overlay_toggle_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<TutorialOverlayToggle>)>,
    mut settings: ResMut<WindowSettings>,
) {
    for interaction in &interactions {
        if *interaction == Interaction::Pressed {
            settings.tutorial_enabled = !settings.tutorial_enabled;
        }
    }
}

#[derive(Component)]
pub(crate) struct LivesText;

#[derive(Component)]
pub(crate) struct LivesNumberText;

fn update_lives_text(
    mode: Res<GameMode>,
    run: Res<RunState>,
    mut q_root: Query<&mut Visibility, With<LivesText>>,
    mut q_text: Query<&mut Text, With<LivesNumberText>>,
) {
    let lives_visible = mode.is_run();
    for mut v in &mut q_root {
        *v = if lives_visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    if lives_visible {
        for mut text in &mut q_text {
            text.0 = format!("{}", run.lives);
        }
    }
}

fn update_tutorial_overlay_toggle_text(
    settings: Res<WindowSettings>,
    mut q: Query<&mut Text, With<TutorialOverlayToggleText>>,
) {
    for mut text in &mut q {
        if settings.tutorial_enabled {
            text.0 = "Tutorial: ON [X]".to_string();
        } else {
            text.0 = "Tutorial: OFF [ ]".to_string();
        }
    }
}

/// Refreshes all static HUD labels that are language-tagged whenever `WindowSettings` changes
/// (i.e. when the user cycles the language in Options).
fn update_static_hud_labels(
    settings: Res<WindowSettings>,
    mut moves_labels: Query<
        &mut Text,
        (
            With<MovesUnitLabel>,
            Without<LivesUnitLabel>,
            Without<ShopHeaderLabel>,
            Without<ShopCoresLabel>,
            Without<ShopModifiersLabel>,
            Without<TutorialCloseBtnLabel>,
        ),
    >,
    mut lives_labels: Query<
        &mut Text,
        (
            With<LivesUnitLabel>,
            Without<MovesUnitLabel>,
            Without<ShopHeaderLabel>,
            Without<ShopCoresLabel>,
            Without<ShopModifiersLabel>,
            Without<TutorialCloseBtnLabel>,
        ),
    >,
    mut shop_header: Query<
        &mut Text,
        (
            With<ShopHeaderLabel>,
            Without<MovesUnitLabel>,
            Without<LivesUnitLabel>,
            Without<ShopCoresLabel>,
            Without<ShopModifiersLabel>,
            Without<TutorialCloseBtnLabel>,
        ),
    >,
    mut shop_cores: Query<
        &mut Text,
        (
            With<ShopCoresLabel>,
            Without<MovesUnitLabel>,
            Without<LivesUnitLabel>,
            Without<ShopHeaderLabel>,
            Without<ShopModifiersLabel>,
            Without<TutorialCloseBtnLabel>,
        ),
    >,
    mut shop_modifiers: Query<
        &mut Text,
        (
            With<ShopModifiersLabel>,
            Without<MovesUnitLabel>,
            Without<LivesUnitLabel>,
            Without<ShopHeaderLabel>,
            Without<ShopCoresLabel>,
            Without<TutorialCloseBtnLabel>,
        ),
    >,
    mut tutorial_close: Query<
        &mut Text,
        (
            With<TutorialCloseBtnLabel>,
            Without<MovesUnitLabel>,
            Without<LivesUnitLabel>,
            Without<ShopHeaderLabel>,
            Without<ShopCoresLabel>,
            Without<ShopModifiersLabel>,
        ),
    >,
) {
    let lang = settings.language;
    for mut t in &mut moves_labels {
        t.0 = lang.tr(TrKey::Moves).to_string();
    }
    for mut t in &mut lives_labels {
        t.0 = lang.tr(TrKey::Lives).to_string();
    }
    for mut t in &mut shop_header {
        t.0 = lang.tr(TrKey::Shop).to_string();
    }
    for mut t in &mut shop_cores {
        t.0 = lang.tr(TrKey::Cores).to_string();
    }
    for mut t in &mut shop_modifiers {
        t.0 = lang.tr(TrKey::ShopModifiers).to_string();
    }
    for mut t in &mut tutorial_close {
        t.0 = lang.tr(TrKey::TutorialClose).to_string();
    }
}

#[derive(Component)]
pub(crate) struct BoonTooltipContainer;

#[derive(Component)]
pub(crate) struct BoonTooltipText;

#[derive(Component, Clone)]
pub(crate) struct TooltipTrigger {
    pub(crate) title: String,
    pub(crate) description: String,
}

pub(crate) fn get_item_tooltip(item: ShopItem, lang: Language) -> TooltipTrigger {
    match item {
        ShopItem::Swap => TooltipTrigger {
            title: lang.tr(TrKey::TooltipSwapTitle).to_string(),
            description: lang.tr(TrKey::TooltipSwapDesc).to_string(),
        },
        ShopItem::Eliminate => TooltipTrigger {
            title: lang.tr(TrKey::TooltipEliminateTitle).to_string(),
            description: lang.tr(TrKey::TooltipEliminateDesc).to_string(),
        },
        ShopItem::Upgrade => TooltipTrigger {
            title: lang.tr(TrKey::TooltipUpgradeTitle).to_string(),
            description: lang.tr(TrKey::TooltipUpgradeDesc).to_string(),
        },
        ShopItem::Life => TooltipTrigger {
            title: lang.tr(TrKey::TooltipLifeTitle).to_string(),
            description: lang.tr(TrKey::TooltipLifeDesc).to_string(),
        },
        ShopItem::Boon(boon) => match boon {
            BoonKind::RedValue => TooltipTrigger {
                title: lang.tr(TrKey::TooltipBoonRedTitle).to_string(),
                description: lang.tr(TrKey::TooltipBoonRedDesc).to_string(),
            },
            BoonKind::GreenReserve => TooltipTrigger {
                title: lang.tr(TrKey::TooltipBoonGreenTitle).to_string(),
                description: lang.tr(TrKey::TooltipBoonGreenDesc).to_string(),
            },
            BoonKind::BlueMoves => TooltipTrigger {
                title: lang.tr(TrKey::TooltipBoonBlueTitle).to_string(),
                description: lang.tr(TrKey::TooltipBoonBlueDesc).to_string(),
            },
            BoonKind::SparkBounty => TooltipTrigger {
                title: lang.tr(TrKey::TooltipBoonSparkTitle).to_string(),
                description: lang.tr(TrKey::TooltipBoonSparkDesc).to_string(),
            },
            BoonKind::PowerBounty => TooltipTrigger {
                title: lang.tr(TrKey::TooltipBoonPowerTitle).to_string(),
                description: lang.tr(TrKey::TooltipBoonPowerDesc).to_string(),
            },
            BoonKind::HollowWard => TooltipTrigger {
                title: lang.tr(TrKey::TooltipBoonHollowTitle).to_string(),
                description: lang.tr(TrKey::TooltipBoonHollowDesc).to_string(),
            },
        },
    }
}

fn update_tooltip_system(
    triggers: Query<(&Interaction, &TooltipTrigger)>,
    mut q_container: Query<(&mut Visibility, &mut BorderColor), With<BoonTooltipContainer>>,
    mut q_text: Single<&mut Text, With<BoonTooltipText>>,
) {
    let mut hovered_trigger = None;
    for (interaction, trigger) in &triggers {
        if *interaction == Interaction::Hovered {
            hovered_trigger = Some(trigger);
            break;
        }
    }

    if let Ok((mut vis, mut border)) = q_container.single_mut() {
        if let Some(trigger) = hovered_trigger {
            *vis = Visibility::Visible;
            q_text.0 = format!("{}\n{}", trigger.title, trigger.description);
            *border = BorderColor::all(Color::srgba(0.85, 0.65, 0.18, 0.75));
        } else {
            *vis = Visibility::Hidden;
        }
    }
}
