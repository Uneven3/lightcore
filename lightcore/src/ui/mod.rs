use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::core::locale::{Language, TrKey};
use crate::core::prelude::*;
use crate::core::run::{BoonKind, RunState};
use crate::embedded;
use crate::gameplay::shop::{
    BTN_BORDER_ARMED, BTN_BORDER_BROKE, BTN_BORDER_IDLE, BTN_IDLE, Shop, ShopBar, ShopButton,
    ShopCard, ShopItem, SpecialMoveButton, SpecialMoveInventory,
};
use crate::gameplay::{
    CoreReserve, DisplayedCollectedCores, DisplayedScore, GameMode, LevelTimer, MovesLeft,
    ScoreAnchor, ScoreGlow, ShadowCount, SparksCollected, StatsBook, StatsPopupOpen,
};
use crate::menu::options::{WindowSettings, DeviceMode};
use crate::state::{MatchPhase, Overlay, Screen};
use crate::visuals::assets::VisualCache;
use crate::visuals::render_target::{
    FinalCamera, InternalRenderTarget, WorldCamera, final_viewport_logical_rect,
    window_point_to_world,
};

const SCORE_NEON_BASE: f32 = 1.7;
const SCORE_NEON_PULSE: f32 = 2.6;
const SCORE_PULSE_FREQ: f32 = 22.0;
const SCORE_PULSE_DECAY: f32 = 2.2;
/// Jelly squash/stretch: shares `ScoreGlow::pulse` (the same envelope driving the neon color kick)
/// as its amplitude, oscillating on a faster/springier frequency so an arriving shard reads as a
/// squishy little bounce, not just a size pulse. Volume-preserving (x and y move opposite).
const SCORE_JELLY_FREQ: f32 = 16.0;
const SCORE_JELLY_AMOUNT: f32 = 0.16;

pub(crate) struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GoalHintTouchTimer>()
            .init_resource::<TutorialState>()
            .init_resource::<LevelTutorialShown>()
            .init_resource::<PendingBoonSale>()
            .add_systems(Startup, (setup_ui, setup_watermark))
            // The HUD is only meaningful during a match — hide it on every menu screen (the app
            // boots straight into `MainMenu`, so this also covers first launch) and bring it back
            // the moment a mode starts loading, rather than tying it to one specific state's
            // `OnExit` (which would also fire on LevelMenu → MainMenu's "Volver").
            .add_systems(OnEnter(Screen::MainMenu), hide_hud)
            .add_systems(OnEnter(Screen::LevelMenu), hide_hud)
            .add_systems(OnEnter(Overlay::Options), hide_hud)
            .add_systems(OnEnter(Overlay::AdvancedOptions), hide_hud)
            .add_systems(
                OnEnter(MatchPhase::Loading),
                (show_hud, reset_level_tutorial_shown),
            )
            // The match stays alive while paused — keep the HUD up; this also restores it when
            // returning from Options (which hid it) back to the pause overlay.
            .add_systems(OnEnter(Overlay::Paused), show_hud)
            .add_systems(OnEnter(MatchPhase::Playing), check_show_tutorial_on_start)
            .add_systems(OnExit(MatchPhase::Playing), reset_tutorial_state)
            .add_systems(Update, update_watermark_fps)
            .add_systems(
                Update,
                (
                    pause_button_system,
                    shop_toggle_system,
                    stats_button_system,
                    sell_boon_button_system,
                )
                    .run_if(in_state(MatchPhase::Playing).and_then(in_state(Overlay::None))),
            )
            .add_systems(
                Update,
                (
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
                        update_shop_toggle_button.run_if(resource_changed::<Shop>),
                        update_shop_reserve_text.run_if(resource_changed::<CoreReserve>),
                        update_shop_bar_visibility.run_if(resource_changed::<Shop>),
                        update_shop_button_texts.run_if(
                            resource_changed::<CoreReserve>
                                .or_else(resource_changed::<RunState>)
                                .or_else(resource_changed::<Shop>)
                                .or_else(resource_changed::<WindowSettings>),
                        ),
                        update_special_move_counts.run_if(
                            resource_changed::<SpecialMoveInventory>.or_else(resource_changed::<Shop>),
                        ),
                        update_shop_active_badge.run_if(
                            resource_changed::<Shop>.or_else(resource_changed::<WindowSettings>),
                        ),
                    ),
                    (
                        update_slow_mo_badge,
                        update_static_hud_labels.run_if(resource_changed::<WindowSettings>),
                        update_goal_hint,
                        update_stats_popup.run_if(
                            resource_changed::<StatsPopupOpen>
                                .or_else(resource_changed::<StatsBook>)
                                .or_else(resource_changed::<WindowSettings>),
                        ),
                        update_boon_indicators.run_if(
                            resource_changed::<RunState>
                                .or_else(resource_changed::<WindowSettings>)
                                .or_else(resource_changed::<PendingBoonSale>),
                        ),
                        tutorial_close_button_system,
                        tutorial_overlay_toggle_system,
                        update_tutorial_overlay_toggle_text.run_if(resource_changed::<WindowSettings>),
                        update_tutorial_visibility.run_if(resource_changed::<TutorialState>),
                        update_lives_text
                            .run_if(resource_changed::<RunState>.or_else(resource_changed::<GameMode>)),
                        update_tooltip_system,
                        toggle_hud_descriptions_on_hover,
                    ),
                )
                    .run_if(in_gameplay_state),
            );
    }
}

fn in_gameplay_state(screen: Res<State<Screen>>, overlay: Res<State<Overlay>>) -> bool {
    // The HUD is live during the whole match, pause overlay included — but not under the
    // fullscreen Options overlays, which hide it.
    *screen.get() == Screen::Match
        && !matches!(overlay.get(), Overlay::Options | Overlay::AdvancedOptions)
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
/// Visual root for the compact economy/status block (moves, lives, core reserve and specials).
#[derive(Component)]
struct PlayerStatusPanel;
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
struct SpecialMoveCountText(ShopItem);
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
    last_text: Option<String>,
    elapsed_since_refresh: f32,
}

#[derive(Default)]
struct SlowMoBadgeState {
    active: bool,
    last_tenths: i32,
}

#[derive(Resource, Clone)]
pub(crate) struct HudIcons {
    pub(crate) heart: Handle<Image>,
    pub(crate) moves: Handle<Image>,
    pub(crate) swap: Handle<Image>,
    pub(crate) eliminate: Handle<Image>,
    pub(crate) upgrade: Handle<Image>,
}

fn make_procedural_icon<F>(width: u32, height: u32, draw_fn: F) -> Image
where
    F: Fn(f32, f32) -> Color,
{
    let mut data = vec![0; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let nx = (x as f32 / (width - 1) as f32) * 2.0 - 1.0;
            let ny = (y as f32 / (height - 1) as f32) * 2.0 - 1.0;
            let color = draw_fn(nx, ny);
            let srgba = color.to_srgba();
            let idx = ((y * width + x) * 4) as usize;
            data[idx] = (srgba.red * 255.0) as u8;
            data[idx + 1] = (srgba.green * 255.0) as u8;
            data[idx + 2] = (srgba.blue * 255.0) as u8;
            data[idx + 3] = (srgba.alpha * 255.0) as u8;
        }
    }
    Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

fn setup_ui(
    mut commands: Commands,
    cache: Res<VisualCache>,
    settings: Res<WindowSettings>,
    mut images: ResMut<Assets<Image>>,
) {
    let heart = images.add(make_procedural_icon(32, 32, |x, y| {
        let cx = x * 1.3;
        let cy = -y * 1.3 + 0.1;
        let lhs = cx * cx + cy * cy - 1.0;
        let val = lhs * lhs * lhs - cx * cx * cy * cy * cy;
        if val <= 0.0 {
            Color::srgb(1.0, 0.35, 0.35)
        } else {
            Color::NONE
        }
    }));
    let moves = images.add(make_procedural_icon(32, 32, |x, y| {
        let px = x;
        let py = y;
        let line_h = py.abs() < 0.08 && px.abs() < 0.7;
        let line_v = px.abs() < 0.08 && py.abs() < 0.7;
        let head_r = px >= 0.4 && px <= 0.7 && py.abs() <= (0.7 - px) * 0.8;
        let head_l = px >= -0.7 && px <= -0.4 && py.abs() <= (px - (-0.7)) * 0.8;
        let head_t = py >= -0.7 && py <= -0.4 && px.abs() <= (py - (-0.7)) * 0.8;
        let head_b = py >= 0.4 && py <= 0.7 && px.abs() <= (0.7 - py) * 0.8;
        if line_h || line_v || head_r || head_l || head_t || head_b {
            Color::srgb(1.0, 0.85, 0.4)
        } else {
            Color::NONE
        }
    }));
    let swap = images.add(make_procedural_icon(32, 32, |x, y| {
        let px = x;
        let py = y;
        let on_top_line = (py - (-0.35)).abs() < 0.08 && px >= -0.6 && px <= 0.5;
        let on_top_head = px >= 0.3 && px <= 0.6 && (py - (-0.35)).abs() <= (0.6 - px) * 0.8;
        let on_bot_line = (py - 0.35).abs() < 0.08 && px >= -0.5 && px <= 0.6;
        let on_bot_head = px >= -0.6 && px <= -0.3 && (py - 0.35).abs() <= (px - (-0.6)) * 0.8;
        if on_top_line || on_top_head || on_bot_line || on_bot_head {
            Color::srgb(0.5, 0.8, 1.0)
        } else {
            Color::NONE
        }
    }));
    let eliminate = images.add(make_procedural_icon(32, 32, |x, y| {
        let px = x;
        let py = y;
        let r = (px * px + py * py).sqrt();
        let on_circle = (r - 0.6).abs() < 0.08;
        let on_dot = r < 0.15;
        let on_horiz = py.abs() < 0.06 && px.abs() >= 0.25 && px.abs() <= 0.75;
        let on_vert = px.abs() < 0.06 && py.abs() >= 0.25 && py.abs() <= 0.75;
        if on_circle || on_dot || on_horiz || on_vert {
            Color::srgb(1.0, 0.4, 0.4)
        } else {
            Color::NONE
        }
    }));
    let upgrade = images.add(make_procedural_icon(32, 32, |x, y| {
        let px = x;
        let py = y;
        let on_upper = (py - (-0.3 + px.abs() * 0.7)).abs() < 0.08 && px.abs() < 0.6;
        let on_lower = (py - (0.1 + px.abs() * 0.7)).abs() < 0.08 && px.abs() < 0.6;
        if on_upper || on_lower {
            Color::srgb(0.4, 1.0, 0.4)
        } else {
            Color::NONE
        }
    }));
    let icons = HudIcons {
        heart: heart.clone(),
        moves: moves.clone(),
        swap: swap.clone(),
        eliminate: eliminate.clone(),
        upgrade: upgrade.clone(),
    };
    commands.insert_resource(icons.clone());

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
                // PlayerStatusPanel (the vertical container)
                hud.spawn((
                    PlayerStatusPanel,
                    Interaction::default(),
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(12.0),
                        right: Val::Px(12.0),
                        width: Val::Px(84.0),
                        height: Val::Auto,
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::FlexStart,
                        padding: UiRect::all(Val::Px(8.0)),
                        row_gap: Val::Px(10.0),
                        ..default()
                    },
                    BorderColor::all(Color::NONE),
                    BackgroundColor(Color::srgba(0.035, 0.06, 0.11, 0.5)),
                    Visibility::Hidden,
                ))
                .with_children(|panel| {
                    // Moves Container
                    panel.spawn((
                        MovesText,
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        Visibility::Inherited,
                    ))
                    .with_children(|m| {
                        m.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(4.0),
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((
                                ImageNode {
                                    image: icons.moves.clone(),
                                    ..default()
                                },
                                Node {
                                    width: Val::Px(18.0),
                                    height: Val::Px(18.0),
                                    ..default()
                                },
                            ));
                            row.spawn((
                                MovesNumberText,
                                Text::new("30"),
                                TextFont {
                                    font_size: FontSize::Px(16.0),
                                    ..default()
                                },
                                TextColor(Color::WHITE),
                            ));
                        });
                        m.spawn((
                            MovesUnitLabel,
                            HudDescriptionLabel,
                            Text::new("moves"),
                            TextFont {
                                font_size: FontSize::Px(9.0),
                                ..default()
                            },
                            TextColor(Color::srgba(1.0, 1.0, 1.0, 0.5)),
                            Visibility::Hidden,
                        ));
                    });

                    // Lives Container
                    panel.spawn((
                        LivesText,
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        Visibility::Inherited,
                    ))
                    .with_children(|l| {
                        l.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(4.0),
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((
                                ImageNode {
                                    image: icons.heart.clone(),
                                    ..default()
                                },
                                Node {
                                    width: Val::Px(18.0),
                                    height: Val::Px(18.0),
                                    ..default()
                                },
                            ));
                            row.spawn((
                                LivesNumberText,
                                Text::new("2"),
                                TextFont {
                                    font_size: FontSize::Px(16.0),
                                    ..default()
                                },
                                TextColor(Color::srgb(1.0, 0.45, 0.45)),
                            ));
                        });
                        l.spawn((
                            LivesUnitLabel,
                            HudDescriptionLabel,
                            Text::new("vidas"),
                            TextFont {
                                font_size: FontSize::Px(9.0),
                                ..default()
                            },
                            TextColor(Color::srgba(1.0, 0.6, 0.6, 0.5)),
                            Visibility::Hidden,
                        ));
                    });

                    // Shop Toggle Button (integrated)
                    panel.spawn((
                        Button,
                        ShopToggleButton,
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            padding: UiRect::vertical(Val::Px(4.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BorderColor::all(Color::NONE),
                        BackgroundColor(Color::NONE),
                        Visibility::Inherited,
                    ))
                    .with_children(|b| {
                        b.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(4.0),
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((
                                ImageNode {
                                    image: cache.core_image.clone(),
                                    color: Color::srgb(0.70, 0.86, 1.0),
                                    ..default()
                                },
                                Node {
                                    width: Val::Px(18.0),
                                    height: Val::Px(18.0),
                                    ..default()
                                },
                            ));
                            row.spawn((
                                ShopReserveText,
                                Text::new("0"),
                                TextFont {
                                    font_size: FontSize::Px(16.0),
                                    ..default()
                                },
                                TextColor(Color::WHITE),
                            ));
                        });
                        b.spawn((
                            ShopHeaderLabel,
                            HudDescriptionLabel,
                            Text::new("SHOP"),
                            TextFont {
                                font_size: FontSize::Px(8.0),
                                ..default()
                            },
                            TextColor(Color::srgb(0.70, 0.86, 1.0)),
                            Visibility::Hidden,
                        ));
                    });

                    // Special Moves Buttons
                    for (item, label, icon) in [
                        (ShopItem::Swap, "SWAP", icons.swap.clone()),
                        (ShopItem::Eliminate, "ELIM", icons.eliminate.clone()),
                        (ShopItem::Upgrade, "UPGRD", icons.upgrade.clone()),
                    ] {
                        panel.spawn((
                            Button,
                            SpecialMoveButton(item),
                            Node {
                                width: Val::Percent(100.0),
                                flex_direction: FlexDirection::Column,
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                padding: UiRect::vertical(Val::Px(4.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BorderColor::all(Color::NONE),
                            BackgroundColor(Color::NONE),
                        ))
                        .with_children(|button| {
                            button.spawn(Node {
                                flex_direction: FlexDirection::Row,
                                align_items: AlignItems::Center,
                                column_gap: Val::Px(4.0),
                                ..default()
                            })
                            .with_children(|row| {
                                row.spawn((
                                    ImageNode {
                                        image: icon,
                                        ..default()
                                    },
                                    Node {
                                        width: Val::Px(18.0),
                                        height: Val::Px(18.0),
                                        ..default()
                                    },
                                ));
                                row.spawn((
                                    SpecialMoveCountText(item),
                                    Text::new("0"),
                                    TextFont {
                                        font_size: FontSize::Px(14.0),
                                        ..default()
                                    },
                                    TextColor(Color::srgba(0.68, 0.80, 0.94, 0.58)),
                                ));
                            });
                            button.spawn((
                                HudDescriptionLabel,
                                Text::new(label),
                                TextFont {
                                    font_size: FontSize::Px(8.0),
                                    ..default()
                                },
                                TextColor(Color::srgba(0.68, 0.80, 0.94, 0.58)),
                                Visibility::Hidden,
                            ));
                        });
                    }
                });

                hud.spawn((
                    SlowMoBadge,
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(280.0),
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
                        // This is the former shop slot: boons are always visible and directly
                        // sellable instead of being a small, disconnected top-left strip.
                        bottom: Val::Px(16.0),
                        right: Val::Px(12.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::FlexEnd,
                        row_gap: Val::Px(6.0),
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

    spawn_shop_bar(&mut commands, hud_root_id, settings.language, settings.device_mode);
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
                // Versión del juego (Cargo.toml), para poder ubicar en qué build ocurrió un bug reportado.
                Text::new(concat!("v", env!("CARGO_PKG_VERSION"))),
                TextFont {
                    font_size: FontSize::Px(12.0),
                    ..default()
                },
                TextColor(cyan),
            ));
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

fn spawn_shop_bar(commands: &mut Commands, parent: Entity, lang: Language, mode: DeviceMode) {
    let compact = mode == DeviceMode::Mobile;
    let bar_padding = if compact { 10.0 } else { 14.0 };
    let row_gap = if compact { 6.0 } else { 10.0 };
    let list_gap = if compact { 6.0 } else { 6.0 };

    // Cards layout
    let card_height = if compact { 44.0 } else { 52.0 };
    let card_padding = if compact { UiRect::axes(Val::Px(6.0), Val::Px(4.0)) } else { UiRect::axes(Val::Px(8.0), Val::Px(6.0)) };

    // Font sizes
    let title_font_sz = if compact { 11.5 } else { 14.0 };
    let cost_font_sz = if compact { 11.5 } else { 13.0 };
    let status_font_sz = if compact { 8.5 } else { 10.0 };

    let bar = commands
        .spawn((
            ShopBar,
            Node {
                position_type: PositionType::Absolute,
                // Drawer owned by the status panel: special moves are purchased directly below
                // moves/lives/cores, never from a second unrelated corner of the HUD.
                top: Val::Px(106.0),
                right: Val::Px(12.0),
                width: Val::Px(258.0),
                max_width: Val::Percent(66.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Stretch,
                row_gap: Val::Px(row_gap),
                padding: UiRect::all(Val::Px(bar_padding)),
                border: UiRect::all(Val::Px(1.5)),
                ..default()
            },
            BorderColor::all(Color::srgba(0.50, 0.74, 1.0, 0.28)),
            BackgroundColor(Color::srgba(0.05, 0.08, 0.13, 0.94)),
            Visibility::Hidden,
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
                column_gap: Val::Px(list_gap),
                row_gap: Val::Px(list_gap),
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
                                min_height: Val::Px(card_height),
                                justify_content: JustifyContent::SpaceBetween,
                                flex_direction: FlexDirection::Column,
                                align_items: AlignItems::Stretch,
                                padding: card_padding,
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
                                            font_size: FontSize::Px(title_font_sz),
                                            ..default()
                                        },
                                        TextColor(Color::WHITE),
                                    ));
                                    row.spawn((
                                        ShopButtonCostText(item),
                                        Text::new(""),
                                        TextFont {
                                            font_size: FontSize::Px(cost_font_sz),
                                            ..default()
                                        },
                                        TextColor(Color::srgb(1.0, 0.86, 0.48)),
                                    ));
                                });
                            b.spawn((
                                ShopButtonStatusText(item),
                                Text::new(item.status_label(lang)),
                                TextFont {
                                    font_size: FontSize::Px(status_font_sz),
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
                // Part of the same status cluster: this is the live indicator for the currently
                // armed special move, rather than a detached message beside the goal.
                top: Val::Px(106.0),
                right: Val::Px(12.0),
                max_width: Val::Px(246.0),
                padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
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

/// The in-match HUD entities (spawned once at `Startup`) are hidden while the main menu is up so
/// "Nivel 1 / Moves / Meta" don't sit behind the menu.
type HideHudFilter = Or<(
    With<ScoreText>,
    With<MovesText>,
    With<GoalText>,
    With<LivesText>,
    With<PlayerStatusPanel>,
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

/// Static HUD pieces that should become visible automatically when entering a match.
/// Stateful drawers/popups/badges stay out of this list; their own systems decide visibility.
type ShowHudFilter = Or<(
    With<ScoreText>,
    With<MovesText>,
    With<GoalText>,
    With<LivesText>,
    With<PlayerStatusPanel>,
    With<PauseButton>,
    With<ShopToggleButton>,
    With<StatsButton>,
    With<BoonIndicatorBar>,
    With<HudRoot>,
)>;

fn hide_hud(mut q: Query<&mut Visibility, HideHudFilter>) {
    for mut v in &mut q {
        *v = Visibility::Hidden;
    }
}

fn show_hud(mut q: Query<&mut Visibility, ShowHudFilter>) {
    for mut v in &mut q {
        *v = Visibility::Visible;
    }
}

fn update_watermark_fps(
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    settings: Res<WindowSettings>,
    internal_target: Res<InternalRenderTarget>,
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
    if state.elapsed_since_refresh < 0.25 && state.last_text.is_some() {
        return;
    }
    state.elapsed_since_refresh = 0.0;
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0)
        .round()
        .max(0.0) as u32;
    let next_text = format!(
        "FPS {fps:>4} | {}x{}",
        internal_target.size.x, internal_target.size.y
    );
    if state.last_text.as_deref() != Some(next_text.as_str()) {
        text.0 = next_text.clone();
        state.last_text = Some(next_text);
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
    mut q: Single<(&mut TextColor, &mut Transform), With<ScoreText>>,
) {
    glow.pulse = (glow.pulse - SCORE_PULSE_DECAY * time.delta_secs()).max(0.0);
    let osc = (time.elapsed_secs() * SCORE_PULSE_FREQ).sin() * 0.5 + 0.5; // 0..1, fast
    let brightness = SCORE_NEON_BASE + glow.pulse * SCORE_NEON_PULSE * osc;
    let c = glow.rgb * brightness;
    let max_val = c.x.max(c.y).max(c.z);
    let (color, transform) = &mut *q;
    if max_val > 1.0 {
        color.0 = Color::srgb(c.x / max_val, c.y / max_val, c.z / max_val);
    } else {
        color.0 = Color::srgb(c.x, c.y, c.z);
    }

    // Jelly: a squishy squash/stretch bounce every time a shard lands, sharing `pulse` as its
    // decaying amplitude — x and y move opposite so it reads as compressible jelly, not a resize.
    let jelly = glow.pulse * (time.elapsed_secs() * SCORE_JELLY_FREQ).sin() * SCORE_JELLY_AMOUNT;
    transform.scale = Vec3::new(1.0 + jelly, 1.0 - jelly, 1.0);
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
    let (vp_pos, vp_size) = final_viewport_logical_rect(final_cam, &window);
    // Score siempre centrado en el tercio superior de la pantalla
    let score_screen_pos = Vec2::new(vp_size.x * 0.5, 36.0);
    let score_window_pos = score_screen_pos + vp_pos;
    let Some(world) = window_point_to_world(cam, cam_t, vp_pos, vp_size, score_window_pos) else {
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
    state: Res<State<MatchPhase>>,
    overlay: Res<State<Overlay>>,
    settings: Res<WindowSettings>,
    mut touch_timer: ResMut<GoalHintTouchTimer>,
    interaction: Single<&Interaction, With<GoalText>>,
    hint: Single<(&mut Visibility, &mut Node), With<GoalHintContainer>>,
    mut hint_text: Single<&mut Text, With<GoalHintText>>,
) {
    let (mut visibility, mut node) = hint.into_inner();
    if *state.get() != MatchPhase::Playing || *overlay.get() != Overlay::None {
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
    mouse: Res<ButtonInput<MouseButton>>,
) {
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
    tutorial: Res<TutorialState>,
    mut next: ResMut<NextState<Overlay>>,
) {
    if tutorial.open {
        return;
    }
    for interaction in &interactions {
        if *interaction == Interaction::Pressed {
            next.set(Overlay::Paused);
        }
    }
}

fn shop_toggle_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<ShopToggleButton>)>,
    tutorial: Res<TutorialState>,
    mut shop: ResMut<Shop>,
) {
    if tutorial.open {
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
        bg.0 = Color::srgba(0.19, 0.15, 0.04, 0.46);
        *border = BorderColor::all(Color::srgba(1.0, 0.86, 0.46, 0.42));
    } else {
        bg.0 = Color::NONE;
        *border = BorderColor::all(Color::NONE);
    }
}

fn update_special_move_counts(
    inventory: Res<SpecialMoveInventory>,
    shop: Res<Shop>,
    mut texts: Query<(&SpecialMoveCountText, &mut Text, &mut TextColor)>,
    mut buttons: Query<(&SpecialMoveButton, &mut BorderColor, &mut BackgroundColor)>,
) {
    for (marker, mut text, mut color) in &mut texts {
        let count = inventory.count(marker.0);
        text.0 = format!("{count}");
        color.0 = if shop.armed_item() == Some(marker.0) {
            Color::srgb(1.0, 0.90, 0.55)
        } else if count > 0 {
            Color::srgb(0.76, 0.92, 1.0)
        } else {
            Color::srgba(0.68, 0.80, 0.94, 0.42)
        };
    }
    for (button, mut border, mut background) in &mut buttons {
        if shop.armed_item() == Some(button.0) {
            *border = BorderColor::all(BTN_BORDER_ARMED);
            background.0 = Color::srgba(0.25, 0.18, 0.04, 0.42);
        } else {
            *border = BorderColor::all(Color::NONE);
            background.0 = Color::NONE;
        }
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
    if !shop.open && let Some(label) = shop.active_badge_text(settings.language) {
        *visibility = Visibility::Visible;
        node.display = Display::Flex;
        text.0 = format!("ESPECIAL · {label}");
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

/// Active boon cards expose selling mid-level, with a deliberate two-tap confirmation and the
/// complete price of the most recently purchased rank as refund.
#[derive(Component, Clone, Copy)]
struct BoonSellButton(BoonKind);

/// First tap selects a boon for sale; the second, deliberate tap confirms it. This prevents a
/// stray touch on a persistent HUD card from immediately deleting a run upgrade.
#[derive(Resource, Default)]
struct PendingBoonSale(Option<BoonKind>);

#[derive(Component)]
pub(crate) struct HudRoot;

#[derive(Component)]
struct TutorialTitleText;

#[derive(Component)]
struct TutorialText;

fn update_boon_indicators(
    run: Res<RunState>,
    pending_sale: Res<PendingBoonSale>,
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
                let confirming_sale = pending_sale.0 == Some(boon);
                let color = match boon {
                    BoonKind::RedValue => Color::srgba(1.2, 0.4, 0.4, 0.85),
                    BoonKind::GreenReserve => Color::srgba(0.4, 1.2, 0.4, 0.85),
                    BoonKind::BlueMoves => Color::srgba(0.4, 0.6, 1.3, 0.85),
                    BoonKind::StarBounty => Color::srgba(1.3, 0.7, 0.1, 0.85),
                    BoonKind::PowerBounty => Color::srgba(1.1, 0.4, 1.2, 0.85),
                    BoonKind::HollowWard => Color::srgba(0.5, 0.5, 0.6, 0.85),
                    BoonKind::RedSpawn => Color::srgba(1.0, 0.2, 0.2, 0.85),
                    BoonKind::GreenSpawn => Color::srgba(0.2, 1.0, 0.2, 0.85),
                    BoonKind::BlueSpawn => Color::srgba(0.2, 0.4, 1.0, 0.85),
                    BoonKind::YellowSpawn => Color::srgba(1.0, 0.9, 0.2, 0.85),
                    BoonKind::PurpleSpawn => Color::srgba(0.8, 0.2, 0.9, 0.85),
                };

                parent
                    .spawn((
                        Button,
                        BoonSellButton(boon),
                        Interaction::default(),
                        get_item_tooltip(ShopItem::Boon(boon), lang),
                        Node {
                            width: Val::Px(58.0),
                            height: Val::Px(48.0),
                            flex_direction: FlexDirection::Column,
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
                            Text::new(if confirming_sale {
                                format!("VENDER {}{}?", boon.notation(), lvl)
                            } else {
                                format!("{}{}", boon.notation(), lvl)
                            }),
                            TextFont {
                                font_size: FontSize::Px(if confirming_sale { 10.0 } else { 13.0 }),
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                        b.spawn((
                            Text::new(if confirming_sale {
                                format!("CONFIRMA ↺ {}c", boon.cost(lvl - 1))
                            } else {
                                format!("↺ {}c", boon.cost(lvl - 1))
                            }),
                            TextFont {
                                font_size: FontSize::Px(9.0),
                                ..default()
                            },
                            TextColor(Color::srgba(1.0, 0.92, 0.62, 0.9)),
                        ));
                    });
            }
        }
    });
}

fn sell_boon_button_system(
    interactions: Query<(&Interaction, &BoonSellButton), Changed<Interaction>>,
    mut run: ResMut<RunState>,
    mut reserve: ResMut<CoreReserve>,
    mut pending_sale: ResMut<PendingBoonSale>,
) {
    for (interaction, button) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if pending_sale.0 == Some(button.0) {
            if let Some(refund) = run.sell(button.0) {
                reserve.0 = reserve.0.saturating_add(refund);
            }
            pending_sale.0 = None;
        } else {
            pending_sale.0 = Some(button.0);
        }
    }
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
    mut q_root: Query<&mut Node, With<LivesText>>,
    mut q_text: Query<&mut Text, With<LivesNumberText>>,
) {
    let lives_visible = mode.is_run();
    for mut node in &mut q_root {
        node.display = if lives_visible {
            Display::Flex
        } else {
            Display::None
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
            BoonKind::StarBounty => TooltipTrigger {
                title: lang.tr(TrKey::TooltipBoonStarTitle).to_string(),
                description: lang.tr(TrKey::TooltipBoonStarDesc).to_string(),
            },
            BoonKind::PowerBounty => TooltipTrigger {
                title: lang.tr(TrKey::TooltipBoonPowerTitle).to_string(),
                description: lang.tr(TrKey::TooltipBoonPowerDesc).to_string(),
            },
            BoonKind::HollowWard => TooltipTrigger {
                title: lang.tr(TrKey::TooltipBoonHollowTitle).to_string(),
                description: lang.tr(TrKey::TooltipBoonHollowDesc).to_string(),
            },
            BoonKind::RedSpawn => TooltipTrigger {
                title: lang.tr(TrKey::TooltipBoonRedSpawnTitle).to_string(),
                description: lang.tr(TrKey::TooltipBoonRedSpawnDesc).to_string(),
            },
            BoonKind::GreenSpawn => TooltipTrigger {
                title: lang.tr(TrKey::TooltipBoonGreenSpawnTitle).to_string(),
                description: lang.tr(TrKey::TooltipBoonGreenSpawnDesc).to_string(),
            },
            BoonKind::BlueSpawn => TooltipTrigger {
                title: lang.tr(TrKey::TooltipBoonBlueSpawnTitle).to_string(),
                description: lang.tr(TrKey::TooltipBoonBlueSpawnDesc).to_string(),
            },
            BoonKind::YellowSpawn => TooltipTrigger {
                title: lang.tr(TrKey::TooltipBoonYellowSpawnTitle).to_string(),
                description: lang.tr(TrKey::TooltipBoonYellowSpawnDesc).to_string(),
            },
            BoonKind::PurpleSpawn => TooltipTrigger {
                title: lang.tr(TrKey::TooltipBoonPurpleSpawnTitle).to_string(),
                description: lang.tr(TrKey::TooltipBoonPurpleSpawnDesc).to_string(),
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
        if matches!(*interaction, Interaction::Hovered | Interaction::Pressed) {
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

#[derive(Component)]
pub(crate) struct HudDescriptionLabel;

fn toggle_hud_descriptions_on_hover(
    panel: Query<&Interaction, (With<PlayerStatusPanel>, Changed<Interaction>)>,
    mut labels: Query<&mut Visibility, With<HudDescriptionLabel>>,
) {
    for interaction in &panel {
        let visible = match *interaction {
            Interaction::Hovered => Visibility::Visible,
            _ => Visibility::Hidden,
        };
        for mut visibility in &mut labels {
            *visibility = visible;
        }
    }
}
