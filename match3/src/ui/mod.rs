use bevy::prelude::*;
use bevy::sprite::Anchor;

use crate::core::prelude::*;
use crate::embedded;
use crate::gameplay::shop::{
    BTN_BORDER_ARMED, BTN_BORDER_BROKE, BTN_BORDER_IDLE, BTN_IDLE, Shop, ShopBar, ShopButton,
    ShopCard, ShopItem,
};
use crate::gameplay::{
    CoreReserve, DisplayedCollectedCores, DisplayedScore, GameMode, LevelTimer, MovesLeft,
    ScoreAnchor, ScoreGlow, ShadowCount, SparksCollected, StatsBook, StatsPopupOpen,
};
use crate::state::GameState;
use crate::visuals::render_target::{FinalCamera, WorldCamera, window_point_to_world};

const SCORE_NEON_BASE: f32 = 1.7;
const SCORE_NEON_PULSE: f32 = 2.6;
const SCORE_PULSE_FREQ: f32 = 22.0;
const SCORE_PULSE_DECAY: f32 = 2.2;

pub(crate) struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (setup_ui, setup_watermark))
            // The HUD is only meaningful during a match — hide it on every menu screen (the app
            // boots straight into `MainMenu`, so this also covers first launch) and bring it back
            // the moment a mode starts loading, rather than tying it to one specific state's
            // `OnExit` (which would also fire on LevelMenu → MainMenu's "Volver").
            .add_systems(OnEnter(GameState::MainMenu), hide_hud)
            .add_systems(OnEnter(GameState::LevelMenu), hide_hud)
            .add_systems(OnEnter(GameState::Options), hide_hud)
            .add_systems(OnEnter(GameState::Loading), show_hud)
            // The match stays alive while paused — keep the HUD up; this also restores it when
            // returning from Options (which hid it) back to the pause overlay.
            .add_systems(OnEnter(GameState::Paused), show_hud)
            .add_systems(
                Update,
                (
                    update_score_text.run_if(resource_changed::<DisplayedScore>),
                    update_score_glow,
                    position_score,
                    update_moves_text.run_if(resource_changed::<MovesLeft>),
                    update_goal_text.run_if(
                        resource_changed::<DisplayedScore>
                            .or_else(resource_changed::<SparksCollected>)
                            .or_else(resource_changed::<ShadowCount>)
                            .or_else(resource_changed::<LevelTimer>)
                            .or_else(resource_changed::<DisplayedCollectedCores>),
                    ),
                    pause_button_system,
                    shop_toggle_system,
                    update_shop_toggle_button,
                    update_shop_reserve_text.run_if(resource_changed::<CoreReserve>),
                    update_shop_bar_visibility,
                    update_shop_button_texts,
                    update_shop_active_badge,
                    stats_button_system,
                    update_stats_popup,
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
pub(crate) struct ShopButtonStatusText(pub(crate) ShopItem);

fn setup_ui(mut commands: Commands) {
    commands.spawn((
        ScoreText,
        Text2d::new("0"),
        TextFont {
            font_size: FontSize::Px(34.0),
            ..default()
        },
        TextColor(Color::srgb(0.65, 0.85, 1.0)),
        Anchor::CENTER, // Centrado de score
        Transform::default(),
        Visibility::Hidden,
    ));

    // Botón invisible que intercepta clicks sobre el score para abrir los detalles
    commands.spawn((
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

    // Menú desplegable/popup de estadísticas (inicialmente oculto)
    commands
        .spawn((
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

    // Botón de Pausa minimalista y circular en la esquina superior izquierda
    commands
        .spawn((
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

    // Indicador de movimientos restante en la esquina superior derecha (Mobile Friendly)
    commands
        .spawn((
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
                Text::new("moves"),
                TextFont {
                    font_size: FontSize::Px(9.0),
                    ..default()
                },
                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.5)),
            ));
        });

    // Meta como capsule/badge en la esquina inferior izquierda
    commands.spawn((
        GoalText,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(15.0),
            ..default()
        },
        TextColor(Color::srgb(0.8, 1.0, 0.8)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(16.0),
            left: Val::Px(12.0),
            padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
            border: UiRect::all(Val::Px(1.5)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BorderColor::all(Color::srgba(0.8, 1.0, 0.8, 0.2)),
        BackgroundColor(Color::srgba(0.08, 0.15, 0.08, 0.7)),
        Visibility::Hidden,
    ));

    // Botón de tienda minimalista en la esquina inferior derecha
    commands
        .spawn((
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
                Text::new("cores"),
                TextFont {
                    font_size: FontSize::Px(10.0),
                    ..default()
                },
                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.58)),
            ));
        });

    spawn_shop_bar(&mut commands);
    spawn_shop_active_badge(&mut commands);
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
        });
}

/// The in-game booster bar: three buttons centered along the bottom edge. `gameplay::shop` handles
/// arming/targeting; `update_shop_buttons` recolors them (affordable / armed / too dear) each frame.
fn spawn_shop_bar(commands: &mut Commands) {
    commands
        .spawn((
            ShopBar,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(102.0),
                right: Val::Px(12.0),
                width: Val::Px(360.0),
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
                Text::new("MODIFICADORES DE TIENDA"),
                TextFont {
                    font_size: FontSize::Px(12.0),
                    ..default()
                },
                TextColor(Color::srgb(0.72, 0.88, 1.0)),
            ));
            bar.spawn((Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                ..default()
            },))
                .with_children(|list| {
                    for item in ShopItem::ALL {
                        list.spawn((
                            Button,
                            ShopCard,
                            ShopButton(item),
                            Node {
                                width: Val::Percent(100.0),
                                min_height: Val::Px(62.0),
                                justify_content: JustifyContent::SpaceBetween,
                                align_items: AlignItems::Center,
                                padding: UiRect::axes(Val::Px(12.0), Val::Px(10.0)),
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
                                align_items: AlignItems::FlexStart,
                                ..default()
                            },))
                                .with_children(|row| {
                                    row.spawn((
                                        Text::new(item.label()),
                                        TextFont {
                                            font_size: FontSize::Px(17.0),
                                            ..default()
                                        },
                                        TextColor(Color::WHITE),
                                    ));
                                    row.spawn((
                                        Text::new(format!("{}c", item.cost())),
                                        TextFont {
                                            font_size: FontSize::Px(15.0),
                                            ..default()
                                        },
                                        TextColor(Color::srgb(1.0, 0.86, 0.48)),
                                    ));
                                });
                            b.spawn((
                                ShopButtonStatusText(item),
                                Text::new(item.status_label()),
                                TextFont {
                                    font_size: FontSize::Px(12.0),
                                    ..default()
                                },
                                TextColor(Color::srgb(0.64, 0.81, 0.98)),
                            ));
                        });
                    }
                });
        });
}

fn spawn_shop_active_badge(commands: &mut Commands) {
    commands
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
        });
}

/// The in-match HUD entities (spawned once at `Startup`) are hidden while the main menu is up and
/// shown again when a match begins, so "Nivel 1 / Moves / Meta" don't sit behind the menu.
type HudFilter = Or<(
    With<ScoreText>,
    With<MovesText>,
    With<GoalText>,
    // The booster bar root — its children inherit visibility, so hiding the root hides the bar.
    With<ShopBar>,
    With<PauseButton>,
    With<ShopToggleButton>,
    With<ShopActiveBadge>,
    With<StatsButton>,
    With<StatsPopupContainer>,
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
    let is_unbounded = mode.is_sandbox() || matches!(level.goal, LevelGoal::TimedScore { .. });

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
    mut text: Single<&mut Text, With<GoalText>>,
) {
    if mode.is_sandbox() {
        // Sandbox: no goal yet, just show the running capture count.
        **text = Text::new(format!("{}", score.0));
        return;
    }
    **text = Text::new(match &level.goal {
        LevelGoal::Score(target) => format!("Score: {} / {}", score.0, target),
        LevelGoal::Sparks => format!("Chispas: {} / {}", collected.0, level.sparks_total),
        LevelGoal::ClearShadow => format!("Shadows: {}", shadow_count.0),
        LevelGoal::TimedScore { target, .. } => {
            let remaining = level_timer
                .0
                .as_ref()
                .map(Timer::remaining_secs)
                .unwrap_or(0.0)
                .max(0.0);
            let (mins, secs) = (remaining as u32 / 60, remaining as u32 % 60);
            format!("{mins:02}:{secs:02}  |  {} / {}", score.0, target)
        }
        LevelGoal::CollectColor { color, target } => {
            let col_name = match color {
                LightColor::Red => "Rojo",
                LightColor::Green => "Verde",
                LightColor::Blue => "Azul",
                LightColor::Yellow => "Amarillo",
                LightColor::Purple => "Morado",
            };
            let current = displayed_cores.0[color.index()];
            format!("{}: {} / {}", col_name, current, target)
        }
    });
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
    mut q_visible: Single<&mut Visibility, With<StatsPopupContainer>>,
    mut q_node: Single<&mut Node, With<StatsPopupContainer>>,
    mut q_text: Single<&mut Text, With<StatsPopupText>>,
) {
    if open.0 {
        **q_visible = Visibility::Visible;
        q_node.display = Display::Flex;
        q_text.0 = format!(
            "--- DETALLES ---\nRojo: {}\nVerde: {}\nAzul: {}\nAmarillo: {}\nMorado: {}\nEspeciales: {}\nMax Combo: {}x\nCadenas: {}",
            stats.reds,
            stats.greens,
            stats.blues,
            stats.yellows,
            stats.purples,
            stats.lightkinds,
            stats.max_cascade,
            stats.total_chains
        );
    } else {
        **q_visible = Visibility::Hidden;
        q_node.display = Display::None;
    }
}

fn pause_button_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<PauseButton>)>,
    mut next: ResMut<NextState<GameState>>,
) {
    for interaction in &interactions {
        if *interaction == Interaction::Pressed {
            next.set(GameState::Paused);
        }
    }
}

fn shop_toggle_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<ShopToggleButton>)>,
    mut shop: ResMut<Shop>,
) {
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
    shop: Res<Shop>,
    mut texts: Query<(&ShopButtonStatusText, &mut Text, &mut TextColor)>,
) {
    for (status, mut text, mut color) in &mut texts {
        let item = status.0;
        if shop.armed_item() == Some(item) {
            text.0 = if item == ShopItem::Swap && shop.has_first_pick() {
                "Activo: elige destino".to_string()
            } else {
                "Activo: listo".to_string()
            };
            color.0 = Color::srgb(1.0, 0.95, 0.78);
        } else if reserve.0 >= item.cost() {
            text.0 = item.status_label().to_string();
            color.0 = Color::srgb(0.64, 0.81, 0.98);
        } else {
            text.0 = "Sin cores suficientes".to_string();
            color.0 = BTN_BORDER_BROKE.with_alpha(0.92);
        }
    }
}

fn update_shop_active_badge(
    shop: Res<Shop>,
    badge: Single<(&mut Visibility, &mut Node), With<ShopActiveBadge>>,
    mut text: Single<&mut Text, With<ShopActiveBadgeText>>,
) {
    let (mut visibility, mut node) = badge.into_inner();
    if let Some(label) = shop.active_badge_text() {
        *visibility = Visibility::Visible;
        node.display = Display::Flex;
        text.0 = label;
    } else {
        *visibility = Visibility::Hidden;
        node.display = Display::None;
        text.0.clear();
    }
}
