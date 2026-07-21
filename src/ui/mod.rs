use bevy::asset::RenderAssetUsages;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite::Anchor;

use crate::core::locale::{Language, TrKey};
use crate::core::prelude::*;
use crate::core::run::{BoonKind, RunState};
use crate::embedded;
use crate::gameplay::shop::{
    BoonSellRequested, Shop, ShopItem, ShopPurchaseRequested, SpecialMoveInventory,
    SpecialMoveToggleRequested,
};
use crate::gameplay::{
    CoreReserve, GameMode, LevelTimer, MovesLeft, ScoreGlow, ShadowCount, SparksCollected,
    StatsBook, StatsPopupOpen,
};
use crate::presentation::{
    ColorGoalCollector, ColorGoalCollectorPulse, ColorGoalTarget, DisplayedCollectedCores,
    DisplayedScore, GameLayout, LightcoreCollectorTargets, ScoreCollector,
};
use crate::settings::UserSettings;
use crate::state::{MatchPhase, Overlay, Screen, TutorialModalState};
use crate::visuals::assets::VisualCache;
use crate::visuals::render_target::{InternalRenderTarget, WorldCamera, window_point_to_world};

mod match_result;

const SCORE_NEON_BASE: f32 = 1.7;
const SCORE_NEON_PULSE: f32 = 2.6;
const SCORE_PULSE_FREQ: f32 = 22.0;
const SCORE_PULSE_DECAY: f32 = 2.2;
/// Jelly squash/stretch: shares `ScoreGlow::pulse` (the same envelope driving the neon color kick)
/// as its amplitude, oscillating on a faster/springier frequency so an arriving shard reads as a
/// squishy little bounce, not just a size pulse. Volume-preserving (x and y move opposite).
const SCORE_JELLY_FREQ: f32 = 16.0;
const SCORE_JELLY_AMOUNT: f32 = 0.16;
const BTN_BORDER_BROKE: Color = Color::srgba(0.35, 0.40, 0.48, 0.18);
const BTN_BORDER_ARMED: Color = Color::srgba(1.0, 0.86, 0.46, 0.88);

pub(crate) struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(match_result::MatchResultUiPlugin)
            .init_resource::<GoalHintTouchTimer>()
            .init_resource::<LevelTutorialShown>()
            .init_resource::<PendingBoonSale>()
            .init_resource::<PeekedBoon>()
            .init_resource::<TooltipTouchState>()
            .add_systems(
                Startup,
                (setup_ui, setup_watermark, apply_game_layout).chain(),
            )
            // The HUD is only meaningful during a match — hide it on every menu screen (the app
            // boots straight into `MainMenu`, so this also covers first launch) and bring it back
            // the moment a mode starts loading, rather than tying it to one specific state's
            // `OnExit` (which would also fire on LevelMenu → MainMenu's "Volver").
            .add_systems(OnEnter(Screen::MainMenu), hide_hud)
            .add_systems(OnEnter(Screen::LevelMenu), hide_hud)
            .add_systems(OnEnter(Overlay::Options), hide_hud)
            .add_systems(
                OnEnter(MatchPhase::Loading),
                (show_hud, reset_level_tutorial_shown),
            )
            // The match stays alive while paused — keep the HUD up; this also restores it when
            // returning from Options (which hid it) back to the pause overlay.
            .add_systems(OnEnter(Overlay::Paused), show_hud)
            .add_systems(
                OnEnter(MatchPhase::Playing),
                (check_show_tutorial_on_start, update_hud_tooltips),
            )
            .add_systems(OnExit(MatchPhase::Playing), reset_tutorial_state)
            .add_systems(Update, update_watermark_fps)
            // Layout changes can happen while Options hides the HUD. Apply them immediately so
            // returning to the match never revives the previous compact/wide composition.
            .add_systems(
                Update,
                apply_game_layout.run_if(resource_changed::<GameLayout>),
            )
            // Collector geometry must remain current while Options changes the viewport. It is
            // presentation state, not gameplay input, so hiding the HUD must not freeze it.
            .add_systems(
                Update,
                update_color_goal_collector_pulse.run_if(in_state(Screen::Match)),
            )
            .add_systems(
                PostUpdate,
                update_lightcore_collectors
                    .after(bevy::ui::UiSystems::Layout)
                    .run_if(in_state(Screen::Match)),
            )
            .add_systems(
                Update,
                (
                    pause_button_system,
                    stats_button_system,
                    boon_peek_button_system,
                    sell_boon_button_system,
                    emit_shop_purchase_requests,
                    emit_special_move_toggle_requests,
                )
                    .run_if(crate::state::match_active),
            )
            .add_systems(
                Update,
                (
                    (
                        update_score_text.run_if(resource_changed::<DisplayedScore>),
                        update_score_glow,
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
                        update_shop_reserve_text.run_if(resource_changed::<CoreReserve>),
                        update_shop_button_texts.run_if(
                            resource_changed::<CoreReserve>
                                .or_else(resource_changed::<RunState>)
                                .or_else(resource_changed::<Shop>)
                                .or_else(resource_changed::<UserSettings>),
                        ),
                        update_special_move_counts.run_if(
                            resource_changed::<SpecialMoveInventory>
                                .or_else(resource_changed::<Shop>),
                        ),
                        update_shop_active_badge.run_if(
                            resource_changed::<Shop>.or_else(resource_changed::<UserSettings>),
                        ),
                    ),
                    (
                        update_slow_mo_badge,
                        update_tutorial_close_label.run_if(resource_changed::<UserSettings>),
                        update_hud_tooltips.run_if(resource_changed::<UserSettings>),
                        update_goal_hint,
                        update_stats_popup.run_if(
                            resource_changed::<StatsPopupOpen>
                                .or_else(resource_changed::<StatsBook>)
                                .or_else(resource_changed::<UserSettings>),
                        ),
                        update_boon_indicators.run_if(
                            resource_changed::<RunState>
                                .or_else(resource_changed::<UserSettings>)
                                .or_else(resource_changed::<PendingBoonSale>)
                                .or_else(resource_changed::<PeekedBoon>),
                        ),
                        tutorial_close_button_system,
                        tutorial_overlay_toggle_system,
                        // Unconditional: gating on `resource_changed::<UserSettings>` missed the
                        // very first paint (the entity is spawned in `Startup`, and by the time this
                        // runs on the first `Update` tick the resource no longer reads as freshly
                        // changed), leaving the toggle's label permanently blank. The system is a
                        // one-entity string format — negligible cost to just run every frame.
                        update_tutorial_overlay_toggle_text,
                        update_tutorial_visibility.run_if(resource_changed::<TutorialModalState>),
                        update_lives_text.run_if(
                            resource_changed::<RunState>.or_else(resource_changed::<GameMode>),
                        ),
                        update_tooltip_system,
                    ),
                )
                    .run_if(in_gameplay_state),
            );
    }
}

fn in_gameplay_state(screen: Res<State<Screen>>, overlay: Res<State<Overlay>>) -> bool {
    // The HUD is live during the whole match, pause overlay included — but not under the
    // fullscreen Options overlays, which hide it.
    *screen.get() == Screen::Match && *overlay.get() != Overlay::Options
}

#[derive(Component)]
pub(crate) struct MovesStatusCard;
#[derive(Component)]
pub(crate) struct MovesNumberText;
#[derive(Component)]
pub(crate) struct GoalStatusCard;
#[derive(Component)]
pub(crate) struct GoalIcon;
/// The goal icon's actual visual: a tinted swatch of the real in-game asset the player needs to
/// consume for the current goal (a core, an ingredient, a jelly tile...), not a text glyph — some
/// glyphs (e.g. `▧`) render as an empty tofu box on fonts missing that codepoint.
#[derive(Component)]
pub(crate) struct GoalIconImage;
#[derive(Component)]
pub(crate) struct GoalProgressText;
#[derive(Component)]
pub(crate) struct TimerStatusCard;
#[derive(Component)]
pub(crate) struct TimerNumberText;
#[derive(Component)]
pub(crate) struct GoalHintContainer;
#[derive(Component)]
pub(crate) struct GoalHintText;
#[derive(Component)]
pub(crate) struct PauseButton;

#[derive(Component)]
pub(crate) struct ShopCardsContainer;
/// UI-only target for a purchase click. It is translated to `ShopPurchaseRequested`; gameplay
/// never queries this component or Bevy's `Interaction`.
#[derive(Component, Clone, Copy)]
struct ShopButton(ShopItem);
#[derive(Component)]
struct ShopCard;
/// UI-only target for toggling an owned move.
#[derive(Component, Clone, Copy)]
struct SpecialMoveButton(ShopItem);
#[derive(Component)]
pub(crate) struct LevelStatusContainer;
#[derive(Component)]
struct HudStatusItem;
#[derive(Component)]
pub(crate) struct ReserveStatusCard;
#[derive(Component)]
pub(crate) struct BuyButtonTooltipMarker(pub(crate) ShopItem);
#[derive(Component)]
pub(crate) struct LivesTooltipMarker;
#[derive(Component)]
pub(crate) struct SpecialMoveCard(pub(crate) ShopItem);
#[derive(Component)]
pub(crate) struct LivesCard;
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

#[derive(Component)]
struct SpecialMoveCountText(ShopItem);
/// The tutorial modal's "Entendí" button label — the one static HUD label that still needs
/// re-localizing when the language changes (see [`update_tutorial_close_label`]).
#[derive(Component)]
struct TutorialCloseBtnLabel;
#[derive(Component)]
struct FpsWatermarkText;

#[derive(Resource, Default)]
struct GoalHintTouchTimer(Option<Timer>);

#[derive(Resource, Default)]
struct TooltipTouchState {
    trigger: Option<TooltipTrigger>,
    timer: Option<Timer>,
}

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
    pub(crate) boons: [Handle<Image>; 11],
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
    layout: Res<GameLayout>,
    mut images: ResMut<Assets<Image>>,
) {
    let compact = layout.class.is_compact();

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
    let boon_images = BoonKind::ALL.map(|kind| {
        images.add(make_procedural_icon(32, 32, move |x, y| {
            let base_color = match kind {
                BoonKind::RedValue => Color::srgb(1.0, 0.3, 0.3),
                BoonKind::GreenReserve => Color::srgb(0.3, 1.0, 0.3),
                BoonKind::BlueMoves => Color::srgb(0.3, 0.6, 1.0),
                BoonKind::StarBounty => Color::srgb(1.0, 0.85, 0.3),
                BoonKind::PowerBounty => Color::srgb(1.0, 0.5, 0.2),
                BoonKind::HollowWard => Color::srgb(0.8, 0.3, 1.0),
                BoonKind::RedSpawn => Color::srgb(1.0, 0.35, 0.35),
                BoonKind::GreenSpawn => Color::srgb(0.35, 1.0, 0.35),
                BoonKind::BlueSpawn => Color::srgb(0.35, 0.7, 1.0),
                BoonKind::YellowSpawn => Color::srgb(1.0, 0.9, 0.4),
                BoonKind::PurpleSpawn => Color::srgb(0.85, 0.4, 1.0),
            };

            let draw_sign = if kind == BoonKind::HollowWard { -1 } else { 1 };

            // Left side: '+' or '-' centered at cx = -0.4, cy = 0
            let cx = -0.4;
            let dx = x - cx;
            let dy = y;
            let sign_width = 0.25;
            let sign_thickness = 0.06;

            let in_sign = if draw_sign == 1 {
                // Plus sign
                (dy.abs() < sign_thickness && dx.abs() < sign_width)
                    || (dx.abs() < sign_thickness && dy.abs() < sign_width)
            } else {
                // Minus sign
                dy.abs() < sign_thickness && dx.abs() < sign_width
            };

            // Right side: '%' centered at cx = 0.35, cy = 0
            let rx = 0.35;
            let px = x - rx;
            let py = y;

            // Diagonal slash
            let in_slash = (px + py).abs() < 0.06 && px.abs() < 0.4 && py.abs() < 0.4;

            // Top-left ring
            let c1_x = px + 0.18;
            let c1_y = py - 0.18;
            let dist1 = (c1_x * c1_x + c1_y * c1_y).sqrt();
            let in_ring1 = (dist1 - 0.10).abs() < 0.035;

            // Bottom-right ring
            let c2_x = px - 0.18;
            let c2_y = py + 0.18;
            let dist2 = (c2_x * c2_x + c2_y * c2_y).sqrt();
            let in_ring2 = (dist2 - 0.10).abs() < 0.035;

            let in_percent = in_slash || in_ring1 || in_ring2;

            if in_sign || in_percent {
                base_color
            } else {
                Color::NONE
            }
        }))
    });

    let icons = HudIcons {
        heart: heart.clone(),
        moves: moves.clone(),
        swap: swap.clone(),
        eliminate: eliminate.clone(),
        upgrade: upgrade.clone(),
        boons: boon_images,
    };
    commands.insert_resource(icons.clone());

    // This is deliberately world-space: the score is the visible collector that absorbs every
    // score shard emitted by popped lightcores. `update_lightcore_collectors` keeps the entity and
    // its published `LightcoreCollectorTargets::score` at the layout's screen anchor.
    commands.spawn((
        ScoreCollector,
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
                    // `GameLayout` coordinates are viewport-local, so the shell must span that
                    // exact viewport. A centred 600px root made every absolute anchor drift on
                    // compact desktop windows wider than 600px.
                    width: Val::Percent(100.0),
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
                // SlowMoBadge
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

                // BoonIndicatorBar (now horizontal at top-right)
                hud.spawn((
                    BoonIndicatorBar,
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(12.0),
                        left: Val::Px(60.0),
                        right: Val::Px(12.0),
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        justify_content: JustifyContent::FlexEnd,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(6.0),
                        row_gap: Val::Px(4.0),
                        ..default()
                    },
                    Visibility::Hidden,
                ));

                // Independent status cards. Each owns one concept so responsive composition can
                // reorder/hide cards without making goal progress wrap through moves or reserve.
                hud.spawn((
                    LevelStatusContainer,
                    Node {
                        position_type: PositionType::Absolute,
                        bottom: Val::Px(if compact { 104.0 } else { 100.0 }),
                        left: Val::Px(0.0),
                        right: Val::Px(0.0),
                        height: Val::Auto,
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(if compact { 6.0 } else { 12.0 }),
                        ..default()
                    },
                    Visibility::Hidden,
                ))
                .with_children(|status_row| {
                    // 1. Goal progress. On color-goal levels this card is also the explicit
                    // particle collector for only the requested lightcore color.
                    status_row
                        .spawn((
                            Button,
                            GoalStatusCard,
                            ColorGoalCollector,
                            HudStatusItem,
                            UiTransform::default(),
                            Node {
                                min_width: Val::Px(0.0),
                                flex_basis: Val::Px(0.0),
                                flex_grow: 1.0,
                                height: Val::Px(32.0),
                                flex_direction: FlexDirection::Row,
                                column_gap: Val::Px(4.0),
                                padding: UiRect::horizontal(Val::Px(2.0)),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            BorderColor::all(Color::NONE),
                            BackgroundColor(Color::NONE),
                        ))
                        .with_children(|goal| {
                            goal.spawn((
                                GoalIcon,
                                Node {
                                    width: Val::Px(22.0),
                                    height: Val::Px(22.0),
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
                                        width: Val::Px(20.0),
                                        height: Val::Px(20.0),
                                        ..default()
                                    },
                                ));
                            });
                            goal.spawn((
                                GoalProgressText,
                                Text::new("0/150"),
                                TextLayout::no_wrap(),
                                TextFont {
                                    font_size: FontSize::Px(14.0),
                                    ..default()
                                },
                                TextColor(Color::srgb(0.90, 1.0, 0.90)),
                            ));
                        });

                    // 2. Moves
                    status_row
                        .spawn((
                            Button,
                            MovesStatusCard,
                            HudStatusItem,
                            TooltipTrigger {
                                title: "".to_string(),
                                description: "".to_string(),
                            },
                            Node {
                                min_width: Val::Px(0.0),
                                flex_basis: Val::Px(0.0),
                                flex_grow: 1.0,
                                height: Val::Px(32.0),
                                flex_direction: FlexDirection::Row,
                                column_gap: Val::Px(6.0),
                                padding: UiRect::horizontal(Val::Px(4.0)),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            BorderColor::all(Color::NONE),
                            BackgroundColor(Color::NONE),
                        ))
                        .with_children(|m| {
                            m.spawn((
                                ImageNode {
                                    image: icons.moves.clone(),
                                    ..default()
                                },
                                Node {
                                    width: Val::Px(22.0),
                                    height: Val::Px(22.0),
                                    ..default()
                                },
                            ));
                            m.spawn((
                                MovesNumberText,
                                Text::new("30"),
                                TextLayout::no_wrap(),
                                TextFont {
                                    font_size: FontSize::Px(14.0),
                                    ..default()
                                },
                                TextColor(Color::WHITE),
                            ));
                        });

                    // 3. Timer. Timed levels show this card while hiding moves; time and goal
                    // progress never compete inside the same text node.
                    status_row
                        .spawn((
                            TimerStatusCard,
                            HudStatusItem,
                            Node {
                                display: Display::None,
                                min_width: Val::Px(0.0),
                                flex_basis: Val::Px(0.0),
                                flex_grow: 1.0,
                                height: Val::Px(32.0),
                                padding: UiRect::horizontal(Val::Px(4.0)),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                ..default()
                            },
                        ))
                        .with_children(|timer| {
                            timer.spawn((
                                TimerNumberText,
                                Text::new("00:00"),
                                TextLayout::no_wrap(),
                                TextFont {
                                    font_size: FontSize::Px(14.0),
                                    ..default()
                                },
                                TextColor(Color::srgb(1.0, 0.86, 0.34)),
                            ));
                        });

                    // 4. Spendable reserve
                    status_row
                        .spawn((
                            Button,
                            ReserveStatusCard,
                            HudStatusItem,
                            TooltipTrigger {
                                title: "".to_string(),
                                description: "".to_string(),
                            },
                            Node {
                                min_width: Val::Px(0.0),
                                flex_basis: Val::Px(0.0),
                                flex_grow: 1.0,
                                height: Val::Px(32.0),
                                flex_direction: FlexDirection::Row,
                                column_gap: Val::Px(6.0),
                                padding: UiRect::horizontal(Val::Px(4.0)),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            BorderColor::all(Color::NONE),
                            BackgroundColor(Color::NONE),
                        ))
                        .with_children(|b| {
                            b.spawn((
                                ImageNode {
                                    image: cache.core_image.clone(),
                                    color: Color::srgb(0.70, 0.86, 1.0),
                                    ..default()
                                },
                                Node {
                                    width: Val::Px(22.0),
                                    height: Val::Px(22.0),
                                    ..default()
                                },
                            ));
                            b.spawn((
                                ShopReserveText,
                                Text::new("0"),
                                TextLayout::no_wrap(),
                                TextFont {
                                    font_size: FontSize::Px(14.0),
                                    ..default()
                                },
                                TextColor(Color::WHITE),
                            ));
                        });
                });

                // ShopCardsContainer (contains the 4 cards)
                hud.spawn((
                    ShopCardsContainer,
                    Node {
                        position_type: PositionType::Absolute,
                        bottom: Val::Px(if compact { 44.0 } else { 40.0 }),
                        left: Val::Px(0.0),
                        right: Val::Px(0.0),
                        height: Val::Auto,
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(if compact { 2.0 } else { 10.0 }),
                        ..default()
                    },
                    Visibility::Hidden,
                ))
                .with_children(|cards_row| {
                    let card_w = if compact {
                        Val::Percent(24.0)
                    } else {
                        Val::Px(112.0)
                    };
                    let card_h = if compact {
                        Val::Px(42.0)
                    } else {
                        Val::Px(46.0)
                    };

                    // 1. Lives Card (LivesText component on wrapper so it hides in sandbox)
                    cards_row
                        .spawn((
                            LivesText,
                            LivesCard,
                            Node {
                                width: card_w,
                                height: card_h,
                                flex_direction: FlexDirection::Row,
                                align_items: AlignItems::Stretch,
                                ..default()
                            },
                            BorderColor::all(Color::NONE),
                            BackgroundColor(Color::NONE),
                            Visibility::Inherited,
                        ))
                        .with_children(|card| {
                            // Left / Use area (just display for Lives)
                            card.spawn((
                                LivesTooltipMarker,
                                Node {
                                    flex_grow: 1.0,
                                    flex_direction: FlexDirection::Row,
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    column_gap: Val::Px(4.0),
                                    ..default()
                                },
                                BorderColor::all(Color::NONE),
                                Interaction::default(),
                                TooltipTrigger {
                                    title: "".to_string(),
                                    description: "".to_string(),
                                },
                            ))
                            .with_children(|left| {
                                left.spawn((
                                    ImageNode {
                                        image: icons.heart.clone(),
                                        ..default()
                                    },
                                    Node {
                                        width: Val::Px(22.0),
                                        height: Val::Px(22.0),
                                        ..default()
                                    },
                                ));
                                left.spawn((
                                    LivesNumberText,
                                    Text::new("2"),
                                    TextFont {
                                        font_size: FontSize::Px(14.0),
                                        ..default()
                                    },
                                    TextColor(Color::srgb(1.0, 0.45, 0.45)),
                                ));
                            });

                            // Right / Buy button (ShopItem::Life)
                            card.spawn((
                                Button,
                                ShopCard,
                                ShopButton(ShopItem::Life),
                                TooltipTrigger {
                                    title: "".to_string(),
                                    description: "".to_string(),
                                },
                                BuyButtonTooltipMarker(ShopItem::Life),
                                Node {
                                    width: Val::Px(if compact { 40.0 } else { 44.0 }),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                                BorderColor::all(Color::NONE),
                                BackgroundColor(Color::NONE),
                            ))
                            .with_children(|right| {
                                right.spawn((
                                    ShopButtonCostText(ShopItem::Life),
                                    Text::new(""),
                                    TextFont {
                                        font_size: FontSize::Px(12.0),
                                        ..default()
                                    },
                                    TextColor(Color::srgb(1.0, 0.86, 0.48)),
                                ));
                            });
                        });

                    // 2, 3, 4. Special Move Cards (Swap, Eliminate, Upgrade)
                    for (item, icon) in [
                        (ShopItem::Swap, icons.swap.clone()),
                        (ShopItem::Eliminate, icons.eliminate.clone()),
                        (ShopItem::Upgrade, icons.upgrade.clone()),
                    ] {
                        cards_row
                            .spawn((
                                SpecialMoveCard(item),
                                Node {
                                    width: card_w,
                                    height: card_h,
                                    flex_direction: FlexDirection::Row,
                                    align_items: AlignItems::Stretch,
                                    ..default()
                                },
                                BorderColor::all(Color::NONE),
                                BackgroundColor(Color::NONE),
                                Visibility::Inherited,
                            ))
                            .with_children(|card| {
                                // Left / Use button (SpecialMoveButton)
                                card.spawn((
                                    Button,
                                    SpecialMoveButton(item),
                                    TooltipTrigger {
                                        title: "".to_string(),
                                        description: "".to_string(),
                                    },
                                    Node {
                                        flex_grow: 1.0,
                                        flex_direction: FlexDirection::Row,
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        column_gap: Val::Px(4.0),
                                        ..default()
                                    },
                                    BorderColor::all(Color::NONE),
                                    BackgroundColor(Color::NONE),
                                ))
                                .with_children(|left| {
                                    left.spawn((
                                        ImageNode {
                                            image: icon,
                                            ..default()
                                        },
                                        Node {
                                            width: Val::Px(22.0),
                                            height: Val::Px(22.0),
                                            ..default()
                                        },
                                    ));
                                    left.spawn((
                                        SpecialMoveCountText(item),
                                        Text::new("0"),
                                        TextFont {
                                            font_size: FontSize::Px(13.0),
                                            ..default()
                                        },
                                        TextColor(Color::srgba(0.68, 0.80, 0.94, 0.58)),
                                    ));
                                });

                                // Right / Buy button (ShopButton)
                                card.spawn((
                                    Button,
                                    ShopCard,
                                    ShopButton(item),
                                    TooltipTrigger {
                                        title: "".to_string(),
                                        description: "".to_string(),
                                    },
                                    BuyButtonTooltipMarker(item),
                                    Node {
                                        width: Val::Px(if compact { 40.0 } else { 44.0 }),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                    BorderColor::all(Color::NONE),
                                    BackgroundColor(Color::NONE),
                                ))
                                .with_children(|right| {
                                    right.spawn((
                                        ShopButtonCostText(item),
                                        Text::new(""),
                                        TextFont {
                                            font_size: FontSize::Px(12.0),
                                            ..default()
                                        },
                                        TextColor(Color::srgb(1.0, 0.86, 0.48)),
                                    ));
                                });
                            });
                    }
                });

                // GoalHintContainer (Centered above the LevelStatusContainer)
                hud.spawn((
                    GoalHintContainer,
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(70.0),
                        left: Val::Px(0.0),
                        right: Val::Px(0.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        display: Display::None,
                        ..default()
                    },
                    Visibility::Hidden,
                ))
                .with_children(|hint_parent| {
                    hint_parent
                        .spawn((
                            Node {
                                max_width: Val::Px(200.0),
                                padding: UiRect::all(Val::Px(8.0)),
                                border: UiRect::all(Val::Px(1.5)),
                                ..default()
                            },
                            BorderColor::all(Color::srgba(0.70, 0.90, 1.0, 0.35)),
                            BackgroundColor(Color::srgba(0.04, 0.06, 0.09, 0.94)),
                        ))
                        .with_children(|hint| {
                            hint.spawn((
                                GoalHintText,
                                Text::new(""),
                                TextFont {
                                    font_size: FontSize::Px(13.0),
                                    ..default()
                                },
                                TextLayout::justify(Justify::Center),
                                TextColor(Color::WHITE),
                            ));
                        });
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
                    top: Val::Px(70.0),
                    left: Val::Px(0.0),
                    right: Val::Px(0.0),
                    padding: UiRect::horizontal(Val::Px(14.0)),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    display: Display::None,
                    ..default()
                },
                Visibility::Hidden,
            ))
            .with_children(|t| {
                t.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        max_width: Val::Px(400.0),
                        padding: UiRect::axes(Val::Px(14.0), Val::Px(11.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BorderColor::all(Color::srgba(0.52, 0.78, 1.0, 0.42)),
                    BackgroundColor(Color::srgba(0.025, 0.04, 0.075, 0.96)),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        BoonTooltipText,
                        Text::new(""),
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextLayout::justify(Justify::Center),
                        TextColor(Color::WHITE),
                        Node {
                            width: Val::Percent(100.0),
                            ..default()
                        },
                    ));
                });
            });
    });

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

/// Applies the responsive shell without touching gameplay state or rebuilding interactive
/// entities. Compact uses a bottom dock; wide moves the same status/shop content into a side rail.
fn apply_game_layout(
    layout: Res<GameLayout>,
    mut nodes: Query<(
        &mut Node,
        Has<HudRoot>,
        Has<StatsButton>,
        Has<StatsPopupContainer>,
        Has<BoonIndicatorBar>,
        Has<LevelStatusContainer>,
        Has<HudStatusItem>,
        Has<ShopCardsContainer>,
        Has<LivesCard>,
        Has<SpecialMoveCard>,
        Has<ShopCard>,
        Has<GoalHintContainer>,
        Has<ShopActiveBadge>,
    )>,
) {
    let compact = layout.class.is_compact();
    let size = layout.viewport.size;
    let rail = layout.side_rail;
    let dock = layout.bottom_dock;
    let rail_right = rail.map_or(12.0, |rect| size.x - rect.max.x);
    let rail_width = rail.map_or(0.0, |rect| rect.width());
    let status_top = rail.map_or(layout.top_bar.max.y + 8.0, |rect| rect.min.y + 8.0);
    let shop_top = status_top + 44.0;
    let active_badge_top = shop_top + 218.0;
    let goal_hint_top = active_badge_top + 48.0;
    let dock_height = dock.map_or(0.0, |rect| rect.height());
    let status_bottom = (dock_height - 50.0).max(0.0);
    let shop_bottom = (dock_height - 110.0).max(0.0);
    let active_badge_bottom = 8.0;

    for (
        mut node,
        is_root,
        is_stats_button,
        is_stats_popup,
        is_boon_bar,
        is_status,
        is_status_item,
        is_shop,
        is_lives_card,
        is_special_card,
        is_buy_button,
        is_goal_hint,
        is_active_badge,
    ) in &mut nodes
    {
        if is_root {
            node.width = Val::Percent(100.0);
            node.max_width = Val::Percent(100.0);
        }

        if is_stats_button {
            // The node already carries a -60px margin (half its width), so its left anchor must
            // be the score centre itself. Subtracting twice made the score's tap target drift
            // 60px left whenever the responsive layout was applied.
            node.left = Val::Px(layout.score_anchor.x);
        }

        if is_stats_popup {
            // Same centring contract as the score button: the existing -120px margin accounts for
            // half the popup width.
            node.left = Val::Px(layout.score_anchor.x);
        }

        if is_boon_bar {
            node.top = Val::Px(12.0);
            // Score is both primary information and the physical destination of capture shards.
            // Reserve a corridor around it so a wrapping boon list cannot cover the collector.
            node.left = Val::Px(layout.score_anchor.x + 72.0);
            node.right = if compact {
                Val::Px(12.0)
            } else {
                Val::Px((size.x - layout.top_bar.max.x + 12.0).max(12.0))
            };
            node.flex_direction = FlexDirection::Row;
            node.flex_wrap = FlexWrap::Wrap;
            node.justify_content = JustifyContent::FlexEnd;
        }

        if is_status {
            node.left = if compact { Val::Px(0.0) } else { Val::Auto };
            node.right = if compact {
                Val::Px(0.0)
            } else {
                Val::Px(rail_right)
            };
            node.top = if compact {
                Val::Auto
            } else {
                Val::Px(status_top)
            };
            node.bottom = if compact {
                Val::Px(status_bottom)
            } else {
                Val::Auto
            };
            node.width = if compact {
                Val::Auto
            } else {
                Val::Px(rail_width)
            };
            node.flex_direction = FlexDirection::Row;
            node.column_gap = Val::Px(if compact { 6.0 } else { 4.0 });
        }

        if is_status_item {
            node.min_width = Val::Px(0.0);
            node.flex_basis = Val::Px(0.0);
            node.flex_grow = 1.0;
            node.flex_shrink = 1.0;
        }

        if is_shop {
            node.left = if compact { Val::Px(0.0) } else { Val::Auto };
            node.right = if compact {
                Val::Px(0.0)
            } else {
                Val::Px(rail_right)
            };
            node.top = if compact {
                Val::Auto
            } else {
                Val::Px(shop_top)
            };
            node.bottom = if compact {
                Val::Px(shop_bottom)
            } else {
                Val::Auto
            };
            node.width = if compact {
                Val::Auto
            } else {
                Val::Px(rail_width)
            };
            node.flex_direction = if compact {
                FlexDirection::Row
            } else {
                FlexDirection::Column
            };
            node.column_gap = Val::Px(if compact { 2.0 } else { 0.0 });
            node.row_gap = Val::Px(if compact { 0.0 } else { 7.0 });
        }

        if is_lives_card || is_special_card {
            node.width = if compact {
                Val::Percent(24.0)
            } else {
                Val::Percent(100.0)
            };
            node.height = Val::Px(if compact { 42.0 } else { 46.0 });
        }

        if is_buy_button {
            node.width = Val::Px(if compact { 40.0 } else { 48.0 });
        }

        if is_goal_hint {
            node.left = if compact { Val::Px(0.0) } else { Val::Auto };
            node.right = if compact {
                Val::Px(0.0)
            } else {
                Val::Px(rail_right)
            };
            node.top = Val::Px(if compact {
                layout.top_bar.max.y + 6.0
            } else {
                goal_hint_top
            });
            node.width = if compact {
                Val::Auto
            } else {
                Val::Px(rail_width)
            };
        }

        if is_active_badge {
            node.left = if compact { Val::Px(0.0) } else { Val::Auto };
            node.right = if compact {
                Val::Px(0.0)
            } else {
                Val::Px(rail_right)
            };
            node.top = if compact {
                Val::Auto
            } else {
                Val::Px(active_badge_top)
            };
            node.bottom = if compact {
                Val::Px(active_badge_bottom)
            } else {
                Val::Auto
            };
            node.width = if compact {
                Val::Auto
            } else {
                Val::Px(rail_width)
            };
        }
    }
}

fn spawn_shop_active_badge(commands: &mut Commands, parent: Entity) {
    let badge = commands
        .spawn((
            ShopActiveBadge,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(146.0),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                height: Val::Auto,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                display: Display::None,
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|badge_wrapper| {
            badge_wrapper
                .spawn((
                    Node {
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                        border: UiRect::all(Val::Px(1.5)),
                        border_radius: BorderRadius::all(Val::Px(6.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BorderColor::all(Color::srgba(1.0, 0.86, 0.46, 0.88)),
                    BackgroundColor(Color::srgba(0.18, 0.14, 0.04, 0.92)),
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
                });
        })
        .id();
    commands.entity(parent).add_child(badge);
}

/// The in-match HUD entities (spawned once at `Startup`) are hidden while the main menu is up so
/// "Nivel 1 / Moves / Meta" don't sit behind the menu.
type HideHudFilter = Or<(
    With<ScoreCollector>,
    With<MovesStatusCard>,
    With<GoalStatusCard>,
    With<LivesText>,
    With<LevelStatusContainer>,
    With<ShopCardsContainer>,
    With<PauseButton>,
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
    With<ScoreCollector>,
    With<MovesStatusCard>,
    With<GoalStatusCard>,
    With<LivesText>,
    With<LevelStatusContainer>,
    With<ShopCardsContainer>,
    With<PauseButton>,
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
    settings: Res<UserSettings>,
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

fn update_score_text(
    score: Res<DisplayedScore>,
    mut text: Single<&mut Text2d, With<ScoreCollector>>,
) {
    text.0 = format!("{}", score.0);
}

fn update_score_glow(
    time: Res<Time>,
    mut glow: ResMut<ScoreGlow>,
    mut q: Single<(&mut TextColor, &mut Transform), With<ScoreCollector>>,
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

/// Publishes the exact visual positions that collected-light particles can fly into. The score is
/// always available; a color-goal card becomes a second collector only for its requested color.
fn update_lightcore_collectors(
    camera: Single<(&Camera, &GlobalTransform), With<WorldCamera>>,
    mode: Res<GameMode>,
    level: Res<LevelConfig>,
    mut targets: ResMut<LightcoreCollectorTargets>,
    layout: Res<GameLayout>,
    mut collector_transform: Single<&mut Transform, With<ScoreCollector>>,
    goal_collector: Single<
        (&UiGlobalTransform, &ComputedNode),
        (With<ColorGoalCollector>, Without<ScoreCollector>),
    >,
) {
    let (cam, cam_t) = *camera;
    let vp_pos = layout.viewport.window_origin;
    let vp_size = layout.viewport.size;
    let score_window_pos = layout.score_anchor + vp_pos;
    let Some(world) = window_point_to_world(cam, cam_t, vp_pos, vp_size, score_window_pos) else {
        return;
    };
    let position = world.extend(6.0);
    targets.score = position;
    collector_transform.translation = position;

    let goal_color = if mode.is_sandbox() {
        None
    } else {
        level.goal.requested_color()
    };
    let Some(color) = goal_color else {
        targets.color_goal = None;
        return;
    };

    // UI transforms are physical pixels relative to the UI camera viewport. Convert them back to
    // logical viewport coordinates, then add the viewport's window origin (important for the
    // centered 9:16 desktop preview) before using the shared window→world projection.
    let (goal_transform, computed) = goal_collector.into_inner();
    let goal_viewport_pos = goal_transform.translation * computed.inverse_scale_factor();
    let goal_window_pos = goal_viewport_pos + vp_pos;
    targets.color_goal =
        window_point_to_world(cam, cam_t, vp_pos, vp_size, goal_window_pos).map(|world| {
            ColorGoalTarget {
                color,
                position: world.extend(6.0),
            }
        });
}

fn update_color_goal_collector_pulse(
    time: Res<Time>,
    mut pulse: ResMut<ColorGoalCollectorPulse>,
    mut transform: Single<&mut UiTransform, With<ColorGoalCollector>>,
) {
    pulse.0 = (pulse.0 - SCORE_PULSE_DECAY * time.delta_secs()).max(0.0);
    let jelly =
        pulse.0 * (time.elapsed_secs() * SCORE_JELLY_FREQ).sin().abs() * (SCORE_JELLY_AMOUNT * 0.8);
    transform.scale = Vec2::splat(1.0 + jelly);
}

fn update_moves_text(
    mode: Res<GameMode>,
    level: Res<LevelConfig>,
    moves: Res<MovesLeft>,
    mut q_num: Query<&mut Text, With<MovesNumberText>>,
    mut q_badge: Query<&mut Node, With<MovesStatusCard>>,
) {
    let is_unbounded = mode.is_sandbox() || !level.goal.uses_moves();

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
    panel: Single<(&mut BackgroundColor, &mut BorderColor), With<GoalStatusCard>>,
    mut icon: Single<&mut ImageNode, With<GoalIconImage>>,
    mut progress: Single<
        (&mut Text, &mut TextColor),
        (With<GoalProgressText>, Without<TimerNumberText>),
    >,
    mut timer_card: Single<&mut Node, With<TimerStatusCard>>,
    mut timer_text: Single<&mut Text, (With<TimerNumberText>, Without<GoalProgressText>)>,
) {
    // The goal icon is a tinted swatch of the actual asset the player needs to consume: a round
    // "core" for anything that collects lightcores/ingredients/colors, a square for jelly tiles —
    // deliberately not a text glyph, since some (e.g. `▧`) render as an empty box on fonts missing
    // that codepoint (see `GoalIconImage`'s doc comment).
    let remaining = level_timer
        .0
        .as_ref()
        .map(Timer::remaining_secs)
        .map(|secs| secs.max(0.0));
    let status = level.goal_status(GoalFacts {
        score: score.0,
        sparks: collected.0,
        shadows: shadow_count.0,
        collected_cores: displayed_cores.0,
        remaining_secs: remaining,
    });
    let (icon_image, icon_color, progress_value, time_value) = if mode.is_sandbox() {
        (
            cache.core_image.clone(),
            Color::srgb(0.65, 0.85, 1.0),
            format!("{}", score.0),
            None,
        )
    } else {
        let progress = status.target.map_or_else(
            || status.current.to_string(),
            |target| format!("{}/{}", status.current, target),
        );
        let time_value = status.remaining_secs.map(|remaining| {
            let (mins, secs) = (remaining as u32 / 60, remaining as u32 % 60);
            format!("{mins:02}:{secs:02}")
        });
        match status.kind {
            GoalKind::Score => (
                cache.core_image.clone(),
                Color::srgb(0.65, 0.85, 1.0),
                progress,
                time_value,
            ),
            GoalKind::Sparks => (
                cache.core_image.clone(),
                Color::srgb(1.0, 0.58, 0.12),
                progress,
                time_value,
            ),
            GoalKind::ClearShadow => (
                cache.square_image.clone(),
                Color::srgba(0.22, 0.55, 1.0, 0.82),
                progress,
                time_value,
            ),
            GoalKind::TimedScore => (
                cache.core_image.clone(),
                Color::srgb(1.0, 0.86, 0.34),
                progress,
                time_value,
            ),
            GoalKind::CollectColor | GoalKind::TimedCollectColor => (
                cache.core_image.clone(),
                status.color.map_or(Color::WHITE, LightColor::bevy_color),
                progress,
                time_value,
            ),
        }
    };

    let (mut bg, mut border) = panel.into_inner();
    bg.0 = Color::NONE;
    *border = BorderColor::all(Color::NONE);
    icon.image = icon_image;
    icon.color = icon_color;

    progress.0.0 = progress_value;
    progress.1.0 = icon_color.with_alpha(0.96);

    if let Some(value) = time_value {
        timer_card.display = Display::Flex;
        timer_text.0 = value;
    } else {
        timer_card.display = Display::None;
        timer_text.0.clear();
    }
}

fn update_goal_hint(
    time: Res<Time>,
    mode: Res<GameMode>,
    level: Res<LevelConfig>,
    state: Res<State<MatchPhase>>,
    overlay: Res<State<Overlay>>,
    settings: Res<UserSettings>,
    mut touch_timer: ResMut<GoalHintTouchTimer>,
    interaction: Single<&Interaction, With<GoalStatusCard>>,
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
    let detail = if mode.is_sandbox() {
        lang.tr(TrKey::GoalFreePlay)
    } else {
        match level.goal.kind() {
            GoalKind::Score => lang.tr(TrKey::GoalReachTarget),
            GoalKind::Sparks => lang.tr(TrKey::GoalRescueSparks),
            GoalKind::ClearShadow => lang.tr(TrKey::GoalClearShadows),
            GoalKind::TimedScore => lang.tr(TrKey::GoalScoreOnClock),
            GoalKind::CollectColor => lang.tr(TrKey::GoalCollectColor),
            GoalKind::TimedCollectColor => lang.tr(TrKey::GoalColorOnClock),
        }
    };
    format!("{}\n{}", lang.tr(TrKey::GoalTitle), detail)
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
    settings: Res<UserSettings>,
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
    tutorial: Res<TutorialModalState>,
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

fn emit_shop_purchase_requests(
    interactions: Query<(&Interaction, &ShopButton), Changed<Interaction>>,
    mut commands: Commands,
) {
    for (interaction, button) in &interactions {
        if *interaction == Interaction::Pressed {
            commands.trigger(ShopPurchaseRequested(button.0));
        }
    }
}

fn emit_special_move_toggle_requests(
    interactions: Query<(&Interaction, &SpecialMoveButton), Changed<Interaction>>,
    mut commands: Commands,
) {
    for (interaction, button) in &interactions {
        if *interaction == Interaction::Pressed {
            commands.trigger(SpecialMoveToggleRequested(button.0));
        }
    }
}

fn update_special_move_counts(
    inventory: Res<SpecialMoveInventory>,
    shop: Res<Shop>,
    mut texts: Query<(&SpecialMoveCountText, &mut Text, &mut TextColor)>,
    mut buttons: Query<(&SpecialMoveButton, &mut BorderColor, &mut BackgroundColor)>,
    mut cards: Query<
        (&SpecialMoveCard, &mut BorderColor, &mut BackgroundColor),
        Without<SpecialMoveButton>,
    >,
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
    for (card, mut border, mut background) in &mut cards {
        if shop.armed_item() == Some(card.0) {
            *border = BorderColor::all(Color::NONE);
            background.0 = Color::srgba(1.0, 0.76, 0.18, 0.12);
        } else {
            *border = BorderColor::all(Color::NONE);
            background.0 = Color::NONE;
        }
    }
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
    settings: Res<UserSettings>,
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
                text.0 = shop_item_status_label(item, lang).to_string();
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
            let confirming = shop.pending_purchase_item() == Some(cost_item.0);
            text.0 = if confirming {
                "OK".to_string()
            } else {
                format!("+{}c", cost)
            };
            color.0 = if confirming {
                Color::srgb(0.55, 1.0, 0.66)
            } else if reserve.0 >= cost {
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
    settings: Res<UserSettings>,
    badge: Single<(&mut Visibility, &mut Node), With<ShopActiveBadge>>,
    mut text: Single<&mut Text, With<ShopActiveBadgeText>>,
) {
    let (mut visibility, mut node) = badge.into_inner();
    if !shop.open
        && let Some(label) = active_shop_badge_text(&shop, settings.language)
    {
        *visibility = Visibility::Visible;
        node.display = Display::Flex;
        text.0 = format!("ESPECIAL · {label}");
    } else {
        *visibility = Visibility::Hidden;
        node.display = Display::None;
        text.0.clear();
    }
}

fn shop_item_status_label(item: ShopItem, lang: Language) -> &'static str {
    match item {
        ShopItem::Swap => lang.tr(TrKey::ShopSwapStatus),
        ShopItem::Eliminate => lang.tr(TrKey::ShopEliminateStatus),
        ShopItem::Upgrade => lang.tr(TrKey::ShopUpgradeStatus),
        ShopItem::Life => lang.tr(TrKey::ShopLifeStatus),
        ShopItem::Boon(boon) => boon.status_label(lang),
    }
}

fn active_shop_badge_text(shop: &Shop, lang: Language) -> Option<String> {
    let item = shop.armed_item()?;
    Some(match item {
        ShopItem::Swap if shop.has_first_pick() => lang.tr(TrKey::ArmedSwap1of2).to_string(),
        ShopItem::Swap => lang.tr(TrKey::ArmedSwap).to_string(),
        ShopItem::Eliminate => lang.tr(TrKey::ArmedEliminate).to_string(),
        ShopItem::Upgrade => lang.tr(TrKey::ArmedUpgrade).to_string(),
        ShopItem::Life | ShopItem::Boon(_) => return None,
    })
}

#[derive(Component)]
pub(crate) struct TutorialOverlayRoot;
#[derive(Component)]
struct TutorialCloseButton;

fn check_show_tutorial_on_start(
    settings: Res<UserSettings>,
    level: Res<LevelConfig>,
    mut state: ResMut<TutorialModalState>,
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
                        "• Slide lights next to each other to line up 3 or more of the same color.\n\n\
                         • GOAL: Reach at least {} points.\n\n\
                         • After winning or picking an upgrade, tap the screen to go to the next level.\n\n\
                         • You can turn off this tutorial with the button below or in Options.",
                        target
                    )
                } else {
                    format!(
                        "• Desliza luces de al lado para juntar 3 o más del mismo color.\n\n\
                         • META: Consigue al menos {} puntos.\n\n\
                         • Al ganar o elegir una mejora, toca la pantalla para ir al siguiente nivel.\n\n\
                         • Puedes apagar este tutorial con el botón de abajo o en Opciones.",
                        target
                    )
                },
            ),
            LevelGoal::Sparks => (
                lang.tr(TrKey::TutorialSparksTitle),
                if lang == crate::core::locale::Language::English {
                    "• GOAL: Bring the sparks (hexagon pieces) down to the bottom row to collect them.\n\n\
                     • Sparks only fall straight down (they don't slide sideways like normal lights).\n\n\
                     • After winning or picking an upgrade, tap the screen to go to the next level.\n\n\
                     • You can turn off this tutorial with the button below or in Options.".to_string()
                } else {
                    "• OBJETIVO: Lleva las chispas (piezas con forma de hexágono) hasta la fila de abajo para recogerlas.\n\n\
                     • Las chispas solo caen hacia abajo (no se deslizan de lado como las luces normales).\n\n\
                     • Al ganar o elegir una mejora, toca la pantalla para ir al siguiente nivel.\n\n\
                     • Puedes apagar este tutorial con el botón de abajo o en Opciones.".to_string()
                },
            ),
            LevelGoal::ClearShadow => (
                lang.tr(TrKey::TutorialShadowTitle),
                if lang == crate::core::locale::Language::English {
                    "• GOAL: Clear all the dark tiles (shadows) from the board.\n\n\
                     • To clear a shadow, make a match of 3 or more on top of it.\n\n\
                     • After winning or picking an upgrade, tap the screen to go to the next level.\n\n\
                     • You can turn off this tutorial with the button below or in Options.".to_string()
                } else {
                    "• OBJETIVO: Limpia todas las casillas oscuras (sombras) del tablero.\n\n\
                     • Para limpiar una sombra, junta 3 o más luces encima de ella.\n\n\
                     • Al ganar o elegir una mejora, toca la pantalla para ir al siguiente nivel.\n\n\
                     • Puedes apagar este tutorial con el botón de abajo o en Opciones.".to_string()
                },
            ),
            LevelGoal::TimedScore { target, .. } => (
                lang.tr(TrKey::TutorialTimedScoreTitle),
                if lang == crate::core::locale::Language::English {
                    format!(
                        "• GOAL: Reach at least {} points before the timer hits zero.\n\n\
                         • No move limit! Match fast to get more points.\n\n\
                         • After winning or picking an upgrade, tap the screen to go to the next level.\n\n\
                         • You can turn off this tutorial with the button below or in Options.",
                        target
                    )
                } else {
                    format!(
                        "• OBJETIVO: Consigue al menos {} puntos antes de que el reloj llegue a cero.\n\n\
                         • ¡Sin límite de movimientos! Junta rápido para sumar más puntos.\n\n\
                         • Al ganar o elegir una mejora, toca la pantalla para ir al siguiente nivel.\n\n\
                         • Puedes apagar este tutorial con el botón de abajo o en Opciones.",
                        target
                    )
                },
            ),
            LevelGoal::CollectColor { color, target } => {
                let color_name = lang.tr(color.name_key());
                (
                    lang.tr(TrKey::TutorialCollectColorTitle),
                    if lang == crate::core::locale::Language::English {
                        format!(
                            "• GOAL: Collect at least {} {} lights.\n\n\
                             • Only lights of this color count, but you can match the others to clear the board.\n\n\
                             • After winning or picking an upgrade, tap the screen to go to the next level.\n\n\
                             • You can turn off this tutorial with the button below.",
                            target, color_name
                        )
                    } else {
                        format!(
                            "• OBJETIVO: Junta al menos {} luces {}.\n\n\
                             • Solo cuentan las luces de este color, pero puedes juntar las otras para despejar el tablero.\n\n\
                             • Al ganar o elegir una mejora, toca la pantalla para ir al siguiente nivel.\n\n\
                             • Puedes apagar este tutorial con el botón de abajo.",
                            target, color_name
                        )
                    },
                )
            }
            LevelGoal::TimedCollectColor { color, target, .. } => {
                let color_name = lang.tr(color.name_key());
                (
                    lang.tr(TrKey::TutorialTimedColorTitle),
                    if lang == crate::core::locale::Language::English {
                        format!(
                            "• GOAL: Collect at least {} {} lights before the timer hits zero.\n\n\
                             • No move limit! Match fast, focused on this color.\n\n\
                             • After winning or picking an upgrade, tap the screen to go to the next level.\n\n\
                             • You can turn off this tutorial with the button below.",
                            target, color_name
                        )
                    } else {
                        format!(
                            "• OBJETIVO: Junta al menos {} luces {} antes de que el reloj llegue a cero.\n\n\
                             • ¡Sin límite de movimientos! Junta rápido, enfocado en este color.\n\n\
                             • Al ganar o elegir una mejora, toca la pantalla para ir al siguiente nivel.\n\n\
                             • Puedes apagar este tutorial con el botón de abajo.",
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

fn reset_tutorial_state(mut state: ResMut<TutorialModalState>) {
    state.open = false;
}

fn update_tutorial_visibility(
    state: Res<TutorialModalState>,
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
    mut state: ResMut<TutorialModalState>,
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
/// complete price of the most recently purchased rank as refund. Lives only on the "Vender"
/// sub-button that appears once a card is peeked (see [`BoonPeekButton`]) — reading a boon must
/// never itself risk arming a sale.
#[derive(Component, Clone, Copy)]
struct BoonSellButton(BoonKind);

/// Tapping a boon card's icon toggles it "peeked" open (see [`PeekedBoon`]) to read its
/// description — no touch device has a hover state, so this is the only way to read a boon on
/// Android/iOS without also arming its sale, which is what tapping the (mouse-only) tooltip
/// trigger used to do.
#[derive(Component, Clone, Copy)]
struct BoonPeekButton(BoonKind);

/// Which active boon's card is currently expanded to show its full description + Vender button.
/// `None` collapses every card back to its compact `{notation}{level}` form.
#[derive(Resource, Default)]
struct PeekedBoon(Option<BoonKind>);

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
    peeked: Res<PeekedBoon>,
    settings: Res<UserSettings>,
    mut commands: Commands,
    bar: Single<Entity, With<BoonIndicatorBar>>,
    icons: Res<HudIcons>,
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
                let is_peeked = peeked.0 == Some(boon);
                let confirming_sale = pending_sale.0 == Some(boon);

                parent
                    .spawn((
                        Button,
                        BoonPeekButton(boon),
                        Interaction::default(),
                        Node {
                            width: Val::Px(if is_peeked { 180.0 } else { 44.0 }),
                            height: Val::Auto,
                            min_height: Val::Px(if is_peeked { 48.0 } else { 44.0 }),
                            flex_direction: FlexDirection::Column,
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            padding: UiRect::all(Val::Px(if is_peeked { 10.0 } else { 0.0 })),
                            border_radius: BorderRadius::all(Val::Px(8.0)),
                            row_gap: Val::Px(if is_peeked { 4.0 } else { 1.0 }),
                            ..default()
                        },
                        BorderColor::all(Color::NONE),
                        BackgroundColor(if is_peeked {
                            Color::srgba(0.025, 0.04, 0.075, 0.96)
                        } else {
                            Color::NONE
                        }),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            ImageNode {
                                image: icons.boons[boon.index()].clone(),
                                ..default()
                            },
                            Node {
                                width: Val::Px(28.0),
                                height: Val::Px(28.0),
                                ..default()
                            },
                        ));
                        b.spawn((
                            Text::new(if is_peeked {
                                format!("lvl {}", lvl)
                            } else {
                                format!("{}", lvl)
                            }),
                            TextFont {
                                font_size: FontSize::Px(10.0),
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                        if is_peeked {
                            let trigger = get_item_tooltip(ShopItem::Boon(boon), lang);
                            b.spawn((
                                Text::new(format!("{}\n{}", trigger.title, trigger.description)),
                                TextFont {
                                    font_size: FontSize::Px(11.0),
                                    ..default()
                                },
                                TextLayout::justify(Justify::Center),
                                TextColor(Color::WHITE),
                                Node {
                                    width: Val::Percent(100.0),
                                    ..default()
                                },
                            ));
                            b.spawn((
                                Button,
                                BoonSellButton(boon),
                                Interaction::default(),
                                Node {
                                    width: Val::Percent(100.0),
                                    min_height: Val::Px(28.0),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    margin: UiRect::top(Val::Px(2.0)),
                                    ..default()
                                },
                                BorderColor::all(Color::NONE),
                                BackgroundColor(Color::NONE),
                            ))
                            .with_children(|sell| {
                                sell.spawn((
                                    Text::new(if confirming_sale {
                                        format!("CONFIRMAR · +{}c", boon.cost(lvl - 1))
                                    } else {
                                        format!("VENDER · +{}c", boon.cost(lvl - 1))
                                    }),
                                    TextFont {
                                        font_size: FontSize::Px(11.0),
                                        ..default()
                                    },
                                    TextColor(if confirming_sale {
                                        Color::srgb(1.0, 0.48, 0.42)
                                    } else {
                                        Color::srgba(1.0, 0.92, 0.62, 0.92)
                                    }),
                                ));
                            });
                        }
                    });
            }
        }
    });
}

/// Tapping a boon's icon only expands/collapses its card to read the description — it must never
/// arm a sale by itself (that's the touch bug this replaces: on touch there's no hover state, so
/// the only way to read a card used to be the same tap that armed selling it).
fn boon_peek_button_system(
    interactions: Query<(&Interaction, &BoonPeekButton), Changed<Interaction>>,
    mut peeked: ResMut<PeekedBoon>,
) {
    for (interaction, button) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        peeked.0 = if peeked.0 == Some(button.0) {
            None
        } else {
            Some(button.0)
        };
    }
}

/// The two-tap sell confirmation is UI state and stays here; the actual economy transaction
/// (refund + boon rank drop) is owned by gameplay's `on_boon_sell_requested` observer, so this
/// system never mutates authoritative `RunState`/`CoreReserve` — symmetric with the buy path.
fn sell_boon_button_system(
    interactions: Query<(&Interaction, &BoonSellButton), Changed<Interaction>>,
    mut commands: Commands,
    mut pending_sale: ResMut<PendingBoonSale>,
    mut peeked: ResMut<PeekedBoon>,
) {
    for (interaction, button) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if pending_sale.0 == Some(button.0) {
            commands.trigger(BoonSellRequested(button.0));
            pending_sale.0 = None;
            peeked.0 = None;
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
    mut settings: ResMut<UserSettings>,
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
    settings: Res<UserSettings>,
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

/// Re-localizes the tutorial modal's close button whenever `UserSettings` changes (i.e. when the
/// user cycles the language in Options). Every other HUD label is either an icon or already
/// re-localized by its own updater; this is the sole remaining static text label.
fn update_tutorial_close_label(
    settings: Res<UserSettings>,
    mut tutorial_close: Query<&mut Text, With<TutorialCloseBtnLabel>>,
) {
    let lang = settings.language;
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
    time: Res<Time>,
    triggers: Query<(&Interaction, &TooltipTrigger)>,
    mut touch_state: ResMut<TooltipTouchState>,
    mut q_container: Query<(&mut Visibility, &mut Node), With<BoonTooltipContainer>>,
    mut q_text: Single<&mut Text, With<BoonTooltipText>>,
) {
    let mut pressed_trigger = None;
    let mut hovered_trigger = None;
    for (interaction, trigger) in &triggers {
        match *interaction {
            Interaction::Pressed => {
                pressed_trigger = Some(trigger.clone());
                break;
            }
            Interaction::Hovered if hovered_trigger.is_none() => {
                hovered_trigger = Some(trigger.clone());
            }
            _ => {}
        }
    }

    if let Some(trigger) = pressed_trigger.as_ref() {
        touch_state.trigger = Some(trigger.clone());
        touch_state.timer = Some(Timer::from_seconds(3.0, TimerMode::Once));
    }
    if let Some(timer) = touch_state.timer.as_mut() {
        timer.tick(time.delta());
        if timer.is_finished() {
            touch_state.timer = None;
            touch_state.trigger = None;
        }
    }

    let visible_trigger = pressed_trigger
        .as_ref()
        .or(hovered_trigger.as_ref())
        .or(touch_state.trigger.as_ref());

    if let Ok((mut vis, mut node)) = q_container.single_mut() {
        if let Some(trigger) = visible_trigger {
            *vis = Visibility::Visible;
            node.display = Display::Flex;
            q_text.0 = format!("{}\n\n{}", trigger.title, trigger.description);
        } else {
            *vis = Visibility::Hidden;
            node.display = Display::None;
        }
    }
}

fn update_hud_tooltips(
    settings: Res<UserSettings>,
    mut q_triggers: Query<(
        &mut TooltipTrigger,
        Option<&MovesStatusCard>,
        Option<&ReserveStatusCard>,
        Option<&LivesTooltipMarker>,
        Option<&SpecialMoveButton>,
        Option<&BuyButtonTooltipMarker>,
    )>,
) {
    let lang = settings.language;
    for (mut trigger, opt_moves, opt_cores, opt_lives, opt_special, opt_buy) in &mut q_triggers {
        if opt_moves.is_some() {
            trigger.title = lang.tr(TrKey::TooltipMovesTitle).to_string();
            trigger.description = lang.tr(TrKey::TooltipMovesDesc).to_string();
        } else if opt_cores.is_some() {
            trigger.title = lang.tr(TrKey::TooltipCoresTitle).to_string();
            trigger.description = lang.tr(TrKey::TooltipCoresDesc).to_string();
        } else if opt_lives.is_some() {
            trigger.title = lang.tr(TrKey::TooltipLifeTitle).to_string();
            trigger.description = lang.tr(TrKey::TooltipLifeDesc).to_string();
        } else if let Some(btn) = opt_special {
            let item = btn.0;
            let t = get_item_tooltip(item, lang);
            trigger.title = t.title;
            trigger.description = t.description;
        } else if let Some(btn) = opt_buy {
            let item = btn.0;
            let (title, desc) = match lang {
                Language::Spanish => match item {
                    ShopItem::Swap => (
                        "Comprar: Mover",
                        "Compra 1 movimiento por 200 de reserva.",
                    ),
                    ShopItem::Eliminate => (
                        "Comprar: Eliminar",
                        "Compra 1 Eliminar por 450 de reserva.",
                    ),
                    ShopItem::Upgrade => (
                        "Comprar: Subir nivel",
                        "Compra 1 Subir nivel por 900 de reserva.",
                    ),
                    ShopItem::Life => ("Comprar: +1 Vida", "Compra 1 vida extra por 800 de reserva."),
                    _ => ("", ""),
                },
                Language::English => match item {
                    ShopItem::Swap => ("Buy: Move", "Buy 1 Move for 200 reserve."),
                    ShopItem::Eliminate => {
                        ("Buy: Eliminate", "Buy 1 Eliminate for 450 reserve.")
                    }
                    ShopItem::Upgrade => ("Buy: Upgrade", "Buy 1 Level up for 900 reserve."),
                    ShopItem::Life => ("Buy: +1 Life", "Buy 1 extra life for 800 reserve."),
                    _ => ("", ""),
                },
            };
            trigger.title = title.to_string();
            trigger.description = desc.to_string();
        }
    }
}
